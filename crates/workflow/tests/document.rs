//! The document model: parsing, defaults, serde shape, and identity.

use fergents_workflow::document::config::{EndConfig, TransformConfig};
use fergents_workflow::document::{CatchUp, OnError, PermissionRequest, ResumePolicy, TriggerKind};
use fergents_workflow::{NodeKind, WorkflowDocument};
use serde_json::json;

fn parse_yaml(source: &str) -> WorkflowDocument {
    WorkflowDocument::from_yaml(source).expect("document parses")
}

fn parse_json(value: serde_json::Value) -> WorkflowDocument {
    WorkflowDocument::from_json(&value.to_string()).expect("document parses")
}

fn valid() -> WorkflowDocument {
    parse_yaml(include_str!("fixtures/valid.yaml"))
}

fn minimal(nodes: serde_json::Value) -> WorkflowDocument {
    parse_json(json!({ "id": "t", "name": "T", "nodes": nodes }))
}

#[test]
fn valid_fixture_parses() {
    let doc = valid();

    assert_eq!(doc.id, "weather-report");
    assert_eq!(doc.name, "Weather Report");
    assert_eq!(
        doc.description.as_deref(),
        Some("Fetch a forecast and publish it as a mini app.")
    );
    assert_eq!(doc.version, "1.0.0");
    assert_eq!(doc.tags, vec!["demo".to_string()]);
    assert_eq!(doc.variables.get("city"), Some(&json!("Rome")));

    let permission = doc.permissions.get("net.http").expect("net.http declared");
    assert!(permission.is_granted());
    assert!(permission.scopes().is_empty());

    assert_eq!(doc.triggers.len(), 1);
    assert_eq!(doc.triggers[0].id, "manual");
    assert_eq!(doc.triggers[0].kind, TriggerKind::Manual);

    assert_eq!(doc.nodes.len(), 3);
    assert_eq!(doc.nodes[0].kind, NodeKind::Trigger);
    assert_eq!(doc.nodes[1].kind, NodeKind::Transform);
    assert_eq!(doc.nodes[2].kind, NodeKind::End);

    assert_eq!(doc.edges.len(), 2);
    assert_eq!(doc.edges[0].from, "start");
    assert_eq!(doc.edges[0].from_port.as_deref(), Some("event"));
    assert_eq!(doc.edges[1].to_port.as_deref(), Some("result"));

    let app = doc.app.as_ref().expect("app manifest present");
    assert_eq!(app.id, "weather");
    assert_eq!(app.icon.as_deref(), Some("cloud"));
    assert_eq!(app.inputs.len(), 1);
    assert_eq!(app.inputs[0].id, "city");
    assert!(app.inputs[0].required);
    assert!(app.inputs[0].options.is_empty());
    assert_eq!(app.outputs[0].kind, "markdown");
    assert_eq!(app.layout.len(), 3);
}

#[test]
fn missing_version_defaults_to_zero_one_zero() {
    let doc = parse_json(json!({ "id": "t", "name": "T" }));

    assert_eq!(doc.version, "0.1.0");
    assert!(doc.description.is_none());
    assert!(doc.tags.is_empty());
    assert!(doc.triggers.is_empty());
    assert!(doc.nodes.is_empty());
    assert!(doc.edges.is_empty());
    assert!(doc.app.is_none());
}

#[test]
fn identity_fields_are_required() {
    assert!(WorkflowDocument::from_json(r#"{"name":"T"}"#).is_err());
    assert!(WorkflowDocument::from_json(r#"{"id":"t"}"#).is_err());
}

#[test]
fn unknown_top_level_fields_are_rejected() {
    let error = WorkflowDocument::from_json(r#"{"id":"t","name":"T","bogus":1}"#)
        .expect_err("unknown field is rejected");
    assert!(error.to_string().contains("bogus"), "{error}");
}

#[test]
fn documents_round_trip_through_yaml_and_json() {
    let doc = valid();
    let digest = doc.digest().expect("digest");

    let yaml = doc.to_yaml().expect("to_yaml");
    let from_yaml = parse_yaml(&yaml);
    assert_eq!(from_yaml.digest().expect("digest"), digest);

    let json = doc.to_json().expect("to_json");
    let from_json = WorkflowDocument::from_json(&json).expect("from_json");
    assert_eq!(from_json.digest().expect("digest"), digest);
}

#[test]
fn json_is_canonical_and_carries_the_kind_tag() {
    let doc = valid();
    let json = doc.to_json().expect("to_json");

    assert!(json.contains(r#""kind":"manual""#), "{json}");
    assert!(json.contains(r#""type":"trigger""#), "{json}");

    // Canonical means the serialization is stable: re-parsing and
    // re-serializing reproduces the exact same bytes, so the digest is
    // reproducible across round trips.
    let again = WorkflowDocument::from_json(&json)
        .expect("from_json")
        .to_json()
        .expect("to_json");
    assert_eq!(again, json);
}

#[test]
fn digest_is_hex_and_content_sensitive() {
    let doc = valid();
    let digest = doc.digest().expect("digest");

    assert_eq!(digest.len(), 64);
    assert!(digest.chars().all(|c| c.is_ascii_hexdigit()), "{digest}");
    assert_eq!(doc.digest().expect("digest"), digest);

    let mut changed = doc.clone();
    changed.name = "Renamed".to_string();
    assert_ne!(changed.digest().expect("digest"), digest);
}

#[test]
fn nodes_and_triggers_are_looked_up_by_id() {
    let doc = valid();

    assert_eq!(
        doc.node("parse").map(|node| node.kind.clone()),
        Some(NodeKind::Transform)
    );
    assert!(doc.node("missing").is_none());
    assert_eq!(
        doc.trigger("manual").map(|spec| spec.id.clone()),
        Some("manual".to_string())
    );
    assert!(doc.trigger("missing").is_none());
}

#[test]
fn every_builtin_node_kind_round_trips_through_its_name() {
    assert_eq!(NodeKind::BUILTIN.len(), 12);

    for kind in NodeKind::BUILTIN {
        assert!(kind.is_builtin());
        assert_eq!(NodeKind::parse(kind.as_str()), Some(kind));
    }

    assert_eq!(
        NodeKind::parse("plugin:acme:translate"),
        Some(NodeKind::Plugin("plugin:acme:translate".to_string()))
    );
    assert!(!NodeKind::Plugin("plugin:acme:translate".to_string()).is_builtin());
    assert_eq!(NodeKind::parse("bogus"), None);
    // The `plugin:` prefix is structural; the `plugin:<id>:<node>` shape is
    // enforced by validation, not by parsing.
    assert_eq!(
        NodeKind::parse("plugin:acme"),
        Some(NodeKind::Plugin("plugin:acme".to_string()))
    );
}

#[test]
fn node_kinds_agree_on_ports_and_side_effects() {
    assert!(NodeKind::Trigger.input_ports().is_empty());
    assert_eq!(NodeKind::Trigger.output_ports(), &["event"]);
    assert_eq!(NodeKind::End.input_ports(), &["result"]);
    assert!(NodeKind::End.output_ports().is_empty());
    assert_eq!(NodeKind::Condition.output_ports(), &["true", "false"]);
    assert!(NodeKind::Plugin("plugin:a:b".to_string())
        .input_ports()
        .is_empty());

    for kind in [
        NodeKind::Tool,
        NodeKind::Agent,
        NodeKind::Code,
        NodeKind::Http,
        NodeKind::Human,
        NodeKind::Subworkflow,
    ] {
        assert!(kind.has_side_effects(), "{kind:?} has side effects");
    }
    for kind in [
        NodeKind::Trigger,
        NodeKind::Llm,
        NodeKind::Condition,
        NodeKind::End,
    ] {
        assert!(!kind.has_side_effects(), "{kind:?} is pure");
    }
}

#[test]
fn node_unknown_types_are_rejected_with_a_hint() {
    let error =
        WorkflowDocument::from_json(r#"{"id":"t","name":"T","nodes":[{"id":"a","type":"bogus"}]}"#)
            .expect_err("unknown node type is rejected");

    let message = error.to_string();
    assert!(message.contains("unknown node type `bogus`"), "{message}");
    assert!(message.contains("plugin:"), "{message}");
}

#[test]
fn node_defaults_are_applied() {
    let doc = minimal(json!([
        { "id": "a", "type": "transform" },
        { "id": "b", "type": "http" },
        {
            "id": "c",
            "type": "tool",
            "timeout_s": 5,
            "on_error": "continue",
            "resume": "skip"
        }
    ]));

    let pure = doc.node("a").expect("a");
    assert_eq!(pure.timeout_s(), 60);
    assert_eq!(pure.on_error(), OnError::Fail);
    assert_eq!(pure.resume(), ResumePolicy::Retry);

    let side_effecting = doc.node("b").expect("b");
    assert_eq!(side_effecting.resume(), ResumePolicy::Fail);

    let explicit = doc.node("c").expect("c");
    assert_eq!(explicit.timeout_s(), 5);
    assert_eq!(explicit.on_error(), OnError::Continue);
    assert_eq!(explicit.resume(), ResumePolicy::Skip);
}

#[test]
fn node_configs_deserialize_into_typed_views() {
    let doc = valid();

    let transform: TransformConfig = doc
        .node("parse")
        .expect("parse")
        .config_as()
        .expect("config");
    assert_eq!(transform.ops.len(), 1);
    assert!(transform.template.is_none());

    let end: EndConfig = doc
        .node("finish")
        .expect("finish")
        .config_as()
        .expect("config");
    assert_eq!(end.result.as_deref(), Some("{{ parse.output }}"));

    let error = doc
        .node("parse")
        .expect("parse")
        .config_as::<EndConfig>()
        .expect_err("transform config is not an end config");
    assert!(error.to_string().contains("node `parse`"), "{error}");
}

#[test]
fn nodes_tolerate_forward_compatible_fields() {
    // `Node` deliberately has no `deny_unknown_fields`, so a newer document
    // still loads in an older kernel.
    let doc = minimal(json!([{ "id": "a", "type": "end", "note": "ignored" }]));
    assert_eq!(doc.nodes.len(), 1);
}

#[test]
fn edges_reject_unknown_fields() {
    let error = WorkflowDocument::from_json(
        r#"{"id":"t","name":"T","edges":[{"from":"a","to":"b","label":"x"}]}"#,
    )
    .expect_err("edge fields are closed");

    assert!(error.to_string().contains("label"), "{error}");
}

#[test]
fn permissions_accept_bools_and_scope_lists() {
    let doc = parse_json(json!({
        "id": "t",
        "name": "T",
        "permissions": {
            "net.http": true,
            "fs.write": false,
            "shell.exec": ["cargo *", "git *"]
        }
    }));

    assert_eq!(
        doc.permissions.get("net.http"),
        Some(&PermissionRequest::Enabled(true))
    );
    assert!(!doc.permissions["fs.write"].is_granted());
    assert!(doc.permissions["shell.exec"].is_granted());
    assert_eq!(
        doc.permissions["shell.exec"].scopes(),
        &["cargo *".to_string(), "git *".to_string()]
    );
}

#[test]
fn every_trigger_kind_parses_and_names_itself() {
    let doc = parse_json(json!({
        "id": "t",
        "name": "T",
        "triggers": [
            { "id": "manual", "kind": "manual" },
            { "id": "ai", "kind": "ai", "description": "ask me" },
            { "id": "cron", "kind": "cron", "schedule": "0 8 * * 1", "timezone": "Europe/Rome" },
            { "id": "hook", "kind": "webhook", "path": "/hook" },
            {
                "id": "file",
                "kind": "file",
                "path": "inbox/**/*.md",
                "events": ["create", "modify"]
            },
            { "id": "event", "kind": "event", "topic": "note.created", "filter": "/path" }
        ]
    }));

    let names: Vec<&str> = doc.triggers.iter().map(|spec| spec.kind.as_str()).collect();
    assert_eq!(
        names,
        vec!["manual", "ai", "cron", "webhook", "file", "event"]
    );

    match &doc.triggers[2].kind {
        TriggerKind::Cron {
            schedule,
            timezone,
            catch_up,
            concurrency,
        } => {
            assert_eq!(schedule, "0 8 * * 1");
            assert_eq!(timezone.as_deref(), Some("Europe/Rome"));
            assert_eq!(*catch_up, CatchUp::Skip);
            assert!(concurrency.is_none());
        }
        other => panic!("expected a cron trigger, got {other:?}"),
    }

    match &doc.triggers[3].kind {
        TriggerKind::Webhook { path, method, auth } => {
            assert_eq!(path, "/hook");
            assert_eq!(method, "POST");
            assert_eq!(auth, "bearer");
        }
        other => panic!("expected a webhook trigger, got {other:?}"),
    }

    match &doc.triggers[5].kind {
        TriggerKind::Event { topic, filter } => {
            assert_eq!(topic, "note.created");
            assert_eq!(filter.as_deref(), Some("/path"));
        }
        other => panic!("expected an event trigger, got {other:?}"),
    }
}

#[test]
fn trigger_kinds_keep_their_tag_when_serialized() {
    let doc = parse_json(json!({
        "id": "t",
        "name": "T",
        "triggers": [{ "id": "cron", "kind": "cron", "schedule": "0 0 * * *", "catch_up": "run_once" }]
    }));

    let json = doc.to_json().expect("to_json");
    assert!(json.contains(r#""kind":"cron""#), "{json}");
    assert!(json.contains(r#""catch_up":"run_once""#), "{json}");
}

#[test]
fn app_manifest_fields_default() {
    let doc = parse_json(json!({
        "id": "t",
        "name": "T",
        "app": { "id": "mini", "name": "Mini" }
    }));

    let app = doc.app.as_ref().expect("app");
    assert!(app.icon.is_none());
    assert!(app.description.is_none());
    assert!(app.inputs.is_empty());
    assert!(app.outputs.is_empty());
    assert!(app.layout.is_empty());
}

#[test]
fn catch_up_and_file_events_are_closed_enums() {
    assert_eq!(CatchUp::default(), CatchUp::Skip);
    assert!(serde_json::from_str::<CatchUp>("\"run_all\"").is_ok());
    assert!(serde_json::from_str::<CatchUp>("\"sometimes\"").is_err());
}
