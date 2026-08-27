//! LLM（OpenAI 兼容协议）翻译适配器（§5.2：Bearer 认证）。

use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use serde_json::json;

use crate::services::transport::Transport;

use super::{lang, provider_label, EngineError, ProviderConfig, TranslateRequest, TranslateResult};

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
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "temperature": cfg.temperature.unwrap_or(0.3),
        "max_tokens": cfg.max_tokens.unwrap_or(1024),
    });

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

    let parsed: ChatResponse =
        serde_json::from_str(&body).map_err(|e| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: e.to_string(),
        })?;
    let output = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "choices[0].message.content 为空".into(),
        })?;

    Ok(TranslateResult {
        engine: provider.into(),
        output,
        input: req.text.clone(),
        detected_from: source,
        target: req.to.clone(),
        duration_ms: 0,
    })
}

/// 测试连接：一次极小的 chat/completions 请求（max_tokens=1），不伪造 ok。
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
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": "Reply with the single word ok." },
            { "role": "user", "content": "ping" }
        ],
        "max_tokens": 1,
        "temperature": 0.0,
    });
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
    let parsed: ChatResponse =
        serde_json::from_str(&body).map_err(|e| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: e.to_string(),
        })?;
    let _ = parsed
        .choices
        .first()
        .and_then(|c| c.message.content.as_deref())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "测试响应为空".into(),
        })?;
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
}
