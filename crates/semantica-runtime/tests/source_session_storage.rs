//! Finite /2 integration controls. Original49 core receiving remains separate.
use semantica_contracts::{SemanticSourceSessionRequest, SemanticSourceSessionRequest2};
use semantica_runtime::{
    dataset::source_session::{validate_request, validate_request2},
    runtime::RustSemanticRuntime,
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
fn request(data: &str, mut actions: Vec<Value>) -> SemanticSourceSessionRequest2 {
    let mut load = action("load", "load");
    load["resource_id"] = json!("urn:source");
    actions.insert(0, load);
    serde_json::from_value(json!({"schema":"semantica.source-session-request/2",
        "profile":"semantica.rdflib760-source-session/2","cache_queries":true,
        "created_at":"2026-10-14T12:00:00Z",
        "resources":[{"binding":{"schema":"semantica.source-binding/1","source_id":"urn:source",
            "byte_count":data.len(),"source_sha256":semantica_runtime::sha256(data.as_bytes())},
            "data":data,"rdf_format":"turtle"}],"actions":actions}))
    .unwrap()
}
const A: &str = "<urn:diff:a> <urn:p> \"left\" .\n";
const B: &str = "INSERT DATA {<urn:diff:b> <urn:p> \"right\"}";

#[test]
fn source_session2_interleaves_storage_and_mutations_in_one_session() {
    let input = request(
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
    );
    let result = RustSemanticRuntime.execute_source_session2(&input).unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(result.steps.len(), 11);
    assert_eq!(result.steps[2].result, json!({"text":A,"warnings":[]}));
    // Frozen original SD106 complete value, independently known before adapter.
    let expected = json!({"schema_version":"1.0",
        "before_sha256":"b89ebd6122534c2504dfe7d263f1a37f857456f2409ec4b7ac45d0675011b4a9",
        "after_sha256":"d0bcf3462e477698577bdca4c2c1da03a84314037fea06bb6143ed01129015b6",
        "added_quads":["<urn:diff:b> <urn:p> \"right\" ."],"removed_quads":[],
        "unchanged_count":1,"changed":true,"created_at":"2026-10-14T12:00:00Z"});
    assert_eq!(result.steps[4].result, expected);
    assert_eq!(result.steps[6].result, expected);
    assert_eq!(result.steps[3].source_state["snapshot_cached"], false);
    // Existing diff(current) legitimately warms the same owner's snapshot cache.
    assert_eq!(result.steps[4].source_state["snapshot_cached"], true);
    assert_eq!(result.steps[7].source_state["quad_count"], 2);
    assert_eq!(result.steps[9].result, json!({"text":"","warnings":[]}));
    assert_eq!(result.steps[10].result["added_quads"], json!([]));
    assert_eq!(
        result.steps[10].result["removed_quads"],
        json!([A.trim_end(), "<urn:diff:b> <urn:p> \"right\" ."])
    );
    assert_eq!(result.steps[1].revision_after, 1);
    assert_eq!(result.steps[7].revision_after, 2);
    assert_eq!(result.final_state.unwrap().revision, 3);
}

#[test]
fn source_session2_rejects_unavailable_snapshot_references() {
    let base = request(
        A,
        vec![
            action("before", "snapshot"),
            diff("delta", "before", None),
            action("future", "snapshot"),
        ],
    );
    for bad_ref in ["missing", "load", "delta", "future"] {
        let mut bad = base.clone();
        bad.actions[2].before_snapshot_action_id = Some(bad_ref.into());
        assert!(
            validate_request2(&bad)
                .unwrap_err()
                .to_string()
                .contains("not_prior_snapshot")
        );
        assert!(RustSemanticRuntime.execute_source_session2(&bad).is_err());
    }
    let mut bad = base.clone();
    bad.actions[2].id = "before".into();
    assert!(validate_request2(&bad).is_err());
    let mut bad = base.clone();
    bad.actions[1].serialize_request = Some(
        serde_json::from_value(json!({"format":"nt","graph_name":null,"base_uri":null})).unwrap(),
    );
    assert!(validate_request2(&bad).is_err());
    let mut bad = base.clone();
    bad.actions[2].after_snapshot_action_id = Some("future".into());
    assert!(validate_request2(&bad).is_err());
}

#[test]
fn source_session2_preserves_original_graph_selection_errors_and_warnings() {
    let data = "<urn:d> <urn:p> \"default\" .\n<urn:g> { <urn:n> <urn:p> \"named\" . }\n";
    let mut input = request(
        data,
        vec![
            serialize("selected-nq", "nq", Some("urn:g")),
            serialize("lossy-nt", "nt", None),
            serialize("selected-nt", "nt", Some("urn:g")),
            serialize("empty-nt", "nt", Some("urn:missing")),
            update("must-skip", B),
        ],
    );
    input.resources[0].rdf_format = "trig".into();
    input.actions[3]
        .serialize_request
        .as_mut()
        .unwrap()
        .base_uri = Some("urn:base".into());
    let r = RustSemanticRuntime.execute_source_session2(&input).unwrap();
    assert_eq!(r.status, "failed");
    assert_eq!(
        r.steps[1]
            .failure
            .as_ref()
            .unwrap()
            .original_class
            .as_deref(),
        Some("SemanticInputError")
    );
    assert_eq!(
        r.steps[2]
            .failure
            .as_ref()
            .unwrap()
            .original_class
            .as_deref(),
        Some("LossySerializationError")
    );
    assert_eq!(
        r.steps[3].result,
        json!({"text":"<urn:n> <urn:p> \"named\" .\n","warnings":["NTSerializer does not support base."]})
    );
    assert_eq!(r.steps[4].result, json!({"text":"","warnings":[]}));
    assert_eq!(r.steps[5].status, "skipped");
    assert!(
        r.steps[4].source_state["contexts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["value"] == "urn:missing")
    );
}

#[test]
fn source_session2_reads_actual_prefix_after_failed_update() {
    let input = request(
        A,
        vec![
            action("before", "snapshot"),
            update(
                "partial",
                "INSERT DATA {<urn:diff:b> <urn:p> \"right\"}; CREATE GRAPH <urn:unsupported>",
            ),
            serialize("retained", "nt", None),
            diff("changed", "before", None),
            update("skip", B),
        ],
    );
    let r = RustSemanticRuntime.execute_source_session2(&input).unwrap();
    assert_eq!(r.status, "failed");
    assert_eq!(r.steps[2].status, "failed");
    assert_eq!(r.steps[2].backend_phase, "reached");
    assert_eq!(r.steps[2].revision_after, 1);
    assert_eq!(r.steps[2].source_state["quad_count"], 2);
    assert_eq!(r.steps[2].source_state["snapshot_cached"], false);
    assert_eq!(r.steps[3].status, "completed");
    assert_eq!(r.steps[3].revision_after, 1);
    assert!(
        r.steps[3].result["text"]
            .as_str()
            .unwrap()
            .contains("<urn:diff:b>")
    );
    assert_eq!(r.steps[4].result["changed"], true);
    assert_eq!(r.steps[5].status, "skipped");
    assert_eq!(r.final_state.unwrap().quad_count, 2);
}

#[test]
fn source_session2_preserves_source_identity_and_version_boundaries() {
    let mut input = request(A, vec![serialize("export", "nt", None)]);
    input.resources[0].binding.source_sha256 = "0".repeat(64);
    assert!(RustSemanticRuntime.execute_source_session2(&input).is_err());
    let mut input = request(A, vec![serialize("export", "nt", None)]);
    input.resources[0].binding.byte_count += 1;
    assert!(validate_request2(&input).is_err());
    let mut input = request(A, vec![serialize("export", "nt", None)]);
    input.profile = "semantica.rdflib760-source-session/1".into();
    assert!(validate_request2(&input).is_err());
    let mut old = serde_json::to_value(request(A, vec![serialize("export", "nt", None)])).unwrap();
    old["schema"] = json!("semantica.source-session-request/1");
    old["profile"] = json!("semantica.rdflib760-source-session/1");
    for a in old["actions"].as_array_mut().unwrap() {
        for key in [
            "serialize_request",
            "before_snapshot_action_id",
            "after_snapshot_action_id",
        ] {
            a.as_object_mut().unwrap().remove(key);
        }
    }
    let old: SemanticSourceSessionRequest = serde_json::from_value(old).unwrap();
    assert!(validate_request(&old).is_err());
}

#[test]
fn source_session2_serialization_is_not_reload_compatibility() {
    let input = request(
        "<urn:seed> <urn:p> \"retained\" .",
        vec![
            update("generalized", "INSERT DATA {?slot <urn:p> \"source-only\"}"),
            serialize("export", "nquads", None),
        ],
    );
    let r = RustSemanticRuntime.execute_source_session2(&input).unwrap();
    assert_eq!(r.steps[2].status, "completed");
    let text = r.steps[2].result["text"].as_str().unwrap();
    assert!(text.contains("?slot"));
    // Host-side explicit save/re-read creates the next source binding from actual
    // bytes. This neither grants worker file access nor restores session caches.
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("actual.nq");
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .unwrap();
    f.write_all(text.as_bytes()).unwrap();
    f.sync_all().unwrap();
    drop(f);
    let saved = std::fs::read(&path).unwrap();
    assert_eq!(saved, text.as_bytes());
    let mut next = request(
        "<urn:seed> <urn:p> \"retained\" .",
        vec![action("bad-reload", "load"), action("state", "snapshot")],
    );
    let mut resource = next.resources[0].clone();
    resource.binding.source_id = "urn:saved".into();
    resource.data = String::from_utf8(saved).unwrap();
    resource.rdf_format = "nquads".into();
    resource.binding.byte_count = resource.data.len() as u64;
    resource.binding.source_sha256 = semantica_runtime::sha256(resource.data.as_bytes());
    next.resources.push(resource);
    next.actions[1].resource_id = Some("urn:saved".into());
    let r = RustSemanticRuntime.execute_source_session2(&next).unwrap();
    assert_eq!(r.status, "failed");
    assert_eq!(r.steps[1].status, "failed");
    assert_eq!(r.steps[1].revision_after, 1);
    assert_eq!(r.steps[2].result["quad_count"], 1);
    assert_eq!(
        r.steps[1]
            .failure
            .as_ref()
            .unwrap()
            .original_class
            .as_deref(),
        Some("SemanticInputError")
    );
}

#[test]
fn source_session2_storage_capture_budget_preserves_prefix_and_skips_references() {
    let data: String = (0..1024)
        .map(|i| format!("<urn:large:{i}> <urn:p> \"{}\" .\n", "x".repeat(2048)))
        .collect();
    let mut actions = vec![action("before", "snapshot")];
    for i in 0..16 {
        actions.push(serialize(&format!("export-{i}"), "nt", None));
    }
    actions.push(action("unobserved", "snapshot"));
    actions.push(diff("no-fabrication", "unobserved", None));
    let input = request(&data, actions);
    let r = RustSemanticRuntime.execute_source_session2(&input).unwrap();
    assert_eq!(r.status, "partial");
    assert_eq!(r.steps.len(), 20);
    assert_eq!(
        r.failure.as_ref().unwrap().code,
        "source_session_result_byte_budget"
    );
    let crossing = r.steps.iter().position(|s| s.status == "failed").unwrap();
    assert!(crossing > 2);
    assert!(r.steps[..crossing].iter().all(|s| s.status == "completed"));
    assert!(r.steps[crossing].result.is_null());
    assert!(
        r.steps[crossing + 1..]
            .iter()
            .all(|s| s.status == "skipped" && s.result.is_null())
    );
    assert_eq!(r.final_state.unwrap().quad_count, 1024);
}

#[test]
fn source_session2_host_roundtrip_preserves_source_terms_and_integrity_errors() {
    // Synthetic variant of the independently received source49 conditions.
    // No captured result is injected as a snapshot or as a future answer.
    let data = "<urn:storage:s> <urn:p> \"same\" .\n<urn:storage:s> <urn:p> \"same\"^^<http://www.w3.org/2001/XMLSchema#string> .\n<urn:storage:l> <urn:p> \"language\"@EN-us <urn:storage:g> .\n<urn:storage:e> <urn:p> \"line\\nend\" .\n";
    let mut input = request(
        data,
        vec![
            action("before", "snapshot"),
            serialize("export", "nquads", None),
            diff("integrity", "before", None),
        ],
    );
    input.resources[0].rdf_format = "nquads".into();
    let r = RustSemanticRuntime.execute_source_session2(&input).unwrap();
    assert_eq!(r.steps[1].result["quad_count"], 4);
    assert_eq!(r.steps[2].status, "completed");
    assert_eq!(r.steps[3].status, "failed");
    assert_eq!(
        r.steps[3]
            .failure
            .as_ref()
            .unwrap()
            .original_class
            .as_deref(),
        Some("SemanticInputError")
    );
    assert_eq!(
        r.steps[3].failure.as_ref().unwrap().message,
        "cannot diff a dataset snapshot with invalid integrity"
    );
    let text = r.steps[2].result["text"].as_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roundtrip.nq");
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    file.write_all(text.as_bytes()).unwrap();
    file.sync_all().unwrap();
    drop(file);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes, text.as_bytes());
    let mut next = request(
        std::str::from_utf8(&bytes).unwrap(),
        vec![action("reloaded", "snapshot")],
    );
    next.resources[0].rdf_format = "nquads".into();
    let reloaded = RustSemanticRuntime.execute_source_session2(&next).unwrap();
    assert_eq!(reloaded.status, "completed");
    assert_eq!(reloaded.final_state.as_ref().unwrap().revision, 1);
    assert_eq!(reloaded.steps[1].result["quad_count"], 4);
    assert_eq!(
        reloaded.steps[1].result["canonical_nquads"],
        r.steps[1].result["canonical_nquads"]
    );
    let quads = reloaded.steps[0].source_state["quads"].as_array().unwrap();
    assert!(quads.iter().any(|q| q[0]["value"] == "urn:storage:s"
        && q[2]["value"] == "same"
        && q[2]["datatype"].is_null()));
    assert!(quads.iter().any(|q| q[0]["value"] == "urn:storage:s"
        && q[2]["datatype"] == "http://www.w3.org/2001/XMLSchema#string"));
    assert!(
        quads
            .iter()
            .any(|q| q[2]["language"] == "EN-us" && q[3]["value"] == "urn:storage:g")
    );
    assert!(
        quads
            .iter()
            .any(|q| q[0]["value"] == "urn:storage:e" && q[2]["value"] == "line\nend")
    );
}
