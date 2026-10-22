use anyhow::{Result, bail};
use oe_core::json::{canonical_json, strict_json};
use oe_core::{SkillRoot, sha256};
use oe_semantic::review::{
    NativeReviewExecutor, NativeReviewInput, execute_reviews, execute_reviews_in_context_with,
    execute_reviews_with, pointer_value, project_record, validate_binding,
};
use serde_json::{Value, json};
use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn skill() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Case {
    root: PathBuf,
    request: Value,
}
impl Case {
    fn new() -> Self {
        let root = skill().join("var/maintenance/current-rust-tests").join(format!(
            "review-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("var/project/evidence")).unwrap();
        fs::create_dir(root.join("var/project/registry")).unwrap();
        fs::write(root.join("SKILL.md"), "synthetic test root").unwrap();
        let original = skill();
        let lock: Value =
            strict_json(&fs::read(original.join("runtime/semantic-bundles.json")).unwrap())
                .unwrap();
        for relative in [
            "runtime/semantic-bundles.json",
            "runtime/semantica-source-lock.json",
            lock["bundles"]["engineering-judgment-intake"]["path"]
                .as_str()
                .unwrap(),
            lock["bundles"]["engineering-evidence-methods"]["path"]
                .as_str()
                .unwrap(),
        ] {
            fs::create_dir_all(root.join(relative).parent().unwrap()).unwrap();
            fs::copy(original.join(relative), root.join(relative)).unwrap();
        }
        for name in [
            "review-current-support-identity_current.json",
            "review-sources.json",
        ] {
            fs::copy(
                original.join("examples/judgment_intake").join(name),
                root.join("var/project/evidence").join(name),
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
            root.join("var/project/binding.json"),
            canonical_json(&binding).unwrap(),
        )
        .unwrap();
        let request = json!({"id":"identity-check","record":"var/project/evidence/review-current-support-identity_current.json",
            "record_sha256":sha256(fs::read(root.join("var/project/evidence/review-current-support-identity_current.json")).unwrap()),
            "binding":"var/project/binding.json","binding_sha256":sha256(fs::read(root.join("var/project/binding.json")).unwrap()),
            "evidence_root":"var/project/evidence","workspace":"var/project/registry","actor":"synthetic-operator"});
        Self { root, request }
    }
    fn execute(&self, requests: Value, executor: &dyn NativeReviewExecutor) -> Result<Value> {
        execute_reviews_with(
            &self.root,
            &requests,
            "synthetic-project",
            Path::new("var/project/reviews"),
            executor,
        )
    }
    fn binding(&mut self, change: impl FnOnce(&mut Value)) {
        let path = self.root.join(self.request["binding"].as_str().unwrap());
        let mut binding = strict_json(&fs::read(&path).unwrap()).unwrap();
        change(&mut binding);
        let raw = canonical_json(&binding).unwrap();
        self.request["binding_sha256"] = sha256(&raw).into();
        fs::write(path, raw).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

type Mutation = dyn Fn(&Path, &NativeReviewInput, &mut Value);
#[derive(Default)]
struct Fixture {
    calls: Cell<usize>,
    mutation: Option<Box<Mutation>>,
    fail: bool,
    fail_call: Option<usize>,
    clear: bool,
}
impl NativeReviewExecutor for Fixture {
    fn review(&self, root: &Path, input: &NativeReviewInput) -> Result<Value> {
        self.calls.set(self.calls.get() + 1);
        if self.fail || self.fail_call == Some(self.calls.get()) {
            bail!("synthetic_native_denied");
        }
        let task = strict_json(&fs::read(&input.task)?)?;
        assert_eq!(task["actor_id"], "synthetic-operator");
        let bundle = oe_semantic::load_bundle(root, "engineering-judgment-intake")?;
        let asset_hash = |id: &str| {
            bundle.manifest["assets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["asset_id"] == id)
                .unwrap()["sha256"]
                .clone()
        };
        let mut native = json!({"contract":"semantica.ontology.decision-review/v1","status":if self.clear {"clear"} else {"blocked"},
            "scope_id":input.scope_id,"focus_iri":input.focus_iri,"focus_type_iri":input.focus_type_iri,
            "evidence":{"id":input.source_id,"sha256":sha256(fs::read(&input.evidence_file)?),"uri":task["evidence"][0]["uri"],
                "captured_at":task["evidence"][0]["captured_at"],"format":"turtle","input_quad_count":35,
                "input_dataset_sha256":"a".repeat(64),"evaluated_dataset_sha256":"b".repeat(64)},
            "package":{"id":bundle.spec["package_id"],"version":bundle.spec["package_version"],"registry_package_sha256":bundle.spec["package_sha256"],
                "origin":"promoted_registry","query_asset_id":input.query_asset_id,"shape_asset_id":input.shape_asset_id,
                "query_asset_sha256":asset_hash(&input.query_asset_id),"shape_asset_sha256":asset_hash(&input.shape_asset_id),"digest":"c".repeat(64)},
            "focus_present":true,"focus_type_present":true,"focus_instance_count":1,"shape_conforms":self.clear,
            "findings":if self.clear {json!([])} else {json!([{"source":"synthetic-query","detail":"revision mismatch"}])},
            "violations":if self.clear {json!([])} else {json!([{"source":"synthetic-shape","detail":"missing identity"}])},
            "created_at":"2026-09-27T00:00:00Z","meaning":"semantic-record-consistency-only; no physical acceptance"});
        native["report_sha256"] = sha256(canonical_json(&native)?).into();
        if let Some(mutation) = &self.mutation {
            mutation(root, input, &mut native);
        }
        Ok(json!({"execution":{"review":native}}))
    }
}

#[test]
fn empty_requests_need_neither_root_runtime_nor_output() {
    let report = execute_reviews(
        Path::new("/nonexistent-review-root"),
        &json!([]),
        "concept",
        Path::new("unused"),
    )
    .unwrap();
    assert_eq!(report["status"], "not_run");
    assert_eq!(report["reviews"], json!([]));
    let mut content = report.clone();
    content.as_object_mut().unwrap().remove("feedback_sha256");
    assert_eq!(
        report["feedback_sha256"],
        sha256(canonical_json(&content).unwrap())
    );
}

#[test]
fn projection_matches_existing_python_contract_golden_bytes() {
    let case = Case::new();
    let projection = project_record(
        &case.root,
        Path::new(case.request["record"].as_str().unwrap()),
        Path::new(case.request["evidence_root"].as_str().unwrap()),
        "engineering-judgment-intake",
    )
    .unwrap();
    // Independently retained from the existing source-bound native fixture.
    assert_eq!(
        projection.audit["record_sha256"],
        "7a0ce956b74af283faaf184ca9bf79f90b46c18a41346953926b98de88c9da17"
    );
    assert_eq!(
        sha256(&projection.rdf),
        "529787fc58fe7a3ef399577b2e62e0fe34f5fb46b5c6e9c034ae4a1c4f936f21"
    );
    assert_eq!(projection.resolved_facts["subject_id"], "assembly-A");
    assert_eq!(projection.audit["semantic_execution"], "not_run");
}

#[test]
fn findings_violations_artifact_hashes_and_actor_remain_bound() {
    let case = Case::new();
    let fixture = Fixture::default();
    let report = case.execute(json!([case.request]), &fixture).unwrap();
    assert_eq!(report["status"], "completed");
    let item = &report["reviews"][0];
    assert_eq!(item["review"]["status"], "blocked");
    assert_eq!(item["review"]["findings"].as_array().unwrap().len(), 1);
    assert_eq!(item["review"]["violations"].as_array().unwrap().len(), 1);
    assert_eq!(item["actor"], "synthetic-operator");
    assert_eq!(item["claim"]["subject_revision"], "R2");
    assert_eq!(
        item["inputs"]["record"]["sha256"],
        case.request["record_sha256"]
    );
    assert_eq!(
        item["inputs"]["binding"]["sha256"],
        case.request["binding_sha256"]
    );
    assert_eq!(
        item["request_sha256"],
        sha256(canonical_json(&case.request).unwrap())
    );
    for artifact in item["artifacts"].as_object().unwrap().values() {
        assert_eq!(
            artifact["sha256"],
            sha256(fs::read(case.root.join(artifact["path"].as_str().unwrap())).unwrap())
        );
    }
    assert!(
        !String::from_utf8(canonical_json(&report).unwrap())
            .unwrap()
            .contains(case.root.to_str().unwrap())
    );
    assert_eq!(
        strict_json(&fs::read(case.root.join("var/project/reviews/feedback.json")).unwrap())
            .unwrap(),
        report
    );
    execute_reviews_with(
        &case.root,
        &json!([case.request]),
        "synthetic-project",
        Path::new("var/project/again"),
        &fixture,
    )
    .unwrap();
    assert_eq!(fixture.calls.get(), 2);
}

#[test]
fn external_project_review_reads_rules_from_software_and_receipts_from_data() {
    let software = Case::new();
    let data = Case::new();
    fs::remove_dir_all(software.root.join("var")).unwrap();
    fs::remove_dir_all(data.root.join("runtime")).unwrap();
    fs::remove_file(data.root.join("SKILL.md")).unwrap();
    let root = SkillRoot::new(&software.root)
        .unwrap()
        .with_project_data_root(&data.root)
        .unwrap();
    let expected_source = root.path().to_path_buf();
    let expected_data = root.data_path().to_path_buf();
    let fixture = Fixture {
        mutation: Some(Box::new(move |source, input, _| {
            assert_eq!(source, expected_source);
            assert!(input.binding.starts_with(&expected_data));
            assert!(input.workspace.starts_with(&expected_data));
            assert!(input.task.starts_with(&expected_data));
        })),
        ..Default::default()
    };
    let report = execute_reviews_in_context_with(
        &root,
        &json!([data.request]),
        "synthetic-project",
        Path::new("reviews/run"),
        &fixture,
    )
    .unwrap();
    assert_eq!(report["status"], "completed");
    assert_eq!(fixture.calls.get(), 1);
    assert_eq!(report["fact_admitted"], false);
    assert!(!software.root.join("reviews").exists());
    for artifact in report["reviews"][0]["artifacts"]
        .as_object()
        .unwrap()
        .values()
    {
        let path = artifact["path"].as_str().unwrap();
        assert!(path.starts_with("reviews/run/"));
        assert_eq!(
            artifact["sha256"],
            sha256(fs::read(data.root.join(path)).unwrap())
        );
    }
    assert!(
        execute_reviews_in_context_with(
            &root,
            &json!([data.request]),
            "synthetic-project",
            &software.root.join("var/escape"),
            &fixture,
        )
        .is_err()
    );
    assert_eq!(
        fixture.calls.get(),
        1,
        "outside output must fail before execution"
    );
}

#[test]
fn clear_native_review_never_admits_a_fact_or_physical_acceptance() {
    let case = Case::new();
    let fixture = Fixture {
        clear: true,
        ..Fixture::default()
    };
    let report = case.execute(json!([case.request]), &fixture).unwrap();
    assert_eq!(report["reviews"][0]["review"]["status"], "clear");
    assert_eq!(report["fact_admitted"], false);
    assert_eq!(report["reviews"][0]["fact_admitted"], false);
}

#[test]
fn all_requests_preflight_before_any_native_call_or_output() {
    for kind in ["bad_hash", "duplicate", "budget", "raw_pass", "wrong_type"] {
        let case = Case::new();
        let fixture = Fixture::default();
        let requests = match kind {
            "budget" => json!(vec![case.request.clone(); 17]),
            "wrong_type" => json!({"review":case.request}),
            _ => {
                let mut second = case.request.clone();
                second["id"] = "second".into();
                if kind == "bad_hash" {
                    second["record_sha256"] = "0".repeat(64).into();
                }
                if kind == "duplicate" {
                    second["id"] = case.request["id"].clone();
                }
                if kind == "raw_pass" {
                    second["status"] = "PASS".into();
                }
                json!([case.request, second])
            }
        };
        assert!(case.execute(requests, &fixture).is_err(), "{kind}");
        assert_eq!(fixture.calls.get(), 0, "{kind}");
        assert!(!case.root.join("var/project/reviews").exists(), "{kind}");
    }
}

#[test]
fn invalid_paths_hashes_project_or_binding_authority_never_dispatch() {
    for kind in [
        "record_sha",
        "binding_sha",
        "foreign_project",
        "wrong_package",
        "wrong_baseline",
        "no_review",
        "lifecycle_escalation",
        "backend",
        "actor",
        "missing_sha",
        "outside",
        "traversal",
        "overlap",
    ] {
        let mut case = Case::new();
        let fixture = Fixture::default();
        match kind {
            "record_sha" => case.request["record_sha256"] = "0".repeat(64).into(),
            "binding_sha" => case.request["binding_sha256"] = "0".repeat(64).into(),
            "foreign_project" => case.binding(|b| b["project"]["project_id"] = "foreign".into()),
            "wrong_package" => {
                case.binding(|b| b["semantic_target"]["package_id"] = "foreign".into())
            }
            "wrong_baseline" => case.binding(|b| b["baseline"]["digest"] = "0".repeat(64).into()),
            "no_review" => case.binding(|b| b["allowed_actions"] = json!(["propose"])),
            "lifecycle_escalation" => case.binding(|b| {
                b["lifecycle_actions"] = json!(["candidate", "proposed", "committed"])
            }),
            "backend" => case.binding(|b| b["backend"] = "invented".into()),
            "actor" => case.request["actor"] = " ".into(),
            "missing_sha" => {
                case.request
                    .as_object_mut()
                    .unwrap()
                    .remove("record_sha256");
            }
            "outside" => {
                case.request["record"] =
                    json!(skill().join(
                        "examples/judgment_intake/review-current-support-identity_current.json"
                    ))
            }
            "traversal" => {
                case.request["record"] =
                    "var/project/evidence/../evidence/review-current-support-identity_current.json"
                        .into()
            }
            "overlap" => {}
            _ => unreachable!(),
        }
        let output = if kind == "overlap" {
            "var/project/evidence/reviews"
        } else {
            "var/project/reviews"
        };
        let result = execute_reviews_with(
            &case.root,
            &json!([case.request]),
            "synthetic-project",
            Path::new(output),
            &fixture,
        );
        assert!(result.is_err(), "{kind}");
        assert_eq!(fixture.calls.get(), 0, "{kind}");
        assert!(!case.root.join(output).exists(), "{kind}");
    }
}

#[test]
fn input_and_generated_artifact_changes_invalidate_complete_feedback() {
    for name in [
        "record",
        "binding",
        "source",
        "actor_task",
        "projection",
        "audit",
        "workspace",
    ] {
        let case = Case::new();
        let fixture = Fixture {
            mutation: Some(Box::new(move |root, input, _| {
                let path = match name {
                    "record" => root
                        .join("var/project/evidence/review-current-support-identity_current.json"),
                    "binding" => input.binding.clone(),
                    "source" => root.join("var/project/evidence/review-sources.json"),
                    "actor_task" => input.task.clone(),
                    "projection" => input.evidence_file.clone(),
                    "audit" => input.task.parent().unwrap().join("projection.json"),
                    "workspace" => {
                        fs::rename(&input.workspace, input.workspace.with_extension("old"))
                            .unwrap();
                        fs::create_dir(&input.workspace).unwrap();
                        return;
                    }
                    _ => unreachable!(),
                };
                fs::write(path, b"{}").unwrap();
            })),
            ..Fixture::default()
        };
        let report = case.execute(json!([case.request]), &fixture).unwrap();
        assert_blocked_failure(&case, &report);
        assert_eq!(fixture.calls.get(), 1);
        assert!(
            report["failed_reviews"][0]["artifacts"]
                .get("returned-review.json")
                .is_some(),
            "{name}"
        );
    }
}

#[test]
fn forged_native_identity_hash_or_status_is_not_accepted() {
    for field in [
        "scope_id",
        "evidence",
        "package",
        "asset",
        "status",
        "findings",
        "report_sha256",
        "clear_with_findings",
    ] {
        let case = Case::new();
        let fixture = Fixture {
            mutation: Some(Box::new(move |_, _, native| match field {
                "scope_id" => native["scope_id"] = "foreign".into(),
                "evidence" => native["evidence"]["id"] = "foreign".into(),
                "package" => native["package"]["origin"] = "authoring_manifest".into(),
                "asset" => native["package"]["query_asset_sha256"] = "0".repeat(64).into(),
                "status" => native["status"] = "PASS".into(),
                "findings" => native["findings"] = json!({}),
                "report_sha256" => native["report_sha256"] = "0".repeat(64).into(),
                "clear_with_findings" => {
                    native["status"] = "clear".into();
                    native.as_object_mut().unwrap().remove("report_sha256");
                    native["report_sha256"] = sha256(canonical_json(native).unwrap()).into();
                }
                _ => unreachable!(),
            })),
            ..Fixture::default()
        };
        let report = case.execute(json!([case.request]), &fixture).unwrap();
        assert_blocked_failure(&case, &report);
        assert!(
            report["failed_reviews"][0]["artifacts"]
                .get("returned-review.json")
                .is_some(),
            "{field}"
        );
        assert!(report["failed_reviews"][0].get("review").is_none());
    }
}

#[test]
fn denied_execution_has_no_completed_feedback_receipt() {
    let case = Case::new();
    let fixture = Fixture {
        fail: true,
        ..Fixture::default()
    };
    let report = case.execute(json!([case.request]), &fixture).unwrap();
    assert_blocked_failure(&case, &report);
    assert!(
        case.root
            .join("var/project/reviews/review-identity-check/task.json")
            .is_file()
    );
}

#[test]
fn unavailable_production_runtime_retains_not_run_artifact() {
    let case = Case::new();
    // This isolated skill has data packages but no executable runtime/wheel.
    let report = execute_reviews(
        &case.root,
        &json!([case.request]),
        "synthetic-project",
        Path::new("var/project/reviews"),
    )
    .unwrap();
    assert_blocked_failure(&case, &report);
    let path = case
        .root
        .join("var/project/reviews/review-identity-check/native-execution.json");
    let execution = strict_json(&fs::read(path).unwrap()).unwrap();
    assert_eq!(execution["semantic_execution"], "not_run");
    assert!(
        report["failed_reviews"][0]["artifacts"]
            .get("native-execution.json")
            .is_some()
    );
}

#[test]
fn pointer_contract_handles_escaping_and_rejects_ambiguous_indices() {
    let value = json!({"a/b":{"~":["first","second"]},"":"empty-key"});
    assert_eq!(pointer_value(&value, "/a~1b/~0/1").unwrap(), "second");
    assert_eq!(pointer_value(&value, "").unwrap(), &value);
    assert_eq!(pointer_value(&value, "/").unwrap(), "empty-key");
    for pointer in [
        "a/b",
        "/a~2b",
        "/a~1b/~0/01",
        "/a~1b/~0/-",
        "/a~1b/~0/2",
        "/a~1b/~0/999999999999999999999999",
    ] {
        assert!(pointer_value(&value, pointer).is_err(), "{pointer}");
    }
}

fn source_fixture(case: &Case, kind: &str, value: &Value) -> (String, Value) {
    let bundle = oe_semantic::load_bundle(&case.root, "engineering-evidence-methods").unwrap();
    let asset = bundle.manifest["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|asset| asset["asset_id"] == "input-profiles")
        .unwrap();
    let profiles = strict_json(&bundle.payload[asset["path"].as_str().unwrap()]).unwrap();
    let (method, field) = profiles["profiles"]
        .as_object()
        .unwrap()
        .iter()
        .find_map(|(method, profile)| {
            profile["fields"]
                .as_object()
                .unwrap()
                .iter()
                .find(|(_, ty)| *ty == kind)
                .map(|(field, _)| (method.clone(), field.clone()))
        })
        .expect("locked profiles cover source primitive");
    let bytes = canonical_json(&json!({"value":value})).unwrap();
    fs::write(case.root.join("var/project/evidence/fact.json"), &bytes).unwrap();
    let mut record =
        strict_json(&fs::read(case.root.join(case.request["record"].as_str().unwrap())).unwrap())
            .unwrap();
    record["method"] = method.into();
    record["sources"] = json!([{"id":"source","path":"fact.json","sha256":sha256(&bytes),"media_type":"application/json"}]);
    record["facts"] = json!({&field:{"source_id":"source","pointer":"/value"}});
    (field, record)
}
fn project_fixture(case: &Case, record: &Value) -> Result<oe_semantic::review::MethodProjection> {
    let path = case.root.join(case.request["record"].as_str().unwrap());
    fs::write(&path, canonical_json(record)?)?;
    project_record(
        &case.root,
        &path,
        Path::new("var/project/evidence"),
        "engineering-evidence-methods",
    )
}

#[test]
fn source_types_distinguish_numbers_booleans_and_unique_text_arrays() {
    let case = Case::new();
    for (kind, value) in [
        ("number", json!(false)),
        ("number", json!(true)),
        ("number", json!("1.25")),
        ("boolean", json!(0)),
        ("boolean", json!(1.0)),
        ("string", json!("   ")),
        ("strings", json!(["a", 1])),
        ("strings", json!(["a", false])),
        ("strings", json!(["a", "a"])),
        ("strings", json!(["a", ""])),
        ("strings", json!(["a", "   "])),
    ] {
        let (_, record) = source_fixture(&case, kind, &value);
        assert!(project_fixture(&case, &record).is_err(), "{kind}: {value}");
    }
    for (kind, value) in [
        ("number", json!(1.25)),
        ("boolean", json!(false)),
        ("strings", json!(["a", "b"])),
    ] {
        let (field, record) = source_fixture(&case, kind, &value);
        let projection = project_fixture(&case, &record).unwrap();
        assert_eq!(projection.resolved_facts[&field], value);
        assert_eq!(projection.audit["engineering_verdict"], "not_assessed");
    }
}

#[test]
fn empty_list_missing_fact_and_escaped_claim_text_keep_distinct_meanings() {
    let case = Case::new();
    let (field, mut record) = source_fixture(&case, "strings", &json!([]));
    record["claim"]["id"] = "synthetic > . <urn:bad>\n中文".into();
    record["claim"]["statement"] = "line one\n\"quoted\" \\ slash".into();
    let empty = project_fixture(&case, &record).unwrap();
    let ttl = String::from_utf8(empty.rdf).unwrap();
    assert!(ttl.contains(&format!("{field}Complete> true .")));
    assert!(!ttl.contains("<urn:bad>"));
    assert!(ttl.contains("%3E") && ttl.contains("%E4%B8%AD%E6%96%87"));
    assert!(ttl.contains("line one\\n\\\"quoted\\\""));
    record["facts"] = json!({});
    let missing = project_fixture(&case, &record).unwrap();
    assert!(
        !String::from_utf8(missing.rdf)
            .unwrap()
            .contains(&format!("{field}Complete>"))
    );
    assert_eq!(missing.audit["field_lineage"], json!({}));
    assert_eq!(missing.audit["semantic_execution"], "not_run");
}

#[test]
fn current_hash_cannot_hide_duplicate_keys_or_nonfinite_source_values() {
    let case = Case::new();
    for raw in [
        r#"{"value":1,"value":2}"#,
        r#"{"value":NaN}"#,
        r#"{"value":Infinity}"#,
        r#"{"value":1e999}"#,
    ] {
        let (_, mut record) = source_fixture(&case, "number", &json!(1));
        fs::write(case.root.join("var/project/evidence/fact.json"), raw).unwrap();
        record["sources"][0]["sha256"] = sha256(raw).into();
        assert!(project_fixture(&case, &record).is_err(), "{raw}");
    }
    let (_, record) = source_fixture(&case, "string", &json!("source"));
    let mut raw = canonical_json(&record).unwrap();
    raw.pop();
    raw.extend_from_slice(br#", "method":"forged"}"#);
    let path = case.root.join(case.request["record"].as_str().unwrap());
    fs::write(&path, raw).unwrap();
    assert!(
        project_record(
            &case.root,
            &path,
            Path::new("var/project/evidence"),
            "engineering-judgment-intake"
        )
        .is_err()
    );
}

#[test]
fn timestamp_contract_rejects_ambiguous_python_only_iso_spellings() {
    let mut case = Case::new();
    for invalid in [
        "2026-09-27",
        "20260927T000000Z",
        "2026-W39-7T00:00:00Z",
        "2026-09-27T00:00:00",
    ] {
        case.binding(|binding| binding["created_at"] = invalid.into());
        let binding =
            strict_json(&fs::read(case.root.join("var/project/binding.json")).unwrap()).unwrap();
        assert!(validate_binding(&binding, None, None).is_err(), "{invalid}");
    }
    for valid in ["2026-09-27T00:00:00Z", "2026-09-27T00:00:00.123456+08:00"] {
        case.binding(|binding| binding["created_at"] = valid.into());
        let binding =
            strict_json(&fs::read(case.root.join("var/project/binding.json")).unwrap()).unwrap();
        assert!(validate_binding(&binding, None, None).is_ok(), "{valid}");
    }
}

#[cfg(unix)]
#[test]
fn symlink_sources_and_symlink_parent_directories_are_rejected() {
    use std::os::unix::fs::symlink;
    let mut case = Case::new();
    let fixture = Fixture::default();
    symlink(
        case.root.join("var/project/evidence"),
        case.root.join("var/project/alias"),
    )
    .unwrap();
    case.request["evidence_root"] = "var/project/alias".into();
    assert!(case.execute(json!([case.request]), &fixture).is_err());
    assert_eq!(fixture.calls.get(), 0);
    case.request["evidence_root"] = "var/project/evidence".into();
    let source = case.root.join("var/project/evidence/review-sources.json");
    fs::rename(&source, source.with_extension("original")).unwrap();
    symlink(source.with_extension("original"), &source).unwrap();
    assert!(case.execute(json!([case.request]), &fixture).is_err());
    assert_eq!(fixture.calls.get(), 0);
}

fn assert_blocked_failure(case: &Case, report: &Value) {
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["dependent_adoption"], "blocked");
    assert_eq!(report["fact_admitted"], false);
    assert_eq!(report["reviews"], json!([]));
    assert_eq!(report["failed_reviews"][0]["id"], "identity-check");
    assert_eq!(
        report["failed_reviews"][0]["request_sha256"],
        sha256(canonical_json(&case.request).unwrap())
    );
    assert_eq!(
        report["failed_reviews"][0]["inputs"]["record"]["sha256"],
        case.request["record_sha256"]
    );
    assert_eq!(report["failed_reviews"][0]["fact_admitted"], false);
    assert_eq!(
        strict_json(&fs::read(case.root.join("var/project/reviews/feedback.json")).unwrap())
            .unwrap(),
        *report
    );
}

#[test]
fn batch_failure_preserves_completed_findings_and_failed_request_identity() {
    // Injected native DTOs exercise the real review adapter, not engineering acceptance.
    for failed_call in [1, 2] {
        let case = Case::new();
        let mut second = case.request.clone();
        second["id"] = "second-check".into();
        let fixture = Fixture {
            fail_call: Some(failed_call),
            ..Fixture::default()
        };
        let report = case
            .execute(json!([case.request, second]), &fixture)
            .unwrap();
        assert_eq!(fixture.calls.get(), 2);
        assert_eq!(report["status"], "partial");
        assert_eq!(report["dependent_adoption"], "blocked");
        assert_eq!(report["fact_admitted"], false);
        let success = &report["reviews"][0];
        assert_eq!(
            success["id"],
            if failed_call == 1 {
                "second-check"
            } else {
                "identity-check"
            }
        );
        assert_eq!(success["review"]["status"], "blocked");
        assert_eq!(
            success["review"]["findings"],
            json!([{"source":"synthetic-query","detail":"revision mismatch"}])
        );
        assert_eq!(
            success["review"]["violations"],
            json!([{"source":"synthetic-shape","detail":"missing identity"}])
        );
        let failure = &report["failed_reviews"][0];
        assert_eq!(
            failure["id"],
            if failed_call == 1 {
                "identity-check"
            } else {
                "second-check"
            }
        );
        assert_eq!(
            failure["request_sha256"],
            sha256(
                canonical_json(if failed_call == 1 {
                    &case.request
                } else {
                    &second
                })
                .unwrap()
            )
        );
        assert_eq!(failure["failure_phase"], "native_execution");
        assert_eq!(failure["executor_called"], true);
        assert_eq!(failure["reason"], "synthetic_native_denied");
        assert!(failure.get("review").is_none());
        for entry in [success, failure] {
            for artifact in entry["artifacts"].as_object().unwrap().values() {
                assert_eq!(
                    artifact["sha256"],
                    sha256(fs::read(case.root.join(artifact["path"].as_str().unwrap())).unwrap())
                );
            }
        }
        let mut content = report.clone();
        content.as_object_mut().unwrap().remove("feedback_sha256");
        assert_eq!(
            report["feedback_sha256"],
            sha256(canonical_json(&content).unwrap())
        );
    }
}

#[test]
fn later_invalid_native_response_preserves_prior_clear_without_overall_acceptance() {
    let case = Case::new();
    let mut second = case.request.clone();
    second["id"] = "second-check".into();
    let count = Cell::new(0);
    let fixture = Fixture {
        clear: true,
        mutation: Some(Box::new(move |_, _, native| {
            count.set(count.get() + 1);
            if count.get() == 2 {
                native["package"]["registry_package_sha256"] = "0".repeat(64).into();
            }
        })),
        ..Fixture::default()
    };
    let report = case
        .execute(json!([case.request, second]), &fixture)
        .unwrap();
    assert_eq!(report["status"], "partial");
    assert_eq!(report["dependent_adoption"], "blocked");
    assert_eq!(report["fact_admitted"], false);
    assert_eq!(report["reviews"].as_array().unwrap().len(), 1);
    assert_eq!(report["reviews"][0]["id"], "identity-check");
    assert_eq!(report["reviews"][0]["review"]["status"], "clear");
    let failure = &report["failed_reviews"][0];
    assert_eq!(failure["id"], "second-check");
    assert_eq!(failure["failure_phase"], "native_validation");
    assert!(failure.get("review").is_none());
    let original = &failure["artifacts"]["returned-review.json"];
    let raw = fs::read(case.root.join(original["path"].as_str().unwrap())).unwrap();
    assert_eq!(original["sha256"], sha256(&raw));
    assert_eq!(
        strict_json(&raw).unwrap()["execution"]["review"]["package"]["registry_package_sha256"],
        "0".repeat(64)
    );
}
