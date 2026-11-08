//! Refinement of proposed drawing-expression members (segments, points, arrow tips, text boxes) on
//! the ink, and erasure of recognised members before the remaining ink is vectorised.

use std::collections::BTreeMap;

use crate::Ink;
use crate::geom::{
    P, add, clip_line_to_box, dist, dist_point_segment, dot, fit_line, median, mul, perp, sub, unit,
};
use crate::skeleton::N8;

/// Support radius: a sample point is supported when a dark pixel centre lies within this distance.
pub(crate) const SUPPORT_RADIUS: f64 = 1.5;
/// Corridor half-widths are clamped to this range (pixels).
const CORRIDOR_RANGE: (f64, f64) = (0.5, 64.0);
/// A fit whose direction differs from the proposal by more than this is rejected (another structure
/// dominated the corridor); 25 degrees.
const MAX_ROTATION_COS: f64 = 0.906_307_787;
/// Components with fewer pixels than this are specks for `text_box_fit`.
const MIN_COMPONENT_PIXELS: usize = 3;
/// Extra radius beyond half the width when erasing a stroke. Small enough that the 8-neighbours of
/// a 1-px diagonal stroke (at 0.707 px) survive.
const ERASE_MARGIN: f64 = 0.2;

/// Result of refining a proposed segment on the ink.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// Refined start point.
    pub p: (f64, f64),
    /// Refined end point.
    pub q: (f64, f64),
    /// Fraction of 1-px samples along the proposal `p`–`q` with ink within 1.5 px.
    pub support_before: f64,
    /// Fraction of 1-px samples along the refined segment with ink within 1.5 px.
    pub support_after: f64,
    /// Larger displacement of the two endpoints, in pixels.
    pub moved_px: f64,
    /// Median stroke thickness along the refined segment, in pixels (0 when nothing was fitted).
    pub width_px: f64,
}

/// Fraction of 1-px samples along `p`–`q` with ink within 1.5 px.
pub(crate) fn segment_support(ink: &Ink, p: P, q: P) -> f64 {
    let len = dist(p, q);
    if !len.is_finite() {
        return 0.0;
    }
    let n = (len.ceil() as usize).max(1) + 1;
    let mut hits = 0usize;
    for i in 0..n {
        let t = i as f64 / (n - 1) as f64;
        let x = p.0 + (q.0 - p.0) * t;
        let y = p.1 + (q.1 - p.1) * t;
        if ink.has_dark_within(x, y, SUPPORT_RADIUS) {
            hits += 1;
        }
    }
    hits as f64 / n as f64
}

/// Refine a proposed segment `p`–`q` on the ink.
///
/// 1. Dark pixels inside the corridor (`|s| ≤ corridor_px`, `t ∈ [-corridor, len + corridor]`
///    around the proposal) are fitted by an orthogonal least-squares line; the fit is repeated on the
///    pixels within `max(1, 2·median|residual| + 0.5)` px of the first line so crossing strokes do
///    not tilt it.
/// 2. The proposal endpoints are projected onto the fitted line.
/// 3. Each endpoint is trimmed inward to the ink and then extended outward along the line, 1-px bin
///    by bin, across empty runs of at most `max_gap_px` bins, to the last pixel with ink within 1.5 px
///    of the line. The endpoint is the projection of that extreme pixel centre.
/// 4. `width_px` is the median, over 1-px bins along the fitted extent, of the number of dark pixels
///    in the bin within the inlier band, i.e. ink area per unit length, which equals the stroke
///    thickness for solid strokes of any orientation.
///
/// When the corridor holds no usable ink, or the fitted direction turns more than 25° away from the
/// proposal, the proposal is returned unchanged with `moved_px = 0`, `width_px = 0` and
/// `support_after == support_before`, so the caller can see that nothing was refined. The corridor
/// is clamped to `[0.5, 64]` px.
pub fn fit_segment(
    ink: &Ink,
    p: (f64, f64),
    q: (f64, f64),
    corridor_px: f64,
    max_gap_px: f64,
) -> Fit {
    let support_before = segment_support(ink, p, q);
    let unchanged = Fit {
        p,
        q,
        support_before,
        support_after: support_before,
        moved_px: 0.0,
        width_px: 0.0,
    };
    if !(corridor_px.is_finite() && max_gap_px.is_finite()) {
        return unchanged;
    }
    let corridor = corridor_px.clamp(CORRIDOR_RANGE.0, CORRIDOR_RANGE.1);
    let max_gap = max_gap_px.max(0.0);
    let len = dist(p, q);
    let Some(d0) = unit(sub(q, p)) else {
        return unchanged;
    };
    let pts = corridor_pixels(ink, p, d0, -corridor, len + corridor, corridor);
    if pts.len() < 2 {
        return unchanged;
    }
    let Some(first) = fit_line(&pts) else {
        return unchanged;
    };
    let n1 = perp(first.d);
    let mut res: Vec<f64> = pts
        .iter()
        .map(|&x| dot(sub(x, first.c), n1).abs())
        .collect();
    let med = median(&mut res).unwrap_or(0.0);
    let thr = (2.0 * med + 0.5).max(1.0).min(corridor);
    let inliers: Vec<P> = pts
        .iter()
        .copied()
        .filter(|&x| dot(sub(x, first.c), n1).abs() <= thr)
        .collect();
    let fit = match fit_line(&inliers) {
        Some(f) if inliers.len() >= 2 => f,
        _ => first,
    };
    let mut d = fit.d;
    if dot(d, d0) < 0.0 {
        d = mul(d, -1.0);
    }
    if dot(d, d0) < MAX_ROTATION_COS {
        return unchanged;
    }
    let c = fit.c;
    let tp = dot(sub(p, c), d);
    let tq = dot(sub(q, c), d);
    let bins = line_bins(ink, c, d, thr.max(SUPPORT_RADIUS));
    if bins.is_empty() {
        return unchanged;
    }
    let Some(tq2) = end_along(&bins, tq, tp, 1, max_gap) else {
        return unchanged;
    };
    let Some(tp2) = end_along(&bins, tp, tq, -1, max_gap) else {
        return unchanged;
    };
    if tq2.partial_cmp(&tp2) != Some(std::cmp::Ordering::Greater) {
        return unchanged;
    }
    let p2 = add(c, mul(d, tp2));
    let q2 = add(c, mul(d, tq2));
    let (b0, b1) = (tp2.round() as i64, tq2.round() as i64);
    let mut counts: Vec<f64> = bins
        .range(b0..=b1)
        .filter(|(_, b)| b.count > 0)
        .map(|(_, b)| b.count as f64)
        .collect();
    let width_px = median(&mut counts).unwrap_or(0.0);
    Fit {
        p: p2,
        q: q2,
        support_before,
        support_after: segment_support(ink, p2, q2),
        moved_px: dist(p, p2).max(dist(q, q2)),
        width_px,
    }
}

/// Dark pixel centres with `t ∈ [t0, t1]` and `|s| ≤ r` in the frame (`origin`, `d`, left normal),
/// sorted in raster order without duplicates.
fn corridor_pixels(ink: &Ink, origin: P, d: P, t0: f64, t1: f64, r: f64) -> Vec<P> {
    let n = perp(d);
    let mut px: Vec<(i64, i64)> = Vec::new();
    let mut t = t0 - 0.5;
    while t <= t1 + 0.5 {
        let mut s = -r - 0.5;
        while s <= r + 0.5 {
            let pt = add(origin, add(mul(d, t), mul(n, s)));
            let (x, y) = (pt.0.round() as i64, pt.1.round() as i64);
            if ink.is_dark(x, y) {
                let v = sub((x as f64, y as f64), origin);
                let tt = dot(v, d);
                let ss = dot(v, n);
                if tt >= t0 && tt <= t1 && ss.abs() <= r {
                    px.push((y, x));
                }
            }
            s += 0.5;
        }
        t += 0.5;
    }
    px.sort_unstable();
    px.dedup();
    px.into_iter().map(|(y, x)| (x as f64, y as f64)).collect()
}

/// Ink statistics of one 1-px bin along a fitted line.
struct Bin {
    seen: Vec<(i32, i32)>,
    /// Some pixel within 1.5 px of the line.
    near: bool,
    t_min: f64,
    t_max: f64,
    /// Pixels within the width band.
    count: usize,
}

/// Bin the dark pixels within `band` of the infinite line `c + t·d` over the whole image by
/// `round(t)`.
fn line_bins(ink: &Ink, c: P, d: P, band: f64) -> BTreeMap<i64, Bin> {
    let mut bins: BTreeMap<i64, Bin> = BTreeMap::new();
    let m = band + 1.0;
    let Some((t0, t1)) = clip_line_to_box(
        c,
        d,
        -m,
        -m,
        ink.width as f64 - 1.0 + m,
        ink.height as f64 - 1.0 + m,
    ) else {
        return bins;
    };
    let n = perp(d);
    let mut t = t0;
    while t <= t1 {
        let mut s = -band - 0.5;
        while s <= band + 0.5 {
            let pt = add(c, add(mul(d, t), mul(n, s)));
            let (x, y) = (pt.0.round() as i64, pt.1.round() as i64);
            if ink.is_dark(x, y) {
                let v = sub((x as f64, y as f64), c);
                let tt = dot(v, d);
                let ss = dot(v, n).abs();
                if ss <= band {
                    let bin = bins.entry(tt.round() as i64).or_insert_with(|| Bin {
                        seen: Vec::new(),
                        near: false,
                        t_min: f64::INFINITY,
                        t_max: f64::NEG_INFINITY,
                        count: 0,
                    });
                    let pix = (x as i32, y as i32);
                    if !bin.seen.contains(&pix) {
                        bin.seen.push(pix);
                        bin.count += 1;
                        if ss <= SUPPORT_RADIUS {
                            bin.near = true;
                            bin.t_min = bin.t_min.min(tt);
                            bin.t_max = bin.t_max.max(tt);
                        }
                    }
                }
            }
            s += 0.5;
        }
        t += 0.5;
    }
    bins
}

/// Find the ink end in direction `dir` (+1 towards larger t) starting from the projected proposal
/// endpoint `t_start`: anchor on the first ink bin at or inward of it (towards `t_other`), or just
/// outside within the gap allowance, then extend across empty runs of at most `max_gap` bins.
fn end_along(
    bins: &BTreeMap<i64, Bin>,
    t_start: f64,
    t_other: f64,
    dir: i64,
    max_gap: f64,
) -> Option<f64> {
    let near = |b: i64| bins.get(&b).is_some_and(|x| x.near);
    let lo = *bins.keys().next()?;
    let hi = *bins.keys().next_back()?;
    let start = t_start.round() as i64;
    let other = t_other.round() as i64;
    let mut anchor = None;
    let mut b = start;
    loop {
        if near(b) {
            anchor = Some(b);
            break;
        }
        if (b - other) * dir <= 0 {
            break;
        }
        b -= dir;
    }
    if anchor.is_none() {
        let reach = max_gap.ceil() as i64 + 1;
        for k in 1..=reach {
            let b = start + dir * k;
            if near(b) {
                anchor = Some(b);
                break;
            }
        }
    }
    let mut anchor = anchor?;
    let mut empty = 0i64;
    let mut b = anchor + dir;
    while b >= lo && b <= hi {
        if near(b) {
            anchor = b;
            empty = 0;
        } else {
            empty += 1;
            if empty as f64 > max_gap {
                break;
            }
        }
        b += dir;
    }
    let bin = bins.get(&anchor)?;
    Some(if dir > 0 { bin.t_max } else { bin.t_min })
}

/// Centre of the dark pixel nearest to `p` within `radius_px`, or `None`. Ties resolve to the first
/// pixel in raster order.
pub fn snap_point(ink: &Ink, p: (f64, f64), radius_px: f64) -> Option<(f64, f64)> {
    ink.nearest_dark(p.0, p.1, radius_px)
        .map(|(x, y)| (x as f64, y as f64))
}

/// Apex of a filled arrowhead.
///
/// `along` is the direction the arrow points (towards the tip). The 8-connected ink component
/// containing the dark pixel nearest to `approx_tip`, restricted to a disc of radius `window_px`, is
/// examined; among its pixels within `max(1.5, window/2)` px of the axis through `approx_tip`, the
/// one furthest along `along` is the tip (ties: nearest the axis, then raster order). An extension
/// line crossing exactly at the tip adds at most half its thickness; erase it first for the best
/// result. `None` when no dark pixel lies in the window or `along` is zero.
pub fn arrow_tip(
    ink: &Ink,
    approx_tip: (f64, f64),
    along: (f64, f64),
    window_px: f64,
) -> Option<(f64, f64)> {
    let a = unit(along)?;
    if !(window_px.is_finite() && window_px > 0.0) {
        return None;
    }
    let w = window_px.min(256.0);
    let (sx, sy) = ink.nearest_dark(approx_tip.0, approx_tip.1, w)?;
    let comp = component(ink, (sx as i64, sy as i64), |x, y| {
        dist((x as f64, y as f64), approx_tip) <= w
    });
    let n = perp(a);
    let band = (0.5 * w).max(SUPPORT_RADIUS);
    let mut best: Option<(f64, f64, (i64, i64))> = None;
    for &(x, y) in &comp {
        let v = sub((x as f64, y as f64), approx_tip);
        let t = dot(v, a);
        let s = dot(v, n).abs();
        if s > band {
            continue;
        }
        let better = match best {
            None => true,
            Some((bt, bs, _)) => t > bt + 1e-9 || ((t - bt).abs() <= 1e-9 && s < bs - 1e-9),
        };
        if better {
            best = Some((t, s, (x, y)));
        }
    }
    best.map(|(_, _, (x, y))| (x as f64, y as f64))
}

/// 8-connected dark component containing `seed`, restricted to pixels accepted by `inside`, in
/// raster order.
fn component(ink: &Ink, seed: (i64, i64), inside: impl Fn(i64, i64) -> bool) -> Vec<(i64, i64)> {
    use std::collections::{BTreeSet, VecDeque};
    let mut out = Vec::new();
    if !ink.is_dark(seed.0, seed.1) || !inside(seed.0, seed.1) {
        return out;
    }
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    seen.insert((seed.1, seed.0));
    queue.push_back(seed);
    while let Some((x, y)) = queue.pop_front() {
        out.push((y, x));
        for (dx, dy) in N8 {
            let n = (x + dx, y + dy);
            if ink.is_dark(n.0, n.1) && inside(n.0, n.1) && seen.insert((n.1, n.0)) {
                queue.push_back(n);
            }
        }
    }
    out.sort_unstable();
    out.into_iter().map(|(y, x)| (x, y)).collect()
}

/// 8-connected dark components inside the pixel rectangle (inclusive bounds), each in raster order,
/// components ordered by their first pixel.
fn components_in_rect(ink: &Ink, x0: i64, y0: i64, x1: i64, y1: i64) -> Vec<Vec<(i64, i64)>> {
    let rw = (x1 - x0 + 1) as usize;
    let rh = (y1 - y0 + 1) as usize;
    let mut seen = vec![false; rw * rh];
    let local = |x: i64, y: i64| (y - y0) as usize * rw + (x - x0) as usize;
    let mut comps = Vec::new();
    let mut stack = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            if !ink.is_dark(x, y) || seen[local(x, y)] {
                continue;
            }
            seen[local(x, y)] = true;
            stack.push((x, y));
            let mut comp = Vec::new();
            while let Some((cx, cy)) = stack.pop() {
                comp.push((cy, cx));
                for (dx, dy) in N8 {
                    let (nx, ny) = (cx + dx, cy + dy);
                    if nx < x0 || ny < y0 || nx > x1 || ny > y1 || !ink.is_dark(nx, ny) {
                        continue;
                    }
                    let li = local(nx, ny);
                    if !seen[li] {
                        seen[li] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            comp.sort_unstable();
            comps.push(comp.into_iter().map(|(y, x)| (x, y)).collect());
        }
    }
    comps
}

fn normalise_box(b: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    (b.0.min(b.2), b.1.min(b.3), b.0.max(b.2), b.1.max(b.3))
}

/// Tighten a proposed text box to the ink it contains.
///
/// Dark components inside the box expanded by `pad_px` are kept when they have at least 3 pixels
/// (isolated specks are ignored) and at least half of their pixels lie inside the proposed box
/// (strokes merely passing the margin are ignored). The result is the bounding box of the kept pixel
/// centres (add 0.5 on each side for pixel footprints). When nothing is kept the (normalised)
/// proposal is returned unchanged.
pub fn text_box_fit(ink: &Ink, bbox: (f64, f64, f64, f64), pad_px: f64) -> (f64, f64, f64, f64) {
    let (x0, y0, x1, y1) = normalise_box(bbox);
    if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
        return (x0, y0, x1, y1);
    }
    let pad = if pad_px.is_finite() {
        pad_px.max(0.0)
    } else {
        0.0
    };
    let rx0 = ((x0 - pad).floor() as i64).max(0);
    let ry0 = ((y0 - pad).floor() as i64).max(0);
    let rx1 = ((x1 + pad).ceil() as i64).min(ink.width as i64 - 1);
    let ry1 = ((y1 + pad).ceil() as i64).min(ink.height as i64 - 1);
    if rx0 > rx1 || ry0 > ry1 {
        return (x0, y0, x1, y1);
    }
    let mut kept: Option<(i64, i64, i64, i64)> = None;
    for comp in components_in_rect(ink, rx0, ry0, rx1, ry1) {
        if comp.len() < MIN_COMPONENT_PIXELS {
            continue;
        }
        let inside = comp
            .iter()
            .filter(|&&(x, y)| {
                let (xf, yf) = (x as f64, y as f64);
                xf >= x0 && xf <= x1 && yf >= y0 && yf <= y1
            })
            .count();
        if inside * 2 < comp.len() {
            continue;
        }
        for &(x, y) in &comp {
            kept = Some(match kept {
                None => (x, y, x, y),
                Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
            });
        }
    }
    match kept {
        Some((a, b, c, d)) => (a as f64, b as f64, c as f64, d as f64),
        None => (x0, y0, x1, y1),
    }
}

/// A recognised stroke to erase: `(p, q, width_px)`.
pub type Segment = ((f64, f64), (f64, f64), f64);

/// Clear the ink covered by recognised members: for each `(p, q, width_px)` every pixel whose centre
/// is strictly closer than `width/2 + 0.2` px to the segment (round caps), and for each box every
/// pixel whose centre lies inside it (inclusive). Used to subtract recognised members before the
/// remaining structure is vectorised.
pub fn erase(ink: &mut Ink, segments: &[Segment], boxes: &[(f64, f64, f64, f64)]) {
    for &(p, q, w) in segments {
        if ![p.0, p.1, q.0, q.1, w].iter().all(|v| v.is_finite()) {
            continue;
        }
        let r = w.max(0.0) * 0.5 + ERASE_MARGIN;
        let x0 = ((p.0.min(q.0) - r).floor() as i64).max(0);
        let y0 = ((p.1.min(q.1) - r).floor() as i64).max(0);
        let x1 = ((p.0.max(q.0) + r).ceil() as i64).min(ink.width as i64 - 1);
        let y1 = ((p.1.max(q.1) + r).ceil() as i64).min(ink.height as i64 - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                if ink.is_dark(x, y) && dist_point_segment((x as f64, y as f64), p, q) < r {
                    ink.set_dark(x as usize, y as usize, false);
                }
            }
        }
    }
    for &b in boxes {
        let (x0, y0, x1, y1) = normalise_box(b);
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            continue;
        }
        let xs = (x0.ceil() as i64).max(0);
        let ys = (y0.ceil() as i64).max(0);
        let xe = (x1.floor() as i64).min(ink.width as i64 - 1);
        let ye = (y1.floor() as i64).min(ink.height as i64 - 1);
        for y in ys..=ye {
            for x in xs..=xe {
                ink.set_dark(x as usize, y as usize, false);
            }
        }
    }
}
