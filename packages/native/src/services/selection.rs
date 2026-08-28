//! 选中文本读取（§4.5 降级链：AX/UIA → 剪贴板模拟 → 手动输入）。

use std::sync::atomic::{AtomicU32, Ordering};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

/// 每次翻译/语法热键 +1。过期的剪贴板兜底不得写回上一次选区。
static CAPTURE_EPOCH: AtomicU32 = AtomicU32::new(0);

pub fn begin_capture() -> u32 {
    CAPTURE_EPOCH.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn is_current_epoch(epoch: u32) -> bool {
    CAPTURE_EPOCH.load(Ordering::SeqCst) == epoch
}

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
    pub epoch: u32,
}

/// 剪贴板稍后完成时补发（与对应热键的 epoch 对齐；可覆盖滞后的 AX/UIA）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "capture://changed")]
pub struct CaptureChangedEvent {
    pub epoch: u32,
    pub selection: SelectedText,
}

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

/// 等热键修饰键松开 → 模拟复制 → 剪贴板有变更才补发。
/// 即使 AX/UIA 已有文本也跑：浏览器选区经常滞后，剪贴板才是当前选中。
/// 不阻塞面板 reveal。
pub fn begin_clipboard_fallback(app: AppHandle, epoch: u32) {
    #[cfg(target_os = "macos")]
    macos::begin_clipboard_fallback(app, epoch);
    #[cfg(target_os = "windows")]
    windows::begin_clipboard_fallback(app, epoch);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        let _ = epoch;
    }
}

/// Windows 剪贴板 Ctrl+C 必须在源窗口仍是前台时发送；聚焦面板要等修饰键松开。
#[cfg(target_os = "windows")]
pub(crate) fn wait_for_hotkey_modifiers_up() {
    windows::wait_for_hotkey_modifiers_up();
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

    #[test]
    fn later_capture_invalidates_previous_epoch() {
        let first = begin_capture();
        assert!(is_current_epoch(first));
        let second = begin_capture();
        assert!(!is_current_epoch(first));
        assert!(is_current_epoch(second));
        assert!(second > first);
    }
}
