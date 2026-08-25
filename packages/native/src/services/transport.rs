//! 统一出网封装（§5.6 Transport 边界）：v1 为本地 reqwest，未来可加 RemoteTransport。

use std::sync::OnceLock;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, Response};

/// 翻译请求级超时（§5.5：翻译 8s / 语法 20s；M1 只有翻译）。
pub const TRANSLATE_TIMEOUT: Duration = Duration::from_secs(8);

#[async_trait]
pub trait Transport: Send + Sync {
    async fn post_json_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: serde_json::Value,
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
        let client = Client::builder()
            .timeout(TRANSLATE_TIMEOUT)
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
        let mut request = self.client.post(url);
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
        let mut request = self.client.get(url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        request.send().await
    }
}
