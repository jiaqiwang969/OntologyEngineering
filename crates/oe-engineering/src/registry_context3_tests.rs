//! These tests exercise owned input resolution and refusal using real files.
//! The structural descriptors are deliberately not executable qualification
//! evidence. No test certifies whole-code eligibility or an adopted A/B/A cycle.
use super::*;
use oe_contracts::{CodeArtifactEligibilityRequest, CodeRevisionQualificationContext3};
use std::fs;

struct Fixture {
    _temp: tempfile::TempDir,
    root: SkillRoot,
    requests: [CodeArtifactEligibilityRequest; 2],
    contexts: Value,
}

impl Fixture {
    fn new() -> Self {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        fs::create_dir_all(workspace.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(workspace.join("var/builds")).unwrap();
        fs::write(
            temp.path().join("SKILL.md"),
            "private context3 input fixture",
        )
        .unwrap();
        let root = SkillRoot::new(temp.path()).unwrap();
        fs::create_dir_all(root.path().join("var/input")).unwrap();
        let requests = ["A", "B"].map(|revision| {
            let source = root
                .path()
                .join(format!("var/input/source-{revision}.json"));
            let assurance = root
                .path()
                .join(format!("var/input/assurance-{revision}.json"));
            write_new(&source, &json!({"revision":revision,"eligible":true})).unwrap();
            write_new(&assurance, &json!({"revision":revision,"passed":true})).unwrap();
            CodeArtifactEligibilityRequest {
                schema: "oev2.code-artifact-eligibility-request/1".into(),
                binding_id: "code".into(),
                revision: revision.into(),
                scope_id: "semantica.full-library/1".into(),
                source: artifacts::reference(&root, &source).unwrap(),
                assurance: artifacts::reference(&root, &assurance).unwrap(),
            }
        });
        let contexts = json!({
            "schema":"oev2.code-qualification-context/3",
            "binding_id":"code", "scope_id":"semantica.full-library/1",
            "revisions": requests.iter().map(leaf).collect::<Vec<_>>()
        });
        let result = Self {
            _temp: temp,
            root,
            requests,
            contexts,
        };
        result.install(&result.contexts, "oev2.controlled-version-authority/2");
        result
    }

    fn install(&self, contexts: &Value, policy_schema: &str) {
        let path = self.root.path().join("var/input/context.json");
        fs::write(&path, serde_json::to_vec(contexts).unwrap()).unwrap();
        let reference = artifacts::reference(&self.root, &path).unwrap();
        let policy = self.root.path().join(POLICY);
        fs::create_dir_all(policy.parent().unwrap()).unwrap();
        fs::write(
            &policy,
            serde_json::to_vec(&json!({
                "schema":policy_schema, "bindings":{"code":{
                    "kind":"code_artifact", "scope_id":"semantica.full-library/1",
                    "actor_scopes":{"maintainer":["register","activate","restore"]},
                    "code_qualification":reference
                }}
            }))
            .unwrap(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(policy, fs::Permissions::from_mode(0o600)).unwrap();
        }
    }

    fn no_execution_or_events(&self) {
        for path in [
            "var/code-qualification",
            "var/code-owner-replays",
            "var/state/knowledge/experience-events",
        ] {
            assert!(!self.root.path().join(path).exists(), "unexpected {path}");
        }
        assert!(!base(&self.root, "code").unwrap().join("events").exists());
    }
}

fn leaf(request: &CodeArtifactEligibilityRequest) -> Value {
    let reference = json!({"path":"var/input/not-executed.json", "sha256":"a".repeat(64)});
    let binary = json!({"path":format!("/not-executed/{}",request.revision),
        "sha256":"b".repeat(64), "bytes":1,
        "source_root":"/not-executed/source", "source_manifest_sha256":"c".repeat(64)});
    json!({"schema":"oev2.code-qualification-context/2",
    "full_scope_request":{"path":format!("var/input/scope-{}.json",request.revision),"sha256":"e".repeat(64)},
    "qualification":{
        "schema":"oev2.code-qualification-context/1", "binding_id":request.binding_id,
        "revision":request.revision, "scope_id":request.scope_id, "purpose":"full_code_release",
        "source":request.source, "assurance":request.assurance,
        "experiment":reference, "experiment_sha256":"d".repeat(64),
        "driver":binary, "evaluator":binary, "required_protocol":reference,
        "dispatch_budget":{"timeout_ms":600000,"output_bytes":4194304}
    }})
}

fn denied(f: &Fixture, request: &CodeArtifactEligibilityRequest) -> String {
    let error = match qualification_context(&f.root, request) {
        Ok(_) => panic!("invalid context accepted"),
        Err(error) => error.to_string(),
    };
    f.no_execution_or_events();
    error
}

#[test]
fn context3_resolves_a_b_a_under_one_immutable_policy() {
    for schema in [
        "oev2.controlled-version-authority/2",
        "oev2.controlled-version-authority/3",
    ] {
        let f = Fixture::new();
        f.install(&f.contexts, schema);
        let policy_before = fs::read(f.root.path().join(POLICY)).unwrap();
        let set_before = fs::read(f.root.path().join("var/input/context.json")).unwrap();
        let mut resolutions = Vec::new();
        for index in [0, 1, 0] {
            let (owned, policy_sha, reference) = qualification_context(&f.root, &f.requests[index])
                .unwrap()
                .unwrap();
            assert_eq!(
                serde_json::to_value(&owned.qualification).unwrap(),
                f.contexts["revisions"][index]["qualification"]
            );
            assert_eq!(
                serde_json::to_value(&owned.full_scope_request).unwrap(),
                f.contexts["revisions"][index]["full_scope_request"]
            );
            assert_eq!(policy_sha, sha256(&policy_before));
            assert_eq!(reference.sha256, sha256(&set_before));
            assert_eq!(fs::read(f.root.path().join(POLICY)).unwrap(), policy_before);
            assert_eq!(
                fs::read(f.root.path().join("var/input/context.json")).unwrap(),
                set_before
            );
            resolutions.push(owned);
        }
        assert!(resolutions[0] == resolutions[2]);
        assert!(resolutions[0] != resolutions[1]);
        f.no_execution_or_events();
    }
}

#[test]
fn context3_rejects_missing_duplicate_and_empty_revisions() {
    let f = Fixture::new();
    let mut absent = f.requests[0].clone();
    absent.revision = "C".into();
    assert!(denied(&f, &absent).contains("revision_not_registered"));
    for replacement in [
        json!([]),
        json!([f.contexts["revisions"][0], f.contexts["revisions"][0]]),
    ] {
        let mut contexts = f.contexts.clone();
        contexts["revisions"] = replacement;
        f.install(&contexts, "oev2.controlled-version-authority/2");
        denied(&f, &f.requests[0]);
    }
    let mut contexts = f.contexts.clone();
    contexts["revisions"][1]["qualification"]["revision"] = json!(" ");
    f.install(&contexts, "oev2.controlled-version-authority/2");
    denied(&f, &f.requests[0]);
}

#[test]
fn context3_rejects_unselected_owner_scope_purpose_and_schema() {
    let f = Fixture::new();
    for (field, value) in [
        ("binding_id", "other-owner"),
        ("scope_id", "reduced-scope"),
        ("purpose", "local_trial"),
        ("schema", "oev2.code-qualification-context/2"),
    ] {
        let mut contexts = f.contexts.clone();
        contexts["revisions"][1]["qualification"][field] = json!(value);
        f.install(&contexts, "oev2.controlled-version-authority/2");
        denied(&f, &f.requests[0]);
    }
    for field in ["binding_id", "scope_id"] {
        let mut contexts = f.contexts.clone();
        contexts[field] = json!("other");
        f.install(&contexts, "oev2.controlled-version-authority/2");
        assert!(denied(&f, &f.requests[0]).contains("revision_set_binding_changed"));
    }
    let mut contexts = f.contexts.clone();
    contexts["revisions"][1]["qualification"]["source"]["sha256"] = json!("invalid");
    f.install(&contexts, "oev2.controlled-version-authority/2");
    denied(&f, &f.requests[0]);
}

#[test]
fn context3_requires_complete_original_leaves_and_strict_wire() {
    let f = Fixture::new();
    let decoded: CodeRevisionQualificationContext3 =
        serde_json::from_value(f.contexts.clone()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), f.contexts);
    for mode in 0..7 {
        let mut contexts = f.contexts.clone();
        match mode {
            0 => {
                contexts["revisions"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("full_scope_request");
            }
            1 => contexts["revisions"][1]["full_scope_request"] = Value::Null,
            2 => contexts["revisions"][1]["schema"] = json!("oev2.code-qualification-context/1"),
            3 => contexts["revisions"][1] = f.contexts.clone(),
            4 => contexts["eligible"] = json!(true),
            5 => contexts["revisions"][1]["qualification"]["eligible"] = json!(true),
            6 => contexts["revisions"] = Value::Null,
            _ => unreachable!(),
        }
        f.install(&contexts, "oev2.controlled-version-authority/2");
        denied(&f, &f.requests[0]);
    }
}

#[test]
fn context3_selected_identity_cannot_be_substituted() {
    let f = Fixture::new();
    for source in [true, false] {
        let mut request = f.requests[0].clone();
        if source {
            request.source = f.requests[1].source.clone();
        } else {
            request.assurance = f.requests[1].assurance.clone();
        }
        assert!(denied(&f, &request).contains("context_binding_changed"));
    }
    // The selection is driven by the exact revision, not map position or a fallback.
    let mut contexts = f.contexts.clone();
    contexts["revisions"].as_array_mut().unwrap().reverse();
    f.install(&contexts, "oev2.controlled-version-authority/2");
    let (owned, _, _) = qualification_context(&f.root, &f.requests[0])
        .unwrap()
        .unwrap();
    assert_eq!(owned.qualification.revision, "A");
    assert_eq!(owned.qualification.source, f.requests[0].source);
    f.no_execution_or_events();
}

#[test]
fn context3_frozen_set_and_selected_source_drift_are_rejected() {
    let f = Fixture::new();
    let policy_before = fs::read(f.root.path().join(POLICY)).unwrap();
    let path = f.root.path().join("var/input/context.json");
    let mut changed = fs::read(&path).unwrap();
    changed.push(b'\n');
    fs::write(&path, changed).unwrap();
    assert!(denied(&f, &f.requests[0]).contains("artifact_source_changed"));
    assert_eq!(fs::read(f.root.path().join(POLICY)).unwrap(), policy_before);

    let f = Fixture::new();
    fs::write(f.root.path().join(&f.requests[0].source.path), b"{}").unwrap();
    assert!(
        code_eligibility(&f.root, &f.requests[0])
            .unwrap_err()
            .to_string()
            .contains("artifact_source_changed")
    );
    f.no_execution_or_events();
}

#[test]
fn context3_reads_only_selected_actual_inputs_after_structural_validation() {
    let f = Fixture::new();
    // Unselected source files may be unavailable during A's resolution. Their
    // declarations are still structurally validated and frozen in the map.
    fs::remove_file(f.root.path().join(&f.requests[1].source.path)).unwrap();
    fs::remove_file(f.root.path().join(&f.requests[1].assurance.path)).unwrap();
    let (owned, _, _) = qualification_context(&f.root, &f.requests[0])
        .unwrap()
        .unwrap();
    assert_eq!(owned.qualification.source, f.requests[0].source);
    // Choosing B must then go through the original actual-byte reader and fail.
    assert!(code_eligibility(&f.root, &f.requests[1]).is_err());
    f.no_execution_or_events();
}

#[test]
fn context3_mapping_never_grants_activation_or_restore() {
    let f = Fixture::new();
    let policy_before = fs::read(f.root.path().join(POLICY)).unwrap();
    for request in &f.requests {
        assert!(qualification_context(&f.root, request).unwrap().is_some());
        let definition = ControlledVersionDefinition {
            schema: "oev2.controlled-version-definition/1".into(),
            binding_id: request.binding_id.clone(),
            revision: request.revision.clone(),
            kind: "code_artifact".into(),
            scope_id: request.scope_id.clone(),
            source: request.source.clone(),
            assurance: request.assurance.clone(),
        };
        register(&f.root, &definition, "maintainer").unwrap();
        let path = definition_path(&f.root, &request.binding_id, &request.revision).unwrap();
        let definition_before = fs::read(&path).unwrap();
        assert!(code_eligibility(&f.root, request).is_err());
        let selection = VersionSelectionRequest {
            schema: "oev2.version-selection-request/1".into(),
            actor: "maintainer".into(),
            binding_id: request.binding_id.clone(),
            revision: request.revision.clone(),
            reason: "mapping is not qualification".into(),
            expected_head_sha256: None,
        };
        for restore in [false, true] {
            assert!(select(&f.root, &selection, restore).is_err());
        }
        assert_eq!(fs::read(path).unwrap(), definition_before);
    }
    assert!(active(&f.root, "code").is_err());
    assert_eq!(fs::read(f.root.path().join(POLICY)).unwrap(), policy_before);
    f.no_execution_or_events();
}

#[test]
fn context3_preserves_old_context1_context2_and_policy1_boundaries() {
    let f = Fixture::new();
    let original2 = f.contexts["revisions"][0].clone();
    let original1 = original2["qualification"].clone();
    for original in [&original1, &original2] {
        f.install(original, "oev2.controlled-version-authority/2");
        let (owned, _, _) = qualification_context(&f.root, &f.requests[0])
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(owned.qualification).unwrap(),
            original1
        );
        assert_eq!(
            owned.full_scope_request.is_some(),
            original["schema"] == "oev2.code-qualification-context/2"
        );
        let mut extra = original.clone();
        extra["revisions"] = Value::Null;
        f.install(&extra, "oev2.controlled-version-authority/2");
        denied(&f, &f.requests[0]);
    }
    let mut extra = original1.clone();
    extra["full_scope_request"] = Value::Null;
    f.install(&extra, "oev2.controlled-version-authority/2");
    assert!(denied(&f, &f.requests[0]).contains("scope_request_requires_context2"));
    let mut missing = original2.clone();
    missing["full_scope_request"] = Value::Null;
    f.install(&missing, "oev2.controlled-version-authority/2");
    assert!(denied(&f, &f.requests[0]).contains("full_scope_request_required"));
    f.install(&f.contexts, "oev2.controlled-version-authority/1");
    assert!(denied(&f, &f.requests[0]).contains("requires_explicit_policy2"));
    f.no_execution_or_events();
}
