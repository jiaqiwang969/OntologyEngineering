use oe_kernel::source_session::{execute_local_source_session4, validate_received4};
use semantica_contracts::SemanticSourceSessionRequest4;
use serde_json::{Value, json};
fn action(id: &str, op: &str) -> Value {
    json!({"id":id,"operation":op,"resource_id":null,"query":null,"namespaces":{},"bindings":[],
        "base_uri":null,"graph_name":null,"use_cache":false,"serialize_request":null,
        "before_snapshot_action_id":null,"after_snapshot_action_id":null})
}
fn request(actions: Vec<Value>) -> SemanticSourceSessionRequest4 {
    serde_json::from_value(json!({"schema":"semantica.source-session-request/4",
        "profile":"semantica.rdflib760-source-session/4","resources":[],"actions":actions,
        "cache_queries":true,"created_at":"2026-10-17T00:00:00Z"}))
    .unwrap()
}
#[test]
fn source_session4_failure_keeps_effects_reads_and_original_refusals() {
    let mut partial = action("partial", "update");
    partial["query"] =
        json!("INSERT DATA {<urn:s> <http://one/p> \"x\"}; CREATE GRAPH <urn:x-rdflib:default>");
    let mut turtle = action("read", "serialize");
    turtle["serialize_request"] = json!({"format":"turtle","base_uri":null,"graph_name":null});
    let r = request(vec![partial, action("skipped", "clear"), turtle]);
    let actual = execute_local_source_session4(&r).unwrap();
    assert_eq!(actual.status, "failed");
    assert_eq!(actual.steps[0].backend_phase, "reached");
    assert_eq!(actual.steps[0].source_state["quad_count"], 1);
    assert_eq!(actual.steps[1].status, "skipped");
    assert_eq!(actual.steps[2].status, "completed");
    validate_received4(&r, &actual).unwrap();
    let mut false_success = actual.clone();
    false_success.status = "completed".into();
    assert!(validate_received4(&r, &false_success).is_err());
    let mut skipped_read = actual.clone();
    skipped_read.steps[2].status = "skipped".into();
    assert!(validate_received4(&r, &skipped_read).is_err());
    for text in ["LOAD <urn:unbound>", "LOAD SILENT <urn:unbound>"] {
        let mut a = action("unbound", "update");
        a["query"] = json!(text);
        let r = request(vec![a]);
        let actual = execute_local_source_session4(&r).unwrap();
        assert_eq!(actual.status, "failed");
        assert!(!actual.steps[0].resource_requests.is_empty());
        validate_received4(&r, &actual).unwrap();
    }
}
