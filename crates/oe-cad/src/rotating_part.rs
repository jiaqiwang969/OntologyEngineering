//! Local source/parameter projection into the existing NX prepare/worker chain.
use anyhow::{Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::path::Path;

pub fn plan(root: &SkillRoot, source: &Path, member: &str) -> Result<Value> {
    let path = root.expand(source)?;
    ensure!(
        path.starts_with(root.path()),
        "cylinder_source_outside_current_root"
    );
    ensure!(
        !member.starts_with("inputs/"),
        "cylinder_intent_requires_unprefixed_worker_member"
    );
    let relative = oe_nx_worker::geometry::input_relative(member)?;
    let before = read_regular_bytes(&path, oe_nx_worker::geometry::rotating_part::INTENT_BYTES)?;
    let intent: oe_contracts::NxCylinderIntent =
        serde_json::from_value(oe_core::json::strict_json(&before)?)?;
    oe_nx_worker::geometry::rotating_part::validate(&intent)?;
    let params = oe_contracts::NxCylinderCreateParameters {
        schema: oe_nx_worker::geometry::rotating_part::PARAMS_SCHEMA.into(),
        action: "create_cylinder_solid".into(),
        intent: oe_contracts::ArtifactReference {
            path: member.into(),
            sha256: sha256(&before),
        },
    };
    let raw = serde_json::to_vec(&params)?;
    let checked: oe_nx_worker::geometry::Parameters = serde_json::from_slice(&raw)?;
    checked.validate()?;
    ensure!(
        read_regular_bytes(&path, oe_nx_worker::geometry::rotating_part::INTENT_BYTES)? == before,
        "cylinder_source_changed_during_plan"
    );
    Ok(
        json!({"schema":"oev2.nx-cylinder-local-plan/1","parameters":params,"intent":intent,
        "input_binding":{"source":oe_core::portable_source_path(root.path(),&path)?,"member":relative.as_str()},
        "source_bytes":before.len(),"native_execution":"not_run","engineering_adoption":false,
        "next":"Use this exact source/parameter mapping in existing cad nx prepare; worker/native/source Windows qualification separate"}),
    )
}
