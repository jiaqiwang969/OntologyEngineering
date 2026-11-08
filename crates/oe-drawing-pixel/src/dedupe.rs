//! De-duplication of proposals across tile overlaps (design §4): proposals of the same category and
//! the same normalised text whose centres lie within 70 px are one object; the one farthest from
//! its tile's edges is kept, and each of its members takes the better supported refined geometry of
//! its duplicates. Members a duplicate saw that the kept proposal lacks (an extension line beyond
//! the tile edge) are added. Centre lines without text merge when collinear and continuing.
//!
//! A text read whole on one tile and in part (with `?`) on its neighbour is one dimension or note
//! when the readings fit with `?` standing for any run of characters and lie together: their text
//! boxes overlap (a third of the smaller box for notes; a reading with nothing readable needs the same
//! dimension line), their centres lie within 70 px with two readable characters, or their dimension
//! lines (as refined, else as proposed) are one line. The whole reading is kept with the best supported
//! geometry of both, its text box too when only the partial reading's box is supported.
//!
//! Duplicates are also recognised by geometry: two dimensions whose refined dimension lines are
//! collinear (3 px) and overlap are one dimension, and dimension, leader-note and tolerance-frame
//! proposals whose refined leader segments coincide are one object (the observer lists a thread note
//! both as a leader dimension and as a note, with partial texts). The kept proposal is the one whose
//! text has no unread `?`, then the longer text, then the dimension over a note, then the one
//! farthest from its tile edge; text members of a duplicate with another text are never taken.

use crate::geom::{dist, dot, perp, sub, undirected_angle, unit};
use crate::observe::{Category, P, Shape};
use crate::pattern::Dsu;
use crate::refine::{RefinedMember, RefinedProposal};
use crate::textform::normalise_key;

/// Thresholds of the de-duplication, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DedupeParams {
    /// Centre distance of duplicates with text.
    pub centre_px: f64,
    /// Endpoint distance of matching members.
    pub member_px: f64,
    /// Collinearity of centre-line pieces: angle and offset.
    pub line_angle_rad: f64,
    pub line_offset_px: f64,
    /// Gap along the line between continuing centre-line pieces.
    pub line_gap_px: f64,
    /// Coincident strokes (dimension lines, leaders) of two proposals: offset, and the least overlap
    /// in pixels (two readings of one line cut by a tile edge share the tile overlap, some 200 px;
    /// neighbours of a chain overlap by an arrowhead at most).
    pub coincident_offset_px: f64,
    pub coincident_overlap_px: f64,
}

impl Default for DedupeParams {
    fn default() -> Self {
        DedupeParams {
            centre_px: 70.0,
            member_px: 25.0,
            line_angle_rad: 0.052_36,
            line_offset_px: 12.0,
            line_gap_px: 40.0,
            coincident_offset_px: 3.0,
            coincident_overlap_px: 60.0,
        }
    }
}

/// Two segments on one line (within the angle and offset tolerances) sharing at least
/// `min_overlap` pixels (and a tenth of the shorter one); with `min_overlap` 0 they may merely touch
/// (1 px).
fn coincident(a: (P, P), b: (P, P), angle_tol: f64, offset_px: f64, min_overlap: f64) -> bool {
    let (Some(da), Some(db)) = (unit(sub(a.1, a.0)), unit(sub(b.1, b.0))) else {
        return false;
    };
    if undirected_angle(da, db) > angle_tol {
        return false;
    }
    let n = perp(da);
    if dot(sub(b.0, a.0), n).abs() > offset_px || dot(sub(b.1, a.0), n).abs() > offset_px {
        return false;
    }
    let (la, lb) = (dist(a.0, a.1), dist(b.0, b.1));
    let (t0, t1) = (dot(sub(b.0, a.0), da), dot(sub(b.1, a.0), da));
    let overlap = t0.max(t1).min(la) - t0.min(t1).max(0.0);
    if min_overlap <= 0.0 {
        overlap >= -1.0
    } else {
        overlap >= min_overlap.max(0.1 * la.min(lb))
    }
}

/// Supported refined segments of a role.
fn segments_of<'a>(p: &'a RefinedProposal, role: &'a str) -> impl Iterator<Item = (P, P)> + 'a {
    p.members.iter().filter(move |m| m.supported && m.role == role).filter_map(|m| match m.refined {
        Shape::Segment(a, b) => Some((a, b)),
        _ => None,
    })
}

fn any_coincident(a: &RefinedProposal, b: &RefinedProposal, role_a: &str, role_b: &str, params: &DedupeParams) -> bool {
    segments_of(a, role_a).any(|sa| {
        segments_of(b, role_b).any(|sb| coincident(sa, sb, params.line_angle_rad, params.coincident_offset_px, params.coincident_overlap_px))
    })
}

/// Preference among duplicates: a text without an unread `?`, the longer text, a dimension over a
/// tolerance frame over a note, then the proposal farthest from its tile edge.
fn rank(p: &RefinedProposal) -> (u8, usize, u8, f64) {
    let category = match p.proposal.category {
        Category::Dimension => 3,
        Category::GdtFrame | Category::Datum => 2,
        Category::Note => 1,
        _ => 0,
    };
    (
        u8::from(!p.proposal.text.contains('?')),
        p.proposal.text.chars().count(),
        category,
        p.proposal.edge_distance,
    )
}

/// What one merge did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    pub kept: String,
    pub dropped: Vec<String>,
    pub swapped: Vec<(String, String)>,
    pub added: Vec<(String, String)>,
}

fn origin(p: &RefinedProposal) -> String {
    format!("{}#{}:{}", p.proposal.provenance.tile, p.proposal.provenance.level, p.proposal.provenance.item_index)
}

fn best_segment(m: &RefinedMember) -> Option<(P, P)> {
    match (&m.refined, m.supported) {
        (Shape::Segment(p, q), true) => Some((*p, *q)),
        _ => match &m.proposed {
            Shape::Segment(p, q) => Some((*p, *q)),
            _ => None,
        },
    }
}

/// Two segments collinear within the tolerances and overlapping or within `gap` of each other.
fn continuing(a: (P, P), b: (P, P), params: &DedupeParams) -> bool {
    let (Some(da), Some(db)) = (unit(sub(a.1, a.0)), unit(sub(b.1, b.0))) else {
        return false;
    };
    if undirected_angle(da, db) > params.line_angle_rad {
        return false;
    }
    let n = perp(da);
    if dot(sub(b.0, a.0), n).abs() > params.line_offset_px || dot(sub(b.1, a.0), n).abs() > params.line_offset_px {
        return false;
    }
    let la = dist(a.0, a.1);
    let (t0, t1) = (dot(sub(b.0, a.0), da), dot(sub(b.1, a.0), da));
    let gap = (t0.min(t1) - la).max(-t0.max(t1));
    gap <= params.line_gap_px
}

fn bbox_of(points: &[P]) -> [f64; 4] {
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for p in points {
        b[0] = b[0].min(p.0);
        b[1] = b[1].min(p.1);
        b[2] = b[2].max(p.0);
        b[3] = b[3].max(p.1);
    }
    b
}

/// Intersection over the smaller box's area.
fn containment(a: [f64; 4], b: [f64; 4]) -> f64 {
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let small = ((a[2] - a[0]) * (a[3] - a[1])).min((b[2] - b[0]) * (b[3] - b[1]));
    if small > 0.0 { w * h / small } else { 0.0 }
}

/// The first cell of a tolerance frame's text (its characteristic symbol).
fn first_cell(text: &str) -> String {
    text.split('|').next().unwrap_or("").trim().to_owned()
}

fn overlap(a: [f64; 4], b: [f64; 4]) -> f64 {
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = w * h;
    let union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter;
    if union > 0.0 { inter / union } else { 0.0 }
}

/// Two readings of one text are compatible when they agree, when one of them has an unread `?`, or
/// when one is part of the other (`攻深22` within `M8 攻深22`). Two readable, different values on one
/// line are a chain of dimensions, never one dimension.
fn compatible_texts(a: &str, b: &str) -> bool {
    let (ka, kb) = (normalise_key(a), normalise_key(b));
    ka == kb || ka.contains('?') || kb.contains('?') || ka.is_empty() || kb.is_empty() || ka.contains(&kb) || kb.contains(&ka)
}

/// Whether two readings can be one text: normalised, `?` standing for any run of characters (an unread
/// part), some text matches both (`?01` and `Ø20+0.02/-0.01`, `Ø?` and `Ø30`).
fn wildcard_compatible(a: &str, b: &str) -> bool {
    let (ka, kb): (Vec<char>, Vec<char>) = (normalise_key(a).chars().collect(), normalise_key(b).chars().collect());
    let (n, m) = (ka.len(), kb.len());
    // fits[i][j]: the rests ka[i..] and kb[j..] match a common text.
    let mut fits = vec![vec![false; m + 1]; n + 1];
    fits[n][m] = true;
    for i in (0..=n).rev() {
        for j in (0..=m).rev() {
            if i == n && j == m {
                continue;
            }
            fits[i][j] = if i < n && ka[i] == '?' {
                fits[i + 1][j] || (j < m && fits[i][j + 1])
            } else if j < m && kb[j] == '?' {
                fits[i][j + 1] || (i < n && fits[i + 1][j])
            } else {
                i < n && j < m && ka[i] == kb[j] && fits[i + 1][j + 1]
            };
        }
    }
    fits[0][0]
}

/// Text boxes of a proposal: refined when supported, else as proposed.
fn text_boxes(p: &RefinedProposal) -> Vec<[f64; 4]> {
    p.members
        .iter()
        .filter_map(|m| match (&m.refined, &m.proposed, m.supported) {
            (Shape::TextBox { bbox, .. }, _, true) => Some(*bbox),
            (_, Shape::TextBox { bbox, .. }, false) => Some(*bbox),
            _ => None,
        })
        .collect()
}

/// A dimension or note read whole on one tile and in part on another (see the module notes).
fn partial_reading_pair(a: &RefinedProposal, b: &RefinedProposal, params: &DedupeParams) -> bool {
    let (ta, tb) = (&a.proposal.text, &b.proposal.text);
    if !(ta.contains('?') || tb.contains('?')) || !wildcard_compatible(ta, tb) {
        return false;
    }
    let readable = |t: &str| normalise_key(t).chars().filter(|c| *c != '?').count();
    let least = readable(ta).min(readable(tb));
    let dimension = a.proposal.category == Category::Dimension;
    let lines = |p: &RefinedProposal| -> Vec<(P, P)> { p.members.iter().filter(|m| m.role == "dimension_line").filter_map(best_segment).collect() };
    let same_line = dimension
        && lines(a).iter().any(|sa| {
            lines(b)
                .iter()
                .any(|sb| coincident(*sa, *sb, params.line_angle_rad, params.coincident_offset_px.max(4.0), params.coincident_overlap_px))
        });
    if same_line {
        return true;
    }
    if least == 0 {
        return false;
    }
    let need = if dimension { 1e-9 } else { 0.3 };
    let boxes = text_boxes(a).iter().any(|x| text_boxes(b).iter().any(|y| containment(*x, *y) >= need));
    let close = least >= 2 && dist(a.proposal.centre, b.proposal.centre) < params.centre_px;
    boxes || close
}

fn same_object(a: &RefinedProposal, b: &RefinedProposal, params: &DedupeParams) -> bool {
    let (ca, cb) = (a.proposal.category, b.proposal.category);
    // A label read on a datum symbol (its letter in the frame) is the datum, never a callout.
    if let ((Category::Label, Category::Datum) | (Category::Datum, Category::Label), Some(ba), Some(bb)) =
        ((ca, cb), a.proposal.bbox, b.proposal.bbox)
    {
        let (label, datum) = if ca == Category::Label { (ba, bb) } else { (bb, ba) };
        let label_area = ((label[2] - label[0]) * (label[3] - label[1])).max(1e-9);
        let w = (label[2].min(datum[2]) - label[0].max(datum[0])).max(0.0);
        let h = (label[3].min(datum[3]) - label[1].max(datum[1])).max(0.0);
        return normalise_key(&a.proposal.text) == normalise_key(&b.proposal.text) && w * h / label_area >= 0.5;
    }
    // Geometry: one dimension line, or one leader, drawn once on the page.
    let compatible = compatible_texts(&a.proposal.text, &b.proposal.text);
    if ca == Category::Dimension && cb == Category::Dimension && compatible && any_coincident(a, b, "dimension_line", "dimension_line", params) {
        return true;
    }
    let leader_pair = matches!((ca, cb), (Category::Dimension | Category::Note, Category::Dimension | Category::Note))
        || (ca == cb && ca == Category::GdtFrame);
    if leader_pair && compatible && any_coincident(a, b, "leader_line", "leader_line", params) {
        return true;
    }
    // A dimension the observer could not read whose "dimension line" is the leader of a note or a
    // tolerance frame is that leader read twice.
    let unread = |p: &RefinedProposal| p.proposal.category == Category::Dimension && normalise_key(&p.proposal.text).contains('?');
    let owns_leader = |p: &RefinedProposal| matches!(p.proposal.category, Category::Dimension | Category::Note | Category::GdtFrame);
    if (unread(a) && owns_leader(b) && any_coincident(a, b, "dimension_line", "leader_line", params))
        || (unread(b) && owns_leader(a) && any_coincident(b, a, "dimension_line", "leader_line", params))
    {
        return true;
    }
    if ca != cb {
        return false;
    }
    if matches!(ca, Category::Dimension | Category::Note) && partial_reading_pair(a, b, params) {
        return true;
    }
    let close = dist(a.proposal.centre, b.proposal.centre) < params.centre_px;
    match ca {
        Category::CentreLine => a.members.iter().filter_map(best_segment).any(|sa| {
            b.members
                .iter()
                .filter_map(best_segment)
                .any(|sb| continuing(sa, sb, params))
        }),
        Category::Hatch => {
            // Pieces of one section region read on two tiles: touching or overlapping polygons with
            // the same measured hatch angle (within 3°; an unmeasured piece agrees with any).
            let pa: Vec<P> = a.members.iter().flat_map(|m| m.refined.points()).collect();
            let pb: Vec<P> = b.members.iter().flat_map(|m| m.refined.points()).collect();
            let (ba, bb) = (bbox_of(&pa), bbox_of(&pb));
            let touching = ba[0] <= bb[2] + 6.0 && bb[0] <= ba[2] + 6.0 && ba[1] <= bb[3] + 6.0 && bb[1] <= ba[3] + 6.0;
            let angle = |p: &RefinedProposal| {
                p.members.iter().find_map(|m| match m.refined {
                    Shape::Hatch { angle_deg, .. } if m.adjustments.iter().any(|x| x.starts_with("hatch_measured")) => Some(angle_deg),
                    _ => None,
                })
            };
            let agree = match (angle(a), angle(b)) {
                (Some(x), Some(y)) => {
                    let d = (x - y).rem_euclid(180.0);
                    d.min(180.0 - d) <= 3.0
                }
                _ => true,
            };
            touching && agree
        }
        Category::GdtFrame => {
            // One frame read on two tiles: the boxes overlap mostly (one may be cut by a tile edge) and
            // the characteristic symbols agree (or one is unread).
            let (sa, sb) = (first_cell(&a.proposal.text), first_cell(&b.proposal.text));
            let symbols = sa == sb || sa.contains('?') || sb.contains('?') || sa.is_empty() || sb.is_empty();
            match (a.proposal.bbox, b.proposal.bbox) {
                (Some(ba), Some(bb)) => symbols && containment(ba, bb) >= 0.6,
                _ => close && normalise_key(&a.proposal.text) == normalise_key(&b.proposal.text),
            }
        }
        Category::SurfaceTexture => {
            // One symbol read on two tiles: the same value (a partly unread one fitting with `?` as a
            // wildcard) and the symbol boxes overlapping (IoU above 0.3), as observed or as refined
            // from the symbol's strokes.
            let (ta, tb) = (&a.proposal.text, &b.proposal.text);
            let values = normalise_key(ta) == normalise_key(tb) || ((ta.contains('?') || tb.contains('?')) && wildcard_compatible(ta, tb));
            let symbol_box = |p: &RefinedProposal| {
                let pts: Vec<P> = p
                    .members
                    .iter()
                    .filter(|m| m.supported && m.role.starts_with("roughness_symbol"))
                    .flat_map(|m| m.refined.points())
                    .collect();
                (!pts.is_empty()).then(|| bbox_of(&pts))
            };
            let observed = matches!((a.proposal.bbox, b.proposal.bbox), (Some(ba), Some(bb)) if overlap(ba, bb) > 0.3);
            let refined = matches!((symbol_box(a), symbol_box(b)), (Some(ba), Some(bb)) if overlap(ba, bb) > 0.3);
            values && (observed || refined || close)
        }
        Category::BreakLine => {
            let pa: Vec<P> = a.members.iter().flat_map(|m| m.proposed.points()).collect();
            let pb: Vec<P> = b.members.iter().flat_map(|m| m.proposed.points()).collect();
            close || overlap(bbox_of(&pa), bbox_of(&pb)) > 0.3
        }
        _ => close && normalise_key(&a.proposal.text) == normalise_key(&b.proposal.text),
    }
}

/// Whether two members of the same role describe the same stroke, tip, box or curve.
fn same_member(a: &RefinedMember, b: &RefinedMember, params: &DedupeParams) -> bool {
    if a.role != b.role {
        return false;
    }
    let near = |p: P, q: P| dist(p, q) <= params.member_px;
    match (&a.refined, &b.refined) {
        (Shape::Segment(p, q), Shape::Segment(r, s)) => {
            (near(*p, *r) && near(*q, *s))
                || (near(*p, *s) && near(*q, *r))
                || {
                    let (Some(d1), Some(d2)) = (unit(sub(*q, *p)), unit(sub(*s, *r))) else {
                        return false;
                    };
                    undirected_angle(d1, d2) < 0.1 && near(a.refined.reference(), b.refined.reference())
                }
                // One stroke seen in two extents (a tile edge cut it): collinear and overlapping.
                || coincident((*p, *q), (*r, *s), params.line_angle_rad, params.coincident_offset_px, 0.0)
        }
        (Shape::Tip { approx: p, .. }, Shape::Tip { approx: q, .. })
        | (Shape::Point(p), Shape::Point(q)) => dist(*p, *q) <= 15.0,
        (Shape::TextBox { .. }, Shape::TextBox { .. }) => {
            dist(a.refined.reference(), b.refined.reference()) <= 40.0
        }
        (Shape::Polyline(v), Shape::Polyline(w)) => {
            let (Some(v0), Some(v1), Some(w0), Some(w1)) = (v.first(), v.last(), w.first(), w.last()) else {
                return false;
            };
            (near(*v0, *w0) && near(*v1, *w1)) || (near(*v0, *w1) && near(*v1, *w0))
        }
        (Shape::Hatch { polygon: pa, .. }, Shape::Hatch { polygon: pb, .. }) => overlap(bbox_of(pa), bbox_of(pb)) > 0.6,
        (Shape::Circle { c: ca, r: ra }, Shape::Circle { c: cb, r: rb }) => dist(*ca, *cb) <= 6.0 && (ra - rb).abs() <= 6.0,
        _ => false,
    }
}

/// Two supported collinear segments of one member seen in different extents (a tile edge cut the
/// stroke) become one segment spanning both, on the carrier of the better supported one, when the
/// union adds more than 5 px; the support weighs the carrier over its length and the other over the
/// added length. `None` when the members are not two overlapping or touching collinear segments, or
/// when the union adds nothing worth it (the better one is then simply kept).
fn union_segments(a: &RefinedMember, b: &RefinedMember, params: &DedupeParams) -> Option<RefinedMember> {
    let (Shape::Segment(pa, qa), Shape::Segment(pb, qb)) = (&a.refined, &b.refined) else {
        return None;
    };
    if !(a.supported && b.supported && coincident((*pa, *qa), (*pb, *qb), params.line_angle_rad, params.coincident_offset_px, 0.0)) {
        return None;
    }
    let (carrier, other) = if b.support_after > a.support_after + 1e-9 { (b, a) } else { (a, b) };
    let Shape::Segment(cp, cq) = carrier.refined else { return None };
    let Shape::Segment(op, oq) = other.refined else { return None };
    let d = unit(sub(cq, cp))?;
    let lc = dist(cp, cq);
    let (t0, t1) = (dot(sub(op, cp), d), dot(sub(oq, cp), d));
    let (lo, hi) = (t0.min(t1).min(0.0), t0.max(t1).max(lc));
    let added = (hi - lo) - lc;
    if added <= 5.0 {
        return None;
    }
    let (p, q) = (
        (cp.0 + d.0 * lo, cp.1 + d.1 * lo),
        (cp.0 + d.0 * hi, cp.1 + d.1 * hi),
    );
    let mut u = carrier.clone();
    u.refined = Shape::Segment(p, q);
    u.support_after = ((carrier.support_after * lc + other.support_after * added) / (lc + added)).min(1.0);
    u.width_px = carrier.width_px.max(other.width_px);
    u.moved_px = carrier.moved_px.max(other.moved_px);
    Some(u)
}

/// Merge duplicates. Returns the kept proposals in their original order and the merge records.
pub fn dedupe(items: Vec<RefinedProposal>, params: &DedupeParams) -> (Vec<RefinedProposal>, Vec<Merge>) {
    let n = items.len();
    let mut dsu = Dsu::new(n);
    for i in 0..n {
        for j in i + 1..n {
            if same_object(&items[i], &items[j], params) {
                dsu.union(i, j);
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = std::collections::BTreeMap::new();
    for i in 0..n {
        groups.entry(dsu.find(i)).or_default().push(i);
    }
    let mut keep: Vec<Option<RefinedProposal>> = items.into_iter().map(Some).collect();
    let mut merges = Vec::new();
    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        // The representative: the readable, fuller text, then farthest from its tile's edges; ties
        // keep the first.
        let rep = *members
            .iter()
            .max_by(|&&a, &&b| {
                let ra = keep[a].as_ref().map(rank).unwrap_or_default();
                let rb = keep[b].as_ref().map(rank).unwrap_or_default();
                ra.0.cmp(&rb.0)
                    .then(ra.1.cmp(&rb.1))
                    .then(ra.2.cmp(&rb.2))
                    .then(ra.3.total_cmp(&rb.3))
                    .then(b.cmp(&a))
            })
            .unwrap_or(&members[0]);
        let mut merged = keep[rep].take().expect("representative present");
        let mut record = Merge {
            kept: origin(&merged),
            dropped: Vec::new(),
            swapped: Vec::new(),
            added: Vec::new(),
        };
        for &o in members {
            if o == rep {
                continue;
            }
            let other = keep[o].take().expect("duplicate present");
            let from = origin(&other);
            let same_text = normalise_key(&other.proposal.text) == normalise_key(&merged.proposal.text);
            record.dropped.push(from.clone());
            // The best verification of the cluster.
            if let Some(v) = &other.proposal.provenance.verification {
                let better = merged
                    .proposal
                    .provenance
                    .verification
                    .as_ref()
                    .is_none_or(|w| v.p_proposal > w.p_proposal);
                if better {
                    merged.proposal.provenance.verification = Some(v.clone());
                }
            }
            for m in other.members {
                // A duplicate with another reading never lends its text; its box replaces the kept
                // text's box only when that one is unsupported and its own is supported.
                if !same_text && matches!(m.refined, Shape::TextBox { .. }) {
                    let unsupported_text = merged
                        .members
                        .iter()
                        .position(|r| r.role == m.role && !r.supported && matches!(r.refined, Shape::TextBox { .. }));
                    if let (Some(k), true) = (unsupported_text, m.supported) {
                        let r = &mut merged.members[k];
                        r.refined = m.refined.clone();
                        r.support_after = m.support_after;
                        r.supported = true;
                        r.moved_px = m.moved_px;
                        r.merged_from = Some(from.clone());
                        r.adjustments.push(format!("text_box_from_partial_reading:{from}"));
                        record.swapped.push((format!("{}:box", m.role), from.clone()));
                    }
                    continue;
                }
                let found = merged
                    .members
                    .iter()
                    .position(|r| same_member(r, &m, params));
                match found {
                    Some(k) => {
                        let unioned = union_segments(&merged.members[k], &m, params);
                        if let Some(u) = unioned {
                            record.swapped.push((format!("{}:union", m.role), from.clone()));
                            let mut u = u;
                            u.merged_from = Some(from.clone());
                            merged.members[k] = u;
                        } else if m.supported && m.support_after > merged.members[k].support_after + 1e-9 {
                            record.swapped.push((m.role.clone(), from.clone()));
                            let mut m = m;
                            m.merged_from = Some(from.clone());
                            merged.members[k] = m;
                        }
                    }
                    None => {
                        // A duplicate of another category (an unread dimension that was a leader)
                        // lends only roles the kept object already has.
                        let foreign_role = other.proposal.category != merged.proposal.category
                            && !merged.members.iter().any(|r| r.role == m.role);
                        if m.supported && !foreign_role {
                            record.added.push((m.role.clone(), from.clone()));
                            let mut m = m;
                            m.merged_from = Some(from.clone());
                            merged.members.push(m);
                        }
                    }
                }
            }
            merged
                .proposal
                .notes
                .push(format!("duplicate of {from} merged"));
        }
        keep[rep] = Some(merged);
        merges.push(record);
    }
    (keep.into_iter().flatten().collect(), merges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::{Proposal, ProposedMember, Provenance};

    fn member(role: &str, shape: Shape, support: f64) -> RefinedMember {
        RefinedMember {
            role: role.into(),
            text: None,
            proposed: shape.clone(),
            refined: shape,
            support_before: support,
            support_after: support,
            moved_px: 0.0,
            width_px: 2.0,
            pattern: None,
            supported: support >= 0.8,
            merged_from: None,
            arrow_base: None,
            arrow_length_px: None,
            discarded_components: 0,
            adjustments: Vec::new(),
            span_px: None,
        }
    }

    #[test]
    fn frames_roughness_and_datum_labels_merge_by_their_boxes() {
        let text_box = |b: [f64; 4]| Shape::TextBox { bbox: b, vertical: false };
        let with_box = |mut p: RefinedProposal, b: [f64; 4]| {
            p.proposal.bbox = Some(b);
            p
        };
        // A frame cut by the tile edge and the whole frame: one frame, the fuller reading kept.
        let f1 = with_box(proposal(Category::GdtFrame, "↗ | ?", (2960.0, 2367.0), 20.0, "t_1400_1400.png", vec![member("frame_cell", Shape::Segment((2926.0, 2312.0), (2999.0, 2312.0)), 0.9)]), [2926.0, 2312.0, 2999.0, 2422.0]);
        let f2 = with_box(proposal(Category::GdtFrame, "↗ | 0.03 | C-D", (3134.0, 2367.0), 200.0, "t_2800_1400.png", vec![member("frame_cell", Shape::Segment((2926.0, 2312.0), (3342.0, 2312.0)), 0.95)]), [2926.0, 2312.0, 3342.0, 2422.0]);
        let f3 = with_box(proposal(Category::GdtFrame, "≡ | 0.03 | G", (3300.0, 3070.0), 200.0, "t_2800_2800.png", vec![member("frame_cell", Shape::Segment((3136.0, 3014.0), (3480.0, 3014.0)), 0.95)]), [3136.0, 3014.0, 3480.0, 3125.0]);
        // Two readings of one roughness symbol whose value boxes lie apart; another symbol with another value.
        let r1 = with_box(proposal(Category::SurfaceTexture, "3.2", (600.0, 600.0), 50.0, "t_0_0.png", vec![member("roughness_value", text_box([580.0, 590.0, 620.0, 610.0]), 1.0)]), [540.0, 550.0, 690.0, 720.0]);
        let r2 = with_box(proposal(Category::SurfaceTexture, "3.2", (690.0, 600.0), 150.0, "t_1400_0.png", vec![member("roughness_value", text_box([670.0, 590.0, 710.0, 610.0]), 1.0)]), [560.0, 548.0, 700.0, 725.0]);
        let r3 = with_box(proposal(Category::SurfaceTexture, "1.6", (690.0, 600.0), 150.0, "t_1400_0.png", vec![member("roughness_value", text_box([670.0, 590.0, 710.0, 610.0]), 1.0)]), [560.0, 548.0, 700.0, 725.0]);
        // A callout label read on datum D's frame.
        let datum = with_box(proposal(Category::Datum, "D", (4684.0, 1596.0), 100.0, "t_4200_1400.png", vec![member("datum_letter", text_box([4660.0, 1575.0, 4708.0, 1618.0]), 1.0)]), [4646.0, 1559.0, 4722.0, 1634.0]);
        let label = with_box(proposal(Category::Label, "D", (4684.0, 1579.0), 300.0, "t_4200_0.png", vec![member("view_letter", text_box([4646.0, 1558.0, 4722.0, 1600.0]), 1.0)]), [4646.0, 1558.0, 4722.0, 1600.0]);
        let (kept, _) = dedupe(vec![f1, f2, f3, r1, r2, r3, datum, label], &DedupeParams::default());
        let texts: Vec<(Category, String)> = kept.iter().map(|k| (k.proposal.category, k.proposal.text.clone())).collect();
        assert_eq!(texts.len(), 5, "{texts:?}");
        assert!(texts.contains(&(Category::GdtFrame, "↗ | 0.03 | C-D".to_owned())));
        assert!(texts.contains(&(Category::GdtFrame, "≡ | 0.03 | G".to_owned())));
        assert_eq!(texts.iter().filter(|t| t.0 == Category::SurfaceTexture).count(), 2);
        let d = kept.iter().find(|k| k.proposal.category == Category::Datum).expect("the datum stays");
        assert!(d.members.iter().all(|m| m.role != "view_letter"), "the label lends no callout letter");
        assert!(kept.iter().all(|k| k.proposal.category != Category::Label));
    }

    #[test]
    fn roughness_readings_merge_by_their_refined_symbols_and_a_partly_unread_value() {
        let legs = |x: f64, y: f64| {
            vec![
                member("roughness_symbol", Shape::Segment((x, y), (x + 40.0, y + 70.0)), 1.0),
                member("roughness_symbol", Shape::Segment((x + 40.0, y + 70.0), (x + 140.0, y - 100.0)), 1.0),
            ]
        };
        // One symbol read on two tiles with no observed boxes and centres 120 px apart; the same for a
        // symbol read once as '6.3' and once with its value unread; a third symbol elsewhere.
        let a = proposal(Category::SurfaceTexture, "1.6", (2900.0, 2000.0), 50.0, "t_1400_1400.png", legs(2891.0, 1990.0));
        let b = proposal(Category::SurfaceTexture, "1.6", (3020.0, 2000.0), 150.0, "t_2800_1400.png", legs(2892.0, 1988.0));
        let c = proposal(Category::SurfaceTexture, "?", (5700.0, 1200.0), 20.0, "t_4200_0.png", legs(5703.0, 1200.0));
        let d = proposal(Category::SurfaceTexture, "6.3", (5800.0, 1200.0), 150.0, "t_5600_0.png", legs(5702.0, 1205.0));
        let e = proposal(Category::SurfaceTexture, "1.6", (900.0, 900.0), 150.0, "t_0_0.png", legs(890.0, 890.0));
        let (kept, _) = dedupe(vec![a, b, c, d, e], &DedupeParams::default());
        let texts: Vec<String> = kept.iter().map(|k| k.proposal.text.clone()).collect();
        assert_eq!(texts.len(), 3, "{texts:?}");
        assert!(texts.contains(&"6.3".to_owned()) && !texts.contains(&"?".to_owned()), "{texts:?}");
    }

    #[test]
    fn hatch_pieces_of_one_region_merge_when_their_measured_angles_agree() {
        let hatch = |poly: Vec<P>, angle: f64, measured: bool| {
            let mut m = member("hatch_fill", Shape::Hatch { polygon: poly, angle_deg: angle, spacing_px: 20.0 }, 1.0);
            if measured {
                m.adjustments.push("hatch_measured:lines=9".into());
            }
            m
        };
        let a = proposal(Category::Hatch, "", (100.0, 100.0), 100.0, "t_0_0.png", vec![hatch(vec![(0.0, 0.0), (200.0, 0.0), (200.0, 200.0), (0.0, 200.0)], 45.0, true)]);
        let b = proposal(Category::Hatch, "", (300.0, 100.0), 100.0, "t_1400_0.png", vec![hatch(vec![(195.0, 0.0), (400.0, 0.0), (400.0, 200.0), (195.0, 200.0)], 46.5, true)]);
        let c = proposal(Category::Hatch, "", (300.0, 400.0), 100.0, "t_1400_0.png", vec![hatch(vec![(195.0, 250.0), (400.0, 250.0), (400.0, 450.0), (195.0, 450.0)], 135.0, true)]);
        let d = proposal(Category::Hatch, "", (600.0, 100.0), 100.0, "t_1400_0.png", vec![hatch(vec![(398.0, 0.0), (800.0, 0.0), (800.0, 200.0), (398.0, 200.0)], 135.0, true)]);
        let (kept, _) = dedupe(vec![a, b, c, d], &DedupeParams::default());
        assert_eq!(kept.len(), 3, "{:?}", kept.iter().map(|k| k.members.len()).collect::<Vec<_>>());
        let region = kept.iter().find(|k| k.members.len() == 2).expect("the two touching 45° pieces are one region with two pieces");
        assert!(region.members.iter().all(|m| m.role == "hatch_fill"));
    }

    #[test]
    fn dimensions_on_one_line_and_notes_on_one_leader_are_one_object() {
        let text_box = |x: f64| Shape::TextBox { bbox: [x, 100.0, x + 60.0, 130.0], vertical: false };
        // The same dimension read twice on two tiles: once as "Ø?" with the line, once fully, 150 px apart in centre.
        let a = proposal(Category::Dimension, "Ø?", (300.0, 160.0), 300.0, "t_0_0.png",
            vec![member("dimension_line", Shape::Segment((100.0, 150.0), (500.0, 150.0)), 0.95), member("dimension_text", text_box(270.0), 1.0)]);
        let b = proposal(Category::Dimension, "Ø35", (450.0, 160.0), 100.0, "t_1400_0.png",
            vec![member("dimension_line", Shape::Segment((102.0, 151.0), (498.0, 151.0)), 0.9), member("dimension_text", text_box(420.0), 1.0)]);
        // A thread note as a leader dimension and as a partial note on the same leader.
        let c = proposal(Category::Dimension, "M8 攻深22", (700.0, 160.0), 50.0, "t_0_0.png",
            vec![member("leader_line", Shape::Segment((600.0, 200.0), (700.0, 200.0)), 0.95), member("dimension_text", text_box(700.0), 1.0)]);
        let mut d = proposal(Category::Note, "攻深22", (720.0, 160.0), 400.0, "t_1400_0.png",
            vec![member("leader_line", Shape::Segment((602.0, 201.0), (698.0, 201.0)), 0.99), member("note_text", text_box(705.0), 1.0)]);
        d.proposal.kind = String::new();
        // An unrelated dimension on a parallel line 10 px away stays, and so does the next dimension
        // of a chain on the same line with another readable value.
        let e = proposal(Category::Dimension, "20", (300.0, 175.0), 100.0, "t_0_0.png",
            vec![member("dimension_line", Shape::Segment((100.0, 161.0), (500.0, 161.0)), 0.9)]);
        let f = proposal(Category::Dimension, "12", (560.0, 160.0), 100.0, "t_0_0.png",
            vec![member("dimension_line", Shape::Segment((470.0, 150.0), (650.0, 150.0)), 0.9), member("dimension_text", text_box(530.0), 1.0)]);
        let (kept, merges) = dedupe(vec![a, b, c, d, e, f], &DedupeParams::default());
        assert_eq!(kept.len(), 4, "{:?}", kept.iter().map(|k| k.proposal.text.clone()).collect::<Vec<_>>());
        let dim = kept.iter().find(|k| k.proposal.text == "Ø35").expect("the readable text wins");
        assert_eq!(dim.members.iter().filter(|m| m.role == "dimension_text").count(), 1, "no second text from the '?' reading");
        assert_eq!(dim.members.iter().filter(|m| m.role == "dimension_line").count(), 1, "one line, not two coincident ones");
        assert!((dim.members[0].support_after - 0.95).abs() < 1e-9, "the better supported line is taken");
        assert!(kept.iter().any(|k| k.proposal.text == "12"));
        let note = kept.iter().find(|k| k.proposal.text == "M8 攻深22").expect("the fuller text wins");
        assert_eq!(note.proposal.category, Category::Dimension);
        assert_eq!(note.members.iter().filter(|m| matches!(m.refined, Shape::TextBox { .. })).count(), 1);
        assert!(kept.iter().any(|k| k.proposal.text == "20"));
        assert_eq!(merges.len(), 2);
    }

    fn proposal(category: Category, text: &str, centre: P, edge: f64, tile: &str, members: Vec<RefinedMember>) -> RefinedProposal {
        RefinedProposal {
            proposal: Proposal {
                category,
                kind: String::new(),
                text: text.into(),
                centre,
                edge_distance: edge,
                members: members
                    .iter()
                    .map(|m| ProposedMember {
                        role: m.role.clone(),
                        shape: m.proposed.clone(),
                        text: None,
                    })
                    .collect(),
                provenance: Provenance {
                    tile: tile.into(),
                    tile_sha256: String::new(),
                    level: "L1A".into(),
                    model: "m".into(),
                    prompt_sha256: String::new(),
                    file: format!("{tile}.json"),
                    item_index: 0,
                    verification: None,
                },
                notes: vec![],
                bbox: None,
            },
            members,
        }
    }

    #[test]
    fn partial_readings_merge_with_the_whole_reading_by_position_and_wildcard_text() {
        assert!(wildcard_compatible("?01", "Ø20+0.02/-0.01"));
        assert!(wildcard_compatible("Ø?", "Ø30") && wildcard_compatible("Ø1?", "Ø12"));
        assert!(wildcard_compatible("3. 括号内尺寸为参考?", "?为参考尺寸；"));
        assert!(!wildcard_compatible("Ø2?", "Ø12") && !wildcard_compatible("?5", "Ø30"));
        let tb = |b: [f64; 4]| Shape::TextBox { bbox: b, vertical: false };
        let tip = |p: P| Shape::Tip { approx: p, along: (1.0, 0.0) };
        // The whole reading with an unsupported line; the partial one with the supported collinear line.
        let a = proposal(Category::Dimension, "Ø30", (4750.0, 1400.0), 100.0, "t_4200_0.png",
            vec![member("dimension_text", tb([4661.0, 1259.0, 4900.0, 1475.0]), 1.0), member("dimension_line", Shape::Segment((4167.0, 1791.0), (4838.0, 1321.0)), 0.77)]);
        let mut b = proposal(Category::Dimension, "Ø?", (4400.0, 1650.0), 20.0, "t_4200_1400.png",
            vec![member("dimension_text", tb([4661.0, 1400.0, 4816.0, 1476.0]), 0.0), member("dimension_line", Shape::Segment((4060.0, 1866.0), (4671.0, 1438.0)), 0.98)]);
        b.proposal.provenance.item_index = 1;
        // A text read whole (no geometry) and its end read with the line and arrows on the next tile.
        let c = proposal(Category::Dimension, "Ø20+0.02/-0.01", (3139.0, 998.0), 100.0, "t_2800_0.png", vec![member("dimension_text", tb([2925.0, 537.0, 3354.0, 1460.0]), 1.0)]);
        let d = proposal(Category::Dimension, "?01", (3500.0, 1700.0), 20.0, "t_2800_1400.png",
            vec![member("dimension_text", tb([3309.0, 1362.0, 3489.0, 1599.0]), 1.0), member("dimension_line", Shape::Segment((3204.0, 1052.0), (3793.0, 2315.0)), 0.84),
                 member("arrowhead", tip((3569.0, 1836.0)), 1.0), member("arrowhead", tip((3824.0, 2381.0)), 1.0)]);
        // A diameter read in part next to another one (boxes apart, one readable character): kept apart.
        let e = proposal(Category::Dimension, "Ø?", (700.0, 300.0), 20.0, "t_0_0.png", vec![member("dimension_text", tb([650.0, 280.0, 700.0, 320.0]), 1.0)]);
        let f = proposal(Category::Dimension, "Ø20", (760.0, 300.0), 20.0, "t_0_0.png", vec![member("dimension_text", tb([720.0, 280.0, 800.0, 320.0]), 1.0)]);
        let (kept, _) = dedupe(vec![a, b, c, d, e, f], &DedupeParams::default());
        assert_eq!(kept.len(), 4, "{:?}", kept.iter().map(|k| k.proposal.text.clone()).collect::<Vec<_>>());
        let d30 = kept.iter().find(|k| k.proposal.text == "Ø30").expect("the whole reading is kept");
        let line = d30.members.iter().find(|m| m.role == "dimension_line").unwrap();
        assert!(line.supported && (line.support_after - 0.98).abs() < 1e-9, "the supported line is taken");
        let d20 = kept.iter().find(|k| k.proposal.text == "Ø20+0.02/-0.01").expect("the whole reading is kept");
        assert!(d20.members.iter().any(|m| m.role == "dimension_line" && m.supported));
        assert_eq!(d20.members.iter().filter(|m| m.role == "arrowhead").count(), 2);
        assert_eq!(d20.members.iter().filter(|m| m.role == "dimension_text").count(), 1);
        assert!(kept.iter().any(|k| k.proposal.text == "Ø?") && kept.iter().any(|k| k.proposal.text == "Ø20"));
    }

    #[test]
    fn duplicates_across_tiles_merge_to_the_inner_proposal_with_the_better_members() {
        let seg = |y: f64| Shape::Segment((1420.0, y), (1580.0, y));
        let a = proposal(
            Category::Dimension,
            "☆Ø32 0/-0.025",
            (1500.0, 200.0),
            60.0,
            "t_0_0.png",
            vec![member("dimension_line", seg(220.0), 0.95), member("extension_line", Shape::Segment((1420.0, 100.0), (1420.0, 230.0)), 0.9)],
        );
        let b = proposal(
            Category::Dimension,
            "☆ φ32 0/-0.025",
            (1530.0, 205.0),
            130.0,
            "t_1400_0.png",
            vec![
                member("dimension_line", seg(221.0), 0.99),
                member("extension_line", Shape::Segment((1580.0, 100.0), (1580.0, 230.0)), 0.9),
            ],
        );
        let c = proposal(Category::Dimension, "Ø40", (1500.0, 600.0), 10.0, "t_0_0.png", vec![member("dimension_line", seg(620.0), 0.9)]);
        let (kept, merges) = dedupe(vec![a, b, c], &DedupeParams::default());
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].proposal.provenance.tile, "t_1400_0.png");
        // The dimension line of the kept proposal already had the higher support; the missing
        // extension line of the other tile was added.
        assert_eq!(kept[0].members.len(), 3);
        assert!(kept[0].members.iter().filter(|m| m.role == "extension_line").count() == 2);
        assert_eq!(merges.len(), 1);
        assert_eq!(merges[0].dropped, vec!["t_0_0.png#L1A:0"]);
        assert!(merges[0].swapped.is_empty());
        assert_eq!(merges[0].added.len(), 1);
        assert_eq!(kept[1].proposal.text, "Ø40");
    }

    #[test]
    fn higher_support_member_replaces_the_kept_one_and_far_apart_texts_stay() {
        let a = proposal(Category::Label, "A向", (100.0, 100.0), 200.0, "t_0_0.png", vec![member("view_title", Shape::TextBox { bbox: [80.0, 90.0, 120.0, 110.0], vertical: false }, 1.0)]);
        let mut weak = member("dimension_line", Shape::Segment((0.0, 50.0), (100.0, 50.0)), 0.81);
        weak.width_px = 1.0;
        let strong = member("dimension_line", Shape::Segment((1.0, 50.0), (101.0, 50.0)), 0.99);
        let d1 = proposal(Category::Dimension, "30", (50.0, 40.0), 300.0, "t_0_0.png", vec![weak]);
        let d2 = proposal(Category::Dimension, "30", (52.0, 41.0), 100.0, "t_1400_0.png", vec![strong]);
        let d3 = proposal(Category::Dimension, "30", (500.0, 41.0), 100.0, "t_1400_0.png", vec![member("dimension_line", Shape::Segment((450.0, 50.0), (550.0, 50.0)), 0.9)]);
        let (kept, merges) = dedupe(vec![a, d1, d2, d3], &DedupeParams::default());
        assert_eq!(kept.len(), 3);
        let d = kept.iter().find(|k| k.proposal.edge_distance == 300.0).unwrap();
        assert!((d.members[0].support_after - 0.99).abs() < 1e-9);
        assert_eq!(d.members[0].merged_from.as_deref(), Some("t_1400_0.png#L1A:0"));
        assert_eq!(merges[0].swapped, vec![("dimension_line".to_owned(), "t_1400_0.png#L1A:0".to_owned())]);
    }

    #[test]
    fn centre_line_pieces_continue_across_tiles() {
        let a = proposal(Category::CentreLine, "", (700.0, 373.0), 200.0, "t_0_0.png", vec![member("center_line", Shape::Segment((343.0, 373.0), (1599.0, 373.0)), 0.9)]);
        let b = proposal(Category::CentreLine, "", (2000.0, 374.0), 199.0, "t_1400_0.png", vec![member("center_line", Shape::Segment((1420.0, 374.0), (2600.0, 374.0)), 0.9)]);
        let c = proposal(Category::CentreLine, "", (2000.0, 900.0), 199.0, "t_1400_0.png", vec![member("center_line", Shape::Segment((1420.0, 900.0), (2600.0, 900.0)), 0.9)]);
        let (kept, _) = dedupe(vec![a, b, c], &DedupeParams::default());
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].members.len(), 1, "the continuing piece joins the kept one into one segment");
        let Shape::Segment(p, q) = kept[0].members[0].refined else { panic!() };
        assert!(p.0 <= 343.5 && q.0 >= 2599.5, "{p:?} {q:?}");
        assert!(kept[0].members[0].merged_from.is_some());
    }

    #[test]
    fn one_stroke_seen_in_two_extents_becomes_one_member() {
        // The same dimension (text in the overlap of two tiles) whose line each tile saw up to its
        // edge: [100, 1600] and [1400, 2300]. The text rule merges the proposals; the two readings
        // of the line become one member spanning both, not two coincident lines.
        let a = proposal(Category::Dimension, "275", (1500.0, 100.0), 100.0, "t_0_0.png",
            vec![member("dimension_line", Shape::Segment((100.0, 150.0), (1600.0, 150.0)), 0.97)]);
        let b = proposal(Category::Dimension, "275", (1520.0, 100.0), 120.0, "t_1400_0.png",
            vec![member("dimension_line", Shape::Segment((1400.0, 150.5), (2300.0, 150.5)), 0.95)]);
        let (kept, merges) = dedupe(vec![a, b], &DedupeParams::default());
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].members.len(), 1);
        let Shape::Segment(p, q) = kept[0].members[0].refined else { panic!() };
        assert!((p.0 - 100.0).abs() < 1e-6 && (q.0 - 2300.0).abs() < 1e-6, "{p:?} {q:?}");
        assert!(kept[0].members[0].support_after > 0.95);
        assert_eq!(merges[0].swapped, vec![("dimension_line:union".to_owned(), "t_0_0.png#L1A:0".to_owned())]);
        // Two partial readings of one line, the second with its number cut off by the tile edge,
        // share the tile overlap and are one dimension; two readable, different values are a chain
        // however much their lines touch.
        let c = proposal(Category::Dimension, "275", (800.0, 100.0), 200.0, "t_0_0.png",
            vec![member("dimension_line", Shape::Segment((100.0, 150.0), (1600.0, 150.0)), 0.97)]);
        let d = proposal(Category::Dimension, "?75", (1900.0, 100.0), 150.0, "t_1400_0.png",
            vec![member("dimension_line", Shape::Segment((1400.0, 150.5), (2300.0, 150.5)), 0.95)]);
        let (kept, _) = dedupe(vec![c, d], &DedupeParams::default());
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].proposal.text, "275");
        let c = proposal(Category::Dimension, "275", (800.0, 100.0), 200.0, "t_0_0.png",
            vec![member("dimension_line", Shape::Segment((100.0, 150.0), (1600.0, 150.0)), 0.97)]);
        let d = proposal(Category::Dimension, "40", (1900.0, 100.0), 150.0, "t_1400_0.png",
            vec![member("dimension_line", Shape::Segment((1400.0, 150.5), (2300.0, 150.5)), 0.95)]);
        assert_eq!(dedupe(vec![c, d], &DedupeParams::default()).0.len(), 2);
        // An unread dimension whose line is the leader of a note is dropped for the note.
        let n = proposal(Category::Note, "M8 攻深22", (700.0, 160.0), 50.0, "t_0_0.png",
            vec![member("leader_line", Shape::Segment((600.0, 300.0), (900.0, 300.0)), 0.95), member("note_text", Shape::TextBox { bbox: [700.0, 100.0, 760.0, 130.0], vertical: false }, 1.0)]);
        let q = proposal(Category::Dimension, "?", (750.0, 320.0), 50.0, "t_1400_0.png",
            vec![member("dimension_line", Shape::Segment((602.0, 300.5), (899.0, 300.5)), 0.9), member("dimension_text", Shape::TextBox { bbox: [700.0, 100.0, 760.0, 130.0], vertical: false }, 1.0)]);
        let (kept, _) = dedupe(vec![n, q], &DedupeParams::default());
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].proposal.category, Category::Note);
        assert!(kept[0].members.iter().all(|m| m.role != "dimension_line"), "the unread dimension lends no line");
    }
}
