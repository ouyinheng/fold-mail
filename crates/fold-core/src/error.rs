use std::path::PathBuf;

/// fold-core 的统一错误类型。
///
/// 每个变体都带有足够的上下文，UI 层据此显示可行动提示而不是原始报错。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("存储层错误: {0}")]
    Store(#[from] rusqlite::Error),

    #[error("存储层 IO 错误: {path}: {source}")]
    StoreIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("配置错误: {0}")]
    Config(String),

    #[error("JSON 序列化错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("配置读取失败 {path}: {source}")]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("配置解析失败 {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("邮件解析失败: {0}")]
    Parse(String),

    #[error("未找到: {kind} id={id}")]
    NotFound { kind: &'static str, id: i64 },

    #[error("参数无效: {0}")]
    InvalidArgument(String),

    #[error("凭据存储不可用: {0}")]
    CredentialUnavailable(String),

    #[error("同步失败: {0}")]
    Sync(String),

    #[error("发送失败: {0}")]
    Send(String),

    #[error("渲染失败: {0}")]
    Render(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
