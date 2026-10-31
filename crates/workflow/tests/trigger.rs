//! The trigger runtime: each of the six kinds starts exactly one run, bad
//! webhook credentials stay undiscoverable, and a cron restart never replays a
//! window (spec §2, task 5.3).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{TimeZone, Utc};
use fergents_events::EventBus;
use fergents_protocol::Event;
use fergents_storage::{SqliteStorage, Storage};
use fergents_workflow::document::{CatchUp, FileEvent};
use fergents_workflow::executor::Cancellation;
use fergents_workflow::trigger::{
    fire_ai, fire_manual, test_trigger, CronTrigger, CronTriggerConfig, EventTrigger,
    EventTriggerConfig, FileTrigger, FileTriggerConfig, FireOutcome, MemoryTokenStore, TriggerFire,
    TriggerRegistry, WebhookListener, WebhookRequest, WebhookRouter,
};
use fergents_workflow::{WorkflowDocument, WorkflowError};
use serde_json::{json, Value};

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn storage() -> Arc<dyn Storage> {
    Arc::new(SqliteStorage::open_in_memory(&migrations_dir()).expect("migrations apply"))
}

/// Records every fire and hands back a numbered run id.
#[derive(Default)]
struct Recorder {
    fires: Mutex<Vec<TriggerFire>>,
}

impl Recorder {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn fire_fn(self: &Arc<Self>) -> fergents_workflow::trigger::FireFn {
        let recorder = Arc::clone(self);
        Arc::new(move |fire: TriggerFire| -> Result<String, WorkflowError> {
            let mut fires = recorder.fires.lock().expect("recorder poisoned");
            fires.push(fire);
            Ok(format!("run-{}", fires.len()))
        })
    }

    fn len(&self) -> usize {
        self.fires.lock().expect("recorder poisoned").len()
    }

    fn take(&self) -> Vec<TriggerFire> {
        self.fires.lock().expect("recorder poisoned").clone()
    }
}

fn document(yaml: &str) -> WorkflowDocument {
    WorkflowDocument::from_yaml(yaml).expect("document parses")
}

/// A UTC instant, so cron assertions never depend on the machine's zone.
fn utc(month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    Utc.with_ymd_and_hms(2026, month, day, hour, minute, 0)
        .single()
        .expect("valid instant")
        .timestamp_millis()
}

// ---------------------------------------------------------------------------
// Cron (§2.2)
// ---------------------------------------------------------------------------

const CRON_DOC: &str = r#"
id: nightly
name: Nightly
triggers:
  - { id: nightly, kind: cron, schedule: "0 9 * * *", timezone: UTC }
"#;

fn cron_config(catch_up: CatchUp) -> CronTriggerConfig {
    CronTriggerConfig {
        workflow_id: "nightly".to_string(),
        trigger_id: "nightly".to_string(),
        schedule: "0 9 * * *".to_string(),
        timezone: Some("UTC".to_string()),
        catch_up,
        concurrency: None,
    }
}

fn cron_trigger(
    config: CronTriggerConfig,
    registry: &Arc<TriggerRegistry>,
    clock: &Arc<fergents_workflow::trigger::ManualClock>,
    recorder: &Arc<Recorder>,
) -> CronTrigger {
    let clock: Arc<dyn fergents_workflow::trigger::Clock> = clock.clone();
    CronTrigger::new(
        config,
        "UTC",
        Arc::clone(registry),
        clock,
        recorder.fire_fn(),
    )
    .expect("trigger builds")
}

#[test]
fn cron_fires_exactly_one_run_per_due_window() {
    let registry = Arc::new(TriggerRegistry::new(storage()));
    let clock = Arc::new(fergents_workflow::trigger::ManualClock::at(utc(
        3, 1, 8, 30,
    )));
    let recorder = Recorder::new();
    let trigger = cron_trigger(cron_config(CatchUp::Skip), &registry, &clock, &recorder);

    let next = trigger.arm().expect("armed").expect("a next fire");
    assert_eq!(next, utc(3, 1, 9, 0));

    // Nothing is due before the armed instant.
    assert_eq!(trigger.tick().expect("tick"), 0);
    assert_eq!(recorder.len(), 0);

    clock.set(next);
    assert_eq!(trigger.tick().expect("tick"), 1);

    let fires = recorder.take();
    assert_eq!(fires.len(), 1);
    assert_eq!(fires[0].kind, "cron");
    assert_eq!(fires[0].workflow_id, "nightly");
    assert_eq!(fires[0].trigger_id, "nightly");
    assert_eq!(fires[0].actor, "trigger:nightly");
    assert_eq!(fires[0].inputs["scheduled_at"], json!(next));

    // The next fire is already persisted, so the same tick does not repeat.
    assert_eq!(trigger.tick().expect("tick"), 0);
    assert_eq!(recorder.len(), 1);
}

#[test]
fn cron_restart_with_run_once_does_not_replay_missed_windows() {
    let registry = Arc::new(TriggerRegistry::new(storage()));
    let clock = Arc::new(fergents_workflow::trigger::ManualClock::at(utc(
        3, 1, 8, 30,
    )));
    let recorder = Recorder::new();
    let trigger = cron_trigger(cron_config(CatchUp::RunOnce), &registry, &clock, &recorder);
    trigger.arm().expect("armed");

    // The kernel was down for three days: four windows are due, one is run.
    clock.set(utc(3, 4, 8, 30));
    assert_eq!(trigger.tick().expect("tick"), 1);
    assert_eq!(recorder.len(), 1);

    let row = registry
        .get("nightly", "nightly")
        .expect("registry readable")
        .expect("row exists");
    assert_eq!(row.next_fire_at, Some(utc(3, 4, 9, 0)));

    // A restart re-arms from the persisted instant and fires nothing.
    let restarted = cron_trigger(cron_config(CatchUp::RunOnce), &registry, &clock, &recorder);
    assert_eq!(restarted.arm().expect("armed"), Some(utc(3, 4, 9, 0)));
    assert_eq!(restarted.tick().expect("tick"), 0);
    assert_eq!(recorder.len(), 1);
}

#[test]
fn cron_skip_drops_a_missed_window() {
    let registry = Arc::new(TriggerRegistry::new(storage()));
    let clock = Arc::new(fergents_workflow::trigger::ManualClock::at(utc(
        3, 1, 8, 30,
    )));
    let recorder = Recorder::new();
    let trigger = cron_trigger(cron_config(CatchUp::Skip), &registry, &clock, &recorder);
    trigger.arm().expect("armed");

    clock.set(utc(3, 4, 8, 30));
    assert_eq!(trigger.tick().expect("tick"), 0);
    assert_eq!(recorder.len(), 0);

    let row = registry
        .get("nightly", "nightly")
        .expect("registry readable")
        .expect("row exists");
    assert_eq!(row.next_fire_at, Some(utc(3, 4, 9, 0)));
}

#[test]
fn the_registry_keeps_state_across_a_document_sync() {
    let registry = TriggerRegistry::new(storage());
    let doc = document(CRON_DOC);

    let rows = registry.sync(&doc).expect("synced");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, "cron");

    registry
        .set_enabled("nightly", "nightly", false)
        .expect("disabled");
    registry
        .mark_fired("nightly", "nightly", 42)
        .expect("marked");

    let rows = registry.sync(&doc).expect("resynced");
    assert!(!rows[0].enabled);
    assert_eq!(rows[0].last_fired_at, Some(42));

    let without = document("id: nightly\nname: Nightly\n");
    assert!(registry.sync(&without).expect("synced").is_empty());
}

// ---------------------------------------------------------------------------
// Webhooks (§2.3)
// ---------------------------------------------------------------------------

const HOOK_DOC: &str = r#"
id: hooky
name: Hooky
triggers:
  - { id: hook, kind: webhook, path: /in }
"#;

fn build_router(
    tokens: Arc<MemoryTokenStore>,
    recorder: &Arc<Recorder>,
    now: i64,
) -> WebhookRouter {
    let mut router = WebhookRouter::with_clock(
        tokens,
        recorder.fire_fn(),
        Arc::new(fergents_workflow::trigger::ManualClock::at(now)),
    );
    assert_eq!(router.add_workflow(&document(HOOK_DOC)), 1);
    router
}

fn tokens() -> Arc<MemoryTokenStore> {
    let tokens = Arc::new(MemoryTokenStore::new());
    tokens.set("hooky", "hook", "s3cret");
    tokens
}

fn request(method: &str, path: &str, token: Option<&str>) -> WebhookRequest {
    let mut headers = BTreeMap::new();
    if let Some(token) = token {
        headers.insert("authorization".to_string(), format!("Bearer {token}"));
    }
    headers.insert("content-type".to_string(), "application/json".to_string());
    WebhookRequest {
        method: method.to_string(),
        path: path.to_string(),
        headers,
        body: br#"{"event":"push"}"#.to_vec(),
    }
}

fn rejected(outcome: &FireOutcome) -> (u16, &'static str) {
    match outcome {
        FireOutcome::Rejected { status, code } => (*status, code),
        other => panic!("expected a rejection, got {other:?}"),
    }
}

#[test]
fn a_valid_webhook_starts_one_run_with_body_and_headers() {
    let recorder = Recorder::new();
    let router = build_router(tokens(), &recorder, 1_000);

    let outcome = router.dispatch(&request("POST", "/hooks/hooky/hook", Some("s3cret")));
    assert_eq!(outcome, FireOutcome::Started("run-1".to_string()));

    let response = router.handle(&request("POST", "/hooks/hooky/hook", Some("s3cret")));
    assert_eq!(response.status, 202);
    assert_eq!(response.body, json!({ "run_id": "run-2" }));

    let fires = recorder.take();
    assert_eq!(fires.len(), 2);
    assert_eq!(fires[0].kind, "webhook");
    assert_eq!(fires[0].actor, "trigger:hook");
    assert_eq!(fires[0].inputs["body"], json!({ "event": "push" }));
    assert_eq!(
        fires[0].inputs["headers"]["authorization"],
        json!("Bearer s3cret")
    );
}

#[test]
fn a_bad_token_is_not_found_and_starts_no_run() {
    let recorder = Recorder::new();
    let router = build_router(tokens(), &recorder, 1_000);

    let wrong = router.dispatch(&request("POST", "/hooks/hooky/hook", Some("guess")));
    assert_eq!(rejected(&wrong), (404, "not_found"));

    let missing = router.dispatch(&request("POST", "/hooks/hooky/hook", None));
    assert_eq!(rejected(&missing), (404, "not_found"));

    let unknown_route = router.dispatch(&request("POST", "/hooks/hooky/other", Some("s3cret")));
    assert_eq!(rejected(&unknown_route), (404, "not_found"));

    let wrong_method = router.dispatch(&request("GET", "/hooks/hooky/hook", Some("s3cret")));
    assert_eq!(rejected(&wrong_method), (404, "not_found"));

    assert_eq!(recorder.len(), 0);

    // A trigger without a token is undiscoverable too, not unauthenticated.
    let bare = Arc::new(MemoryTokenStore::new());
    let bare_router = build_router(bare, &recorder, 1_000);
    let outcome = bare_router.dispatch(&request("POST", "/hooks/hooky/hook", Some("anything")));
    assert_eq!(rejected(&outcome), (404, "not_found"));
    assert_eq!(recorder.len(), 0);
}

#[test]
fn a_repeated_idempotency_key_returns_the_first_run() {
    let recorder = Recorder::new();
    let router = build_router(tokens(), &recorder, 1_000);

    let mut first = request("POST", "/hooks/hooky/hook", Some("s3cret"));
    first
        .headers
        .insert("idempotency-key".to_string(), "abc".to_string());
    assert_eq!(
        router.dispatch(&first),
        FireOutcome::Started("run-1".to_string())
    );

    let mut again = request("POST", "/hooks/hooky/hook", Some("s3cret"));
    again
        .headers
        .insert("idempotency-key".to_string(), "abc".to_string());
    assert_eq!(
        router.dispatch(&again),
        FireOutcome::Duplicate("run-1".to_string())
    );
    assert_eq!(recorder.len(), 1);

    // The response tells the caller the run already exists.
    assert_eq!(
        router.handle(&again).body,
        json!({ "run_id": "run-1", "duplicate": true })
    );
}

#[test]
fn the_rate_limit_answers_429_without_starting_a_run() {
    let recorder = Recorder::new();
    let router = build_router(tokens(), &recorder, 1_000);

    for _ in 0..60 {
        let outcome = router.dispatch(&request("POST", "/hooks/hooky/hook", Some("s3cret")));
        assert!(matches!(outcome, FireOutcome::Started(_)));
    }
    let limited = router.dispatch(&request("POST", "/hooks/hooky/hook", Some("s3cret")));
    assert_eq!(rejected(&limited), (429, "rate_limited"));
    assert_eq!(recorder.len(), 60);
}

#[test]
fn a_full_queue_answers_429_with_the_workflow_code() {
    let fire: fergents_workflow::trigger::FireFn =
        Arc::new(|_| Err(WorkflowError::QueueFull("32 queued".to_string())));
    let mut router = WebhookRouter::new(
        tokens() as Arc<dyn fergents_workflow::trigger::TokenStore>,
        fire,
    );
    router.add_workflow(&document(HOOK_DOC));

    let outcome = router.dispatch(&request("POST", "/hooks/hooky/hook", Some("s3cret")));
    assert_eq!(rejected(&outcome), (429, "workflow_queue_full"));
}

#[tokio::test]
async fn the_loopback_listener_answers_over_http() {
    let recorder = Recorder::new();
    let router = Arc::new(build_router(tokens(), &recorder, 1_000));
    let listener = WebhookListener::bind("127.0.0.1:0", Arc::clone(&router))
        .await
        .expect("listener binds");
    let addr = listener.local_addr().expect("bound address");

    let cancel = Cancellation::new();
    let serving = tokio::spawn(listener.serve(cancel.clone()));

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connects to the loopback listener");
    stream
        .write_all(
            b"POST /hooks/hooky/hook HTTP/1.1\r\nHost: localhost\r\n\
              Authorization: Bearer s3cret\r\nContent-Length: 16\r\n\r\n{\"event\":\"push\"}",
        )
        .await
        .expect("request written");

    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .expect("response arrives")
        .expect("response reads");

    let text = String::from_utf8_lossy(&response);
    assert!(text.starts_with("HTTP/1.1 202 Accepted"), "got {text}");
    assert!(text.contains("\"run_id\":\"run-1\""), "got {text}");
    assert_eq!(recorder.len(), 1);

    cancel.cancel();
    let _ = serving.await;
}

// ---------------------------------------------------------------------------
// Files (§2.4)
// ---------------------------------------------------------------------------

#[test]
fn file_fires_once_per_coalesced_burst() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
    let clock = Arc::new(fergents_workflow::trigger::ManualClock::at(1_000));
    let recorder = Recorder::new();

    let trigger = FileTrigger::start_with_clock(
        FileTriggerConfig::new("wf", "watch", dir.path(), "src/**/*.rs", Vec::new()),
        Arc::clone(&clock) as Arc<dyn fergents_workflow::trigger::Clock>,
        recorder.fire_fn(),
    )
    .expect("watcher starts");

    let inside = dir.path().join("src").join("main.rs");
    assert!(trigger
        .observe(&inside, FileEvent::Create)
        .expect("observed"));
    assert_eq!(recorder.len(), 1);

    let fires = recorder.take();
    assert_eq!(fires[0].kind, "file");
    assert_eq!(fires[0].actor, "trigger:watch");
    assert_eq!(fires[0].inputs["path"], json!("src/main.rs"));
    assert_eq!(fires[0].inputs["event"], json!("create"));

    // A burst inside the coalescing window is one run.
    assert!(!trigger
        .observe(&inside, FileEvent::Modify)
        .expect("observed"));
    assert_eq!(recorder.len(), 1);

    clock.advance(2_000);
    assert!(trigger
        .observe(&inside, FileEvent::Modify)
        .expect("observed"));
    assert_eq!(recorder.len(), 2);
    assert_eq!(recorder.take()[1].inputs["event"], json!("modify"));
}

#[test]
fn file_ignores_other_events_other_globs_and_escapes() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join(".git")).expect(".git dir");
    std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("secret.rs"), b"x").expect("outside file");

    let clock = Arc::new(fergents_workflow::trigger::ManualClock::at(1_000));
    let recorder = Recorder::new();
    let trigger = FileTrigger::start_with_clock(
        FileTriggerConfig::new("wf", "watch", dir.path(), "**/*", vec![FileEvent::Create]),
        Arc::clone(&clock) as Arc<dyn fergents_workflow::trigger::Clock>,
        recorder.fire_fn(),
    )
    .expect("watcher starts");

    // Declared event kinds only.
    assert!(!trigger
        .observe(&dir.path().join("src").join("a.rs"), FileEvent::Delete)
        .expect("observed"));

    // Ignored directories never fire.
    assert!(!trigger
        .observe(&dir.path().join(".git").join("config"), FileEvent::Create)
        .expect("observed"));

    // A path that resolves outside the root never fires.
    assert!(!trigger
        .observe(&outside.path().join("secret.rs"), FileEvent::Create)
        .expect("observed"));

    // A glob that does not match the path never fires.
    let narrow = FileTrigger::start_with_clock(
        FileTriggerConfig::new("wf", "rs", dir.path(), "src/*.rs", Vec::new()),
        Arc::clone(&clock) as Arc<dyn fergents_workflow::trigger::Clock>,
        recorder.fire_fn(),
    )
    .expect("watcher starts");
    assert!(!narrow
        .observe(
            &dir.path().join("src").join("nested").join("a.rs"),
            FileEvent::Create
        )
        .expect("observed"));

    assert_eq!(recorder.len(), 0);
}

// ---------------------------------------------------------------------------
// Events (§2.5)
// ---------------------------------------------------------------------------

fn event(id: &str, kind: &str, payload: Value) -> Event {
    Event {
        id: id.to_string(),
        ts: 0,
        kind: kind.to_string(),
        source: "test".to_string(),
        correlation_id: None,
        brain_id: None,
        project_id: None,
        payload,
        version: 1,
    }
}

fn event_trigger(topic: &str, filter: Option<&str>, recorder: &Arc<Recorder>) -> EventTrigger {
    let bus = Arc::new(EventBus::new(storage(), 32));
    EventTrigger::with_clock(
        EventTriggerConfig {
            workflow_id: "wf".to_string(),
            trigger_id: "on_event".to_string(),
            topic: topic.to_string(),
            filter: filter.map(str::to_string),
        },
        bus,
        Arc::new(fergents_workflow::trigger::ManualClock::at(1_000)),
        recorder.fire_fn(),
    )
    .expect("trigger builds")
}

#[test]
fn an_event_topic_prefix_and_filter_select_the_run() {
    let recorder = Recorder::new();
    let trigger = event_trigger("plugin.", Some("/project_id"), &recorder);

    assert!(trigger.accepts(&event(
        "e1",
        "plugin.installed",
        json!({ "project_id": "p1" })
    )));
    assert!(!trigger.accepts(&event("e2", "plugin.installed", json!({ "other": "p1" }))));
    assert!(!trigger.accepts(&event(
        "e3",
        "kernel.started",
        json!({ "project_id": "p1" })
    )));

    let started = trigger
        .observe(&event(
            "e1",
            "plugin.installed",
            json!({ "project_id": "p1" }),
        ))
        .expect("observed")
        .expect("a run started");
    assert_eq!(started, "run-1");

    let fires = recorder.take();
    assert_eq!(fires[0].kind, "event");
    assert_eq!(fires[0].actor, "trigger:on_event");
    assert_eq!(fires[0].inputs["topic"], json!("plugin.installed"));
    assert_eq!(fires[0].inputs["event_id"], json!("e1"));
    assert_eq!(fires[0].inputs["payload"]["project_id"], json!("p1"));
}

#[test]
fn a_redelivered_event_starts_one_run() {
    let recorder = Recorder::new();
    let trigger = event_trigger("plugin.", None, &recorder);
    let delivered = event("e1", "plugin.installed", json!({}));

    assert!(trigger.observe(&delivered).expect("observed").is_some());
    assert!(trigger.observe(&delivered).expect("observed").is_none());
    assert_eq!(recorder.len(), 1);

    // A different event id is a different run.
    let other = event("e2", "plugin.installed", json!({}));
    assert!(trigger.observe(&other).expect("observed").is_some());
    assert_eq!(recorder.len(), 2);
}

#[test]
fn a_workflow_never_reacts_to_its_own_events() {
    let recorder = Recorder::new();
    let trigger = event_trigger("workflow.", None, &recorder);

    let own = event("e1", "workflow.run.done", json!({ "workflow_id": "wf" }));
    assert!(!trigger.accepts(&own));
    assert!(trigger.observe(&own).expect("observed").is_none());

    let peer = event("e2", "workflow.run.done", json!({ "workflow_id": "other" }));
    assert!(trigger.accepts(&peer));
    assert!(trigger.observe(&peer).expect("observed").is_some());
    assert_eq!(recorder.len(), 1);
}

#[test]
fn an_event_trigger_needs_a_topic_and_a_json_pointer() {
    let bus = Arc::new(EventBus::new(storage(), 8));
    let recorder = Recorder::new();

    let empty = EventTrigger::new(
        EventTriggerConfig {
            workflow_id: "wf".to_string(),
            trigger_id: "t".to_string(),
            topic: "  ".to_string(),
            filter: None,
        },
        Arc::clone(&bus),
        recorder.fire_fn(),
    );
    assert!(empty.is_err());

    let bad_filter = EventTrigger::new(
        EventTriggerConfig {
            workflow_id: "wf".to_string(),
            trigger_id: "t".to_string(),
            topic: "plugin.".to_string(),
            filter: Some("project_id".to_string()),
        },
        bus,
        recorder.fire_fn(),
    );
    assert!(bad_filter.is_err());
}

// ---------------------------------------------------------------------------
// Manual and AI (§2.6)
// ---------------------------------------------------------------------------

#[test]
fn manual_and_ai_fire_record_who_asked() {
    let recorder = Recorder::new();
    let fire = recorder.fire_fn();

    fire_manual(&fire, "wf", None, "user", json!({ "a": 1 })).expect("manual fires");
    test_trigger(&fire, "wf", Some("canvas"), json!({ "b": 2 })).expect("test fires");
    fire_ai(&fire, "wf", None, "researcher", json!({ "c": 3 })).expect("ai fires");

    let fires = recorder.take();
    assert_eq!(fires.len(), 3);

    assert_eq!(fires[0].kind, "manual");
    assert_eq!(fires[0].actor, "user");
    assert_eq!(fires[0].trigger_id, "manual");
    assert_eq!(fires[0].inputs, json!({ "a": 1 }));

    assert_eq!(fires[1].kind, "manual");
    assert_eq!(fires[1].actor, "user");
    assert_eq!(fires[1].trigger_id, "canvas");
    assert_eq!(
        fires[1].inputs,
        json!({ "test": true, "inputs": { "b": 2 } })
    );

    assert_eq!(fires[2].kind, "ai");
    assert_eq!(fires[2].actor, "agent:researcher");
    assert_eq!(fires[2].inputs, json!({ "c": 3 }));
}
