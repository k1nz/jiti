//! 引擎 Provider 层（§5）。
//!
//! 翻译适配器：DeepL、LLM、有道智云、Google Cloud Translation、Bing（Microsoft Translator）。
//! 已启用的翻译引擎并行请求，面板按条展示。所有引擎 HTTP 一律由 Rust `reqwest` 发出（D1）。

pub mod bing;
pub mod clean;
pub mod deepl;
pub mod enrich;
pub mod error;
pub mod google;
pub mod grammar;
pub mod lang;
pub mod llm;
pub mod plan;
pub mod review;
pub mod youdao;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::services;

pub use error::{EngineError, EngineErrorPayload};

pub const PROVIDER_DEEPL: &str = "deepl";
pub const PROVIDER_LLM: &str = "llm";
pub const PROVIDER_YOUDAO: &str = "youdao";
pub const PROVIDER_GOOGLE: &str = "google";
pub const PROVIDER_BING: &str = "bing";

const TRANSLATE_PROVIDERS: [&str; 5] = [
    PROVIDER_YOUDAO,
    PROVIDER_GOOGLE,
    PROVIDER_BING,
    PROVIDER_DEEPL,
    PROVIDER_LLM,
];

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
    /// Bing / Azure Translator 资源区域，例如 `eastasia`。空则不发送区域头。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
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
            region: None,
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
            PROVIDER_YOUDAO => Self {
                enabled: false,
                base_url: Some("https://openapi.youdao.com/api".into()),
                ..Default::default()
            },
            PROVIDER_GOOGLE => Self {
                enabled: false,
                base_url: Some("https://translation.googleapis.com/language/translate/v2".into()),
                ..Default::default()
            },
            PROVIDER_BING => Self {
                enabled: false,
                base_url: Some("https://api.cognitive.microsofttranslator.com".into()),
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
    #[serde(default)]
    pub google: ProviderConfig,
    #[serde(default)]
    pub bing: ProviderConfig,
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
            google: ProviderConfig::builtin(PROVIDER_GOOGLE),
            bing: ProviderConfig::builtin(PROVIDER_BING),
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
    pub has_secret: bool,
    pub kind: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub formality: Option<String>,
    pub region: Option<String>,
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
    /// 英语短词词卡；失败时省略，不影响译文。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<enrich::EnglishEnrichment>,
    /// 词卡对应的英语单词；加载态即可显示。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment_word: Option<String>,
    /// 后台正在拉词卡；完成后经 `translate://enriched` 清除。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enrichment_pending: bool,
}

/// 单个引擎的译文或错误。失败时 `output` 为空，`error` 有值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineTranslation {
    pub id: String,
    pub engine: String,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected_from: Option<String>,
    pub target: String,
    pub duration_ms: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<EngineErrorPayload>,
}

/// 一次翻译里所有已启用引擎的结果。词卡只挂在 `preferred_id` 那一条上。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TranslateOutcome {
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_id: Option<String>,
    pub results: Vec<EngineTranslation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<enrich::EnglishEnrichment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment_word: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub enrichment_pending: bool,
}

/// 已启用的翻译引擎并行请求。默认引擎排在最前，第一条成功的结果作为首选。
pub async fn run_translate(
    app: &AppHandle,
    request: TranslateRequest,
) -> Result<TranslateOutcome, EngineError> {
    let config =
        services::settings::load_provider_config(app).map_err(|e| EngineError::InvalidConfig {
            provider: "settings".into(),
            detail: e,
        })?;
    let ids = translate_targets(&config);
    if ids.is_empty() {
        return Err(EngineError::InvalidConfig {
            provider: "translate".into(),
            detail: "没有已启用的翻译引擎".into(),
        });
    }
    let futs = ids.iter().copied().map(|id| {
        let cfg = config_for(&config, id).clone();
        let api_key = services::secrets::get_api_key(app, id).ok();
        let app_secret = services::secrets::get_app_secret(app, id).ok();
        let request = request.clone();
        async move {
            (
                id,
                translate_one(id, &cfg, api_key, app_secret, request).await,
            )
        }
    });
    let finished = futures_util::future::join_all(futs).await;

    let mut results = Vec::with_capacity(finished.len());
    let mut preferred: Option<TranslateResult> = None;
    let mut preferred_id = None;
    for (id, outcome) in finished {
        match outcome {
            Ok(translated) => {
                if preferred.is_none() {
                    preferred_id = Some(id.to_string());
                    preferred = Some(translated.clone());
                }
                results.push(engine_ok(id, translated));
            }
            Err(err) => results.push(engine_err(id, err, &request.to)),
        }
    }

    let mut batch = TranslateOutcome {
        input: request.text,
        preferred_id,
        results,
        enrichment: None,
        enrichment_word: None,
        enrichment_pending: false,
    };
    if let Some(translated) = preferred {
        if let Some(word) =
            enrich::english_headword(&translated.target, &translated.input, &translated.output)
        {
            batch.enrichment_word = Some(word.clone());
            batch.enrichment_pending = true;
            let llm = llm_for_enrich(app, &config).map(|(cfg, key)| (cfg.clone(), key));
            enrich::spawn(
                app,
                translated.input.clone(),
                translated.output.clone(),
                word,
                llm,
            );
        }
    }
    Ok(batch)
}

fn llm_for_enrich<'a>(
    app: &AppHandle,
    config: &'a ProvidersConfig,
) -> Option<(&'a ProviderConfig, String)> {
    if !config.llm.enabled {
        return None;
    }
    let key = services::secrets::get_api_key(app, PROVIDER_LLM).ok()?;
    if key.trim().is_empty() {
        None
    } else {
        Some((&config.llm, key))
    }
}

async fn translate_one(
    id: &str,
    cfg: &ProviderConfig,
    api_key: Option<String>,
    app_secret: Option<String>,
    request: TranslateRequest,
) -> Result<TranslateResult, EngineError> {
    let Some(api_key) = api_key.filter(|key| !key.trim().is_empty()) else {
        return Err(EngineError::MissingKey {
            provider: provider_label(id).into(),
        });
    };
    if id == PROVIDER_YOUDAO && app_secret.as_deref().unwrap_or("").trim().is_empty() {
        return Err(EngineError::InvalidConfig {
            provider: provider_label(id).into(),
            detail: "缺少应用密钥".into(),
        });
    }
    translate_with(id, cfg, api_key, app_secret, request).await
}

/// 适配器级入口：测试连接与 mock 用例直接走这里，不依赖 secrets/AppHandle。
pub async fn translate_with(
    id: &str,
    cfg: &ProviderConfig,
    api_key: String,
    app_secret: Option<String>,
    request: TranslateRequest,
) -> Result<TranslateResult, EngineError> {
    let started = std::time::Instant::now();
    let transport = services::transport::shared();
    let mut translated = match id {
        PROVIDER_DEEPL => deepl::translate(cfg, &api_key, &request, &transport).await,
        PROVIDER_LLM => llm::translate(cfg, &api_key, &request, &transport).await,
        PROVIDER_YOUDAO => {
            youdao::translate(
                cfg,
                &api_key,
                app_secret.as_deref().unwrap_or(""),
                &request,
                &transport,
            )
            .await
        }
        PROVIDER_GOOGLE => google::translate(cfg, &api_key, &request, &transport).await,
        PROVIDER_BING => bing::translate(cfg, &api_key, &request, &transport).await,
        _ => Err(EngineError::InvalidConfig {
            provider: "unknown".into(),
            detail: format!("未注册的 Provider: {id}"),
        }),
    }?;
    let cleaned = clean::sanitize_translation_output(&translated.output);
    if !cleaned.is_empty() {
        translated.output = cleaned;
    }
    translated.duration_ms = started.elapsed().as_millis() as u32;
    Ok(translated)
}

/// 已启用引擎：默认引擎在前，其余按注册表顺序。
pub fn translate_targets(config: &ProvidersConfig) -> Vec<&'static str> {
    let mut ids = Vec::new();
    if let Some(id) = TRANSLATE_PROVIDERS
        .iter()
        .copied()
        .find(|id| *id == config.default_translate)
    {
        if config_for(config, id).enabled {
            ids.push(id);
        }
    }
    for id in TRANSLATE_PROVIDERS {
        if config_for(config, id).enabled && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// 默认引擎解析：配置的默认值 → 第一个已启用的适配器。
pub fn resolve_provider(config: &ProvidersConfig) -> &'static str {
    translate_targets(config)
        .into_iter()
        .next()
        .unwrap_or(PROVIDER_DEEPL)
}

pub fn config_for<'a>(config: &'a ProvidersConfig, id: &str) -> &'a ProviderConfig {
    match id {
        PROVIDER_DEEPL => &config.deepl,
        PROVIDER_LLM => &config.llm,
        PROVIDER_YOUDAO => &config.youdao,
        PROVIDER_GOOGLE => &config.google,
        PROVIDER_BING => &config.bing,
        _ => &config.llm,
    }
}

pub(crate) fn configured_base_url(cfg: &ProviderConfig, id: &str) -> Result<String, EngineError> {
    if let Some(url) = cfg
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(url.to_string());
    }
    ProviderConfig::builtin(id)
        .base_url
        .ok_or_else(|| EngineError::InvalidConfig {
            provider: provider_label(id).into(),
            detail: "baseUrl 未配置".into(),
        })
}

pub fn provider_label(id: &str) -> &'static str {
    match id {
        PROVIDER_DEEPL => "DeepL",
        PROVIDER_LLM => "LLM",
        PROVIDER_YOUDAO => "有道",
        PROVIDER_GOOGLE => "Google Translate",
        PROVIDER_BING => "Bing 翻译",
        _ => "unknown",
    }
}

fn engine_ok(id: &str, translated: TranslateResult) -> EngineTranslation {
    EngineTranslation {
        id: id.to_string(),
        engine: translated.engine,
        output: translated.output,
        detected_from: translated.detected_from,
        target: translated.target,
        duration_ms: translated.duration_ms,
        error: None,
    }
}

fn engine_err(id: &str, err: EngineError, target: &str) -> EngineTranslation {
    EngineTranslation {
        id: id.to_string(),
        engine: provider_label(id).into(),
        output: String::new(),
        detected_from: None,
        target: target.to_string(),
        duration_ms: 0,
        error: Some(err.payload()),
    }
}

pub fn snapshot(app: &AppHandle) -> Result<ProvidersSnapshot, String> {
    let config = services::settings::load_provider_config(app)?;
    let has_key = |id: &str| services::secrets::has_api_key(app, id);
    let has_secret = |id: &str| services::secrets::has_app_secret(app, id);
    Ok(ProvidersSnapshot {
        default_translate: config.default_translate.clone(),
        write_history: config.write_history,
        providers: vec![
            provider_view(
                PROVIDER_YOUDAO,
                "有道智云",
                &config.youdao,
                has_key(PROVIDER_YOUDAO),
                has_secret(PROVIDER_YOUDAO),
            ),
            provider_view(
                PROVIDER_GOOGLE,
                "Google Translate",
                &config.google,
                has_key(PROVIDER_GOOGLE),
                false,
            ),
            provider_view(
                PROVIDER_BING,
                "Bing 翻译",
                &config.bing,
                has_key(PROVIDER_BING),
                false,
            ),
            provider_view(
                PROVIDER_DEEPL,
                "DeepL",
                &config.deepl,
                has_key(PROVIDER_DEEPL),
                false,
            ),
            provider_view(
                PROVIDER_LLM,
                "LLM（OpenAI 兼容）",
                &config.llm,
                has_key(PROVIDER_LLM),
                false,
            ),
        ],
    })
}

fn provider_view(
    id: &str,
    label: &str,
    cfg: &ProviderConfig,
    has_key: bool,
    has_secret: bool,
) -> ProviderView {
    ProviderView {
        id: id.to_string(),
        label: label.to_string(),
        enabled: cfg.enabled,
        has_key,
        has_secret,
        kind: cfg.kind.clone(),
        base_url: cfg.base_url.clone(),
        model: cfg.model.clone(),
        temperature: cfg.temperature,
        max_tokens: cfg.max_tokens,
        formality: cfg.formality.clone(),
        region: cfg.region.clone(),
        testable: true,
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
            None,
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
    fn enabled_providers_lead_with_the_default() {
        let mut config = ProvidersConfig::default();
        config.deepl.enabled = true;
        config.youdao.enabled = true;
        config.google.enabled = true;
        config.bing.enabled = false;
        config.default_translate = PROVIDER_GOOGLE.into();
        assert_eq!(
            translate_targets(&config),
            vec![PROVIDER_GOOGLE, PROVIDER_YOUDAO, PROVIDER_DEEPL]
        );
    }

    #[test]
    fn provider_view_never_contains_key() {
        let cfg = ProviderConfig::builtin(PROVIDER_LLM);
        let v = provider_view(PROVIDER_LLM, "LLM", &cfg, true, false);
        assert!(!serde_json::to_string(&v).unwrap().contains("secret"));
    }
}
