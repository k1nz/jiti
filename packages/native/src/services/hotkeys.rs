//! 全局热键（tauri-plugin-global-shortcut，Carbon/RegisterHotKey 底层）。
//! 默认：⌥⌘T 翻译 / ⌥⌘G 语法 / ⌥⌘Space 统一面板（§7.3，可配置化在 M4）。

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use super::panel::Mode;

fn mods() -> Modifiers {
    Modifiers::META | Modifiers::ALT
}

pub fn register(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let gs = app.global_shortcut();
    gs.register(Shortcut::new(Some(mods()), Code::KeyT))?;
    gs.register(Shortcut::new(Some(mods()), Code::KeyG))?;
    gs.register(Shortcut::new(Some(mods()), Code::Space))?;
    Ok(())
}

/// 热键 → 模式（统一面板不指定具体 Tab，前端保留上次模式）。
pub fn mode_for(shortcut: &Shortcut) -> Mode {
    let m = mods();
    if shortcut.matches(&m, &Code::KeyT) {
        Mode::Translate
    } else if shortcut.matches(&m, &Code::KeyG) {
        Mode::Grammar
    } else {
        Mode::Panel
    }
}