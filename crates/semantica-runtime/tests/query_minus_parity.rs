use oxigraph::io::RdfFormat;
use oxigraph::model::{BlankNode, Literal, NamedNode, Term};
use semantica_runtime::graph;
use serde_json::Value;
use std::collections::BTreeMap;

fn binding(value: &Value) -> Term {
    match value["kind"].as_str().unwrap() {
        "iri" => NamedNode::new(value["value"].as_str().unwrap())
            .unwrap()
            .into(),
        "blank" => BlankNode::new(value["value"].as_str().unwrap())
            .unwrap()
            .into(),
        "literal" => {
            let text = value["value"].as_str().unwrap();
            if let Some(language) = value["language"].as_str() {
                Literal::new_language_tagged_literal(text, language)
                    .unwrap()
                    .into()
            } else {
                Literal::new_typed_literal(
                    text,
                    NamedNode::new(value["datatype"].as_str().unwrap()).unwrap(),
                )
                .into()
            }
        }
        kind => panic!("unknown oracle term {kind}"),
    }
}
fn assert_oracle(text: &str, id: &str) {
    let oracle: Value = serde_json::from_str(text).unwrap();
    let case = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    let store = graph::load_format(
        case.get("data")
            .unwrap_or(&oracle["data"])
            .as_str()
            .unwrap()
            .as_bytes(),
        RdfFormat::Turtle,
    )
    .unwrap();
    let namespaces = oracle["namespaces"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
        .collect::<BTreeMap<_, _>>();
    let bindings = case["bindings"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), binding(v)))
        .collect();
    let result = graph::query_with_options(
        &store,
        case["query"].as_str().unwrap(),
        &namespaces,
        &bindings,
    )
    .unwrap_or_else(|error| panic!("original RDFLib oracle {id}: {error}"));
    assert_eq!(result["columns"], case["columns"], "projection: {id}");
    let mut actual = result["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>();
    let mut expected = case["expected_rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected, "original RDFLib row multiset: {id}");
}

#[test]
fn original_disjoint_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "original_disjoint_project",
    );
}

#[test]
fn shared_initial_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "shared_initial_project",
    );
}

#[test]
fn shared_dynamic_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "shared_dynamic_project",
    );
}

#[test]
fn conflicting_dynamic_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "conflicting_dynamic_project",
    );
}

#[test]
fn nested_disjoint_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "nested_disjoint_project",
    );
}

#[test]
fn nested_shared_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "nested_shared_project",
    );
}

#[test]
fn left_project_hides_initial() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "left_project_hides_initial",
    );
}

#[test]
fn both_project_disjoint() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "both_project_disjoint",
    );
}

#[test]
fn empty_right() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "empty_right",
    );
}

#[test]
fn right_optional_unbound() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "right_optional_unbound",
    );
}

#[test]
fn right_union_domain() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "right_union_domain",
    );
}

#[test]
fn right_plain_bgp_initial_domain() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "right_plain_bgp_initial_domain",
    );
}

#[test]
fn right_project_filter_initial() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "right_project_filter_initial",
    );
}

#[test]
fn post_project_filter_initial() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "post_project_filter_initial",
    );
}

#[test]
fn nested_inner_minus() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "nested_inner_minus",
    );
}

#[test]
fn project_then_bgp_restores_initial() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-oracle.json"),
        "project_then_bgp_restores_initial",
    );
}

#[test]
fn default_original_disjoint_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_original_disjoint_project",
    );
}

#[test]
fn default_shared_initial_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_shared_initial_project",
    );
}

#[test]
fn default_shared_dynamic_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_shared_dynamic_project",
    );
}

#[test]
fn default_conflicting_dynamic_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_conflicting_dynamic_project",
    );
}

#[test]
fn default_nested_disjoint_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_nested_disjoint_project",
    );
}

#[test]
fn default_nested_shared_project() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_nested_shared_project",
    );
}

#[test]
fn default_left_project_hides_initial() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_left_project_hides_initial",
    );
}

#[test]
fn default_both_project_disjoint() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_both_project_disjoint",
    );
}

#[test]
fn default_empty_right() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_empty_right",
    );
}

#[test]
fn default_right_optional_unbound() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_right_optional_unbound",
    );
}

#[test]
fn default_right_union_domain() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_right_union_domain",
    );
}

#[test]
fn default_right_plain_bgp_initial_domain() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_right_plain_bgp_initial_domain",
    );
}

#[test]
fn default_right_project_filter_initial() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_right_project_filter_initial",
    );
}

#[test]
fn default_post_project_filter_initial() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_post_project_filter_initial",
    );
}

#[test]
fn default_nested_inner_minus() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_nested_inner_minus",
    );
}

#[test]
fn default_project_then_bgp_restores_initial() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "default_project_then_bgp_restores_initial",
    );
}

#[test]
fn blank_initial_disjoint() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "blank_initial_disjoint",
    );
}

#[test]
fn blank_initial_shared() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "blank_initial_shared",
    );
}

#[test]
fn typed_initial_disjoint() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "typed_initial_disjoint",
    );
}

#[test]
fn typed_initial_different_lexical() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "typed_initial_different_lexical",
    );
}

#[test]
fn typed_initial_shared() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "typed_initial_shared",
    );
}

#[test]
fn language_initial_disjoint() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "language_initial_disjoint",
    );
}

#[test]
fn language_initial_mismatch() {
    assert_oracle(
        include_str!("fixtures/query-minus-rdflib-regression.json"),
        "language_initial_mismatch",
    );
}

fn original15(id: &str) {
    use oxigraph::io::RdfParser;
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/query-bindings-original15.json")).unwrap();
    let case = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap();
    let data = graph::load(oracle["input_data"].as_str().unwrap().as_bytes()).unwrap();
    let ns = BTreeMap::from([("ex".to_string(), "urn:".to_string())]);
    let bindings = case["bindings"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            let source = format!("<urn:binding> <urn:value> {} .", v["n3"].as_str().unwrap());
            let value = RdfParser::from_format(RdfFormat::NTriples)
                .for_reader(source.as_bytes())
                .next()
                .unwrap()
                .unwrap()
                .object;
            (k.clone(), value)
        })
        .collect();
    let result =
        graph::query_with_options(&data, case["query"].as_str().unwrap(), &ns, &bindings).unwrap();
    match case["kind"].as_str().unwrap() {
        "SELECT" => {
            assert_eq!(result["columns"], case["columns"]);
            let mut actual = result["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    row.as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), binding(v).to_string()))
                        .collect::<BTreeMap<_, _>>()
                })
                .collect::<Vec<_>>();
            let mut expected = case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    row.as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
                        .collect::<BTreeMap<_, _>>()
                })
                .collect::<Vec<_>>();
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected, "original15 {id}");
        }
        "ASK" => assert_eq!(result["value"], case["value"]),
        "CONSTRUCT" | "DESCRIBE" => {
            let mut actual = result["triples"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| format!("{} .", v.as_str().unwrap()))
                .collect::<Vec<_>>();
            let mut expected = case["triples"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>();
            actual.sort();
            expected.sort();
            assert_eq!(actual, expected, "original15 {id}");
        }
        kind => panic!("unsupported original15 parity {kind}"),
    }
}

#[test]
fn original15_filter() {
    original15("filter");
}

#[test]
fn original15_filter_only_parameter() {
    original15("filter_only_parameter");
}

#[test]
fn original15_optional() {
    original15("optional");
}

#[test]
fn original15_union() {
    original15("union");
}

#[test]
fn original15_distinct() {
    original15("distinct");
}

#[test]
fn original15_group() {
    original15("group");
}

#[test]
fn original15_subquery() {
    original15("subquery");
}

#[test]
fn original15_subquery_filter() {
    original15("subquery_filter");
}

#[test]
fn original15_ask() {
    original15("ask");
}

#[test]
fn original15_ask_absent() {
    original15("ask_absent");
}

#[test]
fn original15_ask_exists() {
    original15("ask_exists");
}

#[test]
fn original15_construct() {
    original15("construct");
}

#[test]
fn original15_construct_template_only() {
    original15("construct_template_only");
}

#[test]
fn original15_unused_binding() {
    original15("unused_binding");
}

#[test]
fn original15_describe_hidden_parameter() {
    original15("describe_hidden_parameter");
}
