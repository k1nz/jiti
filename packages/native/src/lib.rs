//! Jiti 原生壳。
//!
//! M0 职责（见 docs/architecture.md §15 M0）：
//! - 常驻后台（Accessory 策略 + 单实例 + 托盘退出路径）
//! - 全局热键 → 唤起与直达模式的隐藏面板（不重建窗口）
//! - WebView 存活三件套 + 首帧同步（§4.6 A.1/A.2）
//! - IPC 最小集（tauri-specta 单源契约，生成 TS 绑定）
//!
//! 架构决策冲突说明：目录使用 §10 文档树的 `packages/native`（= src-tauri），
//! 而非约定之外的顶层 `src-tauri`；CSP 仅 `self` 系（§9），IPC 走自定义命令。

mod commands;
mod ipc;
mod providers;
mod services;

use tauri::Manager as _;
use tauri_plugin_global_shortcut::ShortcutState;

pub fn run() {
    // M5.1：仅当构建时注入 JITI_SENTRY_DSN 才上报。guard 必须活过 app.run。
    let _crash_guard = services::crash::init();

    let specta = {
        let b = ipc::generate();
        #[cfg(debug_assertions)]
        ipc::export(&b);
        b
    };

    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        // 已注册热键：只“显示已创建的隐藏窗”，绝不按热键建窗
                        let mode = services::hotkeys::mode_for(shortcut);
                        services::panel::show_panel(app, mode);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 二次启动：唤起已有实例的面板
            services::panel::show_panel(app, services::panel::Mode::Panel);
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .invoke_handler(specta.invoke_handler())
        .setup(move |app| {
            // specta 事件（hotkey://pressed / capture://changed / engine://error）必须先挂上，
            // 否则 Event::emit 会 panic：EventRegistry not found。
            specta.mount_events(app);

            #[cfg(target_os = "macos")]
            services::panel::macos::apply_activation_policy(app.handle());

            #[cfg(target_os = "windows")]
            services::panel::windows::install_outside_click_dismiss(app.handle());

            services::hotkeys::register(app.handle())?;
            services::tray::setup(app)?;

            if let Some(win) = app.get_webview_window("main") {
                // 预热态：alpha=0 + 前置 + 忽略鼠标 + JS 空转 rAF（§4.6 A.1）。
                // macOS 13 无 windowOcclusionDetectionEnabled 私有键，见 services/panel.rs 决策说明。
                services::panel::prewarm(&win);
                services::panel::disable_browser_chrome(&win);
            }

            #[cfg(debug_assertions)]
            services::dev::maybe_autoshow(app);

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                services::panel::hide_panel(window.app_handle());
            }
            tauri::WindowEvent::Focused(focused) if window.label() == "main" => {
                services::panel::on_focus_changed(window.app_handle(), *focused);
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("error while building jiti tauri application");

    app.run(|_app_handle, _event| {});
}
