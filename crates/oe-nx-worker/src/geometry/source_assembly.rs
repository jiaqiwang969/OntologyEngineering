//! Build a new direct-occurrence assembly from real, privately copied PRTs.
//! Original sources, native acquisition and engineering adoption stay distinct.
use super::{NativeApi, Workspace, input_relative, motion_task};
use crate::{
    assembly::{self, Rigid},
    motion_source,
};
use anyhow::{Context, Result, ensure};
use oe_contracts::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub const PARAMS_SCHEMA: &str = "oev2.nx-source-assembly-params/1";
pub fn validate_source_set(i: &NxAssemblyBuildIntent) -> Result<()> {
    motion_source::validate_source_files(&i.files, i.max_total_bytes)?;
    input_relative(&i.root_part)?;
    ensure!(
        !i.root_part.contains('/') && i.root_part.to_ascii_lowercase().ends_with(".prt"),
        "build_root_leaf_prt_required"
    );
    let root = oe_nx_protocol::compatibility_collision_key(&i.root_part);
    for f in &i.files {
        input_relative(&f.path)?;
        ensure!(
            !f.path.starts_with("inputs/") && f.path.to_ascii_lowercase().ends_with(".prt"),
            "build_source_prt_required"
        );
        ensure!(
            oe_nx_protocol::compatibility_collision_key(f.path.split('/').next().unwrap()) != root,
            "build_root_conflicts_with_source"
        );
    }
    Ok(())
}
pub fn validate(i: &NxAssemblyBuildIntent, b: &NxAssemblyBudget) -> Result<()> {
    i.validate()
        .map_err(|_| anyhow::anyhow!("build_generated_intent_invalid"))?;
    ensure!(
        i.schema == "oev2.nx-assembly-build-intent/1",
        "build_schema_invalid"
    );
    assembly::budget_valid(b)?;
    assembly::unit_to_m(&i.root_native_units)?;
    validate_source_set(i)?;
    ensure!(
        !i.placements.is_empty()
            && i.placements.len() as u64 <= b.max_occurrences
            && (i.files.len() as u64) < b.max_prototypes,
        "build_inventory_budget"
    );
    ensure!(
        !i.source_refs.is_empty()
            && i.source_refs.len() <= 4096
            && i.source_refs.iter().all(|r| !r.is_empty())
            && i.source_refs.iter().collect::<BTreeSet<_>>().len() == i.source_refs.len(),
        "build_sources_invalid"
    );
    crate::assembly_motion::sample_valid(&NxAssemblyMotionSample {
        sample_id: "construction".into(),
        time_s: 0.,
        targets: i.placements.clone(),
    })?;
    let files: BTreeSet<_> = i.files.iter().map(|f| f.path.as_str()).collect();
    for p in &i.placements {
        ensure!(
            p.occurrence_path.len() == 1
                && p.occurrence_path[0].len() <= 64
                && p.occurrence_path[0]
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_'),
            "build_direct_instance_name_required"
        );
        ensure!(
            files.contains(p.prototype_id.as_str()),
            "build_prototype_not_declared"
        );
    }
    Ok(())
}
pub fn source(
    w: &dyn Workspace,
    r: &ArtifactReference,
    b: &NxAssemblyBudget,
) -> Result<NxAssemblyBuildIntent> {
    let i = motion_task::read(w, r, 32 * 1024 * 1024)?;
    validate(&i, b)?;
    Ok(i)
}
pub fn check_inventory(
    i: &NxAssemblyBuildIntent,
    b: &NxAssemblyBudget,
    v: &NxAssemblyObservation,
) -> Result<()> {
    assembly::validate_observation(v, b)?;
    ensure!(
        v.root_prototype_id == i.root_part && v.source_role == "reference_observation",
        "build_native_root_or_role_differs"
    );
    let expected: BTreeMap<_, _> = i
        .placements
        .iter()
        .map(|p| (p.occurrence_path[0].as_str(), p))
        .collect();
    ensure!(
        v.occurrences.len() == expected.len(),
        "build_native_occurrence_count_differs"
    );
    let mut seen = BTreeSet::new();
    for o in &v.occurrences {
        ensure!(
            o.parent_occurrence_id.is_none() && seen.insert(o.name.as_str()),
            "build_native_nested_or_duplicate_instance"
        );
        let p = expected
            .get(o.name.as_str())
            .context("build_native_unexpected_instance")?;
        ensure!(
            o.prototype_id == p.prototype_id
                && Rigid::checked(&o.local_to_root_m)?.near(Rigid::checked(&p.local_to_root_m)?)
                && Rigid::checked(&o.local_to_parent_m)?.near(Rigid::checked(&p.local_to_root_m)?),
            "build_native_pose_or_prototype_differs"
        );
    }
    let protos: BTreeSet<_> = i
        .placements
        .iter()
        .map(|p| p.prototype_id.as_str())
        .chain(std::iter::once(i.root_part.as_str()))
        .collect();
    ensure!(
        v.prototypes
            .iter()
            .map(|p| p.prototype_id.as_str())
            .collect::<BTreeSet<_>>()
            == protos,
        "build_native_prototype_closure_differs"
    );
    let root = v
        .prototypes
        .iter()
        .find(|p| p.prototype_id == i.root_part)
        .context("build_native_root_missing")?;
    ensure!(
        root.native_units == i.root_native_units && root.solids.is_empty() && root.sheet_count == 0,
        "build_new_root_geometry_or_units_differs"
    );
    Ok(())
}
fn loaded(
    api: &mut dyn NativeApi,
    namespace: &std::path::Path,
    i: &NxAssemblyBuildIntent,
) -> Result<()> {
    let declared: BTreeSet<_> = i
        .files
        .iter()
        .map(|f| f.path.as_str())
        .chain(std::iter::once(i.root_part.as_str()))
        .collect();
    let files = api.loaded_assembly_files(namespace)?;
    let mut seen = BTreeSet::new();
    ensure!(
        !files.is_empty() && files.len() <= 4096,
        "build_loaded_budget"
    );
    for f in &files {
        ensure!(
            declared.contains(f.as_str()) && seen.insert(f.as_str()),
            "build_loaded_undeclared_or_duplicate"
        );
    }
    ensure!(
        seen.contains(i.root_part.as_str()),
        "build_loaded_root_missing"
    );
    Ok(())
}
/// Flush even a failed native operation; preserve its error if persistence succeeds.
fn step<T>(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    operation: impl FnOnce(&mut dyn NativeApi) -> Result<T>,
) -> Result<T> {
    let result = operation(api);
    api.persist_load_diagnostics(&mut |batch| w.record("native_load_status", batch))?;
    result
}
pub fn execute(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    r: &ArtifactReference,
    i: &NxAssemblyBuildIntent,
    b: &NxAssemblyBudget,
) -> Result<Value> {
    validate(i, b)?;
    let (total, file, count) = w.motion_output_bounds()?;
    ensure!(
        i.files.len() + 1 <= count
            && i.files.iter().all(|f| f.bytes <= file)
            && i.files.iter().map(|f| f.bytes).sum::<u64>() < total,
        "build_output_capacity"
    );
    let (root_path, namespace) = w.prepare_assembly_build(i)?;
    let root = step(api, w, |a| {
        a.create_assembly_units_part(&root_path, &i.root_native_units)
    })?;
    // Native namespace reads require a materialized regular PRT. This saves
    // only the new empty private root, before any source instance is added.
    step(api, w, |a| a.set_work_part(root))?;
    step(api, w, |a| a.save_work_part())?;
    w.record(
        "source_assembly_empty_root_saved",
        json!({"root_part":i.root_part,"assembly_complete":false}),
    )?;
    let mut parts = BTreeMap::new();
    let mut originals = BTreeMap::new();
    for p in &i.placements {
        if parts.contains_key(&p.prototype_id) {
            continue;
        }
        let tag = step(api, w, |a| a.open_part(&namespace.join(&p.prototype_id)))?;
        loaded(api, &namespace, i)?;
        let before = step(api, w, |a| {
            a.full_assembly_info(tag, &namespace, b, "reference_observation")
        })?;
        assembly::validate_observation(&before, b)?;
        ensure!(
            before.root_prototype_id == p.prototype_id
                && before.occurrences.is_empty()
                && before.prototypes.len() == 1,
            "build_source_is_not_declared_leaf"
        );
        parts.insert(p.prototype_id.clone(), tag);
        originals.insert(p.prototype_id.clone(), before);
    }
    step(api, w, |a| {
        a.prepare_owned_assembly_context(root, &namespace)
    })?;
    let scale = assembly::unit_to_m(&i.root_native_units)?;
    for p in &i.placements {
        let m = &p.local_to_root_m;
        let rotation = [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]];
        step(api, w, |a| {
            a.add_assembly_entire_instance_nxopen(
                root,
                parts[&p.prototype_id],
                &namespace.join(&p.prototype_id),
                &p.occurrence_path[0],
                [m[3] / scale, m[7] / scale, m[11] / scale],
                &rotation,
            )
        })?;
    }
    loaded(api, &namespace, i)?;
    let before = step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    let before_record = w.record_bound(
        "source_assembly_before_save",
        serde_json::to_value(&before)?,
    )?;
    check_inventory(i, b, &before)?;
    for (id, original) in &originals {
        let mut current = original.clone();
        current.prototypes = before
            .prototypes
            .iter()
            .filter(|p| &p.prototype_id == id)
            .cloned()
            .collect();
        assembly::same_geometry(original, &current)?;
    }
    let save = step(api, w, |a| a.save_all_owned_assembly(&namespace))?;
    w.record("source_assembly_save_returned", save.clone())?;
    let after = step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    let after_record =
        w.record_bound("source_assembly_after_save", serde_json::to_value(&after)?)?;
    check_inventory(i, b, &after)?;
    assembly::same_geometry(&before, &after)?;
    loaded(api, &namespace, i)?;
    w.verify_motion_source()?;
    Ok(
        json!({"action":"create_source_assembly","object":i.object,"intent":r,"before_save_record":before_record,"after_save_record":after_record,"occurrences":after.occurrences.len(),"prototypes":after.prototypes.len(),
  "save":save,"independent_reopen_checked":false,"engineering_adoption":"not_assessed"}),
    )
}
