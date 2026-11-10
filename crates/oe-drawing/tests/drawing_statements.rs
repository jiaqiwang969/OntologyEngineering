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
fn circle(id: &str, layer: &str, c: P, r: f64) -> String {
    ent(
        "CIRCLE",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]),
    )
}
fn arc(id: &str, layer: &str, c: P, r: f64, a0: f64, a1: f64) -> String {
    ent(
        "ARC",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]),
    )
}
fn text(id: &str, layer: &str, s: &str, p: P, h: f64) -> String {
    ent(
        "TEXT",
        id,
        layer,
        &format!("1\n{s}\n10\n{}\n20\n{}\n40\n{h}\n50\n0\n", p[0], p[1]),
    )
}
/// A line with its own line weight (1/100 mm).
fn line_w(id: &str, layer: &str, w: i32, a: P, b: P) -> String {
    ent(
        "LINE",
        id,
        layer,
        &format!(
            "370\n{w}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
            a[0], a[1], b[0], b[1]
        ),
    )
}
/// MTEXT with an attachment point (5 middle centre, 8 bottom centre, 4 middle left).
fn mtext(id: &str, layer: &str, s: &str, p: P, h: f64, attach: i32) -> String {
    ent(
        "MTEXT",
        id,
        layer,
        &format!(
            "10\n{}\n20\n{}\n40\n{h}\n71\n{attach}\n1\n{s}\n7\nS\n",
            p[0], p[1]
        ),
    )
}
/// A filled arrowhead drawn as a SOLID.
fn solid(id: &str, layer: &str, tip: P, base: P) -> String {
    let d = [base[0] - tip[0], base[1] - tip[1]];
    let p = [-d[1] * 0.15, d[0] * 0.15];
    let (b1, b2) = (
        [base[0] + p[0], base[1] + p[1]],
        [base[0] - p[0], base[1] - p[1]],
    );
    ent(
        "SOLID",
        id,
        layer,
        &format!(
            "10\n{}\n20\n{}\n11\n{}\n21\n{}\n12\n{}\n22\n{}\n13\n{}\n23\n{}\n",
            tip[0], tip[1], b1[0], b1[1], b2[0], b2[1], b2[0], b2[1]
        ),
    )
}
/// A solid-filled HATCH triangle (an arrowhead inside a block).
fn head(id: &str, layer: &str, tip: P, base: P, half: f64) -> String {
    let d = [base[0] - tip[0], base[1] - tip[1]];
    let n = d[0].hypot(d[1]);
    let p = [-d[1] / n * half, d[0] / n * half];
    let (b1, b2) = (
        [base[0] + p[0], base[1] + p[1]],
        [base[0] - p[0], base[1] - p[1]],
    );
    ent(
        "HATCH",
        id,
        layer,
        &format!(
            "10\n0\n20\n0\n30\n0\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n2\n72\n0\n73\n1\n93\n3\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n97\n0\n75\n0\n76\n1\n98\n0\n",
            tip[0], tip[1], b1[0], b1[1], b2[0], b2[1]
        ),
    )
}
fn lwpoly(id: &str, layer: &str, pts: &[P]) -> String {
    let mut t = format!("90\n{}\n70\n0\n", pts.len());
    for p in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
    }
    ent("LWPOLYLINE", id, layer, &t)
}
fn insert(id: &str, layer: &str, name: &str, at: P) -> String {
    ent(
        "INSERT",
        id,
        layer,
        &format!(
            "2\n{name}\n10\n{}\n20\n{}\n41\n1\n42\n1\n50\n0\n",
            at[0], at[1]
        ),
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
    // the hatch angle is the direction of the first family, as in a pattern given by angle and spacing
    t.push_str(&format!(
        "97\n0\n75\n0\n76\n1\n52\n{}\n41\n1\n77\n0\n78\n{}\n",
        families.first().map_or(0., |f| f.0),
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

// ---------------------------------------------------------------- thread_representation: names, forms, one thread in two views

fn concepts(e: &MdEngineeringExpression) -> BTreeSet<&str> {
    e.concept_ids.iter().map(String::as_str).collect()
}
fn view_label<'a>(g: &'a MdEngineeringExpressionGraph, e: &MdEngineeringExpression) -> &'a str {
    g.views
        .iter()
        .find(|v| Some(&v.id) == e.view_id.as_ref())
        .map_or("", |v| v.label.as_str())
}

#[test]
fn thread_end_view_with_the_thin_arc_outside_is_internal() {
    let mut ents = outline("P", 0., 0., 60., 40.);
    ents.push(arc("TA", "02", [20., 20.], 6., 90., 0.));
    ents.push(circle("TC", "01", [20., 20.], 5.1));
    let g = run(&[], &ents);
    let e = expr_with(&g, "TA", "thread_convention");
    assert_eq!(e.label, "主视图 · 内螺纹（端视） 大径 12 小径 10.2");
    assert_eq!(e.qualifiers, vec!["internal_thread".to_string()]);
    assert!(
        concepts(e).contains("DC-G3-005") && !concepts(e).contains("DC-G3-024"),
        "{:?}",
        e.concept_ids
    );
    assert!(ev(e, "card:thread_representation") && ev(e, "variant:internal_thread_end_view"));
    // the thick circle is the minor diameter of the same expression
    assert_eq!(expr_with(&g, "TC", "thread_convention").id, e.id);
}

#[test]
fn thread_end_view_with_the_thin_arc_inside_is_external() {
    // an external thread end view and, in the same view, an internal one: two groups
    let mut ents = outline("P", 0., 0., 60., 40.);
    ents.push(arc("XA", "02", [20., 20.], 5.1, 90., 0.));
    ents.push(circle("XC", "01", [20., 20.], 6.));
    ents.push(arc("TA", "02", [45., 20.], 4., 90., 0.));
    ents.push(circle("TC", "01", [45., 20.], 3.4));
    let g = run(&[], &ents);
    let x = expr_with(&g, "XA", "thread_convention");
    assert_eq!(x.label, "主视图 · 外螺纹（端视） 大径 12 小径 10.2");
    assert_eq!(x.qualifiers, vec!["external_thread".to_string()]);
    assert!(
        concepts(x).contains("DC-G3-024") && !concepts(x).contains("DC-G3-005"),
        "{:?}",
        x.concept_ids
    );
    assert_eq!(expr_with(&g, "XC", "thread_convention").id, x.id);
    let t = expr_with(&g, "TA", "thread_convention");
    assert_ne!(t.id, x.id);
    assert!(
        t.label.contains("内螺纹（端视）") && !concepts(t).contains("DC-G3-024"),
        "{} {:?}",
        t.label,
        t.concept_ids
    );
}

/// A tapped hole in section along x from the face at `x0`: thin pair `major` apart over `thread`,
/// thick pair `minor` apart over `drill`, the axis on `y`; ids prefixed with `p`.
fn tapped(p: &str, x0: f64, y: f64, major: f64, minor: f64, thread: f64, drill: f64) -> Vec<String> {
    let id = |s: &str| format!("{p}{s}");
    vec![
        line(&id("M1"), "02", [x0, y + major / 2.], [x0 + thread, y + major / 2.]),
        line(&id("M2"), "02", [x0, y - major / 2.], [x0 + thread, y - major / 2.]),
        line(&id("N1"), "01", [x0, y + minor / 2.], [x0 + drill, y + minor / 2.]),
        line(&id("N2"), "01", [x0, y - minor / 2.], [x0 + drill, y - minor / 2.]),
        line(&id("AX"), "05", [x0 - 3., y], [x0 + drill + 3., y]),
    ]
}

#[test]
fn thread_in_section_is_named_by_form_and_sizes() {
    // through the wall between two faces
    let mut ents = vec![
        line("FW", "01", [0., 0.], [0., 60.]),
        line("FZ", "01", [16., 0.], [16., 60.]),
    ];
    ents.extend(tapped("A", 0., 30., 12., 10.2, 16., 16.));
    let g = run(&[], &ents);
    let e = expr_with(&g, "AM1", "thread_convention");
    assert_eq!(
        e.label,
        format!("{} · 内螺纹（剖视） 大径 12 小径 10.2 通孔", view_label(&g, e))
    );
    assert!(ev(e, "through_thread") && !ev(e, "blind_thread"));
    // ending in the material: a thread end line, the drill deeper with its bottom and point
    let mut ents = vec![line("FW", "01", [0., 0.], [0., 60.])];
    ents.extend(tapped("B", 0., 30., 9., 7.2, 11., 15.));
    ents.extend([
        line("BTE", "01", [11., 25.5], [11., 34.5]),
        line("BDB", "01", [15., 26.4], [15., 33.6]),
        line("BP1", "01", [15., 33.6], [17.078, 30.]),
        line("BP2", "01", [15., 26.4], [17.078, 30.]),
    ]);
    let g = run(&[], &ents);
    let e = expr_with(&g, "BM1", "thread_convention");
    assert_eq!(
        e.label,
        format!(
            "{} · 内螺纹（剖视） 大径 9 小径 7.2 盲孔 螺纹深 11",
            view_label(&g, e)
        )
    );
    assert!(
        ev(e, "blind_thread") && ev(e, "thread_depth:11.0000") && ev(e, "drill_depth:15.0000"),
        "{:?}",
        e.evidence_ids
    );
    for x in &g.expressions {
        assert!(
            !g.source.entities.iter().any(|s| s.id == x.label),
            "{} is named by a handle",
            x.id
        );
    }
}

/// A view with a tapped hole in section (axis on y = `y`) and, 140 to its right, a view showing
/// an end view of radius `r` at height 30.
fn section_and_end_view(y: f64, r: f64, extra: Vec<String>) -> MdEngineeringExpressionGraph {
    let mut ents = outline("S", 0., y - 30., 40., y + 30.);
    ents.push(line("SZ", "01", [16., y - 30.], [16., y + 30.]));
    ents.extend(tapped("S", 0., y, 12., 10.2, 16., 16.));
    ents.extend(outline("E", 140., 0., 200., 60.));
    ents.push(arc("EA", "02", [170., 30.], r, 90., 0.));
    ents.push(circle("EC", "01", [170., 30.], 0.85 * r));
    ents.extend(extra);
    run(&[], &ents)
}

#[test]
fn thread_end_view_on_the_axis_of_its_section_is_the_same_thread() {
    let g = section_and_end_view(30., 6., vec![]);
    let (s, e) = (
        expr_with(&g, "SM1", "thread_convention"),
        expr_with(&g, "EA", "thread_convention"),
    );
    assert_ne!(s.view_id, e.view_id);
    assert_eq!(ev_with(s, "same_thread_as:"), vec![e.id.as_str()]);
    assert_eq!(ev_with(e, "same_thread_as:"), vec![s.id.as_str()]);
    assert!(
        ev(s, "end_view_on_section_axis") && ev(s, "end_view:EA"),
        "{:?}",
        s.evidence_ids
    );
    assert!(ev(s, "variant:same_thread_in_two_views"));
}

#[test]
fn thread_end_view_on_the_cutting_line_of_its_section_is_the_same_thread() {
    // the section is placed away from the projection of the end view and named R-R; the cutting
    // symbol R stands on the vertical line through the end view
    let symbol = vec![
        line("K1", "01", [170., 64.], [170., 69.]),
        line("K2", "01", [170., -4.], [170., -9.]),
        text("R1", "11", "R", [172., 69.], 3.5),
        text("R2", "11", "R", [172., -12.], 3.5),
        text("RN", "11", "R-R", [12., 174.], 3.5),
    ];
    let g = section_and_end_view(140., 6., symbol);
    let (s, e) = (
        expr_with(&g, "SM1", "thread_convention"),
        expr_with(&g, "EA", "thread_convention"),
    );
    let k = expr_with(&g, "K1", "view_callout");
    assert!(
        ev(k, &format!("references_view:{}", s.view_id.as_deref().unwrap())),
        "{:?} {:?}",
        k.evidence_ids,
        s.view_id
    );
    assert_eq!(ev_with(s, "same_thread_as:"), vec![e.id.as_str()]);
    assert_eq!(
        ev_with(s, "end_view_on_cutting_line:"),
        vec![k.id.as_str()],
        "{:?}",
        s.evidence_ids
    );
    assert_eq!(ev_with(e, "same_thread_as:"), vec![s.id.as_str()]);
}

#[test]
fn thread_end_view_of_another_diameter_is_not_the_same_thread() {
    let g = section_and_end_view(30., 8., vec![]);
    let (s, e) = (
        expr_with(&g, "SM1", "thread_convention"),
        expr_with(&g, "EA", "thread_convention"),
    );
    assert!(ev_with(s, "same_thread_as:").is_empty(), "{:?}", s.evidence_ids);
    assert!(ev_with(e, "same_thread_as:").is_empty(), "{:?}", e.evidence_ids);
}

// ---------------------------------------------------------------- view_callout and section_view_type

fn vev(v: &MdDrawingView, x: &str) -> bool {
    v.evidence_ids.iter().any(|e| e == x)
}
fn view_of<'a>(g: &'a MdEngineeringExpressionGraph, src: &str) -> &'a MdDrawingView {
    let e = exprs(g, src)
        .into_iter()
        .find(|e| e.view_id.is_some())
        .unwrap_or_else(|| panic!("{src} has no view"));
    view(g, e.view_id.as_deref().unwrap())
}
fn referenced<'a>(e: &'a MdEngineeringExpression) -> Vec<&'a str> {
    ev_with(e, "references_view:")
}

/// Two round end views flanking a view titled `name` that holds a section hatch.
fn disc_views(name: &str) -> Vec<String> {
    let mut ents = Vec::new();
    for (p, x) in [("C", 40.), ("E", 220.)] {
        for (i, r) in [30., 20., 10.].iter().enumerate() {
            ents.push(circle(&format!("{p}{i}"), "01", [x, 40.], *r));
        }
    }
    ents.extend(outline("S", 120., 10., 140., 70.));
    ents.push(hatch(
        "SH",
        "10",
        "QQPAT",
        &rect_pts(120., 10., 140., 70.),
        &[(45., 2.)],
    ));
    ents.push(text("SN", "11", name, [124., 74.], 3.5));
    ents
}
/// A straight cutting symbol as one block: two thick strokes on x = 0, an arrow at each outer
/// end, the letter at both.
fn straight_symbol(letter: &str) -> Vec<String> {
    vec![
        line_w("K1", "08", 50, [0., 80.], [0., 72.]),
        line_w("K2", "08", 50, [0., 8.], [0., 0.]),
        line_w("K3", "08", 18, [0., 80.], [-12., 80.]),
        line_w("K4", "08", 18, [0., 0.], [-12., 0.]),
        head("K5", "08", [-12., 80.], [-6., 80.], 0.75),
        head("K6", "08", [-12., 0.], [-6., 0.], 0.75),
        mtext("K7", "08", letter, [-6., 85.], 5., 5),
        mtext("K8", "08", letter, [-6., -5.], 5., 5),
    ]
}

#[test]
fn cutting_symbol_names_the_section_in_the_principal_position() {
    let mut ents = disc_views("D-D");
    ents.push(insert("KP", "08", "*K1", [40., 0.]));
    let g = run(&[("*K1", straight_symbol("D"))], &ents);
    let p = view(&g, "view:principal");
    assert_eq!((p.kind.as_str(), p.label.as_str()), ("principal", "D-D"));
    let k = expr_with(&g, "KP", "view_callout");
    assert!(k.qualifiers.contains(&"cutting_plane".to_string()));
    assert_eq!(referenced(k), vec!["view:principal"]);
    // the view keeps its kind and says how it was obtained
    assert!(
        vev(p, "view_type:section")
            && vev(p, "section_name:D")
            && vev(p, &format!("cutting_symbol:{}", k.id)),
        "{:?}",
        p.evidence_ids
    );
    // a straight cut: no turn, so no statement about the cutting planes, and no half section
    assert!(
        !p.evidence_ids.iter().any(|x| x.starts_with("section_type:")),
        "{:?}",
        p.evidence_ids
    );
}

/// A viewing arrow with its letter drawn loose at `at`, pointing in +x; `stroke` adds the
/// cutting stroke across its tail.
fn loose_arrow(p: &str, letter: &str, at: P, stroke: bool) -> Vec<String> {
    let id = |s: &str| format!("{p}{s}");
    let mut v = vec![
        line(&id("S"), "02", at, [at[0] + 9.5, at[1]]),
        solid(&id("H"), "02", [at[0] + 12., at[1]], [at[0] + 9., at[1]]),
        text(&id("L"), "11", letter, [at[0] - 2.5, at[1] + 2.], 3.5),
    ];
    if stroke {
        v.push(line(&id("K"), "01", [at[0], at[1] - 6.], at));
    }
    v
}

#[test]
fn cutting_symbol_does_not_name_a_detail_view_of_its_letter() {
    // a detail view titled with the same letter and a scale; no section of that name
    let mut ents = outline("P", 0., 0., 60., 40.);
    ents.extend(loose_arrow("A", "D", [-20., 20.], true));
    ents.extend(outline("Z", 150., 0., 180., 20.));
    ents.push(text("ZT", "11", "D 4:1", [158., 24.], 3.5));
    let g = run(&[], &ents);
    assert_eq!(view_of(&g, "Z1").kind, "detail_view");
    let k = expr_with(&g, "AK", "view_callout");
    assert!(k.qualifiers.contains(&"cutting_plane".to_string()));
    assert!(referenced(k).is_empty(), "{:?}", k.evidence_ids);
}

#[test]
fn loose_arrow_with_a_stroke_is_a_cutting_symbol() {
    let mut ents = outline("P", 0., 0., 60., 40.);
    ents.extend(loose_arrow("A", "H", [-20., 20.], true));
    let g = run(&[], &ents);
    let k = expr_with(&g, "AL", "view_callout");
    assert_eq!(k.id, expr_with(&g, "AK", "view_callout").id);
    assert_eq!(k.qualifiers, vec!["cutting_plane".to_string()]);
    assert!(
        concepts(k).contains("DC-G2-043") && !concepts(k).contains("DC-G2-072"),
        "{:?}",
        k.concept_ids
    );
    assert!(ev(k, "variant:cutting_symbol_by_stroke"));
}

#[test]
fn loose_arrow_without_a_stroke_stays_a_view_direction() {
    let mut ents = outline("P", 0., 0., 60., 40.);
    ents.extend(loose_arrow("A", "H", [-20., 20.], false));
    let g = run(&[], &ents);
    let k = expr_with(&g, "AL", "view_callout");
    assert_eq!(k.qualifiers, vec!["view_direction".to_string()]);
    assert!(
        concepts(k).contains("DC-G2-072") && !concepts(k).contains("DC-G2-043"),
        "{:?}",
        k.concept_ids
    );
}

/// A view of thick outline titled `name` with a section hatch, at `x`.
fn titled_section(p: &str, name: &str, x: f64) -> Vec<String> {
    let mut v = outline(p, x, 80., x + 40., 120.);
    v.push(hatch(
        &format!("{p}H"),
        "10",
        "QQPAT",
        &rect_pts(x, 80., x + 40., 120.),
        &[(45., 2.)],
    ));
    v.push(text(&format!("{p}T"), "11", name, [x + 14., 124.], 3.5));
    v
}

#[test]
fn loose_cutting_path_with_an_oblique_turn_shows_intersecting_planes() {
    // a cutting path through a round view: start stroke with arrow and letter, a turn at the
    // centre from the oblique plane into the vertical one, an end stroke with its arrow
    let mut ents = vec![
        circle("C1", "01", [100., 100.], 40.),
        circle("C2", "01", [100., 100.], 20.),
        line("S1", "01", [50., 150.], [54.95, 145.05]),
        line("A1", "02", [50., 150.], [57.07, 157.07]),
        solid("H1", "02", [59.19, 159.19], [57.07, 157.07]),
        text("M1", "11", "M", [50., 156.], 3.5),
        line("T1", "01", [95.05, 104.95], [100., 100.]),
        line("T2", "01", [100., 100.], [100., 93.]),
        text("M2", "11", "M", [102., 101.], 3.5),
        line("S2", "01", [100., 55.], [100., 48.]),
        line("A2", "02", [100., 48.], [107., 48.]),
        solid("H2", "02", [109., 48.], [107., 48.]),
    ];
    ents.extend(titled_section("W", "M-M", 260.));
    let g = run(&[], &ents);
    let k = expr_with(&g, "T1", "view_callout");
    assert_eq!(k.id, expr_with(&g, "M1", "view_callout").id);
    assert!(k.qualifiers.contains(&"cutting_plane".to_string()));
    assert!(!k.qualifiers.contains(&"view_direction".to_string()));
    assert!(
        ev(k, "intersecting_cutting_planes") && ev(k, "turn_at_centre_of:C1"),
        "{:?}",
        k.evidence_ids
    );
    assert!(
        concepts(k).contains("DC-G2-016") && !concepts(k).contains("DC-G2-072"),
        "{:?}",
        k.concept_ids
    );
    let w = view_of(&g, "W1");
    assert_eq!(referenced(k), vec![w.id.as_str()]);
    assert!(
        vev(w, "section_type:intersecting_planes") && vev(w, "variant:by_intersecting_planes"),
        "{:?}",
        w.evidence_ids
    );
    assert_eq!(w.kind, "section_view");
}

#[test]
fn cutting_path_with_a_right_angle_turn_shows_parallel_planes() {
    // a stepped path: down on x = 40, across on y = 50, down on x = 80
    let mut ents = outline("P", 0., 0., 120., 80.);
    ents.extend([
        line("S1", "01", [40., 90.], [40., 84.]),
        line("A1", "02", [40., 90.], [50., 90.]),
        solid("H1", "02", [53., 90.], [50., 90.]),
        text("N1", "11", "N", [36., 92.], 3.5),
        line("T1", "01", [40., 56.], [40., 50.]),
        line("T2", "01", [40., 50.], [46., 50.]),
        text("N2", "11", "N", [35., 46.], 3.5),
        line("T3", "01", [74., 50.], [80., 50.]),
        line("T4", "01", [80., 50.], [80., 44.]),
        text("N3", "11", "N", [82., 52.], 3.5),
        line("S2", "01", [80., -4.], [80., -10.]),
        line("A2", "02", [80., -10.], [90., -10.]),
        solid("H2", "02", [93., -10.], [90., -10.]),
    ]);
    let mut section = outline("W", 260., 0., 300., 80.);
    section.push(hatch(
        "WH",
        "10",
        "QQPAT",
        &rect_pts(260., 0., 300., 80.),
        &[(45., 2.)],
    ));
    section.push(text("WT", "11", "N-N", [274., 84.], 3.5));
    ents.extend(section);
    let g = run(&[], &ents);
    let k = expr_with(&g, "S1", "view_callout");
    for s in ["T1", "T2", "T3", "T4", "S2"] {
        assert_eq!(expr_with(&g, s, "view_callout").id, k.id, "{s}");
    }
    assert!(
        ev(k, "parallel_cutting_planes") && !ev(k, "intersecting_cutting_planes"),
        "{:?}",
        k.evidence_ids
    );
    assert!(concepts(k).contains("DC-G2-015"), "{:?}", k.concept_ids);
    let w = view_of(&g, "W1");
    assert_eq!(referenced(k), vec![w.id.as_str()]);
    assert!(
        vev(w, "section_type:parallel_planes")
            && !vev(w, "section_type:intersecting_planes"),
        "{:?}",
        w.evidence_ids
    );
}

#[test]
fn block_cutting_symbol_with_an_oblique_turn_types_the_view_it_names() {
    // the path turns at the centre of the round view into an oblique plane (cos 0.6)
    let u = [0.8, -0.6];
    let at = |t: f64| [u[0] * t, u[1] * t];
    let block = vec![
        line_w("K1", "08", 50, [0., 60.], [0., 52.]),
        line_w("K2", "08", 50, [0., 8.], [0., 0.]),
        line_w("K3", "08", 50, [0., 0.], at(8.)),
        line_w("K4", "08", 50, at(50.), at(58.)),
        line_w("K5", "08", 18, [0., 60.], [12., 60.]),
        head("K6", "08", [12., 60.], [6., 60.], 0.75),
        line_w(
            "K7",
            "08",
            18,
            at(58.),
            [at(58.)[0] + 12. * 0.6, at(58.)[1] + 12. * 0.8],
        ),
        head(
            "K8",
            "08",
            [at(58.)[0] + 12. * 0.6, at(58.)[1] + 12. * 0.8],
            [at(58.)[0] + 6. * 0.6, at(58.)[1] + 6. * 0.8],
            0.75,
        ),
        mtext("K9", "08", "E", [6., 65.], 5., 5),
        mtext("KA", "08", "E", [-4., -4.], 5., 5),
        mtext("KB", "08", "E", [at(58.)[0] + 3., at(58.)[1] - 4.], 5., 5),
    ];
    let mut ents = vec![
        circle("C1", "01", [100., 100.], 45.),
        circle("C2", "01", [100., 100.], 20.),
        insert("KP", "08", "*K2", [100., 100.]),
    ];
    ents.extend(titled_section("W", "E-E", 260.));
    let g = run(&[("*K2", block)], &ents);
    let k = expr_with(&g, "KP", "view_callout");
    assert_eq!(k.rule, "card:view_callout/cutting_plane_group");
    assert!(
        ev(k, "intersecting_cutting_planes") && ev(k, "turn_at_centre_of:C1"),
        "{:?}",
        k.evidence_ids
    );
    let w = view_of(&g, "W1");
    assert_eq!(referenced(k), vec![w.id.as_str()]);
    assert!(
        vev(w, "section_type:intersecting_planes") && vev(w, "view_type:section"),
        "{:?}",
        w.evidence_ids
    );
}

/// A sleeve drawn along x with its centre line on y = 20: the outline, the bore line of the
/// upper half and section lines from x = `h0` to `h1` between the bore and the outside; `extra`
/// adds entities.
fn sleeve(h0: f64, h1: f64, extra: Vec<String>) -> MdEngineeringExpressionGraph {
    let mut ents = outline("P", 0., 0., 80., 40.);
    ents.push(line("PB", "01", [0., 28.], [80., 28.]));
    ents.push(line("AX", "05", [-5., 20.], [85., 20.]));
    ents.push(hatch(
        "HU",
        "10",
        "QQPAT",
        &rect_pts(h0, 28., h1, 40.),
        &[(45., 2.)],
    ));
    ents.extend(extra);
    run(&[], &ents)
}
fn half(g: &MdEngineeringExpressionGraph) -> bool {
    g.views.iter().any(|v| vev(v, "section_type:half"))
}

#[test]
fn section_lines_on_one_side_of_the_centre_line_make_a_half_section() {
    let g = sleeve(0., 80., vec![]);
    let p = view_of(&g, "P1");
    assert!(
        vev(p, "section_type:half")
            && vev(p, "view_type:section")
            && vev(p, "half_section_axis:AX")
            && vev(p, "card:section_view_type"),
        "{:?}",
        p.evidence_ids
    );
    assert_eq!(p.kind, "principal");
    let h = expr_with(&g, "HU", "section_hatch");
    assert!(
        concepts(h).contains("DC-G2-020") && ev(h, "variant:half_section"),
        "{:?} {:?}",
        h.concept_ids,
        h.evidence_ids
    );
}

#[test]
fn section_lines_on_both_sides_of_the_centre_line_are_not_a_half_section() {
    // the lower half is cut as well (its bore drawn a little off the mirror position)
    let g = sleeve(
        0.,
        80.,
        vec![
            line("PL", "01", [0., 14.], [80., 14.]),
            hatch(
                "HD",
                "10",
                "QQPAT",
                &rect_pts(0., 0., 80., 14.),
                &[(45., 2.)],
            ),
        ],
    );
    assert!(!half(&g), "{:?}", view_of(&g, "P1").evidence_ids);
}

#[test]
fn local_section_beside_the_centre_line_is_not_a_half_section() {
    // the cut ends at a wavy break line across the upper wall
    let mut wave = vec![[60., 28.]];
    for i in 1..8 {
        wave.push([if i % 2 == 0 { 59.2 } else { 60.8 }, 28. + 1.5 * f64::from(i)]);
    }
    wave.push([60., 40.]);
    let g = sleeve(0., 60., vec![lwpoly("WV", "02", &wave)]);
    assert!(
        exprs(&g, "WV").iter().any(|e| e.kind == "break_line"),
        "the wavy line is a break line"
    );
    assert!(!half(&g), "{:?}", view_of(&g, "P1").evidence_ids);
}

#[test]
fn short_hatch_beside_the_centre_line_is_not_a_half_section() {
    let g = sleeve(0., 20., vec![line("PV", "01", [20., 28.], [20., 40.])]);
    assert!(!half(&g), "{:?}", view_of(&g, "P1").evidence_ids);
}

#[test]
fn view_drawn_on_one_side_of_the_centre_line_is_not_a_half_section() {
    // only the upper half is drawn, open towards the centre line
    let ents = vec![
        line("P1", "01", [0., 40.], [80., 40.]),
        line("P2", "01", [0., 20.], [0., 40.]),
        line("P3", "01", [80., 20.], [80., 40.]),
        line("PB", "01", [0., 28.], [80., 28.]),
        line("AX", "05", [-5., 20.], [85., 20.]),
        hatch(
            "HU",
            "10",
            "QQPAT",
            &rect_pts(0., 28., 80., 40.),
            &[(45., 2.)],
        ),
    ];
    let g = run(&[], &ents);
    assert!(!half(&g), "{:?}", view_of(&g, "P1").evidence_ids);
}

/// A view with a loose cutting symbol `G` and a view titled in direction-view form, with or
/// without section lines.
fn direction_form(hatched: bool) -> MdEngineeringExpressionGraph {
    let mut ents = outline("P", 0., 0., 60., 40.);
    ents.extend(loose_arrow("A", "G", [-20., 20.], true));
    ents.extend(outline("W", 200., 0., 240., 40.));
    if hatched {
        ents.push(hatch(
            "WH",
            "10",
            "QQPAT",
            &rect_pts(200., 0., 240., 40.),
            &[(45., 2.)],
        ));
    }
    ents.push(text("WT", "11", "G向", [214., 44.], 3.5));
    run(&[], &ents)
}

#[test]
fn direction_view_with_section_lines_and_cutting_strokes_is_a_section() {
    let g = direction_form(true);
    let w = view_of(&g, "W1");
    let k = expr_with(&g, "AK", "view_callout");
    assert_eq!((w.kind.as_str(), w.label.as_str()), ("direction_view", "G向"));
    assert_eq!(referenced(k), vec![w.id.as_str()]);
    assert!(
        vev(w, "view_type:section")
            && vev(w, "variant:section_named_as_direction_view")
            && vev(w, &format!("cutting_symbol:{}", k.id)),
        "{:?}",
        w.evidence_ids
    );
}

#[test]
fn direction_view_without_section_lines_is_not_stated_a_section() {
    let g = direction_form(false);
    let w = view_of(&g, "W1");
    assert_eq!(w.kind, "direction_view");
    assert!(
        !vev(w, "view_type:section") && !w.evidence_ids.iter().any(|x| x.starts_with("section_type:")),
        "{:?}",
        w.evidence_ids
    );
}
