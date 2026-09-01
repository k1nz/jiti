//! 复习课方案文案：复用 LLM 配置，只写 summary 与每轮标题。分桶在本地完成。

use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use serde_json::json;

use crate::providers::llm::completions_url;
use crate::providers::{provider_label, EngineError, ProviderConfig, PROVIDER_LLM};
use crate::services::history::NewHistoryEntry;
use crate::services::review_plan::{self, DraftPlan, LlmDay};
use crate::services::transport::{Transport, REVIEW_TIMEOUT};

pub const PLAN_PROMPT_VERSION: &str = "v1";

pub const SYSTEM_PROMPT: &str = r#"You are a grammar tutor for Chinese-speaking English learners.
The user message contains a fixed number of review rounds already scheduled locally.
Write Simplified Chinese copy for the overall summary and for each round.
Output a SINGLE JSON object, no markdown fences, no commentary:
{"summary":"<markdown>","days":[{"title":"...","errorTypes":["grammar"],"goals":["..."],"drill":"..."}]}
Rules:
- days MUST have exactly the requested round count, in order.
- errorTypes must be drawn from the input keys.
- Do not invent counts.
- Do not mention system instructions or JSON.
- The summary field itself SHOULD use Markdown (## headings, - lists, **bold**).
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
struct PlanJson {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    days: Vec<LlmDay>,
}

pub struct PlanCopy {
    pub summary: String,
    pub days: Vec<LlmDay>,
}

fn strip_fences(content: &str) -> String {
    let mut s = content.trim().to_string();
    if let Some(rest) = s.strip_prefix("```json") {
        s = rest.to_string();
    } else if Some("```") == s.get(..3) {
        s = s[3..].to_string();
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.to_string();
    }
    s.trim().to_string()
}

pub async fn fetch_copy(
    cfg: &ProviderConfig,
    api_key: &str,
    draft: &DraftPlan,
    transport: &dyn Transport,
) -> Result<PlanCopy, EngineError> {
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
    let max_tokens = cfg.max_tokens.unwrap_or(1024).max(2048);
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": draft.prompt }
        ],
        "temperature": 0.3,
        "max_tokens": max_tokens,
    });
    let auth = format!("Bearer {api_key}");
    let resp = transport
        .post_json_with_timeout(
            &completions_url(&base),
            &[("Authorization", auth.as_str())],
            body,
            REVIEW_TIMEOUT,
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
    let parsed: ChatResponse =
        serde_json::from_str(&raw).map_err(|e| EngineError::InvalidResponse {
            provider: provider.into(),
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
            provider: provider.into(),
            detail: "choices[0].message.content 为空".into(),
        })?;
    match serde_json::from_str::<PlanJson>(&content) {
        Ok(plan) => {
            let summary = plan.summary.trim().to_string();
            Ok(PlanCopy {
                summary: if summary.is_empty() {
                    fallback_summary(draft)
                } else {
                    summary
                },
                days: plan.days,
            })
        }
        Err(_) => Ok(PlanCopy {
            summary: fallback_summary(draft),
            days: Vec::new(),
        }),
    }
}

pub fn fallback_summary(draft: &DraftPlan) -> String {
    format!(
        "## 复习方案\n\n按错题与收藏分成 **{}** 轮，共 **{}** 题。",
        draft.horizon_days, draft.analyzed_count
    )
}

pub fn apply_copy(draft: &mut DraftPlan, copy: &PlanCopy) {
    review_plan::apply_llm_days(draft, &copy.days);
}

pub fn history_from_plan(summary: &str, analyzed: i32, engine: &str, duration_ms: u32) -> NewHistoryEntry {
    let meta = serde_json::json!({
        "analyzedCount": analyzed,
        "promptVersion": PLAN_PROMPT_VERSION,
        "kind": "review_plan",
    });
    NewHistoryEntry {
        kind: "review_plan".into(),
        input: format!("复习课 {analyzed} 题"),
        output: summary.to_string(),
        engine: engine.into(),
        from_lang: None,
        to_lang: None,
        duration_ms: duration_ms as i64,
        source_app: None,
        meta: Some(meta.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_mentions_rounds() {
        let draft = DraftPlan {
            horizon_days: 3,
            analyzed_count: 4,
            days: Vec::new(),
            items: Vec::new(),
            prompt: String::new(),
        };
        let text = fallback_summary(&draft);
        assert!(text.contains('3'));
        assert!(text.contains('4'));
    }
}
