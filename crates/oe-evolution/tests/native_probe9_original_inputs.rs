//! Exact original development input checks. This does not recreate the original
//! live producer, expired deadline, actual environment or compiler execution.
use anyhow::{Context, Result, ensure};
use oe_core::{no_symlinks, read_regular_bytes, sha256};
use oe_evolution::{
    CodeNativeBuildRecipe6, CodeNativeCompilationSources4, CodeNativeSideContext6,
    native_compiler_probe9::{ProbeInputScope9, check_input_domain9},
};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn read_reference(value: &Value, base: &Path) -> Result<Value> {
    let path = Path::new(value["path"].as_str().context("original_reference_path")?);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    no_symlinks(&path)?;
    let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&bytes) == value["sha256"],
        "original_reference_changed"
    );
    if let Some(length) = value["bytes"].as_u64() {
        ensure!(bytes.len() as u64 == length, "original_reference_length");
    }
    Ok(strict_json(&bytes)?)
}

#[test]
#[ignore = "requires private exact original compiler-probe inputs; source/content checks only"]
fn private_original_thirteen_probe_content_checks() -> Result<()> {
    let path = PathBuf::from(std::env::var("OE_NATIVE_PROBE9_ORIGINAL_INPUTS")?);
    let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&bytes) == "dbbb1738ca807387d45c585ec71ad064626560eb0cf9e2eb486aa1f17947dc83",
        "original_thirteen_inputs_changed"
    );
    let original = strict_json(&bytes)?;
    let rows = original["original_probe_inputs"]
        .as_array()
        .context("original_probe_rows")?;
    ensure!(rows.len() == 13, "original_probe_count");
    let mut files = 0;
    let mut stdin = 0;
    let mut observations = Vec::new();
    for row in rows {
        let context = read_reference(&row["stage_context"], Path::new("/"))?;
        let base = Path::new(
            context["source_root"]
                .as_str()
                .context("original_source_root")?,
        );
        let raw = read_reference(&row["original_raw_refusal"], base)?;
        ensure!(
            raw["qualification"] == "not_established" && raw["arguments"] == row["arguments"],
            "original_refusal_content_changed"
        );
        let recipe: CodeNativeBuildRecipe6 =
            serde_json::from_value(read_reference(&context["recipe"], base)?)?;
        let side: CodeNativeSideContext6 =
            serde_json::from_value(read_reference(&context["side_context"], base)?)?;
        let sources: CodeNativeCompilationSources4 =
            serde_json::from_value(read_reference(&row["complete_selected_sources"], base)?)?;
        let selected = sources
            .roots
            .iter()
            .filter(|source| source.package_id == row["package_id"])
            .collect::<Vec<_>>();
        ensure!(
            selected.len() == 1,
            "original_selected_source_missing_duplicate"
        );
        let source = selected[0];
        ensure!(
            raw["cwd"] == source.root
                && serde_json::to_value(&source.cargo_manifest)?
                    == row["original_package_manifest"]
                && serde_json::to_value(&source.package_archive)?
                    == row["original_package_archive"],
            "original_package_content_changed"
        );
        let arguments: Vec<String> = serde_json::from_value(row["arguments"].clone())?;
        let output = Path::new(
            row["declared_output_dir"]
                .as_str()
                .context("original_declared_output")?,
        );
        // Reconstruct the original declared output namespace for a content case.
        // This is not an observation of a live OUT_DIR or encoded environment.
        let original_out = if output.file_name().is_some_and(|name| name == "probe") {
            output.parent().context("original_output_parent")?
        } else {
            output
        };
        let scope = ProbeInputScope9 {
            target_dir: Path::new(&side.build.target_dir),
            platform: &side.build.platform,
            stage: context["parent_stage"].as_str().context("original_stage")?,
            cargo_out_dir: original_out,
            encoded_flags: &recipe.raw_encoded_rustflags,
            actual_encoded_flags: &recipe.raw_encoded_rustflags,
            cwd: Path::new(&source.root),
        };
        let domain = check_input_domain9(source, &arguments, &scope)?;
        ensure!(
            domain.arguments == arguments
                && domain.out_dir == output.to_str().context("original_output_utf8")?,
            "original_argv_or_output_rewritten"
        );
        if let Some(file) = &domain.source_file {
            let occurrences = row["source_file_occurrences"]
                .as_array()
                .context("original_source_occurrences")?;
            ensure!(
                occurrences.len() == 1
                    && serde_json::to_value(file)? == occurrences[0]["source"]
                    && domain.input_argument_ordinal
                        == occurrences[0]["argument_ordinal"]
                            .as_u64()
                            .context("original_source_ordinal")?,
                "original_source_occurrence_changed"
            );
            files += 1;
            let mut changed = source.clone();
            let binding = changed
                .files
                .iter_mut()
                .find(|binding| binding.path == file.path)
                .context("original_file_binding")?;
            binding.sha256 = "0".repeat(64);
            ensure!(
                check_input_domain9(&changed, &arguments, &scope).is_err(),
                "changed_source_hash_not_rejected"
            );
            let mut duplicate = source.clone();
            duplicate.files.push(file.clone());
            ensure!(
                check_input_domain9(&duplicate, &arguments, &scope).is_err(),
                "duplicate_source_not_rejected"
            );
        } else {
            ensure!(
                domain.input_kind == "actual_stdin_required"
                    && row["stdin_bytes"]
                        .as_str()
                        .context("original_stdin_status")?
                        .starts_with("not_retained"),
                "original_stdin_not_captured"
            );
            stdin += 1;
        }
        let mut altered = arguments.clone();
        altered.push("--emit=link".into());
        ensure!(
            check_input_domain9(source, &altered, &scope).is_err(),
            "native_emit_not_rejected"
        );
        observations.push(serde_json::to_value(domain)?);
    }
    ensure!(files == 10 && stdin == 3, "original_input_partition");
    println!(
        "{}",
        json!({"original_vectors":13,"original_file_occurrences":files,"actual_stdin_still_required":stdin,
        "observations":observations,"scope":"original development content cases only",
        "original_live_environment_producer_deadline_compiler_execution_and_adoption":"not_recreated"})
    );
    Ok(())
}
