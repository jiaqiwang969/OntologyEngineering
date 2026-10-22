use oe_contracts::{
    ArtifactReference, DesignComparisonRule, EngineeringCalculation,
    EngineeringExplorationRunRequest,
};
use oe_core::{SkillRoot, sha256};
use oe_engineering::workflow::exploration;
use oe_kernel::project::{Project, context};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn pin(root: &SkillRoot, name: &str, value: &Value) -> ArtifactReference {
    let p = root.path().join(name);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let raw = serde_json::to_vec_pretty(value).unwrap();
    fs::write(&p, &raw).unwrap();
    ArtifactReference {
        path: name.into(),
        sha256: sha256(raw),
    }
}
fn q(value: f64, unit: &str) -> Value {
    json!({"value":value,"unit":unit,"precision":"independent declared input"})
}

fn host_input(
    f: &Fixture,
    frozen: &ArtifactReference,
    previous: Option<&ArtifactReference>,
    receipt: Value,
    name: &str,
) -> ArtifactReference {
    pin(
        &f.root,
        &format!("var/{name}-input.json"),
        &json!({
            "schema":"oev2.exploration-host-input/1","frozen":frozen,
            "previous":previous,"receipt":receipt,"output":format!("var/{name}")
        }),
    )
}
fn host_reference(f: &Fixture, name: &str) -> ArtifactReference {
    let path = format!("var/{name}/completed.json");
    ArtifactReference {
        sha256: sha256(fs::read(f.root.path().join(&path)).unwrap()),
        path,
    }
}
fn host_observation(
    f: &Fixture,
    frozen: &ArtifactReference,
    action: &str,
    execution: &str,
    name: &str,
) -> Value {
    let artifact = pin(
        &f.root,
        &format!("var/{name}.json"),
        &json!({
            "schema":"oev2.host-source-observation/1","frozen":frozen,"action_id":action,
            "project":f.work["project"],"object":f.work["object"],"execution":execution,
            "observation":{"registered":false},"sources":[f.plan["observations"][0]],
            "limitations":["synthetic source statement; physical and native acceptance remain unknown"]
        }),
    );
    json!({"action_id":action,"owner":"source_statement","artifact":artifact})
}

#[test]
fn host_chain_keeps_pending_results_history_and_original_guards() {
    let mut f = Fixture::new();
    f.plan["actions"][0]["kind"] = json!("external");
    f.plan["actions"][0]["source"] = Value::Null;
    let mut second = f.plan["actions"][0].clone();
    second["action_id"] = json!("confirm");
    second["when_outcomes"] = json!(["absent-registration"]);
    f.plan["actions"].as_array_mut().unwrap().push(second);
    let mut more = f.plan["outcomes"].as_array().unwrap().clone();
    for (i, o) in more.iter_mut().enumerate() {
        o["action_id"] = json!("confirm");
        o["outcome_id"] = json!(format!("confirmation-{i}"));
    }
    f.plan["outcomes"].as_array_mut().unwrap().extend(more);
    f.plan["independent_expected"] = json!(pin(
        &f.root,
        "var/expected.json",
        &json!({
            "schema":"oev2.exploration-expectations/1","exposure":"development_replay",
            "quality":{"preserve_complete_acceptance":true,"action_order":["inspect","confirm"],"decision_conclusions":{"registration":"supported"}},
            "cost":{"maximum_actions":2,"maximum_model_calls":0}
        })
    ));
    let frozen = f.freeze();
    let original = fs::read(f.root.path().join("var/project.json")).unwrap();
    let start = host_input(&f, &frozen, None, Value::Null, "host-start");
    let pending = exploration::host::advance(&f.root, &start).unwrap();
    assert_eq!(pending.status, "requires_host_agent");
    assert_eq!(
        f.read("var/host-start/run/assessment.json")["behavior_quality"],
        "pending_verification"
    );
    assert_eq!(pending.pending.len(), 1);
    assert_eq!(pending.pending[0]["action"]["action_id"], "inspect");
    assert_eq!(
        f.read(&pending.run.result.path)["decision_conclusions"]["registration"],
        "unknown"
    );
    let before = host_reference(&f, "host-start");
    let receipt = host_observation(&f, &frozen, "inspect", "completed", "host-result");
    let mut skipped = receipt.clone();
    skipped["action_id"] = json!("confirm");
    assert!(
        exploration::host::advance(
            &f.root,
            &host_input(&f, &frozen, Some(&before), skipped, "skipped")
        )
        .is_err()
    );
    assert!(!f.root.path().join("var/skipped").exists());
    assert!(
        exploration::host::advance(
            &f.root,
            &host_input(&f, &frozen, None, receipt.clone(), "premature")
        )
        .is_err()
    );
    let next = exploration::host::advance(
        &f.root,
        &host_input(&f, &frozen, Some(&before), receipt.clone(), "host-next"),
    )
    .unwrap();
    assert_eq!(next.status, "requires_host_agent");
    assert_eq!(next.pending[0]["action"]["action_id"], "confirm");
    assert_eq!(next.receipts.len(), 1);
    let next_ref = host_reference(&f, "host-next");
    assert!(
        exploration::host::advance(
            &f.root,
            &host_input(&f, &frozen, Some(&next_ref), receipt, "duplicate")
        )
        .is_err()
    );
    let second = host_observation(&f, &frozen, "confirm", "completed", "host-second-result");
    let end = exploration::host::advance(
        &f.root,
        &host_input(&f, &frozen, Some(&next_ref), second.clone(), "host-end"),
    )
    .unwrap();
    assert_eq!(end.status, "results_recorded");
    assert!(end.pending.is_empty());
    assert_eq!(end.receipts.len(), 2);
    assert_eq!(
        f.read("var/host-end/run/assessment.json")["behavior_quality"],
        "supported_for_frozen_replay"
    );
    assert_eq!(
        f.read(&end.run.result.path)["decision_conclusions"]["registration"],
        "supported"
    );
    assert!(!end.adopted);
    assert_eq!(end.method_benefit, "unknown");
    assert_eq!(
        fs::read(f.root.path().join("var/project.json")).unwrap(),
        original
    );
    // A failure cannot be discarded by editing the previous serialized state.
    let mut altered = f.read(&next_ref.path);
    altered["receipts"] = json!([]);
    let altered_ref = pin(&f.root, &next_ref.path, &altered);
    assert!(
        exploration::host::advance(
            &f.root,
            &host_input(
                &f,
                &frozen,
                Some(&altered_ref),
                second.clone(),
                "dropped-history"
            )
        )
        .is_err()
    );
    // Even with an old valid state, changed live evidence must fail before writing.
    pin(&f.root, &next_ref.path, &json!(next));
    fs::write(f.root.path().join("var/source.json"), b"{}").unwrap();
    assert!(
        exploration::host::advance(
            &f.root,
            &host_input(&f, &frozen, Some(&next_ref), second, "drift")
        )
        .is_err()
    );
}

#[test]
fn host_failed_partial_and_causal_observations_keep_unknown() {
    for (execution, kind) in [
        ("failed", "source_statement"),
        ("partial", "source_statement"),
        ("unknown", "source_statement"),
        ("completed", "causal_hypothesis"),
        ("completed", "model_prediction"),
    ] {
        let mut f = Fixture::new();
        f.plan["actions"][0]["kind"] = json!("external");
        f.plan["actions"][0]["source"] = Value::Null;
        f.plan["hypotheses"][0]["kind"] = json!(kind);
        let frozen = f.freeze();
        exploration::host::advance(
            &f.root,
            &host_input(&f, &frozen, None, Value::Null, "start"),
        )
        .unwrap();
        let previous = host_reference(&f, "start");
        let result = host_observation(&f, &frozen, "inspect", execution, "result");
        let state = exploration::host::advance(
            &f.root,
            &host_input(&f, &frozen, Some(&previous), result, "end"),
        )
        .unwrap();
        assert_eq!(state.status, "needs_review");
        assert_eq!(
            f.read(&state.run.result.path)["decision_conclusions"]["registration"],
            "unknown"
        );
        assert_eq!(state.receipts.len(), 1);
        assert!(!state.adopted);
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    root: SkillRoot,
    project: Project,
    work: Value,
    plan: Value,
}
impl Fixture {
    fn new() -> Self {
        Self::source(json!({"registered":false,"metric":q(0.03,"mm")}))
    }
    fn source(source_value: Value) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("SKILL.md"),
            "synthetic engineering inquiry test",
        )
        .unwrap();
        let root = SkillRoot::new(dir.path().canonicalize().unwrap()).unwrap();
        fs::create_dir_all(root.path().join("specs/owner-schemas")).unwrap();
        fs::write(
            root.path()
                .join("specs/owner-schemas/engineering-exploration.json"),
            include_bytes!("../../../specs/owner-schemas/engineering-exploration.json"),
        )
        .unwrap();
        let source = pin(&root, "var/source.json", &source_value);
        let purpose = "Diagnose source registration while retaining complete software and physical engineering obligations";
        let acceptance = json!([
            "Register every frozen query source without changing its bytes",
            "Retain all original query expectations and platform obligations",
            "Physical169 each crosshair-to-lens distance strictly <0.03mm"
        ]);
        let object = json!({"object_id":"query-owner","revision":"snapshot1","condition_scope":"inquiry","source_sha256":source.sha256});
        let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"query-owner","object_revision":"snapshot1",
            "purpose":purpose,"acceptance":acceptance,"facts":[],"assertions":[]});
        let project:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"query-inquiry","revision":"r1",
            "purpose":purpose,"objects":{"query-owner":"snapshot1"},"conditions":{"inquiry":basis.to_string()},
            "evidence":{"source":source.sha256},"dependencies":{},"decisions":[{"id":"registration","object_id":"query-owner",
                "object_revision":"snapshot1","condition_scope":"inquiry","source_sha256":source.sha256,
                "conclusion":"unknown","adopted":false,"basis":["source"]}]})).unwrap();
        let project_ref = pin(&root, "var/project.json", &json!(project));
        let work = json!({"schema":"oev2.engineering-work/3","project":project_ref,"object":object,
            "project_context":context(&project,&serde_json::from_value(object.clone()).unwrap()).unwrap(),
            "acceptance":acceptance,"necessary_functions":["retain source identity"],"physical_mechanisms":["current Rust input inventory"],
            "gaps":[{"gap_id":"missing-registration","question":"Does the current source observation include registration?",
                "necessary_function":"bind frozen expectations","physical_mechanism":"explicit Rust source inventory",
                "required_conditions":["this exact source snapshot only"],"needed_evidence":["actual owner output, original source SHA and receiving result"]}],
            "source_requests":[],"calculations":[],"handoff":null});
        let work_ref = pin(&root, "var/work.json", &work);
        let expected = pin(
            &root,
            "var/expected.json",
            &json!({"schema":"oev2.exploration-expectations/1","exposure":"development_replay",
            "quality":{"preserve_complete_acceptance":true,"action_order":["inspect"],"decision_conclusions":{"registration":"supported"}},
            "cost":{"maximum_actions":2,"maximum_model_calls":0}}),
        );
        let plan = json!({"schema":"oev2.engineering-exploration/1","exploration_id":"registration-probe","parent_work":work_ref,
            "comparison_mode":"mechanism","parameter_rules":[],"comparison_extension_reason":"A source inventory inquiry requires explicit result branches outside the old controlled-calculation comparison.",
            "prior_contract":null,"observations":[source],"unknowns":["physical169 calibration, coverage and uncertainty remain open"],
            "candidates":[{"candidate_id":"explicit-inventory","work":work_ref,"principle":"bind source expectations explicitly",
                "representation":"current Rust inventory","applicability":["this query owner and original fixture snapshot"],
                "source_refs":[source],"remaining_obligations":acceptance}],
            "hypotheses":[{"hypothesis_id":"missing","decision_id":"registration","candidate_id":"explicit-inventory",
                "gap_id":"missing-registration","statement":"Registration is absent in this bound source observation.",
                "kind":"source_statement","grounding":"grounded_explanation","source_refs":[source],"scope":["frozen inventory snapshot only"]}],
            "actions":[{"action_id":"inspect","candidate_id":"explicit-inventory","gap_id":"missing-registration",
                "requirement":acceptance[0],"kind":"readback","calculation_id":null,"source":source,"pointer":"/observation/registered",
                "hypotheses":["missing"],"purpose":"Distinguish source registration present from absent.",
                "expected_impact":"Absent reopens registration; present retains this scoped route.","required_facts":[],"when_outcomes":[],
                "selection_reason":"A required source gap has two distinguishable branches; local read is feasible; external cost is unknown.",
                "cost_basis":"unknown","cost_sources":[]}],
            "outcomes":[{"outcome_id":"absent-registration","action_id":"inspect","operator":"eq","value":false,
                "supported":["missing"],"rejected":[],"review":[],"retained_candidates":[],"rejected_candidates":[],"review_candidates":["explicit-inventory"]},
                {"outcome_id":"present-registration","action_id":"inspect","operator":"eq","value":true,
                "supported":[],"rejected":["missing"],"review":[],"retained_candidates":["explicit-inventory"],"rejected_candidates":[],"review_candidates":[]}],
            "independent_expected":expected,"exposure":"development_replay"});
        Self {
            _dir: dir,
            root,
            project,
            work,
            plan,
        }
    }
    fn sync(&mut self) {
        let project_ref = pin(&self.root, "var/project.json", &json!(self.project));
        self.work["project"] = json!(project_ref);
        self.work["project_context"] = json!(
            context(
                &self.project,
                &serde_json::from_value(self.work["object"].clone()).unwrap()
            )
            .unwrap()
        );
        let work = pin(&self.root, "var/work.json", &self.work);
        self.plan["parent_work"] = json!(work);
        self.plan["candidates"][0]["work"] = json!(work);
    }
    fn input(&self) -> ArtifactReference {
        pin(&self.root, "var/plan.json", &self.plan)
    }
    fn freeze(&self) -> ArtifactReference {
        exploration::freeze(&self.root, &self.input(), Path::new("var/frozen")).unwrap();
        let raw = fs::read(self.root.path().join("var/frozen/completed.json")).unwrap();
        ArtifactReference {
            path: "var/frozen/completed.json".into(),
            sha256: sha256(raw),
        }
    }
    fn run(&self) -> oe_contracts::EngineeringExplorationRunResult {
        exploration::execute(
            &self.root,
            &EngineeringExplorationRunRequest {
                frozen: self.freeze(),
                output: "var/run".into(),
            },
        )
        .unwrap()
    }
    fn read(&self, name: &str) -> Value {
        serde_json::from_slice(&fs::read(self.root.path().join(name)).unwrap()).unwrap()
    }
    fn basis(&self) -> Value {
        serde_json::from_str(&self.project.conditions["inquiry"]).unwrap()
    }
    fn set_basis(&mut self, basis: Value) {
        self.project
            .conditions
            .insert("inquiry".into(), basis.to_string());
        self.sync();
    }
    fn model(&mut self) {
        let object: oe_contracts::ObjectIdentity =
            serde_json::from_value(self.work["object"].clone()).unwrap();
        let inputs = json!({"force":q(10.0,"N"),"span":q(100.0,"mm"),"elastic_modulus":q(200000.0,"MPa"),"second_moment":q(1000.0,"mm4")});
        let calc = json!({"calculation_id":"probe","object":object,"method":"beam-cantilever-end-load/1","inputs":inputs,
            "assumptions":["declared linear elastic beam"],"validity_conditions":["small displacement and fixed-free end loading"],
            "invalid_when":["actual support or load differs"],"source_refs":["source"]});
        self.work["calculations"] = json!([calc]);
        let mut basis = self.basis();
        basis["facts"] = json!([{"property":"force","value":q(10.0,"N"),"status":"declared","source_refs":["source"]},
            {"property":"span","value":q(100.0,"mm"),"status":"declared","source_refs":["source"]},
            {"property":"elastic_modulus","value":q(200000.0,"MPa"),"status":"declared","source_refs":["source"]},
            {"property":"second_moment","value":q(1000.0,"mm4"),"status":"declared","source_refs":["source"]},
            {"property":"support","value":"cantilever_fixed_free","status":"declared","source_refs":["source"]},
            {"property":"load_position","value":"end","status":"declared","source_refs":["source"]},
            {"property":"regime","value":"linear_elastic_small_deflection","status":"declared","source_refs":["source"]}]);
        basis["assertions"] = json!([{"assertion_id":"beam-boundary","kind":"model_boundary","subject":"probe",
            "requirements":{"method":"beam-cantilever-end-load/1","inputs":inputs},"source_refs":["source"],"affected_decisions":[]}]);
        self.set_basis(basis);
        self.plan["hypotheses"][0]["kind"] = json!("model_prediction");
        let a = &mut self.plan["actions"][0];
        a["kind"] = json!("calculation");
        a["source"] = Value::Null;
        a["calculation_id"] = json!("probe");
        a["pointer"] = json!("/result");
        self.plan["outcomes"][0]["operator"] = json!("lt");
        self.plan["outcomes"][0]["value"] = q(0.03, "mm");
        self.plan["outcomes"][1]["operator"] = json!("ge");
        self.plan["outcomes"][1]["value"] = q(0.03, "mm");
    }
}

#[test]
fn actual_source_task_updates_project_view_and_harness() {
    let f = Fixture::new();
    let original = fs::read(f.root.path().join("var/project.json")).unwrap();
    let run = f.run();
    assert_eq!(
        f.read(&run.project.path)["decisions"][0]["conclusion"],
        "supported"
    );
    assert_eq!(
        f.read(&run.result.path)["candidate_routes"]["explicit-inventory"]["disposition"],
        "requires_review"
    );
    assert_eq!(
        f.read(&run.task_view.path)["input"]["previous"]["sha256"],
        sha256(&original)
    );
    let harness = f.read(&run.harness.path);
    assert_eq!(harness["schema"], "oev2.harness-checkpoint/1");
    assert_eq!(harness["model_execution"], "not_run");
    assert_eq!(
        fs::read(f.root.path().join("var/project.json")).unwrap(),
        original
    );
}
#[test]
fn hypotheses_do_not_change_facts_or_adopt_decisions() {
    let mut f = Fixture::new();
    f.plan["hypotheses"][0]["kind"] = json!("causal_hypothesis");
    let run = f.run();
    let p = f.read(&run.project.path);
    assert_eq!(p["conditions"], json!(f.project.conditions));
    assert_eq!(p["decisions"][0]["conclusion"], "unknown");
    assert_eq!(p["decisions"][0]["adopted"], false);
    assert_eq!(f.read(&run.result.path)["facts_mutated"], false);
    let mut f = Fixture::new();
    f.model();
    f.plan["hypotheses"][0]["kind"] = json!("causal_hypothesis");
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["actions"][0]["disposition"],
        "distinguished"
    );
    assert_eq!(
        f.read(&run.project.path)["decisions"][0]["conclusion"],
        "unknown"
    );
}
#[test]
fn frozen_sources_and_same_revision_changes_are_rejected() {
    let f = Fixture::new();
    let frozen = f.freeze();
    fs::write(f.root.path().join("var/source.json"), b"{}").unwrap();
    assert!(exploration::verify(&f.root, &frozen).is_err());
    let f = Fixture::new();
    let frozen = f.freeze();
    let mut p = f.read("var/project.json");
    p["purpose"] = json!("changed same revision");
    pin(&f.root, "var/project.json", &p);
    assert!(exploration::verify(&f.root, &frozen).is_err());
}
#[test]
fn complete_acceptance_and_parent_load_cannot_be_removed() {
    let mut f = Fixture::new();
    f.work["acceptance"].as_array_mut().unwrap().pop();
    f.sync();
    assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
    let mut f = Fixture::new();
    f.model();
    f.plan["candidates"][0]["remaining_obligations"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
    let mut f = Fixture::new();
    f.model();
    f.work["calculations"][0]["inputs"]["force"] = q(5.0, "N");
    f.sync();
    let run = f.run();
    let result = f.read(&run.result.path);
    assert_eq!(result["actions"][0]["readback"]["decision_support"], false);
    assert_eq!(result["decision_conclusions"]["registration"], "unknown");
    assert_eq!(f.basis()["facts"][0]["value"], q(10.0, "N"));
}
#[test]
fn adopted_or_foreign_scope_hypothesis_is_rejected() {
    let mut f = Fixture::new();
    f.project.decisions[0].adopted = true;
    f.project.decisions[0].conclusion = "supported".into();
    f.sync();
    assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
    let mut f = Fixture::new();
    f.project.decisions[0].condition_scope = "foreign".into();
    f.sync();
    assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
}
#[test]
fn criteria_and_discriminating_branches_are_required_before_execution() {
    let mut f = Fixture::new();
    f.plan["outcomes"].as_array_mut().unwrap().pop();
    assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
    let mut f = Fixture::new();
    f.plan["outcomes"][1]["supported"] = json!(["missing"]);
    f.plan["outcomes"][1]["rejected"] = json!([]);
    assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
}
#[test]
fn conditional_next_action_follows_actual_outcome() {
    let mut f = Fixture::new();
    let mut a = f.plan["actions"][0].clone();
    a["action_id"] = json!("followup");
    a["when_outcomes"] = json!(["present-registration"]);
    f.plan["actions"].as_array_mut().unwrap().push(a);
    for original in f.plan["outcomes"].as_array().unwrap().clone() {
        let mut o = original;
        o["action_id"] = json!("followup");
        o["outcome_id"] = json!(format!("followup-{}", o["outcome_id"].as_str().unwrap()));
        f.plan["outcomes"].as_array_mut().unwrap().push(o);
    }
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["actions"][1]["selection"],
        "not_selected"
    );
    let mut f = Fixture::source(json!({"registered":true}));
    let mut a = f.plan["actions"][0].clone();
    a["action_id"] = json!("followup");
    a["when_outcomes"] = json!(["present-registration"]);
    f.plan["actions"].as_array_mut().unwrap().push(a);
    for mut o in f.plan["outcomes"].as_array().unwrap().clone() {
        o["action_id"] = json!("followup");
        o["outcome_id"] = json!(format!("followup-{}", o["outcome_id"].as_str().unwrap()));
        f.plan["outcomes"].as_array_mut().unwrap().push(o);
    }
    let expected = pin(
        &f.root,
        "var/positive-expected.json",
        &json!({
        "schema":"oev2.exploration-expectations/1","exposure":"development_replay",
        "quality":{"preserve_complete_acceptance":true,"action_order":["inspect","followup"],
            "decision_conclusions":{"registration":"refuted"}},
        "cost":{"maximum_actions":2,"maximum_model_calls":0}}),
    );
    f.plan["independent_expected"] = json!(expected);
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["actions"][1]["selection"],
        "selected"
    );
    assert_eq!(
        f.read("var/run/assessment.json")["behavior_quality"],
        "supported_for_frozen_replay"
    );
}
fn rules(acceptance: &Value) -> Vec<DesignComparisonRule> {
    acceptance.as_array().unwrap().iter().enumerate().map(|(i,a)|serde_json::from_value(if i==0{
        json!({"rule_id":"model","acceptance":a,"owner":"calculated_model","baseline_calculation":"probe","candidate_calculation":"probe",
            "symbol":"deflection","unit":"mm","direction":"lower","minimum_improvement":0.0,"design_variables":[]})
    }else{json!({"rule_id":format!("open-{i}"),"acceptance":a,"owner":"not_implemented","baseline_calculation":null,
        "candidate_calculation":null,"symbol":null,"unit":null,"direction":null,"minimum_improvement":null,"design_variables":[]})}).unwrap()).collect()
}
#[test]
fn parameter_comparison_reuses_original_model_identity_guard() {
    let mut f = Fixture::new();
    f.model();
    let a: EngineeringCalculation =
        serde_json::from_value(f.work["calculations"][0].clone()).unwrap();
    let mut b = a.clone();
    b.method = "beam-simple-center-load/1".into();
    let r = rules(&f.work["acceptance"]);
    let accept: Vec<String> = serde_json::from_value(f.work["acceptance"].clone()).unwrap();
    let baseline = oe_kernel::learning::design_comparison::Side {
        object: &a.object,
        calculations: std::slice::from_ref(&a),
        execution: "not_run",
    };
    let candidate = oe_kernel::learning::design_comparison::Side {
        object: &b.object,
        calculations: std::slice::from_ref(&b),
        execution: "not_run",
    };
    assert!(
        oe_kernel::learning::design_comparison::validate_rules(
            &f.root, &f.project, &a.object, &accept, &r, &baseline, &candidate
        )
        .unwrap_err()
        .to_string()
        .contains("model_or_premises_changed")
    );
}
#[test]
fn mechanism_change_uses_new_contract_and_preserves_old_comparison() {
    let mut f = Fixture::new();
    f.model();
    let mut other = f.work.clone();
    other["calculations"][0]["method"] = json!("beam-simple-center-load/1");
    let work = pin(&f.root, "var/alternative-work.json", &other);
    let mut candidate = f.plan["candidates"][0].clone();
    candidate["candidate_id"] = json!("alternative");
    candidate["work"] = json!(work);
    candidate["principle"] = json!("different restrained support construction");
    candidate["representation"] = json!("simple-center beam");
    f.plan["candidates"].as_array_mut().unwrap().push(candidate);
    let frozen = f.freeze();
    assert_eq!(
        exploration::verify(&f.root, &frozen).unwrap()["status"],
        "current_frozen_exploration_verified"
    );
    let mut f = Fixture::new();
    f.plan["comparison_extension_reason"] = json!("");
    assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
}
#[test]
fn real_calculation_readback_changes_only_bounded_prediction() {
    let mut f = Fixture::new();
    f.model();
    let run = f.run();
    let r = f.read(&run.result.path);
    let actual = r["actions"][0]["readback"]["result"]["value"]
        .as_f64()
        .unwrap();
    assert!((actual - 1.0 / 60.0).abs() < 1e-14);
    assert_eq!(r["decision_conclusions"]["registration"], "supported");
    assert_eq!(r["adopted"], false);
    assert_eq!(r["acceptance"], f.work["acceptance"]);
}
#[test]
fn missing_partial_ambiguous_and_failure_remain_unknown() {
    let mut f = Fixture::new();
    f.plan["actions"][0]["pointer"] = json!("/observation/missing");
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["actions"][0]["disposition"],
        "inconclusive"
    );
    assert_eq!(
        f.read(&run.project.path)["decisions"][0]["conclusion"],
        "unknown"
    );
    let mut f = Fixture::new();
    f.plan["outcomes"][1]["value"] = json!(false);
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["actions"][0]["disposition"],
        "ambiguous"
    );
    assert_eq!(
        f.read(&run.project.path)["decisions"][0]["conclusion"],
        "unknown"
    );
    let mut f = Fixture::new();
    f.model();
    f.work["calculations"][0]["inputs"]["force"] = Value::Null;
    f.sync();
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["actions"][0]["readback"]["execution"],
        "not_run"
    );
    let mut f = Fixture::new();
    f.model();
    f.work["calculations"][0]["method"] = json!("unimplemented-model");
    f.sync();
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["actions"][0]["readback"]["execution"],
        "failed"
    );
    assert_eq!(
        f.read(&run.project.path)["decisions"][0]["conclusion"],
        "unknown"
    );
}
#[test]
fn units_and_strict_thresholds_remain_exact() {
    let mut f = Fixture::new();
    f.plan["actions"][0]["pointer"] = json!("/observation/metric");
    f.plan["outcomes"][0]["operator"] = json!("lt");
    f.plan["outcomes"][0]["value"] = q(0.03, "mm");
    f.plan["outcomes"][1]["operator"] = json!("ge");
    f.plan["outcomes"][1]["value"] = q(0.03, "mm");
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["decision_conclusions"]["registration"],
        "refuted"
    );
    let mut f = Fixture::new();
    f.model();
    f.plan["outcomes"][0]["value"] = q(0.03, "m");
    let run = f.run();
    assert_eq!(
        f.read(&run.result.path)["actions"][0]["disposition"],
        "comparison_failed"
    );
    assert_eq!(
        f.read(&run.project.path)["decisions"][0]["conclusion"],
        "unknown"
    );
}
#[test]
fn frozen_copy_drift_and_original_history_are_preserved() {
    let f = Fixture::new();
    let frozen = f.freeze();
    let doc = f.read("var/frozen/completed.json");
    let file = doc["sources"][0]["copy"].as_str().unwrap();
    fs::write(f.root.path().join("var/frozen").join(file), b"changed").unwrap();
    assert!(exploration::verify(&f.root, &frozen).is_err());
    assert!(f.root.path().join("var/frozen/completed.json").is_file());
    let f = Fixture::new();
    let original = fs::read(f.root.path().join("var/project.json")).unwrap();
    let frozen = f.freeze();
    let req = EngineeringExplorationRunRequest {
        frozen,
        output: "var/run".into(),
    };
    exploration::execute(&f.root, &req).unwrap();
    assert!(exploration::execute(&f.root, &req).is_err());
    assert_eq!(
        fs::read(f.root.path().join("var/project.json")).unwrap(),
        original
    );
}
#[test]
fn method_assessment_separates_replay_behavior_from_benefit() {
    let f = Fixture::new();
    let run = f.run();
    let a = f.read("var/run/assessment.json");
    assert_eq!(a["behavior_quality"], "supported_for_frozen_replay");
    assert_eq!(a["method_benefit"], "unknown");
    assert_eq!(a["whole_engineering_achievement"], "unknown");
    assert_eq!(a["promotion"], "not_admissible_from_this_assessment");
    let mut expected = f.read("var/expected.json");
    expected["quality"]["decision_conclusions"]["registration"] = json!("refuted");
    assert_eq!(
        exploration::assessment(&expected, &f.read(&run.result.path)).unwrap()["behavior_quality"],
        "violated"
    );
    let mut result = f.read(&run.result.path);
    result["cost"]["model_calls"] = json!(1);
    assert_eq!(
        exploration::assessment(&expected, &result).unwrap()["local_cost_budget"],
        "violated"
    );
}

#[test]
fn owner_fields_and_wire_types_match_pinned_source() {
    let f = Fixture::new();
    f.freeze();
    let schema: Value = serde_json::from_slice(include_bytes!(
        "../../../specs/owner-schemas/engineering-exploration.json"
    ))
    .unwrap();
    let plan = f.read("var/frozen/plan.json");
    for (name, actual) in [
        ("EngineeringExplorationPlan", &plan),
        ("EngineeringExplorationHypothesis", &plan["hypotheses"][0]),
        ("EngineeringExplorationCandidate", &plan["candidates"][0]),
        ("EngineeringExplorationAction", &plan["actions"][0]),
        ("EngineeringExplorationOutcome", &plan["outcomes"][0]),
    ] {
        let definition = schema["types"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name)
            .unwrap();
        let declared: std::collections::BTreeSet<_> = definition["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect();
        let encoded: std::collections::BTreeSet<_> = actual
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(encoded, declared, "{name}");
    }
    for path in [
        "",
        "/hypotheses/0",
        "/candidates/0",
        "/actions/0",
        "/outcomes/0",
    ] {
        let mut f = Fixture::new();
        f.plan.pointer_mut(path).unwrap()["unknown_field"] = json!("must reject");
        assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
    }
    let mut f = Fixture::new();
    f.plan["hypotheses"][0]["grounding"] = json!("already_known");
    assert!(exploration::freeze(&f.root, &f.input(), Path::new("var/rejected")).is_err());
    let f = Fixture::new();
    let frozen = f.freeze();
    fs::write(
        f.root
            .path()
            .join("specs/owner-schemas/engineering-exploration.json"),
        b"drift",
    )
    .unwrap();
    assert!(exploration::verify(&f.root, &frozen).is_err());
}
