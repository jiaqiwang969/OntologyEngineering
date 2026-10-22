//! Author regressions for explicit locator transport, not model comprehension.
use oe_core::json::{ordered_json, strict_json};
use oe_engineering::harness::context;
use serde_json::json;

#[test]
fn indexed_transport_preserves_source_fields_markers_and_exact_normalized_values() {
    let source = json!({"purpose":"承托 / 中文","unknowns":["material",null],"unit":"mm",
        "source_sha256":"a".repeat(64),"source_looking_reference":{"$oev2_ref":"0"},
        "nested_literal":{"$oev2_literal":{"$oev2_rows":{"$oev2_ref":"original fact"}}},
        "conditions":"unchanged condition ".repeat(24),"status":"failed","load":1.25});
    let full = json!({"project_state":{"conditions":[serde_json::to_string(&source).unwrap()]},
        "previous":{"state":{"conditions":[serde_json::to_string(&source).unwrap()]},"adoption_inherited":false},
        "observations":[source.clone(),source.clone(),source.clone(),source.clone()]});
    let old = context::policy(Some(&json!({"schema":"oev2.harness-context-policy/2",
        "mode":"lossless_shared_text","byte_budget":256000})))
    .unwrap();
    let new = context::policy(Some(&json!({"schema":"oev2.harness-context-policy/3",
        "mode":"lossless_indexed_text","byte_budget":256000})))
    .unwrap();
    let (before, original_record) = context::project(&full, &old).unwrap();
    let (view, record) = context::project(&full, &new).unwrap();
    assert_eq!(
        context::expand(&before).unwrap(),
        context::expand(&view).unwrap()
    );
    assert_eq!(context::expand(&view).unwrap()["observations"][0], source);
    assert_eq!(
        context::project(&full, &new).unwrap(),
        (view.clone(), record.clone())
    );
    assert_eq!(
        context::project_for_archive(&full, &old, &original_record).unwrap(),
        (before, original_record)
    );
    assert_eq!(
        context::project_for_archive(&full, &new, &record).unwrap(),
        (view.clone(), record)
    );
    let wire = strict_json(view.as_str().unwrap().as_bytes()).unwrap();
    assert_eq!(wire["schema"], "oev2.harness-shared-context/2");
    assert!(
        ordered_json(&wire)
            .unwrap()
            .windows(64)
            .any(|w| w == "a".repeat(64).as_bytes())
    );
    let mut changed = wire.clone();
    changed["normalized_sha256"] = json!("0".repeat(64));
    assert!(
        context::expand(&changed)
            .unwrap_err()
            .to_string()
            .contains("expansion_mismatch")
    );
    let mut extra = wire.clone();
    let n = extra["dictionary"].as_object().unwrap().len();
    extra["dictionary"][n.to_string()] = json!({"extra":"unreferenced source"});
    assert!(
        context::expand(&extra)
            .unwrap_err()
            .to_string()
            .contains("dictionary_unused")
    );
}

#[test]
fn indexed_policy_does_not_change_legacy_policy_or_allow_implicit_modes() {
    for (schema, mode) in [
        ("oev2.harness-context-policy/1", "lossless_indexed_text"),
        ("oev2.harness-context-policy/2", "lossless_indexed_text"),
        ("oev2.harness-context-policy/3", "lossless_shared_text"),
        ("oev2.harness-context-policy/3", "full"),
    ] {
        assert!(
            context::policy(Some(
                &json!({"schema":schema,"mode":mode,"byte_budget":96000})
            ))
            .is_err()
        );
    }
    for budget in [28000, 256000] {
        assert!(
            context::policy(Some(&json!({"schema":"oev2.harness-context-policy/3",
            "mode":"lossless_indexed_text","byte_budget":budget})))
            .is_ok()
        );
    }
    for budget in [27999, 256001] {
        assert!(
            context::policy(Some(&json!({"schema":"oev2.harness-context-policy/3",
            "mode":"lossless_indexed_text","byte_budget":budget})))
            .is_err()
        );
    }
}

fn indexed_view(
    root: serde_json::Value,
    dictionary: serde_json::Value,
    expected: &serde_json::Value,
) -> serde_json::Value {
    json!({"schema":context::INDEXED_VIEW_SCHEMA,"coverage":"all_normalized_semantic_values",
        "root":root,"dictionary":dictionary,"normalized_sha256":oe_core::sha256(oe_core::json::canonical_json(expected).unwrap())})
}

#[test]
fn indexed_output_node_budget_excludes_reference_traversal() {
    let item = json!(vec![serde_json::Value::Null; 99_997]);
    let expected =
        json!({"changes":null,"payload":[item.clone(),item.clone()],"project_state":null});
    let root = json!({"changes":null,"payload":[{"$oev2_ref":"0"},{"$oev2_ref":"0"}],"project_state":null});
    let view = indexed_view(root.clone(), json!({"0":item}), &expected);
    assert_eq!(context::expand(&view).unwrap(), expected);
    let old_id = oe_core::sha256(oe_core::json::canonical_json(&view["dictionary"]["0"]).unwrap());
    let mut legacy = view.clone();
    legacy["schema"] = json!(context::VIEW_SCHEMA);
    legacy["dictionary"] = json!({(&old_id):view["dictionary"]["0"]});
    legacy["root"]["payload"] = json!([{"$oev2_ref":old_id},{"$oev2_ref":old_id}]);
    assert_eq!(
        context::expand(&legacy).unwrap_err().to_string(),
        "harness_context_structure_budget_exceeded",
        "legacy conservative traversal behavior retains its original scope"
    );
    let mut over_expected = expected;
    over_expected["payload"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::Value::Null);
    let mut over_root = root;
    over_root["payload"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::Value::Null);
    let over = indexed_view(over_root, view["dictionary"].clone(), &over_expected);
    assert_eq!(
        context::expand(&over).unwrap_err().to_string(),
        "harness_context_structure_budget_exceeded"
    );
}

#[test]
fn indexed_output_byte_budget_excludes_reference_and_literal_work() {
    // Final UTF-8 JSON is exactly 2*n+12 bytes, including array/null framing.
    let n = 33_554_426;
    let text = "x".repeat(n);
    let expected = json!([text.clone(), text.clone(), null]);
    assert_eq!(ordered_json(&expected).unwrap().len(), 67_108_864);
    for node in [json!(text.clone()), json!({"$oev2_literal":text.clone()})] {
        let view = indexed_view(
            json!([{"$oev2_ref":"0"},{"$oev2_ref":"0"},null]),
            json!({"0":node}),
            &expected,
        );
        assert_eq!(context::expand(&view).unwrap(), expected);
    }
    // A common subtree is decoded once and copied into two rows; intermediate
    // copies are work, not three occurrences in the final JSON byte budget.
    let empty_rows = json!([[{"x":""},{"x":""}],null]);
    let remaining = 67_108_864 - ordered_json(&empty_rows).unwrap().len();
    assert_eq!(remaining % 2, 0);
    let common_text = "r".repeat(remaining / 2);
    let rows_expected = json!([[{"x":common_text.clone()},{"x":common_text.clone()}],null]);
    assert_eq!(ordered_json(&rows_expected).unwrap().len(), 67_108_864);
    let rows = json!([{"$oev2_rows":{"common":{"x":common_text},"fields":[],"columns":[],"count":2}},null]);
    assert_eq!(
        context::expand(&indexed_view(rows, json!({}), &rows_expected)).unwrap(),
        rows_expected
    );
    // Keep encoded bytes far below the cap while final false is one byte
    // longer than null: this isolates the expanded-byte rejection.
    let expanded_over = json!([text.clone(), text.clone(), false]);
    assert_eq!(ordered_json(&expanded_over).unwrap().len(), 67_108_865);
    let expanded_over_view = indexed_view(
        json!([{"$oev2_ref":"0"},{"$oev2_ref":"0"},false]),
        json!({"0":text.clone()}),
        &expanded_over,
    );
    assert_eq!(
        context::expand(&expanded_over_view)
            .unwrap_err()
            .to_string(),
        "harness_context_expansion_budget_exceeded"
    );
    let over_expected = json!([text.clone(), format!("{text}x"), null]);
    let over = indexed_view(
        json!([{"$oev2_ref":"0"},{"$oev2_ref":"1"},null]),
        json!({"0":text,"1":format!("{}x", "x".repeat(n))}),
        &over_expected,
    );
    assert!(context::expand(&over).is_err());
}

#[test]
fn indexed_reference_depth_and_aliases_measure_expanded_tree() {
    let mut expected = serde_json::Value::Null;
    let mut dictionary = serde_json::Map::new();
    for i in (0..96).rev() {
        dictionary.insert(
            i.to_string(),
            if i == 95 {
                json!([null])
            } else {
                json!([{"$oev2_ref":(i+1).to_string()}])
            },
        );
        expected = json!([expected]);
    }
    let view = indexed_view(json!({"$oev2_ref":"0"}), dictionary.into(), &expected);
    assert_eq!(context::expand(&view).unwrap(), expected);
    let mut aliases = serde_json::Map::new();
    for i in 0..128 {
        aliases.insert(
            i.to_string(),
            if i == 127 {
                json!("unknown")
            } else {
                json!({"$oev2_ref":(i+1).to_string()})
            },
        );
    }
    let alias_view = indexed_view(
        json!({"$oev2_ref":"0"}),
        aliases.clone().into(),
        &json!("unknown"),
    );
    assert_eq!(context::expand(&alias_view).unwrap(), json!("unknown"));
    aliases.insert("127".into(), json!({"$oev2_ref":"0"}));
    assert!(
        context::expand(&indexed_view(
            json!({"$oev2_ref":"0"}),
            aliases.into(),
            &json!("unknown")
        ))
        .unwrap_err()
        .to_string()
        .contains("cycle")
    );
}
