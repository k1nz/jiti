//! SSE 分帧：按 WHATWG EventSource 规则从任意 chunk 边界还原 `data:` 字段。
//! 处理跨 chunk UTF-8、空行、注释、`[DONE]`。

use bytes::Bytes;
use eventsource_stream::{Event, EventStreamError, Eventsource};
use futures_util::Stream;
use futures_util::StreamExt;
use serde::Deserialize;

use crate::providers::EngineError;

#[derive(Debug, Deserialize)]
struct ChatChunk {
    choices: Vec<ChatChunkChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChunkChoice {
    #[serde(default)]
    delta: ChatDelta,
}

#[derive(Debug, Default, Deserialize)]
struct ChatDelta {
    content: Option<String>,
}

pub fn extract_delta_content(data: &str) -> Option<String> {
    let trimmed = data.trim();
    if trimmed.is_empty() || trimmed == "[DONE]" {
        return None;
    }
    let chunk: ChatChunk = serde_json::from_str(trimmed).ok()?;
    chunk
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.delta.content)
        .filter(|s| !s.is_empty())
}

fn map_sse_error(err: EventStreamError<reqwest::Error>) -> EngineError {
    EngineError::InvalidResponse {
        provider: "LLM".into(),
        detail: err.to_string(),
    }
}

/// 从 OpenAI 兼容 SSE 流中抽出文本增量。`[DONE]` 结束。
pub async fn fold_openai_sse<S>(
    byte_stream: S,
    mut on_delta: impl FnMut(&str),
) -> Result<String, EngineError>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    let mut events = byte_stream.eventsource();
    let mut assembled = String::new();
    while let Some(item) = events.next().await {
        let Event { data, .. } = item.map_err(map_sse_error)?;
        if data.trim() == "[DONE]" {
            break;
        }
        if let Some(delta) = extract_delta_content(&data) {
            on_delta(&delta);
            assembled.push_str(&delta);
        }
    }
    Ok(assembled)
}

/// 行装配：去掉 Markdown 围栏，按换行切出完整 NDJSON 行。
#[derive(Debug, Default)]
pub struct LineAssembler {
    buf: String,
}

impl LineAssembler {
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.buf.push_str(delta);
        self.strip_opening_fence();
        let mut lines = Vec::new();
        while let Some(pos) = self.buf.find('\n') {
            let line = self.buf[..pos].trim_end_matches('\r').to_string();
            self.buf.replace_range(..=pos, "");
            let trimmed = line.trim();
            if trimmed.is_empty() || is_fence(trimmed) {
                continue;
            }
            lines.push(trimmed.to_string());
        }
        lines
    }

    pub fn finish(mut self) -> Vec<String> {
        self.strip_opening_fence();
        let trimmed = self.buf.trim();
        if trimmed.is_empty() || is_fence(trimmed) {
            Vec::new()
        } else {
            vec![trimmed.to_string()]
        }
    }

    fn strip_opening_fence(&mut self) {
        let trimmed_start = self.buf.trim_start();
        if let Some(rest) = trimmed_start.strip_prefix("```json") {
            self.buf = rest.trim_start_matches(['\r', '\n']).to_string();
        } else if let Some(rest) = trimmed_start.strip_prefix("```") {
            self.buf = rest.trim_start_matches(['\r', '\n']).to_string();
        }
    }
}

fn is_fence(line: &str) -> bool {
    line == "```" || line == "```json"
}

#[cfg(test)]
mod tests {
    use super::*;
    use eventsource_stream::Eventsource;
    use futures_util::stream;

    #[test]
    fn assembler_handles_fence_empty_and_partial_lines() {
        let mut a = LineAssembler::default();
        let first = a.push("```json\n{\"type\":\"overall\",\"text\":\"ok\"}\n{\"type\":");
        assert_eq!(first, vec![r#"{"type":"overall","text":"ok"}"#]);
        let second = a.push("\"correctedText\",\"text\":\"Hi.\"}\n```\n");
        assert_eq!(
            second,
            vec![r#"{"type":"correctedText","text":"Hi."}"#]
        );
        assert!(a.finish().is_empty());
    }

    #[test]
    fn extract_delta_ignores_done_and_empty() {
        assert_eq!(extract_delta_content("[DONE]"), None);
        assert_eq!(
            extract_delta_content(r#"{"choices":[{"delta":{"content":"He "}}]}"#).as_deref(),
            Some("He ")
        );
        assert_eq!(
            extract_delta_content(r#"{"choices":[{"delta":{}}]}"#),
            None
        );
    }

    #[test]
    fn sse_frames_across_arbitrary_utf8_chunks() {
        let payload = "你好世界";
        let json = format!(
            r#"{{"choices":[{{"delta":{{"content":{}}}}}]}}"#,
            serde_json::to_string(payload).unwrap()
        );
        let frame = format!("data: {json}\n\ndata: [DONE]\n\n");
        let bytes = frame.as_bytes();
        for split in 1..bytes.len() {
            let chunks = vec![
                Ok::<Bytes, reqwest::Error>(Bytes::copy_from_slice(&bytes[..split])),
                Ok(Bytes::copy_from_slice(&bytes[split..])),
            ];
            let assembled = tauri::async_runtime::block_on(async {
                let mut events = stream::iter(chunks).eventsource();
                let mut text = String::new();
                while let Some(item) = events.next().await {
                    let event = item.expect("sse frame");
                    if event.data.trim() == "[DONE]" {
                        break;
                    }
                    if let Some(delta) = extract_delta_content(&event.data) {
                        text.push_str(&delta);
                    }
                }
                text
            });
            assert_eq!(assembled, payload, "split at {split}");
        }
    }
}
