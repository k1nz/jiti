//! 托盘：退出路径（Accessory 策略下无 Dock 图标 + 无菜单栏）。
//! macOS 用 NSStatusItem；Windows 用系统托盘（M1+ 适配）。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;

use super::panel::Mode;

pub fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "显示面板", true, None::<&str>)?;
    let quit = PredefinedMenuItem::quit(app, Some("退出"))?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let Some(icon) = app.default_window_icon().cloned() else {
        // 配置缺图标时不建托盘（避免空白图标）；正常工程 icon 一定存在
        return Ok(());
    };

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Jiti")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => crate::services::panel::show_panel(app, Mode::Panel),
            _ => {}
        })
        .build(app)?;

    Ok(())
}