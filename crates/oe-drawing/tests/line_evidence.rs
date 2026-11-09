//! Lines read from the drawing's own evidence (new-drawings correction round, lines workstream):
//! width by lineweight or plotting colour and by how a line joins the outline, joins across small
//! drafting gaps, section lines by their own spacing, holes named by a count × diameter callout,
//! thread conventions, outlines broken for a symbol. Fixtures are authored from card knowledge
//! (GB/T 4457.4, 4457.5, 4458.4, 4459.1, 14665); handles, coordinates, layer names other than the
//! GB/T 14665 table 6 identifiers, and labels are invented.
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
/// A line with an explicit colour (group 62).
fn line_c(id: &str, layer: &str, color: i32, a: [f64; 2], b: [f64; 2]) -> String {
    ent(
        "LINE",
        id,
        layer,
        &format!(
            "62\n{color}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
            a[0], a[1], b[0], b[1]
        ),
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
fn arc_c(id: &str, layer: &str, color: i32, c: [f64; 2], r: f64, a0: f64, a1: f64) -> String {
    ent(
        "ARC",
        id,
        layer,
        &format!(
            "62\n{color}\n10\n{}\n20\n{}\n40\n{r}\n50\n{a0}\n51\n{a1}\n",
            c[0], c[1]
        ),
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
/// GB/T 14665 table 6 layers (01 thick in colour 7; thin layers 02, 08 and 10 in colour 3; 09 in
/// colour 7 as some templates draw it), checked by their linetypes.
const GB: &[(&str, i32, &str, Option<i32>)] = &[
    ("0", 7, "CONTINUOUS", None),
    ("01", 7, "CONTINUOUS", None),
    ("02", 3, "CONTINUOUS", None),
    ("04", 2, "DASHX", None),
    ("05", 1, "AXISX", None),
    ("08", 3, "CONTINUOUS", None),
    ("09", 7, "CONTINUOUS", None),
    ("10", 3, "CONTINUOUS", None),
    ("11", 3, "CONTINUOUS", None),
];
/// Layers named outside any convention, with layer lineweights 0.50 / 0.25 (group 370).
const WEIGHTS: &[(&str, i32, &str, Option<i32>)] = &[
    ("0", 7, "CONTINUOUS", None),
    ("P", 7, "CONTINUOUS", Some(50)),
    ("Q", 7, "CONTINUOUS", Some(25)),
    ("R", 1, "AXISX", Some(25)),
];
/// The same layers with one lineweight only.
const ONE_WEIGHT: &[(&str, i32, &str, Option<i32>)] = &[
    ("0", 7, "CONTINUOUS", None),
    ("P", 7, "CONTINUOUS", Some(25)),
    ("Q", 7, "CONTINUOUS", Some(25)),
    ("R", 1, "AXISX", Some(25)),
];
fn tables(layers: &[(&str, i32, &str, Option<i32>)]) -> String {
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
    for (name, color, lt, w) in layers {
        s.push_str(&format!("0\nLAYER\n2\n{name}\n62\n{color}\n6\n{lt}\n"));
        if let Some(w) = w {
            s.push_str(&format!("370\n{w}\n"));
        }
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
fn run_full(
    header: &str,
    layers: &[(&str, i32, &str, Option<i32>)],
    blocks: &str,
    ents: &[String],
) -> MdEngineeringExpressionGraph {
    let head = if header.is_empty() {
        String::new()
    } else {
        format!("0\nSECTION\n2\nHEADER\n{header}0\nENDSEC\n")
    };
    let bytes = format!(
        "{head}{}{blocks}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        tables(layers),
        ents.concat()
    );
    match discover(bytes.as_bytes()) {
        Ok(g) => g,
        Err(e) => panic!("{e:#}"),
    }
}
fn run(ents: &[String]) -> MdEngineeringExpressionGraph {
    run_full("", GB, "", ents)
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
fn line_type(g: &MdEngineeringExpressionGraph, e: &MdEngineeringExpression, src: &str) -> String {
    g.members
        .iter()
        .find(|m| m.expression_id == e.id && m.source_handle == src)
        .map(|m| m.line_type.clone())
        .unwrap_or_default()
}
fn names(e: &MdEngineeringExpression, card: &str, variant: &str) -> bool {
    (e.rule.contains(&format!("card:{card}")) || e.evidence_ids.contains(&format!("card:{card}")))
        && (e.rule.ends_with(variant) || e.evidence_ids.contains(&format!("variant:{variant}")))
}
fn unresolved(g: &MdEngineeringExpressionGraph, src: &str) -> bool {
    kinds(g, src).contains("unresolved")
}

// ---------------------------------------------------------------- line_width_evidence

/// A free thick arc with nothing joined to it, and a centre line, on layers whose weights are
/// given in the layer table.
fn weighted(layers: &[(&str, i32, &str, Option<i32>)]) -> MdEngineeringExpressionGraph {
    run_full(
        "",
        layers,
        "",
        &[
            arc("WA", "P", [0., 0.], 12., 20., 160.),
            line("WX", "R", [-15., 0.], [15., 0.]),
        ],
    )
}

#[test]
fn thick_lineweight_arc_is_outline_without_junctions() {
    let g = weighted(WEIGHTS);
    let e =
        expr_with(&g, "WA", "visible_contour").unwrap_or_else(|| panic!("{:?}", kinds(&g, "WA")));
    assert!(
        names(e, "line_width_evidence", "lineweight_classes"),
        "{:?}",
        e.evidence_ids
    );
    assert_eq!(line_type(&g, e, "WA"), "thick_continuous");
}

#[test]
fn one_lineweight_class_does_not_decide() {
    let g = weighted(ONE_WEIGHT);
    assert!(
        !kinds(&g, "WA").contains("visible_contour"),
        "{:?}",
        kinds(&g, "WA")
    );
    assert!(unresolved(&g, "WA"));
}

#[test]
fn thin_lineweight_line_is_not_a_thick_outline() {
    // a step drawn thick, and a thin-weight line across it from corner to corner
    let g = run_full(
        "",
        WEIGHTS,
        "",
        &[
            line("SA", "P", [0., 0.], [30., 0.]),
            line("SB", "P", [30., 0.], [30., 15.]),
            line("SC", "P", [30., 15.], [0., 15.]),
            line("SD", "P", [0., 15.], [0., 0.]),
            line("ST", "Q", [10., 0.], [10., 15.]),
            line("SX", "R", [-5., 7.5], [35., 7.5]),
        ],
    );
    let e =
        expr_with(&g, "ST", "visible_contour").unwrap_or_else(|| panic!("{:?}", kinds(&g, "ST")));
    assert_eq!(line_type(&g, e, "ST"), "thin_continuous");
    let outline = expr_with(&g, "SA", "visible_contour").unwrap();
    assert_eq!(line_type(&g, outline, "SA"), "thick_continuous");
}

/// A rectangle drawn on the thick layer with its right side given by `side`.
fn rectangle(side: Vec<String>) -> MdEngineeringExpressionGraph {
    let mut ents = vec![
        line("RA", "01", [0., 0.], [40., 0.]),
        line("RT", "01", [0., 20.], [40., 20.]),
        line("RL", "01", [0., 0.], [0., 20.]),
        line("RX", "05", [-5., 10.], [45., 10.]),
    ];
    ents.extend(side);
    run(&ents)
}

#[test]
fn outline_colour_line_on_a_thin_layer_joined_at_both_ends_is_outline() {
    let g = rectangle(vec![line("RR", "09", [40., 0.], [40., 20.])]);
    let e =
        expr_with(&g, "RR", "visible_contour").unwrap_or_else(|| panic!("{:?}", kinds(&g, "RR")));
    assert!(
        names(e, "line_width_evidence", "outline_colour_on_thin_layer"),
        "{:?}",
        e.evidence_ids
    );
    assert_eq!(line_type(&g, e, "RR"), "thick_continuous");
}

#[test]
fn outline_colour_line_on_a_thin_layer_with_a_free_end_is_not_outline() {
    // from the middle of the top edge up to nothing: a leader or reference line, not outline
    let g = rectangle(vec![
        line("RR", "01", [40., 0.], [40., 20.]),
        line("RF", "09", [20., 20.], [26., 32.]),
    ]);
    assert!(
        !kinds(&g, "RF").contains("visible_contour"),
        "{:?}",
        kinds(&g, "RF")
    );
}

#[test]
fn thin_colour_outline_joined_at_both_ends_is_outline() {
    // a slot: two thick edges closed by an arc drawn on the thick layer in the thin colour
    let g = run(&[
        line("LA", "01", [0., 0.], [30., 0.]),
        line("LB", "01", [0., 10.], [30., 10.]),
        line("LC", "01", [0., 0.], [0., 10.]),
        arc_c("LR", "01", 3, [30., 5.], 5., 270., 90.),
        line("LX", "05", [-5., 5.], [40., 5.]),
    ]);
    let e =
        expr_with(&g, "LR", "visible_contour").unwrap_or_else(|| panic!("{:?}", kinds(&g, "LR")));
    assert!(
        names(e, "line_width_evidence", "outline_in_thin_colour"),
        "{:?}",
        e.evidence_ids
    );
    assert_eq!(line_type(&g, e, "LR"), "thick_continuous");
}

#[test]
fn thin_colour_extension_line_stays_annotation() {
    // the thin-coloured line on the thick layer runs from the outline corner past the
    // dimension line of a dimension drawn with loose terminators: an extension line
    let g = run(&[
        line("OA", "01", [0., 0.], [30., 0.]),
        line("OB", "01", [30., 0.], [30., 20.]),
        line("OC", "01", [0., 0.], [0., 20.]),
        line_c("EA", "01", 3, [0., 20.], [0., 34.]),
        line_c("EB", "01", 3, [30., 20.], [30., 34.]),
        solid("T1", "08", [0., 31.], [3., 31.]),
        solid("T2", "08", [30., 31.], [27., 31.]),
        line("DL", "08", [3., 31.], [27., 31.]),
        text("DV", "11", "30", [12., 32.], 3.5),
    ]);
    assert!(
        !kinds(&g, "EA").contains("visible_contour"),
        "{:?}",
        kinds(&g, "EA")
    );
    assert!(
        !kinds(&g, "EB").contains("visible_contour"),
        "{:?}",
        kinds(&g, "EB")
    );
}

#[test]
fn thin_layer_line_between_outline_ends_is_a_thin_structural_line() {
    let g = rectangle(vec![
        line("RR", "01", [40., 0.], [40., 20.]),
        line("RM", "10", [25., 0.], [25., 20.]),
    ]);
    let e =
        expr_with(&g, "RM", "visible_contour").unwrap_or_else(|| panic!("{:?}", kinds(&g, "RM")));
    assert!(
        names(e, "line_width_evidence", "thin_line_joined_at_both_ends"),
        "{:?}",
        e.evidence_ids
    );
    assert_eq!(line_type(&g, e, "RM"), "thin_continuous");
}

// ---------------------------------------------------------------- outline_joins

/// Two half circles of one round outline drawn end to end with a gap of `gap` at both joins.
fn halves(gap: f64) -> MdEngineeringExpressionGraph {
    let r: f64 = 20.;
    let d = (gap / r).to_degrees();
    run_full(
        "",
        GB,
        "",
        &[
            arc("HA", "0", [0., 0.], r, 0., 180.),
            arc("HB", "0", [0., 0.], r, 180. + d, 360. - d),
        ],
    )
}

#[test]
fn arcs_meeting_within_the_drafting_gap_are_joined() {
    let g = halves(0.05);
    for h in ["HA", "HB"] {
        let e = expr_with(&g, h, "visible_contour")
            .unwrap_or_else(|| panic!("{h}: {:?}", kinds(&g, h)));
        assert!(
            names(e, "outline_joins", "arc_to_arc_gap"),
            "{:?}",
            e.evidence_ids
        );
    }
}

#[test]
fn arcs_apart_beyond_the_drafting_gap_are_not_joined() {
    let g = halves(1.0);
    assert!(
        !kinds(&g, "HB").contains("visible_contour"),
        "{:?}",
        kinds(&g, "HB")
    );
}

#[test]
fn outline_arc_ending_on_a_thread_circle_is_joined() {
    // a tapped hole's end view (thin three-quarter arc about a full circle) and an outline line
    // stopping on the thin arc, its other end a corner
    let g = run(&[
        arc("TA", "02", [0., 0.], 3., 90., 0.),
        circle("TC", "0", [0., 0.], 2.55),
        line("TL", "0", [3., 0.], [20., 0.]),
        line("TV", "0", [20., 0.], [20., 15.]),
        line("TW", "0", [20., 15.], [3., 15.]),
    ]);
    let e =
        expr_with(&g, "TL", "visible_contour").unwrap_or_else(|| panic!("{:?}", kinds(&g, "TL")));
    assert!(
        names(e, "outline_joins", "end_on_thread_circle"),
        "{:?}",
        e.evidence_ids
    );
}

// ---------------------------------------------------------------- section_line_region

/// A square outline with thin 45° lines inside at offsets `offs` (perpendicular distance between
/// neighbours is the difference of the offsets times 1/√2).
fn square_with_lines(offs: &[f64]) -> MdEngineeringExpressionGraph {
    let mut ents = vec![
        line("QA", "01", [0., 0.], [60., 0.]),
        line("QB", "01", [60., 0.], [60., 60.]),
        line("QC", "01", [60., 60.], [0., 60.]),
        line("QD", "01", [0., 60.], [0., 0.]),
    ];
    for (i, o) in offs.iter().enumerate() {
        // from the bottom edge (o, 0) up and to the right, stopping at the right edge
        let top = (60. - o).min(60.);
        ents.push(line(&format!("H{i}"), "02", [*o, 0.], [o + top, top]));
    }
    run(&ents)
}

#[test]
fn section_lines_with_enlarged_spacing_are_one_hatch() {
    let g = square_with_lines(&[5., 19.142, 33.284, 47.426]);
    for h in ["H0", "H1", "H2", "H3"] {
        let e =
            expr_with(&g, h, "section_hatch").unwrap_or_else(|| panic!("{h}: {:?}", kinds(&g, h)));
        assert!(
            names(e, "section_line_region", "loose_lines_by_own_spacing"),
            "{:?}",
            e.evidence_ids
        );
    }
}

#[test]
fn parallel_lines_with_uneven_spacing_are_not_section_lines() {
    let g = square_with_lines(&[5., 19.142, 43.284, 47.426]);
    assert!(
        !kinds(&g, "H1").contains("section_hatch"),
        "{:?}",
        kinds(&g, "H1")
    );
}

/// A square region hatched at 45° with its own outline; `extra` adds lines.
fn hatched_square(extra: Vec<String>) -> MdEngineeringExpressionGraph {
    let mut ents = vec![
        line("QA", "01", [0., 0.], [40., 0.]),
        line("QB", "01", [40., 0.], [40., 40.]),
        line("QC", "01", [40., 40.], [0., 40.]),
        line("QD", "01", [0., 40.], [0., 0.]),
        hatch_edges(
            "QH",
            "10",
            &[[0., 0.], [40., 0.], [40., 40.], [0., 40.]],
            45.,
        ),
    ];
    ents.extend(extra);
    run(&ents)
}

#[test]
fn short_stroke_inside_a_hatch_along_its_pattern_joins_it() {
    let g = hatched_square(vec![line("QS", "02", [20., 20.], [20.8, 20.8])]);
    let e = expr_with(&g, "QS", "section_hatch").unwrap_or_else(|| panic!("{:?}", kinds(&g, "QS")));
    assert!(
        names(e, "section_line_region", "remnant_stroke"),
        "{:?}",
        e.evidence_ids
    );
    assert!(role(&g, e, "QH").contains("hatch_fill"));
}

#[test]
fn corner_line_inside_a_hatch_is_a_chamfer_edge() {
    // the outline is chamfered at the top right corner; the hatch boundary was taken square
    let g = run(&[
        line("CA", "01", [0., 0.], [40., 0.]),
        line("CB", "01", [40., 0.], [40., 37.]),
        line("CK", "01", [40., 37.], [37., 40.]),
        line("CC", "01", [37., 40.], [0., 40.]),
        line("CD", "01", [0., 40.], [0., 0.]),
        hatch_edges(
            "CH",
            "10",
            &[[0., 0.], [40., 0.], [40., 40.], [0., 40.]],
            135.,
        ),
    ]);
    assert!(
        !kinds(&g, "CK").contains("section_hatch"),
        "{:?}",
        kinds(&g, "CK")
    );
    assert!(kinds(&g, "CK").contains("visible_contour"));
}

// ---------------------------------------------------------------- dimension_line_parts

/// Native linear DIMENSION from (x, y) to (x + 30, y), its own block drawing the extension lines
/// from y to y + 12; a root line along the left extension line on `layer`.
fn dimension_along(layer: &str) -> MdEngineeringExpressionGraph {
    let (x, y) = (100., 50.);
    let children = [
        line("D1", "0", [x, y + 10.], [x + 30., y + 10.]),
        line("D2", "0", [x, y], [x, y + 12.]),
        line("D3", "0", [x + 30., y], [x + 30., y + 12.]),
        solid("D4", "0", [x, y + 10.], [x + 2., y + 10.]),
        solid("D5", "0", [x + 30., y + 10.], [x + 28., y + 10.]),
    ];
    let blk = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*D9\n10\n0\n20\n0\n{}0\nENDBLK\n0\nENDSEC\n",
        children.concat()
    );
    let dim = ent(
        "DIMENSION",
        "DM",
        "08",
        &format!(
            "2\n*D9\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n70\n160\n1\n<>\n13\n{x}\n23\n{y}\n14\n{}\n24\n{y}\n42\n30\n50\n0\n",
            x + 30.,
            y + 10.,
            x + 15.,
            y + 11.,
            x + 30.
        ),
    );
    run_full(
        "",
        GB,
        &blk,
        &[
            dim,
            line("AL", layer, [x, y - 30.], [x, y + 12.]),
            line("BX", "01", [x - 10., y], [x + 40., y]),
        ],
    )
}

#[test]
fn centre_line_used_as_extension_line_stays_a_centre_line() {
    let g = dimension_along("05");
    let e = expr_with(&g, "AL", "center_axis").unwrap_or_else(|| panic!("{:?}", kinds(&g, "AL")));
    assert!(
        names(e, "dimension_line_parts", "axis_as_extension_line"),
        "{:?}",
        e.evidence_ids
    );
    assert!(kinds(&g, "AL").iter().any(|k| k.starts_with("dimension_")));
    assert!(!unresolved(&g, "AL"));
}

#[test]
fn continuous_extension_line_on_an_axis_is_not_a_centre_line() {
    let g = dimension_along("02");
    assert!(
        !kinds(&g, "AL").contains("center_axis"),
        "{:?}",
        kinds(&g, "AL")
    );
}

// ---------------------------------------------------------------- hole_pattern

/// Four equal circles of radius `r` evenly spaced on a pitch circle of radius 50 about `c`.
fn four_holes(prefix: &str, c: [f64; 2], r: f64) -> Vec<String> {
    (0..4)
        .map(|i| {
            let a = (45. + 90. * i as f64).to_radians();
            circle(
                &format!("{prefix}{i}"),
                "09",
                [c[0] + 50. * a.cos(), c[1] + 50. * a.sin()],
                r,
            )
        })
        .collect()
}
/// A count × diameter callout across circle `c` of radius `r`: terminators pointing outward at the
/// circle, the dimension line between them, a leader on to the value (GB/T 4458.4).
fn callout(c: [f64; 2], r: f64, value: &str) -> Vec<String> {
    let (y, x0, x1) = (c[1], c[0] - r, c[0] + r);
    vec![
        solid("K1", "08", [x0, y], [x0 + 2., y]),
        solid("K2", "08", [x1, y], [x1 - 2., y]),
        line("KL", "08", [x0 + 2., y], [x1 - 2., y]),
        line("KD", "08", [x1, y], [x1 + 12., y]),
        line("KS", "08", [x1 + 12., y], [x1 + 30., y]),
        text("KV", "11", value, [x1 + 13., y + 1.2], 3.5),
    ]
}

#[test]
fn circles_named_by_a_count_diameter_callout_are_holes() {
    let first = [
        50. * 45f64.to_radians().cos(),
        50. * 45f64.to_radians().sin(),
    ];
    let mut ents = four_holes("A", [0., 0.], 4.5);
    ents.extend(four_holes("B", [300., 0.], 4.5));
    ents.extend(callout(first, 4.5, "4-%%c9"));
    let g = run(&ents);
    for h in ["A0", "A1", "A2", "A3"] {
        let e = expr_with(&g, h, "visible_contour")
            .unwrap_or_else(|| panic!("{h}: {:?}", kinds(&g, h)));
        assert!(
            names(e, "hole_pattern", "count_diameter_callout"),
            "{:?}",
            e.evidence_ids
        );
    }
    for h in ["B0", "B1", "B2", "B3"] {
        let e = expr_with(&g, h, "visible_contour")
            .unwrap_or_else(|| panic!("{h}: {:?}", kinds(&g, h)));
        assert!(
            names(e, "hole_pattern", "same_pattern_in_another_view"),
            "{:?}",
            e.evidence_ids
        );
    }
}

#[test]
fn circle_of_another_diameter_is_not_a_named_hole() {
    let first = [
        50. * 45f64.to_radians().cos(),
        50. * 45f64.to_radians().sin(),
    ];
    let mut ents = four_holes("A", [0., 0.], 4.5);
    ents.extend(four_holes("B", [300., 0.], 3.5));
    ents.extend(callout(first, 4.5, "4-%%c9"));
    let g = run(&ents);
    assert!(
        !kinds(&g, "B0").contains("visible_contour"),
        "{:?}",
        kinds(&g, "B0")
    );
}

// ---------------------------------------------------------------- thread_representation

#[test]
fn thread_end_view_drawn_with_a_small_minor_circle() {
    let g = run(&[
        arc("EA", "02", [0., 0.], 6., 90., 0.),
        circle("EC", "0", [0., 0.], 3.9),
    ]);
    let e =
        expr_with(&g, "EA", "thread_convention").unwrap_or_else(|| panic!("{:?}", kinds(&g, "EA")));
    assert!(
        names(e, "thread_representation", "end_view_wide_ratio"),
        "{:?}",
        e.evidence_ids
    );
    let c = expr_with(&g, "EC", "thread_convention").unwrap();
    assert_eq!(line_type(&g, c, "EC"), "thick_continuous");
}

#[test]
fn end_view_with_a_plain_diameter_across_it_is_not_a_thread() {
    let mut ents = vec![
        arc("EA", "02", [0., 0.], 6., 90., 0.),
        circle("EC", "0", [0., 0.], 3.9),
    ];
    ents.extend(callout([0., 0.], 6., "%%c12"));
    let g = run(&ents);
    assert!(
        !kinds(&g, "EA").contains("thread_convention"),
        "{:?}",
        kinds(&g, "EA")
    );
}

/// A tapped hole in section along x from the face at x = 0: thin pair at ±`major`/2 to x =
/// `thread`, thick pair at ±`minor`/2 to x = `drill`; the axis on y = 0.
fn tapped_section(thin_layer: &str, extra: Vec<String>) -> Vec<String> {
    let mut v = vec![
        line("FA", "01", [0., -20.], [0., 20.]),
        line("FB", "01", [16., -20.], [16., 20.]),
        line("M1", thin_layer, [0., 5.], [16., 5.]),
        line("M2", thin_layer, [0., -5.], [16., -5.]),
        line("N1", "01", [0., 4.2], [16., 4.2]),
        line("N2", "01", [0., -4.2], [16., -4.2]),
        line("AX", "05", [-3., 0.], [19., 0.]),
    ];
    v.extend(extra);
    v
}

#[test]
fn internal_thread_in_section_is_a_thread() {
    let g = run(&tapped_section("02", vec![]));
    let e =
        expr_with(&g, "M1", "thread_convention").unwrap_or_else(|| panic!("{:?}", kinds(&g, "M1")));
    assert!(
        names(e, "thread_representation", "internal_thread_in_section"),
        "{:?}",
        e.evidence_ids
    );
    assert!(
        role(&g, e, "M2").contains("thread_line") && role(&g, e, "N1").contains("contour_segment")
    );
    assert!(!kinds(&g, "M1").contains("visible_contour"));
}

#[test]
fn two_thick_pairs_are_a_stepped_hole_not_a_thread() {
    let g = run(&tapped_section("01", vec![]));
    assert!(
        !kinds(&g, "M1").contains("thread_convention"),
        "{:?}",
        kinds(&g, "M1")
    );
}

#[test]
fn blind_tapped_hole_in_section_with_drill_point() {
    // thread to x = 10, ended by a short dash-dot line that prints solid (shorter than its first
    // dash at the drawing's linetype scale); drill to x = 14 with a 120° point
    let header = "9\n$LTSCALE\n40\n2.0\n";
    let ents = vec![
        line("FA", "01", [0., -20.], [0., 20.]),
        line("M1", "02", [0., 4.], [10., 4.]),
        line("M2", "02", [0., -4.], [10., -4.]),
        line("N1", "01", [0., 3.], [14., 3.]),
        line("N2", "01", [0., -3.], [14., -3.]),
        line("TE", "05", [10., -4.], [10., 4.]),
        line("DB", "01", [14., -3.], [14., 3.]),
        line("P1", "01", [14., 3.], [15.732050807568877, 0.]),
        line("P2", "01", [14., -3.], [15.732050807568877, 0.]),
        line("AX", "05", [-3., 0.], [18., 0.]),
    ];
    let g = run_full(header, GB, "", &ents);
    let e =
        expr_with(&g, "M1", "thread_convention").unwrap_or_else(|| panic!("{:?}", kinds(&g, "M1")));
    assert!(
        names(e, "thread_representation", "blind_tapped_hole_in_section"),
        "{:?}",
        e.evidence_ids
    );
    for h in ["TE", "DB", "P1", "P2"] {
        assert!(
            role(&g, e, h).contains("contour_segment"),
            "{h}: {:?}",
            role(&g, e, h)
        );
    }
    assert!(
        e.evidence_ids
            .iter()
            .any(|x| x == "thread_end_line_printed_solid:TE"),
        "{:?}",
        e.evidence_ids
    );
    assert!(!kinds(&g, "TE").contains("center_axis"));
}

// ---------------------------------------------------------------- annotation_break

/// Two collinear outline pieces with a gap from x = 20 to 30, each turning down at its outer
/// end; `symbol` inserts a texture symbol whose arms cross the gap.
fn broken_edge(symbol: bool) -> MdEngineeringExpressionGraph {
    let sym = vec![
        line("s", "0", [0., 0.], [-2.5, 4.33]),
        line("l", "0", [0., 0.], [5., 8.66]),
        text("t", "0", "3.2", [-1., 6.], 2.5),
    ];
    let mut ents = vec![
        line("GA", "0", [0., 0.], [20., 0.]),
        line("GB", "0", [30., 0.], [50., 0.]),
        line("GC", "0", [0., 0.], [0., -15.]),
        line("GD", "0", [50., 0.], [50., -15.]),
        line("GE", "0", [0., -15.], [50., -15.]),
    ];
    if symbol {
        ents.push(insert("GS", "11", "TSYM", [24., -3.]));
    }
    run_full("", GB, &blocks(&[("TSYM", sym)]), &ents)
}

#[test]
fn outline_broken_for_a_texture_symbol_is_one_line() {
    let g = broken_edge(true);
    assert!(
        g.implied_spans
            .iter()
            .any(|s| s.cause == "text_break" && s.evidence_ids.iter().any(|x| x == "symbol:GS")),
        "{:?}",
        g.structure_ends
    );
}

#[test]
fn gap_without_annotation_is_not_bridged() {
    let g = broken_edge(false);
    assert!(g.implied_spans.is_empty(), "{:?}", g.implied_spans);
}

// ---------------------------------------------------------------- dimension_line_parts: pieces and copies

/// The native dimension of `dimension_along` with its left extension line drawn as a root line
/// from the outline at y = 50, and a second piece below it on the same carrier ending `gap`
/// short of it, starting on an outline at y = 30.
fn split_extension(gap: f64) -> MdEngineeringExpressionGraph {
    let (x, y) = (100., 50.);
    let children = [
        line("D1", "0", [x, y + 10.], [x + 30., y + 10.]),
        line("D2", "0", [x, y], [x, y + 12.]),
        line("D3", "0", [x + 30., y], [x + 30., y + 12.]),
        solid("D4", "0", [x, y + 10.], [x + 2., y + 10.]),
        solid("D5", "0", [x + 30., y + 10.], [x + 28., y + 10.]),
    ];
    let blk = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*D8\n10\n0\n20\n0\n{}0\nENDBLK\n0\nENDSEC\n",
        children.concat()
    );
    let dim = ent(
        "DIMENSION",
        "DN",
        "08",
        &format!(
            "2\n*D8\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n70\n160\n1\n<>\n13\n{x}\n23\n{y}\n14\n{}\n24\n{y}\n42\n30\n50\n0\n",
            x + 30.,
            y + 10.,
            x + 15.,
            y + 11.,
            x + 30.
        ),
    );
    run_full(
        "",
        GB,
        &blk,
        &[
            dim,
            line("E1", "02", [x, y], [x, y + 12.]),
            line("E2", "02", [x, 30.], [x, y - gap]),
            line("BX", "01", [x - 10., 30.], [x + 40., 30.]),
        ],
    )
}

#[test]
fn extension_line_split_for_a_symbol_is_one_extension_line() {
    let g = split_extension(7.);
    let e = exprs_of(&g, "E1")
        .into_iter()
        .find(|e| e.kind.starts_with("dimension_"))
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "E1")));
    assert!(
        role(&g, e, "E2").contains("extension_line"),
        "{:?}",
        kinds(&g, "E2")
    );
    assert!(
        names(e, "dimension_line_parts", "split_extension_line"),
        "{:?}",
        e.evidence_ids
    );
}

#[test]
fn collinear_line_far_from_an_extension_line_is_not_absorbed() {
    let g = split_extension(16.);
    assert!(
        !kinds(&g, "E2").iter().any(|k| k.starts_with("dimension_")),
        "{:?}",
        kinds(&g, "E2")
    );
}

#[test]
fn dimension_parts_drawn_twice_join_the_dimension() {
    // a hole diameter drawn with loose parts; its terminator and dimension line drawn again
    let ents = vec![
        solid("A1", "08", [0., 0.], [3., 0.]),
        solid("A2", "08", [40., 0.], [37., 0.]),
        line("DL", "08", [3., 0.], [37., 0.]),
        line("LD", "08", [40., 0.], [50., 0.]),
        line("SH", "08", [50., 0.], [64., 0.]),
        text("VT", "11", "%%c47", [52., 1.2], 3.5),
        solid("A3", "08", [0., 0.01], [3., 0.01]),
        line("DR", "08", [5., 0.], [30., 0.]),
    ];
    let g = run(&ents);
    let e = expr_with(&g, "VT", "dimension_diameter")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "VT")));
    assert!(
        role(&g, e, "A3").contains("arrowhead"),
        "{:?}",
        kinds(&g, "A3")
    );
    assert!(
        role(&g, e, "DR").contains("dimension_line"),
        "{:?}",
        kinds(&g, "DR")
    );
    assert!(
        names(e, "dimension_line_parts", "drawn_twice"),
        "{:?}",
        e.evidence_ids
    );
}

// ---------------------------------------------------------------- local_section_boundary: a chain of lines

/// A block outline with a U-shaped chain of five straight thin lines hanging from its top edge
/// (each end just past the edge); `hatched` adds the section hatch inside the U.
fn chain_section(hatched: bool) -> MdEngineeringExpressionGraph {
    let pts = [
        [10., 40.4],
        [14., 30.],
        [20., 24.],
        [30., 22.],
        [40., 26.],
        [46., 40.4],
    ];
    let mut ents = vec![
        line("BT", "01", [0., 40.], [60., 40.]),
        line("BR", "01", [60., 40.], [60., 0.]),
        line("BB", "01", [60., 0.], [0., 0.]),
        line("BL", "01", [0., 0.], [0., 40.]),
    ];
    for (i, w) in pts.windows(2).enumerate() {
        ents.push(line(&format!("U{i}"), "02", w[0], w[1]));
    }
    if hatched {
        ents.push(hatch_edges(
            "UH",
            "10",
            &[
                [14., 40.],
                [16., 29.],
                [22., 26.],
                [30., 25.],
                [38., 28.],
                [43., 40.],
            ],
            45.,
        ));
    }
    run(&ents)
}

#[test]
fn chain_of_short_lines_around_a_section_is_a_local_section_boundary() {
    let g = chain_section(true);
    for h in ["U0", "U1", "U2", "U3", "U4"] {
        let e = expr_with(&g, h, "break_line").unwrap_or_else(|| panic!("{h}: {:?}", kinds(&g, h)));
        assert!(
            names(e, "local_section_boundary", "line_chain"),
            "{:?}",
            e.evidence_ids
        );
    }
}

#[test]
fn chain_of_lines_without_a_section_is_not_a_boundary() {
    let g = chain_section(false);
    assert!(
        !kinds(&g, "U2").contains("break_line"),
        "{:?}",
        kinds(&g, "U2")
    );
}
