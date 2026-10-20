//! 句法分析领域：面向学习者的句型、时态、成分与从句。
//!
//! 只接通 LLM。缓存、取消与语法检查分开，不收录错题。

pub mod llm;

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::providers::grammar::unique_char_span;
use crate::providers::{provider_label, EngineError, ProviderConfig, PROVIDER_LLM};
use crate::services::{self, transport::Transport};

pub const ENGINE_LLM: &str = "llm";
pub const MAX_INPUT_CHARS: usize = 4000;
pub const PROMPT_VERSION: &str = "syntax-v1";
const CACHE_CAP: usize = 32;

/// 句法请求：默认且只允许 `llm`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxRequest {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// 前端 runId；新请求会作废仍在飞行的旧分析。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxLanguage {
    Zh,
    En,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SentenceType {
    Declarative,
    Interrogative,
    Imperative,
    Exclamatory,
    Fragment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxTense {
    Present,
    Past,
    Future,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxAspect {
    Simple,
    Progressive,
    Perfect,
    PerfectProgressive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SyntaxVoice {
    Active,
    Passive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConstituentRole {
    Subject,
    Predicate,
    Object,
    Attribute,
    Adverbial,
    Complement,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ClauseKind {
    Main,
    Relative,
    Noun,
    Adverbial,
    Coordinate,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxProfile {
    pub language: SyntaxLanguage,
    pub sentence_type: SentenceType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tense: Option<SyntaxTense>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect: Option<SyntaxAspect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<SyntaxVoice>,
}

/// 句子成分。`text` 必须是原文子串；偏移仅在唯一匹配时写入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxConstituent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    pub role: ConstituentRole,
    pub text: String,
    pub note: String,
}

/// 从句。`text` 必须是原文子串；偏移仅在唯一匹配时写入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxClause {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    pub kind: ClauseKind,
    pub text: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxResult {
    pub engine: String,
    pub input: String,
    pub summary: String,
    pub profile: SyntaxProfile,
    pub constituents: Vec<SyntaxConstituent>,
    pub clauses: Vec<SyntaxClause>,
    pub duration_ms: u32,
}

/// Channel 语义事件。前端只渲染这些，不接触原始 JSON/SSE。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SyntaxProgressEvent {
    Started { engine: String },
    Summary { text: String },
    Profile { profile: SyntaxProfile },
    Constituent { constituent: SyntaxConstituent },
    Clause { clause: SyntaxClause },
    Retrying { reason: String },
    Finished,
}

#[derive(Debug, Clone, Default)]
pub struct SyntaxDraft {
    pub summary: Option<String>,
    pub profile: Option<SyntaxProfile>,
    pub constituents: Vec<SyntaxConstituent>,
    pub clauses: Vec<SyntaxClause>,
    pub done: bool,
}

impl SyntaxDraft {
    pub fn ingest(
        &mut self,
        record: NdjsonRecord,
        input: &str,
    ) -> Result<Option<SyntaxProgressEvent>, ParseFail> {
        match record {
            NdjsonRecord::Summary { text } => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Err(ParseFail::MissingRecord);
                }
                self.summary = Some(text.clone());
                Ok(Some(SyntaxProgressEvent::Summary { text }))
            }
            NdjsonRecord::Profile(raw) => {
                let profile = raw.into_profile();
                self.profile = Some(profile.clone());
                Ok(Some(SyntaxProgressEvent::Profile { profile }))
            }
            NdjsonRecord::Constituent(raw) => {
                let constituent = attach_constituent(raw, input)?;
                self.constituents.push(constituent.clone());
                Ok(Some(SyntaxProgressEvent::Constituent { constituent }))
            }
            NdjsonRecord::Clause(raw) => {
                let clause = attach_clause(raw, input)?;
                self.clauses.push(clause.clone());
                Ok(Some(SyntaxProgressEvent::Clause { clause }))
            }
            NdjsonRecord::Done => {
                self.done = true;
                Ok(None)
            }
        }
    }

    pub fn finalize(self, engine: String, input: String) -> Result<SyntaxResult, ParseFail> {
        if !self.done {
            return Err(ParseFail::MissingDone);
        }
        let (Some(summary), Some(profile)) = (self.summary, self.profile) else {
            return Err(ParseFail::MissingRecord);
        };
        Ok(SyntaxResult {
            engine,
            input,
            summary,
            profile,
            constituents: self.constituents,
            clauses: self.clauses,
            duration_ms: 0,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum NdjsonRecord {
    #[serde(rename = "summary")]
    Summary { text: String },
    #[serde(rename = "profile")]
    Profile(NdjsonProfile),
    #[serde(rename = "constituent")]
    Constituent(NdjsonConstituent),
    #[serde(rename = "clause")]
    Clause(NdjsonClause),
    #[serde(rename = "done")]
    Done,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NdjsonProfile {
    pub language: SyntaxLanguage,
    pub sentence_type: SentenceType,
    #[serde(default)]
    pub tense: Option<SyntaxTense>,
    #[serde(default)]
    pub aspect: Option<SyntaxAspect>,
    #[serde(default)]
    pub voice: Option<SyntaxVoice>,
}

impl NdjsonProfile {
    fn into_profile(self) -> SyntaxProfile {
        SyntaxProfile {
            language: self.language,
            sentence_type: self.sentence_type,
            tense: self.tense,
            aspect: self.aspect,
            voice: self.voice,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NdjsonConstituent {
    pub role: ConstituentRole,
    pub text: String,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NdjsonClause {
    pub kind: ClauseKind,
    pub text: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFail {
    InvalidLine(String),
    MissingDone,
    MissingRecord,
    SpanMissing,
}

impl ParseFail {
    pub fn reason(&self) -> String {
        match self {
            Self::InvalidLine(line) => format!("无法解析记录：{line}"),
            Self::MissingDone => "响应缺少结束记录".into(),
            Self::MissingRecord => "响应缺少总评或句型".into(),
            Self::SpanMissing => "成分或从句缺少有效原文片段或说明".into(),
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

fn require_span(text: &str, note: &str, input: &str) -> Result<(), ParseFail> {
    let text = text.trim();
    let note = note.trim();
    if text.is_empty() || note.is_empty() || !input.contains(text) {
        return Err(ParseFail::SpanMissing);
    }
    Ok(())
}

fn span_offset(input: &str, text: &str) -> (Option<u32>, Option<u32>) {
    match unique_char_span(input, text) {
        Some((offset, length)) => (Some(offset), Some(length)),
        None => (None, None),
    }
}

fn attach_constituent(raw: NdjsonConstituent, input: &str) -> Result<SyntaxConstituent, ParseFail> {
    require_span(&raw.text, &raw.note, input)?;
    let (offset, length) = span_offset(input, &raw.text);
    Ok(SyntaxConstituent {
        offset,
        length,
        role: raw.role,
        text: raw.text,
        note: raw.note.trim().to_string(),
    })
}

fn attach_clause(raw: NdjsonClause, input: &str) -> Result<SyntaxClause, ParseFail> {
    require_span(&raw.text, &raw.note, input)?;
    let (offset, length) = span_offset(input, &raw.text);
    Ok(SyntaxClause {
        offset,
        length,
        kind: raw.kind,
        text: raw.text,
        note: raw.note.trim().to_string(),
    })
}

pub fn validate_request(request: &SyntaxRequest) -> Result<(), EngineError> {
    let text = request.text.trim();
    if text.is_empty() {
        return Err(EngineError::BadRequest {
            provider: "syntax".into(),
            detail: "文本为空".into(),
        });
    }
    if request.text.chars().count() > MAX_INPUT_CHARS {
        return Err(EngineError::BadRequest {
            provider: "syntax".into(),
            detail: format!("文本超过 {MAX_INPUT_CHARS} 个字符"),
        });
    }
    Ok(())
}

pub fn resolve_engine(request: &SyntaxRequest) -> Result<&'static str, EngineError> {
    match request
        .engine
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None | Some(ENGINE_LLM) => Ok(ENGINE_LLM),
        Some(other) => Err(EngineError::InvalidConfig {
            provider: other.into(),
            detail: format!("句法分析仅支持 llm 引擎，收到：{other}"),
        }),
    }
}

struct LruCache {
    map: HashMap<String, SyntaxResult>,
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

    fn get(&mut self, key: &str) -> Option<SyntaxResult> {
        if !self.map.contains_key(key) {
            return None;
        }
        self.order.retain(|k| k != key);
        self.order.push_back(key.to_string());
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: String, value: SyntaxResult) {
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

pub fn begin_syntax_job() -> u64 {
    ACTIVE_JOB.fetch_add(1, Ordering::SeqCst) + 1
}

pub fn syntax_job_is_current(id: u64) -> bool {
    ACTIVE_JOB.load(Ordering::SeqCst) == id
}

pub fn cache_key(input: &str, model: &str, base_url: &str) -> String {
    format!("{PROMPT_VERSION}\n{model}\n{base_url}\n{input}")
}

pub fn cache_get(key: &str) -> Option<SyntaxResult> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(key)
}

pub fn cache_put(key: String, value: SyntaxResult) {
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

fn replay_result(result: &SyntaxResult, emit: &mut impl FnMut(SyntaxProgressEvent)) {
    emit(SyntaxProgressEvent::Started {
        engine: result.engine.clone(),
    });
    emit(SyntaxProgressEvent::Summary {
        text: result.summary.clone(),
    });
    emit(SyntaxProgressEvent::Profile {
        profile: result.profile.clone(),
    });
    for constituent in &result.constituents {
        emit(SyntaxProgressEvent::Constituent {
            constituent: constituent.clone(),
        });
    }
    for clause in &result.clauses {
        emit(SyntaxProgressEvent::Clause {
            clause: clause.clone(),
        });
    }
    emit(SyntaxProgressEvent::Finished);
}

/// 完整链路：校验 → 缓存 → LLM 适配器。
pub async fn run_syntax(
    app: &AppHandle,
    request: SyntaxRequest,
    emit: &mut impl FnMut(SyntaxProgressEvent),
    alive: &impl Fn() -> bool,
) -> Result<SyntaxResult, EngineError> {
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
        other => Err(EngineError::InvalidConfig {
            provider: "unknown".into(),
            detail: format!("未注册的句法引擎: {other}"),
        }),
    }
}

pub async fn check_llm(
    cfg: &ProviderConfig,
    api_key: String,
    text: String,
    transport: &dyn Transport,
    emit: &mut impl FnMut(SyntaxProgressEvent),
    alive: &impl Fn() -> bool,
) -> Result<SyntaxResult, EngineError> {
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

    const INPUT: &str = "The book that I bought was expensive.";

    fn summary_line() -> NdjsonRecord {
        parse_ndjson_line(r#"{"type":"summary","text":"含定语从句的过去时陈述句"}"#).unwrap()
    }

    fn profile_line() -> NdjsonRecord {
        parse_ndjson_line(
            r#"{"type":"profile","language":"en","sentenceType":"declarative","tense":"past","aspect":"simple","voice":"active"}"#,
        )
        .unwrap()
    }

    #[test]
    fn ndjson_aggregates_in_order_and_requires_done() {
        let mut draft = SyntaxDraft::default();
        draft.ingest(summary_line(), INPUT).unwrap();
        draft.ingest(profile_line(), INPUT).unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"constituent","role":"subject","text":"The book","note":"主语，名词短语"}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"constituent","role":"predicate","text":"was","note":"系动词"}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"clause","kind":"relative","text":"that I bought","note":"修饰 book"}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        assert!(matches!(
            draft.clone().finalize("LLM".into(), INPUT.into()),
            Err(ParseFail::MissingDone)
        ));
        draft
            .ingest(parse_ndjson_line(r#"{"type":"done"}"#).unwrap(), INPUT)
            .unwrap();
        let result = draft.finalize("LLM".into(), INPUT.into()).unwrap();
        assert_eq!(result.summary, "含定语从句的过去时陈述句");
        assert_eq!(result.profile.language, SyntaxLanguage::En);
        assert_eq!(result.profile.tense, Some(SyntaxTense::Past));
        assert_eq!(result.profile.voice, Some(SyntaxVoice::Active));
        assert_eq!(result.constituents[0].role, ConstituentRole::Subject);
        assert_eq!(result.constituents[0].offset, Some(0));
        assert_eq!(result.constituents[0].length, Some(8));
        assert_eq!(result.constituents[1].role, ConstituentRole::Predicate);
        assert_eq!(result.clauses[0].kind, ClauseKind::Relative);
        assert_eq!(result.clauses[0].offset, Some(9));
    }

    #[test]
    fn chinese_profile_may_omit_tense_and_voice() {
        let input = "我昨天买的书很贵。";
        let mut draft = SyntaxDraft::default();
        draft
            .ingest(
                parse_ndjson_line(r#"{"type":"summary","text":"带定语的陈述句"}"#).unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"profile","language":"zh","sentenceType":"declarative","tense":null,"aspect":null,"voice":null}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"constituent","role":"subject","text":"书","note":"主语"}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(parse_ndjson_line(r#"{"type":"done"}"#).unwrap(), input)
            .unwrap();
        let result = draft.finalize("LLM".into(), input.into()).unwrap();
        assert_eq!(result.profile.language, SyntaxLanguage::Zh);
        assert_eq!(result.profile.tense, None);
        assert_eq!(result.profile.voice, None);
        assert_eq!(result.constituents[0].offset, Some(5));
    }

    #[test]
    fn repeated_span_has_no_offset() {
        let input = "I saw a cat and a cat.";
        let mut draft = SyntaxDraft::default();
        draft.ingest(summary_line(), input).unwrap();
        draft.ingest(profile_line(), input).unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"constituent","role":"object","text":"a cat","note":"宾语"}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(parse_ndjson_line(r#"{"type":"done"}"#).unwrap(), input)
            .unwrap();
        let result = draft.finalize("LLM".into(), input.into()).unwrap();
        assert_eq!(result.constituents[0].offset, None);
        assert_eq!(result.constituents[0].length, None);
    }

    #[test]
    fn missing_profile_is_incomplete() {
        let mut draft = SyntaxDraft::default();
        draft.ingest(summary_line(), INPUT).unwrap();
        draft
            .ingest(parse_ndjson_line(r#"{"type":"done"}"#).unwrap(), INPUT)
            .unwrap();
        assert!(matches!(
            draft.finalize("LLM".into(), INPUT.into()),
            Err(ParseFail::MissingRecord)
        ));
    }

    #[test]
    fn span_outside_input_is_rejected() {
        let err = SyntaxDraft::default().ingest(
            parse_ndjson_line(
                r#"{"type":"constituent","role":"subject","text":"she","note":"主语"}"#,
            )
            .unwrap(),
            INPUT,
        );
        assert!(matches!(err, Err(ParseFail::SpanMissing)));
    }

    #[test]
    fn illegal_enum_is_parse_failure() {
        assert!(matches!(
            parse_ndjson_line(
                r#"{"type":"constituent","role":"topic","text":"The book","note":"主语"}"#
            ),
            Err(ParseFail::InvalidLine(_))
        ));
        assert!(matches!(
            parse_ndjson_line(
                r#"{"type":"profile","language":"en","sentenceType":"declarative","tense":"pluperfect"}"#
            ),
            Err(ParseFail::InvalidLine(_))
        ));
    }

    #[test]
    fn empty_and_oversize_requests_are_rejected() {
        assert!(validate_request(&SyntaxRequest {
            text: "   ".into(),
            engine: None,
            request_id: None,
        })
        .is_err());
        let long: String = "a".repeat(MAX_INPUT_CHARS + 1);
        assert!(validate_request(&SyntaxRequest {
            text: long,
            engine: None,
            request_id: None,
        })
        .is_err());
        assert!(validate_request(&SyntaxRequest {
            text: "ok".into(),
            engine: None,
            request_id: None,
        })
        .is_ok());
    }

    #[test]
    fn unknown_engine_is_rejected() {
        let err = resolve_engine(&SyntaxRequest {
            text: "Hello".into(),
            engine: Some("languagetool".into()),
            request_id: None,
        })
        .unwrap_err();
        assert!(matches!(err, EngineError::InvalidConfig { .. }));
        assert_eq!(
            resolve_engine(&SyntaxRequest {
                text: "Hello".into(),
                engine: None,
                request_id: None,
            })
            .unwrap(),
            ENGINE_LLM
        );
    }

    #[test]
    fn cache_key_is_separate_from_grammar_prompt() {
        let key = cache_key("hi", "gpt-4o-mini", "https://api.openai.com/v1");
        assert!(key.starts_with("syntax-v1"));
        assert_ne!(key, cache_key("hi", "gpt-4o", "https://api.openai.com/v1"));
    }

    #[test]
    fn newer_job_supersedes_the_previous() {
        let first = begin_syntax_job();
        assert!(syntax_job_is_current(first));
        let second = begin_syntax_job();
        assert!(!syntax_job_is_current(first));
        assert!(syntax_job_is_current(second));
    }
}
