//! View callouts found on the ink around observed letters: the projection arrow beside a direction
//! letter (with the cutting-plane stroke at its tail when it marks a section), the detail circle of a
//! detail letter with the leader from the letter's shelf, the bar between the letter and the scale of a
//! detail-view title, and the letter and 向 parts of a direction-view title.

use std::collections::BTreeMap;

use crate::Ink;
use crate::geom::{P, add, dist, dot, fit_circle, fit_line, mul, perp, sub, unit};
use crate::lines::wedge_at_end;
use crate::members::fit_segment;
use crate::pattern::Dsu;
use crate::refine::text_box_fit_glyphs;
use crate::stroke::Primitive;
use crate::vector::vectorize;

fn centre(b: [f64; 4]) -> P {
    ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5)
}

/// Distance from a point to a box (0 inside).
fn box_distance(b: [f64; 4], p: P) -> f64 {
    let dx = (b[0] - p.0).max(0.0).max(p.0 - b[2]);
    let dy = (b[1] - p.1).max(0.0).max(p.1 - b[3]);
    dx.hypot(dy)
}

/// Distance from a segment to a box (0 when they touch), sampled every 2 px.
fn segment_box_distance(b: [f64; 4], p: P, q: P) -> f64 {
    let n = (dist(p, q) / 2.0).ceil().max(1.0) as usize;
    (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64;
            box_distance(b, (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t))
        })
        .fold(f64::INFINITY, f64::min)
}

/// How far the ink runs on from `p` along `d` (dark within 1.5 px of the axis, 1-px steps), at most `max`.
fn ink_reach(ink: &Ink, p: P, d: P, max: f64) -> f64 {
    let mut t = 0.0;
    while t < max {
        let q = add(p, mul(d, t + 1.0));
        if !ink.has_dark_within(q.0, q.1, 1.5) {
            break;
        }
        t += 1.0;
    }
    t
}

/// Arcs (centre, radius, start, sweep) and lines of the ink in a square window around a letter, the
/// letter itself left out, page pixels. A nearly straight arc (sweep up to 30°, sagitta up to 1.5 px,
/// as a shelf ending in a junction may be fitted) is a line between its ends.
struct WindowStrokes {
    lines: Vec<(P, P)>,
    arcs: Vec<(P, f64, f64, f64)>,
}

fn window_strokes(ink: &Ink, letter: [f64; 4], half: f64) -> Option<WindowStrokes> {
    let c = centre(letter);
    let x0 = ((c.0 - half).floor() as i64).max(0);
    let y0 = ((c.1 - half).floor() as i64).max(0);
    let x1 = ((c.0 + half).ceil() as i64).min(ink.width as i64 - 1);
    let y1 = ((c.1 + half).ceil() as i64).min(ink.height as i64 - 1);
    if x1 - x0 < 20 || y1 - y0 < 20 {
        return None;
    }
    let (w, hh) = ((x1 - x0 + 1) as usize, (y1 - y0 + 1) as usize);
    let mut mask = vec![false; w * hh];
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (xf, yf) = (x as f64, y as f64);
            let in_letter = xf >= letter[0] - 2.0 && xf <= letter[2] + 2.0 && yf >= letter[1] - 2.0 && yf <= letter[3] + 2.0;
            if !in_letter && ink.is_dark(x, y) {
                mask[(y - y0) as usize * w + (x - x0) as usize] = true;
            }
        }
    }
    let offset = (x0 as f64, y0 as f64);
    let mut out = WindowStrokes { lines: Vec::new(), arcs: Vec::new() };
    for s in vectorize(&Ink::from_mask(w, hh, mask), 4.0) {
        match s.primitive {
            Primitive::Arc { c, r, start_rad, sweep_rad } if sweep_rad.abs() <= 0.52 && r * (1.0 - (0.5 * sweep_rad).cos()) <= 1.5 => {
                let end = |a: f64| add(add(c, offset), mul((a.cos(), a.sin()), r));
                out.lines.push((end(start_rad), end(start_rad + sweep_rad)));
            }
            Primitive::Arc { c, r, start_rad, sweep_rad } => out.arcs.push((add(c, offset), r, start_rad, sweep_rad)),
            Primitive::Line { p, q } => out.lines.push((add(p, offset), add(q, offset))),
        }
    }
    Some(out)
}

/// A projection arrow, page pixels: the tail end of its shaft, the base and the tip of its head, and
/// the cutting-plane stroke at the tail when the arrow marks a section plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewArrow {
    pub tail: P,
    pub base: P,
    pub tip: P,
    pub cutting: Option<(P, P)>,
}

/// The head at the `end` of a shaft running from `tail`: following the ink on from the end (through
/// the head when the stroke stops at its base) to where it stops, a filled wedge must end in a tip
/// there (`wedge_at_end`). Returns the tip and the base.
fn head_at(ink: &Ink, end: P, tail: P, sw: f64) -> Option<(P, P)> {
    let d = unit(sub(end, tail))?;
    let len = dist(end, tail);
    let reach = ink_reach(ink, end, d, 0.7 * len + 60.0);
    wedge_at_end(ink, add(end, mul(d, reach)), mul(d, -1.0), sw, len + reach)
}

/// The projection arrow beside a letter. The ink in a window of six letter heights around the letter
/// (the letter left out) is vectorised; a line 0.5 to 40 heights long (at least 25 px) passing within
/// three heights of the letter is the shaft of an arrow when a filled head ends in a tip at one of its
/// ends and not at the other (a dimension line has two). The nearest such shaft is the arrow; a line
/// perpendicular to it with an end at its tail is the cutting-plane stroke of a section (GB/T 4458.6).
pub(crate) fn find_view_arrow(ink: &Ink, letter: [f64; 4], h: f64, sw: f64) -> Option<ViewArrow> {
    let ws = window_strokes(ink, letter, (6.0 * h).clamp(150.0, 600.0))?;
    let min_len = (0.5 * h).max(25.0);
    let mut candidates: Vec<(f64, P, P)> = ws
        .lines
        .iter()
        .filter(|(p, q)| (min_len..=40.0 * h).contains(&dist(*p, *q)))
        .map(|(p, q)| (segment_box_distance(letter, *p, *q), *p, *q))
        .filter(|(d, ..)| *d <= 3.0 * h)
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, p, q) in candidates {
        for (end, tail) in [(p, q), (q, p)] {
            let Some((tip, base)) = head_at(ink, end, tail, sw) else { continue };
            if dist(tail, base) < 0.5 * min_len || head_at(ink, tail, end, sw).is_some() {
                continue;
            }
            let d = unit(sub(end, tail)).unwrap_or((1.0, 0.0));
            let cutting = cutting_stroke(&ws.lines, tail, d, h);
            return Some(ViewArrow { tail, base, tip, cutting });
        }
    }
    None
}

/// The cutting-plane stroke at the tail of a section arrow: a line perpendicular to the shaft
/// (direction `d`), 0.4 to 4 letter heights long (at least 15 px), with an end within 0.3 heights
/// (at least 10 px) of the tail.
fn cutting_stroke(lines: &[(P, P)], tail: P, d: P, h: f64) -> Option<(P, P)> {
    let reach = (0.3 * h).max(10.0);
    let near_end = |m: P, n: P| dist(tail, m).min(dist(tail, n));
    lines
        .iter()
        .filter(|(m, n)| {
            let l = dist(*m, *n);
            l >= (0.4 * h).max(15.0) && l <= 4.0 * h && unit(sub(*n, *m)).is_some_and(|u| dot(u, d).abs() < 0.12) && near_end(*m, *n) <= reach
        })
        .min_by(|a, b| near_end(a.0, a.1).total_cmp(&near_end(b.0, b.1)))
        .copied()
}

/// A detail callout found from its letter, page pixels: the circle and the leader from the letter's
/// shelf to it as polyline vertices (empty when the letter stands beside the circle).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DetailCallout {
    pub c: P,
    pub r: f64,
    pub leader: Vec<P>,
}

/// Share of 120 points on the circumference with ink within 2 px.
fn inked_ring(ink: &Ink, c: P, r: f64) -> f64 {
    let inked = (0..120)
        .filter(|k| {
            let a = *k as f64 * std::f64::consts::TAU / 120.0;
            ink.has_dark_within(c.0 + r * a.cos(), c.1 + r * a.sin(), 2.0)
        })
        .count();
    inked as f64 / 120.0
}

/// A circle refitted to the dark pixels of its ring (within 3 px, then 2 px); `None` when the ring
/// has too few pixels or the fit moves away (radius off by more than 15 %, centre by a fifth of it).
fn refine_circle(ink: &Ink, c0: P, r0: f64) -> Option<(P, f64)> {
    let (mut c, mut r) = (c0, r0);
    for band in [3.0, 2.0] {
        let x0 = ((c.0 - r - band).floor() as i64).max(0);
        let y0 = ((c.1 - r - band).floor() as i64).max(0);
        let x1 = ((c.0 + r + band).ceil() as i64).min(ink.width as i64 - 1);
        let y1 = ((c.1 + r + band).ceil() as i64).min(ink.height as i64 - 1);
        let mut pts: Vec<P> = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                if ink.is_dark(x, y) && ((x as f64 - c.0).hypot(y as f64 - c.1) - r).abs() <= band {
                    pts.push((x as f64, y as f64));
                }
            }
        }
        if pts.len() < 12 {
            return None;
        }
        let fit = fit_circle(&pts)?;
        if (fit.r / r0 - 1.0).abs() > 0.15 || dist(fit.c, c0) > 0.2 * r0 {
            return None;
        }
        (c, r) = (fit.c, fit.r);
    }
    Some((c, r))
}

/// Circles of a window: co-circular arcs (centres and radii within 4 % or a few px, radius 8 px to
/// ten letter heights) covering at least 120°, refitted to the ink of their ring and inked along
/// 80 % of the circumference; one per circle.
fn circles_on_ink(ink: &Ink, arcs: &[(P, f64, f64, f64)], h: f64) -> Vec<(P, f64)> {
    let arcs: Vec<(P, f64, f64, f64)> = arcs.iter().copied().filter(|a| a.1 >= 8.0 && a.1 <= 10.0 * h).collect();
    let mut dsu = Dsu::new(arcs.len());
    for i in 0..arcs.len() {
        for j in i + 1..arcs.len() {
            let (ci, ri, ..) = arcs[i];
            let (cj, rj, ..) = arcs[j];
            let r = ri.max(rj);
            if dist(ci, cj) <= (0.04 * r).max(4.0) && (ri - rj).abs() <= (0.04 * r).max(3.0) {
                dsu.union(i, j);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..arcs.len() {
        groups.entry(dsu.find(i)).or_default().push(i);
    }
    let mut out: Vec<(P, f64)> = Vec::new();
    for members in groups.values() {
        let mut bins = [false; 360];
        let (mut wsum, mut cx, mut cy, mut rr) = (0.0, 0.0, 0.0, 0.0);
        for &i in members {
            let (ac, r, start, sweep) = arcs[i];
            let steps = (sweep.abs().to_degrees().ceil() as usize).max(1);
            for k in 0..=steps {
                let a = start + sweep * k as f64 / steps as f64;
                bins[(a.to_degrees().rem_euclid(360.0) as usize).min(359)] = true;
            }
            let wgt = r * sweep.abs();
            wsum += wgt;
            cx += ac.0 * wgt;
            cy += ac.1 * wgt;
            rr += r * wgt;
        }
        if bins.iter().filter(|b| **b).count() < 120 || wsum <= 0.0 {
            continue;
        }
        let Some((c, r)) = refine_circle(ink, (cx / wsum, cy / wsum), rr / wsum) else { continue };
        if inked_ring(ink, c, r) < 0.8 || out.iter().any(|(oc, or)| dist(*oc, c) <= 3.0 && (or - r).abs() <= 3.0) {
            continue;
        }
        out.push((c, r));
    }
    out
}

/// Follow a straight stroke on the ink from its end `e` along `d` to the circle (c, r): steps of 2 px,
/// each re-centred on the ink nearest the predicted point (within 2.5 px), the direction refitted on
/// the points followed; gaps up to 10 px (crossings, breaks) are passed. The point where the stroke
/// meets the circumference, `None` when the ink stops first, the circle is not ahead or is farther than `max`.
fn follow_to_circle(ink: &Ink, e: P, d: P, c: P, r: f64, max: f64) -> Option<P> {
    let ahead = dot(sub(c, e), d);
    let aside = dot(sub(c, e), perp(d)).abs();
    if ahead <= 0.0 || aside > r + 30.0 || ahead - r > max || dist(e, c) <= r {
        return None;
    }
    let (mut pos, mut dir) = (e, d);
    let mut followed: Vec<P> = vec![e];
    let (mut gap, mut travelled) = (0.0, 0.0);
    while travelled <= max {
        if dist(pos, c) <= r + 2.0 {
            return Some(add(c, mul(unit(sub(pos, c))?, r)));
        }
        let predicted = add(pos, mul(dir, 2.0));
        let n = perp(dir);
        let offset = (-5..=5)
            .map(|k| k as f64 * 0.5)
            .filter(|s| {
                let q = add(predicted, mul(n, *s));
                ink.is_dark(q.0.round() as i64, q.1.round() as i64)
            })
            .min_by(|a, b| a.abs().total_cmp(&b.abs()));
        travelled += 2.0;
        match offset {
            Some(s) => {
                pos = add(predicted, mul(n, 0.5 * s));
                gap = 0.0;
                followed.push(pos);
                if followed.len() >= 6
                    && let Some(fit) = fit_line(&followed)
                {
                    dir = if dot(fit.d, d) >= 0.0 { fit.d } else { mul(fit.d, -1.0) };
                }
            }
            None => {
                pos = predicted;
                gap += 2.0;
                if gap > 10.0 {
                    return None;
                }
            }
        }
    }
    None
}

/// Where a stroke `(p, q)` reaches the circle (c, r): an end on the circumference (within 6 px), or
/// the ink followed on from an end (`ends`: both, or only `q`) meets it within `max` px.
fn reaches(ink: &Ink, (p, q): (P, P), both_ends: bool, c: P, r: f64, max: f64) -> Option<P> {
    let ends: Vec<(P, P)> = if both_ends { vec![(p, q), (q, p)] } else { vec![(q, p)] };
    for &(e, _) in &ends {
        if (dist(e, c) - r).abs() <= 6.0 {
            return unit(sub(e, c)).map(|u| add(c, mul(u, r)));
        }
    }
    ends.iter().find_map(|&(e, o)| unit(sub(e, o)).and_then(|d| follow_to_circle(ink, e, d, c, r, max)))
}

/// Directions in which a straight stroke leaves the point `e`: of 180 directions (2° apart), those
/// along which the points 4 to 30 px out have ink within 1.5 px nine times in ten; each run of such
/// directions gives its middle one.
fn stroke_directions(ink: &Ink, e: P) -> Vec<P> {
    const N: usize = 180;
    let angle = |k: f64| k * std::f64::consts::TAU / N as f64;
    let inked: Vec<bool> = (0..N)
        .map(|k| {
            let d = (angle(k as f64).cos(), angle(k as f64).sin());
            let hits = (0..14)
                .filter(|i| {
                    let q = add(e, mul(d, 4.0 + 2.0 * *i as f64));
                    ink.has_dark_within(q.0, q.1, 1.5)
                })
                .count();
            hits * 10 >= 14 * 9
        })
        .collect();
    let Some(gap) = inked.iter().position(|x| !x) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    for step in 1..=N {
        let k = (gap + step) % N;
        if inked[k] {
            run.push(gap + step);
        } else if !run.is_empty() {
            let mid = 0.5 * (run[0] + run[run.len() - 1]) as f64;
            out.push((angle(mid).cos(), angle(mid).sin()));
            run.clear();
        }
    }
    out
}

/// Vertices of a leader from its strokes (first at the letter) to the point `at` on the circle: the
/// free end of the first stroke, the joints (the intersection of consecutive strokes meeting at 3° or
/// more, else skipped as one straight run), and the point on the circle.
fn leader_vertices(pieces: &[(P, P)], at: P) -> Vec<P> {
    let n = pieces.len();
    let mut oriented: Vec<(P, P)> = Vec::with_capacity(n);
    for k in 0..n {
        let (a, b) = pieces[k];
        let next = if k + 1 < n {
            let (c, d) = pieces[k + 1];
            if dist(a, c).min(dist(a, d)) < dist(b, c).min(dist(b, d)) { a } else { b }
        } else if dist(a, at) < dist(b, at) {
            a
        } else {
            b
        };
        oriented.push(if next == a { (b, a) } else { (a, b) });
    }
    let mut v: Vec<P> = vec![oriented[0].0];
    for k in 0..n.saturating_sub(1) {
        let ((a, b), (c, d)) = (oriented[k], oriented[k + 1]);
        let (Some(u), Some(w)) = (unit(sub(b, a)), unit(sub(d, c))) else { continue };
        let cross = u.0 * w.1 - u.1 * w.0;
        if cross.abs() < 3f64.to_radians().sin() {
            continue;
        }
        let t = ((c.0 - a.0) * w.1 - (c.1 - a.1) * w.0) / cross;
        let x = add(a, mul(u, t));
        v.push(if dist(x, b) <= 15.0 && dist(x, c) <= 15.0 { x } else { mul(add(b, c), 0.5) });
    }
    v.push(at);
    v.dedup_by(|a, b| dist(*a, *b) < 1.0);
    v
}

/// The detail circle of a letter. The letter stands on the shelf of a leader running to a small circle
/// around the detailed feature, or beside the circle. In a window of fourteen letter heights the ink
/// (the letter left out) is vectorised and its circles found (`circles_on_ink`). The leader starts
/// with a stroke passing within 0.8 heights of the letter with an end within two heights of it (the
/// shelf, or the leader itself), possibly followed by a stroke joining it end to end (6 px); a stroke
/// reaches a circle with an end on its circumference or when its ink, followed on, meets it. The circle
/// reached by the shorter leader (then nearest to the letter) is the detail circle, the leader with the
/// shelf in front when it starts beside one. A leader the vectorisation broke up is found by following
/// the straight strokes leaving an end of a first stroke on the ink (`stroke_directions`,
/// `follow_to_circle`). Without a leader, a circle passing within two heights of the letter.
pub(crate) fn find_detail_circle(ink: &Ink, letter: [f64; 4], h: f64) -> Option<DetailCallout> {
    let half = (14.0 * h).clamp(250.0, 700.0);
    let ws = window_strokes(ink, letter, half)?;
    let circles = circles_on_ink(ink, &ws.arcs, h);
    if circles.is_empty() {
        return None;
    }
    let lines = &ws.lines;
    let first: Vec<usize> = (0..lines.len())
        .filter(|&i| {
            let (p, q) = lines[i];
            segment_box_distance(letter, p, q) <= 0.8 * h && box_distance(letter, p).min(box_distance(letter, q)) <= 2.0 * h
        })
        .collect();
    let mut paths: Vec<Vec<usize>> = first.iter().map(|&i| vec![i]).collect();
    for &i in &first {
        let (a, b) = lines[i];
        for (j, &(c, d)) in lines.iter().enumerate() {
            if !first.contains(&j) && [a, b].iter().any(|e| dist(*e, c) <= 6.0 || dist(*e, d) <= 6.0) {
                paths.push(vec![i, j]);
            }
        }
    }
    for len in 1..=2 {
        let mut best: Option<(f64, &Vec<usize>, usize, P)> = None;
        for path in paths.iter().filter(|p| p.len() == len) {
            let last = lines[*path.last()?];
            // The second stroke is followed from its end away from the first.
            let stroke = if len == 1 {
                last
            } else {
                let (a, b) = lines[path[0]];
                let near_first = |e: P| dist(e, a).min(dist(e, b));
                if near_first(last.0) <= near_first(last.1) { last } else { (last.1, last.0) }
            };
            for (k, &(c, r)) in circles.iter().enumerate() {
                let Some(at) = reaches(ink, stroke, len == 1, c, r, half) else { continue };
                let off = (dist(centre(letter), c) - r).abs();
                if best.as_ref().is_none_or(|b| off < b.0) {
                    best = Some((off, path, k, at));
                }
            }
        }
        if let Some((_, path, k, at)) = best {
            let mut pieces: Vec<(P, P)> = path.iter().map(|&i| lines[i]).collect();
            // The shelf the letter stands on, when the leader found starts beside it: a horizontal
            // stroke near the letter joining the leader's start end to end.
            let horizontal = |(p, q): (P, P)| (q.1 - p.1).abs() <= 0.09 * dist(p, q);
            if !horizontal(pieces[0]) {
                let start = leader_vertices(&pieces, at)[0];
                let shelf = (0..lines.len())
                    .filter(|j| !path.contains(j))
                    .map(|j| lines[j])
                    .filter(|&(p, q)| horizontal((p, q)) && dist(p, q) >= 10.0 && segment_box_distance(letter, p, q) <= 0.8 * h)
                    .map(|(p, q)| (dist(start, p).min(dist(start, q)), (p, q)))
                    .filter(|(d, _)| *d <= 6.0)
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, s)) = shelf {
                    pieces.insert(0, s);
                }
            }
            let (c, r) = circles[k];
            return Some(DetailCallout { c, r, leader: leader_vertices(&pieces, at) });
        }
    }
    // A leader the vectorisation did not give as a stroke: the straight strokes leaving an end of a
    // first stroke (the shelf), followed on the ink to a circle.
    let lc = centre(letter);
    let mut best: Option<(f64, usize, Vec<P>)> = None;
    for &i in &first {
        let (p, q) = lines[i];
        for (e, o) in [(p, q), (q, p)] {
            let back = unit(sub(o, e));
            for d in stroke_directions(ink, e) {
                if back.is_some_and(|b| dot(b, d) > 0.97) {
                    continue;
                }
                for (k, &(c, r)) in circles.iter().enumerate() {
                    let Some(at) = follow_to_circle(ink, e, d, c, r, half) else { continue };
                    let off = (dist(lc, c) - r).abs();
                    if best.as_ref().is_none_or(|b| off < b.0) {
                        best = Some((off, k, leader_vertices(&[(o, e), (e, at)], at)));
                    }
                }
            }
        }
    }
    if let Some((_, k, leader)) = best {
        let (c, r) = circles[k];
        return Some(DetailCallout { c, r, leader });
    }
    circles
        .iter()
        .map(|&(c, r)| (c, r, (dist(lc, c) - r).abs()))
        .filter(|(_, r, off)| *off <= 2.0 * h + 0.2 * r)
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(c, r, _)| DetailCallout { c, r, leader: Vec::new() })
}

/// The bar of a detail-view title between its letter (above) and its scale (below): a horizontal
/// line fitted across both boxes midway between them.
pub(crate) fn find_title_bar(ink: &Ink, letter: [f64; 4], scale: [f64; 4]) -> Option<(P, P)> {
    let (top, bottom) = (letter[3], scale[1]);
    if bottom - top < 3.0 {
        return None;
    }
    let y = 0.5 * (top + bottom);
    let corridor = (0.5 * (bottom - top) - 1.0).max(2.0);
    let margin = 0.3 * (scale[3] - scale[1]).max(letter[3] - letter[1]);
    let (xa, xb) = (letter[0].min(scale[0]) - margin, letter[2].max(scale[2]) + margin);
    let fit = fit_segment(ink, (xa, y), (xb, y), corridor, 4.0);
    let horizontal = (fit.q.1 - fit.p.1).abs() <= 0.05 * dist(fit.p, fit.q);
    (fit.support_after >= 0.8 && horizontal && dist(fit.p, fit.q) >= 0.8 * (letter[2] - letter[0])).then_some((fit.p, fit.q))
}

/// Split the box of a direction-view title ("A向") into the letter part and the 向 part: at the
/// widest empty column run near the split the character widths suggest (`letter_fraction` of the
/// box), else at that estimate; both parts tightened to their glyphs.
pub(crate) fn split_title(ink: &Ink, b: [f64; 4], letter_fraction: f64) -> ([f64; 4], [f64; 4]) {
    let (x0, y0, x1, y1) = (b[0], b[1], b[2], b[3]);
    let w = x1 - x0;
    let guess = x0 + w * letter_fraction.clamp(0.15, 0.85);
    let mut runs: Vec<(f64, f64)> = Vec::new();
    let mut start: Option<i64> = None;
    let (xa, xb) = ((guess - 0.3 * w).max(x0 + 1.0).ceil() as i64, (guess + 0.3 * w).min(x1 - 1.0).floor() as i64);
    for x in xa..=xb {
        let empty = (y0.ceil() as i64..=y1.floor() as i64).all(|y| !ink.is_dark(x, y));
        match (empty, start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                runs.push((s as f64, (x - 1) as f64));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s as f64, xb as f64));
    }
    let split = runs
        .iter()
        .min_by(|a, b| {
            let da = ((a.0 + a.1) * 0.5 - guess).abs() - (a.1 - a.0);
            let db = ((b.0 + b.1) * 0.5 - guess).abs() - (b.1 - b.0);
            da.total_cmp(&db)
        })
        .map_or(guess, |r| (r.0 + r.1) * 0.5);
    let h = y1 - y0;
    let left = text_box_fit_glyphs(ink, [x0, y0, split - 1.0, y1], 0.0, h, &[]).0;
    let right = text_box_fit_glyphs(ink, [split + 1.0, y0, x1, y1], 0.0, h, &[]).0;
    (left, right)
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

    #[test]
    fn a_projection_arrow_and_a_detail_circle_are_found_beside_their_letters() {
        // Letter box at (300..320, 100..140); an arrow below it from (310, 160) down to the tip at (310, 300),
        // its head 36 px long and 16 px wide; a circle of radius 120 around (600, 400) with a letter beside it.
        let ink = ink_with(900, 700, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let letter = (302..=318).contains(&x) && (102..=138).contains(&y) && (x % 5 == 0 || y % 9 == 0);
            let shaft = (309..=311).contains(&x) && (160..=264).contains(&y);
            let head = (264.0..=300.0).contains(&yf) && (xf - 310.0).abs() <= (300.0 - yf) * 8.0 / 36.0 + 0.5;
            let r = ((xf - 600.0).powi(2) + (yf - 400.0).powi(2)).sqrt();
            let circle = (r - 120.0).abs() <= 1.2;
            let letter2 = (740..=756).contains(&x) && (250..=286).contains(&y) && (x % 5 == 0 || y % 9 == 0);
            letter || shaft || head || circle || letter2
        });
        let arrow = find_view_arrow(&ink, [300.0, 100.0, 320.0, 140.0], 40.0, 3.0).unwrap();
        assert!((arrow.tip.1 - 300.0).abs() <= 1.5 && (arrow.tip.0 - 310.0).abs() <= 1.0, "{arrow:?}");
        assert!((arrow.base.1 - 264.5).abs() <= 2.0, "{arrow:?}");
        assert!((arrow.tail.1 - 160.0).abs() <= 1.5, "{arrow:?}");
        let dc = find_detail_circle(&ink, [738.0, 248.0, 758.0, 288.0], 40.0).unwrap();
        assert!(dist(dc.c, (600.0, 400.0)) <= 2.0 && (dc.r - 120.0).abs() <= 2.0, "{dc:?}");
        assert!(arrow.cutting.is_none(), "{arrow:?}");
        // A small detail circle reached by a leader from a letter standing on its shelf.
        let ink2 = ink_with(700, 600, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let letter = (500..=512).contains(&x) && (100..=136).contains(&y) && (x % 4 == 0 || y % 9 == 0);
            let shelf = (141..=143).contains(&y) && (490..=530).contains(&x);
            // From the shelf's left end (490, 142) down-left to the circle around (300, 400), radius 40.
            let t = (490.0 - xf) / (490.0 - 328.0);
            let leader = (0.0..=1.0).contains(&t) && (yf - (142.0 + t * (372.0 - 142.0))).abs() <= 1.5;
            let r = ((xf - 300.0).powi(2) + (yf - 400.0).powi(2)).sqrt();
            let circle = (r - 40.0).abs() <= 1.2;
            letter || shelf || leader || circle
        });
        let dc2 = find_detail_circle(&ink2, [498.0, 98.0, 514.0, 138.0], 40.0).unwrap();
        assert!(dist(dc2.c, (300.0, 400.0)) <= 2.0 && (dc2.r - 40.0).abs() <= 2.0, "{dc2:?}");
        // The leader: the shelf's right end, the joint at its left end, the point on the circle.
        assert_eq!(dc2.leader.len(), 3, "{dc2:?}");
        assert!(dc2.leader[0].0 >= 525.0 && dist(dc2.leader[1], (490.0, 142.0)) <= 4.0, "{dc2:?}");
        assert!((dist(dc2.leader[2], (300.0, 400.0)) - 40.0).abs() <= 2.5, "{dc2:?}");
        // A section arrow: the cutting-plane stroke (vertical, 80 px) at the tail of a horizontal arrow.
        let ink3 = ink_with(600, 400, |x, y| {
            let (xf, yf) = (x as f64, y as f64);
            let letter = (200..=216).contains(&x) && (150..=190).contains(&y) && (x % 4 == 0 || y % 9 == 0);
            let stroke = (238..=240).contains(&x) && (140..=220).contains(&y);
            let shaft = (139..=141).contains(&y) && (238..=300).contains(&x);
            let head = (300.0..=336.0).contains(&xf) && (yf - 140.0).abs() <= (336.0 - xf) * 7.0 / 36.0 + 0.5;
            letter || stroke || shaft || head
        });
        let section = find_view_arrow(&ink3, [198.0, 148.0, 218.0, 192.0], 44.0, 3.0).unwrap();
        assert!((section.tip.0 - 336.0).abs() <= 2.0 && (section.tail.0 - 239.0).abs() <= 4.0, "{section:?}");
        let (m, n) = section.cutting.expect("cutting stroke");
        assert!((m.0 - 239.0).abs() <= 2.0 && (n.0 - 239.0).abs() <= 2.0 && (m.1 - n.1).abs() >= 70.0, "{section:?}");
        // No arrow beside the circle's letter, no circle beside the arrow's letter.
        assert!(find_view_arrow(&ink, [738.0, 248.0, 758.0, 288.0], 40.0, 3.0).is_none());
        assert!(find_detail_circle(&ink, [300.0, 100.0, 320.0, 140.0], 40.0).is_none());
    }

    #[test]
    fn a_title_splits_at_its_gap_and_a_detail_bar_is_found() {
        // "A向": a letter blob 100..118 and a wider glyph 128..160, rows 50..86; the bar of a detail title
        // under a letter (200..215, 40..80) above a scale (190..230, 100..130).
        let ink = ink_with(300, 160, |x, y| {
            let title = ((100..=118).contains(&x) || (128..=160).contains(&x)) && (50..=86).contains(&y) && (x % 3 == 0 || y % 4 == 0);
            let letter = (200..=215).contains(&x) && (40..=80).contains(&y) && x % 4 == 0;
            let bar = (89..=91).contains(&y) && (185..=235).contains(&x);
            let scale = (190..=230).contains(&x) && (100..=130).contains(&y) && (x % 4 == 0 || y % 6 == 0);
            title || letter || bar || scale
        });
        let (l, r) = split_title(&ink, [100.0, 50.0, 160.0, 86.0], 0.44);
        assert!(l[2] <= 119.0 && r[0] >= 127.0, "{l:?} {r:?}");
        let (p, q) = find_title_bar(&ink, [200.0, 40.0, 215.0, 80.0], [190.0, 100.0, 230.0, 130.0]).unwrap();
        assert!((p.1 - 90.0).abs() <= 1.0 && (q.1 - 90.0).abs() <= 1.0 && p.0 <= 186.0 && q.0 >= 234.0, "{p:?} {q:?}");
    }
}
