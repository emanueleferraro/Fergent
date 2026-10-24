//! Serde round-trip tests for the Phase 3 project protocol types
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

fn project() -> Value {
    json!({
        "id": "01J9PROJECT",
        "path": "/work/repo",
        "name": "repo",
        "vcs": "git",
        "brain_id": null,
        "trust": "untrusted",
        "settings": null,
        "last_opened_at": 1792694626000_i64,
        "created_at": 1792694626000_i64,
        "updated_at": 1792694626000_i64
    })
}

#[test]
fn project_roundtrip() {
    assert_roundtrip::<Project>(project());
    assert_roundtrip::<TrustState>(json!("trusted"));
    assert_roundtrip::<ProjectOpenParams>(json!({ "path": "/work/repo" }));
    assert_roundtrip::<ProjectOpenResult>(json!({ "project": project() }));
    assert_roundtrip::<ProjectListResult>(json!({ "projects": [project()] }));
    assert_roundtrip::<ProjectIdParams>(json!({ "project_id": "01J9PROJECT" }));
    assert_roundtrip::<ProjectGetResult>(json!({ "project": null }));
    assert_roundtrip::<ProjectCloseResult>(json!({ "closed": true }));
}

#[test]
fn project_mutation_roundtrip() {
    assert_roundtrip::<ProjectTrustParams>(json!({
        "project_id": "01J9PROJECT",
        "trusted": true
    }));
    assert_roundtrip::<ProjectTrustResult>(json!({ "project": project() }));
    assert_roundtrip::<ProjectUpdateParams>(json!({
        "project_id": "01J9PROJECT",
        "name": "Renamed",
        "brain_id": "01J9BRAIN",
        "settings": { "tab_size": 2 }
    }));
    assert_roundtrip::<ProjectUpdateResult>(json!({ "project": project() }));
}

#[test]
fn file_roundtrip() {
    assert_roundtrip::<FileSnapshot>(json!({
        "path": "src/main.rs",
        "content": "fn main() {}\n",
        "etag": "a".repeat(64),
        "size": 13,
        "language_id": "rust",
        "truncated": false,
        "binary": false
    }));
    assert_roundtrip::<FileEntry>(json!({
        "name": "main.rs",
        "path": "src/main.rs",
        "kind": "file",
        "size": 13
    }));
    assert_roundtrip::<FileStat>(json!({
        "path": "src/main.rs",
        "kind": "file",
        "size": 13,
        "modified_at": 1792694626000_i64,
        "etag": "a".repeat(64)
    }));
    assert_roundtrip::<FileReadParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs"
    }));
    assert_roundtrip::<FileReadResult>(json!({ "file": {
        "path": "src/main.rs",
        "content": "fn main() {}\n",
        "etag": "a".repeat(64),
        "size": 13,
        "language_id": "rust",
        "truncated": false,
        "binary": false
    }}));
}

#[test]
fn file_mutation_roundtrip() {
    assert_roundtrip::<FileWriteParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "content": "fn main() {}\n",
        "base_etag": "a".repeat(64)
    }));
    assert_roundtrip::<FileCreateParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/new.rs",
        "content": ""
    }));
    assert_roundtrip::<FilePathParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs"
    }));
    assert_roundtrip::<FileRenameParams>(json!({
        "project_id": "01J9PROJECT",
        "from": "src/main.rs",
        "to": "src/lib.rs"
    }));
    assert_roundtrip::<FileDeleteResult>(json!({ "deleted": true }));
    assert_roundtrip::<FileListParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src"
    }));
    assert_roundtrip::<FileListResult>(json!({ "entries": [{
        "name": "main.rs",
        "path": "src/main.rs",
        "kind": "file",
        "size": 13
    }]}));
    assert_roundtrip::<FileStatResult>(json!({ "entry": {
        "path": "src/main.rs",
        "kind": "file",
        "size": 13,
        "modified_at": 1792694626000_i64,
        "etag": "a".repeat(64)
    }}));
}

#[test]
fn editor_layout_roundtrip() {
    let layout = json!({
        "kind": "split",
        "direction": "row",
        "ratio": 0.5,
        "first": { "kind": "pane", "view": "chat" },
        "second": { "kind": "pane", "view": "ide" }
    });
    assert_roundtrip::<EditorLayoutGetParams>(json!({ "project_id": "01J9PROJECT" }));
    assert_roundtrip::<EditorLayoutGetResult>(json!({ "layout": layout.clone() }));
    assert_roundtrip::<EditorLayoutGetResult>(json!({ "layout": null }));
    assert_roundtrip::<EditorLayoutSetParams>(json!({
        "project_id": "01J9PROJECT",
        "layout": layout.clone()
    }));
    assert_roundtrip::<EditorLayoutSetResult>(json!({ "layout": layout }));
}

#[test]
fn project_query_roundtrip() {
    assert_roundtrip::<ProjectFilesParams>(json!({
        "project_id": "01J9PROJECT",
        "query": "main",
        "limit": 20
    }));
    assert_roundtrip::<ProjectFilesResult>(json!({
        "files": ["src/main.rs", "README.md"]
    }));
    assert_roundtrip::<ProjectSearchHit>(json!({
        "path": "src/main.rs",
        "line": 1,
        "text": "fn main() {}"
    }));
    assert_roundtrip::<ProjectSearchParams>(json!({
        "project_id": "01J9PROJECT",
        "query": "main"
    }));
    assert_roundtrip::<ProjectSearchResult>(json!({
        "hits": [{ "path": "src/main.rs", "line": 1, "text": "fn main() {}" }]
    }));
}
