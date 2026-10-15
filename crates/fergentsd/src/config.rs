//! Global kernel configuration (`<data_dir>/config.toml`).
//!
//! Phase 0 shipped no configuration file; this module introduces the global
//! `config.toml` described in `ARCHITECTURE.md` §7.1 and `AI_WORKFLOW.md`.
//! Only the `[log]` section is defined so far; unknown sections are preserved
//! by the parser's forward-compatible defaults.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The global configuration, read from `<data_dir>/config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct GlobalConfig {
    /// Log rotation and retention settings (`[log]`).
    pub log: LogConfig,
}

/// Log retention settings (`[log]`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct LogConfig {
    /// Delete rotated log files older than this many days.
    pub retention_days: u64,
    /// Keep at most this many log files, newest first (`0` disables the cap).
    pub max_files: usize,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            retention_days: 7,
            max_files: 30,
        }
    }
}

impl GlobalConfig {
    /// The configuration file path inside `data_dir`.
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("config.toml")
    }

    /// Load the configuration, writing a default file when none exists.
    pub fn load(data_dir: &Path) -> anyhow::Result<Self> {
        let path = Self::path(data_dir);
        if !path.exists() {
            let config = Self::default();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, toml::to_string_pretty(&config)?)?;
            return Ok(config);
        }
        let raw = std::fs::read_to_string(&path)?;
        toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("invalid config at {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_default_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let config = GlobalConfig::load(dir.path()).unwrap();
        assert_eq!(config.log.retention_days, 7);
        assert_eq!(config.log.max_files, 30);

        let raw = std::fs::read_to_string(GlobalConfig::path(dir.path())).unwrap();
        assert!(raw.contains("retention_days"));
        assert!(raw.contains("max_files"));
    }

    #[test]
    fn round_trips_explicit_values() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            GlobalConfig::path(dir.path()),
            "[log]\nretention_days = 3\nmax_files = 5\n",
        )
        .unwrap();
        let config = GlobalConfig::load(dir.path()).unwrap();
        assert_eq!(config.log.retention_days, 3);
        assert_eq!(config.log.max_files, 5);
    }

    #[test]
    fn partial_log_section_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            GlobalConfig::path(dir.path()),
            "[log]\nretention_days = 1\n",
        )
        .unwrap();
        let config = GlobalConfig::load(dir.path()).unwrap();
        assert_eq!(config.log.retention_days, 1);
        assert_eq!(config.log.max_files, 30);
    }

    #[test]
    fn invalid_config_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(GlobalConfig::path(dir.path()), "not = = toml").unwrap();
        assert!(GlobalConfig::load(dir.path()).is_err());
    }
}
