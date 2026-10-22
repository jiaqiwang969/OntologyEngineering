use oe_kernel::source_session::{execute_local_source_session2, validate_received2};
use semantica_contracts::{SemanticSourceSessionFailure, SemanticSourceSessionRequest2};
use serde_json::{Value, json};
fn a(id: &str, operation: &str) -> Value {
    json!({"id":id,"operation":operation,"resource_id":null,"query":null,"namespaces":{},"bindings":[],"base_uri":null,"graph_name":null,"use_cache":false,"serialize_request":null,"before_snapshot_action_id":null,"after_snapshot_action_id":null})
}
fn request(fail: bool) -> SemanticSourceSessionRequest2 {
    let mut update = a("update", "update");
    update["query"] = json!(if fail {
        "INSERT DATA {<urn:s> <urn:p> <urn:o>}; CREATE GRAPH <urn:unsupported>"
    } else {
        "INSERT DATA {<urn:s> <urn:p> <urn:o>}"
    });
    let mut export = a("export", "serialize");
    export["serialize_request"] = json!({"format":"nt","base_uri":null,"graph_name":null});
    let mut diff = a("diff", "diff");
    diff["before_snapshot_action_id"] = json!("before");
    diff["after_snapshot_action_id"] = json!("after");
    serde_json::from_value(json!({"schema":"semantica.source-session-request/2","profile":"semantica.rdflib760-source-session/2","cache_queries":true,"created_at":"2026-10-14T12:00:00Z","resources":[],"actions":[a("before","snapshot"),update,a("after","snapshot"),export,diff]})).unwrap()
}
fn failure() -> SemanticSourceSessionFailure {
    SemanticSourceSessionFailure {
        code: "source_session_action_failed".into(),
        phase: "snapshot".into(),
        message: "independent mutation control".into(),
        original_module: None,
        original_class: None,
    }
}
#[test]
fn source_session2_receiving_rejects_storage_identity_and_false_success() {
    let input = request(false);
    let actual = execute_local_source_session2(&input).unwrap();
    validate_received2(&input, &actual).unwrap();
    let mut bad = actual.clone();
    bad.schema = "semantica.source-session-result/1".into();
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.profile = "semantica.rdflib760-source-session/1".into();
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.request_sha256 = "0".repeat(64);
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.source_bindings_sha256 = "0".repeat(64);
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.steps.pop();
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.steps.swap(0, 2);
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.steps[3]
        .result
        .as_object_mut()
        .unwrap()
        .remove("warnings");
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.steps[4].result["before_sha256"] = json!("0".repeat(64));
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.steps[4].result["after_sha256"] = json!("0".repeat(64));
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.steps[3].failure = Some(failure());
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.steps[3].revision_after += 1;
    assert!(validate_received2(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.final_state = None;
    assert!(validate_received2(&input, &bad).is_err());
    // A successful diff may never reference an unavailable producer, even if
    // the rest of a failed envelope is self-consistent.
    let mut bad = actual.clone();
    bad.status = "failed".into();
    bad.steps[2].status = "failed".into();
    let snapshot: semantica_contracts::SemanticRdfLibDatasetSnapshot =
        serde_json::from_value(bad.steps[2].result.clone()).unwrap();
    bad.steps[2].result = Value::Null;
    bad.steps[2].failure = Some(failure());
    bad.steps[2].after_state = Some(snapshot);
    assert!(
        validate_received2(&input, &bad)
            .unwrap_err()
            .to_string()
            .contains("endpoint_not_retained")
    );
    let input = request(true);
    let actual = execute_local_source_session2(&input).unwrap();
    assert_eq!(actual.status, "failed");
    validate_received2(&input, &actual).unwrap();
    let mut bad = actual.clone();
    bad.status = "completed".into();
    assert!(validate_received2(&input, &bad).is_err());
    assert_eq!(actual.steps[3].status, "completed");
    assert_eq!(actual.steps[4].status, "completed");
}
