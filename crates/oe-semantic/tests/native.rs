//! Real external Semantica integration, explicitly opted in on an installed host.
//! It copies a previously source-bound synthetic registry into a fresh private
//! directory; no actual project or source artifact is mutated.
use oe_semantic::native::{DecisionReviewRequest, NativeRuntime, OpenEngagementRequest};
use oe_semantic::{canonical_json, digest, strict_json};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let path = root.join(format!(
            "var/builds/rust-native-tests/{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        oe_core::create_new_private_dir(&path).unwrap();
        Self { root, path }
    }
    fn runtime(&self) -> NativeRuntime {
        NativeRuntime::new(&self.root, &self.root.join("runtime/.venv/bin/python")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn read(path: &Path) -> Value {
    strict_json(&fs::read(path).unwrap()).unwrap()
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
fn native_binding(value: &Value) -> Value {
    let authority =
        |kind: &str| String::from_utf8(canonical_json(&value["authority"][kind]).unwrap()).unwrap();
    json!({"schema_version":"1.0","binding_id":value["binding_id"],"project_id":value["project"]["project_id"],
        "domain":value["project"]["domain"],"package_id":value["semantic_target"]["package_id"],
        "workspace_id":value["semantic_target"]["workspace_id"],"baseline_version":value["baseline"]["version"],
        "baseline_package_sha256":value["baseline"]["digest"],"evidence_root":value["evidence"]["logical_root"],
        "fact_authorities":[authority("fact")],"decision_authorities":[authority("decision")],
        "allowed_actions":value["lifecycle_actions"],"semantic_api_contract":value["semantic_api"],
        "promotion_target":value["promotion"]["target"],"created_at":value["created_at"]})
}

#[test]
#[ignore = "requires installed locked Semantica and synthetic native-harness fixture; no network"]
fn actual_native_review_clear_blocked_and_stale_binding() {
    let fixture = Fixture::new();
    let source = std::env::var_os("OE_NATIVE_REVIEW_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            fixture
                .root
                .join("var/maintenance/dual-loop-refactor-20260927/native-harness-e2e")
        });
    let workspace = fixture.path.join("registry");
    copy_tree(&source.join("inputs/registry"), &workspace);
    let binding = native_binding(&read(&source.join("inputs/binding.json")));
    let runtime = fixture.runtime();
    for (name, status) in [("review-current", "clear"), ("review-stale", "blocked")] {
        let directory = source.join("first/semantic-feedback").join(name);
        let audit = read(&directory.join("projection.json"));
        let task = read(&directory.join("task.json"));
        let reference = &task["evidence"][0];
        let field = |v: &Value, k: &str| v[k].as_str().unwrap().to_owned();
        let request = DecisionReviewRequest {
            workspace: workspace.clone(),
            binding: binding.clone(),
            evidence: fs::read(directory.join("projection.ttl")).unwrap(),
            evidence_sha256: field(reference, "sha256"),
            evidence_format: "turtle".into(),
            evidence_id: field(reference, "source_id"),
            evidence_uri: field(reference, "uri"),
            evidence_captured_at: field(reference, "captured_at"),
            scope_id: field(&audit, "scope"),
            focus_iri: field(&audit, "focus"),
            focus_type_iri: field(&audit, "focus_type"),
            query_asset_id: field(&audit, "query_asset"),
            shape_asset_id: field(&audit, "shape_asset"),
            created_at: None,
        };
        let result = runtime.decision_review(&request);
        assert_eq!(result.semantic_execution, "completed", "{result:?}");
        let value = result.into_result().unwrap();
        assert_eq!(value["review"]["status"], status);
        assert_eq!(value["execution_registry_id"], binding["workspace_id"]);
        assert_eq!(
            value["review"]["evidence"]["sha256"],
            digest(&request.evidence)
        );
        if status == "blocked" {
            assert!(
                !value["review"]["findings"].as_array().unwrap().is_empty()
                    || !value["review"]["violations"].as_array().unwrap().is_empty()
            );
        }
        let mut stale = request.clone();
        stale.binding["baseline_package_sha256"] = "0".repeat(64).into();
        let rejected = runtime.decision_review(&stale);
        assert_eq!(rejected.semantic_execution, "not_run", "{rejected:?}");
        assert!(rejected.error.unwrap().contains("baseline_mismatch"));
    }
}

#[test]
#[ignore = "requires installed source-locked Semantica; no network"]
fn actual_native_runtime_task_and_new_blocked_engagement() {
    let fixture = Fixture::new();
    let runtime = fixture.runtime();
    let result = runtime.create_runtime();
    assert_eq!(result.semantic_execution, "completed", "{result:?}");
    assert_eq!(result.into_result().unwrap()["name"], "ontology-runtime");
    let empty = digest(
        canonical_json(
            &json!({"schema_version":"1.0","package_id":null,"version":"0","assets":[]}),
        )
        .unwrap(),
    );
    let binding = json!({"schema_version":"1.0","binding_id":"rust-synthetic-binding","project_id":"rust-native-test",
        "domain":"engineering-evidence","package_id":"synthetic.empty","workspace_id":"rust-native-registry",
        "baseline_version":"0","baseline_package_sha256":empty,"evidence_root":"evidence:rust-native-test",
        "fact_authorities":["fixture-author"],"decision_authorities":["fixture-controller"],
        "allowed_actions":["candidate","proposed"],"semantic_api_contract":"semantica.ontology.refinery/v1",
        "promotion_target":"industry-registry","created_at":"2026-09-27T00:00:00Z"});
    let envelope = json!({"schema_version":"1.0","task_id":"rust-native-test","task_kind":"synthetic-test","project_id":"rust-native-test",
        "domain":"engineering-evidence","intent":"Check the native boundary","requested_decision":"No engineering acceptance",
        "actor_id":"fixture-controller","requested_actions":["engagement"],"required_capabilities":["semantica.ontology.decision-review/v1"],
        "evidence":[{"source_id":"fixture","uri":"evidence:rust-native-test/fixture","sha256":digest(b"fixture"),"media_type":"text/plain","captured_at":"2026-09-27T00:00:00Z"}],
        "created_at":"2026-09-27T00:00:00Z"});
    let dto = runtime
        .task_and_binding(&envelope, &binding)
        .into_result()
        .unwrap();
    assert!(dto["envelope"]["envelope_sha256"].is_string());
    let result = runtime.open_engagement(&OpenEngagementRequest {
        workspace: fixture.path.join("new-registry"),
        binding,
        envelope,
    });
    assert_eq!(result.semantic_execution, "completed", "{result:?}");
    let result = result.into_result().unwrap();
    assert_eq!(result["status"], "blocked");
    assert_eq!(result["learning"]["status"], "no_delta");
    assert!(result["execution_receipts"].as_array().unwrap().is_empty());
}

#[test]
#[ignore = "requires installed source-locked Semantica; no network"]
fn actual_package_run_verifies_with_the_same_retained_native_runner() {
    let fixture = Fixture::new();
    let result = fixture.runtime().run_and_verify_package(
        "semantica.chapter_packages.vol1.ch03",
        Some("OE-V1-CH03-SCN-CQ-ACCEPTANCE-001"),
    );
    assert_eq!(result.semantic_execution, "completed", "{result:?}");
    let result = result.into_result().unwrap();
    assert_eq!(
        result["run"]["package_id"],
        "semantica.chapter_packages.vol1.ch03"
    );
    assert_eq!(
        result["run"]["scenario_id"],
        "OE-V1-CH03-SCN-CQ-ACCEPTANCE-001"
    );
    assert_eq!(result["run"]["status"], "passed");
    assert!(result["run"]["receipt"].is_object());
    assert!(result["verification"].is_object());
    // The chapter remains partial even though this scenario executes.
    assert_eq!(result["verification"]["status"], "blocked");
    let checks = result["verification"]["checks"].as_array().unwrap();
    assert!(
        checks.iter().any(
            |check| check["check_id"] == "runner.scenario_execution" && check["passed"] == true
        )
    );
    assert!(
        !checks.iter().any(
            |check| check["check_id"] == "runner.execution_context" && check["passed"] == false
        )
    );
}
