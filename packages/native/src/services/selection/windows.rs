//! Windows 选中捕获（§4.5）：UI Automation 首选 → Ctrl+C + 剪贴板还原兜底 → 手动输入。

use std::mem::size_of;
use std::time::Duration;

use tauri::AppHandle;
use tauri_specta::Event;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
    UIA_TextPatternId, UIA_ValuePatternId,
};
use windows::core::PCWSTR;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::{CaptureChangedEvent, SelectedText, SelectionMethod};

#[link(name = "user32")]
extern "system" {
    fn GetClipboardSequenceNumber() -> u32;
}

pub fn read_preferred() -> SelectedText {
    if foreground_is_self() {
        return SelectedText::empty();
    }
    if let Some(text) = uia_selected() {
        if !text.trim().is_empty() {
            return SelectedText {
                text,
                method: SelectionMethod::Uia,
            };
        }
    }
    SelectedText::empty()
}

pub fn read() -> SelectedText {
    let preferred = read_preferred();
    if !preferred.is_blank() {
        return preferred;
    }
    if let Some(text) = clipboard_fallback() {
        return SelectedText {
            text,
            method: SelectionMethod::Clipboard,
        };
    }
    SelectedText::empty()
}

pub fn begin_clipboard_fallback(app: AppHandle, epoch: u32) {
    let source_pid = foreground_pid();
    if source_pid == 0 || source_pid == std::process::id() {
        return;
    }
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
            if foreground_pid() != source_pid {
                let _ = tx.send(None);
                return;
            }
            let seq = unsafe { GetClipboardSequenceNumber() };
            let snap = unsafe { snapshot_clipboard() };
            if snap.is_some() {
                send_ctrl_c();
            }
            let _ = tx.send(snap.map(|s| (s, seq)));
        });
        let Ok(Some((snap, seq))) = rx.recv_timeout(Duration::from_millis(800)) else {
            return;
        };
        std::thread::sleep(Duration::from_millis(180));
        if !super::is_current_epoch(epoch) {
            let restore_app = app.clone();
            let _ = restore_app.run_on_main_thread(move || unsafe {
                restore_clipboard(snap);
            });
            return;
        }
        let emit_app = app.clone();
        let _ = app.run_on_main_thread(move || {
            let changed = unsafe { GetClipboardSequenceNumber() } != seq;
            let text = unsafe { read_selected_from_clipboard() }.unwrap_or_default();
            unsafe { restore_clipboard(snap) };
            if !changed || text.trim().is_empty() || !super::is_current_epoch(epoch) {
                return;
            }
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

fn foreground_pid() -> u32 {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid
    }
}

fn foreground_is_self() -> bool {
    let pid = foreground_pid();
    pid != 0 && pid == std::process::id()
}

fn uia_selected() -> Option<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).ok()?;
        let focused = automation.GetFocusedElement().ok()?;
        let walker = automation.ControlViewWalker().ok();

        // Foxmail 读信框是 CEF。文档控件的 Value 是
        // `file:///.../mail_frame.html?foxhandler=...`，TextPattern 才是选区。
        // 选区为空时不要退回 Value，否则页面地址会占住输入框，剪贴板兜底被丢掉。
        let mut saw_text_pattern = false;
        let mut current = focused.clone();
        for _ in 0..8 {
            if let Ok(pattern) =
                current.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
            {
                saw_text_pattern = true;
                if let Some(text) = text_from_selection(&pattern) {
                    if let Some(text) = reject_document_location(text) {
                        return Some(text);
                    }
                }
            }
            let Some(walker) = walker.as_ref() else {
                break;
            };
            match walker.GetParentElement(&current) {
                Ok(parent) => current = parent,
                Err(_) => break,
            }
        }

        if saw_text_pattern {
            return None;
        }

        if let Ok(pattern) =
            focused.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            if let Ok(bstr) = pattern.CurrentValue() {
                return reject_document_location(bstr.to_string());
            }
        }
        None
    }
}

fn text_from_selection(pattern: &IUIAutomationTextPattern) -> Option<String> {
    unsafe {
        let ranges = pattern.GetSelection().ok()?;
        let len = ranges.Length().ok()?;
        let mut parts = Vec::new();
        for index in 0..len {
            if let Ok(range) = ranges.GetElement(index) {
                if let Ok(bstr) = range.GetText(-1) {
                    let text = bstr.to_string();
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }
}

fn reject_document_location(text: String) -> Option<String> {
    let comparable = text.trim_matches(|ch: char| ch.is_whitespace() || ch == '\0');
    if comparable.is_empty() || is_document_location(comparable) {
        None
    } else {
        Some(text)
    }
}

/// 单行 `file://` / `http(s)://` / `about:`。Foxmail 读信框和浏览器文档的 UIA Value 都是这种地址。
pub(crate) fn is_document_location(text: &str) -> bool {
    let text = text.trim_matches(|ch: char| ch.is_whitespace() || ch == '\0');
    if text.is_empty() || text.len() > 2048 || text.chars().any(char::is_whitespace) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    lower.starts_with("file://")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("about:")
}

fn clipboard_fallback() -> Option<String> {
    if foreground_is_self() {
        return None;
    }
    wait_for_hotkey_modifiers_up();
    let seq = unsafe { GetClipboardSequenceNumber() };
    let snapshot = unsafe { snapshot_clipboard() }?;
    send_ctrl_c();
    std::thread::sleep(Duration::from_millis(180));
    let changed = unsafe { GetClipboardSequenceNumber() } != seq;
    let text = unsafe { read_selected_from_clipboard() }.unwrap_or_default();
    unsafe { restore_clipboard(snapshot) };
    if !changed || text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

#[derive(Default)]
struct ClipboardSnapshot {
    items: Vec<ClipboardData>,
}

struct ClipboardData {
    format: u32,
    bytes: Vec<u8>,
}

/// 单条格式上限：防止异常巨大的 HGLOBAL 把进程拖垮。
const MAX_CLIPBOARD_FORMAT_BYTES: usize = 32 * 1024 * 1024;

/// 只有系统用 `GlobalAlloc` 存放的格式才能 `GlobalSize`/`GlobalLock`。
/// `CF_BITMAP` / `CF_ENHMETAFILE` / `CF_PALETTE` 等是 GDI 句柄；当成 HGLOBAL
/// 会触发 `STATUS_HEAP_CORRUPTION`（0xC0000374）。
fn clipboard_format_is_hglobal(format: u32) -> bool {
    const CF_TEXT: u32 = 1;
    const CF_SYLK: u32 = 4;
    const CF_DIF: u32 = 5;
    const CF_TIFF: u32 = 6;
    const CF_OEMTEXT: u32 = 7;
    const CF_DIB: u32 = 8;
    const CF_PENDATA: u32 = 10;
    const CF_RIFF: u32 = 11;
    const CF_WAVE: u32 = 12;
    const CF_UNICODETEXT: u32 = 13;
    const CF_HDROP: u32 = 15;
    const CF_LOCALE: u32 = 16;
    const CF_DIBV5: u32 = 17;
    const CF_DSPTEXT: u32 = 0x0081;
    matches!(
        format,
        CF_TEXT
            | CF_SYLK
            | CF_DIF
            | CF_TIFF
            | CF_OEMTEXT
            | CF_DIB
            | CF_PENDATA
            | CF_RIFF
            | CF_WAVE
            | CF_UNICODETEXT
            | CF_HDROP
            | CF_LOCALE
            | CF_DIBV5
            | CF_DSPTEXT
    ) || (0xC000..=0xFFFF).contains(&format)
}

/// 保存前先完整读取剪贴板；打不开就放弃兜底，避免污染用户剪贴板。
/// 只快照 HGLOBAL 格式；GDI 对象跳过（Windows 常能从 CF_DIB 再合成位图）。
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
        if !clipboard_format_is_hglobal(format) {
            continue;
        }
        if let Ok(handle) = GetClipboardData(format) {
            let global = HGLOBAL(handle.0);
            let size = GlobalSize(global);
            if size == 0 || size > MAX_CLIPBOARD_FORMAT_BYTES {
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
        let unicode = read_clipboard_unicode()?;
        let html = read_clipboard_html();
        let chosen = prefer_clipboard_text(&unicode, html.as_deref());
        if chosen.trim().is_empty() {
            None
        } else {
            Some(chosen)
        }
    })();
    let _ = CloseClipboard();
    text
}

unsafe fn read_clipboard_unicode() -> Option<String> {
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
}

unsafe fn read_clipboard_html() -> Option<Vec<u8>> {
    let mut name: Vec<u16> = "HTML Format".encode_utf16().collect();
    name.push(0);
    let format = RegisterClipboardFormatW(PCWSTR(name.as_ptr()));
    if format == 0 {
        return None;
    }
    let handle = GetClipboardData(format).ok()?;
    let global = HGLOBAL(handle.0);
    let size = GlobalSize(global);
    if size == 0 || size > MAX_CLIPBOARD_FORMAT_BYTES {
        return None;
    }
    let ptr = GlobalLock(global);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), size).to_vec();
    let _ = GlobalUnlock(global);
    Some(bytes)
}

/// 纯文本如果就是 HTML 剪贴板的 `SourceURL`，那是页面地址不是选区。
/// 改用 `StartFragment`/`EndFragment` 里的文字。没有 HTML 格式时保持原纯文本
/// （记事本里选中一个网址仍然要能翻译）。
pub(crate) fn prefer_clipboard_text(unicode: &str, html: Option<&[u8]>) -> String {
    let comparable = unicode.trim_matches(|ch: char| ch.is_whitespace() || ch == '\0');
    let Some(html) = html else {
        return unicode.to_string();
    };
    let Some(source) = html_source_url(html) else {
        return unicode.to_string();
    };
    if !is_document_location(comparable) || !same_document_url(comparable, &source) {
        return unicode.to_string();
    }
    html_fragment_plain(html).unwrap_or_default()
}

fn same_document_url(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn cf_html_header(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|byte| *byte == b'<').unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

fn html_source_url(bytes: &[u8]) -> Option<String> {
    for line in cf_html_header(bytes).lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.eq_ignore_ascii_case("SourceURL") {
            let url = value.trim();
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

fn header_offset(header: &str, key: &str) -> Option<usize> {
    for line in header.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case(key) {
            return value.trim().parse().ok();
        }
    }
    None
}

fn html_fragment_plain(bytes: &[u8]) -> Option<String> {
    let header = cf_html_header(bytes);
    let start = header_offset(header, "StartFragment")?;
    let end = header_offset(header, "EndFragment")?;
    if start > end || end > bytes.len() {
        return None;
    }
    let raw = String::from_utf8_lossy(&bytes[start..end]);
    let plain = html_fragment_to_plain(&raw);
    let plain = plain.trim();
    if plain.is_empty() || is_document_location(plain) {
        None
    } else {
        Some(plain.to_string())
    }
}

fn html_fragment_to_plain(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(tag_at) = rest.find('<') {
            push_decoded(&mut out, &rest[..tag_at]);
            rest = &rest[tag_at + 1..];
            let Some(end) = rest.find('>') else {
                break;
            };
            if tag_inserts_break(&rest[..end]) {
                out.push('\n');
            }
            rest = &rest[end + 1..];
        } else {
            push_decoded(&mut out, rest);
            break;
        }
    }
    out
}

fn tag_inserts_break(tag_body: &str) -> bool {
    let body = tag_body.trim();
    let closing = body.starts_with('/');
    let name = body
        .trim_start_matches('/')
        .split(|ch: char| ch.is_whitespace() || ch == '/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    name == "br"
        || (closing
            && matches!(
                name.as_str(),
                "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            ))
}

fn push_decoded(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        let Some(end) = rest.find(';') else {
            out.push('&');
            out.push_str(rest);
            return;
        };
        match decode_entity(&rest[..end]) {
            Some(decoded) => out.push_str(decoded),
            None => {
                out.push('&');
                out.push_str(&rest[..end]);
                out.push(';');
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
}

fn decode_entity(entity: &str) -> Option<&'static str> {
    match entity {
        "amp" => Some("&"),
        "lt" => Some("<"),
        "gt" => Some(">"),
        "quot" => Some("\""),
        "apos" => Some("'"),
        "nbsp" => Some(" "),
        _ => None,
    }
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

pub(super) fn wait_for_hotkey_modifiers_up() {
    for _ in 0..30 {
        unsafe {
            let ctrl_down = GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000 != 0;
            let shift_down = GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000 != 0;
            let alt_down = GetAsyncKeyState(VK_MENU.0 as i32) as u16 & 0x8000 != 0;
            let lwin_down = GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000 != 0;
            let rwin_down = GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000 != 0;
            if !ctrl_down && !shift_down && !alt_down && !lwin_down && !rwin_down {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdi_clipboard_formats_are_not_treated_as_hglobal() {
        assert!(!clipboard_format_is_hglobal(2), "CF_BITMAP");
        assert!(!clipboard_format_is_hglobal(3), "CF_METAFILEPICT");
        assert!(!clipboard_format_is_hglobal(9), "CF_PALETTE");
        assert!(!clipboard_format_is_hglobal(14), "CF_ENHMETAFILE");
        assert!(!clipboard_format_is_hglobal(0x0082), "CF_DSPBITMAP");
        assert!(!clipboard_format_is_hglobal(0x0300), "CF_GDIOBJFIRST");
        assert!(!clipboard_format_is_hglobal(0x0200), "CF_PRIVATEFIRST");
    }

    fn cf_html(source_url: &str, fragment: &str) -> Vec<u8> {
        let html = format!(
            "<html><body>\r\n<!--StartFragment-->{fragment}<!--EndFragment-->\r\n</body></html>"
        );
        let dummy = format!(
            "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\nSourceURL:{source_url}\r\n",
            0, 0, 0, 0
        );
        let start_html = dummy.len();
        let marker = "<!--StartFragment-->";
        let start_fragment = start_html + html.find(marker).unwrap() + marker.len();
        let end_fragment = start_html + html.find("<!--EndFragment-->").unwrap();
        let end_html = start_html + html.len();
        let header = format!(
            "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\nSourceURL:{source_url}\r\n",
            start_html, end_html, start_fragment, end_fragment
        );
        assert_eq!(header.len(), dummy.len());
        let mut bytes = header.into_bytes();
        bytes.extend(html.into_bytes());
        bytes
    }

    #[test]
    fn foxmail_mail_frame_url_is_a_document_location() {
        assert!(is_document_location(
            "file:///C:/Users/77251/AppData/Roaming/Foxmail7/Temp-54432-20261101174538/mail_frame.html?foxhandler=MailReadProcessHandler&b=2"
        ));
        assert!(is_document_location("https://mail.example.com/read?id=1"));
        assert!(is_document_location("about:blank"));
        assert!(!is_document_location("Delivery to the following recipient failed"));
        assert!(!is_document_location("See https://example.com for details"));
    }

    #[test]
    fn clipboard_keeps_real_text_when_html_only_records_the_page_url() {
        let url = "file:///C:/Users/77251/AppData/Roaming/Foxmail7/Temp-54432-20261101174538/mail_frame.html?foxhandler=MailReadProcessHandler&b=2";
        let html = cf_html(url, "Delivery failed");
        assert_eq!(
            prefer_clipboard_text("Delivery failed", Some(&html)),
            "Delivery failed"
        );
    }

    #[test]
    fn clipboard_drops_foxmail_page_url_and_uses_html_fragment() {
        let url = "file:///C:/Users/77251/AppData/Roaming/Foxmail7/Temp-54432-20261101174538/mail_frame.html?foxhandler=MailReadProcessHandler&b=2";
        let html = cf_html(url, "hello <b>world</b><br>next &amp; more");
        assert_eq!(
            prefer_clipboard_text(url, Some(&html)),
            "hello world\nnext & more"
        );
    }

    #[test]
    fn selected_url_without_html_clipboard_is_kept() {
        assert_eq!(
            prefer_clipboard_text("https://example.com/path", None),
            "https://example.com/path"
        );
    }

    #[test]
    fn selected_link_is_not_replaced_by_the_page_url() {
        let page = "file:///C:/mail_frame.html?foxhandler=MailReadProcessHandler&b=2";
        let html = cf_html(page, "https://example.com/a");
        assert_eq!(
            prefer_clipboard_text("https://example.com/a", Some(&html)),
            "https://example.com/a"
        );
    }

    #[test]
    fn memory_clipboard_formats_are_hglobal() {
        assert!(clipboard_format_is_hglobal(1), "CF_TEXT");
        assert!(clipboard_format_is_hglobal(8), "CF_DIB");
        assert!(clipboard_format_is_hglobal(13), "CF_UNICODETEXT");
        assert!(clipboard_format_is_hglobal(15), "CF_HDROP");
        assert!(clipboard_format_is_hglobal(16), "CF_LOCALE");
        assert!(clipboard_format_is_hglobal(17), "CF_DIBV5");
        assert!(clipboard_format_is_hglobal(0xC0A0), "registered format");
    }
}
