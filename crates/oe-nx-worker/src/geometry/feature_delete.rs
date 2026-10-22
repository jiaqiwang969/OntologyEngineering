//! Explicit P05: delete a uniquely observed cylindrical hole feature in an
//! owned source closure. Missing geometry, vendor lineage and adoption differ.
use super::{NativeApi, Workspace, model_edges, motion_task};
use crate::{assembly, assembly_reference_sets, motion_source};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CircularEdgeIntentV2, NxAssemblyBudget, NxAssemblyObservation,
    NxAssemblyReferenceSets2, NxFeatureDeleteIntent1, NxFeatureDeleteOutcome1, NxFeatureInventory1,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub const ACTION: &str = "delete_hole_feature";
pub const PARAMS_SCHEMA: &str = "oev2.nx-feature-delete-params/1";
pub const INTENT_SCHEMA: &str = "oev2.nx-feature-delete-intent/1";
pub const INVENTORY_SCHEMA: &str = "oev2.nx-feature-inventory/1";
pub const OUTCOME_SCHEMA: &str = "oev2.nx-feature-delete-outcome/1";
pub const MAX_FEATURES: usize = 128;

// Public observation checks may be called without the execution/source gate.
// Validate vector shapes and numeric inputs before indexing or projecting the
// top-circle selector onto the plate's two end planes.
fn validate_geometry_input(i: &NxFeatureDeleteIntent1) -> Result<()> {
    ensure!(
        i.plate_bounds_mm.len() == 6
            && i.feature_location_mm.len() == 3
            && i.plate_bounds_mm
                .iter()
                .chain(&i.feature_location_mm)
                .all(|n| n.is_finite()),
        "delete_geometry_intent_invalid"
    );
    model_edges::validate_v2(&i.target)?;
    model_edges::validate_v2(&i.retained)?;
    Ok(())
}

pub fn validate(i: &NxFeatureDeleteIntent1, b: &NxAssemblyBudget) -> Result<()> {
    i.validate()
        .map_err(|e| anyhow::anyhow!("delete_intent_contract:{e:?}"))?;
    i.object
        .validate()
        .map_err(|e| anyhow::anyhow!("delete_object_contract:{e:?}"))?;
    assembly::budget_valid(b)?;
    motion_source::validate_source_closure(&i.source)?;
    ensure!(
        i.schema == INTENT_SCHEMA && i.feature_type == "CYLINDER",
        "delete_profile_or_type_unsupported"
    );
    validate_geometry_input(i)?;
    for a in 0..3 {
        ensure!(
            i.plate_bounds_mm[a + 3] > i.plate_bounds_mm[a],
            "delete_plate_extent_invalid"
        );
    }
    ensure!(
        !i.source_refs.is_empty()
            && i.source_refs.len() <= 4096
            && i.source_refs.iter().all(|s| !s.trim().is_empty())
            && i.source_refs.iter().collect::<BTreeSet<_>>().len() == i.source_refs.len(),
        "delete_sources_invalid"
    );
    let mut target = 0;
    for f in &i.source.files {
        super::input_relative(&f.path)?;
        ensure!(
            f.path.to_ascii_lowercase().ends_with(".prt"),
            "delete_prt_closure_required"
        );
        if f.path == i.target_part {
            target += 1;
            ensure!(
                f.sha256 == i.object.source_sha256,
                "delete_target_source_identity_differs"
            );
        }
    }
    ensure!(
        target == 1 && i.target_part != i.source.root_part,
        "delete_target_prototype_not_unique"
    );
    for circle in [&i.target, &i.retained] {
        ensure!(
            circle.unit == "mm"
                && circle.local_frame == "part_absolute"
                && circle.full_circle
                && circle.center_tolerance <= 1e-6
                && circle.radius_tolerance <= 1e-6
                && circle.center[2] == i.plate_bounds_mm[5]
                && circle.normal[0] == 0.
                && circle.normal[1] == 0.
                && circle.normal[2].abs() == 1.,
            "delete_vertical_top_circle_profile_required"
        );
        for a in 0..2 {
            ensure!(
                circle.center[a] - circle.radius > i.plate_bounds_mm[a]
                    && circle.center[a] + circle.radius < i.plate_bounds_mm[a + 3],
                "delete_hole_not_inside_plate"
            );
        }
    }
    ensure!(
        i.target.feature_id != i.retained.feature_id
            && (i.target.center[0] - i.retained.center[0])
                .hypot(i.target.center[1] - i.retained.center[1])
                > i.target.radius + i.retained.radius,
        "delete_holes_overlap_or_identity_reused"
    );
    ensure!(
        i.feature_location_mm[0] == i.target.center[0]
            && i.feature_location_mm[1] == i.target.center[1]
            && i.feature_location_mm[2] < i.plate_bounds_mm[2],
        "delete_cylinder_feature_location_invalid"
    );
    Ok(())
}
pub fn source(
    w: &dyn Workspace,
    r: &ArtifactReference,
    b: &NxAssemblyBudget,
) -> Result<NxFeatureDeleteIntent1> {
    let i = motion_task::read(w, r, 4 * 1024 * 1024)?;
    validate(&i, b)?;
    Ok(i)
}
fn finite_vec(v: &Value, n: usize) -> Result<Vec<f64>> {
    let a = v.as_array().context("delete_observation_vector_missing")?;
    ensure!(a.len() == n, "delete_observation_vector_size");
    a.iter()
        .map(|x| {
            x.as_f64()
                .filter(|n| n.is_finite())
                .context("delete_observation_nonfinite")
        })
        .collect()
}
fn tag(v: &Value) -> Result<u64> {
    v.as_u64()
        .filter(|n| *n > 0 && *n <= u32::MAX as u64)
        .context("delete_observation_tag_invalid")
}
fn tol(i: &NxFeatureDeleteIntent1) -> f64 {
    i.target.center_tolerance.max(i.retained.center_tolerance)
}
// UF_MODL_ask_face_data promises an enclosing box, not minimum bounds.
fn encloses(actual: &[f64], expected: &[f64], tolerance: f64) -> bool {
    (0..3).all(|a| {
        actual[a] <= actual[a + 3]
            && actual[a] <= expected[a] + tolerance
            && actual[a + 3] >= expected[a + 3] - tolerance
    })
}
fn volume(i: &NxFeatureDeleteIntent1, deleted: bool) -> f64 {
    let p = &i.plate_bounds_mm;
    let h = p[5] - p[2];
    ((p[3] - p[0]) * (p[4] - p[1]) * h
        - std::f64::consts::PI
            * h
            * (i.retained.radius.powi(2) + if deleted { 0. } else { i.target.radius.powi(2) }))
        * 1e-9
}
fn face_is_hole(f: &Value, c: &CircularEdgeIntentV2, bounds: &[f64]) -> Result<bool> {
    if f["surface_type"] != 16 {
        return Ok(false);
    }
    let p = finite_vec(&f["axis_point"], 3)?;
    let d = finite_vec(&f["axis_direction"], 3)?;
    let b = finite_vec(&f["face_bounds"], 6)?;
    let r = f["radius"]
        .as_f64()
        .filter(|r| r.is_finite())
        .context("delete_face_radius_invalid")?;
    let expected = [
        c.center[0] - c.radius,
        c.center[1] - c.radius,
        bounds[2],
        c.center[0] + c.radius,
        c.center[1] + c.radius,
        bounds[5],
    ];
    Ok(
        (p[0] - c.center[0]).hypot(p[1] - c.center[1]) <= c.center_tolerance
            && d[0].abs() <= c.angular_tolerance_radians
            && d[1].abs() <= c.angular_tolerance_radians
            && (d[2].abs() - 1.).abs() <= 1e-12
            && (r - c.radius).abs() <= c.radius_tolerance
            && encloses(&b, &expected, c.center_tolerance.max(c.radius_tolerance))
            && matches!(f["normal_direction"].as_i64(), Some(-1 | 1)),
    )
}
/// Complete finite two-hole plate profile. Does not identify a persistent NX tag.
/// `deleted=true` requires the original target's complete candidate domain empty.
pub fn check_geometry(i: &NxFeatureDeleteIntent1, detail: &Value, deleted: bool) -> Result<Value> {
    validate_geometry_input(i)?;
    ensure!(
        detail["units"] == "mm" && detail["solid_count"] == 1 && detail["sheet_count"] == 0,
        "delete_part_units_or_body_count_differs"
    );
    let body = tag(&detail["solid_tags"][0])?;
    ensure!(
        detail["solid_tags"]
            .as_array()
            .is_some_and(|a| a.len() == 1),
        "delete_solid_domain_incomplete"
    );
    ensure!(
        detail["exact_work_csys_bounds"]
            .as_array()
            .is_some_and(|a| a.len() == 1),
        "delete_exact_bounds_domain_incomplete"
    );
    let bounds: super::ExactBounds =
        serde_json::from_value(detail["exact_work_csys_bounds"][0].clone())
            .context("delete_exact_bounds_fields_invalid")?;
    let recomputed =
        super::ExactBounds::new(bounds.min_corner, bounds.directions, bounds.distances)?;
    // The axis-aligned projection is derived data. Verify the original native
    // corner/frame/distances with the existing owner before trusting it.
    ensure!(
        recomputed.absolute_axis_bounds == bounds.absolute_axis_bounds,
        "delete_exact_bounds_derived_value_changed"
    );
    ensure!(
        recomputed
            .absolute_axis_bounds
            .iter()
            .zip(&i.plate_bounds_mm)
            .all(|(a, b)| (a - b).abs() <= tol(i)),
        "delete_plate_outer_bounds_changed"
    );
    let mut checks = Vec::new();
    let mut expected_edges = BTreeSet::new();
    for (c, absent) in [(&i.target, deleted), (&i.retained, false)] {
        for z in [i.plate_bounds_mm[2], i.plate_bounds_mm[5]] {
            let mut selector = c.clone();
            selector.center[2] = z;
            let q = model_edges::select_v2(&selector, detail)?;
            if absent {
                ensure!(
                    q["conclusion"] == "violated"
                        && q["reason"] == "model_circle_no_edge_matches"
                        && q["candidate_edge_tags"]
                            .as_array()
                            .is_some_and(Vec::is_empty),
                    "delete_target_circle_not_proven_missing"
                );
            } else {
                ensure!(
                    q["conclusion"] == "pass",
                    "delete_required_circle_missing_or_ambiguous"
                );
                ensure!(
                    expected_edges.insert(tag(&q["candidate_edge_tags"][0])?),
                    "delete_circle_identity_aliased"
                );
            }
            checks.push(q);
        }
    }
    let edges = detail["model_edges"]["entries"]
        .as_array()
        .context("delete_edges_missing")?;
    let arcs: BTreeSet<_> = edges
        .iter()
        .filter(|e| e["query"]["is_arc"] == true)
        .map(|e| tag(&e["edge_tag"]))
        .collect::<Result<_>>()?;
    ensure!(
        arcs == expected_edges,
        "delete_unexpected_circular_geometry"
    );
    ensure!(
        detail["solid_face_inventory_complete"] == true,
        "delete_face_inventory_incomplete"
    );
    let faces = detail["solid_face_inventory"]
        .as_array()
        .context("delete_faces_missing")?;
    ensure!(
        faces.len() == if deleted { 7 } else { 8 },
        "delete_face_domain_count_differs"
    );
    let mut face_ids = BTreeSet::new();
    let mut planes = BTreeSet::new();
    let mut holes = Vec::new();
    for f in faces {
        ensure!(
            f["query_code"] == 0
                && tag(&f["body_tag"])? == body
                && face_ids.insert(tag(&f["face_tag"])?),
            "delete_face_domain_invalid"
        );
        let p = finite_vec(&f["axis_point"], 3)?;
        let dir = finite_vec(&f["axis_direction"], 3)?;
        let bounds = finite_vec(&f["face_bounds"], 6)?;
        ensure!(
            matches!(f["normal_direction"].as_i64(), Some(-1 | 1)),
            "delete_face_normal_unknown"
        );
        if f["surface_type"] == 22 {
            let mut matched = Vec::new();
            for a in 0..3 {
                for side in 0..2 {
                    let v = i.plate_bounds_mm[a + side * 3];
                    let mut footprint = i.plate_bounds_mm.clone();
                    footprint[a] = v;
                    footprint[a + 3] = v;
                    if (p[a] - v).abs() <= tol(i)
                        && (dir[a].abs() - 1.).abs() <= 1e-12
                        && (0..3).filter(|k| *k != a).all(|k| dir[k].abs() <= 1e-12)
                        && encloses(&bounds, &footprint, tol(i))
                    {
                        matched.push((a, side));
                    }
                }
            }
            ensure!(
                matched.len() == 1 && planes.insert(matched[0]),
                "delete_exterior_plane_changed"
            );
        } else {
            let target = face_is_hole(f, &i.target, &i.plate_bounds_mm)?;
            let retained = face_is_hole(f, &i.retained, &i.plate_bounds_mm)?;
            ensure!(
                target != retained && (!deleted || !target),
                "delete_unexpected_or_retained_cylinder_changed"
            );
            holes.push(if target { "target" } else { "retained" });
        }
    }
    holes.sort_unstable();
    ensure!(
        planes.len() == 6
            && holes
                == if deleted {
                    vec!["retained"]
                } else {
                    vec!["retained", "target"]
                },
        "delete_full_geometry_profile_differs"
    );
    Ok(
        json!({"target_feature_id":i.target.feature_id,"target_missing":deleted,"retained_feature_id":i.retained.feature_id,
        "body_tag":body,"circle_checks":checks,"complete_face_count":faces.len(),"expected_volume_m3":volume(i,deleted),
        "volume_check":"separate native assembly mass observation required","face_bounds_role":"enclosing only, not minimum extent",
        "geometry_scope":"bounded rectangular plate and two vertical through-hole profile", "engineering_adoption":"not_established"}),
    )
}
/// A body/owner/type/location AND observed current face association are required.
pub fn select_feature(
    i: &NxFeatureDeleteIntent1,
    inventory: &NxFeatureInventory1,
    detail: &Value,
) -> Result<u64> {
    check_geometry(i, detail, false)?;
    ensure!(
        inventory.schema == INVENTORY_SCHEMA
            && inventory.complete
            && inventory.part_tag == tag(&detail["part_tag"])?
            && inventory.body_tag == tag(&detail["solid_tags"][0])?
            && !inventory.features.is_empty()
            && inventory.features.len() <= MAX_FEATURES,
        "delete_feature_inventory_incomplete"
    );
    let faces = detail["solid_face_inventory"]
        .as_array()
        .context("delete_faces_missing")?;
    let face_domain: BTreeSet<_> = faces
        .iter()
        .map(|f| tag(&f["face_tag"]))
        .collect::<Result<_>>()?;
    let target_faces: BTreeSet<_> = faces
        .iter()
        .filter_map(|f| match face_is_hole(f, &i.target, &i.plate_bounds_mm) {
            Ok(true) => Some(tag(&f["face_tag"])),
            Ok(false) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<_>>()?;
    ensure!(
        target_faces.len() == 1,
        "delete_current_target_face_not_unique"
    );
    let mut ids = BTreeSet::new();
    let mut candidates = Vec::new();
    for f in &inventory.features {
        ensure!(
            f.complete
                && f.feature_tag > 0
                && ids.insert(f.feature_tag)
                && !f.is_occurrence
                && f.owning_part_tag == inventory.part_tag
                && f.body_tag == inventory.body_tag
                && f.query_codes == [0, 0, 0, 0, 0]
                && !f.feature_type.is_empty()
                && f.location_mm.len() == 3
                && f.location_mm.iter().all(|n| n.is_finite())
                && f.face_tags.len() <= faces.len()
                && f.face_tags.iter().collect::<BTreeSet<_>>().len() == f.face_tags.len()
                && f.face_tags.iter().all(|t| face_domain.contains(t)),
            "delete_feature_query_partial_or_owner_changed"
        );
        if f.feature_type == i.feature_type
            && f.location_mm
                .iter()
                .zip(&i.feature_location_mm)
                .all(|(a, b)| (a - b).abs() <= tol(i))
            && target_faces.iter().all(|t| f.face_tags.contains(t))
        {
            candidates.push(f.feature_tag);
        }
    }
    ensure!(candidates.len() == 1, "delete_feature_missing_or_ambiguous");
    Ok(candidates[0])
}
pub fn check_call(
    call: &NxFeatureDeleteOutcome1,
    part: u64,
    body: u64,
    feature: u64,
) -> Result<()> {
    ensure!(
        call.schema == OUTCOME_SCHEMA
            && call.part_tag == part
            && call.body_tag == body
            && call.feature_tag == feature
            && call.call_entered
            && call.call_returned
            && call.complete
            && call.return_code == 0
            && call.list_create_code == 0
            && call.list_put_code == 0
            && call.list_count_code == 0
            && call.list_item_code == 0
            && call.list_delete_code == 0
            && call.list_count == 1
            && call.list_item_tag == feature
            && call.update_code == Some(0),
        "delete_native_call_failed_or_incomplete"
    );
    Ok(())
}
/// Original geometry/pose persistence algorithm, with ONLY intended target volume
/// replaced in a local comparison projection after separately checking both values.
pub fn check_assembly(
    i: &NxFeatureDeleteIntent1,
    b: &NxAssemblyBudget,
    before: &NxAssemblyObservation,
    after: &NxAssemblyObservation,
) -> Result<Value> {
    validate(i, b)?;
    assembly::validate_observation(before, b)?;
    assembly::validate_observation(after, b)?;
    let mut comparison = before.clone();
    let p = comparison
        .prototypes
        .iter_mut()
        .find(|p| p.prototype_id == i.target_part)
        .context("delete_target_prototype_missing")?;
    let q = after
        .prototypes
        .iter()
        .find(|p| p.prototype_id == i.target_part)
        .context("delete_target_prototype_missing_after")?;
    ensure!(
        p.native_units == "mm"
            && q.native_units == "mm"
            && p.solids.len() == 1
            && q.solids.len() == 1
            && p.native_part_tag == q.native_part_tag
            && p.solids[0].body_tag == q.solids[0].body_tag,
        "delete_current_target_identity_changed"
    );
    for (p, deleted) in [(&*p, false), (q, true)] {
        ensure!(
            (p.solids[0]
                .volume_m3
                .context("delete_assembly_volume_missing")?
                - volume(i, deleted))
            .abs()
                <= 1e-12,
            "delete_assembly_volume_profile_differs"
        );
    }
    p.solids[0].volume_m3 = q.solids[0].volume_m3;
    assembly::same_geometry(&comparison, after)?;
    for p in &before.prototypes {
        let q = after
            .prototypes
            .iter()
            .find(|q| q.prototype_id == p.prototype_id)
            .context("delete_prototype_missing")?;
        ensure!(
            p.native_part_tag == q.native_part_tag
                && p.part_path == q.part_path
                && p.solids
                    .iter()
                    .map(|s| (s.body_tag, s.owning_part_tag))
                    .eq(q.solids.iter().map(|s| (s.body_tag, s.owning_part_tag))),
            "delete_other_prototype_identity_changed"
        );
    }
    for o in &before.occurrences {
        let q = after
            .occurrences
            .iter()
            .find(|q| q.part_occurrence_tag == o.part_occurrence_tag)
            .context("delete_instance_missing")?;
        ensure!(
            serde_json::to_value(o)? == serde_json::to_value(q)?,
            "delete_instance_relation_changed"
        );
    }
    Ok(
        json!({"target_part":i.target_part,"target_feature_id":i.target.feature_id,"assembly_relations":"preserved_current_process",
        "geometry_volume_policy":"existing assembly persistence 1e-12 m3; original copy1 unchanged","independent_reopen":"required_separate_process"}),
    )
}
pub fn check_reference_sets(
    before: &NxAssemblyObservation,
    a: &NxAssemblyReferenceSets2,
    after: &NxAssemblyObservation,
    b: &NxAssemblyReferenceSets2,
) -> Result<()> {
    assembly_reference_sets::require_entire(a, before)?;
    assembly_reference_sets::require_entire(b, after)?;
    ensure!(
        serde_json::to_value(a)? == serde_json::to_value(b)?,
        "delete_reference_sets_changed"
    );
    Ok(())
}
pub(super) fn step<T>(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    action: impl FnOnce(&mut dyn NativeApi) -> Result<T>,
) -> Result<T> {
    let result = action(api);
    let persisted = api.persist_load_diagnostics(&mut |v| w.record("native_load_status", v));
    match (result, persisted) {
        (Err(e), Err(p)) => Err(e.context(format!("delete_diagnostic_persistence_failed:{p:#}"))),
        (Err(e), _) => Err(e),
        (Ok(v), p) => {
            p?;
            Ok(v)
        }
    }
}
pub(super) fn loaded(
    api: &mut dyn NativeApi,
    dir: &Path,
    i: &NxFeatureDeleteIntent1,
) -> Result<()> {
    let got = api.loaded_assembly_files(dir)?;
    let set: BTreeSet<_> = got.iter().map(String::as_str).collect();
    ensure!(
        set.len() == got.len() && set == i.source.files.iter().map(|f| f.path.as_str()).collect(),
        "delete_loaded_source_closure_differs"
    );
    Ok(())
}
pub fn execute(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    r: &ArtifactReference,
    i: &NxFeatureDeleteIntent1,
    b: &NxAssemblyBudget,
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
    let proto = before
        .prototypes
        .iter()
        .find(|p| p.prototype_id == i.target_part)
        .context("delete_target_prototype_missing")?;
    ensure!(
        proto.solids.len() == 1 && proto.native_units == "mm",
        "delete_target_solid_profile"
    );
    ensure!(
        (proto.solids[0]
            .volume_m3
            .context("delete_before_assembly_volume_missing")?
            - volume(i, false))
        .abs()
            <= 1e-12,
        "delete_before_assembly_volume_profile_differs"
    );
    let (part, body) = (proto.native_part_tag, proto.solids[0].body_tag);
    let before_ref = w.record_bound("feature_delete_before", serde_json::to_value(&before)?)?;
    let sets = step(api, w, |a| a.assembly_reference_sets(&before))?;
    assembly_reference_sets::require_entire(&sets, &before)?;
    let sets_ref = w.record_bound(
        "feature_delete_before_reference_sets",
        serde_json::to_value(&sets)?,
    )?;
    step(api, w, |a| a.set_work_part(u32::try_from(part)?))?;
    let detail = step(api, w, |a| a.detailed_info(u32::try_from(part)?, false))?;
    let detail_ref = w.record_bound("feature_delete_before_detail", detail.clone())?;
    let inventory = step(api, w, |a| a.feature_inventory(part, body))?;
    let inventory_ref = w.record_bound(
        "feature_delete_feature_inventory",
        serde_json::to_value(&inventory)?,
    )?;
    ensure!(
        inventory.part_tag == part && inventory.body_tag == body,
        "delete_feature_inventory_current_part_differs"
    );
    let feature = select_feature(i, &inventory, &detail)?;
    let selection_ref = w.record_bound("feature_delete_selection",json!({"feature_tag":feature,"part_tag":part,"body_tag":body,"target_feature_id":i.target.feature_id,"inventory":inventory_ref,"detail":detail_ref}))?;
    w.verify_motion_source()?;
    loaded(api, &namespace, i)?;
    w.record(
        "feature_delete_pending",
        json!({"intent":r,"feature_tag":feature,"target_part":i.target_part}),
    )?;
    let call = step(api, w, |a| a.delete_feature(part, body, feature))?;
    let call_ref = w.record_bound("feature_delete_returned", serde_json::to_value(&call)?)?;
    let after_detail = step(api, w, |a| a.detailed_info(u32::try_from(part)?, false))?;
    let after_detail_ref = w.record_bound("feature_delete_after_detail", after_detail.clone())?;
    let after = step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    let after_ref = w.record_bound("feature_delete_after", serde_json::to_value(&after)?)?;
    check_call(&call, part, body, feature)?;
    check_geometry(i, &after_detail, true)?;
    let checked = check_assembly(i, b, &before, &after)?;
    let after_sets = step(api, w, |a| a.assembly_reference_sets(&after))?;
    let after_sets_ref = w.record_bound(
        "feature_delete_after_reference_sets",
        serde_json::to_value(&after_sets)?,
    )?;
    check_reference_sets(&before, &sets, &after, &after_sets)?;
    w.verify_motion_source()?;
    loaded(api, &namespace, i)?;
    w.record(
        "feature_delete_save_pending",
        json!({"namespace":namespace}),
    )?;
    let saved = step(api, w, |a| a.save_all_owned_assembly(&namespace))?;
    let saved_ref = w.record_bound("feature_delete_save_returned", saved.clone())?;
    ensure!(
        saved["api"] == "UF_PART_save_all"
            && saved["return_code"] == 0
            && saved["file_error_count"] == 0
            && saved["file_errors"].as_array().is_some_and(Vec::is_empty),
        "delete_save_failed_or_partial"
    );
    let saved_detail = step(api, w, |a| a.detailed_info(u32::try_from(part)?, false))?;
    let saved_detail_ref = w.record_bound("feature_delete_saved_detail", saved_detail.clone())?;
    check_geometry(i, &saved_detail, true)?;
    let saved_inventory = step(api, w, |a| {
        a.full_assembly_info(root, &namespace, b, "reference_observation")
    })?;
    let saved_inventory_ref = w.record_bound(
        "feature_delete_saved_inventory",
        serde_json::to_value(&saved_inventory)?,
    )?;
    check_assembly(i, b, &before, &saved_inventory)?;
    let saved_sets = step(api, w, |a| a.assembly_reference_sets(&saved_inventory))?;
    let saved_sets_ref = w.record_bound(
        "feature_delete_saved_reference_sets",
        serde_json::to_value(&saved_sets)?,
    )?;
    check_reference_sets(&before, &sets, &saved_inventory, &saved_sets)?;
    w.verify_motion_source()?;
    loaded(api, &namespace, i)?;
    Ok(
        json!({"action":ACTION,"object":i.object,"intent":r,"target_part":i.target_part,
        "before":before_ref,"before_detail":detail_ref,"feature_inventory":inventory_ref,"selection":selection_ref,"call":call_ref,
        "after":after_ref,"after_detail":after_detail_ref,"saved":saved_ref,"saved_detail":saved_detail_ref,"saved_inventory":saved_inventory_ref,
        "before_reference_sets":sets_ref,"after_reference_sets":after_sets_ref,"saved_reference_sets":saved_sets_ref,"checked":checked,
        "independent_reopen_checked":false,"engineering_adoption":"not_assessed"}),
    )
}
