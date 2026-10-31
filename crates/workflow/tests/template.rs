//! Integration tests for the `{{ … }}` template engine (spec §1.5, Appendix D).
//!
//! The engine is pure: a template compiles once, then renders against a
//! [`RenderContext`] built by the caller. Everything here is deterministic
//! because the clock is injected.

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};

use fergents_workflow::template::{
    as_text, render_value, truthy, NoSecrets, RenderContext, SecretResolver, Template,
};
use fergents_workflow::WorkflowError;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 31, 9, 30, 0)
        .single()
        .expect("a valid instant")
}

fn context(data: Value) -> RenderContext {
    RenderContext {
        data,
        result: None,
        run_id: "01J8ZY4Q7K".to_string(),
        workflow_id: "weather-report".to_string(),
        now: now(),
        secrets: Arc::new(NoSecrets),
    }
}

fn render(source: &str, data: Value) -> Value {
    Template::parse(source)
        .expect("template parses")
        .render(&context(data))
        .expect("template renders")
}

fn render_err(source: &str, data: Value) -> WorkflowError {
    Template::parse(source)
        .expect("template parses")
        .render(&context(data))
        .expect_err("render fails")
}

fn parse_err(source: &str) -> WorkflowError {
    Template::parse(source).expect_err("template is rejected")
}

#[test]
fn literal_templates_are_static_and_render_verbatim() {
    let template = Template::parse("hello, world").expect("parses");
    assert!(template.is_static());
    assert!(!template.reads_secret());
    assert!(template.context_paths().is_empty());
    assert_eq!(
        template.render(&context(json!({}))).expect("renders"),
        json!("hello, world")
    );

    let empty = Template::parse("").expect("parses");
    assert!(empty.is_static());
    assert_eq!(
        empty.render(&context(json!({}))).expect("renders"),
        json!("")
    );
}

#[test]
fn a_single_expression_keeps_the_native_json_type() {
    let data = json!({"fetch": {"items": [1, 2, 3], "status": 200, "ok": true}});
    assert_eq!(render("{{ fetch.items }}", data.clone()), json!([1, 2, 3]));
    assert_eq!(render("{{ fetch.status }}", data.clone()), json!(200));
    assert_eq!(render("{{ fetch.ok }}", data), json!(true));
}

#[test]
fn surrounding_text_makes_the_result_a_string() {
    let data = json!({"fetch": {"status": 200}});
    assert_eq!(
        render("status={{ fetch.status }}", data),
        json!("status=200")
    );
}

#[test]
fn nested_paths_walk_keys_and_indices() {
    let data = json!({"a": {"b": [{"c": 1}, {"c": 2}]}, "grid": [[1, 2], [3, 4]]});
    assert_eq!(render("{{ a.b[1].c }}", data.clone()), json!(2));
    assert_eq!(render("{{ grid[1][0] }}", data), json!(3));
}

#[test]
fn whitespace_inside_the_braces_is_ignored() {
    let data = json!({"a": "x"});
    assert_eq!(render("{{   a   }}", data.clone()), json!("x"));
    assert_eq!(render("{{ a|upper }}", data), json!("X"));
}

#[test]
fn builtins_render_identity_and_the_injected_clock() {
    let ctx = context(json!({}));
    let identity = Template::parse("{{ run.id }}/{{ workflow.id }}").expect("parses");
    assert_eq!(
        identity.render(&ctx).expect("renders"),
        json!("01J8ZY4Q7K/weather-report")
    );
    assert_eq!(
        render("{{ now }}", json!({})),
        json!("2026-10-31T09:30:00+00:00")
    );
    assert_eq!(render("{{ today }}", json!({})), json!("2026-10-31"));
}

#[test]
fn the_result_root_reads_the_current_node_result() {
    let mut ctx = context(json!({"a": 1}));
    ctx.result = Some(json!({"text": "hi"}));
    let template = Template::parse("{{ result.text }}").expect("parses");
    assert_eq!(template.render(&ctx).expect("renders"), json!("hi"));
    assert!(template.context_paths().is_empty());
}

#[test]
fn length_counts_characters_items_and_keys() {
    assert_eq!(render("{{ s | length }}", json!({"s": "héllo"})), json!(5));
    assert_eq!(render("{{ xs | length }}", json!({"xs": [1, 2]})), json!(2));
    assert_eq!(render("{{ m | length }}", json!({"m": {"a": 1}})), json!(1));
    assert_eq!(render("{{ m | length }}", json!({"m": null})), json!(0));

    let err = render_err("{{ m | length }}", json!({"m": true}));
    assert!(
        err.to_string()
            .contains("filter `length` expects string, array, or object, got boolean"),
        "{err}"
    );
}

#[test]
fn upper_and_lower_require_strings() {
    assert_eq!(render("{{ s | upper }}", json!({"s": "Abc"})), json!("ABC"));
    assert_eq!(render("{{ s | lower }}", json!({"s": "AbC"})), json!("abc"));

    let err = render_err("{{ s | upper }}", json!({"s": 5}));
    assert!(matches!(err, WorkflowError::TemplateInvalid(_)));
    assert_eq!(err.code(), "workflow_template_invalid");
    assert!(
        err.to_string()
            .contains("filter `upper` expects string, got number"),
        "{err}"
    );
}

#[test]
fn json_filter_serializes_the_value() {
    assert_eq!(
        render("{{ data | json }}", json!({"data": {"b": [1, 2]}})),
        json!("{\"b\":[1,2]}")
    );
}

#[test]
fn join_flattens_arrays_with_a_separator() {
    assert_eq!(
        render("{{ xs | join(\", \") }}", json!({"xs": [1, "a", true]})),
        json!("1, a, true")
    );
    assert_eq!(
        render("{{ xs | join('') }}", json!({"xs": ["a", "b"]})),
        json!("ab")
    );

    let err = render_err("{{ xs | join(\",\") }}", json!({"xs": "nope"}));
    assert!(
        err.to_string()
            .contains("filter `join` expects array, got string"),
        "{err}"
    );
}

#[test]
fn default_rescues_a_missing_path() {
    assert_eq!(
        render("{{ missing | default(\"n/a\") }}", json!({})),
        json!("n/a")
    );
    assert_eq!(render("{{ missing | default(0) }}", json!({})), json!(0));
    assert_eq!(
        render("{{ a.b.c | default(false) }}", json!({"a": {"b": {}}})),
        json!(false)
    );
    assert_eq!(
        render("{{ a | default(\"z\") }}", json!({"a": "v"})),
        json!("v")
    );
}

#[test]
fn a_missing_path_without_a_default_fails_the_render() {
    let err = render_err("{{ a.b.c }}", json!({"a": {"b": {}}}));
    assert!(
        matches!(err, WorkflowError::TemplateMissing(ref path) if path == "a.b.c"),
        "{err}"
    );
    assert_eq!(err.code(), "workflow_template_missing");

    let err = render_err("{{ missing }}", json!({}));
    assert!(matches!(err, WorkflowError::TemplateMissing(ref path) if path == "missing"));
}

#[test]
fn missing_paths_inside_larger_text_also_fail() {
    let err = render_err("x={{ missing }}", json!({}));
    assert!(matches!(err, WorkflowError::TemplateMissing(_)));
}

#[test]
fn date_filters_format_rfc3339_epochs_and_the_clock() {
    assert_eq!(
        render(
            "{{ stamp | date(\"%Y/%m/%d\") }}",
            json!({"stamp": "2026-10-31T09:30:00Z"})
        ),
        json!("2026/10/31")
    );
    assert_eq!(
        render(
            "{{ stamp | date(\"%Y-%m-%dT%H:%M:%SZ\") }}",
            json!({"stamp": 0})
        ),
        json!("1970-01-01T00:00:00Z")
    );
    assert_eq!(
        render("{{ now | date(\"%d.%m.%Y\") }}", json!({})),
        json!("31.10.2026")
    );
}

#[test]
fn date_filters_reject_garbage() {
    let err = render_err("{{ stamp | date(\"%Y\") }}", json!({"stamp": "yesterday"}));
    assert!(
        err.to_string()
            .contains("`yesterday` is not an RFC 3339 timestamp or an epoch"),
        "{err}"
    );
    assert_eq!(err.code(), "workflow_template_invalid");

    let err = parse_err("{{ stamp | date(\"%Q\") }}");
    assert!(
        err.to_string().contains("invalid date format `%Q`"),
        "{err}"
    );
}

#[test]
fn syntax_errors_are_rejected_at_parse_time() {
    let cases = [
        ("{{ a", "unterminated `{{`"),
        ("x {{ a }", "unterminated `{{`"),
        ("{{ }}", "empty expression"),
        ("{{ 1a }}", "expected an identifier, found `1`"),
        ("{{ a b }}", "unexpected `b` in path"),
        ("{{ a[ }}", "unterminated `[`"),
        ("{{ a[x] }}", "array index must be an integer"),
        ("{{ a | nope }}", "unknown filter `nope`"),
        ("{{ a | length(1) }}", "unknown filter `length`"),
        ("{{ a | default(nope) }}", "is not a literal"),
        ("{{ a | join }}", "`join` requires an argument"),
        ("{{ a | join(\",\" }}", "unterminated `(`"),
        ("{{ a | join(\"x) }}", "unterminated string"),
    ];
    for (source, expected) in cases {
        let err = parse_err(source);
        assert!(
            err.to_string().contains(expected),
            "`{source}` should report `{expected}`, got `{err}`"
        );
        assert_eq!(err.code(), "workflow_template_invalid");
    }
}

#[test]
fn root_specific_paths_are_enforced() {
    let cases = [
        ("{{ run }}", "`run` only supports `run.id`"),
        ("{{ run.foo }}", "`run` only supports `run.id`"),
        ("{{ workflow.x }}", "`workflow` only supports `workflow.id`"),
        ("{{ now.x }}", "`now` takes no path"),
        ("{{ today.x }}", "`today` takes no path"),
        ("{{ secrets }}", "requires a secret name"),
    ];
    for (source, expected) in cases {
        let err = parse_err(source);
        assert!(
            err.to_string().contains(expected),
            "`{source}` should report `{expected}`, got `{err}`"
        );
    }
}

#[test]
fn doubled_braces_are_not_an_escape() {
    // There is no escape for a literal `{{`: every occurrence starts an
    // expression, so `{{{{ a }}` is a syntax error rather than literal text.
    let err = parse_err("{{{{ a }}");
    assert!(
        err.to_string()
            .contains("expected an identifier, found `{`"),
        "{err}"
    );
}

#[test]
fn secrets_resolve_through_the_resolver_and_are_flagged() {
    struct Tokens;

    impl SecretResolver for Tokens {
        fn resolve(&self, name: &str) -> Option<Value> {
            (name == "github_token").then(|| json!("ghp_secret"))
        }
    }

    let mut ctx = context(json!({}));
    ctx.secrets = Arc::new(Tokens);

    let template = Template::parse("Bearer {{ secrets.github_token }}").expect("parses");
    assert!(template.reads_secret());
    assert_eq!(
        template.render(&ctx).expect("renders"),
        json!("Bearer ghp_secret")
    );

    let filtered = Template::parse("{{ secrets.github_token | upper }}").expect("parses");
    assert!(filtered.reads_secret());
    assert_eq!(filtered.render(&ctx).expect("renders"), json!("GHP_SECRET"));

    assert!(!Template::parse("{{ a }}").expect("parses").reads_secret());
}

#[test]
fn unresolved_secrets_fail_like_missing_paths() {
    let err = render_err("{{ secrets.github_token }}", json!({}));
    assert!(
        matches!(err, WorkflowError::TemplateMissing(ref path) if path == "secrets.github_token"),
        "{err}"
    );
    assert_eq!(err.code(), "workflow_template_missing");
}

#[test]
fn context_paths_lists_only_context_roots() {
    let source = "{{ fetch.items[0].title }} {{ result.text }} {{ secrets.token }} \
                  {{ now }} {{ today }} {{ run.id }} {{ workflow.id }} {{ variables.city }}";
    let template = Template::parse(source).expect("parses");
    assert_eq!(
        template.context_paths(),
        vec![
            "fetch.items[0].title".to_string(),
            "variables.city".to_string()
        ]
    );
}

#[test]
fn render_value_recurses_through_arrays_and_objects() {
    let ctx = context(json!({"a": "x", "n": 7}));
    let input = json!({
        "template": "{{ a }}",
        "number": 1,
        "flag": true,
        "nested": { "deep": ["{{ a }}-{{ n }}", false] },
        "{{ a }}": "keys are untouched",
    });
    let rendered = render_value(&input, &ctx).expect("renders");
    assert_eq!(
        rendered,
        json!({
            "template": "x",
            "number": 1,
            "flag": true,
            "nested": { "deep": ["x-7", false] },
            "{{ a }}": "keys are untouched",
        })
    );
}

#[test]
fn as_text_matches_json_rendering() {
    assert_eq!(as_text(&Value::Null), "");
    assert_eq!(as_text(&json!("x")), "x");
    assert_eq!(as_text(&json!(7)), "7");
    assert_eq!(as_text(&json!(true)), "true");
    assert_eq!(as_text(&json!({"a": 1})), "{\"a\":1}");
    assert_eq!(as_text(&json!([1, 2])), "[1,2]");
}

#[test]
fn truthy_follows_json_truthiness() {
    assert!(!truthy(&Value::Null));
    assert!(!truthy(&json!(false)));
    assert!(!truthy(&json!(0)));
    assert!(!truthy(&json!(0.0)));
    assert!(!truthy(&json!("")));
    assert!(!truthy(&json!([])));
    assert!(!truthy(&json!({})));

    assert!(truthy(&json!(true)));
    assert!(truthy(&json!(1)));
    assert!(truthy(&json!(-1)));
    assert!(truthy(&json!("x")));
    assert!(truthy(&json!([0])));
    assert!(truthy(&json!({"a": 0})));
}

#[test]
fn render_string_coerces_non_strings() {
    let ctx = context(json!({"n": 7, "m": {"a": 1}}));
    let number = Template::parse("{{ n }}").expect("parses");
    assert_eq!(number.render_string(&ctx).expect("renders"), "7");
    let object = Template::parse("{{ m }}").expect("parses");
    assert_eq!(object.render_string(&ctx).expect("renders"), "{\"a\":1}");
}

#[test]
fn with_data_keeps_identity_and_clock_but_drops_the_result() {
    let mut ctx = context(json!({"a": 1}));
    ctx.result = Some(json!("previous"));
    let derived = ctx.with_data(json!({"b": 2}));

    assert_eq!(derived.data, json!({"b": 2}));
    assert!(derived.result.is_none());
    assert_eq!(derived.run_id, ctx.run_id);
    assert_eq!(derived.workflow_id, ctx.workflow_id);
    assert_eq!(derived.now, ctx.now);
}

#[test]
fn for_validation_starts_empty() {
    let ctx = RenderContext::for_validation(now());
    assert_eq!(ctx.data, json!({}));
    assert!(ctx.result.is_none());
    assert_eq!(ctx.run_id, "");
    assert_eq!(ctx.workflow_id, "");

    // The clock is still injected, so date filters stay testable.
    assert_eq!(
        Template::parse("{{ today }}")
            .expect("parses")
            .render(&ctx)
            .expect("renders"),
        json!("2026-10-31")
    );
}
