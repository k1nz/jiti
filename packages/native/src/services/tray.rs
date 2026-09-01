//! 托盘：退出路径（Accessory 策略下无 Dock 图标 + 无菜单栏）。
//! macOS 用 NSStatusItem；Windows 用系统托盘。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle};

use super::panel::Mode;
use super::prefs::{self, UiLocale};

const TRAY_ID: &str = "main";

pub fn setup(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let locale = prefs::resolved_locale(prefs::load_prefs(app.handle()).locale);
    let menu = build_menu(app.handle(), locale)?;

    let Some(icon) = app.default_window_icon().cloned() else {
        // 配置缺图标时不建托盘（避免空白图标）；正常工程 icon 一定存在
        return Ok(());
    };

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Jiti")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => crate::services::panel::show_panel(app, Mode::Panel),
            "study" => {
                crate::services::study_window::open_detached(app);
            }
            "settings" => {
                crate::services::settings_window::open_detached(app);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

pub fn refresh_labels(app: &AppHandle, locale: UiLocale) {
    let resolved = prefs::resolved_locale(locale);
    let Ok(menu) = build_menu(app, resolved) else {
        return;
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_menu(app: &AppHandle, locale: UiLocale) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let (show, study, settings, quit) = labels(locale);
    let show = MenuItem::with_id(app, "show", show, true, None::<&str>)?;
    let study = MenuItem::with_id(app, "study", study, true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", settings, true, None::<&str>)?;
    let quit = PredefinedMenuItem::quit(app, Some(quit))?;
    Menu::with_items(app, &[&show, &study, &settings, &quit])
}

fn labels(locale: UiLocale) -> (&'static str, &'static str, &'static str, &'static str) {
    match locale {
        UiLocale::EnUs => ("Show Panel", "Review", "Settings", "Quit"),
        UiLocale::ZhCn | UiLocale::System => ("显示面板", "复习课", "设置", "退出"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_tray_labels() {
        assert_eq!(
            labels(UiLocale::EnUs),
            ("Show Panel", "Review", "Settings", "Quit")
        );
    }

    #[test]
    fn chinese_tray_labels() {
        assert_eq!(
            labels(UiLocale::ZhCn),
            ("显示面板", "复习课", "设置", "退出")
        );
    }
}
