//! Mixed parser/template input, independently observed before this Rust receiving.
//! Reuse the existing owner and complete recorder; no new UPDATE algorithm.
use super::*;

const INPUTS: &str = "source-constructor-composition-27/INPUTS-01.json";
const PROJECTION: &str = "source-constructor-composition-27/FROZEN-CONSTRUCTION-PROJECTION-01.json";
const REVIEW: &str =
    "source-constructor-composition-27/ORIGINAL-OBSERVATIONS-INDEPENDENT-REVIEW-01.json";
const CASES: [(&str, &str); 2] = [
    (
        "constructor_composition_ascending_01",
        "source-constructor-composition-27/constructor_composition_ascending_01.json",
    ),
    (
        "constructor_composition_descending_01",
        "source-constructor-composition-27/constructor_composition_descending_01.json",
    ),
];

fn projected_tape(case: &Value, projected: &Value) -> Tape {
    assert_eq!(case["case_id"], projected["case_id"]);
    assert_eq!(case["actual_role_order"], Value::Null);
    assert_eq!(case["expected_output"], Value::Null);
    assert_eq!(
        case["predicted_counts_from_source_not_observed"],
        json!({"parser_anon":1,"parser_property_list":1,"prologue_graph":1,"template":1})
    );
    let pools = case["construction_inputs_by_role"].as_object().unwrap();
    assert_eq!(pools.len(), 4);
    let mut unique = BTreeSet::new();
    for role in [
        "parser_anon",
        "parser_property_list",
        "prologue_graph",
        "template",
    ] {
        let values = pools[role].as_array().unwrap();
        assert_eq!(values.len(), 1);
        let hex = values[0].as_str().unwrap();
        assert_eq!(hex.len(), 32);
        assert!(
            hex.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        );
        assert_eq!(hex.as_bytes()[12], b'4');
        assert!(matches!(hex.as_bytes()[16], b'8' | b'9' | b'a' | b'b'));
        assert!(unique.insert(hex));
    }
    let events = projected["events"].as_array().unwrap();
    assert_eq!(events.len(), 4);
    let mut seen = BTreeSet::new();
    let mut labels = Vec::new();
    for (index, event) in events.iter().enumerate() {
        let role = event["role"].as_str().unwrap();
        assert!(pools.contains_key(role) && seen.insert(role));
        assert_eq!(event["ordinal"], json!(index + 1));
        assert_eq!(event["role_ordinal"], 1);
        assert_eq!(event["input_hex"], pools[role][0]);
        if role != "prologue_graph" {
            // The value is selected from the pre-observation input pool. The
            // observed trace supplies order only, never answer blank labels.
            labels.push(format!("N{}", pools[role][0].as_str().unwrap()));
        }
    }
    assert_eq!(seen.len(), 4);
    assert_eq!(labels.len(), 3);
    Tape {
        labels: Rc::new(labels),
        calls: Rc::new(RefCell::new(vec![])),
    }
}

#[test]
fn source_update_composition_projection_rejects_missing_extra_or_exchanged_role_inputs() {
    let inputs = input(INPUTS);
    let projection = input(PROJECTION);
    let case = &inputs["cases"][0];
    let projected = &projection["cases"][0];
    assert_eq!(projected_tape(case, projected).labels.len(), 3);
    let mut missing = projected.clone();
    missing["events"].as_array_mut().unwrap().pop();
    let mut extra = projected.clone();
    extra["events"]
        .as_array_mut()
        .unwrap()
        .push(projected["events"][3].clone());
    let mut exchanged = projected.clone();
    let events = exchanged["events"].as_array_mut().unwrap();
    let parser = events
        .iter()
        .position(|v| v["role"] == "parser_anon")
        .unwrap();
    let template = events.iter().position(|v| v["role"] == "template").unwrap();
    let saved = events[parser]["input_hex"].clone();
    events[parser]["input_hex"] = events[template]["input_hex"].clone();
    events[template]["input_hex"] = saved;
    for rejected in [missing, extra, exchanged] {
        assert!(std::panic::catch_unwind(|| projected_tape(case, &rejected)).is_err());
    }
}

#[test]
fn source_update_composition_replays_18_original_actions() {
    let input_bytes = crate::private_test_fixtures::text(INPUTS);
    let inputs: Value = serde_json::from_str(&input_bytes).unwrap();
    let projection = input(PROJECTION);
    assert_eq!(
        inputs["schema"],
        "private.next-semantica-constructor-composition-plan/1"
    );
    assert_eq!(
        projection["schema"],
        "private.constructor-composition-input-projection/1"
    );
    assert_eq!(
        projection["original_inputs_sha256"],
        crate::sha256(&input_bytes)
    );
    assert_eq!(
        projection["output_values_used_as_constructor_inputs"],
        false
    );
    let cases = inputs["cases"].as_array().unwrap();
    let projected_cases = projection["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    assert_eq!(projected_cases.len(), 2);
    let resources = Resources::read().unwrap();
    let mut executions = Vec::new();
    for ((case, projected), (case_id, _)) in cases.iter().zip(projected_cases).zip(CASES) {
        assert_eq!(case["case_id"], case_id);
        let actions = case["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 9);
        assert_eq!(actions[4]["operation"], "update");
        assert_eq!(actions[4]["query"], case["complete_update_query"]);
        let tape = projected_tape(case, projected);
        let mut session = DatasetSession::new_source_update_internal(true);
        let mut records = Vec::new();
        for action in actions {
            let record = observe_action_with(&mut session, action, |session, trace| {
                execute_with_tape(session, action, &resources, trace, &tape)
            });
            println!(
                "OE_SOURCE_UPDATE_ADDITIONAL_ACTUAL {}",
                json!({"group":"composition","case_id":case_id,"record":record})
            );
            records.push(record);
        }
        executions.push((records, tape.trace(), tape.expected_trace()));
    }
    assert_eq!(executions.iter().map(|e| e.0.len()).sum::<usize>(), 18);
    // All live actions finish before any output oracle or qualification is opened.
    let review_bytes = crate::private_test_fixtures::text(REVIEW);
    assert_eq!(
        projection["qualified_review_sha256"],
        crate::sha256(&review_bytes)
    );
    let review: Value = serde_json::from_str(&review_bytes).unwrap();
    assert_eq!(
        review["schema"],
        "private.original-observation-independent-oracle-review/1"
    );
    assert_eq!(
        review["status"],
        "original_observations_qualified_for_explicit_scope_not_rust_acceptance"
    );
    let mut mismatches = Vec::new();
    for (((case, projected), (case_id, id)), (records, trace, expected_trace)) in cases
        .iter()
        .zip(projected_cases)
        .zip(CASES)
        .zip(&executions)
    {
        let observed = original(&review, "composition", case_id, id);
        assert_eq!(
            observed["schema"],
            "private.original-constructor-composition-observation/1"
        );
        assert_eq!(
            projected["original_observation_sha256"],
            crate::sha256(&crate::private_test_fixtures::text(id))
        );
        assert_eq!(
            observed["constructor_role_counts"],
            case["predicted_counts_from_source_not_observed"]
        );
        let events: Vec<_> = observed["constructor_trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                json!({"ordinal":e["ordinal"],"role":e["role"],
                "role_ordinal":e["role_ordinal"],"input_hex":e["input_hex"]})
            })
            .collect();
        assert_eq!(projected["events"], json!(events));
        if trace != expected_trace {
            mismatches.push(format!("{case_id}:input_consumption"));
        }
        let actions = case["actions"].as_array().unwrap();
        for ((action, answer), record) in actions
            .iter()
            .zip(observed["actions"].as_array().unwrap())
            .zip(records)
        {
            assert_eq!(action["id"], answer["id"]);
            assert_eq!(action["operation"], answer["operation"]);
            let matched = answer == &record["actual"] && record["loader_calls"] == json!([]);
            if !matched {
                mismatches.push(format!("{case_id}:{}", text(action, "id").unwrap()));
            }
            println!(
                "OE_SOURCE_UPDATE_ADDITIONAL_COMPARISON {}",
                json!({"group":"composition",
                "case_id":case_id,"id":action["id"],"matched":matched,"expected":answer,
                "actual":record["actual"],"comparison":"strict_full_json_including_order_and_blank_identity"})
            );
        }
        println!(
            "OE_SOURCE_UPDATE_EXTERNAL_CONSTRUCTION {}",
            json!({"case_id":case_id,
            "original_role_counts":observed["constructor_role_counts"],"original_events":events,
            "rust_calls":trace,"rust_prologue_calls":0,"input_projection_not_rust_role_observation":true})
        );
    }
    println!(
        "OE_SOURCE_UPDATE_ADDITIONAL_SUMMARY {}",
        json!({"group":"composition",
        "actions":18,"mismatch_ids":mismatches,"original_884_and_controlled63_unchanged":true,
        "old_random_order_failures_closed":false,"exposure":"development_replay",
        "engineering_adoption":false,"full_semantica_complete":false})
    );
    assert!(
        mismatches.is_empty(),
        "composition full-original mismatches: {mismatches:?}"
    );
}
