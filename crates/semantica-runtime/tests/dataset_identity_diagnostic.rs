use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use oxigraph::io::RdfFormat;
use semantica_contracts::{SemanticDatasetIdentityDiagnosticPort, SemanticRdfLibSnapshotRequest};
use semantica_runtime::{dataset::DatasetSession, sha256};
use serde_json::{Value, json};

const WHEN: &str = "2026-08-19T00:00:00Z";

fn request() -> SemanticRdfLibSnapshotRequest {
    SemanticRdfLibSnapshotRequest {
        created_at: WHEN.into(),
    }
}

#[test]
fn original_mixed_case_variants_report_alias_and_separate_identities() {
    let input_fixture: &str =
        &private_test_fixtures::text("original/LANGUAGE-SYMMETRY-OBSERVATION-INPUTS.json");
    let observation_fixture: &str =
        &private_test_fixtures::text("original/LANGUAGE-SYMMETRY-OBSERVATION.json");
    assert_eq!(
        sha256(input_fixture),
        "c085e5214429b7f3bf97662e39385bcabc1f6092d56c606fc74fe8342834f856"
    );
    assert_eq!(
        sha256(observation_fixture),
        "42c18d0e37054991ffff13e004ddc531b68a0c30ca784478b014b2997ca8afbe"
    );
    let inputs: Value = serde_json::from_str(input_fixture).unwrap();
    let observations: Value = serde_json::from_str(observation_fixture).unwrap();
    for plan in inputs["plans"].as_array().unwrap() {
        let name = plan[0].as_str().unwrap();
        let source = plan[1].as_str().unwrap();
        let reversed = source.lines().rev().collect::<Vec<_>>().join("\n") + "\n";
        let renamed = source
            .replace("_:a", "_:temporary")
            .replace("_:b", "_:a")
            .replace("_:temporary", "_:b");
        let original = observations["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["case_id"] == name)
            .unwrap();
        let observed_hashes = original["unique_hashes"].as_array().unwrap();
        let mut rdf11_identity = None;
        for variant in [source.to_owned(), reversed, renamed] {
            let mut session = DatasetSession::new();
            session
                .load_rdflib(variant.as_bytes(), RdfFormat::NQuads, None)
                .unwrap();
            let before_revision = session.revision();
            let before_legacy = session.snapshot_rdflib(WHEN).unwrap();
            let before_rdf11 = session.snapshot().unwrap();
            let report = session.diagnose_identity(&request()).unwrap();
            report.validate().unwrap();
            assert_eq!(report.schema, "semantica.dataset-identity-diagnostic/1");
            assert_eq!(
                report.diagnostic_scope,
                "retained_projection_language_case/1"
            );
            assert_eq!(
                report.legacy_stability_assessment,
                "known_language_case_alias"
            );
            assert_eq!(report.loaded_source_byte_sha256, vec![sha256(&variant)]);
            assert_eq!(report.legacy_snapshot, before_legacy);
            assert_eq!(report.rdf11_snapshot, before_rdf11);
            assert_eq!(report.revision, before_revision);
            assert_eq!(session.revision(), before_revision);
            assert_eq!(session.snapshot_rdflib(WHEN).unwrap(), before_legacy);
            assert_eq!(session.snapshot().unwrap(), before_rdf11);
            assert!(observed_hashes.contains(&json!(report.legacy_snapshot.dataset_sha256)));
            assert_eq!(report.language_case_aliases.len(), 1);
            let alias = &report.language_case_aliases[0];
            assert_eq!(alias.text_sha256, sha256("hello"));
            assert_eq!(alias.datatype, None);
            assert_eq!(alias.language_lowercase, "en");
            assert_eq!(alias.retained_spellings, vec!["EN", "en"]);
            if let Some(previous) = &rdf11_identity {
                assert_eq!(&report.rdf11_snapshot.dataset_sha256, previous);
            } else {
                rdf11_identity = Some(report.rdf11_snapshot.dataset_sha256.clone());
            }
        }
    }
}

#[test]
fn plain_typed_string_and_same_owner_mutations_keep_distinct_domains() {
    let source = "<urn:s> <urn:p> \"value\" .\n<urn:s> <urn:p> \"value\"^^<http://www.w3.org/2001/XMLSchema#string> .\n";
    let integer = "<urn:i> <urn:p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n";
    let mut session = DatasetSession::new();
    session
        .load_rdflib(source.as_bytes(), RdfFormat::NQuads, None)
        .unwrap();
    let report = session.diagnose_identity(&request()).unwrap();
    assert_eq!(report.legacy_snapshot.quad_count, 2);
    assert_eq!(report.rdf11_snapshot.quad_count, 1);
    assert_eq!(report.legacy_stability_assessment, "not_established");
    assert_eq!(report.revision, 1);
    session
        .load_rdflib(integer.as_bytes(), RdfFormat::NQuads, None)
        .unwrap();
    let core_before_delete = session.snapshot().unwrap();
    let deleted = session
        .update_rdflib("DELETE DATA { <urn:s> <urn:p> \"value\" . }")
        .unwrap();
    assert_eq!(deleted.removed, 1);
    let report = session.diagnose_identity(&request()).unwrap();
    assert_eq!(report.revision, deleted.revision);
    assert_eq!(report.legacy_snapshot.revision, report.revision);
    assert_eq!(report.rdf11_snapshot.revision, report.revision);
    assert_eq!(report.legacy_snapshot.quad_count, 2);
    assert_eq!(report.rdf11_snapshot.quad_count, 2);
    assert_eq!(
        report.rdf11_snapshot.dataset_sha256,
        core_before_delete.dataset_sha256
    );
    assert!(
        report
            .rdf11_snapshot
            .canonical_nquads
            .contains("\"01\"^^<http://www.w3.org/2001/XMLSchema#integer>")
    );
    assert_eq!(
        report.loaded_source_byte_sha256,
        vec![sha256(source), sha256(integer)]
    );
    let cleared = session.clear_rdflib(None).unwrap();
    let empty = session.diagnose_identity(&request()).unwrap();
    assert_eq!(empty.revision, cleared.revision);
    assert_eq!(empty.legacy_snapshot.quad_count, 0);
    assert_eq!(empty.rdf11_snapshot.quad_count, 0);
    assert!(empty.loaded_source_byte_sha256.is_empty());
    assert!(empty.language_case_aliases.is_empty());
    assert_eq!(empty.legacy_stability_assessment, "not_established");
}

#[test]
fn different_terms_and_lost_spelling_do_not_claim_stability() {
    for source in [
        "<urn:a> <urn:p> \"one\"@EN .\n<urn:b> <urn:p> \"two\"@en .\n<urn:c> <urn:p> \"one\"@fr .\n",
        "<urn:a> <urn:p> \"one\"@EN .\n<urn:a> <urn:p> \"one\"@en .\n",
    ] {
        let mut session = DatasetSession::new();
        session
            .load_rdflib(source.as_bytes(), RdfFormat::NQuads, None)
            .unwrap();
        let report = session.diagnose_identity(&request()).unwrap();
        assert!(report.language_case_aliases.is_empty());
        assert_eq!(report.legacy_stability_assessment, "not_established");
        assert_eq!(report.loaded_source_byte_sha256, vec![sha256(source)]);
        assert_eq!(report.revision, session.revision());
    }
}

#[test]
fn unavailable_projection_and_caller_graph_refuse_atomically() {
    for field in ["graph", "source_sha256", "stable", "proof", "revision"] {
        let mut input = json!({"created_at": WHEN});
        input[field] = json!("caller_invented_authority");
        assert!(serde_json::from_value::<SemanticRdfLibSnapshotRequest>(input).is_err());
    }
    let mut session = DatasetSession::new();
    session
        .load_rdflib(b"<urn:s> <urn:p> \"value\" .\n", RdfFormat::NQuads, None)
        .unwrap();
    let before_revision = session.revision();
    let before_legacy = session.snapshot_rdflib(WHEN).unwrap();
    let before_rdf11 = session.snapshot().unwrap();
    let overlong = SemanticRdfLibSnapshotRequest {
        created_at: "x".repeat(1025),
    };
    let refusal = session.diagnose_identity(&overlong).unwrap_err();
    assert_eq!(refusal.code, "identity_diagnostic_failed");
    assert!(
        refusal
            .message
            .contains("identity_diagnostic_time_byte_budget")
    );
    assert_eq!(session.revision(), before_revision);
    assert_eq!(session.snapshot_rdflib(WHEN).unwrap(), before_legacy);
    assert_eq!(session.snapshot().unwrap(), before_rdf11);
    session
        .update("INSERT DATA { <urn:other> <urn:p> \"other\" . }")
        .unwrap();
    let revision = session.revision();
    let core = session.snapshot().unwrap();
    let refusal = session.diagnose_identity(&request()).unwrap_err();
    assert_eq!(refusal.code, "identity_diagnostic_failed");
    assert!(
        refusal
            .message
            .contains("rdflib_lossless_source_projection_unavailable")
    );
    assert_eq!(session.revision(), revision);
    assert_eq!(session.snapshot().unwrap(), core);
    assert!(session.snapshot_rdflib(WHEN).is_err());
}
