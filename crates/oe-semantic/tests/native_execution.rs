//! Explicitly opted-in external execution tests use only isolated copies of a
//! small synthetic registry. They neither bootstrap the large method catalog
//! nor mutate the retained fixture or an actual engineering project.
use oe_semantic::native::{NativeExecution, NativeRuntime, RegistryRequest, RegistryRunRequest};
use oe_semantic::{digest, strict_json};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const STAMP: &str = "2026-09-27T00:00:00Z";
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn directory(label: &str) -> PathBuf {
    let path = root().join(format!(
        "var/maintenance/rust-rewrite-20260927/phase4-native/{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    oe_core::create_new_private_dir(&path).unwrap();
    path
}
fn runtime() -> NativeRuntime {
    NativeRuntime::new(&root(), &root().join("runtime/.venv/bin/python")).unwrap()
}
fn retain(path: &Path, name: &str, value: &impl serde::Serialize) {
    oe_core::write_new_bytes(
        &path.join(format!("{name}.json")),
        &serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}
fn completed(path: &Path, name: &str, value: NativeExecution) -> Value {
    retain(path, name, &value);
    assert_eq!(value.semantic_execution, "completed", "{name}: {value:?}");
    value.into_result().unwrap()
}
fn copy_tree(source: &Path, target: &Path) {
    let metadata = fs::symlink_metadata(source).unwrap();
    assert!(!metadata.file_type().is_symlink());
    if metadata.is_dir() {
        fs::create_dir(target).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            copy_tree(&entry.path(), &target.join(entry.file_name()));
        }
    } else {
        assert!(metadata.is_file());
        fs::copy(source, target).unwrap();
    }
}
fn inventory(path: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, path: &Path, output: &mut BTreeMap<String, String>) {
        for item in fs::read_dir(path).unwrap() {
            let path = item.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            assert!(!meta.file_type().is_symlink());
            if meta.is_dir() {
                walk(base, &path, output);
            } else {
                assert!(meta.is_file());
                output.insert(
                    path.strip_prefix(base).unwrap().to_str().unwrap().into(),
                    digest(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut output = BTreeMap::new();
    walk(path, path, &mut output);
    output
}
fn fixture_source() -> PathBuf {
    let path=std::env::var_os("OE_NATIVE_EXECUTION_FIXTURE").map(PathBuf::from).unwrap_or_else(||
        root().join("var/maintenance/rust-rewrite-20260927/lifecycle-native/actual-15427-1790573609433903000"));
    oe_core::SkillRoot::new(root())
        .unwrap()
        .private_path(&path)
        .unwrap()
}
fn binding(current: &Value) -> Value {
    json!({"schema_version":"1.0","binding_id":"rust-lifecycle-binding","project_id":"rust-lifecycle",
        "domain":"native-boundary-acceptance","package_id":"rust.native.lifecycle.acceptance","workspace_id":"rust-lifecycle-registry",
        "baseline_version":current["subject"]["version"],"baseline_package_sha256":current["subject"]["package_sha256"],
        "evidence_root":"evidence:rust-lifecycle","fact_authorities":["fixture-author"],"decision_authorities":["fixture-controller"],
        "allowed_actions":["candidate","proposed","committed","regression_passed","release_complete","promoted"],
        "semantic_api_contract":"semantica.ontology.refinery/v1","promotion_target":"industry-registry","created_at":STAMP})
}

#[test]
fn run_inputs_fail_before_interpreter_or_workspace_creation() {
    let path = directory("invalid-inputs");
    let runtime = NativeRuntime::new(&root(), Path::new("/unavailable/native-python")).unwrap();
    for result in [
        runtime.run_and_verify_package_at("", None, None),
        runtime.run_and_verify_package_at("registered", Some(""), None),
        runtime.run_and_verify_package_at("registered", None, Some("2026-99-00")),
        runtime.run_registry(&RegistryRunRequest {
            registry: RegistryRequest {
                workspace: path.join("missing"),
                binding: json!({}),
            },
            scenario_id: None,
            created_at: None,
        }),
    ] {
        assert_eq!(result.semantic_execution, "not_run");
        assert!(result.runtime_identity.is_none());
        assert!(result.result.is_none());
    }
    assert!(!path.join("missing").exists());
    fs::remove_dir(path).unwrap();
}

#[cfg(unix)]
#[test]
fn registry_run_rejects_symlink_before_interpreter() {
    let path = directory("symlink-input");
    fs::create_dir(path.join("real")).unwrap();
    std::os::unix::fs::symlink(path.join("real"), path.join("alias")).unwrap();
    let runtime = NativeRuntime::new(&root(), Path::new("/unavailable/native-python")).unwrap();
    let result = runtime.run_registry(&RegistryRunRequest {
        registry: RegistryRequest {
            workspace: path.join("alias"),
            binding: json!({}),
        },
        scenario_id: None,
        created_at: None,
    });
    assert_eq!(result.semantic_execution, "not_run");
    assert!(result.runtime_identity.is_none());
    fs::remove_dir_all(path).unwrap();
}

#[test]
#[ignore = "requires installed source-locked Semantica; native diagnostics and small built-in scenarios only"]
fn actual_diagnostics_builtin_identity_default_and_partial_release() {
    let path = directory("actual-builtins");
    let runtime = runtime();
    let diagnostics = completed(&path, "diagnostics", runtime.registry_diagnostics());
    assert_eq!(diagnostics["chapters"].as_array().unwrap().len(), 29);
    assert!(!diagnostics["domains"].as_array().unwrap().is_empty());
    assert!(diagnostics["chapter_issues"].as_array().unwrap().is_empty());
    assert!(diagnostics["domain_issues"].as_array().unwrap().is_empty());
    for key in ["chapters", "domains"] {
        assert!(
            diagnostics[key]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v.get("manifest_path").is_none())
        );
    }
    let value = completed(
        &path,
        "builtin-default",
        runtime.run_and_verify_package_at(
            "semantica.chapter_packages.vol1.ch03",
            None,
            Some(STAMP),
        ),
    );
    assert_eq!(
        value["run"]["scenario_id"],
        "OE-V1-CH03-SCN-CQ-ACCEPTANCE-001"
    );
    assert_eq!(value["run"]["status"], "passed");
    assert_eq!(value["verification"]["status"], "blocked");
    assert_eq!(
        value["run"]["package_digest"],
        value["subject"]["package_digest"]
    );
    assert_eq!(
        value["loaded_identity"]["digest"],
        value["subject"]["package_digest"]
    );
    assert_eq!(value["manifest"]["version"], value["descriptor"]["version"]);
    assert_eq!(value["run"]["created_at"], STAMP);
    assert_eq!(value["verification"]["checked_at"], STAMP);
    assert_eq!(value["receipt_integrity"], true);
    let missing = completed(
        &path,
        "builtin-unknown-scenario",
        runtime.run_and_verify_package_at(
            "semantica.chapter_packages.vol1.ch03",
            Some("not-registered"),
            Some(STAMP),
        ),
    );
    assert_eq!(missing["run"]["status"], "blocked");
    assert_eq!(missing["verification"]["status"], "blocked");
    assert!(
        missing["run"]["oracle_checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["actual"]["error_code"] == "missing_scenario")
    );
    let missing_package = runtime.run_and_verify_package("not.a.registered.package", None);
    retain(&path, "builtin-missing-package", &missing_package);
    assert_eq!(missing_package.semantic_execution, "failed");
    assert!(missing_package.result.is_some());
    println!("retained actual built-in execution: {}", path.display());
}

#[test]
#[ignore = "requires source-locked Semantica and the retained synthetic four-scenario native lifecycle fixture"]
fn actual_registry_run_is_read_only_current_bound_and_preserves_blocked_scenarios() {
    let path = directory("actual-registry");
    let source = fixture_source();
    let source_before = inventory(&source.join("registry"));
    let current =
        strict_json(&fs::read(source.join("current.json")).unwrap()).unwrap()["result"].clone();
    copy_tree(&source.join("registry"), &path.join("registry"));
    let before = inventory(&path.join("registry"));
    let request = RegistryRunRequest {
        registry: RegistryRequest {
            workspace: path.join("registry"),
            binding: binding(&current),
        },
        scenario_id: Some("scenario-current".into()),
        created_at: Some(STAMP.into()),
    };
    retain(
        &path,
        "request",
        &json!({"workspace":request.registry.workspace,"binding":request.registry.binding,"scenario_id":request.scenario_id,"created_at":request.created_at}),
    );
    let runtime = runtime();
    let result = completed(&path, "current", runtime.run_registry(&request));
    assert_eq!(result["subject"], current["subject"]);
    assert_eq!(
        result["registry_id"],
        request.registry.binding["workspace_id"]
    );
    assert_eq!(result["executor"]["status"], "passed");
    assert_eq!(result["release"]["status"], "complete");
    assert_eq!(result["receipt_integrity"], true);
    // Registry package identity and runnable projection identity are distinct.
    assert_ne!(
        result["subject"]["package_sha256"],
        result["executor"]["package_digest"]
    );
    assert_eq!(
        result["executor"]["package_digest"],
        result["loaded_identity"]["digest"]
    );
    let default = completed(
        &path,
        "ambiguous-default",
        runtime.run_registry(&RegistryRunRequest {
            scenario_id: None,
            ..request.clone()
        }),
    );
    assert_eq!(default["executor"]["status"], "blocked");
    assert_eq!(default["executor"]["scenario_id"], "unresolved");
    assert_eq!(default["release"]["status"], "blocked");
    assert!(
        default["executor"]["oracle_checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["actual"]["error_code"] == "ambiguous_scenario")
    );
    let unknown = completed(
        &path,
        "unknown-scenario",
        runtime.run_registry(&RegistryRunRequest {
            scenario_id: Some("not-registered".into()),
            ..request.clone()
        }),
    );
    assert_eq!(unknown["executor"]["status"], "blocked");
    assert_eq!(unknown["release"]["status"], "blocked");
    for (name, key, value) in [
        ("stale-version", "baseline_version", "0"),
        ("stale-digest", "baseline_package_sha256", &"0".repeat(64)),
        ("wrong-registry", "workspace_id", "another-registry"),
    ] {
        let mut bad = request.clone();
        bad.registry.binding[key] = value.into();
        let rejected = runtime.run_registry(&bad);
        retain(&path, name, &rejected);
        assert_eq!(rejected.semantic_execution, "not_run");
        assert!(rejected.result.is_some());
        assert!(rejected.error.is_some());
    }
    assert_eq!(before, inventory(&request.registry.workspace));
    let metadata = request.registry.workspace.join("registry.json");
    let original = fs::read(&metadata).unwrap();
    let mut changed = strict_json(&original).unwrap();
    changed["registry_id"] = "replacement-registry".into();
    fs::write(&metadata, serde_json::to_vec(&changed).unwrap()).unwrap();
    let replaced = runtime.run_registry(&request);
    retain(&path, "replaced-registry-id", &replaced);
    assert_eq!(replaced.semantic_execution, "not_run");
    assert!(replaced.error.is_some());
    fs::write(&metadata, &original).unwrap();
    let mut events = fs::read_dir(request.registry.workspace.join("registry-events"))
        .unwrap()
        .map(|v| v.unwrap().path())
        .collect::<Vec<_>>();
    events.sort();
    let event = &events[0];
    let original = fs::read(event).unwrap();
    fs::write(event, b"{}").unwrap();
    let corrupted = runtime.run_registry(&request);
    retain(&path, "corrupt-history", &corrupted);
    assert_eq!(corrupted.semantic_execution, "not_run");
    assert!(corrupted.error.is_some());
    fs::write(event, original).unwrap();
    let asset = current["manifest"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["asset_id"] == "ontology-core")
        .unwrap()["sha256"]
        .as_str()
        .unwrap();
    let object = request
        .registry
        .workspace
        .join("objects/sha256")
        .join(&asset[..2])
        .join(asset);
    let original = fs::read(&object).unwrap();
    fs::write(&object, b"corrupt private fixture CAS").unwrap();
    let corrupted = runtime.run_registry(&request);
    retain(&path, "corrupt-cas", &corrupted);
    assert_eq!(corrupted.semantic_execution, "not_run");
    assert!(corrupted.error.is_some());
    fs::write(object, original).unwrap();
    assert_eq!(before, inventory(&request.registry.workspace));
    assert_eq!(source_before, inventory(&source.join("registry")));
    retain(&path, "unchanged-registry-inventory", &before);
    println!("retained actual registry execution: {}", path.display());
}
