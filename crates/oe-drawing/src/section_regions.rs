//! Section regions are evidence, independent of view kind and of the source line type.
//! GB/T 17452-1998 3.1–3.2, 6–7; GB/T 4458.6-2002 4.4, 7.1–7.8;
//! GB/T 4459.1-1995 3.5. See the five `section_*` knowledge cards.
//!
//! A hatch boundary locates a cut face; it does not establish the depth of every line
//! projected across it. Unknown origin is deliberate, never permission to erase a line.
//! Boundary evidence carries source intervals and drawing points; a centre line keeps its
//! type even when part of it carries a boundary. No member, view or expression is replaced.
use super::*;

fn push(ev: &mut Vec<String>, s: String) {
    if !ev.contains(&s) {
        ev.push(s);
    }
}

fn param(name: &str) -> Result<f64> {
    representation::param(representation::card_ref("section_region_boundary")?, name)
}

/// Only actual straight native edges. In particular, a bulged polyline's chord is
/// not a boundary. Curved/unsupported edges remain explicitly unverified.
fn straight_edges(r: &Record, arcs: &mut Vec<[f64; 5]>) -> (Vec<(P, P)>, bool) {
    let tags = &r.tags;
    let Some(start) = tags.iter().position(|t| t.0 == 91) else {
        return (vec![], true);
    };
    let end = tags
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, t)| t.0 == 75)
        .map_or(tags.len(), |(i, _)| i);
    let mut edges = Vec::new();
    let mut curved = false;
    let mut i = start + 1;
    while i < end {
        if tags[i].0 != 92 {
            i += 1;
            continue;
        }
        let flag = tags[i].1.trim().parse::<i32>().unwrap_or(0);
        i += 1;
        let z = (i..end).find(|&k| tags[k].0 == 92).unwrap_or(end);
        let path = &tags[i..z];
        if flag & 2 != 0 {
            let closed = path
                .iter()
                .find(|t| t.0 == 73)
                .is_some_and(|t| t.1.trim() == "1");
            let mut pts = Vec::new();
            let mut bulges = Vec::new();
            let mut k = 0;
            while k < path.len() {
                if path[k].0 == 10 && k + 1 < path.len() && path[k + 1].0 == 20 {
                    if let (Ok(x), Ok(y)) = (path[k].1.trim().parse(), path[k + 1].1.trim().parse())
                    {
                        pts.push([x, y]);
                        let b = path
                            .get(k + 2)
                            .filter(|t| t.0 == 42)
                            .and_then(|t| t.1.trim().parse::<f64>().ok())
                            .unwrap_or(0.);
                        bulges.push(b);
                    }
                    k += 2;
                } else {
                    k += 1;
                }
            }
            let n = pts.len();
            for j in 0..n.saturating_sub(usize::from(!closed)) {
                if bulges[j].abs() > 1e-12 {
                    curved = true;
                    if n > 1 {
                        let (a, z, bulge) = (pts[j], pts[(j + 1) % n], bulges[j]);
                        let d = sub(z, a);
                        let c = add(
                            mid(a, z),
                            mul([-d[1], d[0]], (1. - bulge * bulge) / (4. * bulge)),
                        );
                        let first = if bulge > 0. { a } else { z };
                        arcs.push([
                            c[0],
                            c[1],
                            dist(c, a),
                            (first[1] - c[1]).atan2(first[0] - c[0]),
                            4. * bulge.atan().abs(),
                        ]);
                    }
                } else if n > 1 {
                    edges.push((pts[j], pts[(j + 1) % n]));
                }
            }
        } else {
            let starts: Vec<usize> = path
                .iter()
                .enumerate()
                .filter(|(_, t)| t.0 == 72)
                .map(|(k, _)| k)
                .collect();
            for (j, &k) in starts.iter().enumerate() {
                let vals = &path[k + 1..starts.get(j + 1).copied().unwrap_or(path.len())];
                let num = |c| {
                    vals.iter()
                        .find(|t| t.0 == c)
                        .and_then(|t| t.1.trim().parse::<f64>().ok())
                };
                if path[k].1.trim() == "1" {
                    if let (Some(x), Some(y), Some(u), Some(v)) =
                        (num(10), num(20), num(11), num(21))
                    {
                        edges.push(([x, y], [u, v]));
                    }
                } else {
                    curved = true;
                    if path[k].1.trim() == "2"
                        && let (Some(x), Some(y), Some(r), Some(a), Some(z)) =
                            (num(10), num(20), num(40), num(50), num(51))
                    {
                        // DXF clockwise HATCH arc angles are complementary angles; normalize
                        // to a CCW carrier (Autodesk boundary-path fields; ezdxf ArcEdge loader).
                        let (a, z) = if num(73).is_some_and(|v| v == 0.) {
                            (-z, -a)
                        } else {
                            (a, z)
                        };
                        let sweep = if (z - a).abs() >= 360. - 1e-9 {
                            std::f64::consts::TAU
                        } else {
                            (z - a).rem_euclid(360.).to_radians()
                        };
                        arcs.push([x, y, r, a.to_radians(), sweep]);
                    }
                }
            }
        }
        i = z;
    }
    (edges, curved)
}

/// Collinear overlap in the parameter of a source segment. Point contact is not a boundary.
fn overlap(a: P, z: P, p: P, q: P, tol: f64) -> Option<[f64; 2]> {
    let len = dist(a, z);
    if len <= tol || dist(p, q) <= tol {
        return None;
    }
    let (t, d) = project(p, a, z);
    let (u, e) = project(q, a, z);
    if d.abs() > tol || e.abs() > tol {
        return None;
    }
    let span = [t.min(u).max(0.), t.max(u).min(1.)];
    ((span[1] - span[0]) * len > tol).then_some(span)
}

fn merged(mut spans: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    spans.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut out: Vec<[f64; 2]> = Vec::new();
    for s in spans {
        if let Some(last) = out.last_mut()
            && s[0] <= last[1] + 1e-9
        {
            last[1] = last[1].max(s[1]);
        } else {
            out.push(s);
        }
    }
    out
}

#[derive(Clone)]
struct Boundary {
    member: usize,
    interval: [f64; 2],
    points: [P; 2],
    parameter: &'static str,
    arc: Option<[f64; 5]>,
}

fn boundaries(
    cx: &Ctx,
    b: &Builder,
    indices: &[usize],
    edges: &[(P, P)],
    tol: f64,
) -> Vec<Boundary> {
    let mut out = Vec::new();
    for &i in indices {
        let m = &b.members[i];
        if m.member_role == "hatch_fill" {
            continue;
        }
        let mut pts = member_points(cx, m);
        // Native block paths have already been transformed. Never interpret a bulged
        // source polyline as straight segments, including legacy VERTEX records.
        let (kind, closed, bulged) = if let Some(id) = &m.native_member_id {
            match cx.blocks.part(id).map(|p| &p.geom) {
                Some(annotation_blocks::Geom::Line(_, _)) => ("LINE", false, false),
                Some(annotation_blocks::Geom::Path(_, closed)) => ("LWPOLYLINE", *closed, false),
                _ => continue,
            }
        } else {
            let (_, closed, bulged) = cx.polys.get(&m.source_handle).cloned().unwrap_or_default();
            (cx.kind(&m.source_handle), closed, bulged)
        };
        if bulged
            || !matches!(kind, "LINE" | "LWPOLYLINE" | "POLYLINE")
            || not_plotted::source(cx, &m.source_handle)
        {
            continue;
        }
        if closed && pts.len() > 2 && dist(pts[0], *pts.last().unwrap()) > tol {
            pts.push(pts[0]);
        }
        let lengths: Vec<f64> = pts.windows(2).map(|w| dist(w[0], w[1])).collect();
        let total: f64 = lengths.iter().sum();
        if total <= tol {
            continue;
        }
        let mut offset = 0.;
        for (j, w) in pts.windows(2).enumerate() {
            let spans = merged(
                edges
                    .iter()
                    .filter_map(|(p, q)| overlap(w[0], w[1], *p, *q, tol))
                    .collect(),
            );
            for s in spans {
                let local = [
                    (offset + s[0] * lengths[j]) / total,
                    (offset + s[1] * lengths[j]) / total,
                ];
                let interval = local.map(|t| m.interval[0] + t * (m.interval[1] - m.interval[0]));
                out.push(Boundary {
                    member: i,
                    interval,
                    points: [lerp(w[0], w[1], s[0]), lerp(w[0], w[1], s[1])],
                    arc: None,
                    parameter: if kind == "LINE" {
                        "source_line"
                    } else {
                        "polyline_length"
                    },
                });
            }
            offset += lengths[j];
        }
    }
    out
}

/// Exact angular overlap; no chord is used to declare circular boundary ownership.
fn arc_boundaries(
    cx: &Ctx,
    b: &Builder,
    indices: &[usize],
    arcs: &[[f64; 5]],
    tol: f64,
) -> Vec<Boundary> {
    let tau = std::f64::consts::TAU;
    let mut out = Vec::new();
    for &i in indices {
        let m = &b.members[i];
        let source = if let Some(id) = &m.native_member_id {
            cx.blocks.part(id).and_then(|p| match p.geom {
                annotation_blocks::Geom::Arc(c, r, a, sweep) => Some((c, r, a, sweep)),
                annotation_blocks::Geom::Circle(c, r) => Some((c, r, 0., tau)),
                _ => None,
            })
        } else {
            cx.arc(&m.source_handle)
                .map(|(c, r, a, sweep, _)| (c, r, a, sweep))
        };
        let Some((c, r, a, sweep)) = source else {
            continue;
        };
        if sweep <= 1e-12 || r <= tol {
            continue;
        }
        let a = a.rem_euclid(tau);
        let mut spans = Vec::new();
        for edge in arcs {
            if dist(c, [edge[0], edge[1]]) > tol || (r - edge[2]).abs() > tol {
                continue;
            }
            let start = edge[3].rem_euclid(tau);
            for shift in [-tau, 0., tau] {
                let lo = ((start + shift - a) / sweep).max(m.interval[0]);
                let hi = ((start + shift + edge[4] - a) / sweep).min(m.interval[1]);
                if (hi - lo) * r * sweep > tol {
                    spans.push([lo, hi]);
                }
            }
        }
        for interval in merged(spans) {
            let point = |t: f64| {
                [
                    c[0] + r * (a + t * sweep).cos(),
                    c[1] + r * (a + t * sweep).sin(),
                ]
            };
            out.push(Boundary {
                member: i,
                interval,
                points: [point(interval[0]), point(interval[1])],
                parameter: "source_arc",
                arc: Some([
                    c[0],
                    c[1],
                    r,
                    a + interval[0] * sweep,
                    (interval[1] - interval[0]) * sweep,
                ]),
            });
        }
    }
    out
}

fn curve_member(m: &MdExpressionMember) -> bool {
    matches!(
        m.member_role.as_str(),
        "contour_segment"
            | "hidden_segment"
            | "center_line"
            | "thread_line"
            | "hatch_fill"
            | "break_curve"
            | "unresolved_source"
    ) || matches!(
        m.line_type.as_str(),
        "thick_continuous" | "thin_continuous" | "thin_dash_dot" | "thin_dashed"
    )
}

fn origin(b: &mut Builder, i: usize, region: &str, class: &str, basis: &str) {
    let ev = &mut b.members[i].evidence_ids;
    push(ev, format!("section_region:{region}"));
    push(
        ev,
        format!(
            "section_origin_region:{}",
            serde_json::json!({"region":region,"class":class,"basis":basis})
        ),
    );
}

/// Add source-preserving, region-scoped facts after local-section topology has been read.
pub(super) fn state(cx: &Ctx, b: &mut Builder, views: &[Region]) -> Result<()> {
    let hs = section_hatching::hatches(cx, b)?;
    let tol = cx.scale * param("coincidence_tolerance")?;
    let margin = cx.scale * param("context_margin")?;
    let compact = param("embedded_extent_max")?;
    let widths = line_evidence::Widths::of(cx);
    let expr_kind: BTreeMap<String, String> = b
        .exprs
        .iter()
        .map(|e| (e.id.clone(), e.kind.clone()))
        .collect();
    let points: Vec<Vec<P>> = b.members.iter().map(|m| member_points(cx, m)).collect();
    for h in hs {
        let eid = b.exprs[h.expr].id.clone();
        let view = views.iter().find(|v| Some(&v.id) == h.view.as_ref());
        let native: Vec<&Record> = b
            .members
            .iter()
            .filter(|m| m.expression_id == eid && m.native_member_id.is_none())
            .filter_map(|m| cx.rec.get(&m.source_handle).copied())
            .filter(|r| r.kind == "HATCH")
            .collect();
        let mut edges = Vec::new();
        let mut arcs = Vec::new();
        let mut incomplete = native.is_empty();
        for r in native {
            let (es, curved) = straight_edges(r, &mut arcs);
            edges.extend(es);
            incomplete |= curved;
        }
        let Some(bb) = expr_bbox(cx, b, h.expr) else {
            continue;
        };
        let indices: Vec<usize> = b
            .members
            .iter()
            .enumerate()
            .filter(|(i, m)| {
                m.view_id == h.view
                    && curve_member(m)
                    && bbox_of(&points[*i]).is_some_and(|mb| bbox_gap(bb, mb) <= margin)
            })
            .map(|(i, _)| i)
            .collect();
        let mut bs = boundaries(cx, b, &indices, &edges, tol);
        bs.extend(arc_boundaries(cx, b, &indices, &arcs, tol));
        let boundary_members: BTreeSet<usize> = bs.iter().map(|s| s.member).collect();
        let profile = indices.iter().any(|&i| {
            b.members[i]
                .evidence_ids
                .iter()
                .any(|s| s.starts_with("section_profile:"))
        });
        let detail = view.is_some_and(|v| v.kind == "detail_view");
        let whole_section = view.is_some_and(|v| {
            v.evidence
                .iter()
                .any(|s| s.starts_with("section_type:") || s == "view_type:section")
        });
        let embedded = view.is_some_and(|v| {
            (bb[2] - bb[0]).hypot(bb[3] - bb[1])
                < compact * (v.bbox[2] - v.bbox[0]).hypot(v.bbox[3] - v.bbox[1])
        });
        let crossing: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| {
                let m = &b.members[i];
                expr_kind[&m.expression_id] == "visible_contour"
                    && !boundary_members.contains(&i)
                    && points[i].len() == 2
                    && !edges.is_empty()
                    && !representation::inside_segments(points[i][0], &edges)
                    && !representation::inside_segments(points[i][1], &edges)
                    && (1..16).any(|k| {
                        representation::inside_segments(
                            lerp(points[i][0], points[i][1], k as f64 / 16.),
                            &edges,
                        )
                    })
            })
            .collect();
        let thin_boundary = !bs.is_empty()
            && !incomplete
            && edges.iter().all(|(a, z)| {
                let witnesses: Vec<(P, P)> = bs
                    .iter()
                    .filter(|s| {
                        widths.weight(&b.members[s.member].source_handle)
                            == Some(line_evidence::Width::Thin)
                    })
                    .map(|s| (s.points[0], s.points[1]))
                    .collect();
                section_hatching::shared_length(&[(*a, *z)], &witnesses, tol) >= dist(*a, *z) - tol
            });
        // A compact, named, separate cut with no projected interior structure is a removed
        // section candidate. Do not decide from a hatch or from a small bounding box alone.
        let contour_indices: Vec<usize> = b
            .members
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                m.view_id == h.view && expr_kind[&m.expression_id] == "visible_contour"
            })
            .map(|(i, _)| i)
            .collect();
        let only_face = !contour_indices.is_empty()
            && contour_indices.iter().all(|i| boundary_members.contains(i));
        let cutting_reference = b.exprs.iter().any(|e| {
            h.view
                .as_ref()
                .is_some_and(|v| e.evidence_ids.contains(&format!("references_view:{v}")))
                && e.member_ids.iter().any(|id| {
                    b.members
                        .iter()
                        .any(|m| &m.id == id && m.member_role == "cutting_stroke")
                })
        });
        let (ctype, concept, reason) = if !detail
            && !profile
            && embedded
            && !whole_section
            && thin_boundary
            && !crossing.is_empty()
        {
            (
                "superimposed",
                "DC-G2-034",
                "thin_closed_boundary_and_continuing_parent_outline",
            )
        } else if !detail && !profile && !incomplete && only_face && cutting_reference {
            (
                "removed",
                "DC-G2-028",
                "separate_named_cut_only_face_boundary",
            )
        } else {
            (
                "unknown",
                if embedded { "DC-G2-034" } else { "DC-G2-028" },
                "placement_width_or_cut_depth_not_established",
            )
        };
        let ev = &mut b.exprs[h.expr].evidence_ids;
        for card in [
            "section_region_boundary",
            "section_line_origin",
            "section_extent_context",
            "section_line_exceptions",
        ] {
            push(ev, format!("card:{card}"));
        }
        push(ev, "card:section_cross_section_type".into());
        push(ev, format!("concept:{concept}"));
        push(ev, format!("cross_section_type:{ctype}"));
        push(ev, format!("cross_section_basis:{reason}"));
        if ctype == "unknown" {
            push(ev, "cross_section_concepts:candidates_not_confirmed".into());
        }
        push(
            ev,
            format!(
                "section_boundary_geometry:{}",
                if incomplete {
                    "partial_or_unavailable"
                } else {
                    "native_straight_edges"
                }
            ),
        );
        push(
            ev,
            format!("section_boundary_member_count:{}", boundary_members.len()),
        );
        push(
            ev,
            format!(
                "section_context:{}",
                if detail {
                    "detail_independent_of_cut_type"
                } else if profile {
                    "local"
                } else {
                    "view_evidence_independent_of_cut_type"
                }
            ),
        );
        if let Some(v) = view {
            for s in &v.evidence {
                if s.starts_with("section_type:") {
                    push(ev, format!("section_context:{s}"));
                }
            }
        }
        push(ev, "section_exception:longitudinal_rib_spoke_shaft_requires_orientation:GB/T_4458.6-2002_4.4".into());
        for s in &bs {
            let m = &mut b.members[s.member];
            let carrier = if m.line_type.contains("dash_dot")
                || expr_kind[&m.expression_id] == "center_axis"
            {
                "center_line"
            } else {
                "source_line"
            };
            push(
                &mut m.evidence_ids,
                format!(
                    "section_boundary:{}",
                    serde_json::json!({"region":eid,"interval":s.interval,"parameter":s.parameter,"points":s.points,"carrier":carrier,"arc":s.arc})
                ),
            );
        }
        for i in indices {
            let m = &b.members[i];
            let (class, basis) = if expr_kind[&m.expression_id] == "thread_convention"
                || m.member_role == "thread_line"
            {
                ("pre_existing", "thread_line_retained_GB/T_4459.1-1995_3.5")
            } else if expr_kind[&m.expression_id] == "center_axis" {
                ("pre_existing", "reference_axis_is_not_a_cut_edge")
            } else if m.member_role == "hatch_fill" && m.expression_id == eid {
                ("cut_formed", "cut_face_symbol_not_a_physical_edge")
            } else if ctype == "superimposed" && boundary_members.contains(&i) {
                ("cut_formed", "rotated_cut_face_boundary")
            } else if ctype == "superimposed" && crossing.contains(&i) {
                (
                    "pre_existing",
                    "parent_outline_continues_GB/T_4458.6-2002_7.8",
                )
            } else {
                (
                    "unknown",
                    "projection_alone_does_not_establish_cut_depth_or_suppression",
                )
            };
            origin(b, i, &eid, class, basis);
        }
    }
    // An unhatched detail is not automatically a section (ribs/shafts may lawfully be
    // unsectioned). Keep it as a candidate with unknown origin, without creating a hatch.
    for v in views.iter().filter(|v| v.kind == "detail_view") {
        if b.exprs
            .iter()
            .any(|e| e.view_id.as_ref() == Some(&v.id) && e.kind == "section_hatch")
        {
            continue;
        }
        for i in 0..b.members.len() {
            if b.members[i].view_id.as_ref() == Some(&v.id) && curve_member(&b.members[i]) {
                origin(
                    b,
                    i,
                    &v.id,
                    "unknown",
                    "unhatched_detail_not_proof_of_a_cut",
                );
            }
        }
    }
    for m in &mut b.members {
        let classes: BTreeSet<String> = m
            .evidence_ids
            .iter()
            .filter_map(|e| e.strip_prefix("section_origin_region:"))
            .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .filter_map(|v| v["class"].as_str().map(str::to_owned))
            .collect();
        if !classes.is_empty() {
            push(
                &mut m.evidence_ids,
                format!(
                    "section_origin:{}",
                    if classes.len() == 1 {
                        classes.first().unwrap().as_str()
                    } else {
                        "unknown"
                    }
                ),
            );
        }
    }
    Ok(())
}
