//! The persisted `index_jobs` queue (`SEARCH.md` §13.2, §13.4; spec Appendix B).
//!
//! Jobs are durable: the queue survives restarts, so pending work resumes on
//! the next start. Jobs are ordered by `priority` (lower runs first) then
//! `enqueued_at`, so user-visible work can jump the line.

use std::sync::Arc;

use fergents_storage::{Storage, Value};

use crate::error::IndexError;

/// The operation a job performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexJobOp {
    /// Index (or re-index) a file.
    Upsert,
    /// Soft-delete a document.
    Delete,
    /// Force a full re-index of a file.
    Reindex,
}

impl IndexJobOp {
    /// The `index_jobs.op` string.
    pub fn as_str(self) -> &'static str {
        match self {
            IndexJobOp::Upsert => "upsert",
            IndexJobOp::Delete => "delete",
            IndexJobOp::Reindex => "reindex",
        }
    }

    /// Parse an `index_jobs.op` string.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "upsert" => Some(IndexJobOp::Upsert),
            "delete" => Some(IndexJobOp::Delete),
            "reindex" => Some(IndexJobOp::Reindex),
            _ => None,
        }
    }
}

/// A persisted indexing job (`index_jobs` row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexJob {
    /// ULID primary key.
    pub id: String,
    /// Vault-relative path.
    pub path: String,
    /// The operation.
    pub op: IndexJobOp,
    /// Lower runs first.
    pub priority: i64,
    /// Enqueue time (ms since the Unix epoch).
    pub enqueued_at: i64,
    /// Retry attempts so far.
    pub attempts: i64,
}

/// Counts for `index.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueStats {
    /// Jobs waiting in the queue.
    pub queued: usize,
    /// Total attempts across queued jobs.
    pub attempts: i64,
}

/// The persisted job queue.
#[derive(Clone)]
pub struct IndexQueue {
    storage: Arc<dyn Storage>,
}

impl IndexQueue {
    /// Create a queue over the per-brain store.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Enqueue a job, returning the persisted row.
    pub fn enqueue(
        &self,
        path: &str,
        op: IndexJobOp,
        priority: i64,
    ) -> Result<IndexJob, IndexError> {
        let job = IndexJob {
            id: ulid::Ulid::new().to_string(),
            path: path.to_string(),
            op,
            priority,
            enqueued_at: now_ms(),
            attempts: 0,
        };
        self.storage.execute(
            "INSERT INTO index_jobs (id, path, op, priority, enqueued_at, attempts) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[
                Value::String(job.id.clone()),
                Value::String(job.path.clone()),
                Value::String(job.op.as_str().to_string()),
                Value::from(job.priority),
                Value::from(job.enqueued_at),
                Value::from(job.attempts),
            ],
        )?;
        Ok(job)
    }

    /// All pending jobs, highest priority first.
    pub fn pending(&self) -> Result<Vec<IndexJob>, IndexError> {
        let rows = self.storage.query(
            "SELECT id, path, op, priority, enqueued_at, attempts FROM index_jobs \
             ORDER BY priority ASC, enqueued_at ASC, id ASC",
            &[],
        )?;
        Ok(rows.iter().filter_map(row_to_job).collect())
    }

    /// The pending job with the highest priority, if any.
    pub fn peek(&self) -> Result<Option<IndexJob>, IndexError> {
        Ok(self.pending()?.into_iter().next())
    }

    /// Remove a job (called once it has been processed).
    pub fn remove(&self, id: &str) -> Result<(), IndexError> {
        self.storage.execute(
            "DELETE FROM index_jobs WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        Ok(())
    }

    /// Increment a job's attempt counter (kept for retry/telemetry).
    pub fn record_attempt(&self, id: &str) -> Result<(), IndexError> {
        self.storage.execute(
            "UPDATE index_jobs SET attempts = attempts + 1 WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        Ok(())
    }

    /// Queue counts for `index.status`.
    pub fn stats(&self) -> Result<QueueStats, IndexError> {
        let rows = self.storage.query(
            "SELECT count(*) AS queued, COALESCE(sum(attempts), 0) AS attempts FROM index_jobs",
            &[],
        )?;
        let row = rows.first();
        Ok(QueueStats {
            queued: row
                .and_then(|r| r.get("queued"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
            attempts: row
                .and_then(|r| r.get("attempts"))
                .and_then(Value::as_i64)
                .unwrap_or(0),
        })
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> Result<bool, IndexError> {
        Ok(self.stats()?.queued == 0)
    }
}

fn row_to_job(row: &fergents_storage::Row) -> Option<IndexJob> {
    let id = row.get("id").and_then(Value::as_str)?.to_string();
    Some(IndexJob {
        id,
        path: row
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        op: row
            .get("op")
            .and_then(Value::as_str)
            .and_then(IndexJobOp::parse)
            .unwrap_or(IndexJobOp::Upsert),
        priority: row.get("priority").and_then(Value::as_i64).unwrap_or(0),
        enqueued_at: row.get("enqueued_at").and_then(Value::as_i64).unwrap_or(0),
        attempts: row.get("attempts").and_then(Value::as_i64).unwrap_or(0),
    })
}

/// Wall-clock time in ms since the Unix epoch.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}
