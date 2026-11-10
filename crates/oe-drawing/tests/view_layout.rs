//! View layout: the first-angle arrangement of the basic views and the principal view, regions
//! inside other views, several sheets in one model space, view names written apart (split section
//! names, names on leader shoulders, arrows and letters apart, cutting strokes without arrows,
//! whole cutting paths) and detail views with loose names and boundaries (cards view_arrangement,
//! drawing_sheet, view_callout and detail_view). Fixtures are authored from the card knowledge
//! (GB/T 14692, GB/T 4458.1, GB/T 4458.6, GB/T 14689, GB/T 10609.1); handles, coordinates,
//! letters, numbers and scales are invented and differ from any project drawing.
use oe_contracts::{MdDrawingView, MdEngineeringExpression, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::discover;
use oe_drawing::expression_text::{Lex, is_scale, lex, roman_value, scale_text};
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
fn text(id: &str, layer: &str, s: &str, p: [f64; 2], h: f64) -> String {
    ent(
        "TEXT",
        id,
        layer,
        &format!("1\n{s}\n10\n{}\n20\n{}\n40\n{h}\n50\n0\n", p[0], p[1]),
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
/// A rectangle of thick lines `p` (four sides) with extra full-width cross lines at the given
/// heights (more pieces make a view larger in the size order).
fn view(p: &str, x0: f64, y0: f64, x1: f64, y1: f64, cross: &[f64]) -> Vec<String> {
    let mut v = vec![
        line(&format!("{p}1"), "01", [x0, y0], [x1, y0]),
        line(&format!("{p}2"), "01", [x1, y0], [x1, y1]),
        line(&format!("{p}3"), "01", [x1, y1], [x0, y1]),
        line(&format!("{p}4"), "01", [x0, y1], [x0, y0]),
    ];
    for (i, y) in cross.iter().enumerate() {
        v.push(line(&format!("{p}X{i}"), "01", [x0, *y], [x1, *y]));
    }
    v
}
fn view_of<'a>(g: &'a MdEngineeringExpressionGraph, src: &str) -> Option<&'a MdDrawingView> {
    let e = exprs_of(g, src)
        .into_iter()
        .find(|e| e.kind == "visible_contour")?;
    g.views.iter().find(|v| Some(&v.id) == e.view_id.as_ref())
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
fn rule_expr<'a>(
    g: &'a MdEngineeringExpressionGraph,
    rule: &str,
) -> Vec<&'a MdEngineeringExpression> {
    g.expressions.iter().filter(|e| e.rule == rule).collect()
}
fn roles(g: &MdEngineeringExpressionGraph, e: &MdEngineeringExpression) -> Vec<(String, String)> {
    g.members
        .iter()
        .filter(|m| m.expression_id == e.id)
        .map(|m| (m.source_handle.clone(), m.member_role.clone()))
        .collect()
}
fn principal(g: &MdEngineeringExpressionGraph) -> &MdDrawingView {
    g.views
        .iter()
        .find(|v| v.kind == "principal")
        .unwrap_or_else(|| panic!("{:?}", labels(g)))
}
fn labels(g: &MdEngineeringExpressionGraph) -> Vec<(String, String, String)> {
    g.views
        .iter()
        .map(|v| (v.id.clone(), v.kind.clone(), v.label.clone()))
        .collect()
}

// ---------------------------------------------------------------- view_arrangement

#[test]
fn principal_view_is_flanked_by_aligned_views() {
    // three views at one height; the middle one has the fewest pieces
    let mut ents = view("R", 0., 0., 40., 60., &[20., 40.]);
    ents.extend(view("M", 80., 0., 110., 60., &[]));
    ents.extend(view("L", 150., 0., 190., 60., &[30.]));
    let g = run("", &ents);
    let p = principal(&g);
    assert_eq!(
        view_of(&g, "M1").map(|v| &v.id),
        Some(&p.id),
        "{:?}",
        labels(&g)
    );
    assert!(
        p.evidence_ids.iter().any(|e| e == "variant:flanked"),
        "{:?}",
        p.evidence_ids
    );
    assert_eq!(view_of(&g, "R1").map(|v| v.label.as_str()), Some("右视图"));
    assert_eq!(view_of(&g, "L1").map(|v| v.label.as_str()), Some("左视图"));
}

#[test]
fn principal_view_aligned_in_row_and_column() {
    // a view with an aligned view on its right and one below it; the right one is the largest
    let mut ents = view("M", 0., 50., 60., 80., &[]);
    ents.extend(view("L", 100., 50., 130., 80., &[60., 70.]));
    ents.extend(view("T", 0., 0., 60., 30., &[15.]));
    let g = run("", &ents);
    let p = principal(&g);
    assert_eq!(
        view_of(&g, "M1").map(|v| &v.id),
        Some(&p.id),
        "{:?}",
        labels(&g)
    );
    assert!(p.evidence_ids.iter().any(|e| e == "variant:row_and_column"));
    assert_eq!(view_of(&g, "L1").map(|v| v.label.as_str()), Some("左视图"));
    assert_eq!(view_of(&g, "T1").map(|v| v.label.as_str()), Some("俯视图"));
}

#[test]
fn titled_axial_section_between_end_views_is_principal() {
    let mut ents = Vec::new();
    for (p, x) in [("C", 40.), ("E", 180.)] {
        for (i, r) in [30., 20., 10.].iter().enumerate() {
            ents.push(circle(&format!("{p}{i}"), "01", [x, 40.], *r));
        }
    }
    ents.extend(view("S", 100., 10., 120., 70., &[]));
    ents.push(text("SN", "11", "D-D", [104., 74.], 3.5));
    let g = run("", &ents);
    let p = principal(&g);
    assert_eq!(p.label, "D-D", "{:?}", labels(&g));
    assert!(
        p.evidence_ids
            .iter()
            .any(|e| e == "variant:section_principal")
    );
    assert_eq!(view_of(&g, "S1").map(|v| &v.id), Some(&p.id));
    assert_eq!(view_of(&g, "C0").map(|v| v.label.as_str()), Some("右视图"));
    assert_eq!(view_of(&g, "E0").map(|v| v.label.as_str()), Some("左视图"));
}

#[test]
fn two_aligned_views_principal_is_the_left_one() {
    let mut ents = view("P", 0., 0., 30., 60., &[]);
    ents.extend(view("Q", 70., 0., 130., 60., &[20., 40.]));
    let g = run("", &ents);
    let p = principal(&g);
    assert_eq!(
        view_of(&g, "P1").map(|v| &v.id),
        Some(&p.id),
        "{:?}",
        labels(&g)
    );
    assert!(
        p.evidence_ids
            .iter()
            .any(|e| e == "variant:two_aligned_views")
    );
    assert_eq!(view_of(&g, "Q1").map(|v| v.label.as_str()), Some("左视图"));
}

#[test]
fn fragment_inside_a_view_box_joins_that_view() {
    let mut ents = view("O", 0., 0., 100., 100., &[]);
    ents.extend(view("I", 45., 45., 55., 55., &[]));
    let g = run("", &ents);
    assert_eq!(
        g.views.iter().filter(|v| v.kind != "sheet").count(),
        1,
        "{:?}",
        labels(&g)
    );
    assert_eq!(
        view_of(&g, "I1").map(|v| &v.id),
        view_of(&g, "O1").map(|v| &v.id)
    );
}

#[test]
fn titled_views_in_a_row_do_not_make_a_principal() {
    // removed sections in a row under a long view: the middle section is not the principal view
    let mut ents = view("W", 0., 100., 200., 130., &[115.]);
    for (p, x, name) in [("C", 20., "C-C"), ("D", 90., "D-D"), ("E", 160., "E-E")] {
        ents.extend(view(p, x, 0., x + 20., 20., &[]));
        ents.push(text(&format!("{p}N"), "11", name, [x + 3., 24.], 3.5));
    }
    let g = run("", &ents);
    let p = principal(&g);
    assert_eq!(
        view_of(&g, "W1").map(|v| &v.id),
        Some(&p.id),
        "{:?}",
        labels(&g)
    );
    assert!(
        p.evidence_ids
            .iter()
            .any(|e| e == "largest_untitled_region")
    );
    assert_eq!(
        view_of(&g, "D1").map(|v| v.kind.as_str()),
        Some("section_view")
    );
}

#[test]
fn views_overlapping_only_in_part_are_not_aligned() {
    let mut ents = view("P", 0., 0., 30., 60., &[]);
    ents.extend(view("Q", 70., 30., 130., 100., &[50., 70.]));
    let g = run("", &ents);
    let p = principal(&g);
    assert_eq!(
        view_of(&g, "Q1").map(|v| &v.id),
        Some(&p.id),
        "{:?}",
        labels(&g)
    );
    assert!(
        !g.views
            .iter()
            .any(|v| v.label == "左视图" || v.label == "右视图")
    );
}

#[test]
fn titled_view_inside_a_view_box_stays_a_view() {
    let mut ents = view("O", 0., 0., 100., 100., &[]);
    ents.extend(view("I", 40., 30., 60., 50., &[]));
    ents.push(text("IN", "11", "E向", [44., 53.], 3.5));
    let g = run("", &ents);
    let v = view_of(&g, "I1").unwrap_or_else(|| panic!("{:?}", labels(&g)));
    assert_eq!(v.label, "E向");
    assert_ne!(view_of(&g, "O1").map(|v| &v.id), Some(&v.id));
}

// ---------------------------------------------------------------- drawing_sheet

/// One sheet frame with a title block in its lower right corner, a view, and title-block texts.
fn sheet(n: usize, x: f64, with_title: bool, date: &str) -> Vec<String> {
    let mut v = vec![insert(&format!("F{n}"), "0", "FRM", [x, 0.])];
    if with_title {
        v.push(insert(&format!("T{n}"), "0", "TBK", [x + 240., 0.]));
        v.push(text(
            &format!("N{n}"),
            "11",
            "TS.901.77",
            [x + 330., 30.],
            3.5,
        ));
        v.push(text(&format!("D{n}"), "11", date, [x + 260., 10.], 3.5));
    }
    v.extend(view(&format!("V{n}"), x + 50., 150., x + 150., 250., &[]));
    v
}
fn sheet_blocks() -> String {
    let frame = vec![lwpoly(
        "r",
        "0",
        &[[0., 0.], [420., 0.], [420., 297.], [0., 297.]],
        true,
    )];
    let mut title = Vec::new();
    for (i, y) in [0., 14., 28., 42., 56.].iter().enumerate() {
        title.push(line(&format!("h{i}"), "0", [0., *y], [180., *y]));
    }
    for (i, x) in [0., 60., 120., 180.].iter().enumerate() {
        title.push(line(&format!("v{i}"), "0", [*x, 0.], [*x, 56.]));
    }
    title.push(text("w1", "0", "设计", [3., 45.], 3.5));
    title.push(text("w2", "0", "审核", [3., 31.], 3.5));
    blocks(&[("FRM", frame), ("TBK", title)])
}
/// Two terminators facing each other across the gap between two sheets, a value beside one.
fn facing_terminators() -> Vec<String> {
    vec![
        line("DL1", "02", [360., 200.], [380., 200.]),
        solid("DA1", "02", [380., 200.], [377., 200.]),
        line("DL2", "02", [520., 200.], [540., 200.]),
        solid("DA2", "02", [520., 200.], [523., 200.]),
        text("DT", "11", "25", [384., 201.], 3.5),
    ]
}

#[test]
fn two_sheets_in_one_model_space_keep_their_views_apart() {
    let mut ents = sheet(1, 0., true, "20240315");
    ents.extend(sheet(2, 500., true, "20250620"));
    ents.extend(facing_terminators());
    let g = run(&sheet_blocks(), &ents);
    let ids: Vec<&str> = g.views.iter().map(|v| v.id.as_str()).collect();
    let unique: BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "{ids:?}");
    for s in ["view:sheet-1", "view:sheet-2"] {
        assert!(ids.contains(&s), "{ids:?}");
        assert!(ids.contains(&format!("{s}:principal").as_str()), "{ids:?}");
    }
    // every view lies within its own sheet
    for v in g.views.iter().filter(|v| v.kind != "sheet") {
        let s = g
            .views
            .iter()
            .find(|x| v.id.starts_with(&format!("{}:", x.id)))
            .unwrap_or_else(|| panic!("{}", v.id));
        assert!(
            v.bbox[0] >= s.bbox[0] && v.bbox[2] <= s.bbox[2],
            "{} {:?}",
            v.id,
            s.bbox
        );
    }
    // terminators on two sheets are never one dimension
    assert!(
        !g.expressions.iter().any(|e| {
            let r = roles(&g, e);
            r.iter().any(|x| x.0 == "DA1") && r.iter().any(|x| x.0 == "DA2")
        }),
        "a dimension paired terminators across sheets"
    );
}

#[test]
fn later_dated_sheet_with_a_revision_note_supersedes_the_earlier() {
    let mut ents = sheet(1, 0., true, "20240315");
    ents.extend(sheet(2, 500., true, "20250620"));
    let plain = run(&sheet_blocks(), &ents);
    let ev = |g: &MdEngineeringExpressionGraph, id: &str| {
        g.views
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.evidence_ids.clone())
            .unwrap_or_default()
    };
    let e2 = ev(&plain, "view:sheet-2");
    assert!(
        e2.iter().any(|x| x == "same_drawing_number:view:sheet-1"),
        "{e2:?}"
    );
    assert!(
        !e2.iter().any(|x| x.starts_with("supersedes:")),
        "no note, no supersession"
    );
    ents.push(text("RN", "11", "改 20250620", [700., -30.], 3.5));
    let g = run(&sheet_blocks(), &ents);
    let (e1, e2) = (ev(&g, "view:sheet-1"), ev(&g, "view:sheet-2"));
    assert!(
        e2.iter().any(|x| x == "supersedes:view:sheet-1")
            && e2.iter().any(|x| x == "revision_note:RN")
            && e2.iter().any(|x| x == "title_block_date:20250620"),
        "{e2:?}"
    );
    assert!(
        e1.iter().any(|x| x == "superseded_by:view:sheet-2"),
        "{e1:?}"
    );
}

#[test]
fn one_sheet_keeps_plain_view_ids() {
    let g = run(&sheet_blocks(), &sheet(1, 0., true, "20240315"));
    let ids: Vec<&str> = g.views.iter().map(|v| v.id.as_str()).collect();
    assert!(
        ids.contains(&"view:sheet") && ids.contains(&"view:principal"),
        "{ids:?}"
    );
    assert!(!ids.iter().any(|i| i.contains("sheet-")), "{ids:?}");
    // and the names stay plain
    assert!(
        !g.views.iter().any(|v| v.label.starts_with("图纸 ")),
        "{:?}",
        labels(&g)
    );
}

#[test]
fn views_on_two_sheets_name_their_sheet() {
    let mut ents = sheet(1, 0., true, "20240315");
    ents.extend(sheet(2, 500., true, "20250620"));
    ents.push(text("RN", "11", "改 20250620", [700., -30.], 3.5));
    let g = run(&sheet_blocks(), &ents);
    let label = |id: &str| {
        g.views
            .iter()
            .find(|v| v.id == id)
            .map(|v| v.label.clone())
            .unwrap_or_else(|| panic!("{id}: {:?}", labels(&g)))
    };
    assert_eq!(label("view:sheet-1:principal"), "图纸 1 · 主视图");
    assert_eq!(label("view:sheet-2:principal"), "图纸 2 · 主视图");
    assert_eq!(label("view:sheet-2"), "图纸 2");
    // expressions grouped by view name the sheet too
    let e = exprs_of(&g, "V21")
        .into_iter()
        .find(|e| e.kind == "visible_contour")
        .unwrap();
    assert!(e.label.starts_with("图纸 2 · 主视图"), "{}", e.label);
    // the later edition supersedes the earlier as a candidate, with its evidence
    let s2 = g.views.iter().find(|v| v.id == "view:sheet-2").unwrap();
    for x in [
        "supersedes:view:sheet-1",
        "supersession:candidate",
        "title_block_date:20250620",
        "earlier_title_block_date:20240315",
        "revision_note:RN",
    ] {
        assert!(
            s2.evidence_ids.iter().any(|y| y == x),
            "{x}: {:?}",
            s2.evidence_ids
        );
    }
    assert!(
        s2.unknowns.iter().any(|u| u.contains("candidate")),
        "{:?}",
        s2.unknowns
    );
    let s1 = g.views.iter().find(|v| v.id == "view:sheet-1").unwrap();
    assert!(
        s1.unknowns.iter().any(|u| u.contains("candidate")),
        "{:?}",
        s1.unknowns
    );
}

#[test]
fn frame_without_a_title_block_is_not_a_sheet() {
    let mut ents = sheet(1, 0., true, "20240315");
    ents.extend(sheet(2, 500., false, ""));
    let g = run(&sheet_blocks(), &ents);
    let ids: Vec<&str> = g.views.iter().map(|v| v.id.as_str()).collect();
    assert!(ids.contains(&"view:sheet"), "{ids:?}");
    assert!(!ids.iter().any(|i| i.contains("sheet-")), "{ids:?}");
}

// ---------------------------------------------------------------- view_callout

#[test]
fn section_name_written_as_two_letters_and_a_dash() {
    let mut ents = view("S", 0., 0., 40., 30., &[]);
    ents.push(text("L1", "11", "C", [10., 34.], 3.5));
    ents.push(text("L2", "11", "C", [22., 34.], 3.5));
    ents.push(line("DS", "01", [14.5, 35.75], [20., 35.75]));
    let g = run("", &ents);
    let e = rule_expr(&g, "card:view_callout/section_name_split");
    let [e] = e.as_slice() else {
        panic!("{:?}", labels(&g))
    };
    assert_eq!(e.label, "C—C");
    assert!(e.concept_ids.iter().any(|c| c == "DC-G2-065"));
    assert!(
        exprs_of(&g, "DS").iter().all(|x| x.kind == "view_callout"),
        "the dash is part of the name, not an outline"
    );
    assert_eq!(
        view_of(&g, "S1").map(|v| (v.kind.as_str(), v.label.as_str())),
        Some(("section_view", "C—C"))
    );
}

fn leader_name(with_leader: bool) -> MdEngineeringExpressionGraph {
    let mut ents = view("V", 0., 0., 100., 80., &[]);
    ents.push(text("NM", "11", "D-D", [60., 92.], 3.5));
    ents.push(line("SH", "02", [56., 91.], [75., 91.]));
    if with_leader {
        ents.push(line("LD", "02", [56., 91.], [40., 60.]));
        ents.push(text("NT", "11", "4处", [58., 86.], 3.5));
    }
    run("", &ents)
}

#[test]
fn section_name_on_a_leader_shoulder_names_a_local_section() {
    let g = leader_name(true);
    let e = exprs_of(&g, "NM")
        .into_iter()
        .find(|e| e.kind == "view_callout")
        .unwrap();
    assert!(
        e.qualifiers.iter().any(|q| q == "local_section_name"),
        "{:?}",
        e.qualifiers
    );
    let r = roles(&g, e);
    for (h, role) in [
        ("SH", "leader_line"),
        ("LD", "leader_line"),
        ("NT", "note_text"),
    ] {
        assert!(r.contains(&(h.to_string(), role.to_string())), "{r:?}");
    }
    assert!(
        !g.views.iter().any(|v| v.label == "D-D"),
        "{:?}",
        labels(&g)
    );
    assert_eq!(
        view_of(&g, "V1").map(|v| v.kind.as_str()),
        Some("principal")
    );
}

#[test]
fn section_name_on_a_plain_line_still_titles_the_view() {
    let g = leader_name(false);
    assert_eq!(
        view_of(&g, "V1").map(|v| (v.kind.as_str(), v.label.as_str())),
        Some(("section_view", "D-D")),
        "{:?}",
        labels(&g)
    );
}

fn arrow_parts(p: &str) -> Vec<String> {
    vec![
        line(&format!("{p}s"), "02", [0., 0.], [12., 0.]),
        solid(&format!("{p}h"), "02", [15., 0.], [12., 0.]),
    ]
}

#[test]
fn arrow_block_takes_a_loose_letter_beside_it() {
    let g = run(
        &blocks(&[("VAB", arrow_parts("a"))]),
        &[
            insert("VA", "02", "VAB", [100., 100.]),
            text("VL", "11", "H", [104., 102.5], 3.5),
        ],
    );
    let e = rule_expr(&g, "card:view_callout/arrow_and_letter_apart");
    let [e] = e.as_slice() else {
        panic!(
            "{:?}",
            g.expressions.iter().map(|e| &e.rule).collect::<Vec<_>>()
        )
    };
    assert_eq!(e.label, "H");
    let r = roles(&g, e);
    assert!(
        r.contains(&("VA".into(), "view_arrow".into()))
            && r.contains(&("VL".into(), "view_letter".into())),
        "{r:?}"
    );
    assert!(e.concept_ids.iter().any(|c| c == "DC-G2-072"));
}

#[test]
fn loose_arrow_takes_a_letter_block() {
    let g = run(
        &blocks(&[("LET", vec![text("t", "0", "H", [0., 0.], 3.5)])]),
        &[
            line("AS", "02", [100., 100.], [112., 100.]),
            solid("AH", "02", [115., 100.], [112., 100.]),
            insert("AL", "11", "LET", [102., 103.]),
        ],
    );
    let e = rule_expr(&g, "card:view_callout/arrow_and_letter_apart");
    let [e] = e.as_slice() else {
        panic!("no callout")
    };
    let r = roles(&g, e);
    for (h, role) in [
        ("AS", "view_arrow"),
        ("AH", "arrowhead"),
        ("AL", "view_letter"),
    ] {
        assert!(r.contains(&(h.to_string(), role.to_string())), "{r:?}");
    }
}

fn unarrowed_strokes(named: bool) -> MdEngineeringExpressionGraph {
    let mut ents = view("V", 0., 0., 100., 60., &[]);
    ents.push(line("K1", "01", [50., 64.], [50., 69.]));
    ents.push(line("K2", "01", [50., -4.], [50., -9.]));
    ents.push(text("E1", "11", "E", [52., 69.], 3.5));
    ents.push(text("E2", "11", "E", [52., -12.], 3.5));
    if named {
        ents.extend(view("W", 200., 0., 240., 30., &[]));
        ents.push(text("EN", "11", "E-E", [212., 34.], 3.5));
    }
    run("", &ents)
}

#[test]
fn cutting_strokes_without_arrows_with_letters() {
    let g = unarrowed_strokes(true);
    let e = rule_expr(&g, "card:view_callout/strokes_without_arrows");
    let [e] = e.as_slice() else {
        panic!("no cutting plane")
    };
    assert_eq!(e.label, "E");
    let r = roles(&g, e);
    for (h, role) in [
        ("K1", "cutting_stroke"),
        ("K2", "cutting_stroke"),
        ("E1", "view_letter"),
        ("E2", "view_letter"),
    ] {
        assert!(r.contains(&(h.to_string(), role.to_string())), "{r:?}");
    }
    assert!(exprs_of(&g, "K1").iter().all(|x| x.kind == "view_callout"));
    assert!(e.concept_ids.iter().any(|c| c == "DC-G2-043"));
}

#[test]
fn lettered_strokes_without_a_section_name_are_not_a_cutting_plane() {
    let g = unarrowed_strokes(false);
    assert!(rule_expr(&g, "card:view_callout/strokes_without_arrows").is_empty());
}

/// A cutting plane through a circular view: start stroke with arrow and letter, a turn at the
/// centre (two strokes, letter), an end stroke with an arrow and no letter.
fn cutting_path(extra: Vec<String>) -> MdEngineeringExpressionGraph {
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
    ents.extend(extra);
    run("", &ents)
}

#[test]
fn cutting_path_keeps_its_turn_letter_and_end_arrow() {
    let g = cutting_path(vec![]);
    let e = exprs_of(&g, "M1")
        .into_iter()
        .find(|e| e.kind == "view_callout")
        .unwrap_or_else(|| panic!("no callout"));
    let r = roles(&g, e);
    for (h, role) in [
        ("S1", "cutting_stroke"),
        ("T1", "cutting_stroke"),
        ("T2", "cutting_stroke"),
        ("M2", "view_letter"),
        ("S2", "cutting_stroke"),
        ("A2", "view_arrow"),
        ("H2", "arrowhead"),
    ] {
        assert!(r.contains(&(h.to_string(), role.to_string())), "{h}: {r:?}");
    }
    assert!(e.evidence_ids.iter().any(|x| x == "variant:cutting_path"));
    for s in ["S1", "T1", "T2", "S2"] {
        assert!(
            exprs_of(&g, s).iter().all(|x| x.kind == "view_callout"),
            "{s} is not an outline"
        );
    }
}

#[test]
fn stroke_off_the_cutting_plane_is_not_taken() {
    let g = cutting_path(vec![line("X1", "01", [62., 140.], [67., 140.])]);
    let e = exprs_of(&g, "M1")
        .into_iter()
        .find(|e| e.kind == "view_callout")
        .unwrap();
    assert!(!roles(&g, e).iter().any(|x| x.0 == "X1"));
}

#[test]
fn stroke_at_the_tail_of_an_arrow_block() {
    let mut parts = arrow_parts("a");
    parts.push(insert("n", "0", "VLN", [0., 0.]));
    let g = run(
        &blocks(&[
            ("VLN", vec![text("t", "0", "N", [-4., 1.], 3.5)]),
            ("VAN", parts),
        ]),
        &[
            insert("VB", "02", "VAN", [100., 100.]),
            line("SK", "01", [100., 96.], [100., 104.]),
        ],
    );
    let e = exprs_of(&g, "VB")
        .into_iter()
        .find(|e| e.kind == "view_callout")
        .unwrap();
    assert_eq!(e.label, "N");
    assert!(roles(&g, e).contains(&("SK".into(), "cutting_stroke".into())));
}

// ---------------------------------------------------------------- detail_view

#[test]
fn detail_name_with_rotation_word_and_full_width_colon() {
    assert_eq!(roman_value("Ⅲ"), Some(3));
    assert_eq!(roman_value("III"), roman_value("Ⅲ"));
    assert!(is_scale("3：1") && lex("3：1") == Lex::Scale && scale_text("3：1") == "3:1");
    let mut ents = view("M", 0., 0., 60., 40., &[]);
    ents.extend(view("Z", 100., 0., 130., 20., &[]));
    ents.push(text("U", "11", "Ⅲ 旋转", [108., 32.], 3.5));
    ents.push(line("UL", "02", [106., 29.5], [122., 29.5]));
    ents.push(text("SC", "11", "3：1", [109., 25.], 3.5));
    let g = run("", &ents);
    let e = rule_expr(&g, "card:detail_view/title_forms");
    let [e] = e.as_slice() else {
        panic!("{:?}", labels(&g))
    };
    assert_eq!(e.label, "Ⅲ 旋转 3:1");
    assert!(
        e.concept_ids.iter().any(|c| c == "DC-G2-048")
            && e.concept_ids.iter().any(|c| c == "DC-G2-075")
    );
    let v = view_of(&g, "Z1").unwrap();
    assert_eq!(
        (v.kind.as_str(), v.label.as_str(), v.scale_text.as_deref()),
        ("detail_view", "Ⅲ 旋转 3:1", Some("3:1"))
    );
}

#[test]
fn letter_detail_name_over_a_scale() {
    let mut ents = view("M", 0., 0., 60., 40., &[]);
    ents.extend(view("Z", 100., 0., 130., 20., &[]));
    ents.push(text("U", "11", "D", [112., 32.], 3.5));
    ents.push(text("SC", "11", "4:1", [110., 25.], 3.5));
    let g = run("", &ents);
    let v = view_of(&g, "Z1").unwrap();
    assert_eq!(
        (v.kind.as_str(), v.label.as_str()),
        ("detail_view", "D 4:1"),
        "{:?}",
        labels(&g)
    );
}

#[test]
fn letter_without_a_scale_is_not_a_detail_name() {
    let mut ents = view("M", 0., 0., 60., 40., &[]);
    ents.extend(view("Z", 100., 0., 130., 20., &[]));
    ents.push(text("U", "11", "D", [112., 25.], 3.5));
    let g = run("", &ents);
    assert!(rule_expr(&g, "card:detail_view/title_forms").is_empty());
    assert!(!g.views.iter().any(|v| v.kind == "detail_view"));
}

#[test]
fn detail_view_bounded_by_an_arc_and_a_wavy_line_takes_its_title() {
    let mut ents = view("M", 0., 100., 60., 160., &[]);
    ents.extend(view("Z", 180., 20., 220., 30., &[]));
    // a thin arc over the detail, closed below by a wavy line through both of its ends
    let r: f64 = 50.;
    let (a0, a1) = (20f64, 160f64);
    let end = |a: f64| [200. + r * a.to_radians().cos(), r * a.to_radians().sin()];
    ents.push(arc("B", "02", [200., 0.], r, a0, a1));
    let (p, q) = (end(a1), end(a0));
    let mut wave = vec![p];
    for i in 1..10 {
        let t = f64::from(i) / 10.;
        let y = p[1] + if i % 2 == 0 { 1.5 } else { -1.5 };
        wave.push([p[0] + t * (q[0] - p[0]), y]);
    }
    wave.push(q);
    ents.push(lwpoly("W", "02", &wave, false));
    ents.push(text("U", "11", "Ⅱ 旋转", [190., 38.], 3.5));
    ents.push(line("UL", "02", [188., 35.5], [206., 35.5]));
    ents.push(text("SC", "11", "5:1", [192., 31.], 3.5));
    let g = run("", &ents);
    let b = rule_expr(&g, "card:detail_view/boundary_arc");
    let [b] = b.as_slice() else {
        panic!(
            "{:?}",
            g.expressions
                .iter()
                .filter(|e| e.kind == "break_line")
                .map(|e| &e.rule)
                .collect::<Vec<_>>()
        )
    };
    assert_eq!(b.kind, "break_line");
    let v = view_of(&g, "Z1").unwrap();
    assert_eq!(
        (v.kind.as_str(), v.label.as_str()),
        ("detail_view", "Ⅱ 旋转 5:1"),
        "{:?}",
        labels(&g)
    );
    assert!(
        v.evidence_ids
            .iter()
            .any(|x| x == "title_inside_detail_boundary")
    );
    assert!(
        v.bbox[3] >= r - 1. && b.view_id.as_ref() == Some(&v.id),
        "{:?}",
        v.bbox
    );
}

#[test]
fn detail_view_inside_a_boundary_circle() {
    let mut ents = view("M", 0., 100., 60., 160., &[]);
    ents.extend(view("Z", 190., -10., 210., 10., &[]));
    ents.push(circle("B", "02", [200., 0.], 25.));
    ents.push(text("U", "11", "Ⅳ", [198., 33.], 3.5));
    ents.push(text("SC", "11", "4:1", [196., 27.], 3.5));
    let g = run("", &ents);
    let b = rule_expr(&g, "card:detail_view/boundary_circle");
    let [b] = b.as_slice() else {
        panic!("no boundary")
    };
    let v = view_of(&g, "Z1").unwrap();
    assert_eq!(v.kind, "detail_view", "{:?}", labels(&g));
    assert!(
        v.bbox[0] <= 175.5 && b.view_id.as_ref() == Some(&v.id),
        "{:?}",
        v.bbox
    );
}

#[test]
fn indicator_circle_crossed_by_the_view_is_not_a_boundary() {
    let mut ents = view("M", 0., 0., 100., 60., &[50.]);
    ents.push(line("G1", "01", [46., 46.], [54., 46.]));
    ents.push(line("G2", "01", [46., 54.], [54., 54.]));
    ents.push(circle("B", "02", [50., 50.], 8.));
    ents.push(text("U", "11", "Ⅴ", [49., 69.], 3.5));
    ents.push(text("SC", "11", "3:1", [47., 63.], 3.5));
    let g = run("", &ents);
    assert!(
        !g.expressions
            .iter()
            .any(|e| e.rule.starts_with("card:detail_view/boundary_")),
        "the circle crossed by the view is an indicator"
    );
}

#[test]
fn thin_circle_without_a_detail_name_is_not_a_boundary() {
    let mut ents = view("M", 0., 100., 60., 160., &[]);
    ents.extend(view("Z", 190., -10., 210., 10., &[]));
    ents.push(circle("B", "02", [200., 0.], 25.));
    let g = run("", &ents);
    assert!(
        !g.expressions
            .iter()
            .any(|e| e.rule.starts_with("card:detail_view/boundary_"))
    );
}
