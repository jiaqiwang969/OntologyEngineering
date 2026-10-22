//! Independent expected values came from the unmodified, pinned original
//! SemanticRuntime and runner. These legacy DTO tests certify content parity,
//! not a new execution, source authentication or engineering adoption.
use anyhow::Result;
use semantica_contracts::SemanticPackageLoadResult;
use semantica_runtime::{book_runner, dataset::DatasetSession, release_validation};
use serde_json::{Value, json};

const ORIGINAL: &str = include_str!("fixtures/release_original.json");
const SCALAR: &str = include_str!("fixtures/release_original_scalar.json");
const EQUALITY: &str = include_str!("fixtures/release_original_equality.json");

#[test]
fn original_actual_runtime_65_release_content_cases() -> Result<()> {
    assert_eq!(
        semantica_runtime::sha256(ORIGINAL),
        "ac9fcb6578ea07e3bfb6d162d9d07276970441f607688129aba4ec89e72673e6"
    );
    let original: Value = serde_json::from_str(ORIGINAL)?;
    assert_eq!(original["case_count"], 65);
    assert_eq!(
        original["original_oracle_sha256"],
        "5106de9f3f8f831b3decc27bccf01bbdecafda42e52794a26aabb03754e4dec4"
    );
    assert_eq!(
        original["original_context_sha256"],
        "5a892b4b276053f5022389aa725860eb2bdac1bbdcb7bf786632089ef6e839f5"
    );
    for case in original["cases"].as_array().unwrap() {
        let id = case["case_id"].as_str().unwrap();
        let expected = &case["expected_verdict"];
        if id == "foreign-runner-reverify" {
            let value = book_runner::verify(
                &book_runner::BookRunnerState::default(),
                &mut DatasetSession::default(),
                &case["receipt"],
                &json!({}),
            )?;
            assert_eq!(value["status"], expected["status"], "{id}");
            assert_eq!(
                value["checks"][0]["check_id"], expected["checks"][0]["check_id"],
                "{id}"
            );
            assert_eq!(value["checks"][0]["passed"], false, "{id}");
            continue;
        }
        let registered: Option<SemanticPackageLoadResult> = if case["registered"].is_null() {
            None
        } else {
            Some(serde_json::from_value(case["registered"].clone())?)
        };
        let mut verdict = release_validation::verify_legacy_content_data(
            &case["receipt"],
            registered.as_ref(),
            &case["snapshot"],
            &case["runtime_profile"],
            expected["checked_at"].as_str().unwrap(),
        )?;
        if expected["checks"].as_array().unwrap().len() == 17 {
            verdict = release_validation::with_runner_checks(
                verdict,
                case["declared_release_status"].as_str().unwrap(),
                case["original_scenario_passed"].as_bool().unwrap(),
            )?;
        }
        let mut actual = serde_json::to_value(verdict)?;
        assert_eq!(
            actual["profile"],
            release_validation::LEGACY_PROFILE,
            "{id}"
        );
        actual.as_object_mut().unwrap().remove("profile");
        assert_eq!(&actual, expected, "independent original case: {id}");
    }
    Ok(())
}

#[test]
fn legacy_revision_keeps_python_unicode_strip_and_explicit_rust_domain() {
    assert!(release_validation::is_legacy_source_revision(&json!(
        "\u{001c}ABCDEF0\u{001f}"
    )));
    assert!(!release_validation::is_legacy_source_revision(&json!(
        format!("semantica.rust-source/{}", "a".repeat(64))
    )));
    assert!(!release_validation::is_legacy_source_revision(&json!(true)));
}

#[test]
fn legacy_data_lookup_duplicate_merge_and_integer_precision_keep_original_meaning() -> Result<()> {
    let fixture: Value = serde_json::from_str(ORIGINAL)?;
    let case = &fixture["cases"][0];
    let registered: SemanticPackageLoadResult = serde_json::from_value(case["registered"].clone())?;
    let at = case["expected_verdict"]["checked_at"].as_str().unwrap();
    let mut wrong_key = registered.clone();
    wrong_key.identity.package_id = "unregistered-other-key".into();
    let wrong = release_validation::verify_legacy_content_data(
        &case["receipt"],
        Some(&wrong_key),
        &case["snapshot"],
        &case["runtime_profile"],
        at,
    )?;
    for id in [
        "package.identity",
        "package.asset_hashes",
        "package.chapter_contract_sha256",
    ] {
        assert!(
            !wrong
                .checks
                .iter()
                .find(|check| check.check_id == id)
                .unwrap()
                .passed,
            "{id}"
        );
    }
    let original = release_validation::verify_legacy_content_data(
        &case["receipt"],
        Some(&registered),
        &case["snapshot"],
        &case["runtime_profile"],
        at,
    )?;
    let blocked = release_validation::with_runner_checks(original, "partial", false)?;
    let replaced = release_validation::with_runner_checks(blocked, "complete", true)?;
    assert_eq!(replaced.checks.len(), 17);
    assert_eq!(replaced.status, "complete");
    // This is a pure-data integer comparison, not a manufactured large Dataset.
    let mut receipt = case["receipt"].clone();
    let mut snapshot = case["snapshot"].clone();
    snapshot["quad_count"] = json!(9007199254740992u64);
    receipt["dataset_quad_count"] = json!(9007199254740993u64);
    let unequal = release_validation::verify_legacy_content_data(
        &receipt,
        Some(&registered),
        &snapshot,
        &case["runtime_profile"],
        at,
    )?;
    assert!(
        !unequal
            .checks
            .iter()
            .find(|check| check.check_id == "dataset.integrity")
            .unwrap()
            .passed
    );
    Ok(())
}

#[test]
fn original_actual_runtime_eight_scalar_constructor_cases() -> Result<()> {
    assert_eq!(
        semantica_runtime::sha256(SCALAR),
        "36a39d98a5f05b8909d1efeea7da5f404b8d43b7bda8a5f9157876fa29e14773"
    );
    let fixture: Value = serde_json::from_str(SCALAR)?;
    assert_eq!(fixture["case_count"], 8);
    assert_eq!(
        fixture["original_oracle_sha256"],
        "7fd1681f96771cb7393b939162f770dbc8533a6262364a301369a183f4785999"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let registered: SemanticPackageLoadResult =
            serde_json::from_value(case["registered"].clone())?;
        let expected = &case["expected_verdict"];
        let actual = release_validation::verify_legacy_content_data(
            &case["receipt"],
            Some(&registered),
            &case["snapshot"],
            &case["runtime_profile"],
            expected["checked_at"].as_str().unwrap(),
        )?;
        let mut actual = serde_json::to_value(actual)?;
        assert_eq!(actual["profile"], release_validation::LEGACY_PROFILE);
        actual.as_object_mut().unwrap().remove("profile");
        assert_eq!(
            &actual, expected,
            "independent original scalar case: {}",
            case["case_id"]
        );
    }
    Ok(())
}

#[test]
fn original_actual_runtime_six_deep_json_equality_cases() -> Result<()> {
    let fixture: Value = serde_json::from_str(EQUALITY)?;
    assert_eq!(fixture["case_count"], 6);
    assert_eq!(
        fixture["original_oracle_sha256"],
        "4a1d0b3da26fdfd4784c626f8225a9b754ef63ad9a2d4b6554b89b934ee732bf"
    );
    let mut mismatches = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let registered: SemanticPackageLoadResult =
            serde_json::from_value(case["registered"].clone())?;
        let expected = &case["expected_verdict"];
        let actual = release_validation::verify_legacy_content_data(
            &case["receipt"],
            Some(&registered),
            &case["snapshot"],
            &case["runtime_profile"],
            expected["checked_at"].as_str().unwrap(),
        )?;
        let mut actual = serde_json::to_value(actual)?;
        actual.as_object_mut().unwrap().remove("profile");
        if &actual != expected {
            mismatches.push(json!({"case_id":case["case_id"],"expected":expected["reasons"],"actual":actual["reasons"]}));
        }
    }
    assert!(
        mismatches.is_empty(),
        "actual independent original differences: {mismatches:?}"
    );
    Ok(())
}
