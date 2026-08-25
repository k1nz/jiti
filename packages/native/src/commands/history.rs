//! 历史命令（§6.1 history 表）。

use tauri::AppHandle;

use crate::services::history::{self, HistoryEntry};

#[tauri::command]
#[specta::specta]
pub fn history_list(app: AppHandle) -> Result<Vec<HistoryEntry>, String> {
    let conn = history::open(&app)?;
    history::list(&conn, 100)
}

#[tauri::command]
#[specta::specta]
pub fn history_clear(app: AppHandle) -> Result<i32, String> {
    let conn = history::open(&app)?;
    history::clear(&conn)
}
