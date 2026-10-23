//! The chat service: sessions, messages, search, forks, and prompts.

use std::sync::Arc;

use fergents_events::{now_ms, EventBus};
use fergents_storage::{Storage, Value};
use serde_json::{json, Value as Json};

use crate::error::ChatError;
use crate::model::{
    Attachment, AttachmentRouting, ChatMessage, ChatSession, PromptTemplate, SearchHit,
};

/// The default cap on automatic continuation turns.
pub const DEFAULT_MAX_AUTO_CONTINUES: u32 = 3;

/// Reads and writes chat sessions, messages, and prompts.
pub struct ChatService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    max_auto_continues: u32,
}

impl ChatService {
    /// Wrap `storage`, capping auto-continue at [`DEFAULT_MAX_AUTO_CONTINUES`].
    pub fn new(storage: Arc<dyn Storage>, bus: Arc<EventBus>) -> Self {
        Self {
            storage,
            bus,
            max_auto_continues: DEFAULT_MAX_AUTO_CONTINUES,
        }
    }

    /// Override the auto-continue cap.
    pub fn with_max_auto_continues(mut self, max: u32) -> Self {
        self.max_auto_continues = max;
        self
    }

    /// The configured auto-continue cap.
    pub fn max_auto_continues(&self) -> u32 {
        self.max_auto_continues
    }

    /// Create a session.
    pub fn create_session(
        &self,
        agent_id: &str,
        title: Option<&str>,
        model: Option<&str>,
    ) -> Result<ChatSession, ChatError> {
        if agent_id.trim().is_empty() {
            return Err(ChatError::Invalid("agent_id is empty".to_string()));
        }
        let id = ulid::Ulid::new().to_string();
        let now = now_ms();
        self.storage.execute(
            "INSERT INTO agent_sessions \
             (id, agent_id, title, model, state, permission_mode, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'created', 'auto', ?5, ?5)",
            &[
                Value::String(id.clone()),
                Value::String(agent_id.to_string()),
                title.map_or(Value::Null, |t| Value::String(t.to_string())),
                model.map_or(Value::Null, |m| Value::String(m.to_string())),
                Value::from(now),
            ],
        )?;
        self.bus.publish(
            "chat.session.created",
            "chat",
            json!({ "session_id": id, "agent_id": agent_id }),
        )?;
        self.get(&id)?.ok_or_else(|| ChatError::UnknownSession(id))
    }

    /// Fetch a session.
    pub fn get(&self, session_id: &str) -> Result<Option<ChatSession>, ChatError> {
        let rows = self.storage.query(
            "SELECT * FROM agent_sessions WHERE id = ?1",
            &[Value::String(session_id.to_string())],
        )?;
        Ok(rows.first().map(session_from_row))
    }

    /// List sessions, newest first.
    pub fn list(&self, limit: usize) -> Result<Vec<ChatSession>, ChatError> {
        let rows = self.storage.query(
            "SELECT * FROM agent_sessions ORDER BY updated_at DESC LIMIT ?1",
            &[Value::from(limit as i64)],
        )?;
        Ok(rows.iter().map(session_from_row).collect())
    }

    /// Rename a session.
    pub fn rename(&self, session_id: &str, title: &str) -> Result<ChatSession, ChatError> {
        self.require(session_id)?;
        self.storage.execute(
            "UPDATE agent_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            &[
                Value::String(title.to_string()),
                Value::from(now_ms()),
                Value::String(session_id.to_string()),
            ],
        )?;
        self.bus.publish(
            "chat.session.updated",
            "chat",
            json!({ "session_id": session_id, "title": title }),
        )?;
        self.require(session_id)
    }

    /// Delete a session and its messages.
    pub fn delete(&self, session_id: &str) -> Result<(), ChatError> {
        self.require(session_id)?;
        self.storage.execute(
            "DELETE FROM agent_messages WHERE session_id = ?1",
            &[Value::String(session_id.to_string())],
        )?;
        self.storage.execute(
            "DELETE FROM agent_sessions WHERE id = ?1",
            &[Value::String(session_id.to_string())],
        )?;
        self.bus.publish(
            "chat.session.deleted",
            "chat",
            json!({ "session_id": session_id }),
        )?;
        Ok(())
    }

    /// The messages of a session, oldest first.
    pub fn messages(&self, session_id: &str) -> Result<Vec<ChatMessage>, ChatError> {
        let rows = self.storage.query(
            "SELECT * FROM agent_messages WHERE session_id = ?1 ORDER BY seq, rowid",
            &[Value::String(session_id.to_string())],
        )?;
        Ok(rows.iter().map(message_from_row).collect())
    }

    /// Append a user message.
    pub fn send(&self, session_id: &str, text: &str) -> Result<ChatMessage, ChatError> {
        if text.trim().is_empty() {
            return Err(ChatError::Invalid("message is empty".to_string()));
        }
        self.append(session_id, "user", text, None)
    }

    /// Append a message to a session.
    pub fn append(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        tool_calls: Option<Json>,
    ) -> Result<ChatMessage, ChatError> {
        self.require(session_id)?;
        let seq = self.next_seq(session_id)?;
        let id = ulid::Ulid::new().to_string();
        let now = now_ms();
        let stored_calls = tool_calls
            .as_ref()
            .map_or(Value::Null, |calls| Value::String(calls.to_string()));
        self.storage.execute(
            "INSERT INTO agent_messages \
             (id, session_id, seq, role, content, tool_calls, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            &[
                Value::String(id.clone()),
                Value::String(session_id.to_string()),
                Value::from(seq),
                Value::String(role.to_string()),
                Value::String(content.to_string()),
                stored_calls,
                Value::from(now),
            ],
        )?;
        self.touch(session_id)?;
        self.bus.publish(
            "chat.message.created",
            "chat",
            json!({ "session_id": session_id, "message_id": id, "role": role, "seq": seq }),
        )?;
        Ok(ChatMessage {
            id,
            session_id: session_id.to_string(),
            seq,
            role: role.to_string(),
            content: content.to_string(),
            tool_calls,
            tokens: 0,
            created_at: now,
        })
    }

    /// Full-text search across every message.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, ChatError> {
        let match_query = sanitize_fts(query);
        if match_query.is_empty() {
            return Ok(Vec::new());
        }
        let rows = self.storage.query(
            "SELECT m.id, m.session_id, m.role, \
                    snippet(agent_messages_fts, 0, '', '', '…', 12) AS snippet \
             FROM agent_messages_fts f \
             JOIN agent_messages m ON m.rowid = f.rowid \
             WHERE agent_messages_fts MATCH ?1 \
             ORDER BY rank LIMIT ?2",
            &[Value::String(match_query), Value::from(limit as i64)],
        )?;
        Ok(rows
            .iter()
            .map(|row| SearchHit {
                message_id: str_col(row, "id"),
                session_id: str_col(row, "session_id"),
                role: str_col(row, "role"),
                snippet: str_col(row, "snippet"),
            })
            .collect())
    }

    /// Fork a session: copy its messages into a new child session.
    pub fn fork(&self, session_id: &str, title: Option<&str>) -> Result<ChatSession, ChatError> {
        let parent = self.require(session_id)?;
        let id = ulid::Ulid::new().to_string();
        let now = now_ms();
        let fork_title = title
            .map(str::to_string)
            .or_else(|| parent.title.as_ref().map(|t| format!("{t} (fork)")));
        self.storage.execute(
            "INSERT INTO agent_sessions \
             (id, agent_id, title, model, state, permission_mode, parent_session_id, \
              created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, 'created', ?5, ?6, ?7, ?7)",
            &[
                Value::String(id.clone()),
                Value::String(parent.agent_id.clone()),
                fork_title.map_or(Value::Null, Value::String),
                parent.model.clone().map_or(Value::Null, Value::String),
                Value::String(parent.permission_mode.clone()),
                Value::String(session_id.to_string()),
                Value::from(now),
            ],
        )?;
        let messages = self.messages(session_id)?;
        for message in messages {
            self.storage.execute(
                "INSERT INTO agent_messages \
                 (id, session_id, seq, role, content, tool_calls, tokens, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                &[
                    Value::String(ulid::Ulid::new().to_string()),
                    Value::String(id.clone()),
                    Value::from(message.seq),
                    Value::String(message.role),
                    Value::String(message.content),
                    message
                        .tool_calls
                        .map_or(Value::Null, |calls| Value::String(calls.to_string())),
                    Value::from(message.tokens),
                    Value::from(message.created_at),
                ],
            )?;
        }
        self.bus.publish(
            "chat.forked",
            "chat",
            json!({ "session_id": id, "parent_session_id": session_id }),
        )?;
        self.require(&id)
    }

    /// Drop the last assistant message so the caller can regenerate it,
    /// returning the user message it responded to.
    pub fn regenerate(&self, session_id: &str) -> Result<Option<ChatMessage>, ChatError> {
        self.require(session_id)?;
        let messages = self.messages(session_id)?;
        let Some(last_assistant) = messages.iter().rev().find(|m| m.role == "assistant") else {
            return Ok(messages.iter().rev().find(|m| m.role == "user").cloned());
        };
        self.storage.execute(
            "DELETE FROM agent_messages WHERE id = ?1",
            &[Value::String(last_assistant.id.clone())],
        )?;
        self.touch(session_id)?;
        Ok(self
            .messages(session_id)?
            .into_iter()
            .rev()
            .find(|m| m.role == "user"))
    }

    /// Append `requested` continue turns, clamped to the configured cap.
    ///
    /// Returns the number of turns actually appended.
    pub fn auto_continue(&self, session_id: &str, requested: u32) -> Result<u32, ChatError> {
        self.require(session_id)?;
        let turns = requested.min(self.max_auto_continues);
        for index in 0..turns {
            self.append(session_id, "user", "Continue from where you stopped.", None)?;
            self.bus.publish(
                "chat.continued",
                "chat",
                json!({ "session_id": session_id, "turn": index + 1 }),
            )?;
        }
        Ok(turns)
    }

    /// Record an attachment on the conversation, degrading gracefully when the
    /// type cannot be consumed by the model.
    pub fn attach(
        &self,
        session_id: &str,
        attachment: Attachment,
    ) -> Result<AttachmentRouting, ChatError> {
        let routing = match attachment.kind {
            crate::model::AttachmentKind::Text => AttachmentRouting {
                routed: "inline".to_string(),
                note: None,
                attachment: attachment.clone(),
            },
            crate::model::AttachmentKind::Image => AttachmentRouting {
                routed: "vision".to_string(),
                note: Some("routed as an image block; non-vision models ignore it".to_string()),
                attachment: attachment.clone(),
            },
            crate::model::AttachmentKind::Pdf => AttachmentRouting {
                routed: "metadata_only".to_string(),
                note: Some("PDF text extraction is not available yet".to_string()),
                attachment: attachment.clone(),
            },
            crate::model::AttachmentKind::Other => AttachmentRouting {
                routed: "metadata_only".to_string(),
                note: Some("unsupported attachment type".to_string()),
                attachment: attachment.clone(),
            },
        };
        let content = json!({
            "attachment": {
                "name": attachment.name,
                "media_type": attachment.media_type,
                "size": attachment.size,
                "kind": attachment.kind.as_str(),
            },
            "routed": routing.routed,
        });
        self.append(session_id, "user", &content.to_string(), None)?;
        Ok(routing)
    }

    /// The saved prompts.
    pub fn list_prompts(&self) -> Result<Vec<PromptTemplate>, ChatError> {
        let rows = self
            .storage
            .query("SELECT * FROM chat_prompts ORDER BY created_at DESC", &[])?;
        Ok(rows.iter().map(prompt_from_row).collect())
    }

    /// Save a prompt.
    pub fn create_prompt(
        &self,
        title: &str,
        body: &str,
        tags: &[String],
    ) -> Result<PromptTemplate, ChatError> {
        if title.trim().is_empty() || body.trim().is_empty() {
            return Err(ChatError::Invalid(
                "prompt title and body are required".to_string(),
            ));
        }
        let prompt = PromptTemplate {
            id: ulid::Ulid::new().to_string(),
            title: title.to_string(),
            body: body.to_string(),
            tags: tags.to_vec(),
            created_at: now_ms(),
        };
        self.storage.execute(
            "INSERT INTO chat_prompts (id, title, body, tags, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            &[
                Value::String(prompt.id.clone()),
                Value::String(prompt.title.clone()),
                Value::String(prompt.body.clone()),
                Value::String(serde_json::to_string(&prompt.tags)?),
                Value::from(prompt.created_at),
            ],
        )?;
        Ok(prompt)
    }

    /// Delete a prompt. Returns whether it existed.
    pub fn delete_prompt(&self, id: &str) -> Result<bool, ChatError> {
        let affected = self.storage.execute(
            "DELETE FROM chat_prompts WHERE id = ?1",
            &[Value::String(id.to_string())],
        )?;
        Ok(affected > 0)
    }

    fn require(&self, session_id: &str) -> Result<ChatSession, ChatError> {
        self.get(session_id)?
            .ok_or_else(|| ChatError::UnknownSession(session_id.to_string()))
    }

    fn next_seq(&self, session_id: &str) -> Result<i64, ChatError> {
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

    fn touch(&self, session_id: &str) -> Result<(), ChatError> {
        self.storage.execute(
            "UPDATE agent_sessions SET updated_at = ?1 WHERE id = ?2",
            &[Value::from(now_ms()), Value::String(session_id.to_string())],
        )?;
        Ok(())
    }
}

/// Quote every term so user input cannot break FTS5 query syntax.
fn sanitize_fts(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn session_from_row(row: &fergents_storage::Row) -> ChatSession {
    ChatSession {
        id: str_col(row, "id"),
        agent_id: str_col(row, "agent_id"),
        title: opt_str(row, "title"),
        model: opt_str(row, "model"),
        state: str_col(row, "state"),
        parent_session_id: opt_str(row, "parent_session_id"),
        permission_mode: row
            .get("permission_mode")
            .and_then(Value::as_str)
            .unwrap_or("auto")
            .to_string(),
        summary: opt_str(row, "summary"),
        tokens_in: row.get("tokens_in").and_then(Value::as_i64).unwrap_or(0),
        tokens_out: row.get("tokens_out").and_then(Value::as_i64).unwrap_or(0),
        cost_usd: row.get("cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
        updated_at: row.get("updated_at").and_then(Value::as_i64).unwrap_or(0),
    }
}

fn message_from_row(row: &fergents_storage::Row) -> ChatMessage {
    ChatMessage {
        id: str_col(row, "id"),
        session_id: str_col(row, "session_id"),
        seq: row.get("seq").and_then(Value::as_i64).unwrap_or(0),
        role: str_col(row, "role"),
        content: str_col(row, "content"),
        tool_calls: row
            .get("tool_calls")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str(text).ok()),
        tokens: row.get("tokens").and_then(Value::as_i64).unwrap_or(0),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
    }
}

fn prompt_from_row(row: &fergents_storage::Row) -> PromptTemplate {
    PromptTemplate {
        id: str_col(row, "id"),
        title: str_col(row, "title"),
        body: str_col(row, "body"),
        tags: row
            .get("tags")
            .and_then(Value::as_str)
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default(),
        created_at: row.get("created_at").and_then(Value::as_i64).unwrap_or(0),
    }
}

fn str_col(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn opt_str(row: &fergents_storage::Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_string)
}
