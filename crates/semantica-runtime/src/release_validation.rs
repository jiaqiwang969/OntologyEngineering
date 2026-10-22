//! Release-content checks against the caller's existing DatasetSession.
//!
//! Content validity, retained execution, compilation origin and engineering
//! adoption are separate questions. This module does not publish a package,
//! create an execution token or authorize an engineering conclusion.
use crate::{dataset::DatasetSession, execution_provenance};
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticReleaseContentCheck as Check, SemanticReleaseContentVerdict as Verdict,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const LEGACY_PROFILE: &str = "semantica.release-content-legacy-data/1";
pub const RUST_PROFILE: &str = "semantica.release-content-rust/1";

fn digest(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
/// Preserve the original runtime helper's normalized 7..64 hexadecimal domain.
/// The refinery's stricter legacy source DTO is a different contract.
pub fn is_legacy_source_revision(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        let s = semantica_contracts::json::python_strip(s).to_lowercase();
        (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
    })
}
pub fn chapter_contract_hash(assets: &[semantica_contracts::SemanticPackageAsset]) -> Option<&str> {
    let mut selected = assets.iter().filter(|asset| {
        matches!(
            semantica_contracts::json::python_strip(&asset.role)
                .to_lowercase()
                .as_str(),
            "chapter_contract" | "chapter_manifest" | "contract"
        )
    });
    let first = selected.next()?;
    if selected.next().is_some() {
        None
    } else {
        Some(first.sha256.as_str())
    }
}
fn check(checks: &mut Vec<Check>, id: &str, passed: bool, message: &str) {
    checks.push(Check {
        check_id: id.into(),
        passed,
        message: message.into(),
    });
}
fn verdict(profile: &str, receipt: &Value, checked_at: &str, checks: Vec<Check>) -> Verdict {
    let reasons = checks
        .iter()
        .filter(|v| !v.passed)
        .map(|v| v.check_id.clone())
        .collect::<Vec<_>>();
    Verdict {
        schema_version: "1.0".into(),
        profile: profile.into(),
        status: if reasons.is_empty() {
            "complete"
        } else {
            "blocked"
        }
        .into(),
        receipt_sha256: receipt["receipt_sha256"].as_str().unwrap_or("").into(),
        checked_at: checked_at.into(),
        checks,
        reasons,
    }
}
fn current_source_matches(receipt: &Value, source: &Value) -> bool {
    let Ok(source) =
        serde_json::from_value::<semantica_contracts::SemanticRefineryRustSource>(source.clone())
    else {
        return false;
    };
    source.validate().is_ok()
        && source.executable_size_bytes > 0
        && source.status == "source_bound"
        && receipt["runtime_commit"] == format!("semantica.rust-source/{}", source.source_sha256)
        && receipt["runtime_artifact_sha256"] == source.executable_sha256
}
fn count_matches(value: &Value, expected: u64) -> bool {
    match value {
        Value::Bool(value) => u64::from(*value) == expected,
        Value::Number(value) => {
            if let Some(n) = value.as_u64() {
                return n == expected;
            }
            if value.as_i64().is_some() {
                return false;
            }
            value.as_f64().is_some_and(|n| {
                n.is_finite()
                    && n >= 0.0
                    && n < 18446744073709551616.0
                    && n == n.trunc()
                    && n as u64 == expected
            })
        }
        _ => false,
    }
}

fn verify_content(
    receipt: &Value,
    registered: Option<&semantica_contracts::SemanticPackageLoadResult>,
    snapshot: &Value,
    runtime_profile: &Value,
    checked_at: &str,
    current_source: Option<&Value>,
) -> Result<Verdict> {
    ensure!(receipt.is_object(), "release_receipt_object_required");
    ensure!(
        receipt["schema_version"] == "1.0",
        "release_receipt_schema_invalid"
    );
    ensure!(
        digest(&receipt["receipt_sha256"]),
        "release_receipt_digest_invalid"
    );
    ensure!(
        receipt["asset_hashes"].is_object(),
        "release_asset_hashes_object_required"
    );
    ensure!(
        receipt["output_hashes"].is_object(),
        "release_output_hashes_object_required"
    );
    // Original runtime lookup happens before every package-related check.
    ensure!(
        !receipt["package_id"].is_array()
            && !receipt["package_id"].is_object()
            && !receipt["package_version"].is_array()
            && !receipt["package_version"].is_object(),
        "release_package_key_not_hashable"
    );
    let registered = registered.filter(|p| {
        receipt["package_id"] == p.identity.package_id
            && receipt["package_version"] == p.identity.version
    });
    let mut checks = Vec::with_capacity(15);
    check(
        &mut checks,
        "receipt.integrity",
        execution_provenance::verify_receipt(receipt)?,
        "receipt, reports, and provenance hashes must verify",
    );
    let runtime_version = &receipt["runtime_version"];
    let identifies_version = match runtime_version {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(_) => !semantica_contracts::json::python_equal(runtime_version, &json!(0))?,
        Value::String(value) => {
            !value.is_empty() && !["unknown", "not-installed"].contains(&value.as_str())
        }
        Value::Array(value) if value.is_empty() => false,
        Value::Object(value) if value.is_empty() => false,
        _ => anyhow::bail!("release_runtime_version_not_hashable"),
    };
    check(
        &mut checks,
        "runtime.version",
        identifies_version,
        "runtime_version must identify the executing Semantica build",
    );
    match current_source {
        None => check(
            &mut checks,
            "runtime.commit",
            is_legacy_source_revision(&receipt["runtime_commit"]),
            "runtime_commit must be an injected Git-style source revision",
        ),
        Some(source) => check(
            &mut checks,
            "runtime.source_identity",
            current_source_matches(receipt, source),
            "Rust source and executable observations must match their explicit source profile",
        ),
    }
    check(
        &mut checks,
        "runtime.artifact_sha256",
        digest(&receipt["runtime_artifact_sha256"]),
        "runtime_artifact_sha256 must bind the installed wheel/artifact",
    );
    check(
        &mut checks,
        "package.identity",
        registered.is_some_and(|p| {
            receipt["package_digest"] == p.identity.digest && digest(&receipt["package_digest"])
        }),
        "receipt package id, version, and digest must match a registered package",
    );
    let expected_assets = registered.map(|p| {
        p.assets
            .iter()
            .map(|a| (a.asset_id.clone(), json!(a.sha256)))
            .collect::<serde_json::Map<_, _>>()
    });
    let assets = receipt["asset_hashes"].as_object().unwrap();
    check(
        &mut checks,
        "package.asset_hashes",
        !assets.is_empty()
            && expected_assets.as_ref() == Some(assets)
            && assets.values().all(digest),
        "every registered package asset must be content-bound",
    );
    let expected_contract = registered.and_then(|p| chapter_contract_hash(&p.assets));
    check(
        &mut checks,
        "package.chapter_contract_sha256",
        expected_contract.is_some_and(|s| {
            receipt["chapter_contract_sha256"] == s && digest(&receipt["chapter_contract_sha256"])
        }),
        "exactly one registered chapter contract must match the receipt",
    );
    check(
        &mut checks,
        "dataset.integrity",
        digest(&receipt["dataset_sha256"])
            && receipt["dataset_sha256"] == snapshot["dataset_sha256"]
            && snapshot["quad_count"]
                .as_u64()
                .is_some_and(|count| count_matches(&receipt["dataset_quad_count"], count))
            && snapshot["revision"]
                .as_u64()
                .is_some_and(|revision| count_matches(&receipt["dataset_revision"], revision)),
        "receipt dataset must match the current canonical runtime dataset",
    );
    for kind in ["capability", "cq", "shacl", "oracle"] {
        let report = &receipt[format!("{kind}_report")];
        check(
            &mut checks,
            &format!("report.{kind}"),
            report["kind"] == kind
                && report["status"] == "passed"
                && execution_provenance::verify_report(report)?,
            &format!("{kind} report must be present, intact, and passed"),
        );
    }
    let capability = &receipt["capability_report"]["payload"];
    check(
        &mut checks,
        "report.capability_profile",
        capability.is_object()
            && match capability.get("profile") {
                Some(profile) => semantica_contracts::json::python_equal(profile, runtime_profile)?,
                None => false,
            }
            && capability["missing"] == json!([])
            && capability["unknown"] == json!([]),
        "capability report must describe this runtime profile without gaps",
    );
    check(
        &mut checks,
        "output.hashes",
        receipt["output_hashes"]
            .as_object()
            .unwrap()
            .values()
            .all(digest),
        "every declared output hash must be a SHA-256 digest",
    );
    check(
        &mut checks,
        "provenance.bundle",
        execution_provenance::verify_bundle(&receipt["provenance_bundle"])?
            && execution_provenance::verify_receipt_bindings(receipt)?,
        "provenance bundle must be intact and bind the exact receipt evidence",
    );
    Ok(verdict(
        if current_source.is_some() {
            RUST_PROFILE
        } else {
            LEGACY_PROFILE
        },
        receipt,
        checked_at,
        checks,
    ))
}

/// Original fifteen content checks on legacy DTO data. Legacy RDFLib snapshot
/// bytes keep their own algorithm and are never generated by rewriting RDFC.
/// A `complete` value describes this data; it does not certify execution.
pub fn verify_legacy_content_data(
    receipt: &Value,
    registered: Option<&semantica_contracts::SemanticPackageLoadResult>,
    snapshot: &Value,
    runtime_profile: &Value,
    checked_at: &str,
) -> Result<Verdict> {
    verify_content(
        receipt,
        registered,
        snapshot,
        runtime_profile,
        checked_at,
        None,
    )
}
/// New Rust source identity keeps its own domain rather than impersonating Git.
/// Compilation authentication and engineering adoption remain separate evidence.
pub fn verify_rust_content(
    receipt: &Value,
    session: &mut DatasetSession,
    runtime_profile: &Value,
    checked_at: &str,
    source: &Value,
) -> Result<Verdict> {
    let snapshot = serde_json::to_value(session.snapshot()?)?;
    let package_id = receipt["package_id"]
        .as_str()
        .context("release_package_id_required")?;
    let version = receipt["package_version"]
        .as_str()
        .context("release_package_version_required")?;
    verify_content(
        receipt,
        session.registered_package(package_id, version),
        &snapshot,
        runtime_profile,
        checked_at,
        Some(source),
    )
}
/// The original runner merges the two checks by id and sorts all seventeen.
pub fn with_runner_checks(
    mut value: Verdict,
    declared_release_status: &str,
    scenario_passed: bool,
) -> Result<Verdict> {
    let declared = if declared_release_status.is_empty() {
        "missing"
    } else {
        declared_release_status
    };
    let declaration_message = format!(
        "manifest release_status must be complete; got {}",
        semantica_contracts::json::python_repr(&json!(declared))?
    );
    check(
        &mut value.checks,
        "package.declared_release_status",
        declared_release_status == "complete",
        &declaration_message,
    );
    check(
        &mut value.checks,
        "runner.scenario_execution",
        scenario_passed,
        "scenario execution and exact oracle checks must pass",
    );
    value.checks = value
        .checks
        .into_iter()
        .map(|check| (check.check_id.clone(), check))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect();
    value.reasons = value
        .checks
        .iter()
        .filter(|v| !v.passed)
        .map(|v| v.check_id.clone())
        .collect();
    value.status = if value.reasons.is_empty() {
        "complete"
    } else {
        "blocked"
    }
    .into();
    Ok(value)
}
