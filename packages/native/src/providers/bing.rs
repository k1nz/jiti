//! Bing 翻译，走 Microsoft Translator v3。
//! 订阅密钥放在 `Ocp-Apim-Subscription-Key`；区域来自设置里的 region。

use serde::Deserialize;
use serde_json::json;

use crate::services::transport::Transport;

use super::{
    lang, provider_label, EngineError, ProviderConfig, TranslateRequest, TranslateResult,
    PROVIDER_BING,
};

#[derive(Debug, Deserialize)]
struct BingItem {
    #[serde(rename = "detectedLanguage")]
    detected_language: Option<BingDetected>,
    translations: Vec<BingTranslation>,
}

#[derive(Debug, Deserialize)]
struct BingDetected {
    language: String,
}

#[derive(Debug, Deserialize)]
struct BingTranslation {
    text: String,
}

pub async fn translate(
    cfg: &ProviderConfig,
    api_key: &str,
    req: &TranslateRequest,
    transport: &dyn Transport,
) -> Result<TranslateResult, EngineError> {
    let provider = provider_label(PROVIDER_BING);
    let source = lang::resolve_source(req.from.as_deref(), &req.text);
    let url = translate_url(
        &super::configured_base_url(cfg, PROVIDER_BING)?,
        source.as_deref(),
        &req.to,
    );
    let mut headers = vec![("Ocp-Apim-Subscription-Key", api_key)];
    let region = cfg
        .region
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(region) = region {
        headers.push(("Ocp-Apim-Subscription-Region", region));
    }
    let resp = transport
        .post_json_with_headers(&url, &headers, json!([{ "Text": req.text }]))
        .await
        .map_err(|e| EngineError::from_transport(provider, &e))?;
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let raw = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(EngineError::from_http(
            provider,
            status,
            &truncate(&raw, 300),
            retry_after,
        ));
    }
    let parsed: Vec<BingItem> =
        serde_json::from_str(&raw).map_err(|e| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: e.to_string(),
        })?;
    let Some(item) = parsed.into_iter().next() else {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "响应为空".into(),
        });
    };
    let detected = item
        .detected_language
        .map(|lang| lang::short_code(&lang.language))
        .or_else(|| source.as_deref().map(lang::short_code));
    let Some(hit) = item.translations.into_iter().next() else {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "translations 为空".into(),
        });
    };
    let output = hit.text.trim().to_string();
    if output.is_empty() {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "text 为空".into(),
        });
    }
    Ok(TranslateResult {
        engine: provider.into(),
        output,
        input: req.text.clone(),
        detected_from: detected,
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
    Ok(format!("{} 连接正常", provider_label(PROVIDER_BING)))
}

fn translate_url(base: &str, from: Option<&str>, to: &str) -> String {
    let base = base.trim_end_matches('/').split('?').next().unwrap_or(base);
    let root = if base.ends_with("/translate") {
        base.to_string()
    } else {
        format!("{base}/translate")
    };
    let mut url = format!(
        "{root}?api-version=3.0&to={}",
        lang::map_lang(to, "zh-Hans")
    );
    if let Some(from) = from {
        url.push_str("&from=");
        url.push_str(&lang::map_lang(from, "zh-Hans"));
    }
    url
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
        let mut c = ProviderConfig::builtin(PROVIDER_BING);
        c.base_url = Some(base_url.to_string());
        c.region = Some("eastasia".into());
        c.enabled = true;
        c
    }

    #[test]
    fn bing_mock_sends_region_and_key() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/translate")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("api-version".into(), "3.0".into()),
                mockito::Matcher::UrlEncoded("from".into(), "en".into()),
                mockito::Matcher::UrlEncoded("to".into(), "zh-Hans".into()),
            ]))
            .match_header("ocp-apim-subscription-key", "test-key")
            .match_header("ocp-apim-subscription-region", "eastasia")
            .with_status(200)
            .with_body(
                r#"[{"detectedLanguage":{"language":"en","score":1},"translations":[{"text":"你好","to":"zh-Hans"}]}]"#,
            )
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "hello".into(),
                from: Some("en".into()),
                to: "zh".into(),
            };
            translate(
                &cfg(&server.url()),
                "test-key",
                &req,
                &ReqwestTransport::default(),
            )
            .await
        });
        mock.assert();
        let out = out.expect("Bing 应成功");
        assert_eq!(out.output, "你好");
        assert_eq!(out.detected_from.as_deref(), Some("en"));
        assert_eq!(out.target, "zh");
        assert_eq!(out.engine, "Bing 翻译");
    }

    #[test]
    fn bing_401_is_unauthorized() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/translate")
            .match_query(mockito::Matcher::Any)
            .with_status(401)
            .with_body(r#"{"error":{"message":"unauthorized"}}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "hello".into(),
                from: None,
                to: "zh".into(),
            };
            translate(
                &cfg(&server.url()),
                "bad",
                &req,
                &ReqwestTransport::default(),
            )
            .await
        });
        mock.assert();
        assert!(matches!(out, Err(EngineError::Unauthorized { .. })));
    }
}
