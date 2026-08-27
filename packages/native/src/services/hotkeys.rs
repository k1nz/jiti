//! 全局热键（tauri-plugin-global-shortcut，Carbon/RegisterHotKey 底层）。
//!
//! 默认（§7.3，均可改）：
//! - macOS：⌥⌘T 翻译 / ⌥⌘G 语法 / ⌥⌘Space 面板
//! - Windows：Ctrl+Shift+T 翻译 / Ctrl+Alt+G 语法 / Ctrl+Alt+Space 面板
//!
//! 配置落 `settings.json` 的 `hotkeys` 键；变更即 Unregister+Register，冲突返回错误。

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tauri_plugin_store::StoreExt;

use super::panel::Mode;
use super::permissions;

const STORE_KEY: &str = "hotkeys";

static REGISTERED: LazyLock<Mutex<HashMap<u32, Mode>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyId {
    Translate,
    Grammar,
    Panel,
}

impl HotkeyId {
    pub fn all() -> [Self; 3] {
        [Self::Translate, Self::Grammar, Self::Panel]
    }

    pub fn mode(self) -> Mode {
        match self {
            Self::Translate => Mode::Translate,
            Self::Grammar => Mode::Grammar,
            Self::Panel => Mode::Panel,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Translate => "翻译",
            Self::Grammar => "语法",
            Self::Panel => "面板",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeysConfig {
    #[serde(default = "default_translate_accel")]
    pub translate: String,
    #[serde(default = "default_grammar_accel")]
    pub grammar: String,
    #[serde(default = "default_panel_accel")]
    pub panel: String,
}

fn default_translate_accel() -> String {
    defaults_for(permissions::platform()).translate
}

fn default_grammar_accel() -> String {
    defaults_for(permissions::platform()).grammar
}

fn default_panel_accel() -> String {
    defaults_for(permissions::platform()).panel
}

impl HotkeysConfig {
    fn get(&self, id: HotkeyId) -> &str {
        match id {
            HotkeyId::Translate => &self.translate,
            HotkeyId::Grammar => &self.grammar,
            HotkeyId::Panel => &self.panel,
        }
    }

    fn set(&mut self, id: HotkeyId, value: String) {
        match id {
            HotkeyId::Translate => self.translate = value,
            HotkeyId::Grammar => self.grammar = value,
            HotkeyId::Panel => self.panel = value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyBinding {
    pub id: HotkeyId,
    pub title: String,
    pub accelerator: String,
    pub display: String,
    pub is_default: bool,
    pub registered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HotkeysSnapshot {
    pub platform: String,
    pub bindings: Vec<HotkeyBinding>,
}

pub fn defaults_for(platform: &str) -> HotkeysConfig {
    if platform == "macos" {
        HotkeysConfig {
            translate: "alt+super+KeyT".into(),
            grammar: "alt+super+KeyG".into(),
            panel: "alt+super+Space".into(),
        }
    } else {
        HotkeysConfig {
            translate: "shift+control+KeyT".into(),
            grammar: "control+alt+KeyG".into(),
            panel: "control+alt+Space".into(),
        }
    }
}

pub fn defaults() -> HotkeysConfig {
    defaults_for(permissions::platform())
}

pub fn parse_accelerator(s: &str) -> Result<Shortcut, String> {
    let shortcut = Shortcut::from_str(s.trim()).map_err(|e| format!("无法识别快捷键：{e}"))?;
    if shortcut.key == Code::Escape {
        return Err("Esc 用于隐藏面板，不能设为全局热键".into());
    }
    let mods = Modifiers::SHIFT | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER;
    if !shortcut.mods.intersects(mods) {
        return Err("快捷键至少需要一个修饰键（Ctrl / Alt / Shift / ⌘）".into());
    }
    Ok(shortcut)
}

pub fn display_accelerator(accel: &str, platform: &str) -> String {
    match parse_accelerator(accel) {
        Ok(shortcut) => format_shortcut(&shortcut, platform),
        Err(_) => accel.to_string(),
    }
}

fn format_shortcut(shortcut: &Shortcut, platform: &str) -> String {
    let key = key_label(shortcut.key);
    if platform == "macos" {
        let mut s = String::new();
        if shortcut.mods.contains(Modifiers::CONTROL) {
            s.push('⌃');
        }
        if shortcut.mods.contains(Modifiers::ALT) {
            s.push('⌥');
        }
        if shortcut.mods.contains(Modifiers::SHIFT) {
            s.push('⇧');
        }
        if shortcut.mods.contains(Modifiers::SUPER) {
            s.push('⌘');
        }
        s.push_str(&key);
        s
    } else {
        let mut parts = Vec::new();
        if shortcut.mods.contains(Modifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if shortcut.mods.contains(Modifiers::SHIFT) {
            parts.push("Shift");
        }
        if shortcut.mods.contains(Modifiers::ALT) {
            parts.push("Alt");
        }
        if shortcut.mods.contains(Modifiers::SUPER) {
            parts.push("Win");
        }
        parts.push(key.as_str());
        parts.join("+")
    }
}

fn key_label(code: Code) -> String {
    let raw = code.to_string();
    if let Some(rest) = raw.strip_prefix("Key") {
        return rest.to_string();
    }
    if let Some(rest) = raw.strip_prefix("Digit") {
        return rest.to_string();
    }
    match raw.as_str() {
        "ArrowUp" => "↑".into(),
        "ArrowDown" => "↓".into(),
        "ArrowLeft" => "←".into(),
        "ArrowRight" => "→".into(),
        other => other.to_string(),
    }
}

fn assigned_to(
    config: &HotkeysConfig,
    shortcut: &Shortcut,
    except: Option<HotkeyId>,
) -> Option<HotkeyId> {
    for id in HotkeyId::all() {
        if Some(id) == except {
            continue;
        }
        if let Ok(existing) = parse_accelerator(config.get(id)) {
            if existing.id() == shortcut.id() {
                return Some(id);
            }
        }
    }
    None
}

fn normalize(mut config: HotkeysConfig) -> HotkeysConfig {
    let fallback = defaults();
    for id in HotkeyId::all() {
        match parse_accelerator(config.get(id)) {
            Ok(shortcut) => config.set(id, shortcut.to_string()),
            Err(_) => config.set(id, fallback.get(id).to_string()),
        }
    }
    config
}

fn load(app: &AppHandle) -> HotkeysConfig {
    let Ok(store) = app.store("settings.json") else {
        return defaults();
    };
    match store.get(STORE_KEY) {
        Some(value) => serde_json::from_value(value)
            .map(normalize)
            .unwrap_or_else(|_| defaults()),
        None => defaults(),
    }
}

fn save(app: &AppHandle, config: &HotkeysConfig) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let value = serde_json::to_value(config).map_err(|e| e.to_string())?;
    store.set(STORE_KEY, value);
    store.save().map_err(|e| e.to_string())
}

fn map_lock() -> std::sync::MutexGuard<'static, HashMap<u32, Mode>> {
    REGISTERED.lock().unwrap_or_else(|e| e.into_inner())
}

fn remember(shortcut: Shortcut, mode: Mode) {
    map_lock().insert(shortcut.id(), mode);
}

fn is_remembered(shortcut: &Shortcut) -> bool {
    map_lock().contains_key(&shortcut.id())
}

/// 启动时按配置注册；单个冲突跳过，不拖垮启动。
pub fn register(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    apply_registrations(app, &load(app));
    Ok(())
}

fn apply_registrations(app: &AppHandle, config: &HotkeysConfig) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    map_lock().clear();
    for id in HotkeyId::all() {
        let Ok(shortcut) = parse_accelerator(config.get(id)) else {
            continue;
        };
        match gs.register(shortcut) {
            Ok(()) => remember(shortcut, id.mode()),
            Err(err) => {
                let label = display_accelerator(config.get(id), permissions::platform());
                eprintln!(
                    "[jiti] 全局热键 {}（{}）注册失败，已跳过：{err}",
                    label,
                    id.title()
                );
            }
        }
    }
}

/// 热键 → 模式。未登记的回落到统一面板。
pub fn mode_for(shortcut: &Shortcut) -> Mode {
    map_lock()
        .get(&shortcut.id())
        .copied()
        .unwrap_or(Mode::Panel)
}

/// 录制新热键前卸掉已注册快捷键，否则 OS 会吞掉按键，WebView 收不到。
pub fn suspend(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}

pub fn resume(app: &AppHandle) {
    apply_registrations(app, &load(app));
}

pub fn snapshot(app: &AppHandle) -> HotkeysSnapshot {
    let platform = permissions::platform();
    let config = load(app);
    let defaults = defaults_for(platform);
    let gs = app.global_shortcut();
    let bindings = HotkeyId::all()
        .into_iter()
        .map(|id| {
            let accelerator = config.get(id).to_string();
            let parsed = parse_accelerator(&accelerator).ok();
            let registered = parsed
                .map(|shortcut| gs.is_registered(shortcut) || is_remembered(&shortcut))
                .unwrap_or(false);
            HotkeyBinding {
                id,
                title: id.title().into(),
                display: display_accelerator(&accelerator, platform),
                is_default: accelerator == defaults.get(id),
                registered,
                accelerator,
            }
        })
        .collect();
    HotkeysSnapshot {
        platform: platform.into(),
        bindings,
    }
}

pub fn set_binding(
    app: &AppHandle,
    id: HotkeyId,
    accelerator: &str,
) -> Result<HotkeysSnapshot, String> {
    let new = parse_accelerator(accelerator)?;
    let mut config = load(app);
    if let Some(owner) = assigned_to(&config, &new, Some(id)) {
        return Err(format!("该快捷键已被「{}」占用", owner.title()));
    }

    let previous = config.clone();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Err(err) = gs.register(new) {
        apply_registrations(app, &previous);
        return Err(register_error(&new, err));
    }

    config.set(id, new.to_string());
    if let Err(err) = save(app, &config) {
        apply_registrations(app, &previous);
        return Err(err);
    }
    apply_registrations(app, &config);
    Ok(snapshot(app))
}

pub fn reset(app: &AppHandle) -> Result<HotkeysSnapshot, String> {
    let config = defaults();
    apply_registrations(app, &config);
    save(app, &config)?;
    Ok(snapshot(app))
}

fn register_error(shortcut: &Shortcut, err: impl std::fmt::Display) -> String {
    let label = format_shortcut(shortcut, permissions::platform());
    format!("无法注册 {label}，可能已被其他程序占用（{err}）")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_default_translate_is_ctrl_shift_t() {
        let d = defaults_for("windows");
        let shortcut = parse_accelerator(&d.translate).unwrap();
        assert!(shortcut.mods.contains(Modifiers::CONTROL));
        assert!(shortcut.mods.contains(Modifiers::SHIFT));
        assert!(!shortcut.mods.contains(Modifiers::ALT));
        assert!(!shortcut.mods.contains(Modifiers::SUPER));
        assert_eq!(shortcut.key, Code::KeyT);
        assert_eq!(display_accelerator(&d.translate, "windows"), "Ctrl+Shift+T");
    }

    #[test]
    fn macos_defaults_are_option_command() {
        let d = defaults_for("macos");
        let translate = parse_accelerator(&d.translate).unwrap();
        assert!(translate.mods.contains(Modifiers::ALT));
        assert!(translate.mods.contains(Modifiers::SUPER));
        assert_eq!(translate.key, Code::KeyT);
        assert_eq!(display_accelerator(&d.translate, "macos"), "⌥⌘T");
        assert_eq!(display_accelerator(&d.grammar, "macos"), "⌥⌘G");
        assert_eq!(display_accelerator(&d.panel, "macos"), "⌥⌘Space");
    }

    #[test]
    fn windows_grammar_and_panel_stay_ctrl_alt() {
        let d = defaults_for("windows");
        assert_eq!(display_accelerator(&d.grammar, "windows"), "Ctrl+Alt+G");
        assert_eq!(display_accelerator(&d.panel, "windows"), "Ctrl+Alt+Space");
    }

    #[test]
    fn parse_accepts_user_typed_forms() {
        let a = parse_accelerator("Ctrl+Shift+T").unwrap();
        let b = parse_accelerator("shift+control+KeyT").unwrap();
        assert_eq!(a.id(), b.id());
        assert_eq!(a.to_string(), "shift+control+KeyT");
    }

    #[test]
    fn parse_rejects_bare_key_and_escape() {
        assert!(parse_accelerator("KeyT").is_err());
        assert!(parse_accelerator("T").is_err());
        assert!(parse_accelerator("Ctrl+Escape").is_err());
        assert!(parse_accelerator("Shift+Ctrl").is_err());
    }

    #[test]
    fn internal_conflict_detects_other_binding() {
        let config = defaults_for("windows");
        let shortcut = parse_accelerator(&config.grammar).unwrap();
        assert_eq!(
            assigned_to(&config, &shortcut, Some(HotkeyId::Translate)),
            Some(HotkeyId::Grammar)
        );
        assert_eq!(
            assigned_to(&config, &shortcut, Some(HotkeyId::Grammar)),
            None
        );
    }

    #[test]
    fn normalize_replaces_invalid_fields_with_defaults() {
        let restored = normalize(HotkeysConfig {
            translate: "not-a-key".into(),
            grammar: "ctrl+alt+KeyG".into(),
            panel: String::new(),
        });
        let d = defaults();
        assert_eq!(restored.translate, d.translate);
        assert_eq!(restored.panel, d.panel);
        assert_eq!(
            parse_accelerator(&restored.grammar).unwrap().id(),
            parse_accelerator("ctrl+alt+KeyG").unwrap().id()
        );
    }

    #[test]
    fn defaults_have_no_internal_conflicts() {
        for platform in ["macos", "windows"] {
            let config = defaults_for(platform);
            for id in HotkeyId::all() {
                let shortcut = parse_accelerator(config.get(id)).unwrap();
                assert_eq!(
                    assigned_to(&config, &shortcut, Some(id)),
                    None,
                    "{platform} {id:?}"
                );
            }
        }
    }
}
