//! Pure translations of refinery.py:5696-5973 and :7805-7825.
//!
//! These functions calculate checks, not execution authority. The owning Engine
//! must reread and validate retained suite/result/receipt/manifest/closure bytes
//! before calling them and must recompute all six regression results itself.
//! In particular, CQ bindings are canonical *entry* digests, never the digest of
//! a whole CQ-registry asset. No filesystem, registry, clock, or mutation lives
//! in this module.
use anyhow::{Result, anyhow, ensure};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const REGRESSION_REQUIRED_CHECK_IDS: [&str; 6] = [
    "cq.prior",
    "cq.current",
    "case.positive",
    "case.negative",
    "case.ambiguity",
    "case.prior_release",
];
pub const RELEASE_REQUIRED_CHECK_IDS: [&str; 6] = [
    "package.coverage",
    "capability.coverage",
    "receipt.binding",
    "provenance.binding",
    "source.rights",
    "io.binding",
];
const PACKAGE_ASSET_CATEGORIES: [&str; 8] = [
    "ontology",
    "competency_questions",
    "shapes",
    "queries",
    "rules",
    "cases",
    "contract",
    "provenance",
];
const CASE_KINDS: [&str; 4] = ["positive", "negative", "ambiguity", "prior_release"];
type Bindings = BTreeMap<String, String>;
type CaseAsset = (String, String, String);

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}
fn mapping<'a>(value: &'a Value, name: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| anyhow!("{name} must be a mapping"))
}
fn array<'a>(value: &'a Value, name: &str) -> Result<&'a [Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow!("{name} must be a list"))
}
fn text(value: &Value, name: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("{name} must be text"))
}
// _text_tuple deliberately does not strip strings or require uniqueness.
fn text_tuple(value: &Value, name: &str, allow_empty: bool) -> Result<Vec<String>> {
    let items = array(value, name)?;
    ensure!(allow_empty || !items.is_empty(), "{name} must not be empty");
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| anyhow!("{name} entries must be text"))
        })
        .collect()
}
fn digest(value: &Value, name: &str) -> Result<String> {
    let valid = value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    });
    ensure!(valid, "{name} must be a lowercase SHA-256 digest");
    Ok(value.as_str().unwrap().to_owned())
}
fn bindings(value: &Value, name: &str) -> Result<Bindings> {
    mapping(value, name)?
        .iter()
        .map(|(key, value)| Ok((key.clone(), digest(value, name)?)))
        .collect()
}
fn cases(value: &Value) -> Result<Vec<CaseAsset>> {
    array(value, "case_assets")?
        .iter()
        .map(|item| {
            mapping(item, "case asset")?;
            Ok((
                text(field(item, "case_kind"), "case_kind")?,
                text(field(item, "asset_id"), "asset_id")?,
                digest(field(item, "sha256"), "case asset sha256")?,
            ))
        })
        .collect()
}
fn manifest_assets(manifest: &Value) -> Result<&[Value]> {
    mapping(manifest, "manifest")?;
    match manifest.get("assets") {
        None => Ok(&[]),
        Some(value) => array(value, "manifest assets"),
    }
}
fn runs(suite: &Value) -> Result<Vec<&Value>> {
    mapping(suite, "suite")?;
    let runs = array(field(suite, "runs"), "suite runs")?;
    for run in runs {
        mapping(run, "suite run")?;
    }
    Ok(runs.iter().collect())
}
fn check(check_id: &str, passed: bool, yes: &str, no: &str, details: Value) -> Value {
    json!({"check_id":check_id,"passed":passed,
        "message":if passed {yes} else {no},"details":details})
}

/// Original regression evaluator. Inputs use the retained DTO `as_dict()` shape.
pub fn regression(
    check_id: &str,
    contract: &Value,
    suite: &Value,
    manifest: &Value,
) -> Result<Value> {
    mapping(contract, "gate suite check")?;
    let scenario_ids = text_tuple(field(contract, "scenario_ids"), "gate scenario_ids", false)?;
    let mut cq_ids = text_tuple(field(contract, "cq_ids"), "gate cq_ids", true)?;
    let mut case_ids = text_tuple(
        field(contract, "case_asset_ids"),
        "gate case_asset_ids",
        true,
    )?;
    let prior_cq_bindings = if check_id == "cq.prior" {
        bindings(
            field(suite, "required_prior_cq_bindings"),
            "required_prior_cq_bindings",
        )?
    } else {
        Bindings::new()
    };
    let prior_case_bindings = if check_id == "case.prior_release" {
        bindings(
            field(suite, "required_prior_case_bindings"),
            "required_prior_case_bindings",
        )?
    } else {
        Bindings::new()
    };
    if check_id == "cq.prior" {
        cq_ids = prior_cq_bindings.keys().cloned().collect();
    }
    if check_id == "case.prior_release" {
        case_ids = prior_case_bindings.keys().cloned().collect();
    }

    let all_runs = runs(suite)?;
    let mut run_by_id = BTreeMap::new();
    for &run in &all_runs {
        let id = text(field(run, "scenario_id"), "scenario_id")?;
        ensure!(
            run_by_id.insert(id, run).is_none(),
            "suite run must be unique"
        );
    }
    let bound_runs: Vec<_> = scenario_ids
        .iter()
        .map(|id| run_by_id.get(id).copied())
        .collect();
    let mut missing_scenarios: Vec<_> = scenario_ids
        .iter()
        .zip(&bound_runs)
        .filter(|(_, run)| run.is_none())
        .map(|(id, _)| id.clone())
        .collect();
    let mut green_scenarios = Vec::new();
    for &run in bound_runs.iter().flatten() {
        if field(run, "status") == "passed" && field(run, "release_status") == "complete" {
            green_scenarios.push(text(field(run, "scenario_id"), "scenario_id")?);
        }
    }
    // Prior obligations are satisfied by exact input identities anywhere in the
    // complete executed suite, while the contract's scenarios still must pass.
    let coverage_runs: Vec<&Value> = if matches!(check_id, "cq.prior" | "case.prior_release") {
        all_runs
    } else {
        bound_runs.iter().flatten().copied().collect()
    };
    let mut actual_cqs = BTreeSet::new();
    let mut actual_cases = BTreeSet::new();
    for run in coverage_runs {
        actual_cqs.extend(text_tuple(field(run, "cq_ids"), "suite run cq_ids", true)?);
        actual_cases.extend(cases(field(run, "case_assets"))?);
    }
    let cq_registry = bindings(field(suite, "cq_registry_bindings"), "cq_registry_bindings")?;
    let actual_cq_bindings: Bindings = actual_cqs
        .iter()
        .filter_map(|id| cq_registry.get(id).map(|sha| (id.clone(), sha.clone())))
        .collect();
    let mut manifest_cases = BTreeMap::new();
    for item in manifest_assets(manifest)? {
        if item.is_object() && field(item, "category") == "cases" {
            manifest_cases.insert(
                text(field(item, "asset_id"), "asset_id")?,
                (
                    text(field(item, "case_kind"), "case_kind")?,
                    text(field(item, "sha256"), "sha256")?,
                ),
            );
        }
    }
    let required_cqs: BTreeSet<_> = cq_ids.iter().cloned().collect();
    let missing_cqs: Vec<_> = required_cqs.difference(&actual_cqs).cloned().collect();
    let missing_cq_bindings: Vec<_> = prior_cq_bindings
        .iter()
        .filter(|(id, sha)| actual_cq_bindings.get(*id) != Some(*sha))
        .map(|(id, sha)| json!({"cq_id":id,"sha256":sha}))
        .collect();
    let mut missing_cases = Vec::new();
    for id in &case_ids {
        let present = if let Some(required_digest) = prior_case_bindings.get(id) {
            // Prior case identity is its immutable asset id and exact bytes;
            // the old case-kind label is deliberately not an extra condition.
            actual_cases
                .iter()
                .any(|(_, current, sha)| current == id && sha == required_digest)
        } else if let Some((kind, sha)) = manifest_cases.get(id) {
            actual_cases.contains(&(kind.clone(), id.clone(), sha.clone()))
        } else {
            false
        };
        if !present {
            missing_cases.push(id.clone());
        }
    }
    let green_set: BTreeSet<_> = green_scenarios.iter().cloned().collect();
    let required_scenarios: BTreeSet<_> = scenario_ids.iter().cloned().collect();
    let passed = missing_scenarios.is_empty()
        && green_set == required_scenarios
        && missing_cqs.is_empty()
        && missing_cq_bindings.is_empty()
        && missing_cases.is_empty();
    green_scenarios.sort();
    missing_scenarios.sort();
    let actual_case_assets: Vec<_> = actual_cases
        .into_iter()
        .map(|(kind, id, sha)| json!({"case_kind":kind,"asset_id":id,"sha256":sha}))
        .collect();
    Ok(check(
        check_id,
        passed,
        "contract-bound scenarios, CQs, and case inputs passed",
        "contract-bound execution output is incomplete or blocked",
        json!({"scenario_ids":scenario_ids,"green_scenario_ids":green_scenarios,
            "missing_scenario_ids":missing_scenarios,"required_cq_ids":cq_ids,
            "actual_cq_ids":actual_cqs,"missing_cq_ids":missing_cqs,
            "required_prior_cq_bindings":prior_cq_bindings,"actual_cq_bindings":actual_cq_bindings,
            "missing_prior_cq_bindings":missing_cq_bindings,"required_case_asset_ids":case_ids,
            "required_prior_case_bindings":prior_case_bindings,"actual_case_assets":actual_case_assets,
            "missing_case_asset_ids":missing_cases}),
    ))
}

/// Exact original helper, including eight-family then four-kind message order.
pub fn package_coverage_violations(manifest: &Value) -> Vec<String> {
    let Some(assets) = manifest.get("assets").and_then(Value::as_array) else {
        return vec!["package manifest assets are missing".into()];
    };
    let categories: BTreeSet<_> = assets
        .iter()
        .filter(|item| item.is_object())
        .filter_map(|item| field(item, "category").as_str())
        .collect();
    let mut violations: Vec<_> = PACKAGE_ASSET_CATEGORIES
        .iter()
        .filter(|category| !categories.contains(**category))
        .map(|category| format!("package is missing {category} assets"))
        .collect();
    let kinds: BTreeSet<_> = assets
        .iter()
        .filter(|item| item.is_object() && field(item, "category") == "cases")
        .filter_map(|item| field(item, "case_kind").as_str())
        .collect();
    for kind in CASE_KINDS {
        if !kinds.contains(kind) {
            violations.push(format!("package is missing {kind} case"));
        }
    }
    violations
}

// Validate the Engine's ephemeral, independently reconstructed scenario table.
// This is not obtained by projecting or copying the closure being checked.
fn expected_scenarios(value: &Value, all_runs: &[&Value]) -> Result<Value> {
    let items = array(value, "independent provenance scenarios")?;
    let fields: BTreeSet<_> = [
        "scenario_id",
        "input_asset_hashes",
        "case_assets",
        "output_hashes",
        "run_result_object_sha256",
        "receipt_sha256",
        "receipt_object_sha256",
        "native_provenance_bundle_sha256",
    ]
    .into_iter()
    .collect();
    let mut expected = BTreeMap::new();
    for item in items {
        let object = mapping(item, "independent provenance scenario")?;
        ensure!(
            object.keys().map(String::as_str).collect::<BTreeSet<_>>() == fields,
            "provenance scenario binding is incomplete"
        );
        let id = text(field(item, "scenario_id"), "provenance scenario_id")?;
        bindings(field(item, "input_asset_hashes"), "input_asset_hashes")?;
        bindings(field(item, "output_hashes"), "output_hashes")?;
        cases(field(item, "case_assets"))?;
        for name in [
            "run_result_object_sha256",
            "receipt_sha256",
            "receipt_object_sha256",
            "native_provenance_bundle_sha256",
        ] {
            digest(field(item, name), name)?;
        }
        ensure!(
            expected.insert(id, item).is_none(),
            "closure scenario_id must be unique"
        );
    }
    ensure!(
        expected.len() == all_runs.len(),
        "independent provenance scenario set differs from suite"
    );
    for &run in all_runs {
        let id = text(field(run, "scenario_id"), "scenario_id")?;
        let scenario = expected
            .get(&id)
            .ok_or_else(|| anyhow!("independent provenance scenario is missing"))?;
        for name in [
            "case_assets",
            "run_result_object_sha256",
            "receipt_sha256",
            "receipt_object_sha256",
        ] {
            ensure!(
                field(scenario, name) == field(run, name),
                "independent provenance scenario differs from suite:{name}"
            );
        }
    }
    // _closure_scenario_bindings always sorts its independently rebuilt DTOs.
    Ok(Value::Array(expected.into_values().cloned().collect()))
}

/// Original release evaluator, in the original six-check order.
///
/// `regression_results` is an Engine-computed map from check id to the full
/// output of `regression`; it must never be accepted from an API caller.
/// `provenance_evidence` is the Engine-only ephemeral value
/// `{ "bindings": {asset_id: digest}, "scenarios": [complete scenario DTOs] }`.
/// The Engine rereads named source-evidence JSON and checks its exact schema,
/// fields and source_evidence against delta (refinery.py:4946-4994), and rebuilds
/// scenarios from actual retained result/receipt bytes (:5035-5081).
/// `closure` is the separately reread and integrity-validated execution closure.
pub fn release(
    delta: &Value,
    suite: &Value,
    manifest: &Value,
    projection: &Value,
    regression_results: &Value,
    provenance_evidence: &Value,
    closure: &Value,
) -> Result<Vec<Value>> {
    let coverage_violations = package_coverage_violations(manifest);
    let package_coverage = coverage_violations.is_empty();
    let required_capabilities = text_tuple(
        field(delta, "required_capabilities"),
        "required_capabilities",
        true,
    )?;
    let observed_capabilities = text_tuple(
        field(suite, "observed_capabilities"),
        "observed_capabilities",
        true,
    )?;
    let required_set: BTreeSet<_> = required_capabilities.iter().cloned().collect();
    let observed_set: BTreeSet<_> = observed_capabilities.iter().cloned().collect();
    let missing_capabilities: Vec<_> = required_set.difference(&observed_set).cloned().collect();
    let all_runs = runs(suite)?;
    let receipt_binding = field(suite, "status") == "complete"
        && !all_runs.is_empty()
        && all_runs.iter().all(|run| {
            field(run, "status") == "passed" && field(run, "release_status") == "complete"
        });

    let provenance_ids = text_tuple(
        field(projection, "provenance_evidence_asset_ids"),
        "provenance_evidence_asset_ids",
        false,
    )?;
    let provenance = bindings(
        field(provenance_evidence, "bindings"),
        "provenance_evidence bindings",
    )?;
    let mut manifest_by_id = BTreeMap::new();
    for item in manifest_assets(manifest)? {
        if item.is_object() {
            manifest_by_id.insert(text(field(item, "asset_id"), "asset_id")?, item);
        }
    }
    let mut named_provenance = Bindings::new();
    for id in &provenance_ids {
        let item = manifest_by_id
            .get(id)
            .filter(|item| field(item, "category") == "provenance")
            .ok_or_else(|| anyhow!("named provenance-evidence asset is absent"))?;
        named_provenance.insert(
            id.clone(),
            digest(field(item, "sha256"), "provenance evidence sha256")?,
        );
    }
    ensure!(
        provenance == named_provenance,
        "provenance-evidence asset does not exactly bind delta source_evidence"
    );
    let closure_sha = digest(field(closure, "closure_sha256"), "closure_sha256")?;
    let suite_closure_sha = digest(
        field(suite, "provenance_closure_sha256"),
        "provenance_closure_sha256",
    )?;
    let provenance_binding =
        if receipt_binding && !provenance.is_empty() && closure_sha == suite_closure_sha {
            let scenarios = expected_scenarios(field(provenance_evidence, "scenarios"), &all_runs)?;
            field(closure, "scenarios") == &scenarios
                && bindings(
                    field(closure, "provenance_evidence_assets"),
                    "closure provenance_evidence_assets",
                )? == provenance
        } else {
            false
        };

    let rights_ids = text_tuple(
        field(projection, "rights_evidence_asset_ids"),
        "rights_evidence_asset_ids",
        false,
    )?;
    let mut rights_bindings = Vec::new();
    for id in &rights_ids {
        if let Some(item) = manifest_by_id.get(id) {
            rights_bindings.push(json!({"asset_id":id,"sha256":field(item,"sha256"),"category":field(item,"category")}));
        }
    }
    let rights_bound = !rights_ids.is_empty()
        && rights_bindings.len() == rights_ids.len()
        && rights_bindings
            .iter()
            .all(|item| field(item, "category") == "provenance");
    let mut regression_checks = Map::new();
    for (id, value) in mapping(regression_results, "regression results")? {
        let passed = field(value, "passed")
            .as_bool()
            .ok_or_else(|| anyhow!("regression result passed must be boolean"))?;
        regression_checks.insert(id.clone(), Value::Bool(passed));
    }
    let io_binding = regression_checks
        .values()
        .all(|value| value == &Value::Bool(true));
    let run_result_objects: Vec<_> = all_runs
        .iter()
        .map(|run| field(run, "run_result_object_sha256").clone())
        .collect();
    let receipt_objects: Vec<_> = all_runs
        .iter()
        .map(|run| field(run, "receipt_object_sha256").clone())
        .collect();
    let receipt_shas: Vec<_> = all_runs
        .iter()
        .map(|run| field(run, "receipt_sha256").clone())
        .collect();
    let provenance_assets: Vec<_> = provenance
        .iter()
        .map(|(id, sha)| json!({"asset_id":id,"sha256":sha}))
        .collect();
    Ok(vec![
        check(
            "package.coverage",
            package_coverage,
            "all eight asset families and four case kinds are content-bound",
            "package asset-family or case-kind coverage is incomplete",
            json!({"violations":coverage_violations}),
        ),
        check(
            "capability.coverage",
            missing_capabilities.is_empty(),
            "the Semantica execution profile covers required capabilities",
            "required runtime capabilities are missing",
            json!({"required":required_capabilities,"observed":observed_capabilities,"missing":missing_capabilities}),
        ),
        check(
            "receipt.binding",
            receipt_binding,
            "full runner results and native receipts bind every scenario",
            "one or more runner results or native receipts are blocked",
            json!({"suite_status":field(suite,"status"),"run_result_object_sha256":run_result_objects,"receipt_object_sha256":receipt_objects}),
        ),
        check(
            "provenance.binding",
            provenance_binding,
            "package provenance assets and native provenance bundles are bound",
            "package or native execution provenance is incomplete",
            json!({"provenance_evidence_assets":provenance_assets,"receipt_sha256":receipt_shas,
                "execution_provenance_closure_sha256":suite_closure_sha,"legal_ruling":"not_inferred"}),
        ),
        check(
            "source.rights",
            rights_bound,
            "named rights-evidence bytes are content-bound; no legal ruling is inferred",
            "named rights-evidence bytes are absent or unbound",
            json!({"rights_evidence":rights_bindings,"legal_ruling":"not_inferred"}),
        ),
        check(
            "io.binding",
            io_binding,
            "actual CQ and case I/O is bound to the fixed regression suite",
            "actual CQ or case I/O does not satisfy the fixed suite",
            json!({"regression_checks":regression_checks}),
        ),
    ])
}
