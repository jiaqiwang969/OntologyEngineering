//! View representation and composition, independent of where annotations were drawn.
//! The `view_representation_facts` card cites the source clauses and bounds the tests.
use super::*;

const FACTS: &str = "view_representation_facts";

fn push(xs: &mut Vec<String>, s: impl Into<String>) {
    let s = s.into();
    if !xs.contains(&s) {
        xs.push(s);
    }
}

fn has(xs: &[String], s: &str) -> bool {
    xs.iter().any(|x| x == s)
}

fn boundary_intervals(m: &MdExpressionMember) -> Vec<[f64; 2]> {
    m.evidence_ids
        .iter()
        .filter_map(|s| s.strip_prefix("section_boundary:"))
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .filter_map(|v| Some([v["interval"][0].as_f64()?, v["interval"][1].as_f64()?]))
        .collect()
}

/// Full interval coverage is required: a line touching a face is not all face boundary.
fn all_boundary(m: &MdExpressionMember) -> bool {
    let mut spans = boundary_intervals(m);
    spans.sort_by(|a, z| a[0].total_cmp(&z[0]));
    let mut end = m.interval[0];
    for [a, z] in spans {
        if a > end + 1e-8 {
            return false;
        }
        end = end.max(z);
    }
    end >= m.interval[1] - 1e-8
}

fn position(v: &MdDrawingView) -> &'static str {
    if v.kind == "sheet" {
        return "not_applicable";
    }
    if v.kind == "principal" {
        return "principal";
    }
    for (old, role) in [
        ("top_view", "top"),
        ("bottom_view", "bottom"),
        ("left_view", "left"),
        ("right_view", "right"),
        ("rear_view", "rear"),
    ] {
        if has(&v.evidence_ids, &format!("first_angle_position:{old}")) {
            return role;
        }
    }
    if !v.label_source_ids.is_empty() {
        "free"
    } else {
        "unknown"
    }
}

fn surface(cx: &Ctx, b: &Builder, symbols: &[&MdEngineeringExpression]) -> Result<&'static str> {
    let card = representation::card_ref(FACTS)?;
    let tol = representation::param(card, "collinear_tolerance")? * cx.scale;
    let parallel = representation::param(card, "parallel_sine")?;
    let mut answers = BTreeSet::new();
    for e in symbols {
        let offset = has(&e.evidence_ids, "parallel_cutting_planes");
        let rotated = has(&e.evidence_ids, "intersecting_cutting_planes");
        if offset || rotated {
            answers.insert(match (offset, rotated) {
                (true, true) => "combined_planes",
                (true, false) => "offset_planes",
                _ => "rotated_planes",
            });
            continue;
        }
    }
    // The two ends may be stored in different callout expressions or nested blocks.
    if answers.is_empty() {
        let ids: BTreeSet<_> = symbols.iter().map(|e| e.id.as_str()).collect();
        let ss: Vec<_> = b
            .members
            .iter()
            .filter(|m| ids.contains(m.expression_id.as_str()) && m.member_role == "cutting_stroke")
            .map(|m| member_points(cx, m))
            .filter(|p| p.len() == 2 && dist(p[0], p[1]) > tol)
            .collect();
        if let Some(first) = ss.first().filter(|_| ss.len() >= 2) {
            let u = unit(sub(first[1], first[0]));
            let same = ss
                .iter()
                .all(|p| cross(u, unit(sub(p[1], p[0]))).abs() <= parallel);
            if same {
                answers.insert(
                    if ss
                        .iter()
                        .all(|p| cross(u, sub(p[0], first[0])).abs() <= tol)
                    {
                        "single_plane"
                    } else {
                        "offset_planes"
                    },
                );
            }
        }
    }
    Ok(if answers.len() == 1 {
        *answers.first().unwrap()
    } else {
        "unknown"
    })
}

fn representation(cx: &Ctx, b: &Builder, v: &mut MdDrawingView) -> Result<()> {
    if v.kind == "sheet" {
        return Ok(());
    }
    let here: Vec<_> = b
        .exprs
        .iter()
        .filter(|e| e.view_id.as_ref() == Some(&v.id))
        .collect();
    let hs: Vec<_> = here
        .iter()
        .copied()
        .filter(|e| e.kind == "section_hatch")
        .collect();
    let symbols: Vec<_> = b
        .exprs
        .iter()
        .filter(|e| {
            has(&e.evidence_ids, &format!("references_view:{}", v.id))
                && b.members
                    .iter()
                    .any(|m| m.expression_id == e.id && m.member_role == "cutting_stroke")
                && e.view_id.as_deref().map(view_types::sheet_key)
                    == Some(view_types::sheet_key(&v.id))
        })
        .collect();
    let half = has(&v.evidence_ids, "section_type:half");
    let local = hs
        .iter()
        .any(|h| h.section_extent.as_deref() == Some("local"))
        || b.members.iter().any(|m| {
            m.view_id.as_ref() == Some(&v.id)
                && m.evidence_ids
                    .iter()
                    .any(|s| s.starts_with("section_profile:"))
        });
    let cut = view_layout::section_letters(&v.label).is_some()
        || !symbols.is_empty()
        || has(&v.evidence_ids, "view_type:section")
        || half;
    let contour_ids: BTreeSet<_> = here
        .iter()
        .filter(|e| e.kind == "visible_contour")
        .map(|e| e.id.as_str())
        .collect();
    let contours: Vec<_> = b
        .members
        .iter()
        .filter(|m| {
            contour_ids.contains(m.expression_id.as_str()) && m.member_role == "contour_segment"
        })
        .collect();
    let face_only = !hs.is_empty()
        && !contours.is_empty()
        && hs
            .iter()
            .all(|h| h.section_geometry.as_deref() == Some("complete"))
        && contours.iter().all(|m| all_boundary(m));
    v.representation_kind = if v.kind == "detail_view" {
        "detail_view"
    } else if cut && !local {
        if face_only {
            "cross_section"
        } else {
            "section_view"
        }
    } else {
        "view"
    }
    .into();
    let hatch_ids: BTreeSet<_> = hs.iter().map(|e| e.id.as_str()).collect();
    let hatch_points: Vec<_> = b
        .members
        .iter()
        .filter(|m| hatch_ids.contains(m.expression_id.as_str()) && m.member_role == "hatch_fill")
        .flat_map(|m| member_points(cx, m))
        .collect();
    let outline_points: Vec<_> = contours.iter().flat_map(|m| member_points(cx, m)).collect();
    let tol =
        representation::param(representation::card_ref(FACTS)?, "collinear_tolerance")? * cx.scale;
    let full = !hs.is_empty()
        && hs
            .iter()
            .all(|h| h.section_geometry.as_deref() == Some("complete"))
        && bbox_of(&hatch_points)
            .zip(bbox_of(&outline_points))
            .is_some_and(|(a, z)| a.iter().zip(z).all(|(x, y)| (x - y).abs() <= tol));
    v.section_extent = if half {
        "half"
    } else if local {
        "local"
    } else if v.representation_kind == "cross_section" {
        "cross_section"
    } else if cut && full {
        "full"
    } else if cut {
        "unknown"
    } else if hs.is_empty() {
        "not_applicable"
    } else {
        "unknown"
    }
    .into();
    v.cutting_surface_kind = if cut || local {
        surface(cx, b, &symbols)?
    } else if hs.is_empty() {
        "not_applicable"
    } else {
        "unknown"
    }
    .into();
    push(&mut v.evidence_ids, format!("card:{FACTS}"));
    if cut {
        push(
            &mut v.evidence_ids,
            "representation_basis:section_name_or_cutting_symbol",
        );
    }
    if v.section_extent == "full" {
        push(
            &mut v.evidence_ids,
            "section_extent_basis:complete_cut_face_spans_outline",
        );
    }
    if face_only {
        push(
            &mut v.evidence_ids,
            "representation_basis:all_contours_are_cut_face_boundaries",
        );
    }
    if v.section_extent == "unknown" && cut {
        push(
            &mut v.unknowns,
            "剖切范围未知：缺少可区分全剖、半剖及局部剖的完整图形证据",
        );
    }
    if v.cutting_surface_kind == "unknown" && (cut || local) {
        push(
            &mut v.unknowns,
            "剖切面种类未知：剖切路径的端点或转折证据不足",
        );
    }
    Ok(())
}

pub(super) fn state(cx: &Ctx, b: &mut Builder, views: &mut [MdDrawingView]) -> Result<()> {
    representation::card_ref(FACTS)?;
    for v in views {
        v.position_role = position(v).into();
        if v.kind != "sheet" {
            v.unknowns
                .retain(|s| s != "Projection relation to the principal view remains a candidate");
            if v.position_role == "unknown" {
                push(&mut v.unknowns, "位置角色未知：没有已确认的投影配置或名称");
            }
        }
        representation(cx, b, v)?;
    }
    Ok(())
}
