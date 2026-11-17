//! SQLite schema 迁移。使用 PRAGMA user_version 做版本管理。

use crate::error::Result;
use rusqlite::Connection;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
  id            INTEGER PRIMARY KEY,
  email         TEXT NOT NULL UNIQUE,
  display_name  TEXT,
  provider      TEXT,
  auth_type     TEXT NOT NULL,
  enabled       INTEGER NOT NULL DEFAULT 1,
  created_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS folders (
  id           INTEGER PRIMARY KEY,
  account_id   INTEGER NOT NULL REFERENCES accounts(id),
  remote_name  TEXT NOT NULL,
  local_name   TEXT,
  kind         TEXT NOT NULL,
  flags        TEXT,
  UNIQUE(account_id, remote_name)
);

CREATE TABLE IF NOT EXISTS messages (
  id             INTEGER PRIMARY KEY,
  account_id     INTEGER NOT NULL REFERENCES accounts(id),
  folder_id      INTEGER REFERENCES folders(id),
  remote_uid     INTEGER,
  thread_id      INTEGER,
  from_addr      TEXT NOT NULL,
  from_name      TEXT,
  to_addrs       TEXT,
  cc_addrs       TEXT,
  subject        TEXT,
  preview        TEXT,
  date           TEXT NOT NULL,
  flags          INTEGER NOT NULL DEFAULT 0,
  has_attachment INTEGER NOT NULL DEFAULT 0,
  body_kind      TEXT NOT NULL,
  body_cached    TEXT,
  is_encrypted   INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  UNIQUE(account_id, folder_id, remote_uid)
);
CREATE INDEX IF NOT EXISTS idx_msg_date ON messages(date DESC);
CREATE INDEX IF NOT EXISTS idx_msg_thread ON messages(thread_id);
CREATE INDEX IF NOT EXISTS idx_msg_from ON messages(from_addr);

CREATE TABLE IF NOT EXISTS attachments (
  id           INTEGER PRIMARY KEY,
  message_id   INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  filename     TEXT NOT NULL,
  content_type TEXT,
  size         INTEGER,
  storage_path TEXT NOT NULL,
  cid          TEXT
);

CREATE TABLE IF NOT EXISTS threads (
  id              INTEGER PRIMARY KEY,
  subject         TEXT,
  last_message_at TEXT,
  message_count   INTEGER
);

CREATE TABLE IF NOT EXISTS labels (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE,
  color TEXT
);
CREATE TABLE IF NOT EXISTS message_labels (
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  label_id   INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (message_id, label_id)
);

CREATE TABLE IF NOT EXISTS outbox (
  id          INTEGER PRIMARY KEY,
  account_id  INTEGER NOT NULL REFERENCES accounts(id),
  from_addr   TEXT NOT NULL,
  to_addrs    TEXT NOT NULL,
  cc_addrs    TEXT,
  bcc_addrs   TEXT,
  subject     TEXT,
  body        TEXT,
  attachments TEXT,
  send_at     TEXT NOT NULL,
  status      TEXT NOT NULL,
  created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_outbox_pending ON outbox(send_at) WHERE status='pending';

CREATE TABLE IF NOT EXISTS sync_state (
  account_id   INTEGER NOT NULL REFERENCES accounts(id),
  folder_id    INTEGER NOT NULL REFERENCES folders(id),
  uidvalidity  INTEGER,
  last_uid     INTEGER,
  last_sync_at TEXT,
  PRIMARY KEY (account_id, folder_id)
);

CREATE TABLE IF NOT EXISTS index_state (
  account_id  INTEGER NOT NULL,
  folder_id   INTEGER NOT NULL,
  last_uid    INTEGER,
  indexed_at  TEXT,
  PRIMARY KEY (account_id, folder_id)
);

CREATE TABLE IF NOT EXISTS privacy_rules (
  id         INTEGER PRIMARY KEY,
  domain     TEXT NOT NULL UNIQUE,
  action     TEXT NOT NULL,
  scope      TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tracked_events (
  id         INTEGER PRIMARY KEY,
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  domain     TEXT NOT NULL,
  kind       TEXT NOT NULL,
  action     TEXT NOT NULL
);
"#;

/// 迁移到最新 schema。可安全重复调用。
pub fn migrate(conn: &Connection) -> Result<()> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version == 0 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1u32)?;
        tracing::info!("schema migrated to v1");
    }
    // 未来版本在此追加 else if 分支
    Ok(())
}

/// 应用标准 PRAGMA（WAL + 合理同步级别 + 忙等待）。
pub fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA busy_timeout=5000;
         PRAGMA foreign_keys=ON;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // 幂等

        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 12, "应有 12 张业务表");

        let v: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }
}
