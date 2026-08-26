//! 选中文本读取（§4.5 降级链：AX/UIA → 剪贴板模拟 → 手动输入）。

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SelectionMethod {
    Ax,
    Uia,
    Clipboard,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectedText {
    pub text: String,
    pub method: SelectionMethod,
}

impl SelectedText {
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            method: SelectionMethod::Manual,
        }
    }

    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// 热键载荷：模式 + 显示前读到的选中文本（§3.4）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "hotkey://pressed")]
pub struct HotkeyPressedEvent {
    pub mode: String,
    pub selection: SelectedText,
}

/// 剪贴板兜底稍后完成时补发（AX/UIA 为空才走）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "capture://changed")]
pub struct CaptureChangedEvent(pub SelectedText);

/// AX/UIA 首选路径：快、必须在主线程。不含剪贴板睡眠。
pub fn read_preferred() -> SelectedText {
    #[cfg(target_os = "macos")]
    {
        return macos::read_preferred();
    }
    #[cfg(target_os = "windows")]
    {
        return windows::read_preferred();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    SelectedText::empty()
}

/// 完整降级链（含阻塞 120ms 剪贴板）。供命令在主线程调用。
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
    SelectedText::empty()
}

/// AX/UIA 为空时：等热键修饰键松开 → 模拟复制 → 120ms 后读剪贴板并还原。
/// 不阻塞面板 reveal。
pub fn begin_clipboard_fallback(app: AppHandle) {
    #[cfg(target_os = "macos")]
    macos::begin_clipboard_fallback(app);
    #[cfg(target_os = "windows")]
    windows::begin_clipboard_fallback(app);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_event_name_matches_architecture() {
        assert_eq!(HotkeyPressedEvent::NAME, "hotkey://pressed");
        assert_eq!(CaptureChangedEvent::NAME, "capture://changed");
    }

    #[test]
    fn empty_selection_is_manual() {
        let s = SelectedText::empty();
        assert!(s.is_blank());
        assert_eq!(s.method, SelectionMethod::Manual);
    }
}
