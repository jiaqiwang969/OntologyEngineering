//! Independent model-level comparisons. The expected numbers were frozen in
//! INDEPENDENT-TEST-INTENT-01.json before the comparison implementation existed.
//! Neither synthetic source declarations nor a computed deflection establish
//! native CAD, manufactured geometry, physical acceptance or method promotion.
#[path = "support/design_attempt_fixture.rs"]
mod fixture;
#[path = "../../oe-knowledge/tests/support/incad_fixture.rs"]
mod incad_fixture;

use fixture::{Fixture, quantity, save};
use oe_contracts::{
    ArtifactReference, EngineeringCalculation, IncadWorkPreparationRequest, ObjectIdentity,
};
use oe_engineering::design_comparison::{evaluate, freeze, verify};
use oe_kernel::project::context;
use oe_knowledge::poppler::PopplerTools;
use serde_json::{Value, json};
use std::{fs, path::Path};

struct Comparison {
    fixture: Fixture,
    request: Value,
    works: [Value; 2],
    acceptance: Vec<Value>,
}

fn pin(root: &Path, relative: &str, value: &Value) -> ArtifactReference {
    ArtifactReference {
        path: relative.into(),
        sha256: save(&root.join(relative), value),
    }
}

fn side_record(f: &Fixture, name: &str, work: &Value) -> ArtifactReference {
    let root = f.root.path();
    let base = format!("var/{name}");
    let mut intake_request: IncadWorkPreparationRequest = f.request.clone();
    intake_request.work = pin(root, &format!("{base}/work.json"), work);
    let intake_ref = pin(
        root,
        &format!("{base}/incad-request.json"),
        &json!(intake_request),
    );
    let intake = oe_engineering::incad::prepare(&f.root, &intake_request).unwrap();
    let intake_dir = format!("{base}/incad-prepared");
    fs::create_dir_all(root.join(&intake_dir)).unwrap();
    for (file, bytes) in [
        (
            "request.json",
            fs::read(root.join(&intake_ref.path)).unwrap(),
        ),
        ("work.json", intake.work_bytes.clone()),
        (
            "specifications.json",
            intake.intake.specifications_bytes().to_vec(),
        ),
        ("zero-design-task.json", intake.intake.task_bytes().to_vec()),
    ] {
        fs::write(root.join(&intake_dir).join(file), bytes).unwrap();
    }
    save(
        &root.join(&intake_dir).join("preparation.json"),
        &intake.result,
    );
    save(
        &root.join(&intake_dir).join("completed.json"),
        &json!({
            "status":"prepared", "work_sha256":oe_core::sha256(&intake.work_bytes),
            "native_execution":"not_run", "engineering_applicability":"not_reviewed",
            "authority":"synthetic input preparation, not a tool execution receipt"
        }),
    );
    intake.ensure_current(&f.root).unwrap();
    let intent = pin(
        root,
        &format!("{base}/independent-intent.json"),
        &json!({
            "schema":"oev2.design-intent/1", "object":work["object"],
            "statement":"Compare the declared section stiffness under the unchanged parent load; compute only the declared beam model.",
            "source_refs":["task"], "alternatives":["section A", "section B"],
            "unknowns":["actual geometry and material are unmeasured", "native CAD and physical acceptance are not established"]
        }),
    );
    let steps: Vec<_> = ["work", "harness", "cad_evidence", "handoff", "kernel_review", "feedback"]
        .into_iter().map(|activity| json!({
            "step_id":format!("step-{activity}"), "activity":activity,
            "acceptance":work["acceptance"], "required_evidence":[format!("actual {activity} evidence bound to this object")]
        })).collect();
    let attempt = pin(
        root,
        &format!("{base}/attempt.json"),
        &json!({
            "schema":"oev2.design-attempt/1", "attempt_id":format!("synthetic-{name}"),
            "incad_request":intake_ref, "incad_prepared":intake_dir, "independent_intent":intent,
            "tool_plan":steps, "reference_exposure":[{
                "stage":"before_freeze", "basis":"The fixture and complete source family have already been inspected; this is development replay.",
                "source_refs":["supplier_html", "task"]
            }]
        }),
    );
    let prepared = format!("{base}/attempt-prepared");
    oe_engineering::design_attempt::prepare_to(&f.root, &attempt, &root.join(&prepared)).unwrap();
    pin(
        root,
        &format!("{base}/record-request.json"),
        &json!({
            "schema":"oev2.design-attempt-record/1", "attempt":attempt, "prepared":prepared,
            "artifacts":[], "review":null, "reference_exposure":[],
            "notes":["Only actual owner results count; all physical and native obligations remain open."]
        }),
    )
}

impl Comparison {
    fn new(full_obligations: bool, candidate_inertia: f64) -> Self {
        Self::edited(full_obligations, candidate_inertia, |_, _, _| {})
    }

    // Edits precede all goal-support, Project, work, preparation and input hashes.
    // Rejected cases therefore exercise engineering identity, not a stale SHA.
    fn edited(
        full_obligations: bool,
        candidate_inertia: f64,
        edit: impl FnOnce(&mut [Value; 2], &mut [Value; 2], &mut Value),
    ) -> Self {
        let mut f = Fixture::new();
        if !full_obligations {
            f.project.purpose = "Compare one declared elastic-deflection metric at 10 N; this task does not establish a physical machine".into();
        }
        let acceptance = if full_obligations {
            f.work["acceptance"].as_array().unwrap().clone()
        } else {
            vec![f.work["acceptance"][0].clone()]
        };
        let parent = ObjectIdentity {
            object_id: "machine".into(),
            revision: "r1".into(),
            condition_scope: "machine-goal".into(),
            source_sha256: f.object.source_sha256.clone(),
        };
        let candidate = ObjectIdentity {
            object_id: "guide-candidate".into(),
            revision: "r1".into(),
            condition_scope: "candidate-study".into(),
            source_sha256: f.object.source_sha256.clone(),
        };
        let objects = [f.object.clone(), candidate];
        f.project
            .objects
            .insert(parent.object_id.clone(), parent.revision.clone());
        f.project
            .objects
            .insert(objects[1].object_id.clone(), objects[1].revision.clone());
        let shared_facts: Vec<_> = f.basis["facts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["property"] != "second_moment")
            .cloned()
            .collect();
        let mut parent_basis = json!({
            "schema":"oev2.engineering-basis/1", "object_id":parent.object_id, "object_revision":"r1",
            "purpose":f.project.purpose, "acceptance":acceptance, "facts":shared_facts, "assertions":[]
        });
        let mut bases = [f.basis.clone(), f.basis.clone()];
        let mut works = [f.work.clone(), f.work.clone()];
        for (i, inertia) in [1000.0, candidate_inertia].into_iter().enumerate() {
            bases[i]["schema"] = json!("oev2.engineering-basis/2");
            bases[i]["object_id"] = json!(objects[i].object_id);
            bases[i]["purpose"] = json!(
                "Assess the same parent obligations using this explicitly declared candidate section"
            );
            bases[i]["acceptance"] = json!(acceptance);
            bases[i]["facts"][3]["value"] = quantity(inertia, "mm4");
            bases[i]["assertions"][0]["requirements"]["inputs"]["second_moment"] =
                quantity(inertia, "mm4");
            works[i]["object"] = json!(objects[i]);
            works[i]["acceptance"] = json!(acceptance);
            works[i]["calculations"][0]["object"] = json!(objects[i]);
            works[i]["calculations"][0]["inputs"]["second_moment"] = quantity(inertia, "mm4");
        }
        edit(&mut bases, &mut works, &mut parent_basis);
        let parent_text = parent_basis.to_string();
        f.project
            .conditions
            .insert(parent.condition_scope.clone(), parent_text.clone());
        for i in 0..2 {
            let body_sha = oe_core::sha256(oe_core::json::canonical_json(&bases[i]).unwrap());
            let obligations: Vec<_> = bases[i]["acceptance"].as_array().unwrap().iter().enumerate()
                .map(|(n, local)| json!({
                    "obligation_id":format!("obligation-{n}"), "local_acceptance":local,
                    "parent_acceptance":local, "contribution":"Local evidence contributes; parent acceptance remains unestablished.",
                    "assertion_ids":["guide-function"], "fact_properties":[]
                })).collect();
            bases[i]["goal_support"] = json!({
                "schema":"oev2.engineering-goal-support/1", "project_id":f.project.project_id,
                "project_revision":f.project.revision, "project_purpose":f.project.purpose,
                "local_object":objects[i], "parent_object":parent,
                "parent_condition_sha256":oe_core::sha256(parent_text.as_bytes()), "local_basis_sha256":body_sha,
                "sources":[{"source_ref":"supplier_html","source_sha256":f.project.evidence["supplier_html"]},
                    {"source_ref":"task","source_sha256":f.project.evidence["task"]}],
                "obligations":obligations, "affected_decisions":[], "claim_scope":"local_obligation_only"
            });
            f.project
                .conditions
                .insert(objects[i].condition_scope.clone(), bases[i].to_string());
        }
        let project_ref = pin(f.root.path(), "var/project.json", &json!(f.project));
        for i in 0..2 {
            works[i]["project"] = json!(project_ref);
            works[i]["project_context"] = json!(context(&f.project, &objects[i]).unwrap());
        }
        let mut parent_work = f.work.clone();
        parent_work["project"] = json!(project_ref);
        parent_work["project_context"] = json!(context(&f.project, &parent).unwrap());
        parent_work["object"] = json!(parent);
        parent_work["acceptance"] = parent_basis["acceptance"].clone();
        parent_work["calculations"] = json!([]);
        let parent_work = pin(f.root.path(), "var/parent-work.json", &parent_work);
        let baseline = side_record(&f, "baseline", &works[0]);
        let candidate = side_record(&f, "candidate", &works[1]);
        let rules: Vec<_> = acceptance.iter().enumerate().map(|(i, acceptance)| {
            if i == 0 { json!({
                "rule_id":"elastic-deflection", "acceptance":acceptance, "owner":"calculated_model",
                "baseline_calculation":"stiffness", "candidate_calculation":"stiffness",
                "symbol":"deflection", "unit":"mm", "direction":"lower", "minimum_improvement":0.0001,
                "design_variables":[{"property":"second_moment", "lower":quantity(500.0,"mm4"), "upper":quantity(3000.0,"mm4")}]
            }) } else { json!({
                "rule_id":format!("remaining-{i}"), "acceptance":acceptance, "owner":"not_implemented",
                "baseline_calculation":null, "candidate_calculation":null, "symbol":null,
                "unit":null, "direction":null, "minimum_improvement":null, "design_variables":[]
            }) }
        }).collect();
        let request = json!({
            "schema":"oev2.design-comparison/1", "comparison_id":"independent-beam-comparison",
            "parent_work":parent_work, "baseline":baseline, "candidate":candidate, "rules":rules,
            "source_family":"incad/cn/000019/already-exposed-synthetic-family", "exposure":"development_replay"
        });
        Self {
            fixture: f,
            request,
            works,
            acceptance,
        }
    }

    fn freeze(&self) -> ArtifactReference {
        let reference = pin(
            self.fixture.root.path(),
            "var/comparison-input.json",
            &self.request,
        );
        freeze(
            &self.fixture.root,
            &reference,
            &self.fixture.root.path().join("var/comparison-frozen"),
        )
        .unwrap();
        let path = "var/comparison-frozen/completed.json";
        ArtifactReference {
            path: path.into(),
            sha256: oe_core::sha256(fs::read(self.fixture.root.path().join(path)).unwrap()),
        }
    }

    fn evaluate(&self, frozen: &ArtifactReference) -> Value {
        let forbidden = self
            .fixture
            .root
            .path()
            .join("var/no-external-tool-may-run");
        let tools = PopplerTools {
            pdfinfo: forbidden.clone(),
            pdftotext: forbidden.clone(),
            pdftoppm: forbidden,
        };
        evaluate(
            &self.fixture.root,
            frozen,
            &self.fixture.root.path().join("var/comparison-evaluated"),
            &tools,
        )
        .unwrap()
    }

    fn rejects(&self) {
        let root = &self.fixture.root;
        let reference = pin(root.path(), "var/comparison-invalid.json", &self.request);
        let output = root.path().join("var/comparison-rejected");
        assert!(
            freeze(root, &reference, &output).is_err(),
            "invalid case unexpectedly frozen: {}",
            self.request
        );
        assert!(
            !output.join("completed.json").exists(),
            "rejection cannot publish success terminal"
        );
    }

    fn unchanged_project(&self, before: &[u8]) {
        assert_eq!(
            fs::read(self.fixture.root.path().join("var/project.json")).unwrap(),
            before
        );
    }
}

fn approximate(value: &Value, expected: f64) {
    let actual = value
        .as_f64()
        .unwrap_or_else(|| panic!("expected finite numeric evidence, got {value}"));
    assert!(
        (actual - expected).abs() < 1e-12,
        "actual {actual}, independent expectation {expected}"
    );
}

fn bounded_result(result: &Value, acceptance: &[Value]) {
    assert_eq!(result["schema"], "oev2.design-comparison-result/1");
    let a = &result["assessment"];
    assert_eq!(a["cost"]["status"], "unknown");
    assert_eq!(a["cost"]["candidate_is_cheaper"], false);
    assert_eq!(a["engineering_conclusion"], "unknown");
    assert_eq!(a["design_complete"], false);
    assert_eq!(a["adopted"], false);
    assert_eq!(a["method_promotion"], "not_performed");
    let remaining = result["remaining_acceptance"].as_array().unwrap();
    assert_eq!(remaining.len(), acceptance.len());
    for (row, expected) in remaining.iter().zip(acceptance) {
        assert_eq!(&row["acceptance"], expected);
        assert_eq!(row["engineering_acceptance"], "not_established");
    }
    for row in a["rows"].as_array().unwrap() {
        assert_eq!(row["engineering_acceptance"], "not_established");
    }
}

#[test]
fn independent_beam_oracle_improvement_is_computed_under_frozen_parent_task() {
    let c = Comparison::new(false, 2000.0);
    c.fixture.install_method_sources();
    // Independent mechanics, not a value copied from a comparison receipt.
    // 10*100^3/(3*200000*1000)=1/60 and doubling I halves displacement.
    for (work, expected) in c.works.iter().zip([1.0 / 60.0, 1.0 / 120.0]) {
        let calculation: EngineeringCalculation =
            serde_json::from_value(work["calculations"][0].clone()).unwrap();
        let check = oe_kernel::engineering::calculation_check(&c.fixture.project, &calculation);
        assert_eq!(check.conclusion, "pass", "{check:?}");
        approximate(
            &oe_kernel::calculation::calculate(&calculation).unwrap()["result"]["value"],
            expected,
        );
    }
    let before = fs::read(c.fixture.root.path().join("var/project.json")).unwrap();
    let frozen = c.freeze();
    verify(&c.fixture.root, &frozen).unwrap();
    let result = c.evaluate(&frozen);
    assert_eq!(result["assessment"]["verdict"], "improved");
    assert_eq!(result["assessment"]["counts"]["denominator"], 1);
    assert_eq!(result["assessment"]["counts"]["improved"], 1);
    assert_eq!(result["assessment"]["counts"]["unknown"], 0);
    let row = &result["assessment"]["rows"][0];
    approximate(&row["baseline"]["value"], 1.0 / 60.0);
    approximate(&row["candidate"]["value"], 1.0 / 120.0);
    approximate(&row["difference"], -1.0 / 120.0);
    approximate(&row["gain"], 1.0 / 120.0);
    for side in ["baseline", "candidate"] {
        assert_eq!(row[side]["model_applicability"]["conclusion"], "pass");
        assert_eq!(row[side]["arithmetic"]["execution"], "completed");
        assert_eq!(
            result[side]["record"]["work"]["calculations"][0]["execution"],
            "completed"
        );
    }
    bounded_result(&result, &c.acceptance);
    assert_eq!(result["reference_exposure"]["scope"], "development_replay");
    let continuation_path = c
        .fixture
        .root
        .path()
        .join("var/comparison-evaluated/candidate-feedback-record-request.json");
    let continuation: oe_contracts::EngineeringDesignAttemptRecordRequest =
        serde_json::from_slice(&fs::read(continuation_path).unwrap()).unwrap();
    let forbidden = c.fixture.root.path().join("var/no-external-tool-may-run");
    let tools = PopplerTools {
        pdfinfo: forbidden.clone(),
        pdftotext: forbidden.clone(),
        pdftoppm: forbidden,
    };
    let next = oe_engineering::design_attempt::record(
        &c.fixture.root,
        &continuation,
        &c.fixture.root.path().join("var/candidate-feedback-record"),
        &tools,
    )
    .unwrap();
    let feedback = next["artifacts"].as_array().unwrap().last().unwrap();
    assert_eq!(feedback["role"], "feedback");
    assert_eq!(feedback["authority"], "author_report_only");
    assert_eq!(feedback["original"]["verdict"], "improved");
    assert_eq!(feedback["original"]["method_promotion"], "not_performed");
    assert_eq!(next["design_complete"], false);
    assert_eq!(next["adopted"], false);
    c.unchanged_project(&before);
}

#[test]
fn identical_design_is_no_delta_and_weaker_section_is_regression() {
    for (inertia, verdict, row_status, expected) in [
        (1000.0, "no_delta", "unchanged", 1.0 / 60.0),
        (500.0, "regressed", "regressed", 1.0 / 30.0),
    ] {
        let c = Comparison::new(false, inertia);
        c.fixture.install_method_sources();
        let result = c.evaluate(&c.freeze());
        assert_eq!(result["assessment"]["verdict"], verdict);
        assert_eq!(result["assessment"]["rows"][0]["comparison"], row_status);
        approximate(
            &result["assessment"]["rows"][0]["candidate"]["value"],
            expected,
        );
        bounded_result(&result, &c.acceptance);
    }
}

#[test]
fn native_manufacturing_and_physical_obligations_remain_in_denominator() {
    let c = Comparison::new(true, 2000.0);
    c.fixture.install_method_sources();
    let result = c.evaluate(&c.freeze());
    assert_eq!(result["assessment"]["verdict"], "inconclusive");
    let count = &result["assessment"]["counts"];
    assert_eq!(count["denominator"], 4);
    assert_eq!(count["comparable"], 1);
    assert_eq!(count["improved"], 1);
    assert_eq!(count["unknown"], 3);
    assert_eq!(result["assessment"]["rows"].as_array().unwrap().len(), 4);
    for row in result["assessment"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
    {
        assert_eq!(row["comparison"], "unknown");
        assert_eq!(row["candidate"]["execution"], "not_run");
        assert!(row["candidate"]["value"].is_null());
        assert!(row["difference"].is_null());
    }
    bounded_result(&result, &c.acceptance);
}

#[test]
fn missing_required_calculation_is_unknown_not_zero_or_no_delta() {
    let c = Comparison::edited(false, 2000.0, |_, works, _| {
        works[1]["calculations"] = json!([])
    });
    c.fixture.install_method_sources();
    let result = c.evaluate(&c.freeze());
    assert_eq!(result["assessment"]["verdict"], "inconclusive");
    assert_eq!(result["assessment"]["counts"]["denominator"], 1);
    assert_eq!(result["assessment"]["counts"]["unknown"], 1);
    let row = &result["assessment"]["rows"][0];
    assert_eq!(row["candidate"]["execution"], "not_run");
    assert_eq!(row["comparison"], "unknown");
    assert!(row["candidate"]["value"].is_null());
    assert!(row["gain"].is_null());
    bounded_result(&result, &c.acceptance);
}

#[test]
fn valid_arithmetic_with_the_wrong_declared_model_cannot_score() {
    let c = Comparison::edited(false, 2000.0, |bases, _, _| {
        bases[1]["assertions"][0]["requirements"]["inputs"]["second_moment"] =
            quantity(1000.0, "mm4");
    });
    c.fixture.install_method_sources();
    let calculation: EngineeringCalculation =
        serde_json::from_value(c.works[1]["calculations"][0].clone()).unwrap();
    approximate(
        &oe_kernel::calculation::calculate(&calculation).unwrap()["result"]["value"],
        1.0 / 120.0,
    );
    assert_eq!(
        oe_kernel::engineering::calculation_check(&c.fixture.project, &calculation).conclusion,
        "violated"
    );
    let result = c.evaluate(&c.freeze());
    assert_eq!(result["assessment"]["verdict"], "inconclusive");
    let row = &result["assessment"]["rows"][0];
    assert_eq!(
        row["candidate"]["model_applicability"]["conclusion"],
        "violated"
    );
    assert_eq!(row["comparison"], "unknown");
    assert!(row["candidate"]["value"].is_null());
    assert_eq!(result["assessment"]["counts"]["denominator"], 1);
    assert_eq!(result["assessment"]["counts"]["unknown"], 1);
    bounded_result(&result, &c.acceptance);
}

#[test]
fn real_work_owner_failure_cannot_be_erased_by_successful_arithmetic() {
    let c = Comparison::new(false, 2000.0);
    // Deliberately no installed policy: the actual work owner fails to load it.
    // No fake subprocess, authored failed result or changed production guard.
    let result = c.evaluate(&c.freeze());
    assert_eq!(result["assessment"]["verdict"], "inconclusive");
    let count = &result["assessment"]["counts"];
    assert_eq!(count["denominator"], 1);
    assert_eq!(count["unknown"], 1);
    assert_eq!(count["baseline_failed"], 1);
    assert_eq!(count["candidate_failed"], 1);
    for side in ["baseline", "candidate"] {
        assert_eq!(result[side]["record"]["work"]["execution"], "failed");
        assert_eq!(result["assessment"]["rows"][0][side]["execution"], "failed");
        assert!(result["assessment"]["rows"][0][side]["value"].is_null());
    }
    bounded_result(&result, &c.acceptance);
}

#[test]
fn rebinding_every_file_cannot_change_load_source_acceptance_or_units() {
    for case in [
        "candidate_load",
        "both_loads",
        "source",
        "acceptance",
        "unit",
        "outside_range",
    ] {
        let c = Comparison::edited(
            false,
            if case == "outside_range" {
                4000.0
            } else {
                2000.0
            },
            |bases, works, _| match case {
                "candidate_load" | "both_loads" => {
                    let start = if case == "both_loads" { 0 } else { 1 };
                    for i in start..2 {
                        bases[i]["facts"][0]["value"] = quantity(20.0, "N");
                        bases[i]["assertions"][0]["requirements"]["inputs"]["force"] =
                            quantity(20.0, "N");
                        works[i]["calculations"][0]["inputs"]["force"] = quantity(20.0, "N");
                    }
                }
                "source" => works[1]["calculations"][0]["source_refs"] = json!(["supplier_html"]),
                "acceptance" => {
                    bases[1]["acceptance"][0] = json!("Accept 0.2 mm instead of 0.02 mm");
                    works[1]["acceptance"] = bases[1]["acceptance"].clone();
                }
                "unit" => {
                    bases[1]["facts"][3]["value"] = quantity(2000.0, "m4");
                    bases[1]["assertions"][0]["requirements"]["inputs"]["second_moment"] =
                        quantity(2000.0, "m4");
                    works[1]["calculations"][0]["inputs"]["second_moment"] = quantity(2000.0, "m4");
                }
                "outside_range" => (),
                _ => unreachable!(),
            },
        );
        c.rejects();
    }
}

#[test]
fn incomplete_rule_coverage_and_relabeling_exposure_are_rejected() {
    for case in [
        "omitted",
        "duplicate",
        "unbound",
        "hidden",
        "load_as_variable",
    ] {
        let mut c = Comparison::new(true, 2000.0);
        match case {
            "omitted" => {
                c.request["rules"].as_array_mut().unwrap().pop();
            }
            "duplicate" => c.request["rules"][1]["rule_id"] = json!("elastic-deflection"),
            "unbound" => c.request["rules"][0]["acceptance"] = json!("A new easier objective"),
            "hidden" => c.request["exposure"] = json!("unseen_holdout"),
            "load_as_variable" => c.request["rules"][0]["design_variables"]
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "property":"force", "lower":quantity(1.0,"N"), "upper":quantity(20.0,"N")
                })),
            _ => unreachable!(),
        }
        c.rejects();
    }
}

#[test]
fn frozen_current_project_source_and_rule_bytes_cannot_drift() {
    let c = Comparison::new(false, 2000.0);
    let frozen = c.freeze();
    for relative in [
        "var/project.json",
        "var/candidate/independent-intent.json",
        "var/parent-work.json",
        "var/comparison-input.json",
        "var/baseline/incad-prepared/specifications.json",
    ] {
        let path = c.fixture.root.path().join(relative);
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        if relative == "var/project.json" {
            let mut project: Value = serde_json::from_slice(&original).unwrap();
            project["decisions"] = json!([{
                "id":"new-author-decision", "object_id":"guide", "object_revision":"r1", "condition_scope":"study",
                "source_sha256":c.fixture.object.source_sha256, "conclusion":"unknown", "adopted":false, "basis":["beam-boundary"]
            }]);
            assert_eq!(project["revision"], "r1");
            changed = serde_json::to_vec_pretty(&project).unwrap();
        } else {
            changed.push(b'\n');
        }
        fs::write(&path, changed).unwrap();
        assert!(
            verify(&c.fixture.root, &frozen).is_err(),
            "drift accepted: {relative}"
        );
        fs::write(&path, original).unwrap();
        verify(&c.fixture.root, &frozen).unwrap();
    }
}

#[test]
fn authored_pass_and_cost_claims_cannot_improve_an_identical_candidate() {
    let mut c = Comparison::new(false, 1000.0);
    c.fixture.install_method_sources();
    let reference = pin(
        c.fixture.root.path(),
        "var/candidate/author-feedback.json",
        &json!({
            "status":"passed", "design_complete":true, "adopted":true, "native_execution":"completed",
            "measured_deflection_mm":0.0, "candidate_is_cheaper":true, "cost":0,
            "scope":"Unverified author assertions deliberately contradict the actual owner evidence"
        }),
    );
    let record_path = c.request["candidate"]["path"].as_str().unwrap().to_owned();
    let mut record: Value =
        serde_json::from_slice(&fs::read(c.fixture.root.path().join(&record_path)).unwrap())
            .unwrap();
    record["artifacts"] = json!([{"step_id":"step-feedback", "role":"feedback", "artifact":reference, "source_root":null}]);
    c.request["candidate"] = json!(pin(c.fixture.root.path(), &record_path, &record));
    let result = c.evaluate(&c.freeze());
    assert_eq!(result["assessment"]["verdict"], "no_delta");
    assert_eq!(result["assessment"]["counts"]["improved"], 0);
    approximate(
        &result["assessment"]["rows"][0]["candidate"]["value"],
        1.0 / 60.0,
    );
    let artifact = &result["candidate"]["record"]["artifacts"][0];
    assert_eq!(artifact["authority"], "author_report_only");
    assert_eq!(
        artifact["original"]["design_complete"], true,
        "retain contradictory original, never turn it into validated execution"
    );
    bounded_result(&result, &c.acceptance);
}

#[test]
fn feedback_capacity_preserves_all_originals_and_reserves_only_candidate_slot() {
    fn original_feedback(c: &mut Comparison, side: &str, count: usize) -> Vec<Value> {
        let mut artifacts = Vec::new();
        let mut originals = Vec::new();
        for index in 0..count {
            let original = json!({
                "schema":"private.synthetic-feedback/1", "source_index":index,
                "statement":"Distinct original author observation; not validated engineering success"
            });
            let reference = pin(
                c.fixture.root.path(),
                &format!("var/{side}/capacity-feedback-{index:02}.json"),
                &original,
            );
            artifacts.push(json!({"step_id":"step-feedback", "role":"feedback",
                "artifact":reference, "source_root":null}));
            originals.push(original);
        }
        let record_path = c.request[side]["path"].as_str().unwrap().to_owned();
        let mut record: Value =
            serde_json::from_slice(&fs::read(c.fixture.root.path().join(&record_path)).unwrap())
                .unwrap();
        record["artifacts"] = json!(artifacts);
        c.request[side] = json!(pin(c.fixture.root.path(), &record_path, &record));
        originals
    }
    fn tools(c: &Comparison) -> PopplerTools {
        let absent = c.fixture.root.path().join("var/no-external-tool-may-run");
        PopplerTools {
            pdfinfo: absent.clone(),
            pdftotext: absent.clone(),
            pdftoppm: absent,
        }
    }
    let mut c = Comparison::new(false, 2000.0);
    c.fixture.install_method_sources();
    let baseline_originals = original_feedback(&mut c, "baseline", 32);
    let candidate_originals = original_feedback(&mut c, "candidate", 31);
    let project_before = fs::read(c.fixture.root.path().join("var/project.json")).unwrap();
    let result = c.evaluate(&c.freeze());
    assert_eq!(result["assessment"]["verdict"], "improved");
    for (side, originals) in [
        ("baseline", &baseline_originals),
        ("candidate", &candidate_originals),
    ] {
        assert_eq!(result[side]["execution"], "completed");
        let recorded = result[side]["record"]["artifacts"].as_array().unwrap();
        assert_eq!(recorded.len(), originals.len());
        for (row, original) in recorded.iter().zip(originals) {
            assert_eq!(&row["original"], original);
            assert_eq!(row["authority"], "author_report_only");
        }
    }
    let continuation: oe_contracts::EngineeringDesignAttemptRecordRequest = serde_json::from_slice(
        &fs::read(
            c.fixture
                .root
                .path()
                .join("var/comparison-evaluated/candidate-feedback-record-request.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        continuation.artifacts.len(),
        32,
        "31 originals plus this comparison; no truncation or owner-limit expansion"
    );
    let next = oe_engineering::design_attempt::record(
        &c.fixture.root,
        &continuation,
        &c.fixture
            .root
            .path()
            .join("var/capacity-feedback-continued"),
        &tools(&c),
    )
    .unwrap();
    let recorded = next["artifacts"].as_array().unwrap();
    assert_eq!(recorded.len(), 32);
    for (row, original) in recorded.iter().zip(&candidate_originals) {
        assert_eq!(&row["original"], original);
        let reference: ArtifactReference =
            serde_json::from_value(row["reference"].clone()).unwrap();
        let bytes = fs::read(c.fixture.root.path().join(&reference.path)).unwrap();
        assert_eq!(oe_core::sha256(&bytes), reference.sha256);
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), *original);
    }
    let actual_feedback: Value = serde_json::from_slice(
        &fs::read(
            c.fixture
                .root
                .path()
                .join("var/comparison-evaluated/feedback.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(recorded[31]["original"], actual_feedback);
    assert_eq!(recorded[31]["authority"], "author_report_only");
    assert_eq!(next["design_complete"], false);
    assert_eq!(next["adopted"], false);
    c.unchanged_project(&project_before);

    let mut full = Comparison::new(false, 2000.0);
    full.fixture.install_method_sources();
    let originals = original_feedback(&mut full, "candidate", 32);
    let record_path = full.request["candidate"]["path"].as_str().unwrap();
    let original_request: oe_contracts::EngineeringDesignAttemptRecordRequest =
        serde_json::from_slice(&fs::read(full.fixture.root.path().join(record_path)).unwrap())
            .unwrap();
    let control = oe_engineering::design_attempt::record(
        &full.fixture.root,
        &original_request,
        &full
            .fixture
            .root
            .path()
            .join("var/existing-owner-capacity-control"),
        &tools(&full),
    )
    .unwrap();
    assert_eq!(
        control["artifacts"].as_array().unwrap().len(),
        originals.len(),
        "the original owner still accepts all 32"
    );
    let reference = pin(
        full.fixture.root.path(),
        "var/comparison-full-input.json",
        &full.request,
    );
    let output = full
        .fixture
        .root
        .path()
        .join("var/comparison-full-rejected");
    let error = freeze(&full.fixture.root, &reference, &output).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("design_comparison_feedback_capacity"),
        "{error:#}"
    );
    assert!(!output.join("completed.json").exists());
}
