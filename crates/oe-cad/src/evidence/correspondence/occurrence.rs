//! Source-backed instance use of a prototype feature in one observed assembly.
//! Reuses correspondence sources, original circle checks, receipt and assembly
//! owners. Derived root geometry is not a native occurrence-edge observation.
use super::*;
use oe_contracts::{
    CadAssemblyUseSource1, CadOccurrenceBindingRequest1, NxAssemblyBudget, NxAssemblyMotionSample,
    NxAssemblyObservation,
};
use oe_nx_worker::{
    assembly::{self, Rigid},
    geometry::{self, model_edges},
};

#[derive(Debug)]
pub struct VerifiedOccurrenceBindings {
    request: CadOccurrenceBindingRequest1,
    report: Value,
    qualified: BTreeMap<String, Value>,
    seeds: BTreeSet<String>,
}
impl VerifiedOccurrenceBindings {
    pub fn request(&self) -> &CadOccurrenceBindingRequest1 {
        &self.request
    }
    pub fn report(&self) -> &Value {
        &self.report
    }
    pub fn impact_seed_ids(&self) -> BTreeSet<String> {
        self.seeds.clone()
    }
    pub fn require_current_feature(
        &self,
        feature_id: &str,
        assembly: &ObjectIdentity,
        assembly_native_sha256: &str,
        definition_native_sha256: &str,
    ) -> Result<Value> {
        ensure!(
            *assembly == self.request.assembly.object,
            "cad_occurrence_current_assembly_differs"
        );
        ensure!(
            self.report["assembly"]["native_result_sha256"] == assembly_native_sha256
                && self.report["definition"]["native_result_sha256"] == definition_native_sha256,
            "cad_occurrence_current_observation_differs"
        );
        self.qualified
            .get(feature_id)
            .cloned()
            .context("cad_occurrence_feature_unresolved_or_ambiguous")
    }
}

pub(super) struct ObservedAssembly {
    pub(super) value: NxAssemblyObservation,
    pub(super) budget: NxAssemblyBudget,
    pub(super) prototypes: BTreeMap<String, Value>,
    pub(super) native_sha256: String,
    pub(super) report: Value,
}
pub(super) fn observed_assembly(
    sources: &mut Sources<'_>,
    input: &CadAssemblyUseSource1,
) -> Result<ObservedAssembly> {
    input
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_occurrence_assembly_source_invalid"))?;
    input
        .object
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_occurrence_assembly_object_invalid"))?;
    input
        .project_context
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_occurrence_assembly_context_invalid"))?;
    ensure!(
        valid_id(&input.native_result_ref),
        "cad_occurrence_native_reference_invalid"
    );
    sources.read(&input.job)?;
    let job_path = sources.root.expand(Path::new(&input.job.path))?;
    ensure!(
        job_path.file_name().and_then(|v| v.to_str()) == Some("job.json"),
        "cad_occurrence_assembly_job_locator_invalid"
    );
    let directory = job_path
        .parent()
        .context("cad_occurrence_assembly_job_parent_missing")?;
    let (native, job) = crate::drawing::associations::receipt(directory)?;
    ensure!(
        native.action == "inspect_assembly",
        "cad_occurrence_requires_assembly_inspection"
    );
    let raw = read_regular_bytes(
        &crate::drawing::member(directory, "params.json")?,
        SOURCE_BYTES,
    )?;
    let params: geometry::Parameters = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    params.validate()?;
    let geometry::Parameters::InspectAssembly {
        root_part,
        budget,
        source_role,
        ..
    } = params
    else {
        anyhow::bail!("cad_occurrence_assembly_parameters_differ")
    };
    ensure!(
        source_role == "reference_observation"
            && native.readback["source_role"] == source_role
            && native.readback["save_requested"] == false,
        "cad_occurrence_assembly_observation_role_differs"
    );
    let root_input = format!("inputs/{root_part}");
    ensure!(
        input.part_input == root_input,
        "cad_occurrence_assembly_root_input_differs"
    );
    let root_file = job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == root_input)
        .context("cad_occurrence_assembly_root_source_missing")?;
    ensure!(
        root_file.identity.sha256.as_str() == input.object.source_sha256,
        "cad_occurrence_assembly_object_source_differs"
    );
    let value: NxAssemblyObservation = serde_json::from_value(native.readback["assembly"].clone())?;
    assembly::validate_observation(&value, &budget)?;
    ensure!(
        value.source_role == source_role,
        "cad_occurrence_assembly_source_role_differs"
    );
    let root_name = Path::new(&root_part)
        .file_name()
        .and_then(|v| v.to_str())
        .context("cad_occurrence_root_leaf_missing")?;
    ensure!(
        value.root_prototype_id == root_name,
        "cad_occurrence_native_root_differs"
    );
    let namespace = Path::new(&root_input)
        .parent()
        .context("cad_occurrence_input_namespace_missing")?;
    let root = value
        .prototypes
        .iter()
        .find(|p| p.prototype_id == value.root_prototype_id)
        .unwrap();
    let root_path = root.part_path.replace('\\', "/");
    let native_namespace = root_path
        .rsplit_once('/')
        .context("cad_occurrence_native_namespace_missing")?
        .0;
    let mut prototypes = BTreeMap::new();
    for p in &value.prototypes {
        oe_nx_protocol::RelativePath::new(p.prototype_id.clone())?;
        let path = namespace
            .join(&p.prototype_id)
            .to_string_lossy()
            .replace('\\', "/");
        let file = job
            .inputs
            .iter()
            .find(|f| f.path.as_str() == path)
            .context("cad_occurrence_prototype_source_missing")?;
        ensure!(
            p.part_path.replace('\\', "/") == format!("{native_namespace}/{}", p.prototype_id),
            "cad_occurrence_native_prototype_namespace_differs"
        );
        // The shared receipt reader has already read and verified every byte.
        prototypes.insert(p.prototype_id.clone(),json!({"job_input":path,"sha256":file.identity.sha256.as_str(),"bytes":file.identity.bytes}));
    }
    let native_raw = read_regular_bytes(
        &crate::drawing::member(directory, "native-result.json")?,
        SOURCE_BYTES,
    )?;
    let native_sha256 = sha256(&native_raw);
    let reread: oe_nx_protocol::NativeResult = oe_nx_protocol::decode(&native_raw)?;
    ensure!(
        oe_nx_protocol::record_digest(&reread)? == oe_nx_protocol::record_digest(&native)?,
        "cad_occurrence_native_changed_during_check"
    );
    let report = json!({"object":input.object,"project_context":input.project_context,"job":input.job,
        "part_input":input.part_input,"native_result_ref":input.native_result_ref,"native_result_sha256":native_sha256,
        "assembly":value,"prototype_sources":prototypes,"observation":"source_bound_completed_assembly_inspection"});
    Ok(ObservedAssembly {
        value,
        budget,
        prototypes,
        native_sha256,
        report,
    })
}

fn binding(
    binding: &Binding,
    part: &NativeDrawingObservation,
    assembly: &ObservedAssembly,
) -> Result<Value> {
    let target = binding
        .intent
        .occurrence
        .as_ref()
        .context("cad_occurrence_explicit_instance_required")?;
    let resolved = oe_nx_worker::assembly_motion::resolve_targets(
        &assembly.value,
        &assembly.budget,
        &NxAssemblyMotionSample {
            sample_id: "current-feature-binding".into(),
            time_s: 0.,
            targets: vec![target.clone()],
        },
    )?;
    ensure!(resolved.len() == 1, "cad_occurrence_resolution_incomplete");
    let nodes: BTreeMap<_, _> = assembly
        .value
        .occurrences
        .iter()
        .map(|o| (o.occurrence_id.as_str(), o))
        .collect();
    let o = nodes
        .get(resolved[0].occurrence_id.as_str())
        .context("cad_occurrence_resolved_node_missing")?;
    let transform = Rigid::checked(&o.local_to_root_m)?;
    ensure!(
        transform.near(resolved[0].desired_root_m),
        "cad_occurrence_pose_differs"
    );
    let mut chain = Vec::new();
    let mut current = Some(*o);
    while let Some(node) = current {
        ensure!(
            chain.len() < assembly.budget.max_depth as usize,
            "cad_occurrence_parent_depth_exceeded"
        );
        // Only entire-part references are received for geometric composition.
        ensure!(
            node.reference_set == "None",
            "cad_occurrence_reference_set_not_received"
        );
        chain.push(node.clone());
        current = node.parent_occurrence_id.as_deref().map(|p| nodes[p]);
    }
    chain.reverse();
    ensure!(
        chain
            .iter()
            .map(|o| o.name.as_str())
            .eq(target.occurrence_path.iter().map(String::as_str)),
        "cad_occurrence_parent_chain_differs"
    );
    let source = assembly
        .prototypes
        .get(&target.prototype_id)
        .context("cad_occurrence_prototype_source_missing")?;
    ensure!(
        source["sha256"] == binding.intent.object.source_sha256,
        "cad_occurrence_definition_and_assembly_source_differ"
    );
    let prototype = assembly
        .value
        .prototypes
        .iter()
        .find(|p| p.prototype_id == target.prototype_id)
        .unwrap();
    let unit = part.part["units"]
        .as_str()
        .context("cad_occurrence_definition_unit_missing")?;
    ensure!(
        prototype.native_units == unit,
        "cad_occurrence_prototype_units_differ"
    );
    ensure!(
        part.part["solid_count"].as_u64() == Some(prototype.solids.len() as u64)
            && part.part["sheet_count"].as_u64() == Some(prototype.sheet_count as u64),
        "cad_occurrence_prototype_inventory_differs"
    );
    let tags = binding.check["candidate_edge_tags"]
        .as_array()
        .context("cad_occurrence_circle_candidates_missing")?;
    ensure!(
        tags.len() == 1,
        "cad_occurrence_definition_circle_not_unique"
    );
    let edges = part.part["model_edges"]["entries"]
        .as_array()
        .context("cad_occurrence_definition_edges_missing")?;
    let selected: Vec<_> = edges.iter().filter(|e| e["edge_tag"] == tags[0]).collect();
    ensure!(
        selected.len() == 1,
        "cad_occurrence_definition_edge_ambiguous"
    );
    let edge = selected[0];
    let arc = &edge["arc"];
    let scale = assembly::unit_to_m(unit)?;
    let center =
        model_edges::vector(&arc["center"]).context("cad_occurrence_circle_center_missing")?;
    let x = model_edges::vector(&arc["x_axis"]).context("cad_occurrence_circle_x_missing")?;
    let y = model_edges::vector(&arc["y_axis"]).context("cad_occurrence_circle_y_missing")?;
    let normal = model_edges::cross(x, y);
    let length = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
    ensure!(
        length.is_finite() && length > 0.,
        "cad_occurrence_circle_normal_invalid"
    );
    let radius = arc["radius"]
        .as_f64()
        .context("cad_occurrence_circle_radius_missing")?
        * scale;
    Ok(
        json!({"binding_id":binding.intent.binding_id,"feature_id":binding.intent.selector.feature_id,
        "status":"bounded_instance_use","object":binding.intent.object,"prototype_selector":binding.intent.selector,
        "prototype_source":source,"occurrence_path":target.occurrence_path,"prototype_id":target.prototype_id,
        "occurrence_id":o.occurrence_id,"part_occurrence_tag":o.part_occurrence_tag,"parent_chain":chain,
        "local_to_root_m":o.local_to_root_m,"prototype_edge_tag":tags[0],"prototype_body_tag":edge["body_tag"],
        "definition_native_result_sha256":part.native_result_sha256,"assembly_native_result_sha256":assembly.native_sha256,
        "derived_circle_root_m":{"center":transform.point(center.map(|c|c*scale))?,
            "plane_normal":transform.vector(normal.map(|n|n/length))?,"radius":radius},
        "geometry_basis":"actual prototype circle plus native assembly transform; derived root-frame geometry",
        "native_occurrence_edge_observed":false,"creator_lineage":"not_established","engineering_adoption":"not_established"}),
    )
}

/// A saved report cannot construct the opaque verified capability.
pub fn verify(
    root: &SkillRoot,
    request: &CadOccurrenceBindingRequest1,
) -> Result<VerifiedOccurrenceBindings> {
    verify_endpoint(root, request, false)
}
/// Empty selected-use endpoints are admitted only by explicit cross-version
/// correspondence, whose original change owner checks complete before/after coverage.
pub(super) fn verify_for_correspondence(
    root: &SkillRoot,
    request: &CadOccurrenceBindingRequest1,
) -> Result<VerifiedOccurrenceBindings> {
    verify_endpoint(root, request, true)
}
fn verify_endpoint(
    root: &SkillRoot,
    request: &CadOccurrenceBindingRequest1,
    allow_empty: bool,
) -> Result<VerifiedOccurrenceBindings> {
    request
        .validate()
        .map_err(|_| anyhow::anyhow!("cad_occurrence_request_invalid"))?;
    ensure!(
        request.schema == "oev2.cad-occurrence-binding/1"
            && request.definition.backend == "nx"
            && (allow_empty || !request.definition.bindings.is_empty()),
        "cad_occurrence_profile_or_bindings_invalid"
    );
    ensure!(
        request.definition.project_context.project_id
            == request.assembly.project_context.project_id
            && request.definition.project_context.project_revision
                == request.assembly.project_context.project_revision
            && request.definition.project_context.project_sha256
                == request.assembly.project_context.project_sha256,
        "cad_occurrence_requires_same_project"
    );
    let mut sources = Sources::new(root);
    let definition = state(&mut sources, &request.definition)?;
    let assembly = observed_assembly(&mut sources, &request.assembly)?;
    let mut bindings = Vec::new();
    let mut seeds = BTreeSet::from([
        request.definition.object.object_id.clone(),
        request.assembly.object.object_id.clone(),
        request.definition.native_result_ref.clone(),
        request.assembly.native_result_ref.clone(),
    ]);
    for b in definition.bindings.values() {
        seeds.extend(b.intent.dependency_ids.iter().cloned());
        let r = if binding_status(&b.check) != "unique" {
            json!({"binding_id":b.intent.binding_id,"feature_id":b.intent.selector.feature_id,"status":"unresolved",
                "reason":"cad_occurrence_definition_geometry_unqualified","definition_check":b.check})
        } else if let Some(part) = definition.observation.as_ref() {
            match binding(b, part, &assembly) {
                Ok(value) => value,
                Err(e) => json!({"binding_id":b.intent.binding_id,
                "feature_id":b.intent.selector.feature_id,"status":"unresolved","reason":e.to_string(),"definition_check":b.check}),
            }
        } else {
            json!({"binding_id":b.intent.binding_id,"feature_id":b.intent.selector.feature_id,"status":"unresolved","reason":"cad_occurrence_definition_observation_unavailable"})
        };
        bindings.push(r);
    }
    // Alias rejection is symmetric: no first-row privilege or overwritten ID.
    let mut uses = BTreeMap::<(String, u64), usize>::new();
    let mut feature_ids = BTreeMap::<String, usize>::new();
    for b in &bindings {
        *feature_ids
            .entry(b["feature_id"].as_str().unwrap().into())
            .or_default() += 1;
        if b["status"] == "bounded_instance_use" {
            *uses
                .entry((
                    b["occurrence_id"].as_str().unwrap().into(),
                    b["prototype_edge_tag"].as_u64().unwrap(),
                ))
                .or_default() += 1;
        }
    }
    let mut qualified = BTreeMap::new();
    for b in &mut bindings {
        if b["status"] == "bounded_instance_use" {
            let key = (
                b["occurrence_id"].as_str().unwrap().into(),
                b["prototype_edge_tag"].as_u64().unwrap(),
            );
            if uses[&key] > 1 || feature_ids[b["feature_id"].as_str().unwrap()] > 1 {
                b["status"] = json!("unresolved");
                b["reason"] = json!("cad_occurrence_instance_feature_aliased");
            } else {
                qualified.insert(b["feature_id"].as_str().unwrap().into(), b.clone());
            }
        }
    }
    sources.verify_unchanged()?;
    let second = observed_assembly(&mut sources, &request.assembly)?;
    ensure!(
        second.report == assembly.report,
        "cad_occurrence_assembly_changed_during_check"
    );
    if let Some(first) = definition.observation.as_ref() {
        let job = root.expand(Path::new(&request.definition.job.path))?;
        let second = crate::drawing::intent::observe_part_bound(
            job.parent().unwrap(),
            &request.definition.part_input,
            &request.definition.object,
            &request.definition.native_result_ref,
        )?;
        ensure!(
            *first == second,
            "cad_occurrence_definition_changed_during_check"
        );
    }
    sources.verify_unchanged()?;
    let report = json!({"schema":"oev2.cad-occurrence-binding-report/1","definition":definition.report,
        "assembly":assembly.report,"bindings":bindings,"sources":sources.report(),"impact_seed_ids":seeds,
        "project_review":"requires_existing_kernel_check","native_dispatch":"not_run",
        "creator_lineage":"not_established","persistent_topology_naming":"not_established",
        "engineering_adoption":"not_established","cross_version_correspondence":"not_established_by_this_current_state_profile"});
    Ok(VerifiedOccurrenceBindings {
        request: request.clone(),
        report,
        qualified,
        seeds,
    })
}
