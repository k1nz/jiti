//! 非密钥配置（settings.json via tauri-plugin-store，§6.2）。

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::providers::ProvidersConfig;
use crate::services::mistakes::MistakePreferences;

const PROVIDERS_KEY: &str = "providers";
const MISTAKES_KEY: &str = "mistakes";

pub fn load_provider_config(app: &AppHandle) -> Result<ProvidersConfig, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    match store.get(PROVIDERS_KEY) {
        Some(value) => serde_json::from_value(value)
            .map_err(|e| format!("providers 配置解析失败：{e}")),
        _ => Ok(ProvidersConfig::default()),
    }
}

pub fn save_provider_config(app: &AppHandle, config: &ProvidersConfig) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(config).map_err(|e| e.to_string())?;
    store.set(PROVIDERS_KEY, value);
    store.save().map_err(|e| e.to_string())
}

pub fn load_mistake_preferences(app: &AppHandle) -> Result<MistakePreferences, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    match store.get(MISTAKES_KEY) {
        Some(value) => serde_json::from_value::<MistakePreferences>(value)
            .map(|prefs| prefs.sanitized())
            .map_err(|e| format!("mistakes 配置解析失败：{e}")),
        None => Ok(MistakePreferences::default()),
    }
}

pub fn save_mistake_preferences(
    app: &AppHandle,
    prefs: &MistakePreferences,
) -> Result<MistakePreferences, String> {
    let prefs = prefs.clone().sanitized();
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(&prefs).map_err(|e| e.to_string())?;
    store.set(MISTAKES_KEY, value);
    store.save().map_err(|e| e.to_string())?;
    Ok(prefs)
}
