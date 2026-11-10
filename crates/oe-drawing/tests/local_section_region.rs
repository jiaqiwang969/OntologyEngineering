//! Local sections, outline gaps and arc continuation in the structure topology (correction round,
//! section workstream). Fixtures are authored from drafting knowledge — GB/T 17452-1998 6.3 and
//! GB/T 4458.6-2002 5.6 for local sections, GB/T 4458.4-2003 4.3.4 for lines kept clear of the
//! number, dimension line and terminators — never from a real drawing: handles, coordinates and
//! labels are invented; layer names are the GB/T 14665 table 6 identifiers.
use oe_contracts::{MdEngineeringExpressionGraph, MdImpliedSpan, MdStructuralLine};
use oe_drawing::engineering_expression::discover;

fn ent(kind: &str, id: &str, layer: &str, tags: &str) -> String {
    format!("0\n{kind}\n5\n{id}\n8\n{layer}\n330\nMODEL\n{tags}")
}
fn line(id: &str, layer: &str, a: [f64; 2], b: [f64; 2]) -> String {
    ent(
        "LINE",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]),
    )
}
fn arc(id: &str, layer: &str, c: [f64; 2], r: f64, a0: f64, a1: f64) -> String {
    ent(
        "ARC",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]),
    )
}
fn circle(id: &str, layer: &str, c: [f64; 2], r: f64) -> String {
    ent(
        "CIRCLE",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]),
    )
}
fn text(id: &str, layer: &str, s: &str, p: [f64; 2], h: f64, rot: f64) -> String {
    ent(
        "TEXT",
        id,
        layer,
        &format!("1\n{s}\n10\n{}\n20\n{}\n40\n{h}\n50\n{rot}\n", p[0], p[1]),
    )
}
fn solid(id: &str, layer: &str, tip: [f64; 2], base: [f64; 2]) -> String {
    let d = [base[0] - tip[0], base[1] - tip[1]];
    let p = [-d[1] * 0.15, d[0] * 0.15];
    ent(
        "SOLID",
        id,
        layer,
        &format!(
            "10\n{}\n20\n{}\n11\n{}\n21\n{}\n12\n{}\n22\n{}\n13\n{}\n23\n{}\n",
            tip[0],
            tip[1],
            base[0] + p[0],
            base[1] + p[1],
            base[0] - p[0],
            base[1] - p[1],
            base[0] - p[0],
            base[1] - p[1]
        ),
    )
}
fn lwpoly(id: &str, layer: &str, pts: &[[f64; 2]]) -> String {
    let mut t = format!("90\n{}\n70\n0\n", pts.len());
    for p in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
    }
    ent("LWPOLYLINE", id, layer, &t)
}
/// A hatch whose one boundary path is the closed polygon `pts`.
fn hatch(id: &str, pts: &[[f64; 2]]) -> String {
    let mut t = format!(
        "10\n0\n20\n0\n30\n0\n2\nANSI31\n70\n0\n71\n0\n91\n1\n92\n2\n72\n0\n73\n1\n93\n{}\n",
        pts.len()
    );
    for q in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", q[0], q[1]));
    }
    t.push_str("97\n0\n75\n0\n76\n1\n98\n0\n");
    ent("HATCH", id, "10", &t)
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
        ("11", 3, "CONTINUOUS"),
    ] {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n62\n{color}\n6\n{lt}\n"));
    }
    s.push_str("0\nENDSEC\n");
    s
}
fn run_with(blocks: &str, ents: &[String]) -> MdEngineeringExpressionGraph {
    let mut all = ents.to_vec();
    // a text far away fixes the text height (3.5) the ratios of the cards refer to
    all.push(text("T0", "11", "scale reference", [0., -40.], 3.5, 0.));
    let bytes = format!(
        "{}{blocks}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        tables(),
        all.concat()
    );
    discover(bytes.as_bytes()).unwrap()
}
fn run(ents: &[String]) -> MdEngineeringExpressionGraph {
    run_with("", ents)
}

/// A freehand curve from `a` to `b`: a small wave about the chord, through `a` and `b` and
/// crossing the chord every quarter of `wave` along it.
fn wavy(a: [f64; 2], b: [f64; 2], wave: f64) -> Vec<[f64; 2]> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx.hypot(dy);
    let (u, n) = ([dx / l, dy / l], [-dy / l, dx / l]);
    let steps = ((l / (wave / 10.)).round() as usize).max(8);
    (0..=steps)
        .map(|i| {
            let s = l * i as f64 / steps as f64;
            let w = 0.4 * (std::f64::consts::TAU * s / wave).sin();
            if i == steps {
                b
            } else {
                [a[0] + u[0] * s + n[0] * w, a[1] + u[1] * s + n[1] * w]
            }
        })
        .collect()
}

/// Evidence of the members of `src` that say it exists only because of a local section.
fn section_marks(g: &MdEngineeringExpressionGraph, src: &str) -> Vec<String> {
    g.members
        .iter()
        .filter(|m| m.source_handle == src)
        .filter(|m| {
            m.evidence_ids
                .iter()
                .any(|x| x.starts_with("section_profile:"))
        })
        .flat_map(|m| m.evidence_ids.iter().cloned())
        .collect()
}
fn marked(g: &MdEngineeringExpressionGraph, src: &str) -> bool {
    !section_marks(g, src).is_empty()
}
fn implied_between<'a>(
    g: &'a MdEngineeringExpressionGraph,
    a: &str,
    b: &str,
) -> Vec<&'a MdImpliedSpan> {
    g.implied_spans
        .iter()
        .filter(|s| {
            (s.from_source_handle == a && s.to_source_handle == b)
                || (s.from_source_handle == b && s.to_source_handle == a)
        })
        .collect()
}
/// Structural lines holding a member of `src`.
fn line_of<'a>(g: &'a MdEngineeringExpressionGraph, src: &str) -> Vec<&'a MdStructuralLine> {
    let ids: std::collections::BTreeSet<String> = g
        .members
        .iter()
        .filter(|m| m.source_handle == src)
        .map(|m| format!("member:{}", m.id))
        .collect();
    g.structural_lines
        .iter()
        .filter(|l| {
            l.pieces
                .iter()
                .chain(&l.coincident)
                .any(|p| ids.contains(p))
        })
        .collect()
}
fn kinds(g: &MdEngineeringExpressionGraph, src: &str) -> Vec<String> {
    let ids: std::collections::BTreeSet<&str> = g
        .members
        .iter()
        .filter(|m| m.source_handle == src)
        .map(|m| m.expression_id.as_str())
        .collect();
    g.expressions
        .iter()
        .filter(|e| ids.contains(e.id.as_str()))
        .map(|e| e.kind.clone())
        .collect()
}
fn no_self_or_long_arc_span(g: &MdEngineeringExpressionGraph) {
    for s in &g.implied_spans {
        assert!(
            s.from_source_handle != s.to_source_handle
                && s.arc
                    .as_ref()
                    .is_none_or(|a| a.len() == 5 && a[4] <= std::f64::consts::PI),
            "{s:?}"
        );
    }
}

/// A plate 40 × 20 whose bottom edge is opened by a blind hole (walls W1, W2, bottom WB) laid
/// open by a local section: a freehand curve from the bottom edge round the hole and back to it.
fn bay(extra: Vec<String>) -> Vec<String> {
    let curve = [
        [12., 0.],
        [12.6, 2.4],
        [14.4, 4.6],
        [17., 6.1],
        [20., 6.6],
        [23., 6.1],
        [25.6, 4.6],
        [27.4, 2.4],
        [28., 0.],
    ];
    let mut region: Vec<[f64; 2]> = curve.to_vec();
    region.extend([[23., 0.], [23., 5.], [17., 5.], [17., 0.]]);
    let mut v = vec![
        line("L", "01", [0., 0.], [0., 20.]),
        line("T", "01", [0., 20.], [40., 20.]),
        line("R", "01", [40., 20.], [40., 0.]),
        line("B1", "01", [0., 0.], [17., 0.]),
        line("B2", "01", [23., 0.], [40., 0.]),
        line("W1", "01", [17., 0.], [17., 5.]),
        line("W2", "01", [23., 0.], [23., 5.]),
        line("WB", "01", [17., 5.], [23., 5.]),
        lwpoly("C", "02", &curve),
        hatch("H", &region),
    ];
    v.extend(extra);
    v
}

#[test]
fn local_section_region_between_a_break_curve_and_one_outline() {
    let g = run(&bay(vec![]));
    assert!(
        kinds(&g, "C").contains(&"break_line".into()),
        "{:?}",
        kinds(&g, "C")
    );
    // the profile of the cut face, the hatch and the break curve exist only because of the cut
    for h in ["W1", "W2", "WB"] {
        let ev = section_marks(&g, h);
        assert!(
            ev.iter().any(|x| x == "variant:cut_face_profile"),
            "{h}: {ev:?}"
        );
    }
    assert!(marked(&g, "H"));
    let ev = section_marks(&g, "C");
    assert!(
        ev.iter().any(|x| x == "card:local_section_boundary")
            && ev.iter().any(|x| x == "variant:between_curve_ends"),
        "{ev:?}"
    );
    // the outline outside the region, and the edge the region's boundary runs along, are not
    for h in ["L", "T", "R", "B1", "B2"] {
        assert!(!marked(&g, h), "{h} marked");
    }
    // the bottom edge, opened by the hole only in the section, is one line in the complete structure
    let s = implied_between(&g, "B1", "B2");
    assert_eq!(s.len(), 1, "{:?}", g.implied_spans);
    assert_eq!(s[0].cause, "local_section");
    assert!(
        s[0].evidence_ids.iter().any(|x| x == "section:C"),
        "{:?}",
        s[0].evidence_ids
    );
    assert_eq!(line_of(&g, "B1")[0].id, line_of(&g, "B2")[0].id);
}

#[test]
fn outline_outside_the_region_is_never_section_only() {
    // a fillet beside the break curve, inside the extent of the hatch but outside the region
    let g = run(&bay(vec![arc("F", "01", [10.5, 4.5], 1.5, 0., 90.)]));
    assert!(kinds(&g, "F").contains(&"visible_contour".into()));
    assert!(!marked(&g, "F"), "{:?}", section_marks(&g, "F"));
    assert!(marked(&g, "WB"));
}

#[test]
fn feature_repeated_outside_the_region_is_not_section_only() {
    // two holes of one pattern, one of them inside the region; a smaller circle only there
    let g = run(&bay(vec![
        circle("K1", "01", [20., 2.5], 0.8),
        circle("K2", "01", [6., 10.], 0.8),
        circle("K3", "01", [25., 2.], 0.4),
    ]));
    assert!(!marked(&g, "K1"), "{:?}", section_marks(&g, "K1"));
    assert!(!marked(&g, "K2"));
    assert!(marked(&g, "K3"));
}

#[test]
fn view_named_as_a_section_holds_no_local_section() {
    let g = run(&bay(vec![text("TT", "11", "C-C", [17., 24.], 3.5, 0.)]));
    assert!(
        g.views.iter().any(|v| v.label == "C-C"),
        "{:?}",
        g.views
            .iter()
            .map(|v| (&v.id, &v.kind, &v.label))
            .collect::<Vec<_>>()
    );
    assert!(
        !g.members.iter().any(|m| m
            .evidence_ids
            .iter()
            .any(|x| x.starts_with("section_profile:"))),
        "section-only members in a section view"
    );
    assert!(!g.structure_ends.iter().any(|e| e.cause == "local_section"));
}

/// A plate 40 × 20 cut open between two freehand curves from the bottom to the top edge; a hidden
/// bore (H1, H2) is drawn thick (V) where the cut lays it open.
fn band(hatched: bool, extra: Vec<String>) -> Vec<String> {
    let c1 = wavy([14., 0.], [14., 20.], 5.);
    let c2 = wavy([24., 0.], [24., 20.], 5.);
    let mut region = c1.clone();
    region.extend(c2.iter().rev().copied());
    let mut v = vec![
        line("B", "01", [0., 0.], [40., 0.]),
        line("T", "01", [0., 20.], [40., 20.]),
        line("L", "01", [0., 0.], [0., 20.]),
        line("R", "01", [40., 0.], [40., 20.]),
        line("H1", "04", [0., 10.], [14., 10.]),
        line("H2", "04", [24., 10.], [40., 10.]),
        line("V", "01", [14., 10.], [24., 10.]),
        lwpoly("C1", "02", &c1),
        lwpoly("C2", "02", &c2),
    ];
    if hatched {
        v.push(hatch("H", &region));
    }
    v.extend(extra);
    v
}

#[test]
fn local_section_band_between_two_break_curves() {
    let g = run(&band(true, vec![]));
    for h in ["C1", "C2"] {
        let ev = section_marks(&g, h);
        assert!(
            ev.iter().any(|x| x == "variant:band_between_break_curves"),
            "{h}: {ev:?}"
        );
    }
    assert!(marked(&g, "H"));
    // the outline crossing the region in one line type is the outside view's
    for h in ["B", "T", "L", "R", "H1", "H2"] {
        assert!(!marked(&g, h), "{h} marked");
    }
}

#[test]
fn hidden_edge_laid_open_by_the_cut_is_one_hidden_line() {
    let g = run(&band(true, vec![]));
    let ev = section_marks(&g, "V");
    assert!(
        ev.iter().any(|x| x == "variant:revealed_hidden_edge"),
        "{ev:?}"
    );
    let s = implied_between(&g, "H1", "H2");
    assert_eq!(s.len(), 1, "{:?}", g.implied_spans);
    assert_eq!(
        (s[0].cause.as_str(), s[0].line_class.as_str()),
        ("local_section", "hidden_outline")
    );
    let l = line_of(&g, "H1");
    assert_eq!(l.len(), 1);
    assert_eq!(line_of(&g, "H2")[0].id, l[0].id);
    assert_eq!(l[0].line_type, "thin_dashed");
    assert!(line_of(&g, "V").is_empty(), "{:?}", line_of(&g, "V"));
}

#[test]
fn break_curves_without_hatch_bound_no_section() {
    // the limits of a partial or broken view: nothing is section-only, nothing goes on across
    let g = run(&band(false, vec![]));
    assert!(!g.members.iter().any(|m| {
        m.evidence_ids
            .iter()
            .any(|x| x.starts_with("section_profile:"))
    }),);
    assert!(implied_between(&g, "H1", "H2").is_empty());
    assert!(!g.structure_ends.iter().any(|e| e.cause == "local_section"));
}

#[test]
fn fillet_with_one_free_end_never_closes_into_a_circle() {
    // an 86° fillet whose straight continuation is missing, its free end inside the section
    let g = run(&band(
        true,
        vec![
            line("LH", "01", [5., 14.], [13.5, 14.]),
            arc("F", "01", [13.5, 15.], 1., 270., 356.),
        ],
    ));
    assert!(!marked(&g, "F"));
    assert!(
        implied_between(&g, "F", "F").is_empty(),
        "{:?}",
        g.implied_spans
    );
    no_self_or_long_arc_span(&g);
}

#[test]
fn section_broken_off_at_a_break_curve_is_no_local_section() {
    // a half disc broken off along a freehand curve: the outline stops at the curve
    let curve = wavy([28., 10.], [12., 10.], 4.);
    let mut region: Vec<[f64; 2]> = (0..=18)
        .map(|i| {
            let a = (i as f64 * 10.).to_radians();
            [20. + 8. * a.cos(), 10. + 8. * a.sin()]
        })
        .collect();
    region.extend(curve.iter().rev().skip(1).copied());
    let g = run(&[
        arc("A", "01", [20., 10.], 8., 0., 180.),
        lwpoly("C", "02", &curve),
        hatch("H", &region),
    ]);
    assert!(
        kinds(&g, "C").contains(&"break_line".into()),
        "{:?}",
        kinds(&g, "C")
    );
    assert!(!g.members.iter().any(|m| {
        m.evidence_ids
            .iter()
            .any(|x| x.starts_with("section_profile:"))
    }),);
}

#[test]
fn local_section_inside_a_detail_circle() {
    // a shoulder line G; the local section runs from G to the detail circle and the circle
    // limits it; S is a line of the cut face
    let curve = wavy(
        [30., 14.],
        [
            30. + 4. * 200f64.to_radians().cos(),
            12. + 4. * 200f64.to_radians().sin(),
        ],
        2.,
    );
    let mut region: Vec<[f64; 2]> = curve.clone();
    region.reverse();
    region.push([30., 8.]);
    for a in [260f64, 250., 240., 230., 220., 210.] {
        let r = a.to_radians();
        region.push([30. + 4. * r.cos(), 12. + 4. * r.sin()]);
    }
    let g = run(&[
        line("B", "01", [0., 0.], [40., 0.]),
        line("T", "01", [0., 20.], [40., 20.]),
        line("L", "01", [0., 0.], [0., 20.]),
        line("R", "01", [40., 0.], [40., 20.]),
        line("G", "01", [30., 0.], [30., 20.]),
        circle("D", "02", [30., 12.], 4.),
        text("DT", "11", "I", [35.5, 17.5], 3.5, 0.),
        line("S", "01", [27.6, 9.4], [29.4, 9.4]),
        lwpoly("C", "02", &curve),
        hatch("H", &region),
    ]);
    assert!(
        kinds(&g, "D").contains(&"view_callout".into()),
        "{:?}",
        kinds(&g, "D")
    );
    let ev = section_marks(&g, "C");
    assert!(
        ev.iter().any(|x| x == "variant:inside_detail_circle"),
        "{ev:?}"
    );
    assert!(marked(&g, "S") && marked(&g, "H"));
    for h in ["G", "D", "B", "T", "L", "R"] {
        assert!(!marked(&g, h), "{h} marked");
    }
}

#[test]
fn detail_indicator_circle_over_a_section_bounds_nothing() {
    // a full section (hatch bounded by outline only) with a detail-indicator circle over it and a
    // line broken under the circle: no local section, so no section-only member and no cause
    let g = run(&[
        line("B", "01", [0., 0.], [40., 0.]),
        line("T", "01", [0., 20.], [40., 20.]),
        line("L", "01", [0., 0.], [0., 20.]),
        line("R", "01", [40., 0.], [40., 20.]),
        line("P1", "01", [30., 0.], [30., 8.]),
        line("P2", "01", [30., 12.], [30., 20.]),
        hatch("H", &[[0., 0.], [40., 0.], [40., 20.], [0., 20.]]),
        circle("D", "02", [30., 10.], 5.),
        text("DT", "11", "II", [37., 17.], 3.5, 0.),
    ]);
    assert!(
        kinds(&g, "D").contains(&"view_callout".into()),
        "{:?}",
        kinds(&g, "D")
    );
    assert!(!g.members.iter().any(|m| {
        m.evidence_ids
            .iter()
            .any(|x| x.starts_with("section_profile:"))
    }),);
    assert!(!g.structure_ends.iter().any(|e| e.cause == "local_section"));
    assert!(implied_between(&g, "P1", "P2").is_empty());
}

// ---------------------------------------------------------------- annotation_break: dimension lines

/// A native linear dimension 0..30 along y = 40, terminators 2 long at both ends, the value
/// written above the dimension line from x = 12.
fn dimension(id: &str, block: &str) -> (String, String) {
    let children = [
        line(&format!("{id}1"), "0", [0., 40.], [30., 40.]),
        line(&format!("{id}2"), "0", [0., 30.], [0., 42.]),
        line(&format!("{id}3"), "0", [30., 30.], [30., 42.]),
        solid(&format!("{id}4"), "0", [0., 40.], [2., 40.]),
        solid(&format!("{id}5"), "0", [30., 40.], [28., 40.]),
        ent(
            "MTEXT",
            &format!("{id}T"),
            "0",
            "1\n30\n10\n12\n20\n41\n40\n2.5\n71\n7\n",
        ),
    ];
    let blk = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n{block}\n10\n0\n20\n0\n{}0\nENDBLK\n0\nENDSEC\n",
        children.concat()
    );
    let dim = ent(
        "DIMENSION",
        id,
        "0",
        &format!(
            "2\n{block}\n10\n30\n20\n40\n11\n15\n21\n41\n70\n33\n1\n<>\n13\n0\n23\n30\n14\n30\n24\n30\n42\n30\n50\n0\n"
        ),
    );
    (blk, dim)
}

#[test]
fn outline_broken_for_a_dimension_line_is_one_line() {
    let (blk, dim) = dimension("D", "*D1");
    let g = run_with(
        &blk,
        &[
            dim,
            // crossed by the dimension line
            line("P1", "01", [10., 25.], [10., 39.]),
            line("P2", "01", [10., 41.], [10., 55.]),
            // crossed by the body of a terminator next to its tip
            line("Q1", "01", [1.2, 25.], [1.2, 39.6]),
            line("Q2", "01", [1.2, 40.4], [1.2, 55.]),
        ],
    );
    for (a, b) in [("P1", "P2"), ("Q1", "Q2")] {
        let s = implied_between(&g, a, b);
        assert_eq!(s.len(), 1, "{a}-{b}: {:?}", g.implied_spans);
        assert_eq!(s[0].cause, "text_break");
        assert!(
            s[0].evidence_ids
                .iter()
                .any(|x| x == "card:annotation_break")
                && s[0]
                    .evidence_ids
                    .iter()
                    .any(|x| x == "variant:broken_for_dimension_line"),
            "{:?}",
            s[0].evidence_ids
        );
        assert_eq!(line_of(&g, a)[0].id, line_of(&g, b)[0].id);
    }
}

#[test]
fn slot_crossed_by_its_dimension_is_not_bridged() {
    // a slot 3 wide opening in a face, the dimension line running through the opening: the
    // facing ends meet the slot walls, so the opening is real
    let (blk, dim) = dimension("D", "*D1");
    let g = run_with(
        &blk,
        &[
            dim,
            line("E1", "01", [10., 25.], [10., 38.5]),
            line("E2", "01", [10., 41.5], [10., 55.]),
            line("W1", "01", [10., 38.5], [16., 38.5]),
            line("W2", "01", [10., 41.5], [16., 41.5]),
            line("W3", "01", [16., 38.5], [16., 41.5]),
        ],
    );
    assert!(
        implied_between(&g, "E1", "E2").is_empty(),
        "{:?}",
        g.implied_spans
    );
}

// ---------------------------------------------------------------- arcs along their circle

#[test]
fn circle_broken_twice_for_numbers_is_one_circle() {
    // two arcs of one circle, each gap (40°) covered by a number: one circle in the complete
    // structure
    let g = run(&[
        arc("A1", "01", [60., 50.], 10., 20., 160.),
        arc("A2", "01", [60., 50.], 10., 200., 340.),
        text("TR", "08", "M12", [71.75, 46.43], 3.5, 90.),
        text("TL", "08", "M12", [51.75, 46.43], 3.5, 90.),
    ]);
    let s = implied_between(&g, "A1", "A2");
    assert_eq!(s.len(), 2, "{:?}", g.implied_spans);
    assert!(
        s.iter().all(|x| x.cause == "text_break" && x.arc.is_some()),
        "{s:?}"
    );
    let l = line_of(&g, "A1");
    assert_eq!(l.len(), 1);
    assert_eq!(
        (l[0].geometry.as_str(), l[0].id.as_str()),
        ("circle", line_of(&g, "A2")[0].id.as_str())
    );
    no_self_or_long_arc_span(&g);
}

#[test]
fn arc_broken_once_keeps_its_gap() {
    // one arc with a gap for a number: the cause is named, the arc is not joined to itself
    let g = run(&[
        arc("S1", "01", [60., 50.], 6., 30., 330.),
        text("TS", "08", "R6x", [67.75, 46.43], 3.5, 90.),
    ]);
    assert!(
        implied_between(&g, "S1", "S1").is_empty(),
        "{:?}",
        g.implied_spans
    );
    let ends: Vec<_> = g
        .structure_ends
        .iter()
        .filter(|e| e.source_handle == "S1")
        .collect();
    assert_eq!(ends.len(), 2, "{ends:?}");
    assert!(
        ends.iter()
            .all(|e| e.cause == "text_break" && e.partner_end_id.is_none()),
        "{ends:?}"
    );
    let l = line_of(&g, "S1");
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].geometry, "arc");
    no_self_or_long_arc_span(&g);
}

#[test]
fn arcs_facing_across_a_large_gap_are_not_joined() {
    // two short arcs of one circle under a text: each gap is longer than what is drawn of the
    // circle and longer than a quarter turn, so the circle is not completed
    let g = run(&[
        arc("A1", "01", [100., 50.], 3., 0., 20.),
        arc("A2", "01", [100., 50.], 3., 180., 200.),
        text("TA", "08", "ABCDE", [96.5, 49.9], 3.5, 0.),
        text("TB", "08", "ABCDE", [96.5, 46.6], 3.5, 0.),
    ]);
    assert!(
        implied_between(&g, "A1", "A2").is_empty(),
        "{:?}",
        g.implied_spans
    );
    assert!(
        g.structure_ends
            .iter()
            .filter(|e| e.source_handle == "A1" || e.source_handle == "A2")
            .all(|e| e.partner_end_id.is_none()),
    );
    no_self_or_long_arc_span(&g);
}
