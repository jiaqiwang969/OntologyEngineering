//! Authored synthetic source; no expert material, saved reference graph or actual drawing identifiers.
use oe_contracts::*;
use oe_core::sha256;
use oe_drawing::{correction, drafting_issues};
use oe_drawing_check::drafting;
use serde_json::{Value, json};

fn fixture() -> (Vec<u8>, Value) {
    let source=b"0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n5\n1\n70\n1\n0\nBLOCK_RECORD\n5\n2\n330\n1\n2\n*Model_Space\n0\nENDTAB\n0\nSTYLE\n5\n3\n2\nLetterStyle\n41\n0.85\n3\ntxt.shx\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n10\n330\n2\n8\n0\n10\n0\n20\n0\n11\n0\n21\n8\n0\nLINE\n5\n11\n330\n2\n8\n0\n10\n0\n20\n80\n11\n0\n21\n72\n0\nTEXT\n5\n12\n330\n2\n8\n0\n10\n-1\n20\n-6\n40\n2.7\n50\n0\n7\nLetterStyle\n41\n0.9\n51\n7\n71\n0\n1\nN\n0\nTEXT\n5\n13\n330\n2\n8\n0\n10\n30\n20\n50\n40\n2.7\n1\nN-N\n0\nENDSEC\n0\nEOF\n".to_vec();
    let members:Vec<_>=[("10","cutting_stroke","thick_continuous"),("11","cutting_stroke","thick_continuous"),("12","view_letter","text"),("13","view_title","text")].into_iter().enumerate().map(|(i,(h,role,lt))|json!({"id":format!("e:m{i}"),"expression_id":if h=="13"{"title"}else{"e"},"source_handle":h,"native_member_id":null,"member_role":role,"line_type":lt,"interval":[0.,1.],"evidence_ids":[]})).collect();
    (
        source,
        json!({"members":members,"expressions":[{"id":"e","kind":"view_callout","label":"N","qualifiers":["cutting_plane"]},{"id":"title","kind":"view_callout","label":"N-N","qualifiers":[]}],"structural_lines":[],"implied_spans":[],"views":[],"source":{"entities":[]}}),
    )
}
fn report(source: &[u8], graph: &Value) -> MdDraftingRuleReport {
    let k = drafting::knowledge().unwrap();
    let rules = drafting::rules(&k.shapes)
        .unwrap()
        .into_iter()
        .map(|r| MdDraftingRule {
            id: r.id,
            group: r.group,
            concept_ids: r.concept_ids,
            clause: r.clause,
            label: r.label,
            message: r.messages.join("；"),
            severity: r.severity,
            target_classes: r.target_classes,
            confidence: r.confidence,
            proposed_correction: r.proposed_correction,
        })
        .collect::<Vec<_>>();
    MdDraftingRuleReport {
        schema: drafting::SCHEMA.into(),
        status: "checked".into(),
        graph_sha256: sha256(serde_json::to_vec(graph).unwrap()),
        source_sha256: sha256(source),
        ontology_sha256: k.sha256,
        rule_count: rules.len() as u64,
        rules,
        layers: vec![],
        findings: drafting::evaluate_bytes(source, graph)
            .unwrap()
            .into_values()
            .flatten()
            .collect(),
        by_rule: Default::default(),
        scope: "synthetic rule and correction execution".into(),
    }
}
#[test]
fn repeated_issues_plan_drawing_and_receipt_are_byte_identical() {
    let (source, graph) = fixture();
    let mut r = report(&source, &graph);
    let first = drafting_issues::collect(&r, &graph, &source).unwrap();
    assert_eq!(
        first.len(),
        1,
        "{:?}",
        first
            .iter()
            .map(|i| (&i.rule, &i.location))
            .collect::<Vec<_>>()
    );
    assert_eq!(first[0].layers.len(), 3);
    assert_eq!(first[0].confidence, "certain");
    assert!(!first[0].rule_label.is_empty());
    let listing = drafting_issues::text(&first);
    assert!(listing.contains(&first[0].rule_label));
    assert!(listing.contains(&format!("（{}）", first[0].rule)));
    let plan = drafting_issues::plan(&source, first.clone()).unwrap();
    assert_eq!(plan.operations.len(), 1);
    let (drawing, receipt) = correction::rewrite(&source, &plan).unwrap();
    r.findings.reverse();
    r.rules.reverse();
    let second = drafting_issues::collect(&r, &graph, &source).unwrap();
    let plan2 = drafting_issues::plan(&source, second.clone()).unwrap();
    let (drawing2, receipt2) = correction::rewrite(&source, &plan2).unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(
        drafting_issues::text(&first),
        drafting_issues::text(&second)
    );
    assert_eq!(
        serde_json::to_vec(&plan).unwrap(),
        serde_json::to_vec(&plan2).unwrap()
    );
    assert_eq!(drawing, drawing2);
    assert_eq!(
        serde_json::to_vec(&receipt).unwrap(),
        serde_json::to_vec(&receipt2).unwrap()
    );
    let new = String::from_utf8(drawing).unwrap();
    let added = new.rsplit("0\nTEXT\n").next().unwrap();
    assert!(added.contains("7\nLetterStyle\n41\n0.9\n51\n7\n71\n0\n"));
    assert_eq!(receipt.source_sha256, sha256(&source));
    assert_eq!(receipt.applied.len(), 1);
}
#[test]
fn recognition_only_certain_findings_are_separate_and_never_corrected() {
    let (source, graph) = fixture();
    let mut r = report(&source, &graph);
    r.findings.retain(|f| f.layer != "source");
    let issues = drafting_issues::collect(&r, &graph, &source).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].classification, "recognition_deviation");
    let mut plan = drafting_issues::plan(&source, issues).unwrap();
    assert!(plan.operations.is_empty());
    // The byte writer also rejects a forged manual operation for an interpretation-only item.
    plan.operations = drafting_issues::plan(
        &source,
        drafting_issues::collect(&report(&source, &graph), &graph, &source).unwrap(),
    )
    .unwrap()
    .operations;
    assert!(correction::rewrite(&source, &plan).is_err());
}
#[test]
fn old_rule_metadata_defaults_and_source_occurrences_merge_without_expression_identity() {
    let rules = drafting::rules(&drafting::knowledge().unwrap().shapes).unwrap();
    // The merged rule set also contains explicitly annotated section and recognition rules.
    // Default metadata still applies to exactly the original thirty rules.
    let annotated = [
        (drafting_issues::CUTTING_RULE, "certain"),
        ("G4-059-unresolved-terminator-on-carrier", "likely"),
        ("G4-059-unresolved-dimension-continuation", "likely"),
        ("G2-028-boundary-centre-only", "likely"),
        ("G2-028-boundary-centre-only-unknown", "needs_expert"),
        ("G2-018-outline-in-cut-face", "likely"),
        ("G2-018-outline-in-cut-face-unknown", "needs_expert"),
        ("G2-019-section-hatch-crosses-outline", "likely"),
        ("V6-section-title-required", "likely"),
        ("V6-section-cutting-position-required", "likely"),
        ("V6-section-direction-required", "likely"),
        ("V6-section-letters-required", "likely"),
        ("V6-detail-marking", "certain"),
        ("V6-detail-single-numbered", "needs_expert"),
        ("V6-direction-view-marking", "likely"),
    ];
    let mut defaults = 0;
    for r in &rules {
        if let Some((_, confidence)) = annotated.iter().find(|(id, _)| *id == r.id) {
            assert_eq!(r.confidence, *confidence, "{}", r.id);
            assert!(!r.proposed_correction.is_empty(), "{}", r.id);
            assert_ne!(r.proposed_correction, "请专家确认", "{}", r.id);
        } else {
            assert_eq!(r.confidence, "needs_expert", "{}", r.id);
            assert_eq!(r.proposed_correction, "请专家确认", "{}", r.id);
            defaults += 1;
        }
    }
    assert_eq!(defaults, 30);
    assert_eq!(rules.len(), defaults + annotated.len());
    let (source, graph) = fixture();
    let mut r = report(&source, &graph);
    let mut duplicate = r.findings[0].clone();
    duplicate.focus = "source:another-expression".into();
    r.findings.push(duplicate);
    let issues = drafting_issues::collect(&r, &graph, &source).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].details["findings"].as_array().unwrap().len(), 4);
}

#[test]
fn same_rule_same_native_line_through_contour_and_borrowed_member_merges() {
    let (source, mut graph) = fixture();
    let mut r = report(&source, &graph);
    graph["members"].as_array_mut().unwrap().extend([
        json!({
            "id":"region-long-synthetic-member",
            "expression_id":"synthetic-expression",
            "source_handle":"REGION-SYNTHETIC",
            "native_member_id":null,
            "member_role":"section_region",
            "line_type":"continuous",
            "interval":[0.,1.],
            "evidence_ids":[]
        }),
        json!({
            "id":"contour-synthetic-member",
            "expression_id":"synthetic-expression",
            "source_handle":"LINE-SYNTHETIC",
            "native_member_id":null,
            "member_role":"visible_outline",
            "line_type":"continuous",
            "interval":[0.25,0.75],
            "evidence_ids":[]
        }),
        json!({
            "id":"dimension-borrowed-synthetic-member",
            "expression_id":"synthetic-expression",
            "source_handle":"LINE-SYNTHETIC",
            "native_member_id":null,
            "member_role":"dimension_line",
            "line_type":"continuous",
            "interval":[0.25,0.75],
            "evidence_ids":[]
        }),
    ]);
    let finding = |focus: &str| MdDraftingFinding {
        layer: "source".into(),
        rule_id: "G1-021-centre-line-beyond-outline".into(),
        focus: focus.into(),
        focus_classes: vec!["Line".into()],
        source_handles: vec!["LINE-SYNTHETIC".into()],
        expression_ids: vec![],
        value: None,
        message: "synthetic same-line finding".into(),
        severity: "warning".into(),
    };
    r.findings = vec![
        finding("source:region-long-synthetic-member:source:contour-synthetic-member"),
        finding("source:dimension-borrowed-synthetic-member"),
    ];

    let issues = drafting_issues::collect(&r, &graph, &source).unwrap();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(
        issues[0].source_occurrences,
        vec!["LINE-SYNTHETIC#[0.25,0.75]"]
    );
    assert_eq!(issues[0].details["findings"].as_array().unwrap().len(), 2);
}
