//! Serde round-trip tests for the Phase 2 protocol types
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
fn capability_list_roundtrip() {
    assert_roundtrip::<CapabilityListResult>(json!({
        "capabilities": [{
            "id": "capability.chat",
            "name": "Chat",
            "version": "0.1.0",
            "source": "kernel",
            "description": "Conversational chat.",
            "dependencies": ["capability.llm"],
            "resources": [],
            "enabled": true,
            "default_enabled": true,
            "hot_toggle": false,
            "restart_required": false,
            "config": null
        }]
    }));
    assert_roundtrip::<CapabilitySetParams>(json!({
        "id": "capability.charts",
        "enabled": true,
        "auto_enable_deps": true
    }));
    assert_roundtrip::<CapabilityChangedPayload>(json!({
        "id": "capability.charts",
        "enabled": true,
        "restart_required": false
    }));
}

#[test]
fn chat_roundtrip() {
    assert_roundtrip::<ChatMessageSendParams>(json!({
        "session_id": "01J8SESSION",
        "text": "Summarize the MCP spec."
    }));
    assert_roundtrip::<ChatMessageListResult>(json!({
        "messages": [{
            "id": "01J8MSG",
            "session_id": "01J8SESSION",
            "seq": 1,
            "role": "assistant",
            "content": "Done.",
            "tool_calls": null,
            "tokens": 42,
            "created_at": 1792694626000_i64
        }]
    }));
    assert_roundtrip::<ChatMessageRetryParams>(json!({ "session_id": "01J8SESSION" }));
    assert_roundtrip::<ChatMessageSendResult>(json!({
        "session_id": "01J8SESSION",
        "messages": [{
            "id": "01J8MSG",
            "session_id": "01J8SESSION",
            "seq": 2,
            "role": "assistant",
            "content": "Done.",
            "tool_calls": null,
            "tokens": 42,
            "created_at": 1792694626000_i64
        }]
    }));
    assert_roundtrip::<ChatSessionListResult>(json!({
        "sessions": [{
            "id": "01J8SESSION",
            "agent_id": "assistant",
            "title": "MCP research",
            "model": "anthropic:claude-sonnet-4.5",
            "state": "created",
            "parent_session_id": null,
            "permission_mode": "auto",
            "summary": null,
            "tokens_in": 0,
            "tokens_out": 0,
            "cost_usd": 0.0,
            "created_at": 1792694626000_i64,
            "updated_at": 1792694626000_i64
        }]
    }));
    assert_roundtrip::<AttachmentRouting>(json!({
        "attachment": {
            "name": "chart.png",
            "media_type": "image/png",
            "size": 1024,
            "kind": "image"
        },
        "routed": "vision",
        "note": null
    }));
}

#[test]
fn spec_workflow_roundtrip() {
    assert_roundtrip::<SpecStartParams>(json!({
        "slug": "mcp-security",
        "task": "Write the MCP security note.",
        "mode": "batch",
        "session_id": null
    }));
    assert_roundtrip::<SpecGetResult>(json!({
        "run": {
            "id": "01J8RUN",
            "session_id": null,
            "slug": "mcp-security",
            "mode": "supervised",
            "state": "awaiting_spec_approval",
            "pending_gate": "spec",
            "artifacts": { "spec": "specs/mcp-security/spec.md" },
            "created_at": 1792694626000_i64,
            "updated_at": 1792694626000_i64
        },
        "artifacts": { "spec": "# Spec" }
    }));
    assert_roundtrip::<RunApprovalPayload>(json!({
        "run_id": "01J8RUN",
        "gate": "spec",
        "status": "approved"
    }));
}

#[test]
fn permissions_and_backups_roundtrip() {
    assert_roundtrip::<PermissionResolved>(json!({
        "decision": "prompt",
        "category": "shell.exec",
        "reason": "risky in auto mode",
        "card": {
            "action": "shell.exec",
            "target": "cargo test",
            "reason": "run the test suite",
            "category": "shell.exec",
            "choices": ["allow", "deny"]
        }
    }));
    assert_roundtrip::<BackupListResult>(json!({
        "backups": [{
            "id": "01J8BK",
            "path": "research/note.md",
            "backup_path": "backups/01J8BK/note.md",
            "existed": true,
            "created_at": 1792694626000_i64,
            "run_id": "01J8RUN",
            "session_id": null
        }]
    }));
}

#[test]
fn secrets_roundtrip() {
    assert_roundtrip::<SecretRef>(json!({
        "name": "anthropic.api_key",
        "provider": "anthropic",
        "created_at": 1792694626000_i64
    }));
    assert_roundtrip::<SecretListResult>(json!({
        "secrets": [{
            "name": "anthropic.api_key",
            "provider": null,
            "created_at": 1792694626000_i64
        }]
    }));
    assert_roundtrip::<SecretSetParams>(json!({
        "name": "openai.api_key",
        "value": "sk-secret",
        "provider": "openai"
    }));
    assert_roundtrip::<SecretNameParams>(json!({ "name": "openai.api_key" }));
    assert_roundtrip::<SecretDeletedResult>(json!({ "deleted": true }));
    assert_roundtrip::<SecretTestResult>(json!({ "name": "openai.api_key", "present": false }));
}

#[test]
fn providers_roundtrip() {
    assert_roundtrip::<ProviderConfig>(json!({
        "id": "anthropic",
        "kind": "anthropic",
        "base_url": null,
        "secret_name": "anthropic.api_key",
        "models": ["claude-sonnet-4.5"],
        "created_at": 1792694626000_i64
    }));
    assert_roundtrip::<ProviderListResult>(json!({
        "providers": [{
            "id": "mock",
            "kind": "mock",
            "base_url": null,
            "secret_name": null,
            "models": ["mock-1"],
            "created_at": 1792694626000_i64
        }]
    }));
    assert_roundtrip::<ProviderAddParams>(json!({
        "id": "ollama",
        "kind": "ollama",
        "base_url": "http://localhost:11434/v1",
        "secret_name": null,
        "models": ["qwen2.5:7b"]
    }));
    assert_roundtrip::<ProviderIdParams>(json!({ "id": "ollama" }));
    assert_roundtrip::<ProviderDeletedResult>(json!({ "deleted": false }));
    assert_roundtrip::<ProviderTestResult>(json!({
        "id": "ollama",
        "ok": true,
        "detail": null
    }));
    assert_roundtrip::<ProviderModelsResult>(json!({ "models": ["mock-1"] }));
    assert_roundtrip::<ProviderRoute>(json!({
        "task": "chat",
        "models": ["mock-1"]
    }));
    assert_roundtrip::<ProviderRoutesResult>(json!({
        "routes": [{ "task": "chat", "models": ["mock-1"] }]
    }));
    assert_roundtrip::<ProviderRouteSetParams>(json!({
        "task": "summarize",
        "models": ["mock-1", "mock-2"]
    }));
}

#[test]
fn mcp_roundtrip() {
    assert_roundtrip::<McpServer>(json!({
        "id": "filesystem",
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-filesystem"],
        "allow": true,
        "connected": false,
        "created_at": 1792694626000_i64
    }));
    assert_roundtrip::<McpServerListResult>(json!({
        "servers": [{
            "id": "filesystem",
            "command": null,
            "args": [],
            "allow": false,
            "connected": false,
            "created_at": 1792694626000_i64
        }]
    }));
    assert_roundtrip::<McpServerAddParams>(json!({
        "id": "filesystem",
        "command": "npx",
        "args": ["-y", "server"],
        "allow": true
    }));
    assert_roundtrip::<McpServerIdParams>(json!({ "id": "filesystem" }));
    assert_roundtrip::<McpServerDeletedResult>(json!({ "deleted": true }));
    assert_roundtrip::<McpServerTestResult>(json!({
        "id": "filesystem",
        "ok": false,
        "detail": "no command configured"
    }));
    assert_roundtrip::<McpServerHealthResult>(json!({
        "id": "filesystem",
        "connected": true,
        "healthy": true
    }));
    assert_roundtrip::<McpTool>(json!({
        "server": "filesystem",
        "name": "read_file",
        "description": "Read a file."
    }));
    assert_roundtrip::<McpToolListResult>(json!({
        "tools": [{ "server": "filesystem", "name": "read_file", "description": "" }]
    }));
}

#[test]
fn compare_roundtrip() {
    assert_roundtrip::<ChatCompareSide>(json!({
        "session_id": "01J8LEFT",
        "model": "mock-1",
        "content": "echo: hello",
        "error": null
    }));
    assert_roundtrip::<ChatCompareStartParams>(json!({
        "agent_id": "assistant",
        "prompt": "hello",
        "models": ["mock-1", "mock-2"],
        "title": null
    }));
    assert_roundtrip::<ChatCompareStartResult>(json!({
        "sides": [
            { "session_id": "01J8LEFT", "model": "mock-1", "content": "echo: hello", "error": null },
            { "session_id": "01J8RIGHT", "model": "mock-2", "content": null, "error": "no provider" }
        ]
    }));
}

#[test]
fn context_and_memory_roundtrip() {
    assert_roundtrip::<ContextSnapshot>(json!({
        "id": "01J8CTX",
        "session_id": "01J8SESSION",
        "turn_seq": 1,
        "sections": [{
            "name": "rag",
            "enabled": true,
            "cacheable": true,
            "tokens": 128,
            "source_refs": ["note.md#section"],
            "content": "..."
        }],
        "total_tokens": 128,
        "budget": 128000
    }));
    assert_roundtrip::<MemoryEntriesResult>(json!({
        "entries": [{
            "id": "01J8MEM",
            "scope": "agent:assistant",
            "kind": "fact",
            "content": "Prefers concise answers.",
            "importance": 0.8,
            "created_at": 1792694626000_i64,
            "updated_at": 1792694626000_i64,
            "last_used_at": null
        }]
    }));
}

#[test]
fn phase2_types_tolerate_unknown_fields() {
    // Rule 3: additive schema; unknown fields must not break parsing.
    let params: RunRejectParams = serde_json::from_value(json!({
        "run_id": "01J8RUN",
        "reason": "needs work",
        "future": 1
    }))
    .expect("new Phase 2 types tolerate unknown fields");
    assert_eq!(params.reason.as_deref(), Some("needs work"));
}
