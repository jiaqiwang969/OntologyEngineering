//! Bounded illustration geometry. Projection adapted from Anatomy bc943e4,
//! Copyright (c) 2026 Ryan, MIT (docs/third-party/ANATOMY-LICENSE.txt).
//! No native CAD, continuous collision, or manufacturing acceptance is implied.
use anyhow::{Result, ensure};
use oe_contracts::{PresentationScene1, PresentationState1};
use std::collections::{BTreeMap, BTreeSet};

pub struct PlacedPart {
    pub index: usize,
    pub origin: [f64; 3],
    pub size: [f64; 3],
}
pub struct Frame {
    pub parts: Vec<PlacedPart>,
}
pub struct Evaluation {
    pub frames: Vec<Frame>,
    pub pair_check_count: u64,
    pub possible_overlaps: Vec<String>,
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 240 && !s.chars().any(char::is_control)
}
fn bounded(x: f64) -> bool {
    x.is_finite() && x.abs() <= 1_000_000.0
}
fn vector(v: &[f64]) -> bool {
    v.len() == 3 && v.iter().copied().all(bounded)
}

/// Fixed 45°/30° camera: exactly the upstream iso-kit scale-one projection.
pub fn project([x, y, z]: [f64; 3]) -> [f64; 2] {
    let a = 45_f64.to_radians();
    let e = 30_f64.to_radians();
    let k = 1.6_f64.sqrt();
    [
        (x * a.sin() - y * a.cos()) * k,
        ((x * a.cos() + y * a.sin()) * e.sin() - z * e.cos()) * k,
    ]
}
fn depth([x, y, z]: [f64; 3]) -> f64 {
    (x + y) * 0.5_f64.sqrt() * 30_f64.to_radians().cos() + z * 0.5
}
fn values(state: &PresentationState1) -> BTreeMap<&str, f64> {
    state
        .values
        .iter()
        .map(|v| (v.parameter_id.as_str(), v.value))
        .collect()
}

pub fn evaluate(scene: &PresentationScene1) -> Result<Evaluation> {
    scene
        .validate()
        .map_err(|e| anyhow::anyhow!("presentation_contract:{e:?}"))?;
    ensure!(
        (1..=64).contains(&scene.parts.len())
            // A CAD snapshot may be intentionally static; zero parameters is
            // a valid finite scene and avoids inventing a motion control.
            && (0..=8).contains(&scene.parameters.len())
            && (1..=32).contains(&scene.states.len()),
        "presentation_budget"
    );
    let mut parameters = BTreeMap::new();
    for p in &scene.parameters {
        p.validate()
            .map_err(|e| anyhow::anyhow!("presentation_parameter_contract:{e:?}"))?;
        ensure!(
            identifier(&p.id)
                && text(&p.label)
                && p.unit == scene.unit
                && bounded(p.minimum)
                && bounded(p.maximum)
                && p.minimum <= p.maximum
                && parameters.insert(p.id.as_str(), p).is_none(),
            "presentation_parameter_invalid"
        );
    }
    let mut ids = BTreeSet::new();
    let mut used = BTreeSet::new();
    for p in &scene.parts {
        p.validate()
            .map_err(|e| anyhow::anyhow!("presentation_part_contract:{e:?}"))?;
        ensure!(
            identifier(&p.id)
                && ids.insert(&p.id)
                && text(&p.label)
                && vector(&p.origin)
                && vector(&p.size)
                && p.size.iter().all(|x| *x >= 0.000_001)
                && vector(&p.axis),
            "presentation_part_invalid"
        );
        ensure!(
            p.primitive != "cylinder_z" || p.size[0] == p.size[1],
            "presentation_cylinder_diameter"
        );
        match &p.parameter_id {
            Some(id) => {
                ensure!(
                    parameters.contains_key(id.as_str())
                        && (p.axis.iter().map(|x| x * x).sum::<f64>() - 1.0).abs() < 1e-12,
                    "presentation_motion_axis"
                );
                used.insert(id.as_str());
            }
            None => ensure!(p.axis.iter().all(|x| *x == 0.0), "presentation_static_axis"),
        }
    }
    ensure!(
        used.len() == parameters.len(),
        "presentation_unused_parameter"
    );
    let mut state_ids = BTreeSet::new();
    let mut frames = Vec::new();
    let mut possible_overlaps = Vec::new();
    let mut pair_check_count = 0;
    for s in &scene.states {
        s.validate()
            .map_err(|e| anyhow::anyhow!("presentation_state_contract:{e:?}"))?;
        for v in &s.values {
            v.validate()
                .map_err(|e| anyhow::anyhow!("presentation_value_contract:{e:?}"))?;
        }
        ensure!(
            identifier(&s.id) && text(&s.label) && state_ids.insert(&s.id),
            "presentation_state_identity"
        );
        let vals = values(s);
        ensure!(
            vals.len() == s.values.len()
                && vals.len() == parameters.len()
                && vals.keys().eq(parameters.keys()),
            "presentation_state_coverage"
        );
        for (id, v) in &vals {
            let p = parameters[id];
            ensure!(
                v.is_finite() && *v >= p.minimum && *v <= p.maximum,
                "presentation_state_range"
            );
        }
        let mut parts = Vec::new();
        for (index, p) in scene.parts.iter().enumerate() {
            let offset = p.parameter_id.as_ref().map_or(0.0, |id| vals[id.as_str()]);
            let origin = std::array::from_fn(|i| p.origin[i] + p.axis[i] * offset);
            ensure!(
                origin
                    .iter()
                    .enumerate()
                    .all(|(i, x)| bounded(*x) && bounded(*x + p.size[i])),
                "presentation_transformed_bounds"
            );
            parts.push(PlacedPart {
                index,
                origin,
                size: [p.size[0], p.size[1], p.size[2]],
            });
        }
        for i in 0..parts.len() {
            for j in i + 1..parts.len() {
                pair_check_count += 1;
                let a = &parts[i];
                let b = &parts[j];
                if (0..3).all(|k| {
                    a.origin[k].max(b.origin[k])
                        < (a.origin[k] + a.size[k]).min(b.origin[k] + b.size[k])
                }) {
                    possible_overlaps.push(format!(
                        "{}:{}:{}",
                        s.id, scene.parts[i].id, scene.parts[j].id
                    ));
                }
            }
        }
        frames.push(Frame { parts });
    }
    Ok(Evaluation {
        frames,
        pair_check_count,
        possible_overlaps,
    })
}
fn corners(p: &PlacedPart) -> Vec<[f64; 3]> {
    (0..8)
        .map(|i| {
            std::array::from_fn(|k| p.origin[k] + if i & (1 << k) == 0 { 0.0 } else { p.size[k] })
        })
        .collect()
}
/// Reader-facing unit label; the source contract retains its exact unit identifier.
pub fn unit_label(unit: &str) -> &str {
    if unit == "illustration_unit" {
        "示意单位"
    } else {
        unit
    }
}

// Positive-axis camera: an unopposed separating plane establishes a part order.
// Ambiguous pairs and cycles fall back to center depth, so this is not a general occlusion proof.
fn part_order(frame: &Frame) -> Vec<usize> {
    let n = frame.parts.len();
    let mut edges = vec![Vec::new(); n];
    let mut incoming = vec![0usize; n];
    for i in 0..n {
        for j in i + 1..n {
            let a = &frame.parts[i];
            let b = &frame.parts[j];
            let ab = (0..3).any(|k| a.origin[k] + a.size[k] <= b.origin[k]);
            let ba = (0..3).any(|k| b.origin[k] + b.size[k] <= a.origin[k]);
            if ab && !ba {
                edges[i].push(j);
                incoming[j] += 1;
            }
            if ba && !ab {
                edges[j].push(i);
                incoming[i] += 1;
            }
        }
    }
    let center_depth = |i: usize| {
        let p = &frame.parts[i];
        depth(std::array::from_fn(|k| p.origin[k] + p.size[k] * 0.5))
    };
    let mut remaining: BTreeSet<_> = (0..n).collect();
    let mut order = Vec::new();
    while !remaining.is_empty() {
        let choose = |eligible: Vec<usize>| {
            eligible
                .into_iter()
                .min_by(|a, b| center_depth(*a).total_cmp(&center_depth(*b)).then(a.cmp(b)))
        };
        let next = choose(
            remaining
                .iter()
                .copied()
                .filter(|i| incoming[*i] == 0)
                .collect(),
        )
        .or_else(|| choose(remaining.iter().copied().collect()))
        .expect("nonempty remaining parts");
        remaining.remove(&next);
        order.push(next);
        for &j in &edges[next] {
            incoming[j] = incoming[j].saturating_sub(1);
        }
    }
    let mut ranks = vec![0; n];
    for (rank, index) in order.into_iter().enumerate() {
        ranks[index] = rank;
    }
    ranks
}
struct Face {
    points: Vec<[f64; 3]>,
    fill: &'static str,
    part: usize,
}
fn faces(scene: &PresentationScene1, frame: &Frame) -> Vec<Face> {
    let mut result = Vec::new();
    for p in &frame.parts {
        let moving = scene.parts[p.index].parameter_id.is_some();
        let palette = if moving {
            ["#4f7eac", "#6c9ac2", "#a8cce5"]
        } else {
            ["#a4b2bb", "#bbc7cb", "#dce4e4"]
        };
        if scene.parts[p.index].primitive == "box" {
            let c = corners(p);
            for (indices, fill) in [
                ([1, 3, 7, 5], palette[0]),
                ([2, 6, 7, 3], palette[1]),
                ([4, 5, 7, 6], palette[2]),
            ] {
                result.push(Face {
                    points: indices.map(|i| c[i]).to_vec(),
                    fill,
                    part: p.index,
                });
            }
        } else {
            let ring: Vec<[f64; 3]> = (0..48)
                .map(|i| {
                    let a = i as f64 * std::f64::consts::TAU / 48.0;
                    [
                        p.origin[0] + p.size[0] * 0.5 * (1.0 + a.cos()),
                        p.origin[1] + p.size[1] * 0.5 * (1.0 + a.sin()),
                        p.origin[2],
                    ]
                })
                .collect();
            for i in 0..48 {
                let j = (i + 1) % 48;
                let a = (i as f64 + 0.5) * std::f64::consts::TAU / 48.0;
                if a.cos() + a.sin() > 0.0 {
                    let mut top_i = ring[i];
                    top_i[2] += p.size[2];
                    let mut top_j = ring[j];
                    top_j[2] += p.size[2];
                    result.push(Face {
                        points: vec![ring[i], ring[j], top_j, top_i],
                        fill: if a.cos() > a.sin() {
                            palette[0]
                        } else {
                            palette[1]
                        },
                        part: p.index,
                    });
                }
            }
            result.push(Face {
                points: ring
                    .into_iter()
                    .map(|mut x| {
                        x[2] += p.size[2];
                        x
                    })
                    .collect(),
                fill: palette[2],
                part: p.index,
            });
        }
    }
    let ranks = part_order(frame);
    result.sort_by(|a, b| {
        let d = |f: &Face| f.points.iter().map(|p| depth(*p)).sum::<f64>() / f.points.len() as f64;
        ranks[a.part]
            .cmp(&ranks[b.part])
            .then_with(|| d(a).total_cmp(&d(b)))
    });
    result
}

/// All frames share one camera fit. Face-depth ordering is a display heuristic.
pub fn render(scene: &PresentationScene1, evaluation: &Evaluation, at: usize) -> String {
    let points: Vec<_> = evaluation
        .frames
        .iter()
        .flat_map(|f| &f.parts)
        .flat_map(corners)
        .map(project)
        .collect();
    let lo: [f64; 2] =
        std::array::from_fn(|k| points.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min));
    let hi: [f64; 2] = std::array::from_fn(|k| {
        points
            .iter()
            .map(|p| p[k])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let scale = (760.0 / (hi[0] - lo[0])).min(350.0 / (hi[1] - lo[1]));
    let xy = |p: [f64; 3]| {
        let p = project(p);
        [
            450.0 + (p[0] - (lo[0] + hi[0]) * 0.5) * scale,
            260.0 + (p[1] - (lo[1] + hi[1]) * 0.5) * scale,
        ]
    };
    let state = &scene.states[at];
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 900 520\" role=\"img\" aria-label=\"{}\"><title>{}</title><rect width=\"900\" height=\"520\" rx=\"18\" fill=\"#f4f6f4\"/><g fill=\"none\" stroke=\"#e3e8e5\" stroke-width=\"1\">",
        escape(&state.label),
        escape(&state.label)
    );
    for i in 0..19 {
        let x = i * 50;
        svg.push_str(&format!("<path d=\"M{x} 0V520\"/>"));
    }
    for i in 0..11 {
        let y = i * 50;
        svg.push_str(&format!("<path d=\"M0 {y}H900\"/>"));
    }
    svg.push_str("</g><g stroke=\"#476076\" stroke-width=\"1\" stroke-linejoin=\"round\">");
    for f in faces(scene, &evaluation.frames[at]) {
        let points = f
            .points
            .iter()
            .map(|p| {
                let p = xy(*p);
                format!("{:.3},{:.3}", p[0], p[1])
            })
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            "<polygon points=\"{points}\" fill=\"{}\"><title>{}</title></polygon>",
            f.fill,
            escape(&scene.parts[f.part].label)
        ));
    }
    svg.push_str("</g><g font-family=\"system-ui,sans-serif\" fill=\"#40566a\"><text x=\"28\" y=\"36\" font-size=\"14\">ORTHOGRAPHIC · 45° / 30°</text>");
    svg.push_str(&format!(
        "<text x=\"28\" y=\"492\" font-size=\"15\">{} · {}</text></g></svg>",
        escape(&state.label),
        escape(unit_label(&scene.unit))
    ));
    svg
}
