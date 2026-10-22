//! Current Project consumer of the shared source-backed occurrence owner.
use super::correspondence::{artifact, nodes, project_state, read};
use anyhow::{Result, ensure};
use oe_cad::evidence::correspondence::occurrence;
use oe_contracts::CadOccurrenceBindingRequest1;
use oe_core::{SkillRoot, sha256, write_new, write_new_bytes};
use oe_kernel::project;
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn run(root: &SkillRoot, input: &Path, output: &Path) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_occurrence_binding")?;
    let bytes = read(root, input)?;
    let request: CadOccurrenceBindingRequest1 =
        serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
    let checked = occurrence::verify(root, &request)?;
    let (project_bytes, p) = check_project(root, &request, &checked)?;
    let seeds = checked.impact_seed_ids();
    let impact = project::impact(&p, &seeds)?;
    let rechecked = occurrence::verify(root, &request)?;
    ensure!(
        rechecked.report() == checked.report() && rechecked.impact_seed_ids() == seeds,
        "cad_occurrence_sources_changed_during_project_check"
    );
    ensure!(
        read(root, input)? == bytes
            && artifact(root, &request.definition.project)? == project_bytes,
        "cad_occurrence_project_or_request_changed"
    );
    let completion = crate::v2::require_execution(root, "cad_occurrence_binding_completion")?;
    for field in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            execution[field] == completion[field],
            "cad_occurrence_execution_identity_changed"
        );
    }
    let directory = root.new_private_dir(output)?;
    let report = json!({"schema":"oev2.current-cad-occurrence-binding/1","input_sha256":sha256(&bytes),
        "project_sha256":sha256(&project_bytes),"request":request,"occurrence_binding":checked.report(),
        "project_dependency_impact":impact,"project_write":"not_run","native_dispatch":"not_run",
        "engineering_adoption":"not_established","execution_binding":execution,"execution_recheck":completion});
    write_new_bytes(&directory.join("input.json"), &bytes)?;
    write_new(&directory.join("report.json"), &report)?;
    Ok((report, 0))
}

/// Shared actual Project checks for both current use and explicit cross-version use.
/// A caller cannot pair an unrelated request with an already verified capability.
pub(super) fn check_project(
    root: &SkillRoot,
    request: &CadOccurrenceBindingRequest1,
    checked: &occurrence::VerifiedOccurrenceBindings,
) -> Result<(Vec<u8>, project::Project)> {
    ensure!(
        checked.request() == request,
        "cad_occurrence_project_capability_request_differs"
    );
    let (project_bytes, p) = project_state(root, &request.definition)?;
    project::cad_focus::bind_cad_focus(
        &project_bytes,
        &request.assembly.object,
        &request.assembly.project_context,
    )?;
    ensure!(
        p.evidence
            .values()
            .any(|s| s == &request.assembly.job.sha256),
        "cad_occurrence_assembly_job_not_in_project"
    );
    for (name, id) in [
        ("definition", &request.definition.native_result_ref),
        ("assembly", &request.assembly.native_result_ref),
    ] {
        if let Some(digest) = checked.report()[name]["native_result_sha256"].as_str() {
            ensure!(
                p.evidence.get(id).is_some_and(|s| s == digest),
                "cad_occurrence_native_result_not_in_project"
            );
        } else {
            ensure!(
                name == "definition"
                    && checked.report()[name]["native_observation"] == "unavailable",
                "cad_occurrence_native_result_identity_missing"
            );
        }
    }
    let seeds = checked.impact_seed_ids();
    let known = nodes(&p);
    ensure!(
        seeds.iter().all(|id| known.contains(id)),
        "cad_occurrence_project_dependency_missing"
    );
    Ok((project_bytes, p))
}
