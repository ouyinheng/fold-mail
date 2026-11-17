use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Flags;

/// 邮件账号（不含凭据，凭据存系统 keyring）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub email: String,
    pub display_name: Option<String>,
    /// gmail / outlook / qq / 163 / custom
    pub provider: Option<String>,
    /// oauth2 / app_password
    pub auth_type: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderKind {
    Inbox,
    Sent,
    Draft,
    Archive,
    Trash,
    Custom,
}

impl FolderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Sent => "sent",
            Self::Draft => "draft",
            Self::Archive => "archive",
            Self::Trash => "trash",
            Self::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "inbox" => Self::Inbox,
            "sent" => Self::Sent,
            "draft" => Self::Draft,
            "archive" => Self::Archive,
            "trash" => Self::Trash,
            _ => Self::Custom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub account_id: i64,
    /// IMAP 原始名，如 INBOX
    pub remote_name: String,
    pub local_name: Option<String>,
    pub kind: FolderKind,
    pub flags: Vec<String>,
}

/// 可渲染正文的类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    Html,
    Text,
    /// blitz 渲染失败，回退纯文本
    RenderFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub account_id: i64,
    pub folder_id: Option<i64>,
    pub remote_uid: Option<i64>,
    pub thread_id: Option<i64>,
    pub from_addr: String,
    pub from_name: Option<String>,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub subject: Option<String>,
    /// 纯文本摘要，列表直接读，无需解析正文
    pub preview: Option<String>,
    pub date: DateTime<Utc>,
    pub flags: Flags,
    pub has_attachment: bool,
    pub body_kind: BodyKind,
    /// 清洗后的可渲染正文
    pub body_cached: Option<String>,
    pub is_encrypted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Message {
    pub fn is_unread(&self) -> bool {
        !self.flags.is_seen()
    }
    pub fn is_starred(&self) -> bool {
        self.flags.is_starred()
    }
    pub fn display_subject(&self) -> &str {
        self.subject.as_deref().unwrap_or("(无主题)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i64,
    pub message_id: i64,
    pub filename: String,
    pub content_type: Option<String>,
    pub size: i64,
    /// 相对附件目录
    pub storage_path: String,
    /// 内联图片引用
    pub cid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: i64,
    pub subject: Option<String>,
    pub last_message_at: DateTime<Utc>,
    pub message_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxStatus {
    Pending,
    Sending,
    Sent,
    Failed,
}

impl OutboxStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "sending" => Self::Sending,
            "sent" => Self::Sent,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// 本地发信队列。定时发送与 10 秒撤销都靠 send_at + status。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outbox {
    pub id: i64,
    pub account_id: i64,
    pub from_addr: String,
    pub to_addrs: Vec<String>,
    pub cc_addrs: Vec<String>,
    pub bcc_addrs: Vec<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
    /// 附件相对路径列表（JSON）
    pub attachments: Vec<String>,
    pub send_at: DateTime<Utc>,
    pub status: OutboxStatus,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_kind_roundtrip() {
        for k in [
            FolderKind::Inbox,
            FolderKind::Sent,
            FolderKind::Draft,
            FolderKind::Archive,
            FolderKind::Trash,
            FolderKind::Custom,
        ] {
            assert_eq!(FolderKind::parse(k.as_str()), k);
        }
    }

    #[test]
    fn outbox_status_roundtrip() {
        for s in [
            OutboxStatus::Pending,
            OutboxStatus::Sending,
            OutboxStatus::Sent,
            OutboxStatus::Failed,
        ] {
            assert_eq!(OutboxStatus::parse(s.as_str()), s);
        }
        assert_eq!(OutboxStatus::parse("unknown"), OutboxStatus::Pending);
    }
}
