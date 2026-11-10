//! What a reading states above the single objects (new-drawings correction round, annotation
//! workstream): the direction, spacing and neighbours of a section hatch and its name; what a
//! thread convention is called and whether it is an internal or an external thread; how a view
//! was obtained apart from its place in the arrangement; which view a cutting symbol names; the
//! sheets of a file; and whether a sheet is an assembly drawing. Fixtures are authored from the
//! card knowledge (GB/T 4457.5, 17453, 4459.1, 4458.6, 17452, 4458.2, 10609.2); handles,
//! coordinates, letters, numbers and names are invented and differ from any project drawing.
use oe_contracts::{MdDrawingView, MdEngineeringExpression, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::discover;
use std::collections::BTreeSet;

type P = [f64; 2];

fn ent(kind: &str, id: &str, layer: &str, tags: &str) -> String {
    format!("0\n{kind}\n5\n{id}\n8\n{layer}\n330\nMODEL\n{tags}")
}
fn line(id: &str, layer: &str, a: P, b: P) -> String {
    ent(
        "LINE",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]),
    )
}
/// A hatch with one closed polyline boundary and a pattern of line families given as (direction in
/// degrees, spacing); `name` is the pattern name the CAD system stores.
fn hatch(id: &str, layer: &str, name: &str, pts: &[P], families: &[(f64, f64)]) -> String {
    let mut t = format!(
        "10\n0\n20\n0\n30\n0\n2\n{name}\n70\n0\n71\n0\n91\n1\n92\n2\n72\n0\n73\n1\n93\n{}\n",
        pts.len()
    );
    for p in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
    }
    t.push_str(&format!(
        "97\n0\n75\n0\n76\n1\n52\n0\n41\n1\n77\n0\n78\n{}\n",
        families.len()
    ));
    for (a, s) in families {
        let (sin, cos) = a.to_radians().sin_cos();
        t.push_str(&format!(
            "53\n{a}\n43\n0\n44\n0\n45\n{}\n46\n{}\n79\n0\n",
            -sin * s,
            cos * s
        ));
    }
    t.push_str("98\n0\n");
    ent("HATCH", id, layer, &t)
}
/// A solid-filled hatch with one closed polyline boundary.
fn solid_hatch(id: &str, layer: &str, pts: &[P]) -> String {
    let mut t = format!(
        "10\n0\n20\n0\n30\n0\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n2\n72\n0\n73\n1\n93\n{}\n",
        pts.len()
    );
    for p in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
    }
    t.push_str("97\n0\n75\n0\n76\n1\n98\n0\n");
    ent("HATCH", id, layer, &t)
}
fn tables() -> String {
    let mut s = String::from("0\nSECTION\n2\nTABLES\n");
    for (name, el) in [
        ("CONTINUOUS", vec![]),
        ("DASHX", vec![2., -1.]),
        ("AXISX", vec![8., -1., 1., -1.]),
    ] {
        s.push_str(&format!("0\nLTYPE\n2\n{name}\n73\n{}\n", el.len()));
        for e in el {
            s.push_str(&format!("49\n{e}\n"));
        }
    }
    for (name, color, lt) in [
        ("0", 7, "CONTINUOUS"),
        ("01", 7, "CONTINUOUS"),
        ("02", 3, "CONTINUOUS"),
        ("04", 2, "DASHX"),
        ("05", 1, "AXISX"),
        ("08", 3, "CONTINUOUS"),
        ("10", 3, "CONTINUOUS"),
        ("11", 3, "CONTINUOUS"),
    ] {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n62\n{color}\n6\n{lt}\n"));
    }
    s.push_str("0\nSTYLE\n2\nS\n3\ntxt.shx\n0\nENDSEC\n");
    s
}
fn blocks(defs: &[(&str, Vec<String>)]) -> String {
    let mut s = String::from("0\nSECTION\n2\nBLOCKS\n");
    for (name, ents) in defs {
        s.push_str(&format!(
            "0\nBLOCK\n2\n{name}\n10\n0\n20\n0\n{}0\nENDBLK\n",
            ents.concat()
        ));
    }
    s.push_str("0\nENDSEC\n");
    s
}
fn run(defs: &[(&str, Vec<String>)], ents: &[String]) -> MdEngineeringExpressionGraph {
    let bytes = format!(
        "{}{}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        tables(),
        blocks(defs),
        ents.concat()
    );
    match discover(bytes.as_bytes()) {
        Ok(g) => g,
        Err(e) => panic!("{e:#}"),
    }
}
fn exprs<'a>(g: &'a MdEngineeringExpressionGraph, src: &str) -> Vec<&'a MdEngineeringExpression> {
    let ids: BTreeSet<&str> = g
        .members
        .iter()
        .filter(|m| m.source_handle == src)
        .map(|m| m.expression_id.as_str())
        .collect();
    g.expressions
        .iter()
        .filter(|e| ids.contains(e.id.as_str()))
        .collect()
}
fn expr_with<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
    kind: &str,
) -> &'a MdEngineeringExpression {
    exprs(g, src)
        .into_iter()
        .find(|e| e.kind == kind)
        .unwrap_or_else(|| {
            panic!(
                "{src}: no {kind} in {:?}",
                exprs(g, src)
                    .iter()
                    .map(|e| (&e.kind, &e.label))
                    .collect::<Vec<_>>()
            )
        })
}
fn ev(e: &MdEngineeringExpression, x: &str) -> bool {
    e.evidence_ids.iter().any(|v| v == x)
}
fn ev_with<'a>(e: &'a MdEngineeringExpression, prefix: &str) -> Vec<&'a str> {
    e.evidence_ids
        .iter()
        .filter_map(|v| v.strip_prefix(prefix))
        .collect()
}
#[allow(dead_code)]
fn view<'a>(g: &'a MdEngineeringExpressionGraph, id: &str) -> &'a MdDrawingView {
    g.views.iter().find(|v| v.id == id).unwrap_or_else(|| {
        panic!(
            "{id}: {:?}",
            g.views.iter().map(|v| &v.id).collect::<Vec<_>>()
        )
    })
}
/// A closed rectangle of thick outline on layer 01.
fn outline(prefix: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<String> {
    vec![
        line(&format!("{prefix}1"), "01", [x0, y0], [x1, y0]),
        line(&format!("{prefix}2"), "01", [x1, y0], [x1, y1]),
        line(&format!("{prefix}3"), "01", [x1, y1], [x0, y1]),
        line(&format!("{prefix}4"), "01", [x0, y1], [x0, y0]),
    ]
}
fn rect_pts(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<P> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

// ---------------------------------------------------------------- section_line_region: signature and neighbours

#[test]
fn hatch_states_the_direction_and_spacing_of_its_pattern() {
    let mut ents = outline("P", 0., 0., 50., 30.);
    ents.push(hatch(
        "HA",
        "10",
        "QQPAT",
        &rect_pts(0., 0., 50., 30.),
        &[(60., 2.5)],
    ));
    let g = run(&[], &ents);
    let e = expr_with(&g, "HA", "section_hatch");
    assert_eq!(e.label, "主视图 · 剖面线 60° 间距 2.5");
    assert!(
        ev(e, "hatch_angle:60") && ev(e, "hatch_spacing:2.5") && ev(e, "pattern_name:QQPAT"),
        "{:?}",
        e.evidence_ids
    );
    assert!(ev(e, "card:section_line_region") && ev(e, "variant:pattern_signature"));
    // a direction beyond a half turn is the same line direction; two families are both named
    let mut ents = outline("P", 0., 0., 50., 30.);
    ents.push(hatch(
        "HB",
        "10",
        "QQPAT",
        &rect_pts(0., 0., 25., 30.),
        &[(300., 4.)],
    ));
    ents.push(hatch(
        "HC",
        "10",
        "QQNET",
        &rect_pts(30., 0., 50., 30.),
        &[(45., 2.), (135., 2.)],
    ));
    let g = run(&[], &ents);
    assert_eq!(
        expr_with(&g, "HB", "section_hatch").label,
        "主视图 · 剖面线 120° 间距 4"
    );
    let c = expr_with(&g, "HC", "section_hatch");
    assert_eq!(c.label, "主视图 · 剖面线 45°/135° 间距 2");
    assert_eq!(ev_with(c, "hatch_family:"), vec!["45:2", "135:2"]);
}

#[test]
fn loose_section_lines_state_their_own_direction_and_spacing() {
    // four thin lines at 45° whose offsets step by 6 across their direction
    let mut ents = outline("Q", 0., 0., 60., 60.);
    for (i, o) in [5., 13.485, 21.971, 30.456].iter().enumerate() {
        ents.push(line(
            &format!("L{i}"),
            "02",
            [*o, 0.],
            [o + (60. - o), 60. - o],
        ));
    }
    let g = run(&[], &ents);
    let e = expr_with(&g, "L0", "section_hatch");
    assert_eq!(e.label, "主视图 · 剖面线 45° 间距 6");
    assert!(
        ev(e, "hatch_angle:45") && ev(e, "hatch_spacing:6"),
        "{:?}",
        e.evidence_ids
    );
}

#[test]
fn hatch_regions_sharing_a_boundary_name_each_other() {
    let mut ents = outline("P", 0., 0., 60., 30.);
    ents.push(line("PM", "01", [35., 0.], [35., 30.]));
    ents.push(hatch(
        "HL",
        "10",
        "QQPAT",
        &rect_pts(0., 0., 35., 30.),
        &[(45., 3.)],
    ));
    ents.push(hatch(
        "HR",
        "10",
        "QQPAT",
        &rect_pts(35., 0., 60., 30.),
        &[(135., 3.)],
    ));
    let g = run(&[], &ents);
    let (l, r) = (
        expr_with(&g, "HL", "section_hatch"),
        expr_with(&g, "HR", "section_hatch"),
    );
    assert_eq!(ev_with(l, "adjacent:"), vec![r.id.as_str()]);
    assert_eq!(ev_with(r, "adjacent:"), vec![l.id.as_str()]);
    assert!(ev(l, "variant:shared_boundary"));
}

#[test]
fn separate_hatch_regions_are_not_neighbours() {
    // two regions of one view a gap apart; a corner touching in one point is no shared line either
    let mut ents = outline("P", 0., 0., 60., 30.);
    ents.push(hatch(
        "HL",
        "10",
        "QQPAT",
        &rect_pts(0., 0., 20., 15.),
        &[(45., 3.)],
    ));
    ents.push(hatch(
        "HR",
        "10",
        "QQPAT",
        &rect_pts(30., 0., 60., 30.),
        &[(135., 3.)],
    ));
    ents.push(hatch(
        "HK",
        "10",
        "QQPAT",
        &rect_pts(20., 15., 28., 30.),
        &[(45., 3.)],
    ));
    let g = run(&[], &ents);
    for h in ["HL", "HR", "HK"] {
        let e = expr_with(&g, h, "section_hatch");
        assert!(ev_with(e, "adjacent:").is_empty(), "{h}: {:?}", e.evidence_ids);
    }
}

#[test]
fn solid_filled_section_states_no_direction() {
    let mut ents = outline("P", 0., 0., 60., 30.);
    ents.push(solid_hatch("HS", "10", &rect_pts(10., 10., 50., 11.5)));
    let g = run(&[], &ents);
    let e = expr_with(&g, "HS", "section_hatch");
    assert!(
        !e.label.contains('°') && e.label.contains("涂黑") && e.label != "SOLID",
        "{}",
        e.label
    );
    assert!(ev(e, "hatch_solid") && ev_with(e, "hatch_angle:").is_empty());
}
