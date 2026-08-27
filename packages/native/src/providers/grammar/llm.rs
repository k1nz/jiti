//! LLM 语法适配器：NDJSON 流式协议、SSE 解码、结构失败时一次非流式重试。

use std::time::Duration;

use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use serde_json::json;

use super::sse::{fold_openai_sse, LineAssembler};
use super::{
    parse_ndjson_line, GrammarDraft, GrammarProgressEvent, GrammarResult, NdjsonError,
    NdjsonRecord, ParseFail,
};
use crate::providers::llm::completions_url;
use crate::providers::{provider_label, EngineError, ProviderConfig, PROVIDER_LLM};
use crate::services::transport::{Transport, GRAMMAR_TIMEOUT};

pub const SYSTEM_PROMPT: &str = r#"You are an English grammar checker for Chinese-speaking learners.
Check ONLY English text. Explain every issue in Simplified Chinese.
Output ONLY NDJSON: one JSON object per line. No markdown, no commentary.

Emit records in this exact order:
1. {"type":"overall","text":"<one-sentence Chinese summary>"}
2. {"type":"correctedText","text":"<full corrected English text>"}
3. zero or more error records
4. {"type":"done"}

Each error record:
{"type":"error","original":"<exact substring copied from the input>","corrected":"<replacement>","errorType":"grammar|spelling|punctuation|word_choice|style","severity":"low|medium|high","explanation":"<Chinese explanation>","suggestions":["<optional alternatives>"]}

Rules:
- original MUST be copied verbatim from the input so it can be located uniquely.
- errorType MUST be one of: grammar, spelling, punctuation, word_choice, style.
- severity MUST be one of: low, medium, high.
- If the text is already correct, still emit overall, correctedText (same as input), and done; omit error records.
- Do not wrap output in markdown fences.
"#;

pub const RETRY_SYSTEM_PROMPT: &str = r#"You are an English grammar checker for Chinese-speaking learners.
Check ONLY English text. Explain every issue in Simplified Chinese.
Output a SINGLE JSON object, no markdown, no NDJSON:

{"overall":"<one-sentence Chinese summary>","correctedText":"<full corrected English text>","errors":[{"original":"<exact substring from the input>","corrected":"<replacement>","errorType":"grammar|spelling|punctuation|word_choice|style","severity":"low|medium|high","explanation":"<Chinese explanation>","suggestions":[]}]}

Rules:
- original MUST be copied verbatim from the input.
- errorType MUST be one of: grammar, spelling, punctuation, word_choice, style.
- severity MUST be one of: low, medium, high.
- If the text is already correct, errors is an empty array.
- Do not wrap output in markdown fences.
"#;

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StrictGrammarJson {
    overall: String,
    corrected_text: String,
    #[serde(default)]
    errors: Vec<NdjsonError>,
}

#[cfg(test)]
pub fn prompt_version() -> &'static str {
    super::PROMPT_VERSION
}

pub async fn check(
    cfg: &ProviderConfig,
    api_key: &str,
    text: &str,
    transport: &dyn Transport,
    emit: &mut impl FnMut(GrammarProgressEvent),
) -> Result<GrammarResult, EngineError> {
    check_with_timeout(cfg, api_key, text, transport, emit, GRAMMAR_TIMEOUT).await
}

pub async fn check_with_timeout(
    cfg: &ProviderConfig,
    api_key: &str,
    text: &str,
    transport: &dyn Transport,
    emit: &mut impl FnMut(GrammarProgressEvent),
    timeout: Duration,
) -> Result<GrammarResult, EngineError> {
    let provider = provider_label(PROVIDER_LLM);
    emit(GrammarProgressEvent::Started {
        engine: provider.into(),
    });

    match check_streaming(cfg, api_key, text, transport, emit, timeout).await {
        Ok(result) => {
            emit(GrammarProgressEvent::Finished);
            Ok(result)
        }
        Err(EngineError::InvalidResponse { .. }) => {
            emit(GrammarProgressEvent::Retrying {
                reason: "正在重新解析".into(),
            });
            let result = check_strict(cfg, api_key, text, transport, emit, timeout).await?;
            emit(GrammarProgressEvent::Finished);
            Ok(result)
        }
        Err(err) => Err(err),
    }
}

async fn check_streaming(
    cfg: &ProviderConfig,
    api_key: &str,
    text: &str,
    transport: &dyn Transport,
    emit: &mut impl FnMut(GrammarProgressEvent),
    timeout: Duration,
) -> Result<GrammarResult, EngineError> {
    let (url, auth, model, max_tokens) = request_parts(cfg, api_key)?;
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": text }
        ],
        "temperature": 0.0,
        "max_tokens": max_tokens,
        "stream": true,
    });
    let resp = post(transport, &url, &auth, body, timeout).await?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(http_error(resp, status).await);
    }

    let mut draft = GrammarDraft::default();
    let mut assembler = LineAssembler::default();
    let mut parse_err: Option<EngineError> = None;
    let stream = resp.bytes_stream();
    futures_util::pin_mut!(stream);
    fold_openai_sse(stream, |delta| {
        if parse_err.is_some() {
            return;
        }
        for line in assembler.push(delta) {
            match apply_line(&mut draft, &line, text, emit) {
                Ok(()) => {}
                Err(err) => parse_err = Some(err),
            }
        }
    })
    .await?;
    if let Some(err) = parse_err {
        return Err(err);
    }
    for line in assembler.finish() {
        apply_line(&mut draft, &line, text, emit)?;
    }
    draft
        .finalize(provider_label(PROVIDER_LLM).into(), text.into())
        .map_err(parse_to_engine)
}

fn apply_line(
    draft: &mut GrammarDraft,
    line: &str,
    text: &str,
    emit: &mut impl FnMut(GrammarProgressEvent),
) -> Result<(), EngineError> {
    let record = parse_ndjson_line(line).map_err(parse_to_engine)?;
    if let Some(event) = draft.ingest(record, text).map_err(parse_to_engine)? {
        emit(event);
    }
    Ok(())
}

async fn check_strict(
    cfg: &ProviderConfig,
    api_key: &str,
    text: &str,
    transport: &dyn Transport,
    emit: &mut impl FnMut(GrammarProgressEvent),
    timeout: Duration,
) -> Result<GrammarResult, EngineError> {
    let (url, auth, model, max_tokens) = request_parts(cfg, api_key)?;
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": RETRY_SYSTEM_PROMPT },
            { "role": "user", "content": text }
        ],
        "temperature": 0.0,
        "max_tokens": max_tokens,
        "stream": false,
    });
    let resp = post(transport, &url, &auth, body, timeout).await?;
    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(EngineError::from_http(
            provider_label(PROVIDER_LLM),
            status,
            &raw.chars().take(300).collect::<String>(),
            None,
        ));
    }
    let parsed: ChatResponse = serde_json::from_str(&raw).map_err(|e| EngineError::InvalidResponse {
        provider: provider_label(PROVIDER_LLM).into(),
        detail: e.to_string(),
    })?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .map(|s| strip_fences(&s))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| EngineError::InvalidResponse {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: "choices[0].message.content 为空".into(),
        })?;
    let strict: StrictGrammarJson =
        serde_json::from_str(&content).map_err(|e| EngineError::InvalidResponse {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: e.to_string(),
        })?;

    let mut draft = GrammarDraft::default();
    apply_record(
        &mut draft,
        NdjsonRecord::Overall {
            text: strict.overall,
        },
        text,
        emit,
    )?;
    apply_record(
        &mut draft,
        NdjsonRecord::CorrectedText {
            text: strict.corrected_text,
        },
        text,
        emit,
    )?;
    for raw_err in strict.errors {
        apply_record(&mut draft, NdjsonRecord::Error(raw_err), text, emit)?;
    }
    apply_record(&mut draft, NdjsonRecord::Done, text, emit)?;
    draft
        .finalize(provider_label(PROVIDER_LLM).into(), text.into())
        .map_err(parse_to_engine)
}

fn apply_record(
    draft: &mut GrammarDraft,
    record: NdjsonRecord,
    text: &str,
    emit: &mut impl FnMut(GrammarProgressEvent),
) -> Result<(), EngineError> {
    if let Some(event) = draft.ingest(record, text).map_err(parse_to_engine)? {
        emit(event);
    }
    Ok(())
}

fn request_parts(
    cfg: &ProviderConfig,
    api_key: &str,
) -> Result<(String, String, String, u32), EngineError> {
    let provider = provider_label(PROVIDER_LLM);
    let Some(base) = cfg.base_url.clone() else {
        return Err(EngineError::InvalidConfig {
            provider: provider.into(),
            detail: "baseUrl 未配置".into(),
        });
    };
    let Some(model) = cfg.model.clone() else {
        return Err(EngineError::InvalidConfig {
            provider: provider.into(),
            detail: "model 未配置".into(),
        });
    };
    Ok((
        completions_url(&base),
        format!("Bearer {api_key}"),
        model,
        cfg.max_tokens.unwrap_or(1024),
    ))
}

async fn post(
    transport: &dyn Transport,
    url: &str,
    auth: &str,
    body: serde_json::Value,
    timeout: Duration,
) -> Result<reqwest::Response, EngineError> {
    transport
        .post_json_with_timeout(
            url,
            &[("Authorization", auth), ("Accept", "text/event-stream")],
            body,
            timeout,
        )
        .await
        .map_err(|e| EngineError::Network {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: e.to_string(),
        })
}

async fn http_error(resp: reqwest::Response, status: u16) -> EngineError {
    let retry_after = resp
        .headers()
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let detail = resp.text().await.unwrap_or_default();
    EngineError::from_http(
        provider_label(PROVIDER_LLM),
        status,
        &detail.chars().take(300).collect::<String>(),
        retry_after,
    )
}

fn parse_to_engine(fail: ParseFail) -> EngineError {
    EngineError::InvalidResponse {
        provider: provider_label(PROVIDER_LLM).into(),
        detail: fail.reason(),
    }
}

fn strip_fences(content: &str) -> String {
    let mut s = content.trim().to_string();
    if let Some(rest) = s.strip_prefix("```json") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix("```") {
        s = rest.to_string();
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.to_string();
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::grammar::{cache_clear, check_llm};
    use crate::providers::PROVIDER_LLM;
    use crate::services::transport::ReqwestTransport;

    fn cfg(base_url: &str) -> ProviderConfig {
        let mut c = ProviderConfig::builtin(PROVIDER_LLM);
        c.base_url = Some(base_url.to_string());
        c.enabled = true;
        c
    }

    fn ndjson_sse_body() -> String {
        let records = [
            r#"{"type":"overall","text":"主谓不一致"}"#,
            r#"{"type":"correctedText","text":"He goes to school."}"#,
            r#"{"type":"error","original":"go","corrected":"goes","errorType":"grammar","severity":"high","explanation":"第三人称单数需 goes","suggestions":["goes"]}"#,
            r#"{"type":"done"}"#,
        ];
        let joined = records.join("\n");
        let mut sse = String::new();
        for ch in joined.chars() {
            let data = serde_json::json!({
                "choices": [{ "delta": { "content": ch.to_string() } }]
            });
            sse.push_str(&format!("data: {data}\n\n"));
        }
        sse.push_str("data: [DONE]\n\n");
        sse
    }

    #[test]
    fn streaming_mock_full_chain_emits_semantic_events() {
        cache_clear();
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer sk-test")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(ndjson_sse_body())
            .create();
        let mut events = Vec::new();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            check(
                &cfg(&base),
                "sk-test",
                "He go to school.",
                &transport,
                &mut |e| events.push(e),
            )
            .await
        });
        mock.assert();
        let result = out.expect("stream grammar should succeed");
        assert_eq!(result.corrected_text.as_deref(), Some("He goes to school."));
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].offset, Some(3));
        assert!(events
            .iter()
            .any(|e| matches!(e, GrammarProgressEvent::Overall { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, GrammarProgressEvent::Error { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, GrammarProgressEvent::Finished)));
        assert!(!events
            .iter()
            .any(|e| matches!(e, GrammarProgressEvent::Retrying { .. })));
    }

    #[test]
    fn structure_failure_retries_once_with_strict_json() {
        cache_clear();
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let bad = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body("data: {\"choices\":[{\"delta\":{\"content\":\"not json\"}}]}\n\ndata: [DONE]\n\n")
            .expect(1)
            .create();
        let good = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(
                r#"{"choices":[{"message":{"content":"{\"overall\":\"无误\",\"correctedText\":\"Hello.\",\"errors\":[]}"}}]}"#,
            )
            .expect(1)
            .create();
        let mut events = Vec::new();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            check(
                &cfg(&base),
                "sk-test",
                "Hello.",
                &transport,
                &mut |e| events.push(e),
            )
            .await
        });
        bad.assert();
        good.assert();
        let result = out.expect("retry should succeed");
        assert_eq!(result.overall.as_deref(), Some("无误"));
        assert!(events
            .iter()
            .any(|e| matches!(e, GrammarProgressEvent::Retrying { .. })));
    }

    #[test]
    fn cache_hit_skips_http() {
        cache_clear();
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(ndjson_sse_body())
            .expect(1)
            .create();
        let transport = ReqwestTransport::default();
        tauri::async_runtime::block_on(async {
            let mut ignore = |_| {};
            check_llm(
                &cfg(&base),
                "sk-test".into(),
                "He go to school.".into(),
                &transport,
                &mut ignore,
            )
            .await
            .unwrap();
            let mut ignore = |_| {};
            check_llm(
                &cfg(&base),
                "sk-test".into(),
                "He go to school.".into(),
                &transport,
                &mut ignore,
            )
            .await
            .unwrap();
        });
        mock.assert();
    }

    #[test]
    fn prompt_is_english_check_chinese_explain_closed_enums() {
        assert!(SYSTEM_PROMPT.contains("Simplified Chinese"));
        assert!(SYSTEM_PROMPT.contains("NDJSON"));
        assert!(SYSTEM_PROMPT.contains("word_choice"));
        assert!(SYSTEM_PROMPT.contains(r#"{"type":"done"}"#));
        assert!(RETRY_SYSTEM_PROMPT.contains("SINGLE JSON object"));
        assert_eq!(prompt_version(), "v1");
    }

    #[test]
    fn short_timeout_against_hanging_socket_is_network() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let _keep = listener;
            std::thread::sleep(std::time::Duration::from_secs(30));
        });
        let started = std::time::Instant::now();
        let mut events = Vec::new();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::new().unwrap();
            check_with_timeout(
                &cfg(&format!("http://{addr}/v1")),
                "sk-test",
                "Hello.",
                &transport,
                &mut |e| events.push(e),
                Duration::from_millis(250),
            )
            .await
        });
        assert!(matches!(out, Err(EngineError::Network { .. })));
        assert!(started.elapsed() < Duration::from_secs(4));
        assert!(!events
            .iter()
            .any(|e| matches!(e, GrammarProgressEvent::Retrying { .. })));
    }
}
