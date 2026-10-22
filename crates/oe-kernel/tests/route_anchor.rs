//! Independent, frozen expectations for root-goal route anchoring.
//! Synthetic cases are public; the private drift replay reads originals only
//! when OE_ROUTE_ANCHOR_REPLAY points at them (ignored by default).
use oe_contracts::RouteAnchor;
use oe_kernel::project::{Project, route};
use serde_json::{Value, json};

fn project() -> Project {
    serde_json::from_value(json!({
        "schema": "oev2.project/1",
        "project_id": "pump-study",
        "revision": "r1",
        "purpose": "可仿真的泵完整结构",
        "objects": {
            "pump": "rev-a",
            "pump-impeller": "rev-a",
            "pump-flow-model": "m1",
            "other-pump": "catalogue",
            "sensor-paper": "2019"
        },
        "conditions": {}, "evidence": {}, "decisions": [], "dependencies": {}
    }))
    .unwrap()
}

fn stage(
    id: &str,
    object: &str,
    relation: &str,
    target: &str,
    advances: &[&str],
    outcome: &str,
) -> Value {
    json!({"id": id, "object_id": object, "relation": relation, "relation_target": target,
           "advances": advances, "outcome": outcome})
}

fn base() -> Value {
    json!({
        "schema": "oev2.route-anchor/1",
        "project_id": "pump-study",
        "root_object": "pump",
        "deliverable": "可直接用于仿真的泵完整结构，功能清单每项有结构、尺寸来源与状态",
        "requests": [{"id": "U1", "kind": "root", "text": "把这台泵的结构全部解析出来", "source": "chat 1", "affects": []}],
        "functions": [
            {"id": "F1", "statement": "吸入并加压流体", "structures": ["叶轮", "蜗壳"]},
            {"id": "F2", "statement": "密封", "structures": []}
        ],
        "acceptance": [
            {"id": "A1", "statement": "流道几何完整", "functions": ["F1"], "status": "partial"},
            {"id": "A2", "statement": "密封结构有来源", "functions": ["F2"], "status": "open"}
        ],
        "stages": [
            stage("S1", "pump", "root", "pump", &["A1"], "advanced"),
            stage("S2", "pump-impeller", "component", "pump", &["A1"], "advanced"),
            stage("S3", "pump-flow-model", "model", "pump-impeller", &["A1"], "advanced"),
            {"id": "S4", "object_id": "other-pump", "relation": "reference_case", "relation_target": "pump",
             "advances": ["A2"], "outcome": "advanced",
             "return_to": {"acceptance": "A2", "condition": "取得同类机械密封的安装形式后回到本泵密封腔核对"}},
            stage("S5", "pump", "root", "pump", &["A2"], "advanced")
        ],
        "reviews": [{"id": "R1", "after_stage": "S5", "source": "route review 1"}],
        "policy": {"off_root_stage_limit": 2, "no_progress_stage_limit": 2, "review_interval": 5}
    })
}

fn audit(anchor: Value) -> oe_contracts::RouteAuditReport {
    let anchor: RouteAnchor = serde_json::from_value(anchor).unwrap();
    route::audit(&project(), &anchor, &"0".repeat(64), &"1".repeat(64)).unwrap()
}
fn codes(r: &oe_contracts::RouteAuditReport) -> Vec<String> {
    r.findings.iter().map(|f| f.code.clone()).collect()
}
fn with_sensor_anchored(mut a: Value) -> Value {
    // Mention the remaining project object so unanchored-object findings do not mask the case.
    a["stages"].as_array_mut().unwrap().push(json!({
        "id": "S6", "object_id": "sensor-paper", "relation": "reference_case", "relation_target": "pump",
        "advances": ["A1"], "outcome": "advanced",
        "return_to": {"acceptance": "A1", "condition": "确认测压口位置后回到流道"}}));
    a["stages"].as_array_mut().unwrap().push(stage(
        "S7",
        "pump",
        "root",
        "pump",
        &["A1"],
        "advanced",
    ));
    a["reviews"] = json!([{"id": "R1", "after_stage": "S7", "source": "route review 1"}]);
    a
}

#[test]
fn anchored_route_passes() {
    let r = audit(with_sensor_anchored(base()));
    assert_eq!(r.status, "anchored", "{:?}", codes(&r));
    assert_eq!(r.open_acceptance, vec!["A1", "A2"]);
    assert_eq!(r.schema, "oev2.route-audit/1");
}

#[test]
fn unanchored_project_object_is_drift() {
    let r = audit(base());
    assert_eq!(r.status, "drift");
    assert_eq!(r.unanchored_objects, vec!["sensor-paper"]);
}

#[test]
fn two_consecutive_external_stages_are_drift_unless_approved() {
    let mut a = with_sensor_anchored(base());
    let s = a["stages"].as_array_mut().unwrap();
    s.insert(4, json!({"id": "S4b", "object_id": "sensor-paper", "relation": "external_case", "relation_target": "pump",
        "advances": ["A2"], "outcome": "advanced",
        "return_to": {"acceptance": "A2", "condition": "回到密封"}}));
    // S6 also references sensor-paper; keep relation consistent.
    s[6]["relation"] = json!("external_case");
    a["reviews"] = json!([{"id": "R1", "after_stage": "S3", "source": "route review 1"}]);
    let r = audit(a.clone());
    assert_eq!(r.status, "drift");
    assert!(codes(&r).contains(&"off_root_streak".into()));
    assert_eq!(
        r.findings
            .iter()
            .find(|f| f.code == "off_root_streak")
            .unwrap()
            .stage
            .as_deref(),
        Some("S4b")
    );

    a["requests"].as_array_mut().unwrap().push(json!({"id": "U2", "kind": "approval", "text": "可以，先看看传感器论文", "source": "chat 2", "after_stage": "S4", "affects": []}));
    a["stages"][4]["approval"] = json!("U2");
    let r = audit(a);
    assert!(
        !codes(&r).contains(&"off_root_streak".into()),
        "{:?}",
        codes(&r)
    );
}

#[test]
fn two_stages_without_progress_are_drift() {
    let mut a = with_sensor_anchored(base());
    a["stages"][1]["outcome"] = json!("no_progress");
    a["stages"][2]["advances"] = json!([]);
    a["stages"][2]["outcome"] = json!("no_progress");
    a["reviews"] = json!([{"id": "R1", "after_stage": "S2", "source": "route review 1"}]);
    let r = audit(a);
    assert_eq!(r.status, "drift");
    assert_eq!(
        r.findings
            .iter()
            .find(|f| f.code == "no_progress_streak")
            .unwrap()
            .stage
            .as_deref(),
        Some("S3")
    );
}

#[test]
fn route_review_after_a_streak_clears_it_and_leaving_again_reopens_drift() {
    let mut a = with_sensor_anchored(base());
    let s = a["stages"].as_array_mut().unwrap();
    s.insert(4, json!({"id": "S4b", "object_id": "sensor-paper", "relation": "external_case", "relation_target": "pump",
        "advances": ["A2"], "outcome": "advanced", "return_to": {"acceptance": "A2", "condition": "回到密封"}}));
    s[6]["relation"] = json!("external_case");
    // Review before the streak: drift.
    a["reviews"] = json!([{"id": "R1", "after_stage": "S3", "source": "r1"}]);
    assert!(codes(&audit(a.clone())).contains(&"off_root_streak".into()));
    // Review after the streak (route returned to root at S5): cleared.
    a["reviews"] = json!([{"id": "R1", "after_stage": "S3", "source": "r1"}, {"id": "R2", "after_stage": "S5", "source": "r2"}]);
    let r = audit(a.clone());
    assert_eq!(r.status, "anchored", "{:?}", codes(&r));
    // Leaving the root again for two stages after the review: drift again.
    let s = a["stages"].as_array_mut().unwrap();
    s.push(json!({"id": "S8", "object_id": "other-pump", "relation": "reference_case", "relation_target": "pump",
        "advances": ["A2"], "outcome": "advanced", "return_to": {"acceptance": "A2", "condition": "回到密封"}}));
    s.push(json!({"id": "S9", "object_id": "sensor-paper", "relation": "external_case", "relation_target": "pump",
        "advances": ["A1"], "outcome": "advanced", "return_to": {"acceptance": "A1", "condition": "回到流道"}}));
    let r = audit(a);
    assert_eq!(
        r.findings
            .iter()
            .find(|f| f.code == "off_root_streak")
            .unwrap()
            .stage
            .as_deref(),
        Some("S9")
    );
}

#[test]
fn overdue_route_review_is_drift() {
    let mut a = with_sensor_anchored(base());
    a["reviews"] = json!([{"id": "R1", "after_stage": "S1", "source": "r"}]);
    let r = audit(a);
    assert!(codes(&r).contains(&"route_review_overdue".into()));
    assert_eq!(r.status, "drift");
}

#[test]
fn unaddressed_correction_is_drift_and_later_stage_clears_it() {
    let mut a = with_sensor_anchored(base());
    a["requests"].as_array_mut().unwrap().push(
        json!({"id": "U3", "kind": "correction", "text": "叶轮应该是双吸的吧？两侧都进液？",
        "source": "chat 3", "after_stage": "S7", "affects": ["A1"]}),
    );
    let r = audit(a.clone());
    assert!(codes(&r).contains(&"correction_not_addressed".into()));
    a["stages"].as_array_mut().unwrap().push(stage(
        "S8",
        "pump-impeller",
        "component",
        "pump",
        &["A1"],
        "pending",
    ));
    let r = audit(a);
    assert_eq!(r.status, "anchored", "{:?}", codes(&r));
}

#[test]
fn invalid_declarations_are_rejected() {
    let cases: Vec<(&str, Box<dyn Fn(&mut Value)>)> = vec![
        (
            "missing_return_condition",
            Box::new(|a: &mut Value| {
                a["stages"][3].as_object_mut().unwrap().remove("return_to");
            }),
        ),
        (
            "stage_object_unknown",
            Box::new(|a: &mut Value| {
                a["stages"][1]["object_id"] = json!("ghost");
            }),
        ),
        (
            "advance_acceptance_unknown",
            Box::new(|a: &mut Value| {
                a["stages"][0]["advances"] = json!(["A9"]);
            }),
        ),
        (
            "relation_chain_not_rooted",
            Box::new(|a: &mut Value| {
                a["stages"][2]["relation_target"] = json!("other-pump");
            }),
        ),
        (
            "policy_looser_than_method",
            Box::new(|a: &mut Value| {
                a["policy"]["off_root_stage_limit"] = json!(10);
            }),
        ),
        (
            "policy_looser_than_method",
            Box::new(|a: &mut Value| {
                a["policy"]["review_interval"] = json!(0);
            }),
        ),
        (
            "root_request_missing",
            Box::new(|a: &mut Value| {
                a["requests"][0]["kind"] = json!("correction");
            }),
        ),
        (
            "function_list_missing",
            Box::new(|a: &mut Value| {
                a["functions"] = json!([]);
                a["acceptance"][0]["functions"] = json!([]);
                a["acceptance"][1]["functions"] = json!([]);
            }),
        ),
        (
            "inconsistent_object_relation",
            Box::new(|a: &mut Value| {
                a["stages"][4] = stage("S5", "pump-impeller", "model", "pump", &["A2"], "advanced");
            }),
        ),
        (
            "project_mismatch",
            Box::new(|a: &mut Value| {
                a["project_id"] = json!("other");
            }),
        ),
        (
            "advance_without_acceptance",
            Box::new(|a: &mut Value| {
                a["stages"][0]["advances"] = json!([]);
            }),
        ),
        (
            "approval_unknown",
            Box::new(|a: &mut Value| {
                a["stages"][3]["approval"] = json!("U1");
            }),
        ),
        (
            "anchor_contract_invalid",
            Box::new(|a: &mut Value| {
                a["stages"][1]["relation"] = json!("inspiration");
            }),
        ),
        (
            "anchor_contract_invalid",
            Box::new(|a: &mut Value| {
                a["acceptance"][0]["status"] = json!("done");
            }),
        ),
    ];
    for (code, mutate) in cases {
        let mut a = with_sensor_anchored(base());
        mutate(&mut a);
        let r = audit(a);
        assert_eq!(r.status, "invalid", "{code}: {:?}", codes(&r));
        assert!(
            codes(&r).contains(&code.to_string()),
            "{code}: {:?}",
            codes(&r)
        );
    }
}

#[test]
fn component_cycle_is_not_rooted() {
    let mut a = with_sensor_anchored(base());
    a["stages"][1] = stage(
        "S2",
        "pump-impeller",
        "component",
        "pump-flow-model",
        &["A1"],
        "advanced",
    );
    let r = audit(a);
    assert_eq!(r.status, "invalid");
    assert!(codes(&r).contains(&"relation_chain_not_rooted".into()));
}

#[test]
fn unknown_anchor_fields_are_refused() {
    let mut a = base();
    a["extra"] = json!(true);
    assert!(serde_json::from_value::<RouteAnchor>(a).is_err());
}

/// Private development replay of a real drift (E7). Requires
/// OE_ROUTE_ANCHOR_REPLAY=<dir> with project.json, project-expected.json,
/// anchor-actual.json, anchor-actual-as-recorded.json and anchor-expected.json;
/// run with --include-ignored.
#[test]
#[ignore]
fn private_replay_actual_route_is_drift_and_anchored_route_passes() {
    let dir = std::path::PathBuf::from(
        std::env::var("OE_ROUTE_ANCHOR_REPLAY").expect("OE_ROUTE_ANCHOR_REPLAY"),
    );
    let read = |name: &str| std::fs::read(dir.join(name)).unwrap();
    let run = |project_name: &str, name: &str| {
        let project_bytes = read(project_name);
        let project: Project = serde_json::from_slice(&project_bytes).unwrap();
        let bytes = read(name);
        let anchor: RouteAnchor = serde_json::from_slice(&bytes).unwrap();
        route::audit(
            &project,
            &anchor,
            &oe_core::sha256(&project_bytes),
            &oe_core::sha256(&bytes),
        )
        .unwrap()
    };
    let actual = run("project.json", "anchor-actual.json");
    // Final state: the agent's own route review after S31 settled the past streaks,
    // but Project objects created on the detours were never anchored.
    assert_eq!(actual.status, "drift", "{:?}", codes(&actual));
    assert!(codes(&actual).contains(&"unanchored_project_object".to_string()));
    // Gate before each stage: the real route is stopped at S03.
    let project_bytes = read("project.json");
    let project: Project = serde_json::from_slice(&project_bytes).unwrap();
    let full: Value = serde_json::from_slice(&read("anchor-actual.json")).unwrap();
    let prefix = |k: usize| {
        let mut a = full.clone();
        let stages: Vec<Value> = full["stages"].as_array().unwrap()[..k].to_vec();
        let ids: Vec<String> = stages
            .iter()
            .map(|s| s["id"].as_str().unwrap().to_string())
            .collect();
        let mut seen: Vec<String> = stages
            .iter()
            .map(|s| s["object_id"].as_str().unwrap().to_string())
            .collect();
        seen.push(full["root_object"].as_str().unwrap().to_string());
        a["stages"] = Value::Array(stages);
        a["reviews"] = Value::Array(
            full["reviews"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| ids.iter().any(|i| i == r["after_stage"].as_str().unwrap()))
                .cloned()
                .collect(),
        );
        a["requests"] = Value::Array(
            full["requests"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|u| {
                    u.get("after_stage")
                        .and_then(Value::as_str)
                        .is_none_or(|s| ids.iter().any(|i| i == s))
                })
                .cloned()
                .collect(),
        );
        let mut p = project.clone();
        p.objects.retain(|k, _| seen.contains(k));
        p.decisions.retain(|d| seen.contains(&d.object_id));
        let anchor: RouteAnchor = serde_json::from_value(a).unwrap();
        route::audit(&p, &anchor, &"0".repeat(64), &"1".repeat(64)).unwrap()
    };
    assert_eq!(prefix(2).status, "anchored", "{:?}", codes(&prefix(2)));
    let gate = prefix(3);
    assert_eq!(gate.status, "drift");
    assert_eq!(
        gate.findings
            .iter()
            .find(|f| f.code == "off_root_streak")
            .unwrap()
            .stage
            .as_deref(),
        Some("S03")
    );
    eprintln!("final findings {:?}", codes(&actual));
    let recorded = run("project.json", "anchor-actual-as-recorded.json");
    assert_eq!(recorded.status, "invalid", "{:?}", codes(&recorded));
    assert!(codes(&recorded).contains(&"missing_return_condition".to_string()));
    let expected = run("project-expected.json", "anchor-expected.json");
    assert_eq!(expected.status, "anchored", "{:?}", expected.findings);
}

// ---- Route anchor /2: acceptance closure against Project decisions and registered evidence ----

fn project2(d1: &str, d1_basis: &[&str]) -> Project {
    let mut p = serde_json::to_value(project()).unwrap();
    p["evidence"] = json!({"src": "a".repeat(64), "tool-result:R1": "b".repeat(64)});
    let dec = |id: &str, c: &str, basis: &[&str]| {
        json!({"id": id, "object_id": "pump", "object_revision": "rev-a",
        "condition_scope": "cond", "source_sha256": "a".repeat(64), "conclusion": c, "adopted": false, "basis": basis})
    };
    p["conditions"] = json!({"cond": "x"});
    p["decisions"] = json!([dec("D1", d1, d1_basis), dec("D2", "unknown", &[])]);
    serde_json::from_value(p).unwrap()
}

fn anchor2(a1_status: &str, a1_decisions: &[&str], a1_evidence: &[&str]) -> Value {
    let mut a = with_sensor_anchored(base());
    a["schema"] = json!("oev2.route-anchor/2");
    a["acceptance"] = json!([
        {"id": "A1", "statement": "流道几何完整", "functions": ["F1"], "status": a1_status, "decisions": a1_decisions, "evidence": a1_evidence},
        {"id": "A2", "statement": "密封结构有来源", "functions": ["F2"], "status": "open", "decisions": ["D2"], "evidence": []}
    ]);
    a
}

fn audit2(p: &Project, a: Value) -> oe_contracts::RouteAuditReport2 {
    let anchor: oe_contracts::RouteAnchor2 = serde_json::from_value(a).unwrap();
    route::audit2(p, &anchor, &"0".repeat(64), &"1".repeat(64)).unwrap()
}

#[test]
fn closed_acceptance_with_supported_registered_decisions_is_deliverable() {
    let r = audit2(
        &project2("supported", &["tool-result:R1"]),
        anchor2("closed", &["D1"], &["tool-result:R1"]),
    );
    assert_eq!(r.status, "anchored", "{:?}", codes2(&r));
    assert_eq!(r.delivery_verified, vec!["A1"]);
    assert_eq!(r.delivery_open, vec!["A2"]);
    assert!(r.delivery_inconsistent.is_empty());
    assert_eq!(r.schema, "oev2.route-audit/2");
}

#[test]
fn closed_acceptance_without_support_is_invalid_and_inconsistent() {
    let cases: Vec<(Project, Value, &str)> = vec![
        (
            project2("unknown", &[]),
            anchor2("closed", &["D1"], &[]),
            "decision_not_supported:D1",
        ),
        (
            project2("supported", &["tool-result:R9"]),
            anchor2("closed", &["D1"], &[]),
            "decision_basis_not_registered:D1",
        ),
        (
            project2("supported", &["tool-result:R1"]),
            anchor2("closed", &["D1"], &["tool-result:R9"]),
            "evidence_unregistered:tool-result:R9",
        ),
        (
            project2("supported", &["tool-result:R1"]),
            anchor2("closed", &[], &[]),
            "closed_without_decisions",
        ),
        (
            project2("supported", &["tool-result:R1"]),
            anchor2("closed", &["DX"], &[]),
            "decision_unknown:DX",
        ),
    ];
    for (p, a, code) in cases {
        let r = audit2(&p, a);
        assert_eq!(r.status, "invalid", "{code}");
        assert_eq!(r.delivery_inconsistent, vec!["A1"], "{code}");
        assert!(
            r.findings
                .iter()
                .any(|f| f.code == "acceptance_closure" && f.detail.ends_with(code)),
            "{code}: {:?}",
            r.findings
        );
    }
}

#[test]
fn open_acceptance_with_unknown_reference_is_invalid_but_not_delivered() {
    let r = audit2(&project2("unknown", &[]), anchor2("partial", &["DX"], &[]));
    assert_eq!(r.status, "invalid");
    assert!(r.delivery_verified.is_empty() && r.delivery_open == vec!["A1", "A2"]);
    assert!(r.findings.iter().any(|f| f.code == "acceptance_reference"));
}

fn codes2(r: &oe_contracts::RouteAuditReport2) -> Vec<String> {
    r.findings.iter().map(|f| f.code.clone()).collect()
}

#[test]
fn closed_acceptance_on_a_revised_object_is_stale_until_redecided() {
    let mut p = serde_json::to_value(project2("supported", &["tool-result:R1"])).unwrap();
    p["objects"]["pump"] = json!("rev-b");
    let p: Project = serde_json::from_value(p).unwrap();
    let r = audit2(&p, anchor2("closed", &["D1"], &["tool-result:R1"]));
    assert_eq!(r.status, "invalid");
    assert_eq!(r.delivery_inconsistent, vec!["A1"]);
    assert!(
        r.findings
            .iter()
            .any(|f| f.code == "acceptance_closure"
                && f.detail.ends_with("decision_object_stale:D1")),
        "{:?}",
        r.findings
    );
    let mut p = serde_json::to_value(project2("supported", &["tool-result:R1"])).unwrap();
    p["evidence"].as_object_mut().unwrap().remove("src");
    let p: Project = serde_json::from_value(p).unwrap();
    let r = audit2(&p, anchor2("closed", &["D1"], &["tool-result:R1"]));
    assert_eq!(r.delivery_inconsistent, vec!["A1"]);
    assert!(
        r.findings
            .iter()
            .any(|f| f.detail.ends_with("decision_source_missing:D1")),
        "{:?}",
        r.findings
    );
}

#[test]
fn memory_evidence_cannot_close_an_acceptance_item() {
    let mut p = serde_json::to_value(project2("supported", &["tool-result:R1"])).unwrap();
    p["evidence"]["memory-candidate:exp-1@r2"] = json!("c".repeat(64));
    let p: Project = serde_json::from_value(p).unwrap();
    let r = audit2(
        &p,
        anchor2(
            "closed",
            &["D1"],
            &["tool-result:R1", "memory-candidate:exp-1@r2"],
        ),
    );
    assert_eq!(r.status, "invalid");
    assert_eq!(r.delivery_inconsistent, vec!["A1"]);
    assert!(
        r.findings.iter().any(|f| f.code == "acceptance_closure"
            && f.detail
                .ends_with("memory_evidence_not_decisive:memory-candidate:exp-1@r2")),
        "{:?}",
        r.findings
    );
    // Listed against an open item it is only a reference.
    let r = audit2(&p, anchor2("open", &["D1"], &["memory-candidate:exp-1@r2"]));
    assert!(r.delivery_inconsistent.is_empty());
    assert!(
        !r.findings
            .iter()
            .any(|f| f.detail.contains("memory_evidence")),
        "{:?}",
        r.findings
    );
}
