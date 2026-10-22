//! File snapshots, restores, and the audit trail that ties them together.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_events::{now_ms, EventBus};
use fergents_storage::{AuditEntry, Storage, Value};
use serde::{Deserialize, Serialize};

use crate::error::ReversibilityError;

/// The run/session context a backup belongs to.
#[derive(Debug, Clone, Default)]
pub struct BackupContext {
    /// The workflow/chat run that caused the mutation.
    pub run_id: Option<String>,
    /// The session that caused the mutation.
    pub session_id: Option<String>,
    /// The acting agent.
    pub agent_id: Option<String>,
}

impl BackupContext {
    /// An empty context (a user-initiated mutation).
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a run.
    pub fn for_run(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// Attach a session.
    pub fn in_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Attach an agent.
    pub fn by_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }
}

/// A recorded backup of one file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Backup {
    /// The backup id.
    pub id: String,
    /// The original file path.
    pub path: String,
    /// The copy under `<data_dir>/backups/`.
    pub backup_path: String,
    /// Whether the file existed when the backup was taken (an absent file is
    /// restored by deleting it).
    pub existed: bool,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
    /// The run that caused it, when any.
    pub run_id: Option<String>,
    /// The session that caused it, when any.
    pub session_id: Option<String>,
}

/// Copies files aside before mutation and restores them on request.
pub struct ReversibilityService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    root: PathBuf,
}

impl ReversibilityService {
    /// Create the service, writing backups under `root`.
    pub fn new(storage: Arc<dyn Storage>, bus: Arc<EventBus>, root: impl Into<PathBuf>) -> Self {
        Self {
            storage,
            bus,
            root: root.into(),
        }
    }

    /// The backup directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Snapshot `path` before it is mutated. A missing file is recorded as
    /// `existed: false`, so restoring removes it again.
    pub fn snapshot(
        &self,
        path: impl AsRef<Path>,
        context: &BackupContext,
    ) -> Result<Backup, ReversibilityError> {
        let path = path.as_ref();
        let id = ulid::Ulid::new().to_string();
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".to_string());
        let backup_dir = self.root.join(&id);
        std::fs::create_dir_all(&backup_dir)?;
        let backup_path = backup_dir.join(&file_name);

        let existed = path.exists();
        if existed {
            std::fs::copy(path, &backup_path)?;
        }

        let backup = Backup {
            id: id.clone(),
            path: path.to_string_lossy().into_owned(),
            backup_path: backup_path.to_string_lossy().into_owned(),
            existed,
            created_at: now_ms(),
            run_id: context.run_id.clone(),
            session_id: context.session_id.clone(),
        };

        let args = serde_json::json!({
            "path": backup.path,
            "backup_path": backup.backup_path,
            "existed": existed,
        });
        self.audit(
            &id,
            &context.session_id,
            "reversibility.snapshot",
            &backup.path,
            &args,
            "snapshot",
            true,
            Some(&backup.backup_path),
            context,
        )?;
        self.bus.publish(
            "reversibility.snapshot",
            "reversibility",
            serde_json::json!({
                "backup_id": id,
                "path": backup.path,
                "run_id": context.run_id,
                "session_id": context.session_id,
            }),
        )?;
        Ok(backup)
    }

    /// Restore a backup by id, returning the restored path.
    pub fn restore(&self, backup_id: &str) -> Result<String, ReversibilityError> {
        let rows = self.storage.query(
            "SELECT * FROM audit_log WHERE action = 'reversibility.snapshot' AND revert_ref LIKE ?1 \
             ORDER BY rowid DESC LIMIT 1",
            &[Value::String(format!("%{backup_id}%"))],
        )?;
        let row = rows
            .first()
            .ok_or_else(|| ReversibilityError::UnknownBackup(backup_id.to_string()))?;
        let target = row
            .get("target")
            .and_then(Value::as_str)
            .ok_or_else(|| ReversibilityError::UnknownBackup(backup_id.to_string()))?
            .to_string();
        let backup_path = row
            .get("revert_ref")
            .and_then(Value::as_str)
            .ok_or_else(|| ReversibilityError::UnknownBackup(backup_id.to_string()))?
            .to_string();
        let existed = row
            .get("args_json")
            .and_then(Value::as_str)
            .and_then(|args| serde_json::from_str::<serde_json::Value>(args).ok())
            .and_then(|args| args.get("existed").and_then(Value::as_bool))
            .unwrap_or(true);

        if existed {
            std::fs::copy(&backup_path, &target)?;
        } else if Path::new(&target).exists() {
            std::fs::remove_file(&target)?;
        }

        let args = serde_json::json!({ "backup_id": backup_id });
        let session_id = row
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let audit_id = ulid::Ulid::new().to_string();
        let context = BackupContext {
            session_id: session_id.clone(),
            ..BackupContext::default()
        };
        self.audit(
            &audit_id,
            &session_id,
            "reversibility.restore",
            &target,
            &args,
            "restored",
            false,
            None,
            &context,
        )?;
        self.bus.publish(
            "reversibility.restored",
            "reversibility",
            serde_json::json!({ "backup_id": backup_id, "path": target }),
        )?;
        Ok(target)
    }

    /// Restore every file mutated by `run_id`, newest first.
    pub fn restore_run(&self, run_id: &str) -> Result<Vec<String>, ReversibilityError> {
        let rows = self.storage.query(
            "SELECT * FROM audit_log WHERE action = 'reversibility.snapshot' \
             AND correlation_id = ?1 ORDER BY rowid DESC",
            &[Value::String(run_id.to_string())],
        )?;
        let mut restored = Vec::new();
        for row in &rows {
            let Some(revert_ref) = row.get("revert_ref").and_then(Value::as_str) else {
                continue;
            };
            let target = row
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let existed = row
                .get("args_json")
                .and_then(Value::as_str)
                .and_then(|args| serde_json::from_str::<serde_json::Value>(args).ok())
                .and_then(|args| args.get("existed").and_then(Value::as_bool))
                .unwrap_or(true);
            if existed {
                std::fs::copy(revert_ref, &target)?;
            } else if Path::new(&target).exists() {
                std::fs::remove_file(&target)?;
            }
            restored.push(target);
        }
        Ok(restored)
    }

    /// The recorded backups of a run, newest first.
    pub fn backups_for_run(&self, run_id: &str) -> Result<Vec<Backup>, ReversibilityError> {
        let rows = self.storage.query(
            "SELECT * FROM audit_log WHERE action = 'reversibility.snapshot' \
             AND correlation_id = ?1 ORDER BY rowid DESC",
            &[Value::String(run_id.to_string())],
        )?;
        Ok(rows.iter().map(backup_from_row).collect())
    }

    /// All recorded backups, newest first.
    pub fn list(&self, limit: usize) -> Result<Vec<Backup>, ReversibilityError> {
        let rows = self.storage.query(
            "SELECT * FROM audit_log WHERE action = 'reversibility.snapshot' \
             ORDER BY rowid DESC LIMIT ?1",
            &[Value::from(limit as i64)],
        )?;
        Ok(rows.iter().map(backup_from_row).collect())
    }

    #[allow(clippy::too_many_arguments)]
    fn audit(
        &self,
        id: &str,
        session_id: &Option<String>,
        action: &str,
        target: &str,
        args: &serde_json::Value,
        result: &str,
        reversible: bool,
        revert_ref: Option<&str>,
        context: &BackupContext,
    ) -> Result<(), ReversibilityError> {
        let args_text = args.to_string();
        let (actor_type, actor_id) = match &context.agent_id {
            Some(agent_id) => ("agent", agent_id.as_str()),
            None => ("user", "user"),
        };
        let entry = AuditEntry {
            id,
            ts: now_ms(),
            actor_type,
            actor_id,
            session_id: session_id.as_deref(),
            action,
            target: Some(target),
            args_json: &args_text,
            result,
            reason: None,
            correlation_id: context.run_id.as_deref(),
            reversible,
            revert_ref,
        };
        self.storage.audit(&entry)?;
        Ok(())
    }
}

fn backup_from_row(row: &fergents_storage::Row) -> Backup {
    let args = row
        .get("args_json")
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .unwrap_or(serde_json::Value::Null);
    Backup {
        id: row
            .get("revert_ref")
            .and_then(Value::as_str)
            .and_then(|path| Path::new(path).parent())
            .and_then(|parent| parent.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        path: str_col(row, "target"),
        backup_path: str_col(row, "revert_ref"),
        existed: args.get("existed").and_then(Value::as_bool).unwrap_or(true),
        created_at: row.get("ts").and_then(Value::as_i64).unwrap_or(0),
        run_id: row
            .get("correlation_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        session_id: row
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn str_col(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
