//! Zhang–Suen thinning, pruning to a minimal 8-connected skeleton, and tracing of the skeleton into
//! pixel chains between endpoints and junctions.

use crate::Ink;

/// 8-neighbour offsets in ring order N, NE, E, SE, S, SW, W, NW (Zhang–Suen's P2..P9).
pub(crate) const N8: [(i64, i64); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// A binary grid the size of the page.
pub(crate) struct Grid {
    pub w: usize,
    pub h: usize,
    pub on: Vec<bool>,
}

impl Grid {
    #[inline]
    pub fn get(&self, x: i64, y: i64) -> bool {
        x >= 0
            && y >= 0
            && (x as usize) < self.w
            && (y as usize) < self.h
            && self.on[y as usize * self.w + x as usize]
    }

    #[inline]
    pub fn idx(&self, x: i64, y: i64) -> usize {
        y as usize * self.w + x as usize
    }

    #[inline]
    fn ring(&self, x: i64, y: i64) -> [bool; 8] {
        let mut r = [false; 8];
        for (k, (dx, dy)) in N8.iter().enumerate() {
            r[k] = self.get(x + dx, y + dy);
        }
        r
    }

    pub fn degree(&self, x: i64, y: i64) -> usize {
        self.ring(x, y).iter().filter(|&&b| b).count()
    }
}

/// Number of 0→1 transitions around the ring (Zhang–Suen's A(P1)).
fn transitions(r: &[bool; 8]) -> usize {
    (0..8).filter(|&k| !r[k] && r[(k + 1) % 8]).count()
}

/// Yokoi connectivity number for 8-connectivity. `1` means the pixel is simple: deleting it does
/// not change the number of components or holes.
fn connectivity8(r: &[bool; 8]) -> usize {
    // Yokoi's x1..x8 = E, NE, N, NW, W, SW, S, SE, expressed in ring indices.
    const MAP: [usize; 8] = [2, 1, 0, 7, 6, 5, 4, 3];
    let x = |k: usize| -> i32 { i32::from(r[MAP[(k - 1) % 8]]) };
    let mut c = 0;
    for k in [1usize, 3, 5, 7] {
        let nx = 1 - x(k);
        let nx1 = 1 - x(k + 1);
        let nx2 = 1 - x(k + 2);
        c += nx - nx * nx1 * nx2;
    }
    c.max(0) as usize
}

/// Zhang–Suen thinning of the ink followed by removal of simple points so that the result is a
/// minimal 8-connected skeleton (no staircase corners, no 2-px thick diagonals left behind).
///
/// Known properties: endpoints of thick strokes recede by about half the stroke thickness, small
/// 2×2 blobs vanish, and junction pixels may shift by one pixel.
pub(crate) fn thin(ink: &Ink) -> Grid {
    let (w, h) = (ink.width, ink.height);
    let mut g = Grid {
        w,
        h,
        on: ink.mask().to_vec(),
    };
    // Only boundary pixels can be deleted; keep a candidate list instead of rescanning the page.
    let mut in_cand = vec![false; w * h];
    let mut cand: Vec<usize> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if g.on[i] && g.degree(x as i64, y as i64) < 8 {
                in_cand[i] = true;
                cand.push(i);
            }
        }
    }
    let mut to_del: Vec<usize> = Vec::new();
    loop {
        let mut changed = false;
        for step in 0..2 {
            to_del.clear();
            for &i in &cand {
                if !g.on[i] {
                    continue;
                }
                let (x, y) = ((i % w) as i64, (i / w) as i64);
                let r = g.ring(x, y);
                let b = r.iter().filter(|&&v| v).count();
                if !(2..=6).contains(&b) || transitions(&r) != 1 {
                    continue;
                }
                let (p2, p4, p6, p8) = (r[0], r[2], r[4], r[6]);
                // Step 1 removes south-east boundary points (P2·P4·P6 = 0 and P4·P6·P8 = 0),
                // step 2 north-west ones (P2·P4·P8 = 0 and P2·P6·P8 = 0); written minimally.
                let ok = if step == 0 {
                    !p4 || !p6 || (!p2 && !p8)
                } else {
                    !p2 || !p8 || (!p4 && !p6)
                };
                if ok {
                    to_del.push(i);
                }
            }
            if to_del.is_empty() {
                continue;
            }
            changed = true;
            for &i in &to_del {
                g.on[i] = false;
            }
            for &i in &to_del {
                let (x, y) = ((i % w) as i64, (i / w) as i64);
                for (dx, dy) in N8 {
                    let (nx, ny) = (x + dx, y + dy);
                    if g.get(nx, ny) {
                        let j = g.idx(nx, ny);
                        if !in_cand[j] {
                            in_cand[j] = true;
                            cand.push(j);
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
        cand.retain(|&i| g.on[i]);
    }
    prune(&mut g);
    g
}

/// Sequentially (raster order, repeated until stable) delete every simple point that is neither an
/// endpoint nor a crossing centre (a pixel with three or four 4-adjacent skeleton neighbours, whose
/// removal would shift a T or + junction by a pixel). Preserves topology; yields a skeleton in which
/// every pixel of degree 2 has exactly two mutually non-adjacent neighbours, so tracing is
/// unambiguous.
fn prune(g: &mut Grid) {
    let pixels: Vec<usize> = (0..g.on.len()).filter(|&i| g.on[i]).collect();
    loop {
        let mut changed = false;
        for &i in &pixels {
            if !g.on[i] {
                continue;
            }
            let (x, y) = ((i % g.w) as i64, (i / g.w) as i64);
            let r = g.ring(x, y);
            let deg = r.iter().filter(|&&v| v).count();
            let four = [r[0], r[2], r[4], r[6]].iter().filter(|&&v| v).count();
            if deg >= 2 && four < 3 && connectivity8(&r) == 1 {
                g.on[i] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// A traced skeleton chain: consecutive 8-connected pixels. Open chains start and end at an
/// endpoint or at the representative pixel of a junction cluster (shared between all branches that
/// meet there); closed chains are loops whose last pixel is adjacent to the first.
pub(crate) struct Chain {
    pub pts: Vec<(i32, i32)>,
    pub closed: bool,
}

/// Junction pixels (degree ≥ 3) grouped into 8-connected clusters, each with a representative.
struct Junctions {
    /// Cluster id per pixel, `usize::MAX` for non-junction pixels.
    cluster: Vec<usize>,
    /// Representative pixel per cluster: most 4-adjacent neighbours, then highest degree, then
    /// nearest the cluster centroid, then raster order.
    reps: Vec<(i64, i64)>,
}

impl Junctions {
    fn rep(&self, g: &Grid, p: (i64, i64)) -> (i64, i64) {
        self.reps[self.cluster[g.idx(p.0, p.1)]]
    }
}

fn junctions(g: &Grid, deg: &[u8], on_idx: &[usize]) -> Junctions {
    let w = g.w;
    let at = |i: usize| ((i % w) as i64, (i / w) as i64);
    let mut cluster = vec![usize::MAX; g.on.len()];
    let mut reps = Vec::new();
    for &i in on_idx {
        if deg[i] < 3 || cluster[i] != usize::MAX {
            continue;
        }
        let id = reps.len();
        cluster[i] = id;
        let mut members = vec![i];
        let mut k = 0;
        while k < members.len() {
            let (x, y) = at(members[k]);
            for (dx, dy) in N8 {
                let (nx, ny) = (x + dx, y + dy);
                if g.get(nx, ny) {
                    let j = g.idx(nx, ny);
                    if deg[j] >= 3 && cluster[j] == usize::MAX {
                        cluster[j] = id;
                        members.push(j);
                    }
                }
            }
            k += 1;
        }
        members.sort_unstable();
        let n = members.len() as f64;
        let (mut cx, mut cy) = (0.0, 0.0);
        for &m in &members {
            let (x, y) = at(m);
            cx += x as f64 / n;
            cy += y as f64 / n;
        }
        let key = |m: usize| {
            let (x, y) = at(m);
            let four = [(0, -1), (1, 0), (0, 1), (-1, 0)]
                .iter()
                .filter(|(dx, dy)| g.get(x + dx, y + dy))
                .count();
            let d2 = (x as f64 - cx).powi(2) + (y as f64 - cy).powi(2);
            (four, deg[m], -d2)
        };
        let mut best = members[0];
        let mut best_key = key(best);
        for &m in &members[1..] {
            let k = key(m);
            if k.0 > best_key.0
                || (k.0 == best_key.0
                    && (k.1 > best_key.1 || (k.1 == best_key.1 && k.2 > best_key.2)))
            {
                best = m;
                best_key = k;
            }
        }
        reps.push(at(best));
    }
    Junctions { cluster, reps }
}

/// Trace a (minimal) skeleton into chains. Pass 1 walks from endpoints, pass 2 from junction pixels
/// along each unvisited branch, pass 3 picks up the remaining pure loops. All orders are fixed.
pub(crate) fn trace(g: &Grid) -> Vec<Chain> {
    let (w, h) = (g.w, g.h);
    let mut deg = vec![0u8; w * h];
    let mut on_idx = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if g.on[i] {
                deg[i] = g.degree(x as i64, y as i64) as u8;
                on_idx.push(i);
            }
        }
    }
    let jn = junctions(g, &deg, &on_idx);
    let topo = Topo {
        g,
        deg: &deg,
        jn: &jn,
    };
    let mut visited = vec![false; w * h];
    let mut chains = Vec::new();
    let at = |i: usize| ((i % w) as i64, (i / w) as i64);
    let px = |p: (i64, i64)| (p.0 as i32, p.1 as i32);
    // Pass 1: endpoints.
    for &i in &on_idx {
        if deg[i] == 1 && !visited[i] {
            let s = at(i);
            visited[i] = true;
            let mut pts = vec![px(s)];
            walk(&topo, &mut visited, None, s, None, &mut pts);
            chains.push(Chain { pts, closed: false });
        }
    }
    // Pass 2: branches leaving junction pixels, starting at the cluster representative.
    for &i in &on_idx {
        if deg[i] < 3 {
            continue;
        }
        let j = at(i);
        let rep = jn.rep(g, j);
        for (dx, dy) in N8 {
            let n = (j.0 + dx, j.1 + dy);
            if !g.get(n.0, n.1) {
                continue;
            }
            let ni = g.idx(n.0, n.1);
            if deg[ni] >= 3 || visited[ni] {
                continue;
            }
            visited[ni] = true;
            let mut pts = vec![px(rep)];
            if j != rep {
                pts.push(px(j));
            }
            pts.push(px(n));
            if deg[ni] == 2 {
                walk(&topo, &mut visited, Some(j), n, None, &mut pts);
            }
            chains.push(Chain { pts, closed: false });
        }
    }
    // Pass 3: closed loops without any endpoint or junction.
    for &i in &on_idx {
        if deg[i] == 2 && !visited[i] {
            let s = at(i);
            visited[i] = true;
            let mut pts = vec![px(s)];
            let closed = walk(&topo, &mut visited, None, s, Some(s), &mut pts);
            chains.push(Chain { pts, closed });
        }
    }
    chains
}

/// Skeleton topology shared by the tracing passes.
struct Topo<'a> {
    g: &'a Grid,
    deg: &'a [u8],
    jn: &'a Junctions,
}

/// Walk from `cur` (already pushed) through degree-2 pixels until an endpoint, a junction pixel
/// (then its cluster representative), a dead end, or `loop_start`. Returns `true` when the walk
/// closed onto `loop_start`.
fn walk(
    t: &Topo,
    visited: &mut [bool],
    mut prev: Option<(i64, i64)>,
    mut cur: (i64, i64),
    loop_start: Option<(i64, i64)>,
    pts: &mut Vec<(i32, i32)>,
) -> bool {
    let (g, deg, jn) = (t.g, t.deg, t.jn);
    loop {
        let mut next: Option<(i64, i64)> = None;
        let mut junction: Option<(i64, i64)> = None;
        for (dx, dy) in N8 {
            let n = (cur.0 + dx, cur.1 + dy);
            if !g.get(n.0, n.1) || Some(n) == prev {
                continue;
            }
            if loop_start == Some(n) && pts.len() > 2 {
                return true;
            }
            let ni = g.idx(n.0, n.1);
            if deg[ni] >= 3 {
                if junction.is_none() {
                    junction = Some(n);
                }
                continue;
            }
            if visited[ni] {
                continue;
            }
            if next.is_none() {
                next = Some(n);
            }
        }
        match (next, junction) {
            (Some(n), _) => {
                let ni = g.idx(n.0, n.1);
                visited[ni] = true;
                pts.push((n.0 as i32, n.1 as i32));
                if deg[ni] != 2 {
                    return false;
                }
                prev = Some(cur);
                cur = n;
            }
            (None, Some(j)) => {
                pts.push((j.0 as i32, j.1 as i32));
                let rep = jn.rep(g, j);
                if rep != j {
                    pts.push((rep.0 as i32, rep.1 as i32));
                }
                return false;
            }
            (None, None) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str]) -> Ink {
        let h = rows.len();
        let w = rows[0].len();
        let mut m = Vec::with_capacity(w * h);
        for r in rows {
            for ch in r.chars() {
                m.push(ch == '#');
            }
        }
        Ink::from_mask(w, h, m)
    }

    #[test]
    fn thick_bar_thins_to_one_row_and_one_chain() {
        let ink = grid(&[
            "..........",
            ".########.",
            ".########.",
            ".########.",
            "..........",
        ]);
        let g = thin(&ink);
        let on: Vec<(usize, usize)> = (0..g.on.len())
            .filter(|&i| g.on[i])
            .map(|i| (i % g.w, i / g.w))
            .collect();
        assert!(
            on.iter().all(|&(_, y)| y == 2),
            "skeleton must be the centre row: {on:?}"
        );
        let chains = trace(&g);
        assert_eq!(chains.len(), 1);
        assert!(!chains[0].closed);
    }

    #[test]
    fn plus_junction_gives_four_branches() {
        let ink = grid(&[
            "....#....",
            "....#....",
            "....#....",
            "....#....",
            "#########",
            "....#....",
            "....#....",
            "....#....",
            "....#....",
        ]);
        let g = thin(&ink);
        let chains = trace(&g);
        assert_eq!(chains.len(), 4);
        for c in &chains {
            assert_eq!(c.pts.len(), 5);
            assert!(c.pts.contains(&(4, 4)));
        }
    }

    #[test]
    fn t_junction_keeps_its_centre() {
        let ink = grid(&[
            ".........",
            "#########",
            "....#....",
            "....#....",
            "....#....",
            "....#....",
        ]);
        let g = thin(&ink);
        assert!(g.get(4, 1), "the crossing centre must survive pruning");
        let chains = trace(&g);
        assert_eq!(chains.len(), 3);
        for c in &chains {
            assert!(c.pts.contains(&(4, 1)), "{:?}", c.pts);
            assert!(
                c.pts.first() == Some(&(4, 1)) || c.pts.last() == Some(&(4, 1)),
                "{:?}",
                c.pts
            );
        }
    }

    #[test]
    fn ring_is_one_closed_chain() {
        let ink = grid(&[
            ".......", "..###..", ".#...#.", ".#...#.", ".#...#.", "..###..", ".......",
        ]);
        let g = thin(&ink);
        let chains = trace(&g);
        assert_eq!(chains.len(), 1);
        assert!(chains[0].closed);
        assert_eq!(chains[0].pts.len(), 12);
    }

    #[test]
    fn staircase_corner_is_pruned() {
        let ink = grid(&["##...", ".##..", "..##.", "...##"]);
        let g = thin(&ink);
        let chains = trace(&g);
        assert_eq!(chains.len(), 1);
        // A minimal 8-connected diagonal needs one pixel per row/column step.
        assert!(chains[0].pts.len() <= 5, "{:?}", chains[0].pts);
    }
}
