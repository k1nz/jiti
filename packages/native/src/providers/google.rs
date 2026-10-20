//! Google Cloud Translation API v2。Key 放在 `X-Goog-Api-Key`，避免出现在 URL 里。

use serde::Deserialize;
use serde_json::json;

use crate::services::transport::Transport;

use super::{
    lang, provider_label, EngineError, ProviderConfig, TranslateRequest, TranslateResult,
    PROVIDER_GOOGLE,
};

#[derive(Debug, Deserialize)]
struct GoogleResponse {
    data: GoogleData,
}

#[derive(Debug, Deserialize)]
struct GoogleData {
    translations: Vec<GoogleTranslation>,
}

#[derive(Debug, Deserialize)]
struct GoogleTranslation {
    #[serde(rename = "translatedText")]
    translated_text: String,
    #[serde(rename = "detectedSourceLanguage")]
    detected_source_language: Option<String>,
}

pub async fn translate(
    cfg: &ProviderConfig,
    api_key: &str,
    req: &TranslateRequest,
    transport: &dyn Transport,
) -> Result<TranslateResult, EngineError> {
    let provider = provider_label(PROVIDER_GOOGLE);
    let url = translate_url(&super::configured_base_url(cfg, PROVIDER_GOOGLE)?);
    let source = lang::resolve_source(req.from.as_deref(), &req.text);
    let mut body = json!({
        "q": req.text,
        "target": lang::map_lang(&req.to, "zh-CN"),
        "format": "text",
    });
    if let Some(from) = &source {
        body["source"] = json!(lang::map_lang(from, "zh-CN"));
    }
    let resp = transport
        .post_json_with_headers(&url, &[("X-Goog-Api-Key", api_key)], body)
        .await
        .map_err(|e| EngineError::from_transport(provider, &e))?;
    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(EngineError::from_http(
            provider,
            status,
            &truncate(&raw, 300),
            None,
        ));
    }
    let parsed: GoogleResponse =
        serde_json::from_str(&raw).map_err(|e| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: e.to_string(),
        })?;
    let Some(hit) = parsed.data.translations.into_iter().next() else {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "translations 为空".into(),
        });
    };
    let output = unescape_text(hit.translated_text.trim());
    if output.is_empty() {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "translatedText 为空".into(),
        });
    }
    Ok(TranslateResult {
        engine: provider.into(),
        output,
        input: req.text.clone(),
        detected_from: hit
            .detected_source_language
            .as_deref()
            .map(lang::short_code)
            .or_else(|| source.as_deref().map(lang::short_code)),
        target: lang::short_code(&req.to),
        duration_ms: 0,
        enrichment: None,
        enrichment_word: None,
        enrichment_pending: false,
    })
}

pub async fn test_connection(
    cfg: &ProviderConfig,
    api_key: &str,
    transport: &dyn Transport,
) -> Result<String, EngineError> {
    let req = TranslateRequest {
        text: "hi".into(),
        from: Some("en".into()),
        to: "zh".into(),
    };
    translate(cfg, api_key, &req, transport).await?;
    Ok(format!("{} 连接正常", provider_label(PROVIDER_GOOGLE)))
}

fn translate_url(base: &str) -> String {
    let base = base.trim_end_matches('/').split('?').next().unwrap_or(base);
    if base.contains("/language/translate/") {
        base.to_string()
    } else {
        format!("{base}/language/translate/v2")
    }
}

fn unescape_text(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
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
    use crate::services::transport::ReqwestTransport;

    fn cfg(base_url: &str) -> ProviderConfig {
        let mut c = ProviderConfig::builtin(PROVIDER_GOOGLE);
        c.base_url = Some(base_url.to_string());
        c.enabled = true;
        c
    }

    #[test]
    fn google_mock_uses_api_key_header() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/language/translate/v2")
            .match_header("x-goog-api-key", "test-key")
            .match_body(mockito::Matcher::PartialJson(json!({
                "q": "hello",
                "source": "en",
                "target": "zh-CN",
                "format": "text",
            })))
            .with_status(200)
            .with_body(
                r#"{"data":{"translations":[{"translatedText":"你好","detectedSourceLanguage":"en"}]}}"#,
            )
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "hello".into(),
                from: Some("en".into()),
                to: "zh".into(),
            };
            translate(
                &cfg(&format!("{}/language/translate/v2", server.url())),
                "test-key",
                &req,
                &ReqwestTransport::default(),
            )
            .await
        });
        mock.assert();
        let out = out.expect("Google 应成功");
        assert_eq!(out.output, "你好");
        assert_eq!(out.detected_from.as_deref(), Some("en"));
        assert_eq!(out.target, "zh");
        assert_eq!(out.engine, "Google Translate");
    }

    #[test]
    fn google_403_is_unauthorized() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/language/translate/v2")
            .with_status(403)
            .with_body(r#"{"error":{"message":"forbidden"}}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "hello".into(),
                from: None,
                to: "zh".into(),
            };
            translate(
                &cfg(&format!("{}/language/translate/v2", server.url())),
                "bad",
                &req,
                &ReqwestTransport::default(),
            )
            .await
        });
        mock.assert();
        assert!(matches!(out, Err(EngineError::Unauthorized { .. })));
    }

    #[test]
    fn google_unescapes_entities() {
        assert_eq!(unescape_text("A &amp; B"), "A & B");
    }
}
