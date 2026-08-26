//! 辅助功能权限命令（兼容入口；完整矩阵见 `permissions`）。

use crate::services::permissions::{self, PermissionsSnapshot};

/// 旧设置页状态卡形状，由 permissions snapshot 投影而来。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccessibilityStatus {
    pub trusted: bool,
    pub platform: String,
    pub hint: Option<String>,
}

impl From<&PermissionsSnapshot> for AccessibilityStatus {
    fn from(snap: &PermissionsSnapshot) -> Self {
        let item = snap
            .items
            .iter()
            .find(|item| item.id == permissions::ACCESSIBILITY_ID);
        Self {
            trusted: item.map(|item| item.granted).unwrap_or(true),
            platform: snap.platform.clone(),
            hint: item.and_then(|item| item.hint.clone()),
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn accessibility_status() -> AccessibilityStatus {
    let items = permissions::items(permissions::accessibility_trusted());
    AccessibilityStatus::from(&permissions::snapshot_from(items, true))
}

#[tauri::command]
#[specta::specta]
pub fn open_accessibility_settings() -> Result<bool, String> {
    permissions::open_accessibility_settings()
}
