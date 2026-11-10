//! The drafting rules are ontology constraints: the vocabulary is declared once and agreed by the ontology, the
//! fact contract and the exporter; every rule has authored fixtures; the three layers are evaluated with the same
//! shapes. The fixtures are original drawings written here, not content of any real drawing.
use oe_drawing_check::drafting::{self, CLASSES, LAYERS, PREDICATES};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn spec_allowed(type_name: &str, field: &str) -> BTreeSet<String> {
    let spec: toml::Value = toml::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../specs/modules/drawing-understanding.toml"
        ))
        .unwrap(),
    )
    .unwrap();
    let t = spec["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"].as_str() == Some(type_name))
        .unwrap_or_else(|| panic!("type {type_name} missing"));
    let f = t["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"].as_str() == Some(field))
        .unwrap();
    f["allowed_values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn the_ontology_the_fact_contract_and_the_exporter_declare_the_same_vocabulary() {
    let k = drafting::knowledge().unwrap();
    let (classes, properties) = drafting::ontology_vocabulary(&k.ontology).unwrap();
    let code_classes: BTreeSet<String> = CLASSES.iter().map(|c| (*c).to_owned()).collect();
    let code_predicates: BTreeSet<String> = PREDICATES.iter().map(|p| (*p).to_owned()).collect();
    assert_eq!(classes, code_classes, "ontology classes versus exporter");
    assert_eq!(properties, code_predicates, "ontology properties versus exporter");
    assert_eq!(classes, spec_allowed("MdDraftingObject", "class"), "ontology classes versus spec");
    assert_eq!(
        properties,
        spec_allowed("MdDraftingRelation", "predicate"),
        "ontology properties versus spec"
    );
    assert_eq!(
        LAYERS.iter().map(|l| (*l).to_owned()).collect::<BTreeSet<_>>(),
        spec_allowed("MdDraftingObject", "layer")
    );
    let rules = drafting::rules(&k.shapes).unwrap();
    assert!(!rules.is_empty());
    for r in &rules {
        assert!(!r.target_classes.is_empty(), "{} targets nothing", r.id);
        for name in r.referenced.iter().chain(&r.target_classes) {
            assert!(
                classes.contains(name) || properties.contains(name),
                "rule {} references {name}, which the ontology does not declare",
                r.id
            );
        }
    }
}

// ------------------------------------------------------------------------------ an original drawing builder

#[derive(Default)]
struct Drawing {
    entities: Vec<String>,
    kinds: Vec<(String, String)>,
    members: Vec<Value>,
    expressions: Vec<Value>,
    implied: Vec<Value>,
    structural: Vec<Value>,
    views: Vec<Value>,
    n: usize,
}
impl Drawing {
    fn id(&mut self) -> String {
        self.n += 1;
        format!("F{:X}", self.n)
    }
    fn entity(&mut self, kind: &str, body: &str) -> String {
        let h = self.id();
        self.entities
            .push(format!("0\n{kind}\n5\n{h}\n8\n0\n{body}"));
        self.kinds.push((h.clone(), kind.to_owned()));
        h
    }
    fn line(&mut self, a: [f64; 2], b: [f64; 2]) -> String {
        self.entity(
            "LINE",
            &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]),
        )
    }
    fn arc(&mut self, c: [f64; 2], r: f64, a0: f64, a1: f64) -> String {
        self.entity(
            "ARC",
            &format!("10\n{}\n20\n{}\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]),
        )
    }
    fn circle(&mut self, c: [f64; 2], r: f64) -> String {
        self.entity("CIRCLE", &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]))
    }
    fn text(&mut self, s: &str, p: [f64; 2], height: f64, rotation: f64) -> String {
        self.entity(
            "TEXT",
            &format!("10\n{}\n20\n{}\n40\n{height}\n50\n{rotation}\n1\n{s}\n", p[0], p[1]),
        )
    }
    fn polyline(&mut self, points: &[[f64; 2]]) -> String {
        let mut body = format!("90\n{}\n70\n0\n", points.len());
        for p in points {
            body.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
        }
        self.entity("LWPOLYLINE", &body)
    }
    fn hatch(&mut self, angle: f64) -> String {
        self.entity("HATCH", &format!("2\nANSI31\n70\n0\n52\n{angle}\n91\n0\n"))
    }
    /// A hatch with pattern data: one pattern line at `angle` whose rows are `spacing` apart.
    fn hatch_pattern(&mut self, angle: f64, spacing: f64) -> String {
        let t = angle.to_radians();
        let (dx, dy) = (-t.sin() * spacing, t.cos() * spacing);
        self.entity(
            "HATCH",
            &format!("2\nANSI31\n70\n0\n91\n0\n52\n{angle}\n78\n1\n53\n{angle}\n43\n0\n44\n0\n45\n{dx}\n46\n{dy}\n79\n0\n"),
        )
    }
    /// An expression without members yet; members join it with `member_in`.
    fn expression(&mut self, kind: &str, label: &str) -> String {
        let e = format!("expression:{}", self.expressions.len());
        self.expressions
            .push(json!({"id": e, "kind": kind, "label": label}));
        e
    }
    fn member_in(&mut self, e: &str, handle: &str, role: &str, line_type: &str) {
        let n = self.members.iter().filter(|m| m["expression_id"] == e).count();
        let m = format!("{e}:m{n}");
        self.members.push(json!({"id": m, "source_handle": handle, "member_role": role, "line_type": line_type,
            "expression_id": e, "interval": [0.0, 1.0], "view_id": "view:main"}));
    }
    /// A detail view of the graph labelled by the given texts.
    fn detail_view(&mut self, id: &str, label_handles: &[&str]) {
        self.views.push(json!({"id": id, "kind": "detail_view", "label": "", "label_source_ids": label_handles,
            "bbox": [0.0, 0.0, 1.0, 1.0], "state": "candidate"}));
    }
    /// A member with its own expression; the role gives the class, the kind and label the recognition.
    fn member(&mut self, handle: &str, role: &str, line_type: &str, kind: &str, label: &str) {
        let e = self.expression(kind, label);
        self.member_in(&e, handle, role, line_type);
    }
    fn implied_span(&mut self, from: &str, to: &str, a: [f64; 2], b: [f64; 2]) {
        self.implied.push(json!({"line_class": "visible_outline", "points": [a[0], a[1], b[0], b[1]], "arc": null,
            "from_source_handle": from, "to_source_handle": to, "view_id": "view:main", "cause": "detail_circle_crossing"}));
    }
    fn dxf(&self) -> Vec<u8> {
        format!(
            "0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
            self.entities.concat()
        )
        .into_bytes()
    }
    fn graph(&self) -> Value {
        json!({
            "expressions": self.expressions,
            "members": self.members,
            "structural_lines": self.structural,
            "implied_spans": self.implied,
            "views": self.views,
            "source": {"entities": self.kinds.iter().map(|(h, k)| json!({"id": h, "kind": k})).collect::<Vec<_>>()},
        })
    }
    fn findings(&self) -> BTreeMap<String, Vec<oe_contracts::MdDraftingFinding>> {
        drafting::evaluate_bytes(&self.dxf(), &self.graph()).unwrap()
    }
    fn rule_ids(&self) -> BTreeSet<String> {
        self.findings()
            .values()
            .flatten()
            .map(|f| f.rule_id.clone())
            .collect()
    }
}

/// A wavy boundary between x=0 and x=10 whose ends sit on two vertical outlines.
fn wavy(d: &mut Drawing, x0: f64, x1: f64) -> String {
    let w = d.polyline(&[
        [x0, 0.0],
        [x0 + 0.25 * (x1 - x0), 1.0],
        [x0 + 0.5 * (x1 - x0), -0.5],
        [x0 + 0.75 * (x1 - x0), 1.2],
        [x1, 0.3],
    ]);
    d.member(&w, "break_curve", "wavy", "break_line", "");
    w
}
fn outline(d: &mut Drawing, a: [f64; 2], b: [f64; 2]) -> String {
    let l = d.line(a, b);
    d.member(&l, "contour_segment", "thick_continuous", "visible_contour", "");
    l
}

/// The violating and the conforming drawing of one rule.
fn fixture(rule: &str, violating: bool) -> Drawing {
    let mut d = Drawing::default();
    match rule {
        "G4-059-unresolved-terminator-on-carrier" | "G4-059-unresolved-dimension-continuation" => {
            let e = d.expression("dimension_linear", "8");
            let l = d.line([0., 0.], [24., 0.]);
            d.member_in(&e, &l, "dimension_line", "thin_continuous");
            let t = d.text("8", [9., 1.], 2., 0.);
            d.member_in(&e, &t, "dimension_text", "thin_continuous");
            let h = d.entity("SOLID", "10\n26\n20\n0\n11\n24\n21\n0.32\n12\n24\n22\n-0.32\n13\n24\n23\n-0.32\n");
            if rule == "G4-059-unresolved-terminator-on-carrier" && violating {
                d.member(&h, "unresolved_source", "thin_continuous", "unresolved", "");
            } else {
                d.member_in(&e, &h, "arrowhead", "thin_continuous");
            }
            if rule == "G4-059-unresolved-dimension-continuation" {
                let remainder = d.line([24., 0.], [26., 0.]);
                if violating {
                    d.member(&remainder, "unresolved_source", "thin_continuous", "unresolved", "");
                } else {
                    d.member_in(&e, &remainder, "dimension_line", "thin_continuous");
                }
            }
        }
        "G1-032-wavy-ends-on-outline" => {
            outline(&mut d, [0.0, -5.0], [0.0, 5.0]);
            outline(&mut d, [10.0, -5.0], [10.0, 5.0]);
            if violating {
                wavy(&mut d, 2.0, 8.0);
            } else {
                wavy(&mut d, 0.0, 10.0);
            }
        }
        "G1-032-wavy-not-crossed-by-outline" => {
            outline(&mut d, [0.0, -5.0], [0.0, 5.0]);
            outline(&mut d, [10.0, -5.0], [10.0, 5.0]);
            wavy(&mut d, 0.0, 10.0);
            if violating {
                outline(&mut d, [-4.0, 0.4], [14.0, 0.4]);
            } else {
                outline(&mut d, [-4.0, 4.0], [14.0, 4.0]);
            }
        }
        "G1-055-centre-line-not-coincident-with-outline" => {
            outline(&mut d, [0.0, 0.0], [20.0, 0.0]);
            let y = if violating { 0.05 } else { 3.0 };
            let c = d.line([-3.0, y], [23.0, y]);
            d.member(&c, "center_line", "thin_dash_dot", "center_axis", "");
        }
        "G1-055-hidden-line-not-coincident-with-outline" => {
            outline(&mut d, [0.0, 0.0], [20.0, 0.0]);
            let y = if violating { -0.05 } else { 2.0 };
            let h = d.line([5.0, y], [15.0, y]);
            d.member(&h, "hidden_segment", "thin_dashed", "hidden_contour", "");
        }
        "G1-028-leader-not-crossing-text-or-leader" => {
            let t = d.text("Ra 3.2", [10.0, 10.0], 3.5, 0.0);
            d.member(&t, "note_text", "text", "technical_general", "Ra 3.2");
            let end = if violating { [30.0, 30.0] } else { [8.0, 8.0] };
            let l = d.line([0.0, 0.0], end);
            d.member(&l, "leader_line", "thin_continuous", "surface_roughness", "");
        }
        "G2-023-section-boundary-not-coincident" => {
            let e = d.line([0.0, 0.0], [20.0, 0.0]);
            d.member(&e, "extension_line", "thin_continuous", "dimension_linear", "12");
            let y = if violating { 0.0 } else { 2.0 };
            let b = d.polyline(&[[2.0, y], [6.0, y], [10.0, y], [14.0, y], [18.0, y]]);
            d.member(&b, "break_curve", "thin_continuous", "break_line", "");
        }
        "G2-019-hatch-angle-45" => {
            let dy = if violating { 0.577 } else { 1.0 };
            for i in 0..3 {
                let x = 2.0 * i as f64;
                let h = d.line([x, 0.0], [x + 4.0, 4.0 * dy]);
                d.member(&h, "hatch_fill", "thin_continuous", "section_hatch", "");
            }
            let region = d.hatch(if violating { 30.0 } else { 45.0 });
            d.member(&region, "hatch_fill", "fill", "section_hatch", "");
        }
        "G2-001-hatch-broken-at-text" => {
            let t = d.text("5", [10.0, 10.0], 3.5, 0.0);
            d.member(&t, "note_text", "text", "technical_general", "5");
            let h = if violating {
                d.line([6.0, 6.0], [16.0, 16.0])
            } else {
                d.line([0.0, 0.0], [6.0, 6.0])
            };
            d.member(&h, "hatch_fill", "thin_continuous", "section_hatch", "");
        }
        "G4-001-dimension-text-not-crossed" => {
            let t = d.text("%%c40", [10.0, 10.0], 3.5, 0.0);
            d.member(&t, "dimension_text", "text", "dimension_diameter", "Ø40");
            if violating {
                let c = d.line([0.0, 11.5], [40.0, 11.5]);
                d.member(&c, "center_line", "thin_dash_dot", "center_axis", "");
            } else {
                for (a, b) in [([0.0, 11.5], [8.5, 11.5]), ([22.0, 11.5], [40.0, 11.5])] {
                    let c = d.line(a, b);
                    d.member(&c, "center_line", "thin_dash_dot", "center_axis", "");
                }
            }
        }
        "G4-032-angle-unit-written" => {
            let raw = if violating { "30" } else { "30%%d" };
            let t = d.text(raw, [10.0, 10.0], 3.5, 0.0);
            d.member(&t, "dimension_text", "text", "dimension_angle", "30°");
        }
        "G4-032-angle-text-horizontal" => {
            let rotation = if violating { 45.0 } else { 0.0 };
            let t = d.text("30%%d", [10.0, 10.0], 3.5, rotation);
            d.member(&t, "dimension_text", "text", "dimension_angle", "30°");
        }
        "G4-059-dimension-line-thin-continuous" => {
            let l = d.line([0.0, 0.0], [20.0, 0.0]);
            let lt = if violating { "thick_continuous" } else { "thin_continuous" };
            d.member(&l, "dimension_line", lt, "dimension_linear", "20");
        }
        "G4-058-extension-line-thin-continuous" => {
            let l = d.line([0.0, 0.0], [0.0, 8.0]);
            let lt = if violating { "thick_continuous" } else { "thin_continuous" };
            d.member(&l, "extension_line", lt, "dimension_linear", "20");
        }
        "G4-059-dimension-line-not-coincident" => {
            outline(&mut d, [0.0, 0.0], [20.0, 0.0]);
            let y = if violating { 0.0 } else { 6.0 };
            let l = d.line([0.0, y], [20.0, y]);
            d.member(&l, "dimension_line", "thin_continuous", "dimension_linear", "20");
        }
        // ---- second batch
        "G1-021-centre-line-beyond-outline" => {
            outline(&mut d, [0.0, -10.0], [0.0, 10.0]);
            outline(&mut d, [30.0, -10.0], [30.0, 10.0]);
            let (x0, x1) = if violating { (-0.3, 30.3) } else { (-3.0, 33.0) };
            let c = d.line([x0, 0.0], [x1, 0.0]);
            d.member(&c, "center_line", "thin_dash_dot", "center_axis", "");
        }
        "G1-114-scale-in-standard-series" => {
            let s = d.text(if violating { "1:7" } else { "1:2" }, [10.0, 10.0], 3.5, 0.0);
            d.member(&s, "scale_text", "text", "title_block", "");
        }
        "G2-009-hatch-direction-consistent" => {
            let a = d.hatch_pattern(45.0, 3.0);
            d.member(&a, "hatch_fill", "fill", "section_hatch", "");
            let b = d.hatch_pattern(if violating { 135.0 } else { 45.0 }, 3.0);
            d.member(&b, "hatch_fill", "fill", "section_hatch", "");
        }
        "G2-009-hatch-spacing-consistent" => {
            let a = d.hatch_pattern(45.0, 3.0);
            d.member(&a, "hatch_fill", "fill", "section_hatch", "");
            let b = d.hatch_pattern(45.0, if violating { 6.0 } else { 3.0 });
            d.member(&b, "hatch_fill", "fill", "section_hatch", "");
        }
        "G2-065-view-title-has-callout-letter" => {
            let e = d.expression("view_callout", "B向");
            let t = d.text("B向", [10.0, 30.0], 5.0, 0.0);
            d.member_in(&e, &t, "view_title", "text");
            if !violating {
                let c = d.expression("view_callout", "B");
                let l = d.text("B", [40.0, 0.0], 5.0, 0.0);
                d.member_in(&c, &l, "view_letter", "text");
                let arrow = d.line([35.0, 0.0], [35.0, 10.0]);
                d.member_in(&c, &arrow, "view_arrow", "thin_continuous");
            }
        }
        "G2-043-callout-letter-has-view-title" => {
            let c = d.expression("view_callout", "A");
            let l = d.text("A", [40.0, 0.0], 5.0, 0.0);
            d.member_in(&c, &l, "view_letter", "text");
            let arrow = d.line([35.0, 0.0], [35.0, 10.0]);
            d.member_in(&c, &arrow, "view_arrow", "thin_continuous");
            if !violating {
                let e = d.expression("view_callout", "A向");
                let t = d.text("A向", [10.0, 30.0], 5.0, 0.0);
                d.member_in(&e, &t, "view_title", "text");
            }
        }
        "G2-075-detail-view-title-has-scale" => {
            let e = d.expression("view_callout", "I 2:1");
            let t = d.text("I", [10.0, 30.0], 5.0, 0.0);
            d.member_in(&e, &t, "view_title", "text");
            d.detail_view("view:I2:1", &[&t]);
            if !violating {
                let s = d.text("2:1", [10.0, 24.0], 5.0, 0.0);
                d.member_in(&e, &s, "scale_text", "text");
            }
            let c = d.expression("view_callout", "I");
            let l = d.text("I", [48.0, 2.0], 3.5, 0.0);
            d.member_in(&c, &l, "view_letter", "text");
            let circle = d.circle([40.0, 5.0], 4.0);
            d.member_in(&c, &circle, "detail_circle", "thin_continuous");
        }
        "G4-058-extension-line-beyond-dimension-line" => {
            let e = d.expression("dimension_linear", "20");
            let l = d.line([0.0, 10.0], [20.0, 10.0]);
            d.member_in(&e, &l, "dimension_line", "thin_continuous");
            let far = if violating { 10.3 } else { 12.0 };
            let x0 = d.line([0.0, 0.0], [0.0, far]);
            d.member_in(&e, &x0, "extension_line", "thin_continuous");
            let x1 = d.line([20.0, 0.0], [20.0, 12.0]);
            d.member_in(&e, &x1, "extension_line", "thin_continuous");
        }
        "G4-058-extension-line-perpendicular" => {
            let e = d.expression("dimension_linear", "20");
            let l = d.line([0.0, 10.0], [20.0, 10.0]);
            d.member_in(&e, &l, "dimension_line", "thin_continuous");
            let top = if violating { [4.0, 12.0] } else { [0.0, 12.0] };
            let x0 = d.line([0.0, 0.0], top);
            d.member_in(&e, &x0, "extension_line", "thin_continuous");
            let x1 = d.line([20.0, 0.0], [20.0, 12.0]);
            d.member_in(&e, &x1, "extension_line", "thin_continuous");
        }
        "G4-006-radius-dimension-line-through-centre" => {
            let arc = d.arc([0.0, 0.0], 10.0, 0.0, 90.0);
            d.member(&arc, "contour_segment", "thick_continuous", "visible_contour", "");
            let e = d.expression("dimension_radius", "R10");
            let end = if violating { [3.0, 6.0] } else { [3.0, 3.0] };
            let l = d.line([7.0710678, 7.0710678], end);
            d.member_in(&e, &l, "dimension_line", "thin_continuous");
        }
        "G4-059-dimension-line-not-on-outline-extension" => {
            outline(&mut d, [0.0, 0.0], [20.0, 0.0]);
            let y = if violating { 0.0 } else { 6.0 };
            let l = d.line([23.0, y], [40.0, y]);
            d.member(&l, "dimension_line", "thin_continuous", "dimension_linear", "17");
        }
        "G4-102-linear-dimension-text-along-dimension-line" => {
            let e = d.expression("dimension_linear", "40");
            let l = d.line([5.0, 0.0], [5.0, 40.0]);
            d.member_in(&e, &l, "dimension_line", "thin_continuous");
            let (p, rotation) = if violating { ([6.5, 18.0], 0.0) } else { ([3.0, 18.0], 90.0) };
            let t = d.text("40", p, 3.5, rotation);
            d.member_in(&e, &t, "dimension_text", "text");
        }
        "G4-102-avoid-30-degree-zone" => {
            let end = if violating { [-5.18, 19.32] } else { [10.0, 17.32] };
            let l = d.line([0.0, 0.0], end);
            d.member(&l, "dimension_line", "thin_continuous", "dimension_linear", "20");
        }
        "G5-091-cylindrical-zone-for-axis-characteristics" => {
            let kind = if violating { "gps_runout" } else { "gps_position" };
            let t = d.text("%%c0.02", [10.0, 10.0], 3.5, 0.0);
            d.member(&t, "tolerance_value", "text", kind, "");
        }
        "G5-009-datum-letter-horizontal" => {
            let rotation = if violating { 90.0 } else { 0.0 };
            let t = d.text("A", [10.0, 10.0], 3.5, rotation);
            d.member(&t, "datum_letter", "text", "datum_symbol", "A");
        }
        "G6-073-roughness-value-in-series" => {
            let t = d.text(if violating { "3.5" } else { "3.2" }, [10.0, 10.0], 3.5, 0.0);
            d.member(&t, "roughness_value", "text", "surface_roughness", "");
        }
        other => panic!("no fixture for rule {other}"),
    }
    d
}

#[test]
fn every_rule_has_a_violating_and_a_conforming_fixture() {
    let k = drafting::knowledge().unwrap();
    let rules = drafting::rules(&k.shapes).unwrap();
    let ids: BTreeSet<String> = rules.iter().map(|r| r.id.clone()).collect();
    assert!(ids.len() >= 28, "{ids:?}");
    let groups = |g: &str| ids.iter().filter(|i| i.starts_with(g)).count();
    assert!(groups("G1-") >= 7 && groups("G2-") >= 8 && groups("G4-") >= 12 && groups("G5-") >= 2 && groups("G6-") >= 1, "{ids:?}");
    for id in &ids {
        let bad = fixture(id, true);
        assert_eq!(
            bad.rule_ids(),
            BTreeSet::from([id.clone()]),
            "violating fixture of {id}: {:#?}",
            bad.findings()
        );
        let good = fixture(id, false);
        assert!(
            good.rule_ids().is_empty(),
            "conforming fixture of {id}: {:#?}",
            good.findings()
        );
    }
}

/// The exception written as knowledge (DC-G4-016): two adjacent small dimensions whose arrows are drawn outside
/// have dimension lines that overlap; joined by the extension line between them they are a chain and not a fault.
#[test]
fn chained_small_dimensions_are_not_a_coincidence_fault() {
    let mut d = Drawing::default();
    let e1 = d.expression("dimension_linear", "5");
    let l1 = d.line([-3.0, 10.0], [8.0, 10.0]);
    d.member_in(&e1, &l1, "dimension_line", "thin_continuous");
    for x in [0.0, 5.0] {
        let x0 = d.line([x, 0.0], [x, 12.0]);
        d.member_in(&e1, &x0, "extension_line", "thin_continuous");
    }
    let e2 = d.expression("dimension_linear", "6");
    let l2 = d.line([2.0, 10.0], [14.0, 10.0]);
    d.member_in(&e2, &l2, "dimension_line", "thin_continuous");
    let x1 = d.line([11.0, 0.0], [11.0, 12.0]);
    d.member_in(&e2, &x1, "extension_line", "thin_continuous");
    let f = d.findings();
    assert!(f["source"].is_empty(), "{f:#?}");
    // Native dimensions stop their lines an arrow short of the extension lines; the chain still holds.
    let mut d = Drawing::default();
    let e1 = d.expression("dimension_linear", "40");
    let l1 = d.line([3.0, 10.0], [37.0, 10.0]);
    d.member_in(&e1, &l1, "dimension_line", "dimension");
    for x in [0.0, 40.0] {
        let x0 = d.line([x, 0.0], [x, 11.0]);
        d.member_in(&e1, &x0, "extension_line", "dimension");
    }
    let e2 = d.expression("dimension_linear", "5");
    for (p, q) in [([34.0, 10.0], [40.0, 10.0]), ([40.0, 10.0], [45.0, 10.0]), ([45.0, 10.0], [51.0, 10.0])] {
        let l = d.line(p, q);
        d.member_in(&e2, &l, "dimension_line", "dimension");
    }
    // The small dimension owns both of its extension lines; the one at x=40 is shared with the first dimension.
    for x in [40.0, 45.0] {
        let x1 = d.line([x, 0.0], [x, 11.0]);
        d.member_in(&e2, &x1, "extension_line", "dimension");
    }
    let f = d.findings();
    assert!(f["source"].is_empty(), "{f:#?}");
    // Two long chained dimensions whose lines overlap are not the small-dimension case: the fault stands.
    let mut d = Drawing::default();
    let e1 = d.expression("dimension_linear", "40");
    let l1 = d.line([0.0, 10.0], [43.0, 10.0]);
    d.member_in(&e1, &l1, "dimension_line", "thin_continuous");
    for x in [0.0, 40.0] {
        let x0 = d.line([x, 0.0], [x, 12.0]);
        d.member_in(&e1, &x0, "extension_line", "thin_continuous");
    }
    let e2 = d.expression("dimension_linear", "30");
    let l2 = d.line([37.0, 10.0], [70.0, 10.0]);
    d.member_in(&e2, &l2, "dimension_line", "thin_continuous");
    let x1 = d.line([70.0, 0.0], [70.0, 12.0]);
    d.member_in(&e2, &x1, "extension_line", "thin_continuous");
    let ids: BTreeSet<String> = d.findings()["source"].iter().map(|x| x.rule_id.clone()).collect();
    assert!(ids.contains("G4-059-dimension-line-not-coincident"), "{ids:?}");
    // The same two short lines without a shared extension line are a plain coincidence.
    let mut d = Drawing::default();
    let e1 = d.expression("dimension_linear", "5");
    let l1 = d.line([-3.0, 10.0], [8.0, 10.0]);
    d.member_in(&e1, &l1, "dimension_line", "thin_continuous");
    let e2 = d.expression("dimension_linear", "6");
    let l2 = d.line([2.0, 10.0], [14.0, 10.0]);
    d.member_in(&e2, &l2, "dimension_line", "thin_continuous");
    assert_eq!(d.rule_ids(), BTreeSet::from(["G4-059-dimension-line-not-coincident".to_owned()]));
}

/// Pieces of one line are measured as one line: a centre line broken at a text and a dimension line broken
/// at its number keep their outer ends; the inner ends of the pieces are not ends of the line.
#[test]
fn collinear_pieces_are_measured_as_one_line() {
    let mut d = Drawing::default();
    outline(&mut d, [0.0, -10.0], [0.0, 10.0]);
    outline(&mut d, [40.0, -10.0], [40.0, 10.0]);
    let e = d.expression("center_axis", "");
    for (p, q) in [([-3.0, 0.0], [15.0, 0.0]), ([22.0, 0.0], [43.0, 0.0])] {
        let c = d.line(p, q);
        d.member_in(&e, &c, "center_line", "thin_dash_dot");
    }
    let f = d.findings();
    assert!(f["source"].is_empty(), "{f:#?}");
    // A radius dimension line broken at its number still reaches the arc as one line.
    let mut d = Drawing::default();
    let arc = d.arc([0.0, 0.0], 20.0, 0.0, 90.0);
    d.member(&arc, "contour_segment", "thick_continuous", "visible_contour", "");
    let e = d.expression("dimension_radius", "R20");
    for (p, q) in [([14.142, 14.142], [10.0, 10.0]), ([6.0, 6.0], [2.0, 2.0])] {
        let l = d.line(p, q);
        d.member_in(&e, &l, "dimension_line", "dimension");
    }
    let f = d.findings();
    assert!(f["source"].is_empty(), "{f:#?}");
}

#[test]
fn the_same_shapes_are_evaluated_on_the_three_layers() {
    // A fault only the page draws: an inferred span bridging two outline pieces across a wavy boundary.
    let mut d = Drawing::default();
    outline(&mut d, [0.0, -5.0], [0.0, 5.0]);
    outline(&mut d, [10.0, -5.0], [10.0, 5.0]);
    wavy(&mut d, 0.0, 10.0);
    let a = outline(&mut d, [5.0, -8.0], [5.0, -3.0]);
    let b = outline(&mut d, [5.0, 3.0], [5.0, 8.0]);
    d.implied_span(&a, &b, [5.0, -3.0], [5.0, 3.0]);
    let f = d.findings();
    assert!(f["source"].is_empty() && f["recognition"].is_empty(), "{f:#?}");
    let presentation: BTreeSet<&str> = f["presentation"].iter().map(|x| x.rule_id.as_str()).collect();
    assert_eq!(presentation, BTreeSet::from(["G1-032-wavy-not-crossed-by-outline"]), "{f:#?}");
    assert!(
        f["presentation"]
            .iter()
            .all(|x| x.layer == "presentation" && x.value.as_deref().is_some_and(|v| v.contains("implied")))
    );
    // A fault only in the raw text, which the recognition already supplied: the unit of an angle.
    let mut d = Drawing::default();
    let t = d.text("30", [10.0, 10.0], 3.5, 0.0);
    d.member(&t, "dimension_text", "text", "dimension_angle", "30°");
    let f = d.findings();
    assert!(f["recognition"].is_empty(), "{f:#?}");
    for layer in ["source", "presentation"] {
        assert_eq!(f[layer].len(), 1, "{f:#?}");
        assert_eq!(f[layer][0].rule_id, "G4-032-angle-unit-written");
        assert_eq!(f[layer][0].layer, layer);
    }
}

#[test]
fn repeated_rule_reports_are_byte_identical() {
    use oe_core::{SkillRoot, sha256, write_new_bytes};
    use std::path::Path;

    let mut d = Drawing::default();
    let line = d.line([17.0, 29.0], [39.0, 29.0]);
    d.member(&line, "dimension_line", "thick_continuous", "dimension_linear", "22");
    let text = d.text("47", [61.0, 53.0], 2.6, 17.0);
    d.member(&text, "dimension_text", "text", "dimension_angle", "47");
    // Both property shapes of one rule fail on the same leader: deduplication must also be repeatable.
    for (a, b) in [([85.0, 73.0], [105.0, 93.0]), ([85.0, 93.0], [105.0, 73.0])] {
        let leader = d.line(a, b);
        d.member(&leader, "leader_line", "thin_continuous", "technical_note", "");
    }
    let note = d.text("SYNTHETIC", [91.0, 80.0], 5.0, 0.0);
    d.member(&note, "note_text", "text", "technical_note", "SYNTHETIC");
    let single_leader = d.line([145.0, 73.0], [165.0, 93.0]);
    d.member(&single_leader, "leader_line", "thin_continuous", "technical_note", "");
    let single_note = d.text("SINGLE", [151.0, 80.0], 5.0, 0.0);
    d.member(&single_note, "note_text", "text", "technical_note", "SINGLE");
    let raw = d.dxf();
    let mut graph = d.graph();
    graph["source_sha256"] = json!(sha256(&raw));
    let graph = serde_json::to_vec_pretty(&graph).unwrap();
    let software = SkillRoot::new(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap()).unwrap();
    let mut outputs = Vec::new();
    for _ in 0..2 {
        // Separate stores keep identical portable report paths, without overwriting the first run.
        // Retain these synthetic artifacts under the checkout's private test directory for inspection.
        let home = software.unique_private_dir(Path::new("var/tests"), "drafting-repeat").unwrap();
        let root = software.clone().with_project_data_root(&home).unwrap();
        std::fs::create_dir(home.join("classified")).unwrap();
        write_new_bytes(&home.join("source.dxf"), &raw).unwrap();
        write_new_bytes(&home.join("classified/graph.json"), &graph).unwrap();
        let request = json!({"schema":"oev2.drawing-discovery-request/1",
            "source":{"path":"source.dxf","sha256":sha256(&raw)},"output_directory":"classified"});
        write_new_bytes(&home.join("request.json"), &serde_json::to_vec(&request).unwrap()).unwrap();
        let report = drafting::check_rules(&root, &oe_contracts::MdDraftingRuleRequest {
            schema: drafting::REQUEST_SCHEMA.into(),
            discovery_request: "request.json".into(),
            output: "report.json".into(),
        }).unwrap();
        assert!(report.layers.iter().all(|layer| layer.finding_count > 1));
        let leaders: Vec<_> = report.findings.iter()
            .filter(|f| f.rule_id == "G1-028-leader-not-crossing-text-or-leader").collect();
        assert_eq!(leaders.len(), 9, "three leaders on each of the three layers");
        let mut single_count = 0;
        for finding in leaders {
            if finding.source_handles == [single_leader.clone()] {
                single_count += 1;
                assert_eq!(finding.message, "指引线穿过了注写文字");
            } else {
                // p1 sorts before p2 even though their message text sorts in the opposite order.
                assert_eq!(finding.message, "指引线穿过了注写文字；指引线与另一条指引线相穿");
            }
        }
        assert_eq!(single_count, 3);
        let angles: Vec<_> = report.findings.iter()
            .filter(|f| f.rule_id == "G4-032-angle-text-horizontal").collect();
        assert_eq!(angles.len(), 3);
        assert!(angles.iter().all(|f| f.message == "角度尺寸的数字不是水平书写"));
        let mut bytes = vec![std::fs::read(home.join("report.json")).unwrap()];
        for layer in &report.layers {
            bytes.push(std::fs::read(home.join(&layer.facts_file)).unwrap());
        }
        outputs.push(bytes);
    }
    for (index, (first, second)) in outputs[0].iter().zip(&outputs[1]).enumerate() {
        assert_eq!(sha256(first), sha256(second), "report/facts artifact {index} differs between identical runs");
        assert!(first == second);
    }
}
