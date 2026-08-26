//! macOS 选中捕获（§4.5）：AX 首选 → Cmd+C + 剪贴板还原兜底 → 手动输入。
//! 本模块只在主进程主线程触 AppKit/ApplicationServices。

use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Duration;

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_application_services::{AXError, AXIsProcessTrusted, AXUIElement, AXValue, AXValueType};
use objc2_core_foundation::{CFRange, CFRetained, CFString, CFType, ConcreteType};
use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID};
use objc2_foundation::NSString;
use tauri::AppHandle;
use tauri_specta::Event;

use super::{CaptureChangedEvent, SelectedText, SelectionMethod};

const KEY_C: u16 = 8; // kVK_ANSI_C
const KEY_COMMAND: u16 = 55; // kVK_Command

/// 仅 AX（含 SelectedTextRange 回退）。主线程、无睡眠。
pub fn read_preferred() -> SelectedText {
    if frontmost_is_self() {
        #[cfg(debug_assertions)]
        eprintln!("[jiti] selection AX skipped: Jiti is frontmost");
        return SelectedText::empty();
    }
    let trusted = unsafe { AXIsProcessTrusted() };
    #[cfg(debug_assertions)]
    eprintln!("[jiti] selection AX trusted={trusted}");
    if !trusted {
        return SelectedText::empty();
    }
    match ax_selected_text() {
        Some(text) if !text.trim().is_empty() => {
            #[cfg(debug_assertions)]
            eprintln!("[jiti] selection AX captured {} bytes", text.len());
            SelectedText {
                text,
                method: SelectionMethod::Ax,
            }
        }
        _ => {
            #[cfg(debug_assertions)]
            eprintln!("[jiti] selection AX returned no text; using clipboard fallback");
            SelectedText::empty()
        }
    }
}

pub fn read() -> SelectedText {
    let preferred = read_preferred();
    if !preferred.is_blank() {
        return preferred;
    }
    clipboard_fallback_blocking()
}

/// 热键路径：不阻塞 reveal。对着热键按下时的源进程发 Cmd+C，不依赖之后谁是前台。
pub fn begin_clipboard_fallback(app: AppHandle, epoch: u32) {
    let target_pid = match frontmost_pid() {
        Some(pid) if pid as u32 != std::process::id() => pid,
        _ => return,
    };
    std::thread::spawn(move || {
        if !super::is_current_epoch(epoch) {
            return;
        }
        wait_for_hotkey_modifiers_up();
        if !super::is_current_epoch(epoch) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let posted = app.clone();
        let _ = posted.run_on_main_thread(move || {
            let pasteboard = NSPasteboard::generalPasteboard();
            let change_count = pasteboard.changeCount();
            let original = snapshot_clipboard_string();
            post_cmd_c_to_pid(target_pid);
            let _ = tx.send(Some((original, change_count)));
        });
        let Ok(Some((original, change_count))) = rx.recv_timeout(Duration::from_millis(800)) else {
            return;
        };
        std::thread::sleep(Duration::from_millis(180));
        if !super::is_current_epoch(epoch) {
            let restore_app = app.clone();
            let _ = restore_app.run_on_main_thread(move || {
                restore_clipboard_string(original);
            });
            return;
        }
        let emit_app = app.clone();
        let _ = app.run_on_main_thread(move || {
            let pasteboard = NSPasteboard::generalPasteboard();
            let changed = pasteboard.changeCount() != change_count;
            let text = snapshot_clipboard_string().unwrap_or_default();
            restore_clipboard_string(original);
            if !changed || text.trim().is_empty() || !super::is_current_epoch(epoch) {
                #[cfg(debug_assertions)]
                eprintln!("[jiti] selection clipboard fallback skipped changed={changed}");
                return;
            }
            #[cfg(debug_assertions)]
            eprintln!("[jiti] selection clipboard captured {} bytes", text.len());
            let _ = CaptureChangedEvent {
                epoch,
                selection: SelectedText {
                    text,
                    method: SelectionMethod::Clipboard,
                },
            }
            .emit(&emit_app);
        });
    });
}

fn clipboard_fallback_blocking() -> SelectedText {
    let Some(pid) = frontmost_pid() else {
        return SelectedText::empty();
    };
    if pid as u32 == std::process::id() {
        return SelectedText::empty();
    }
    wait_for_hotkey_modifiers_up();
    let pasteboard = NSPasteboard::generalPasteboard();
    let change_count = pasteboard.changeCount();
    let original = snapshot_clipboard_string();
    post_cmd_c_to_pid(pid);
    std::thread::sleep(Duration::from_millis(180));
    let changed = pasteboard.changeCount() != change_count;
    let text = snapshot_clipboard_string().unwrap_or_default();
    restore_clipboard_string(original);
    if !changed || text.trim().is_empty() {
        SelectedText::empty()
    } else {
        SelectedText {
            text,
            method: SelectionMethod::Clipboard,
        }
    }
}

fn frontmost_pid() -> Option<libc::pid_t> {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
}

fn frontmost_is_self() -> bool {
    frontmost_pid().is_some_and(|pid| pid as u32 == std::process::id())
}

fn ax_pid_is_self(element: &AXUIElement) -> bool {
    let mut pid: libc::pid_t = 0;
    let Some(out) = NonNull::new(&mut pid) else {
        return false;
    };
    let err = unsafe { element.pid(out) };
    err == AXError::Success && pid as u32 == std::process::id()
}

fn ax_selected_text() -> Option<String> {
    let system = unsafe { AXUIElement::new_system_wide() };
    let _ = unsafe { system.set_messaging_timeout(1.0) };

    let focused_app = CFString::from_static_str("AXFocusedApplication");
    let focused_element = CFString::from_static_str("AXFocusedUIElement");
    let selected_text = CFString::from_static_str("AXSelectedText");

    if let Some(app) = ax_attr::<AXUIElement>(&system, &focused_app) {
        if ax_pid_is_self(&app) {
            return None;
        }
        if let Some(focused) = ax_attr::<AXUIElement>(&app, &focused_element) {
            if let Some(text) = nonempty_ax_string(&focused, &selected_text) {
                return Some(text);
            }
            if let Some(text) = ax_selected_via_range(&focused) {
                return Some(text);
            }
        }
    }

    if let Some(focused) = ax_attr::<AXUIElement>(&system, &focused_element) {
        if let Some(text) = nonempty_ax_string(&focused, &selected_text) {
            return Some(text);
        }
        if let Some(text) = ax_selected_via_range(&focused) {
            return Some(text);
        }
    }
    None
}

fn nonempty_ax_string(element: &AXUIElement, attribute: &CFString) -> Option<String> {
    let text = ax_attr::<CFString>(element, attribute)?.to_string();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Chrome / VS Code 等经常不实现 AXSelectedText，但给得出 range + AXValue。
fn ax_selected_via_range(element: &AXUIElement) -> Option<String> {
    let range_attr = CFString::from_static_str("AXSelectedTextRange");
    let value_attr = CFString::from_static_str("AXValue");
    let range_val = ax_attr::<AXValue>(element, &range_attr)?;
    if unsafe { range_val.r#type() } != AXValueType::CFRange {
        return None;
    }
    let mut range = CFRange {
        location: 0,
        length: 0,
    };
    let ok = unsafe {
        range_val.value(
            AXValueType::CFRange,
            NonNull::new((&mut range as *mut CFRange).cast::<c_void>()).expect("range 指针恒有效"),
        )
    };
    if !ok || range.length <= 0 {
        return None;
    }
    let full = ax_attr::<CFString>(element, &value_attr)?.to_string();
    let utf16: Vec<u16> = full.encode_utf16().collect();
    let start = range.location.max(0) as usize;
    if start >= utf16.len() {
        return None;
    }
    let end = start.saturating_add(range.length as usize).min(utf16.len());
    let sliced = String::from_utf16_lossy(&utf16[start..end]);
    if sliced.trim().is_empty() {
        None
    } else {
        Some(sliced)
    }
}

fn ax_attr<T: ConcreteType>(element: &AXUIElement, attribute: &CFString) -> Option<CFRetained<T>> {
    let mut value: *const CFType = std::ptr::null();
    let err = unsafe {
        element.copy_attribute_value(
            attribute,
            NonNull::new(&mut value).expect("value out pointer 恒有效"),
        )
    };
    if err != AXError::Success || value.is_null() {
        return None;
    }
    let retained =
        unsafe { CFRetained::from_raw(NonNull::new(value as *mut CFType).expect("非空")) };
    retained.downcast::<T>().ok()
}

/// 等用户松开当前热键的修饰键，避免合成复制变成 ⌥⌘C / ⌃⇧C。
fn wait_for_hotkey_modifiers_up() {
    for _ in 0..30 {
        let flags = CGEventSource::flags_state(CGEventSourceStateID::HIDSystemState);
        if !flags.contains(CGEventFlags::MaskAlternate)
            && !flags.contains(CGEventFlags::MaskCommand)
            && !flags.contains(CGEventFlags::MaskShift)
            && !flags.contains(CGEventFlags::MaskControl)
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn post_cmd_c_to_pid(pid: libc::pid_t) {
    // Private：不与当前 HID 修饰键合并，否则 ⌥ 仍按下时会变成 ⌥⌘C。
    // 发给热键按下时的源进程，避免面板成为前台后复制到自己的输入框。
    let source = CGEventSource::new(CGEventSourceStateID::Private);

    let post = |key: u16, key_down: bool, flags: CGEventFlags| {
        if let Some(event) = CGEvent::new_keyboard_event(source.as_deref(), key, key_down) {
            CGEvent::set_flags(Some(&event), flags);
            CGEvent::post_to_pid(pid, Some(&event));
        }
    };

    post(KEY_COMMAND, true, CGEventFlags::MaskCommand);
    post(KEY_C, true, CGEventFlags::MaskCommand);
    post(KEY_C, false, CGEventFlags::MaskCommand);
    post(KEY_COMMAND, false, CGEventFlags::empty());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_key_code_is_not_c_key_code() {
        assert_ne!(KEY_COMMAND, KEY_C);
    }
}

fn snapshot_clipboard_string() -> Option<String> {
    NSPasteboard::generalPasteboard()
        .stringForType(unsafe { NSPasteboardTypeString })
        .map(|s| s.to_string())
}

/// 只还原纯文本。回写旧的 NSPasteboardItem 在 Cmd+C 之后会抛 NSException，
/// Rust 接不住（foreign exception）直接 abort。
fn restore_clipboard_string(original: Option<String>) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    if let Some(text) = original {
        pasteboard.setString_forType(&NSString::from_str(&text), unsafe {
            NSPasteboardTypeString
        });
    }
}
