//! Synthetic byte-preservation and occurrence-isolation checks. No reference drawing or rule verdict is input.
use oe_contracts::*;
use oe_core::sha256;
use oe_drawing::correction::{record_sha256, rewrite, verify, write_new};

#[path = "support/correction_fixture.rs"]
mod input;
use input::fixture;

fn issue(id: &str, confidence: &str) -> MdDraftingIssue {
    MdDraftingIssue {
        id: id.into(),
        location: "synthetic source".into(),
        source_occurrences: vec![],
        observed: vec!["authored synthetic condition".into()],
        rule: "synthetic_execution_fixture".into(),
        clause: "fixture only; no engineering rule asserted".into(),
        concept_ids: vec!["DC-G9-023".into()],
        confidence: confidence.into(),
        alternatives: vec![],
        proposed_correction: "test the declared byte operation".into(),
    }
}
fn op(bytes: &[u8], id: &str, kind: &str, path: &[&str]) -> MdDrawingCorrectionOperation {
    let occurrence: Vec<_> = path.iter().map(|s| (*s).into()).collect();
    let expected_record_sha256 =
        (!path.is_empty()).then(|| record_sha256(bytes, &occurrence).unwrap());
    MdDrawingCorrectionOperation {
        id: id.into(),
        issue_id: id.into(),
        kind: kind.into(),
        occurrence,
        expected_record_sha256,
        text: None,
        style: vec![],
        translation: vec![],
    }
}
fn plan(bytes: &[u8], operations: Vec<MdDrawingCorrectionOperation>) -> MdDrawingCorrectionPlan {
    MdDrawingCorrectionPlan {
        source_sha256: sha256(bytes),
        issues: operations.iter().map(|o| issue(&o.id, "certain")).collect(),
        operations,
    }
}
fn text_op(bytes: &[u8]) -> MdDrawingCorrectionOperation {
    let mut o = op(bytes, "label", "add_text", &[]);
    o.text = Some(MdCorrectionText {
        value: "R".into(),
        position: vec![32., 17.],
        height: 3.,
        rotation: 90.,
        layer: "0".into(),
    });
    o
}
fn root_text(s: &[u8], handle: &str) -> String {
    let text = String::from_utf8(s.to_vec()).unwrap().replace("\r\n", "\n");
    let lines: Vec<_> = text.lines().collect();
    let mut begin = 0;
    let mut found = false;
    for (i, p) in lines.chunks_exact(2).enumerate() {
        if p[0].trim() == "0" {
            if found {
                return lines[begin..i * 2].join("\n") + "\n";
            }
            begin = i * 2;
        }
        if p[0].trim() == "5" && p[1] == handle {
            found = true;
        }
    }
    assert!(found);
    lines[begin..].join("\n") + "\n"
}

#[test]
fn correction_applies_four_operations_preserves_unknown_tags_and_rechecks_bytes() {
    let b = fixture();
    let hash = sha256(&b);
    let mut style = op(&b, "style", "set_style", &["40"]);
    style.style = vec![MdDxfStyleEdit {
        code: 62,
        value: "3".into(),
    }];
    let mut movement = op(&b, "move", "move", &["41"]);
    movement.translation = vec![6., -2.];
    let deletion = op(&b, "delete", "delete", &["31"]);
    let mut p = plan(&b, vec![text_op(&b), style, movement, deletion]);
    p.issues.push(issue("ambiguous", "needs_expert"));
    let (out, r) = rewrite(&b, &p).unwrap();
    assert_eq!(sha256(&b), hash);
    assert_eq!(r.applied.len(), 4);
    assert_eq!(r.remaining_issue_ids, ["ambiguous"]);
    assert_eq!(r.new_handles, ["F0"]);
    assert!(root_text(&out, "40").contains("  3.000 \n"));
    assert!(root_text(&out, "40").contains("Keep this source spacing."));
    assert!(root_text(&out, "40").contains("62\n3"));
    assert!(root_text(&out, "41").contains("10\n13\n20\n12"));
    assert_eq!(root_text(&b, "30"), root_text(&out, "30"));
    assert!(!String::from_utf8_lossy(&out).contains("\r\n5\r\n31\r\n"));
    let g = oe_drawing::engineering_expression::discover(&out).unwrap();
    assert!(
        g.source
            .entities
            .iter()
            .any(|e| e.handle == "F0" && e.kind == "TEXT")
    );
    verify(&b, &out, &p, &r).unwrap();
    let mut bad = out.clone();
    let at = bad.windows(4).position(|w| w == b"Keep").unwrap();
    bad[at] = b'X';
    assert!(verify(&b, &bad, &p, &r).is_err());
}
#[test]
fn correction_clones_nested_shared_blocks_for_only_one_occurrence() {
    let b = fixture();
    let mut edit = op(&b, "nested", "set_style", &["30", "21", "11"]);
    edit.style = vec![MdDxfStyleEdit {
        code: 62,
        value: "4".into(),
    }];
    let p = plan(&b, vec![edit]);
    let (out, r) = rewrite(&b, &p).unwrap();
    assert_eq!(root_text(&b, "31"), root_text(&out, "31"));
    for h in ["3", "4", "10", "11", "12", "13", "20", "21", "22"] {
        assert_eq!(
            root_text(&b, h),
            root_text(&out, h),
            "original definition {h}"
        );
    }
    assert!(root_text(&out, "30").contains("2\nOE_CORR_"));
    let h = &r.applied[0].result_handles[0];
    assert!(root_text(&out, h).contains("62\n4"));
    assert_eq!(r.new_handles.len(), 9);
    verify(&b, &out, &p, &r).unwrap();
    let g = oe_drawing::engineering_expression::discover(&out).unwrap();
    assert_eq!(g.source.entities.len(), 4);
}
#[test]
fn correction_refuses_stale_uncertain_duplicate_and_unsafe_plans() {
    let b = fixture();
    let p = plan(&b, vec![text_op(&b)]);
    let mut stale = p.clone();
    stale.source_sha256 = "0".repeat(64);
    assert!(rewrite(&b, &stale).is_err());
    let mut uncertain = p.clone();
    uncertain.issues[0].confidence = "needs_expert".into();
    assert!(rewrite(&b, &uncertain).is_err());
    let mut duplicate = p.clone();
    duplicate.operations.push(duplicate.operations[0].clone());
    assert!(rewrite(&b, &duplicate).is_err());
    let mut style = op(&b, "unsafe", "set_style", &["40"]);
    style.style = vec![MdDxfStyleEdit {
        code: 5,
        value: "A0".into(),
    }];
    assert!(rewrite(&b, &plan(&b, vec![style])).is_err());
    let mut movement = op(&b, "badmove", "move", &["40"]);
    movement.translation = vec![f64::NAN, 0.];
    assert!(rewrite(&b, &plan(&b, vec![movement])).is_err());
    let mut bad = b.clone();
    bad.extend_from_slice(b"0\nLINE\n5\n40\n");
    assert!(rewrite(&bad, &plan(&bad, vec![text_op(&bad)])).is_err());
}
#[test]
fn correction_file_is_new_and_source_survives_publication_failure() {
    let dir = std::env::temp_dir().join(format!("oe-correction-test-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let src = dir.join("source.dxf");
    let dst = dir.join("corrected.dxf");
    let b = fixture();
    std::fs::write(&src, &b).unwrap();
    let p = plan(&b, vec![text_op(&b)]);
    let r = write_new(&src, &dst, &p).unwrap();
    assert_eq!(std::fs::read(&src).unwrap(), b);
    assert_eq!(sha256(std::fs::read(&dst).unwrap()), r.corrected_sha256);
    let saved = std::fs::read(&dst).unwrap();
    assert!(write_new(&src, &dst, &p).is_err());
    assert_eq!(std::fs::read(&dst).unwrap(), saved);
    assert!(write_new(&src, &src, &p).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn correction_audit_rejects_consistently_rehashed_changes_outside_the_plan() {
    let b = fixture();
    let mut edit = op(&b, "style", "set_style", &["40"]);
    edit.style = vec![MdDxfStyleEdit {
        code: 62,
        value: "3".into(),
    }];
    let p = plan(&b, vec![edit]);
    let (out, mut r) = rewrite(&b, &p).unwrap();
    let altered = String::from_utf8(out)
        .unwrap()
        .replace("  3.000 ", "  8.000 ")
        .into_bytes();
    r.corrected_sha256 = sha256(&altered);
    let hx = |s: &str| s.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    for c in &mut r.changes {
        c.replacement_hex = c.replacement_hex.replace(&hx("  3.000 "), &hx("  8.000 "));
    }
    assert!(
        verify(&b, &altered, &p, &r)
            .unwrap_err()
            .to_string()
            .contains("unplanned_target_field")
    );
    let p = plan(&b, vec![op(&b, "delete", "delete", &["31"])]);
    let (_, mut r) = rewrite(&b, &p).unwrap();
    r.changes.clear();
    r.corrected_sha256 = sha256(&b);
    assert!(verify(&b, &b, &p, &r).is_err());
}

#[test]
fn correction_clone_audit_preserves_unselected_sibling_and_rejects_extra_geometry() {
    let b = fixture();
    let mut edit = op(&b, "nested", "set_style", &["30", "21", "11"]);
    edit.style = vec![MdDxfStyleEdit {
        code: 62,
        value: "4".into(),
    }];
    let p = plan(&b, vec![edit]);
    let (out, mut r) = rewrite(&b, &p).unwrap();
    let text = String::from_utf8(out).unwrap();
    let at = text.rfind("40\r\n2\r\n").unwrap();
    let mut altered = text.into_bytes();
    altered[at + 4] = b'6';
    r.corrected_sha256 = sha256(&altered);
    let hx = |s: &str| s.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    for c in &mut r.changes {
        c.replacement_hex = c
            .replacement_hex
            .replace(&hx("40\r\n2\r\n"), &hx("40\r\n6\r\n"));
    }
    assert!(verify(&b, &altered, &p, &r).is_err());
}

#[test]
fn correction_groups_two_edits_in_one_shared_occurrence_without_touching_its_alias() {
    let b = fixture();
    let mut style = op(&b, "style", "set_style", &["30", "21", "11"]);
    style.style = vec![MdDxfStyleEdit {
        code: 62,
        value: "5".into(),
    }];
    let mut movement = op(&b, "movement", "move", &["30", "21", "12"]);
    movement.translation = vec![4., 6.];
    let p = plan(&b, vec![style, movement]);
    let (out, r) = rewrite(&b, &p).unwrap();
    assert_eq!(r.applied.len(), 2);
    assert_eq!(r.new_handles.len(), 9);
    assert_eq!(root_text(&b, "31"), root_text(&out, "31"));
    let moved = r
        .applied
        .iter()
        .find(|a| a.operation_id == "movement")
        .unwrap();
    assert!(root_text(&out, &moved.result_handles[0]).contains("10\n15\n20\n13"));
    verify(&b, &out, &p, &r).unwrap();
}

#[test]
fn correction_audit_detects_unplanned_whitespace_even_when_all_digests_match() {
    let b = fixture();
    let mut edit = op(&b, "style", "set_style", &["40"]);
    edit.style = vec![MdDxfStyleEdit {
        code: 62,
        value: "3".into(),
    }];
    let p = plan(&b, vec![edit]);
    let (out, mut r) = rewrite(&b, &p).unwrap();
    let altered = String::from_utf8(out)
        .unwrap()
        .replace("  3.000 ", " 3.000  ")
        .into_bytes();
    r.corrected_sha256 = sha256(&altered);
    let hx = |s: &str| s.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    for c in &mut r.changes {
        c.replacement_hex = c.replacement_hex.replace(&hx("  3.000 "), &hx(" 3.000  "));
    }
    assert!(verify(&b, &altered, &p, &r).is_err());
}
