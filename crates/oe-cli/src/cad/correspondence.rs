//! Reuse CAD observation checks and the existing Project change owner.
use anyhow::{Context, Result, ensure};
use oe_cad::evidence::correspondence::{self, VerifiedCorrespondence};
use oe_contracts::{
    ArtifactReference, CadCorrespondenceRequest1, CadCorrespondenceState1, CadFeatureIntent1,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub(super) fn read(root: &SkillRoot, path: &Path) -> Result<Vec<u8>> {
    let path = root.expand(path)?;
    ensure!(
        path.starts_with(root.path()),
        "cad_correspondence_input_outside_root"
    );
    oe_core::no_symlinks(&path)?;
    read_regular_bytes(&path, 16 * 1024 * 1024)
}

pub(super) fn artifact(root: &SkillRoot, reference: &ArtifactReference) -> Result<Vec<u8>> {
    let raw = read(root, Path::new(&reference.path))?;
    ensure!(
        sha256(&raw) == reference.sha256,
        "cad_correspondence_project_source_drift"
    );
    Ok(raw)
}

pub(super) fn project_state(
    root: &SkillRoot,
    state: &CadCorrespondenceState1,
) -> Result<(Vec<u8>, Project)> {
    let raw = artifact(root, &state.project)?;
    let project: Project = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    project::cad_focus::bind_cad_focus(&raw, &state.object, &state.project_context)?;
    let in_project = |source: &ArtifactReference| -> Result<()> {
        ensure!(
            project
                .evidence
                .values()
                .any(|digest| digest == &source.sha256),
            "cad_correspondence_source_not_in_project:{}",
            source.path
        );
        Ok(())
    };
    in_project(&state.job)?;
    let declared_nodes = nodes(&project);
    for binding in &state.bindings {
        in_project(&binding.intent.source)?;
        let source = oe_core::json::strict_json(&artifact(root, &binding.intent.source)?)?;
        let intent: CadFeatureIntent1 = serde_json::from_value(
            source
                .pointer(&binding.intent.pointer)
                .context("cad_correspondence_intent_pointer_missing")?
                .clone(),
        )?;
        ensure!(
            intent
                .dependency_ids
                .iter()
                .all(|id| declared_nodes.contains(id)),
            "cad_correspondence_dependency_missing_from_endpoint_project"
        );
    }
    Ok((raw, project))
}

pub(super) fn nodes(project: &Project) -> BTreeSet<String> {
    project
        .objects
        .keys()
        .chain(project.conditions.keys())
        .chain(project.evidence.keys())
        .chain(project.dependencies.keys())
        .cloned()
        .chain(
            project
                .dependencies
                .values()
                .flat_map(|ids| ids.iter().cloned()),
        )
        .chain(
            project
                .decisions
                .iter()
                .flat_map(|d| std::iter::once(d.id.clone()).chain(d.basis.clone())),
        )
        .collect()
}

/// Returns a freshly checked capability; a saved report cannot recreate it.
pub(super) fn check(root: &SkillRoot, input: &Path) -> Result<(VerifiedCorrespondence, Value)> {
    let bytes = read(root, input)?;
    let request: CadCorrespondenceRequest1 =
        serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
    let checked = correspondence::verify(root, &request)?;
    let (previous_raw, previous) = project_state(root, &request.previous)?;
    let (current_raw, current) = project_state(root, &request.current)?;
    for (name, state, project) in [
        ("previous", &request.previous, &previous),
        ("current", &request.current, &current),
    ] {
        let observation = &checked.report()[name];
        let digest = observation
            .get("native_result_sha256")
            .context("cad_correspondence_native_identity_field_missing")?;
        match observation["native_observation"].as_str() {
            Some("source_bound_completed_inspection") => {
                let digest = digest
                    .as_str()
                    .context("cad_correspondence_native_identity_missing")?;
                ensure!(
                    project
                        .evidence
                        .get(&state.native_result_ref)
                        .is_some_and(|v| v == digest),
                    "cad_correspondence_native_observation_not_in_project"
                );
            }
            Some("unavailable") => ensure!(
                digest.is_null() && observation["native_failure"].is_string(),
                "cad_correspondence_unavailable_observation_must_retain_failure"
            ),
            _ => anyhow::bail!("cad_correspondence_observation_state_invalid"),
        }
    }
    for change in &request.changes {
        // A new change declaration can postdate the preserved old Project.
        ensure!(
            current
                .evidence
                .values()
                .any(|s| s == &change.source.sha256),
            "cad_correspondence_change_source_not_in_current_project"
        );
    }
    let known: BTreeSet<_> = nodes(&previous).union(&nodes(&current)).cloned().collect();
    let seeds = checked.impact_seed_ids();
    ensure!(
        seeds.iter().all(|id| known.contains(id)),
        "cad_correspondence_undeclared_project_dependency"
    );
    let reconciliation = project::reconcile(&previous, &current)?;
    let old_impact = project::impact(&previous, &seeds)?;
    let current_impact = project::impact(&current, &seeds)?;
    let mut requires_review = BTreeSet::new();
    for view in [&reconciliation, &old_impact, &current_impact] {
        for item in view["decisions"]
            .as_array()
            .context("cad_correspondence_impact_decisions_missing")?
        {
            requires_review.insert(
                item["decision_id"]
                    .as_str()
                    .context("cad_correspondence_decision_id_missing")?
                    .to_owned(),
            );
        }
    }
    // Project interpretation must not outlive the source closure it reviewed.
    let rechecked = correspondence::verify(root, &request)?;
    ensure!(
        rechecked.report() == checked.report() && rechecked.impact_seed_ids() == seeds,
        "cad_correspondence_sources_changed_during_project_review"
    );
    ensure!(
        read(root, input)? == bytes
            && artifact(root, &request.previous.project)? == previous_raw
            && artifact(root, &request.current.project)? == current_raw,
        "cad_correspondence_inputs_changed_during_check"
    );
    let result = json!({"schema":"oev2.current-cad-correspondence/1",
        "input_sha256":sha256(&bytes), "previous_project_sha256":sha256(&previous_raw),
        "current_project_sha256":sha256(&current_raw), "correspondence":checked.report(),
        "project_reconciliation":reconciliation, "previous_dependency_impact":old_impact,
        "current_dependency_impact":current_impact, "requires_review":requires_review,
        "unknown_dependencies":"unlisted links remain unverified", "project_write":"not_run",
        "native_tool_execution":"not_run; existing source-bound receipts checked",
        "engineering_adoption":"not_established"});
    Ok((checked, result))
}

pub(super) fn run(root: &SkillRoot, input: &Path, output: &Path) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_correspondence")?;
    let (checked, mut report) = check(root, input)?;
    let raw = read(root, input)?;
    ensure!(
        sha256(&raw) == report["input_sha256"],
        "cad_correspondence_request_changed"
    );
    let completion = crate::v2::require_execution(root, "cad_correspondence_completion")?;
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
    let output = root.new_private_dir(output)?;
    report["execution_binding"] = execution;
    report["execution_recheck"] = completion;
    report["request"] = serde_json::to_value(checked.request())?;
    write_new_bytes(&output.join("input.json"), &raw)?;
    write_new(&output.join("report.json"), &report)?;
    Ok((report, 0))
}

/// Optional drawing /2 consumer: all original drawing checks still apply.
pub(super) fn drawing_check(
    root: &SkillRoot,
    input: &Path,
    project_bytes: &[u8],
    intent: &oe_contracts::DrawingReferenceIntent,
    observation: &oe_contracts::NativeDrawingObservation,
) -> Value {
    let attempt = (|| -> Result<Value> {
        ensure!(
            intent.schema == "oev2.drawing-reference-intent/3",
            "cad_correspondence_drawing_profile_requires_separate_circle_tolerances"
        );
        let (checked, report) = check(root, input)?;
        ensure!(
            report["current_project_sha256"] == sha256(project_bytes),
            "cad_correspondence_drawing_project_changed"
        );
        ensure!(
            !intent.points.is_empty(),
            "cad_correspondence_drawing_points_missing"
        );
        let mut bindings = Vec::new();
        for point in &intent.points {
            let selector = point
                .parent_edge_v2
                .as_ref()
                .context("cad_correspondence_drawing_circle_selector_required")?;
            let current = checked.require_current_feature(
                &selector.feature_id,
                &intent.object,
                &observation.native_result_sha256,
            )?;
            ensure!(
                current["selector"] == serde_json::to_value(selector)?,
                "cad_correspondence_drawing_selector_differs"
            );
            bindings.push(current);
        }
        Ok(
            json!({"conclusion":"pass","input_sha256":report["input_sha256"],
            "current_project_sha256":report["current_project_sha256"],"bindings":bindings,
            "project_reconciliation":report["project_reconciliation"],
            "requires_review":report["requires_review"],"engineering_adoption":"not_established"}),
        )
    })();
    match attempt {
        Ok(value) => value,
        Err(error) => {
            json!({"conclusion":"unknown","reason":"cad_correspondence_current_drawing_unbound",
            "error":error.to_string(),"engineering_adoption":"not_established"})
        }
    }
}
