//! Synthetic drafting compositions, authored independently of evaluation drawings.
use oe_contracts::{MdEngineeringExpression, MdEngineeringExpressionGraph};
use oe_drawing::engineering_expression::discover;

fn entity(kind: &str, id: &str, geometry: bool, tags: &str) -> String {
    let (layer, colour, weight) = if geometry {
        ("profile", 7, 50)
    } else {
        ("annotation", 3, 0)
    };
    format!("0\n{kind}\n5\n{id}\n330\nMODEL\n8\n{layer}\n62\n{colour}\n370\n{weight}\n{tags}")
}
fn line(id: &str, a: [f64; 2], b: [f64; 2]) -> String {
    entity(
        "LINE",
        id,
        false,
        &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], b[0], b[1]),
    )
}
fn text(id: &str, value: &str, p: [f64; 2]) -> String {
    entity(
        "TEXT",
        id,
        false,
        &format!("1\n{value}\n10\n{}\n20\n{}\n40\n4\n", p[0], p[1]),
    )
}
fn circle(id: &str, c: [f64; 2], r: f64) -> String {
    entity(
        "CIRCLE",
        id,
        true,
        &format!("10\n{}\n20\n{}\n40\n{r}\n", c[0], c[1]),
    )
}
fn arrow(id: &str, tip: [f64; 2], base: [f64; 2]) -> String {
    let d = [base[0] - tip[0], base[1] - tip[1]];
    let p = [-(0.15 * d[1]), 0.15 * d[0]];
    entity(
        "SOLID",
        id,
        false,
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
fn run(entities: Vec<String>) -> MdEngineeringExpressionGraph {
    let bytes = format!(
        "0\nSECTION\n2\nTABLES\n0\nLTYPE\n2\nCONTINUOUS\n73\n0\n0\nLAYER\n2\nprofile\n62\n7\n370\n50\n6\nCONTINUOUS\n0\nLAYER\n2\nannotation\n62\n3\n370\n0\n6\nCONTINUOUS\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        entities.concat()
    );
    discover(bytes.as_bytes()).unwrap()
}
fn owner<'a>(
    g: &'a MdEngineeringExpressionGraph,
    source: &str,
    kind: &str,
) -> &'a MdEngineeringExpression {
    g.expressions
        .iter()
        .find(|e| {
            e.kind == kind
                && g.members
                    .iter()
                    .any(|m| m.expression_id == e.id && m.source_handle == source)
        })
        .unwrap_or_else(|| panic!("{source}/{kind}: {:?}", g.expressions))
}
fn owns(g: &MdEngineeringExpressionGraph, e: &MdEngineeringExpression, source: &str) -> bool {
    g.members
        .iter()
        .any(|m| m.expression_id == e.id && m.source_handle == source)
}

#[test]
fn concentric_radius_paths_stop_at_their_common_centre() {
    let g = run(vec![
        circle("OUTER", [60., 60.], 31.),
        circle("INNER", [60., 60.], 19.),
        line("OUTRAD", [60., 60.], [60., 91.]),
        line("OUTEXT", [60., 91.], [60., 107.]),
        line("OUTSH", [43., 107.], [60., 107.]),
        text("OUTTEXT", "R31", [44., 109.]),
        arrow("OUTHEAD", [60., 91.], [60., 87.]),
        line("INRAD", [60., 60.], [79., 60.]),
        line("INEXT", [79., 60.], [95., 60.]),
        line("INSH", [95., 60.], [110., 60.]),
        text("SMALLVALUE", "R19", [96., 62.]),
        arrow("INHEAD", [79., 60.], [75., 60.]),
    ]);
    let outer = owner(&g, "OUTTEXT", "dimension_radius");
    let inner = owner(&g, "SMALLVALUE", "dimension_radius");
    assert!(owns(&g, outer, "OUTRAD") && owns(&g, outer, "OUTHEAD"));
    assert!(
        outer
            .evidence_ids
            .contains(&"variant:radius_leader_carrier".into())
    );
    assert!(owns(&g, inner, "INRAD") && owns(&g, inner, "INHEAD"));
    assert!(!owns(&g, outer, "INRAD") && !owns(&g, outer, "INHEAD"));
    assert!(!owns(&g, inner, "OUTRAD") && !owns(&g, inner, "OUTHEAD"));
}

fn internal_radius(with_arc: bool) -> MdEngineeringExpressionGraph {
    let mut entities = vec![
        line("RADIAL", [180., 160.], [187., 160.]),
        line("OUTSIDE", [187., 160.], [204., 160.]),
        text("RNOTE", "R11", [194., 162.]),
        arrow("RHEAD", [187., 160.], [183., 160.]),
    ];
    if with_arc {
        entities.push(circle("CURVE", [180., 160.], 7.));
    }
    run(entities)
}

#[test]
fn internal_radius_head_uses_arc_geometry_not_nominal_scale() {
    let g = internal_radius(true);
    let e = owner(&g, "RNOTE", "dimension_radius");
    assert!(owns(&g, e, "RHEAD") && owns(&g, e, "RADIAL") && owns(&g, e, "OUTSIDE"));
}

#[test]
fn internal_head_without_a_supporting_arc_is_not_invented() {
    let g = internal_radius(false);
    let e = owner(&g, "RNOTE", "dimension_radius");
    assert!(!owns(&g, e, "RHEAD"));
}

fn polar(angle: f64, radius: f64) -> [f64; 2] {
    let t = angle.to_radians();
    [40. + radius * t.cos(), 40. + radius * t.sin()]
}
fn angular_head(id: &str, angle: f64, start: bool) -> String {
    let t = angle.to_radians();
    let p = polar(angle, 25.);
    let sign = if start { 1. } else { -1. };
    arrow(
        id,
        p,
        [p[0] - sign * 4. * t.sin(), p[1] + sign * 4. * t.cos()],
    )
}
fn chained(ambiguous: bool, taken: bool) -> MdEngineeringExpressionGraph {
    let mut entities = vec![
        entity(
            "ARC",
            "FIRSTARC",
            false,
            "10\n40\n20\n40\n40\n25\n50\n12\n51\n30\n",
        ),
        entity(
            "ARC",
            "FIRSTARC_GAP",
            false,
            "10\n40\n20\n40\n40\n25\n50\n50\n51\n62\n",
        ),
        entity(
            "ARC",
            "SECONDARC",
            false,
            "10\n40\n20\n40\n40\n25\n50\n64\n51\n97\n",
        ),
        angular_head("FIRSTSTART", 10., true),
        angular_head("FIRSTEND", 62., false),
        angular_head("SECONDSTART", 62., true),
        angular_head("SECONDEND", 99., false),
        text("FIRSTVALUE", "52%%d", polar(20., 25.)),
        text("SECONDVALUE", "37%%d", polar(81., 25.)),
        line("SIDESTART", [40., 40.], polar(10., 28.)),
        line("SIDESHARED", [40., 40.], polar(62., 28.)),
        line("DISTANTSIDE", polar(99., 100.), polar(99., 120.)),
    ];
    if ambiguous {
        entities.push(text("COMPETINGVALUE", "53%%d", polar(25., 25.)));
    }
    if taken {
        entities.push(text("TAKENVALUE", "41%%d", polar(40., 25.)));
    }
    run(entities)
}

#[test]
fn adjacent_angle_values_own_separate_arcs_and_opposed_heads() {
    let g = chained(false, false);
    let first = owner(&g, "FIRSTVALUE", "dimension_angle");
    let second = owner(&g, "SECONDVALUE", "dimension_angle");
    assert_ne!(first.id, second.id);
    for src in ["FIRSTARC", "FIRSTSTART", "FIRSTEND", "SIDESTART"] {
        assert!(owns(&g, first, src), "{src}");
    }
    for src in ["SECONDARC", "SECONDSTART", "SECONDEND"] {
        assert!(owns(&g, second, src), "{src}");
        assert!(!owns(&g, first, src), "{src}");
    }
    assert!(!owns(&g, second, "FIRSTEND"));
    assert!(owns(&g, first, "SIDESHARED") && owns(&g, second, "SIDESHARED"));
}

#[test]
fn ambiguous_angular_values_and_distant_side_are_not_claimed() {
    let g = chained(false, false);
    assert!(
        !g.members
            .iter()
            .any(|m| m.source_handle == "DISTANTSIDE" && m.member_role == "extension_line")
    );
    let g = chained(true, false);
    assert!(!g.expressions.iter().any(|e| {
        e.evidence_ids
            .contains(&"variant:chained_angle_sectors".into())
    }));
}

#[test]
fn angle_value_in_a_gap_taken_by_a_neighbour_is_recovered() {
    let g = chained(false, true);
    let e = owner(&g, "TAKENVALUE", "dimension_angle");
    assert!(
        e.evidence_ids
            .contains(&"variant:chained_angle_sector_shared".into())
    );
    assert!(owns(&g, e, "FIRSTARC") && owns(&g, e, "FIRSTSTART"));
}

fn outside_value(extra_head: bool) -> MdEngineeringExpressionGraph {
    let mut entities = vec![
        arrow("LEFTEND", [80., 120.], [84., 120.]),
        arrow("RIGHTEND", [107., 120.], [103., 120.]),
        line("BETWEEN", [81.5, 120.], [105.5, 120.]),
        line("OUTVALUE", [80.04, 120.01], [58., 120.]),
        text("VALUEOUT", "27", [72., 122.]),
        line("LEFTEXT", [80., 100.], [80., 124.]),
        line("RIGHTEXT", [107., 100.], [107., 124.]),
    ];
    if extra_head {
        entities.push(arrow("OTHEREND", [59., 120.], [55., 120.]));
    }
    run(entities)
}
#[test]
fn outside_value_carrier_joins_its_existing_dimension() {
    let g = outside_value(false);
    let e = owner(&g, "VALUEOUT", "dimension_linear");
    assert!(owns(&g, e, "BETWEEN") && owns(&g, e, "OUTVALUE"));
    assert!(
        e.evidence_ids
            .contains(&"variant:outside_value_continuation".into())
    );
}
#[test]
fn outside_carrier_does_not_cross_another_terminator() {
    let g = outside_value(true);
    assert!(!g.expressions.iter().any(|e| {
        e.evidence_ids
            .contains(&"variant:outside_value_continuation".into())
    }));
}

fn native_footprint() -> MdEngineeringExpressionGraph {
    let children = [
        line("BLOCKLINE", [24., 35.04], [54., 35.04]),
        arrow("BLOCKSTART", [20., 35.04], [24., 35.04]),
        arrow("BLOCKEND", [58., 35.04], [54., 35.04]),
    ]
    .concat();
    let block = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*FOOT\n10\n0\n20\n0\n{children}0\nENDBLK\n0\nENDSEC\n"
    );
    let dim = entity(
        "DIMENSION",
        "NATIVE",
        false,
        "2\n*FOOT\n70\n32\n10\n71\n20\n52.04\n12\n13\n22\n17\n13\n33\n23\n40\n14\n71\n24\n40\n42\n38\n50\n0\n",
    );
    let roots = [
        dim,
        line("ROOTCARRIER", [21., 52.], [71., 52.]),
        line("PARALLEL", [21., 53.], [71., 53.]),
        line("SHORTNEAR", [30., 52.], [32., 52.]),
    ]
    .concat();
    let source = format!(
        "0\nSECTION\n2\nTABLES\n0\nLTYPE\n2\nCONTINUOUS\n73\n0\n0\nLAYER\n2\nannotation\n62\n3\n370\n0\n6\nCONTINUOUS\n0\nENDSEC\n{block}0\nSECTION\n2\nENTITIES\n{roots}0\nENDSEC\n0\nEOF\n"
    );
    discover(source.as_bytes()).unwrap()
}
#[test]
fn native_arrow_fill_covers_only_its_root_carrier_interval() {
    let g = native_footprint();
    let e = owner(&g, "NATIVE", "dimension_linear");
    let fill: Vec<_> = g
        .members
        .iter()
        .filter(|m| {
            m.expression_id == e.id
                && m.source_handle == "ROOTCARRIER"
                && m.evidence_ids
                    .contains(&"variant:native_terminator_footprint".into())
        })
        .collect();
    assert_eq!(fill.len(), 2, "{fill:?}");
    for (m, expected) in fill.iter().zip([[0.24, 0.32], [0.92, 1.]]) {
        assert!(
            (m.interval[0] - expected[0]).abs() < 1e-9
                && (m.interval[1] - expected[1]).abs() < 1e-9,
            "{m:?}"
        );
    }
    assert!(g.members.iter().any(|m| m.source_handle == "ROOTCARRIER"
        && m.member_role == "unresolved_source"
        && (m.interval[1] - 0.24).abs() < 1e-9));
}
#[test]
fn native_head_does_not_take_parallel_or_unconnected_lines() {
    let g = native_footprint();
    let e = owner(&g, "NATIVE", "dimension_linear");
    assert!(!owns(&g, e, "PARALLEL") && !owns(&g, e, "SHORTNEAR"));
}

fn viewing(framed: bool, named: bool, branched: bool) -> MdEngineeringExpressionGraph {
    let mut entities = vec![
        text("LETTER", "Q", [230., 208.]),
        line("SHAFT", [242., 207.], [242., 223.]),
        arrow("VIEWHEAD", [242., 227.], [242., 223.]),
        circle("VIEWBODY", [350., 180.], 18.),
    ];
    if named {
        entities.push(text("VIEWNAME", "Q向", [344., 205.]));
    }
    if framed {
        entities.extend([
            line("BOXBOTTOM", [228., 206.], [235., 206.]),
            line("BOXTOP", [228., 214.], [235., 214.]),
            line("BOXLEFT", [228., 206.], [228., 214.]),
            line("BOXRIGHT", [235., 206.], [235., 214.]),
        ]);
    }
    if branched {
        entities.push(line("SHOULDER", [242., 207.], [260., 207.]));
    }
    run(entities)
}
#[test]
fn loose_letter_head_and_shaft_reference_the_named_view() {
    let g = viewing(false, true, false);
    let e = owner(&g, "LETTER", "view_callout");
    assert!(owns(&g, e, "VIEWHEAD") && owns(&g, e, "SHAFT"));
    assert!(
        e.evidence_ids.contains(&"references_view:view:Q向".into()),
        "{e:?}"
    );
    assert!(
        e.evidence_ids
            .contains(&"variant:loose_view_direction".into())
    );
}
#[test]
fn datum_or_unmatched_letter_does_not_create_a_direction_view() {
    for (framed, named, branched) in [
        (true, true, false),
        (false, false, false),
        (false, true, true),
    ] {
        let g = viewing(framed, named, branched);
        assert!(!g.expressions.iter().any(|e| {
            e.evidence_ids
                .contains(&"variant:loose_view_direction".into())
        }));
    }
}

fn aliases() -> MdEngineeringExpressionGraph {
    let rectangle = [
        ([140., 100.], [160., 100.]),
        ([160., 100.], [160., 140.]),
        ([160., 140.], [140., 140.]),
        ([140., 140.], [140., 100.]),
    ];
    let mut entities: Vec<_> = rectangle
        .iter()
        .enumerate()
        .map(|(i, (a, z))| {
            entity(
                "LINE",
                &format!("EDGE{i}"),
                true,
                &format!("10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a[0], a[1], z[0], z[1]),
            )
        })
        .collect();
    entities.extend([
        line("ALIAS", [115.98, 100.], [160., 100.]),
        line("PARALLELCOPY", [142., 100.4], [158., 100.4]),
        line("PARTIALCOPY", [145., 140.], [170., 140.]),
        entity(
            "DIMENSION",
            "PARTIALDIM",
            false,
            "2\n*PART\n70\n32\n10\n140\n20\n100\n13\n116\n23\n90\n14\n140\n24\n90\n42\n24\n50\n0\n",
        ),
    ]);
    let block = format!(
        "0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*PART\n10\n0\n20\n0\n{}0\nENDBLK\n0\nENDSEC\n",
        line("PARTLINE", [116., 100.], [140., 100.])
    );
    let source = format!(
        "0\nSECTION\n2\nTABLES\n0\nLTYPE\n2\nCONTINUOUS\n73\n0\n0\nLAYER\n2\nprofile\n62\n7\n370\n50\n6\nCONTINUOUS\n0\nLAYER\n2\nannotation\n62\n3\n370\n0\n6\nCONTINUOUS\n0\nENDSEC\n{block}0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n0\nEOF\n",
        entities.concat()
    );
    discover(source.as_bytes()).unwrap()
}
#[test]
fn contour_alias_keeps_the_unmatched_sliver_explicit() {
    let g = aliases();
    let edge = owner(&g, "EDGE0", "visible_contour");
    let alias = g
        .members
        .iter()
        .find(|m| {
            m.expression_id == edge.id
                && m.source_handle == "ALIAS"
                && m.member_role == "coincident_source"
        })
        .expect("known alias");
    assert!((alias.interval[0] - (140. - 115.98) / (160. - 115.98)).abs() < 1e-9);
    assert_eq!(alias.interval[1], 1.);
    assert!(g.members.iter().any(|m| m.source_handle == "ALIAS"
        && m.member_role == "unresolved_source"
        && m.interval[0] == 0.
        && (m.interval[1] - 0.02 / 44.02).abs() < 1e-9));
    assert!(
        g.members
            .iter()
            .any(|m| m.source_handle == "ALIAS" && m.member_role == "dimension_line")
    );
}
#[test]
fn nearby_parallel_and_partly_overlapping_lines_are_not_aliases() {
    let g = aliases();
    assert!(!g.members.iter().any(|m| {
        ["PARALLELCOPY", "PARTIALCOPY"].contains(&m.source_handle.as_str())
            && m.evidence_ids
                .contains(&"variant:coincident_contour_interval".into())
    }));
}
