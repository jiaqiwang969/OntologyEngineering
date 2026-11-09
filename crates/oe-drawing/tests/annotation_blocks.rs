//! Drafting objects stored as one block reference (the way CAD annotation tools write a leader
//! note, a cutting-plane symbol, a detail indicator, its title and the enlarged view, technical
//! requirements, a tolerance frame, item numbers, the parts list, a library part, masks and the
//! sheet frame). Fixtures are authored from the card knowledge (GB/T 4457.2, 4458.1, 4458.2,
//! 4458.6, 10609.2, 1182, 131, 14689); handles, coordinates, letters and values are invented, and
//! no drawing, reference or correction ledger is read.
use oe_contracts::{MdEngineeringExpression, MdEngineeringExpressionGraph};
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
fn circle(id: &str, layer: &str, c: P, r: f64) -> String {
    ent(
        "CIRCLE",
        id,
        layer,
        &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]),
    )
}
/// MTEXT with an attachment point (1 top left … 5 middle centre … 8 bottom centre).
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
fn text(id: &str, layer: &str, s: &str, p: P, h: f64) -> String {
    ent(
        "TEXT",
        id,
        layer,
        &format!("1\n{s}\n10\n{}\n20\n{}\n40\n{h}\n50\n0\n", p[0], p[1]),
    )
}
/// A solid-filled HATCH triangle (polyline boundary).
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
/// A solid-filled dot: a HATCH whose boundary is two half circles (bulge 1).
fn dot(id: &str, layer: &str, c: P, r: f64) -> String {
    ent(
        "HATCH",
        id,
        layer,
        &format!(
            "10\n0\n20\n0\n30\n0\n2\nSOLID\n70\n1\n71\n0\n91\n1\n92\n2\n72\n1\n73\n1\n93\n2\n10\n{}\n20\n{}\n42\n1\n10\n{}\n20\n{}\n42\n1\n97\n0\n75\n0\n76\n1\n98\n0\n",
            c[0] + r,
            c[1],
            c[0] - r,
            c[1]
        ),
    )
}
fn insert(id: &str, layer: &str, name: &str, at: P, scale: f64) -> String {
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
/// A WIPEOUT whose clip polygon is given in world coordinates inside the box `at` + `size`.
fn wipeout(id: &str, layer: &str, at: P, size: P, poly: &[P]) -> String {
    let mut t = format!(
        "10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n0\n31\n0\n12\n0\n22\n{}\n32\n0\n13\n1\n23\n1\n70\n7\n280\n1\n71\n2\n91\n{}\n",
        at[0],
        at[1],
        size[0],
        size[1],
        poly.len()
    );
    for p in poly {
        // pixel coordinates of a 1×1 image: x from −0.5 at the left, y from −0.5 at the top
        t.push_str(&format!(
            "14\n{}\n24\n{}\n",
            (p[0] - at[0]) / size[0] - 0.5,
            0.5 - (p[1] - at[1]) / size[1]
        ));
    }
    ent("WIPEOUT", id, layer, &t)
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
fn by_rule<'a>(
    g: &'a MdEngineeringExpressionGraph,
    rule: &str,
) -> Vec<&'a MdEngineeringExpression> {
    g.expressions.iter().filter(|e| e.rule == rule).collect()
}
fn roles(g: &MdEngineeringExpressionGraph, e: &MdEngineeringExpression) -> Vec<(String, String)> {
    g.members
        .iter()
        .filter(|m| m.expression_id == e.id)
        .map(|m| {
            (
                m.member_role.clone(),
                m.native_member_id.clone().unwrap_or_default(),
            )
        })
        .collect()
}
fn unresolved(g: &MdEngineeringExpressionGraph) -> Vec<String> {
    g.expressions
        .iter()
        .filter(|e| e.kind == "unresolved")
        .map(|e| e.label.clone())
        .collect()
}
fn ev(e: &MdEngineeringExpression, x: &str) -> bool {
    e.evidence_ids.iter().any(|v| v == x)
}
/// A closed rectangle of thick outline on layer 01 (a view of the part).
fn outline(prefix: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<String> {
    vec![
        line(&format!("{prefix}1"), "01", [x0, y0], [x1, y0]),
        line(&format!("{prefix}2"), "01", [x1, y0], [x1, y1]),
        line(&format!("{prefix}3"), "01", [x1, y1], [x0, y1]),
        line(&format!("{prefix}4"), "01", [x0, y1], [x0, y0]),
    ]
}

// ---------------------------------------------------------------- technical requirements

fn requirement_block(items: Vec<String>) -> Vec<String> {
    let mut v = vec![mtext("RH", "08", "\\T1.1;技术要求", [40., 0.], 5., 2)];
    v.extend(items);
    v
}

#[test]
fn requirement_block_items_split_at_inline_numbers() {
    let block = requirement_block(vec![
        mtext("R1", "08", "1.锐边倒钝。", [0., -10.], 3.5, 1),
        mtext("R2", "08", "2.未注倒角C1。", [0., -16.], 3.5, 1),
        mtext("R3", "08", "3.调质处理后", [0., -22.], 3.5, 1),
        mtext("R4", "08", "硬度HB240～280。", [0., -28.], 3.5, 1),
    ]);
    let mut ents = outline("V", 0., 0., 60., 30.);
    ents.push(insert("RB", "08", "*R1", [100., 80.], 1.));
    let g = run(&[("*R1", block)], &ents);
    let mine = by_rule(&g, "card:technical_note/requirement_block");
    let labels: Vec<(&str, &str)> = mine
        .iter()
        .map(|e| (e.kind.as_str(), e.label.as_str()))
        .collect();
    assert_eq!(
        labels,
        vec![
            ("technical_general", "技术要求"),
            ("technical_general", "1.锐边倒钝。"),
            ("technical_general", "2.未注倒角C1。"),
            ("material_heat", "3.调质处理后硬度HB240～280。"),
        ],
        "{labels:?}"
    );
    // the block owns its parts: members are the block root with the parts' paths
    let item3 = mine[3];
    assert_eq!(
        roles(&g, item3)
            .iter()
            .map(|r| r.1.as_str())
            .collect::<Vec<_>>(),
        vec!["RB/R3", "RB/R4"]
    );
    assert!(
        mine.iter()
            .all(|e| e.view_id.as_deref() == Some("view:sheet"))
    );
    assert!(unresolved(&g).is_empty(), "{:?}", unresolved(&g));
}

#[test]
fn decimal_at_line_start_is_not_an_item_number() {
    let block = requirement_block(vec![
        mtext("R1", "08", "1.未注圆角R2，", [0., -10.], 3.5, 1),
        mtext("R2", "08", "1.5以下的锐边倒钝。", [0., -16.], 3.5, 1),
    ]);
    let g = run(
        &[("*R2", block)],
        &[insert("RB", "08", "*R2", [0., 0.], 1.)],
    );
    let items: Vec<&MdEngineeringExpression> = by_rule(&g, "card:technical_note/requirement_block")
        .into_iter()
        .filter(|e| e.label != "技术要求")
        .collect();
    assert_eq!(
        items.len(),
        1,
        "{:?}",
        items.iter().map(|e| &e.label).collect::<Vec<_>>()
    );
    assert_eq!(roles(&g, items[0]).len(), 2);
}

/// Texture symbol strokes: tip, short arm at 120°, long arm at 60° (ratio 2.2), bar.
fn texture(prefix: &str, layer: &str, tip: P) -> Vec<String> {
    let s = [tip[0] - 5.774, tip[1] + 10.];
    let l = [tip[0] + 12.7, tip[1] + 22.];
    vec![
        line(&format!("{prefix}a"), layer, tip, s),
        line(&format!("{prefix}b"), layer, tip, l),
        line(
            &format!("{prefix}c"),
            layer,
            s,
            [tip[0] + 5.774, tip[1] + 10.],
        ),
    ]
}

#[test]
fn requirement_block_with_general_roughness_is_partitioned() {
    let mut nested = texture("Q", "08", [40., -50.]);
    nested.push(mtext("QT", "08", "3.其余", [0., -40.], 3.5, 1));
    nested.push(mtext("QV", "08", "0.4", [42., -36.], 3.5, 5));
    let mut block = requirement_block(vec![
        mtext("R1", "08", "1.锐边倒钝。", [0., -10.], 3.5, 1),
        mtext("R2", "08", "2.未注倒角C1。", [0., -16.], 3.5, 1),
    ]);
    block.push(insert("RN", "08", "*R4", [0., 0.], 1.));
    let g = run(
        &[("*R4", nested), ("*R3", block)],
        &[insert("RB", "08", "*R3", [0., 0.], 1.)],
    );
    let kinds: BTreeSet<&str> = exprs(&g, "RB").iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["technical_general", "surface_roughness"]),
        "{kinds:?}"
    );
    let r = exprs(&g, "RB")
        .into_iter()
        .find(|e| e.kind == "surface_roughness")
        .unwrap();
    assert_eq!(r.label, "其余 0.4");
    assert_eq!(r.rule, "card:surface_texture_symbol/most_surfaces");
    assert!(r.qualifiers.contains(&"general_requirement".to_string()));
    assert!(
        r.concept_ids.contains(&"DC-G6-006".to_string()),
        "{:?}",
        r.concept_ids
    );
    let paths: BTreeSet<String> = roles(&g, r).into_iter().map(|x| x.1).collect();
    assert!(paths.iter().all(|p| p.starts_with("RB/RN/")), "{paths:?}");
    // the items are not the roughness
    assert_eq!(
        by_rule(&g, "card:technical_note/requirement_block").len(),
        3
    );
    assert!(unresolved(&g).is_empty(), "{:?}", unresolved(&g));
}

#[test]
fn remaining_roughness_block_without_heading_stays_roughness() {
    let mut block = texture("Q", "08", [40., -50.]);
    block.push(mtext("QT", "08", "其余", [20., -40.], 3.5, 5));
    block.push(mtext("QV", "08", "50", [42., -36.], 3.5, 5));
    let g = run(
        &[("*R5", block)],
        &[insert("RB", "08", "*R5", [0., 0.], 1.)],
    );
    let e = exprs(&g, "RB");
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str()),
        ("surface_roughness", "其余 50")
    );
    assert!(by_rule(&g, "card:technical_note/requirement_block").is_empty());
}

// ---------------------------------------------------------------- tolerance frames

/// Frame cells drawn as separate lines: rows of 10 high, glyph cell 10 wide, value cell 20, an
/// optional datum cell 10; `rows` lists (glyph, value, datum) from the top.
fn frame_grid(prefix: &str, rows: &[(&str, &str, &str)], font: &str) -> Vec<String> {
    let n = rows.len() as f64;
    let top = 10. * n;
    let mut v = Vec::new();
    for (k, y) in (0..=rows.len()).map(|k| (k, top - 10. * k as f64)) {
        v.push(line(&format!("{prefix}h{k}"), "08", [0., y], [40., y]));
    }
    for (k, x) in [0., 10., 30., 40.].iter().enumerate() {
        v.push(line(&format!("{prefix}v{k}"), "08", [*x, 0.], [*x, top]));
    }
    for (k, (glyph, value, datum)) in rows.iter().enumerate() {
        let y = top - 10. * k as f64 - 5.;
        v.push(mtext(
            &format!("{prefix}g{k}"),
            "08",
            &format!("{{{font}{glyph}}}"),
            [5., y],
            5.,
            5,
        ));
        v.push(mtext(
            &format!("{prefix}t{k}"),
            "08",
            value,
            [12., y],
            5.,
            4,
        ));
        if !datum.is_empty() {
            v.push(mtext(
                &format!("{prefix}r{k}"),
                "08",
                datum,
                [35., y],
                5.,
                5,
            ));
        }
    }
    v
}

#[test]
fn frame_block_with_line_grid_and_font_glyph() {
    let mut block = frame_grid("F", &[("j", "0.012", "R")], "\\fgdtfont|b0;");
    // a leader from the left border to a filled head
    block.push(line("FL1", "08", [0., 5.], [-15., 5.]));
    block.push(line("FL2", "08", [-15., 5.], [-15., -20.]));
    block.push(head("FL3", "08", [-15., -25.], [-15., -20.], 0.6));
    let g = run(
        &[("*F1", block)],
        &[insert("FX", "08", "*F1", [100., 50.], 1.)],
    );
    let e = exprs(&g, "FX");
    assert_eq!(
        e.len(),
        1,
        "{:?}",
        e.iter().map(|x| (&x.kind, &x.label)).collect::<Vec<_>>()
    );
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str()),
        ("gps_position", "⌖ 0.012 | R")
    );
    assert_eq!(e[0].rule, "card:gdt_frame_symbol/line_grid_font_glyph");
    let r: BTreeSet<String> = roles(&g, e[0]).into_iter().map(|x| x.0).collect();
    for want in [
        "symbol_glyph",
        "frame_cell",
        "tolerance_value",
        "datum_reference",
        "leader_line",
        "arrowhead",
    ] {
        assert!(r.contains(want), "{want} {r:?}");
    }
    assert!(unresolved(&g).is_empty(), "{:?}", unresolved(&g));
}

#[test]
fn font_letter_in_an_ordinary_font_is_not_a_glyph() {
    let mut block = frame_grid("F", &[("j", "0.012", "R")], "");
    block.push(line("FL1", "08", [0., 5.], [-15., 5.]));
    let g = run(
        &[("*F2", block)],
        &[insert("FX", "08", "*F2", [100., 50.], 1.)],
    );
    assert!(
        exprs(&g, "FX")
            .iter()
            .all(|e| !e.kind.starts_with("gps_") && e.kind != "geometric_tolerance")
    );
}

#[test]
fn stacked_frame_rows_are_two_tolerances() {
    let mut block = frame_grid(
        "F",
        &[("f", "0.015", ""), ("t", "0.04", "U")],
        "\\fgdtfont;",
    );
    // one leader leaves the right border at the row boundary
    block.push(line("FL1", "08", [40., 10.], [55., 10.]));
    block.push(line("FL2", "08", [55., 10.], [55., 30.]));
    block.push(head("FL3", "08", [55., 35.], [55., 30.], 0.6));
    // the second leader is a block of its own, starting at the left border
    let second = vec![
        line("FM1", "08", [-20., 10.], [-0.2, 10.]),
        line("FM2", "08", [-20., 10.], [-20., -15.]),
        head("FM3", "08", [-20., -20.], [-20., -15.], 0.6),
    ];
    let g = run(
        &[("*F3", block), ("*F4", second)],
        &[
            insert("FX", "08", "*F3", [100., 50.], 1.),
            insert("FY", "08", "*F4", [100., 50.], 1.),
        ],
    );
    let mut e: Vec<(&str, &str)> = exprs(&g, "FX")
        .iter()
        .map(|x| (x.kind.as_str(), x.label.as_str()))
        .collect();
    e.sort();
    assert_eq!(
        e,
        vec![
            ("gps_parallelism", "∥ 0.015"),
            ("gps_total_runout", "⌰ 0.04 | U")
        ]
    );
    for x in exprs(&g, "FX") {
        assert!(
            ev(x, "variant:stacked_rows") && ev(x, "variant:multiple_leaders"),
            "{:?}",
            x.evidence_ids
        );
        assert!(
            exprs(&g, "FY").iter().any(|y| y.id == x.id),
            "the second leader belongs to each row"
        );
    }
    assert!(unresolved(&g).is_empty(), "{:?}", unresolved(&g));
}

#[test]
fn mask_joins_the_frame_it_follows() {
    let mut block = frame_grid("F", &[("j", "0.012", "R")], "\\fgdtfont;");
    block.push(line("FL1", "08", [0., 5.], [-15., 5.]));
    block.push(line("FL2", "08", [-15., 5.], [-15., -20.]));
    block.push(head("FL3", "08", [-15., -25.], [-15., -20.], 0.6));
    let at = [100., 50.];
    let mask = wipeout(
        "WM",
        "08",
        at,
        [40., 10.],
        &[[100., 50.], [140., 50.], [140., 60.], [100., 60.]],
    );
    let g = run(
        &[("*F5", block)],
        &[mask, insert("FX", "08", "*F5", at, 1.)],
    );
    let e = exprs(&g, "WM");
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].kind, "gps_position");
    let m = g.members.iter().find(|m| m.source_handle == "WM").unwrap();
    assert_eq!(
        (m.member_role.as_str(), m.line_type.as_str()),
        ("native_member", "fill")
    );
    assert!(m.evidence_ids.contains(&"card:annotation_mask".to_string()));
    assert!(unresolved(&g).is_empty(), "{:?}", unresolved(&g));
}

#[test]
fn mask_following_no_object_stays_unresolved() {
    let block = frame_grid("F", &[("j", "0.012", "R")], "\\fgdtfont;");
    let mask = wipeout(
        "WM",
        "08",
        [300., 300.],
        [20., 20.],
        &[[300., 300.], [320., 300.], [310., 320.]],
    );
    let g = run(
        &[("*F6", block)],
        &[mask, insert("FX", "08", "*F6", [100., 50.], 1.)],
    );
    assert_eq!(
        exprs(&g, "WM")
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["unresolved"]
    );
}

// ---------------------------------------------------------------- cutting planes

/// A section view titled `X-X` beside the view the cutting plane is drawn on.
fn section_views() -> Vec<String> {
    let mut v = outline("PA", 0., 0., 40., 60.);
    v.extend(outline("PB", 100., 0., 140., 60.));
    v.push(text("PT", "11", "P-P", [114., 66.], 5.));
    v
}

#[test]
fn cutting_plane_symbol_block_names_its_section() {
    let block = vec![
        line_w("K1", "08", 50, [0., 70.], [0., 62.]),
        line_w("K2", "08", 50, [0., 8.], [0., 0.]),
        line_w("K3", "08", 18, [0., 70.], [-12., 70.]),
        line_w("K4", "08", 18, [0., 0.], [-12., 0.]),
        head("K5", "08", [-12., 70.], [-6., 70.], 0.75),
        head("K6", "08", [-12., 0.], [-6., 0.], 0.75),
        mtext("K7", "08", "P", [-6., 75.], 5., 5),
        mtext("K8", "08", "P", [-6., -5.], 5., 5),
    ];
    let mut ents = section_views();
    ents.push(insert("KP", "08", "*K1", [20., -5.], 1.));
    let g = run(&[("*K1", block)], &ents);
    let e = exprs(&g, "KP");
    assert_eq!(
        e.len(),
        1,
        "{:?}",
        e.iter().map(|x| (&x.kind, &x.label)).collect::<Vec<_>>()
    );
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str(), e[0].rule.as_str()),
        ("view_callout", "P", "card:view_callout/cutting_plane_group")
    );
    assert!(e[0].qualifiers.contains(&"cutting_plane".to_string()));
    assert!(
        ev(e[0], "references_view:view:P-P"),
        "{:?}",
        e[0].evidence_ids
    );
    let strokes: Vec<(String, String)> = g
        .members
        .iter()
        .filter(|m| m.expression_id == e[0].id && m.member_role == "cutting_stroke")
        .map(|m| {
            (
                m.native_member_id.clone().unwrap_or_default(),
                m.line_type.clone(),
            )
        })
        .collect();
    assert_eq!(
        strokes,
        vec![
            ("KP/K1".into(), "thick_continuous".into()),
            ("KP/K2".into(), "thick_continuous".into())
        ]
    );
    assert!(e[0].concept_ids.contains(&"DC-G2-043".to_string()));
}

#[test]
fn aligned_cutting_plane_block_with_a_turn() {
    // the path turns at the view centre into an oblique plane (cos 0.6)
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
    let g = run(
        &[("*K2", block)],
        &[insert("KP", "08", "*K2", [0., 0.], 1.)],
    );
    let e = exprs(&g, "KP");
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].rule, "card:view_callout/cutting_plane_group");
    assert!(
        ev(e[0], "intersecting_cutting_planes"),
        "{:?}",
        e[0].evidence_ids
    );
    assert!(
        e[0].concept_ids.contains(&"DC-G2-016".to_string()),
        "{:?}",
        e[0].concept_ids
    );
    assert_eq!(
        g.members
            .iter()
            .filter(|m| m.expression_id == e[0].id && m.member_role == "view_letter")
            .count(),
        3
    );
}

#[test]
fn arrow_block_without_strokes_is_not_a_cutting_plane() {
    let block = vec![
        line_w("K3", "08", 18, [0., 70.], [-12., 70.]),
        line_w("K4", "08", 18, [0., 0.], [-12., 0.]),
        head("K5", "08", [-12., 70.], [-6., 70.], 0.75),
        head("K6", "08", [-12., 0.], [-6., 0.], 0.75),
        mtext("K7", "08", "P", [-6., 75.], 5., 5),
        mtext("K8", "08", "P", [-6., -5.], 5., 5),
    ];
    let g = run(
        &[("*K3", block)],
        &[insert("KP", "08", "*K3", [20., -5.], 1.)],
    );
    assert!(by_rule(&g, "card:view_callout/cutting_plane_group").is_empty());
}

// ---------------------------------------------------------------- detail indicator, title and view

/// A part outline with a detail indicator at its lower right corner, the enlarged view drawn as one
/// block (×4, boundary circle, outline copy and section lines) and its title block. `shift`
/// moves the copy away from what it should repeat.
fn detail_sheet(shift: f64) -> MdEngineeringExpressionGraph {
    let indicator = vec![
        circle("I1", "02", [0., 0.], 4.),
        line("I2", "02", [2.828, -2.828], [10., -12.]),
        line("I3", "02", [10., -12.], [22., -12.]),
        mtext("I4", "08", "L", [16., -11.5], 4., 8),
    ];
    // the copy in block units (one unit = 4 on the sheet), about the indicator's centre
    let mut copy = vec![
        line_w("Y1", "01", 50, [-3. + shift, shift], [shift, shift]),
        line_w("Y2", "01", 50, [shift, shift], [shift, 3. + shift]),
    ];
    for (k, c) in [0.4, 1.2, 2.0, 2.8].iter().enumerate() {
        copy.push(line_w(
            &format!("Y{}", k + 3),
            "02",
            18,
            [-*c, 0.],
            [0., *c],
        ));
    }
    let view = vec![
        insert("V1", "0", "*D2", [0., 0.], 1.),
        circle("V2", "0", [0., 0.], 4.),
    ];
    let title = vec![
        mtext("T1", "08", "L", [0., 5.], 4., 5),
        line("T2", "02", [-5., 0.], [5., 0.]),
        mtext("T3", "08", "4:1", [0., -5.], 4., 5),
    ];
    let mut ents = outline("P", 0., 0., 60., 30.);
    ents.push(insert("DI", "02", "*D1", [60., 0.], 1.));
    ents.push(insert("DV", "02", "*D3", [150., 20.], 4.));
    ents.push(insert("DT", "08", "*D4", [150., 45.], 1.));
    run(
        &[
            ("*D1", indicator),
            ("*D2", copy),
            ("*D3", view),
            ("*D4", title),
        ],
        &ents,
    )
}

#[test]
fn detail_label_and_view_drawn_as_blocks() {
    let g = detail_sheet(0.);
    let region = g
        .views
        .iter()
        .find(|v| v.kind == "detail_view")
        .expect("detail view region");
    assert_eq!(region.label, "L 4:1");
    let ind = exprs(&g, "DI");
    assert_eq!(ind.len(), 1);
    assert_eq!(
        (ind[0].label.as_str(), ind[0].rule.as_str()),
        ("L", "card:view_callout/detail_indicator")
    );
    assert!(
        ev(ind[0], &format!("references_view:{}", region.id)),
        "{:?}",
        ind[0].evidence_ids
    );
    assert!(
        ind.iter().all(|e| e.kind != "leader_note"),
        "an indicator is not a leader note"
    );
    let title = exprs(&g, "DT");
    assert_eq!(
        (title[0].label.as_str(), title[0].rule.as_str()),
        ("L 4:1", "card:view_callout/name_with_scale")
    );
    assert!(title[0].qualifiers.contains(&"detail_title".to_string()));
    // the label's scale over the measured enlargement is the sheet scale the label assumes
    assert!(
        ev(title[0], "enlargement_measured:4") && ev(title[0], "sheet_scale_assumed_by_label:1:1"),
        "{:?}",
        title[0].evidence_ids
    );
    let kinds: BTreeSet<&str> = exprs(&g, "DV").iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["visible_contour", "section_hatch", "view_callout"]),
        "{kinds:?}"
    );
    for e in exprs(&g, "DV") {
        assert_eq!(
            e.view_id.as_deref(),
            Some(region.id.as_str()),
            "{} {}",
            e.kind,
            e.label
        );
        assert!(ev(e, "enlargement_measured:4"), "{:?}", e.evidence_ids);
    }
    let outline = exprs(&g, "DV")
        .into_iter()
        .find(|e| e.kind == "visible_contour")
        .unwrap();
    let paths: BTreeSet<String> = roles(&g, outline).into_iter().map(|x| x.1).collect();
    assert_eq!(
        paths,
        BTreeSet::from(["DV/V1/Y1".to_string(), "DV/V1/Y2".to_string()])
    );
    assert!(unresolved(&g).is_empty(), "{:?}", unresolved(&g));
}

#[test]
fn block_inside_a_circle_that_copies_nothing_is_not_a_detail_view() {
    let g = detail_sheet(-0.8);
    assert!(by_rule(&g, "card:detail_view/block_view").is_empty());
    assert_eq!(
        exprs(&g, "DV")
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["unresolved"]
    );
}

#[test]
fn circled_letter_with_leader_is_a_datum_not_a_detail() {
    let block = vec![
        circle("Z1", "08", [0., 0.], 5.),
        mtext("Z2", "08", "T", [0., 0.], 5., 5),
        line("Z3", "08", [0., -5.], [0., -15.]),
        line_w("Z4", "08", 50, [-4., -15.], [4., -15.]),
    ];
    let g = run(
        &[("*Z1", block)],
        &[insert("ZB", "08", "*Z1", [0., 0.], 1.)],
    );
    let e = exprs(&g, "ZB");
    assert_eq!(e[0].kind, "datum_symbol");
    assert!(by_rule(&g, "card:view_callout/detail_indicator").is_empty());
}

// ---------------------------------------------------------------- leader notes

fn leader_block(words: &str, tip_head: bool) -> Vec<String> {
    let mut v = vec![
        line("L1", "08", [0., 0.], [-20., 0.]),
        mtext("L2", "08", words, [-10., 1.], 3.5, 8),
        line("L3", "08", [0., 0.], [10., -15.]),
    ];
    if tip_head {
        let d = [10. / 18.028, -15. / 18.028];
        v.push(head(
            "L4",
            "08",
            [10., -15.],
            [10. - 3. * d[0], -15. - 3. * d[1]],
            0.5,
        ));
    }
    v
}

#[test]
fn leader_note_block_with_thread_designation() {
    let g = run(
        &[("*L1", leader_block("\\T1.1;M20×2", true))],
        &[
            circle("H1", "01", [20., -15.], 10.),
            insert("LB", "08", "*L1", [0., 0.], 1.),
        ],
    );
    let e = exprs(&g, "LB");
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str(), e[0].rule.as_str()),
        ("thread_note", "M20×2", "card:leader_note/block_object")
    );
    assert!(ev(e[0], "leader_end_on:H1"), "{:?}", e[0].evidence_ids);
    assert!(ev(e[0], "terminator:filled_arrow"));
    let r: BTreeSet<String> = roles(&g, e[0]).into_iter().map(|x| x.0).collect();
    assert_eq!(
        r,
        BTreeSet::from([
            "dimension_text".to_string(),
            "leader_line".to_string(),
            "arrowhead".to_string()
        ])
    );
}

#[test]
fn leader_note_block_with_plain_words() {
    let g = run(
        &[("*L2", leader_block("配作", true))],
        &[insert("LB", "08", "*L2", [0., 0.], 1.)],
    );
    let e = exprs(&g, "LB");
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str()),
        ("leader_note", "配作")
    );
    assert_eq!(e[0].family, "note");
    // a leader note is not an item of the technical requirements
    assert!(
        !e[0].concept_ids.contains(&"DC-G9-002".to_string()),
        "{:?}",
        e[0].concept_ids
    );
    assert!(by_rule(&g, "card:item_number/underline").is_empty());
}

#[test]
fn chamfer_note_block_without_terminator() {
    // the leader continues the 25° chamfer edge from its outer end; no terminator
    let (c, s) = (25f64.to_radians().cos(), 25f64.to_radians().sin());
    let end = [12., 10. + 2. * s / c];
    let far = [end[0] + 8. * c, end[1] + 8. * s];
    let block = vec![
        line("L1", "08", [0., 0.], [far[0] - end[0], far[1] - end[1]]),
        line(
            "L2",
            "08",
            [far[0] - end[0], far[1] - end[1]],
            [far[0] - end[0] + 15., far[1] - end[1]],
        ),
        mtext(
            "L3",
            "08",
            "\\T1.1;2×25%%D",
            [far[0] - end[0] + 7.5, far[1] - end[1] + 1.],
            3.5,
            8,
        ),
    ];
    let g = run(
        &[("*L3", block)],
        &[
            line("EX1", "01", [10., 10.], end),
            insert("LB", "08", "*L3", end, 1.),
        ],
    );
    let e = exprs(&g, "LB");
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str()),
        ("dimension_chamfer", "2×25°")
    );
    assert!(
        ev(e[0], "leader_end_on:EX1") && ev(e[0], "terminator:none"),
        "{:?}",
        e[0].evidence_ids
    );
}

// ---------------------------------------------------------------- item numbers, parts list, library parts

fn item_block(prefix: &str, number: &str, end: P) -> Vec<String> {
    // the handles keep a letter outside the hexadecimal digits
    vec![
        mtext(&format!("{prefix}1"), "02", number, [6., 1.], 5., 8),
        line(&format!("{prefix}2"), "02", [0., 0.], [12., 0.]),
        line(&format!("{prefix}3"), "02", [0., 0.], end),
        dot(&format!("{prefix}4"), "02", end, 0.6),
    ]
}

#[test]
fn item_number_block_with_dot_is_not_a_note() {
    let mut ents = outline("P", -40., -50., 10., -20.);
    ents.push(insert("NB", "02", "*N1", [0., 0.], 1.));
    let g = run(&[("*N1", item_block("N", "4", [-20., -30.]))], &ents);
    let e = exprs(&g, "NB");
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str(), e[0].rule.as_str()),
        ("item_number", "4", "card:item_number/underline")
    );
    assert!(by_rule(&g, "card:leader_note/block_object").is_empty());
}

#[test]
fn item_number_in_a_circle() {
    let block = vec![
        circle("N1", "02", [0., 0.], 4.),
        mtext("N2", "02", "5", [0., 0.], 5., 5),
        line("N3", "02", [-2.828, -2.828], [-15., -15.]),
        dot("N4", "02", [-15., -15.], 0.6),
    ];
    let g = run(
        &[("*N2", block)],
        &[insert("NB", "02", "*N2", [0., 0.], 1.)],
    );
    let e = exprs(&g, "NB");
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str(), e[0].rule.as_str()),
        ("item_number", "5", "card:item_number/circle")
    );
}

#[test]
fn texture_symbol_with_integer_value_is_not_an_item_number() {
    let mut block = texture("Q", "08", [0., 0.]);
    block.push(mtext("QV", "08", "50", [2., 14.], 3.5, 5));
    let g = run(
        &[("*N3", block)],
        &[insert("NB", "08", "*N3", [0., 0.], 1.)],
    );
    let e = exprs(&g, "NB");
    assert_eq!(e[0].kind, "surface_roughness");
    assert!(by_rule(&g, "card:item_number/underline").is_empty());
}

/// A parts list as one block: header at the bottom, four rows numbered upwards; `code4` is the
/// code of row 4.
fn parts_list_block(code4: &str, first: [&str; 4]) -> Vec<String> {
    let xs = [0., 8., 48., 92., 100., 120.];
    let ys = [0., 10., 17., 24., 31., 38.];
    let mut v = Vec::new();
    for (k, y) in ys.iter().enumerate() {
        v.push(line(&format!("M{k}"), "02", [0., *y], [120., *y]));
    }
    for (k, x) in xs.iter().enumerate() {
        v.push(line(&format!("MV{k}"), "02", [*x, 0.], [*x, 38.]));
    }
    for (k, w) in ["序号", "代号", "名称", "数量", "备注"].iter().enumerate() {
        v.push(mtext(
            &format!("MH{k}"),
            "02",
            w,
            [(xs[k] + xs[k + 1]) / 2., 5.],
            3.5,
            5,
        ));
    }
    let rows = [
        ["XY-11", "轴套"],
        ["XY-12", "压盖"],
        ["XY-14", "垫片"],
        [code4, "螺钉 M5×12"],
    ];
    for (r, row) in rows.iter().enumerate() {
        let y = (ys[r + 1] + ys[r + 2]) / 2.;
        v.push(mtext(&format!("MA{r}"), "02", first[r], [1., y], 3.5, 4));
        v.push(mtext(&format!("MB{r}"), "02", row[0], [9., y], 3.5, 4));
        v.push(mtext(&format!("MC{r}"), "02", row[1], [49., y], 3.5, 4));
        v.push(mtext(&format!("MD{r}"), "02", "1", [93., y], 3.5, 4));
    }
    // an empty remark cell, written as a text that draws nothing
    v.push(mtext("MEX", "02", "\\T1.1;", [101., 13.5], 3.5, 4));
    v
}

/// A screw drawn as a library block: thick outline, thin thread lines, a centre line.
fn screw_block() -> Vec<String> {
    vec![
        line_w("S1", "0", 50, [0., -2.5], [12., -2.5]),
        line_w("S2", "0", 50, [12., -2.5], [12., 2.5]),
        line_w("S3", "0", 50, [12., 2.5], [0., 2.5]),
        line_w("S4", "0", 50, [0., 2.5], [0., -2.5]),
        line_w("S5", "0", 18, [0., -2.1], [12., -2.1]),
        line_w("S6", "0", 18, [0., 2.1], [12., 2.1]),
        ent(
            "LINE",
            "S7",
            "05",
            "370\n18\n10\n-1\n20\n0\n11\n13\n21\n0\n",
        ),
    ]
}

fn assembly(code4: &str) -> MdEngineeringExpressionGraph {
    let mut ents = outline("P", -60., -40., 40., 20.);
    ents.push(insert("ML", "02", "*M1", [200., -100.], 1.));
    ents.push(insert("NX1", "02", "*N1", [0., 30.], 1.));
    ents.push(insert("NX2", "02", "*N2", [20., 30.], 1.));
    ents.push(insert("NX4", "02", "*N3", [-30., 30.], 1.));
    ents.push(insert("SX", "01", "*S1", [-20., -10.], 1.));
    run(
        &[
            ("*M1", parts_list_block(code4, ["1", "2", "3", "4"])),
            ("*N1", item_block("IA", "1", [-30., -40.])),
            ("*N2", item_block("IB", "2", [-10., -60.])),
            ("*N3", item_block("IC", "4", [20., -40.])),
            ("*S1", screw_block()),
        ],
        &ents,
    )
}

#[test]
fn parts_list_block_rows_match_item_numbers() {
    let g = assembly("GB/T 70.1-2008");
    let list = exprs(&g, "ML");
    assert_eq!(list.len(), 1);
    assert_eq!(
        (list[0].kind.as_str(), list[0].rule.as_str()),
        ("parts_list", "card:parts_list/block_table")
    );
    assert_eq!(list[0].view_id.as_deref(), Some("view:sheet"));
    assert_eq!(
        list[0]
            .evidence_ids
            .iter()
            .filter(|x| x.starts_with("row:"))
            .count(),
        4
    );
    assert!(ev(list[0], "header_below_rows"));
    assert!(
        g.members.iter().any(|m| m.expression_id == list[0].id
            && m.native_member_id.as_deref() == Some("ML/MEX")
            && m.evidence_ids.iter().any(|x| x == "empty_cell")),
        "the empty cell belongs to the list"
    );
    for n in ["NX1", "NX2", "NX4"] {
        let e = exprs(&g, n);
        assert_eq!(e[0].kind, "item_number");
        assert!(
            e[0].evidence_ids
                .iter()
                .any(|x| x.starts_with("parts_list_row:")),
            "{:?}",
            e[0].evidence_ids
        );
    }
    let part = exprs(&g, "SX");
    assert_eq!(part.len(), 1);
    assert_eq!(part[0].kind, "standard_part");
    assert!(
        part[0].label.contains("GB/T 70.1-2008") && part[0].label.starts_with('4'),
        "{}",
        part[0].label
    );
    let r: BTreeSet<String> = roles(&g, part[0]).into_iter().map(|x| x.0).collect();
    assert!(
        r.contains("contour_segment") && r.contains("center_line"),
        "{r:?}"
    );
    assert!(unresolved(&g).is_empty(), "{:?}", unresolved(&g));
}

#[test]
fn structure_block_without_a_standard_row_is_not_a_standard_part() {
    let g = assembly("XY-13");
    assert!(by_rule(&g, "card:standard_part/library_block").is_empty());
    assert_eq!(
        exprs(&g, "SX")
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["unresolved"]
    );
}

#[test]
fn table_block_without_item_column_is_not_a_parts_list() {
    let g = run(
        &[("*M2", parts_list_block("XY-13", ["P", "Q", "R", "S"]))],
        &[insert("ML", "02", "*M2", [0., 0.], 1.)],
    );
    assert!(by_rule(&g, "card:parts_list/block_table").is_empty());
}

// ---------------------------------------------------------------- sheet frame

fn frame_block(inner_texts: bool) -> Vec<String> {
    let mut v = vec![
        line("G1", "02", [0., 0.], [420., 0.]),
        line("G2", "02", [420., 0.], [420., 297.]),
        line("G3", "02", [420., 297.], [0., 297.]),
        line("G4", "02", [0., 297.], [0., 0.]),
        line("G5", "01", [25., 5.], [415., 5.]),
        line("G6", "01", [415., 5.], [415., 292.]),
        line("G7", "01", [415., 292.], [25., 292.]),
        line("G8", "01", [25., 292.], [25., 5.]),
        // binding-margin columns with their words
        line("G9", "02", [0., 60.], [25., 60.]),
        line("GA", "02", [0., 120.], [25., 120.]),
        line("GB", "02", [12., 60.], [12., 120.]),
        mtext("GC", "08", "签字", [6., 90.], 3.5, 5),
        mtext("GD", "08", "日期", [18., 90.], 3.5, 5),
    ];
    if inner_texts {
        // a title grid in the lower right corner inside the frame
        for (k, y) in [5., 15., 25.].iter().enumerate() {
            v.push(line(&format!("GT{k}"), "02", [285., *y], [415., *y]));
        }
        for (k, x) in [285., 320., 360.].iter().enumerate() {
            v.push(line(&format!("GU{k}"), "02", [*x, 5.], [*x, 25.]));
        }
        v.push(mtext("GV", "08", "设计", [300., 10.], 3.5, 5));
        v.push(mtext("GW", "08", "审核", [300., 20.], 3.5, 5));
        v.push(mtext("GX", "08", "比例", [340., 10.], 3.5, 5));
    }
    v
}

#[test]
fn frame_block_with_margin_columns_is_not_a_title_block() {
    let mut ents = outline("P", 100., 100., 200., 160.);
    ents.push(insert("FR", "0", "*G1", [0., 0.], 1.));
    let g = run(&[("*G1", frame_block(false))], &ents);
    let e = exprs(&g, "FR");
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].kind.as_str(), e[0].rule.as_str()),
        (
            "sheet_frame",
            "card:sheet_frame_border/border_with_margin_columns"
        )
    );
    assert!(g.expressions.iter().all(|x| x.kind != "title_block"));
}

#[test]
fn frame_block_holding_a_title_grid_stays_a_title_block() {
    let mut ents = outline("P", 100., 100., 200., 160.);
    ents.push(insert("FR", "0", "*G2", [0., 0.], 1.));
    let g = run(&[("*G2", frame_block(true))], &ents);
    let e = exprs(&g, "FR");
    assert_eq!(e[0].kind, "title_block");
    assert!(by_rule(&g, "card:sheet_frame_border/border_with_margin_columns").is_empty());
}

// ---------------------------------------------------------------- attachment to a dimension

#[test]
fn datum_block_aligned_with_a_dimension_line_takes_its_view() {
    // a vertical size of the left view, its dimension line drawn far to the right, nearer the
    // right view; the datum symbol's connector continues that dimension line
    let dim_block = vec![
        line("DL1", "08", [120., 0.], [120., 60.]),
        line("DL2", "08", [40., 0.], [123., 0.]),
        line("DL3", "08", [40., 60.], [123., 60.]),
        mtext("DL4", "08", "60", [125., 30.], 3.5, 5),
    ];
    let dim = ent(
        "DIMENSION",
        "DM",
        "08",
        "2\n*D9\n10\n120\n20\n60\n11\n125\n21\n30\n70\n32\n13\n40\n23\n0\n14\n40\n24\n60\n42\n60\n50\n90\n",
    );
    let datum = vec![
        circle("Z1", "08", [0., -20.], 5.),
        mtext("Z2", "08", "S", [0., -20.], 5., 5),
        line("Z3", "08", [0., -15.], [0., 0.]),
        line_w("Z4", "08", 50, [-5., 0.], [5., 0.]),
    ];
    let mut ents = outline("PL", 0., 0., 40., 60.);
    ents.push(line("PL5", "01", [0., 30.], [40., 30.]));
    ents.extend(outline("PR", 150., 0., 210., 60.));
    ents.push(dim);
    ents.push(insert("ZB", "08", "*Z9", [120., -10.], 1.));
    let g = run(&[("*D9", dim_block), ("*Z9", datum)], &ents);
    let e = exprs(&g, "ZB");
    assert_eq!(e[0].kind, "datum_symbol");
    assert!(
        ev(e[0], "connector_along_dimension_line:DM"),
        "{:?}",
        e[0].evidence_ids
    );
    let d = exprs(&g, "DM");
    assert_eq!(
        e[0].view_id, d[0].view_id,
        "the datum takes the view of the dimensioned feature"
    );
    let left = g
        .views
        .iter()
        .find(|v| v.bbox[0] < 1. && v.bbox[2] < 50.)
        .expect("left view");
    assert_eq!(e[0].view_id.as_deref(), Some(left.id.as_str()));
}

// ---------------------------------------------------------------- viewing-direction arrows

#[test]
fn view_arrow_block_with_a_filled_hatch_head() {
    let block = vec![
        line("W1", "08", [0., 0.], [0., 12.]),
        head("W2", "08", [0., 0.], [0., 4.], 0.8),
        mtext("W3", "08", "Q", [4., 8.], 5., 5),
    ];
    let mut ents = outline("PV", -20., -40., 20., -5.);
    ents.push(insert("WA", "08", "*W1", [0., -5.], 1.));
    let g = run(&[("*W1", block)], &ents);
    let e = exprs(&g, "WA");
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].kind.as_str(), e[0].label.as_str(), e[0].rule.as_str()),
        ("view_callout", "Q", "card:view_callout/arrow_block")
    );
    assert!(e[0].qualifiers.contains(&"view_direction".to_string()));
    assert!(
        e[0].concept_ids.contains(&"DC-G2-072".to_string()),
        "{:?}",
        e[0].concept_ids
    );
    assert!(
        by_rule(&g, "card:view_callout/cutting_plane_group").is_empty(),
        "no strokes, no cutting plane"
    );
}
