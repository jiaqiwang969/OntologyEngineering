use oe_kernel::source_session::{execute_local_source_session, validate_received};
use semantica_contracts::{SemanticSourceSessionFailure, SemanticSourceSessionRequest};
use serde_json::json;

fn request(fail: bool) -> SemanticSourceSessionRequest {
    let query = if fail {
        "INSERT DATA {<urn:s> <urn:p> <urn:o>}; CREATE GRAPH <urn:unsupported>"
    } else {
        "INSERT DATA {<urn:s> <urn:p> <urn:o>}"
    };
    serde_json::from_value(json!({"schema":"semantica.source-session-request/1",
        "profile":"semantica.rdflib760-source-session/1","cache_queries":true,"created_at":"2026-10-15T09:00:00Z","resources":[],
        "actions":[{"id":"update","operation":"update","resource_id":null,"query":query,"namespaces":{},"bindings":[],"base_uri":null,"graph_name":null,"use_cache":false},
        {"id":"observe","operation":"snapshot","resource_id":null,"query":null,"namespaces":{},"bindings":[],"base_uri":null,"graph_name":null,"use_cache":false}]})).unwrap()
}
fn error() -> SemanticSourceSessionFailure {
    SemanticSourceSessionFailure {
        code: "source_session_state_observation_failed".into(),
        phase: "observe".into(),
        message: "unavailable".into(),
        original_module: None,
        original_class: None,
    }
}
#[test]
fn source_session_worker_receipt_rejects_identity_and_missing_actions() {
    let input = request(false);
    let result = execute_local_source_session(&input).unwrap();
    validate_received(&input, &result).unwrap();
    let mut bad = result.clone();
    bad.request_sha256 = "0".repeat(64);
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.source_bindings_sha256 = "0".repeat(64);
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps.pop();
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps[1].id = bad.steps[0].id.clone();
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps[0].backend_phase = "not_entered".into();
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.initial_state = serde_json::Value::Null;
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps[0].source_state = serde_json::Value::Null;
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps[0].source_state["revision"] = json!(99);
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps[0].after_state_failure = Some(error());
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps[0].source_state_failure = Some(error());
    assert!(validate_received(&input, &bad).is_err());
}
#[test]
fn source_session_worker_preserves_failure_and_requires_later_observation() {
    let input = request(true);
    let result = execute_local_source_session(&input).unwrap();
    assert_eq!(result.status, "failed");
    assert_eq!(result.steps[0].revision_after, 0);
    assert_eq!(result.steps[0].source_state["quad_count"], 1);
    validate_received(&input, &result).unwrap();
    let mut bad = result.clone();
    bad.final_state = None;
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps[0].after_state = None;
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.status = "completed".into();
    assert!(validate_received(&input, &bad).is_err());
    let mut bad = result.clone();
    bad.steps[1].status = "skipped".into();
    bad.steps[1].result = serde_json::Value::Null;
    bad.steps[1].source_state = serde_json::Value::Null;
    assert!(validate_received(&input, &bad).is_err());
}
