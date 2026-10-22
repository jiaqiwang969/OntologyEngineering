//! Executing normalizer provenance is separate from target source observations.
use anyhow::{Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use oe_wire::json::{canonical_json, strict_json};
use serde_json::{Value, json};
#[path = "../../../build_support/provenance.rs"]
mod inventory;

pub fn compiled() -> Result<Value> {
    Ok(strict_json(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/compiled-source.json"
    )))?)
}
pub fn binding(root: &SkillRoot) -> Result<Value> {
    let captured = compiled()?;
    let mut missing = Vec::new();
    let mut changed = Vec::new();
    let expected = captured["files"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("compiled_manifest_invalid"))?;
    for (p, id) in expected {
        match read_regular_bytes(&root.path().join(p), 16 * 1024 * 1024) {
            Ok(bytes) if id["sha256"] == sha256(&bytes) && id["bytes"] == bytes.len() as u64 => {}
            Ok(_) => changed.push(p.clone()),
            Err(_) => missing.push(p.clone()),
        }
    }
    let mut added = Vec::new();
    if let Ok(current) = inventory::inputs(root.path()) {
        for p in current {
            let p = oe_core::portable_source_path(root.path(), &root.path().join(p))?;
            if !expected.contains_key(&p) {
                added.push(p);
            }
        }
    } else if missing.is_empty() {
        missing.push("inventory_unreadable".into());
    }
    Ok(json!({"schema":"oev2.execution-source-binding/1",
        "status":if !changed.is_empty() || !added.is_empty(){"mismatch"}else if !missing.is_empty(){"incomplete"}else{"matched"},
        "compiled_source_sha256":sha256(canonical_json(&captured["files"])?),
        "build_conditions_sha256":sha256(canonical_json(&captured)?),"target":captured["target"],
        "package_features":captured["package_features"],"compiler":captured["compiler"],
        "changed":changed,"missing":missing,"added":added,"scope":captured["scope"],"uncovered":captured["uncovered"]}))
}
pub fn require(root: &SkillRoot) -> Result<Value> {
    let record = binding(root)?;
    ensure!(
        record["status"] == "matched",
        "compiled_source_binding_mismatch"
    );
    Ok(record)
}
/// Private execution record; executable identity does not pollute deterministic artifacts.
pub fn execution(root: &SkillRoot, role: &str) -> Result<Value> {
    let mut record = require(root)?;
    record["execution_role"] = json!(role);
    record["executable_sha256"] =
        json!(oe_core::fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)?.sha256);
    Ok(record)
}
