//! 全局热键配置 IPC（§7.3）：快照 / 单条重配 / 恢复默认。

use tauri::AppHandle;

use crate::services::hotkeys::{self, HotkeyId, HotkeysSnapshot};

#[tauri::command]
#[specta::specta]
pub fn hotkeys_snapshot(app: AppHandle) -> HotkeysSnapshot {
    hotkeys::snapshot(&app)
}

#[tauri::command]
#[specta::specta]
pub fn hotkeys_set(
    app: AppHandle,
    id: HotkeyId,
    accelerator: String,
) -> Result<HotkeysSnapshot, String> {
    hotkeys::set_binding(&app, id, &accelerator)
}

#[tauri::command]
#[specta::specta]
pub fn hotkeys_reset(app: AppHandle) -> Result<HotkeysSnapshot, String> {
    hotkeys::reset(&app)
}

#[tauri::command]
#[specta::specta]
pub fn hotkeys_suspend(app: AppHandle) {
    hotkeys::suspend(&app);
}

#[tauri::command]
#[specta::specta]
pub fn hotkeys_resume(app: AppHandle) {
    hotkeys::resume(&app);
}
