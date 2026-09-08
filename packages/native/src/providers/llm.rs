//! LLM（OpenAI 兼容协议）翻译适配器（§5.2：Bearer 认证）。

use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::services::transport::Transport;

use super::{lang, provider_label, EngineError, ProviderConfig, TranslateRequest, TranslateResult};

#[derive(Debug, Deserialize)]
pub(crate) struct ChatResponse {
    #[serde(default)]
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatChoice {
    pub message: ChatMessage,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ChatMessage {
    #[serde(default)]
    pub content: Option<MessageContent>,
}

/// OpenAI 兼容接口里 `message.content` 可能是字符串，也可能是 parts 数组。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Deserialize)]
pub(crate) struct ContentPart {
    #[serde(default)]
    text: Option<String>,
}

impl MessageContent {
    fn as_text(&self) -> Option<String> {
        match self {
            Self::Text(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            Self::Parts(parts) => {
                let joined: String = parts.iter().filter_map(|p| p.text.as_deref()).collect();
                let t = joined.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
        }
    }
}

impl ChatResponse {
    pub(crate) fn first_text(&self) -> Option<String> {
        self.choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .and_then(MessageContent::as_text)
    }
}

pub(crate) fn parse_chat_response(raw: &str) -> Result<ChatResponse, EngineError> {
    serde_json::from_str(raw).map_err(|e| EngineError::InvalidResponse {
        provider: provider_label(super::PROVIDER_LLM).into(),
        detail: e.to_string(),
    })
}

pub(crate) fn first_choice_text(raw: &str) -> Result<String, EngineError> {
    parse_chat_response(raw)?
        .first_text()
        .ok_or_else(|| EngineError::InvalidResponse {
            provider: provider_label(super::PROVIDER_LLM).into(),
            detail: "choices[0].message.content 为空".into(),
        })
}

/// DeepSeek V4 等 thinking 模型默认把输出额度花在 `reasoning_content` 上，
/// 翻译/语法/测试都不需要思维链；关掉后才有稳定的 `content`。
pub(crate) fn with_openai_compat(mut body: Value, cfg: &ProviderConfig) -> Value {
    if is_deepseek_compat(cfg) {
        body["thinking"] = json!({ "type": "disabled" });
    }
    body
}

fn is_deepseek_compat(cfg: &ProviderConfig) -> bool {
    let url = cfg.base_url.as_deref().unwrap_or("");
    let kind = cfg.kind.as_deref().unwrap_or("");
    let model = cfg.model.as_deref().unwrap_or("");
    url.to_ascii_lowercase().contains("deepseek")
        || kind.eq_ignore_ascii_case("deepseek")
        || model.to_ascii_lowercase().contains("deepseek")
}

pub async fn translate(
    cfg: &ProviderConfig,
    api_key: &str,
    req: &TranslateRequest,
    transport: &dyn Transport,
) -> Result<TranslateResult, EngineError> {
    let provider = provider_label(super::PROVIDER_LLM);
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

    let source = lang::resolve_source(req.from.as_deref(), &req.text);
    let target_name = lang::language_name(&req.to);
    let system = format!(
        "You are a professional translation engine. Translate the user's text into {target_name}. Output only the translated text in {target_name}. Never copy the source unchanged when the target language differs. Do not add explanations, quotes, markdown, or JSON."
    );
    let user = match &source {
        Some(from) => format!(
            "Source language: {}\n\n{}",
            lang::language_name(from),
            req.text
        ),
        None => req.text.clone(),
    };
    let body = with_openai_compat(
        json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "temperature": cfg.temperature.unwrap_or(0.3),
            "max_tokens": cfg.max_tokens.unwrap_or(1024),
        }),
        cfg,
    );

    let auth = format!("Bearer {api_key}");
    let resp = transport
        .post_json_with_headers(
            &completions_url(&base),
            &[("Authorization", auth.as_str())],
            body,
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
    let body = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        let detail = body.chars().take(300).collect::<String>();
        return Err(EngineError::from_http(
            &provider,
            status,
            &detail,
            retry_after,
        ));
    }

    let output = first_choice_text(&body)?;

    Ok(TranslateResult {
        engine: provider.into(),
        output,
        input: req.text.clone(),
        detected_from: source,
        target: req.to.clone(),
        duration_ms: 0,
        enrichment: None,
        enrichment_word: None,
        enrichment_pending: false,
    })
}

/// 测试连接：一次真实的 chat/completions 请求，不伪造 ok。
/// DeepSeek V4 thinking 模型会把 `max_tokens=1` 全部花在思维链上，导致 `content` 为空，
/// 因此这里给够少量 completion 额度，并在 DeepSeek 兼容端关掉 thinking。
pub async fn test_connection(
    cfg: &ProviderConfig,
    api_key: &str,
    transport: &dyn Transport,
) -> Result<String, EngineError> {
    let provider = provider_label(super::PROVIDER_LLM);
    let Some(base) = cfg.base_url.clone() else {
        return Err(EngineError::InvalidConfig {
            provider: provider.into(),
            detail: "baseUrl 未配置".into(),
        });
    };
    let model = cfg.model.clone().unwrap_or_else(|| "gpt-4o-mini".into());
    let body = with_openai_compat(
        json!({
            "model": model,
            "messages": [
                { "role": "system", "content": "Reply with the single word ok." },
                { "role": "user", "content": "ping" }
            ],
            "max_tokens": 32,
            "temperature": 0.0,
        }),
        cfg,
    );
    let auth = format!("Bearer {api_key}");
    let resp = transport
        .post_json_with_headers(
            &completions_url(&base),
            &[("Authorization", auth.as_str())],
            body,
        )
        .await
        .map_err(|e| EngineError::Network {
            provider: provider.into(),
            detail: e.to_string(),
        })?;
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(EngineError::from_http(
            &provider,
            status,
            &body.chars().take(300).collect::<String>(),
            None,
        ));
    }
    let parsed = parse_chat_response(&body)?;
    if parsed.first_text().is_none() && parsed.choices.is_empty() {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "测试响应为空".into(),
        });
    }
    Ok(format!("{provider} 连接正常"))
}

pub(crate) fn completions_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::PROVIDER_LLM;
    use crate::services::transport::ReqwestTransport;

    fn cfg(base_url: &str) -> ProviderConfig {
        let mut c = ProviderConfig::builtin(PROVIDER_LLM);
        c.base_url = Some(base_url.to_string());
        c.enabled = true;
        c
    }

    #[test]
    fn deepseek_compat_disables_thinking() {
        let mut c = cfg("https://api.deepseek.com");
        c.model = Some("deepseek-v4.1-flash-expires-on-0910".into());
        let body = with_openai_compat(json!({"model": "x"}), &c);
        assert_eq!(body["thinking"]["type"], "disabled");
    }

    #[test]
    fn openai_compat_does_not_send_thinking() {
        let body = with_openai_compat(
            json!({"model": "gpt-4o-mini"}),
            &cfg("https://api.openai.com/v1"),
        );
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn parses_string_and_array_content() {
        let text = parse_chat_response(r#"{"choices":[{"message":{"content":"你好"}}]}"#)
            .unwrap()
            .first_text();
        assert_eq!(text.as_deref(), Some("你好"));
        let parts = parse_chat_response(
            r#"{"choices":[{"message":{"content":[{"type":"text","text":"你好"}]}}]}"#,
        )
        .unwrap()
        .first_text();
        assert_eq!(parts.as_deref(), Some("你好"));
        let empty = parse_chat_response(
            r#"{"choices":[{"message":{"content":"","reasoning_content":"think"}}]}"#,
        )
        .unwrap();
        assert!(empty.first_text().is_none());
        assert_eq!(empty.choices.len(), 1);
    }

    #[test]
    fn llm_mock_full_chain_translates() {
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer sk-test")
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"content":"你好，世界"}}]}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "Hello, world".into(),
                from: Some("en".into()),
                to: "zh".into(),
            };
            let transport = ReqwestTransport::default();
            translate(&cfg(&base), "sk-test", &req, &transport).await
        });
        mock.assert();
        let out = out.expect("mock LLM 应成功");
        assert_eq!(out.output, "你好，世界");
        assert_eq!(out.engine, "LLM");
        assert_eq!(out.detected_from.as_deref(), Some("en"));
    }

    #[test]
    fn llm_short_latin_uses_english_and_chinese_names() {
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .match_body(mockito::Matcher::Regex(
                r#"Simplified Chinese[\s\S]*Source language: English[\s\S]*Epoch"#.into(),
            ))
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"content":"纪元"}}]}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "Epoch".into(),
                from: None,
                to: "zh".into(),
            };
            let transport = ReqwestTransport::default();
            translate(&cfg(&base), "sk-test", &req, &transport).await
        });
        mock.assert();
        let out = out.expect("mock LLM 应成功");
        assert_eq!(out.output, "纪元");
        assert_eq!(out.detected_from.as_deref(), Some("en"));
    }

    #[test]
    fn llm_401_is_unauthorized() {
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(401)
            .with_body(r#"{"error":{"message":"Invalid API key"}}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "x".into(),
                from: None,
                to: "zh".into(),
            };
            let transport = ReqwestTransport::default();
            translate(&cfg(&base), "bad", &req, &transport).await
        });
        mock.assert();
        assert!(matches!(out, Err(EngineError::Unauthorized { .. })));
    }

    #[test]
    fn test_connection_accepts_empty_content_with_choices() {
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"content":"","reasoning_content":"..."}}]}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            test_connection(&cfg(&base), "sk-test", &transport).await
        });
        mock.assert();
        assert!(out
            .expect("empty content still proves the endpoint is live")
            .contains("连接正常"));
    }

    #[test]
    fn llm_array_content_translates() {
        let mut server = mockito::Server::new();
        let base = format!("{}/v1", server.url());
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"content":[{"type":"text","text":"你好"}]}}]}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "Hello".into(),
                from: Some("en".into()),
                to: "zh".into(),
            };
            let transport = ReqwestTransport::default();
            translate(&cfg(&base), "sk-test", &req, &transport).await
        });
        mock.assert();
        assert_eq!(out.expect("array content").output, "你好");
    }
}
