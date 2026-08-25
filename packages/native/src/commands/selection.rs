//! 读取选中文本命令。
//!
//! M0 占位：返回空串，链路（AX → 剪贴板兜底 → 手动输入，§4.5）在 M1 实现，
//! services::selection 只预留边界。红线：M0 不做翻译/语法/错题本真实逻辑。

use crate::services;

#[tauri::command]
#[specta::specta]
pub fn get_selected_text() -> String {
    services::selection::read_selected_text()
}