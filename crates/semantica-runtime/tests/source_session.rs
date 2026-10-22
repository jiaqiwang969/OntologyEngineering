//! Synthetic independent expectations; no external oracle or private paths.
use semantica_contracts::SemanticSourceSessionRequest;
use semantica_runtime::{dataset::source_session::*, runtime::RustSemanticRuntime};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/source-session-controls.json")).unwrap()
}
fn request(id: &str) -> SemanticSourceSessionRequest {
    serde_json::from_value(fixture()["requests"][id].clone()).unwrap()
}
fn check(id: &str) {
    let expected = &fixture()["expected"][id];
    let request = request(id);
    let result = RustSemanticRuntime.execute_source_session(&request);
    if expected["admission"] == "rejected_before_any_action" {
        let failure = result.unwrap_err();
        assert_eq!(failure.phase, "admission");
        assert_eq!(failure.code, "source_session_admission_refused");
        assert!(!failure.retryable);
        return;
    }
    let result = result.unwrap();
    assert_eq!(
        result.steps.len(),
        expected["action_order"].as_array().unwrap().len()
    );
    assert_eq!(result.engineering_adoption, "not_assessed");
    assert_eq!(result.request_sha256, request_sha256(&request).unwrap());
    assert!(result.initial_state_failure.is_none());
    for (i, step) in result.steps.iter().enumerate() {
        assert_eq!(step.id, expected["action_order"][i]);
        assert_eq!(step.status, expected["statuses"][i]);
        assert_eq!(
            json!([step.revision_before, step.revision_after]),
            expected["revision_before_after"][i]
        );
        assert!(step.source_state_failure.is_none());
        assert!(step.resource_requests_complete);
        if step.status != "skipped" {
            assert_eq!(step.source_state["revision"], step.revision_after);
        }
        if let Some(q) = expected["query_checks"].get(&step.id) {
            assert_eq!(step.result["cache_hit"], q["cache_hit"]);
            let raw = &step.result["result"];
            if let Some(boolean) = q.get("boolean") {
                assert_eq!(&raw["boolean"], boolean);
            }
            if let Some(subjects) = q.get("ordered_iri_subjects") {
                assert_eq!(raw["query_type"], "SELECT");
                assert_eq!(raw["variables"], json!(["s"]));
                let wanted: Vec<_> = subjects.as_array().unwrap().iter().map(|s|
                    json!({"values":[["s",{"term_type":"iri","value":s,"datatype":null,"language":null}]]})).collect();
                assert_eq!(raw["rows"], json!(wanted));
            }
        }
        for category in ["mutation_checks", "snapshots"] {
            if let Some(fields) = expected[category].get(&step.id) {
                for (key, value) in fields.as_object().unwrap() {
                    assert_eq!(&step.result[key], value, "{id}/{}:{key}", step.id);
                }
            }
        }
    }
    if let Some(f) = expected.get("failure") {
        assert_eq!(result.status, "failed");
        let step = result.steps.iter().find(|s| s.id == f["action"]).unwrap();
        assert_eq!(step.backend_phase, f["backend_phase"]);
        let error = step.failure.as_ref().unwrap();
        if let Some(m) = f.get("module") {
            assert_eq!(error.original_module.as_deref(), m.as_str());
        }
        if let Some(c) = f.get("class") {
            assert_eq!(error.original_class.as_deref(), c.as_str());
        }
        if let Some(message) = f.get("message") {
            assert_eq!(error.message, message.as_str().unwrap());
        }
        if let Some(text) = f.get("diagnostic_contains") {
            assert!(error.message.contains(text.as_str().unwrap()));
            assert!(error.original_class.is_none() && error.original_module.is_none());
        }
        assert_eq!(step.source_state["query_cache_entries"], 0);
        assert_eq!(step.source_state["snapshot_cached"], false);
        assert!(step.after_state.is_some() && step.after_state_failure.is_none());
    } else {
        assert_eq!(result.status, "completed");
    }
}

#[test]
fn source_session_rejects_wrong_identity_and_duplicate_action_before_execution() {
    for id in [
        "S04_wrong_source_sha",
        "S05_wrong_utf8_byte_count",
        "S06_duplicate_action_id",
        "S07_duplicate_resource_id",
    ] {
        check(id);
    }
}
#[test]
fn source_session_preserves_order_cache_revision_and_failure_state() {
    for id in [
        "S01_same_session",
        "S02_reached_prefix_failure",
        "S08_authorized_load",
    ] {
        check(id);
    }
    let mut input = request("S02_reached_prefix_failure");
    let mut clear = input.actions[4].clone();
    clear.id = "skip-clear".into();
    clear.operation = "clear".into();
    clear.query = None;
    input.actions.insert(6, clear);
    let actual = RustSemanticRuntime.execute_source_session(&input).unwrap();
    assert_eq!(actual.steps[6].status, "skipped");
    assert_eq!(actual.final_state.unwrap().quad_count, 1);
}
#[test]
fn source_session_unknown_silent_resources_are_refused() {
    for id in ["S03_unknown_load_silent", "S09_unknown_using"] {
        check(id);
    }
}
#[test]
fn source_session_clear_uses_same_owner_and_observes_empty_revision() {
    let mut input = request("S01_same_session");
    let mut clear = input.actions.last().unwrap().clone();
    clear.id = "clear".into();
    clear.operation = "clear".into();
    input.actions.push(clear);
    let mut snapshot = input.actions[3].clone();
    snapshot.id = "empty".into();
    input.actions.push(snapshot);
    let actual = RustSemanticRuntime.execute_source_session(&input).unwrap();
    assert_eq!(actual.status, "completed");
    assert_eq!(actual.steps[8].result["removed"], 2);
    assert_eq!(actual.steps[8].source_state["snapshot_cached"], false);
    assert_eq!(actual.steps[9].result["quad_count"], 0);
    assert_eq!(actual.steps[9].result["revision"], 3);
    assert_eq!(actual.steps[9].source_state["snapshot_cached"], true);
}
#[test]
fn source_session_preserves_decoded_and_wire_budgets_without_truncation() {
    let mut input = request("S01_same_session");
    input.actions.truncate(1);
    let data = format!(
        "#{}\n<urn:s> <urn:p> <urn:o> .",
        "x".repeat(16 * 1024 * 1024 - 27)
    );
    // Exactly bounded source bytes are admissible, including the original graph.
    assert_eq!(data.len(), 16 * 1024 * 1024);
    input.resources[0].data = data;
    input.resources[0].binding.byte_count = input.resources[0].data.len() as u64;
    input.resources[0].binding.source_sha256 =
        semantica_runtime::sha256(input.resources[0].data.as_bytes());
    validate_request(&input).unwrap();
    input.resources[0].data.push(' ');
    input.resources[0].binding.byte_count += 1;
    input.resources[0].binding.source_sha256 =
        semantica_runtime::sha256(input.resources[0].data.as_bytes());
    assert!(
        validate_request(&input)
            .unwrap_err()
            .to_string()
            .contains("resource_byte_budget")
    );
    let mut input = request("S01_same_session");
    input.actions[1].query = Some(" ".repeat(1024 * 1024 + 1));
    assert!(
        validate_request(&input)
            .unwrap_err()
            .to_string()
            .contains("query_byte_budget")
    );
    let mut input = request("S01_same_session");
    input.actions = (0..33)
        .map(|i| {
            let mut a = input.actions[0].clone();
            a.id = format!("a{i}");
            a
        })
        .collect();
    assert!(
        validate_request(&input)
            .unwrap_err()
            .to_string()
            .contains("action_budget")
    );
}

#[test]
fn source_session_oversized_term_failure_precedes_capture_budget() {
    let mut input = request("S01_same_session");
    let data = format!("<urn:s> <urn:p> \"{}\" .", "x".repeat(2 * 1024 * 1024));
    input.resources[0].data = data;
    input.resources[0].binding.byte_count = input.resources[0].data.len() as u64;
    input.resources[0].binding.source_sha256 =
        semantica_runtime::sha256(input.resources[0].data.as_bytes());
    let snapshot = input.actions[3].clone();
    input.actions.truncate(1);
    for i in 0..16 {
        let mut a = snapshot.clone();
        a.id = format!("snapshot-{i}");
        input.actions.push(a);
    }
    let result = RustSemanticRuntime.execute_source_session(&input).unwrap();
    // The original single 2 MiB literal exceeds the existing per-value limit;
    // it cannot exercise the cumulative result-capture limit.
    assert_eq!(result.status, "failed");
    assert_eq!(result.steps.len(), 17);
    assert_eq!(result.steps[0].status, "failed");
    let failure = result.steps[0].failure.as_ref().unwrap();
    assert_eq!(failure.code, "source_session_action_failed");
    assert_eq!(failure.phase, "load");
    assert_eq!(failure.message, "numeric resource limit: input_characters");
    assert_eq!(result.steps[0].revision_after, 0);
    assert_eq!(result.steps[0].source_state["quad_count"], 0);
    assert!(result.steps[1..].iter().all(|step| {
        step.status == "completed"
            && step.revision_after == 0
            && step.result["quad_count"] == 0
    }));
    assert!(result.failure.is_none());
    assert_eq!(result.final_state.as_ref().unwrap().quad_count, 0);
}

#[test]
fn source_session_result_budget_preserves_prefix_and_marks_unobserved_tail() {
    let mut input = request("S01_same_session");
    // Distinct subjects preserve all 1024 statements. Each value stays within
    // the original 4096-character limit; the total observations exceed 32 MiB.
    input.resources[0].data = (0..1024)
        .map(|i| format!("<urn:s{i}> <urn:p> \"{}\" .\n", "x".repeat(2048)))
        .collect();
    input.resources[0].binding.byte_count = input.resources[0].data.len() as u64;
    input.resources[0].binding.source_sha256 =
        semantica_runtime::sha256(input.resources[0].data.as_bytes());
    let snapshot = input.actions[3].clone();
    input.actions.truncate(1);
    for i in 0..16 {
        let mut action = snapshot.clone();
        action.id = format!("snapshot-{i}");
        input.actions.push(action);
    }
    let result = RustSemanticRuntime.execute_source_session(&input).unwrap();
    assert_eq!(result.status, "partial");
    assert_eq!(result.steps.len(), 17);
    assert_eq!(result.steps[0].status, "completed");
    assert_eq!(result.steps[0].result["added"], 1024);
    assert_eq!(result.steps[0].revision_after, 1);
    assert_eq!(result.steps[0].source_state["quad_count"], 1024);
    let capture = result
        .steps
        .iter()
        .position(|s| s.status == "failed")
        .unwrap();
    assert!(capture > 0 && capture + 1 < result.steps.len());
    assert_eq!(
        result.steps[capture].failure.as_ref().unwrap().code,
        "source_session_result_byte_budget"
    );
    assert!(result.steps[capture].result.is_null());
    assert!(
        result.steps[capture + 1..]
            .iter()
            .all(|s| s.status == "skipped")
    );
    assert_eq!(result.final_state.as_ref().unwrap().quad_count, 1024);
    assert!(serde_json::to_vec(&result).unwrap().len() <= MAX_OUTPUT_BYTES);
}

#[test]
fn source_session_clear_named_graph_keeps_other_contexts() {
    let mut input = request("S01_same_session");
    let data = "<urn:g1> {<urn:s1> <urn:p> <urn:o>} <urn:g2> {<urn:s2> <urn:p> <urn:o>}";
    input.resources[0].rdf_format = "trig".into();
    input.resources[0].data = data.into();
    input.resources[0].binding.byte_count = data.len() as u64;
    input.resources[0].binding.source_sha256 = semantica_runtime::sha256(data);
    let mut clear = input.actions[3].clone();
    clear.id = "clear-g1".into();
    clear.operation = "clear".into();
    clear.graph_name = Some("urn:g1".into());
    let mut snap = input.actions[3].clone();
    snap.id = "remaining".into();
    input.actions = vec![input.actions[0].clone(), clear, snap];
    let result = RustSemanticRuntime.execute_source_session(&input).unwrap();
    assert_eq!(result.status, "completed");
    assert_eq!(result.steps[1].result["removed"], 1);
    assert_eq!(result.steps[2].result["quad_count"], 1);
    assert_eq!(
        result.steps[2].result["canonical_nquads"],
        "<urn:s2> <urn:p> <urn:o> <urn:g2> ."
    );
}
