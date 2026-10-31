//! Semantic validation: every rule, its code, and where it is reported.

use fergents_workflow::{validate, Validation, WorkflowDocument};
use serde_json::{json, Value};

fn start() -> Value {
    json!({ "id": "start", "type": "trigger", "config": { "trigger_id": "manual" } })
}

fn finish() -> Value {
    json!({ "id": "finish", "type": "end", "config": {} })
}

fn edge(from: &str, from_port: &str, to: &str, to_port: &str) -> Value {
    json!({ "from": from, "from_port": from_port, "to": to, "to_port": to_port })
}

fn manual() -> Value {
    json!([{ "id": "manual", "kind": "manual" }])
}

fn check_doc(document: Value) -> Validation {
    let doc = WorkflowDocument::from_json(&document.to_string()).expect("document parses");
    validate(&doc)
}

fn check_triggers(triggers: Value) -> Validation {
    check_doc(json!({
        "id": "t",
        "name": "T",
        "triggers": triggers,
        "nodes": [start(), finish()],
        "edges": [edge("start", "event", "finish", "result")]
    }))
}

fn check_parts(nodes: Value, edges: Value) -> Validation {
    check_doc(json!({
        "id": "t",
        "name": "T",
        "triggers": manual(),
        "nodes": nodes,
        "edges": edges
    }))
}

/// Validate `start -> … -> finish` with the given middle nodes.
fn check_chain(middle: Value) -> Validation {
    let mut nodes = vec![start()];
    let mut edges = Vec::new();
    let mut previous = "start".to_string();
    let mut previous_port = "event".to_string();

    if let Value::Array(items) = middle {
        for (index, node) in items.iter().enumerate() {
            let id = node["id"].as_str().expect("a node id").to_string();
            let entry = node["entry"].as_str().unwrap_or(default_entry(node));
            edges.push(edge(&previous, &previous_port, &id, entry));
            previous = id;
            previous_port = default_exit(node).to_string();
            if index + 1 == items.len() {
                break;
            }
        }
        nodes.extend(items);
    } else {
        nodes.push(middle);
    }

    edges.push(edge(&previous, &previous_port, "finish", "result"));
    nodes.push(finish());
    check_parts(Value::Array(nodes), Value::Array(edges))
}

fn default_entry(node: &Value) -> &'static str {
    match node["type"].as_str().unwrap_or_default() {
        "llm" => "prompt",
        "tool" => "args",
        "agent" => "task",
        "code" => "input",
        "http" => "url",
        "condition" => "value",
        "loop" => "items",
        "subworkflow" => "inputs",
        "human" => "prompt",
        "transform" => "input",
        "end" => "result",
        _ => "input",
    }
}

fn default_exit(node: &Value) -> &'static str {
    match node["type"].as_str().unwrap_or_default() {
        "llm" => "text",
        "tool" => "result",
        "agent" => "result",
        "code" => "output",
        "http" => "status",
        "condition" => "true",
        "loop" => "results",
        "subworkflow" => "outputs",
        "human" => "decision",
        "transform" => "output",
        _ => "output",
    }
}

fn check_app(app: Value) -> Validation {
    check_doc(json!({
        "id": "t",
        "name": "T",
        "triggers": manual(),
        "nodes": [start(), finish()],
        "edges": [edge("start", "event", "finish", "result")],
        "app": app
    }))
}

fn pairs(validation: &Validation) -> Vec<(&'static str, String)> {
    validation
        .errors
        .iter()
        .chain(validation.warnings.iter())
        .map(|issue| (issue.code, issue.path.clone()))
        .collect()
}

fn assert_issue(validation: &Validation, code: &str, path: &str) {
    let found = pairs(validation)
        .into_iter()
        .any(|(candidate, candidate_path)| candidate == code && candidate_path == path);
    assert!(
        found,
        "expected `{code}` at `{path}`, got {:#?}",
        pairs(validation)
    );
}

fn assert_error(validation: &Validation, code: &str) {
    assert!(
        validation.errors.iter().any(|issue| issue.code == code),
        "expected the error `{code}`, got {:#?}",
        pairs(validation)
    );
}

fn assert_warning(validation: &Validation, code: &str) {
    assert!(
        validation.warnings.iter().any(|issue| issue.code == code),
        "expected the warning `{code}`, got {:#?}",
        pairs(validation)
    );
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

#[test]
fn the_valid_fixture_has_no_errors_and_no_warnings() {
    let doc = WorkflowDocument::from_yaml(include_str!("fixtures/valid.yaml")).expect("parses");
    let validation = validate(&doc);

    assert!(validation.is_valid(), "{:#?}", validation.errors);
    assert!(validation.warnings.is_empty(), "{:#?}", validation.warnings);
    assert!(validation.messages().is_empty());
    assert!(validation.into_result().is_ok());
}

#[test]
fn the_broken_fixture_reports_every_fault_with_a_path() {
    let doc = WorkflowDocument::from_yaml(include_str!("fixtures/broken.yaml")).expect("parses");
    let validation = validate(&doc);

    assert!(!validation.is_valid());

    // Identity and triggers.
    assert_issue(&validation, "invalid_version", "version");
    assert_issue(&validation, "invalid_cron", "triggers[0].schedule");
    assert_issue(&validation, "duplicate_webhook", "triggers[2].path");

    // Node configs and loops.
    assert_issue(&validation, "unknown_trigger", "nodes[1].config.trigger_id");
    assert_issue(&validation, "subworkflow_cycle", "nodes[7].config.workflow");
    assert!(
        validation
            .errors
            .iter()
            .any(|issue| issue.code == "loop_scope_conflict"
                && issue.path == "nodes[5].config.body"
                && issue.message.contains("`shared`")),
        "expected a loop scope conflict on `shared`, got {:#?}",
        pairs(&validation)
    );

    // Edges.
    assert_issue(&validation, "trigger_has_inbound", "edges[3].to");
    assert_issue(&validation, "end_has_outbound", "edges[10].from");
    assert_issue(&validation, "unknown_port", "edges[11].from_port");
    assert_issue(&validation, "cycle", "edges");

    // Reachability and templates. `bad_ref` is a `trigger` node, so it seeds
    // reachability even though its `trigger_id` is unknown; `stranded` is the
    // genuinely unreachable node.
    assert_issue(&validation, "unreachable_node", "nodes[10]");
    assert_issue(
        &validation,
        "secret_outside_config",
        "nodes[8].inputs.value",
    );
    assert_issue(&validation, "unknown_source", "nodes[8].config.template");

    // App manifest.
    assert_issue(&validation, "missing_label", "app.inputs[0].label");
    assert_issue(&validation, "unknown_reference", "app.layout[0].fields");
    assert_issue(&validation, "unknown_reference", "app.layout[1].ref");

    // Lints.
    assert_issue(&validation, "unused_node", "nodes[10]");
    assert_issue(&validation, "unreachable_port", "nodes[4]");
    assert_issue(&validation, "backoff_exceeds_timeout", "nodes[9].retry");

    let error = validation.into_result().expect_err("errors block the run");
    assert!(error.to_string().contains("error(s):"), "{error}");
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[test]
fn workflow_identity_is_validated() {
    let nodes = json!([start(), finish()]);
    let edges = json!([edge("start", "event", "finish", "result")]);

    let bad_id = check_doc(json!({
        "id": "Bad Id", "name": "T", "triggers": manual(), "nodes": nodes, "edges": edges
    }));
    assert_issue(&bad_id, "invalid_id", "id");

    let bad_name = check_doc(json!({
        "id": "t", "name": "", "triggers": manual(), "nodes": nodes, "edges": edges
    }));
    assert_issue(&bad_name, "missing_name", "name");

    let bad_version = check_doc(json!({
        "id": "t", "name": "T", "version": "1.0", "triggers": manual(), "nodes": nodes, "edges": edges
    }));
    assert_issue(&bad_version, "invalid_version", "version");
}

#[test]
fn a_workflow_needs_a_trigger_and_an_end() {
    let no_triggers = check_doc(json!({ "id": "t", "name": "T", "nodes": [start()] }));
    assert_issue(&no_triggers, "missing_trigger", "triggers");
    assert_issue(&no_triggers, "missing_end", "nodes");

    let no_end = check_parts(json!([start()]), json!([]));
    assert_issue(&no_end, "missing_end", "nodes");
}

#[test]
fn declarations_are_unique_and_well_formed() {
    let duplicate_node = check_parts(
        json!([start(), start(), finish()]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert_issue(&duplicate_node, "duplicate_node_id", "nodes[1].id");

    let invalid_node_id = check_parts(
        json!([start(), { "id": "Bad", "type": "transform" }, finish()]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert_issue(&invalid_node_id, "invalid_node_id", "nodes[1].id");

    let invalid_timeout = check_parts(
        json!([start(), { "id": "dead", "type": "transform", "timeout_s": 0 }, finish()]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert_issue(&invalid_timeout, "invalid_timeout", "nodes[1].timeout_s");

    let invalid_trigger_id = check_triggers(json!([{ "id": "Tick", "kind": "manual" }]));
    assert_issue(&invalid_trigger_id, "invalid_id", "triggers[0].id");
}

#[test]
fn duplicate_trigger_ids_are_rejected() {
    let validation = check_triggers(json!([
        { "id": "manual", "kind": "manual" },
        { "id": "manual", "kind": "ai" }
    ]));
    assert_issue(&validation, "duplicate_trigger_id", "triggers[1].id");
}

// ---------------------------------------------------------------------------
// Triggers
// ---------------------------------------------------------------------------

#[test]
fn cron_triggers_are_checked() {
    let schedule = check_triggers(json!([{ "id": "tick", "kind": "cron", "schedule": "* * *" }]));
    assert_issue(&schedule, "invalid_cron", "triggers[0].schedule");

    let timezone = check_triggers(json!([{
        "id": "tick", "kind": "cron", "schedule": "0 8 * * *", "timezone": "garbage"
    }]));
    assert_issue(&timezone, "invalid_timezone", "triggers[0].timezone");

    let concurrency = check_triggers(json!([{
        "id": "tick", "kind": "cron", "schedule": "0 8 * * *", "concurrency": 9
    }]));
    assert_issue(
        &concurrency,
        "invalid_concurrency",
        "triggers[0].concurrency",
    );
}

#[test]
fn webhook_triggers_are_checked() {
    let path = check_triggers(json!([{ "id": "hook", "kind": "webhook", "path": "hook" }]));
    assert_issue(&path, "invalid_webhook", "triggers[0].path");

    let method = check_triggers(json!([{
        "id": "hook", "kind": "webhook", "path": "/hook", "method": "FETCH"
    }]));
    assert_issue(&method, "invalid_webhook", "triggers[0].method");

    let auth = check_triggers(json!([{
        "id": "hook", "kind": "webhook", "path": "/hook", "auth": "basic"
    }]));
    assert_issue(&auth, "invalid_webhook", "triggers[0].auth");

    let duplicate = check_triggers(json!([
        { "id": "hook", "kind": "webhook", "path": "/hook" },
        { "id": "hook2", "kind": "webhook", "path": "/hook", "method": "POST" }
    ]));
    assert_issue(&duplicate, "duplicate_webhook", "triggers[1].path");
}

#[test]
fn file_and_event_triggers_are_checked() {
    let file = check_triggers(json!([{ "id": "fs", "kind": "file", "path": "" }]));
    assert_issue(&file, "invalid_file_trigger", "triggers[0].path");

    let topic = check_triggers(json!([{ "id": "ev", "kind": "event", "topic": "" }]));
    assert_issue(&topic, "invalid_event_trigger", "triggers[0].topic");

    let filter = check_triggers(json!([{
        "id": "ev", "kind": "event", "topic": "note.created", "filter": "path"
    }]));
    assert_issue(&filter, "invalid_event_trigger", "triggers[0].filter");
}

// ---------------------------------------------------------------------------
// Node configs
// ---------------------------------------------------------------------------

#[test]
fn trigger_nodes_must_name_a_declared_trigger() {
    let validation = check_parts(
        json!([
            { "id": "start", "type": "trigger", "config": { "trigger_id": "nope" } },
            finish()
        ]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert_issue(&validation, "unknown_trigger", "nodes[0].config.trigger_id");
}

#[test]
fn llm_nodes_are_checked() {
    let validation = check_chain(json!([{
        "id": "ask", "type": "llm",
        "config": { "model": "ollama:qwen2.5:7b", "prompt": "", "temperature": 3.0, "max_tokens": 0 }
    }]));
    assert_issue(&validation, "invalid_config", "nodes[1].config.prompt");
    assert_issue(&validation, "invalid_config", "nodes[1].config.temperature");
    assert_issue(&validation, "invalid_config", "nodes[1].config.max_tokens");
}

#[test]
fn tool_agent_and_code_nodes_are_checked() {
    // A missing required key fails typed deserialization and is reported at
    // the config root.
    let missing_tool = check_chain(json!([{ "id": "call", "type": "tool", "config": {} }]));
    assert_issue(&missing_tool, "invalid_config", "nodes[1].config");

    // A present-but-blank value is reported at the offending field.
    let tool = check_chain(json!([{
        "id": "call", "type": "tool", "config": { "tool": "  " }
    }]));
    assert_issue(&tool, "invalid_config", "nodes[1].config.tool");

    let agent = check_chain(json!([{
        "id": "delegate", "type": "agent", "config": { "agent": "researcher", "task": "" }
    }]));
    assert_issue(&agent, "invalid_config", "nodes[1].config.task");

    let language = check_chain(json!([{
        "id": "run", "type": "code", "config": { "language": "python", "source": "print(1)" }
    }]));
    assert_issue(&language, "invalid_config", "nodes[1].config.language");

    let source = check_chain(json!([{
        "id": "run", "type": "code", "config": { "language": "rust", "source": "  " }
    }]));
    assert_issue(&source, "invalid_config", "nodes[1].config.source");
}

#[test]
fn http_nodes_are_checked() {
    let validation = check_chain(json!([{
        "id": "fetch", "type": "http",
        "config": { "method": "FETCH", "url": "", "max_bytes": 9 * 1024 * 1024 }
    }]));
    assert_issue(&validation, "invalid_config", "nodes[1].config.method");
    assert_issue(&validation, "invalid_config", "nodes[1].config.url");
    assert_issue(&validation, "invalid_config", "nodes[1].config.max_bytes");
}

#[test]
fn condition_and_loop_nodes_are_checked() {
    let condition = check_chain(json!([{
        "id": "gate", "type": "condition", "config": { "expr": "event.n >" }
    }]));
    assert_issue(&condition, "invalid_expression", "nodes[1].config.expr");

    let both_sources = check_chain(json!([{
        "id": "each", "type": "loop", "config": { "count": 2, "items": [], "body": [] }
    }]));
    assert_error(&both_sources, "invalid_config");

    let parallel = check_chain(json!([{
        "id": "each", "type": "loop", "config": { "count": 2, "parallel": 9, "body": [] }
    }]));
    assert_issue(&parallel, "invalid_config", "nodes[1].config.parallel");

    let iterations = check_chain(json!([{
        "id": "each", "type": "loop", "config": { "count": 2, "max_iterations": 5000, "body": [] }
    }]));
    assert_issue(
        &iterations,
        "invalid_config",
        "nodes[1].config.max_iterations",
    );
}

#[test]
fn subworkflow_and_human_nodes_are_checked() {
    let self_call = check_chain(json!([{
        "id": "nested", "type": "subworkflow", "config": { "workflow": "t" }
    }]));
    assert_issue(&self_call, "subworkflow_cycle", "nodes[1].config.workflow");

    let version = check_chain(json!([{
        "id": "nested", "type": "subworkflow", "config": { "workflow": "other", "version": 0 }
    }]));
    assert_issue(&version, "invalid_config", "nodes[1].config.version");

    let human = check_chain(json!([{
        "id": "approve", "type": "human",
        "config": { "prompt": "", "options": ["yes"], "on_timeout": "continue:maybe" }
    }]));
    assert_issue(&human, "invalid_config", "nodes[1].config.prompt");
    assert_issue(&human, "invalid_config", "nodes[1].config.on_timeout");
}

#[test]
fn transform_operations_are_validated_at_save_time() {
    let pick = check_chain(json!([{
        "id": "trim", "type": "transform", "config": { "ops": [{ "op": "pick", "keys": [] }] }
    }]));
    assert_issue(&pick, "invalid_config", "nodes[1].config.ops[0]");

    let sort = check_chain(json!([{
        "id": "trim", "type": "transform", "config": { "ops": [{ "op": "sort", "by": "a b" }] }
    }]));
    assert_issue(&sort, "invalid_config", "nodes[1].config.ops[0]");
}

#[test]
fn loop_bodies_are_checked() {
    let unknown = check_parts(
        json!([start(), { "id": "each", "type": "loop", "config": { "count": 2, "body": ["ghost"] } }, finish()]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert_issue(&unknown, "unknown_node", "nodes[1].config.body");

    let itself = check_parts(
        json!([start(), { "id": "each", "type": "loop", "config": { "count": 2, "body": ["each"] } }, finish()]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert_error(&itself, "invalid_config");

    let trigger = check_parts(
        json!([start(), { "id": "each", "type": "loop", "config": { "count": 2, "body": ["start"] } }, finish()]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert_error(&trigger, "invalid_config");

    let shared = check_parts(
        json!([
            start(),
            { "id": "outer", "type": "loop", "config": { "count": 2, "body": ["shared"] } },
            { "id": "inner", "type": "loop", "config": { "count": 2, "body": ["shared"] } },
            { "id": "shared", "type": "transform" },
            finish()
        ]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert!(
        shared
            .errors
            .iter()
            .any(|issue| issue.code == "loop_scope_conflict"),
        "expected a scope conflict, got {:#?}",
        pairs(&shared)
    );
}

// ---------------------------------------------------------------------------
// Edges and graph shape
// ---------------------------------------------------------------------------

#[test]
fn edges_must_reference_known_nodes_and_ports() {
    let unknown_from = check_parts(
        json!([start(), finish()]),
        json!([edge("ghost", "event", "finish", "result")]),
    );
    assert_issue(&unknown_from, "unknown_node", "edges[0].from");

    let unknown_to = check_parts(
        json!([start(), finish()]),
        json!([edge("start", "event", "ghost", "result")]),
    );
    assert_issue(&unknown_to, "unknown_node", "edges[0].to");

    let unknown_from_port = check_parts(
        json!([start(), finish()]),
        json!([edge("start", "widget", "finish", "result")]),
    );
    assert_issue(&unknown_from_port, "unknown_port", "edges[0].from_port");

    let unknown_to_port = check_parts(
        json!([start(), finish()]),
        json!([edge("start", "event", "finish", "widget")]),
    );
    assert_issue(&unknown_to_port, "unknown_port", "edges[0].to_port");
}

#[test]
fn triggers_have_no_inbound_edges_and_ends_have_no_outbound_edges() {
    let inbound = check_parts(
        json!([start(), finish()]),
        json!([edge("start", "event", "start", "event")]),
    );
    assert_issue(&inbound, "trigger_has_inbound", "edges[0].to");

    let outbound = check_parts(
        json!([start(), finish()]),
        json!([edge("finish", "", "start", "event")]),
    );
    assert_issue(&outbound, "end_has_outbound", "edges[0].from");
}

#[test]
fn cycles_and_unreachable_nodes_are_detected() {
    let cyclic = check_parts(
        json!([
            start(),
            { "id": "a", "type": "transform" },
            { "id": "b", "type": "transform" },
            finish()
        ]),
        json!([
            edge("start", "event", "a", "input"),
            edge("a", "output", "b", "input"),
            edge("b", "output", "a", "input"),
            edge("a", "output", "finish", "result")
        ]),
    );
    assert_error(&cyclic, "cycle");

    let unreachable = check_parts(
        json!([start(), { "id": "island", "type": "transform" }, finish()]),
        json!([edge("start", "event", "finish", "result")]),
    );
    assert_issue(&unreachable, "unreachable_node", "nodes[1]");
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

#[test]
fn templates_are_checked_for_syntax_secrets_and_sources() {
    let syntax = check_chain(json!([{
        "id": "tail", "type": "end", "config": { "result": "{{ broken" }
    }]));
    assert_error(&syntax, "invalid_template");

    let secret = check_chain(json!([{
        "id": "leak", "type": "transform", "inputs": { "value": "{{ secrets.api_key }}" }
    }]));
    assert_issue(&secret, "secret_outside_config", "nodes[1].inputs.value");

    let unknown = check_chain(json!([{
        "id": "leak", "type": "transform", "config": { "template": "{{ ghost }}" }
    }]));
    assert_issue(&unknown, "unknown_source", "nodes[1].config.template");

    let condition = check_chain(json!([{
        "id": "gate", "type": "condition", "config": { "expr": "{{ ghost }} == 1" }
    }]));
    assert_issue(&condition, "unknown_source", "nodes[1].config.expr");

    let upstream = check_chain(json!([{
        "id": "tail", "type": "end", "config": { "result": "{{ start.event }}" }
    }]));
    assert!(
        !upstream.has_code("unknown_source"),
        "{:#?}",
        pairs(&upstream)
    );
}

// ---------------------------------------------------------------------------
// App manifest
// ---------------------------------------------------------------------------

#[test]
fn a_minimal_app_manifest_is_valid() {
    let validation = check_app(json!({ "id": "mini", "name": "Mini" }));
    assert!(validation.is_valid(), "{:#?}", pairs(&validation));
}

#[test]
fn app_identity_and_inputs_are_checked() {
    let identity = check_app(json!({ "id": "Bad Id", "name": "", "icon": "Cloud Icon" }));
    assert_issue(&identity, "invalid_id", "app.id");
    assert_issue(&identity, "missing_name", "app.name");
    assert_issue(&identity, "invalid_icon", "app.icon");

    let duplicate = check_app(json!({
        "id": "mini", "name": "Mini",
        "inputs": [
            { "id": "city", "type": "string", "label": "City" },
            { "id": "city", "type": "string", "label": "City again" }
        ]
    }));
    assert_issue(&duplicate, "duplicate_id", "app.inputs[1].id");

    let bad_type = check_app(json!({
        "id": "mini", "name": "Mini",
        "inputs": [{ "id": "when", "type": "date", "label": "When" }]
    }));
    assert_issue(&bad_type, "invalid_app_input", "app.inputs[0].type");

    let select = check_app(json!({
        "id": "mini", "name": "Mini",
        "inputs": [{ "id": "size", "type": "select", "label": "Size" }]
    }));
    assert_issue(&select, "invalid_app_input", "app.inputs[0].options");

    let label = check_app(json!({
        "id": "mini", "name": "Mini",
        "inputs": [{ "id": "city", "type": "string", "label": "" }]
    }));
    assert_issue(&label, "missing_label", "app.inputs[0].label");
}

#[test]
fn app_outputs_are_checked() {
    let duplicate = check_app(json!({
        "id": "mini", "name": "Mini",
        "outputs": [
            { "id": "report", "type": "markdown", "label": "Report" },
            { "id": "report", "type": "text", "label": "Report again" }
        ]
    }));
    assert_issue(&duplicate, "duplicate_id", "app.outputs[1].id");

    let bad_type = check_app(json!({
        "id": "mini", "name": "Mini",
        "outputs": [{ "id": "report", "type": "html", "label": "Report" }]
    }));
    assert_issue(&bad_type, "invalid_app_output", "app.outputs[0].type");
}

#[test]
fn app_layout_blocks_are_checked() {
    let not_an_object = check_app(json!({
        "id": "mini", "name": "Mini", "layout": ["form"]
    }));
    assert_issue(&not_an_object, "invalid_app_layout", "app.layout[0]");

    let missing_type = check_app(json!({
        "id": "mini", "name": "Mini", "layout": [{ "fields": [] }]
    }));
    assert_issue(&missing_type, "invalid_app_layout", "app.layout[0].type");

    let unknown_kind = check_app(json!({
        "id": "mini", "name": "Mini", "layout": [{ "type": "chart" }]
    }));
    assert_issue(&unknown_kind, "invalid_app_layout", "app.layout[0].type");

    let form = check_app(json!({
        "id": "mini", "name": "Mini", "layout": [{ "type": "form", "fields": ["town"] }]
    }));
    assert_issue(&form, "unknown_reference", "app.layout[0].fields");

    let output = check_app(json!({
        "id": "mini", "name": "Mini", "layout": [{ "type": "output", "ref": "summary" }]
    }));
    assert_issue(&output, "unknown_reference", "app.layout[0].ref");

    let button = check_app(json!({
        "id": "mini", "name": "Mini", "layout": [{ "type": "button" }]
    }));
    assert_issue(&button, "invalid_app_layout", "app.layout[0].label");

    let action = check_app(json!({
        "id": "mini", "name": "Mini", "layout": [{ "type": "button", "label": "Go", "action": "reset" }]
    }));
    assert_issue(&action, "invalid_app_layout", "app.layout[0].action");

    let history = check_app(json!({
        "id": "mini", "name": "Mini", "layout": [{ "type": "history", "limit": 0 }]
    }));
    assert_issue(&history, "invalid_app_layout", "app.layout[0].limit");
}

// ---------------------------------------------------------------------------
// Lints
// ---------------------------------------------------------------------------

#[test]
fn lints_are_warnings_that_do_not_block_a_run() {
    let unused = check_parts(
        json!([start(), { "id": "dead", "type": "transform" }, finish()]),
        json!([
            edge("start", "event", "dead", "input"),
            edge("start", "event", "finish", "result")
        ]),
    );
    assert_warning(&unused, "unused_node");
    assert!(unused.is_valid(), "{:#?}", unused.errors);

    let ports = check_chain(json!([{
        "id": "fetch", "type": "http", "config": { "method": "GET", "url": "https://example.com" }
    }]));
    assert_warning(&ports, "unreachable_port");
    assert!(ports.is_valid(), "{:#?}", ports.errors);

    let backoff = check_chain(json!([{
        "id": "fetch",
        "type": "http",
        "timeout_s": 1,
        "retry": { "max": 1, "backoff_ms": 2000 },
        "config": { "method": "GET", "url": "https://example.com" }
    }]));
    assert_issue(&backoff, "backoff_exceeds_timeout", "nodes[1].retry");
    assert!(backoff.is_valid(), "{:#?}", backoff.errors);
}

#[test]
fn messages_and_into_result_describe_the_issues() {
    let validation = check_parts(
        json!([start()]),
        json!([edge("start", "event", "ghost", "result")]),
    );

    let messages = validation.messages();
    assert!(!messages.is_empty());
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("edges[0].to: ")),
        "{messages:?}"
    );

    let error = validation.into_result().expect_err("errors block the run");
    let text = error.to_string();
    assert!(text.contains("error(s):"), "{text}");
    assert!(text.contains("edges[0].to"), "{text}");
}
