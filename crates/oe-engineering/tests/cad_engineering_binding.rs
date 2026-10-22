//! Synthetic source-bound association controls. Original CAD execution claims
//! stay supplied declarations; this suite performs no native/model operation.
use oe_contracts::EngineeringDesignAttemptRecordRequest2;
use oe_core::{SkillRoot, sha256};
use oe_engineering::design_attempt::cad_binding::{record, verify_record};
use oe_kernel::project::{Project, context};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn save(root: &Path, path: &str, value: &Value) -> String {
    let bytes = serde_json::to_vec_pretty(value).unwrap();
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, &bytes).unwrap();
    sha256(bytes)
}
struct Fixture {
    _dir: tempfile::TempDir,
    root: SkillRoot,
    request: EngineeringDesignAttemptRecordRequest2,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("SKILL.md"), "synthetic").unwrap();
        let root = SkillRoot::new(dir.path().canonicalize().unwrap()).unwrap();
        let oracle: Value = serde_json::from_str(include_str!(
            "../../oe-cad/tests/fixtures/cad-evidence-legacy-v1.json"
        ))
        .unwrap();
        let original = oracle["verification"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "cad" && c["valid"] == true)
            .unwrap();
        for (name, text) in original["files"].as_object().unwrap() {
            let path = root.path().join("var/cad").join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text.as_str().unwrap()).unwrap();
        }
        let mut packet = original["packet"].clone();
        packet["project"] = json!({"project_id":"project","configuration_id":"study","model_revision":"r2","product_state":"reference_only"});
        let packet_sha = save(root.path(), "var/cad/packet.json", &packet);
        let acceptance = json!([
            "original purpose",
            "independent intent and native/save/reopen remain unverified",
            "manufacturing and physical acceptance remain unverified"
        ]);
        let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"plain-object","object_revision":"r2","purpose":"fixture","acceptance":acceptance,"facts":[],"assertions":[]});
        let project:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"project","revision":"r1","purpose":"fixture","objects":{"plain-object":"r2"},"conditions":{"study":basis.to_string()},"evidence":{"original":"a".repeat(64)},"decisions":[],"dependencies":{}})).unwrap();
        let object = oe_contracts::ObjectIdentity {
            object_id: "plain-object".into(),
            revision: "r2".into(),
            condition_scope: "study".into(),
            source_sha256: "a".repeat(64),
        };
        let current = context(&project, &object).unwrap();
        let project_sha = save(root.path(), "var/project.json", &json!(project));
        let work = json!({"schema":"oev2.engineering-work/3","project":{"path":"var/project.json","sha256":project_sha},"object":object,"project_context":current,"acceptance":acceptance,"necessary_functions":["source association"],"physical_mechanisms":["unverified"],"gaps":[],"source_requests":[],"calculations":[],"handoff":null});
        let work_sha = save(root.path(), "var/work.json", &work);
        let plan:Vec<_>=["work","harness","cad_evidence","handoff","kernel_review","feedback"].iter().map(|a|json!({"step_id":a,"activity":a,"acceptance":acceptance,"required_evidence":["original source; unknown until verified"]})).collect();
        let request=serde_json::from_value(json!({"schema":"oev2.design-attempt-record/2","attempt_id":"reference-replay","binding":{"project":{"path":"var/project.json","sha256":project_sha},"work":{"path":"var/work.json","sha256":work_sha},"object":object,"project_context":current,"condition_sha256":current.condition_sha256,"acceptance":acceptance,"product_state":"reference_only","evaluation_scope":"development_replay","independent_intent":"unknown"},"artifacts":[{"step_id":"cad_evidence","role":"cad_evidence","artifact":{"path":"var/cad/packet.json","sha256":packet_sha},"source_root":"var/cad"}],"claims":[{"artifact_index":0,"source_pointer":"/objects/0","acceptance_index":1,"acceptance":acceptance[1]}],"tool_plan":plan,"reference_exposure":[{"stage":"unknown","source_refs":["original"],"basis":"synthetic legacy oracle already exposed"}],"notes":["no native execution"]})).unwrap();
        Self {
            _dir: dir,
            root,
            request,
        }
    }
}

#[test]
fn record_and_revalidate_use_actual_owners_and_preserve_all_original_obligations() {
    let f = Fixture::new();
    let output = record(&f.root, &f.request, Path::new("var/record")).unwrap();
    assert_eq!(
        output["association"]["status"],
        "verified_source_association_only"
    );
    assert_eq!(
        output["association"]["remaining_acceptance"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(output["association"]["formal_semantic_review"], "not_run");
    assert_eq!(output["association"]["native_adapter"], "not_run");
    assert_eq!(output["association"]["independent_intent"], "unknown");
    assert_eq!(output["engineering_conclusion"], "unknown");
    assert_eq!(output["adopted"], false);
    assert!(
        output["association"]["artifacts"][0]["source_integrity"]["integrity"]["source_results"]
            .as_array()
            .unwrap()
            .len()
            > 0
    );
    assert_eq!(
        output["association"]["selected_claims"][0]["original_value"],
        output["association"]["artifacts"][0]["original"]["objects"][0]
    );
    verify_record(&f.root, &f.request, Path::new("var/record")).unwrap();
    fs::write(f.root.path().join("var/record/result.json"), b"{}").unwrap();
    assert!(verify_record(&f.root, &f.request, Path::new("var/record")).is_err());
}

#[test]
fn mismatched_current_object_scope_acceptance_and_reference_qualification_are_refused() {
    let f = Fixture::new();
    let original = json!(f.request);
    for (path, value) in [
        ("/binding/object/object_id", json!("another-object")),
        ("/binding/object/condition_scope", json!("another-scope")),
        ("/binding/condition_sha256", json!("b".repeat(64))),
        ("/binding/focus_iri", json!("urn:forged")),
        ("/binding/independent_intent", json!("independent")),
        ("/binding/product_state", json!("independent_design")),
        ("/claims/0/acceptance", json!("forged requirement")),
        ("/claims/0/source_pointer", json!("/missing")),
        ("/claims/0/artifact_index", json!(31)),
        ("/artifacts/0/role", json!("feedback")),
    ] {
        let mut bad = original.clone();
        if path == "/binding/focus_iri" {
            bad["binding"]["focus_iri"] = value;
        } else {
            *bad.pointer_mut(path).unwrap() = value;
        }
        let bad = serde_json::from_value(bad).unwrap();
        assert!(
            record(&f.root, &bad, Path::new("var/rejected")).is_err(),
            "{path}"
        );
        assert!(!f.root.path().join("var/rejected").exists());
    }
    for mode in ["missing", "order", "text"] {
        let mut bad = original.clone();
        let a = bad["binding"]["acceptance"].as_array_mut().unwrap();
        match mode {
            "missing" => {
                a.pop();
            }
            "order" => a.swap(0, 1),
            _ => a[0] = json!("changed"),
        };
        let bad = serde_json::from_value(bad).unwrap();
        assert!(
            record(&f.root, &bad, Path::new("var/rejected")).is_err(),
            "{mode}"
        );
    }
}

#[test]
fn same_revision_project_condition_and_packet_sources_cannot_reuse_a_record() {
    let f = Fixture::new();
    record(&f.root, &f.request, Path::new("var/record")).unwrap();
    let p = f.root.path().join("var/project.json");
    let original = fs::read(&p).unwrap();
    let mut changed: Value = serde_json::from_slice(&original).unwrap();
    changed["decisions"] = json!([{"id":"d","object_id":"plain-object","object_revision":"r2","condition_scope":"study","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":[]}]);
    fs::write(&p, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(verify_record(&f.root, &f.request, Path::new("var/record")).is_err());
    fs::write(&p, &original).unwrap();
    verify_record(&f.root, &f.request, Path::new("var/record")).unwrap();
    changed = serde_json::from_slice(&original).unwrap();
    changed["conditions"]["study"] = json!("changed same scope body");
    fs::write(&p, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(verify_record(&f.root, &f.request, Path::new("var/record")).is_err());
    fs::write(&p, &original).unwrap();
    let packet: Value =
        serde_json::from_slice(&fs::read(f.root.path().join("var/cad/packet.json")).unwrap())
            .unwrap();
    let path = f
        .root
        .path()
        .join("var/cad")
        .join(packet["sources"][0]["path"].as_str().unwrap());
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, b"changed original source").unwrap();
    assert!(verify_record(&f.root, &f.request, Path::new("var/record")).is_err());
    fs::write(&path, bytes).unwrap();
    verify_record(&f.root, &f.request, Path::new("var/record")).unwrap();
}

#[test]
fn resigned_inventory_cannot_omit_or_rewrite_saved_originals() {
    let f = Fixture::new();
    record(&f.root, &f.request, Path::new("var/record")).unwrap();
    let directory = f.root.path().join("var/record");
    let mut terminal: Value =
        serde_json::from_slice(&fs::read(directory.join("completed.json")).unwrap()).unwrap();
    terminal["files"] = json!({});
    fs::remove_file(directory.join("artifact-000.json")).unwrap();
    fs::write(directory.join("project.json"), b"{}").unwrap();
    fs::write(
        directory.join("association-declarations.nt"),
        b"<urn:forged> <urn:claim> \"passed\" .\n",
    )
    .unwrap();
    save(f.root.path(), "var/record/completed.json", &terminal);
    assert!(
        verify_record(&f.root, &f.request, Path::new("var/record")).is_err(),
        "caller-resigned empty inventory must not bypass missing/changed required originals"
    );
}

#[test]
fn resigned_declaration_graph_and_extra_inventory_cannot_pass() {
    let f = Fixture::new();
    record(&f.root, &f.request, Path::new("var/record")).unwrap();
    let directory = f.root.path().join("var/record");
    let mut terminal: Value =
        serde_json::from_slice(&fs::read(directory.join("completed.json")).unwrap()).unwrap();
    let bytes = b"<urn:forged> <urn:claim> \"passed\" .\n";
    fs::write(directory.join("association-declarations.nt"), bytes).unwrap();
    terminal["files"]["association-declarations.nt"] = json!(sha256(bytes));
    save(f.root.path(), "var/record/completed.json", &terminal);
    assert!(
        verify_record(&f.root, &f.request, Path::new("var/record")).is_err(),
        "a caller-rehashed graph must be compared against current owner declarations"
    );
}

#[test]
fn terminal_semantics_cannot_be_resigned_as_success() {
    let f = Fixture::new();
    record(&f.root, &f.request, Path::new("var/record")).unwrap();
    let path = f.root.path().join("var/record/completed.json");
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for (field, value) in [
        ("adopted", json!(true)),
        ("design_complete", json!(true)),
        ("project_mutated", json!(true)),
        ("native_adapter", json!("passed")),
        ("formal_semantic_review", json!("passed")),
        ("engineering_conclusion", json!("pass")),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        save(f.root.path(), "var/record/completed.json", &changed);
        assert!(
            verify_record(&f.root, &f.request, Path::new("var/record")).is_err(),
            "terminal {field} is an exact original limitation, not caller authority"
        );
    }
}

#[test]
fn repeated_valid_original_subtrees_refuse_file_budget_before_output_creation() {
    let mut f = Fixture::new();
    let path = f.root.path().join("var/cad/packet.json");
    let mut original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    original["objects"][0]["statement"] = json!("x".repeat(8 * 1024 * 1024));
    f.request.artifacts[0].artifact.sha256 = save(f.root.path(), "var/cad/packet.json", &original);
    let mut request = json!(f.request);
    request["claims"] = json!((0..3).map(|i| json!({"artifact_index":0,"source_pointer":"/objects/0","acceptance_index":i,"acceptance":request["binding"]["acceptance"][i]})).collect::<Vec<_>>());
    let request = serde_json::from_value(request).unwrap();
    let result = record(&f.root, &request, Path::new("var/oversize"));
    assert!(result.is_err());
    assert!(
        !f.root.path().join("var/oversize").exists(),
        "per-file refusal must precede any output creation, never write an over16MiB result first"
    );
}

#[test]
fn complete_serialized_collection_including_terminal_is_bounded_before_creation() {
    let mut f = Fixture::new();
    let padding = vec![b' '; 8 * 1024 * 1024];
    let project_path = f.root.path().join("var/project.json");
    let mut project = padding.clone();
    project.extend(fs::read(&project_path).unwrap());
    fs::write(&project_path, &project).unwrap();
    f.request.binding.project.sha256 = sha256(&project);
    let work_path = f.root.path().join("var/work.json");
    let mut work: Value = serde_json::from_slice(&fs::read(&work_path).unwrap()).unwrap();
    work["project"]["sha256"] = json!(f.request.binding.project.sha256);
    let mut work_bytes = padding.clone();
    work_bytes.extend(serde_json::to_vec(&work).unwrap());
    fs::write(&work_path, &work_bytes).unwrap();
    f.request.binding.work.sha256 = sha256(&work_bytes);
    let bytes = fs::read(f.root.path().join("var/cad/packet.json")).unwrap();
    let template = f.request.artifacts[0].clone();
    f.request.artifacts.clear();
    for i in 0..4 {
        let relative = format!("var/cad/packet-{i}.json");
        let mut packet = padding.clone();
        packet.extend(&bytes);
        fs::write(f.root.path().join(&relative), &packet).unwrap();
        let mut artifact = template.clone();
        artifact.artifact.path = relative;
        artifact.artifact.sha256 = sha256(&packet);
        f.request.artifacts.push(artifact);
    }
    f.request.notes = vec!["x".repeat(10 * 1024 * 1024)];
    let result = record(&f.root, &f.request, Path::new("var/oversize-total"));
    assert!(result.is_err());
    assert!(
        !f.root.path().join("var/oversize-total").exists(),
        "complete serialized copies/result/request/terminal must fit64MiB before creation"
    );
}

#[test]
fn required_inventory_rejects_extra_missing_and_nonregular_entries() {
    let f = Fixture::new();
    record(&f.root, &f.request, Path::new("var/record")).unwrap();
    let directory = f.root.path().join("var/record");
    fs::write(directory.join("unexpected.json"), b"{}").unwrap();
    assert!(verify_record(&f.root, &f.request, Path::new("var/record")).is_err());
    fs::remove_file(directory.join("unexpected.json")).unwrap();
    verify_record(&f.root, &f.request, Path::new("var/record")).unwrap();
    let copy = fs::read(directory.join("project.json")).unwrap();
    fs::remove_file(directory.join("project.json")).unwrap();
    assert!(verify_record(&f.root, &f.request, Path::new("var/record")).is_err());
    fs::create_dir(directory.join("project.json")).unwrap();
    assert!(verify_record(&f.root, &f.request, Path::new("var/record")).is_err());
    fs::remove_dir(directory.join("project.json")).unwrap();
    fs::write(directory.join("project.json"), copy).unwrap();
    verify_record(&f.root, &f.request, Path::new("var/record")).unwrap();
}

#[test]
fn full_u64_indices_are_rejected_without_truncation() {
    let f = Fixture::new();
    for name in ["artifact_index", "acceptance_index"] {
        let mut request = json!(f.request);
        request["claims"][0][name] = json!(u64::MAX);
        let request = serde_json::from_value(request).unwrap();
        assert!(record(&f.root, &request, Path::new("var/rejected-index")).is_err());
        assert!(!f.root.path().join("var/rejected-index").exists());
    }
}

fn supplied_cli_files(
    f: &Fixture,
    core_sha: &str,
    note_bytes: usize,
) -> std::collections::BTreeMap<String, Vec<u8>> {
    use oe_engineering::design_attempt::cad_binding::bounded_json;
    let input = json!(f.request);
    let input_sha = save(f.root.path(), "var/cli-request.json", &input);
    let raw = fs::read(f.root.path().join("var/cli-request.json")).unwrap();
    // These are explicitly supplied records, not a real executable assertion.
    let identity = json!({"normalizer":{"scope":"supplied synthetic"},"cli":{"scope":"supplied synthetic"},"semantic_contracts_sha256":"a".repeat(64),"executable_sha256":"b".repeat(64),"notes":"x".repeat(note_bytes)});
    let bytes = bounded_json(&identity).unwrap();
    let terminal = json!({"schema":"oev2.design-attempt-cli-completed/1","status":"completed","operation":"record","input":{"path":"var/cli-request.json","sha256":input_sha},
        "core_completed":{"path":"completed.json","sha256":core_sha},"input_copy":{"path":"cli-input.json","sha256":sha256(&raw)},
        "execution_binding":{"path":"execution-binding.json","sha256":sha256(&bytes)},"execution_recheck":{"path":"execution-recheck.json","sha256":sha256(&bytes)},
        "adopted":false,"design_complete":false,"project_mutated":false});
    std::collections::BTreeMap::from([
        (
            "cli-completed.json".into(),
            bounded_json(&terminal).unwrap(),
        ),
        ("cli-input.json".into(), raw),
        ("execution-binding.json".into(), bytes.clone()),
        ("execution-recheck.json".into(), bytes),
    ])
}

#[test]
fn complete_cli_attachments_are_bounded_and_retained_without_authenticating_supplied_identity() {
    use oe_engineering::design_attempt::cad_binding::prepare_record;
    let f = Fixture::new();
    let plan = prepare_record(&f.root, &f.request).unwrap();
    let outer = supplied_cli_files(&f, &plan.core_completed_sha256(), 0);
    plan.publish(&f.root, Path::new("var/cli-record"), outer, || Ok(()))
        .unwrap();
    let result = verify_record(&f.root, &f.request, Path::new("var/cli-record")).unwrap();
    assert_eq!(result["engineering_conclusion"], "unknown");
    assert!(
        result["cli_identity_authority"]
            .as_str()
            .unwrap()
            .contains("does not authenticate")
    );
    fs::remove_file(f.root.path().join("var/cli-record/execution-recheck.json")).unwrap();
    assert!(verify_record(&f.root, &f.request, Path::new("var/cli-record")).is_err());
}

#[test]
fn cli_attachments_and_core_share_the_same_prepublication_collection_budget() {
    use oe_engineering::design_attempt::cad_binding::prepare_record;
    let mut f = Fixture::new();
    let padding = vec![b' '; 8 * 1024 * 1024];
    for kind in ["project", "work"] {
        let path = f.root.path().join(format!("var/{kind}.json"));
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if kind == "work" {
            value["project"]["sha256"] = json!(f.request.binding.project.sha256);
        }
        let mut bytes = padding.clone();
        bytes.extend(serde_json::to_vec(&value).unwrap());
        fs::write(&path, &bytes).unwrap();
        if kind == "project" {
            f.request.binding.project.sha256 = sha256(&bytes);
        } else {
            f.request.binding.work.sha256 = sha256(&bytes);
        }
    }
    let original = fs::read(f.root.path().join("var/cad/packet.json")).unwrap();
    let template = f.request.artifacts[0].clone();
    f.request.artifacts.clear();
    for i in 0..4 {
        let relative = format!("var/cad/budget-packet-{i}.json");
        let mut bytes = padding.clone();
        bytes.extend(&original);
        fs::write(f.root.path().join(&relative), &bytes).unwrap();
        let mut a = template.clone();
        a.artifact.path = relative;
        a.artifact.sha256 = sha256(bytes);
        f.request.artifacts.push(a);
    }
    let plan = prepare_record(&f.root, &f.request).unwrap();
    let outer = supplied_cli_files(&f, &plan.core_completed_sha256(), 9 * 1024 * 1024);
    assert!(
        plan.publish(&f.root, Path::new("var/cli-overbudget"), outer, || Ok(()))
            .is_err()
    );
    assert!(!f.root.path().join("var/cli-overbudget").exists());
}
