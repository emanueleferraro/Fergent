//! Persistence for workflow runs, steps, and checkpoints (spec §3.2, §3.3).
//!
//! The store is a thin, typed layer over [`fergents_storage::Storage`]: every
//! read returns a record, never a raw row, and every write is a single
//! statement. YAML remains the source of truth for workflow definitions; this
//! module only records what happened at run time.

use std::sync::Arc;

use fergents_events::now_ms;
use fergents_storage::{Row, Storage, Value};

use crate::error::WorkflowError;

/// How many checkpoints are kept per run (spec §3.3).
pub const CHECKPOINT_KEEP: i64 = 50;

/// The lifecycle states of a run (spec §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// Accepted, not yet started.
    Queued,
    /// Executing nodes.
    Running,
    /// Paused by the user or by a budget.
    Paused,
    /// Waiting on a `human` node decision.
    WaitingHuman,
    /// A crash interrupted the run; it awaits a resume decision.
    Interrupted,
    /// Finished successfully.
    Done,
    /// Finished with an error.
    Failed,
    /// Cancelled by the user, by a timeout, or by the kernel.
    Cancelled,
}

impl RunState {
    /// Every state, in lifecycle order.
    pub const ALL: [RunState; 8] = [
        RunState::Queued,
        RunState::Running,
        RunState::Paused,
        RunState::WaitingHuman,
        RunState::Interrupted,
        RunState::Done,
        RunState::Failed,
        RunState::Cancelled,
    ];

    /// The stable wire string.
    pub fn as_str(&self) -> &'static str {
        match self {
            RunState::Queued => "queued",
            RunState::Running => "running",
            RunState::Paused => "paused",
            RunState::WaitingHuman => "waiting_human",
            RunState::Interrupted => "interrupted",
            RunState::Done => "done",
            RunState::Failed => "failed",
            RunState::Cancelled => "cancelled",
        }
    }

    /// Parse a wire string. Unknown values read as [`RunState::Queued`].
    pub fn parse(value: &str) -> RunState {
        match value {
            "running" => RunState::Running,
            "paused" => RunState::Paused,
            "waiting_human" => RunState::WaitingHuman,
            "interrupted" => RunState::Interrupted,
            "done" => RunState::Done,
            "failed" => RunState::Failed,
            "cancelled" => RunState::Cancelled,
            _ => RunState::Queued,
        }
    }

    /// Whether the run has stopped for good.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunState::Done | RunState::Failed | RunState::Cancelled
        )
    }
}

/// The states of a single node attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    /// Claimed but not yet started.
    Pending,
    /// Executing.
    Running,
    /// Finished successfully.
    Done,
    /// Finished with an error.
    Failed,
    /// Not executed because no inbound edge activated.
    Skipped,
    /// A crash interrupted the attempt.
    Interrupted,
    /// Exceeded its `timeout_s`.
    TimedOut,
}

impl StepState {
    /// The stable wire string.
    pub fn as_str(&self) -> &'static str {
        match self {
            StepState::Pending => "pending",
            StepState::Running => "running",
            StepState::Done => "done",
            StepState::Failed => "failed",
            StepState::Skipped => "skipped",
            StepState::Interrupted => "interrupted",
            StepState::TimedOut => "timed_out",
        }
    }

    /// Parse a wire string. Unknown values read as [`StepState::Pending`].
    pub fn parse(value: &str) -> StepState {
        match value {
            "running" => StepState::Running,
            "done" => StepState::Done,
            "failed" => StepState::Failed,
            "skipped" => StepState::Skipped,
            "interrupted" => StepState::Interrupted,
            "timed_out" => StepState::TimedOut,
            _ => StepState::Pending,
        }
    }

    /// Whether the attempt has stopped.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            StepState::Done
                | StepState::Failed
                | StepState::Skipped
                | StepState::Interrupted
                | StepState::TimedOut
        )
    }
}

/// A persisted run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRecord {
    /// The run id (a ULID).
    pub id: String,
    /// The workflow id.
    pub workflow_id: String,
    /// The pinned workflow version.
    pub workflow_version: i64,
    /// The parent run, for sub-workflows.
    pub parent_run_id: Option<String>,
    /// `manual`, `cron`, `webhook`, `file`, `event`, or `ai`.
    pub trigger_kind: String,
    /// The trigger that started the run, if any.
    pub trigger_id: Option<String>,
    /// The lifecycle state.
    pub state: RunState,
    /// `user`, `agent:<id>`, or `trigger:<id>`.
    pub actor: String,
    /// Whether side effects were suppressed.
    pub dry_run: bool,
    /// The run inputs.
    pub inputs: Option<Value>,
    /// The latest context snapshot.
    pub context: Option<Value>,
    /// The error that failed or paused the run.
    pub error: Option<String>,
    /// When the run started executing.
    pub started_at: Option<i64>,
    /// When the run reached a terminal state.
    pub finished_at: Option<i64>,
    /// When the run was accepted.
    pub created_at: i64,
}

/// The fields needed to create a run.
#[derive(Debug, Clone)]
pub struct NewRun<'a> {
    /// The workflow id.
    pub workflow_id: &'a str,
    /// The pinned workflow version.
    pub workflow_version: i64,
    /// The parent run, for sub-workflows.
    pub parent_run_id: Option<&'a str>,
    /// The trigger kind.
    pub trigger_kind: &'a str,
    /// The trigger id, if any.
    pub trigger_id: Option<&'a str>,
    /// `user`, `agent:<id>`, or `trigger:<id>`.
    pub actor: &'a str,
    /// Whether side effects are suppressed.
    pub dry_run: bool,
    /// The run inputs.
    pub inputs: Option<Value>,
}

/// A persisted node attempt.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRecord {
    /// The step id (a ULID).
    pub id: String,
    /// The owning run.
    pub run_id: String,
    /// The node id.
    pub node_id: String,
    /// The loop iteration, when the node runs inside a `loop`.
    pub iteration: Option<i64>,
    /// The attempt state.
    pub state: StepState,
    /// The attempt number, starting at 1.
    pub attempt: u32,
    /// The resolved inputs.
    pub inputs: Option<Value>,
    /// The node outputs.
    pub outputs: Option<Value>,
    /// The error code and message, when the attempt failed.
    pub error: Option<String>,
    /// When the attempt started.
    pub started_at: Option<i64>,
    /// When the attempt finished.
    pub finished_at: Option<i64>,
}

/// A persisted context snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointRecord {
    /// The owning run.
    pub run_id: String,
    /// A monotonically increasing sequence number within the run.
    pub seq: i64,
    /// The node whose completion produced the snapshot.
    pub node_id: Option<String>,
    /// The context snapshot.
    pub context: Value,
    /// When the snapshot was written.
    pub created_at: i64,
}

/// Reads and writes `workflow_runs`, `workflow_steps`, and
/// `workflow_checkpoints`.
pub struct RunStore {
    storage: Arc<dyn Storage>,
}

impl RunStore {
    /// Wrap `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Insert a queued run and return it.
    pub fn create_run(&self, new: NewRun<'_>) -> Result<RunRecord, WorkflowError> {
        let id = ulid::Ulid::new().to_string();
        let now = now_ms();
        self.storage.execute(
            "INSERT INTO workflow_runs \
             (id, workflow_id, workflow_version, parent_run_id, trigger_kind, trigger_id, \
              state, actor, dry_run, inputs, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            &[
                Value::String(id.clone()),
                Value::String(new.workflow_id.to_string()),
                Value::from(new.workflow_version),
                opt_str(new.parent_run_id),
                Value::String(new.trigger_kind.to_string()),
                opt_str(new.trigger_id),
                Value::String(RunState::Queued.as_str().to_string()),
                Value::String(new.actor.to_string()),
                Value::Bool(new.dry_run),
                opt_json(&new.inputs),
                Value::from(now),
            ],
        )?;
        self.get_run(&id)?
            .ok_or_else(|| WorkflowError::NotFound(id))
    }

    /// Move a run to `state`, stamping `started_at` and `finished_at`.
    pub fn transition(
        &self,
        run_id: &str,
        state: RunState,
        error: Option<&str>,
    ) -> Result<RunRecord, WorkflowError> {
        let now = now_ms();
        let wire = state.as_str();
        self.storage.execute(
            "UPDATE workflow_runs SET state = ?1, \
             started_at = CASE WHEN ?1 = 'running' AND started_at IS NULL THEN ?2 ELSE started_at END, \
             finished_at = CASE WHEN ?1 IN ('done', 'failed', 'cancelled') THEN ?2 ELSE finished_at END, \
             error = ?3 WHERE id = ?4",
            &[
                Value::String(wire.to_string()),
                Value::from(now),
                opt_str(error),
                Value::String(run_id.to_string()),
            ],
        )?;
        self.get_run(run_id)?
            .ok_or_else(|| WorkflowError::NotFound(run_id.to_string()))
    }

    /// Replace the run's latest context snapshot.
    pub fn set_context(&self, run_id: &str, context: &Value) -> Result<(), WorkflowError> {
        self.storage.execute(
            "UPDATE workflow_runs SET context = ?1 WHERE id = ?2",
            &[json_param(context), Value::String(run_id.to_string())],
        )?;
        Ok(())
    }

    /// Fetch a run by id.
    pub fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, WorkflowError> {
        let rows = self.storage.query(
            "SELECT * FROM workflow_runs WHERE id = ?1",
            &[Value::String(run_id.to_string())],
        )?;
        rows.first().map(run_from_row).transpose()
    }

    /// List runs, newest first, optionally restricted to one workflow.
    pub fn list_runs(
        &self,
        workflow_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RunRecord>, WorkflowError> {
        let rows = match workflow_id {
            Some(id) => self.storage.query(
                "SELECT * FROM workflow_runs WHERE workflow_id = ?1 \
                 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
                &[Value::String(id.to_string()), Value::from(limit as i64)],
            )?,
            None => self.storage.query(
                "SELECT * FROM workflow_runs ORDER BY created_at DESC, rowid DESC LIMIT ?1",
                &[Value::from(limit as i64)],
            )?,
        };
        rows.iter().map(run_from_row).collect()
    }

    /// Insert a running step and return its id.
    pub fn start_step(
        &self,
        run_id: &str,
        node_id: &str,
        iteration: Option<u32>,
        attempt: u32,
        inputs: Option<&Value>,
    ) -> Result<String, WorkflowError> {
        let id = ulid::Ulid::new().to_string();
        self.storage.execute(
            "INSERT INTO workflow_steps \
             (id, run_id, node_id, iteration, state, attempt, inputs, started_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            &[
                Value::String(id.clone()),
                Value::String(run_id.to_string()),
                Value::String(node_id.to_string()),
                iteration.map_or(Value::Null, Value::from),
                Value::String(StepState::Running.as_str().to_string()),
                Value::from(attempt),
                opt_json(&inputs.cloned()),
                Value::from(now_ms()),
            ],
        )?;
        Ok(id)
    }

    /// Record the outcome of a step.
    pub fn finish_step(
        &self,
        step_id: &str,
        state: StepState,
        outputs: Option<&Value>,
        error: Option<&str>,
    ) -> Result<(), WorkflowError> {
        self.storage.execute(
            "UPDATE workflow_steps SET state = ?1, outputs = ?2, error = ?3, finished_at = ?4 \
             WHERE id = ?5",
            &[
                Value::String(state.as_str().to_string()),
                opt_json(&outputs.cloned()),
                opt_str(error),
                Value::from(now_ms()),
                Value::String(step_id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// The steps of a run, oldest first.
    pub fn steps(&self, run_id: &str) -> Result<Vec<StepRecord>, WorkflowError> {
        let rows = self.storage.query(
            "SELECT * FROM workflow_steps WHERE run_id = ?1 ORDER BY started_at, rowid",
            &[Value::String(run_id.to_string())],
        )?;
        rows.iter().map(step_from_row).collect()
    }

    /// Append a context snapshot, pruning old ones, and return its sequence.
    pub fn write_checkpoint(
        &self,
        run_id: &str,
        node_id: Option<&str>,
        context: &Value,
    ) -> Result<i64, WorkflowError> {
        let seq = self.next_checkpoint_seq(run_id)?;
        self.storage.execute(
            "INSERT INTO workflow_checkpoints (run_id, seq, node_id, context, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            &[
                Value::String(run_id.to_string()),
                Value::from(seq),
                opt_str(node_id),
                json_param(context),
                Value::from(now_ms()),
            ],
        )?;
        self.prune_checkpoints(run_id, CHECKPOINT_KEEP)?;
        Ok(seq)
    }

    /// The newest checkpoint of a run.
    pub fn latest_checkpoint(
        &self,
        run_id: &str,
    ) -> Result<Option<CheckpointRecord>, WorkflowError> {
        let rows = self.storage.query(
            "SELECT * FROM workflow_checkpoints WHERE run_id = ?1 ORDER BY seq DESC LIMIT 1",
            &[Value::String(run_id.to_string())],
        )?;
        rows.first().map(checkpoint_from_row).transpose()
    }

    /// The checkpoints of a run, newest first.
    pub fn checkpoints(
        &self,
        run_id: &str,
        limit: usize,
    ) -> Result<Vec<CheckpointRecord>, WorkflowError> {
        let rows = self.storage.query(
            "SELECT * FROM workflow_checkpoints WHERE run_id = ?1 \
             ORDER BY seq DESC LIMIT ?2",
            &[Value::String(run_id.to_string()), Value::from(limit as i64)],
        )?;
        rows.iter().map(checkpoint_from_row).collect()
    }

    /// Keep only the newest `keep` checkpoints of a run.
    pub fn prune_checkpoints(&self, run_id: &str, keep: i64) -> Result<usize, WorkflowError> {
        let keep = keep.max(1);
        let removed = self.storage.execute(
            "DELETE FROM workflow_checkpoints WHERE run_id = ?1 AND seq <= \
             (SELECT MAX(seq) FROM workflow_checkpoints WHERE run_id = ?1) - ?2",
            &[Value::String(run_id.to_string()), Value::from(keep)],
        )?;
        Ok(removed)
    }

    /// How many checkpoints a run has.
    pub fn checkpoint_count(&self, run_id: &str) -> Result<i64, WorkflowError> {
        let rows = self.storage.query(
            "SELECT COUNT(*) AS n FROM workflow_checkpoints WHERE run_id = ?1",
            &[Value::String(run_id.to_string())],
        )?;
        Ok(rows
            .first()
            .and_then(|row| row.get("n"))
            .and_then(Value::as_i64)
            .unwrap_or(0))
    }

    fn next_checkpoint_seq(&self, run_id: &str) -> Result<i64, WorkflowError> {
        let rows = self.storage.query(
            "SELECT COALESCE(MAX(seq), 0) + 1 AS next FROM workflow_checkpoints WHERE run_id = ?1",
            &[Value::String(run_id.to_string())],
        )?;
        Ok(rows
            .first()
            .and_then(|row| row.get("next"))
            .and_then(Value::as_i64)
            .unwrap_or(1))
    }
}

fn run_from_row(row: &Row) -> Result<RunRecord, WorkflowError> {
    Ok(RunRecord {
        id: str_col(row, "id"),
        workflow_id: str_col(row, "workflow_id"),
        workflow_version: int_col(row, "workflow_version"),
        parent_run_id: opt_str_col(row, "parent_run_id"),
        trigger_kind: str_col(row, "trigger_kind"),
        trigger_id: opt_str_col(row, "trigger_id"),
        state: RunState::parse(&str_col(row, "state")),
        actor: str_col(row, "actor"),
        dry_run: row
            .get("dry_run")
            .and_then(Value::as_i64)
            .is_some_and(|value| value != 0),
        inputs: json_col(row, "inputs")?,
        context: json_col(row, "context")?,
        error: opt_str_col(row, "error"),
        started_at: row.get("started_at").and_then(Value::as_i64),
        finished_at: row.get("finished_at").and_then(Value::as_i64),
        created_at: int_col(row, "created_at"),
    })
}

fn step_from_row(row: &Row) -> Result<StepRecord, WorkflowError> {
    Ok(StepRecord {
        id: str_col(row, "id"),
        run_id: str_col(row, "run_id"),
        node_id: str_col(row, "node_id"),
        iteration: row.get("iteration").and_then(Value::as_i64),
        state: StepState::parse(&str_col(row, "state")),
        attempt: row
            .get("attempt")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .try_into()
            .unwrap_or(1),
        inputs: json_col(row, "inputs")?,
        outputs: json_col(row, "outputs")?,
        error: opt_str_col(row, "error"),
        started_at: row.get("started_at").and_then(Value::as_i64),
        finished_at: row.get("finished_at").and_then(Value::as_i64),
    })
}

fn checkpoint_from_row(row: &Row) -> Result<CheckpointRecord, WorkflowError> {
    Ok(CheckpointRecord {
        run_id: str_col(row, "run_id"),
        seq: int_col(row, "seq"),
        node_id: opt_str_col(row, "node_id"),
        context: json_col(row, "context")?.unwrap_or(Value::Null),
        created_at: int_col(row, "created_at"),
    })
}

fn str_col(row: &Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn opt_str_col(row: &Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_string)
}

fn int_col(row: &Row, key: &str) -> i64 {
    row.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn json_col(row: &Row, key: &str) -> Result<Option<Value>, WorkflowError> {
    match row.get(key).and_then(Value::as_str) {
        Some(raw) => Ok(Some(serde_json::from_str(raw)?)),
        None => Ok(None),
    }
}

fn opt_str(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |text| Value::String(text.to_string()))
}

fn opt_json(value: &Option<Value>) -> Value {
    value.as_ref().map_or(Value::Null, json_param)
}

fn json_param(value: &Value) -> Value {
    Value::String(value.to_string())
}
