//! 独立复习课窗口：首次创建、再次聚焦、关闭即销毁（与设置窗同构）。
//!
//! Windows：`WebviewWindowBuilder::build` 不能在同步 IPC 或托盘/菜单回调里调用。

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const STUDY_LABEL: &str = "study";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StudyWindowAction {
    FocusExisting,
    Create,
}

pub fn next_action(exists: bool) -> StudyWindowAction {
    if exists {
        StudyWindowAction::FocusExisting
    } else {
        StudyWindowAction::Create
    }
}

pub fn open(app: &AppHandle) -> Result<(), String> {
    crate::services::panel::hide_panel(app);

    if next_action(app.get_webview_window(STUDY_LABEL).is_some())
        == StudyWindowAction::FocusExisting
    {
        if let Some(win) = app.get_webview_window(STUDY_LABEL) {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
        return Ok(());
    }

    WebviewWindowBuilder::new(app, STUDY_LABEL, WebviewUrl::App("study.html".into()))
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
        .map_err(|e| format!("无法打开复习课窗口：{e}"))?;
    if let Some(win) = app.get_webview_window(STUDY_LABEL) {
        crate::services::panel::disable_browser_chrome(&win);
    }
    Ok(())
}

pub fn open_detached(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("jiti-open-study".into())
        .spawn(move || {
            if let Err(err) = open(&app) {
                eprintln!("[jiti] open study: {err}");
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_existing_window() {
        assert_eq!(next_action(true), StudyWindowAction::FocusExisting);
        assert_eq!(next_action(false), StudyWindowAction::Create);
    }
}
