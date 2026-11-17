//! TOML + XDG 配置。配置只存偏好和账号列表，不含凭据。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: Theme,
    pub density: Density,
    pub serif_body: bool,
    pub default_account: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            density: Density::Comfortable,
            serif_body: false,
            default_account: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Comfortable,
    Compact,
}

/// 返回 fold 的数据目录（XDG_DATA_HOME/fold），macOS 回退到 Application Support。
pub fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("fold");
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library/Application Support")
                .join("fold");
        }
    }
    PathBuf::from(".fold")
}

/// 返回 fold 的配置目录（XDG_CONFIG_HOME/fold）。
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("fold");
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Application Support/fold");
        }
    }
    PathBuf::from(".fold")
}

pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(path).map_err(|source| CoreError::ConfigRead {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| CoreError::ConfigParse {
        path: path.to_path_buf(),
        source,
    })
}

pub fn save(path: &Path, cfg: &Config) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| CoreError::ConfigRead {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| CoreError::Config(e.to_string()))?;
    std::fs::write(path, text).map_err(|source| CoreError::ConfigRead {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = Config {
            theme: Theme::Light,
            density: Density::Compact,
            serif_body: true,
            default_account: Some("a@b.com".into()),
        };
        save(&path, &cfg).unwrap();
        assert_eq!(load(&path).unwrap(), cfg);
    }

    #[test]
    fn missing_config_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = load(&dir.path().join("nope.toml")).unwrap();
        assert_eq!(cfg.theme, Theme::Dark);
        assert_eq!(cfg.density, Density::Comfortable);
    }
}
