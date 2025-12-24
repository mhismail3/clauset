//! Server configuration.

use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_static_dir")]
    pub static_dir: PathBuf,
    #[serde(default = "default_claude_path")]
    pub claude_path: PathBuf,
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
    #[serde(default = "default_max_sessions")]
    pub max_concurrent_sessions: usize,
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default = "default_projects_root")]
    pub projects_root: PathBuf,
}

fn default_projects_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Downloads/projects")
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_static_dir() -> PathBuf {
    PathBuf::from("./frontend/dist")
}

fn default_claude_path() -> PathBuf {
    PathBuf::from("/opt/homebrew/bin/claude")
}

fn default_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("clauset")
        .join("sessions.db")
}

fn default_max_sessions() -> usize {
    10
}

fn default_model() -> String {
    "haiku".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            static_dir: default_static_dir(),
            claude_path: default_claude_path(),
            db_path: default_db_path(),
            max_concurrent_sessions: default_max_sessions(),
            default_model: default_model(),
            projects_root: default_projects_root(),
        }
    }
}

impl Config {
    /// Load config from a specific file path.
    pub fn load_from(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Load config from default location (config/default.toml) or fall back to defaults.
    pub fn load() -> Result<Self> {
        // Try to load from config file
        let config_path = PathBuf::from("config/default.toml");
        if config_path.exists() {
            return Self::load_from(&config_path);
        }

        // Fall back to defaults
        Ok(Config::default())
    }
}
