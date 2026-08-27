//! 面板显示/隐藏命令。M0 语义：只操作“已创建的隐藏窗”。

use tauri::AppHandle;

use crate::services::panel::{self, Mode};

#[tauri::command]
#[specta::specta]
pub fn show_popup(app: AppHandle, mode: Option<String>) -> Result<(), String> {
    panel::show_panel(&app, Mode::from_opt(mode.as_deref()));
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn hide_popup(app: AppHandle) -> Result<(), String> {
    panel::hide_panel(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_panel_pinned(pinned: bool) -> bool {
    panel::set_pinned(pinned)
}

#[tauri::command]
#[specta::specta]
pub fn panel_pinned() -> bool {
    panel::is_pinned()
}
