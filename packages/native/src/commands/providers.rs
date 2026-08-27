//! Provider 设置命令：D2（Key 只进 keys.json，WebView 只见状态）。

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::providers::{config_for, provider_label, snapshot, ProvidersConfig, ProvidersSnapshot};
use crate::services::secrets;
use crate::services::settings;
use crate::services::transport;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TestProviderResult {
    pub ok: bool,
    pub reason: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn providers_snapshot(app: AppHandle) -> Result<ProvidersSnapshot, String> {
    snapshot(&app)
}

#[tauri::command]
#[specta::specta]
pub fn providers_save(
    app: AppHandle,
    config: ProvidersConfig,
) -> Result<ProvidersSnapshot, String> {
    settings::save_provider_config(&app, &config)?;
    snapshot(&app)
}

#[tauri::command]
#[specta::specta]
pub fn provider_save_api_key(
    app: AppHandle,
    provider: String,
    api_key: String,
) -> Result<ProvidersSnapshot, String> {
    if api_key.trim().is_empty() {
        secrets::delete_api_key(&app, &provider);
    } else {
        secrets::set_api_key(&app, &provider, &api_key)?;
    }
    snapshot(&app)
}

#[tauri::command]
#[specta::specta]
pub async fn provider_test(app: AppHandle, provider: String) -> Result<TestProviderResult, String> {
    let config = match settings::load_provider_config(&app) {
        Ok(config) => config,
        Err(e) => return Ok(test_fail(e)),
    };
    let label = provider_label(&provider);
    let api_key = match secrets::get_api_key(&app, &provider) {
        Ok(key) => key,
        Err(_) => {
            return Ok(test_fail(format!(
                "未配置 {label} API Key；请在设置里录入后重试"
            )))
        }
    };
    let transport = transport::shared();

    let result: Result<String, crate::providers::EngineError> = match provider.as_str() {
        "deepl" => {
            crate::providers::deepl::test_connection(
                config_for(&config, &provider),
                &api_key,
                &transport,
            )
            .await
        }
        "llm" => {
            crate::providers::llm::test_connection(
                config_for(&config, &provider),
                &api_key,
                &transport,
            )
            .await
        }
        _ => Err(crate::providers::EngineError::InvalidConfig {
            provider: label.into(),
            detail: "该 Provider 在 M1 无法测试".into(),
        }),
    };

    Ok(match result {
        Ok(reason) => TestProviderResult {
            ok: true,
            reason: Some(reason),
        },
        Err(err) => test_fail(err.payload().copyable),
    })
}

fn test_fail(reason: impl Into<String>) -> TestProviderResult {
    TestProviderResult {
        ok: false,
        reason: Some(reason.into()),
    }
}
