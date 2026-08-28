//! DeepL 适配器（§5.2：`auth_key` header）。

use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use serde_json::json;

use crate::services::transport::Transport;

use super::{lang, provider_label, EngineError, ProviderConfig, TranslateRequest, TranslateResult};

#[derive(Debug, Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Debug, Deserialize)]
struct DeepLTranslation {
    text: String,
    #[serde(rename = "detected_source_language")]
    detected_source_language: Option<String>,
}

pub async fn translate(
    cfg: &ProviderConfig,
    api_key: &str,
    req: &TranslateRequest,
    transport: &dyn Transport,
) -> Result<TranslateResult, EngineError> {
    let provider = provider_label(super::PROVIDER_DEEPL);
    let Some(base) = cfg.base_url.clone() else {
        return Err(EngineError::InvalidConfig {
            provider: provider.into(),
            detail: "baseUrl 未配置".into(),
        });
    };

    let mut body = json!({
        "text": [req.text.clone()],
        "target_lang": normalize_deepl_lang(&req.to),
    });
    let source = lang::resolve_source(req.from.as_deref(), &req.text);
    if let Some(from) = &source {
        body["source_lang"] = json!(normalize_deepl_lang(from));
    }

    let auth = format!("DeepL-Auth-Key {api_key}");
    let url = translate_url(&base);
    let resp = transport
        .post_json_with_headers(&url, &[("Authorization", auth.as_str())], body)
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
        let detail = truncate(&body, 300);
        return Err(EngineError::from_http(
            &provider,
            status,
            &detail,
            retry_after,
        ));
    }

    let parsed: DeepLResponse =
        serde_json::from_str(&body).map_err(|e| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: e.to_string(),
        })?;
    let Some(t) = parsed.translations.into_iter().next() else {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "translations 为空".into(),
        });
    };

    Ok(TranslateResult {
        engine: provider.into(),
        output: t.text,
        input: req.text.clone(),
        detected_from: t
            .detected_source_language
            .as_deref()
            .map(normalize_detected)
            .or_else(|| source.map(|s| normalize_detected(&s))),
        target: normalize_target(&req.to),
        duration_ms: 0,
        enrichment: None,
        enrichment_word: None,
        enrichment_pending: false,
    })
}

/// 测试连接：GET /v2/usage，只报告 ok/fail + 原因（§5.1）。
pub async fn test_connection(
    cfg: &ProviderConfig,
    api_key: &str,
    transport: &dyn Transport,
) -> Result<String, EngineError> {
    let provider = provider_label(super::PROVIDER_DEEPL);
    let Some(base) = cfg.base_url.clone() else {
        return Err(EngineError::InvalidConfig {
            provider: provider.into(),
            detail: "baseUrl 未配置".into(),
        });
    };
    let auth = format!("DeepL-Auth-Key {api_key}");
    let resp = transport
        .get_with_headers(&usage_url(&base), &[("Authorization", auth.as_str())])
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
            &truncate(&body, 300),
            None,
        ));
    }
    Ok(format!("{provider} 连接正常"))
}

/// baseUrl 若已带 `/v2` 直接拼 `/translate`，否则补 `/v2/translate`。
fn translate_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/v2") {
        format!("{base}/translate")
    } else {
        format!("{base}/v2/translate")
    }
}

fn usage_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/v2") {
        format!("{base}/usage")
    } else {
        format!("{base}/v2/usage")
    }
}

fn normalize_deepl_lang(s: &str) -> String {
    let s = s.trim();
    if s.eq_ignore_ascii_case("zh")
        || s.eq_ignore_ascii_case("zh-cn")
        || s.eq_ignore_ascii_case("zh-hans")
    {
        return "ZH".into();
    }
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

fn normalize_target(s: &str) -> String {
    let v = normalize_deepl_lang(s);
    if v == "ZH" {
        "zh".into()
    } else {
        v.to_ascii_lowercase()
    }
}

fn normalize_detected(s: &str) -> String {
    s.split('-').next().unwrap_or(s).to_ascii_lowercase()
}

fn truncate(s: &str, max: usize) -> String {
    let taken: String = s.chars().take(max).collect();
    if taken.chars().count() < s.chars().count() {
        format!("{taken}…")
    } else {
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::PROVIDER_DEEPL;
    use crate::services::transport::ReqwestTransport;

    fn cfg(base_url: &str) -> ProviderConfig {
        let mut c = ProviderConfig::builtin(PROVIDER_DEEPL);
        c.base_url = Some(base_url.to_string());
        c.enabled = true;
        c
    }

    #[test]
    fn deepl_mock_full_chain_returns_normalized_result() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v2/translate")
            .match_header("authorization", "DeepL-Auth-Key test-key")
            .with_status(200)
            .with_body(
                r#"{"translations":[{"detected_source_language":"EN","text":"你好，世界"}]}"#,
            )
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "Hello, world".into(),
                from: None,
                to: "zh".into(),
            };
            let transport = ReqwestTransport::default();
            translate(&cfg(&server.url()), "test-key", &req, &transport).await
        });
        mock.assert();
        let out = out.expect("mock DeepL 应成功");
        assert_eq!(out.output, "你好，世界");
        assert_eq!(out.detected_from.as_deref(), Some("en"));
        assert_eq!(out.target, "zh");
        assert_eq!(out.engine, "DeepL");
    }

    #[test]
    fn deepl_short_latin_sends_english_source() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v2/translate")
            .match_body(mockito::Matcher::PartialJson(json!({
                "text": ["Epoch"],
                "source_lang": "EN",
                "target_lang": "ZH",
            })))
            .with_status(200)
            .with_body(r#"{"translations":[{"detected_source_language":"EN","text":"纪元"}]}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "Epoch".into(),
                from: None,
                to: "zh".into(),
            };
            let transport = ReqwestTransport::default();
            translate(&cfg(&server.url()), "test-key", &req, &transport).await
        });
        mock.assert();
        let out = out.expect("mock DeepL 应成功");
        assert_eq!(out.output, "纪元");
        assert_eq!(out.detected_from.as_deref(), Some("en"));
        assert_eq!(out.target, "zh");
    }

    #[test]
    fn deepl_403_is_classified_as_unauthorized() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v2/translate")
            .with_status(403)
            .with_body(r#"{"message":"Forbidden"}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "x".into(),
                from: None,
                to: "zh".into(),
            };
            let transport = ReqwestTransport::default();
            translate(&cfg(&server.url()), "bad", &req, &transport).await
        });
        mock.assert();
        assert!(matches!(out, Err(EngineError::Unauthorized { .. })));
    }

    #[test]
    fn deepl_429_is_rate_limited() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/v2/translate")
            .with_status(429)
            .with_header("retry-after", "7")
            .with_body("too many")
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "x".into(),
                from: None,
                to: "zh".into(),
            };
            let transport = ReqwestTransport::default();
            translate(&cfg(&server.url()), "k", &req, &transport).await
        });
        mock.assert();
        assert!(matches!(
            out,
            Err(EngineError::RateLimited {
                retry_after: Some(7),
                ..
            })
        ));
    }
}
