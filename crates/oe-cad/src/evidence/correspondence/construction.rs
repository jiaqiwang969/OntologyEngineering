//! Source-bound native creation and independent reinspection of a saved assembly.
//! Native tags link an add only within its creator process. No general copy/delete
//! lineage, persistent topology identity or engineering adoption follows from this.
use super::{SOURCE_BYTES, Sources, occurrence, valid_id};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CadAssemblyConstructionRequest1, CadAssemblyConstructionRequest2,
    NxAssemblyAddOutcome, NxAssemblyBuildIntent, NxAssemblyObservation,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use oe_nx_worker::{
    assembly, event_index,
    geometry::{self, source_assembly},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct VerifiedAssemblyConstruction {
    request: CadAssemblyConstructionRequest1,
    report: Value,
    qualified: BTreeMap<Vec<String>, Value>,
}
impl VerifiedAssemblyConstruction {
    pub fn request(&self) -> &CadAssemblyConstructionRequest1 {
        &self.request
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn impact_seed_ids(&self) -> BTreeSet<String> {
        BTreeSet::from([
            self.request.assembly.object.object_id.clone(),
            self.request.assembly.native_result_ref.clone(),
            self.request.creator_native_result_ref.clone(),
        ])
    }
    /// A checked source chain for one constructed instance, never an edge tag
    /// qualified for a later process or an inherited engineering approval.
    pub fn require_current_instance(&self, path: &[String]) -> Result<Value> {
        self.qualified
            .get(path)
            .cloned()
            .context("cad_construction_instance_not_qualified")
    }
}
pub(super) fn job_member(
    sources: &mut Sources<'_>,
    directory: &Path,
    name: &str,
) -> Result<Vec<u8>> {
    let path = crate::drawing::member(directory, name)?;
    let raw = read_regular_bytes(&path, SOURCE_BYTES)?;
    let reference = ArtifactReference {
        path: path
            .strip_prefix(sources.root.path())?
            .to_string_lossy()
            .replace('\\', "/"),
        sha256: sha256(&raw),
    };
    ensure!(
        sources.read(&reference)? == raw,
        "cad_construction_member_changed"
    );
    Ok(raw)
}
pub(super) fn event(
    sources: &mut Sources<'_>,
    directory: &Path,
    job: &str,
    reference: &ArtifactReference,
    phase: &str,
) -> Result<(u64, Value)> {
    let mut read = |name: &str| job_member(sources, directory, name);
    let mut observation = event_index::receive_event(job, reference, phase, &mut read)?;
    let raw = read(&reference.path)?;
    let record = oe_core::json::strict_json(&raw)?;
    let sequence = record["sequence"]
        .as_u64()
        .context("cad_construction_event_sequence_missing")?;
    if observation["schema"] == event_index::OBSERVATION_SCHEMA {
        observation = event_index::receive_observation(job, &observation, &mut read)?;
    }
    Ok((sequence, observation))
}
pub(super) fn source_ref(
    sources: &Sources<'_>,
    directory: &Path,
    name: &str,
    digest: &str,
) -> Result<ArtifactReference> {
    let path = crate::drawing::member(directory, name)?;
    Ok(ArtifactReference {
        path: path
            .strip_prefix(sources.root.path())?
            .to_string_lossy()
            .replace('\\', "/"),
        sha256: digest.into(),
    })
}
pub(super) fn job_directory(root: &SkillRoot, reference: &ArtifactReference) -> Result<PathBuf> {
    let path = root.expand(Path::new(&reference.path))?;
    ensure!(
        path.file_name().and_then(|s| s.to_str()) == Some("job.json"),
        "cad_construction_job_locator_invalid"
    );
    Ok(path
        .parent()
        .context("cad_construction_job_parent_missing")?
        .to_path_buf())
}
pub fn verify(
    root: &SkillRoot,
    request: &CadAssemblyConstructionRequest1,
) -> Result<VerifiedAssemblyConstruction> {
    verify_profile(root, request, false)
}

#[derive(Debug)]
pub struct VerifiedAssemblyConstruction2 {
    request: CadAssemblyConstructionRequest2,
    checked: VerifiedAssemblyConstruction,
}
impl VerifiedAssemblyConstruction2 {
    pub fn request(&self) -> &CadAssemblyConstructionRequest2 {
        &self.request
    }
    pub fn report(&self) -> &Value {
        self.checked.report()
    }
    pub fn impact_seed_ids(&self) -> BTreeSet<String> {
        self.checked.impact_seed_ids()
    }
    pub fn require_current_instance(&self, path: &[String]) -> Result<Value> {
        self.checked.require_current_instance(path)
    }
}

pub fn verify2(
    root: &SkillRoot,
    request: &CadAssemblyConstructionRequest2,
) -> Result<VerifiedAssemblyConstruction2> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_construction_request_invalid"))?;
    ensure!(
        request.schema == "oev2.cad-assembly-construction/2",
        "cad_construction_schema_or_source_id_invalid"
    );
    // Shared source fields only. The private profile, not this internal carrier's
    // schema, chooses the comparison; the public capability keeps the /2 request.
    let common = CadAssemblyConstructionRequest1 {
        schema: "oev2.cad-assembly-construction/1".into(),
        project: request.project.clone(),
        assembly: request.assembly.clone(),
        creator_job: request.creator_job.clone(),
        creator_native_result_ref: request.creator_native_result_ref.clone(),
    };
    Ok(VerifiedAssemblyConstruction2 {
        request: request.clone(),
        checked: verify_profile(root, &common, true)?,
    })
}

fn verify_profile(
    root: &SkillRoot,
    request: &CadAssemblyConstructionRequest1,
    scoped_reference_sets: bool,
) -> Result<VerifiedAssemblyConstruction> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_construction_request_invalid"))?;
    ensure!(
        request.schema == "oev2.cad-assembly-construction/1"
            && valid_id(&request.creator_native_result_ref),
        "cad_construction_schema_or_source_id_invalid"
    );
    let mut sources = Sources::new(root);
    sources.read(&request.project)?;
    sources.read(&request.creator_job)?;
    let current = occurrence::observed_assembly(&mut sources, &request.assembly)?;
    let directory = job_directory(root, &request.creator_job)?;
    let (native, job) = crate::drawing::associations::receipt(&directory)?;
    ensure!(
        native.action == "create_source_assembly",
        "cad_construction_requires_source_assembly_creator"
    );
    let current_directory = job_directory(root, &request.assembly.job)?;
    let (inspected, _) = crate::drawing::associations::receipt(&current_directory)?;
    let reference_sets = if scoped_reference_sets {
        let params: geometry::Parameters = serde_json::from_value(oe_core::json::strict_json(
            &job_member(&mut sources, &current_directory, "params.json")?,
        )?)?;
        ensure!(
            matches!(
                params,
                geometry::Parameters::InspectAssembly {
                    capture_reference_sets: Some(true),
                    ..
                }
            ),
            "cad_construction_reference_set_capture_required"
        );
        let observed: oe_contracts::NxAssemblyReferenceSets2 =
            serde_json::from_value(inspected.readback["reference_sets"].clone())
                .context("cad_construction_reference_sets_missing")?;
        oe_nx_worker::assembly_reference_sets::require_entire(&observed, &current.value)?;
        Some(observed)
    } else {
        None
    };
    ensure!(
        !(native.process.host_id == inspected.process.host_id
            && native.process.pid == inspected.process.pid
            && native.process.creation_time_100ns == inspected.process.creation_time_100ns),
        "cad_construction_independent_process_required"
    );
    let native_reference = source_ref(
        &sources,
        &directory,
        "native-result.json",
        &sha256(&read_regular_bytes(
            &crate::drawing::member(&directory, "native-result.json")?,
            SOURCE_BYTES,
        )?),
    )?;
    let raw = sources.read(&native_reference)?;
    let reread: oe_nx_protocol::NativeResult = oe_nx_protocol::decode(&raw)?;
    ensure!(
        oe_nx_protocol::record_digest(&reread)? == oe_nx_protocol::record_digest(&native)?,
        "cad_construction_native_changed"
    );
    let params: geometry::Parameters = serde_json::from_value(oe_core::json::strict_json(
        &job_member(&mut sources, &directory, "params.json")?,
    )?)?;
    params.validate()?;
    let geometry::Parameters::CreateSourceAssembly { intent, budget, .. } = params else {
        anyhow::bail!("cad_construction_creator_parameters_differ")
    };
    let intent_name = format!("inputs/{}", intent.path);
    let input = job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == intent_name)
        .context("cad_construction_intent_not_in_manifest")?;
    ensure!(
        input.identity.sha256.as_str() == intent.sha256,
        "cad_construction_intent_identity_differs"
    );
    let intent_ref = source_ref(&sources, &directory, &intent_name, &intent.sha256)?;
    let intent_raw = sources.read_limit(&intent_ref, 32 * 1024 * 1024)?;
    let build: NxAssemblyBuildIntent =
        serde_json::from_value(oe_core::json::strict_json(&intent_raw)?)?;
    source_assembly::validate(&build, &budget)?;
    ensure!(
        build.object.object_id == request.assembly.object.object_id
            && build.object.revision == request.assembly.object.revision
            && build.object.condition_scope == request.assembly.object.condition_scope,
        "cad_construction_object_revision_or_conditions_differ"
    );
    for file in &build.files {
        let name = format!("inputs/{}", file.path);
        let actual = job
            .inputs
            .iter()
            .find(|f| f.path.as_str() == name)
            .context("cad_construction_prototype_input_missing")?;
        ensure!(
            actual.identity.sha256.as_str() == file.sha256 && actual.identity.bytes == file.bytes,
            "cad_construction_prototype_input_differs"
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
        readback["action"] == "create_source_assembly"
            && readback["object"] == serde_json::to_value(&build.object)?
            && readback["intent"] == serde_json::to_value(&intent)?
            && readback["save"]["return_code"] == 0
            && readback["save"]["file_error_count"] == 0
            && readback["save"]["file_errors"]
                .as_array()
                .is_some_and(Vec::is_empty),
        "cad_construction_creation_or_save_incomplete"
    );
    let before_ref: ArtifactReference =
        serde_json::from_value(readback["before_save_record"].clone())?;
    let after_ref: ArtifactReference =
        serde_json::from_value(readback["after_save_record"].clone())?;
    let (before_sequence, before) = event(
        &mut sources,
        &directory,
        job.job_id.as_str(),
        &before_ref,
        "source_assembly_before_save",
    )?;
    let (after_sequence, after) = event(
        &mut sources,
        &directory,
        job.job_id.as_str(),
        &after_ref,
        "source_assembly_after_save",
    )?;
    ensure!(
        before_sequence < after_sequence,
        "cad_construction_save_event_order_differs"
    );
    let before: NxAssemblyObservation = serde_json::from_value(before)?;
    let after: NxAssemblyObservation = serde_json::from_value(after)?;
    source_assembly::check_inventory(&build, &budget, &before)?;
    source_assembly::check_inventory(&build, &budget, &after)?;
    source_assembly::check_inventory(&build, &current.budget, &current.value)?;
    assembly::same_geometry(&before, &after)?;
    assembly::same_geometry(&after, &current.value)?;
    let roots: Vec<_> = native
        .artifacts
        .iter()
        .filter(|a| {
            Path::new(a.path.as_str())
                .file_name()
                .and_then(|s| s.to_str())
                == Some(build.root_part.as_str())
        })
        .collect();
    ensure!(
        roots.len() == 1
            && roots[0].identity.sha256.as_str() == request.assembly.object.source_sha256,
        "cad_construction_saved_root_differs"
    );
    let namespace = Path::new(roots[0].path.as_str())
        .parent()
        .context("cad_construction_saved_namespace_missing")?;
    ensure!(
        native.artifacts.len() == current.prototypes.len(),
        "cad_construction_saved_closure_differs"
    );
    for (id, actual) in &current.prototypes {
        let name = namespace.join(id).to_string_lossy().replace('\\', "/");
        let produced = native
            .artifacts
            .iter()
            .find(|a| a.path.as_str() == name)
            .context("cad_construction_saved_prototype_missing")?;
        ensure!(
            actual["sha256"] == produced.identity.sha256.as_str()
                && actual["bytes"] == produced.identity.bytes,
            "cad_construction_saved_prototype_differs"
        );
    }
    let root_tag = before
        .prototypes
        .iter()
        .find(|p| p.prototype_id == before.root_prototype_id)
        .unwrap()
        .native_part_tag;
    let mut additions = BTreeMap::new();
    let mut sequences = BTreeSet::new();
    let mut last_sequence = 0;
    let mut add_order = Vec::new();
    let records = readback["load_status_records"]
        .as_array()
        .context("cad_construction_add_records_missing")?;
    ensure!(records.len() <= 65536, "cad_construction_event_budget");
    for reference in records {
        ensure!(
            reference["schema"] == "cad-agent.nx-native-event-reference/v2"
                && reference["job_id"] == job.job_id.as_str()
                && reference["phase"] == "native_load_status",
            "cad_construction_add_record_identity_differs"
        );
        let identity: oe_nx_protocol::BlobIdentity =
            serde_json::from_value(reference["record"]["identity"].clone())?;
        let name = reference["record"]["path"]
            .as_str()
            .context("cad_construction_add_record_path_missing")?;
        let raw = job_member(&mut sources, &directory, name)?;
        identity.verify(&raw)?;
        let expected_sequence = reference["sequence"]
            .as_u64()
            .context("cad_construction_add_reference_sequence_missing")?;
        let reference = ArtifactReference {
            path: name.into(),
            sha256: identity.sha256.as_str().into(),
        };
        let (sequence, batch) = event(
            &mut sources,
            &directory,
            job.job_id.as_str(),
            &reference,
            "native_load_status",
        )?;
        ensure!(
            sequence == expected_sequence && sequence > last_sequence && sequences.insert(sequence),
            "cad_construction_add_record_reused"
        );
        last_sequence = sequence;
        ensure!(
            batch["schema"] == "oev2.nx-load-diagnostic-batch/1"
                && batch["ordering"] == "native_emission_order",
            "cad_construction_add_batch_invalid"
        );
        let items = batch["observations"]
            .as_array()
            .context("cad_construction_add_batch_missing")?;
        let actual_bytes = items
            .iter()
            .try_fold(0usize, |used, value| -> Result<usize> {
                used.checked_add(serde_json::to_vec(value)?.len())
                    .context("cad_construction_add_byte_account")
            })?;
        ensure!(
            batch["observation_bytes"].as_u64() == Some(u64::try_from(actual_bytes)?),
            "cad_construction_add_byte_account"
        );
        for value in items {
            if value["schema"] != "cad-agent.nxopen-entire-loaded-add/v1" {
                continue;
            }
            ensure!(
                sequence < before_sequence
                    && value["wrapper_code"] == 0
                    && value["phase"] == 3
                    && value["exception_code"] == 0
                    && value["direct_owner_matches"] == 1
                    && value["entire_set_returned"] == 1
                    && value["builder_destroyed"] == 1,
                "cad_construction_native_add_incomplete"
            );
            let outcome: NxAssemblyAddOutcome = serde_json::from_value(value["outcome"].clone())?;
            let selected: Vec<_> = before
                .occurrences
                .iter()
                .filter(|o| o.part_occurrence_tag == outcome.component_tag)
                .collect();
            ensure!(
                selected.len() == 1,
                "cad_construction_added_instance_missing_or_ambiguous"
            );
            let occurrence = selected[0];
            let prototype = before
                .prototypes
                .iter()
                .find(|p| p.prototype_id == occurrence.prototype_id)
                .unwrap();
            assembly::check_nxopen_add(
                &outcome,
                u32::try_from(root_tag)?,
                u32::try_from(prototype.native_part_tag)?,
            )?;
            ensure!(
                outcome.definition_instance_tag == occurrence.instance_tag
                    && if scoped_reference_sets {
                        // NXOpen's observed vendor EntirePartRefsetName was
                        // checked above. UF's documented sentinel is separate.
                        occurrence.reference_set == "None"
                    } else {
                        outcome.reference_set == occurrence.reference_set
                    }
                    && additions
                        .insert(
                            occurrence.name.clone(),
                            json!({"sequence":sequence,"outcome":outcome})
                        )
                        .is_none(),
                "cad_construction_added_instance_reused_or_definition_differs"
            );
            add_order.push(occurrence.name.clone());
        }
    }
    ensure!(
        additions.len() == before.occurrences.len(),
        "cad_construction_add_coverage_incomplete"
    );
    ensure!(
        add_order
            == build
                .placements
                .iter()
                .map(|p| p.occurrence_path[0].clone())
                .collect::<Vec<_>>(),
        "cad_construction_add_order_differs_from_intent"
    );
    let qualified=build.placements.iter().map(|p| {
        let creation=&additions[&p.occurrence_path[0]];
        (p.occurrence_path.clone(),json!({"occurrence_path":p.occurrence_path,"prototype_id":p.prototype_id,
            "placement":p.local_to_root_m,"creation":creation,"geometry_basis":"existing complete assembly inventory and persistence comparison",
            "persistent_topology_identity":"not_established","copy_delete_lineage":"not_established","engineering_adoption":"not_established"}))
    }).collect::<BTreeMap<_,_>>();
    let (creator_again, _) = crate::drawing::associations::receipt(&directory)?;
    let current_again = occurrence::observed_assembly(&mut sources, &request.assembly)?;
    ensure!(
        oe_nx_protocol::record_digest(&creator_again)? == oe_nx_protocol::record_digest(&native)?
            && current_again.report == current.report,
        "cad_construction_sources_changed_during_check"
    );
    sources.verify_unchanged()?;
    let mut report = json!({"schema":"oev2.cad-assembly-construction-report/1","status":"bounded_creation_and_independent_reinspection",
        "assembly":current.report,"creator_job":request.creator_job,"creator_native_result_ref":request.creator_native_result_ref,
        "creator_native_result_sha256":native_reference.sha256,"creator_process":native.process,"inspection_process":inspected.process,
        "construction_intent":intent_ref,"declared_source_refs":build.source_refs,"creator_declared_object":build.object,
        "instances":qualified.values().collect::<Vec<_>>(),"source_records":sources.report(),
        "independent_reinspection_checked":true,"native_tags_scope":"creator and inspector separately; never persistent IDs",
        "copy_delete_lineage":"not_established","persistent_topology_identity":"not_established","engineering_adoption":"not_established"});
    if let Some(reference_sets) = reference_sets {
        report["schema"] = json!("oev2.cad-assembly-construction-report/2");
        report["reference_set_contract"] = json!({
            "schema":"oev2.cad-assembly-reference-set-contract/2",
            "nxopen":"actual outcome must equal observed vendor EntirePartRefsetName",
            "uf":"raw None means entire part; definition and actual part occurrence checked separately",
            "inspection":reference_sets,"legacy1":"unchanged exact string comparison"
        });
    }
    Ok(VerifiedAssemblyConstruction {
        request: request.clone(),
        report,
        qualified,
    })
}
