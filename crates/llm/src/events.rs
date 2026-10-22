//! Lifecycle events emitted by the router (`LLM.md` §9.3).

use crate::types::Usage;

/// A router/provider lifecycle event.
#[derive(Debug, Clone, PartialEq)]
pub enum LlmEvent {
    /// A candidate model was selected for a task.
    Routed {
        /// The task.
        task: String,
        /// The selected model.
        model: String,
        /// The provider id.
        provider: String,
    },
    /// A request is about to be sent.
    Request {
        /// The task.
        task: String,
        /// The model.
        model: String,
    },
    /// A response completed.
    Response {
        /// The model.
        model: String,
        /// Token usage.
        usage: Usage,
        /// Cost in USD.
        cost_usd: f64,
        /// Wall-clock duration.
        duration_ms: u64,
    },
    /// A model failed.
    Error {
        /// The model.
        model: String,
        /// The error message.
        message: String,
    },
    /// A model rate limited the request.
    RateLimited {
        /// The model.
        model: String,
    },
    /// A response was served from cache.
    CacheHit {
        /// The model.
        model: String,
    },
}

/// A sink for [`LlmEvent`]s. The kernel adapts the event bus to this trait.
pub trait LlmEventSink: Send + Sync {
    /// Emit an event.
    fn emit(&self, event: LlmEvent);
}

/// A sink that drops every event (the default).
#[derive(Debug, Default)]
pub struct NoopSink;

impl LlmEventSink for NoopSink {
    fn emit(&self, _event: LlmEvent) {}
}

/// A test sink that records events in memory.
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: std::sync::Mutex<Vec<LlmEvent>>,
}

impl RecordingSink {
    /// Create an empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// The events recorded so far.
    pub fn events(&self) -> Vec<LlmEvent> {
        self.events.lock().expect("recording sink poisoned").clone()
    }
}

impl LlmEventSink for RecordingSink {
    fn emit(&self, event: LlmEvent) {
        self.events
            .lock()
            .expect("recording sink poisoned")
            .push(event);
    }
}
