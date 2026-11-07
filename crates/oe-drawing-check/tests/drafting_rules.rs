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
    /// A member with its own expression; the role gives the class, the kind and label the recognition.
    fn member(&mut self, handle: &str, role: &str, line_type: &str, kind: &str, label: &str) {
        let e = format!("expression:{}", self.expressions.len());
        let m = format!("{e}:m0");
        self.expressions
            .push(json!({"id": e, "kind": kind, "label": label}));
        self.members.push(json!({"id": m, "source_handle": handle, "member_role": role, "line_type": line_type,
            "expression_id": e, "interval": [0.0, 1.0], "view_id": "view:main"}));
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
        other => panic!("no fixture for rule {other}"),
    }
    d
}

#[test]
fn every_rule_has_a_violating_and_a_conforming_fixture() {
    let k = drafting::knowledge().unwrap();
    let rules = drafting::rules(&k.shapes).unwrap();
    let ids: BTreeSet<String> = rules.iter().map(|r| r.id.clone()).collect();
    assert!(ids.len() >= 12, "{ids:?}");
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
