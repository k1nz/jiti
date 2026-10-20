//! 翻译命令（D1：出网只在 Rust）。

use tauri::AppHandle;

use crate::providers::{
    run_translate, EngineError, EngineErrorPayload, TranslateOutcome, TranslateRequest,
};
use crate::services::history::{self, NewHistoryEntry};

#[tauri::command]
#[specta::specta]
pub async fn translate(
    app: AppHandle,
    request: TranslateRequest,
) -> Result<TranslateOutcome, EngineErrorPayload> {
    if request.text.trim().is_empty() {
        let err = EngineError::BadRequest {
            provider: "translate".into(),
            detail: "文本为空".into(),
        };
        err.emit(&app);
        return Err(err.payload());
    }
    if request.to.trim().is_empty() {
        let err = EngineError::BadRequest {
            provider: "translate".into(),
            detail: "目标语言为空".into(),
        };
        err.emit(&app);
        return Err(err.payload());
    }

    match run_translate(&app, request.clone()).await {
        Ok(result) => {
            let should_write = crate::services::settings::load_provider_config(&app)
                .map(|c| c.write_history)
                .unwrap_or(true);
            if should_write {
                if let Some(hit) = result.results.iter().find(|hit| {
                    result.preferred_id.as_deref() == Some(hit.id.as_str()) && hit.error.is_none()
                }) {
                    let _ = history::record(
                        &app,
                        NewHistoryEntry {
                            kind: "translate".into(),
                            input: result.input.clone(),
                            output: hit.output.clone(),
                            engine: hit.engine.clone(),
                            from_lang: hit.detected_from.clone().or(request.from),
                            to_lang: Some(hit.target.clone()),
                            duration_ms: hit.duration_ms as i64,
                            source_app: None,
                            meta: None,
                        },
                    );
                }
            }
            Ok(result)
        }
        Err(err) => {
            err.emit(&app);
            Err(err.payload())
        }
    }
}
