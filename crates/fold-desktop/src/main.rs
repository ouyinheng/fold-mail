//! Fold 桌面端入口。
//!
//! UI 框架在 M0 技术验证后确定（iced 或 GPUI）。M1 阶段先验证
//! 核心库可链接、本地数据可读，不引入重量级 UI 依赖。

use anyhow::Result;
use fold_core::{config, store};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_path = config::data_dir().join("fold.sqlite");
    let repo = store::open(&db_path)?;
    let count = repo.list_messages(None, 1)?.len();
    let accounts = repo.list_accounts()?.len();

    tracing::info!(
        db = %db_path.display(),
        accounts,
        recent_messages = count,
        "fold-desktop M1 skeleton ready"
    );
    println!("Fold 桌面端骨架就绪 — 数据库: {}", db_path.display());
    println!("账号数: {}, 最近邮件: {}", accounts, count);
    println!("UI 框架将在 M0 技术验证后接入。");
    Ok(())
}
