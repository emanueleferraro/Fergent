//! Persistence for sessions and messages.

use std::sync::Arc;

use fergents_events::now_ms;
use fergents_storage::{Storage, Value};

use crate::error::AgentError;
use crate::session::{AgentMessageRecord, AgentSessionRecord, RunState};

/// Reads and writes `agent_sessions` / `agent_messages`.
pub struct AgentStore {
    storage: Arc<dyn Storage>,
}

impl AgentStore {
    /// Wrap `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self { storage }
    }

    /// Insert a new session.
    pub fn create_session(
        &self,
        id: &str,
        agent_id: &str,
        title: Option<&str>,
    ) -> Result<(), AgentError> {
        let now = now_ms();
        self.storage.execute(
            "INSERT INTO agent_sessions \
             (id, agent_id, title, state, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            &[
                Value::String(id.to_string()),
                Value::String(agent_id.to_string()),
                title.map_or(Value::Null, |t| Value::String(t.to_string())),
                Value::String(RunState::Created.as_str().to_string()),
                Value::from(now),
            ],
        )?;
        Ok(())
    }

    /// Update a session's state.
    pub fn set_state(&self, id: &str, state: RunState) -> Result<(), AgentError> {
        self.storage.execute(
            "UPDATE agent_sessions SET state = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::String(state.as_str().to_string()),
                Value::from(now_ms()),
                Value::String(id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Add a run's token usage and cost to the session totals.
    pub fn add_usage(
        &self,
        id: &str,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
    ) -> Result<(), AgentError> {
        self.storage.execute(
            "UPDATE agent_sessions SET \
             tokens_in = COALESCE(tokens_in, 0) + ?1, \
             tokens_out = COALESCE(tokens_out, 0) + ?2, \
             cost_usd = COALESCE(cost_usd, 0) + ?3, \
             updated_at = ?4 WHERE id = ?5",
            &[
                Value::from(input_tokens),
                Value::from(output_tokens),
                Value::from(cost_usd),
                Value::from(now_ms()),
                Value::String(id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Update a session's summary.
    pub fn set_summary(&self, id: &str, summary: &str) -> Result<(), AgentError> {
        self.storage.execute(
            "UPDATE agent_sessions SET summary = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::String(summary.to_string()),
                Value::from(now_ms()),
                Value::String(id.to_string()),
            ],
        )?;
        Ok(())
    }

    /// Append a message to a session.
    pub fn append_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<(), AgentError> {
        let seq = self.next_seq(session_id)?;
        self.storage.execute(
            "INSERT INTO agent_messages (id, session_id, seq, role, content, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[
                Value::String(ulid::Ulid::new().to_string()),
                Value::String(session_id.to_string()),
                Value::from(seq),
                Value::String(role.to_string()),
                Value::String(content.to_string()),
                Value::from(now_ms()),
            ],
        )?;
        Ok(())
    }

    /// The next free sequence number within a session.
    fn next_seq(&self, session_id: &str) -> Result<i64, AgentError> {
        let rows = self.storage.query(
            "SELECT COALESCE(MAX(seq), 0) + 1 AS next FROM agent_messages WHERE session_id = ?1",
            &[Value::String(session_id.to_string())],
        )?;
        Ok(rows
            .first()
            .and_then(|row| row.get("next"))
            .and_then(Value::as_i64)
            .unwrap_or(1))
    }

    /// Fetch a session by id.
    pub fn get_session(&self, id: &str) -> Result<Option<AgentSessionRecord>, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM agent_sessions WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        Ok(rows.first().map(session_from_row))
    }

    /// List sessions, newest first, up to `limit`.
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<AgentSessionRecord>, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM agent_sessions ORDER BY updated_at DESC LIMIT ?1",
            &[Value::from(limit as i64)],
        )?;
        Ok(rows.iter().map(session_from_row).collect())
    }

    /// The messages of a session, oldest first.
    pub fn messages(&self, session_id: &str) -> Result<Vec<AgentMessageRecord>, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM agent_messages WHERE session_id = ?1 ORDER BY created_at, rowid",
            &[Value::String(session_id.to_string())],
        )?;
        Ok(rows.iter().map(message_from_row).collect())
    }
}

fn session_from_row(row: &fergents_storage::Row) -> AgentSessionRecord {
    AgentSessionRecord {
        id: str_col(row, "id"),
        agent_id: str_col(row, "agent_id"),
        title: row.get("title").and_then(Value::as_str).map(str::to_string),
        model: row.get("model").and_then(Value::as_str).map(str::to_string),
        state: row
            .get("state")
            .and_then(Value::as_str)
            .map_or(RunState::Created, RunState::parse),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
        updated_at: row.get("updated_at").and_then(Value::as_i64).unwrap_or(0),
        summary: row
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn message_from_row(row: &fergents_storage::Row) -> AgentMessageRecord {
    AgentMessageRecord {
        id: str_col(row, "id"),
        session_id: str_col(row, "session_id"),
        role: str_col(row, "role"),
        content: str_col(row, "content"),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
    }
}

fn str_col(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
