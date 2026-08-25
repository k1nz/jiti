//! 选中文本读取（§4.5 降级链：AX/UIA → 剪贴板模拟 → 手动输入）。

use serde::{Deserialize, Serialize};
use specta::Type;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SelectionMethod {
    Ax,
    Clipboard,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectedText {
    pub text: String,
    pub method: SelectionMethod,
}

pub fn read_selected_text() -> SelectedText {
    #[cfg(target_os = "macos")]
    {
        return macos::read();
    }
    #[cfg(target_os = "windows")]
    {
        return windows::read();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    SelectedText {
        text: String::new(),
        method: SelectionMethod::Manual,
    }
}
