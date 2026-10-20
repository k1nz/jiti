//! 有道智云文本翻译（v3 签名：appKey + appSecret）。

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::services::transport::Transport;

use super::{
    lang, provider_label, EngineError, ProviderConfig, TranslateRequest, TranslateResult,
    PROVIDER_YOUDAO,
};

pub async fn translate(
    cfg: &ProviderConfig,
    app_key: &str,
    app_secret: &str,
    req: &TranslateRequest,
    transport: &dyn Transport,
) -> Result<TranslateResult, EngineError> {
    let provider = provider_label(PROVIDER_YOUDAO);
    if app_key.trim().is_empty() {
        return Err(EngineError::MissingKey {
            provider: provider.into(),
        });
    }
    if app_secret.trim().is_empty() {
        return Err(EngineError::InvalidConfig {
            provider: provider.into(),
            detail: "缺少应用密钥".into(),
        });
    }
    let url = super::configured_base_url(cfg, PROVIDER_YOUDAO)?;
    let from =
        lang::resolve_source(req.from.as_deref(), &req.text).unwrap_or_else(|| "auto".into());
    let from_code = lang::map_lang(&from, "zh-CHS");
    let to_code = lang::map_lang(&req.to, "zh-CHS");
    let (salt, curtime) = fresh_salt();
    let sign = sign_v3(app_key, &req.text, &salt, &curtime, app_secret);
    let body = form_body(&[
        ("q", req.text.as_str()),
        ("from", from_code.as_str()),
        ("to", to_code.as_str()),
        ("appKey", app_key),
        ("salt", salt.as_str()),
        ("sign", sign.as_str()),
        ("signType", "v3"),
        ("curtime", curtime.as_str()),
    ]);
    let resp = transport
        .post_bytes(
            &url,
            &[("Content-Type", "application/x-www-form-urlencoded")],
            body,
        )
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
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| EngineError::InvalidResponse {
        provider: provider.into(),
        detail: e.to_string(),
    })?;
    let code = error_code(&parsed);
    if code != "0" {
        return Err(classify_youdao(provider, &code));
    }
    let output = parsed["translation"]
        .as_array()
        .and_then(|items| items.iter().filter_map(|v| v.as_str()).next())
        .unwrap_or("")
        .trim()
        .to_string();
    if output.is_empty() {
        return Err(EngineError::InvalidResponse {
            provider: provider.into(),
            detail: "translation 为空".into(),
        });
    }
    let detected = parsed["l"]
        .as_str()
        .and_then(|pair| pair.split('2').next())
        .map(lang::short_code)
        .or_else(|| {
            if from.eq_ignore_ascii_case("auto") {
                None
            } else {
                Some(lang::short_code(&from))
            }
        });
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
    app_key: &str,
    app_secret: &str,
    transport: &dyn Transport,
) -> Result<String, EngineError> {
    let req = TranslateRequest {
        text: "hi".into(),
        from: Some("en".into()),
        to: "zh".into(),
    };
    translate(cfg, app_key, app_secret, &req, transport).await?;
    Ok(format!("{} 连接正常", provider_label(PROVIDER_YOUDAO)))
}

pub fn sign_v3(app_key: &str, q: &str, salt: &str, curtime: &str, app_secret: &str) -> String {
    let input = format!("{app_key}{}{salt}{curtime}{app_secret}", truncate_query(q));
    let digest = Sha256::digest(input.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 有道 v3：长度按 Unicode 标量计，超过 20 时取前 10 + 长度 + 后 10。
pub fn truncate_query(q: &str) -> String {
    let chars: Vec<char> = q.chars().collect();
    let len = chars.len();
    if len <= 20 {
        return q.to_string();
    }
    let head: String = chars.iter().take(10).collect();
    let tail: String = chars.iter().skip(len - 10).collect();
    format!("{head}{len}{tail}")
}

fn fresh_salt() -> (String, String) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (format!("{:x}", now.as_nanos()), now.as_secs().to_string())
}

fn error_code(value: &Value) -> String {
    match &value["errorCode"] {
        Value::String(code) => code.clone(),
        Value::Number(code) => code.to_string(),
        _ => String::new(),
    }
}

fn classify_youdao(provider: &str, code: &str) -> EngineError {
    let detail = format!("errorCode {code}");
    match code {
        "108" | "110" | "111" | "202" | "203" => EngineError::Unauthorized {
            provider: provider.into(),
            detail,
        },
        "401" | "411" => EngineError::RateLimited {
            provider: provider.into(),
            retry_after: None,
            detail,
        },
        "101" | "102" | "103" | "104" | "105" | "106" | "113" | "201" | "206" | "207" => {
            EngineError::BadRequest {
                provider: provider.into(),
                detail,
            }
        }
        _ => EngineError::Upstream {
            provider: provider.into(),
            status: 200,
            detail,
        },
    }
}

fn form_body(fields: &[(&str, &str)]) -> Vec<u8> {
    let mut body = String::new();
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            body.push('&');
        }
        body.push_str(&urlencoded(key));
        body.push('=');
        body.push_str(&urlencoded(value));
    }
    body.into_bytes()
}

fn urlencoded(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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
        let mut c = ProviderConfig::builtin(PROVIDER_YOUDAO);
        c.base_url = Some(base_url.to_string());
        c.enabled = true;
        c
    }

    #[test]
    fn v3_sign_matches_sha256_of_truncated_query() {
        assert_eq!(
            truncate_query("abcdefghij1234567890XYZ"),
            "abcdefghij234567890XYZ"
        );
        assert_eq!(truncate_query("hello"), "hello");
        assert_eq!(
            sign_v3("appKey", "hello", "salt", "1600000000", "secret"),
            "b8015a3369e256107f0448bd2535db51edfc66b563c73cd57d7516a0aabcfd48"
        );
    }

    #[test]
    fn youdao_mock_returns_translation() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api")
            .match_body(mockito::Matcher::Regex(
                r"q=hello&from=en&to=zh-CHS&appKey=test-key&salt=[0-9a-fA-F]+&sign=[0-9a-f]{64}&signType=v3&curtime=[0-9]+".into(),
            ))
            .with_status(200)
            .with_body(r#"{"errorCode":"0","translation":["你好"],"l":"en2zh-CHS"}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "hello".into(),
                from: Some("en".into()),
                to: "zh".into(),
            };
            translate(
                &cfg(&format!("{}/api", server.url())),
                "test-key",
                "secret",
                &req,
                &ReqwestTransport::default(),
            )
            .await
        });
        mock.assert();
        let out = out.expect("有道应成功");
        assert_eq!(out.output, "你好");
        assert_eq!(out.detected_from.as_deref(), Some("en"));
        assert_eq!(out.target, "zh");
        assert_eq!(out.engine, "有道");
    }

    #[test]
    fn youdao_error_code_is_unauthorized_when_sign_fails() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/api")
            .with_status(200)
            .with_body(r#"{"errorCode":"202"}"#)
            .create();
        let out = tauri::async_runtime::block_on(async {
            let req = TranslateRequest {
                text: "hello".into(),
                from: Some("en".into()),
                to: "zh".into(),
            };
            translate(
                &cfg(&format!("{}/api", server.url())),
                "test-key",
                "secret",
                &req,
                &ReqwestTransport::default(),
            )
            .await
        });
        mock.assert();
        assert!(matches!(out, Err(EngineError::Unauthorized { .. })));
    }
}
