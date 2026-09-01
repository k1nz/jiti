//! 收藏命令：保存（去重）/ 列表 / 改状态与备注 / 删除。

use tauri::AppHandle;

use crate::services::clips::{Clip, ClipFilter, ClipList, ClipPatch, ClipSaveResult, NewClip};
use crate::services::{clips, database};

#[tauri::command]
#[specta::specta]
pub fn clips_save(app: AppHandle, item: NewClip) -> Result<ClipSaveResult, String> {
    let conn = database::open(&app)?;
    clips::save(&conn, &item)
}

#[tauri::command]
#[specta::specta]
pub fn clips_list(app: AppHandle, filter: ClipFilter) -> Result<ClipList, String> {
    let conn = database::open(&app)?;
    clips::list(&conn, &filter)
}

#[tauri::command]
#[specta::specta]
pub fn clips_update(app: AppHandle, id: i32, patch: ClipPatch) -> Result<Clip, String> {
    let conn = database::open(&app)?;
    clips::update(&conn, id, &patch)
}

#[tauri::command]
#[specta::specta]
pub fn clips_delete(app: AppHandle, id: i32) -> Result<i32, String> {
    let conn = database::open(&app)?;
    clips::delete(&conn, id)
}
