//! No motion, update or save. Process independence is proved by host receipts.
use super::{NativeApi, Workspace, motion_task};
use crate::{assembly_position as position, motion_source};
use anyhow::{Result, ensure};
use oe_contracts::*;
use serde_json::{Value, json};
pub const PARAMS_SCHEMA: &str = "oev2.nx-assembly-position-reopen-params/1";
pub fn source(
    w: &dyn Workspace,
    closure_ref: &ArtifactReference,
    expected_ref: &ArtifactReference,
    budget: &NxAssemblyBudget,
) -> Result<(NxAssemblySourceClosure, NxAssemblyPositionObservation)> {
    let closure: NxAssemblySourceClosure = motion_task::read(w, closure_ref, 4 * 1024 * 1024)?;
    let expected: NxAssemblyPositionObservation =
        motion_task::read(w, expected_ref, 32 * 1024 * 1024)?;
    motion_source::validate_source_closure(&closure)?;
    position::validate_position_observation(&expected, budget)?;
    ensure!(
        closure.root_part == expected.root_prototype_id,
        "position_reopen_source_root_differs"
    );
    for f in &closure.files {
        super::input_relative(&f.path)?;
        ensure!(
            f.path.to_ascii_lowercase().ends_with(".prt"),
            "position_reopen_native_part_required"
        );
    }
    // Detect ambiguous persistent paths before native activation.
    position::persistence::compare(&expected, &expected, budget)?;
    Ok((closure, expected))
}
pub fn execute(
    api: &mut dyn NativeApi,
    w: &mut dyn Workspace,
    closure_ref: &ArtifactReference,
    expected_ref: &ArtifactReference,
    closure: &NxAssemblySourceClosure,
    expected: &NxAssemblyPositionObservation,
    budget: &NxAssemblyBudget,
) -> Result<Value> {
    let (root_path, namespace) = w.prepare_motion_source(closure)?;
    w.verify_motion_copy_files()?;
    let root = api.open_part(&root_path)?;
    let loaded = api.loaded_assembly_files(&namespace)?;
    motion_task::check_loaded(&loaded, closure)?;
    let actual = api.assembly_positions(root, &namespace, budget, "reference_observation")?;
    motion_task::check_loaded(&api.loaded_assembly_files(&namespace)?, closure)?;
    ensure!(
        actual.root_prototype_id == closure.root_part,
        "position_reopen_native_root_differs"
    );
    position::persistence::compare(expected, &actual, budget)?;
    w.verify_motion_copy_files()?;
    w.verify_motion_source()?;
    let result = json!({"action":"inspect_assembly_positions","source":closure_ref,"expected_position":expected_ref,
  "comparison":"persistent_component_positions_and_bounded_geometry_match","observation":actual,"loaded_parts":loaded,
  "private_copy_bytes_unchanged":true,"save_requested":false,"motion_requested":false,
  "independent_process":"requires_controller_process_receipts","whole_topology_checked":false,"engineering_adoption":"not_assessed"});
    w.record("position_reopen_compared", result.clone())?;
    Ok(result)
}
