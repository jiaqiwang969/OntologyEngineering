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
    for r in rules
        .iter()
        .filter(|r| r.id != drafting_issues::CUTTING_RULE)
    {
        assert_eq!(r.confidence, "needs_expert");
        assert_eq!(r.proposed_correction, "请专家确认");
    }
    let (source, graph) = fixture();
    let mut r = report(&source, &graph);
    let mut duplicate = r.findings[0].clone();
    duplicate.focus = "source:another-expression".into();
    r.findings.push(duplicate);
    let issues = drafting_issues::collect(&r, &graph, &source).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].details["findings"].as_array().unwrap().len(), 4);
}
