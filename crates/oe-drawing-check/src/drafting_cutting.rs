//! Source geometry of explicit, named cutting paths. No absent path is invented.
use super::*;

#[derive(Clone)]
pub(super) struct Site {
    pub id: String,
    pub expression_id: String,
    pub view: Option<String>,
    pub source_handles: Vec<String>,
    pub member_ids: Vec<String>,
    pub position: [f64; 2],
    pub kind: String,
    pub letter: String,
    pub present: bool,
    pub template_handle: String,
    pub proposed_text: Option<Value>,
    pub applicability: String,
}

impl Site {
    fn value(&self) -> Value {
        serde_json::json!({"id":self.id,"expression_id":self.expression_id,"view":self.view,"source_handles":self.source_handles,"member_ids":self.member_ids,"position":self.position,"kind":self.kind,"letter":self.letter,"present":self.present,"template_handle":self.template_handle,"proposed_text":self.proposed_text,"applicability":self.applicability})
    }
}
pub(super) fn values(bytes: &[u8], graph: &Value) -> Result<Vec<Value>> {
    let records = dxf::records(&dxf::decode(bytes)?)?;
    let d = Dxf::new(&records)?;
    let objects = objects("source", graph, &d)?;
    Ok(sites(graph, &objects, &d)?
        .iter()
        .map(Site::value)
        .collect())
}
fn parameter(name: &str) -> Result<f64> {
    let k: toml::Value = toml::from_str(KNOWLEDGE)?;
    k["cutting_identifiers"]["parameters"][name]
        .as_float()
        .with_context(|| format!("cutting_identifier_parameter:{name}"))
}
fn centre(t: TextBox) -> P {
    let (c, s) = (t.rotation_rad.cos(), t.rotation_rad.sin());
    (
        t.origin.0 + c * t.width / 2. - s * t.height / 2.,
        t.origin.1 + s * t.width / 2. + c * t.height / 2.,
    )
}
fn unit(p: P) -> P {
    let l = p.0.hypot(p.1);
    if l > 1e-12 {
        (p.0 / l, p.1 / l)
    } else {
        (0., -1.)
    }
}
fn text_bounds(t: TextBox) -> [f64; 4] {
    let (c, s) = (t.rotation_rad.cos(), t.rotation_rad.sin());
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for (x, y) in [(0., 0.), (t.width, 0.), (0., t.height), (t.width, t.height)] {
        let p = (t.origin.0 + c * x - s * y, t.origin.1 + s * x + c * y);
        b[0] = b[0].min(p.0);
        b[1] = b[1].min(p.1);
        b[2] = b[2].max(p.0);
        b[3] = b[3].max(p.1);
    }
    b
}
fn clear(candidate: TextBox, objects: &[Object], padding: f64) -> bool {
    let (c, s) = (candidate.rotation_rad.cos(), candidate.rotation_rad.sin());
    let t = TextBox {
        origin: (
            candidate.origin.0 - padding * c + padding * s,
            candidate.origin.1 - padding * s - padding * c,
        ),
        width: candidate.width + 2. * padding,
        height: candidate.height + 2. * padding,
        ..candidate
    };
    let a = text_bounds(t);
    for o in objects {
        if o.geometry
            .chains
            .iter()
            .any(|chain| chain.windows(2).any(|w| t.chord(w[0], w[1]) > 1e-9))
        {
            return false;
        }
        if let Some(tb) = o.geometry.text_box {
            let b = text_bounds(tb);
            if a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3] {
                return false;
            }
        }
    }
    true
}

/// The graph supplies candidate membership; every endpoint, letter and font reference is read from source geometry.
/// This finite profile handles loose planar strokes and TEXT. Nested occurrences remain outside automatic correction.
pub(super) fn sites(g: &Value, objects: &[Object], d: &Dxf<'_>) -> Result<Vec<Site>> {
    let join = parameter("join_height_ratio")?;
    let reach = parameter("letter_reach_height_ratio")?;
    let turn = parameter("turn_sine_min")?;
    let outside = parameter("outside_height_ratio")?;
    let step = parameter("placement_step_height_ratio")?;
    let padding = parameter("clearance_height_ratio")?;
    let mut sites = Vec::new();
    for expression in list(g, "expressions")? {
        if !expression["qualifiers"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v == "cutting_plane"))
        {
            continue;
        }
        let eid = text(expression, "id")?;
        let members: Vec<_> = list(g, "members")?
            .iter()
            .filter(|m| m["expression_id"] == eid)
            .collect();
        let object = |m: &Value| {
            objects
                .iter()
                .find(|o| o.member_ids.iter().any(|id| m["id"] == id.as_str()))
        };
        let strokes: Vec<_> = members
            .iter()
            .filter(|m| m["member_role"] == "cutting_stroke" && m["native_member_id"].is_null())
            .filter_map(|m| Some((*m, object(m)?, object(m)?.segment()?)))
            .collect();
        if strokes.len() < 2 {
            continue;
        }
        let mut templates: Vec<_> = members
            .iter()
            .filter(|m| m["member_role"] == "view_letter" && m["native_member_id"].is_null())
            .filter_map(|m| {
                let o = object(m)?;
                let h = o.source_handles.first()?;
                let r = *d.by_handle.get(h.as_str())?;
                (r.kind == "TEXT"
                    && o.text
                        .as_ref()
                        .is_some_and(|s| s.len() == 1 && s.as_bytes()[0].is_ascii_uppercase())
                    && o.geometry.text_box.is_some())
                .then_some((*m, o, r))
            })
            .collect();
        templates.sort_by(|a, b| a.1.source_handles.cmp(&b.1.source_handles));
        let letters: BTreeSet<_> = templates
            .iter()
            .filter_map(|(_, o, _)| o.text.as_deref())
            .collect();
        if letters.len() != 1 {
            continue;
        }
        let Some((_, template, record)) = templates
            .iter()
            .find(|(m, _, _)| {
                !m["evidence_ids"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|v| v == "cutting_path_turn"))
            })
            .or(templates.first())
        else {
            continue;
        };
        let tb = template.geometry.text_box.context("cutting_template_box")?;
        let letter = template.text.clone().unwrap_or_default();
        let mut paired = BTreeSet::new();
        let mut positions: Vec<(Vec<usize>, P, P, &str)> = Vec::new();
        for i in 0..strokes.len() {
            for j in i + 1..strokes.len() {
                let (a, b) = strokes[i].2;
                let (c, d) = strokes[j].2;
                for (p, other) in [(a, b), (b, a)] {
                    for (q, next) in [(c, d), (d, c)] {
                        let u = unit((other.0 - p.0, other.1 - p.1));
                        let v = unit((next.0 - q.0, next.1 - q.1));
                        if dist(p, q) <= join * tb.height && (u.0 * v.1 - u.1 * v.0).abs() > turn {
                            paired.extend([i, j]);
                            positions.push((vec![i, j], p, unit((-u.0 - v.0, -u.1 - v.1)), "turn"));
                        }
                    }
                }
            }
        }
        for (i, (_, _, (a, b))) in strokes.iter().enumerate() {
            if paired.contains(&i) {
                continue;
            }
            let nearest = |p: P| {
                strokes
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, (_, _, (c, d)))| dist(p, *c).min(dist(p, *d)))
                    .fold(f64::INFINITY, f64::min)
            };
            let (at, inner) = if nearest(*a) >= nearest(*b) {
                (*a, *b)
            } else {
                (*b, *a)
            };
            positions.push((vec![i], at, unit((at.0 - inner.0, at.1 - inner.1)), "end"));
        }
        for (indices, at, outward, kind) in positions {
            let mut handles: Vec<_> = indices
                .iter()
                .flat_map(|i| strokes[*i].1.source_handles.clone())
                .collect();
            handles.sort();
            handles.dedup();
            let mut mids: Vec<_> = indices
                .iter()
                .flat_map(|i| strokes[*i].1.member_ids.clone())
                .collect();
            mids.sort();
            mids.dedup();
            let nearby = |o: &&Object| {
                o.geometry
                    .text_box
                    .is_some_and(|t| dist(centre(t), at) < reach * tb.height)
            };
            let present = objects
                .iter()
                .filter(|o| o.text.as_deref() == Some(letter.as_str()))
                .any(|o| nearby(&o));
            let conflicting = objects
                .iter()
                .filter(|o| {
                    o.text.as_ref().is_some_and(|s| {
                        s.len() == 1 && s.as_bytes()[0].is_ascii_uppercase() && *s != letter
                    })
                })
                .any(|o| nearby(&o));
            // A conflicting identifier is a separate ambiguity, not permission to add another letter.
            if conflicting && !present {
                continue;
            }
            let mut proposed = None;
            if !present && record.tag_or(71, "0") == "0" {
                'search: for n in 0..5 {
                    for side in [0., 1., -1., 2., -2.] {
                        let along = (outside + f64::from(n) * step) * tb.height;
                        let across = side * step * tb.height;
                        let p = (
                            at.0 + outward.0 * along - outward.1 * across,
                            at.1 + outward.1 * along + outward.0 * across,
                        );
                        if dist(p, at) > reach * tb.height {
                            continue;
                        }
                        let (c, s) = (tb.rotation_rad.cos(), tb.rotation_rad.sin());
                        let candidate = TextBox {
                            origin: (
                                p.0 - c * tb.width / 2. + s * tb.height / 2.,
                                p.1 - s * tb.width / 2. - c * tb.height / 2.,
                            ),
                            ..tb
                        };
                        if clear(candidate, objects, padding * tb.height) {
                            proposed = Some(
                                serde_json::json!({"value":letter,"position":[candidate.origin.0,candidate.origin.1],"height":tb.height,
                            "rotation":record.float(50).unwrap_or(0.),"layer":record.tag_or(8,"0"),"template_handle":template.source_handles[0]}),
                            );
                            break 'search;
                        }
                    }
                }
            }
            sites.push(Site{id:format!("cutting-site:{}",handles.join("+")),expression_id:eid.into(),view:optional_text(expression,"view_id"),
                source_handles:handles,member_ids:mids,position:[at.0,at.1],kind:kind.into(),letter:letter.clone(),present,
                template_handle:template.source_handles[0].clone(),proposed_text:proposed,
                applicability:"Explicit named cutting path: GB/T 4458.6 6.1 applies; 6.2 may omit arrows only; unmarked symmetric/local sections of 6.3/6.4 are not inferred as missing paths".into()});
        }
    }
    sites.sort_by(|a, b| a.id.cmp(&b.id));
    sites.dedup_by(|a, b| a.id == b.id);
    Ok(sites)
}

pub(super) fn append(
    layer: &str,
    g: &Value,
    d: &Dxf<'_>,
    objects: &mut Vec<Object>,
    relations: &mut Vec<Relation>,
) -> Result<()> {
    for site in sites(g, objects, d)? {
        let i = objects.len();
        objects.push(Object {
            id: format!("{layer}:{}", site.id),
            classes: vec!["CuttingLetterSite"],
            source_handles: site.source_handles,
            member_ids: site.member_ids,
            expression_id: Some(site.expression_id),
            expression_kind: Some("view_callout".into()),
            view: site.view,
            line_type: None,
            text: Some(site.letter),
            rotation_deg: None,
            angle_deg: None,
            height: None,
            spacing: None,
            dxf_layer: None,
            colour: None,
            layer_colour: None,
            inferred_geometry: false,
            span: None,
            view_fact: None,
            geometry: Geometry::default(),
        });
        relations.push(Relation::number(
            i,
            "cutLetterCount",
            if site.present { 1. } else { 0. },
        ));
    }
    Ok(())
}
