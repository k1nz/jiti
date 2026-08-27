//! 系统权限检测与首次引导状态（§7.2）。
//!
//! 当前真正需要用户授权的只有 macOS **辅助功能**：
//! AX 读选中文本 + CGEvent 模拟 Cmd+C 兜底都依赖它。
//! 全局热键走 Carbon RegisterEventHotKey，不监听按键，因此不索要「输入监控」。
//! Windows UIA 无对等的用户授权闸门。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const ONBOARDING_SEEN_KEY: &str = "onboardingSeen";
pub const ACCESSIBILITY_ID: &str = "accessibility";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PermissionItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub granted: bool,
    pub required: bool,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PermissionsSnapshot {
    pub platform: String,
    pub items: Vec<PermissionItem>,
    pub all_required_granted: bool,
    pub onboarding_seen: bool,
    pub needs_onboarding: bool,
}

pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "windows"
    }
}

pub fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { objc2_application_services::AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn items(ax_trusted: bool) -> Vec<PermissionItem> {
    #[cfg(target_os = "macos")]
    {
        vec![accessibility_item(ax_trusted)]
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ax_trusted;
        Vec::new()
    }
}

pub fn snapshot_from(items: Vec<PermissionItem>, onboarding_seen: bool) -> PermissionsSnapshot {
    let all_required_granted = items
        .iter()
        .filter(|item| item.required)
        .all(|item| item.granted);
    PermissionsSnapshot {
        platform: platform().into(),
        items,
        all_required_granted,
        onboarding_seen,
        needs_onboarding: !onboarding_seen && !all_required_granted,
    }
}

pub fn snapshot(app: &AppHandle) -> PermissionsSnapshot {
    snapshot_from(items(accessibility_trusted()), onboarding_seen(app))
}

pub fn onboarding_seen(app: &AppHandle) -> bool {
    let Ok(store) = app.store("settings.json") else {
        return false;
    };
    match store.get(ONBOARDING_SEEN_KEY) {
        Some(Value::Bool(true)) => true,
        _ => false,
    }
}

pub fn set_onboarding_seen(app: &AppHandle, seen: bool) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(ONBOARDING_SEEN_KEY, Value::Bool(seen));
    store.save().map_err(|e| e.to_string())
}

/// 弹出系统「辅助功能」提示（异步，不影响返回值），并打开系统设置对应页。
pub fn request_and_open(id: &str) -> Result<bool, String> {
    match id {
        ACCESSIBILITY_ID => {
            prompt_accessibility();
            open_accessibility_settings()
        }
        other => Err(format!("未知权限：{other}")),
    }
}

pub fn open_accessibility_settings() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map(|_| true)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    Ok(false)
}

fn prompt_accessibility() {
    #[cfg(target_os = "macos")]
    {
        use objc2_application_services::{
            kAXTrustedCheckOptionPrompt, AXIsProcessTrustedWithOptions,
        };
        use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};

        let prompt = unsafe { kAXTrustedCheckOptionPrompt };
        let dict =
            CFDictionary::<CFString, CFBoolean>::from_slices(&[prompt], &[CFBoolean::new(true)]);
        let _ = unsafe { AXIsProcessTrustedWithOptions(Some(dict.as_ref())) };
    }
}

#[cfg(target_os = "macos")]
fn accessibility_item(granted: bool) -> PermissionItem {
    PermissionItem {
        id: ACCESSIBILITY_ID.into(),
        title: "辅助功能".into(),
        description: "读取其他应用中的选中文字；无法直接读取时用模拟复制兜底。".into(),
        granted,
        required: true,
        hint: if granted {
            Some("若授权后仍读不到选中文字，请从托盘退出后重新打开。".into())
        } else {
            Some("在系统设置里找到 Jiti，打开右侧开关。授权后回到这里，状态会自动更新。".into())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_required_permission_needs_onboarding() {
        let item = PermissionItem {
            id: ACCESSIBILITY_ID.into(),
            title: "辅助功能".into(),
            description: "x".into(),
            granted: false,
            required: true,
            hint: None,
        };
        let snap = snapshot_from(vec![item], false);
        assert!(snap.needs_onboarding);
        assert!(!snap.all_required_granted);
    }

    #[test]
    fn skip_hides_onboarding_even_if_still_missing() {
        let item = PermissionItem {
            id: ACCESSIBILITY_ID.into(),
            title: "辅助功能".into(),
            description: "x".into(),
            granted: false,
            required: true,
            hint: None,
        };
        let snap = snapshot_from(vec![item], true);
        assert!(!snap.needs_onboarding);
        assert!(!snap.all_required_granted);
    }

    #[test]
    fn granted_permissions_skip_onboarding() {
        let snap = snapshot_from(Vec::new(), false);
        assert!(snap.all_required_granted);
        assert!(!snap.needs_onboarding);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_lists_accessibility_as_required() {
        let items = items(false);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, ACCESSIBILITY_ID);
        assert!(items[0].required);
        assert!(!items[0].granted);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn windows_has_no_required_permissions() {
        assert!(items(false).is_empty());
        assert_eq!(platform(), "windows");
    }
}
