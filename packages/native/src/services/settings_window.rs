//! 独立设置窗口：首次创建、再次聚焦、关闭即销毁（§4.1 / §10 / §13）。
//!
//! Windows：`WebviewWindowBuilder::build` 不能在同步 IPC 或托盘/菜单回调里调用，
//! 否则 WebView2 会死锁（白屏、标题栏关闭无响应）。见 Tauri 文档 Known issues。

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_LABEL: &str = "settings";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsWindowAction {
    FocusExisting,
    Create,
}

pub fn next_action(exists: bool) -> SettingsWindowAction {
    if exists {
        SettingsWindowAction::FocusExisting
    } else {
        SettingsWindowAction::Create
    }
}

/// 打开设置窗。调用方必须不在事件循环线程上：用 async 命令，或 [`open_detached`]。
pub fn open(app: &AppHandle) -> Result<(), String> {
    // 主面板 always-on-top，不先收起的话会盖住设置窗。
    crate::services::panel::hide_panel(app);

    if next_action(app.get_webview_window(SETTINGS_LABEL).is_some())
        == SettingsWindowAction::FocusExisting
    {
        if let Some(win) = app.get_webview_window(SETTINGS_LABEL) {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
        return Ok(());
    }

    WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
        .title("Jiti")
        .inner_size(840.0, 620.0)
        .min_inner_size(680.0, 480.0)
        .resizable(true)
        .maximizable(false)
        .minimizable(true)
        .decorations(true)
        .transparent(false)
        .always_on_top(false)
        .skip_taskbar(false)
        .visible(true)
        .focused(true)
        .build()
        .map_err(|e| format!("无法打开设置窗口：{e}"))?;
    if let Some(win) = app.get_webview_window(SETTINGS_LABEL) {
        crate::services::panel::disable_browser_chrome(&win);
    }
    Ok(())
}

/// 托盘等事件循环回调使用：扔到后台线程再 `build`，避免 Windows WebView2 死锁。
pub fn open_detached(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("jiti-open-settings".into())
        .spawn(move || {
            if let Err(err) = open(&app) {
                eprintln!("[jiti] open settings: {err}");
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_existing_window() {
        assert_eq!(next_action(true), SettingsWindowAction::FocusExisting);
        assert_eq!(next_action(false), SettingsWindowAction::Create);
    }
}
