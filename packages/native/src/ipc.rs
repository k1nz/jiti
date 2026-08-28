//! IPC 契约（tauri-specta 单源）：Rust 命令签名 = TS 类型唯一事实源。
//! 生成的 `../ui/src/ipc/bindings.ts` 由 debug 构建自动导出，勿手改。

use tauri::Wry as TauriRuntime;
use tauri_specta::{collect_commands, collect_events, Builder as SpectaBuilder};

/// 命令以完整模块路径传入：tauri/specta 的辅助宏（`__cmd_*` / `__specta__fn_*`）
/// 走“函数所在模块的 pub use”路径解析（见 commands/panel.rs 等处的生成物），
/// 不允许用 `use crate::__cmd_*` 直接导入（Rust 对 proc-macro 生成宏的限制）。
pub fn generate() -> SpectaBuilder<TauriRuntime> {
    SpectaBuilder::<TauriRuntime>::new()
        .commands(collect_commands![
            crate::commands::panel::show_popup,
            crate::commands::panel::hide_popup,
            crate::commands::panel::set_panel_pinned,
            crate::commands::panel::panel_pinned,
            crate::commands::prefs::preferences_snapshot,
            crate::commands::prefs::preferences_update,
            crate::commands::prefs::open_settings,
            crate::commands::settings::settings_get,
            crate::commands::settings::settings_set,
            crate::commands::selection::get_selected_text,
            crate::commands::translate::translate,
            crate::commands::grammar::grammar_check,
            crate::commands::providers::providers_snapshot,
            crate::commands::providers::providers_save,
            crate::commands::providers::provider_save_api_key,
            crate::commands::providers::provider_test,
            crate::commands::history::history_list,
            crate::commands::history::history_clear,
            crate::commands::mistakes::mistakes_list,
            crate::commands::mistakes::mistakes_create,
            crate::commands::mistakes::mistakes_update,
            crate::commands::mistakes::mistakes_delete,
            crate::commands::mistakes::mistakes_preferences,
            crate::commands::mistakes::mistakes_set_preferences,
            crate::commands::mistakes::mistakes_export,
            crate::commands::mistakes::mistakes_ai_review,
            crate::commands::hotkeys::hotkeys_snapshot,
            crate::commands::hotkeys::hotkeys_set,
            crate::commands::hotkeys::hotkeys_reset,
            crate::commands::hotkeys::hotkeys_suspend,
            crate::commands::hotkeys::hotkeys_resume,
            crate::commands::accessibility::accessibility_status,
            crate::commands::accessibility::open_accessibility_settings,
            crate::commands::permissions::permissions_snapshot,
            crate::commands::permissions::request_permission,
            crate::commands::permissions::open_permission_settings,
            crate::commands::permissions::complete_onboarding,
            crate::commands::permissions::restart_app,
            crate::commands::updates::app_version,
            crate::commands::updates::check_for_updates,
            crate::commands::updates::open_external_url,
        ])
        .events(collect_events![
            crate::providers::error::EngineErrorEvent,
            crate::providers::enrich::TranslateEnrichedEvent,
            crate::services::selection::HotkeyPressedEvent,
            crate::services::selection::CaptureChangedEvent,
            crate::services::prefs::PreferencesChangedEvent,
        ])
}

/// 仅 debug 构建导出（M0 阶段前端必须能立即拿到绑定）。
/// specta 的类型图收集/序列化递归较深，测试线程默认栈不够 —— 放进大栈线程执行。
#[cfg(debug_assertions)]
pub fn export(builder: &SpectaBuilder<TauriRuntime>) {
    use specta_typescript::Typescript;
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/src/ipc/bindings.ts");
    let builder = builder.clone();
    std::thread::Builder::new()
        .name("jiti:specta-export".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            builder
                .export(Typescript::default(), &out)
                .expect("tauri-specta: failed to export bindings.ts");
        })
        .expect("failed to spawn specta exporter")
        .join()
        .expect("specta exporter panicked");
}

/// Request/Response 与 Event 是互不相交的两个形状（§3.5）。
/// M1 的 `engine://error` / `hotkey://pressed` / `capture://changed` / `translate://enriched` 已纳入 specta 单源事件；
/// M0 的 `panel://visibility` 保持普通 emit/listen 契约。
///
/// §3.5 要求的版本化常量：破坏性变更必升。SCHEMA 12 为词卡中英对照与加载态。
#[allow(dead_code)]
pub const SCHEMA_VERSION: u32 = 12;

#[cfg(test)]
mod tests {
    /// `cargo test` 时会重新导出 TS 绑定（与 `tauri dev` 启动时的导出幂等）。
    /// 保证前端 `pnpm typecheck` / `vitest` 在没跑过 GUI 的干净 checkout 上也能过。
    #[test]
    fn generate_typescript_bindings() {
        super::export(&super::generate());
    }
}
