//! 统一出网封装（§5.6 Transport 边界）：v1 为本地 reqwest，未来可加 RemoteTransport。

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use reqwest::{Client, Response};

/// 翻译请求级超时（§5.5：翻译 8s / 语法 20s；AI 复习复用语法超时）。
pub const TRANSLATE_TIMEOUT: Duration = Duration::from_secs(8);
pub const GRAMMAR_TIMEOUT: Duration = Duration::from_secs(20);
pub const REVIEW_TIMEOUT: Duration = GRAMMAR_TIMEOUT;
/// 连接 / 首字节超过此时长即给用户可见错误；不得用完整 20s 去等 TTFB。
pub const FIRST_BYTE_TIMEOUT: Duration = Duration::from_secs(10);
/// 短于常见 NAT、系统代理和 DeepSeek 空闲断开时间，避免复用已被对端关掉的连接。
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(20);
const TCP_KEEPALIVE: Duration = Duration::from_secs(15);
const HTTP2_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
const HTTP2_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(5);

pub fn deadline_from_now(total: Duration) -> Instant {
    Instant::now() + total
}

pub fn remaining_budget(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}

/// 流式尝试与一次严格重试共享同一 deadline；重试拿剩余时间，不重新获得完整 20s。
pub fn retry_budget(deadline: Instant) -> Option<Duration> {
    let left = remaining_budget(deadline);
    if left.is_zero() {
        None
    } else {
        Some(left)
    }
}

pub fn first_byte_budget(deadline: Instant) -> Duration {
    remaining_budget(deadline).min(FIRST_BYTE_TIMEOUT)
}

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

    async fn post_bytes(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: Vec<u8>,
    ) -> Result<Response, reqwest::Error>;
}

#[derive(Clone)]
pub struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, reqwest::Error> {
        // 连接池共享；超时在请求级设置，避免翻译 8s 拖死语法 20s。
        // HTTP/2 空闲时也发 PING：对端或本地代理先关掉连接时，下次请求会立刻失败，
        // Display 只有 “error sending request for url (...)”，服务本身往往仍可用。
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(POOL_IDLE_TIMEOUT)
            .tcp_keepalive(TCP_KEEPALIVE)
            .http2_keep_alive_interval(HTTP2_KEEPALIVE_INTERVAL)
            .http2_keep_alive_timeout(HTTP2_KEEPALIVE_TIMEOUT)
            .http2_keep_alive_while_idle(true)
            .build()?;
        Ok(Self { client })
    }

    async fn send(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<Response, reqwest::Error> {
        match build().send().await {
            Ok(resp) => Ok(resp),
            Err(err) if is_transient_send_error(&err) => {
                eprintln!(
                    "[jiti] transport: retrying once after {}",
                    describe_reqwest_error(&err)
                );
                build().send().await
            }
            Err(err) => Err(err),
        }
    }
}

/// reqwest 的 Display 不带 source。把超时、连接重置等真正原因拼进去，方便复制排错。
pub fn describe_reqwest_error(err: &reqwest::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = std::error::Error::source(err);
    while let Some(inner) = source {
        let text = inner.to_string();
        if parts.iter().all(|part| part != &text) {
            parts.push(text);
        }
        source = inner.source();
        if parts.len() >= 4 {
            break;
        }
    }
    parts.join(": ")
}

/// 空闲连接被对端关掉时，`.send()` 会立刻失败且还没有响应。重试一次会换新连接。
/// 超时不重试，避免把 8s / 20s 预算翻倍。
fn is_transient_send_error(err: &reqwest::Error) -> bool {
    if err.is_timeout() {
        return false;
    }
    err.is_connect() || err.is_request()
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new().expect("reqwest client 构建失败")
    }
}

// 单进程复用连接池，避免每次翻译重建 Client（T8：自有边际成本要小）。
static SHARED: OnceLock<ReqwestTransport> = OnceLock::new();

pub fn shared() -> ReqwestTransport {
    SHARED.get_or_init(ReqwestTransport::default).clone()
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
        self.send(|| {
            let mut request = self.client.post(url).timeout(timeout);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.json(&body)
        })
        .await
    }

    async fn get_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<Response, reqwest::Error> {
        self.send(|| {
            let mut request = self.client.get(url).timeout(TRANSLATE_TIMEOUT);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request
        })
        .await
    }

    async fn post_bytes(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: Vec<u8>,
    ) -> Result<Response, reqwest::Error> {
        self.send(|| {
            let mut request = self
                .client
                .post(url)
                .timeout(TRANSLATE_TIMEOUT)
                .body(body.clone());
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_timeout_is_twenty_seconds() {
        assert_eq!(GRAMMAR_TIMEOUT, Duration::from_secs(20));
        assert_eq!(TRANSLATE_TIMEOUT, Duration::from_secs(8));
        assert_eq!(FIRST_BYTE_TIMEOUT, Duration::from_secs(10));
    }

    #[test]
    fn first_byte_budget_caps_at_ten_seconds() {
        let deadline = Instant::now() + Duration::from_secs(20);
        assert!(first_byte_budget(deadline) <= FIRST_BYTE_TIMEOUT);
        let short = Instant::now() + Duration::from_millis(250);
        assert!(first_byte_budget(short) <= Duration::from_millis(250));
    }

    #[test]
    fn retry_does_not_get_a_fresh_full_budget() {
        let deadline = Instant::now() + Duration::from_millis(500);
        std::thread::sleep(Duration::from_millis(50));
        let left = retry_budget(deadline).expect("still have time");
        assert!(left < Duration::from_secs(20));
        assert!(left <= Duration::from_millis(500));
        let expired = Instant::now() - Duration::from_millis(1);
        assert!(retry_budget(expired).is_none());
        assert!(remaining_budget(expired).is_zero());
    }

    #[test]
    fn reset_connection_is_retried_once() {
        use std::io::{Read, Write};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let accepts = Arc::new(AtomicUsize::new(0));
        let accepts_bg = accepts.clone();
        std::thread::spawn(move || {
            for _ in 0..2 {
                let Ok((mut sock, _)) = listener.accept() else {
                    break;
                };
                let n = accepts_bg.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = sock.set_read_timeout(Some(Duration::from_secs(1)));
                let _ = sock.read(&mut buf);
                if n == 0 {
                    let _ = sock.shutdown(std::net::Shutdown::Both);
                    drop(sock);
                    continue;
                }
                let body = "ok";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes());
            }
        });

        let transport = ReqwestTransport::new().unwrap();
        let resp = tauri::async_runtime::block_on(async {
            transport
                .post_json_with_timeout(
                    &format!("http://{addr}/v1/chat/completions"),
                    &[],
                    serde_json::json!({ "ping": true }),
                    Duration::from_secs(3),
                )
                .await
        });
        let resp = resp.expect("被对端重置的连接应重试成功");
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        assert_eq!(accepts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn timeout_is_not_retried_and_detail_includes_cause() {
        use std::io::Read;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let accepts = Arc::new(AtomicUsize::new(0));
        let accepts_bg = accepts.clone();
        std::thread::spawn(move || {
            let _ = listener.set_nonblocking(true);
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(2) {
                match listener.accept() {
                    Ok((mut sock, _)) => {
                        accepts_bg.fetch_add(1, Ordering::SeqCst);
                        std::thread::spawn(move || {
                            let mut buf = [0u8; 4096];
                            let _ = sock.read(&mut buf);
                            std::thread::sleep(Duration::from_secs(2));
                        });
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(15));
                    }
                    Err(_) => break,
                }
            }
        });

        let transport = ReqwestTransport::new().unwrap();
        let started = Instant::now();
        let err = tauri::async_runtime::block_on(async {
            transport
                .post_json_with_timeout(
                    &format!("http://{addr}/"),
                    &[],
                    serde_json::json!({ "ping": true }),
                    Duration::from_millis(300),
                )
                .await
        })
        .expect_err("对端不回包应超时");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(err.is_timeout());
        let detail = describe_reqwest_error(&err);
        assert!(
            detail.contains("operation timed out"),
            "应带上被 Display 藏掉的原因，实际是 {detail}"
        );
        assert!(!err.to_string().contains("operation timed out"));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(accepts.load(Ordering::SeqCst), 1);
    }
}
