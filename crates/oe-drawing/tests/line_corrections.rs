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
