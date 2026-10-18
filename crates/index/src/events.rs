//! Event emission for the indexing pipeline.
//!
//! The indexer does not depend on the event bus; it emits through the
//! [`EventSink`] trait. The kernel wires an `EventBus`-backed implementation;
//! tests use [`RecordingSink`]. Payloads are JSON matching the Phase 1 protocol
//! schema (`index.*`, `file.*`, `note.*`).

use std::sync::Mutex;

use serde_json::Value;

/// A sink for indexer events: `(kind, payload)`.
pub trait EventSink: Send + Sync {
    /// Emit one event. Implementations must not panic on failure.
    fn emit(&self, kind: &str, payload: Value);
}

/// An [`EventSink`] that discards everything.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _kind: &str, _payload: Value) {}
}

/// An [`EventSink`] that records events in memory, for tests.
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: Mutex<Vec<(String, Value)>>,
}

impl RecordingSink {
    /// A new, empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// All recorded `(kind, payload)` pairs, in order.
    pub fn events(&self) -> Vec<(String, Value)> {
        self.events.lock().expect("sink poisoned").clone()
    }

    /// All recorded event kinds, in order.
    pub fn kinds(&self) -> Vec<String> {
        self.events
            .lock()
            .expect("sink poisoned")
            .iter()
            .map(|(kind, _)| kind.clone())
            .collect()
    }

    /// The first payload recorded for `kind`.
    pub fn first(&self, kind: &str) -> Option<Value> {
        self.events
            .lock()
            .expect("sink poisoned")
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, payload)| payload.clone())
    }

    /// How many events of `kind` were recorded.
    pub fn count(&self, kind: &str) -> usize {
        self.events
            .lock()
            .expect("sink poisoned")
            .iter()
            .filter(|(k, _)| k == kind)
            .count()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, kind: &str, payload: Value) {
        self.events
            .lock()
            .expect("sink poisoned")
            .push((kind.to_string(), payload));
    }
}
