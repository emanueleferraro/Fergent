//! Inline AI backend (Phase 3 spec §6, tasks 3.12).
//!
//! The editor sends the text around the cursor; the kernel never owns the
//! buffer. Completions are prompt-based, bounded, and cancellable: a cancelled
//! request still returns (with `cancelled: true` and an empty suggestion), so
//! the client never waits on a stranded call.
//!
//! Quick actions (`inline.action`) reuse the Composer pipeline: the proposal
//! is returned as a normal run, so the UI can open the same diff preview and
//! nothing is written until the user approves hunks.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use fergents_events::EventBus;
use fergents_ide::ProjectService;
use fergents_llm::{ChatRequest, Message};
use fergents_protocol::{ComposerRun, InlineCompleteResult};

use crate::composer::{
    CompletionClient, ComposerError, ComposerService, ModelResolver, ProposalRequest,
};

/// The most characters accepted on either side of the cursor.
pub const MAX_CONTEXT_CHARS: usize = 40_000;
/// The default output cap for a completion.
pub const DEFAULT_MAX_TOKENS: u32 = 256;
/// The hard output cap for a completion.
pub const MAX_TOKENS_CAP: u32 = 1024;
/// How many cancelled request ids are remembered.
const MAX_CANCELLED: usize = 256;

const INLINE_SYSTEM: &str = "\
You are an inline code completion engine inside an editor.

Continue the code at the cursor. Reply with the characters to insert only:
no explanation, no markdown fences, no repetition of the text before or after
the cursor. Prefer a single expression or statement. If nothing sensible
applies, reply with an empty string.";

/// One completion request.
#[derive(Debug, Clone, Default)]
pub struct CompleteRequest {
    /// The project the buffer belongs to.
    pub project_id: String,
    /// The project-relative path, used as context for the model.
    pub path: String,
    /// The text before the cursor.
    pub prefix: String,
    /// The text after the cursor.
    pub suffix: String,
    /// Monaco language id, when known.
    pub language_id: Option<String>,
    /// Explicit model override.
    pub model: Option<String>,
    /// Output token cap.
    pub max_tokens: Option<u32>,
    /// Client-generated id, used by `inline.cancel`.
    pub request_id: Option<String>,
}

/// Errors raised by Inline AI.
#[derive(Debug, thiserror::Error)]
pub enum InlineError {
    /// A project operation failed.
    #[error(transparent)]
    Project(#[from] fergents_ide::ProjectError),
    /// The Composer pipeline failed (quick actions).
    #[error(transparent)]
    Composer(#[from] ComposerError),
    /// The event bus rejected a publication.
    #[error(transparent)]
    Event(#[from] fergents_events::EventError),
    /// The request is too large for a bounded completion.
    #[error("inline context is too large")]
    ContextTooLarge,
    /// No provider advertises a model for the task.
    #[error("no model configured for the {0} task")]
    NoModel(String),
    /// The provider call failed.
    #[error("model call failed: {0}")]
    Model(String),
}

/// Inline completions and selection quick actions.
pub struct InlineService {
    projects: Arc<ProjectService>,
    composer: Arc<ComposerService>,
    client: Arc<dyn CompletionClient>,
    model_for: ModelResolver,
    cancelled: RwLock<HashSet<String>>,
    bus: Arc<EventBus>,
}

impl InlineService {
    /// Create the service.
    pub fn new(
        projects: Arc<ProjectService>,
        composer: Arc<ComposerService>,
        client: Arc<dyn CompletionClient>,
        model_for: ModelResolver,
        bus: Arc<EventBus>,
    ) -> Self {
        Self {
            projects,
            composer,
            client,
            model_for,
            cancelled: RwLock::new(HashSet::new()),
            bus,
        }
    }

    /// Produce a completion for the text around the cursor.
    pub async fn complete(
        &self,
        request: CompleteRequest,
    ) -> Result<InlineCompleteResult, InlineError> {
        self.projects.require_trusted(&request.project_id)?;
        if request.prefix.len() > MAX_CONTEXT_CHARS || request.suffix.len() > MAX_CONTEXT_CHARS {
            return Err(InlineError::ContextTooLarge);
        }
        let model = request
            .model
            .clone()
            .or_else(|| (self.model_for)("inline"))
            .ok_or_else(|| InlineError::NoModel("inline".to_string()))?;
        let request_id = request
            .request_id
            .clone()
            .unwrap_or_else(|| ulid::Ulid::new().to_string());
        let started = Instant::now();
        let chat = ChatRequest {
            model: model.clone(),
            messages: vec![
                Message::system(INLINE_SYSTEM),
                Message::user(build_prompt(&request)),
            ],
            tools: Vec::new(),
            temperature: Some(0.1),
            max_tokens: Some(
                request
                    .max_tokens
                    .unwrap_or(DEFAULT_MAX_TOKENS)
                    .clamp(1, MAX_TOKENS_CAP),
            ),
            stream: false,
        };
        let raw = self
            .client
            .complete(&model, chat)
            .await
            .map_err(InlineError::Model)?;
        let cancelled = self
            .cancelled
            .write()
            .expect("inline cancel lock")
            .remove(&request_id);
        let suggestion = if cancelled {
            String::new()
        } else {
            sanitize(&raw)
        };
        let latency_ms = started.elapsed().as_millis() as i64;
        let kind = if cancelled {
            "inline.cancelled"
        } else {
            "inline.suggested"
        };
        self.bus.publish(
            kind,
            "inline",
            serde_json::json!({
                "request_id": request_id,
                "project_id": request.project_id,
                "path": request.path,
                "characters": suggestion.chars().count(),
                "latency_ms": latency_ms,
            }),
        )?;
        Ok(InlineCompleteResult {
            suggestion,
            model,
            latency_ms,
            cancelled,
        })
    }

    /// Mark a request id as cancelled. Returns `false` when it was already marked.
    pub fn cancel(&self, request_id: &str) -> bool {
        let mut cancelled = self.cancelled.write().expect("inline cancel lock");
        if cancelled.len() >= MAX_CANCELLED {
            cancelled.clear();
        }
        cancelled.insert(request_id.to_string())
    }

    /// Run a quick action over the selection through the Composer.
    pub async fn action(
        &self,
        request: ProposalRequest,
        action: &str,
    ) -> Result<ComposerRun, InlineError> {
        let prompt = action_prompt(action, &request.prompt);
        self.composer
            .propose(ProposalRequest { prompt, ..request })
            .await
            .map_err(InlineError::from)
    }
}

/// The quick-action instruction, with the user's extra text appended.
fn action_prompt(action: &str, extra: &str) -> String {
    let base = match action {
        "explain" => {
            "Explain the selected code clearly and concisely. Do not modify any file; put the \
             explanation in `message` and return an empty `files` array."
        }
        "refactor" => {
            "Refactor the selected code without changing behaviour. Return the full modified file."
        }
        "tests" => "Add focused tests that cover the selected code. Return the full modified file.",
        "document" => {
            "Document the selected code with concise doc comments. Return the full modified file."
        }
        "fix" => "Fix the bug in the selected code. Return the full modified file.",
        _ => "Apply the requested change to the selected code. Return the full modified file.",
    };
    if extra.trim().is_empty() {
        base.to_string()
    } else {
        format!("{base}\n\nAdditional instruction:\n{}", extra.trim())
    }
}

/// Strip a wrapping markdown fence and trailing whitespace.
fn sanitize(raw: &str) -> String {
    let trimmed = raw.trim_end();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let body = rest.split_once('\n').map_or("", |(_, rest)| rest);
        return body.trim_end_matches("```").trim_end().to_string();
    }
    trimmed.to_string()
}

/// The user message: the file, the language, and both sides of the cursor.
fn build_prompt(request: &CompleteRequest) -> String {
    let mut out = String::new();
    out.push_str("File: ");
    out.push_str(&request.path);
    if let Some(language) = &request.language_id {
        out.push_str(" (");
        out.push_str(language);
        out.push(')');
    }
    out.push_str("\n\nBefore the cursor:\n");
    out.push_str(&request.prefix);
    out.push_str("\n\nAfter the cursor:\n");
    out.push_str(&request.suffix);
    out.push_str("\n\nReturn only the text to insert at the cursor.");
    out
}
