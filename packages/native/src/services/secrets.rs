//! API Key 存储：应用数据目录下的 `keys.json`（tauri-plugin-store）。
//!
//! 不走 Keychain / Credential Manager，避免每次读写都弹系统密码。
//! WebView 永远拿不到密钥明文（D2），这里只暴露存在性与读写动作。

use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "keys.json";

fn entry_name(provider: &str) -> String {
    format!("{provider}_api_key")
}

fn extract_key(value: Option<Value>) -> Option<String> {
    match value {
        Some(Value::String(key)) if !key.trim().is_empty() => Some(key),
        _ => None,
    }
}

pub fn get_api_key(app: &AppHandle, provider: &str) -> Result<String, String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    extract_key(store.get(entry_name(provider)))
        .ok_or_else(|| format!("未配置 {provider} API Key"))
}

pub fn has_api_key(app: &AppHandle, provider: &str) -> bool {
    get_api_key(app, provider).is_ok()
}

pub fn set_api_key(app: &AppHandle, provider: &str, api_key: &str) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(entry_name(provider), Value::String(api_key.to_string()));
    store.save().map_err(|e| format!("密钥写入失败：{e}"))
}

pub fn delete_api_key(app: &AppHandle, provider: &str) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.delete(entry_name(provider));
        let _ = store.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_or_missing_is_not_a_key() {
        assert!(extract_key(None).is_none());
        assert!(extract_key(Some(json!(""))).is_none());
        assert!(extract_key(Some(json!("   "))).is_none());
        assert_eq!(extract_key(Some(json!("sk-abc"))).as_deref(), Some("sk-abc"));
    }
}
