//! 句法讲解：拆开理解、整句意思、语法结构、句型骨架、理解要点。
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
pub const PROMPT_VERSION: &str = "syntax-v2";
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
pub enum ReadingTone {
    Literal,
    Natural,
}

/// 拆开理解的一块。`text` 必须是原文子串。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxChunk {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    pub text: String,
    pub gloss: String,
    pub notes: Vec<String>,
}

/// 整句的一种中文说法。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxReading {
    pub text: String,
    pub tone: ReadingTone,
}

/// 语法结构。`text` 必须是原文子串；`note` 用来写省略、从句等补充。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxStructure {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u32>,
    pub role: ConstituentRole,
    pub text: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 句型骨架。`text` 可以补上原文省略的词，例如 `(that)`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxSkeleton {
    pub role: ConstituentRole,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SyntaxResult {
    pub engine: String,
    pub input: String,
    pub chunks: Vec<SyntaxChunk>,
    pub readings: Vec<SyntaxReading>,
    pub structures: Vec<SyntaxStructure>,
    pub skeletons: Vec<SyntaxSkeleton>,
    pub tips: Vec<String>,
    pub duration_ms: u32,
}

impl SyntaxResult {
    pub fn primary_reading(&self) -> &str {
        self.readings.first().map(|r| r.text.as_str()).unwrap_or("")
    }
}

/// Channel 语义事件。前端只渲染这些，不接触原始 JSON/SSE。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SyntaxProgressEvent {
    Started { engine: String },
    Chunk { chunk: SyntaxChunk },
    Reading { reading: SyntaxReading },
    Structure { structure: SyntaxStructure },
    Skeleton { skeleton: SyntaxSkeleton },
    Tip { text: String },
    Retrying { reason: String },
    Finished,
}

#[derive(Debug, Clone, Default)]
pub struct SyntaxDraft {
    pub chunks: Vec<SyntaxChunk>,
    pub readings: Vec<SyntaxReading>,
    pub structures: Vec<SyntaxStructure>,
    pub skeletons: Vec<SyntaxSkeleton>,
    pub tips: Vec<String>,
    pub done: bool,
}

impl SyntaxDraft {
    pub fn ingest(
        &mut self,
        record: NdjsonRecord,
        input: &str,
    ) -> Result<Option<SyntaxProgressEvent>, ParseFail> {
        match record {
            NdjsonRecord::Chunk(raw) => {
                let chunk = attach_chunk(raw, input)?;
                self.chunks.push(chunk.clone());
                Ok(Some(SyntaxProgressEvent::Chunk { chunk }))
            }
            NdjsonRecord::Reading(raw) => {
                let reading = attach_reading(raw)?;
                self.readings.push(reading.clone());
                Ok(Some(SyntaxProgressEvent::Reading { reading }))
            }
            NdjsonRecord::Structure(raw) => {
                let structure = attach_structure(raw, input)?;
                self.structures.push(structure.clone());
                Ok(Some(SyntaxProgressEvent::Structure { structure }))
            }
            NdjsonRecord::Skeleton(raw) => {
                let skeleton = attach_skeleton(raw)?;
                self.skeletons.push(skeleton.clone());
                Ok(Some(SyntaxProgressEvent::Skeleton { skeleton }))
            }
            NdjsonRecord::Tip(raw) => {
                let text = nonempty(&raw.text).ok_or(ParseFail::MissingRecord)?;
                self.tips.push(text.clone());
                Ok(Some(SyntaxProgressEvent::Tip { text }))
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
        if self.chunks.is_empty() || self.readings.is_empty() || self.structures.is_empty() {
            return Err(ParseFail::MissingRecord);
        }
        Ok(SyntaxResult {
            engine,
            input,
            chunks: self.chunks,
            readings: self.readings,
            structures: self.structures,
            skeletons: self.skeletons,
            tips: self.tips,
            duration_ms: 0,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum NdjsonRecord {
    #[serde(rename = "chunk")]
    Chunk(NdjsonChunk),
    #[serde(rename = "reading")]
    Reading(NdjsonReading),
    #[serde(rename = "structure")]
    Structure(NdjsonStructure),
    #[serde(rename = "skeleton")]
    Skeleton(NdjsonSkeleton),
    #[serde(rename = "tip")]
    Tip(NdjsonTip),
    #[serde(rename = "done")]
    Done,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NdjsonChunk {
    pub text: String,
    pub gloss: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NdjsonReading {
    pub text: String,
    pub tone: ReadingTone,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NdjsonStructure {
    pub role: ConstituentRole,
    pub text: String,
    pub label: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NdjsonSkeleton {
    pub role: ConstituentRole,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NdjsonTip {
    pub text: String,
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
            Self::MissingRecord => "响应缺少拆解、整句意思或语法结构".into(),
            Self::SpanMissing => "拆解或语法结构的片段不在原文中".into(),
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

fn nonempty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn require_verbatim(text: &str, input: &str) -> Result<(), ParseFail> {
    if text.trim().is_empty() || !input.contains(text) {
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

fn attach_chunk(raw: NdjsonChunk, input: &str) -> Result<SyntaxChunk, ParseFail> {
    require_verbatim(&raw.text, input)?;
    let gloss = nonempty(&raw.gloss).ok_or(ParseFail::MissingRecord)?;
    let notes = raw
        .notes
        .into_iter()
        .filter_map(|note| nonempty(&note))
        .collect();
    let (offset, length) = span_offset(input, &raw.text);
    Ok(SyntaxChunk {
        offset,
        length,
        text: raw.text,
        gloss,
        notes,
    })
}

fn attach_reading(raw: NdjsonReading) -> Result<SyntaxReading, ParseFail> {
    let text = nonempty(&raw.text).ok_or(ParseFail::MissingRecord)?;
    Ok(SyntaxReading {
        text,
        tone: raw.tone,
    })
}

fn attach_structure(raw: NdjsonStructure, input: &str) -> Result<SyntaxStructure, ParseFail> {
    require_verbatim(&raw.text, input)?;
    let label = nonempty(&raw.label).ok_or(ParseFail::MissingRecord)?;
    let (offset, length) = span_offset(input, &raw.text);
    Ok(SyntaxStructure {
        offset,
        length,
        role: raw.role,
        text: raw.text,
        label,
        note: nonempty(&raw.note),
    })
}

fn attach_skeleton(raw: NdjsonSkeleton) -> Result<SyntaxSkeleton, ParseFail> {
    let text = nonempty(&raw.text).ok_or(ParseFail::MissingRecord)?;
    Ok(SyntaxSkeleton {
        role: raw.role,
        text,
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
    for chunk in &result.chunks {
        emit(SyntaxProgressEvent::Chunk {
            chunk: chunk.clone(),
        });
    }
    for reading in &result.readings {
        emit(SyntaxProgressEvent::Reading {
            reading: reading.clone(),
        });
    }
    for structure in &result.structures {
        emit(SyntaxProgressEvent::Structure {
            structure: structure.clone(),
        });
    }
    for skeleton in &result.skeletons {
        emit(SyntaxProgressEvent::Skeleton {
            skeleton: skeleton.clone(),
        });
    }
    for tip in &result.tips {
        emit(SyntaxProgressEvent::Tip { text: tip.clone() });
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

    const INPUT: &str = "and everything it ships targets Apple";

    fn ingest_minimum(draft: &mut SyntaxDraft, input: &str) {
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"chunk","text":"it ships","gloss":"它发布的所有东西","notes":["ship 在科技语境里常指发布、出货"]}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"reading","text":"并且它发布的所有产品都是针对苹果的。","tone":"literal"}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"structure","role":"subject","text":"everything it ships","label":"名词性短语，作主语","note":"相当于 everything that it ships，省略了 that"}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
    }

    #[test]
    fn ndjson_aggregates_modules_and_requires_done() {
        let mut draft = SyntaxDraft::default();
        ingest_minimum(&mut draft, INPUT);
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"chunk","text":"targets Apple","gloss":"以苹果为目标","notes":["target 作动词时是以……为目标"]}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"reading","text":"而且它推出的所有东西，都是冲着苹果来的。","tone":"natural"}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"structure","role":"predicate","text":"targets Apple","label":"谓语，动词加宾语","note":""}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"skeleton","role":"subject","text":"everything (that) it ships"}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"skeleton","role":"predicate","text":"targets Apple"}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(r#"{"type":"tip","text":"关系代词 that 被省略了。"}"#).unwrap(),
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
        assert_eq!(result.chunks[0].text, "it ships");
        assert_eq!(result.chunks[0].offset, Some(15));
        assert_eq!(result.chunks[0].notes.len(), 1);
        assert_eq!(result.readings[0].tone, ReadingTone::Literal);
        assert_eq!(result.readings[1].tone, ReadingTone::Natural);
        assert_eq!(result.structures[0].role, ConstituentRole::Subject);
        assert_eq!(result.structures[0].offset, Some(4));
        assert!(result.structures[0]
            .note
            .as_deref()
            .unwrap()
            .contains("that"));
        assert_eq!(result.structures[1].note, None);
        assert_eq!(result.skeletons[0].text, "everything (that) it ships");
        assert_eq!(result.tips.len(), 1);
        assert_eq!(
            result.primary_reading(),
            "并且它发布的所有产品都是针对苹果的。"
        );
    }

    #[test]
    fn tips_and_skeleton_are_optional() {
        let input = "我昨天买的书很贵。";
        let mut draft = SyntaxDraft::default();
        draft
            .ingest(
                parse_ndjson_line(r#"{"type":"chunk","text":"书","gloss":"书","notes":[]}"#)
                    .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"reading","text":"我昨天买的书很贵。","tone":"literal"}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"structure","role":"subject","text":"书","label":"主语"}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(parse_ndjson_line(r#"{"type":"done"}"#).unwrap(), input)
            .unwrap();
        let result = draft.finalize("LLM".into(), input.into()).unwrap();
        assert!(result.skeletons.is_empty());
        assert!(result.tips.is_empty());
        assert_eq!(result.structures[0].note, None);
        assert_eq!(result.chunks[0].offset, Some(5));
    }

    #[test]
    fn repeated_span_has_no_offset() {
        let input = "I saw a cat and a cat.";
        let mut draft = SyntaxDraft::default();
        draft
            .ingest(
                parse_ndjson_line(r#"{"type":"chunk","text":"a cat","gloss":"一只猫","notes":[]}"#)
                    .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"reading","text":"我看见一只猫和一只猫。","tone":"literal"}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"structure","role":"object","text":"a cat","label":"宾语"}"#,
                )
                .unwrap(),
                input,
            )
            .unwrap();
        draft
            .ingest(parse_ndjson_line(r#"{"type":"done"}"#).unwrap(), input)
            .unwrap();
        let result = draft.finalize("LLM".into(), input.into()).unwrap();
        assert_eq!(result.chunks[0].offset, None);
        assert_eq!(result.structures[0].offset, None);
    }

    #[test]
    fn missing_reading_is_incomplete() {
        let mut draft = SyntaxDraft::default();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"chunk","text":"it ships","gloss":"它发布的","notes":[]}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"structure","role":"subject","text":"everything it ships","label":"主语"}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
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
            parse_ndjson_line(r#"{"type":"chunk","text":"she","gloss":"她","notes":[]}"#).unwrap(),
            INPUT,
        );
        assert!(matches!(err, Err(ParseFail::SpanMissing)));
    }

    #[test]
    fn skeleton_may_add_words_missing_from_input() {
        let mut draft = SyntaxDraft::default();
        ingest_minimum(&mut draft, INPUT);
        draft
            .ingest(
                parse_ndjson_line(
                    r#"{"type":"skeleton","role":"subject","text":"everything (that) it ships"}"#,
                )
                .unwrap(),
                INPUT,
            )
            .unwrap();
        draft
            .ingest(parse_ndjson_line(r#"{"type":"done"}"#).unwrap(), INPUT)
            .unwrap();
        let result = draft.finalize("LLM".into(), INPUT.into()).unwrap();
        assert!(!INPUT.contains(&result.skeletons[0].text));
    }

    #[test]
    fn illegal_enum_is_parse_failure() {
        assert!(matches!(
            parse_ndjson_line(
                r#"{"type":"structure","role":"topic","text":"it ships","label":"主语"}"#
            ),
            Err(ParseFail::InvalidLine(_))
        ));
        assert!(matches!(
            parse_ndjson_line(r#"{"type":"reading","text":"好","tone":"loose"}"#),
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
    fn cache_key_uses_syntax_v2() {
        let key = cache_key("hi", "gpt-4o-mini", "https://api.openai.com/v1");
        assert!(key.starts_with("syntax-v2"));
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
