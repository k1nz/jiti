//! 统一出网封装（§5.6 Transport 边界）：v1 为本地 reqwest，未来可加 RemoteTransport。

use std::sync::OnceLock;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, Response};

/// 翻译请求级超时（§5.5：翻译 8s / 语法 20s；AI 复习复用语法超时）。
pub const TRANSLATE_TIMEOUT: Duration = Duration::from_secs(8);
pub const GRAMMAR_TIMEOUT: Duration = Duration::from_secs(20);
pub const REVIEW_TIMEOUT: Duration = GRAMMAR_TIMEOUT;

#[async_trait]
pub trait Transport: Send + Sync {
    async fn post_json_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: serde_json::Value,
    ) -> Result<Response, reqwest::Error>;

    async fn post_json_with_timeout(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: serde_json::Value,
        timeout: Duration,
    ) -> Result<Response, reqwest::Error>;

    async fn get_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<Response, reqwest::Error>;
}

#[derive(Clone)]
pub struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, reqwest::Error> {
        // 连接池共享；超时在请求级设置，避免翻译 8s 拖死语法 20s。
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()?;
        Ok(Self { client })
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new().expect("reqwest client 构建失败")
    }
}

// 单进程复用连接池，避免每次翻译重建 Client（T8：自有边际成本要小）。
static SHARED: OnceLock<ReqwestTransport> = OnceLock::new();

pub fn shared() -> ReqwestTransport {
    SHARED
        .get_or_init(ReqwestTransport::default)
        .clone()
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn post_json_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: serde_json::Value,
    ) -> Result<Response, reqwest::Error> {
        self.post_json_with_timeout(url, headers, body, TRANSLATE_TIMEOUT)
            .await
    }

    async fn post_json_with_timeout(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: serde_json::Value,
        timeout: Duration,
    ) -> Result<Response, reqwest::Error> {
        let mut request = self.client.post(url).timeout(timeout);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        request.json(&body).send().await
    }

    async fn get_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<Response, reqwest::Error> {
        let mut request = self.client.get(url).timeout(TRANSLATE_TIMEOUT);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        request.send().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_timeout_is_twenty_seconds() {
        assert_eq!(GRAMMAR_TIMEOUT, Duration::from_secs(20));
        assert_eq!(TRANSLATE_TIMEOUT, Duration::from_secs(8));
    }
}
