//! 权限检测 / 首次引导 IPC（§7.2）。

use tauri::AppHandle;

use crate::services::permissions::{self, PermissionsSnapshot};

#[tauri::command]
#[specta::specta]
pub fn permissions_snapshot(app: AppHandle) -> PermissionsSnapshot {
    permissions::snapshot(&app)
}

#[tauri::command]
#[specta::specta]
pub fn request_permission(id: String) -> Result<bool, String> {
    permissions::request_and_open(&id)
}

#[tauri::command]
#[specta::specta]
pub fn open_permission_settings(id: String) -> Result<bool, String> {
    match id.as_str() {
        permissions::ACCESSIBILITY_ID => permissions::open_accessibility_settings(),
        other => Err(format!("未知权限：{other}")),
    }
}

#[tauri::command]
#[specta::specta]
pub fn complete_onboarding(app: AppHandle) -> Result<PermissionsSnapshot, String> {
    permissions::set_onboarding_seen(&app, true)?;
    Ok(permissions::snapshot(&app))
}

#[tauri::command]
#[specta::specta]
pub fn restart_app(app: AppHandle) {
    app.restart();
}
