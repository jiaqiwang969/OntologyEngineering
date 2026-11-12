//! Synthetic cut faces from GB/T 4458.6 §§4.4, 7 and GB/T 4459.1 §3.5.
//! Invented handles, geometry and layer names; no project drawing is an input.
use oe_contracts::{MdEngineeringExpression, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::discover;
use serde_json::Value;

type P = [f64; 2];
fn ent(kind: &str, id: &str, layer: &str, tags: &str) -> String {
    format!("0\n{kind}\n5\n{id}\n8\n{layer}\n330\nMODEL\n{tags}")
}
fn line(id: &str, a: P, z: P, weight: i32, axis: bool) -> String {
    ent(
        "LINE",
        id,
        "part-ink",
        &format!(
            "370\n{weight}\n6\n{}\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n",
            if axis { "MIDPAT" } else { "CONTINUOUS" },
            a[0],
            a[1],
            z[0],
            z[1]
        ),
    )
}
fn text(id: &str, value: &str, at: P) -> String {
    ent(
        "TEXT",
        id,
        "notes-ink",
        &format!("1\n{value}\n10\n{}\n20\n{}\n40\n3\n", at[0], at[1]),
    )
}
fn rect_pts(bb: [f64; 4]) -> [P; 4] {
    [
        [bb[0], bb[1]],
        [bb[2], bb[1]],
        [bb[2], bb[3]],
        [bb[0], bb[3]],
    ]
}
fn outline(id: &str, bb: [f64; 4], weight: i32) -> Vec<String> {
    let pts = rect_pts(bb);
    (0..4)
        .map(|i| {
            line(
                &format!("{id}-{i}"),
                pts[i],
                pts[(i + 1) % 4],
                weight,
                false,
            )
        })
        .collect()
}
fn hatch(id: &str, pts: &[P], bulge: bool) -> String {
    let mut tags = format!(
        "10\n0\n20\n0\n30\n0\n2\nTESTLINES\n70\n0\n71\n0\n91\n1\n92\n2\n72\n{}\n73\n1\n93\n{}\n",
        usize::from(bulge),
        pts.len()
    );
    for (i, p) in pts.iter().enumerate() {
        tags += &format!("10\n{}\n20\n{}\n", p[0], p[1]);
        if bulge {
            tags += &format!("42\n{}\n", if i == 0 { 0.5 } else { 0. });
        }
    }
    tags += "97\n0\n75\n0\n76\n1\n52\n45\n41\n1\n77\n0\n78\n1\n53\n45\n43\n0\n44\n0\n45\n-1\n46\n1\n79\n0\n98\n0\n";
    ent("HATCH", id, "cut-ink", &tags)
}
fn run(mut ents: Vec<String>) -> MdEngineeringExpressionGraph {
    ents.push(text("scale-note", "synthetic specimen", [-40., -28.]));
    let tables = "0\nSECTION\n2\nTABLES\n0\nLTYPE\n2\nCONTINUOUS\n73\n0\n0\nLTYPE\n2\nMIDPAT\n73\n4\n49\n9\n49\n-1.5\n49\n1.5\n49\n-1.5\n0\nLAYER\n2\npart-ink\n62\n7\n6\nCONTINUOUS\n0\nLAYER\n2\ncut-ink\n62\n3\n6\nCONTINUOUS\n0\nLAYER\n2\nnotes-ink\n62\n7\n6\nCONTINUOUS\n0\nENDSEC\n";
    discover(
        format!(
            "{tables}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
            ents.concat()
        )
        .as_bytes(),
    )
    .unwrap()
}
fn face(g: &MdEngineeringExpressionGraph) -> &MdEngineeringExpression {
    g.expressions
        .iter()
        .find(|e| e.kind == "section_hatch")
        .unwrap()
}
fn has(e: &MdEngineeringExpression, s: &str) -> bool {
    e.evidence_ids.iter().any(|v| v == s)
}
fn bounds(g: &MdEngineeringExpressionGraph, src: &str) -> Vec<Value> {
    g.members
        .iter()
        .filter(|m| m.source_handle == src)
        .flat_map(|m| &m.evidence_ids)
        .filter_map(|s| s.strip_prefix("section_boundary:"))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn embedded(weight: i32) -> Vec<String> {
    let mut es = outline("parent", [0., 0., 84., 42.], 60);
    es.extend(outline("face", [32., 15., 52., 27.], weight));
    es.push(line("crossing", [0., 21.], [84., 21.], 60, false));
    es.push(hatch("cut-fill", &rect_pts([32., 15., 52., 27.]), false));
    es
}

#[test]
fn axis_boundary_is_bounded_and_keeps_line_type() {
    let mut es = outline("parent", [-12., -18., 90., 40.], 60);
    es.push(line("axis-span", [-12., 0.], [90., 0.], 18, true));
    es.push(hatch("cut-fill", &rect_pts([18., 0., 54., 12.]), false));
    let g = run(es);
    let b = bounds(&g, "axis-span");
    assert!(!b.is_empty());
    for s in b {
        assert_eq!(s["carrier"], "center_line");
        let a = s["interval"][0].as_f64().unwrap();
        let z = s["interval"][1].as_f64().unwrap();
        assert!(
            (a - 30. / 102.).abs() < 1e-8 && (z - 66. / 102.).abs() < 1e-8,
            "{s}"
        );
        assert_eq!(s["points"], serde_json::json!([[18., 0.], [54., 0.]]));
    }
    for m in g.members.iter().filter(|m| m.source_handle == "axis-span") {
        assert_eq!(m.line_type, "thin_dash_dot");
        assert_eq!(m.view_component_kind, "view_geometry");
        assert_eq!(m.view_relation_role, "component");
        assert_eq!(m.interval, [0., 1.]);
        assert!(
            m.evidence_ids
                .iter()
                .any(|s| s == "section_origin:pre_existing")
        );
    }
}

#[test]
fn superimposed_thin_face_retains_parent_outline() {
    let g = run(embedded(18));
    assert!(
        has(face(&g), "cross_section_type:superimposed"),
        "{:?}",
        face(&g).evidence_ids
    );
    assert_eq!(face(&g).cross_section_type.as_deref(), Some("superimposed"));
    assert_eq!(face(&g).section_extent.as_deref(), Some("cross_section"));
    assert_eq!(face(&g).section_origin.as_deref(), Some("cut_formed"));
    assert_eq!(face(&g).section_geometry.as_deref(), Some("complete"));
    assert!(face(&g).concept_ids.iter().any(|s| s == "DC-G2-034"));
    for m in g.members.iter().filter(|m| m.source_handle == "crossing") {
        assert_eq!(
            g.expressions
                .iter()
                .find(|e| e.id == m.expression_id)
                .unwrap()
                .kind,
            "visible_contour"
        );
        assert!(
            m.evidence_ids
                .iter()
                .any(|s| s == "section_origin:pre_existing")
        );
        assert_eq!(m.section_origin.as_deref(), Some("pre_existing"));
    }
}

#[test]
fn ambiguous_width_never_suppresses_a_crossing() {
    let g = run(embedded(60));
    assert!(has(face(&g), "cross_section_type:unknown"));
    assert_eq!(face(&g).cross_section_type.as_deref(), Some("unknown"));
    assert_eq!(face(&g).section_extent.as_deref(), Some("unknown"));
    assert_eq!(face(&g).section_origin.as_deref(), Some("cut_formed"));
    assert_eq!(face(&g).section_geometry.as_deref(), Some("complete"));
    assert!(!face(&g).concept_ids.iter().any(|s| s == "DC-G2-033"));
    for m in g.members.iter().filter(|m| m.source_handle == "crossing") {
        assert!(m.evidence_ids.iter().any(|s| s == "section_origin:unknown"));
        assert!(
            !m.evidence_ids
                .iter()
                .any(|s| s == "section_origin:suppressed")
        );
        assert_eq!(m.section_origin.as_deref(), Some("unknown"));
    }
}

#[test]
fn point_contact_is_not_a_boundary() {
    let mut es = embedded(18);
    es.push(line("touch-only", [26., 9.], [32., 15.], 18, false));
    assert!(bounds(&run(es), "touch-only").is_empty());
}

#[test]
fn closed_polyline_keeps_its_closing_boundary() {
    let mut es = outline("parent", [0., 0., 84., 42.], 60);
    let pts = rect_pts([32., 15., 52., 27.]);
    let mut tags = String::from("370\n18\n90\n4\n70\n1\n");
    for p in pts {
        tags += &format!("10\n{}\n20\n{}\n", p[0], p[1]);
    }
    es.push(ent("LWPOLYLINE", "closed-edge", "part-ink", &tags));
    es.push(hatch("cut-fill", &pts, false));
    let g = run(es);
    let spans = bounds(&g, "closed-edge");
    assert_eq!(spans.len(), 4);
    assert_eq!(
        spans[3]["points"],
        serde_json::json!([[32., 27.], [32., 15.]])
    );
    assert_eq!(spans[3]["interval"][1], 1.);
}

#[test]
fn circular_boundary_uses_exact_angular_span() {
    for (start, end, ccw) in [(0, 90, 1), (270, 360, 0)] {
        let mut es = outline("parent", [0., 0., 84., 42.], 60);
        es.push(ent(
            "ARC",
            "round-boundary",
            "part-ink",
            "370\n18\n10\n38\n20\n21\n40\n9\n50\n0\n51\n180\n",
        ));
        es.push(ent("HATCH", "round-fill", "cut-ink", &format!(
            "10\n0\n20\n0\n2\nTESTLINES\n70\n0\n91\n1\n92\n0\n93\n3\n72\n2\n10\n38\n20\n21\n40\n9\n50\n{start}\n51\n{end}\n73\n{ccw}\n72\n1\n10\n38\n20\n30\n11\n38\n21\n21\n72\n1\n10\n38\n20\n21\n11\n47\n21\n21\n97\n0\n75\n0\n76\n0\n52\n45\n41\n2\n98\n0\n")));
        let g = run(es);
        let spans = bounds(&g, "round-boundary");
        assert_eq!(spans.len(), 1);
        let span = &spans[0];
        assert_eq!(span["parameter"], "source_arc");
        assert!(span["interval"][0].as_f64().unwrap().abs() < 1e-8);
        assert!((span["interval"][1].as_f64().unwrap() - 0.5).abs() < 1e-8);
        assert!((span["arc"][4].as_f64().unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-8);
    }
}

#[test]
fn bulged_hatch_chord_is_not_a_boundary() {
    let mut es = outline("parent", [-12., -18., 90., 40.], 60);
    es.push(line("false-chord", [18., 0.], [54., 0.], 18, true));
    es.push(hatch("cut-fill", &rect_pts([18., 0., 54., 12.]), true));
    let g = run(es);
    assert!(bounds(&g, "false-chord").is_empty());
    assert!(has(
        face(&g),
        "section_boundary_geometry:partial_or_unavailable"
    ));
}

#[test]
fn all_hatch_regions_have_scoped_origins() {
    let mut es = embedded(18);
    es.push(hatch("another-cut", &rect_pts([55., 15., 67., 27.]), false));
    let g = run(es);
    let hs: Vec<_> = g
        .expressions
        .iter()
        .filter(|e| e.kind == "section_hatch")
        .collect();
    assert_eq!(hs.len(), 2);
    for h in hs {
        assert!(h.cross_section_type.is_some());
        assert!(h.section_extent.is_some());
        assert!(h.section_origin.is_some());
        assert!(h.section_geometry.is_some());
        assert!(
            g.members
                .iter()
                .any(|m| m.evidence_ids.contains(&format!("section_region:{}", h.id)))
        );
        for m in g.members.iter().filter(|m| {
            m.evidence_ids
                .iter()
                .any(|s| s == &format!("section_region:{}", h.id))
        }) {
            assert!(
                m.evidence_ids
                    .iter()
                    .any(|s| s.starts_with("section_origin:"))
            );
            let refs: Vec<_> = m
                .evidence_ids
                .iter()
                .filter_map(|s| s.strip_prefix("section_region:"))
                .filter_map(|id| g.expressions.iter().find(|e| e.id == id))
                .filter(|e| e.kind == "section_hatch")
                .collect();
            let types: std::collections::BTreeSet<_> = refs
                .iter()
                .filter_map(|e| e.cross_section_type.as_deref())
                .collect();
            let extents: std::collections::BTreeSet<_> = refs
                .iter()
                .filter_map(|e| e.section_extent.as_deref())
                .collect();
            let geometries: std::collections::BTreeSet<_> = refs
                .iter()
                .filter_map(|e| e.section_geometry.as_deref())
                .collect();
            assert_eq!(
                m.cross_section_type.as_deref(),
                (types.len() == 1)
                    .then(|| types.iter().next().copied())
                    .flatten()
                    .or(Some("unknown"))
            );
            assert_eq!(
                m.section_extent.as_deref(),
                (extents.len() == 1)
                    .then(|| extents.iter().next().copied())
                    .flatten()
                    .or(Some("unknown"))
            );
            assert_eq!(
                m.section_geometry.as_deref(),
                (geometries.len() == 1)
                    .then(|| geometries.iter().next().copied())
                    .flatten()
                    .or(Some("unavailable"))
            );
            assert!(m.section_origin.is_some());
        }
    }
}

#[test]
fn unhatched_shaft_has_no_invented_section() {
    let mut es = outline("shaft", [0., 0., 84., 12.], 60);
    es.push(line("shaft-axis", [-6., 6.], [90., 6.], 18, true));
    let g = run(es);
    assert!(g.expressions.iter().all(|e| e.kind != "section_hatch"));
    assert!(g.members.iter().all(|m| {
        m.evidence_ids
            .iter()
            .all(|s| !s.starts_with("section_origin:"))
    }));
}

#[test]
fn unhatched_detail_keeps_unknown_origins() {
    let mut es = outline("main", [0., 0., 84., 42.], 60);
    es.extend(outline("detail", [180., 0., 210., 24.], 60));
    es.push(text("detail-title", "J 4:1", [186., 29.]));
    let g = run(es);
    let v = g.views.iter().find(|v| v.kind == "detail_view").unwrap();
    let ms: Vec<_> = g
        .members
        .iter()
        .filter(|m| m.view_id.as_ref() == Some(&v.id) && m.member_role == "contour_segment")
        .collect();
    assert!(!ms.is_empty());
    assert!(
        ms.iter()
            .all(|m| m.evidence_ids.iter().any(|s| s == "section_origin:unknown"))
    );
    assert!(ms.iter().all(|m| {
        m.cross_section_type.as_deref() == Some("unknown")
            && m.section_extent.as_deref() == Some("unknown")
            && m.section_origin.as_deref() == Some("unknown")
            && m.section_geometry.as_deref() == Some("unavailable")
    }));
}

#[test]
fn thread_inside_hatch_is_pre_existing() {
    let mut es = outline("body", [0., -24., 28., 24.], 60);
    for (id, y, w) in [
        ("thread-top", 7., 18),
        ("thread-bottom", -7., 18),
        ("bore-top", 5.9, 60),
        ("bore-bottom", -5.9, 60),
    ] {
        es.push(line(id, [0., y], [28., y], w, false));
    }
    es.push(line("bore-axis", [-4., 0.], [32., 0.], 18, true));
    es.push(hatch("cut-fill", &rect_pts([0., 5.9, 28., 24.]), false));
    let g = run(es);
    let ms: Vec<_> = g
        .members
        .iter()
        .filter(|m| m.source_handle == "thread-top" && m.member_role == "thread_line")
        .collect();
    assert!(
        !ms.is_empty(),
        "{:?}",
        g.members
            .iter()
            .filter(|m| m.source_handle == "thread-top")
            .collect::<Vec<_>>()
    );
    assert!(ms.iter().all(|m| {
        m.evidence_ids
            .iter()
            .any(|s| s == "section_origin:pre_existing")
    }));
}

#[test]
fn separate_named_face_is_removed() {
    let mut es = outline("parent", [0., 0., 84., 42.], 60);
    es.extend(outline("detached", [180., 0., 210., 24.], 60));
    es.push(hatch("cut-fill", &rect_pts([180., 0., 210., 24.]), false));
    es.push(text("cut-title", "Q向", [187., 29.]));
    es.push(line("cut-stroke", [-18., 15.], [-18., 21.], 60, false));
    es.push(line("cut-tail", [-18., 21.], [-8.5, 21.], 18, false));
    es.push(ent(
        "SOLID",
        "cut-arrow",
        "part-ink",
        "10\n-6\n20\n21\n11\n-9\n21\n20.5\n12\n-9\n22\n21.5\n13\n-9\n23\n21.5\n",
    ));
    es.push(text("cut-letter", "Q", [-20.5, 23.]));
    let g = run(es);
    assert!(
        has(face(&g), "cross_section_type:removed"),
        "{:?}",
        face(&g).evidence_ids
    );
    assert!(face(&g).concept_ids.iter().any(|s| s == "DC-G2-028"));
    assert!(!face(&g).concept_ids.iter().any(|s| s == "DC-G2-033"));
}
