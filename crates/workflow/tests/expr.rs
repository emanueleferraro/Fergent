//! Integration tests for the closed condition and transform language
//! (spec §1.6, Appendix D).
//!
//! Conditions are parsed into an AST and evaluated; user text is never
//! executed. Transforms are declarative operation lists that walk JSON values.
//! Both are pure, so every test here is deterministic.

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};

use fergents_workflow::expr::{
    apply_ops, validate_templates, with_bindings, Comparison, Condition, TransformOp,
};
use fergents_workflow::template::{NoSecrets, RenderContext, Template};
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

fn eval(source: &str, data: Value) -> bool {
    Condition::parse(source)
        .expect("condition parses")
        .eval(&context(data))
        .expect("condition evaluates")
}

fn eval_err(source: &str, data: Value) -> WorkflowError {
    Condition::parse(source)
        .expect("condition parses")
        .eval(&context(data))
        .expect_err("condition fails")
}

fn parse_err(source: &str) -> WorkflowError {
    Condition::parse(source).expect_err("condition is rejected")
}

fn op(value: Value) -> TransformOp {
    serde_json::from_value(value).expect("operation deserializes")
}

#[test]
fn literals_and_paths_are_tested_for_truthiness() {
    assert!(eval("true", json!({})));
    assert!(!eval("false", json!({})));
    assert!(!eval("null", json!({})));

    assert!(eval("flag", json!({"flag": true})));
    assert!(eval("count", json!({"count": 1})));
    assert!(!eval("zero", json!({"zero": 0})));
    assert!(!eval("empty", json!({"empty": []})));
    assert!(!eval("missing", json!({})));
}

#[test]
fn comparison_operators_accept_words_and_symbols() {
    let cases = [
        ("n eq 1", true),
        ("n == 1", true),
        ("n ne 2", true),
        ("n != 2", true),
        ("n gt 0", true),
        ("n > 0", true),
        ("n gte 1", true),
        ("n >= 1", true),
        ("n lt 2", true),
        ("n < 2", true),
        ("n lte 1", true),
        ("n <= 1", true),
        ("n eq 2", false),
        ("n gt 1", false),
        ("n lt 1", false),
    ];
    for (source, expected) in cases {
        assert_eq!(eval(source, json!({"n": 1})), expected, "`{source}`");
    }
}

#[test]
fn numeric_comparisons_cross_integer_and_float_boundaries() {
    assert!(eval("n eq 1", json!({"n": 1.0})));
    assert!(eval("n gt 1", json!({"n": 1.5})));
    assert!(eval("n eq 1.0", json!({"n": 1})));
}

#[test]
fn equality_is_structural() {
    assert!(eval(
        "a eq b",
        json!({"a": {"x": [1, 2]}, "b": {"x": [1, 2]}})
    ));
    assert!(!eval("a eq b", json!({"a": {"x": 1}, "b": {"x": 2}})));
    assert!(eval("a eq [1, 2]", json!({"a": [1, 2]})));
    assert!(eval("a eq null", json!({})));
    assert!(eval("missing eq null", json!({})));
}

#[test]
fn ordering_requires_comparable_types() {
    assert!(eval("\"a\" lt \"b\"", json!({})));
    assert!(eval("true gt false", json!({})));
    assert!(eval("-3 lt 0", json!({})));

    // Mixed types have no ordering, so every range comparison is false.
    assert!(!eval("s gt 1", json!({"s": "a"})));
    assert!(!eval("1 lt \"2\"", json!({})));
    assert!(!eval("s gte s", json!({"s": {"a": 1}})));
}

#[test]
fn membership_and_containment_are_duals() {
    assert!(eval("x in xs", json!({"x": 2, "xs": [1, 2, 3]})));
    assert!(!eval("x in xs", json!({"x": 9, "xs": [1, 2, 3]})));
    assert!(eval("\"k\" in obj", json!({"obj": {"k": 1}})));
    assert!(eval("\"ell\" in text", json!({"text": "hello"})));
    assert!(!eval("x in 5", json!({"x": 5})));

    assert!(eval("xs contains 2", json!({"xs": [1, 2, 3]})));
    assert!(eval("obj contains \"k\"", json!({"obj": {"k": 0}})));
    assert!(eval("text contains \"ell\"", json!({"text": "hello"})));
    assert!(!eval("5 contains x", json!({"x": 5})));
}

#[test]
fn matches_uses_regular_expressions() {
    assert!(eval("name matches \"^ab\"", json!({"name": "abc"})));
    assert!(!eval("name matches \"xyz\"", json!({"name": "abc"})));
    assert!(eval("name matches \"b.\"", json!({"name": "abc"})));

    let err = eval_err("name matches \"(\"", json!({"name": "abc"}));
    assert!(err.to_string().contains("invalid pattern"), "{err}");
    assert!(matches!(err, WorkflowError::ExpressionInvalid(_)));

    let err = eval_err("name matches 5", json!({"name": "abc"}));
    assert!(
        err.to_string()
            .contains("`matches` expects a pattern string, got 5"),
        "{err}"
    );
}

#[test]
fn and_or_not_compose_with_precedence() {
    assert!(!eval("a and b", json!({"a": true, "b": false})));
    assert!(eval("a or b", json!({"a": false, "b": true})));
    assert!(!eval("not a", json!({"a": true})));
    assert!(eval("not not a", json!({"a": true})));

    // `and` binds tighter than `or`: this is `true or (false and false)`.
    assert!(eval("true or false and false", json!({})));
    assert!(!eval("not true and false", json!({})));
    assert!(eval("not (true and false)", json!({})));
}

#[test]
fn parentheses_group_conditions() {
    let data = json!({"a": false, "b": true, "c": false});
    assert!(!eval("(a or b) and c", data.clone()));
    assert!(eval("a or (b and not c)", data));
}

#[test]
fn template_operands_are_accepted() {
    assert!(eval("{{ a }} eq 1", json!({"a": 1})));
    assert!(eval("{{ a.b }} contains \"x\"", json!({"a": {"b": ["x"]}})));
}

#[test]
fn array_literals_hold_literals_only() {
    assert!(eval("x in [1, 2, 3]", json!({"x": 3})));
    assert!(!eval("x in []", json!({"x": 1})));
    assert!(eval("\"b\" in [\"a\", \"b\"]", json!({})));

    let err = parse_err("x in [a]");
    assert!(
        err.to_string()
            .contains("array literals hold literals only, found `a`"),
        "{err}"
    );
    let err = parse_err("x in [1 2]");
    assert!(
        err.to_string()
            .contains("expected `,` or `]` in array literal"),
        "{err}"
    );
}

#[test]
fn string_literals_honour_quotes_and_escapes() {
    assert!(eval("s eq 'single'", json!({"s": "single"})));
    assert!(eval(
        "s eq \"he said \\\"hi\\\"\"",
        json!({"s": "he said \"hi\""})
    ));
}

#[test]
fn parse_errors_name_the_offending_token() {
    let cases = [
        ("", "empty condition expression"),
        ("   ", "empty condition expression"),
        ("a eq", "expected an operand"),
        ("a eq 1 eq 2", "unexpected `eq` in `a eq 1 eq 2`"),
        ("a @ 1", "unexpected character `@`"),
        ("(a eq 1", "expected `)` to close the group"),
        ("()", "expected an operand, found `)`"),
        ("a eq \"x", "unterminated string literal"),
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
fn context_paths_exclude_builtins() {
    let condition = Condition::parse(
        "trigger.body eq 'x' and result.ok and now gt 0 \
         and today eq '2026-10-31' and secrets.t ne null \
         and run.id eq workflow.id",
    )
    .expect("parses");
    assert_eq!(condition.context_paths(), vec!["trigger.body".to_string()]);
}

#[test]
fn reads_secret_detects_secret_operands() {
    assert!(Condition::parse("secrets.token ne null")
        .expect("parses")
        .reads_secret());
    assert!(Condition::parse("a eq b and secrets.k eq 1")
        .expect("parses")
        .reads_secret());
    assert!(!Condition::parse("a eq b").expect("parses").reads_secret());
}

#[test]
fn with_bindings_adds_names_without_mutating_the_original() {
    let ctx = context(json!({"a": 1}));
    let bound = with_bindings(&ctx, &[("item", json!(5)), ("index", json!(0))]);

    let item = Template::parse("{{ item }}").expect("parses");
    assert_eq!(item.render(&bound).expect("renders"), json!(5));
    let kept = Template::parse("{{ a }}").expect("parses");
    assert_eq!(kept.render(&bound).expect("renders"), json!(1));

    assert!(matches!(
        item.render(&ctx),
        Err(WorkflowError::TemplateMissing(_))
    ));
    assert_eq!(bound.run_id, ctx.run_id);
}

#[test]
fn comparison_wire_names_are_stable() {
    let cases = [
        (Comparison::Eq, "eq"),
        (Comparison::Ne, "ne"),
        (Comparison::Gt, "gt"),
        (Comparison::Gte, "gte"),
        (Comparison::Lt, "lt"),
        (Comparison::Lte, "lte"),
        (Comparison::In, "in"),
        (Comparison::Contains, "contains"),
        (Comparison::Matches, "matches"),
    ];
    for (comparison, name) in cases {
        assert_eq!(comparison.as_str(), name);
    }
}

#[test]
fn transform_operations_round_trip_through_json() {
    let pick = op(json!({"op": "pick", "keys": ["a", "b"]}));
    assert_eq!(
        pick,
        TransformOp::Pick {
            from: None,
            keys: vec!["a".to_string(), "b".to_string()],
        }
    );
    assert_eq!(
        serde_json::to_value(&pick).expect("serializes")["op"],
        json!("pick")
    );

    let sort = op(json!({
        "op": "sort",
        "from": "{{ data.rows }}",
        "by": "meta.rank",
        "desc": true,
    }));
    assert_eq!(
        sort,
        TransformOp::Sort {
            from: Some("{{ data.rows }}".to_string()),
            by: "meta.rank".to_string(),
            desc: true,
        }
    );

    assert!(serde_json::from_value::<TransformOp>(json!({"op": "nope"})).is_err());
}

#[test]
fn transform_validation_catches_bad_operations() {
    let err = TransformOp::Pick {
        from: None,
        keys: Vec::new(),
    }
    .validate()
    .expect_err("empty pick is rejected");
    assert!(
        err.to_string().contains("`pick` requires at least one key"),
        "{err}"
    );

    for by in ["", "a..b", "1a", "a b"] {
        let err = TransformOp::Sort {
            from: None,
            by: by.to_string(),
            desc: false,
        }
        .validate()
        .expect_err("bad sort key is rejected");
        assert!(
            err.to_string().contains("must be a dotted identifier path"),
            "{err}"
        );
    }

    let err = op(json!({"op": "filter", "when": "a eq"}))
        .validate()
        .expect_err("invalid condition is rejected");
    assert!(matches!(err, WorkflowError::ExpressionInvalid(_)));

    let err = op(json!({"op": "map", "to": {"v": "{{ bad"}}))
        .validate()
        .expect_err("invalid template is rejected");
    assert!(matches!(err, WorkflowError::TemplateInvalid(_)));

    assert!(op(json!({"op": "pick", "keys": ["a"]})).validate().is_ok());
    assert!(op(json!({"op": "sort", "by": "meta.rank"}))
        .validate()
        .is_ok());
    assert!(op(json!({"op": "merge", "deep": true})).validate().is_ok());
}

#[test]
fn pick_keeps_the_requested_keys() {
    let ctx = context(json!({}));
    let picked = apply_ops(
        &[op(json!({"op": "pick", "keys": ["b", "a", "missing"]}))],
        json!({"a": 1, "b": 2, "c": 3}),
        &ctx,
    )
    .expect("applies");
    assert_eq!(picked, json!({"b": 2, "a": 1}));
}

#[test]
fn map_projects_each_item_through_a_template_object() {
    let ctx = context(json!({}));
    let mapped = apply_ops(
        &[op(json!({
            "op": "map",
            "to": {"name": "{{ item.name }}", "rank": "{{ index }}"},
        }))],
        json!([{"name": "a"}, {"name": "b"}]),
        &ctx,
    )
    .expect("applies");
    assert_eq!(
        mapped,
        json!([{"name": "a", "rank": 0}, {"name": "b", "rank": 1}])
    );
}

#[test]
fn map_can_select_its_source_with_from() {
    let ctx = context(json!({"data": {"xs": [1, 2]}}));
    let mapped = apply_ops(
        &[op(json!({
            "op": "map",
            "from": "{{ data.xs }}",
            "to": {"v": "{{ item }}"},
        }))],
        Value::Null,
        &ctx,
    )
    .expect("applies");
    assert_eq!(mapped, json!([{"v": 1}, {"v": 2}]));
}

#[test]
fn filter_keeps_items_matching_a_condition() {
    let ctx = context(json!({}));
    let filtered = apply_ops(
        &[op(json!({"op": "filter", "when": "item.score gte 10"}))],
        json!([{"score": 5}, {"score": 10}, {"score": 30}]),
        &ctx,
    )
    .expect("applies");
    assert_eq!(filtered, json!([{"score": 10}, {"score": 30}]));
}

#[test]
fn merge_combines_an_array_of_objects() {
    let ctx = context(json!({}));
    let items = json!([{"a": {"x": 1}}, {"a": {"y": 2}}]);

    let shallow = apply_ops(&[op(json!({"op": "merge"}))], items.clone(), &ctx).expect("applies");
    assert_eq!(shallow, json!({"a": {"y": 2}}));

    let deep =
        apply_ops(&[op(json!({"op": "merge", "deep": true}))], items, &ctx).expect("applies");
    assert_eq!(deep, json!({"a": {"x": 1, "y": 2}}));
}

#[test]
fn sort_orders_by_a_dotted_key_and_mirrors_missing_keys() {
    let ctx = context(json!({}));
    let items = json!([{"meta": {"rank": 2}}, {"meta": {"rank": 1}}, {"other": 0}]);

    let ascending = apply_ops(
        &[op(json!({"op": "sort", "by": "meta.rank"}))],
        items.clone(),
        &ctx,
    )
    .expect("applies");
    assert_eq!(
        ascending,
        json!([{"meta": {"rank": 1}}, {"meta": {"rank": 2}}, {"other": 0}])
    );

    // `desc` reverses the whole comparator, so the item without the key
    // (which sorts last ascending) comes first descending.
    let descending = apply_ops(
        &[op(json!({"op": "sort", "by": "meta.rank", "desc": true}))],
        items,
        &ctx,
    )
    .expect("applies");
    assert_eq!(
        descending,
        json!([{"other": 0}, {"meta": {"rank": 2}}, {"meta": {"rank": 1}}])
    );
}

#[test]
fn a_transform_pipeline_threads_the_value() {
    let ctx = context(json!({"data": {"rows": [{"id": 1, "n": 1}, {"id": 2, "n": 5}]}}));
    let ops = vec![
        op(json!({
            "op": "filter",
            "from": "{{ data.rows }}",
            "when": "item.n gt 1",
        })),
        op(json!({"op": "map", "to": {"id": "{{ item.id }}"}})),
    ];
    let out = apply_ops(&ops, Value::Null, &ctx).expect("applies");
    assert_eq!(out, json!([{"id": 2}]));
}

#[test]
fn apply_ops_validates_before_applying() {
    let ctx = context(json!({}));
    let err = apply_ops(
        &[op(json!({"op": "pick", "keys": []}))],
        json!({"a": 1}),
        &ctx,
    )
    .expect_err("invalid operation is rejected");
    assert!(matches!(err, WorkflowError::ExpressionInvalid(_)));
    assert!(
        err.to_string().contains("`pick` requires at least one key"),
        "{err}"
    );
}

#[test]
fn apply_reports_the_operation_and_the_value_type() {
    let ctx = context(json!({}));
    let cases = [
        (
            json!({"op": "pick", "keys": ["a"]}),
            json!(5),
            "expects an object",
        ),
        (
            json!({"op": "map", "to": {"v": "{{ item }}"}}),
            json!({}),
            "expects an array",
        ),
        (
            json!({"op": "filter", "when": "true"}),
            json!("x"),
            "expects an array",
        ),
        (json!({"op": "merge"}), json!([1]), "expects an object"),
        (
            json!({"op": "sort", "by": "rank"}),
            json!("x"),
            "expects an array",
        ),
    ];
    for (operation, input, expected) in cases {
        let err = op(operation)
            .apply(input, &ctx)
            .expect_err("operation fails");
        assert!(
            err.to_string().contains(expected),
            "expected `{expected}`, got `{err}`"
        );
    }
}

#[test]
fn a_missing_from_template_fails_the_transform() {
    let ctx = context(json!({}));
    let err = op(json!({"op": "pick", "from": "{{ nope }}", "keys": ["a"]}))
        .apply(json!({"a": 1}), &ctx)
        .expect_err("missing source fails");
    assert!(matches!(err, WorkflowError::TemplateMissing(ref path) if path == "nope"));
}

#[test]
fn template_validation_walks_nested_values() {
    assert!(validate_templates(&json!({
        "a": ["{{ ok }}"],
        "b": {"c": "plain"},
        "d": 1,
    }))
    .is_ok());

    let err = validate_templates(&json!({"b": {"c": "{{ bad"}}))
        .expect_err("nested invalid template is rejected");
    assert!(matches!(err, WorkflowError::TemplateInvalid(_)));
}
