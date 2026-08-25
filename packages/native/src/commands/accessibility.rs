//! 辅助功能权限命令（§7.2：macOS 权限状态卡 + 一键跳系统设置）。

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccessibilityStatus {
    pub trusted: bool,
    pub platform: String,
    pub hint: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn accessibility_status() -> AccessibilityStatus {
    #[cfg(target_os = "macos")]
    {
        let trusted = unsafe { objc2_application_services::AXIsProcessTrusted() };
        return AccessibilityStatus {
            trusted,
            platform: "macos".into(),
            hint: if trusted {
                None
            } else {
                Some(
                    "Jiti 需要辅助功能权限才能直接读取选中文本；授权后通常需重启应用生效。"
                        .into(),
                )
            },
        };
    }
    #[cfg(not(target_os = "macos"))]
    AccessibilityStatus {
        trusted: true,
        platform: "windows".into(),
        hint: None,
    }
}

#[tauri::command]
#[specta::specta]
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
