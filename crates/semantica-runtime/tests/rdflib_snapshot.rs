use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use oxigraph::io::RdfFormat;
use semantica_runtime::dataset::DatasetSession;
use serde_json::Value;

#[test]
fn original_rdflib_constructor_snapshot_bytes_and_mutations() {
    let input_fixture: &str = &private_test_fixtures::text("original/ORIGINAL-FROZEN-INPUTS.json");
    let expected_fixture: &str =
        &private_test_fixtures::text("original/ORIGINAL-FROZEN-ORACLE.json");
    assert_eq!(
        semantica_runtime::sha256(input_fixture),
        "868fc20bf19f1f31d3fa2392a00b2e0a5d605ebe3226c3d4f744bdc42201f4d2"
    );
    assert_eq!(
        semantica_runtime::sha256(expected_fixture),
        "978699b8085dd92084c3074671838b793b2b638ec0f52c1cd78212dd04b60123"
    );
    let input: Value = serde_json::from_str(input_fixture).unwrap();
    let oracle: Value = serde_json::from_str(expected_fixture).unwrap();
    assert_eq!(input["cases"].as_array().unwrap().len(), 52);
    assert_eq!(oracle["case_count"], 52);
    for (case, expected) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(oracle["cases"].as_array().unwrap())
    {
        let mut session = DatasetSession::new();
        for (index, action) in case["actions"].as_array().unwrap().iter().enumerate() {
            let mutation = match action["operation"].as_str().unwrap() {
                "load" => session
                    .load_rdflib(
                        action["data"].as_str().unwrap().as_bytes(),
                        RdfFormat::NQuads,
                        None,
                    )
                    .unwrap(),
                "clear" => session.clear_rdflib(action["graph_name"].as_str()).unwrap(),
                "update" => session
                    .update_rdflib(action["query"].as_str().unwrap())
                    .unwrap(),
                _ => unreachable!(),
            };
            let mut mutation = serde_json::to_value(mutation).unwrap();
            if action["operation"] == "update" {
                mutation.as_object_mut().unwrap().remove("operation");
            }
            assert_eq!(
                mutation, expected["mutations"][index],
                "{} operation {index}",
                case["case_id"]
            );
            assert_eq!(
                serde_json::to_value(
                    session
                        .snapshot_rdflib(input["created_at"].as_str().unwrap())
                        .unwrap()
                )
                .unwrap(),
                expected["intermediate_snapshots"][index],
                "{} intermediate {index}",
                case["case_id"]
            );
        }
        let got = session
            .snapshot_rdflib(input["created_at"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            serde_json::to_value(got).unwrap(),
            expected["snapshot"],
            "{}",
            case["case_id"]
        );
        assert_eq!(
            serde_json::to_value(session.snapshot_rdflib("2026-08-20T00:00:00Z").unwrap()).unwrap(),
            expected["rebound_snapshot"],
            "{} time rebind",
            case["case_id"]
        );
    }
}

#[test]
fn additional_original_constructor_lexical_and_random_graphs() {
    let input_fixture: &str = &private_test_fixtures::text("original/EXTRA-FROZEN-INPUTS.json");
    let expected_fixture: &str = &private_test_fixtures::text("original/EXTRA-FROZEN-ORACLE.json");
    let inputs: Value = serde_json::from_str(input_fixture).unwrap();
    let oracle: Value = serde_json::from_str(expected_fixture).unwrap();
    assert_eq!(
        semantica_runtime::sha256(input_fixture),
        oracle["input_sha256"]
    );
    assert_eq!(inputs["cases"].as_array().unwrap().len(), 91);
    let mut differences = Vec::new();
    for (case, expected) in inputs["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(oracle["cases"].as_array().unwrap())
    {
        let mut session = DatasetSession::new();
        match session
            .load_rdflib(
                case["data"].as_str().unwrap().as_bytes(),
                RdfFormat::NQuads,
                None,
            )
            .and_then(|_| session.snapshot_rdflib(inputs["created_at"].as_str().unwrap()))
        {
            Ok(snapshot) => {
                let got = serde_json::to_value(snapshot).unwrap();
                if got != expected["snapshot"] {
                    differences.push(format!(
                        "{}: got {} expected {}",
                        case["case_id"], got, expected["snapshot"]
                    ));
                }
            }
            Err(e) => differences.push(format!("{}: error {}", case["case_id"], e)),
        }
    }
    assert!(
        differences.is_empty(),
        "{} original differences:\n{}",
        differences.len(),
        differences.join("\n")
    );
}

#[test]
fn original_numeric_boundary_constructor_snapshots() {
    let input: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/BOUNDARY-FROZEN-INPUTS.json",
    ))
    .unwrap();
    let oracle: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/BOUNDARY-FROZEN-ORACLE.json",
    ))
    .unwrap();
    assert_eq!(input["cases"].as_array().unwrap().len(), 28);
    assert_eq!(
        semantica_runtime::sha256(&private_test_fixtures::text(
            "original/BOUNDARY-FROZEN-INPUTS.json"
        )),
        oracle["input_sha256"]
    );
    let mut differences = Vec::new();
    for (case, expected) in input["cases"].as_array().unwrap()[..22]
        .iter()
        .zip(&oracle["cases"].as_array().unwrap()[..22])
    {
        let mut session = DatasetSession::new();
        let actual = session
            .load_rdflib(
                case["data"].as_str().unwrap().as_bytes(),
                RdfFormat::NQuads,
                None,
            )
            .and_then(|_| session.snapshot_rdflib(input["created_at"].as_str().unwrap()));
        match actual {
            Ok(snapshot) => {
                if serde_json::to_value(snapshot).unwrap() != expected["snapshot"] {
                    differences.push(format!(
                        "{} got {:?} expected {}",
                        case["case_id"],
                        session.snapshot_rdflib(input["created_at"].as_str().unwrap()),
                        expected["snapshot"]
                    ));
                }
            }
            Err(e) => differences.push(format!("{} error {e}", case["case_id"])),
        }
    }
    assert!(
        differences.is_empty(),
        "original boundary differences:\n{}",
        differences.join("\n")
    );
}

const PAIR:&[u8]=b"<urn:s> <urn:p> \"a\" .\n<urn:s> <urn:p> \"a\"^^<http://www.w3.org/2001/XMLSchema#string> .\n";

#[test]
fn independently_frozen_original_token_whitespace_constructor_snapshots() {
    let raw = &private_test_fixtures::text("normalizer-review/CONSTRUCTOR-INPUTS.json");
    let input: Value = serde_json::from_str(raw).unwrap();
    let oracle: Value = serde_json::from_str(&private_test_fixtures::text(
        "normalizer-review/CONSTRUCTOR-ORACLE.json",
    ))
    .unwrap();
    assert_eq!(semantica_runtime::sha256(raw), oracle["input_sha256"]);
    assert_eq!(input["cases"].as_array().unwrap().len(), 10);
    let mut differences = Vec::new();
    for (case, expected) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(oracle["cases"].as_array().unwrap())
    {
        let mut session = DatasetSession::new();
        session
            .load_rdflib(
                case["actions"][0]["data"].as_str().unwrap().as_bytes(),
                RdfFormat::NQuads,
                None,
            )
            .unwrap();
        let got =
            serde_json::to_value(session.snapshot_rdflib("2026-08-19T00:00:00Z").unwrap()).unwrap();
        if got != expected["snapshot"] {
            differences.push(format!(
                "{} got {got} expected {}",
                case["case_id"], expected["snapshot"]
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "original whitespace differences:\n{}",
        differences.join("\n")
    );
}

#[test]
fn one_revision_owner_and_explicit_term_domains_do_not_rewrite_rdfc() {
    let mut session = DatasetSession::new();
    let current = session.load(PAIR, RdfFormat::NTriples, None, None).unwrap();
    assert_eq!(
        (current.added, current.quad_count, current.revision),
        (1, 1, 1)
    );
    let rdfc = session.snapshot().unwrap();
    let legacy = session.snapshot_rdflib("first").unwrap();
    assert_eq!((legacy.quad_count, legacy.revision), (2, 1));
    assert_eq!(session.snapshot().unwrap(), rdfc);
    let rebased = session.snapshot_rdflib("second").unwrap();
    assert_eq!(legacy.dataset_sha256, rebased.dataset_sha256);
    assert_eq!(session.revision(), 1);
    let query = session
        .query("SELECT (COUNT(*) AS ?n) WHERE {?s ?p ?o}", false)
        .unwrap();
    assert!(
        serde_json::to_string(&query)
            .unwrap()
            .contains("\"value\":\"1\"")
    );
    let repeat = session
        .load_rdflib(PAIR, RdfFormat::NTriples, None)
        .unwrap();
    assert_eq!(
        (repeat.added, repeat.quad_count, repeat.revision),
        (0, 2, 2)
    );
    assert_eq!(
        session.snapshot_rdflib("third").unwrap().dataset_sha256,
        legacy.dataset_sha256
    );
    assert_eq!(
        session.snapshot().unwrap().dataset_sha256,
        rdfc.dataset_sha256
    );
    session
        .update("DELETE DATA {<urn:s> <urn:p> \"a\" .}")
        .unwrap();
    assert_eq!(session.revision(), 3);
    assert!(session.snapshot_rdflib("fourth").is_err());
    session.clear(None).unwrap();
    assert_eq!(session.snapshot_rdflib("empty").unwrap().quad_count, 0);
    assert_eq!(session.revision(), 4);
}

#[test]
fn source_backed_batches_and_verified_packages_keep_one_revision() {
    let mut session = DatasetSession::new();
    let sources = [
        (PAIR, RdfFormat::NTriples, None, None),
        (
            b"_:a <urn:p> \"b\" .\n".as_slice(),
            RdfFormat::NTriples,
            None,
            None,
        ),
    ];
    let batch = session.load_batch(&sources).unwrap();
    assert_eq!((batch.quad_count, batch.revision), (2, 1));
    assert_eq!(
        (
            session.snapshot_rdflib("batch").unwrap().quad_count,
            session.revision()
        ),
        (3, 1)
    );
    let mut package = DatasetSession::new();
    package.snapshot_rdflib("pre").unwrap();
    package
        .load_verified_package("original-terms", "1", &"a".repeat(64), &sources)
        .unwrap();
    assert_eq!(
        (
            package.snapshot_rdflib("package").unwrap().quad_count,
            package.revision()
        ),
        (3, 1)
    );
    let legacy = package.snapshot_rdflib("before").unwrap();
    package
        .load_verified_package("original-terms", "1", &"a".repeat(64), &sources)
        .unwrap();
    assert_eq!(package.snapshot_rdflib("before").unwrap(), legacy);
    let empty = [(b"".as_slice(), RdfFormat::Turtle, None, None)];
    package
        .load_verified_package("unsupported-source", "1", &"b".repeat(64), &empty)
        .unwrap();
    assert_eq!(package.revision(), 1);
    assert!(package.snapshot_rdflib("stale-must-not-return").is_err());
}

#[test]
fn unsupported_original_source_profiles_and_malformed_mutations_are_atomic() {
    let mut session = DatasetSession::new();
    session
        .load_rdflib(PAIR, RdfFormat::NTriples, None)
        .unwrap();
    let before = session.snapshot_rdflib("frozen").unwrap();
    for data in [b"<urn:s> <urn:p>".as_slice(), b"<urn:s> <urn:p> \"x\\q\" ."] {
        assert!(
            session
                .load_rdflib(data, RdfFormat::NTriples, None)
                .is_err()
        );
        assert_eq!(session.snapshot_rdflib("frozen").unwrap(), before);
    }
    assert!(
        session
            .load_rdflib(b"<urn:s> <urn:p> 2.", RdfFormat::Turtle, None)
            .is_err()
    );
    for update in [
        "INSERT DATA {<urn:s> <urn:p> \"z\"}",
        "DELETE WHERE {?s ?p ?o}",
        "DELETE DATA {<urn:s> <urn:p> \"a\"}; DELETE DATA {<urn:s> <urn:p> \"a\"}",
        "PREFIX u:<urn:> DELETE DATA {u:s u:p \"a\" .}",
        "DELETE DATA {GRAPH <urn:g> {<urn:s> <urn:p> \"a\"}}",
    ] {
        assert!(session.update_rdflib(update).is_err());
        assert_eq!(session.snapshot_rdflib("frozen").unwrap(), before);
    }
    let inputs: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/BOUNDARY-FROZEN-INPUTS.json",
    ))
    .unwrap();
    // The T predecessor rejected six original-success normalizers. Its exact
    // source/receipt remains archived; this version checks all six original
    // successful snapshots in rdflib_literal_consumer, after actual migration.
    assert_eq!(inputs["cases"].as_array().unwrap()[22..].len(), 6);
    assert_eq!(session.revision(), 1);
}

#[test]
fn original_canonicalization_budget_and_generated_port_fail_explicitly() {
    use semantica_contracts::{SemanticRdfLibSnapshotPort, SemanticRdfLibSnapshotRequest};
    let mut session = DatasetSession::new();
    let data = (0..1025)
        .map(|i| format!("_:n{i} <urn:p> \"{i}\" .\n"))
        .collect::<String>();
    session
        .load_rdflib(data.as_bytes(), RdfFormat::NTriples, None)
        .unwrap();
    let error = session
        .snapshot_legacy(&SemanticRdfLibSnapshotRequest {
            created_at: "budget".into(),
        })
        .unwrap_err();
    assert!(error.message.contains("rdflib_blank_node_budget"));
    assert_eq!(session.revision(), 1);
    assert!(
        serde_json::from_str::<SemanticRdfLibSnapshotRequest>(
            r#"{"created_at":"x","graph":["caller invented"]}"#
        )
        .is_err()
    );
    let snapshot = DatasetSession::new()
        .snapshot_legacy(&SemanticRdfLibSnapshotRequest {
            created_at: String::new(),
        })
        .unwrap();
    assert_eq!((snapshot.quad_count, snapshot.revision), (0, 0));
    assert!(snapshot.created_at.ends_with('Z'));
}

#[test]
fn legacy_delete_and_clear_preserve_unrelated_actual_core_lexical_terms() {
    let mut session = DatasetSession::new();
    let data = [
        PAIR,
        b"<urn:n> <urn:p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n",
    ]
    .concat();
    session
        .load_rdflib(&data, RdfFormat::NTriples, None)
        .unwrap();
    let rdfc_before = session.snapshot().unwrap();
    session
        .update_rdflib("DELETE DATA {<urn:s> <urn:p> \"a\" .}")
        .unwrap();
    // The other original xsd:string still projects to the same RDF1.1 quad;
    // unrelated raw integer lexical identity must not be normalized by DELETE.
    assert_eq!(
        session.snapshot().unwrap().dataset_sha256,
        rdfc_before.dataset_sha256
    );
    let mut session = DatasetSession::new();
    session
        .load_rdflib(&data, RdfFormat::NTriples, None)
        .unwrap();
    let rdfc_before = session.snapshot().unwrap();
    session
        .load_rdflib(
            b"<urn:t> <urn:p> \"b\" <urn:g> .\n",
            RdfFormat::NQuads,
            None,
        )
        .unwrap();
    session.clear_rdflib(Some("urn:g")).unwrap();
    assert_eq!(
        session.snapshot().unwrap().dataset_sha256,
        rdfc_before.dataset_sha256
    );
}

#[test]
fn original_python_integer_conversion_limit_preserves_failed_lexical_values() {
    let raw = &private_test_fixtures::text("original/INTEGER-LIMIT-FROZEN-INPUTS.json");
    let input: Value = serde_json::from_str(raw).unwrap();
    let oracle: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/INTEGER-LIMIT-FROZEN-ORACLE.json",
    ))
    .unwrap();
    assert_eq!(semantica_runtime::sha256(raw), oracle["input_sha256"]);
    assert_eq!(input["python_int_max_str_digits"], 4300);
    for (case, expected) in input["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(oracle["cases"].as_array().unwrap())
    {
        let mut session = DatasetSession::new();
        session
            .load_rdflib(
                case["data"].as_str().unwrap().as_bytes(),
                RdfFormat::NQuads,
                None,
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(
                session
                    .snapshot_rdflib(input["created_at"].as_str().unwrap())
                    .unwrap()
            )
            .unwrap(),
            expected["snapshot"],
            "{}",
            case["case_id"]
        );
    }
}

#[test]
fn post_batch_blank_nodes_follow_existing_parser_identity_not_reused_revision_labels() {
    let mut session = DatasetSession::new();
    let data = b"_:a <urn:p> \"same\" .\n".as_slice();
    session
        .load_batch(&[
            (data, RdfFormat::NTriples, None, None),
            (data, RdfFormat::NTriples, None, None),
        ])
        .unwrap();
    assert_eq!((session.revision(), session.quad_count()), (1, 2));
    session
        .load_rdflib(data, RdfFormat::NTriples, None)
        .unwrap();
    assert_eq!(
        (
            session.revision(),
            session.quad_count(),
            session.snapshot_rdflib("after").unwrap().quad_count
        ),
        (2, 3, 3)
    );
}
