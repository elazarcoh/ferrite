pub mod schema;
pub mod watcher;

use anyhow::{Context, Result};
use std::path::PathBuf;

pub use schema::Config;

/// Returns `%LOCALAPPDATA%\my-pet\config.toml`.
pub fn config_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("my-pet").join("config.toml")
}

/// Load config from disk, or return `Config::default()` if not found.
pub fn load(path: &std::path::Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(path).context("read config")?;
    toml::from_str(&text).context("parse config TOML")
}

/// Persist config to disk, creating parent directories as needed.
pub fn save(path: &std::path::Path, config: &Config) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context("create config dir")?;
    }
    let text = toml::to_string_pretty(config).context("serialize config")?;
    std::fs::write(path, text).context("write config")
}
