//! Fresh execution evidence attached to the existing full migration inventory.
//! Case observation is never whole API compatibility or release eligibility.
use crate::{FixedExperiment, command};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    CodeBehaviorWitness, CodeScopeBinding, CodeScopeItemEvidence, CodeScopeWitnessDispatch,
    CodeScopeWitnessReport, CodeScopeWitnessRequest,
};
use oe_core::{SkillRoot, process::run_command, write_new};
use oe_wire::json::strict_json;
use serde_json::Value;
use std::{collections::BTreeSet, path::Path, time::Duration};

pub const SCOPE_SHA: &str = "b1470d9980755811c815be24cdc041740de5f55e28232c327f0845d160455226";
pub const LOCK_SHA: &str = "d874aecfa20e3753ea85725c0a9b24e2f4795e4b54df819b52a31ac56b93e1dc";
pub const ORACLE_SHA: &str = "07c0481d530ff868ca777ae49dc19803bcb0523347baf6b06b7e5ca243be30fe";
pub const PROTOCOL_SHA: &str = "12cf2e3bcd171ac1000373257553dc0202d9bc42a7d8ff46f910338158ea2809";
pub const DENOMINATOR: usize = 13_494;

pub mod q40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Profile {
    Legacy,
    Q40,
}
impl Profile {
    fn request(schema: &str) -> Result<Self> {
        match schema {
            "oev2.code-scope-witness-request/1" => Ok(Self::Legacy),
            "oev2.code-scope-witness-request/2" => Ok(Self::Q40),
            _ => anyhow::bail!("scope_witness_request_schema"),
        }
    }
    fn permitted(self, scope: &str, case: &str) -> bool {
        match self {
            Self::Legacy => permitted(scope, case),
            Self::Q40 => q40::permitted(scope, case),
        }
    }
    fn report_schema(self) -> &'static str {
        match self {
            Self::Legacy => "oev2.code-scope-witness-report/1",
            Self::Q40 => "oev2.code-scope-witness-observation/2",
        }
    }
    fn oracle(self) -> &'static str {
        match self {
            Self::Legacy => ORACLE_SHA,
            Self::Q40 => q40::ORACLE_SHA,
        }
    }
}

/// Explicit wire versions share the same one-dispatch owner and raw checks.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectedReport {
    Legacy(CodeScopeWitnessReport),
    Q40(oe_contracts::CodeScopeWitnessReport2),
}
impl SelectedReport {
    pub fn observation(&self) -> &CodeScopeWitnessReport {
        match self {
            Self::Legacy(report) => report,
            Self::Q40(report) => &report.observation,
        }
    }
    pub fn value(&self) -> Result<Value> {
        Ok(match self {
            Self::Legacy(report) => serde_json::to_value(report)?,
            Self::Q40(report) => serde_json::to_value(report)?,
        })
    }
    pub fn parse(value: Value) -> Result<Self> {
        match value["schema"].as_str() {
            Some("oev2.code-scope-witness-report/1") => {
                Ok(Self::Legacy(serde_json::from_value(value)?))
            }
            Some("oev2.code-scope-witness-report/2") => {
                Ok(Self::Q40(serde_json::from_value(value)?))
            }
            _ => anyhow::bail!("scope_witness_report_schema"),
        }
    }
    pub fn validate(&self, root: &Path, request: &CodeScopeWitnessRequest) -> Result<()> {
        match (self, Profile::request(&request.schema)?) {
            (Self::Legacy(report), Profile::Legacy) => validate_report(root, report),
            (Self::Q40(report), Profile::Q40) => q40::validate_report(root, report),
            _ => anyhow::bail!("scope_witness_report_request_version_changed"),
        }
    }
}

pub fn validate_request(
    root: &Path,
    request: &CodeScopeWitnessRequest,
    experiment: &oe_contracts::CodeExperiment,
) -> Result<()> {
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("scope_witness_request_contract:{e:?}"))?;
    match Profile::request(&request.schema)? {
        Profile::Legacy => {
            ensure!(
                experiment.scope_sha256 == SCOPE_SHA
                    && experiment.oracle_sha256 == ORACLE_SHA
                    && experiment.protocol_expectations_sha256 == PROTOCOL_SHA
                    && experiment.cases.len() == 6,
                "scope_witness_fixed_profile_changed"
            );
            validate_bindings(root, &request.items)
        }
        Profile::Q40 => {
            q40::validate_experiment(experiment)?;
            q40::validate_bindings(root, &request.items)
        }
    }
}

/// Read the same frozen inventory for an explicitly selected source root.
pub fn inventory(root: &Path) -> Result<Value> {
    let audit = oe_design::migration::audit(root)?;
    ensure!(
        audit["scope_sha256"] == SCOPE_SHA
            && audit["scope_lock_sha256"] == LOCK_SHA
            && audit["items"]
                .as_array()
                .is_some_and(|v| v.len() == DENOMINATOR),
        "scope_witness_original_inventory_changed"
    );
    Ok(audit)
}
fn ids(audit: &Value) -> Result<BTreeSet<String>> {
    audit["items"]
        .as_array()
        .context("scope_witness_inventory_items")?
        .iter()
        .map(|v| {
            v["id"]
                .as_str()
                .map(str::to_owned)
                .context("scope_witness_inventory_id")
        })
        .collect()
}
fn permitted(scope_id: &str, case_id: &str) -> bool {
    // These are fixed case-to-call declarations, not full API equivalence.
    match scope_id {
        "semantica/reasoning/datalog_reasoner.py|method|DatalogReasoner.query|344" => {
            ["datalog-recursive-multivar", "datalog-repeated-variable"].contains(&case_id)
        }
        "semantica/ontology/runtime.py|method|SemanticRuntime.validate_shacl|1831" => {
            case_id == "shacl-missing-required-property"
        }
        _ => false,
    }
}
pub fn validate_bindings(root: &Path, rows: &[CodeScopeBinding]) -> Result<()> {
    validate_bindings_profile(root, rows, Profile::Legacy)
}
fn validate_bindings_profile(
    root: &Path,
    rows: &[CodeScopeBinding],
    profile: Profile,
) -> Result<()> {
    let expected = ids(&inventory(root)?)?;
    let mut seen = BTreeSet::new();
    for row in rows {
        row.validate()
            .map_err(|e| anyhow::anyhow!("scope_witness_binding_contract:{e:?}"))?;
        ensure!(
            seen.insert(row.scope_id.clone()),
            "scope_witness_duplicate_scope_id"
        );
        ensure!(
            expected.contains(&row.scope_id),
            "scope_witness_unknown_scope_id"
        );
        let mut cases = BTreeSet::new();
        for case in &row.case_ids {
            ensure!(
                cases.insert(case) && profile.permitted(&row.scope_id, case),
                "scope_witness_unapproved_case_to_api_binding"
            );
        }
    }
    ensure!(
        seen == expected && rows.len() == DENOMINATOR,
        "scope_witness_missing_scope_id"
    );
    Ok(())
}
pub fn unknown_bindings(root: &Path) -> Result<Vec<CodeScopeBinding>> {
    Ok(ids(&inventory(root)?)?
        .into_iter()
        .map(|scope_id| CodeScopeBinding {
            scope_id,
            case_ids: vec![],
        })
        .collect())
}
pub fn validate_report(root: &Path, report: &CodeScopeWitnessReport) -> Result<()> {
    validate_report_profile(root, report, Profile::Legacy)
}
fn validate_report_profile(
    root: &Path,
    report: &CodeScopeWitnessReport,
    profile: Profile,
) -> Result<()> {
    report
        .validate()
        .map_err(|e| anyhow::anyhow!("scope_witness_report_contract:{e:?}"))?;
    ensure!(
        report.schema == profile.report_schema()
            && report.scope_sha256 == SCOPE_SHA
            && report.scope_lock_sha256 == LOCK_SHA
            && report.fixed_scope_count == DENOMINATOR as u64
            && report.behavior_status == "unknown"
            && report.eligibility == "unknown"
            && report.whole_code_cycle == "not_connected",
        "scope_witness_unearned_report_status"
    );
    let mut seen = BTreeSet::new();
    for item in &report.items {
        ensure!(
            seen.insert(item.scope_id.clone()),
            "scope_witness_duplicate_report_item"
        );
        ensure!(
            item.behavior_status == "unknown",
            "scope_witness_unearned_api_status"
        );
        let mut cases = BTreeSet::new();
        for case in &item.observed_case_ids {
            ensure!(
                report.witness.execution == "completed"
                    && cases.insert(case)
                    && profile.permitted(&item.scope_id, case),
                "scope_witness_unearned_case_observation"
            );
        }
    }
    ensure!(
        seen == ids(&inventory(root)?)? && report.items.len() == DENOMINATOR,
        "scope_witness_missing_report_item"
    );
    Ok(())
}

/// This API mints evidence only from a fresh invocation of the frozen independent
/// evaluator. No user-supplied comparison/witness is accepted as execution truth.
pub fn run(
    fixed: &FixedExperiment,
    request: &CodeScopeWitnessRequest,
    output: &Path,
) -> Result<CodeScopeWitnessReport> {
    ensure!(
        request.schema == "oev2.code-scope-witness-request/1",
        "scope_witness_fixed_profile_changed"
    );
    match run_selected(fixed, request, output)? {
        SelectedReport::Legacy(report) => Ok(report),
        _ => anyhow::bail!("scope_witness_old_entry_version"),
    }
}

pub fn run_selected(
    fixed: &FixedExperiment,
    request: &CodeScopeWitnessRequest,
    output: &Path,
) -> Result<SelectedReport> {
    let profile = Profile::request(&request.schema)?;
    fixed.require()?;
    request
        .validate()
        .map_err(|e| anyhow::anyhow!("scope_witness_request_contract:{e:?}"))?;
    let experiment = fixed.experiment();
    ensure!(
        request.experiment_sha256 == fixed.identity(),
        "scope_witness_fixed_profile_changed"
    );
    ensure!(
        (1..=600_000).contains(&request.dispatch_budget.timeout_ms)
            && (1024..=4 * 1024 * 1024).contains(&request.dispatch_budget.output_bytes),
        "scope_witness_dispatch_budget"
    );
    let root = SkillRoot::new(&experiment.driver.source_root)?;
    validate_request(root.path(), request, experiment)?;
    let folder = root.new_private_dir(output)?;
    let dispatch = CodeScopeWitnessDispatch {
        schema: match profile {
            Profile::Legacy => "oev2.code-scope-witness-dispatch/1",
            Profile::Q40 => "oev2.code-scope-witness-dispatch/2",
        }
        .into(),
        experiment: experiment.clone(),
        experiment_sha256: fixed.identity().into(),
        dispatch_budget: request.dispatch_budget.clone(),
        evidence_path: format!(
            "{}/independent-witness",
            output.to_str().context("scope_witness_output_utf8")?
        ),
    };
    let input = folder.join("dispatch.json");
    write_new(&input, &serde_json::to_value(&dispatch)?)?;
    let process = run_command(
        &command(
            Path::new(&experiment.evaluator.path),
            vec!["witness".into(), input.as_os_str().into()],
            Path::new(&experiment.evaluator.source_root),
            Duration::from_millis(request.dispatch_budget.timeout_ms.saturating_add(30_000)),
            4 * 1024 * 1024,
        ),
        &folder.join("evaluator-dispatch"),
    )?;
    fixed.require()?;
    ensure!(
        process.status == "completed" && process.exit_code == Some(0),
        "scope_witness_independent_evaluator_failed_or_partial"
    );
    let witness: CodeBehaviorWitness = serde_json::from_value(strict_json(&process.stdout)?)?;
    witness
        .validate()
        .map_err(|e| anyhow::anyhow!("scope_witness_observation_contract:{e:?}"))?;
    ensure!(
        witness.schema == "oev2.code-behavior-witness/1"
            && witness.experiment_sha256 == fixed.identity()
            && witness.driver_sha256 == experiment.driver.sha256
            && witness.evaluator_sha256 == experiment.evaluator.sha256
            && witness.oracle_sha256 == profile.oracle()
            && witness.baseline_sha256 == experiment.baseline.manifest_sha256
            && witness.candidate_sha256 == experiment.candidate.manifest_sha256,
        "scope_witness_external_identity_changed"
    );
    let process_raw = oe_core::read_regular_bytes(
        &root.private_path(Path::new(&witness.process.path))?,
        4 * 1024 * 1024,
    )?;
    ensure!(
        oe_core::sha256(&process_raw) == witness.process.sha256,
        "scope_witness_actual_process_artifact_changed"
    );
    let actual_process = strict_json(&process_raw)?;
    ensure!(
        actual_process["executable"]["identity"]["sha256"] == experiment.driver.sha256
            && actual_process["arguments"][0] == "run"
            && (witness.execution != "completed"
                || (actual_process["status"] == "completed" && actual_process["exit_code"] == 0)),
        "scope_witness_actual_driver_process_changed"
    );
    let matched: BTreeSet<String> = if witness.execution == "completed" {
        witness
            .comparison
            .as_ref()
            .map(|comparison| {
                comparison
                    .slots
                    .iter()
                    .filter(|slot| slot["candidate"]["status"] == "matched")
                    .filter_map(|slot| slot["case_id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        BTreeSet::new()
    };
    let items = request
        .items
        .iter()
        .map(|item| CodeScopeItemEvidence {
            scope_id: item.scope_id.clone(),
            observed_case_ids: item
                .case_ids
                .iter()
                .filter(|c| matched.contains(*c))
                .cloned()
                .collect(),
            behavior_status: "unknown".into(),
        })
        .collect();
    let report = CodeScopeWitnessReport {
        schema:profile.report_schema().into(), scope_sha256:SCOPE_SHA.into(),
        scope_lock_sha256:LOCK_SHA.into(), fixed_scope_count:DENOMINATOR as u64,
        witness, items, behavior_status:"unknown".into(), eligibility:"unknown".into(),
        whole_code_cycle:"not_connected".into(),
        uncovered:vec!["All13494 original identities retain unknown whole behavior; observed fixed cases are partial evidence only".into(),
            "All19 original protocol requirements and22 owner gates remain separate incomplete denominators".into(),
            "Shared reserved Value DTO representation and full external/native/feature closure remain unknown".into(),
            "Total engineering cost and industrial effectiveness unknown".into()],
    };
    let report = match profile {
        Profile::Legacy => SelectedReport::Legacy(report),
        Profile::Q40 => SelectedReport::Q40(oe_contracts::CodeScopeWitnessReport2 {
            schema: "oev2.code-scope-witness-report/2".into(),
            catalog_sha256: q40::CATALOG_SHA.into(),
            relations: q40::relations(&report)?,
            observation: report,
        }),
    };
    report.validate(root.path(), request)?;
    write_new(&folder.join("scope-witness-report.json"), &report.value()?)?;
    Ok(report)
}
