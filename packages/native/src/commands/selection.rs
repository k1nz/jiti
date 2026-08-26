//! 读取选中文本命令（§4.5：AX/UIA → 剪贴板兜底 → 手动输入）。
//! 必须调度到主线程：AppKit / UIA 不能在 Tauri 默认线程池上调用。

use std::time::Duration;

use tauri::AppHandle;

use crate::services;

#[tauri::command]
#[specta::specta]
pub fn get_selected_text(app: AppHandle) -> services::selection::SelectedText {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let _ = app.run_on_main_thread(move || {
        let _ = tx.send(services::selection::read_selected_text());
    });
    rx.recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|_| services::selection::SelectedText::empty())
}
