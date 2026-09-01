//! SQLite 打开与版本化迁移（§6.1）。
//!
//! 用 `PRAGMA user_version` 兼容已有 `jiti.db`：M2 库只有 `history` 且
//! user_version=0，升级时保留历史行并补 `mistakes` 表。v2 追加收藏与复习方案。

use rusqlite::Connection;
use tauri::{AppHandle, Manager as _};

use super::history::HISTORY_SCHEMA;

/// 当前库版本。破坏性表结构变更必须递增，并在 `migrate` 里追加分支。
pub const USER_VERSION: i32 = 2;

pub const MISTAKES_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS mistakes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  source_text TEXT NOT NULL,
  fragment TEXT NOT NULL,
  correction TEXT NOT NULL,
  error_type TEXT,
  severity TEXT DEFAULT 'medium',
  explanation TEXT,
  corrected_sentence TEXT,
  suggestions TEXT,
  engine TEXT,
  source_app TEXT,
  tags TEXT,
  status TEXT DEFAULT 'open',
  meta TEXT,
  server_id TEXT,
  synced_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_mistakes_created ON mistakes(created_at);
CREATE INDEX IF NOT EXISTS idx_mistakes_type ON mistakes(error_type);
"#;

pub const CLIPS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS clips (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  text TEXT NOT NULL,
  text_norm TEXT NOT NULL,
  note TEXT,
  kind TEXT,
  source_app TEXT,
  status TEXT DEFAULT 'open',
  meta TEXT,
  server_id TEXT,
  synced_at TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_clips_text_norm ON clips(text_norm);
CREATE INDEX IF NOT EXISTS idx_clips_created ON clips(created_at);
CREATE INDEX IF NOT EXISTS idx_clips_status ON clips(status);
"#;

pub const REVIEW_PLANS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS review_plans (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  status TEXT NOT NULL DEFAULT 'active',
  horizon_days INTEGER NOT NULL,
  analyzed_count INTEGER NOT NULL,
  summary TEXT NOT NULL,
  days_json TEXT NOT NULL,
  engine TEXT,
  duration_ms INTEGER,
  prompt_version TEXT,
  filter_json TEXT,
  meta TEXT
);
CREATE INDEX IF NOT EXISTS idx_review_plans_status ON review_plans(status);

CREATE TABLE IF NOT EXISTS review_plan_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  plan_id INTEGER NOT NULL,
  day_index INTEGER NOT NULL,
  sort_order INTEGER NOT NULL,
  source_kind TEXT NOT NULL,
  source_id INTEGER NOT NULL,
  prompt_text TEXT NOT NULL,
  expected TEXT NOT NULL,
  hint TEXT,
  fragment TEXT,
  result TEXT,
  reviewed_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_review_plan_items_plan ON review_plan_items(plan_id, day_index);
"#;

pub fn user_version(conn: &Connection) -> rusqlite::Result<i32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version = user_version(conn)?;
    if version < 1 {
        conn.execute_batch(HISTORY_SCHEMA)?;
        conn.execute_batch(MISTAKES_SCHEMA)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    let version = user_version(conn)?;
    if version < 2 {
        conn.execute_batch(CLIPS_SCHEMA)?;
        conn.execute_batch(REVIEW_PLANS_SCHEMA)?;
        conn.pragma_update(None, "user_version", USER_VERSION)?;
    }
    Ok(())
}

pub fn open(app: &AppHandle) -> Result<Connection, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("取 app data 目录失败：{e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let conn = Connection::open(dir.join("jiti.db")).map_err(|e| e.to_string())?;
    migrate(&conn).map_err(|e| e.to_string())?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn table_exists(conn: &Connection, name: &str) -> i32 {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
            params![name],
            |row| row.get(0),
        )
        .unwrap()
    }

    #[test]
    fn fresh_db_creates_core_tables_and_sets_version() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(user_version(&conn).unwrap(), 0);
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn).unwrap(), USER_VERSION);
        assert_eq!(table_exists(&conn, "history"), 1);
        assert_eq!(table_exists(&conn, "mistakes"), 1);
        assert_eq!(table_exists(&conn, "clips"), 1);
        assert_eq!(table_exists(&conn, "review_plans"), 1);
        assert_eq!(table_exists(&conn, "review_plan_items"), 1);
    }

    #[test]
    fn old_history_only_db_keeps_rows_and_gains_later_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(HISTORY_SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO history (kind, input, output, engine, duration_ms)
             VALUES ('translate', 'Hello', '你好', 'mock', 12)",
            [],
        )
        .unwrap();
        assert_eq!(user_version(&conn).unwrap(), 0);

        migrate(&conn).unwrap();

        assert_eq!(user_version(&conn).unwrap(), USER_VERSION);
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let input: String = conn
            .query_row("SELECT input FROM history WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(input, "Hello");
        conn.execute(
            "INSERT INTO mistakes (source_text, fragment, correction, status)
             VALUES (?1, ?2, ?3, 'open')",
            params!["He go.", "go", "goes"],
        )
        .unwrap();
        let mistakes: i32 = conn
            .query_row("SELECT COUNT(*) FROM mistakes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mistakes, 1);
        assert_eq!(table_exists(&conn, "clips"), 1);
    }

    #[test]
    fn v1_mistakes_db_gains_clips_and_plans() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(HISTORY_SCHEMA).unwrap();
        conn.execute_batch(MISTAKES_SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO mistakes (source_text, fragment, correction, status)
             VALUES ('He go.', 'go', 'goes', 'open')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();

        migrate(&conn).unwrap();

        assert_eq!(user_version(&conn).unwrap(), 2);
        let mistakes: i32 = conn
            .query_row("SELECT COUNT(*) FROM mistakes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mistakes, 1);
        assert_eq!(table_exists(&conn, "clips"), 1);
        assert_eq!(table_exists(&conn, "review_plans"), 1);
    }

    #[test]
    fn migrate_is_idempotent_on_current_version() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn).unwrap(), USER_VERSION);
    }

    #[test]
    fn schema_matches_architecture_fields() {
        assert!(MISTAKES_SCHEMA.contains("source_text TEXT NOT NULL"));
        assert!(MISTAKES_SCHEMA.contains("server_id TEXT"));
        assert!(MISTAKES_SCHEMA.contains("synced_at TEXT"));
        assert!(MISTAKES_SCHEMA.contains("idx_mistakes_created"));
        assert!(MISTAKES_SCHEMA.contains("idx_mistakes_type"));
        assert!(CLIPS_SCHEMA.contains("text_norm TEXT NOT NULL"));
        assert!(CLIPS_SCHEMA.contains("idx_clips_text_norm"));
        assert!(CLIPS_SCHEMA.contains("server_id TEXT"));
        assert!(REVIEW_PLANS_SCHEMA.contains("days_json TEXT NOT NULL"));
        assert!(REVIEW_PLANS_SCHEMA.contains("source_kind TEXT NOT NULL"));
    }
}
