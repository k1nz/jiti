//! 错题本命令：CRUD / 偏好 / Markdown 导出 / AI 复习。

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

use crate::providers::review::{history_from_review, review, AiReviewResult};
use crate::providers::{
    config_for, provider_label, EngineError, EngineErrorPayload, PROVIDER_LLM,
};
use crate::services::database;
use crate::services::history;
use crate::services::mistakes::{
    self, Mistake, MistakeFilter, MistakeList, MistakePatch, MistakePreferences, NewMistake,
};
use crate::services::{settings, transport};

#[tauri::command]
#[specta::specta]
pub fn mistakes_list(app: AppHandle, filter: MistakeFilter) -> Result<MistakeList, String> {
    let conn = database::open(&app)?;
    mistakes::list(&conn, &filter)
}

#[tauri::command]
#[specta::specta]
pub fn mistakes_create(app: AppHandle, item: NewMistake) -> Result<Mistake, String> {
    let prefs = settings::load_mistake_preferences(&app).unwrap_or_default();
    let conn = database::open(&app)?;
    mistakes::create(&conn, &item, &prefs.default_status)
}

#[tauri::command]
#[specta::specta]
pub fn mistakes_update(app: AppHandle, id: i32, patch: MistakePatch) -> Result<Mistake, String> {
    let conn = database::open(&app)?;
    mistakes::update(&conn, id, &patch)
}

#[tauri::command]
#[specta::specta]
pub fn mistakes_delete(app: AppHandle, id: i32) -> Result<i32, String> {
    let conn = database::open(&app)?;
    mistakes::delete(&conn, id)
}

#[tauri::command]
#[specta::specta]
pub fn mistakes_preferences(app: AppHandle) -> Result<MistakePreferences, String> {
    settings::load_mistake_preferences(&app)
}

#[tauri::command]
#[specta::specta]
pub fn mistakes_set_preferences(
    app: AppHandle,
    prefs: MistakePreferences,
) -> Result<MistakePreferences, String> {
    settings::save_mistake_preferences(&app, &prefs)
}

/// 按当前筛选导出 Markdown。取消保存对话框时返回 `None`，不写盘。
#[tauri::command]
#[specta::specta]
pub async fn mistakes_export(
    app: AppHandle,
    filter: MistakeFilter,
) -> Result<Option<String>, String> {
    let markdown = {
        let conn = database::open(&app)?;
        let items = mistakes::list_all(&conn, &filter)?;
        mistakes::render_markdown(&items)
    };
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Markdown", &["md"])
            .set_file_name("jiti-mistakes.md")
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(file) = picked else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, markdown).map_err(|e| e.to_string())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
#[specta::specta]
pub async fn mistakes_ai_review(
    app: AppHandle,
    filter: MistakeFilter,
) -> Result<AiReviewResult, EngineErrorPayload> {
    let items = {
        let conn = database::open(&app).map_err(|e| {
            EngineError::InvalidConfig {
                provider: "ai_review".into(),
                detail: e,
            }
            .payload()
        })?;
        mistakes::list_all(&conn, &filter).map_err(|e| {
            EngineError::InvalidConfig {
                provider: "ai_review".into(),
                detail: e,
            }
            .payload()
        })?
    };
    if items.is_empty() {
        let err = EngineError::BadRequest {
            provider: "ai_review".into(),
            detail: "当前筛选没有错题".into(),
        };
        err.emit(&app);
        return Err(err.payload());
    }

    let config = settings::load_provider_config(&app).map_err(|e| {
        EngineError::InvalidConfig {
            provider: "settings".into(),
            detail: e,
        }
        .payload()
    })?;
    let cfg = config_for(&config, PROVIDER_LLM);
    let key = crate::services::secrets::get_api_key(&app, PROVIDER_LLM).map_err(|_| {
        EngineError::MissingKey {
            provider: provider_label(PROVIDER_LLM).into(),
        }
        .payload()
    })?;
    let transport = transport::shared();
    match review(cfg, &key, &items, &transport).await {
        Ok(result) => {
            if let Ok(conn) = database::open(&app) {
                let _ = history::insert(&conn, &history_from_review(&result, &filter));
            }
            Ok(result)
        }
        Err(err) => {
            err.emit(&app);
            Err(err.payload())
        }
    }
}
