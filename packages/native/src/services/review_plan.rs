//! 复习方案：本地分桶 + 持久化会话。LLM 只写文案。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::clips::{self, Clip, ClipFilter, KIND_PHRASE, KIND_SENTENCE, KIND_WORD};
use super::mistakes::{
    self, error_type_label, Mistake, MistakeFilter, MistakeTimeRange, STATUS_OPEN,
};

pub const PLAN_STATUS_ACTIVE: &str = "active";
pub const PLAN_STATUS_ARCHIVED: &str = "archived";
pub const SOURCE_MISTAKE: &str = "mistake";
pub const SOURCE_CLIP: &str = "clip";
pub const DAY_PENDING: &str = "pending";
pub const DAY_DONE: &str = "done";
pub const ITEM_CORRECT: &str = "correct";
pub const ITEM_WRONG: &str = "wrong";
pub const ITEM_SKIPPED: &str = "skipped";
pub const ITEM_MASTERED: &str = "mastered";

const MAX_PER_DAY: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPlanDay {
    pub day_index: i32,
    pub title: String,
    pub error_types: Vec<String>,
    pub goals: Vec<String>,
    pub drill: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPlanItem {
    pub id: i32,
    pub day_index: i32,
    pub sort_order: i32,
    pub source_kind: String,
    pub source_id: i32,
    pub prompt_text: String,
    pub expected: String,
    pub hint: Option<String>,
    pub fragment: Option<String>,
    pub result: Option<String>,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPlan {
    pub id: i32,
    pub created_at: String,
    pub status: String,
    pub horizon_days: i32,
    pub analyzed_count: i32,
    pub summary: String,
    pub engine: Option<String>,
    pub duration_ms: Option<i32>,
    pub prompt_version: Option<String>,
    pub current_day: i32,
    pub days: Vec<ReviewPlanDay>,
    pub items: Vec<ReviewPlanItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewGradeRequest {
    pub item_id: i32,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub mark_learned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviewGradeResult {
    pub item: ReviewPlanItem,
    pub matched: bool,
    pub plan: ReviewPlan,
}

#[derive(Debug, Clone)]
pub struct DraftItem {
    pub source_kind: String,
    pub source_id: i32,
    pub bucket_key: String,
    pub prompt_text: String,
    pub expected: String,
    pub hint: Option<String>,
    pub fragment: Option<String>,
    pub day_index: i32,
    pub sort_order: i32,
}

#[derive(Debug, Clone)]
pub struct DraftPlan {
    pub horizon_days: i32,
    pub analyzed_count: i32,
    pub days: Vec<ReviewPlanDay>,
    pub items: Vec<DraftItem>,
    pub prompt: String,
}

pub fn horizon_days(n: usize) -> u32 {
    if n == 0 {
        0
    } else if n <= 6 {
        3
    } else {
        7
    }
}

pub fn normalize_answer(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn answers_match(expected: &str, got: &str) -> bool {
    normalize_answer(expected) == normalize_answer(got)
}

pub fn cloze_prompt(source: &str, fragment: &str) -> String {
    if fragment.is_empty() {
        return source.to_string();
    }
    if let Some(pos) = source.find(fragment) {
        format!("{}____{}", &source[..pos], &source[pos + fragment.len()..])
    } else {
        source.to_string()
    }
}

fn severity_rank(severity: &str) -> i32 {
    match severity {
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

fn clip_bucket(kind: &str) -> String {
    format!("clip:{kind}")
}

pub fn bucket_title(key: &str) -> String {
    if let Some(kind) = key.strip_prefix("clip:") {
        return match kind {
            KIND_WORD => "单词".into(),
            KIND_PHRASE => "短语".into(),
            KIND_SENTENCE => "句子".into(),
            _ => "收藏".into(),
        };
    }
    error_type_label(key).into()
}

fn from_mistake(item: &Mistake) -> DraftItem {
    let bucket = item
        .error_type
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "other".into());
    DraftItem {
        source_kind: SOURCE_MISTAKE.into(),
        source_id: item.id,
        bucket_key: bucket,
        prompt_text: cloze_prompt(&item.source_text, &item.fragment),
        expected: item.correction.clone(),
        hint: item.explanation.clone(),
        fragment: Some(item.fragment.clone()),
        day_index: 1,
        sort_order: 0,
    }
}

fn from_clip(item: &Clip) -> DraftItem {
    let (expected, hint) = clips::bilingual_pair(&item.text, item.note.as_deref());
    let prompt_text = hint
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if expected.chars().any(|c| c.is_ascii_alphabetic()) {
                dictation_mask(&expected)
            } else {
                expected.clone()
            }
        });
    DraftItem {
        source_kind: SOURCE_CLIP.into(),
        source_id: item.id,
        bucket_key: clip_bucket(&item.kind),
        prompt_text,
        expected,
        hint,
        fragment: None,
        day_index: 1,
        sort_order: 0,
    }
}

fn dictation_mask(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .enumerate()
                .map(|(i, ch)| {
                    if i == 0 || !ch.is_ascii_alphabetic() {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn distribute(mut items: Vec<DraftItem>) -> DraftPlan {
    let n = items.len();
    let mut horizon = horizon_days(n) as usize;
    if horizon == 0 {
        return DraftPlan {
            horizon_days: 0,
            analyzed_count: 0,
            days: Vec::new(),
            items: Vec::new(),
            prompt: String::new(),
        };
    }
    let cap = horizon * MAX_PER_DAY;
    if items.len() > cap {
        items.truncate(cap);
    }
    let n = items.len();
    horizon = (horizon_days(n) as usize).min(n).max(1);
    let base = n / horizon;
    let extra = n % horizon;
    let mut sizes = Vec::with_capacity(horizon);
    for i in 0..horizon {
        let size = base + usize::from(i < extra);
        if size > 0 {
            sizes.push(size);
        }
    }
    let mut assigned = Vec::new();
    let mut cursor = 0;
    for (i, size) in sizes.iter().enumerate() {
        let day_index = (i + 1) as i32;
        for (sort, item) in items[cursor..cursor + size].iter().cloned().enumerate() {
            assigned.push(DraftItem {
                day_index,
                sort_order: sort as i32,
                ..item
            });
        }
        cursor += size;
    }
    let days: Vec<ReviewPlanDay> = sizes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let day_index = (i + 1) as i32;
            let keys = unique_buckets(&assigned, day_index);
            let title = keys
                .first()
                .map(|k| bucket_title(k))
                .unwrap_or_else(|| format!("第 {day_index} 轮"));
            ReviewPlanDay {
                day_index,
                title,
                error_types: keys,
                goals: Vec::new(),
                drill: None,
                status: DAY_PENDING.into(),
            }
        })
        .collect();
    DraftPlan {
        horizon_days: days.len() as i32,
        analyzed_count: assigned.len() as i32,
        days,
        items: assigned,
        prompt: String::new(),
    }
}

fn unique_buckets(items: &[DraftItem], day_index: i32) -> Vec<String> {
    let mut keys = Vec::new();
    for item in items.iter().filter(|item| item.day_index == day_index) {
        if !keys.iter().any(|k| k == &item.bucket_key) {
            keys.push(item.bucket_key.clone());
        }
    }
    keys
}

fn sort_pool(mistakes: &[Mistake], clips: &[Clip]) -> Vec<DraftItem> {
    let mut items: Vec<(i32, String, DraftItem)> = mistakes
        .iter()
        .map(|m| {
            let item = from_mistake(m);
            (-severity_rank(&m.severity), m.created_at.clone(), item)
        })
        .chain(clips.iter().map(|c| {
            let item = from_clip(c);
            (-2, c.created_at.clone(), item)
        }))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.bucket_key.cmp(&b.2.bucket_key)));
    // Cluster by bucket while keeping severity order loosely: group consecutive by key after sort.
    let mut clustered: Vec<DraftItem> = Vec::new();
    let mut remaining = items.into_iter().map(|(_, _, item)| item).collect::<Vec<_>>();
    while !remaining.is_empty() {
        let key = remaining[0].bucket_key.clone();
        let mut keep = Vec::new();
        for item in remaining.drain(..) {
            if item.bucket_key == key {
                clustered.push(item);
            } else {
                keep.push(item);
            }
        }
        remaining = keep;
    }
    clustered
}

pub fn build_draft(mistakes: &[Mistake], clips: &[Clip]) -> Result<DraftPlan, String> {
    if mistakes.is_empty() && clips.is_empty() {
        return Err("没有可复习的错题或收藏".into());
    }
    let mut draft = distribute(sort_pool(mistakes, clips));
    draft.prompt = build_prompt(&draft, mistakes, clips);
    Ok(draft)
}

fn build_prompt(draft: &DraftPlan, mistakes: &[Mistake], clips: &[Clip]) -> String {
    let mut out = format!(
        "请为正好 {} 轮复习写文案。错题 {} 条，收藏 {} 条，本方案收入 {} 题。\n\n轮次：\n",
        draft.horizon_days,
        mistakes.len(),
        clips.len(),
        draft.analyzed_count
    );
    for day in &draft.days {
        out.push_str(&format!(
            "- 第 {} 轮：{}\n",
            day.day_index,
            day.error_types
                .iter()
                .map(|k| format!("{}（{k}）", bucket_title(k)))
                .collect::<Vec<_>>()
                .join("、")
        ));
    }
    out.push_str("\n类型频次：\n");
    let mut counts: Vec<(String, usize)> = Vec::new();
    for item in &draft.items {
        if let Some((_, n)) = counts.iter_mut().find(|(k, _)| k == &item.bucket_key) {
            *n += 1;
        } else {
            counts.push((item.bucket_key.clone(), 1));
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (key, n) in &counts {
        out.push_str(&format!("- {}（{key}）：{n}\n", bucket_title(key)));
    }
    out.push_str("\n代表例句（每种最多 3 条，已截断）：\n");
    for (key, _) in &counts {
        out.push_str(&format!("### {}\n", bucket_title(key)));
        for (i, item) in draft
            .items
            .iter()
            .filter(|item| item.bucket_key == *key)
            .take(3)
            .enumerate()
        {
            let snippet: String = item.prompt_text.chars().take(160).collect();
            out.push_str(&format!(
                "{}. {}\n   期望：{}\n",
                i + 1,
                snippet,
                item.expected
            ));
        }
    }
    out
}

pub fn apply_llm_days(draft: &mut DraftPlan, days: &[LlmDay]) {
    if days.len() != draft.days.len() {
        return;
    }
    for (slot, copy) in draft.days.iter_mut().zip(days.iter()) {
        let title = copy.title.trim();
        if !title.is_empty() {
            slot.title = title.to_string();
        }
        if !copy.error_types.is_empty() {
            slot.error_types = copy.error_types.clone();
        }
        slot.goals = copy
            .goals
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let drill = copy.drill.as_deref().map(str::trim).filter(|s| !s.is_empty());
        slot.drill = drill.map(str::to_string);
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmDay {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub error_types: Vec<String>,
    #[serde(default)]
    pub goals: Vec<String>,
    #[serde(default)]
    pub drill: Option<String>,
}

pub fn persist(
    conn: &Connection,
    draft: &DraftPlan,
    summary: &str,
    engine: &str,
    duration_ms: u32,
    prompt_version: &str,
) -> Result<ReviewPlan, String> {
    conn.execute(
        "UPDATE review_plans SET status = ?1 WHERE status = ?2",
        params![PLAN_STATUS_ARCHIVED, PLAN_STATUS_ACTIVE],
    )
    .map_err(|e| e.to_string())?;
    let days_json = serde_json::to_string(&draft.days).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO review_plans (
            status, horizon_days, analyzed_count, summary, days_json, engine,
            duration_ms, prompt_version
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            PLAN_STATUS_ACTIVE,
            draft.horizon_days,
            draft.analyzed_count,
            summary,
            days_json,
            engine,
            duration_ms as i32,
            prompt_version,
        ],
    )
    .map_err(|e| e.to_string())?;
    let plan_id = conn.last_insert_rowid() as i32;
    for item in &draft.items {
        conn.execute(
            "INSERT INTO review_plan_items (
                plan_id, day_index, sort_order, source_kind, source_id,
                prompt_text, expected, hint, fragment
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                plan_id,
                item.day_index,
                item.sort_order,
                item.source_kind,
                item.source_id,
                item.prompt_text,
                item.expected,
                item.hint,
                item.fragment,
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    load_by_id(conn, plan_id)?.ok_or_else(|| "写入后无法读取方案".into())
}

fn map_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewPlanItem> {
    Ok(ReviewPlanItem {
        id: row.get(0)?,
        day_index: row.get(1)?,
        sort_order: row.get(2)?,
        source_kind: row.get(3)?,
        source_id: row.get(4)?,
        prompt_text: row.get(5)?,
        expected: row.get(6)?,
        hint: row.get(7)?,
        fragment: row.get(8)?,
        result: row.get(9)?,
        reviewed_at: row.get(10)?,
    })
}

fn load_items(conn: &Connection, plan_id: i32) -> Result<Vec<ReviewPlanItem>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, day_index, sort_order, source_kind, source_id, prompt_text,
                    expected, hint, fragment, result, reviewed_at
             FROM review_plan_items WHERE plan_id = ?1
             ORDER BY day_index ASC, sort_order ASC, id ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![plan_id], map_item)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

fn current_day(days: &[ReviewPlanDay], items: &[ReviewPlanItem]) -> i32 {
    for day in days {
        if day.status != DAY_DONE {
            let unfinished = items.iter().any(|item| {
                item.day_index == day.day_index
                    && item.result.as_deref() != Some(ITEM_MASTERED)
                    && item.result.as_deref() != Some(ITEM_SKIPPED)
                    && item.result.as_deref() != Some(ITEM_CORRECT)
                    && item.result.as_deref() != Some(ITEM_WRONG)
            });
            if unfinished || day.status == DAY_PENDING {
                return day.day_index;
            }
        }
    }
    days.last().map(|d| d.day_index).unwrap_or(1)
}

fn assemble(
    id: i32,
    created_at: String,
    status: String,
    horizon_days: i32,
    analyzed_count: i32,
    summary: String,
    days_json: String,
    engine: Option<String>,
    duration_ms: Option<i32>,
    prompt_version: Option<String>,
    items: Vec<ReviewPlanItem>,
) -> Result<ReviewPlan, String> {
    let days: Vec<ReviewPlanDay> =
        serde_json::from_str(&days_json).map_err(|e| e.to_string())?;
    let current_day = current_day(&days, &items);
    Ok(ReviewPlan {
        id,
        created_at,
        status,
        horizon_days,
        analyzed_count,
        summary,
        engine,
        duration_ms,
        prompt_version,
        current_day,
        days,
        items,
    })
}

pub fn load_by_id(conn: &Connection, id: i32) -> Result<Option<ReviewPlan>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, created_at, status, horizon_days, analyzed_count, summary,
                    days_json, engine, duration_ms, prompt_version
             FROM review_plans WHERE id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let row = stmt
        .query_row(params![id], |row| {
            Ok((
                row.get::<_, i32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, i32>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<i32>>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let items = load_items(conn, id)?;
    Ok(Some(assemble(
        row.0, row.1, row.2, row.3, row.4, row.5, row.6, row.7, row.8, row.9, items,
    )?))
}

pub fn load_active(conn: &Connection) -> Result<Option<ReviewPlan>, String> {
    let id: Option<i32> = conn
        .query_row(
            "SELECT id FROM review_plans WHERE status = ?1 ORDER BY id DESC LIMIT 1",
            params![PLAN_STATUS_ACTIVE],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match id {
        Some(id) => load_by_id(conn, id),
        None => Ok(None),
    }
}

fn valid_outcome(value: &str) -> bool {
    matches!(
        value,
        ITEM_CORRECT | ITEM_WRONG | ITEM_SKIPPED | ITEM_MASTERED
    )
}

pub fn grade(
    conn: &Connection,
    request: &ReviewGradeRequest,
) -> Result<ReviewGradeResult, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, day_index, sort_order, source_kind, source_id, prompt_text,
                    expected, hint, fragment, result, reviewed_at, plan_id
             FROM review_plan_items WHERE id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let (mut item, plan_id) = stmt
        .query_row(params![request.item_id], |row| {
            Ok((map_item(row)?, row.get::<_, i32>(11)?))
        })
        .map_err(|e| e.to_string())?;
    let matched = request
        .answer
        .as_deref()
        .map(|answer| answers_match(&item.expected, answer))
        .unwrap_or(false);
    let outcome = if request.mark_learned {
        ITEM_MASTERED
    } else if request.answer.is_some() {
        if matched {
            ITEM_CORRECT
        } else {
            ITEM_WRONG
        }
    } else if valid_outcome(&request.outcome) {
        request.outcome.as_str()
    } else {
        return Err(format!("无效结果：{}", request.outcome));
    };
    conn.execute(
        "UPDATE review_plan_items SET result = ?1, reviewed_at = datetime('now') WHERE id = ?2",
        params![outcome, request.item_id],
    )
    .map_err(|e| e.to_string())?;
    item.result = Some(outcome.to_string());
    if request.mark_learned {
        mark_learned(conn, &item.source_kind, item.source_id)?;
    }
    let plan = load_by_id(conn, plan_id)?.ok_or_else(|| "方案不存在".to_string())?;
    Ok(ReviewGradeResult {
        matched,
        item,
        plan,
    })
}

fn mark_learned(conn: &Connection, source_kind: &str, source_id: i32) -> Result<(), String> {
    if source_kind == SOURCE_MISTAKE {
        let _ = mistakes::update(
            conn,
            source_id,
            &mistakes::MistakePatch {
                status: Some("learned".into()),
                tags: None,
            },
        );
    } else if source_kind == SOURCE_CLIP {
        let _ = clips::update(
            conn,
            source_id,
            &clips::ClipPatch {
                status: Some("learned".into()),
                note: None,
            },
        );
    }
    Ok(())
}

pub fn complete_day(conn: &Connection, plan_id: i32, day_index: i32) -> Result<ReviewPlan, String> {
    let mut plan = load_by_id(conn, plan_id)?.ok_or_else(|| "方案不存在".to_string())?;
    for day in &mut plan.days {
        if day.day_index == day_index {
            day.status = DAY_DONE.into();
        }
    }
    let days_json = serde_json::to_string(&plan.days).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE review_plans SET days_json = ?1 WHERE id = ?2",
        params![days_json, plan_id],
    )
    .map_err(|e| e.to_string())?;
    load_by_id(conn, plan_id)?.ok_or_else(|| "方案不存在".into())
}

pub fn open_pool(conn: &Connection) -> Result<(Vec<Mistake>, Vec<Clip>), String> {
    let mistakes = mistakes::list_all(
        conn,
        &MistakeFilter {
            error_type: None,
            status: Some(STATUS_OPEN.into()),
            time_range: Some(MistakeTimeRange::All),
            limit: None,
            offset: None,
        },
    )?;
    let clips = clips::list_all(
        conn,
        &ClipFilter {
            status: Some(STATUS_OPEN.into()),
            kind: None,
            limit: None,
            offset: None,
        },
    )?;
    Ok((mistakes, clips))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database;
    use crate::services::mistakes::{NewMistake, STATUS_LEARNED};

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        database::migrate(&conn).unwrap();
        conn
    }

    fn mistake(id: i32, fragment: &str, error_type: &str, severity: &str) -> Mistake {
        Mistake {
            id,
            created_at: format!("2026-08-0{id} 01:00:00"),
            source_text: format!("He {fragment} to school."),
            fragment: fragment.into(),
            correction: "goes".into(),
            error_type: Some(error_type.into()),
            severity: severity.into(),
            explanation: Some("第三人称".into()),
            corrected_sentence: None,
            suggestions: vec![],
            engine: None,
            source_app: None,
            tags: None,
            status: STATUS_OPEN.into(),
            meta: None,
            server_id: None,
            synced_at: None,
        }
    }

    fn clip(id: i32, text: &str, kind: &str) -> Clip {
        Clip {
            id,
            created_at: format!("2026-08-1{id} 01:00:00"),
            text: text.into(),
            note: Some("备注".into()),
            kind: kind.into(),
            source_app: None,
            status: STATUS_OPEN.into(),
            meta: None,
            server_id: None,
            synced_at: None,
        }
    }

    #[test]
    fn horizon_is_three_or_seven() {
        assert_eq!(horizon_days(0), 0);
        assert_eq!(horizon_days(1), 3);
        assert_eq!(horizon_days(6), 3);
        assert_eq!(horizon_days(7), 7);
    }

    #[test]
    fn answers_ignore_case_and_spaces() {
        assert!(answers_match("On the other hand", "  on   the OTHER hand "));
        assert!(!answers_match("goes", "go"));
    }

    #[test]
    fn cloze_replaces_fragment() {
        assert_eq!(cloze_prompt("He go to school.", "go"), "He ____ to school.");
    }

    #[test]
    fn draft_does_not_mix_cloze_with_dictation() {
        let draft = build_draft(
            &[mistake(1, "go", "grammar", "high")],
            &[clip(2, "on the other hand", KIND_PHRASE)],
        )
        .unwrap();
        let mistake_item = draft
            .items
            .iter()
            .find(|item| item.source_kind == SOURCE_MISTAKE)
            .unwrap();
        let clip_item = draft
            .items
            .iter()
            .find(|item| item.source_kind == SOURCE_CLIP)
            .unwrap();
        assert!(mistake_item.prompt_text.contains("____"));
        assert_eq!(clip_item.expected, "on the other hand");
        assert_eq!(clip_item.prompt_text, "备注");
        assert_eq!(clip_item.hint.as_deref(), Some("备注"));
        assert!(!clip_item.prompt_text.contains("____"));
        assert!(draft.horizon_days >= 1);
        assert_eq!(draft.items.len(), 2);
    }

    #[test]
    fn clip_dictation_uses_chinese_or_letter_mask() {
        let with_note = super::from_clip(&clip(1, "simply is", KIND_PHRASE));
        assert_eq!(with_note.expected, "simply is");
        assert_eq!(with_note.prompt_text, "备注");
        assert_eq!(with_note.hint.as_deref(), Some("备注"));

        let mut bare = clip(2, "on the other hand", KIND_PHRASE);
        bare.note = None;
        let masked = super::from_clip(&bare);
        assert_eq!(masked.expected, "on the other hand");
        assert_eq!(masked.prompt_text, "o_ t__ o____ h___");
        assert!(masked.hint.is_none());
    }

    #[test]
    fn empty_pool_is_error() {
        assert!(build_draft(&[], &[]).is_err());
    }

    #[test]
    fn persist_and_grade_can_mark_learned() {
        let conn = db();
        let created = mistakes::create(
            &conn,
            &NewMistake {
                source_text: "He go.".into(),
                fragment: "go".into(),
                correction: "goes".into(),
                error_type: Some("grammar".into()),
                severity: Some("high".into()),
                explanation: Some("第三人称".into()),
                corrected_sentence: None,
                suggestions: vec![],
                engine: None,
                source_app: None,
                tags: None,
                status: Some(STATUS_OPEN.into()),
            },
            STATUS_OPEN,
        )
        .unwrap();
        let draft = build_draft(&[created.clone()], &[]).unwrap();
        let plan = persist(&conn, &draft, "主谓一致", "LLM", 12, "v1").unwrap();
        assert_eq!(plan.status, PLAN_STATUS_ACTIVE);
        assert_eq!(plan.items.len(), 1);
        let item_id = plan.items[0].id;
        grade(
            &conn,
            &ReviewGradeRequest {
                item_id,
                answer: Some("goes".into()),
                outcome: String::new(),
                mark_learned: true,
            },
        )
        .unwrap();
        let updated = mistakes::get(&conn, created.id).unwrap().unwrap();
        assert_eq!(updated.status, STATUS_LEARNED);
        let active = load_active(&conn).unwrap().unwrap();
        assert_eq!(active.items[0].result.as_deref(), Some(ITEM_MASTERED));
    }

    #[test]
    fn regenerate_archives_previous() {
        let conn = db();
        let m = mistake(1, "go", "grammar", "high");
        let first = persist(&conn, &build_draft(&[m.clone()], &[]).unwrap(), "a", "LLM", 1, "v1")
            .unwrap();
        let second = persist(&conn, &build_draft(&[m], &[]).unwrap(), "b", "LLM", 1, "v1")
            .unwrap();
        assert_ne!(first.id, second.id);
        let archived: String = conn
            .query_row(
                "SELECT status FROM review_plans WHERE id = ?1",
                params![first.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(archived, PLAN_STATUS_ARCHIVED);
        assert_eq!(load_active(&conn).unwrap().unwrap().id, second.id);
    }
}
