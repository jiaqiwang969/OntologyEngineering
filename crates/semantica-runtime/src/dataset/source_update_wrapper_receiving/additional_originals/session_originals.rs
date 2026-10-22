//! New public session port receives all original eighteen default actions.
//! Inputs are frozen independently; original answers are opened after both runs.
use super::*;
use semantica_contracts::SemanticSourceSessionRequest;

const INPUTS: &str = "source-default-blank-27/SOURCE-UPDATE-OBSERVATION-PLAN-01.json";
const ORIGINAL: &str = "source-default-blank-27/ORIGINAL-OUTPUT-02.json";
const REVIEW: &str = "source-default-blank-27/ORIGINAL-RECEIVED-02.json";

fn request(actions: &[Value]) -> SemanticSourceSessionRequest {
    assert_eq!(actions.len(), 9);
    assert_eq!(actions[0]["operation"], "new");
    assert_eq!(actions[0]["profile"], "rdf");
    let mut resources = Vec::new();
    let mut steps = Vec::new();
    for action in &actions[1..] {
        let op = action["operation"].as_str().unwrap();
        let mut resource_id = Value::Null;
        if op == "load" {
            let data = action["data"].as_str().unwrap();
            resource_id = json!(format!(
                "urn:source-session:{}",
                action["id"].as_str().unwrap()
            ));
            resources.push(json!({"binding":{
                "schema":"semantica.source-binding/1", "source_id":resource_id,
                "source_sha256":crate::sha256(data), "byte_count":data.len()
            },"data":data,"rdf_format":action["format"]}));
        }
        let bindings: Vec<_> = action
            .get("bindings")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|v| v.iter())
            .map(|(name, term)| {
                let mut term = term.clone();
                term["name"] = json!(name);
                term
            })
            .collect();
        steps.push(json!({"id":action["id"],"operation":if op == "select" {"query"} else {op},
            "resource_id":resource_id,"query":action.get("query"),
            "namespaces":action.get("namespaces").cloned().unwrap_or(json!({})),
            "bindings":bindings,"base_uri":action.get("base_uri"),"graph_name":action.get("graph_name"),
            "use_cache":action.get("use_cache").and_then(Value::as_bool).unwrap_or(false)}));
    }
    let at = actions
        .iter()
        .find(|a| a["operation"] == "snapshot")
        .unwrap()["created_at"]
        .clone();
    assert!(
        actions
            .iter()
            .filter(|a| a["operation"] == "snapshot")
            .all(|a| a["created_at"] == at)
    );
    serde_json::from_value(json!({"schema":"semantica.source-session-request/1",
        "profile":"semantica.rdflib760-source-session/1","cache_queries":actions[0]["cache_queries"],
        "created_at":at,"resources":resources,"actions":steps})).unwrap()
}

#[test]
fn source_session_receives_all_18_original_actions_and_exact_default_identity() {
    let input_bytes = crate::private_test_fixtures::text(INPUTS);
    let plan: Value = serde_json::from_str(&input_bytes).unwrap();
    assert_eq!(plan["cases"], 2);
    assert_eq!(plan["candidate_results_used"], false);
    let actions = plan["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 18);
    let mut records = Vec::new();
    // Neither original output nor its receiving report has been read here.
    for case in actions.chunks_exact(9) {
        let request = request(case);
        let result = crate::runtime::RustSemanticRuntime
            .execute_source_session(&request)
            .unwrap();
        assert_eq!(result.status, "completed");
        assert_eq!(result.steps.len(), 8);
        assert!(result.initial_state_failure.is_none());
        records.push(
            json!({"id":case[0]["id"],"operation":"new","status":"completed",
            "raw":result.initial_result,"state":result.initial_state}),
        );
        for (original_action, step) in case[1..].iter().zip(&result.steps) {
            assert_eq!(step.status, "completed");
            assert!(step.source_state_failure.is_none());
            assert!(step.resource_requests_complete && step.resource_requests.is_empty());
            let mut raw = step.result.clone();
            match original_action["operation"].as_str().unwrap() {
                "select" => {
                    raw = step.result["result"].clone();
                    raw["revision"] = step.result["revision"].clone();
                    raw["cache_hit"] = step.result["cache_hit"].clone();
                }
                "update" => {
                    raw.as_object_mut().unwrap().remove("operation");
                }
                _ => {}
            }
            records.push(
                json!({"id":step.id,"operation":original_action["operation"],
                "status":"completed","raw":raw,"state":step.source_state}),
            );
        }
        assert_eq!(result.steps[6].operation, "clear");
        assert_eq!(result.steps[6].revision_after, 3);
        assert_eq!(result.steps[7].result["quad_count"], 0);
        assert_eq!(result.steps[7].result["canonical_nquads"], "");
        assert_eq!(result.steps[7].result["revision"], 3);
    }
    assert_eq!(records.len(), 18);
    for record in &records {
        println!("OE_SOURCE_SESSION_ORIGINAL_ACTUAL {}", record);
    }
    let review = input(REVIEW);
    let original_bytes = crate::private_test_fixtures::text(ORIGINAL);
    let original: Value = serde_json::from_str(&original_bytes).unwrap();
    assert_eq!(review["status"], "received_original_frozen_expectations");
    assert_eq!(review["original_actions"], 18);
    assert_eq!(review["old_UP0078_UP0726_closed"], false);
    assert_eq!(review["inputs"]["bytes"], input_bytes.len());
    assert_eq!(review["inputs"]["sha256"], crate::sha256(&input_bytes));
    assert_eq!(review["raw_output"]["bytes"], original_bytes.len());
    assert_eq!(
        review["raw_output"]["sha256"],
        crate::sha256(&original_bytes)
    );
    assert_eq!(original["plan_sha256"], crate::sha256(&input_bytes));
    assert_eq!(original["candidate_results_used"], false);
    let checks = review["checks"].as_array().unwrap();
    assert!(!checks.is_empty() && checks.iter().all(|c| c["passed"] == true));
    let answers = original["actions"].as_array().unwrap();
    assert_eq!(answers.len(), 18);
    let mut comparison = GlobalComparison::default();
    let mut mismatches = Vec::new();
    for (index, ((action, answer), actual)) in actions.iter().zip(answers).zip(&records).enumerate()
    {
        assert_eq!(action["id"], answer["id"]);
        assert_eq!(action["operation"], answer["operation"]);
        let identity = if matches!(index % 9, 4..=6) {
            assert!(default_originals::exact_default_identity(answer));
            default_originals::exact_default_identity(actual)
        } else {
            true
        };
        let compared = comparison.compare(answer, actual);
        let matched = compared["matched"] == true && identity;
        if !matched {
            mismatches.push(action["id"].clone());
        }
        println!(
            "OE_SOURCE_SESSION_ORIGINAL_COMPARISON {}",
            json!({"id":action["id"],
            "comparison":compared,"exact_caller_and_fresh_identity":identity,
            "matched":matched,"expected":answer,"actual":actual})
        );
    }
    println!(
        "OE_SOURCE_SESSION_ORIGINAL_SUMMARY {}",
        json!({"actions":18,
        "mismatch_ids":mismatches,"comparison":comparison.summary(),
        "input_constructor_override":false,"old_884_failures_closed":false,
        "engineering_adoption":false,"full_semantica_complete":false})
    );
    assert!(
        mismatches.is_empty(),
        "session port original mismatches: {mismatches:?}"
    );
}
