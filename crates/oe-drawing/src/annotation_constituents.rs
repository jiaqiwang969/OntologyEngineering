//! Local constituent ownership from drafting geometry. No reference answers or
//! drawing identities enter these rules (GB/T 4458.4, 4458.1 and 14665).
use super::*;

const CARD: &str = "annotation_constituents";

fn parameter(name: &str) -> Result<f64> {
    representation::param(representation::card_ref(CARD)?, name)
}

fn evidence(variant: &str, concept: &str) -> Vec<String> {
    vec![
        format!("card:{CARD}"),
        format!("variant:{variant}"),
        format!("concept:{concept}"),
    ]
}

type Branch = (Vec<String>, Vec<String>, Vec<P>);
type RadiusFrontier = (P, Vec<String>, Option<(String, P, f64)>);

struct SelectedAngle {
    owner: usize,
    score: f64,
    shared: Vec<String>,
    arrows: Vec<String>,
}

/// A radius head may be at an inner vertex of the carrier rather than its end.
/// Follow the text's shoulder to that head, then only toward its arc centre.
/// In particular, a second radius sharing the centre is not another branch.
pub(super) fn radius_branch(
    cx: &Ctx,
    b: &Builder,
    starts: &[(P, P)],
    taken: &BTreeSet<String>,
) -> Result<Option<Branch>> {
    let incidence = parameter("incidence_arrow")?;
    let direction = parameter("direction_sine")?;
    let segments = free_segments(cx, b);
    let mut frontier: Vec<RadiusFrontier> =
        starts.iter().map(|(p, _)| (*p, vec![], None)).collect();
    let mut candidates = Vec::new();
    // Four connected pieces is the existing shoulder traversal budget.
    for _ in 0..=4 {
        let mut next = Vec::new();
        for (p, path, mut head) in frontier {
            if head.is_none() {
                let matches: Vec<_> = cx
                    .arrows
                    .iter()
                    .filter(|(id, a)| !b.used(id) && dist(p, a.tip) <= incidence * a.len)
                    .flat_map(|(id, a)| {
                        cx.rec.keys().filter_map(move |h| {
                            let (c, r, start, sweep, full) = cx.arc(h)?;
                            let radial = sub(a.tip, c);
                            let angle = radial[1].atan2(radial[0]);
                            let inside = full
                                || (angle - start).rem_euclid(std::f64::consts::TAU)
                                    <= sweep + incidence * a.len / r;
                            (matches!(cx.kind(h), "ARC" | "CIRCLE")
                                && !b.kinds_of(h).iter().any(|k| k.starts_with("dimension_"))
                                && (norm(radial) - r).abs() <= incidence * a.len
                                && cross(unit(radial), a.dir).abs() <= direction
                                && inside)
                                .then(|| (id.clone(), c, incidence * a.len))
                        })
                    })
                    .collect();
                if let Some(first) = matches.first()
                    && matches
                        .iter()
                        .all(|m| m.0 == first.0 && dist(m.1, first.1) <= first.2)
                {
                    head = Some(first.clone());
                }
            }
            if let Some((id, c, tol)) = &head
                && dist(p, *c) <= *tol
            {
                candidates.push((path, vec![id.clone()], vec![*c]));
                continue;
            }
            for (id, pts) in &segments {
                if taken.contains(id) || path.contains(id) {
                    continue;
                }
                let tol = 1e-3 * cx.scale.max(1.);
                let other = if dist(pts[0], p) < tol {
                    Some(pts[pts.len() - 1])
                } else if dist(pts[pts.len() - 1], p) < tol {
                    Some(pts[0])
                } else {
                    None
                };
                let Some(q) = other else { continue };
                if let Some((_, c, tol)) = &head
                    && (dist(q, *c) >= dist(p, *c)
                        || pts.iter().any(|v| project(*v, p, *c).1 > *tol))
                {
                    continue;
                }
                let mut path = path.clone();
                path.push(id.clone());
                next.push((q, path, head.clone()));
            }
        }
        frontier = next;
    }
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    candidates.dedup();
    Ok((candidates.len() == 1).then(|| candidates.remove(0)))
}

pub(super) fn tag_radius(b: &mut Builder, e: usize) {
    b.exprs[e]
        .evidence_ids
        .extend(evidence("radius_internal_head", "DC-G4-006"));
}

/// A radius written on a leader-style shoulder still has one radius terminal.
/// Keep the line as a leader in the graph, but mark the composition so the
/// checker can apply the radius-terminal relation without promoting every
/// leader to a dimension line.
pub(super) fn tag_radius_leader(b: &mut Builder, e: usize) {
    b.exprs[e]
        .evidence_ids
        .extend(evidence("radius_leader_carrier", "DC-G4-006"));
}

fn part(b: &mut Builder, cx: &Ctx, e: usize, id: &str, role: &str, ev: Vec<String>) {
    b.member(
        e,
        id,
        None,
        [0., 1.],
        role,
        member_line_type(cx.lt(id), role),
        "candidate",
        vec![],
        ev,
    );
}

fn phase(p: P, c: P) -> f64 {
    (p[1] - c[1])
        .atan2(p[0] - c[0])
        .rem_euclid(std::f64::consts::TAU)
}

/// Each angular value owns the arc pieces between its two oriented terminators.
/// The complete source arcs must fit that sector: crossing arcs need a separate
/// rendering/interval treatment and are not silently clipped here.
pub(super) fn chained_angles(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let near = parameter("angular_circle_arrow")?;
    let reach = parameter("angular_text_height")?;
    let tangent = parameter("angular_tangent_cos")?;
    let orientation = parameter("terminator_direction_cos")?;
    let mut circles: Vec<(P, f64)> = Vec::new();
    for id in cx
        .rec
        .keys()
        .filter(|id| cx.kind(id) == "ARC" && !b.used(id) && cx.annotation_style(id))
    {
        if let Some((c, r, ..)) = cx.arc(id)
            && !circles
                .iter()
                .any(|(p, rad)| dist(c, *p) < 1e-6 && (r - rad).abs() < 1e-6)
        {
            circles.push((c, r));
        }
    }
    for (c, r) in circles {
        let heads: Vec<_> = cx
            .arrows
            .iter()
            .filter(|(id, a)| {
                !b.used(id)
                    && (dist(a.tip, c) - r).abs() <= near * a.len
                    && dot(a.dir, unit(sub(a.tip, c))).abs() < tangent
            })
            .collect();
        if heads.len() < 4 {
            continue;
        }
        let mut sites: Vec<f64> = heads.iter().map(|(_, a)| phase(a.tip, c)).collect();
        sites.sort_by(f64::total_cmp);
        sites.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        let texts: Vec<_> = cx
            .texts
            .values()
            .filter(|t| {
                !b.used(&t.id)
                    && t.lex == Lex::Angle
                    && (dist(t.center, c) - r).abs() <= reach * t.height
            })
            .collect();
        let arcs: Vec<_> = cx
            .rec
            .keys()
            .filter_map(|id| {
                let (p, rad, start, sweep, _) = cx.arc(id)?;
                (cx.kind(id) == "ARC"
                    && !b.used(id)
                    && cx.annotation_style(id)
                    && dist(p, c) < 1e-6
                    && (rad - r).abs() < 1e-6)
                    .then(|| (id.clone(), start, sweep))
            })
            .collect();
        let mut sectors = Vec::new();
        for (i, start) in sites.iter().enumerate() {
            let end = sites[(i + 1) % sites.len()];
            let sweep = (end - start).rem_euclid(std::f64::consts::TAU);
            let values: Vec<_> = texts
                .iter()
                .filter(|t| {
                    let at = (phase(t.center, c) - start).rem_euclid(std::f64::consts::TAU);
                    at > 1e-6 && at < sweep - 1e-6
                })
                .collect();
            let value = if values.len() == 1 {
                values[0]
            } else {
                // If one value is in a rendered arc and one is in the gap
                // between two pieces, keep the supported value here and let
                // `taken_angle_sectors` recover the swallowed one. Two values
                // in the same rendered piece remain ambiguous.
                let in_arc: Vec<_> = values
                    .iter()
                    .filter(|t| {
                        let at = phase(t.center, c);
                        arcs.iter()
                            .any(|(_, start, sweep)| on_arc_phase(at, *start, *sweep, 1e-6))
                    })
                    .collect();
                let in_gap = values.len().saturating_sub(in_arc.len());
                if in_arc.len() == 1 && in_gap == 1 {
                    in_arc[0]
                } else {
                    continue;
                }
            };
            let pick = |at: f64, sign: f64| {
                let matches: Vec<_> = heads
                    .iter()
                    .filter(|(_, a)| {
                        let gap = (phase(a.tip, c) - at).rem_euclid(std::f64::consts::TAU);
                        let radial = unit(sub(a.tip, c));
                        gap.min(std::f64::consts::TAU - gap) < 1e-6
                            && sign * dot(a.dir, [-radial[1], radial[0]]) > orientation
                    })
                    .map(|(id, _)| (*id).clone())
                    .collect();
                (matches.len() == 1).then(|| matches[0].clone())
            };
            let (Some(first), Some(last)) = (pick(*start, -1.), pick(end, 1.)) else {
                continue;
            };
            let slack = near * cx.arrows[&first].len.max(cx.arrows[&last].len) / r;
            let pieces: Vec<String> = arcs
                .iter()
                .filter(|(_, at, len)| {
                    let off = (at - start).rem_euclid(std::f64::consts::TAU);
                    off <= sweep + slack && off + len <= sweep + slack
                })
                .map(|(id, _, _)| id.clone())
                .collect();
            if pieces.is_empty() {
                continue;
            }
            // Both ends must have actual dimension-arc support beyond the arrow fill.
            let bound = parameter("angular_end_arrow")?;
            let supported = |tip: &Arrow| {
                pieces.iter().any(|id| {
                    let (_, _, s, sw, _) = cx.arc(id).expect("selected arc");
                    [s, s + sw].iter().any(|a| {
                        dist(add(c, mul([a.cos(), a.sin()], r)), tip.tip) <= bound * tip.len
                    })
                })
            };
            if supported(&cx.arrows[&first]) && supported(&cx.arrows[&last]) {
                sectors.push(((*value).clone(), pieces, first, last));
            }
        }
        if sectors.len() < 2 {
            continue;
        }
        // No source piece can be assigned twice by this variant.
        let mut used = BTreeSet::new();
        if sectors
            .iter()
            .flat_map(|(_, arcs, a, z)| arcs.iter().chain([a, z]))
            .any(|id| !used.insert(id.clone()))
        {
            continue;
        }
        for (t, arcs, first, last) in sectors {
            let e = b.expr(
                "dimension_angle",
                &t.dec.text,
                "concentric_arc_tangent_terminators_and_text",
                "candidate",
                tx::qualifiers(&t.dec),
                evidence("chained_angle_sectors", "DC-G4-032"),
                vec!["Measured angle sides require projection evidence".into()],
            );
            part(b, cx, e, &t.id, "dimension_text", vec![]);
            for id in arcs {
                part(b, cx, e, &id, "dimension_line", vec![]);
            }
            for id in [first, last] {
                part(b, cx, e, &id, "arrowhead", vec![]);
                angular_side(cx, b, e, c, &cx.arrows[&id])?;
            }
        }
    }
    Ok(())
}

fn on_arc_phase(at: f64, start: f64, sweep: f64, tolerance: f64) -> bool {
    (at - start).rem_euclid(std::f64::consts::TAU) <= sweep + tolerance
}

fn between_phases(at: f64, start: f64, end: f64, tolerance: f64) -> bool {
    let sweep = (end - start).rem_euclid(std::f64::consts::TAU);
    sweep > tolerance
        && sweep < std::f64::consts::PI + 0.25
        && (at - start).rem_euclid(std::f64::consts::TAU) > tolerance
        && (at - start).rem_euclid(std::f64::consts::TAU) < sweep - tolerance
}

/// Recover an angle value left in a gap after a neighbouring dimension has
/// claimed the arc and its two terminators.  A value already inside a claimed
/// arc is intentionally left alone: two values in one sector are ambiguous.
pub(super) fn taken_angle_sectors(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let reach = parameter("angular_text_height")?;
    let near = parameter("angular_circle_arrow")?;
    let texts: Vec<_> = cx
        .texts
        .values()
        .filter(|t| !b.used(&t.id) && t.lex == Lex::Angle)
        .cloned()
        .collect();
    for text in texts {
        let mut selected: Option<SelectedAngle> = None;
        for (ei, expr) in b.exprs.iter().enumerate() {
            if expr.kind != "dimension_angle" {
                continue;
            }
            let member_ids: Vec<_> = b
                .members
                .iter()
                .filter(|m| m.expression_id == expr.id)
                .collect();
            let arcs: Vec<(String, P, f64, f64, f64)> = member_ids
                .iter()
                .filter(|m| m.member_role == "dimension_line")
                .filter_map(|m| {
                    let (c, r, start, sweep, _) = cx.arc(&m.source_handle)?;
                    Some((m.source_handle.clone(), c, r, start, sweep))
                })
                .collect();
            let Some((_, centre, radius, _, _)) = arcs.first() else {
                continue;
            };
            if arcs
                .iter()
                .any(|(_, c, r, _, _)| dist(*c, *centre) > 1e-3 || (r - radius).abs() > 1e-3)
                || (dist(text.center, *centre) - radius).abs() > reach * text.height
            {
                continue;
            }
            let arrows: Vec<String> = member_ids
                .iter()
                .filter(|m| m.member_role == "arrowhead")
                .filter(|m| cx.arrow(&m.source_handle).is_some())
                .map(|m| m.source_handle.clone())
                .collect();
            if arrows.len() < 2 {
                continue;
            }
            let at = phase(text.center, *centre);
            let gap = near
                * arrows
                    .iter()
                    .filter_map(|id| cx.arrow(id).map(|a| a.len))
                    .fold(0., f64::max)
                / radius.max(1e-9);
            // A value in an already claimed arc competes with that value; it is
            // not safe to choose one from source order.
            if arcs
                .iter()
                .any(|(_, _, _, start, sweep)| on_arc_phase(at, *start, *sweep, gap))
            {
                continue;
            }
            let phases: Vec<(String, f64)> = arrows
                .iter()
                .filter_map(|id| cx.arrow(id).map(|a| (id.clone(), phase(a.tip, *centre))))
                .collect();
            let mut bounds = None;
            'pairs: for (first, start) in &phases {
                for (last, end) in &phases {
                    if first == last || !between_phases(at, *start, *end, gap) {
                        continue;
                    }
                    let before = arcs.iter().any(|(_, _, _, s, sw)| {
                        let p = (*s + *sw).rem_euclid(std::f64::consts::TAU);
                        between_phases(p, *start, at, gap)
                    });
                    let after = arcs
                        .iter()
                        .any(|(_, _, _, s, _)| between_phases(*s, at, *end, gap));
                    if before && after {
                        bounds = Some((first.clone(), last.clone()));
                        break 'pairs;
                    }
                }
            }
            let Some((first, last)) = bounds else {
                continue;
            };
            let mut shared: Vec<String> = member_ids
                .iter()
                .filter(|m| m.member_role == "dimension_line")
                .map(|m| m.source_handle.clone())
                .collect();
            shared.sort();
            shared.dedup();
            let score = (dist(text.center, *centre) - radius).abs();
            if selected.as_ref().is_none_or(|old| score < old.score) {
                selected = Some(SelectedAngle {
                    owner: ei,
                    score,
                    shared,
                    arrows: vec![first, last],
                });
            }
        }
        let Some(SelectedAngle {
            owner,
            shared,
            arrows,
            ..
        }) = selected
        else {
            continue;
        };
        let ev = evidence("chained_angle_sector_shared", "DC-G4-032");
        let e = b.expr(
            "dimension_angle",
            &text.dec.text,
            "concentric_arc_tangent_terminators_and_text",
            "candidate",
            tx::qualifiers(&text.dec),
            ev.clone(),
            vec!["Angle sector is shared with a neighbouring dimension; side projection remains unknown".into()],
        );
        part(b, cx, e, &text.id, "dimension_text", ev.clone());
        for id in shared {
            if b.members
                .iter()
                .any(|m| m.expression_id == b.exprs[owner].id && m.source_handle == id)
            {
                part(b, cx, e, &id, "dimension_line", ev.clone());
            }
        }
        for id in arrows {
            part(b, cx, e, &id, "arrowhead", ev.clone());
        }
    }
    Ok(())
}

fn angular_side(cx: &Ctx, b: &mut Builder, e: usize, c: P, a: &Arrow) -> Result<()> {
    let end = parameter("side_gap_arrow")? * a.len;
    let direction = parameter("direction_sine")?;
    let lateral = parameter("angular_circle_arrow")? * a.len;
    if let Some((id, _)) = cx.lines.iter().find(|(id, (p, q))| {
        let shared = b.by_source.get(*id).is_some_and(|ms| {
            ms.iter().all(|&i| {
                let m = &b.members[i];
                m.member_role == "extension_line"
                    && b.exprs
                        .iter()
                        .any(|e| e.id == m.expression_id && e.kind == "dimension_angle")
            })
        });
        (!b.used(id) || shared)
            && (cx.annotation_style(id) || cx.lt(id) == "thin_dash_dot")
            && cross(unit(sub(*q, *p)), unit(sub(a.tip, c))).abs() < direction
            && project(a.tip, *p, *q).1 <= lateral
            && on_seg(a.tip, *p, *q, end).is_some()
    }) {
        part(
            b,
            cx,
            e,
            id,
            "extension_line",
            evidence("bounded_angular_side", "DC-G4-058"),
        );
    }
    Ok(())
}

/// A value outside an already recognised pair may be carried by a separate line
/// beyond its end. Require the actual value above that carrier, and a connection
/// through this pair's terminator. Proximity alone does not make a dimension.
pub(super) fn outside_lines(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let incidence = parameter("incidence_arrow")?;
    let direction = parameter("direction_sine")?;
    for e in 0..b.exprs.len() {
        if !matches!(
            b.exprs[e].kind.as_str(),
            "dimension_linear" | "dimension_diameter"
        ) {
            continue;
        }
        let members: Vec<_> = b
            .members
            .iter()
            .filter(|m| m.expression_id == b.exprs[e].id)
            .collect();
        let heads: Vec<_> = members
            .iter()
            .filter(|m| m.member_role == "arrowhead" && m.native_member_id.is_none())
            .filter_map(|m| cx.arrows.get(&m.source_handle))
            .collect();
        let [first, last] = heads.as_slice() else {
            continue;
        };
        let texts: Vec<_> = members
            .iter()
            .filter(|m| m.member_role == "dimension_text")
            .filter_map(|m| cx.texts.get(&m.source_handle))
            .collect();
        let axis = unit(sub(last.tip, first.tip));
        let carriers: Vec<_> = members
            .iter()
            .filter(|m| m.member_role == "dimension_line")
            .filter_map(|m| cx.lines.get(&m.source_handle))
            .collect();
        let mut candidates = Vec::new();
        for (id, (p, q)) in &cx.lines {
            if b.used(id)
                || !cx.annotation_style(id)
                || cross(unit(sub(*q, *p)), axis).abs() > direction
                || !texts.iter().any(|t| on_shoulder(t, &[*p, *q]))
            {
                continue;
            }
            for (head, other) in [(first, last), (last, first)] {
                let tol = incidence * head.len;
                let (near, far) = if dist(*p, head.tip) < dist(*q, head.tip) {
                    (*p, *q)
                } else {
                    (*q, *p)
                };
                let away = unit(sub(head.tip, other.tip));
                if dist(near, head.tip) > tol
                    || dot(sub(far, head.tip), away) <= head.len
                    || !carriers
                        .iter()
                        .any(|(a, z)| on_seg(head.tip, *a, *z, head.len).is_some())
                {
                    continue;
                }
                // A different terminator on the continuation belongs to another annotation.
                if cx.arrows.values().any(|a| {
                    dist(a.tip, head.tip) > head.len && on_seg(a.tip, near, far, tol).is_some()
                }) {
                    continue;
                }
                candidates.push(id.clone());
            }
        }
        candidates.sort();
        candidates.dedup();
        if let [id] = candidates.as_slice() {
            let ev = evidence("outside_value_continuation", "DC-G4-059");
            b.exprs[e].evidence_ids.extend(ev.clone());
            part(b, cx, e, id, "dimension_line", ev);
        }
    }
    Ok(())
}

fn uncovered(b: &Builder, source: &str) -> Vec<[f64; 2]> {
    let mut gaps = Vec::new();
    let mut cursor = 0.;
    for [a, z] in b.covered(source) {
        if a > cursor + 1e-12 {
            gaps.push([cursor, a]);
        }
        cursor = f64::max(cursor, z);
    }
    if cursor < 1. - 1e-12 {
        gaps.push([cursor, 1.]);
    }
    gaps
}

/// Root copies of native dimension lines also pass beneath the native arrow's
/// fill. Extend only their uncovered footprint, never the rest of the carrier.
pub(super) fn native_head_pieces(
    cx: &Ctx,
    b: &mut Builder,
    e: usize,
    heads: &[Arrow],
    block_lines: &[(P, P, &str)],
) -> Result<()> {
    let ratio = parameter("incidence_arrow")?;
    let direction = parameter("direction_sine")?;
    let roots: BTreeSet<_> = b
        .members
        .iter()
        .filter(|m| {
            m.expression_id == b.exprs[e].id
                && m.member_role == "dimension_line"
                && m.native_member_id.is_none()
        })
        .map(|m| m.source_handle.clone())
        .collect();
    for id in roots {
        let Some((p, q)) = cx.lines.get(&id) else {
            continue;
        };
        for head in heads {
            let tol = ratio * head.len;
            if cross(unit(sub(*q, *p)), head.dir).abs() > direction
                || project(head.tip, *p, *q).1 > tol
                || !block_lines.iter().any(|(a, z, role)| {
                    *role == "dimension_line"
                        && cross(unit(sub(*z, *a)), head.dir).abs() <= direction
                        && project(head.tip, *a, *z).1 <= tol
                        && on_seg(head.tip, *a, *z, head.len + tol).is_some()
                })
            {
                continue;
            }
            let (a, z) = (
                project(base_of(head), *p, *q).0,
                project(head.tip, *p, *q).0,
            );
            for gap in uncovered(b, &id) {
                let iv = [gap[0].max(a.min(z)).max(0.), gap[1].min(a.max(z)).min(1.)];
                if iv[1] - iv[0] <= 1e-12 {
                    continue;
                }
                let ev = evidence("native_terminator_footprint", "DC-G4-059");
                b.exprs[e].evidence_ids.extend(ev.clone());
                b.member(
                    e,
                    &id,
                    None,
                    iv,
                    "dimension_line",
                    member_line_type(cx.lt(&id), "dimension_line"),
                    "candidate",
                    vec![],
                    ev,
                );
            }
        }
    }
    Ok(())
}

/// The loose/loose encoding of a viewing-direction callout. The corresponding
/// named view must exist in the source; a capital beside a leader alone is weak
/// evidence, especially inside a datum frame or at a branch of a dimension.
pub(super) fn view_arrows(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let reach = representation::param(representation::card_ref("view_callout")?, "letter_reach")?;
    let length = parameter("view_shaft_arrow")?;
    let incidence = parameter("view_incidence_arrow")?;
    let frames = line_rectangles(cx, b);
    let letters: Vec<_> = cx
        .texts
        .values()
        .filter(|t| {
            !b.used(&t.id)
                && t.lex == Lex::Letters
                && (1..=2).contains(&t.dec.text.len())
                && t.dec.text.chars().all(|c| c.is_ascii_uppercase())
                && !frames.iter().any(|(_, bb)| inside(t.center, *bb, 0.))
                && cx.texts.values().any(|v| {
                    v.id != t.id
                        && v.dec.text.split_whitespace().collect::<String>()
                            == format!("{}向", t.dec.text)
                })
        })
        .collect();
    let mut matches = Vec::new();
    for (aid, a) in &cx.arrows {
        if b.used(aid) {
            continue;
        }
        for (lid, (p, q)) in &cx.lines {
            if b.used(lid)
                || !cx.annotation_style(lid)
                || dist(*p, *q) > length * a.len
                || cross(unit(sub(*q, *p)), a.dir).abs() > parameter("direction_sine")?
            {
                continue;
            }
            let gap = |x| dist(x, a.tip).min(dist(x, base_of(a)));
            let (near, far) = if gap(*p) <= gap(*q) {
                (*p, *q)
            } else {
                (*q, *p)
            };
            if !continues(a, near, far, incidence * a.len) {
                continue;
            }
            // A connected shoulder/cutting stroke is another annotation composition.
            if cx.lines.iter().any(|(id, (x, y))| {
                id != lid && (dist(*x, far).min(dist(*y, far)) < incidence * a.len)
            }) {
                continue;
            }
            let nearby: Vec<_> = letters
                .iter()
                .filter(|t| {
                    let t0 = project(t.center, far, a.tip).0.clamp(0., 1.);
                    dist(t.center, lerp(far, a.tip, t0)) < reach * t.height
                })
                .collect();
            if let [t] = nearby.as_slice() {
                matches.push((t.id.clone(), aid.clone(), lid.clone()));
            }
        }
    }
    for (letter, head, shaft) in &matches {
        if matches
            .iter()
            .filter(|(l, a, _)| l == letter || a == head)
            .count()
            != 1
        {
            continue;
        }
        let e = b.expr(
            "view_callout",
            &cx.texts[letter].dec.text,
            "viewing_direction_arrow_and_letter",
            "candidate",
            vec!["view_direction".into()],
            evidence("loose_view_direction", "DC-G2-072"),
            vec![],
        );
        for (id, role) in [
            (letter, "view_letter"),
            (head, "view_arrow"),
            (shaft, "view_arrow"),
        ] {
            part(b, cx, e, id, role, vec![]);
        }
    }
    Ok(())
}

/// Preserve a second source interval as an alias of a contour that already has
/// independent structural support. A whole uncovered gap must be coincident;
/// unmatched tails and close parallel lines retain their own unknown status.
pub(super) fn contour_aliases(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let tolerance = parameter("coincidence_roundoff_height")?;
    // Never loosen the previous exact-coincidence bound at large drawing scale.
    let distance_tolerance = tolerance * cx.scale.min(1.);
    let contours: Vec<_> = b
        .members
        .iter()
        .filter(|m| m.member_role == "contour_segment" && m.line_type == "thick_continuous")
        .filter_map(|m| {
            let (a, z) = cx.lines.get(&m.source_handle)?;
            let e = b.exprs.iter().position(|e| e.id == m.expression_id)?;
            Some((
                e,
                m.source_handle.clone(),
                lerp(*a, *z, m.interval[0]),
                lerp(*a, *z, m.interval[1]),
            ))
        })
        .collect();
    for (source, (a, z)) in &cx.lines {
        for gap in uncovered(b, source) {
            let (p, q) = (lerp(*a, *z, gap[0]), lerp(*a, *z, gap[1]));
            let owners: Vec<_> = contours
                .iter()
                .filter(|(_, id, x, y)| {
                    id != source
                        && cross(unit(sub(*y, *x)), unit(sub(q, p))).abs() <= tolerance
                        && on_seg(p, *x, *y, distance_tolerance).is_some()
                        && on_seg(q, *x, *y, distance_tolerance).is_some()
                })
                .collect();
            let Some(first) = owners.first() else {
                continue;
            };
            // Different independently recognised contours need a view/identity decision.
            if !owners.iter().all(|owner| owner.0 == first.0) {
                continue;
            }
            let ev = evidence("coincident_contour_interval", "DC-G1-055");
            b.exprs[first.0].evidence_ids.extend(ev.clone());
            let mut member_ev = ev;
            member_ev.push(format!("coincident_with:{}", first.1));
            b.member(
                first.0,
                source,
                None,
                gap,
                "coincident_source",
                cx.lt(source),
                "candidate",
                vec![],
                member_ev,
            );
        }
    }
    Ok(())
}
