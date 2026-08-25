//! Windows 选中捕获（§4.5）：UI Automation 首选 → Ctrl+C + 剪贴板还原兜底 → 手动输入。

use std::mem::size_of;
use std::time::Duration;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
    UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY, VK_CONTROL, VK_C,
};

use super::{SelectedText, SelectionMethod};

pub fn read() -> SelectedText {
    if let Some(text) = uia_selected() {
        if !text.trim().is_empty() {
            return SelectedText {
                text,
                method: SelectionMethod::Ax,
            };
        }
    }
    if let Some(text) = clipboard_fallback() {
        return SelectedText {
            text,
            method: SelectionMethod::Clipboard,
        };
    }
    SelectedText {
        text: String::new(),
        method: SelectionMethod::Manual,
    }
}

fn uia_selected() -> Option<String> {
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).ok()?;
        let focused = automation.GetFocusedElement().ok()?;

        if let Ok(pattern) =
            focused.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        {
            if let Ok(ranges) = pattern.GetSelection() {
                let len = ranges.Length().ok()?;
                for index in 0..len {
                    if let Ok(range) = ranges.GetElement(index) {
                        if let Ok(bstr) = range.GetText(-1) {
                            let text = bstr.to_string();
                            if !text.trim().is_empty() {
                                return Some(text);
                            }
                        }
                    }
                }
            }
        }

        if let Ok(pattern) =
            focused.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            if let Ok(bstr) = pattern.CurrentValue() {
                let text = bstr.to_string();
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
        }
        None
    }
}

fn clipboard_fallback() -> Option<String> {
    let snapshot = unsafe { snapshot_clipboard() }?;
    send_ctrl_c();
    std::thread::sleep(Duration::from_millis(120));
    let text = unsafe { read_selected_from_clipboard() }.unwrap_or_default();
    unsafe { restore_clipboard(snapshot) };
    if text.trim().is_empty() { None } else { Some(text) }
}

#[derive(Default)]
struct ClipboardSnapshot {
    items: Vec<ClipboardData>,
}

struct ClipboardData {
    format: u32,
    bytes: Vec<u8>,
}

/// 保存前先完整读取剪贴板；打不开就放弃兜底，避免污染用户剪贴板。
unsafe fn snapshot_clipboard() -> Option<ClipboardSnapshot> {
    if OpenClipboard(None).is_err() {
        return None;
    }
    let mut snapshot = ClipboardSnapshot::default();
    let mut format = 0u32;
    loop {
        format = EnumClipboardFormats(format);
        if format == 0 {
            break;
        }
        if let Ok(handle) = GetClipboardData(format) {
            let global = HGLOBAL(handle.0);
            let size = GlobalSize(global);
            if size == 0 {
                continue;
            }
            let ptr = GlobalLock(global);
            if ptr.is_null() {
                continue;
            }
            let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), size).to_vec();
            let _ = GlobalUnlock(global);
            snapshot.items.push(ClipboardData { format, bytes });
        }
    }
    let _ = CloseClipboard();
    Some(snapshot)
}

unsafe fn read_selected_from_clipboard() -> Option<String> {
    if OpenClipboard(None).is_err() {
        return None;
    }
    let text = (|| {
        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
        let global = HGLOBAL(handle.0);
        let size = GlobalSize(global);
        if size < 2 {
            return None;
        }
        let ptr = GlobalLock(global) as *const u16;
        if ptr.is_null() {
            return None;
        }
        let words = std::slice::from_raw_parts(ptr, size / 2);
        let text = String::from_utf16_lossy(words);
        let _ = GlobalUnlock(global);
        Some(text)
    })();
    let _ = CloseClipboard();
    text
}

/// 字格式级还原：把复制前的每一种剪贴板格式写回去，而不是只还原纯文本。
unsafe fn restore_clipboard(snapshot: ClipboardSnapshot) {
    if OpenClipboard(None).is_err() {
        return;
    }
    let _ = EmptyClipboard();
    for item in snapshot.items {
        if item.bytes.is_empty() {
            continue;
        }
        let Some(global) = GlobalAlloc(GMEM_MOVEABLE, item.bytes.len()).ok() else {
            continue;
        };
        let ptr = GlobalLock(global);
        if ptr.is_null() {
            let _ = GlobalFree(Some(global));
            continue;
        }
        std::ptr::copy_nonoverlapping(item.bytes.as_ptr(), ptr.cast::<u8>(), item.bytes.len());
        let _ = GlobalUnlock(global);
        let handle = HANDLE(global.0);
        if SetClipboardData(item.format, Some(handle)).is_err() {
            let _ = GlobalFree(Some(global));
        }
    }
    let _ = CloseClipboard();
}

fn send_ctrl_c() {
    let key_input = |vk: VIRTUAL_KEY, up: bool| -> INPUT {
        let mut input = INPUT::default();
        input.r#type = INPUT_KEYBOARD;
        let flags = if up {
            KEYEVENTF_KEYUP
        } else {
            KEYBD_EVENT_FLAGS(0)
        };
        input.Anonymous.ki = KEYBDINPUT {
            wVk: vk,
            wScan: 0,
            dwFlags: flags,
            time: 0,
            dwExtraInfo: 0,
        };
        input
    };
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VK_C, false),
        key_input(VK_C, true),
        key_input(VK_CONTROL, true),
    ];
    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}
