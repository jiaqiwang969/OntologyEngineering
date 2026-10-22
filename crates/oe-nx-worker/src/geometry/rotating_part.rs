//! One source-bound new solid cylinder. No engineering adoption or journal input.
use super::{NativeApi, Workspace, input_relative};
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, NxCylinderIntent};
use oe_core::{json::strict_json, read_regular_bytes, sha256};
use serde_json::{Value, json};

pub const PARAMS_SCHEMA: &str = "oev2.nx-rotating-part-params/1";
pub const INTENT_SCHEMA: &str = "oev2.nx-cylinder-intent/1";
pub const INTENT_BYTES: usize = 128 * 1024;

pub fn validate(intent: &NxCylinderIntent) -> Result<()> {
    intent
        .validate()
        .map_err(|_| anyhow::anyhow!("cylinder_generated_intent_invalid"))?;
    intent
        .object
        .validate()
        .map_err(|_| anyhow::anyhow!("cylinder_object_identity_invalid"))?;
    ensure!(
        intent.schema == INTENT_SCHEMA
            && intent.unit == "mm"
            && intent.coordinate_system == "absolute_part_cartesian"
            && intent.axis_direction == [0.0, 0.0, 1.0],
        "cylinder_supported_frame_mm_positive_z_only"
    );
    ensure!(
        intent.origin_mm.len() == 3
            && intent
                .origin_mm
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1e6),
        "cylinder_origin_invalid"
    );
    ensure!(
        [intent.diameter_mm, intent.length_mm]
            .into_iter()
            .all(|v| v.is_finite() && v > 0.0 && v <= 1e5),
        "cylinder_dimension_invalid"
    );
    ensure!(
        !intent.feature_id.is_empty()
            && intent.feature_id.len() <= 64
            && intent
                .feature_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        "cylinder_feature_identity_invalid"
    );
    ensure!(
        matches!(
            intent.source_role.as_str(),
            "independent_software_fixture" | "independent_design_declaration"
        ),
        "cylinder_source_role_invalid"
    );
    Ok(())
}

pub fn source(
    workspace: &dyn Workspace,
    reference: &ArtifactReference,
) -> Result<(NxCylinderIntent, Vec<u8>)> {
    reference
        .validate()
        .map_err(|_| anyhow::anyhow!("cylinder_intent_reference_invalid"))?;
    ensure!(
        !reference.path.starts_with("inputs/"),
        "cylinder_intent_requires_unprefixed_worker_member"
    );
    let path = workspace.input(&input_relative(&reference.path)?)?;
    let raw = read_regular_bytes(&path, INTENT_BYTES)?;
    ensure!(
        sha256(&raw) == reference.sha256,
        "cylinder_intent_bytes_changed"
    );
    let intent: NxCylinderIntent = serde_json::from_value(strict_json(&raw)?)?;
    validate(&intent)?;
    Ok((intent, raw))
}

fn close(actual: f64, expected: f64) -> bool {
    actual.is_finite() && expected.is_finite() && (actual - expected).abs() <= 1e-6
}
fn direction_close(actual: f64, expected: f64) -> bool {
    actual.is_finite() && (actual - expected).abs() <= 1e-9
}
fn vector(value: &Value) -> Result<[f64; 3]> {
    let a = value.as_array().context("cylinder_native_vector_missing")?;
    ensure!(a.len() == 3, "cylinder_native_vector_length");
    Ok([
        a[0].as_f64().context("cylinder_native_number")?,
        a[1].as_f64().context("cylinder_native_number")?,
        a[2].as_f64().context("cylinder_native_number")?,
    ])
}

/// Geometric equivalence to the independently supplied intent. Native tags are
/// process-local observations, never a persistent manufacturing feature identity.
pub fn check(intent: &NxCylinderIntent, observation: &Value) -> Result<Value> {
    validate(intent)?;
    let part: super::PartInfo = serde_json::from_value(observation.clone())?;
    super::require_controlled_refset(&part)?;
    ensure!(
        observation["rotating_body_inventory_complete"] == true
            && observation["rotating_body_inventory_tags"] == json!(part.solid_tags),
        "cylinder_complete_owned_body_inventory_missing"
    );
    ensure!(
        part.units == "mm"
            && part.solid_count == 1
            && part.solid_tags.len() == 1
            && part.solid_tags[0] != 0
            && part.sheet_count == 0
            && part.components.is_empty(),
        "cylinder_single_solid_mm_required"
    );
    let origin: [f64; 3] = intent.origin_mm.clone().try_into().unwrap();
    let radius = intent.diameter_mm / 2.0;
    let expected = [
        origin[0] - radius,
        origin[1] - radius,
        origin[2],
        origin[0] + radius,
        origin[1] + radius,
        origin[2] + intent.length_mm,
    ];
    ensure!(
        part.exact_work_csys_bounds.len() == 1,
        "cylinder_exact_bounds_missing"
    );
    let bound = &part.exact_work_csys_bounds[0];
    let recomputed = super::ExactBounds::new(bound.min_corner, bound.directions, bound.distances)?;
    ensure!(
        recomputed
            .absolute_axis_bounds
            .iter()
            .zip(bound.absolute_axis_bounds)
            .all(|(a, b)| close(*a, b)),
        "cylinder_exact_bounds_derived_value_changed"
    );
    ensure!(
        recomputed
            .absolute_axis_bounds
            .iter()
            .zip(expected)
            .all(|(a, b)| close(*a, b)),
        "cylinder_exact_bounds_mismatch"
    );
    let faces = observation["cylindrical_faces"]
        .as_array()
        .context("cylinder_native_face_inventory_missing")?;
    ensure!(
        faces.len() == 1,
        "cylinder_unique_cylindrical_face_required"
    );
    let face = &faces[0];
    let point = vector(&face["axis_point"])?;
    let direction = vector(&face["axis_direction"])?;
    ensure!(
        face["surface_type"] == 16
            && face["body_tag"] == part.solid_tags[0]
            && close(point[0], origin[0])
            && close(point[1], origin[1])
            && direction_close(direction[0], 0.0)
            && direction_close(direction[1], 0.0)
            && direction_close(direction[2].abs(), 1.0)
            && face["radius"].as_f64().is_some_and(|r| close(r, radius)),
        "cylinder_native_face_mismatch"
    );
    let topology = observation["solid_face_inventory"]
        .as_array()
        .context("cylinder_native_topology_missing")?;
    let tags: std::collections::BTreeSet<_> = topology
        .iter()
        .filter_map(|f| f["face_tag"].as_u64().and_then(|n| u32::try_from(n).ok()))
        .collect();
    ensure!(
        observation["solid_face_inventory_complete"] == true
            && tags.len() == 3
            && !tags.contains(&0)
            && topology.len() == 3
            && topology
                .iter()
                .all(|f| f["body_tag"] == part.solid_tags[0] && f["query_code"] == 0)
            && topology.iter().filter(|f| f["surface_type"] == 16).count() == 1
            && topology.iter().filter(|f| f["surface_type"] == 22).count() == 2,
        "cylinder_full_face_topology_mismatch"
    );
    ensure!(
        topology
            .iter()
            .find(|f| f["surface_type"] == 16)
            .is_some_and(|f| f["face_tag"] == face["face_tag"]
                && f["axis_point"] == face["axis_point"]
                && f["axis_direction"] == face["axis_direction"]
                && f["radius"] == face["radius"]),
        "cylinder_face_inventory_disagrees"
    );
    let mut plane_z = Vec::new();
    for plane in topology.iter().filter(|f| f["surface_type"] == 22) {
        let point = vector(&plane["axis_point"])?;
        let normal = vector(&plane["axis_direction"])?;
        ensure!(
            direction_close(normal[0], 0.0)
                && direction_close(normal[1], 0.0)
                && direction_close(normal[2].abs(), 1.0),
            "cylinder_cap_plane_normal_mismatch"
        );
        plane_z.push(point[2]);
    }
    plane_z.sort_by(f64::total_cmp);
    ensure!(
        close(plane_z[0], origin[2]) && close(plane_z[1], origin[2] + intent.length_mm),
        "cylinder_cap_plane_position_mismatch"
    );
    let mut selections = Vec::new();
    let mut selected_tags = Vec::new();
    for (index, z) in [origin[2], origin[2] + intent.length_mm]
        .into_iter()
        .enumerate()
    {
        let circle = oe_contracts::CircularEdgeIntent {
            feature_id: format!("{}-end-{index}", intent.feature_id),
            center: vec![origin[0], origin[1], z],
            normal: vec![0.0, 0.0, 1.0],
            radius,
            linear_tolerance: 1e-6,
            angular_tolerance_radians: 1e-8,
            unit: "mm".into(),
            local_frame: "part_absolute".into(),
            full_circle: true,
        };
        let selected = super::model_edges::select(&circle, observation)?;
        ensure!(
            selected["conclusion"] == "pass",
            "cylinder_unique_full_end_circle_unproven"
        );
        selected_tags.push(selected["candidate_edge_tags"][0].clone());
        selections.push(selected);
    }
    ensure!(
        selected_tags[0] != selected_tags[1],
        "cylinder_end_circle_same_tag"
    );
    let volume = &observation["native_volume"];
    let expected_mm3 = std::f64::consts::PI * radius * radius * intent.length_mm;
    let raw_mass = volume["raw47"]
        .as_array()
        .context("cylinder_mass_raw_missing")?;
    ensure!(
        volume["schema"] == "oev2.nx-native-solid-mass-properties/1"
            && raw_mass.len() == 47
            && raw_mass[1] == volume["volume_m3"]
            && volume["return_code"] == 0
            && volume["unit_selector"] == 4
            && volume["unit"] == "m3"
            && volume["object_type"] == 1
            && volume["body_tags"] == json!(part.solid_tags)
            && volume["volume_m3"].as_f64().is_some_and(|v| v.is_finite()
                && v > 0.0
                && (v * 1e9 - expected_mm3).abs() <= 1e-5_f64.max(expected_mm3 * 1e-6)),
        "cylinder_native_volume_missing_or_mismatch"
    );
    Ok(
        json!({"schema":"oev2.nx-cylinder-geometry-check/1","conclusion":"matches_declared_geometry",
        "object":intent.object,"feature_id":intent.feature_id,"expected_bounds_mm":expected,
        "end_circle_selections":selections,
        "proof_scope":"single solid, exact bounds, complete three-face inventory and two full circular edges",
        "native_volume":volume,"independent_expected_volume_mm3":expected_mm3,
        "manufacturing_or_physical_acceptance":"not_assessed"}),
    )
}

pub fn create(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    reference: &ArtifactReference,
    intent: &NxCylinderIntent,
    raw: &[u8],
) -> Result<Value> {
    let target = workspace.output("cylinder.prt")?;
    let part = api.create_mm_part(&target)?;
    api.set_work_part(part)?;
    let origin = intent.origin_mm.clone().try_into().unwrap();
    let feature = api.create_cylinder_solid(origin, intent.length_mm, intent.diameter_mm)?;
    ensure!(feature != 0, "cylinder_feature_null");
    api.prepare_controlled_representation(part)?;
    let before = api.rotating_part_info(part)?;
    workspace.record("cylinder_before_save", before.clone())?;
    let checked = check(intent, &before)?;
    api.save_work_part()?;
    let saved = workspace.require_regular_output("cylinder.prt")?;
    let after = api.rotating_part_info(part)?;
    workspace.record("cylinder_after_save", after.clone())?;
    check(intent, &after)?;
    ensure!(
        source(workspace, reference)?.1 == raw,
        "cylinder_intent_changed_during_creation"
    );
    Ok(
        json!({"action":"create_cylinder_solid","intent_reference":reference,"intent":intent,
        "intent_bytes_sha256":sha256(raw),"native_feature_tag":feature,"feature_tag_scope":"this process only",
        "before_save":before,"after_save":after,"geometry_check":checked,
        "saved_part":{"member":"out/cylinder.prt","sha256":sha256(&saved),"bytes":saved.len()},
        "process_id":std::process::id(),
        "independent_reopen":"required_distinct_process","engineering_adoption":false,"engineering_verdict":"not_assessed"}),
    )
}

pub fn inspect(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    part_name: &str,
    reference: &ArtifactReference,
    intent: &NxCylinderIntent,
    raw: &[u8],
) -> Result<Value> {
    let path = workspace.input(&input_relative(part_name)?)?;
    let original = read_regular_bytes(&path, 128 * 1024 * 1024)?;
    let part = api.open_part(&path)?;
    api.set_work_part(part)?;
    let observed = api.rotating_part_info(part)?;
    workspace.record("cylinder_independent_inspection", observed.clone())?;
    let checked = check(intent, &observed)?;
    ensure!(
        read_regular_bytes(&path, 128 * 1024 * 1024)? == original
            && source(workspace, reference)?.1 == raw,
        "cylinder_inspection_input_changed"
    );
    Ok(
        json!({"action":"inspect_cylinder_solid","intent_reference":reference,"intent":intent,
        "source_part":{"sha256":sha256(&original),"bytes":original.len()},"observation":observed,
        "geometry_check":checked,"process_id":std::process::id(),"input_saved":false,
        "independent_reopen_scope":"Caller/controller must join exact prior saved bytes and distinct process identity",
        "engineering_adoption":false,"engineering_verdict":"not_assessed"}),
    )
}
