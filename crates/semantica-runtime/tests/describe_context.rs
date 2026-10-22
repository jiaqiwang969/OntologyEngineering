//! Source-frozen original RDFLib output, not evaluator-produced expectations.
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{NamedNode, Term};
use semantica_contracts::{SemanticQueryOptionsPortV2, SemanticQueryOptionsRequestV2};
use semantica_runtime::graph;
use semantica_runtime::runtime::RustSemanticRuntime;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/describe-context-original17.json")).unwrap()
}
fn execute_neutral(case: &Value) -> anyhow::Result<Value> {
    let store = graph::load(case["data"].as_str().unwrap().as_bytes()).unwrap();
    let bindings = case["bindings"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| {
            let line = format!("<urn:binding> <urn:value> {} .", value.as_str().unwrap());
            let term = RdfParser::from_format(RdfFormat::NTriples)
                .for_reader(line.as_bytes())
                .next()
                .unwrap()
                .unwrap()
                .object;
            (name.clone(), term)
        })
        .collect();
    graph::query_with_options(
        &store,
        case["query"].as_str().unwrap(),
        &BTreeMap::from([("ex".into(), "urn:".into())]),
        &bindings,
    )
}
fn options_input(case: &Value) -> SemanticQueryOptionsRequestV2 {
    SemanticQueryOptionsRequestV2 {
        schema: "semantica.query-options-request/2".into(),
        profile: "rdflib760-describe-context/2".into(),
        data: case["data"].as_str().unwrap().into(),
        data_sha256: semantica_runtime::sha256(case["data"].as_str().unwrap().as_bytes()),
        rdf_format: "turtle".into(),
        query: case["query"].as_str().unwrap().into(),
        namespaces: BTreeMap::from([("ex".into(), "urn:".into())]),
        bindings: serde_json::from_value(case["bindings"].clone()).unwrap(),
    }
}
fn execute(case: &Value) -> anyhow::Result<Value> {
    RustSemanticRuntime
        .query_with_options_v2(&options_input(case))
        .map(|response| response.query_result.result)
        .map_err(|error| anyhow::anyhow!("{}:{}:{}", error.phase, error.code, error.message))
}
fn assert_original(id: &str) {
    let original = oracle();
    let case = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    assert_observation(id, case);
}
fn assert_observation(id: &str, case: &Value) {
    assert_eq!(
        case["kind"], "DESCRIBE",
        "An original error cannot be relabelled as success"
    );
    let result = execute(case).unwrap();
    let mut actual: Vec<_> = result["triples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| format!("{} .", x.as_str().unwrap()))
        .collect();
    if case["blank_subject_label_projection"] == true {
        let mut labels = BTreeSet::new();
        for triple in &actual {
            for part in triple.split_whitespace() {
                if part.starts_with("_:") {
                    labels.insert(part.to_owned());
                }
            }
        }
        assert_eq!(
            labels.len(),
            1,
            "Independent oracle domain is one persistent blank subject; do not collapse several labels"
        );
        let label = labels.into_iter().next().unwrap();
        actual = actual
            .into_iter()
            .map(|s| s.replace(&label, "_:target"))
            .collect();
    }
    actual.sort();
    let expected: Vec<_> = case["triples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        actual, expected,
        "original describe target/context case {id}"
    );
}

#[test]
fn original_hidden_parameter_keeps_only_explicit_part_targets() {
    assert_original("original15_hidden_constant");
}
#[test]
fn original_constant_context_and_zero_where_preserve_target_domain() {
    for id in [
        "constant_unused_context",
        "constant_filter_context",
        "constant_no_where",
    ] {
        assert_original(id);
    }
}
#[test]
fn original_variable_targets_and_application_values_are_both_readable() {
    for id in [
        "variable_target_initial",
        "variable_target_other_is_legitimate",
        "variable_target_from_hidden_context",
        "literal_context_is_read",
        "multiple_targets_include_explicit_other",
    ] {
        assert_original(id);
    }
}
#[test]
fn original_nested_and_minus_domains_do_not_become_describe_targets() {
    for id in [
        "nested_projection_context",
        "minus_explicit_disjoint_domains",
        "variable_no_solution",
        "single_blank_target_preserved",
    ] {
        assert_original(id);
    }
}
#[test]
fn original_modifier_observation_is_kept_for_precise_current_comparison() {
    assert_original("modifiers_keep_targets");
}

#[test]
fn original_describe_distinct_target_projection_precedes_slice() {
    let original: Value =
        serde_json::from_str(include_str!("fixtures/describe-modifier-original9.json")).unwrap();
    assert_eq!(original["cases"].as_array().unwrap().len(), 9);
    for case in original["cases"].as_array().unwrap() {
        // This recorded observation has tied ORDER BY values. Its unchanged
        // exact comparison remains in describe_original_comparison, while a
        // separate pre-frozen total-order control supplies a unique assertion.
        if case["id"] == "original_modifier_no_initial" {
            continue;
        }
        assert_observation(case["id"].as_str().unwrap(), case);
    }
}

#[test]
fn options2_original_modifier_and_nested_keys_preserve_selected_targets() {
    let controls: Value = serde_json::from_str(include_str!(
        "fixtures/describe-sorted-target-original2.json"
    ))
    .unwrap();
    assert_observation("total_order_without_initial", &controls["cases"][0]);
    let child = &controls["cases"][1];
    assert_eq!(
        child["triples"].as_array().unwrap().len(),
        7,
        "Original child-IRI target spill is historical output, not explicit target parity"
    );
    let current = execute(child).unwrap();
    let original = oracle();
    let part = &original["cases"][0]["triples"];
    let triples: Vec<_> = current["triples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|triple| format!("{} .", triple.as_str().unwrap()))
        .collect();
    assert_eq!(
        serde_json::to_value(triples).unwrap(),
        *part,
        "A modifier-key IRI must not become an unselected Describe target"
    );
    let nine: Value =
        serde_json::from_str(include_str!("fixtures/describe-modifier-original9.json")).unwrap();
    let modifier = &nine["cases"][0];
    let neutral = execute_neutral(modifier).unwrap();
    assert_eq!(
        neutral["triples"].as_array().unwrap().len(),
        4,
        "Neutral default preserves pre-increment modifier behavior"
    );
    assert_observation("original_modifier_visible_only", modifier);
    assert_observation(
        "nested_context_read_before_distinct_slice",
        &nine["cases"][8],
    );
    let tied = &nine["cases"][2];
    let current = execute(tied).unwrap();
    let current: Vec<_> = current["triples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|triple| format!("{} .", triple.as_str().unwrap()))
        .collect();
    let current = serde_json::to_value(current).unwrap();
    let other = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "variable_target_other_is_legitimate")
        .unwrap();
    assert!(
        current == *part || current == other["triples"],
        "Tied solutions permit either original resource domain, no unique reference-parity claim"
    );
}

#[test]
fn options2_rejects_identity_profile_terms_and_original_query_boundaries() {
    let original = oracle();
    let input = options_input(&original["cases"][0]);
    let response = RustSemanticRuntime.query_with_options_v2(&input).unwrap();
    assert_eq!(response.profile, "rdflib760-describe-context/2");
    assert_eq!(
        response.request_sha256,
        semantica_runtime::sha256(
            semantica_json::canonical_json(&serde_json::to_value(&input).unwrap()).unwrap()
        )
    );
    for (field, value) in [
        ("schema", "semantica.query-request/1"),
        ("profile", "neutral/1"),
        ("data_sha256", &"0".repeat(64)),
    ] {
        let mut raw = serde_json::to_value(&input).unwrap();
        raw[field] = Value::String(value.into());
        let mutation: SemanticQueryOptionsRequestV2 = serde_json::from_value(raw).unwrap();
        assert!(
            RustSemanticRuntime
                .query_with_options_v2(&mutation)
                .is_err()
        );
    }
    for term in [
        "<urn:other",
        "\"bad\" .\n<urn:extra> <urn:value> <urn:part>",
    ] {
        let mut mutation = input.clone();
        mutation.bindings.insert("s".into(), term.into());
        assert_eq!(
            RustSemanticRuntime
                .query_with_options_v2(&mutation)
                .unwrap_err()
                .code,
            "binding_parse_failed"
        );
    }
    let mut mutation = input.clone();
    mutation.bindings = (0..129)
        .map(|index| (format!("b{index}"), "<urn:part>".into()))
        .collect();
    assert_eq!(
        RustSemanticRuntime
            .query_with_options_v2(&mutation)
            .unwrap_err()
            .code,
        "input_budget_exceeded"
    );
    for (query, bindings) in [
        (
            "SELECT * WHERE { SERVICE <https://example.invalid/> { ?s ?p ?o } }",
            BTreeMap::new(),
        ),
        ("SELECT * FROM <urn:g> WHERE { ?s ?p ?o }", BTreeMap::new()),
        (
            "SELECT ?name WHERE { BIND(\"new\" AS ?name) }",
            BTreeMap::from([("name".into(), "\"old\"".into())]),
        ),
        (
            "SELECT (COUNT(?s) AS ?count) WHERE { ?s ex:name ?name }",
            BTreeMap::from([(
                "count".into(),
                "\"2\"^^<http://www.w3.org/2001/XMLSchema#integer>".into(),
            )]),
        ),
    ] {
        let mut mutation = input.clone();
        mutation.query = query.into();
        mutation.bindings = bindings;
        assert_eq!(
            RustSemanticRuntime
                .query_with_options_v2(&mutation)
                .unwrap_err()
                .code,
            "query_execution_failed"
        );
    }
}

#[test]
fn original_wildcard_capture_errors_are_not_rust_success_oracles() {
    let original = oracle();
    let errors: Vec<_> = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "ORIGINAL_ERROR")
        .collect();
    assert_eq!(errors.len(), 3);
    assert!(errors.iter().all(|c| c["error_type"] == "TypeError"));
    // Independent W3C16.4 finite target-domain controls. Existing wildcard
    // capability remains; unused application values are absent from its scope.
    for (query, count) in [
        ("DESCRIBE * WHERE { ?s ex:name ?name }", 4),
        (
            "DESCRIBE * WHERE { ?s ex:name ?name FILTER(?gate=ex:part) }",
            4,
        ),
        ("DESCRIBE * WHERE { FILTER(false) }", 0),
    ] {
        let data = graph::load(original["cases"][0]["data"].as_str().unwrap().as_bytes()).unwrap();
        let initial: BTreeMap<String, Term> = BTreeMap::from([
            ("s".into(), NamedNode::new("urn:part").unwrap().into()),
            ("gate".into(), NamedNode::new("urn:part").unwrap().into()),
            ("unused".into(), NamedNode::new("urn:other").unwrap().into()),
        ]);
        let result = graph::query_with_options(
            &data,
            query,
            &BTreeMap::from([("ex".into(), "urn:".into())]),
            &initial,
        )
        .unwrap();
        assert_eq!(result["triples"].as_array().unwrap().len(), count);
        assert!(
            result["triples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| !t.as_str().unwrap().contains("urn:other"))
        );
    }
}
