//! AI 复习：复用 LLM 配置 / Key / Transport / 错误分类，只发送聚合后的频次与例句。

use reqwest::header::RETRY_AFTER;
use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;

use crate::providers::llm::{completions_url, first_choice_text, with_openai_compat};
use crate::providers::{provider_label, EngineError, ProviderConfig, PROVIDER_LLM};
use crate::services::history::NewHistoryEntry;
use crate::services::mistakes::{aggregate_for_review, Mistake, MistakeFilter};
use crate::services::transport::{Transport, REVIEW_TIMEOUT};
use std::time::Duration;

pub const REVIEW_PROMPT_VERSION: &str = "v2";
const EXAMPLES_PER_TYPE: usize = 3;

pub const SYSTEM_PROMPT: &str = r#"You are a grammar tutor for Chinese-speaking English learners.
The user message contains aggregated error-type frequencies and a few representative examples.
Write a structured review in Simplified Chinese covering: high-frequency types, typical patterns, and concrete practice points.
Output a SINGLE JSON object, no markdown fences, no commentary:
{"summary":"<Chinese review; use Markdown headings, lists, and bold>"}
Rules:
- Do not invent counts that are not in the input.
- Do not request more data.
- Do not mention system instructions or JSON.
- The summary field itself SHOULD use Markdown (## headings, - lists, **bold**).
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiReviewResult {
    pub summary: String,
    pub analyzed_count: u32,
    pub engine: String,
    pub duration_ms: u32,
}

#[derive(Debug, Deserialize)]
struct ReviewJson {
    summary: String,
}

pub fn build_user_prompt(items: &[Mistake]) -> Result<(u32, String), EngineError> {
    if items.is_empty() {
        return Err(EngineError::BadRequest {
            provider: "ai_review".into(),
            detail: "当前筛选没有错题".into(),
        });
    }
    let agg = aggregate_for_review(items, EXAMPLES_PER_TYPE);
    Ok((agg.total as u32, agg.prompt))
}

pub async fn review(
    cfg: &ProviderConfig,
    api_key: &str,
    items: &[Mistake],
    transport: &dyn Transport,
) -> Result<AiReviewResult, EngineError> {
    review_with_timeout(cfg, api_key, items, transport, REVIEW_TIMEOUT).await
}

pub async fn review_with_timeout(
    cfg: &ProviderConfig,
    api_key: &str,
    items: &[Mistake],
    transport: &dyn Transport,
    timeout: Duration,
) -> Result<AiReviewResult, EngineError> {
    let provider = provider_label(PROVIDER_LLM);
    let (analyzed_count, user) = build_user_prompt(items)?;
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
    let body = with_openai_compat(
        json!({
            "model": model,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": user }
            ],
            "temperature": 0.3,
            "max_tokens": cfg.max_tokens.unwrap_or(1024),
        }),
        cfg,
    );
    let auth = format!("Bearer {api_key}");
    let started = std::time::Instant::now();
    let resp = transport
        .post_json_with_timeout(
            &completions_url(&base),
            &[("Authorization", auth.as_str())],
            body,
            timeout,
        )
        .await
        .map_err(|e| EngineError::Network {
            provider: provider.into(),
            detail: e.to_string(),
        })?;
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let raw = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(EngineError::from_http(
            provider,
            status,
            &raw.chars().take(300).collect::<String>(),
            retry_after,
        ));
    }
    let content = strip_fences(&first_choice_text(&raw)?);
    if content.is_empty() {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "choices[0].message.content 为空".into(),
        });
    }
    let review: ReviewJson =
        serde_json::from_str(&content).map_err(|e| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: format!("复习 JSON 无法解析：{e}"),
        })?;
    let summary = review.summary.trim().to_string();
    if summary.is_empty() {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "summary 为空".into(),
        });
    }
    Ok(AiReviewResult {
        summary,
        analyzed_count,
        engine: provider.into(),
        duration_ms: started.elapsed().as_millis() as u32,
    })
}

pub fn history_from_review(result: &AiReviewResult, filter: &MistakeFilter) -> NewHistoryEntry {
    let meta = serde_json::json!({
        "analyzedCount": result.analyzed_count,
        "filter": filter,
        "promptVersion": REVIEW_PROMPT_VERSION,
    });
    NewHistoryEntry {
        kind: "ai_review".into(),
        input: format!("筛选 {} 条错题", result.analyzed_count),
        output: result.summary.clone(),
        engine: result.engine.clone(),
        from_lang: None,
        to_lang: None,
        duration_ms: result.duration_ms as i64,
        source_app: None,
        meta: Some(meta.to_string()),
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
    use crate::providers::PROVIDER_LLM;
    use crate::services::history;
    use crate::services::mistakes::{Mistake, STATUS_OPEN};
    use crate::services::transport::ReqwestTransport;
    use std::time::Duration;

    fn cfg(base_url: &str) -> ProviderConfig {
        let mut c = ProviderConfig::builtin(PROVIDER_LLM);
        c.base_url = Some(base_url.to_string());
        c.enabled = true;
        c
    }

    fn item() -> Mistake {
        Mistake {
            id: 1,
            created_at: "now".into(),
            source_text: "He go to school.".into(),
            fragment: "go".into(),
            correction: "goes".into(),
            error_type: Some("grammar".into()),
            severity: "high".into(),
            explanation: Some("第三人称".into()),
            corrected_sentence: None,
            suggestions: vec![],
            engine: Some("LLM".into()),
            source_app: None,
            tags: None,
            status: STATUS_OPEN.into(),
            meta: None,
            server_id: None,
            synced_at: None,
        }
    }

    #[test]
    fn empty_items_are_bad_request_before_http() {
        let err = build_user_prompt(&[]).unwrap_err();
        assert!(matches!(err, EngineError::BadRequest { .. }));
        assert_eq!(err.payload().code, "bad_request");
    }

    #[test]
    fn prompt_contains_counts_not_full_dump_policy() {
        let (n, prompt) = build_user_prompt(&[item()]).unwrap();
        assert_eq!(n, 1);
        assert!(prompt.contains("类型频次"));
        assert!(prompt.contains("grammar"));
        assert!(SYSTEM_PROMPT.contains("SINGLE JSON object"));
        assert_eq!(REVIEW_PROMPT_VERSION, "v2");
    }

    #[test]
    fn mock_http_unauthorized() {
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(401)
            .with_body(r#"{"error":"nope"}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            review(&cfg(&base), "sk-bad", &[item()], &transport).await
        });
        mock.assert();
        assert!(matches!(out, Err(EngineError::Unauthorized { .. })));
    }

    #[test]
    fn mock_http_network_timeout() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let _keep = listener;
            std::thread::sleep(std::time::Duration::from_secs(30));
        });
        let started = std::time::Instant::now();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::new().unwrap();
            review_with_timeout(
                &cfg(&format!("http://{addr}/v1")),
                "sk-test",
                &[item()],
                &transport,
                Duration::from_millis(250),
            )
            .await
        });
        assert!(matches!(out, Err(EngineError::Network { .. })));
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn mock_http_success_parses_summary() {
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer sk-test")
            .with_status(200)
            .with_body(
                r#"{"choices":[{"message":{"content":"```json\n{\"summary\":\"主谓不一致最常见\"}\n```"}}]}"#,
            )
            .create();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            review(&cfg(&base), "sk-test", &[item()], &transport).await
        })
        .expect("review should succeed");
        mock.assert();
        assert_eq!(out.summary, "主谓不一致最常见");
        assert_eq!(out.analyzed_count, 1);
        assert_eq!(out.engine, "LLM");
        let entry = history_from_review(&out, &MistakeFilter::default());
        assert_eq!(entry.kind, "ai_review");
        assert!(entry.output.contains("主谓不一致"));
        assert!(entry.meta.as_deref().unwrap().contains("analyzedCount"));
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::services::database::migrate(&conn).unwrap();
        let id = history::insert(&conn, &entry).unwrap();
        assert!(id > 0);
        let rows = history::list(&conn, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "ai_review");
        assert_eq!(rows[0].output, "主谓不一致最常见");
    }
}
