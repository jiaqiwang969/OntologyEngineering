use semantica_runtime::reasoner::{MAX_FACTS, MAX_RULES, MAX_TEXT_BYTES, PROFILE, infer_facts};
use serde_json::{Value, json};

#[test]
fn original_reasoner_eight_frozen_oracles() {
    let oracle = semantica_contracts::json::protocol_json(
        include_str!("../../semantica-service/tests/fixtures/original-reasoner-oracle.json"),
        1024 * 1024,
    )
    .unwrap();
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 8);
    for case in oracle["cases"].as_array().unwrap() {
        assert_eq!(
            infer_facts(&case["facts"], &case["rules"]).unwrap(),
            case["expected"],
            "{}",
            case["id"]
        );
    }
}

#[test]
fn original_reasoner_sorted_facts_same_pass_and_insertion_order_replacement() {
    assert_eq!(
        infer_facts(&json!(["A(Z)", "A(A)"]), &json!(["IF A(?x) THEN B(?x)"])).unwrap(),
        json!(["B(A)", "B(Z)"])
    );
    assert_eq!(
        infer_facts(
            &json!(["A(a)"]),
            &json!([
                "IF B(?x) THEN D(?x)",
                "IF A(?x) THEN B(?x)",
                "IF B(?x) THEN C(?x)",
            ])
        )
        .unwrap(),
        json!(["B(a)", "C(a)", "D(a)"])
    );
    assert_eq!(
        infer_facts(
            &json!(["One(A)", "Pair(A,B)"]),
            &json!(["IF One(?xy) AND Pair(?xy,?x) THEN Result(?x,?xy)"])
        )
        .unwrap(),
        json!(["Result(B,A)"])
    );
    // Original sequential replace also processes placeholders introduced by
    // earlier binding values. Neither result uses token-safe substitution.
    assert_eq!(
        infer_facts(
            &json!(["Pair(?y,B)"]),
            &json!(["IF Pair(?x,?y) THEN Result(?x,?y)"])
        )
        .unwrap(),
        json!(["Result(B,B)"])
    );
}

#[test]
fn original_reasoner_literal_escape_lazy_captures_and_dictionary_coercion() {
    assert_eq!(
        infer_facts(
            &json!(["C++(A[1].*)"]),
            &json!(["IF C++(?x) THEN Name(?x)"])
        )
        .unwrap(),
        json!(["Name(A[1].*)"])
    );
    assert_eq!(
        infer_facts(
            &json!(["Pair(A,B,A,B)", "Pair(A,B,A,C)"]),
            &json!(["IF Pair(?x,?x) THEN Equal(?x)"])
        )
        .unwrap(),
        json!(["Equal(A,B)"])
    );
    assert_eq!(
        infer_facts(
            &json!({"type":"P","id":"x"}),
            &json!(["IF P(?x) THEN Seen(?x)"])
        )
        .unwrap(),
        json!(["Seen(x)"])
    );
    assert_eq!(
        infer_facts(
            &json!([
                {"type":"Person","name":" John ","id":"ignored"},
                {"type":"Person","name":null,"id":"ignored"},
                {"type":null,"name":true},
                {"type":"Link","source_id":"s","target_id":"t"},
                {"source_name":null,"source_id":"ignored","target_name":null,"target_id":"ignored"},
            ]),
            &json!([
                "IF Person(?x) THEN Seen(?x)",
                "IF None(?x) THEN Nulltype(?x)",
                "IF Link(?x, ?y) THEN Rel(?x,?y)",
                "IF Relationship(?x, ?y) THEN Missing(?x,?y)",
            ])
        )
        .unwrap(),
        json!([
            "Seen( John )",
            "Seen(None)",
            "Nulltype(True)",
            "Rel(s,t)",
            "Missing(None,None)"
        ])
    );
    assert_eq!(
        infer_facts(
            &json!({"type":"Thing","name":["a",false,null,{"x":1}]}),
            &json!(["IF Thing(?x) THEN Value(?x)"])
        )
        .unwrap(),
        json!(["Value(['a', False, None, {'x': 1}])"])
    );
    assert_eq!(
        infer_facts(
            &json!([{ "type":"R","name":"entity","source_name":"actor","target_name":"target"}]),
            &json!([
                "IF R(?x) THEN Entity(?x)",
                "IF R(?x, ?y) THEN Relationship(?x,?y)"
            ])
        )
        .unwrap(),
        json!(["Entity(entity)"])
    );
}

#[test]
fn original_reasoner_python_whitespace_invalid_groups_and_nonempty_dot() {
    assert_eq!(
        infer_facts(
            &json!([{"ignored":1},7,null,true,"\u{001c}A(α)\u{001f}"]),
            &json!(["IF\u{001c}A(?x)\u{001f}THEN Unicode(?x)"])
        )
        .unwrap(),
        json!(["Unicode(α)"])
    );
    assert_eq!(
        infer_facts(&json!(["A(x)"]), &json!(["İF A(?x) THEN B(?x)"])).unwrap(),
        json!(["B(x)"])
    );
    for (fact, rule) in [
        ("P()", "IF P(?x) THEN Empty(?x)"),
        ("P(x\ny)", "IF P(?x) THEN Multiline(?x)"),
        ("P(x)", "IF P(?1) THEN Invalid(?1)"),
        ("?literal", "IF ? literal THEN Invalid"),
        ("??x", "IF ??x THEN Invalid"),
    ] {
        assert_eq!(
            infer_facts(&json!([fact]), &json!([rule])).unwrap(),
            json!([]),
            "{rule}"
        );
    }
    assert_eq!(
        infer_facts(
            &json!(["A?", "Literal(?😀)"]),
            &json!(["IF A? THEN B", "IF Literal(?😀) THEN Emoji"])
        )
        .unwrap(),
        json!(["B", "Emoji"])
    );
}

#[test]
fn original_reasoner_default_fifty_passes_do_not_claim_convergence() {
    let actual = infer_facts(&json!(["A(a)"]), &json!(["IF A(?x) THEN A(z?x)"])).unwrap();
    let expected = Value::Array(
        (1..=50)
            .map(|length| Value::String(format!("A({}a)", "z".repeat(length))))
            .collect(),
    );
    assert_eq!(actual, expected);
}

#[test]
fn reasoner_resource_and_uncovered_profiles_fail_explicitly() {
    assert!(PROFILE.contains("default-50-bounded"));
    let oversized = json!(vec!["A"; MAX_FACTS + 1]);
    assert!(
        infer_facts(&oversized, &json!([]))
            .unwrap_err()
            .to_string()
            .contains("reasoner_fact_limit_exceeded")
    );
    assert!(
        infer_facts(&json!([]), &json!(vec!["not a rule"; MAX_RULES + 1]))
            .unwrap_err()
            .to_string()
            .contains("reasoner_rule_limit_exceeded")
    );
    assert!(
        infer_facts(&json!(["A".repeat(MAX_TEXT_BYTES + 1)]), &json!([]))
            .unwrap_err()
            .to_string()
            .contains("reasoner_text_limit_exceeded")
    );
    assert!(
        infer_facts(&json!(["A(a)"]), &json!(["IF A(?变量) THEN B(?变量)"]))
            .unwrap_err()
            .to_string()
            .contains("reasoner_unicode_variable_uncovered")
    );
    assert!(
        infer_facts(
            &json!(["A"]),
            &json!([{"conditions":["A"],"conclusion":"B"}])
        )
        .unwrap_err()
        .to_string()
        .contains("reasoner_rule_kind_uncovered")
    );
    assert_eq!(infer_facts(&json!(["A"]), &Value::Null).unwrap(), json!([]));
}
