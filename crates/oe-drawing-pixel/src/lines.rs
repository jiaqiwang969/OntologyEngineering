//! Dimension lines and leaders followed on the ink. The width profile along a line (the ink extent
//! across it at every pixel) is split into runs: paper, line (the stroke's own width), wide (an
//! arrowhead or a bump) and crossing (another line crossing it). A dimension line's extent is
//! followed run by run from its proposal outward: lines continue, text breaks and crossings are
//! passed, and a wedge entered at its wide end (its base) is this dimension's own arrowhead, whose tip
//! is the end of the line; a wedge pointing back at a crossing belongs to the neighbour of a chain (or
//! is an outside arrow of this dimension). Filled arrowheads at line ends, robust stroke widths and
//! axis snapping complete the module.

use std::collections::BTreeMap;

use crate::Ink;
use crate::geom::{P, add, clip_line_to_box, dist, dot, median, mul, perp, sub, unit};
use crate::refine::perp_extent;

/// Directions within this many degrees of 0°, 90°, 180° or 270° are snapped to the axis.
pub(crate) const AXIS_SNAP_DEG: f64 = 0.3;

/// Kind of a run of the width profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RunKind {
    Gap,
    Line,
    Wide,
    Cross,
}

/// A run of equal kind along the line, `t0..=t1` in 1-px samples, with the widths sampled.
#[derive(Clone, Debug)]
pub(crate) struct Run {
    pub kind: RunKind,
    pub t0: f64,
    pub t1: f64,
    pub widths: Vec<f64>,
}

impl Run {
    fn len(&self) -> f64 {
        self.t1 - self.t0 + 1.0
    }
    fn near(&self, dir: f64) -> f64 {
        if dir > 0.0 { self.t0 } else { self.t1 }
    }
    fn far(&self, dir: f64) -> f64 {
        if dir > 0.0 { self.t1 } else { self.t0 }
    }
    fn centre(&self) -> f64 {
        0.5 * (self.t0 + self.t1)
    }
    fn max_width(&self) -> f64 {
        self.widths.iter().copied().fold(0.0, f64::max)
    }
}

/// Upper width of a line sample and lower width of a crossing sample, for stroke width `sw`.
fn thresholds(sw: f64) -> (f64, f64) {
    (sw.max(1.0) + 2.5, (6.0 * sw).max(34.0))
}

/// Width across the line at `c`: the perpendicular ink extent through the axis pixel, or through a
/// pixel one step to either side when the axis pixel is paper; 0 when all three are paper.
fn width_at(ink: &Ink, c: P, n: P) -> f64 {
    let e = perp_extent(ink, c, n);
    if e > 0.0 {
        return e;
    }
    perp_extent(ink, add(c, n), n).max(perp_extent(ink, sub(c, n), n))
}

fn push(out: &mut Vec<Run>, kind: RunKind, t: f64, w: f64) {
    match out.last_mut() {
        Some(r) if r.kind == kind => {
            r.t1 = t;
            r.widths.push(w);
        }
        _ => out.push(Run {
            kind,
            t0: t,
            t1: t,
            widths: vec![w],
        }),
    }
}

fn merge(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());
    for r in runs {
        match out.last_mut() {
            Some(last) if last.kind == r.kind => {
                last.t1 = r.t1;
                last.widths.extend(r.widths);
            }
            _ => out.push(r),
        }
    }
    out
}

/// Flicker removal: paper runs of at most 2 px between ink runs join their neighbours (as line when
/// either neighbour is line, else as the previous kind); wide runs shorter than 5 px are part of a
/// crossing when they touch one (a line crossing at an angle) and line otherwise (a bump or a glyph
/// touching the line); one line sample inside a wedge is part of the wedge.
fn clean(runs: Vec<Run>) -> Vec<Run> {
    let mut v = merge(runs);
    let n = v.len();
    for i in 0..n {
        let prev = (i > 0).then(|| v[i - 1].kind);
        let next = (i + 1 < n).then(|| v[i + 1].kind);
        let len = v[i].len();
        match v[i].kind {
            RunKind::Gap if len <= 2.0 => {
                if let (Some(a), Some(b)) = (prev, next)
                    && a != RunKind::Gap
                    && b != RunKind::Gap
                {
                    v[i].kind = if a == RunKind::Line || b == RunKind::Line { RunKind::Line } else { a };
                }
            }
            RunKind::Wide if len < 5.0 => {
                v[i].kind = if prev == Some(RunKind::Cross) || next == Some(RunKind::Cross) {
                    RunKind::Cross
                } else {
                    RunKind::Line
                };
            }
            RunKind::Line if len <= 1.0 && prev == Some(RunKind::Wide) && next == Some(RunKind::Wide) => {
                v[i].kind = RunKind::Wide;
            }
            _ => {}
        }
    }
    merge(v)
}

/// The width profile of the line `origin + t·d` for `t` in `[t_from, t_to]` (1-px steps) as runs.
pub(crate) fn runs(ink: &Ink, origin: P, d: P, t_from: f64, t_to: f64, sw: f64) -> Vec<Run> {
    let n = perp(d);
    let (line_max, cross_min) = thresholds(sw);
    let mut out: Vec<Run> = Vec::new();
    let mut t = t_from;
    while t <= t_to + 1e-9 {
        let w = width_at(ink, add(origin, mul(d, t)), n);
        let kind = if w <= 0.0 {
            RunKind::Gap
        } else if w <= line_max {
            RunKind::Line
        } else if w >= cross_min {
            RunKind::Cross
        } else {
            RunKind::Wide
        };
        push(&mut out, kind, t, w);
        t += 1.0;
    }
    clean(out)
}

/// Orientation of a wedge as met walking in direction `dir`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wedge {
    /// The wide end comes first: an arrow pointing away from where the walk comes from.
    Base,
    /// The narrow end comes first: an arrow pointing back at where the walk comes from.
    Tip,
    Blob,
}

fn orientation(r: &Run, dir: f64) -> Wedge {
    let ws: Vec<f64> = if dir > 0.0 {
        r.widths.clone()
    } else {
        r.widths.iter().rev().copied().collect()
    };
    let max = ws.iter().copied().fold(0.0, f64::max);
    if ws.is_empty() || max <= 0.0 {
        return Wedge::Blob;
    }
    let k = (ws.len() / 4).clamp(1, 3);
    let head = ws[..k].iter().sum::<f64>() / k as f64;
    let tail = ws[ws.len() - k..].iter().sum::<f64>() / k as f64;
    if head >= 0.7 * max && tail <= 0.65 * max {
        Wedge::Base
    } else if head <= 0.65 * max && tail >= 0.7 * max {
        Wedge::Tip
    } else {
        Wedge::Blob
    }
}

/// A wide run long and wide enough to be a filled arrowhead on a line of width `sw`.
fn is_arrow(r: &Run, sw: f64) -> bool {
    r.kind == RunKind::Wide && r.len() >= 6.0 && r.max_width() >= (2.5 * sw).max(6.0)
}

/// The narrow tip of an arrowhead reads as line width: for a straight wedge whose wide part is `L`
/// long and `W` wide at the base, the part narrower than the line threshold is about
/// `L · line_max / (W − line_max)` long; a line run up to 1.3 times that (plus 2 px) next to the wedge
/// is part of the head.
fn tip_allowance(wedge: &Run, sw: f64) -> f64 {
    let (line_max, _) = thresholds(sw);
    (1.3 * wedge.len() * line_max / (wedge.max_width() - line_max).max(1.0) + 2.0).clamp(4.0, 40.0)
}

/// What the follower knows about the dimension it follows.
pub(crate) struct LineContext<'a> {
    /// Stroke width of the line, pixels.
    pub sw: f64,
    /// Longest paper run bridged outside the proposal (one text height).
    pub max_gap: f64,
    /// The proposal's extent in `t`; paper runs inside it are bridged whatever their length.
    pub proposal: (f64, f64),
    /// Where the dimension's own extension lines cross the line, in `t`.
    pub own_crossings: &'a [f64],
}

/// One end of a followed line: its `t`, how it ended and its arrowhead (`tip t`, `base t`) when the
/// line ends in one of its own.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LineEnd {
    pub t: f64,
    pub how: &'static str,
    pub arrow: Option<(f64, f64)>,
}

/// Follow the runs from `anchor` in direction `dir` (+1 towards larger `t`, −1 towards smaller).
/// A paper run outside the proposal (longer than three stroke widths) is bridged only on trial: a line
/// that then ends on paper, at the end of the profile or at a crossing, without reaching an arrowhead or
/// an extension line, ends before that run (`ink_end_before_gap`), so the line never runs on through a
/// text or into another stroke beyond its own end.
pub(crate) fn follow(runs: &[Run], anchor: usize, dir: i64, ctx: &LineContext) -> LineEnd {
    let step = |i: usize, k: i64| -> Option<usize> {
        let j = i as i64 + k * dir;
        (j >= 0 && (j as usize) < runs.len()).then_some(j as usize)
    };
    let dirf = dir as f64;
    let own = |t: f64| ctx.own_crossings.iter().any(|c| (c - t).abs() <= 4.0);
    let mut i = anchor;
    let mut end = runs[i].far(dirf);
    let mut before_jump: Option<f64> = None;
    let open_end = |how: &'static str, t: f64, before: Option<f64>| match before {
        Some(b) => LineEnd { t: b, how: "ink_end_before_gap", arrow: None },
        None => LineEnd { t, how, arrow: None },
    };
    loop {
        let Some(j) = step(i, 1) else {
            return open_end("profile_end", end, before_jump);
        };
        let r = &runs[j];
        match r.kind {
            RunKind::Line => {
                end = r.far(dirf);
                i = j;
            }
            RunKind::Gap => {
                let inside = r.t0 >= ctx.proposal.0 - 2.0 && r.t1 <= ctx.proposal.1 + 2.0;
                let next_ink = step(j, 1).is_some_and(|k| matches!(runs[k].kind, RunKind::Line | RunKind::Wide));
                if (r.len() <= ctx.max_gap || inside) && next_ink {
                    if !inside && r.len() > (3.0 * ctx.sw).max(8.0) && before_jump.is_none() {
                        before_jump = Some(end);
                    }
                    i = j;
                } else {
                    return open_end("ink_end", end, before_jump);
                }
            }
            RunKind::Wide => {
                if is_arrow(r, ctx.sw) && orientation(r, dirf) == Wedge::Base {
                    // This dimension's own arrowhead pointing outward: the base is where the line meets it.
                    // Its narrow tip may read as a short line run; a longer line after it means the wedge
                    // was a bulge on a line that goes on.
                    let base = r.near(dirf) - 0.5 * dirf;
                    let (mut k, mut tip_end) = (j, r.far(dirf));
                    let mut bulge = false;
                    if let Some(n1) = step(j, 1)
                        && runs[n1].kind == RunKind::Line
                    {
                        if runs[n1].len() <= tip_allowance(r, ctx.sw) {
                            k = n1;
                            tip_end = runs[n1].far(dirf);
                        } else {
                            bulge = true;
                        }
                    }
                    if !bulge {
                        return match step(k, 1).map(|m| &runs[m]) {
                            Some(n) if n.kind == RunKind::Cross => LineEnd {
                                t: n.centre(),
                                how: "tip_at_crossing",
                                arrow: Some((n.centre(), base)),
                            },
                            _ => LineEnd {
                                t: tip_end,
                                how: "tip",
                                arrow: Some((tip_end, base)),
                            },
                        };
                    }
                }
                end = r.far(dirf);
                i = j;
            }
            RunKind::Cross => {
                let centre = r.centre();
                let next = step(j, 1).map(|k| (k, &runs[k]));
                // The wedge after the crossing, past a short line run that is its narrow tip.
                let wedge = next.and_then(|(k, n)| {
                    if n.kind == RunKind::Line {
                        step(k, 1)
                            .map(|m| (m, &runs[m]))
                            .filter(|(_, w)| w.kind == RunKind::Wide && n.len() <= tip_allowance(w, ctx.sw))
                    } else {
                        Some((k, n))
                    }
                });
                if let Some((k, n)) = wedge
                    && is_arrow(n, ctx.sw)
                    && orientation(n, dirf) == Wedge::Tip
                {
                    // A wedge pointing back at the crossing: an outside arrow of this dimension (at its own
                    // extension line, with no tail or a short one ending on paper or on another line, not
                    // in an arrowhead) or the neighbour's arrow of a chain.
                    let tail = step(k, 1).map(|m| &runs[m]);
                    let tail_len = tail.filter(|t| t.kind == RunKind::Line).map_or(0.0, |t| t.len());
                    let after = step(k, 2).map(|m| runs[m].kind);
                    let short_tail = match tail.map(|t| t.kind) {
                        Some(RunKind::Line) => tail_len <= 2.5 * n.len() && after.is_none_or(|a| matches!(a, RunKind::Gap | RunKind::Cross)),
                        Some(RunKind::Gap) | None => true,
                        _ => false,
                    };
                    if own(centre) && short_tail {
                        let far_t = match tail {
                            Some(t) if t.kind == RunKind::Line => t.far(dirf),
                            _ => n.far(dirf),
                        };
                        return LineEnd {
                            t: far_t,
                            how: "outside_arrow",
                            arrow: Some((centre, n.far(dirf) + 0.5 * dirf)),
                        };
                    }
                    return LineEnd { t: centre, how: "shared_extension", arrow: None };
                }
                if own(centre) {
                    return LineEnd { t: centre, how: "own_extension", arrow: None };
                }
                match next {
                    Some((_, n)) if matches!(n.kind, RunKind::Line | RunKind::Wide) => {
                        end = centre;
                        i = j;
                    }
                    _ => return open_end("crossing_end", centre, before_jump),
                }
            }
        }
    }
}

/// A dimension line followed on the ink: its carrier (`origin`, unit `d`), both ends, and the parts
/// running on beyond an own extension line at an end (out to a text placed outside), as `t` ranges
/// from the crossing outward.
#[derive(Clone, Debug)]
pub(crate) struct Extent {
    pub origin: P,
    pub d: P,
    pub start: LineEnd,
    pub end: LineEnd,
    pub outer: Vec<(f64, f64)>,
}

impl Extent {
    pub fn at(&self, t: f64) -> P {
        add(self.origin, mul(self.d, t))
    }
}

/// Follow a dimension line on the ink. `fitted` is the line fitted to the proposal (its carrier is
/// kept), `proposal` the observed endpoints, `own_extensions` the dimension's refined extension lines.
/// The profile reaches as far beyond the proposal as the proposal is long (at least 300 px, at most
/// 2500 px). The line is anchored between its own extension lines when the proposal spans them (the
/// line run there nearest their middle, not an arrow's narrow tip: the arrows lie inside even when the
/// line runs out to a text beyond an extension line), else on the line run nearest the proposal's middle; a part running on beyond an own
/// extension line where the proposal goes on is followed as an outer part. `None` when no line run lies
/// near the proposal's middle.
pub(crate) fn dimension_extent(
    ink: &Ink,
    fitted: (P, P),
    proposal: (P, P),
    sw: f64,
    max_gap: f64,
    own_extensions: &[(P, P)],
) -> Option<Extent> {
    let d = unit(sub(fitted.1, fitted.0))?;
    let origin = fitted.0;
    let (a, b) = (dot(sub(proposal.0, origin), d), dot(sub(proposal.1, origin), d));
    let (lo, hi) = (a.min(b).min(0.0), a.max(b).max(dist(fitted.0, fitted.1)));
    let reach = (hi - lo).clamp(300.0, 2500.0);
    let (c0, c1) = clip_line_to_box(origin, d, 1.0, 1.0, ink.width as f64 - 2.0, ink.height as f64 - 2.0)?;
    let (from, to) = ((lo - reach).max(c0), (hi + reach).min(c1));
    if to - from < 4.0 {
        return None;
    }
    let rs = runs(ink, origin, d, from.ceil(), to.floor(), sw);
    let own: Vec<f64> = own_extensions
        .iter()
        .filter_map(|&(p, q)| {
            let e = sub(q, p);
            let den = d.0 * e.1 - d.1 * e.0;
            if den.abs() < 1e-9 {
                return None;
            }
            let w = sub(p, origin);
            let t = (w.0 * e.1 - w.1 * e.0) / den;
            let u = (w.0 * d.1 - w.1 * d.0) / den;
            (-0.2..=1.2).contains(&u).then_some(t)
        })
        .collect();
    let (plo, phi) = (a.min(b), a.max(b));
    let mid = 0.5 * (plo + phi);
    let mut own_sorted = own.clone();
    own_sorted.sort_by(|x, y| x.total_cmp(y));
    let interior = match (own_sorted.first(), own_sorted.last()) {
        (Some(&ta), Some(&tb)) if tb - ta >= 8.0 && (phi.min(tb) - plo.max(ta)).max(0.0) >= 0.6 * (tb - ta) => Some((ta, tb)),
        _ => None,
    };
    let inner_anchor = interior.and_then(|(ta, tb)| {
        let middle = 0.5 * (ta + tb);
        rs.iter()
            .enumerate()
            .filter(|(_, r)| r.kind == RunKind::Line && r.len() >= 3.0 && r.t0 >= ta + 1.0 && r.t1 <= tb - 1.0)
            .min_by(|(_, x), (_, y)| (middle - middle.clamp(x.t0, x.t1)).abs().total_cmp(&(middle - middle.clamp(y.t0, y.t1)).abs()))
            .map(|(i, _)| i)
    });
    let anchor = match inner_anchor {
        Some(i) => i,
        None => {
            let i = rs
                .iter()
                .enumerate()
                .filter(|(_, r)| r.kind == RunKind::Line)
                .min_by(|(_, x), (_, y)| {
                    let dx = (mid - mid.clamp(x.t0, x.t1)).abs();
                    let dy = (mid - mid.clamp(y.t0, y.t1)).abs();
                    dx.total_cmp(&dy)
                })
                .map(|(i, _)| i)?;
            let r = &rs[i];
            if (mid - mid.clamp(r.t0, r.t1)).abs() > 0.5 * (phi - plo) + 20.0 {
                return None;
            }
            i
        }
    };
    let ctx = LineContext {
        sw,
        max_gap,
        proposal: (plo, phi),
        own_crossings: &own,
    };
    let start = follow(&rs, anchor, -1, &ctx);
    let end = follow(&rs, anchor, 1, &ctx);
    // Beyond an own extension line where the line stops and the proposal goes on: the line run after
    // the crossing, followed out to its end (not to another arrowhead: that is a neighbour's line).
    let mut outer = Vec::new();
    if interior.is_some() {
        let out_ctx = LineContext { own_crossings: &[], ..ctx };
        for (e, dir) in [(&start, -1i64), (&end, 1i64)] {
            let beyond = if dir > 0 { phi - e.t } else { e.t - plo };
            if !matches!(e.how, "tip_at_crossing" | "own_extension") || beyond < 20.0 {
                continue;
            }
            let Some(ci) = rs.iter().position(|r| r.kind == RunKind::Cross && (r.centre() - e.t).abs() <= 1.5) else { continue };
            let next = ci as i64 + dir;
            if next < 0 || next as usize >= rs.len() || rs[next as usize].kind != RunKind::Line {
                continue;
            }
            let o = follow(&rs, next as usize, dir, &out_ctx);
            let open = matches!(o.how, "ink_end" | "ink_end_before_gap" | "profile_end" | "crossing_end");
            if open && (o.t - e.t).abs() >= 20.0 {
                outer.push((e.t, o.t));
            }
        }
    }
    (end.t - start.t >= 4.0).then_some(Extent { origin, d, start, end, outer })
}

/// A filled arrowhead at the end `end` of a line running inward along `inward`: walking inward from
/// just outside the end, past paper and a crossing (the outline the arrow touches), a wedge whose
/// narrow end comes first and that the line resumes after. Returns the tip (on the crossing when the
/// arrow touches one) and the base (where the line resumes).
pub(crate) fn wedge_at_end(ink: &Ink, end: P, inward: P, sw: f64, cap: f64) -> Option<(P, P)> {
    let d = unit(inward)?;
    let rs = runs(ink, end, d, -6.0, cap.max(12.0), sw);
    let mut idx = 0;
    while idx < rs.len() && matches!(rs[idx].kind, RunKind::Gap | RunKind::Cross) {
        idx += 1;
    }
    let lead = idx;
    // The narrow tip may read as a short line run before the wedge.
    if let (Some(t), Some(w)) = (rs.get(idx), rs.get(idx + 1))
        && t.kind == RunKind::Line
        && w.kind == RunKind::Wide
        && t.len() <= tip_allowance(w, sw)
    {
        idx += 1;
    }
    let r = rs.get(idx)?;
    if !(is_arrow(r, sw) && orientation(r, 1.0) == Wedge::Tip) {
        return None;
    }
    if rs.get(idx + 1)?.kind != RunKind::Line {
        return None;
    }
    let tip_t = match lead.checked_sub(1).map(|k| &rs[k]) {
        Some(c) if c.kind == RunKind::Cross => c.centre(),
        _ => rs[lead].t0,
    };
    let base_t = r.t1 + 0.5;
    Some((add(end, mul(d, tip_t)), add(end, mul(d, base_t))))
}

/// Stroke width along `p`–`q`, robust to arrowheads and junctions: dark pixels within `band` are
/// counted per 1-px bin over `[skip_p, len − skip_q]`; bins counting more than twice the lower
/// quartile (crossings, arrow remnants) are left out; the median of the rest is the width.
pub(crate) fn robust_width(ink: &Ink, p: P, q: P, band: f64, skip_p: f64, skip_q: f64) -> f64 {
    let Some(d) = unit(sub(q, p)) else {
        return 0.0;
    };
    let n = perp(d);
    let len = dist(p, q);
    let (mut t0, mut t1) = (skip_p.max(0.0), len - skip_q.max(0.0));
    if t1 - t0 < 4.0 {
        (t0, t1) = (0.0, len);
    }
    let x0 = ((p.0.min(q.0) - band).floor() as i64).max(0);
    let y0 = ((p.1.min(q.1) - band).floor() as i64).max(0);
    let x1 = ((p.0.max(q.0) + band).ceil() as i64).min(ink.width as i64 - 1);
    let y1 = ((p.1.max(q.1) + band).ceil() as i64).min(ink.height as i64 - 1);
    let mut bins: BTreeMap<i64, usize> = BTreeMap::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            if !ink.is_dark(x, y) {
                continue;
            }
            let v = sub((x as f64, y as f64), p);
            let (tt, ss) = (dot(v, d), dot(v, n).abs());
            if ss <= band && tt >= t0 - 0.5 && tt <= t1 + 0.5 {
                *bins.entry(tt.round() as i64).or_insert(0) += 1;
            }
        }
    }
    let mut counts: Vec<f64> = bins.values().map(|c| *c as f64).collect();
    if counts.is_empty() {
        return 0.0;
    }
    counts.sort_by(|a, b| a.total_cmp(b));
    let q25 = counts[(counts.len() - 1) / 4];
    let limit = (2.0 * q25).max(q25 + 2.0);
    let mut kept: Vec<f64> = counts.into_iter().filter(|c| *c <= limit).collect();
    median(&mut kept).unwrap_or(0.0)
}

/// A segment within `AXIS_SNAP_DEG` of an axis made exactly axis-aligned about its middle; returns
/// the snapped segment and the deviation in degrees, `None` when it is aligned already or farther off.
pub(crate) fn snap_axis(p: P, q: P) -> Option<((P, P), f64)> {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    if dx.hypot(dy) < 2.0 {
        return None;
    }
    let a = dy.atan2(dx).to_degrees().rem_euclid(180.0);
    let off_h = a.min(180.0 - a);
    let off_v = (a - 90.0).abs();
    if off_h > 1e-9 && off_h <= AXIS_SNAP_DEG {
        let y = 0.5 * (p.1 + q.1);
        return Some((((p.0, y), (q.0, y)), off_h));
    }
    if off_v > 1e-9 && off_v <= AXIS_SNAP_DEG {
        let x = 0.5 * (p.0 + q.0);
        return Some((((x, p.1), (x, q.1)), off_v));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink_with(w: usize, h: usize, f: impl Fn(usize, usize) -> bool) -> Ink {
        let mut m = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                m[y * w + x] = f(x, y);
            }
        }
        Ink::from_mask(w, h, m)
    }

    /// A horizontal baseline y = 50 carrying a chain of two dimensions: extension lines at x = 100,
    /// 400 and 700 (80 px tall), each dimension with filled arrowheads (36 px long, 8 px half width)
    /// whose tips touch its extension lines; the line resumes behind every base.
    fn chain() -> Ink {
        let wedge = |x: f64, y: f64, tip: f64, dir: f64| -> bool {
            // dir = +1: the arrow points to larger x (tip at `tip`), base 36 px before it.
            let t = (tip - x) * dir;
            (0.0..=36.0).contains(&t) && (y - 50.0).abs() <= t * 8.0 / 36.0 + 0.5
        };
        ink_with(800, 100, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let line = (49..=51).contains(&y) && (100..=700).contains(&x);
            let ext = [100, 400, 700].iter().any(|e| (e - 1..=e + 1).contains(&x)) && (10..=90).contains(&y);
            let arrows = wedge(xf, yf, 101.0, -1.0) || wedge(xf, yf, 399.0, 1.0) || wedge(xf, yf, 401.0, -1.0) || wedge(xf, yf, 699.0, 1.0);
            line || ext || arrows
        })
    }

    #[test]
    fn the_width_profile_has_runs_of_line_wedge_and_crossing() {
        let ink = chain();
        let rs = runs(&ink, (0.0, 50.0), (1.0, 0.0), 80.0, 420.0, 3.0);
        let kinds: Vec<RunKind> = rs.iter().map(|r| r.kind).collect();
        // Paper, the extension line, the narrow tip (line width), the wedge, the line, the next wedge, its
        // narrow tip, the middle extension line.
        assert_eq!(
            &kinds[..8],
            &[RunKind::Gap, RunKind::Cross, RunKind::Line, RunKind::Wide, RunKind::Line, RunKind::Wide, RunKind::Line, RunKind::Cross],
            "{rs:?}"
        );
        let wedge = &rs[3];
        assert!(is_arrow(wedge, 3.0));
        assert_eq!(orientation(wedge, 1.0), Wedge::Tip);
        assert_eq!(orientation(wedge, -1.0), Wedge::Base);
    }

    #[test]
    fn a_dimension_of_a_chain_ends_at_its_own_arrow_tips() {
        let ink = chain();
        // The proposal covers only the middle of the left dimension; the follower finds both arrows.
        let e = dimension_extent(&ink, ((200.0, 50.0), (300.0, 50.0)), ((200.0, 50.0), (300.0, 50.0)), 3.0, 30.0, &[]).unwrap();
        let (s, t) = (e.at(e.start.t), e.at(e.end.t));
        assert!((s.0 - 100.0).abs() <= 1.0 && (t.0 - 400.0).abs() <= 1.0, "{s:?} {t:?} {e:?}");
        assert_eq!(e.start.how, "tip_at_crossing");
        assert_eq!(e.end.how, "tip_at_crossing");
        let (tip, base) = e.end.arrow.unwrap();
        assert!((e.at(tip).0 - 400.0).abs() <= 1.0 && (e.at(base).0 - 363.5).abs() <= 1.5, "{:?} {:?}", e.at(tip), e.at(base));
        // A proposal reaching past the shared extension line into the neighbour still ends at the own tip.
        let e = dimension_extent(&ink, ((150.0, 50.0), (450.0, 50.0)), ((150.0, 50.0), (450.0, 50.0)), 3.0, 30.0, &[]).unwrap();
        assert!((e.at(e.end.t).0 - 400.0).abs() <= 1.0, "{:?}", e.end);
    }

    #[test]
    fn a_text_break_inside_the_proposal_is_bridged_and_an_arrow_at_an_end_is_found() {
        // A vertical line x = 50 from y = 20 to 180 broken for a text between 80 and 130, with a filled
        // arrowhead at the top end (tip at y = 20, 30 px long) and a crossing outline at y = 180.
        let ink = ink_with(100, 220, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let line = (49..=51).contains(&x) && ((20..80).contains(&y) || (130..=180).contains(&y));
            let head = (20.0..=50.0).contains(&yf) && (xf - 50.0).abs() <= (yf - 20.0) * 7.0 / 30.0 + 0.5;
            let outline = (179..=181).contains(&y) && (5..=95).contains(&x);
            line || head || outline
        });
        let e = dimension_extent(&ink, ((50.0, 40.0), (50.0, 170.0)), ((50.0, 30.0), (50.0, 175.0)), 3.0, 20.0, &[]).unwrap();
        assert!((e.at(e.start.t).1 - 20.0).abs() <= 1.0, "{e:?}");
        assert_eq!(e.start.how, "tip");
        assert!((e.at(e.start.arrow.unwrap().1).1 - 50.5).abs() <= 1.5);
        // Downward the outline crossing is passed and the line ends on paper below it.
        assert!((e.at(e.end.t).1 - 180.0).abs() <= 2.0, "{:?}", e.end);
        // The same arrow from the end inward.
        let (tip, base) = wedge_at_end(&ink, (50.0, 21.0), (0.0, 1.0), 3.0, 80.0).unwrap();
        assert!((tip.1 - 20.0).abs() <= 1.0 && (base.1 - 50.5).abs() <= 1.5, "{tip:?} {base:?}");
        assert!(wedge_at_end(&ink, (50.0, 180.0), (0.0, -1.0), 3.0, 40.0).is_none());
    }

    #[test]
    fn a_line_does_not_run_on_through_a_gap_to_ink_that_ends_on_paper() {
        // A line from x = 50 to 200 with an arrowhead at its left end (tip 50); past a 40 px gap a short
        // stroke of a text lies on its carrier (250..262), then paper. The proposal ends at 205.
        let ink = ink_with(400, 100, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let line = (49..=51).contains(&y) && (50..=200).contains(&x);
            let head = (50.0..=86.0).contains(&xf) && (yf - 50.0).abs() <= (xf - 50.0) * 8.0 / 36.0 + 0.5;
            let glyph = (49..=51).contains(&y) && (240..=252).contains(&x);
            line || head || glyph
        });
        let e = dimension_extent(&ink, ((60.0, 50.0), (205.0, 50.0)), ((60.0, 50.0), (205.0, 50.0)), 3.0, 60.0, &[]).unwrap();
        assert!((e.at(e.end.t).0 - 200.0).abs() <= 1.0, "{:?}", e.end);
        assert_eq!(e.end.how, "ink_end_before_gap");
    }

    #[test]
    fn inside_arrows_anchor_the_line_and_its_run_out_to_an_outside_text_is_an_outer_part() {
        // Extension lines at x = 100 and 200; inside arrows (36 px) with tips on them; the line runs on
        // past the right extension line to x = 340 under a text. The proposal spans 110..330.
        let ink = ink_with(400, 100, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let wedge = |tip: f64, dir: f64| {
                let t = (tip - xf) * dir;
                (0.0..=36.0).contains(&t) && (yf - 50.0).abs() <= t * 8.0 / 36.0 + 0.5
            };
            let line = (49..=51).contains(&y) && (100..=340).contains(&x);
            let ext = [100, 200].iter().any(|e| (e - 1..=e + 1).contains(&x)) && (10..=90).contains(&y);
            line || ext || wedge(101.0, -1.0) || wedge(199.0, 1.0)
        });
        let own = [((100.0, 10.0), (100.0, 90.0)), ((200.0, 10.0), (200.0, 90.0))];
        let e = dimension_extent(&ink, ((110.0, 50.0), (330.0, 50.0)), ((110.0, 50.0), (330.0, 50.0)), 3.0, 30.0, &own).unwrap();
        assert!((e.at(e.start.t).0 - 100.0).abs() <= 1.5 && (e.at(e.end.t).0 - 200.0).abs() <= 1.5, "{e:?}");
        assert!(e.start.arrow.is_some() && e.end.arrow.is_some(), "{e:?}");
        assert_eq!(e.outer.len(), 1, "{e:?}");
        let (a, b) = e.outer[0];
        assert!((e.at(a).0 - 200.0).abs() <= 1.5 && (e.at(b).0 - 340.0).abs() <= 1.5, "{e:?}");
    }

    #[test]
    fn outside_arrows_with_tails_ending_on_another_line_are_this_dimensions() {
        // Extension lines at y = 100 and 200 across a vertical dimension line x = 50; outside arrows (40 px,
        // 12 px half width) with tips on them pointing in; their tails run 40 px on to lines crossing at
        // y = 20 and y = 280.
        let ink = ink_with(100, 300, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let wedge = |tip: f64, dir: f64| {
                let t = (yf - tip) * dir;
                (0.0..=40.0).contains(&t) && (xf - 50.0).abs() <= t * 12.0 / 40.0 + 0.5
            };
            let line = (49..=51).contains(&x) && (20..=280).contains(&y);
            let ext = [100, 200, 20, 280].iter().any(|e| (e - 1..=e + 1).contains(&y)) && (5..=95).contains(&x);
            line || ext || wedge(99.0, -1.0) || wedge(201.0, 1.0)
        });
        let own = [((5.0, 100.0), (95.0, 100.0)), ((5.0, 200.0), (95.0, 200.0))];
        let e = dimension_extent(&ink, ((50.0, 110.0), (50.0, 190.0)), ((50.0, 110.0), (50.0, 190.0)), 3.0, 30.0, &own).unwrap();
        assert_eq!((e.start.how, e.end.how), ("outside_arrow", "outside_arrow"), "{e:?}");
        let (tip, base) = e.start.arrow.unwrap();
        assert!((e.at(tip).1 - 100.0).abs() <= 1.5 && (e.at(base).1 - 59.5).abs() <= 2.5, "{e:?}");
    }

    #[test]
    fn widths_ignore_arrows_and_crossings_and_axes_snap() {
        let ink = chain();
        // Over the whole left dimension (arrows and the middle extension line included) the width is 3.
        assert!((robust_width(&ink, (100.0, 50.0), (400.0, 50.0), 6.0, 0.0, 0.0) - 3.0).abs() < 1e-9);
        let ((p, q), off) = snap_axis((0.0, 0.0), (100.0, 0.3)).unwrap();
        assert!(p.1 == q.1 && (p.1 - 0.15).abs() < 1e-12 && off > 0.17 && off < 0.18);
        assert!(snap_axis((0.0, 0.0), (100.0, 1.0)).is_none());
        assert!(snap_axis((0.0, 0.0), (0.0, 100.0)).is_none());
        let ((p, q), _) = snap_axis((10.0, 0.0), (10.2, 100.0)).unwrap();
        assert!(p.0 == q.0 && (p.0 - 10.1).abs() < 1e-12);
    }
}
