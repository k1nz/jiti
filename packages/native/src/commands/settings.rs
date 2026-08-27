//! 设置命令：tauri-plugin-store 落盘 `settings.json`。
//! API Key 走 `keys.json`（§6.2），普通配置走本命令。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

/// settings.json 里的值：JSON 语义的封闭子集（形状与 `serde_json::Value` 同构）。
///
/// 用 `#[derive(Type)]` 生成 specta 类型：derive 内部按 TypeId 缓存递归定义，
/// 因此自递归安全；不能用 `serde_json::Value` 直接进命令签名 —— specta 对它的
/// legacy 实现是 `inline` 非缓存自递归，导致 bindings 导出时栈溢出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum SettingsValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<SettingsValue>),
    Object(Vec<(String, SettingsValue)>),
}

impl From<Value> for SettingsValue {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => SettingsValue::Null,
            Value::Bool(b) => SettingsValue::Bool(b),
            Value::Number(n) => SettingsValue::Number(n.as_f64().unwrap_or(f64::NAN)),
            Value::String(s) => SettingsValue::String(s),
            Value::Array(items) => {
                SettingsValue::Array(items.into_iter().map(SettingsValue::from).collect())
            }
            Value::Object(map) => SettingsValue::Object(
                map.into_iter()
                    .map(|(k, v)| (k, SettingsValue::from(v)))
                    .collect(),
            ),
        }
    }
}

impl From<SettingsValue> for Value {
    fn from(v: SettingsValue) -> Self {
        match v {
            SettingsValue::Null => Value::Null,
            SettingsValue::Bool(b) => Value::Bool(b),
            SettingsValue::Number(n) => serde_json::Number::from_f64(n)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            SettingsValue::String(s) => Value::String(s),
            SettingsValue::Array(items) => {
                Value::Array(items.into_iter().map(Value::from).collect())
            }
            SettingsValue::Object(entries) => {
                let map = entries
                    .into_iter()
                    .map(|(k, v)| (k, Value::from(v)))
                    .collect();
                Value::Object(map)
            }
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn settings_get(app: AppHandle, key: String) -> Result<Option<SettingsValue>, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    Ok(store.get(&key).map(SettingsValue::from))
}

#[tauri::command]
#[specta::specta]
pub fn settings_set(app: AppHandle, key: String, value: SettingsValue) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(key, Value::from(value));
    store.save().map_err(|e| e.to_string())
}
