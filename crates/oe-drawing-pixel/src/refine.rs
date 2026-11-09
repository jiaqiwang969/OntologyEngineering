//! Refinement of proposals on the ink: every proposed member is fitted, snapped or tightened with
//! the member algorithms, its support before and after is recorded, and members the ink does not
//! support are marked so that they stay out of the graph (they are reported as unknowns instead).
//!
//! The members of one proposal are refined in phases: strokes first; then arrow tips along the fitted
//! dimension line, with the arrowhead base measured on the ink and the dimension or leader line cut
//! back to that base (the DXF route's lines stop where the arrowhead begins); then text boxes, which
//! keep glyph-sized components only and never the strokes of the same proposal.

use std::collections::BTreeMap;

use crate::geom::{add, dist, dist_point_segment, dot, median, mul, perp, sub, unit};
use crate::members::{arrow_tip, fit_segment, segment_support, snap_point};
use crate::lines;
use crate::observe::{Category, P, Proposal, ProposedMember, Shape};
use crate::stroke::Pattern;
use crate::structure::point_in_polygon;
use crate::{Ink, Segment};

/// A fitted segment may run past a proposed endpoint by at most this much (3 mm).
const MAX_OVERRUN_PX: f64 = 35.0;
/// Beyond a proposed endpoint, an empty run of this many 1-px samples ends the segment.
const OVERRUN_GAP_PX: usize = 2;
/// Beyond a proposed endpoint, ink wider than this many times the stroke's own width is another
/// structure (an outline the line runs into), unless the line continues behind it (a crossing).
const JUNCTION_WIDEN_RATIO: f64 = 1.8;
/// A crossing stroke is passed when the line's own width returns within this many pixels.
const CROSSING_LOOKAHEAD_PX: f64 = 8.0;
/// Arrowheads are searched this far back from the tip, and are at least this wide at the base.
const ARROW_SEARCH_PX: f64 = 60.0;
const ARROW_MIN_WIDTH_PX: f64 = 4.0;
/// A perpendicular ink extent above this at an arrowhead sample is a crossing stroke, not the head.
const ARROW_CROSSING_PX: f64 = 20.0;
/// The arrow tip must lie this close (plus the arrow length) to the end of the line it terminates.
const ARROW_END_SLACK_PX: f64 = 8.0;

/// Thresholds of the refinement, in pixels at 300 dpi.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RefineParams {
    /// Corridor half-width around a proposed segment (1 mm).
    pub corridor_px: f64,
    /// Empty run bridged when extending a fitted segment.
    pub max_gap_px: f64,
    /// The same for centre lines, whose dash gaps must be bridged.
    pub centre_gap_px: f64,
    /// Search window of an arrow tip.
    pub arrow_window_px: f64,
    /// Margin around a proposed text box.
    pub text_pad_px: f64,
    /// Snap radius of points and polyline vertices.
    pub snap_radius_px: f64,
    /// Members below this support after refinement are unsupported.
    pub support_min: f64,
}

impl Default for RefineParams {
    fn default() -> Self {
        RefineParams {
            corridor_px: 12.0,
            max_gap_px: 8.0,
            centre_gap_px: 40.0,
            arrow_window_px: 20.0,
            text_pad_px: 6.0,
            snap_radius_px: 12.0,
            support_min: 0.8,
        }
    }
}

/// One member after refinement.
#[derive(Clone, Debug, PartialEq)]
pub struct RefinedMember {
    pub role: String,
    pub text: Option<String>,
    pub proposed: Shape,
    pub refined: Shape,
    pub support_before: f64,
    pub support_after: f64,
    pub moved_px: f64,
    pub width_px: f64,
    /// Line pattern measured along a refined segment.
    pub pattern: Option<Pattern>,
    pub supported: bool,
    /// Set when de-duplication took this member from another tile's proposal (`tile#index`).
    pub merged_from: Option<String>,
    /// Base of an arrowhead measured on the ink (page pixels) and its length from the tip.
    pub arrow_base: Option<P>,
    pub arrow_length_px: Option<f64>,
    /// Connected components a text box fit discarded as strokes rather than glyphs.
    pub discarded_components: usize,
    /// What later steps did to the member (extension span re-derived, hatch measured, ...).
    pub adjustments: Vec<String>,
    /// A dimension line's extent from arrow tip to arrow tip, before it was cut at the arrow bases.
    pub span_px: Option<(P, P)>,
}

/// A proposal with its refined members.
#[derive(Clone, Debug, PartialEq)]
pub struct RefinedProposal {
    pub proposal: Proposal,
    pub members: Vec<RefinedMember>,
}

impl RefinedProposal {
    pub fn supported(&self) -> impl Iterator<Item = &RefinedMember> {
        self.members.iter().filter(|m| m.supported)
    }

    /// Supported refined segments of one role, with their widths.
    pub fn segments(&self, role: &str) -> Vec<Segment> {
        self.members
            .iter()
            .filter(|m| m.supported && m.role == role)
            .filter_map(|m| match m.refined {
                Shape::Segment(p, q) => Some((p, q, m.width_px)),
                _ => None,
            })
            .collect()
    }
}

/// Count of dark pixels inside a box (inclusive pixel centres).
fn dark_in_box(ink: &Ink, b: [f64; 4]) -> usize {
    let x0 = (b[0].ceil() as i64).max(0);
    let y0 = (b[1].ceil() as i64).max(0);
    let x1 = (b[2].floor() as i64).min(ink.width as i64 - 1);
    let y1 = (b[3].floor() as i64).min(ink.height as i64 - 1);
    let mut n = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if ink.is_dark(x, y) {
                n += 1;
            }
        }
    }
    n
}

/// Last dark step from `c` along `dir` in 0.5-px steps (0 when the next pixel is already paper).
fn probe(ink: &Ink, c: P, dir: P, limit: f64) -> f64 {
    let mut last = 0.0;
    let mut s = 0.5;
    while s <= limit {
        let pt = add(c, mul(dir, s));
        if ink.is_dark(pt.0.round() as i64, pt.1.round() as i64) {
            last = s;
        } else {
            break;
        }
        s += 0.5;
    }
    last
}

/// Ink extent across a point along the normal `n`: 0 when the point itself is paper.
pub(crate) fn perp_extent(ink: &Ink, c: P, n: P) -> f64 {
    if !ink.is_dark(c.0.round() as i64, c.1.round() as i64) {
        return 0.0;
    }
    probe(ink, c, n, 32.0) + probe(ink, c, (-n.0, -n.1), 32.0) + 1.0
}

/// Keep a fitted segment from running past the proposal into other ink. Extension lines are often
/// the prolongation of a contour side with a small gap between the two, or touch an outline at a
/// junction; `fit_segment` bridges gaps up to its allowance and follows connected ink. Beyond either
/// proposed endpoint (projected on the fitted line) the segment is cut back at the first empty run of
/// `OVERRUN_GAP_PX` samples or where the ink widens or branches beyond the stroke's own width (a
/// crossing stroke, behind which the own width returns within `CROSSING_LOOKAHEAD_PX`, is passed),
/// and the overrun is capped at `MAX_OVERRUN_PX`; inside the proposed extent nothing changes.
pub(crate) fn restrain_overrun(ink: &Ink, proposed: (P, P), p: P, q: P) -> (P, P) {
    let Some(d) = unit(sub(q, p)) else {
        return (p, q);
    };
    let n = perp(d);
    let len = dist(p, q);
    let lo0 = dot(sub(proposed.0, p), d).clamp(0.0, len);
    let hi0 = dot(sub(proposed.1, p), d).clamp(0.0, len);
    let (lo0, hi0) = (lo0.min(hi0), lo0.max(hi0));
    let at = |t: f64| add(p, mul(d, t));
    let on = |t: f64| {
        let s = at(t);
        ink.has_dark_within(s.0, s.1, 1.5)
    };
    // The stroke's own width: the median extent at interior samples of the proposal.
    let mut own: Vec<f64> = (0..7)
        .map(|i| lo0 + (hi0 - lo0) * (i as f64 + 1.0) / 8.0)
        .map(|t| perp_extent(ink, at(t), n))
        .filter(|e| *e > 0.0)
        .collect();
    let own = median(&mut own).unwrap_or(3.0);
    let wide = (own * JUNCTION_WIDEN_RATIO).max(own + 3.0);
    let walk = |start: f64, end: f64, step: f64| -> f64 {
        let mut last_on = start;
        let mut empty = 0usize;
        let mut t = start;
        while (step > 0.0 && t <= end) || (step < 0.0 && t >= end) {
            if (t - start).abs() > MAX_OVERRUN_PX {
                return last_on;
            }
            if on(t) {
                if perp_extent(ink, at(t), n) > wide {
                    // A crossing stroke is passed when the own width returns right behind it.
                    let mut k = t + step;
                    let mut passes = false;
                    while (k - t).abs() <= CROSSING_LOOKAHEAD_PX && ((step > 0.0 && k <= end) || (step < 0.0 && k >= end)) {
                        let e = perp_extent(ink, at(k), n);
                        if e > 0.0 && e <= own + 1.5 {
                            passes = true;
                            break;
                        }
                        if e == 0.0 {
                            break;
                        }
                        k += step;
                    }
                    if !passes {
                        return last_on;
                    }
                } else {
                    last_on = t;
                }
                empty = 0;
            } else {
                empty += 1;
                if empty >= OVERRUN_GAP_PX {
                    return last_on;
                }
            }
            t += step;
        }
        if empty > 0 { last_on } else { end }
    };
    let new_lo = if lo0 > 1.0 { walk(lo0, 0.0, -1.0) } else { 0.0 };
    let new_hi = if hi0 < len - 1.0 { walk(hi0, len, 1.0) } else { len };
    if new_hi - new_lo < 1.0 {
        return (p, q);
    }
    (at(new_lo), at(new_hi))
}

/// Stroke thickness along `p`–`q`: the median, over 1-px bins along the segment, of the number of
/// dark pixels within `band` of it (ink area per unit length). Used when the fitted extent was
/// restrained, since `fit_segment` measured the width over the extent it had found.
pub(crate) fn measure_width(ink: &Ink, p: P, q: P, band: f64) -> f64 {
    let Some(d) = unit(sub(q, p)) else {
        return 0.0;
    };
    let n = perp(d);
    let len = dist(p, q);
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
            if ss <= band && tt >= -0.5 && tt <= len + 0.5 {
                *bins.entry(tt.round() as i64).or_insert(0) += 1;
            }
        }
    }
    let mut counts: Vec<f64> = bins.values().map(|c| *c as f64).collect();
    median(&mut counts).unwrap_or(0.0)
}

/// Line pattern along a segment from the ink runs on it: `Continuous` without gaps of 3 px or more,
/// `DashDot` when the ink runs alternate between long dashes and short dots (longest ≥ 2.5 × shortest,
/// shortest ≤ 20 px), `Dashed` when the runs and gaps are regular, `Unknown` otherwise.
pub(crate) fn measure_pattern(ink: &Ink, p: P, q: P) -> Pattern {
    let len = dist(p, q);
    if !len.is_finite() || len < 4.0 {
        return Pattern::Continuous;
    }
    let n = len.ceil() as usize + 1;
    let mut runs: Vec<(bool, usize)> = Vec::new();
    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let on = ink.has_dark_within(p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t, 1.5);
        match runs.last_mut() {
            Some((o, l)) if *o == on => *l += 1,
            _ => runs.push((on, 1)),
        }
    }
    while runs.first().is_some_and(|r| !r.0) {
        runs.remove(0);
    }
    while runs.last().is_some_and(|r| !r.0) {
        runs.pop();
    }
    // Merge tiny gaps (< 3 px) into the surrounding ink runs.
    let mut merged: Vec<(bool, usize)> = Vec::new();
    for (on, l) in runs {
        if !on && l < 3
            && let Some((true, last)) = merged.last_mut()
        {
            *last += l;
            continue;
        }
        match merged.last_mut() {
            Some((o, last)) if *o == on => *last += l,
            _ => merged.push((on, l)),
        }
    }
    let ons: Vec<f64> = merged.iter().filter(|r| r.0).map(|r| r.1 as f64).collect();
    let offs: Vec<f64> = merged.iter().filter(|r| !r.0).map(|r| r.1 as f64).collect();
    if offs.is_empty() {
        return Pattern::Continuous;
    }
    if ons.len() < 3 {
        return Pattern::Unknown;
    }
    let lo = ons.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = ons.iter().copied().fold(0.0, f64::max);
    let shorts = ons.iter().filter(|&&l| l <= lo * 1.8 + 2.0).count();
    let longs = ons.iter().filter(|&&l| l >= hi * 0.6).count();
    if hi >= 2.5 * lo && lo <= 20.0 && shorts >= 1 && longs >= 1 && shorts + longs >= ons.len() * 3 / 4 {
        return Pattern::DashDot;
    }
    let cv = |v: &[f64]| {
        let m = v.iter().sum::<f64>() / v.len() as f64;
        let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / v.len() as f64;
        if m > 0.0 { var.sqrt() / m } else { 0.0 }
    };
    if cv(&ons) < 0.6 && cv(&offs) < 0.8 {
        Pattern::Dashed
    } else {
        Pattern::Unknown
    }
}

/// A filled arrowhead on the ink near `approx`, on the axis through it with direction `axis` (either
/// sense): walking the axis, the ink's perpendicular extent rises from the line to at least
/// `ARROW_MIN_WIDTH_PX` over the head and falls back on both sides — abruptly at the base (a flat
/// edge, within a few pixels) and gradually at the tip (the taper, over the head's length). That tells
/// the tip from the base, hence the direction the arrow points whatever sense `axis` has, and a line
/// running on through the tip (a diameter line to a text outside the circle, a leader) does not move
/// the tip. Samples wider than `ARROW_CROSSING_PX` are a crossing stroke and are skipped; the base of a
/// large head is wider than that and so lies among skipped samples, which does not break the head's
/// run. The wide runs within `window_px` (plus the thin apex stretch) of `approx` are tried nearest
/// first — a crossing stroke at the tip leaves a short wide run that fails the head's proportions — and
/// for each the axis is re-centred once on its ink, since a proposal a pixel or two off the axis would
/// cut the taper short. The apex is no wider than the line over its last stretch (a 20° head on a 3 px
/// line for some 8 px) and a stroke crossing at the tip hides it, so the tip is where the taper, fitted
/// over the head's samples, comes down to one pixel — taken when it falls beyond the last wide sample,
/// and past the fall to the line by at most the head's measured length. `None` without a head.
/// Returns (tip, base, direction, length).
pub(crate) fn arrow_head(ink: &Ink, approx: P, axis: P, line_width: f64, window_px: f64) -> Option<(P, P, P, f64)> {
    let a = unit(axis)?;
    let n = perp(a);
    let floor = (line_width + 1.5).max(2.5);
    let reach = 2.0 * ARROW_SEARCH_PX;
    let steps = (2.0 * reach / 0.5) as i64;
    // Extent at every half pixel of the axis through `c`, crossings as `None`.
    let profile = |c: P| -> Vec<(f64, Option<f64>)> {
        (0..=steps)
            .map(|k| {
                let t = -reach + k as f64 * 0.5;
                let e = perp_extent(ink, add(c, mul(a, t)), n);
                (t, (e <= ARROW_CROSSING_PX).then_some(e))
            })
            .collect()
    };
    let is_wide = |s: &(f64, Option<f64>)| s.1.is_some_and(|e| e >= ARROW_MIN_WIDTH_PX);
    // The runs of head-wide samples (crossings inside a run do not end it) reaching within the
    // window of `approx`, as (first, peak, last) sample indices, nearest first.
    let runs = |prof: &[(f64, Option<f64>)]| -> Vec<(usize, usize, usize)> {
        let mut out: Vec<(f64, (usize, usize, usize))> = Vec::new();
        let mut i = 0;
        while i < prof.len() {
            if !is_wide(&prof[i]) {
                i += 1;
                continue;
            }
            let (start, mut end, mut peak) = (i, i, i);
            let mut j = i + 1;
            while j < prof.len() {
                match prof[j].1 {
                    Some(e) if e >= ARROW_MIN_WIDTH_PX => {
                        end = j;
                        if e > prof[peak].1.unwrap_or(0.0) {
                            peak = j;
                        }
                    }
                    None => {}
                    Some(_) => break,
                }
                j += 1;
            }
            let near = if prof[start].0 > 0.0 {
                prof[start].0
            } else if prof[end].0 < 0.0 {
                -prof[end].0
            } else {
                0.0
            };
            if near <= window_px + ARROW_CROSSING_PX {
                out.push((near, (start, peak, end)));
            }
            i = j.max(i + 1);
        }
        out.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.0.cmp(&y.1.0)));
        out.into_iter().map(|(_, r)| r).collect()
    };
    // Tip, base and sense of one run: (t_tip, t_base, tip_side, length), `None` when it is not a head.
    let evaluate = |prof: &[(f64, Option<f64>)], (_, peak, _): (usize, usize, usize)| -> Option<(f64, f64, i64, f64)> {
        let peak_e = prof[peak].1?;
        let base_level = floor.max(0.5 * peak_e);
        // From the peak outwards on each side: where the extent falls to a level.
        let fall = |dir: i64, level: f64| -> Option<usize> {
            let mut k = peak as i64;
            loop {
                k += dir;
                if k < 0 || k as usize >= prof.len() {
                    return None;
                }
                if let Some(e) = prof[k as usize].1
                    && e <= level
                {
                    return Some(k as usize);
                }
            }
        };
        let plus = fall(1, base_level)?;
        let minus = fall(-1, base_level)?;
        let (d_plus, d_minus) = (prof[plus].0 - prof[peak].0, prof[peak].0 - prof[minus].0);
        // The taper is the longer descent.
        let tip_side: i64 = if d_plus >= d_minus { 1 } else { -1 };
        let tip_k = fall(tip_side, floor)?;
        let base_k = if tip_side > 0 { minus } else { plus };
        let dir = tip_side as f64;
        let t_base = prof[base_k].0 + 0.5 * dir;
        let t_floor = prof[tip_k].0 - 0.5 * dir;
        let taper: Vec<(f64, f64)> = ((base_k.min(tip_k) + 1)..tip_k.max(base_k))
            .filter_map(|k| prof[k].1.filter(|e| *e > floor).map(|e| (prof[k].0, e)))
            .collect();
        let mut t_tip = t_floor;
        if taper.len() >= 4 {
            let count = taper.len() as f64;
            let (mt, me) = (taper.iter().map(|s| s.0).sum::<f64>() / count, taper.iter().map(|s| s.1).sum::<f64>() / count);
            let (sxy, sxx) = taper
                .iter()
                .fold((0.0, 0.0), |acc, (t, e)| (acc.0 + (t - mt) * (e - me), acc.1 + (t - mt) * (t - mt)));
            if sxx > 0.0 {
                let slope = sxy / sxx;
                if slope * dir < -1e-6 {
                    let at_one = mt + (1.0 - me) / slope;
                    let last_wide = taper.last().map_or(t_floor, |s| s.0);
                    let past_wide = (at_one - last_wide) * dir;
                    let past_floor = (at_one - t_floor) * dir;
                    if past_wide > 0.0 {
                        t_tip = if past_floor <= 0.0 { at_one } else { t_floor + dir * past_floor.min((t_floor - t_base).abs()) };
                    }
                }
            }
        }
        let len = (t_tip - t_base).abs();
        // A head is longer than it is wide.
        (len >= ARROW_MIN_WIDTH_PX && len >= 1.5 * peak_e && len <= reach).then_some((t_tip, t_base, tip_side, len))
    };
    let first = profile(approx);
    for run in runs(&first) {
        let t_peak = first[run.1].0;
        let at = add(approx, mul(a, t_peak));
        let shift = 0.5 * (probe(ink, at, n, 32.0) - probe(ink, at, (-n.0, -n.1), 32.0));
        let (c, prof) = if shift.abs() >= 0.5 {
            let c = add(approx, mul(n, shift));
            (c, profile(c))
        } else {
            (approx, first.clone())
        };
        // The same run on the re-centred profile: the one whose peak lies nearest.
        let Some(found) = runs(&prof)
            .into_iter()
            .min_by(|x, y| (prof[x.1].0 - t_peak).abs().total_cmp(&(prof[y.1].0 - t_peak).abs()))
        else {
            continue;
        };
        if let Some((t_tip, t_base, tip_side, len)) = evaluate(&prof, found) {
            let along = if tip_side > 0 { a } else { (-a.0, -a.1) };
            return Some((add(c, mul(a, t_tip)), add(c, mul(a, t_base)), along, len));
        }
    }
    None
}

/// Base of a filled arrowhead with its tip at `tip`, pointing along `along`: walking back from the
/// tip the ink widens over the head and narrows again to the line behind it; the base is where the
/// width falls back to the line (`line_width` + 1.5 px, or half the head's width). Samples wider
/// than `ARROW_CROSSING_PX` are a crossing stroke (the extension line at the tip) and are skipped.
/// `None` when no head at least `ARROW_MIN_WIDTH_PX` wide is found within `ARROW_SEARCH_PX`.
pub(crate) fn arrow_base(ink: &Ink, tip: P, along: P, line_width: f64) -> Option<(P, f64)> {
    let a = unit(along)?;
    let n = perp(a);
    let floor = (line_width + 1.5).max(2.5);
    let (mut peak, mut t_peak) = (0.0f64, 0.0f64);
    let mut t = 1.0;
    while t <= ARROW_SEARCH_PX {
        let e = perp_extent(ink, sub(tip, mul(a, t)), n);
        if e > ARROW_CROSSING_PX {
            t += 0.5;
            continue;
        }
        if e > peak {
            peak = e;
            t_peak = t;
        }
        if peak >= ARROW_MIN_WIDTH_PX && t > t_peak + 0.5 && e <= floor.max(0.5 * peak) {
            let len = (t - 0.5).max(1.0);
            return Some((sub(tip, mul(a, len)), len));
        }
        t += 0.5;
    }
    None
}

/// Tighten a proposed text box to its glyphs, discarding strokes. The ink of a window reaching
/// `max(3 h, 30 px)` beyond the box grown by `pad_px` is taken without the pixels of the `exclude`
/// strokes (the refined strokes of the same proposal); its 8-connected components are split along
/// straight pass-through strokes (narrow contacts on two opposite window sides, joined by a line
/// whose band is removed); each remaining component is kept when it has at least 3 pixels, lies at
/// least half inside the proposed box, is not long and thin (longer than three box heights with an
/// aspect over 6) and does not reach more than half a box height outside the proposed box. Returns
/// the box of the kept pixel centres (the proposal when nothing is kept), the number of kept
/// components and the number discarded as strokes.
pub(crate) fn text_box_fit_glyphs(
    ink: &Ink,
    bbox: [f64; 4],
    pad_px: f64,
    height_px: f64,
    exclude: &[Segment],
) -> ([f64; 4], usize, usize) {
    let (x0, y0, x1, y1) = (bbox[0].min(bbox[2]), bbox[1].min(bbox[3]), bbox[0].max(bbox[2]), bbox[1].max(bbox[3]));
    let proposal = [x0, y0, x1, y1];
    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
        return (proposal, 0, 0);
    }
    let h = height_px.max(1.0);
    let pad = pad_px.max(0.0);
    let ext = (3.0 * h).max(30.0) + pad;
    let rx0 = ((x0 - ext).floor() as i64).max(0);
    let ry0 = ((y0 - ext).floor() as i64).max(0);
    let rx1 = ((x1 + ext).ceil() as i64).min(ink.width as i64 - 1);
    let ry1 = ((y1 + ext).ceil() as i64).min(ink.height as i64 - 1);
    if rx0 > rx1 || ry0 > ry1 {
        return (proposal, 0, 0);
    }
    let (w, hh) = ((rx1 - rx0 + 1) as usize, (ry1 - ry0 + 1) as usize);
    let idx = |x: i64, y: i64| (y - ry0) as usize * w + (x - rx0) as usize;
    // The window's ink without the excluded strokes.
    let mut grid = vec![false; w * hh];
    for y in ry0..=ry1 {
        for x in rx0..=rx1 {
            if !ink.is_dark(x, y) {
                continue;
            }
            let pt = (x as f64, y as f64);
            let excluded = exclude
                .iter()
                .any(|&(p, q, wdt)| dist_point_segment(pt, p, q) <= wdt * 0.5 + 1.0);
            grid[idx(x, y)] = !excluded;
        }
    }
    let components = |grid: &[bool], seeds: Option<&[(i64, i64)]>| -> Vec<Vec<(i64, i64)>> {
        let mut seen = vec![false; w * hh];
        let mut out = Vec::new();
        let all: Vec<(i64, i64)>;
        let seeds: &[(i64, i64)] = match seeds {
            Some(s) => s,
            None => {
                all = (ry0..=ry1).flat_map(|y| (rx0..=rx1).map(move |x| (x, y))).collect();
                &all
            }
        };
        for &(sx, sy) in seeds {
            if !grid[idx(sx, sy)] || seen[idx(sx, sy)] {
                continue;
            }
            seen[idx(sx, sy)] = true;
            let mut stack = vec![(sx, sy)];
            let mut comp = Vec::new();
            while let Some((cx, cy)) = stack.pop() {
                comp.push((cx, cy));
                for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    let (nx, ny) = (cx + dx, cy + dy);
                    if nx < rx0 || ny < ry0 || nx > rx1 || ny > ry1 {
                        continue;
                    }
                    let i = idx(nx, ny);
                    if grid[i] && !seen[i] {
                        seen[i] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            comp.sort_unstable_by_key(|&(x, y)| (y, x));
            out.push(comp);
        }
        out
    };
    // Straight strokes through a component: a row or column run at least three box heights long
    // (no glyph is that wide); its pixels are removed and the rest is split again.
    let run_min = (3.0 * h).ceil() as i64;
    let long_runs = |v: &[i64]| -> Vec<(i64, i64)> {
        let mut out = Vec::new();
        if v.is_empty() {
            return out;
        }
        let (mut start, mut prev) = (v[0], v[0]);
        for &t in &v[1..] {
            if t != prev + 1 {
                if prev - start + 1 >= run_min {
                    out.push((start, prev));
                }
                start = t;
            }
            prev = t;
        }
        if prev - start + 1 >= run_min {
            out.push((start, prev));
        }
        out
    };
    let (mut n_kept, mut n_discarded) = (0usize, 0usize);
    let mut kept: Option<[i64; 4]> = None;
    let mut work = grid.clone();
    for comp in components(&grid, None) {
        if comp.len() < 3 {
            continue;
        }
        let mut rows: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        let mut cols: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        for &(x, y) in &comp {
            rows.entry(y).or_default().push(x);
            cols.entry(x).or_default().push(y);
        }
        let mut remove: Vec<(i64, i64)> = Vec::new();
        let mut strokes = 0usize;
        for (y, xs) in &rows {
            let mut xs = xs.clone();
            xs.sort_unstable();
            for (a, b) in long_runs(&xs) {
                remove.extend((a..=b).map(|x| (x, *y)));
            }
        }
        if !remove.is_empty() {
            strokes += 1;
        }
        let before = remove.len();
        for (x, ys) in &cols {
            let mut ys = ys.clone();
            ys.sort_unstable();
            for (a, b) in long_runs(&ys) {
                remove.extend((a..=b).map(|y| (*x, y)));
            }
        }
        if remove.len() > before {
            strokes += 1;
        }
        let parts: Vec<Vec<(i64, i64)>> = if remove.is_empty() {
            vec![comp]
        } else {
            n_discarded += strokes;
            for &(x, y) in &remove {
                work[idx(x, y)] = false;
            }
            components(&work, Some(&comp))
        };
        for part in parts {
            if part.len() < 3 {
                continue;
            }
            let inside = part
                .iter()
                .filter(|&&(x, y)| {
                    let (xf, yf) = (x as f64, y as f64);
                    xf >= x0 && xf <= x1 && yf >= y0 && yf <= y1
                })
                .count();
            let (mut cx0, mut cy0, mut cx1, mut cy1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
            for &(x, y) in &part {
                cx0 = cx0.min(x);
                cy0 = cy0.min(y);
                cx1 = cx1.max(x);
                cy1 = cy1.max(y);
            }
            let (cw, ch) = ((cx1 - cx0 + 1) as f64, (cy1 - cy0 + 1) as f64);
            let (long, thin) = (cw.max(ch), cw.min(ch));
            let long_thin = long > 3.0 * h && long / thin > 6.0;
            let outside = (cx0 as f64) < x0 - 0.5 * h
                || (cy0 as f64) < y0 - 0.5 * h
                || (cx1 as f64) > x1 + 0.5 * h
                || (cy1 as f64) > y1 + 0.5 * h;
            if inside * 2 < part.len() || long_thin || outside {
                n_discarded += 1;
                continue;
            }
            n_kept += 1;
            kept = Some(match kept {
                None => [cx0, cy0, cx1, cy1],
                Some([a, b, c, d]) => [a.min(cx0), b.min(cy0), c.max(cx1), d.max(cy1)],
            });
        }
    }
    match kept {
        Some([a, b, c, d]) => ([a as f64, b as f64, c as f64, d as f64], n_kept, n_discarded),
        None => (proposal, 0, n_discarded),
    }
}

fn blank(role: &str, text: Option<&String>, shape: &Shape) -> RefinedMember {
    RefinedMember {
        role: role.to_owned(),
        text: text.cloned(),
        proposed: shape.clone(),
        refined: shape.clone(),
        support_before: 0.0,
        support_after: 0.0,
        moved_px: 0.0,
        width_px: 0.0,
        pattern: None,
        supported: false,
        merged_from: None,
        arrow_base: None,
        arrow_length_px: None,
        discarded_components: 0,
        adjustments: Vec::new(),
        span_px: None,
    }
}

/// Hatch pattern measured on the ink inside a polygon by projection: the dark pixels of the region
/// are projected onto candidate normal directions (2° steps, then 0.25°), and the direction whose
/// 1-px offset histogram is sharpest (largest variance over mean) is the hatch normal; the peaks of
/// that histogram are the hatch lines and the median peak distance is the spacing. Robust to lines
/// broken into fragments. Returns the hatch angle in the y-up drawing frame (degrees in [0, 180)),
/// the spacing in pixels and the number of lines; `None` with too little ink or fewer than two lines.
pub(crate) fn measure_hatch(ink: &Ink, polygon: &[P]) -> Option<(f64, f64, usize)> {
    if polygon.len() < 3 {
        return None;
    }
    let x0 = (polygon.iter().map(|p| p.0).fold(f64::INFINITY, f64::min).floor() as i64).max(0);
    let y0 = (polygon.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).floor() as i64).max(0);
    let x1 = (polygon.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max).ceil() as i64).min(ink.width as i64 - 1);
    let y1 = (polygon.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).ceil() as i64).min(ink.height as i64 - 1);
    if x1 - x0 < 4 || y1 - y0 < 4 {
        return None;
    }
    let mut pts: Vec<P> = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            if ink.is_dark(x, y) && point_in_polygon((x as f64, y as f64), polygon) {
                pts.push((x as f64, y as f64));
            }
        }
    }
    if pts.len() < 30 {
        return None;
    }
    let histogram = |phi: f64| -> (f64, Vec<usize>) {
        let (c, sn) = (phi.cos(), phi.sin());
        let offs: Vec<f64> = pts.iter().map(|p| p.0 * c + p.1 * sn).collect();
        let lo = offs.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = offs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let n = (hi - lo).ceil() as usize + 1;
        let mut hist = vec![0usize; n];
        for o in &offs {
            hist[((o - lo).round() as usize).min(n - 1)] += 1;
        }
        let mean = pts.len() as f64 / n as f64;
        let var = hist.iter().map(|&h| (h as f64 - mean).powi(2)).sum::<f64>() / n as f64;
        (var / mean.max(1e-9), hist)
    };
    let mut best = (f64::NEG_INFINITY, 0.0f64);
    let mut phi = 0.0f64;
    while phi < 180.0 {
        let (score, _) = histogram(phi.to_radians());
        if score > best.0 {
            best = (score, phi);
        }
        phi += 2.0;
    }
    let coarse = best.1;
    let mut phi = coarse - 2.0;
    while phi <= coarse + 2.0 {
        let (score, _) = histogram(phi.to_radians());
        if score > best.0 {
            best = (score, phi);
        }
        phi += 0.25;
    }
    let (_, hist) = histogram(best.1.to_radians());
    let max = *hist.iter().max()? as f64;
    // Peaks: local maxima at least 40 % of the highest, 3 px apart.
    let mut peaks: Vec<usize> = Vec::new();
    for i in 0..hist.len() {
        let h = hist[i];
        if (h as f64) < 0.4 * max {
            continue;
        }
        let left = if i > 0 { hist[i - 1] } else { 0 };
        let right = if i + 1 < hist.len() { hist[i + 1] } else { 0 };
        if h >= left && h >= right {
            match peaks.last() {
                Some(&last) if i - last < 3 => {
                    if h > hist[last] {
                        *peaks.last_mut().expect("peak") = i;
                    }
                }
                _ => peaks.push(i),
            }
        }
    }
    if peaks.len() < 2 {
        return None;
    }
    let mut gaps: Vec<f64> = peaks.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
    let spacing = median(&mut gaps)?;
    // The hatch direction is perpendicular to the normal; image angles turn the other way once y points up.
    let direction_img = best.1.to_radians() + std::f64::consts::FRAC_PI_2;
    let angle = (-direction_img).to_degrees().rem_euclid(180.0);
    Some((angle, spacing, peaks.len()))
}

/// Length of the dark run beside a point: from `start` outward along `dir` in 0.5-px steps, one
/// half-pixel gap tolerated, up to `limit`.
fn side_run(ink: &Ink, c: P, dir: P, start: f64, limit: f64) -> f64 {
    let mut last = 0.0;
    let mut gap = 0;
    let mut s = start;
    while s <= start + limit {
        let pt = add(c, mul(dir, s));
        if ink.is_dark(pt.0.round() as i64, pt.1.round() as i64) {
            last = s - start + 0.5;
            gap = 0;
        } else {
            gap += 1;
            if gap > 1 {
                break;
            }
        }
        s += 0.5;
    }
    last
}

/// A perpendicular junction joins this many pixels of ink beside the line (2 mm).
const JUNCTION_RUN_PX: f64 = 23.6;
/// Known crossing lines within this distance of a junction are passed through.
const PASS_THROUGH_PX: f64 = 4.0;
/// The walk skips this much around the dimension-line crossing (its width and the arrowheads).
const CROSSING_SKIP_PX: f64 = 10.0;
/// Caps of the feature-side and overshoot-side walks.
const FEATURE_WALK_PX: f64 = 1200.0;
const OVERSHOOT_WALK_PX: f64 = 95.0;
/// The crossing may lie this far beyond the ends of the (arrow-trimmed) dimension line.
const CROSSING_SLACK_PX: f64 = 45.0;
/// Nominal overshoot of an extension line shared with an outer dimension (2 mm).
const NOMINAL_OVERSHOOT_PX: f64 = 23.6;

/// The span of an extension line derived from its dimension-line crossing: from the crossing, along
/// the fitted carrier, towards the feature up to the first perpendicular junction (an ink run of at
/// least 2 mm joining on either side, the outline corner) or the ink end, never beyond; and on the
/// other side the overshoot up to its ink end or junction. Known lines (`pass_through`: other
/// dimension lines, centre lines, leaders) crossing the carrier are passed on the feature side; on
/// the overshoot side such a crossing means the extension line goes on to serve an outer dimension,
/// and this dimension's member ends at the nominal 2 mm overshoot. `None` when the carrier has no
/// ink on the feature side of the crossing or the crossing lies off the carrier.
pub(crate) fn extension_span(ink: &Ink, carrier: (P, P), dimension: (P, P), width_px: f64, pass_through: &[(P, P)]) -> Option<((P, P), String)> {
    let (p, q) = carrier;
    let d = unit(sub(q, p))?;
    let n = perp(d);
    let e = sub(dimension.1, dimension.0);
    let denom = d.0 * e.1 - d.1 * e.0;
    if denom.abs() < 1e-9 {
        return None;
    }
    let wv = sub(dimension.0, p);
    let tc = (wv.0 * e.1 - wv.1 * e.0) / denom;
    let u = (wv.0 * d.1 - wv.1 * d.0) / denom;
    let dimension_len = dist(dimension.0, dimension.1);
    if u * dimension_len < -CROSSING_SLACK_PX || u * dimension_len > dimension_len + CROSSING_SLACK_PX {
        return None;
    }
    let at = |t: f64| add(p, mul(d, t));
    let on = |t: f64| {
        let s = at(t);
        ink.has_dark_within(s.0, s.1, 1.5)
    };
    let half = width_px.max(1.0) * 0.5 + 1.0;
    let perpendicular = |t: f64| -> bool {
        let c = at(t);
        side_run(ink, c, n, half, 30.0) >= JUNCTION_RUN_PX || side_run(ink, c, (-n.0, -n.1), half, 30.0) >= JUNCTION_RUN_PX
    };
    let passes = |t: f64| -> bool {
        let c = at(t);
        pass_through.iter().any(|&(a, b)| dist_point_segment(c, a, b) <= PASS_THROUGH_PX)
    };
    let walk = |sign: f64, cap: f64, overshoot: bool| -> (f64, &'static str) {
        let mut t = tc + sign * 2.0;
        let mut last_on = tc;
        let mut gap = 0usize;
        let mut found = false;
        while (t - tc).abs() <= cap {
            if on(t) {
                found = true;
                // No junction test across the dimension line and its arrowheads.
                if (t - tc).abs() > CROSSING_SKIP_PX && perpendicular(t) {
                    if !passes(t) {
                        return (t - sign * 1.0, "junction");
                    }
                    if overshoot {
                        // The line goes on to an outer dimension: shared ink, nominal overshoot.
                        let nominal = NOMINAL_OVERSHOOT_PX.min((t - tc).abs() - 2.0).max(2.0);
                        return (tc + sign * nominal, "shared_nominal");
                    }
                }
                last_on = t;
                gap = 0;
            } else {
                gap += 1;
                if gap > 2 {
                    break;
                }
            }
            t += sign;
        }
        let capped = (t - tc).abs() > cap;
        (last_on, if !found { "none" } else if capped { "cap" } else { "ink_end" })
    };
    // The feature side: the side whose ink ends at a perpendicular junction (the outline corner);
    // when both or neither do, the side with the longer ink run. The fitted carrier's extents are
    // not trusted for this, since a fit may have followed a shared extension line far past the
    // dimension lines of the stack.
    let (t_plus, end_plus) = walk(1.0, FEATURE_WALK_PX, false);
    let (t_minus, end_minus) = walk(-1.0, FEATURE_WALK_PX, false);
    let feature_sign = match (end_plus == "junction", end_minus == "junction") {
        (true, false) => 1.0,
        (false, true) => -1.0,
        _ => {
            if (t_plus - tc).abs() >= (t_minus - tc).abs() {
                1.0
            } else {
                -1.0
            }
        }
    };
    let (tf, feature_end) = if feature_sign > 0.0 { (t_plus, end_plus) } else { (t_minus, end_minus) };
    if feature_end == "none" || (tf - tc).abs() < 20.0 {
        return None;
    }
    let (to, overshoot_end) = walk(-feature_sign, OVERSHOOT_WALK_PX, true);
    // Ink running on past the overshoot walk's cap serves something else (a dimension the observer
    // did not list, another structure): this member ends at the nominal overshoot as well.
    let (to, overshoot_end) = match overshoot_end {
        "none" => (tc, overshoot_end),
        "cap" => (tc - feature_sign * NOMINAL_OVERSHOOT_PX, "cap_nominal"),
        _ => (to, overshoot_end),
    };
    let (a, b) = if feature_sign > 0.0 { (to, tf) } else { (tf, to) };
    Some(((at(a), at(b)), format!("extension_span:crossing={tc:.1},feature_end={feature_end},overshoot_end={overshoot_end}")))
}

/// Refine one stroke, point, polyline or hatch member (texts and tips have their own phases).
fn refine_member(ink: &Ink, category: Category, role: &str, shape: &Shape, text: Option<&String>, params: &RefineParams) -> RefinedMember {
    let mut out = blank(role, text, shape);
    match shape {
        Shape::Segment(p, q) => {
            let gap = if category == Category::CentreLine {
                params.centre_gap_px
            } else {
                params.max_gap_px
            };
            let fit = fit_segment(ink, *p, *q, params.corridor_px, gap);
            let (rp, rq) = if fit.moved_px > 0.0 {
                restrain_overrun(ink, (*p, *q), fit.p, fit.q)
            } else {
                (fit.p, fit.q)
            };
            out.support_before = fit.support_before;
            let restrained = (rp, rq) != (fit.p, fit.q);
            out.support_after = if restrained {
                segment_support(ink, rp, rq)
            } else {
                fit.support_after
            };
            out.moved_px = dist(*p, rp).max(dist(*q, rq));
            out.width_px = if restrained {
                measure_width(ink, rp, rq, params.corridor_px * 0.5)
            } else {
                fit.width_px
            };
            let (rp, rq) = match lines::snap_axis(rp, rq) {
                Some(((sp, sq), off)) => {
                    out.adjustments.push(format!("axis_snap:{off:.3}deg"));
                    (sp, sq)
                }
                None => (rp, rq),
            };
            out.refined = Shape::Segment(rp, rq);
            out.supported = out.support_after >= params.support_min && dist(rp, rq) >= 2.0;
            if out.supported {
                out.pattern = Some(measure_pattern(ink, rp, rq));
            }
        }
        Shape::Circle { c, r } => {
            let inked = (0..72)
                .filter(|k| {
                    let a = *k as f64 * std::f64::consts::TAU / 72.0;
                    ink.has_dark_within(c.0 + r * a.cos(), c.1 + r * a.sin(), 1.5)
                })
                .count() as f64
                / 72.0;
            out.support_before = inked;
            out.support_after = inked;
            out.supported = inked >= params.support_min;
        }
        Shape::Point(p) => {
            out.support_before = f64::from(u8::from(ink.has_dark_within(p.0, p.1, 1.5)));
            if let Some(s) = snap_point(ink, *p, params.snap_radius_px) {
                out.refined = Shape::Point(s);
                out.moved_px = dist(s, *p);
                out.support_after = 1.0;
                out.supported = true;
            }
        }
        Shape::Polyline(v) => {
            let before: f64 = v.windows(2).map(|w| segment_support(ink, w[0], w[1])).sum::<f64>()
                / (v.len() - 1).max(1) as f64;
            let snapped: Vec<P> = v
                .iter()
                .map(|p| snap_point(ink, *p, params.snap_radius_px).unwrap_or(*p))
                .collect();
            let after: f64 = snapped
                .windows(2)
                .map(|w| segment_support(ink, w[0], w[1]))
                .sum::<f64>()
                / (snapped.len() - 1).max(1) as f64;
            out.moved_px = v
                .iter()
                .zip(&snapped)
                .map(|(a, b)| dist(*a, *b))
                .fold(0.0, f64::max);
            out.refined = Shape::Polyline(snapped);
            out.support_before = before;
            out.support_after = after;
            out.supported = after >= params.support_min;
        }
        Shape::Hatch { polygon, angle_deg, spacing_px } => {
            // Density of ink inside the region's bounding box, reported; the angle and spacing are
            // measured on the ink when it holds enough parallel strokes, else the observer's stay.
            let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in polygon {
                x0 = x0.min(p.0);
                y0 = y0.min(p.1);
                x1 = x1.max(p.0);
                y1 = y1.max(p.1);
            }
            let area = ((x1 - x0 + 1.0) * (y1 - y0 + 1.0)).max(1.0);
            let density = dark_in_box(ink, [x0, y0, x1, y1]) as f64 / area;
            out.support_before = density;
            out.support_after = density;
            out.supported = density > 0.005;
            match measure_hatch(ink, polygon) {
                Some((angle, spacing, lines)) => {
                    out.refined = Shape::Hatch {
                        polygon: polygon.clone(),
                        angle_deg: angle,
                        spacing_px: spacing,
                    };
                    out.adjustments.push(format!("hatch_measured:lines={lines},proposed={angle_deg:.1}deg/{spacing_px:.1}px"));
                }
                None => out.adjustments.push("hatch_unmeasured:observer_values_kept".into()),
            }
        }
        Shape::Tip { .. } | Shape::TextBox { .. } => {}
    }
    out
}

/// Refine an arrow tip: along the fitted dimension line when there is one, else along the proposed
/// direction; then the arrowhead base on the ink.
fn refine_tip(ink: &Ink, role: &str, approx: P, along: P, dimension: Option<(P, P)>, line_width: f64, params: &RefineParams) -> RefinedMember {
    let mut along = along;
    if let Some((p, q)) = dimension
        && along != (0.0, 0.0)
        && let Some(fd) = unit(sub(q, p))
    {
        let fd = crate::observe::snap_axis(fd);
        along = if dot(fd, along) >= 0.0 { fd } else { (-fd.0, -fd.1) };
    }
    let shape = Shape::Tip { approx, along };
    let mut out = blank(role, None, &shape);
    out.support_before = f64::from(u8::from(ink.has_dark_within(approx.0, approx.1, 1.5)));
    // The head itself on the ink decides tip, base and the sense of the direction: the observer's
    // direction is only the axis (it comes from the midpoint of a line that may run on to the text).
    if along != (0.0, 0.0)
        && let Some((tip, base, dir, len)) = arrow_head(ink, approx, along, line_width, params.arrow_window_px)
    {
        out.refined = Shape::Tip { approx: tip, along: dir };
        out.moved_px = dist(tip, approx);
        out.support_after = 1.0;
        out.supported = true;
        out.arrow_base = Some(base);
        out.arrow_length_px = Some(len);
        out.adjustments.push(if dot(dir, along) < 0.0 { "arrow_head_from_ink:reversed" } else { "arrow_head_from_ink" }.into());
        return out;
    }
    let found = if along == (0.0, 0.0) {
        snap_point(ink, approx, params.arrow_window_px)
    } else {
        arrow_tip(ink, approx, along, params.arrow_window_px)
    };
    if let Some(tip) = found {
        out.refined = Shape::Tip { approx: tip, along };
        out.moved_px = dist(tip, approx);
        out.support_after = 1.0;
        out.supported = true;
        if along != (0.0, 0.0)
            && let Some((base, len)) = arrow_base(ink, tip, along, line_width)
        {
            out.arrow_base = Some(base);
            out.arrow_length_px = Some(len);
        }
    }
    out
}

/// Two segments on one line (3 px, 3°) covering at least nine tenths of the shorter one.
fn same_stroke(a: (P, P), b: (P, P)) -> bool {
    let (Some(da), Some(db)) = (unit(sub(a.1, a.0)), unit(sub(b.1, b.0))) else {
        return false;
    };
    if dot(da, db).abs() < 0.998_63 {
        return false;
    }
    let n = perp(da);
    if dot(sub(b.0, a.0), n).abs() > 3.0 || dot(sub(b.1, a.0), n).abs() > 3.0 {
        return false;
    }
    let (la, lb) = (dist(a.0, a.1), dist(b.0, b.1));
    let (t0, t1) = (dot(sub(b.0, a.0), da), dot(sub(b.1, a.0), da));
    let overlap = t0.max(t1).min(la) - t0.min(t1).max(0.0);
    overlap >= 0.9 * la.min(lb)
}

/// Cut the dimension or leader line a supported arrowhead terminates back to the arrowhead base:
/// the line whose end lies within the arrow length plus a little of the tip, when the base projects
/// inside the line (arrows outside a short dimension leave the line as fitted).
fn trim_to_arrow_bases(ink: &Ink, members: &mut [Option<RefinedMember>]) {
    let tips: Vec<(P, P, P, f64)> = members
        .iter()
        .flatten()
        .filter(|m| m.supported)
        .filter_map(|m| match (&m.refined, m.arrow_base, m.arrow_length_px) {
            (Shape::Tip { approx, along }, Some(base), Some(len)) => Some((*approx, *along, base, len)),
            _ => None,
        })
        .collect();
    for (tip, along, base, len) in tips {
        let mut best: Option<(f64, usize, bool)> = None;
        for (i, m) in members.iter().enumerate() {
            let Some(m) = m else { continue };
            if !m.supported || !matches!(m.role.as_str(), "dimension_line" | "leader_line") {
                continue;
            }
            let Shape::Segment(p, q) = m.refined else { continue };
            let Some(d) = unit(sub(q, p)) else { continue };
            if dot(d, along).abs() < 0.9 {
                continue;
            }
            let (dp, dq) = (dist(tip, p), dist(tip, q));
            let (near, at_p) = if dp <= dq { (dp, true) } else { (dq, false) };
            if near <= len + ARROW_END_SLACK_PX && best.is_none_or(|(b, _, _)| near < b) {
                best = Some((near, i, at_p));
            }
        }
        let Some((_, i, at_p)) = best else { continue };
        let Some(m) = members[i].as_mut() else { continue };
        let Shape::Segment(p, q) = m.refined else { continue };
        let Some(d) = unit(sub(q, p)) else { continue };
        let length = dist(p, q);
        let tb = dot(sub(base, p), d);
        let (np, nq) = if at_p {
            if tb > 0.5 && tb < length - 1.0 { (add(p, mul(d, tb)), q) } else { continue }
        } else if tb < length - 0.5 && tb > 1.0 {
            (p, add(p, mul(d, tb)))
        } else {
            continue;
        };
        m.refined = Shape::Segment(np, nq);
        m.support_after = segment_support(ink, np, nq);
        m.supported = m.support_after >= 0.8 && dist(np, nq) >= 2.0;
    }
}

/// A refined arrowhead found on the ink: tip, base, the direction it points, from where.
fn arrow_member(role: &str, proposed: &Shape, tip: P, base: P, how: &str) -> RefinedMember {
    let along = unit(sub(tip, base)).unwrap_or((0.0, 0.0));
    let mut m = blank(role, None, proposed);
    m.refined = Shape::Tip { approx: tip, along };
    m.support_before = 1.0;
    m.support_after = 1.0;
    m.supported = true;
    m.moved_px = dist(proposed.reference(), tip);
    m.arrow_base = Some(base);
    m.arrow_length_px = Some(dist(tip, base));
    m.adjustments.push(format!("arrow_from_ink:{how}"));
    m
}

/// The proposed arrow tip nearest `tip` within 30 px that is still unrefined.
fn claim_tip(shapes: &[ProposedMember], slots: &[Option<RefinedMember>], dropped: &[bool], tip: P) -> Option<usize> {
    shapes
        .iter()
        .enumerate()
        .filter(|(i, m)| slots[*i].is_none() && !dropped[*i] && matches!(m.shape, Shape::Tip { .. }) && m.role == "arrowhead")
        .map(|(i, m)| (i, dist(m.shape.reference(), tip)))
        .filter(|(_, d)| *d <= 30.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Place a found arrowhead: on the proposed tip it matches, or as a new member.
fn place_arrow(
    shapes: &mut Vec<ProposedMember>,
    slots: &mut Vec<Option<RefinedMember>>,
    dropped: &mut Vec<bool>,
    tip: P,
    base: P,
    how: &str,
) {
    match claim_tip(shapes, slots, dropped, tip) {
        Some(i) => slots[i] = Some(arrow_member("arrowhead", &shapes[i].shape, tip, base, how)),
        None => {
            let shape = Shape::Tip { approx: tip, along: unit(sub(tip, base)).unwrap_or((0.0, 0.0)) };
            let mut m = arrow_member("arrowhead", &shape, tip, base, how);
            m.adjustments.push("arrow_not_proposed".into());
            shapes.push(ProposedMember { role: "arrowhead".into(), shape, text: None });
            slots.push(Some(m));
            dropped.push(false);
        }
    }
}

/// Supported refined segments of a role.
fn segments_with(slots: &[Option<RefinedMember>], dropped: &[bool], role: &str) -> Vec<(usize, P, P)> {
    slots
        .iter()
        .enumerate()
        .filter(|(i, _)| !dropped[*i])
        .filter_map(|(i, m)| match m {
            Some(m) if m.supported && m.role == role => match m.refined {
                Shape::Segment(p, q) => Some((i, p, q)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The frame of a datum symbol: a circle when the ring of ink around the letter fits one (radius
/// within 0.7–1.3 of the observed half-size, RMS ≤ 1.6 px, 70 % of the circumference inked), else the
/// square of the four refined sides.
fn datum_frame(ink: &Ink, shapes: &mut Vec<ProposedMember>, slots: &mut Vec<Option<RefinedMember>>, dropped: &mut Vec<bool>) {
    let sides: Vec<usize> = (0..shapes.len()).filter(|i| shapes[*i].role == "datum_frame#").collect();
    if sides.is_empty() {
        return;
    }
    let pts: Vec<P> = sides.iter().flat_map(|i| shapes[*i].shape.points()).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in &pts {
        x0 = x0.min(p.0);
        y0 = y0.min(p.1);
        x1 = x1.max(p.0);
        y1 = y1.max(p.1);
    }
    let c = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let r0 = 0.5 * (x1 - x0).min(y1 - y0);
    let letter = shapes.iter().find_map(|m| match (&m.shape, m.role.as_str()) {
        (Shape::TextBox { bbox, .. }, "datum_letter") => Some(*bbox),
        _ => None,
    });
    let mut ring: Vec<P> = Vec::new();
    if r0 >= 6.0 {
        let (ax0, ay0) = ((c.0 - 1.4 * r0).floor() as i64, (c.1 - 1.4 * r0).floor() as i64);
        let (ax1, ay1) = ((c.0 + 1.4 * r0).ceil() as i64, (c.1 + 1.4 * r0).ceil() as i64);
        for y in ay0.max(0)..=ay1.min(ink.height as i64 - 1) {
            for x in ax0.max(0)..=ax1.min(ink.width as i64 - 1) {
                if !ink.is_dark(x, y) {
                    continue;
                }
                let pt = (x as f64, y as f64);
                let rr = dist(pt, c);
                let in_letter = letter.is_some_and(|b| pt.0 >= b[0] - 2.0 && pt.0 <= b[2] + 2.0 && pt.1 >= b[1] - 2.0 && pt.1 <= b[3] + 2.0);
                if rr >= 0.6 * r0 && rr <= 1.35 * r0 && !in_letter {
                    ring.push(pt);
                }
            }
        }
    }
    let circle = (ring.len() >= 30).then(|| crate::geom::fit_circle(&ring)).flatten().filter(|cf| {
        cf.rms <= 1.6 && cf.r >= 0.7 * r0 && cf.r <= 1.3 * r0 && dist(cf.c, c) <= 0.3 * r0 && {
            let inked = (0..72)
                .filter(|k| {
                    let a = *k as f64 * std::f64::consts::TAU / 72.0;
                    ink.has_dark_within(cf.c.0 + cf.r * a.cos(), cf.c.1 + cf.r * a.sin(), 1.5)
                })
                .count();
            inked as f64 / 72.0 >= 0.7
        }
    });
    match circle {
        Some(cf) => {
            for i in &sides {
                dropped[*i] = true;
            }
            let proposed = Shape::Circle { c, r: r0 };
            let mut m = blank("datum_frame", None, &proposed);
            m.refined = Shape::Circle { c: cf.c, r: cf.r };
            m.support_before = 1.0;
            m.support_after = 1.0;
            m.supported = true;
            m.moved_px = dist(cf.c, c).max((cf.r - r0).abs());
            m.width_px = 2.0 * cf.rms.max(1.0);
            m.pattern = Some(Pattern::Continuous);
            m.adjustments.push(format!("datum_frame:circle,rms={:.2}", cf.rms));
            shapes.push(ProposedMember { role: "datum_frame".into(), shape: proposed, text: None });
            slots.push(Some(m));
            dropped.push(false);
        }
        None => {
            for i in &sides {
                shapes[*i].role = "datum_frame".into();
                if let Some(m) = slots[*i].as_mut() {
                    m.role = "datum_frame".into();
                    m.adjustments.push("datum_frame:square".into());
                }
            }
        }
    }
}

/// The cells of a tolerance frame: interior vertical dividers are columns inked over at least 90 %
/// of the frame's inner height; each divider becomes a frame cell line, and the cell texts of the
/// observation (symbol, tolerance value, datum references) are placed in the cells found, replacing
/// the observer's evenly divided boxes. Nothing changes when fewer cells are found than texts.
fn gdt_cells(ink: &Ink, proposal: &Proposal, shapes: &mut Vec<ProposedMember>, slots: &mut Vec<Option<RefinedMember>>, dropped: &mut Vec<bool>) {
    let frame = segments_with(slots, dropped, "frame_cell");
    if frame.len() < 3 {
        return;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut widths: Vec<f64> = Vec::new();
    for (i, p, q) in &frame {
        for e in [p, q] {
            x0 = x0.min(e.0);
            y0 = y0.min(e.1);
            x1 = x1.max(e.0);
            y1 = y1.max(e.1);
        }
        if let Some(m) = &slots[*i] {
            widths.push(m.width_px);
        }
    }
    let sw = median(&mut widths).unwrap_or(3.0).clamp(1.5, 6.0);
    if x1 - x0 < 20.0 || y1 - y0 < 12.0 {
        return;
    }
    let rows: Vec<i64> = ((y0 + sw + 2.0).ceil() as i64..=(y1 - sw - 2.0).floor() as i64).collect();
    if rows.len() < 5 {
        return;
    }
    let mut dividers: Vec<f64> = Vec::new();
    let mut group: Vec<f64> = Vec::new();
    let flush = |group: &mut Vec<f64>, dividers: &mut Vec<f64>| {
        if !group.is_empty() {
            dividers.push(group.iter().sum::<f64>() / group.len() as f64);
            group.clear();
        }
    };
    for x in (x0 + sw + 4.0).ceil() as i64..=(x1 - sw - 4.0).floor() as i64 {
        let covered = rows.iter().filter(|&&y| ink.is_dark(x, y)).count() as f64 / rows.len() as f64;
        if covered >= 0.9 {
            if group.last().is_some_and(|last| x as f64 - last > 1.5) {
                flush(&mut group, &mut dividers);
            }
            group.push(x as f64);
        } else {
            flush(&mut group, &mut dividers);
        }
    }
    flush(&mut group, &mut dividers);
    if dividers.is_empty() {
        return;
    }
    for d in &dividers {
        let (p, q) = ((*d, y0), (*d, y1));
        let mut m = blank("frame_cell", None, &Shape::Segment(p, q));
        m.support_after = segment_support(ink, p, q);
        m.support_before = m.support_after;
        m.supported = m.support_after >= 0.8;
        m.width_px = sw;
        m.pattern = Some(Pattern::Continuous);
        m.adjustments.push("gdt_divider_from_ink".into());
        shapes.push(ProposedMember { role: "frame_cell".into(), shape: Shape::Segment(p, q), text: None });
        slots.push(Some(m));
        dropped.push(false);
    }
    let mut edges = vec![x0];
    edges.extend(dividers.iter().copied());
    edges.push(x1);
    let texts: Vec<String> = proposal.text.split('|').map(|t| t.trim().to_owned()).collect();
    if texts.len() > edges.len() - 1 {
        return;
    }
    for i in 0..shapes.len() {
        if matches!(shapes[i].shape, Shape::TextBox { .. })
            && matches!(shapes[i].role.as_str(), "symbol_glyph" | "tolerance_value" | "datum_reference")
        {
            dropped[i] = true;
        }
    }
    for (k, text) in texts.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let role = match k {
            0 => "symbol_glyph",
            1 => "tolerance_value",
            _ => "datum_reference",
        };
        let bbox = [edges[k] + sw + 2.0, y0 + sw + 2.0, edges[k + 1] - sw - 2.0, y1 - sw - 2.0];
        shapes.push(ProposedMember {
            role: role.into(),
            shape: Shape::TextBox { bbox, vertical: false },
            text: Some(text.clone()),
        });
        slots.push(None);
        dropped.push(false);
    }
}

/// Refine every member of a proposal in phases: strokes (snapped to an axis within 0.3°), the frame
/// of a datum symbol and the cells of a tolerance frame from the ink; then each dimension line
/// followed on the ink run by run to its own arrowheads (found from the width profile, proposed or
/// not) and cut at their bases, arrowheads at the free ends of leaders, the remaining proposed tips,
/// stroke widths robust to arrows and junctions; finally text boxes fitted to glyphs with the
/// proposal's own strokes excluded. For a surface texture symbol, the two leg chiralities proposed
/// are both fitted and only the better supported pair is kept, as `roughness_symbol`.
pub fn refine(ink: &Ink, proposal: &Proposal, params: &RefineParams) -> RefinedProposal {
    let mut shapes: Vec<ProposedMember> = proposal.members.clone();
    let n = shapes.len();
    let mut slots: Vec<Option<RefinedMember>> = vec![None; n];
    let mut dropped: Vec<bool> = vec![false; n];
    // Phase 1: strokes, points, polylines, hatch regions.
    for (i, m) in shapes.iter().enumerate() {
        if !matches!(m.shape, Shape::Tip { .. } | Shape::TextBox { .. }) {
            slots[i] = Some(refine_member(ink, proposal.category, &m.role, &m.shape, m.text.as_ref(), params));
        }
    }
    // One stroke proposed twice in one proposal (a leader polyline with a repeated vertex, a line
    // listed twice): the better supported reading stays, the other is dropped.
    for i in 0..n {
        for j in (i + 1)..n {
            let (Some(a), Some(b)) = (&slots[i], &slots[j]) else { continue };
            if a.role != b.role || !a.supported || !b.supported {
                continue;
            }
            let (Shape::Segment(pa, qa), Shape::Segment(pb, qb)) = (&a.refined, &b.refined) else { continue };
            if !same_stroke((*pa, *qa), (*pb, *qb)) {
                continue;
            }
            let drop = if b.support_after > a.support_after + 1e-9 { i } else { j };
            slots[drop] = None;
            dropped[drop] = true;
        }
    }
    // The better supported leg chirality of a surface texture symbol.
    if proposal.category == Category::SurfaceTexture {
        let score = |k: char| -> f64 {
            slots
                .iter()
                .flatten()
                .filter(|m| m.role.starts_with("roughness_symbol#") && m.role.chars().nth(17) == Some(k))
                .map(|m| m.support_after)
                .sum()
        };
        let keep = if score('1') > score('0') + 1e-9 { '1' } else { '0' };
        for (i, slot) in slots.iter_mut().enumerate() {
            let drop = slot
                .as_ref()
                .is_some_and(|m| m.role.starts_with("roughness_symbol#") && m.role.chars().nth(17) != Some(keep));
            if drop {
                *slot = None;
                dropped[i] = true;
            } else if let Some(m) = slot.as_mut()
                && m.role.starts_with("roughness_symbol#")
            {
                m.role = "roughness_symbol".into();
            }
        }
    }
    match proposal.category {
        Category::Datum => datum_frame(ink, &mut shapes, &mut slots, &mut dropped),
        Category::GdtFrame => gdt_cells(ink, proposal, &mut shapes, &mut slots, &mut dropped),
        _ => {}
    }
    // Phase 2a: dimension lines followed on the ink, with their own arrowheads.
    let text_height = shapes
        .iter()
        .find_map(|m| match (&m.shape, m.role.as_str()) {
            (Shape::TextBox { bbox, .. }, "dimension_text") => Some((bbox[2] - bbox[0]).min(bbox[3] - bbox[1])),
            _ => None,
        })
        .unwrap_or(40.0)
        .clamp(20.0, 120.0);
    let mut followed: Vec<usize> = Vec::new();
    for (i, p, q) in segments_with(&slots, &dropped, "dimension_line") {
        let Shape::Segment(pp, pq) = shapes[i].shape else { continue };
        let own: Vec<(P, P)> = segments_with(&slots, &dropped, "extension_line").into_iter().map(|(_, a, b)| (a, b)).collect();
        let sw = slots[i].as_ref().map_or(3.0, |m| if m.width_px > 0.0 { m.width_px } else { 3.0 }).clamp(1.5, 6.0);
        let Some(ext) = lines::dimension_extent(ink, (p, q), (pp, pq), sw, text_height, &own) else { continue };
        let cut = |e: &lines::LineEnd| match e.arrow {
            Some((_, base)) if e.how != "outside_arrow" => base,
            _ => e.t,
        };
        let (t0, t1) = (cut(&ext.start), cut(&ext.end));
        let (np, nq) = (ext.at(t0), ext.at(t1));
        if dist(np, nq) < 2.0 {
            continue;
        }
        followed.push(i);
        if let Some(m) = slots[i].as_mut() {
            m.refined = Shape::Segment(np, nq);
            m.span_px = Some((ext.at(ext.start.t), ext.at(ext.end.t)));
            m.support_after = segment_support(ink, np, nq);
            m.supported = m.support_after >= params.support_min;
            m.moved_px = dist(pp, np).max(dist(pq, nq));
            m.adjustments.push(format!("line_extent:start={},end={}", ext.start.how, ext.end.how));
        }
        for e in [&ext.start, &ext.end] {
            if let Some((tip, base)) = e.arrow {
                place_arrow(&mut shapes, &mut slots, &mut dropped, ext.at(tip), ext.at(base), e.how);
            }
        }
        // The line running on beyond an own extension line out to a text placed outside: a second
        // dimension-line member from the crossing.
        for &(ta, tb) in &ext.outer {
            let (op, oq) = (ext.at(ta), ext.at(tb));
            let shape = Shape::Segment(op, oq);
            let mut m = blank("dimension_line", None, &shape);
            m.refined = shape.clone();
            m.support_before = 1.0;
            m.support_after = segment_support(ink, op, oq);
            m.supported = m.support_after >= params.support_min;
            m.width_px = sw;
            m.pattern = slots[i].as_ref().and_then(|x| x.pattern);
            m.span_px = Some((op, oq));
            m.adjustments.push("dimension_line_beyond_extension_line".into());
            shapes.push(ProposedMember { role: "dimension_line".into(), shape, text: None });
            slots.push(Some(m));
            dropped.push(false);
            followed.push(slots.len() - 1);
        }
    }
    // Phase 2b: arrowheads at the free ends of leaders (an end no other leader piece meets).
    let leaders = segments_with(&slots, &dropped, "leader_line");
    for &(i, p, q) in &leaders {
        let others: Vec<P> = leaders.iter().filter(|(j, ..)| *j != i).flat_map(|(_, a, b)| [*a, *b]).collect();
        let sw = slots[i].as_ref().map_or(3.0, |m| if m.width_px > 0.0 { m.width_px } else { 3.0 }).clamp(1.5, 6.0);
        let len = dist(p, q);
        let mut ends = (p, q);
        for at_p in [true, false] {
            let (end, other) = if at_p { (ends.0, ends.1) } else { (ends.1, ends.0) };
            if others.iter().any(|o| dist(*o, end) <= 12.0) {
                continue;
            }
            let Some((tip, base)) = lines::wedge_at_end(ink, end, sub(other, end), sw, (0.7 * len).min(200.0)) else { continue };
            let Some(d) = unit(sub(other, end)) else { continue };
            let tb = dot(sub(base, end), d);
            if tb > 1.0 && tb < dist(end, other) - 2.0 {
                let nb = add(end, mul(d, tb));
                ends = if at_p { (nb, ends.1) } else { (ends.0, nb) };
            }
            place_arrow(&mut shapes, &mut slots, &mut dropped, tip, base, "leader_end");
        }
        if ends != (p, q)
            && let Some(m) = slots[i].as_mut()
        {
            m.refined = Shape::Segment(ends.0, ends.1);
            m.support_after = segment_support(ink, ends.0, ends.1);
            m.supported = m.support_after >= params.support_min;
            m.adjustments.push("cut_at_arrow_base".into());
        }
    }
    // Phase 2c: the remaining proposed tips along the dimension line, with their bases; a leader or
    // an unfollowed dimension line they terminate is cut back to the base.
    let dimension = segments_with(&slots, &dropped, "dimension_line").first().map(|(_, p, q)| (*p, *q));
    let line_width = slots
        .iter()
        .flatten()
        .filter(|m| m.supported && matches!(m.refined, Shape::Segment(..)))
        .map(|m| m.width_px)
        .fold(0.0, f64::max)
        .max(2.0);
    let mut tipped: Vec<usize> = Vec::new();
    for i in 0..shapes.len() {
        if slots[i].is_some() || dropped[i] {
            continue;
        }
        if let Shape::Tip { approx, along } = shapes[i].shape {
            slots[i] = Some(refine_tip(ink, &shapes[i].role, approx, along, dimension, line_width, params));
            tipped.push(i);
        }
    }
    if !tipped.is_empty() {
        let mut view: Vec<Option<RefinedMember>> = slots
            .iter()
            .enumerate()
            .map(|(i, m)| match m {
                Some(m) if !dropped[i] && (tipped.contains(&i) || (m.role == "leader_line") || (m.role == "dimension_line" && !followed.contains(&i))) => Some(m.clone()),
                _ => None,
            })
            .collect();
        trim_to_arrow_bases(ink, &mut view);
        for (i, m) in view.into_iter().enumerate() {
            if let Some(m) = m
                && matches!(m.role.as_str(), "leader_line" | "dimension_line")
            {
                slots[i] = Some(m);
            }
        }
    }
    // Phase 2d: stroke widths away from arrowheads and junctions.
    for i in 0..slots.len() {
        if dropped[i] {
            continue;
        }
        if let Some(m) = slots[i].as_mut()
            && m.supported
            && matches!(m.role.as_str(), "dimension_line" | "extension_line" | "leader_line")
            && let Shape::Segment(p, q) = m.refined
        {
            let w = lines::robust_width(ink, p, q, params.corridor_px * 0.5, 2.0, 2.0);
            if w > 0.0 {
                m.width_px = w;
            }
        }
    }
    // Phase 3: text boxes, with the proposal's own strokes excluded.
    let mut exclude: Vec<Segment> = Vec::new();
    for (i, m) in slots.iter().enumerate() {
        let Some(m) = m else { continue };
        if dropped[i] || !m.supported {
            continue;
        }
        match m.refined {
            Shape::Segment(p, q) => exclude.push((p, q, m.width_px.max(1.0))),
            Shape::Circle { c, r } => {
                for k in 0..24 {
                    let a0 = k as f64 * std::f64::consts::TAU / 24.0;
                    let a1 = (k + 1) as f64 * std::f64::consts::TAU / 24.0;
                    exclude.push(((c.0 + r * a0.cos(), c.1 + r * a0.sin()), (c.0 + r * a1.cos(), c.1 + r * a1.sin()), m.width_px.max(2.0) + 1.0));
                }
            }
            _ => {}
        }
    }
    for i in 0..shapes.len() {
        if dropped[i] || slots[i].is_some() {
            continue;
        }
        let m = &shapes[i];
        if let Shape::TextBox { bbox, vertical } = m.shape {
            let mut out = blank(&m.role, m.text.as_ref(), &m.shape);
            out.support_before = f64::from(u8::from(dark_in_box(ink, bbox) >= 3));
            let height = if vertical { bbox[2] - bbox[0] } else { bbox[3] - bbox[1] };
            let (refined, kept, discarded) = text_box_fit_glyphs(ink, bbox, params.text_pad_px, height, &exclude);
            out.refined = Shape::TextBox {
                bbox: refined,
                vertical,
            };
            out.moved_px = dist((bbox[0], bbox[1]), (refined[0], refined[1])).max(dist((bbox[2], bbox[3]), (refined[2], refined[3])));
            out.discarded_components = discarded;
            out.support_after = f64::from(u8::from(kept > 0));
            out.supported = kept > 0;
            slots[i] = Some(out);
        }
    }
    RefinedProposal {
        proposal: proposal.clone(),
        members: slots
            .into_iter()
            .zip(dropped)
            .filter_map(|(m, d)| if d { None } else { m })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::{ProposedMember, Provenance};

    fn ink_with(w: usize, h: usize, f: impl Fn(usize, usize) -> bool) -> Ink {
        let mut m = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                m[y * w + x] = f(x, y);
            }
        }
        Ink::from_mask(w, h, m)
    }

    fn proposal(category: Category, members: Vec<ProposedMember>) -> Proposal {
        Proposal {
            category,
            kind: "linear".into(),
            text: "30".into(),
            centre: (0.0, 0.0),
            edge_distance: 100.0,
            members,
            provenance: Provenance {
                tile: "t".into(),
                tile_sha256: String::new(),
                level: "L1A".into(),
                model: "m".into(),
                prompt_sha256: String::new(),
                file: "f".into(),
                item_index: 0,
                verification: None,
            },
            notes: vec![],
            bbox: None,
        }
    }

    #[test]
    fn patterns_are_read_from_ink_runs() {
        // Continuous, dashed (40 on / 12 off), dash-dot (120 on / 10 off / 6 on / 10 off).
        let cont = ink_with(400, 5, |_, y| y == 2);
        assert_eq!(measure_pattern(&cont, (0.0, 2.0), (399.0, 2.0)), Pattern::Continuous);
        let dashed = ink_with(400, 5, |x, y| y == 2 && x % 52 < 40);
        assert_eq!(measure_pattern(&dashed, (0.0, 2.0), (399.0, 2.0)), Pattern::Dashed);
        let dashdot = ink_with(600, 5, |x, y| {
            let m = x % 146;
            y == 2 && (m < 120 || (130..136).contains(&m))
        });
        assert_eq!(measure_pattern(&dashdot, (0.0, 2.0), (599.0, 2.0)), Pattern::DashDot);
    }

    #[test]
    fn a_fitted_extension_line_stops_at_the_gap_before_a_collinear_contour() {
        // A thick contour side on y = 50 up to x = 100, then a 7 px gap, then the thin extension
        // line from x = 108 to 200; the proposal covers the extension line only.
        let ink = ink_with(260, 100, |x, y| {
            ((48..=52).contains(&y) && (20..=100).contains(&x)) || ((49..=51).contains(&y) && (108..=200).contains(&x))
        });
        let fit = fit_segment(&ink, (112.0, 50.0), (196.0, 50.0), 12.0, 8.0);
        assert!(fit.p.0 < 30.0, "the plain fit bridges the gap: {:?}", fit.p);
        let (p, q) = restrain_overrun(&ink, ((112.0, 50.0), (196.0, 50.0)), fit.p, fit.q);
        assert!((p.0 - 108.0).abs() <= 1.5, "{p:?}");
        assert!((q.0 - 200.0).abs() <= 1.5, "{q:?}");
        // Inside the proposal nothing is cut, and a short overrun through continuous ink stays.
        let (p2, q2) = restrain_overrun(&ink, ((120.0, 50.0), (190.0, 50.0)), (108.0, 50.0), (200.0, 50.0));
        assert!((p2.0 - 108.0).abs() <= 1.5 && (q2.0 - 200.0).abs() <= 1.5, "{p2:?} {q2:?}");
        // The width over the restrained extent is that of the thin line, not of the contour.
        assert!((measure_width(&ink, p, q, 6.0) - 3.0).abs() < 1e-9);
        assert!((measure_width(&ink, (20.0, 50.0), (100.0, 50.0), 6.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn an_extension_line_stops_at_the_outline_it_touches_but_passes_a_crossing() {
        // Thin vertical line x = 100 from y = 20 to 150 touching a thick horizontal outline at
        // y = 150..156 (which continues left and right); a thin horizontal line crosses it at y = 60.
        let ink = ink_with(200, 200, |x, y| {
            ((99..=101).contains(&x) && (20..=156).contains(&y))
                || ((150..=156).contains(&y) && (40..=160).contains(&x))
                || ((59..=61).contains(&y) && (70..=130).contains(&x))
        });
        // The proposal ends above the crossing and well above the outline; the fit reaches the outline.
        let fit = fit_segment(&ink, (100.0, 25.0), (100.0, 50.0), 12.0, 8.0);
        assert!(fit.q.1 > 150.0, "{:?}", fit.q);
        let (p, q) = restrain_overrun(&ink, ((100.0, 25.0), (100.0, 50.0)), fit.p, fit.q);
        assert!((p.1 - 20.0).abs() <= 1.5, "{p:?}");
        // Capped at 35 px of overrun past the proposal end (y = 50): the crossing at 60 is passed.
        assert!(q.1 > 62.0 && q.1 <= 86.0, "{q:?}");
        // With a proposal reaching y = 140 the walk meets the outline at 150 and stops there.
        let (_, q) = restrain_overrun(&ink, ((100.0, 25.0), (100.0, 140.0)), (100.0, 20.0), (100.0, 156.0));
        assert!(q.1 >= 146.0 && q.1 <= 150.5, "{q:?}");
    }

    #[test]
    fn arrowhead_is_read_from_its_taper_whatever_the_proposed_sense_and_through_a_continuing_line() {
        // A horizontal line y = 100 from x = 20 to 260 running on through the tip of a filled head
        // (tip at x = 200, 30 px long, 12 px wide at the base) and a vertical line crossing at the tip.
        let ink = ink_with(300, 200, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let line = (99..=101).contains(&y) && (20..=260).contains(&x);
            let head = (170.0..=200.0).contains(&xf) && (yf - 100.0).abs() <= (200.0 - xf) * 0.2;
            let ext = (199..=201).contains(&x) && (60..=140).contains(&y);
            line || head || ext
        });
        for (approx, axis) in [((198.0, 101.0), (1.0, 0.0)), ((198.0, 101.0), (-1.0, 0.0)), ((215.0, 102.0), (1.0, 0.0)), ((180.0, 99.0), (-1.0, 0.0))] {
            let (tip, base, along, len) = arrow_head(&ink, approx, axis, 3.0, 20.0).unwrap_or_else(|| panic!("{approx:?} {axis:?}"));
            assert!((tip.0 - 200.0).abs() <= 2.0 && (tip.1 - 100.0).abs() <= 1.0, "{approx:?} {axis:?} tip {tip:?}");
            assert!((base.0 - 170.0).abs() <= 2.0, "{approx:?} {axis:?} base {base:?}");
            assert_eq!(along, (1.0, 0.0), "{approx:?} {axis:?}");
            assert!((len - 30.0).abs() <= 3.0, "{len}");
        }
        // A bare line has no head; a proposal far from the head finds none either.
        assert!(arrow_head(&ink, (60.0, 100.0), (1.0, 0.0), 3.0, 20.0).is_none());
        // The proposal through refine_tip: the reversed observer direction is corrected on the ink.
        let m = refine_tip(&ink, "arrowhead", (198.0, 101.0), (-1.0, 0.0), None, 3.0, &RefineParams::default());
        let Shape::Tip { approx, along } = m.refined else { panic!() };
        assert!((approx.0 - 200.0).abs() <= 2.0 && along == (1.0, 0.0) && m.arrow_length_px.is_some_and(|l| (l - 30.0).abs() <= 3.0));
        assert!(m.adjustments.iter().any(|a| a == "arrow_head_from_ink:reversed"), "{:?}", m.adjustments);
    }

    #[test]
    fn arrowhead_base_is_measured_and_the_line_is_cut_back_to_it() {
        // A horizontal line y = 100 from x = 20 to 200 ending in a filled arrowhead with its tip at
        // x = 200, 30 px long and 12 px wide at the base; a vertical extension line through the tip.
        let ink = ink_with(260, 200, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let line = (99..=101).contains(&y) && (20..=200).contains(&x);
            let head = (170.0..=200.0).contains(&xf) && (yf - 100.0).abs() <= (200.0 - xf) * 0.2;
            let ext = (199..=201).contains(&x) && (60..=140).contains(&y);
            line || head || ext
        });
        let (base, len) = arrow_base(&ink, (200.0, 100.0), (1.0, 0.0), 3.0).unwrap();
        assert!((len - 30.0).abs() <= 2.0, "{len}");
        assert!((base.0 - 170.0).abs() <= 2.0 && (base.1 - 100.0).abs() < 1e-9, "{base:?}");
        let p = proposal(Category::Dimension, vec![
            ProposedMember { role: "dimension_line".into(), shape: Shape::Segment((22.0, 100.0), (196.0, 100.0)), text: None },
            ProposedMember { role: "arrowhead".into(), shape: Shape::Tip { approx: (198.0, 101.0), along: (1.0, 0.0) }, text: None },
            ProposedMember { role: "extension_line".into(), shape: Shape::Segment((200.0, 62.0), (200.0, 138.0)), text: None },
        ]);
        let r = refine(&ink, &p, &RefineParams::default());
        let line = r.members.iter().find(|m| m.role == "dimension_line").unwrap();
        let tip = r.members.iter().find(|m| m.role == "arrowhead").unwrap();
        assert!(tip.supported && tip.arrow_length_px.is_some_and(|l| (l - 30.0).abs() <= 2.0));
        let Shape::Segment(a, b) = line.refined else { panic!() };
        assert!((a.0 - 20.0).abs() <= 1.5, "{a:?}");
        assert!((b.0 - 170.0).abs() <= 2.0, "the line stops at the arrow base: {b:?}");
        assert!(line.supported);
    }

    #[test]
    fn text_boxes_keep_glyphs_and_drop_crossing_strokes() {
        // Two glyph blobs inside the proposed box, a long horizontal line through them, and a long
        // vertical stroke leaving the box downwards from the line.
        let ink = ink_with(300, 300, |x, y| {
            ((60..=72).contains(&x) && (80..=100).contains(&y))
                || ((80..=92).contains(&x) && (80..=100).contains(&y))
                || ((89..=91).contains(&y) && (10..=290).contains(&x))
                || ((110..=112).contains(&x) && (90..=260).contains(&y))
        });
        // Without exclusions the line is split off as a pass-through stroke and the vertical stroke
        // is long and thin; the glyph pieces remain.
        let (b, kept, discarded) = text_box_fit_glyphs(&ink, [55.0, 76.0, 118.0, 104.0], 6.0, 28.0, &[]);
        assert!(kept >= 2 && discarded >= 2, "{kept} kept {discarded} discarded {b:?}");
        assert!(b[0] >= 58.0 && b[1] >= 78.0 && b[2] <= 94.0 && b[3] <= 102.0, "{b:?}");
        // With the line excluded as the proposal's own stroke the result is the same glyph box.
        let (b2, kept2, _) = text_box_fit_glyphs(&ink, [55.0, 76.0, 118.0, 104.0], 6.0, 28.0, &[((10.0, 90.0), (290.0, 90.0), 3.0)]);
        assert!(kept2 >= 2 && b2[2] <= 94.0, "{kept2} {b2:?}");
        // A box holding only the strokes keeps nothing.
        let (b3, kept3, discarded3) = text_box_fit_glyphs(&ink, [100.0, 76.0, 118.0, 104.0], 6.0, 28.0, &[]);
        assert_eq!(kept3, 0, "{b3:?}");
        assert!(discarded3 >= 2);
        assert_eq!(b3, [100.0, 76.0, 118.0, 104.0]);
    }

    #[test]
    fn unsupported_members_are_marked_not_dropped() {
        let ink = ink_with(200, 100, |x, y| y == 50 && (20..=180).contains(&x));
        let p = proposal(Category::Dimension, vec![
            ProposedMember { role: "dimension_line".into(), shape: Shape::Segment((25.0, 52.0), (175.0, 52.0)), text: None },
            ProposedMember { role: "extension_line".into(), shape: Shape::Segment((20.0, 10.0), (20.0, 40.0)), text: None },
            ProposedMember { role: "dimension_text".into(), shape: Shape::TextBox { bbox: [80.0, 10.0, 120.0, 30.0], vertical: false }, text: Some("30".into()) },
        ]);
        let r = refine(&ink, &p, &RefineParams::default());
        assert_eq!(r.members.len(), 3);
        assert!(r.members[0].supported && r.members[0].support_after > 0.99);
        assert!(matches!(r.members[0].refined, Shape::Segment((x0, y0), (x1, _)) if (y0 - 50.0).abs() < 0.6 && x0 < 21.0 && x1 > 179.0));
        assert_eq!(r.members[0].pattern, Some(Pattern::Continuous));
        assert!(!r.members[1].supported && r.members[1].support_after < 0.1);
        assert!(!r.members[2].supported, "an empty box is not a text");
    }

    #[test]
    fn hatch_angle_and_spacing_are_measured_on_the_ink() {
        // 45° hatch lines (rising to the right in the y-up frame: image direction (1, -1)), 20 px apart.
        let ink = ink_with(300, 300, |x, y| {
            let s = (x as f64 + y as f64) / 2f64.sqrt();
            let k = s / 20.0;
            (k - k.round()).abs() * 20.0 <= 1.0 && (40..=260).contains(&x) && (40..=260).contains(&y)
        });
        let polygon = vec![(50.0, 50.0), (250.0, 50.0), (250.0, 250.0), (50.0, 250.0)];
        let (angle, spacing, lines) = measure_hatch(&ink, &polygon).unwrap();
        assert!((angle - 45.0).abs() <= 2.0, "{angle}");
        assert!((spacing - 20.0).abs() <= 1.5, "{spacing}");
        assert!(lines >= 5);
        // The same hatch broken into fragments (every fourth pixel along the lines missing) and a
        // small region with three lines are still measured.
        let broken = ink_with(300, 300, |x, y| {
            let s = (x as f64 + y as f64) / 2f64.sqrt();
            let k = s / 20.0;
            (k - k.round()).abs() * 20.0 <= 1.0 && (40..=260).contains(&x) && (40..=260).contains(&y) && (x + y) % 4 != 0
        });
        let (angle, spacing, _) = measure_hatch(&broken, &polygon).unwrap();
        assert!((angle - 45.0).abs() <= 2.0 && (spacing - 20.0).abs() <= 1.5, "{angle} {spacing}");
        let small = vec![(60.0, 60.0), (110.0, 60.0), (110.0, 110.0), (60.0, 110.0)];
        let (angle, spacing, lines) = measure_hatch(&ink, &small).unwrap();
        assert!((angle - 45.0).abs() <= 3.0 && (spacing - 20.0).abs() <= 2.0 && lines >= 2, "{angle} {spacing} {lines}");
        assert!(measure_hatch(&ink, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]).is_none());
    }

    #[test]
    fn an_extension_span_runs_from_the_crossing_to_the_first_junction() {
        // A part outline: left side x = 100 from y = 200 to 400 with the top edge y = 200 from
        // x = 100 to 300. The extension line continues the side upward to y = 60, crossing a
        // dimension line y = 80 (x 60..200), with 2 mm overshoot above it. A parallel dimension line
        // y = 140 (another dimension) also crosses the extension line and must be passed.
        let ink = ink_with(400, 500, |x, y| {
            ((99..=101).contains(&x) && (60..=400).contains(&y))
                || ((199..=201).contains(&y) && (100..=300).contains(&x))
                || ((79..=81).contains(&y) && (60..=200).contains(&x))
                || ((139..=141).contains(&y) && (60..=250).contains(&x))
        });
        // The fitted carrier covers side and extension together.
        let carrier = ((100.0, 60.0), (100.0, 400.0));
        let other_dimension = ((60.0, 140.0), (250.0, 140.0));
        let (span, note) = extension_span(&ink, carrier, ((60.0, 80.0), (200.0, 80.0)), 3.0, &[other_dimension]).unwrap();
        assert!((span.0.1 - 60.0).abs() <= 1.5, "overshoot end {:?} ({note})", span.0);
        assert!((span.1.1 - 199.0).abs() <= 2.5, "feature end at the corner {:?} ({note})", span.1);
        assert!(note.contains("feature_end=junction"));
        // Without the pass-through list the parallel dimension line is taken for the junction.
        let (span, _) = extension_span(&ink, carrier, ((60.0, 80.0), (200.0, 80.0)), 3.0, &[]).unwrap();
        assert!((span.1.1 - 139.0).abs() <= 2.5, "{:?}", span.1);
        // A stacked arrangement: the same extension line goes on upward through an outer dimension
        // line at y = 40 (ink from y = 20). The inner dimension's member ends at the nominal 2 mm
        // overshoot above its own line, the outer dimension's at the ink end.
        let stacked = ink_with(400, 500, |x, y| {
            ((99..=101).contains(&x) && (20..=400).contains(&y))
                || ((199..=201).contains(&y) && (100..=300).contains(&x))
                || ((79..=81).contains(&y) && (60..=200).contains(&x))
                || ((39..=41).contains(&y) && (60..=200).contains(&x))
        });
        let carrier = ((100.0, 20.0), (100.0, 400.0));
        let inner = ((60.0, 80.0), (200.0, 80.0));
        let outer = ((60.0, 40.0), (200.0, 40.0));
        let (span, note) = extension_span(&stacked, carrier, inner, 3.0, &[outer]).unwrap();
        assert!((span.0.1 - (80.0 - 23.6)).abs() <= 1.5, "{:?} ({note})", span.0);
        assert!(note.contains("overshoot_end=shared_nominal"));
        let (span, note) = extension_span(&stacked, carrier, outer, 3.0, &[inner]).unwrap();
        assert!((span.0.1 - 20.0).abs() <= 1.5, "{:?} ({note})", span.0);
        assert!((span.1.1 - 199.0).abs() <= 2.5, "{:?} ({note})", span.1);
        // A crossing just past an arrow-trimmed short dimension line is still accepted.
        let short = ((130.0, 80.0), (170.0, 80.0));
        assert!(extension_span(&stacked, carrier, short, 3.0, &[outer]).is_some());
    }
}
