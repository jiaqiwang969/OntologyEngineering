//! Tests of the actual private publication functions, using synthetic objects
//! and real private files. These are not fresh driver/runner execution evidence.
use super::*;
use oe_contracts::{ArtifactReference, CodeScopeAssociationIndex, CodeScopeAssociationResult};
use serde_json::{Value, json};
use std::{cell::Cell, fs, path::PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
fn publication_root() -> (tempfile::TempDir, SkillRoot, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().canonicalize().unwrap();
    fs::write(
        path.join("SKILL.md"),
        "synthetic private publication unit fixture; not qualified",
    )
    .unwrap();
    let root = SkillRoot::new(&path).unwrap();
    let folder = root
        .new_private_dir(Path::new("var/code-qualification/synthetic-unit-only"))
        .unwrap();
    (temp, root, folder)
}
fn objects() -> (CodeScopeAssociationIndex, SelectedReport) {
    let hash = "a".repeat(64);
    let source = json!({"path":"var/synthetic-not-executable/source.json","sha256":hash});
    let items = oe_evolution::scope_witness::unknown_bindings(&workspace()).unwrap().into_iter()
        .map(|row| json!({"scope_id":row.scope_id,"observed_case_ids":[],"behavior_status":"unknown"}))
        .collect::<Vec<_>>();
    assert_eq!(items.len(), 13494);
    let relations = oe_evolution::scope_witness::q40::catalog().unwrap()["relations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            json!({"scope_id":row["scope_id"],"case_id":row["case_id"],"role":row["role"],
            "observation":"unknown","behavior_status":"unknown"})
        })
        .collect::<Vec<_>>();
    let observation = json!({
        "schema":"oev2.code-scope-witness-observation/2",
        "scope_sha256":oe_evolution::scope_witness::SCOPE_SHA,
        "scope_lock_sha256":oe_evolution::scope_witness::LOCK_SHA,"fixed_scope_count":13494,
        "items":items,"behavior_status":"unknown","eligibility":"unknown","whole_code_cycle":"not_connected",
        "uncovered":["synthetic publication objects only; no semantic execution"],
        "witness":{"schema":"oev2.code-behavior-witness/1","experiment_sha256":hash,
            "driver_sha256":hash,"evaluator_sha256":hash,"oracle_sha256":oe_evolution::scope_witness::q40::ORACLE_SHA,
            "baseline_sha256":hash,"candidate_sha256":hash,"execution":"partial","comparison":null,
            "process":source,"uncovered":["never executed"]}
    });
    let report = SelectedReport::parse(json!({"schema":"oev2.code-scope-witness-report/2",
        "catalog_sha256":oe_evolution::scope_witness::q40::CATALOG_SHA,"relations":relations,
        "observation":observation}))
    .unwrap();
    let index: CodeScopeAssociationIndex = serde_json::from_value(json!({
        "schema":"oev2.code-scope-association-index/1",
        "scope_sha256":oe_evolution::scope_witness::SCOPE_SHA,
        "scope_lock_sha256":oe_evolution::scope_witness::LOCK_SHA,"fixed_scope_count":13494,
        "request":{"schema":"oev2.code-artifact-eligibility-request/1","binding_id":"synthetic-only","revision":"r2",
            "scope_id":"semantica.full-library/1","source":source,"assurance":source},
        "candidate_manifest_sha256":hash,"policy_sha256":hash,"context":source,
        "candidate_mappings_sha256":null,"witness":source,"items":items,"implementations":[],
        "uncovered":["synthetic publication objects only; not actual qualification"],
        "behavior_status":"unknown","eligibility":"unknown"
    })).unwrap();
    (index, report)
}
fn stage_index(root: &SkillRoot, folder: &Path, value: &Value) -> ArtifactReference {
    write_new(&folder.join("scope-association-index.json"), value).unwrap();
    artifacts::reference(root, &folder.join("scope-association-index.json")).unwrap()
}
fn receipt(index: Option<ArtifactReference>, completed: bool) -> CodeScopeAssociationResult {
    let execution = if completed { "completed" } else { "failed" };
    CodeScopeAssociationResult {
        schema: "oev2.code-scope-association-result/1".into(),
        execution: execution.into(),
        index,
        reason: "synthetic publication boundary only; no actual owner qualification".into(),
        qualification: CodeArtifactEligibilityResult {
            schema: "oev2.code-artifact-eligibility-result/1".into(),
            execution: execution.into(),
            eligibility: "unknown".into(),
            artifact_sha256: "a".repeat(64),
            comparison_sha256: "b".repeat(64),
            reason: "synthetic object; not fresh process evidence".into(),
            uncovered: vec!["all real execution pending".into()],
        },
    }
}

#[test]
fn q40_owner_wrapper_preserves_inventory_roles_and_explicit_wire_version() {
    let (index, report) = objects();
    let exact_inventory = serde_json::to_value(&index).unwrap();
    let value = wrap_association_index(index.clone(), &report).unwrap();
    assert_eq!(value["schema"], "oev2.code-scope-association-index/2");
    assert_eq!(value["inventory"], exact_inventory);
    assert_eq!(value["inventory"]["items"].as_array().unwrap().len(), 13494);
    assert_eq!(value["relations"], report.value().unwrap()["relations"]);
    assert_eq!(value["relations"].as_array().unwrap().len(), 40);
    assert!(serde_json::from_value::<CodeScopeAssociationIndex>(value.clone()).is_err());
    let (_temp, root, folder) = publication_root();
    let reference = stage_index(&root, &folder, &value);
    validate_retained_association_index(&root, &reference, &report).unwrap();
    let mut legacy = report.observation().clone();
    legacy.schema = "oev2.code-scope-witness-report/1".into();
    let legacy = SelectedReport::Legacy(legacy);
    assert_eq!(
        wrap_association_index(index, &legacy).unwrap(),
        exact_inventory,
        "legacy serialization must not acquire a /2 wrapper"
    );
    assert!(validate_retained_association_index(&root, &reference, &legacy).is_err());
}

#[test]
fn q40_owner_final_read_rejects_changed_bytes_and_rebound_roles_without_receipt() {
    for rebound in [false, true] {
        let (index, report) = objects();
        let (_temp, root, folder) = publication_root();
        let mut value = wrap_association_index(index, &report).unwrap();
        let original_ref = stage_index(&root, &folder, &value);
        // Change only this synthetic private artifact, never a runtime/catalog.
        value["relations"][0]["role"] = json!("backend_dependency");
        fs::write(
            folder.join("scope-association-index.json"),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        let reference = if rebound {
            artifacts::reference(&root, &folder.join("scope-association-index.json")).unwrap()
        } else {
            original_ref
        };
        let err = finish_association_receipt(
            &root,
            &folder,
            &receipt(Some(reference), true),
            Some(&report),
            || Ok(()),
        )
        .unwrap_err();
        let text = format!("{err:?}");
        assert!(
            text.contains(if rebound {
                "q40_association_retained_relations_changed"
            } else {
                "artifact_source_changed"
            }),
            "{text}"
        );
        assert!(!folder.join("scope-association-receipt.json").exists());
        assert!(
            folder.join("scope-association-index.json").is_file(),
            "no fictitious transaction rollback"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(
                &fs::read(folder.join("scope-association-index.json")).unwrap()
            )
            .unwrap(),
            value
        );
    }
}

#[test]
fn q40_owner_final_guard_runs_before_retained_read_and_failure_keeps_originals() {
    let (index, report) = objects();
    let (_temp, root, folder) = publication_root();
    let value = wrap_association_index(index, &report).unwrap();
    let reference = stage_index(&root, &folder, &value);
    let before = fs::read(folder.join("scope-association-index.json")).unwrap();
    let input = folder.join("synthetic-original-input.json");
    fs::write(&input, b"{\"synthetic\":1}").unwrap();
    let bound = artifacts::reference(&root, &input).unwrap();
    fs::write(&input, b"{\"synthetic\":2}").unwrap();
    let checks = Cell::new(0);
    let error = finish_association_receipt(
        &root,
        &folder,
        &receipt(Some(reference), true),
        Some(&report),
        || {
            checks.set(checks.get() + 1);
            // Actual existing artifact identity checker over a changed private input.
            // This is not the full production snapshot/context/raw-stream guard.
            artifacts::json(&root, &bound)?;
            Ok(())
        },
    )
    .unwrap_err();
    assert!(format!("{error:?}").contains("artifact_source_changed"));
    assert_eq!(checks.get(), 1);
    assert!(!folder.join("scope-association-receipt.json").exists());
    assert_eq!(
        fs::read(folder.join("scope-association-index.json")).unwrap(),
        before
    );
    // If the guard removed the retained file, the finalizer must read afterward
    // and fail: this discriminates a stale pre-guard read from the real order.
    let reference =
        artifacts::reference(&root, &folder.join("scope-association-index.json")).unwrap();
    assert!(
        finish_association_receipt(
            &root,
            &folder,
            &receipt(Some(reference), true),
            Some(&report),
            || {
                fs::remove_file(folder.join("scope-association-index.json"))?;
                Ok(())
            }
        )
        .is_err()
    );
    assert!(!folder.join("scope-association-receipt.json").exists());
}

#[test]
fn q40_owner_failed_result_has_no_index_and_receipt_write_cannot_overwrite() {
    let (index, report) = objects();
    let (_temp, root, folder) = publication_root();
    let value = wrap_association_index(index, &report).unwrap();
    let reference = stage_index(&root, &folder, &value);
    let index_bytes = fs::read(folder.join("scope-association-index.json")).unwrap();
    let final_path = folder.join("scope-association-receipt.json");
    fs::write(&final_path, b"preserved prior terminal bytes").unwrap();
    assert!(
        finish_association_receipt(
            &root,
            &folder,
            &receipt(Some(reference), true),
            Some(&report),
            || Ok(())
        )
        .is_err()
    );
    assert_eq!(
        fs::read(&final_path).unwrap(),
        b"preserved prior terminal bytes"
    );
    assert_eq!(
        fs::read(folder.join("scope-association-index.json")).unwrap(),
        index_bytes
    );
    let failure_folder = root
        .new_private_dir(Path::new("var/code-qualification/synthetic-failed-only"))
        .unwrap();
    let failed = receipt(None, false);
    let called = Cell::new(false);
    finish_association_receipt(&root, &failure_folder, &failed, None, || {
        called.set(true);
        Ok(())
    })
    .unwrap();
    assert!(called.get());
    let stored: Value = serde_json::from_slice(
        &fs::read(failure_folder.join("scope-association-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stored, serde_json::to_value(failed).unwrap());
    assert_eq!(stored["execution"], "failed");
    assert_eq!(stored["qualification"]["eligibility"], "unknown");
    assert!(stored["index"].is_null());
    assert!(!failure_folder.join("scope-association-index.json").exists());
}
