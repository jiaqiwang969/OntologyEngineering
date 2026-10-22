//! Fixed source and mutation controls; synthetic cases establish provenance
//! behavior only. Original163/169 cases are explicitly development replay.
use oe_contracts::{ArtifactReference, CurrentTaskMethodUseInput};
use oe_core::{SkillRoot, sha256};
use oe_engineering::current_task_view::{
    self,
    method_use::{derive_with_methods, verify_method_view},
};
use oe_knowledge::conversation;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

fn write(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn reference(root: &SkillRoot, path: &Path) -> ArtifactReference {
    ArtifactReference {
        path: path
            .strip_prefix(root.path())
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        sha256: sha256(std::fs::read(path).unwrap()),
    }
}
fn fixture() -> (tempfile::TempDir, SkillRoot, CurrentTaskMethodUseInput) {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("SKILL.md"),
        "explicit synthetic source provenance fixture",
    )
    .unwrap();
    let root = SkillRoot::new(&std::fs::canonicalize(temp.path()).unwrap()).unwrap();
    std::fs::create_dir(root.path().join("var")).unwrap();
    let archive = root.path().join("var/archive.jsonl");
    // Frozen independently of the producer: escaped text, exact parent/time,
    // and a second visible original message which is not selected for use.
    let raw = concat!(
        "{\"type\":\"response_item\",\"timestamp\":\"2026-09-07T06:16:27.770Z\",\"parentUuid\":\"p0\",\"payload\":{\"type\":\"message\",\"id\":\"m1\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"\\u5b54列应随长度增减；制图更新后保存。\\n\"}]}}\n",
        "{\"type\":\"assistant\",\"uuid\":\"m2\",\"parentUuid\":\"m1\",\"timestamp\":\"2026-09-07T06:18:49.162Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"另一个未选消息，不转为当前方法理解。\"}]}}\n"
    );
    std::fs::write(&archive, raw).unwrap();
    let intake = conversation::read_archive(&archive, 512 * 1024 * 1024).unwrap();
    assert_eq!(
        intake["entries"][0]["text"],
        "孔列应随长度增减；制图更新后保存。\n"
    );
    let intake_path = root.path().join("var/intake.json");
    write(&intake_path, &intake);
    let intake_ref = reference(&root, &intake_path);
    let geometry = sha256(b"original geometry source, not physical acceptance");
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"part","object_revision":"V2",
        "purpose":"Original complete service purpose","acceptance":["original service acceptance", "physical measurement remains required"],
        "facts":[{"property":"change_dependency_method","value":{"role":"operation_method_instance","trigger":"size change","action":"reopen holes/mating/drawings"},"status":"declared","source_refs":["intake"]}],"assertions":[]});
    let project = json!({"schema":"oev2.project/1","project_id":"synthetic-source-method-project","revision":"2",
        "purpose":"Original complete service purpose","objects":{"part":"V2","other":"1"},
        "conditions":{"service":serde_json::to_string(&basis).unwrap(),"other":"Unrelated original condition"},
        "evidence":{"geometry":geometry,"intake":intake_ref.sha256},
        "decisions":[{"id":"service","object_id":"part","object_revision":"V2","condition_scope":"service","source_sha256":geometry,"conclusion":"unknown","adopted":false,"basis":["intake"]},
            {"id":"drawing","object_id":"part","object_revision":"V2","condition_scope":"service","source_sha256":geometry,"conclusion":"unknown","adopted":false,"basis":["service"]},
            {"id":"unrelated","object_id":"other","object_revision":"1","condition_scope":"other","source_sha256":geometry,"conclusion":"unknown","adopted":false,"basis":["geometry"]}],
        "dependencies":{"drawing":["service"],"inspection":["drawing"]}});
    let project_path = root.path().join("var/project.json");
    write(&project_path, &project);
    let input: CurrentTaskMethodUseInput = serde_json::from_value(json!({"schema":"oev2.current-task-view-input/2",
        "task":{"schema":"oev2.current-task-view-input/1","current":reference(&root,&project_path),"previous":null,"work":null,"sources":[intake_ref]},
        "methods":[{"object":{"object_id":"part","revision":"V2","condition_scope":"service","source_sha256":geometry},
            "basis_sha256":sha256(project["conditions"]["service"].as_str().unwrap()),"fact_property":"change_dependency_method","source_id":"intake",
            "intake":reference(&root,&intake_path),"archive":reference(&root,&archive),"entry_ids":["r1b0"]}]})).unwrap();
    (temp, root, input)
}
fn resign_intake(root: &SkillRoot, input: &mut CurrentTaskMethodUseInput, value: &Value) {
    let path = root.path().join(&input.methods[0].intake.path);
    write(&path, value);
    input.methods[0].intake = reference(root, &path);
    input.task.sources = vec![input.methods[0].intake.clone()];
    let path = root.path().join(&input.task.current.path);
    let mut project: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    project["evidence"]["intake"] = json!(input.methods[0].intake.sha256);
    write(&path, &project);
    input.task.current = reference(root, &path);
}
fn assert_original_checks(
    root: &SkillRoot,
    input: &CurrentTaskMethodUseInput,
    view: &oe_contracts::CurrentTaskMethodUseView,
) {
    assert_eq!(
        view.task_view,
        current_task_view::derive(root, &input.task).unwrap()
    );
    assert_eq!(view.task_view.model_execution, "not_run");
    assert_eq!(view.task_view.project_update, "none");
    assert_eq!(view.task_view.engineering_adoption, "not_established");
    assert_eq!(view.task_view.arrangement, "unscheduled");
    assert!(view.task_view.decisions.iter().all(|d| {
        serde_json::from_str::<Value>(d.current_value.as_ref().unwrap()).unwrap()["adopted"]
            == false
    }));
}

#[test]
fn selected_original_messages_and_forged_intake_remain_distinct() {
    let (_temp, root, input) = fixture();
    let before = derive_with_methods(&root, &input).unwrap();
    assert_eq!(before.freshness, "matched");
    verify_method_view(&root, &before).unwrap();
    assert_original_checks(&root, &input, &before);
    let method = &before.methods[0];
    assert_eq!(method.fact_status.as_deref(), Some("declared"));
    assert_eq!(method.entries.len(), 1);
    assert_eq!(method.intake_scope.as_ref().unwrap()["entry_count"], 2);
    assert!(
        method
            .intake_scope
            .as_ref()
            .unwrap()
            .get("entries")
            .is_none()
    );
    let row = method.entries[0].intake_entry.as_ref().unwrap();
    assert_eq!(row["role"], "user");
    assert_eq!(row["parent_id"], "p0");
    assert_ne!(
        method.entries[0].text_sha256.as_ref().unwrap(),
        row["raw_sha256"].as_str().unwrap()
    );
    for mutation in [
        "text",
        "role",
        "timestamp",
        "parent_id",
        "record_id",
        "call_id",
        "kind",
        "line_and_id",
        "range_start",
        "range_end",
    ] {
        let (_temp, root, mut input) = fixture();
        let mut intake: Value = serde_json::from_slice(
            &std::fs::read(root.path().join(&input.methods[0].intake.path)).unwrap(),
        )
        .unwrap();
        let row = &mut intake["entries"][0];
        match mutation {
            "text" => row["text"] = json!("伪造已验证收益"),
            "role" => row["role"] = json!("assistant"),
            "timestamp" => row["timestamp"] = json!("2026-10-06T00:00:00Z"),
            "parent_id" => row["parent_id"] = json!("unrelated parent"),
            "record_id" => row["record_id"] = json!("other message"),
            "call_id" => row["call_id"] = json!("invented call"),
            "kind" => row["kind"] = json!("runtime_context"),
            "line_and_id" => {
                row["line"] = json!(99);
                row["id"] = json!("r99b0");
                input.methods[0].entry_ids = vec!["r99b0".into()];
            }
            "range_start" => row["byte_start"] = json!(1),
            "range_end" => row["byte_end"] = json!(row["byte_end"].as_u64().unwrap() - 1),
            _ => unreachable!(),
        }
        resign_intake(&root, &mut input, &intake);
        let view = derive_with_methods(&root, &input).unwrap();
        assert_eq!(view.methods[0].entries[0].status, "refuted", "{mutation}");
        assert_eq!(view.freshness, "refuted", "{mutation}");
        assert!(verify_method_view(&root, &view).is_err(), "{mutation}");
        assert_original_checks(&root, &input, &view);
    }
}

#[test]
fn method_source_missing_drift_and_restoration_preserve_current_checks() {
    let (_temp, root, mut input) = fixture();
    let original = derive_with_methods(&root, &input).unwrap();
    let path = root
        .path()
        .join(&input.methods[0].archive.as_ref().unwrap().path);
    let raw = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let missing = derive_with_methods(&root, &input).unwrap();
    assert_eq!(missing.freshness, "unknown");
    assert_original_checks(&root, &input, &missing);
    assert!(missing.affected.contains(&"drawing".into()));
    assert!(missing.affected.contains(&"inspection".into()));
    assert!(!missing.affected.contains(&"unrelated".into()));
    assert!(
        verify_method_view(&root, &original)
            .unwrap_err()
            .to_string()
            .contains("freshness_unknown")
    );
    std::fs::write(&path, &raw).unwrap();
    assert_eq!(derive_with_methods(&root, &input).unwrap(), original);
    verify_method_view(&root, &original).unwrap();
    std::fs::write(&path, [raw.as_slice(), b"\n"].concat()).unwrap();
    let changed = derive_with_methods(&root, &input).unwrap();
    assert_eq!(changed.freshness, "refuted");
    assert_eq!(
        changed.methods[0]
            .archive_observation
            .as_ref()
            .unwrap()
            .status,
        "drift"
    );
    assert!(
        verify_method_view(&root, &original)
            .unwrap_err()
            .to_string()
            .contains("source_refuted")
    );
    std::fs::write(&path, &raw).unwrap();
    input.methods[0].archive = None;
    let no_reference = derive_with_methods(&root, &input).unwrap();
    assert_eq!(no_reference.freshness, "unknown");
    assert!(no_reference.methods[0].archive_observation.is_none());
    assert!(
        no_reference.methods[0]
            .reasons
            .contains(&"method_original_archive_not_provided".into())
    );
    assert_original_checks(&root, &input, &no_reference);
}

#[test]
fn current_project_fact_scope_and_whole_view_identity_cannot_be_resigned_away() {
    for mutation in [
        "object",
        "scope",
        "basis",
        "fact",
        "source",
        "intake_identity",
    ] {
        let (_temp, root, mut input) = fixture();
        match mutation {
            "object" => input.methods[0].object.revision = "old".into(),
            "scope" => input.methods[0].object.condition_scope = "other".into(),
            "basis" => input.methods[0].basis_sha256 = sha256(b"old basis"),
            "fact" => input.methods[0].fact_property = "not registered".into(),
            "source" => input.methods[0].source_id = "geometry".into(),
            "intake_identity" => input.methods[0].intake.sha256 = sha256(b"resigned summary"),
            _ => unreachable!(),
        }
        let view = derive_with_methods(&root, &input).unwrap();
        assert_eq!(view.freshness, "refuted", "{mutation}");
        assert_original_checks(&root, &input, &view);
    }
    let (_temp, root, mut input) = fixture();
    let old = derive_with_methods(&root, &input).unwrap();
    let path = root.path().join(&input.task.current.path);
    let mut project: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let revision = project["revision"].clone();
    project["purpose"] = json!("Same revision changed whole purpose");
    write(&path, &project);
    assert_eq!(project["revision"], revision);
    assert!(verify_method_view(&root, &old).is_err());
    input.task.current = reference(&root, &path);
    let refreshed = derive_with_methods(&root, &input).unwrap();
    assert_eq!(refreshed.freshness, "matched");
    assert_ne!(
        refreshed.task_view.project_value_sha256,
        old.task_view.project_value_sha256
    );
    let mut invented = refreshed.clone();
    invented.task_view.checks.clear();
    assert!(
        verify_method_view(&root, &invented)
            .unwrap_err()
            .to_string()
            .contains("view_stale")
    );
    invented = refreshed.clone();
    invented.methods[0].entries[0]
        .intake_entry
        .as_mut()
        .unwrap()["role"] = json!("assistant");
    assert!(
        verify_method_view(&root, &invented)
            .unwrap_err()
            .to_string()
            .contains("view_stale")
    );
}

#[test]
fn intake_missing_drift_duplicates_and_original_resource_guards_keep_unknowns() {
    for mutation in [
        "missing",
        "drift",
        "ambiguous",
        "entry_missing",
        "snapshot",
        "source_path",
    ] {
        let (_temp, root, mut input) = fixture();
        let path = root.path().join(&input.methods[0].intake.path);
        let raw = std::fs::read(&path).unwrap();
        let mut intake: Value = serde_json::from_slice(&raw).unwrap();
        match mutation {
            "missing" => std::fs::remove_file(&path).unwrap(),
            "drift" => std::fs::write(&path, [raw.as_slice(), b"\n"].concat()).unwrap(),
            "ambiguous" => {
                let row = intake["entries"][0].clone();
                intake["entries"].as_array_mut().unwrap().push(row);
                intake["entry_count"] = json!(3);
                resign_intake(&root, &mut input, &intake);
            }
            "entry_missing" => {
                input.methods[0].entry_ids = vec!["r999b0".into()];
            }
            "snapshot" => {
                intake["scanned_range_sha256"] = json!(sha256(b"false snapshot"));
                resign_intake(&root, &mut input, &intake);
            }
            "source_path" => {
                intake["source_path"] = json!("/unauthorized/no-read-fallback.jsonl");
                resign_intake(&root, &mut input, &intake);
            }
            _ => unreachable!(),
        }
        let view = derive_with_methods(&root, &input).unwrap();
        assert_eq!(
            view.freshness,
            if mutation == "missing" {
                "unknown"
            } else {
                "refuted"
            },
            "{mutation}"
        );
        assert_original_checks(&root, &input, &view);
        assert!(verify_method_view(&root, &view).is_err());
    }
    let (_temp, root, mut input) = fixture();
    let path = root.path().join("var/over512.jsonl");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(512 * 1024 * 1024 + 1).unwrap();
    drop(file);
    input.methods[0].archive = Some(ArtifactReference {
        path: "var/over512.jsonl".into(),
        sha256: sha256(b"fixed expected; no oversized read"),
    });
    let view = derive_with_methods(&root, &input).unwrap();
    assert_eq!(view.freshness, "unknown");
    assert!(
        view.methods[0]
            .archive_observation
            .as_ref()
            .unwrap()
            .reason
            .as_ref()
            .unwrap()
            .contains("limit")
    );
    assert_original_checks(&root, &input, &view);
}

#[test]
fn same_revision_decisions_adoption_conditions_and_complete_work_reject_resigned_old_views() {
    for mutation in ["decision", "adoption", "condition"] {
        let (_temp, root, input) = fixture();
        let old = derive_with_methods(&root, &input).unwrap();
        let path = root.path().join(&input.task.current.path);
        let raw = std::fs::read(&path).unwrap();
        let mut project: Value = serde_json::from_slice(&raw).unwrap();
        let revision = project["revision"].clone();
        match mutation {
            "decision" => project["decisions"][2]["conclusion"] = json!("refuted"),
            "adoption" => {
                project["decisions"][2]["conclusion"] = json!("supported");
                project["decisions"][2]["adopted"] = json!(true);
            }
            "condition" => {
                project["conditions"]["other"] =
                    json!("Same-revision unrelated complete condition changed")
            }
            _ => unreachable!(),
        }
        assert_eq!(project["revision"], revision);
        write(&path, &project);
        assert!(
            verify_method_view(&root, &old).is_err(),
            "{mutation}: bound raw bytes"
        );
        let mut forged = old.clone();
        forged.input.task.current = reference(&root, &path);
        forged.input_sha256 = sha256(
            oe_core::json::canonical_json(&serde_json::to_value(&forged.input).unwrap()).unwrap(),
        );
        assert!(
            verify_method_view(&root, &forged).is_err(),
            "{mutation}: fresh input SHA cannot authorize stale whole nested view/checks"
        );
        std::fs::write(&path, raw).unwrap();
        verify_method_view(&root, &old).unwrap();
    }
    let (_temp, root, mut input) = fixture();
    let project: oe_kernel::project::Project =
        serde_json::from_slice(&std::fs::read(root.path().join(&input.task.current.path)).unwrap())
            .unwrap();
    let object = input.methods[0].object.clone();
    let basis = oe_kernel::engineering::basis(&project, &object)
        .unwrap()
        .unwrap();
    let context = oe_kernel::project::context(&project, &object).unwrap();
    let work = json!({"schema":"oev2.engineering-work/3","project":{"path":input.task.current.path,"sha256":input.task.current.sha256},
        "object":object,"project_context":context,"acceptance":basis.acceptance.clone(),
        "necessary_functions":["maintain mating and dependent drawings"],"physical_mechanisms":["original dependent feature declaration"],
        "gaps":[],"source_requests":[],"calculations":[],"handoff":null});
    let path = root.path().join("var/work.json");
    write(&path, &work);
    input.task.work = Some(reference(&root, &path));
    let old = derive_with_methods(&root, &input).unwrap();
    let raw = std::fs::read(&path).unwrap();
    for field in ["physical_mechanisms", "necessary_functions", "acceptance"] {
        let mut changed = work.clone();
        changed[field][0] = json!("Same project revision changed complete work field");
        write(&path, &changed);
        assert!(
            verify_method_view(&root, &old).is_err(),
            "{field}: old work byte identity"
        );
        let mut forged = old.clone();
        forged.input.task.work = Some(reference(&root, &path));
        forged.input_sha256 = sha256(
            oe_core::json::canonical_json(&serde_json::to_value(&forged.input).unwrap()).unwrap(),
        );
        assert!(
            verify_method_view(&root, &forged).is_err(),
            "{field}: re-signed input cannot keep old necessary checks or original acceptance"
        );
        std::fs::write(&path, &raw).unwrap();
        verify_method_view(&root, &old).unwrap();
    }
}

#[test]
fn window_lines_and_original_two_mib_record_guard_are_explicit() {
    let (_temp, root, mut input) = fixture();
    let path = root
        .path()
        .join(&input.methods[0].archive.as_ref().unwrap().path);
    let raw = std::fs::read(&path).unwrap();
    let start = raw.iter().position(|b| *b == b'\n').unwrap() + 1;
    let window = conversation::read_window(&path, start as u64, raw.len() as u64).unwrap();
    assert_eq!(window["entries"][0]["line"], 1);
    assert_eq!(window["entries"][0]["role"], "assistant");
    resign_intake(&root, &mut input, &window);
    let view = derive_with_methods(&root, &input).unwrap();
    assert_eq!(view.freshness, "matched");
    assert_eq!(
        view.methods[0].intake_scope.as_ref().unwrap()["window"]["line_origin"],
        "range_relative"
    );
    verify_method_view(&root, &view).unwrap();
    let raw=serde_json::to_vec(&json!({"type":"response_item","timestamp":"2026-10-06T00:00:00Z","payload":{"type":"message","id":"large","role":"user","content":[{"type":"input_text","text":"x".repeat(2*1024*1024)}]}})).unwrap();
    let mut raw = raw;
    raw.push(b'\n');
    std::fs::write(&path, &raw).unwrap();
    let mut gap_intake = conversation::read_archive(&path, 512 * 1024 * 1024).unwrap();
    assert_eq!(gap_intake["issue_count"], 1);
    assert_eq!(
        gap_intake["issues"][0]["code"],
        "record_exceeds_memory_guard"
    );
    let record: Value = serde_json::from_slice(&raw).unwrap();
    let item = conversation::visible_record(&record)[0].clone();
    gap_intake["entries"] = json!([{"id":"r1b0","line":1,"byte_start":0,"byte_end":raw.len(),"raw_sha256":sha256(&raw),"record_id":"large","parent_id":null,"timestamp":"2026-10-06T00:00:00Z","kind":"message","role":"user","text":item["text"],"call_id":null}]);
    gap_intake["entry_count"] = json!(1);
    input.methods[0].archive = Some(reference(&root, &path));
    resign_intake(&root, &mut input, &gap_intake);
    let view = derive_with_methods(&root, &input).unwrap();
    assert_eq!(view.freshness, "refuted");
    assert!(
        view.methods[0].entries[0]
            .reasons
            .iter()
            .any(|r| r.contains("exceeds_original_guard"))
    );
    assert_original_checks(&root, &input, &view);
}

#[test]
fn resigned_intake_statistics_remain_supplied_declarations_outside_selected_message_proof() {
    let (_temp, root, mut input) = fixture();
    let original = derive_with_methods(&root, &input).unwrap();
    let mut intake: Value = serde_json::from_slice(
        &std::fs::read(root.path().join(&input.methods[0].intake.path)).unwrap(),
    )
    .unwrap();
    intake["status"] = json!("partial");
    intake["issues"] = json!([{"code":"re_signed_unverified_statistic","claim":"all old records read; not an authenticated fact"}]);
    intake["issue_count"] = json!(1);
    intake["record_types"] = json!({"re_signed_unverified_claim":12345});
    resign_intake(&root, &mut input, &intake);
    let current = derive_with_methods(&root, &input).unwrap();
    assert_eq!(
        current.freshness, "matched",
        "Only selected original messages remain verified; no full corpus scan was requested"
    );
    assert_eq!(current.proof_scope, "selected_message_provenance_only");
    assert_eq!(
        current.methods[0].intake_statistics_verification,
        "historical_supplied_declaration"
    );
    assert_eq!(
        current.methods[0].intake_scope.as_ref().unwrap()["issues"],
        intake["issues"]
    );
    assert_eq!(
        current.methods[0].intake_scope.as_ref().unwrap()["record_types"],
        intake["record_types"]
    );
    assert_eq!(
        current.methods[0].entries[0],
        original.methods[0].entries[0]
    );
    assert!(
        verify_method_view(&root, &original).is_err(),
        "Source identity/whole Project changed even though selected message did not"
    );
    verify_method_view(&root, &current).unwrap();
    assert_original_checks(&root, &input, &current);
}

#[test]
fn original_one_envelopes_remain_exact_and_cannot_silently_accept_two() {
    let (_temp, root, input) = fixture();
    let old = current_task_view::derive(&root, &input.task).unwrap();
    let view = derive_with_methods(&root, &input).unwrap();
    assert_eq!(
        serde_json::to_value(&view.task_view).unwrap(),
        serde_json::to_value(&old).unwrap()
    );
    assert_eq!(view.task_view.input, input.task);
    assert!(
        serde_json::from_value::<oe_contracts::CurrentTaskViewInput>(
            serde_json::to_value(&input).unwrap()
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<oe_contracts::CurrentTaskView>(
            serde_json::to_value(&view).unwrap()
        )
        .is_err()
    );
    let mut mixed = serde_json::to_value(&input.task).unwrap();
    mixed["methods"] = serde_json::to_value(&input.methods).unwrap();
    assert!(serde_json::from_value::<oe_contracts::CurrentTaskViewInput>(mixed).is_err());
}

#[test]
fn original163_and_both_complete169_tasks_are_development_replay() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let oracle: Value =
        serde_json::from_slice(&std::fs::read(parent.join("ORACLE-01.json")).unwrap()).unwrap();
    let root = SkillRoot::new(&parent.join("workspace")).unwrap();
    let history = "var/maintenance/engineering-process-163-169-20261001-01";
    for (project_name,work_name,decisions,acceptance) in [
        (format!("{history}/169-work-03/project.json"),format!("{history}/169-work-03/work.json"),9,4),
        ("var/maintenance/engineering-goal-token-diagnostics-20261003-01/current169/inputs-01/project-r260.json".into(),"var/maintenance/engineering-goal-token-diagnostics-20261003-01/current169/inputs-01/work-input.json".into(),10,5)
    ] {
        let project:Value=serde_json::from_slice(&std::fs::read(root.path().join(&project_name)).unwrap()).unwrap();
        let work:Value=serde_json::from_slice(&std::fs::read(root.path().join(&work_name)).unwrap()).unwrap();
        let scope="V262-A450-service-current";let body=project["conditions"][scope].as_str().unwrap();
        let input:CurrentTaskMethodUseInput=serde_json::from_value(json!({"schema":"oev2.current-task-view-input/2",
            "task":{"schema":"oev2.current-task-view-input/1","current":reference(&root,&root.path().join(&project_name)),"previous":null,"work":reference(&root,&root.path().join(&work_name)),
                "sources":[reference(&root,&root.path().join(format!("{history}/163-intake.json"))),reference(&root,&root.path().join("var/method-use-originals/case253.json"))]},
            "methods":[{"object":{"object_id":"urn:oev2:169:fixed-left-service","revision":"V262","condition_scope":scope,"source_sha256":"8db49c94213a7b1bab6930e798337700a129f5dbb66ba991273f01f6e18c3d1b"},
                "basis_sha256":sha256(body),"fact_property":"change_dependency_method","source_id":"163-causal-intake","intake":reference(&root,&root.path().join(format!("{history}/163-intake.json"))),
                "archive":{"path":oracle["archive"]["path"],"sha256":oracle["archive"]["sha256"]},"entry_ids":["r87b0","r118b0"]}]})).unwrap();
        let view=derive_with_methods(&root,&input).unwrap();verify_method_view(&root,&view).unwrap();
        assert_eq!(view.freshness,"matched");assert_original_checks(&root,&input,&view);
        assert_eq!(view.task_view.decisions.len(),decisions);assert_eq!(work["acceptance"].as_array().unwrap().len(),acceptance);
        assert_eq!(&view.task_view.requirements[1..],serde_json::from_value::<Vec<String>>(work["acceptance"].clone()).unwrap());
        assert_eq!(view.methods[0].intake_scope.as_ref().unwrap()["status"],"partial");
        assert_eq!(view.methods[0].intake_scope.as_ref().unwrap()["entry_count"],1071);
        assert_eq!(view.methods[0].intake_scope.as_ref().unwrap()["issues"],oracle["original_issues"]);
        assert_eq!(view.methods[0].entries.len(),2);
        for (entry,expected) in view.methods[0].entries.iter().zip(oracle["selected_entries"].as_array().unwrap()) {
            assert_eq!(entry.status,"matched");assert_eq!(entry.intake_entry.as_ref().unwrap(),&expected["original_entry"]);
            assert_eq!(entry.text_sha256.as_deref(),expected["text_sha256"].as_str());
        }
        let downstream:BTreeSet<_>=view.methods[0].impact["decisions"].as_array().unwrap().iter().map(|d|d["decision_id"].as_str().unwrap()).collect();
        for id in oracle["expected_nine_decision_ids"].as_array().unwrap(){assert!(downstream.contains(id.as_str().unwrap()));}
        assert!(view.task_view.checks.iter().any(|c|c.check_id=="engineering:final-crosshair" && c.conclusion=="unknown"));
        assert_eq!(oracle["req14_original"]["target"]["value"],0.03);assert_eq!(oracle["req14_original"]["target"]["verified_machine_capability"],false);
        let mut missing=input.clone();missing.methods[0].archive=None;
        let missing=derive_with_methods(&root,&missing).unwrap();assert_eq!(missing.freshness,"unknown");
        assert_eq!(missing.task_view,view.task_view);assert!(verify_method_view(&root,&missing).is_err());
    }
}
