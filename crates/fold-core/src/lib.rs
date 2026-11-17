//! fold-core — Fold 邮件客户端的本地优先核心库。
//!
//! UI（桌面端 / CLI）只读本地数据；网络操作异步完成后再通知界面。

pub mod config;
pub mod error;
pub mod model;
pub mod store;

pub use error::{CoreError, Result};
pub use store::{Repository, open};
