//! 读取选中文本命令（§4.5：AX/UIA → 剪贴板兜底 → 手动输入）。

use crate::services;

#[tauri::command]
#[specta::specta]
pub fn get_selected_text() -> services::selection::SelectedText {
    services::selection::read_selected_text()
}
