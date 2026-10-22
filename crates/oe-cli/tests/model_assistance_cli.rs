//! Actual CLI admission, source binding and no-dispatch tests. The service and
//! credentials are synthetic; no live provider or engineering adoption occurs.
use oe_contracts::ObjectIdentity;
use oe_core::{SkillRoot, sha256};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn snapshot() -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let parent = source.join("var/maintenance/model-assistance-cli-tests");
    fs::create_dir_all(&parent).unwrap();
    let target = tempfile::tempdir_in(parent).unwrap();
    let manifest = oe_design::provenance::compiled().unwrap();
    let mut files: Vec<String> = manifest["files"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    files.extend(
        [
            "SKILL.md",
            "generated/contracts.rs",
            "generated/semantic-contracts.rs",
            "references/context-capabilities.json",
            "references/context-routing-instructions.json",
        ]
        .map(str::to_owned),
    );
    for relative in files {
        let path = target.path().join(&relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(source.join(relative), path).unwrap();
    }
    fs::create_dir_all(target.path().join("var")).unwrap();
    target
}
fn save(root: &Path, relative: &str, value: &Value) {
    fs::write(
        root.join(relative),
        serde_json::to_vec_pretty(value).unwrap(),
    )
    .unwrap();
}
fn load(root: &Path, relative: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(relative)).unwrap()).unwrap()
}
fn reference(root: &Path, relative: &str) -> Value {
    json!({"path":relative,"sha256":sha256(fs::read(root.join(relative)).unwrap())})
}
fn inputs(root: &Path) -> (Value, Value) {
    let support = "Nominal contact alone cannot establish physical holding capacity.\n";
    fs::write(root.join("var/support.txt"), support).unwrap();
    let source_sha = sha256(support.as_bytes());
    let object = ObjectIdentity {
        object_id: "urn:fixture:support".into(),
        revision: "r1".into(),
        condition_scope: "holding".into(),
        source_sha256: source_sha.clone(),
    };
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":object.object_id,
        "object_revision":"r1","purpose":"Review support before manufacture",
        "acceptance":["retain holding capacity unknown"],"facts":[],"assertions":[]});
    let project: Project = serde_json::from_value(json!({"schema":"oev2.project/1",
        "project_id":"model-cli-fixture","revision":"r1","purpose":"Review support before manufacture",
        "objects":{object.object_id.clone():"r1"},"conditions":{"holding":serde_json::to_string(&basis).unwrap()},
        "evidence":{"fixture-source":source_sha},"decisions":[],"dependencies":{}}))
    .unwrap();
    save(
        root,
        "var/project.json",
        &serde_json::to_value(&project).unwrap(),
    );
    let original = json!({"schema":"oev2.advisory-questions/1",
        "state":{"purpose":"Review support before manufacture","unknowns":["holding capacity unmeasured"],
            "source_text":"Nominal contact is not physical holding."},
        "questions":{"book":{"type":"choice","instructions":"Is source review needed?",
            "criteria":{"needed":"Required source review","not_needed":"Source review unnecessary","unknown":"Cannot determine"}}}});
    save(root, "var/original.json", &original);
    let mut original_ref = reference(root, "var/original.json");
    original_ref["role"] = json!("historical_model_request");
    let mut support_ref = reference(root, "var/support.txt");
    support_ref["role"] = json!("current_support");
    let input = json!({"schema":"oev2.model-assistance-input/1","project":reference(root,"var/project.json"),
        "object":object,"project_context":project::context(&project,&object).unwrap(),"purpose":project.purpose,
        "task":"Identify required holding evidence without inventing a load.",
        "original_requests":[original_ref],"support_files":[support_ref]});
    save(root, "var/input.json", &input);
    save(
        root,
        "var/profile.json",
        &json!({"schema":"oev2.model-service-profile/1",
        "service_id":"fixture-no-live-service","api_kind":"openai_responses",
        "endpoint":"http://127.0.0.1:1/v1/responses","requested_model":"fixture-large-context-1",
        "accepted_response_models":["fixture-large-context-1"],"context_window_tokens":2000000,
        "maximum_request_bytes":1000000,"maximum_output_tokens":2048,"maximum_response_bytes":1000000,
        "wrapper_reserve_tokens":4096,"timeout_seconds":1,
        "credential_file":root.join("var/definitely-absent-credential.txt"),
        "capacity_evidence":"Synthetic fixture declaration; provider capacity unverified"}),
    );
    assert!(!root.join("var/definitely-absent-credential.txt").exists());
    (serde_json::to_value(project).unwrap(), original)
}
fn run(root: &Path, action: &str, output: &str, live: bool) -> (i32, Value) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
    command
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root)
        .args([
            "project",
            "model-assistance",
            action,
            "--input",
            "var/input.json",
            "--profile",
            "var/profile.json",
            "--output-dir",
            output,
        ]);
    if live {
        command.arg("--live");
    }
    let output = command.output().unwrap();
    assert!(output.stdout.is_empty() || output.stderr.is_empty());
    let data = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    (
        output.status.code().unwrap(),
        serde_json::from_slice(data).unwrap(),
    )
}

#[test]
fn actual_prepare_preserves_original_questions_and_current_project_without_credentials() {
    let dir = snapshot();
    SkillRoot::new(dir.path()).unwrap();
    let (project, original) = inputs(dir.path());
    let project_before = fs::read(dir.path().join("var/project.json")).unwrap();
    let original_before = fs::read(dir.path().join("var/original.json")).unwrap();
    let (code, summary) = run(dir.path(), "prepare", "var/prepare", false);
    assert_eq!(code, 0);
    assert_eq!(summary["status"], "prepared");
    assert_eq!(summary["model_execution"], "not_run");
    let entry = load(dir.path(), "var/prepare/entry.json");
    assert_eq!(
        entry["execution_binding"]["normalizer"]["status"],
        "matched"
    );
    assert_eq!(entry["engineering_adoption"], "not_established");
    let request = load(dir.path(), "var/prepare/prepared/request.json");
    let payload: Value =
        serde_json::from_str(request["input"][0]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["current_project"], project);
    assert_eq!(payload["original_requests"][0]["request"], original);
    assert_eq!(
        payload["original_requests"][0]["request"]["questions"],
        original["questions"]
    );
    assert_eq!(request["store"], false);
    assert_eq!(request["truncation"], "disabled");
    assert_eq!(request["stream"], false);
    assert_eq!(
        fs::read(
            dir.path()
                .join("var/prepare/prepared/original-request-0.original")
        )
        .unwrap(),
        original_before
    );
    assert_eq!(
        fs::read(dir.path().join("var/project.json")).unwrap(),
        project_before
    );
    assert_eq!(
        fs::read(dir.path().join("var/original.json")).unwrap(),
        original_before
    );
    assert!(
        !dir.path()
            .join("var/definitely-absent-credential.txt")
            .exists()
    );
}

#[test]
fn actual_run_without_live_never_constructs_adapter_or_reads_missing_credential() {
    let dir = snapshot();
    inputs(dir.path());
    let (code, summary) = run(dir.path(), "run", "var/not-live", false);
    assert_eq!(code, 1);
    assert_eq!(summary["status"], "not_run");
    assert_eq!(summary["model_execution"], "not_run");
    assert_eq!(summary["adapter_error"], Value::Null);
    let entry = load(dir.path(), "var/not-live/entry.json");
    assert_eq!(entry["live_requested"], false);
    assert_eq!(entry["adapter_constructed"], false);
    assert_eq!(entry["adapter_error"], Value::Null);
    assert_eq!(entry["result"]["client_dispatch_attempts"], 0);
    assert_eq!(entry["result"]["adopted"], false);
    assert_eq!(entry["codex_execution"], false);
    assert_eq!(entry["automatic_judgment_execution"], false);
    assert!(
        !dir.path()
            .join("var/definitely-absent-credential.txt")
            .exists()
    );
}

#[test]
fn actual_oversized_live_request_is_blocked_before_missing_credential_read() {
    let dir = snapshot();
    inputs(dir.path());
    let mut profile = load(dir.path(), "var/profile.json");
    profile["maximum_request_bytes"] = json!(64);
    save(dir.path(), "var/profile.json", &profile);
    let (code, summary) = run(dir.path(), "run", "var/blocked-live", true);
    assert_eq!(code, 1);
    assert_eq!(summary["status"], "blocked");
    assert_eq!(summary["model_execution"], "not_run");
    assert_eq!(summary["adapter_error"], Value::Null);
    let entry = load(dir.path(), "var/blocked-live/entry.json");
    assert_eq!(entry["live_requested"], true);
    assert_eq!(entry["adapter_constructed"], false);
    assert_eq!(entry["adapter_error"], Value::Null);
    assert_eq!(entry["result"]["client_dispatch_attempts"], 0);
    assert!(
        entry["result"]["plan"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "model_assistance_request_byte_budget_exceeded")
    );
    assert_eq!(entry["result"]["engineering_adoption"], "not_established");
    assert!(
        !dir.path()
            .join("var/definitely-absent-credential.txt")
            .exists()
    );
}

#[test]
fn actual_entry_rejects_source_comment_change_and_exact_restoration_is_executable() {
    let dir = snapshot();
    inputs(dir.path());
    let (code, _) = run(dir.path(), "prepare", "var/source-before", false);
    assert_eq!(code, 0);
    let source = dir
        .path()
        .join("crates/oe-engineering/src/model_assistance/transport.rs");
    let original = fs::read(&source).unwrap();
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n// private CLI source binding counterexample\n");
    fs::write(&source, changed).unwrap();
    let (code, failure) = run(dir.path(), "prepare", "var/source-stale", false);
    assert_eq!(code, 2);
    assert_eq!(
        failure["error"]["reason"],
        "compiled_source_binding_mismatch"
    );
    assert!(!dir.path().join("var/source-stale").exists());
    fs::write(&source, &original).unwrap();
    assert_eq!(fs::read(&source).unwrap(), original);
    let (code, summary) = run(dir.path(), "prepare", "var/source-restored", false);
    assert_eq!(code, 0);
    assert_eq!(summary["status"], "prepared");
    let entry = load(dir.path(), "var/source-restored/entry.json");
    assert_eq!(
        entry["execution_binding"]["normalizer"]["status"],
        "matched"
    );
}
