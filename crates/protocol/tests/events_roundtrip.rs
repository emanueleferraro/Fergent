//! Serde round-trip tests for the Phase 1 `file.*`, `note.*`, `index.*`,
//! `embedding.*`, `tag.updated`, and `link.updated` event payloads
//! (protocol/schema/protocol.schema.json).

use fergents_protocol::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

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
fn file_events_roundtrip() {
    assert_roundtrip::<FileCreatedPayload>(json!({"path": "notes/a.md"}));
    assert_roundtrip::<FileChangedPayload>(json!({"path": "notes/a.md"}));
    assert_roundtrip::<FileDeletedPayload>(json!({"path": "notes/a.md"}));
    assert_roundtrip::<FileRenamedPayload>(json!({"from_path": "a.md", "to_path": "b.md"}));
}

#[test]
fn note_events_roundtrip() {
    assert_roundtrip::<NoteCreatedPayload>(json!({
        "document": {
            "id": "01J8K", "path": "notes/a.md", "kind": "note",
            "title": "A", "word_count": 1, "created_at": 1, "updated_at": 1
        }
    }));
    assert_roundtrip::<NoteLinkedPayload>(json!({
        "link": {"src_document_id": "01J8K", "dst_path": "notes/b", "kind": "wiki", "unresolved": false}
    }));
}

#[test]
fn index_events_roundtrip() {
    assert_roundtrip::<IndexQueuedPayload>(
        json!({"brain_id": "b1", "path": "notes/a.md", "op": "upsert"}),
    );
    assert_roundtrip::<IndexStartedPayload>(json!({"brain_id": "b1", "path": "notes/a.md"}));
    assert_roundtrip::<IndexProgressPayload>(
        json!({"brain_id": "b1", "path": "notes/a.md", "phase": "embed", "done": 3, "total": 10}),
    );
    assert_roundtrip::<IndexCompletedPayload>(json!({"brain_id": "b1", "path": "notes/a.md"}));
    assert_roundtrip::<IndexFailedPayload>(
        json!({"brain_id": "b1", "path": "notes/a.md", "error": "parse failed"}),
    );
}

#[test]
fn index_enums_roundtrip() {
    assert_roundtrip::<IndexJobOp>(json!("reindex"));
    assert_roundtrip::<IndexPhase>(json!("extract_entities"));
}

#[test]
fn embedding_tag_link_events_roundtrip() {
    assert_roundtrip::<EmbeddingCreatedPayload>(json!({"chunk_id": "c1"}));
    assert_roundtrip::<EmbeddingFailedPayload>(json!({"chunk_id": "c1", "error": "oom"}));
    assert_roundtrip::<TagUpdatedPayload>(json!({"tag": {"id": "t1", "name": "research"}}));
    assert_roundtrip::<LinkUpdatedPayload>(json!({
        "link": {"src_document_id": "01J8K", "kind": "md", "unresolved": true}
    }));
}

#[test]
fn phase1_event_types_tolerate_unknown_fields() {
    // Rule 3: additive schema; unknown fields must not break parsing.
    let payload: FileCreatedPayload =
        serde_json::from_value(json!({"path": "a.md", "future": true}))
            .expect("new Phase 1 types tolerate unknown fields");
    assert_eq!(payload.path, "a.md");
}
