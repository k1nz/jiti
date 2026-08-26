//! 引擎错误统一模型（§5.5 请求级超时；§3.5 `engine://error`）。
//!
//! 分类只做用户可行动的四种：缺 Key / 鉴权失败 / 限流 / 网络，
//! 其余上游与解析问题映射为明确状态码，避免把 WebView 变成错误码垃圾桶。

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

/// 可复制、可展示的错误负载。这也是 `engine://error` 事件的 payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineErrorPayload {
    pub provider: String,
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
    pub copyable: String,
}

/// specta 单源事件契约；Tauri 侧实际事件名保持 `engine://error`（§3.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[tauri_specta(event_name = "engine://error")]
pub struct EngineErrorEvent(pub EngineErrorPayload);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    MissingKey {
        provider: String,
    },
    Unauthorized {
        provider: String,
        detail: String,
    },
    RateLimited {
        provider: String,
        retry_after: Option<u64>,
        detail: String,
    },
    Network {
        provider: String,
        detail: String,
    },
    InvalidConfig {
        provider: String,
        detail: String,
    },
    BadRequest {
        provider: String,
        detail: String,
    },
    InvalidResponse {
        provider: String,
        detail: String,
    },
    Upstream {
        provider: String,
        status: u16,
        detail: String,
    },
}

impl EngineError {
    /// 统一的 HTTP 错误分类（验收：无 Key / Key 错误 / 限流 / 网络 可区分）。
    pub fn from_http(
        provider: &str,
        status: u16,
        detail: &str,
        retry_after: Option<u64>,
    ) -> Self {
        match status {
            401 | 403 => EngineError::Unauthorized {
                provider: provider.into(),
                detail: format!("HTTP {status} {detail}"),
            },
            429 => EngineError::RateLimited {
                provider: provider.into(),
                retry_after,
                detail: format!("HTTP {status} {detail}"),
            },
            400 | 404 | 422 => EngineError::BadRequest {
                provider: provider.into(),
                detail: format!("HTTP {status} {detail}"),
            },
            500..=599 => EngineError::Upstream {
                provider: provider.into(),
                status,
                detail: detail.into(),
            },
            _ => EngineError::Upstream {
                provider: provider.into(),
                status,
                detail: detail.into(),
            },
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            EngineError::MissingKey { .. } => "missing_key",
            EngineError::Unauthorized { .. } => "unauthorized",
            EngineError::RateLimited { .. } => "rate_limited",
            EngineError::Network { .. } => "network",
            EngineError::InvalidConfig { .. } => "invalid_config",
            EngineError::BadRequest { .. } => "bad_request",
            EngineError::InvalidResponse { .. } => "invalid_response",
            EngineError::Upstream { .. } => "upstream",
        }
    }

    pub fn provider(&self) -> &str {
        match self {
            EngineError::MissingKey { provider }
            | EngineError::Unauthorized { provider, .. }
            | EngineError::RateLimited { provider, .. }
            | EngineError::Network { provider, .. }
            | EngineError::InvalidConfig { provider, .. }
            | EngineError::BadRequest { provider, .. }
            | EngineError::InvalidResponse { provider, .. }
            | EngineError::Upstream { provider, .. } => provider,
        }
    }

    pub fn payload(&self) -> EngineErrorPayload {
        let provider = self.provider().to_string();
        let code = self.code().to_string();
        let (message, hint, copyable) = match self {
            EngineError::MissingKey { provider } => (
                format!("未配置 {provider} API Key"),
                Some("到 设置 → 引擎 粘贴 Key 后重试".into()),
                format!("[jiti] {code}: {provider} 缺少 API Key"),
            ),
            EngineError::Unauthorized { provider, detail } => (
                format!("{provider} API Key 无效或已被撤销"),
                Some("检查 Key 是否完整、是否已到期；重新粘贴后测试连接".into()),
                format!("[jiti] {code}: {provider} {detail}"),
            ),
            EngineError::RateLimited {
                provider,
                retry_after,
                detail,
            } => (
                format!("{provider} 限流或额度用尽"),
                Some(match retry_after {
                    Some(secs) => format!("{secs} 秒后重试，或检查账户额度"),
                    None => "稍后重试，或检查账户额度".into(),
                }),
                format!("[jiti] {code}: {provider} {detail}"),
            ),
            EngineError::Network { provider, detail } => (
                format!("无法连接 {provider}（超时或网络不可达）"),
                Some("检查网络，或确认 baseUrl 可达；国内网络建议使用国内镜像".into()),
                format!("[jiti] {code}: {provider} {detail}"),
            ),
            EngineError::InvalidConfig { provider, detail } => (
                format!("{provider} 配置不完整"),
                Some("补齐 baseUrl / model / Key 后重试".into()),
                format!("[jiti] {code}: {provider} {detail}"),
            ),
            EngineError::BadRequest { provider, detail } => (
                format!("{provider} 拒绝了本次请求"),
                Some("检查文本内容与长度后重试".into()),
                format!("[jiti] {code}: {provider} {detail}"),
            ),
            EngineError::InvalidResponse { provider, detail } => (
                format!("{provider} 返回了无法解析的响应"),
                Some("可能是临时故障，重试一次；仍旧失败则检查 Provider 配置".into()),
                format!("[jiti] {code}: {provider} {detail}"),
            ),
            EngineError::Upstream {
                provider,
                status,
                detail,
            } => (
                format!("{provider} 服务异常（HTTP {status}）"),
                Some("服务端临时故障，稍后重试".into()),
                format!("[jiti] {code}: {provider} HTTP {status} {detail}"),
            ),
        };
        EngineErrorPayload {
            provider,
            code,
            message,
            hint,
            copyable,
        }
    }

    /// 错误经 `engine://error` 事件回传（§3.5 / M1 验收）。
    pub fn emit(&self, app: &AppHandle) {
        let _ = EngineErrorEvent(self.payload()).emit(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_key_has_copyable_payload() {
        let err = EngineError::MissingKey {
            provider: "DeepL".into(),
        };
        let p = err.payload();
        assert_eq!(p.code, "missing_key");
        assert!(p.copyable.contains("缺少 API Key"));
        assert!(p.hint.is_some());
    }

    #[test]
    fn rate_limit_keeps_retry_hint() {
        let err = EngineError::RateLimited {
            provider: "LLM".into(),
            retry_after: Some(9),
            detail: "429".into(),
        };
        let p = err.payload();
        assert_eq!(p.code, "rate_limited");
        assert!(p.hint.unwrap().contains("9 秒"));
    }

    #[test]
    fn event_name_is_architected_slash_event() {
        assert_eq!(EngineErrorEvent::NAME, "engine://error");
    }

    #[test]
    fn http_error_classification_covers_acceptance_cases() {
        assert!(matches!(
            EngineError::from_http("DeepL", 403, "forbidden", None),
            EngineError::Unauthorized { .. }
        ));
        assert!(matches!(
            EngineError::from_http("LLM", 429, "slow down", Some(5)),
            EngineError::RateLimited { retry_after: Some(5), .. }
        ));
        assert!(matches!(
            EngineError::from_http("LLM", 503, "down", None),
            EngineError::Upstream { status: 503, .. }
        ));
    }
}
