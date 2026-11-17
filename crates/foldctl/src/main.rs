use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use fold_core::{config, store};
use std::io::Write;

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_target(false)
        .init();
}

#[derive(Parser)]
#[command(name = "foldctl", about = "Fold 邮件客户端命令行工具")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 列出邮件
    List {
        #[arg(long, default_value = "20")]
        limit: i64,
        #[arg(long)]
        unread: bool,
    },
    /// 搜索邮件（M1 用 SQL LIKE，M3 换 Tantivy）
    Search {
        query: String,
        #[arg(long, default_value = "20")]
        limit: i64,
    },
    /// 标为已读
    Read { id: i64 },
    /// 切换星标
    Star { id: i64 },
    /// 导出为 .eml
    Export {
        id: i64,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// 显示配置路径
    Config,
}

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    let db_path = config::data_dir().join("fold.sqlite");
    let repo =
        store::open(&db_path).with_context(|| format!("打开数据库失败: {}", db_path.display()))?;

    match cli.command {
        Commands::List { limit, unread } => {
            let messages = repo.list_messages(None, limit)?;
            for m in messages.into_iter().filter(|m| !unread || m.is_unread()) {
                let flag = if m.is_unread() { "●" } else { " " };
                let star = if m.is_starred() { "★" } else { " " };
                println!(
                    "{}{}{:>5}  {:<22} {:<40} {}",
                    flag,
                    star,
                    m.id,
                    m.from_addr,
                    truncate(m.display_subject(), 40),
                    m.date.format("%m-%d %H:%M")
                );
            }
        }
        Commands::Search { query, limit } => {
            let messages = repo.search_messages(&query, limit)?;
            if messages.is_empty() {
                println!("没有匹配的邮件");
            }
            for m in messages {
                println!(
                    "{:>5}  {:<22} {}",
                    m.id,
                    m.from_addr,
                    truncate(m.display_subject(), 50)
                );
            }
        }
        Commands::Read { id } => {
            repo.mark_seen(id)?;
            println!("已将 {} 标为已读", id);
        }
        Commands::Star { id } => {
            let starred = repo.toggle_star(id)?;
            println!(
                "{}",
                if starred {
                    "已加星标"
                } else {
                    "已取消星标"
                }
            );
        }
        Commands::Export { id, output } => {
            let m = repo
                .get_message(id)?
                .ok_or_else(|| anyhow::anyhow!("邮件 {} 不存在", id))?;
            let path = output.unwrap_or_else(|| format!("{}.eml", id));
            let mut f = std::fs::File::create(&path)?;
            writeln!(f, "From: {}", m.from_addr)?;
            writeln!(f, "To: {}", m.to_addrs.join(", "))?;
            writeln!(f, "Subject: {}", m.display_subject())?;
            writeln!(f, "Date: {}", m.date.to_rfc3339())?;
            writeln!(f)?;
            writeln!(
                f,
                "{}",
                m.body_cached
                    .as_deref()
                    .unwrap_or(m.preview.as_deref().unwrap_or(""))
            )?;
            println!("已导出到 {}", path);
        }
        Commands::Config => {
            println!("数据目录: {}", config::data_dir().display());
            println!("配置目录: {}", config::config_dir().display());
            println!("数据库:   {}", db_path.display());
        }
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        format!("{}…", chars[..max].iter().collect::<String>())
    }
}
