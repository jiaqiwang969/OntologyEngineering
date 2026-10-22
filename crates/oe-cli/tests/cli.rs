use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNT: AtomicU64 = AtomicU64::new(0);

#[test]
fn lifecycle_without_runtime_delivers_one_blocked_response_without_registry() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.join("SKILL.md"),
        "# Isolated missing-runtime receiver\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(&fixture.0)
        .args(["semantics", "discover"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["$schema"],
        "ontology-engineering.semantic-engagement-response/v1"
    );
    assert_eq!(value["command_verdict"], "blocked");
    assert_eq!(value["native_execution"]["semantic_execution"], "not_run");
    assert_eq!(value["execution"]["status"], "blocked");
    assert_eq!(value["release"]["status"], "not_checked");
    assert_eq!(value["learning"]["publication"]["published"], Value::Null);
    assert!(!fixture.0.join("registry").exists());
    assert!(output.stderr.is_empty());
}

#[test]
fn invalid_candidate_never_becomes_successful_empty_history() {
    let fixture = Fixture::new();
    let output = command()
        .args(["semantics", "history", "--binding"])
        .arg(fixture.0.join("missing-binding.json"))
        .arg("--workspace")
        .arg(fixture.0.join("registry"))
        .args(["--candidate", "not-a-hash"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command_verdict"], "blocked");
    assert_eq!(value["execution"]["status"], "blocked");
    assert!(value["execution"].get("events").is_none());
    assert!(!fixture.0.join("registry").exists());
}

#[test]
fn lifecycle_backend_override_is_blocked_as_one_json_without_echo() {
    let output = command()
        .args(["semantics", "open", "--backend", "PRIVATE_BACKEND_VALUE"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command_verdict"], "blocked");
    assert_eq!(value["execution"]["error_type"], "BindingValidationError");
    assert_eq!(value["learning"]["promotion"]["status"], "not_requested");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_BACKEND_VALUE"));
    assert!(output.stderr.is_empty());
}

#[test]
fn doctor_keeps_full_diagnostics_separate_from_identity_only_mode() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("SKILL.md"), "# Missing runtime fixture\n").unwrap();
    for (mode, code) in [(vec![], 0), (vec!["--identity-only"], 1)] {
        let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
            .env_clear()
            .env("PATH", "")
            .arg("--root")
            .arg(&fixture.0)
            .args(["semantics", "doctor"])
            .args(&mode)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        if mode.is_empty() {
            assert_eq!(value["command"], "doctor");
            assert_eq!(value["command_verdict"], "blocked");
            assert_eq!(value["native_execution"]["semantic_execution"], "not_run");
        } else {
            assert_eq!(value["scope"], "external_runtime_identity");
            assert_eq!(value["status"], "blocked");
            assert_eq!(value["semantic_execution"], "not_run");
        }
        assert!(output.stderr.is_empty());
        assert!(!fixture.0.join("registry").exists());
    }
}

#[test]
fn native_execution_arguments_fail_closed_without_echoing_values() {
    for args in [
        vec!["doctor", "--backend", "PRIVATE_BACKEND_VALUE"],
        vec!["run", "--backend", "PRIVATE_BACKEND_VALUE"],
        vec!["doctor", "--staging-descriptor", "PRIVATE_STAGING_VALUE"],
        vec![
            "doctor",
            "--identity-only",
            "--binding",
            "PRIVATE_BINDING_VALUE",
        ],
        vec![
            "verify",
            "--binding",
            "missing-binding.json",
            "--candidate",
            "candidate",
            "--scenario",
            "PRIVATE_SCENARIO_VALUE",
        ],
        vec![
            "verify",
            "--binding",
            "missing-binding.json",
            "--workspace",
            "PRIVATE_WORKSPACE_VALUE",
        ],
    ] {
        let output = command().arg("semantics").args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["command_verdict"], "blocked", "{args:?}");
        assert_eq!(
            value["execution"]["error_type"], "BindingValidationError",
            "{args:?}"
        );
        assert!(output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_"));
    }
}

#[test]
fn semantic_root_failure_is_stable_delivery_not_private_path_output() {
    let fixture = Fixture::new();
    let absent = fixture.0.join("PRIVATE_ABSENT_SKILL_ROOT");
    for command in ["doctor", "run", "verify"] {
        let mut process = Command::new(env!("CARGO_BIN_EXE_oev2"));
        process
            .arg("--root")
            .arg(&absent)
            .args(["semantics", command]);
        if command != "doctor" {
            process.args(["--binding", "PRIVATE_BINDING_VALUE"]);
        }
        let output = process.output().unwrap();
        assert_eq!(output.status.code(), Some(0));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["command_verdict"], "blocked");
        assert!(output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_"));
        assert!(!absent.exists());
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oev2"));
    // Native acceptance: no Python executable or subprocess tool can be found.
    command
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(root());
    command
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = root().join(format!(
            "var/tests/unified-system/cli/cli-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn source_bundle_validates_with_no_python_on_path() {
    let output = command()
        .args([
            "semantics",
            "bundle",
            "--name",
            "engineering-judgment-intake",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["package_id"], "semantica.engineering.judgment-intake");
}

fn harness_input(modules: Value) -> Value {
    json!({"schema":"oev2.harness-checkpoint-input/1", "task_id":"premise-handoff",
        "context":"Synthetic CLI premise handoff; no engineering conclusion.",
        "request":"Keep each capability's scoped premise selection.",
        "harness":{"project_id":"fixture-project", "decision_id":"fixture-decision", "object_id":"fixture-object",
            "revision":"r1", "problem_class":"premise-handoff", "goal":"Preserve premise coverage",
            "conditions":[], "constraints":[], "unknowns":["Physical applicability remains unverified"],
            "semantic_modules":modules, "review_requests":[], "observations":[],
            "checkpoint_policy":{"schema":"oev2.harness-checkpoint-policy/1","mode":"verified_no_model",
                "byte_budget":16777216,"reason":"Retain source-bound premises without a model"}}})
}

fn write_checkpoint_input(fixture: &Fixture) -> PathBuf {
    let path = fixture.0.join("checkpoint-input.json");
    fs::write(
        &path,
        serde_json::to_vec(&harness_input(json!([]))).unwrap(),
    )
    .unwrap();
    path
}

fn premise_selection(capability: &str, asset: &str) -> Value {
    json!({"bundle":"engineering-judgment-intake", "asset_ids":[asset],
        "purpose":format!("Synthetic {capability} selection"),
        "applicability":{"capability_id":capability, "applicability_status":"caller_assertion_not_established"}})
}

#[test]
fn harness_premises_preserve_distinct_capabilities_in_the_same_bundle() {
    let f = Fixture::new();
    let evidence = premise_selection("evidence", "query-pattern-claim");
    let modeling = premise_selection("project_modeling", "query-pattern-realization");
    let input = f.0.join("input.json");
    let advice = f.0.join("advice.json");
    fs::write(
        &input,
        serde_json::to_vec(&harness_input(json!([evidence]))).unwrap(),
    )
    .unwrap();
    fs::write(
        &advice,
        serde_json::to_vec(&json!({"selections":[evidence, modeling]})).unwrap(),
    )
    .unwrap();
    let output = f.0.join("step");
    let response = command()
        .args(["harness", "step", "--input"])
        .arg(&input)
        .arg("--premises")
        .arg(&advice)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(
        response.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&response.stdout),
        String::from_utf8_lossy(&response.stderr)
    );
    let snapshot: Value =
        serde_json::from_slice(&fs::read(output.join("premises.json")).unwrap()).unwrap();
    assert_eq!(snapshot["asset_count"], 2);
    let selections = snapshot["selections"].as_array().unwrap();
    assert_eq!(selections.len(), 2);
    assert_eq!(selections[0]["applicability"]["capability_id"], "evidence");
    assert_eq!(
        selections[1]["applicability"]["capability_id"],
        "project_modeling"
    );
    assert_eq!(snapshot["semantic_execution"], "not_run");
    assert_eq!(snapshot["fact_admitted"], false);
}

#[test]
fn harness_premises_reject_missing_selections_and_invalid_existing_modules() {
    for (modules, advice) in [(json!([]), json!({})), (json!({}), json!([]))] {
        let f = Fixture::new();
        let input = f.0.join("input.json");
        let premises = f.0.join("advice.json");
        let output = f.0.join("step");
        fs::write(&input, serde_json::to_vec(&harness_input(modules)).unwrap()).unwrap();
        fs::write(&premises, serde_json::to_vec(&advice).unwrap()).unwrap();
        let response = command()
            .args(["harness", "step", "--input"])
            .arg(&input)
            .arg("--premises")
            .arg(&premises)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(response.status.code(), Some(2));
        assert!(!output.exists());
    }
}

#[test]
fn duplicate_input_is_rejected_without_echoing_private_content() {
    let f = Fixture::new();
    let input = f.0.join("input.json");
    fs::write(
        &input,
        r#"{"secret":"PRIVATE_TEST_VALUE","secret":"duplicate"}"#,
    )
    .unwrap();
    let output = command()
        .args(["harness", "step", "--output"])
        .arg(f.0.join("rejected-step"))
        .arg("--input")
        .arg(input)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_TEST_VALUE"));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["status"],
        json!("input_or_execution_error")
    );
}

#[test]
fn harness_without_credentials_records_the_problem_without_false_acceptance() {
    let f = Fixture::new();
    let output = command()
        .args(["harness", "step", "--input"])
        .arg(write_checkpoint_input(&f))
        .arg("--output")
        .arg(f.0.join("step"))
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "recorded");
    assert_eq!(summary["model_execution"], "not_run");
    assert_eq!(summary["model_calls"], 0);
    assert_eq!(summary["semantic_feedback_status"], "not_run");
    let saved: Value =
        serde_json::from_slice(&fs::read(f.0.join("step/iteration.json")).unwrap()).unwrap();
    assert_eq!(saved["premises"]["status"], "not_provided");
    assert_eq!(saved["engineering_acceptance"], "not_determined");
    assert_eq!(saved["ontology_promotion"], "not_performed");
    assert!(!saved["state"]["unknowns"].as_array().unwrap().is_empty());
}

#[test]
fn empty_review_is_not_run_and_does_not_open_a_semantic_runtime() {
    let f = Fixture::new();
    let input = f.0.join("input.json");
    fs::write(
        &input,
        br#"{"project_id":"synthetic","review_requests":[]}"#,
    )
    .unwrap();
    let output = command()
        .args(["semantics", "review", "--input"])
        .arg(input)
        .arg("--output")
        .arg(f.0.join("reviews"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "not_run");
    assert_eq!(result["fact_admitted"], false);
    assert!(!f.0.join("reviews").exists());
}

#[test]
fn inverse_checkpoint_with_no_candidate_retains_no_delta_without_promotion() {
    let f = Fixture::new();
    let first = command()
        .args(["harness", "step", "--input"])
        .arg(write_checkpoint_input(&f))
        .arg("--output")
        .arg(f.0.join("step"))
        .output()
        .unwrap();
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    let path = f.0.join("step/iteration.json");
    let checkpoint = json!({
        "schema": "oev2.consolidation-input/3",
        "checkpoint_id": "synthetic-checkpoint",
        "problem_class": "premise-handoff",
        "reason": "条件尚未确定，保留项目未知，不人为制造新本体。",
        "iterations": [{"path": path, "sha256": oe_core::sha256(fs::read(&path).unwrap())}],
        "candidates": [],
        "archive_policy":{"schema":"oev2.consolidation-archive-policy/1","mode":"verified_empty_checkpoint","byte_budget":16777216}
    });
    let input = f.0.join("checkpoint.json");
    fs::write(&input, serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    let output = command()
        .args(["harness", "consolidate", "--input"])
        .arg(input)
        .arg("--output")
        .arg(f.0.join("checkpoint"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["status"], "no_delta");
    assert_eq!(result["formal_candidate"], "not_created");
    assert_eq!(result["ontology_promotion"], "not_performed");
    let response: Value =
        serde_json::from_slice(&fs::read(f.0.join("checkpoint/response.json")).unwrap()).unwrap();
    assert_eq!(response["execution_kind"], "not_run");
}

#[cfg(unix)]
#[test]
fn fifo_input_is_rejected_without_waiting_for_a_writer() {
    use std::os::unix::ffi::OsStrExt;
    use std::time::{Duration, Instant};
    let f = Fixture::new();
    let path = f.0.join("pipe");
    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let mut child = command()
        .args(["harness", "step", "--output"])
        .arg(f.0.join("rejected-step"))
        .arg("--input")
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(2) {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("FIFO input blocked");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(2));
}

fn native_review_cli_fixture(record_name: &str) -> (Fixture, std::process::Output) {
    use oe_core::json::{canonical_json, strict_json};
    use oe_core::sha256;
    let fixture = Fixture::new();
    let isolated = &fixture.0;
    fs::write(isolated.join("SKILL.md"), "# Native Rust review fixture\n").unwrap();
    fs::create_dir_all(isolated.join("var/project/evidence")).unwrap();
    fs::create_dir(isolated.join("var/project/registry")).unwrap();
    let original = root();
    let lock =
        strict_json(&fs::read(original.join("runtime/semantic-bundles.json")).unwrap()).unwrap();
    for relative in [
        "runtime/semantic-bundles.json",
        "runtime/semantica-source-lock.json",
        lock["bundles"]["engineering-judgment-intake"]["path"]
            .as_str()
            .unwrap(),
    ] {
        let target = isolated.join(relative);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(original.join(relative), target).unwrap();
    }
    for name in [record_name, "review-sources.json"] {
        fs::copy(
            original.join("examples/judgment_intake").join(name),
            isolated.join("var/project/evidence").join(name),
        )
        .unwrap();
    }
    let spec = &lock["bundles"]["engineering-judgment-intake"];
    let binding = json!({"$schema":"ontology-engineering.semantic-project-binding/v1","binding_id":"synthetic-binding",
            "project":{"project_id":"synthetic-project","domain":"discrete-manufacturing"},
            "semantic_target":{"kind":"workspace","package_id":spec["package_id"],"workspace_id":"synthetic-registry"},
            "baseline":{"version":spec["package_version"],"digest":spec["package_sha256"]},
            "evidence":{"logical_root":"evidence:synthetic-review"},
            "authority":{"fact":{"authority_id":"synthetic-engineer","scope":["test-evidence"]},"decision":{"authority_id":"synthetic-reviewer","scope":["test-review"]}},
            "allowed_actions":["review","propose"],"lifecycle_actions":["candidate"],
            "promotion":{"target":"industry-registry","requires_decision_authority":true},"created_at":"2026-09-27T00:00:00Z",
            "semantic_api":"semantica.ontology.refinery/v1"});
    fs::write(
        isolated.join("var/project/binding.json"),
        canonical_json(&binding).unwrap(),
    )
    .unwrap();
    let record_relative = format!("var/project/evidence/{record_name}");
    let request = json!({"id":"identity-check","record":record_relative,
            "record_sha256":sha256(fs::read(isolated.join(&record_relative)).unwrap()),
            "binding":"var/project/binding.json","binding_sha256":sha256(fs::read(isolated.join("var/project/binding.json")).unwrap()),
            "evidence_root":"var/project/evidence","workspace":"var/project/registry","actor":"synthetic-operator"});
    let input = isolated.join("var/input.json");
    fs::write(
        &input,
        canonical_json(&json!({"project_id":"synthetic-project", "review_requests":[request]}))
            .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_oev2"))
        .env_clear()
        .env("PATH", "")
        .arg("--root")
        .arg(isolated)
        .args(["semantics", "review", "--input"])
        .arg(input)
        .args(["--output", "var/project/reviews"])
        .output()
        .unwrap();
    (fixture, output)
}

#[test]
fn blocked_review_retains_structured_failure_and_exits_nonzero() {
    use oe_core::json::strict_json;
    use oe_core::sha256;
    // This unchanged negative engineering fixture checks stale evidence. A
    // missing Python environment no longer causes native Rust execution to fail.
    let (fixture, output) = native_review_cli_fixture("review-old-report-identity_stale.json");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report = strict_json(&output.stdout).unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["dependent_adoption"], "blocked");
    assert_eq!(report["failed_reviews"], json!([]));
    assert_eq!(report["reviews"].as_array().unwrap().len(), 1);
    let review = &report["reviews"][0];
    assert_eq!(review["id"], "identity-check");
    assert_eq!(review["fact_admitted"], false);
    assert_eq!(review["review"]["status"], "blocked");
    assert!(!review["review"]["findings"].as_array().unwrap().is_empty());
    assert_eq!(
        strict_json(&fs::read(fixture.0.join("var/project/reviews/feedback.json")).unwrap())
            .unwrap(),
        report
    );
    let native_ref = &review["artifacts"]["native-execution.json"];
    let raw = fs::read(fixture.0.join(native_ref["path"].as_str().unwrap())).unwrap();
    assert_eq!(native_ref["sha256"], sha256(&raw));
    let receipt = strict_json(&raw).unwrap();
    assert_eq!(receipt["execution"], "completed");
    assert_eq!(receipt["conclusion"], "violated");
}

#[test]
fn valid_native_review_runs_without_interpreter_or_transport_on_path() {
    use oe_core::json::strict_json;
    let (fixture, output) =
        native_review_cli_fixture("review-current-support-identity_current.json");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let report = strict_json(&output.stdout).unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["failed_reviews"], json!([]));
    assert_eq!(report["reviews"].as_array().unwrap().len(), 1);
    assert_eq!(report["reviews"][0]["review"]["status"], "clear");
    assert_eq!(report["reviews"][0]["fact_admitted"], false);
    assert!(!fixture.0.join("runtime/.venv").exists());
}

#[test]
fn current_cli_archive_keeps_handoff_and_rejects_nonempty_candidates_without_promotion() {
    let f = Fixture::new();
    let first = command()
        .args(["harness", "step", "--input"])
        .arg(write_checkpoint_input(&f))
        .arg("--output")
        .arg(f.0.join("current-step"))
        .output()
        .unwrap();
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    let path = f.0.join("current-step/iteration.json");
    let original = fs::read(&path).unwrap();
    let mut checkpoint = json!({"schema":"oev2.consolidation-input/3",
        "checkpoint_id":"current-synthetic-checkpoint", "problem_class":"premise-handoff",
        "reason":"Retain necessary conditions and unknowns under current Rust checks.",
        "iterations":[{"path":path,"sha256":oe_core::sha256(&original)}],"candidates":[],
        "archive_policy":{"schema":"oev2.consolidation-archive-policy/1","mode":"verified_empty_checkpoint","byte_budget":16777216}});
    for (name, code) in [("empty", 0), ("nonempty", 2), ("over-budget", 2)] {
        if name == "nonempty" {
            checkpoint["candidates"] = json!([{"id":"gap-current","proposal":"Requires explicit Agent review and current evidence",
                "applicability":["Conditions unknown"],"counterexamples":["No field verification"],"open_questions":["Reusable?"]}]);
        } else if name == "over-budget" {
            checkpoint["candidates"] = json!([]);
            checkpoint["archive_policy"]["byte_budget"] = json!(1);
        }
        let input = f.0.join(format!("current-{name}-input.json"));
        fs::write(&input, serde_json::to_vec(&checkpoint).unwrap()).unwrap();
        let output_dir = f.0.join(format!("current-{name}"));
        let output = command()
            .args(["harness", "consolidate", "--input"])
            .arg(&input)
            .arg("--output")
            .arg(&output_dir)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code), "{name}: {output:?}");
        assert!(output.stderr.is_empty());
        assert_eq!(fs::read(&path).unwrap(), original);
        if code != 0 {
            assert!(!output_dir.join("consolidation.json").exists());
            continue;
        }
        let summary: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            summary["handoff"]["native_proposal_entrypoint"],
            "oev2 semantics propose"
        );
        assert_eq!(
            summary["handoff"]["current_review_entrypoint"],
            "oev2 kernel review"
        );
        assert_eq!(summary["handoff"]["next_action"], "retain_no_delta");
        assert_eq!(
            summary["handoff"]["native_proposal_migration"],
            "current_registry_not_verified"
        );
        assert_eq!(summary["formal_candidate"], "not_created");
        assert_eq!(summary["ontology_promotion"], "not_performed");
        let saved: Value =
            serde_json::from_slice(&fs::read(output_dir.join("consolidation.json")).unwrap())
                .unwrap();
        assert_eq!(saved["schema"], "oev2.consolidation/2");
        assert_eq!(saved["handoff"], summary["handoff"]);
        assert_eq!(saved["authority"], "author_declaration_only");
        assert_eq!(saved["model_calls"], 0);
        assert_eq!(saved["request_bytes"], 0);
        assert_eq!(
            saved["historical_checks"],
            "historical_checks_not_reexecuted"
        );
        assert_eq!(
            saved["historical_handoff"]["current_review_entrypoint"],
            "oev2 kernel review"
        );
    }
}
