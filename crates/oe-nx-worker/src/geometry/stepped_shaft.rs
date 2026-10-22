//! Source-bound stepped solids in the existing v2 NX owner.
use super::{NativeApi, Workspace, input_relative, stepped_profile::SteppedProfile};
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, NxSteppedShaftIntent};
use oe_core::{json::strict_json, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

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
    super::require_controlled_refset(&info)?;
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
    api.prepare_controlled_representation(part)?;
    let before = api.stepped_part_info(part)?;
    workspace.record("stepped_before_save", before.clone())?;
    let checked = check(intent, &before)?;
    api.save_work_part()?;
    let saved = workspace.require_regular_output("stepped-shaft.prt")?;
    let after = api.stepped_part_info(part)?;
    workspace.record("stepped_after_save", after.clone())?;
    check(intent, &after)?;
    ensure!(
        source(workspace, reference)?.1 == raw,
        "stepped_source_changed_during_creation"
    );
    Ok(
        json!({"action":"create_stepped_shaft","intent_reference":reference,"intent":intent,
        "intent_bytes_sha256":sha256(raw),"native_feature_tags":features,"tag_scope":"this process only",
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
    let part = api.open_part(&path)?;
    api.set_work_part(part)?;
    let observed = api.stepped_part_info(part)?;
    workspace.record("stepped_independent_inspection", observed.clone())?;
    let checked = check(intent, &observed)?;
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
