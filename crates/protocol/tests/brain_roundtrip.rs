//! Serde round-trip tests for the Phase 1 `brain.*` protocol types
//! (protocol/schema/protocol.schema.json).

use fergents_protocol::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

/// Deserialize `value`, re-serialize it, and deserialize again; the two
/// in-memory values must be equal.
fn assert_roundtrip<T>(value: Value)
where
    T: DeserializeOwned + Serialize + PartialEq + std::fmt::Debug,
{
    let first: T = serde_json::from_value(value).expect("deserialize");
    let encoded = serde_json::to_value(&first).expect("serialize");
    let second: T = serde_json::from_value(encoded).expect("deserialize round-trip");
    assert_eq!(first, second);
}

#[test]
fn brain_create_params_roundtrip() {
    assert_roundtrip::<BrainCreateParams>(json!({"name": "MCP Research", "template": "research"}));
    assert_roundtrip::<BrainCreateParams>(json!({"name": "Blank"}));
}

#[test]
fn brain_list_params_roundtrip() {
    assert_roundtrip::<BrainListParams>(json!({}));
    assert_roundtrip::<BrainListParams>(json!({"include_archived": true}));
}

#[test]
fn brain_attach_params_roundtrip() {
    assert_roundtrip::<BrainAttachParams>(
        json!({"brain_id": "b1", "project_id": "p1", "role": "read-only"}),
    );
}

#[test]
fn brain_state_and_role_roundtrip() {
    assert_roundtrip::<BrainState>(json!("indexing"));
    assert_roundtrip::<BrainRole>(json!("scratch"));
}

#[test]
fn brain_get_result_roundtrip() {
    assert_roundtrip::<BrainGetResult>(json!({
        "brain": {"brain_id": "b1", "name": "Personal", "path": "/brains/personal"},
        "state": "ready",
        "config": {
            "index": {"chunk_size": 512, "chunk_overlap": 64, "watch": true},
            "search": {"default_mode": "hybrid"}
        }
    }));
}

#[test]
fn brain_export_params_and_event_payload_roundtrip() {
    assert_roundtrip::<BrainExportParams>(json!({"brain_id": "b1", "output": "/tmp/p.fergbrain"}));
    assert_roundtrip::<BrainExportedPayload>(
        json!({"brain_id": "b1", "output": "/tmp/p.fergbrain"}),
    );
}

#[test]
fn brain_events_payload_roundtrip() {
    assert_roundtrip::<BrainCreatedPayload>(
        json!({"brain": {"brain_id": "b1", "name": "Personal", "path": "/brains/personal"}}),
    );
    assert_roundtrip::<BrainClosedPayload>(json!({"brain_id": "b1"}));
}

#[test]
fn phase1_types_tolerate_unknown_fields() {
    // Rule 3: the schema is additive, so unknown future fields must not break
    // parsing of the Phase 1 types. (The Phase 0 types stay strict and are
    // migrated to permissive parsing in a separate PR.)
    let params: BrainCreateParams =
        serde_json::from_value(json!({"name": "X", "future_field": 42}))
            .expect("new Phase 1 types tolerate unknown fields");
    assert_eq!(params.name, "X");
}
