use semantica_runtime::sha256 as private_fixture_sha256;
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use oxigraph::io::RdfFormat;
use semantica_runtime::dataset::DatasetSession;
use serde_json::Value;

const STAMP: &str = "2026-08-19T00:00:00Z";

fn compare_original_constructor(
    inputs: &str,
    oracle: &str,
    input_sha: &str,
    oracle_sha: &str,
    count: usize,
) {
    assert_eq!(semantica_runtime::sha256(inputs), input_sha);
    assert_eq!(semantica_runtime::sha256(oracle), oracle_sha);
    let inputs: Value = serde_json::from_str(inputs).unwrap();
    let oracle: Value = serde_json::from_str(oracle).unwrap();
    assert_eq!(oracle["input_sha256"], input_sha);
    let cases = inputs["cases"].as_array().unwrap();
    let expected_cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), count);
    assert_eq!(expected_cases.len(), count);
    let mut differences = Vec::new();
    for (case, expected) in cases.iter().zip(expected_cases) {
        assert_eq!(case["case_id"], expected["case_id"]);
        let data = case["nquads"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "<urn:s> <urn:p> {}^^<{}> .\n",
                    serde_json::to_string(&case["text"]).unwrap(),
                    case["datatype"].as_str().unwrap()
                )
            });
        let mut session = DatasetSession::new();
        let before = serde_json::to_value(session.snapshot().unwrap()).unwrap();
        let before_legacy = serde_json::to_value(session.snapshot_rdflib(STAMP).unwrap()).unwrap();
        let mutation = session.load_rdflib(data.as_bytes(), RdfFormat::NQuads, None);
        if expected.get("constructor_error").is_some() {
            if mutation.is_ok() {
                differences.push(format!(
                    "{} expected original constructor error, accepted",
                    case["case_id"]
                ));
            }
            assert_eq!(
                serde_json::to_value(session.snapshot().unwrap()).unwrap(),
                before,
                "{} error must preserve actual dataset identity/revision/cache",
                case["case_id"]
            );
            assert_eq!(
                serde_json::to_value(session.snapshot_rdflib(STAMP).unwrap()).unwrap(),
                before_legacy,
                "{} error must preserve lossless projection/cache",
                case["case_id"]
            );
            // Also exercise errors against a populated owner, with both caches read.
            session.load_rdflib(b"<urn:seed> <urn:predicate> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n", RdfFormat::NQuads, None).unwrap();
            let seeded = serde_json::to_value(session.snapshot().unwrap()).unwrap();
            let seeded_legacy =
                serde_json::to_value(session.snapshot_rdflib(STAMP).unwrap()).unwrap();
            assert!(
                session
                    .load_rdflib(data.as_bytes(), RdfFormat::NQuads, None)
                    .is_err(),
                "{} populated error",
                case["case_id"]
            );
            assert_eq!(
                serde_json::to_value(session.snapshot().unwrap()).unwrap(),
                seeded
            );
            assert_eq!(
                serde_json::to_value(session.snapshot_rdflib(STAMP).unwrap()).unwrap(),
                seeded_legacy
            );
            continue;
        }
        match mutation {
            Err(e) => differences.push(format!(
                "{} original success, Rust error {e:#}",
                case["case_id"]
            )),
            Ok(mutation) => {
                let actual = serde_json::to_value(mutation).unwrap();
                if actual != expected["mutation"] {
                    differences.push(format!(
                        "{} mutation {} expected {}",
                        case["case_id"], actual, expected["mutation"]
                    ));
                }
                match session.snapshot_rdflib(STAMP) {
                    Err(e) => differences.push(format!("{} snapshot error {e:#}", case["case_id"])),
                    Ok(snapshot) => {
                        let actual = serde_json::to_value(snapshot).unwrap();
                        if actual != expected["snapshot"] {
                            differences.push(format!(
                                "{} full snapshot {} expected {}",
                                case["case_id"], actual, expected["snapshot"]
                            ));
                        }
                        let rebound = session.snapshot_rdflib("2026-08-20T00:00:00Z").unwrap();
                        let mut rebound_expected = expected["snapshot"].clone();
                        rebound_expected["created_at"] = "2026-08-20T00:00:00Z".into();
                        assert_eq!(
                            serde_json::to_value(rebound).unwrap(),
                            rebound_expected,
                            "{} observed time rebind",
                            case["case_id"]
                        );
                    }
                }
            }
        }
    }
    assert!(
        differences.is_empty(),
        "{} original constructor differences:\n{}",
        differences.len(),
        differences.join("\n")
    );
}

#[test]
fn original_282_literal_constructor_mutation_and_full_snapshot() {
    compare_original_constructor(
        &private_test_fixtures::text("literal-eight/ORIGINAL-INPUTS-01.json"),
        &private_test_fixtures::text("literal-eight/ORIGINAL-ORACLE-01.json"),
        "0364e11610abc2e7e2ef68944093408d5800f17e4a1ade89a02d39015d5c2f02",
        "c6055f9eb11a907a4dd062efe5d03315877a4f8afe18fc9556d6e55415e31d69",
        282,
    );
}

#[test]
fn original_310_supplement_constructor_mutation_and_full_snapshot() {
    compare_original_constructor(
        &private_test_fixtures::text("literal-eight/SUPPLEMENT-INPUTS-01.json"),
        &private_test_fixtures::text("literal-eight/SUPPLEMENT-ORACLE-01.json"),
        "2dfe02d8f42e2e82a1e3defea7c588c6fa1e10c7ec937863230206a2b8c65a19",
        "1e2844b46a0b2ffac161a9eba116b61c44da4b5cf5474c215369650fd23756e7",
        310,
    );
}

#[test]
fn original_139_xml_html_complete_constructor_snapshots() {
    compare_original_constructor(
        &private_test_fixtures::text("literal-xml-html/ORIGINAL-FROZEN-INPUTS.json"),
        &private_test_fixtures::text("literal-xml-html/ORIGINAL-FROZEN-ORACLE.json"),
        "59a754dd427a2caff0dd02d29332e4cfd31a7e2c011354aae1da5d4f29dff6f2",
        "f1cf869a2bffcdcc68ca2225c5aaee4ced8302945f6f36201ee2a6179f5f0dfc",
        139,
    );
}

#[test]
fn original_83_xml_html_unicode_and_markup_boundary_snapshots() {
    compare_original_constructor(
        &private_test_fixtures::text("literal-xml-html/EXTRA-ORIGINAL-FROZEN-INPUTS.json"),
        &private_test_fixtures::text("literal-xml-html/EXTRA-ORIGINAL-FROZEN-ORACLE.json"),
        "b64704a32907a60c369f07c383ae7faec8c6d347cda161368ef2c82e548fd035",
        "e8ef8f1787579c87b9cbd8f44022a70f9307ee4fda161a5c060408b746363a6c",
        83,
    );
}

#[test]
fn original_126_xml_html_entity_and_parser_policy_snapshots() {
    compare_original_constructor(
        &private_test_fixtures::text("literal-xml-html/POLICY-ORIGINAL-FROZEN-INPUTS.json"),
        &private_test_fixtures::text("literal-xml-html/POLICY-ORIGINAL-FROZEN-ORACLE.json"),
        "08eafa6065396b59c2b842371c699ebd2a625eaa733fed8d0be2cd0ba6707812",
        "3d9fcda841f142b605a14c9be42b27de12c64facc1f8a01991b583cac9597856",
        126,
    );
}

#[test]
fn original_10_html_internal_alias_and_foreign_namespace_snapshots() {
    compare_original_constructor(
        &private_test_fixtures::text("literal-xml-html/BINDING-ORIGINAL-FROZEN-INPUTS.json"),
        &private_test_fixtures::text("literal-xml-html/BINDING-ORIGINAL-FROZEN-ORACLE.json"),
        "5422fa04cd4441f8b26cb6e83497e5648b3b786e3c4c28467164a16fe660d2db",
        "1fff14a5b500d396fd25cf58d32f7c1d6a5e5080574fb5a7616eaeca0dfee207",
        10,
    );
}

#[test]
fn formerly_unported_original_six_successes_compare_complete_snapshot_and_mutation() {
    let inputs: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/BOUNDARY-FROZEN-INPUTS.json",
    ))
    .unwrap();
    let oracle: Value = serde_json::from_str(&private_test_fixtures::text(
        "original/BOUNDARY-FROZEN-ORACLE.json",
    ))
    .unwrap();
    assert_eq!(
        semantica_runtime::sha256(&private_test_fixtures::text(
            "original/BOUNDARY-FROZEN-INPUTS.json"
        )),
        oracle["input_sha256"]
    );
    assert_eq!(inputs["cases"].as_array().unwrap().len(), 28);
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 28);
    for (case, expected) in inputs["cases"].as_array().unwrap()[22..]
        .iter()
        .zip(&oracle["cases"].as_array().unwrap()[22..])
    {
        let mut session = DatasetSession::new();
        let mutation = session
            .load_rdflib(
                case["data"].as_str().unwrap().as_bytes(),
                RdfFormat::NQuads,
                None,
            )
            .unwrap();
        assert_eq!(
            serde_json::to_value(mutation).unwrap(),
            expected["mutation"],
            "{}",
            case["case_id"]
        );
        assert_eq!(
            serde_json::to_value(
                session
                    .snapshot_rdflib(inputs["created_at"].as_str().unwrap())
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
fn lexical_work_budgets_fail_before_parse_fallback_and_preserve_populated_owner() {
    let mut session = DatasetSession::new();
    session
        .load_rdflib(
            b"<urn:seed> <urn:p> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n",
            RdfFormat::NQuads,
            None,
        )
        .unwrap();
    let before = session.snapshot().unwrap();
    let before_legacy = session.snapshot_rdflib(STAMP).unwrap();
    for (datatype, text) in [
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral",
            "x".repeat(10_001),
        ),
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#HTML",
            "x".repeat(10_001),
        ),
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#XMLLiteral",
            "<x/>".repeat(257),
        ),
        (
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#HTML",
            "<span>x</span>".repeat(129),
        ),
        (
            "http://www.w3.org/2001/XMLSchema#base64Binary",
            "x".repeat(10_001),
        ),
    ] {
        let input = format!(
            "<urn:s> <urn:p> {}^^<{}> .\n",
            serde_json::to_string(&text).unwrap(),
            datatype
        );
        assert!(
            session
                .load_rdflib(input.as_bytes(), RdfFormat::NQuads, None)
                .is_err(),
            "{} resource refusal must not become retained lexical success",
            datatype
        );
        assert_eq!(session.snapshot().unwrap(), before);
        assert_eq!(session.snapshot_rdflib(STAMP).unwrap(), before_legacy);
    }
}
