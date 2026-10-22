//! Invoke official ggen in isolation and publish an entire generation atomically.
use crate::{Design, inspection::files};
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256, write_new_bytes};
use oe_wire::json::{canonical_json, strict_json};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn resource_profile() -> Value {
    json!({"schema":oe_core::source_limits::PROFILE,"derived_model_bytes":oe_core::source_limits::DERIVED_MODEL_BYTES,"other_artifact_bytes":oe_core::source_limits::PUBLIC_FILE_BYTES,"editable_spec_bytes":4*1024*1024,"template_file_bytes":4*1024*1024})
}

pub fn snapshot(directory: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for p in files(directory, directory)? {
        out.insert(
            p.strip_prefix(directory)?.to_string_lossy().into_owned(),
            sha256(&read_regular_bytes(
                &p,
                oe_core::source_limits::generated_file_bytes(p.strip_prefix(directory)?) as usize,
            )?),
        );
    }
    Ok(out)
}
fn new_stage(root: &SkillRoot) -> Result<PathBuf> {
    root.unique_private_dir(Path::new("var/builds/generation"), "")
}
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir(to)?;
    for p in files(from, from)? {
        let target = to.join(p.strip_prefix(from)?);
        std::fs::create_dir_all(target.parent().context("template_parent_missing")?)?;
        write_new_bytes(&target, &read_regular_bytes(&p, 4 * 1024 * 1024)?)?;
    }
    Ok(())
}
fn source_snapshot(root: &SkillRoot) -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    for p in files(root.path(), &root.path().join("templates"))? {
        hashes.insert(
            oe_core::portable_source_path(root.path(), &p)?,
            sha256(&read_regular_bytes(&p, 4 * 1024 * 1024)?),
        );
    }
    hashes.insert(
        "runtime/ggen-lock.json".into(),
        sha256(&read_regular_bytes(
            &root.path().join("runtime/ggen-lock.json"),
            128 * 1024,
        )?),
    );
    Ok(hashes)
}
fn verify_sources(root: &SkillRoot, hashes: &BTreeMap<String, String>) -> Result<()> {
    for (path, hash) in hashes {
        crate::safe_relative(path)?;
        ensure!(
            sha256(&read_regular_bytes(
                &root.path().join(path),
                16 * 1024 * 1024
            )?) == *hash,
            "generation_source_changed:{path}"
        );
    }
    Ok(())
}
/// Record the editable original as well as its normalized derivation.
pub fn run_spec(root: &SkillRoot, spec: &Path, ggen: &Path) -> Result<(PathBuf, Value)> {
    crate::provenance::require(root)?;
    let anchored =
        crate::composition::transport::anchored(root.path(), spec).map_err(anyhow::Error::msg)?;
    let spec = anchored.as_path();
    let bytes = read_regular_bytes(spec, 4 * 1024 * 1024)?;
    if crate::composition::transport::schema(&bytes)
        .ok()
        .as_deref()
        .is_some_and(crate::composition::transport::is_composed)
    {
        let loaded = crate::composition::load(root.path(), spec)?;
        crate::composition::require_current(root, &loaded)?;
    }
    run_editable(root, spec, ggen)
}
fn run_editable(root: &SkillRoot, spec: &Path, ggen: &Path) -> Result<(PathBuf, Value)> {
    let anchored =
        crate::composition::transport::anchored(root.path(), spec).map_err(anyhow::Error::msg)?;
    let spec = anchored.as_path();
    no_symlinks(spec)?;
    let relative = oe_core::portable_source_path(root.path(), spec)?;
    let bytes = read_regular_bytes(spec, 4 * 1024 * 1024)?;
    if crate::composition::transport::schema(&bytes)
        .ok()
        .as_deref()
        .is_some_and(crate::composition::transport::is_composed)
    {
        return run_composed(root, spec, ggen);
    }
    let d = crate::load(spec)?;
    ensure!(
        read_regular_bytes(spec, 4 * 1024 * 1024)? == bytes,
        "design_spec_changed_during_load"
    );
    let (output, mut receipt) = run(root, &d, ggen)?;
    ensure!(
        read_regular_bytes(spec, 4 * 1024 * 1024)? == bytes,
        "design_spec_changed_during_generation"
    );
    receipt["source_inputs"][&relative] = json!(sha256(&bytes));
    std::fs::write(output.join("generation.json"), canonical_json(&receipt)?)?;
    Ok((output, receipt))
}

fn run_composed(root: &SkillRoot, spec: &Path, ggen: &Path) -> Result<(PathBuf, Value)> {
    let loaded = crate::composition::load(root.path(), spec)?;
    let binding = crate::composition::binding(root, &loaded)?;
    // Diagnostic check may retain an unmatched producer; publication separately
    // requires current capture. Formal run_spec already requires code binding.
    let (output, mut receipt) = run(root, &loaded.design, ggen)?;
    loaded.verify(root.path())?;
    let normalized = loaded.normalized()?;
    ensure!(
        normalized.len() as u64 <= oe_core::source_limits::PUBLIC_FILE_BYTES,
        "normalized_composition_artifact_budget"
    );
    let source = loaded.source_bytes()?;
    ensure!(
        source.len() as u64 <= oe_core::source_limits::PUBLIC_FILE_BYTES,
        "composition_source_map_artifact_budget"
    );
    std::fs::write(output.join("normalized-design.json"), &normalized)?;
    write_new_bytes(&output.join("design-composition-source.json"), &source)?;
    receipt["schema"] = json!(loaded.generation_schema());
    receipt["input_schema"] = json!(loaded.input_schema());
    receipt["composition_source"] = loaded.source_identity()?;
    receipt["composition_binding"] = binding;
    receipt["composition_resource"] = json!({"file_bytes":crate::composition::transport::FILE_BYTES,"total_bytes":crate::composition::transport::TOTAL_BYTES,"file_count":crate::composition::transport::FILE_COUNT});
    for (path, hash) in &loaded.snapshot.hashes {
        receipt["source_inputs"][path] = json!(hash);
    }
    receipt["design_sha256"] = json!(sha256(&normalized));
    let mut products = snapshot(&output)?;
    products.remove("generation.json");
    receipt["products"] = serde_json::to_value(products)?;
    loaded.verify(root.path())?;
    std::fs::write(output.join("generation.json"), canonical_json(&receipt)?)?;
    Ok((output, receipt))
}
/// Diagnostic derivation may describe mismatched source, but cannot publish it.
pub fn run(root: &SkillRoot, design: &Design, ggen: &Path) -> Result<(PathBuf, Value)> {
    design.validate()?;
    let normalized = design.canonical()?;
    let (model, code_hashes) = crate::model(root.path(), design)?;
    let mut sources = source_snapshot(root)?;
    sources.extend(code_hashes);
    let tool = std::fs::canonicalize(ggen).context("ggen_binary_missing")?;
    let lock = strict_json(&read_regular_bytes(
        &root.path().join("runtime/ggen-lock.json"),
        128 * 1024,
    )?)?;
    let tool_hash = sha256(&read_regular_bytes(&tool, 512 * 1024 * 1024)?);
    ensure!(
        lock["binary_sha256"] == tool_hash
            && lock["upstream_commit"] == "ff96f04e8c7b851e5cca53f3faf5ce1d5f43ce6e",
        "ggen_binary_identity_mismatch"
    );
    let stage = new_stage(root)?;
    write_new_bytes(&stage.join("model.ttl"), model.as_bytes())?;
    write_new_bytes(&stage.join("normalized-design.json"), &normalized)?;
    write_new_bytes(
        &stage.join("ggen.toml"),
        &read_regular_bytes(&root.path().join("templates/ggen.toml"), 128 * 1024)?,
    )?;
    copy_dir(
        &root.path().join("templates/pack"),
        &stage.join("templates"),
    )?;
    let before = snapshot(&stage)?;
    let command = CommandSpec {
        executable: tool.clone(),
        arguments: vec!["sync".into(), "run".into()],
        cwd: stage.clone(),
        environment: BTreeMap::new(),
        remove_environment: vec!["OPENAI_API_KEY".into(), "ANTHROPIC_API_KEY".into()],
        timeout: Duration::from_secs(120),
        output_limit: 4 * 1024 * 1024,
    };
    let observation = run_command(&command, &stage.join("execution"))?;
    ensure!(
        observation.status == "completed" && observation.exit_code == Some(0),
        "ggen_execution_failed:{}",
        String::from_utf8_lossy(&observation.stderr)
    );
    for (path, hash) in &before {
        ensure!(
            sha256(&read_regular_bytes(
                &stage.join(path),
                oe_core::source_limits::generated_file_bytes(Path::new(path)) as usize
            )?) == *hash,
            "generation_input_changed:{path}"
        );
    }
    ensure!(
        sha256(&read_regular_bytes(&tool, 512 * 1024 * 1024)?) == tool_hash,
        "ggen_binary_changed_during_execution"
    );
    let mut current = source_snapshot(root)?;
    // Recheck exactly the input closure used to build the model, including
    // the frozen migration scope and any declared original-API mappings.
    current.extend(crate::model(root.path(), design)?.1);
    ensure!(current == sources, "generation_source_set_changed");
    verify_sources(root, &sources)?;
    let output = stage.join("out");
    let products = snapshot(&output)?;
    for p in [
        "contracts.rs",
        "semantic-contracts.rs",
        "contract-tests.rs",
        "semantic-contract-tests.rs",
        "interfaces.md",
        "capabilities.json",
        "checks.rq",
        "source-map.json",
        "validation-plan.json",
    ] {
        ensure!(products.contains_key(p), "generation_product_missing:{p}");
    }
    let static_map = strict_json(&read_regular_bytes(
        &output.join("source-map.json"),
        4 * 1024 * 1024,
    )?)?;
    ensure!(
        static_map["missing_checkers"]
            .as_array()
            .is_some_and(|v| v.is_empty()),
        "generation_constraint_checker_missing"
    );
    ensure!(
        static_map["missing_interfaces"]
            .as_array()
            .is_some_and(|v| v.is_empty()),
        "generation_interface_implementation_missing"
    );
    write_new_bytes(&output.join("model.ttl"), model.as_bytes())?;
    write_new_bytes(&output.join("normalized-design.json"), &normalized)?;
    let products = snapshot(&output)?;
    let receipt = json!({"schema":"oev2.generation/1","resource_profile":resource_profile(),"upstream":"ggen","upstream_version":"26.9.28","upstream_commit":lock["upstream_commit"],"binary_sha256":tool_hash,"model_sha256":sha256(model.as_bytes()),"design_sha256":sha256(&normalized),"source_inputs":sources,"inputs":before,"products":products,"normalizer_binding":crate::provenance::binding(root)?,"semantic_review":"not_run","engineering_acceptance":"not_run"});
    write_new_bytes(&output.join("generation.json"), &canonical_json(&receipt)?)?;
    Ok((output, receipt))
}
fn composition_envelope(receipt: &Value, normalized: &Value, has_source: bool) -> Result<bool> {
    let fourth = receipt["schema"] == "oev2.generation/4";
    let third = receipt["schema"] == "oev2.generation/3";
    let composed = fourth || third || receipt["schema"] == "oev2.generation/2";
    let normalized_third = normalized["schema"] == "oev2.normalized-design/3";
    ensure!(
        fourth == (normalized["schema"] == "oev2.normalized-design/4")
            && third == normalized_third
            && (fourth
                || third
                || ((normalized["schema"] == "oev2.normalized-design/2") == composed))
            && has_source == composed,
        "generation_composition_schema_downgrade"
    );
    Ok(composed)
}

pub fn publish(root: &SkillRoot, output: &Path) -> Result<()> {
    ensure!(
        output.starts_with(root.path().join("var/builds/generation")),
        "generation_publish_source_outside_stage"
    );
    no_symlinks(output)?;
    let dest = root.path().join("generated");
    no_symlinks(&dest)?;
    let _guard =
        oe_core::AdvisoryFileLock::acquire(&root.path().join("var/state/generation.lock"), true)?;
    let receipt = strict_json(&read_regular_bytes(
        &output.join("generation.json"),
        4 * 1024 * 1024,
    )?)?;
    ensure!(
        [
            "oev2.generation/1",
            "oev2.generation/2",
            "oev2.generation/3",
            "oev2.generation/4"
        ]
        .contains(&receipt["schema"].as_str().unwrap_or("")),
        "generation_schema_unsupported"
    );
    let normalized = strict_json(&read_regular_bytes(
        &output.join("normalized-design.json"),
        oe_core::source_limits::PUBLIC_FILE_BYTES as usize,
    )?)?;
    let composed = composition_envelope(
        &receipt,
        &normalized,
        output.join("design-composition-source.json").is_file(),
    )?;
    if crate::provenance::compiled()?["composition_files"]
        .as_object()
        .is_some_and(|files| !files.is_empty())
    {
        ensure!(composed, "current_composition_root_requires_generation2");
    }
    ensure!(
        receipt["resource_profile"] == resource_profile(),
        "generation_resource_profile_mismatch"
    );
    if composed {
        let relative = receipt["composition_source"]["root"]
            .as_str()
            .context("composition_root_identity_missing")?;
        crate::safe_relative(relative)?;
        let loaded = crate::composition::load(root.path(), &root.path().join(relative))?;
        crate::composition::require_current(root, &loaded)?;
        ensure!(
            normalized == strict_json(&loaded.normalized()?)?
                && receipt["design_sha256"] == sha256(&loaded.normalized()?)
                && receipt["input_schema"] == loaded.input_schema()
                && receipt["schema"] == loaded.generation_schema(),
            "generation_composition_normalized_source_mismatch"
        );
        let source_bytes = read_regular_bytes(
            &output.join("design-composition-source.json"),
            oe_core::source_limits::PUBLIC_FILE_BYTES as usize,
        )?;
        let retained_source = strict_json(&source_bytes)?;
        ensure!(
            retained_source == loaded.evidence,
            "generation_composition_source_map_mismatch"
        );
        loaded.verify_reference_source(&normalized, &source_bytes)?;
        ensure!(
            receipt["composition_binding"]["status"] == "matched"
                && receipt["composition_source"] == loaded.source_identity()?,
            "generation_composition_source_changed_or_uncompiled"
        );
        ensure!(
            receipt["composition_resource"]
                == json!({"file_bytes":crate::composition::transport::FILE_BYTES,"total_bytes":crate::composition::transport::TOTAL_BYTES,"file_count":crate::composition::transport::FILE_COUNT}),
            "generation_composition_resource_mismatch"
        );
    }
    verify_sources(
        root,
        &serde_json::from_value(receipt["source_inputs"].clone())?,
    )?;
    let products: BTreeMap<String, String> = serde_json::from_value(receipt["products"].clone())?;
    let mut actual = snapshot(output)?;
    actual.remove("generation.json");
    ensure!(actual == products, "generation_staged_product_drift");
    crate::provenance::require(root)?;
    if !dest.exists() {
        oe_core::rename_directory_exclusive(output, &dest)?;
        return Ok(());
    }
    ensure!(dest.is_dir(), "generated_target_not_directory");
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let a = std::ffi::CString::new(output.as_os_str().as_bytes())?;
        let b = std::ffi::CString::new(dest.as_os_str().as_bytes())?;
        #[cfg(target_os = "macos")]
        let result = unsafe {
            libc::renameatx_np(
                libc::AT_FDCWD,
                a.as_ptr(),
                libc::AT_FDCWD,
                b.as_ptr(),
                libc::RENAME_SWAP,
            )
        };
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                a.as_ptr(),
                libc::AT_FDCWD,
                b.as_ptr(),
                libc::RENAME_EXCHANGE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error())
                .context("generation_atomic_exchange_failed");
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    anyhow::bail!("atomic_generation_publication_unsupported_platform")
}

#[cfg(test)]
mod reference_envelope_tests {
    use super::*;
    #[test]
    fn composition4_envelope_rejects_every_mixed_pair_and_missing_source() {
        for i in 1..=4 {
            for j in 1..=4 {
                for source in [false, true] {
                    let receipt = json!({"schema":format!("oev2.generation/{i}")});
                    let normalized = json!({"schema":if j == 1 {"oev2.design/1".into()} else {format!("oev2.normalized-design/{j}")}});
                    assert_eq!(
                        composition_envelope(&receipt, &normalized, source).is_ok(),
                        i == j && source == (i != 1),
                        "{i} {j} {source}"
                    );
                }
            }
        }
    }
    #[test]
    fn composition3_envelope_preserves_old_pairs_and_rejects_mixed_versions() {
        let input = [
            "oev2.generation/1",
            "oev2.generation/2",
            "oev2.generation/3",
        ];
        let output = [
            "oev2.design/1",
            "oev2.normalized-design/2",
            "oev2.normalized-design/3",
        ];
        for (i, g) in input.iter().enumerate() {
            for (j, n) in output.iter().enumerate() {
                for source in [false, true] {
                    let result =
                        composition_envelope(&json!({"schema":g}), &json!({"schema":n}), source);
                    assert_eq!(
                        result.is_ok(),
                        i == j && source == (i != 0),
                        "{g} {n} {source}"
                    );
                }
            }
        }
    }
}
pub fn check(root: &SkillRoot, design: &Design, ggen: &Path) -> Result<Value> {
    let (output, receipt) = run(root, design, ggen)?;
    compare(root, &output, receipt)
}
pub fn check_spec(root: &SkillRoot, spec: &Path, ggen: &Path) -> Result<Value> {
    let (output, receipt) = run_editable(root, spec, ggen)?;
    compare(root, &output, receipt)
}
fn compare(root: &SkillRoot, output: &Path, receipt: Value) -> Result<Value> {
    let expected = snapshot(output)?;
    let actual = snapshot(&root.path().join("generated"))?;
    let drift: Vec<_> = expected
        .keys()
        .chain(actual.keys())
        .filter(|k| expected.get(*k) != actual.get(*k))
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(
        json!({"schema":match receipt["schema"].as_str(){Some("oev2.generation/4")=>"oev2.generation-drift/4",Some("oev2.generation/3")=>"oev2.generation-drift/3",Some("oev2.generation/2")=>"oev2.generation-drift/2",_=>"oev2.generation-drift/1"},"status":if drift.is_empty() && receipt["normalizer_binding"]["status"]=="matched" && (!matches!(receipt["schema"].as_str(), Some("oev2.generation/2" | "oev2.generation/3" | "oev2.generation/4")) || receipt["composition_binding"]["status"]=="matched"){"consistent"}else{"drift"},"drift":drift,"generation":receipt,"execution_role":"diagnostic_only","previous_working_version_preserved":true}),
    )
}
