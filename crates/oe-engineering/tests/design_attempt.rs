//! Independent bounded design-attempt regressions. Synthetic evidence is never
//! relabeled as native CAD, physical acceptance or unseen method improvement.
#[path = "support/design_attempt_fixture.rs"]
mod fixture;
#[path = "../../oe-knowledge/tests/support/incad_fixture.rs"]
mod incad_fixture;

use fixture::{Fixture, quantity, save};
use oe_contracts::{EngineeringDesignAttemptRecordRequest, EngineeringDesignAttemptRequest};
use oe_engineering::design_attempt::{prepare, prepare_to, record, verify_preparation};
use oe_knowledge::poppler::PopplerTools;
use serde_json::{Value, json};
use std::fs;

fn check<'a>(value: &'a Value, id: &str) -> &'a Value {
    value["engineering_checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["id"] == id)
        .unwrap_or_else(|| panic!("missing original check {id}"))
}
fn project_bytes(f: &Fixture) -> Vec<u8> {
    fs::read(f.root.path().join("var/project.json")).unwrap()
}
fn frozen(
    f: &Fixture,
    request: &EngineeringDesignAttemptRequest,
) -> oe_contracts::ArtifactReference {
    let reference = f.reference("attempt", &json!(request));
    prepare_to(
        &f.root,
        &reference,
        &f.root.path().join("var/design-prepared"),
    )
    .unwrap();
    reference
}
fn recording(
    f: &Fixture,
    request: &EngineeringDesignAttemptRequest,
) -> EngineeringDesignAttemptRecordRequest {
    serde_json::from_value(json!({
        "schema":"oev2.design-attempt-record/1","attempt":frozen(f, request),
        "prepared":"var/design-prepared","artifacts":[],"review":null,
        "reference_exposure":[{"stage":"after_freeze","basis":"No new native execution or unexposed task was supplied in this synthetic observation.","source_refs":["task"]}],
        "notes":["Development evidence only; manufacturing and physical obligations remain open."]
    })).unwrap()
}

#[test]
fn freeze_preserves_independent_intent_complete_work_and_partial_source_limits() {
    let f = Fixture::new();
    let request = f.design_request();
    let before = project_bytes(&f);
    let prepared = prepare(&f.root, &request).unwrap();
    assert_eq!(prepared.project_bytes, before);
    assert_eq!(
        prepared.work_bytes,
        fs::read(f.root.path().join("var/work.json")).unwrap()
    );
    assert_eq!(
        prepared.intent_bytes,
        fs::read(f.root.path().join(&request.independent_intent.path)).unwrap()
    );
    assert_eq!(prepared.result["work"], f.work);
    assert_eq!(prepared.result["basis"], f.basis);
    assert_eq!(
        prepared.result["condition_text"],
        f.project.conditions["study"]
    );
    assert_eq!(
        prepared.result["work"]["acceptance"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        prepared.result["reference_exposure"],
        json!(request.reference_exposure)
    );
    assert_eq!(prepared.result["evaluation_scope"], "development_replay");
    assert_eq!(prepared.result["engineering_conclusion"], "unknown");
    for field in ["work_execution", "native_execution", "model_execution"] {
        assert_eq!(prepared.result[field], "not_run", "{field}");
    }
    for field in ["design_complete", "adopted", "project_mutated"] {
        assert_eq!(prepared.result[field], false, "{field}");
    }
    assert_eq!(
        prepared.result["incad"],
        oe_engineering::incad::prepare(&f.root, &f.request)
            .unwrap()
            .result
    );
    for id in [
        "engineering:beam-boundary",
        "engineering:source-boundary",
        "engineering:guide-function",
    ] {
        assert_eq!(check(&prepared.result, id)["conclusion"], "pass");
    }
    frozen(&f, &request);
    verify_preparation(
        &f.root,
        &request,
        &f.root.path().join("var/design-prepared"),
    )
    .unwrap();
    prepared.ensure_current(&f.root).unwrap();
    assert_eq!(project_bytes(&f), before);
}

#[test]
fn omitted_obligations_wrong_stage_mapping_and_unbound_exposure_are_rejected() {
    let f = Fixture::new();
    let valid = f.design_request();
    let before = project_bytes(&f);
    let mut cases = vec![];
    let mut omitted = valid.clone();
    for step in &mut omitted.tool_plan {
        step.acceptance.pop();
    }
    cases.push(("lost physical acceptance", omitted));
    let mut invented = valid.clone();
    invented.tool_plan[0]
        .acceptance
        .push("Changed target: 0.2 mm is sufficient".into());
    cases.push(("weakened extra criterion", invented));
    let mut duplicate = valid.clone();
    duplicate.tool_plan[1].step_id = duplicate.tool_plan[0].step_id.clone();
    cases.push(("duplicate step identity", duplicate));
    let mut activity = valid.clone();
    activity.tool_plan[0].activity = "declare_success".into();
    cases.push(("invented execution activity", activity));
    let mut missing_stage = valid.clone();
    missing_stage
        .tool_plan
        .retain(|step| step.activity != "kernel_review");
    cases.push(("missing formal review stage", missing_stage));
    let mut exposure = valid.clone();
    exposure.reference_exposure[0].source_refs = vec!["unregistered-reference".into()];
    cases.push(("unbound exposure", exposure));
    for (label, request) in cases {
        assert!(prepare(&f.root, &request).is_err(), "{label}");
        assert_eq!(project_bytes(&f), before, "{label}");
    }
}

#[test]
fn source_intent_and_current_project_drift_cannot_reuse_frozen_preparation() {
    for relative in [
        "var/independent-intent.json",
        "var/work.json",
        "var/project.json",
        "var/incad-prepared/specifications.json",
    ] {
        let f = Fixture::new();
        let request = f.design_request();
        let prepared = prepare(&f.root, &request).unwrap();
        frozen(&f, &request);
        let path = f.root.path().join(relative);
        let original = fs::read(&path).unwrap();
        let mut changed = original.clone();
        changed.push(b'\n');
        fs::write(&path, &changed).unwrap();
        assert!(prepare(&f.root, &request).is_err(), "{relative}");
        assert!(
            verify_preparation(
                &f.root,
                &request,
                &f.root.path().join("var/design-prepared")
            )
            .is_err(),
            "{relative}"
        );
        assert!(prepared.ensure_current(&f.root).is_err(), "{relative}");
        fs::write(path, original).unwrap();
        prepared.ensure_current(&f.root).unwrap();
    }
    let f = Fixture::new();
    let request = f.design_request();
    fs::remove_file(f.root.path().join("var/incad-prepared/completed.json")).unwrap();
    assert!(prepare(&f.root, &request).is_err());

    let f = Fixture::new();
    let request = f.design_request();
    let prepared = prepare(&f.root, &request).unwrap();
    frozen(&f, &request);
    let mut changed = json!(f.project);
    changed["decisions"] = json!([{
        "id":"later-author-decision","object_id":"guide","object_revision":"r1","condition_scope":"study",
        "source_sha256":f.object.source_sha256,"conclusion":"unknown","adopted":false,"basis":["beam-boundary"]
    }]);
    assert_eq!(changed["revision"], f.project.revision);
    save(&f.root.path().join("var/project.json"), &changed);
    assert!(
        prepared.ensure_current(&f.root).is_err(),
        "same revision does not mean same complete Project"
    );
    assert!(
        verify_preparation(
            &f.root,
            &request,
            &f.root.path().join("var/design-prepared")
        )
        .is_err()
    );
}

#[test]
fn refreshing_all_hashes_does_not_erase_model_source_or_function_conflicts() {
    for (case, id, conclusion, reason) in [
        (
            "load",
            "engineering:beam-boundary",
            "violated",
            "engineering_model_condition_mismatch",
        ),
        (
            "medium",
            "engineering:source-boundary",
            "violated",
            "engineering_source_not_applicable",
        ),
        (
            "function",
            "engineering:guide-function",
            "violated",
            "engineering_necessary_function_unassigned",
        ),
        (
            "unknown",
            "engineering:source-boundary",
            "unknown",
            "engineering_condition_unknown",
        ),
    ] {
        let mut f = Fixture::new();
        match case {
            "load" => f.basis["facts"][0]["value"] = quantity(20.0, "N"),
            "medium" => f.basis["facts"][7]["value"] = json!("glycol25"),
            "function" => {
                f.basis["assertions"][2]["requirements"]["current_realization"] = Value::Null
            }
            "unknown" => f.basis["facts"][7]["status"] = json!("unknown"),
            _ => unreachable!(),
        }
        f.rebind();
        let request = f.design_request();
        let before = project_bytes(&f);
        let prepared = prepare(&f.root, &request).unwrap();
        let diagnostic = check(&prepared.result, id);
        assert_eq!(diagnostic["conclusion"], conclusion, "{case}: {diagnostic}");
        assert!(
            diagnostic.to_string().contains(reason),
            "{case}: {diagnostic}"
        );
        assert_eq!(prepared.result["design_complete"], false);
        assert_eq!(prepared.result["adopted"], false);
        assert_eq!(project_bytes(&f), before);
    }
}

fn no_external_tools(f: &Fixture) -> PopplerTools {
    let absent = f.root.path().join("var/no-external-tool-may-run");
    PopplerTools {
        pdfinfo: absent.clone(),
        pdftotext: absent.clone(),
        pdftoppm: absent,
    }
}

#[test]
fn actual_work_arithmetic_does_not_complete_missing_cad_review_or_physical_acceptance() {
    let f = Fixture::new();
    f.install_method_sources();
    let attempt = f.design_request();
    let input = recording(&f, &attempt);
    let before = project_bytes(&f);
    let result = record(
        &f.root,
        &input,
        &f.root.path().join("var/record"),
        &no_external_tools(&f),
    )
    .unwrap();
    assert_eq!(result["schema"], "oev2.design-attempt-recorded/1");
    assert_eq!(result["work"]["schema"], "oev2.engineering-work-result/1");
    let calculation = &result["work"]["calculations"][0];
    assert_eq!(calculation["execution"], "completed");
    // Independent reference for the declared load and cantilever boundary:
    // delta = 10 * 100^3 / (3 * 200000 * 1000) = 1/60 mm.
    assert!((calculation["result"]["value"].as_f64().unwrap() - 1.0 / 60.0).abs() < 1e-12);
    assert_eq!(calculation["result"]["unit"], "mm");
    assert_eq!(result["work"]["handoff"]["execution"], "not_run");
    assert_eq!(result["semantic_review"]["execution"], "not_run");
    let obligations = result["remaining_acceptance"].as_array().unwrap();
    assert_eq!(
        obligations.len(),
        f.work["acceptance"].as_array().unwrap().len()
    );
    for (row, original) in obligations
        .iter()
        .zip(f.work["acceptance"].as_array().unwrap())
    {
        assert_eq!(&row["acceptance"], original);
        assert_eq!(row["engineering_acceptance"], "not_established");
        assert!(!row["planned_steps"].as_array().unwrap().is_empty());
    }
    assert_eq!(
        result["reference_exposure"]["frozen"],
        json!(attempt.reference_exposure)
    );
    assert_eq!(
        result["reference_exposure"]["subsequent"],
        json!(input.reference_exposure)
    );
    assert_eq!(result["evaluation_scope"], "development_replay");
    for field in ["design_complete", "adopted", "project_mutated"] {
        assert_eq!(result[field], false);
    }
    assert_eq!(project_bytes(&f), before);
}

#[test]
fn native_success_words_and_author_feedback_cannot_forge_owner_verification() {
    let f = Fixture::new();
    f.install_method_sources();
    let attempt = f.design_request();
    let mut input = recording(&f, &attempt);
    let forged = f.reference("invented-native-pass", &json!({
        "schema":"unverified.author.native-summary/1","project_id":f.project.project_id,
        "object":f.object,"status":"COMPLETED","save":true,"independent_reopen":true,"engineering_adoption":"pass"
    }));
    let feedback = f.reference("author-feedback", &json!({
        "schema":"author.note/1","claim":"I think all criteria passed","candidate_disposition":"no_delta",
        "unknowns":["native files and physical measurements were not supplied"]
    }));
    input.artifacts = serde_json::from_value(json!([
        {"step_id":"step-cad_evidence","role":"cad_evidence","artifact":forged,"source_root":null},
        {"step_id":"step-feedback","role":"feedback","artifact":feedback,"source_root":null}
    ]))
    .unwrap();
    let before = project_bytes(&f);
    let result = record(
        &f.root,
        &input,
        &f.root.path().join("var/record-false-native"),
        &no_external_tools(&f),
    )
    .unwrap();
    let rows = result["artifacts"].as_array().unwrap();
    let native = rows
        .iter()
        .find(|row| row["role"] == "cad_evidence")
        .unwrap();
    assert_eq!(native["verification"]["execution"], "failed");
    assert_eq!(native["verification"]["conclusion"], "unknown");
    assert!(
        !native["authority"]
            .as_str()
            .unwrap()
            .contains("owner checked source consistency")
    );
    let note = rows.iter().find(|row| row["role"] == "feedback").unwrap();
    assert_eq!(note["authority"], "author_report_only");
    assert_eq!(note["original"]["claim"], "I think all criteria passed");
    assert_eq!(result["design_complete"], false);
    assert_eq!(result["adopted"], false);
    assert_eq!(result["remaining_acceptance"].as_array().unwrap().len(), 4);
    assert_eq!(project_bytes(&f), before);
}

#[test]
fn artifact_step_role_mismatch_and_changed_payload_are_not_execution_evidence() {
    for case in ["wrong-activity", "unknown-step", "changed-bytes"] {
        let f = Fixture::new();
        f.install_method_sources();
        let attempt = f.design_request();
        let mut input = recording(&f, &attempt);
        let reference = f.reference("feedback", &json!({"note":"No reusable method delta"}));
        input.artifacts = serde_json::from_value(json!([{
            "step_id":match case { "wrong-activity" => "step-cad_evidence", "unknown-step" => "not-in-frozen-plan", _ => "step-feedback" },
            "role":"feedback","artifact":reference,"source_root":null
        }])).unwrap();
        if case == "changed-bytes" {
            save(
                &f.root.path().join(&reference.path),
                &json!({"note":"different assertion after freeze"}),
            );
        }
        let before = project_bytes(&f);
        assert!(
            record(
                &f.root,
                &input,
                &f.root.path().join("var/rejected-record"),
                &no_external_tools(&f)
            )
            .is_err(),
            "{case}"
        );
        assert_eq!(project_bytes(&f), before);
    }
}

#[test]
fn actual_execution_keeps_conflicting_boundary_and_arithmetic_separate() {
    let mut f = Fixture::new();
    f.install_method_sources();
    f.basis["facts"][0]["value"] = quantity(20.0, "N");
    f.rebind();
    let attempt = f.design_request();
    let input = recording(&f, &attempt);
    let result = record(
        &f.root,
        &input,
        &f.root.path().join("var/record-mismatch"),
        &no_external_tools(&f),
    )
    .unwrap();
    assert_eq!(
        check(&result, "engineering:beam-boundary")["conclusion"],
        "violated"
    );
    assert_eq!(result["work"]["status"], "partial");
    let calculation = &result["work"]["calculations"][0];
    assert_eq!(calculation["execution"], "completed");
    assert!((calculation["result"]["value"].as_f64().unwrap() - 1.0 / 60.0).abs() < 1e-12);
    assert_eq!(calculation["model_applicability"]["conclusion"], "violated");
    assert_eq!(calculation["current_support"], "not_established");
    assert_eq!(result["design_complete"], false);
    assert_eq!(result["adopted"], false);
}

#[test]
fn actual_partial_and_failed_work_can_continue_as_bound_history_without_becoming_success() {
    for outcome in ["partial", "failed"] {
        let mut f = Fixture::new();
        if outcome == "partial" {
            f.install_method_sources();
            // Real arithmetic still runs, but this current load contradicts
            // its independently declared 10 N applicability boundary.
            f.basis["facts"][0]["value"] = quantity(20.0, "N");
            f.rebind();
        } else {
            // Leave the actual method-source files absent: workflow::run must
            // return its real source-loading error, without a mock result.
            assert!(
                !f.root
                    .path()
                    .join("references/supplier-knowledge-policy.json")
                    .exists()
            );
        }
        let attempt = f.design_request();
        let mut input = recording(&f, &attempt);
        let before = project_bytes(&f);
        let work_bytes = fs::read(f.root.path().join("var/work.json")).unwrap();
        let first = record(
            &f.root,
            &input,
            &f.root.path().join("var/record-first"),
            &no_external_tools(&f),
        )
        .unwrap();
        let original_path = "var/record-first/work-result.json";
        let original_bytes = fs::read(f.root.path().join(original_path)).unwrap();
        let historical: Value = serde_json::from_slice(&original_bytes).unwrap();
        assert_eq!(historical, first["work"], "{outcome}");
        assert_eq!(historical["schema"], "oev2.engineering-work-result/1");
        assert_eq!(
            historical["work_input_sha256"],
            oe_core::sha256(&work_bytes)
        );
        assert_eq!(historical["project_reference"], f.work["project"]);
        assert_eq!(historical["object"], f.work["object"]);
        assert_eq!(historical["acceptance"], f.work["acceptance"]);
        if outcome == "partial" {
            assert_eq!(historical["status"], "partial");
            assert_eq!(historical["calculations"][0]["execution"], "completed");
            assert_eq!(
                historical["calculations"][0]["model_applicability"]["conclusion"],
                "violated"
            );
        } else {
            assert_eq!(historical["execution"], "failed");
            assert_eq!(historical["conclusion"], "unknown");
            assert!(!historical["reason"].as_str().unwrap().is_empty());
        }
        assert_eq!(historical["engineering_conclusion"], "unknown");
        assert_eq!(historical["adopted"], false);
        input.artifacts = serde_json::from_value(json!([{
            "step_id":"step-work","role":"work_result",
            "artifact":{"path":original_path,"sha256":oe_core::sha256(&original_bytes)},
            "source_root":null
        }]))
        .unwrap();
        let second = record(
            &f.root,
            &input,
            &f.root.path().join("var/record-second"),
            &no_external_tools(&f),
        )
        .unwrap();
        let imported = &second["artifacts"][0];
        assert_eq!(imported["original"], historical, "{outcome}");
        assert_eq!(imported["verification"]["execution"], "completed");
        assert_eq!(
            imported["verification"]["status"],
            "bound_historical_result"
        );
        if outcome == "partial" {
            assert_eq!(second["work"]["status"], "partial");
        } else {
            assert_eq!(second["work"]["execution"], "failed");
        }
        assert_eq!(second["remaining_acceptance"].as_array().unwrap().len(), 4);
        for row in second["remaining_acceptance"].as_array().unwrap() {
            assert_eq!(row["engineering_acceptance"], "not_established");
        }
        assert_eq!(second["engineering_conclusion"], "unknown");
        for flag in ["design_complete", "adopted", "project_mutated"] {
            assert_eq!(second[flag], false, "{outcome}: {flag}");
        }
        assert_eq!(
            fs::read(f.root.path().join(original_path)).unwrap(),
            original_bytes
        );
        assert_eq!(project_bytes(&f), before);
    }
}

#[test]
fn local_goal_support_retains_parent_obligations_and_detects_rebound_parent_drift() {
    let mut f = Fixture::new();
    let parent = oe_contracts::ObjectIdentity {
        object_id: "machine".into(),
        revision: "r1".into(),
        condition_scope: "machine-goal".into(),
        source_sha256: f.object.source_sha256.clone(),
    };
    f.project.objects.insert("machine".into(), "r1".into());
    let parent_basis = json!({
        "schema":"oev2.engineering-basis/1","object_id":"machine","object_revision":"r1",
        "purpose":f.project.purpose,"acceptance":["Machine physical function and inspection must be established"],"facts":[],"assertions":[]
    });
    let parent_raw = parent_basis.to_string();
    f.project
        .conditions
        .insert("machine-goal".into(), parent_raw.clone());
    f.basis["schema"] = json!("oev2.engineering-basis/2");
    f.basis["purpose"] =
        json!("Review a local guide candidate without declaring machine completion");
    let obligations: Vec<_> = f.work["acceptance"].as_array().unwrap().iter().enumerate().map(|(i, acceptance)| json!({
        "obligation_id":format!("local-{i}"),"local_acceptance":acceptance,
        "parent_acceptance":"Machine physical function and inspection must be established",
        "contribution":"This local design obligation contributes evidence; machine acceptance remains unestablished.",
        "assertion_ids":["guide-function"],"fact_properties":[]
    })).collect();
    let local_sha = oe_core::sha256(oe_core::json::canonical_json(&f.basis).unwrap());
    f.basis["goal_support"] = json!({
        "schema":"oev2.engineering-goal-support/1","project_id":f.project.project_id,"project_revision":f.project.revision,
        "project_purpose":f.project.purpose,"local_object":f.object,"parent_object":parent,
        "parent_condition_sha256":oe_core::sha256(parent_raw.as_bytes()),"local_basis_sha256":local_sha,
        "sources":[{"source_ref":"supplier_html","source_sha256":f.project.evidence["supplier_html"]},{"source_ref":"task","source_sha256":f.project.evidence["task"]}],
        "obligations":obligations,"affected_decisions":[],"claim_scope":"local_obligation_only"
    });
    f.rebind();
    let request = f.design_request();
    let prepared = prepare(&f.root, &request).unwrap();
    assert_eq!(
        prepared.result["basis"], f.basis,
        "full /2 survives, including goal_support"
    );
    let goal = check(&prepared.result, "engineering_goal_support");
    assert_eq!(goal["conclusion"], "pass");
    assert_eq!(
        goal["result"]["details"]["total_goal_achievement"],
        "not_established"
    );
    let remaining = goal["result"]["details"]["remaining_parent_obligations"]
        .as_array()
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0]["acceptance"], parent_basis["acceptance"][0]);
    assert_eq!(remaining[0]["achievement"], "not_established");
    assert_eq!(prepared.result["design_complete"], false);

    let mut changed = parent_basis;
    changed["acceptance"][0] = json!("Additional real machine endurance test is required");
    f.project
        .conditions
        .insert("machine-goal".into(), changed.to_string());
    // All transport/current-Project hashes are refreshed. The independently
    // frozen goal-support relationship must still reject its old parent body.
    f.rebind();
    let request = f.design_request();
    let updated = prepare(&f.root, &request).unwrap();
    let goal = check(&updated.result, "engineering_goal_support");
    assert_eq!(goal["conclusion"], "violated");
    assert!(
        goal.to_string()
            .contains("engineering_goal_parent_condition_changed")
    );
    assert_eq!(updated.result["adopted"], false);
}
