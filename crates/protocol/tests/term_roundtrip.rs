//! Serde round-trip tests for the Phase 3 terminal protocol types
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

fn session() -> Value {
    json!({
        "id": "01J9TERM",
        "project_id": "01J9PROJECT",
        "shell": "bash",
        "cwd": "/work/repo",
        "cols": 120,
        "rows": 40,
        "state": "running",
        "exit_code": null,
        "actor": "user",
        "created_at": 1792694626000_i64,
        "closed_at": null
    })
}

#[test]
fn term_session_roundtrip() {
    assert_roundtrip::<TermSessionState>(json!("running"));
    assert_roundtrip::<TermSessionState>(json!("closed"));
    assert_roundtrip::<TermSession>(session());
    assert_roundtrip::<TermSessionResult>(json!({ "session": session() }));
    assert_roundtrip::<TermListResult>(json!({ "sessions": [session()] }));
    let mut exited = session();
    exited["state"] = json!("exited");
    exited["exit_code"] = json!(0);
    exited["closed_at"] = json!(1792694726000_i64);
    assert_roundtrip::<TermSession>(exited);
}

#[test]
fn term_params_roundtrip() {
    assert_roundtrip::<TermOpenParams>(json!({
        "project_id": "01J9PROJECT",
        "shell": null,
        "cwd": "src",
        "cols": 100,
        "rows": 30,
        "actor": "user"
    }));
    assert_roundtrip::<TermIdParams>(json!({ "session_id": "01J9TERM" }));
    assert_roundtrip::<TermWriteParams>(json!({
        "session_id": "01J9TERM",
        "data": "ls\r"
    }));
    assert_roundtrip::<TermResizeParams>(json!({
        "session_id": "01J9TERM",
        "cols": 120,
        "rows": 40
    }));
    assert_roundtrip::<TermAck>(json!({ "ok": true }));
}

#[test]
fn term_agent_surface_roundtrip() {
    assert_roundtrip::<TermListParams>(json!({ "project_id": null }));
    assert_roundtrip::<TermReadParams>(json!({
        "session_id": "01J9TERM",
        "max_lines": 200,
        "max_bytes": 8192
    }));
    assert_roundtrip::<TermReadResult>(json!({ "output": "PASSWORD=[redacted]" }));
    assert_roundtrip::<TermExecParams>(json!({
        "project_id": "01J9PROJECT",
        "command": "cargo test",
        "actor": "agent:test"
    }));
}
