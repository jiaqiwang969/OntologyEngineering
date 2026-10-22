//! Reuse current Project focus, reconciliation and dependency owners for copy.
use super::correspondence::{artifact, nodes, read};
use anyhow::{Context, Result, ensure};
use oe_cad::evidence::correspondence::component_copy;
use oe_contracts::{ArtifactReference, CadAssemblyCopyRequest1, CadAssemblyUseSource1};
use oe_core::{SkillRoot, sha256, write_new, write_new_bytes};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

fn endpoint(
    root: &SkillRoot,
    reference: &ArtifactReference,
    input: &CadAssemblyUseSource1,
    native_sha256: &str,
) -> Result<(Vec<u8>, Project)> {
    let raw = artifact(root, reference)?;
    let p: Project = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    project::cad_focus::bind_cad_focus(&raw, &input.object, &input.project_context)?;
    ensure!(
        p.evidence.values().any(|s| s == &input.job.sha256)
            && p.evidence
                .get(&input.native_result_ref)
                .is_some_and(|s| s == native_sha256),
        "cad_copy_inspection_not_in_project"
    );
    Ok((raw, p))
}

pub(super) fn run(root: &SkillRoot, input: &Path, output: &Path) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_assembly_copy")?;
    let raw = read(root, input)?;
    let request: CadAssemblyCopyRequest1 =
        serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let checked = component_copy::verify(root, &request)?;
    let digest = |name: &str| {
        checked.report()[name]["native_result_sha256"]
            .as_str()
            .context("cad_copy_inspection_digest_missing")
    };
    let (prior_raw, previous) = endpoint(
        root,
        &request.previous_project,
        &request.before,
        digest("before")?,
    )?;
    let (current_raw, current) = endpoint(
        root,
        &request.project,
        &request.assembly,
        digest("assembly")?,
    )?;
    ensure!(
        previous.project_id == current.project_id,
        "cad_copy_project_identity_mismatch"
    );
    for digest in [
        request.copy_job.sha256.as_str(),
        checked.report()["copy_intent"]["sha256"]
            .as_str()
            .context("cad_copy_intent_digest_missing")?,
    ] {
        ensure!(
            current.evidence.values().any(|s| s == digest),
            "cad_copy_source_not_in_project"
        );
    }
    ensure!(
        current
            .evidence
            .get(&request.copy_native_result_ref)
            .is_some_and(
                |s| checked.report()["copy_native_result_sha256"].as_str() == Some(s.as_str())
            ),
        "cad_copy_native_result_not_in_project"
    );
    let seeds = checked.impact_seed_ids();
    let known: BTreeSet<_> = nodes(&previous).union(&nodes(&current)).cloned().collect();
    ensure!(
        seeds.iter().all(|s| known.contains(s)),
        "cad_copy_project_dependency_missing"
    );
    let reconciliation = project::reconcile(&previous, &current)?;
    let prior_impact = project::impact(&previous, &seeds)?;
    let current_impact = project::impact(&current, &seeds)?;
    let mut requires_review = BTreeSet::new();
    for report in [&reconciliation, &prior_impact, &current_impact] {
        for d in report["decisions"]
            .as_array()
            .context("cad_copy_impact_decisions_missing")?
        {
            requires_review.insert(
                d["decision_id"]
                    .as_str()
                    .context("cad_copy_impact_decision_id_missing")?
                    .to_owned(),
            );
        }
    }
    let rechecked = component_copy::verify(root, &request)?;
    ensure!(
        checked.report() == rechecked.report()
            && rechecked.impact_seed_ids() == seeds
            && read(root, input)? == raw
            && artifact(root, &request.previous_project)? == prior_raw
            && artifact(root, &request.project)? == current_raw,
        "cad_copy_sources_changed_during_project_check"
    );
    let completion = crate::v2::require_execution(root, "cad_assembly_copy_completion")?;
    for key in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            execution[key] == completion[key],
            "cad_copy_execution_identity_changed"
        );
    }
    let report = json!({"schema":"oev2.current-cad-assembly-copy/1","request":request,"input_sha256":sha256(&raw),
        "previous_project_sha256":sha256(&prior_raw),"project_sha256":sha256(&current_raw),"copy":checked.report(),
        "project_reconciliation":reconciliation,"previous_dependency_impact":prior_impact,"current_dependency_impact":current_impact,
        "requires_review":requires_review,"unknown_dependencies":"unlisted links remain unverified",
        "project_write":"not_run","native_dispatch":"not_run; original source-bound copy and inspections checked",
        "engineering_adoption":"not_established","execution_binding":execution,"execution_recheck":completion});
    let directory = root.new_private_dir(output)?;
    write_new_bytes(&directory.join("input.json"), &raw)?;
    write_new(&directory.join("report.json"), &report)?;
    Ok((report, 0))
}
