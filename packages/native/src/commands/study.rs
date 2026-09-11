//! 复习课窗口与方案命令。

use tauri::AppHandle;

use crate::providers::plan::{
    apply_copy, fallback_summary, fetch_copy, history_from_plan, PLAN_PROMPT_VERSION,
};
use crate::providers::{config_for, provider_label, EngineError, EngineErrorPayload, PROVIDER_LLM};
use crate::services::review_plan::{self, ReviewGradeRequest, ReviewGradeResult, ReviewPlan};
use crate::services::{database, history, settings, study_window, transport};

#[tauri::command]
#[specta::specta]
pub async fn open_study(app: AppHandle) -> Result<(), String> {
    study_window::open(&app)
}

#[tauri::command]
#[specta::specta]
pub fn review_plan_active(app: AppHandle) -> Result<Option<ReviewPlan>, String> {
    let conn = database::open(&app)?;
    review_plan::load_active(&conn)
}

#[tauri::command]
#[specta::specta]
pub async fn review_plan_generate(app: AppHandle) -> Result<ReviewPlan, EngineErrorPayload> {
    let (mistakes, clips) = {
        let conn = database::open(&app).map_err(|e| {
            EngineError::InvalidConfig {
                provider: "review_plan".into(),
                detail: e,
            }
            .payload()
        })?;
        review_plan::open_pool(&conn).map_err(|e| {
            EngineError::InvalidConfig {
                provider: "review_plan".into(),
                detail: e,
            }
            .payload()
        })?
    };
    let mut draft = review_plan::build_draft(&mistakes, &clips).map_err(|detail| {
        let err = EngineError::BadRequest {
            provider: "review_plan".into(),
            detail,
        };
        err.emit(&app);
        err.payload()
    })?;

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
    let started = std::time::Instant::now();
    let copy = match fetch_copy(cfg, &key, &draft, &transport).await {
        Ok(copy) => copy,
        Err(err) => {
            err.emit(&app);
            return Err(err.payload());
        }
    };
    apply_copy(&mut draft, &copy);
    let summary = if copy.summary.trim().is_empty() {
        fallback_summary(&draft)
    } else {
        copy.summary
    };
    let duration_ms = started.elapsed().as_millis() as u32;
    let engine = provider_label(PROVIDER_LLM);
    let plan = {
        let conn = database::open(&app).map_err(|e| {
            EngineError::InvalidConfig {
                provider: "review_plan".into(),
                detail: e,
            }
            .payload()
        })?;
        let plan = review_plan::persist(
            &conn,
            &draft,
            &summary,
            engine,
            duration_ms,
            PLAN_PROMPT_VERSION,
        )
        .map_err(|e| {
            EngineError::InvalidConfig {
                provider: "review_plan".into(),
                detail: e,
            }
            .payload()
        })?;
        let _ = history::insert(
            &conn,
            &history_from_plan(&summary, plan.analyzed_count, engine, duration_ms),
        );
        plan
    };
    Ok(plan)
}

#[tauri::command]
#[specta::specta]
pub fn review_plan_grade(
    app: AppHandle,
    request: ReviewGradeRequest,
) -> Result<ReviewGradeResult, String> {
    let conn = database::open(&app)?;
    review_plan::grade(&conn, &request)
}

#[tauri::command]
#[specta::specta]
pub fn review_plan_complete_day(
    app: AppHandle,
    plan_id: i32,
    day_index: i32,
) -> Result<ReviewPlan, String> {
    let conn = database::open(&app)?;
    review_plan::complete_day(&conn, plan_id, day_index)
}
