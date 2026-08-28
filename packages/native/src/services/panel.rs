//! 面板窗口服务：显示 / 隐藏 / 定位 / 焦点语义。
//!
//! M0 铁律（§4.1 / 验收红线）：
//! - 热键只触发“显示已创建的隐藏窗”，绝不按热键建窗；
//! - 关闭 = 隐藏（alpha=0 + 忽略鼠标），不销毁进程/SQLite 之外的任何东西；
//! - 显示默认抢焦点（`set_focus`）；设置里可关掉，改走 macOS `orderFrontRegardless` / Windows `show()` 不激活路径。
//!
//! 失焦关闭：默认未固定时，真正失去焦点或点到其他 App 即隐藏；图钉固定后保持打开。
//! Windows：装饰区 start_dragging 的伪失焦不关窗；另有外点监视补齐「已失焦后再点外部」路径。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_specta::Event;

/// 面板四边圆角（12px，贴近系统浮层；透明窗由 CSS + 原生 layer 共同裁剪）。
pub const PANEL_CORNER_RADIUS: f64 = 12.0;

/// 显示后短暂忽略失焦/外点，避免 orderFront 路径上的伪 Focused(false) 立刻把窗关掉。
const BLUR_GRACE: Duration = Duration::from_millis(250);

static PINNED: AtomicBool = AtomicBool::new(false);
static SHOWN: AtomicBool = AtomicBool::new(false);
static HAD_FOCUS: AtomicBool = AtomicBool::new(false);
static IGNORE_BLUR_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

/// 开发期遥测（§13 感知目标：热键→弹窗暖启 <30ms）：
/// show_panel_inner 记录发起时刻，macos::reveal 翻转 alpha 时打印耗时。
#[cfg(debug_assertions)]
static SHOW_STAMP: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

#[cfg(debug_assertions)]
fn stamp_show() {
    if let Ok(mut g) = SHOW_STAMP.lock() {
        *g = Some(std::time::Instant::now());
    }
}

#[cfg(debug_assertions)]
#[allow(dead_code)] // 仅在 macOS reveal 路径使用；Windows 构建下避免死代码告警
fn stamp_reveal() {
    if let Ok(g) = SHOW_STAMP.try_lock() {
        if let Some(t) = *g {
            eprintln!(
                "[jiti] panel revealed after {:.1} ms since show",
                t.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}

/// 面板模式（热键直达；统一面板保留前端上次模式）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Translate,
    Grammar,
    Panel,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Translate => "translate",
            Mode::Grammar => "grammar",
            Mode::Panel => "panel",
        }
    }

    /// 只有直达翻译/语法才读选中；托盘「显示面板」不模拟 Cmd+C。
    pub fn captures_selection(self) -> bool {
        matches!(self, Mode::Translate | Mode::Grammar)
    }

    pub fn from_opt(s: Option<&str>) -> Self {
        match s {
            Some("translate") => Mode::Translate,
            Some("grammar") => Mode::Grammar,
            _ => Mode::Panel,
        }
    }
}

/// 显示面板：定位到光标附近；是否抢焦点由 `prefs.focusOnInvoke` 决定（默认抢）。
pub fn show_panel(app: &AppHandle, mode: Mode) {
    show_panel_inner(app, mode, true);
}

/// 窗口操作必须落在主线程（AppKit 只允许主线程触碰 NSWindow）。
/// follow_cursor=false 时可与上游 center() 组合，用于开发自检（截图）。
pub(crate) fn show_panel_inner(app: &AppHandle, mode: Mode, follow_cursor: bool) {
    #[cfg(debug_assertions)]
    stamp_show();
    let app = app.clone();
    let inner = app.clone();
    #[cfg(debug_assertions)]
    eprintln!(
        "[jiti] show_panel mode={} follow_cursor={}",
        mode.as_str(),
        follow_cursor
    );
    let _ = app.run_on_main_thread(move || {
        // 显示前在主线程读选中（§3.4）。reveal 之后 AX 焦点可能已经不在原 App。
        // 托盘「显示面板」不走捕获：剪贴板回写过期 NSPasteboardItem 会抛 ObjC 异常并 abort。
        let capture = mode.captures_selection();
        let epoch = if capture {
            crate::services::selection::begin_capture()
        } else {
            0
        };
        let selection = if capture {
            crate::services::selection::read_preferred()
        } else {
            crate::services::selection::SelectedText::empty()
        };
        // 先记下源 PID 并开始等修饰键，再 reveal，避免剪贴板复制打到面板自己。
        if capture {
            crate::services::selection::begin_clipboard_fallback(inner.clone(), epoch);
        }
        if let Some(win) = inner.get_webview_window("main") {
            if follow_cursor {
                position_near_cursor(&win);
            }
            apply_panel_material(&win);
            apply_corner_radius(&win);
            let _ = win.set_resizable(false);
            reveal(&win);
            mark_shown();
            if crate::services::prefs::load_prefs(&inner).focus_on_invoke {
                request_panel_focus(&win, capture);
            }
        }
        let _ = crate::services::selection::HotkeyPressedEvent {
            mode: mode.as_str().to_string(),
            selection,
            epoch,
        }
        .emit(&inner);
        let _ = inner.emit("panel://visibility", "shown");
    });
}

/// 隐藏面板：alpha=0 + 忽略鼠标 + 归还焦点。
pub fn hide_panel(app: &AppHandle) {
    #[cfg(debug_assertions)]
    eprintln!("[jiti] hide_panel");
    let app = app.clone();
    let inner = app.clone();
    let _ = app.run_on_main_thread(move || {
        mark_hidden();
        if let Some(win) = inner.get_webview_window("main") {
            conceal(&win);
        }
        let _ = inner.emit("panel://visibility", "hidden");
    });
}

/// 用户图钉：固定后失焦 / 点到其他 App 不再自动隐藏。
pub fn is_pinned() -> bool {
    PINNED.load(Ordering::SeqCst)
}

pub fn set_pinned(pinned: bool) -> bool {
    PINNED.store(pinned, Ordering::SeqCst);
    pinned
}

/// 窗口焦点变化：从未成为 key 的伪失焦忽略；固定态不关。
pub fn on_focus_changed(app: &AppHandle, focused: bool) {
    if focused {
        HAD_FOCUS.store(true, Ordering::SeqCst);
        return;
    }
    // Windows：空白装饰区 `data-tauri-drag-region` → start_dragging 会在主键按下时抛出
    // Focused(false)。此时若光标仍在面板内，视为拖窗伪失焦，不隐藏。
    #[cfg(target_os = "windows")]
    if let Some(win) = app.get_webview_window("main") {
        if windows::is_chrome_drag_blur(&win) {
            return;
        }
    }
    let had = HAD_FOCUS.swap(false, Ordering::SeqCst);
    if should_dismiss_on_blur(
        SHOWN.load(Ordering::SeqCst),
        is_pinned(),
        had,
        in_blur_grace(),
    ) {
        hide_panel(app);
    }
}

fn mark_shown() {
    SHOWN.store(true, Ordering::SeqCst);
    arm_blur_grace();
}

fn mark_hidden() {
    SHOWN.store(false, Ordering::SeqCst);
    HAD_FOCUS.store(false, Ordering::SeqCst);
}

fn arm_blur_grace() {
    if let Ok(mut g) = IGNORE_BLUR_UNTIL.lock() {
        *g = Some(Instant::now() + BLUR_GRACE);
    }
}

fn in_blur_grace() -> bool {
    IGNORE_BLUR_UNTIL
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_some_and(|until| Instant::now() < until)
}

pub(crate) fn should_dismiss_on_blur(
    shown: bool,
    pinned: bool,
    had_focus: bool,
    in_grace: bool,
) -> bool {
    shown && !pinned && had_focus && !in_grace
}

pub(crate) fn should_dismiss_on_click_outside(shown: bool, pinned: bool, in_grace: bool) -> bool {
    shown && !pinned && !in_grace
}

/// 物理坐标点是否落在窗口外接矩形内（含左边/顶边，不含右边/底边）。
pub(crate) fn point_in_physical_rect(
    px: i32,
    py: i32,
    rect_x: i32,
    rect_y: i32,
    rect_w: u32,
    rect_h: u32,
) -> bool {
    let w = rect_w as i32;
    let h = rect_h as i32;
    px >= rect_x && py >= rect_y && px < rect_x + w && py < rect_y + h
}

fn apply_corner_radius(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::apply_corner_radius(win);
    #[cfg(not(target_os = "macos"))]
    let _ = (win, PANEL_CORNER_RADIUS);
}

/// 主面板毛玻璃：macOS HudWindow vibrancy，Windows Acrylic（失败则 Blur）。
/// 设置窗不走这里。失败时忽略——CSS 半透明 tint 仍可读。
fn apply_panel_material(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        let _ = apply_vibrancy(
            win,
            NSVisualEffectMaterial::HudWindow,
            Some(NSVisualEffectState::Active),
            Some(PANEL_CORNER_RADIUS),
        );
    }
    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_acrylic, apply_blur};
        if apply_acrylic(win, Some((18, 18, 18, 90))).is_err() {
            let _ = apply_blur(win, Some((18, 18, 18, 90)));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = win;
}

/// 预热态（首次页载完成后调用一次）：alpha=0 前置 + 忽略鼠标。
/// WebKit 因 occlusion 检测被关闭 + 前置可见而维持满帧预算，
/// JS 侧再以空转 rAF 保活（§4.6 A.1）。
pub fn prewarm(win: &WebviewWindow) {
    let _ = win.set_resizable(false);
    apply_panel_material(win);
    #[cfg(target_os = "macos")]
    macos::prewarm(win);
}

/// 跟随光标定位，并限制在包含光标的屏幕可见工作区内（§4.4 默认策略）。
fn position_near_cursor(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::position_near_cursor(win);
    #[cfg(target_os = "windows")]
    windows::position_near_cursor(win);
}

/// 光标右下方出现，并夹取到工作区。坐标与窗口系统同原点（左上或已换算）。
#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn clamp_panel_position(
    cursor_x: f64,
    cursor_y: f64,
    panel_w: f64,
    panel_h: f64,
    work_x: f64,
    work_y: f64,
    work_w: f64,
    work_h: f64,
    offset: f64,
    margin: f64,
) -> (f64, f64) {
    let x_raw = cursor_x + offset;
    let y_raw = cursor_y + offset;
    let x_max = (work_x + work_w - panel_w - margin).max(work_x + margin);
    let y_max = (work_y + work_h - panel_h - margin).max(work_y + margin);
    (
        x_raw.clamp(work_x + margin, x_max),
        y_raw.clamp(work_y + margin, y_max),
    )
}

/// 去掉 WebView 浏览器右键菜单 / 链接预览 / 字典查询。
pub fn disable_browser_chrome(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::disable_browser_chrome(win);
    #[cfg(target_os = "windows")]
    windows::disable_browser_chrome(win);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = win;
}

fn reveal(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::reveal(win);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = win.show();
    }
}

/// 把键盘交给面板。须在 `reveal` 之后调用：macOS 的 `set_focus` 要求窗口已可见。
/// 翻译/语法热键要等剪贴板模拟复制打到源窗口之后再抢前台，否则源应用失焦、选区被清，
/// 兜底会复制空内容或面板自己。
fn request_panel_focus(win: &WebviewWindow, capturing: bool) {
    if should_defer_focus_for_clipboard(capturing) {
        schedule_deferred_focus(win);
        return;
    }
    let _ = win.set_focus();
}

pub(crate) fn should_defer_focus_for_clipboard(capturing: bool) -> bool {
    capturing
}

fn schedule_deferred_focus(win: &WebviewWindow) {
    let win = win.clone();
    std::thread::spawn(move || {
        crate::services::selection::wait_for_hotkey_modifiers_up();
        // 剪贴板路径在修饰键松开后立刻发复制，再等 ~180ms 读结果。
        std::thread::sleep(Duration::from_millis(320));
        let app = win.app_handle().clone();
        let focused = win.clone();
        let _ = app.run_on_main_thread(move || {
            if !SHOWN.load(Ordering::SeqCst) {
                return;
            }
            let _ = focused.set_focus();
        });
    });
}

fn conceal(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    macos::conceal(win);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = win.hide();
    }
}

#[cfg(target_os = "macos")]
pub mod macos {
    //! macOS 原生缝（objc2）：渲染表面以下的一切 —— Accessory / 防节流 / 无焦点显示 / 首帧同步。

    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::MainThreadMarker;
    use objc2::{msg_send, sel};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSEvent, NSScreen, NSView, NSWindow,
        NSWindowStyleMask,
    };
    use tauri::{LogicalPosition, Manager, WebviewWindow};

    static PREWARMED: AtomicBool = AtomicBool::new(false);

    /// 由 tauri 的窗口句柄取得 `&NSWindow`。
    /// 指针由运行期持有、生命周期等于本窗口存活期，集中在此做唯一一次 unsafe 解引用。
    fn window<'a>(win: &'a WebviewWindow) -> Option<&'a NSWindow> {
        let raw = win.ns_window().ok()?;
        Some(unsafe { &*(raw as *const NSWindow) })
    }

    /// 无 Dock 图标后台常驻（NSApplicationActivationPolicyAccessory）。
    pub fn apply_activation_policy(app: &tauri::AppHandle) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        NSApplication::sharedApplication(mtm)
            .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        install_outside_click_dismiss(app);
    }

    /// 架构决策说明（§4.6 A.1 在 macOS 13 的落地）：
    /// - skill 文档里 `window.setValue(false, forKey:"windowOcclusionDetectionEnabled")`
    ///   在本机 macOS 13.7 上不成立 —— 该 KVC 键在 NSWindow 上不存在，
    ///   调用 `setValue:forKey:` 会抛 NSUnknownKeyException 并导致 objc2 边界 panic abort。
    ///   因此不设置此键（macOS 14+ 如果该私有键存在可择机补回，需包 autoreleasepool）。
    /// - A.1 的核心目标是「WebKit 不因隐藏/遮挡而降频」。本实现令窗口在预热期与"隐藏"期
    ///   始终保持 **在屏且 `isVisible == true`**（alpha=0 + orderFrontRegardless + 忽略鼠标），
    ///   不 `orderOut`，因此 WebKit 恒判可见，rAF 预算不被回收 —— 无需该私有键。
    /// - `drawsBackground=false` 由 wry 在 `transparent: true` 时自行设置（wry 官方行为），
    ///   不再重复 KVC（KVC 写在本 app setup 阶段、无 autorelease pool 的时序下也不安全）。
    ///
    /// 这里没有额外的窗口 flag 需要设置；预热工作全部在 `prewarm` / `reveal` / `conceal`。

    /// 预热：alpha=0 前置 + 忽略鼠标。窗口在屏（不可见）→ WebKit 持续分配帧预算。
    pub fn prewarm(win: &WebviewWindow) {
        if PREWARMED.swap(true, Ordering::SeqCst) {
            return;
        }
        if MainThreadMarker::new().is_none() {
            return; // 只在主线程触碰 AppKit
        }
        let Some(window) = window(win) else {
            return;
        };
        apply_corner_radius(win);
        disable_resize(window);
        window.setAlphaValue(0.0);
        window.setIgnoresMouseEvents(true);
        window.orderFrontRegardless();
    }

    /// 透明无边框窗：把 contentView layer 裁成 12px 圆角，四边一致。
    pub fn apply_corner_radius(win: &WebviewWindow) {
        if MainThreadMarker::new().is_none() {
            return;
        }
        let Some(ns_window) = window(win) else {
            return;
        };
        let Some(content) = ns_window.contentView() else {
            return;
        };
        content.setWantsLayer(true);
        unsafe {
            let view = &*content as *const NSView as *mut AnyObject;
            let layer: *mut AnyObject = msg_send![view, layer];
            if !layer.is_null() {
                let _: () = msg_send![layer, setCornerRadius: super::PANEL_CORNER_RADIUS];
                let _: () = msg_send![layer, setMasksToBounds: true];
            }
        }
        ns_window.invalidateShadow();
    }

    fn disable_resize(window: &NSWindow) {
        window.setStyleMask(window.styleMask().difference(NSWindowStyleMask::Resizable));
        window.setMovable(true);
    }

    /// 点到其他 App 时关闭（面板默认不抢焦点，仅靠 Focused(false) 不够）。
    pub fn install_outside_click_dismiss(app: &tauri::AppHandle) {
        use std::ptr::NonNull;

        use objc2_app_kit::NSEventMask;

        static INSTALLED: AtomicBool = AtomicBool::new(false);
        if INSTALLED.load(Ordering::SeqCst) || MainThreadMarker::new().is_none() {
            return;
        }

        let app = app.clone();
        let block = block2::RcBlock::new(move |_event: NonNull<NSEvent>| {
            if !super::should_dismiss_on_click_outside(
                super::SHOWN.load(Ordering::SeqCst),
                super::is_pinned(),
                super::in_blur_grace(),
            ) {
                return;
            }
            super::hide_panel(&app);
        });
        let Some(monitor) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown.union(NSEventMask::RightMouseDown),
            &block,
        ) else {
            return;
        };
        INSTALLED.store(true, Ordering::SeqCst);
        // AppKit 监视器持有 handler；两者都要活过进程寿命，且 Retained 不是 Sync。
        std::mem::forget(monitor);
        std::mem::forget(block);
    }

    /// 显示：orderFrontRegardless（不激活）+ 首帧同步后再把 alpha 翻到 1（§4.6 A.2）。
    pub fn reveal(win: &WebviewWindow) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some(window) = window(win) else {
            return;
        };
        disable_resize(window);
        window.orderFrontRegardless();

        // 首帧同步：下一帧呈现后执行 block（私有但稳定 ~10 年的 API）。
        let raw2 = win.ns_window().expect("panel window handle") as *mut AnyObject;
        let block = block2::RcBlock::new(move || {
            // block 闭包独立作用域，这里的 unsafe 只覆盖指针解引用
            unsafe {
                let w = &*(raw2 as *const NSWindow);
                w.setAlphaValue(1.0);
                w.setIgnoresMouseEvents(false);
            }
            #[cfg(debug_assertions)]
            super::stamp_reveal();
        });

        // 从窗口 contentView（WryWebViewParent）找到真正的 WKWebView 再发首帧同步 selector。
        // 层级：NSWindow → WryWebViewParent(contentView) → WKWebView。
        let webview = if let Ok(ns_view) = win.ns_view() {
            find_wkwebview(ns_view as *mut AnyObject, mtm)
        } else {
            None
        };

        if let Some(webview) = webview {
            unsafe {
                let _: *mut AnyObject = msg_send![
                    webview,
                    performSelector: sel!(_doAfterNextPresentationUpdate:),
                    withObject: &*block as *const _ as *mut AnyObject
                ];
            }
        } else {
            // WKWebView 未找到时退化为直接显示（由 fallback 再兜底一次）
            window.setAlphaValue(1.0);
            window.setIgnoresMouseEvents(false);
        }

        // 兜底：私有 API 万一不触发，100ms 后强制显示（即使 alpha 已 1 也无副作用）。
        force_reveal_fallback(win.app_handle());
    }

    /// 在给定 contentView 上向下找 WKWebView（一层或自身）。
    fn find_wkwebview(ns_view: *mut AnyObject, _mtm: MainThreadMarker) -> Option<*mut AnyObject> {
        use objc2_app_kit::NSView;
        use objc2_foundation::{NSClassFromString, NSString};
        unsafe {
            let target = NSClassFromString(&NSString::from_str("WKWebView"))?;
            let content_view = &*(ns_view as *const NSView);
            if is_kind_of_class(ns_view, target) {
                return Some(ns_view);
            }
            for sub in content_view.subviews().iter() {
                let ptr = &*sub as *const NSView as *mut AnyObject;
                if is_kind_of_class(ptr, target) {
                    return Some(ptr);
                }
            }
            None
        }
    }

    fn is_kind_of_class(view: *mut AnyObject, cls: &AnyClass) -> bool {
        unsafe {
            let is_kind: bool =
                msg_send![view, isKindOfClass: cls as *const AnyClass as *mut AnyObject];
            is_kind
        }
    }

    fn force_reveal_fallback(app: &tauri::AppHandle) {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            let inner = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Some(win) = inner.get_webview_window("main") {
                    let Some(window) = window(&win) else {
                        return;
                    };
                    window.setAlphaValue(1.0);
                    window.setIgnoresMouseEvents(false);
                }
                #[cfg(debug_assertions)]
                super::stamp_reveal();
            });
        });
    }

    /// 隐藏：alpha=0 + 忽略鼠标 + 若面板正持有 key 则归还给原应用。
    /// 窗口保持在屏，重显只需一帧 alpha（<30ms 暖启）。
    pub fn conceal(win: &WebviewWindow) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Ok(raw) = win.ns_window() else {
            return;
        };
        // 承认失焦：如果 key 还在本面板，交还给原应用
        if let Some(key) = NSApplication::sharedApplication(mtm).keyWindow() {
            if &*key as *const NSWindow as *const c_void == raw {
                key.resignKeyWindow();
            }
        }
        let window = unsafe { &*(raw as *const NSWindow) };
        window.setAlphaValue(0.0);
        window.setIgnoresMouseEvents(true);
    }

    /// 跟随光标定位并钳制在可见工作区内（左上原点换算）。
    pub fn position_near_cursor(win: &WebviewWindow) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let mouse = NSEvent::mouseLocation(); // 左下原点，全局坐标
        let screens = NSScreen::screens(mtm);

        let mut frame: Option<objc2_foundation::NSRect> = None;
        let mut vis: Option<objc2_foundation::NSRect> = None;
        for screen in screens.iter() {
            let f = screen.frame();
            if mouse.x >= f.origin.x
                && mouse.x < f.origin.x + f.size.width
                && mouse.y >= f.origin.y
                && mouse.y < f.origin.y + f.size.height
            {
                frame = Some(f);
                vis = Some(screen.visibleFrame());
                break;
            }
        }
        let (Some(frame), Some(vis)) = (frame.take(), vis.take()) else {
            return;
        };

        let (ww, wh) = win
            .inner_size()
            .map(|s| (s.width as f64, s.height as f64))
            .unwrap_or((640.0, 440.0));

        // 光标坐标（左下原点）→ 左上原点
        let top_off = (frame.origin.y + frame.size.height) - mouse.y;

        // 窗口出现在光标右下方
        let x_raw = mouse.x + 16.0;
        let y_raw = top_off + 16.0;

        // 钳制到可见区
        let v_top = vis.origin.y + vis.size.height;
        let v_bottom = vis.origin.y;

        let x_max = (vis.origin.x + vis.size.width - ww - 8.0).max(vis.origin.x + 8.0);
        let y_max = (v_top - wh - 8.0).max(v_bottom + 8.0);
        let x = x_raw.clamp(vis.origin.x + 8.0, x_max);
        let y = y_raw.clamp(v_bottom + 8.0, y_max);

        let _ = win.set_position(LogicalPosition::new(x, y));
    }

    /// 关掉链接预览 / force-touch 词典；右键菜单由前端 preventDefault 再拦一层。
    pub fn disable_browser_chrome(win: &WebviewWindow) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Ok(ns_view) = win.ns_view() else {
            return;
        };
        let Some(webview) = find_wkwebview(ns_view as *mut AnyObject, mtm) else {
            return;
        };
        unsafe {
            let _: () = msg_send![webview, setAllowsLinkPreview: false];
        }
    }
}

#[cfg(target_os = "windows")]
pub mod windows {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use tauri::{AppHandle, Manager, PhysicalPosition, WebviewWindow};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    pub fn position_near_cursor(win: &WebviewWindow) {
        let mut pt = POINT::default();
        if unsafe { GetCursorPos(&mut pt) }.is_err() {
            return;
        }
        let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return;
        }
        let work = info.rcWork;
        let (ww, wh) = win
            .inner_size()
            .map(|s| (s.width as f64, s.height as f64))
            .unwrap_or((640.0, 440.0));
        let (x, y) = super::clamp_panel_position(
            pt.x as f64,
            pt.y as f64,
            ww,
            wh,
            work.left as f64,
            work.top as f64,
            (work.right - work.left) as f64,
            (work.bottom - work.top) as f64,
            16.0,
            8.0,
        );
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }

    fn mouse_button_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
        unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 }
    }

    /// 光标是否仍在面板外接矩形内。
    pub fn cursor_inside_panel(win: &WebviewWindow) -> bool {
        let mut pt = POINT::default();
        if unsafe { GetCursorPos(&mut pt) }.is_err() {
            return false;
        }
        let Ok(pos) = win.outer_position() else {
            return false;
        };
        let Ok(size) = win.outer_size() else {
            return false;
        };
        super::point_in_physical_rect(pt.x, pt.y, pos.x, pos.y, size.width, size.height)
    }

    /// start_dragging 触发的伪失焦：主键按下且光标仍在面板上。
    pub fn is_chrome_drag_blur(win: &WebviewWindow) -> bool {
        mouse_button_down(VK_LBUTTON) && cursor_inside_panel(win)
    }

    /// 点到其他窗口时关闭。拖空白装饰区会先失焦，仅靠 Focused(false) 不够（已失焦后再点外部不会再收到失焦）。
    pub fn install_outside_click_dismiss(app: &AppHandle) {
        static INSTALLED: AtomicBool = AtomicBool::new(false);
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        let app = app.clone();
        std::thread::Builder::new()
            .name("jiti-win-outside-click".into())
            .spawn(move || {
                let mut was_down = false;
                loop {
                    let down = mouse_button_down(VK_LBUTTON) || mouse_button_down(VK_RBUTTON);
                    if down && !was_down {
                        let app_for_main = app.clone();
                        let _ = app.run_on_main_thread(move || {
                            if !super::should_dismiss_on_click_outside(
                                super::SHOWN.load(Ordering::SeqCst),
                                super::is_pinned(),
                                super::in_blur_grace(),
                            ) {
                                return;
                            }
                            let Some(win) = app_for_main.get_webview_window("main") else {
                                return;
                            };
                            if cursor_inside_panel(&win) {
                                return;
                            }
                            super::hide_panel(&app_for_main);
                        });
                    }
                    was_down = down;
                    std::thread::sleep(Duration::from_millis(16));
                }
            })
            .ok();
    }

    pub fn disable_browser_chrome(win: &WebviewWindow) {
        let _ = win.with_webview(|webview| {
            disable_webview2_chrome(webview);
        });
    }

    fn disable_webview2_chrome(webview: tauri::webview::PlatformWebview) {
        unsafe {
            let Ok(core) = webview.controller().CoreWebView2() else {
                return;
            };
            let Ok(settings) = core.Settings() else {
                return;
            };
            let _ = settings.SetAreDefaultContextMenusEnabled(false);
            let _ = settings.SetIsStatusBarEnabled(false);
            let _ = settings.SetIsZoomControlEnabled(false);
            #[cfg(not(debug_assertions))]
            let _ = settings.SetAreDevToolsEnabled(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_from_opt_defaults_to_panel() {
        assert_eq!(Mode::from_opt(None), Mode::Panel);
        assert_eq!(Mode::from_opt(Some("panel")), Mode::Panel);
        assert_eq!(Mode::from_opt(Some("unknown")), Mode::Panel);
    }

    #[test]
    fn mode_from_opt_maps_direct_modes() {
        assert_eq!(Mode::from_opt(Some("translate")), Mode::Translate);
        assert_eq!(Mode::from_opt(Some("grammar")), Mode::Grammar);
    }

    #[test]
    fn mode_as_str_is_stable_contract() {
        // 这部分字符串同时是事件负载 / 前端 Tab 的契约键，不能随意改
        assert_eq!(Mode::Translate.as_str(), "translate");
        assert_eq!(Mode::Grammar.as_str(), "grammar");
        assert_eq!(Mode::Panel.as_str(), "panel");
    }

    #[test]
    fn tray_panel_does_not_capture_selection() {
        assert!(Mode::Translate.captures_selection());
        assert!(Mode::Grammar.captures_selection());
        assert!(!Mode::Panel.captures_selection());
    }

    #[test]
    fn translate_hotkey_defers_focus_for_clipboard() {
        assert!(should_defer_focus_for_clipboard(true));
        assert!(!should_defer_focus_for_clipboard(false));
    }

    #[test]
    fn blur_hides_only_after_real_focus_when_unpinned() {
        assert!(should_dismiss_on_blur(true, false, true, false));
        assert!(!should_dismiss_on_blur(true, true, true, false));
        assert!(!should_dismiss_on_blur(false, false, true, false));
        assert!(!should_dismiss_on_blur(true, false, false, false));
        assert!(!should_dismiss_on_blur(true, false, true, true));
    }

    #[test]
    fn click_outside_hides_when_shown_and_unpinned() {
        assert!(should_dismiss_on_click_outside(true, false, false));
        assert!(!should_dismiss_on_click_outside(true, true, false));
        assert!(!should_dismiss_on_click_outside(false, false, false));
        assert!(!should_dismiss_on_click_outside(true, false, true));
    }

    #[test]
    fn point_in_physical_rect_covers_bounds() {
        assert!(point_in_physical_rect(10, 20, 10, 20, 100, 50));
        assert!(point_in_physical_rect(109, 69, 10, 20, 100, 50));
        assert!(!point_in_physical_rect(9, 20, 10, 20, 100, 50));
        assert!(!point_in_physical_rect(10, 19, 10, 20, 100, 50));
        assert!(!point_in_physical_rect(110, 20, 10, 20, 100, 50));
        assert!(!point_in_physical_rect(10, 70, 10, 20, 100, 50));
    }

    #[test]
    fn clamp_keeps_panel_inside_work_area() {
        let (x, y) =
            clamp_panel_position(0.0, 0.0, 200.0, 100.0, 0.0, 0.0, 1000.0, 800.0, 16.0, 8.0);
        assert_eq!((x, y), (16.0, 16.0));
        let (x, y) = clamp_panel_position(
            980.0, 760.0, 200.0, 100.0, 0.0, 0.0, 1000.0, 800.0, 16.0, 8.0,
        );
        assert!(x <= 1000.0 - 200.0 - 8.0);
        assert!(y <= 800.0 - 100.0 - 8.0);
        assert!(x >= 8.0);
        assert!(y >= 8.0);
    }
}
