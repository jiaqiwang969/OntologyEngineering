//! Source-bound local planning; performs no network or CAD mutation.
use anyhow::{Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::path::Path;

pub fn plan(root: &SkillRoot, source: &Path, member: &str) -> Result<Value> {
    let path = root.expand(source)?;
    ensure!(
        path.starts_with(root.path()),
        "stepped_source_outside_current_root"
    );
    ensure!(
        !member.starts_with("inputs/"),
        "stepped_unprefixed_member_required"
    );
    let relative = oe_nx_worker::geometry::input_relative(member)?;
    let raw = read_regular_bytes(&path, oe_nx_worker::geometry::stepped_shaft::INTENT_BYTES)?;
    let intent: oe_contracts::NxSteppedShaftIntent =
        serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let profile = oe_nx_worker::geometry::stepped_shaft::validate(&intent)?;
    let params = oe_contracts::NxSteppedShaftCreateParameters {
        schema: oe_nx_worker::geometry::stepped_shaft::PARAMS_SCHEMA.into(),
        action: "create_stepped_shaft".into(),
        intent: oe_contracts::ArtifactReference {
            path: member.into(),
            sha256: sha256(&raw),
        },
    };
    let checked: oe_nx_worker::geometry::Parameters =
        serde_json::from_value(serde_json::to_value(&params)?)?;
    checked.validate()?;
    ensure!(
        read_regular_bytes(&path, oe_nx_worker::geometry::stepped_shaft::INTENT_BYTES)? == raw,
        "stepped_source_changed_during_plan"
    );
    Ok(
        json!({"schema":"oev2.nx-stepped-shaft-local-plan/1","parameters":params,"intent":intent,
        "input_binding":{"source":oe_core::portable_source_path(root.path(),&path)?,"member":relative.as_str()},
        "source_bytes":raw.len(),"expected_bounds_mm":profile.bounds_mm(),"expected_volume_mm3":profile.volume_mm3(),
        "native_execution":"not_run","engineering_adoption":false,
        "next":"Prepare with this exact input and parameter mapping; current Windows/native qualification is separate"}),
    )
}
