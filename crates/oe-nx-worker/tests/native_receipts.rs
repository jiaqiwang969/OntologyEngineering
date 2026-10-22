//! Task-owned native receipts; no private fixture or tool binary is distributed.
use oe_nx_protocol::{BlobIdentity, JobManifest, NativeResult, decode};
use oe_nx_worker::geometry::{Parameters, PartInfo, require_block, require_controlled_refset};
use std::path::{Path, PathBuf};

fn private_job(variable: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(variable).expect("explicit native receipt root"))
        .canonicalize()
        .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../var")
        .canonicalize()
        .unwrap();
    assert!(path.starts_with(root), "native receipt must remain private");
    path
}

fn bytes(path: &Path) -> Vec<u8> {
    let metadata = path.symlink_metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 16 * 1024 * 1024);
    std::fs::read(path).unwrap()
}

#[test]
#[ignore = "requires explicit verified task-private create and fresh-process reopen receipts"]
fn saved_native_parts_match_frozen_parameters_and_reject_single_faults() {
    let create = private_job("OEV2_NATIVE_CREATE_JOB");
    let reopen = private_job("OEV2_NATIVE_REOPEN_JOB");
    let original: NativeResult = decode(&bytes(&create.join("native-result.json"))).unwrap();
    let result: NativeResult = decode(&bytes(&reopen.join("native-result.json"))).unwrap();
    let job: JobManifest = decode(&bytes(&create.join("job.json"))).unwrap();
    let reopened_job: JobManifest = decode(&bytes(&reopen.join("job.json"))).unwrap();
    let parameters = bytes(&create.join("params.json"));
    job.validate_parameters(&parameters).unwrap();
    assert_eq!(original.manifest_digest, job.digest().unwrap());
    assert_eq!(result.manifest_digest, reopened_job.digest().unwrap());
    assert_ne!(original.process, result.process);
    assert_eq!(
        serde_json::to_value(&original).unwrap()["outcome"],
        "failed"
    );
    assert!(
        original
            .errors
            .iter()
            .any(|e| e == "step_export_incomplete_after_timeout")
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap()["outcome"],
        "succeeded"
    );
    assert!(result.errors.is_empty() && result.unknowns.is_empty());
    assert_eq!(result.readback["lifecycle"]["terminate_return"], 0);
    assert_eq!(result.readback["engineering_verdict"], "not_assessed");
    let parsed: Parameters = decode(&parameters).unwrap();
    parsed.validate().unwrap();
    let Parameters::Create {
        updated_length_mm,
        width_mm,
        height_mm,
        spacing_mm,
        ..
    } = parsed
    else {
        panic!("controlled create parameters")
    };
    let parts: Vec<PartInfo> = serde_json::from_value(result.readback["parts"].clone()).unwrap();
    assert_eq!(parts.len(), 2);
    let block = parts
        .iter()
        .find(|p| p.path.ends_with("parametric_block.prt"))
        .unwrap();
    let assembly = parts
        .iter()
        .find(|p| p.path.ends_with("two_instances.prt"))
        .unwrap();
    require_block(block, updated_length_mm, width_mm, height_mm).unwrap();
    require_controlled_refset(block).unwrap();
    assert_eq!(assembly.units, "mm");
    assert_eq!(assembly.solid_count, 2);
    assert_eq!(assembly.sheet_count, 0);
    assert_eq!(assembly.components.len(), 2);
    for (name, x) in [("BLOCK_A", 0.0), ("BLOCK_B", spacing_mm)] {
        let component = assembly.components.iter().find(|c| c.name == name).unwrap();
        assert_eq!(component.origin, [x, 0.0, 0.0]);
        assert_eq!(component.reference_set, "OE_CONTROLLED_BODY");
        assert_eq!(
            component.matrix,
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            component.prototype_path.replace('\\', "/"),
            block.path.replace('\\', "/")
        );
    }
    for name in ["parametric_block.prt", "two_instances.prt"] {
        let raw = bytes(&create.join("out").join(name));
        let input = bytes(&reopen.join("inputs").join(name));
        assert_eq!(raw, input);
        let declared = reopened_job
            .inputs
            .iter()
            .find(|entry| entry.path.as_str() == format!("inputs/{name}"))
            .unwrap();
        assert_eq!(declared.identity, BlobIdentity::of(&input));
    }
    // Mutate observations, never the native files, original oracle or result.
    let mut wrong_extent = block.clone();
    wrong_extent.planar_vertex_bounds[0].as_mut().unwrap()[3] += 1.0;
    assert!(require_block(&wrong_extent, updated_length_mm, width_mm, height_mm).is_err());
    let mut stale_jt = block.clone();
    stale_jt.controlled_jt_mapping.as_mut().unwrap().up_to_date = Some(false);
    assert!(require_controlled_refset(&stale_jt).is_err());
    // A separately exposed checker cannot rely on its caller having already
    // rejected zero/unknown dimensions, even with self-consistent observations.
    let mut degenerate = block.clone();
    degenerate.planar_vertex_bounds[0].as_mut().unwrap()[3] = 0.0;
    degenerate.bbox[0][3] = 0.0;
    degenerate
        .expressions
        .iter_mut()
        .find(|e| e.name == "cad_length")
        .unwrap()
        .value = 0.0;
    assert_eq!(
        require_block(&degenerate, 0.0, width_mm, height_mm)
            .unwrap_err()
            .to_string(),
        "native_block_expected_dimensions_invalid"
    );
}

#[test]
#[ignore = "requires explicit verified Parasolid import, before-execution acceptance and fresh-process reopen receipts"]
fn imported_parasolid_survives_independent_reopen_with_original_native_bytes() {
    let import = private_job("OEV2_PARASOLID_IMPORT_JOB");
    let reopen = private_job("OEV2_PARASOLID_REOPEN_JOB");
    let acceptance_path = private_job("OEV2_PARASOLID_ACCEPTANCE");
    let acceptance: serde_json::Value = serde_json::from_slice(&bytes(&acceptance_path)).unwrap();
    assert_eq!(acceptance["schema"], "oev2.uf2-native-acceptance/1");
    assert_eq!(acceptance["expected_units"], "mm");
    assert_eq!(acceptance["solid_count"], 1);
    assert_eq!(acceptance["sheet_count"], 0);
    assert_eq!(
        acceptance["accurate_bounds_dimensions_mm"],
        serde_json::json!([5, 2, 2])
    );
    assert_eq!(acceptance["comparison_tolerance_mm"], 0.01);
    assert_eq!(acceptance["must_save_and_independently_reopen"], true);
    assert_eq!(acceptance["engineering_adoption"], "unknown");
    let original: NativeResult = decode(&bytes(&import.join("native-result.json"))).unwrap();
    let result: NativeResult = decode(&bytes(&reopen.join("native-result.json"))).unwrap();
    let original_job: JobManifest = decode(&bytes(&import.join("job.json"))).unwrap();
    let reopened_job: JobManifest = decode(&bytes(&reopen.join("job.json"))).unwrap();
    assert_eq!(original.manifest_digest, original_job.digest().unwrap());
    assert_eq!(result.manifest_digest, reopened_job.digest().unwrap());
    assert_ne!(original.process, result.process);
    assert_eq!(original.worker, result.worker);
    assert_eq!(original.runtime, result.runtime);
    assert_eq!(
        BlobIdentity::of(&bytes(&import.join("inputs/UF2-5.x_b")))
            .sha256
            .as_str(),
        acceptance["asset_sha256"].as_str().unwrap()
    );
    for receipt in [&original, &result] {
        assert_eq!(
            serde_json::to_value(receipt).unwrap()["outcome"],
            "succeeded"
        );
        assert!(receipt.errors.is_empty() && receipt.unknowns.is_empty());
        assert_eq!(receipt.readback["lifecycle"]["terminate_return"], 0);
        assert_eq!(receipt.readback["engineering_verdict"], "not_assessed");
    }
    let imported: PartInfo = serde_json::from_value(original.readback["part"].clone()).unwrap();
    let reopened: Vec<PartInfo> = serde_json::from_value(result.readback["parts"].clone()).unwrap();
    assert_eq!(reopened.len(), 1);
    for part in [&imported, &reopened[0]] {
        assert_eq!(part.units, "mm");
        assert_eq!(part.solid_count, 1);
        assert_eq!(part.sheet_count, 0);
        assert_eq!(part.exact_work_csys_bounds.len(), 1);
        let bounds = &part.exact_work_csys_bounds[0];
        let checked = oe_nx_worker::geometry::ExactBounds::new(
            bounds.min_corner,
            bounds.directions,
            bounds.distances,
        )
        .unwrap();
        assert_eq!(checked.absolute_axis_bounds, bounds.absolute_axis_bounds);
        for (observed, expected) in bounds.distances.iter().zip(
            acceptance["accurate_bounds_dimensions_mm"]
                .as_array()
                .unwrap(),
        ) {
            assert!(
                (observed - expected.as_f64().unwrap()).abs()
                    <= acceptance["comparison_tolerance_mm"].as_f64().unwrap(),
                "frozen native-file extent, not a manufacturing tolerance"
            );
        }
    }
    assert_eq!(
        serde_json::to_value(&imported.exact_work_csys_bounds).unwrap(),
        serde_json::to_value(&reopened[0].exact_work_csys_bounds).unwrap()
    );
    let saved = bytes(&import.join("out/parasolid_import.prt"));
    let supplied = bytes(&reopen.join("inputs/supplier_import.prt"));
    assert_eq!(saved, supplied);
    let declared = reopened_job
        .inputs
        .iter()
        .find(|entry| entry.path.as_str() == "inputs/supplier_import.prt")
        .unwrap();
    assert_eq!(declared.identity, BlobIdentity::of(&saved));
}
