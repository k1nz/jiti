//! 引擎 Provider 层（§5）。
//!
//! M1 实现 DeepL 与 LLM（OpenAI 兼容）两个适配器；有道仅保留注册表入口。
//! 所有引擎 HTTP 一律由 Rust `reqwest` 发出（D1），WebView 不对引擎域名 fetch。

pub mod deepl;
pub mod error;
pub mod llm;
pub mod youdao;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::services;

pub use error::{EngineError, EngineErrorPayload};

pub const PROVIDER_DEEPL: &str = "deepl";
pub const PROVIDER_LLM: &str = "llm";
pub const PROVIDER_YOUDAO: &str = "youdao";

/// 非密钥的 Provider 配置（settings.json，§6.2：密钥在 keys.json 里）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formality: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: None,
            model: None,
            kind: None,
            temperature: None,
            max_tokens: None,
            formality: None,
        }
    }
}

impl ProviderConfig {
    pub fn builtin(id: &str) -> Self {
        match id {
            PROVIDER_DEEPL => Self {
                enabled: true,
                base_url: Some("https://api-free.deepl.com/v2".into()),
                formality: Some("default".into()),
                ..Default::default()
            },
            PROVIDER_LLM => Self {
                enabled: false,
                base_url: Some("https://api.openai.com/v1".into()),
                model: Some("gpt-4o-mini".into()),
                kind: Some("openai".into()),
                temperature: Some(0.3),
                max_tokens: Some(1024),
                ..Default::default()
            },
            _ => Self::default(),
        }
    }
}

/// settings.json → `providers` 键的整体形状（§5.1 精简到 M1 需要的字段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersConfig {
    #[serde(default)]
    pub deepl: ProviderConfig,
    #[serde(default)]
    pub llm: ProviderConfig,
    #[serde(default)]
    pub youdao: ProviderConfig,
    #[serde(default = "default_translate")]
    pub default_translate: String,
    #[serde(default = "default_true")]
    pub write_history: bool,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            deepl: ProviderConfig::builtin(PROVIDER_DEEPL),
            llm: ProviderConfig::builtin(PROVIDER_LLM),
            youdao: ProviderConfig::builtin(PROVIDER_YOUDAO),
            default_translate: default_translate(),
            write_history: true,
        }
    }
}

fn default_translate() -> String {
    PROVIDER_DEEPL.into()
}

fn default_true() -> bool {
    true
}

/// WebView 可见的 Provider 状态（D2：任何时候都不含密钥明文）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProviderView {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub has_key: bool,
    pub kind: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub formality: Option<String>,
    pub testable: bool,
}

/// 设置页一次拉取的非密钥配置快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersSnapshot {
    pub default_translate: String,
    pub write_history: bool,
    pub providers: Vec<ProviderView>,
}

/// 翻译请求：§5.2 统一进出参 `{text, from?, to}`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TranslateRequest {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub to: String,
}

/// 翻译结果：§5.4 标准化 Schema。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResult {
    pub engine: String,
    pub output: String,
    pub input: String,
    pub detected_from: Option<String>,
    pub target: String,
    pub duration_ms: u32,
}

/// 完整链路：设置里选默认引擎 → keys.json 取 Key → 对应适配器。
pub async fn run_translate(
    app: &AppHandle,
    request: TranslateRequest,
) -> Result<TranslateResult, EngineError> {
    let config = services::settings::load_provider_config(app)
        .map_err(|e| EngineError::InvalidConfig {
            provider: "settings".into(),
            detail: e,
        })?;
    let id = resolve_provider(&config);
    let provider_config = config_for(&config, id);
    let key = services::secrets::get_api_key(app, id).map_err(|_| EngineError::MissingKey {
        provider: provider_label(id).into(),
    })?;
    translate_with(id, provider_config, key, request).await
}

/// 适配器级入口：测试连接与 mock 用例直接走这里，不依赖 secrets/AppHandle。
pub async fn translate_with(
    id: &str,
    cfg: &ProviderConfig,
    api_key: String,
    request: TranslateRequest,
) -> Result<TranslateResult, EngineError> {
    let started = std::time::Instant::now();
    let transport = services::transport::shared();
    let mut translated = match id {
        PROVIDER_DEEPL => deepl::translate(cfg, &api_key, &request, &transport).await,
        PROVIDER_LLM => llm::translate(cfg, &api_key, &request, &transport).await,
        PROVIDER_YOUDAO => Err(EngineError::InvalidConfig {
            provider: provider_label(PROVIDER_YOUDAO).into(),
            detail: "有道适配器 M1 仅保留注册表入口".into(),
        }),
        _ => Err(EngineError::InvalidConfig {
            provider: "unknown".into(),
            detail: format!("未注册的 Provider: {id}"),
        }),
    }?;
    translated.duration_ms = started.elapsed().as_millis() as u32;
    Ok(translated)
}

/// 默认引擎解析：配置的默认值 → 第一个已启用且有配置的适配器。
pub fn resolve_provider(config: &ProvidersConfig) -> &'static str {
    let ordered = [PROVIDER_DEEPL, PROVIDER_LLM, PROVIDER_YOUDAO];
    let default = ordered
        .iter()
        .find(|id| **id == config.default_translate)
        .copied();
    if let Some(id) = default {
        let cfg = config_for(config, id);
        if cfg.enabled {
            return id;
        }
    }
    ordered
        .iter()
        .find(|id| config_for(config, id).enabled)
        .copied()
        .unwrap_or(PROVIDER_DEEPL)
}

pub fn config_for<'a>(config: &'a ProvidersConfig, id: &str) -> &'a ProviderConfig {
    match id {
        PROVIDER_DEEPL => &config.deepl,
        PROVIDER_LLM => &config.llm,
        PROVIDER_YOUDAO => &config.youdao,
        _ => &config.llm,
    }
}

pub fn provider_label(id: &str) -> &'static str {
    match id {
        PROVIDER_DEEPL => "DeepL",
        PROVIDER_LLM => "LLM",
        PROVIDER_YOUDAO => "有道",
        _ => "unknown",
    }
}

pub fn snapshot(app: &AppHandle) -> Result<ProvidersSnapshot, String> {
    let config = services::settings::load_provider_config(app)?;
    let has_key = |id: &str| services::secrets::has_api_key(app, id);
    Ok(ProvidersSnapshot {
        default_translate: config.default_translate.clone(),
        write_history: config.write_history,
        providers: vec![
            provider_view(
                PROVIDER_DEEPL,
                "DeepL",
                &config.deepl,
                has_key(PROVIDER_DEEPL),
            ),
            provider_view(
                PROVIDER_LLM,
                "LLM（OpenAI 兼容）",
                &config.llm,
                has_key(PROVIDER_LLM),
            ),
            provider_view(
                PROVIDER_YOUDAO,
                "有道智云",
                &config.youdao,
                has_key(PROVIDER_YOUDAO),
            ),
        ],
    })
}

fn provider_view(id: &str, label: &str, cfg: &ProviderConfig, has_key: bool) -> ProviderView {
    ProviderView {
        id: id.to_string(),
        label: label.to_string(),
        enabled: cfg.enabled,
        has_key,
        kind: cfg.kind.clone(),
        base_url: cfg.base_url.clone(),
        model: cfg.model.clone(),
        temperature: cfg.temperature,
        max_tokens: cfg.max_tokens,
        formality: cfg.formality.clone(),
        testable: id != PROVIDER_YOUDAO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_provider_is_rejected_before_http() {
        let cfg = ProviderConfig::default();
        let out = futures::executor::block_on(translate_with(
            "unknown",
            &cfg,
            "k".into(),
            TranslateRequest {
                text: "hello".into(),
                from: None,
                to: "zh".into(),
            },
        ));
        assert!(matches!(out, Err(EngineError::InvalidConfig { .. })));
    }

    #[test]
    fn default_falls_back_to_first_enabled_provider() {
        let mut config = ProvidersConfig::default();
        config.deepl.enabled = false;
        config.llm.enabled = true;
        assert_eq!(resolve_provider(&config), PROVIDER_LLM);
    }

    #[test]
    fn provider_view_never_contains_key() {
        let cfg = ProviderConfig::builtin(PROVIDER_LLM);
        let v = provider_view(PROVIDER_LLM, "LLM", &cfg, true);
        assert!(!serde_json::to_string(&v).unwrap().contains("secret"));
    }
}
