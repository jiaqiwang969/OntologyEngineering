//! Actual addition evidence joins the existing correspondence and Project owners.
use super::correspondence::{artifact, nodes, read};
use anyhow::{Context, Result, ensure};
use oe_cad::evidence::correspondence::feature_add;
use oe_contracts::CadFeatureAddRequest1;
use oe_core::{SkillRoot, sha256, write_new, write_new_bytes};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub(super) fn run(root: &SkillRoot, input: &Path, output: &Path) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_feature_add")?;
    let raw = read(root, input)?;
    let request: CadFeatureAddRequest1 = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let checked = feature_add::verify(root, &request)?;
    let corr_raw = artifact(root, &request.correspondence)?;
    let (declared, correspondence_report) =
        super::correspondence::check(root, Path::new(&request.correspondence.path))?;
    ensure!(
        declared.report() == checked.correspondence().report(),
        "cad_add_correspondence_changed"
    );
    let corr = declared.request();
    let previous_raw = artifact(root, &corr.previous.project)?;
    let current_raw = artifact(root, &corr.current.project)?;
    let previous: Project = serde_json::from_value(oe_core::json::strict_json(&previous_raw)?)?;
    let current: Project = serde_json::from_value(oe_core::json::strict_json(&current_raw)?)?;
    ensure!(
        previous.project_id == current.project_id,
        "cad_add_project_identity_differs"
    );
    for (bytes, project, assembly, key) in [
        (
            &previous_raw,
            &previous,
            &request.before_assembly,
            "before_assembly",
        ),
        (&current_raw, &current, &request.assembly, "assembly"),
    ] {
        project::cad_focus::bind_cad_focus(bytes, &assembly.object, &assembly.project_context)?;
        ensure!(
            project.evidence.values().any(|s| s == &assembly.job.sha256)
                && project
                    .evidence
                    .get(&assembly.native_result_ref)
                    .is_some_and(|s| checked.report()[key]["native_result_sha256"].as_str()
                        == Some(s.as_str())),
            "cad_add_assembly_source_not_in_project"
        );
    }
    // The correspondence request already binds the Project. Requiring that
    // Project to contain its request hash would introduce a circular hash.
    for digest in [
        request.add_job.sha256.as_str(),
        checked.report()["add_intent"]["sha256"]
            .as_str()
            .context("cad_add_intent_digest_missing")?,
    ] {
        ensure!(
            current.evidence.values().any(|s| s == digest),
            "cad_add_source_not_in_project"
        );
    }
    ensure!(
        current
            .evidence
            .get(&request.add_native_result_ref)
            .is_some_and(
                |s| checked.report()["add_native_result_sha256"].as_str() == Some(s.as_str())
            ),
        "cad_add_native_result_not_in_project"
    );
    let seeds = checked.impact_seed_ids();
    let known: BTreeSet<_> = nodes(&previous).union(&nodes(&current)).cloned().collect();
    ensure!(
        seeds.iter().all(|s| known.contains(s)),
        "cad_add_project_dependency_missing"
    );
    let reconciliation = project::reconcile(&previous, &current)?;
    let old_impact = project::impact(&previous, &seeds)?;
    let current_impact = project::impact(&current, &seeds)?;
    let mut requires_review = BTreeSet::new();
    for result in [&reconciliation, &old_impact, &current_impact] {
        for d in result["decisions"]
            .as_array()
            .context("cad_add_impact_decisions_missing")?
        {
            requires_review.insert(
                d["decision_id"]
                    .as_str()
                    .context("cad_add_decision_id_missing")?
                    .to_owned(),
            );
        }
    }
    let rechecked = feature_add::verify(root, &request)?;
    ensure!(
        checked.report() == rechecked.report()
            && seeds == rechecked.impact_seed_ids()
            && read(root, input)? == raw
            && artifact(root, &request.correspondence)? == corr_raw
            && artifact(root, &corr.previous.project)? == previous_raw
            && artifact(root, &corr.current.project)? == current_raw,
        "cad_add_sources_changed_during_project_check"
    );
    let completion = crate::v2::require_execution(root, "cad_feature_add_completion")?;
    for key in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            execution[key] == completion[key],
            "cad_add_execution_identity_changed"
        );
    }
    let report = json!({"schema":"oev2.current-cad-feature-add/1","request":request,
        "input_sha256":sha256(&raw),"previous_project_sha256":sha256(&previous_raw),"project_sha256":sha256(&current_raw),
        "addition":checked.report(),"correspondence":correspondence_report,
        "project_reconciliation":reconciliation,"previous_dependency_impact":old_impact,
        "current_dependency_impact":current_impact,"requires_review":requires_review,
        "unknown_dependencies":"unlisted links remain unverified","project_write":"not_run",
        "native_dispatch":"not_run; original addition and independent inspections checked",
        "engineering_adoption":"not_established","execution_binding":execution,"execution_recheck":completion});
    let directory = root.new_private_dir(output)?;
    write_new_bytes(&directory.join("input.json"), &raw)?;
    write_new(&directory.join("report.json"), &report)?;
    Ok((report, 0))
}
