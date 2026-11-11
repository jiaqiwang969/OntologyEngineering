//! Source-bound constituent geometry, independent of the recognition repairs.
//! Only two new relations are emitted. Existing objects, classes and predicates
//! are untouched; the ontology decides whether a relation is a reading deviation.
use super::{Dxf, Object, P, Relation, TextBox, dist, list, point_line, point_segment};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;

struct Parameters {
    incidence: f64,
    direction: f64,
    base_min: f64,
    base_max: f64,
    symmetry: f64,
    text_band: f64,
    text_carrier_gap: f64,
    text_end: f64,
    text_gap: f64,
    arc_end: f64,
}
impl Parameters {
    fn load() -> Result<Self> {
        let doc: toml::Value = toml::from_str(super::KNOWLEDGE)?;
        let table = &doc["constituents"]["parameters"];
        let value = |key: &str| -> Result<f64> {
            let x = table
                .get(key)
                .and_then(toml::Value::as_float)
                .with_context(|| format!("constituent_parameter:{key}"))?;
            ensure!(x.is_finite() && x > 0., "constituent_parameter_range:{key}");
            Ok(x)
        };
        Ok(Self {
            incidence: value("incidence_arrow")?,
            direction: value("direction_sine")?,
            base_min: value("base_min_arrow")?,
            base_max: value("base_max_arrow")?,
            symmetry: value("base_symmetry_arrow")?,
            text_band: value("text_band_height")?,
            text_carrier_gap: value("text_carrier_gap_height")?,
            text_end: value("text_end_height")?,
            text_gap: value("text_gap_height")?,
            arc_end: value("arc_end_arrow")?,
        })
    }
}

#[derive(Clone, Copy)]
struct Head {
    tip: P,
    base: P,
    length: f64,
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}
fn dot(a: P, b: P) -> f64 {
    a.0 * b.0 + a.1 * b.1
}
fn cross(a: P, b: P) -> f64 {
    a.0 * b.1 - a.1 * b.0
}
fn parallel(a: P, b: P, c: P, d: P, sine: f64) -> bool {
    let lengths = dist(a, b) * dist(c, d);
    lengths > 0. && cross(sub(b, a), sub(d, c)).abs() <= sine * lengths
}
fn same_view(a: &Object, b: &Object) -> bool {
    a.view.as_deref().is_some_and(|v| !v.is_empty()) && a.view == b.view
}

/// Three distinct vertices, with one short base and a symmetric sharp apex.
/// Four-sided fills and curved/ambiguous polyline contours are not terminals.
fn triangle(mut pts: Vec<P>, p: &Parameters) -> Option<Head> {
    if pts.iter().any(|q| !q.0.is_finite() || !q.1.is_finite()) {
        return None;
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    pts.dedup();
    if pts.len() != 3 {
        return None;
    }
    let mut candidates = Vec::new();
    for i in 0..3 {
        let (tip, a, b) = (pts[i], pts[(i + 1) % 3], pts[(i + 2) % 3]);
        let base = ((a.0 + b.0) / 2., (a.1 + b.1) / 2.);
        let length = dist(tip, base);
        let width = dist(a, b);
        if length > 0.
            && width >= p.base_min * length
            && width <= p.base_max * length
            && (dist(tip, a) - dist(tip, b)).abs() <= p.symmetry * length
        {
            candidates.push(Head { tip, base, length });
        }
    }
    (candidates.len() == 1).then(|| candidates[0])
}

/// Read the member's source, including the DIMENSION display-block offset.
fn source<'a>(
    o: &Object,
    members: &BTreeMap<&str, &Value>,
    d: &Dxf<'a>,
) -> Option<(&'a super::dxf::Record, P)> {
    let m = members.get(o.member_ids.first()?.as_str())?;
    let root = m.get("source_handle")?.as_str()?;
    let native = m
        .get("native_member_id")
        .and_then(Value::as_str)
        .unwrap_or(root);
    let handle = native.rsplit('/').next()?;
    let record = *d.by_handle.get(handle)?;
    let offset = if handle != root && d.by_handle.get(root)?.kind == "DIMENSION" {
        let dim = d.by_handle[root];
        (dim.float(12).unwrap_or(0.), dim.float(22).unwrap_or(0.))
    } else {
        (0., 0.)
    };
    Some((record, offset))
}

fn head(o: &Object, members: &BTreeMap<&str, &Value>, d: &Dxf, p: &Parameters) -> Option<Head> {
    let (r, offset) = source(o, members, d)?;
    let pt = |x, y| Some((r.float(x).ok()? + offset.0, r.float(y).ok()? + offset.1));
    let points = match r.kind.as_str() {
        "SOLID" => vec![pt(10, 20)?, pt(11, 21)?, pt(12, 22)?, pt(13, 23)?],
        "LWPOLYLINE" | "POLYLINE" => {
            if r.tag(70)?.parse::<i64>().ok()? & 1 != 1
                || r.tags
                    .iter()
                    .any(|(c, v)| *c == 42 && v.parse::<f64>().unwrap_or(1.) != 0.)
            {
                return None;
            }
            let (chains, arcs) = d.chains(r, offset).ok()?;
            if !arcs.is_empty() || chains.len() != 1 {
                return None;
            }
            chains.into_iter().next()?
        }
        _ => return None,
    };
    triangle(points, p)
}

/// A missing terminal may be stored as one of its straight base/tail pieces
/// instead of a filled triangle. Reuse the nearby triangle's geometry only
/// when the candidate is a short, attached component; an arbitrary line is
/// never promoted to a terminal.
fn component_head(
    candidate: &Object,
    candidate_head: Option<Head>,
    heads: &BTreeMap<usize, Head>,
    objects: &[Object],
    p: &Parameters,
) -> Option<Head> {
    if candidate_head.is_some() {
        return candidate_head;
    }
    let (a, b) = candidate.segment()?;
    heads.iter().find_map(|(i, h)| {
        let other = &objects[*i];
        if !same_view(candidate, other) {
            return None;
        }
        let tol = p.incidence * h.length;
        let length = dist(a, b);
        if length <= 0. || length > 2.5 * h.length {
            return None;
        }
        // A base edge crosses the head axis at its midpoint; a tail follows
        // that axis from the tip. Both forms are finite terminal components.
        let base_edge = length <= p.base_max * h.length
            && point_segment(h.base, a, b).0 <= tol
            && !parallel(a, b, h.base, h.tip, p.direction);
        let tail = [a, b].iter().any(|q| dist(*q, h.tip) <= tol)
            && parallel(a, b, h.base, h.tip, p.direction)
            && dot(sub(mid(a, b), h.tip), sub(h.tip, h.base)) >= -tol;
        (base_edge || tail).then_some(*h)
    })
}

fn mid(a: P, b: P) -> P {
    ((a.0 + b.0) / 2., (a.1 + b.1) / 2.)
}

/// Finite circle/arc support at the tip, on the radial axis of this head.
fn radial_support(h: Head, carrier: &Object, objects: &[Object], p: &Parameters) -> bool {
    let tol = p.incidence * h.length;
    objects
        .iter()
        // A radius may be dimensioned on visible, hidden, or centre-line
        // circles/arcs.  Annotation lines and section/hatch lines are not
        // finite profile support.
        .filter(|o| {
            (o.is("VisibleOutline") || o.is("HiddenOutline") || o.is("CentreLine"))
                && same_view(o, carrier)
        })
        .any(|o| {
            o.geometry.arcs.iter().any(|(centre, radius)| {
                (dist(h.tip, *centre) - radius).abs() <= tol
                    && parallel(h.base, h.tip, *centre, h.tip, p.direction)
                    && o.geometry.chains.iter().any(|c| {
                        c.windows(2).any(|w| {
                            // Sampling is only the finite-span gate; radial incidence is analytic above.
                            point_segment(h.tip, w[0], w[1]).0
                                <= tol + dist(w[0], w[1]).powi(2) / (2. * radius)
                        })
                    })
            })
        })
}

fn head_fits(h: Head, carrier: &Object, objects: &[Object], p: &Parameters) -> bool {
    let tol = p.incidence * h.length;
    if carrier.expression_kind.as_deref() == Some("dimension_angle") {
        return carrier.geometry.arcs.iter().any(|(c, r)| {
            let radial = sub(h.tip, *c);
            let tangent = (-radial.1, radial.0);
            (dist(h.tip, *c) - r).abs() <= tol
                && parallel(h.base, h.tip, (0., 0.), tangent, p.direction)
                && carrier.geometry.chains.iter().any(|chain| {
                    [chain.first(), chain.last()]
                        .into_iter()
                        .flatten()
                        .any(|end| dist(*end, h.tip) <= p.arc_end * h.length)
                })
        });
    }
    let Some((a, b)) = carrier.segment() else {
        return false;
    };
    let endpoint_near = point_segment(h.tip, a, b).0 <= tol || point_segment(h.base, a, b).0 <= tol;
    let endpoint_inside_arrow = [a, b].iter().any(|q| {
        let (off, t) = point_line(*q, h.tip, h.base);
        off <= tol && t >= -tol / h.length && t <= 1. + tol / h.length
    });
    parallel(a, b, h.base, h.tip, p.direction)
        && point_line(h.tip, a, b).0 <= tol
        // CAD dimensions may stop inside the filled arrow.  The endpoint is
        // accepted only on the existing arrow axis and within its tip/base
        // interval; no additional distance parameter is introduced.
        && (endpoint_near || endpoint_inside_arrow)
        && (carrier.expression_kind.as_deref() != Some("dimension_radius")
            || radial_support(h, carrier, objects, p))
}

/// A source interval is not missing when another member already covers that
/// same source interval. A different source handle is not silently discarded.
fn covered(candidate: &Object, carriers: &[usize], objects: &[Object], tol: f64) -> bool {
    let Some((a, b)) = candidate.segment() else {
        return false;
    };
    let length = dist(a, b);
    if length <= 0. {
        return true;
    }
    let mut intervals = Vec::new();
    for &i in carriers {
        let o = &objects[i];
        if !candidate
            .source_handles
            .iter()
            .any(|h| o.source_handles.contains(h))
        {
            continue;
        }
        if let Some((c, d)) = o.segment() {
            let (off_c, tc) = point_line(c, a, b);
            let (off_d, td) = point_line(d, a, b);
            if off_c <= tol && off_d <= tol {
                intervals.push((tc.min(td), tc.max(td)));
            }
        }
    }
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut end = 0.;
    for (lo, hi) in intervals {
        if lo > end + tol / length {
            break;
        }
        end = f64::max(end, hi);
    }
    end >= 1. - tol / length
}

fn text_span(a: P, b: P, tb: TextBox, p: &Parameters) -> bool {
    let (a, b) = (tb.local(a), tb.local(b));
    let (lo, hi) = (a.0.min(b.0), a.0.max(b.0));
    (a.1 - b.1).abs() <= p.direction * dist(a, b)
        // GB/T 14665-2012 5.7 表4 gives a 1 mm minimum to a reference or
        // baseline; with the 3.5 mm A2--A4 alphanumeric height from Table 3,
        // the supported ratio is 2/7. The text box's lower band is not the
        // glyph edge, so add that ratio to the measured 0.3h band.
        && a.1.abs() <= (p.text_band + p.text_carrier_gap) * tb.height
        && b.1.abs() <= (p.text_band + p.text_carrier_gap) * tb.height
        && lo <= 0.
        && hi >= tb.width
        && lo >= -p.text_gap * tb.height
        && hi <= tb.width + p.text_end * tb.height
}

fn crosses_stop(
    a: P,
    b: P,
    candidate: &Object,
    objects: &[Object],
    heads: &BTreeMap<usize, Head>,
    tol: f64,
) -> bool {
    let len = dist(a, b);
    let interior = |q: P| {
        let (off, t) = point_line(q, a, b);
        off <= tol && t > tol / len && t < 1. - tol / len
    };
    heads
        .iter()
        .any(|(i, h)| same_view(candidate, &objects[*i]) && interior(h.tip))
        || objects.iter().any(|o| {
            if o.id == candidate.id || !same_view(o, candidate) {
                return false;
            }
            o.segment().is_some_and(|(c, d)| {
                !parallel(a, b, c, d, tol / len) && (interior(c) || interior(d))
            })
        })
}

fn continuation(
    candidate: &Object,
    carrier: &Object,
    own_heads: &[Head],
    texts: &[TextBox],
    p: &Parameters,
) -> bool {
    let (Some((a, b)), Some((c, d))) = (candidate.segment(), carrier.segment()) else {
        return false;
    };
    own_heads.iter().any(|h| {
        let tol = p.incidence * h.length;
        if !parallel(a, b, c, d, p.direction)
            || point_line(a, c, d).0 > tol
            || point_line(b, c, d).0 > tol
        {
            return false;
        }
        // The finite fill between this dimension's head base and tip bridges a
        // missing root-line interval, including a native display-block head.
        let in_fill = point_segment(a, h.base, h.tip).0 <= tol
            && point_segment(b, h.base, h.tip).0 <= tol
            && [a, b].iter().any(|q| point_segment(*q, c, d).0 <= tol);
        // Outside text needs its own terminal at the joint, a finite text
        // footprint and endpoint connection; no infinite-line or nearest match.
        let at_joint = [a, b]
            .iter()
            .any(|q| dist(*q, h.tip) <= tol || dist(*q, h.base) <= tol);
        let carries_text = at_joint && texts.iter().any(|t| text_span(a, b, *t, p));
        in_fill || carries_text
    })
}

pub(super) fn relations(g: &Value, d: &Dxf, objects: &[Object]) -> Result<Vec<Relation>> {
    let p = Parameters::load()?;
    let members: BTreeMap<&str, &Value> = list(g, "members")?
        .iter()
        .filter_map(|m| Some((m.get("id")?.as_str()?, m)))
        .collect();
    let heads: BTreeMap<usize, Head> = objects
        .iter()
        .enumerate()
        .filter_map(|(i, o)| head(o, &members, d, &p).map(|h| (i, h)))
        .collect();
    let mut dimensions: BTreeMap<(&str, &str), Vec<usize>> = BTreeMap::new();
    for (i, o) in objects.iter().enumerate() {
        let radius_leader =
            o.is("LeaderLine") && o.expression_kind.as_deref() == Some("dimension_radius");
        if (o.is("DimensionLine") || radius_leader)
            && let (Some(e), Some(v)) = (o.expression_id.as_deref(), o.view.as_deref())
            && !v.is_empty()
            && o.expression_kind
                .as_deref()
                .is_some_and(|k| k.starts_with("dimension_"))
        {
            dimensions.entry((e, v)).or_default().push(i);
        }
    }
    for group in dimensions.values_mut() {
        group.sort_by(|a, b| objects[*a].id.cmp(&objects[*b].id));
    }
    let mut out = Vec::new();
    for (i, candidate) in objects.iter().enumerate() {
        if candidate.expression_kind.as_deref() != Some("unresolved") || !candidate.is("OtherLine")
        {
            continue;
        }
        let mut terminal_targets = Vec::new();
        let mut continuation_targets = Vec::new();
        for (&(expression, _view), carriers) in &dimensions {
            let first = &objects[carriers[0]];
            if !same_view(candidate, first) {
                continue;
            }
            let texts: Vec<TextBox> = objects
                .iter()
                .filter(|o| {
                    o.expression_id.as_deref() == Some(expression)
                        && same_view(o, first)
                        && o.is("DimensionText")
                })
                .filter_map(|o| o.geometry.text_box)
                .collect();
            if texts.is_empty() {
                continue;
            }
            let candidate_head =
                component_head(candidate, heads.get(&i).copied(), &heads, objects, &p);
            if let Some(h) = candidate_head
                && let Some(&target) = carriers
                    .iter()
                    .find(|&&j| head_fits(h, &objects[j], objects, &p))
            {
                terminal_targets.push(target);
            }
            let own_heads: Vec<Head> = heads
                .iter()
                .filter(|(j, _)| {
                    objects[**j].is("Arrowhead")
                        && objects[**j].expression_id.as_deref() == Some(expression)
                        && same_view(&objects[**j], first)
                })
                .map(|(_, h)| *h)
                .filter(|h| {
                    carriers
                        .iter()
                        .any(|&j| head_fits(*h, &objects[j], objects, &p))
                })
                .collect();
            let scale = own_heads.iter().map(|h| h.length).reduce(f64::min);
            let Some(scale) = scale else {
                continue;
            };
            let tol = p.incidence * scale;
            let Some((a, b)) = candidate.segment() else {
                continue;
            };
            if dist(a, b) <= 0.
                || source(candidate, &members, d).is_none_or(|(r, _)| r.kind != "LINE")
                || covered(candidate, carriers, objects, tol)
                || crosses_stop(a, b, candidate, objects, &heads, tol)
            {
                continue;
            }
            if let Some(&target) = carriers
                .iter()
                .find(|&&j| continuation(candidate, &objects[j], &own_heads, &texts, &p))
            {
                continuation_targets.push(target);
            }
        }
        // One dimension may have several carriers. Its canonical eligible
        // member was selected above; two different owners remain unknown.
        if terminal_targets.len() == 1 {
            out.push(Relation::node(
                i,
                "terminatorOnCarrier",
                terminal_targets[0],
            ));
        }
        if continuation_targets.len() == 1 {
            out.push(Relation::node(
                i,
                "boundedContinuationOf",
                continuation_targets[0],
            ));
        }
    }
    out.sort_by(|a, b| {
        (
            objects[a.subject].id.as_str(),
            a.predicate,
            objects[a.object.unwrap()].id.as_str(),
        )
            .cmp(&(
                objects[b.subject].id.as_str(),
                b.predicate,
                objects[b.object.unwrap()].id.as_str(),
            ))
    });
    Ok(out)
}
