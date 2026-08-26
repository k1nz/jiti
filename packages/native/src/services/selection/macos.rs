//! macOS 选中捕获（§4.5）：AX 首选 → Cmd+C + 剪贴板还原兜底 → 手动输入。
//! 本模块只在主进程主线程触 AppKit/ApplicationServices。

use std::ptr::NonNull;
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting,
};
use objc2_application_services::{AXError, AXIsProcessTrusted, AXUIElement};
use objc2_core_foundation::{
    CFRetained, CFString, CFType, ConcreteType,
};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
};
use objc2_foundation::NSArray;

use super::{SelectedText, SelectionMethod};

const KEY_C: u16 = 8; // kVK_ANSI_C

pub fn read() -> SelectedText {
    if !unsafe { AXIsProcessTrusted() } {
        return clipboard_fallback();
    }
    match ax_selected_text() {
        Some(text) if !text.trim().is_empty() => SelectedText {
            text,
            method: SelectionMethod::Ax,
        },
        _ => clipboard_fallback(),
    }
}

fn ax_selected_text() -> Option<String> {
    let system = unsafe { AXUIElement::new_system_wide() };
    let _ = unsafe { system.set_messaging_timeout(1.0) };

    // 系统 AXAttributeConstants.h 中 kAX*Attribute 是 CFSTR 宏而非导出符号,
    // 直接构造等价常量字符串。
    let focused_app = CFString::from_static_str("AXFocusedApplication");
    let focused_element = CFString::from_static_str("AXFocusedUIElement");
    let selected_text = CFString::from_static_str("AXSelectedText");

    let app = ax_attr::<AXUIElement>(&system, &focused_app)?;
    let focused = ax_attr::<AXUIElement>(&app, &focused_element)?;
    let text = ax_attr::<CFString>(&focused, &selected_text)?;
    Some(text.to_string())
}

fn ax_attr<T: ConcreteType>(
    element: &AXUIElement,
    attribute: &CFString,
) -> Option<CFRetained<T>> {
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

fn clipboard_fallback() -> SelectedText {
    let pasteboard = NSPasteboard::generalPasteboard();
    let original = pasteboard.pasteboardItems();
    post_cmd_c();
    std::thread::sleep(Duration::from_millis(120));
    let text = pasteboard
        .stringForType(unsafe { NSPasteboardTypeString })
        .map(|s| s.to_string())
        .unwrap_or_default();

    restore_clipboard(original);
    if text.trim().is_empty() {
        SelectedText {
            text: String::new(),
            method: SelectionMethod::Manual,
        }
    } else {
        SelectedText {
            text,
            method: SelectionMethod::Clipboard,
        }
    }
}

fn post_cmd_c() {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState);
    for key_down in [true, false] {
        if let Some(event) = CGEvent::new_keyboard_event(source.as_deref(), KEY_C, key_down) {
            CGEvent::set_flags(Some(&event), CGEventFlags::MaskCommand);
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        }
    }
}

fn restore_clipboard(items: Option<Retained<NSArray<NSPasteboardItem>>>) {
    let Some(items) = items else { return };
    let owned: Vec<Retained<NSPasteboardItem>> = items.into_iter().collect();
    let protos: Vec<&ProtocolObject<dyn NSPasteboardWriting>> = owned
        .iter()
        .map(|item| ProtocolObject::from_ref(&**item))
        .collect();
    let array = NSArray::from_slice(&protos);
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    pasteboard.writeObjects(&array);
}

// kAX*Attribute 在系统头文件里是 CFSTR 宏而非导出符号,见 ax_selected_text 内注释。
