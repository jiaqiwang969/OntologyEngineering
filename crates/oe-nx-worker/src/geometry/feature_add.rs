//! Source-bound addition to a current prototype. Shares the existing bounded
//! plate/assembly checker; never rebuilds a plate or replays a deleted feature.
use super::{NativeApi, Workspace, feature_delete as plate, motion_task};
use crate::{assembly, assembly_reference_sets};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, NxAssemblyBudget, NxAssemblyObservation, NxFeatureAddIntent1,
    NxFeatureAddOutcome1, NxFeatureDeleteIntent1, NxFeatureInventory1,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const ACTION: &str = "add_hole_feature";
pub const PARAMS_SCHEMA: &str = "oev2.nx-feature-add-params/1";
pub const INTENT_SCHEMA: &str = "oev2.nx-feature-add-intent/1";
pub const OUTCOME_SCHEMA: &str = "oev2.nx-feature-add-outcome/1";

// A private, nonserialized geometry projection, not a deletion intent/source
// conversion. The existing one/two-hole profile has the same preconditions and
// independently checks both states. No deletion outcome or history is reused.
fn profile(i: &NxFeatureAddIntent1) -> NxFeatureDeleteIntent1 {
    NxFeatureDeleteIntent1 {
        schema: plate::INTENT_SCHEMA.into(),
        object: i.object.clone(),
        source: i.source.clone(),
        target_part: i.target_part.clone(),
        target: i.target.clone(),
        retained: i.retained.clone(),
        plate_bounds_mm: i.plate_bounds_mm.clone(),
        feature_type: i.feature_type.clone(),
        feature_location_mm: i.feature_location_mm.clone(),
        source_refs: i.source_refs.clone(),
    }
}
pub fn validate(i: &NxFeatureAddIntent1, b: &NxAssemblyBudget) -> Result<()> {
    i.validate()
        .map_err(|e| anyhow::anyhow!("add_intent_contract:{e:?}"))?;
    ensure!(i.schema == INTENT_SCHEMA, "add_intent_schema_invalid");
    plate::validate(&profile(i), b)?;
    let height = i.tool_end_z_mm - i.feature_location_mm[2];
    ensure!(
        i.tool_end_z_mm.is_finite()
            && i.tool_end_z_mm > i.plate_bounds_mm[5]
            && height.is_finite()
            && height > 0.
            && (2. * i.target.radius).is_finite(),
        "add_tool_does_not_cross_plate"
    );
    Ok(())
}
pub fn source(
    w: &dyn Workspace,
    r: &ArtifactReference,
    b: &NxAssemblyBudget,
) -> Result<NxFeatureAddIntent1> {
    let i = motion_task::read(w, r, 4 * 1024 * 1024)?;
    validate(&i, b)?;
    Ok(i)
}
pub fn check_geometry(i: &NxFeatureAddIntent1, detail: &Value, added: bool) -> Result<Value> {
    // This public checker must also refuse malformed unvalidated inputs.
    let p = profile(i);
    plate::check_geometry(&p, detail, !added)
}
pub fn check_assembly(
    i: &NxFeatureAddIntent1,
    b: &NxAssemblyBudget,
    before: &NxAssemblyObservation,
    after: &NxAssemblyObservation,
) -> Result<Value> {
    validate(i, b)?;
    // Reverse only the inputs to the two-state equality check: that owner
    // expects two holes then one. Original observations/events stay untouched.
    plate::check_assembly(&profile(i), b, after, before)?;
    Ok(
        json!({"target_part":i.target_part,"target_feature_id":i.target.feature_id,
        "assembly_relations":"preserved_current_process", "geometry_volume_policy":"existing assembly persistence 1e-12 m3",
        "independent_reopen":"required_separate_process"}),
    )
}
fn inventory_valid(inventory: &NxFeatureInventory1, detail: &Value) -> Result<()> {
    let faces = detail["solid_face_inventory"]
        .as_array()
        .context("add_face_inventory_missing")?;
    let domain: BTreeSet<_> = faces
        .iter()
        .filter_map(|f| f["face_tag"].as_u64())
        .collect();
    ensure!(
        domain.len() == faces.len()
            && !domain.contains(&0)
            && inventory.schema == plate::INVENTORY_SCHEMA
            && inventory.complete
            && Some(inventory.part_tag) == detail["part_tag"].as_u64()
            && detail["solid_tags"] == json!([inventory.body_tag])
            && inventory.part_tag > 0
            && inventory.body_tag > 0
            && !inventory.features.is_empty()
            && inventory.features.len() <= plate::MAX_FEATURES,
        "add_feature_inventory_incomplete"
    );
    let mut ids = BTreeSet::new();
    for f in &inventory.features {
        ensure!(
            f.complete
                && f.feature_tag > 0
                && f.feature_tag <= u32::MAX as u64
                && ids.insert(f.feature_tag)
                && !f.is_occurrence
                && f.owning_part_tag == inventory.part_tag
                && f.body_tag == inventory.body_tag
                && f.query_codes == [0, 0, 0, 0, 0]
                && !f.feature_type.trim().is_empty()
                && f.location_mm.len() == 3
                && f.location_mm.iter().all(|x| x.is_finite())
                && f.face_tags.iter().collect::<BTreeSet<_>>().len() == f.face_tags.len()
                && f.face_tags.iter().all(|t| domain.contains(t)),
            "add_feature_inventory_partial_or_wrong_owner"
        );
    }
    Ok(())
}
pub fn check_before(
    i: &NxFeatureAddIntent1,
    inventory: &NxFeatureInventory1,
    detail: &Value,
) -> Result<()> {
    check_geometry(i, detail, false)?;
    inventory_valid(inventory, detail)
}
pub fn check_call(
    i: &NxFeatureAddIntent1,
    call: &NxFeatureAddOutcome1,
    part: u64,
    body: u64,
) -> Result<()> {
    ensure!(i.feature_location_mm.len() == 3, "add_tool_origin_invalid");
    ensure!(
        call.schema == OUTCOME_SCHEMA
            && call.part_tag == part
            && call.body_tag == body
            && call.feature_tag > 0
            && call.feature_tag <= u32::MAX as u64
            && call.origin_mm == i.feature_location_mm
            && call.height_mm == i.tool_end_z_mm - i.feature_location_mm[2]
            && call.diameter_mm == 2. * i.target.radius
            && call.call_entered
            && call.call_returned
            && call.complete
            && call.return_code == 0
            && call.update_code == Some(0),
        "add_native_call_failed_or_incomplete"
    );
    Ok(())
}
pub fn check_lineage(
    i: &NxFeatureAddIntent1,
    before: &NxFeatureInventory1,
    before_detail: &Value,
    after: &NxFeatureInventory1,
    after_detail: &Value,
    call: &NxFeatureAddOutcome1,
) -> Result<Value> {
    check_before(i, before, before_detail)?;
    check_geometry(i, after_detail, true)?;
    inventory_valid(after, after_detail)?;
    ensure!(
        before.part_tag == after.part_tag && before.body_tag == after.body_tag,
        "add_current_prototype_changed"
    );
    check_call(i, call, before.part_tag, before.body_tag)?;
    let selected = plate::select_feature(&profile(i), after, after_detail)?;
    let previous: BTreeSet<_> = before.features.iter().map(|f| f.feature_tag).collect();
    let current: BTreeSet<_> = after.features.iter().map(|f| f.feature_tag).collect();
    ensure!(
        selected == call.feature_tag
            && !previous.contains(&selected)
            && current.difference(&previous).copied().collect::<Vec<_>>() == [selected]
            && previous.is_subset(&current),
        "add_new_feature_not_unique_or_original_lost"
    );
    for old in &before.features {
        let new = after
            .features
            .iter()
            .find(|f| f.feature_tag == old.feature_tag)
            .unwrap();
        // Faces may be retopologized by the boolean operation. Check their
        // complete geometry separately; never claim persistent face tags.
        ensure!(
            old.owning_part_tag == new.owning_part_tag
                && old.body_tag == new.body_tag
                && old.is_occurrence == new.is_occurrence
                && old.feature_type == new.feature_type
                && old.location_mm == new.location_mm,
            "add_existing_feature_changed"
        );
    }
    Ok(
        json!({"feature_tag":selected,"part_tag":after.part_tag,"body_tag":after.body_tag,
        "target_feature_id":i.target.feature_id,"previous_feature_count":before.features.len(),
        "current_feature_count":after.features.len(),"persistent_topology_identity":"not_established"}),
    )
}

pub fn execute(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    r: &ArtifactReference,
    i: &NxFeatureAddIntent1,
    b: &NxAssemblyBudget,
) -> Result<Value> {
    validate(i, b)?;
    let p = profile(i);
    let (root_path, namespace) = w.prepare_motion_source(&i.source)?;
    let root = plate::step(api, w, |a| a.open_part(&root_path))?;
    plate::loaded(api, &namespace, &p)?;
    plate::step(api, w, |a| {
        a.prepare_owned_assembly_context(root, &namespace)
    })?;
    let before = plate::step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    assembly::validate_observation(&before, b)?;
    let proto = before
        .prototypes
        .iter()
        .find(|p| p.prototype_id == i.target_part)
        .context("add_target_prototype_missing")?;
    ensure!(
        proto.solids.len() == 1 && proto.native_units == "mm",
        "add_target_profile_differs"
    );
    let (part, body) = (proto.native_part_tag, proto.solids[0].body_tag);
    let before_ref = w.record_bound("feature_add_before", serde_json::to_value(&before)?)?;
    let sets = plate::step(api, w, |a| a.assembly_reference_sets(&before))?;
    assembly_reference_sets::require_entire(&sets, &before)?;
    let sets_ref = w.record_bound(
        "feature_add_before_reference_sets",
        serde_json::to_value(&sets)?,
    )?;
    plate::step(api, w, |a| a.set_work_part(u32::try_from(part)?))?;
    let detail = plate::step(api, w, |a| a.detailed_info(u32::try_from(part)?, false))?;
    let detail_ref = w.record_bound("feature_add_before_detail", detail.clone())?;
    let inventory = plate::step(api, w, |a| {
        a.set_work_part(u32::try_from(part)?)?;
        a.feature_inventory(part, body)
    })?;
    let inventory_ref = w.record_bound(
        "feature_add_before_features",
        serde_json::to_value(&inventory)?,
    )?;
    check_before(i, &inventory, &detail)?;
    ensure!(
        inventory.part_tag == part && inventory.body_tag == body,
        "add_current_inventory_differs"
    );
    let expected = check_geometry(i, &detail, false)?["expected_volume_m3"]
        .as_f64()
        .context("add_volume_expectation_missing")?;
    ensure!(
        (proto.solids[0]
            .volume_m3
            .context("add_before_volume_missing")?
            - expected)
            .abs()
            <= 1e-12,
        "add_before_volume_differs"
    );
    w.verify_motion_source()?;
    plate::loaded(api, &namespace, &p)?;
    w.record(
        "feature_add_pending",
        json!({"intent":r,"target_part":i.target_part,"part_tag":part,"body_tag":body}),
    )?;
    let origin = [
        i.feature_location_mm[0],
        i.feature_location_mm[1],
        i.feature_location_mm[2],
    ];
    let call = plate::step(api, w, |a| {
        a.add_hole_feature(
            part,
            body,
            origin,
            i.tool_end_z_mm - origin[2],
            2. * i.target.radius,
        )
    })?;
    let call_ref = w.record_bound("feature_add_returned", serde_json::to_value(&call)?)?;
    let after_detail = plate::step(api, w, |a| a.detailed_info(u32::try_from(part)?, false))?;
    let after_detail_ref = w.record_bound("feature_add_after_detail", after_detail.clone())?;
    let after = plate::step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    let after_ref = w.record_bound("feature_add_after", serde_json::to_value(&after)?)?;
    check_call(i, &call, part, body)?;
    // Assembly observation finishes with the assembly root as work part.
    // Feature queries require their own prototype context; keep that native
    // precondition and propagate selection failures before further work.
    let after_features = plate::step(api, w, |a| {
        a.set_work_part(u32::try_from(part)?)?;
        a.feature_inventory(part, body)
    })?;
    let after_features_ref = w.record_bound(
        "feature_add_after_features",
        serde_json::to_value(&after_features)?,
    )?;
    let lineage = check_lineage(
        i,
        &inventory,
        &detail,
        &after_features,
        &after_detail,
        &call,
    )?;
    let lineage_ref = w.record_bound("feature_add_lineage", lineage)?;
    let checked = check_assembly(i, b, &before, &after)?;
    let after_sets = plate::step(api, w, |a| a.assembly_reference_sets(&after))?;
    let after_sets_ref = w.record_bound(
        "feature_add_after_reference_sets",
        serde_json::to_value(&after_sets)?,
    )?;
    plate::check_reference_sets(&before, &sets, &after, &after_sets)?;
    w.verify_motion_source()?;
    plate::loaded(api, &namespace, &p)?;
    w.record("feature_add_save_pending", json!({"namespace":namespace}))?;
    let saved = plate::step(api, w, |a| a.save_all_owned_assembly(&namespace))?;
    let saved_ref = w.record_bound("feature_add_save_returned", saved.clone())?;
    ensure!(
        saved["api"] == "UF_PART_save_all"
            && saved["return_code"] == 0
            && saved["file_error_count"] == 0
            && saved["file_errors"].as_array().is_some_and(Vec::is_empty),
        "add_save_failed_or_partial"
    );
    let saved_detail = plate::step(api, w, |a| a.detailed_info(u32::try_from(part)?, false))?;
    let saved_detail_ref = w.record_bound("feature_add_saved_detail", saved_detail.clone())?;
    let saved_features = plate::step(api, w, |a| {
        a.set_work_part(u32::try_from(part)?)?;
        a.feature_inventory(part, body)
    })?;
    let saved_features_ref = w.record_bound(
        "feature_add_saved_features",
        serde_json::to_value(&saved_features)?,
    )?;
    check_lineage(
        i,
        &inventory,
        &detail,
        &saved_features,
        &saved_detail,
        &call,
    )?;
    let saved_inventory = plate::step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    let saved_inventory_ref = w.record_bound(
        "feature_add_saved_inventory",
        serde_json::to_value(&saved_inventory)?,
    )?;
    check_assembly(i, b, &before, &saved_inventory)?;
    let saved_sets = plate::step(api, w, |a| a.assembly_reference_sets(&saved_inventory))?;
    let saved_sets_ref = w.record_bound(
        "feature_add_saved_reference_sets",
        serde_json::to_value(&saved_sets)?,
    )?;
    plate::check_reference_sets(&before, &sets, &saved_inventory, &saved_sets)?;
    w.verify_motion_source()?;
    plate::loaded(api, &namespace, &p)?;
    Ok(
        json!({"action":ACTION,"object":i.object,"intent":r,"target_part":i.target_part,
        "before":before_ref,"before_detail":detail_ref,"before_features":inventory_ref,"call":call_ref,
        "after":after_ref,"after_detail":after_detail_ref,"after_features":after_features_ref,"lineage":lineage_ref,
        "saved":saved_ref,"saved_detail":saved_detail_ref,"saved_features":saved_features_ref,"saved_inventory":saved_inventory_ref,
        "before_reference_sets":sets_ref,"after_reference_sets":after_sets_ref,"saved_reference_sets":saved_sets_ref,
        "checked":checked,"independent_reopen_checked":false,"engineering_adoption":"not_assessed"}),
    )
}
