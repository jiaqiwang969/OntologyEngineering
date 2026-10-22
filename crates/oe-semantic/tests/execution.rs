use oe_core::{json::canonical_json, sha256};
use oe_semantic::{
    contracts,
    lifecycle::*,
    native::{NativeExecution, RegistryRunRequest},
    runtime::{RuntimeSourceLock, RuntimeVerification},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
struct Fixture {
    directory: PathBuf,
    binding: PathBuf,
    task: PathBuf,
}
impl Fixture {
    fn new(workspace: bool) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = root().join(format!(
            "var/maintenance/rust-rewrite-20260927/phase4-execution/fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&directory).unwrap();
        let binding = directory.join("binding.json");
        let task = directory.join("task.json");
        let mut value = json!({"$schema":contracts::BINDING_SCHEMA,"binding_id":"binding:execution",
            "project":{"project_id":"project:execution","domain":"test"},
            "semantic_target":{"kind":"package","package_id":"fixture-package","package_version":"1.0"},
            "baseline":{"version":"1.0","digest":"a".repeat(64)},"evidence":{"logical_root":"evidence:execution"},
            "authority":{"fact":{"authority_id":"fact:fixture","scope":["synthetic"]},"decision":{"authority_id":"decision:fixture","scope":["synthetic"]}},
            "allowed_actions":["doctor","discover","run","verify"],"lifecycle_actions":[],
            "promotion":{"target":"industry-registry","requires_decision_authority":true},
            "created_at":"2026-09-27T10:00:00Z","semantic_api":contracts::NATIVE_REFINERY_CONTRACT});
        if workspace {
            value["semantic_target"] = json!({"kind":"workspace","package_id":"fixture-package","workspace_id":"workspace:fixture"});
            value["allowed_actions"] = json!(contracts::KNOWN_ACTIONS);
            value["lifecycle_actions"] = json!(contracts::STATE_MODEL);
        }
        fs::write(&binding, canonical_json(&value).unwrap()).unwrap();
        fs::write(&task, canonical_json(&json!({"$schema":contracts::TASK_SCHEMA,"task_id":"task:execution",
            "task_kind":"fixture","intent":"Run a synthetic package","project":"project:execution","domain":"test",
            "requested_decision":"Inspect the native result","actor_id":"actor:fixture","requested_actions":["run"],
            "required_capabilities":["semantic.package.load"],"created_at":"2026-09-27T10:00:00Z",
            "evidence":[{"source_id":"source:fixture","uri":"evidence:execution/fixture","sha256":"b".repeat(64),"media_type":"application/json","captured_at":"2026-09-27T10:00:00Z"}]})).unwrap()).unwrap();
        Self {
            directory,
            binding,
            task,
        }
    }
    fn request(&self) -> ExecutionRequest<'_> {
        ExecutionRequest {
            binding: &self.binding,
            task: None,
            workspace: None,
            scenario_id: Some("scenario:fixture"),
            recorded_at: None,
        }
    }
    fn edit_binding(&self, edit: impl FnOnce(&mut Value)) {
        let mut value: Value = serde_json::from_slice(&fs::read(&self.binding).unwrap()).unwrap();
        edit(&mut value);
        fs::write(&self.binding, canonical_json(&value).unwrap()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn native_capabilities() -> Value {
    let operations = [
        "build_refinery_acceptance_delta",
        "open_engagement",
        "propose_candidate",
        "commit_candidate",
        "execute_candidate",
        "derive_gate_evidence",
        "verify_candidate",
        "promote_candidate",
        "history",
        "resolve_package",
        "execution_manifest",
        "run_registry",
    ];
    let symbols = [
        "CandidateVerificationDTO",
        "EngagementPhaseDTO",
        "ExecutionReceiptReferenceDTO",
        "IndustryOntologyRegistry",
        "IndustryPackageDescriptorDTO",
        "LearningResultDTO",
        "PackageDelta",
        "ProjectOntologyBinding",
        "RefineryAuthorizationDTO",
        "RefineryGateEvidenceDTO",
        "RefineryStateDTO",
        "RuntimeSourceIdentityDTO",
        "SemanticEngagementReceipt",
        "SemanticTaskEnvelope",
        "SourceEvidenceDTO",
        "SubjectExecutionSuiteDTO",
        "TransitionContextDTO",
    ];
    let declaration = json!({"contract":contracts::NATIVE_REFINERY_CONTRACT,"schema_version":"1.0","states":contracts::STATE_MODEL,
        "asset_categories":["ontology","competency_questions","shapes","queries","rules","cases","contract","provenance"],
        "delta_categories":["ontology","competency_questions","shapes","queries","rules","cases","contract","provenance","book_impact"],
        "book_impacts":["none","vol1-method","vol2-iso-exemplar","both"],"case_kinds":["positive","negative","ambiguity","prior_release"],
        "regression_check_ids":["cq.prior","cq.current","case.positive","case.negative","case.ambiguity","case.prior_release"],
        "release_check_ids":["package.coverage","capability.coverage","receipt.binding","provenance.binding","source.rights","io.binding"],
        "transition_context_actions":["candidate","proposed","committed","execute_candidate","derive_regression_gate","regression_passed","derive_release_gate","release_complete","promoted"],
        "transition_context_required_operations":["propose_candidate","commit_candidate","execute_candidate","derive_gate_evidence","verify_candidate","promote_candidate"],
        "runner_contract":"semantica.chapter_packages.SemanticPackageRunner/v1","operations":operations,"publication_owned_externally":true});
    let mut result = json!({"native":declaration,"operation_presence":{},"symbol_presence":{},
        "constants":{"REFINERY_CONTRACT":contracts::NATIVE_REFINERY_CONTRACT,"REFINERY_SCHEMA_VERSION":"1.0",
        "EMPTY_PACKAGE_SHA256":oe_semantic::delta::empty_package_sha256().unwrap()},
        "decision_review_contract":"semantica.ontology.decision-review/v1","decision_review_available":true});
    for key in operations {
        result["operation_presence"][key] = true.into();
    }
    for key in symbols {
        result["symbol_presence"][key] = true.into();
    }
    result
}
fn proof() -> RuntimeVerification {
    RuntimeVerification {
        source_lock: RuntimeSourceLock {
            commit: "c".repeat(40),
            version: "fixture.1".into(),
            artifact_filename: "fixture.whl".into(),
            artifact_sha256: "d".repeat(64),
        },
        record_files: 1,
        observation_kind: "controlled fixture".into(),
        semantic_execution: "not_run".into(),
        staging_descriptor_sha256: None,
    }
}
fn completed(operation: &str, value: Value) -> NativeExecution {
    NativeExecution {
        operation: operation.into(),
        semantic_execution: "completed".into(),
        runtime_identity: Some(proof()),
        result: Some(value),
        error: None,
        elapsed_seconds: 0.1,
        stderr: "fixture native diagnostic".into(),
    }
}
fn receipt(executor_digest: &str) -> Value {
    let mut value = json!({"package_id":"fixture-package","package_version":"1.0","package_digest":executor_digest,
        "chapter_contract_sha256":"e".repeat(64),"runtime_commit":"c".repeat(40),"runtime_version":"fixture.1","runtime_artifact_sha256":"d".repeat(64)});
    value["receipt_sha256"] = sha256(canonical_json(&value).unwrap()).into();
    value
}
fn run_result(workspace: bool, release: &str) -> Value {
    let digest = if workspace {
        "f".repeat(64)
    } else {
        "a".repeat(64)
    };
    let receipt = receipt(&digest);
    let execution = json!({"package_id":"fixture-package","package_version":"1.0","package_digest":digest,
        "scenario_id":"scenario:fixture","status":"passed","receipt":receipt,"operations":[{"operation":"fixture","status":"passed"}],
        "oracle_checks":[{"check_id":"fixture.oracle","status":"passed"}],"reasons":[],"cq_report":{"status":"passed","original":"retained"},
        "capability_report":{"payload":{"profile":{"capabilities":["semantic.package.load"]}}}});
    let verification = json!({"status":release,"receipt_sha256":receipt["receipt_sha256"],"checks":[{"check_id":"release.fixture","passed":release=="complete"}]});
    if workspace {
        json!({"registry_id":"workspace:fixture","subject":{"package_id":"fixture-package","version":"1.0","package_sha256":"a".repeat(64),"execution_projection_sha256":"e".repeat(64)},"executor":execution,"release":verification,"receipt_integrity":true})
    } else {
        json!({"run":execution,"verification":verification,"receipt_integrity":true})
    }
}
struct Backend {
    capabilities: Value,
    diagnostics: Value,
    packages: Value,
    result: Value,
    calls: RefCell<Vec<String>>,
    times: RefCell<Vec<Option<String>>>,
    mutate: Option<(String, PathBuf)>,
    failure: bool,
    alternate_runtime_at: Option<String>,
}
impl Backend {
    fn new(workspace: bool, release: &str) -> Self {
        Self {
            capabilities: native_capabilities(),
            diagnostics: json!({"chapter_issues":[],"domain_issues":[],"chapters":[{"package_id":"fixture-package"}],"domains":[]}),
            packages: json!({"chapters":[{"descriptor":{"package_id":"fixture-package","version":"1.0"},"package_digest":"a".repeat(64),"required_capabilities":[],"available_capabilities":["semantic.package.load"]}],"domains":[]}),
            result: run_result(workspace, release),
            calls: RefCell::new(vec![]),
            times: RefCell::new(vec![]),
            mutate: None,
            failure: false,
            alternate_runtime_at: None,
        }
    }
    fn answer(&self, operation: &str, value: Value) -> NativeExecution {
        self.calls.borrow_mut().push(operation.into());
        if let Some((when, path)) = &self.mutate
            && when == operation
        {
            let mut data = fs::read(path).unwrap();
            data.push(b' ');
            fs::write(path, data).unwrap();
        }
        let mut result = completed(operation, value);
        if self.alternate_runtime_at.as_deref() == Some(operation) {
            let identity = result.runtime_identity.as_mut().unwrap();
            identity.source_lock.commit = "9".repeat(40);
            identity.source_lock.version = "fixture.2".into();
            identity.source_lock.artifact_filename = "fixture.2.whl".into();
            identity.source_lock.artifact_sha256 = "8".repeat(64);
        }
        if self.failure && operation == "run" {
            result.semantic_execution = "failed".into();
            result.error = Some("native output limit".into());
        }
        result
    }
}
impl ExecutionBackend for Backend {
    fn capabilities(&self) -> NativeExecution {
        self.answer("capabilities", self.capabilities.clone())
    }
    fn registry_diagnostics(&self) -> NativeExecution {
        self.answer("diagnostics", self.diagnostics.clone())
    }
    fn discover_packages(&self) -> NativeExecution {
        self.answer("discover", self.packages.clone())
    }
    fn run_package(&self, _: &str, _: Option<&str>, time: Option<&str>) -> NativeExecution {
        self.times.borrow_mut().push(time.map(str::to_owned));
        self.answer("run", self.result.clone())
    }
    fn run_registry(&self, request: &RegistryRunRequest) -> NativeExecution {
        assert_eq!(
            request.registry.binding["workspace_id"],
            "workspace:fixture"
        );
        self.times.borrow_mut().push(request.created_at.clone());
        self.answer("run", self.result.clone())
    }
}

#[test]
fn scenario_pass_and_receipt_verified_do_not_make_partial_release_pass() {
    let f = Fixture::new(false);
    let b = Backend::new(false, "blocked");
    let v = run_with(&root(), &f.request(), &b).unwrap();
    assert_eq!(v["execution"]["status"], "passed");
    assert_eq!(v["receipt"]["status"], "verified");
    assert_eq!(v["release"]["status"], "blocked");
    assert_eq!(v["command_verdict"], "blocked");
    assert_eq!(v["learning"]["verdict"], "no_delta");
    assert_eq!(v["native_execution"]["result"], b.result);
    assert_eq!(v["native_executions"].as_array().unwrap().len(), 3);
}
#[test]
fn package_verify_runs_current_native_runner_without_task() {
    let f = Fixture::new(false);
    let b = Backend::new(false, "complete");
    let v = verify_package_with(&root(), &f.request(), &b).unwrap();
    assert_eq!(v["command"], "verify");
    assert_eq!(v["command_verdict"], "passed");
    assert_eq!(*b.calls.borrow(), ["capabilities", "discover", "run"]);
}
#[test]
fn run_task_cannot_authorize_verify() {
    let f = Fixture::new(false);
    let b = Backend::new(false, "complete");
    let mut request = f.request();
    request.task = Some(&f.task);
    let e = verify_package_with(&root(), &request, &b).unwrap_err();
    assert_eq!(
        error_response("verify", &e)["execution"]["error_type"],
        "ActionNotAllowedError"
    );
    assert!(b.calls.borrow().is_empty());
    let v = run_with(&root(), &request, &b).unwrap();
    assert_eq!(
        v["execution"]["task_sha256"],
        sha256(fs::read(&f.task).unwrap())
    );
}
#[test]
fn binding_authority_and_exact_preflight_digest_are_enforced() {
    let f = Fixture::new(false);
    let b = Backend::new(false, "complete");
    f.edit_binding(|v| v["allowed_actions"] = json!(["doctor"]));
    assert!(run_with(&root(), &f.request(), &b).is_err());
    assert!(b.calls.borrow().is_empty());
    f.edit_binding(|v| {
        v["allowed_actions"] = json!(["run"]);
        v["baseline"]["digest"] = json!("b".repeat(64));
    });
    let e = run_with(&root(), &f.request(), &b).unwrap_err();
    let v = error_response("run", &e);
    assert_eq!(*b.calls.borrow(), ["capabilities", "discover"]);
    assert_eq!(v["native_execution"]["operation"], "discover");
}
#[test]
fn execution_identity_change_keeps_native_completed_evidence() {
    let f = Fixture::new(false);
    let mut b = Backend::new(false, "complete");
    b.result["run"]["package_digest"] = json!("b".repeat(64));
    let e = run_with(&root(), &f.request(), &b).unwrap_err();
    let v = error_response("run", &e);
    assert_eq!(v["native_execution"]["semantic_execution"], "completed");
    assert_eq!(v["native_execution"]["result"], b.result);
    assert_eq!(v["command_verdict"], "blocked");
}
#[test]
fn source_changes_before_or_after_runner_fail_closed_with_trace() {
    for when in ["capabilities", "run"] {
        let f = Fixture::new(false);
        let mut b = Backend::new(false, "complete");
        b.mutate = Some((when.into(), f.binding.clone()));
        let e = run_with(&root(), &f.request(), &b).unwrap_err();
        let v = error_response("run", &e);
        assert!(e.to_string().contains("source-locked"));
        assert_eq!(v["native_execution"]["operation"], when);
        assert_eq!(v["native_execution"]["semantic_execution"], "completed");
        assert_eq!(v["command_verdict"], "blocked");
    }
}
#[test]
fn unknown_and_ambiguous_scenarios_keep_native_blocked_oracle() {
    for reason in ["missing_scenario", "ambiguous_scenario"] {
        let f = Fixture::new(false);
        let mut b = Backend::new(false, "blocked");
        b.result["run"]["status"] = "blocked".into();
        b.result["run"]["scenario_id"] = "unresolved".into();
        b.result["run"]["receipt"] = Value::Null;
        b.result["receipt_integrity"] = Value::Null;
        b.result["run"]["oracle_checks"] =
            json!([{"check_id":"oracle.contract","status":"failed","message":reason}]);
        b.result["run"]["reasons"] = json!([reason]);
        b.result["verification"]["receipt_sha256"] = json!("0".repeat(64));
        let mut request = f.request();
        if reason == "ambiguous_scenario" {
            request.scenario_id = None;
        }
        let v = run_with(&root(), &request, &b).unwrap();
        assert_eq!(v["command_verdict"], "blocked");
        assert_eq!(v["execution"]["scenario_id"], "unresolved");
        assert_eq!(
            v["execution"]["oracle_checks"],
            b.result["run"]["oracle_checks"]
        );
        assert_eq!(v["receipt"]["status"], "missing");
        assert_eq!(v["native_execution"]["semantic_execution"], "completed");
    }
}
#[test]
fn invalid_receipt_never_becomes_verified_from_success_status() {
    let f = Fixture::new(false);
    let mut b = Backend::new(false, "complete");
    b.result["run"]["receipt"]["runtime_commit"] = json!("0".repeat(40));
    let v = run_with(&root(), &f.request(), &b).unwrap();
    assert_eq!(v["receipt"]["status"], "invalid");
    assert_eq!(v["command_verdict"], "blocked");
    assert_eq!(v["execution"]["status"], "passed");
}
#[test]
fn native_failure_preserves_partial_result_and_prior_calls() {
    let f = Fixture::new(false);
    let mut b = Backend::new(false, "complete");
    b.failure = true;
    let e = run_with(&root(), &f.request(), &b).unwrap_err();
    let v = error_response("run", &e);
    assert_eq!(v["native_execution"]["result"], b.result);
    assert_eq!(v["native_execution"]["semantic_execution"], "failed");
    assert_eq!(v["native_executions"].as_array().unwrap().len(), 3);
}

#[test]
fn valid_runtime_upgrade_between_calls_cannot_replace_the_command_baseline() {
    for operation in ["discover", "run"] {
        let f = Fixture::new(false);
        let mut b = Backend::new(false, "complete");
        b.alternate_runtime_at = Some(operation.into());
        let response = error_response("run", &run_with(&root(), &f.request(), &b).unwrap_err());
        assert_eq!(response["command_verdict"], "blocked");
        assert_eq!(response["runtime_source"]["commit"], "c".repeat(40));
        assert_eq!(response["runtime_source"]["version"], "fixture.1");
        assert_eq!(
            response["native_execution"]["runtime_identity"]["source_lock"]["commit"],
            "9".repeat(40)
        );
        assert_eq!(
            response["native_execution"]["semantic_execution"],
            "completed"
        );
        assert_eq!(response["native_execution"]["operation"], operation);
        assert_eq!(
            response["native_executions"][0]["runtime_identity"]["source_lock"]["version"],
            "fixture.1"
        );
        assert_eq!(
            response["native_executions"].as_array().unwrap().len(),
            if operation == "run" { 3 } else { 2 }
        );
        if operation == "run" {
            assert_eq!(response["native_execution"]["result"], b.result);
        }
    }
    let b = Backend {
        alternate_runtime_at: Some("diagnostics".into()),
        ..Backend::new(false, "complete")
    };
    let response = error_response("doctor", &doctor_with(&root(), None, &b).unwrap_err());
    assert_eq!(response["runtime_source"]["version"], "fixture.1");
    assert_eq!(
        response["native_execution"]["runtime_identity"]["source_lock"]["version"],
        "fixture.2"
    );
    assert_eq!(response["command_verdict"], "blocked");
}
#[test]
fn doctor_reports_capability_and_registry_diagnostics_independently() {
    let f = Fixture::new(false);
    let mut b = Backend::new(false, "complete");
    b.capabilities["operation_presence"]["run_registry"] = false.into();
    b.diagnostics["domain_issues"] = json!(["original native issue"]);
    let v = doctor_with(&root(), Some(&f.binding), &b).unwrap();
    assert_eq!(v["command_verdict"], "blocked");
    assert_eq!(v["execution"]["status"], "not_run");
    assert_eq!(
        v["corpus_found"]["registry_issues"],
        json!(["original native issue"])
    );
    assert_eq!(*b.calls.borrow(), ["capabilities", "diagnostics"]);
    assert_eq!(v["registry_diagnostics"], b.diagnostics);
    let good = Backend::new(false, "complete");
    assert_eq!(
        doctor_with(&root(), None, &good).unwrap()["command_verdict"],
        "passed"
    );
}
#[test]
fn doctor_requires_its_own_binding_permission() {
    let f = Fixture::new(false);
    f.edit_binding(|v| v["allowed_actions"] = json!(["run"]));
    let b = Backend::new(false, "complete");
    assert!(doctor_with(&root(), Some(&f.binding), &b).is_err());
    assert!(b.calls.borrow().is_empty());
}

#[test]
fn missing_unrelated_refinery_operation_blocks_workspace_but_not_builtin_runner() {
    for workspace in [false, true] {
        let f = Fixture::new(workspace);
        let mut b = Backend::new(workspace, "complete");
        b.capabilities["operation_presence"]["promote_candidate"] = false.into();
        let workspace_path = f.directory.join("workspace");
        let mut request = f.request();
        if workspace {
            request.workspace = Some(&workspace_path);
        }
        let result = run_with(&root(), &request, &b);
        if workspace {
            let response = error_response("run", &result.unwrap_err());
            assert_eq!(response["command_verdict"], "blocked");
            assert_eq!(*b.calls.borrow(), ["capabilities"]);
        } else {
            let response = result.unwrap();
            assert_eq!(response["command_verdict"], "passed");
            assert_eq!(response["capabilities"]["native"]["available"], false);
            assert_eq!(response["native_execution"]["result"], b.result);
        }
    }
}

#[test]
fn task_required_capability_is_checked_before_and_against_actual_runner() {
    for post_run in [false, true] {
        let f = Fixture::new(false);
        let mut b = Backend::new(false, "complete");
        if post_run {
            b.result["run"]["capability_report"]["payload"]["profile"]["capabilities"] = json!([]);
        } else {
            b.packages["chapters"][0]["available_capabilities"] = json!([]);
        }
        let mut request = f.request();
        request.task = Some(&f.task);
        let response = error_response("run", &run_with(&root(), &request, &b).unwrap_err());
        assert_eq!(response["command_verdict"], "blocked");
        assert_eq!(
            response["native_execution"]["operation"],
            if post_run { "run" } else { "discover" }
        );
        assert_eq!(
            response["native_execution"]["semantic_execution"],
            "completed"
        );
    }
}

#[test]
fn changing_task_bytes_during_run_cannot_rebind_the_retained_execution() {
    let f = Fixture::new(false);
    let initial = sha256(fs::read(&f.task).unwrap());
    let mut b = Backend::new(false, "complete");
    b.mutate = Some(("run".into(), f.task.clone()));
    let mut request = f.request();
    request.task = Some(&f.task);
    let response = error_response("run", &run_with(&root(), &request, &b).unwrap_err());
    assert_eq!(response["task"]["source_sha256"], initial);
    assert_ne!(initial, sha256(fs::read(&f.task).unwrap()));
    assert_eq!(response["native_execution"]["result"], b.result);
    assert_eq!(response["command_verdict"], "blocked");
}

#[test]
fn doctor_rechecks_bound_source_after_native_diagnostics() {
    let f = Fixture::new(false);
    let mut b = Backend::new(false, "complete");
    b.mutate = Some(("diagnostics".into(), f.binding.clone()));
    let response = error_response(
        "doctor",
        &doctor_with(&root(), Some(&f.binding), &b).unwrap_err(),
    );
    assert_eq!(response["command_verdict"], "blocked");
    assert_eq!(response["registry_diagnostics"], b.diagnostics);
    assert_eq!(
        response["native_execution"]["semantic_execution"],
        "completed"
    );
}

#[test]
fn workspace_recorded_time_is_forwarded_to_native_run() {
    let f = Fixture::new(true);
    let b = Backend::new(true, "complete");
    let workspace = f.directory.join("workspace");
    let mut request = f.request();
    request.workspace = Some(&workspace);
    request.recorded_at = Some("2026-09-27T17:22:00Z");
    assert_eq!(
        run_with(&root(), &request, &b).unwrap()["command_verdict"],
        "passed"
    );
    assert_eq!(*b.times.borrow(), [Some("2026-09-27T17:22:00Z".into())]);
}
#[test]
fn promoted_subject_and_executor_have_distinct_verified_identities() {
    let f = Fixture::new(true);
    let b = Backend::new(true, "complete");
    let workspace = f.directory.join("workspace-not-created");
    let mut request = f.request();
    request.workspace = Some(&workspace);
    let v = run_with(&root(), &request, &b).unwrap();
    assert_eq!(v["command_verdict"], "passed");
    assert_ne!(
        v["execution"]["subject"]["package_sha256"],
        v["execution"]["executor"]["digest"]
    );
    assert!(!workspace.exists());
    assert_eq!(*b.calls.borrow(), ["capabilities", "run"]);
}
#[test]
fn wrong_actual_registry_or_subject_retains_native_result_but_blocks() {
    for field in ["registry", "subject"] {
        let f = Fixture::new(true);
        let mut b = Backend::new(true, "complete");
        if field == "registry" {
            b.result["registry_id"] = "other".into();
        } else {
            b.result["subject"]["package_sha256"] = json!("b".repeat(64));
        }
        let workspace = f.directory.join("workspace");
        let mut request = f.request();
        request.workspace = Some(&workspace);
        let v = error_response("run", &run_with(&root(), &request, &b).unwrap_err());
        assert_eq!(v["command_verdict"], "blocked");
        assert_eq!(v["native_execution"]["result"], b.result);
    }
}
#[test]
fn workspace_is_required_and_candidate_verify_is_not_implicitly_run() {
    let f = Fixture::new(true);
    let b = Backend::new(true, "complete");
    assert!(run_with(&root(), &f.request(), &b).is_err());
    assert!(verify_package_with(&root(), &f.request(), &b).is_err());
    assert!(b.calls.borrow().is_empty());
}
#[test]
fn recorded_at_is_validated_and_forwarded_without_replacing_task_time() {
    let f = Fixture::new(false);
    let b = Backend::new(false, "complete");
    let mut request = f.request();
    request.task = Some(&f.task);
    request.recorded_at = Some("2026-09-27T11:00:00+01:00");
    let v = run_with(&root(), &request, &b).unwrap();
    assert_eq!(
        *b.times.borrow(),
        [Some("2026-09-27T11:00:00+01:00".into())]
    );
    assert_eq!(v["task"]["created_at"], "2026-09-27T10:00:00Z");
    for time in ["2026-09-27", "2026-09-27T10:00:00", "bad"] {
        let b = Backend::new(false, "complete");
        request.recorded_at = Some(time);
        assert!(run_with(&root(), &request, &b).is_err());
        assert!(b.calls.borrow().is_empty());
    }
}
#[test]
fn lifecycle_at_wrappers_reject_bad_time_before_accessing_inputs() {
    let missing = root().join("var/nonexistent-phase4-input");
    assert!(
        propose_at(
            &root(),
            &missing,
            &missing,
            &missing,
            &missing,
            &missing,
            Some("bad")
        )
        .unwrap_err()
        .to_string()
        .contains("RFC3339")
    );
    assert!(
        transition_at(
            &root(),
            "commit",
            &missing,
            &missing,
            &missing,
            &"a".repeat(64),
            None,
            Some("bad")
        )
        .unwrap_err()
        .to_string()
        .contains("RFC3339")
    );
    let r = ProjectReviewRequest {
        binding: &missing,
        workspace: &missing,
        task: &missing,
        evidence_file: &missing,
        source_id: "x",
        evidence_format: "turtle",
        scope_id: "x",
        focus_iri: "urn:x",
        focus_type_iri: "urn:X",
        query_asset_id: "q",
        shape_asset_id: "s",
    };
    assert!(
        project_review_at(&root(), &r, Some("bad"))
            .unwrap_err()
            .to_string()
            .contains("RFC3339")
    );
}

#[test]
#[ignore = "requires installed source-locked external Semantica; no network or project mutation"]
fn actual_full_doctor_and_partial_chapter_run_preserve_separate_results() {
    let f = Fixture::new(false);
    let diagnosis = doctor(&root(), None).unwrap();
    assert_eq!(diagnosis["command_verdict"], "passed");
    assert_eq!(diagnosis["execution"]["status"], "not_run");
    let discovery = discover(&root(), None, None).unwrap();
    let package = discovery["corpus_found"]["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["package_id"] == "semantica.chapter_packages.vol1.ch03")
        .unwrap();
    f.edit_binding(|v| {
        v["semantic_target"]["package_id"] = package["package_id"].clone();
        v["semantic_target"]["package_version"] = package["version"].clone();
        v["baseline"] = json!({"version":package["version"],"digest":package["package_digest"]});
    });
    let mut request = f.request();
    request.scenario_id = Some("OE-V1-CH03-SCN-CQ-ACCEPTANCE-001");
    let response = run(&root(), &request).unwrap();
    assert_eq!(response["execution"]["status"], "passed");
    assert_eq!(response["receipt"]["status"], "verified");
    assert_eq!(response["release"]["status"], "blocked");
    assert_eq!(response["command_verdict"], "blocked");
    assert_eq!(response["learning"]["verdict"], "no_delta");
}
