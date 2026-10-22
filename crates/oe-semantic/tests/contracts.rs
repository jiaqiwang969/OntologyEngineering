use oe_core::{json::canonical_json, sha256};
use oe_semantic::contracts::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

fn binding_document() -> Value {
    json!({"$schema":BINDING_SCHEMA,"binding_id":"binding:test","project":{"project_id":"project:test","domain":"manufacturing"},
        "semantic_target":{"kind":"workspace","package_id":"method-test","workspace_id":"workspace:test"},
        "baseline":{"version":"1.0.0","digest":"a".repeat(64)},"evidence":{"logical_root":"evidence:project-test"},
        "authority":{"fact":{"authority_id":"authority:engineer","scope":["observations","measurements"]},
                     "decision":{"authority_id":"authority:board","scope":["promotion","candidate-verdict"]}},
        "allowed_actions":KNOWN_ACTIONS,"lifecycle_actions":STATE_MODEL,
        "promotion":{"target":PROMOTION_TARGET,"requires_decision_authority":true},"created_at":"2026-09-27T10:00:00Z","semantic_api":NATIVE_REFINERY_CONTRACT})
}
fn parse(value: &Value) -> anyhow::Result<ProjectBinding> {
    parse_project_binding(&canonical_json(value).unwrap())
}
fn task_document(actions: &[&str]) -> Value {
    json!({"$schema":TASK_SCHEMA,"task_id":"task:test","task_kind":"supplier-vibration-investigation-v7","intent":"Assess a source-bound observation", "project":"project:test","domain":"manufacturing",
        "requested_decision":"Decide the next test","evidence":[{"source_id":"source:test","uri":"evidence:project-test/observation","sha256":"b".repeat(64),"media_type":"application/json","captured_at":"2026-09-27T10:00:00Z"}],
        "requested_actions":actions,"actor_id":"actor:engineer","required_capabilities":["z:capability","a:capability"],"created_at":"2026-09-27T11:00:00+01:00"})
}
fn task(value: &Value, binding: &ProjectBinding) -> anyhow::Result<SemanticTaskEnvelope> {
    parse_task_envelope(&canonical_json(value).unwrap(), binding)
}

#[test]
fn normalized_contract_preserves_exact_input_identity() {
    let value = binding_document();
    let compact = canonical_json(&value).unwrap();
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    let first = parse_project_binding(&compact).unwrap();
    let second = parse_project_binding(&pretty).unwrap();
    assert_eq!(first.source_sha256, sha256(&compact));
    assert_ne!(first.source_sha256, second.source_sha256);
    assert_eq!(first.document(), second.document());
    assert_eq!(
        first.fact_authority["scope"],
        json!(["measurements", "observations"])
    );
    assert_eq!(
        parse(&first.document()).unwrap().document(),
        first.document()
    );
    assert!(first.as_dict().get("$schema").is_none());
    assert_eq!(first.as_dict()["source_sha256"], first.source_sha256);
}
#[test]
fn binding_rejects_unknown_nested_fields_and_runtime_selection() {
    for pointer in [
        "/extra",
        "/authority/fact/backend",
        "/semantic_target/fallback",
        "/baseline/source",
        "/promotion/auto_apply",
    ] {
        let mut value = binding_document();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        value
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), json!("injected"));
        assert!(parse(&value).is_err(), "accepted {pointer}");
    }
    let raw = canonical_json(&binding_document()).unwrap();
    let mut duplicate = String::from_utf8(raw).unwrap();
    duplicate.insert_str(1, "\"binding_id\":\"shadow\",");
    assert!(parse_project_binding(duplicate.as_bytes()).is_err());
    assert!(parse_project_binding(b"[]").is_err());
}
#[test]
fn workspace_lifecycle_cannot_skip_or_reorder_authority() {
    for actions in [
        json!([]),
        json!(["proposed"]),
        json!(["candidate", "committed"]),
        json!(["candidate", "candidate"]),
    ] {
        let mut value = binding_document();
        value["lifecycle_actions"] = actions;
        assert!(parse(&value).is_err());
    }
    let mut value = binding_document();
    value["allowed_actions"] = json!(["doctor", "propose"]);
    assert!(parse(&value).is_err());
    value["lifecycle_actions"] = json!(["candidate", "proposed"]);
    assert!(parse(&value).is_ok());
}
#[test]
fn package_binding_is_read_only_and_version_bound() {
    let mut value = binding_document();
    value["semantic_target"] =
        json!({"kind":"package","package_id":"method-test","package_version":"1.0.0"});
    value["lifecycle_actions"] = json!([]);
    value["allowed_actions"] = json!(["run", "verify", "doctor", "discover"]);
    let binding = parse(&value).unwrap();
    assert!(native_binding(&binding).is_err());
    assert!(authorize(&binding, "run").is_ok());
    assert!(authorize(&binding, "propose").is_err());
    value["allowed_actions"] = json!(["run", "promote"]);
    assert!(parse(&value).is_err());
    value["allowed_actions"] = json!(["run"]);
    value["semantic_target"]["package_version"] = json!("2.0.0");
    assert!(parse(&value).is_err());
}
#[test]
fn authority_normalization_never_drops_scope_identity() {
    let mut value = binding_document();
    value["authority"]["fact"]["scope"] = json!([" measurements ", "observations"]);
    let first = parse(&value).unwrap();
    let token = authority_token(&first.fact_authority).unwrap();
    assert_eq!(
        token,
        "{\"authority_id\":\"authority:engineer\",\"scope\":[\"measurements\",\"observations\"]}"
    );
    value["authority"]["fact"]["scope"] = json!(["measurements"]);
    assert_ne!(
        token,
        authority_token(&parse(&value).unwrap().fact_authority).unwrap()
    );
    value["authority"]["fact"]["scope"] = json!(["measurements", " measurements "]);
    assert!(parse(&value).is_err());
}
#[test]
fn task_keeps_open_provenance_kind_and_raw_identity() {
    let binding = parse(&binding_document()).unwrap();
    let value = task_document(&["verify", "run"]);
    let raw = canonical_json(&value).unwrap();
    let task = parse_task_envelope(&raw, &binding).unwrap();
    assert_eq!(task.task_kind, "supplier-vibration-investigation-v7");
    assert_eq!(task.source_sha256, sha256(&raw));
    assert_eq!(task.requested_actions, vec!["run", "verify"]);
    assert_eq!(
        task.required_capabilities,
        vec!["a:capability", "z:capability"]
    );
    assert_eq!(task.document()["evidence"], value["evidence"]);
}
#[test]
fn task_evidence_cannot_escape_root_or_alias_source_identity() {
    let binding = parse(&binding_document()).unwrap();
    for uri in [
        "evidence:project-test-sibling/x",
        "file:///tmp/x",
        "https://example.test/x",
    ] {
        let mut value = task_document(&["run"]);
        value["evidence"][0]["uri"] = json!(uri);
        assert!(task(&value, &binding).is_err());
    }
    for field in ["source_id", "uri"] {
        let mut value = task_document(&["run"]);
        let mut extra = value["evidence"][0].clone();
        extra["source_id"] = json!("source:second");
        extra["uri"] = json!("evidence:project-test/second");
        extra[field] = value["evidence"][0][field].clone();
        value["evidence"].as_array_mut().unwrap().push(extra);
        assert!(task(&value, &binding).is_err());
    }
    let mut value = task_document(&["run"]);
    value["evidence"] = json!([]);
    assert!(task(&value, &binding).is_err());
}
#[test]
fn current_command_requires_both_binding_and_task() {
    let mut value = binding_document();
    value["lifecycle_actions"] = json!(["candidate", "proposed"]);
    value["allowed_actions"] = json!(["propose", "run"]);
    let binding = parse(&value).unwrap();
    assert!(task(&task_document(&["commit"]), &binding).is_err());
    let task = task(&task_document(&["propose"]), &binding).unwrap();
    authorize_task(&binding, &task, "propose").unwrap();
    let err = authorize_task(&binding, &task, "run").unwrap_err();
    assert_eq!(
        err.downcast_ref::<ContractError>().unwrap().kind,
        ContractErrorKind::ActionNotAllowedError
    );
    assert!(authorize_task(&binding, &task, "commit").is_err());
    assert!(authorize(&binding, "fallback").is_err());
}
#[test]
fn single_native_action_prevents_replaying_an_earlier_task() {
    let binding = parse(&binding_document()).unwrap();
    let candidate = task(&task_document(&["propose"]), &binding).unwrap();
    assert_eq!(
        native_task(&candidate, "candidate").unwrap()["requested_actions"],
        json!(["candidate"])
    );
    assert_eq!(
        native_task(&candidate, "proposed").unwrap()["requested_actions"],
        json!(["proposed"])
    );
    for action in [
        "committed",
        "promoted",
        "execute_candidate",
        "derive_regression_gate",
        "release_complete",
        "engagement",
        "unknown",
    ] {
        assert!(
            native_task(&candidate, action).is_err(),
            "accepted {action}"
        );
    }
    let current = task(
        &task_document(&["open", "propose", "commit", "verify", "promote"]),
        &binding,
    )
    .unwrap();
    for action in [
        "engagement",
        "candidate",
        "proposed",
        "committed",
        "execute_candidate",
        "derive_regression_gate",
        "regression_passed",
        "derive_release_gate",
        "release_complete",
        "promoted",
    ] {
        assert_eq!(
            native_task(&current, action).unwrap()["requested_actions"],
            json!([action])
        );
    }
}
#[test]
fn native_binding_matches_existing_review_projection() {
    let value = binding_document();
    let binding = parse(&value).unwrap();
    assert_eq!(
        native_binding(&binding).unwrap(),
        oe_semantic::review::validate_binding(&value, Some("project:test"), None).unwrap()
    );
}
#[test]
fn identity_scope_and_timestamp_constraints_fail_closed() {
    let binding = parse(&binding_document()).unwrap();
    for (pointer, value) in [
        ("/project", json!("other")),
        ("/domain", json!("other")),
        ("/evidence/0/sha256", json!("A".repeat(64))),
        ("/created_at", json!("2026-09-27T10:00:00")),
        ("/created_at", json!("2026-W39-7T10:00:00Z")),
        ("/required_capabilities", json!(["same", "same"])),
    ] {
        let mut document = task_document(&["run"]);
        *document.pointer_mut(pointer).unwrap() = value;
        assert!(task(&document, &binding).is_err(), "accepted {pointer}");
    }
    for (pointer, value) in [
        ("/semantic_api", json!("legacy")),
        ("/promotion/requires_decision_authority", json!(false)),
        ("/promotion/target", json!("project-registry")),
        ("/evidence/logical_root", json!("file:/tmp/x")),
    ] {
        let mut document = binding_document();
        *document.pointer_mut(pointer).unwrap() = value;
        assert!(parse(&document).is_err());
    }
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("var/maintenance/rust-rewrite-20260927/lifecycle-contracts/fixtures")
            .join(format!(
                "contracts-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn successor_is_parseable_projection_and_never_overwrites_predecessor() {
    let scratch = Scratch::new();
    let path = scratch.0.join("binding.json");
    let raw = serde_json::to_vec_pretty(&binding_document()).unwrap();
    fs::write(&path, &raw).unwrap();
    let binding = read_project_binding(&path).unwrap();
    let descriptor = json!({"package_id":"method-test","version":"2.0.0","package_sha256":"c".repeat(64),"manifest_sha256":"d".repeat(64),"promotion_record_sha256":"e".repeat(64),"promoted_at":"2026-09-27T12:00:00Z"});
    let projection = next_binding_projection(&binding, &descriptor).unwrap();
    assert_eq!(fs::read(&path).unwrap(), raw);
    assert_eq!(projection["auto_applied"], false);
    assert_eq!(projection["requires_control_plane_approval"], true);
    let next = parse(&projection["document"]).unwrap();
    assert_eq!(next.baseline_version, "2.0.0");
    assert_eq!(next.predecessor.unwrap()["binding_sha256"], sha256(&raw));
    assert_eq!(next.fact_authority, binding.fact_authority);
    assert_eq!(next.allowed_actions, binding.allowed_actions);
    assert_eq!(
        projection["projection_sha256"],
        sha256(canonical_json(&projection["document"]).unwrap())
    );
    let mut wrong = descriptor.clone();
    wrong["package_id"] = json!("other");
    assert!(next_binding_projection(&binding, &wrong).is_err());
    wrong = descriptor;
    wrong["version"] = json!(binding.baseline_version);
    wrong["package_sha256"] = json!(binding.baseline_digest);
    assert!(next_binding_projection(&binding, &wrong).is_err());
}
#[cfg(unix)]
#[test]
fn contract_read_rejects_symbolic_links() {
    let scratch = Scratch::new();
    let path = scratch.0.join("binding.json");
    fs::write(&path, canonical_json(&binding_document()).unwrap()).unwrap();
    let link = scratch.0.join("alias.json");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(read_project_binding(&link).is_err());
}
