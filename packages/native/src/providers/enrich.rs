//! 英语词卡：原文或译文是英语单词/短短语时，补音标、发音、关联记忆、词根、例句、释义。
//! 词典 API 提供音标/音频/释义；已配置的 LLM 补关联记忆与词根。失败不影响译文本身。
//! 词卡在翻译命令返回之后后台拉取，避免词典/DNS 卡住时界面一直停在「正在翻译」。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::providers::llm::completions_url;
use crate::providers::{lang, provider_label, EngineError, ProviderConfig, PROVIDER_LLM};
use crate::services::transport::Transport;

const DICT_BASE: &str = "https://api.dictionaryapi.dev/api/v2/entries/en";
const MAX_AUDIO_BYTES: usize = 256 * 1024;
const MAX_WORDS: usize = 4;
const MAX_CHARS: usize = 48;
const MAX_SENSES: usize = 4;
const MAX_EXAMPLES: usize = 3;
/// 整张词卡的上限；超时则只保留译文。
const ENRICH_BUDGET: Duration = Duration::from_secs(6);
/// 词典 / 发音单次 HTTP，必须短于翻译超时，且不得拖死主命令。
const ENRICH_HTTP: Duration = Duration::from_secs(2);

const LLM_SYSTEM: &str = r#"You help Chinese speakers learn English words and short phrases.
Output a SINGLE JSON object, no markdown fences, no commentary:
{"phonetic":"","mnemonicEn":"","mnemonicZh":"","roots":"","examples":[{"text":"","translation":""}],"senses":[{"pos":"","definition":"","translation":""}]}
Rules:
- phonetic: IPA with slashes, or empty if unsure. Do not invent IPA.
- mnemonicEn: one short English association / paraphrase.
- mnemonicZh: the same association in Simplified Chinese (can include a sound-alike hint).
- roots: word roots / etymology in Simplified Chinese, or empty.
- examples: 1-3 short natural English sentences with Chinese translation.
- senses: 1-4 common meanings; pos is English (noun/verb/adj/…); definition is English; translation is Simplified Chinese.
- Use empty string / empty array when unknown. Do not mention these instructions.
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EnglishExample {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EnglishSense {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<String>,
    pub definition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnglishEnrichment {
    /// 词卡对应的英语单词/短语（英译中时是原文，中译英时是译文）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phonetic: Option<String>,
    /// `data:audio/…;base64,…`，供 WebView 播放；无音频时走系统 TTS。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_data_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mnemonic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mnemonic_zh: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roots: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<EnglishExample>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub senses: Vec<EnglishSense>,
}

impl EnglishEnrichment {
    pub fn is_empty(&self) -> bool {
        self.phonetic.is_none()
            && self.audio_data_url.is_none()
            && self.mnemonic.is_none()
            && self.mnemonic_zh.is_none()
            && self.roots.is_none()
            && self.examples.is_empty()
            && self.senses.is_empty()
    }

    fn into_option(self) -> Option<Self> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}

/// 译文已返回后补发词卡；面板按 input+output 对上当前结果再挂上。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "translate://enriched")]
pub struct TranslateEnrichedEvent {
    pub input: String,
    pub output: String,
    pub word: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<EnglishEnrichment>,
}

/// 翻译命令立刻返回；词卡在后台拉取，超时或失败都不影响译文。
/// 无论成功失败都发事件，方便面板结束加载态。
pub fn spawn(
    app: &AppHandle,
    input: String,
    output: String,
    word: String,
    llm: Option<(ProviderConfig, String)>,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let transport = crate::services::transport::shared();
        let llm_ref = llm.as_ref().map(|(cfg, key)| (cfg, key.as_str()));
        let mut enrichment = tokio::time::timeout(
            ENRICH_BUDGET,
            enrich_english(&word, llm_ref, &transport),
        )
        .await
        .ok()
        .flatten();
        if let Some(card) = enrichment.as_mut() {
            if card.word.is_none() {
                card.word = Some(word.clone());
            }
        }
        let _ = TranslateEnrichedEvent {
            input,
            output,
            word,
            enrichment,
        }
        .emit(&app);
    });
}

/// 有可做词卡的英语单词：中译英看译文，英译中看原文。长段落只保留纯翻译。
pub fn english_headword(target: &str, input: &str, output: &str) -> Option<String> {
    if is_english_target(target) && is_short_phrase(output) {
        return normalize_headword(output);
    }
    if is_short_phrase(input) {
        return normalize_headword(input);
    }
    None
}

pub fn is_english_target(code: &str) -> bool {
    matches!(
        code.trim().to_ascii_lowercase().as_str(),
        "en" | "en-us" | "en-gb" | "en_us" | "en_gb" | "english"
    )
}

pub fn is_short_phrase(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.chars().count() > MAX_CHARS {
        return false;
    }
    if t.contains('\n') || !lang::has_latin(t) || lang::has_cjk(t) {
        return false;
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    if words.is_empty() || words.len() > MAX_WORDS {
        return false;
    }
    let ends = t.chars().filter(|c| matches!(c, '.' | '!' | '?')).count();
    ends <= 1
}

pub fn normalize_headword(text: &str) -> Option<String> {
    let trimmed = text.trim().trim_matches(|c: char| {
        matches!(
            c,
            '"' | '\''
                | '“'
                | '”'
                | '‘'
                | '’'
                | '('
                | ')'
                | '['
                | ']'
                | '.'
                | '!'
                | '?'
                | ','
                | ';'
                | ':'
        )
    });
    let lowered = trimmed.to_ascii_lowercase();
    let head = lowered.trim();
    if head.is_empty() || !lang::has_latin(head) {
        None
    } else {
        Some(head.to_string())
    }
}

pub async fn enrich_english(
    english: &str,
    llm: Option<(&ProviderConfig, &str)>,
    transport: &dyn Transport,
) -> Option<EnglishEnrichment> {
    enrich_english_at(DICT_BASE, english, llm, transport).await
}

async fn enrich_english_at(
    dict_base: &str,
    english: &str,
    llm: Option<(&ProviderConfig, &str)>,
    transport: &dyn Transport,
) -> Option<EnglishEnrichment> {
    let head = normalize_headword(english)?;
    let dict_fut = dictionary_lookup(dict_base, &head, transport);
    let llm_fut = async {
        match llm {
            Some((cfg, key)) => llm_enrich(cfg, key, &head, english, transport)
                .await
                .ok()
                .flatten(),
            None => None,
        }
    };
    let (dict, llm_card) = tokio::join!(dict_fut, llm_fut);
    let dict = dict.ok().flatten();
    merge(dict, llm_card).into_option()
}

async fn dictionary_lookup(
    base: &str,
    head: &str,
    transport: &dyn Transport,
) -> Result<Option<EnglishEnrichment>, EngineError> {
    let mut last = None;
    for candidate in lookup_candidates(head) {
        last = fetch_dictionary_entry(base, &candidate, transport).await?;
        if last.is_some() {
            break;
        }
    }
    Ok(last)
}

fn lookup_candidates(head: &str) -> Vec<String> {
    let mut out = vec![head.to_string()];
    if let Some(first) = head.split_whitespace().next() {
        if first != head {
            out.push(first.to_string());
        }
    }
    out
}

async fn fetch_dictionary_entry(
    base: &str,
    word: &str,
    transport: &dyn Transport,
) -> Result<Option<EnglishEnrichment>, EngineError> {
    let url = format!("{}/{}", base.trim_end_matches('/'), path_encode(word));
    let Some(resp) = get_limited(&url, &[("Accept", "application/json")], transport).await else {
        return Ok(None);
    };
    let status = resp.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Ok(None);
    }
    let body = resp.text().await.unwrap_or_default();
    let entries: Vec<DictEntry> = serde_json::from_str(&body).unwrap_or_default();
    let Some(entry) = entries.into_iter().next() else {
        return Ok(None);
    };
    let mut card = entry_to_card(entry);
    if let Some(audio_url) = card.audio_data_url.take() {
        card.audio_data_url = fetch_audio_data_url(&audio_url, transport).await;
    }
    Ok(card.into_option())
}

fn entry_to_card(entry: DictEntry) -> EnglishEnrichment {
    let phonetic = entry
        .phonetic
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            entry.phonetics.iter().flatten().find_map(|p| {
                p.text
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
        });
    let audio_url = entry.phonetics.iter().flatten().find_map(|p| {
        p.audio
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty() && is_allowed_audio_url(s))
            .map(str::to_string)
    });
    let mut senses = Vec::new();
    let mut examples = Vec::new();
    for meaning in entry.meanings.into_iter().flatten() {
        let pos = blank_to_none(meaning.part_of_speech);
        for def in meaning.definitions.into_iter().flatten() {
            if let Some(definition) = blank_to_none(def.definition) {
                if senses.len() < MAX_SENSES {
                    senses.push(EnglishSense {
                        pos: pos.clone(),
                        definition,
                        translation: None,
                    });
                }
            }
            if let Some(text) = blank_to_none(def.example) {
                if examples.len() < MAX_EXAMPLES {
                    examples.push(EnglishExample {
                        text,
                        translation: None,
                    });
                }
            }
        }
    }
    EnglishEnrichment {
        word: None,
        phonetic,
        audio_data_url: audio_url,
        mnemonic: None,
        mnemonic_zh: None,
        roots: blank_to_none(entry.origin),
        examples,
        senses,
    }
}

async fn fetch_audio_data_url(url: &str, transport: &dyn Transport) -> Option<String> {
    if !is_allowed_audio_url(url) {
        return None;
    }
    let resp = get_limited(url, &[], transport).await?;
    if !resp.status().is_success() {
        return None;
    }
    if resp.content_length().unwrap_or(0) > MAX_AUDIO_BYTES as u64 {
        return None;
    }
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .filter(|s| s.starts_with("audio/"))
        .unwrap_or_else(|| guess_audio_mime(url).into());
    let bytes = resp.bytes().await.ok()?;
    if bytes.is_empty() || bytes.len() > MAX_AUDIO_BYTES {
        return None;
    }
    Some(format!("data:{mime};base64,{}", encode_base64(&bytes)))
}

fn is_allowed_audio_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://api.dictionaryapi.dev/")
        || lower.starts_with("https://ssl.gstatic.com/")
}

fn guess_audio_mime(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains(".ogg") {
        "audio/ogg"
    } else if lower.contains(".wav") {
        "audio/wav"
    } else {
        "audio/mpeg"
    }
}

async fn llm_enrich(
    cfg: &ProviderConfig,
    api_key: &str,
    head: &str,
    english: &str,
    transport: &dyn Transport,
) -> Result<Option<EnglishEnrichment>, EngineError> {
    let provider = provider_label(PROVIDER_LLM);
    let Some(base) = cfg.base_url.clone() else {
        return Ok(None);
    };
    let Some(model) = cfg.model.clone() else {
        return Ok(None);
    };
    let user = format!("English headword: {head}\nEnglish text: {english}");
    let body = json!({
        "model": model,
        "messages": [
            { "role": "system", "content": LLM_SYSTEM },
            { "role": "user", "content": user }
        ],
        "temperature": 0.4,
        "max_tokens": cfg.max_tokens.unwrap_or(1024),
    });
    let auth = format!("Bearer {api_key}");
    let resp = match tokio::time::timeout(
        ENRICH_BUDGET,
        transport.post_json_with_timeout(
            &completions_url(&base),
            &[("Authorization", auth.as_str())],
            body,
            ENRICH_BUDGET,
        ),
    )
    .await
    {
        Ok(Ok(resp)) => resp,
        _ => return Ok(None),
    };
    let status = resp.status().as_u16();
    let raw = resp.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Ok(None);
    }
    let parsed: ChatResponse =
        serde_json::from_str(&raw).map_err(|e| EngineError::InvalidResponse {
            provider: provider.into(),
            detail: e.to_string(),
        })?;
    let content = parsed
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .map(|s| strip_fences(&s))
        .filter(|s| !s.is_empty());
    let Some(content) = content else {
        return Ok(None);
    };
    let parsed: LlmCard = serde_json::from_str(&content).unwrap_or(LlmCard::default());
    Ok(llm_card_to_enrichment(parsed).into_option())
}

fn llm_card_to_enrichment(card: LlmCard) -> EnglishEnrichment {
    let (mnemonic, mnemonic_zh) = pick_mnemonic(&card);
    EnglishEnrichment {
        word: None,
        phonetic: blank_to_none(card.phonetic),
        audio_data_url: None,
        mnemonic,
        mnemonic_zh,
        roots: blank_to_none(card.roots),
        examples: card
            .examples
            .into_iter()
            .filter_map(|ex| {
                blank_to_none(ex.text).map(|text| EnglishExample {
                    text,
                    translation: blank_to_none(ex.translation),
                })
            })
            .take(MAX_EXAMPLES)
            .collect(),
        senses: card
            .senses
            .into_iter()
            .filter_map(llm_sense)
            .take(MAX_SENSES)
            .collect(),
    }
}

fn pick_mnemonic(card: &LlmCard) -> (Option<String>, Option<String>) {
    let en = blank_to_none(card.mnemonic_en.clone());
    let zh = blank_to_none(card.mnemonic_zh.clone());
    if en.is_some() || zh.is_some() {
        return (en, zh);
    }
    split_bilingual(blank_to_none(card.mnemonic.clone()))
}

fn split_bilingual(value: Option<String>) -> (Option<String>, Option<String>) {
    let Some(value) = value else {
        return (None, None);
    };
    let cjk = lang::has_cjk(&value);
    let latin = lang::has_latin(&value);
    if cjk && !latin {
        (None, Some(value))
    } else if latin && !cjk {
        (Some(value), None)
    } else {
        (None, Some(value))
    }
}

fn llm_sense(sense: LlmSense) -> Option<EnglishSense> {
    let pos = blank_to_none(sense.pos);
    let definition = blank_to_none(sense.definition);
    let translation = blank_to_none(sense.translation);
    match (definition, translation) {
        (Some(definition), Some(translation))
            if lang::has_cjk(&definition) && !lang::has_cjk(&translation) =>
        {
            Some(EnglishSense {
                pos,
                definition: translation,
                translation: Some(definition),
            })
        }
        (Some(definition), translation) => Some(EnglishSense {
            pos,
            definition,
            translation,
        }),
        (None, Some(translation)) => Some(EnglishSense {
            pos,
            definition: translation,
            translation: None,
        }),
        (None, None) => None,
    }
}

fn merge(dict: Option<EnglishEnrichment>, llm: Option<EnglishEnrichment>) -> EnglishEnrichment {
    let mut out = dict.unwrap_or_default();
    let Some(llm) = llm else {
        return out;
    };
    if out.word.is_none() {
        out.word = llm.word;
    }
    if out.phonetic.is_none() {
        out.phonetic = llm.phonetic;
    }
    if out.mnemonic.is_none() {
        out.mnemonic = llm.mnemonic;
    }
    if out.mnemonic_zh.is_none() {
        out.mnemonic_zh = llm.mnemonic_zh;
    }
    if out.roots.is_none() {
        out.roots = llm.roots;
    }
    out.senses = merge_senses(out.senses, llm.senses);
    if out.examples.is_empty() {
        out.examples = llm.examples;
    } else if out.examples.iter().all(|e| e.translation.is_none()) && !llm.examples.is_empty() {
        // 词典例句没有中译时，用 LLM 例句替换，避免只剩英文空壳。
        out.examples = llm.examples;
    }
    out
}

fn merge_senses(dict: Vec<EnglishSense>, llm: Vec<EnglishSense>) -> Vec<EnglishSense> {
    if dict.is_empty() {
        return llm;
    }
    if llm.is_empty() {
        return dict;
    }
    dict.into_iter()
        .enumerate()
        .map(|(index, mut sense)| {
            let Some(llm_sense) = llm.get(index) else {
                return sense;
            };
            if sense.translation.is_none() {
                sense.translation = llm_sense.translation.clone().or_else(|| {
                    if lang::has_cjk(&llm_sense.definition) {
                        Some(llm_sense.definition.clone())
                    } else {
                        None
                    }
                });
            }
            sense
        })
        .collect()
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn strip_fences(content: &str) -> String {
    let mut s = content.trim().to_string();
    if let Some(rest) = s.strip_prefix("```json") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix("```") {
        s = rest.to_string();
    }
    if let Some(rest) = s.strip_suffix("```") {
        s = rest.to_string();
    }
    s.trim().to_string()
}

async fn get_limited(
    url: &str,
    headers: &[(&str, &str)],
    transport: &dyn Transport,
) -> Option<reqwest::Response> {
    tokio::time::timeout(ENRICH_HTTP, transport.get_with_headers(url, headers))
        .await
        .ok()?
        .ok()
}

fn path_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn encode_base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let a = chunk[0] as u32;
        let b = chunk.get(1).copied().unwrap_or(0) as u32;
        let c = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (a << 16) | (b << 8) | c;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct DictEntry {
    phonetic: Option<String>,
    phonetics: Option<Vec<DictPhonetic>>,
    origin: Option<String>,
    meanings: Option<Vec<DictMeaning>>,
}

#[derive(Debug, Deserialize)]
struct DictPhonetic {
    text: Option<String>,
    audio: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DictMeaning {
    part_of_speech: Option<String>,
    definitions: Option<Vec<DictDefinition>>,
}

#[derive(Debug, Deserialize)]
struct DictDefinition {
    definition: Option<String>,
    example: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LlmCard {
    #[serde(default)]
    phonetic: Option<String>,
    #[serde(default)]
    mnemonic: Option<String>,
    #[serde(default)]
    mnemonic_en: Option<String>,
    #[serde(default)]
    mnemonic_zh: Option<String>,
    #[serde(default)]
    roots: Option<String>,
    #[serde(default)]
    examples: Vec<LlmExample>,
    #[serde(default)]
    senses: Vec<LlmSense>,
}

#[derive(Debug, Default, Deserialize)]
struct LlmExample {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    translation: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct LlmSense {
    #[serde(default)]
    pos: Option<String>,
    #[serde(default)]
    definition: Option<String>,
    #[serde(default)]
    translation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::transport::ReqwestTransport;

    #[test]
    fn enriched_event_name() {
        assert_eq!(TranslateEnrichedEvent::NAME, "translate://enriched");
    }

    #[test]
    fn english_short_word_enriches() {
        assert!(english_headword("en", "纪元", "epoch").is_some());
        assert!(english_headword("zh", "able", "能够").is_some());
        assert!(english_headword("EN-US", "查找", "look up").is_some());
        assert!(english_headword("en", "你好", "Hello!").is_some());
        assert_eq!(
            english_headword("zh", "able", "能够").as_deref(),
            Some("able")
        );
        assert_eq!(
            english_headword("en", "能够", "able").as_deref(),
            Some("able")
        );
    }

    #[test]
    fn long_or_non_english_skips() {
        assert!(english_headword(
            "zh",
            "这是一段很长的中文，不应出词卡",
            "This is a long paragraph that should translate normally without a word card."
        )
        .is_none());
        assert!(english_headword("en", "你好", "你好").is_none());
        assert!(english_headword("en", "Hello\nworld", "你好").is_none());
        assert!(english_headword(
            "zh",
            "This is a long english paragraph that is not a dictionary headword.",
            "这不是单词"
        )
        .is_none());
    }

    #[test]
    fn headword_strips_punctuation() {
        assert_eq!(normalize_headword("  Hello! ").as_deref(), Some("hello"));
        assert_eq!(normalize_headword("\"Epoch\"").as_deref(), Some("epoch"));
        assert!(normalize_headword("   ").is_none());
    }

    #[test]
    fn merge_prefers_dictionary_audio_and_llm_mnemonic() {
        let dict = EnglishEnrichment {
            phonetic: Some("/ˈepək/".into()),
            audio_data_url: Some("data:audio/mpeg;base64,QQ==".into()),
            senses: vec![EnglishSense {
                pos: Some("noun".into()),
                definition: "a period of time".into(),
                translation: None,
            }],
            ..Default::default()
        };
        let llm = EnglishEnrichment {
            phonetic: Some("/wrong/".into()),
            mnemonic: Some("a page of history".into()),
            mnemonic_zh: Some("e-poch 像一页历史".into()),
            roots: Some("epi + och".into()),
            senses: vec![EnglishSense {
                pos: Some("noun".into()),
                definition: "a period of time".into(),
                translation: Some("一段时期".into()),
            }],
            ..Default::default()
        };
        let out = merge(Some(dict), Some(llm));
        assert_eq!(out.phonetic.as_deref(), Some("/ˈepək/"));
        assert!(out.audio_data_url.is_some());
        assert_eq!(out.mnemonic.as_deref(), Some("a page of history"));
        assert_eq!(out.mnemonic_zh.as_deref(), Some("e-poch 像一页历史"));
        assert_eq!(out.roots.as_deref(), Some("epi + och"));
        assert_eq!(out.senses[0].translation.as_deref(), Some("一段时期"));
    }

    #[test]
    fn pick_mnemonic_prefers_bilingual_fields() {
        let card = LlmCard {
            mnemonic_en: Some("able to do it".into()),
            mnemonic_zh: Some("能够做某事".into()),
            mnemonic: Some("ignored".into()),
            ..Default::default()
        };
        let (en, zh) = pick_mnemonic(&card);
        assert_eq!(en.as_deref(), Some("able to do it"));
        assert_eq!(zh.as_deref(), Some("能够做某事"));
    }

    #[test]
    fn dictionary_mock_fills_phonetic_and_senses() {
        let mut server = mockito::Server::new();
        let mock = server
            .mock("GET", "/hello")
            .with_status(200)
            .with_body(
                r#"[{"word":"hello","phonetic":"/həˈloʊ/","phonetics":[{"text":"/həˈloʊ/","audio":""}],"meanings":[{"partOfSpeech":"exclamation","definitions":[{"definition":"used as a greeting","example":"hello there"}]}]}]"#,
            )
            .create();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            enrich_english_at(&server.url(), "Hello", None, &transport).await
        });
        mock.assert();
        let out = out.expect("dictionary enrichment");
        assert_eq!(out.phonetic.as_deref(), Some("/həˈloʊ/"));
        assert_eq!(out.senses[0].definition, "used as a greeting");
        assert_eq!(out.examples[0].text, "hello there");
        assert!(out.audio_data_url.is_none());
    }

    #[test]
    fn dictionary_failure_does_not_panic() {
        let mut server = mockito::Server::new();
        let mock = server.mock("GET", "/nope").with_status(404).create();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            enrich_english_at(&server.url(), "nope", None, &transport).await
        });
        mock.assert();
        assert!(out.is_none());
    }

    #[test]
    fn hanging_dictionary_times_out() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let _keep = listener;
            std::thread::sleep(std::time::Duration::from_secs(30));
        });
        let started = std::time::Instant::now();
        let out = tauri::async_runtime::block_on(async {
            let transport = ReqwestTransport::default();
            enrich_english_at(&format!("http://{addr}"), "hello", None, &transport).await
        });
        assert!(out.is_none());
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn base64_encodes_padding() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
    }
}
