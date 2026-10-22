//! Characterisation tests for `bridge_axiom.rs` against the oracle (`fixtures/oracle.json`,
//! section `bridge`).

mod common;

use common::{CountingClock, assert_json_eq, err_json, mask_chain_id, obj, oracle};
use sem_provenance::ProvenanceManager;
use sem_provenance::bridge_axiom::{
    BridgeAxiom, TranslationChain, create_translation_chain, trace_translation_chain,
};
use serde_json::{Map, Value, json};

fn section(key: &str) -> &'static Value {
    &oracle("bridge")[key]
}

fn finance_axiom() -> BridgeAxiom {
    BridgeAxiom {
        input_domain: "ecological".into(),
        output_domain: "financial".into(),
        confidence: 0.92,
        ..BridgeAxiom::new(
            "BA-FINANCE-001",
            "biomass_tourism_elasticity",
            "1% biomass increase \u{2192} 0.346% tourism revenue increase",
            0.346,
            "10.1038/s41586-021-03371-z",
            "Table S4",
        )
    }
}

fn layer_axioms() -> Vec<BridgeAxiom> {
    vec![
        BridgeAxiom {
            confidence: 0.9,
            ..BridgeAxiom::new("BA-L1", "layer_1", "Test", 2.0, "10.1234/l1", "P1")
        },
        BridgeAxiom {
            confidence: 0.8,
            ..BridgeAxiom::new("BA-L2", "layer_2", "Test", 0.5, "10.1234/l2", "P2")
        },
    ]
}

#[test]
fn apply_matches_oracle() {
    let clock = CountingClock::new();
    let ba = finance_axiom();
    assert_json_eq(&Value::Object(ba.to_dict()), section("to_dict"));
    let values = [
        ("int", json!(463)),
        ("float", json!(38.5)),
        ("bool", json!(true)),
        ("zero", json!(0)),
        ("neg", json!(-2.5)),
        ("big", json!(1_000_000_000_000_000_000i64)),
    ];
    for (name, value) in values {
        let result = ba
            .apply("cabo_pulmo_biomass", value, None, &Map::new())
            .unwrap();
        assert_json_eq(&Value::Object(result), &section("apply")[name]);
    }
    for (name, value) in [
        ("none", Value::Null),
        ("str", json!("abc")),
        ("list", json!([1])),
        ("dict", json!({"a": 1})),
    ] {
        let err = ba.apply("x", value, None, &Map::new()).unwrap_err();
        assert_eq!(err_json(&err), section("apply_errors")[name], "{name}");
    }
    let none_entity = ba.apply(Value::Null, 2, None, &Map::new()).unwrap();
    assert_json_eq(&Value::Object(none_entity), section("apply_none_entity"));

    let m = ProvenanceManager::new().unwrap();
    let ba2 = BridgeAxiom {
        source_quote: Some("Test quote".into()),
        metadata: obj(json!({"ignored": true})),
        ..BridgeAxiom::new("BA-TEST-005", "prov", "r", 1.5, "10.1234/test", "Page 1")
    };
    let tracked = ba2
        .apply(
            "input_entity",
            100,
            Some(&m),
            &obj(json!({"unit": "pct", "axiom_id": "kw_override"})),
        )
        .unwrap();
    assert_json_eq(&Value::Object(tracked), section("apply_tracked"));
    let entry = m
        .get_provenance("input_entity_transformed_BA-TEST-005")
        .unwrap()
        .unwrap();
    assert_json_eq(&Value::Object(entry), section("tracked_entry"));
    assert_eq!(json!(clock.calls()), *section("tracked_clock_calls"));
}

#[test]
fn translation_chain_matches_oracle() {
    let clock = CountingClock::new();
    let m = ProvenanceManager::new().unwrap();
    let input = obj(json!({"entity_id": "lineage_test", "value": 75, "source": "test_doc"}));
    let chain = create_translation_chain(&input, &layer_axioms(), Some(&m)).unwrap();
    assert_json_eq(
        &mask_chain_id(Value::Object(chain.to_dict())),
        &mask_chain_id(section("chain").clone()),
    );
    let trace = trace_translation_chain(&chain, &m);
    let mut expected_trace = mask_chain_id(section("trace").clone());
    let mut got_trace = mask_chain_id(Value::Object(trace));
    // `source_documents` inside each lineage comes from a Python set.
    for trace in [&mut expected_trace, &mut got_trace] {
        for layer in trace["layers"].as_array_mut().unwrap() {
            if let Some(Value::Array(docs)) = layer
                .get_mut("provenance")
                .and_then(|p| p.get_mut("source_documents"))
            {
                docs.sort_by_key(|d| d.to_string());
            }
        }
    }
    assert_json_eq(&got_trace, &expected_trace);
    assert_eq!(json!(clock.calls()), *section("chain_clock_calls"));
    drop(clock);

    let _clock = CountingClock::new();
    let axioms = layer_axioms();
    let no_entity =
        create_translation_chain(&obj(json!({"value": 3})), &axioms[..1], None).unwrap();
    assert_json_eq(
        &mask_chain_id(Value::Object(no_entity.to_dict())),
        &mask_chain_id(section("chain_no_entity").clone()),
    );
    let no_axioms =
        create_translation_chain(&obj(json!({"entity_id": "e", "value": 1.5})), &[], None).unwrap();
    assert_json_eq(
        &mask_chain_id(Value::Object(no_axioms.to_dict())),
        &mask_chain_id(section("chain_no_axioms").clone()),
    );
    let err = create_translation_chain(&obj(json!({"entity_id": "x"})), &axioms, None).unwrap_err();
    assert_eq!(err_json(&err), *section("chain_error"));
}

#[test]
fn manual_chain_matches_oracle() {
    let _clock = CountingClock::new();
    let mut tc = TranslationChain::new("chain_002");
    tc.add_layer("L1", "input", 100, Some("doc_1"), Map::new());
    tc.add_layer(
        "L2",
        "bridge_axiom",
        0.5,
        Some("axiom_1"),
        obj(json!({"timestamp": "override", "extra": [1]})),
    );
    tc.add_layer("L3", "output", 50, Value::Null, Map::new());
    assert_json_eq(&Value::Object(tc.to_dict()), section("manual_chain"));
    assert!(tc.get_layer("L9").is_none());
    assert!(section("manual_get_missing").is_null());
    let trace = trace_translation_chain(&tc, &ProvenanceManager::new().unwrap());
    assert_json_eq(&Value::Object(trace), section("manual_trace"));

    assert_eq!(
        format!("{tc:?}"),
        section("repr_manual_chain").as_str().unwrap()
    );
    assert_eq!(
        format!("{:?}", BridgeAxiom::new("A", "n", "r", 0.5, "d", "p")),
        section("repr_axiom").as_str().unwrap()
    );
    assert_eq!(
        format!("{:?}", TranslationChain::new("c")),
        section("repr_chain").as_str().unwrap()
    );

    // `get_layer` returns the live dict in Python; `get_layer_mut` is the mutable access.
    tc.get_layer_mut("L1")
        .unwrap()
        .insert("source".into(), json!("edited"));
    assert_eq!(tc.get_layer("L1").unwrap()["source"], "edited");
}
