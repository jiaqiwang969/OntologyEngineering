//! Independent comparison mutations. Synthetic observations below only test the
//! pure assessor; they are not real process, source, scope or release evidence.
use crate::semantic_outcome::equivalent;
use oe_contracts::{CodeComparisonRequest, CodeReplayCase, CodeReplayObservation};
use oe_wire::json::{canonical_json, strict_json};
use serde_json::{Value, json};

const ORACLE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/q40-replay-cases.json"
));
fn cases() -> Vec<CodeReplayCase> {
    assert_eq!(
        oe_core::sha256(ORACLE),
        "fb556d112c0302389240f83b536c82aa7e5079a36be7c2cec2645a9d8a5b51ce"
    );
    serde_json::from_value(strict_json(ORACLE).unwrap()).unwrap()
}
fn case(id: &str) -> CodeReplayCase {
    cases().into_iter().find(|c| c.case_id == id).unwrap()
}
fn rejected(case: &CodeReplayCase, actual: &Value, label: &str) {
    assert!(
        !equivalent(case, actual).unwrap_or(false),
        "{} accepted {label}",
        case.case_id
    );
}

#[test]
fn independent_all40_typed_expectations_validate_without_runtime_link() {
    let all = cases();
    assert_eq!(all.len(), 40);
    assert_eq!(
        all.iter()
            .filter(|c| c.expected["outcome"] == "success")
            .count(),
        16
    );
    for c in all {
        assert!(equivalent(&c, &c.expected).unwrap(), "{}", c.case_id);
        let mut unknown = c.expected.clone();
        unknown["passed"] = json!(true);
        rejected(&c, &unknown, "caller-authored passed field");
        let mut hash = c.expected.clone();
        hash["request_sha256"] = json!("0".repeat(64));
        rejected(&c, &hash, "wrong request identity");
    }
}

#[test]
fn full_success_result_keeps_priority_confidence_premise_order_and_completeness() {
    let c = case("same_priority_first_input_retains_metadata");
    for (pointer, value) in [
        ("/result/request_sha256", json!("0".repeat(64))),
        ("/result/schema", json!("semantica.reason-result/0")),
        ("/result/data/rules/0/priority", json!(7.0)),
        ("/result/data/rules/0/priority", json!(true)),
        ("/result/data/rules/0/confidence", json!(0.9)),
        (
            "/result/data/inferences/0/premises",
            json!(["A(a)", "B(a)"]),
        ),
        ("/result/data/inferences/0/premises", json!(["B(a)"])),
        ("/result/data/inferences/0/confidence", json!(0.9)),
        ("/result/data/inferences/0/rule_id", json!("second")),
        ("/result/data/complete", json!(false)),
        ("/result/data/all_facts", json!(["A(a)", "B(a)"])),
        ("/stage", json!("request_decode")),
        ("/reason_port_invoked", json!(false)),
        ("/condition", json!("convergence")),
    ] {
        let mut changed = c.expected.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        rejected(&c, &changed, pointer);
    }
    let mut missing = c.expected.clone();
    missing["result"]["data"]
        .as_object_mut()
        .unwrap()
        .remove("complete");
    rejected(&c, &missing, "missing complete");
    let mut both = c.expected.clone();
    both["failure"] = case("priority_boolean").expected["failure"].clone();
    rejected(&c, &both, "success with failure");
    let mut neither = c.expected.clone();
    neither["result"] = Value::Null;
    rejected(&c, &neither, "success without result");
}

#[test]
fn rejection_requires_exact_condition_phase_and_preserved_failure() {
    for c in cases()
        .into_iter()
        .filter(|c| c.expected["outcome"] == "rejected")
    {
        for (pointer, value) in [
            ("/failure/phase", json!("parse")),
            ("/failure/code", json!("unrelated_failure")),
            ("/failure/retryable", json!(true)),
            ("/failure/message", json!("")),
            ("/condition", json!("not_an_authorized_condition")),
            ("/outcome", json!("success")),
            ("/stage", json!("result")),
        ] {
            let mut changed = c.expected.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            rejected(&c, &changed, pointer);
        }
        let mut data = c.expected.clone();
        data["result"] = case("strings_default_ids").expected["result"].clone();
        rejected(&c, &data, "partial data accompanying rejection");
        let mut none = c.expected.clone();
        none["failure"] = Value::Null;
        rejected(&c, &none, "rejection without failure");
        let mut other_message = c.expected.clone();
        other_message["failure"]["message"] = json!("runtime_reason_input: invalid rule syntax");
        if c.case_id == "max_iterations_bool_rejected" {
            // Any nonempty serde diagnostic is retained; typed stage, code and
            // the actual boolean input remain mandatory and are tested below.
            assert!(equivalent(&c, &other_message).unwrap());
            let mut forged_input = c.clone();
            forged_input.input["max_iterations"] = json!(50);
            forged_input.input_sha256 =
                oe_core::sha256(canonical_json(&forged_input.input).unwrap());
            other_message["request_sha256"] = json!(forged_input.input_sha256);
            forged_input.expected["request_sha256"] = other_message["request_sha256"].clone();
            rejected(
                &forged_input,
                &other_message,
                "decode rejection on non-boolean input",
            );
        } else if c.case_id != "empty_text_rejected_at_dto_construction" {
            rejected(&c, &other_message, "wrong input refusal reason");
        }
    }
    let c = case("bound_probe_detects_late_derivation");
    let wrong = case("priority_boolean").expected;
    rejected(&c, &wrong, "input error impersonates convergence");
}

// All paths below are synthetic and are never opened. This cannot mint a
// CodeBehaviorWitness; actual binary/source/process validation is a separate test.
fn synthetic_assessment(c: &CodeReplayCase) -> (CodeComparisonRequest, CodeReplayObservation) {
    let hash = "a".repeat(64);
    let binary = json!({"bytes":1,"path":"/synthetic/no-executable","sha256":hash,
        "source_manifest_sha256":hash,"source_root":"/synthetic/candidate"});
    let artifact = json!({"artifact_id":"synthetic-only","build_processes":[],"compiled":{},"features":[],
        "generation":{},"manifest":{},"manifest_sha256":hash,"platform":"synthetic","profile":"synthetic",
        "runner":binary,"source_root":"/synthetic/candidate","tools":[],"uncovered":["not execution evidence"]});
    let request: CodeComparisonRequest = serde_json::from_value(json!({
        "experiment":{"baseline":artifact,"candidate":artifact,"case_set_sha256":hash,"cases":[c],
            "conditions_sha256":c.conditions_sha256,"driver":binary,"evaluator":binary,"experiment_id":"synthetic-pure-assessor",
            "goal_sha256":hash,"oracle_path":"/synthetic/oracle","oracle_sha256":hash,"owner_connection":"not_connected",
            "protocol_case_ids":[],"protocol_expectations_path":"/synthetic/protocol","protocol_expectations_sha256":hash,
            "schema":"oev2.code-experiment/1","scope_sha256":hash,"uncovered":["not execution evidence"]},
        "observations":[]
    })).unwrap();
    let observation: CodeReplayObservation = serde_json::from_value(json!({
        "artifact_manifest_sha256":hash,"case_id":c.case_id,"cost":null,"elapsed_micros":1,
        "execution":"completed","invocation_sha256":crate::digest(&serde_json::to_value(crate::invocation(c, 0)).unwrap()).unwrap(),
        "observed":{"schema":"oev2.code-runner-observation/1","case_id":c.case_id,"input_sha256":c.input_sha256,
            "conditions_sha256":c.conditions_sha256,"compiled_manifest_sha256":hash,"execution":"completed",
            "result":c.expected,"coverage":c.checks,"unknown":["synthetic only"],"error":null},
        "process":{"synthetic_test_only":true},"run_id":0,"runner_sha256":hash,"side":"candidate",
        "stderr_bytes":0,"stdout_bytes":1,"unknown":["synthetic only"],"usage":null
    })).unwrap();
    (request, observation)
}
fn not_assessed_match(
    c: &CodeReplayCase,
    r: &CodeComparisonRequest,
    o: Option<&CodeReplayObservation>,
    label: &str,
) {
    let actual = crate::assess(c, 0, "candidate", o, r);
    assert!(
        !actual.is_ok_and(|v| v["status"] == "matched"),
        "matched {label}"
    );
}

#[test]
fn expected_semantic_rejection_never_matches_outer_failure_missing_checks_or_identity_drift() {
    for id in [
        "priority_boolean",
        "bound_probe_detects_late_derivation",
        "max_iterations_bool_rejected",
    ] {
        let c = case(id);
        let (r, o) = synthetic_assessment(&c);
        assert_eq!(
            crate::assess(&c, 0, "candidate", Some(&o), &r).unwrap()["status"],
            "matched"
        );
        not_assessed_match(&c, &r, None, "missing observation");
        for execution in ["failed", "not_run", "partial"] {
            let mut changed = o.clone();
            changed.execution = execution.into();
            not_assessed_match(&c, &r, Some(&changed), execution);
        }
        for execution in ["failed", "unsupported", "partial"] {
            let mut changed = o.clone();
            changed.observed.as_mut().unwrap().execution = execution.into();
            not_assessed_match(&c, &r, Some(&changed), execution);
        }
        let mut changed = o.clone();
        changed.observed = None;
        not_assessed_match(&c, &r, Some(&changed), "missing typed observation");
        let mut changed = o.clone();
        changed.observed.as_mut().unwrap().error = Some("process killed".into());
        not_assessed_match(&c, &r, Some(&changed), "outer runner error");
        let mut changed = o.clone();
        changed.observed.as_mut().unwrap().coverage.clear();
        not_assessed_match(&c, &r, Some(&changed), "missing necessary checks");
        let mut changed = o.clone();
        changed.runner_sha256 = "b".repeat(64);
        not_assessed_match(&c, &r, Some(&changed), "runner identity changed");
        let mut changed = o.clone();
        changed.artifact_manifest_sha256 = "b".repeat(64);
        not_assessed_match(&c, &r, Some(&changed), "source identity changed");
        let mut changed = o.clone();
        changed.invocation_sha256 = "b".repeat(64);
        not_assessed_match(&c, &r, Some(&changed), "invocation changed");
        let mut changed = o.clone();
        changed.observed.as_mut().unwrap().conditions_sha256 = "b".repeat(64);
        not_assessed_match(&c, &r, Some(&changed), "condition changed");
        let mut changed = o.clone();
        changed.elapsed_micros = None;
        not_assessed_match(&c, &r, Some(&changed), "missing resource observation");
        let mut changed = o;
        changed.stdout_bytes = Some(c.budget.output_bytes + 1);
        not_assessed_match(&c, &r, Some(&changed), "output over budget");
    }
}
