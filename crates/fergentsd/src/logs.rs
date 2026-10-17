//! Log retention: prune rotated log files under `<data_dir>/logs/`.
//!
//! Retention is driven by the `[log]` section of the global `config.toml`
//! (see [`crate::config::LogConfig`]). Pruning runs on startup and then once
//! per day while the daemon is alive (`BRAIN.md`-style housekeeping; Phase 0
//! gap #1 in the Phase 1 spec, Appendix F).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::config::LogConfig;

/// Seconds in a day, used for the retention cutoff and the daily timer.
const DAY_SECS: u64 = 86_400;

/// Delete log files in `dir` that are older than `retention_days`, then keep
/// at most `max_files` of the newest remaining files. Returns the number of
/// files removed. A missing directory is not an error.
pub fn prune_logs(dir: &Path, retention_days: u64, max_files: usize) -> std::io::Result<usize> {
    prune_logs_at(dir, retention_days, max_files, SystemTime::now())
}

/// [`prune_logs`] with an injected "now", for deterministic tests.
fn prune_logs_at(
    dir: &Path,
    retention_days: u64,
    max_files: usize,
    now: SystemTime,
) -> std::io::Result<usize> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => return Err(err),
    };

    let mut files: Vec<(PathBuf, SystemTime)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            files.push((entry.path(), meta.modified().unwrap_or(now)));
        }
    }

    // Newest first, so the cap keeps the most recent files and the currently
    // open log file (always the newest) is never removed.
    files.sort_by_key(|entry| std::cmp::Reverse(entry.1));

    // `checked_sub` returns `None` only for an impractically large
    // `retention_days`; in that case nothing is considered too old.
    let cutoff = now.checked_sub(Duration::from_secs(retention_days.saturating_mul(DAY_SECS)));

    let mut removed = 0;
    for (index, (path, modified)) in files.iter().enumerate() {
        let too_old = cutoff.is_some_and(|cutoff| *modified < cutoff);
        let over_cap = max_files > 0 && index >= max_files;
        if too_old || over_cap {
            std::fs::remove_file(path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// Prune once at startup, logging the outcome but never failing the daemon.
pub fn prune_on_startup(dir: &Path, config: &LogConfig) {
    match prune_logs(dir, config.retention_days, config.max_files) {
        Ok(0) => {}
        Ok(removed) => tracing::info!(removed, "pruned old log files"),
        Err(err) => tracing::warn!(error = %err, "log pruning failed"),
    }
}

/// Prune every day until `shutdown` flips to `true`.
pub async fn run_daily_prune(
    dir: PathBuf,
    config: LogConfig,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        if *shutdown.borrow() {
            return;
        }
        tokio::select! {
            _ = shutdown.changed() => return,
            _ = tokio::time::sleep(Duration::from_secs(DAY_SECS)) => {
                prune_on_startup(&dir, &config);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"x").unwrap();
    }

    fn file_count(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn removes_files_older_than_retention() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "fergentsd.log.2020-01-01");
        touch(dir.path(), "fergentsd.log.2020-01-02");

        // Pretend "now" is 8 days after the files were written.
        let now = SystemTime::now() + Duration::from_secs(8 * DAY_SECS);
        let removed = prune_logs_at(dir.path(), 7, 30, now).unwrap();

        assert_eq!(removed, 2);
        assert_eq!(file_count(dir.path()), 0);
    }

    #[test]
    fn keeps_files_within_retention() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "fergentsd.log.today");

        let removed = prune_logs_at(dir.path(), 7, 30, SystemTime::now()).unwrap();

        assert_eq!(removed, 0);
        assert_eq!(file_count(dir.path()), 1);
    }

    #[test]
    fn caps_file_count_keeping_newest() {
        let dir = tempfile::tempdir().unwrap();
        for day in 1..=3 {
            touch(dir.path(), &format!("fergentsd.log.2020-01-0{day}"));
        }

        // A large retention keeps every file age-wise; the cap removes one.
        let removed = prune_logs_at(dir.path(), 10_000, 2, SystemTime::now()).unwrap();

        assert_eq!(removed, 1);
        assert_eq!(file_count(dir.path()), 2);
    }

    #[test]
    fn zero_max_files_disables_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        for day in 1..=3 {
            touch(dir.path(), &format!("fergentsd.log.2020-01-0{day}"));
        }

        let removed = prune_logs_at(dir.path(), 10_000, 0, SystemTime::now()).unwrap();

        assert_eq!(removed, 0);
        assert_eq!(file_count(dir.path()), 3);
    }

    #[test]
    fn missing_directory_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("logs");
        assert_eq!(prune_logs(&missing, 7, 30).unwrap(), 0);
    }

    #[test]
    fn ignores_subdirectories() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("archive")).unwrap();
        touch(dir.path(), "fergentsd.log.today");

        let removed = prune_logs_at(dir.path(), 10_000, 0, SystemTime::now()).unwrap();

        assert_eq!(removed, 0);
    }
}
