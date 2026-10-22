//! Additional, independently qualified original observations; original 884 stays intact.
//! Both tests finish every live action before opening any oracle or qualification report.
//! The template tape comes only from inputs frozen before either implementation ran.
use super::*;
use std::{cell::RefCell, rc::Rc};

const INPUTS_BLANK: &str = "source-update-additional-08/FROZEN-EXTERNAL-INPUTS-02.json";
const INPUTS_CONTROLS: &str = "source-update-additional-08/CONTROLS-INPUTS-01.json";
const QUALIFICATION: &str =
    "source-update-additional-08/ORIGINAL-OBSERVATIONS-INDEPENDENT-REVIEW-01.json";
const BLANK_CASES: [(&str, &str); 3] = [
    (
        "external_blank_input_ascending_v2",
        "source-update-additional-08/external_blank_input_ascending_v2.json",
    ),
    (
        "external_blank_input_descending_v2",
        "source-update-additional-08/external_blank_input_descending_v2.json",
    ),
    (
        "external_blank_input_nonmonotonic_v2",
        "source-update-additional-08/external_blank_input_nonmonotonic_v2.json",
    ),
];
const CONTROL_CASES: [(&str, &str); 2] = [
    (
        "literal_false_optional",
        "source-update-additional-08/literal_false_optional.json",
    ),
    (
        "duplicate_union_update",
        "source-update-additional-08/duplicate_union_update.json",
    ),
];

fn input(id: &str) -> Value {
    serde_json::from_str(&crate::private_test_fixtures::text(id)).unwrap()
}

/// This is a test-only external input source, not a blank-node implementation.
/// Actual allocation, template reuse, mutation and error handling remain owners' work.
#[derive(Clone)]
struct Tape {
    labels: Rc<Vec<String>>,
    calls: Rc<RefCell<Vec<Value>>>,
}
impl Tape {
    fn from_case(case: &Value) -> Self {
        assert_eq!(case["use_expected_output_as_input"], false);
        assert_eq!(
            case["expected_constructor_calls_by_role"],
            json!({"prologue_graph":1,"template":5})
        );
        assert_eq!(
            case["expected_role_order"],
            json!([
                "prologue_graph",
                "template",
                "template",
                "template",
                "template",
                "template"
            ])
        );
        let graph = case["construction_inputs"]["prologue_graph"]
            .as_array()
            .unwrap();
        let template = case["construction_inputs"]["template"].as_array().unwrap();
        assert_eq!(graph.len(), 1);
        assert_eq!(template.len(), 5);
        let mut unique = BTreeSet::new();
        for hex in graph.iter().chain(template) {
            let hex = hex.as_str().unwrap();
            assert_eq!(hex.len(), 32);
            assert!(
                hex.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            );
            assert_eq!(hex.as_bytes()[12], b'4');
            assert!(matches!(hex.as_bytes()[16], b'8' | b'9' | b'a' | b'b'));
            assert!(unique.insert(hex));
        }
        Self {
            labels: Rc::new(
                template
                    .iter()
                    .map(|h| format!("N{}", h.as_str().unwrap()))
                    .collect(),
            ),
            calls: Rc::new(RefCell::new(vec![])),
        }
    }
    fn constructor(&self) -> spargebra::SourceBlankNodeConstructor {
        let labels = self.labels.clone();
        let calls = self.calls.clone();
        Rc::new(move || {
            let mut calls = calls.borrow_mut();
            let ordinal = calls.len();
            let label = labels.get(ordinal).cloned();
            calls.push(json!({"ordinal":ordinal+1,"label":label}));
            let label =
                label.ok_or_else(|| std::io::Error::other("external_template_input_exhausted"))?;
            Ok(oxrdf::BlankNode::new(label)?)
        })
    }
    fn trace(&self) -> Value {
        json!(*self.calls.borrow())
    }
    fn expected_trace(&self) -> Value {
        // Input-derived trace, not an answer-derived choice of identifiers.
        json!(
            self.labels
                .iter()
                .enumerate()
                .map(|(i, label)| json!({"ordinal":i+1,"label":label}))
                .collect::<Vec<_>>()
        )
    }
}

fn execute_with_tape(
    session: &mut DatasetSession,
    action: &Value,
    resources: &Resources,
    loader_trace: &LoaderTrace,
    tape: &Tape,
) -> Result<Value> {
    if text(action, "operation")? != "update" {
        return execute(session, action, resources, loader_trace);
    }
    let namespaces = serde_json::from_value(action["namespaces"].clone())?;
    let bindings = serde_json::from_value(action["bindings"].clone())?;
    let query = action.get("query").context("update_query_field_missing")?;
    let mut loader = resources.loader(loader_trace.clone())?;
    let result = session.update_source_full_with_constructor_internal(
        query,
        &namespaces,
        &bindings,
        &mut loader,
        Some(tape.constructor()),
    )?;
    Ok(
        json!({"added":result.added,"removed":result.removed,"quad_count":result.quad_count,"revision":result.revision}),
    )
}

fn qualified_report() -> Value {
    let report = input(QUALIFICATION);
    assert_eq!(
        report["schema"],
        "private.original-observation-independent-oracle-review/1"
    );
    assert_eq!(
        report["status"],
        "original_observations_qualified_for_explicit_scope_not_rust_acceptance"
    );
    report
}

fn original(report: &Value, group: &str, case_id: &str, id: &str) -> Value {
    let groups: Vec<_> = report["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["group"] == group)
        .collect();
    assert_eq!(groups.len(), 1);
    let cases: Vec<_> = groups[0]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["case_id"] == case_id)
        .collect();
    assert_eq!(cases.len(), 1);
    let bytes = crate::private_test_fixtures::text(id);
    assert_eq!(cases[0]["stdout"]["bytes"], json!(bytes.len()));
    assert_eq!(cases[0]["stdout"]["sha256"], crate::sha256(&bytes));
    assert_eq!(cases[0]["actions"], 9);
    assert_eq!(cases[0]["all_completed"], true);
    let observation: Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(observation["case_id"], case_id);
    assert_eq!(observation["candidate_results_used"], false);
    assert_eq!(observation["historical_884_changed"], false);
    assert_eq!(observation["actions"].as_array().unwrap().len(), 9);
    observation
}

#[test]
fn source_update_external_constructor_replays_27_original_actions() {
    let inputs = input(INPUTS_BLANK);
    assert_eq!(
        inputs["schema"],
        "private.source-update-external-blank-inputs/2"
    );
    let cases = inputs["cases"].as_array().unwrap();
    let actions = inputs["original_input_actions"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    assert_eq!(actions.len(), 9);
    assert_eq!(
        actions
            .iter()
            .filter(|a| a["operation"] == "update")
            .count(),
        1
    );
    let resources = Resources::read().unwrap();
    let mut executions = Vec::new();
    for (case, (case_id, _)) in cases.iter().zip(BLANK_CASES) {
        assert_eq!(case["case_id"], case_id);
        assert_eq!(
            case["original_action_ids"],
            json!(actions.iter().map(|a| a["id"].clone()).collect::<Vec<_>>())
        );
        let tape = Tape::from_case(case);
        let mut session = DatasetSession::new_source_update_internal(true);
        let mut records = Vec::new();
        for action in actions {
            let record = observe_action_with(&mut session, action, |session, trace| {
                execute_with_tape(session, action, &resources, trace, &tape)
            });
            println!(
                "OE_SOURCE_UPDATE_ADDITIONAL_ACTUAL {}",
                json!({"group":"external_blank","case_id":case_id,"record":record})
            );
            records.push(record);
        }
        executions.push((records, tape.trace(), tape.expected_trace()));
    }
    assert_eq!(executions.iter().map(|e| e.0.len()).sum::<usize>(), 27);
    // The live phase above has no handle to the oracle/report or comparator.
    let report = qualified_report();
    let mut mismatches = Vec::new();
    for ((case_id, id), (records, trace, expected_trace)) in BLANK_CASES.iter().zip(&executions) {
        let observed = original(&report, "blank", case_id, id);
        assert_eq!(
            observed["constructor_role_counts"],
            json!({"prologue_graph":1,"template":5})
        );
        if trace != expected_trace {
            mismatches.push(format!("{case_id}:template_input_consumption"));
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
                json!({"group":"external_blank","case_id":case_id,"id":action["id"],"matched":matched,"expected":answer,"actual":record["actual"],"comparison":"strict_full_json_including_order_and_blank_identity"})
            );
        }
        println!(
            "OE_SOURCE_UPDATE_EXTERNAL_CONSTRUCTION {}",
            json!({"case_id":case_id,"original_role_counts":observed["constructor_role_counts"],"rust_template_calls":trace,"rust_prologue_calls":0,"original_prologue_allocation_has_no_rust_counterpart":true})
        );
    }
    println!(
        "OE_SOURCE_UPDATE_ADDITIONAL_SUMMARY {}",
        json!({"group":"external_blank","actions":27,"mismatch_ids":mismatches,"original_884_unchanged":true,"old_random_order_failures_closed":false,"exposure":"development_replay","engineering_adoption":false})
    );
    assert!(
        mismatches.is_empty(),
        "external input full-original mismatches: {mismatches:?}"
    );
}

#[test]
fn source_update_controls_replays_18_original_actions() {
    let inputs = input(INPUTS_CONTROLS);
    assert_eq!(
        inputs["schema"],
        "private.source-update-controls-original-inputs/1"
    );
    assert_eq!(inputs["independent_observation_actions"], 18);
    assert_eq!(inputs["random_constructor_injection"], false);
    let cases = inputs["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    let resources = Resources::read().unwrap();
    let mut executions = Vec::new();
    for (case, (case_id, _)) in cases.iter().zip(CONTROL_CASES) {
        assert_eq!(case["case_id"], case_id);
        let actions = case["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 9);
        let mut session = DatasetSession::new_source_update_internal(true);
        let mut records = Vec::new();
        for action in actions {
            let record = observe_action(&mut session, action, &resources);
            println!(
                "OE_SOURCE_UPDATE_ADDITIONAL_ACTUAL {}",
                json!({"group":"controls","case_id":case_id,"record":record})
            );
            records.push(record);
        }
        executions.push(records);
    }
    assert_eq!(executions.iter().map(Vec::len).sum::<usize>(), 18);
    let report = qualified_report();
    // Reuse the complete original comparator. It never reorders SELECT rows.
    let mut comparator = GlobalComparison::default();
    let mut mismatches = Vec::new();
    for (((case_id, id), case), records) in CONTROL_CASES.iter().zip(cases).zip(&executions) {
        let observed = original(&report, "controls", case_id, id);
        assert_eq!(observed["constructor_injection"], false);
        for ((action, answer), record) in case["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(observed["actions"].as_array().unwrap())
            .zip(records)
        {
            assert_eq!(action["id"], answer["id"]);
            assert_eq!(action["operation"], answer["operation"]);
            let compared = comparator.compare(answer, &record["actual"]);
            if compared["matched"] != true || record["loader_calls"] != json!([]) {
                mismatches.push(action["id"].clone());
            }
            println!(
                "OE_SOURCE_UPDATE_ADDITIONAL_COMPARISON {}",
                json!({"group":"controls","case_id":case_id,"id":action["id"],"comparison":compared,"expected":answer,"actual":record["actual"]})
            );
        }
    }
    println!(
        "OE_SOURCE_UPDATE_ADDITIONAL_SUMMARY {}",
        json!({"group":"controls","actions":18,"mismatch_ids":mismatches,"comparison":comparator.summary(),"original_884_unchanged":true,"exposure":"development_replay","engineering_adoption":false})
    );
    assert!(
        mismatches.is_empty(),
        "complete original control mismatches: {mismatches:?}"
    );
}

mod parser_originals;

mod composition_originals;

mod default_originals;

#[cfg(feature = "source-session-local-v1")]
mod session_originals;
