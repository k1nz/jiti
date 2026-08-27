//! 语法检查命令：输入校验、Channel 转发、耗时、历史与错题收录。

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::Channel;
use tauri::AppHandle;

use crate::providers::grammar::{run_grammar, GrammarProgressEvent, GrammarRequest, GrammarResult};
use crate::providers::EngineErrorPayload;
use crate::services::database;
use crate::services::history::{self, NewHistoryEntry};
use crate::services::mistakes;
use crate::services::settings;

/// 命令层结果：标准 `GrammarResult` 不掺持久化状态；收录 id 与 errors 按下标对齐。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrammarCheckOutcome {
    pub result: GrammarResult,
    pub mistake_ids: Vec<Option<i32>>,
}

#[tauri::command]
#[specta::specta]
pub async fn grammar_check(
    app: AppHandle,
    request: GrammarRequest,
    on_progress: Channel<GrammarProgressEvent>,
) -> Result<GrammarCheckOutcome, EngineErrorPayload> {
    let mut emit = |event: GrammarProgressEvent| {
        let _ = on_progress.send(event);
    };
    match run_grammar(&app, request, &mut emit).await {
        Ok(result) => {
            let should_write = settings::load_provider_config(&app)
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
            let prefs = settings::load_mistake_preferences(&app).unwrap_or_default();
            let mistake_ids = match database::open(&app) {
                Ok(mut conn) => mistakes::collect_from_grammar(&mut conn, &result, &prefs)
                    .map_err(|e| {
                        crate::providers::EngineError::InvalidConfig {
                            provider: "mistakes".into(),
                            detail: format!("错题收录失败：{e}"),
                        }
                        .payload()
                    })?,
                Err(e) => {
                    return Err(crate::providers::EngineError::InvalidConfig {
                        provider: "mistakes".into(),
                        detail: e,
                    }
                    .payload());
                }
            };
            Ok(GrammarCheckOutcome {
                result,
                mistake_ids,
            })
        }
        Err(err) => {
            err.emit(&app);
            Err(err.payload())
        }
    }
}
