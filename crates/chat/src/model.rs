//! Chat model types.

use serde::{Deserialize, Serialize};

/// A chat session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatSession {
    /// The session id.
    pub id: String,
    /// The agent that owns the session.
    pub agent_id: String,
    /// The session title.
    pub title: Option<String>,
    /// The selected model.
    pub model: Option<String>,
    /// The lifecycle state (`created`, `running`, `done`, ...).
    pub state: String,
    /// The session this was forked from, if any.
    pub parent_session_id: Option<String>,
    /// The permission mode in force.
    pub permission_mode: String,
    /// The rolling compaction summary.
    pub summary: Option<String>,
    /// Accumulated input tokens.
    pub tokens_in: i64,
    /// Accumulated output tokens.
    pub tokens_out: i64,
    /// Accumulated cost in USD.
    pub cost_usd: f64,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
    /// Last update time (ms since the epoch).
    pub updated_at: i64,
}

/// A chat message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// The message id.
    pub id: String,
    /// The owning session.
    pub session_id: String,
    /// Monotonic sequence within the session.
    pub seq: i64,
    /// `user`, `assistant`, `tool`, or `system`.
    pub role: String,
    /// The message content.
    pub content: String,
    /// Tool calls, when this is an assistant tool-call turn.
    pub tool_calls: Option<serde_json::Value>,
    /// Token count, when known.
    pub tokens: i64,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
}

/// A chat-search hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// The owning session.
    pub session_id: String,
    /// The matching message.
    pub message_id: String,
    /// The message role.
    pub role: String,
    /// A snippet around the match.
    pub snippet: String,
}

/// The kind of an attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    /// Plain text or Markdown.
    Text,
    /// An image.
    Image,
    /// A PDF document.
    Pdf,
    /// Anything else.
    Other,
}

impl AttachmentKind {
    /// Classify an attachment by its media type and name.
    pub fn classify(media_type: &str, name: &str) -> AttachmentKind {
        let lower = media_type.to_ascii_lowercase();
        if lower.starts_with("text/")
            || lower == "application/json"
            || name.to_ascii_lowercase().ends_with(".md")
        {
            AttachmentKind::Text
        } else if lower.starts_with("image/") {
            AttachmentKind::Image
        } else if lower == "application/pdf" || name.to_ascii_lowercase().ends_with(".pdf") {
            AttachmentKind::Pdf
        } else {
            AttachmentKind::Other
        }
    }

    /// The persisted label.
    pub fn as_str(self) -> &'static str {
        match self {
            AttachmentKind::Text => "text",
            AttachmentKind::Image => "image",
            AttachmentKind::Pdf => "pdf",
            AttachmentKind::Other => "other",
        }
    }
}

/// An attachment reference supplied with a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// The file name.
    pub name: String,
    /// The MIME type.
    pub media_type: String,
    /// The size in bytes.
    pub size: u64,
    /// The classified kind.
    pub kind: AttachmentKind,
}

impl Attachment {
    /// Build and classify an attachment.
    pub fn new(name: impl Into<String>, media_type: impl Into<String>, size: u64) -> Self {
        let name = name.into();
        let media_type = media_type.into();
        let kind = AttachmentKind::classify(&media_type, &name);
        Self {
            name,
            media_type,
            size,
            kind,
        }
    }
}

/// How an attachment was routed into the conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachmentRouting {
    /// The attachment.
    pub attachment: Attachment,
    /// `inline`, `vision`, or `metadata_only`.
    pub routed: String,
    /// A note explaining a degraded route.
    pub note: Option<String>,
}

/// A saved prompt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// The prompt id.
    pub id: String,
    /// The title.
    pub title: String,
    /// The prompt body.
    pub body: String,
    /// Tags.
    pub tags: Vec<String>,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
}
