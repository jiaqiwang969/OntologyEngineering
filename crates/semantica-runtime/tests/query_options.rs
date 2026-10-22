use oxigraph::model::{Literal, NamedNode, Term};
use semantica_runtime::graph;
use std::collections::BTreeMap;

#[test]
fn typed_bindings_and_namespaces_preserve_select_ask_construct_and_language() {
    let data = graph::load(
        b"<urn:part> <urn:name> \"shaft\"@en . <urn:other> <urn:name> \"bearing\"@en .",
    )
    .unwrap();
    let namespaces = BTreeMap::from([("ex".into(), "urn:".into())]);
    let bindings = BTreeMap::from([(
        "?s".into(),
        Term::NamedNode(NamedNode::new("urn:part").unwrap()),
    )]);
    let result = graph::query_with_options(
        &data,
        "SELECT ?name WHERE { ?s ex:name ?name }",
        &namespaces,
        &bindings,
    )
    .unwrap();
    assert_eq!(result["row_count"], 1);
    assert_eq!(result["rows"][0]["name"]["value"], "shaft");
    assert_eq!(result["rows"][0]["name"]["language"], "en");
    let literal = BTreeMap::from([(
        "name".into(),
        Term::Literal(Literal::new_language_tagged_literal("shaft", "en").unwrap()),
    )]);
    let ask = graph::query_with_options(
        &data,
        "ASK { ex:part ex:name ?name }",
        &namespaces,
        &literal,
    )
    .unwrap();
    assert_eq!(ask["value"], true);
    let constructed = graph::query_with_options(
        &data,
        "CONSTRUCT { ?s ex:label ?name } WHERE { ?s ex:name ?name }",
        &namespaces,
        &bindings,
    )
    .unwrap();
    assert_eq!(constructed["triples"].as_array().unwrap().len(), 1);
    assert!(
        constructed["triples"][0]
            .as_str()
            .unwrap()
            .contains("shaft")
    );
}

#[test]
fn query_options_do_not_authorize_remote_queries_or_bad_rdf_parameters() {
    let data = graph::load(b"").unwrap();
    let prefixes = BTreeMap::from([("remote".into(), "https://example.invalid/sparql".into())]);
    assert!(
        graph::query_with_options(
            &data,
            "ASK { SERVICE remote: { ?s ?p ?o } }",
            &prefixes,
            &BTreeMap::new()
        )
        .unwrap_err()
        .to_string()
        .contains("remote_service_unsupported")
    );
    let bad = BTreeMap::from([("ex".into(), "relative namespace".into())]);
    assert!(graph::query_with_options(&data, "ASK {}", &bad, &BTreeMap::new()).is_err());
    let bad_binding = BTreeMap::from([(
        "invalid name".into(),
        Term::Literal(Literal::new_simple_literal("x")),
    )]);
    assert!(graph::query_with_options(&data, "ASK {}", &BTreeMap::new(), &bad_binding).is_err());
}

// Frozen original RDFLib 7.6.0 local oracle: private
// var/maintenance/query-bindings-parity/20261002-01/FROZEN-RDFLIB-ORACLE.json.
fn options_data() -> oxigraph::model::Dataset {
    graph::load(
        br#"<urn:part> <urn:name> "shaft"@en ; <urn:tag> "A", "B" ;
             <urn:alias> "shaft"@en .
            <urn:other> <urn:name> "bearing"@en ; <urn:tag> "A" ;
             <urn:note> "catalog" ."#,
    )
    .unwrap()
}
fn ex() -> BTreeMap<String, String> {
    BTreeMap::from([("ex".into(), "urn:".into())])
}
fn part_binding() -> BTreeMap<String, Term> {
    BTreeMap::from([("s".into(), NamedNode::new("urn:part").unwrap().into())])
}
fn assert_name_projection(result: &serde_json::Value, rows: u64) {
    assert_eq!(result["columns"], serde_json::json!(["name"]));
    assert_eq!(result["row_count"], rows);
    for row in result["rows"].as_array().unwrap() {
        assert_eq!(row.as_object().unwrap().len(), 1);
        assert_eq!(row["name"]["value"], "shaft");
        assert_eq!(row["name"]["language"], "en");
    }
}

#[test]
fn initial_bindings_reach_filters_optional_union_and_distinct_without_projection_leaks() {
    let data = options_data();
    let bindings = part_binding();
    for (query, expected_rows) in [
        (
            "SELECT ?name WHERE { ?s ex:name ?name FILTER(?s = ex:part) }",
            1,
        ),
        (
            "SELECT ?name WHERE { { ?s ex:name ?name } UNION { ?s ex:alias ?name } }",
            2,
        ),
        (
            "SELECT DISTINCT ?name WHERE { ?s ex:name ?name ; ex:tag ?tag }",
            1,
        ),
    ] {
        let result = graph::query_with_options(&data, query, &ex(), &bindings).unwrap();
        assert_name_projection(&result, expected_rows);
    }
    let optional = graph::query_with_options(
        &data,
        "SELECT ?name ?note WHERE { ?s ex:name ?name OPTIONAL { ?s ex:note ?note } }",
        &ex(),
        &bindings,
    )
    .unwrap();
    assert_eq!(optional["columns"], serde_json::json!(["name", "note"]));
    assert_eq!(optional["row_count"], 1);
    assert_eq!(optional["rows"][0]["name"]["value"], "shaft");
    assert!(optional["rows"][0].get("note").is_none());
}

#[test]
fn filter_only_initial_variables_and_unused_bindings_remain_available_in_scope() {
    let data = options_data();
    let mut bindings = part_binding();
    bindings.insert("gate".into(), Literal::new_simple_literal("go").into());
    bindings.insert(
        "unused".into(),
        Literal::new_simple_literal("unused").into(),
    );
    let result = graph::query_with_options(
        &data,
        "SELECT ?name WHERE { ?s ex:name ?name FILTER(?gate = \"go\") }",
        &ex(),
        &bindings,
    )
    .unwrap();
    assert_name_projection(&result, 1);
    let result = graph::query_with_options(
        &data,
        "SELECT ?name WHERE { ?s ex:name ?name }",
        &ex(),
        &bindings,
    )
    .unwrap();
    assert_name_projection(&result, 1);
}

#[test]
fn grouped_counts_and_subqueries_observe_nonprojected_initial_variables() {
    let data = options_data();
    let bindings = part_binding();
    let counted = graph::query_with_options(
        &data,
        "SELECT (COUNT(?tag) AS ?count) WHERE { ?s ex:name ?name ; ex:tag ?tag }",
        &ex(),
        &bindings,
    )
    .unwrap();
    assert_eq!(counted["columns"], serde_json::json!(["count"]));
    assert_eq!(counted["row_count"], 1);
    assert_eq!(counted["rows"][0]["count"]["value"], "2");
    assert_eq!(
        counted["rows"][0]["count"]["datatype"],
        "http://www.w3.org/2001/XMLSchema#integer"
    );
    let result = graph::query_with_options(
        &data,
        "SELECT ?name WHERE { { SELECT ?name WHERE { ?s ex:name ?name } } }",
        &ex(),
        &bindings,
    )
    .unwrap();
    assert_name_projection(&result, 1);
    let mut bindings = bindings;
    bindings.insert("gate".into(), Literal::new_simple_literal("go").into());
    let result = graph::query_with_options(
        &data,
        "SELECT ?name WHERE { { SELECT ?name WHERE { ?s ex:name ?name FILTER(BOUND(?gate)) } } }",
        &ex(),
        &bindings,
    )
    .unwrap();
    assert_name_projection(&result, 1);
}

#[test]
fn ask_exists_and_construct_use_typed_initial_values_without_extra_results() {
    let data = options_data();
    let bindings = part_binding();
    for query in [
        "ASK { ?s ex:name ?name }",
        "ASK { FILTER EXISTS { ?s ex:name ?name } }",
    ] {
        let result = graph::query_with_options(&data, query, &ex(), &bindings).unwrap();
        assert_eq!(result["value"], true);
    }
    let missing = BTreeMap::from([("s".into(), NamedNode::new("urn:missing").unwrap().into())]);
    let result =
        graph::query_with_options(&data, "ASK { ?s ex:name ?name }", &ex(), &missing).unwrap();
    assert_eq!(result["value"], false);
    let constructed = graph::query_with_options(
        &data,
        "CONSTRUCT { ?s ex:label ?name } WHERE { ?s ex:name ?name }",
        &ex(),
        &bindings,
    )
    .unwrap();
    assert_eq!(
        constructed["triples"],
        serde_json::json!(["<urn:part> <urn:label> \"shaft\"@en"])
    );
    let template_binding = BTreeMap::from([(
        "target".into(),
        NamedNode::new("urn:document").unwrap().into(),
    )]);
    let constructed = graph::query_with_options(
        &data,
        "CONSTRUCT { ?target ex:label ?name } WHERE { ex:part ex:name ?name }",
        &ex(),
        &template_binding,
    )
    .unwrap();
    assert_eq!(
        constructed["triples"],
        serde_json::json!(["<urn:document> <urn:label> \"shaft\"@en"])
    );
}

#[test]
fn values_conflicts_and_alias_binding_conflicts_do_not_override_initial_identity() {
    let data = options_data();
    let result = graph::query_with_options(
        &data,
        "SELECT ?name WHERE { ?s ex:name ?name VALUES ?s { ex:other } }",
        &ex(),
        &part_binding(),
    )
    .unwrap();
    assert_eq!(result["row_count"], 0);
    let bindings = BTreeMap::from([
        ("s".into(), NamedNode::new("urn:part").unwrap().into()),
        ("?s".into(), NamedNode::new("urn:other").unwrap().into()),
    ]);
    assert!(
        graph::query_with_options(
            &data,
            "SELECT ?name WHERE { ?s ex:name ?name }",
            &ex(),
            &bindings
        )
        .unwrap_err()
        .to_string()
        .contains("query_initial_binding_alias_conflict")
    );
}

#[test]
fn describe_target_scope_is_separate_and_initial_reassignment_still_fails() {
    let data = options_data();
    let bound_other = BTreeMap::from([("s".into(), NamedNode::new("urn:other").unwrap().into())]);
    // The frozen original oracle describes only ex:part. Extending the
    // DESCRIBE projection with bound ?s would wrongly describe ex:other too.
    // Original4c52 independent output now requires actual current parity.
    let described = graph::query_with_options(
        &data,
        "DESCRIBE ex:part WHERE { ?s ex:name ?name }",
        &ex(),
        &bound_other,
    )
    .unwrap();
    let original: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/query-bindings-original15.json")).unwrap();
    let expected = original["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == "describe_hidden_parameter")
        .unwrap();
    let actual: Vec<_> = described["triples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|triple| format!("{} .", triple.as_str().unwrap()))
        .collect();
    assert_eq!(
        actual,
        expected["triples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|triple| triple.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    let described = graph::query_with_options(
        &data,
        "DESCRIBE ?s WHERE { ?s ex:name ?name }",
        &ex(),
        &part_binding(),
    )
    .unwrap();
    assert_eq!(described["triples"].as_array().unwrap().len(), 4);
    assert!(
        described["triples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| !v.as_str().unwrap().contains("urn:other"))
    );
    let bound_name = BTreeMap::from([("name".into(), Literal::new_simple_literal("prior").into())]);
    let error = graph::query_with_options(
        &data,
        "SELECT ?name WHERE { BIND(\"next\" AS ?name) }",
        &ex(),
        &bound_name,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(error, "query_initial_binding_reassignment_unsupported");
}

#[test]
fn query_local_prefixes_and_fixed_value_types_preserve_their_original_identity() {
    let data = options_data();
    let supplied = BTreeMap::from([("ex".into(), "urn:unused:".into())]);
    let result = graph::query_with_options(
        &data,
        "PREFIX ex: <urn:> SELECT ?name WHERE { ?s ex:name ?name }",
        &supplied,
        &part_binding(),
    )
    .unwrap();
    assert_name_projection(&result, 1);
    let mut bindings = part_binding();
    bindings.insert(
        "name".into(),
        Literal::new_language_tagged_literal("shaft", "fr")
            .unwrap()
            .into(),
    );
    let result = graph::query_with_options(
        &data,
        "SELECT ?name WHERE { ?s ex:name ?name }",
        &ex(),
        &bindings,
    )
    .unwrap();
    assert_eq!(result["row_count"], 0);
}

#[test]
fn minus_initial_slot_preserves_the_original_nonempty_result_domain() {
    let data = options_data();
    let query = "SELECT ?name WHERE { ?s ex:name ?name MINUS { SELECT ?dummy WHERE { BIND(\"x\" AS ?dummy) } } }";
    let result = graph::query_with_options(&data, query, &ex(), &part_binding()).unwrap();
    assert_name_projection(&result, 1);
    let unbound = graph::query_with_options(&data, query, &ex(), &BTreeMap::new()).unwrap();
    assert_eq!(unbound["row_count"], 2);
}
