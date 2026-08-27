//! 独立设置窗口：首次创建、再次聚焦、关闭即销毁（§4.1 / §10 / §13）。

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

pub fn open(app: &AppHandle) -> Result<(), String> {
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
        .inner_size(560.0, 720.0)
        .min_inner_size(420.0, 520.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_existing_window() {
        assert_eq!(next_action(true), SettingsWindowAction::FocusExisting);
        assert_eq!(next_action(false), SettingsWindowAction::Create);
    }
}
