//! Ordinary default constructors: independent original18, no supplied UUID tape.
//! Existing wrapper execution/recording/comparison remain the only owners.
use super::*;

const INPUTS: &str = "source-default-blank-27/SOURCE-UPDATE-OBSERVATION-PLAN-01.json";
const ORIGINAL: &str = "source-default-blank-27/ORIGINAL-OUTPUT-02.json";
const REVIEW: &str = "source-default-blank-27/ORIGINAL-RECEIVED-02.json";

fn term(kind: &str, value: &str) -> Value {
    json!({"value":value,"term_type":kind,"datatype":null,"language":null})
}

/// Caller identity is exact, independently of the comparator's fresh-node bijection.
/// This inspects observed data; it never allocates a node or changes the live store.
pub(super) fn exact_default_identity(record: &Value) -> bool {
    let Some(quads) = record["state"]["quads"].as_array() else { return false; };
    let predicate = term("iri", "urn:kind");
    let anchor = term("literal", "anchor");
    let fresh = term("literal", "fresh");
    let anchors: Vec<_> = quads.iter().filter(|q| q.get(1) == Some(&predicate) && q.get(2) == Some(&anchor)).collect();
    let generated: Vec<_> = quads.iter().filter(|q| q.get(1) == Some(&predicate) && q.get(2) == Some(&fresh)).collect();
    if anchors.len() != 1 || generated.len() != 1 { return false; }
    if anchors[0][0] != term("blank_node", "Nzz") || anchors[0][3] != Value::Null || generated[0][3] != Value::Null { return false; }
    let Some(name) = generated[0][0]["value"].as_str() else { return false; };
    let bytes = name.as_bytes();
    if bytes.len() != 33 || bytes[0] != b'N' || !bytes[1..].iter().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)) { return false; }
    bytes[13] == b'4' && matches!(bytes[17], b'8' | b'9' | b'a' | b'b')
        && generated[0][0] == term("blank_node", name)
}

#[test]
fn source_update_default_constructor_replays_18_original_actions() {
    let input_bytes = crate::private_test_fixtures::text(INPUTS);
    let inputs: Value = serde_json::from_str(&input_bytes).unwrap();
    assert_eq!(inputs["schema"], "private.source-update-original-plan/1");
    assert_eq!(inputs["cases"], 2);
    assert_eq!(inputs["candidate_results_used"], false);
    let actions = inputs["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 18);
    let resources = Resources::read().unwrap();
    let mut session = DatasetSession::new_source_update_internal(true);
    let mut records = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        let case_id = ["default_template", "default_parser_control"][index / 9];
        if index % 9 == 0 {
            assert_eq!(action["operation"], "new");
            assert_eq!(action["case_label"], case_id);
        }
        if action["operation"] == "update" {
            assert_eq!(action["bindings"], json!({"anchor":term("blank_node", "Nzz")}));
        }
        // Existing execute calls update_source_full_internal: ordinary None path.
        // No constructor tape, oracle, or comparison result reaches execution.
        let record = observe_action(&mut session, action, &resources);
        println!("OE_SOURCE_UPDATE_ADDITIONAL_ACTUAL {}", json!({
            "group":"default_constructor", "case_id":case_id, "record":record
        }));
        records.push(record);
    }
    assert_eq!(records.len(), 18);
    // All live actions finish before either qualification or output is opened.
    let review = input(REVIEW);
    let original_bytes = crate::private_test_fixtures::text(ORIGINAL);
    let original: Value = serde_json::from_str(&original_bytes).unwrap();
    assert_eq!(review["schema"], "private.source-default-blank-original-receiving/1");
    assert_eq!(review["status"], "received_original_frozen_expectations");
    assert_eq!(review["original_actions"], 18);
    assert_eq!(review["old_UP0078_UP0726_closed"], false);
    let checks = review["checks"].as_array().unwrap();
    assert!(!checks.is_empty() && checks.iter().all(|c| c["passed"] == true));
    assert_eq!(review["inputs"]["bytes"], json!(input_bytes.len()));
    assert_eq!(review["inputs"]["sha256"], crate::sha256(&input_bytes));
    assert_eq!(review["raw_output"]["bytes"], json!(original_bytes.len()));
    assert_eq!(review["raw_output"]["sha256"], crate::sha256(&original_bytes));
    assert_eq!(original["schema"], "private.original-load-update-observations/1");
    assert_eq!(original["plan_sha256"], crate::sha256(&input_bytes));
    assert_eq!(original["candidate_results_used"], false);
    let answers = original["actions"].as_array().unwrap();
    assert_eq!(answers.len(), 18);
    let mut comparator = GlobalComparison::default();
    let mut mismatches = Vec::new();
    for (index, ((action, answer), record)) in actions.iter().zip(answers).zip(&records).enumerate() {
        assert_eq!(action["id"], answer["id"]);
        assert_eq!(action["operation"], answer["operation"]);
        assert_eq!(answer["status"], "completed");
        let actual = &record["actual"];
        let identity = if matches!(index % 9, 4..=6) {
            assert!(exact_default_identity(answer), "original caller/fresh identity");
            exact_default_identity(actual)
        } else { true };
        let comparison = comparator.compare(answer, actual);
        let matched = comparison["matched"] == true && identity && record["loader_calls"] == json!([]);
        if !matched { mismatches.push(action["id"].clone()); }
        println!("OE_SOURCE_UPDATE_ADDITIONAL_COMPARISON {}", json!({
            "group":"default_constructor", "case_id":(["default_template", "default_parser_control"][index/9]),
            "id":action["id"], "comparison":comparison, "exact_caller_and_fresh_identity":identity,
            "matched":matched, "expected":answer, "actual":actual
        }));
    }
    println!("OE_SOURCE_UPDATE_ADDITIONAL_SUMMARY {}", json!({
        "group":"default_constructor", "actions":18, "mismatch_ids":mismatches,
        "comparison":comparator.summary(), "constructor_override":false,
        "original_884_and_prior_additional_unchanged":true, "old_random_order_failures_closed":false,
        "exposure":"development_replay", "engineering_adoption":false, "full_semantica_complete":false
    }));
    assert!(mismatches.is_empty(), "default constructor full-original mismatches: {mismatches:?}");
}

#[test]
fn source_update_default_identity_controls_reject_renamed_anchor_and_reordered_or_missing_rows() {
    let original = input(ORIGINAL);
    let expected = &original["actions"][5];
    assert!(exact_default_identity(expected));
    assert_eq!(GlobalComparison::default().compare(expected, expected)["matched"], true);
    let mut renamed = expected.clone();
    for quad in renamed["state"]["quads"].as_array_mut().unwrap() {
        if quad[0] == term("blank_node", "Nzz") { quad[0] = term("blank_node", "Nzz_changed"); }
    }
    // Isomorphism alone would accept the caller-name change: exact identity must not.
    assert_eq!(GlobalComparison::default().compare(expected, &renamed)["matched"], true);
    assert!(!exact_default_identity(&renamed));
    let mut reordered = expected.clone();
    reordered["raw"]["rows"].as_array_mut().unwrap().reverse();
    assert_eq!(GlobalComparison::default().compare(expected, &reordered)["matched"], false);
    let mut missing = expected.clone();
    missing["raw"]["rows"].as_array_mut().unwrap().pop();
    assert_eq!(GlobalComparison::default().compare(expected, &missing)["matched"], false);
}
