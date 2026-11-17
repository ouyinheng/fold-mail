//! fold-ipc — UI 与 core 之间的命令协议与 DTO。
//!
//! M1 阶段 UI 与 core 同进程，此 crate 只定义共享类型；
//! 后续可无缝切换到 Unix Socket / 本地 HTTP。

use serde::{Deserialize, Serialize};

/// UI 可发给 core 的命令。与快捷键共用同一套枚举。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    OpenMessage { id: i64 },
    Archive { id: i64 },
    Delete { id: i64 },
    ToggleStar { id: i64 },
    MarkLater { id: i64 },
    Move { delta: i32 },
    Search { query: String },
    SetFolder { folder: String },
    ToggleTheme,
    Undo,
}

/// core 广播给 UI 的领域事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainEvent {
    NewMessage { id: i64 },
    MessageChanged { id: i64 },
    MessageRemoved { id: i64 },
    SyncCompleted { account_id: i64, new_count: i64 },
    SendFailed { id: i64, reason: String },
}
