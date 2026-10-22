//! Fergents chat backend (spec: Chat Sessions, Messages, and Streaming).
//!
//! Sessions and messages live in the global store (`agent_sessions` /
//! `agent_messages`), with an FTS5 index for chat search, a prompt library,
//! session forks, regeneration, bounded auto-continue, and an attachment
//! pipeline that degrades gracefully when a model cannot consume a type.

mod error;
mod model;
mod service;

pub use error::ChatError;
pub use model::{
    Attachment, AttachmentKind, AttachmentRouting, ChatMessage, ChatSession, PromptTemplate,
    SearchHit,
};
pub use service::ChatService;
