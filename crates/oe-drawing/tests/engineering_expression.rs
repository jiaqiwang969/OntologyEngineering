//! Authored synthetic sources for the engineering-expression layer. None of these
//! fixtures is taken from a real drawing; handles, coordinates and layer names are
//! invented so that rules cannot depend on any project drawing.
use oe_contracts::MdEngineeringExpressionGraph;
use oe_drawing::engineering_expression::{Pattern, classify_pattern, discover};
use oe_drawing::expression_text::{Lex, decode, dimension_kind, lex, note_kind};

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
fn tables(extra_layers: &[(&str, i32, &str)]) -> String {
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
    for (name, color, lt) in extra_layers {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n62\n{color}\n6\n{lt}\n"));
    }
    s.push_str("0\nENDSEC\n");
    s
}
fn convention_layers() -> Vec<(&'static str, i32, &'static str)> {
    vec![
        ("0", 7, "CONTINUOUS"),
        ("01", 7, "CONTINUOUS"),
        ("02", 3, "CONTINUOUS"),
        ("04", 2, "DASHX"),
        ("05", 1, "AXISX"),
        ("07", 6, "PHANTX"),
        ("08", 3, "CONTINUOUS"),
        ("11", 3, "CONTINUOUS"),
    ]
}
fn drawing(prefix: &str, entities: &[String]) -> Vec<u8> {
    format!(
        "{prefix}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        entities.concat()
    )
    .into_bytes()
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
fn expr_with<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
    kind: &str,
) -> Option<&'a oe_contracts::MdEngineeringExpression> {
    let ids: std::collections::BTreeSet<&str> = g
        .members
        .iter()
        .filter(|m| m.source_handle == src)
        .map(|m| m.expression_id.as_str())
        .collect();
    g.expressions
        .iter()
        .find(|e| ids.contains(e.id.as_str()) && e.kind == kind)
}
fn members_of<'a>(
    g: &'a MdEngineeringExpressionGraph,
    e: &str,
) -> Vec<&'a oe_contracts::MdExpressionMember> {
    g.members.iter().filter(|m| m.expression_id == e).collect()
}
fn covered(g: &MdEngineeringExpressionGraph) {
    for root in &g.source.entities {
        let mut v: Vec<[f64; 2]> = g
            .members
            .iter()
            .filter(|m| m.source_handle == root.id)
            .map(|m| [m.interval[0], m.interval[1]])
            .collect();
        v.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut c = 0f64;
        for [a, b] in v {
            assert!(a <= c + 1e-9, "gap on {}", root.id);
            c = c.max(b);
        }
        assert!(c >= 1. - 1e-9, "uncovered tail on {}", root.id);
    }
}

#[test]
fn text_decoding_reads_stacks_and_lexical_classes() {
    let d = decode("18\u{e1d6}+0.1\u{e1da}\u{e1d7} 0\u{e1d9}");
    assert_eq!(d.main, "18");
    assert_eq!(d.upper.as_deref(), Some("+0.1"));
    assert_eq!(d.lower.as_deref(), Some("0"));
    let m = decode("\\A0;<>{\\H0.700000x;\\S 0^ -0.3;}");
    assert_eq!(m.upper.as_deref(), Some("0"));
    assert_eq!(m.lower.as_deref(), Some("-0.3"));
    assert_eq!(lex(&decode("%%C47.5").text), Lex::Diameter);
    assert_eq!(lex("M10"), Lex::Thread);
    assert_eq!(lex("-32UNF-2B"), Lex::Thread);
    assert_eq!(lex("1.5x45°"), Lex::Chamfer);
    assert_eq!(lex("3.5×15°"), Lex::Chamfer);
    assert_eq!(lex("15°"), Lex::Angle);
    assert_eq!(lex("R0.8"), Lex::Radius);
    assert_eq!(lex("II"), Lex::Roman);
    assert_eq!(lex("5:1"), Lex::Scale);
    assert_eq!(lex(" 3."), Lex::ListNumber);
    assert_eq!(lex("4、"), Lex::ListNumber);
    assert_eq!(lex("攻深16"), Lex::ThreadDepth);
    assert_eq!(dimension_kind("(2.5)"), Some("dimension_linear"));
    assert_eq!(
        note_kind("零件整体淬火后回火，硬度HRC40~45"),
        "material_heat"
    );
    assert_eq!(
        note_kind("锐边去毛刺，外露表面涂防锈油"),
        "protection_requirement"
    );
    assert_eq!(
        note_kind("两端中心孔配作，键槽对称于轴线"),
        "process_requirement"
    );
    assert_eq!(note_kind("未注倒角均为C0.5"), "technical_general");
}

#[test]
fn actual_dash_elements_decide_patterns() {
    assert_eq!(classify_pattern(&[]), Pattern::Continuous);
    assert_eq!(classify_pattern(&[2., -1.]), Pattern::Dashed);
    assert_eq!(classify_pattern(&[8., -1., 1., -1.]), Pattern::DashDot);
    assert_eq!(classify_pattern(&[8., -1., 0., -1.]), Pattern::DashDot);
    assert_eq!(
        classify_pattern(&[8., -1., 1., -1., 1., -1.]),
        Pattern::DashDoubleDot
    );
    assert_eq!(
        classify_pattern(&[8., -1., 1., -1., 8., -1., 1., -1.]),
        Pattern::DashDot
    );
    assert_eq!(classify_pattern(&[0., -1.]), Pattern::Unknown);
}

fn native_dimension(
    id: &str,
    block: &str,
    dtype: i32,
    text: &str,
    measured: f64,
) -> (String, String) {
    let children = [
        line(&format!("{id}1"), "0", [0., 40.], [30., 40.]),
        line(&format!("{id}2"), "0", [0., 30.], [0., 42.]),
        line(&format!("{id}3"), "0", [30., 30.], [30., 42.]),
        solid(&format!("{id}4"), "0", [0., 40.], [2., 40.]),
        solid(&format!("{id}5"), "0", [30., 40.], [28., 40.]),
        ent(
            "MTEXT",
            &format!("{id}6"),
            "0",
            "1\nshown\n10\n15\n20\n41\n40\n2.5\n71\n8\n",
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
            "2\n{block}\n10\n30\n20\n40\n11\n15\n21\n41\n70\n{dtype}\n1\n{text}\n13\n0\n23\n30\n14\n30\n24\n30\n42\n{measured}\n50\n0\n"
        ),
    );
    (blk, dim)
}

#[test]
fn display_text_decides_dimension_kind_over_native_enum() {
    let (b1, d1) = native_dimension("DA", "*DX1", 160, "%%C47.5", 47.5);
    let (b2, d2) = native_dimension("DB", "*DX2", 163, "M10", 8.);
    let (b3, d3) = native_dimension("DC", "*DX3", 32, "<>%%P0.05", 74.);
    let blocks = format!("0\nSECTION\n2\nBLOCKS\n{b1}{b2}{b3}0\nENDSEC\n");
    let g = discover(&drawing(
        &format!("{}{}", tables(&convention_layers()), blocks),
        &[d1, d2, d3],
    ))
    .unwrap();
    assert!(
        kinds(&g, "DA").contains(&"dimension_diameter".into()),
        "{:?}",
        kinds(&g, "DA")
    );
    assert!(kinds(&g, "DB").contains(&"thread_note".into()));
    assert!(kinds(&g, "DC").contains(&"dimension_linear".into()));
    let e = expr_with(&g, "DC", "dimension_linear").unwrap();
    assert_eq!(e.label, "74±0.05");
    assert!(e.qualifiers.contains(&"toleranced".into()));
    let roles: Vec<String> = members_of(&g, &expr_with(&g, "DA", "dimension_diameter").unwrap().id)
        .iter()
        .map(|m| m.member_role.clone())
        .collect();
    for r in [
        "native_dimension",
        "dimension_line",
        "extension_line",
        "arrowhead",
        "dimension_text",
    ] {
        assert!(roles.contains(&r.into()), "{r} missing in {roles:?}");
    }
    covered(&g);
}

#[test]
fn exploded_dimensions_reassemble_split_lines_texts_and_extensions() {
    let l = convention_layers();
    let ents = vec![
        // Horizontal 20 with the dimension line split around its text.
        line("H1", "08", [100., 50.], [108., 50.]),
        line("H2", "08", [112., 50.], [120., 50.]),
        solid("HA", "08", [100., 50.], [103., 50.]),
        solid("HB", "08", [120., 50.], [117., 50.]),
        text("HT", "08", "20", [108.6, 50.4], 2.5, 0.),
        line("HE1", "02", [100., 40.], [100., 52.]),
        line("HE2", "02", [120., 40.], [120., 52.]),
        // Vertical diameter, text rotated with the dimension line.
        line("V1", "08", [150., 0.], [150., 35.]),
        solid("VA", "08", [150., 0.], [150., 3.]),
        solid("VB", "08", [150., 35.], [150., 32.]),
        text(
            "VT",
            "08",
            "%%C28\u{e1d6}-0.02\u{e1da}\u{e1d7}-0.08\u{e1d9}",
            [149.4, 12.],
            2.5,
            90.,
        ),
    ];
    let g = discover(&drawing(&tables(&l), &ents)).unwrap();
    let e = expr_with(&g, "HT", "dimension_linear").expect("linear dimension");
    let srcs: Vec<String> = members_of(&g, &e.id)
        .iter()
        .map(|m| m.source_handle.clone())
        .collect();
    for s in ["H1", "H2", "HA", "HB", "HT", "HE1", "HE2"] {
        assert!(srcs.contains(&s.into()), "{s} not in {srcs:?}");
    }
    let d = expr_with(&g, "VT", "dimension_diameter").expect("diameter dimension");
    assert!(d.qualifiers.contains(&"toleranced".into()));
    let srcs: Vec<String> = members_of(&g, &d.id)
        .iter()
        .map(|m| m.source_handle.clone())
        .collect();
    for s in ["V1", "VA", "VB", "VT"] {
        assert!(srcs.contains(&s.into()));
    }
    covered(&g);
}

#[test]
fn thin_three_quarter_arc_concentric_with_circle_is_thread_convention() {
    let ents = vec![
        ent("CIRCLE", "C1", "01", "10\n50\n20\n50\n40\n3\n"),
        ent("ARC", "A1", "02", "10\n50\n20\n50\n40\n4\n50\n0\n51\n270\n"),
        ent("ARC", "A2", "02", "10\n80\n20\n50\n40\n4\n50\n0\n51\n180\n"),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    assert_eq!(g.layer_convention, "gbt14665_2012_table6_consistent");
    assert!(kinds(&g, "A1").contains(&"thread_convention".into()));
    assert!(!kinds(&g, "A2").contains(&"thread_convention".into()));
    assert!(kinds(&g, "C1").contains(&"visible_contour".into()));
    let m = g
        .members
        .iter()
        .find(|m| m.source_handle == "A1" && m.member_role == "thread_line")
        .unwrap();
    assert_eq!(m.line_type, "thin_continuous");
}

#[test]
fn wavy_polyline_and_chained_segments_are_break_lines_but_smooth_curves_are_not() {
    let wave: Vec<[f64; 2]> = (0..30)
        .map(|i| [10. + (i as f64 * 0.6).sin() * 0.8, i as f64])
        .collect();
    let smooth: Vec<[f64; 2]> = (0..12)
        .map(|i| {
            [
                40. + (i as f64 * 0.1).cos() * 5.,
                (i as f64 * 0.1).sin() * 5.,
            ]
        })
        .collect();
    let mut ents = vec![lwpoly("W1", "02", &wave), lwpoly("S1", "02", &smooth)];
    let chain: Vec<[f64; 2]> = (0..13)
        .map(|i| [70. + i as f64 * 0.3, if i % 2 == 0 { 0. } else { 0.25 }])
        .collect();
    for (i, w) in chain.windows(2).enumerate() {
        ents.push(line(&format!("Z{i:02}"), "03X", w[0], w[1]));
    }
    ents.push(text("T0", "11", "scale reference", [0., -20.], 3.5, 0.));
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    assert!(kinds(&g, "W1").contains(&"break_line".into()));
    assert!(!kinds(&g, "S1").contains(&"break_line".into()));
    assert!(
        kinds(&g, "Z00").contains(&"break_line".into()),
        "{:?}",
        kinds(&g, "Z00")
    );
    let m = g.members.iter().find(|m| m.source_handle == "W1").unwrap();
    assert_eq!(m.line_type, "wavy");
}

#[test]
fn shared_shoulder_line_keeps_only_span_between_junctions_minus_thicker_coincident_source() {
    let ents = vec![
        line("A", "0", [0., 0.], [10., 0.]),
        line("L", "0", [10., -0.02], [10., 12.5]),
        line("K", "01", [10., 3.], [10., 30.]),
        line("B", "01", [10., 30.], [0., 30.]),
        line("D", "0", [0., 30.], [0., 0.]),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let vis: Vec<_> = g
        .members
        .iter()
        .filter(|m| m.source_handle == "L" && m.member_role == "contour_segment")
        .collect();
    assert_eq!(vis.len(), 1, "{vis:?}");
    let len = (vis[0].interval[1] - vis[0].interval[0]) * 12.52;
    assert!((len - 3.0).abs() < 1e-6, "structural length {len}");
    assert!(g.members.iter().any(|m| m.source_handle == "L"
        && m.member_role == "coincident_source"
        && m.evidence_ids.iter().any(|e| e == "coincident_with:K")));
    for full in ["A", "K", "B", "D"] {
        assert!(
            g.members.iter().any(|m| m.source_handle == full
                && m.member_role == "contour_segment"
                && m.interval == vec![0., 1.]),
            "{full}"
        );
    }
    covered(&g);
}

#[test]
fn layer_names_count_only_when_their_linetypes_are_consistent() {
    let mut bad = convention_layers();
    bad.retain(|l| l.0 != "04");
    bad.push(("04", 2, "CONTINUOUS"));
    let ents = vec![
        line("X", "04", [0., 0.], [10., 0.]),
        line("Y", "01", [0., 5.], [10., 5.]),
    ];
    let g = discover(&drawing(&tables(&bad), &ents)).unwrap();
    assert_eq!(g.layer_convention, "not_qualified");
    let style = g
        .line_styles
        .iter()
        .find(|s| s.source_handle == "Y")
        .unwrap();
    assert_eq!(style.line_type, "continuous");
    // Renamed layers with real patterns still classify by the pattern itself.
    let renamed = vec![("P", 1, "AXISX"), ("Q", 2, "DASHX")];
    let ents = vec![
        line("C", "P", [0., 0.], [10., 0.]),
        line("H", "Q", [0., 2.], [10., 2.]),
    ];
    let g = discover(&drawing(&tables(&renamed), &ents)).unwrap();
    assert!(kinds(&g, "C").contains(&"center_axis".into()));
    assert!(kinds(&g, "H").contains(&"hidden_contour".into()));
}

#[test]
fn view_titles_attach_to_the_region_below_and_the_largest_untitled_region_is_principal() {
    let mut ents = Vec::new();
    for (p, x0, y0) in [("M", 0., 0.), ("S", 200., 0.)] {
        let w = if p == "M" { 80. } else { 30. };
        ents.push(line(&format!("{p}1"), "01", [x0, y0], [x0 + w, y0]));
        ents.push(line(
            &format!("{p}2"),
            "01",
            [x0 + w, y0],
            [x0 + w, y0 + 30.],
        ));
        ents.push(line(
            &format!("{p}3"),
            "01",
            [x0 + w, y0 + 30.],
            [x0, y0 + 30.],
        ));
        ents.push(line(&format!("{p}4"), "01", [x0, y0 + 30.], [x0, y0]));
    }
    ents.push(text("TA", "11", "B", [210., 40.], 5., 0.));
    ents.push(text("TB", "11", "向", [215., 40.], 5., 0.));
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let principal = g
        .views
        .iter()
        .find(|v| v.kind == "principal")
        .expect("principal view");
    let titled = g
        .views
        .iter()
        .find(|v| v.label == "B向")
        .expect("titled view");
    let vis_m = expr_with(&g, "M1", "visible_contour").unwrap();
    let vis_s = expr_with(&g, "S1", "visible_contour").unwrap();
    assert_eq!(vis_m.view_id.as_deref(), Some(principal.id.as_str()));
    assert_eq!(vis_s.view_id.as_deref(), Some(titled.id.as_str()));
}

#[test]
fn exploded_symmetry_frame_collects_glyph_cells_value_datum_and_leader() {
    let glyph = [
        line("G1", "0", [1., 2.], [9., 2.]),
        line("G2", "0", [0., 5.], [10., 5.]),
        line("G3", "0", [1., 8.], [9., 8.]),
    ];
    let blocks = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nSYMX\n10\n0\n20\n0\n{}0\nENDBLK\n0\nENDSEC\n",
        glyph.concat()
    );
    let ents = vec![
        ent(
            "INSERT",
            "GI",
            "01",
            "2\nSYMX\n10\n100\n20\n100\n41\n1\n42\n1\n50\n0\n",
        ),
        lwpoly(
            "C1",
            "02",
            &[
                [100., 100.],
                [110., 100.],
                [110., 110.],
                [100., 110.],
                [100., 100.],
            ],
        ),
        lwpoly(
            "C2",
            "02",
            &[[110., 100.], [126., 100.], [126., 110.], [110., 110.]],
        ),
        lwpoly(
            "C3",
            "02",
            &[[126., 100.], [132., 100.], [132., 110.], [126., 110.]],
        ),
        text("V", "11", "0.02", [113., 103.], 4., 0.),
        text("R", "11", "D", [127.5, 103.], 4., 0.),
        line("L1", "02", [100., 105.], [90., 105.]),
        line("L2", "02", [90., 105.], [90., 80.]),
        solid("AR", "02", [90., 80.], [90., 83.]),
    ];
    let g = discover(&drawing(
        &format!("{}{}", tables(&convention_layers()), blocks),
        &ents,
    ))
    .unwrap();
    let e = expr_with(&g, "GI", "gps_symmetry").expect("symmetry frame");
    let srcs: Vec<String> = members_of(&g, &e.id)
        .iter()
        .map(|m| format!("{}:{}", m.source_handle, m.member_role))
        .collect();
    for s in [
        "GI:symbol_glyph",
        "C1:frame_cell",
        "C2:frame_cell",
        "C3:frame_cell",
        "V:tolerance_value",
        "R:datum_reference",
        "L1:leader_line",
        "L2:leader_line",
        "AR:arrowhead",
    ] {
        assert!(srcs.contains(&s.into()), "{s} not in {srcs:?}");
    }
}

#[test]
fn numbered_technical_requirements_become_topic_items() {
    let ents = vec![
        text("H", "11", "技术要求", [100., 100.], 7., 0.),
        text("N1", "11", " 1.", [90., 88.], 5., 0.),
        text("I1", "11", "淬火后回火至HRC40~45；", [100., 88.], 5., 0.),
        text("N2", "11", " 2.", [90., 78.], 5., 0.),
        text("I2", "11", "未注倒角均为C0.5；", [100., 78.], 5., 0.),
        text("N3", "11", " 3.", [90., 68.], 5., 0.),
        text("I3", "11", "锐边去毛刺，外露表面", [100., 68.], 5., 0.),
        text("I4", "11", "涂防锈油。", [100., 58.], 5., 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    assert!(kinds(&g, "H").contains(&"technical_general".into()));
    assert!(kinds(&g, "I1").contains(&"material_heat".into()));
    assert!(kinds(&g, "N1").contains(&"material_heat".into()));
    assert!(kinds(&g, "I2").contains(&"technical_general".into()));
    assert!(kinds(&g, "I3").contains(&"protection_requirement".into()));
    assert!(kinds(&g, "I4").contains(&"protection_requirement".into()));
    let e = expr_with(&g, "I3", "protection_requirement").unwrap();
    assert_eq!(e.view_id.as_deref(), Some("view:sheet"));
}

fn lwpoly_w(id: &str, layer: &str, pts: &[([f64; 2], f64, f64)]) -> String {
    let mut t = format!("90\n{}\n70\n0\n", pts.len());
    for (p, sw, ew) in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n40\n{sw}\n41\n{ew}\n", p[0], p[1]));
    }
    ent("LWPOLYLINE", id, layer, &t)
}
fn polyline(id: &str, layer: &str, flags: i32, verts: &[([f64; 2], i32)]) -> String {
    let mut s = ent(
        "POLYLINE",
        id,
        layer,
        &format!("66\n1\n10\n0\n20\n0\n70\n{flags}\n"),
    );
    for (i, (p, f)) in verts.iter().enumerate() {
        s.push_str(&ent(
            "VERTEX",
            &format!("{id}V{i}"),
            layer,
            &format!("10\n{}\n20\n{}\n70\n{f}\n", p[0], p[1]),
        ));
    }
    s.push_str(&ent("SEQEND", &format!("{id}E"), layer, ""));
    s
}
fn roles_of(g: &MdEngineeringExpressionGraph, e: &str) -> Vec<String> {
    members_of(g, e)
        .iter()
        .map(|m| format!("{}:{}", m.source_handle, m.member_role))
        .collect()
}
fn symmetry_frame_entities() -> (String, Vec<String>) {
    let glyph = [
        line("G1", "0", [1., 2.], [9., 2.]),
        line("G2", "0", [0., 5.], [10., 5.]),
        line("G3", "0", [1., 8.], [9., 8.]),
    ];
    let blocks = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nSYMX\n10\n0\n20\n0\n{}0\nENDBLK\n0\nENDSEC\n",
        glyph.concat()
    );
    let ents = vec![
        ent(
            "INSERT",
            "GI",
            "01",
            "2\nSYMX\n10\n100\n20\n100\n41\n1\n42\n1\n50\n0\n",
        ),
        lwpoly(
            "C1",
            "02",
            &[
                [100., 100.],
                [110., 100.],
                [110., 110.],
                [100., 110.],
                [100., 100.],
            ],
        ),
        lwpoly(
            "C2",
            "02",
            &[[110., 100.], [126., 100.], [126., 110.], [110., 110.]],
        ),
        lwpoly(
            "C3",
            "02",
            &[[126., 100.], [132., 100.], [132., 110.], [126., 110.]],
        ),
        text("V", "11", "0.02", [113., 103.], 4., 0.),
        text("R", "11", "D", [127.5, 103.], 4., 0.),
    ];
    (blocks, ents)
}

#[test]
fn one_terminator_dimension_line_drawn_into_the_head_takes_arrow_extension_and_contour_origin() {
    let ents = vec![
        line("E", "01", [100., 0.], [120., 0.]),
        line("X", "02", [80., 0.], [100., 0.]),
        line("V", "08", [82., 25.], [82., 1.5]),
        solid("AR", "08", [82., 0.], [82., 3.]),
        text("T", "08", "%%C32", [81., 8.], 2.5, 90.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let e = expr_with(&g, "T", "dimension_diameter").expect("half dimension");
    assert_eq!(e.rule, "one_terminator_dimension_line_and_text");
    let roles = roles_of(&g, &e.id);
    for r in [
        "V:dimension_line",
        "AR:arrowhead",
        "X:extension_line",
        "E:extension_origin",
    ] {
        assert!(roles.contains(&r.into()), "{r} not in {roles:?}");
    }
    assert!(
        kinds(&g, "E").contains(&"visible_contour".into()),
        "shared contour keeps its own expression"
    );
    covered(&g);
}

#[test]
fn frame_on_the_dimension_line_of_a_feature_of_size_shares_its_terminators_and_takes_the_stacked_value()
 {
    let (blocks, mut ents) = symmetry_frame_entities();
    ents.extend([
        line("E1", "02", [80., 90.], [80., 107.]),
        line("E2", "02", [90., 90.], [90., 107.]),
        line("D1", "08", [80., 105.], [90., 105.]),
        line("D2", "08", [90., 105.], [100., 105.]),
        solid("A1", "08", [80., 105.], [83., 105.]),
        solid("A2", "08", [90., 105.], [87., 105.]),
        text("T", "11", "10", [112., 110.5], 3., 0.),
    ]);
    let g = discover(&drawing(
        &format!("{}{}", tables(&convention_layers()), blocks),
        &ents,
    ))
    .unwrap();
    let d = expr_with(&g, "T", "dimension_linear").expect("size dimension");
    assert_eq!(
        d.rule,
        "dimension_line_of_feature_of_size_with_attached_frame"
    );
    let roles = roles_of(&g, &d.id);
    for r in [
        "T:dimension_text",
        "A1:arrowhead",
        "A2:arrowhead",
        "D1:dimension_line",
        "D2:dimension_line",
        "E1:extension_line",
        "E2:extension_line",
    ] {
        assert!(roles.contains(&r.into()), "{r} not in {roles:?}");
    }
    let f = expr_with(&g, "GI", "gps_symmetry").expect("frame");
    let froles = roles_of(&g, &f.id);
    for r in [
        "D2:leader_line",
        "A1:arrowhead",
        "A2:arrowhead",
        "D1:dimension_line",
        "E1:extension_line",
        "E2:extension_line",
        "V:tolerance_value",
        "R:datum_reference",
    ] {
        assert!(froles.contains(&r.into()), "{r} not in {froles:?}");
    }
    assert!(
        f.evidence_ids
            .iter()
            .any(|x| x == &format!("applies_to_size_dimension:{}", d.id)),
        "{:?}",
        f.evidence_ids
    );
    assert!(
        d.evidence_ids
            .iter()
            .any(|x| x == &format!("toleranced_by_frame:{}", f.id))
    );
    covered(&g);
}

#[test]
fn hole_callout_rows_continue_across_the_attached_frame() {
    let (blocks, mut ents) = symmetry_frame_entities();
    ents.extend([
        line("D1", "08", [80., 105.], [90., 105.]),
        line("D2", "08", [90., 105.], [100., 105.]),
        solid("A1", "08", [80., 105.], [83., 105.]),
        solid("A2", "08", [90., 105.], [87., 105.]),
        text("T", "11", "4×M6", [110., 110.6], 3., 0.),
        text("U", "11", "攻深10", [110., 94.], 3.5, 0.),
        text("W", "11", "均布", [124., 94.], 3.5, 0.),
    ]);
    let g = discover(&drawing(
        &format!("{}{}", tables(&convention_layers()), blocks),
        &ents,
    ))
    .unwrap();
    let n = expr_with(&g, "T", "thread_note").expect("thread callout");
    let roles = roles_of(&g, &n.id);
    for r in [
        "T:dimension_text",
        "U:note_text",
        "W:note_text",
        "A1:arrowhead",
        "D1:dimension_line",
    ] {
        assert!(roles.contains(&r.into()), "{r} not in {roles:?}");
    }
    assert!(
        !roles
            .iter()
            .any(|r| r.starts_with("V:") || r.starts_with("R:")),
        "frame fields stay with the frame: {roles:?}"
    );
    let f = expr_with(&g, "GI", "gps_symmetry").expect("frame");
    let froles = roles_of(&g, &f.id);
    for s in ["D1:", "D2:", "A1:", "A2:"] {
        assert!(
            froles.iter().any(|r| r.starts_with(s)),
            "{s} not shared by the frame: {froles:?}"
        );
    }
}

#[test]
fn leader_polyline_with_its_own_tapered_head_ends_in_that_head() {
    let (blocks, mut ents) = symmetry_frame_entities();
    ents.push(lwpoly_w(
        "LP",
        "02",
        &[
            ([100., 105.], 0., 0.),
            ([90., 105.], 0., 0.),
            ([90., 83.], 1., 0.),
            ([90., 80.], 0., 0.),
        ],
    ));
    let g = discover(&drawing(
        &format!("{}{}", tables(&convention_layers()), blocks),
        &ents,
    ))
    .unwrap();
    let f = expr_with(&g, "GI", "gps_symmetry").expect("frame");
    let roles = roles_of(&g, &f.id);
    assert!(
        roles.contains(&"LP:leader_line".into()) && roles.contains(&"LP:arrowhead".into()),
        "{roles:?}"
    );
}

#[test]
fn boxed_value_is_a_theoretically_exact_dimension() {
    let ents = vec![
        line("H1", "08", [100., 50.], [108., 50.]),
        line("H2", "08", [112., 50.], [120., 50.]),
        solid("HA", "08", [100., 50.], [103., 50.]),
        solid("HB", "08", [120., 50.], [117., 50.]),
        text("HT", "08", "20", [108.6, 50.4], 2.5, 0.),
        line("HE1", "02", [100., 40.], [100., 52.]),
        line("HE2", "02", [120., 40.], [120., 52.]),
        line("B1", "08", [107.8, 53.6], [113.2, 53.6]),
        line("B2", "08", [107.8, 53.6], [107.8, 50.]),
        line("B3", "08", [113.2, 53.6], [113.2, 50.]),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let e = expr_with(&g, "HT", "dimension_linear").expect("dimension");
    assert!(
        e.qualifiers.contains(&"theoretically_exact".into()),
        "{:?}",
        e.qualifiers
    );
    let roles = roles_of(&g, &e.id);
    for r in ["B1:frame_cell", "B2:frame_cell", "B3:frame_cell"] {
        assert!(roles.contains(&r.into()), "{r} not in {roles:?}");
    }
}

#[test]
fn spline_frame_control_points_are_not_curve_vertices() {
    // Fitted vertices (flag 8) of a smooth quarter curve; frame control points
    // (flag 16) at both ends would otherwise fold the path back and forth.
    let mut verts: Vec<([f64; 2], i32)> = vec![([10., 0.], 16)];
    for i in 0..9 {
        let a = std::f64::consts::FRAC_PI_2 * i as f64 / 8.;
        verts.push(([10. * a.cos(), 10. * a.sin()], 8));
    }
    verts.push(([1., 9.], 16));
    verts.push(([0., 10.], 16));
    let ents = vec![
        polyline("SP", "02", 4, &verts),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    assert!(
        !kinds(&g, "SP").contains(&"break_line".into()),
        "{:?}",
        kinds(&g, "SP")
    );
    covered(&g);
}

#[test]
fn duplicate_incidence_keeps_thick_lines_whole_and_open_ends_and_axes_join() {
    let ents = vec![
        line("K", "01", [0., 0.], [0., 10.]),
        line("B", "01", [0., 10.], [10., 10.]),
        line("AX", "05", [-5., 0.], [15., 0.]),
        line("Q", "0", [10., 10.], [10., 4.]),
        line("S", "0", [0., 3.], [-4., 3.]),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    for full in ["K", "B", "Q"] {
        assert!(
            g.members.iter().any(|m| m.source_handle == full
                && m.member_role == "contour_segment"
                && m.interval == vec![0., 1.]),
            "{full}"
        );
    }
    // A free line joined only in its interior is not turned into outline.
    assert!(!g.members.iter().any(|m| m.source_handle == "S"
        && m.member_role == "contour_segment"
        && m.interval == vec![0., 1.]));
    covered(&g);
}

#[test]
fn slightly_skewed_thin_piece_continuing_an_extension_line_stays_extension_line() {
    let ents = vec![
        line("V1", "08", [82., 0.], [82., 30.]),
        solid("VA", "08", [82., 0.], [82., 3.]),
        solid("VB", "08", [82., 30.], [82., 27.]),
        text("VT", "08", "%%C30", [81.4, 12.], 2.5, 90.),
        line("X1", "02", [81., 0.], [90., 0.]),
        line("X2", "02", [90., 0.], [99.5, -0.018]),
        line("X3", "02", [81., 30.], [90., 30.]),
        line("X4", "02", [90., 30.], [99.5, 29.982]),
        line("C1", "01", [99.5, -0.018], [101., -0.018]),
        line("C2", "01", [99.5, 29.982], [101., 29.982]),
        line("F1", "01", [99.5, -0.018], [99.5, 29.982]),
        line("F2", "01", [101., -0.018], [101., 29.982]),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let e = expr_with(&g, "VT", "dimension_diameter").expect("diameter");
    let roles = roles_of(&g, &e.id);
    for r in [
        "X1:extension_line",
        "X2:extension_line",
        "X3:extension_line",
        "X4:extension_line",
        "C1:extension_origin",
        "C2:extension_origin",
    ] {
        assert!(roles.contains(&r.into()), "{r} not in {roles:?}");
    }
    for x in ["X2", "X4"] {
        assert!(
            !kinds(&g, x).contains(&"visible_contour".into()),
            "{x}: {:?}",
            kinds(&g, x)
        );
    }
    assert!(kinds(&g, "C1").contains(&"visible_contour".into()));
    covered(&g);
}

#[test]
fn one_terminator_diameter_ending_on_the_far_silhouette_shares_it() {
    let ents = vec![
        line("E", "01", [100., 0.], [120., 0.]),
        line("S", "01", [100., 32.], [120., 32.]),
        line("X", "02", [80., 0.], [100., 0.]),
        line("V", "08", [82., 32.], [82., 1.5]),
        solid("AR", "08", [82., 0.], [82., 3.]),
        text("T", "08", "%%C32", [81., 8.], 2.5, 90.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let e = expr_with(&g, "T", "dimension_diameter").expect("half dimension");
    let m = members_of(&g, &e.id)
        .into_iter()
        .find(|m| m.source_handle == "S")
        .expect("far silhouette is a member");
    assert_eq!(m.member_role, "extension_origin");
    assert!(
        m.evidence_ids
            .contains(&"far_silhouette_at_dimension_line_end".into())
    );
    assert!(kinds(&g, "S").contains(&"visible_contour".into()));
    covered(&g);
}

#[test]
fn extension_line_from_the_virtual_corner_of_a_fillet_names_the_filleted_outline() {
    let ents = vec![
        line("V", "08", [82., 25.], [82., 1.5]),
        solid("AR", "08", [82., 0.], [82., 3.]),
        text("T", "08", "%%C34", [81., 8.], 2.5, 90.),
        line("X", "02", [80., 0.], [100., 0.]),
        line("L", "01", [100.5, 0.], [110., 0.]),
        ent(
            "ARC",
            "FA",
            "01",
            "10\n100.5\n20\n-0.5\n40\n0.5\n50\n90\n51\n180\n",
        ),
        line("FD", "01", [100., -0.5], [100., -10.]),
        line("FU", "01", [100., 0.], [100., 20.]),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let e = expr_with(&g, "T", "dimension_diameter").expect("half dimension");
    let ms = members_of(&g, &e.id);
    let l = ms
        .iter()
        .find(|m| m.source_handle == "L")
        .expect("filleted outline is the measured feature");
    assert_eq!(l.member_role, "extension_origin");
    assert!(
        l.evidence_ids
            .contains(&"contour_extended_to_virtual_corner_of_fillet".into())
    );
    assert!(
        !ms.iter()
            .any(|m| m.source_handle == "FU" || m.source_handle == "FD"),
        "faces through the corner are not the measured feature: {:?}",
        roles_of(&g, &e.id)
    );
    covered(&g);
}

#[test]
fn contour_drawn_again_over_another_names_one_primary() {
    let ents = vec![
        line("K", "01", [0., 0.], [0., 10.]),
        line("K2", "01", [0., 10.], [0., 0.]),
        line("P", "0", [0., 2.], [0., 6.]),
        line("B", "01", [0., 10.], [10., 10.]),
        line("D", "01", [10., 10.], [10., 0.]),
        line("A", "01", [10., 0.], [0., 0.]),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let dup = |s: &str| -> Vec<String> {
        g.members
            .iter()
            .filter(|m| m.source_handle == s && m.member_role == "contour_segment")
            .flat_map(|m| {
                m.evidence_ids
                    .iter()
                    .filter(|x| x.starts_with("duplicate_of:"))
                    .cloned()
            })
            .collect()
    };
    assert!(dup("K").is_empty(), "{:?}", dup("K"));
    assert_eq!(dup("K2"), vec!["duplicate_of:K".to_string()]);
    assert_eq!(dup("P"), vec!["duplicate_of:K".to_string()]);
    for s in ["B", "D", "A"] {
        assert!(dup(s).is_empty(), "{s}");
    }
    covered(&g);
}

fn glyph_frame(glyph: &[String]) -> (String, Vec<String>) {
    let blocks = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nGLYX\n10\n0\n20\n0\n{}0\nENDBLK\n0\nENDSEC\n",
        glyph.concat()
    );
    let ents = vec![
        ent(
            "INSERT",
            "GI",
            "01",
            "2\nGLYX\n10\n100\n20\n100\n41\n1\n42\n1\n50\n0\n",
        ),
        lwpoly(
            "C1",
            "02",
            &[
                [100., 100.],
                [110., 100.],
                [110., 110.],
                [100., 110.],
                [100., 100.],
            ],
        ),
        lwpoly(
            "C2",
            "02",
            &[[110., 100.], [126., 100.], [126., 110.], [110., 110.]],
        ),
        lwpoly(
            "C3",
            "02",
            &[[126., 100.], [132., 100.], [132., 110.], [126., 110.]],
        ),
        text("V", "11", "0.02", [113., 103.], 4., 0.),
        text("R", "11", "A", [127.5, 103.], 4., 0.),
        lwpoly_w(
            "LP",
            "02",
            &[
                ([100., 105.], 0., 0.),
                ([90., 105.], 0., 0.),
                ([90., 83.], 1., 0.),
                ([90., 80.], 0., 0.),
            ],
        ),
    ];
    (blocks, ents)
}

#[test]
fn perpendicularity_and_parallelism_symbols_name_their_own_tolerances() {
    for (glyph, want) in [
        (
            vec![
                line("G1", "0", [1., 2.], [9., 2.]),
                line("G2", "0", [5., 2.], [5., 9.]),
            ],
            "gps_perpendicularity",
        ),
        (
            vec![
                line("G1", "0", [2., 1.], [5., 9.]),
                line("G2", "0", [5., 1.], [8., 9.]),
            ],
            "gps_parallelism",
        ),
    ] {
        let (blocks, ents) = glyph_frame(&glyph);
        let g = discover(&drawing(
            &format!("{}{}", tables(&convention_layers()), blocks),
            &ents,
        ))
        .unwrap();
        assert!(
            expr_with(&g, "GI", want).is_some(),
            "{want}: {:?}",
            kinds(&g, "GI")
        );
        assert!(!g.expressions.iter().any(|e| e.kind == "gps_direction"));
        covered(&g);
    }
}

fn hatch(id: &str, layer: &str, pts: &[[f64; 2]]) -> String {
    let mut t = format!(
        "10\n0\n20\n0\n30\n0\n2\nANSI31\n70\n0\n71\n0\n91\n1\n92\n2\n72\n0\n73\n1\n93\n{}\n",
        pts.len()
    );
    for q in pts {
        t.push_str(&format!("10\n{}\n20\n{}\n", q[0], q[1]));
    }
    t.push_str("97\n0\n75\n0\n76\n1\n98\n0\n");
    ent("HATCH", id, layer, &t)
}

#[test]
fn free_curve_bounding_a_hatched_local_section_is_its_boundary_but_a_plain_curve_is_not() {
    let curve = |x0: f64, amp: f64| -> Vec<[f64; 2]> {
        (0..9)
            .map(|i| {
                let s = i as f64 / 8.;
                [x0 + amp * (std::f64::consts::PI * s).sin(), 20. * s]
            })
            .collect()
    };
    let sb = curve(25., 3.);
    let mut region = vec![[0., 0.]];
    region.extend(sb.iter().copied());
    region.push([0., 20.]);
    let ents = vec![
        line("A", "01", [0., 0.], [40., 0.]),
        line("B", "01", [40., 0.], [40., 20.]),
        line("C", "01", [40., 20.], [0., 20.]),
        line("D", "01", [0., 20.], [0., 0.]),
        lwpoly("SB", "02", &sb),
        lwpoly("SC", "02", &curve(33., 2.)),
        hatch("H1", "10", &region),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    assert!(
        kinds(&g, "SB").contains(&"break_line".into()),
        "{:?}",
        kinds(&g, "SB")
    );
    assert!(
        !kinds(&g, "SC").contains(&"break_line".into()),
        "{:?}",
        kinds(&g, "SC")
    );
    covered(&g);
}

fn implied_between<'a>(
    g: &'a MdEngineeringExpressionGraph,
    a: &str,
    b: &str,
) -> Vec<&'a oe_contracts::MdImpliedSpan> {
    g.implied_spans
        .iter()
        .filter(|s| {
            (s.from_source_handle == a && s.to_source_handle == b)
                || (s.from_source_handle == b && s.to_source_handle == a)
        })
        .collect()
}

#[test]
fn outline_broken_for_a_dimension_text_is_continued_in_the_complete_structure() {
    let ents = vec![
        line("F", "01", [0., 0.], [0., 20.]),
        line("TL", "01", [0., 20.], [25., 20.]),
        line("BL", "01", [0., 0.], [25., 0.]),
        line("RF", "01", [25., 0.], [25., 20.]),
        line("U1", "0", [0., 15.], [3., 15.]),
        line("U2", "0", [9., 15.], [20., 15.]),
        line("L1", "0", [0., 5.], [3.4, 5.]),
        line("L2", "0", [9.2, 5.], [20., 5.]),
        ent("ARC", "SA", "0", "10\n20\n20\n10\n40\n5\n50\n-90\n51\n90\n"),
        line("C1", "0", [1., 15.], [1., 20.]),
        line("C2", "0", [1., 0.], [1., 5.]),
        line("AX", "05", [-2., 10.], [27., 10.]),
        text("T", "08", "%%C30.5", [8.5, 1.], 5., 90.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    for (a, b) in [("U1", "U2"), ("L1", "L2")] {
        let s = implied_between(&g, a, b);
        assert_eq!(s.len(), 1, "{a}-{b}: {:?}", g.implied_spans);
        assert_eq!(s[0].cause, "text_break");
        assert_eq!(s[0].line_class, "visible_outline");
        assert_eq!(s[0].representation, "complete_structure");
        assert!(
            s[0].evidence_ids.contains(&"text:T".into()),
            "{:?}",
            s[0].evidence_ids
        );
        assert!(
            s[0].evidence_ids
                .iter()
                .any(|x| x.starts_with("mirror_counterpart:")),
            "{:?}",
            s[0].evidence_ids
        );
    }
    // The piece beyond the chamfer junction reaches its explained end.
    for h in ["U1", "L1"] {
        assert!(
            g.members.iter().any(|m| m.source_handle == h
                && m.member_role == "contour_segment"
                && m.interval == vec![0., 1.]),
            "{h}"
        );
        assert!(
            !g.members
                .iter()
                .any(|m| m.source_handle == h && m.member_role == "unresolved_source")
        );
    }
    assert!(
        g.structure_ends
            .iter()
            .any(|e| e.source_handle == "U1" && e.cause == "text_break")
    );
    assert!(
        !g.members
            .iter()
            .any(|m| g.implied_spans.iter().any(|s| s.id == m.id))
    );
    covered(&g);
}

#[test]
fn a_slot_opening_stays_open_even_under_a_text() {
    let ents = vec![
        line("A1", "0", [30., 0.], [30., 5.]),
        line("A2", "0", [30., 10.], [30., 20.]),
        line("S1", "0", [30., 5.], [40., 5.]),
        line("S2", "0", [30., 10.], [40., 10.]),
        ent(
            "ARC",
            "SA",
            "0",
            "10\n40\n20\n7.5\n40\n2.5\n50\n-90\n51\n90\n",
        ),
        line("B", "01", [30., 0.], [45., 0.]),
        line("T2", "01", [30., 20.], [45., 20.]),
        text("T", "08", "%%C12", [31.5, 4.44], 3., 90.),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    assert!(
        implied_between(&g, "A1", "A2").is_empty(),
        "{:?}",
        g.implied_spans
    );
    covered(&g);
}

/// A freehand curve across a plate from `y0` to `y1` at `x`: a small wave about the vertical,
/// through both ends (a local section boundary from edge to edge).
fn wave_across(x: f64, y0: f64, y1: f64) -> Vec<[f64; 2]> {
    let n = ((y1 - y0) / 0.5).round() as usize;
    (0..=n)
        .map(|i| {
            let y = y0 + (y1 - y0) * i as f64 / n as f64;
            [x + 0.4 * (std::f64::consts::TAU * (y - y0) / 5.).sin(), y]
        })
        .collect()
}

/// A plate from x = `x0` to `x0 + 40`, y 0 to 20, cut open between two freehand curves from its
/// bottom to its top edge at `x0 + 7` and `x0 + 13`, the band between them hatched.
fn plate_with_band(x0: f64, hatch_id: &str) -> Vec<String> {
    let (w1, w2) = (
        wave_across(x0 + 7., 0., 20.),
        wave_across(x0 + 13., 0., 20.),
    );
    let mut band = w1.clone();
    band.extend(w2.iter().rev().copied());
    vec![
        line("B", "01", [x0, 0.], [x0 + 40., 0.]),
        line("T", "01", [x0, 20.], [x0 + 40., 20.]),
        line("LF", "01", [x0, 0.], [x0, 20.]),
        line("RF", "01", [x0 + 40., 0.], [x0 + 40., 20.]),
        lwpoly("W1", "02", &w1),
        lwpoly("W2", "02", &w2),
        hatch(hatch_id, "10", &band),
    ]
}

#[test]
fn local_section_interruption_is_bridged_but_an_unexplained_gap_is_not() {
    // Q is drawn up to the region of the local section on both sides; R has a gap with no cause.
    let mut ents = plate_with_band(55., "H1");
    ents.extend([
        line("Q1", "0", [55., 12.], [62., 12.]),
        line("Q2", "0", [68., 12.], [95., 12.]),
        line("R1", "0", [80., 0.], [80., 8.]),
        line("R2", "0", [80., 12.], [80., 20.]),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ]);
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let s = implied_between(&g, "Q1", "Q2");
    assert_eq!(s.len(), 1, "{:?}", g.implied_spans);
    assert_eq!(s[0].cause, "local_section");
    assert!(
        s[0].evidence_ids.iter().any(|x| x == "section:H1"),
        "{:?}",
        s[0].evidence_ids
    );
    // Without a section or text the same gap is neither bridged nor used to
    // extend the pieces to their open ends.
    assert!(implied_between(&g, "R1", "R2").is_empty());
    assert!(!g.members.iter().any(|m| m.source_handle == "R1"
        && m.member_role == "contour_segment"
        && m.interval[1] > 1. - 1e-9));
    assert!(g.members.iter().any(|m| m.source_handle == "Q1"
        && m.member_role == "contour_segment"
        && m.interval == vec![0., 1.]));
    covered(&g);
}

#[test]
fn an_imprecise_joint_is_reported_as_a_drafting_gap_and_never_bridged() {
    let ents = vec![
        line("D1", "0", [80., 0.], [90., 0.]),
        line("D2", "0", [80., 10.], [90., 10.]),
        line("D3", "01", [80., 0.], [80., 10.]),
        ent(
            "ARC",
            "DA",
            "0",
            "10\n90\n20\n5\n40\n4.992\n50\n-90\n51\n90\n",
        ),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    assert!(
        g.structure_ends
            .iter()
            .any(|e| e.source_handle == "DA" && e.cause == "drafting_gap"),
        "{:?}",
        g.structure_ends
    );
    assert!(g.implied_spans.is_empty());
    covered(&g);
}

#[test]
fn section_only_members_are_marked_in_a_local_section_and_never_in_a_full_section() {
    // SP is a line of the cut face inside the band of a local section; a second plate is a full
    // section (hatch bounded by outline only) with a line SQ inside it.
    let mut ents = plate_with_band(55., "H1");
    ents.extend([
        line("SP", "01", [63., 9.], [65., 11.]),
        line("B2", "01", [105., 0.], [145., 0.]),
        line("T2", "01", [105., 20.], [145., 20.]),
        line("L2", "01", [105., 0.], [105., 20.]),
        line("R2", "01", [145., 0.], [145., 20.]),
        hatch(
            "H2",
            "10",
            &[[105., 0.], [145., 0.], [145., 20.], [105., 20.]],
        ),
        line("SQ", "01", [124., 9.], [126., 11.]),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ]);
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let marked = |h: &str| {
        g.members.iter().any(|m| {
            m.source_handle == h
                && m.evidence_ids
                    .iter()
                    .any(|x| x.starts_with("section_profile:"))
        })
    };
    for h in ["SP", "H1", "W1", "W2"] {
        assert!(marked(h), "{h} not marked");
    }
    for h in ["B", "T", "LF", "RF", "B2", "T2", "L2", "R2", "H2", "SQ"] {
        assert!(!marked(h), "{h} marked");
    }
    covered(&g);
}

/// Structural line holding a member of `src`.
fn line_of<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
) -> Vec<&'a oe_contracts::MdStructuralLine> {
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
fn arc_ent(id: &str, layer: &str, c: [f64; 2], r: f64, a0: f64, a1: f64) -> String {
    ent(
        "ARC",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n50\n{a0}\n51\n{a1}\n", c[0], c[1]),
    )
}
fn same_points(p: &[f64], a: [f64; 2], b: [f64; 2], tol: f64) -> bool {
    let d = |x: [f64; 2], y: [f64; 2]| (x[0] - y[0]).hypot(x[1] - y[1]);
    let (p0, p1) = ([p[0], p[1]], [p[2], p[3]]);
    (d(p0, a) <= tol && d(p1, b) <= tol) || (d(p0, b) <= tol && d(p1, a) <= tol)
}

#[test]
fn a_circle_broken_for_texts_is_bridged_along_the_circle_and_is_one_circle() {
    let ents = vec![
        arc_ent("A1", "01", [200., 50.], 10., 20., 160.),
        arc_ent("A2", "01", [200., 50.], 10., 200., 340.),
        text("TR", "08", "M12", [211.75, 46.43], 3.5, 90.),
        text("TL", "08", "M12", [191.75, 46.43], 3.5, 90.),
        arc_ent("S1", "01", [260., 50.], 6., 30., 330.),
        text("TS", "08", "R6x", [267.75, 46.43], 3.5, 90.),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let ab = implied_between(&g, "A1", "A2");
    assert_eq!(ab.len(), 2, "{:?}", g.implied_spans);
    for s in &ab {
        assert_eq!(s.cause, "text_break");
        assert!(s.arc.as_ref().is_some_and(|a| a.len() == 5), "{s:?}");
    }
    // One arc broken once for a number is never joined to its own other end (card
    // annotation_break, counter-example): its ends are explained, its gap stays.
    assert!(
        implied_between(&g, "S1", "S1").is_empty(),
        "{:?}",
        g.implied_spans
    );
    assert!(
        g.structure_ends
            .iter()
            .filter(|e| e.source_handle == "S1")
            .all(|e| e.cause == "text_break")
    );
    assert_eq!(line_of(&g, "S1")[0].geometry, "arc");
    let l = line_of(&g, "A1");
    assert_eq!(l.len(), 1, "{:?}", g.structural_lines);
    assert_eq!(l[0].geometry, "circle", "{:?}", l[0]);
    assert_eq!(l[0].line_type, "thick_continuous");
    assert!(l[0].pieces.iter().any(|p| p.starts_with("implied:")));
    assert_eq!(line_of(&g, "A1")[0].id, line_of(&g, "A2")[0].id);
    assert_eq!(line_of(&g, "A1")[0].pieces.len(), 4);
    covered(&g);
}

#[test]
fn a_line_split_into_pieces_and_drawn_again_is_one_line() {
    let ents = vec![
        line("P1", "01", [0., 20.], [10., 20.]),
        line("P2", "01", [10., 20.], [22., 20.]),
        line("P3", "01", [22., 20.], [30., 20.]),
        line("P2B", "01", [22., 20.], [10., 20.]),
        line("PD", "01", [5., 20.], [15., 20.]),
        line("BB", "01", [0., 0.], [30., 0.]),
        line("LS", "01", [0., 0.], [0., 20.]),
        line("RS", "01", [30., 0.], [30., 20.]),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let l = line_of(&g, "P1");
    assert_eq!(l.len(), 1);
    for h in ["P2", "P3", "P2B", "PD"] {
        assert_eq!(line_of(&g, h)[0].id, l[0].id, "{h}");
    }
    assert!(
        same_points(&l[0].points, [0., 20.], [30., 20.], 1e-9),
        "{:?}",
        l[0]
    );
    assert!(l[0].coincident.len() >= 2, "{:?}", l[0]);
    assert_eq!(l[0].pieces.len() + l[0].coincident.len(), 5);
    assert_ne!(line_of(&g, "BB")[0].id, l[0].id);
    covered(&g);
}

#[test]
fn near_collinear_pieces_are_one_line_but_a_real_step_is_not() {
    let ents = vec![
        line("N1", "01", [0., 20.], [15., 20.]),
        line("N2", "01", [15., 20.], [30., 20.012]),
        line("NB", "01", [0., 0.], [30., 0.]),
        line("NL", "01", [0., 0.], [0., 20.]),
        line("NR", "01", [30., 0.], [30., 20.012]),
        line("M1", "01", [40., 20.], [55., 20.]),
        line("MS", "01", [55., 20.], [55., 20.1]),
        line("M2", "01", [55., 20.1], [70., 20.1]),
        line("MB", "01", [40., 0.], [70., 0.]),
        line("ML", "01", [40., 0.], [40., 20.]),
        line("MR", "01", [70., 0.], [70., 20.1]),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let n = line_of(&g, "N1");
    assert_eq!(n.len(), 1);
    assert_eq!(line_of(&g, "N2")[0].id, n[0].id);
    assert!(
        n[0].evidence_ids
            .iter()
            .any(|e| e.starts_with("near_collinear:")),
        "{:?}",
        n[0]
    );
    assert!(same_points(&n[0].points, [0., 20.], [30., 20.012], 1e-9));
    assert_ne!(line_of(&g, "M1")[0].id, line_of(&g, "M2")[0].id);
    covered(&g);
}

#[test]
fn a_text_break_across_a_slightly_offset_piece_is_bridged_and_merged() {
    let ents = vec![
        line("F", "01", [0., 0.], [0., 20.]),
        line("TL", "01", [0., 20.], [25., 20.]),
        line("BL", "01", [0., 0.], [25., 0.]),
        line("RF", "01", [25., 0.], [25., 20.]),
        line("U1", "0", [0., 15.], [3., 15.]),
        line("U2", "0", [9., 15.012], [20., 15.]),
        ent("ARC", "SA", "0", "10\n20\n20\n10\n40\n5\n50\n-90\n51\n90\n"),
        line("L1", "0", [0., 5.], [20., 5.]),
        line("AX", "05", [-2., 10.], [27., 10.]),
        text("T", "08", "%%C30.5", [8.5, 1.], 5., 90.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let s = implied_between(&g, "U1", "U2");
    assert_eq!(s.len(), 1, "{:?}", g.implied_spans);
    let l = line_of(&g, "U1");
    assert_eq!(l.len(), 1);
    assert_eq!(line_of(&g, "U2")[0].id, l[0].id);
    assert!(l[0].pieces.contains(&s[0].id));
    covered(&g);
}

#[test]
fn a_real_opening_is_never_one_line() {
    let ents = vec![
        line("A1", "0", [30., 0.], [30., 5.]),
        line("A2", "0", [30., 10.], [30., 20.]),
        line("S1", "0", [30., 5.], [40., 5.]),
        line("S2", "0", [30., 10.], [40., 10.]),
        arc_ent("SA", "0", [40., 7.5], 2.5, -90., 90.),
        line("B", "01", [30., 0.], [45., 0.]),
        line("T2", "01", [30., 20.], [45., 20.]),
        text("T", "08", "%%C12", [31.5, 4.44], 3., 90.),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let (a1, a2) = (line_of(&g, "A1"), line_of(&g, "A2"));
    assert_eq!((a1.len(), a2.len()), (1, 1));
    assert_ne!(a1[0].id, a2[0].id);
    assert!(!a1[0].pieces.iter().any(|p| p.starts_with("implied:")));
    covered(&g);
}

#[test]
fn a_hidden_line_broken_for_a_text_is_one_dashed_line() {
    let ents = vec![
        line("LF", "01", [0., 0.], [0., 20.]),
        line("TT", "01", [0., 20.], [25., 20.]),
        line("BB", "01", [0., 0.], [25., 0.]),
        line("RF", "01", [25., 0.], [25., 20.]),
        line("H1", "04", [0., 10.], [8., 10.]),
        line("H2", "04", [14., 10.], [25., 10.]),
        text("T", "08", "M12", [8.5, 8.6], 3.5, 0.),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let s = implied_between(&g, "H1", "H2");
    assert_eq!(s.len(), 1, "{:?}", g.implied_spans);
    assert_eq!(s[0].line_class, "hidden_outline");
    let l = line_of(&g, "H1");
    assert_eq!(l.len(), 1);
    assert_eq!(line_of(&g, "H2")[0].id, l[0].id);
    assert_eq!(l[0].line_class, "hidden_outline");
    assert_eq!(l[0].line_type, "thin_dashed");
    assert!(same_points(&l[0].points, [0., 10.], [25., 10.], 1e-9));
    covered(&g);
}

#[test]
fn pieces_in_different_views_are_never_one_line_or_bridged() {
    let ents = vec![
        line("T1", "01", [0., 20.], [25., 20.]),
        line("L1", "01", [0., 0.], [0., 20.]),
        line("R1", "01", [20., 0.], [20., 20.]),
        line("B1", "01", [0., 0.], [20., 0.]),
        line("T2", "01", [35., 20.], [60., 20.]),
        line("L2", "01", [40., 0.], [40., 20.]),
        line("R2", "01", [60., 0.], [60., 20.]),
        line("B2", "01", [40., 0.], [60., 0.]),
        text("T", "08", "ABCDE", [25.2, 18.6], 3.5, 0.),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    let (l1, l2) = (line_of(&g, "T1"), line_of(&g, "T2"));
    assert_eq!((l1.len(), l2.len()), (1, 1));
    assert_ne!(l1[0].view_id, l2[0].view_id, "{:?}", g.views);
    assert_ne!(l1[0].id, l2[0].id);
    assert!(
        implied_between(&g, "T1", "T2").is_empty(),
        "{:?}",
        g.implied_spans
    );
    covered(&g);
}

#[test]
fn a_line_meeting_only_section_profile_is_cut_at_the_inferred_outline() {
    // A local section at the left face: a freehand curve from the face round to the face. The
    // face line Q is drawn up to the curve on both sides; N ends inside the region on SP only.
    let curve = [
        [55., 7.],
        [57., 7.2],
        [59.5, 7.6],
        [62., 8.4],
        [63.6, 9.3],
        [64., 10.],
        [63.6, 10.7],
        [62., 11.6],
        [59.5, 12.4],
        [57., 12.8],
        [55., 13.],
    ];
    let ents = vec![
        line("Q1", "0", [60., 0.], [60., 7.76]),
        line("Q2", "0", [60., 12.24], [60., 20.]),
        line("N", "01", [59., 10.], [75., 10.]),
        line("SP", "0", [59., 10.], [58., 9.]),
        line("B", "01", [55., 0.], [75., 0.]),
        line("T", "01", [55., 20.], [75., 20.]),
        line("LF", "01", [55., 0.], [55., 20.]),
        line("RF", "01", [75., 0.], [75., 20.]),
        hatch("H1", "10", &curve),
        lwpoly("W", "02", &curve),
        text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
    ];
    let g = discover(&drawing(&tables(&convention_layers()), &ents)).unwrap();
    assert_eq!(
        implied_between(&g, "Q1", "Q2").len(),
        1,
        "{:?}",
        g.implied_spans
    );
    let l = line_of(&g, "N");
    assert_eq!(l.len(), 1);
    assert!(
        same_points(&l[0].points, [60., 10.], [75., 10.], 1e-9),
        "{:?}",
        l[0]
    );
    let n = g
        .members
        .iter()
        .find(|m| m.source_handle == "N" && m.member_role == "contour_segment")
        .unwrap();
    assert_eq!(
        l[0].trimmed,
        vec![format!("member:{}:0.000000:0.062500", n.id)]
    );
    // The face line through the section is one line with its inferred span.
    let q = line_of(&g, "Q1");
    assert_eq!(line_of(&g, "Q2")[0].id, q[0].id);
    assert!(same_points(&q[0].points, [60., 0.], [60., 20.], 1e-9));
    covered(&g);
}

/// A native dimension whose block shows `shown` (one MTEXT per piece).
fn native_dimension_shown(
    id: &str,
    block: &str,
    dtype: i32,
    text: &str,
    measured: f64,
    shown: &[&str],
) -> (String, String) {
    let mut children = vec![
        line(&format!("{id}1"), "0", [0., 40.], [30., 40.]),
        line(&format!("{id}2"), "0", [0., 30.], [0., 42.]),
        line(&format!("{id}3"), "0", [30., 30.], [30., 42.]),
        solid(&format!("{id}4"), "0", [0., 40.], [2., 40.]),
        solid(&format!("{id}5"), "0", [30., 40.], [28., 40.]),
    ];
    for (k, s) in shown.iter().enumerate() {
        children.push(ent(
            "MTEXT",
            &format!("{id}T{k}"),
            "0",
            &format!("1\n{s}\n10\n{}\n20\n41\n40\n2.5\n71\n7\n", 12 + 3 * k),
        ));
    }
    let blk = format!(
        "0\nBLOCK\n2\n{block}\n10\n0\n20\n0\n{}0\nENDBLK\n",
        children.concat()
    );
    let dim = ent(
        "DIMENSION",
        id,
        "0",
        &format!(
            "2\n{block}\n10\n30\n20\n40\n11\n15\n21\n41\n70\n{dtype}\n1\n{text}\n13\n0\n23\n30\n14\n30\n24\n30\n42\n{measured}\n50\n0\n"
        ),
    );
    (blk, dim)
}

#[test]
fn an_uncomputed_measurement_is_never_shown_as_a_value() {
    // A DXF can store -1 when the measurement was not computed: the block shows the value.
    let (b1, d1) = native_dimension_shown("DM", "*DM1", 33, "<>%%P0.05", -1., &["74", "%%P0.05"]);
    // No number in the block: the definition points (13, 14 of an aligned dimension) give it.
    let (b2, d2) = native_dimension_shown("DN", "*DN1", 33, "<>%%P0.05", -1., &["%%P0.05"]);
    // A computed measurement is used as before.
    let (b3, d3) = native_dimension_shown("DO", "*DO1", 33, "<>", 30., &["30"]);
    let blocks = format!("0\nSECTION\n2\nBLOCKS\n{b1}{b2}{b3}0\nENDSEC\n");
    let g = discover(&drawing(
        &format!("{}{}", tables(&convention_layers()), blocks),
        &[d1, d2, d3],
    ))
    .unwrap();
    assert_eq!(
        expr_with(&g, "DM", "dimension_linear").unwrap().label,
        "74±0.05"
    );
    assert_eq!(
        expr_with(&g, "DN", "dimension_linear").unwrap().label,
        "30±0.05"
    );
    assert_eq!(expr_with(&g, "DO", "dimension_linear").unwrap().label, "30");
    assert!(!g.expressions.iter().any(|e| e.label.starts_with("-1")));
    covered(&g);
}

#[test]
fn text_of_a_dxf_before_2007_is_read_in_its_code_page() {
    let header = |page: &str| {
        format!(
            "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$DWGCODEPAGE\n3\n{page}\n0\nENDSEC\n"
        )
    };
    let with = |page: &str| {
        let mut b = drawing(
            &format!("{}{}", header(page), tables(&convention_layers())),
            &[
                line("A", "01", [0., 0.], [40., 0.]),
                text("T", "11", "XXXXXXXX", [0., -10.], 3.5, 0.),
            ],
        );
        let at = b.windows(8).position(|w| w == b"XXXXXXXX").unwrap();
        // 技术要求 in GBK.
        b.splice(at..at + 8, b"\xbc\xbc\xca\xf5\xd2\xaa\xc7\xf3".to_vec());
        b
    };
    for page in ["ANSI_936", "GB2312"] {
        let g = discover(&with(page)).unwrap();
        assert!(
            g.expressions.iter().any(|e| e.label.contains("技术要求")),
            "{page}: {:?}",
            g.expressions.iter().map(|e| &e.label).collect::<Vec<_>>()
        );
        covered(&g);
    }
    // Bytes that are not UTF-8 under a code page this reader does not know: an error, no guess.
    assert!(discover(&with("ANSI_99999")).is_err());
}

#[test]
fn coordinates_printed_with_fewer_digits_change_nothing() {
    // One DXF writer keeps the exact double, another rounds the last digit: the same drawing
    // must give the same members and structural lines (which twin is the piece, line order).
    let make = |e: f64| {
        let ents = vec![
            line("L0", "01", [0., 0.], [0., 10.]),
            line("L1", "01", [0., 0.], [10., 0.]),
            line("L1B", "01", [0., 0.], [10. + e, 0.]),
            line("L2", "01", [10., 0.], [10., 10.]),
            line("TT", "01", [0., 10.], [10., 10.]),
            line("C1", "01", [10., 10.], [11., 11.]),
            line("C2", "01", [0., 10.], [-1., 11.]),
            line("C3", "01", [10., 0.], [11. + e, -1.]),
            text("T0", "11", "scale reference", [0., -20.], 3.5, 0.),
        ];
        discover(&drawing(&tables(&convention_layers()), &ents)).unwrap()
    };
    let (a, b) = (make(0.), make(4e-15));
    let key = |g: &MdEngineeringExpressionGraph| {
        let mut m: Vec<(String, String, Vec<i64>)> = g
            .members
            .iter()
            .map(|m| {
                (
                    m.source_handle.clone(),
                    m.member_role.clone(),
                    m.interval
                        .iter()
                        .map(|x| (x * 1e9).round() as i64)
                        .collect(),
                )
            })
            .collect();
        m.sort();
        m
    };
    assert_eq!(key(&a), key(&b));
    let lines = |g: &MdEngineeringExpressionGraph| {
        g.structural_lines
            .iter()
            .map(|l| (l.id.clone(), l.pieces.clone(), l.coincident.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(lines(&a), lines(&b));
    covered(&a);
    covered(&b);
}
