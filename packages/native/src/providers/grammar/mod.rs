//! 语法检查领域：标准类型、校验、偏移计算、进程内缓存与 Provider 分派。
//!
//! M2 只接通 LLM；`languagetool` 分支保留，不提前加配置/UI。

pub mod llm;
mod sse;

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::providers::{provider_label, EngineError, ProviderConfig, PROVIDER_LLM};
use crate::services::{self, transport::Transport};

pub const ENGINE_LLM: &str = "llm";
pub const ENGINE_LANGUAGETOOL: &str = "languagetool";
pub const MAX_INPUT_CHARS: usize = 4000;
pub const PROMPT_VERSION: &str = "v1";
const CACHE_CAP: usize = 32;

/// 语法请求：M2 默认且只允许 `llm`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrammarRequest {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// 前端 runId；新请求会作废仍在飞行的旧检查。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GrammarErrorType {
    Grammar,
    Spelling,
    Punctuation,
    WordChoice,
    Style,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum GrammarSeverity {
    Low,
    Medium,
    High,
}

/// 单条语法错误。`fragment` 是原文片段；偏移仅在唯一匹配时由 Rust 计算。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrammarError {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    pub fragment: String,
    pub correction: String,
    #[serde(rename = "type")]
    pub error_type: GrammarErrorType,
    pub severity: GrammarSeverity,
    pub explanation: String,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GrammarResult {
    pub engine: String,
    pub input: String,
    pub errors: Vec<GrammarError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrected_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overall: Option<String>,
    pub duration_ms: u32,
}

/// Channel 语义事件。前端只渲染这些，不接触原始 JSON/SSE。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GrammarProgressEvent {
    Started { engine: String },
    Overall { text: String },
    CorrectedText { text: String },
    Error { error: GrammarError },
    Retrying { reason: String },
    Finished,
}

#[derive(Debug, Clone, Default)]
pub struct GrammarDraft {
    pub overall: Option<String>,
    pub corrected_text: Option<String>,
    pub errors: Vec<GrammarError>,
    pub done: bool,
}

impl GrammarDraft {
    pub fn ingest(
        &mut self,
        record: NdjsonRecord,
        input: &str,
    ) -> Result<Option<GrammarProgressEvent>, ParseFail> {
        match record {
            NdjsonRecord::Overall { text } => {
                self.overall = Some(text.clone());
                Ok(Some(GrammarProgressEvent::Overall { text }))
            }
            NdjsonRecord::CorrectedText { text } => {
                self.corrected_text = Some(text.clone());
                Ok(Some(GrammarProgressEvent::CorrectedText { text }))
            }
            NdjsonRecord::Error(raw) => {
                let error = attach_offset(raw.into_error(), input);
                self.errors.push(error.clone());
                Ok(Some(GrammarProgressEvent::Error { error }))
            }
            NdjsonRecord::Done => {
                self.done = true;
                Ok(None)
            }
        }
    }

    pub fn finalize(self, engine: String, input: String) -> Result<GrammarResult, ParseFail> {
        if !self.done {
            return Err(ParseFail::MissingDone);
        }
        if self.overall.is_none() || self.corrected_text.is_none() {
            return Err(ParseFail::MissingRecord);
        }
        Ok(GrammarResult {
            engine,
            input,
            errors: self.errors,
            corrected_text: self.corrected_text,
            overall: self.overall,
            duration_ms: 0,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum NdjsonRecord {
    #[serde(rename = "overall")]
    Overall { text: String },
    #[serde(rename = "correctedText")]
    CorrectedText { text: String },
    #[serde(rename = "error")]
    Error(NdjsonError),
    #[serde(rename = "done")]
    Done,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NdjsonError {
    pub original: String,
    pub corrected: String,
    pub error_type: GrammarErrorType,
    pub severity: GrammarSeverity,
    pub explanation: String,
    #[serde(default)]
    pub suggestions: Vec<String>,
}

impl NdjsonError {
    fn into_error(self) -> GrammarError {
        GrammarError {
            offset: None,
            length: None,
            fragment: self.original,
            correction: self.corrected,
            error_type: self.error_type,
            severity: self.severity,
            explanation: self.explanation,
            suggestions: self.suggestions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFail {
    InvalidLine(String),
    MissingDone,
    MissingRecord,
}

impl ParseFail {
    pub fn reason(&self) -> String {
        match self {
            Self::InvalidLine(line) => format!("无法解析记录：{line}"),
            Self::MissingDone => "响应缺少结束记录".into(),
            Self::MissingRecord => "响应缺少总评或改写".into(),
        }
    }
}

pub fn parse_ndjson_line(line: &str) -> Result<NdjsonRecord, ParseFail> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("```") {
        return Err(ParseFail::InvalidLine(trimmed.into()));
    }
    serde_json::from_str(trimmed).map_err(|_| ParseFail::InvalidLine(trimmed.into()))
}

/// 仅在片段于原文中唯一出现时写入 Unicode 字符偏移。
pub fn unique_char_span(haystack: &str, needle: &str) -> Option<(u32, u32)> {
    if needle.is_empty() {
        return None;
    }
    let hay: Vec<char> = haystack.chars().collect();
    let ned: Vec<char> = needle.chars().collect();
    if hay.len() < ned.len() {
        return None;
    }
    let mut found: Option<usize> = None;
    for i in 0..=hay.len() - ned.len() {
        if hay[i..i + ned.len()] == ned[..] {
            if found.is_some() {
                return None;
            }
            found = Some(i);
        }
    }
    found.map(|start| (start as u32, ned.len() as u32))
}

pub fn attach_offset(mut error: GrammarError, input: &str) -> GrammarError {
    if let Some((offset, length)) = unique_char_span(input, &error.fragment) {
        error.offset = Some(offset);
        error.length = Some(length);
    }
    error
}

pub fn validate_request(request: &GrammarRequest) -> Result<(), EngineError> {
    let text = request.text.trim();
    if text.is_empty() {
        return Err(EngineError::BadRequest {
            provider: "grammar".into(),
            detail: "文本为空".into(),
        });
    }
    if request.text.chars().count() > MAX_INPUT_CHARS {
        return Err(EngineError::BadRequest {
            provider: "grammar".into(),
            detail: format!("文本超过 {MAX_INPUT_CHARS} 个字符"),
        });
    }
    Ok(())
}

pub fn resolve_engine(request: &GrammarRequest) -> Result<&'static str, EngineError> {
    match request
        .engine
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None | Some(ENGINE_LLM) => Ok(ENGINE_LLM),
        Some(ENGINE_LANGUAGETOOL) => Err(EngineError::InvalidConfig {
            provider: "LanguageTool".into(),
            detail: "LanguageTool 适配器将在后续里程碑提供".into(),
        }),
        Some(other) => Err(EngineError::InvalidConfig {
            provider: other.into(),
            detail: format!("M2 仅支持 llm 引擎，收到：{other}"),
        }),
    }
}

struct LruCache {
    map: HashMap<String, GrammarResult>,
    order: VecDeque<String>,
    cap: usize,
}

impl LruCache {
    fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            cap,
        }
    }

    fn get(&mut self, key: &str) -> Option<GrammarResult> {
        if !self.map.contains_key(key) {
            return None;
        }
        self.order.retain(|k| k != key);
        self.order.push_back(key.to_string());
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: String, value: GrammarResult) {
        if self.map.contains_key(&key) {
            self.order.retain(|k| k != &key);
        }
        self.order.push_back(key.clone());
        self.map.insert(key, value);
        while self.order.len() > self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
    }
}

static CACHE: LazyLock<Mutex<LruCache>> = LazyLock::new(|| Mutex::new(LruCache::new(CACHE_CAP)));
static ACTIVE_JOB: AtomicU64 = AtomicU64::new(0);

pub fn begin_grammar_job() -> u64 {
    ACTIVE_JOB.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn grammar_job_is_current(id: u64) -> bool {
    ACTIVE_JOB.load(Ordering::SeqCst) == id
}

pub fn cache_key(input: &str, model: &str, base_url: &str) -> String {
    format!("{PROMPT_VERSION}\n{model}\n{base_url}\n{input}")
}

pub fn cache_get(key: &str) -> Option<GrammarResult> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(key)
}

pub fn cache_put(key: String, value: GrammarResult) {
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key, value);
}

#[cfg(test)]
pub fn cache_clear() {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *cache = LruCache::new(CACHE_CAP);
}

fn replay_result(result: &GrammarResult, emit: &mut impl FnMut(GrammarProgressEvent)) {
    emit(GrammarProgressEvent::Started {
        engine: result.engine.clone(),
    });
    if let Some(text) = &result.overall {
        emit(GrammarProgressEvent::Overall { text: text.clone() });
    }
    if let Some(text) = &result.corrected_text {
        emit(GrammarProgressEvent::CorrectedText { text: text.clone() });
    }
    for error in &result.errors {
        emit(GrammarProgressEvent::Error {
            error: error.clone(),
        });
    }
    emit(GrammarProgressEvent::Finished);
}

/// 完整链路：校验 → 缓存 → LLM 适配器。LanguageTool 仅占位。
pub async fn run_grammar(
    app: &AppHandle,
    request: GrammarRequest,
    emit: &mut impl FnMut(GrammarProgressEvent),
    alive: &impl Fn() -> bool,
) -> Result<GrammarResult, EngineError> {
    validate_request(&request)?;
    let engine = resolve_engine(&request)?;
    match engine {
        ENGINE_LLM => {
            let config = services::settings::load_provider_config(app).map_err(|e| {
                EngineError::InvalidConfig {
                    provider: "settings".into(),
                    detail: e,
                }
            })?;
            let cfg = crate::providers::config_for(&config, PROVIDER_LLM);
            let key = services::secrets::get_api_key(app, PROVIDER_LLM).map_err(|_| {
                EngineError::MissingKey {
                    provider: provider_label(PROVIDER_LLM).into(),
                }
            })?;
            let transport = services::transport::shared();
            check_llm(cfg, key, request.text, &transport, emit, alive).await
        }
        ENGINE_LANGUAGETOOL => Err(EngineError::InvalidConfig {
            provider: "LanguageTool".into(),
            detail: "LanguageTool 适配器将在后续里程碑提供".into(),
        }),
        other => Err(EngineError::InvalidConfig {
            provider: "unknown".into(),
            detail: format!("未注册的语法引擎: {other}"),
        }),
    }
}

pub async fn check_llm(
    cfg: &ProviderConfig,
    api_key: String,
    text: String,
    transport: &dyn Transport,
    emit: &mut impl FnMut(GrammarProgressEvent),
    alive: &impl Fn() -> bool,
) -> Result<GrammarResult, EngineError> {
    let model = cfg.model.clone().unwrap_or_else(|| "gpt-4o-mini".into());
    let base = cfg.base_url.clone().unwrap_or_default();
    let key = cache_key(&text, &model, &base);
    if let Some(hit) = cache_get(&key) {
        if !alive() {
            return Err(EngineError::Cancelled {
                provider: provider_label(PROVIDER_LLM).into(),
            });
        }
        replay_result(&hit, emit);
        return Ok(hit);
    }

    let started = std::time::Instant::now();
    let mut result = llm::check_until(
        cfg,
        &api_key,
        &text,
        transport,
        emit,
        crate::services::transport::GRAMMAR_TIMEOUT,
        alive,
    )
    .await?;
    result.duration_ms = started.elapsed().as_millis() as u32;
    if alive() {
        cache_put(key, result.clone());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_span_uses_unicode_chars_not_bytes() {
        let input = "你好 world 你好";
        assert_eq!(unique_char_span(input, "world"), Some((3, 5)));
        assert_eq!(unique_char_span(input, "你好"), None);
        assert_eq!(unique_char_span("café", "é"), Some((3, 1)));
        assert_eq!(unique_char_span("aa", "a"), None);
        assert_eq!(unique_char_span("abc", "z"), None);
    }

    #[test]
    fn ndjson_aggregates_and_requires_done() {
        let input = "He go to school.";
        let mut draft = GrammarDraft::default();
        draft
            .ingest(
                parse_ndjson_line(r#"{"type":"overall","text":"主谓不一致"}"#).unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(r#"{"type":"correctedText","text":"He goes to school."}"#)
                    .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"error","original":"go","corrected":"goes","errorType":"grammar","severity":"high","explanation":"第三人称单数","suggestions":["goes"]}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        assert!(matches!(
            draft.clone().finalize("LLM".into(), input.into()),
            Err(ParseFail::MissingDone)
        ));
        draft
            .ingest(parse_ndjson_line(r#"{"type":"done"}"#).unwrap(), input)
            .unwrap();
        let result = draft.finalize("LLM".into(), input.into()).unwrap();
        assert_eq!(result.errors[0].offset, Some(3));
        assert_eq!(result.errors[0].length, Some(2));
        assert_eq!(result.overall.as_deref(), Some("主谓不一致"));
    }

    #[test]
    fn illegal_enum_is_parse_failure() {
        assert!(matches!(
            parse_ndjson_line(
                r#"{"type":"error","original":"a","corrected":"b","errorType":"syntax","severity":"high","explanation":"x","suggestions":[]}"#
            ),
            Err(ParseFail::InvalidLine(_))
        ));
    }

    #[test]
    fn empty_and_oversize_requests_are_rejected() {
        assert!(validate_request(&GrammarRequest {
            text: "   ".into(),
            engine: None,
            request_id: None,
        })
        .is_err());
        let long: String = "a".repeat(MAX_INPUT_CHARS + 1);
        assert!(validate_request(&GrammarRequest {
            text: long,
            engine: None,
            request_id: None,
        })
        .is_err());
        assert!(validate_request(&GrammarRequest {
            text: "ok".into(),
            engine: None,
            request_id: None,
        })
        .is_ok());
    }

    #[test]
    fn languagetool_is_reserved_not_implemented() {
        let err = resolve_engine(&GrammarRequest {
            text: "Hello".into(),
            engine: Some("languagetool".into()),
            request_id: None,
        })
        .unwrap_err();
        assert!(matches!(err, EngineError::InvalidConfig { .. }));
        assert_eq!(
            resolve_engine(&GrammarRequest {
                text: "Hello".into(),
                engine: None,
                request_id: None,
            })
            .unwrap(),
            ENGINE_LLM
        );
    }

    #[test]
    fn cache_key_changes_with_model_and_prompt() {
        let a = cache_key("hi", "gpt-4o-mini", "https://api.openai.com/v1");
        let b = cache_key("hi", "gpt-4o", "https://api.openai.com/v1");
        let c = cache_key("hi", "gpt-4o-mini", "https://example.com/v1");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with(PROMPT_VERSION));
    }

    #[test]
    fn newer_job_supersedes_the_previous() {
        let first = begin_grammar_job();
        assert!(grammar_job_is_current(first));
        let second = begin_grammar_job();
        assert!(!grammar_job_is_current(first));
        assert!(grammar_job_is_current(second));
    }
}
