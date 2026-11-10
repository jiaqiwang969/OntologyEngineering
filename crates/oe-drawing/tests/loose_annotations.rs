//! Annotations drawn with loose lines, solids and texts (older AutoCAD drawings): tolerance frames
//! whose outline is drawn in line pieces, characteristic arrows with drawn heads, leaders drawn in
//! overlapping pieces, numbered datum letters, boxes of theoretically exact dimensions drawn as
//! closed polylines, words written with a dimension or on a leader's reference line, leader notes,
//! requirement items numbered in their text, the indication for most surfaces, notes in the sheet
//! margin and loose copies of a block. Fixtures are authored from card knowledge (GB/T 1182,
//! GB/T 4457.2, GB/T 4458.4, GB/T 16675.2, GB/T 131, GB/T 14665, GB/T 10609.1); handles,
//! coordinates and texts are invented, and no reference reading is used.
use oe_contracts::{MdEngineeringExpression, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::{discover, text_concepts};
use oe_drawing::expression_text::{Lex, lex};
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
fn circle(id: &str, layer: &str, c: [f64; 2], r: f64) -> String {
    ent(
        "CIRCLE",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]),
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
fn solid3(id: &str, layer: &str, p: [[f64; 2]; 3]) -> String {
    ent(
        "SOLID",
        id,
        layer,
        &format!(
            "10\n{}\n20\n{}\n11\n{}\n21\n{}\n12\n{}\n22\n{}\n13\n{}\n23\n{}\n",
            p[0][0], p[0][1], p[1][0], p[1][1], p[2][0], p[2][1], p[2][0], p[2][1]
        ),
    )
}
/// A filled arrowhead with its tip at `tip` and its base at `base`.
fn solid(id: &str, layer: &str, tip: [f64; 2], base: [f64; 2]) -> String {
    let d = [base[0] - tip[0], base[1] - tip[1]];
    let p = [-d[1] * 0.15, d[0] * 0.15];
    solid3(
        id,
        layer,
        [
            tip,
            [base[0] + p[0], base[1] + p[1]],
            [base[0] - p[0], base[1] - p[1]],
        ],
    )
}
fn lwpoly(id: &str, layer: &str, pts: &[[f64; 2]], closed: bool) -> String {
    let mut t = format!("90\n{}\n70\n{}\n", pts.len(), i32::from(closed));
    for p in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
    }
    ent("LWPOLYLINE", id, layer, &t)
}
fn rect(id: &str, layer: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    lwpoly(
        id,
        layer,
        &[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]],
        false,
    )
}
fn insert(id: &str, layer: &str, name: &str, at: [f64; 2], scale: f64) -> String {
    ent(
        "INSERT",
        id,
        layer,
        &format!(
            "2\n{name}\n10\n{}\n20\n{}\n41\n{scale}\n42\n{scale}\n50\n0\n",
            at[0], at[1]
        ),
    )
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
/// Block definitions; child handles are made unique by prefixing the block name.
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
fn has(e: &MdEngineeringExpression, ids: &[&str]) -> bool {
    ids.iter().all(|i| e.concept_ids.iter().any(|c| c == i))
}
fn covered(g: &MdEngineeringExpressionGraph) {
    for root in &g.source.entities {
        assert!(
            !kinds(g, &root.id).contains("unresolved"),
            "unresolved {}",
            root.id
        );
    }
}

// ---------------------------------------------------------------- tolerance frames drawn with lines

/// A position frame whose outline sides are drawn in staggered, overlapping pieces, divided by two
/// full-height lines: the glyph (circle and cross) in the first cell, `Ø` and the value as two
/// texts in the second, two datum letters in the third.
fn staggered_frame() -> Vec<String> {
    vec![
        line("T1", "02", [100., 207.], [112., 207.]),
        line("T2", "08", [108., 207.], [130., 207.]),
        line("T3", "02", [130., 207.], [146., 207.]),
        line("B1", "02", [100., 200.], [111., 200.]),
        line("B2", "08", [109., 200.], [129., 200.]),
        line("B3", "02", [129., 200.], [146., 200.]),
        line("SL", "02", [100., 200.], [100., 207.]),
        line("SR", "02", [146., 200.], [146., 207.]),
        line("D1", "02", [107., 200.], [107., 207.]),
        line("D2", "02", [128., 200.], [128., 207.]),
        circle("GC", "01", [103.5, 203.5], 1.6),
        line("GH", "01", [101.4, 203.5], [105.6, 203.5]),
        line("GV", "01", [103.5, 201.4], [103.5, 205.6]),
        text("VZ", "11", "%%c", [108.5, 202.], 3.5, 0.),
        text("VV", "11", "0.25", [111., 202.], 3.5, 0.),
        text("R1", "11", "C", [130., 202.], 3.5, 0.),
        text("R2", "11", "D", [138., 202.], 3.5, 0.),
    ]
}

#[test]
fn frame_outline_drawn_in_staggered_line_pieces() {
    let g = run("", &staggered_frame());
    let e = expr_with(&g, "GC", "gps_position").unwrap_or_else(|| panic!("{:?}", kinds(&g, "GC")));
    assert_eq!(e.label, "⌖ Ø0.25 | C D");
    assert!(
        names(e, "gdt_frame_content", "line_outline_dividers"),
        "{:?}",
        e.evidence_ids
    );
    for s in ["T1", "T2", "T3", "B1", "B2", "B3", "SL", "SR", "D1", "D2"] {
        assert!(
            role(&g, e, s).contains("frame_cell"),
            "{s}: {:?}",
            role(&g, e, s)
        );
    }
    for (s, r) in [
        ("VZ", "tolerance_value"),
        ("VV", "tolerance_value"),
        ("R1", "datum_reference"),
        ("R2", "datum_reference"),
    ] {
        assert!(role(&g, e, s).contains(r), "{s}");
    }
    assert!(
        has(e, &["DC-G5-041", "DC-G5-004", "DC-G5-089"]),
        "{:?}",
        e.concept_ids
    );
    for s in ["GC", "GH", "GV"] {
        assert!(!kinds(&g, s).contains("visible_contour"));
    }
    covered(&g);
}

#[test]
fn frame_leader_drawn_in_overlapping_pieces() {
    let mut ents = staggered_frame();
    ents.extend([
        line("P1", "02", [146., 203.5], [160., 203.5]),
        line("P2", "08", [159.6, 203.5], [175., 203.5]),
        line("P3", "08", [172., 203.5], [175., 203.5]),
        solid("PA", "08", [175., 203.5], [172., 203.5]),
        line("OL", "01", [175., 190.], [175., 220.]),
    ]);
    let g = run("", &ents);
    let e = expr_with(&g, "GC", "gps_position").expect("frame");
    for s in ["P1", "P2", "P3"] {
        assert!(
            role(&g, e, s).contains("leader_line"),
            "{s}: {:?}",
            role(&g, e, s)
        );
    }
    assert!(role(&g, e, "PA").contains("arrowhead"));
    assert!(
        names(e, "gdt_frame_content", "leader_pieces"),
        "{:?}",
        e.evidence_ids
    );
}

#[test]
fn value_and_callout_words_in_one_text_over_a_frame() {
    let mut ents = staggered_frame();
    ents.extend([
        text("DV", "11", "6-%%c7 均布", [101., 209.], 3.5, 0.),
        line("P1", "02", [146., 203.5], [157., 203.5]),
        solid("A1", "08", [160., 203.5], [157., 203.5]),
        solid("A2", "08", [167., 203.5], [170., 203.5]),
        line("DL", "08", [160., 203.5], [167., 203.5]),
        line("TL", "08", [170., 203.5], [175., 203.5]),
    ]);
    let g = run("", &ents);
    let d = expr_with(&g, "DV", "dimension_diameter")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "DV")));
    assert_eq!(d.label, "6-Ø7 均布");
    assert!(role(&g, d, "DV").contains("dimension_text"));
    let f = expr_with(&g, "GC", "gps_position").expect("frame");
    assert!(
        !role(&g, f, "DV").contains("note_text"),
        "the value is not a scope note of the frame"
    );
}

#[test]
fn table_rows_are_not_a_line_drawn_frame() {
    let mut ents = Vec::new();
    for (i, y) in [0., 7., 14., 21.].iter().enumerate() {
        ents.push(line(&format!("H{i}"), "02", [0., *y], [60., *y]));
    }
    for (i, x) in [0., 20., 40., 60.].iter().enumerate() {
        ents.push(line(&format!("V{i}"), "02", [*x, 0.], [*x, 21.]));
    }
    ents.extend([
        circle("GC", "01", [10., 10.4], 1.6),
        line("GH", "01", [7.9, 10.4], [12.1, 10.4]),
        line("GV", "01", [10., 8.4], [10., 12.6]),
        text("VV", "11", "0.25", [22., 9.], 3.5, 0.),
        text("R1", "11", "C", [42., 9.], 3.5, 0.),
    ]);
    let g = run("", &ents);
    assert!(
        !g.expressions.iter().any(|e| e.kind.starts_with("gps_")),
        "{:?}",
        g.expressions.iter().map(|e| &e.kind).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------- characteristic arrows with drawn heads

/// The closed head of an arrow at `tip` along the unit direction `u`: three lines joined end to end.
fn head_lines(prefix: &str, tip: [f64; 2], u: [f64; 2], gap: f64) -> Vec<String> {
    let back = [tip[0] - 2.4 * u[0], tip[1] - 2.4 * u[1]];
    let n = [-u[1], u[0]];
    let b1 = [back[0] + 0.35 * n[0], back[1] + 0.35 * n[1]];
    let b2 = [back[0] - 0.35 * n[0], back[1] - 0.35 * n[1]];
    let b2g = [b2[0] - gap * u[0], b2[1] - gap * u[1]];
    vec![
        line(&format!("{prefix}1"), "01", tip, b1),
        line(&format!("{prefix}2"), "01", b1, b2),
        line(&format!("{prefix}3"), "01", b2g, tip),
    ]
}

#[test]
fn runout_glyph_with_a_head_of_three_lines() {
    let u = [3.5 / 5.94, 4.8 / 5.94];
    let mut ents = vec![
        // three cells, each drawn with its own lines, sharing sides
        line("A1", "02", [200., 107.], [207., 107.]),
        line("A2", "02", [200., 100.], [207., 100.]),
        line("A3", "02", [200., 100.], [200., 107.]),
        line("A4", "02", [207., 100.], [207., 107.]),
        line("C1", "02", [207., 107.], [225., 107.]),
        line("C2", "02", [207., 100.], [225., 100.]),
        line("C3", "02", [225., 100.], [225., 107.]),
        line("E1", "02", [225., 107.], [233., 107.]),
        line("E2", "02", [225., 100.], [233., 100.]),
        line("E3", "02", [233., 100.], [233., 107.]),
        line("ST", "01", [201.5, 101.2], [205., 106.]),
        text("VV", "11", "0.07", [209., 102.], 3.5, 0.),
        text("RE", "11", "E", [227.5, 102.], 3.5, 0.),
    ];
    ents.extend(head_lines("HD", [205., 106.], u, 0.));
    let g = run("", &ents);
    let e = expr_with(&g, "ST", "gps_runout").unwrap_or_else(|| panic!("{:?}", kinds(&g, "ST")));
    assert_eq!(e.label, "↗ 0.07 | E");
    for s in ["ST", "HD1", "HD2", "HD3"] {
        assert!(role(&g, e, s).contains("symbol_glyph"), "{s}");
        assert!(!kinds(&g, s).contains("visible_contour"), "{s}");
    }
    assert!(
        names(e, "gdt_characteristic", "drawn_arrow_head"),
        "{:?}",
        e.evidence_ids
    );
}

/// An outline of four lines divided by two full-height lines, with an oblique stroke in the first
/// cell and the given closed head.
fn outline_frame(head: Vec<String>) -> Vec<String> {
    let mut v = vec![
        line("OT", "02", [300., 107.], [333., 107.]),
        line("OB", "02", [300., 100.], [333., 100.]),
        line("OL", "02", [300., 100.], [300., 107.]),
        line("OR", "02", [333., 100.], [333., 107.]),
        line("OD1", "02", [307., 100.], [307., 107.]),
        line("OD2", "02", [325., 100.], [325., 107.]),
        line("ST", "01", [301.5, 101.2], [305., 106.]),
        text("VV", "11", "0.04", [309., 102.], 3.5, 0.),
        text("RE", "11", "C-D", [325.5, 102.], 3.5, 0.),
    ];
    v.extend(head);
    v
}

#[test]
fn runout_glyph_with_a_filled_head_in_a_line_frame() {
    let g = run(
        "",
        &outline_frame(vec![solid("HS", "01", [305., 106.], [303.586, 104.061])]),
    );
    let e = expr_with(&g, "ST", "gps_runout").unwrap_or_else(|| panic!("{:?}", kinds(&g, "ST")));
    assert!(!kinds(&g, "ST").contains("gps_straightness"));
    assert!(role(&g, e, "HS").contains("symbol_glyph"));
    assert!(names(e, "gdt_frame_content", "line_outline_dividers"));
    assert!(has(e, &["DC-G5-057", "DC-G5-002"]), "{:?}", e.concept_ids);
    assert_eq!(e.label, "↗ 0.04 | C-D");
}

#[test]
fn oblique_stroke_without_a_head_is_not_runout() {
    let u = [3.5 / 5.94, 4.8 / 5.94];
    // three short lines near the stroke end that do not close
    let g = run("", &outline_frame(head_lines("HD", [305., 106.], u, 0.3)));
    assert!(!g.expressions.iter().any(|e| e.kind == "gps_runout"));
}

// ---------------------------------------------------------------- numbered datum letters, square of lines

/// Old-edition datum symbol: letter in a circle, connector and a thick bar.
fn old_datum(prefix: &str, at: [f64; 2], letter: &str) -> Vec<String> {
    let [x, y] = at;
    let w = letter.chars().count() as f64 * 0.68 * 3.5;
    vec![
        line(
            &format!("{prefix}BAR"),
            "01",
            [x - 3., y + 7.7],
            [x + 3., y + 7.7],
        ),
        line(&format!("{prefix}CON"), "02", [x, y + 7.7], [x, y + 3.5]),
        circle(&format!("{prefix}CIR"), "02", [x, y], 3.5),
        text(
            &format!("{prefix}LET"),
            "11",
            letter,
            [x - w / 2., y - 1.75],
            3.5,
            0.,
        ),
    ]
}

#[test]
fn numbered_datum_named_by_a_frame_is_a_datum() {
    let mut ents = vec![
        circle("P0", "01", [105., 105.], 1.4),
        line("P1", "01", [105., 101.5], [105., 108.5]),
        line("P2", "01", [101.5, 105.], [108.5, 105.]),
        rect("C1", "02", 100., 100., 110., 110.),
        rect("C2", "02", 110., 100., 126., 110.),
        rect("C3", "02", 126., 100., 142., 110.),
        text("V", "11", "0.1", [111., 103.], 3.5, 0.),
        text("R", "11", "E1-E2", [127., 103.], 3.5, 0.),
    ];
    ents.extend(old_datum("X", [60., 60.], "E1"));
    ents.extend(old_datum("Y", [80., 60.], "E2"));
    let g = run("", &ents);
    for (p, name) in [("X", "E1"), ("Y", "E2")] {
        let e =
            expr_with(&g, &format!("{p}LET"), "datum_symbol").unwrap_or_else(|| panic!("{name}"));
        assert_eq!(e.label, name);
        assert!(
            names(e, "datum_feature_symbol", "numbered_letter"),
            "{:?}",
            e.evidence_ids
        );
        assert!(role(&g, e, &format!("{p}BAR")).contains("datum_terminal"));
    }
}

#[test]
fn numbered_letter_without_a_frame_is_not_a_datum() {
    let g = run("", &old_datum("X", [60., 60.], "F3"));
    assert!(!g.expressions.iter().any(|e| e.kind == "datum_symbol"));
}

#[test]
fn datum_square_of_four_lines_is_one_symbol() {
    let ents = vec![
        line("S1", "02", [46.5, 46.5], [53.5, 46.5]),
        line("S2", "02", [46.5, 53.5], [53.5, 53.5]),
        line("S3", "02", [46.5, 46.5], [46.5, 53.5]),
        line("S4", "02", [53.5, 46.5], [53.5, 53.5]),
        text("L", "11", "G", [48.8, 48.25], 3.5, 0.),
        line("CN", "02", [50., 46.5], [50., 40.]),
        solid3("TR", "02", [[48., 40.], [52., 40.], [50., 37.]]),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "L", "datum_symbol").unwrap_or_else(|| panic!("{:?}", kinds(&g, "L")));
    assert!(
        names(e, "datum_feature_symbol", "line_square")
            && names(e, "datum_feature_symbol", "new_square_triangle"),
        "{:?}",
        e.evidence_ids
    );
    for s in ["S1", "S2", "S3", "S4"] {
        assert!(role(&g, e, s).contains("datum_frame"), "{s}");
    }
    assert!(role(&g, e, "TR").contains("datum_terminal"));
}

// ---------------------------------------------------------------- theoretically exact dimension, polyline box

/// A linear dimension along a 30° direction with its value written along it and a closed
/// four-vertex polyline round the value; `extra` texts are added inside the box.
fn rotated_boxed_dimension(extra: bool) -> Vec<String> {
    let (c30, s30) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
    let u = [c30, s30];
    let n = [-s30, c30];
    let at = |a: f64, b: f64| [a * u[0] + b * n[0], a * u[1] + b * n[1]];
    let mut v = vec![
        solid("A1", "08", at(0., 0.), at(3., 0.)),
        solid("A2", "08", at(40., 0.), at(37., 0.)),
        line("DL", "08", at(3., 0.), at(37., 0.)),
        line("E1", "02", at(0., -10.), at(0., 2.)),
        line("E2", "02", at(40., -10.), at(40., 2.)),
        text("VT", "08", "48", at(18., 1.), 3.5, 30.),
        lwpoly(
            "BX",
            "11",
            &[
                at(15.9, -0.225),
                at(24.86, -0.225),
                at(24.86, 5.725),
                at(15.9, 5.725),
            ],
            true,
        ),
    ];
    if extra {
        v.push(text("XT", "11", "X", at(23.2, 2.), 1.5, 30.));
    }
    v
}

#[test]
fn closed_polyline_box_around_a_rotated_value_is_theoretically_exact() {
    let g = run("", &rotated_boxed_dimension(false));
    let e =
        expr_with(&g, "VT", "dimension_linear").unwrap_or_else(|| panic!("{:?}", kinds(&g, "VT")));
    assert!(
        e.qualifiers.iter().any(|q| q == "theoretically_exact"),
        "{:?}",
        e.qualifiers
    );
    assert!(
        names(e, "theoretically_exact_dimension", "closed_polyline_box") && has(e, &["DC-G4-029"])
    );
    assert!(role(&g, e, "BX").contains("frame_cell"));
}

#[test]
fn closed_box_holding_two_texts_is_not_a_ted_box() {
    let g = run("", &rotated_boxed_dimension(true));
    let e =
        expr_with(&g, "VT", "dimension_linear").unwrap_or_else(|| panic!("{:?}", kinds(&g, "VT")));
    assert!(
        !e.qualifiers.iter().any(|q| q == "theoretically_exact"),
        "{:?}",
        e.qualifiers
    );
}

// ---------------------------------------------------------------- loose copy of a read block

/// A registration panel block: a grid of eight lines and two signature words.
fn panel() -> Vec<String> {
    let mut v = vec![
        line("L1", "02", [0., 0.], [0., 40.]),
        line("L2", "02", [10., 0.], [10., 40.]),
        line("L3", "02", [5., 0.], [5., 40.]),
    ];
    for (i, y) in [0., 10., 20., 30., 40.].iter().enumerate() {
        v.push(line(&format!("H{i}"), "02", [0., *y], [10., *y]));
    }
    v.push(text("T1", "11", "描图", [1., 1.], 2., 0.));
    v.push(text("T2", "11", "描校", [1., 11.], 2., 0.));
    v
}

#[test]
fn loose_copy_of_a_read_block_is_coincident() {
    let blk = blocks(&[("REG", panel())]);
    let mut ents = vec![insert("RG", "0", "REG", [0., 0.], 2.)];
    ents.extend([
        line("C1", "11", [0., 0.], [0., 80.]),
        line("C2", "11", [20., 80.], [20., 0.]),
        line("C3", "11", [10., 0.], [10., 80.]),
    ]);
    for (i, y) in [0., 20., 40., 60., 80.].iter().enumerate() {
        ents.push(line(&format!("CH{i}"), "11", [0., *y], [20., *y]));
    }
    ents.push(text("CT1", "11", "描图", [2., 2.], 4., 0.));
    ents.push(text("CT2", "11", "描 校", [2., 22.], 4., 0.));
    let g = run(&blk, &ents);
    let e = expr_with(&g, "RG", "sheet_frame").expect("registration panel");
    for s in [
        "C1", "C2", "C3", "CH0", "CH1", "CH2", "CH3", "CH4", "CT1", "CT2",
    ] {
        assert!(
            role(&g, e, s).contains("coincident_source"),
            "{s}: {:?}",
            kinds(&g, s)
        );
    }
    assert!(
        names(e, "coincident_redraw", "block_redrawn_loose") && has(e, &["DC-G1-055"]),
        "{:?}",
        e.evidence_ids
    );
    covered(&g);
}

#[test]
fn single_line_on_a_block_line_is_not_a_copy() {
    let blk = blocks(&[("REG", panel())]);
    let ents = vec![
        insert("RG", "0", "REG", [0., 0.], 2.),
        line("C1", "11", [0., 0.], [0., 80.]),
    ];
    let g = run(&blk, &ents);
    let e = expr_with(&g, "RG", "sheet_frame").expect("registration panel");
    assert!(!role(&g, e, "C1").contains("coincident_source"));
}

// ---------------------------------------------------------------- dimension text: inch threads, deviations

#[test]
fn inch_thread_designation_is_a_thread() {
    for s in ["1/2\"-13", "5/16\"-18 UNC", "1\"-8", "6-1/2\"-13"] {
        assert_eq!(lex(s), Lex::Thread, "{s}");
        assert!(
            text_concepts("dimension_text", s)
                .unwrap()
                .contains("DC-G3-088"),
            "{s}"
        );
    }
    for s in ["1/2", "13", "2\""] {
        assert_ne!(lex(s), Lex::Thread, "{s}");
    }
}

#[test]
fn deviations_written_as_two_small_texts_join_the_value() {
    let ents = vec![
        solid("A1", "08", [0., 0.], [3., 0.]),
        solid("A2", "08", [30., 0.], [27., 0.]),
        line("DL", "08", [3., 0.], [27., 0.]),
        line("E1", "02", [0., -10.], [0., 2.]),
        line("E2", "02", [30., -10.], [30., 2.]),
        text("VT", "08", "%%c30", [10., 1.], 3.5, 0.),
        text("DU", "08", "+0.1", [18.2, 3.6], 2.45, 0.),
        text("DD", "08", "-0.3", [18.2, 0.9], 2.45, 0.),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "VT", "dimension_diameter")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "VT")));
    assert_eq!(e.label, "Ø30 +0.1/-0.3");
    for s in ["DU", "DD"] {
        assert!(role(&g, e, s).contains("tolerance_text"), "{s}");
    }
    assert!(
        e.qualifiers.iter().any(|q| q == "toleranced") && has(e, &["DC-G4-132"]),
        "{:?}",
        e.concept_ids
    );
    covered(&g);
}

// ---------------------------------------------------------------- callout rows

/// A radius written on the shoulder of a leader whose arrow is at the arc, and a word under the
/// shoulder.
fn radius_on_shoulder(under: &str) -> Vec<String> {
    vec![
        line("SH", "08", [18., 0.], [30., 0.]),
        line("LD", "08", [18., 0.], [12.12, -5.88]),
        solid("AR", "08", [10., -8.], [12.12, -5.88]),
        text("RT", "08", "R6", [20., 1.], 3.5, 0.),
        text("UT", "11", under, [20., -4.5], 3.5, 0.),
    ]
}

#[test]
fn callout_words_under_the_value_join_the_dimension() {
    let g = run("", &radius_on_shoulder("两侧"));
    let e =
        expr_with(&g, "RT", "dimension_radius").unwrap_or_else(|| panic!("{:?}", kinds(&g, "RT")));
    assert_eq!(e.label, "R6 两侧");
    assert!(role(&g, e, "UT").contains("note_text"));
    assert!(
        names(e, "callout_rows", "rows_on_reference_line") && has(e, &["DC-G4-067", "DC-G1-111"]),
        "{:?}",
        e.concept_ids
    );
}

#[test]
fn other_text_under_a_value_is_not_a_callout_row() {
    let g = run("", &radius_on_shoulder("加工面"));
    let e =
        expr_with(&g, "RT", "dimension_radius").unwrap_or_else(|| panic!("{:?}", kinds(&g, "RT")));
    assert_eq!(e.label, "R6");
    assert!(role(&g, e, "UT").is_empty());
}

#[test]
fn hole_callout_rows_on_and_under_the_reference_line() {
    let u = [2. / 5f64.sqrt(), -1. / 5f64.sqrt()];
    let at = |s: f64| [100. + s * u[0], 50. + s * u[1]];
    let k = at(-25.);
    let ents = vec![
        solid("A1", "08", at(0.), at(-3.)),
        solid("A2", "08", at(12.7), at(15.7)),
        line("DL", "08", at(0.), at(12.7)),
        line("LD", "08", at(-3.), k),
        line("SH1", "08", k, [60., k[1]]),
        line("SH2", "08", [62., k[1] + 0.02], [35., k[1] + 0.02]),
        text("N1", "11", "6-", [42., k[1] + 1.], 3.5, 0.),
        text("N2", "11", "螺孔", [47.5, k[1] + 1.], 3.5, 0.),
        text("TH", "11", "1/2\"-13", [55., k[1] + 1.], 3.5, 0.),
        text("DP", "11", "攻深15", [47., k[1] - 5.], 3.5, 0.),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "TH", "thread_note").unwrap_or_else(|| panic!("{:?}", kinds(&g, "TH")));
    assert_eq!(e.label, "6-螺孔 1/2\"-13 攻深15");
    for s in ["N1", "N2", "DP"] {
        assert!(role(&g, e, s).contains("note_text"), "{s}");
    }
    assert!(
        role(&g, e, "SH2").contains("leader_line"),
        "{:?}",
        kinds(&g, "SH2")
    );
    assert!(names(e, "callout_rows", "rows_on_reference_line"));
    covered(&g);
}

#[test]
fn sentence_referring_to_a_diameter_is_a_bounded_requirement() {
    let ents = vec![
        solid("AR", "08", [50., 0.], [47., 0.]),
        line("DL", "08", [47., 0.], [5., 0.]),
        text("VT", "08", "%%c100", [20., 1.], 3.5, 0.),
        arc("BA", "02", [0., 0.], 50., -10., 10.),
        text("ST", "11", "此直径以内不加工", [15., -3.], 3.5, 0.),
    ];
    let g = run("", &ents);
    let d = expr_with(&g, "VT", "dimension_diameter")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "VT")));
    let r = expr_with(&g, "ST", "process_requirement")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "ST")));
    assert!(
        names(r, "callout_rows", "bounded_requirement") && has(r, &["DC-G1-047", "DC-G4-067"]),
        "{:?}",
        r.concept_ids
    );
    assert!(
        role(&g, r, "BA").contains("extension_line")
            && role(&g, d, "BA").contains("extension_line")
    );
    for s in ["VT", "AR", "DL"] {
        assert!(
            !role(&g, r, s).is_empty(),
            "{s} shared with the requirement"
        );
    }
    assert!(
        r.evidence_ids
            .iter()
            .any(|x| x == &format!("bounded_by_dimension:{}", d.id)),
        "{:?}",
        r.evidence_ids
    );
    covered(&g);
}

// ---------------------------------------------------------------- leader notes

#[test]
fn leader_note_with_an_arrow_on_the_outline() {
    let ents = vec![
        text("NT", "11", "非加工面", [40., 51.], 3.5, 0.),
        line("SH", "02", [38., 50.], [56., 50.]),
        line("LD", "02", [56., 50.], [70., 30.]),
        solid("AR", "02", [71.72, 27.54], [70., 30.]),
        line("OL", "01", [60., 27.54], [90., 27.54]),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "NT", "leader_note").unwrap_or_else(|| panic!("{:?}", kinds(&g, "NT")));
    assert_eq!(e.label, "非加工面");
    assert!(
        names(e, "leader_note", "arrow_terminator") && has(e, &["DC-G1-028", "DC-G1-096"]),
        "{:?}",
        e.concept_ids
    );
    for (s, r) in [
        ("SH", "leader_line"),
        ("LD", "leader_line"),
        ("AR", "arrowhead"),
    ] {
        assert!(role(&g, e, s).contains(r), "{s}");
    }
    assert!(
        !e.concept_ids.iter().any(|c| c == "DC-G9-002"),
        "not a technical requirement item"
    );
}

#[test]
fn leader_note_without_terminator_ending_at_a_line() {
    let ents = vec![
        text("NT", "11", "3凸台", [40., 51.], 3.5, 0.),
        text("NU", "11", "均布", [40., 45.5], 3.5, 0.),
        line("SH", "02", [38., 50.], [52., 50.]),
        line("LD", "02", [52., 50.], [70., 30.]),
        arc("HC", "04", [70., 0.], 29.7, 60., 120.),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "NT", "leader_note").unwrap_or_else(|| panic!("{:?}", kinds(&g, "NT")));
    assert_eq!(e.label, "3凸台 均布");
    assert!(
        names(e, "leader_note", "no_terminator") && has(e, &["DC-G1-098"]),
        "{:?}",
        e.evidence_ids
    );
    assert!(role(&g, e, "NU").contains("note_text"));
}

#[test]
fn underlined_text_without_a_leader_is_not_a_leader_note() {
    let g = run(
        "",
        &[
            text("NT", "11", "非加工面", [40., 51.], 3.5, 0.),
            line("SH", "02", [38., 50.], [56., 50.]),
        ],
    );
    assert!(!g.expressions.iter().any(|e| e.kind == "leader_note"));
}

// ---------------------------------------------------------------- technical requirement items

#[test]
fn requirement_items_numbered_at_the_head_of_their_text() {
    let ents = vec![
        text("HD", "11", "技术要求", [0., 100.], 5., 0.),
        text("I1", "11", "1、未注倒角C1，", [0., 95.], 3.5, 0.),
        text("I1B", "11", "锐边倒钝；", [7., 90.], 3.5, 0.),
        text("I2", "11", "2、去除毛刺。", [0., 85.], 3.5, 0.),
    ];
    let g = run("", &ents);
    let first = exprs_of(&g, "I1")
        .into_iter()
        .find(|e| e.family == "note")
        .expect("item 1");
    let second = exprs_of(&g, "I2")
        .into_iter()
        .find(|e| e.family == "note")
        .expect("item 2");
    assert_ne!(first.id, second.id);
    assert_eq!(first.label, "1、未注倒角C1，锐边倒钝；");
    assert!(exprs_of(&g, "I1B").iter().any(|e| e.id == first.id));
    assert_eq!(second.label, "2、去除毛刺。");
}

#[test]
fn ruled_table_texts_are_not_requirement_items() {
    let mut ents = vec![
        text("HD", "11", "技术要求", [0., 100.], 5., 0.),
        text("I1", "11", "1、未注倒角C1。", [0., 95.], 3.5, 0.),
    ];
    for (i, y) in [80., 85., 90., 95.].iter().enumerate() {
        ents.push(line(&format!("TH{i}"), "02", [60., *y], [90., *y]));
    }
    for (i, x) in [60., 70., 80., 90.].iter().enumerate() {
        ents.push(line(&format!("TV{i}"), "02", [*x, 80.], [*x, 95.]));
    }
    ents.extend([
        text("T1", "11", "代号", [61., 91.], 3., 0.),
        text("T2", "11", "L2", [71., 91.], 3., 0.),
        text("T3", "11", "A", [61., 86.], 3., 0.),
        text("T4", "11", "12", [71., 86.], 3., 0.),
    ]);
    let g = run("", &ents);
    for t in ["T1", "T2", "T3", "T4"] {
        assert!(
            !exprs_of(&g, t).iter().any(|e| e.family == "note"),
            "{t} in a requirement item"
        );
    }
    assert!(exprs_of(&g, "I1").iter().any(|e| e.family == "note"));
}

// ---------------------------------------------------------------- the indication for most surfaces

/// A texture symbol not allowing material removal: two arms at 60° and the inscribed circle.
fn texture_v() -> Vec<String> {
    vec![
        line("A1", "11", [15., 0.], [12.4, 4.503]),
        line("A2", "11", [15., 0.], [21.25, 10.825]),
        circle("CI", "11", [15., 2.5], 1.2),
    ]
}

#[test]
fn most_surfaces_word_and_valueless_symbol_in_one_block() {
    let mut inner = texture_v();
    inner.push(text("W", "11", "其余", [0., 0.], 3.5, 0.));
    let blk = blocks(&[("GEN", inner)]);
    let g = run(&blk, &[insert("GN", "11", "GEN", [100., 100.], 1.)]);
    let e =
        expr_with(&g, "GN", "surface_roughness").unwrap_or_else(|| panic!("{:?}", kinds(&g, "GN")));
    // the symbol without a value is named by its form (no material removal)
    assert_eq!(e.label, "其余 不去除材料");
    assert!(e.qualifiers.iter().any(|q| q == "general_requirement"));
    assert!(
        names(e, "surface_texture_symbol", "most_surfaces") && has(e, &["DC-G6-006", "DC-G6-063"]),
        "{:?}",
        e.concept_ids
    );
    assert_eq!(e.view_id.as_deref(), Some("view:sheet"));
}

#[test]
fn most_surfaces_word_written_before_a_symbol_block() {
    let blk = blocks(&[("SYM", texture_v())]);
    let g = run(
        &blk,
        &[
            insert("SY", "11", "SYM", [130., 100.], 1.),
            text("W", "11", "其余", [134., 100.], 3.5, 0.),
        ],
    );
    let e =
        expr_with(&g, "SY", "surface_roughness").unwrap_or_else(|| panic!("{:?}", kinds(&g, "SY")));
    // the symbol without a value is named by its form (no material removal)
    assert_eq!(e.label, "其余 不去除材料");
    assert!(e.qualifiers.iter().any(|q| q == "general_requirement"));
    assert!(role(&g, e, "W").contains("note_text"));
    assert!(has(e, &["DC-G6-006"]), "{:?}", e.concept_ids);
}

// ---------------------------------------------------------------- notes in the sheet margin

/// A sheet frame block: the paper edge and the frame line inside it (binding margin on the left).
fn frame_block() -> String {
    blocks(&[(
        "FRM",
        vec![
            lwpoly(
                "PE",
                "02",
                &[[-25., -10.], [410., -10.], [410., 287.], [-25., 287.]],
                true,
            ),
            lwpoly(
                "FL",
                "02",
                &[[0., 0.], [400., 0.], [400., 277.], [0., 277.]],
                true,
            ),
        ],
    )])
}

#[test]
fn text_in_the_binding_margin_is_a_sheet_note() {
    let g = run(
        &frame_block(),
        &[
            insert("FR", "02", "FRM", [0., 0.], 1.),
            text("MT", "11", "质量 2.5kg", [-12., 100.], 3.5, 90.),
        ],
    );
    let e = expr_with(&g, "MT", "title_block").unwrap_or_else(|| panic!("{:?}", kinds(&g, "MT")));
    assert!(
        names(e, "title_block_field", "margin_note") && has(e, &["DC-G9-026", "DC-G1-085"]),
        "{:?}",
        e.concept_ids
    );
}

#[test]
fn dated_change_sentence_outside_the_sheets_is_a_revision_note() {
    let g = run(
        &frame_block(),
        &[
            insert("FR", "02", "FRM", [0., 0.], 1.),
            text("RV", "11", "修改孔距。20210304", [50., -30.], 3.5, 0.),
        ],
    );
    let e = expr_with(&g, "RV", "title_block").unwrap_or_else(|| panic!("{:?}", kinds(&g, "RV")));
    assert!(
        names(e, "title_block_field", "revision_note") && has(e, &["DC-G9-023"]),
        "{:?}",
        e.concept_ids
    );
}

#[test]
fn text_inside_the_frame_is_not_a_margin_note() {
    let g = run(
        &frame_block(),
        &[
            insert("FR", "02", "FRM", [0., 0.], 1.),
            text("MT", "11", "质量 2.5kg", [200., 150.], 3.5, 0.),
        ],
    );
    assert!(!kinds(&g, "MT").contains("title_block"));
}
