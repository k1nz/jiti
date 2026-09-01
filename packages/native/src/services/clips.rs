//! 收藏（播客/阅读选中的短语与句子），与错题分表。

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use super::mistakes::{is_valid_status, STATUS_OPEN};
use super::{database, panel, selection};

pub const KIND_WORD: &str = "word";
pub const KIND_PHRASE: &str = "phrase";
pub const KIND_SENTENCE: &str = "sentence";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: i32,
    pub created_at: String,
    pub text: String,
    pub note: Option<String>,
    pub kind: String,
    pub source_app: Option<String>,
    pub status: String,
    pub meta: Option<String>,
    pub server_id: Option<String>,
    pub synced_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewClip {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipList {
    pub items: Vec<Clip>,
    pub total: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClipSaveResult {
    pub clip: Clip,
    pub created: bool,
}

/// 收藏热键结果：面板在可见时用它提示；成功静默时前端也可以忽略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "clip://saved")]
pub struct ClipSavedEvent {
    pub status: String,
    pub text: Option<String>,
    pub created: bool,
}

const SELECT_COLUMNS: &str =
    "id, created_at, text, note, kind, source_app, status, meta, server_id, synced_at";

pub fn normalize_text(text: &str) -> String {
    collapse_ws(text).to_lowercase()
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn has_cjk(text: &str) -> bool {
    text.chars()
        .any(|c| ('\u{3400}'..='\u{4DBF}').contains(&c) || ('\u{4E00}'..='\u{9FFF}').contains(&c))
}

fn has_latin(text: &str) -> bool {
    text.chars().any(|c| c.is_ascii_alphabetic())
}

/// 英语进 `text`（默写对象），中文释义进 `note`。写反会互换。
pub fn bilingual_pair(text: &str, note: Option<&str>) -> (String, Option<String>) {
    let text = collapse_ws(text);
    let note = note
        .map(collapse_ws)
        .filter(|s| !s.is_empty() && normalize_text(s) != normalize_text(&text));
    match note {
        Some(note) => {
            let text_zh = has_cjk(&text);
            let text_en = has_latin(&text);
            let note_zh = has_cjk(&note);
            let note_en = has_latin(&note);
            if text_zh && !text_en && note_en {
                (note, Some(text))
            } else if text_en && note_zh {
                (text, Some(note))
            } else if text_zh && note_en {
                (note, Some(text))
            } else {
                (text, Some(note))
            }
        }
        None => (text, None),
    }
}

fn pick_english(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .copied()
        .find(|s| !s.is_empty() && has_latin(s) && !has_cjk(s))
        .or_else(|| {
            candidates
                .iter()
                .copied()
                .find(|s| !s.is_empty() && has_latin(s))
        })
        .map(str::to_string)
}

fn pick_chinese(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .copied()
        .find(|s| !s.is_empty() && has_cjk(s))
        .map(str::to_string)
}

fn merge_pair(
    a_text: &str,
    a_note: Option<&str>,
    b_text: &str,
    b_note: Option<&str>,
) -> (String, Option<String>) {
    let a_note = a_note.unwrap_or("");
    let b_note = b_note.unwrap_or("");
    let en = pick_english(&[a_text, b_text, a_note, b_note]);
    let zh = pick_chinese(&[b_note, a_note, a_text, b_text]);
    match (en, zh) {
        (Some(en), Some(zh)) if normalize_text(&en) != normalize_text(&zh) => (en, Some(zh)),
        (Some(en), _) => (en, None),
        (None, Some(zh)) => (zh, None),
        _ => bilingual_pair(b_text, if b_note.is_empty() { None } else { Some(b_note) }),
    }
}

pub fn infer_kind(text: &str) -> &'static str {
    let words = text.split_whitespace().count();
    if words <= 1 {
        KIND_WORD
    } else if words <= 6 {
        KIND_PHRASE
    } else {
        KIND_SENTENCE
    }
}

fn is_valid_kind(kind: &str) -> bool {
    matches!(kind, KIND_WORD | KIND_PHRASE | KIND_SENTENCE)
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Clip> {
    Ok(Clip {
        id: row.get(0)?,
        created_at: row.get(1)?,
        text: row.get(2)?,
        note: row.get(3)?,
        kind: row
            .get::<_, Option<String>>(4)?
            .filter(|s| is_valid_kind(s))
            .unwrap_or_else(|| KIND_PHRASE.into()),
        source_app: row.get(5)?,
        status: row
            .get::<_, Option<String>>(6)?
            .unwrap_or_else(|| STATUS_OPEN.into()),
        meta: row.get(7)?,
        server_id: row.get(8)?,
        synced_at: row.get(9)?,
    })
}

pub fn get(conn: &Connection, id: i32) -> Result<Option<Clip>, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {SELECT_COLUMNS} FROM clips WHERE id = ?1"))
        .map_err(|e| e.to_string())?;
    stmt.query_row(params![id], map_row)
        .optional()
        .map_err(|e| e.to_string())
}

fn get_by_norm(conn: &Connection, text_norm: &str) -> Result<Option<Clip>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM clips WHERE text_norm = ?1"
        ))
        .map_err(|e| e.to_string())?;
    stmt.query_row(params![text_norm], map_row)
        .optional()
        .map_err(|e| e.to_string())
}

fn find_existing(
    conn: &Connection,
    text: &str,
    note: Option<&str>,
) -> Result<Option<Clip>, String> {
    let mut norms = Vec::new();
    let text_n = normalize_text(text);
    if !text_n.is_empty() {
        norms.push(text_n);
    }
    if let Some(note) = note {
        let note_n = normalize_text(note);
        if !note_n.is_empty() && !norms.contains(&note_n) {
            norms.push(note_n);
        }
    }
    for n in &norms {
        if let Some(clip) = get_by_norm(conn, n)? {
            return Ok(Some(clip));
        }
    }
    if norms.is_empty() {
        return Ok(None);
    }
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM clips WHERE note IS NOT NULL AND trim(note) != ''"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], map_row)
        .map_err(|e| e.to_string())?;
    for row in rows {
        let clip = row.map_err(|e| e.to_string())?;
        if let Some(stored) = clip.note.as_deref() {
            if norms.iter().any(|n| *n == normalize_text(stored)) {
                return Ok(Some(clip));
            }
        }
    }
    Ok(None)
}

fn persist_pair(
    conn: &Connection,
    id: i32,
    text: &str,
    note: Option<&str>,
) -> Result<Clip, String> {
    let text_norm = normalize_text(text);
    if let Some(other) = get_by_norm(conn, &text_norm)? {
        if other.id != id {
            let (_, merged_note) = merge_pair(&other.text, other.note.as_deref(), text, note);
            if other.note.as_deref() != merged_note.as_deref() {
                conn.execute(
                    "UPDATE clips SET note = ?1 WHERE id = ?2",
                    params![merged_note, other.id],
                )
                .map_err(|e| e.to_string())?;
            }
            return get(conn, other.id)?.ok_or_else(|| "写入后无法读取收藏".into());
        }
    }
    let kind = infer_kind(text);
    conn.execute(
        "UPDATE clips SET text = ?1, text_norm = ?2, note = ?3, kind = ?4 WHERE id = ?5",
        params![text, text_norm, note, kind, id],
    )
    .map_err(|e| e.to_string())?;
    get(conn, id)?.ok_or_else(|| "写入后无法读取收藏".into())
}

pub fn save(conn: &Connection, item: &NewClip) -> Result<ClipSaveResult, String> {
    let (text, note) = bilingual_pair(&item.text, item.note.as_deref());
    if text.is_empty() {
        return Err("收藏内容为空".into());
    }
    if let Some(existing) = find_existing(conn, &text, note.as_deref())? {
        let (merged_text, merged_note) =
            merge_pair(&existing.text, existing.note.as_deref(), &text, note.as_deref());
        let clip = if merged_text != existing.text || merged_note != existing.note {
            persist_pair(conn, existing.id, &merged_text, merged_note.as_deref())?
        } else {
            existing
        };
        return Ok(ClipSaveResult {
            clip,
            created: false,
        });
    }
    let kind = item
        .kind
        .as_deref()
        .filter(|s| is_valid_kind(s))
        .unwrap_or_else(|| infer_kind(&text));
    let text_norm = normalize_text(&text);
    conn.execute(
        "INSERT INTO clips (text, text_norm, note, kind, source_app, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![text, text_norm, note, kind, item.source_app, STATUS_OPEN],
    )
    .map_err(|e| e.to_string())?;
    let id = conn.last_insert_rowid() as i32;
    let clip = get(conn, id)?.ok_or_else(|| "写入后无法读取收藏".to_string())?;
    Ok(ClipSaveResult {
        clip,
        created: true,
    })
}

fn filter_parts(filter: &ClipFilter) -> (String, Vec<String>) {
    let mut clauses = vec!["1=1".to_string()];
    let mut params = Vec::new();
    if let Some(status) = filter
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        clauses.push("status = ?".into());
        params.push(status.to_string());
    }
    if let Some(kind) = filter
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        clauses.push("kind = ?".into());
        params.push(kind.to_string());
    }
    (clauses.join(" AND "), params)
}

pub fn list(conn: &Connection, filter: &ClipFilter) -> Result<ClipList, String> {
    let (where_sql, bound) = filter_parts(filter);
    let total: i32 = {
        let sql = format!("SELECT COUNT(*) FROM clips WHERE {where_sql}");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        stmt.query_row(params_from_iter(bound.iter()), |row| row.get(0))
            .map_err(|e| e.to_string())?
    };
    let mut sql = format!(
        "SELECT {SELECT_COLUMNS} FROM clips WHERE {where_sql} ORDER BY id DESC"
    );
    let mut query_params: Vec<String> = bound;
    if let Some(limit) = filter.limit.filter(|n| *n > 0) {
        sql.push_str(" LIMIT ?");
        query_params.push(limit.to_string());
        if let Some(offset) = filter.offset.filter(|n| *n >= 0) {
            sql.push_str(" OFFSET ?");
            query_params.push(offset.to_string());
        }
    }
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(query_params.iter()), map_row)
        .map_err(|e| e.to_string())?;
    let items = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(ClipList { items, total })
}

pub fn list_all(conn: &Connection, filter: &ClipFilter) -> Result<Vec<Clip>, String> {
    let unbounded = ClipFilter {
        limit: None,
        offset: None,
        ..filter.clone()
    };
    Ok(list(conn, &unbounded)?.items)
}

pub fn update(conn: &Connection, id: i32, patch: &ClipPatch) -> Result<Clip, String> {
    if patch.status.is_none() && patch.note.is_none() {
        return get(conn, id)?.ok_or_else(|| "收藏不存在".into());
    }
    if let Some(status) = &patch.status {
        if !is_valid_status(status) {
            return Err(format!("无效状态：{status}"));
        }
        conn.execute(
            "UPDATE clips SET status = ?1 WHERE id = ?2",
            params![status, id],
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(note) = &patch.note {
        let note = note.trim();
        let stored = if note.is_empty() { None } else { Some(note) };
        conn.execute(
            "UPDATE clips SET note = ?1 WHERE id = ?2",
            params![stored, id],
        )
        .map_err(|e| e.to_string())?;
    }
    get(conn, id)?.ok_or_else(|| "收藏不存在".into())
}

pub fn delete(conn: &Connection, id: i32) -> Result<i32, String> {
    conn.execute("DELETE FROM clips WHERE id = ?1", params![id])
        .map(|n| n as i32)
        .map_err(|e| e.to_string())
}

/// 收藏热键：读选区并入库。有内容则不唤起面板；空选区才 show 面板提示。
pub fn save_from_hotkey(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("jiti-save-clip".into())
        .spawn(move || {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let inner = app.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = tx.send(selection::read_selected_text());
            });
            let selection = rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap_or_else(|_| selection::SelectedText::empty());
            if selection.is_blank() {
                panel::show_panel(&inner, panel::Mode::Panel);
                let _ = ClipSavedEvent {
                    status: "empty".into(),
                    text: None,
                    created: false,
                }
                .emit(&inner);
                return;
            }
            let outcome = (|| {
                let conn = database::open(&inner)?;
                save(
                    &conn,
                    &NewClip {
                        text: selection.text.clone(),
                        note: None,
                        kind: None,
                        source_app: None,
                    },
                )
            })();
            match outcome {
                Ok(result) => {
                    let _ = ClipSavedEvent {
                        status: if result.created {
                            "saved".into()
                        } else {
                            "duplicate".into()
                        },
                        text: Some(result.clip.text),
                        created: result.created,
                    }
                    .emit(&inner);
                }
                Err(err) => {
                    eprintln!("[jiti] save clip: {err}");
                    panel::show_panel(&inner, panel::Mode::Panel);
                    let _ = ClipSavedEvent {
                        status: "error".into(),
                        text: Some(err),
                        created: false,
                    }
                    .emit(&inner);
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database;
    use crate::services::mistakes::STATUS_LEARNED;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        database::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn normalize_collapses_case_and_space() {
        assert_eq!(normalize_text("  On  The  Other  Hand  "), "on the other hand");
        assert_eq!(infer_kind("Epoch"), KIND_WORD);
        assert_eq!(infer_kind("on the other hand"), KIND_PHRASE);
        assert_eq!(
            infer_kind("I ended up going to the store after all."),
            KIND_SENTENCE
        );
    }

    #[test]
    fn save_dedupes_normalized_text() {
        let db = conn();
        let first = save(
            &db,
            &NewClip {
                text: "On the other hand".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(first.created);
        assert_eq!(first.clip.kind, KIND_PHRASE);
        let again = save(
            &db,
            &NewClip {
                text: "  on   the other HAND ".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!again.created);
        assert_eq!(again.clip.id, first.clip.id);
        let list = list(&db, &ClipFilter::default()).unwrap();
        assert_eq!(list.total, 1);
    }

    #[test]
    fn empty_text_is_rejected() {
        let db = conn();
        let err = save(
            &db,
            &NewClip {
                text: "   \n".into(),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(err.contains("空"));
    }

    #[test]
    fn update_status_and_note() {
        let db = conn();
        let saved = save(
            &db,
            &NewClip {
                text: "kick the bucket".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let updated = update(
            &db,
            saved.clip.id,
            &ClipPatch {
                status: Some(STATUS_LEARNED.into()),
                note: Some("  一命呜呼  ".into()),
            },
        )
        .unwrap();
        assert_eq!(updated.status, STATUS_LEARNED);
        assert_eq!(updated.note.as_deref(), Some("一命呜呼"));
    }

    #[test]
    fn save_keeps_english_as_text_and_chinese_as_note() {
        let db = conn();
        let saved = save(
            &db,
            &NewClip {
                text: "简直是".into(),
                note: Some("simply is".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(saved.created);
        assert_eq!(saved.clip.text, "simply is");
        assert_eq!(saved.clip.note.as_deref(), Some("简直是"));
        assert_eq!(saved.clip.kind, KIND_PHRASE);
    }

    #[test]
    fn save_upgrades_chinese_only_row_when_english_arrives() {
        let db = conn();
        let first = save(
            &db,
            &NewClip {
                text: "简直是".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(first.clip.text, "简直是");
        let again = save(
            &db,
            &NewClip {
                text: "simply is".into(),
                note: Some("简直是".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!again.created);
        assert_eq!(again.clip.id, first.clip.id);
        assert_eq!(again.clip.text, "simply is");
        assert_eq!(again.clip.note.as_deref(), Some("简直是"));
        assert_eq!(again.clip.kind, KIND_PHRASE);
        assert_eq!(list(&db, &ClipFilter::default()).unwrap().total, 1);
    }

    #[test]
    fn save_fills_note_on_english_duplicate() {
        let db = conn();
        let first = save(
            &db,
            &NewClip {
                text: "simply is".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let again = save(
            &db,
            &NewClip {
                text: "Simply  is".into(),
                note: Some("简直是".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!again.created);
        assert_eq!(again.clip.id, first.clip.id);
        assert_eq!(again.clip.text, "simply is");
        assert_eq!(again.clip.note.as_deref(), Some("简直是"));
    }
}
