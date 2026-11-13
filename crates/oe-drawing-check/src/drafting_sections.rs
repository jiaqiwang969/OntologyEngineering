//! Region-scoped drafting facts. Geometry comes from native DXF, context from the frozen graph.
//! This exporter does not decide a drafting violation: the two rule groups and their exceptions
//! live in drafting-ontology.toml. It never changes the graph, source types or member intervals.
use super::*;
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Edge {
    Line(P, P),
    Arc {
        c: P,
        r: f64,
        start: f64,
        sweep: f64,
    },
}
impl Edge {
    fn at(self, t: f64) -> P {
        match self {
            Self::Line(a, b) => (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)),
            Self::Arc { c, r, start, sweep } => {
                let a = start + t * sweep;
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            }
        }
    }
    fn length(self) -> f64 {
        match self {
            Self::Line(a, b) => dist(a, b),
            Self::Arc { r, sweep, .. } => r * sweep.abs(),
        }
    }
    fn slice(self, lo: f64, hi: f64) -> Self {
        match self {
            Self::Line(_, _) => Self::Line(self.at(lo), self.at(hi)),
            Self::Arc { c, r, start, sweep } => Self::Arc {
                c,
                r,
                start: start + lo * sweep,
                sweep: (hi - lo) * sweep,
            },
        }
    }
    fn parameters(self, p: P) -> Vec<f64> {
        match self {
            Self::Line(a, b) => {
                let l2 = (b.0 - a.0).powi(2) + (b.1 - a.1).powi(2);
                if l2 <= f64::EPSILON {
                    vec![]
                } else {
                    vec![((p.0 - a.0) * (b.0 - a.0) + (p.1 - a.1) * (b.1 - a.1)) / l2]
                }
            }
            Self::Arc {
                c, start, sweep, ..
            } => {
                let a = (p.1 - c.1).atan2(p.0 - c.0);
                (-3..=3)
                    .map(|k| (a - start + f64::from(k) * TAU) / sweep)
                    .collect()
            }
        }
    }
    fn contains(self, p: P, tol: f64) -> bool {
        self.parameters(p)
            .iter()
            .any(|t| *t >= -1e-10 && *t <= 1. + 1e-10 && dist(self.at(t.clamp(0., 1.)), p) <= tol)
    }
    /// Positive-length overlap measured in this edge's parameter. An arc's chord is never an edge.
    fn overlaps(self, other: Self, tol: f64) -> Vec<[f64; 2]> {
        let compatible = match (self, other) {
            (Self::Line(a, b), Self::Line(c, d)) => {
                let len = dist(a, b);
                len > tol
                    && ((c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0)).abs() / len <= tol
                    && ((d.0 - a.0) * (b.1 - a.1) - (d.1 - a.1) * (b.0 - a.0)).abs() / len <= tol
            }
            (Self::Arc { c, r, .. }, Self::Arc { c: c2, r: r2, .. }) => {
                dist(c, c2) <= tol && (r - r2).abs() <= tol
            }
            _ => false,
        };
        if !compatible {
            return vec![];
        }
        let mut cuts = vec![0., 1.];
        for p in [other.at(0.), other.at(1.)] {
            cuts.extend(
                self.parameters(p)
                    .into_iter()
                    .filter(|t| *t > 0. && *t < 1.),
            );
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        cuts.windows(2)
            .filter(|w| {
                (w[1] - w[0]) * self.length() > tol
                    && other.contains(self.at((w[0] + w[1]) / 2.), tol)
            })
            .map(|w| [w[0], w[1]])
            .collect()
    }
    fn sample(self, sagitta: f64) -> Option<Vec<P>> {
        let count = match self {
            Self::Line(_, _) => 1,
            Self::Arc { r, sweep, .. } => {
                if !r.is_finite() || r <= 0. || !sweep.is_finite() {
                    return None;
                }
                let step = (2. * (1. - (sagitta / r).min(1.)).acos()).max(1e-6);
                (sweep.abs() / step).ceil().max(1.) as usize
            }
        };
        if count > 4096 {
            return None;
        }
        Some(
            (0..=count)
                .map(|i| self.at(i as f64 / count as f64))
                .collect(),
        )
    }
}

#[derive(Default)]
struct Face {
    edges: Vec<Edge>,
    polygon_edges: Vec<(P, P)>,
    /// Every native path edge, including seams between adjacent paths and hole boundaries.  A
    /// source line coincident with any such edge is a section boundary even when parity removes
    /// the seam from the exterior polygon.
    boundary_edges: Vec<(P, P)>,
    /// Native path order is retained for the small cut-off-area test.  The exterior parity list
    /// intentionally loses this order, while a boundary chain must be compared with a nearby
    /// path before it can be promoted from an interior use to a section boundary.
    boundary_loops: Vec<Vec<Edge>>,
    boundary_area: f64,
    complete: bool,
}

fn bulged(a: P, b: P, bulge: f64) -> Edge {
    if bulge.abs() < 1e-12 {
        return Edge::Line(a, b);
    }
    let c = (
        (a.0 + b.0) / 2. - (b.1 - a.1) * (1. - bulge * bulge) / (4. * bulge),
        (a.1 + b.1) / 2. + (b.0 - a.0) * (1. - bulge * bulge) / (4. * bulge),
    );
    Edge::Arc {
        c,
        r: dist(c, a),
        start: (a.1 - c.1).atan2(a.0 - c.0),
        sweep: 4. * bulge.atan(),
    }
}

/// Native straight/circular HATCH paths, with holes retained by even-odd containment.
/// Unsupported, open or malformed paths keep partial geometry and cannot support interior claims.
fn face(r: &dxf::Record, tol: f64, sagitta: f64) -> Face {
    let mut out = Face::default();
    if r.kind != "HATCH"
        || r.tag(75).is_some_and(|_| r.float(75).ok() != Some(0.))
        || r.tag(210).is_some_and(|_| r.float(210).ok() != Some(0.))
        || r.tag(220).is_some_and(|_| r.float(220).ok() != Some(0.))
        || r.tag(230).is_some_and(|_| r.float(230).ok() != Some(1.))
    {
        return out;
    }
    let Some(start) = r.tags.iter().position(|t| t.0 == 91) else {
        return out;
    };
    let end = r
        .tags
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, t)| t.0 == 75)
        .map_or(r.tags.len(), |(i, _)| i);
    let starts: Vec<usize> = (start + 1..end).filter(|i| r.tags[*i].0 == 92).collect();
    out.complete =
        !starts.is_empty() && r.tags[start].1.parse::<usize>().ok() == Some(starts.len());
    for (n, &i) in starts.iter().enumerate() {
        let ts = &r.tags[i + 1..starts.get(n + 1).copied().unwrap_or(end)];
        let num = |code| {
            ts.iter()
                .find(|t| t.0 == code)
                .and_then(|t| t.1.parse::<f64>().ok())
        };
        let mut edges = Vec::new();
        if r.tags[i].1.parse::<i64>().unwrap_or(0) & 2 != 0 {
            let mut vertices = Vec::new();
            for k in 0..ts.len().saturating_sub(1) {
                if ts[k].0 == 10 && ts[k + 1].0 == 20 {
                    if let (Ok(x), Ok(y)) = (ts[k].1.parse::<f64>(), ts[k + 1].1.parse::<f64>()) {
                        let b = ts
                            .get(k + 2)
                            .filter(|v| v.0 == 42)
                            .and_then(|v| v.1.parse::<f64>().ok())
                            .unwrap_or(0.);
                        vertices.push(((x, y), b));
                    } else {
                        out.complete = false;
                    }
                }
            }
            let closed = num(73) == Some(1.);
            if !closed || num(93) != Some(vertices.len() as f64) {
                out.complete = false;
            }
            for j in 0..vertices.len().saturating_sub(usize::from(!closed)) {
                if vertices.len() > 1 {
                    edges.push(bulged(
                        vertices[j].0,
                        vertices[(j + 1) % vertices.len()].0,
                        vertices[j].1,
                    ));
                }
            }
        } else {
            let indices: Vec<usize> = ts
                .iter()
                .enumerate()
                .filter(|(_, t)| t.0 == 72)
                .map(|(i, _)| i)
                .collect();
            if num(93) != Some(indices.len() as f64) {
                out.complete = false;
            }
            for (j, &k) in indices.iter().enumerate() {
                let vals = &ts[k + 1..indices.get(j + 1).copied().unwrap_or(ts.len())];
                let get = |code| {
                    vals.iter()
                        .find(|v| v.0 == code)
                        .and_then(|v| v.1.parse::<f64>().ok())
                };
                match ts[k].1.as_str() {
                    "1" => {
                        if let (Some(x), Some(y), Some(u), Some(v)) =
                            (get(10), get(20), get(11), get(21))
                        {
                            edges.push(Edge::Line((x, y), (u, v)));
                        } else {
                            out.complete = false;
                        }
                    }
                    "2" => {
                        if let (Some(x), Some(y), Some(radius), Some(a), Some(z)) =
                            (get(10), get(20), get(40), get(50), get(51))
                        {
                            let sweep = if (z - a).abs() >= 360. - 1e-9 {
                                TAU
                            } else {
                                (z - a).rem_euclid(360.).to_radians()
                            };
                            let direction = if get(73) == Some(0.) { -1. } else { 1. };
                            edges.push(Edge::Arc {
                                c: (x, y),
                                r: radius,
                                start: direction * a.to_radians(),
                                sweep: direction * sweep,
                            });
                        } else {
                            out.complete = false;
                        }
                    }
                    _ => out.complete = false,
                }
            }
        }
        if edges.is_empty()
            || edges
                .iter()
                .any(|e| !e.length().is_finite() || e.length() <= 0.)
        {
            out.complete = false;
        }
        // Edge paths may list an edge backwards; closure is topological, independent of winding.
        for e in &edges {
            for p in [e.at(0.), e.at(1.)] {
                let degree = edges
                    .iter()
                    .flat_map(|v| [v.at(0.), v.at(1.)])
                    .filter(|q| dist(p, *q) <= tol)
                    .count();
                if degree != 2 {
                    out.complete = false;
                }
            }
            if let Some(ps) = e.sample(sagitta) {
                out.polygon_edges
                    .extend(ps.windows(2).map(|w| (w[0], w[1])));
            } else {
                out.complete = false;
            }
        }
        if !edges.is_empty() {
            out.boundary_loops.push(edges.clone());
        }
        out.edges.extend(edges);
    }
    if out.edges.len() > 4096 {
        return Face::default();
    }
    // Keep all native path edges for boundary tests.  A native hatch may partition one filled face into adjacent closed paths. Their coincident
    // seams are not the face's exterior. Normal hatch filling uses parity, including hole paths.
    // Split overlaps before cancelling even coverage, so a partly shared edge keeps its exterior.
    let mut all_boundary_edges = Vec::new();
    for edge in &out.edges {
        if let Some(ps) = edge.sample(sagitta) {
            all_boundary_edges.extend(ps.windows(2).map(|w| (w[0], w[1])));
        }
    }
    out.boundary_edges = all_boundary_edges;
    out.boundary_area = out
        .boundary_loops
        .iter()
        .filter_map(|loop_edges| sampled_loop(loop_edges, sagitta))
        .map(|points| polygon_area(&points).abs())
        .sum();
    let mut exterior = Vec::new();
    for (i, &edge) in out.edges.iter().enumerate() {
        let overlaps: Vec<(usize, [f64; 2])> = out
            .edges
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .flat_map(|(j, other)| {
                edge.overlaps(*other, tol)
                    .into_iter()
                    .map(move |span| (j, span))
            })
            .collect();
        let mut cuts = vec![0., 1.];
        cuts.extend(overlaps.iter().flat_map(|(_, span)| *span));
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        for w in cuts.windows(2) {
            let t = (w[0] + w[1]) / 2.;
            let covering: Vec<usize> = overlaps
                .iter()
                .filter(|(_, span)| span[0] < t && t < span[1])
                .map(|(j, _)| *j)
                .collect();
            if covering.len().is_multiple_of(2) && covering.iter().all(|j| *j > i) {
                exterior.push(edge.slice(w[0], w[1]));
            }
        }
    }
    out.edges = exterior;
    out.polygon_edges.clear();
    for edge in &out.edges {
        if let Some(ps) = edge.sample(sagitta) {
            out.polygon_edges
                .extend(ps.windows(2).map(|w| (w[0], w[1])));
        } else {
            out.complete = false;
        }
    }
    out
}

fn inside(p: P, edges: &[(P, P)], tol: f64) -> bool {
    if edges.iter().any(|(a, b)| {
        Edge::Line(*a, *b)
            .parameters(p)
            .iter()
            .any(|t| dist(Edge::Line(*a, *b).at(t.clamp(0., 1.)), p) <= tol)
    }) {
        return false;
    }
    edges
        .iter()
        .filter(|(a, b)| {
            (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0
        })
        .count()
        % 2
        == 1
}

fn sampled_loop(edges: &[Edge], sagitta: f64) -> Option<Vec<P>> {
    let mut points = Vec::new();
    for edge in edges {
        let samples = edge.sample(sagitta)?;
        if points.is_empty() {
            points.extend(samples);
        } else {
            points.extend(samples.into_iter().skip(1));
        }
    }
    if points.len() < 3 {
        return None;
    }
    if dist(*points.first()?, *points.last()?) > 1e-7 {
        points.push(*points.first()?);
    }
    Some(points)
}

fn polygon_area(points: &[P]) -> f64 {
    points
        .windows(2)
        .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
        .sum::<f64>()
        / 2.
}

fn point_in_polygon(p: P, polygon: &[P], tol: f64) -> bool {
    if polygon
        .windows(2)
        .any(|w| point_segment(p, w[0], w[1]).0 <= tol)
    {
        return false;
    }
    polygon
        .windows(2)
        .filter(|w| {
            (w[0].1 > p.1) != (w[1].1 > p.1)
                && p.0 < (w[1].0 - w[0].0) * (p.1 - w[0].1) / (w[1].1 - w[0].1) + w[0].0
        })
        .count()
        % 2
        == 1
}

fn distance_to_chain(point: P, chain: &[P]) -> f64 {
    chain
        .windows(2)
        .map(|w| point_segment(point, w[0], w[1]).0)
        .fold(f64::INFINITY, f64::min)
}

fn interval_overrun(a: P, b: P, chain: &[P]) -> f64 {
    let midpoint = ((a.0 + b.0) / 2., (a.1 + b.1) / 2.);
    [a, midpoint, b]
        .into_iter()
        .map(|point| distance_to_chain(point, chain))
        .fold(0., f64::max)
}

fn boundary_route(face: &Face, start: P, end: P, tol: f64) -> Option<Vec<P>> {
    let mut nodes: Vec<P> = Vec::new();
    let mut graph: Vec<Vec<(usize, f64)>> = Vec::new();
    let add_node = |p: P, nodes: &mut Vec<P>, graph: &mut Vec<Vec<(usize, f64)>>| {
        if let Some(i) = nodes.iter().position(|q| dist(*q, p) <= tol) {
            i
        } else {
            let i = nodes.len();
            nodes.push(p);
            graph.push(Vec::new());
            i
        }
    };
    add_node(start, &mut nodes, &mut graph);
    add_node(end, &mut nodes, &mut graph);
    for edge in &face.boundary_edges {
        let mut pieces = vec![(0., edge.0), (1., edge.1)];
        for p in [start, end] {
            let (_, u) = point_segment(p, edge.0, edge.1);
            if point_segment(p, edge.0, edge.1).0 <= tol {
                pieces.push((u.clamp(0., 1.), p));
            }
        }
        pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
        pieces.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-10 && dist(a.1, b.1) <= tol);
        for pair in pieces.windows(2) {
            let a = add_node(pair[0].1, &mut nodes, &mut graph);
            let b = add_node(pair[1].1, &mut nodes, &mut graph);
            if a == b {
                continue;
            }
            let length = dist(pair[0].1, pair[1].1);
            graph[a].push((b, length));
            graph[b].push((a, length));
        }
    }
    // The endpoint nodes may have merged with a sampled boundary point.  Locate them again so
    // the route starts and ends at the exact visible-outline endpoints.
    let start_i = nodes.iter().position(|p| dist(*p, start) <= tol)?;
    let end_i = nodes.iter().position(|p| dist(*p, end) <= tol)?;
    let mut distances = vec![f64::INFINITY; nodes.len()];
    let mut previous = vec![None; nodes.len()];
    let mut used = vec![false; nodes.len()];
    distances[start_i] = 0.;
    for _ in 0..nodes.len() {
        let Some(current) = (0..nodes.len())
            .filter(|i| !used[*i])
            .min_by(|a, b| distances[*a].total_cmp(&distances[*b]))
        else {
            break;
        };
        if !distances[current].is_finite() {
            break;
        }
        used[current] = true;
        for &(next, weight) in &graph[current] {
            let candidate = distances[current] + weight;
            if candidate < distances[next] {
                distances[next] = candidate;
                previous[next] = Some(current);
            }
        }
    }
    if !distances[end_i].is_finite() {
        return None;
    }
    let mut route = Vec::new();
    let mut current = end_i;
    route.push(nodes[current]);
    while current != start_i {
        current = previous[current]?;
        route.push(nodes[current]);
    }
    route.reverse();
    Some(route)
}

#[derive(Clone)]
struct BoundaryCut {
    members: Vec<usize>,
    /// The closed polygon made by the candidate contour and the native boundary route.
    polygon: Vec<P>,
    /// Sampled points of the visible contour itself.  Pattern lines are tested against this
    /// chain, rather than against the polygon's native-boundary side.
    contour: Vec<P>,
}

#[derive(Clone, Copy)]
struct CutSegment {
    object: usize,
    edge: Edge,
    nodes: [usize; 2],
}

struct BoundaryPathSearch<'a> {
    target: usize,
    component: &'a BTreeSet<usize>,
    adjacency: &'a [Vec<usize>],
    segments: &'a [CutSegment],
    visited_nodes: &'a mut BTreeSet<usize>,
    used_segments: &'a mut BTreeSet<usize>,
    path: &'a mut Vec<usize>,
    paths: &'a mut Vec<Vec<usize>>,
    limit: usize,
}

impl BoundaryPathSearch<'_> {
    fn visit(&mut self, current: usize) {
        if self.paths.len() >= 128 || self.path.len() > self.limit {
            return;
        }
        if current == self.target {
            if !self.path.is_empty() {
                self.paths.push(self.path.clone());
            }
            return;
        }
        let adjacent = self.adjacency[current].clone();
        for segment in adjacent {
            if !self.component.contains(&segment) || self.used_segments.contains(&segment) {
                continue;
            }
            let data = self.segments[segment];
            let next = if data.nodes[0] == current {
                data.nodes[1]
            } else if data.nodes[1] == current {
                data.nodes[0]
            } else {
                continue;
            };
            if self.visited_nodes.contains(&next) {
                continue;
            }
            self.visited_nodes.insert(next);
            self.used_segments.insert(segment);
            self.path.push(segment);
            self.visit(next);
            self.path.pop();
            self.used_segments.remove(&segment);
            self.visited_nodes.remove(&next);
        }
    }
}

/// Classify every interior portion of an outline piece against a candidate cut polygon.  A
/// single midpoint is not enough here: a spoke or a circular hole can cross a small cut-off
/// corner while its midpoint remains on the retained side.  Split sampled pieces at polygon
/// crossings, then use interval midpoints so both sides of the same entity are visible.
fn cut_side_evidence(
    edge: Edge,
    polygon: &[P],
    face: &Face,
    tol: f64,
    sagitta: f64,
) -> (bool, bool) {
    let Some(samples) = edge.sample(sagitta) else {
        return (false, false);
    };
    let mut cut_side = false;
    let mut remaining_side = false;
    for window in samples.windows(2) {
        let segment = Edge::Line(window[0], window[1]);
        let mut cuts = vec![0., 1.];
        for boundary in polygon.windows(2) {
            if let Some(point) =
                segment_intersection(window[0], window[1], boundary[0], boundary[1])
            {
                cuts.extend(
                    segment
                        .parameters(point)
                        .into_iter()
                        .filter(|t| *t > 0. && *t < 1.),
                );
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        for interval in cuts.windows(2) {
            let midpoint = segment.at((interval[0] + interval[1]) / 2.);
            if !inside(midpoint, &face.polygon_edges, tol) {
                continue;
            }
            if point_in_polygon(midpoint, polygon, tol) {
                cut_side = true;
            } else {
                remaining_side = true;
            }
        }
    }
    (cut_side, remaining_side)
}

fn boundary_cut_chains(
    face: &Face,
    outlines: &[usize],
    curves: &[Vec<Piece>],
    interior: &[usize],
    tol: f64,
    sagitta: f64,
    cutoff_ratio: f64,
) -> Vec<BoundaryCut> {
    if !face.complete || face.boundary_area <= 0. {
        return Vec::new();
    }
    let mut nodes: Vec<P> = Vec::new();
    let add_node = |p: P, nodes: &mut Vec<P>| {
        if let Some(i) = nodes.iter().position(|q| dist(*q, p) <= tol) {
            i
        } else {
            let i = nodes.len();
            nodes.push(p);
            i
        }
    };
    let mut segments = Vec::new();
    for &object in outlines {
        for piece in &curves[object] {
            if piece.edge.length() > tol {
                let a = add_node(piece.edge.at(0.), &mut nodes);
                let b = add_node(piece.edge.at(1.), &mut nodes);
                if a != b {
                    segments.push(CutSegment {
                        object,
                        edge: piece.edge,
                        nodes: [a, b],
                    });
                }
            }
        }
    }
    let mut adjacency = vec![Vec::<usize>::new(); nodes.len()];
    for (i, segment) in segments.iter().enumerate() {
        adjacency[segment.nodes[0]].push(i);
        adjacency[segment.nodes[1]].push(i);
    }
    let mut visited = vec![false; segments.len()];
    let mut cuts: Vec<BoundaryCut> = Vec::new();
    for first in 0..segments.len() {
        if visited[first] {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![first];
        visited[first] = true;
        while let Some(segment) = stack.pop() {
            component.push(segment);
            for node in segments[segment].nodes {
                for &next in &adjacency[node] {
                    if !visited[next] {
                        visited[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        let component_set: BTreeSet<usize> = component.iter().copied().collect();
        let boundary_nodes: Vec<usize> = component
            .iter()
            .flat_map(|i| segments[*i].nodes)
            .filter(|i| point_on_boundary(nodes[*i], face, tol))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if boundary_nodes.len() < 2 {
            continue;
        }
        for (left, &start_node) in boundary_nodes.iter().enumerate() {
            for &end_node in &boundary_nodes[left + 1..] {
                let mut paths = Vec::new();
                let mut visited_nodes = BTreeSet::from([start_node]);
                let mut used_segments = BTreeSet::new();
                let mut path = Vec::new();
                let mut search = BoundaryPathSearch {
                    target: end_node,
                    component: &component_set,
                    adjacency: &adjacency,
                    segments: &segments,
                    visited_nodes: &mut visited_nodes,
                    used_segments: &mut used_segments,
                    path: &mut path,
                    paths: &mut paths,
                    limit: 4,
                };
                search.visit(start_node);
                for path in paths {
                    let mut current = start_node;
                    let mut ordered = Vec::new();
                    let mut member_set = BTreeSet::new();
                    for &segment in &path {
                        let data = segments[segment];
                        let forward = data.nodes[0] == current;
                        ordered.push(if forward {
                            data.edge
                        } else {
                            data.edge.slice(1., 0.)
                        });
                        current = if forward {
                            data.nodes[1]
                        } else {
                            data.nodes[0]
                        };
                        member_set.insert(data.object);
                    }
                    if current != end_node {
                        continue;
                    }
                    let mut chain_points = Vec::new();
                    let mut interior_length_sum = 0.;
                    let mut valid = true;
                    for edge in &ordered {
                        let Some(samples) = edge.sample(sagitta) else {
                            valid = false;
                            break;
                        };
                        if chain_points.is_empty() {
                            chain_points.extend(samples.iter().copied());
                        } else {
                            chain_points.extend(samples.iter().copied().skip(1));
                        }
                        interior_length_sum += samples
                            .windows(2)
                            .map(|w| interior_length(w[0], w[1], face, tol))
                            .sum::<f64>();
                    }
                    if !valid || chain_points.len() < 2 || interior_length_sum <= tol {
                        continue;
                    }
                    let start = *chain_points.first().unwrap();
                    let end = *chain_points.last().unwrap();
                    let Some(route) = boundary_route(face, start, end, tol) else {
                        continue;
                    };
                    let mut polygon = chain_points.clone();
                    polygon.extend(route.into_iter().rev().skip(1));
                    if polygon.len() < 3 {
                        continue;
                    }
                    let area_ratio = polygon_area(&polygon).abs() / face.boundary_area;
                    if !area_ratio.is_finite() || area_ratio <= 0. || area_ratio > cutoff_ratio {
                        continue;
                    }
                    let mut cut_side = false;
                    let mut remaining_side = false;
                    for &object in interior {
                        if member_set.contains(&object) {
                            continue;
                        }
                        for piece in &curves[object] {
                            let (piece_cut_side, piece_remaining_side) =
                                cut_side_evidence(piece.edge, &polygon, face, tol, sagitta);
                            cut_side |= piece_cut_side;
                            remaining_side |= piece_remaining_side;
                        }
                    }
                    if cut_side && remaining_side {
                        continue;
                    }
                    let members: Vec<usize> = member_set.into_iter().collect();
                    if cuts.iter().any(|cut| cut.members == members) {
                        continue;
                    }
                    cuts.push(BoundaryCut {
                        members,
                        polygon,
                        contour: chain_points,
                    });
                }
            }
        }
    }
    cuts
}

/// Return a finite hatch-pattern segment for every pattern row that crosses the native face.
/// DXF stores the row origin, direction and row delta on the HATCH itself; no rendered hatch
/// lines are synthesized when the source has no pattern-line definition.
fn native_pattern_segments(
    record: &dxf::Record,
    face: &Face,
    tol: f64,
    _sagitta: f64,
) -> Vec<Edge> {
    let Some(count) = record.tag(78).and_then(|v| v.parse::<usize>().ok()) else {
        return Vec::new();
    };
    if count == 0 || face.polygon_edges.len() < 3 {
        return Vec::new();
    }
    let Some((min_x, max_x, min_y, max_y)) =
        face.polygon_edges
            .iter()
            .fold(None, |acc: Option<(f64, f64, f64, f64)>, (a, b)| {
                let next = [*a, *b];
                let (lo_x, hi_x, lo_y, hi_y) = next.iter().fold(
                    (
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                    ),
                    |(lx, hx, ly, hy), p| (lx.min(p.0), hx.max(p.0), ly.min(p.1), hy.max(p.1)),
                );
                Some(match acc {
                    Some((lx, hx, ly, hy)) => {
                        (lx.min(lo_x), hx.max(hi_x), ly.min(lo_y), hy.max(hi_y))
                    }
                    None => (lo_x, hi_x, lo_y, hi_y),
                })
            })
    else {
        return Vec::new();
    };
    let diagonal = ((max_x - min_x).powi(2) + (max_y - min_y).powi(2)).sqrt();
    if !diagonal.is_finite() || diagonal <= tol {
        return Vec::new();
    }
    let mut definitions = Vec::new();
    let starts: Vec<usize> = record
        .tags
        .iter()
        .enumerate()
        .filter_map(|(i, (code, _))| (*code == 53).then_some(i))
        .take(count)
        .collect();
    for (n, start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(record.tags.len());
        let slice = &record.tags[*start..end];
        let get = |code| {
            slice
                .iter()
                .find(|(c, _)| *c == code)
                .and_then(|(_, v)| v.parse::<f64>().ok())
        };
        let Some(angle) = get(53).or_else(|| record.float(52).ok()) else {
            continue;
        };
        let (base_x, base_y) = (get(43).unwrap_or(0.), get(44).unwrap_or(0.));
        let (delta_x, delta_y) = (get(45).unwrap_or(0.), get(46).unwrap_or(0.));
        let t = angle.to_radians();
        let direction = (t.cos(), t.sin());
        let normal = (-direction.1, direction.0);
        let step = (delta_x * normal.0 + delta_y * normal.1).abs();
        let step = if step > tol {
            step
        } else {
            (delta_x * delta_x + delta_y * delta_y).sqrt()
        };
        if !step.is_finite() || step <= tol {
            continue;
        }
        let base_offset = base_x * normal.0 + base_y * normal.1;
        let projections = [
            (min_x, min_y),
            (min_x, max_y),
            (max_x, min_y),
            (max_x, max_y),
        ]
        .map(|p| p.0 * normal.0 + p.1 * normal.1);
        let lo = projections.into_iter().fold(f64::INFINITY, f64::min);
        let hi = projections.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let k0 = ((lo - base_offset) / step).floor() as i64 - 2;
        let k1 = ((hi - base_offset) / step).ceil() as i64 + 2;
        definitions.push((base_offset, direction, normal, step, k0, k1));
    }
    let mut out = Vec::new();
    let center = ((min_x + max_x) / 2., (min_y + max_y) / 2.);
    for (base_offset, direction, normal, step, k0, k1) in definitions {
        let center_projection = center.0 * normal.0 + center.1 * normal.1;
        let projections = [
            (min_x, min_y),
            (min_x, max_y),
            (max_x, min_y),
            (max_x, max_y),
        ]
        .map(|p| p.0 * normal.0 + p.1 * normal.1);
        let min_projection = projections.into_iter().fold(f64::INFINITY, f64::min);
        let max_projection = projections.into_iter().fold(f64::NEG_INFINITY, f64::max);
        for k in k0..=k1 {
            let projection = base_offset + k as f64 * step;
            if projection < min_projection - step || projection > max_projection + step {
                continue;
            }
            // The HATCH base point and row delta determine a line's normal offset; its
            // position along the line is arbitrary.  Re-anchor it at the face centre so the
            // finite segment spans the complete face even when the DXF base is at (0,0).
            let offset = projection - center_projection;
            let origin = (center.0 + normal.0 * offset, center.1 + normal.1 * offset);
            let half = diagonal * 2. + step;
            let a = (origin.0 - direction.0 * half, origin.1 - direction.1 * half);
            let b = (origin.0 + direction.0 * half, origin.1 + direction.1 * half);
            let mut cuts = vec![0., 1.];
            let line = Edge::Line(a, b);
            for (p, q) in &face.polygon_edges {
                if let Some(point) = segment_intersection(a, b, *p, *q) {
                    cuts.extend(
                        line.parameters(point)
                            .into_iter()
                            .filter(|u| *u > 0. && *u < 1.),
                    );
                }
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup_by(|u, v| (*u - *v).abs() < 1e-10);
            for w in cuts.windows(2) {
                if (w[1] - w[0]) * line.length() <= tol {
                    continue;
                }
                let midpoint = line.at((w[0] + w[1]) / 2.);
                if inside(midpoint, &face.polygon_edges, tol) {
                    out.push(line.slice(w[0], w[1]));
                }
            }
        }
    }
    out
}

/// Maximum visible continuation of a hatch segment inside the polygon cut off by a contour.
/// A crossing at a contour endpoint or vertex is a touch, not a visible overrun.
fn overrun_inside_cut(edge: Edge, contour: &[P], polygon: &[P], tol: f64, sagitta: f64) -> f64 {
    let Some(samples) = edge.sample(sagitta) else {
        return 0.;
    };
    let mut best: f64 = 0.;
    for window in samples.windows(2) {
        let segment = Edge::Line(window[0], window[1]);
        let mut cuts = vec![0., 1.];
        let mut crossings = Vec::new();
        for boundary in contour.windows(2) {
            if let Some(point) =
                segment_intersection(window[0], window[1], boundary[0], boundary[1])
                && let Some(t) = segment
                    .parameters(point)
                    .into_iter()
                    .find(|u| *u >= -1e-10 && *u <= 1. + 1e-10)
                && t > 1e-9
                && t < 1. - 1e-9
                && contour.iter().all(|q| dist(*q, point) > tol)
            {
                cuts.push(t);
                crossings.push(t);
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|u, v| (*u - *v).abs() < 1e-10);
        for w in cuts.windows(2) {
            let midpoint = segment.at((w[0] + w[1]) / 2.);
            if !point_in_polygon(midpoint, polygon, tol) {
                continue;
            }
            let touches_crossing = crossings
                .iter()
                .any(|t| (*t - w[0]).abs() < 1e-8 || (*t - w[1]).abs() < 1e-8);
            if touches_crossing {
                best = best.max(interval_overrun(
                    segment.at(w[0]),
                    segment.at(w[1]),
                    contour,
                ));
            }
        }
    }
    best
}

/// Maximum visible continuation outside the native hatch face for a loose HatchLine relation.
fn overrun_outside_face(
    hatch: Edge,
    outline: Edge,
    face: &Face,
    tol: f64,
    sagitta: f64,
) -> f64 {
    let Some(hatch_samples) = hatch.sample(sagitta) else {
        return 0.;
    };
    let Some(outline_samples) = outline.sample(sagitta) else {
        return 0.;
    };
    let mut best: f64 = 0.;
    for window in hatch_samples.windows(2) {
        let segment = Edge::Line(window[0], window[1]);
        let mut cuts = vec![0., 1.];
        let mut crossings = Vec::new();
        for boundary in outline_samples.windows(2) {
            if let Some(point) =
                segment_intersection(window[0], window[1], boundary[0], boundary[1])
                && let Some(t) = segment
                    .parameters(point)
                    .into_iter()
                    .find(|u| *u >= -1e-10 && *u <= 1. + 1e-10)
                && t > 1e-9
                && t < 1. - 1e-9
                && [
                    *outline_samples.first().unwrap(),
                    *outline_samples.last().unwrap(),
                ]
                .iter()
                .all(|q| dist(*q, point) > tol)
            {
                cuts.push(t);
                crossings.push(t);
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|u, v| (*u - *v).abs() < 1e-10);
        for w in cuts.windows(2) {
            let midpoint = segment.at((w[0] + w[1]) / 2.);
            if inside(midpoint, &face.polygon_edges, tol) {
                continue;
            }
            if crossings
                .iter()
                .any(|t| (*t - w[0]).abs() < 1e-8 || (*t - w[1]).abs() < 1e-8)
            {
                best = best.max(interval_overrun(
                    segment.at(w[0]),
                    segment.at(w[1]),
                    &outline_samples,
                ));
            }
        }
    }
    best
}

/// For a loose hatch region with no native face polygon, a proper interior crossing still
/// proves visible continuation.  Endpoint contacts and tangent touches are excluded by
/// requiring the intersection to be interior to both sampled chains.
fn overrun_without_face(hatch: Edge, outline: Edge, sagitta: f64) -> f64 {
    let Some(hatch_samples) = hatch.sample(sagitta) else {
        return 0.;
    };
    let Some(outline_samples) = outline.sample(sagitta) else {
        return 0.;
    };
    let mut best: f64 = 0.;
    for hatch_window in hatch_samples.windows(2) {
        let segment = Edge::Line(hatch_window[0], hatch_window[1]);
        for outline_window in outline_samples.windows(2) {
            let Some(point) = segment_intersection(
                hatch_window[0],
                hatch_window[1],
                outline_window[0],
                outline_window[1],
            ) else {
                continue;
            };
            if !segment
                .parameters(point)
                .into_iter()
                .any(|t| t > 1e-9 && t < 1. - 1e-9)
            {
                continue;
            }
            let outline_segment = Edge::Line(outline_window[0], outline_window[1]);
            if !outline_segment
                .parameters(point)
                .into_iter()
                .any(|t| t > 1e-9 && t < 1. - 1e-9)
            {
                continue;
            }
            let left = segment.at(0.);
            let right = segment.at(1.);
            best = best.max(interval_overrun(left, point, &outline_samples));
            best = best.max(interval_overrun(point, right, &outline_samples));
        }
    }
    best
}

fn raw_records<'a>(
    object: &Object,
    d: &'a Dxf<'a>,
    members: &BTreeMap<&str, &Value>,
) -> Vec<&'a dxf::Record> {
    let mut handles = BTreeSet::new();
    handles.extend(object.source_handles.iter().map(String::as_str));
    for member_id in &object.member_ids {
        if let Some(native) = members
            .get(member_id.as_str())
            .and_then(|m| m.get("native_member_id"))
            .and_then(Value::as_str)
        {
            handles.insert(native.rsplit('/').next().unwrap_or(native));
        }
    }
    handles
        .into_iter()
        .filter_map(|handle| d.by_handle.get(handle).copied())
        .collect()
}

fn record_lineweight(record: &dxf::Record, d: &Dxf<'_>) -> Option<f64> {
    record
        .tag(370)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| *v > 0.)
        .map(|v| v / 100.)
        .or_else(|| {
            record
                .tag(43)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| *v > 0.)
        })
        .or_else(|| {
            record
                .tag(8)
                .and_then(|layer| d.layer_lineweight.get(layer).copied())
        })
}

fn contour_lineweight(
    object: &Object,
    d: &Dxf<'_>,
    members: &BTreeMap<&str, &Value>,
) -> Option<f64> {
    raw_records(object, d, members)
        .into_iter()
        .find_map(|record| record_lineweight(record, d))
}

fn record_points(record: &dxf::Record) -> Vec<P> {
    let mut points = Vec::new();
    for (index, (code, value)) in record.tags.iter().enumerate() {
        if !matches!(*code, 10..=13) {
            continue;
        }
        let y_code = *code + 10;
        let Some((_, y)) = record.tags[index + 1..].iter().find(|(c, _)| *c == y_code) else {
            continue;
        };
        let (Ok(x), Ok(y)) = (value.parse::<f64>(), y.parse::<f64>()) else {
            continue;
        };
        points.push((x, y));
    }
    points
}

fn arrow_length(objects: &[Object], d: &Dxf<'_>, members: &BTreeMap<&str, &Value>) -> Option<f64> {
    let mut longest: f64 = 0.;
    for object in objects
        .iter()
        .filter(|o| o.is("Arrowhead") && !o.inferred_geometry)
    {
        for chain in &object.geometry.chains {
            for window in chain.windows(2) {
                longest = longest.max(dist(window[0], window[1]));
            }
        }
        for record in raw_records(object, d, members) {
            let points = record_points(record);
            for (index, a) in points.iter().enumerate() {
                for b in points.iter().skip(index + 1) {
                    longest = longest.max(dist(*a, *b));
                }
            }
        }
    }
    (longest > 0. && longest.is_finite()).then_some(longest)
}

/// Determine the visible-overrun threshold from the contour being crossed.  A hatch line is a
/// thin line and its own width/spacing is deliberately never used as the contour width.  When
/// the contour has no explicit DXF width, the drawing's text height or arrow length supplies the
/// scale witness named by the knowledge card; if neither exists, the rule remains unclassified.
struct OverrunScale {
    text_height: Option<f64>,
    arrow_length: Option<f64>,
    text_ratio: f64,
    arrow_strokes: f64,
    visible_ratio: f64,
}

#[derive(Clone, Copy, Debug)]
struct OverrunSpan {
    hatch: [f64; 2],
    outline: [f64; 2],
    entry: P,
    exit: P,
    /// The same visible-continuation measure used by the existing threshold decision.
    exposed: f64,
    /// Physical length of the hatch interval, retained for the exported fact.
    length: f64,
}

fn piece_interval(piece: &Piece, local: [f64; 2]) -> [f64; 2] {
    [
        piece.lo + (piece.hi - piece.lo) * local[0],
        piece.lo + (piece.hi - piece.lo) * local[1],
    ]
}

fn edge_parameter(edge: Edge, point: P) -> f64 {
    edge.parameters(point)
        .into_iter()
        .filter(|t| *t >= -1e-8 && *t <= 1. + 1e-8)
        .min_by(f64::total_cmp)
        .unwrap_or(0.)
        .clamp(0., 1.)
}

fn crossings(hatch: Edge, outline: Edge, sagitta: f64) -> Vec<(f64, P, f64)> {
    let (Some(hatch_samples), Some(outline_samples)) =
        (hatch.sample(sagitta), outline.sample(sagitta))
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for hw in hatch_samples.windows(2) {
        let hseg = Edge::Line(hw[0], hw[1]);
        for ow in outline_samples.windows(2) {
            let oseg = Edge::Line(ow[0], ow[1]);
            let Some(point) = segment_intersection(hw[0], hw[1], ow[0], ow[1]) else {
                continue;
            };
            let Some(ht) = hseg
                .parameters(point)
                .into_iter()
                .find(|t| *t > 1e-9 && *t < 1. - 1e-9)
            else {
                continue;
            };
            let Some(ot) = oseg
                .parameters(point)
                .into_iter()
                .find(|t| *t > 1e-9 && *t < 1. - 1e-9)
            else {
                continue;
            };
            out.push((ht, point, ot));
        }
    }
    out
}

fn overrun_span_for_interval(
    hatch: &Piece,
    outline: &Piece,
    hatch_local: [f64; 2],
    outline_point: P,
    exposed: f64,
) -> OverrunSpan {
    let hatch_range = piece_interval(hatch, hatch_local);
    let outline_t = edge_parameter(outline.edge, outline_point);
    let outline_range = [
        outline.lo + (outline.hi - outline.lo) * outline_t,
        outline.lo + (outline.hi - outline.lo) * outline_t,
    ];
    OverrunSpan {
        hatch: hatch_range,
        outline: outline_range,
        entry: hatch.edge.at(hatch_local[0]),
        exit: hatch.edge.at(hatch_local[1]),
        exposed,
        length: (hatch_local[1] - hatch_local[0]).abs() * hatch.edge.length(),
    }
}

fn overrun_outside_face_spans(
    hatch: &Piece,
    outline: &Piece,
    face: &Face,
    tol: f64,
    sagitta: f64,
) -> Vec<OverrunSpan> {
    let Some(hatch_samples) = hatch.edge.sample(sagitta) else {
        return Vec::new();
    };
    let Some(outline_samples) = outline.edge.sample(sagitta) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let n = hatch_samples.len().saturating_sub(1).max(1) as f64;
    for (index, window) in hatch_samples.windows(2).enumerate() {
        let segment = Edge::Line(window[0], window[1]);
        let mut cuts = vec![0., 1.];
        let mut crossed = Vec::new();
        for (t, point, _) in crossings(segment, outline.edge, sagitta) {
            if [
                *outline_samples.first().unwrap_or(&point),
                *outline_samples.last().unwrap_or(&point),
            ]
            .iter()
            .all(|q| dist(*q, point) > tol)
            {
                cuts.push(t);
                crossed.push((t, point));
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        for w in cuts.windows(2) {
            let midpoint = segment.at((w[0] + w[1]) / 2.);
            if inside(midpoint, &face.polygon_edges, tol)
                || !crossed
                    .iter()
                    .any(|(t, _)| (*t - w[0]).abs() < 1e-8 || (*t - w[1]).abs() < 1e-8)
            {
                continue;
            }
            let exposed = interval_overrun(segment.at(w[0]), segment.at(w[1]), &outline_samples);
            if exposed <= 0. {
                continue;
            }
            let base = [index as f64 / n, (index as f64 + 1.) / n];
            let local = [
                base[0] + (base[1] - base[0]) * w[0],
                base[0] + (base[1] - base[0]) * w[1],
            ];
            let point = crossed
                .iter()
                .find(|(t, _)| (*t - w[0]).abs() < 1e-8 || (*t - w[1]).abs() < 1e-8)
                .map(|(_, p)| *p)
                .unwrap_or_else(|| segment.at(w[0]));
            out.push(overrun_span_for_interval(
                hatch, outline, local, point, exposed,
            ));
        }
    }
    out
}

fn overrun_without_face_spans(hatch: &Piece, outline: &Piece, sagitta: f64) -> Vec<OverrunSpan> {
    let Some(hatch_samples) = hatch.edge.sample(sagitta) else {
        return Vec::new();
    };
    let Some(outline_samples) = outline.edge.sample(sagitta) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let n = hatch_samples.len().saturating_sub(1).max(1) as f64;
    for (index, window) in hatch_samples.windows(2).enumerate() {
        let segment = Edge::Line(window[0], window[1]);
        for (t, point, _) in crossings(segment, outline.edge, sagitta) {
            let base = [index as f64 / n, (index as f64 + 1.) / n];
            let local_cross = base[0] + (base[1] - base[0]) * t;
            for local in [[base[0], local_cross], [local_cross, base[1]]].into_iter() {
                if local[1] - local[0] <= 1e-9 {
                    continue;
                }
                let exposed = interval_overrun(
                    hatch.edge.at(local[0]),
                    hatch.edge.at(local[1]),
                    &outline_samples,
                );
                if exposed > 0. {
                    out.push(overrun_span_for_interval(
                        hatch, outline, local, point, exposed,
                    ));
                }
            }
        }
    }
    out
}

fn overrun_inside_cut_spans(
    hatch: &Piece,
    outline: &Piece,
    contour: &[P],
    polygon: &[P],
    tol: f64,
    sagitta: f64,
) -> Vec<OverrunSpan> {
    let Some(hatch_samples) = hatch.edge.sample(sagitta) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let n = hatch_samples.len().saturating_sub(1).max(1) as f64;
    for (index, window) in hatch_samples.windows(2).enumerate() {
        let segment = Edge::Line(window[0], window[1]);
        let mut cuts = vec![0., 1.];
        let mut crossed = Vec::new();
        for boundary in contour.windows(2) {
            let Some(point) = segment_intersection(window[0], window[1], boundary[0], boundary[1])
            else {
                continue;
            };
            let Some(t) = segment
                .parameters(point)
                .into_iter()
                .find(|u| *u > 1e-9 && *u < 1. - 1e-9)
            else {
                continue;
            };
            if contour.iter().all(|q| dist(*q, point) > tol) {
                cuts.push(t);
                crossed.push((t, point));
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        for w in cuts.windows(2) {
            let midpoint = segment.at((w[0] + w[1]) / 2.);
            if !point_in_polygon(midpoint, polygon, tol)
                || !crossed
                    .iter()
                    .any(|(t, _)| (*t - w[0]).abs() < 1e-8 || (*t - w[1]).abs() < 1e-8)
            {
                continue;
            }
            let exposed = interval_overrun(segment.at(w[0]), segment.at(w[1]), contour);
            if exposed <= 0. {
                continue;
            }
            let base = [index as f64 / n, (index as f64 + 1.) / n];
            let local = [
                base[0] + (base[1] - base[0]) * w[0],
                base[0] + (base[1] - base[0]) * w[1],
            ];
            let point = crossed
                .iter()
                .find(|(t, _)| (*t - w[0]).abs() < 1e-8 || (*t - w[1]).abs() < 1e-8)
                .map(|(_, p)| *p)
                .unwrap_or_else(|| segment.at(w[0]));
            out.push(overrun_span_for_interval(
                hatch, outline, local, point, exposed,
            ));
        }
    }
    out
}

fn visible_overrun_threshold(
    contour: &Object,
    d: &Dxf<'_>,
    members: &BTreeMap<&str, &Value>,
    scale: &OverrunScale,
) -> Option<f64> {
    let thick = contour_lineweight(contour, d, members)
        .or_else(|| scale.text_height.map(|height| height * scale.text_ratio))
        .or_else(|| {
            scale
                .arrow_length
                .map(|length| length / scale.arrow_strokes)
        })?;
    Some((thick * scale.visible_ratio).max(1e-9))
}

fn same_source_interval(
    objects: &[Object],
    curves: &[Vec<Piece>],
    a: usize,
    b: usize,
    tol: f64,
) -> bool {
    if objects[a].source_handles.is_empty()
        || !objects[a]
            .source_handles
            .iter()
            .any(|h| objects[b].source_handles.contains(h))
    {
        return false;
    }
    let length_a: f64 = curves[a].iter().map(|piece| piece.edge.length()).sum();
    let length_b: f64 = curves[b].iter().map(|piece| piece.edge.length()).sum();
    if length_a <= tol || length_b <= tol {
        return false;
    }
    let overlap: f64 = curves[a]
        .iter()
        .flat_map(|left| {
            curves[b]
                .iter()
                .flat_map(move |right| left.edge.overlaps(right.edge, tol))
                .map(move |span| (span[1] - span[0]) * left.edge.length())
        })
        .sum();
    overlap + tol >= length_a && overlap + tol >= length_b
}

fn point_on_boundary(point: P, face: &Face, tol: f64) -> bool {
    face.boundary_edges
        .iter()
        .any(|(a, b)| point_segment(point, *a, *b).0 <= tol)
}

fn interior_length(a: P, b: P, face: &Face, tol: f64) -> f64 {
    let e = Edge::Line(a, b);
    let mut cuts = vec![0., 1.];
    for (p, q) in &face.polygon_edges {
        if let Some(x) = segment_intersection(a, b, *p, *q) {
            cuts.extend(e.parameters(x).into_iter().filter(|t| *t > 0. && *t < 1.));
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
    cuts.windows(2)
        .filter(|w| inside(e.at((w[0] + w[1]) / 2.), &face.polygon_edges, tol))
        .map(|w| (w[1] - w[0]) * e.length())
        .sum()
}

#[derive(Clone, Copy, PartialEq)]
struct Piece {
    edge: Edge,
    lo: f64,
    hi: f64,
    parameter: &'static str,
}
fn pieces(o: &Object, d: &Dxf<'_>, layer: &str, members: &BTreeMap<&str, &Value>) -> Vec<Piece> {
    let raw = o
        .source_handles
        .first()
        .and_then(|h| d.by_handle.get(h.as_str()).copied());
    if raw.is_some_and(|r| matches!(r.kind.as_str(), "SPLINE" | "ELLIPSE")) {
        return vec![];
    }
    let direct = o.member_ids.len() == 1 && !o.id.contains(":line:") && !o.id.contains(":implied:");
    let mut edges = Vec::new();
    let mut parameter = "polyline_length";
    if direct
        && let Some(r) = raw
        && matches!(r.kind.as_str(), "ARC" | "CIRCLE")
    {
        if let (Ok(x), Ok(y), Ok(radius)) = (r.float(10), r.float(20), r.float(40)) {
            let (a, z) = if r.kind == "CIRCLE" {
                (0., 360.)
            } else {
                (r.float(50).unwrap_or(0.), r.float(51).unwrap_or(0.))
            };
            let sweep = if (z - a).abs() >= 360. - 1e-9 {
                TAU
            } else {
                (z - a).rem_euclid(360.).to_radians()
            };
            edges.push(Edge::Arc {
                c: (x, y),
                r: radius,
                start: a.to_radians(),
                sweep,
            });
            parameter = "source_arc";
        }
    } else if o.geometry.arcs.is_empty() {
        // The existing exporter already clips LINE on recognition; take original bytes for a single root LINE.
        if direct
            && let Some(r) = raw
            && r.kind == "LINE"
        {
            if let (Ok(x), Ok(y), Ok(u), Ok(v)) =
                (r.float(10), r.float(20), r.float(11), r.float(21))
            {
                edges.push(Edge::Line((x, y), (u, v)));
                parameter = "source_line";
            }
        } else {
            edges.extend(
                o.geometry
                    .chains
                    .iter()
                    .flat_map(|c| c.windows(2).map(|w| Edge::Line(w[0], w[1]))),
            );
        }
    }
    let total: f64 = edges.iter().map(|e| e.length()).sum();
    if !total.is_finite() || total <= 0. {
        return vec![];
    }
    let range = if direct && layer != "source" {
        o.member_ids
            .first()
            .and_then(|id| members.get(id.as_str()))
            .and_then(|m| m.get("interval"))
            .and_then(Value::as_array)
            .and_then(|a| Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?]))
            .unwrap_or([0., 1.])
    } else {
        [0., 1.]
    };
    if !range.iter().all(|v| v.is_finite())
        || range[0] < 0.
        || range[1] > 1.
        || range[0] >= range[1]
    {
        return vec![];
    }
    let mut offset = 0.;
    let mut out = Vec::new();
    for edge in edges {
        let next = offset + edge.length() / total;
        let lo = offset.max(range[0]);
        let hi = next.min(range[1]);
        if hi > lo {
            out.push(Piece {
                edge: edge.slice(
                    (lo - offset) / (next - offset),
                    (hi - offset) / (next - offset),
                ),
                lo,
                hi,
                parameter,
            });
        }
        offset = next;
    }
    out
}

/// The presentation exporter merges source members into one structural line.  When a local
/// section interrupts that line, the `implied:<n>` piece is a reader-inferred span rather than
/// an original contour.  Return the inferred spans in the structural line's normalized
/// parameter so G2-019 can retain the native pieces on either side.
fn structural_implied_intervals(g: &Value, o: &Object, tol: f64) -> Option<Vec<[f64; 2]>> {
    let index =
        o.id.strip_prefix("presentation:line:")
            .and_then(|s| s.parse::<usize>().ok())?;
    let line = g.get("structural_lines")?.as_array()?.get(index)?;
    let pieces = line.get("pieces")?.as_array()?;
    let implied: Vec<usize> = pieces
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|piece| piece.strip_prefix("implied:"))
        .filter_map(|n| n.parse::<usize>().ok())
        .collect();
    if implied.is_empty() {
        return None;
    }
    let points = line.get("points")?.as_array()?;
    if points.len() != 4 {
        // Arc structural lines cannot be partitioned by a straight parameter here.  Keeping
        // the whole inferred object out is safer than treating an unknown interval as native.
        return None;
    }
    let a = (points[0].as_f64()?, points[1].as_f64()?);
    let b = (points[2].as_f64()?, points[3].as_f64()?);
    if dist(a, b) <= 1e-9 {
        return None;
    }
    let spans = g.get("implied_spans")?.as_array()?;
    let mut out = Vec::new();
    for span_index in implied {
        let span = spans.get(span_index)?;
        let span_points = span.get("points")?.as_array()?;
        if span_points.len() != 4 || span.get("arc").is_some_and(|arc| !arc.is_null()) {
            return None;
        }
        let p = (span_points[0].as_f64()?, span_points[1].as_f64()?);
        let q = (span_points[2].as_f64()?, span_points[3].as_f64()?);
        let (off_p, t_p) = point_line(p, a, b);
        let (off_q, t_q) = point_line(q, a, b);
        if off_p > tol || off_q > tol || !t_p.is_finite() || !t_q.is_finite() {
            return None;
        }
        let lo = t_p.min(t_q).clamp(0., 1.);
        let hi = t_p.max(t_q).clamp(0., 1.);
        if hi > lo + 1e-10 {
            out.push([lo, hi]);
        }
    }
    if out.is_empty() {
        return None;
    }
    out.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut merged: Vec<[f64; 2]> = Vec::with_capacity(out.len());
    for span in out {
        if let Some(last) = merged.last_mut()
            && span[0] <= last[1] + 1e-10
        {
            last[1] = last[1].max(span[1]);
        } else {
            merged.push(span);
        }
    }
    Some(merged)
}

/// Whether a presentation structural line still contains at least one original member.  An
/// all-`implied:` line is wholly reader-generated and must stay out of section-boundary facts.
fn has_native_structural_piece(g: &Value, o: &Object) -> bool {
    if !o.inferred_geometry {
        return true;
    }
    let Some(index) =
        o.id.strip_prefix("presentation:line:")
            .and_then(|s| s.parse::<usize>().ok())
    else {
        return false;
    };
    g.get("structural_lines")
        .and_then(Value::as_array)
        .and_then(|lines| lines.get(index))
        .and_then(|line| line.get("pieces"))
        .and_then(Value::as_array)
        .is_some_and(|pieces| {
            pieces
                .iter()
                .filter_map(Value::as_str)
                .any(|piece| piece.starts_with("member:"))
        })
}

/// Curves belonging to a target outline after removing only its inferred structural spans.
/// Non-presentation and wholly inferred objects retain their existing behavior.
fn native_outline_pieces(
    g: &Value,
    o: &Object,
    index: usize,
    curves: &[Vec<Piece>],
    tol: f64,
) -> Vec<Piece> {
    if !o.inferred_geometry {
        return curves[index].clone();
    }
    if !has_native_structural_piece(g, o) {
        return Vec::new();
    }
    let Some(implied) = structural_implied_intervals(g, o, tol) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for piece in &curves[index] {
        let mut cuts = vec![piece.lo, piece.hi];
        for [lo, hi] in &implied {
            let lo = lo.max(piece.lo);
            let hi = hi.min(piece.hi);
            if hi > lo + 1e-10 {
                cuts.extend([lo, hi]);
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        for window in cuts.windows(2) {
            let (lo, hi) = (window[0], window[1]);
            if hi <= lo + 1e-10 {
                continue;
            }
            let midpoint = (lo + hi) / 2.;
            if implied.iter().any(|[inferred_lo, inferred_hi]| {
                midpoint > *inferred_lo && midpoint < *inferred_hi
            }) {
                continue;
            }
            let span = (piece.hi - piece.lo).max(1e-12);
            out.push(Piece {
                edge: piece
                    .edge
                    .slice((lo - piece.lo) / span, (hi - piece.lo) / span),
                lo,
                hi,
                parameter: piece.parameter,
            });
        }
    }
    out
}
fn evidence(v: &Value) -> Vec<&str> {
    v.get("evidence_ids")
        .or_else(|| v.get("evidence"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}
fn single<'a>(v: &'a Value, prefix: &str, allowed: &[&str]) -> &'a str {
    let values: BTreeSet<&str> = evidence(v)
        .into_iter()
        .filter_map(|s| s.strip_prefix(prefix))
        .collect();
    if values.len() == 1 && allowed.contains(values.first().unwrap()) {
        values.first().unwrap()
    } else {
        "unknown"
    }
}
fn formal<'a>(v: &'a Value, name: &str, allowed: &[&str]) -> Option<&'a str> {
    v.get(name)
        .and_then(Value::as_str)
        .filter(|value| allowed.contains(value))
}
fn origin(o: &Object, region: &str, members: &BTreeMap<&str, &Value>) -> String {
    let mut found = BTreeSet::new();
    for id in &o.member_ids {
        if let Some(m) = members.get(id.as_str()) {
            if let Some(c) = formal(
                m,
                "section_origin",
                &["pre_existing", "cut_formed", "suppressed", "unknown"],
            ) {
                found.insert(c.to_owned());
                continue;
            }
            for s in evidence(m)
                .into_iter()
                .filter_map(|s| s.strip_prefix("section_origin_region:"))
            {
                if let Ok(v) = serde_json::from_str::<Value>(s)
                    && v["region"].as_str() == Some(region)
                    && let Some(c) = v["class"].as_str()
                    && ["pre_existing", "cut_formed", "suppressed", "unknown"].contains(&c)
                {
                    found.insert(c.to_owned());
                }
            }
        }
    }
    if found.len() == 1 {
        found.into_iter().next().unwrap()
    } else {
        "unknown".into()
    }
}
fn literal(out: &mut Vec<Relation>, subject: usize, predicate: &'static str, value: &str) {
    out.push(Relation {
        subject,
        predicate,
        object: None,
        literal: Some(value.into()),
        number: None,
    });
}
fn number(out: &mut Vec<Relation>, subject: usize, predicate: &'static str, value: f64) {
    // Source parameters retain full f64 precision; the old drawing-unit rounding is not applicable.
    out.push(Relation {
        subject,
        predicate,
        object: None,
        literal: None,
        number: Some(value),
    });
}
fn occurrence(
    layer: &str,
    class: &'static str,
    region: &Object,
    line: &Object,
    serial: usize,
) -> Object {
    let mut o = line.clone();
    o.id = format!("{layer}:{class}:{}:{}:{serial}", region.id, line.id);
    o.classes = vec![class];
    o.expression_id = region.expression_id.clone();
    o.expression_kind = None;
    o.geometry = Geometry::default();
    o.angle_deg = None;
    o.spacing = None;
    o.text = None;
    o.span = None;
    o
}
fn context(
    region: &Object,
    expressions: &BTreeMap<&str, &Value>,
    views: &BTreeMap<&str, &Value>,
) -> (String, String) {
    let empty = Value::Null;
    let expression = region
        .expression_id
        .as_deref()
        .and_then(|id| expressions.get(id))
        .copied()
        .unwrap_or(&empty);
    let view = region
        .view
        .as_deref()
        .and_then(|id| views.get(id))
        .copied()
        .unwrap_or(&empty);
    let ct = formal(
        expression,
        "cross_section_type",
        &["superimposed", "removed", "unknown"],
    )
    .unwrap_or_else(|| {
        single(
            expression,
            "cross_section_type:",
            &["superimposed", "removed", "unknown"],
        )
    });
    let ev = evidence(view);
    let ee = evidence(expression);
    let extent = formal(
        expression,
        "section_extent",
        &["half", "local", "local_axis", "cross_section", "unknown"],
    )
    .unwrap_or_else(|| {
        if ev.contains(&"section_type:half") || ee.contains(&"section_context:section_type:half") {
            "half"
        } else if ee.contains(&"section_context:local") {
            "local"
        } else if ct != "unknown" {
            "cross_section"
        } else {
            "unknown"
        }
    });
    // `section_geometry` remains a graph fact, but the v7 rules exporter
    // derives its fact from the native HATCH boundary below. The two sources
    // are intentionally kept separate until a later unification decision.
    (ct.into(), extent.into())
}

pub(super) fn augment(
    layer: &str,
    g: &Value,
    d: &Dxf<'_>,
    objects: &mut Vec<Object>,
    relations: &mut Vec<Relation>,
) -> Result<()> {
    // Generic geometry relations are intentionally drawing-wide because they serve annotation
    // rules.  A hatch boundary, however, can only be crossed by an outline in the same view;
    // discard cross-view pairs before the section-specific shape consumes `crossedBy`.
    relations.retain(|r| {
        if r.predicate != "crossedBy" {
            return true;
        }
        let Some(source) = objects.get(r.subject) else {
            return true;
        };
        if !source.is("HatchLine") {
            return true;
        }
        let Some(target) = r.object.and_then(|i| objects.get(i)) else {
            return true;
        };
        if source.inferred_geometry
            || (target.inferred_geometry && !has_native_structural_piece(g, target))
        {
            return false;
        }
        !target.is("VisibleOutline") || source.view == target.view
    });
    let knowledge: toml::Value = toml::from_str(KNOWLEDGE)?;
    let param = |key: &str| -> Result<f64> {
        knowledge["section_parameters"][key]
            .as_float()
            .filter(|v| v.is_finite() && *v > 0.)
            .context("section_parameter")
    };
    let members: BTreeMap<&str, &Value> = list(g, "members")?
        .iter()
        .filter_map(|m| Some((m["id"].as_str()?, m)))
        .collect();
    let text_heights: Vec<f64> = objects
        .iter()
        .filter_map(|o| o.height)
        .filter(|h| *h > 0. && h.is_finite())
        .collect();
    let text_height = median(&mut text_heights.clone());
    let arrow_length = arrow_length(objects, d, &members);
    let mut heights = text_heights;
    // With no text, use median hatch spacing as a drawing scale; otherwise preserve unavailable coverage.
    if heights.is_empty() {
        heights = objects
            .iter()
            .filter_map(|o| o.spacing)
            .filter(|h| *h > 0. && h.is_finite())
            .collect();
    }
    let Some(scale) = median(&mut heights) else {
        return Ok(());
    };
    let (tol, sagitta, min_chord, cutoff_ratio, stroke_ratio, arrow_ratio, visible_ratio) = (
        scale * param("coincidence_text_height")?,
        scale * param("sagitta_text_height")?,
        scale * param("interior_chord_text_height")?,
        param("section_cutoff_area_ratio_max")?,
        param("section_nominal_thick_stroke_text_height_ratio")?,
        param("section_arrow_length_in_thick_strokes")?,
        param("section_visible_overrun_stroke_ratio")?,
    );
    let overrun_scale = OverrunScale {
        text_height,
        arrow_length,
        text_ratio: stroke_ratio,
        arrow_strokes: arrow_ratio,
        visible_ratio,
    };
    let expressions: BTreeMap<&str, &Value> = list(g, "expressions")?
        .iter()
        .filter_map(|e| Some((e["id"].as_str()?, e)))
        .collect();
    let views: BTreeMap<&str, &Value> = g["views"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| Some((v["id"].as_str()?, v)))
        .collect();
    let original_count = objects.len();
    let curves: Vec<Vec<Piece>> = objects
        .iter()
        .map(|o| pieces(o, d, layer, &members))
        .collect();
    for ri in 0..original_count {
        if !objects[ri].is("SectionRegion") {
            continue;
        }
        let region = objects[ri].clone();
        let (ctype, extent) = context(&region, &expressions, &views);
        let raw = if region.source_handles.len() == 1 {
            d.by_handle.get(region.source_handles[0].as_str()).copied()
        } else {
            None
        };
        let face = raw
            .filter(|r| r.kind == "HATCH")
            .map(|r| face(r, tol, sagitta))
            .unwrap_or_default();
        let inferred_geometry = if face.complete {
            "complete"
        } else if face.edges.is_empty() {
            "unavailable"
        } else {
            "partial"
        };
        let geometry = inferred_geometry;
        // A complete hatch in an explicitly named detail view can carry a local
        // symmetric half boundary on its centre axis.  This is the narrow
        // geometry-only exception covered by GB/T 4458.6 5.5/5.6 and GB/T
        // 16675.1 5.2; a general unknown view remains unknown.
        let detail_axis_exception = extent == "unknown"
            && face.complete
            && region
                .view
                .as_deref()
                .and_then(|id| views.get(id))
                .and_then(|v| v["kind"].as_str())
                == Some("detail_view");
        literal(relations, ri, "crossSectionType", &ctype);
        literal(relations, ri, "sectionExtent", &extent);
        literal(relations, ri, "sectionGeometry", geometry);
        let region_expression = region.expression_id.clone();
        let mut overrun_specs: Vec<(usize, usize, OverrunSpan)> = Vec::new();
        relations.retain(|r| {
            if r.predicate != "crossedBy" {
                return true;
            }
            let Some(source) = objects.get(r.subject) else {
                return true;
            };
            let Some(target) = r.object.and_then(|i| objects.get(i)) else {
                return true;
            };
            if !source.is("HatchLine")
                || !target.is("VisibleOutline")
                || source.expression_id != region_expression
            {
                return true;
            }
            if source.inferred_geometry {
                return false;
            }
            let outline_pieces =
                native_outline_pieces(g, target, r.object.unwrap_or(r.subject), &curves, tol);
            if outline_pieces.is_empty() {
                return false;
            }
            let Some(threshold) = visible_overrun_threshold(target, d, &members, &overrun_scale)
            else {
                return false;
            };
            for hatch in &curves[r.subject] {
                for outline in &outline_pieces {
                    let spans = if face.complete {
                        overrun_outside_face_spans(hatch, outline, &face, tol, sagitta)
                    } else {
                        overrun_without_face_spans(hatch, outline, sagitta)
                    };
                    for span in spans.into_iter().filter(|span| span.exposed > threshold) {
                        overrun_specs.push((r.subject, r.object.unwrap_or(r.subject), span));
                    }
                }
            }
            // The generic relation is replaced by one relation per visible continuation.  A
            // conforming touch therefore leaves no G2-019 edge, while a long line crossing more
            // than once produces independently highlightable spans.
            false
        });
        let mut overrun_serial = 0usize;
        for (source_index, carrier_index, span) in overrun_specs.drain(..) {
            let mut carrier = objects[carrier_index].clone();
            overrun_serial += 1;
            carrier.id = format!(
                "{layer}:hatch_overrun_span:{:.9}:{:.9}:target:{:.9}:{:.9}:{overrun_serial}",
                span.hatch[0], span.hatch[1], span.outline[0], span.outline[1]
            );
            carrier.span = Some(SpanInfo {
                source: span.hatch,
                parameter: "hatch_overrun",
                detail: serde_json::json!({
                    "kind":"hatch_overrun",
                    "hatch_interval":span.hatch,
                    "outline_interval":span.outline,
                    "entry":[span.entry.0,span.entry.1],
                    "exit":[span.exit.0,span.exit.1],
                    "length":span.length,
                    "exposed":span.exposed
                })
                .to_string(),
            });
            let carrier_index = objects.len();
            objects.push(carrier);
            relations.push(Relation::node(source_index, "crossedBy", carrier_index));
        }
        let same_view = |i: usize| objects[i].view == region.view;
        let centers: Vec<usize> = (0..original_count)
            .filter(|i| objects[*i].is("CentreLine") && same_view(*i))
            .collect();
        // Only the source drawing's contour expression may form a section boundary.  A
        // dimension's extension member can share the VisibleOutline class when it reuses a
        // native line, but it is annotation geometry rather than part geometry; admitting it
        // here would turn a keyway boundary into a cut and suppress its independent unknown
        // finding.  The same distinction is used for side evidence below.
        let is_part_outline = |o: &Object| {
            o.is("VisibleOutline")
                && !o.inferred_geometry
                && o.expression_kind
                    .as_deref()
                    .is_none_or(|kind| kind == "visible_contour")
        };
        let is_part_curve = |o: &Object| {
            is_part_outline(o)
                || (o.is("ThreadLine")
                    && !o.inferred_geometry
                    && o.expression_kind
                        .as_deref()
                        .is_none_or(|kind| kind == "thread_line"))
        };
        let outlines: Vec<usize> = (0..original_count)
            .filter(|i| is_part_outline(&objects[*i]) && same_view(*i))
            .collect();
        let interior: Vec<usize> = (0..original_count)
            .filter(|i| {
                !objects[*i].inferred_geometry
                    && (objects[*i].is("VisibleOutline") || objects[*i].is("ThreadLine"))
                    && same_view(*i)
            })
            .collect();
        // Boundary promotion uses only independent part geometry for its two-sided test.  The
        // full interior list is retained below so an annotation alias that is not eligible to
        // form a boundary still receives the original G2-018 unknown finding.
        let boundary_evidence: Vec<usize> = interior
            .iter()
            .copied()
            .filter(|i| is_part_curve(&objects[*i]))
            .collect();
        let boundary_cuts = boundary_cut_chains(
            &face,
            &outlines,
            &curves,
            &boundary_evidence,
            tol,
            sagitta,
            cutoff_ratio,
        );
        let cut_outline_indices: BTreeSet<usize> = boundary_cuts
            .iter()
            .flat_map(|cut| cut.members.iter().copied())
            .collect();
        // Native HATCH has no loose HatchLine object.  Reuse the approved crossedBy relation on
        // SectionRegion so a boundary chain is visible to the source-layer shape as well, but
        // only when one of the HATCH's own pattern lines visibly continues into the cut block.
        let native_pattern = raw
            .filter(|r| r.kind == "HATCH")
            .map(|r| native_pattern_segments(r, &face, tol, sagitta))
            .unwrap_or_default();
        for cut in boundary_cuts.iter() {
            for &outline in &cut.members {
                let Some(threshold) =
                    visible_overrun_threshold(&objects[outline], d, &members, &overrun_scale)
                else {
                    continue;
                };
                for line in &native_pattern {
                    let hatch_piece = Piece {
                        edge: *line,
                        lo: 0.,
                        hi: 1.,
                        parameter: "pattern_derived",
                    };
                    for segment in cut.contour.windows(2) {
                        let outline_piece = Piece {
                            edge: Edge::Line(segment[0], segment[1]),
                            lo: 0.,
                            hi: 1.,
                            parameter: "polyline_length",
                        };
                        for span in overrun_inside_cut_spans(
                            &hatch_piece,
                            &outline_piece,
                            &cut.contour,
                            &cut.polygon,
                            tol,
                            sagitta,
                        )
                        .into_iter()
                        .filter(|span| span.exposed > threshold)
                        {
                            overrun_specs.push((ri, outline, span));
                        }
                    }
                }
            }
        }
        for (source_index, carrier_index, span) in overrun_specs.drain(..) {
            let mut carrier = objects[carrier_index].clone();
            overrun_serial += 1;
            carrier.id = format!(
                "{layer}:hatch_overrun_span:{:.9}:{:.9}:target:{:.9}:{:.9}:{overrun_serial}",
                span.hatch[0], span.hatch[1], span.outline[0], span.outline[1]
            );
            carrier.span = Some(SpanInfo {
                source: span.hatch,
                parameter: "hatch_overrun",
                detail: serde_json::json!({
                    "kind":"hatch_overrun",
                    "hatch_interval":span.hatch,
                    "outline_interval":span.outline,
                    "entry":[span.entry.0,span.entry.1],
                    "exit":[span.exit.0,span.exit.1],
                    "length":span.length,
                    "exposed":span.exposed,
                    "pattern_derived":true
                })
                .to_string(),
            });
            let carrier_index = objects.len();
            objects.push(carrier);
            relations.push(Relation::node(source_index, "crossedBy", carrier_index));
        }
        let region_id = region.expression_id.as_deref().unwrap_or("");
        let mut serial = 0;
        for si in centers {
            for p in &curves[si] {
                // In a local section a dash-dot circular/arc centreline is a
                // pitch/distribution circle, not a section outer edge.  Keep
                // straight axis spans eligible for the symmetric-half exception.
                if extent == "local" && matches!(p.edge, Edge::Arc { .. }) {
                    continue;
                }
                let mut spans: Vec<[f64; 2]> = face
                    .edges
                    .iter()
                    .flat_map(|edge| p.edge.overlaps(*edge, tol))
                    .collect();
                spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
                let mut merged: Vec<[f64; 2]> = Vec::new();
                for span in spans {
                    if let Some(last) = merged.last_mut()
                        && span[0] <= last[1] + 1e-10
                    {
                        last[1] = last[1].max(span[1]);
                    } else {
                        merged.push(span);
                    }
                }
                for span in merged {
                    let coverage: Vec<(usize, [f64; 2])> = outlines
                        .iter()
                        .filter(|i| **i != si)
                        .flat_map(|i| {
                            curves[*i].iter().flat_map(move |q| {
                                p.edge
                                    .overlaps(q.edge, tol)
                                    .into_iter()
                                    .map(move |s| (*i, s))
                            })
                        })
                        .collect();
                    let mut cuts = vec![span[0], span[1]];
                    cuts.extend(
                        coverage
                            .iter()
                            .flat_map(|(_, s)| *s)
                            .filter(|t| *t > span[0] && *t < span[1]),
                    );
                    cuts.sort_by(f64::total_cmp);
                    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
                    for w in cuts
                        .windows(2)
                        .filter(|w| (w[1] - w[0]) * p.edge.length() > tol)
                    {
                        let id = objects.len();
                        serial += 1;
                        objects.push(occurrence(
                            layer,
                            "SectionBoundarySpan",
                            &region,
                            &objects[si],
                            serial,
                        ));
                        relations.push(Relation::node(id, "sectionRegion", ri));
                        relations.push(Relation::node(id, "sourceLine", si));
                        relations.push(Relation::node(id, "boundaryCarrier", si));
                        let midpoint = (w[0] + w[1]) / 2.;
                        let covered: BTreeSet<usize> = coverage
                            .iter()
                            .filter(|(_, s)| s[0] <= midpoint && midpoint <= s[1])
                            .map(|(i, _)| *i)
                            .collect();
                        for i in covered {
                            relations.push(Relation::node(id, "boundaryCarrier", i));
                        }
                        number(
                            relations,
                            id,
                            "boundaryIntervalStart",
                            p.lo + (p.hi - p.lo) * w[0],
                        );
                        number(
                            relations,
                            id,
                            "boundaryIntervalEnd",
                            p.lo + (p.hi - p.lo) * w[1],
                        );
                        literal(relations, id, "boundaryParameter", p.parameter);
                        literal(relations, id, "sectionGeometry", geometry);
                        literal(
                            relations,
                            id,
                            "sectionExtent",
                            if detail_axis_exception || extent == "local" {
                                "local_axis"
                            } else {
                                &extent
                            },
                        );
                        literal(
                            relations,
                            id,
                            "sectionOrigin",
                            &origin(&objects[si], region_id, &members),
                        );
                    }
                }
            }
        }
        if !face.complete {
            continue;
        }
        for &si in &interior {
            if cut_outline_indices.contains(&si)
                || cut_outline_indices
                    .iter()
                    .any(|boundary| same_source_interval(objects, &curves, si, *boundary, tol))
            {
                continue;
            }
            let samples: Option<Vec<Vec<P>>> =
                curves[si].iter().map(|p| p.edge.sample(sagitta)).collect();
            let Some(samples) = samples else {
                continue;
            };
            let inside_length: f64 = samples
                .iter()
                .flat_map(|p| p.windows(2))
                .map(|w| interior_length(w[0], w[1], &face, tol))
                .sum();
            if inside_length < min_chord {
                continue;
            }
            let closed = samples
                .first()
                .and_then(|p| p.first())
                .zip(samples.last().and_then(|p| p.last()))
                .is_some_and(|(a, b)| dist(*a, *b) <= tol);
            let connected = samples.windows(2).all(|w| {
                w[0].last()
                    .zip(w[1].first())
                    .is_some_and(|(a, b)| dist(*a, *b) <= tol)
            });
            let continues = connected
                && if closed {
                    samples
                        .iter()
                        .flatten()
                        .any(|p| !inside(*p, &face.polygon_edges, tol))
                } else {
                    samples
                        .first()
                        .and_then(|p| p.first())
                        .zip(samples.last().and_then(|p| p.last()))
                        .is_some_and(|(a, b)| {
                            !inside(*a, &face.polygon_edges, tol)
                                && !inside(*b, &face.polygon_edges, tol)
                        })
                };
            let id = objects.len();
            serial += 1;
            objects.push(occurrence(
                layer,
                "SectionInteriorUse",
                &region,
                &objects[si],
                serial,
            ));
            relations.push(Relation::node(id, "sectionRegion", ri));
            // Preserve all graph roles of the very same source curve and interval. The shape decides
            // whether any of these roles supplies an exception; no exception is applied here.
            for &alias in &interior {
                if objects[alias].source_handles == objects[si].source_handles
                    && curves[alias] == curves[si]
                {
                    relations.push(Relation::node(id, "sourceLine", alias));
                }
            }
            literal(relations, id, "sectionGeometry", "complete");
            literal(relations, id, "crossSectionType", &ctype);
            literal(
                relations,
                id,
                "parentOutlineContinuation",
                if continues { "true" } else { "false" },
            );
            literal(
                relations,
                id,
                "sectionOrigin",
                &origin(&objects[si], region_id, &members),
            );
        }
    }
    ensure!(objects.len() <= 20_000, "section_fact_object_budget");
    Ok(())
}
