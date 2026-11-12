//! Invented drawing fragments. No project drawing or reference ledger is an input.
use oe_contracts::{MdDrawingView, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::discover;
type P = [f64; 2];
fn ent(kind: &str, id: &str, tags: &str) -> String {
    format!("0\n{kind}\n5\n{id}\n8\nink\n330\nMODEL\n{tags}")
}
fn line(id: &str, a: P, z: P, w: i32) -> String {
    ent(
        "LINE",
        id,
        &format!(
            "370\n{w}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
            a[0], a[1], z[0], z[1]
        ),
    )
}
fn text(id: &str, s: &str, p: P) -> String {
    ent(
        "TEXT",
        id,
        &format!("1\n{s}\n10\n{}\n20\n{}\n40\n3\n", p[0], p[1]),
    )
}
fn rect(id: &str, bb: [f64; 4]) -> Vec<String> {
    let p = [
        [bb[0], bb[1]],
        [bb[2], bb[1]],
        [bb[2], bb[3]],
        [bb[0], bb[3]],
    ];
    (0..4)
        .map(|i| line(&format!("{id}{i}"), p[i], p[(i + 1) % 4], 60))
        .collect()
}
fn hatch(id: &str, bb: [f64; 4]) -> String {
    ent(
        "HATCH",
        id,
        &format!(
            "10\n0\n20\n0\n2\nTESTLINES\n70\n0\n91\n1\n92\n2\n72\n0\n73\n1\n93\n4\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n10\n{}\n20\n{}\n97\n0\n75\n0\n76\n1\n52\n45\n41\n1\n78\n1\n53\n45\n43\n0\n44\n0\n45\n-1\n46\n1\n79\n0\n98\n0\n",
            bb[0], bb[1], bb[2], bb[1], bb[2], bb[3], bb[0], bb[3]
        ),
    )
}
fn run(es: Vec<String>) -> MdEngineeringExpressionGraph {
    run_blocks("", es)
}
fn run_blocks(blocks: &str, mut es: Vec<String>) -> MdEngineeringExpressionGraph {
    es.push(text("scale-anchor", "synthetic specimen", [-49., -31.]));
    discover(format!("0\nSECTION\n2\nTABLES\n0\nLTYPE\n2\nCONTINUOUS\n73\n0\n0\nLAYER\n2\nink\n62\n7\n6\nCONTINUOUS\n0\nENDSEC\n{blocks}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",es.concat()).as_bytes()).unwrap()
}
fn view_of<'a>(g: &'a MdEngineeringExpressionGraph, src: &str) -> &'a MdDrawingView {
    let m = g
        .members
        .iter()
        .find(|m| m.source_handle == src && m.member_role == "contour_segment")
        .unwrap();
    g.views
        .iter()
        .find(|v| Some(&v.id) == m.view_id.as_ref())
        .unwrap()
}
#[test]
fn composition_first_angle_names_follow_projection() {
    let mut es = rect("hub", [30., 110., 92., 158.]);
    es.extend(rect("below", [30., 20., 92., 68.]));
    es.extend(rect("above", [30., 205., 92., 253.]));
    es.extend(rect("left", [-100., 110., -38., 158.]));
    es.extend(rect("right", [180., 110., 242., 158.]));
    let g = run(es);
    for (src, role) in [
        ("hub0", "principal"),
        ("below0", "top"),
        ("above0", "bottom"),
        ("left0", "right"),
        ("right0", "left"),
    ] {
        assert_eq!(view_of(&g, src).position_role, role, "{src}");
        assert_eq!(view_of(&g, src).representation_kind, "view");
    }
}
fn named_face(extra: bool) -> Vec<String> {
    let mut es = rect("parent", [0., 0., 63., 47.]);
    es.extend(rect("face", [190., 0., 223., 27.]));
    es.push(hatch("fill", [190., 0., 223., 27.]));
    es.push(text("name", "U—U", [198., 33.]));
    if extra {
        es.push(line("behind", [190., 13.], [223., 13.], 60));
    }
    es
}
#[test]
fn composition_face_only_differs_from_projected_contour() {
    let a = run(named_face(false));
    assert_eq!(view_of(&a, "face0").representation_kind, "cross_section");
    assert_eq!(view_of(&a, "face0").section_extent, "cross_section");
    let b = run(named_face(true));
    assert_eq!(view_of(&b, "face0").representation_kind, "section_view");
}
#[test]
fn composition_principal_section_and_cross_section_are_independent() {
    let mut es = rect("hub", [30., 110., 92., 158.]);
    es.push(hatch("fill", [30., 110., 92., 158.]));
    es.push(text("name", "U—U", [48., 164.]));
    es.push(line("behind", [30., 133.], [92., 133.], 60));
    es.extend(rect("side", [180., 110., 242., 158.]));
    es.extend(rect("below", [30., 20., 92., 68.]));
    let g = run(es);
    let v = view_of(&g, "hub0");
    assert_eq!(v.position_role, "principal");
    assert_eq!(v.representation_kind, "section_view");
    assert_eq!(v.section_extent, "full");
}
#[test]
fn composition_unlabelled_hatch_does_not_invent_cutting_plane() {
    let mut es = rect("body", [0., 0., 63., 47.]);
    es.push(hatch("fill", [0., 0., 63., 47.]));
    let g = run(es);
    let v = view_of(&g, "body0");
    assert_eq!(v.representation_kind, "view");
    assert_eq!(v.cutting_surface_kind, "unknown");
    assert_eq!(v.section_extent, "unknown");
}
#[test]
fn composition_cutting_surface_requires_two_collinear_ends() {
    let mut es = named_face(false);
    es.extend([
        line("end-low", [-17., 8.], [-17., 14.], 60),
        line("end-high", [-17., 38.], [-17., 44.], 60),
        text("letter-low", "U", [-20., 4.]),
        text("letter-high", "U", [-20., 44.]),
    ]);
    let g = run(es);
    let v = view_of(&g, "face0");
    assert_eq!(
        v.cutting_surface_kind,
        "single_plane",
        "{:?}",
        g.expressions
            .iter()
            .filter(|e| e.kind == "view_callout")
            .collect::<Vec<_>>()
    );
}
