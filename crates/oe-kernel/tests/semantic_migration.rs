//! Independent migration examples follow W3C SHACL, not implementation outputs.
use oe_kernel::{chapters, graph, shacl};
use serde_json::{Value, json};
use std::path::Path;

const PREFIXES: &str = "@prefix ex: <urn:test:> . @prefix sh: <http://www.w3.org/ns/shacl#> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . ";

fn validate(data: &str, shape: &str) -> Value {
    shacl::validate(
        format!("{PREFIXES}{data}").as_bytes(),
        format!("{PREFIXES}{shape}").as_bytes(),
    )
    .expect("supported migration shape must actually execute")
}

#[test]
fn historical_xml_and_quality_oracles_execute_with_original_expectations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root must resolve before strict source identity checks");
    let report = chapters::migration(&root).unwrap();
    for chapter in ["vol1/ch04", "vol1/ch07"] {
        let row = report["chapters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["chapter"] == chapter)
            .unwrap();
        for scenario in row["scenarios"].as_array().unwrap() {
            let cases = scenario["cases"]
                .as_array()
                .unwrap_or_else(|| panic!("{chapter} must run its frozen oracle: {scenario}"));
            assert_eq!(cases.len(), 2);
            for case in cases {
                assert_eq!(case["execution"], "completed");
                assert_eq!(case["regression"], "pass", "{chapter}: {case}");
            }
        }
    }
}

#[test]
fn property_pair_ordering_checks_every_pair_and_type_errors() {
    // SHACL 4.5.4 requires one finding per failing pair, including type errors.
    let shape =
        "ex:s sh:targetNode ex:item; sh:property [sh:path ex:need; sh:lessThanOrEquals ex:got].";
    let positive = validate("ex:item ex:need 5, 6; ex:got 6, 7.", shape);
    assert_eq!(positive["violation_count"], 0);
    let negative = validate("ex:item ex:need 5, 6; ex:got 4, 5.", shape);
    assert_eq!(negative["violation_count"], 3);
    let incomparable = validate("ex:item ex:need 5; ex:got \"6\".", shape);
    assert_eq!(incomparable["violation_count"], 1);
    let absent = validate("ex:item ex:need 5.", shape);
    assert_eq!(absent["violation_count"], 0);
    // Distinct integers above 2^53 must not collapse through a floating-point cast.
    let precise = validate(
        "ex:item ex:need 9007199254740993; ex:got 9007199254740992.",
        shape,
    );
    assert_eq!(precise["violation_count"], 1);
    let invalid_shape =
        format!("{PREFIXES}ex:s sh:targetNode ex:item; sh:lessThanOrEquals ex:got.");
    assert!(shacl::validate(PREFIXES.as_bytes(), invalid_shape.as_bytes()).is_err());
}

#[test]
fn range_bounds_keep_strictness_and_invalid_values() {
    // SHACL 4.3: a false comparison or a comparison error is a violation.
    let shape = "ex:s sh:targetNode ex:item; sh:property [sh:path ex:power; sh:minExclusive 0; sh:maxInclusive 10].";
    assert_eq!(
        validate("ex:item ex:power 10.", shape)["violation_count"],
        0
    );
    assert_eq!(validate("ex:item ex:power 0.", shape)["violation_count"], 1);
    assert_eq!(
        validate("ex:item ex:power 11.", shape)["violation_count"],
        1
    );
    assert_eq!(
        validate("ex:item ex:power \"bad\".", shape)["violation_count"],
        2
    );
    let inclusive = "ex:s sh:targetNode ex:item; sh:property [sh:path ex:power; sh:minInclusive 0; sh:maxExclusive 10].";
    assert_eq!(
        validate("ex:item ex:power 0.", inclusive)["violation_count"],
        0
    );
    assert_eq!(
        validate("ex:item ex:power 10.", inclusive)["violation_count"],
        1
    );
}

#[test]
fn date_lexical_validation_and_comparison_keep_the_boundary() {
    let shape = "ex:s sh:targetNode ex:item; sh:property [sh:path ex:date; sh:datatype xsd:date; sh:lessThanOrEquals ex:created].";
    let positive = validate(
        "ex:item ex:date \"2024-02-29\"^^xsd:date; ex:created \"2024-03-01\"^^xsd:date.",
        shape,
    );
    assert_eq!(positive["violation_count"], 0);
    let after = validate(
        "ex:item ex:date \"2024-03-02\"^^xsd:date; ex:created \"2024-03-01\"^^xsd:date.",
        shape,
    );
    assert_eq!(after["violation_count"], 1);
    let invalid = validate(
        "ex:item ex:date \"2023-02-29\"^^xsd:date; ex:created \"2024-03-01\"^^xsd:date.",
        shape,
    );
    assert_eq!(invalid["violation_count"], 2);
}

#[test]
fn rdf11_interoperability_is_separate_from_the_frozen_exact_oracle() {
    let expected = json!({"kind":"binding_multiset","variables":["label"],"rows":[{"label":{"value":"motor","term_type":"literal","datatype":null,"language":null}}]});
    let actual = json!({"columns":["label"],"rows":[{"label":{"value":"motor","kind":"literal","datatype":"http://www.w3.org/2001/XMLSchema#string","language":null}}]});
    let retained = expected.clone();
    assert!(!chapters::compare_cq_oracle(&actual, &expected).unwrap());
    assert!(chapters::compare_cq_interoperability(&actual, &expected).unwrap());
    assert_eq!(retained, expected);
    for (field, value) in [
        (
            "datatype",
            json!("http://www.w3.org/2001/XMLSchema#decimal"),
        ),
        ("language", json!("en")),
        ("kind", json!("iri")),
    ] {
        let mut different = actual.clone();
        different["rows"][0]["label"][field] = value;
        assert!(!chapters::compare_cq_interoperability(&different, &expected).unwrap());
    }
    let mut duplicate = actual.clone();
    duplicate["rows"]
        .as_array_mut()
        .unwrap()
        .push(actual["rows"][0].clone());
    assert!(!chapters::compare_cq_interoperability(&duplicate, &expected).unwrap());
}

#[test]
fn graph_merge_separates_blank_nodes_and_document_prefixes() {
    use oxigraph::io::RdfFormat;
    let xml=br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:ex="urn:test:"><rdf:Description rdf:nodeID="shared"><ex:value>left</ex:value></rdf:Description></rdf:RDF>"#;
    let turtle = b"@prefix ex: <urn:test:> . _:shared ex:value \"right\" .";
    let merged = graph::merge_sources(&[
        (xml.as_slice(), RdfFormat::RdfXml),
        (turtle.as_slice(), RdfFormat::Turtle),
    ])
    .unwrap();
    let observed = graph::query(
        &graph::load(&merged).unwrap(),
        "SELECT ?s ?v WHERE {?s <urn:test:value> ?v}",
    )
    .unwrap();
    assert_eq!(observed["row_count"], 2);
    assert_ne!(observed["rows"][0]["s"], observed["rows"][1]["s"]);
    assert_eq!(
        merged,
        graph::merge_sources(&[
            (xml.as_slice(), RdfFormat::RdfXml),
            (turtle.as_slice(), RdfFormat::Turtle)
        ])
        .unwrap()
    );
    assert!(graph::load_format(b"<broken>", RdfFormat::RdfXml).is_err());
}

#[test]
fn declared_prefixes_execute_and_conflicts_remain_errors() {
    let shape = "ex:ns sh:declare [sh:prefix \"e\";sh:namespace \"urn:test:\"^^xsd:anyURI]. ex:s sh:targetNode ex:item;sh:sparql [sh:prefixes ex:ns;sh:select \"SELECT $this WHERE {$this e:value ?v FILTER(?v < 0)}\"].";
    let positive = validate("ex:item ex:value 1.", shape);
    assert_eq!(positive["violation_count"], 0);
    assert_eq!(positive["legacy_document_prefix_fallbacks"], json!([]));
    assert_eq!(
        validate("ex:item ex:value -1.", shape)["violation_count"],
        1
    );
    let conflict = format!(
        "{PREFIXES}{shape} ex:ns sh:declare [sh:prefix \"e\";sh:namespace \"urn:other:\"^^xsd:anyURI]."
    );
    assert!(
        shacl::validate(PREFIXES.as_bytes(), conflict.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("shacl_prefix_conflict")
    );
    // The exact old profile /2 input was refused as an unsupported predicate.
    // Profile /3 supports qualified shapes; without a qualified min/max there
    // is no cardinality constraint. Empty property values remain empty, and
    // this formal result does not establish any engineering acceptance.
    let previously_unsupported = format!(
        "{PREFIXES}ex:s sh:targetNode ex:item;sh:property [sh:path ex:value;sh:qualifiedValueShape [sh:class ex:A]]."
    );
    let expanded = shacl::validate(PREFIXES.as_bytes(), previously_unsupported.as_bytes()).unwrap();
    assert_eq!(expanded["conforms"], true);
    assert_eq!(expanded["violation_count"], 0);
    assert_eq!(expanded["focus_count"], 1);
}
