//! 句法分析命令：输入校验、Channel 转发、耗时与历史。不收录错题。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::AppHandle;

use crate::providers::syntax::{
    begin_syntax_job, run_syntax, syntax_job_is_current, SyntaxLanguage, SyntaxProgressEvent,
    SyntaxRequest, SyntaxResult,
};
use crate::providers::EngineErrorPayload;
use crate::services::history::{self, NewHistoryEntry};
use crate::services::settings;

#[tauri::command]
#[specta::specta]
pub async fn syntax_analyze(
    app: AppHandle,
    request: SyntaxRequest,
    on_progress: Channel<SyntaxProgressEvent>,
) -> Result<SyntaxResult, EngineErrorPayload> {
    let job = begin_syntax_job();
    let channel_closed = Arc::new(AtomicBool::new(false));
    let alive = {
        let channel_closed = channel_closed.clone();
        move || syntax_job_is_current(job) && !channel_closed.load(Ordering::SeqCst)
    };
    let mut emit = |event: SyntaxProgressEvent| {
        if !syntax_job_is_current(job) || channel_closed.load(Ordering::SeqCst) {
            return;
        }
        if on_progress.send(event).is_err() {
            channel_closed.store(true, Ordering::SeqCst);
        }
    };
    match run_syntax(&app, request, &mut emit, &alive).await {
        Ok(result) => {
            if !alive() {
                return Err(crate::providers::EngineError::Cancelled {
                    provider: "syntax".into(),
                }
                .payload());
            }
            let should_write = settings::load_provider_config(&app)
                .map(|c| c.write_history)
                .unwrap_or(true);
            if should_write {
                let from_lang = match result.profile.language {
                    SyntaxLanguage::Zh => Some("zh".into()),
                    SyntaxLanguage::En => Some("en".into()),
                    SyntaxLanguage::Mixed => None,
                };
                let meta = serde_json::to_string(&result).ok();
                let _ = history::record(
                    &app,
                    NewHistoryEntry {
                        kind: "syntax".into(),
                        input: result.input.clone(),
                        output: result.summary.clone(),
                        engine: result.engine.clone(),
                        from_lang,
                        to_lang: None,
                        duration_ms: result.duration_ms as i64,
                        source_app: None,
                        meta,
                    },
                );
            }
            if !alive() {
                return Err(crate::providers::EngineError::Cancelled {
                    provider: "syntax".into(),
                }
                .payload());
            }
            Ok(result)
        }
        Err(err) => {
            err.emit(&app);
            Err(err.payload())
        }
    }
}
