//! Permission resolution: modes, policy precedence, overrides, and audit.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fergents_events::EventBus;
use fergents_permissions::{
    Category, Decision, Mode, PermissionRequest, PermissionService, Policy,
};
use fergents_storage::{SqliteStorage, Storage};
use serde_json::json;

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations")
}

fn service() -> (
    tempfile::TempDir,
    Arc<SqliteStorage>,
    Arc<PermissionService>,
) {
    let dir = tempfile::tempdir().unwrap();
    let storage =
        Arc::new(SqliteStorage::open(&dir.path().join("g.sqlite"), &migrations_dir()).unwrap());
    let bus = Arc::new(EventBus::new(storage.clone(), 256));
    let storage_dyn: Arc<dyn Storage> = storage.clone();
    let service = Arc::new(PermissionService::new(storage_dyn, bus));
    (dir, storage, service)
}

fn audit_rows(storage: &SqliteStorage) -> Vec<fergents_storage::Row> {
    storage
        .query("SELECT * FROM audit_log ORDER BY rowid ASC", &[])
        .unwrap()
}

fn read() -> PermissionRequest {
    PermissionRequest::new("fs.read", json!({ "path": "/tmp/a" }))
}

fn write() -> PermissionRequest {
    PermissionRequest::new("fs.write", json!({ "path": "/tmp/a" }))
}

#[test]
fn modes_control_what_prompts() {
    let (_dir, storage, service) = service();

    // Auto (default): reads pass, writes prompt.
    assert_eq!(service.check(&read()).unwrap().decision, Decision::Allow);
    assert_eq!(service.check(&write()).unwrap().decision, Decision::Prompt);

    // Manual: everything prompts.
    service.set_mode(None, Mode::Manual).unwrap();
    assert_eq!(service.check(&read()).unwrap().decision, Decision::Prompt);

    // Full: nothing prompts.
    service.set_mode(None, Mode::Full).unwrap();
    assert_eq!(service.check(&write()).unwrap().decision, Decision::Allow);
    assert_eq!(service.check(&read()).unwrap().decision, Decision::Allow);

    // Session-scoped modes do not leak to other sessions or the default.
    service.set_mode(Some("s1"), Mode::Manual).unwrap();
    assert_eq!(
        service.check(&read().in_session("s1")).unwrap().decision,
        Decision::Prompt
    );
    assert_eq!(
        service.check(&read().in_session("s2")).unwrap().decision,
        Decision::Allow
    );

    assert!(audit_rows(&storage).len() >= 5);
}

#[test]
fn policy_first_match_wins_and_denials_are_final() {
    let (_dir, _storage, service) = service();
    service.set_mode(None, Mode::Full).unwrap();
    let toml = r#"
[[rule]]
tool = "fs.write"
scope = ["/tmp/"]
action = "allow"
note = "scratch"

[[rule]]
category = "fs.write"
action = "deny"
note = "writes forbidden"

[[rule]]
category = "shell.exec"
action = "prompt"
note = "ask before shell"
"#;
    service.set_policy_toml(toml).unwrap();
    assert_eq!(service.policy().rules.len(), 3);

    let scratch = PermissionRequest {
        target: Some("/tmp/x".to_string()),
        ..write()
    };
    assert_eq!(service.check(&scratch).unwrap().decision, Decision::Allow);

    // Second rule matches first for non-scoped writes and denies — full mode
    // cannot relax a denial.
    let elsewhere = PermissionRequest {
        target: Some("/etc/x".to_string()),
        ..write()
    };
    let resolved = service.check(&elsewhere).unwrap();
    assert_eq!(resolved.decision, Decision::Deny);
    assert_eq!(resolved.reason, "writes forbidden");

    // Full mode relaxes a policy prompt.
    let shell = PermissionRequest::new("shell.exec", json!({}));
    assert_eq!(service.check(&shell).unwrap().decision, Decision::Allow);

    // Invalid TOML is rejected and the old policy stays.
    assert!(service.set_policy_toml("[[rule]]\naction = 1").is_err());
    assert_eq!(service.policy().rules.len(), 3);
}

#[test]
fn agent_overrides_can_only_narrow() {
    let (_dir, _storage, service) = service();
    service.set_mode(None, Mode::Full).unwrap();

    // A restrictive agent override applies.
    service.set_agent_override("cautious", Category::FsWrite, Decision::Deny);
    let request = write().by_agent("cautious");
    assert_eq!(service.check(&request).unwrap().decision, Decision::Deny);

    // An agent cannot widen a policy denial.
    let toml = "[[rule]]\ncategory = \"fs.write\"\naction = \"deny\"\n";
    service.set_policy_toml(toml).unwrap();
    service.set_agent_override("permissive", Category::FsWrite, Decision::Allow);
    let request = write().by_agent("permissive");
    assert_eq!(service.check(&request).unwrap().decision, Decision::Deny);

    // Tool minimum levels also only narrow.
    service.set_policy_toml("").unwrap();
    service.set_tool_level("fs.read", Decision::Prompt);
    assert_eq!(service.check(&read()).unwrap().decision, Decision::Prompt);
    // ... and full mode still relaxes it? No: overrides apply after the mode,
    // so the tool level wins.
    assert_eq!(service.check(&read()).unwrap().decision, Decision::Prompt);
}

#[test]
fn cards_and_audit_are_emitted() {
    let (_dir, storage, service) = service();
    let resolved = service
        .check(&write().in_session("s1").by_agent("a1"))
        .unwrap();
    assert_eq!(resolved.decision, Decision::Prompt);
    let card = resolved.card.expect("prompt carries a card");
    assert_eq!(card.action, "fs.write");
    assert_eq!(card.category, "fs.write");
    assert_eq!(card.choices, vec!["deny", "allow_once", "allow_session"]);

    let rows = audit_rows(&storage);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["action"], json!("permission.prompt"));
    assert_eq!(rows[0]["actor_type"], json!("agent"));
    assert_eq!(rows[0]["actor_id"], json!("a1"));
    assert_eq!(rows[0]["target"], json!("fs.write"));
    assert_eq!(rows[0]["result"], json!("prompt"));
    assert_eq!(rows[0]["session_id"], json!("s1"));
    assert!(rows[0]["hash"].as_str().unwrap().len() == 64);

    // The policy round-trips through TOML.
    let policy =
        Policy::parse_toml("[[rule]]\ncategory = \"fs.read\"\naction = \"allow\"\n").unwrap();
    assert_eq!(
        Policy::parse_toml(&policy.to_toml().unwrap()).unwrap(),
        policy
    );
}

#[test]
fn gate_adapter_maps_prompt_to_a_refusal() {
    let (_dir, _storage, service) = service();
    let gate = service.gate(Some("s1".to_string()), None);
    assert_eq!(
        fergents_agents::PermissionGate::check(&*gate, "fs.read", &json!({})),
        fergents_agents::PermissionDecision::Allow
    );
    match fergents_agents::PermissionGate::check(&*gate, "fs.write", &json!({})) {
        fergents_agents::PermissionDecision::Deny(reason) => {
            assert!(reason.starts_with("requires approval"))
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}
