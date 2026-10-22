//! Independent resource-boundary probes for the public shared-context decoder.
use oe_core::{json::canonical_json, sha256};
use oe_engineering::harness::context;
use serde_json::{Value, json};

const MAX_NODES: usize = 200_000;
const MAX_DEPTH: usize = 96;

fn envelope(root: Value, expected: &Value) -> Value {
    envelope_digest(root, sha256(canonical_json(expected).unwrap()))
}
fn envelope_digest(root: Value, digest: String) -> Value {
    json!({"schema":context::VIEW_SCHEMA,"coverage":"all_normalized_semantic_values",
        "normalized_sha256":digest,"root":root,"dictionary":{}})
}
fn common_rows_digest(count: usize) -> String {
    let expected = Value::Array((0..count).map(|_| json!({"a":0,"b":1})).collect());
    sha256(canonical_json(&expected).unwrap())
}
fn nested(depth: usize) -> Value {
    let mut value = json!(0);
    for _ in 0..depth {
        value = json!({"next":value});
    }
    value
}
fn rejected(view: &Value, code: &str) {
    let result = context::expand(view);
    let observed = result.as_ref().err().map(ToString::to_string);
    println!(
        "expected={code} actual={}",
        observed.as_deref().unwrap_or("accepted")
    );
    assert!(
        observed.as_ref().is_some_and(|s| s == code),
        "unexpected rejection: {observed:?}"
    );
}

#[test]
fn audit_columns_expansion_enforces_output_node_budget() {
    let view = envelope_digest(
        json!({"$oev2_rows":{"common":{"a":0,"b":1},"fields":[],"columns":[],"count":70_000}}),
        common_rows_digest(70_000),
    );
    rejected(&view, "harness_context_structure_budget_exceeded");
}
#[test]
fn audit_rows_expansion_enforces_output_node_budget() {
    let view = envelope_digest(
        json!({"$oev2_rows":{"common":{"a":0,"b":1},"fields":[],"rows":vec![Vec::<Value>::new();70_000]}}),
        common_rows_digest(70_000),
    );
    rejected(&view, "harness_context_structure_budget_exceeded");
}
#[test]
fn audit_literal_expansion_enforces_depth_budget() {
    let expected = nested(100);
    let view = envelope(json!({"$oev2_literal":expected.clone()}), &expected);
    rejected(&view, "harness_context_structure_budget_exceeded");
}

#[test]
fn shared_node_limit_boundary_preserves_all_values() {
    let expected = Value::Array(vec![Value::Null; MAX_NODES - 1]);
    let view = envelope(expected.clone(), &expected);
    assert_eq!(context::expand(&view).unwrap(), expected);
}
#[test]
fn shared_plain_over_limit_remains_rejected() {
    let expected = Value::Array(vec![Value::Null; MAX_NODES]);
    let view = envelope(expected.clone(), &expected);
    rejected(&view, "harness_context_structure_budget_exceeded");
}
#[test]
fn shared_literal_depth_boundary_is_lossless() {
    let expected = nested(MAX_DEPTH);
    let view = envelope(json!({"$oev2_literal":expected.clone()}), &expected);
    assert_eq!(context::expand(&view).unwrap(), expected);
}
#[test]
fn shared_literal_reference_depth_boundary_is_lossless() {
    let expected = nested(MAX_DEPTH);
    let id = sha256(canonical_json(&expected).unwrap());
    let mut view = envelope(json!({"$oev2_ref":id}), &expected);
    view["dictionary"] = json!({(id):{"$oev2_literal":expected.clone()}});
    assert_eq!(context::expand(&view).unwrap(), expected);
}
#[test]
fn shared_literal_subtree_nodes_are_counted() {
    let expected = Value::Array(vec![Value::Null; MAX_NODES]);
    let view = envelope(json!({"$oev2_literal":expected.clone()}), &expected);
    rejected(&view, "harness_context_structure_budget_exceeded");
}
#[test]
fn shared_reserved_literal_remains_data() {
    let expected = json!({"$oev2_rows":{"nested":{"$oev2_ref":"not-a-codec-reference"},
        "$oev2_literal":{"$oev2_rows":"user-data"}}});
    let view = envelope(json!({"$oev2_literal":expected.clone()}), &expected);
    assert_eq!(context::expand(&view).unwrap(), expected);
}
#[test]
fn shared_rows_and_columns_preserve_common_and_variable_values() {
    let expected = json!([{"status":"unknown","x":0,"note":"a"},
        {"status":"unknown","x":1,"note":"b"}]);
    for table in [
        json!({"common":{"status":"unknown"},"fields":["x","note"],"rows":[[0,"a"],[1,"b"]]}),
        json!({"common":{"status":"unknown"},"fields":["x","note"],"columns":[[0,1],["a","b"]],"count":2}),
    ] {
        let view = envelope(json!({"$oev2_rows":table}), &expected);
        assert_eq!(context::expand(&view).unwrap(), expected);
    }
}
#[test]
fn shared_repeated_field_names_are_charged_before_output_allocation() {
    // Less than 100 KiB of encoded input; its repeated 4 KiB key alone would
    // create more than 64 MiB of output. No expanded expected tree is allocated.
    let view = envelope_digest(
        json!({"$oev2_rows":{"common":{},
        "fields":["k".repeat(4096)],"columns":[vec![Value::Null;17_000]],"count":17_000}}),
        "not-used".into(),
    );
    rejected(&view, "harness_context_expansion_budget_exceeded");
}
#[test]
fn shared_rows_repeated_field_names_are_charged_before_output_allocation() {
    let view = envelope_digest(
        json!({"$oev2_rows":{"common":{},
        "fields":["k".repeat(4096)],"rows":vec![vec![Value::Null];17_000]}}),
        "not-used".into(),
    );
    rejected(&view, "harness_context_expansion_budget_exceeded");
}
#[test]
fn shared_field_escaping_utf8_and_references_are_lossless() {
    let key = "中文\n\"\\\u{0001}";
    let values = json!(["first\nvalue", "第二个值"]);
    let id = sha256(canonical_json(&values).unwrap());
    let expected = json!([{(key):"first\nvalue"},{(key):"第二个值"}]);
    let mut view = envelope(
        json!({"$oev2_rows":{"common":{},"fields":[key],
        "columns":[{"$oev2_ref":id}],"count":2}}),
        &expected,
    );
    view["dictionary"] = json!({(id):values});
    assert_eq!(context::expand(&view).unwrap(), expected);
}
#[test]
fn shared_table_output_depth_counts_the_row_container() {
    for depth in [94, 95] {
        let value = nested(depth);
        let expected = json!([{"x":value.clone()}]);
        for table in [
            json!({"common":{},"fields":["x"],"rows":[[value.clone()]]}),
            json!({"common":{},"fields":["x"],"columns":[[value.clone()]],"count":1}),
        ] {
            let view = envelope(json!({"$oev2_rows":table}), &expected);
            if depth == 94 {
                assert_eq!(context::expand(&view).unwrap(), expected);
            } else {
                rejected(&view, "harness_context_structure_budget_exceeded");
            }
        }
    }
}
#[test]
fn shared_repeated_common_value_is_charged_before_output_allocation() {
    let view = envelope_digest(
        json!({"$oev2_rows":{"common":{"text":"x".repeat(131_072)},
        "fields":[],"columns":[],"count":513}}),
        "not-used".into(),
    );
    rejected(&view, "harness_context_expansion_budget_exceeded");
}
#[test]
fn shared_cyclic_reference_remains_rejected() {
    let mut view = envelope_digest(json!({"$oev2_ref":"loop"}), "not-used".into());
    view["dictionary"] = json!({"loop":{"$oev2_ref":"loop"}});
    rejected(&view, "harness_shared_context_cycle");
}
