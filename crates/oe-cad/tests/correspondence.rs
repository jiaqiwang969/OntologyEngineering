//! Independent development-replay controls. No native process or API is launched.
//! These controls cover correspondence software, not the complete 16-case pilot.
//! P01/P03/P04 instance positives, P08 actual reopen, Project adoption and physical
//! acceptance stay pending until their separate owners supply real evidence.
#[path = "support/correspondence_fixture.rs"]
mod fixture;

use fixture::{Fixture, change, observation, selector};
use oe_cad::evidence::correspondence::{self, VerifiedCorrespondence};
use oe_contracts::{CadCorrespondenceRequest1, CadCorrespondenceState1, CircularEdgeIntentV2};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs};

fn pair(
    f: &Fixture,
    after: &str,
    selectors: &[&str],
) -> (CadCorrespondenceState1, CadCorrespondenceState1) {
    (
        f.endpoint("before", "r1", observation("before"), &["left_before"]),
        f.endpoint("current", "r2", observation(after), selectors),
    )
}
fn verify(f: &Fixture, request: &CadCorrespondenceRequest1) -> VerifiedCorrespondence {
    correspondence::verify(&f.root, request).unwrap()
}
fn assert_not_qualified(checked: &VerifiedCorrespondence, request: &CadCorrespondenceRequest1) {
    assert!(
        checked
            .require_current_feature(
                "hole:left",
                &request.current.object,
                checked.report()["current"]["native_result_sha256"]
                    .as_str()
                    .unwrap_or("")
            )
            .is_err()
    );
    assert_eq!(checked.report()["engineering_adoption"], "not_established");
    assert_eq!(checked.report()["native_dispatch"], "not_run");
}
fn assert_error(error: anyhow::Error, expected: &str) {
    let chain = format!("{error:#}");
    assert!(
        chain.contains(expected),
        "expected {expected}; received {chain}"
    );
}

#[test]
fn diameter_change_rebinds_both_holes_without_inventing_right_hole_redesign() {
    let f = Fixture::new();
    let previous = f.endpoint(
        "before",
        "r1",
        observation("before"),
        &["left_before", "right_before"],
    );
    let current = f.endpoint(
        "current",
        "r2",
        observation("diameter"),
        &["left_after", "right_before"],
    );
    let request = f.retained(previous, current);
    let checked = verify(&f, &request);
    let rows = checked.report()["changes"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(
            row["correspondence_status"],
            "bounded_declared_correspondence"
        );
        assert_eq!(row["evidence_rebinding_required"], true);
        assert_eq!(row["review"], "required");
        assert_eq!(row["engineering_adoption"], "not_established");
    }
    assert_eq!(
        rows[0]["declared_design_changes"],
        json!(["selector.radius"])
    );
    assert_eq!(rows[1]["declared_design_changes"], json!([]));
    assert_eq!(
        rows[0]["before_checks"][0]["candidate_edge_tags"],
        json!([101])
    );
    assert_eq!(
        rows[0]["after_checks"][0]["candidate_edge_tags"],
        json!([201])
    );
    assert_eq!(
        rows[1]["after_checks"][0]["candidate_edge_tags"],
        json!([202])
    );
    let expected: BTreeSet<String> = [
        "plate",
        "native-before",
        "native-current",
        "drawing:left",
        "fit:left",
        "inspection:left",
        "drawing:right",
        "fit:right",
        "inspection:right",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(checked.impact_seed_ids(), expected);
    let current = checked
        .require_current_feature(
            "hole:right",
            &request.current.object,
            &f.native_sha(&request.current),
        )
        .unwrap();
    assert_eq!(current["creator_lineage"], "not_verified_by_this_profile");
    assert_eq!(
        checked.report()["persistent_topology_naming"],
        "not_established"
    );
    assert_eq!(checked.request(), &request);
}

#[test]
fn identical_observation_and_intent_produce_no_correspondence_delta_without_adoption() {
    let f = Fixture::new();
    let state = f.endpoint("unchanged", "r1", observation("before"), &["left_before"]);
    let request = f.retained(state.clone(), state);
    let checked = verify(&f, &request);
    let row = &checked.report()["changes"][0];
    assert_eq!(
        row["correspondence_status"],
        "bounded_declared_correspondence"
    );
    assert_eq!(row["declared_design_changes"], json!([]));
    assert_eq!(row["evidence_rebinding_required"], false);
    assert_eq!(row["review"], "no_correspondence_delta");
    assert!(checked.impact_seed_ids().is_empty());
    assert_eq!(
        checked.report()["project_review"],
        "requires_existing_kernel_check"
    );
    assert_eq!(row["engineering_adoption"], "not_established");
}

#[test]
fn changed_transient_tags_use_fresh_observation_not_old_process_identity() {
    let f = Fixture::new();
    let previous = f.endpoint(
        "before",
        "r1",
        observation("before"),
        &["left_before", "right_before"],
    );
    let current = f.endpoint(
        "retagged",
        "r1",
        observation("retagged"),
        &["left_before", "right_before"],
    );
    assert_eq!(previous.object, current.object);
    let request = f.retained(previous, current);
    let checked = verify(&f, &request);
    assert_eq!(
        checked.report()["changes"][0]["after_checks"][0]["candidate_edge_tags"],
        json!([701])
    );
    assert_eq!(
        checked.report()["changes"][1]["after_checks"][0]["candidate_edge_tags"],
        json!([702])
    );
    assert_eq!(
        checked.report()["changes"][0]["declared_design_changes"],
        json!([])
    );
    assert_eq!(
        checked.report()["changes"][0]["evidence_rebinding_required"],
        true
    );
    assert_error(
        checked
            .require_current_feature(
                "hole:left",
                &request.current.object,
                &f.native_sha(&request.previous),
            )
            .unwrap_err(),
        "cad_correspondence_current_observation_differs",
    );
    assert!(
        checked
            .require_current_feature(
                "hole:left",
                &request.current.object,
                &f.native_sha(&request.current)
            )
            .is_ok()
    );
    // This synthetic re-tagging control is not evidence of actual independent NX reopen.
}

#[test]
fn zero_matches_preserve_missing_feature_and_old_obligations() {
    let f = Fixture::new();
    let (before, current) = pair(&f, "deleted", &["left_before"]);
    let request = f.retained(before, current);
    let checked = verify(&f, &request);
    let row = &checked.report()["changes"][0];
    assert_eq!(row["correspondence_status"], "unresolved");
    assert_eq!(row["reason"], "cad_correspondence_feature_missing");
    assert_eq!(row["after_checks"][0]["candidate_edge_tags"], json!([]));
    assert_eq!(
        row["after_checks"][0]["reason"],
        "model_circle_no_edge_matches"
    );
    assert!(checked.impact_seed_ids().contains("inspection:left"));
    assert_not_qualified(&checked, &request);
}

#[test]
fn two_geometric_candidates_remain_ambiguous_without_nearest_or_tag_tiebreak() {
    let f = Fixture::new();
    let (before, current) = pair(&f, "ambiguous", &["left_before"]);
    let request = f.retained(before, current);
    let checked = verify(&f, &request);
    let row = &checked.report()["changes"][0];
    assert_eq!(row["correspondence_status"], "ambiguous");
    assert_eq!(
        row["after_checks"][0]["candidate_edge_tags"],
        json!([401, 403])
    );
    assert_eq!(row["after_checks"][0]["reason"], "model_circle_ambiguous");
    assert_not_qualified(&checked, &request);
}

#[test]
fn split_and_merge_declarations_cannot_self_certify_topology_lineage() {
    for (kind, before_ids, after_ids) in [
        ("split", vec![0], vec![0, 1]),
        ("merged", vec![0, 1], vec![0]),
    ] {
        let f = Fixture::new();
        let before_selectors: &[&str] = if kind == "split" {
            &["left_before"]
        } else {
            &["left_before", "right_before"]
        };
        let after_selectors: &[&str] = if kind == "split" {
            &["left_before", "right_before"]
        } else {
            &["left_before"]
        };
        let previous = f.endpoint("before", "r1", observation("before"), before_selectors);
        let current = f.endpoint("current", "r2", observation("before"), after_selectors);
        let declaration = change(
            "topology-change",
            kind,
            &previous,
            &current,
            &before_ids,
            &after_ids,
        );
        let request = f.request(previous, current, vec![declaration]);
        let checked = verify(&f, &request);
        assert_eq!(
            checked.report()["changes"][0]["correspondence_status"],
            "unresolved"
        );
        assert_eq!(
            checked.report()["changes"][0]["reason"],
            "cad_correspondence_split_merge_not_supported"
        );
        assert_not_qualified(&checked, &request);
    }
}

#[test]
fn instance_use_is_not_qualified_by_a_unique_definition_circle() {
    let f = Fixture::new();
    let (mut before, mut current) = pair(&f, "before", &["left_before"]);
    for (state, path, x) in [
        (&mut before, "pin:0", -0.020),
        (&mut current, "pin:1", 0.020),
    ] {
        f.replace_intents(state, |value| {
            value["features"]["left_before"]["occurrence"] = json!({
                "occurrence_path":["pilot",path],"prototype_id":"shared-pin",
                "local_to_root_m":[1,0,0,x,0,1,0,0,0,0,1,0,0,0,0,1]
            });
        });
    }
    let request = f.retained(before, current);
    let checked = verify(&f, &request);
    assert_eq!(
        checked.report()["current"]["bindings"][0]["definition_geometry_check"]["conclusion"],
        "pass"
    );
    assert_eq!(
        checked.report()["current"]["bindings"][0]["use_scope"],
        "instance"
    );
    assert_eq!(
        checked.report()["changes"][0]["correspondence_status"],
        "unresolved"
    );
    assert_eq!(
        checked.report()["changes"][0]["reason"],
        "cad_correspondence_instance_not_qualified"
    );
    assert_not_qualified(&checked, &request);
    // P01/P03 positive occurrence binding remains unfinished; this is only a refusal control.
}

#[test]
fn new_copy_and_delete_keep_declarations_separate_from_native_lineage() {
    for kind in ["new", "copied", "deleted"] {
        let f = Fixture::new();
        let before_selectors: &[&str] = if kind == "new" { &[] } else { &["left_before"] };
        let after_selectors: &[&str] = if kind == "deleted" {
            &[]
        } else if kind == "copied" {
            &["right_before"]
        } else {
            &["left_before"]
        };
        let previous = f.endpoint("before", "r1", observation("before"), before_selectors);
        let current = f.endpoint("current", "r2", observation("before"), after_selectors);
        let before_ids: &[usize] = if kind == "new" { &[] } else { &[0] };
        let after_ids: &[usize] = if kind == "deleted" { &[] } else { &[0] };
        let declaration = change(
            "declared-change",
            kind,
            &previous,
            &current,
            before_ids,
            after_ids,
        );
        let request = f.request(previous, current, vec![declaration]);
        let checked = verify(&f, &request);
        assert_eq!(
            checked.report()["changes"][0]["correspondence_status"],
            "declared_only"
        );
        assert_eq!(
            checked.report()["changes"][0]["reason"],
            "cad_correspondence_change_lineage_not_verified"
        );
        assert_not_qualified(&checked, &request);
    }
    let f = Fixture::new();
    let (previous, current) = pair(&f, "before", &["left_before"]);
    let declaration = change("copy", "copied", &previous, &current, &[0], &[0]);
    let request = f.request(previous, current, vec![declaration]);
    assert_error(
        correspondence::verify(&f.root, &request).unwrap_err(),
        "cad_correspondence_copy_requires_new_feature_identity",
    );
}

#[test]
fn current_feature_consumer_rejects_old_object_old_receipt_and_serialized_report() {
    let f = Fixture::new();
    let (before, current) = pair(&f, "diameter", &["left_after"]);
    let request = f.retained(before, current);
    let checked = verify(&f, &request);
    assert_error(
        checked
            .require_current_feature(
                "hole:left",
                &request.previous.object,
                &f.native_sha(&request.current),
            )
            .unwrap_err(),
        "cad_correspondence_current_object_differs",
    );
    assert_error(
        checked
            .require_current_feature(
                "hole:left",
                &request.current.object,
                &f.native_sha(&request.previous),
            )
            .unwrap_err(),
        "cad_correspondence_current_observation_differs",
    );
    let mut forged = checked.report().clone();
    forged["engineering_adoption"] = json!("adopted");
    assert!(
        serde_json::from_value::<CadCorrespondenceRequest1>(forged).is_err(),
        "a saved report is not a source-bound request and cannot restore the capability"
    );
    assert_eq!(checked.report()["engineering_adoption"], "not_established");
}

#[test]
fn changed_original_bytes_cannot_keep_old_declared_hash_and_revision() {
    for target in ["intent", "change", "project", "job"] {
        let f = Fixture::new();
        let (before, current) = pair(&f, "before", &["left_before"]);
        let request = f.retained(before, current);
        let source = match target {
            "intent" => &request.current.bindings[0].intent.source,
            "change" => &request.changes[0].source,
            "project" => &request.current.project,
            "job" => &request.current.job,
            _ => unreachable!(),
        };
        let path = f.root.path().join(&source.path);
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b' '); // Even semantically identical JSON is a new original.
        fs::write(path, bytes).unwrap();
        assert_error(
            correspondence::verify(&f.root, &request).unwrap_err(),
            "cad_correspondence_source_changed",
        );
    }
}

#[test]
fn binding_id_and_complete_change_source_prevent_request_retargeting() {
    // First: a local ID cannot point at a source carrying another ID.
    let f = Fixture::new();
    let (before, mut current) = pair(&f, "before", &["left_before", "right_before"]);
    current.bindings[0].intent.pointer = current.bindings[1].intent.pointer.clone();
    current.bindings.truncate(1);
    let request = f.retained(before, current);
    assert_error(
        correspondence::verify(&f.root, &request).unwrap_err(),
        "cad_correspondence_feature_object_mismatch",
    );

    // Second: even a source with the same ID must equal the frozen declaration's
    // complete original (path, SHA and pointer), not just its local spelling.
    for variant in ["different-source", "different-pointer"] {
        let f = Fixture::new();
        let (before, mut current) = pair(&f, "before", &["left_before"]);
        f.replace_intents(&mut current, |value| {
            value["alias"] = value["features"]["left_before"].clone();
        });
        let mut request = f.retained(before, current);
        if variant == "different-source" {
            let original = f.read_json(&request.current.bindings[0].intent.source);
            request.current.bindings[0].intent.source =
                f.write_json("same-id-other-original", &original);
        } else {
            request.current.bindings[0].intent.pointer = "/alias".into();
        }
        assert_error(
            correspondence::verify(&f.root, &request).unwrap_err(),
            "cad_correspondence_change_binding_source_mismatch",
        );
    }
}

#[test]
fn claimed_success_fields_are_rejected_in_typed_intent_sources() {
    let f = Fixture::new();
    let (before, mut current) = pair(&f, "before", &["left_before"]);
    f.replace_intents(&mut current, |value| {
        value["features"]["left_before"]["correspondence_status"] =
            json!("bounded_declared_correspondence");
    });
    let request = f.retained(before, current);
    assert_error(
        correspondence::verify(&f.root, &request).unwrap_err(),
        "cad_correspondence_source_pointer_contract",
    );
}

#[test]
fn native_worker_model_and_process_mismatch_remain_unavailable_without_fallback() {
    for target in ["worker-source", "model-bytes", "runtime", "process"] {
        let f = Fixture::new();
        let (before, current) = pair(&f, "before", &["left_before"]);
        let request = f.retained(before, current);
        match target {
            "worker-source" => fs::write(
                f.job_directory(&request.current).join("sources/main.rs"),
                b"changed worker source",
            )
            .unwrap(),
            "model-bytes" => fs::write(
                f.job_directory(&request.current).join("inputs/plate.prt"),
                b"changed model, same declared identity",
            )
            .unwrap(),
            "runtime" => f.change_result(&request.current, |result| {
                result.runtime.product_label = "different SDK".into()
            }),
            "process" => f.change_result(&request.current, |result| {
                result.process.host_id = "foreign-host".into()
            }),
            _ => unreachable!(),
        }
        let checked = verify(&f, &request);
        assert_eq!(
            checked.report()["current"]["native_observation"],
            "unavailable"
        );
        assert!(checked.report()["current"]["native_failure"].is_string());
        assert_eq!(
            checked.report()["changes"][0]["correspondence_status"],
            "unresolved"
        );
        assert_not_qualified(&checked, &request);
    }
}

#[test]
fn one_failed_unrelated_edge_prevents_claiming_the_known_match_is_unique() {
    let f = Fixture::new();
    let (before, current) = pair(&f, "partial", &["left_before"]);
    let request = f.retained(before, current);
    let checked = verify(&f, &request);
    let row = &checked.report()["changes"][0];
    assert_eq!(row["after_checks"][0]["candidate_edge_tags"], json!([101]));
    assert_eq!(row["after_checks"][0]["conclusion"], "unknown");
    assert_eq!(row["correspondence_status"], "unresolved");
    assert_eq!(row["reason"], "cad_correspondence_observation_incomplete");
    assert_eq!(
        checked.report()["previous"]["bindings"][0]["definition_geometry_check"]["conclusion"],
        "pass"
    );
    assert_not_qualified(&checked, &request);
}

#[test]
fn failure_unknown_and_incomplete_completion_preserve_receipts_and_remaining_review() {
    for failure in [
        "failed-result",
        "unknown-result",
        "explicit-error",
        "unknown-detail",
        "terminate",
        "running-state",
        "unbound-result",
    ] {
        let f = Fixture::new();
        let (before, current) = pair(&f, "before", &["left_before"]);
        let request = f.retained(before, current);
        match failure {
            "failed-result" => f.change_result(&request.current, |r| {
                r.outcome = oe_nx_protocol::ResultOutcome::Failed
            }),
            "unknown-result" => f.change_result(&request.current, |r| {
                r.outcome = oe_nx_protocol::ResultOutcome::Unknown
            }),
            "explicit-error" => f.change_result(&request.current, |r| {
                r.errors.push("fixture failure kept".into())
            }),
            "unknown-detail" => f.change_result(&request.current, |r| {
                r.unknowns.push("fixture partial result kept".into())
            }),
            "terminate" => f.change_result(&request.current, |r| {
                r.readback["lifecycle"]["terminate_return"] = json!(17)
            }),
            "running-state" => {
                let path = f.job_directory(&request.current).join("state.json");
                let mut state: oe_nx_protocol::StateRecord =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                state.state = oe_nx_protocol::State::UnknownAfterDispatch;
                fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
            }
            "unbound-result" => {
                let path = f.job_directory(&request.current).join("native-result.json");
                let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                value["readback"]["diagnostic"] = json!("result changed after completion");
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            _ => unreachable!(),
        }
        let result_path = f.job_directory(&request.current).join("native-result.json");
        let result_before = fs::read(&result_path).unwrap();
        let checked = verify(&f, &request);
        assert_eq!(
            checked.report()["current"]["native_observation"],
            "unavailable"
        );
        assert_eq!(
            checked.report()["changes"][0]["correspondence_status"],
            "unresolved"
        );
        assert_eq!(checked.report()["changes"][0]["review"], "required");
        assert_eq!(
            fs::read(result_path).unwrap(),
            result_before,
            "verification preserves failure originals"
        );
        assert_not_qualified(&checked, &request);
    }
}

#[test]
fn unit_center_radius_and_complete_domain_checks_retain_original_selector_answers() {
    for failure in ["unit", "center", "radius", "axis", "incomplete-domain"] {
        let f = Fixture::new();
        let mut part = observation("before");
        match failure {
            "unit" => part["model_edges"]["unit"] = json!("inch"),
            "center" => {
                part["model_edges"]["entries"][0]["arc"]["center"] = json!([-19.999, 0, 10])
            }
            "radius" => part["model_edges"]["entries"][0]["arc"]["radius"] = json!(5.001),
            "axis" => part["model_edges"]["entries"][0]["arc"]["y_axis"] = json!([0, 0, 1]),
            "incomplete-domain" => {
                part["model_edges"]["entries"].as_array_mut().unwrap().pop();
            }
            _ => unreachable!(),
        }
        let required: CircularEdgeIntentV2 =
            serde_json::from_value(selector("left_before")).unwrap();
        let original = oe_nx_worker::geometry::model_edges::select_v2(&required, &part).unwrap();
        assert_ne!(original["conclusion"], "pass");
        let before = f.endpoint("before", "r1", observation("before"), &["left_before"]);
        let current = f.endpoint("current", "r2", part, &["left_before"]);
        let request = f.retained(before, current);
        let checked = verify(&f, &request);
        assert_eq!(
            checked.report()["changes"][0]["after_checks"][0],
            original,
            "the correspondence layer must preserve the full specialized answer"
        );
        assert_not_qualified(&checked, &request);
    }
}

#[test]
fn incomplete_change_coverage_and_reused_current_binding_are_refused() {
    for failure in ["coverage", "reused-current", "empty-changes"] {
        let f = Fixture::new();
        let previous = f.endpoint(
            "before",
            "r1",
            observation("before"),
            &["left_before", "right_before"],
        );
        let current = f.endpoint(
            "current",
            "r2",
            observation("before"),
            &["left_before", "right_before"],
        );
        let mut declarations = vec![change("first", "retained", &previous, &current, &[0], &[0])];
        if failure == "reused-current" {
            declarations.push(change(
                "second",
                "retained",
                &previous,
                &current,
                &[1],
                &[0],
            ));
        }
        if failure == "empty-changes" {
            declarations.clear();
        }
        let request = f.request(previous, current, declarations);
        let expected = match failure {
            "coverage" => "cad_correspondence_binding_coverage_incomplete",
            "reused-current" => "cad_correspondence_current_binding_reused",
            "empty-changes" => "cad_correspondence_schema_or_budget",
            _ => unreachable!(),
        };
        assert_error(
            correspondence::verify(&f.root, &request).unwrap_err(),
            expected,
        );
    }
}

#[test]
fn solidworks_endpoint_stays_unqualified_without_nx_or_legacy_fallback() {
    let f = Fixture::new();
    let (before, mut current) = pair(&f, "before", &["left_before"]);
    current.backend = "solidworks".into();
    let request = f.retained(before, current);
    let checked = verify(&f, &request);
    assert_eq!(
        checked.report()["current"]["native_failure"],
        "cad_correspondence_backend_not_qualified"
    );
    assert_not_qualified(&checked, &request);
}
