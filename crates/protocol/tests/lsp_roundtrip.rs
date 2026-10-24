//! Serde round-trip tests for the Phase 3 LSP protocol types
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

fn server() -> Value {
    json!({
        "id": "01J9SERVER",
        "language_id": "rust",
        "command": "rust-analyzer",
        "args": [],
        "env": { "RUST_LOG": "error" },
        "root_strategy": "project",
        "source": "builtin",
        "enabled": true,
        "status": "ready",
        "installed": true
    })
}

#[test]
fn lsp_registry_roundtrip() {
    assert_roundtrip::<LspServerStatus>(json!("not_installed"));
    assert_roundtrip::<LspServerStatus>(json!("ready"));
    assert_roundtrip::<LspServer>(server());
    assert_roundtrip::<LspServerListResult>(json!({ "servers": [server()] }));
    assert_roundtrip::<LspServerAddParams>(json!({
        "language_id": "rust",
        "command": "rust-analyzer",
        "args": ["--log-file", "lsp.log"],
        "env": { "RUST_LOG": "error" },
        "source": "user",
        "enabled": true
    }));
    assert_roundtrip::<LspServerResult>(json!({ "server": server() }));
    assert_roundtrip::<LspServerIdParams>(json!({ "id": "01J9SERVER" }));
    assert_roundtrip::<LspServerRemoveResult>(json!({ "removed": true }));
}

#[test]
fn lsp_lifecycle_roundtrip() {
    assert_roundtrip::<LspSessionParams>(json!({
        "project_id": "01J9PROJECT",
        "language_id": "rust"
    }));
    assert_roundtrip::<LspServerStopResult>(json!({ "stopped": false }));
    assert_roundtrip::<LspAck>(json!({ "ok": true }));
}

#[test]
fn lsp_document_roundtrip() {
    assert_roundtrip::<LspDocumentOpenParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "text": "fn main() {}\n",
        "language_id": null
    }));
    assert_roundtrip::<LspDocumentChangeParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "text": "fn main() { }\n",
        "range": null
    }));
    assert_roundtrip::<LspDocumentChangeResult>(json!({ "version": 2 }));
    assert_roundtrip::<LspDocumentPathParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "text": null
    }));
}

#[test]
fn lsp_request_roundtrip() {
    assert_roundtrip::<LspPositionParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "line": 3,
        "character": 7
    }));
    assert_roundtrip::<LspRenameParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "line": 3,
        "character": 7,
        "new_name": "other"
    }));
    assert_roundtrip::<LspResolveParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "item": { "label": "fake_item" }
    }));
    assert_roundtrip::<LspWorkspaceSymbolsParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "query": "main"
    }));
    assert_roundtrip::<LspFormatParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "tab_size": 2,
        "insert_spaces": true
    }));
    assert_roundtrip::<LspCodeActionParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs",
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        },
        "diagnostics": []
    }));
    assert_roundtrip::<LspRequestResult>(json!({ "result": null }));
    assert_roundtrip::<LspRequestResult>(json!({
        "result": { "contents": { "kind": "markdown", "value": "hi" } }
    }));
}

#[test]
fn lsp_diagnostics_roundtrip() {
    let diagnostic = json!({
        "path": "src/main.rs",
        "line": 0,
        "column": 1,
        "end_line": 0,
        "end_column": 4,
        "severity": 2,
        "message": "fake warning",
        "source": "fake",
        "code": "F001"
    });
    assert_roundtrip::<LspDiagnostic>(diagnostic.clone());
    assert_roundtrip::<LspDiagnosticsParams>(json!({
        "project_id": "01J9PROJECT",
        "path": "src/main.rs"
    }));
    assert_roundtrip::<LspDiagnosticsResult>(json!({ "diagnostics": [diagnostic] }));
}
