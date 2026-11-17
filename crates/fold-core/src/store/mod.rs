pub mod repo;
pub mod schema;

pub use repo::Repository;
use std::path::PathBuf;

/// 打开（或创建）主数据库并完成迁移。
pub fn open(path: impl Into<PathBuf>) -> crate::error::Result<Repository> {
    let path = path.into();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| crate::error::CoreError::StoreIo {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let conn = rusqlite::Connection::open(&path)?;
    schema::configure(&conn)?;
    schema::migrate(&conn)?;
    Ok(Repository::new(conn))
}
