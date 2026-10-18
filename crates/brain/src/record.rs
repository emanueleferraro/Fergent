//! The brain registry record: one row of the global `brains` table
//! (`BRAIN.md` §9; `migrations/global/0002_brains_lifecycle.sql`).

use std::path::PathBuf;

use fergents_storage::{Row, Value};

use crate::error::BrainError;
use crate::roles::BrainRole;

/// A registered brain, as stored in the global `brains` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrainRecord {
    /// Stable brain id (`brain.toml` `[brain] id`).
    pub brain_id: String,
    /// Human-readable name.
    pub name: String,
    /// Absolute path to the brain folder.
    pub path: PathBuf,
    /// The brain's role in its project, when attached.
    pub role: Option<BrainRole>,
    /// Highest per-brain schema version applied.
    pub schema_version: i64,
    /// Creation time (ms since the Unix epoch).
    pub created_at: i64,
    /// Last time the brain was opened (ms since the Unix epoch).
    pub last_opened_at: Option<i64>,
    /// When the brain was archived (ms since the Unix epoch); `None` = active.
    pub archived_at: Option<i64>,
}

impl BrainRecord {
    /// Whether the brain is archived.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// Build a record from a `brains` row.
    pub fn from_row(row: &Row) -> Result<Self, BrainError> {
        let get_str = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
        let brain_id = get_str("brain_id")
            .ok_or_else(|| BrainError::invalid("brains row is missing brain_id"))?;
        let name =
            get_str("name").ok_or_else(|| BrainError::invalid("brains row is missing name"))?;
        let path =
            get_str("path").ok_or_else(|| BrainError::invalid("brains row is missing path"))?;
        Ok(Self {
            brain_id,
            name,
            path: PathBuf::from(path),
            role: get_str("role").as_deref().and_then(BrainRole::parse_opt),
            schema_version: row
                .get("schema_version")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
            last_opened_at: row.get("last_opened_at").and_then(Value::as_i64),
            archived_at: row.get("archived_at").and_then(Value::as_i64),
        })
    }
}
