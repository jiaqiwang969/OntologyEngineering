//! Frozen source-derived selected-graph expectations. No original Python execution.
//! The source-derived fixture is retained unchanged; no original Python execution.
use oxigraph::io::RdfFormat;
use semantica_contracts::{
    SemanticDatasetResourceSelection, SemanticNamedGraphValidationPort,
    SemanticNamedGraphValidationRequest,
};
use semantica_runtime::{dataset::DatasetSession, runtime::RustSemanticRuntime, shacl};
use serde_json::{Value, json};

fn fixture() -> Value {
    semantica_contracts::json::protocol_json(
        include_str!("fixtures/named-graph-source-derived.json"),
        1024 * 1024,
    )
    .unwrap()
}
fn input(value: &Value) -> SemanticNamedGraphValidationRequest {
    serde_json::from_value(value.clone()).unwrap()
}
fn format(value: &str) -> RdfFormat {
    match value {
        "trig" => RdfFormat::TriG,
        "nquads" => RdfFormat::NQuads,
        _ => panic!("fixture format"),
    }
}
fn assert_failure(request: &SemanticNamedGraphValidationRequest, code: &str, phase: &str) {
    let error = RustSemanticRuntime
        .validate_named_graph(request)
        .unwrap_err();
    assert_eq!(error.code, code);
    assert_eq!(error.phase, phase);
    assert!(!error.retryable);
}

#[test]
fn named_graph_source_cases_preserve_selection_and_request_identity() {
    let fixtures = fixture();
    assert_eq!(fixtures["expectation_origin"], "source_derived");
    assert_eq!(fixtures["original_execution"], false);
    let cases = fixtures["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    let mut reports = Vec::new();
    let mut identities = Vec::new();
    for case in cases {
        let request = input(&case["input"]);
        let expected = &case["expected"];
        let response = RustSemanticRuntime
            .validate_named_graph(&request)
            .unwrap_or_else(|e| panic!("{}: {e:?}", case["id"]));
        assert_eq!(response.schema, "semantica.named-graph-validation-result/1");
        let canonical =
            semantica_contracts::json::canonical_json(&serde_json::to_value(&request).unwrap())
                .unwrap();
        assert_eq!(
            response.request_sha256,
            semantica_runtime::sha256(canonical),
            "{}",
            case["id"]
        );
        assert_eq!(response.data_sha256, request.data_sha256);
        assert_eq!(response.shapes_sha256, request.shapes_sha256);
        assert_eq!(response.graph_name, request.graph_name);
        assert_eq!(
            response.selected_quad_count,
            expected["selected_quad_count"].as_u64().unwrap()
        );
        assert_eq!(
            response.revision_before,
            expected["revision_before"].as_u64().unwrap()
        );
        assert_eq!(
            response.revision_after,
            expected["revision_after"].as_u64().unwrap()
        );
        for field in [
            "conforms",
            "findings",
            "violation_count",
            "focus_count",
            "focus_nodes",
        ] {
            assert_eq!(
                response.report[field], expected[field],
                "{} field {field}",
                case["id"]
            );
        }
        assert_eq!(response.report["profile"], shacl::supported());
        assert_eq!(
            response.report["legacy_document_prefix_fallbacks"],
            json!([])
        );
        identities.push(response.request_sha256);
        reports.push(response.report);
    }
    // Foreign graph edits change full-source identity without changing selected findings.
    assert_eq!(reports[1], reports[5]);
    assert_ne!(identities[1], identities[5]);
    assert_ne!(reports[1], reports[6]);
    assert_ne!(identities[1], identities[6]);
    // TriG and N-Quads select the same graph; raw source/request identity remains different.
    assert_eq!(reports[1], reports[7]);
    assert_ne!(identities[1], identities[7]);
}

#[test]
fn named_graph_validation_preserves_session_and_original_default_guard() {
    let fixtures = fixture();
    let source = &fixtures["cases"][0]["input"];
    let mut session = DatasetSession::new();
    session
        .load(
            source["data"].as_str().unwrap().as_bytes(),
            format(source["dataset_format"].as_str().unwrap()),
            None,
            None,
        )
        .unwrap();
    let query = "SELECT ?s ?p ?o WHERE { GRAPH <urn:ex:bad> { ?s ?p ?o } }";
    assert!(!session.query(query, true).unwrap().cache_hit);
    let cached = session.query(query, true).unwrap();
    assert!(cached.cache_hit);
    let before = session.serialize(RdfFormat::NQuads, None).unwrap();
    let revision = session.revision();
    let count = session.quad_count();
    let packages = session.packages();
    // Blank-free source: this existing low-cost snapshot may be primed independently.
    let snapshot = session.snapshot().unwrap();
    let shapes = source["shapes"].as_str().unwrap().as_bytes();
    assert_eq!(
        shacl::validate_dataset(session.dataset(), shapes)
            .unwrap_err()
            .to_string(),
        "shacl_named_graph_view_unsupported"
    );
    for (name, expected_count, conforms) in [
        ("urn:ex:good", 2, true),
        ("urn:ex:bad", 1, false),
        ("urn:ex:absent", 0, true),
    ] {
        let (selected, report) = session.validate_named_graph(name, shapes).unwrap();
        assert_eq!(selected, expected_count);
        assert_eq!(report["conforms"], conforms);
    }
    assert!(
        session
            .validate_named_graph("relative/path", shapes)
            .is_err()
    );
    assert!(
        session
            .validate_named_graph(
                "urn:ex:absent",
                fixtures["malformed_empty_view_shapes"]
                    .as_str()
                    .unwrap()
                    .as_bytes()
            )
            .is_err()
    );
    assert_eq!(session.revision(), revision);
    assert_eq!(session.quad_count(), count);
    assert_eq!(session.serialize(RdfFormat::NQuads, None).unwrap(), before);
    assert_eq!(session.packages(), packages);
    assert_eq!(session.snapshot().unwrap(), snapshot);
    let after = session.query(query, true).unwrap();
    assert!(after.cache_hit);
    assert_eq!(after.result, cached.result);
    assert_eq!(after.revision, cached.revision);
    assert_eq!(
        shacl::validate_dataset(session.dataset(), shapes)
            .unwrap_err()
            .to_string(),
        "shacl_named_graph_view_unsupported"
    );
}

#[test]
fn named_graph_blank_terms_and_language_literals_keep_loaded_identity() {
    let mut session = DatasetSession::new();
    session.load(b"_:b <urn:ex:label> \"Bonjour\"@fr <urn:ex:focus> .\n<urn:ex:part> <urn:ex:links> _:b <urn:ex:elsewhere> .\n", RdfFormat::NQuads, None, None).unwrap();
    // Use the actual once-loaded blank identity as the independent pre-call reference.
    let subject = session
        .dataset()
        .iter()
        .find(|q| {
            q.graph_name.to_string() == "<urn:ex:focus>" && q.predicate.as_str() == "urn:ex:label"
        })
        .unwrap()
        .subject
        .to_string();
    assert!(subject.starts_with("_:"));
    let before = session.serialize(RdfFormat::NQuads, None).unwrap();
    let revision = session.revision();
    let shapes = br#"@prefix sh:<http://www.w3.org/ns/shacl#>.
<urn:ex:shape> sh:targetSubjectsOf <urn:ex:label>; sh:nodeKind sh:IRI; sh:sparql <urn:ex:C>.
<urn:ex:C> sh:message "langue"@fr; sh:select "SELECT $this ?value WHERE {$this <urn:ex:label> ?value}".
"#;
    let (count, report) = session
        .validate_named_graph("urn:ex:focus", shapes)
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(report["focus_count"], 1);
    assert_eq!(
        report["findings"],
        json!([
            {"focus":subject,"path":null,"component":"nodeKind","value":subject,"message":"RDF node kind differs",
             "source_shape":"<urn:ex:shape>","source_constraint_component":"http://www.w3.org/ns/shacl#NodeKindConstraintComponent",
             "severity":"http://www.w3.org/ns/shacl#Violation","messages":[]},
            {"focus":subject,"path":null,"component":"sparql","value":"\"Bonjour\"@fr","message":"langue",
             "source_shape":"<urn:ex:shape>","source_constraint_component":"http://www.w3.org/ns/shacl#SPARQLConstraintComponent",
             "severity":"http://www.w3.org/ns/shacl#Violation","messages":[{"kind":"literal","value":"langue",
             "datatype":"http://www.w3.org/1999/02/22-rdf-syntax-ns#langString","language":"fr"}]}
        ])
    );
    assert_eq!(session.revision(), revision);
    assert_eq!(session.serialize(RdfFormat::NQuads, None).unwrap(), before);

    // Selection must not acquire a new RDFC/snapshot precondition for ambiguous blanks.
    let source = (0..9)
        .map(|i| format!("_:b{i} <urn:ex:label> \"x\" <urn:ex:many> .\n"))
        .collect::<String>();
    let mut many = DatasetSession::new();
    many.load(source.as_bytes(), RdfFormat::NQuads, None, None)
        .unwrap();
    let before = many.serialize(RdfFormat::NQuads, None).unwrap();
    let revision = many.revision();
    let (count, report) = many.validate_named_graph("urn:ex:many", b"@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> sh:targetSubjectsOf <urn:ex:label>; sh:nodeKind sh:BlankNode .").unwrap();
    assert_eq!(count, 9);
    assert_eq!(report["conforms"], true);
    assert_eq!(report["focus_count"], 9);
    assert_eq!(many.revision(), revision);
    assert_eq!(many.serialize(RdfFormat::NQuads, None).unwrap(), before);
}

#[test]
fn named_graph_contract_rejects_source_shape_schema_format_and_selector_drift() {
    let fixtures = fixture();
    let original = input(&fixtures["cases"][0]["input"]);
    let mut changed = original.clone();
    changed.data.push(' ');
    assert_failure(&changed, "source_identity_mismatch", "source");
    let mut changed = original.clone();
    changed.shapes.push(' ');
    assert_failure(&changed, "source_identity_mismatch", "source");
    // A bad digest has precedence over parse/evaluation for invalid RDF too.
    let mut changed = original.clone();
    changed.data = "broken RDF".into();
    assert_failure(&changed, "source_identity_mismatch", "source");
    let mut changed = original.clone();
    changed.shapes = "broken shapes".into();
    assert_failure(&changed, "source_identity_mismatch", "source");
    let mut changed = original.clone();
    changed.schema = fixtures["invalid_schema"].as_str().unwrap().into();
    assert_failure(&changed, "invalid_contract", "contract");
    for name in fixtures["invalid_selectors"].as_array().unwrap() {
        let mut changed = original.clone();
        changed.graph_name = name.as_str().unwrap().into();
        // Rejection is the source-derived semantic requirement; the new wire contract
        // distinguishes an empty field from a nonempty malformed absolute IRI.
        let code = if changed.graph_name.trim().is_empty() {
            "invalid_contract"
        } else {
            "invalid_graph_name"
        };
        assert_failure(&changed, code, "contract");
    }
    for value in fixtures["invalid_formats"].as_array().unwrap() {
        let mut changed = original.clone();
        changed.dataset_format = value.as_str().unwrap().into();
        assert_failure(&changed, "invalid_contract", "contract");
    }
    // A syntactically legal IRI must fail specifically at the decoded byte
    // boundary, not merely because a CLI error was sanitized or RDF was invalid.
    let mut changed = original.clone();
    changed.graph_name = format!("urn:{}", "x".repeat(semantica_runtime::MAX_BYTES - 3));
    assert_eq!(changed.graph_name.len(), semantica_runtime::MAX_BYTES + 1);
    assert!(oxigraph::model::NamedNode::new(changed.graph_name.as_str()).is_ok());
    assert_failure(&changed, "input_budget_exceeded", "contract");
    let mut wire = serde_json::to_value(original).unwrap();
    wire["unknown"] = json!(true);
    assert!(serde_json::from_value::<SemanticNamedGraphValidationRequest>(wire).is_err());
}

#[test]
fn named_graph_empty_selection_preserves_shape_preflight_and_activation() {
    let fixtures = fixture();
    let mut request = input(&fixtures["cases"][0]["input"]);
    request.graph_name = "urn:ex:absent".into();
    request.shapes = fixtures["malformed_empty_view_shapes"]
        .as_str()
        .unwrap()
        .into();
    request.shapes_sha256 = semantica_runtime::sha256(request.shapes.as_bytes());
    let error = RustSemanticRuntime
        .validate_named_graph(&request)
        .unwrap_err();
    assert_eq!(error.code, "named_graph_validation_failed");
    assert_eq!(error.phase, "validate");
    assert!(
        error
            .message
            .contains(fixtures["malformed_empty_view_error"].as_str().unwrap())
    );
    // Activation is tested in fixed cases12..15, including full active sibling/Core findings.
    for case in fixtures["cases"].as_array().unwrap().iter().skip(12) {
        let response = RustSemanticRuntime
            .validate_named_graph(&input(&case["input"]))
            .unwrap();
        assert_eq!(response.report["findings"], case["expected"]["findings"]);
    }
}

#[test]
fn named_graph_selection_cannot_bypass_full_dataset_budget() {
    let fixtures = fixture();
    let original = input(&fixtures["cases"][0]["input"]);
    for source in ["data", "shapes"] {
        let mut changed = original.clone();
        let oversized = "x".repeat(semantica_runtime::MAX_BYTES + 1);
        if source == "data" {
            changed.data = oversized;
        } else {
            changed.shapes = oversized;
        }
        // Check decoded byte bound before hashing/serializing or interpreting content.
        assert_failure(&changed, "input_budget_exceeded", "contract");
    }
    let mut session = DatasetSession::with_resources(&SemanticDatasetResourceSelection {
        schema: "semantica.dataset-resource-selection/1".into(),
        profile: "semantica.context-50k/1".into(),
    })
    .unwrap();
    let data = (0..100_001)
        .map(|i| format!("<urn:s:{i}> <urn:p> <urn:o> <urn:foreign> .\n"))
        .collect::<String>();
    session
        .load(data.as_bytes(), RdfFormat::NQuads, None, None)
        .unwrap();
    let revision = session.revision();
    let before = session.serialize(RdfFormat::NQuads, None).unwrap();
    assert_eq!(
        session
            .validate_named_graph("urn:absent", original.shapes.as_bytes())
            .unwrap_err()
            .to_string(),
        "dataset_quad_budget"
    );
    assert_eq!(session.quad_count(), 100_001);
    assert_eq!(session.revision(), revision);
    assert_eq!(session.serialize(RdfFormat::NQuads, None).unwrap(), before);
    drop(session);
    drop(before);
    // A small selected view does not exempt the rest of the source from byte limits.
    let mut large = DatasetSession::with_resources(&SemanticDatasetResourceSelection {
        schema: "semantica.dataset-resource-selection/1".into(),
        profile: "semantica.context-50k/1".into(),
    })
    .unwrap();
    // Keep each RDF token below the parser's lexical buffer limit; the
    // whole valid Dataset, not an oversized individual token, is the counterexample.
    let literal = "x".repeat(1024 * 1024);
    let source = (0..17)
        .map(|i| format!("<urn:s:{i}> <urn:p> \"{literal}\" <urn:foreign> .\n"))
        .collect::<String>();
    assert!(source.len() > semantica_runtime::MAX_BYTES);
    large
        .load(source.as_bytes(), RdfFormat::NQuads, None, None)
        .unwrap();
    assert_eq!(large.quad_count(), 17);
    let before = large.serialize(RdfFormat::NQuads, None).unwrap();
    assert!(before.len() > semantica_runtime::MAX_BYTES);
    let revision = large.revision();
    assert_eq!(
        large
            .validate_named_graph("urn:absent", original.shapes.as_bytes())
            .unwrap_err()
            .to_string(),
        "dataset_byte_budget"
    );
    assert_eq!(large.revision(), revision);
    assert_eq!(large.serialize(RdfFormat::NQuads, None).unwrap(), before);
    let empty = DatasetSession::new();
    let oversized = vec![b'x'; semantica_runtime::MAX_BYTES + 1];
    assert_eq!(
        empty
            .validate_named_graph("urn:absent", &oversized)
            .unwrap_err()
            .to_string(),
        "rdf_byte_budget"
    );
}
