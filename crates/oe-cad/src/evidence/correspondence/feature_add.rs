//! Receive actual feature addition without changing declaration-only correspondence.
//! The existing Project caller still decides the affected engineering obligations.
use super::{
    Sources, VerifiedCorrespondence,
    construction::{event, job_directory, job_member, source_ref},
    occurrence, valid_id,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CadCorrespondenceRequest1, CadFeatureAddRequest1, NxAssemblyObservation,
    NxAssemblyReferenceSets2, NxFeatureAddIntent1, NxFeatureAddOutcome1, NxFeatureInventory1,
};
use oe_core::{SkillRoot, sha256};
use oe_nx_worker::{
    assembly, assembly_reference_sets, event_index,
    geometry::{self, feature_add},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug)]
pub struct VerifiedFeatureAdd {
    request: CadFeatureAddRequest1,
    correspondence: VerifiedCorrespondence,
    report: Value,
}
impl VerifiedFeatureAdd {
    pub fn request(&self) -> &CadFeatureAddRequest1 {
        &self.request
    }
    pub fn correspondence(&self) -> &VerifiedCorrespondence {
        &self.correspondence
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn impact_seed_ids(&self) -> BTreeSet<String> {
        let mut ids = self.correspondence.impact_seed_ids();
        ids.extend([
            self.request.before_assembly.object.object_id.clone(),
            self.request.assembly.object.object_id.clone(),
            self.request.before_assembly.native_result_ref.clone(),
            self.request.assembly.native_result_ref.clone(),
            self.request.add_native_result_ref.clone(),
        ]);
        ids
    }
}

fn inspection(
    sources: &mut Sources<'_>,
    input: &oe_contracts::CadAssemblyUseSource1,
    observed: &NxAssemblyObservation,
) -> Result<(oe_nx_protocol::NativeResult, NxAssemblyReferenceSets2)> {
    let dir = job_directory(sources.root, &input.job)?;
    let (native, _) = crate::drawing::associations::receipt(&dir)?;
    let params: geometry::Parameters = serde_json::from_value(oe_core::json::strict_json(
        &job_member(sources, &dir, "params.json")?,
    )?)?;
    ensure!(
        matches!(
            params,
            geometry::Parameters::InspectAssembly {
                capture_reference_sets: Some(true),
                ..
            }
        ),
        "cad_add_reference_set_capture_required"
    );
    let sets: NxAssemblyReferenceSets2 =
        serde_json::from_value(native.readback["reference_sets"].clone())?;
    assembly_reference_sets::require_entire(&sets, observed)?;
    Ok((native, sets))
}

pub fn verify(root: &SkillRoot, request: &CadFeatureAddRequest1) -> Result<VerifiedFeatureAdd> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_add_request_invalid"))?;
    ensure!(
        request.schema == "oev2.cad-feature-add/1"
            && valid_id(&request.add_native_result_ref)
            && valid_id(&request.change_id),
        "cad_add_schema_or_id_invalid"
    );
    let mut sources = Sources::new(root);
    let corr: CadCorrespondenceRequest1 = serde_json::from_value(oe_core::json::strict_json(
        &sources.read(&request.correspondence)?,
    )?)?;
    let correspondence = super::verify(root, &corr)?;
    ensure!(
        corr.previous.backend == "nx"
            && corr.current.backend == "nx"
            && corr.previous.object.object_id == corr.current.object.object_id
            && corr.previous.object.revision != corr.current.object.revision
            && corr.previous.object.condition_scope == corr.current.object.condition_scope,
        "cad_add_definition_transition_invalid"
    );
    let prior_definition = super::state(&mut sources, &corr.previous)?;
    let current_definition = super::state(&mut sources, &corr.current)?;
    let prior_part = prior_definition
        .observation
        .as_ref()
        .context("cad_add_previous_detail_required")?;
    let current_part = current_definition
        .observation
        .as_ref()
        .context("cad_add_current_detail_required")?;
    let prior = occurrence::observed_assembly(&mut sources, &request.before_assembly)?;
    let current = occurrence::observed_assembly(&mut sources, &request.assembly)?;
    let (prior_native, prior_sets) =
        inspection(&mut sources, &request.before_assembly, &prior.value)?;
    let (inspected, inspection_sets) = inspection(&mut sources, &request.assembly, &current.value)?;
    ensure!(
        request.before_assembly.object.object_id == request.assembly.object.object_id
            && request.before_assembly.object.revision != request.assembly.object.revision,
        "cad_add_assembly_transition_invalid"
    );
    sources.read(&request.add_job)?;
    let directory = job_directory(root, &request.add_job)?;
    let (native, job) = crate::drawing::associations::receipt(&directory)?;
    ensure!(
        native.action == "add_hole_feature",
        "cad_add_native_action_differs"
    );
    let process_key =
        |p: &oe_nx_protocol::ProcessIdentity| (p.host_id.clone(), p.pid, p.creation_time_100ns);
    for input in [&corr.previous, &corr.current] {
        let (n, _) = crate::drawing::associations::receipt(&job_directory(root, &input.job)?)?;
        ensure!(
            process_key(&native.process) != process_key(&n.process),
            "cad_add_independent_detail_process_required"
        );
    }
    ensure!(
        process_key(&native.process) != process_key(&prior_native.process)
            && process_key(&native.process) != process_key(&inspected.process)
            && process_key(&prior_native.process) != process_key(&inspected.process),
        "cad_add_independent_assembly_process_required"
    );
    let native_raw = job_member(&mut sources, &directory, "native-result.json")?;
    let native_ref = source_ref(
        &sources,
        &directory,
        "native-result.json",
        &sha256(&native_raw),
    )?;
    let params: geometry::Parameters = serde_json::from_value(oe_core::json::strict_json(
        &job_member(&mut sources, &directory, "params.json")?,
    )?)?;
    params.validate()?;
    let geometry::Parameters::AddHoleFeature { intent, budget, .. } = params else {
        anyhow::bail!("cad_add_parameters_differ")
    };
    let name = format!("inputs/{}", intent.path);
    let input = job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == name)
        .context("cad_add_intent_input_missing")?;
    ensure!(
        input.identity.sha256.as_str() == intent.sha256,
        "cad_add_intent_input_differs"
    );
    let intent_ref = source_ref(&sources, &directory, &name, &intent.sha256)?;
    let build: NxFeatureAddIntent1 =
        serde_json::from_value(oe_core::json::strict_json(&sources.read(&intent_ref)?)?)?;
    feature_add::validate(&build, &budget)?;
    ensure!(
        build.object == corr.previous.object,
        "cad_add_original_definition_differs"
    );
    let declared = correspondence.report()["changes"]
        .as_array()
        .context("cad_add_changes_missing")?;
    let selected: Vec<_> = declared
        .iter()
        .filter(|c| c["change_id"] == request.change_id)
        .collect();
    ensure!(
        selected.len() == 1 && selected[0]["declared_kind"] == "new",
        "cad_add_original_declaration_missing"
    );
    let binding_id = selected[0]["declaration"]["after_bindings"][0]["binding_id"]
        .as_str()
        .context("cad_add_original_binding_missing")?;
    let binding = current_definition
        .bindings
        .get(binding_id)
        .context("cad_add_original_binding_missing")?;
    ensure!(
        binding.status == "unique"
            && binding.intent.occurrence.is_none()
            && binding.intent.selector == build.target,
        "cad_add_original_selector_differs"
    );
    ensure!(
        build.source.root_part == prior.value.root_prototype_id
            && build.source.files.len() == prior.prototypes.len(),
        "cad_add_original_closure_differs"
    );
    for f in &build.source.files {
        let actual = job
            .inputs
            .iter()
            .find(|x| x.path.as_str() == format!("inputs/{}", f.path))
            .context("cad_add_source_input_missing")?;
        let before = prior
            .prototypes
            .get(&f.path)
            .context("cad_add_original_prototype_missing")?;
        ensure!(
            actual.identity.sha256.as_str() == f.sha256
                && actual.identity.bytes == f.bytes
                && before["sha256"] == f.sha256
                && before["bytes"] == f.bytes,
            "cad_add_original_source_differs"
        );
    }
    ensure!(
        prior
            .prototypes
            .get(&build.target_part)
            .is_some_and(|f| f["sha256"] == corr.previous.object.source_sha256)
            && current
                .prototypes
                .get(&build.target_part)
                .is_some_and(|f| f["sha256"] == corr.current.object.source_sha256),
        "cad_add_definition_assembly_source_differs"
    );
    let readback = if native.readback_schema == event_index::READBACK_SCHEMA {
        event_index::receive_readback(
            job.job_id.as_str(),
            &native.readback_schema,
            &native.readback,
            &mut |p| job_member(&mut sources, &directory, p),
        )?
    } else {
        native.readback.clone()
    };
    ensure!(
        readback["action"] == "add_hole_feature"
            && readback["target_part"] == build.target_part
            && readback["intent"] == serde_json::to_value(&intent)?
            && readback["object"] == serde_json::to_value(&build.object)?
            && readback["independent_reopen_checked"] == false,
        "cad_add_readback_identity_differs"
    );
    let mut values = BTreeMap::new();
    let mut last = None;
    for (key, phase) in [
        ("before", "feature_add_before"),
        ("before_reference_sets", "feature_add_before_reference_sets"),
        ("before_detail", "feature_add_before_detail"),
        ("before_features", "feature_add_before_features"),
        ("call", "feature_add_returned"),
        ("after_detail", "feature_add_after_detail"),
        ("after", "feature_add_after"),
        ("after_features", "feature_add_after_features"),
        ("lineage", "feature_add_lineage"),
        ("after_reference_sets", "feature_add_after_reference_sets"),
        ("saved", "feature_add_save_returned"),
        ("saved_detail", "feature_add_saved_detail"),
        ("saved_features", "feature_add_saved_features"),
        ("saved_inventory", "feature_add_saved_inventory"),
        ("saved_reference_sets", "feature_add_saved_reference_sets"),
    ] {
        let reference: ArtifactReference = serde_json::from_value(readback[key].clone())
            .with_context(|| format!("cad_add_event_missing:{key}"))?;
        let (sequence, value) = event(
            &mut sources,
            &directory,
            job.job_id.as_str(),
            &reference,
            phase,
        )?;
        ensure!(
            last.is_none_or(|p| p < sequence),
            "cad_add_event_order_differs"
        );
        last = Some(sequence);
        values.insert(key, value);
    }
    let before: NxAssemblyObservation = serde_json::from_value(values["before"].clone())?;
    let after: NxAssemblyObservation = serde_json::from_value(values["after"].clone())?;
    let saved: NxAssemblyObservation = serde_json::from_value(values["saved_inventory"].clone())?;
    let inventory: NxFeatureInventory1 = serde_json::from_value(values["before_features"].clone())?;
    let after_features: NxFeatureInventory1 =
        serde_json::from_value(values["after_features"].clone())?;
    let saved_features: NxFeatureInventory1 =
        serde_json::from_value(values["saved_features"].clone())?;
    let call: NxFeatureAddOutcome1 = serde_json::from_value(values["call"].clone())?;
    let prototype = before
        .prototypes
        .iter()
        .find(|p| p.prototype_id == build.target_part)
        .context("cad_add_current_target_missing")?;
    ensure!(
        prototype.solids.len() == 1
            && prototype.native_part_tag == inventory.part_tag
            && prototype.solids[0].body_tag == inventory.body_tag,
        "cad_add_inventory_owner_differs"
    );
    let selection = feature_add::check_lineage(
        &build,
        &inventory,
        &values["before_detail"],
        &after_features,
        &values["after_detail"],
        &call,
    )?;
    ensure!(values["lineage"] == selection, "cad_add_lineage_differs");
    let saved_lineage = feature_add::check_lineage(
        &build,
        &inventory,
        &values["before_detail"],
        &saved_features,
        &values["saved_detail"],
        &call,
    )?;
    ensure!(saved_lineage == selection, "cad_add_saved_lineage_differs");
    // Each detailed read must belong to the target observed at that same
    // mutation stage. Independent reinspection has its own process-local tags.
    for (stage, observed) in [
        ("before_detail", &before),
        ("after_detail", &after),
        ("saved_detail", &saved),
    ] {
        let target = observed
            .prototypes
            .iter()
            .find(|p| p.prototype_id == build.target_part)
            .context("cad_add_current_target_missing")?;
        ensure!(
            target.solids.len() == 1
                && values[stage]["part_tag"].as_u64() == Some(target.native_part_tag)
                && values[stage]["solid_tags"] == json!([target.solids[0].body_tag]),
            "cad_add_stage_detail_owner_differs"
        );
    }
    for (part, added) in [
        (&prior_part.part, false),
        (&values["before_detail"], false),
        (&values["after_detail"], true),
        (&values["saved_detail"], true),
        (&current_part.part, true),
    ] {
        feature_add::check_geometry(&build, part, added)?;
    }
    feature_add::check_assembly(&build, &budget, &before, &after)?;
    feature_add::check_assembly(&build, &budget, &before, &saved)?;
    assembly::same_geometry(&prior.value, &before)?;
    assembly::same_geometry(&after, &saved)?;
    assembly::same_geometry(&saved, &current.value)?;
    let mut sets = Vec::new();
    for (key, observed) in [
        ("before_reference_sets", &before),
        ("after_reference_sets", &after),
        ("saved_reference_sets", &saved),
    ] {
        let s: NxAssemblyReferenceSets2 = serde_json::from_value(values[key].clone())?;
        assembly_reference_sets::require_entire(&s, observed)?;
        sets.push(s);
    }
    ensure!(
        serde_json::to_value(&sets[0])? == serde_json::to_value(&sets[1])?
            && serde_json::to_value(&sets[0])? == serde_json::to_value(&sets[2])?,
        "cad_add_reference_sets_changed"
    );
    let save = &values["saved"];
    ensure!(
        save["api"] == "UF_PART_save_all"
            && save["return_code"].as_i64() == Some(0)
            && save["file_error_count"].as_u64() == Some(0)
            && save["file_errors"].as_array().is_some_and(Vec::is_empty),
        "cad_add_save_incomplete"
    );
    let artifact_paths: BTreeSet<_> = native
        .artifacts
        .iter()
        .map(|a| a.path.as_str().to_owned())
        .collect();
    let root_relative = Path::new(&build.source.root_part);
    let roots: Vec<_> = native
        .artifacts
        .iter()
        .filter_map(|a| {
            let path = Path::new(a.path.as_str());
            if !path.ends_with(root_relative) {
                return None;
            }
            let namespace = path.ancestors().nth(root_relative.components().count())?;
            let expected: BTreeSet<_> = current
                .prototypes
                .keys()
                .map(|id| namespace.join(id).to_string_lossy().replace('\\', "/"))
                .collect();
            // The complete relative closure identifies the output namespace;
            // a nested component may legitimately share the root's leaf name.
            (expected == artifact_paths).then_some((a, namespace))
        })
        .collect();
    ensure!(
        roots.len() == 1
            && roots[0].0.identity.sha256.as_str() == request.assembly.object.source_sha256,
        "cad_add_saved_root_differs"
    );
    let namespace = roots[0].1;
    ensure!(
        native.artifacts.len() == current.prototypes.len(),
        "cad_add_saved_closure_differs"
    );
    for (id, actual) in &current.prototypes {
        let name = namespace.join(id).to_string_lossy().replace('\\', "/");
        let output = native
            .artifacts
            .iter()
            .find(|a| a.path.as_str() == name)
            .context("cad_add_saved_prototype_missing")?;
        ensure!(
            actual["sha256"] == output.identity.sha256.as_str()
                && actual["bytes"] == output.identity.bytes,
            "cad_add_saved_prototype_differs"
        );
    }
    let (again, _) = crate::drawing::associations::receipt(&directory)?;
    let corr_again = super::verify(root, &corr)?;
    let prior_again = occurrence::observed_assembly(&mut sources, &request.before_assembly)?;
    let current_again = occurrence::observed_assembly(&mut sources, &request.assembly)?;
    ensure!(
        oe_nx_protocol::record_digest(&native)? == oe_nx_protocol::record_digest(&again)?
            && correspondence.report() == corr_again.report()
            && prior.report == prior_again.report
            && current.report == current_again.report,
        "cad_add_sources_changed_during_check"
    );
    sources.verify_unchanged()?;
    Ok(VerifiedFeatureAdd {
        request: request.clone(),
        correspondence,
        report: json!({"schema":"oev2.cad-feature-add-report/1", "status":"bounded_addition_and_independent_saved_reinspection",
            "before_assembly":prior.report, "assembly":current.report, "add_job":request.add_job,
            "add_native_result_ref":request.add_native_result_ref,"add_native_result_sha256":native_ref.sha256,
            "add_intent":intent_ref,"added_feature_id":build.target.feature_id,"change_id":request.change_id,
            "current_target_mapping":"unique_in_complete_reopened_geometry",
            "native_outcome":call,"lineage":selection,"add_process":native.process,
            "previous_inspection_process":prior_native.process,"inspection_process":inspected.process,
            "reference_set_contract":{"previous":prior_sets,"current":inspection_sets,"add":sets},
            "source_records":sources.report(),"independent_reinspection_checked":true,
            "declaration_contract":"original correspondence/1 remains unchanged",
            "persistent_topology_identity":"not_established","engineering_adoption":"not_established"}),
    })
}
