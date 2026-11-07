//! Correction round after wave 1: tests for the card variants added from the user's and the
//! expert's corrections and from the assistant's own check. Fixtures are authored from card
//! knowledge (drafting conventions of GB/T 4458.4, 4458.6, 4457.4, 1182, 131); handles,
//! coordinates and labels are invented, and no correction ledger is read.
use oe_contracts::{MdEngineeringExpression, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::discover;
use std::collections::BTreeSet;

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
fn lwpoly(id: &str, layer: &str, pts: &[[f64; 2]], closed: bool) -> String {
    let mut t = format!("90\n{}\n70\n{}\n", pts.len(), i32::from(closed));
    for p in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
    }
    ent("LWPOLYLINE", id, layer, &t)
}
fn insert(id: &str, layer: &str, name: &str, at: [f64; 2]) -> String {
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
/// HATCH with one boundary of line edges and a pattern angle.
fn hatch_edges(id: &str, layer: &str, pts: &[[f64; 2]], angle: f64) -> String {
    let mut t = format!(
        "10\n0\n20\n0\n30\n0\n2\nANSI31\n70\n0\n71\n0\n91\n1\n92\n1\n93\n{}\n",
        pts.len()
    );
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        t.push_str(&format!(
            "72\n1\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
            a[0], a[1], b[0], b[1]
        ));
    }
    t.push_str(&format!("97\n0\n75\n0\n76\n1\n52\n{angle}\n41\n1\n77\n0\n78\n1\n53\n{angle}\n43\n0\n44\n0\n45\n-1\n46\n1\n79\n0\n98\n0\n"));
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
    // W3: a layer outside the GB/T 14665 table, drawn in the drawing's thin colour
    for (name, color, lt) in [
        ("0", 7, "CONTINUOUS"),
        ("01", 7, "CONTINUOUS"),
        ("02", 3, "CONTINUOUS"),
        ("04", 2, "DASHX"),
        ("05", 1, "AXISX"),
        ("08", 3, "CONTINUOUS"),
        ("11", 3, "CONTINUOUS"),
        ("W3", 3, "CONTINUOUS"),
    ] {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n62\n{color}\n6\n{lt}\n"));
    }
    s.push_str("0\nENDSEC\n");
    s
}
fn blocks(defs: &[(&str, Vec<String>)]) -> String {
    let mut s = String::from("0\nSECTION\n2\nBLOCKS\n");
    for (name, ents) in defs {
        let body: String = ents
            .iter()
            .map(|e| e.replacen("\n5\n", &format!("\n5\n{name}"), 1))
            .collect();
        s.push_str(&format!(
            "0\nBLOCK\n2\n{name}\n10\n0\n20\n0\n{body}0\nENDBLK\n"
        ));
    }
    s.push_str("0\nENDSEC\n");
    s
}
fn run(blocks: &str, ents: &[String]) -> MdEngineeringExpressionGraph {
    let bytes = format!(
        "{}{blocks}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        tables(),
        ents.concat()
    );
    match discover(bytes.as_bytes()) {
        Ok(g) => g,
        Err(e) => panic!("{e:#}"),
    }
}
fn exprs_of<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
) -> Vec<&'a MdEngineeringExpression> {
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
fn kinds(g: &MdEngineeringExpressionGraph, src: &str) -> BTreeSet<String> {
    exprs_of(g, src)
        .into_iter()
        .map(|e| e.kind.clone())
        .collect()
}
fn expr_with<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
    kind: &str,
) -> Option<&'a MdEngineeringExpression> {
    exprs_of(g, src).into_iter().find(|e| e.kind == kind)
}
fn role(
    g: &MdEngineeringExpressionGraph,
    e: &MdEngineeringExpression,
    src: &str,
) -> BTreeSet<String> {
    g.members
        .iter()
        .filter(|m| m.expression_id == e.id && m.source_handle == src)
        .map(|m| m.member_role.clone())
        .collect()
}
fn names(e: &MdEngineeringExpression, card: &str, variant: &str) -> bool {
    (e.rule.contains(&format!("card:{card}")) || e.evidence_ids.contains(&format!("card:{card}")))
        && (e.rule.ends_with(variant) || e.evidence_ids.contains(&format!("variant:{variant}")))
}
fn unresolved(g: &MdEngineeringExpressionGraph, src: &str) -> bool {
    kinds(g, src).contains("unresolved")
}
/// Native linear DIMENSION measuring along x from (x, y) to (x + 30, y), dimension line at y + 10;
/// its own block draws the extension lines from y to y + 12.
fn native_dimension(id: &str, block: &str, txt: &str, x: f64, y: f64) -> (String, String) {
    let children = [
        line(&format!("{id}1"), "0", [x, y + 10.], [x + 30., y + 10.]),
        line(&format!("{id}2"), "0", [x, y], [x, y + 12.]),
        line(&format!("{id}3"), "0", [x + 30., y], [x + 30., y + 12.]),
        solid(&format!("{id}4"), "0", [x, y + 10.], [x + 2., y + 10.]),
        solid(
            &format!("{id}5"),
            "0",
            [x + 30., y + 10.],
            [x + 28., y + 10.],
        ),
        ent(
            "MTEXT",
            &format!("{id}6"),
            "0",
            &format!(
                "1\nshown\n10\n{}\n20\n{}\n40\n2.5\n71\n8\n",
                x + 15.,
                y + 11.
            ),
        ),
    ];
    let blk = format!(
        "0\nBLOCK\n2\n{block}\n10\n0\n20\n0\n{}0\nENDBLK\n",
        children.concat()
    );
    let dim = ent(
        "DIMENSION",
        id,
        "0",
        &format!(
            "2\n{block}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n70\n160\n1\n{txt}\n13\n{x}\n23\n{y}\n14\n{}\n24\n{y}\n42\n30\n50\n0\n",
            x + 30.,
            y + 10.,
            x + 15.,
            y + 11.,
            x + 30.
        ),
    );
    (blk, dim)
}
fn blocks_section(parts: &[String]) -> String {
    format!("0\nSECTION\n2\nBLOCKS\n{}0\nENDSEC\n", parts.concat())
}

// ---------------------------------------------------------------- section_line_region

#[test]
fn section_line_drawn_by_hand_on_the_hatch_boundary() {
    // end face runs from the axis to the top; the hatch boundary was picked along a hand-drawn
    // section line from the axis to the end face, leaving an unhatched sliver below it
    let ents = vec![
        line("EF", "01", [50., 0.], [50., 20.]),
        line("TP", "01", [0., 20.], [50., 20.]),
        line("LF", "01", [0., 0.], [0., 20.]),
        line("AX", "05", [-5., 0.], [55., 0.]),
        line("HS", "01", [40., 0.], [50., 9.]),
        hatch_edges(
            "HH",
            "02",
            &[[40., 0.], [50., 9.], [50., 20.], [0., 20.], [0., 0.]],
            45.,
        ),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "HS", "section_hatch").unwrap_or_else(|| panic!("{:?}", kinds(&g, "HS")));
    assert!(
        names(e, "section_line_region", "boundary_along_drawn_line"),
        "{:?}",
        e.evidence_ids
    );
    assert!(!kinds(&g, "HS").contains("visible_contour"));
    assert!(kinds(&g, "EF").contains("visible_contour"));
}

#[test]
fn chamfer_edge_on_the_hatch_boundary_stays_outline() {
    // a 135° chamfer at the top corner, parallel to a 135° pattern; the outlines end at it
    let ents = vec![
        line("BT", "01", [0., 0.], [50., 0.]),
        line("EF", "01", [50., 0.], [50., 11.]),
        line("CH", "01", [50., 11.], [41., 20.]),
        line("TP", "01", [41., 20.], [0., 20.]),
        line("LF", "01", [0., 20.], [0., 0.]),
        hatch_edges(
            "HH",
            "02",
            &[[0., 0.], [50., 0.], [50., 11.], [41., 20.], [0., 20.]],
            135.,
        ),
    ];
    let g = run("", &ents);
    assert!(
        kinds(&g, "CH").contains("visible_contour"),
        "{:?}",
        kinds(&g, "CH")
    );
    assert!(!kinds(&g, "CH").contains("section_hatch"));
}

// ---------------------------------------------------------------- gdt_frame_content

#[test]
fn frame_block_with_outline_dividers_and_nested_glyph() {
    let glyph = vec![
        line("s", "0", [2., 1.], [8., 8.]),
        lwpoly(
            "h",
            "0",
            &[[8., 8.], [6.4, 7.4], [7.4, 6.4], [8., 8.]],
            false,
        ),
    ];
    let frame = vec![
        lwpoly(
            "o",
            "0",
            &[[0., 0.], [40., 0.], [40., 10.], [0., 10.]],
            true,
        ),
        insert("g", "0", "GRUN", [0., 0.]),
        line("d1", "0", [10., 0.], [10., 10.]),
        line("d2", "0", [28., 0.], [28., 10.]),
        line("d3", "0", [34., 0.], [34., 10.]),
        text("v", "0", "0.04", [12., 3.], 4., 0.),
        text("a", "0", "A", [29.5, 3.], 4., 0.),
        text("c", "0", "C", [35.5, 3.], 4., 0.),
    ];
    let g = run(
        &blocks(&[("GRUN", glyph), ("FRB", frame)]),
        &[insert("FB", "02", "FRB", [100., 100.])],
    );
    let e = expr_with(&g, "FB", "gps_runout").unwrap_or_else(|| panic!("{:?}", kinds(&g, "FB")));
    assert!(
        e.label.contains("0.04") && e.concept_ids.iter().any(|c| c == "DC-G5-057"),
        "{} {:?}",
        e.label,
        e.concept_ids
    );
}

// ---------------------------------------------------------------- view_callout

fn arrow_block(with_letter: bool) -> String {
    let mut v = vec![
        line("s", "0", [0., 0.], [12., 0.]),
        solid("h", "0", [15., 0.], [12., 0.]),
    ];
    if with_letter {
        v.push(insert("n", "0", "VLET", [0., 0.]));
    }
    blocks(&[
        ("VLET", vec![text("t", "0", "C", [-4., 1.], 3.5, 0.)]),
        ("VAB", v),
    ])
}

#[test]
fn view_arrow_drawn_as_one_block() {
    let g = run(
        &arrow_block(true),
        &[insert("VA", "02", "VAB", [100., 100.])],
    );
    let e = expr_with(&g, "VA", "view_callout").unwrap_or_else(|| panic!("{:?}", kinds(&g, "VA")));
    assert_eq!(e.label, "C");
    assert!(
        names(e, "view_callout", "arrow_block") && e.concept_ids.iter().any(|c| c == "DC-G2-072")
    );
}

#[test]
fn arrow_block_without_letter_is_not_a_view_arrow() {
    let g = run(
        &arrow_block(false),
        &[insert("VA", "02", "VAB", [100., 100.])],
    );
    assert!(
        !kinds(&g, "VA").contains("view_callout"),
        "{:?}",
        kinds(&g, "VA")
    );
}

// ---------------------------------------------------------------- surface_texture_symbol

#[test]
fn texture_symbol_takes_the_extension_line_it_stands_on() {
    // upright symbol (tip at the origin) with an old-edition value; it stands on a thin line
    // continuing the outline
    let sym = vec![
        line("s", "0", [0., 0.], [-2.5, 4.33]),
        line("l", "0", [0., 0.], [5., 8.66]),
        text("t", "0", "3.2", [-1., 6.], 2.5, 0.),
    ];
    let ents = vec![
        line("OL", "01", [0., 0.], [30., 0.]),
        line("OR", "01", [30., 0.], [30., -20.]),
        line("XL", "02", [33., 0.], [50., 0.]),
        insert("RS", "11", "RSYM", [42., 0.]),
    ];
    let g = run(&blocks(&[("RSYM", sym)]), &ents);
    let e = expr_with(&g, "RS", "surface_roughness").unwrap();
    assert!(
        role(&g, e, "XL").contains("extension_line"),
        "{:?}",
        kinds(&g, "XL")
    );
    assert!(e.concept_ids.iter().any(|c| c == "DC-G6-052"));
}

// ---------------------------------------------------------------- theoretically_exact_dimension

/// Box of four lines around a value at `c` (text height h): top, two sides and a bottom.
fn value_box(prefix: &str, x0: f64, x1: f64, y0: f64, y1: f64, layer: &str) -> Vec<String> {
    vec![
        line(&format!("{prefix}T"), layer, [x0, y1], [x1, y1]),
        line(&format!("{prefix}L"), layer, [x0, y1], [x0, y0]),
        line(&format!("{prefix}R"), layer, [x1, y1], [x1, y0]),
    ]
}

#[test]
fn boxed_native_value_is_theoretically_exact() {
    let (blk, dim) = native_dimension("ND", "*DT1", "50", 0., 0.);
    let mut ents = vec![dim];
    // the value is drawn at (15, 11), height 2.5: a box from x 11 to 19, top at y 14.5, down to the dimension line
    ents.extend(value_box("B", 11., 19., 10., 14.5, "08"));
    let g = run(&blocks_section(&[blk]), &ents);
    let e = expr_with(&g, "ND", "dimension_linear").unwrap();
    assert!(
        e.qualifiers.iter().any(|q| q == "theoretically_exact"),
        "{:?}",
        e.qualifiers
    );
    assert!(
        names(e, "theoretically_exact_dimension", "native_value_box")
            && e.concept_ids.iter().any(|c| c == "DC-G4-029")
    );
    assert!(role(&g, e, "BT").contains("frame_cell"));
}

#[test]
fn boxed_loose_value_is_theoretically_exact() {
    let mut ents = vec![
        line("H1", "08", [100., 50.], [120., 50.]),
        solid("HA", "08", [100., 50.], [103., 50.]),
        solid("HB", "08", [120., 50.], [117., 50.]),
        text("HT", "08", "20", [108.6, 50.4], 2.5, 0.),
        line("HE1", "02", [100., 40.], [100., 52.]),
        line("HE2", "02", [120., 40.], [120., 52.]),
    ];
    ents.extend(value_box("B", 107.8, 112.8, 50., 54., "08"));
    let g = run("", &ents);
    let e = expr_with(&g, "HT", "dimension_linear").unwrap();
    assert!(
        e.qualifiers.iter().any(|q| q == "theoretically_exact")
            && names(e, "theoretically_exact_dimension", "line_box"),
        "{:?} {:?}",
        e.qualifiers,
        e.evidence_ids
    );
}

/// Hole-circle diameter: terminators pointing outward at the circle, the dimension line between
/// their bases, a collinear leader from one tip to a value written outside on a shoulder, with
/// or without a box.
fn extended_diameter(boxed: bool) -> Vec<String> {
    let mut v = vec![
        solid("A1", "08", [0., 0.], [3., 0.]),
        solid("A2", "08", [40., 0.], [37., 0.]),
        line("DL", "08", [3., 0.], [37., 0.]),
        line("LD", "08", [40., 0.], [50., 0.]),
        line("SH", "08", [50., 0.], [64., 0.]),
        text("VT", "11", "%%c47", [52., 1.2], 3.5, 0.),
    ];
    if boxed {
        v.extend(value_box("X", 50., 63.5, 0., 6., "11"));
    }
    v
}

#[test]
fn boxed_value_on_extended_dimension_line_is_one_dimension() {
    let g = run("", &extended_diameter(true));
    let e = expr_with(&g, "VT", "dimension_diameter")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "VT")));
    for (s, r) in [
        ("A1", "arrowhead"),
        ("A2", "arrowhead"),
        ("DL", "dimension_line"),
        ("LD", "leader_line"),
        ("XT", "frame_cell"),
        ("XL", "frame_cell"),
        ("XR", "frame_cell"),
        ("SH", "frame_cell"),
    ] {
        assert!(role(&g, e, s).contains(r), "{s}: {:?}", role(&g, e, s));
    }
    assert!(e.qualifiers.iter().any(|q| q == "theoretically_exact"));
    assert!(
        names(e, "dimension_line_parts", "extended_to_outside_value")
            && names(e, "theoretically_exact_dimension", "line_box")
    );
    assert!(
        ["DC-G4-029", "DC-G4-059", "DC-G4-030"]
            .iter()
            .all(|c| e.concept_ids.iter().any(|x| x == c)),
        "{:?}",
        e.concept_ids
    );
}

#[test]
fn dimension_line_continues_past_the_terminator_to_the_value() {
    let g = run("", &extended_diameter(false));
    let e = expr_with(&g, "VT", "dimension_diameter")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "VT")));
    assert!(
        role(&g, e, "DL").contains("dimension_line") && role(&g, e, "LD").contains("leader_line")
    );
    assert!(
        names(e, "dimension_line_parts", "extended_to_outside_value")
            && !e.qualifiers.iter().any(|q| q == "theoretically_exact")
    );
}

#[test]
fn frame_cell_value_is_not_a_theoretically_exact_dimension() {
    // a symmetry frame drawn with lines: the value cell is a box around a number, but it is a frame cell
    let glyph = vec![
        line("a", "0", [2., 2.], [8., 2.]),
        line("b", "0", [0.5, 5.], [9.5, 5.]),
        line("c", "0", [2., 8.], [8., 8.]),
    ];
    let ents = vec![
        insert("FG", "01", "GSYM", [0., 0.]),
        line("B1", "02", [0., 0.], [10., 0.]),
        line("T1", "02", [0., 10.], [10., 10.]),
        line("B2", "02", [10., 0.], [26., 0.]),
        line("T2", "02", [10., 10.], [26., 10.]),
        line("V0", "02", [0., 0.], [0., 10.]),
        line("V1", "02", [10., 0.], [10., 10.]),
        line("V2", "02", [26., 0.], [26., 10.]),
        text("FV", "11", "0.06", [12., 3.], 4., 0.),
    ];
    let g = run(&blocks(&[("GSYM", glyph)]), &ents);
    assert!(
        kinds(&g, "FV").iter().any(|k| k.starts_with("gps_")),
        "{:?}",
        kinds(&g, "FV")
    );
    assert!(
        !g.expressions
            .iter()
            .any(|e| e.qualifiers.iter().any(|q| q == "theoretically_exact"))
    );
}

// ---------------------------------------------------------------- local_section_boundary

/// A step outline with a thin freehand curve from the top edge down to the axis.
fn local_section(ents_extra: Vec<String>) -> Vec<String> {
    let curve = [
        [20., 20.],
        [17., 19.4],
        [14.5, 18.],
        [12.6, 15.8],
        [11.4, 12.8],
        [10.9, 9.],
        [10.8, 4.5],
        [11., 0.],
    ];
    let mut v = vec![
        line("TP", "01", [0., 20.], [40., 20.]),
        line("EF", "01", [40., 20.], [40., 0.]),
        line("AX", "05", [-5., 0.], [45., 0.]),
        lwpoly("WV", "W3", &curve, false),
    ];
    v.extend(ents_extra);
    v
}

#[test]
fn local_section_boundary_around_a_hatch_entity() {
    let boundary = [
        [20., 20.],
        [17., 19.4],
        [14.5, 18.],
        [12.6, 15.8],
        [11.4, 12.8],
        [10.9, 9.],
        [10.8, 4.5],
        [11., 0.],
        [40., 0.],
        [40., 20.],
    ];
    let g = run(
        "",
        &local_section(vec![hatch_edges("HH", "02", &boundary, 45.)]),
    );
    let e = expr_with(&g, "WV", "break_line").unwrap_or_else(|| panic!("{:?}", kinds(&g, "WV")));
    assert!(
        names(e, "local_section_boundary", "hatch_entity"),
        "{:?}",
        e.evidence_ids
    );
}

#[test]
fn local_section_boundary_around_loose_section_lines() {
    // three parallel thin 45° section lines from the curve to the end face
    let lines = vec![
        line("S1", "02", [11.2, 2.], [29., 19.8]),
        line("S2", "02", [10.9, 7.], [24., 20.]),
        line("S3", "02", [20., 0.], [40., 20.]),
    ];
    let g = run("", &local_section(lines));
    let e = expr_with(&g, "WV", "break_line").unwrap_or_else(|| panic!("{:?}", kinds(&g, "WV")));
    assert!(
        names(e, "local_section_boundary", "loose_hatch_lines"),
        "{:?}",
        e.evidence_ids
    );
    assert!(!kinds(&g, "WV").contains("visible_contour"));
}

#[test]
fn thin_free_curve_without_section_lines_is_not_an_outline() {
    let g = run("", &local_section(vec![]));
    assert!(
        !kinds(&g, "WV").contains("visible_contour"),
        "{:?}",
        kinds(&g, "WV")
    );
    assert!(kinds(&g, "TP").contains("visible_contour"));
}

// ---------------------------------------------------------------- dimension_line_parts

#[test]
fn extension_line_end_piece_joins_the_extension_line() {
    // exploded dimension whose left extension line is drawn in two collinear pieces running far
    // past the dimension line; the rule claims a short overshoot, the end piece remains
    let ents = vec![
        line("H1", "08", [100., 50.], [120., 50.]),
        solid("HA", "08", [100., 50.], [103., 50.]),
        solid("HB", "08", [120., 50.], [117., 50.]),
        text("HT", "08", "20", [108.6, 50.4], 2.5, 0.),
        line("E1", "02", [100., 40.], [100., 52.]),
        line("E2", "02", [100., 52.], [100., 59.]),
        line("E3", "02", [120., 40.], [120., 52.]),
    ];
    let g = run("", &ents);
    assert!(!unresolved(&g, "E2"), "{:?}", kinds(&g, "E2"));
    let e = expr_with(&g, "HT", "dimension_linear").unwrap();
    assert!(
        names(e, "dimension_line_parts", "end_piece")
            && role(&g, e, "E2").contains("extension_line"),
        "{:?}",
        e.evidence_ids
    );
}

/// A long root line on the right extension line of a native dimension; a second dimension has
/// its only terminator on the middle of that line, far from its ends.
fn shared_line(with_second: bool) -> (String, Vec<String>) {
    let (blk, dim) = native_dimension("ND", "*DE3", "30", 0., 0.);
    let mut ents = vec![dim, line("EX", "02", [30., 0.], [30., 80.])];
    if with_second {
        ents.extend([
            solid("B1", "08", [30., 40.], [33., 40.]),
            line("D2", "08", [33., 40.], [60., 40.]),
            text("T2", "08", "%%c40", [40., 40.4], 2.5, 0.),
        ]);
    }
    (blocks_section(&[blk]), ents)
}

#[test]
fn one_line_is_extension_line_of_two_dimensions() {
    let (blk, ents) = shared_line(true);
    let g = run(&blk, &ents);
    assert!(!unresolved(&g, "EX"), "{:?}", kinds(&g, "EX"));
    let e = exprs_of(&g, "T2")
        .into_iter()
        .find(|e| e.family == "dimension")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "T2")));
    assert!(
        role(&g, e, "EX").contains("extension_line") && names(e, "dimension_line_parts", "shared"),
        "{} {:?}",
        e.kind,
        e.evidence_ids
    );
}

#[test]
fn long_free_piece_of_an_extension_line_is_not_absorbed() {
    let (blk, ents) = shared_line(false);
    let g = run(&blk, &ents);
    assert!(unresolved(&g, "EX"), "{:?}", kinds(&g, "EX"));
}
