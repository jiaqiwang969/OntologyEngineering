//! Versioned generation evidence for the existing artifact owner. Generation /2
//! retains its actual composed source; it is never projected into a /1 receipt.
use anyhow::{Context, Result, ensure};
use oe_contracts::CodeSourceArtifact;
use oe_core::{read_regular_bytes, sha256};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub(super) fn verify(artifact: &CodeSourceArtifact) -> Result<()> {
    let generation = &artifact.generation;
    let declared = &generation["receipt"];
    ensure!(
        generation["check"]["status"] == "consistent"
            && declared["normalizer_binding"]["status"] == "matched"
            && matches!(
                declared["schema"].as_str(),
                Some(
                    "oev2.generation/1"
                        | "oev2.generation/2"
                        | "oev2.generation/3"
                        | "oev2.generation/4"
                )
            ),
        "code_artifact_generation_unverified"
    );
    let root = Path::new(&artifact.source_root);
    let receipt = strict_json(&read_regular_bytes(
        &root.join("generated/generation.json"),
        16 * 1024 * 1024,
    )?)?;
    ensure!(
        receipt == *declared,
        "code_artifact_generation_receipt_changed"
    );
    if matches!(
        receipt["schema"].as_str(),
        Some("oev2.generation/2" | "oev2.generation/3" | "oev2.generation/4")
    ) {
        let drift_schema = if receipt["schema"] == "oev2.generation/4" {
            "oev2.generation-drift/4"
        } else if receipt["schema"] == "oev2.generation/3" {
            "oev2.generation-drift/3"
        } else {
            "oev2.generation-drift/2"
        };
        ensure!(
            generation["check"]["schema"] == drift_schema
                && generation["check"]["drift"] == json!([])
                && generation["check"]["generation"] == receipt,
            "code_artifact_composed_drift_receipt_changed"
        );
        verify_composed(root, &receipt, &artifact.manifest)?;
    }
    // All original /1 semantics remain above. Binary/source checks and native
    // five-stage qualification stay with their existing independent owners.
    Ok(())
}

fn verify_composed(root: &Path, receipt: &Value, manifest: &Value) -> Result<()> {
    use oe_design::composition::transport;
    ensure!(
        receipt["input_schema"]
            .as_str()
            .is_some_and(transport::is_composed)
            && receipt["resource_profile"] == oe_design::generation::resource_profile()
            && receipt["composition_resource"]
                == json!({"file_bytes":transport::FILE_BYTES,
            "total_bytes":transport::TOTAL_BYTES,"file_count":transport::FILE_COUNT}),
        "code_artifact_composed_generation_profile"
    );
    let name = receipt["composition_source"]["root"]
        .as_str()
        .context("code_artifact_composition_root")?;
    oe_design::safe_relative(name)?;
    let loaded = oe_design::composition::load(root, &root.join(name))?;
    ensure!(
        receipt["input_schema"] == loaded.input_schema()
            && receipt["schema"] == loaded.generation_schema(),
        "code_artifact_composed_generation_schema_mismatch"
    );
    let identity = loaded.source_identity()?;
    ensure!(
        identity == receipt["composition_source"],
        "code_artifact_composition_source_changed"
    );
    let files: BTreeMap<_, _> = loaded
        .snapshot
        .files
        .iter()
        .map(|(path, bytes)| {
            (
                path.clone(),
                json!({"bytes":bytes.len(),"sha256":loaded.snapshot.hashes[path]}),
            )
        })
        .collect();
    let files = serde_json::to_value(files)?;
    let binding = &receipt["composition_binding"];
    ensure!(
        binding["schema"] == "oev2.compiled-composition-binding/2"
            && binding["status"] == "matched"
            && binding["input_schema"] == loaded.input_schema()
            && binding["root"] == name
            && binding["source"] == identity
            && binding["current_files"] == files
            && binding["compiled_files"] == files,
        "code_artifact_composition_capture_changed"
    );
    let manifest_files = manifest["files"]
        .as_object()
        .context("code_artifact_composition_manifest")?;
    let source_inputs = receipt["source_inputs"]
        .as_object()
        .context("code_artifact_composition_inputs")?;
    for (path, expected) in source_inputs {
        oe_design::safe_relative(path)?;
        ensure!(
            manifest_files
                .get(path)
                .is_some_and(|file| file["sha256"] == *expected),
            "code_artifact_generation_source_not_in_manifest:{path}"
        );
    }
    for (path, bytes) in &loaded.snapshot.files {
        ensure!(
            source_inputs.get(path) == Some(&json!(loaded.snapshot.hashes[path]))
                && manifest_files
                    .get(path)
                    .is_some_and(|file| file["sha256"] == loaded.snapshot.hashes[path]
                        && file["bytes"].as_u64() == Some(bytes.len() as u64)),
            "code_artifact_composition_input_missing:{path}"
        );
    }
    let normalized = loaded.normalized()?;
    let source = loaded.source_bytes()?;
    ensure!(
        read_regular_bytes(
            &root.join("generated/normalized-design.json"),
            16 * 1024 * 1024
        )? == normalized
            && sha256(&normalized) == receipt["design_sha256"]
            && read_regular_bytes(
                &root.join("generated/design-composition-source.json"),
                16 * 1024 * 1024
            )? == source,
        "code_artifact_composed_products_changed"
    );
    let mut products = oe_design::generation::snapshot(&root.join("generated"))?;
    products.remove("generation.json");
    ensure!(
        serde_json::to_value(&products)? == receipt["products"],
        "code_artifact_generation_products_changed"
    );
    for (path, hash) in products {
        ensure!(
            manifest_files
                .get(&format!("generated/{path}"))
                .is_some_and(|file| file["sha256"] == hash),
            "code_artifact_generation_product_not_in_manifest:{path}"
        );
    }
    loaded.verify(root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oe_wire::json::canonical_json;
    fn actual() -> (std::path::PathBuf, Value, Value) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_owned();
        let receipt =
            strict_json(&std::fs::read(root.join("generated/generation.json")).unwrap()).unwrap();
        let current =
            oe_design::composition::load(&root, &root.join("specs/engineering.toml")).unwrap();
        assert_eq!(receipt["schema"], current.generation_schema());
        let manifest = crate::public_manifest(&root).unwrap();
        (root, receipt, manifest)
    }
    #[test]
    fn composed_generation_matches_actual_source_and_products() {
        let (root, receipt, manifest) = actual();
        verify_composed(&root, &receipt, &manifest).unwrap();
    }
    #[test]
    fn composed_generation_rejects_lost_rebound_or_forged_inputs() {
        let (root, receipt, manifest) = actual();
        let original = canonical_json(&receipt).unwrap();
        let source = receipt["composition_source"]["files"][0]["path"]
            .as_str()
            .unwrap()
            .to_owned();
        for fault in [
            "source",
            "capture",
            "input",
            "product",
            "resource",
            "manifest",
            "outside_root",
            "schema",
        ] {
            let mut r = receipt.clone();
            let mut m = manifest.clone();
            let expected = match fault {
                "source" => {
                    r["composition_source"]["files"]
                        .as_array_mut()
                        .unwrap()
                        .pop();
                    "code_artifact_composition_source_changed"
                }
                "capture" => {
                    r["composition_binding"]["compiled_files"]
                        .as_object_mut()
                        .unwrap()
                        .remove(&source);
                    "code_artifact_composition_capture_changed"
                }
                "input" => {
                    r["source_inputs"].as_object_mut().unwrap().remove(&source);
                    "code_artifact_composition_input_missing"
                }
                "product" => {
                    r["products"]["contracts.rs"] = json!("0".repeat(64));
                    "code_artifact_generation_products_changed"
                }
                "resource" => {
                    r["composition_resource"]["total_bytes"] = json!(16 * 1024 * 1024);
                    "code_artifact_composed_generation_profile"
                }
                "manifest" => {
                    m["files"][&source]["sha256"] = json!("0".repeat(64));
                    "code_artifact_generation_source_not_in_manifest"
                }
                "outside_root" => {
                    r["composition_source"]["root"] = json!("../outside.toml");
                    "design_unsafe_path"
                }
                "schema" => {
                    r["schema"] = if receipt["schema"] == "oev2.generation/3" {
                        json!("oev2.generation/2")
                    } else {
                        json!("oev2.generation/3")
                    };
                    "code_artifact_composed_generation_schema_mismatch"
                }
                _ => unreachable!(),
            };
            let error = verify_composed(&root, &r, &m).unwrap_err().to_string();
            assert!(error.starts_with(expected), "{fault}: {error}");
        }
        assert_eq!(canonical_json(&receipt).unwrap(), original);
        verify_composed(&root, &receipt, &manifest).unwrap();
    }
}
