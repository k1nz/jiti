//! 版本与检查更新（GitHub 公开 Releases / tags）。

use crate::services::transport;
use crate::services::updates::{self, UpdateCheckResult};

#[tauri::command]
#[specta::specta]
pub fn app_version() -> String {
    updates::current_version().into()
}

#[tauri::command]
#[specta::specta]
pub async fn check_for_updates() -> UpdateCheckResult {
    updates::check(&transport::shared()).await
}

#[tauri::command]
#[specta::specta]
pub fn open_external_url(url: String) -> Result<(), String> {
    updates::open_https_url(&url)
}
