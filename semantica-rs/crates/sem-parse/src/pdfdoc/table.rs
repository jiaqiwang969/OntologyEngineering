//! pdfplumber's default table extraction (`Page.extract_tables()`, "lines" strategy for both
//! directions): edges from lines, rectangle sides and curve segments; snapping
//! (`snap_tolerance=3`), joining (`join_tolerance=3`) and length filtering
//! (`edge_min_length=3`); intersections (`intersection_tolerance=3`); cells; contiguous
//! tables of more than one cell; cell text with `utils.extract_text`.

use std::collections::{HashMap, HashSet};

use super::interp::{Char, PathKind, PathObj};
use super::text::{cluster_objects, simple_text};

const SNAP: f64 = 3.0;
const JOIN: f64 = 3.0;
const EDGE_MIN_LENGTH: f64 = 3.0;
const EDGE_MIN_LENGTH_PREFILTER: f64 = 1.0;
const INTERSECTION_TOL: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Orient {
    H,
    V,
    None,
}

#[derive(Debug, Clone, Copy)]
struct Edge {
    x0: f64,
    x1: f64,
    top: f64,
    bottom: f64,
    width: f64,
    height: f64,
    orientation: Orient,
}

impl Edge {
    fn bbox_key(&self) -> (u64, u64, u64, u64) {
        (
            self.x0.to_bits(),
            self.top.to_bits(),
            self.x1.to_bits(),
            self.bottom.to_bits(),
        )
    }
}

fn page_edges(paths: &[PathObj]) -> Vec<Edge> {
    let mut lines = Vec::new();
    let mut rects = Vec::new();
    let mut curves = Vec::new();
    for p in paths {
        match p.kind {
            PathKind::Line => lines.push(Edge {
                x0: p.x0,
                x1: p.x1,
                top: p.top,
                bottom: p.bottom,
                width: p.width,
                height: p.height,
                orientation: if p.top == p.bottom { Orient::H } else { Orient::V },
            }),
            PathKind::Rect => {
                let base = |o: Orient| Edge {
                    x0: p.x0,
                    x1: p.x1,
                    top: p.top,
                    bottom: p.bottom,
                    width: p.width,
                    height: p.height,
                    orientation: o,
                };
                let mut top = base(Orient::H);
                top.height = 0.0;
                top.bottom = p.top;
                let mut bottom = base(Orient::H);
                bottom.height = 0.0;
                bottom.top = p.top + p.height;
                let mut left = base(Orient::V);
                left.width = 0.0;
                left.x1 = p.x0;
                let mut right = base(Orient::V);
                right.width = 0.0;
                right.x0 = p.x1;
                rects.extend([top, bottom, left, right]);
            }
            PathKind::Curve => {
                for pair in p.pts.windows(2) {
                    let (p0, p1) = (pair[0], pair[1]);
                    curves.push(Edge {
                        x0: p0.0.min(p1.0),
                        x1: p0.0.max(p1.0),
                        top: p0.1.min(p1.1),
                        bottom: p0.1.max(p1.1),
                        width: (p0.0 - p1.0).abs(),
                        height: (p0.1 - p1.1).abs(),
                        orientation: if p0.0 == p1.0 {
                            Orient::V
                        } else if p0.1 == p1.1 {
                            Orient::H
                        } else {
                            Orient::None
                        },
                    });
                }
            }
        }
    }
    lines.into_iter().chain(rects).chain(curves).collect()
}

fn filter_edges(edges: &[Edge], orientation: Option<Orient>, min_length: f64) -> Vec<Edge> {
    edges
        .iter()
        .filter(|e| {
            let dim = if e.orientation == Orient::V { e.height } else { e.width };
            orientation.is_none_or(|o| e.orientation == o) && dim >= min_length
        })
        .copied()
        .collect()
}

/// `snap_objects(objs, attr, tolerance)` for vertical (`x0`) or horizontal (`top`) edges.
fn snap(edges: Vec<Edge>, vertical: bool) -> Vec<Edge> {
    let keys: Vec<f64> = edges.iter().map(|e| if vertical { e.x0 } else { e.top }).collect();
    let mut out = Vec::new();
    for cluster in cluster_objects(&keys, SNAP, false) {
        let avg = cluster.iter().map(|&i| keys[i]).sum::<f64>() / cluster.len() as f64;
        for &i in &cluster {
            let mut e = edges[i];
            let d = avg - keys[i];
            if vertical {
                e.x0 += d;
                e.x1 += d;
            } else {
                e.top += d;
                e.bottom += d;
            }
            out.push(e);
        }
    }
    out
}

fn merge_edges(edges: Vec<Edge>) -> Vec<Edge> {
    let (v, h): (Vec<Edge>, Vec<Edge>) = edges.into_iter().partition(|e| e.orientation == Orient::V);
    let mut snapped = snap(v, true);
    snapped.extend(snap(h, false));
    // sorted by ("h", top) / ("v", x0): horizontal groups first.
    let key = |e: &Edge| -> (u8, f64) {
        if e.orientation == Orient::H { (0, e.top) } else { (1, e.x0) }
    };
    snapped.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::new();
    let mut i = 0;
    while i < snapped.len() {
        let k = key(&snapped[i]);
        let mut j = i;
        while j < snapped.len() && key(&snapped[j]) == k {
            j += 1;
        }
        let mut group: Vec<Edge> = snapped[i..j].to_vec();
        let horizontal = k.0 == 0;
        let min = |e: &Edge| if horizontal { e.x0 } else { e.top };
        let max = |e: &Edge| if horizontal { e.x1 } else { e.bottom };
        group.sort_by(|a, b| min(a).partial_cmp(&min(b)).unwrap_or(std::cmp::Ordering::Equal));
        let mut joined: Vec<Edge> = vec![group[0]];
        for e in &group[1..] {
            let last = *joined.last().expect("joined");
            if min(e) <= max(&last) + JOIN {
                if max(e) > max(&last) {
                    let mut ext = last;
                    if horizontal {
                        ext.x1 = e.x1;
                        ext.width = ext.x1 - ext.x0;
                    } else {
                        ext.bottom = e.bottom;
                        ext.height = ext.bottom - ext.top;
                    }
                    *joined.last_mut().expect("joined") = ext;
                }
            } else {
                joined.push(*e);
            }
        }
        out.extend(joined);
        i = j;
    }
    out
}

type Point = (u64, u64);

fn pt(x: f64, y: f64) -> Point {
    (x.to_bits(), y.to_bits())
}

fn val(p: Point) -> (f64, f64) {
    (f64::from_bits(p.0), f64::from_bits(p.1))
}

/// A table's cell bounding boxes (`x0, top, x1, bottom`).
type Cell = (f64, f64, f64, f64);

fn find_tables(paths: &[PathObj]) -> Vec<Vec<Cell>> {
    let all = page_edges(paths);
    let mut edges = filter_edges(&all, Some(Orient::V), EDGE_MIN_LENGTH_PREFILTER);
    edges.extend(filter_edges(&all, Some(Orient::H), EDGE_MIN_LENGTH_PREFILTER));
    let edges = filter_edges(&merge_edges(edges), None, EDGE_MIN_LENGTH);

    // edges_to_intersections
    let mut v: Vec<Edge> = edges.iter().filter(|e| e.orientation == Orient::V).copied().collect();
    let mut h: Vec<Edge> = edges.iter().filter(|e| e.orientation == Orient::H).copied().collect();
    let fcmp = |a: (f64, f64), b: (f64, f64)| a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
    v.sort_by(|a, b| fcmp((a.x0, a.top), (b.x0, b.top)));
    h.sort_by(|a, b| fcmp((a.top, a.x0), (b.top, b.x0)));
    let mut order: Vec<Point> = Vec::new();
    let mut inter: HashMap<Point, (Vec<Edge>, Vec<Edge>)> = HashMap::new();
    for ve in &v {
        for he in &h {
            if ve.top <= he.top + INTERSECTION_TOL
                && ve.bottom >= he.top - INTERSECTION_TOL
                && ve.x0 >= he.x0 - INTERSECTION_TOL
                && ve.x0 <= he.x1 + INTERSECTION_TOL
            {
                let p = pt(ve.x0, he.top);
                let entry = inter.entry(p).or_insert_with(|| {
                    order.push(p);
                    (Vec::new(), Vec::new())
                });
                entry.0.push(*ve);
                entry.1.push(*he);
            }
        }
    }
    let _ = order;

    // intersections_to_cells
    let mut points: Vec<Point> = inter.keys().copied().collect();
    points.sort_by(|a, b| fcmp(val(*a), val(*b)));
    let edge_connects = |p1: Point, p2: Point| -> bool {
        let set = |es: &Vec<Edge>| -> HashSet<(u64, u64, u64, u64)> { es.iter().map(Edge::bbox_key).collect() };
        let (a, b) = (&inter[&p1], &inter[&p2]);
        if val(p1).0 == val(p2).0 && set(&a.0).intersection(&set(&b.0)).next().is_some() {
            return true;
        }
        if val(p1).1 == val(p2).1 && set(&a.1).intersection(&set(&b.1)).next().is_some() {
            return true;
        }
        false
    };
    let n = points.len();
    let mut cells: Vec<Cell> = Vec::new();
    for i in 0..n {
        if i == n - 1 {
            continue;
        }
        let p = points[i];
        let (px, py) = val(p);
        let rest = &points[i + 1..];
        let below: Vec<Point> = rest.iter().filter(|q| val(**q).0 == px).copied().collect();
        let right: Vec<Point> = rest.iter().filter(|q| val(**q).1 == py).copied().collect();
        let mut found = None;
        'outer: for &b in &below {
            if !edge_connects(p, b) {
                continue;
            }
            for &r in &right {
                if !edge_connects(p, r) {
                    continue;
                }
                let br = pt(val(r).0, val(b).1);
                if inter.contains_key(&br) && edge_connects(br, r) && edge_connects(br, b) {
                    found = Some((px, py, val(br).0, val(br).1));
                    break 'outer;
                }
            }
        }
        if let Some(c) = found {
            cells.push(c);
        }
    }

    // cells_to_tables
    let corners = |c: &Cell| [pt(c.0, c.1), pt(c.0, c.3), pt(c.2, c.1), pt(c.2, c.3)];
    let mut remaining: Vec<Cell> = cells;
    let mut current_corners: HashSet<Point> = HashSet::new();
    let mut current: Vec<Cell> = Vec::new();
    let mut tables: Vec<Vec<Cell>> = Vec::new();
    while !remaining.is_empty() {
        let initial = current.len();
        for cell in remaining.clone() {
            let cc = corners(&cell);
            if current.is_empty() || cc.iter().any(|c| current_corners.contains(c)) {
                current_corners.extend(cc);
                current.push(cell);
                if let Some(pos) = remaining.iter().position(|x| *x == cell) {
                    remaining.remove(pos);
                }
            }
        }
        if current.len() == initial {
            tables.push(std::mem::take(&mut current));
            current_corners.clear();
        }
    }
    if !current.is_empty() {
        tables.push(current);
    }
    let tkey = |t: &Vec<Cell>| -> (f64, f64) {
        t.iter()
            .map(|c| (c.1, c.0))
            .fold((f64::INFINITY, f64::INFINITY), |a, b| if fcmp(b, a).is_lt() { b } else { a })
    };
    tables.sort_by(|a, b| fcmp(tkey(a), tkey(b)));
    tables.into_iter().filter(|t| t.len() > 1).collect()
}

fn char_in_bbox(c: &Char, bbox: Cell) -> bool {
    let v_mid = (c.top + c.bottom) / 2.0;
    let h_mid = (c.x0 + c.x1) / 2.0;
    let (x0, top, x1, bottom) = bbox;
    h_mid >= x0 && h_mid < x1 && v_mid >= top && v_mid < bottom
}

/// `page.extract_tables()`: each table as rows of cell texts (`None` for missing cells).
pub fn extract_tables(paths: &[PathObj], chars: &[Char]) -> Vec<Vec<Vec<Option<String>>>> {
    let fcmp = |a: (f64, f64), b: (f64, f64)| a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
    let mut out = Vec::new();
    for cells in find_tables(paths) {
        // Table.rows: sort by (top, x0), group by top, one slot per distinct x0.
        let mut sorted = cells.clone();
        sorted.sort_by(|a, b| fcmp((a.1, a.0), (b.1, b.0)));
        let mut xs: Vec<f64> = cells.iter().map(|c| c.0).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        xs.dedup();
        let mut rows: Vec<Vec<Option<Cell>>> = Vec::new();
        let mut i = 0;
        while i < sorted.len() {
            let top = sorted[i].1;
            let mut j = i;
            let mut xdict: HashMap<u64, Cell> = HashMap::new();
            while j < sorted.len() && sorted[j].1 == top {
                xdict.insert(sorted[j].0.to_bits(), sorted[j]);
                j += 1;
            }
            rows.push(xs.iter().map(|x| xdict.get(&x.to_bits()).copied()).collect());
            i = j;
        }
        let mut table = Vec::new();
        for row in rows {
            let present: Vec<Cell> = row.iter().flatten().copied().collect();
            let bbox = (
                present.iter().map(|c| c.0).fold(f64::INFINITY, f64::min),
                present.iter().map(|c| c.1).fold(f64::INFINITY, f64::min),
                present.iter().map(|c| c.2).fold(f64::NEG_INFINITY, f64::max),
                present.iter().map(|c| c.3).fold(f64::NEG_INFINITY, f64::max),
            );
            let row_chars: Vec<&Char> = chars.iter().filter(|c| char_in_bbox(c, bbox)).collect();
            let mut arr = Vec::new();
            for cell in row {
                match cell {
                    None => arr.push(None),
                    Some(cell) => {
                        let cell_chars: Vec<Char> = row_chars
                            .iter()
                            .filter(|c| char_in_bbox(c, cell))
                            .map(|c| (*c).clone())
                            .collect();
                        arr.push(Some(if cell_chars.is_empty() {
                            String::new()
                        } else {
                            simple_text(&cell_chars)
                        }));
                    }
                }
            }
            table.push(arr);
        }
        out.push(table);
    }
    out
}
