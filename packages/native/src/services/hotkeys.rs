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
    // 全局热键可能已被其他程序占用（RegisterHotKey 返回 ERROR_HOTKEY_ALREADY_REGISTERED）。
    // 逐个尝试，失败的跳过并告警 —— 不能让单个热键冲突拖垮整个应用启动。
    let candidates: [(Shortcut, &str); 3] = [
        (Shortcut::new(Some(mods()), Code::KeyT), "⌥⌘T（翻译）"),
        (Shortcut::new(Some(mods()), Code::KeyG), "⌥⌘G（语法）"),
        (Shortcut::new(Some(mods()), Code::Space), "⌥⌘Space（面板）"),
    ];
    for (shortcut, label) in candidates {
        if let Err(err) = gs.register(shortcut) {
            eprintln!("[jiti] 全局热键 {label} 注册失败，已跳过：{err}");
        }
    }
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