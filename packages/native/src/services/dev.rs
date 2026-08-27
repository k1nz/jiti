//! 开发期自检钩子（仅 debug 构建）。
//!
//! 环境变量：
//! - `JITI_AUTOSHOW_MS`：启动后 N 毫秒自动唤起面板，与热键走完全同一代码路径，
//!   用于无辅助功能权限下的自测/截图。
//! - `JITI_CENTER`：配合上者，把面板居中（方便截图对齐），默认跟随光标。

#[cfg(debug_assertions)]
use tauri::Manager as _;

#[cfg(debug_assertions)]
pub fn maybe_autoshow(app: &tauri::App) {
    let Ok(ms_str) = std::env::var("JITI_AUTOSHOW_MS") else {
        return;
    };
    let Ok(ms) = ms_str.parse::<u64>() else {
        return;
    };
    let center = std::env::var("JITI_CENTER").is_ok();

    let handle = app.handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let inner = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Some(win) = inner.get_webview_window("main") {
                if center {
                    let _ = win.center();
                }
            }
            super::panel::show_panel_inner(&inner, super::panel::Mode::Panel, !center);
        });
    });
}
