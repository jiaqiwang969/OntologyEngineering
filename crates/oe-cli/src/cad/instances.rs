//! Explicit instance correspondence with the original Project change owner.
use super::{
    correspondence::{artifact, nodes, read},
    occurrence::check_project,
};
use anyhow::{Context, Result, ensure};
use oe_cad::evidence::correspondence::instances;
use oe_contracts::CadInstanceCorrespondenceRequest2;
use oe_core::{SkillRoot, sha256, write_new, write_new_bytes};
use oe_kernel::project;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub(super) fn run(root: &SkillRoot, input: &Path, output: &Path) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_instance_correspondence")?;
    let raw = read(root, input)?;
    let request: CadInstanceCorrespondenceRequest2 =
        serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let checked = instances::verify(root, &request)?;
    let (previous_raw, previous) =
        check_project(root, &request.previous, checked.previous_bindings())?;
    let (current_raw, current) = check_project(root, &request.current, checked.current_bindings())?;
    ensure!(
        previous.project_id == current.project_id,
        "cad_instance_correspondence_project_identity_mismatch"
    );
    for source in &request.changes {
        ensure!(
            current
                .evidence
                .values()
                .any(|digest| digest == &source.source.sha256),
            "cad_instance_correspondence_current_project_missing_change_source"
        );
    }
    let known: BTreeSet<_> = nodes(&previous).union(&nodes(&current)).cloned().collect();
    let seeds = checked.impact_seed_ids();
    ensure!(
        seeds.iter().all(|id| known.contains(id)),
        "cad_correspondence_undeclared_project_dependency"
    );
    let reconciliation = project::reconcile(&previous, &current)?;
    let previous_impact = project::impact(&previous, &seeds)?;
    let current_impact = project::impact(&current, &seeds)?;
    let mut requires_review = BTreeSet::new();
    for view in [&reconciliation, &previous_impact, &current_impact] {
        for row in view["decisions"]
            .as_array()
            .context("cad_correspondence_impact_decisions_missing")?
        {
            requires_review.insert(
                row["decision_id"]
                    .as_str()
                    .context("cad_correspondence_decision_id_missing")?
                    .to_owned(),
            );
        }
    }
    let rechecked = instances::verify(root, &request)?;
    ensure!(
        rechecked.report() == checked.report() && rechecked.impact_seed_ids() == seeds,
        "cad_instance_correspondence_sources_changed_during_project_review"
    );
    ensure!(
        read(root, input)? == raw
            && artifact(root, &request.previous.definition.project)? == previous_raw
            && artifact(root, &request.current.definition.project)? == current_raw,
        "cad_correspondence_inputs_changed_during_check"
    );
    let completion = crate::v2::require_execution(root, "cad_instance_correspondence_completion")?;
    for key in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            execution[key] == completion[key],
            "cad_correspondence_execution_identity_changed"
        );
    }
    let report = json!({"schema":"oev2.current-cad-correspondence/2","request":request,"input_sha256":sha256(&raw),
        "previous_project_sha256":sha256(&previous_raw),"current_project_sha256":sha256(&current_raw),
        "correspondence":checked.report(),"project_reconciliation":reconciliation,
        "previous_dependency_impact":previous_impact,"current_dependency_impact":current_impact,
        "requires_review":requires_review,"unknown_dependencies":"unlisted links remain unverified",
        "project_write":"not_run","native_tool_execution":"not_run; existing source-bound receipts checked",
        "engineering_adoption":"not_established","execution_binding":execution,"execution_recheck":completion});
    let output = root.new_private_dir(output)?;
    write_new_bytes(&output.join("input.json"), &raw)?;
    write_new(&output.join("report.json"), &report)?;
    Ok((report, 0))
}
