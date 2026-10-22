//! Explicit3 same-owner behavior; frozen original core49 remains independent.
use semantica_contracts::{SemanticSourceSessionRequest2, SemanticSourceSessionRequest3};
use semantica_runtime::{
    dataset::source_session::validate_storage_relations3, runtime::RustSemanticRuntime,
};
use serde_json::{Value, json};
fn action(id: &str, operation: &str) -> Value {
    json!({"id":id,"operation":operation,"resource_id":null,"query":null,
        "namespaces":{},"bindings":[],"base_uri":null,"graph_name":null,"use_cache":false,
        "serialize_request":null,"before_snapshot_action_id":null,"after_snapshot_action_id":null})
}
fn update(id: &str, query: &str) -> Value {
    let mut a = action(id, "update");
    a["query"] = json!(query);
    a
}
fn serialize(id: &str, format: &str, graph: Option<&str>) -> Value {
    let mut a = action(id, "serialize");
    a["serialize_request"] = json!({"format":format,"graph_name":graph,"base_uri":null});
    a
}
fn diff(id: &str, before: &str, after: Option<&str>) -> Value {
    let mut a = action(id, "diff");
    a["before_snapshot_action_id"] = json!(before);
    a["after_snapshot_action_id"] = json!(after);
    a
}
fn request(data: &str, mut actions: Vec<Value>) -> SemanticSourceSessionRequest3 {
    let mut load = action("load", "load");
    load["resource_id"] = json!("urn:source");
    actions.insert(0, load);
    serde_json::from_value(json!({"schema":"semantica.source-session-request/3",
        "profile":"semantica.rdflib760-source-session/3","cache_queries":true,
        "created_at":"2026-10-14T12:00:00Z",
        "resources":[{"binding":{"schema":"semantica.source-binding/1","source_id":"urn:source",
            "byte_count":data.len(),"source_sha256":semantica_runtime::sha256(data.as_bytes())},
            "data":data,"rdf_format":"turtle"}],"actions":actions}))
    .unwrap()
}
const A: &str = "<urn:diff:a> <urn:p> \"left\" .\n";
const B: &str = "INSERT DATA {<urn:diff:b> <urn:p> \"right\"}";

fn ordered() -> SemanticSourceSessionRequest3 {
    request(
        A,
        vec![
            action("before", "snapshot"),
            serialize("export-a", "nt", None),
            update("update-b", B),
            diff("delta-current", "before", None),
            action("after", "snapshot"),
            diff("delta-explicit", "before", Some("after")),
            serialize("export-ab", "nquads", None),
            action("clear", "clear"),
            serialize("export-empty", "nt", None),
            diff("delta-empty", "after", None),
        ],
    )
}

#[test]
fn source_session3_preserves_original_ordered_storage_results() {
    let input = ordered();
    let actual = RustSemanticRuntime.execute_source_session3(&input).unwrap();
    assert_eq!(actual.status, "completed");
    assert_eq!(actual.storage_witnesses.len(), 6);
    validate_storage_relations3(&input, &actual).unwrap();
    let mut old_input = serde_json::to_value(&input).unwrap();
    old_input["schema"] = json!("semantica.source-session-request/2");
    old_input["profile"] = json!("semantica.rdflib760-source-session/2");
    let old_input: SemanticSourceSessionRequest2 = serde_json::from_value(old_input).unwrap();
    let old = RustSemanticRuntime
        .execute_source_session2(&old_input)
        .unwrap();
    assert_eq!(actual.steps, old.steps);
    assert_eq!(actual.initial_state, old.initial_state);
    assert_eq!(actual.final_state, old.final_state);
    assert_eq!(actual.steps[2].result, json!({"text":A,"warnings":[]}));
    assert_eq!(
        actual.steps[4].result,
        json!({"schema_version":"1.0", "before_sha256":"b89ebd6122534c2504dfe7d263f1a37f857456f2409ec4b7ac45d0675011b4a9", "after_sha256":"d0bcf3462e477698577bdca4c2c1da03a84314037fea06bb6143ed01129015b6", "added_quads":["<urn:diff:b> <urn:p> \"right\" ."], "removed_quads":[], "unchanged_count":1,"changed":true,"created_at":"2026-10-14T12:00:00Z"})
    );
    assert!(actual.storage_witnesses[1].current_snapshot.is_some());
    assert!(actual.storage_witnesses[2].current_snapshot.is_none());
    assert_eq!(actual.steps[3].source_state["snapshot_cached"], false);
    assert_eq!(actual.steps[4].source_state["snapshot_cached"], true);
}
#[test]
fn source_session3_preserves_generalized_terms_and_selected_contexts() {
    let data = "<urn:d> <urn:p> \"default\" . <urn:g> { <urn:n> <urn:p> \"named\"@EN . }";
    let mut input = request(
        data,
        vec![
            serialize("selected", "nt", Some("urn:g")),
            serialize("empty", "nt", Some("urn:missing")),
            action("before", "snapshot"),
            diff("same", "before", None),
        ],
    );
    input.resources[0].rdf_format = "trig".into();
    input.actions[1]
        .serialize_request
        .as_mut()
        .unwrap()
        .base_uri = Some("".into());
    let actual = RustSemanticRuntime.execute_source_session3(&input).unwrap();
    assert_eq!(actual.status, "completed");
    validate_storage_relations3(&input, &actual).unwrap();
    assert_eq!(
        actual.steps[1].result,
        json!({"text":"<urn:n> <urn:p> \"named\"@EN .\n","warnings":["NTSerializer does not support base."]})
    );
    assert_eq!(actual.steps[2].result["text"], "");
    assert!(
        actual.storage_witnesses[2]
            .current_source
            .as_ref()
            .unwrap()
            .contexts
            .iter()
            .any(|v| v.value == "urn:missing")
    );
    // Existing generalized DATA owner retains Variable and the original DTO
    // error presentation. No witness turns that observation into an RDF fact.
    let input = request(
        A,
        vec![
            update("var", "INSERT DATA {?x <urn:p> \"variable\"}"),
            serialize("var-write", "nt", None),
        ],
    );
    let actual = RustSemanticRuntime.execute_source_session3(&input).unwrap();
    assert_eq!(actual.steps[2].status, "completed");
    validate_storage_relations3(&input, &actual).unwrap();
    assert!(
        actual.storage_witnesses[0]
            .serialization
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .any(|r| r.subject.term_type == "variable")
    );
}
#[test]
fn source_session3_source_terms_preserve_lexical_metadata_and_blank_identity() {
    let data = "<urn:s> <urn:p> \"same\" .\n<urn:s> <urn:p> \"same\"^^<http://www.w3.org/2001/XMLSchema#string> .\n<urn:l> <urn:p> \"language\"@EN-us <urn:g> .\n_:fixed <urn:p> \"line\\nend\" .\n";
    let mut input = request(data, vec![serialize("nq", "nq", None)]);
    input.resources[0].rdf_format = "nquads".into();
    let actual = RustSemanticRuntime.execute_source_session3(&input).unwrap();
    assert_eq!(actual.steps[1].status, "completed");
    validate_storage_relations3(&input, &actual).unwrap();
    let rows = &actual.storage_witnesses[0]
        .serialization
        .as_ref()
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 4);
    assert!(
        rows.iter()
            .any(|r| r.object.value == "same" && r.object.datatype.is_none())
    );
    assert!(rows.iter().any(|r| r.object.value == "same"
        && r.object.datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#string")));
    assert!(
        rows.iter()
            .any(|r| r.object.language.as_deref() == Some("EN-us")
                && r.graph.as_ref().is_some_and(|g| g.value == "urn:g"))
    );
    assert!(
        rows.iter()
            .any(|r| r.subject.term_type == "blank_node" && r.object.value == "line\nend")
    );
}

#[test]
fn source_session3_witness_capture_is_bounded_and_failure_preserves_state() {
    let mut data = String::new();
    for n in 0..1024 {
        data.push_str(&format!(
            "<urn:row:{n}> <urn:p> \"{}\" .\n",
            "x".repeat(2048)
        ));
    }
    let input = request(
        &data,
        (0..16)
            .map(|n| serialize(&format!("export-{n}"), "nt", None))
            .collect(),
    );
    let actual = RustSemanticRuntime.execute_source_session3(&input).unwrap();
    assert_eq!(actual.status, "partial");
    assert_eq!(
        actual.failure.as_ref().unwrap().code,
        "source_session_result_byte_budget"
    );
    assert!(!actual.storage_witnesses.is_empty());
    assert!(actual.steps.iter().any(|s| s.status == "skipped"));
    assert_eq!(actual.final_state.as_ref().unwrap().quad_count, 1024);
    validate_storage_relations3(&input, &actual).unwrap();
    let input = request(
        A,
        vec![
            action("before", "snapshot"),
            update(
                "bad",
                "INSERT DATA {<urn:diff:b> <urn:p> \"right\"}; CREATE GRAPH <urn:unsupported>",
            ),
            diff("prefix", "before", None),
            serialize("prefix-write", "nt", None),
            update("skip", B),
        ],
    );
    let actual = RustSemanticRuntime.execute_source_session3(&input).unwrap();
    assert_eq!(actual.status, "failed");
    assert_eq!(actual.steps[2].revision_after, 1);
    assert_eq!(actual.steps[3].result["changed"], true);
    assert_eq!(actual.steps[5].status, "skipped");
    validate_storage_relations3(&input, &actual).unwrap();
}
#[test]
fn source_session3_failed_snapshot_keeps_readonly_continuation() {
    let input = request(
        A,
        vec![
            action("before", "snapshot"),
            update(
                "generalized",
                "INSERT { <urn:new> ?p \"object\" } WHERE { VALUES ?p { \"predicate\" } }",
            ),
            action("bad-snapshot", "snapshot"),
            diff("bad-producer", "bad-snapshot", None),
            diff("old-self", "before", Some("before")),
            serialize("retained", "nt", None),
            action("skip-clear", "clear"),
        ],
    );
    let actual = RustSemanticRuntime.execute_source_session3(&input).unwrap();
    assert_eq!(actual.status, "failed");
    assert_eq!(actual.steps[3].status, "failed");
    assert_eq!(
        actual.steps[3]
            .failure
            .as_ref()
            .unwrap()
            .original_class
            .as_deref(),
        Some("ValueError")
    );
    assert_eq!(actual.steps[4].status, "failed");
    assert_eq!(actual.steps[5].status, "completed");
    assert_eq!(actual.steps[6].status, "completed");
    assert_eq!(actual.steps[7].status, "skipped");
    assert_eq!(actual.steps[5].result["changed"], false);
    validate_storage_relations3(&input, &actual).unwrap();
    assert_eq!(actual.storage_witnesses.len(), 2);
}
