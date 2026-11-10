//! Lines after the expert's review of the four new drawings (correction round, lines workstream):
//! content that never plots, BYBLOCK as no colour of its own, the width class a line is plotted
//! with, extension lines against the outline they start from, terminators and value fragments of
//! native dimensions. Fixtures are authored from card knowledge (GB/T 4457.4, 4458.4, 14665,
//! 18229); handles, coordinates, layer names other than the GB/T 14665 table 6 identifiers and the
//! CAD definition-point layer, and labels are invented.
use oe_contracts::{
    MdEngineeringExpression, MdEngineeringExpressionGraph, MdExpressionMember, MdLineStyle,
};
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

#[test]
fn outline_behind_the_origin_or_beyond_the_dimension_line_is_not_lent() {
    // the width dimensioned above the plate; on the right carrier the outline stops short of the
    // origin (a step), and on the left one another edge starts just beyond the dimension line
    let mut children = dimension_line("NB", [100., 62.], [130., 62.]);
    children.push(line("NB4", "0", [100., 50.], [100., 64.]));
    children.push(line("NB5", "0", [130., 50.], [130., 64.]));
    let (blk, dim) = native_linear("DB", "*D1", 0., [130., 62.], [100., 50.], [130., 50.], &children);
    // drawn by lineweight, without a layer convention
    let g = run_full(
        WEIGHTS,
        &blk,
        &[
            line("BB", "P", [100., 20.], [130., 20.]),
            line("BT", "P", [100., 50.], [126., 50.]),
            line("BL", "P", [100., 20.], [100., 50.]),
            line("BS", "P", [126., 50.], [126., 46.]),
            line("BU", "P", [126., 46.], [130., 46.]),
            line("BR", "P", [130., 20.], [130., 46.]),
            line("BQ", "P", [100., 66.], [100., 90.]),
            line("BX", "R", [115., 15.], [115., 55.]),
            dim,
        ],
    );
    is_lent(&g, "BL");
    for h in ["BR", "BQ"] {
        assert!(in_dimension(&g, h).is_empty(), "{h}: {:?}", in_dimension(&g, h));
        assert!(kinds(&g, h).contains("visible_contour"), "{h}");
    }
    extension_lines_are_thin_and_off_the_outline(&g);
}

// ---------------------------------------------------------------- line_width_evidence: the width each line is plotted with

fn run_with_header(
    header: &str,
    layers: &[Layer],
    blocks: &str,
    ents: &[String],
) -> MdEngineeringExpressionGraph {
    let bytes = format!(
        "0\nSECTION\n2\nHEADER\n{header}0\nENDSEC\n{}{blocks}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        tables(layers),
        ents.concat()
    );
    match discover(bytes.as_bytes()) {
        Ok(g) => g,
        Err(e) => panic!("{e:#}"),
    }
}
/// Named plot styles: the drawing does not plot by colour.
const NAMED_PLOT_STYLES: &str = "9\n$PSTYLEMODE\n290\n0\n";

fn style<'a>(g: &'a MdEngineeringExpressionGraph, src: &str) -> &'a MdLineStyle {
    g.line_styles
        .iter()
        .find(|s| s.source_handle == src)
        .unwrap_or_else(|| panic!("{src}"))
}
fn has(v: &[String], x: &str) -> bool {
    v.iter().any(|e| e == x)
}

/// The plate with its width dimensioned above it by loose parts (terminators and dimension line
/// on the dimension layer, the value on the text layer); the two extension lines are drawn from
/// the upper corners on `layer` with the entity tags `more`.
fn loose_width(header: &str, layer: &str, more: &str) -> MdEngineeringExpressionGraph {
    let mut ents = plate();
    ents.extend([
        line_with("XA", layer, more, [100., 50.], [100., 64.]),
        line_with("XB", layer, more, [130., 50.], [130., 64.]),
        solid("T1", "08", [100., 61.], [103., 61.]),
        solid("T2", "08", [130., 61.], [127., 61.]),
        line("DL", "08", [103., 61.], [127., 61.]),
        text("DV", "11", "30", [112., 62.], 3.5),
    ]);
    run_with_header(header, GB, "", &ents)
}

#[test]
fn thin_colour_line_on_the_thick_layer_is_reported_thin() {
    // the drawing plots by colour: the thin colour of the extension lines decides, not the layer
    let g = loose_width("", "01", "62\n3\n");
    for h in ["XA", "XB"] {
        let s = style(&g, h);
        assert_eq!(s.line_type, "thin_continuous", "{h}");
        assert_eq!(s.basis, "observed_pattern", "{h}");
        assert!(has(&s.evidence_ids, "variant:colour_before_layer"), "{:?}", s.evidence_ids);
        let ms = in_dimension(&g, h);
        assert!(
            !ms.is_empty()
                && ms
                    .iter()
                    .all(|m| m.member_role == "extension_line" && m.line_type == "thin_continuous"),
            "{h}: {ms:?}"
        );
    }
    extension_lines_are_thin_and_off_the_outline(&g);
}

#[test]
fn colour_is_no_width_evidence_with_named_plot_styles() {
    let g = loose_width(NAMED_PLOT_STYLES, "01", "62\n3\n");
    for h in ["XA", "XB"] {
        // the line table keeps the layer's width
        let s = style(&g, h);
        assert_eq!(s.line_type, "thick_continuous", "{h}");
        assert_eq!(s.basis, "layer_convention", "{h}");
        // the member is thin by its role
        let ms = in_dimension(&g, h);
        assert!(
            !ms.is_empty() && ms.iter().all(|m| m.line_type == "thin_continuous"),
            "{h}: {ms:?}"
        );
    }
}

#[test]
fn thick_colour_line_on_a_thin_layer_is_reported_thick() {
    // the right side of the box is drawn on a thin layer in the outline colour
    let g = rectangle_with_side(GB, "09");
    assert!(kinds(&g, "NR").contains("visible_contour"), "{:?}", kinds(&g, "NR"));
    let s = style(&g, "NR");
    assert_eq!(s.line_type, "thick_continuous");
    assert_eq!(s.basis, "observed_pattern");
    assert!(has(&s.evidence_ids, "variant:colour_before_layer"), "{:?}", s.evidence_ids);
    // a line in the outline colour on a layer that gives no width is reported thick as well
    assert_eq!(style(&g, "NA").line_type, "thick_continuous");
}

#[test]
fn extension_line_on_the_thick_text_layer_is_thin() {
    // the extension lines lie on the layer whose table meaning is thick text, in a colour that is
    // neither the outline colour nor a thin-line colour
    let g = loose_width("", "12", "");
    for h in ["XA", "XB"] {
        let s = style(&g, h);
        assert_eq!(s.line_type, "continuous", "{h}");
        assert!(
            has(&s.evidence_ids, "variant:text_layer_is_no_line_width"),
            "{:?}",
            s.evidence_ids
        );
        let ms = in_dimension(&g, h);
        assert!(
            !ms.is_empty()
                && ms
                    .iter()
                    .all(|m| m.member_role == "extension_line" && m.line_type == "thin_continuous"),
            "{h}: {ms:?}"
        );
    }
}

/// The plate's width dimensioned by a native dimension above it, with a root line drawn over the
/// block's dimension line on `layer`.
fn native_width_over(layers: &[Layer], header: &str, plate_layer: &str, layer: &str) -> MdEngineeringExpressionGraph {
    let mut children = dimension_line("NW", [100., 62.], [130., 62.]);
    children.push(line("NW4", "0", [100., 50.], [100., 64.]));
    children.push(line("NW5", "0", [130., 50.], [130., 64.]));
    let (blk, dim) = native_linear("DW", "*D2", 0., [130., 62.], [100., 50.], [130., 50.], &children);
    let mut ents: Vec<String> = [
        ("PB", [100., 20.], [130., 20.]),
        ("PT", [100., 50.], [130., 50.]),
        ("PL", [100., 20.], [100., 50.]),
        ("PR", [130., 20.], [130., 50.]),
    ]
    .iter()
    .map(|(h, a, b)| line(h, plate_layer, *a, *b))
    .collect();
    ents.extend([
        line("PX", if layers == GB { "05" } else { "R" }, [115., 15.], [115., 55.]),
        line("RD", layer, [103., 62.], [127., 62.]),
        dim,
    ]);
    run_with_header(header, layers, &blk, &ents)
}

#[test]
fn dimension_line_on_the_thick_layer_without_colour_evidence_is_thin() {
    let g = native_width_over(GB, NAMED_PLOT_STYLES, "01", "01");
    let ms = in_dimension(&g, "RD");
    assert!(
        !ms.is_empty()
            && ms
                .iter()
                .all(|m| m.member_role == "dimension_line" && m.line_type == "thin_continuous"),
        "{ms:?}"
    );
    assert!(has(&ms[0].evidence_ids, "variant:annotation_roles_thin"));
    // the line table still says what the layer says
    assert_eq!(style(&g, "RD").line_type, "thick_continuous");
}

#[test]
fn dimension_line_drawn_with_the_thick_lineweight_is_reported_thick() {
    // the drawing's lineweights fall into two classes: the line over the dimension line is drawn
    // with the thick one, so the sheet shows it thick
    let g = native_width_over(WEIGHTS, "", "P", "P");
    let ms = in_dimension(&g, "RD");
    assert!(
        !ms.is_empty()
            && ms
                .iter()
                .all(|m| m.member_role == "dimension_line" && m.line_type == "thick_continuous"),
        "{ms:?}"
    );
    // drawn with the thin one it is thin
    let g = native_width_over(WEIGHTS, "", "P", "Q");
    let ms = in_dimension(&g, "RD");
    assert!(
        !ms.is_empty() && ms.iter().all(|m| m.line_type == "thin_continuous"),
        "{ms:?}"
    );
}

/// Two short strokes on `layer` above and below a view, each with the letter of a section named
/// apart, and that section.
fn lettered_strokes(header: &str, layer: &str) -> MdEngineeringExpressionGraph {
    let mut ents: Vec<String> = [
        ("V1", [0., 0.], [100., 0.]),
        ("V2", [100., 0.], [100., 60.]),
        ("V3", [100., 60.], [0., 60.]),
        ("V4", [0., 60.], [0., 0.]),
        ("W1", [200., 0.], [240., 0.]),
        ("W2", [240., 0.], [240., 30.]),
        ("W3", [240., 30.], [200., 30.]),
        ("W4", [200., 30.], [200., 0.]),
    ]
    .iter()
    .map(|(h, a, b)| line(h, "01", *a, *b))
    .collect();
    ents.extend([
        line("K1", layer, [50., 64.], [50., 69.]),
        line("K2", layer, [50., -4.], [50., -9.]),
        text("E1", "11", "G", [52., 69.], 3.5),
        text("E2", "11", "G", [52., -12.], 3.5),
        text("EN", "11", "G-G", [212., 34.], 3.5),
    ]);
    run_with_header(header, GB, "", &ents)
}

#[test]
fn cutting_stroke_in_the_outline_colour_on_a_thin_layer_is_thick() {
    let strokes = |g: &MdEngineeringExpressionGraph| -> Vec<MdExpressionMember> {
        g.members
            .iter()
            .filter(|m| m.member_role == "cutting_stroke")
            .cloned()
            .collect()
    };
    // plotted by colour, the strokes on the thin reference layer are in the outline colour
    let g = lettered_strokes("", "09");
    let ms = strokes(&g);
    assert_eq!(ms.len(), 2, "{ms:?}");
    for m in &ms {
        assert_eq!(m.line_type, "thick_continuous", "{}", m.source_handle);
        assert!(has(&m.evidence_ids, "variant:cutting_stroke_by_outline_evidence"));
    }
    // without colour plotting nothing says they are thick: they keep the layer's width
    let g = lettered_strokes(NAMED_PLOT_STYLES, "09");
    assert!(strokes(&g).iter().all(|m| m.line_type == "thin_continuous"));
}

// ---------------------------------------------------------------- dimension_line_parts: terminators whatever stores them

/// A hatch filled solid whose boundary is the triangle with `tip` and a base of half width `half`
/// centred at `base`.
fn hatch_triangle(id: &str, layer: &str, tip: P, base: P, half: f64) -> String {
    let l = ((base[0] - tip[0]).powi(2) + (base[1] - tip[1]).powi(2)).sqrt();
    let n = [-(base[1] - tip[1]) / l * half, (base[0] - tip[0]) / l * half];
    let (b1, b2) = ([base[0] + n[0], base[1] + n[1]], [base[0] - n[0], base[1] - n[1]]);
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
/// A two-vertex polyline tapering from width `w` at `base` to nothing at `tip`.
fn tapered(id: &str, layer: &str, tip: P, base: P, w: f64) -> String {
    ent(
        "LWPOLYLINE",
        id,
        layer,
        &format!(
            "90\n2\n70\n0\n10\n{}\n20\n{}\n40\n{w}\n41\n0\n10\n{}\n20\n{}\n",
            base[0], base[1], tip[0], tip[1]
        ),
    )
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
fn blocks_section(defs: &[(&str, Vec<String>)]) -> String {
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
/// The plate's width dimensioned above it by a native dimension whose block draws the dimension
/// line, the extension lines and the two terminators `heads`, and `more`.
fn width_with_heads(heads: [String; 2], more: Vec<String>, extra_blocks: &[(&str, Vec<String>)]) -> MdEngineeringExpressionGraph {
    let mut children = vec![
        line("HL1", "0", [100., 62.], [130., 62.]),
        line("HL4", "0", [100., 50.], [100., 64.]),
        line("HL5", "0", [130., 50.], [130., 64.]),
    ];
    children.extend(heads);
    children.extend(more);
    let (_, dim) = native_linear("DH", "*DT", 0., [130., 62.], [100., 50.], [130., 50.], &[]);
    let mut defs: Vec<(&str, Vec<String>)> = vec![("*DT", children)];
    defs.extend(extra_blocks.iter().cloned());
    let mut ents = plate();
    ents.push(dim);
    run_full(GB, &blocks_section(&defs), &ents)
}
/// Role of the native member `id` of dimension `DH`.
fn native_role(g: &MdEngineeringExpressionGraph, id: &str) -> (String, Vec<String>) {
    g.members
        .iter()
        .find(|m| m.native_member_id.as_deref() == Some(&format!("DH/{id}")))
        .map(|m| (m.member_role.clone(), m.evidence_ids.clone()))
        .unwrap_or_else(|| panic!("{id}: {:?}", members(g, "DH")))
}
fn dimension_dh(g: &MdEngineeringExpressionGraph) -> &MdEngineeringExpression {
    exprs_of(g, "DH")
        .into_iter()
        .find(|e| e.kind == "dimension_linear")
        .unwrap()
}

#[test]
fn solid_hatch_triangle_in_a_dimension_block_is_its_terminator() {
    let g = width_with_heads(
        [
            hatch_triangle("HA1", "0", [100., 62.], [104., 62.], 0.5),
            hatch_triangle("HA2", "0", [130., 62.], [126., 62.], 0.5),
        ],
        vec![],
        &[],
    );
    for h in ["HA1", "HA2"] {
        let (role, ev) = native_role(&g, h);
        assert_eq!(role, "arrowhead", "{h}");
        assert!(has(&ev, "variant:terminator_any_entity"), "{ev:?}");
    }
    let e = dimension_dh(&g);
    assert!(names(e, "dimension_line_parts", "terminator_any_entity"));
    assert!(e.concept_ids.contains(&"DC-G4-019".to_string()), "{:?}", e.concept_ids);
}

#[test]
fn tapered_polyline_in_a_dimension_block_is_its_terminator() {
    // the terminators drawn outside the extension lines, on the dimension line's continuation
    let g = width_with_heads(
        [
            tapered("HT1", "0", [100., 62.], [96., 62.], 1.),
            tapered("HT2", "0", [130., 62.], [134., 62.], 1.),
        ],
        vec![line("HL6", "0", [92., 62.], [100., 62.])],
        &[],
    );
    for h in ["HT1", "HT2"] {
        assert_eq!(native_role(&g, h).0, "arrowhead", "{h}");
    }
}

#[test]
fn filled_triangle_across_the_dimension_line_is_not_a_terminator() {
    // two filled triangles whose tips touch the dimension line but whose axes cross it
    let g = width_with_heads(
        [
            hatch_triangle("HX1", "0", [110., 62.], [110., 58.], 0.5),
            hatch_triangle("HX2", "0", [120., 62.], [120., 66.], 0.5),
        ],
        vec![],
        &[],
    );
    for h in ["HX1", "HX2"] {
        assert_eq!(native_role(&g, h).0, "native_member", "{h}");
    }
    assert!(!dimension_dh(&g)
        .evidence_ids
        .contains(&"variant:terminator_any_entity".to_string()));
}

#[test]
fn block_drawing_only_text_in_a_dimension_is_part_of_its_value() {
    let value = vec![text("VT1", "0", "%%c8", [0., 0.], 2.5)];
    let g = width_with_heads(
        [
            hatch_triangle("HB1", "0", [100., 62.], [104., 62.], 0.5),
            hatch_triangle("HB2", "0", [130., 62.], [126., 62.], 0.5),
        ],
        vec![insert("HBV", "0", "VALUE", [112., 63.])],
        &[("VALUE", value)],
    );
    let (role, ev) = native_role(&g, "HBV");
    assert_eq!(role, "dimension_text");
    assert!(has(&ev, "block_draws_only_text"), "{ev:?}");
    assert_eq!(native_role(&g, "HB1").0, "arrowhead");
}
