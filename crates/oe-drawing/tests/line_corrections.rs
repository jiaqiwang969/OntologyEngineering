//! Lines after the expert's review of the four new drawings (correction round, lines workstream):
//! content that never plots, BYBLOCK as no colour of its own, the width class a line is plotted
//! with, extension lines against the outline they start from, terminators and value fragments of
//! native dimensions. Fixtures are authored from card knowledge (GB/T 4457.4, 4458.4, 14665,
//! 18229); handles, coordinates, layer names other than the GB/T 14665 table 6 identifiers and the
//! CAD definition-point layer, and labels are invented.
use oe_contracts::{MdEngineeringExpression, MdEngineeringExpressionGraph, MdExpressionMember};
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
/// A line with further entity tags (colour 62, linetype 6) written as DXF pairs.
fn line_with(id: &str, layer: &str, more: &str, a: [f64; 2], b: [f64; 2]) -> String {
    ent(
        "LINE",
        id,
        layer,
        &format!(
            "{more}10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
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
fn point(id: &str, layer: &str, p: [f64; 2]) -> String {
    ent("POINT", id, layer, &format!("10\n{}\n20\n{}\n", p[0], p[1]))
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
/// A layer: name, colour, linetype, and further LAYER tags (lineweight 370, plot switch 290,
/// flags 70) written as DXF pairs.
type Layer<'a> = (&'a str, i32, &'a str, &'a str);
/// GB/T 14665 table 6 layers: 01 thick in colour 7, thin layers in colour 3, 09 in colour 7 as
/// some templates draw it, dashed 04 and dash-dot 05.
const GB: &[Layer] = &[
    ("0", 7, "CONTINUOUS", ""),
    ("01", 7, "CONTINUOUS", ""),
    ("02", 3, "CONTINUOUS", ""),
    ("04", 2, "DASHX", ""),
    ("05", 1, "AXISX", ""),
    ("08", 3, "CONTINUOUS", ""),
    ("09", 7, "CONTINUOUS", ""),
    ("10", 3, "CONTINUOUS", ""),
    ("11", 3, "CONTINUOUS", ""),
    ("12", 5, "CONTINUOUS", ""),
];
fn with<'a>(more: &[Layer<'a>]) -> Vec<Layer<'a>> {
    GB.iter().chain(more).copied().collect()
}
fn tables(layers: &[Layer]) -> String {
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
    for (name, color, lt, more) in layers {
        s.push_str(&format!(
            "0\nLAYER\n2\n{name}\n62\n{color}\n6\n{lt}\n{more}"
        ));
    }
    s.push_str("0\nENDSEC\n");
    s
}
fn run_full(layers: &[Layer], blocks: &str, ents: &[String]) -> MdEngineeringExpressionGraph {
    let bytes = format!(
        "{}{blocks}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        tables(layers),
        ents.concat()
    );
    match discover(bytes.as_bytes()) {
        Ok(g) => g,
        Err(e) => panic!("{e:#}"),
    }
}
fn run(ents: &[String]) -> MdEngineeringExpressionGraph {
    run_full(GB, "", ents)
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
fn members<'a>(g: &'a MdEngineeringExpressionGraph, src: &str) -> Vec<&'a MdExpressionMember> {
    g.members
        .iter()
        .filter(|m| m.source_handle == src)
        .collect()
}
fn expr_with<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
    kind: &str,
) -> Option<&'a MdEngineeringExpression> {
    exprs_of(g, src).into_iter().find(|e| e.kind == kind)
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
fn marked(m: &MdExpressionMember) -> bool {
    m.evidence_ids.iter().any(|e| e == "not_plotted")
}
/// The source is read as construction that is not on the sheet: its only kind, every member
/// marked, the expression qualified, and no structural line made of it.
fn is_unplotted_construction(g: &MdEngineeringExpressionGraph, src: &str, variant: &str) {
    assert_eq!(
        kinds(g, src),
        BTreeSet::from(["construction_reference".to_string()]),
        "{src}"
    );
    let ms = members(g, src);
    assert!(!ms.is_empty() && ms.iter().all(|m| marked(m)), "{src}");
    for e in exprs_of(g, src) {
        assert!(e.qualifiers.iter().any(|q| q == "not_plotted"), "{src}");
        assert!(
            e.evidence_ids.contains(&"card:not_plotted_content".to_string())
                && e.evidence_ids.contains(&format!("variant:{variant}")),
            "{:?}",
            e.evidence_ids
        );
        assert!(e.concept_ids.contains(&"DC-G1-101".to_string()), "{src}");
    }
    let ids: BTreeSet<String> = ms.iter().map(|m| format!("member:{}", m.id)).collect();
    assert!(
        !g.structural_lines
            .iter()
            .any(|l| l.pieces.iter().chain(&l.coincident).any(|p| ids.contains(p))),
        "{src} is a piece of a structural line"
    );
}

// ---------------------------------------------------------------- not_plotted_content

/// A rectangle 40 × 20 on the thick layer whose right side lies on `layer`, with its axis.
fn rectangle_with_side(layers: &[Layer], layer: &str) -> MdEngineeringExpressionGraph {
    run_full(
        layers,
        "",
        &[
            line("NA", "01", [0., 0.], [40., 0.]),
            line("NT", "01", [0., 20.], [40., 20.]),
            line("NL", "01", [0., 0.], [0., 20.]),
            line("NR", layer, [40., 0.], [40., 20.]),
            line("NX", "05", [-5., 10.], [45., 10.]),
        ],
    )
}

#[test]
fn outline_drawn_on_the_definition_point_layer_is_construction() {
    // the layer name is matched in any letter case
    for name in ["Defpoints", "DEFPOINTS"] {
        let g = rectangle_with_side(&with(&[(name, 7, "CONTINUOUS", "")]), name);
        is_unplotted_construction(&g, "NR", "definition_point_layer");
        // the printed sides stay outline, with a free end where only the unprinted side met them
        assert!(kinds(&g, "NA").contains("visible_contour"));
        assert!(members(&g, "NA").iter().all(|m| !marked(m)));
    }
}

#[test]
fn definition_point_is_marked_not_plotted() {
    let g = run_full(
        &with(&[("Defpoints", 7, "CONTINUOUS", "")]),
        "",
        &[
            line("PA", "01", [0., 0.], [40., 0.]),
            line("PB", "01", [40., 0.], [40., 20.]),
            point("PP", "Defpoints", [40., 30.]),
        ],
    );
    let ms = members(&g, "PP");
    assert!(
        ms.iter()
            .all(|m| m.member_role == "definition_point" && marked(m)),
        "{ms:?}"
    );
    assert!(
        exprs_of(&g, "PP")
            .iter()
            .all(|e| e.kind == "construction_reference"
                && e.qualifiers.iter().any(|q| q == "not_plotted"))
    );
}

#[test]
fn line_on_a_layer_with_the_plot_switch_off_is_construction() {
    let g = rectangle_with_side(&with(&[("AUX", 7, "CONTINUOUS", "290\n0\n")]), "AUX");
    is_unplotted_construction(&g, "NR", "plot_switch_off");
}

#[test]
fn line_on_a_switched_off_layer_is_construction() {
    // a negative colour number is a layer that is switched off
    let g = rectangle_with_side(&with(&[("AUX", -7, "CONTINUOUS", "")]), "AUX");
    is_unplotted_construction(&g, "NR", "layer_off_or_frozen");
}

#[test]
fn hidden_line_on_a_frozen_layer_is_no_hidden_outline() {
    let g = rectangle_with_side(&with(&[("AUX", 2, "DASHX", "70\n1\n")]), "AUX");
    is_unplotted_construction(&g, "NR", "layer_off_or_frozen");
    // the same dashed line on a layer that plots is hidden outline
    let g = rectangle_with_side(&with(&[("AUX", 2, "DASHX", "70\n0\n")]), "AUX");
    assert_eq!(
        kinds(&g, "NR"),
        BTreeSet::from(["hidden_contour".to_string()])
    );
    assert!(members(&g, "NR").iter().all(|m| !marked(m)));
}

#[test]
fn layer_whose_name_only_contains_the_definition_point_name_plots() {
    let g = rectangle_with_side(
        &with(&[("DEFPOINTS-B", 7, "CONTINUOUS", "")]),
        "DEFPOINTS-B",
    );
    assert!(kinds(&g, "NR").contains("visible_contour"), "{:?}", kinds(&g, "NR"));
    assert!(members(&g, "NR").iter().all(|m| !marked(m)));
}

#[test]
fn dimension_with_a_terminator_on_a_never_plotted_layer_stays_a_dimension() {
    // a width drawn with loose parts; one terminator was left on the definition-point layer
    let g = run_full(
        &with(&[("Defpoints", 7, "CONTINUOUS", "")]),
        "",
        &[
            line("OA", "01", [0., 0.], [30., 0.]),
            line("OB", "01", [30., 0.], [30., 20.]),
            line("OC", "01", [0., 0.], [0., 20.]),
            line("EA", "02", [0., 20.], [0., 34.]),
            line("EB", "02", [30., 20.], [30., 34.]),
            solid("T1", "08", [0., 31.], [3., 31.]),
            solid("T2", "Defpoints", [30., 31.], [27., 31.]),
            line("DL", "08", [3., 31.], [27., 31.]),
            text("DV", "11", "30", [12., 32.], 3.5),
        ],
    );
    let e = exprs_of(&g, "DV")
        .into_iter()
        .find(|e| e.kind == "dimension_linear")
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "DV")));
    let head = members(&g, "T2")
        .into_iter()
        .find(|m| m.expression_id == e.id)
        .unwrap_or_else(|| panic!("{:?}", kinds(&g, "T2")));
    assert!(head.member_role == "arrowhead" && marked(head));
    assert!(!e.qualifiers.iter().any(|q| q == "not_plotted"));
    assert!(
        e.unknowns
            .iter()
            .any(|u| u.contains("arrowhead T2") && u.contains("never plots")),
        "{:?}",
        e.unknowns
    );
    assert!(members(&g, "T1").iter().all(|m| !marked(m)));
}

/// An open box on the thick layer (bottom, left) closed at the top by a line on `top`; a line in
/// the outline colour on a thin layer stands on the middle of the bottom and ends on the top.
fn stand_under(top: &str) -> MdEngineeringExpressionGraph {
    run_full(
        &with(&[("Defpoints", 7, "CONTINUOUS", "")]),
        "",
        &[
            line("UA", "01", [0., 0.], [40., 0.]),
            line("UL", "01", [0., 0.], [0., 20.]),
            line("UT", top, [0., 20.], [40., 20.]),
            line("UR", "01", [40., 0.], [40., 20.]),
            line("UM", "09", [20., 0.], [20., 20.]),
            line("UX", "05", [-5., 10.], [45., 10.]),
        ],
    )
}

#[test]
fn outline_joined_only_through_a_never_plotted_line_is_not_admitted() {
    // joined to the printed outline at both ends: outline
    let g = stand_under("01");
    assert!(kinds(&g, "UM").contains("visible_contour"), "{:?}", kinds(&g, "UM"));
    // its upper end meets only a line that is not on the sheet: one junction is not enough
    let g = stand_under("Defpoints");
    assert!(
        !kinds(&g, "UM").contains("visible_contour"),
        "{:?}",
        kinds(&g, "UM")
    );
    is_unplotted_construction(&g, "UT", "definition_point_layer");
}

// ---------------------------------------------------------------- line_width_evidence: BYBLOCK is not a colour

#[test]
fn byblock_colour_line_joined_at_both_ends_is_outline() {
    // the right side of the box carries the colour BYBLOCK (group 62 = 0) outside any block; a
    // step stands on it
    let g = run(&[
        line("BA", "01", [0., 0.], [40., 0.]),
        line("BT", "01", [0., 20.], [40., 20.]),
        line("BL", "01", [0., 0.], [0., 20.]),
        line_with("BR", "0", "62\n0\n", [40., 0.], [40., 20.]),
        line("S1", "0", [40., 5.], [48., 5.]),
        line("S2", "01", [48., 5.], [48., 15.]),
        line("S3", "0", [48., 15.], [40., 15.]),
        line("BX", "05", [-5., 10.], [55., 10.]),
    ]);
    let e =
        expr_with(&g, "BR", "visible_contour").unwrap_or_else(|| panic!("{:?}", kinds(&g, "BR")));
    assert!(
        names(e, "line_width_evidence", "default_colour_at_sheet_level"),
        "{:?}",
        e.evidence_ids
    );
    assert_eq!(line_type(&g, e, "BR"), "thick_continuous");
    for h in ["S1", "S3"] {
        assert!(kinds(&g, h).contains("visible_contour"), "{h} {:?}", kinds(&g, h));
    }
}

#[test]
fn byblock_colour_extension_line_stays_annotation() {
    // lines in the colour BYBLOCK run from the outline corners past the dimension line of a
    // dimension drawn with loose terminators: extension lines
    let g = run(&[
        line("OA", "01", [0., 0.], [30., 0.]),
        line("OB", "01", [30., 0.], [30., 20.]),
        line("OC", "01", [0., 0.], [0., 20.]),
        line_with("EZA", "0", "62\n0\n", [0., 20.], [0., 34.]),
        line_with("EZB", "0", "62\n0\n", [30., 20.], [30., 34.]),
        solid("T1", "08", [0., 31.], [3., 31.]),
        solid("T2", "08", [30., 31.], [27., 31.]),
        line("DL", "08", [3., 31.], [27., 31.]),
        text("DV", "11", "30", [12., 32.], 3.5),
    ]);
    for h in ["EZA", "EZB"] {
        assert!(!kinds(&g, h).contains("visible_contour"), "{h} {:?}", kinds(&g, h));
        assert!(kinds(&g, h).contains("dimension_linear"), "{h} {:?}", kinds(&g, h));
    }
}

#[test]
fn exploded_piece_with_byblock_linetype_is_not_outline() {
    // a piece of an exploded annotation keeps the line type BYBLOCK and a thin colour; lying
    // between two outline corners does not make it outline
    let g = run(&[
        line("XA", "01", [0., 0.], [40., 0.]),
        line("XT", "01", [0., 20.], [40., 20.]),
        line("XL", "01", [0., 0.], [0., 20.]),
        line_with("XR", "0", "6\nBYBLOCK\n62\n3\n", [40., 0.], [40., 20.]),
        line("XX", "05", [-5., 10.], [45., 10.]),
    ]);
    assert!(
        !kinds(&g, "XR").contains("visible_contour"),
        "{:?}",
        kinds(&g, "XR")
    );
}

// ---------------------------------------------------------------- dimension_line_parts: extension line and outline

type P = [f64; 2];

/// A native linear DIMENSION whose dimension line runs along `rot` degrees through `p10`, with
/// the origins `o13` and `o14` and the entities its block draws.
fn native_linear(
    id: &str,
    block: &str,
    rot: f64,
    p10: P,
    o13: P,
    o14: P,
    children: &[String],
) -> (String, String) {
    let blk = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n{block}\n10\n0\n20\n0\n{}0\nENDBLK\n0\nENDSEC\n",
        children.concat()
    );
    let dim = ent(
        "DIMENSION",
        id,
        "08",
        &format!(
            "2\n{block}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n70\n160\n1\n<>\n13\n{}\n23\n{}\n14\n{}\n24\n{}\n42\n30\n50\n{rot}\n",
            p10[0],
            p10[1],
            p10[0] + 1.,
            p10[1] + 1.,
            o13[0],
            o13[1],
            o14[0],
            o14[1]
        ),
    );
    (blk, dim)
}
/// The dimension line from `a` to `b` with a terminator at each end, as a dimension block draws it.
fn dimension_line(prefix: &str, a: P, b: P) -> Vec<String> {
    let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let u = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
    vec![
        line(&format!("{prefix}1"), "0", a, b),
        solid(
            &format!("{prefix}2"),
            "0",
            a,
            [a[0] + 3. * u[0], a[1] + 3. * u[1]],
        ),
        solid(
            &format!("{prefix}3"),
            "0",
            b,
            [b[0] - 3. * u[0], b[1] - 3. * u[1]],
        ),
    ]
}
/// The members a dimension-like expression holds of one source.
fn in_dimension<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
) -> Vec<&'a MdExpressionMember> {
    let dims: BTreeSet<&str> = g
        .expressions
        .iter()
        .filter(|e| e.kind.starts_with("dimension_"))
        .map(|e| e.id.as_str())
        .collect();
    g.members
        .iter()
        .filter(|m| m.source_handle == src && dims.contains(m.expression_id.as_str()))
        .collect()
}
/// What the round requires of every reading: no extension line lies on a source span read as
/// visible outline, and no dimension or extension line is typed thick.
fn extension_lines_are_thin_and_off_the_outline(g: &MdEngineeringExpressionGraph) {
    let visible: BTreeSet<&str> = g
        .expressions
        .iter()
        .filter(|e| e.kind == "visible_contour")
        .map(|e| e.id.as_str())
        .collect();
    for m in &g.members {
        if m.member_role == "dimension_line" || m.member_role == "extension_line" {
            assert!(
                !m.line_type.starts_with("thick"),
                "{} {} {}",
                m.source_handle,
                m.member_role,
                m.line_type
            );
        }
        if m.member_role != "extension_line" {
            continue;
        }
        for c in g.members.iter().filter(|c| {
            c.source_handle == m.source_handle
                && c.member_role == "contour_segment"
                && visible.contains(c.expression_id.as_str())
        }) {
            let overlap = m.interval[1].min(c.interval[1]) - m.interval[0].max(c.interval[0]);
            assert!(overlap <= 1e-6, "{} is extension line and outline", m.source_handle);
        }
    }
}
/// The outline line is lent to a dimension: one shared membership in its own thick line type,
/// and it is still visible outline.
fn is_lent(g: &MdEngineeringExpressionGraph, src: &str) {
    let ms = in_dimension(g, src);
    assert_eq!(ms.len(), 1, "{src}: {ms:?}");
    assert_eq!(ms[0].member_role, "extension_origin", "{src}");
    assert_eq!(ms[0].line_type, "thick_continuous", "{src}");
    assert!(kinds(g, src).contains("visible_contour"), "{src}");
    let e = g
        .expressions
        .iter()
        .find(|e| e.id == ms[0].expression_id)
        .unwrap();
    assert!(
        names(e, "dimension_line_parts", "outline_as_extension_line"),
        "{:?}",
        e.evidence_ids
    );
    assert!(e.concept_ids.contains(&"DC-G4-058".to_string()));
}

/// A plate 30 wide (x 100..130) and 30 high (y 20..50) on the thick layer with its axis.
fn plate() -> Vec<String> {
    vec![
        line("PB", "01", [100., 20.], [130., 20.]),
        line("PT", "01", [100., 50.], [130., 50.]),
        line("PL", "01", [100., 20.], [100., 50.]),
        line("PR", "01", [130., 20.], [130., 50.]),
        line("PX", "05", [115., 15.], [115., 55.]),
    ]
}

#[test]
fn outline_under_a_native_extension_line_is_lent_to_the_dimension() {
    // the width is dimensioned inside the plate: the block draws its extension lines from the
    // lower corners up along the two sides to the dimension line and a little past it
    let mut children = dimension_line("ND", [100., 30.], [130., 30.]);
    children.push(line("ND4", "0", [100., 20.], [100., 32.]));
    children.push(line("ND5", "0", [130., 20.], [130., 32.]));
    let (blk, dim) = native_linear("DW", "*D7", 0., [130., 30.], [100., 20.], [130., 20.], &children);
    let mut ents = plate();
    ents.push(dim);
    let g = run_full(GB, &blk, &ents);
    is_lent(&g, "PL");
    is_lent(&g, "PR");
    extension_lines_are_thin_and_off_the_outline(&g);
}

#[test]
fn outline_the_terminators_stand_on_is_lent_to_the_dimension() {
    // the same dimension with its extension lines suppressed: the terminators stand on the sides
    let children = dimension_line("NS", [100., 30.], [130., 30.]);
    let (blk, dim) = native_linear("DS", "*D6", 0., [130., 30.], [100., 20.], [130., 20.], &children);
    let mut ents = plate();
    ents.push(dim);
    let g = run_full(GB, &blk, &ents);
    is_lent(&g, "PL");
    is_lent(&g, "PR");
    extension_lines_are_thin_and_off_the_outline(&g);
}

/// Layers named outside any convention, with lineweights 0.50 (P) and 0.25 (Q, R).
const WEIGHTS: &[Layer] = &[
    ("0", 7, "CONTINUOUS", ""),
    ("P", 7, "CONTINUOUS", "370\n50\n"),
    ("Q", 7, "CONTINUOUS", "370\n25\n"),
    ("R", 1, "AXISX", "370\n25\n"),
];

/// The plate drawn by lineweight, its left side in two collinear pieces, a thin line below the
/// plate on the same carrier, and the width dimensioned above it with drawn extension lines.
fn plate_dimensioned_above() -> MdEngineeringExpressionGraph {
    let mut children = dimension_line("NA", [100., 62.], [130., 62.]);
    children.push(line("NA4", "Q", [100., 50.], [100., 64.]));
    children.push(line("NA5", "Q", [130., 50.], [130., 64.]));
    let (blk, dim) = native_linear("DA", "*D5", 0., [130., 62.], [100., 50.], [130., 50.], &children);
    run_full(
        WEIGHTS,
        &blk,
        &[
            line("WB", "P", [100., 20.], [130., 20.]),
            line("WT", "P", [100., 50.], [130., 50.]),
            line("WL1", "P", [100., 35.], [100., 50.]),
            line("WL2", "P", [100., 20.], [100., 35.]),
            line("WR", "P", [130., 20.], [130., 50.]),
            line("WS", "P", [100., 35.], [110., 35.]),
            line("WU", "P", [110., 35.], [110., 20.]),
            line("WQ", "Q", [100., 4.], [100., 20.]),
            line("WX", "R", [115., 0.], [115., 55.]),
            dim,
        ],
    )
}

#[test]
fn outline_continuing_an_extension_line_is_the_measured_feature() {
    let g = plate_dimensioned_above();
    // the side that continues each extension line behind its origin is the measured outline
    is_lent(&g, "WL1");
    is_lent(&g, "WR");
    assert!(
        in_dimension(&g, "WL1")[0]
            .evidence_ids
            .iter()
            .any(|e| e == "behind_the_origin"),
        "{:?}",
        in_dimension(&g, "WL1")[0].evidence_ids
    );
    extension_lines_are_thin_and_off_the_outline(&g);
}

#[test]
fn collinear_growth_stops_at_the_first_outline_piece() {
    let g = plate_dimensioned_above();
    // the second piece of the side and the thin line below it lie on the same carrier but
    // beyond the first outline piece: they are not parts of this dimension
    for h in ["WL2", "WQ"] {
        assert!(in_dimension(&g, h).is_empty(), "{h}: {:?}", in_dimension(&g, h));
    }
    assert!(kinds(&g, "WL2").contains("visible_contour"));
}

#[test]
fn extension_lines_from_opposite_sides_each_stop_past_the_dimension_line() {
    // a vertical dimension line at x = 50 between two origins: one 30 to its left, one 30 to its
    // right; a thin line runs from each origin through the dimension line and 70 beyond it
    let children = dimension_line("NO", [50., 0.], [50., 10.]);
    let (blk, dim) = native_linear("DO", "*D4", 90., [50., 10.], [20., 0.], [80., 10.], &children);
    let g = run_full(
        GB,
        &blk,
        &[
            line("E1", "02", [20., 0.], [120., 0.]),
            line("E2", "02", [80., 10.], [-20., 10.]),
            dim,
        ],
    );
    for h in ["E1", "E2"] {
        let ms = in_dimension(&g, h);
        assert!(!ms.is_empty(), "{h}");
        for m in ms {
            assert_eq!(m.member_role, "extension_line", "{h}");
            // from its own origin (parameter 0) to at most two terminator lengths past the
            // dimension line (30 + 6 of 100), never the far part
            assert!(m.interval[0] == 0. && m.interval[1] <= 0.36 + 1e-9, "{h} {:?}", m.interval);
        }
        assert!(
            in_dimension(&g, h)
                .iter()
                .any(|m| m.evidence_ids.iter().any(|e| e == "runs_past_the_overshoot")),
            "{h}"
        );
    }
    extension_lines_are_thin_and_off_the_outline(&g);
}

#[test]
fn thin_extension_line_drawn_past_its_origin_stays_an_extension_line() {
    // hand-drawn thin extension lines from the upper corners; the dimension (its own extension
    // lines suppressed) takes its origins midway on them
    let children = dimension_line("NH", [100., 72.], [130., 72.]);
    let (blk, dim) = native_linear("DH", "*D3", 0., [130., 72.], [100., 60.], [130., 60.], &children);
    let mut ents = plate();
    ents.extend([
        line("H1", "02", [100., 50.], [100., 74.]),
        line("H2", "02", [130., 50.], [130., 74.]),
        dim,
    ]);
    let g = run_full(GB, &blk, &ents);
    for h in ["H1", "H2"] {
        let ms = in_dimension(&g, h);
        assert!(!ms.is_empty(), "{h}");
        assert!(
            ms.iter()
                .all(|m| m.member_role == "extension_line" && m.line_type == "thin_continuous"),
            "{h}: {ms:?}"
        );
        assert!(!kinds(&g, h).contains("visible_contour"), "{h}");
    }
    // the sides they start from are not parts of the dimension
    assert!(in_dimension(&g, "PL").is_empty());
    extension_lines_are_thin_and_off_the_outline(&g);
}
