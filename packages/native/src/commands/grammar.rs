//! 语法检查命令：输入校验、Channel 转发、耗时与历史写入。

use tauri::ipc::Channel;
use tauri::AppHandle;

use crate::providers::grammar::{run_grammar, GrammarProgressEvent, GrammarRequest, GrammarResult};
use crate::providers::EngineErrorPayload;
use crate::services::history::{self, NewHistoryEntry};

#[tauri::command]
#[specta::specta]
pub async fn grammar_check(
    app: AppHandle,
    request: GrammarRequest,
    on_progress: Channel<GrammarProgressEvent>,
) -> Result<GrammarResult, EngineErrorPayload> {
    let mut emit = |event: GrammarProgressEvent| {
        let _ = on_progress.send(event);
    };
    match run_grammar(&app, request, &mut emit).await {
        Ok(result) => {
            let should_write = crate::services::settings::load_provider_config(&app)
                .map(|c| c.write_history)
                .unwrap_or(true);
            if should_write {
                let meta = serde_json::to_string(&result).ok();
                let _ = history::record(
                    &app,
                    NewHistoryEntry {
                        kind: "grammar".into(),
                        input: result.input.clone(),
                        output: result.corrected_text.clone().unwrap_or_default(),
                        engine: result.engine.clone(),
                        from_lang: Some("en".into()),
                        to_lang: None,
                        duration_ms: result.duration_ms as i64,
                        source_app: None,
                        meta,
                    },
                );
            }
            Ok(result)
        }
        Err(err) => {
            err.emit(&app);
            Err(err.payload())
        }
    }
}
