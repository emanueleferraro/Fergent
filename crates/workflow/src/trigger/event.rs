//! Event-bus triggers (spec §2.5).
//!
//! A trigger subscribes to a kernel topic and fires when a matching event
//! arrives. Matching is prefix-based (through
//! [`fergents_events::topic_matches`]) plus an optional JSON pointer into the
//! payload, so `file.changed` with `/project_id` only fires for one project.
//!
//! Two guards keep this safe:
//!
//! - an event id is remembered for [`DEDUPE_WINDOW_MS`], so a redelivered
//!   event cannot start two runs;
//! - a workflow never reacts to its own `workflow.*` events, which would
//!   otherwise trigger itself without bound.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fergents_events::{topic_matches, EventBus};
use fergents_protocol::Event;
use serde_json::{json, Value};

use crate::error::WorkflowError;
use crate::executor::Cancellation;
use crate::trigger::{Clock, FireFn, SystemClock, TriggerFire};

/// How long an event id is remembered (spec §2.5: 1 s).
pub const DEDUPE_WINDOW_MS: i64 = 1_000;

/// The declared settings of one event trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTriggerConfig {
    /// The workflow that owns the trigger.
    pub workflow_id: String,
    /// The trigger id within the document.
    pub trigger_id: String,
    /// The subscribed topic; prefixes match.
    pub topic: String,
    /// An optional JSON pointer into the payload; empty means "any".
    pub filter: Option<String>,
}

/// Watches the event bus for one topic (spec §2.5).
pub struct EventTrigger {
    config: EventTriggerConfig,
    bus: Arc<EventBus>,
    fire: FireFn,
    clock: Arc<dyn Clock>,
    seen: Mutex<HashMap<String, i64>>,
}

impl EventTrigger {
    /// Build a trigger over `bus`, using the system clock.
    pub fn new(
        config: EventTriggerConfig,
        bus: Arc<EventBus>,
        fire: FireFn,
    ) -> Result<Self, WorkflowError> {
        Self::with_clock(config, bus, Arc::new(SystemClock), fire)
    }

    /// Build a trigger with an injected clock.
    pub fn with_clock(
        config: EventTriggerConfig,
        bus: Arc<EventBus>,
        clock: Arc<dyn Clock>,
        fire: FireFn,
    ) -> Result<Self, WorkflowError> {
        if config.topic.trim().is_empty() {
            return Err(WorkflowError::Invalid(
                "event trigger needs a topic".to_string(),
            ));
        }
        if let Some(filter) = &config.filter {
            if !filter.is_empty() && !filter.starts_with('/') {
                return Err(WorkflowError::Invalid(format!(
                    "`{filter}` is not a JSON pointer"
                )));
            }
        }
        Ok(Self {
            config,
            bus,
            fire,
            clock,
            seen: Mutex::new(HashMap::new()),
        })
    }

    /// The trigger configuration.
    pub fn config(&self) -> &EventTriggerConfig {
        &self.config
    }

    /// Whether `event` should start a run. Pure, so tests can drive it.
    pub fn accepts(&self, event: &Event) -> bool {
        topic_matches(&event.kind, &self.config.topic)
            && !self.is_own_event(event)
            && matches_filter(&event.payload, self.config.filter.as_deref())
    }

    /// Fire for `event` if it is accepted and not a duplicate.
    ///
    /// Returns the new run id when a run started.
    pub fn observe(&self, event: &Event) -> Result<Option<String>, WorkflowError> {
        if !self.accepts(event) {
            return Ok(None);
        }
        let now = self.clock.now_ms();
        if !self.remember(&event.id, now) {
            return Ok(None);
        }
        let run_id = (self.fire)(TriggerFire {
            workflow_id: self.config.workflow_id.clone(),
            trigger_id: self.config.trigger_id.clone(),
            kind: "event",
            actor: format!("trigger:{}", self.config.trigger_id),
            inputs: json!({
                "topic": event.kind,
                "event_id": event.id,
                "payload": event.payload,
            }),
        })?;
        Ok(Some(run_id))
    }

    /// Subscribe and fire until `cancel` is requested.
    pub async fn run(self, cancel: Cancellation) {
        let mut subscription = self.bus.subscribe_topics(vec![self.config.topic.clone()]);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                received = subscription.recv() => {
                    match received {
                        Ok(event) => {
                            if let Err(err) = self.observe(&event) {
                                tracing::warn!(
                                    workflow_id = %self.config.workflow_id,
                                    trigger_id = %self.config.trigger_id,
                                    error = %err,
                                    "event trigger failed to start a run"
                                );
                            }
                        }
                        Err(err) => {
                            tracing::warn!(
                                workflow_id = %self.config.workflow_id,
                                error = %err,
                                "event subscription closed"
                            );
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Whether `event` was emitted by the workflow that owns this trigger.
    ///
    /// Spec §2.5: a workflow cannot subscribe to its own `workflow.*` events,
    /// which would let it trigger itself without bound.
    fn is_own_event(&self, event: &Event) -> bool {
        event.kind.starts_with("workflow.")
            && event
                .payload
                .get("workflow_id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == self.config.workflow_id)
    }

    /// Record `id`, returning `false` when it is still within the dedupe
    /// window. Expired ids are pruned on the way through.
    fn remember(&self, id: &str, now: i64) -> bool {
        let mut seen = self.seen.lock().expect("event trigger poisoned");
        seen.retain(|_, at| now - *at < DEDUPE_WINDOW_MS);
        if seen.contains_key(id) {
            return false;
        }
        seen.insert(id.to_string(), now);
        true
    }
}

/// Whether `payload` satisfies `filter`.
///
/// An absent or empty filter matches everything; otherwise the pointer must
/// resolve to a non-null value.
fn matches_filter(payload: &Value, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some("") => true,
        Some(pointer) => resolve_pointer(payload, pointer).is_some_and(|value| !value.is_null()),
    }
}

/// Resolve a JSON pointer (RFC 6901) against `document`.
fn resolve_pointer<'a>(document: &'a Value, pointer: &str) -> Option<&'a Value> {
    let mut current = document;
    for token in pointer.trim_start_matches('/').split('/') {
        let token = token.replace("~1", "/").replace("~0", "~");
        current = match current {
            Value::Object(map) => map.get(&token)?,
            Value::Array(items) => items.get(token.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> Value {
        json!({ "project_id": "proj-1", "paths": ["a.rs", "b.rs"] })
    }

    #[test]
    fn pointers_resolve_objects_and_arrays() {
        assert_eq!(
            resolve_pointer(&payload(), "/project_id").and_then(Value::as_str),
            Some("proj-1")
        );
        assert_eq!(
            resolve_pointer(&payload(), "/paths/1").and_then(Value::as_str),
            Some("b.rs")
        );
        assert!(resolve_pointer(&payload(), "/missing").is_none());
        assert!(resolve_pointer(&payload(), "/paths/7").is_none());
    }

    #[test]
    fn filters_match_only_present_non_null_values() {
        assert!(matches_filter(&payload(), None));
        assert!(matches_filter(&payload(), Some("")));
        assert!(matches_filter(&payload(), Some("/project_id")));
        assert!(!matches_filter(&payload(), Some("/project")));
        assert!(!matches_filter(&json!({ "a": null }), Some("/a")));
    }

    #[test]
    fn escaped_pointer_tokens_are_unescaped() {
        let doc = json!({ "a/b": { "c~d": 1 } });
        assert_eq!(
            resolve_pointer(&doc, "/a~1b/c~0d").and_then(Value::as_i64),
            Some(1)
        );
    }
}
