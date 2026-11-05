//! Generic source-only usage ontology. No caller classifications or repair handles.
use crate::{
    native_layers::source_members,
    native_readback::{Drawing, Record},
};
use anyhow::{Result, ensure};
use oe_contracts::*;
use oe_core::sha256;
use std::{
    collections::{BTreeMap, BTreeSet},
    f64::consts::TAU,
    path::Path,
};

pub const PROFILE: &str = "dxf-usage-rules/1";
pub const EPS: f64 = 1e-7; // Original drawing units, never an engineering tolerance.
type P = [f64; 2];
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}
fn dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn lerp(a: P, b: P, t: f64) -> P {
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
}
fn finite(p: P) -> bool {
    p.iter().all(|x| x.is_finite() && x.abs() < 1e8)
}

#[derive(Clone, Debug)]
enum Curve {
    Line(P, P),
    Arc {
        center: P,
        r: f64,
        start: f64,
        sweep: f64,
        full: bool,
    },
}
impl Curve {
    fn at(&self, t: f64) -> P {
        match self {
            Self::Line(a, b) => lerp(*a, *b, t),
            Self::Arc {
                center,
                r,
                start,
                sweep,
                ..
            } => [
                center[0] + r * (start + t * sweep).cos(),
                center[1] + r * (start + t * sweep).sin(),
            ],
        }
    }
    fn ends(&self) -> Vec<(f64, P)> {
        match self {
            Self::Arc { full: true, .. } => vec![],
            _ => vec![(0., self.at(0.)), (1., self.at(1.))],
        }
    }
    fn parameter(&self, p: P) -> Option<(f64, f64)> {
        if !finite(p) {
            return None;
        }
        match self {
            Self::Line(a, b) => {
                let d = sub(*b, *a);
                let len = dist(*a, *b);
                let t = dot(sub(p, *a), d) / dot(d, d);
                if t < -EPS / len || t > 1. + EPS / len {
                    return None;
                }
                let t = t.clamp(0., 1.);
                let residual = dist(p, self.at(t));
                (residual <= EPS).then_some((t, residual))
            }
            Self::Arc {
                center,
                r,
                start,
                sweep,
                full,
            } => {
                let residual = (dist(p, *center) - r).abs();
                if residual > EPS {
                    return None;
                }
                let mut angle =
                    ((p[1] - center[1]).atan2(p[0] - center[0]) - start).rem_euclid(TAU);
                if TAU - angle <= EPS / r {
                    angle = 0.
                }
                if !full && angle > *sweep + EPS / r {
                    return None;
                }
                Some(((angle / sweep).clamp(0., 1.), residual))
            }
        }
    }
}
fn planar(r: &Record) -> bool {
    [
        (30, 0.),
        (31, 0.),
        (39, 0.),
        (210, 0.),
        (220, 0.),
        (230, 1.),
    ]
    .into_iter()
    .all(|(c, v)| r.get(c).is_none() || r.number(c).is_ok_and(|x| x == v))
}
fn curve(r: &Record) -> Result<Curve> {
    ensure!(planar(r) && r.block.is_empty(), "not_root_xy_profile");
    let p = r.xy(10)?;
    ensure!(finite(p), "coordinates_out_of_profile");
    if r.kind == "LINE" {
        let q = r.xy(11)?;
        ensure!(finite(q) && dist(p, q) > EPS, "degenerate_line");
        return Ok(Curve::Line(p, q));
    }
    ensure!(
        matches!(r.kind.as_str(), "ARC" | "CIRCLE"),
        "unsupported_curve_kind"
    );
    let radius = r.number(40)?;
    ensure!(radius > EPS && radius < 1e8, "radius_out_of_profile");
    let full = r.kind == "CIRCLE";
    let start = if full {
        0.
    } else {
        r.number(50)?.to_radians().rem_euclid(TAU)
    };
    let sweep = if full {
        TAU
    } else {
        (r.number(51)?.to_radians() - start).rem_euclid(TAU)
    };
    ensure!(sweep * radius > EPS, "empty_arc");
    Ok(Curve::Arc {
        center: p,
        r: radius,
        start,
        sweep,
        full,
    })
}
fn geometry(c: &Curve) -> (&'static str, Vec<f64>) {
    match c {
        Curve::Line(a, b) => ("line", a.iter().chain(b).copied().collect()),
        Curve::Arc {
            center,
            r,
            start,
            sweep,
            full,
        } => (
            if *full { "circle" } else { "arc" },
            vec![center[0], center[1], *r, *start, *sweep],
        ),
    }
}
fn dimension_kind(r: &Record) -> i32 {
    r.get(70).and_then(|v| v.parse::<i32>().ok()).unwrap_or(-1) & 15
}
fn native_role(r: &Record) -> (&'static str, &'static str) {
    match r.kind.as_str() {
        "DIMENSION" => (
            match dimension_kind(r) {
                0 | 1 => "dimension_linear",
                2 | 5 => "dimension_angle",
                3 => "dimension_diameter",
                4 => "dimension_radius",
                _ => "dimension_other",
            },
            "observed",
        ),
        "HATCH" => ("hatch_native", "observed"),
        "TOLERANCE" => ("tolerance_native", "observed"),
        "LEADER" | "MULTILEADER" => ("leader_native", "observed"),
        "TEXT" | "MTEXT" => ("text_native", "observed"),
        "POINT" => ("construction_native", "observed"),
        "INSERT" => ("block_unresolved", "unknown"),
        "SOLID" | "TRACE" => ("fill_unresolved", "unknown"),
        "LINE" | "ARC" | "CIRCLE" | "LWPOLYLINE" | "POLYLINE" | "ELLIPSE" | "SPLINE" => {
            ("geometry_candidate", "candidate")
        }
        _ => ("other_unresolved", "unknown"),
    }
}
#[allow(clippy::too_many_arguments)]
fn add_use(
    g: &mut MdUsageGraph,
    root: &str,
    role: &str,
    consumer: &str,
    state: &str,
    range: [f64; 2],
    rule: &str,
    relations: Vec<String>,
) -> Result<()> {
    ensure!(g.uses.len() < 30000, "usage_budget_uses");
    if let Some(u) = g.uses.iter_mut().find(|u| {
        u.occurrence_id == root
            && u.role == role
            && u.consumer_id == consumer
            && u.interval == range
    }) {
        for r in relations {
            if !u.relation_ids.contains(&r) {
                u.relation_ids.push(r)
            }
        }
        return Ok(());
    }
    g.uses.push(MdDiscoveredUse {
        id: format!("use:{}", g.uses.len()),
        occurrence_id: root.into(),
        role: role.into(),
        consumer_id: consumer.into(),
        engineering_target: None,
        state: state.into(),
        interval: range.to_vec(),
        rule: rule.into(),
        relation_ids: relations,
    });
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn relation(
    g: &mut MdUsageGraph,
    kind: &str,
    from: &str,
    to: Option<&str>,
    a: Option<f64>,
    b: Option<f64>,
    span: Vec<f64>,
    points: Vec<f64>,
    residual: Option<f64>,
    state: &str,
    rule: &str,
) -> Result<String> {
    ensure!(g.relations.len() < 100000, "usage_budget_relations");
    let id = format!("relation:{}", g.relations.len());
    g.relations.push(MdUsageRelation {
        id: id.clone(),
        kind: kind.into(),
        from_id: from.into(),
        to_id: to.map(str::to_string),
        from_parameter: a,
        to_parameter: b,
        source_interval: span,
        points,
        residual,
        state: state.into(),
        rule: rule.into(),
    });
    Ok(id)
}
fn projected(a: P, b: P, p: P) -> f64 {
    dot(sub(p, a), sub(b, a)) / dot(sub(b, a), sub(b, a))
}
fn overlap(a: P, b: P, c: P, d: P) -> Option<([f64; 2], [f64; 2])> {
    let len = dist(a, b);
    if len <= EPS || dist(c, d) <= EPS {
        return None;
    }
    if [c, d]
        .iter()
        .any(|p| cross(sub(*p, a), sub(b, a)).abs() / len > EPS)
    {
        return None;
    }
    let x = projected(a, b, c);
    let y = projected(a, b, d);
    let lo = x.min(y).max(0.);
    let hi = x.max(y).min(1.);
    if (hi - lo) * len <= EPS {
        return None;
    }
    Some((
        [lo, hi],
        [
            projected(c, d, lerp(a, b, lo)).clamp(0., 1.),
            projected(c, d, lerp(a, b, hi)).clamp(0., 1.),
        ],
    ))
}

/// The discovery entry accepts only drawing bytes. There is no parameter for
/// previous classifications, manually selected handles, units or paired geometry.
pub fn discover(bytes: &[u8]) -> Result<MdUsageGraph> {
    let d = Drawing::parse(bytes)?;
    let members = source_members(bytes)?;
    let mut roots: Vec<_> = members
        .iter()
        .filter(|m| m.block_path.len() == 1)
        .map(|m| d.entity(&m.handle))
        .collect::<Result<_>>()?;
    roots.sort_by_key(|r| r.get(5));
    ensure!(
        !roots.is_empty() && roots.len() <= 10000,
        "usage_root_budget"
    );
    let layers: BTreeMap<_, _> = d
        .records
        .iter()
        .filter(|r| r.kind == "LAYER")
        .filter_map(|r| Some((r.get(2)?, r.get(6).unwrap_or("CONTINUOUS"))))
        .collect();
    let mut g=MdUsageGraph{schema:"oev2.drawing-usage-graph/1".into(),profile:PROFILE.into(),source_sha256:sha256(bytes),epsilon_drawing_units:EPS,entities:vec![],members,uses:vec![],relations:vec![],segments:vec![],unknowns:vec![
        "Unclassified original source only; no previous review, manual source intervals, repaired drawing or paired model was consumed".into(),
        "Generic geometric uses, linetype hints and dimension support matches remain candidates; physical features, process intent and view identities remain unestablished".into(),
        "Root planar LINE/ARC/CIRCLE endpoint incidence and LINE crossings/overlap only; nested block geometry, curved interior intersections and exploded annotation recognition are unqualified".into(),
        "Source coordinates are not calibrated product mm. Endpoint obligations are not missing-line verdicts; no forced closure or new geometry".into(),
        format!("Native INSUNITS declaration: {} (retained, not used as product scale)",d.header("$INSUNITS",70).unwrap_or("not supplied"))]};
    let mut curves = BTreeMap::new();
    let mut cuts: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for r in &roots {
        let h = r.handle()?;
        let layer = r.get(8).unwrap_or("0");
        let raw = r.get(6).unwrap_or("BYLAYER");
        let line_type = if raw.eq_ignore_ascii_case("BYLAYER") {
            layers.get(layer).copied().unwrap_or("unresolved_BYLAYER")
        } else {
            raw
        }
        .to_string();
        let qualified = curve(r);
        let (kind, coordinates, limit) = match qualified {
            Ok(c) => {
                let (k, p) = geometry(&c);
                if matches!(c, Curve::Line(..)) {
                    cuts.insert(h.into(), vec![0., 1.]);
                }
                curves.insert(h.to_string(), c);
                (k, p, None)
            }
            Err(e) => ("unqualified", vec![], Some(e.to_string())),
        };
        let text = matches!(
            r.kind.as_str(),
            "TEXT" | "MTEXT" | "TOLERANCE" | "DIMENSION"
        )
        .then(|| {
            r.tags
                .iter()
                .filter(|(c, _)| *c == 1 || (*c == 3 && r.kind == "MTEXT"))
                .map(|(_, s)| s.as_str())
                .collect::<String>()
        })
        .filter(|s| !s.is_empty());
        let member_ids = g
            .members
            .iter()
            .filter(|m| m.block_path.first().is_some_and(|x| x == h))
            .map(|m| m.id.clone())
            .collect();
        g.entities.push(MdUsageEntity {
            id: h.into(),
            handle: h.into(),
            kind: r.kind.clone(),
            layer: layer.into(),
            native_owner: r.get(330).map(str::to_string),
            line_type: line_type.clone(),
            text,
            member_ids,
            geometry_kind: kind.into(),
            coordinates,
            geometry_limit: limit,
        });
        let (role, state) = native_role(r);
        add_use(
            &mut g,
            h,
            role,
            h,
            state,
            [0., 1.],
            "native_kind_or_unresolved_geometry",
            vec![],
        )?;
        if role == "geometry_candidate" {
            let lt = line_type.to_uppercase();
            for (hint, hit) in [
                (
                    "axis_candidate",
                    lt.contains("CENTER")
                        || lt.contains("CENTRE")
                        || lt.contains("中心")
                        || lt.contains("轴线"),
                ),
                (
                    "hidden_candidate",
                    lt.contains("HIDDEN") || lt.contains("DASHED") || lt.contains("虚线"),
                ),
            ] {
                if hit {
                    add_use(
                        &mut g,
                        h,
                        hint,
                        h,
                        "candidate",
                        [0., 1.],
                        "source_linetype_hint",
                        vec![],
                    )?
                }
            }
        }
    }
    ensure!(curves.len() <= 4096, "usage_curve_budget");
    for m in g.members.clone() {
        if m.block_path.len() > 1 {
            let parent = m.block_path[..m.block_path.len() - 1].join("/");
            relation(
                &mut g,
                "native_member",
                &parent,
                Some(&m.id),
                None,
                None,
                vec![],
                vec![],
                None,
                "observed",
                "native_container_expansion",
            )?;
        }
    }
    let owner: BTreeMap<_, _> = g
        .entities
        .iter()
        .map(|e| (e.id.clone(), e.native_owner.clone()))
        .collect();
    let mut linked = BTreeSet::new();
    for (h, c) in &curves {
        if let Curve::Arc { full: true, .. } = c {
            relation(
                &mut g,
                "native_closed_curve",
                h,
                Some(h),
                None,
                None,
                vec![0., 1.],
                vec![],
                Some(0.),
                "observed",
                "circle_domain_not_material_closure",
            )?;
        }
        for (t, p) in c.ends() {
            for (other, k) in &curves {
                if other == h || owner[h] != owner[other] {
                    continue;
                }
                if let Some((s, res)) = k.parameter(p) {
                    relation(
                        &mut g,
                        "endpoint_on_curve",
                        h,
                        Some(other),
                        Some(t),
                        Some(s),
                        vec![],
                        p.to_vec(),
                        Some(res),
                        "observed",
                        "source_incidence_not_physical_adjacency",
                    )?;
                    linked.insert((h.clone(), t == 1.));
                    if let Some(v) = cuts.get_mut(other) {
                        v.push(s)
                    }
                }
            }
        }
    }
    let entries: Vec<_> = curves.iter().collect();
    for (i, (h, c)) in entries.iter().enumerate() {
        for (k, e) in entries.iter().skip(i + 1) {
            if owner[*h] != owner[*k] {
                continue;
            }
            if let (Curve::Line(a, b), Curve::Line(c, d)) = (c, e) {
                if let Some((ab, cd)) = overlap(*a, *b, *c, *d) {
                    relation(
                        &mut g,
                        "collinear_overlap",
                        h,
                        Some(k),
                        Some(ab[0]),
                        Some(cd[0]),
                        ab.to_vec(),
                        [lerp(*a, *b, ab[0]), lerp(*a, *b, ab[1])]
                            .into_iter()
                            .flatten()
                            .collect(),
                        Some(0.),
                        "observed",
                        "distinct_sources_with_common_support",
                    )?;
                    cuts.get_mut(*h).unwrap().extend(ab);
                    cuts.get_mut(*k).unwrap().extend(cd);
                } else {
                    let u = sub(*b, *a);
                    let v = sub(*d, *c);
                    let den = cross(u, v);
                    if den.abs() > 1e-12 * dist(*a, *b) * dist(*c, *d) {
                        let t = cross(sub(*c, *a), v) / den;
                        let s = cross(sub(*c, *a), u) / den;
                        if t > 0. && t < 1. && s > 0. && s < 1. {
                            relation(
                                &mut g,
                                "interior_crossing",
                                h,
                                Some(k),
                                Some(t),
                                Some(s),
                                vec![],
                                lerp(*a, *b, t).to_vec(),
                                Some(0.),
                                "observed",
                                "drawing_crossing_not_physical_junction",
                            )?;
                            cuts.get_mut(*h).unwrap().push(t);
                            cuts.get_mut(*k).unwrap().push(s);
                        }
                    }
                }
            }
        }
    }
    for (h, c) in &curves {
        for (t, p) in c.ends() {
            if !linked.contains(&(h.clone(), t == 1.)) {
                relation(
                    &mut g,
                    "open_endpoint",
                    h,
                    None,
                    Some(t),
                    None,
                    vec![],
                    p.to_vec(),
                    None,
                    "unknown",
                    "endpoint_expression_obligation_unresolved",
                )?;
            }
        }
    }
    // Construction constraints are native definition facts plus candidate use
    // matches; they never remove the loose geometry's pre-existing unknown use.
    for r in roots.iter().filter(|r| r.kind == "DIMENSION") {
        let h = r.handle()?;
        let kind = dimension_kind(r);
        if !matches!(kind, 0 | 1)
            || !planar(r)
            || r.get(52)
                .is_some_and(|_| !r.number(52).is_ok_and(|v| v == 0.))
        {
            continue;
        }
        let Ok(a) = r.xy(13) else { continue };
        let Ok(b) = r.xy(14) else { continue };
        let Ok(q) = r.xy(10) else { continue };
        if ![a, b, q].into_iter().all(finite)
            || dist(a, b) <= EPS
            || [33, 34].iter().any(|c| {
                r.get(*c)
                    .is_some_and(|_| !r.number(*c).is_ok_and(|z| z == 0.))
            })
        {
            continue;
        }
        let angle = if kind == 1 {
            (b[1] - a[1]).atan2(b[0] - a[0])
        } else {
            match r.get(50) {
                Some(_) => r.number(50)?.to_radians(),
                None => 0.,
            }
        };
        let u = [angle.cos(), angle.sin()];
        let v = [-u[1], u[0]];
        let foot = |p: P| {
            let t = dot(sub(q, p), v);
            [p[0] + t * v[0], p[1] + t * v[1]]
        };
        let fa = foot(a);
        let fb = foot(b);
        for (k, c) in &curves {
            if owner[h] != owner[k] {
                continue;
            }
            for (idx, p) in [a, b].into_iter().enumerate() {
                if let Some((t, res)) = c.parameter(p) {
                    relation(
                        &mut g,
                        "dimension_definition_incidence",
                        h,
                        Some(k),
                        None,
                        Some(t),
                        vec![],
                        p.to_vec(),
                        Some(res),
                        "observed",
                        if idx == 0 {
                            "definition_13_on_curve"
                        } else {
                            "definition_14_on_curve"
                        },
                    )?;
                    if let Some(x) = cuts.get_mut(k) {
                        x.push(t)
                    }
                }
            }
            if let Curve::Line(p, z) = c {
                for (x, y, rule) in [
                    (fa, fb, "linear_dimension_line_support"),
                    (a, fa, "linear_extension_13_support"),
                    (b, fb, "linear_extension_14_support"),
                ] {
                    if let Some((range, _)) = overlap(*p, *z, x, y) {
                        let rid = relation(
                            &mut g,
                            "dimension_construction_overlap",
                            k,
                            Some(h),
                            Some(range[0]),
                            None,
                            range.to_vec(),
                            [lerp(*p, *z, range[0]), lerp(*p, *z, range[1])]
                                .into_iter()
                                .flatten()
                                .collect(),
                            Some(0.),
                            "candidate",
                            rule,
                        )?;
                        add_use(
                            &mut g,
                            k,
                            "dimension_linear",
                            h,
                            "candidate",
                            range,
                            rule,
                            vec![rid],
                        )?;
                        cuts.get_mut(k).unwrap().extend(range);
                    }
                }
            }
        }
    }
    for (h, mut values) in cuts {
        let c = &curves[&h];
        let length = dist(c.at(0.), c.at(1.));
        values.sort_by(f64::total_cmp);
        values.dedup_by(|a, b| (*a - *b).abs() * length <= EPS);
        // Always preserve both actual support bounds, including sub-epsilon
        // bookkeeping differences at the last contact.
        values[0] = 0.;
        *values.last_mut().unwrap() = 1.;
        for pair in values.windows(2) {
            let mid = (pair[0] + pair[1]) / 2.;
            let ids = g
                .uses
                .iter()
                .filter(|u| u.occurrence_id == h && u.interval[0] <= mid && mid <= u.interval[1])
                .map(|u| u.id.clone())
                .collect::<Vec<_>>();
            ensure!(
                !ids.is_empty() && g.segments.len() < 30000,
                "usage_segment_coverage_or_budget"
            );
            g.segments.push(MdUsageSegment {
                id: format!("segment:{}", g.segments.len()),
                occurrence_id: h.clone(),
                interval: pair.to_vec(),
                coordinates: [c.at(pair[0]), c.at(pair[1])]
                    .into_iter()
                    .flatten()
                    .collect(),
                use_ids: ids,
            });
        }
    }
    ensure!(
        g.validate().is_ok()
            && g.entities.iter().all(|x| x.validate().is_ok())
            && g.uses.iter().all(|x| x.validate().is_ok())
            && g.relations.iter().all(|x| x.validate().is_ok())
            && g.segments.iter().all(|x| x.validate().is_ok()),
        "usage_graph_contract"
    );
    ensure!(
        serde_json::to_vec(&g)?.len() <= 16 * 1024 * 1024,
        "usage_graph_output_budget"
    );
    Ok(g)
}

pub fn worker_profile(executable: &Path) -> Result<String> {
    let path = serde_json::to_string(
        executable
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("worker_path_utf8"))?,
    )?;
    Ok(format!(
        "(version 1)\n(deny default)\n(allow process-exec (literal {path}))\n(allow sysctl-read)\n(allow mach-lookup)\n(allow file-read-metadata)\n(allow file-read* (literal \"/\") (literal {path}) (subpath \"/System/Library\") (subpath \"/usr/lib\") (subpath \"/private/var/db/dyld\") (literal \"/dev/null\"))\n"
    ))
}
