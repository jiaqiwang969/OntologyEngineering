//! Recorded CopyComponents lineage under the original source/assembly owners.
//! A new instance never inherits adoption; process-local tags do not persist.
use super::{
    Sources,
    construction::{event, job_directory, job_member, source_ref},
    occurrence, valid_id,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CadAssemblyCopyRequest1, NxAssemblyObservation, NxAssemblyReferenceSets2,
    NxComponentCopyIntent1, NxComponentCopyOutcome1,
};
use oe_core::{SkillRoot, sha256};
use oe_nx_worker::{
    assembly, assembly_reference_sets, event_index,
    geometry::{self, component_copy},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

#[derive(Debug)]
pub struct VerifiedAssemblyCopy {
    request: CadAssemblyCopyRequest1,
    report: Value,
}
impl VerifiedAssemblyCopy {
    pub fn request(&self) -> &CadAssemblyCopyRequest1 {
        &self.request
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn impact_seed_ids(&self) -> BTreeSet<String> {
        BTreeSet::from([
            self.request.assembly.object.object_id.clone(),
            self.request.before.native_result_ref.clone(),
            self.request.assembly.native_result_ref.clone(),
            self.request.copy_native_result_ref.clone(),
        ])
    }
    /// Bounded saved-instance lineage, not a transferable edge tag or approval.
    pub fn require_current_copy(&self, path: &[String]) -> Result<Value> {
        ensure!(
            serde_json::to_value(path)? == self.report["copy"]["new_occurrence_path"],
            "cad_copy_current_instance_not_qualified"
        );
        Ok(self.report["copy"].clone())
    }
}

fn inspected_reference_sets(
    sources: &mut Sources<'_>,
    input: &oe_contracts::CadAssemblyUseSource1,
    assembly: &NxAssemblyObservation,
) -> Result<(oe_nx_protocol::NativeResult, NxAssemblyReferenceSets2)> {
    let directory = job_directory(sources.root, &input.job)?;
    let (native, _) = crate::drawing::associations::receipt(&directory)?;
    let params: geometry::Parameters = serde_json::from_value(oe_core::json::strict_json(
        &job_member(sources, &directory, "params.json")?,
    )?)?;
    ensure!(
        matches!(
            params,
            geometry::Parameters::InspectAssembly {
                capture_reference_sets: Some(true),
                ..
            }
        ),
        "cad_copy_reference_set_capture_required"
    );
    let sets: NxAssemblyReferenceSets2 =
        serde_json::from_value(native.readback["reference_sets"].clone())
            .context("cad_copy_inspected_reference_sets_missing")?;
    assembly_reference_sets::require_entire(&sets, assembly)?;
    Ok((native, sets))
}

pub fn verify(root: &SkillRoot, request: &CadAssemblyCopyRequest1) -> Result<VerifiedAssemblyCopy> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_copy_request_invalid"))?;
    ensure!(
        request.schema == "oev2.cad-assembly-copy/1" && valid_id(&request.copy_native_result_ref),
        "cad_copy_schema_or_source_id_invalid"
    );
    ensure!(
        request.before.object.object_id == request.assembly.object.object_id
            && request.before.object.revision != request.assembly.object.revision,
        "cad_copy_object_or_version_transition_invalid"
    );
    let mut sources = Sources::new(root);
    sources.read(&request.previous_project)?;
    sources.read(&request.project)?;
    sources.read(&request.copy_job)?;
    let prior = occurrence::observed_assembly(&mut sources, &request.before)?;
    let current = occurrence::observed_assembly(&mut sources, &request.assembly)?;
    let (prior_native, prior_sets) =
        inspected_reference_sets(&mut sources, &request.before, &prior.value)?;
    let (inspected, inspection_sets) =
        inspected_reference_sets(&mut sources, &request.assembly, &current.value)?;
    let directory = job_directory(root, &request.copy_job)?;
    let (native, job) = crate::drawing::associations::receipt(&directory)?;
    ensure!(
        native.action == "copy_assembly_component",
        "cad_copy_native_action_differs"
    );
    let process_key =
        |p: &oe_nx_protocol::ProcessIdentity| (p.host_id.clone(), p.pid, p.creation_time_100ns);
    ensure!(
        process_key(&native.process) != process_key(&inspected.process)
            && process_key(&prior_native.process) != process_key(&native.process)
            && process_key(&prior_native.process) != process_key(&inspected.process),
        "cad_copy_independent_process_required"
    );
    let native_raw = job_member(&mut sources, &directory, "native-result.json")?;
    let native_ref = source_ref(
        &sources,
        &directory,
        "native-result.json",
        &sha256(&native_raw),
    )?;
    sources.read(&native_ref)?;
    let params: geometry::Parameters = serde_json::from_value(oe_core::json::strict_json(
        &job_member(&mut sources, &directory, "params.json")?,
    )?)?;
    params.validate()?;
    let geometry::Parameters::CopyAssemblyComponent { intent, budget, .. } = params else {
        anyhow::bail!("cad_copy_native_parameters_differ")
    };
    let intent_name = format!("inputs/{}", intent.path);
    let intent_file = job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == intent_name)
        .context("cad_copy_intent_input_missing")?;
    ensure!(
        intent_file.identity.sha256.as_str() == intent.sha256,
        "cad_copy_intent_input_differs"
    );
    let intent_ref = source_ref(&sources, &directory, &intent_name, &intent.sha256)?;
    let build: NxComponentCopyIntent1 =
        serde_json::from_value(oe_core::json::strict_json(&sources.read(&intent_ref)?)?)?;
    component_copy::validate(&build, &budget)?;
    ensure!(
        build.object.object_id == request.assembly.object.object_id
            && build.object.revision == request.assembly.object.revision
            && build.object.condition_scope == request.assembly.object.condition_scope,
        "cad_copy_object_revision_or_conditions_differ"
    );
    ensure!(
        build.source.root_part == prior.value.root_prototype_id
            && build.source.files.len() == prior.prototypes.len(),
        "cad_copy_original_closure_differs"
    );
    for f in &build.source.files {
        let inspected = prior
            .prototypes
            .get(&f.path)
            .context("cad_copy_original_prototype_missing")?;
        let actual = job
            .inputs
            .iter()
            .find(|a| a.path.as_str() == format!("inputs/{}", f.path))
            .context("cad_copy_source_input_missing")?;
        ensure!(
            inspected["sha256"] == f.sha256
                && inspected["bytes"] == f.bytes
                && actual.identity.sha256.as_str() == f.sha256
                && actual.identity.bytes == f.bytes,
            "cad_copy_original_source_bytes_differ"
        );
    }
    let readback = if native.readback_schema == event_index::READBACK_SCHEMA {
        event_index::receive_readback(
            job.job_id.as_str(),
            &native.readback_schema,
            &native.readback,
            &mut |name| job_member(&mut sources, &directory, name),
        )?
    } else {
        native.readback.clone()
    };
    ensure!(
        readback["action"] == "copy_assembly_component"
            && readback["object"] == serde_json::to_value(&build.object)?
            && readback["intent"] == serde_json::to_value(&intent)?
            && readback["independent_reopen_checked"] == false,
        "cad_copy_readback_identity_differs"
    );
    let mut observations = Vec::new();
    let mut previous_sequence = None;
    for (key, phase) in [
        ("before", "component_copy_before"),
        (
            "before_reference_sets",
            "component_copy_before_reference_sets",
        ),
        ("call", "component_copy_returned"),
        ("after", "component_copy_after"),
        (
            "after_reference_sets",
            "component_copy_after_reference_sets",
        ),
        ("saved", "component_copy_save_returned"),
        ("saved_inventory", "component_copy_saved_inventory"),
        (
            "saved_reference_sets",
            "component_copy_saved_reference_sets",
        ),
    ] {
        let reference: ArtifactReference = serde_json::from_value(readback[key].clone())
            .with_context(|| format!("cad_copy_event_missing:{key}"))?;
        let (sequence, observed) = event(
            &mut sources,
            &directory,
            job.job_id.as_str(),
            &reference,
            phase,
        )?;
        ensure!(
            previous_sequence.is_none_or(|p| p < sequence),
            "cad_copy_event_order_differs"
        );
        previous_sequence = Some(sequence);
        observations.push(observed);
    }
    let before: NxAssemblyObservation = serde_json::from_value(observations[0].clone())?;
    let before_sets: NxAssemblyReferenceSets2 = serde_json::from_value(observations[1].clone())?;
    let call: NxComponentCopyOutcome1 = serde_json::from_value(observations[2].clone())?;
    let after: NxAssemblyObservation = serde_json::from_value(observations[3].clone())?;
    let after_sets: NxAssemblyReferenceSets2 = serde_json::from_value(observations[4].clone())?;
    let save = &observations[5];
    let saved: NxAssemblyObservation = serde_json::from_value(observations[6].clone())?;
    let saved_sets: NxAssemblyReferenceSets2 = serde_json::from_value(observations[7].clone())?;
    let copy = component_copy::check_change(&build, &budget, &before, &call, &after)?;
    component_copy::check_change(&build, &budget, &before, &call, &saved)?;
    component_copy::check_reference_sets(&before, &before_sets, &after, &after_sets, &call)?;
    component_copy::check_reference_sets(&before, &before_sets, &saved, &saved_sets, &call)?;
    assembly_reference_sets::require_entire(&before_sets, &before)?;
    assembly_reference_sets::require_entire(&saved_sets, &saved)?;
    ensure!(
        save["api"] == "UF_PART_save_all"
            && save["return_code"].as_i64() == Some(0)
            && save["file_error_count"].as_u64() == Some(0)
            && save["file_errors"].as_array().is_some_and(Vec::is_empty),
        "cad_copy_save_incomplete"
    );
    assembly::same_geometry(&prior.value, &before)?;
    assembly::same_geometry(&after, &saved)?;
    assembly::same_geometry(&saved, &current.value)?;
    let roots: Vec<_> = native
        .artifacts
        .iter()
        .filter(|a| {
            Path::new(a.path.as_str())
                .file_name()
                .and_then(|s| s.to_str())
                == Some(build.source.root_part.as_str())
        })
        .collect();
    ensure!(
        roots.len() == 1
            && roots[0].identity.sha256.as_str() == request.assembly.object.source_sha256,
        "cad_copy_saved_root_differs"
    );
    let namespace = Path::new(roots[0].path.as_str())
        .parent()
        .context("cad_copy_saved_namespace_missing")?;
    ensure!(
        native.artifacts.len() == current.prototypes.len(),
        "cad_copy_saved_closure_differs"
    );
    for (id, actual) in &current.prototypes {
        let name = namespace.join(id).to_string_lossy().replace('\\', "/");
        let produced = native
            .artifacts
            .iter()
            .find(|a| a.path.as_str() == name)
            .context("cad_copy_saved_prototype_missing")?;
        ensure!(
            actual["sha256"] == produced.identity.sha256.as_str()
                && actual["bytes"] == produced.identity.bytes,
            "cad_copy_saved_prototype_differs"
        );
    }
    let (again, _) = crate::drawing::associations::receipt(&directory)?;
    let prior_again = occurrence::observed_assembly(&mut sources, &request.before)?;
    let current_again = occurrence::observed_assembly(&mut sources, &request.assembly)?;
    ensure!(
        oe_nx_protocol::record_digest(&again)? == oe_nx_protocol::record_digest(&native)?
            && prior_again.report == prior.report
            && current_again.report == current.report,
        "cad_copy_sources_changed_during_check"
    );
    sources.verify_unchanged()?;
    Ok(VerifiedAssemblyCopy {
        request: request.clone(),
        report: json!({
        "schema":"oev2.cad-assembly-copy-report/1","status":"bounded_copy_and_independent_saved_reinspection",
        "before":prior.report,"assembly":current.report,"copy_job":request.copy_job,
        "copy_native_result_ref":request.copy_native_result_ref,"copy_native_result_sha256":native_ref.sha256,
        "copy_intent":intent_ref,"declared_source_refs":build.source_refs,"copy":copy,"native_outcome":call,
        "copy_process":native.process,"previous_inspection_process":prior_native.process,"inspection_process":inspected.process,
        "reference_set_contract":{"previous":prior_sets,"inspection":inspection_sets,"copy_before":before_sets,
            "copy_after":after_sets,"copy_saved":saved_sets},"source_records":sources.report(),
        "independent_reinspection_checked":true,"native_tags_scope":"each original process separately",
        "persistent_topology_identity":"not_established","engineering_adoption":"not_established"}),
    })
}
