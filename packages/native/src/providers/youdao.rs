//! 有道智云适配器占位：M1 只保留注册表与签名参数边界（§5.1），不实现真实请求。

use crate::services::transport::Transport;

use super::{EngineError, ProviderConfig};

#[allow(dead_code)]
pub async fn translate(
    _cfg: &ProviderConfig,
    _api_key: &str,
    _request: &super::TranslateRequest,
    _transport: &dyn Transport,
) -> Result<super::TranslateResult, EngineError> {
    Err(EngineError::InvalidConfig {
        provider: super::provider_label(super::PROVIDER_YOUDAO).into(),
        detail: "有道签名接口在 M1 仅预留，翻译请先用 DeepL / LLM".into(),
    })
}

#[allow(dead_code)]
pub async fn test_connection(
    _cfg: &ProviderConfig,
    _api_key: &str,
    _transport: &dyn Transport,
) -> Result<String, EngineError> {
    Err(EngineError::InvalidConfig {
        provider: super::provider_label(super::PROVIDER_YOUDAO).into(),
        detail: "有道适配器未接入".into(),
    })
}
