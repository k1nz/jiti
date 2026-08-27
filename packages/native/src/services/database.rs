//! SQLite 打开与版本化迁移（§6.1）。
//!
//! 用 `PRAGMA user_version` 兼容已有 `jiti.db`：M2 库只有 `history` 且
//! user_version=0，升级时保留历史行并补 `mistakes` 表。

use rusqlite::Connection;
use tauri::{AppHandle, Manager as _};

use super::history::HISTORY_SCHEMA;

/// 当前库版本。破坏性表结构变更必须递增，并在 `migrate` 里追加分支。
pub const USER_VERSION: i32 = 1;

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

pub fn user_version(conn: &Connection) -> rusqlite::Result<i32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version = user_version(conn)?;
    if version < 1 {
        conn.execute_batch(HISTORY_SCHEMA)?;
        conn.execute_batch(MISTAKES_SCHEMA)?;
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

    #[test]
    fn fresh_db_creates_both_tables_and_sets_version() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(user_version(&conn).unwrap(), 0);
        migrate(&conn).unwrap();
        assert_eq!(user_version(&conn).unwrap(), USER_VERSION);
        let history_exists: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='history'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mistakes_exists: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='mistakes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history_exists, 1);
        assert_eq!(mistakes_exists, 1);
    }

    #[test]
    fn old_history_only_db_keeps_rows_and_gains_mistakes() {
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
    }
}
