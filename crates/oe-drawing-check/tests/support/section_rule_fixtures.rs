//! Authored section examples. No production drawing or review material is read here.
use super::*;
pub(super) const RULE_IDS: [&str; 5] = [
    "G2-028-boundary-centre-only",
    "G2-028-boundary-centre-only-unknown",
    "G2-018-outline-in-cut-face",
    "G2-018-outline-in-cut-face-unknown",
    "G2-019-section-hatch-crosses-outline",
];
fn face(d: &mut Drawing, loops: &[Vec<[f64; 3]>], kind: &str, extent: &str, scale: f64) -> String {
    let mut body = format!("2\nANSI31\n70\n0\n52\n45\n91\n{}\n", loops.len());
    for ps in loops {
        body.push_str(&format!("92\n2\n72\n1\n73\n1\n93\n{}\n", ps.len()));
        for p in ps {
            body.push_str(&format!(
                "10\n{}\n20\n{}\n42\n{}\n",
                p[0] * scale,
                p[1] * scale,
                p[2]
            ));
        }
        body.push_str("97\n0\n");
    }
    body.push_str("75\n0\n");
    let h = d.entity("HATCH", &body);
    let e = d.expression("section_hatch", "");
    d.member_in(&e, &h, "hatch_fill", "thin_continuous");
    d.expressions.last_mut().unwrap()["evidence_ids"] = json!([
        format!("cross_section_type:{kind}"),
        format!("section_context:{extent}")
    ]);
    e
}
fn rectangle() -> Vec<[f64; 3]> {
    vec![[0., 0., 0.], [24., 0., 0.], [24., 16., 0.], [0., 16., 0.]]
}
fn drawing(kind: &str, extent: &str, scale: f64) -> (Drawing, String) {
    let mut d = Drawing::default();
    let t = d.text("QZ", [80. * scale, 70. * scale], 4. * scale, 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    let e = face(&mut d, &[rectangle()], kind, extent, scale);
    (d, e)
}
fn large_rectangle() -> Vec<[f64; 3]> {
    vec![
        [0., 0., 0.],
        [100., 0., 0.],
        [100., 100., 0.],
        [0., 100., 0.],
    ]
}
fn native_boundary_chain_drawing(chain: &[[[f64; 2]; 2]]) -> Drawing {
    let mut d = Drawing::default();
    let t = d.text("QZ", [140., 130.], 4., 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    face(&mut d, &[large_rectangle()], "removed", "unknown", 1.);
    for [a, b] in chain {
        outline(&mut d, *a, *b);
    }
    d
}
fn axis(d: &mut Drawing, a: [f64; 2], b: [f64; 2]) -> String {
    let h = d.line(a, b);
    d.member(&h, "center_line", "thin_dash_dot", "center_axis", "");
    h
}
fn origin(d: &mut Drawing, e: &str, class: &str) {
    let m = d.members.last_mut().unwrap();
    m["evidence_ids"] = json!([format!(
        "section_origin_region:{}",
        json!({"region":e,"class":class,"basis":"authored generic section"})
    )]);
}
fn new_findings(d: &Drawing) -> Vec<oe_contracts::MdDraftingFinding> {
    d.findings()
        .into_values()
        .flatten()
        .filter(|f| RULE_IDS.contains(&f.rule_id.as_str()))
        .collect()
}
pub(super) fn fixture(rule: &str, violating: bool) -> Drawing {
    if rule == "G2-019-section-hatch-crosses-outline" {
        let mut d = Drawing::default();
        let e = face(&mut d, &[rectangle()], "removed", "unknown", 1.);
        let hatch = d.line(
            if violating { [8., -3.] } else { [8., 0.] },
            if violating { [14., 3.] } else { [14., 6.] },
        );
        d.member_in(&e, &hatch, "hatch_fill", "thin_continuous");
        // The line is the lower boundary of the filled region.  A hatch line that
        // ends at it is conforming; one that continues through it is the deviation.
        outline(&mut d, [0., 0.], [24., 0.]);
        return d;
    }
    let unknown = rule.ends_with("-unknown");
    let (mut d, _) = drawing(if unknown { "unknown" } else { "removed" }, "unknown", 1.);
    if rule.starts_with("G2-028") {
        axis(&mut d, [-12., 0.], [36., 0.]);
        if !violating {
            d.views
                .push(json!({"id":"view:main","evidence":["section_type:half"]}));
        }
    } else {
        let y = if violating { 8. } else { 28. };
        outline(&mut d, [-12., y], [36., y]);
    }
    d
}
/// Exercise the real fact writer in a fresh private directory, as the three-layer reports do.
fn written_facts(d: &Drawing) -> BTreeMap<String, String> {
    use oe_core::{SkillRoot, sha256, write_new_bytes};
    use std::path::Path;
    let software = SkillRoot::new(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
    )
    .unwrap();
    let home = software
        .unique_private_dir(Path::new("var/tests"), "section-facts")
        .unwrap();
    let root = software.with_project_data_root(&home).unwrap();
    std::fs::create_dir(home.join("classified")).unwrap();
    let raw = d.dxf();
    let mut graph = d.graph();
    graph["source_sha256"] = json!(sha256(&raw));
    write_new_bytes(&home.join("source.dxf"), &raw).unwrap();
    write_new_bytes(
        &home.join("classified/graph.json"),
        &serde_json::to_vec(&graph).unwrap(),
    )
    .unwrap();
    let request = json!({"source":{"path":"source.dxf","sha256":sha256(&raw)}, "output_directory":"classified"});
    write_new_bytes(
        &home.join("request.json"),
        &serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    let report = drafting::check_rules(
        &root,
        &oe_contracts::MdDraftingRuleRequest {
            schema: drafting::REQUEST_SCHEMA.into(),
            discovery_request: "request.json".into(),
            output: "report.json".into(),
        },
    )
    .unwrap();
    report
        .layers
        .into_iter()
        .map(|l| {
            (
                l.layer,
                std::fs::read_to_string(home.join(l.facts_file)).unwrap(),
            )
        })
        .collect()
}
fn interval_values(ttl: &str, predicate: &str) -> Vec<f64> {
    ttl.lines()
        .filter_map(|l| {
            let (_, rest) = l.split_once(&format!("md:{predicate} "))?;
            rest.split('"').nth(1)?.parse().ok()
        })
        .collect()
}
#[test]
fn section_boundary_intervals_are_exact() {
    let d = fixture(RULE_IDS[1], true);
    assert_eq!(new_findings(&d).len(), 3);
    for ttl in written_facts(&d).into_values() {
        assert_eq!(interval_values(&ttl, "boundaryIntervalStart"), vec![0.25]);
        assert_eq!(interval_values(&ttl, "boundaryIntervalEnd"), vec![0.75]);
    }
}
#[test]
fn section_known_half_and_local_axis_are_exempt() {
    for extent in ["half", "local"] {
        let (mut d, _) = drawing("unknown", extent, 1.);
        axis(&mut d, [-12., 0.], [36., 0.]);
        if extent == "half" {
            d.views
                .push(json!({"id":"view:main","evidence":["section_type:half"]}));
        }
        assert!(new_findings(&d).is_empty(), "{extent}");
    }
}
#[test]
fn detail_axis_boundary_is_exempt() {
    let (mut d, _) = drawing("unknown", "unknown", 1.);
    axis(&mut d, [-12., 0.], [36., 0.]);
    d.views.push(json!({
        "id":"view:main",
        "kind":"detail_view",
        "label":"I 2:1"
    }));
    assert!(new_findings(&d).is_empty(), "{:?}", new_findings(&d));
}
#[test]
fn section_unknown_context_is_not_an_exemption() {
    assert_eq!(new_findings(&fixture(RULE_IDS[1], true)).len(), 3);
}
#[test]
fn section_partial_outline_cover_is_partitioned() {
    let mut d = fixture(RULE_IDS[1], true);
    outline(&mut d, [0., 0.], [12., 0.]);
    let fs = new_findings(&d);
    assert_eq!(fs.len(), 3);
    for ttl in written_facts(&d).into_values() {
        assert_eq!(
            interval_values(&ttl, "boundaryIntervalStart"),
            vec![0.25, 0.5]
        );
        assert_eq!(
            interval_values(&ttl, "boundaryIntervalEnd"),
            vec![0.5, 0.75]
        );
    }
    outline(&mut d, [12., 0.], [24., 0.]);
    assert!(new_findings(&d).is_empty());
}
#[test]
fn section_arc_and_bulge_boundaries() {
    let mut d = Drawing::default();
    let t = d.text("QZ", [80., 70.], 4., 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    face(
        &mut d,
        &[vec![[0., 0., 1.], [24., 0., 1.]]],
        "unknown",
        "unknown",
        1.,
    );
    let h = d.circle([12., 0.], 12.);
    d.member(&h, "center_line", "thin_dash_dot", "center_axis", "");
    assert_eq!(new_findings(&d).len(), 3);
    // Explicit circular edge with clockwise DXF angles, independent of polyline bulges.
    let hatch = d
        .entities
        .iter_mut()
        .find(|s| s.starts_with("0\nHATCH\n"))
        .unwrap();
    let prefix = hatch.split("2\nANSI31").next().unwrap().to_owned();
    *hatch = format!(
        "{prefix}2\nANSI31\n70\n0\n52\n45\n91\n1\n92\n1\n93\n1\n72\n2\n10\n12\n20\n0\n40\n12\n50\n0\n51\n360\n73\n0\n97\n0\n75\n0\n"
    );
    assert_eq!(new_findings(&d).len(), 3);
}
#[test]
fn pitch_circle_is_not_section_boundary() {
    let mut d = Drawing::default();
    let t = d.text("QZ", [80., 70.], 4., 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    let h = d.entity(
        "HATCH",
        "2\nANSI31\n70\n0\n52\n45\n91\n1\n92\n1\n72\n2\n10\n12\n20\n8\n40\n8\n50\n0\n51\n360\n73\n0\n93\n1\n97\n0\n75\n0\n",
    );
    let e = d.expression("section_hatch", "");
    d.member_in(&e, &h, "hatch_fill", "thin_continuous");
    d.expressions.last_mut().unwrap()["evidence_ids"] =
        json!(["section_context:local"]);
    let circle = d.arc([12., 8.], 8., 0., 360.);
    d.member(&circle, "center_line", "thin_dash_dot", "center_axis", "");
    let fs = new_findings(&d);
    assert!(
        fs.iter().all(|f| !f.rule_id.starts_with("G2-028")),
        "{fs:?}"
    );
}
#[test]
fn section_point_contact_and_arc_chord_are_not_boundaries() {
    let (mut d, _) = drawing("unknown", "unknown", 1.);
    axis(&mut d, [-12., -12.], [0., 0.]);
    assert!(new_findings(&d).is_empty());
    let mut d = Drawing::default();
    let t = d.text("QZ", [80., 70.], 4., 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    face(
        &mut d,
        &[vec![[0., 0., 1.], [24., 0., 1.]]],
        "unknown",
        "unknown",
        1.,
    );
    axis(&mut d, [0., 0.], [24., 0.]);
    assert!(new_findings(&d).is_empty());
}
#[test]
fn section_internal_outline_known_and_unknown() {
    for id in &RULE_IDS[2..4] {
        let fs = new_findings(&fixture(id, true));
        assert_eq!(fs.len(), 3);
        assert!(fs.iter().all(|f| f.rule_id == *id));
    }
}
#[test]
fn section_thread_exception_is_in_shape() {
    let (mut d, _) = drawing("removed", "unknown", 1.);
    let h = outline(&mut d, [-12., 8.], [36., 8.]);
    d.member(
        &h,
        "thread_line",
        "thin_continuous",
        "thread_convention",
        "",
    );
    assert!(new_findings(&d).is_empty(), "{:?}", new_findings(&d));
}
#[test]
fn section_parent_outline_exception_needs_region_evidence() {
    let (mut d, e) = drawing("superimposed", "unknown", 1.);
    outline(&mut d, [-12., 8.], [36., 8.]);
    origin(&mut d, &e, "pre_existing");
    assert!(new_findings(&d).is_empty());
    origin(&mut d, &e, "unknown");
    assert_eq!(new_findings(&d).len(), 3);
}
#[test]
fn section_origins_do_not_leak_between_regions() {
    let (mut d, e) = drawing("superimposed", "unknown", 1.);
    let other = face(&mut d, &[rectangle()], "removed", "unknown", 1.);
    outline(&mut d, [-12., 8.], [36., 8.]);
    origin(&mut d, &e, "pre_existing");
    let fs = new_findings(&d);
    assert_eq!(fs.len(), 3);
    assert!(fs.iter().all(|f| f.expression_ids == vec![other.clone()]));
}
#[test]
fn section_holes_and_tangency_preserve_empty_space() {
    let mut d = Drawing::default();
    let t = d.text("QZ", [80., 70.], 4., 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    let hole = vec![[6., 4., 0.], [18., 4., 0.], [18., 12., 0.], [6., 12., 0.]];
    face(&mut d, &[rectangle(), hole], "removed", "unknown", 1.);
    outline(&mut d, [8., 8.], [16., 8.]);
    outline(&mut d, [-12., 0.], [36., 0.]);
    assert!(new_findings(&d).is_empty());
    outline(&mut d, [-12., 2.], [36., 2.]);
    assert_eq!(new_findings(&d).len(), 3);
}
#[test]
fn section_incomplete_native_geometry_is_not_a_cut_face() {
    for (before, after) in [
        ("73\n1\n93\n4", "73\n0\n93\n4"),
        ("75\n0\n", "75\n1\n"),
        ("75\n0\n", "75\n2\n"),
    ] {
        let mut d = fixture(RULE_IDS[3], true);
        let h = d
            .entities
            .iter_mut()
            .find(|s| s.starts_with("0\nHATCH\n"))
            .unwrap();
        assert!(h.contains(before));
        *h = h.replace(before, after);
        assert!(
            new_findings(&d).is_empty(),
            "unsupported path/style {after}"
        );
    }
}
#[test]
fn section_layer_intervals_are_independent() {
    let mut d = fixture(RULE_IDS[3], true);
    d.members.last_mut().unwrap()["interval"] = json!([0., 0.2]);
    let fs = new_findings(&d);
    assert_eq!(fs.len(), 1);
    assert_eq!(fs[0].layer, "source");
}
#[test]
fn section_rules_are_scale_equivariant() {
    for scale in [0.2, 1., 7.] {
        let (mut d, _) = drawing("unknown", "unknown", scale);
        axis(&mut d, [-12. * scale, 0.], [36. * scale, 0.]);
        outline(
            &mut d,
            [-12. * scale, 8. * scale],
            [36. * scale, 8. * scale],
        );
        assert_eq!(new_findings(&d).len(), 6, "scale {scale}");
    }
}
#[test]
fn section_knowledge_metadata_and_synthetic_coverage_are_bound() {
    let k: toml::Value = toml::from_str(drafting::KNOWLEDGE).unwrap();
    let tests = include_str!("section_rule_fixtures.rs");
    for group in k["section_rule_knowledge"].as_array().unwrap() {
        for key in ["variants", "counter_examples"] {
            for example in group[key].as_array().unwrap() {
                let name = example.as_str().unwrap().rsplit(": ").next().unwrap();
                assert!(
                    tests.contains(&format!("fn {name}(")),
                    "missing test {name}"
                );
            }
        }
        assert!(tests.contains(&format!("fn {}(", group["scale_test"].as_str().unwrap())));
    }
    let rules = drafting::rules(k["shapes"]["text"].as_str().unwrap()).unwrap();
    for id in RULE_IDS {
        assert!(
            !rules
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .messages
                .is_empty()
        );
        let start = k["shapes"]["text"]
            .as_str()
            .unwrap()
            .find(&format!("rule:{id} a sh:NodeShape"))
            .unwrap();
        let body = &k["shapes"]["text"].as_str().unwrap()[start..];
        let body = body.split("\n\n").next().unwrap();
        assert!(
            body.contains("md:confidence")
                && body.contains("md:proposedCorrection")
                && body.contains("md:clause")
        );
        if id.ends_with("unknown") {
            assert!(body.contains("needs_expert"));
        }
    }
}

#[test]
fn section_extrusion_numbers_are_not_string_identifiers() {
    let mut d = fixture(RULE_IDS[1], true);
    let h = d
        .entities
        .iter_mut()
        .find(|s| s.starts_with("0\nHATCH\n"))
        .unwrap();
    h.push_str("210\n0.0\n220\n0.0\n230\n1.0\n");
    assert_eq!(new_findings(&d).len(), 3);
    let h = d
        .entities
        .iter_mut()
        .find(|s| s.starts_with("0\nHATCH\n"))
        .unwrap();
    *h = h.replace("230\n1.0", "230\n-1.0");
    assert!(new_findings(&d).is_empty());
}
#[test]
fn section_thread_alias_must_cover_the_same_interval() {
    let (mut d, _) = drawing("removed", "unknown", 1.);
    let h = outline(&mut d, [-12., 8.], [36., 8.]);
    d.member(
        &h,
        "thread_line",
        "thin_continuous",
        "thread_convention",
        "",
    );
    d.members.last_mut().unwrap()["interval"] = json!([0., 0.2]);
    let fs = new_findings(&d);
    assert_eq!(fs.len(), 2);
    assert!(fs.iter().all(|f| f.layer != "source"));
}

#[test]
fn section_hatch_boundary_crossing_is_reported() {
    let d = fixture("G2-019-section-hatch-crosses-outline", true);
    let fs = new_findings(&d);
    assert_eq!(fs.len(), 3);
    assert!(
        fs.iter()
            .all(|f| f.rule_id == "G2-019-section-hatch-crosses-outline")
    );
}

#[test]
fn section_hatch_boundary_touch_is_not_reported() {
    let d = fixture("G2-019-section-hatch-crosses-outline", false);
    let fs = new_findings(&d);
    assert!(fs.is_empty(), "{fs:?}");
}

#[test]
fn section_native_boundary_chain_is_reported_source_layer() {
    let d = native_boundary_chain_drawing(&[[[0., 10.], [10., 0.]]]);
    let fs = new_findings(&d);
    assert!(
        fs.iter().all(|f| f.rule_id != "G2-018-outline-in-cut-face"
            && f.rule_id != "G2-018-outline-in-cut-face-unknown"),
        "{fs:?}"
    );
    assert!(
        fs.iter().any(|f| {
            f.rule_id == "G2-019-section-hatch-crosses-outline" && f.layer == "source"
        }),
        "native HATCH boundary chain must be visible to source layer: {fs:?}"
    );
}

#[test]
fn section_native_boundary_slot_chain_is_reported() {
    let d = native_boundary_chain_drawing(&[
        [[0., 20.], [6., 20.]],
        [[6., 20.], [6., 30.]],
        [[6., 30.], [0., 30.]],
    ]);
    let fs = new_findings(&d);
    assert!(fs.iter().all(|f| !f.rule_id.starts_with("G2-018")), "{fs:?}");
    assert!(
        fs.iter()
            .filter(|f| f.rule_id == "G2-019-section-hatch-crosses-outline")
            .any(|f| f.layer == "source"),
        "{fs:?}"
    );
}

#[test]
fn section_large_boundary_cut_remains_interior() {
    let d = native_boundary_chain_drawing(&[[[0., 0.], [100., 100.]]]);
    let fs = new_findings(&d);
    assert!(
        fs.iter().any(|f| f.rule_id == "G2-018-outline-in-cut-face")
            && fs.iter().all(|f| f.rule_id != "G2-019-section-hatch-crosses-outline"),
        "large cut-off area must not be promoted: {fs:?}"
    );
}

#[test]
fn section_two_sided_boundary_cut_remains_interior() {
    let d = native_boundary_chain_drawing(&[
        [[0., 10.], [10., 0.]],
        [[1., 1.], [2., 2.]],
        [[50., 50.], [60., 60.]],
    ]);
    let fs = new_findings(&d);
    assert!(
        fs.iter().any(|f| f.rule_id == "G2-018-outline-in-cut-face")
            && fs.iter().all(|f| f.rule_id != "G2-019-section-hatch-crosses-outline"),
        "two-sided entity evidence must not be promoted: {fs:?}"
    );
}

#[test]
fn section_local_pitch_circle_is_not_an_axis_divider() {
    let mut d = Drawing::default();
    let t = d.text("QZ", [80., 70.], 4., 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    face(
        &mut d,
        &[vec![[0., 0., 1.], [24., 0., 1.]]],
        "unknown",
        "local",
        1.,
    );
    let h = d.circle([12., 0.], 12.);
    d.member(&h, "center_line", "thin_dash_dot", "center_axis", "");
    let fs = new_findings(&d);
    assert!(fs.iter().all(|f| !f.rule_id.starts_with("G2-028")), "{fs:?}");
}

#[test]
fn section_adjacent_hatch_paths_have_no_internal_boundary() {
    let mut d = Drawing::default();
    let t = d.text("QZ", [80., 70.], 4., 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    let left = vec![[0., 0., 0.], [12., 0., 0.], [12., 16., 0.], [0., 16., 0.]];
    let right = vec![[12., 0., 0.], [24., 0., 0.], [24., 16., 0.], [12., 16., 0.]];
    face(&mut d, &[left, right], "removed", "unknown", 1.);
    axis(&mut d, [12., -8.], [12., 24.]);
    assert!(new_findings(&d).is_empty());
    outline(&mut d, [6., 2.], [6., 14.]);
    let fs = new_findings(&d);
    assert_eq!(fs.len(), 3);
    assert!(fs.iter().all(|f| f.rule_id == RULE_IDS[2]));
    axis(&mut d, [-12., 0.], [36., 0.]);
    let fs = new_findings(&d);
    assert_eq!(fs.iter().filter(|f| f.rule_id == RULE_IDS[0]).count(), 3);
}
#[test]
fn section_partial_hatch_seam_keeps_the_unshared_boundary() {
    let mut d = Drawing::default();
    let t = d.text("QZ", [80., 70.], 4., 0.);
    d.member(&t, "note_text", "thin_continuous", "note", "");
    let extension = vec![[24., 0., 0.], [32., 0., 0.], [32., 8., 0.], [24., 8., 0.]];
    face(&mut d, &[rectangle(), extension], "removed", "unknown", 1.);
    axis(&mut d, [24., 0.], [24., 16.]);
    assert_eq!(new_findings(&d).len(), 3);
    for ttl in written_facts(&d).into_values() {
        assert_eq!(interval_values(&ttl, "boundaryIntervalStart"), vec![0.5]);
        assert_eq!(interval_values(&ttl, "boundaryIntervalEnd"), vec![1.]);
    }
}
