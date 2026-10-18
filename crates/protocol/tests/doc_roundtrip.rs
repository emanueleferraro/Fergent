//! Serde round-trip tests for the Phase 1 `doc.*`, `links.*`, `tag.*`,
//! `daily.*`, `template.*`, and `index.status` protocol types
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
fn document_kind_and_document_roundtrip() {
    assert_roundtrip::<DocumentKind>(json!("note"));
    assert_roundtrip::<Document>(json!({
        "id": "01J8K",
        "path": "notes/mcp.md",
        "kind": "note",
        "title": "MCP Security",
        "word_count": 320,
        "created_at": 1,
        "updated_at": 2
    }));
}

#[test]
fn link_ref_and_kind_roundtrip() {
    assert_roundtrip::<LinkKind>(json!("cross_brain"));
    assert_roundtrip::<LinkRef>(json!({
        "src_document_id": "01J8K",
        "dst_path": "notes/target",
        "anchor": "See the note.",
        "kind": "wiki",
        "unresolved": true
    }));
}

#[test]
fn outline_entries_roundtrip() {
    assert_roundtrip::<DocOutlineResult>(json!({
        "outline": [
            {"level": 1, "text": "Findings"},
            {"level": 2, "text": "Details"}
        ]
    }));
}

#[test]
fn tag_and_template_roundtrip() {
    assert_roundtrip::<Tag>(json!({"id": "01J8T", "name": "research/mcp"}));
    assert_roundtrip::<TemplateRenderParams>(json!({"name": "daily", "title": "Today"}));
    assert_roundtrip::<TemplateRenderResult>(json!({"content": "# Today\n"}));
}

#[test]
fn daily_and_index_roundtrip() {
    assert_roundtrip::<DailyOpenResult>(json!({
        "document": {
            "id": "01J8D",
            "path": "daily/2025-10-14.md",
            "kind": "note",
            "title": "2025-10-14",
            "word_count": 0,
            "created_at": 1,
            "updated_at": 1
        },
        "created": true
    }));
    assert_roundtrip::<IndexStatusResult>(
        json!({"brain_id": "b1", "indexing": true, "queued": 3, "running": 1, "attempts": 0}),
    );
}

#[test]
fn doc_backlinks_and_unlinked_mentions_roundtrip() {
    assert_roundtrip::<DocBacklinksResult>(json!({"backlinks": []}));
    assert_roundtrip::<DocUnlinkedMentionsResult>(json!({
        "mentions": [{"document_id": "01J8K", "path": "notes/a.md", "title": "A"}]
    }));
}

#[test]
fn phase1_doc_types_tolerate_unknown_fields() {
    // Rule 3: additive schema; unknown fields must not break parsing.
    let params: DocGetParams = serde_json::from_value(json!({"document_id": "01J8K", "future": 1}))
        .expect("new Phase 1 types tolerate unknown fields");
    assert_eq!(params.document_id, "01J8K");
}
