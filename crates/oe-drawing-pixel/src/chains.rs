//! Leaders as polylines: the refined leader pieces of a proposal are ordered into chains by endpoint
//! proximity (or, for pieces overlapping or falling short at a corner, by the intersection of their
//! lines), consecutive pieces meet at that intersection (exact shared vertices, so no piece overshoots
//! the next), near-axis pieces are levelled, and each chain becomes one leader member with a polyline
//! shape. A dimension line observed on a leader's own stroke (in a note proposal, or one whose kind is
//! leader) is part of that leader piece. A text of the same proposal is kept at or above a shelf under it:
//! its box bottom at most a tenth of its height below the shelf line, the tolerance the checker's
//! crossing test applies.

use crate::geom::{P, add, dist, median, mul, sub, unit};
use crate::lines::snap_axis;
use crate::observe::{Category, Shape};
use crate::refine::{RefinedMember, RefinedProposal};
use crate::stroke::Pattern;

/// Pieces whose ends lie this close join into one chain.
const JOIN_PX: f64 = 12.0;
/// A joint at the intersection of two pieces is used when it lies this close to both ends.
const JOINT_PX: f64 = 15.0;

fn line_intersection(a: P, b: P, c: P, d: P) -> Option<P> {
    let (r, s) = (sub(b, a), sub(d, c));
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-9 {
        return None;
    }
    let t = ((c.0 - a.0) * s.1 - (c.1 - a.1) * s.0) / den;
    Some(add(a, mul(r, t)))
}

/// How far the end `e` of a piece running `from → e` is from joining the end `f` of a piece running
/// `f → g`: their distance, or, when the pieces meet at 3° or more and their lines intersect within
/// `JOINT_PX` of both ends (the pieces overlap at the corner or fall short of it), the larger distance
/// to that intersection.
fn join_distance(from: P, e: P, f: P, g: P) -> f64 {
    let sharp = match (unit(sub(e, from)), unit(sub(g, f))) {
        (Some(u), Some(w)) => (u.0 * w.1 - u.1 * w.0).abs() >= 0.052,
        _ => false,
    };
    let via = line_intersection(from, e, f, g)
        .filter(|_| sharp)
        .map(|x| dist(x, e).max(dist(x, f)))
        .filter(|d| *d <= JOINT_PX)
        .unwrap_or(f64::INFINITY);
    dist(e, f).min(via)
}

/// The union of two segments on one line (within 3 px and 2°, overlapping), along the first.
fn coincident_union(a: (P, P), b: (P, P)) -> Option<(P, P)> {
    let u = unit(sub(a.1, a.0))?;
    let w = unit(sub(b.1, b.0))?;
    if (u.0 * w.1 - u.1 * w.0).abs() > 2f64.to_radians().sin() {
        return None;
    }
    let off = |p: P| ((p.0 - a.0.0) * -u.1 + (p.1 - a.0.1) * u.0).abs();
    let along = |p: P| (p.0 - a.0.0) * u.0 + (p.1 - a.0.1) * u.1;
    if off(b.0) > 3.0 || off(b.1) > 3.0 {
        return None;
    }
    let (s0, s1) = (0.0, dist(a.0, a.1));
    let (t0, t1) = (along(b.0).min(along(b.1)), along(b.0).max(along(b.1)));
    if t1 < s0 - 2.0 || t0 > s1 + 2.0 {
        return None;
    }
    Some((add(a.0, mul(u, s0.min(t0))), add(a.0, mul(u, s1.max(t1)))))
}

/// Order pieces into chains: start at a free end (preferably the one nearest an arrow tip), append
/// the piece whose end is nearest to joining the current end (`join_distance` within `JOIN_PX`),
/// oriented to continue it. Returns chains of `(piece index, reversed)`.
fn order(pieces: &[(P, P)], tips: &[P]) -> Vec<Vec<(usize, bool)>> {
    let n = pieces.len();
    let mut used = vec![false; n];
    let mut chains = Vec::new();
    let free = |i: usize, at_p: bool, used: &[bool]| {
        let (e, from) = if at_p { (pieces[i].0, pieces[i].1) } else { (pieces[i].1, pieces[i].0) };
        !(0..n).any(|j| {
            j != i && !used[j] && (join_distance(from, e, pieces[j].0, pieces[j].1) <= JOIN_PX || join_distance(from, e, pieces[j].1, pieces[j].0) <= JOIN_PX)
        })
    };
    while used.iter().any(|u| !u) {
        // The start: a free end, the one nearest a tip first; else the first unused piece.
        let mut start: Option<(usize, bool, f64)> = None;
        for i in (0..n).filter(|i| !used[*i]) {
            for at_p in [true, false] {
                if !free(i, at_p, &used) {
                    continue;
                }
                let e = if at_p { pieces[i].0 } else { pieces[i].1 };
                let d = tips.iter().map(|t| dist(*t, e)).fold(f64::INFINITY, f64::min);
                if start.is_none_or(|(_, _, b)| d < b) {
                    start = Some((i, at_p, d));
                }
            }
        }
        let (first, at_p) = match start {
            Some((i, at_p, _)) => (i, at_p),
            None => ((0..n).find(|i| !used[*i]).expect("an unused piece"), true),
        };
        // Oriented so the chain starts at the free end.
        let mut chain = vec![(first, !at_p)];
        used[first] = true;
        let (mut from, mut end) = if at_p { (pieces[first].0, pieces[first].1) } else { (pieces[first].1, pieces[first].0) };
        loop {
            let next = (0..n)
                .filter(|j| !used[*j])
                .flat_map(|j| {
                    [
                        (j, false, join_distance(from, end, pieces[j].0, pieces[j].1)),
                        (j, true, join_distance(from, end, pieces[j].1, pieces[j].0)),
                    ]
                })
                .filter(|(_, _, d)| *d <= JOIN_PX)
                .min_by(|a, b| a.2.total_cmp(&b.2));
            let Some((j, reversed, _)) = next else { break };
            used[j] = true;
            (from, end) = if reversed { (pieces[j].1, pieces[j].0) } else { (pieces[j].0, pieces[j].1) };
            chain.push((j, reversed));
        }
        chains.push(chain);
    }
    chains
}

/// Vertices of an ordered chain: its free start, the joints and its free end.
fn vertices(pieces: &[(P, P)], chain: &[(usize, bool)]) -> Vec<P> {
    let oriented: Vec<(P, P)> = chain
        .iter()
        .map(|&(i, rev)| if rev { (pieces[i].1, pieces[i].0) } else { pieces[i] })
        .collect();
    let mut v = vec![oriented[0].0];
    for k in 0..oriented.len().saturating_sub(1) {
        let (a, b) = oriented[k];
        let (c, d) = oriented[k + 1];
        let sharp = match (unit(sub(b, a)), unit(sub(d, c))) {
            (Some(u), Some(w)) => (u.0 * w.1 - u.1 * w.0).abs() >= 0.052,
            _ => false,
        };
        let joint = line_intersection(a, b, c, d)
            .filter(|x| sharp && dist(*x, b) <= JOINT_PX && dist(*x, c) <= JOINT_PX)
            .unwrap_or(((b.0 + c.0) * 0.5, (b.1 + c.1) * 0.5));
        v.push(joint);
    }
    v.push(oriented[oriented.len() - 1].1);
    // Level the pieces within the axis tolerance; a levelled piece moves both its vertices.
    for k in 0..v.len() - 1 {
        if let Some(((p, q), _)) = snap_axis(v[k], v[k + 1]) {
            v[k] = p;
            v[k + 1] = q;
        }
    }
    v
}

/// Keep a text at or above a shelf of its own leader: when a near-horizontal piece runs under the
/// text box (overlapping a third of its width, below its middle) and the box bottom reaches more than
/// a tenth of the height below it, the box bottom is clipped to the shelf. Returns the clip in px.
fn keep_above_shelf(text: &mut RefinedMember, chain: &[P]) -> Option<f64> {
    let Shape::TextBox { bbox, vertical: false } = &mut text.refined else {
        return None;
    };
    let h = bbox[3] - bbox[1] + 1.0;
    for w in chain.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = dist(a, b);
        if len < 4.0 || (b.1 - a.1).abs() > 0.05 * len {
            continue;
        }
        let y = 0.5 * (a.1 + b.1);
        let overlap = a.0.max(b.0).min(bbox[2]) - a.0.min(b.0).max(bbox[0]);
        if overlap < 0.3 * (bbox[2] - bbox[0]) || y < bbox[1] + 0.5 * h || y > bbox[3] + h {
            continue;
        }
        let bottom = bbox[3] + 0.5;
        let allowed = y + 0.1 * h;
        if bottom > allowed {
            let clip = bottom - (y + 0.05 * h);
            if clip > 0.6 * h {
                return None;
            }
            bbox[3] -= clip;
            return Some(clip);
        }
    }
    None
}

/// Turn the leader pieces of every proposal into polyline members. Returns the number of chains.
pub(crate) fn chain_leaders(kept: &mut [RefinedProposal]) -> usize {
    let mut chains_made = 0;
    for rp in kept.iter_mut() {
        // A dimension line on the leader's own stroke, in a note, a leader or a dimension observed as a leader.
        if rp.proposal.category != Category::Dimension || rp.proposal.kind == "leader" {
            let mut absorbed: Vec<usize> = Vec::new();
            for d in 0..rp.members.len() {
                let Shape::Segment(a, b) = rp.members[d].refined else { continue };
                if !(rp.members[d].supported && rp.members[d].role == "dimension_line") {
                    continue;
                }
                for l in 0..rp.members.len() {
                    let Shape::Segment(p, q) = rp.members[l].refined else { continue };
                    if l == d || !rp.members[l].supported || rp.members[l].role != "leader_line" {
                        continue;
                    }
                    if let Some((u, v)) = coincident_union((p, q), (a, b)) {
                        rp.members[l].refined = Shape::Segment(u, v);
                        rp.members[l].adjustments.push("dimension_line_on_leader_stroke_joined".into());
                        absorbed.push(d);
                        break;
                    }
                }
            }
            let mut k = 0;
            rp.members.retain(|_| {
                let keep = !absorbed.contains(&k);
                k += 1;
                keep
            });
        }
        let idx: Vec<usize> = rp
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| m.supported && m.role == "leader_line" && matches!(m.refined, Shape::Segment(..)))
            .map(|(i, _)| i)
            .collect();
        if idx.is_empty() {
            continue;
        }
        let pieces: Vec<(P, P)> = idx
            .iter()
            .map(|&i| match rp.members[i].refined {
                Shape::Segment(p, q) => (p, q),
                _ => unreachable!("filtered to segments"),
            })
            .collect();
        let proposed: Vec<(P, P)> = idx
            .iter()
            .map(|&i| match rp.members[i].proposed {
                Shape::Segment(p, q) => (p, q),
                _ => pieces[0],
            })
            .collect();
        let tips: Vec<P> = rp
            .members
            .iter()
            .filter(|m| m.supported && m.role == "arrowhead")
            .filter_map(|m| match m.refined {
                Shape::Tip { approx, .. } => Some(approx),
                _ => None,
            })
            .collect();
        let mut made: Vec<RefinedMember> = Vec::new();
        for chain in order(&pieces, &tips) {
            let v = vertices(&pieces, &chain);
            let pv = vertices(&proposed, &chain);
            let members: Vec<&RefinedMember> = chain.iter().map(|&(k, _)| &rp.members[idx[k]]).collect();
            let total: f64 = chain.iter().map(|&(k, _)| dist(pieces[k].0, pieces[k].1)).sum();
            let support = if total > 0.0 {
                chain.iter().map(|&(k, _)| rp.members[idx[k]].support_after * dist(pieces[k].0, pieces[k].1)).sum::<f64>() / total
            } else {
                0.0
            };
            let mut widths: Vec<f64> = members.iter().map(|m| m.width_px).filter(|w| *w > 0.0).collect();
            let pattern = if members.iter().all(|m| m.pattern == Some(Pattern::Continuous)) {
                Some(Pattern::Continuous)
            } else {
                members.iter().find_map(|m| m.pattern.filter(|p| *p != Pattern::Continuous))
            };
            let mut m = members[0].clone();
            m.proposed = Shape::Polyline(pv);
            m.refined = Shape::Polyline(v);
            m.support_before = members.iter().map(|x| x.support_before).sum::<f64>() / members.len() as f64;
            m.support_after = support;
            m.width_px = median(&mut widths).unwrap_or(m.width_px);
            m.pattern = pattern;
            m.supported = true;
            m.moved_px = members.iter().map(|x| x.moved_px).fold(0.0, f64::max);
            m.merged_from = members.iter().find_map(|x| x.merged_from.clone());
            let mut adjustments = vec![format!("leader_chain:pieces={}", chain.len())];
            for x in &members {
                for a in &x.adjustments {
                    if !adjustments.contains(a) {
                        adjustments.push(a.clone());
                    }
                }
            }
            m.adjustments = adjustments;
            made.push(m);
            chains_made += 1;
        }
        // The text kept above the shelf of its own leader.
        for t in rp.members.iter_mut().filter(|m| m.supported && matches!(m.refined, Shape::TextBox { .. })) {
            for chain in &made {
                if let Shape::Polyline(v) = &chain.refined
                    && let Some(clip) = keep_above_shelf(t, v)
                {
                    t.adjustments.push(format!("text_above_shelf:clipped_{clip:.1}px"));
                }
            }
        }
        let mut k = 0;
        rp.members.retain(|_| {
            let keep = !idx.contains(&k);
            k += 1;
            keep
        });
        rp.members.extend(made);
    }
    chains_made
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::{Proposal, ProposedMember, Provenance};

    fn seg(role: &str, p: P, q: P) -> RefinedMember {
        RefinedMember {
            role: role.into(),
            text: None,
            proposed: Shape::Segment(p, q),
            refined: Shape::Segment(p, q),
            support_before: 1.0,
            support_after: 1.0,
            moved_px: 0.0,
            width_px: 3.0,
            pattern: Some(Pattern::Continuous),
            supported: true,
            merged_from: None,
            arrow_base: None,
            arrow_length_px: None,
            discarded_components: 0,
            adjustments: vec![],
            span_px: None,
        }
    }

    fn proposal(members: Vec<RefinedMember>) -> RefinedProposal {
        RefinedProposal {
            proposal: Proposal {
                category: Category::Note,
                kind: String::new(),
                text: "note".into(),
                centre: (140.0, 180.0),
                edge_distance: 100.0,
                members: members.iter().map(|m| ProposedMember { role: m.role.clone(), shape: m.proposed.clone(), text: None }).collect(),
                provenance: Provenance {
                    tile: "t".into(),
                    tile_sha256: String::new(),
                    level: "L1B".into(),
                    model: "m".into(),
                    prompt_sha256: String::new(),
                    file: "f".into(),
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
    fn pieces_overlapping_at_a_corner_join_and_a_dimension_line_on_the_leader_stroke_is_part_of_it() {
        // A shelf (100, 200)–(180, 200) and an inclined piece starting 12 px inside it at (168, 201), going to
        // (220, 150): their ends are 12.04 px apart, their lines meet near (169, 200).
        let mut rp = proposal(vec![seg("leader_line", (100.0, 200.0), (180.0, 200.0)), seg("leader_line", (168.0, 201.0), (220.0, 150.0))]);
        assert_eq!(chain_leaders(std::slice::from_mut(&mut rp)), 1);
        let Shape::Polyline(v) = &rp.members.iter().find(|m| m.role == "leader_line").unwrap().refined else { panic!() };
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(dist(v[1], (169.0, 200.0)) <= 1.5, "{v:?}");
        // A dimension line observed along the inclined piece (and beyond it) becomes part of that piece.
        let mut rp = proposal(vec![
            seg("leader_line", (100.0, 200.0), (180.0, 200.0)),
            seg("leader_line", (180.0, 200.0), (220.0, 160.0)),
            seg("dimension_line", (179.0, 201.0), (260.0, 120.0)),
        ]);
        assert_eq!(chain_leaders(std::slice::from_mut(&mut rp)), 1);
        assert!(!rp.members.iter().any(|m| m.role == "dimension_line"));
        let Shape::Polyline(v) = &rp.members.iter().find(|m| m.role == "leader_line").unwrap().refined else { panic!() };
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(dist(v[0], (260.0, 120.0)) <= 1.5 || dist(v[2], (260.0, 120.0)) <= 1.5, "{v:?}");
    }

    #[test]
    fn leader_pieces_become_one_polyline_with_shared_vertices_and_the_text_stays_above_the_shelf() {
        // A shelf under a note from (100, 200) to (180, 200.4) and an inclined piece from (178, 199) to (240, 260)
        // that overshoots the shelf's end; an arrow tip at (240, 260); the note box reaches 8 px below the shelf.
        let mut text = seg("note_text", (0.0, 0.0), (0.0, 0.0));
        text.refined = Shape::TextBox { bbox: [105.0, 160.0, 175.0, 208.0], vertical: false };
        let mut tip = seg("arrowhead", (0.0, 0.0), (0.0, 0.0));
        tip.refined = Shape::Tip { approx: (240.0, 260.0), along: (0.7, 0.7) };
        let members = vec![seg("leader_line", (100.0, 200.0), (180.0, 200.4)), seg("leader_line", (178.0, 199.0), (240.0, 260.0)), text, tip];
        let mut rp = RefinedProposal {
            proposal: Proposal {
                category: Category::Note,
                kind: String::new(),
                text: "note".into(),
                centre: (140.0, 180.0),
                edge_distance: 100.0,
                members: members
                    .iter()
                    .map(|m| ProposedMember { role: m.role.clone(), shape: m.proposed.clone(), text: None })
                    .collect(),
                provenance: Provenance {
                    tile: "t".into(),
                    tile_sha256: String::new(),
                    level: "L1B".into(),
                    model: "m".into(),
                    prompt_sha256: String::new(),
                    file: "f".into(),
                    item_index: 0,
                    verification: None,
                },
                notes: vec![],
                bbox: None,
            },
            members,
        };
        assert_eq!(chain_leaders(std::slice::from_mut(&mut rp)), 1);
        let chain = rp.members.iter().find(|m| m.role == "leader_line").unwrap();
        let Shape::Polyline(v) = &chain.refined else { panic!() };
        assert_eq!(v.len(), 3, "{v:?}");
        // It starts at the arrow tip, and the joint is the intersection of the two pieces' lines.
        assert!(dist(v[0], (240.0, 260.0)) < 1e-9, "{v:?}");
        assert!((v[1].1 - v[2].1).abs() < 1e-9, "the shelf is level: {v:?}");
        let x = line_intersection((100.0, 200.2), (180.0, 200.2), (178.0, 199.0), (240.0, 260.0)).unwrap();
        assert!(dist(v[1], x) < 0.5, "{v:?} {x:?}");
        let text = rp.members.iter().find(|m| m.role == "note_text").unwrap();
        let Shape::TextBox { bbox, .. } = text.refined else { panic!() };
        // Bottom edge (bbox[3] + 0.5) at most 0.1 h below the shelf (y = 200.2).
        assert!(bbox[3] + 0.5 <= 200.2 + 0.1 * (bbox[3] - bbox[1] + 1.0) + 1e-9, "{bbox:?}");
        assert!(text.adjustments.iter().any(|a| a.starts_with("text_above_shelf")));
    }
}
