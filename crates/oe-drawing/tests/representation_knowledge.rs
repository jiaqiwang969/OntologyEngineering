//! Authored synthetic sources for the drafting representation knowledge cards. Every variant
//! and counter-example named in `knowledge/drawing-representation.toml` has a test here.
//! None of these fixtures is taken from a real drawing: handles, coordinates and layer names
//! are invented, and no expert reference is read.
use oe_contracts::{MdEngineeringExpression, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::{USED_CARDS, discover, drawing_concepts, representation_knowledge};
use std::collections::BTreeSet;

fn ent(kind: &str, id: &str, layer: &str, tags: &str) -> String {
    format!("0\n{kind}\n5\n{id}\n8\n{layer}\n330\nMODEL\n{tags}")
}
fn line(id: &str, layer: &str, a: [f64; 2], b: [f64; 2]) -> String {
    ent("LINE", id, layer, &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]))
}
fn line_c(id: &str, layer: &str, color: i32, a: [f64; 2], b: [f64; 2]) -> String {
    ent("LINE", id, layer, &format!("62\n{color}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]))
}
fn text(id: &str, layer: &str, s: &str, p: [f64; 2], h: f64, rot: f64) -> String {
    ent("TEXT", id, layer, &format!("1\n{s}\n10\n{}\n20\n{}\n40\n{h}\n50\n{rot}\n", p[0], p[1]))
}
fn circle(id: &str, layer: &str, c: [f64; 2], r: f64) -> String {
    ent("CIRCLE", id, layer, &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]))
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
fn solid(id: &str, layer: &str, tip: [f64; 2], base: [f64; 2]) -> String {
    let d = [base[0] - tip[0], base[1] - tip[1]];
    let p = [-d[1] * 0.15, d[0] * 0.15];
    solid3(id, layer, [tip, [base[0] + p[0], base[1] + p[1]], [base[0] - p[0], base[1] - p[1]]])
}
fn lwpoly(id: &str, layer: &str, pts: &[[f64; 2]]) -> String {
    let mut t = format!("90\n{}\n70\n0\n", pts.len());
    for p in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", p[0], p[1]));
    }
    ent("LWPOLYLINE", id, layer, &t)
}
fn lwpoly_w(id: &str, layer: &str, pts: &[([f64; 2], f64, f64)]) -> String {
    let mut t = format!("90\n{}\n70\n0\n", pts.len());
    for (p, s, e) in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n40\n{s}\n41\n{e}\n", p[0], p[1]));
    }
    ent("LWPOLYLINE", id, layer, &t)
}
fn rect(id: &str, layer: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    lwpoly(id, layer, &[[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]])
}
/// HATCH with one edge-defined boundary of line edges and a pattern angle.
fn hatch_edges(id: &str, layer: &str, pts: &[[f64; 2]], angle: f64) -> String {
    let mut t = format!(
        "10\n0\n20\n0\n30\n0\n2\nANSI31\n70\n0\n71\n0\n91\n1\n92\n1\n93\n{}\n",
        pts.len()
    );
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        t.push_str(&format!("72\n1\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]));
    }
    t.push_str(&format!("97\n0\n75\n0\n76\n1\n52\n{angle}\n41\n1\n77\n0\n78\n1\n53\n45\n43\n0\n44\n0\n45\n-1\n46\n1\n79\n0\n98\n0\n"));
    ent("HATCH", id, layer, &t)
}
fn tables() -> String {
    let mut s = String::from("0\nSECTION\n2\nTABLES\n");
    for (name, el) in [
        ("CONTINUOUS", vec![]),
        ("DASHX", vec![2., -1.]),
        ("AXISX", vec![8., -1., 1., -1.]),
        ("PHANTX", vec![8., -1., 1., -1., 1., -1.]),
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
        ("07", 6, "PHANTX"),
        ("08", 3, "CONTINUOUS"),
        ("11", 3, "CONTINUOUS"),
    ] {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n62\n{color}\n6\n{lt}\n"));
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
    discover(bytes.as_bytes()).unwrap()
}
fn blocks(defs: &[(&str, Vec<String>)]) -> String {
    let mut s = String::from("0\nSECTION\n2\nBLOCKS\n");
    for (name, ents) in defs {
        s.push_str(&format!("0\nBLOCK\n2\n{name}\n10\n0\n20\n0\n{}0\nENDBLK\n", ents.concat()));
    }
    s.push_str("0\nENDSEC\n");
    s
}
fn insert(id: &str, layer: &str, name: &str, at: [f64; 2]) -> String {
    ent("INSERT", id, layer, &format!("2\n{name}\n10\n{}\n20\n{}\n41\n1\n42\n1\n50\n0\n", at[0], at[1]))
}
fn kinds(g: &MdEngineeringExpressionGraph, src: &str) -> BTreeSet<String> {
    let ids: BTreeSet<&str> = g.members.iter().filter(|m| m.source_handle == src).map(|m| m.expression_id.as_str()).collect();
    g.expressions.iter().filter(|e| ids.contains(e.id.as_str())).map(|e| e.kind.clone()).collect()
}
fn expr_with<'a>(g: &'a MdEngineeringExpressionGraph, src: &str, kind: &str) -> Option<&'a MdEngineeringExpression> {
    let ids: BTreeSet<&str> = g.members.iter().filter(|m| m.source_handle == src).map(|m| m.expression_id.as_str()).collect();
    g.expressions.iter().find(|e| ids.contains(e.id.as_str()) && e.kind == kind)
}
fn role(g: &MdEngineeringExpressionGraph, e: &MdEngineeringExpression, src: &str) -> BTreeSet<String> {
    g.members.iter().filter(|m| m.expression_id == e.id && m.source_handle == src).map(|m| m.member_role.clone()).collect()
}
fn names(e: &MdEngineeringExpression, card: &str) -> bool {
    e.rule.contains(&format!("card:{card}")) || e.evidence_ids.contains(&format!("card:{card}"))
}
fn covered(g: &MdEngineeringExpressionGraph) {
    for root in &g.source.entities {
        assert!(g.members.iter().any(|m| m.source_handle == root.id), "uncovered {}", root.id);
    }
}

// ---------------------------------------------------------------- datum feature symbol

fn old_datum(prefix: &str, at: [f64; 2], letter: &str) -> Vec<String> {
    let [x, y] = at;
    vec![
        line(&format!("{prefix}BAR"), "01", [x - 3., y + 7.7], [x + 3., y + 7.7]),
        line(&format!("{prefix}CON"), "02", [x, y + 7.7], [x, y + 3.5]),
        circle(&format!("{prefix}CIR"), "02", [x, y], 3.5),
        text(&format!("{prefix}LET"), "11", letter, [x - 0.8, y - 1.75], 3.5, 0.),
    ]
}

#[test]
fn datum_old_circle_bar_loose_is_one_symbol() {
    let mut ents = old_datum("D", [20., 0.5], "A");
    ents.push(line("O1", "01", [0., 9.7], [40., 9.7]));
    ents.push(line("O2", "01", [0., 9.7], [0., 30.]));
    let g = run("", &ents);
    let e = expr_with(&g, "DLET", "datum_symbol").expect("datum symbol");
    assert_eq!(e.label, "A");
    assert!(names(e, "datum_feature_symbol") && e.evidence_ids.contains(&"variant:old_circle_bar".into()));
    assert!(role(&g, e, "DBAR").contains("datum_terminal"));
    assert!(role(&g, e, "DCON").contains("leader_line"));
    assert!(role(&g, e, "DCIR").contains("datum_frame"));
    assert!(!kinds(&g, "DBAR").contains("visible_contour"), "{:?}", kinds(&g, "DBAR"));
    covered(&g);
}

#[test]
fn datum_old_circle_bar_block_is_one_symbol() {
    let blk = blocks(&[(
        "DTMB",
        vec![
            circle("K1", "0", [0., 0.], 3.5),
            text("K2", "0", "B", [-0.8, -1.75], 3.5, 0.),
            line("K3", "0", [0., 3.5], [0., 8.]),
        ],
    )]);
    let g = run(&blk, &[insert("DI", "11", "DTMB", [200., 0.])]);
    let e = expr_with(&g, "DI", "datum_symbol").expect("block datum");
    assert!(names(e, "datum_feature_symbol") && e.evidence_ids.contains(&"encoding:block".into()));
}

#[test]
fn datum_new_square_triangle_loose_is_one_symbol() {
    let ents = vec![
        rect("SQ", "02", 46.5, -3.5, 53.5, 3.5),
        text("SL", "11", "C", [49.2, -1.75], 3.5, 0.),
        line("SC", "02", [50., -3.5], [50., -8.]),
        solid3("ST", "02", [[50., -8.], [47.5, -10.5], [52.5, -10.5]]),
        line("SF", "01", [40., -10.5], [60., -10.5]),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "SL", "datum_symbol").expect("square datum");
    assert!(e.evidence_ids.contains(&"variant:new_square_triangle".into()), "{:?}", e.evidence_ids);
    assert!(role(&g, e, "ST").contains("datum_terminal"));
    assert!(role(&g, e, "SQ").contains("datum_frame"));
}

#[test]
fn circled_letter_without_connector_is_not_a_datum() {
    let ents = vec![circle("VC", "02", [0., 0.], 3.5), text("VL", "11", "E", [-0.8, -1.75], 3.5, 0.)];
    let g = run("", &ents);
    assert!(!g.expressions.iter().any(|e| e.kind == "datum_symbol"));
}

#[test]
fn datum_letter_cell_of_a_frame_is_not_a_datum_symbol() {
    // A frame whose last cell is a square with a datum letter, and whose leader starts from
    // that cell and ends in an arrowhead.
    let (blk, mut ents) = block_frame(vec![line("G1", "0", [1., 2.], [9., 2.]), line("G2", "0", [5., 2.], [5., 9.])], "0.02");
    ents.retain(|e| !e.contains("\nLP\n") && !e.contains("\nC3\n") && !e.contains("\nR\n"));
    ents.push(rect("C3", "02", 126., 100., 136., 110.));
    ents.push(text("R", "11", "A", [130.2, 103.], 4., 0.));
    ents.push(line("LQ", "02", [136., 105.], [145., 105.]));
    ents.push(line("LR", "02", [145., 105.], [145., 92.]));
    ents.push(solid("LA", "02", [145., 89.], [145., 92.]));
    let g = run(&blk, &ents);
    assert!(!g.expressions.iter().any(|e| e.kind == "datum_symbol"), "{:?}", g.expressions.iter().map(|e| &e.kind).collect::<Vec<_>>());
}

#[test]
fn short_thick_line_without_datum_letter_stays_outline() {
    let ents = vec![
        line("SB", "01", [17., 8.2], [23., 8.2]),
        line("SK", "02", [20., 8.2], [20., 4.]),
        circle("SR", "02", [20., 0.5], 3.5),
        line("SO", "01", [23., 8.2], [23., 20.]),
        line("SP", "01", [17., 8.2], [17., 20.]),
    ];
    let g = run("", &ents);
    assert!(!g.expressions.iter().any(|e| e.kind == "datum_symbol"));
    assert!(kinds(&g, "SB").contains("visible_contour"), "{:?}", kinds(&g, "SB"));
}

// ---------------------------------------------------------------- centre hole

fn shaft_end(v: &[String]) -> Vec<String> {
    let mut e = vec![
        line("EF", "01", [100., 0.], [100., 20.]),
        line("ET", "01", [100., 20.], [160., 20.]),
        line("EB", "01", [100., 0.], [160., 0.]),
        line("EX", "05", [95., 10.], [165., 10.]),
    ];
    e.extend(v.iter().cloned());
    e
}
fn designation() -> Vec<String> {
    vec![
        line("NL", "11", [70., 10.], [88., 10.]),
        text("N1", "11", "中心孔 2-B4", [72., 11.], 3.5, 0.),
        text("N2", "11", "GB145-85", [72., 5.5], 3.5, 0.),
    ]
}

#[test]
fn center_hole_keep_symbol_with_designation() {
    let mut v = vec![line("A1", "01", [100., 10.], [94.8, 7.]), line("A2", "01", [100., 10.], [94.8, 13.])];
    v.extend(designation());
    let g = run("", &shaft_end(&v));
    let e = expr_with(&g, "A1", "center_hole").expect("centre hole");
    assert!(names(e, "center_hole_symbol") && e.qualifiers.contains(&"center_hole:keep".into()));
    assert!(e.label.contains("中心孔"), "{}", e.label);
    for (s, r) in [("A1", "symbol_glyph"), ("A2", "symbol_glyph"), ("N1", "note_text"), ("N2", "note_text"), ("NL", "leader_line")] {
        assert!(role(&g, e, s).contains(r), "{s}: {:?}", role(&g, e, s));
    }
    assert!(!kinds(&g, "A1").contains("visible_contour"));
    covered(&g);
}

#[test]
fn center_hole_not_allowed_symbol() {
    let v = vec![
        line("A1", "01", [100., 10.], [94.8, 7.]),
        line("A2", "01", [100., 10.], [94.8, 13.]),
        line("A3", "01", [97., 8.], [97., 12.]),
    ];
    let g = run("", &shaft_end(&v));
    let e = expr_with(&g, "A1", "center_hole").expect("centre hole");
    assert!(e.qualifiers.contains(&"center_hole:not_allowed".into()), "{:?}", e.qualifiers);
    assert!(role(&g, e, "A3").contains("symbol_glyph"));
}

#[test]
fn v_lines_off_axis_are_not_a_center_hole() {
    let v = vec![line("A1", "01", [100., 15.], [94.8, 12.]), line("A2", "01", [100., 15.], [94.8, 18.])];
    let g = run("", &shaft_end(&v));
    assert!(!g.expressions.iter().any(|e| e.kind == "center_hole"));
}

#[test]
fn v_lines_opening_into_the_part_are_not_a_center_hole() {
    let v = vec![line("A1", "01", [100., 10.], [105.2, 7.]), line("A2", "01", [100., 10.], [105.2, 13.])];
    let g = run("", &shaft_end(&v));
    assert!(!g.expressions.iter().any(|e| e.kind == "center_hole"));
}

// ---------------------------------------------------------------- tolerance frames

fn block_frame(glyph: Vec<String>, value: &str) -> (String, Vec<String>) {
    let blk = blocks(&[("GLYX", glyph)]);
    let ents = vec![
        insert("GI", "01", "GLYX", [100., 100.]),
        rect("C1", "02", 100., 100., 110., 110.),
        rect("C2", "02", 110., 100., 126., 110.),
        rect("C3", "02", 126., 100., 132., 110.),
        text("V", "11", value, [111., 103.], 4., 0.),
        text("R", "11", "A", [127.5, 103.], 4., 0.),
        lwpoly_w("LP", "02", &[([100., 105.], 0., 0.), ([90., 105.], 0., 0.), ([90., 83.], 1., 0.), ([90., 80.], 0., 0.)]),
    ];
    (blk, ents)
}
fn position_glyph() -> Vec<String> {
    vec![
        circle("G0", "0", [5., 5.], 2.),
        line("G1", "0", [5., 1.5], [5., 8.5]),
        line("G2", "0", [1.5, 5.], [8.5, 5.]),
    ]
}

#[test]
fn frame_with_block_glyph_is_read() {
    let (blk, ents) = block_frame(vec![line("G1", "0", [1., 2.], [9., 2.]), line("G2", "0", [5., 2.], [5., 9.])], "0.02");
    let g = run(&blk, &ents);
    let e = expr_with(&g, "GI", "gps_perpendicularity").expect("frame");
    assert!(names(e, "gdt_frame_symbol") && e.evidence_ids.contains(&"variant:block_glyph".into()));
}

#[test]
fn frame_with_loose_position_glyph_is_read() {
    let ents = vec![
        circle("P0", "01", [105., 105.], 1.4),
        line("P1", "01", [105., 101.5], [105., 108.5]),
        line("P2", "01", [101.5, 105.], [108.5, 105.]),
        rect("C1", "02", 100., 100., 110., 110.),
        rect("C2", "02", 110., 100., 126., 110.),
        rect("C3", "02", 126., 100., 132., 110.),
        rect("C4", "02", 132., 100., 138., 110.),
        text("V", "11", "%%C0.20", [111., 103.], 4., 0.),
        text("R1", "11", "C", [127.5, 103.], 4., 0.),
        text("R2", "11", "B", [133.5, 103.], 4., 0.),
        line("L1", "02", [138., 105.], [150., 105.]),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "P0", "gps_position").expect("position frame");
    assert!(e.evidence_ids.contains(&"variant:loose_glyph".into()), "{:?}", e.evidence_ids);
    for (s, r) in [("P1", "symbol_glyph"), ("P2", "symbol_glyph"), ("V", "tolerance_value"), ("R1", "datum_reference"), ("R2", "datum_reference")] {
        assert!(role(&g, e, s).contains(r), "{s}: {:?}", role(&g, e, s));
    }
    assert!(e.qualifiers.contains(&"tolerance_zone:cylindrical_or_circular".into()));
    assert!(!kinds(&g, "V").contains("dimension_diameter"));
    assert!(!kinds(&g, "P1").contains("visible_contour"));
    covered(&g);
}

#[test]
fn cells_without_a_glyph_are_not_a_frame() {
    let ents = vec![
        rect("C1", "02", 100., 100., 110., 110.),
        rect("C2", "02", 110., 100., 126., 110.),
        text("T1", "11", "12", [101., 103.], 4., 0.),
        text("T2", "11", "34", [111., 103.], 4., 0.),
    ];
    let g = run("", &ents);
    assert!(!g.expressions.iter().any(|e| e.kind.starts_with("gps_") || e.kind == "geometric_tolerance"));
}

#[test]
fn phi_in_frame_value_is_a_cylindrical_zone() {
    let (blk, ents) = block_frame(position_glyph(), "%%C0.1");
    let g = run(&blk, &ents);
    let e = expr_with(&g, "GI", "gps_position").expect("position frame");
    assert!(e.qualifiers.contains(&"tolerance_zone:cylindrical_or_circular".into()), "{:?}", e.qualifiers);
    assert!(names(e, "tolerance_zone_modifier"));
    assert!(!kinds(&g, "V").contains("dimension_diameter"));
}

#[test]
fn sphi_in_frame_value_is_a_spherical_zone() {
    let (blk, ents) = block_frame(position_glyph(), "S%%C0.1");
    let g = run(&blk, &ents);
    let e = expr_with(&g, "GI", "gps_position").expect("position frame");
    assert!(e.qualifiers.contains(&"tolerance_zone:spherical".into()), "{:?}", e.qualifiers);
}

#[test]
fn phi_text_with_dimension_lines_stays_a_diameter() {
    let ents = vec![
        line("V1", "08", [150., 0.], [150., 35.]),
        solid("VA", "08", [150., 0.], [150., 3.]),
        solid("VB", "08", [150., 35.], [150., 32.]),
        text("VT", "08", "%%C28", [149.4, 12.], 2.5, 90.),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "VT", "dimension_diameter").expect("diameter");
    assert!(!e.qualifiers.iter().any(|q| q.starts_with("tolerance_zone")));
}

// ---------------------------------------------------------------- special marks

fn linear20(t: &str) -> Vec<String> {
    vec![
        line("H1", "08", [100., 50.], [108., 50.]),
        line("H2", "08", [112., 50.], [120., 50.]),
        solid("HA", "08", [100., 50.], [103., 50.]),
        solid("HB", "08", [120., 50.], [117., 50.]),
        text("HT", "08", t, [108.6, 50.4], 2.5, 0.),
        line("HE1", "02", [100., 40.], [100., 52.]),
        line("HE2", "02", [120., 40.], [120., 52.]),
    ]
}

#[test]
fn star_inside_dimension_text_is_kept() {
    let g = run("", &linear20("☆20"));
    let e = expr_with(&g, "HT", "dimension_linear").expect("linear");
    assert!(e.label.starts_with('☆') && e.qualifiers.contains(&"key_characteristic".into()));
    assert!(e.evidence_ids.contains(&"variant:inline".into()));
}

#[test]
fn standalone_star_before_linear_dimension_joins_it() {
    let mut ents = linear20("20");
    ents.push(text("ST", "0", "☆", [105.8, 50.4], 2.5, 0.));
    let g = run("", &ents);
    let e = expr_with(&g, "HT", "dimension_linear").expect("linear");
    assert!(role(&g, e, "ST").contains("special_mark"), "{:?} {:?}", kinds(&g, "ST"), e);
    assert!(e.label.starts_with('☆') && e.qualifiers.contains(&"key_characteristic".into()));
    assert!(names(e, "special_characteristic_mark"));
    covered(&g);
}

#[test]
fn standalone_star_before_rotated_diameter_text_joins_it() {
    let ents = vec![
        line("V1", "08", [150., 0.], [150., 35.]),
        solid("VA", "08", [150., 0.], [150., 3.]),
        solid("VB", "08", [150., 35.], [150., 32.]),
        text("VT", "08", "%%C28", [149.4, 12.], 2.5, 90.),
        text("VS", "0", "☆", [149.4, 8.6], 2.5, 90.),
    ];
    let g = run("", &ents);
    let e = expr_with(&g, "VT", "dimension_diameter").expect("diameter");
    assert!(role(&g, e, "VS").contains("special_mark"), "{:?}", kinds(&g, "VS"));
    assert!(e.label.starts_with('☆'));
}

#[test]
fn star_far_from_any_dimension_is_not_attached() {
    let mut ents = linear20("20");
    ents.push(text("ST", "0", "☆", [60., 90.], 2.5, 0.));
    let g = run("", &ents);
    assert!(!g.members.iter().any(|m| m.member_role == "special_mark"));
}

// ---------------------------------------------------------------- angle unit

fn native_dimension(id: &str, block: &str, dtype: i32, text: &str, measured: f64) -> (Vec<String>, String) {
    let children = vec![
        line(&format!("{id}1"), "0", [0., 40.], [30., 40.]),
        line(&format!("{id}2"), "0", [0., 30.], [0., 42.]),
        line(&format!("{id}3"), "0", [30., 30.], [30., 42.]),
        solid(&format!("{id}4"), "0", [0., 40.], [2., 40.]),
        solid(&format!("{id}5"), "0", [30., 40.], [28., 40.]),
        ent("MTEXT", &format!("{id}6"), "0", "1\nshown\n10\n15\n20\n41\n40\n2.5\n71\n8\n"),
    ];
    let dim = ent(
        "DIMENSION",
        id,
        "0",
        &format!("2\n{block}\n10\n30\n20\n40\n11\n15\n21\n41\n70\n{dtype}\n1\n{text}\n13\n0\n23\n30\n14\n30\n24\n30\n42\n{measured}\n50\n0\n"),
    );
    (children, dim)
}

fn one_dimension(dtype: i32, t: &str) -> MdEngineeringExpressionGraph {
    let (ch, dim) = native_dimension("DA", "*DX1", dtype, t, 30.);
    run(&blocks(&[("*DX1", ch)]), &[dim])
}

#[test]
fn angle_without_unit_gets_a_degree_sign() {
    let g = one_dimension(162, "30");
    let e = expr_with(&g, "DA", "dimension_angle").expect("angle");
    assert_eq!(e.label, "30°");
    assert!(e.qualifiers.contains(&"unit_supplied:degree".into()));
    assert!(e.evidence_ids.contains(&"source_text:30".into()) && names(e, "angle_unit"));
}

#[test]
fn angle_with_unit_is_unchanged() {
    let g = one_dimension(162, "45%%d");
    let e = expr_with(&g, "DA", "dimension_angle").expect("angle");
    assert_eq!(e.label, "45°");
    assert!(!e.qualifiers.contains(&"unit_supplied:degree".into()));
}

#[test]
fn linear_number_gets_no_degree_sign() {
    let g = one_dimension(160, "30");
    let e = expr_with(&g, "DA", "dimension_linear").expect("linear");
    assert_eq!(e.label, "30");
}

// ---------------------------------------------------------------- leaders to outline extensions

fn frame_with_outline(outline_to: f64, drawn_extension: bool) -> MdEngineeringExpressionGraph {
    let (blk, mut ents) = block_frame(vec![line("G1", "0", [1., 2.], [9., 2.]), line("G2", "0", [5., 2.], [5., 9.])], "0.02");
    ents.push(line("OT", "01", [20., 80.], [outline_to, 80.]));
    ents.push(line("OL", "01", [20., 40.], [20., 80.]));
    ents.push(line("OR", "01", [outline_to, 40.], [outline_to, 80.]));
    ents.push(line("OB", "01", [20., 40.], [outline_to, 40.]));
    if drawn_extension {
        ents.push(line("XL", "02", [outline_to + 0.5, 80.], [90., 80.]));
    }
    run(&blk, &ents)
}

#[test]
fn frame_leader_to_a_drawn_extension_line() {
    let g = frame_with_outline(70., true);
    let e = expr_with(&g, "GI", "gps_perpendicularity").expect("frame");
    assert!(role(&g, e, "XL").contains("extension_line"), "{:?}", kinds(&g, "XL"));
    assert!(e.evidence_ids.contains(&"variant:drawn".into()) && names(e, "gdt_leader_extension"));
}

#[test]
fn frame_leader_without_extension_gets_a_constructed_one() {
    let g = frame_with_outline(70., false);
    let e = expr_with(&g, "GI", "gps_perpendicularity").expect("frame");
    assert!(e.evidence_ids.iter().any(|x| x.starts_with("constructed_extension:OT:")), "{:?}", e.evidence_ids);
    assert!(e.qualifiers.contains(&"leader_to_outline_extension".into()));
}

#[test]
fn frame_leader_on_the_outline_needs_no_extension() {
    let g = frame_with_outline(95., false);
    let e = expr_with(&g, "GI", "gps_perpendicularity").expect("frame");
    assert!(!e.evidence_ids.iter().any(|x| x.starts_with("constructed_extension") || x == "variant:drawn"), "{:?}", e.evidence_ids);
}

// ---------------------------------------------------------------- section lines

fn sectioned(extra: Vec<String>) -> MdEngineeringExpressionGraph {
    let mut ents = vec![
        line("RB", "01", [0., 0.], [20., 0.]),
        line("RR", "01", [20., 0.], [20., 10.]),
        line("RT", "01", [20., 10.], [0., 10.]),
        line("RL", "01", [0., 10.], [0., 0.]),
        hatch_edges("HH", "02", &[[0., 0.], [20., 0.], [20., 10.], [0., 10.]], 45.),
    ];
    ents.extend(extra);
    run("", &ents)
}

#[test]
fn thick_line_inside_hatch_parallel_to_pattern_is_a_section_line() {
    let g = sectioned(vec![line("TL", "01", [12., 0.], [20., 8.])]);
    let e = expr_with(&g, "TL", "section_hatch").expect("section line");
    assert!(names(e, "section_line_region"));
    assert!(!kinds(&g, "TL").contains("visible_contour"), "{:?}", kinds(&g, "TL"));
}

#[test]
fn thick_line_on_the_hatch_boundary_stays_outline() {
    let g = sectioned(vec![]);
    assert!(kinds(&g, "RB").contains("visible_contour") && !kinds(&g, "RB").contains("section_hatch"));
}

#[test]
fn line_inside_hatch_not_parallel_to_pattern_is_not_a_section_line() {
    let g = sectioned(vec![line("TV", "01", [8., 0.], [8., 10.])]);
    assert!(!kinds(&g, "TV").contains("section_hatch"), "{:?}", kinds(&g, "TV"));
}

// ---------------------------------------------------------------- detail circles and visible edges

fn box_with_top(top: Vec<String>, more: Vec<String>) -> MdEngineeringExpressionGraph {
    let mut ents = vec![
        line("BL", "01", [0., 0.], [0., 20.]),
        line("BB", "01", [0., 0.], [60., 0.]),
        line("BR", "01", [60., 0.], [60., 20.]),
    ];
    ents.extend(top);
    ents.extend(more);
    run("", &ents)
}

#[test]
fn outline_cut_by_a_detail_circle_is_one_line() {
    let g = box_with_top(
        vec![line("T1", "01", [0., 20.], [26., 20.]), line("T2", "01", [34., 20.], [60., 20.])],
        vec![circle("DC", "02", [30., 20.], 4.), text("DN", "11", "I", [36., 26.], 3.5, 0.)],
    );
    assert!(kinds(&g, "DC").contains("view_callout"), "{:?}", kinds(&g, "DC"));
    let spans: Vec<_> = g
        .implied_spans
        .iter()
        .filter(|s| s.cause == "detail_circle_crossing" && [s.from_source_handle.as_str(), s.to_source_handle.as_str()].iter().all(|h| ["T1", "T2"].contains(h)))
        .collect();
    assert_eq!(spans.len(), 1, "{:?}", g.implied_spans);
    assert!(g.structure_ends.iter().filter(|e| e.cause == "detail_circle_crossing").count() >= 2);
    assert!(!g.structure_ends.iter().any(|e| e.cause == "unexplained"));
    let line_of = |h: &str| {
        g.structural_lines
            .iter()
            .position(|l| l.pieces.iter().any(|p| g.members.iter().any(|x| p == &format!("member:{}", x.id) && x.source_handle == h)))
    };
    assert!(line_of("T1").is_some() && line_of("T1") == line_of("T2"));
}

#[test]
fn pieces_ending_on_different_circles_are_not_joined() {
    let g = box_with_top(
        vec![line("T1", "01", [0., 20.], [26., 20.]), line("T2", "01", [34., 20.], [60., 20.])],
        vec![
            circle("DC", "02", [22., 20.], 4.),
            text("DN", "11", "I", [28., 26.], 3.5, 0.),
            circle("DD", "02", [38., 20.], 4.),
            text("DM", "11", "II", [44., 26.], 3.5, 0.),
        ],
    );
    assert!(!g.implied_spans.iter().any(|s| s.cause == "detail_circle_crossing"));
}

fn chamfered_end(extra: Vec<String>) -> MdEngineeringExpressionGraph {
    let mut ents = vec![
        line("CT", "01", [0., 20.], [49., 20.]),
        line("CC1", "01", [49., 20.], [50., 19.]),
        line("CF", "01", [50., 1.], [50., 19.]),
        line("CC2", "01", [50., 1.], [49., 0.]),
        line("CB", "01", [0., 0.], [49., 0.]),
        line("CL", "01", [0., 0.], [0., 20.]),
    ];
    ents.extend(extra);
    run("", &ents)
}

#[test]
fn chamfer_edge_line_in_thin_colour_is_visible_outline() {
    let g = chamfered_end(vec![line_c("CE", "0", 3, [49., 0.], [49., 20.])]);
    let e = expr_with(&g, "CE", "visible_contour").expect("edge line");
    assert!(names(e, "visible_edge_line"));
    // The same edge cut by a detail circle is joined across it.
    let g = chamfered_end(vec![
        line_c("CE1", "0", 3, [49., 20.], [49., 13.]),
        line_c("CE2", "0", 3, [49., 7.], [49., 0.]),
        circle("DC", "02", [49., 10.], 3.),
        text("DN", "11", "I", [55., 15.], 3.5, 0.),
    ]);
    let e = expr_with(&g, "CE1", "visible_contour").expect("edge pieces");
    assert!(names(e, "detail_circle_interruption") && role(&g, e, "CE2").contains("contour_segment"));
    assert!(g.implied_spans.iter().any(|s| s.cause == "detail_circle_crossing"), "{:?}", g.structure_ends);
}

#[test]
fn edge_cut_by_a_circle_labelled_through_a_leader_is_joined() {
    let g = chamfered_end(vec![
        line_c("CE1", "0", 3, [49., 20.], [49., 13.]),
        line_c("CE2", "0", 3, [49., 7.], [49., 0.]),
        circle("DC", "02", [49., 10.], 3.),
        line("DL1", "02", [51.75, 11.6], [70., 25.]),
        line("DL2", "02", [70., 25.], [76., 25.]),
        text("DN", "11", "I", [72., 26.], 3.5, 0.),
    ]);
    let e = expr_with(&g, "CE1", "visible_contour").expect("edge pieces");
    assert!(role(&g, e, "CE2").contains("contour_segment"));
}

#[test]
fn frame_leader_to_an_extension_line_drawn_into_the_outline() {
    let (blk, mut ents) = block_frame(vec![line("G1", "0", [1., 2.], [9., 2.]), line("G2", "0", [5., 2.], [5., 9.])], "0.02");
    ents.push(line("OT", "01", [20., 80.], [70., 80.]));
    ents.push(line("OL", "01", [20., 40.], [20., 80.]));
    ents.push(line("OR", "01", [70., 40.], [70., 80.]));
    ents.push(line("OB", "01", [20., 40.], [70., 40.]));
    ents.push(line("XL", "02", [68., 80.], [90., 80.]));
    let g = run(&blk, &ents);
    let e = expr_with(&g, "GI", "gps_perpendicularity").expect("frame");
    assert!(role(&g, e, "XL").contains("extension_line"), "{:?}", kinds(&g, "XL"));
}

#[test]
fn extension_line_from_a_chamfer_corner_stays_annotation() {
    let g = chamfered_end(vec![line_c("CX", "0", 3, [49., 20.], [49., 32.])]);
    assert!(!kinds(&g, "CX").contains("visible_contour"), "{:?}", kinds(&g, "CX"));
}

// ---------------------------------------------------------------- the cards themselves

#[test]
fn every_card_is_used_and_every_named_test_exists() {
    let k = representation_knowledge().expect("knowledge parses and validates");
    let ids: BTreeSet<&str> = k.cards.iter().map(|c| c.id.as_str()).collect();
    let used: BTreeSet<&str> = USED_CARDS.iter().copied().collect();
    assert_eq!(ids, used, "every card is used by recognition and every used card exists");
    let src = [
        include_str!("representation_knowledge.rs"),
        include_str!("recognition_wave1.rs"),
        include_str!("recognition_corrections.rs"),
        include_str!("view_layout.rs"),
        include_str!("annotation_blocks.rs"),
        include_str!("loose_annotations.rs"),
        include_str!("line_evidence.rs"),
        include_str!("local_section_region.rs"),
        include_str!("drawing_statements.rs"),
        include_str!("line_corrections.rs"),
        include_str!("annotation_constituents.rs"),
        include_str!("section_regions.rs"),
        include_str!("../src/revisions.rs"),
        include_str!("../../oe-drawing-check/tests/drafting_rules.rs"),
    ]
    .concat();
    for c in &k.cards {
        assert!(!c.meaning.is_empty() && !c.components.is_empty() && !c.counter_examples.is_empty() && !c.sources.is_empty(), "{}", c.id);
        let named: Vec<&str> = c
            .variants
            .iter()
            .flat_map(|v| v.tests.iter().map(String::as_str))
            .chain(c.counter_examples.iter().map(|x| x.test.as_str()))
            .collect();
        for t in named {
            assert!(src.contains(&format!("fn {t}(")), "{}: test {t} missing", c.id);
        }
    }
    for c in ["datum_feature_symbol", "center_hole_symbol", "gdt_frame_symbol"] {
        assert!(k.cards.iter().find(|x| x.id == c).unwrap().variants.len() >= 2, "{c}");
    }
}

#[test]
fn every_concept_card_is_complete_and_linked() {
    let groups = drawing_concepts().expect("concept catalogue parses, validates and links both ways");
    assert_eq!(groups.len(), 9);
    let total: usize = groups.iter().map(|g| g.concepts.len()).sum();
    assert!(total >= 900, "catalogue covers the drafting concepts: {total}");
    for g in groups {
        assert!(!g.concepts.is_empty(), "{}", g.group);
        for c in &g.concepts {
            assert!(["high", "medium", "low", "rare"].contains(&c.priority.as_str()), "{}", c.id);
            assert!(c.meaning.chars().count() <= 400 && c.components.iter().all(|x| x.chars().count() <= 400), "{} paraphrase length", c.id);
        }
    }
    let cards = representation_knowledge().unwrap();
    for card in &cards.cards {
        assert!(!card.concepts.is_empty(), "{} names its concepts", card.id);
    }
}
