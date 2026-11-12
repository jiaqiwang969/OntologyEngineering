//! Names define their own figure; cutting, direction and detail marks refer to that figure.
//! Matching is sheet-local and source-bound. Elimination alone stays an unresolved candidate.
use super::*;

const CARD: &str = "view_mark_targets";

fn push(xs: &mut Vec<String>, s: impl Into<String>) {
    let s = s.into();
    if !xs.contains(&s) {
        xs.push(s);
    }
}

fn same_sheet(e: &MdEngineeringExpression, v: &MdDrawingView) -> bool {
    v.kind != "sheet"
        && e.view_id.as_deref().map(view_types::sheet_key) == Some(view_types::sheet_key(&v.id))
}

fn label(v: &MdDrawingView) -> &str {
    if view_types::sheet_key(&v.id).is_empty() {
        &v.label
    } else {
        v.label.split_once(" · ").map_or(&v.label, |(_, l)| l)
    }
}

fn matches_key(a: &str, z: &str) -> bool {
    a == z
        || tx::roman_value(a)
            .zip(tx::roman_value(z))
            .is_some_and(|(x, y)| x == y)
}

fn point_distance(p: P, a: P, z: P) -> f64 {
    let d = sub(z, a);
    let t = (dot(sub(p, a), d) / dot(d, d).max(1e-18)).clamp(0., 1.);
    dist(p, add(a, mul(d, t)))
}

/// A local name's leader must end in an actual cut region, not merely in a view's box.
fn local_targets(cx: &Ctx, b: &Builder, i: usize, views: &[MdDrawingView]) -> Result<Vec<String>> {
    let e = &b.exprs[i];
    let tol = representation::param(representation::card_ref(CARD)?, "leader_contact")? * cx.scale;
    let lines: Vec<_> = b
        .members
        .iter()
        .filter(|m| m.expression_id == e.id && m.member_role == "leader_line")
        .map(|m| member_points(cx, m))
        .filter(|p| p.len() >= 2)
        .collect();
    let text_points: Vec<_> = b
        .members
        .iter()
        .filter(|m| {
            m.expression_id == e.id
                && matches!(m.member_role.as_str(), "view_title" | "view_letter")
        })
        .flat_map(|m| member_points(cx, m))
        .collect();
    let Some(text_bb) = bbox_of(&text_points) else {
        return Ok(vec![]);
    };
    let text_center = [
        (text_bb[0] + text_bb[2]) / 2.,
        (text_bb[1] + text_bb[3]) / 2.,
    ];
    let mut ends = Vec::new();
    for (j, p) in lines.iter().enumerate() {
        for q in [p[0], p[p.len() - 1]] {
            if !lines
                .iter()
                .enumerate()
                .any(|(k, r)| k != j && r.iter().any(|z| dist(q, *z) <= tol))
            {
                ends.push(q);
            }
        }
    }
    // The free terminal farthest from the name, along its already recognised leader chain.
    let Some(end) = ends
        .into_iter()
        .max_by(|a, z| dist(*a, text_center).total_cmp(&dist(*z, text_center)))
    else {
        return Ok(vec![]);
    };
    let hs = section_hatching::hatches(cx, b)?;
    let mut out = Vec::new();
    for h in hs.iter().filter(|h| !h.boundary.is_empty()) {
        let Some(v) = views
            .iter()
            .find(|v| Some(&v.id) == h.view.as_ref() && same_sheet(e, v))
        else {
            continue;
        };
        if representation::inside_segments(end, &h.boundary)
            || h.boundary
                .iter()
                .any(|(a, z)| point_distance(end, *a, *z) <= tol)
        {
            push(&mut out, v.id.clone());
        }
    }
    Ok(out)
}

/// The native block reader already descends through nested INSERTs. Preserve those leaf paths
/// as letter members even when an older arrow recognizer has claimed only its carrier.
fn nested_letters(cx: &Ctx, b: &mut Builder) {
    for i in 0..b.exprs.len() {
        let e = &b.exprs[i];
        if e.kind != "view_callout"
            || e.qualifiers
                .iter()
                .any(|q| q.ends_with("title") || q == "local_section_name")
            || b.members
                .iter()
                .any(|m| m.expression_id == e.id && m.member_role == "view_letter")
        {
            continue;
        }
        if !b.members.iter().any(|m| {
            m.expression_id == e.id
                && matches!(
                    m.member_role.as_str(),
                    "view_arrow" | "arrowhead" | "cutting_stroke"
                )
        }) {
            continue;
        }
        let roots: BTreeSet<_> = b
            .members
            .iter()
            .filter(|m| m.expression_id == e.id)
            .map(|m| m.source_handle.clone())
            .collect();
        let letters: Vec<_> = roots
            .iter()
            .filter_map(|r| cx.blocks.roots.get(r))
            .flat_map(|bl| bl.parts.iter().map(move |p| (bl, p)))
            .filter_map(|(bl, p)| match &p.geom {
                annotation_blocks::Geom::Text(t, _)
                    if !t.dec.text.is_empty()
                        && t.dec.text.len() <= 2
                        && t.dec.text.chars().all(|c| c.is_ascii_uppercase()) =>
                {
                    Some((
                        bl.root.clone(),
                        p.path.clone(),
                        t.dec.text.trim().to_owned(),
                    ))
                }
                _ => None,
            })
            .collect();
        let values: BTreeSet<_> = letters.iter().map(|(_, _, s)| s.as_str()).collect();
        if values.len() != 1 {
            continue;
        }
        let value = (*values.first().unwrap()).to_owned();
        let first_new = b.members.len();
        let located = b.exprs[i].view_id.clone();
        for (root, path, _) in &letters {
            b.member(
                i,
                root,
                Some(path.clone()),
                [0., 1.],
                "view_letter",
                "text",
                "candidate",
                vec![],
                vec![
                    format!("card:{CARD}"),
                    "variant:nested_letter_member".into(),
                ],
            );
        }
        for m in &mut b.members[first_new..] {
            m.view_id = located.clone();
        }
        if b.exprs[i].label.trim().is_empty()
            || b.exprs[i].rule == "arrow_with_short_shaft_at_outline_without_letter"
        {
            b.exprs[i].label = value;
        }
    }
}

pub(super) fn state(cx: &Ctx, b: &mut Builder, views: &[MdDrawingView]) -> Result<()> {
    representation::card_ref(CARD)?;
    nested_letters(cx, b);
    let mut local = BTreeMap::<usize, Vec<String>>::new();
    for (i, _) in b
        .exprs
        .iter()
        .enumerate()
        .filter(|(_, e)| e.qualifiers.iter().any(|q| q == "local_section_name"))
    {
        let targets = local_targets(cx, b, i, views)?;
        if targets.len() == 1 {
            local.insert(i, targets);
        }
    }
    // Definitions are keyed by exact names and source identities, never the closest region.
    let mut names: Vec<(String, String, String)> = Vec::new();
    for v in views.iter().filter(|v| v.kind != "sheet") {
        if let Some(key) = view_layout::section_letters(label(v)) {
            names.push((view_types::sheet_key(&v.id).to_owned(), key, v.id.clone()));
        }
    }
    for (&i, targets) in &local {
        if let Some(key) = view_layout::section_letters(&b.exprs[i].label) {
            for target in targets {
                names.push((
                    view_types::sheet_key(target).to_owned(),
                    key.clone(),
                    target.clone(),
                ));
            }
        }
    }
    for i in 0..b.exprs.len() {
        if b.exprs[i].kind != "view_callout" {
            continue;
        }
        let e = &b.exprs[i];
        let ms: Vec<_> = b
            .members
            .iter()
            .filter(|m| m.expression_id == e.id)
            .collect();
        let title = e
            .qualifiers
            .iter()
            .any(|q| q == "view_title" || q == "detail_title" || q == "local_section_name");
        let cutting = ms.iter().any(|m| m.member_role == "cutting_stroke")
            || e.qualifiers.iter().any(|q| q == "cutting_plane");
        let detail = e.qualifiers.iter().any(|q| q == "detail_indicator");
        let boundary = e.rule.starts_with("card:detail_view/boundary_");
        let kind = if title {
            "view_name"
        } else if cutting {
            "cutting_symbol"
        } else if detail || boundary {
            "detail_mark"
        } else {
            "direction_mark"
        };
        let key = e.label.split_whitespace().next().unwrap_or("");
        let mut targets = Vec::new();
        if title {
            if e.qualifiers.iter().any(|q| q == "local_section_name") {
                targets = local.get(&i).cloned().unwrap_or_default();
            } else {
                for v in views.iter().filter(|v| same_sheet(e, v)) {
                    if ms
                        .iter()
                        .any(|m| v.label_source_ids.contains(&m.source_handle))
                    {
                        push(&mut targets, v.id.clone());
                    }
                }
            }
        } else if boundary {
            for v in views
                .iter()
                .filter(|v| same_sheet(e, v) && v.kind == "detail_view")
            {
                if e.view_id.as_ref() == Some(&v.id) {
                    push(&mut targets, v.id.clone());
                }
            }
        } else if !key.is_empty() && ms.iter().any(|m| m.member_role == "view_letter") {
            if cutting {
                for (sheet, name, target) in &names {
                    if sheet.as_str()
                        == e.view_id
                            .as_deref()
                            .map(view_types::sheet_key)
                            .unwrap_or("")
                        && name == key
                    {
                        push(&mut targets, target.clone());
                    }
                }
                // A figure named as a direction view can still be obtained by cutting.
                if targets.is_empty() {
                    for v in views
                        .iter()
                        .filter(|v| same_sheet(e, v) && label(v) == format!("{key}向"))
                    {
                        push(&mut targets, v.id.clone());
                    }
                }
            } else {
                for v in views.iter().filter(|v| same_sheet(e, v)) {
                    let name = label(v).split_whitespace().next().unwrap_or("");
                    if (detail && v.kind == "detail_view" && matches_key(key, name))
                        || (!detail && label(v) == format!("{key}向"))
                    {
                        push(&mut targets, v.id.clone());
                    }
                }
            }
        }
        let e = &mut b.exprs[i];
        e.view_mark_kind = kind.into();
        push(&mut e.evidence_ids, format!("card:{CARD}"));
        if targets.len() == 1 {
            if title || boundary {
                e.defines_view_ids = targets;
            } else {
                for v in &targets {
                    push(&mut e.evidence_ids, format!("references_view:{v}"));
                }
                e.references_view_ids = targets;
            }
        } else {
            push(
                &mut e.unknowns,
                if targets.is_empty() {
                    "视图标注目标未知：缺少同页名称、编号或引线剖面接触证据"
                } else {
                    "视图标注目标未知：同页有多个同名候选，不能仅靠排除法确认"
                },
            );
        }
    }
    Ok(())
}
