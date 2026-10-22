//! A separate explicit parser-input condition. Old 884 and additional 45 remain intact.
//! Only the two semantic parser roles are supplied; the original prologue role is not.
use super::*;

const PARSER_INPUTS: &str = "source-update-parser-08/FROZEN-EXTERNAL-INPUTS-01.json";
const PARSER_QUALIFICATION: &str =
    "source-update-parser-08/PARSER-ORIGINAL-OBSERVATIONS-INDEPENDENT-REVIEW-01.json";
const PARSER_CASES: [(&str, &str); 2] = [
    (
        "external_parser_blank_ascending_v1",
        "source-update-parser-08/external_parser_blank_ascending_v1.json",
    ),
    (
        "external_parser_blank_descending_v1",
        "source-update-parser-08/external_parser_blank_descending_v1.json",
    ),
];

impl Tape {
    fn from_parser_case(case: &Value) -> Self {
        assert_eq!(case["use_expected_output_as_input"], false);
        assert_eq!(case["expected_constructor_calls"], 3);
        assert_eq!(
            case["expected_constructor_calls_by_role"],
            json!({"parser_anon":1,"parser_property_list":1,"prologue_graph":1})
        );
        assert_eq!(
            case["expected_role_order"],
            json!(["parser_anon", "parser_property_list", "prologue_graph"])
        );
        let supplied = case["construction_inputs"].as_object().unwrap();
        assert_eq!(supplied.len(), 3);
        let mut unique = BTreeSet::new();
        let mut labels = Vec::new();
        for role in ["parser_anon", "parser_property_list", "prologue_graph"] {
            let values = supplied[role].as_array().unwrap();
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
            if role != "prologue_graph" {
                labels.push(format!("N{hex}"));
            }
        }
        assert_eq!(labels.len(), 2);
        Self {
            labels: Rc::new(labels),
            calls: Rc::new(RefCell::new(vec![])),
        }
    }
}

#[test]
fn source_update_external_parser_replays_18_original_actions() {
    let inputs = input(PARSER_INPUTS);
    assert_eq!(
        inputs["schema"],
        "private.source-update-parser-blank-inputs/1"
    );
    let cases = inputs["cases"].as_array().unwrap();
    let actions = inputs["original_input_actions"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    assert_eq!(actions.len(), 9);
    assert_eq!(
        actions.iter().map(|v| v["id"].clone()).collect::<Vec<_>>(),
        (73..82)
            .map(|i| json!(format!("UP{i:04}")))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| a["operation"] == "update")
            .count(),
        1
    );
    assert_eq!(actions[4]["id"], "UP0077");
    assert_eq!(
        actions[4]["query"],
        "INSERT DATA { [] <urn:p> [ <urn:q> \"nested\" ] }"
    );
    let resources = Resources::read().unwrap();
    let mut executions = Vec::new();
    for (case, (case_id, _)) in cases.iter().zip(PARSER_CASES) {
        assert_eq!(case["case_id"], case_id);
        assert_eq!(
            case["original_action_ids"],
            json!(actions.iter().map(|a| a["id"].clone()).collect::<Vec<_>>())
        );
        let tape = Tape::from_parser_case(case);
        let mut session = DatasetSession::new_source_update_internal(true);
        let mut records = Vec::new();
        for action in actions {
            let record = observe_action_with(&mut session, action, |session, trace| {
                execute_with_tape(session, action, &resources, trace, &tape)
            });
            println!(
                "OE_SOURCE_UPDATE_ADDITIONAL_ACTUAL {}",
                json!({"group":"external_parser","case_id":case_id,"record":record})
            );
            records.push(record);
        }
        executions.push((records, tape.trace(), tape.expected_trace()));
    }
    assert_eq!(executions.iter().map(|e| e.0.len()).sum::<usize>(), 18);
    // No oracle or qualification is opened until all18 real actions above finish.
    // Missing/unqualified future original data cannot be manufactured or skipped.
    let report = input(PARSER_QUALIFICATION);
    assert_eq!(
        report["schema"],
        "private.original-observation-independent-oracle-review/1"
    );
    assert_eq!(
        report["status"],
        "original_observations_qualified_for_explicit_scope_not_rust_acceptance"
    );
    let mut mismatches = Vec::new();
    for ((case_id, id), (records, trace, expected_trace)) in PARSER_CASES.iter().zip(&executions) {
        let observed = original(&report, "parser", case_id, id);
        assert_eq!(
            observed["schema"],
            "private.original-source-update-parser-blank-observation/1"
        );
        assert_eq!(
            observed["constructor_role_counts"],
            json!({"parser_anon":1,"parser_property_list":1,"prologue_graph":1})
        );
        if trace != expected_trace {
            mismatches.push(format!("{case_id}:parser_input_consumption"));
        }
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
                json!({"group":"external_parser","case_id":case_id,"id":action["id"],"matched":matched,"expected":answer,"actual":record["actual"],"comparison":"strict_full_json_including_order_and_blank_identity"})
            );
        }
        println!(
            "OE_SOURCE_UPDATE_EXTERNAL_CONSTRUCTION {}",
            json!({"case_id":case_id,"original_role_counts":observed["constructor_role_counts"],"rust_parser_calls":trace,"rust_prologue_calls":0,"original_prologue_allocation_has_no_rust_counterpart":true})
        );
    }
    println!(
        "OE_SOURCE_UPDATE_ADDITIONAL_SUMMARY {}",
        json!({"group":"external_parser","actions":18,"mismatch_ids":mismatches,"original_884_and_additional_45_unchanged":true,"old_random_order_failures_closed":false,"exposure":"development_replay","engineering_adoption":false})
    );
    assert!(
        mismatches.is_empty(),
        "external parser full-original mismatches: {mismatches:?}"
    );
}
