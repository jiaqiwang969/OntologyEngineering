//! Line-pattern inference: group collinear line pieces and co-circular arc pieces, then read the
//! dash-length and gap statistics of each regular run.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use crate::Ink;
use crate::geom::{P, add, dist, dot, median, mul, perp, sub, undirected_angle, unit, wrap_pi};
use crate::linetypes::{bounds, steps};
use crate::stroke::{Pattern, Primitive, Stroke, line_direction};

/// Collinearity: directions within 3° and endpoints within 1.5 px of the reference line.
const ANGLE_TOL: f64 = 0.052_36;
const OFFSET_TOL: f64 = 1.5;
/// Two pieces belong to the same candidate run when their gap is at most
/// `GAP_FACTOR × longer piece + GAP_FLOOR` (dash gaps are shorter than dashes in every standard).
const GAP_FACTOR: f64 = 1.5;
const GAP_FLOOR: f64 = 3.0;
/// Runs whose median gap is below this are touching fragments of one continuous stroke.
const TOUCH_GAP: f64 = 1.0;
// The line-type bounds (dash, long dash, dot and gap lengths on paper; gap ratios; the blank-gap share) are knowledge:
// `crate::linetypes::bounds()`, read from knowledge/line-types.json with their sources.

/// What the pattern inference reads besides the strokes: the page resolution, for the paper lengths of the line
/// types, and the original page ink, on which a gap between pieces must be blank to be a dash gap. Without the
/// original ink every gap counts as blank (strokes vectorized from the page itself).
#[derive(Clone, Copy)]
pub struct PatternContext<'a> {
    pub px_per_mm: f64,
    pub original: Option<&'a Ink>,
    /// Whether a point lies in a view of the drawing (`None`: everywhere). The line-type knowledge of this round is read
    /// in the views; the frame and the title block keep the round-3 reading.
    pub in_view: Option<&'a dyn Fn(P) -> bool>,
}

impl Default for PatternContext<'_> {
    fn default() -> Self {
        PatternContext { px_per_mm: 300.0 / 25.4, original: None, in_view: None }
    }
}

impl PatternContext<'_> {
    /// Whether the fourth round's reading of discontinuous lines applies to pieces at these points: at the evaluation
    /// step that has it (scope item 3) and in a view.
    fn round4(&self, mut points: impl Iterator<Item = P>) -> bool {
        steps().chain_short_dash && self.in_view.is_none_or(|f| points.all(f))
    }

    /// Whether the original ink is blank along the sample points of a gap.
    fn blank(&self, points: impl Iterator<Item = P>) -> bool {
        let Some(ink) = self.original else {
            return true;
        };
        let (mut n, mut dark) = (0usize, 0usize);
        for p in points {
            n += 1;
            if ink.has_dark_within(p.0, p.1, 1.0) {
                dark += 1;
            }
        }
        n == 0 || (dark as f64) <= bounds().blank_gap_max_dark * n as f64
    }

    /// Whether the original ink fills a gap along all its sample points (but the blank share): the line ran on through
    /// it and a crossing erased before vectorizing cut it there. A gap with blank stretches is a dash gap, even when a
    /// crossing or a dot lies in it.
    fn filled(&self, points: impl Iterator<Item = P>) -> bool {
        let Some(ink) = self.original else {
            return false;
        };
        let (mut n, mut dark) = (0usize, 0usize);
        for p in points {
            n += 1;
            if ink.has_dark_within(p.0, p.1, 1.0) {
                dark += 1;
            }
        }
        n > 0 && (dark as f64) >= (1.0 - bounds().blank_gap_max_dark) * n as f64
    }
}

/// Co-circularity tolerances (absolute px or relative to the radius, whichever is larger).
const CIRCLE_CENTRE_TOL: f64 = 2.0;
const CIRCLE_RADIUS_TOL: f64 = 1.5;
const CIRCLE_REL_TOL: f64 = 0.02;

/// Union–find with path halving (deterministic).
pub(crate) struct Dsu(Vec<usize>);

impl Dsu {
    pub fn new(n: usize) -> Dsu {
        Dsu((0..n).collect())
    }

    pub fn find(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }

    pub fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Smaller root wins so the result does not depend on call order.
            let (lo, hi) = (ra.min(rb), ra.max(rb));
            self.0[hi] = lo;
        }
    }
}

/// Assign patterns to all strokes: collinear line runs first, then co-circular runs (arcs plus the
/// short line pieces that dots of a dash-dot circle become).
pub(crate) fn assign(strokes: &mut [Stroke], ctx: &PatternContext) {
    assign_collinear(strokes, ctx);
    assign_cocircular(strokes, ctx);
}

/// Sample points every 2 px over the central half of a straight gap: the ink of a piece reaches past its thinned,
/// vectorized end by up to half its width, which is not the line continuing.
fn gap_points_line(a: P, b: P) -> Vec<P> {
    let l = dist(a, b);
    let Some(d) = unit(sub(b, a)) else { return Vec::new() };
    let clear = (0.25 * l).max(1.5);
    let (from, to) = (clear, l - clear);
    if to <= from {
        return Vec::new();
    }
    let n = ((to - from) / 2.0).ceil().max(1.0) as usize;
    (0..=n).map(|k| add(a, mul(d, from + (to - from) * k as f64 / n as f64))).collect()
}

/// Sample points along all of a straight gap, 1.5 px clear of its ends, every pixel.
fn gap_points_full_line(a: P, b: P) -> Vec<P> {
    let l = dist(a, b);
    let Some(d) = unit(sub(b, a)) else { return Vec::new() };
    if l <= 3.0 {
        return Vec::new();
    }
    let n = (l - 3.0).ceil().max(1.0) as usize;
    (0..=n).map(|k| add(a, mul(d, 1.5 + (l - 3.0) * k as f64 / n as f64))).collect()
}

fn assign_collinear(strokes: &mut [Stroke], ctx: &PatternContext) {
    let idx: Vec<usize> = (0..strokes.len())
        .filter(|&i| line_direction(&strokes[i].primitive).is_some())
        .collect();
    let n = idx.len();
    if n < 3 {
        return;
    }
    let mut dsu = Dsu::new(n);
    for a in 0..n {
        for b in a + 1..n {
            if collinear_pair(&strokes[idx[a]].primitive, &strokes[idx[b]].primitive) {
                dsu.union(a, b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &i) in idx.iter().enumerate() {
        groups.entry(dsu.find(k)).or_default().push(i);
    }
    for members in groups.values() {
        if members.len() < 3 {
            continue;
        }
        let reference = longest(strokes, members);
        let Primitive::Line { p: c, q } = strokes[reference].primitive else {
            continue;
        };
        let Some(d) = unit(sub(q, c)) else {
            continue;
        };
        let mut items: Vec<(f64, f64, usize)> = members
            .iter()
            .filter_map(|&i| match strokes[i].primitive {
                Primitive::Line { p, q } => {
                    let t0 = dot(sub(p, c), d);
                    let t1 = dot(sub(q, c), d);
                    Some((t0.min(t1), t0.max(t1), i))
                }
                Primitive::Arc { .. } => None,
            })
            .collect();
        items.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.2.cmp(&b.2))
        });
        let order: Vec<usize> = items.iter().map(|it| it.2).collect();
        let lengths: Vec<f64> = items.iter().map(|it| it.1 - it.0).collect();
        let gaps: Vec<f64> = items
            .windows(2)
            .map(|w| (w[1].0 - w[0].1).max(0.0))
            .collect();
        let at = |t: f64| add(c, mul(d, t));
        // A gap at whose edges other lines end is where features break the line (an outline between grooves), not a
        // dash gap: lines meet a dashed line on its dashes, and a CAD crossing may fall in a gap but runs on through it.
        let edge = bounds().closure_join_paper_mm * ctx.px_per_mm;
        let blank: Vec<bool> = items
            .windows(2)
            .map(|w| {
                ctx.blank(gap_points_line(at(w[0].1), at(w[1].0)).into_iter())
                    && !(ctx.round4([strokes[w[0].2].primitive.mid(), strokes[w[1].2].primitive.mid()].into_iter())
                        && line_ends_in_gap(strokes, &|i| members.contains(&i), c, d, w[0].1, w[1].0, edge, true))
            })
            .collect();
        let filled: Vec<bool> = items.windows(2).map(|w| ctx.filled(gap_points_full_line(at(w[0].1), at(w[1].0)).into_iter())).collect();
        apply_runs(strokes, &order, &lengths, &gaps, &blank, &filled, ctx);
    }
}

/// Whether other lines end at a gap of the line through `c` with direction `d` (between `t0` and `t1` along it, within
/// `edge` of it, ends of strokes `skip` excepted): strokes at least `edge` long, not running along the line, with an end
/// at the gap or in it — and not continued from that point on the line's other side by another stroke (a crossing line
/// the skeleton split where it meets this one). With `both_edges`, ends at both edges of the gap on one side are needed
/// (the walls of a groove breaking an outline, two strokes of letters): one line ending in a gap is a CAD intersection
/// falling in a gap and leaves it a dash gap; without, any end at the gap or in it counts (the strict reading of the
/// two-dash rule). A line meeting this one on a dash, or crossing it in a gap, never counts (DC-G1-061: lines meet a
/// dashed line on its dashes; CAD crossings may fall in gaps).
#[allow(clippy::too_many_arguments)]
pub(crate) fn line_ends_in_gap(strokes: &[Stroke], skip: &dyn Fn(usize) -> bool, c: P, d: P, t0: f64, t1: f64, edge: f64, both_edges: bool) -> bool {
    let n = perp(d);
    let side = |pt: P| dot(sub(pt, c), n);
    let ends = |s: &Stroke| [(s.primitive.start(), s.primitive.end()), (s.primitive.end(), s.primitive.start())];
    // Every end at the gap: its position along the line and the side its line leaves to.
    let mut found: Vec<(f64, f64)> = Vec::new();
    for (i, s) in strokes.iter().enumerate() {
        if skip(i) || s.primitive.length() < edge || line_direction(&s.primitive).is_some_and(|di| undirected_angle(di, d) <= 0.35) {
            continue;
        }
        for &(e, far) in &ends(s) {
            let t = dot(sub(e, c), d);
            if side(e).abs() > edge || t < t0 - 2.0 || t > t1 + 2.0 {
                continue;
            }
            let continued = strokes.iter().enumerate().any(|(k, o)| {
                k != i
                    && !skip(k)
                    && ends(o).iter().any(|&(e2, far2)| dist(e2, e) <= edge && side(far2) * side(far) < 0.0)
            });
            if !continued {
                found.push((t, side(far).signum()));
            }
        }
    }
    if !both_edges {
        return !found.is_empty();
    }
    found.iter().any(|&(ta, sa)| (ta - t0).abs() <= edge && found.iter().any(|&(tb, sb)| (tb - t1).abs() <= edge && sb == sa))
}

fn longest(strokes: &[Stroke], members: &[usize]) -> usize {
    let mut best = members[0];
    for &i in &members[1..] {
        if strokes[i].primitive.length() > strokes[best].primitive.length() {
            best = i;
        }
    }
    best
}

/// Whether two line pieces are collinear and close enough along the line to be dashes of one run.
fn collinear_pair(a: &Primitive, b: &Primitive) -> bool {
    let (Primitive::Line { p: pa, q: qa }, Primitive::Line { p: pb, q: qb }) = (a, b) else {
        return false;
    };
    let (la, lb) = (dist(*pa, *qa), dist(*pb, *qb));
    let (rp, rq, sp, sq, lr) = if la >= lb {
        (*pa, *qa, *pb, *qb, la)
    } else {
        (*pb, *qb, *pa, *qa, lb)
    };
    let (Some(d), Some(ds)) = (unit(sub(rq, rp)), unit(sub(sq, sp))) else {
        return false;
    };
    if undirected_angle(d, ds) > ANGLE_TOL {
        return false;
    }
    let n = perp(d);
    if dot(sub(sp, rp), n).abs() > OFFSET_TOL || dot(sub(sq, rp), n).abs() > OFFSET_TOL {
        return false;
    }
    let (ta, tb) = (dot(sub(sp, rp), d), dot(sub(sq, rp), d));
    let (t0, t1) = (ta.min(tb), ta.max(tb));
    let gap = (t0 - lr).max(-t1);
    gap <= GAP_FACTOR * lr + GAP_FLOOR
}

/// Cut a sorted sequence of pieces into runs: pieces touching one another (gap of at most one pixel beyond the touch
/// gap), or apart by a gap the original page ink fills (`filled`), are first one element — a dash the skeleton split
/// where another line crosses it, or one a crossing erased before vectorizing cut (lines cross a dashed line on its
/// dashes; the line runs on through a filled gap, which is no dash gap); then the elements are cut where a gap is longer than any dash gap on paper
/// (the pieces on either side are separate lines) and, within each such stretch, where a gap leaves a factor two of the
/// stretch's own median gap (1 px slack); every run of at least three elements is classified.
#[allow(clippy::too_many_arguments)]
fn apply_runs(strokes: &mut [Stroke], order: &[usize], lengths: &[f64], gaps: &[f64], blank: &[bool], filled: &[bool], ctx: &PatternContext) {
    if order.is_empty() {
        return;
    }
    let mut elements: Vec<(Vec<usize>, f64)> = vec![(vec![order[0]], lengths[0])];
    let (mut egaps, mut eblank): (Vec<f64>, Vec<bool>) = (Vec::new(), Vec::new());
    // (The joining of pieces serves the chain line's dashes, scope item 3; the round-3 reading keeps every piece.)
    for k in 0..gaps.len() {
        let join = ctx.round4([strokes[order[k]].primitive.mid(), strokes[order[k + 1]].primitive.mid()].into_iter());
        if join && (gaps[k] <= TOUCH_GAP + 1.0 || filled[k]) {
            let last = elements.last_mut().expect("an element");
            last.0.push(order[k + 1]);
            last.1 += gaps[k] + lengths[k + 1];
        } else {
            elements.push((vec![order[k + 1]], lengths[k + 1]));
            egaps.push(gaps[k]);
            eblank.push(blank[k]);
        }
    }
    let max_gap = bounds().max_dash_gap_paper_mm * ctx.px_per_mm;
    let mut start = 0usize;
    for i in 0..=egaps.len() {
        if i < egaps.len() && egaps[i] <= max_gap {
            continue;
        }
        // The stretch start..=i: elements joined by gaps no longer than a dash gap.
        let mut g = egaps[start..i].to_vec();
        let gm = median(&mut g).unwrap_or(0.0);
        let mut run_start = start;
        for j in start..=i {
            let spread = bounds().piece_spread;
            let cut = j == i || egaps[j] > spread * gm + 1.0 || egaps[j] < gm / spread - 1.0;
            if cut {
                let members: Vec<usize> = elements[run_start..=j].iter().flat_map(|e| e.0.iter().copied()).collect();
                let lens: Vec<f64> = elements[run_start..=j].iter().map(|e| e.1).collect();
                classify_run(strokes, &members, &lens, &egaps[run_start..j], &eblank[run_start..j], ctx);
                run_start = j + 1;
            }
        }
        start = i + 1;
    }
}

/// Classify one run: `members` are its strokes, `lengths` its elements (touching fragments joined), at least three.
fn classify_run(strokes: &mut [Stroke], members: &[usize], lengths: &[f64], gaps: &[f64], blank: &[bool], ctx: &PatternContext) {
    if lengths.len() < 3 {
        return;
    }
    let mut g = gaps.to_vec();
    let gm = median(&mut g).unwrap_or(0.0);
    if gm < TOUCH_GAP {
        return;
    }
    // Gaps the original page inks are crossings erased before vectorizing: a run that is not mostly blank between
    // its pieces is one line cut where others crossed it.
    if blank.iter().filter(|b| **b).count() * 4 < gaps.len() * 3 {
        return;
    }
    let round4 = ctx.round4(members.iter().map(|&i| strokes[i].primitive.mid()));
    let pattern = dash_pattern(lengths, gm, ctx, round4);
    if pattern == Pattern::Continuous {
        return;
    }
    for &i in members {
        strokes[i].pattern = pattern;
    }
}

/// Dash statistics of one regular run with median gap `gap` (bounds from the line-type knowledge): similar lengths
/// (interior pieces within a factor two of the median, end pieces may be truncated) are `Dashed` when the gap is at least
/// the dashed family's share of the median piece and the piece no longer than a dash on paper, else `Continuous` (a line
/// cut by crossings). Long pieces alternating with short ones are a chain line (`DashDot`) when the short ones are dots
/// or short dashes about as long as the gaps (textbook p16: a very short dash may replace the dot; CAD CENTER), the
/// short ones much shorter than the long ones and the long ones no longer than a long dash on paper, else `Continuous`.
/// Two short elements between long dashes (the double chain family, CAD PHANTOM) are marked `DashDot` as well, so that
/// the run is merged; the measurement on the page ink records the family and does not class it as a centre line.
/// Anything else is `Unknown`.
fn dash_pattern(lengths: &[f64], gap: f64, ctx: &PatternContext, round4: bool) -> Pattern {
    let b = bounds();
    let n = lengths.len();
    let mut sorted = lengths.to_vec();
    let lmed = median(&mut sorted).unwrap_or(0.0);
    let spread = b.piece_spread;
    let interior_ok = lengths[1..n - 1]
        .iter()
        .all(|&l| l >= lmed / spread && l <= spread * lmed);
    let ends_ok = lengths[0] <= spread * lmed && lengths[n - 1] <= spread * lmed;
    if interior_ok && ends_ok {
        return if gap >= b.dashed_min_gap_to_dash * lmed && lmed <= b.dashed_max_dash_paper_mm * ctx.px_per_mm {
            Pattern::Dashed
        } else {
            Pattern::Continuous
        };
    }
    let lmin = sorted[0];
    let lmax = sorted[n - 1];
    let thr = 0.5 * (lmin + lmax);
    let long: Vec<bool> = lengths.iter().map(|&l| l > thr).collect();
    if !round4 {
        return dash_pattern_round3(lengths, &long, ctx);
    }
    let medians = || {
        let mut longs: Vec<f64> = lengths.iter().zip(&long).filter(|&(_, &l)| l).map(|(&x, _)| x).collect();
        let mut shorts: Vec<f64> = lengths.iter().zip(&long).filter(|&(_, &l)| !l).map(|(&x, _)| x).collect();
        (median(&mut longs).unwrap_or(0.0), median(&mut shorts).unwrap_or(0.0))
    };
    // A chain line's short element: a dot, or a short dash about as long as the gaps beside it.
    let chain_short = |lm: f64, sm: f64| {
        sm <= b.chain_max_short_to_long * lm
            && (sm <= b.chain_dot_max_paper_mm * ctx.px_per_mm || sm <= b.chain_max_short_to_gap * gap)
            && lm <= b.chain_max_long_paper_mm * ctx.px_per_mm
    };
    if short_groups(&long) == Some(2) {
        let (lm, sm) = medians();
        if chain_short(lm, sm) {
            return Pattern::DashDot;
        }
    }
    let alternating = long.windows(2).all(|w| w[0] != w[1]);
    if alternating && long.iter().any(|&l| l) && long.iter().any(|&l| !l) {
        let (lm, sm) = medians();
        if sm <= b.chain_max_short_to_long * lm {
            return if chain_short(lm, sm) { Pattern::DashDot } else { Pattern::Continuous };
        }
    }
    Pattern::Unknown
}

/// The round-3 reading of alternating pieces, kept for evaluation step 0 with round 3's constants: strictly alternating
/// long and short pieces whose short ones are at most half the long ones are a chain line (`DashDot`) when the short ones
/// are dots of at most 20 px and the long ones no longer than a long dash on paper, else `Continuous`.
fn dash_pattern_round3(lengths: &[f64], long: &[bool], ctx: &PatternContext) -> Pattern {
    let alternating = long.windows(2).all(|w| w[0] != w[1]);
    if alternating && long.iter().any(|&l| l) && long.iter().any(|&l| !l) {
        let mut longs: Vec<f64> = lengths.iter().zip(long).filter(|&(_, &l)| l).map(|(&x, _)| x).collect();
        let mut shorts: Vec<f64> = lengths.iter().zip(long).filter(|&(_, &l)| !l).map(|(&x, _)| x).collect();
        let (lm, sm) = (median(&mut longs).unwrap_or(0.0), median(&mut shorts).unwrap_or(0.0));
        if sm <= 0.5 * lm {
            return if sm <= 20.0 && lm <= bounds().chain_max_long_paper_mm * ctx.px_per_mm { Pattern::DashDot } else { Pattern::Continuous };
        }
    }
    Pattern::Unknown
}

/// The number of short elements between consecutive long ones when it is the same everywhere (the run starting and
/// ending with long elements, at least one group): 1 for a chain line, 2 for a double chain line.
pub(crate) fn short_groups(long: &[bool]) -> Option<usize> {
    if long.len() < 3 || !long[0] || !long[long.len() - 1] {
        return None;
    }
    let mut groups: Vec<usize> = Vec::new();
    let mut count = 0usize;
    for &l in &long[1..] {
        if l {
            groups.push(count);
            count = 0;
        } else {
            count += 1;
        }
    }
    let first = *groups.first()?;
    (first > 0 && groups.iter().all(|&g| g == first)).then_some(first)
}

fn circle_of(s: &Stroke) -> (P, f64) {
    match s.primitive {
        Primitive::Arc { c, r, .. } => (c, r),
        Primitive::Line { .. } => ((0.0, 0.0), 0.0),
    }
}

fn same_circle(ca: P, ra: f64, cb: P, rb: f64) -> bool {
    let r = ra.max(rb);
    dist(ca, cb) <= CIRCLE_CENTRE_TOL.max(CIRCLE_REL_TOL * r)
        && (ra - rb).abs() <= CIRCLE_RADIUS_TOL.max(CIRCLE_REL_TOL * r)
}

fn assign_cocircular(strokes: &mut [Stroke], ctx: &PatternContext) {
    let arcs: Vec<usize> = (0..strokes.len())
        .filter(|&i| matches!(strokes[i].primitive, Primitive::Arc { .. }))
        .collect();
    if arcs.is_empty() {
        return;
    }
    let mut dsu = Dsu::new(arcs.len());
    for a in 0..arcs.len() {
        for b in a + 1..arcs.len() {
            let (ca, ra) = circle_of(&strokes[arcs[a]]);
            let (cb, rb) = circle_of(&strokes[arcs[b]]);
            if same_circle(ca, ra, cb, rb) {
                dsu.union(a, b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, &i) in arcs.iter().enumerate() {
        groups.entry(dsu.find(k)).or_default().push(i);
    }
    let lines: Vec<usize> = (0..strokes.len())
        .filter(|&i| matches!(strokes[i].primitive, Primitive::Line { .. }))
        .collect();
    for members in groups.values() {
        let reference = longest(strokes, members);
        let (c, r) = circle_of(&strokes[reference]);
        if !r.is_finite() || r <= 0.0 {
            continue;
        }
        let on_circle = |p: P| (dist(p, c) - r).abs() <= OFFSET_TOL;
        // Angular intervals (a0, a1) with a1 > a0, plus the member index.
        let mut items: Vec<(f64, f64, usize)> = Vec::new();
        for &i in members {
            if let Primitive::Arc {
                start_rad,
                sweep_rad,
                ..
            } = strokes[i].primitive
            {
                let (a0, a1) = if sweep_rad >= 0.0 {
                    (start_rad, start_rad + sweep_rad)
                } else {
                    (start_rad + sweep_rad, start_rad)
                };
                items.push((a0.rem_euclid(TAU), a1 - a0, i));
            }
        }
        // Dots of a dash-dot circle come out as short chords lying on the circle.
        for &i in &lines {
            let Primitive::Line { p, q } = strokes[i].primitive else {
                continue;
            };
            let mid = ((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5);
            if dist(p, q) <= 0.5 * r && on_circle(p) && on_circle(q) && on_circle(mid) {
                let ap = (p.1 - c.1).atan2(p.0 - c.0);
                let aq = (q.1 - c.1).atan2(q.0 - c.0);
                let span = wrap_pi(aq - ap);
                let a0 = if span >= 0.0 { ap } else { aq };
                items.push((a0.rem_euclid(TAU), span.abs(), i));
            }
        }
        if items.len() < 3 {
            continue;
        }
        // items: (start angle in [0, 2π), angular length, index); sort around the circle.
        items.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.2.cmp(&b.2))
        });
        let m = items.len();
        // Cyclic gaps in px; the largest gap is where the linear sequence is cut.
        let cyc_gaps: Vec<f64> = (0..m)
            .map(|k| {
                let end = items[k].0 + items[k].1;
                let next = if k + 1 < m {
                    items[k + 1].0
                } else {
                    items[0].0 + TAU
                };
                ((next - end) * r).max(0.0)
            })
            .collect();
        let mut cut = 0usize;
        for k in 1..m {
            if cyc_gaps[k] > cyc_gaps[cut] {
                cut = k;
            }
        }
        let rotated: Vec<usize> = (0..m).map(|k| (cut + 1 + k) % m).collect();
        let order: Vec<usize> = rotated.iter().map(|&k| items[k].2).collect();
        let lengths: Vec<f64> = rotated.iter().map(|&k| items[k].1 * r).collect();
        let gaps: Vec<f64> = rotated[..m - 1].iter().map(|&k| cyc_gaps[k]).collect();
        // The gap after piece k runs along the circle from its end to the next piece's start.
        let blank: Vec<bool> = rotated[..m - 1]
            .iter()
            .map(|&k| {
                let from = items[k].0 + items[k].1;
                let span = cyc_gaps[k] / r;
                // The central half of the gap, as for straight gaps.
                let clear = (0.25 * span).max(1.5 / r);
                let (a, b) = (from + clear, from + span - clear);
                let n = (((b - a) * r) / 2.0).ceil().max(1.0) as usize;
                ctx.blank((0..=n).filter(|_| b > a).map(|j| {
                    let t = a + (b - a) * j as f64 / n as f64;
                    (c.0 + r * t.cos(), c.1 + r * t.sin())
                }))
            })
            .collect();
        // All of the gap along the circle, 1.5 px clear of its ends.
        let filled: Vec<bool> = rotated[..m - 1]
            .iter()
            .map(|&k| {
                let from = items[k].0 + items[k].1;
                let span = cyc_gaps[k] / r;
                let clear = 1.5 / r;
                let (a, b) = (from + clear, from + span - clear);
                let n = ((b - a) * r).ceil().max(1.0) as usize;
                b > a
                    && ctx.filled((0..=n).map(|j| {
                        let t = a + (b - a) * j as f64 / n as f64;
                        (c.0 + r * t.cos(), c.1 + r * t.sin())
                    }))
            })
            .collect();
        apply_runs(strokes, &order, &lengths, &gaps, &blank, &filled, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dash_pattern_at(lengths: &[f64], gap: f64) -> Pattern {
        dash_pattern(lengths, gap, &PatternContext::default(), steps().chain_short_dash)
    }

    #[test]
    fn dash_statistics() {
        assert_eq!(dash_pattern_at(&[20.0, 20.0, 20.0, 20.0], 5.0), Pattern::Dashed);
        assert_eq!(
            dash_pattern_at(&[12.0, 20.0, 20.0, 20.0, 9.0], 5.0),
            Pattern::Dashed
        );
        assert_eq!(
            dash_pattern_at(&[30.0, 3.0, 30.0, 3.0, 30.0], 5.0),
            Pattern::DashDot
        );
        assert_eq!(
            dash_pattern_at(&[30.0, 3.0, 3.0, 30.0, 30.0], 5.0),
            Pattern::Unknown
        );
    }

    #[test]
    fn a_line_cut_by_crossings_is_not_dashed() {
        // The I1 contour circle (r = 291 px) between its erased centre and dimension lines: pieces of
        // 113–310 px with gaps of 13–21 px, which also alternate long/short.
        assert_eq!(
            dash_pattern_at(&[267.0, 113.0, 310.0, 167.0], 14.0),
            Pattern::Continuous
        );
        // A real hidden line of the 085 page: dashes of 78 px with gaps of about 20 px (12d : 3d).
        assert_eq!(dash_pattern_at(&[78.0, 78.0, 78.0, 78.0], 20.0), Pattern::Dashed);
        // Alternating pieces whose short ones are not dots are a cut line, not a centre line.
        assert_eq!(
            dash_pattern_at(&[200.0, 60.0, 200.0, 60.0, 200.0], 14.0),
            Pattern::Continuous
        );
        // Dots of a 细点画线: 24d long dashes, dots within the bound, gaps 3d.
        assert_eq!(
            dash_pattern_at(&[96.0, 6.0, 96.0, 6.0, 96.0], 12.0),
            Pattern::DashDot
        );
    }

    fn line(x0: f64, x1: f64, y: f64) -> Stroke {
        Stroke { primitive: Primitive::Line { p: (x0, y), q: (x1, y) }, width_px: 3.0, pattern: Pattern::Continuous, support: 1.0, pixels: (x1 - x0) as usize }
    }

    #[test]
    fn a_dash_run_is_found_beside_unrelated_pieces_on_the_same_row() {
        // The 085 hidden line: dashes of 78 px every 115 px; far along the same row, touching fragments of outline
        // (gaps of 2 px) that a row-wide median gap would let swamp the dash gaps.
        let mut strokes: Vec<Stroke> = (0..10).map(|k| line(100.0 + 22.0 * k as f64, 120.0 + 22.0 * k as f64, 50.0)).collect();
        strokes.extend((0..6).map(|k| line(2600.0 + 115.0 * k as f64, 2678.0 + 115.0 * k as f64, 50.0)));
        assign(&mut strokes, &PatternContext::default());
        assert!(strokes[10..].iter().all(|s| s.pattern == Pattern::Dashed), "{:?}", strokes[10..].iter().map(|s| s.pattern).collect::<Vec<_>>());
        assert!(strokes[..10].iter().all(|s| s.pattern != Pattern::Dashed));
    }

    #[test]
    fn gaps_the_original_page_inks_are_crossings_not_dash_gaps() {
        // A solid line drawn on the page, cut into pieces of 100 px with gaps of 15 px where crossings were erased.
        let (w, h) = (700usize, 60usize);
        let mut mask = vec![false; w * h];
        for y in 29..=31 {
            for x in 20..640 {
                mask[y * w + x] = true;
            }
        }
        let page = Ink::from_mask(w, h, mask);
        let cut = || (0..5).map(|k| line(20.0 + 115.0 * k as f64, 120.0 + 115.0 * k as f64, 30.0)).collect::<Vec<_>>();
        let mut strokes = cut();
        assign(&mut strokes, &PatternContext::default());
        assert!(strokes.iter().all(|s| s.pattern == Pattern::Dashed), "without the page every gap counts");
        let mut strokes = cut();
        assign(&mut strokes, &PatternContext { px_per_mm: 300.0 / 25.4, original: Some(&page), in_view: None });
        assert!(strokes.iter().all(|s| s.pattern == Pattern::Continuous), "{:?}", strokes.iter().map(|s| s.pattern).collect::<Vec<_>>());
    }

    #[test]
    fn a_dash_is_at_most_a_paper_length() {
        // Pieces of 200 px (17 mm at 300 dpi) with gaps of 40 px are not the dashes of a hidden line; at 600 dpi the
        // same pixels are 8.5 mm dashes and are.
        let run = || (0..5).map(|k| line(240.0 * k as f64, 240.0 * k as f64 + 200.0, 30.0)).collect::<Vec<_>>();
        let mut strokes = run();
        assign(&mut strokes, &PatternContext::default());
        assert!(strokes.iter().all(|s| s.pattern != Pattern::Dashed));
        let mut strokes = run();
        assign(&mut strokes, &PatternContext { px_per_mm: 600.0 / 25.4, original: None, in_view: None });
        assert!(strokes.iter().all(|s| s.pattern == Pattern::Dashed));
    }
}
