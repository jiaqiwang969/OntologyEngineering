use oe_contracts::{
    CodeArtifactEligibilityRequest, ControlledVersionDefinition, VersionSelectionRequest,
};
use oe_core::{SkillRoot, sha256, write_new};
use oe_engineering::{artifacts, registry};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn root() -> (tempfile::TempDir, SkillRoot) {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    fs::create_dir_all(workspace.join("var/builds")).unwrap();
    let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
    fs::write(
        temp.path().join("SKILL.md"),
        "private code qualification fixture",
    )
    .unwrap();
    let root = SkillRoot::new(temp.path()).unwrap();
    (temp, root)
}
fn policy(root: &SkillRoot, schema: &str, kind: &str, context: Option<Value>) {
    let mut binding = json!({"kind":kind,"scope_id":"semantica.full-library/1",
        "actor_scopes":{"maintainer":["register","activate","restore"]}});
    if let Some(context) = context {
        binding["code_qualification"] = context;
    }
    let path = root.path().join(registry::POLICY);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        serde_json::to_vec(&json!({"schema":schema,"bindings":{"code":binding}})).unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
fn request(root: &SkillRoot) -> CodeArtifactEligibilityRequest {
    CodeArtifactEligibilityRequest {
        schema: "oev2.code-artifact-eligibility-request/1".into(),
        binding_id: "code".into(),
        revision: "r2".into(),
        scope_id: "semantica.full-library/1".into(),
        source: artifacts::reference(root, &root.path().join("var/input/source.json")).unwrap(),
        assurance: artifacts::reference(root, &root.path().join("var/input/comparison.json"))
            .unwrap(),
    }
}
fn fake(root: &SkillRoot) -> CodeArtifactEligibilityRequest {
    let input = root.path().join("var/input");
    fs::create_dir_all(&input).unwrap();
    write_new(
        &input.join("source.json"),
        &json!({"passed":true,"eligible":true}),
    )
    .unwrap();
    write_new(
        &input.join("comparison.json"),
        &json!({"passed":true,"eligibility":"eligible"}),
    )
    .unwrap();
    request(root)
}
fn definition(q: &CodeArtifactEligibilityRequest) -> ControlledVersionDefinition {
    ControlledVersionDefinition {
        schema: "oev2.controlled-version-definition/1".into(),
        binding_id: q.binding_id.clone(),
        revision: q.revision.clone(),
        kind: "code_artifact".into(),
        scope_id: q.scope_id.clone(),
        source: q.source.clone(),
        assurance: q.assurance.clone(),
    }
}
fn selection(q: &CodeArtifactEligibilityRequest) -> VersionSelectionRequest {
    VersionSelectionRequest {
        schema: "oev2.version-selection-request/1".into(),
        actor: "maintainer".into(),
        binding_id: q.binding_id.clone(),
        revision: q.revision.clone(),
        expected_head_sha256: None,
        reason: "independent full criteria remain unverified".into(),
    }
}
fn no_events(root: &SkillRoot) {
    assert!(
        !root
            .path()
            .join("var/state/controlled-versions")
            .join(sha256("code"))
            .join("events")
            .exists()
    );
    assert!(
        !root
            .path()
            .join("var/state/knowledge/experience-events")
            .exists()
    );
}

fn structural_context(q: &CodeArtifactEligibilityRequest, schema: &str) -> Value {
    let reference = json!({"path":"var/input/not-executed.json", "sha256":"a".repeat(64)});
    let binary = json!({"path":"/not-run/binary", "sha256":"b".repeat(64), "bytes":1,
        "source_root":"/not-run/source", "source_manifest_sha256":"c".repeat(64)});
    let qualification = json!({"schema":"oev2.code-qualification-context/1", "binding_id":q.binding_id, "revision":q.revision,
        "scope_id":q.scope_id, "purpose":"full_code_release", "source":q.source,
        "assurance":q.assurance, "experiment":reference, "experiment_sha256":"d".repeat(64),
        "driver":binary, "evaluator":binary, "required_protocol":reference,
        "dispatch_budget":{"timeout_ms":600000,"output_bytes":4194304}});
    if schema == "oev2.code-qualification-context/2" {
        json!({"schema":schema, "qualification":qualification})
    } else {
        qualification
    }
}

#[test]
fn context1_typed_roundtrip_preserves_original_wire_fields() {
    let (_temp, root) = root();
    let q = fake(&root);
    let original = structural_context(&q, "oev2.code-qualification-context/1");
    let decoded: oe_contracts::CodeQualificationContext =
        serde_json::from_value(original.clone()).unwrap();
    decoded.validate().unwrap();
    let encoded = serde_json::to_value(decoded).unwrap();
    assert_eq!(original, encoded);
    assert!(encoded.get("full_scope_request").is_none());
    assert!(encoded.get("qualification").is_none());
}

#[test]
fn context1_scope_request_presence_including_null_is_rejected() {
    for extra in [
        Value::Null,
        json!({"path":"var/input/scope.json", "sha256":"a".repeat(64)}),
    ] {
        let (_temp, root) = root();
        let q = fake(&root);
        let mut context = structural_context(&q, "oev2.code-qualification-context/1");
        context["full_scope_request"] = extra;
        let path = root.path().join("var/input/context.json");
        write_new(&path, &context).unwrap();
        let reference = artifacts::reference(&root, &path).unwrap();
        policy(
            &root,
            "oev2.controlled-version-authority/2",
            "code_artifact",
            Some(serde_json::to_value(reference).unwrap()),
        );
        assert!(
            registry::code_eligibility(&root, &q)
                .unwrap_err()
                .to_string()
                .contains("scope_request_requires_context2")
        );
        assert!(!root.path().join("var/code-qualification").exists());
        no_events(&root);
    }
}

#[test]
fn context2_requires_a_bound_nonnull_scope_request_before_dispatch() {
    for explicit_null in [false, true] {
        let (_temp, root) = root();
        let q = fake(&root);
        let mut context = structural_context(&q, "oev2.code-qualification-context/2");
        if explicit_null {
            context["full_scope_request"] = Value::Null;
        }
        let path = root.path().join("var/input/context.json");
        write_new(&path, &context).unwrap();
        let reference = artifacts::reference(&root, &path).unwrap();
        policy(
            &root,
            "oev2.controlled-version-authority/2",
            "code_artifact",
            Some(serde_json::to_value(reference).unwrap()),
        );
        assert!(
            registry::code_eligibility(&root, &q)
                .unwrap_err()
                .to_string()
                .contains("full_scope_request_required")
        );
        assert!(!root.path().join("var/code-qualification").exists());
        no_events(&root);
    }
}

#[test]
fn missing_context_preserves_all_four_closed_operations() {
    let (_temp, root) = root();
    let q = fake(&root);
    policy(
        &root,
        "oev2.controlled-version-authority/1",
        "code_artifact",
        None,
    );
    let result = registry::code_eligibility(&root, &q).unwrap();
    assert_eq!(result.execution, "not_run");
    assert_eq!(result.eligibility, "not_connected");
    registry::register(&root, &definition(&q), "maintainer").unwrap();
    assert!(registry::select(&root, &selection(&q), false).is_err());
    assert!(registry::select(&root, &selection(&q), true).is_err());
    assert!(registry::active(&root, "code").is_err());
    assert!(registry::use_method(&root, "code", &json!({}), Path::new("var/use")).is_err());
    for operation in ["select", "use", "feedback", "recover"] {
        assert_eq!(
            oe_evolution::owner_connection(operation).unwrap().status,
            "not_connected"
        );
    }
    no_events(&root);
}

#[test]
fn old_authority_cannot_silently_enable_context() {
    let (_temp, root) = root();
    let q = fake(&root);
    let reference = json!({"path":"var/input/context.json","sha256":"a".repeat(64)});
    policy(
        &root,
        "oev2.controlled-version-authority/1",
        "code_artifact",
        Some(reference.clone()),
    );
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("explicit_policy2")
    );
    policy(
        &root,
        "oev2.controlled-version-authority/2",
        "operation_method",
        Some(reference),
    );
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("explicit_policy2")
    );
    no_events(&root);
}

#[test]
fn old_policy_null_context_retains_original_strict_field_protocol() {
    let (_temp, root) = root();
    let q = fake(&root);
    policy(
        &root,
        "oev2.controlled-version-authority/1",
        "code_artifact",
        Some(Value::Null),
    );
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("explicit_policy2")
    );
    policy(
        &root,
        "oev2.controlled-version-authority/2",
        "operation_method",
        Some(Value::Null),
    );
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("explicit_policy2")
    );
    no_events(&root);
}

#[test]
fn source_and_assurance_drift_fail_before_dispatch() {
    let (_temp, root) = root();
    let q = fake(&root);
    fs::write(root.path().join(&q.source.path), b"{}").unwrap();
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("artifact_source_changed")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
}

fn frozen_fixture() -> (PathBuf, SkillRoot, CodeArtifactEligibilityRequest) {
    let original = PathBuf::from(
        std::env::var_os("OE_CODE_QUALIFICATION_FIXTURE")
            .expect("explicit independent frozen fixture root is required"),
    );
    let (temp, root) = root();
    let input = root.path().join("var/input");
    fs::create_dir_all(&input).unwrap();
    for file in [
        "source.json",
        "comparison.json",
        "experiment.json",
        "protocol19.json",
        "context.json",
    ] {
        fs::copy(original.join("var/input").join(file), input.join(file)).unwrap();
    }
    let reference = artifacts::reference(&root, &input.join("context.json")).unwrap();
    policy(
        &root,
        "oev2.controlled-version-authority/2",
        "code_artifact",
        Some(serde_json::to_value(reference).unwrap()),
    );
    let q = request(&root);
    // Explicit frozen-artifact tests retain the full private inputs, forensic
    // copies and partial process evidence; their log records the actual root.
    let persisted = temp.keep();
    eprintln!(
        "retained code qualification fixture: {}",
        persisted.display()
    );
    (persisted, root, q)
}
fn set_context(root: &SkillRoot, mutate: impl FnOnce(&mut Value)) {
    let path = root.path().join("var/input/context.json");
    let mut context: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    mutate(&mut context);
    fs::write(&path, serde_json::to_vec(&context).unwrap()).unwrap();
    let reference = artifacts::reference(root, &path).unwrap();
    policy(
        root,
        "oev2.controlled-version-authority/2",
        "code_artifact",
        Some(serde_json::to_value(reference).unwrap()),
    );
}

// Each counterexample gets new private experiment bytes/identity. The original
// frozen six-case experiment, oracles, binaries and public source stay intact.
fn negative_input(
    root: &SkillRoot,
    id: &str,
    mutate: impl FnOnce(&mut Value, &mut Value),
) -> CodeArtifactEligibilityRequest {
    let dir = root.path().join("var/input");
    let mut source: Value =
        serde_json::from_slice(&fs::read(dir.join("source.json")).unwrap()).unwrap();
    let mut experiment: Value =
        serde_json::from_slice(&fs::read(dir.join("experiment.json")).unwrap()).unwrap();
    mutate(&mut source, &mut experiment);
    experiment["candidate"] = source.clone();
    experiment["experiment_id"] = json!(format!("owner-negative-{id}"));
    let mut assurance: Value =
        serde_json::from_slice(&fs::read(dir.join("comparison.json")).unwrap()).unwrap();
    assurance["experiment_id"] = experiment["experiment_id"].clone();
    for (name, value) in [
        ("source.json", &source),
        ("experiment.json", &experiment),
        ("comparison.json", &assurance),
    ] {
        fs::write(dir.join(name), serde_json::to_vec(value).unwrap()).unwrap();
    }
    let q = request(root);
    let experiment_ref = artifacts::reference(root, &dir.join("experiment.json")).unwrap();
    set_context(root, |c| {
        c["source"] = serde_json::to_value(&q.source).unwrap();
        c["assurance"] = serde_json::to_value(&q.assurance).unwrap();
        c["experiment"] = serde_json::to_value(experiment_ref).unwrap();
        c["experiment_sha256"] = json!(oe_evolution::digest(&experiment).unwrap());
        c["driver"] = experiment["driver"].clone();
        c["evaluator"] = experiment["evaluator"].clone();
    });
    q
}
fn copied_public(root: &SkillRoot, source: &Value) -> PathBuf {
    let original = Path::new(source["source_root"].as_str().unwrap());
    let manifest = oe_evolution::public_manifest(original).unwrap();
    let copy = root.path().join("var/forensic-public");
    fs::create_dir_all(&copy).unwrap();
    for logical in manifest["files"].as_object().unwrap().keys() {
        let target = copy.join(logical);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(original.join(logical), target).unwrap();
    }
    copy
}
fn writable_private_copy(path: &Path) {
    // fs::copy inherits immutable source modes. Only the private counterexample
    // copy gets owner write permission; the frozen original stays untouched.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path).unwrap().permissions().mode();
        fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o200)).unwrap();
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
    }
}
fn failed_driver_reason(root: &SkillRoot) -> String {
    let folder = fs::read_dir(root.path().join("var/code-qualification"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    String::from_utf8(fs::read(folder.join("fixed-driver/stderr")).unwrap()).unwrap()
}

#[test]
#[ignore = "requires explicitly frozen independent artifacts; mutate private copies only"]
fn real_binary_drift_and_third_runner_are_rejected() {
    let (_temp, root, _q) = frozen_fixture();
    let q = negative_input(&root, "binary-byte", |source, _experiment| {
        let path = root.path().join("var/changed-runner");
        fs::copy(source["runner"]["path"].as_str().unwrap(), &path).unwrap();
        writable_private_copy(&path);
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"changed")
            .unwrap();
        source["runner"]["path"] = json!(path);
    });
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("code_executable_changed")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
    let (_temp, root, _q) = frozen_fixture();
    let q = negative_input(&root, "third-runner", |source, experiment| {
        for field in ["path", "sha256", "bytes"] {
            source["runner"][field] = experiment["baseline"]["runner"][field].clone();
        }
    });
    let result = registry::code_eligibility(&root, &q).unwrap();
    assert_eq!(result.execution, "failed");
    assert_eq!(result.eligibility, "unknown");
    assert!(failed_driver_reason(&root).contains("code_runner_compiled_source_mismatch"));
    no_events(&root);
}

#[test]
#[ignore = "requires explicitly frozen independent artifacts; copies public assets without modifying frozen source"]
fn public_source_and_generation_receipt_drift_are_rejected() {
    let (_temp, root, _q) = frozen_fixture();
    let q = negative_input(&root, "public-source", |source, _experiment| {
        let copy = copied_public(&root, source);
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(copy.join("crates/semantica-runtime/src/graph.rs"))
            .unwrap()
            .write_all(b"\n// private drift\n")
            .unwrap();
        source["source_root"] = json!(copy);
        source["runner"]["source_root"] = source["source_root"].clone();
    });
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("code_binary_source_changed")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
    let (_temp, root, _q) = frozen_fixture();
    let q = negative_input(&root, "generation", |source, _experiment| {
        let copy = copied_public(&root, source);
        let path = copy.join("generated/generation.json");
        let mut receipt: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        receipt["private_drift"] = json!(true);
        fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        source["source_root"] = json!(copy);
        source["runner"]["source_root"] = source["source_root"].clone();
    });
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("code_artifact_generation_receipt_changed")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
}

#[test]
#[ignore = "requires explicitly frozen independent artifacts; only copied tools and oracle are mutated"]
fn frozen_tool_and_original_oracle_drift_are_rejected() {
    let (_temp, root, _q) = frozen_fixture();
    let q = negative_input(&root, "driver-byte", |_source, experiment| {
        let path = root.path().join("var/changed-driver");
        fs::copy(experiment["driver"]["path"].as_str().unwrap(), &path).unwrap();
        writable_private_copy(&path);
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"changed")
            .unwrap();
        experiment["driver"]["path"] = json!(path);
    });
    // The bound comparison also binds the original driver SHA, so a new path
    // cannot bypass actual byte verification by keeping that unchanged label.
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("code_executable_changed")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
    let (_temp, root, _q) = frozen_fixture();
    let q = negative_input(&root, "oracle-byte", |_source, experiment| {
        let path = root.path().join("var/changed-oracle.json");
        fs::copy(experiment["oracle_path"].as_str().unwrap(), &path).unwrap();
        writable_private_copy(&path);
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"\n")
            .unwrap();
        experiment["oracle_path"] = json!(path);
    });
    let result = registry::code_eligibility(&root, &q).unwrap();
    assert_eq!(result.execution, "failed");
    assert_eq!(result.eligibility, "unknown");
    assert!(failed_driver_reason(&root).contains("code_oracle_changed"));
    no_events(&root);
}

#[test]
#[ignore = "requires explicitly frozen independent before/after binaries, driver, evaluator and original oracles"]
fn frozen_external_replay_completes_but_full_release_stays_unknown() {
    let (_temp, root, q) = frozen_fixture();
    let source: oe_contracts::CodeSourceArtifact =
        serde_json::from_value(artifacts::json(&root, &q.source).unwrap()).unwrap();
    assert!(source.runner.bytes > 16 * 1024 * 1024);
    let result = registry::code_eligibility(&root, &q).unwrap();
    assert_eq!(result.execution, "completed");
    assert_eq!(result.eligibility, "unknown");
    assert_eq!(result.artifact_sha256, q.source.sha256);
    assert_eq!(result.comparison_sha256, q.assurance.sha256);
    assert!(result.uncovered.iter().any(|x| x.contains("13494")));
    registry::register(&root, &definition(&q), "maintainer").unwrap();
    assert!(
        registry::select(&root, &selection(&q), false)
            .unwrap_err()
            .to_string()
            .contains("not_connected")
    );
    no_events(&root);
}

fn bind_full_scope(root: &SkillRoot) {
    let context: Value =
        serde_json::from_slice(&fs::read(root.path().join("var/input/context.json")).unwrap())
            .unwrap();
    let mut items = oe_evolution::scope_witness::unknown_bindings(Path::new(
        context["driver"]["source_root"].as_str().unwrap(),
    ))
    .unwrap();
    for item in &mut items {
        item.case_ids = match item.scope_id.as_str() {
            "semantica/reasoning/datalog_reasoner.py|method|DatalogReasoner.query|344" => vec![
                "datalog-recursive-multivar".into(),
                "datalog-repeated-variable".into(),
            ],
            "semantica/ontology/runtime.py|method|SemanticRuntime.validate_shacl|1831" => {
                vec!["shacl-missing-required-property".into()]
            }
            _ => vec![],
        };
    }
    let request = oe_contracts::CodeScopeWitnessRequest {
        schema: "oev2.code-scope-witness-request/1".into(),
        experiment_sha256: context["experiment_sha256"].as_str().unwrap().into(),
        dispatch_budget: oe_contracts::CodeBudget {
            timeout_ms: 550000,
            output_bytes: 4194304,
        },
        items,
    };
    let path = root.path().join("var/input/full-scope-request.json");
    write_new(&path, &serde_json::to_value(request).unwrap()).unwrap();
    let reference = artifacts::reference(root, &path).unwrap();
    set_context(root, |value| {
        *value = json!({"schema":"oev2.code-qualification-context/2", "qualification":value.clone(),
            "full_scope_request":reference});
    });
}

#[test]
#[ignore = "requires original frozen independent six-case artifacts; executed explicitly with source-bound processes"]
fn full_scope_owner_dispatch_retains_all_ids_and_cannot_activate() {
    let (_temp, root, q) = frozen_fixture();
    bind_full_scope(&root);
    let result = registry::code_eligibility(&root, &q).unwrap();
    assert_eq!(result.execution, "completed");
    assert_eq!(result.eligibility, "unknown");
    let folder = fs::read_dir(root.path().join("var/code-qualification"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let report: oe_contracts::CodeScopeWitnessReport =
        serde_json::from_slice(&fs::read(folder.join("scope-witness-report.json")).unwrap())
            .unwrap();
    assert_eq!(report.fixed_scope_count, 13494);
    assert_eq!(report.items.len(), 13494);
    assert_eq!(
        report
            .items
            .iter()
            .map(|r| &r.scope_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        13494
    );
    assert!(report.items.iter().all(|r| r.behavior_status == "unknown"));
    assert_eq!(
        report
            .items
            .iter()
            .map(|r| r.observed_case_ids.len())
            .sum::<usize>(),
        3
    );
    assert_eq!(report.eligibility, "unknown");
    let observation: Value =
        serde_json::from_slice(&fs::read(folder.join("replay-observation.json")).unwrap()).unwrap();
    assert_eq!(observation["process"]["arguments"][0], "scope-witness");
    assert_eq!(observation["process"]["status"], "completed");
    assert_eq!(observation["process"]["exit_code"], 0);
    let reference: oe_contracts::ArtifactReference =
        serde_json::from_value(observation["scope_witness_report"].clone()).unwrap();
    assert_eq!(
        artifacts::json(&root, &reference).unwrap(),
        serde_json::to_value(&report).unwrap()
    );
    registry::register(&root, &definition(&q), "maintainer").unwrap();
    assert!(registry::select(&root, &selection(&q), false).is_err());
    assert!(registry::select(&root, &selection(&q), true).is_err());
    assert!(registry::active(&root, "code").is_err());
    no_events(&root);
}

#[test]
#[ignore = "requires original frozen artifacts; mutated request is a private counterexample only"]
fn full_scope_request_drift_and_missing_id_fail_before_process() {
    let (_temp, root, q) = frozen_fixture();
    bind_full_scope(&root);
    let path = root.path().join("var/input/full-scope-request.json");
    fs::write(&path, b"{}").unwrap();
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("artifact_source_changed")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
    let (_temp, root, q) = frozen_fixture();
    bind_full_scope(&root);
    let path = root.path().join("var/input/full-scope-request.json");
    let mut request: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    request["items"].as_array_mut().unwrap().pop();
    fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
    let reference = artifacts::reference(&root, &path).unwrap();
    set_context(&root, |c| {
        c["full_scope_request"] = serde_json::to_value(reference).unwrap()
    });
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("missing_scope_id")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
}

#[test]
#[ignore = "requires explicitly frozen independent artifacts; outer cancellation must retain failure/unknown"]
fn timeout_preserves_partial_evidence_and_unknown() {
    let (_temp, root, q) = frozen_fixture();
    set_context(&root, |c| c["dispatch_budget"]["timeout_ms"] = json!(1));
    let result = registry::code_eligibility(&root, &q).unwrap();
    assert_eq!(result.execution, "failed");
    assert_eq!(result.eligibility, "unknown");
    let folder = fs::read_dir(root.path().join("var/code-qualification"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let receipt: Value =
        serde_json::from_slice(&fs::read(folder.join("replay-observation.json")).unwrap()).unwrap();
    assert_eq!(receipt["process"]["status"], "timed_out");
    assert!(receipt["comparison"].is_null());
    no_events(&root);
}

#[test]
#[ignore = "requires explicitly frozen independent metadata; no production runner mutation"]
fn rebound_wrong_context_and_protocol_reject_before_process() {
    let (_temp, root, q) = frozen_fixture();
    set_context(&root, |c| c["revision"] = json!("unselected"));
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("context_binding_changed")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    let (_temp, root, q) = frozen_fixture();
    fs::write(
        root.path().join("var/input/protocol19.json"),
        b"{\"cases\":[]}",
    )
    .unwrap();
    set_context(&root, |c| {
        c["required_protocol"]["sha256"] = json!(sha256(b"{\"cases\":[]}"))
    });
    assert!(
        registry::code_eligibility(&root, &q)
            .unwrap_err()
            .to_string()
            .contains("protocol_changed")
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
}

#[test]
fn association_missing_context_never_imports_claimed_witness() {
    use oe_contracts::CodeScopeAssociationPort;
    let (_temp, root) = root();
    let q = fake(&root);
    let actual = registry::CodeArtifactEligibilityExecutor { root: &root }
        .associate(&q)
        .unwrap();
    assert_eq!(actual.execution, "not_connected");
    assert!(actual.index.is_none());
    assert_eq!(actual.qualification.eligibility, "not_connected");
    no_events(&root);
}

#[test]
#[ignore = "requires original frozen independent six-case artifacts; no pasted receipt is execution"]
fn actual_scope_association_keeps_original_ids_and_owner_roles() {
    use oe_contracts::CodeScopeAssociationPort;
    use std::collections::BTreeSet;
    let (_temp, root, q) = frozen_fixture();
    bind_full_scope(&root);
    let source: oe_contracts::CodeSourceArtifact =
        serde_json::from_value(artifacts::json(&root, &q.source).unwrap()).unwrap();
    let candidate = Path::new(&source.source_root);
    // Independent oracle from original scope bytes, not from the join/audit
    // function under test or from the produced index's IDs.
    let scope_raw = fs::read(candidate.join("runtime/semantica-scope.json")).unwrap();
    assert_eq!(sha256(&scope_raw), oe_evolution::scope_witness::SCOPE_SHA);
    let scope: Value = serde_json::from_slice(&scope_raw).unwrap();
    let mut expected_ids = BTreeSet::new();
    for (section, kind) in [
        ("tracked_files", "tracked_file"),
        ("empty_files", "original_empty_file"),
        ("package_python", "package_source"),
        ("outside_python", "outside_source"),
        ("frontend_code", "frontend_source"),
        ("test_files", "original_test_file"),
        ("python_test_sources", "original_python_test_source"),
    ] {
        for row in scope[section].as_array().unwrap() {
            assert!(expected_ids.insert(format!("{}|{kind}||0", row["path"].as_str().unwrap())));
        }
    }
    for section in [
        "public_symbols",
        "outside_symbols",
        "python_exports",
        "frontend_exports",
        "routes",
        "original_test_symbols",
        "placeholder_candidates",
        "empty_modules",
        "console_entrypoints",
    ] {
        for row in scope[section].as_array().unwrap() {
            assert!(expected_ids.insert(row["id"].as_str().unwrap().to_owned()));
        }
    }
    assert_eq!(expected_ids.len(), 13494);
    let mappings_raw =
        fs::read(candidate.join("runtime/semantica-migration-mappings.json")).unwrap();
    let mappings: Value = serde_json::from_slice(&mappings_raw).unwrap();
    let mappings_sha = sha256(&mappings_raw);
    assert_eq!(
        source.manifest["files"]["runtime/semantica-migration-mappings.json"]["sha256"],
        mappings_sha
    );
    let mut expected_implementations = Vec::new();
    for claim in mappings["claims"].as_array().unwrap() {
        assert_eq!(claim["status"], "declared");
        let path = claim["rust_path"].as_str().unwrap();
        let digest = source.manifest["files"]
            .get(path)
            .map(|value| value["sha256"].as_str().unwrap().to_owned());
        if let Some(digest) = &digest {
            assert_eq!(&sha256(fs::read(candidate.join(path)).unwrap()), digest);
        }
        for id in claim["scope_ids"].as_array().unwrap() {
            assert!(expected_ids.contains(id.as_str().unwrap()));
            expected_implementations.push(json!({
                "scope_id":id, "module":claim["module"], "rust_path":path,
                "source_sha256":digest,
                "resolution":if digest.is_some() { "declared_source_matched" } else { "declared_source_missing" },
            }));
        }
    }
    expected_implementations.sort_by(|a, b| {
        (
            a["scope_id"].as_str(),
            a["module"].as_str(),
            a["rust_path"].as_str(),
        )
            .cmp(&(
                b["scope_id"].as_str(),
                b["module"].as_str(),
                b["rust_path"].as_str(),
            ))
    });
    // This explicit retained fixture has 62 actual relations; empty output
    // cannot satisfy the source-association assertions vacuously.
    assert_eq!(expected_implementations.len(), 62);
    let result = registry::CodeArtifactEligibilityExecutor { root: &root }
        .associate(&q)
        .unwrap();
    assert_eq!(result.execution, "completed");
    assert_eq!(result.qualification.eligibility, "unknown");
    let folders: Vec<_> = fs::read_dir(root.path().join("var/code-qualification"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(
        folders.len(),
        1,
        "one port invocation must dispatch only once"
    );
    let disk_receipt: Value = serde_json::from_slice(
        &fs::read(folders[0].join("scope-association-receipt.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(disk_receipt, serde_json::to_value(&result).unwrap());
    let reference = result.index.as_ref().unwrap();
    let index: oe_contracts::CodeScopeAssociationIndex =
        serde_json::from_value(artifacts::json(&root, reference).unwrap()).unwrap();
    assert_eq!(index.request, q);
    assert_eq!(index.fixed_scope_count, 13494);
    assert_eq!(index.items.len(), 13494);
    let actual_ids: BTreeSet<_> = index.items.iter().map(|x| x.scope_id.clone()).collect();
    assert_eq!(actual_ids, expected_ids);
    assert!(index.items.iter().all(|x| x.behavior_status == "unknown"));
    assert_eq!(
        index
            .items
            .iter()
            .map(|x| x.observed_case_ids.len())
            .sum::<usize>(),
        3
    );
    assert_eq!(
        index.candidate_mappings_sha256.as_deref(),
        Some(mappings_sha.as_str())
    );
    assert_eq!(
        serde_json::to_value(&index.implementations).unwrap(),
        Value::Array(expected_implementations)
    );
    let witness: oe_contracts::CodeScopeWitnessReport =
        serde_json::from_value(artifacts::json(&root, &index.witness).unwrap()).unwrap();
    assert_eq!(index.candidate_manifest_sha256, source.manifest_sha256);
    assert_eq!(witness.witness.candidate_sha256, source.manifest_sha256);
    // Candidate and driver of this retained positive fixture have equal public
    // manifests. It proves owner-relative evidence, but is not a discriminating
    // execution test of candidate-vs-driver source-root confusion.
    registry::register(&root, &definition(&q), "maintainer").unwrap();
    assert!(registry::select(&root, &selection(&q), false).is_err());
    assert!(registry::select(&root, &selection(&q), true).is_err());
    no_events(&root);
}

#[test]
#[ignore = "requires original frozen descriptors; changes only the private owner input copy"]
fn association_requires_full_scope_and_rejects_changed_descriptor_before_dispatch() {
    use oe_contracts::CodeScopeAssociationPort;
    let (_temp, root, q) = frozen_fixture();
    let error = registry::CodeArtifactEligibilityExecutor { root: &root }
        .associate(&q)
        .unwrap_err();
    assert!(format!("{error:?}").contains("scope_association_full_scope_context_required"));
    assert!(!root.path().join("var/code-qualification").exists());
    bind_full_scope(&root);
    fs::write(
        root.path().join(&q.source.path),
        b"{\"passed\":true,\"eligibility\":\"eligible\"}",
    )
    .unwrap();
    assert!(
        registry::CodeArtifactEligibilityExecutor { root: &root }
            .associate(&q)
            .is_err()
    );
    assert!(!root.path().join("var/code-qualification").exists());
    no_events(&root);
}
