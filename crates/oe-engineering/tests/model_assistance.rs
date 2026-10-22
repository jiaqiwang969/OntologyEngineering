//! Author development regressions frozen before production. Injected responses
//! are software fixtures, not live model or engineering adoption evidence.
use oe_contracts::ObjectIdentity;
use oe_core::{SkillRoot, sha256};
use oe_engineering::model_assistance::{
    self,
    transport::{ModelTransport, ModelTransportError, endpoint_identity},
};
use oe_kernel::project::{self, Project};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: SkillRoot,
    input: PathBuf,
    profile: PathBuf,
    project_path: PathBuf,
    original: PathBuf,
    support: PathBuf,
    object: ObjectIdentity,
}
fn save(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn load(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn reference(path: &Path) -> Value {
    json!({"path":path,"sha256":sha256(fs::read(path).unwrap())})
}
impl Fixture {
    fn new() -> Self {
        let installed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let path = installed.join(format!(
            "var/tests/unified-system/model-assistance-author-tests/{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("var")).unwrap();
        fs::write(
            path.join("SKILL.md"),
            "Synthetic bounded model-assistance fixture",
        )
        .unwrap();
        let root = SkillRoot::new(&path).unwrap();
        let input = path.join("var/input.json");
        let profile = path.join("var/profile.json");
        let project_path = path.join("var/project.json");
        let original = path.join("var/original.json");
        let support = path.join("var/support.txt");
        fs::write(
            &support,
            "Source says nominal contact does not establish holding capacity.\n",
        )
        .unwrap();
        let source = sha256(fs::read(&support).unwrap());
        let object = ObjectIdentity {
            object_id: "urn:fixture:support".into(),
            revision: "r1".into(),
            condition_scope: "holding".into(),
            source_sha256: source.clone(),
        };
        let basis = json!({"schema":"oev2.engineering-basis/1","object_id":object.object_id,"object_revision":"r1",
            "purpose":"Review support before manufacture","acceptance":["retain holding capacity unknown"],"facts":[],"assertions":[]});
        let project:Project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"model-fixture","revision":"r1",
            "purpose":"Review support before manufacture","objects":{object.object_id.clone():"r1"},"conditions":{"holding":serde_json::to_string(&basis).unwrap()},
            "evidence":{"fixture-source":source},"decisions":[],"dependencies":{}})).unwrap();
        save(&project_path, &serde_json::to_value(&project).unwrap());
        save(
            &original,
            &json!({"schema":"oev2.advisory-questions/1","state":{"purpose":"Review support before manufacture","unknowns":["holding capacity unmeasured"],
            "source_text":"Nominal contact is not physical holding."},"questions":{"book":{"type":"choice","instructions":"Is a source review needed?",
                "criteria":{"needed":"Needed source review","not_needed":"No source review needed","unknown":"Cannot decide"}}}}),
        );
        let mut original_ref = reference(&original);
        original_ref["role"] = json!("historical_model_request");
        let mut support_ref = reference(&support);
        support_ref["role"] = json!("current_support");
        save(
            &input,
            &json!({"schema":"oev2.model-assistance-input/1","project":reference(&project_path),"object":object,
            "project_context":project::context(&project,&object).unwrap(),"purpose":project.purpose,
            "task":"Identify required holding evidence, without inventing a load.","original_requests":[original_ref],"support_files":[support_ref]}),
        );
        save(
            &profile,
            &json!({"schema":"oev2.model-service-profile/1","service_id":"synthetic-service","api_kind":"openai_responses",
            "endpoint":"http://127.0.0.1:1/v1/responses","requested_model":"fixture-large-context-1","accepted_response_models":["fixture-large-context-1"],
            "context_window_tokens":2000000,"maximum_request_bytes":1000000,"maximum_output_tokens":2048,"maximum_response_bytes":1000000,
            "wrapper_reserve_tokens":4096,"timeout_seconds":30,"credential_file":"var/unused-credential.txt",
            "capacity_evidence":"Synthetic caller declaration; no provider capacity validation"}),
        );
        Self {
            root,
            input,
            profile,
            project_path,
            original,
            support,
            object,
        }
    }
    fn prepare(&self) -> model_assistance::Prepared {
        model_assistance::prepare(&self.root, &self.input, &self.profile).unwrap()
    }
    fn run(&self, name: &str, mock: Option<&dyn ModelTransport>) -> Value {
        model_assistance::run(
            &self.root,
            &self.input,
            &self.profile,
            Path::new(&format!("var/{name}")),
            mock,
        )
        .unwrap()
    }
    fn mock(&self) -> Mock {
        Mock {
            identity: endpoint_identity(load(&self.profile)["endpoint"].as_str().unwrap()).unwrap(),
            response: RefCell::new(Ok(response())),
            requests: RefCell::new(vec![]),
            mutate: None,
        }
    }
    fn rebind_project_sha(&self) {
        let mut v = load(&self.input);
        v["project"] = reference(&self.project_path);
        save(&self.input, &v);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.root.path());
    }
}
struct Mock {
    identity: String,
    response: RefCell<Result<Value, ModelTransportError>>,
    requests: RefCell<Vec<Value>>,
    mutate: Option<(PathBuf, Vec<u8>)>,
}
impl ModelTransport for Mock {
    fn identity(&self) -> String {
        self.identity.clone()
    }
    fn send(&self, request: &Value) -> Result<Value, ModelTransportError> {
        self.requests.borrow_mut().push(request.clone());
        if let Some((path, raw)) = &self.mutate {
            fs::write(path, raw).unwrap();
        }
        self.response.borrow().clone()
    }
}
fn advice() -> Value {
    json!({"schema":"oev2.model-advisory/1","summary":"Retain unknown holding capacity and obtain bounded evidence.",
        "answers":[{"request_index":0,"question_id":"book","selected_option":"needed","explanation":"Source review is needed before adopting the support.",
            "source_refs":["fixture-source"],"unknowns":["unmeasured load"],"verification_required":["verify current original source and measure holding"]}],
        "findings":[{"kind":"missing_condition","claim":"Holding capacity and required load are not established.","source_refs":["fixture-source"],
            "assumptions":[],"unknowns":["load and holding capacity"],"verification_required":["obtain current load and holding test"]}]})
}
fn response() -> Value {
    json!({"id":"fixture-response","model":"fixture-large-context-1","status":"completed","error":null,"incomplete_details":null,
        "output":[{"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":serde_json::to_string(&advice()).unwrap()}]}],
        "usage":{"input_tokens":100,"output_tokens":50,"total_tokens":150}})
}
fn replace_advice(response: &mut Value, advice: &Value) {
    response["output"][0]["content"][0]["text"] = json!(serde_json::to_string(advice).unwrap());
}

fn text_message(text: &str) -> Value {
    json!({"type":"message","role":"assistant","status":"completed",
        "content":[{"type":"output_text","text":text}]})
}

#[test]
fn versioned_parser_accepts_only_completed_json_whitespace_without_resigning_v1() {
    let f = Fixture::new();
    let prepared = f.prepare();
    let mut response = response();
    let mut final_answer = response["output"][0].clone();
    final_answer["phase"] = json!("final_answer");
    response["output"] = json!([
        text_message(" "),
        {"type":"reasoning","summary":[]},
        text_message("\t\r\n"),
        final_answer,
        text_message("")
    ]);
    let old = model_assistance::validate_provider_response(
        &prepared,
        &response,
        model_assistance::RESPONSES_PARSER_V1,
    )
    .err()
    .unwrap();
    assert_eq!(
        old.to_string(),
        "model_assistance_response_output_ambiguous"
    );
    let advisory = model_assistance::validate_provider_response(
        &prepared,
        &response,
        model_assistance::RESPONSES_PARSER_V2,
    )
    .unwrap();
    assert_eq!(serde_json::to_value(advisory).unwrap(), advice());
    assert!(
        model_assistance::validate_provider_response(&prepared, &response, "unregistered-parser")
            .is_err()
    );
    let mock = f.mock();
    *mock.response.borrow_mut() = Ok(response);
    let result = f.run("json-whitespace-only", Some(&mock));
    assert_eq!(result["status"], "completed");
    assert_eq!(
        result["response_parser"],
        model_assistance::RESPONSES_PARSER_V2
    );
    assert_eq!(result["adopted"], false);
    assert_eq!(result["observed_http_attempts"], "unknown");
}

#[test]
fn whitespace_parser_still_rejects_nonempty_extras_refusal_and_incomplete_items() {
    for (case, expected) in [
        (
            "all-whitespace",
            "model_assistance_response_output_ambiguous",
        ),
        ("second-json", "model_assistance_response_output_ambiguous"),
        ("explanation", "model_assistance_response_output_ambiguous"),
        ("unicode-nbsp", "model_assistance_response_output_ambiguous"),
        ("form-feed", "model_assistance_response_output_ambiguous"),
        ("refusal", "model_assistance_response_refused"),
        (
            "incomplete-item",
            "model_assistance_response_output_unsupported",
        ),
        (
            "unsupported-item",
            "model_assistance_response_output_unsupported",
        ),
    ] {
        let f = Fixture::new();
        let prepared = f.prepare();
        let mut response = response();
        let final_answer = response["output"][0].clone();
        let extra = match case {
            "all-whitespace" => text_message("\r \t\n"),
            "second-json" => final_answer.clone(),
            "explanation" => text_message("Here is the answer:"),
            "unicode-nbsp" => text_message("\u{00a0}"),
            "form-feed" => text_message("\u{000c}"),
            "refusal" => json!({"type":"message","role":"assistant","status":"completed",
                "content":[{"type":"refusal","refusal":"Cannot comply"}]}),
            "incomplete-item" => {
                let mut item = text_message(" ");
                item["status"] = json!("incomplete");
                item
            }
            "unsupported-item" => json!({"type":"function_call","name":"unrequested"}),
            _ => unreachable!(),
        };
        response["output"] = if case == "all-whitespace" {
            json!([extra, text_message("")])
        } else {
            json!([extra, final_answer])
        };
        let error = model_assistance::validate_provider_response(
            &prepared,
            &response,
            model_assistance::RESPONSES_PARSER_V2,
        )
        .err()
        .unwrap();
        assert_eq!(error.to_string(), expected, "{case}");
        assert!(model_assistance::provider_validation_diagnostics(&error).is_none());
    }
}

#[test]
fn whitespace_compatibility_preserves_exact_source_rejection_and_reports_each_location() {
    let f = Fixture::new();
    let prepared = f.prepare();
    let mut invalid = advice();
    invalid["answers"][0]["source_refs"] =
        json!(["current_project_context", "current_kernel_checks"]);
    invalid["findings"][0]["source_refs"] = json!(["engineering-purpose", "holding"]);
    let mut response = response();
    replace_advice(&mut response, &invalid);
    let final_answer = response["output"][0].clone();
    response["output"] = json!([text_message(" "), final_answer, text_message(" ")]);
    let error = model_assistance::validate_provider_response(
        &prepared,
        &response,
        model_assistance::RESPONSES_PARSER_V2,
    )
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "model_advisory_answer_invalid");
    let diagnostics = model_assistance::provider_validation_diagnostics(&error).unwrap();
    assert_eq!(
        diagnostics["code"],
        "model_advisory_source_refs_unregistered"
    );
    assert_eq!(diagnostics["unregistered_reference_count"], 4);
    assert_eq!(diagnostics["source_mapping"], "none");
    assert_eq!(
        diagnostics["unregistered_references"][0]["pointer"],
        "/answers/0/source_refs/0"
    );
    assert_eq!(
        diagnostics["unregistered_references"][3]["pointer"],
        "/findings/0/source_refs/1"
    );
    let mock = f.mock();
    *mock.response.borrow_mut() = Ok(response.clone());
    let result = f.run("whitespace-bad-source", Some(&mock));
    assert_eq!(result["status"], "failed");
    assert_eq!(result["provider_validation_diagnostics"], diagnostics);
    assert_eq!(result["advisory"], Value::Null);
    assert_eq!(result["current_support"], "not_established");
    assert_eq!(result["adopted"], false);
    assert_eq!(
        load(
            &f.root
                .path()
                .join("var/whitespace-bad-source/response.parsed.json")
        ),
        response
    );
    assert_eq!(
        load(&f.project_path)["evidence"],
        json!({"fixture-source":f.object.source_sha256})
    );
}

#[test]
fn source_context_candidate_derives_exact_ledger_and_schema_without_changing_historical_questions()
{
    let f = Fixture::new();
    let original = fs::read(&f.original).unwrap();
    let prepared = f.prepare();
    let payload: Value = serde_json::from_str(
        prepared.request["input"][0]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(payload["schema"], model_assistance::CONTEXT_SCHEMA_V2);
    assert_eq!(payload["allowed_source_refs"], json!(["fixture-source"]));
    assert_eq!(
        payload["original_requests"][0]["request"],
        load(&f.original)
    );
    assert_eq!(fs::read(&f.original).unwrap(), original);
    assert!(
        prepared.request["instructions"]
            .as_str()
            .unwrap()
            .starts_with(model_assistance::METHOD_V1)
    );
    assert!(
        prepared.request["instructions"]
            .as_str()
            .unwrap()
            .contains("kernel check names/IDs")
    );
    for field in ["answers", "findings"] {
        assert_eq!(
            prepared.request["text"]["format"]["schema"]["properties"][field]["items"]["properties"]
                ["source_refs"]["items"]["enum"],
            payload["allowed_source_refs"]
        );
    }
    let refs = vec!["fixture-source".to_string(), "another-source".to_string()];
    let schema = model_assistance::response_schema_for_sources(&refs).unwrap();
    for field in ["answers", "findings"] {
        assert_eq!(
            schema["properties"][field]["items"]["properties"]["source_refs"]["items"]["enum"],
            json!(refs)
        );
    }
    assert!(
        model_assistance::response_schema_for_sources(&vec![
            "duplicate".into(),
            "duplicate".into()
        ])
        .is_err()
    );
    let empty = model_assistance::response_schema_for_sources(&[]).unwrap();
    assert_eq!(
        empty["properties"]["findings"]["items"]["properties"]["source_refs"]["maxItems"],
        0
    );
    assert_eq!(
        prepared.report["historical_method_sha256"],
        sha256(model_assistance::METHOD_V1.as_bytes())
    );
    assert_eq!(
        prepared.report["allowed_source_refs"],
        payload["allowed_source_refs"]
    );
}

#[test]
fn complete_original_values_and_kernel_unknowns_survive_valid_advisory_without_adoption() {
    let f = Fixture::new();
    let before = fs::read(&f.project_path).unwrap();
    let raw = fs::read(&f.original).unwrap();
    let prepared = f.prepare();
    assert!(prepared.admitted());
    let payload: Value = serde_json::from_str(
        prepared.request["input"][0]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        payload["original_requests"][0]["request"],
        load(&f.original)
    );
    assert_eq!(payload["current_project"], load(&f.project_path));
    assert_eq!(prepared.request["store"], false);
    assert_eq!(prepared.request["truncation"], "disabled");
    assert_eq!(prepared.request["stream"], false);
    let mock = f.mock();
    let result = f.run("completed", Some(&mock));
    assert_eq!(result["status"], "completed");
    assert_eq!(result["adopted"], false);
    assert_eq!(result["project_update"], "none");
    assert_eq!(result["engineering_adoption"], "not_established");
    assert_eq!(
        result["current_kernel_checks_before"],
        result["current_kernel_checks_after"]
    );
    assert!(
        result["current_kernel_checks_after"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["conclusion"] == "unknown")
    );
    assert_eq!(fs::read(&f.project_path).unwrap(), before);
    assert_eq!(fs::read(&f.original).unwrap(), raw);
    assert_eq!(
        fs::read(
            f.root
                .path()
                .join("var/completed/original-request-0.original")
        )
        .unwrap(),
        raw
    );
    assert_eq!(mock.requests.borrow().len(), 1);
    assert_eq!(result["observed_http_attempts"], "unknown");
    assert_eq!(result["usage"]["cost"], "unknown");
}

#[test]
fn project_raw_drift_and_same_scope_condition_text_change_are_rejected_without_masking_checks() {
    let f = Fixture::new();
    let mut p = load(&f.project_path);
    p["conditions"]["holding"] =
        json!(format!("{} ", p["conditions"]["holding"].as_str().unwrap()));
    save(&f.project_path, &p);
    let mock = f.mock();
    let result = f.run("raw-drift", Some(&mock));
    assert_eq!(result["status"], "blocked");
    assert!(mock.requests.borrow().is_empty());
    assert_eq!(
        result["current_kernel_checks_before"][0]["conclusion"],
        "violated"
    );
    assert!(
        result["current_kernel_checks_before"]
            .as_array()
            .unwrap()
            .len()
            > 1
    );
    f.rebind_project_sha();
    let result = f.run("condition-only", Some(&mock));
    assert_eq!(result["status"], "blocked");
    assert_eq!(
        result["current_kernel_checks_before"][0]["conclusion"],
        "violated"
    );
    assert!(mock.requests.borrow().is_empty());
}

#[test]
fn original_request_and_current_support_digest_drift_block_dispatch() {
    for target in ["original", "support"] {
        let f = Fixture::new();
        let path = if target == "original" {
            &f.original
        } else {
            &f.support
        };
        let mut raw = fs::read(path).unwrap();
        raw.push(b' ');
        fs::write(path, raw).unwrap();
        let mock = f.mock();
        let result = f.run("source-drift", Some(&mock));
        assert_eq!(result["status"], "blocked");
        assert!(mock.requests.borrow().is_empty());
        assert!(
            !result["current_kernel_checks_before"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    let f = Fixture::new();
    let mut project = load(&f.project_path);
    project["evidence"]["fixture-source"] = json!("e".repeat(64));
    save(&f.project_path, &project);
    f.rebind_project_sha();
    let mock = f.mock();
    let result = f.run("source-declaration-drift", Some(&mock));
    assert_eq!(result["status"], "blocked");
    assert!(mock.requests.borrow().is_empty());
}

#[test]
fn direct_profile_accounts_for_complete_question_sources_and_can_block() {
    let f = Fixture::new();
    let mut original = load(&f.original);
    original["state"]["source_text"] = json!("载荷未知，材料未知，计量未知；".repeat(5000));
    save(&f.original, &original);
    let mut input = load(&f.input);
    input["original_requests"][0]["sha256"] = json!(sha256(fs::read(&f.original).unwrap()));
    save(&f.input, &input);
    let prepared = f.prepare();
    assert!(prepared.admitted());
    let mock = f.mock();
    let result = f.run("direct-allowed", Some(&mock));
    assert_eq!(result["status"], "completed");
    let mut profile = load(&f.profile);
    profile["context_window_tokens"] = json!(10000);
    save(&f.profile, &profile);
    let mock = f.mock();
    let result = f.run("direct-blocked", Some(&mock));
    assert_eq!(result["status"], "blocked");
    assert!(mock.requests.borrow().is_empty());
    assert_eq!(result["plan"]["budget"]["official_tokenizer"], false);
    assert_eq!(
        result["plan"]["budget"]["provider_capacity"],
        "caller_declared_unverified"
    );
}

#[test]
fn model_identity_and_transport_identity_mismatches_never_become_advisory() {
    let f = Fixture::new();
    let mut mock = f.mock();
    mock.identity = "unbound-transport".into();
    let result = f.run("bad-transport", Some(&mock));
    assert_eq!(result["status"], "blocked");
    assert!(mock.requests.borrow().is_empty());
    let mock = f.mock();
    mock.response.borrow_mut().as_mut().unwrap()["model"] = json!("different-model");
    let result = f.run("bad-model", Some(&mock));
    assert_eq!(result["status"], "failed");
    assert_eq!(result["reason"], "model_assistance_response_model_mismatch");
    assert_eq!(result["advisory"], Value::Null);
    assert!(
        f.root
            .path()
            .join("var/bad-model/response.parsed.json")
            .is_file()
    );
    assert_eq!(result["adopted"], false);
}

#[test]
fn incomplete_refusal_invalid_type_and_missing_question_are_preserved() {
    for (case, expected) in [
        ("incomplete", "partial"),
        ("refusal", "refused"),
        ("type", "failed"),
        ("missing", "failed"),
        ("extra", "failed"),
        ("source", "failed"),
    ] {
        let f = Fixture::new();
        let mock = f.mock();
        let mut response = response();
        match case {
            "incomplete" => response["status"] = json!("incomplete"),
            "refusal" => {
                response["output"][0]["content"] =
                    json!([{"type":"refusal","refusal":"Cannot comply"}])
            }
            "type" => {
                let mut a = advice();
                a["answers"][0]["request_index"] = json!("0");
                replace_advice(&mut response, &a);
            }
            "missing" => {
                let mut a = advice();
                a["answers"] = json!([]);
                replace_advice(&mut response, &a);
            }
            "extra" => {
                let mut a = advice();
                a["answers"][0]["question_id"] = json!("fabricated");
                replace_advice(&mut response, &a);
            }
            "source" => {
                let mut a = advice();
                a["findings"][0]["source_refs"] = json!(["invented-source"]);
                replace_advice(&mut response, &a);
            }
            _ => unreachable!(),
        }
        *mock.response.borrow_mut() = Ok(response);
        let result = f.run(case, Some(&mock));
        assert_eq!(result["status"], expected, "{case}");
        assert_eq!(result["advisory"], Value::Null);
        assert_eq!(result["adopted"], false);
        assert_eq!(mock.requests.borrow().len(), 1);
        assert!(
            f.root
                .path()
                .join(format!("var/{case}/response.parsed.json"))
                .is_file()
        );
    }
}

#[test]
fn missing_or_invalid_usage_remains_unknown_and_prevents_complete_receipt() {
    for bad in [
        Value::Null,
        json!({"input_tokens":100,"output_tokens":50,"total_tokens":1}),
        json!({"input_tokens":-1,"output_tokens":50,"total_tokens":49}),
    ] {
        let f = Fixture::new();
        let mock = f.mock();
        mock.response.borrow_mut().as_mut().unwrap()["usage"] = bad;
        let result = f.run("usage", Some(&mock));
        assert_eq!(result["status"], "partial");
        assert_eq!(result["usage"]["status"], "unknown");
        assert_eq!(result["usage"]["input_tokens"], Value::Null);
        assert_eq!(result["usage"]["cost"], "unknown");
        assert!(result["advisory"].is_object());
        assert_eq!(result["adopted"], false);
    }
}

#[test]
fn project_condition_and_source_drift_during_dispatch_keep_response_but_remove_current_support() {
    for kind in ["condition", "source"] {
        let f = Fixture::new();
        let mut mock = f.mock();
        let (path, raw) = if kind == "condition" {
            let mut v = load(&f.project_path);
            v["conditions"]["holding"] =
                json!(format!("{} ", v["conditions"]["holding"].as_str().unwrap()));
            (f.project_path.clone(), serde_json::to_vec(&v).unwrap())
        } else {
            (
                f.support.clone(),
                b"source changed after HTTP dispatch".to_vec(),
            )
        };
        mock.mutate = Some((path, raw));
        let result = f.run(kind, Some(&mock));
        assert_eq!(result["status"], "stale");
        assert_eq!(result["source_binding_current"], false);
        assert_eq!(result["current_support"], "not_established");
        assert!(result["advisory"].is_object());
        assert_eq!(result["adopted"], false);
        assert!(
            f.root
                .path()
                .join(format!("var/{kind}/response.parsed.json"))
                .is_file()
        );
        if kind == "condition" {
            assert_eq!(
                result["current_kernel_checks_after"][0]["conclusion"],
                "violated"
            );
        }
    }
}

#[test]
fn transport_failure_has_one_attempt_and_no_retry_or_fallback() {
    let f = Fixture::new();
    let mock = f.mock();
    *mock.response.borrow_mut() = Err(ModelTransportError {
        code: "http_error".into(),
        http_status: Some(400),
        dispatch_attempted: true,
    });
    let result = f.run("failed", Some(&mock));
    assert_eq!(result["status"], "failed");
    assert_eq!(result["transport_error"]["http_status"], 400);
    assert_eq!(mock.requests.borrow().len(), 1);
    assert_eq!(result["automatic_retries"], 0);
    assert_eq!(result["automatic_fallback"], false);
    assert_eq!(result["usage"]["status"], "unknown");
    assert_eq!(result["adopted"], false);
}

#[test]
fn no_transport_and_append_only_archive_cannot_manufacture_model_success() {
    let f = Fixture::new();
    let result = f.run("not-run", None);
    assert_eq!(result["status"], "not_run");
    assert_eq!(result["client_dispatch_attempts"], 0);
    assert_eq!(result["current_support"], "not_established");
    let bytes = fs::read(f.root.path().join("var/not-run/result.json")).unwrap();
    assert!(
        model_assistance::run(
            &f.root,
            &f.input,
            &f.profile,
            Path::new("var/not-run"),
            Some(&f.mock())
        )
        .is_err()
    );
    assert_eq!(
        fs::read(f.root.path().join("var/not-run/result.json")).unwrap(),
        bytes
    );
    let prepared = f.prepare();
    let report =
        model_assistance::write_prepared(&f.root, &prepared, Path::new("var/plan")).unwrap();
    assert_eq!(report["model_execution"], "not_run");
}

#[test]
fn edited_plan_is_not_an_execution_input_and_every_question_keeps_its_request_identity() {
    let f = Fixture::new();
    let prepared = f.prepare();
    let mut plan = prepared.report.clone();
    plan["status"] = json!("prepared");
    plan["adopted"] = json!(true);
    let plan_path = f.root.path().join("var/fake-plan.json");
    save(&plan_path, &plan);
    assert!(model_assistance::prepare(&f.root, &plan_path, &f.profile).is_err());
    let second = f.root.path().join("var/second.json");
    fs::copy(&f.original, &second).unwrap();
    let mut input = load(&f.input);
    let mut r = reference(&second);
    r["role"] = json!("historical_model_request");
    input["original_requests"].as_array_mut().unwrap().push(r);
    save(&f.input, &input);
    let mock = f.mock();
    let mut a = advice();
    let mut second_answer = a["answers"][0].clone();
    second_answer["request_index"] = json!(1);
    a["answers"].as_array_mut().unwrap().push(second_answer);
    replace_advice(mock.response.borrow_mut().as_mut().unwrap(), &a);
    let result = f.run("two", Some(&mock));
    assert_eq!(result["status"], "completed");
    assert_eq!(result["advisory"]["answers"].as_array().unwrap().len(), 2);
    let payload: Value = serde_json::from_str(
        mock.requests.borrow()[0]["input"][0]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        payload["original_requests"][0]["request"],
        payload["original_requests"][1]["request"]
    );
    assert_eq!(f.object.object_id, "urn:fixture:support");
}

#[test]
fn prepared_value_edit_and_credential_source_cannot_be_archived() {
    let f = Fixture::new();
    let mut prepared = f.prepare();
    prepared.report["adopted"] = json!(true);
    assert!(
        model_assistance::write_prepared(&f.root, &prepared, Path::new("var/tampered")).is_err()
    );
    assert!(!f.root.path().join("var/tampered/input.original").exists());
    let mut profile = load(&f.profile);
    profile["credential_file"] = json!(f.support);
    save(&f.profile, &profile);
    let error = match model_assistance::prepare(&f.root, &f.input, &f.profile) {
        Err(error) => error,
        Ok(_) => panic!("credential source archived"),
    };
    assert_eq!(
        error.to_string(),
        "model_assistance_credential_source_forbidden"
    );
}

#[test]
fn malformed_original_and_binary_support_keep_original_kernel_checks_and_raw_archive() {
    for kind in ["malformed-request", "binary-support"] {
        let f = Fixture::new();
        let path = if kind == "malformed-request" {
            &f.original
        } else {
            &f.support
        };
        let raw = if kind == "malformed-request" {
            b"{this is not a valid original request}".to_vec()
        } else {
            vec![0xff, 0xfe, 0]
        };
        fs::write(path, &raw).unwrap();
        let mut input = load(&f.input);
        if kind == "malformed-request" {
            input["original_requests"][0]["sha256"] = json!(sha256(&raw));
        } else {
            input["support_files"][0]["sha256"] = json!(sha256(&raw));
            let mut p = load(&f.project_path);
            p["evidence"]["binary-source"] = json!(sha256(&raw));
            save(&f.project_path, &p);
            input["project"] = reference(&f.project_path);
            let project: Project = serde_json::from_value(p).unwrap();
            input["project_context"] = json!(project::context(&project, &f.object).unwrap());
        }
        save(&f.input, &input);
        let mock = f.mock();
        let result = f.run(kind, Some(&mock));
        assert_eq!(result["status"], "blocked");
        assert_eq!(mock.requests.borrow().len(), 0);
        assert!(
            result["current_kernel_checks_before"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["id"] == "engineering_assertions")
        );
        let archive = if kind == "malformed-request" {
            "original-request-0.original"
        } else {
            "support-0.original"
        };
        assert_eq!(
            fs::read(f.root.path().join(format!("var/{kind}/{archive}"))).unwrap(),
            raw
        );
    }
}

#[test]
fn generated_response_schema_tracks_metadata_changes_and_rejects_unsupported_mappings() {
    let actual = model_assistance::response_schema().unwrap();
    assert_eq!(actual["additionalProperties"], false);
    assert_eq!(
        actual["properties"]["schema"]["enum"],
        json!(["oev2.model-advisory/1"])
    );
    assert_eq!(
        actual["properties"]["answers"]["items"]["properties"]["request_index"]["type"],
        "integer"
    );
    let kind = &actual["properties"]["findings"]["items"]["properties"]["kind"]["enum"];
    let values: std::collections::BTreeSet<_> = kind
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        values,
        std::collections::BTreeSet::from(["fact", "candidate", "missing_condition", "unsupported"])
    );
    // A separately stated metadata change must update both properties and
    // required/enum, without an edit to the API-schema algorithm.
    let fields = [
        ("Trial", "children", "Vec<Child>"),
        ("Child", "mode", "String"),
    ];
    let allowed = [("Child", "mode", "original")];
    let baseline = model_assistance::schema_from_metadata("Trial", &fields, &allowed).unwrap();
    let mut changed_fields = fields.to_vec();
    changed_fields.push(("Child", "version", "u64"));
    let mut changed_allowed = allowed.to_vec();
    changed_allowed.push(("Child", "mode", "candidate"));
    let changed =
        model_assistance::schema_from_metadata("Trial", &changed_fields, &changed_allowed).unwrap();
    assert!(
        baseline["properties"]["children"]["items"]["properties"]
            .get("version")
            .is_none()
    );
    assert_eq!(
        changed["properties"]["children"]["items"]["properties"]["version"]["minimum"],
        0
    );
    assert!(
        changed["properties"]["children"]["items"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("version"))
    );
    assert!(
        changed["properties"]["children"]["items"]["properties"]["mode"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("candidate"))
    );
    assert!(
        model_assistance::schema_from_metadata(
            "Unsupported",
            &[("Unsupported", "value", "usize")],
            &[]
        )
        .is_err()
    );
    assert!(
        model_assistance::schema_from_metadata(
            "Recursive",
            &[("Recursive", "child", "Recursive")],
            &[]
        )
        .is_err()
    );
    let nested = format!("{}String{}", "Vec<".repeat(40), ">".repeat(40));
    assert!(
        model_assistance::schema_from_metadata("Deep", &[("Deep", "value", nested.as_str())], &[])
            .is_err()
    );
    let nullable = model_assistance::schema_from_metadata(
        "Nullable",
        &[("Nullable", "optional", "Option<String>")],
        &[],
    )
    .unwrap();
    assert_eq!(nullable["required"], json!(["optional"]));
    assert_eq!(
        nullable["properties"]["optional"]["anyOf"],
        json!([{"type":"string"},{"type":"null"}])
    );
}

#[test]
fn source_question_contract_rejects_provider_wire_and_malformed_options() {
    for mode in [
        "provider_wire",
        "missing_schema",
        "empty_questions",
        "blank_instructions",
        "invalid_options",
    ] {
        let f = Fixture::new();
        let mut source = load(&f.original);
        match mode {
            "provider_wire" => {
                source["model"] = json!("retired-service-model");
            }
            "missing_schema" => {
                source.as_object_mut().unwrap().remove("schema");
            }
            "empty_questions" => {
                source["questions"] = json!({});
            }
            "blank_instructions" => {
                source["questions"]["book"]["instructions"] = json!(" ");
            }
            "invalid_options" => {
                source["questions"]["book"]["criteria"] = json!({"needed": false});
            }
            _ => unreachable!(),
        }
        save(&f.original, &source);
        let mut input = load(&f.input);
        input["original_requests"][0]["sha256"] = json!(sha256(fs::read(&f.original).unwrap()));
        save(&f.input, &input);
        let prepared = model_assistance::prepare(&f.root, &f.input, &f.profile).unwrap();
        assert!(!prepared.admitted(), "{mode}");
        assert_eq!(prepared.value()["status"], "blocked", "{mode}");
        assert!(
            prepared.value()["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["code"] == "model_assistance_original_request_invalid"),
            "{mode}"
        );
        let mock = f.mock();
        let result = model_assistance::run(
            &f.root,
            &f.input,
            &f.profile,
            Path::new("var/refused"),
            Some(&mock),
        )
        .unwrap();
        assert_eq!(result["status"], "blocked", "{mode}");
        assert_eq!(result["adopted"], false, "{mode}");
        assert!(mock.requests.borrow().is_empty());
    }
}
