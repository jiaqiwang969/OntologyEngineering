//! The drafting rules are ontology constraints: the vocabulary is declared once and agreed by the ontology, the
//! fact contract and the exporter; every rule has authored fixtures; the three layers are evaluated with the same
//! shapes. The fixtures are original drawings written here, not content of any real drawing.
use oe_drawing_check::drafting::{self, CLASSES, LAYERS, PREDICATES};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

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
    /// The same entities as a pixel graph carries them, geometry included.
    pixel: Vec<Value>,
    members: Vec<Value>,
    expressions: Vec<Value>,
    implied: Vec<Value>,
    structural: Vec<Value>,
    views: Vec<Value>,
    n: usize,
    /// Paper millimetres per drawing unit of the sheet; `None` is one (the fixtures are drawn at paper size).
    sheet: Option<f64>,
    /// The sheet scale left unknown, as on a drawing without a scale text or an annotation scale.
    sheet_unknown: bool,
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
    /// The pixel form of the entity just written: its geometry as a pixel graph carries it.
    fn pixel(&mut self, h: &str, kind: &str, geometry: Value) {
        let mut e = json!({"id": h, "kind": kind, "layer": "0"});
        for (k, v) in geometry.as_object().into_iter().flatten() {
            e[k] = v.clone();
        }
        self.pixel.push(e);
    }
    fn line(&mut self, a: [f64; 2], b: [f64; 2]) -> String {
        let h = self.entity(
            "LINE",
            &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]),
        );
        self.pixel(&h, "LINE", json!({"coordinates": [a[0], a[1], b[0], b[1]]}));
        h
    }
    fn arc(&mut self, c: [f64; 2], r: f64, a0: f64, a1: f64) -> String {
        let h = self.entity(
            "ARC",
            &format!("10\n{}\n20\n{}\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]),
        );
        // The graph contract: radians, counter-clockwise, start in [0, TAU), positive sweep (DXF runs a0 to a1 CCW).
        let (start, sweep) = (a0.to_radians().rem_euclid(TAU), (a1 - a0).rem_euclid(360.).to_radians());
        self.pixel(&h, "ARC", json!({"coordinates": [c[0], c[1], r, start, sweep]}));
        h
    }
    fn circle(&mut self, c: [f64; 2], r: f64) -> String {
        let h = self.entity("CIRCLE", &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]));
        self.pixel(&h, "CIRCLE", json!({"coordinates": [c[0], c[1], r, 0.0, TAU]}));
        h
    }
    fn text(&mut self, s: &str, p: [f64; 2], height: f64, rotation: f64) -> String {
        let h = self.entity(
            "TEXT",
            &format!("10\n{}\n20\n{}\n40\n{height}\n50\n{rotation}\n1\n{s}\n", p[0], p[1]),
        );
        self.pixel(&h, "TEXT", json!({"coordinates": [p[0], p[1], height, rotation.to_radians(), 1.0], "text": s}));
        h
    }
    fn polyline(&mut self, points: &[[f64; 2]]) -> String {
        let mut body = format!("90\n{}\n70\n0\n", points.len());
        for p in points {
            body.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
        }
        let h = self.entity("LWPOLYLINE", &body);
        let flat: Vec<f64> = points.iter().flatten().copied().collect();
        self.pixel(&h, "LWPOLYLINE", json!({"coordinates": flat}));
        h
    }
    fn hatch(&mut self, angle: f64) -> String {
        let h = self.entity("HATCH", &format!("2\nANSI31\n70\n0\n52\n{angle}\n91\n0\n"));
        self.pixel(&h, "HATCH", json!({"coordinates": [angle.to_radians()]}));
        h
    }
    /// A hatch with pattern data: one pattern line at `angle` whose rows are `spacing` apart.
    fn hatch_pattern(&mut self, angle: f64, spacing: f64) -> String {
        let t = angle.to_radians();
        let (dx, dy) = (-t.sin() * spacing, t.cos() * spacing);
        let h = self.entity(
            "HATCH",
            &format!("2\nANSI31\n70\n0\n91\n0\n52\n{angle}\n78\n1\n53\n{angle}\n43\n0\n44\n0\n45\n{dx}\n46\n{dy}\n79\n0\n"),
        );
        self.pixel(&h, "HATCH", json!({"coordinates": [angle.to_radians(), spacing]}));
        h
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
            "conditions": [if self.sheet_unknown {
                json!({"id": "sheet_scale", "state": "unknown", "value": null, "values": [], "basis": "none", "evidence": [], "note": "fixture"})
            } else {
                let s = self.sheet.unwrap_or(1.0);
                json!({"id": "sheet_scale", "state": "known", "value": s, "values": [s], "basis": "title_block_scale_text", "evidence": [], "note": "fixture"})
            }],
            "expressions": self.expressions,
            "members": self.members,
            "structural_lines": self.structural,
            "implied_spans": self.implied,
            "views": self.views,
            "source": {"entities": self.kinds.iter().map(|(h, k)| json!({"id": h, "kind": k})).collect::<Vec<_>>()},
        })
    }
    /// The same drawing as a pixel graph: members, expressions and structure unchanged, the geometry in the
    /// entities.
    fn pixel_graph(&self) -> Value {
        let mut g = self.graph();
        g["profile"] = json!(drafting::PIXEL_PROFILE);
        g["source"]["entities"] = json!(self.pixel);
        g
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

/// A pixel graph, whose geometry is its own entities, is evaluated like the DXF of the same drawing: every fixture,
/// drawn with lines, arcs, circles, polylines, texts and hatches, gives the same findings both ways, and a graph
/// written by hand with the coordinates of the dimension-text fixture gives the fixture's finding.
#[test]
fn a_pixel_graph_yields_the_findings_of_the_dxf_path() {
    let k = drafting::knowledge().unwrap();
    let rules = drafting::rules(&k.shapes).unwrap();
    let mut with_polylines = 0;
    for r in &rules {
        for violating in [true, false] {
            let d = fixture(&r.id, violating);
            let pixel = d.pixel_graph();
            let entities = pixel["source"]["entities"].as_array().unwrap();
            with_polylines += usize::from(entities.iter().any(|e| e["kind"] == "LWPOLYLINE"));
            let by_pixel = drafting::evaluate_pixel_graph(&pixel).unwrap();
            assert_eq!(d.findings(), by_pixel, "{} violating={violating}", r.id);
        }
    }
    // Wavy break lines and the section boundary are polylines: both fixtures of G1-032 (two rules) and of G2-023.
    assert!(with_polylines >= 6, "{with_polylines} fixtures with polylines");
    // By hand: the violating fixture of G4-001 (a diameter text crossed by a centre line) as a pixel graph, with the
    // fixture's coordinates as entity coordinates; the text's height, rotation and width factor follow its point.
    let d = fixture("G4-001-dimension-text-not-crossed", true);
    let pixel = json!({
        "profile": drafting::PIXEL_PROFILE,
        "expressions": [
            {"id": "expression:0", "kind": "dimension_diameter", "label": "Ø40"},
            {"id": "expression:1", "kind": "center_axis", "label": ""}
        ],
        "members": [
            {"id": "expression:0:m0", "source_handle": "F1", "member_role": "dimension_text", "line_type": "text",
             "expression_id": "expression:0", "interval": [0.0, 1.0], "view_id": "view:main"},
            {"id": "expression:1:m0", "source_handle": "F2", "member_role": "center_line", "line_type": "thin_dash_dot",
             "expression_id": "expression:1", "interval": [0.0, 1.0], "view_id": "view:main"}
        ],
        "structural_lines": [],
        "implied_spans": [],
        "views": [],
        "source": {"entities": [
            {"id": "F1", "kind": "TEXT", "layer": "0", "coordinates": [10.0, 10.0, 3.5, 0.0, 1.0], "text": "%%c40"},
            {"id": "F2", "kind": "LINE", "layer": "0", "coordinates": [0.0, 11.5, 40.0, 11.5]}
        ]}
    });
    let by_pixel = drafting::evaluate_pixel_graph(&pixel).unwrap();
    assert_eq!(d.findings(), by_pixel, "{by_pixel:#?}");
    for layer in LAYERS {
        assert_eq!(by_pixel[layer].len(), 1, "{by_pixel:#?}");
        assert_eq!(by_pixel[layer][0].rule_id, "G4-001-dimension-text-not-crossed");
        assert_eq!(by_pixel[layer][0].focus, format!("{layer}:expression:0:m0"));
        assert_eq!(by_pixel[layer][0].source_handles, vec!["F1".to_owned()]);
    }
    // The general entry: a pixel graph needs no bytes, a DXF graph needs its bytes.
    assert_eq!(drafting::evaluate_graph(&pixel, None).unwrap(), by_pixel);
    assert!(drafting::evaluate_graph(&d.graph(), None).is_err());
    assert_eq!(drafting::evaluate_graph(&d.graph(), Some(&d.dxf())).unwrap(), by_pixel);
}

/// A pixel text written with the font stacking marks (U+E1D6…U+E1DA) has the two boxes of the text model: the
/// stacked tolerance follows the main text, taller than it. A centre line passing above the main text through the
/// stack region alone crosses the dimension text; the same text written flat is one box the line passes above.
#[test]
fn a_stacked_pixel_text_is_crossed_through_its_stack_box() {
    let graph = |raw: &str| {
        json!({
            "profile": drafting::PIXEL_PROFILE,
            "expressions": [
                {"id": "expression:0", "kind": "dimension_diameter", "label": "Ø19+0.05/+0.02"},
                {"id": "expression:1", "kind": "center_axis", "label": ""}
            ],
            "members": [
                {"id": "expression:0:m0", "source_handle": "T1", "member_role": "dimension_text", "line_type": "text",
                 "expression_id": "expression:0", "interval": [0.0, 1.0], "view_id": "view:main"},
                {"id": "expression:1:m0", "source_handle": "L1", "member_role": "center_line", "line_type": "thin_dash_dot",
                 "expression_id": "expression:1", "interval": [0.0, 1.0], "view_id": "view:main"}
            ],
            "structural_lines": [],
            "implied_spans": [],
            "views": [],
            "source": {"entities": [
                // Text height 3.5 at (10, 10): the main text "Ø19" is 8.4 wide and 3.5 tall; the stack begins at
                // x = 18.4, 0.15 heights below the baseline, and is 1.3 heights tall, reaching y = 14.025.
                {"id": "T1", "kind": "TEXT", "layer": "0", "coordinates": [10.0, 10.0, 3.5, 0.0, 1.0], "text": raw},
                // A centre line at y = 13.3: above the main text's box, inside the stack's box.
                {"id": "L1", "kind": "LINE", "layer": "0", "coordinates": [0.0, 13.3, 40.0, 13.3]}
            ]}
        })
    };
    let stacked = drafting::evaluate_pixel_graph(&graph("%%C19\u{E1D6}+0.05\u{E1DA}\u{E1D7}+0.02\u{E1D9}")).unwrap();
    for layer in LAYERS {
        assert_eq!(stacked[layer].len(), 1, "{stacked:#?}");
        let f = &stacked[layer][0];
        assert_eq!(f.rule_id, "G4-001-dimension-text-not-crossed");
        assert_eq!(f.focus, format!("{layer}:expression:0:m0"));
        assert!(f.value.as_deref().is_some_and(|v| v.contains("expression:1:m0")), "{f:#?}");
    }
    let flat = drafting::evaluate_pixel_graph(&graph("%%C19+0.05+0.02")).unwrap();
    assert!(flat.values().all(Vec::is_empty), "{flat:#?}");
}

/// A leader recovered from the page as one polyline (arrow tip, knee, shelf along the bottom of its text's box)
/// crosses nothing: the shelf lies on the edge of the box, not through the text. The same leader recovered as two
/// strokes that run one unit past each other at the knee is two leader lines crossing there (G1-028), whether the
/// leader carries a note or a dimension text: the synthesizer has to emit a leader as one polyline. The DXF path
/// reads both forms alike.
#[test]
fn a_leader_polyline_crosses_nothing_where_overshooting_strokes_cross() {
    // The text, 3.5 high, has its box's lower-left corner at (12, 10) on the shelf line y = 10; the knee is (10, 10).
    let graph = |kind: &str, role: &str, label: &str, leader: &Value| -> Value {
        let mut entities = vec![json!({"id": "PX-TEXT-0001", "kind": "TEXT", "layer": "0", "line_type": "text",
            "coordinates": [12.0, 10.0, 3.5, 0.0, 1.0], "text": label})];
        let mut members = vec![json!({"id": "expression:0:m0", "source_handle": "PX-TEXT-0001", "member_role": role,
            "line_type": "text", "expression_id": "expression:0", "interval": [0.0, 1.0], "view_id": "view:main"})];
        for (n, e) in leader.as_array().unwrap().iter().enumerate() {
            members.push(json!({"id": format!("expression:0:m{}", n + 1), "source_handle": e["id"],
                "member_role": "leader_line", "line_type": "thin_continuous", "expression_id": "expression:0",
                "interval": [0.0, 1.0], "view_id": "view:main"}));
            entities.push(e.clone());
        }
        json!({
            "profile": drafting::PIXEL_PROFILE,
            "expressions": [{"id": "expression:0", "kind": kind, "label": label}],
            "members": members,
            "structural_lines": [],
            "implied_spans": [],
            "views": [],
            "source": {"entities": entities},
        })
    };
    let polyline = json!([{"id": "PX-POLY-0001", "kind": "LWPOLYLINE", "layer": "0", "line_type": "thin_continuous",
        "coordinates": [0.0, 0.0, 10.0, 10.0, 25.0, 10.0]}]);
    // Each stroke runs one unit past the knee: the slanted one on to (10.707, 10.707), the shelf back to x = 9.
    let k = 10.0 + std::f64::consts::FRAC_1_SQRT_2;
    let strokes = json!([
        {"id": "PX-LINE-0001", "kind": "LINE", "layer": "0", "line_type": "thin_continuous", "coordinates": [0.0, 0.0, k, k]},
        {"id": "PX-LINE-0002", "kind": "LINE", "layer": "0", "line_type": "thin_continuous", "coordinates": [9.0, 10.0, 25.0, 10.0]}
    ]);
    // A note, which G1-028 forbids a leader to cross, and a dimension text, which G4-001 forbids any line to cross.
    for (kind, role, label) in [("technical_general", "note_text", "4×M6"), ("dimension_chamfer", "dimension_text", "C1")] {
        let one = drafting::evaluate_pixel_graph(&graph(kind, role, label, &polyline)).unwrap();
        assert!(one.values().all(Vec::is_empty), "{role}: {one:#?}");
        let two = drafting::evaluate_pixel_graph(&graph(kind, role, label, &strokes)).unwrap();
        for layer in LAYERS {
            let (m1, m2) = (format!("{layer}:expression:0:m1"), format!("{layer}:expression:0:m2"));
            let found: BTreeSet<(&str, &str)> = two[layer].iter().map(|f| (f.rule_id.as_str(), f.focus.as_str())).collect();
            let rule = "G1-028-leader-not-crossing-text-or-leader";
            assert_eq!(found, BTreeSet::from([(rule, m1.as_str()), (rule, m2.as_str())]), "{role}: {two:#?}");
            for f in &two[layer] {
                let other = if f.focus == m1 { &m2 } else { &m1 };
                assert!(f.value.as_deref().is_some_and(|v| v.contains(other.as_str())), "{f:#?}");
            }
        }
    }
    // The DXF path: the leader as one DXF polyline and as two DXF lines gives the findings of the pixel path.
    for overshooting in [false, true] {
        let mut d = Drawing::default();
        let e = d.expression("technical_general", "4×M6");
        let t = d.text("4×M6", [12.0, 10.0], 3.5, 0.0);
        d.member_in(&e, &t, "note_text", "text");
        let pieces = if overshooting {
            vec![d.line([0.0, 0.0], [k, k]), d.line([9.0, 10.0], [25.0, 10.0])]
        } else {
            vec![d.polyline(&[[0.0, 0.0], [10.0, 10.0], [25.0, 10.0]])]
        };
        for p in &pieces {
            d.member_in(&e, p, "leader_line", "thin_continuous");
        }
        let by_dxf = d.findings();
        assert_eq!(by_dxf, drafting::evaluate_pixel_graph(&d.pixel_graph()).unwrap());
        assert_eq!(by_dxf["source"].len(), if overshooting { 2 } else { 0 }, "{by_dxf:#?}");
    }
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

/// Paper lengths go through the sheet scale. The centre line of the G1-021 fixtures runs 0.3 units (violating) or
/// 3 units (conforming) past the outline; on a sheet enlarged five times the 0.3 units are 1.5 mm of paper and
/// conform, on a sheet reduced to a quarter the 3 units are 0.75 mm and do not. With the scale unknown the rule does
/// not apply: no finding, its targets counted as not applicable with the reason.
#[test]
fn paper_lengths_go_through_the_sheet_scale_and_an_unknown_scale_leaves_the_rule_not_applicable() {
    let rule = "G1-021-centre-line-beyond-outline";
    let fired = |d: &Drawing| d.rule_ids().contains(rule);
    let mut d = fixture(rule, true);
    assert!(fired(&d), "0.3 units at one paper millimetre per unit");
    d.sheet = Some(5.0);
    assert!(!fired(&d), "0.3 units on a sheet enlarged five times are 1.5 mm of paper");
    let mut d = fixture(rule, false);
    assert!(!fired(&d), "3 units at one paper millimetre per unit");
    d.sheet = Some(0.25);
    assert!(fired(&d), "3 units on a sheet reduced to a quarter are 0.75 mm of paper");
    for violating in [true, false] {
        let mut d = fixture(rule, violating);
        d.sheet_unknown = true;
        let (found, application) = drafting::evaluate_with_application(&d.graph(), Some(&d.dxf())).unwrap();
        assert!(found.values().flatten().all(|f| f.rule_id != rule), "{found:?}");
        let rows: Vec<_> = application.iter().filter(|a| a.rule_id == rule).collect();
        assert_eq!(rows.len(), LAYERS.len());
        for a in rows {
            assert!(a.targets > 0, "{a:?}");
            assert_eq!(a.not_applicable, a.targets, "{a:?}");
            assert_eq!(a.reasons, vec!["sheet_scale:unknown".to_owned()]);
            assert_eq!(a.findings, 0);
            assert_eq!(a.measured, Some(0), "no paper measure is taken without the scale");
        }
        // Rules needing no condition still apply on the same drawing.
        assert!(application.iter().filter(|a| a.rule_id != rule && !a.reasons.is_empty()).all(|a| {
            drafting::rules(&drafting::knowledge().unwrap().shapes).unwrap().iter().any(|r| r.id == a.rule_id && !r.requires.is_empty())
        }));
    }
}

/// The counts never claim more than happened: findings are at most the measured targets, measured at most the
/// targets, and a centre line that meets no outline is a target without a measure instead of a silent pass.
#[test]
fn application_counts_are_consistent_and_show_targets_without_a_measure() {
    let rule = "G1-021-centre-line-beyond-outline";
    let mut d = fixture(rule, false);
    // A second centre line far from every outline.
    let c = d.line([100.0, 50.0], [140.0, 50.0]);
    d.member(&c, "center_line", "thin_dash_dot", "center_axis", "");
    let (_, application) = drafting::evaluate_with_application(&d.graph(), Some(&d.dxf())).unwrap();
    for a in &application {
        assert!(a.findings <= a.targets && a.not_applicable <= a.targets, "{a:?}");
        if let Some(m) = a.measured {
            assert!(m <= a.targets && (a.findings <= m || a.rule_id != rule), "{a:?}");
        }
    }
    let source = application.iter().find(|a| a.rule_id == rule && a.layer == "source").unwrap();
    assert_eq!(source.targets, 2, "{source:?}");
    assert_eq!(source.measured, Some(1), "the far centre line meets no outline: {source:?}");
    assert_eq!(source.findings, 0);
}
