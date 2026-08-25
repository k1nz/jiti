//! 非密钥配置（settings.json via tauri-plugin-store，§6.2）。

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::providers::ProvidersConfig;

const PROVIDERS_KEY: &str = "providers";

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
