//! The trigger registry: one row per `(workflow_id, trigger_id)` in
//! `workflow_triggers` (spec §2.2, migration `0018_workflow.sql`).
//!
//! The registry is deliberately a thin layer over [`Storage`] rather than a
//! method on [`crate::RunStore`]: triggers outlive runs, and the scheduler must
//! be able to read them without touching run history.

use std::sync::Arc;

use fergents_storage::{Row, Storage, Value};

use crate::document::WorkflowDocument;
use crate::error::WorkflowError;

/// A persisted trigger row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerRecord {
    /// The owning workflow.
    pub workflow_id: String,
    /// The trigger id within the document.
    pub trigger_id: String,
    /// `manual`, `ai`, `cron`, `webhook`, `file`, or `event`.
    pub kind: String,
    /// Whether the trigger may fire.
    pub enabled: bool,
    /// When the trigger last started a run.
    pub last_fired_at: Option<i64>,
    /// The next scheduled fire (cron only).
    pub next_fire_at: Option<i64>,
    /// The last failure, kept so a broken trigger stays visible.
    pub last_error: Option<String>,
}

/// Reads and writes `workflow_triggers`.
pub struct TriggerRegistry {
    storage: Arc<dyn Storage>,
}

impl TriggerRegistry {
    /// Wrap `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Reconcile the registry with `doc`'s declared triggers.
    ///
    /// Declared triggers are inserted or refreshed; rows for triggers the
    /// document no longer declares are removed. `enabled`, `last_fired_at`,
    /// and `next_fire_at` survive a refresh so an edit does not replay fires.
    pub fn sync(&self, doc: &WorkflowDocument) -> Result<Vec<TriggerRecord>, WorkflowError> {
        for trigger in &doc.triggers {
            self.storage.execute(
                "INSERT INTO workflow_triggers (workflow_id, trigger_id, kind, enabled) \
                 VALUES (?1, ?2, ?3, 1) \
                 ON CONFLICT(workflow_id, trigger_id) DO UPDATE SET kind = excluded.kind",
                &[
                    Value::String(doc.id.clone()),
                    Value::String(trigger.id.clone()),
                    Value::String(trigger.kind.as_str().to_string()),
                ],
            )?;
        }

        let declared: Vec<String> = doc.triggers.iter().map(|t| t.id.clone()).collect();
        let existing = self.list(&doc.id)?;
        for row in existing {
            if !declared.contains(&row.trigger_id) {
                self.storage.execute(
                    "DELETE FROM workflow_triggers WHERE workflow_id = ?1 AND trigger_id = ?2",
                    &[Value::String(doc.id.clone()), Value::String(row.trigger_id)],
                )?;
            }
        }
        self.list(&doc.id)
    }

    /// Insert a row if it is missing, then return it.
    pub fn ensure(
        &self,
        workflow_id: &str,
        trigger_id: &str,
        kind: &str,
    ) -> Result<TriggerRecord, WorkflowError> {
        self.storage.execute(
            "INSERT OR IGNORE INTO workflow_triggers (workflow_id, trigger_id, kind, enabled) \
             VALUES (?1, ?2, ?3, 1)",
            &[
                Value::String(workflow_id.to_string()),
                Value::String(trigger_id.to_string()),
                Value::String(kind.to_string()),
            ],
        )?;
        self.get(workflow_id, trigger_id)?
            .ok_or_else(|| WorkflowError::NotFound(format!("{workflow_id}/{trigger_id}")))
    }

    /// Fetch one trigger.
    pub fn get(
        &self,
        workflow_id: &str,
        trigger_id: &str,
    ) -> Result<Option<TriggerRecord>, WorkflowError> {
        let rows = self.storage.query(
            "SELECT * FROM workflow_triggers WHERE workflow_id = ?1 AND trigger_id = ?2",
            &[
                Value::String(workflow_id.to_string()),
                Value::String(trigger_id.to_string()),
            ],
        )?;
        rows.first().map(record_from_row).transpose()
    }

    /// Every trigger of one workflow, ordered by trigger id.
    pub fn list(&self, workflow_id: &str) -> Result<Vec<TriggerRecord>, WorkflowError> {
        let rows = self.storage.query(
            "SELECT * FROM workflow_triggers WHERE workflow_id = ?1 ORDER BY trigger_id",
            &[Value::String(workflow_id.to_string())],
        )?;
        rows.iter().map(record_from_row).collect()
    }

    /// Every enabled trigger, ordered by workflow then trigger id.
    pub fn enabled(&self) -> Result<Vec<TriggerRecord>, WorkflowError> {
        let rows = self.storage.query(
            "SELECT * FROM workflow_triggers WHERE enabled = 1 \
             ORDER BY workflow_id, trigger_id",
            &[],
        )?;
        rows.iter().map(record_from_row).collect()
    }

    /// Enable or disable a trigger without unregistering it.
    pub fn set_enabled(
        &self,
        workflow_id: &str,
        trigger_id: &str,
        enabled: bool,
    ) -> Result<(), WorkflowError> {
        self.storage.execute(
            "UPDATE workflow_triggers SET enabled = ?1 WHERE workflow_id = ?2 AND trigger_id = ?3",
            &[
                Value::Bool(enabled),
                Value::String(workflow_id.to_string()),
                Value::String(trigger_id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Persist the computed next fire (cron triggers).
    pub fn set_next_fire(
        &self,
        workflow_id: &str,
        trigger_id: &str,
        next_fire_at: Option<i64>,
    ) -> Result<(), WorkflowError> {
        self.storage.execute(
            "UPDATE workflow_triggers SET next_fire_at = ?1 \
             WHERE workflow_id = ?2 AND trigger_id = ?3",
            &[
                next_fire_at.map_or(Value::Null, Value::from),
                Value::String(workflow_id.to_string()),
                Value::String(trigger_id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Record a successful fire and clear the previous error.
    pub fn mark_fired(
        &self,
        workflow_id: &str,
        trigger_id: &str,
        at: i64,
    ) -> Result<(), WorkflowError> {
        self.storage.execute(
            "UPDATE workflow_triggers SET last_fired_at = ?1, last_error = NULL \
             WHERE workflow_id = ?2 AND trigger_id = ?3",
            &[
                Value::from(at),
                Value::String(workflow_id.to_string()),
                Value::String(trigger_id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Record a failure. The trigger stays armed: the row is untouched apart
    /// from `last_error` (spec §2.2, task 5.3 step 1).
    pub fn mark_error(
        &self,
        workflow_id: &str,
        trigger_id: &str,
        error: &str,
    ) -> Result<(), WorkflowError> {
        self.storage.execute(
            "UPDATE workflow_triggers SET last_error = ?1 \
             WHERE workflow_id = ?2 AND trigger_id = ?3",
            &[
                Value::String(error.to_string()),
                Value::String(workflow_id.to_string()),
                Value::String(trigger_id.to_string()),
            ],
        )?;
        Ok(())
    }
}

fn record_from_row(row: &Row) -> Result<TriggerRecord, WorkflowError> {
    Ok(TriggerRecord {
        workflow_id: text(row, "workflow_id"),
        trigger_id: text(row, "trigger_id"),
        kind: text(row, "kind"),
        enabled: row.get("enabled").and_then(Value::as_i64).unwrap_or(1) != 0,
        last_fired_at: row.get("last_fired_at").and_then(Value::as_i64),
        next_fire_at: row.get("next_fire_at").and_then(Value::as_i64),
        last_error: row
            .get("last_error")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn text(row: &Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
