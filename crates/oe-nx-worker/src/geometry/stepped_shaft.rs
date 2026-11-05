//! Source-bound stepped solids in the existing v2 NX owner.
use super::{NativeApi, Workspace, input_relative, stepped_profile::SteppedProfile};
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, NxSteppedShaftIntent};
use oe_core::{json::strict_json, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct DrawingAxialHole {
    id: String,
    origin_mm: [f64; 3],
    height_mm: f64,
    diameter_mm: f64,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct DrawingRectangularSlot {
    id: String,
    corner_mm: [f64; 3],
    lengths_mm: [f64; 3],
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct DrawingAdditiveCone {
    id: String,
    origin_mm: [f64; 3],
    height_mm: f64,
    diameters_mm: [f64; 2],
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
struct DrawingPostFeatures {
    #[serde(default)]
    schema: String,
    #[serde(default)]
    additive_cones: Vec<DrawingAdditiveCone>,
    #[serde(default)]
    axial_holes: Vec<DrawingAxialHole>,
    #[serde(default)]
    rectangular_slots: Vec<DrawingRectangularSlot>,
}

fn post_features(raw: &[u8]) -> Result<DrawingPostFeatures> {
    let value: Value = serde_json::from_slice(raw).context("drawing_post_features_json")?;
    let Some(value) = value.get("post_features") else {
        return Ok(DrawingPostFeatures::default());
    };
    let features: DrawingPostFeatures = serde_json::from_value(value.clone())
        .context("drawing_post_features_decode")?;
    ensure!(
        features.schema == "oev2.nx-drawing-shaft-post-features/1",
        "drawing_post_features_schema"
    );
    ensure!(
        features.additive_cones.len() <= 32
            && features.axial_holes.len() <= 128
            && features.rectangular_slots.len() <= 32,
        "drawing_post_features_budget"
    );
    for cone in &features.additive_cones {
        ensure!(
            !cone.id.is_empty()
                && cone.id.len() <= 64
                && cone.id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                && cone.origin_mm.iter().all(|v| v.is_finite())
                && cone.height_mm.is_finite()
                && cone.height_mm > 0.0
                && cone.height_mm <= 100_000.0
                && cone.diameters_mm.iter().all(|v| v.is_finite() && *v > 0.0 && *v <= 100_000.0),
            "drawing_additive_cone_invalid"
        );
    }
    for hole in &features.axial_holes {
        ensure!(
            !hole.id.is_empty()
                && hole.id.len() <= 64
                && hole.id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                && hole.origin_mm.iter().all(|v| v.is_finite())
                && hole.height_mm.is_finite()
                && hole.height_mm > 0.0
                && hole.height_mm <= 100_000.0
                && hole.diameter_mm.is_finite()
                && hole.diameter_mm > 0.0
                && hole.diameter_mm <= 100_000.0,
            "drawing_axial_hole_invalid"
        );
    }
    for slot in &features.rectangular_slots {
        ensure!(
            !slot.id.is_empty()
                && slot.id.len() <= 64
                && slot.id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                && slot.corner_mm.iter().all(|v| v.is_finite())
                && slot.lengths_mm.iter().all(|v| v.is_finite() && *v > 0.0 && *v <= 100_000.0),
            "drawing_rectangular_slot_invalid"
        );
    }
    Ok(features)
}

pub const PARAMS_SCHEMA: &str = "oev2.nx-stepped-shaft-params/1";
pub const INTENT_BYTES: usize = 128 * 1024;

pub fn validate(intent: &NxSteppedShaftIntent) -> Result<SteppedProfile> {
    intent
        .validate()
        .map_err(|_| anyhow::anyhow!("stepped_generated_intent_invalid"))?;
    intent
        .object
        .validate()
        .map_err(|_| anyhow::anyhow!("stepped_object_invalid"))?;
    ensure!(
        intent.schema == "oev2.nx-stepped-shaft-intent/1"
            && intent.unit == "mm"
            && intent.coordinate_system == "absolute_part_cartesian"
            && intent.axis_direction == [0.0, 0.0, 1.0],
        "stepped_frame_invalid"
    );
    ensure!(
        !intent.feature_id.is_empty()
            && intent.feature_id.len() <= 64
            && intent
                .feature_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "stepped_feature_invalid"
    );
    ensure!(
        matches!(
            intent.source_role.as_str(),
            "independent_software_fixture" | "independent_design_declaration"
        ),
        "stepped_source_role_invalid"
    );
    let origin: [f64; 3] = intent
        .origin_mm
        .clone()
        .try_into()
        .map_err(|_| anyhow::anyhow!("stepped_origin_dimension"))?;
    ensure!(
        (2..=32).contains(&intent.segments.len()),
        "stepped_segment_count"
    );
    for segment in &intent.segments {
        segment
            .validate()
            .map_err(|_| anyhow::anyhow!("stepped_segment_invalid"))?;
    }
    SteppedProfile::new(
        origin,
        &intent
            .segments
            .iter()
            .map(|s| (s.diameter_mm, s.length_mm))
            .collect::<Vec<_>>(),
    )
}

pub fn source(
    workspace: &dyn Workspace,
    reference: &ArtifactReference,
) -> Result<(NxSteppedShaftIntent, Vec<u8>)> {
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("stepped_reference_invalid"))?;
    ensure!(
        !reference.path.starts_with("inputs/"),
        "stepped_unprefixed_member_required"
    );
    let path = workspace.input(&input_relative(&reference.path)?)?;
    let raw = read_regular_bytes(&path, INTENT_BYTES)?;
    ensure!(sha256(&raw) == reference.sha256, "stepped_source_changed");
    let intent = serde_json::from_value(strict_json(&raw)?)?;
    validate(&intent)?;
    Ok((intent, raw))
}

fn close(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-6
}
fn vector(v: &Value) -> Result<[f64; 3]> {
    super::model_edges::vector(v).context("stepped_native_vector_invalid")
}
fn tag(v: &Value) -> Result<u32> {
    v.as_u64()
        .and_then(|x| u32::try_from(x).ok())
        .filter(|x| *x != 0)
        .context("stepped_native_tag_invalid")
}
fn axial(v: [f64; 3]) -> bool {
    v[0].abs() <= 1e-9 && v[1].abs() <= 1e-9 && (v[2].abs() - 1.0).abs() <= 1e-9
}

pub fn check(intent: &NxSteppedShaftIntent, observed: &Value) -> Result<Value> {
    let profile = validate(intent)?;
    let info: super::PartInfo = serde_json::from_value(observed.clone())?;
    // The outer profile is checked before drawing-specific subtractive features
    // are applied. A newly created part has no controlled reference set yet;
    // reopened legacy stepped shafts still carry and validate that set.
    if info.controlled_reference_set.found {
        super::require_controlled_refset(&info)?;
    }
    ensure!(
        info.units == "mm"
            && info.solid_count == 1
            && info.solid_tags.len() == 1
            && info.solid_tags[0] != 0
            && info.sheet_count == 0
            && info.components.is_empty()
            && observed["rotating_body_inventory_complete"] == true
            && observed["rotating_body_inventory_tags"] == json!(info.solid_tags),
        "stepped_single_owned_solid_required"
    );
    ensure!(
        info.exact_work_csys_bounds.len() == 1,
        "stepped_exact_bounds_required"
    );
    let b = &info.exact_work_csys_bounds[0];
    let actual =
        super::ExactBounds::new(b.min_corner, b.directions, b.distances)?.absolute_axis_bounds;
    ensure!(
        actual
            .iter()
            .zip(b.absolute_axis_bounds)
            .all(|(a, b)| close(*a, b))
            && actual
                .iter()
                .zip(profile.bounds_mm())
                .all(|(a, b)| close(*a, b)),
        "stepped_bounds_mismatch"
    );

    let n = profile.regions().len();
    let faces = observed["solid_face_inventory"]
        .as_array()
        .context("stepped_faces_missing")?;
    ensure!(
        observed["solid_face_inventory_complete"] == true && faces.len() == 2 * n + 1,
        "stepped_face_count"
    );
    let face_ids: BTreeSet<_> = faces
        .iter()
        .map(|f| tag(&f["face_tag"]))
        .collect::<Result<_>>()?;
    ensure!(face_ids.len() == faces.len(), "stepped_duplicate_face");

    // Select every full rim through the existing complete body/edge/owner reader.
    let mut rim_ids = Vec::new();
    for (i, region) in profile.regions().iter().enumerate() {
        for (end, z) in [region.z_start_mm, region.z_end_mm].into_iter().enumerate() {
            let circle = oe_contracts::CircularEdgeIntent {
                feature_id: format!("{}-{i}-{end}", intent.feature_id),
                center: vec![intent.origin_mm[0], intent.origin_mm[1], z],
                normal: vec![0.0, 0.0, 1.0],
                radius: region.radius_mm,
                linear_tolerance: 1e-6,
                angular_tolerance_radians: 1e-8,
                unit: "mm".into(),
                local_frame: "part_absolute".into(),
                full_circle: true,
            };
            let selection = super::model_edges::select(&circle, observed)?;
            ensure!(
                selection["conclusion"] == "pass",
                "stepped_full_rim_unproven"
            );
            rim_ids.push(tag(&selection["candidate_edge_tags"][0])?);
        }
    }
    let rim_set: BTreeSet<_> = rim_ids.iter().copied().collect();
    ensure!(rim_set.len() == 2 * n, "stepped_duplicate_rim");
    let edges = observed["model_edges"]["entries"]
        .as_array()
        .context("stepped_edges_missing")?;
    let mut edge_arc = BTreeMap::new();
    for e in edges {
        let id = tag(&e["edge_tag"])?;
        let arc = e["query"]["is_arc"]
            .as_bool()
            .context("stepped_edge_query_incomplete")?;
        ensure!(edge_arc.insert(id, arc).is_none(), "stepped_duplicate_edge");
    }
    ensure!(
        edge_arc
            .iter()
            .filter_map(|(id, arc)| arc.then_some(*id))
            .collect::<BTreeSet<_>>()
            == rim_set,
        "stepped_unexpected_arc"
    );
    let mut edge_uses = BTreeMap::<u32, usize>::new();
    let mut cylinder_regions = BTreeSet::new();
    let mut planes = BTreeSet::new();
    for f in faces {
        ensure!(
            f["body_tag"] == info.solid_tags[0]
                && f["query_code"] == 0
                && f["face_edges_complete"] == true
                && f["face_edges_query_code"] == 0
                && f["face_body_query_code"] == 0
                && f["face_body_tag"] == info.solid_tags[0],
            "stepped_face_membership_incomplete"
        );
        let links = f["edge_tags"]
            .as_array()
            .context("stepped_face_edges_missing")?;
        ensure!(
            !links.is_empty() && links.len() <= 4,
            "stepped_face_edge_count"
        );
        let ids: BTreeSet<_> = links.iter().map(tag).collect::<Result<_>>()?;
        ensure!(
            ids.len() == links.len() && ids.iter().all(|id| edge_arc.contains_key(id)),
            "stepped_face_edge_membership_invalid"
        );
        for id in &ids {
            *edge_uses.entry(*id).or_default() += 1;
        }
        let circles: BTreeSet<_> = ids.intersection(&rim_set).copied().collect();
        let point = vector(&f["axis_point"])?;
        ensure!(
            axial(vector(&f["axis_direction"])?),
            "stepped_face_axis_invalid"
        );
        match f["surface_type"].as_i64() {
            Some(16) => {
                ensure!(
                    circles.len() == 2
                        && ids.len() <= 3
                        && close(point[0], intent.origin_mm[0])
                        && close(point[1], intent.origin_mm[1]),
                    "stepped_cylinder_axis_or_edges"
                );
                let matches: Vec<_> = (0..n)
                    .filter(|i| circles == BTreeSet::from([rim_ids[2 * i], rim_ids[2 * i + 1]]))
                    .collect();
                ensure!(matches.len() == 1, "stepped_cylinder_axial_span_mismatch");
                let i = matches[0];
                ensure!(
                    cylinder_regions.insert(i)
                        && f["radius"]
                            .as_f64()
                            .is_some_and(|r| close(r, profile.regions()[i].radius_mm)),
                    "stepped_cylinder_radius_or_duplicate"
                );
            }
            Some(22) => {
                ensure!(circles == ids, "stepped_plane_non_circular_boundary");
                let matches: Vec<_> = (0..=n)
                    .filter(|i| {
                        let (z, expected) = if *i == 0 {
                            (
                                profile.regions()[0].z_start_mm,
                                BTreeSet::from([rim_ids[0]]),
                            )
                        } else if *i == n {
                            (
                                profile.regions()[n - 1].z_end_mm,
                                BTreeSet::from([rim_ids[2 * n - 1]]),
                            )
                        } else {
                            (
                                profile.regions()[*i].z_start_mm,
                                BTreeSet::from([rim_ids[2 * i - 1], rim_ids[2 * i]]),
                            )
                        };
                        close(point[2], z) && circles == expected
                    })
                    .collect();
                ensure!(
                    matches.len() == 1 && planes.insert(matches[0]),
                    "stepped_cap_or_shoulder_mismatch"
                );
            }
            _ => anyhow::bail!("stepped_unexpected_surface"),
        }
    }
    ensure!(
        cylinder_regions.len() == n
            && planes.len() == n + 1
            && edge_uses.len() == edge_arc.len()
            && edge_uses
                .iter()
                .all(|(id, count)| *count == if rim_set.contains(id) { 2 } else { 1 }),
        "stepped_topology_incomplete"
    );
    let mass = &observed["native_volume"];
    let raw = mass["raw47"].as_array().context("stepped_mass_missing")?;
    ensure!(
        raw.len() == 47
            && mass["schema"] == "oev2.nx-native-solid-mass-properties/1"
            && mass["return_code"] == 0
            && mass["unit_selector"] == 4
            && mass["unit"] == "m3"
            && mass["object_type"] == 1
            && mass["body_tags"] == json!(info.solid_tags)
            && raw[1] == mass["volume_m3"]
            && raw[0] == mass["surface_area_m2"],
        "stepped_mass_identity"
    );
    for (v, factor, expected) in [
        (&raw[1], 1e9, profile.volume_mm3()),
        (&raw[0], 1e6, profile.surface_area_mm2()),
    ] {
        ensure!(
            v.as_f64().is_some_and(|v| v.is_finite()
                && v > 0.0
                && (v * factor - expected).abs() <= 1e-5_f64.max(expected * 1e-6)),
            "stepped_mass_mismatch"
        );
    }
    Ok(
        json!({"schema":"oev2.nx-stepped-shaft-geometry-check/1","conclusion":"matches_declared_geometry",
        "object":intent.object,"feature_id":intent.feature_id,"expected_bounds_mm":profile.bounds_mm(),
        "expected_volume_mm3":profile.volume_mm3(),"expected_surface_area_mm2":profile.surface_area_mm2(),
        "segments_checked":n,"face_count":faces.len(),"rim_edge_tags":rim_ids,
        "manufacturing_or_physical_acceptance":"not_assessed"}),
    )
}

pub fn create(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    reference: &ArtifactReference,
    intent: &NxSteppedShaftIntent,
    raw: &[u8],
) -> Result<Value> {
    let profile = validate(intent)?;
    let post = post_features(raw)?;
    let output = workspace.output("stepped-shaft.prt")?;
    let part = api.create_mm_part(&output)?;
    api.set_work_part(part)?;
    let mut target = None;
    let mut features = Vec::new();
    for (segment, region) in intent.segments.iter().zip(profile.regions()) {
        let feature = api.create_cylinder_solid(
            [intent.origin_mm[0], intent.origin_mm[1], region.z_start_mm],
            segment.length_mm,
            segment.diameter_mm,
        )?;
        ensure!(feature != 0, "stepped_feature_null");
        let body = api.feature_body(feature)?;
        ensure!(body != 0, "stepped_body_null");
        if let Some(target) = target {
            ensure!(target != body, "stepped_new_body_not_distinct");
            api.unite_bodies(target, body)?;
        } else {
            target = Some(body);
        }
        features.push(feature);
    }
    let before = api.stepped_part_info(part)?;
    workspace.record("stepped_before_save", before.clone())?;
    let checked = check(intent, &before)?;
    let mut post_feature_tags = Vec::new();
    for cone in &post.additive_cones {
        let feature = api.create_additive_cone(
            cone.origin_mm,
            cone.height_mm,
            cone.diameters_mm,
        )?;
        ensure!(feature != 0, "drawing_additive_cone_feature_null");
        let body = api.feature_body(feature)?;
        ensure!(body != 0, "drawing_additive_cone_body_null");
        if let Some(target) = target {
            ensure!(target != body, "drawing_additive_cone_new_body_not_distinct");
            api.unite_bodies(target, body)?;
        } else {
            target = Some(body);
        }
        post_feature_tags.push(json!({
            "id": cone.id,
            "kind": "additive_cone",
            "feature_tag": feature,
            "origin_mm": cone.origin_mm,
            "height_mm": cone.height_mm,
            "diameters_mm": cone.diameters_mm,
            "tag_scope": "this_process_only"
        }));
    }
    for hole in &post.axial_holes {
        let feature = api.create_subtractive_cylinder(
            hole.origin_mm,
            hole.height_mm,
            hole.diameter_mm,
        )?;
        ensure!(feature != 0, "drawing_axial_hole_feature_null");
        post_feature_tags.push(json!({
            "id": hole.id,
            "kind": "axial_hole",
            "feature_tag": feature,
            "origin_mm": hole.origin_mm,
            "height_mm": hole.height_mm,
            "diameter_mm": hole.diameter_mm,
            "tag_scope": "this_process_only"
        }));
    }
    for slot in &post.rectangular_slots {
        let lengths = slot
            .lengths_mm
            .map(|v| v.to_string());
        let feature = api.create_subtractive_block(slot.corner_mm, lengths)?;
        ensure!(feature != 0, "drawing_rectangular_slot_feature_null");
        post_feature_tags.push(json!({
            "id": slot.id,
            "kind": "rectangular_slot",
            "feature_tag": feature,
            "corner_mm": slot.corner_mm,
            "lengths_mm": slot.lengths_mm,
            "tag_scope": "this_process_only"
        }));
    }
    // The controlled reference set is created only after all geometry is in
    // place, so the saved JT/representation belongs to the final solid.
    api.prepare_controlled_representation(part)?;
    api.save_work_part()?;
    let saved = workspace.require_regular_output("stepped-shaft.prt")?;
    let after = if post_feature_tags.is_empty() {
        api.stepped_part_info(part)?
    } else {
        api.rotating_part_info(part)?
    };
    workspace.record("stepped_after_save", after.clone())?;
    if post_feature_tags.is_empty() {
        check(intent, &after)?;
    } else {
        let final_info: super::PartInfo = serde_json::from_value(after.clone())?;
        super::require_controlled_refset(&final_info)?;
        ensure!(
            final_info.units == "mm"
                && final_info.solid_count == 1
                && final_info.sheet_count == 0,
            "drawing_shaft_final_solid_invalid"
        );
    }
    ensure!(
        source(workspace, reference)?.1 == raw,
        "stepped_source_changed_during_creation"
    );
    Ok(
        json!({"action":"create_stepped_shaft","intent_reference":reference,"intent":intent,
        "intent_bytes_sha256":sha256(raw),"native_feature_tags":features,"tag_scope":"this process only",
        "post_features":post,"post_feature_tags":post_feature_tags,
        "before_save":before,"after_save":after,"geometry_check":checked,
        "saved_part":{"member":"out/stepped-shaft.prt","sha256":sha256(&saved),"bytes":saved.len()},
        "process_id":std::process::id(),"independent_reopen":"required_distinct_process","engineering_adoption":false}),
    )
}

pub fn inspect(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    member: &str,
    reference: &ArtifactReference,
    intent: &NxSteppedShaftIntent,
    raw: &[u8],
) -> Result<Value> {
    let path = workspace.input(&input_relative(member)?)?;
    let original = read_regular_bytes(&path, 128 * 1024 * 1024)?;
    let post = post_features(raw)?;
    let part = api.open_part(&path)?;
    api.set_work_part(part)?;
    let observed = if post.additive_cones.is_empty()
        && post.axial_holes.is_empty()
        && post.rectangular_slots.is_empty()
    {
        api.stepped_part_info(part)?
    } else {
        api.rotating_part_info(part)?
    };
    workspace.record("stepped_independent_inspection", observed.clone())?;
    let checked = if post.additive_cones.is_empty()
        && post.axial_holes.is_empty()
        && post.rectangular_slots.is_empty()
    {
        check(intent, &observed)?
    } else {
        let info: super::PartInfo = serde_json::from_value(observed.clone())?;
        super::require_controlled_refset(&info)?;
        ensure!(
            info.units == "mm" && info.solid_count == 1 && info.sheet_count == 0,
            "drawing_shaft_inspection_solid_invalid"
        );
        json!({
            "schema": "oev2.nx-drawing-shaft-geometry-check/1",
            "conclusion": "single_final_solid_reopened",
            "post_features": post,
            "outer_profile": "declared_by_stepped_shaft_intent; detailed feature topology is outside the legacy stepped checker"
        })
    };
    ensure!(
        read_regular_bytes(&path, 128 * 1024 * 1024)? == original
            && source(workspace, reference)?.1 == raw,
        "stepped_inspection_input_changed"
    );
    Ok(
        json!({"action":"inspect_stepped_shaft","intent_reference":reference,"intent":intent,
        "source_part":{"sha256":sha256(&original),"bytes":original.len()},"observation":observed,"geometry_check":checked,
        "process_id":std::process::id(),"input_saved":false,"engineering_adoption":false,
        "independent_reopen_scope":"Caller/controller must join exact saved bytes and distinct process identity"}),
    )
}
