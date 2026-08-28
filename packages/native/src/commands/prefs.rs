//! 通用偏好 IPC：语言 / 主题 / 唤起聚焦 / 开机自启 / 打开设置窗口。

use tauri::AppHandle;

use crate::services::prefs::{self, PreferencesPatch, PreferencesSnapshot};
use crate::services::settings_window;

#[tauri::command]
#[specta::specta]
pub fn preferences_snapshot(app: AppHandle) -> PreferencesSnapshot {
    prefs::snapshot(&app)
}

#[tauri::command]
#[specta::specta]
pub fn preferences_update(
    app: AppHandle,
    patch: PreferencesPatch,
) -> Result<PreferencesSnapshot, String> {
    prefs::update(&app, patch)
}

#[tauri::command]
#[specta::specta]
pub async fn open_settings(app: AppHandle) -> Result<(), String> {
    // Windows：同步命令里 build() 会与 WebView2 死锁（白屏、关闭按钮无响应）。
    settings_window::open(&app)
}
