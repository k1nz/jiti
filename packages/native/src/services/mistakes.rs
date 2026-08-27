//! 错题本领域服务（§6.1 `mistakes` 表）。

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::providers::grammar::{GrammarError, GrammarErrorType, GrammarResult, GrammarSeverity};

pub const STATUS_OPEN: &str = "open";
pub const STATUS_LEARNED: &str = "learned";
pub const STATUS_ARCHIVED: &str = "archived";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MistakeTimeRange {
    SevenDays,
    ThirtyDays,
    All,
}

impl Default for MistakeTimeRange {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MistakePreferences {
    #[serde(default = "default_true")]
    pub auto_collect: bool,
    #[serde(default = "default_status")]
    pub default_status: String,
}

impl Default for MistakePreferences {
    fn default() -> Self {
        Self {
            auto_collect: true,
            default_status: STATUS_OPEN.into(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_status() -> String {
    STATUS_OPEN.into()
}

impl MistakePreferences {
    pub fn sanitized(self) -> Self {
        Self {
            auto_collect: self.auto_collect,
            default_status: if is_valid_status(&self.default_status) {
                self.default_status
            } else {
                STATUS_OPEN.into()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Mistake {
    pub id: i32,
    pub created_at: String,
    pub source_text: String,
    pub fragment: String,
    pub correction: String,
    pub error_type: Option<String>,
    pub severity: String,
    pub explanation: Option<String>,
    pub corrected_sentence: Option<String>,
    pub suggestions: Vec<String>,
    pub engine: Option<String>,
    pub source_app: Option<String>,
    pub tags: Option<String>,
    pub status: String,
    pub meta: Option<String>,
    pub server_id: Option<String>,
    pub synced_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewMistake {
    pub source_text: String,
    pub fragment: String,
    pub correction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrected_sentence: Option<String>,
    #[serde(default)]
    pub suggestions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl NewMistake {
    pub fn from_grammar_error(result: &GrammarResult, error: &GrammarError, status: &str) -> Self {
        Self {
            source_text: result.input.clone(),
            fragment: error.fragment.clone(),
            correction: error.correction.clone(),
            error_type: Some(error_type_key(error.error_type).into()),
            severity: Some(severity_key(error.severity).into()),
            explanation: Some(error.explanation.clone()),
            corrected_sentence: result.corrected_text.clone(),
            suggestions: error.suggestions.clone(),
            engine: Some(result.engine.clone()),
            source_app: None,
            tags: None,
            status: Some(status.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MistakeFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<MistakeTimeRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MistakePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MistakeList {
    pub items: Vec<Mistake>,
    pub total: i32,
}

const SELECT_COLUMNS: &str = "id, created_at, source_text, fragment, correction, error_type,
            severity, explanation, corrected_sentence, suggestions, engine, source_app,
            tags, status, meta, server_id, synced_at";

pub fn is_valid_status(status: &str) -> bool {
    matches!(status, STATUS_OPEN | STATUS_LEARNED | STATUS_ARCHIVED)
}

pub fn error_type_key(value: GrammarErrorType) -> &'static str {
    match value {
        GrammarErrorType::Grammar => "grammar",
        GrammarErrorType::Spelling => "spelling",
        GrammarErrorType::Punctuation => "punctuation",
        GrammarErrorType::WordChoice => "word_choice",
        GrammarErrorType::Style => "style",
    }
}

pub fn error_type_label(value: &str) -> &'static str {
    match value {
        "grammar" => "语法",
        "spelling" => "拼写",
        "punctuation" => "标点",
        "word_choice" => "用词",
        "style" => "风格",
        _ => "其他",
    }
}

pub fn severity_key(value: GrammarSeverity) -> &'static str {
    match value {
        GrammarSeverity::Low => "low",
        GrammarSeverity::Medium => "medium",
        GrammarSeverity::High => "high",
    }
}

pub fn status_label(status: &str) -> &'static str {
    match status {
        STATUS_LEARNED => "已掌握",
        STATUS_ARCHIVED => "已归档",
        _ => "未掌握",
    }
}

fn validate_new(item: &NewMistake) -> Result<(), String> {
    if item.source_text.trim().is_empty() {
        return Err("原句为空".into());
    }
    if item.fragment.trim().is_empty() {
        return Err("出错片段为空".into());
    }
    if item.correction.trim().is_empty() {
        return Err("建议修改为空".into());
    }
    if let Some(status) = &item.status {
        if !is_valid_status(status) {
            return Err(format!("无效状态：{status}"));
        }
    }
    Ok(())
}

fn suggestions_json(items: &[String]) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    serde_json::to_string(items).ok()
}

fn parse_suggestions(raw: Option<String>) -> Vec<String> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Mistake> {
    Ok(Mistake {
        id: row.get(0)?,
        created_at: row.get(1)?,
        source_text: row.get(2)?,
        fragment: row.get(3)?,
        correction: row.get(4)?,
        error_type: row.get(5)?,
        severity: row
            .get::<_, Option<String>>(6)?
            .unwrap_or_else(|| "medium".into()),
        explanation: row.get(7)?,
        corrected_sentence: row.get(8)?,
        suggestions: parse_suggestions(row.get(9)?),
        engine: row.get(10)?,
        source_app: row.get(11)?,
        tags: row.get(12)?,
        status: row
            .get::<_, Option<String>>(13)?
            .unwrap_or_else(|| STATUS_OPEN.into()),
        meta: row.get(14)?,
        server_id: row.get(15)?,
        synced_at: row.get(16)?,
    })
}

fn filter_parts(filter: &MistakeFilter) -> (String, Vec<String>) {
    let mut clauses = vec!["1=1".to_string()];
    let mut params = Vec::new();
    if let Some(error_type) = filter
        .error_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        clauses.push("error_type = ?".into());
        params.push(error_type.to_string());
    }
    if let Some(status) = filter
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        clauses.push("status = ?".into());
        params.push(status.to_string());
    }
    match filter.time_range.unwrap_or(MistakeTimeRange::All) {
        MistakeTimeRange::SevenDays => {
            clauses.push("created_at >= datetime('now', '-7 days')".into());
        }
        MistakeTimeRange::ThirtyDays => {
            clauses.push("created_at >= datetime('now', '-30 days')".into());
        }
        MistakeTimeRange::All => {}
    }
    (clauses.join(" AND "), params)
}

fn insert_one(conn: &Connection, item: &NewMistake, default_status: &str) -> Result<i32, String> {
    validate_new(item)?;
    let status = item
        .status
        .clone()
        .filter(|s| is_valid_status(s))
        .unwrap_or_else(|| {
            if is_valid_status(default_status) {
                default_status.to_string()
            } else {
                STATUS_OPEN.into()
            }
        });
    let severity = item
        .severity
        .clone()
        .filter(|s| matches!(s.as_str(), "low" | "medium" | "high"))
        .unwrap_or_else(|| "medium".into());
    conn.execute(
        "INSERT INTO mistakes (
            source_text, fragment, correction, error_type, severity, explanation,
            corrected_sentence, suggestions, engine, source_app, tags, status
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            item.source_text,
            item.fragment,
            item.correction,
            item.error_type,
            severity,
            item.explanation,
            item.corrected_sentence,
            suggestions_json(&item.suggestions),
            item.engine,
            item.source_app,
            item.tags,
            status,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid() as i32)
}

pub fn create(
    conn: &Connection,
    item: &NewMistake,
    default_status: &str,
) -> Result<Mistake, String> {
    let id = insert_one(conn, item, default_status)?;
    get(conn, id)?.ok_or_else(|| "写入后无法读取错题".into())
}

/// 事务化批量创建：任一失败则整批回滚，不留下半成品。
pub fn create_batch(
    conn: &mut Connection,
    items: &[NewMistake],
    default_status: &str,
) -> Result<Vec<i32>, String> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        match insert_one(&tx, item, default_status) {
            Ok(id) => ids.push(id),
            Err(err) => {
                drop(tx);
                return Err(err);
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ids)
}

pub fn collect_from_grammar(
    conn: &mut Connection,
    result: &GrammarResult,
    prefs: &MistakePreferences,
) -> Result<Vec<Option<i32>>, String> {
    if result.errors.is_empty() {
        return Ok(Vec::new());
    }
    if !prefs.auto_collect {
        return Ok(vec![None; result.errors.len()]);
    }
    let status = if is_valid_status(&prefs.default_status) {
        prefs.default_status.as_str()
    } else {
        STATUS_OPEN
    };
    let items: Vec<NewMistake> = result
        .errors
        .iter()
        .map(|error| NewMistake::from_grammar_error(result, error, status))
        .collect();
    let ids = create_batch(conn, &items, status)?;
    Ok(ids.into_iter().map(Some).collect())
}

pub fn get(conn: &Connection, id: i32) -> Result<Option<Mistake>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM mistakes WHERE id = ?1"
        ))
        .map_err(|e| e.to_string())?;
    stmt.query_row(params![id], map_row)
        .optional()
        .map_err(|e| e.to_string())
}

pub fn list(conn: &Connection, filter: &MistakeFilter) -> Result<MistakeList, String> {
    let (where_sql, bound) = filter_parts(filter);
    let total: i32 = {
        let sql = format!("SELECT COUNT(*) FROM mistakes WHERE {where_sql}");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        stmt.query_row(params_from_iter(bound.iter()), |row| row.get(0))
            .map_err(|e| e.to_string())?
    };
    let mut sql =
        format!("SELECT {SELECT_COLUMNS} FROM mistakes WHERE {where_sql} ORDER BY id DESC");
    let mut query_params = bound;
    if let Some(limit) = filter.limit.filter(|n| *n > 0) {
        sql.push_str(" LIMIT ?");
        query_params.push(limit.to_string());
        let offset = filter.offset.unwrap_or(0).max(0);
        sql.push_str(" OFFSET ?");
        query_params.push(offset.to_string());
    }
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(query_params.iter()), map_row)
        .map_err(|e| e.to_string())?;
    let items = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(MistakeList { items, total })
}

/// 导出 / AI 复习用：忽略分页，取当前筛选全集。
pub fn list_all(conn: &Connection, filter: &MistakeFilter) -> Result<Vec<Mistake>, String> {
    let unbounded = MistakeFilter {
        limit: None,
        offset: None,
        ..filter.clone()
    };
    Ok(list(conn, &unbounded)?.items)
}

pub fn update(conn: &Connection, id: i32, patch: &MistakePatch) -> Result<Mistake, String> {
    if patch.status.is_none() && patch.tags.is_none() {
        return get(conn, id)?.ok_or_else(|| "错题不存在".into());
    }
    if let Some(status) = &patch.status {
        if !is_valid_status(status) {
            return Err(format!("无效状态：{status}"));
        }
        conn.execute(
            "UPDATE mistakes SET status = ?1 WHERE id = ?2",
            params![status, id],
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(tags) = &patch.tags {
        conn.execute(
            "UPDATE mistakes SET tags = ?1 WHERE id = ?2",
            params![tags, id],
        )
        .map_err(|e| e.to_string())?;
    }
    get(conn, id)?.ok_or_else(|| "错题不存在".into())
}

pub fn delete(conn: &Connection, id: i32) -> Result<i32, String> {
    conn.execute("DELETE FROM mistakes WHERE id = ?1", params![id])
        .map(|n| n as i32)
        .map_err(|e| e.to_string())
}

pub fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.'
            | '!' | '|' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("  \n"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn render_markdown(items: &[Mistake]) -> String {
    let mut groups: Vec<(String, Vec<&Mistake>)> = Vec::new();
    for item in items {
        let key = item
            .error_type
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "other".into());
        if let Some((_, bucket)) = groups.iter_mut().find(|(k, _)| k == &key) {
            bucket.push(item);
        } else {
            groups.push((key, vec![item]));
        }
    }

    let mut out = String::from("# Jiti 错题本\n\n");
    if items.is_empty() {
        out.push_str("（当前筛选没有错题）\n");
        return out;
    }
    out.push_str(&format!("共 {} 条。\n", items.len()));
    for (key, bucket) in groups {
        out.push_str(&format!("\n## {}\n", error_type_label(&key)));
        for item in bucket {
            out.push_str(&format!("\n### {}\n\n", escape_markdown(&item.source_text)));
            out.push_str(&format!(
                "- 片段 → 修改：`{}` → `{}`\n",
                item.fragment.replace('`', "'"),
                item.correction.replace('`', "'"),
            ));
            if let Some(explain) = &item.explanation {
                if !explain.is_empty() {
                    out.push_str(&format!("- 讲解：{}\n", escape_markdown(explain)));
                }
            }
            out.push_str(&format!(
                "- 状态：{} · {}\n",
                status_label(&item.status),
                item.created_at
            ));
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewAggregate {
    pub total: usize,
    pub prompt: String,
}

/// 只聚合类型频次和代表例句，避免把整库原文发给模型。
pub fn aggregate_for_review(items: &[Mistake], per_type: usize) -> ReviewAggregate {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for item in items {
        let key = item
            .error_type
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "other".into());
        if let Some((_, n)) = counts.iter_mut().find(|(k, _)| k == &key) {
            *n += 1;
        } else {
            counts.push((key, 1));
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut prompt = format!("错题总数：{}\n\n类型频次：\n", items.len());
    for (key, n) in &counts {
        prompt.push_str(&format!("- {}（{}）：{n}\n", error_type_label(key), key));
    }
    prompt.push_str("\n代表例句（每种最多几条）：\n");
    for (key, _) in &counts {
        prompt.push_str(&format!("### {}\n", error_type_label(key)));
        let examples = items
            .iter()
            .filter(|item| {
                item.error_type
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("other")
                    == key
            })
            .take(per_type);
        for (i, item) in examples.enumerate() {
            let snippet = truncate_chars(&item.source_text, 160);
            prompt.push_str(&format!(
                "{}. 原句：{}\n   片段→修改：{} → {}\n   讲解：{}\n",
                i + 1,
                snippet,
                item.fragment,
                item.correction,
                item.explanation.as_deref().unwrap_or(""),
            ));
        }
    }
    ReviewAggregate {
        total: items.len(),
        prompt,
    }
}

fn truncate_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database;

    fn sample_error(fragment: &str, kind: GrammarErrorType) -> GrammarError {
        GrammarError {
            offset: None,
            length: None,
            fragment: fragment.into(),
            correction: format!("{fragment}s"),
            error_type: kind,
            severity: GrammarSeverity::High,
            explanation: "讲解".into(),
            suggestions: vec!["alt".into()],
        }
    }

    fn sample_result(errors: Vec<GrammarError>) -> GrammarResult {
        GrammarResult {
            engine: "LLM".into(),
            input: "He go to school.".into(),
            errors,
            corrected_text: Some("He goes to school.".into()),
            overall: Some("主谓不一致".into()),
            duration_ms: 12,
        }
    }

    fn seed(conn: &mut Connection, n: usize) -> Vec<i32> {
        let items: Vec<NewMistake> = (0..n)
            .map(|i| NewMistake {
                source_text: format!("sentence {i}"),
                fragment: format!("frag{i}"),
                correction: format!("fix{i}"),
                error_type: Some(if i % 2 == 0 { "grammar" } else { "spelling" }.into()),
                severity: Some(if i == 0 { "high" } else { "low" }.into()),
                explanation: Some(format!("explain {i}")),
                corrected_sentence: Some(format!("fixed {i}")),
                suggestions: vec!["a".into()],
                engine: Some("LLM".into()),
                source_app: None,
                tags: None,
                status: Some(if i == 1 {
                    STATUS_LEARNED.into()
                } else if i == 2 {
                    STATUS_ARCHIVED.into()
                } else {
                    STATUS_OPEN.into()
                }),
            })
            .collect();
        create_batch(conn, &items, STATUS_OPEN).unwrap()
    }

    #[test]
    fn crud_roundtrip_update_and_delete() {
        let conn = Connection::open_in_memory().unwrap();
        database::migrate(&conn).unwrap();
        let created = create(
            &conn,
            &NewMistake {
                source_text: "He go.".into(),
                fragment: "go".into(),
                correction: "goes".into(),
                error_type: Some("grammar".into()),
                severity: Some("high".into()),
                explanation: Some("第三人称".into()),
                corrected_sentence: Some("He goes.".into()),
                suggestions: vec!["goes".into()],
                engine: Some("LLM".into()),
                source_app: None,
                tags: Some("week1".into()),
                status: None,
            },
            STATUS_OPEN,
        )
        .unwrap();
        assert_eq!(created.status, STATUS_OPEN);
        assert_eq!(created.suggestions, vec!["goes".to_string()]);
        let learned = update(
            &conn,
            created.id,
            &MistakePatch {
                status: Some(STATUS_LEARNED.into()),
                tags: Some("done".into()),
            },
        )
        .unwrap();
        assert_eq!(learned.status, STATUS_LEARNED);
        assert_eq!(learned.tags.as_deref(), Some("done"));
        assert_eq!(delete(&conn, created.id).unwrap(), 1);
        assert!(get(&conn, created.id).unwrap().is_none());
    }

    #[test]
    fn combined_filters_and_pagination() {
        let mut conn = Connection::open_in_memory().unwrap();
        database::migrate(&conn).unwrap();
        seed(&mut conn, 5);
        let grammar_open = list(
            &conn,
            &MistakeFilter {
                error_type: Some("grammar".into()),
                status: Some(STATUS_OPEN.into()),
                time_range: Some(MistakeTimeRange::All),
                limit: None,
                offset: None,
            },
        )
        .unwrap();
        assert!(grammar_open.total >= 1);
        assert!(grammar_open
            .items
            .iter()
            .all(|m| m.error_type.as_deref() == Some("grammar") && m.status == STATUS_OPEN));

        let page = list(
            &conn,
            &MistakeFilter {
                error_type: None,
                status: None,
                time_range: Some(MistakeTimeRange::SevenDays),
                limit: Some(2),
                offset: Some(0),
            },
        )
        .unwrap();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total, 5);
    }

    #[test]
    fn batch_is_transactional() {
        let mut conn = Connection::open_in_memory().unwrap();
        database::migrate(&conn).unwrap();
        let ok = NewMistake {
            source_text: "He go.".into(),
            fragment: "go".into(),
            correction: "goes".into(),
            error_type: Some("grammar".into()),
            severity: None,
            explanation: None,
            corrected_sentence: None,
            suggestions: vec![],
            engine: None,
            source_app: None,
            tags: None,
            status: None,
        };
        let bad = NewMistake {
            fragment: String::new(),
            ..ok.clone()
        };
        let err = create_batch(&mut conn, &[ok, bad], STATUS_OPEN).unwrap_err();
        assert!(err.contains("片段"));
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM mistakes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn auto_collect_respects_switch_and_empty_errors() {
        let mut conn = Connection::open_in_memory().unwrap();
        database::migrate(&conn).unwrap();
        let result = sample_result(vec![sample_error("go", GrammarErrorType::Grammar)]);
        let off = collect_from_grammar(
            &mut conn,
            &result,
            &MistakePreferences {
                auto_collect: false,
                default_status: STATUS_OPEN.into(),
            },
        )
        .unwrap();
        assert_eq!(off, vec![None]);
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM mistakes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);

        let ids = collect_from_grammar(&mut conn, &result, &MistakePreferences::default()).unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids[0].is_some());
        let stored = get(&conn, ids[0].unwrap()).unwrap().unwrap();
        assert_eq!(stored.fragment, "go");
        assert_eq!(stored.engine.as_deref(), Some("LLM"));
        assert_eq!(
            stored.corrected_sentence.as_deref(),
            Some("He goes to school.")
        );

        let empty = collect_from_grammar(
            &mut conn,
            &sample_result(vec![]),
            &MistakePreferences::default(),
        )
        .unwrap();
        assert!(empty.is_empty());
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM mistakes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn markdown_groups_and_escapes() {
        let items = vec![
            Mistake {
                id: 1,
                created_at: "2026-08-27 01:00:00".into(),
                source_text: "See *this* #tag".into(),
                fragment: "this".into(),
                correction: "that".into(),
                error_type: Some("grammar".into()),
                severity: "high".into(),
                explanation: Some("星号 * 要转义".into()),
                corrected_sentence: None,
                suggestions: vec![],
                engine: None,
                source_app: None,
                tags: None,
                status: STATUS_OPEN.into(),
                meta: None,
                server_id: None,
                synced_at: None,
            },
            Mistake {
                id: 2,
                created_at: "2026-08-27 01:01:00".into(),
                source_text: "recieve".into(),
                fragment: "recieve".into(),
                correction: "receive".into(),
                error_type: Some("spelling".into()),
                severity: "medium".into(),
                explanation: Some("ie/ei".into()),
                corrected_sentence: None,
                suggestions: vec![],
                engine: None,
                source_app: None,
                tags: None,
                status: STATUS_LEARNED.into(),
                meta: None,
                server_id: None,
                synced_at: None,
            },
        ];
        let md = render_markdown(&items);
        assert!(md.contains("## 语法"));
        assert!(md.contains("## 拼写"));
        assert!(md.contains("\\*this\\*"));
        assert!(md.contains("\\#tag"));
        assert!(md.contains("已掌握"));
        assert!(md.contains("`this` → `that`"));
    }

    #[test]
    fn review_prompt_aggregates_counts_and_caps_examples() {
        let mut items = Vec::new();
        for i in 0..5 {
            items.push(Mistake {
                id: i,
                created_at: "now".into(),
                source_text: format!("long sentence number {i} with extra words"),
                fragment: "x".into(),
                correction: "y".into(),
                error_type: Some(if i < 3 { "grammar" } else { "spelling" }.into()),
                severity: "low".into(),
                explanation: Some("e".into()),
                corrected_sentence: None,
                suggestions: vec![],
                engine: None,
                source_app: None,
                tags: None,
                status: STATUS_OPEN.into(),
                meta: None,
                server_id: None,
                synced_at: None,
            });
        }
        let agg = aggregate_for_review(&items, 2);
        assert_eq!(agg.total, 5);
        assert!(agg.prompt.contains("grammar）：3"));
        assert!(agg.prompt.contains("spelling）：2"));
        assert_eq!(agg.prompt.matches("原句：").count(), 4);
    }

    #[test]
    fn invalid_status_is_rejected() {
        let conn = Connection::open_in_memory().unwrap();
        database::migrate(&conn).unwrap();
        let err = create(
            &conn,
            &NewMistake {
                source_text: "a".into(),
                fragment: "a".into(),
                correction: "b".into(),
                error_type: None,
                severity: None,
                explanation: None,
                corrected_sentence: None,
                suggestions: vec![],
                engine: None,
                source_app: None,
                tags: None,
                status: Some("done".into()),
            },
            STATUS_OPEN,
        )
        .unwrap_err();
        assert!(err.contains("无效状态"));
    }
}
