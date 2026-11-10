//! Extension lines against the outline they start from (card `dimension_line_parts`, DC-G4-058;
//! GB/T 4458.4-2003 4.1, 4.2).
//!
//! An extension line is a thin line drawn from the outline (or an axis) to a little past its
//! dimension line. Each of the two runs from its own origin: when the dimension line lies
//! between the origins the two point in opposite directions. The outline may itself serve as
//! extension line, and the outline an extension line continues is the measured feature; such an
//! outline is lent to the dimension (`extension_origin`), keeps its own line type and stays
//! outline. Pieces drawn along one carrier belong together only up to the first outline piece.
use super::line_evidence::Widths;
use super::*;

const CARD: &str = "dimension_line_parts";
const VARIANT: &str = "outline_as_extension_line";
/// Evidence of a line taken on an extension line's carrier that runs on past the dimension line
/// farther than an extension line does: more than an extension line (a stacked dimension's, or
/// the outline itself).
pub(super) const RUNS_ON: &str = "runs_past_the_overshoot";
/// Evidence of a line taken on an extension line's carrier that lies wholly behind the origin:
/// what the extension line starts from, or a piece drawn before the definition point.
pub(super) const BEHIND: &str = "behind_the_origin";

/// The source serves some dimension the way an extension line is drawn: from the origin to a
/// little past the dimension line. A line that only continues behind an origin, or runs on past
/// the overshoot, or is lent as outline, is not decided by that use.
pub(super) fn drawn_as_extension_line(b: &Builder, h: &str) -> bool {
    b.by_source.get(h).is_some_and(|ms| {
        ms.iter().any(|&m| {
            let x = &b.members[m];
            x.member_role != "extension_origin"
                && !x.evidence_ids.iter().any(|e| e == RUNS_ON || e == BEHIND)
        })
    })
}

/// A root line under a native extension line: origin index, line, interval, relations, rule.
type Use = (usize, String, [f64; 2], Vec<String>, String);

/// Pieces of an extension line on the carrier through `o` along unit `v` (within `ptol`) that
/// overlap or touch the interval `iv`: thin annotation lines grow the chain transitively; an
/// outline piece (thick by the drawing's own evidence) the chain reaches is taken and ends the
/// growth there, so nothing beyond it is reached through it.
#[allow(clippy::too_many_arguments)]
pub(super) fn chain(
    cx: &Ctx,
    b: &Builder,
    w: &Widths,
    o: P,
    v: P,
    iv: [f64; 2],
    ptol: f64,
    skip: &BTreeSet<String>,
) -> Vec<String> {
    let gap = 1e-3 * cx.scale.max(1.);
    let ov = add(o, v);
    // candidates on the carrier: (id, low, high, outline)
    let on: Vec<(&String, f64, f64, bool)> = cx
        .lines
        .iter()
        .filter(|(id, (a, c))| {
            // Drafted pieces may be slightly skewed: one end on the carrier, the other within a
            // small angular deviation.
            let skew = 0.01;
            let (da, dc) = (project(*a, o, ov).1, project(*c, o, ov).1);
            !skip.contains(*id)
                && !exclusive(b, id)
                && cx.annotation_style(id)
                && dist(*a, *c) >= 1e-9
                && cross(unit(sub(*c, *a)), v).abs() <= skew
                && da.min(dc) <= ptol
                && da.max(dc) <= ptol + skew * dist(*a, *c)
        })
        .map(|(id, (a, c))| {
            let (ta, tc) = (dot(sub(*a, o), v), dot(sub(*c, o), v));
            (id, ta.min(tc), ta.max(tc), w.thick(cx, id))
        })
        .collect();
    let mut cover = iv;
    let mut out: Vec<String> = Vec::new();
    loop {
        let mut grew = false;
        for (id, lo, hi, outline) in &on {
            if *outline || out.contains(id) || *hi < cover[0] - gap || *lo > cover[1] + gap {
                continue;
            }
            out.push((*id).clone());
            cover = [cover[0].min(*lo), cover[1].max(*hi)];
            grew = true;
        }
        if !grew {
            break;
        }
    }
    for (id, lo, hi, outline) in &on {
        if *outline && *hi >= cover[0] - gap && *lo <= cover[1] + gap {
            out.push((*id).clone());
        }
    }
    out.sort();
    out
}

/// Root lines on the extension lines of a native linear dimension: from each origin through the
/// dimension line (direction `u`, through `p10`) with its overshoot, and their collinear pieces.
/// Each extension line ends a short overshoot past the dimension line on the side away from its
/// own origin; an origin lying on the dimension line takes the side of the other one.
#[allow(clippy::too_many_arguments)]
pub(super) fn native(
    cx: &Ctx,
    b: &mut Builder,
    e: usize,
    h: &str,
    u: P,
    p10: P,
    origins: &[P],
    alen: f64,
    tol: f64,
) {
    let over = representation::card_ref(CARD)
        .and_then(|c| representation::param(c, "overshoot_max"))
        .unwrap_or(2.)
        * alen;
    let w = Widths::of(cx);
    let v = [-u[1], u[0]];
    let feet: Vec<(P, f64)> = origins
        .iter()
        .map(|o| {
            let t = add(p10, mul(u, dot(sub(*o, p10), u)));
            (t, dot(sub(*o, t), v))
        })
        .collect();
    let common = feet
        .iter()
        .map(|x| x.1)
        .find(|so| so.abs() > tol)
        .map(f64::signum);
    // far direction of each origin, and how a line on its carrier lies against the span from
    // the origin to the overshoot
    let far: Vec<Option<P>> = feet
        .iter()
        .map(|(_, so)| {
            let side = if so.abs() > tol {
                Some(so.signum())
            } else {
                common
            };
            side.map(|sg| mul(v, -sg))
        })
        .collect();
    let marks = |k: usize, a: P, c: P| -> Vec<String> {
        let (t, so) = feet[k];
        let Some(f) = far[k] else {
            return vec![];
        };
        let (sa, sc) = (dot(sub(a, t), f), dot(sub(c, t), f));
        let mut out = Vec::new();
        if sa.max(sc) > over + tol {
            out.push(RUNS_ON.to_string());
        }
        if so.abs() > tol && sa.max(sc) <= -so.abs() + tol {
            out.push(BEHIND.to_string());
        }
        out
    };
    // root lines overlapping the span from an origin to its dimension line
    let uses: Vec<Use> = cx
        .src
        .uses
        .iter()
        .filter(|x| x.consumer_id == h)
        .filter_map(|x| {
            let k = match x.rule.as_str() {
                "linear_extension_13_support" => 0,
                "linear_extension_14_support" => 1,
                _ => return None,
            };
            Some((
                k,
                x.occurrence_id.clone(),
                [x.interval[0], x.interval[1]],
                x.relation_ids.clone(),
                x.rule.clone(),
            ))
        })
        .collect();
    for (k, lid, iv, relations, rule) in uses {
        let mut ev = vec![format!("rule:{rule}")];
        if let (Some((a, c)), true) = (cx.lines.get(&lid), k < feet.len()) {
            ev.extend(marks(k, *a, *c));
        }
        b.member(
            e,
            &lid,
            None,
            iv,
            "extension_line",
            member_line_type(cx.lt(&lid), "extension_line"),
            "candidate",
            relations,
            ev,
        );
    }
    for (k, (t, so)) in feet.iter().enumerate() {
        let iv = [so.min(0.) - over, so.max(0.) + over];
        for lid in chain(cx, b, &w, *t, v, iv, tol, &BTreeSet::new()) {
            let (a, c) = cx.lines[&lid];
            // an outline piece serves this extension line only where it reaches the span from the
            // origin to the dimension line: the measured feature reaches the origin, the outline
            // the extension line is drawn over or the terminator stands on lies in the span; an
            // outline only behind the origin or beyond the dimension line is another feature
            let (ta, tc) = (dot(sub(a, *t), v), dot(sub(c, *t), v));
            if w.thick(cx, &lid) && (ta.max(tc) < so.min(0.) - tol || ta.min(tc) > so.max(0.) + tol)
            {
                continue;
            }
            let clipped = match far[k] {
                Some(f) => clip_along(a, c, *t, f, over, tol),
                None => Some([0., 1.]),
            };
            if let Some(civ) = clipped {
                let mut ev = vec![format!("on_native_extension_line:{h}")];
                ev.extend(marks(k, a, c));
                b.member(
                    e,
                    &lid,
                    None,
                    civ,
                    "extension_line",
                    member_line_type(cx.lt(&lid), "extension_line"),
                    "candidate",
                    vec![],
                    ev,
                );
            }
        }
    }
}

/// After the outline is read: a line a dimension took as extension line that is outline (it has a
/// contour membership) or is drawn thick by the drawing's own evidence is not an extension line.
/// It is the outline the dimension measures or uses as its extension line: one shared membership
/// per dimension with the role `extension_origin`, in the line's own type (GB/T 4458.4-2003 4.1:
/// the outline may serve as extension line; GB/T 14665-2012 4.3: where lines coincide the visible
/// outline is the one drawn).
pub(super) fn lend_outline(cx: &Ctx, b: &mut Builder) -> Result<()> {
    representation::card_ref(CARD)?;
    let w = Widths::of(cx);
    let visible: BTreeSet<&str> = b
        .exprs
        .iter()
        .filter(|e| e.kind == "visible_contour")
        .map(|e| e.id.as_str())
        .collect();
    // outline sources: line type and extent of their contour memberships
    let mut contour: BTreeMap<String, (String, [f64; 2])> = BTreeMap::new();
    for m in &b.members {
        if m.member_role == "contour_segment" && visible.contains(m.expression_id.as_str()) {
            let x = contour
                .entry(m.source_handle.clone())
                .or_insert_with(|| (m.line_type.clone(), [1., 0.]));
            x.1 = [x.1[0].min(m.interval[0]), x.1[1].max(m.interval[1])];
        }
    }
    let mut groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, m) in b.members.iter().enumerate() {
        let h = &m.source_handle;
        if m.member_role == "extension_line"
            && m.native_member_id.is_none()
            && cx.lines.contains_key(h)
            && cx.style[h].pattern == Pattern::Continuous
            && (contour.contains_key(h) || w.thick(cx, h))
        {
            groups
                .entry((m.expression_id.clone(), h.clone()))
                .or_default()
                .push(i);
        }
    }
    // an existing shared membership of the same line in the same dimension joins the group
    for (i, m) in b.members.iter().enumerate() {
        if m.member_role == "extension_origin"
            && m.native_member_id.is_none()
            && let Some(g) = groups.get_mut(&(m.expression_id.clone(), m.source_handle.clone()))
        {
            g.push(i);
        }
    }
    let mut named: BTreeSet<String> = BTreeSet::new();
    for ((eid, h), mut ids) in groups {
        ids.sort();
        // the outline lent is the part read as outline together with the part the dimension
        // uses; a piece that only lies behind the origin adds nothing of its own
        let behind = |i: usize| b.members[i].evidence_ids.iter().any(|x| x == BEHIND);
        let mut hull = contour.get(&h).map_or([1., 0.], |x| x.1);
        for &i in ids
            .iter()
            .filter(|&&i| !behind(i) || !contour.contains_key(&h))
        {
            let iv = &b.members[i].interval;
            hull = [hull[0].min(iv[0]), hull[1].max(iv[1])];
        }
        let interval = hull.to_vec();
        let line_type = contour
            .get(&h)
            .map(|x| x.0.clone())
            .unwrap_or_else(|| member_line_type(cx.lt(&h), "contour_segment").to_string());
        let mut evidence: Vec<String> = Vec::new();
        let mut relations: Vec<String> = Vec::new();
        for &i in &ids {
            for x in &b.members[i].evidence_ids {
                if !evidence.contains(x) {
                    evidence.push(x.clone());
                }
            }
            for x in &b.members[i].relation_ids {
                if !relations.contains(x) {
                    relations.push(x.clone());
                }
            }
        }
        for x in [format!("card:{CARD}"), format!("variant:{VARIANT}")] {
            if !evidence.contains(&x) {
                evidence.push(x);
            }
        }
        // the memberships become one: identical members are kept once when the graph is finished
        for &i in &ids {
            let m = &mut b.members[i];
            m.member_role = "extension_origin".into();
            m.interval = interval.clone();
            m.line_type = line_type.clone();
            m.evidence_ids = evidence.clone();
            m.relation_ids = relations.clone();
        }
        named.insert(eid);
    }
    for e in b.exprs.iter_mut().filter(|e| named.contains(&e.id)) {
        for x in [
            format!("card:{CARD}"),
            format!("variant:{VARIANT}"),
            "concept:DC-G4-058".to_string(),
        ] {
            if !e.evidence_ids.contains(&x) {
                e.evidence_ids.push(x);
            }
        }
    }
    Ok(())
}

/// Dimensioning a smooth transition (card `dimension_line_parts`, variant
/// `outline_extended_to_virtual_corner`; DC-G4-005, GB/T 4458.4-2003 4.1.4 and figure 4): the
/// outlines on both sides of a fillet or chamfer are continued by thin lines to their
/// intersection, and the extension lines start there. A thin line that continues an outline line
/// from its end (or from a point on it) to the start of a dimension's extension line is that
/// dimension's construction (role `extension_line`), never outline; when several dimensions start
/// at that corner it serves each. An outline stub drawn under the thin piece is the outline drawn
/// past the transition into the construction: it is lent to the same dimensions.
pub(super) fn virtual_corners(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = representation::card_ref(CARD)?;
    let tol = representation::param(c, "coincident_tolerance")? * cx.scale.max(1e-6);
    let reach = representation::param(c, "virtual_corner_reach")? * cx.scale.max(1e-6);
    let w = Widths::of(cx);
    let dims: BTreeSet<&str> = b
        .exprs
        .iter()
        .filter(|e| e.kind.starts_with("dimension_") || e.kind == "thread_note")
        .map(|e| e.id.as_str())
        .collect();
    let index: BTreeMap<&str, usize> = b
        .exprs
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    // where extension lines start: the ends of extension-line sources and native origins
    let mut starts: Vec<(P, usize, P)> = Vec::new();
    for m in &b.members {
        if !dims.contains(m.expression_id.as_str()) {
            continue;
        }
        let e = index[m.expression_id.as_str()];
        if m.member_role == "extension_line"
            && m.native_member_id.is_none()
            && let Some((a, z)) = cx.lines.get(&m.source_handle)
        {
            starts.push((*a, e, unit(sub(*z, *a))));
            starts.push((*z, e, unit(sub(*a, *z))));
        }
        if m.member_role == "native_dimension"
            && let Some(r) = cx.rec.get(&m.source_handle)
        {
            for code in [13, 14] {
                if let Ok(o) = r.xy(code) {
                    starts.push((o, e, [0., 0.]));
                }
            }
        }
    }
    // outline lines: drawn thick by the layer convention or by the drawing's own evidence
    let outline: Vec<(&String, P, P)> = cx
        .lines
        .iter()
        .filter(|(h, _)| (cx.structural_style(h) || w.thick(cx, h)) && !exclusive(b, h))
        .map(|(h, (a, z))| (h, *a, *z))
        .collect();
    let mut add: Vec<(usize, String, [f64; 2], &'static str, String)> = Vec::new();
    for (h, (a, z)) in &cx.lines {
        let len = dist(*a, *z);
        if len <= tol
            || len > reach
            || b.used(h)
            || not_plotted::source(cx, h)
            || cx.style[h].pattern != Pattern::Continuous
            || !line_evidence::thin_line(cx, &w, h)
        {
            continue;
        }
        let u = unit(sub(*z, *a));
        for (v, p) in [(*a, *z), (*z, *a)] {
            let at: BTreeSet<usize> = starts
                .iter()
                .filter(|(s, _, _)| dist(*s, v) <= tol)
                .map(|x| x.1)
                .collect();
            if at.is_empty() {
                continue;
            }
            // the outline the thin piece continues: collinear, reaching its other end
            let continued: Vec<&(&String, P, P)> = outline
                .iter()
                .filter(|(_, oa, oz)| {
                    cross(unit(sub(*oz, *oa)), u).abs() <= 0.01
                        && project(p, *oa, *oz).1 <= tol
                        && on_seg(p, *oa, *oz, tol).is_some()
                })
                .collect();
            if continued.is_empty() {
                continue;
            }
            for &e in &at {
                add.push((
                    e,
                    h.clone(),
                    [0., 1.],
                    "extension_line",
                    format!("continues:{}", continued[0].0),
                ));
                // an outline stub lying under the thin piece
                for (o, oa, oz) in &continued {
                    let (t0, t1) = (project(*a, *oa, *oz).0, project(*z, *oa, *oz).0);
                    let iv = [t0.min(t1).max(0.), t0.max(t1).min(1.)];
                    if (iv[1] - iv[0]) * dist(*oa, *oz) > tol {
                        add.push((
                            e,
                            (*o).clone(),
                            iv,
                            "extension_origin",
                            format!("coincident_with:{h}"),
                        ));
                    }
                }
            }
            break;
        }
    }
    let mut named: BTreeSet<usize> = BTreeSet::new();
    for (e, h, iv, role, why) in add {
        let line_type = if role == "extension_origin" {
            member_line_type(cx.lt(&h), "contour_segment")
        } else {
            member_line_type(cx.lt(&h), "extension_line")
        };
        b.member(
            e,
            &h,
            None,
            iv,
            role,
            line_type,
            "candidate",
            vec![],
            vec![
                format!("card:{CARD}"),
                "variant:outline_extended_to_virtual_corner".into(),
                why,
            ],
        );
        named.insert(e);
    }
    for e in named {
        for x in [
            format!("card:{CARD}"),
            "variant:outline_extended_to_virtual_corner".to_string(),
            "concept:DC-G4-005".to_string(),
        ] {
            if !b.exprs[e].evidence_ids.contains(&x) {
                b.exprs[e].evidence_ids.push(x);
            }
        }
    }
    Ok(())
}
