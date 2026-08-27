//! 通用偏好：界面语言、主题（settings.json `prefs` 键）。
//! 开机自启状态来自系统注册，不写进 JSON。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tauri_specta::Event;

use super::autostart;
use super::tray;

const PREFS_KEY: &str = "prefs";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum UiLocale {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en-US")]
    EnUs,
}

impl Default for UiLocale {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThemePref {
    System,
    Light,
    Dark,
}

impl Default for ThemePref {
    fn default() -> Self {
        Self::System
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    pub locale: UiLocale,
    pub theme: ThemePref,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            locale: UiLocale::System,
            theme: ThemePref::System,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AutostartStatus {
    pub enabled: bool,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesSnapshot {
    pub locale: UiLocale,
    pub theme: ThemePref,
    pub autostart: AutostartStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesPatch {
    #[serde(default)]
    pub locale: Option<UiLocale>,
    #[serde(default)]
    pub theme: Option<ThemePref>,
    #[serde(default)]
    pub autostart: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "preferences://changed")]
pub struct PreferencesChangedEvent {
    pub snapshot: PreferencesSnapshot,
}

pub fn prefs_from_value(value: Option<&Value>) -> Prefs {
    let Some(Value::Object(map)) = value else {
        return Prefs::default();
    };
    Prefs {
        locale: match map.get("locale").and_then(Value::as_str) {
            Some("zh-CN") => UiLocale::ZhCn,
            Some("en-US") => UiLocale::EnUs,
            _ => UiLocale::System,
        },
        theme: match map.get("theme").and_then(Value::as_str) {
            Some("light") => ThemePref::Light,
            Some("dark") => ThemePref::Dark,
            _ => ThemePref::System,
        },
    }
}

pub fn prefs_to_value(prefs: Prefs) -> Value {
    json!({
        "locale": prefs.locale,
        "theme": prefs.theme,
    })
}

pub fn load_prefs(app: &AppHandle) -> Prefs {
    let Ok(store) = app.store("settings.json") else {
        return Prefs::default();
    };
    prefs_from_value(store.get(PREFS_KEY).as_ref())
}

pub fn save_prefs(app: &AppHandle, prefs: Prefs) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(PREFS_KEY, prefs_to_value(prefs));
    store.save().map_err(|e| e.to_string())
}

pub fn snapshot(app: &AppHandle) -> PreferencesSnapshot {
    let prefs = load_prefs(app);
    PreferencesSnapshot {
        locale: prefs.locale,
        theme: prefs.theme,
        autostart: autostart::status(app),
    }
}

pub fn update(app: &AppHandle, patch: PreferencesPatch) -> Result<PreferencesSnapshot, String> {
    let mut prefs = load_prefs(app);
    if let Some(locale) = patch.locale {
        prefs.locale = locale;
    }
    if let Some(theme) = patch.theme {
        prefs.theme = theme;
    }
    if let Some(enabled) = patch.autostart {
        autostart::set_enabled(app, enabled)?;
    }
    save_prefs(app, prefs)?;
    let snap = snapshot(app);
    tray::refresh_labels(app, prefs.locale);
    let _ = PreferencesChangedEvent {
        snapshot: snap.clone(),
    }
    .emit(app);
    Ok(snap)
}

pub fn resolved_locale(pref: UiLocale) -> UiLocale {
    match pref {
        UiLocale::System => detect_system_locale(),
        other => other,
    }
}

pub fn detect_system_locale() -> UiLocale {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = std::env::var(key) {
            if val.is_empty() || val == "C" || val == "POSIX" {
                continue;
            }
            return parse_locale_tag(&val);
        }
    }
    UiLocale::ZhCn
}

pub fn parse_locale_tag(tag: &str) -> UiLocale {
    let lower = tag.to_ascii_lowercase();
    if lower.starts_with("zh") {
        UiLocale::ZhCn
    } else {
        UiLocale::EnUs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_prefs_use_system_defaults() {
        assert_eq!(prefs_from_value(None), Prefs::default());
        assert_eq!(
            prefs_from_value(Some(&json!({"locale": "nope", "theme": 1}))),
            Prefs::default()
        );
    }

    #[test]
    fn known_prefs_round_trip() {
        let prefs = Prefs {
            locale: UiLocale::EnUs,
            theme: ThemePref::Dark,
        };
        let value = prefs_to_value(prefs);
        assert_eq!(prefs_from_value(Some(&value)), prefs);
    }

    #[test]
    fn parse_locale_tags() {
        assert_eq!(parse_locale_tag("zh_CN.UTF-8"), UiLocale::ZhCn);
        assert_eq!(parse_locale_tag("zh-Hans"), UiLocale::ZhCn);
        assert_eq!(parse_locale_tag("en_US"), UiLocale::EnUs);
        assert_eq!(parse_locale_tag("ja_JP"), UiLocale::EnUs);
    }

    #[test]
    fn resolved_locale_keeps_manual_choice() {
        assert_eq!(resolved_locale(UiLocale::EnUs), UiLocale::EnUs);
        assert_eq!(resolved_locale(UiLocale::ZhCn), UiLocale::ZhCn);
    }
}
