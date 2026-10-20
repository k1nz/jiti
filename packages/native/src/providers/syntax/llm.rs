//! LLM 句法适配器：NDJSON 流式协议、SSE 解码、结构失败时一次非流式重试。

use std::time::{Duration, Instant};

use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use serde_json::json;

use super::{
    parse_ndjson_line, NdjsonChunk, NdjsonReading, NdjsonRecord, NdjsonSkeleton, NdjsonStructure,
    NdjsonTip, ParseFail, SyntaxDraft, SyntaxProgressEvent, SyntaxResult,
};
use crate::providers::grammar::sse::{fold_openai_sse_until, LineAssembler};
use crate::providers::llm::{completions_url, first_choice_text, with_openai_compat};
use crate::providers::{provider_label, EngineError, ProviderConfig, PROVIDER_LLM};
#[cfg(test)]
use crate::services::transport::GRAMMAR_TIMEOUT;
use crate::services::transport::{first_byte_budget, remaining_budget, retry_budget, Transport};

/// 未单独配置 max_tokens 时，给讲解留够篇幅。
const DEFAULT_MAX_TOKENS: u32 = 2048;

pub const SYSTEM_PROMPT: &str = r#"You are a syntax tutor for Chinese-speaking learners.
Explain an input sentence. It may be Chinese, English, or mixed.
Write every gloss, label, note, reading, and tip in Simplified Chinese.
Do not correct grammar and do not list mistakes.
Output ONLY NDJSON: one JSON object per line. No markdown, no commentary.

Emit records in this exact order:
1. one or more chunk records, in reading order
2. one or more reading records; put the straightforward reading first, then a more natural one when it differs
3. one or more structure records
4. zero or more skeleton records
5. zero or more tip records, only for points that are easy to misread
6. {"type":"done"}

Chunk:
{"type":"chunk","text":"<exact substring copied from the input>","gloss":"<Chinese meaning of this piece>","notes":["<optional word-sense or context note>"]}

Reading:
{"type":"reading","text":"<full-sentence Chinese>","tone":"literal|natural"}

Structure:
{"type":"structure","role":"subject|predicate|object|attribute|adverbial|complement|other","text":"<exact substring copied from the input>","label":"<Chinese role phrase such as 名词性短语，作主语>","note":"<optional note about an omitted word or clause, or empty string>"}

Skeleton:
{"type":"skeleton","role":"subject|predicate|object|attribute|adverbial|complement|other","text":"<pattern line; may add omitted words in parentheses, such as (that)>"}

Tip:
{"type":"tip","text":"<one Chinese point a learner might misread>"}

Rules:
- chunk.text and structure.text MUST be copied verbatim from the input.
- skeleton.text MAY include words that were omitted from the input.
- tone MUST be literal or natural.
- role MUST be one of: subject, predicate, object, attribute, adverbial, complement, other.
- Omit tips when nothing is easy to misread.
- Do not wrap output in markdown fences.
"#;

pub const RETRY_SYSTEM_PROMPT: &str = r#"You are a syntax tutor for Chinese-speaking learners.
Explain an input sentence. It may be Chinese, English, or mixed.
Write every gloss, label, note, reading, and tip in Simplified Chinese.
Do not correct grammar and do not list mistakes.
Output a SINGLE JSON object, no markdown, no NDJSON:

{"chunks":[{"text":"<exact substring copied from the input>","gloss":"<Chinese meaning>","notes":["<optional note>"]}],"readings":[{"text":"<full-sentence Chinese>","tone":"literal|natural"}],"structures":[{"role":"subject|predicate|object|attribute|adverbial|complement|other","text":"<exact substring copied from the input>","label":"<Chinese role phrase>","note":"<optional note or empty string>"}],"skeletons":[{"role":"subject|predicate|object|attribute|adverbial|complement|other","text":"<pattern line; may include omitted words such as (that)>"}],"tips":[{"text":"<one Chinese point>"}]}

Rules:
- chunk text and structure text MUST be copied verbatim from the input.
- skeleton text MAY include words that were omitted from the input.
- tone MUST be literal or natural.
- role MUST be one of: subject, predicate, object, attribute, adverbial, complement, other.
- readings must start with tone literal.
- skeletons and tips may be empty arrays.
- Do not wrap output in markdown fences.
"#;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StrictSyntaxJson {
    #[serde(default)]
    chunks: Vec<NdjsonChunk>,
    #[serde(default)]
    readings: Vec<NdjsonReading>,
    #[serde(default)]
    structures: Vec<NdjsonStructure>,
    #[serde(default)]
    skeletons: Vec<NdjsonSkeleton>,
    #[serde(default)]
    tips: Vec<NdjsonTip>,
}

#[cfg(test)]
pub fn prompt_version() -> &'static str {
    super::PROMPT_VERSION
}

#[cfg(test)]
pub async fn check(
    cfg: &ProviderConfig,
    api_key: &str,
    text: &str,
    transport: &dyn Transport,
    emit: &mut impl FnMut(SyntaxProgressEvent),
) -> Result<SyntaxResult, EngineError> {
    check_until(
        cfg,
        api_key,
        text,
        transport,
        emit,
        GRAMMAR_TIMEOUT,
        &|| true,
    )
    .await
}

#[cfg(test)]
pub async fn check_with_timeout(
    cfg: &ProviderConfig,
    api_key: &str,
    text: &str,
    transport: &dyn Transport,
    emit: &mut impl FnMut(SyntaxProgressEvent),
    timeout: Duration,
) -> Result<SyntaxResult, EngineError> {
    check_until(cfg, api_key, text, transport, emit, timeout, &|| true).await
}

/// 流式尝试 + 一次严格重试共享同一个总预算；`alive` 为假时停止出网并返回 Cancelled。
pub async fn check_until(
    cfg: &ProviderConfig,
    api_key: &str,
    text: &str,
    transport: &dyn Transport,
    emit: &mut impl FnMut(SyntaxProgressEvent),
    timeout: Duration,
    alive: &impl Fn() -> bool,
) -> Result<SyntaxResult, EngineError> {
    let provider = provider_label(PROVIDER_LLM);
    let deadline = crate::services::transport::deadline_from_now(timeout);
    ensure_alive(alive)?;
    emit(SyntaxProgressEvent::Started {
        engine: provider.into(),
    });

    match check_streaming(cfg, api_key, text, transport, emit, deadline, alive).await {
        Ok(result) => {
            ensure_alive(alive)?;
            emit(SyntaxProgressEvent::Finished);
            Ok(result)
        }
        Err(EngineError::Cancelled { .. }) => Err(EngineError::Cancelled {
            provider: provider.into(),
        }),
        Err(EngineError::InvalidResponse { .. }) => {
            let Some(left) = retry_budget(deadline) else {
                return Err(EngineError::Network {
                    provider: provider.into(),
                    detail: "句法分析超过总时限".into(),
                });
            };
            ensure_alive(alive)?;
            emit(SyntaxProgressEvent::Retrying {
                reason: "正在重新解析".into(),
            });
            let result = check_strict(
                cfg,
                api_key,
                text,
                transport,
                emit,
                Instant::now() + left,
                alive,
            )
            .await?;
            ensure_alive(alive)?;
            emit(SyntaxProgressEvent::Finished);
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
    emit: &mut impl FnMut(SyntaxProgressEvent),
    deadline: Instant,
    alive: &impl Fn() -> bool,
) -> Result<SyntaxResult, EngineError> {
    ensure_alive(alive)?;
    let (url, auth, model, max_tokens) = request_parts(cfg, api_key)?;
    let body = with_openai_compat(
        json!({
            "model": model,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": text }
            ],
            "temperature": 0.0,
            "max_tokens": max_tokens,
            "stream": true,
        }),
        cfg,
    );
    let resp = post(transport, &url, &auth, body, deadline).await?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(http_error(resp, status).await);
    }

    let mut draft = SyntaxDraft::default();
    let mut assembler = LineAssembler::default();
    let mut parse_err: Option<EngineError> = None;
    let stream = resp.bytes_stream();
    futures_util::pin_mut!(stream);
    let left = remaining_budget(deadline);
    if left.is_zero() {
        return Err(EngineError::Network {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: "句法分析超过总时限".into(),
        });
    }
    let fold = fold_openai_sse_until(
        stream,
        |delta| {
            if parse_err.is_some() || !alive() {
                return;
            }
            for line in assembler.push(delta) {
                match apply_line(&mut draft, &line, text, emit) {
                    Ok(()) => {}
                    Err(err) => parse_err = Some(err),
                }
            }
        },
        || !alive(),
    );
    match tokio::time::timeout(left, fold).await {
        Ok(Ok(_)) => {}
        Ok(Err(err)) => return Err(err),
        Err(_) => {
            return Err(EngineError::Network {
                provider: provider_label(PROVIDER_LLM).into(),
                detail: "句法分析超过总时限".into(),
            });
        }
    }
    ensure_alive(alive)?;
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
    draft: &mut SyntaxDraft,
    line: &str,
    text: &str,
    emit: &mut impl FnMut(SyntaxProgressEvent),
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
    emit: &mut impl FnMut(SyntaxProgressEvent),
    deadline: Instant,
    alive: &impl Fn() -> bool,
) -> Result<SyntaxResult, EngineError> {
    ensure_alive(alive)?;
    let (url, auth, model, max_tokens) = request_parts(cfg, api_key)?;
    let body = with_openai_compat(
        json!({
            "model": model,
            "messages": [
                { "role": "system", "content": RETRY_SYSTEM_PROMPT },
                { "role": "user", "content": text }
            ],
            "temperature": 0.0,
            "max_tokens": max_tokens,
            "stream": false,
        }),
        cfg,
    );
    let resp = post(transport, &url, &auth, body, deadline).await?;
    ensure_alive(alive)?;
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
    let content = strip_fences(&first_choice_text(&raw)?);
    if content.is_empty() {
        return Err(EngineError::InvalidResponse {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: "choices[0].message.content 为空".into(),
        });
    }
    let strict: StrictSyntaxJson =
        serde_json::from_str(&content).map_err(|e| EngineError::InvalidResponse {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: e.to_string(),
        })?;

    let mut draft = SyntaxDraft::default();
    for raw in strict.chunks {
        apply_record(&mut draft, NdjsonRecord::Chunk(raw), text, emit)?;
    }
    for raw in strict.readings {
        apply_record(&mut draft, NdjsonRecord::Reading(raw), text, emit)?;
    }
    for raw in strict.structures {
        apply_record(&mut draft, NdjsonRecord::Structure(raw), text, emit)?;
    }
    for raw in strict.skeletons {
        apply_record(&mut draft, NdjsonRecord::Skeleton(raw), text, emit)?;
    }
    for raw in strict.tips {
        apply_record(&mut draft, NdjsonRecord::Tip(raw), text, emit)?;
    }
    apply_record(&mut draft, NdjsonRecord::Done, text, emit)?;
    draft
        .finalize(provider_label(PROVIDER_LLM).into(), text.into())
        .map_err(parse_to_engine)
}

fn apply_record(
    draft: &mut SyntaxDraft,
    record: NdjsonRecord,
    text: &str,
    emit: &mut impl FnMut(SyntaxProgressEvent),
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
        cfg.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
    ))
}

fn ensure_alive(alive: &impl Fn() -> bool) -> Result<(), EngineError> {
    if alive() {
        Ok(())
    } else {
        Err(EngineError::Cancelled {
            provider: provider_label(PROVIDER_LLM).into(),
        })
    }
}

async fn post(
    transport: &dyn Transport,
    url: &str,
    auth: &str,
    body: serde_json::Value,
    deadline: Instant,
) -> Result<reqwest::Response, EngineError> {
    let remaining = remaining_budget(deadline);
    if remaining.is_zero() {
        return Err(EngineError::Network {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: "请求超过总时限".into(),
        });
    }
    let first_byte = first_byte_budget(deadline);
    match tokio::time::timeout(
        first_byte,
        transport.post_json_with_timeout(
            url,
            &[("Authorization", auth), ("Accept", "text/event-stream")],
            body,
            remaining,
        ),
    )
    .await
    {
        Ok(Ok(resp)) => Ok(resp),
        Ok(Err(e)) => Err(EngineError::Network {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: e.to_string(),
        }),
        Err(_) => Err(EngineError::Network {
            provider: provider_label(PROVIDER_LLM).into(),
            detail: format!("连接或首字节超过 {}ms", first_byte.as_millis()),
        }),
    }
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
    use crate::providers::syntax::{cache_clear, check_llm};
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
            r#"{"type":"chunk","text":"The book","gloss":"这本书","notes":[]}"#,
            r#"{"type":"reading","text":"我买的那本书很贵。","tone":"literal"}"#,
            r#"{"type":"structure","role":"subject","text":"The book","label":"主语","note":""}"#,
            r#"{"type":"skeleton","role":"subject","text":"The book"}"#,
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
                "The book that I bought was expensive.",
                &transport,
                &mut |e| events.push(e),
            )
            .await
        });
        mock.assert();
        let result = out.expect("stream syntax should succeed");
        assert_eq!(result.primary_reading(), "我买的那本书很贵。");
        assert_eq!(result.chunks.len(), 1);
        assert_eq!(result.chunks[0].offset, Some(0));
        assert_eq!(result.structures.len(), 1);
        assert!(events
            .iter()
            .any(|e| matches!(e, SyntaxProgressEvent::Chunk { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, SyntaxProgressEvent::Reading { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, SyntaxProgressEvent::Finished)));
        assert!(!events
            .iter()
            .any(|e| matches!(e, SyntaxProgressEvent::Retrying { .. })));
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
            .with_body(
                "data: {\"choices\":[{\"delta\":{\"content\":\"not json\"}}]}\n\ndata: [DONE]\n\n",
            )
            .expect(1)
            .create();
        let good = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(
                r#"{"choices":[{"message":{"content":"{\"chunks\":[{\"text\":\"Hello\",\"gloss\":\"你好\",\"notes\":[]}],\"readings\":[{\"text\":\"你好。\",\"tone\":\"literal\"}],\"structures\":[{\"role\":\"subject\",\"text\":\"Hello\",\"label\":\"称呼\",\"note\":\"\"}],\"skeletons\":[],\"tips\":[]}"}}]}"#,
            )
            .expect(1)
            .create();
        let mut events = Vec::new();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            check(&cfg(&base), "sk-test", "Hello.", &transport, &mut |e| {
                events.push(e)
            })
            .await
        });
        bad.assert();
        good.assert();
        let result = out.expect("retry should succeed");
        assert_eq!(result.primary_reading(), "你好。");
        assert_eq!(result.chunks[0].text, "Hello");
        assert!(result.skeletons.is_empty());
        assert!(result.tips.is_empty());
        assert!(events
            .iter()
            .any(|e| matches!(e, SyntaxProgressEvent::Retrying { .. })));
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
        let input = "The book that I bought was expensive.";
        tauri::async_runtime::block_on(async {
            let mut ignore = |_| {};
            check_llm(
                &cfg(&base),
                "sk-test".into(),
                input.into(),
                &transport,
                &mut ignore,
                &|| true,
            )
            .await
            .unwrap();
            let mut ignore = |_| {};
            check_llm(
                &cfg(&base),
                "sk-test".into(),
                input.into(),
                &transport,
                &mut ignore,
                &|| true,
            )
            .await
            .unwrap();
        });
        mock.assert();
    }

    #[test]
    fn prompt_teaches_modules_with_closed_enums() {
        assert!(SYSTEM_PROMPT.contains("Simplified Chinese"));
        assert!(SYSTEM_PROMPT.contains("NDJSON"));
        assert!(SYSTEM_PROMPT.contains(r#""tone":"literal|natural""#));
        assert!(SYSTEM_PROMPT.contains(r#"{"type":"done"}"#));
        assert!(SYSTEM_PROMPT.contains("Do not correct grammar"));
        assert!(SYSTEM_PROMPT.contains("Omit tips"));
        assert!(RETRY_SYSTEM_PROMPT.contains("SINGLE JSON object"));
        assert_eq!(prompt_version(), "syntax-v2");
    }

    #[test]
    fn unset_max_tokens_defaults_to_2048() {
        let mut configured = cfg("https://example.com/v1");
        configured.model = Some("gpt-4o-mini".into());
        configured.max_tokens = None;
        let (_url, _auth, _model, tokens) = request_parts(&configured, "sk-test").expect("parts");
        assert_eq!(tokens, 2048);
        configured.max_tokens = Some(512);
        let (_url, _auth, _model, tokens) = request_parts(&configured, "sk-test").expect("parts");
        assert_eq!(tokens, 512);
    }

    #[test]
    fn cancelled_alive_flag_does_not_retry() {
        cache_clear();
        let mut events = Vec::new();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            super::check_until(
                &cfg("http://127.0.0.1:9/v1"),
                "sk-test",
                "Hello.",
                &transport,
                &mut |e| events.push(e),
                Duration::from_secs(20),
                &|| false,
            )
            .await
        });
        assert!(matches!(out, Err(EngineError::Cancelled { .. })));
        assert!(!events
            .iter()
            .any(|e| matches!(e, SyntaxProgressEvent::Retrying { .. })));
    }

    #[test]
    fn retry_shares_remaining_budget_instead_of_a_fresh_20s() {
        cache_clear();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let body = "data: {\"choices\":[{\"delta\":{\"content\":\"not json\"}}]}\n\ndata: [DONE]\n\n";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
            }
            if let Ok((_s, _)) = listener.accept() {
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
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
                Duration::from_millis(400),
            )
            .await
        });
        assert!(matches!(out, Err(EngineError::Network { .. })));
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(events
            .iter()
            .any(|e| matches!(e, SyntaxProgressEvent::Retrying { .. })));
    }
}
