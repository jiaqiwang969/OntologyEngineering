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
    discover(format!("0\nSECTION\n2\nTABLES\n0\nLTYPE\n2\nCONTINUOUS\n73\n0\n0\nLTYPE\n2\nHIDDEN_SYN\n73\n2\n49\n5\n49\n-2\n0\nLAYER\n2\nink\n62\n7\n6\nCONTINUOUS\n0\nENDSEC\n{blocks}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",es.concat()).as_bytes()).unwrap()
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

fn owner<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
) -> &'a oe_contracts::MdEngineeringExpression {
    let m = g
        .members
        .iter()
        .find(|m| {
            m.source_handle == src
                && g.expressions
                    .iter()
                    .any(|e| e.id == m.expression_id && e.kind == "view_callout")
        })
        .unwrap();
    g.expressions
        .iter()
        .find(|e| e.id == m.expression_id)
        .unwrap()
}
#[test]
fn composition_title_defines_its_figure_and_cutting_ends_reference_it() {
    let mut es = named_face(false);
    es.extend([
        line("low", [-17., 8.], [-17., 14.], 60),
        line("high", [-17., 38.], [-17., 44.], 60),
        text("low-letter", "U", [-20., 4.]),
        text("high-letter", "U", [-20., 44.]),
    ]);
    let g = run(es);
    let target = &view_of(&g, "face0").id;
    assert_eq!(owner(&g, "name").view_mark_kind, "view_name");
    assert_eq!(owner(&g, "name").defines_view_ids, vec![target.clone()]);
    let e = owner(&g, "low");
    assert_eq!(e.view_mark_kind, "cutting_symbol");
    assert_eq!(e.references_view_ids, vec![target.clone()]);
    assert_ne!(e.view_id.as_ref(), Some(target));
    assert!(
        e.evidence_ids
            .contains(&format!("references_view:{target}"))
    );
}
fn local_name(has_region: bool) -> Vec<String> {
    let mut es = rect("body", [0., 0., 79., 59.]);
    es.extend([
        text("local-name", "U—U", [116., 78.]),
        line("shoulder", [112., 77.], [128., 77.], 18),
        line("name-leader", [112., 77.], [29., 22.], 18),
    ]);
    if has_region {
        es.push(hatch("local-fill", [20., 13., 39., 31.]));
    }
    es
}
#[test]
fn composition_local_name_targets_the_cut_face_containing_view() {
    let mut es = local_name(true);
    es.extend([
        line("low", [-17., 8.], [-17., 14.], 60),
        line("high", [-17., 38.], [-17., 44.], 60),
        text("low-letter", "U", [-20., 4.]),
        text("high-letter", "U", [-20., 44.]),
    ]);
    let g = run(es);
    let target = view_of(&g, "body0");
    let name = owner(&g, "local-name");
    assert!(name.qualifiers.contains(&"local_section_name".into()));
    assert_eq!(name.defines_view_ids, vec![target.id.clone()]);
    assert_eq!(
        owner(&g, "low").references_view_ids,
        vec![target.id.clone()]
    );
    assert_eq!(target.representation_kind, "view");
    assert_eq!(target.section_extent, "local");
}
#[test]
fn composition_local_name_without_cut_region_keeps_unknown_target() {
    let g = run(local_name(false));
    let e = owner(&g, "local-name");
    assert!(e.qualifiers.contains(&"local_section_name".into()));
    assert!(e.defines_view_ids.is_empty() && !e.unknowns.is_empty());
}
fn insert(id: &str, name: &str, p: P) -> String {
    ent(
        "INSERT",
        id,
        &format!(
            "2\n{name}\n10\n{}\n20\n{}\n41\n1\n42\n1\n50\n0\n",
            p[0], p[1]
        ),
    )
}
fn block(name: &str, es: Vec<String>) -> String {
    format!(
        "0\nBLOCK\n2\n{name}\n10\n0\n20\n0\n{}0\nENDBLK\n",
        es.concat()
    )
}
fn head(id: &str, p: P) -> String {
    ent(
        "SOLID",
        id,
        &format!(
            "10\n{}\n20\n{}\n11\n{}\n21\n{}\n12\n{}\n22\n{}\n13\n{}\n23\n{}\n",
            p[0],
            p[1],
            p[0] - 3.,
            p[1] - 0.5,
            p[0] - 3.,
            p[1] + 0.5,
            p[0] - 3.,
            p[1] + 0.5
        ),
    )
}
#[test]
fn composition_nested_arrow_letter_keeps_leaf_member_and_target() {
    let blocks = format!(
        "0\nSECTION\n2\nBLOCKS\n{}{}{}0\nENDSEC\n",
        block("LetterPart", vec![text("letter-leaf", "V", [3., 2.])]),
        block(
            "ArrowPart",
            vec![
                line("shaft-leaf", [0., 0.], [12., 0.], 18),
                head("head-leaf", [15., 0.]),
                insert("letter-node", "LetterPart", [0., 0.])
            ]
        ),
        block("Wrapper", vec![insert("arrow-node", "ArrowPart", [0., 0.])])
    );
    let mut es = rect("body", [0., 0., 79., 59.]);
    es.extend(rect("side", [190., 0., 223., 27.]));
    es.extend([
        text("side-name", "V向", [197., 33.]),
        insert("arrow-root", "Wrapper", [17., 70.]),
    ]);
    let g = run_blocks(&blocks, es);
    let e = owner(&g, "arrow-root");
    assert_eq!(e.view_mark_kind, "direction_mark");
    assert_eq!(e.references_view_ids, vec![view_of(&g, "side0").id.clone()]);
    assert!(g.members.iter().any(|m| m.expression_id == e.id
        && m.member_role == "view_letter"
        && m.native_member_id.as_deref() == Some("arrow-root/arrow-node/letter-node/letter-leaf")));
}
#[test]
fn composition_letterless_arrow_does_not_promote_elimination_to_a_reference() {
    let mut es = rect("body", [0., 0., 79., 59.]);
    es.extend(rect("side", [190., 0., 223., 27.]));
    es.extend([
        text("side-name", "V向", [197., 33.]),
        line("shaft", [63., 33.], [75., 33.], 18),
        head("arrow", [78., 33.]),
    ]);
    let g = run(es);
    let e = owner(&g, "arrow");
    assert_eq!(e.rule, "arrow_with_short_shaft_at_outline_without_letter");
    assert!(
        e.evidence_ids
            .iter()
            .any(|s| s.ends_with(":by_elimination"))
    );
    assert!(
        e.references_view_ids.is_empty() && !e.unknowns.is_empty(),
        "{e:?}"
    );
}

#[test]
fn composition_ambiguous_repeated_name_does_not_pick_the_nearest_target() {
    let mut es = named_face(false);
    es.extend(rect("other", [370., 0., 403., 27.]));
    es.extend([
        hatch("other-fill", [370., 0., 403., 27.]),
        text("other-name", "U—U", [378., 33.]),
        line("low", [-17., 8.], [-17., 14.], 60),
        line("high", [-17., 38.], [-17., 44.], 60),
        text("low-letter", "U", [-20., 4.]),
        text("high-letter", "U", [-20., 44.]),
    ]);
    let g = run(es);
    let e = owner(&g, "low");
    assert_eq!(e.view_mark_kind, "cutting_symbol");
    assert!(e.references_view_ids.is_empty() && !e.unknowns.is_empty());
    assert!(
        e.evidence_ids
            .iter()
            .any(|s| s.starts_with("references_view:")),
        "legacy evidence must survive"
    );
}

fn members_of<'a>(
    g: &'a MdEngineeringExpressionGraph,
    src: &str,
) -> Vec<&'a oe_contracts::MdExpressionMember> {
    g.members
        .iter()
        .filter(|m| m.source_handle == src)
        .collect()
}
#[test]
fn composition_geometry_and_dimension_keep_separate_membership_roles() {
    let mut es = rect("body", [0., 0., 79., 59.]);
    es.push(ent(
        "DIMENSION",
        "length",
        "70\n0\n1\n79\n10\n0\n20\n65\n11\n39\n21\n67\n13\n0\n23\n0\n14\n79\n24\n0\n42\n79\n",
    ));
    es.push(text("process-note", "sharp edges removed", [16., 18.]));
    let g = run(es);
    let v = view_of(&g, "body0");
    let dim = g
        .expressions
        .iter()
        .find(|e| e.kind == "dimension_linear")
        .unwrap();
    for m in g.members.iter().filter(|m| m.expression_id == dim.id) {
        assert_eq!(m.view_relation_role, "located_only");
        assert_eq!(m.view_component_kind, "auxiliary_annotation");
        assert!(m.composition_view_ids.is_empty());
    }
    for m in members_of(&g, "body0") {
        assert_eq!(m.view_relation_role, "component");
        assert_eq!(m.composition_view_ids, vec![v.id.clone()]);
        assert_eq!(m.view_component_kind, "view_geometry");
    }
}
#[test]
fn composition_cutting_marks_belong_to_the_target_not_their_location() {
    let mut es = named_face(false);
    es.extend([
        line("low", [-17., 8.], [-17., 14.], 60),
        line("high", [-17., 38.], [-17., 44.], 60),
        text("low-letter", "U", [-20., 4.]),
        text("high-letter", "U", [-20., 44.]),
    ]);
    let g = run(es);
    let v = view_of(&g, "face0");
    for (src, role) in [
        ("low", "cutting_location"),
        ("low-letter", "mark_letters"),
        ("name", "view_title"),
    ] {
        let m = members_of(&g, src)[0];
        assert_eq!(m.view_relation_role, "component");
        assert_eq!(m.view_component_kind, role);
        assert_eq!(m.composition_view_ids, vec![v.id.clone()]);
        if src != "name" {
            assert_ne!(m.view_id.as_ref(), Some(&v.id));
        }
    }
}
#[test]
fn composition_detail_fraction_line_name_and_scale_are_distinct_members() {
    let mut es = rect("main", [0., 0., 79., 59.]);
    es.extend(rect("enlarged", [190., 0., 223., 27.]));
    es.extend([
        text("detail-letter", "V", [203., 42.]),
        text("detail-scale", "7:2", [201., 34.]),
        line("fraction-bar", [199., 40.], [211., 40.], 18),
    ]);
    let g = run(es);
    let v = view_of(&g, "enlarged0");
    assert_eq!(v.representation_kind, "detail_view");
    for (src, role) in [
        ("detail-letter", "view_title"),
        ("detail-scale", "view_scale"),
        ("fraction-bar", "view_title"),
    ] {
        let m = members_of(&g, src)[0];
        assert_eq!(m.view_component_kind, role, "{m:?}");
        assert_eq!(m.composition_view_ids, vec![v.id.clone()]);
    }
}
#[test]
fn composition_unresolved_source_is_unknown_rather_than_located_only() {
    let mut es = rect("body", [0., 0., 79., 59.]);
    es.push(text("unknown-note", "@@@", [31., 23.]));
    let g = run(es);
    let unknown = g
        .members
        .iter()
        .filter(|m| {
            g.expressions
                .iter()
                .any(|e| e.id == m.expression_id && e.kind == "unresolved")
        })
        .collect::<Vec<_>>();
    assert!(!unknown.is_empty());
    for m in unknown {
        assert_eq!(m.view_relation_role, "unknown");
        assert!(m.composition_view_ids.is_empty());
        assert_eq!(m.view_component_kind, "unknown");
    }
}
#[test]
fn composition_local_name_leader_is_title_but_quantity_note_is_located_only() {
    let mut es = local_name(true);
    es.push(text("quantity-note", "7处", [116., 72.]));
    let g = run(es);
    let v = view_of(&g, "body0");
    for src in ["shoulder", "name-leader"] {
        let m = members_of(&g, src)[0];
        assert_eq!(m.view_component_kind, "view_title");
        assert_eq!(m.composition_view_ids, vec![v.id.clone()]);
    }
    let m = members_of(&g, "quantity-note")[0];
    assert_eq!(m.view_relation_role, "located_only");
    assert!(m.composition_view_ids.is_empty());
}

#[test]
fn composition_cut_face_boundary_precedes_visible_contour() {
    let g = run(named_face(true));
    let v = view_of(&g, "face0");
    assert_eq!(v.representation_kind, "section_view");
    for src in ["face0", "face1", "face2", "face3"] {
        let m = members_of(&g, src)[0];
        assert_eq!(m.member_role, "contour_segment");
        assert_eq!(m.view_component_kind, "section_boundary");
        assert_eq!(m.composition_view_ids, vec![v.id.clone()]);
        assert_eq!(m.interval, [0., 1.]);
    }
    assert_eq!(
        members_of(&g, "behind")[0].view_component_kind,
        "section_visible_contour"
    );
    assert_eq!(
        members_of(&g, "fill")[0].view_component_kind,
        "section_region"
    );
}
#[test]
fn composition_loose_hatch_strokes_are_lines_rather_than_native_regions() {
    let mut es = rect("body", [0., 0., 63., 63.]);
    for (i, x) in [5., 15., 25., 35.].into_iter().enumerate() {
        es.push(line(
            &format!("hatch-line-{i}"),
            [x, 0.],
            [63., 63. - x],
            18,
        ));
    }
    let g = run(es);
    for i in 0..4 {
        let m = members_of(&g, &format!("hatch-line-{i}"))[0];
        assert_eq!(m.member_role, "hatch_fill");
        assert_eq!(m.view_component_kind, "section_hatch");
        assert_eq!(m.view_relation_role, "component");
    }
}
#[test]
fn composition_outer_view_keeps_its_local_cut_region() {
    let g = run(local_name(true));
    let v = view_of(&g, "body0");
    assert_eq!(v.representation_kind, "view");
    assert_eq!(v.section_extent, "local");
    let m = members_of(&g, "local-fill")[0];
    assert_eq!(m.view_component_kind, "section_region");
    assert_eq!(m.composition_view_ids, vec![v.id.clone()]);
}
#[test]
fn composition_partial_boundary_does_not_relabel_the_whole_member() {
    let mut es = rect("body", [0., 0., 79., 59.]);
    es.push(hatch("partial-fill", [16., 0., 42., 14.]));
    let g = run(es);
    let m = members_of(&g, "body0")[0];
    assert!(
        m.evidence_ids
            .iter()
            .any(|s| s.starts_with("section_boundary:"))
    );
    assert_eq!(m.interval, [0., 1.]);
    assert_eq!(m.member_role, "contour_segment");
    assert_eq!(m.view_relation_role, "component");
    assert_eq!(m.view_component_kind, "unknown");
    assert!(m.unknowns.iter().any(|s| s.contains("部分区间")));
}

fn no_hatch(g: &MdEngineeringExpressionGraph, sources: &[&str]) {
    for src in sources {
        assert!(!members_of(g, src).is_empty(), "missing source {src}");
        for m in members_of(g, src) {
            assert!(
                g.expressions
                    .iter()
                    .any(|e| e.id == m.expression_id && e.kind != "section_hatch"),
                "{src}: {m:?}"
            );
        }
    }
}
#[test]
fn composition_loose_lattice_without_region_stays_unknown() {
    let mut es = rect("body", [0., 0., 63., 63.]);
    for (i, y) in [15., 18., 21., 24.].into_iter().enumerate() {
        es.push(line(&format!("floating-{i}"), [14., y], [20., y + 6.], 18));
    }
    let g = run(es);
    let e = g
        .expressions
        .iter()
        .find(|e| e.rule == "card:loose_hatch_region_guard/without_region")
        .expect("the lattice was examined, not silently ignored");
    assert_eq!(e.kind, "unresolved");
    assert!(
        e.evidence_ids
            .iter()
            .any(|s| s.starts_with("rejected_hatch_rule:"))
    );
    for i in 0..4 {
        let ms = members_of(&g, &format!("floating-{i}"));
        assert!(!ms.is_empty());
        assert!(
            ms.iter()
                .all(|m| m.view_relation_role == "unknown" && m.composition_view_ids.is_empty())
        );
    }
    no_hatch(
        &g,
        &["floating-0", "floating-1", "floating-2", "floating-3"],
    );
}
#[test]
fn composition_bounded_parallel_lines_keep_independent_region_evidence() {
    let mut es = rect("body", [0., 0., 63., 63.]);
    for (i, x) in [7., 17., 27., 37.].into_iter().enumerate() {
        es.push(line(&format!("bounded-{i}"), [x, 0.], [63., 63. - x], 18));
    }
    let g = run(es);
    let m = members_of(&g, "bounded-0")[0];
    let e = g
        .expressions
        .iter()
        .find(|e| e.id == m.expression_id)
        .unwrap();
    assert_eq!(e.kind, "section_hatch");
    assert!(
        e.evidence_ids
            .iter()
            .any(|s| s == "loose_hatch_region:independent_boundary_contacts")
    );
}
#[test]
fn composition_one_short_chamfer_is_not_hatching() {
    let es = vec![
        line("edge-a", [0., 0.], [36., 0.], 60),
        line("corner", [36., 0.], [41., 5.], 18),
        line("edge-b", [41., 5.], [41., 39.], 60),
    ];
    let g = run(es);
    assert!(!members_of(&g, "corner").is_empty());
    no_hatch(&g, &["corner"]);
}
#[test]
fn composition_two_plane_diagonals_are_not_hatching() {
    let mut es = rect("body", [0., 0., 63., 47.]);
    es.extend([
        line("diagonal-a", [0., 0.], [63., 47.], 18),
        line("diagonal-b", [0., 47.], [63., 0.], 18),
    ]);
    let g = run(es);
    assert!(!members_of(&g, "diagonal-a").is_empty());
    no_hatch(&g, &["diagonal-a", "diagonal-b"]);
}
#[test]
fn composition_crossing_grid_is_not_a_loose_material_hatch() {
    let mut es = rect("body", [0., 0., 63., 63.]);
    for (i, x) in [7., 17., 27., 37.].into_iter().enumerate() {
        es.push(line(&format!("grid-a-{i}"), [x, 0.], [63., 63. - x], 18));
    }
    for (i, y) in [24., 34., 44., 54.].into_iter().enumerate() {
        es.push(line(&format!("grid-b-{i}"), [0., y], [y, 0.], 18));
    }
    let g = run(es);
    assert!(
        g.expressions
            .iter()
            .any(|e| e.unknowns.iter().any(|s| s.contains("网格")))
    );
    no_hatch(
        &g,
        &[
            "grid-a-0", "grid-a-1", "grid-a-2", "grid-a-3", "grid-b-0", "grid-b-1", "grid-b-2",
            "grid-b-3",
        ],
    );
}
#[test]
fn composition_native_pattern_does_not_require_loose_line_count() {
    let mut es = rect("body", [0., 0., 63., 47.]);
    es.push(hatch("native-fill", [0., 0., 63., 47.]));
    let g = run(es);
    let m = members_of(&g, "native-fill")[0];
    assert!(
        g.expressions
            .iter()
            .any(|e| e.id == m.expression_id && e.kind == "section_hatch")
    );
    assert_eq!(m.view_component_kind, "section_region");
}

#[test]
fn composition_hidden_contour_prevents_face_only_classification() {
    let mut es = named_face(false);
    es.push(ent(
        "LINE",
        "behind-hidden",
        "370\n18\n6\nHIDDEN_SYN\n10\n190\n20\n8\n11\n223\n21\n8\n",
    ));
    let g = run(es);
    let m = members_of(&g, "behind-hidden")[0];
    assert!(
        g.expressions
            .iter()
            .any(|e| e.id == m.expression_id && e.kind == "hidden_contour")
    );
    assert_eq!(view_of(&g, "face0").representation_kind, "section_view");
    assert_eq!(m.view_component_kind, "view_geometry");
}
