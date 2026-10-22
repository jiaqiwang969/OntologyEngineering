//! Finite SolidWorks worker. Process/file owners are shared with the Windows
//! controller; engineering interpretation remains in the existing CAD/Project owners.
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    SolidWorksNativeBody, SolidWorksNativeFilePin, SolidWorksNativeObservation,
    SolidWorksNativeRequest,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

#[cfg(windows)]
pub mod com;
#[cfg(windows)]
pub mod installation;
#[cfg(windows)]
pub mod native;
#[cfg(windows)]
pub mod owned;
#[cfg(windows)]
pub mod session_host;

pub const MAX_MODEL_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn hash_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// Native CAD path spelling only. File owners keep their canonical handles and
/// full identities; no path-text conversion establishes file ownership.
pub fn cad_api_path(value: &str) -> Result<String> {
    let path = if let Some(unc) = value.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        value.strip_prefix("\\\\?\\").unwrap_or(value).to_owned()
    };
    let drive = path.as_bytes();
    let dos = drive.len() >= 3
        && drive[0].is_ascii_alphabetic()
        && drive[1] == b':'
        && matches!(drive[2], b'\\' | b'/');
    let unc = if path.starts_with("\\\\")
        && !path.starts_with("\\\\?\\")
        && !path.starts_with("\\\\.\\")
    {
        let mut parts = path[2..].split('\\');
        parts.next().is_some_and(|s| !s.is_empty()) && parts.next().is_some_and(|s| !s.is_empty())
    } else {
        false
    };
    ensure!(
        (dos || unc) && !path.contains('\0') && path.encode_utf16().count() < 260,
        "solidworks_native_api_path_not_qualified_or_overlong"
    );
    Ok(path)
}
fn pin_valid(pin: &SolidWorksNativeFilePin, limit: u64) -> Result<()> {
    ensure!(
        !pin.path.is_empty()
            && !pin.path.contains('\0')
            && pin.bytes > 0
            && pin.bytes <= limit
            && hash_valid(&pin.sha256),
        "solidworks_file_pin_invalid"
    );
    Ok(())
}
pub fn validate_request(r: &SolidWorksNativeRequest) -> Result<()> {
    r.validate()
        .map_err(|_| anyhow::anyhow!("solidworks_generated_request_invalid"))?;
    match r.schema.as_str() {
        "oev2.solidworks-native-request/1" => ensure!(
            r.execution_profile.is_none(),
            "solidworks_v1_profile_cannot_change_ownership_rule"
        ),
        "oev2.solidworks-native-request/2" => ensure!(
            r.execution_profile.as_deref() == Some("owned_private_desktop"),
            "solidworks_v2_requires_explicit_private_desktop_profile"
        ),
        _ => anyhow::bail!("solidworks_request_schema"),
    }
    ensure!(
        (16..=96).contains(&r.job_id.len())
            && r.job_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "solidworks_job_nonce_invalid"
    );
    ensure!(
        matches!(
            r.action.as_str(),
            "api_check" | "inspect_part" | "create_plate" | "update_thickness"
        ),
        "solidworks_action_unsupported"
    );
    for pin in [
        &r.installation.executable,
        &r.installation.type_library,
        &r.installation.constants_library,
    ] {
        pin_valid(pin, 32 * 1024 * 1024)?;
    }
    ensure!(
        r.installation.major_version == 33,
        "solidworks_installation_version_not_qualified"
    );
    if let Some(template) = &r.installation.template {
        pin_valid(template, MAX_MODEL_BYTES)?;
    }
    if r.action != "api_check" {
        for reference in [&r.project, &r.work] {
            let reference = reference
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("solidworks_project_work_binding_required"))?;
            ensure!(
                hash_valid(&reference.sha256)
                    && !reference.path.is_empty()
                    && !reference.path.contains(['\\', ':', '\0']),
                "solidworks_project_reference_invalid"
            );
            ensure!(
                Path::new(&reference.path)
                    .components()
                    .all(|p| matches!(p, std::path::Component::Normal(_))),
                "solidworks_project_reference_not_relative"
            );
        }
    }
    ensure!(
        !r.configuration.contains('\0') && r.configuration.len() <= 256,
        "solidworks_configuration_invalid"
    );
    if let Some(input) = &r.input {
        pin_valid(input, MAX_MODEL_BYTES)?;
        ensure!(
            input.path.to_ascii_lowercase().ends_with(".sldprt"),
            "solidworks_only_independent_part_supported"
        );
    }
    if let Some(name) = &r.output_name {
        ensure!(
            name.len() <= 120
                && name.to_ascii_lowercase().ends_with(".sldprt")
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                && !name.starts_with('.'),
            "solidworks_output_name_invalid"
        );
    }
    if let Some(p) = &r.plate {
        ensure!(
            [p.width_m, p.height_m, p.thickness_m]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.00001 && *v <= 10.0),
            "solidworks_plate_dimensions_invalid"
        );
        ensure!(
            p.bore_diameter_m.is_finite()
                && p.bore_diameter_m >= 0.0
                && p.bore_diameter_m < p.width_m.min(p.height_m),
            "solidworks_bore_invalid"
        );
        ensure!(
            !p.reference_plane.trim().is_empty()
                && !p.reference_plane.contains('\0')
                && p.reference_plane.len() <= 128,
            "solidworks_reference_plane_missing"
        );
        ensure!(
            !p.feature_name.is_empty()
                && p.feature_name.len() <= 64
                && p.feature_name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "solidworks_feature_name_invalid"
        );
    }
    match r.action.as_str() {
        "api_check" => ensure!(
            r.input.is_none()
                && r.output_name.is_none()
                && r.plate.is_none()
                && r.new_thickness_m.is_none(),
            "solidworks_api_check_cannot_request_geometry"
        ),
        "inspect_part" => ensure!(
            r.input.is_some()
                && r.output_name.is_none()
                && r.plate.is_none()
                && r.new_thickness_m.is_none()
                && !r.configuration.is_empty(),
            "solidworks_readonly_inspect_shape"
        ),
        "create_plate" => ensure!(
            r.input.is_none()
                && r.output_name.is_some()
                && r.plate.is_some()
                && r.installation.template.is_some()
                && r.new_thickness_m.is_none(),
            "solidworks_create_shape"
        ),
        "update_thickness" => {
            ensure!(
                r.input.is_some()
                    && r.output_name.is_some()
                    && r.plate.is_some()
                    && !r.configuration.is_empty(),
                "solidworks_update_shape"
            );
            let value = r
                .new_thickness_m
                .ok_or_else(|| anyhow::anyhow!("solidworks_update_depth_missing"))?;
            ensure!(
                value.is_finite() && (0.00001..=10.0).contains(&value),
                "solidworks_update_depth_invalid"
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}

/// Record consistency only. This does not authenticate a producer or establish
/// real execution from an arbitrary JSON assertion.
pub fn check_observation(
    r: &SolidWorksNativeRequest,
    raw_request: &[u8],
    o: &SolidWorksNativeObservation,
) -> Result<Value> {
    validate_request(r)?;
    o.validate()
        .map_err(|_| anyhow::anyhow!("solidworks_generated_observation_invalid"))?;
    ensure!(
        o.schema == observation_schema(r)
            && o.job_id == r.job_id
            && o.action == r.action
            && o.request_sha256 == sha256(raw_request),
        "solidworks_observation_request_mismatch"
    );
    ensure!(
        matches!(o.execution.as_str(), "api_only" | "completed" | "failed"),
        "solidworks_observation_execution_invalid"
    );
    for process in std::iter::once(&o.worker).chain(o.cad.iter()) {
        ensure!(
            (1..=u64::from(u32::MAX)).contains(&process.pid)
                && process.creation_filetime > 0
                && process.session_id <= u64::from(u32::MAX),
            "solidworks_process_record_invalid"
        );
        pin_valid(&process.image, MAX_MODEL_BYTES)?;
    }
    ensure!(o.input == r.input, "solidworks_observation_input_mismatch");
    if o.execution == "api_only" {
        ensure!(
            r.action == "api_check"
                && o.cad.is_none()
                && o.bodies.is_empty()
                && o.output.is_none()
                && !o.cad_exit_observed
                && o.errors.is_empty(),
            "solidworks_api_only_shape"
        );
    }
    ensure!(o.bodies.len() <= 256, "solidworks_body_inventory_budget");
    for (i, b) in o.bodies.iter().enumerate() {
        ensure!(
            b.index as usize == i
                && b.face_count > 0
                && b.face_count <= 1_000_000
                && b.extreme_points_m.len() == 18
                && b.mass_properties_native.len() == 12,
            "solidworks_body_shape_invalid"
        );
        ensure!(
            b.extreme_points_m
                .iter()
                .chain(&b.mass_properties_native)
                .all(|v| v.is_finite())
                && b.mass_properties_native[3] > 0.0,
            "solidworks_geometry_nonfinite_or_empty"
        );
        if let Some(faces) = &b.faces {
            ensure!(
                faces.len() as u64 == b.face_count && faces.len() <= 4096,
                "solidworks_face_inventory_incomplete"
            );
            for (index, face) in faces.iter().enumerate() {
                let size = match face.kind.as_str() {
                    "plane" => 6,
                    "cylinder" => 7,
                    "other" => 0,
                    _ => anyhow::bail!("solidworks_face_type_invalid"),
                };
                ensure!(
                    face.index as usize == index
                        && face.parameters_native.len() == size
                        && face.parameters_native.iter().all(|v| v.is_finite()),
                    "solidworks_face_parameters_invalid"
                );
                for edge in &face.circular_edges {
                    ensure!(
                        edge.parameters_native.len() == 7
                            && edge.parameters_native.iter().all(|v| v.is_finite())
                            && edge.parameters_native[6] > 0.0,
                        "solidworks_circle_parameters_invalid"
                    );
                }
            }
        }
    }
    if o.execution == "completed" {
        ensure!(
            r.action != "api_check"
                && o.errors.is_empty()
                && o.cad.is_some()
                && o.cad_exit_observed
                && !o.bodies.is_empty()
                && o.configuration == r.configuration,
            "solidworks_completed_observation_incomplete"
        );
        let cad = o.cad.as_ref().unwrap();
        ensure!(
            cad.image.sha256 == r.installation.executable.sha256
                && cad.image.bytes == r.installation.executable.bytes,
            "solidworks_cad_image_mismatch"
        );
        if r.action == "inspect_part" {
            ensure!(
                o.output.is_none(),
                "solidworks_inspect_cannot_produce_saved_model"
            );
        } else {
            let output = o
                .output
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("solidworks_saved_model_missing"))?;
            pin_valid(output, MAX_MODEL_BYTES)?;
            ensure!(
                o.save["return_value"] == true && o.save["errors"] == 0,
                "solidworks_completed_save_not_successful"
            );
        }
    }
    Ok(
        json!({"schema":"oev2.solidworks-native-record-check/1","record_consistency":"checked","producer_execution_authentication":"not_established_by_json","independent_process_reopen":"not_established","engineering_adoption":"not_assessed"}),
    )
}
/// Independent finite bored-plate intent. Old observations without the new
/// inventory remain unknown here; a producer's JSON never authenticates itself.
pub fn check_bore(
    plate: &oe_contracts::SolidWorksNativePlate,
    thickness_m: f64,
    bodies: &[SolidWorksNativeBody],
) -> Result<Value> {
    ensure!(
        bodies.len() == 1,
        "solidworks_plate_requires_one_complete_solid"
    );
    let body = &bodies[0];
    let faces = body
        .faces
        .as_ref()
        .context("solidworks_bore_faces_not_observed")?;
    ensure!(
        faces.len() as u64 == body.face_count,
        "solidworks_bore_face_inventory_incomplete"
    );
    let cylinders: Vec<_> = faces.iter().filter(|f| f.kind == "cylinder").collect();
    if plate.bore_diameter_m == 0.0 {
        ensure!(
            cylinders.is_empty(),
            "solidworks_unrequested_cylindrical_face"
        );
        return Ok(json!({"bore":"not_requested","cylinders":0}));
    }
    ensure!(
        cylinders.len() == 1 && faces.iter().filter(|f| f.kind == "plane").count() == 6,
        "solidworks_bore_face_topology_mismatch"
    );
    let c = cylinders[0];
    let p = &c.parameters_native;
    let tolerance = 1e-10;
    ensure!(
        p.len() == 7 && p.iter().all(|v| v.is_finite()) && c.circular_edges.len() == 2,
        "solidworks_bore_parameters_or_rims_missing"
    );
    let radius = plate.bore_diameter_m / 2.0;
    ensure!(
        (p[0] - plate.width_m / 2.0).abs() <= tolerance
            && (p[1] - plate.height_m / 2.0).abs() <= tolerance
            && p[3].abs() <= tolerance
            && p[4].abs() <= tolerance
            && (p[5].abs() - 1.0).abs() <= tolerance
            && (p[6] - radius).abs() <= tolerance,
        "solidworks_bore_axis_center_or_radius_mismatch"
    );
    let mut rims = Vec::new();
    for circle in &c.circular_edges {
        let p = &circle.parameters_native;
        ensure!(
            p.len() == 7 && p.iter().all(|v| v.is_finite()),
            "solidworks_bore_circle_shape"
        );
        ensure!(
            (p[0] - plate.width_m / 2.0).abs() <= tolerance
                && (p[1] - plate.height_m / 2.0).abs() <= tolerance
                && p[3].abs() <= tolerance
                && p[4].abs() <= tolerance
                && (p[5].abs() - 1.0).abs() <= tolerance
                && (p[6] - radius).abs() <= tolerance,
            "solidworks_bore_circle_geometry_mismatch"
        );
        rims.push(p[2]);
    }
    rims.sort_by(f64::total_cmp);
    ensure!(
        rims[0].abs() <= tolerance && (rims[1] - thickness_m).abs() <= tolerance,
        "solidworks_bore_does_not_reach_both_required_boundary_planes"
    );
    Ok(
        json!({"bore":"through_required_plate_thickness","diameter_m":plate.bore_diameter_m,"thickness_m":thickness_m,"rim_z_m":rims,"faces_observed":faces.len(),"geometry_scope":"finite single cylindrical wall, radius/axis/center/two circular boundary planes; full volume and solid inventory remain separately required","physical_and_engineering_adoption":"not_established"}),
    )
}
/// Cross-record consistency only; actual producer/process provenance must remain
/// attached as original evidence in the existing CAD packet.
pub fn check_reopen(
    first: &SolidWorksNativeRequest,
    first_raw: &[u8],
    saved: &SolidWorksNativeObservation,
    second: &SolidWorksNativeRequest,
    second_raw: &[u8],
    reopened: &SolidWorksNativeObservation,
) -> Result<Value> {
    check_observation(first, first_raw, saved)?;
    check_observation(second, second_raw, reopened)?;
    ensure!(
        matches!(first.action.as_str(), "create_plate" | "update_thickness")
            && second.action == "inspect_part"
            && saved.execution == "completed"
            && reopened.execution == "completed",
        "solidworks_reopen_requires_completed_save_and_inspect"
    );
    ensure!(
        first.job_id != second.job_id
            && first.project == second.project
            && first.work == second.work
            && first.installation == second.installation
            && first.schema == second.schema
            && first.execution_profile == second.execution_profile
            && saved.configuration == reopened.configuration,
        "solidworks_reopen_scope_mismatch"
    );
    ensure!(
        saved.output == second.input && saved.output.is_some(),
        "solidworks_reopen_saved_file_mismatch"
    );
    let a = saved.cad.as_ref().unwrap();
    let b = reopened.cad.as_ref().unwrap();
    ensure!(
        (a.pid, a.creation_filetime) != (b.pid, b.creation_filetime)
            && b.creation_filetime > a.creation_filetime
            && (saved.worker.pid, saved.worker.creation_filetime)
                != (reopened.worker.pid, reopened.worker.creation_filetime),
        "solidworks_reopen_requires_distinct_cad_and_worker_processes"
    );
    ensure!(
        saved.worker.image.sha256 == reopened.worker.image.sha256
            && saved.worker.image.bytes == reopened.worker.image.bytes,
        "solidworks_reopen_worker_version_changed"
    );
    ensure!(
        saved.bodies.len() == reopened.bodies.len(),
        "solidworks_reopen_body_count_changed"
    );
    for (a, b) in saved.bodies.iter().zip(&reopened.bodies) {
        ensure!(
            a.face_count == b.face_count,
            "solidworks_reopen_face_count_changed"
        );
        for i in 0..6 {
            let axis = i / 2;
            ensure!(
                (a.extreme_points_m[3 * i + axis] - b.extreme_points_m[3 * i + axis]).abs()
                    <= 1e-10,
                "solidworks_reopen_axis_extreme_changed"
            );
        }
        let v = a.mass_properties_native[3];
        ensure!(
            (v - b.mass_properties_native[3]).abs() <= 1e-14f64.max(v.abs() * 1e-10),
            "solidworks_reopen_volume_changed"
        );
    }
    let bore = if saved.bodies.iter().all(|b| b.faces.is_some())
        && reopened.bodies.iter().all(|b| b.faces.is_some())
    {
        if let Some(plate) = &first.plate {
            let thickness = first.new_thickness_m.unwrap_or(plate.thickness_m);
            json!({"saved":check_bore(plate,thickness,&saved.bodies)?,"reopened":check_bore(plate,thickness,&reopened.bodies)?})
        } else {
            json!({"execution":"not_checked","reason":"independent_plate_intent_not_provided"})
        }
    } else {
        json!({"execution":"not_observed","reason":"legacy observation has no full face/cylinder/rim inventory; prior limited scope preserved"})
    };
    Ok(
        json!({"schema":"oev2.solidworks-reopen-record-check/1","record_consistency":"checked","saved_model":saved.output,"saved_cad":a,"reopened_cad":b,"producer_execution_authentication":"not_established_by_json","geometry_scope":"solid inventory, face count, axis extrema, volume and separately explicit finite bore witness; not general full topology or engineering acceptance","bore_geometry":bore,"engineering_adoption":"not_assessed"}),
    )
}
pub fn capabilities() -> Value {
    json!({"schema":"oev2.solidworks-worker-capabilities/1","implementation":"Rust",
      "platform":"windows-x86_64","finite_actions":["api_check","inspect_part","create_plate","update_thickness"],
      "existing_instance_attachment":"prohibited","v1_existing_instance_rule":"reject_all",
      "v2_execution_profile":"owned_private_desktop; explicit, exact new child only",
      "session_host":"oe-sw-session-host; GUI subsystem, no-window exact worker, durable streams and OS identity",
      "process_and_file_owner":"oe-windows-controller",
      "native_execution":"requires_current_task_receipt","independent_cad_process_reopen":"requires_two_actual_owned_processes",
      "assembly":"not_implemented","drawing":"not_implemented","step_roundtrip":"not_implemented",
      "engineering_adoption":"not_assessed"})
}
pub fn observation_schema(request: &SolidWorksNativeRequest) -> &'static str {
    if request.schema == "oev2.solidworks-native-request/2" {
        "oev2.solidworks-native-observation/2"
    } else {
        "oev2.solidworks-native-observation/1"
    }
}
