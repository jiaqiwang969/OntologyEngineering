//! Independent authored DXF/graph fixtures for reading deviations. No classifier
//! or evaluation corpus is used to manufacture the facts or expected answers.
use oe_drawing_check::drafting;
use serde_json::{Value, json};

const TERMINAL: &str = "G4-059-unresolved-terminator-on-carrier";
const CONTINUATION: &str = "G4-059-unresolved-dimension-continuation";

#[derive(Default)]
struct Drawing {
    records: Vec<String>,
    kinds: Vec<Value>,
    expressions: Vec<Value>,
    members: Vec<Value>,
    scale: f64,
    rotation: f64,
}
impl Drawing {
    fn new(scale: f64, rotation: f64) -> Self {
        Self {
            scale,
            rotation,
            ..Self::default()
        }
    }
    fn point(&self, p: [f64; 2]) -> [f64; 2] {
        let (s, c) = self.rotation.to_radians().sin_cos();
        [
            self.scale * (p[0] * c - p[1] * s),
            self.scale * (p[0] * s + p[1] * c),
        ]
    }
    fn entity(&mut self, kind: &str, tags: String) -> String {
        let h = format!("E{:X}", self.kinds.len() + 1);
        self.records
            .push(format!("0\n{kind}\n5\n{h}\n8\nSYNTH\n{tags}"));
        self.kinds.push(json!({"id":h,"kind":kind}));
        h
    }
    fn line(&mut self, a: [f64; 2], b: [f64; 2]) -> String {
        let (a, b) = (self.point(a), self.point(b));
        self.entity(
            "LINE",
            format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]),
        )
    }
    fn triangle(&mut self, tip: [f64; 2], base: [f64; 2], polyline: bool) -> String {
        let (a, b) = (self.point(tip), self.point(base));
        let n = [-(b[1] - a[1]) * 0.16, (b[0] - a[0]) * 0.16];
        let pts = [a, [b[0] + n[0], b[1] + n[1]], [b[0] - n[0], b[1] - n[1]]];
        if polyline {
            let mut tags = "90\n3\n70\n1\n".to_owned();
            for q in pts {
                tags.push_str(&format!("10\n{}\n20\n{}\n", q[0], q[1]));
            }
            self.entity("LWPOLYLINE", tags)
        } else {
            let mut tags = String::new();
            for (j, q) in [pts[0], pts[1], pts[2], pts[2]].iter().enumerate() {
                tags.push_str(&format!("{}\n{}\n{}\n{}\n", 10 + j, q[0], 20 + j, q[1]));
            }
            self.entity("SOLID", tags)
        }
    }
    fn arc(&mut self, radius: f64, start: f64, end: f64) -> String {
        self.entity(
            "ARC",
            format!(
                "10\n0\n20\n0\n40\n{}\n50\n{}\n51\n{}\n",
                radius * self.scale,
                start + self.rotation,
                end + self.rotation
            ),
        )
    }
    fn text(&mut self, p: [f64; 2]) -> String {
        let q = self.point(p);
        self.entity(
            "TEXT",
            format!(
                "10\n{}\n20\n{}\n40\n{}\n50\n{}\n1\n8\n",
                q[0],
                q[1],
                2. * self.scale,
                self.rotation
            ),
        )
    }
    fn stacked_text(&mut self, p: [f64; 2]) -> String {
        let q = self.point(p);
        self.entity(
            "MTEXT",
            format!(
                "10\n{}\n20\n{}\n40\n{}\n71\n7\n1\n17.5\\S+0.5^-0.5;\n",
                q[0],
                q[1],
                2. * self.scale
            ),
        )
    }
    fn expression(&mut self, kind: &str) -> String {
        let id = format!("expression:{}", self.expressions.len());
        self.expressions
            .push(json!({"id":id,"kind":kind,"label":"8"}));
        id
    }
    fn member(&mut self, expression: &str, handle: &str, role: &str) -> String {
        let id = format!("{expression}:m{}", self.members.len());
        self.members.push(json!({"id":id,"source_handle":handle,"expression_id":expression,
            "member_role":role,"line_type":"thin_continuous","interval":[0.,1.],"view_id":"view:fixture"}));
        id
    }
    fn unknown(&mut self, handle: &str) -> String {
        let e = self.expression("unresolved");
        self.member(&e, handle, "unresolved_source")
    }
    fn dimension(&mut self, kind: &str, line: &str, text: [f64; 2]) -> (String, String) {
        let e = self.expression(kind);
        let m = self.member(&e, line, "dimension_line");
        let t = self.text(text);
        self.member(&e, &t, "dimension_text");
        (e, m)
    }
    fn bytes(&self) -> Vec<u8> {
        format!(
            "0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
            self.records.concat()
        )
        .into_bytes()
    }
    fn graph(&self) -> Value {
        json!({"source":{"entities":self.kinds},"expressions":self.expressions,"members":self.members,
            "views":[],"structural_lines":[],"implied_spans":[]})
    }
    fn assert_hit(&self, rule: &str, handle: &str, member: &str, target: &str) {
        let graph = self.graph();
        let saved = graph.clone();
        let all = drafting::evaluate_bytes(&self.bytes(), &graph).unwrap();
        assert_eq!(graph, saved);
        for (layer, findings) in &all {
            let new: Vec<_> = findings
                .iter()
                .filter(|f| [TERMINAL, CONTINUATION].contains(&f.rule_id.as_str()))
                .collect();
            if layer != "recognition" {
                assert!(new.is_empty(), "{layer}: {new:#?}");
                continue;
            }
            assert_eq!(new.len(), 1, "{all:#?}");
            let f = new[0];
            assert_eq!(f.rule_id, rule);
            assert_eq!(f.focus, format!("recognition:{member}"));
            assert_eq!(f.source_handles, [handle]);
            assert!(f.message.contains("识别偏离"), "{f:#?}");
            let predicate = if rule == TERMINAL {
                "terminatorOnCarrier"
            } else {
                "boundedContinuationOf"
            };
            assert!(
                f.value
                    .as_deref()
                    .unwrap_or("")
                    .contains(&format!("{predicate} recognition:{target}")),
                "{f:#?}"
            );
        }
    }
    fn assert_clear(&self) {
        let all = drafting::evaluate_bytes(&self.bytes(), &self.graph()).unwrap();
        assert!(
            all.values()
                .flatten()
                .all(|f| ![TERMINAL, CONTINUATION].contains(&f.rule_id.as_str())),
            "{all:#?}"
        );
    }
    fn assert_source_hit(&self, rule: &str, handle: &str) {
        let all = drafting::evaluate_bytes(&self.bytes(), &self.graph()).unwrap();
        assert!(
            all.values()
                .flatten()
                .any(|f| { f.rule_id == rule && f.source_handles.iter().any(|h| h == handle) }),
            "{all:#?}"
        );
    }
}

fn terminal_fixture(
    scale: f64,
    rotation: f64,
    polyline: bool,
) -> (Drawing, String, String, String) {
    let mut d = Drawing::new(scale, rotation);
    let l = d.line([0., 0.], [24., 0.]);
    let (_, target) = d.dimension("dimension_linear", &l, [9., 1.]);
    let h = d.triangle([26., 0.], [24., 0.], polyline);
    let m = d.unknown(&h);
    (d, h, m, target)
}
fn continuation_fixture() -> (Drawing, String, String, String) {
    continuation_fixture_at(1., 0.)
}
fn continuation_fixture_at(scale: f64, rotation: f64) -> (Drawing, String, String, String) {
    let mut d = Drawing::new(scale, rotation);
    let l = d.line([0., 0.], [24., 0.]);
    let (e, target) = d.dimension("dimension_linear", &l, [30., 0.3]);
    let h = d.triangle([26., 0.], [24., 0.], false);
    d.member(&e, &h, "arrowhead");
    let piece = d.line([26., 0.], [32., 0.]);
    let m = d.unknown(&piece);
    (d, piece, m, target)
}

#[test]
fn straight_unresolved_terminal() {
    let (d, h, m, t) = terminal_fixture(1., 0., false);
    d.assert_hit(TERMINAL, &h, &m, &t);
}
#[test]
fn closed_polyline_terminal() {
    let (d, h, m, t) = terminal_fixture(1., 0., true);
    d.assert_hit(TERMINAL, &h, &m, &t);
}
#[test]
fn outside_text_continuation() {
    let (d, h, m, t) = continuation_fixture();
    d.assert_hit(CONTINUATION, &h, &m, &t);
}

fn continuation_fixture_with_gap(gap: f64) -> (Drawing, String) {
    let mut d = Drawing::new(1., 0.);
    let l = d.line([0., 0.], [24., 0.]);
    let (_, target) = d.dimension("dimension_linear", &l, [30., gap]);
    let h = d.triangle([26., 0.], [24., 0.], false);
    let e = d.expressions.last().unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    d.member(&e, &h, "arrowhead");
    let piece = d.line([26., 0.], [32., 0.]);
    d.unknown(&piece);
    (d, target)
}

#[test]
fn outside_text_normal_gap_uses_standard_height_ratio() {
    let (d, _) = continuation_fixture_with_gap(1.1);
    let piece = d
        .members
        .iter()
        .find(|m| m["member_role"] == "unresolved_source")
        .unwrap()["source_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    d.assert_source_hit(CONTINUATION, &piece);
}

#[test]
fn outside_text_gap_beyond_standard_ratio_stays_unknown() {
    let (d, _) = continuation_fixture_with_gap(2.2);
    d.assert_clear();
}

fn stacked_continuation_fixture(piece_end: [f64; 2]) -> (Drawing, String) {
    let mut d = Drawing::new(1., 0.);
    let carrier = d.line([0., 0.], [24., 0.]);
    let e = d.expression("dimension_linear");
    d.member(&e, &carrier, "dimension_line");
    // The stack drops below the main text.  The candidate is one unit below
    // the main box, but only at the supported gap from the stack's bottom.
    let text = d.stacked_text([30., 1.3]);
    d.member(&e, &text, "dimension_text");
    let h = d.triangle([26., 0.], [24., 0.], false);
    d.member(&e, &h, "arrowhead");
    let piece = d.line([26., 0.], piece_end);
    d.unknown(&piece);
    (d, piece)
}

#[test]
fn outside_text_with_stacked_tolerance_uses_full_annotation_span() {
    let (d, piece) = stacked_continuation_fixture([41.5, 0.]);
    d.assert_source_hit(CONTINUATION, &piece);
}

#[test]
fn outside_text_stacked_tolerance_beyond_end_stays_unknown() {
    let (d, _) = stacked_continuation_fixture([43., 0.]);
    d.assert_clear();
}

fn radius_fixture(support: bool) -> (Drawing, String, String, String) {
    radius_fixture_with_role(support.then_some("contour_segment"))
}
fn radius_fixture_with_role(role: Option<&str>) -> (Drawing, String, String, String) {
    let mut d = Drawing::new(1., 0.);
    let l = d.line([0., 0.], [35., 0.]);
    let (_, t) = d.dimension("dimension_radius", &l, [15., 1.]);
    let h = d.triangle([29., 0.], [27., 0.], false);
    let m = d.unknown(&h);
    if let Some(role) = role {
        let a = d.arc(29., -35., 35.);
        let e = d.expression("visible_contour");
        d.member(&e, &a, role);
    }
    (d, h, m, t)
}
#[test]
fn radius_internal_terminal() {
    let (d, h, m, t) = radius_fixture(true);
    d.assert_hit(TERMINAL, &h, &m, &t);
}

#[test]
fn radius_hidden_outline_support_is_accepted() {
    let (d, h, m, t) = radius_fixture_with_role(Some("hidden_segment"));
    d.assert_hit(TERMINAL, &h, &m, &t);
}

#[test]
fn radius_leader_carrier_accepts_a_radius_terminal() {
    let mut d = Drawing::new(1., 0.);
    let l = d.line([0., 0.], [35., 0.]);
    let (e, target) = d.dimension("dimension_radius", &l, [15., 1.]);
    d.members
        .iter_mut()
        .find(|m| m["expression_id"] == e && m["member_role"] == "dimension_line")
        .unwrap()["member_role"] = json!("leader_line");
    let a = d.arc(29., -35., 35.);
    let outline = d.expression("visible_contour");
    d.member(&outline, &a, "contour_segment");
    let h = d.triangle([29., 0.], [27., 0.], false);
    let m = d.unknown(&h);
    d.assert_hit(TERMINAL, &h, &m, &target);
}

/// A leader-style radius keeps both physical segments as leader lines.  They
/// remain carriers for recognition-layer relations, but the radial
/// dimension-line rule must not run on either segment.
#[test]
fn radius_leader_segments_are_not_radial_dimension_lines() {
    let mut d = Drawing::new(1., 0.);
    let arc = d.arc(10., 0., 90.);
    let outline = d.expression("visible_contour");
    d.member(&outline, &arc, "contour_segment");
    let e = d.expression("dimension_radius");
    let inclined = d.line([7.0710678, 7.0710678], [3., 3.]);
    d.member(&e, &inclined, "leader_line");
    let shoulder = d.line([3., 3.], [13., 3.]);
    d.member(&e, &shoulder, "leader_line");
    let findings = drafting::evaluate_bytes(&d.bytes(), &d.graph()).unwrap();
    assert!(
        findings
            .values()
            .flatten()
            .all(|f| f.rule_id != "G4-006-radius-dimension-line-through-centre"),
        "{findings:#?}"
    );
}

#[test]
fn radius_without_arc_support_remains_unknown() {
    radius_fixture(false).0.assert_clear();
}

#[test]
fn radius_centreline_arc_support_is_accepted() {
    let (d, h, m, t) = radius_fixture_with_role(Some("center_line"));
    d.assert_hit(TERMINAL, &h, &m, &t);
}

#[test]
fn radius_annotation_arc_support_is_rejected() {
    radius_fixture_with_role(Some("leader_line"))
        .0
        .assert_clear();
}

#[test]
fn dimension_line_through_mid_arrow_uses_existing_head_rule() {
    let mut d = Drawing::new(1., 0.);
    // The dimension line comes from far away and reaches the arrow's base
    // midpoint.  The existing head rule already recognizes this member.
    let carrier = d.line([0., 0.], [26., 0.]);
    let (e, target) = d.dimension("dimension_linear", &carrier, [30., 0.3]);
    let h = d.triangle([24., 0.], [26., 0.], false);
    d.member(&e, &h, "arrowhead");
    let piece = d.line([24., 0.], [32., 0.]);
    let member = d.unknown(&piece);
    d.assert_hit(CONTINUATION, &piece, &member, &target);
}

#[test]
fn angular_terminal() {
    let mut d = Drawing::new(1., 0.);
    let l = d.arc(29., 0., 73.);
    let (_, t) = d.dimension("dimension_angle", &l, [17., 12.]);
    let h = d.triangle([29., 0.], [29., 2.], false);
    let m = d.unknown(&h);
    d.assert_hit(TERMINAL, &h, &m, &t);
}

#[test]
fn terminal_base_and_tail_components_are_related() {
    let (mut d, _, _, _) = terminal_fixture(1., 0., false);
    let base = d.line([24., -0.32], [24., 0.32]);
    d.unknown(&base);
    d.assert_source_hit(TERMINAL, &base);

    let (mut d, _, _, _) = terminal_fixture(1., 0., false);
    let tail = d.line([26., 0.], [28., 0.]);
    d.unknown(&tail);
    d.assert_source_hit(TERMINAL, &tail);
}
#[test]
fn native_head_root_interval() {
    let mut d = Drawing::new(1., 0.);
    let root = d.entity("DIMENSION", "12\n31\n22\n23\n2\nSYNTHBLOCK\n".into());
    let line = d.line([0., 0.], [24., 0.]);
    let (e, t) = d.dimension("dimension_linear", &line, [9., 1.]);
    let h = d.triangle([26., 0.], [24., 0.], false);
    d.member(&e, &h, "arrowhead");
    for m in &mut d.members {
        let native = m["source_handle"].as_str().unwrap().to_owned();
        m["source_handle"] = json!(root);
        m["native_member_id"] = json!(format!("{root}/{native}"));
    }
    let h = d.line([31., 23.], [57., 23.]);
    let m = d.unknown(&h);
    d.members.last_mut().unwrap()["interval"] = json!([24. / 26., 1.]);
    d.assert_hit(CONTINUATION, &h, &m, &t);
}
#[test]
fn constituents_transform_and_order() {
    for scale in [0.2, 1., 7.5] {
        for rotation in [0., 37., 129.] {
            let (mut d, h, m, t) = terminal_fixture(scale, rotation, false);
            d.assert_hit(TERMINAL, &h, &m, &t);
            d.records.reverse();
            d.kinds.reverse();
            d.expressions.reverse();
            d.members.reverse();
            d.assert_hit(TERMINAL, &h, &m, &t);
            let (mut d, h, m, t) = continuation_fixture_at(scale, rotation);
            d.assert_hit(CONTINUATION, &h, &m, &t);
            d.records.reverse();
            d.kinds.reverse();
            d.expressions.reverse();
            d.members.reverse();
            d.assert_hit(CONTINUATION, &h, &m, &t);
        }
    }
}
#[test]
fn parallel_line_is_not_continuation() {
    let (mut d, _, _, _) = continuation_fixture();
    d.members.pop();
    d.expressions.pop();
    let h = d.line([26., 5.], [32., 5.]);
    d.unknown(&h);
    d.assert_clear();
}
#[test]
fn crossing_line_is_not_continuation() {
    let (mut d, _, _, _) = continuation_fixture();
    d.members.pop();
    d.expressions.pop();
    let h = d.line([24., -2.], [24., 2.]);
    d.unknown(&h);
    d.assert_clear();
}
#[test]
fn wrong_axis_is_not_terminal() {
    let (mut d, _, _, _) = terminal_fixture(1., 0., false);
    d.members.pop();
    d.expressions.pop();
    let h = d.triangle([24., 0.], [24., 2.], false);
    d.unknown(&h);
    d.assert_clear();
}
#[test]
fn triangle_without_dimension_context() {
    let (mut d, _, _, _) = terminal_fixture(1., 0., false);
    d.members.retain(|m| m["member_role"] != "dimension_text");
    d.assert_clear();
}
#[test]
fn other_view_does_not_supply_owner() {
    let (mut d, _, _, _) = terminal_fixture(1., 0., false);
    d.members.last_mut().unwrap()["view_id"] = json!("view:other");
    d.assert_clear();
}
#[test]
fn competing_dimensions_remain_unknown() {
    let (mut d, _, _, _) = terminal_fixture(1., 0., false);
    let l = d.line([2., 0.], [24., 0.]);
    d.dimension("dimension_linear", &l, [12., 1.]);
    d.assert_clear();
    d.members.reverse();
    d.expressions.reverse();
    d.assert_clear();
}
#[test]
fn continuation_stops_at_another_terminal() {
    let (mut d, _, _, _) = continuation_fixture();
    let h = d.triangle([28., 0.], [28., 2.], false);
    let e = d.expression("view_callout");
    d.member(&e, &h, "view_arrow");
    d.assert_clear();
}
#[test]
fn continuation_stops_at_branch() {
    let (mut d, _, _, _) = continuation_fixture();
    let h = d.line([28., 0.], [28., 4.]);
    let e = d.expression("visible_contour");
    d.member(&e, &h, "contour_segment");
    d.assert_clear();
}
#[test]
fn already_owned_members_do_not_report() {
    let (mut d, h, _, _) = terminal_fixture(1., 0., false);
    d.members.pop();
    d.expressions.pop();
    d.member("expression:0", &h, "arrowhead");
    d.assert_clear();
    let (mut d, h, _, _) = continuation_fixture();
    d.members.pop();
    d.expressions.pop();
    d.member("expression:0", &h, "dimension_line");
    d.assert_clear();
}
#[test]
fn unknown_view_remains_unknown() {
    let (mut d, _, _, _) = terminal_fixture(1., 0., false);
    for m in &mut d.members {
        m["view_id"] = Value::Null;
    }
    d.assert_clear();
}
#[test]
fn covered_source_interval_is_not_missing() {
    let (mut d, _, _, t) = continuation_fixture();
    let h = d.line([24., 0.], [26., 0.]);
    d.members.pop();
    d.expressions.pop();
    let e = d.members.iter().find(|m| m["id"] == t).unwrap()["expression_id"]
        .as_str()
        .unwrap()
        .to_owned();
    d.member(&e, &h, "dimension_line");
    d.unknown(&h);
    d.assert_clear();
}
#[test]
fn four_sided_fill_is_not_terminal() {
    let (mut d, _, _, _) = terminal_fixture(1., 0., false);
    d.members.pop();
    d.expressions.pop();
    let h = d.entity(
        "SOLID",
        "10\n26\n20\n0\n11\n24\n21\n1\n12\n24\n22\n-1\n13\n26\n23\n1\n".into(),
    );
    d.unknown(&h);
    d.assert_clear();
}

#[test]
fn constituent_knowledge_links_shapes_tests_and_coverage() {
    let doc: toml::Value = toml::from_str(drafting::KNOWLEDGE).unwrap();
    let rules = drafting::rules(&drafting::knowledge().unwrap().shapes).unwrap();
    let ontology = doc["ontology"]["text"].as_str().unwrap();
    assert!(ontology.contains("md:group   a owl:AnnotationProperty ; rdfs:label \"概念组\"@zh .\nmd:confidence a owl:AnnotationProperty ; rdfs:label \"把握程度：certain 确定 / likely 很可能 / needs_expert 要问专家\"@zh .\nmd:proposedCorrection a owl:AnnotationProperty ; rdfs:label \"建议改法\"@zh ."));
    for id in [TERMINAL, CONTINUATION] {
        let r = rules.iter().find(|r| r.id == id).unwrap();
        assert!(!r.concept_ids.is_empty());
        assert!(r.clause.contains("GB/T"));
        let body = doc["shapes"]["text"]
            .as_str()
            .unwrap()
            .split(&format!("rule:{id} a sh:NodeShape"))
            .nth(1)
            .unwrap()
            .split("\nrule:")
            .next()
            .unwrap();
        assert!(body.contains("md:confidence \"likely\""));
        assert!(body.contains("md:proposedCorrection"));
        assert!(body.contains("保留原图"));
    }
    for group in ["variants", "counter_examples"] {
        for item in doc["constituents"][group].as_array().unwrap() {
            let name = item["test"].as_str().unwrap();
            assert!(
                include_str!("drafting_constituents.rs").contains(&format!("fn {name}(")),
                "{name}"
            );
            assert!(!item["meaning"].as_str().unwrap().is_empty());
        }
    }
    let coverage: toml::Value = toml::from_str(include_str!(
        "../../oe-drawing/knowledge/recognizer-coverage.toml"
    ))
    .unwrap();
    for id in ["DC-G4-006", "DC-G4-017", "DC-G4-032", "DC-G4-059"] {
        let c = coverage["concepts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"].as_str() == Some(id))
            .unwrap();
        assert!(
            c["by"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str() == Some("convention:unresolved_dimension_constituents_rules"))
        );
    }
}
