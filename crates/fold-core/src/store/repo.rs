use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::error::Result;
use crate::model::{
    Account, BodyKind, FLAG_FLAGGED, FLAG_SEEN, Flags, Folder, FolderKind, Message, Outbox,
    OutboxStatus,
};

/// 数据仓库。所有方法同步执行；调用方（async）应放到 spawn_blocking。
pub struct Repository {
    conn: Connection,
}

impl Repository {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    // ---------------- Accounts ----------------

    pub fn insert_account(&self, email: &str, auth_type: &str) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO accounts (email, auth_type, created_at) VALUES (?1, ?2, ?3)",
            params![email, auth_type, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_account(&self, id: i64) -> Result<Option<Account>> {
        self.conn
            .query_row(
                "SELECT id, email, display_name, provider, auth_type, enabled, created_at
                 FROM accounts WHERE id=?1",
                params![id],
                |r| {
                    Ok(Account {
                        id: r.get(0)?,
                        email: r.get(1)?,
                        display_name: r.get(2)?,
                        provider: r.get(3)?,
                        auth_type: r.get(4)?,
                        enabled: r.get::<_, i64>(5)? != 0,
                        created_at: parse_rfc3339(&r.get::<_, String>(6)?)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_accounts(&self) -> Result<Vec<Account>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, email, display_name, provider, auth_type, enabled, created_at
             FROM accounts ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Account {
                id: r.get(0)?,
                email: r.get(1)?,
                display_name: r.get(2)?,
                provider: r.get(3)?,
                auth_type: r.get(4)?,
                enabled: r.get::<_, i64>(5)? != 0,
                created_at: parse_rfc3339(&r.get::<_, String>(6)?)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    // ---------------- Folders ----------------

    pub fn upsert_folder(
        &self,
        account_id: i64,
        remote_name: &str,
        local_name: Option<&str>,
        kind: FolderKind,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO folders (account_id, remote_name, local_name, kind, flags)
             VALUES (?1, ?2, ?3, ?4, '[]')
             ON CONFLICT(account_id, remote_name) DO UPDATE SET
               local_name=excluded.local_name, kind=excluded.kind",
            params![account_id, remote_name, local_name, kind.as_str()],
        )?;
        Ok(self.conn.query_row(
            "SELECT id FROM folders WHERE account_id=?1 AND remote_name=?2",
            params![account_id, remote_name],
            |r| r.get(0),
        )?)
    }

    pub fn list_folders(&self, account_id: i64) -> Result<Vec<Folder>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, account_id, remote_name, local_name, kind, flags
             FROM folders WHERE account_id=?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![account_id], |r| {
            let flags_json: String = r.get(5)?;
            let flags: Vec<String> = serde_json::from_str(&flags_json).unwrap_or_default();
            Ok(Folder {
                id: r.get(0)?,
                account_id: r.get(1)?,
                remote_name: r.get(2)?,
                local_name: r.get(3)?,
                kind: FolderKind::parse(r.get::<_, String>(4)?.as_str()),
                flags,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    // ---------------- Messages ----------------

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_message(
        &self,
        account_id: i64,
        folder_id: Option<i64>,
        remote_uid: Option<i64>,
        from_addr: &str,
        from_name: Option<&str>,
        subject: Option<&str>,
        preview: Option<&str>,
        date: DateTime<Utc>,
        flags: Flags,
        body_kind: BodyKind,
        body_cached: Option<&str>,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let date_s = date.to_rfc3339();
        let kind_s = match body_kind {
            BodyKind::Html => "html",
            BodyKind::Text => "text",
            BodyKind::RenderFailed => "render_failed",
        };
        self.conn.execute(
            "INSERT INTO messages
               (account_id, folder_id, remote_uid, from_addr, from_name, to_addrs, cc_addrs,
                subject, preview, date, flags, has_attachment, body_kind, body_cached,
                is_encrypted, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,'[]','[]',?6,?7,?8,?9,0,?10,?11,0,?12,?12)
             ON CONFLICT(account_id, folder_id, remote_uid) DO UPDATE SET
               subject=excluded.subject, preview=excluded.preview,
               flags=excluded.flags, body_cached=excluded.body_cached,
               updated_at=excluded.updated_at",
            params![
                account_id,
                folder_id,
                remote_uid,
                from_addr,
                from_name,
                subject,
                preview,
                date_s,
                flags.bits(),
                kind_s,
                body_cached,
                now
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_message(&self, id: i64) -> Result<Option<Message>> {
        self.conn
            .query_row(
                "SELECT * FROM messages WHERE id=?1",
                params![id],
                map_message,
            )
            .optional()
            .map_err(Into::into)
    }

    /// 按时间倒序列出邮件，支持按文件夹过滤。
    pub fn list_messages(&self, folder_id: Option<i64>, limit: i64) -> Result<Vec<Message>> {
        let sql = match folder_id {
            Some(_) => "SELECT * FROM messages WHERE folder_id=?1 ORDER BY date DESC LIMIT ?2",
            None => "SELECT * FROM messages ORDER BY date DESC LIMIT ?1",
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = if let Some(fid) = folder_id {
            stmt.query_map(params![fid, limit], map_message)?
        } else {
            stmt.query_map(params![limit], map_message)?
        };
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn search_messages(&self, keyword: &str, limit: i64) -> Result<Vec<Message>> {
        // M1 先用 SQL LIKE；M3 由 Tantivy 替换
        let like = format!("%{keyword}%");
        let mut stmt = self.conn.prepare(
            "SELECT * FROM messages
             WHERE from_addr LIKE ?1 OR subject LIKE ?1 OR preview LIKE ?1
             ORDER BY date DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![like, limit], map_message)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn set_flags(&self, id: i64, flags: Flags) -> Result<()> {
        self.conn.execute(
            "UPDATE messages SET flags=?1, updated_at=?2 WHERE id=?3",
            params![flags.bits(), Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn mark_seen(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE messages SET flags = flags | ?1, updated_at=?2 WHERE id=?3",
            params![FLAG_SEEN, Utc::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn toggle_star(&self, id: i64) -> Result<bool> {
        // SQLite 没有按位异或运算符，在 Rust 侧读取并计算
        let flags: i64 =
            self.conn
                .query_row("SELECT flags FROM messages WHERE id=?1", params![id], |r| {
                    r.get(0)
                })?;
        let mut flags = Flags::new(flags);
        flags.toggle(FLAG_FLAGGED);
        self.set_flags(id, flags)?;
        Ok(flags.is_starred())
    }

    pub fn unread_count(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM messages WHERE (flags & ?1) = 0",
            params![FLAG_SEEN],
            |r| r.get(0),
        )?)
    }

    pub fn delete_message(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM messages WHERE id=?1", params![id])?;
        Ok(())
    }

    // ---------------- Outbox ----------------

    pub fn enqueue(&self, outbox: &Outbox) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO outbox
               (account_id, from_addr, to_addrs, cc_addrs, bcc_addrs, subject, body,
                attachments, send_at, status, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                outbox.account_id,
                outbox.from_addr,
                serde_json::to_string(&outbox.to_addrs)?,
                serde_json::to_string(&outbox.cc_addrs)?,
                serde_json::to_string(&outbox.bcc_addrs)?,
                outbox.subject,
                outbox.body,
                serde_json::to_string(&outbox.attachments)?,
                outbox.send_at.to_rfc3339(),
                outbox.status.as_str(),
                now,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 取所有到期待发送的邮件。
    pub fn due_outbox(&self, now: DateTime<Utc>) -> Result<Vec<Outbox>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM outbox WHERE status='pending' AND send_at<=?1 ORDER BY send_at",
        )?;
        let rows = stmt.query_map(params![now.to_rfc3339()], map_outbox)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn set_outbox_status(&self, id: i64, status: OutboxStatus) -> Result<()> {
        self.conn.execute(
            "UPDATE outbox SET status=?1 WHERE id=?2",
            params![status.as_str(), id],
        )?;
        Ok(())
    }

    /// 撤销发送：删除 outbox 记录（本质是改回草稿，M2 接 SMTP 后完善）。
    pub fn cancel_outbox(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM outbox WHERE id=?1", params![id])?;
        Ok(())
    }
}

fn parse_rfc3339(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn map_message(r: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let date_s: String = r.get("date")?;
    let created_s: String = r.get("created_at")?;
    let updated_s: String = r.get("updated_at")?;
    let parse = |s: String| -> rusqlite::Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    };
    let to_json: String = r.get("to_addrs")?;
    let cc_json: String = r.get("cc_addrs")?;
    Ok(Message {
        id: r.get("id")?,
        account_id: r.get("account_id")?,
        folder_id: r.get("folder_id")?,
        remote_uid: r.get("remote_uid")?,
        thread_id: r.get("thread_id")?,
        from_addr: r.get("from_addr")?,
        from_name: r.get("from_name")?,
        to_addrs: serde_json::from_str(&to_json).unwrap_or_default(),
        cc_addrs: serde_json::from_str(&cc_json).unwrap_or_default(),
        subject: r.get("subject")?,
        preview: r.get("preview")?,
        date: parse(date_s)?,
        flags: Flags::new(r.get("flags")?),
        has_attachment: r.get::<_, i64>("has_attachment")? != 0,
        body_kind: match r.get::<_, String>("body_kind")?.as_str() {
            "text" => BodyKind::Text,
            "render_failed" => BodyKind::RenderFailed,
            _ => BodyKind::Html,
        },
        body_cached: r.get("body_cached")?,
        is_encrypted: r.get::<_, i64>("is_encrypted")? != 0,
        created_at: parse(created_s)?,
        updated_at: parse(updated_s)?,
    })
}

fn map_outbox(r: &rusqlite::Row<'_>) -> rusqlite::Result<Outbox> {
    let parse = |s: String| -> rusqlite::Result<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })
    };
    let to: String = r.get("to_addrs")?;
    let cc: String = r.get("cc_addrs")?;
    let bcc: String = r.get("bcc_addrs")?;
    let att: String = r.get("attachments")?;
    let status: String = r.get("status")?;
    Ok(Outbox {
        id: r.get("id")?,
        account_id: r.get("account_id")?,
        from_addr: r.get("from_addr")?,
        to_addrs: serde_json::from_str(&to).unwrap_or_default(),
        cc_addrs: serde_json::from_str(&cc).unwrap_or_default(),
        bcc_addrs: serde_json::from_str(&bcc).unwrap_or_default(),
        subject: r.get("subject")?,
        body: r.get("body")?,
        attachments: serde_json::from_str(&att).unwrap_or_default(),
        send_at: parse(r.get("send_at")?)?,
        status: OutboxStatus::parse(&status),
        created_at: parse(r.get("created_at")?)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BodyKind;
    use chrono::{Duration, Utc};

    fn setup() -> Repository {
        let repo = Repository::new(Connection::open_in_memory().unwrap());
        crate::store::schema::configure(repo.conn()).unwrap();
        crate::store::schema::migrate(repo.conn()).unwrap();
        repo
    }

    fn seed_message(repo: &Repository, from: &str, subject: &str) -> i64 {
        // 外键要求 account/folder 先存在
        let acc_id = match repo.list_accounts().unwrap().first() {
            Some(a) => a.id,
            None => repo.insert_account("me@example.com", "oauth2").unwrap(),
        };
        let folder_id = repo
            .upsert_folder(acc_id, "INBOX", Some("收件箱"), FolderKind::Inbox)
            .unwrap();
        repo.upsert_message(
            acc_id,
            Some(folder_id),
            Some(1),
            from,
            None,
            Some(subject),
            Some("摘要"),
            Utc::now(),
            Flags::default(),
            BodyKind::Text,
            Some("正文"),
        )
        .unwrap()
    }

    #[test]
    fn account_crud() {
        let repo = setup();
        let id = repo.insert_account("a@b.com", "oauth2").unwrap();
        let acc = repo.get_account(id).unwrap().unwrap();
        assert_eq!(acc.email, "a@b.com");
        assert_eq!(acc.auth_type, "oauth2");
        assert!(acc.enabled);
        assert_eq!(repo.list_accounts().unwrap().len(), 1);
    }

    #[test]
    fn folder_upsert_is_idempotent() {
        let repo = setup();
        repo.insert_account("a@b.com", "oauth2").unwrap();
        let id1 = repo
            .upsert_folder(1, "INBOX", Some("收件箱"), FolderKind::Inbox)
            .unwrap();
        let id2 = repo
            .upsert_folder(1, "INBOX", Some("收件箱"), FolderKind::Inbox)
            .unwrap();
        assert_eq!(id1, id2);
        let folders = repo.list_folders(1).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].kind, FolderKind::Inbox);
    }

    #[test]
    fn message_crud_and_flags() {
        let repo = setup();
        let id = seed_message(&repo, "alice@example.com", "你好");
        let m = repo.get_message(id).unwrap().unwrap();
        assert!(m.is_unread());
        assert!(!m.is_starred());

        repo.mark_seen(id).unwrap();
        assert!(!repo.get_message(id).unwrap().unwrap().is_unread());

        assert!(repo.toggle_star(id).unwrap());
        assert!(repo.get_message(id).unwrap().unwrap().is_starred());
        assert!(!repo.toggle_star(id).unwrap());

        assert_eq!(repo.unread_count().unwrap(), 0);
    }

    #[test]
    fn search_messages_by_sender_and_subject() {
        let repo = setup();
        seed_message(&repo, "alice@example.com", "项目进展");
        seed_message(&repo, "bob@example.com", "合同确认");
        assert_eq!(repo.search_messages("alice", 10).unwrap().len(), 1);
        assert_eq!(repo.search_messages("合同", 10).unwrap().len(), 1);
        assert!(repo.search_messages("不存在", 10).unwrap().is_empty());
    }

    #[test]
    fn outbox_enqueue_due_and_cancel() {
        let repo = setup();
        repo.insert_account("a@b.com", "oauth2").unwrap();
        let out = Outbox {
            id: 0,
            account_id: 1,
            from_addr: "a@b.com".into(),
            to_addrs: vec!["c@d.com".into()],
            cc_addrs: vec![],
            bcc_addrs: vec![],
            subject: Some("测试".into()),
            body: Some("内容".into()),
            attachments: vec![],
            send_at: Utc::now() + Duration::seconds(10),
            status: OutboxStatus::Pending,
            created_at: Utc::now(),
        };
        let id = repo.enqueue(&out).unwrap();
        assert!(
            repo.due_outbox(Utc::now()).unwrap().is_empty(),
            "未到时间不应发出"
        );
        assert_eq!(
            repo.due_outbox(Utc::now() + Duration::seconds(20))
                .unwrap()
                .len(),
            1
        );

        repo.cancel_outbox(id).unwrap();
        assert!(
            repo.due_outbox(Utc::now() + Duration::seconds(20))
                .unwrap()
                .is_empty(),
            "撤销后不应发出"
        );
    }

    #[test]
    fn delete_message_cascades() {
        let repo = setup();
        let id = seed_message(&repo, "a@b.com", "测试");
        repo.delete_message(id).unwrap();
        assert!(repo.get_message(id).unwrap().is_none());
    }
}
