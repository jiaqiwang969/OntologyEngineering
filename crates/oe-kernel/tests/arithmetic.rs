use oe_kernel::graph;

#[test]
fn numeric_operators_preserve_left_associativity_and_parentheses() {
    let store = graph::load(b"").unwrap();
    for (expression, expected) in [
        ("120.0 - 100.0 - 15.0 - 5.0", "0"),
        ("5.0 + 3.0 - 6.0 - 2.0", "0"),
        ("24.0 / 3.0 / 2.0", "4"),
        ("24.0 / 3.0 * 2.0", "16"),
        ("20.0 - 6.0 + 2.0", "16"),
        ("20.0 - (6.0 + 2.0)", "12"),
        ("24.0 / (3.0 / 2.0)", "16"),
        ("2.0 + 3.0 * 4.0", "14"),
        ("2.0 - -3.0 - 4.0", "1"),
    ] {
        let query = format!("SELECT ({expression} AS ?result) WHERE {{}}");
        let result = graph::query(&store, &query).unwrap();
        assert_eq!(
            result["rows"][0]["result"]["value"], expected,
            "{expression}"
        );
    }
}

#[test]
fn balance_with_bound_inputs_matches_independent_arithmetic() {
    let store = graph::load(b"").unwrap();
    let result = graph::query(&store, "SELECT (ABS(?gross - ?effective - ?retained - ?other) > ?tolerance AS ?violated) WHERE { VALUES (?gross ?effective ?retained ?other ?tolerance) {(120.0 100.0 15.0 5.0 0.5)} }").unwrap();
    assert_eq!(result["rows"][0]["violated"]["value"], "false");
}

#[test]
fn rdf_values_keep_original_lexical_and_datatype_identity() {
    let data = graph::load(
        br#"<urn:part> <urn:count> "003"^^<http://www.w3.org/2001/XMLSchema#nonNegativeInteger>."#,
    )
    .unwrap();
    let result = graph::query(
        &data,
        "SELECT ?value WHERE { <urn:part> <urn:count> ?value }",
    )
    .unwrap();
    assert_eq!(result["rows"][0]["value"]["value"], "003");
    assert_eq!(
        result["rows"][0]["value"]["datatype"],
        "http://www.w3.org/2001/XMLSchema#nonNegativeInteger"
    );
}
