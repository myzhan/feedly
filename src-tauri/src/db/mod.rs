pub mod articles;
pub mod feeds;
pub mod settings;

use std::path::Path;

use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};

pub type DbPool = r2d2::Pool<SqliteConnectionManager>;

/// Schema version 1. Applied when `PRAGMA user_version` is below 1.
const SCHEMA_V1: &str = r#"
CREATE TABLE feeds (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  parent_id INTEGER REFERENCES feeds(id) ON DELETE SET NULL,
  kind TEXT NOT NULL CHECK(kind IN ('feed','folder')),
  title TEXT NOT NULL,
  feed_url TEXT,
  site_url TEXT,
  description TEXT,
  icon_url TEXT,
  etag TEXT,
  last_modified TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  fetch_error TEXT,
  fetch_fail_count INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  last_fetched_at TEXT
);
CREATE INDEX idx_feeds_parent ON feeds(parent_id);
CREATE UNIQUE INDEX idx_feeds_url ON feeds(feed_url) WHERE feed_url IS NOT NULL;

CREATE TABLE articles (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
  guid TEXT NOT NULL,
  title TEXT,
  link TEXT,
  author TEXT,
  published_at TEXT,
  content_html TEXT NOT NULL,
  plain_text TEXT,
  is_starred INTEGER NOT NULL DEFAULT 0,
  read_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(feed_id, guid)
);
CREATE INDEX idx_articles_feed_time ON articles(feed_id, published_at DESC);
CREATE INDEX idx_articles_read ON articles(read_at);
CREATE INDEX idx_articles_starred ON articles(is_starred);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

/// Schema version 2. Adds full-text cache columns to `articles`. `fulltext_at`
/// marks that extraction was attempted (avoids retry storms); `fulltext_html`
/// holds the extracted article body when it was meaningfully richer than the
/// feed-provided `content_html`.
const SCHEMA_V2: &str = r#"
ALTER TABLE articles ADD COLUMN fulltext_html TEXT;
ALTER TABLE articles ADD COLUMN fulltext_at TEXT;
"#;

/// Schema version 3. Adds `body_chars` — the uncapped visible-text length of
/// `content_html`. The stored `plain_text` is a truncated list preview (capped
/// at ingest time), so it cannot tell whether a feed already ships a full,
/// readable body; `body_chars` fills that role for the `fulltext_ready` flag.
const SCHEMA_V3: &str = r#"
ALTER TABLE articles ADD COLUMN body_chars INTEGER NOT NULL DEFAULT 0;
"#;

/// Opens (creating if necessary) the SQLite database in `app_data_dir`, wires
/// up PRAGMAs per connection, runs migrations, and returns a ready pool.
pub fn init_pool(app_data_dir: &Path) -> AppResult<DbPool> {
    std::fs::create_dir_all(app_data_dir)?;
    let db_path = app_data_dir.join("feedly.db");

    let manager = SqliteConnectionManager::file(&db_path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             PRAGMA busy_timeout=5000;
             PRAGMA synchronous=NORMAL;",
        )
    });

    let pool = r2d2::Pool::builder()
        .max_size(8)
        .build(manager)
        .map_err(|e| AppError::Pool(e.to_string()))?;

    run_migrations(&pool)?;
    Ok(pool)
}

fn run_migrations(pool: &DbPool) -> AppResult<()> {
    let conn = pool.get()?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }

    if version < 2 {
        conn.execute_batch(SCHEMA_V2)?;
        conn.pragma_update(None, "user_version", 2)?;
    }

    if version < 3 {
        conn.execute_batch(SCHEMA_V3)?;
        backfill_body_chars(&conn)?;
        conn.pragma_update(None, "user_version", 3)?;
    }

    Ok(())
}

/// Recomputes `body_chars` for rows that predate the column, using the same
/// uncapped visible-text measure the ingest path now writes. SQLite cannot strip
/// HTML, so this runs in Rust over the stored `content_html`.
fn backfill_body_chars(conn: &Connection) -> AppResult<()> {
    let mut stmt = conn.prepare("SELECT id, content_html FROM articles WHERE body_chars = 0")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    drop(stmt);
    for (id, html) in rows {
        let n = crate::feed::content::plain_text(&html, usize::MAX)
            .chars()
            .count() as i64;
        conn.execute(
            "UPDATE articles SET body_chars = ?1 WHERE id = ?2",
            params![n, id],
        )?;
    }
    Ok(())
}

/// Current UTC timestamp formatted for storage (ISO-8601 / RFC-3339).
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Builds an in-memory-backed pool for tests using a temp file so WAL and
    /// foreign keys behave like production.
    pub fn test_pool() -> (DbPool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_pool(dir.path()).unwrap();
        (pool, dir)
    }

    #[test]
    fn migrations_create_tables() {
        let (pool, _dir) = test_pool();
        let conn = pool.get().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('feeds','articles','settings')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn migration_v2_adds_fulltext_columns() {
        let (pool, _dir) = test_pool();
        let conn = pool.get().unwrap();
        let cols: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('articles') WHERE name IN ('fulltext_html','fulltext_at')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cols, 2);
    }

    #[test]
    fn migration_v3_adds_body_chars_column() {
        let (pool, _dir) = test_pool();
        let conn = pool.get().unwrap();
        let has: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('articles') WHERE name = 'body_chars'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(has, 1);
    }
}
