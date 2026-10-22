//! Agent memory: working/episodic/semantic tiers over `agent_memory`
//! (`AGENTS.md` §8).
//!
//! Recall here is a deterministic keyword+importance ranking over the store.
//! Embedding-based recall reuses the Phase 1 brain index; that integration is
//! layered on later without changing this surface.

use std::fmt;
use std::sync::Arc;

use fergents_events::now_ms;
use fergents_storage::{Storage, Value};
use serde::{Deserialize, Serialize};

use crate::error::AgentError;

/// The scope a memory entry belongs to (`AGENTS.md` §8.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryScope {
    /// Shared across every agent.
    Global,
    /// Private to one agent.
    Agent(String),
    /// Scoped to a project.
    Project(String),
    /// Scoped to a brain.
    Brain(String),
}

impl MemoryScope {
    /// The persisted string form: `global`, `agent:<id>`, ...
    pub fn key(&self) -> String {
        match self {
            MemoryScope::Global => "global".to_string(),
            MemoryScope::Agent(id) => format!("agent:{id}"),
            MemoryScope::Project(id) => format!("project:{id}"),
            MemoryScope::Brain(id) => format!("brain:{id}"),
        }
    }

    /// Parse the persisted string form.
    pub fn parse(value: &str) -> Self {
        match value.split_once(':') {
            Some(("agent", id)) => MemoryScope::Agent(id.to_string()),
            Some(("project", id)) => MemoryScope::Project(id.to_string()),
            Some(("brain", id)) => MemoryScope::Brain(id.to_string()),
            _ => MemoryScope::Global,
        }
    }

    /// Whether an entry in `other` is visible when recalling from `self`
    /// (global entries are always visible).
    pub fn visible_from(&self, other: &MemoryScope) -> bool {
        other == &MemoryScope::Global || other == self
    }
}

impl fmt::Display for MemoryScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key())
    }
}

impl Serialize for MemoryScope {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> Deserialize<'de> for MemoryScope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(MemoryScope::parse(&value))
    }
}

/// The memory tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// Session history summaries.
    Episodic,
    /// Long-term facts.
    Fact,
}

impl MemoryKind {
    /// The persisted string form.
    pub fn key(self) -> &'static str {
        match self {
            MemoryKind::Episodic => "episodic",
            MemoryKind::Fact => "fact",
        }
    }

    /// Parse the persisted string form.
    pub fn parse(value: &str) -> Self {
        match value {
            "fact" => MemoryKind::Fact,
            _ => MemoryKind::Episodic,
        }
    }
}

/// A stored memory entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// The entry id.
    pub id: String,
    /// The scope.
    pub scope: MemoryScope,
    /// The tier.
    pub kind: MemoryKind,
    /// The content.
    pub content: String,
    /// Importance in `[0, 1]`.
    pub importance: f32,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
    /// Last update time (ms since the epoch).
    pub updated_at: i64,
    /// Last time this entry was recalled.
    pub last_used_at: Option<i64>,
}

/// Reads and writes `agent_memory`.
pub struct MemoryStore {
    storage: Arc<dyn Storage>,
}

impl MemoryStore {
    /// Wrap `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Store a new entry.
    pub fn remember(
        &self,
        scope: &MemoryScope,
        kind: MemoryKind,
        content: &str,
        importance: f32,
    ) -> Result<MemoryEntry, AgentError> {
        let id = ulid::Ulid::new().to_string();
        let now = now_ms();
        let importance = importance.clamp(0.0, 1.0);
        self.storage.execute(
            "INSERT INTO agent_memory \
             (id, scope, kind, content, importance, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            &[
                Value::String(id.clone()),
                Value::String(scope.key()),
                Value::String(kind.key().to_string()),
                Value::String(content.to_string()),
                Value::from(f64::from(importance)),
                Value::from(now),
            ],
        )?;
        Ok(MemoryEntry {
            id,
            scope: scope.clone(),
            kind,
            content: content.to_string(),
            importance,
            created_at: now,
            updated_at: now,
            last_used_at: None,
        })
    }

    /// Recall the `k` most relevant entries visible from `scope`, reinforcing
    /// the ones returned.
    pub fn recall(
        &self,
        scope: &MemoryScope,
        query: &str,
        k: usize,
    ) -> Result<Vec<MemoryEntry>, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM agent_memory \
             WHERE (scope = ?1 OR scope = 'global') AND content LIKE ?2 \
             ORDER BY importance DESC, updated_at DESC LIMIT ?3",
            &[
                Value::String(scope.key()),
                Value::String(format!("%{query}%")),
                Value::from(k as i64),
            ],
        )?;
        let entries: Vec<MemoryEntry> = rows.iter().map(entry_from_row).collect();
        for entry in &entries {
            self.reinforce(&entry.id)?;
        }
        Ok(entries)
    }

    /// List entries in exactly `scope`, optionally filtered by tier.
    pub fn list(
        &self,
        scope: &MemoryScope,
        kind: Option<MemoryKind>,
    ) -> Result<Vec<MemoryEntry>, AgentError> {
        let rows = match kind {
            Some(kind) => self.storage.query(
                "SELECT * FROM agent_memory WHERE scope = ?1 AND kind = ?2 \
                 ORDER BY importance DESC, updated_at DESC",
                &[
                    Value::String(scope.key()),
                    Value::String(kind.key().to_string()),
                ],
            )?,
            None => self.storage.query(
                "SELECT * FROM agent_memory WHERE scope = ?1 \
                 ORDER BY importance DESC, updated_at DESC",
                &[Value::String(scope.key())],
            )?,
        };
        Ok(rows.iter().map(entry_from_row).collect())
    }

    /// Remove an entry. Returns whether it existed.
    pub fn forget(&self, id: &str) -> Result<bool, AgentError> {
        let affected = self.storage.execute(
            "DELETE FROM agent_memory WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        Ok(affected > 0)
    }

    /// Reinforce an entry (bump its importance, mark it used).
    pub fn reinforce(&self, id: &str) -> Result<(), AgentError> {
        self.storage.execute(
            "UPDATE agent_memory SET importance = MIN(1.0, importance + 0.1), last_used_at = ?2 \
             WHERE id = ?1",
            &[Value::String(id.to_string()), Value::from(now_ms())],
        )?;
        Ok(())
    }

    /// Decay every entry's importance by age; delete entries that fall below
    /// `min_importance`. Returns the number deleted.
    pub fn decay(&self, half_life_days: f32, min_importance: f32) -> Result<usize, AgentError> {
        let rows = self.storage.query("SELECT * FROM agent_memory", &[])?;
        let now = now_ms();
        let mut removed = 0;
        for row in &rows {
            let entry = entry_from_row(row);
            let age_days = (now - entry.updated_at).max(0) as f32 / 86_400_000.0;
            let factor = 0.5_f32.powf(age_days / half_life_days.max(f32::EPSILON));
            let next = entry.importance * factor;
            if next < min_importance {
                self.storage.execute(
                    "DELETE FROM agent_memory WHERE id = ?1",
                    &[Value::String(entry.id)],
                )?;
                removed += 1;
            } else {
                self.storage.execute(
                    "UPDATE agent_memory SET importance = ?1 WHERE id = ?2",
                    &[Value::from(f64::from(next)), Value::String(entry.id)],
                )?;
            }
        }
        Ok(removed)
    }

    /// Merge duplicate content within `scope`, keeping the highest-importance
    /// entry. Returns the number of entries removed.
    pub fn consolidate(&self, scope: &MemoryScope) -> Result<usize, AgentError> {
        let entries = self.list(scope, None)?;
        let mut best: std::collections::HashMap<String, MemoryEntry> =
            std::collections::HashMap::new();
        let mut removed = 0;
        for entry in entries {
            let key = entry.content.trim().to_lowercase();
            let keep_existing = best
                .get(&key)
                .is_some_and(|kept| kept.importance >= entry.importance);
            if keep_existing {
                self.forget(&entry.id)?;
                removed += 1;
            } else if let Some(previous) = best.insert(key, entry) {
                self.forget(&previous.id)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn entry_from_row(row: &fergents_storage::Row) -> MemoryEntry {
    MemoryEntry {
        id: str_col(row, "id"),
        scope: MemoryScope::parse(&str_col(row, "scope")),
        kind: MemoryKind::parse(&str_col(row, "kind")),
        content: str_col(row, "content"),
        importance: row.get("importance").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
        updated_at: row.get("updated_at").and_then(Value::as_i64).unwrap_or(0),
        last_used_at: row.get("last_used_at").and_then(Value::as_i64),
    }
}

fn str_col(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_round_trips_and_visibility() {
        for scope in [
            MemoryScope::Global,
            MemoryScope::Agent("a".to_string()),
            MemoryScope::Project("p".to_string()),
            MemoryScope::Brain("b".to_string()),
        ] {
            assert_eq!(MemoryScope::parse(&scope.key()), scope);
        }
        let agent = MemoryScope::Agent("a".to_string());
        assert!(agent.visible_from(&MemoryScope::Global));
        assert!(agent.visible_from(&agent));
        assert!(!agent.visible_from(&MemoryScope::Agent("b".to_string())));
    }
}
