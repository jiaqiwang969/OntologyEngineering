//! Independent check of one expression graph against the original DXF bytes, for any drawing.
//!
//! Re-derives from the source bytes, without the classifier: every model-space graphic root kept with its
//! type, layer and owner; native members distinct and present; line patterns from the LTYPE elements; exact
//! LINE end points; member intervals covering each source domain; expression families, member lists, views
//! and the absence of invented part targets; structure ends, inferred spans and structural lines; the
//! rendered SVG identities. Also checks that the worker named in review.json is the expected executable under
//! the deny-default sandbox profile derived from its own path, and that every pinned file is unchanged.
//! Source conservation and output-contract consistency only; no classification or geometry is produced.
use crate::{dxf, svg};
use anyhow::{Context, Result, anyhow, ensure};
use oe_contracts::*;
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::f64::consts::PI;
use std::path::Path;

pub const SCHEMA: &str = "oev2.drawing-expression-graph-check/1";
pub const REQUEST_SCHEMA: &str = "oev2.drawing-expression-graph-check-request/1";
const GRAPH_SCHEMA: &str = "oev2.drawing-expression-graph/1";
const LIMIT: usize = 64 * 1024 * 1024;
/// Labels of the assertions about what ran rather than about the source.
const IDENTITY: [&str; 3] = [
    "Actual worker is the pinned classifier",
    "Review names the requested source",
    "Pinned runtime file unchanged",
];
const SCOPE: &str = "Source conservation and output-contract consistency re-derived from the DXF bytes; not engineering correctness.";

pub struct ExpressionGraphChecker<'a> {
    pub root: &'a SkillRoot,
}
impl MdExpressionGraphCheckPort for ExpressionGraphChecker<'_> {
    fn check_graph(
        &self,
        input: &MdExpressionGraphCheckRequest,
    ) -> std::result::Result<MdExpressionGraphCheck, FailureRecord> {
        check(self.root, input).map_err(|e| failure("drawing_expression_graph_check", e))
    }
}

pub(crate) fn failure(phase: &str, error: anyhow::Error) -> FailureRecord {
    FailureRecord {
        code: ErrorCode::InvalidContract,
        phase: phase.into(),
        retryable: false,
        partial_result: Some(
            serde_json::json!({"execution":"failed","conclusion":"unknown",
            "reason":error.to_string(),"engineering_adoption":"not_established"}),
        ),
    }
}

/// The deny-default profile a worker at `executable` runs under, written out here independently.
pub fn expected_profile(executable: &str) -> Result<String> {
    let literal = serde_json::to_string(executable)?;
    Ok(format!(
        "(version 1)\n(deny default)\n(allow process-exec (literal {literal}))\n(allow sysctl-read)\n(allow mach-lookup)\n(allow file-read-metadata)\n(allow file-read* (literal \"/\") (literal {literal}) (subpath \"/System/Library\") (subpath \"/usr/lib\") (subpath \"/private/var/db/dyld\") (literal \"/dev/null\"))\n"
    ))
}

struct Checks(Vec<String>);
impl Checks {
    fn ok(&mut self, value: bool, label: impl Into<String>) -> Result<()> {
        let label = label.into();
        ensure!(value, "drawing_check_failed:{label}");
        self.0.push(label);
        Ok(())
    }
}

fn get<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .with_context(|| format!("drawing_check_field_missing:{key}"))
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    get(v, key)?
        .as_str()
        .with_context(|| format!("drawing_check_field_not_text:{key}"))
}
fn list<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    get(v, key)?
        .as_array()
        .with_context(|| format!("drawing_check_field_not_list:{key}"))
}
/// A list that older graphs may lack.
fn list_or_empty<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
fn number(v: &Value) -> Result<f64> {
    v.as_f64().context("drawing_check_not_a_number")
}
fn numbers(v: &Value, key: &str) -> Result<Vec<f64>> {
    list(v, key)?.iter().map(number).collect()
}
fn optional_text<'a>(v: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match get(v, key)? {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s)),
        _ => Err(anyhow!("drawing_check_field_not_text:{key}")),
    }
}
fn texts<'a>(v: &'a Value, key: &str) -> Result<Vec<&'a str>> {
    list(v, key)?
        .iter()
        .map(|x| x.as_str().context("drawing_check_not_text"))
        .collect()
}
fn read_json(path: &Path) -> Result<Value> {
    Ok(oe_core::json::strict_json(&read_regular_bytes(
        path, LIMIT,
    )?)?)
}

/// Line pattern of a line type from its LTYPE elements (group 49 lengths, group 74 complex flags).
fn pattern(name: &str, ltypes: &HashMap<String, (Vec<f64>, bool)>) -> &'static str {
    let upper = name.to_uppercase();
    if upper == "BYBLOCK" {
        return "byblock_unresolved";
    }
    if upper == "CONTINUOUS" {
        return "continuous";
    }
    let Some((elements, complex)) = ltypes.get(&upper) else {
        return "unknown";
    };
    if *complex {
        return "unknown";
    }
    if elements.is_empty() || !elements.iter().any(|x| *x < 0.) {
        return "continuous";
    }
    let marks: Vec<f64> = elements.iter().copied().filter(|x| *x >= 0.).collect();
    let longest = marks.iter().copied().fold(0., f64::max);
    if marks.is_empty() || longest <= 0. {
        return "unknown";
    }
    let long = marks.iter().filter(|x| **x >= 0.6 * longest).count();
    let short = marks.len() - long;
    if short == 0 {
        "dashed"
    } else if short == long {
        "dash_dot"
    } else if short == 2 * long {
        "dash_double_dot"
    } else {
        "unknown"
    }
}

fn family(kind: &str) -> Option<&'static str> {
    if kind.starts_with("dimension_") {
        return Some("dimension");
    }
    if kind.starts_with("gps_") {
        return Some("tolerance");
    }
    Some(match kind {
        "visible_contour" | "hidden_contour" | "center_axis" | "phantom_line"
        | "thread_convention" | "break_line" | "section_hatch" => "geometry",
        "thread_note" | "leader" => "dimension",
        "geometric_tolerance" | "datum_symbol" => "tolerance",
        "surface_roughness" => "surface",
        "view_callout" => "view",
        "technical_general"
        | "material_heat"
        | "process_requirement"
        | "protection_requirement" => "note",
        "sheet_frame" | "title_block" => "sheet",
        "construction_reference" => "construction",
        "unresolved" => "unresolved",
        _ => return None,
    })
}

type P = (f64, f64);

pub fn check(
    root: &SkillRoot,
    q: &MdExpressionGraphCheckRequest,
) -> Result<MdExpressionGraphCheck> {
    ensure!(
        q.validate().is_ok() && q.schema == REQUEST_SCHEMA,
        "drawing_check_request_contract"
    );
    let request: Value = read_json(&root.private_path(Path::new(&q.discovery_request))?)?;
    let source = get(&request, "source")?;
    let raw = read_regular_bytes(&root.private_path(Path::new(text(source, "path")?))?, LIMIT)?;
    let output_directory = text(&request, "output_directory")?;
    let graph_path = root.private_path(&Path::new(output_directory).join("graph.json"))?;
    let g = read_json(&graph_path)?;
    let src = get(&g, "source")?;
    let mut c = Checks(Vec::new());
    let digest = sha256(&raw);
    c.ok(
        digest == text(&g, "source_sha256")?
            && digest == text(src, "source_sha256")?
            && digest == text(source, "sha256")?,
        "Source bytes and identity unchanged",
    )?;
    let records = dxf::records(&dxf::decode(&raw)?)?;
    // Model-space graphic roots, every handled record and the line types, from the bytes.
    let mut section = String::new();
    let mut roots: HashMap<String, &dxf::Record> = HashMap::new();
    let mut all_records: HashMap<String, &dxf::Record> = HashMap::new();
    let mut ltypes: HashMap<String, (Vec<f64>, bool)> = HashMap::new();
    for r in &records {
        if r.kind == "SECTION" {
            section = r.tag_or(2, "").to_owned();
        }
        if let Some(h) = r.tag(5).filter(|h| !h.is_empty()) {
            all_records.insert(h.to_owned(), r);
        }
        if r.kind == "LTYPE"
            && let Some(name) = r.tag(2).filter(|n| !n.is_empty())
        {
            let elements = r
                .tags
                .iter()
                .filter(|(k, _)| *k == 49)
                .map(|(_, v)| v.parse::<f64>().context("dxf_ltype_element"))
                .collect::<Result<Vec<_>>>()?;
            let mut complex = false;
            for (k, v) in &r.tags {
                if *k == 74 && v.parse::<i64>().context("dxf_ltype_flag")? & 6 != 0 {
                    complex = true;
                }
            }
            ltypes.insert(name.to_uppercase(), (elements, complex));
        }
        if section == "ENTITIES"
            && !["SECTION", "ENDSEC", "SEQEND", "ATTRIB", "VERTEX"].contains(&r.kind.as_str())
            && r.tag_or(67, "0") == "0"
        {
            roots.insert(r.tag_or(5, "").to_owned(), r);
        }
        if r.kind == "ENDSEC" {
            section.clear();
        }
    }
    let entities = list(src, "entities")?;
    let ents: BTreeSet<&str> = entities
        .iter()
        .map(|e| text(e, "id"))
        .collect::<Result<_>>()?;
    c.ok(
        roots.keys().map(String::as_str).collect::<BTreeSet<_>>() == ents
            && ents.len() == entities.len(),
        "Every original model-space graphic root preserved exactly",
    )?;
    let native = list(src, "members")?;
    let native_ids: BTreeSet<&str> = native
        .iter()
        .map(|m| text(m, "id"))
        .collect::<Result<_>>()?;
    c.ok(
        !native.is_empty() && native_ids.len() == native.len(),
        "All native members retained with distinct occurrence paths",
    )?;
    for m in native {
        let id = text(m, "id")?;
        let path = texts(m, "block_path")?;
        c.ok(
            all_records.contains_key(text(m, "handle")?)
                && path.first().is_some_and(|p| roots.contains_key(*p)),
            format!("Native member source exists: {id}"),
        )?;
    }
    let mut layers: HashMap<&str, &dxf::Record> = HashMap::new();
    for r in records.iter().filter(|r| r.kind == "LAYER") {
        layers.insert(r.tag_or(2, ""), r);
    }
    let styles: HashMap<&str, &Value> = list(&g, "line_styles")?
        .iter()
        .map(|s| Ok((text(s, "source_handle")?, s)))
        .collect::<Result<_>>()?;
    let curve = [
        "LINE",
        "ARC",
        "CIRCLE",
        "LWPOLYLINE",
        "POLYLINE",
        "SPLINE",
        "ELLIPSE",
    ];
    let expressions = list(&g, "expressions")?;
    let members = list(&g, "members")?;
    let exprs: HashMap<&str, &Value> = expressions
        .iter()
        .map(|e| Ok((text(e, "id")?, e)))
        .collect::<Result<_>>()?;
    let views: BTreeSet<&str> = list(&g, "views")?
        .iter()
        .map(|v| text(v, "id"))
        .collect::<Result<_>>()?;
    let mut by_src: HashMap<&str, Vec<&Value>> = HashMap::new();
    let mut by_expr: HashMap<&str, Vec<&Value>> = HashMap::new();
    for m in members {
        by_src.entry(text(m, "source_handle")?).or_default().push(m);
        by_expr
            .entry(text(m, "expression_id")?)
            .or_default()
            .push(m);
    }
    for e in entities {
        let h = text(e, "id")?;
        let r = roots[h];
        c.ok(
            text(e, "kind")? == r.kind
                && text(e, "layer")? == r.tag_or(8, "0")
                && optional_text(e, "native_owner")? == r.tag(330),
            format!("Native type, layer and owner retained: {h}"),
        )?;
        let mut line_type = r.tag_or(6, "BYLAYER");
        if line_type.to_uppercase() == "BYLAYER" {
            line_type = layers
                .get(r.tag_or(8, "0"))
                .map_or("CONTINUOUS", |l| l.tag_or(6, "CONTINUOUS"));
        }
        let kind = text(e, "kind")?;
        if curve.contains(&kind) {
            let label = format!("Line pattern recomputed from LTYPE elements agrees: {h}");
            let observed = styles.get(h).map(|s| text(s, "pattern")).transpose()?;
            c.ok(observed == Some(pattern(line_type, &ltypes)), label)?;
        }
        let geometry = text(e, "geometry_kind")?;
        if geometry == "line" {
            let label = format!("Qualified LINE keeps exact source endpoints: {h}");
            let source: Option<Vec<f64>> =
                [10, 20, 11, 21].iter().map(|k| r.float(*k).ok()).collect();
            c.ok(
                source.is_some() && source == Some(numbers(e, "coordinates")?),
                label,
            )?;
        }
        let ms = by_src.get(h).map_or(&[][..], Vec::as_slice);
        let mut intervals = Vec::new();
        for m in ms {
            let i = numbers(m, "interval")?;
            ensure!(i.len() == 2, "drawing_check_interval_shape");
            intervals.push((i[0], i[1]));
        }
        c.ok(
            !ms.is_empty()
                && intervals
                    .iter()
                    .all(|(a, b)| 0. <= *a && a <= b && *b <= 1.),
            format!("Members exist and stay inside the source domain: {h}"),
        )?;
        intervals.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
        let mut covered = 0.;
        for (a, b) in &intervals {
            c.ok(*a <= covered + 1e-9, format!("No uncovered gap: {h}"))?;
            covered = f64::max(covered, *b);
        }
        c.ok(
            covered >= 1. - 1e-9,
            format!("Source domain covered to its end: {h}"),
        )?;
        c.ok(
            geometry == "line" || intervals.iter().all(|i| *i == (0., 1.)),
            format!("Partial intervals only on qualified LINE sources: {h}"),
        )?;
    }
    for e in expressions {
        let id = text(e, "id")?;
        let kind = text(e, "kind")?;
        c.ok(
            family(kind) == Some(text(e, "family")?),
            format!("Family follows kind: {id}"),
        )?;
        let ms = by_expr.get(id).map_or(&[][..], Vec::as_slice);
        let mut have: Vec<&str> = ms.iter().map(|m| text(m, "id")).collect::<Result<_>>()?;
        let mut declared = texts(e, "member_ids")?;
        have.sort_unstable();
        declared.sort_unstable();
        c.ok(
            have == declared && !ms.is_empty(),
            format!("Expression member list complete: {id}"),
        )?;
        c.ok(
            get(e, "engineering_target")?.is_null(),
            format!("No invented part target: {id}"),
        )?;
        let view = optional_text(e, "view_id")?;
        c.ok(
            view.is_none_or(|v| views.contains(v)),
            format!("View exists: {id}"),
        )?;
        let mut same_view = true;
        for m in ms {
            same_view &= optional_text(m, "view_id")? == view;
        }
        c.ok(
            same_view,
            format!("Members carry their expression view: {id}"),
        )?;
        if kind.starts_with("dimension_") || kind == "thread_note" {
            let mut valued = false;
            for m in ms {
                valued |= ["native_dimension", "dimension_text"].contains(&text(m, "member_role")?);
            }
            c.ok(
                valued || text(e, "state")? == "unknown",
                format!("Dimension carries its value source: {id}"),
            )?;
        }
        for x in texts(e, "evidence_ids")? {
            if let Some(v) = x.strip_prefix("references_view:") {
                let v = v.strip_suffix(":by_elimination").unwrap_or(v);
                c.ok(views.contains(v), format!("Referenced view exists: {id}"))?;
            }
            if let Some(target) = x.strip_prefix("applies_to_size_dimension:") {
                let label = format!("Frame applies to a size dimension: {id}");
                let d = exprs.get(target).copied();
                let d_kind = d.map(|d| text(d, "kind")).transpose()?;
                c.ok(
                    d_kind.is_some_and(|k| k.starts_with("dimension_") || k == "thread_note"),
                    label,
                )?;
                let arrowheads = |list: &[&Value]| -> Result<BTreeSet<String>> {
                    let mut out = BTreeSet::new();
                    for m in list {
                        if text(m, "member_role")? == "arrowhead" {
                            out.insert(text(m, "source_handle")?.to_owned());
                        }
                    }
                    Ok(out)
                };
                let theirs = by_expr.get(target).map_or(&[][..], Vec::as_slice);
                let shared = arrowheads(ms)?.intersection(&arrowheads(theirs)?).count();
                c.ok(
                    shared == 2,
                    format!("Frame shares both terminators of that dimension: {id}"),
                )?;
            }
        }
    }
    let mut open_unknown = true;
    for r in list(src, "relations")? {
        if text(r, "kind")? == "open_endpoint" {
            open_unknown &= text(r, "state")? == "unknown";
        }
    }
    c.ok(
        open_unknown,
        "Open endpoints stay obligations, not missing-line verdicts",
    )?;
    // Structure topology records: ends lie on their source, inferred spans run between exact source end
    // points, stay collinear (or co-circular) with both sources and are never members.
    let line_of = |h: &str| -> Result<[f64; 4]> {
        let r = roots.get(h).context("drawing_check_root_missing")?;
        Ok([r.float(10)?, r.float(20)?, r.float(11)?, r.float(21)?])
    };
    let at = |h: &str, t: f64| -> Result<(P, [f64; 4])> {
        let l = line_of(h)?;
        Ok(((l[0] + t * (l[2] - l[0]), l[1] + t * (l[3] - l[1])), l))
    };
    let arc_of = |h: &str| -> Result<(P, f64, f64, f64)> {
        let r = roots.get(h).context("drawing_check_root_missing")?;
        let centre = (r.float(10)?, r.float(20)?);
        let radius = r.float(40)?;
        let a0 = r.float(50)?.to_radians();
        let a1 = r.float(51)?.to_radians();
        let sweep = (a1 - a0).rem_euclid(2. * PI);
        Ok((
            centre,
            radius,
            a0,
            if sweep == 0. { 2. * PI } else { sweep },
        ))
    };
    let arc_at = |h: &str, t: f64| -> Result<P> {
        let (centre, radius, a0, w) = arc_of(h)?;
        let a = a0 + w * t;
        Ok((centre.0 + radius * a.cos(), centre.1 + radius * a.sin()))
    };
    let dist = |a: P, b: P| (a.0 - b.0).hypot(a.1 - b.1);
    let ends = list_or_empty(&g, "structure_ends");
    let end_ids: Vec<&str> = ends.iter().map(|e| text(e, "id")).collect::<Result<_>>()?;
    c.ok(
        end_ids.len() == end_ids.iter().collect::<BTreeSet<_>>().len(),
        "Structural end identities distinct",
    )?;
    for e in ends {
        let id = text(e, "id")?;
        let h = text(e, "source_handle")?;
        c.ok(
            roots.contains_key(h)
                && ["text_break", "local_section", "drafting_gap", "unexplained"]
                    .contains(&text(e, "cause")?)
                && ["visible_outline", "hidden_outline"].contains(&text(e, "line_class")?),
            format!("Structural end typed: {id}"),
        )?;
        let partner = optional_text(e, "partner_end_id")?;
        c.ok(
            partner.is_none_or(|p| end_ids.contains(&p)),
            format!("Partner end exists: {id}"),
        )?;
        if roots[h].kind == "LINE" {
            let (p, _) = at(h, number(get(e, "parameter")?)?)?;
            let point = numbers(e, "point")?;
            ensure!(point.len() >= 2, "drawing_check_point_shape");
            c.ok(
                (p.0 - point[0]).abs() <= 1e-9 && (p.1 - point[1]).abs() <= 1e-9,
                format!("Structural end lies on its source: {id}"),
            )?;
        }
    }
    let member_ids: BTreeSet<&str> = members
        .iter()
        .map(|m| text(m, "id"))
        .collect::<Result<_>>()?;
    let spans = list_or_empty(&g, "implied_spans");
    for s in spans {
        let id = text(s, "id")?;
        c.ok(
            !member_ids.contains(id) && text(s, "representation")? == "complete_structure",
            format!("Inferred span is not a source member: {id}"),
        )?;
        let from = text(s, "from_source_handle")?;
        let to = text(s, "to_source_handle")?;
        let kinds: BTreeSet<Option<&str>> = [from, to]
            .iter()
            .map(|h| roots.get(*h).map(|r| r.kind.as_str()))
            .collect();
        let points = numbers(s, "points")?;
        ensure!(points.len() >= 4, "drawing_check_points_shape");
        let (tf, tt) = (
            number(get(s, "from_parameter")?)?,
            number(get(s, "to_parameter")?)?,
        );
        let arc = match s.get("arc") {
            Some(Value::Array(a)) if !a.is_empty() => {
                Some(a.iter().map(number).collect::<Result<Vec<f64>>>()?)
            }
            _ => None,
        };
        if let Some(arc) = arc {
            c.ok(
                kinds == BTreeSet::from([Some("ARC")]),
                format!("Inferred arc span bounded by source arcs: {id}"),
            )?;
            let (a, b) = (arc_at(from, tf)?, arc_at(to, tt)?);
            let gap = [
                (a.0 - points[0]).abs(),
                (a.1 - points[1]).abs(),
                (b.0 - points[2]).abs(),
                (b.1 - points[3]).abs(),
            ]
            .into_iter()
            .fold(0., f64::max);
            c.ok(
                gap <= 1e-6,
                format!("Inferred span ends at exact source end points: {id}"),
            )?;
            ensure!(arc.len() >= 3, "drawing_check_arc_shape");
            let mut on_circle = true;
            for h in [from, to] {
                let (centre, radius, _, _) = arc_of(h)?;
                on_circle &=
                    dist(centre, (arc[0], arc[1])).abs() <= 0.02 && (radius - arc[2]).abs() <= 0.02;
            }
            c.ok(
                on_circle,
                format!("Inferred arc span co-circular with both bounding sources: {id}"),
            )?;
        } else {
            c.ok(
                kinds == BTreeSet::from([Some("LINE")]),
                format!("Inferred span bounded by source lines: {id}"),
            )?;
            let ((a, la), (b, lb)) = (at(from, tf)?, at(to, tt)?);
            let gap = [
                (a.0 - points[0]).abs(),
                (a.1 - points[1]).abs(),
                (b.0 - points[2]).abs(),
                (b.1 - points[3]).abs(),
            ]
            .into_iter()
            .fold(0., f64::max);
            c.ok(
                gap <= 1e-9,
                format!("Inferred span ends at exact source end points: {id}"),
            )?;
            let collinear = |l: [f64; 4]| {
                let (dx, dy) = (l[2] - l[0], l[3] - l[1]);
                let n = (dx * dx + dy * dy).sqrt();
                [a, b]
                    .iter()
                    .all(|(px, py)| ((px - l[0]) * dy - (py - l[1]) * dx).abs() / n <= 0.02)
            };
            c.ok(
                collinear(la) && collinear(lb),
                format!("Inferred span collinear with both bounding sources: {id}"),
            )?;
        }
        let cause = text(s, "cause")?;
        let mut reported = false;
        for e in ends {
            let h = text(e, "source_handle")?;
            reported |= (h == from || h == to) && text(e, "cause")? == cause;
        }
        c.ok(
            reported,
            format!("Inferred span starts at a reported end: {id}"),
        )?;
    }
    // Structural lines, checked against the source bytes.
    let class_of = |m: &Value| -> Result<Option<&'static str>> {
        let kind = text(
            exprs
                .get(text(m, "expression_id")?)
                .context("drawing_check_expression_missing")?,
            "kind",
        )?;
        Ok(match (kind, text(m, "member_role")?) {
            ("visible_contour", "contour_segment") => Some("visible_outline"),
            ("hidden_contour", "hidden_segment") => Some("hidden_outline"),
            _ => None,
        })
    };
    let in_section = |m: &Value| -> Result<bool> {
        Ok(texts(m, "evidence_ids")?
            .iter()
            .any(|x| x.starts_with("section_profile:")))
    };
    let mut owners: BTreeMap<String, (Option<String>, String)> = BTreeMap::new();
    let mut mem: HashMap<&str, &Value> = HashMap::new();
    for m in members {
        mem.insert(text(m, "id")?, m);
        if let Some(class) = class_of(m)?
            && !in_section(m)?
        {
            owners.insert(
                format!("member:{}", text(m, "id")?),
                (
                    optional_text(m, "view_id")?.map(str::to_owned),
                    class.to_owned(),
                ),
            );
        }
    }
    let mut imp: HashMap<&str, &Value> = HashMap::new();
    for s in spans {
        imp.insert(text(s, "id")?, s);
        owners.insert(
            text(s, "id")?.to_owned(),
            (
                optional_text(s, "view_id")?.map(str::to_owned),
                text(s, "line_class")?.to_owned(),
            ),
        );
    }
    let lines = list_or_empty(&g, "structural_lines");
    let line_ids: Vec<&str> = lines.iter().map(|l| text(l, "id")).collect::<Result<_>>()?;
    c.ok(
        line_ids.len() == line_ids.iter().collect::<BTreeSet<_>>().len(),
        "Structural line identities distinct",
    )?;
    let seg_pts = |p: &str| -> Result<Option<Vec<P>>> {
        if let Some(mid) = p.strip_prefix("member:") {
            let m = mem.get(mid).context("drawing_check_member_missing")?;
            let h = text(m, "source_handle")?;
            if roots.get(h).is_some_and(|r| r.kind == "LINE") {
                return Ok(Some(
                    numbers(m, "interval")?
                        .iter()
                        .map(|t| at(h, *t).map(|x| x.0))
                        .collect::<Result<_>>()?,
                ));
            }
            return Ok(None);
        }
        let s = imp.get(p).context("drawing_check_span_missing")?;
        if s.get("arc")
            .is_some_and(|a| a.as_array().is_some_and(|a| !a.is_empty()))
        {
            return Ok(None);
        }
        let pts = numbers(s, "points")?;
        ensure!(pts.len() >= 4, "drawing_check_points_shape");
        Ok(Some(vec![(pts[0], pts[1]), (pts[2], pts[3])]))
    };
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for l in lines {
        let id = text(l, "id")?;
        let class = text(l, "line_class")?;
        let view = optional_text(l, "view_id")?;
        let mut parts = texts(l, "pieces")?;
        let coincident = texts(l, "coincident")?;
        parts.extend(coincident.iter().copied());
        for p in &parts {
            *seen.entry((*p).to_owned()).or_default() += 1;
            c.ok(
                owners
                    .get(*p)
                    .is_some_and(|(v, k)| v.as_deref() == view && k == class),
                format!("Structural line piece of its own view and class: {id} {p}"),
            )?;
        }
        c.ok(
            !coincident.iter().any(|p| p.starts_with("implied:")),
            format!("Inferred spans are pieces, never coincident: {id}"),
        )?;
        let line_type = text(l, "line_type")?;
        c.ok(
            (class == "visible_outline") != line_type.contains("dash")
                && (class == "visible_outline" || line_type.contains("dashed")),
            format!("Line type follows the structure class: {id}"),
        )?;
        let geometry = text(l, "geometry")?;
        let trimmed = texts(l, "trimmed")?;
        if geometry == "line" {
            let p = numbers(l, "points")?;
            ensure!(p.len() >= 4, "drawing_check_points_shape");
            let (p0, p1) = ((p[0], p[1]), (p[2], p[3]));
            let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
            let n = dx.hypot(dy);
            let mut pts = Vec::new();
            for part in &parts {
                pts.extend(seg_pts(part)?.unwrap_or_default());
            }
            c.ok(
                n > 0.
                    && pts
                        .iter()
                        .all(|q| ((q.0 - p0.0) * dy - (q.1 - p0.1) * dx).abs() / n <= 0.02 + 1e-9),
                format!("Straight line pieces within 0.02 of the drawn line: {id}"),
            )?;
            let u = (dx / n, dy / n);
            let along = |q: P| (q.0 - p0.0) * u.0 + (q.1 - p0.1) * u.1;
            let label = format!("Drawn line spans its pieces (shorter only where trimmed): {id}");
            ensure!(!pts.is_empty(), "drawing_check_failed:{label}");
            let lo = pts.iter().map(|q| along(*q)).fold(f64::INFINITY, f64::min);
            let hi = pts
                .iter()
                .map(|q| along(*q))
                .fold(f64::NEG_INFINITY, f64::max);
            c.ok(
                along(p0) >= lo - 1e-6
                    && along(p1) <= hi + 1e-6
                    && (!trimmed.is_empty()
                        || ((along(p0) - lo).abs() <= 1e-6 && (along(p1) - hi).abs() <= 1e-6)),
                label,
            )?;
        } else if geometry == "arc" || geometry == "circle" {
            let arc = numbers(l, "arc")?;
            ensure!(arc.len() >= 3, "drawing_check_arc_shape");
            for part in &parts {
                if let Some(mid) = part.strip_prefix("member:") {
                    let h = text(
                        mem.get(mid).context("drawing_check_member_missing")?,
                        "source_handle",
                    )?;
                    let r = roots.get(h).context("drawing_check_root_missing")?;
                    let (centre, radius) = if r.kind == "ARC" {
                        let (c0, r0, _, _) = arc_of(h)?;
                        (c0, r0)
                    } else {
                        ((r.float(10)?, r.float(20)?), r.float(40)?)
                    };
                    c.ok(
                        dist(centre, (arc[0], arc[1])) <= 0.02 + 1e-9
                            && (radius - arc[2]).abs() <= 0.02 + 1e-9,
                        format!("Arc pieces co-circular with the drawn arc: {id} {part}"),
                    )?;
                }
            }
            c.ok(
                geometry != "circle" || arc.get(4).is_some_and(|w| (w - 2. * PI).abs() <= 1e-9),
                format!("Circle closes: {id}"),
            )?;
        }
        for t in &trimmed {
            let label = format!("Trimmed part inside a member of its line: {id}");
            let rest = t.get(7..).unwrap_or_default();
            let mut fields = rest.rsplitn(3, ':');
            let (b, a, mid) = (fields.next(), fields.next(), fields.next());
            let inside = match (
                mid,
                a.and_then(|a| a.parse::<f64>().ok()),
                b.and_then(|b| b.parse::<f64>().ok()),
            ) {
                (Some(mid), Some(a), Some(b)) => {
                    parts.contains(&format!("member:{mid}").as_str())
                        && 0. <= a
                        && a <= b
                        && b <= 1.
                }
                _ => false,
            };
            c.ok(inside, label)?;
        }
    }
    c.ok(
        seen.keys().collect::<BTreeSet<_>>() == owners.keys().collect::<BTreeSet<_>>()
            && seen.values().all(|n| *n == 1),
        "Every outline member outside a section profile and every inferred span in exactly one structural line",
    )?;
    let rendered = svg::inventory(&read_regular_bytes(
        &root.private_path(Path::new(&q.svg))?,
        LIMIT,
    )?)?;
    c.ok(
        rendered.entities.iter().all(|h| roots.contains_key(h)),
        "Presentation SVG identities agree with source roots",
    )?;
    // Identity of what ran: the worker named in review.json under the profile derived from its own path.
    let review = read_json(&root.private_path(&Path::new(output_directory).join("review.json"))?)?;
    let worker = std::fs::read(&q.worker_executable).context("drawing_check_worker_unreadable")?;
    let program_sha256 = sha256(&worker);
    c.ok(
        text(&review, "worker_sha256")? == program_sha256
            && text(&review, "sandbox_profile")? == expected_profile(&q.worker_executable)?,
        "Actual worker is the pinned classifier under its deny-default sandbox",
    )?;
    c.ok(
        get(&review, "source")? == source
            && get(&review, "subject")? == get(&request, "subject")?
            && get(&review, "project_context")? == get(&request, "project_context")?,
        "Review names the requested source, subject and Project context",
    )?;
    for f in &q.pinned {
        let bytes = std::fs::read(&f.path)
            .with_context(|| format!("drawing_check_pinned_unreadable:{}", f.name))?;
        c.ok(
            sha256(&bytes) == f.sha256,
            format!("Pinned runtime file unchanged: {}", f.name),
        )?;
    }
    ensure!(
        text(&g, "schema")? == GRAPH_SCHEMA,
        "drawing_check_graph_schema"
    );
    let mut by_kind: BTreeMap<String, u64> = BTreeMap::new();
    for label in &c.0 {
        *by_kind
            .entry(label.split(':').next().unwrap_or_default().to_owned())
            .or_default() += 1;
    }
    let source_derived_count = by_kind
        .iter()
        .filter(|(k, _)| !IDENTITY.iter().any(|i| k.starts_with(i)))
        .map(|(_, v)| *v)
        .sum();
    let report = MdExpressionGraphCheck {
        schema: SCHEMA.into(),
        status: "passed".into(),
        count: c.0.len() as u64,
        source_derived_count,
        graph_sha256: sha256(read_regular_bytes(&graph_path, LIMIT)?),
        worker_sha256: program_sha256,
        by_kind,
        scope: SCOPE.into(),
    };
    ensure!(report.validate().is_ok(), "drawing_check_report_contract");
    let bytes = serde_json::to_vec_pretty(&report)?;
    oe_core::write_new_bytes(&root.private_path(Path::new(&q.output))?, &bytes)?;
    Ok(report)
}
