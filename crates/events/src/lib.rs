//! Fergents event bus.
//!
//! Events are persisted append-only to the `events` table (global SQLite) and
//! broadcast to in-process subscribers, matching `ARCHITECTURE.md` §4.

use std::sync::{Arc, Mutex};

use fergents_protocol::{Event, EventsDroppedNotice};
use fergents_storage::{Storage, StorageError, Value};
use serde_json::json;
use tokio::sync::broadcast;

/// Errors raised by the event bus.
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    /// A storage failure.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// A serialization failure.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Current wall-clock time in milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Return `true` if an event `kind` matches a subscription `topic`.
///
/// Matching is prefix-based: a subscription to `kernel.` receives every
/// `kernel.*` event, and an exact topic receives exactly that kind.
pub fn topic_matches(kind: &str, topic: &str) -> bool {
    kind == topic || kind.starts_with(topic)
}

/// The kernel event bus: persistence plus in-process fan-out.
pub struct EventBus {
    store: Arc<dyn Storage>,
    sender: broadcast::Sender<Event>,
    generator: Mutex<ulid::Generator>,
}

impl EventBus {
    /// Create a bus over `store`, with an in-process broadcast buffer of
    /// `capacity` events per subscriber.
    pub fn new(store: Arc<dyn Storage>, capacity: usize) -> Self {
        let (sender, _rx) = broadcast::channel(capacity.max(1));
        Self {
            store,
            sender,
            generator: Mutex::new(ulid::Generator::new()),
        }
    }

    /// Generate the next monotonic ULID.
    fn next_id(&self) -> String {
        let mut generator = self.generator.lock().expect("ulid generator poisoned");
        generator
            .generate()
            .map(|ulid| ulid.to_string())
            .unwrap_or_else(|_| ulid::Ulid::new().to_string())
    }

    /// Persist the event and broadcast it to matching subscribers.
    pub fn emit(&self, event: Event) -> Result<Event, EventError> {
        let payload = serde_json::to_string(&event.payload)?;
        self.store.execute(
            "INSERT INTO events \
             (id, ts, kind, source, correlation_id, brain_id, project_id, payload, version) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            &[
                Value::String(event.id.clone()),
                Value::from(event.ts),
                Value::String(event.kind.clone()),
                Value::String(event.source.clone()),
                option_to_value(event.correlation_id.clone()),
                option_to_value(event.brain_id.clone()),
                option_to_value(event.project_id.clone()),
                Value::String(payload),
                Value::from(event.version),
            ],
        )?;
        // A send error only means there are no active receivers; that is fine.
        let _ = self.sender.send(event.clone());
        Ok(event)
    }

    /// Build and emit an event with a fresh id and the current timestamp.
    pub fn publish(
        &self,
        kind: impl Into<String>,
        source: impl Into<String>,
        payload: Value,
    ) -> Result<Event, EventError> {
        let event = Event {
            id: self.next_id(),
            ts: now_ms(),
            kind: kind.into(),
            source: source.into(),
            correlation_id: None,
            brain_id: None,
            project_id: None,
            payload,
            version: 1,
        };
        self.emit(event)
    }

    /// Subscribe to the live stream. The caller filters with
    /// [`Subscription::recv`].
    pub fn subscribe(&self) -> Subscription {
        Subscription {
            rx: self.sender.subscribe(),
            topics: Vec::new(),
        }
    }

    /// Subscribe with a set of topic filters.
    pub fn subscribe_topics(&self, topics: Vec<String>) -> Subscription {
        Subscription {
            rx: self.sender.subscribe(),
            topics,
        }
    }

    /// Read persisted events, optionally after `from_id`, filtered by topics.
    ///
    /// Results are ordered by monotonic ULID `id`.
    pub fn replay(
        &self,
        topics: &[String],
        from_id: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<Event>, EventError> {
        let rows = self.store.query("SELECT * FROM events ORDER BY id", &[])?;
        let mut out = Vec::new();
        for row in rows {
            let Some(event) = row_to_event(&row)? else {
                continue;
            };
            if let Some(from) = from_id {
                if event.id.as_str() <= from {
                    continue;
                }
            }
            if !topics.is_empty() && !topics.iter().any(|t| topic_matches(&event.kind, t)) {
                continue;
            }
            out.push(event);
            if let Some(limit) = limit {
                if limit > 0 && out.len() as i64 >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }
}

fn option_to_value(value: Option<String>) -> Value {
    value.map_or(Value::Null, Value::String)
}

fn row_to_event(row: &fergents_storage::Row) -> Result<Option<Event>, EventError> {
    let get_str = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
    let Some(id) = get_str("id") else {
        return Ok(None);
    };
    let payload = match get_str("payload") {
        Some(raw) => serde_json::from_str(&raw)?,
        None => Value::Null,
    };
    Ok(Some(Event {
        id,
        ts: row.get("ts").and_then(Value::as_i64).unwrap_or(0),
        kind: get_str("kind").unwrap_or_default(),
        source: get_str("source").unwrap_or_default(),
        correlation_id: get_str("correlation_id"),
        brain_id: get_str("brain_id"),
        project_id: get_str("project_id"),
        payload,
        version: row
            .get("version")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .try_into()
            .unwrap_or(1),
    }))
}

/// A live subscription to the event bus.
pub struct Subscription {
    rx: broadcast::Receiver<Event>,
    topics: Vec<String>,
}

impl Subscription {
    /// Receive the next event, waiting if necessary.
    ///
    /// On lag (the subscriber fell behind and events were dropped), a synthetic
    /// `events.dropped` event is returned carrying the [`EventsDroppedNotice`].
    pub async fn recv(&mut self) -> Result<Event, EventError> {
        loop {
            match self.rx.recv().await {
                Ok(event) => {
                    if self.topics.is_empty()
                        || self.topics.iter().any(|t| topic_matches(&event.kind, t))
                    {
                        return Ok(event);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(dropped)) => {
                    return Ok(dropped_notice(dropped, None));
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(EventError::Storage(StorageError::Corrupt(
                        "event bus closed".to_string(),
                    )));
                }
            }
        }
    }
}

/// Build the synthetic `events.dropped` notice.
pub fn dropped_notice(dropped: u64, last_seen_id: Option<String>) -> Event {
    let notice = EventsDroppedNotice {
        dropped: dropped as i64,
        last_seen_id,
    };
    Event {
        id: ulid::Ulid::new().to_string(),
        ts: now_ms(),
        kind: "events.dropped".to_string(),
        source: "kernel".to_string(),
        correlation_id: None,
        brain_id: None,
        project_id: None,
        payload: serde_json::to_value(notice).unwrap_or_else(|_| json!({ "dropped": dropped })),
        version: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fergents_storage::SqliteStorage;
    use std::path::PathBuf;

    fn migrations_dir() -> PathBuf {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            let candidate = dir.join("migrations");
            if candidate.is_dir() {
                return candidate;
            }
            assert!(dir.pop(), "migrations dir not found");
        }
    }

    fn bus(capacity: usize) -> EventBus {
        let storage = SqliteStorage::open_in_memory(&migrations_dir()).expect("open");
        EventBus::new(Arc::new(storage), capacity)
    }

    #[tokio::test]
    async fn publish_is_received_by_subscriber() {
        let bus = bus(16);
        let mut sub = bus.subscribe();
        bus.publish("kernel.started", "kernel", json!({})).unwrap();
        let event = sub.recv().await.unwrap();
        assert_eq!(event.kind, "kernel.started");
    }

    #[tokio::test]
    async fn topic_filtering_applies() {
        let bus = bus(16);
        let mut sub = bus.subscribe_topics(vec!["kernel.".to_string()]);
        bus.publish("file.changed", "indexer", json!({})).unwrap();
        bus.publish("kernel.stopping", "kernel", json!({})).unwrap();
        let event = sub.recv().await.unwrap();
        assert_eq!(event.kind, "kernel.stopping");
    }

    #[tokio::test]
    async fn correlation_order_is_preserved() {
        let bus = bus(16);
        let mut sub = bus.subscribe();
        for i in 0..3 {
            let mut e = bus
                .publish(format!("step.{i}"), "kernel", json!({ "i": i }))
                .unwrap();
            e.correlation_id = Some("corr-1".to_string());
        }
        let mut kinds = Vec::new();
        for _ in 0..3 {
            kinds.push(sub.recv().await.unwrap().kind);
        }
        assert_eq!(kinds, vec!["step.0", "step.1", "step.2"]);
    }

    #[tokio::test]
    async fn events_persist_across_restart() {
        let db = tempfile::NamedTempFile::new().unwrap();
        let path = db.path().to_path_buf();
        {
            let storage = SqliteStorage::open(&path, &migrations_dir()).unwrap();
            let bus = EventBus::new(Arc::new(storage), 16);
            bus.publish("kernel.started", "kernel", json!({})).unwrap();
        }
        let storage = SqliteStorage::open(&path, &migrations_dir()).unwrap();
        let bus = EventBus::new(Arc::new(storage), 16);
        let events = bus.replay(&[], None, None).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "kernel.started");
    }

    #[tokio::test]
    async fn replay_from_id_and_topic() {
        let bus = bus(16);
        let first = bus.publish("kernel.started", "kernel", json!({})).unwrap();
        bus.publish("kernel.stopping", "kernel", json!({})).unwrap();
        let rest = bus.replay(&[], Some(&first.id), None).unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].kind, "kernel.stopping");
        let kernel_only = bus.replay(&["kernel.".to_string()], None, None).unwrap();
        assert_eq!(kernel_only.len(), 2);
    }

    #[tokio::test]
    async fn slow_subscriber_gets_dropped_notice() {
        let bus = bus(4);
        let mut sub = bus.subscribe();
        for i in 0..50 {
            bus.publish("telemetry.tick", "kernel", json!({ "i": i }))
                .unwrap();
        }
        let event = sub.recv().await.unwrap();
        assert_eq!(event.kind, "events.dropped");
        assert!(
            event
                .payload
                .get("dropped")
                .and_then(Value::as_i64)
                .unwrap()
                > 0
        );
    }
}
