//! Serde round-trip tests for the Phase 1 `search.query` and `graph.*` protocol
//! types (protocol/schema/protocol.schema.json).

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
fn search_query_params_match_the_doc_example() {
    // SEARCH.md section 20, verbatim request.
    assert_roundtrip::<SearchQueryParams>(json!({
        "query": "MCP security",
        "brains": ["personal"],
        "mode": "hybrid",
        "top_k": 8,
        "filters": { "tags": ["research"], "after": "2025-01-01" },
        "group_by": "document",
        "rerank": true,
        "explain": false
    }));
    assert_roundtrip::<SearchMode>(json!("vector"));
    assert_roundtrip::<SearchGroupBy>(json!("chunk"));
}

#[test]
fn search_result_matches_the_doc_example() {
    // SEARCH.md section 11.1, verbatim result shape.
    assert_roundtrip::<SearchResult>(json!({
        "chunk_id": "c1",
        "document_id": "d1",
        "path": "notes/mcp-security.md",
        "title": "MCP Security",
        "heading_path": "MCP Security > Threats > Prompt Injection",
        "content": "...",
        "snippet": "...matched <mark>text</mark>...",
        "score": 0.87,
        "scores": { "bm25": 0.42, "vector": 0.91, "sparse": 0.55, "fusion": 0.78, "rerank": 0.87 },
        "metadata": { "tags": ["research"], "created": "2025-10-01", "brain": "personal" }
    }));
}

#[test]
fn search_query_result_with_meta_roundtrip() {
    assert_roundtrip::<SearchQueryResult>(json!({
        "results": [],
        "meta": {
            "latency_ms": 142,
            "candidates": { "bm25": 100, "vector": 100 },
            "cache": { "query": false, "embedding": true, "rerank": false }
        }
    }));
    assert_roundtrip::<SearchExecutedPayload>(
        json!({"query": "MCP", "results": 3, "latency_ms": 12}),
    );
}

#[test]
fn graph_query_roundtrip() {
    assert_roundtrip::<GraphQueryParams>(
        json!({"query": "?[name, count] := ...", "language": "datalog", "brain": "personal"}),
    );
    assert_roundtrip::<GraphQueryResult>(json!({"rows": [{"name": "Ada", "count": 3}]}));
}

#[test]
fn graph_algo_roundtrip() {
    assert_roundtrip::<GraphAlgoParams>(
        json!({"algo": "pagerank", "brain": "personal", "top_k": 20}),
    );
    assert_roundtrip::<GraphAlgoResult>(json!({"results": [{"id": "e1", "score": 0.9}]}));
}

#[test]
fn graph_neighbors_and_merge_roundtrip() {
    assert_roundtrip::<GraphNeighborsResult>(json!({
        "entities": [{"id": "e1", "name": "Ada", "type": "person"}],
        "relations": [{"id": "r1", "src": "e1", "dst": "e2", "kind": "works_with", "confidence": 0.8}]
    }));
    assert_roundtrip::<GraphEntitiesMergeParams>(json!({"source_id": "e2", "target_id": "e1"}));
}

#[test]
fn phase1_search_graph_types_tolerate_unknown_fields() {
    // Rule 3: additive schema; unknown fields must not break parsing.
    let params: GraphAlgoParams = serde_json::from_value(json!({"algo": "cycles", "future": 1}))
        .expect("new Phase 1 types tolerate unknown fields");
    assert_eq!(params.algo, "cycles");
}
