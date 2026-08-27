//! 生产崩溃上报（M5.1）。
//!
//! RC 构建不编译 DSN（`option_env!("JITI_SENTRY_DSN")` 为空），客户端保持关闭。
//! 不向 WebView 注入 Sentry JS，避免把翻译/语法正文和密钥带进面包屑。

use sentry::protocol::{Breadcrumb, Event, Map, Value};
use sentry::{ClientInitGuard, ClientOptions};

const REDACTED: &str = "[redacted]";

fn compiled_dsn() -> Option<&'static str> {
    match option_env!("JITI_SENTRY_DSN") {
        Some(dsn) if !dsn.is_empty() => Some(dsn),
        _ => None,
    }
}

/// 进程级 guard：必须活到 `app.run` 返回。DSN 为空时是 no-op。
pub fn init() -> ClientInitGuard {
    let enabled = compiled_dsn().is_some();
    let mut opts = ClientOptions::default()
        .maybe_release(sentry::release_name!())
        .environment(if enabled { "production" } else { "disabled" })
        .send_default_pii(false)
        .attach_stacktrace(true)
        .before_send(|event| Some(scrub_event(event)))
        .before_breadcrumb(|crumb| Some(scrub_breadcrumb(crumb)));
    if let Some(dsn) = compiled_dsn() {
        opts = opts.dsn(dsn);
    }
    sentry::init(opts)
}

fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "api_key",
        "apikey",
        "app_key",
        "appkey",
        "secret",
        "password",
        "token",
        "authorization",
        "cookie",
        "bearer",
        "dsn",
        "private_key",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn redact_map(map: &mut Map<String, Value>) {
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        if is_sensitive_key(&key) {
            map.insert(key, Value::String(REDACTED.to_string()));
            continue;
        }
        if let Some(Value::String(s)) = map.get_mut(&key) {
            *s = scrub_text(s);
        }
    }
}

fn next_char(input: &str, i: usize) -> Option<(char, usize)> {
    input[i..].chars().next().map(|ch| (ch, ch.len_utf8()))
}

fn redact_sk_tokens(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"sk-") {
            let mut j = i + 3;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-' || bytes[j] == b'_')
            {
                j += 1;
            }
            if j - i >= 11 {
                out.push_str("sk-");
                out.push_str(REDACTED);
                i = j;
                continue;
            }
        }
        match next_char(input, i) {
            Some((ch, len)) => {
                out.push(ch);
                i += len;
            }
            None => break,
        }
    }
    out
}

fn redact_bearer(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let bytes = input.as_bytes();
    while i < bytes.len() {
        if lower
            .get(i..)
            .is_some_and(|rest| rest.starts_with("bearer "))
        {
            out.push_str(&input[i..i + 7]);
            out.push_str(REDACTED);
            i += 7;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }
        match next_char(input, i) {
            Some((ch, len)) => {
                out.push(ch);
                i += len;
            }
            None => break,
        }
    }
    out
}

/// 清洗日志/异常文本：密钥、Bearer、OpenAI 风格 sk- token。
pub fn scrub_text(input: &str) -> String {
    redact_sk_tokens(&redact_bearer(input))
}

pub fn scrub_event(mut event: Event<'static>) -> Event<'static> {
    if let Some(message) = event.message.take() {
        event.message = Some(scrub_text(&message));
    }
    if let Some(entry) = event.logentry.as_mut() {
        entry.message = scrub_text(&entry.message);
        for param in entry.params.iter_mut() {
            if let Value::String(s) = param {
                *s = scrub_text(s);
            }
        }
    }
    for exception in event.exception.values.iter_mut() {
        if let Some(value) = exception.value.take() {
            exception.value = Some(scrub_text(&value));
        }
    }
    for breadcrumb in event.breadcrumbs.values.iter_mut() {
        if let Some(message) = breadcrumb.message.take() {
            breadcrumb.message = Some(scrub_text(&message));
        }
        redact_map(&mut breadcrumb.data);
    }
    redact_map(&mut event.extra);
    event.request = None;
    event.server_name = None;
    event.user = None;
    event
}

fn scrub_breadcrumb(mut breadcrumb: Breadcrumb) -> Breadcrumb {
    if let Some(message) = breadcrumb.message.take() {
        breadcrumb.message = Some(scrub_text(&message));
    }
    redact_map(&mut breadcrumb.data);
    breadcrumb
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::{Breadcrumb, Exception, Values};

    #[test]
    fn scrubs_bearer_and_openai_keys() {
        let raw = "Authorization: Bearer supersecretvalue123 using sk-abcdefghijklmnopqrstuv";
        let cleaned = scrub_text(raw);
        assert!(cleaned.contains(REDACTED));
        assert!(!cleaned.contains("supersecretvalue123"));
        assert!(!cleaned.contains("sk-abcdefghijklmnopqrstuv"));
    }

    #[test]
    fn leaves_ordinary_text() {
        assert_eq!(scrub_text("timeout after 10s"), "timeout after 10s");
    }

    #[test]
    fn scrub_event_drops_request_and_secrets() {
        let mut event = Event::new();
        event.message = Some("Bearer abcdefghijklmnop sk-1234567890ab".into());
        event
            .extra
            .insert("api_key".into(), Value::String("plain-secret".into()));
        event
            .extra
            .insert("note".into(), Value::String("ok".into()));
        event.request = Some(Default::default());
        event.user = Some(Default::default());
        event.exception = Values {
            values: vec![Exception {
                value: Some("token=should-stay-in-value-but-sk-zzzzzzzzzzz".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        event.breadcrumbs = Values {
            values: vec![Breadcrumb {
                message: Some("Bearer leaked-token-value".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let cleaned = scrub_event(event);
        assert!(cleaned.message.as_deref().unwrap().contains(REDACTED));
        assert!(!cleaned
            .message
            .as_deref()
            .unwrap()
            .contains("abcdefghijklmnop"));
        assert_eq!(
            cleaned.extra.get("api_key").unwrap(),
            &Value::String(REDACTED.into())
        );
        assert_eq!(
            cleaned.extra.get("note").unwrap(),
            &Value::String("ok".into())
        );
        assert!(cleaned.request.is_none());
        assert!(cleaned.user.is_none());
        assert!(cleaned.server_name.is_none());
        let breadcrumb = &cleaned.breadcrumbs.values[0];
        assert!(!breadcrumb
            .message
            .as_deref()
            .unwrap()
            .contains("leaked-token-value"));
    }
}
