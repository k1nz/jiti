//! 历史 SQLite 服务（§6.1：`history` 表）。
//!
//! 打开与迁移见 `database.rs`；本模块只负责 history 的读写。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

pub const HISTORY_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS history (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  kind TEXT NOT NULL,
  input TEXT, output TEXT,
  engine TEXT, from_lang TEXT, to_lang TEXT,
  duration_ms INTEGER, source_app TEXT, meta TEXT
);
CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at);
"#;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i32,
    pub created_at: String,
    pub kind: String,
    pub input: String,
    pub output: String,
    pub engine: String,
    pub from_lang: Option<String>,
    pub to_lang: Option<String>,
    pub duration_ms: u32,
    pub source_app: Option<String>,
    pub meta: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewHistoryEntry {
    pub kind: String,
    pub input: String,
    pub output: String,
    pub engine: String,
    pub from_lang: Option<String>,
    pub to_lang: Option<String>,
    pub duration_ms: i64,
    pub source_app: Option<String>,
    pub meta: Option<String>,
}

pub fn open(app: &AppHandle) -> Result<Connection, String> {
    crate::services::database::open(app)
}

pub fn insert(conn: &Connection, entry: &NewHistoryEntry) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO history (kind, input, output, engine, from_lang, to_lang, duration_ms, source_app, meta)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            entry.kind,
            entry.input,
            entry.output,
            entry.engine,
            entry.from_lang,
            entry.to_lang,
            entry.duration_ms,
            entry.source_app,
            entry.meta,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn list(conn: &Connection, limit: i64) -> Result<Vec<HistoryEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, created_at, kind, input, output, engine, from_lang, to_lang,
                    duration_ms, source_app, meta
             FROM history ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                created_at: row.get(1)?,
                kind: row.get(2)?,
                input: row.get(3).unwrap_or_default(),
                output: row.get(4).unwrap_or_default(),
                engine: row.get(5).unwrap_or_default(),
                from_lang: row.get(6)?,
                to_lang: row.get(7)?,
                duration_ms: row.get(8)?,
                source_app: row.get(9)?,
                meta: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())
}

pub fn clear(conn: &Connection) -> Result<i32, String> {
    conn.execute("DELETE FROM history", [])
        .map(|n| n as i32)
        .map_err(|e| e.to_string())
}

pub fn record(app: &AppHandle, entry: NewHistoryEntry) -> Result<i64, String> {
    let conn = open(app)?;
    insert(&conn, &entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(input: &str, output: &str) -> NewHistoryEntry {
        NewHistoryEntry {
            kind: "translate".into(),
            input: input.into(),
            output: output.into(),
            engine: "mock".into(),
            from_lang: None,
            to_lang: Some("zh".into()),
            duration_ms: 12,
            source_app: None,
            meta: None,
        }
    }

    #[test]
    fn history_roundtrip_and_clear() {
        let conn = Connection::open_in_memory().unwrap();
        crate::services::database::migrate(&conn).unwrap();
        let id = insert(&conn, &entry("Hello", "你好")).unwrap();
        assert!(id > 0);
        let rows = list(&conn, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].input, "Hello");
        assert_eq!(rows[0].to_lang.as_deref(), Some("zh"));
        assert_eq!(clear(&conn).unwrap(), 1);
        assert!(list(&conn, 10).unwrap().is_empty());
    }

    #[test]
    fn schema_matches_architecture_fields() {
        assert!(HISTORY_SCHEMA.contains("kind TEXT NOT NULL"));
        assert!(HISTORY_SCHEMA.contains("source_app TEXT"));
        assert!(HISTORY_SCHEMA.contains("meta TEXT"));
    }
}
