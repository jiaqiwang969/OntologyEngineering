//! Line knowledge read from recognition cards (correction round after wave 1).
//!
//! * `extension_line` — an extension line (尺寸界线, GB/T 4458.4) leaves the outline and runs a
//!   little past the dimension line; one drawn line may serve two dimensions; a drafter may draw
//!   it slightly into the outline. Pieces of a line that already serves as an extension line are
//!   given to the dimension they belong to instead of being left unexplained.
//! * `local_section_boundary` — the boundary of a local section is a thin freehand wavy line
//!   (波浪线, GB/T 4457.4); the section lines inside it may be a hatch entity or separate lines
//!   ending on the curve. A thin freehand curve is never a visible outline.
use super::*;

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let v = sub(b, a);
    let n = dot(v, v);
    if n < 1e-18 {
        return dist(p, a);
    }
    let t = (dot(sub(p, a), v) / n).clamp(0., 1.);
    dist(p, add(a, mul(v, t)))
}

/// Uncovered parameter intervals of a source (complement of its member intervals).
fn uncovered(b: &Builder, src: &str) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let mut at = 0.;
    for iv in b.covered(src) {
        if iv[0] > at + 1e-9 {
            out.push([at, iv[0]]);
        }
        at = at.max(iv[1]);
    }
    if at < 1. - 1e-9 {
        out.push([at, 1.]);
    }
    out
}

/// Remainders of extension lines: a short end piece past the dimension line or into the outline
/// joins the extension line it continues; a longer free piece under the terminator of another
/// dimension is that dimension's extension line (one line shared by two dimensions).
pub(super) fn extension_remainders(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("dimension_line_parts")?;
    let short = representation::param(c, "end_piece_max")? * cx.scale.max(1e-6);
    let tip_tol = representation::param(c, "terminator_on_line")? * cx.scale.max(1e-6);
    let perp = representation::param(c, "perpendicular_cos_max")?;
    let ids: Vec<String> = cx.lines.keys().cloned().collect();
    for id in ids {
        let (p, q) = cx.lines[&id];
        let len = dist(p, q);
        if len < 1e-9 {
            continue;
        }
        let ext: Vec<(usize, [f64; 2])> = b
            .by_source
            .get(&id)
            .map(|ms| {
                ms.iter()
                    .map(|&m| &b.members[m])
                    .filter(|m| m.member_role == "extension_line")
                    .filter_map(|m| {
                        b.exprs
                            .iter()
                            .position(|e| e.id == m.expression_id)
                            .map(|e| (e, [m.interval[0], m.interval[1]]))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if ext.is_empty() {
            continue;
        }
        for gap in uncovered(b, &id) {
            let glen = (gap[1] - gap[0]) * len;
            let at_end = gap[0] < 1e-9 || gap[1] > 1. - 1e-9;
            // the expression whose extension-line interval touches this piece
            let touching = ext
                .iter()
                .find(|(_, iv)| (iv[0] - gap[1]).abs() < 1e-6 || (iv[1] - gap[0]).abs() < 1e-6);
            if at_end
                && glen <= short
                && let Some((e, iv)) = touching
            {
                // an end piece past what the rule claimed: beyond the dimension line, or a start
                // drawn into the outline (card dimension_line_parts, variant end_piece)
                let _ = iv;
                let variant = "end_piece";
                let e = *e;
                b.member(
                    e,
                    &id,
                    None,
                    gap,
                    "extension_line",
                    member_line_type(cx.lt(&id), "extension_line"),
                    "candidate",
                    vec![],
                    vec![
                        "card:dimension_line_parts".to_string(),
                        format!("variant:{variant}"),
                    ],
                );
                name(b, e, variant);
                continue;
            }
            // a terminator of another dimension standing on the free piece
            let a = lerp(p, q, gap[0]);
            let z = lerp(p, q, gap[1]);
            let u = unit(sub(q, p));
            let owner = b.exprs.iter().position(|e| {
                (e.kind.starts_with("dimension_") || e.kind == "thread_note")
                    && !ext.iter().any(|(x, _)| b.exprs[*x].id == e.id)
                    && b.members.iter().any(|m| {
                        m.expression_id == e.id
                            && m.member_role == "arrowhead"
                            && cx.arrow(&m.source_handle).is_some_and(|ar| {
                                seg_dist(ar.tip, a, z) < tip_tol && dot(ar.dir, u).abs() < perp
                            })
                    })
            });
            if let Some(e) = owner {
                b.member(
                    e,
                    &id,
                    None,
                    gap,
                    "extension_line",
                    member_line_type(cx.lt(&id), "extension_line"),
                    "candidate",
                    vec![],
                    vec!["card:dimension_line_parts".into(), "variant:shared".into()],
                );
                name(b, e, "shared");
            }
        }
    }
    Ok(())
}

fn name(b: &mut Builder, e: usize, variant: &str) {
    for x in [
        "card:dimension_line_parts".to_string(),
        format!("variant:{variant}"),
        "concept:DC-G4-058".to_string(),
    ] {
        if !b.exprs[e].evidence_ids.contains(&x) {
            b.exprs[e].evidence_ids.push(x);
        }
    }
}

// ---------------------------------------------------------------- local section boundary

/// A freehand curve: an open, unbulged polyline of at least the card's vertex count whose turns
/// are all small and which is not straight (card local_section_boundary).
pub(super) fn free_curve(
    c: &oe_contracts::MdRepresentationCard,
    pts: &[P],
    closed: bool,
    bulged: bool,
) -> bool {
    let (Ok(min_v), Ok(max_turn)) = (
        representation::param(c, "free_curve_min_vertices"),
        representation::param(c, "free_curve_max_turn_deg"),
    ) else {
        return false;
    };
    if closed || bulged || (pts.len() as f64) < min_v {
        return false;
    }
    let mut total = 0.;
    for w in pts.windows(3) {
        let (u, v) = (sub(w[1], w[0]), sub(w[2], w[1]));
        if norm(u) < 1e-9 || norm(v) < 1e-9 {
            continue;
        }
        let ang = cross(unit(u), unit(v))
            .atan2(dot(unit(u), unit(v)))
            .to_degrees();
        if ang.abs() > max_turn {
            return false;
        }
        total += ang.abs();
    }
    total > max_turn
}

/// Thin by the drawing's own colours: the effective colour of the source is a thin colour of the
/// checked layer convention.
pub(super) fn thin_colour(cx: &Ctx, id: &str) -> bool {
    cx.style
        .get(id)
        .is_some_and(|s| cx.thin_colors.contains(&s.color.abs()))
}

/// Separate section lines ending on or near a freehand curve: the curve bounds a local section
/// (card local_section_boundary, variant loose_hatch_lines).
pub(super) fn loose_hatch_boundaries(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref("local_section_boundary")?;
    let end_tol = representation::param(c, "line_end_on_curve")? * cx.scale.max(1e-6);
    let par = representation::param(c, "parallel_deg")?.to_radians().sin();
    let min_lines = representation::param(c, "min_section_lines")? as usize;
    let curves: Vec<(String, Vec<P>)> = cx
        .polys
        .iter()
        .filter(|(h, (pts, closed, bulged))| {
            // the context decides, not the colour: drawings colour their layers differently
            !b.used(h) && cx.lt(h) != "thick_continuous" && free_curve(c, pts, *closed, *bulged)
        })
        .map(|(h, (pts, _, _))| (h.clone(), pts.clone()))
        .collect();
    for (h, pts) in curves {
        let on_curve = |x: P| pts.windows(2).any(|w| seg_dist(x, w[0], w[1]) < end_tol);
        let ending: Vec<(String, P)> = cx
            .lines
            .iter()
            .filter(|(id, (p, q))| {
                !b.used(id)
                    && (cx.annotation_style(id) || thin_colour(cx, id))
                    && (on_curve(*p) || on_curve(*q))
                    && {
                        let u = unit(sub(*q, *p));
                        u[0].abs() > 0.3 && u[1].abs() > 0.3
                    }
            })
            .map(|(id, (p, q))| (id.clone(), unit(sub(*q, *p))))
            .collect();
        // the largest group of mutually parallel lines
        let best = ending
            .iter()
            .map(|(_, u)| {
                ending
                    .iter()
                    .filter(|(_, v)| cross(*u, *v).abs() < par)
                    .count()
            })
            .max()
            .unwrap_or(0);
        if best < min_lines {
            continue;
        }
        let e = b.expr(
            "break_line",
            &h,
            "piece:free_curve_bounding_hatched_local_section",
            "candidate",
            vec![],
            vec![
                "card:local_section_boundary".into(),
                "variant:loose_hatch_lines".into(),
                format!("section_lines_ending_on_curve:{best}"),
            ],
            vec!["Local section boundary; its occluded continuation is not reconstructed".into()],
        );
        b.member(
            e,
            &h,
            None,
            [0., 1.],
            "break_curve",
            member_line_type(cx.lt(&h), "break_curve"),
            "candidate",
            vec![],
            vec![],
        );
    }
    Ok(())
}
