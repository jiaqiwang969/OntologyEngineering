//! A real CopyComponents operation on an explicitly owned source closure.
//! Current process tags connect this operation only; saved reopen and adoption
//! remain separate obligations, and partial copies never use AddComponent.
use super::{NativeApi, Workspace, motion_task};
use crate::{assembly, motion_source};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, NxAssemblyBudget, NxAssemblyObservation, NxAssemblyOccurrenceObservation,
    NxComponentCopyIntent1, NxComponentCopyOutcome1,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const PARAMS_SCHEMA: &str = "oev2.nx-component-copy-params/1";
fn name_valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|x| x.is_ascii_alphanumeric() || x == b'_')
}
pub fn validate(i: &NxComponentCopyIntent1, b: &NxAssemblyBudget) -> Result<()> {
    i.validate()
        .map_err(|e| anyhow::anyhow!("copy_intent_contract:{e:?}"))?;
    i.object
        .validate()
        .map_err(|e| anyhow::anyhow!("copy_object_contract:{e:?}"))?;
    assembly::budget_valid(b)?;
    motion_source::validate_source_closure(&i.source)?;
    ensure!(
        i.source_occurrence_path.len() == 1
            && name_valid(&i.source_occurrence_path[0])
            && name_valid(&i.new_instance_name)
            && i.source_occurrence_path[0] != i.new_instance_name,
        "copy_direct_leaf_and_new_name_required"
    );
    ensure!(
        !i.source_refs.is_empty()
            && i.source_refs.len() <= 4096
            && i.source_refs.iter().all(|x| !x.is_empty())
            && i.source_refs.iter().collect::<BTreeSet<_>>().len() == i.source_refs.len(),
        "copy_sources_missing_or_duplicate"
    );
    for f in &i.source.files {
        super::input_relative(&f.path)?;
        ensure!(
            f.path.to_ascii_lowercase().ends_with(".prt"),
            "copy_native_prt_required"
        );
    }
    Ok(())
}
pub fn source(
    w: &dyn Workspace,
    r: &ArtifactReference,
    b: &NxAssemblyBudget,
) -> Result<NxComponentCopyIntent1> {
    let i = motion_task::read(w, r, 4 * 1024 * 1024)?;
    validate(&i, b)?;
    Ok(i)
}
fn selected<'a>(
    i: &NxComponentCopyIntent1,
    v: &'a NxAssemblyObservation,
) -> Result<&'a NxAssemblyOccurrenceObservation> {
    ensure!(
        v.root_prototype_id == i.source.root_part && v.source_role == "reference_observation",
        "copy_current_root_or_role_differs"
    );
    ensure!(
        v.occurrences
            .iter()
            .all(|x| x.parent_occurrence_id.is_none()),
        "copy_direct_leaf_profile_required"
    );
    ensure!(
        v.occurrences
            .iter()
            .map(|x| x.name.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == v.occurrences.len()
            && v.occurrences
                .iter()
                .map(|x| x.instance_tag)
                .collect::<BTreeSet<_>>()
                .len()
                == v.occurrences.len(),
        "copy_direct_instance_identity_ambiguous"
    );
    ensure!(
        !v.occurrences
            .iter()
            .any(|x| x.name.eq_ignore_ascii_case(&i.new_instance_name)),
        "copy_new_instance_name_exists"
    );
    let choices: Vec<_> = v
        .occurrences
        .iter()
        .filter(|x| x.name == i.source_occurrence_path[0])
        .collect();
    ensure!(choices.len() == 1, "copy_source_missing_or_ambiguous");
    let source = choices[0];
    ensure!(
        !v.occurrences
            .iter()
            .any(|x| x.parent_occurrence_id.as_deref() == Some(&source.occurrence_id)),
        "copy_source_is_not_leaf"
    );
    Ok(source)
}
fn same_occurrence(
    a: &NxAssemblyOccurrenceObservation,
    b: &NxAssemblyOccurrenceObservation,
) -> Result<()> {
    // Enumeration indices can change within a complete before/after inventory.
    // Native tags identify retained objects only inside this one process.
    let mut a = serde_json::to_value(a)?;
    let mut b = serde_json::to_value(b)?;
    a.as_object_mut().unwrap().remove("occurrence_id");
    b.as_object_mut().unwrap().remove("occurrence_id");
    ensure!(a == b, "copy_retained_instance_changed");
    Ok(())
}
pub fn check_change(
    i: &NxComponentCopyIntent1,
    b: &NxAssemblyBudget,
    before: &NxAssemblyObservation,
    call: &NxComponentCopyOutcome1,
    after: &NxAssemblyObservation,
) -> Result<Value> {
    validate(i, b)?;
    assembly::validate_observation(before, b)?;
    assembly::validate_observation(after, b)?;
    call.validate()
        .map_err(|e| anyhow::anyhow!("copy_outcome_contract:{e:?}"))?;
    let source = selected(i, before)?;
    ensure!(
        after.root_prototype_id == before.root_prototype_id
            && after.source_role == before.source_role
            && after.occurrences.len() == before.occurrences.len() + 1
            && after
                .occurrences
                .iter()
                .all(|x| x.parent_occurrence_id.is_none()),
        "copy_after_inventory_differs"
    );
    let prototypes = |v: &NxAssemblyObservation| -> Result<BTreeMap<String, Value>> {
        v.prototypes
            .iter()
            .map(|p| Ok((p.prototype_id.clone(), serde_json::to_value(p)?)))
            .collect()
    };
    ensure!(
        prototypes(before)? == prototypes(after)?,
        "copy_prototype_geometry_or_units_changed"
    );
    let root = before
        .prototypes
        .iter()
        .find(|p| p.prototype_id == before.root_prototype_id)
        .context("copy_root_missing")?;
    let proto = before
        .prototypes
        .iter()
        .find(|p| p.prototype_id == source.prototype_id)
        .context("copy_source_prototype_missing")?;
    ensure!(
        call.call_entered
            && call.call_returned
            && call.complete
            && call.wrapper_code == 0
            && call.exception_code == 0
            && call.returned_count == 1
            && call.returned_component_tags.len() == 1
            && call.rename_attempted
            && call.rename_code == 0,
        "copy_native_call_partial_or_failed"
    );
    ensure!(
        call.source_component_tag == source.part_occurrence_tag
            && call.source_instance_tag == source.instance_tag
            && call.source_parent_tag == root.native_part_tag
            && call.source_prototype_tag == proto.native_part_tag
            && call.new_parent_tag == root.native_part_tag
            && call.new_prototype_tag == proto.native_part_tag,
        "copy_native_parent_prototype_or_source_differs"
    );
    let old_tags: BTreeSet<_> = before
        .occurrences
        .iter()
        .map(|x| x.part_occurrence_tag)
        .collect();
    let old_instances: BTreeSet<_> = before.occurrences.iter().map(|x| x.instance_tag).collect();
    let all_old_tags: BTreeSet<_> = old_tags
        .iter()
        .chain(&old_instances)
        .copied()
        .chain(before.prototypes.iter().map(|p| p.native_part_tag))
        .chain(
            before
                .prototypes
                .iter()
                .flat_map(|p| p.solids.iter().map(|s| s.body_tag)),
        )
        .collect();
    ensure!(
        call.returned_component_tags[0] != 0
            && call.new_instance_tag != 0
            && call.returned_component_tags[0] != call.new_instance_tag
            && !all_old_tags.contains(&call.returned_component_tags[0])
            && !all_old_tags.contains(&call.new_instance_tag),
        "copy_native_identity_reused"
    );
    for prior in &before.occurrences {
        let retained: Vec<_> = after
            .occurrences
            .iter()
            .filter(|x| x.part_occurrence_tag == prior.part_occurrence_tag)
            .collect();
        ensure!(
            retained.len() == 1,
            "copy_retained_instance_missing_or_ambiguous"
        );
        same_occurrence(prior, retained[0])?;
    }
    let added: Vec<_> = after
        .occurrences
        .iter()
        .filter(|x| !old_tags.contains(&x.part_occurrence_tag))
        .collect();
    ensure!(added.len() == 1, "copy_new_identity_missing_or_ambiguous");
    let new = added[0];
    ensure!(
        new.part_occurrence_tag == call.returned_component_tags[0]
            && new.instance_tag == call.new_instance_tag
            && new.name == i.new_instance_name
            && new.prototype_id == source.prototype_id
            && new.reference_set == source.reference_set
            && new.local_to_parent_m == source.local_to_parent_m
            && new.local_to_root_m == source.local_to_root_m,
        "copy_returned_instance_not_current_copy"
    );
    Ok(
        json!({"operation":"NXOpen::Assemblies::ComponentAssembly::CopyComponents",
        "source_occurrence_path":i.source_occurrence_path,"new_occurrence_path":[i.new_instance_name],
        "source_current_component":source.part_occurrence_tag,"new_current_component":new.part_occurrence_tag,
        "prototype_id":source.prototype_id,"current_process_only":true,
        "new_engineering_adoption":"not_established","independent_saved_reopen":"required_separate_process"}),
    )
}
pub fn check_reference_sets(
    before: &NxAssemblyObservation,
    before_sets: &oe_contracts::NxAssemblyReferenceSets2,
    after: &NxAssemblyObservation,
    after_sets: &oe_contracts::NxAssemblyReferenceSets2,
    call: &NxComponentCopyOutcome1,
) -> Result<()> {
    crate::assembly_reference_sets::validate(before_sets, before)?;
    crate::assembly_reference_sets::validate(after_sets, after)?;
    for prior in &before_sets.members {
        let next = after_sets
            .members
            .iter()
            .find(|x| x.instance_tag == prior.instance_tag)
            .context("copy_retained_reference_set_missing")?;
        ensure!(
            next.part_occurrence_tag == prior.part_occurrence_tag
                && next.instance_reference_set == prior.instance_reference_set
                && next.part_occurrence_reference_set == prior.part_occurrence_reference_set,
            "copy_retained_reference_set_changed"
        );
    }
    let source = before_sets
        .members
        .iter()
        .find(|x| x.instance_tag == call.source_instance_tag)
        .context("copy_source_reference_set_missing")?;
    let new = after_sets
        .members
        .iter()
        .find(|x| x.instance_tag == call.new_instance_tag)
        .context("copy_new_reference_set_missing")?;
    ensure!(
        new.instance_reference_set == source.instance_reference_set
            && new.part_occurrence_reference_set == source.part_occurrence_reference_set,
        "copy_new_reference_set_differs"
    );
    Ok(())
}
fn loaded(api: &mut dyn NativeApi, namespace: &Path, i: &NxComponentCopyIntent1) -> Result<()> {
    let actual = api.loaded_assembly_files(namespace)?;
    let expected: BTreeSet<_> = i.source.files.iter().map(|f| f.path.as_str()).collect();
    let seen: BTreeSet<_> = actual.iter().map(String::as_str).collect();
    ensure!(
        actual.len() == seen.len() && seen == expected,
        "copy_loaded_source_closure_differs"
    );
    Ok(())
}
fn step<T>(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    action: impl FnOnce(&mut dyn NativeApi) -> Result<T>,
) -> Result<T> {
    let result = action(api);
    let persistence = api.persist_load_diagnostics(&mut |v| w.record("native_load_status", v));
    match (result, persistence) {
        (Err(e), Err(p)) => Err(e.context(format!("copy_diagnostic_persistence_failed:{p:#}"))),
        (Err(e), _) => Err(e),
        (Ok(v), p) => {
            p?;
            Ok(v)
        }
    }
}
pub fn execute(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    r: &ArtifactReference,
    i: &NxComponentCopyIntent1,
    b: &NxAssemblyBudget,
) -> Result<Value> {
    execute_inner(api, w, r, i, b, None)
}
/// Extra measurements have their own action identity; no old receipt is requalified.
pub fn execute_probed(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    r: &ArtifactReference,
    i: &NxComponentCopyIntent1,
    b: &NxAssemblyBudget,
    repetitions: u32,
) -> Result<Value> {
    super::mass_probe::repetitions_valid(repetitions)?;
    execute_inner(api, w, r, i, b, Some(repetitions))
}
fn execute_inner(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    r: &ArtifactReference,
    i: &NxComponentCopyIntent1,
    b: &NxAssemblyBudget,
    probe: Option<u32>,
) -> Result<Value> {
    validate(i, b)?;
    let (root_path, namespace) = w.prepare_motion_source(&i.source)?;
    let root = step(api, w, |a| a.open_part(&root_path))?;
    loaded(api, &namespace, i)?;
    step(api, w, |a| {
        a.prepare_owned_assembly_context(root, &namespace)
    })?;
    let before = step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    assembly::validate_observation(&before, b)?;
    let source = selected(i, &before)?.clone();
    let before_ref = w.record_bound("component_copy_before", serde_json::to_value(&before)?)?;
    let before_sets = step(api, w, |a| a.assembly_reference_sets(&before))?;
    let before_sets_ref = w.record_bound(
        "component_copy_before_reference_sets",
        serde_json::to_value(&before_sets)?,
    )?;
    let before_samples = match probe {
        Some(n) => super::mass_probe::capture(api, w, &before, n, "before")?,
        None => Vec::new(),
    };
    ensure!(
        (before.occurrences.len() as u64) < b.max_occurrences,
        "copy_occurrence_budget_before_mutation"
    );
    w.record("component_copy_pending",json!({"intent":r,"source_component":source.part_occurrence_tag,
        "source_instance":source.instance_tag,"new_instance_name":i.new_instance_name,"engineering_adoption":"not_established"}))?;
    let call = step(api, w, |a| {
        a.copy_assembly_component(root, &source, &i.new_instance_name)
    })?;
    let call_ref = w.record_bound("component_copy_returned", serde_json::to_value(&call)?)?;
    // An exception or partial return can have native side effects. Preserve the
    // full actual after inventory before rejecting, without retry or saving.
    let after = step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    let after_ref = w.record_bound("component_copy_after", serde_json::to_value(&after)?)?;
    let after_samples = match probe {
        Some(n) => super::mass_probe::capture(api, w, &after, n, "after")?,
        None => Vec::new(),
    };
    loaded(api, &namespace, i)?;
    let checked = check_change(i, b, &before, &call, &after)?;
    let after_sets = step(api, w, |a| a.assembly_reference_sets(&after))?;
    let after_sets_ref = w.record_bound(
        "component_copy_after_reference_sets",
        serde_json::to_value(&after_sets)?,
    )?;
    check_reference_sets(&before, &before_sets, &after, &after_sets, &call)?;
    w.verify_motion_source()?;
    w.record(
        "component_copy_save_pending",
        json!({"namespace":namespace}),
    )?;
    let saved = step(api, w, |a| a.save_all_owned_assembly(&namespace))?;
    let saved_ref = w.record_bound("component_copy_save_returned", saved.clone())?;
    ensure!(
        saved["api"] == "UF_PART_save_all"
            && saved["return_code"].as_i64() == Some(0)
            && saved["file_error_count"].as_u64() == Some(0)
            && saved["file_errors"].as_array().is_some_and(Vec::is_empty),
        "copy_save_result_failed_or_incomplete"
    );
    let saved_inventory = step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    let saved_inventory_ref = w.record_bound(
        "component_copy_saved_inventory",
        serde_json::to_value(&saved_inventory)?,
    )?;
    let saved_samples = match probe {
        Some(n) => super::mass_probe::capture(api, w, &saved_inventory, n, "saved")?,
        None => Vec::new(),
    };
    check_change(i, b, &before, &call, &saved_inventory)?;
    let saved_sets = step(api, w, |a| a.assembly_reference_sets(&saved_inventory))?;
    let saved_sets_ref = w.record_bound(
        "component_copy_saved_reference_sets",
        serde_json::to_value(&saved_sets)?,
    )?;
    check_reference_sets(&before, &before_sets, &saved_inventory, &saved_sets, &call)?;
    loaded(api, &namespace, i)?;
    w.verify_motion_source()?;
    let mut result = json!({"action":"copy_assembly_component","object":i.object,"intent":r,
        "before":before_ref,"call":call_ref,"after":after_ref,"saved":saved_ref,"saved_inventory":saved_inventory_ref,
        "before_reference_sets":before_sets_ref,"after_reference_sets":after_sets_ref,"saved_reference_sets":saved_sets_ref,"checked":checked,
        "engineering_adoption":"not_assessed","independent_reopen_checked":false});
    if let Some(repetitions) = probe {
        result["action"] = json!(super::mass_probe::COPY_ACTION);
        result["mass_property_probes"] = json!({"repetitions":repetitions,"before":before_samples,"after":after_samples,"saved":saved_samples});
        result["qualification"] =
            json!("explicit_diagnostic_action_only; old copy lineage not granted");
    }
    Ok(result)
}
