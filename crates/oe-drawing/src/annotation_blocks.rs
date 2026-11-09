//! Drafting objects stored as one block reference.
//!
//! CAD annotation tools (CAXA, GstarCAD and AutoCAD Mechanical among them) write each annotation
//! object as one block reference, often anonymous: a leader note, a cutting-plane symbol, a detail
//! indicator and its title, the enlarged detail view itself, the technical requirements, a
//! tolerance frame, an item number, the parts list, a library standard part. The block boundary is
//! evidence of object identity; the object is recognised from its parts with the knowledge of the
//! cards (`technical_note`, `leader_note`, `view_callout`, `detail_view`, `gdt_frame_symbol`,
//! `item_number`, `parts_list`, `standard_part`, `annotation_mask`, `sheet_frame_border`).
//!
//! The parts are read bottom-up through nested references and placed by their transforms: lines,
//! arcs, circles, polylines, texts with the font of their run (an inline `\f` switch or the text
//! style), solid-filled triangles (arrowheads) and dots, and masks. Every member an object gets is
//! the block root with the part's native member path (`ROOT/child`), as DIMENSION members are: the
//! block owns its parts. Block texts never join the page-wide text collection, so the drawing's
//! text-height scale is unchanged, and loose rules never pair a block part with a loose one.
use super::*;
use oe_contracts::MdRepresentationCard;
use std::f64::consts::TAU;

// ---------------------------------------------------------------- parts of a block reference

#[derive(Clone, Debug)]
pub(super) enum Geom {
    Line(P, P),
    /// Centre, radius, start angle and counter-clockwise sweep (radians).
    Arc(P, f64, f64, f64),
    Circle(P, f64),
    /// Straight polyline vertices and whether the polyline is closed.
    Path(Vec<P>, bool),
    /// The text fact (its id is the member path) and the font of its run, lower case.
    Text(Box<TextFact>, String),
    /// A solid-filled triangle: tip first, then the two base corners.
    Head([P; 3]),
    /// A solid-filled dot: centre and radius.
    Dot(P, f64),
    /// A background mask polygon.
    Mask(Vec<P>),
}

#[derive(Clone, Debug)]
pub(super) struct Part {
    /// Native member path `ROOT/…/child`.
    pub path: String,
    /// Path of the reference that holds the part.
    pub parent: String,
    /// Line weight in 1/100 mm after ByLayer and ByBlock; negative when the drawing gives none.
    pub weight: i32,
    pub pattern: Pattern,
    pub geom: Geom,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Block {
    pub root: String,
    pub parts: Vec<Part>,
    /// Paths of the nested references, in the order written.
    pub nested: Vec<String>,
    /// Position of the root in the drawing (records written earlier come first).
    pub index: usize,
    /// Texts that draw nothing (an empty cell of a table): path and insertion point.
    pub blank: Vec<(String, P)>,
}

/// The parts of every block reference of model space, by root.
#[derive(Default)]
pub(super) struct Blocks {
    pub roots: BTreeMap<String, Block>,
    pub at: BTreeMap<String, (String, usize)>,
}
impl Blocks {
    pub(super) fn part(&self, path: &str) -> Option<&Part> {
        let (root, i) = self.at.get(path)?;
        self.roots.get(root)?.parts.get(*i)
    }
}

#[derive(Clone)]
struct Inherit {
    layer: String,
    weight: i32,
    pattern: Pattern,
}

struct Reader<'a> {
    layers: BTreeMap<String, (String, i32)>,
    patterns: &'a BTreeMap<String, Pattern>,
    defs: BTreeMap<String, (P, Vec<&'a Record>)>,
    fonts: BTreeMap<String, String>,
}

fn compose(m: &Affine, l: &Affine) -> Affine {
    [
        m[0] * l[0] + m[1] * l[3],
        m[0] * l[1] + m[1] * l[4],
        m[0] * l[2] + m[1] * l[5] + m[2],
        m[3] * l[0] + m[4] * l[3],
        m[3] * l[1] + m[4] * l[4],
        m[3] * l[2] + m[4] * l[5] + m[5],
    ]
}
fn det(m: &Affine) -> f64 {
    m[0] * m[4] - m[1] * m[3]
}
fn linear(m: &Affine, v: P) -> P {
    [m[0] * v[0] + m[1] * v[1], m[3] * v[0] + m[4] * v[1]]
}

/// Placement of a block reference: translate(at) ∘ rotate ∘ scale ∘ translate(−base).
fn placement(ins: &Record, base: P) -> Affine {
    let at = ins.xy(10).unwrap_or([0., 0.]);
    let (sx, sy) = (num_or(ins, 41, 1.), num_or(ins, 42, 1.));
    let (sin, cos) = num_or(ins, 50, 0.).to_radians().sin_cos();
    [
        cos * sx,
        -sin * sy,
        at[0] - cos * sx * base[0] + sin * sy * base[1],
        sin * sx,
        cos * sy,
        at[1] - sin * sx * base[0] - cos * sy * base[1],
    ]
}

/// The font of the first visible run of a text: an inline `\f` or `\F` switch before it, else the
/// font of the text style.
fn run_font(raw: &str, style: &str) -> String {
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('f') | Some('F') => {
                    let name: String = chars.by_ref().take_while(|x| *x != ';').collect();
                    return name.split('|').next().unwrap_or("").trim().to_lowercase();
                }
                // other format codes with an argument up to ';'
                Some('A' | 'C' | 'c' | 'H' | 'h' | 'Q' | 'q' | 'T' | 't' | 'W' | 'w') => {
                    for x in chars.by_ref() {
                        if x == ';' {
                            break;
                        }
                    }
                }
                Some('L' | 'l' | 'O' | 'o' | 'K' | 'k') => {}
                _ => break,
            }
        } else if !matches!(c, '{' | '}') && !c.is_whitespace() {
            break;
        }
    }
    style.to_string()
}

/// Boundary vertices (with bulges) of the first path of a solid-filled HATCH.
fn solid_boundary(r: &Record) -> Option<Vec<(P, f64)>> {
    let solid = r.get(70).is_some_and(|v| v.trim() == "1") || r.get(2) == Some("SOLID");
    if !solid {
        return None;
    }
    let start = r.tags.iter().position(|(c, _)| *c == 91)?;
    let tags = &r.tags[start + 1..];
    let num = |v: &str| v.trim().parse::<f64>().ok();
    let flags = tags
        .first()
        .filter(|t| t.0 == 92)
        .and_then(|t| t.1.trim().parse::<i32>().ok())?;
    let mut out: Vec<(P, f64)> = Vec::new();
    let mut x: Option<f64> = None;
    if flags & 2 != 0 {
        let n = tags
            .iter()
            .find(|t| t.0 == 93)
            .and_then(|t| t.1.trim().parse::<usize>().ok())?;
        let from = tags.iter().position(|t| t.0 == 93)? + 1;
        for (c, v) in &tags[from..] {
            match c {
                10 => x = num(v),
                20 => {
                    if out.len() == n {
                        break;
                    }
                    out.push(([x.take()?, num(v)?], 0.));
                }
                42 => {
                    if let Some(last) = out.last_mut() {
                        last.1 = num(v)?;
                    }
                }
                92 | 97 | 75 => break,
                _ => {}
            }
        }
        (out.len() == n).then_some(out)
    } else {
        // edge path: line edges only (their start points)
        let mut edge = 0;
        for (c, v) in tags.iter().skip(1) {
            match c {
                72 => edge = v.trim().parse::<i32>().ok()?,
                10 if edge == 1 => x = num(v),
                20 if edge == 1 => out.push(([x.take()?, num(v)?], 0.)),
                10 | 20 => return None,
                92 | 97 | 75 => break,
                _ => {}
            }
        }
        (out.len() >= 2).then_some(out)
    }
}

/// A filled triangle with its tip first: the vertex opposite the shortest side.
fn head_of(p: [P; 3]) -> Option<[P; 3]> {
    let sides = [dist(p[1], p[2]), dist(p[0], p[2]), dist(p[0], p[1])];
    let i = (0..3).min_by(|a, b| sides[*a].total_cmp(&sides[*b]))?;
    (sides[i] > 1e-9).then(|| [p[i], p[(i + 1) % 3], p[(i + 2) % 3]])
}
/// Tip, unit direction and length of a filled triangle.
fn head_axis(h: &[P; 3]) -> (P, P, f64) {
    let base = mid(h[1], h[2]);
    (h[0], unit(sub(h[0], base)), dist(h[0], base))
}

/// The polygon of a WIPEOUT (image clip boundary in pixel coordinates).
pub(super) fn wipeout_polygon(r: &Record) -> Option<Vec<P>> {
    let (p, u, v) = (r.xy(10).ok()?, r.xy(11).ok()?, r.xy(12).ok()?);
    let size = r.xy(13).unwrap_or([1., 1.]);
    let mut pix: Vec<P> = Vec::new();
    let mut x = None;
    for (c, val) in &r.tags {
        match c {
            14 => x = val.trim().parse::<f64>().ok(),
            24 => pix.push([x.take()?, val.trim().parse::<f64>().ok()?]),
            _ => {}
        }
    }
    if pix.len() == 2 {
        let (a, b) = (pix[0], pix[1]);
        pix = vec![a, [b[0], a[1]], b, [a[0], b[1]]];
    }
    let world = |q: P| add(add(p, mul(u, q[0] + 0.5)), mul(v, size[1] - 0.5 - q[1]));
    (pix.len() >= 3).then(|| pix.into_iter().map(world).collect())
}

impl Reader<'_> {
    fn resolve(&self, r: &Record, up: &Inherit) -> Inherit {
        let own = r.get(8).unwrap_or("0");
        let layer = if own == "0" {
            up.layer.clone()
        } else {
            own.to_string()
        };
        let (lt_layer, lw_layer) = self
            .layers
            .get(&layer)
            .cloned()
            .unwrap_or(("CONTINUOUS".into(), -3));
        let lt = r.get(6).unwrap_or("BYLAYER");
        let pattern = if lt.eq_ignore_ascii_case("BYBLOCK") {
            up.pattern
        } else if lt.eq_ignore_ascii_case("BYLAYER") {
            pattern_of(&lt_layer, self.patterns)
        } else {
            pattern_of(lt, self.patterns)
        };
        let weight = match r.get(370).and_then(|v| v.trim().parse::<i32>().ok()) {
            None | Some(-1) => lw_layer,
            Some(-2) => up.weight,
            Some(w) => w,
        };
        Inherit {
            layer,
            weight,
            pattern,
        }
    }

    fn walk(
        &self,
        ins: &Record,
        path: &str,
        m: &Affine,
        up: &Inherit,
        depth: usize,
        out: &mut Block,
    ) {
        let Some((base, recs)) = ins.get(2).and_then(|n| self.defs.get(n)) else {
            return;
        };
        if depth > 8 || out.parts.len() > 4000 {
            return;
        }
        let next = compose(m, &placement(ins, *base));
        let s = det(&next).abs().sqrt();
        let mirror = det(&next) < 0.;
        for (i, r) in recs.iter().enumerate() {
            let flat = r.number(210).unwrap_or(0.) == 0.
                && r.number(220).unwrap_or(0.) == 0.
                && r.number(230).unwrap_or(1.) == 1.;
            let Some(h) = r.get(5).filter(|_| flat) else {
                continue;
            };
            let p = format!("{path}/{h}");
            let st = self.resolve(r, up);
            let geom = match r.kind.as_str() {
                "LINE" => r
                    .xy(10)
                    .ok()
                    .zip(r.xy(11).ok())
                    .map(|(a, c)| Geom::Line(apply(&next, a), apply(&next, c))),
                "CIRCLE" => r
                    .xy(10)
                    .ok()
                    .zip(r.number(40).ok())
                    .map(|(c, rad)| Geom::Circle(apply(&next, c), rad * s)),
                "ARC" => r.xy(10).ok().zip(r.number(40).ok()).map(|(c, rad)| {
                    let a0 = r.number(50).unwrap_or(0.).to_radians();
                    let mut sw = (r.number(51).unwrap_or(360.).to_radians() - a0).rem_euclid(TAU);
                    if sw < 1e-12 {
                        sw = TAU;
                    }
                    let at = |a: f64| apply(&next, add(c, mul([a.cos(), a.sin()], rad)));
                    let cw = apply(&next, c);
                    let (p0, p1) = if mirror {
                        (at(a0 + sw), at(a0))
                    } else {
                        (at(a0), at(a0 + sw))
                    };
                    let start = (p0[1] - cw[1]).atan2(p0[0] - cw[0]);
                    let mut w = ((p1[1] - cw[1]).atan2(p1[0] - cw[0]) - start).rem_euclid(TAU);
                    if w < 1e-9 {
                        w = TAU;
                    }
                    Geom::Arc(cw, rad * s, start, w)
                }),
                "LWPOLYLINE" => {
                    let v = lw_vertices(r);
                    let pts: Vec<P> = v.iter().map(|x| apply(&next, x.0)).collect();
                    (pts.len() >= 2).then(|| Geom::Path(pts, int(r, 70) & 1 != 0))
                }
                "POLYLINE" => {
                    let pts: Vec<P> = recs[i + 1..]
                        .iter()
                        .take_while(|x| x.kind == "VERTEX")
                        .filter_map(|x| x.xy(10).ok())
                        .map(|q| apply(&next, q))
                        .collect();
                    (pts.len() >= 2).then(|| Geom::Path(pts, int(r, 70) & 1 != 0))
                }
                "TEXT" | "MTEXT" | "ATTRIB" if text_fact(r).is_none() => {
                    if let Ok(q) = r.xy(10) {
                        out.blank.push((p.clone(), apply(&next, q)));
                    }
                    None
                }
                "TEXT" | "MTEXT" | "ATTRIB" => text_fact(r).map(|t| {
                    let style = r.get(7).unwrap_or("STANDARD").to_lowercase();
                    let font = run_font(
                        &dd::raw_text(r),
                        self.fonts.get(&style).map_or("", |f| f.as_str()),
                    );
                    Geom::Text(
                        Box::new(TextFact {
                            id: p.clone(),
                            center: apply(&next, t.center),
                            dir: unit(linear(&next, t.dir)),
                            height: t.height * s,
                            width: t.width * s,
                            ..t
                        }),
                        font,
                    )
                }),
                "SOLID" | "TRACE" => {
                    let mut v: Vec<P> = Vec::new();
                    for c in 10..=13 {
                        if let Ok(q) = r.xy(c)
                            && !v.iter().any(|w| dist(*w, q) < 1e-9)
                        {
                            v.push(q);
                        }
                    }
                    (v.len() == 3)
                        .then(|| {
                            head_of([apply(&next, v[0]), apply(&next, v[1]), apply(&next, v[2])])
                        })
                        .flatten()
                        .map(Geom::Head)
                }
                "HATCH" => solid_boundary(r).and_then(|v| {
                    let pts: Vec<P> = v.iter().map(|x| apply(&next, x.0)).collect();
                    if v.len() == 2 && v.iter().all(|x| (x.1.abs() - 1.).abs() < 1e-3) {
                        Some(Geom::Dot(mid(pts[0], pts[1]), dist(pts[0], pts[1]) / 2.))
                    } else if v.len() == 3 && v.iter().all(|x| x.1 == 0.) {
                        head_of([pts[0], pts[1], pts[2]]).map(Geom::Head)
                    } else {
                        None
                    }
                }),
                "WIPEOUT" => wipeout_polygon(r)
                    .map(|v| Geom::Mask(v.iter().map(|q| apply(&next, *q)).collect())),
                "INSERT" => {
                    out.nested.push(p.clone());
                    self.walk(r, &p, &next, &st, depth + 1, out);
                    None
                }
                _ => None,
            };
            if let Some(g) = geom {
                out.parts.push(Part {
                    path: p,
                    parent: path.to_string(),
                    weight: st.weight,
                    pattern: st.pattern,
                    geom: g,
                });
            }
        }
    }
}

/// Read the parts of every model-space block reference.
pub(super) fn parse(
    d: &Drawing,
    rec: &BTreeMap<String, &Record>,
    patterns: &BTreeMap<String, Pattern>,
) -> Blocks {
    let mut layers = BTreeMap::new();
    for r in d.records.iter().filter(|r| r.kind == "LAYER") {
        if let Some(n) = r.get(2) {
            let lw = r
                .get(370)
                .and_then(|v| v.trim().parse::<i32>().ok())
                .unwrap_or(-3);
            layers.insert(
                n.to_string(),
                (r.get(6).unwrap_or("CONTINUOUS").to_string(), lw),
            );
        }
    }
    let fonts = d
        .records
        .iter()
        .filter(|r| r.kind == "STYLE")
        .filter_map(|r| {
            Some((
                r.get(2)?.to_lowercase(),
                r.get(3).unwrap_or("").to_lowercase(),
            ))
        })
        .collect();
    let mut defs: BTreeMap<String, (P, Vec<&Record>)> = BTreeMap::new();
    for r in d.records.iter().filter(|r| r.section == "BLOCKS") {
        if r.kind == "BLOCK" {
            defs.insert(r.block.clone(), (r.xy(10).unwrap_or([0., 0.]), Vec::new()));
        } else if r.kind != "ENDBLK"
            && let Some(x) = defs.get_mut(&r.block)
        {
            x.1.push(r);
        }
    }
    let reader = Reader {
        layers,
        patterns,
        defs,
        fonts,
    };
    let mut out = Blocks::default();
    let identity: Affine = [1., 0., 0., 0., 1., 0.];
    for (h, r) in rec.iter().filter(|(_, r)| r.kind == "INSERT") {
        let up = reader.resolve(
            r,
            &Inherit {
                layer: "0".into(),
                weight: -3,
                pattern: Pattern::Continuous,
            },
        );
        let mut bl = Block {
            root: h.clone(),
            index: r.index,
            ..Block::default()
        };
        reader.walk(r, h, &identity, &up, 0, &mut bl);
        for (i, p) in bl.parts.iter().enumerate() {
            out.at.insert(p.path.clone(), (h.clone(), i));
        }
        out.roots.insert(h.clone(), bl);
    }
    out
}

/// Sample points of a part (for view grouping and placement).
pub(super) fn part_points(g: &Geom) -> Vec<P> {
    match g {
        Geom::Line(a, b) => vec![*a, *b],
        Geom::Arc(c, r, a0, w) => (0..=8)
            .map(|i| {
                add(
                    *c,
                    mul(
                        [
                            (a0 + w * i as f64 / 8.).cos(),
                            (a0 + w * i as f64 / 8.).sin(),
                        ],
                        *r,
                    ),
                )
            })
            .collect(),
        Geom::Circle(c, r) => (0..8)
            .map(|i| {
                add(
                    *c,
                    mul(
                        [(TAU * i as f64 / 8.).cos(), (TAU * i as f64 / 8.).sin()],
                        *r,
                    ),
                )
            })
            .collect(),
        Geom::Path(p, _) | Geom::Mask(p) => p.clone(),
        Geom::Text(t, _) => {
            let half = mul(t.dir, t.width / 2.);
            vec![sub(t.center, half), add(t.center, half)]
        }
        Geom::Head(h) => h.to_vec(),
        Geom::Dot(c, _) => vec![*c],
    }
}

/// Points of a member that is a part of a block reference (None for any other member).
pub(super) fn member_points(cx: &Ctx, m: &MdExpressionMember) -> Option<Vec<P>> {
    let path = m.native_member_id.as_deref()?;
    cx.blocks.part(path).map(|p| part_points(&p.geom))
}

// ---------------------------------------------------------------- reading helpers

fn card(id: &str) -> Result<&'static MdRepresentationCard> {
    representation::card_ref(id)
}
fn param(c: &MdRepresentationCard, name: &str) -> Result<f64> {
    representation::param(c, name)
}
fn values<'a>(c: &'a MdRepresentationCard, entry: &str) -> Vec<&'a str> {
    c.lexicon
        .iter()
        .flatten()
        .filter(|l| l.entry == entry)
        .flat_map(|l| l.values.iter().map(String::as_str))
        .collect()
}
fn compact(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

impl Block {
    fn texts(&self) -> Vec<(usize, &TextFact, &str)> {
        self.parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match &p.geom {
                Geom::Text(t, f) => Some((i, t.as_ref(), f.as_str())),
                _ => None,
            })
            .collect()
    }
    fn lines(&self) -> Vec<(usize, P, P)> {
        self.parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match &p.geom {
                Geom::Line(a, b) => Some((i, *a, *b)),
                Geom::Path(v, false) if v.len() == 2 => Some((i, v[0], v[1])),
                _ => None,
            })
            .collect()
    }
    fn heads(&self) -> Vec<(usize, [P; 3])> {
        self.parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match &p.geom {
                Geom::Head(h) => Some((i, *h)),
                _ => None,
            })
            .collect()
    }
    fn dots(&self) -> Vec<(usize, P, f64)> {
        self.parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match &p.geom {
                Geom::Dot(c, r) => Some((i, *c, *r)),
                _ => None,
            })
            .collect()
    }
    fn circles(&self) -> Vec<(usize, P, f64)> {
        self.parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match &p.geom {
                Geom::Circle(c, r) => Some((i, *c, *r)),
                _ => None,
            })
            .collect()
    }
    fn bbox(&self, which: impl Fn(&Part) -> bool) -> Option<[f64; 4]> {
        bbox_of(
            &self
                .parts
                .iter()
                .filter(|p| which(p))
                .flat_map(|p| part_points(&p.geom))
                .collect::<Vec<_>>(),
        )
    }
    /// The thin and thick line weights of the block's curves, when the drawing gives two classes
    /// at least `ratio` apart.
    fn weight_classes(&self, ratio: f64) -> Option<(i32, i32)> {
        let ws: Vec<i32> = self
            .parts
            .iter()
            .filter(|p| {
                !matches!(
                    p.geom,
                    Geom::Text(..) | Geom::Head(_) | Geom::Dot(..) | Geom::Mask(_)
                )
            })
            .map(|p| p.weight)
            .filter(|w| *w > 0)
            .collect();
        let (lo, hi) = (*ws.iter().min()?, *ws.iter().max()?);
        (f64::from(hi) >= ratio * f64::from(lo)).then_some((lo, hi))
    }
}

/// Line type of a part as a member with `role`: its own pattern, the role rule for continuous
/// lines, or the weight class when the drawing gives line weights.
fn part_type(p: &Part, role: &str, thick: Option<bool>) -> &'static str {
    match &p.geom {
        Geom::Text(..) => return "text",
        Geom::Head(_) | Geom::Dot(..) | Geom::Mask(_) => return "fill",
        _ => {}
    }
    match (p.pattern, thick) {
        (Pattern::Dashed, Some(true)) => "thick_dashed",
        (Pattern::Dashed, _) => "thin_dashed",
        (Pattern::DashDot, Some(true)) => "thick_dash_dot",
        (Pattern::DashDot, _) => "thin_dash_dot",
        (Pattern::DashDoubleDot, _) => "thin_dash_double_dot",
        (Pattern::Continuous | Pattern::ByBlock, Some(true)) => "thick_continuous",
        (Pattern::Continuous | Pattern::ByBlock, Some(false)) => "thin_continuous",
        (Pattern::Continuous | Pattern::ByBlock, None) => member_line_type("continuous", role),
        _ => "unknown",
    }
}

fn is_label(t: &TextFact) -> bool {
    let s = compact(&t.dec.text);
    matches!(t.lex, Lex::Roman | Lex::Letters)
        && !s.is_empty()
        && s.chars().count() <= 3
        && s.chars().all(|c| c.is_ascii_uppercase())
}
fn is_integer(t: &TextFact) -> bool {
    let s = compact(&t.dec.text);
    !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit())
}
fn horizontal(a: P, b: P) -> bool {
    (a[1] - b[1]).abs() <= 1e-3 * dist(a, b).max(1e-9)
}
fn vertical(a: P, b: P) -> bool {
    (a[0] - b[0]).abs() <= 1e-3 * dist(a, b).max(1e-9)
}

/// A chain of at most `max` segments from `start` (joined end to end within `tol`), ending where no
/// further segment continues; returns the segment indices and the far end.
fn chain_from(
    lines: &[(usize, P, P)],
    start: P,
    skip: &BTreeSet<usize>,
    tol: f64,
    max: usize,
) -> (Vec<usize>, P) {
    let mut at = start;
    let mut used: Vec<usize> = Vec::new();
    while used.len() < max {
        let next = lines.iter().find(|(i, a, b)| {
            !skip.contains(i)
                && !used.contains(i)
                && (dist(*a, at) <= tol || dist(*b, at) <= tol)
                && dist(*a, *b) > tol
        });
        let Some((i, a, b)) = next else { break };
        used.push(*i);
        at = if dist(*a, at) <= tol { *b } else { *a };
    }
    (used, at)
}

// ---------------------------------------------------------------- recognition plans

/// The surface texture requirement written in a technical-requirement block.
struct GeneralTexture {
    lines: Vec<usize>,
    texts: Vec<usize>,
    value: Option<usize>,
    label: String,
}

struct Requirement {
    heading: usize,
    items: Vec<Vec<usize>>,
    texture: Option<GeneralTexture>,
}

struct Note {
    texts: Vec<usize>,
    lines: Vec<usize>,
    head: Option<usize>,
    /// The leader's feature end.
    end: P,
    kind: &'static str,
    label: String,
}

struct Cutting {
    letter: String,
    strokes: Vec<usize>,
    shafts: Vec<usize>,
    heads: Vec<usize>,
    letters: Vec<usize>,
    turns: Vec<f64>,
}

struct Indicator {
    label: String,
    circle: usize,
    centre: P,
    radius: f64,
    lines: Vec<usize>,
    text: usize,
}

struct Title {
    label: usize,
    scale: usize,
    line: Option<usize>,
}

struct Item {
    number: String,
    text: usize,
    lines: Vec<usize>,
    circle: Option<usize>,
    terminator: usize,
    at: P,
}

struct Row {
    item: Option<String>,
    cells: Vec<(String, String, usize)>,
}
struct PartsList {
    lines: Vec<usize>,
    header: Vec<usize>,
    rows: Vec<Row>,
    bottom: bool,
}

struct FrameRow {
    kind: &'static str,
    glyph: usize,
    cells: Vec<[f64; 4]>,
    fields: Vec<Vec<usize>>,
}
struct Frame {
    rows: Vec<FrameRow>,
    cell_lines: Vec<usize>,
    bbox: [f64; 4],
    height: f64,
    /// Height of the characteristic text.
    text: f64,
    font: String,
}

/// A leader drawn without text: segments chained to a filled head from a free start.
struct Leader {
    lines: Vec<usize>,
    head: usize,
    tip: P,
}

/// The shelf a text rests on and the leader leaving it: segments, feature end, head.
struct Shelf {
    shelf: usize,
    segs: Vec<usize>,
    end: P,
    head: Option<usize>,
}

/// A curve of a block drawn as part geometry: part, expression kind, member role, thick class.
type Class = (usize, &'static str, &'static str, Option<bool>);

// ---------------------------------------------------------------- technical requirements

/// An item number at the start of a text: digits and a separator of the card, not followed by a
/// digit (a decimal such as 1.5 is not an item number).
fn starts_item(text: &str, seps: &[&str]) -> bool {
    let t = text.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return false;
    }
    let rest = t[digits..].trim_start();
    seps.iter().any(|s| {
        rest.strip_prefix(s)
            .is_some_and(|r| !r.trim_start().starts_with(|c: char| c.is_ascii_digit()))
    })
}

fn requirement(bl: &Block) -> Result<Option<Requirement>> {
    let c = card("technical_note")?;
    let row_join = param(c, "row_join")?;
    let seps = values(c, "item_separator");
    let texts = bl.texts();
    let Some(&(heading, hd, _)) = texts
        .iter()
        .filter(|t| t.1.lex == Lex::Heading)
        .max_by(|x, y| x.1.center[1].total_cmp(&y.1.center[1]))
    else {
        return Ok(None);
    };
    // A surface texture symbol inside the block (a nested reference, or loose strokes) is the
    // general requirement for the remaining surfaces: partitioned out with the texts it holds.
    let mut texture = None;
    let mut taken: BTreeSet<usize> = BTreeSet::from([heading]);
    let reach = param(card("surface_texture_symbol")?, "value_reach")?;
    let words = values(card("surface_texture_symbol")?, "general_word");
    let groups: Vec<String> = bl.nested.clone();
    for g in groups
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(bl.root.as_str()))
    {
        let own = |p: &Part| p.parent == g;
        let lines: Vec<(usize, P, P)> = bl
            .lines()
            .into_iter()
            .filter(|(i, ..)| own(&bl.parts[*i]))
            .collect();
        let circles: Vec<(P, f64)> = bl
            .circles()
            .into_iter()
            .filter(|(i, ..)| own(&bl.parts[*i]))
            .map(|x| (x.1, x.2))
            .collect();
        let segs: Vec<(P, P)> = lines.iter().map(|x| (x.1, x.2)).collect();
        let Some(tex) = symbols::texture_of(&segs, &circles) else {
            continue;
        };
        let opening = add(tex.tip, mul(tex.axis, 0.7 * tex.size));
        // a nested symbol holds its own texts; strokes written in the block itself take the value
        // at their opening and the "remaining surfaces" words on its row
        let nested = g != bl.root;
        let value = texts
            .iter()
            .filter(|(i, t, _)| {
                !taken.contains(i)
                    && matches!(t.lex, Lex::Number | Lex::ToleranceValue)
                    && ((nested && own(&bl.parts[*i]))
                        || dist(t.center, opening) < reach * tex.size)
            })
            .min_by(|x, y| dist(x.1.center, opening).total_cmp(&dist(y.1.center, opening)))
            .map(|x| x.0);
        let row_y = value
            .and_then(|v| texts.iter().find(|t| t.0 == v))
            .map_or(tex.tip[1], |t| t.1.center[1]);
        let other: Vec<usize> = texts
            .iter()
            .filter(|(i, t, _)| {
                !taken.contains(i)
                    && Some(*i) != value
                    && if nested {
                        own(&bl.parts[*i])
                    } else {
                        words.iter().any(|w| t.dec.text.contains(w))
                            && (t.center[1] - row_y).abs() < tex.size
                    }
            })
            .map(|x| x.0)
            .collect();
        let near: Vec<usize> = other.iter().copied().chain(value).collect();
        let general = near.iter().any(|i| {
            texts
                .iter()
                .any(|t| t.0 == *i && words.iter().any(|w| t.1.dec.text.contains(w)))
        });
        let v = value
            .and_then(|i| texts.iter().find(|t| t.0 == i))
            .map_or_else(|| "√".to_string(), |t| t.1.dec.text.clone());
        let label = match words.first().filter(|_| general) {
            Some(w) => format!("{w} {v}"),
            None => v,
        };
        taken.extend(near.iter().copied());
        texture = Some(GeneralTexture {
            lines: lines.iter().map(|x| x.0).collect(),
            texts: other,
            value,
            label,
        });
        break;
    }
    let h = hd.height;
    let mut rest: Vec<(usize, &TextFact)> = texts
        .iter()
        .filter(|(i, t, _)| !taken.contains(i) && t.center[1] < hd.center[1] - row_join * h)
        .map(|x| (x.0, x.1))
        .collect();
    rest.sort_by(|a, b| {
        b.1.center[1]
            .total_cmp(&a.1.center[1])
            .then(a.1.center[0].total_cmp(&b.1.center[0]))
    });
    let mut rows: Vec<Vec<(usize, &TextFact)>> = Vec::new();
    for t in rest {
        match rows.iter_mut().find(|r| {
            (r[0].1.center[1] - t.1.center[1]).abs() < row_join * r[0].1.height.max(t.1.height)
        }) {
            Some(r) => r.push(t),
            None => rows.push(vec![t]),
        }
    }
    let mut items: Vec<Vec<usize>> = Vec::new();
    for mut r in rows {
        r.sort_by(|a, b| a.1.center[0].total_cmp(&b.1.center[0]));
        let first = &r[0].1;
        if items.is_empty() || first.lex == Lex::ListNumber || starts_item(&first.dec.text, &seps) {
            items.push(r.iter().map(|x| x.0).collect());
        } else if let Some(last) = items.last_mut() {
            last.extend(r.iter().map(|x| x.0));
        }
    }
    Ok(Some(Requirement {
        heading,
        items,
        texture,
    }))
}

// ---------------------------------------------------------------- leader notes and item numbers

/// Text resting on a horizontal shelf line (the leader's reference line), and the leader that
/// leaves the shelf at one end; the feature end of the leader is a filled head, a dot or the free
/// end.
fn shelf_leader(bl: &Block, text: &TextFact, c: &MdRepresentationCard) -> Result<Option<Shelf>> {
    let above = param(c, "shelf_text_offset_max")?;
    let margin = param(c, "shelf_margin")?;
    let join = param(c, "join")? * text.height;
    let max = param(c, "leader_segments_max")? as usize;
    let reach = param(c, "head_reach")?;
    let lines = bl.lines();
    let shelf = lines
        .iter()
        .filter(|(_, a, b)| {
            horizontal(*a, *b) && {
                let dy = text.center[1] - a[1];
                dy > 0.
                    && dy <= above * text.height
                    && text.center[0] >= a[0].min(b[0]) - margin * text.height
                    && text.center[0] <= a[0].max(b[0]) + margin * text.height
            }
        })
        .min_by(|x, y| (text.center[1] - x.1[1]).total_cmp(&(text.center[1] - y.1[1])));
    let Some(&(si, sa, sb)) = shelf else {
        return Ok(None);
    };
    for start in [sa, sb] {
        let skip = BTreeSet::from([si]);
        let (segs, end) = chain_from(&lines, start, &skip, join, max);
        if segs.is_empty() {
            continue;
        }
        let head = bl.heads().into_iter().find(|(_, h)| {
            let (tip, _, len) = head_axis(h);
            dist(tip, end) <= reach * len || dist(mid(h[1], h[2]), end) <= reach * len
        });
        let end = head.map_or(end, |h| h.1[0]);
        return Ok(Some(Shelf {
            shelf: si,
            segs,
            end,
            head: head.map(|h| h.0),
        }));
    }
    Ok(None)
}

fn leader_note(bl: &Block) -> Result<Option<Note>> {
    let c = card("leader_note")?;
    let texts = bl.texts();
    if texts.is_empty() || !bl.circles().is_empty() {
        return Ok(None);
    }
    // the text nearest above a shelf carries the note; other texts on the same shelf join it
    for (ti, t, _) in &texts {
        if is_label(t) || is_integer(t) {
            continue;
        }
        let Some(Shelf {
            shelf,
            segs,
            end,
            head,
        }) = shelf_leader(bl, t, c)?
        else {
            continue;
        };
        let mut all: Vec<usize> = texts
            .iter()
            .filter(|x| (x.1.center[1] - t.center[1]).abs() < 0.5 * t.height || x.0 == *ti)
            .map(|x| x.0)
            .collect();
        all.sort();
        if all.len() != texts.len() {
            continue;
        }
        let label = all
            .iter()
            .filter_map(|i| texts.iter().find(|x| x.0 == *i))
            .map(|x| x.1.dec.text.clone())
            .collect::<Vec<_>>()
            .join(" ");
        let kind = tx::dimension_kind(&label)
            .filter(|k| {
                matches!(
                    *k,
                    "thread_note" | "dimension_chamfer" | "dimension_diameter" | "dimension_radius"
                )
            })
            .unwrap_or("leader_note");
        let mut lines = vec![shelf];
        lines.extend(segs);
        // the block is the note and nothing else
        if lines.len() + all.len() + usize::from(head.is_some()) != bl.parts.len() {
            continue;
        }
        return Ok(Some(Note {
            texts: all,
            lines,
            head,
            end,
            kind,
            label,
        }));
    }
    Ok(None)
}

fn item_number(bl: &Block) -> Result<Option<Item>> {
    let c = card("item_number")?;
    let (umin, umax) = (param(c, "underline_min")?, param(c, "underline_max")?);
    let above = param(c, "text_above_line_max")?;
    let dot_max = param(c, "dot_radius_max")?;
    let (cmin, cmax) = (
        param(c, "circle_radius_min")?,
        param(c, "circle_radius_max")?,
    );
    let join = param(c, "join")?;
    let texts = bl.texts();
    let [(ti, t, _)] = texts.as_slice() else {
        return Ok(None);
    };
    if !is_integer(t) {
        return Ok(None);
    }
    let h = t.height;
    let lines = bl.lines();
    // reference: an underline under the number, or a circle around it
    let underline = lines.iter().find(|(_, a, b)| {
        let l = dist(*a, *b);
        horizontal(*a, *b)
            && (umin * h..=umax * h).contains(&l)
            && t.center[1] - a[1] > 0.
            && t.center[1] - a[1] <= above * h
            && t.center[0] > a[0].min(b[0])
            && t.center[0] < a[0].max(b[0])
    });
    let circle = bl
        .circles()
        .into_iter()
        .find(|(_, cc, r)| dist(*cc, t.center) < 0.25 * h && (cmin * h..=cmax * h).contains(r));
    let starts: Vec<(Option<usize>, P)> = match (underline, circle) {
        (Some(&(i, a, b)), _) => vec![(Some(i), a), (Some(i), b)],
        (None, Some((_, cc, r))) => lines
            .iter()
            .flat_map(|(_, a, b)| [*a, *b])
            .filter(|p| (dist(*p, cc) - r).abs() <= join * h)
            .map(|p| (None, p))
            .collect(),
        _ => vec![],
    };
    for (under, start) in starts {
        let skip: BTreeSet<usize> = under.into_iter().collect();
        let (segs, end) = chain_from(&lines, start, &skip, join * h, 2);
        if segs.is_empty() {
            continue;
        }
        let dot = bl
            .dots()
            .into_iter()
            .find(|(_, dc, r)| *r <= dot_max * h && dist(*dc, end) <= r + join * h);
        let head = bl.heads().into_iter().find(|(_, hd)| {
            dist(hd[0], end) <= join * h || dist(mid(hd[1], hd[2]), end) <= join * h
        });
        let (terminator, at) = match (dot, head) {
            (Some((i, dc, _)), _) => (i, dc),
            (None, Some((i, hd))) => (i, hd[0]),
            _ => continue,
        };
        let mut ls: Vec<usize> = under.into_iter().collect();
        ls.extend(segs);
        let used = ls.len() + 2 + usize::from(circle.is_some());
        if used != bl.parts.len() {
            continue;
        }
        return Ok(Some(Item {
            number: compact(&t.dec.text),
            text: *ti,
            lines: ls,
            circle: circle.map(|x| x.0),
            terminator,
            at,
        }));
    }
    Ok(None)
}

// ---------------------------------------------------------------- view callouts

fn cutting_plane(bl: &Block) -> Result<Option<Cutting>> {
    let c = card("view_callout")?;
    let (smin, smax) = (param(c, "stroke_min")?, param(c, "stroke_max")?);
    let thick = param(c, "thick_ratio_min")?;
    let perp = param(c, "perpendicular_cos_max")?;
    let join = param(c, "join")?;
    let reach = param(c, "letter_reach")?;
    let texts = bl.texts();
    if texts.len() < 2 || !texts.iter().all(|t| is_label(t.1)) {
        return Ok(None);
    }
    let letter = compact(&texts[0].1.dec.text);
    if texts.iter().any(|t| compact(&t.1.dec.text) != letter) || !bl.circles().is_empty() {
        return Ok(None);
    }
    let h = texts.iter().map(|t| t.1.height).fold(0., f64::max);
    let tol = join * h;
    let lines = bl.lines();
    let Some((thin, _)) = bl.weight_classes(thick) else {
        return Ok(None);
    };
    let strokes: Vec<(usize, P, P)> = lines
        .iter()
        .copied()
        .filter(|(i, a, b)| {
            f64::from(bl.parts[*i].weight) >= thick * f64::from(thin)
                && (smin * h..=smax * h).contains(&dist(*a, *b))
        })
        .collect();
    if strokes.len() < 2 {
        return Ok(None);
    }
    // strokes on one cutting path: collinear neighbours or joined at a turn
    let mut turns = Vec::new();
    for (k, (_, a, b)) in strokes.iter().enumerate() {
        for (_, p, q) in strokes.iter().skip(k + 1) {
            let shared = [a, b]
                .iter()
                .any(|x| dist(**x, *p) <= tol || dist(**x, *q) <= tol);
            let cos = dot(unit(sub(*b, *a)), unit(sub(*q, *p))).abs();
            if shared && cos < 0.999 {
                turns.push(cos);
            }
        }
    }
    let on_path = |s: &(usize, P, P)| {
        strokes.iter().any(|o| {
            o.0 != s.0
                && ([o.1, o.2]
                    .iter()
                    .any(|x| dist(*x, s.1) <= tol || dist(*x, s.2) <= tol)
                    || (cross(unit(sub(s.2, s.1)), unit(sub(o.2, o.1))).abs() < 0.01
                        && project(o.1, s.1, s.2).1 <= tol))
        })
    };
    if !strokes.iter().all(on_path) {
        return Ok(None);
    }
    // arrows at outer ends: a thin shaft perpendicular to the stroke from its end to a filled head
    let heads = bl.heads();
    let stroke_ids: BTreeSet<usize> = strokes.iter().map(|s| s.0).collect();
    let mut shafts: BTreeSet<usize> = BTreeSet::new();
    let mut used_heads: BTreeSet<usize> = BTreeSet::new();
    for (_, a, b) in &strokes {
        for end in [*a, *b] {
            for (li, p, q) in lines.iter().filter(|l| !stroke_ids.contains(&l.0)) {
                let (near, far) = if dist(*p, end) <= tol {
                    (*p, *q)
                } else if dist(*q, end) <= tol {
                    (*q, *p)
                } else {
                    continue;
                };
                if dot(unit(sub(far, near)), unit(sub(*b, *a))).abs() > perp {
                    continue;
                }
                if let Some((hi, _)) = heads.iter().find(|(_, hd)| {
                    let (tip, dir, len) = head_axis(hd);
                    dist(tip, far) <= 0.25 * len + tol && dot(dir, unit(sub(far, near))) > 0.9
                }) {
                    shafts.insert(*li);
                    used_heads.insert(*hi);
                }
            }
        }
    }
    // every letter stands at a stroke end, a shaft end or a turn
    let ends: Vec<P> = strokes
        .iter()
        .flat_map(|s| [s.1, s.2])
        .chain(
            shafts
                .iter()
                .filter_map(|i| lines.iter().find(|l| l.0 == *i))
                .flat_map(|l| [l.1, l.2]),
        )
        .collect();
    if !texts
        .iter()
        .all(|t| ends.iter().any(|e| dist(*e, t.1.center) <= reach * h))
    {
        return Ok(None);
    }
    let mut used: BTreeSet<usize> = stroke_ids.clone();
    used.extend(shafts.iter().copied());
    used.extend(used_heads.iter().copied());
    used.extend(texts.iter().map(|t| t.0));
    if used.len() != bl.parts.len() {
        return Ok(None);
    }
    Ok(Some(Cutting {
        letter,
        strokes: stroke_ids.into_iter().collect(),
        shafts: shafts.into_iter().collect(),
        heads: used_heads.into_iter().collect(),
        letters: texts.iter().map(|t| t.0).collect(),
        turns,
    }))
}

fn detail_indicator(bl: &Block) -> Result<Option<Indicator>> {
    let c = card("view_callout")?;
    let on = param(c, "circle_leader_on")?;
    let reach = param(c, "label_reach")?;
    let datum = param(c, "datum_letter_reach")?;
    let join = param(c, "join")?;
    let texts = bl.texts();
    let circles = bl.circles();
    let ([(ti, t, _)], [(ci, cc, r)]) = (texts.as_slice(), circles.as_slice()) else {
        return Ok(None);
    };
    if !is_label(t) || dist(t.center, *cc) <= datum * r {
        return Ok(None);
    }
    let lines = bl.lines();
    let tol = join * t.height;
    for (li, a, b) in &lines {
        let (near, far) = if (dist(*a, *cc) - r).abs() <= on * r {
            (*a, *b)
        } else if (dist(*b, *cc) - r).abs() <= on * r {
            (*b, *a)
        } else {
            continue;
        };
        if dist(far, *cc) <= dist(near, *cc) {
            continue;
        }
        let mut used = vec![*li];
        // an optional shelf from the leader's far end, the label resting on it
        let shelf = lines.iter().find(|(si, p, q)| {
            si != li
                && horizontal(*p, *q)
                && (dist(*p, far) <= tol || dist(*q, far) <= tol)
                && t.center[1] > p[1]
                && t.center[1] - p[1] <= reach * t.height
                && t.center[0] >= p[0].min(q[0]) - t.height
                && t.center[0] <= p[0].max(q[0]) + t.height
        });
        if let Some(s) = shelf {
            used.push(s.0);
        } else if dist(t.center, far) > reach * t.height {
            continue;
        }
        if used.len() + 2 != bl.parts.len() {
            continue;
        }
        return Ok(Some(Indicator {
            label: compact(&t.dec.text),
            circle: *ci,
            centre: *cc,
            radius: *r,
            lines: used,
            text: *ti,
        }));
    }
    Ok(None)
}

/// A viewing-direction arrow drawn as one block whose head is a filled triangle of any encoding
/// (a solid-filled HATCH as well as a SOLID): one shaft ending at the head, one capital letter
/// beside it (card view_callout, variant arrow_block). Returns the letter, shaft and head.
fn view_arrow(bl: &Block) -> Result<Option<(usize, usize, usize)>> {
    let c = card("view_callout")?;
    let reach = param(c, "letter_reach")?;
    let join = param(c, "join")?;
    let (texts, lines, heads) = (bl.texts(), bl.lines(), bl.heads());
    let ([(ti, t, _)], [(li, a, z)], [(hi, hd)]) =
        (texts.as_slice(), lines.as_slice(), heads.as_slice())
    else {
        return Ok(None);
    };
    if bl.parts.len() != 3 || !is_label(t) || compact(&t.dec.text).chars().count() != 1 {
        return Ok(None);
    }
    let (tip, dir, len) = head_axis(hd);
    let base = sub(tip, mul(dir, len));
    let tol = join * t.height + 0.25 * len;
    let along = dot(unit(sub(*z, *a)), dir).abs() > 0.99;
    let joined = [*a, *z]
        .iter()
        .any(|q| dist(*q, base) <= tol || dist(*q, tip) <= tol);
    let near = project(t.center, *a, *z).1 < reach * t.height
        || dist(t.center, mid(*a, *z)) < reach * t.height + 0.5 * dist(*a, *z);
    Ok((along && joined && near).then_some((*ti, *li, *hi)))
}

fn view_title(bl: &Block) -> Result<Option<Title>> {
    let c = card("view_callout")?;
    let (gmin, gmax) = (param(c, "title_gap_min")?, param(c, "title_gap_max")?);
    let off = param(c, "title_offset_max")?;
    let texts = bl.texts();
    if texts.len() != 2 {
        return Ok(None);
    }
    let (Some(l), Some(s)) = (
        texts.iter().find(|t| is_label(t.1)),
        texts.iter().find(|t| t.1.lex == Lex::Scale),
    ) else {
        return Ok(None);
    };
    let h = l.1.height.max(s.1.height);
    let d = sub(l.1.center, s.1.center);
    if !(d[1] > gmin * h && d[1] < gmax * h && d[0].abs() < off * h) {
        return Ok(None);
    }
    let lines = bl.lines();
    let line = lines.iter().find(|(_, p, q)| {
        horizontal(*p, *q)
            && p[1] < l.1.center[1]
            && p[1] > s.1.center[1]
            && p[0].min(q[0]) < l.1.center[0]
            && p[0].max(q[0]) > l.1.center[0]
    });
    if bl.parts.len() != 2 + usize::from(line.is_some()) {
        return Ok(None);
    }
    Ok(Some(Title {
        label: l.0,
        scale: s.0,
        line: line.map(|x| x.0),
    }))
}

// ---------------------------------------------------------------- tolerance frames

fn glyph_kind_of(
    c: &'static MdRepresentationCard,
    t: &TextFact,
    font: &str,
) -> Option<&'static str> {
    let s = t.dec.text.trim();
    if s.chars().count() != 1 {
        return None;
    }
    let fonts = values(c, "font_file");
    let gdt = fonts.iter().any(|f| font.contains(f));
    let l = c.lexicon.iter().flatten().find(|l| {
        (l.entry == "glyph" && l.values.iter().any(|v| v == s))
            || (gdt && l.entry == "font_char" && l.values.iter().any(|v| v == s))
    })?;
    c.lexicon
        .iter()
        .flatten()
        .find(|k| k.entry == "kind" && k.concept == l.concept)
        .and_then(|k| k.values.first())
        .map(String::as_str)
}

fn frame(bl: &Block) -> Result<Option<Frame>> {
    let g = card("gdt_characteristic")?;
    let f = card("gdt_frame_symbol")?;
    let inset = param(f, "cell_inset")?;
    let aspect = param(f, "glyph_cell_aspect_max")?;
    let texts = bl.texts();
    let glyphs: Vec<(usize, &TextFact, &'static str, &str)> = texts
        .iter()
        .filter_map(|(i, t, font)| glyph_kind_of(g, t, font).map(|k| (*i, *t, k, *font)))
        .collect();
    if glyphs.is_empty() {
        return Ok(None);
    }
    let lines: Vec<(usize, P, P)> = bl
        .lines()
        .into_iter()
        .filter(|l| dist(l.1, l.2) > 1e-9)
        .collect();
    let tol = 1e-3 * glyphs[0].1.height.max(1.);
    let verts: Vec<(usize, f64, f64, f64)> = lines
        .iter()
        .filter(|l| vertical(l.1, l.2))
        .map(|l| (l.0, l.1[0], l.1[1].min(l.2[1]), l.1[1].max(l.2[1])))
        .collect();
    let hors: Vec<(usize, f64, f64, f64)> = lines
        .iter()
        .filter(|l| horizontal(l.1, l.2))
        .map(|l| (l.0, l.1[1], l.1[0].min(l.2[0]), l.1[0].max(l.2[0])))
        .collect();
    // the line at `v` across the span [lo, hi] (a vertical spans y, a horizontal spans x)
    let spans = |set: &[(usize, f64, f64, f64)], v: f64, lo: f64, hi: f64| {
        set.iter()
            .any(|x| (x.1 - v).abs() <= tol && x.2 <= lo + tol && x.3 >= hi - tol)
    };
    let mut rows = Vec::new();
    let mut cell_lines: BTreeSet<usize> = BTreeSet::new();
    for (gi, t, kind, _) in &glyphs {
        let p = t.center;
        let left = verts
            .iter()
            .filter(|v| v.1 < p[0] && v.2 <= p[1] && v.3 >= p[1])
            .map(|v| v.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let right = verts
            .iter()
            .filter(|v| v.1 > p[0] && v.2 <= p[1] && v.3 >= p[1])
            .map(|v| v.1)
            .fold(f64::INFINITY, f64::min);
        let bottom = hors
            .iter()
            .filter(|x| x.1 < p[1] && x.2 <= p[0] && x.3 >= p[0])
            .map(|x| x.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let top = hors
            .iter()
            .filter(|x| x.1 > p[1] && x.2 <= p[0] && x.3 >= p[0])
            .map(|x| x.1)
            .fold(f64::INFINITY, f64::min);
        if ![left, right, bottom, top].iter().all(|v| v.is_finite()) {
            continue;
        }
        let (w, ht) = (right - left, top - bottom);
        if w <= 0. || ht <= 0. || w.max(ht) > aspect * w.min(ht) {
            continue;
        }
        if !(spans(&hors, bottom, left, right) && spans(&hors, top, left, right)) {
            continue;
        }
        let mut cells = vec![[left, bottom, right, top]];
        loop {
            let x0 = cells.last().unwrap()[2];
            let next = verts
                .iter()
                .filter(|v| v.1 > x0 + tol && v.2 <= bottom + tol && v.3 >= top - tol)
                .map(|v| v.1)
                .fold(f64::INFINITY, f64::min);
            if !(next.is_finite() && spans(&hors, bottom, x0, next) && spans(&hors, top, x0, next))
            {
                break;
            }
            cells.push([x0, bottom, next, top]);
        }
        if cells.len() < 2 {
            continue;
        }
        let shrink = |r: [f64; 4]| {
            [
                r[0] + inset * ht,
                r[1] + inset * ht,
                r[2] - inset * ht,
                r[3] - inset * ht,
            ]
        };
        let fields: Vec<Vec<usize>> = cells[1..]
            .iter()
            .map(|r| {
                texts
                    .iter()
                    .filter(|x| x.0 != *gi && inside(x.1.center, shrink(*r), 0.))
                    .map(|x| x.0)
                    .collect()
            })
            .collect();
        rows.push(FrameRow {
            kind,
            glyph: *gi,
            cells,
            fields,
        });
    }
    if rows.is_empty() {
        return Ok(None);
    }
    let bbox = rows
        .iter()
        .map(|r| {
            [
                r.cells[0][0],
                r.cells[0][1],
                r.cells.last().unwrap()[2],
                r.cells[0][3],
            ]
        })
        .reduce(bbox_union)
        .unwrap();
    let height = rows
        .iter()
        .map(|r| r.cells[0][3] - r.cells[0][1])
        .fold(0., f64::max);
    // the grid: lines inside the frame along a row border or a cell boundary (zero-length strokes
    // on it included); a line leaving the frame is a leader
    for (i, p) in bl.parts.iter().enumerate() {
        let Geom::Line(a, b) = p.geom else { continue };
        if !(inside(a, bbox, tol) && inside(b, bbox, tol)) {
            continue;
        }
        let on_row = rows
            .iter()
            .any(|r| (a[1] - r.cells[0][1]).abs() <= tol || (a[1] - r.cells[0][3]).abs() <= tol);
        let on_col = rows
            .iter()
            .flat_map(|r| r.cells.iter())
            .any(|c| (a[0] - c[0]).abs() <= tol || (a[0] - c[2]).abs() <= tol);
        if (horizontal(a, b) && on_row) || (vertical(a, b) && on_col) || dist(a, b) <= 1e-9 {
            cell_lines.insert(i);
        }
    }
    Ok(Some(Frame {
        rows,
        cell_lines: cell_lines.into_iter().collect(),
        bbox,
        height,
        text: glyphs[0].1.height,
        font: glyphs[0].3.to_string(),
    }))
}

/// Leaders of a block drawn without text: segments from a free start to a filled head, the
/// start on the given border.
fn leaders(
    bl: &Block,
    skip: &BTreeSet<usize>,
    on_border: &dyn Fn(P) -> bool,
    tol: f64,
) -> Vec<Leader> {
    let lines: Vec<(usize, P, P)> = bl
        .lines()
        .into_iter()
        .filter(|l| !skip.contains(&l.0))
        .collect();
    let mut out = Vec::new();
    let mut taken: BTreeSet<usize> = skip.clone();
    for (hi, hd) in bl.heads() {
        let (tip, dir, len) = head_axis(&hd);
        // the segment ending at the head
        let Some(&(li, a, b)) = lines.iter().find(|(i, a, b)| {
            !taken.contains(i)
                && [*a, *b].iter().any(|q| {
                    dist(*q, tip) <= 0.25 * len + tol
                        || dist(*q, sub(tip, mul(dir, len))) <= 0.25 * len + tol
                })
        }) else {
            continue;
        };
        let far = if dist(a, tip) < dist(b, tip) { b } else { a };
        let mut own = BTreeSet::from([li]);
        own.extend(taken.iter().copied());
        let (more, start) = chain_from(&lines, far, &own, tol, 3);
        if !on_border(start) {
            continue;
        }
        let mut segs = vec![li];
        segs.extend(more);
        taken.extend(segs.iter().copied());
        out.push(Leader {
            lines: segs,
            head: hi,
            tip,
        });
    }
    out
}

// ---------------------------------------------------------------- parts list

fn parts_list(bl: &Block) -> Result<Option<PartsList>> {
    let c = card("parts_list")?;
    let words = values(c, "header_word");
    let min_words = param(c, "header_words_min")? as usize;
    let row_join = param(c, "row_join")?;
    let item_cols = values(c, "item_column");
    let texts: Vec<(usize, &TextFact, &str)> = bl
        .texts()
        .into_iter()
        .filter(|t| !t.1.dec.text.trim().is_empty())
        .collect();
    if texts.len() < 2 * min_words {
        return Ok(None);
    }
    let mut rows: Vec<Vec<(usize, &TextFact)>> = Vec::new();
    for (i, t, _) in &texts {
        match rows.iter_mut().find(|r| {
            (r[0].1.center[1] - t.center[1]).abs() < row_join * r[0].1.height.max(t.height)
        }) {
            Some(r) => r.push((*i, t)),
            None => rows.push(vec![(*i, t)]),
        }
    }
    let header_of = |t: &TextFact| {
        let s = compact(&t.dec.text);
        words.iter().find(|w| s == **w).copied()
    };
    let Some(hr) = (0..rows.len())
        .max_by_key(|k| rows[*k].iter().filter(|t| header_of(t.1).is_some()).count())
    else {
        return Ok(None);
    };
    if rows[hr].iter().filter(|t| header_of(t.1).is_some()).count() < min_words || rows.len() < 2 {
        return Ok(None);
    }
    // columns: the vertical lines of the grid across the header row
    let lines = bl.lines();
    let hy = rows[hr][0].1.center[1];
    let mut xs: Vec<f64> = lines
        .iter()
        .filter(|(_, a, b)| vertical(*a, *b) && a[1].min(b[1]) <= hy && a[1].max(b[1]) >= hy)
        .map(|(_, a, _)| a[0])
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    if xs.len() < 3 {
        return Ok(None);
    }
    let column = |x: f64| xs.windows(2).position(|w| x > w[0] && x < w[1]);
    let names: BTreeMap<usize, String> = rows[hr]
        .iter()
        .filter_map(|(_, t)| column(t.center[0]).map(|k| (k, compact(&t.dec.text))))
        .collect();
    let item_col = names
        .iter()
        .find(|(_, n)| item_cols.contains(&n.as_str()))
        .map(|x| *x.0);
    let mut data: Vec<Row> = Vec::new();
    let mut ys: Vec<(f64, Option<i64>)> = Vec::new();
    for (k, r) in rows.iter().enumerate() {
        if k == hr {
            continue;
        }
        let mut cells: Vec<(String, String, usize)> = r
            .iter()
            .map(|(i, t)| {
                let col = column(t.center[0]);
                (
                    col.and_then(|k| names.get(&k).cloned()).unwrap_or_default(),
                    t.dec.text.trim().to_string(),
                    *i,
                )
            })
            .collect();
        cells.sort_by(|a, b| {
            let x = |i: usize| r.iter().find(|t| t.0 == i).map_or(0., |t| t.1.center[0]);
            x(a.2).total_cmp(&x(b.2))
        });
        let item = item_col.and_then(|ic| {
            r.iter()
                .find(|(_, t)| column(t.center[0]) == Some(ic) && is_integer(t))
                .map(|(_, t)| compact(&t.dec.text))
        });
        ys.push((
            r[0].1.center[1],
            item.as_ref().and_then(|s| s.parse::<i64>().ok()),
        ));
        data.push(Row { item, cells });
    }
    // the first column holds consecutive item numbers; the header is below them (numbered
    // upwards) or above them (numbered downwards, a continuation sheet)
    if data.is_empty() || data.iter().any(|r| r.item.is_none()) {
        return Ok(None);
    }
    let bottom = ys.iter().all(|y| y.0 > hy);
    let top = ys.iter().all(|y| y.0 < hy);
    if !bottom && !top {
        return Ok(None);
    }
    ys.sort_by(|a, b| a.0.total_cmp(&b.0));
    let nums: Vec<i64> = ys.iter().filter_map(|y| y.1).collect();
    let up = nums.windows(2).all(|w| w[1] == w[0] + 1);
    let down = nums.windows(2).all(|w| w[1] == w[0] - 1);
    if !((bottom && up) || (top && down)) {
        return Ok(None);
    }
    let mut order: Vec<usize> = (0..data.len()).collect();
    order.sort_by_key(|k| data[*k].item.as_ref().and_then(|s| s.parse::<i64>().ok()));
    let data: Vec<Row> = order
        .into_iter()
        .map(|k| {
            std::mem::replace(
                &mut data[k],
                Row {
                    item: None,
                    cells: vec![],
                },
            )
        })
        .collect();
    Ok(Some(PartsList {
        lines: lines.iter().map(|l| l.0).collect(),
        header: rows[hr].iter().map(|t| t.0).collect(),
        rows: data,
        bottom,
    }))
}

// ---------------------------------------------------------------- detail views and standard parts

/// Classes of the curves of a block drawn as part geometry: pattern first; among continuous
/// curves the thick weight class is outline and the thin parallel lattice is section lining.
fn geometry_classes(bl: &Block, c: &MdRepresentationCard) -> Result<Vec<Class>> {
    let lattice_min = param(c, "hatch_lines_min")? as usize;
    let spacing_tol = param(c, "hatch_spacing_tolerance")?;
    let par = param(c, "parallel_sin_max")?;
    let thick_ratio = param(card("view_callout")?, "thick_ratio_min")?;
    let classes = bl.weight_classes(thick_ratio);
    let thick = |p: &Part| {
        classes.map(|(lo, hi)| p.weight > 0 && (p.weight - hi).abs() < (p.weight - lo).abs())
    };
    // section lining: three or more parallel thin straight lines at an even spacing
    let lines: Vec<(usize, P, P)> = bl
        .lines()
        .into_iter()
        .filter(|(i, a, b)| {
            matches!(bl.parts[*i].pattern, Pattern::Continuous | Pattern::ByBlock)
                && thick(&bl.parts[*i]) != Some(true)
                && dist(*a, *b) > 1e-9
        })
        .collect();
    let mut hatch: BTreeSet<usize> = BTreeSet::new();
    for (i, a, b) in &lines {
        let u = unit(sub(*b, *a));
        if u[0].abs() < 0.05 || u[1].abs() < 0.05 {
            continue; // section lining is oblique
        }
        let n = [-u[1], u[0]];
        let mut fam: Vec<(usize, f64)> = lines
            .iter()
            .filter(|(_, p, q)| cross(u, unit(sub(*q, *p))).abs() <= par)
            .map(|(j, p, _)| (*j, dot(*p, n)))
            .collect();
        if fam.len() < lattice_min || !fam.iter().any(|f| f.0 == *i) {
            continue;
        }
        fam.sort_by(|x, y| x.1.total_cmp(&y.1));
        let gaps: Vec<f64> = fam
            .windows(2)
            .map(|w| w[1].1 - w[0].1)
            .filter(|g| *g > 1e-9)
            .collect();
        let Some(s) = median(gaps) else { continue };
        let o = fam.iter().find(|f| f.0 == *i).map_or(0., |f| f.1);
        let on = |x: f64| {
            let k = ((x - o) / s).round();
            ((x - o) - k * s).abs() <= spacing_tol * s
        };
        if fam.iter().filter(|f| on(f.1)).count() >= lattice_min {
            hatch.insert(*i);
        }
    }
    let mut out = Vec::new();
    for (i, p) in bl.parts.iter().enumerate() {
        if !matches!(p.geom, Geom::Line(..) | Geom::Arc(..) | Geom::Path(..)) {
            continue;
        }
        let (kind, role) = match p.pattern {
            Pattern::DashDot => ("center_axis", "center_line"),
            Pattern::Dashed => ("hidden_contour", "native_member"),
            Pattern::DashDoubleDot => ("phantom_line", "phantom_segment"),
            _ if hatch.contains(&i) => ("section_hatch", "hatch_fill"),
            _ => ("visible_contour", "native_member"),
        };
        out.push((i, kind, role, thick(p)));
    }
    Ok(out)
}

/// A detail view drawn as one block: an enlarged copy of the encircled geometry inside a boundary
/// circle, no text. Paired with the indicator whose circle the boundary is an enlargement of and
/// whose encircled geometry the copy repeats.
fn detail_view<'a>(
    cx: &Ctx,
    bl: &Block,
    indicators: &'a [(String, Indicator)],
) -> Result<Option<(usize, &'a Indicator, f64, usize)>> {
    let c = card("detail_view")?;
    let enclose = param(c, "enclose_tolerance")?;
    let ratio_min = param(c, "ratio_min")?;
    let map_tol = param(c, "map_tolerance")?;
    let fraction = param(c, "matched_fraction_min")?;
    if !bl.texts().is_empty() {
        return Ok(None);
    }
    let Some(&(ci, cc, r)) = bl.circles().iter().max_by(|x, y| x.2.total_cmp(&y.2)) else {
        return Ok(None);
    };
    let inner: Vec<&Part> = bl
        .parts
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != ci)
        .map(|x| x.1)
        .collect();
    if inner.is_empty()
        || !inner
            .iter()
            .flat_map(|p| part_points(&p.geom))
            .all(|q| dist(q, cc) <= r * (1. + enclose))
    {
        return Ok(None);
    }
    let classes = geometry_classes(bl, c)?;
    let outline: Vec<(P, P)> = classes
        .iter()
        .filter(|x| x.1 == "visible_contour")
        .filter_map(|x| match bl.parts[x.0].geom {
            Geom::Line(a, b) => Some((a, b)),
            _ => None,
        })
        .collect();
    if outline.is_empty() {
        return Ok(None);
    }
    let mut best: Option<(usize, &Indicator, f64, usize)> = None;
    for (_, ind) in indicators {
        let k = r / ind.radius;
        if k < ratio_min {
            continue;
        }
        let tol = map_tol * ind.radius;
        let back = |q: P| add(ind.centre, mul(sub(q, cc), 1. / k));
        let hits = outline
            .iter()
            .filter(|(a, b)| {
                let (p, q) = (back(*a), back(*b));
                cx.lines.values().any(|(s, t)| {
                    project(p, *s, *t).1 <= tol
                        && project(q, *s, *t).1 <= tol
                        && on_seg(p, *s, *t, tol).is_some()
                        && on_seg(q, *s, *t, tol).is_some()
                })
            })
            .count();
        if hits as f64 >= fraction * outline.len() as f64
            && best.as_ref().is_none_or(|b| hits > b.3)
        {
            best = Some((ci, ind, k, hits));
        }
    }
    Ok(best)
}

// ---------------------------------------------------------------- emission

fn member(b: &mut Builder, e: usize, bl: &Block, i: usize, role: &str, lt: &str, ev: Vec<String>) {
    b.member(
        e,
        &bl.root,
        Some(bl.parts[i].path.clone()),
        [0., 1.],
        role,
        lt,
        "candidate",
        vec![],
        ev,
    );
}
fn tagged(card: &str, variant: &str, root: &str) -> Vec<String> {
    vec![
        format!("source:{root}"),
        format!("card:{card}"),
        format!("variant:{variant}"),
        "encoding:block".into(),
    ]
}
fn text_of(bl: &Block, i: usize) -> String {
    match &bl.parts[i].geom {
        Geom::Text(t, _) => t.dec.text.trim().to_string(),
        _ => String::new(),
    }
}

/// Composite blocks the whole-block predicates must leave to `read` (a technical-requirement
/// block that also holds a texture symbol), and sheet frames whose texts all lie in the binding
/// margin (card sheet_frame_border). Returns true when `blocks()` must skip the block.
pub(super) fn claimed_first(cx: &Ctx, b: &mut Builder, h: &str) -> bool {
    let Some(bl) = cx.blocks.roots.get(h) else {
        return false;
    };
    if bl.texts().iter().any(|t| t.1.lex == Lex::Heading) {
        return true;
    }
    sheet_frame(cx, b, bl).unwrap_or(false)
}

fn sheet_frame(cx: &Ctx, b: &mut Builder, bl: &Block) -> Result<bool> {
    let c = card("sheet_frame_border")?;
    let area_min = param(c, "border_area_min")?;
    let texts = bl.texts();
    if texts.is_empty() {
        return Ok(false);
    }
    let area = (cx.extent[2] - cx.extent[0]).max(1e-9) * (cx.extent[3] - cx.extent[1]).max(1e-9);
    // rectangles formed by four lines meeting at their corners, or by a closed polyline
    let lines = bl.lines();
    let tol = 1e-4 * ((cx.extent[2] - cx.extent[0]) + (cx.extent[3] - cx.extent[1]));
    let mut rects: Vec<[f64; 4]> = bl
        .parts
        .iter()
        .filter_map(|p| match &p.geom {
            Geom::Path(v, true) => cell_rect(&[v.clone(), vec![v[0]]].concat()),
            _ => None,
        })
        .collect();
    let hs: Vec<(f64, f64, f64)> = lines
        .iter()
        .filter(|l| horizontal(l.1, l.2))
        .map(|l| (l.1[1], l.1[0].min(l.2[0]), l.1[0].max(l.2[0])))
        .collect();
    let vs: Vec<(f64, f64, f64)> = lines
        .iter()
        .filter(|l| vertical(l.1, l.2))
        .map(|l| (l.1[0], l.1[1].min(l.2[1]), l.1[1].max(l.2[1])))
        .collect();
    for (y0, x0, x1) in &hs {
        for (y1, a0, a1) in &hs {
            if y1 <= y0
                || (x0 - a0).abs() > tol
                || (x1 - a1).abs() > tol
                || (x1 - x0) * (y1 - y0) < area_min * area
            {
                continue;
            }
            let side = |x: f64| {
                vs.iter()
                    .any(|v| (v.0 - x).abs() <= tol && v.1 <= y0 + tol && v.2 >= y1 - tol)
            };
            if side(*x0) && side(*x1) {
                rects.push([*x0, *y0, *x1, *y1]);
            }
        }
    }
    // the frame: a rectangle of sheet size inset on every side in a larger one (the paper border);
    // every text lies between the two (the binding margin), none inside the frame
    let big: Vec<[f64; 4]> = rects
        .into_iter()
        .filter(|r| (r[2] - r[0]) * (r[3] - r[1]) >= area_min * area)
        .collect();
    let inset = |i: &[f64; 4], o: &[f64; 4]| {
        i[0] > o[0] + tol && i[1] > o[1] + tol && i[2] < o[2] - tol && i[3] < o[3] - tol
    };
    let Some((inner, outer)) = big
        .iter()
        .flat_map(|i| {
            big.iter()
                .filter(move |o| inset(i, o))
                .map(move |o| (*i, *o))
        })
        .min_by(|x, y| {
            ((x.0[2] - x.0[0]) * (x.0[3] - x.0[1]))
                .total_cmp(&((y.0[2] - y.0[0]) * (y.0[3] - y.0[1])))
        })
    else {
        return Ok(false);
    };
    if texts
        .iter()
        .any(|t| inside(t.1.center, inner, -tol) || !inside(t.1.center, outer, tol))
    {
        return Ok(false);
    }
    let words: Vec<String> = texts.iter().map(|t| compact(&t.1.dec.text)).collect();
    let e = b.expr(
        "sheet_frame",
        "图框",
        "card:sheet_frame_border/border_with_margin_columns",
        "candidate",
        vec![],
        tagged("sheet_frame_border", "border_with_margin_columns", &bl.root),
        vec![],
    );
    b.member(
        e,
        &bl.root,
        None,
        [0., 1.],
        "frame_border",
        "block",
        "candidate",
        vec![],
        vec![format!("margin_texts:{}", words.join("|"))],
    );
    Ok(true)
}

/// Recognise every drafting object stored as one block reference the whole-block predicates left;
/// returns the view titles found (detail titles) for view grouping.
pub(super) fn read(
    cx: &Ctx,
    b: &mut Builder,
    def: &Deferred,
    anchors: &mut BTreeMap<usize, P>,
) -> Result<Vec<ViewTitle>> {
    let held: BTreeSet<&String> = def
        .textures
        .iter()
        .chain(def.glyphs.iter().map(|g| &g.0))
        .chain(def.cells.iter().flat_map(|c| c.0.iter()))
        .collect();
    let boxes: BTreeSet<&String> = def.boxes.iter().collect();
    let mut order: Vec<&Block> = cx
        .blocks
        .roots
        .values()
        .filter(|bl| !b.used(&bl.root) && !held.contains(&bl.root))
        .collect();
    order.sort_by_key(|bl| bl.index);
    let mut titles = Vec::new();
    let mut title_exprs: Vec<(usize, String)> = Vec::new();
    let mut indicators: Vec<(String, Indicator)> = Vec::new();
    let mut items: Vec<(usize, String, P)> = Vec::new();
    let mut lists: Vec<(usize, Vec<Row>)> = Vec::new();
    let mut frames: Vec<(&Block, Frame)> = Vec::new();
    for bl in &order {
        if boxes.contains(&bl.root) {
            continue;
        }
        if let Some(r) = requirement(bl)? {
            emit_requirement(b, bl, r)?;
        } else if let Some(p) = parts_list(bl)? {
            let e = emit_parts_list(b, bl, &p);
            lists.push((e, p.rows));
        } else if let Some(f) = frame(bl)? {
            frames.push((bl, f));
        } else if let Some(cut) = cutting_plane(bl)? {
            emit_cutting(b, bl, &cut);
        } else if let Some((letter, shaft, head)) = view_arrow(bl)? {
            let e = b.expr(
                "view_callout",
                &text_of(bl, letter),
                "card:view_callout/arrow_block",
                "candidate",
                vec!["view_direction".into()],
                [
                    tagged("view_callout", "arrow_block", &bl.root),
                    vec!["concept:DC-G2-072".into()],
                ]
                .concat(),
                vec![],
            );
            member(
                b,
                e,
                bl,
                shaft,
                "view_arrow",
                part_type(&bl.parts[shaft], "view_arrow", Some(false)),
                vec![],
            );
            member(b, e, bl, head, "arrowhead", "fill", vec![]);
            member(b, e, bl, letter, "view_letter", "text", vec![]);
        } else if let Some(t) = view_title(bl)? {
            let (e, vt) = emit_title(b, bl, &t);
            title_exprs.push((e, vt.label.clone()));
            titles.push(vt);
        } else if let Some(ind) = detail_indicator(bl)? {
            let e = emit_indicator(b, bl, &ind);
            anchors.insert(e, ind.centre);
            indicators.push((bl.root.clone(), ind));
        } else if let Some(it) = item_number(bl)? {
            let e = emit_item(b, bl, &it);
            anchors.insert(e, it.at);
            items.push((e, it.number.clone(), it.at));
        } else if let Some(n) = leader_note(bl)? {
            let e = emit_note(cx, b, bl, &n);
            anchors.insert(e, n.end);
        }
    }
    // tolerance frames, with the leader-only blocks whose start lies on their border
    let loose_leaders: Vec<&Block> = order
        .iter()
        .copied()
        .filter(|bl| !b.used(&bl.root) && bl.texts().is_empty() && !bl.heads().is_empty())
        .collect();
    for (bl, f) in &frames {
        for (e, p) in emit_frame(cx, b, bl, f, &loose_leaders)?.unwrap_or_default() {
            anchors.insert(e, p);
        }
    }
    // detail views drawn as one block, paired with their indicators
    for bl in &order {
        if b.used(&bl.root) {
            continue;
        }
        if let Some((ci, ind, k, hits)) = detail_view(cx, bl, &indicators)? {
            emit_detail_view(b, bl, ci, ind, k, hits)?;
            // the title with the same label: its scale over the measured enlargement is the
            // sheet scale the label assumes (a source finding, never a rejection)
            for (e, label) in &title_exprs {
                let mut it = label.split_whitespace();
                let (Some(name), Some(scale)) = (it.next(), it.next()) else {
                    continue;
                };
                let ratio = scale
                    .split_once(':')
                    .and_then(|(p, q)| Some(p.parse::<f64>().ok()? / q.parse::<f64>().ok()?))
                    .filter(|r| r.is_finite() && *r > 0.);
                if name != ind.label || ratio.is_none() {
                    continue;
                }
                let assumed = k / ratio.unwrap_or(1.);
                let ev = &mut b.exprs[*e].evidence_ids;
                ev.push(format!(
                    "enlargement_measured:{}",
                    fmt_num((k * 1000.).round() / 1000.)
                ));
                ev.push(format!(
                    "sheet_scale_assumed_by_label:1:{}",
                    fmt_num((assumed * 100.).round() / 100.)
                ));
            }
        }
    }
    datum_anchors(cx, b, anchors);
    // item numbers checked against the rows of the parts list; library parts identified by them
    for (e, rows) in &lists {
        let set: BTreeSet<&str> = rows.iter().filter_map(|r| r.item.as_deref()).collect();
        let found: BTreeSet<&str> = items
            .iter()
            .map(|x| x.1.as_str())
            .filter(|n| set.contains(n))
            .collect();
        let pid = b.exprs[*e].id.clone();
        for (ie, n, _) in &items {
            if set.contains(n.as_str()) {
                b.exprs[*ie]
                    .evidence_ids
                    .push(format!("parts_list_row:{n}"));
                b.links.push((*ie, "names_row_of", *e));
            }
        }
        if !found.is_empty() {
            b.exprs[*e].evidence_ids.push(format!(
                "item_numbers_found:{}:{}",
                found.len(),
                set.len()
            ));
        }
        let _ = pid;
    }
    for bl in &order {
        if !b.used(&bl.root) {
            standard_part(cx, b, bl, &items, &lists)?;
        }
    }
    Ok(titles)
}

fn emit_requirement(b: &mut Builder, bl: &Block, r: Requirement) -> Result<()> {
    let topics = card("technical_note")?;
    let ev = tagged("technical_note", "requirement_block", &bl.root);
    let rule = "card:technical_note/requirement_block";
    let e = b.expr(
        "technical_general",
        &text_of(bl, r.heading),
        rule,
        "candidate",
        vec![],
        ev.clone(),
        vec![],
    );
    member(b, e, bl, r.heading, "note_text", "text", vec![]);
    for item in &r.items {
        let text: String = item
            .iter()
            .map(|i| text_of(bl, *i))
            .collect::<Vec<_>>()
            .join("");
        let kind = notation::note_topic(topics, &text);
        let e = b.expr(
            kind,
            &text,
            rule,
            "candidate",
            vec![],
            ev.clone(),
            vec!["Requirement scope and verification remain to be bound".into()],
        );
        for i in item {
            member(b, e, bl, *i, "note_text", "text", vec![]);
        }
    }
    if let Some(GeneralTexture {
        lines,
        texts,
        value,
        label,
    }) = r.texture
    {
        let general = card("surface_texture_symbol")?
            .lexicon
            .iter()
            .flatten()
            .filter(|l| l.entry == "general_word")
            .flat_map(|l| l.values.iter())
            .any(|w| label.starts_with(w.as_str()));
        let e = b.expr(
            "surface_roughness",
            &label,
            "card:surface_texture_symbol/most_surfaces",
            "candidate",
            if general {
                vec!["general_requirement".into()]
            } else {
                vec![]
            },
            tagged("surface_texture_symbol", "most_surfaces", &bl.root),
            vec!["Controlled surface requires the indicated feature".into()],
        );
        for i in lines {
            member(
                b,
                e,
                bl,
                i,
                "roughness_symbol",
                "thin_continuous",
                vec!["texture_symbol_inside_requirement_block".into()],
            );
        }
        if let Some(v) = value {
            member(b, e, bl, v, "roughness_value", "text", vec![]);
        }
        for i in texts {
            member(b, e, bl, i, "note_text", "text", vec![]);
        }
    }
    Ok(())
}

/// The name of the part in a parts-list row (the column the card names), else its second cell.
fn row_name(r: &Row) -> &str {
    let cols = card("parts_list")
        .map(|c| values(c, "name_column"))
        .unwrap_or_default();
    r.cells
        .iter()
        .find(|c| cols.contains(&c.0.as_str()))
        .map_or("", |c| c.1.as_str())
}

fn emit_parts_list(b: &mut Builder, bl: &Block, p: &PartsList) -> usize {
    let names: Vec<String> = p
        .rows
        .iter()
        .map(|r| {
            let name = Some(row_name(r))
                .filter(|n| !n.is_empty())
                .or_else(|| r.cells.get(1).map(|c| c.1.as_str()))
                .unwrap_or("");
            format!("{} {}", r.item.as_deref().unwrap_or(""), name)
        })
        .collect();
    let mut ev = tagged("parts_list", "block_table", &bl.root);
    ev.push(if p.bottom {
        "header_below_rows".into()
    } else {
        "header_above_rows".into()
    });
    for r in &p.rows {
        ev.push(format!(
            "row:{}",
            r.cells
                .iter()
                .map(|c| if c.0.is_empty() {
                    c.1.clone()
                } else {
                    format!("{}={}", c.0, c.1)
                })
                .collect::<Vec<_>>()
                .join("|")
        ));
    }
    let e = b.expr(
        "parts_list",
        &format!("明细栏 {}", names.join("；")),
        "card:parts_list/block_table",
        "candidate",
        vec![],
        ev,
        vec!["Part identities of the rows require the item numbers on the views".into()],
    );
    let thick = bl.weight_classes(1.5);
    for i in &p.lines {
        let t = thick
            .map(|(lo, hi)| (bl.parts[*i].weight - hi).abs() < (bl.parts[*i].weight - lo).abs());
        let lt = part_type(&bl.parts[*i], "table_line", t);
        member(b, e, bl, *i, "table_line", lt, vec![]);
    }
    for i in &p.header {
        member(
            b,
            e,
            bl,
            *i,
            "table_text",
            "text",
            vec!["header_row".into()],
        );
    }
    for r in &p.rows {
        for c in &r.cells {
            member(
                b,
                e,
                bl,
                c.2,
                "table_text",
                "text",
                vec![format!("row:{}", r.item.as_deref().unwrap_or(""))],
            );
        }
    }
    // texts that draw nothing inside the grid are its empty cells
    let grid = bbox_of(
        &p.lines
            .iter()
            .flat_map(|i| part_points(&bl.parts[*i].geom))
            .collect::<Vec<_>>(),
    );
    for (path, q) in &bl.blank {
        if grid.is_some_and(|g| inside(*q, g, 0.)) {
            b.member(
                e,
                &bl.root,
                Some(path.clone()),
                [0., 1.],
                "table_text",
                "text",
                "candidate",
                vec![],
                vec!["empty_cell".into()],
            );
        }
    }
    e
}

fn emit_cutting(b: &mut Builder, bl: &Block, c: &Cutting) {
    let mut ev = tagged("view_callout", "cutting_plane_group", &bl.root);
    ev.push("concept:DC-G2-043".into());
    ev.push("concept:DC-G1-007".into());
    // a turn of the cutting path: at a right angle parallel planes, at another angle intersecting planes
    if c.turns.iter().any(|cos| *cos < 0.1) {
        ev.push("parallel_cutting_planes".into());
        ev.push("concept:DC-G2-015".into());
    }
    if c.turns.iter().any(|cos| *cos >= 0.1) {
        ev.push("intersecting_cutting_planes".into());
        ev.push("concept:DC-G2-016".into());
    }
    let e = b.expr(
        "view_callout",
        &c.letter,
        "card:view_callout/cutting_plane_group",
        "candidate",
        vec!["cutting_plane".into()],
        ev,
        vec![],
    );
    for i in &c.strokes {
        member(
            b,
            e,
            bl,
            *i,
            "cutting_stroke",
            part_type(&bl.parts[*i], "cutting_stroke", Some(true)),
            vec![],
        );
    }
    for i in &c.shafts {
        member(
            b,
            e,
            bl,
            *i,
            "view_arrow",
            part_type(&bl.parts[*i], "view_arrow", Some(false)),
            vec![],
        );
    }
    for i in &c.heads {
        member(b, e, bl, *i, "arrowhead", "fill", vec![]);
    }
    for i in &c.letters {
        member(b, e, bl, *i, "view_letter", "text", vec![]);
    }
}

fn emit_title(b: &mut Builder, bl: &Block, t: &Title) -> (usize, ViewTitle) {
    let (Geom::Text(l, _), Geom::Text(s, _)) = (&bl.parts[t.label].geom, &bl.parts[t.scale].geom)
    else {
        unreachable!("title parts are texts")
    };
    let label = format!("{} {}", compact(&l.dec.text), compact(&s.dec.text));
    let mut ev = tagged("view_callout", "name_with_scale", &bl.root);
    ev.extend([
        "concept:DC-G2-068".to_string(),
        "concept:DC-G1-115".to_string(),
    ]);
    let e = b.expr(
        "view_callout",
        &label,
        "card:view_callout/name_with_scale",
        "candidate",
        vec!["detail_title".into()],
        ev,
        vec![],
    );
    member(b, e, bl, t.label, "view_letter", "text", vec![]);
    member(b, e, bl, t.scale, "scale_text", "text", vec![]);
    if let Some(i) = t.line {
        member(
            b,
            e,
            bl,
            i,
            "view_title",
            part_type(&bl.parts[i], "table_line", None),
            vec![],
        );
    }
    (
        e,
        ViewTitle {
            label,
            center: mid(l.center, s.center),
            height: l.height.max(s.height),
            kind: "detail_view",
            scale: Some(compact(&s.dec.text)),
            sources: vec![bl.root.clone()],
        },
    )
}

fn emit_indicator(b: &mut Builder, bl: &Block, ind: &Indicator) -> usize {
    let mut ev = tagged("view_callout", "detail_indicator", &bl.root);
    ev.push(format!("indicator_radius:{}", fmt_num(ind.radius)));
    let e = b.expr(
        "view_callout",
        &ind.label,
        "card:view_callout/detail_indicator",
        "candidate",
        vec!["detail_indicator".into()],
        ev,
        vec!["The enlarged detail view is bound through the same label".into()],
    );
    member(
        b,
        e,
        bl,
        ind.circle,
        "detail_circle",
        part_type(&bl.parts[ind.circle], "detail_circle", Some(false)),
        vec![],
    );
    for i in &ind.lines {
        member(
            b,
            e,
            bl,
            *i,
            "leader_line",
            part_type(&bl.parts[*i], "leader_line", Some(false)),
            vec![],
        );
    }
    member(b, e, bl, ind.text, "view_letter", "text", vec![]);
    e
}

fn emit_item(b: &mut Builder, bl: &Block, it: &Item) -> usize {
    let variant = if it.circle.is_some() {
        "circle"
    } else {
        "underline"
    };
    let e = b.expr(
        "item_number",
        &it.number,
        &format!("card:item_number/{variant}"),
        "candidate",
        vec![],
        tagged("item_number", variant, &bl.root),
        vec!["The part the number names lies under the leader's terminator".into()],
    );
    member(b, e, bl, it.text, "note_text", "text", vec![]);
    for i in &it.lines {
        member(
            b,
            e,
            bl,
            *i,
            "leader_line",
            part_type(&bl.parts[*i], "leader_line", Some(false)),
            vec![],
        );
    }
    if let Some(c) = it.circle {
        member(
            b,
            e,
            bl,
            c,
            "leader_line",
            part_type(&bl.parts[c], "leader_line", Some(false)),
            vec!["number_circle".into()],
        );
    }
    member(
        b,
        e,
        bl,
        it.terminator,
        "arrowhead",
        "fill",
        vec![match bl.parts[it.terminator].geom {
            Geom::Dot(..) => "dot_terminator".into(),
            _ => "arrow_terminator".into(),
        }],
    );
    e
}

/// The root geometry the leader's feature end stands on: the line the leader continues (a chamfer
/// edge), else a line ending there, else a line or an arc through it.
fn target(cx: &Ctx, p: P, dir: P, tol: f64) -> Option<String> {
    let through: Vec<(&String, &(P, P))> = cx
        .lines
        .iter()
        .filter(|(_, (a, c))| on_seg(p, *a, *c, tol).is_some())
        .collect();
    through
        .iter()
        .find(|(_, (a, c))| cross(unit(sub(*c, *a)), dir).abs() < 0.02)
        .or_else(|| {
            through
                .iter()
                .find(|(_, (a, c))| dist(*a, p) <= tol || dist(*c, p) <= tol)
        })
        .or_else(|| through.first())
        .map(|(id, _)| (*id).clone())
        .or_else(|| {
            cx.rec
                .keys()
                .filter_map(|id| cx.arc(id).map(|a| (id, a)))
                .filter(|(_, (c, r, s0, sw, full))| {
                    (dist(p, *c) - r).abs() <= tol
                        && (*full
                            || ((p[1] - c[1]).atan2(p[0] - c[0]) - s0).rem_euclid(TAU)
                                <= sw + tol / r.max(1e-9))
                })
                .min_by(|x, y| {
                    (dist(p, x.1.0) - x.1.1)
                        .abs()
                        .total_cmp(&(dist(p, y.1.0) - y.1.1).abs())
                })
                .map(|(id, _)| id.clone())
        })
}

fn emit_note(cx: &Ctx, b: &mut Builder, bl: &Block, n: &Note) -> usize {
    let mut ev = tagged("leader_note", "block_object", &bl.root);
    let h = n
        .texts
        .iter()
        .filter_map(|i| match &bl.parts[*i].geom {
            Geom::Text(t, _) => Some(t.height),
            _ => None,
        })
        .fold(0., f64::max);
    let last = n.lines.last().and_then(|i| match bl.parts[*i].geom {
        Geom::Line(a, c) => Some(if dist(a, n.end) < dist(c, n.end) {
            unit(sub(a, c))
        } else {
            unit(sub(c, a))
        }),
        _ => None,
    });
    if let Some(t) = target(cx, n.end, last.unwrap_or([1., 0.]), 0.05 * h) {
        ev.push(format!("leader_end_on:{t}"));
    }
    ev.push(if n.head.is_some() {
        "terminator:filled_arrow".into()
    } else {
        "terminator:none".into()
    });
    let e = b.expr(
        n.kind,
        &n.label,
        "card:leader_note/block_object",
        "candidate",
        tx::qualifiers(&tx::decode(&n.label)),
        ev,
        vec!["The feature the leader points to requires projection evidence".into()],
    );
    let text_role = if n.kind == "leader_note" {
        "note_text"
    } else {
        "dimension_text"
    };
    for i in &n.texts {
        member(b, e, bl, *i, text_role, "text", vec![]);
    }
    for i in &n.lines {
        member(
            b,
            e,
            bl,
            *i,
            "leader_line",
            part_type(&bl.parts[*i], "leader_line", Some(false)),
            vec![],
        );
    }
    if let Some(i) = n.head {
        member(b, e, bl, i, "arrowhead", "fill", vec![]);
    }
    e
}

fn emit_frame(
    cx: &Ctx,
    b: &mut Builder,
    bl: &Block,
    f: &Frame,
    others: &[&Block],
) -> Result<Option<Vec<(usize, P)>>> {
    let c = card("gdt_frame_symbol")?;
    let reach = param(c, "leader_on_border")?;
    let tol = reach * f.height;
    let fb = f.bbox;
    let on_border = |p: P| {
        inside(p, fb, tol)
            && ((p[0] - fb[0]).abs() <= tol
                || (p[0] - fb[2]).abs() <= tol
                || (p[1] - fb[1]).abs() <= tol
                || (p[1] - fb[3]).abs() <= tol)
    };
    let skip: BTreeSet<usize> = f.cell_lines.iter().copied().collect();
    let jtol = 1e-3 * f.height;
    let own = leaders(bl, &skip, &on_border, jtol);
    let mut extra: Vec<(&Block, Leader)> = Vec::new();
    for o in others.iter().filter(|o| !b.used(&o.root)) {
        let ls = leaders(o, &BTreeSet::new(), &on_border, jtol);
        let lines = o.lines().len();
        if !ls.is_empty()
            && ls.iter().map(|l| l.lines.len()).sum::<usize>() == lines
            && ls.len() == o.heads().len()
            && o.parts.len() == lines + ls.len()
        {
            for l in ls {
                extra.push((o, l));
            }
        }
    }
    let n_leaders = own.len() + extra.len();
    let mut first = None;
    let mut extra_anchors: Vec<(usize, P)> = Vec::new();
    for (k, row) in f.rows.iter().enumerate() {
        let fields: Vec<String> = row
            .fields
            .iter()
            .map(|ids| {
                ids.iter()
                    .map(|i| text_of(bl, *i))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.is_empty())
            .collect();
        let label = format!("{} {}", glyph_label(row.kind), fields.join(" | "));
        let mut ev = tagged("gdt_frame_symbol", "line_grid_font_glyph", &bl.root);
        ev.push(format!("glyph_font:{}", f.font));
        ev.push(format!("cells:{}", row.cells.len()));
        if f.rows.len() > 1 {
            ev.push("variant:stacked_rows".into());
            ev.push(format!("row:{}/{}", k + 1, f.rows.len()));
        }
        if n_leaders > 1 {
            ev.push("variant:multiple_leaders".into());
        }
        let tips: Vec<(P, P)> = own
            .iter()
            .map(|l| {
                (
                    l.tip,
                    head_axis(match &bl.parts[l.head].geom {
                        Geom::Head(h) => h,
                        _ => unreachable!("head"),
                    })
                    .1,
                )
            })
            .chain(extra.iter().map(|(o, l)| {
                (
                    l.tip,
                    head_axis(match &o.parts[l.head].geom {
                        Geom::Head(h) => h,
                        _ => unreachable!("head"),
                    })
                    .1,
                )
            }))
            .collect();
        let mut feature: Option<P> = None;
        for (tip, dir) in &tips {
            if let Some((d, origin)) = along_dimension_line(cx, *tip, *dir, f.text) {
                ev.push(format!("leader_along_dimension_line:{d}"));
                feature = feature.or(Some(origin));
            }
        }
        let e = b.expr(
            row.kind,
            &label,
            "card:gdt_frame_symbol/line_grid_font_glyph",
            "candidate",
            vec![],
            ev,
            vec!["Toleranced feature requires the leader target".into()],
        );
        member(
            b,
            e,
            bl,
            row.glyph,
            "symbol_glyph",
            "text",
            vec![format!("font:{}", f.font)],
        );
        let rb = [
            row.cells[0][0],
            row.cells[0][1],
            row.cells.last().unwrap()[2],
            row.cells[0][3],
        ];
        for i in &f.cell_lines {
            let Geom::Line(a, z) = bl.parts[*i].geom else {
                continue;
            };
            // a border of this row, or a column line over its height (lines shared by stacked
            // rows belong to each)
            let mine = if dist(a, z) <= 1e-9 {
                inside(a, rb, jtol)
            } else if horizontal(a, z) {
                ((a[1] - rb[1]).abs() <= jtol || (a[1] - rb[3]).abs() <= jtol)
                    && a[0].max(z[0]).min(rb[2]) - a[0].min(z[0]).max(rb[0]) > jtol
            } else {
                a[1].max(z[1]).min(rb[3]) - a[1].min(z[1]).max(rb[1]) > jtol
                    && row
                        .cells
                        .iter()
                        .any(|c| (a[0] - c[0]).abs() <= jtol || (a[0] - c[2]).abs() <= jtol)
            };
            if mine {
                member(
                    b,
                    e,
                    bl,
                    *i,
                    "frame_cell",
                    part_type(&bl.parts[*i], "frame_cell", Some(false)),
                    vec![],
                );
            }
        }
        for (n, ids) in row.fields.iter().enumerate() {
            for i in ids {
                member(
                    b,
                    e,
                    bl,
                    *i,
                    if n == 0 {
                        "tolerance_value"
                    } else {
                        "datum_reference"
                    },
                    "text",
                    vec![],
                );
            }
        }
        for l in &own {
            for i in &l.lines {
                member(
                    b,
                    e,
                    bl,
                    *i,
                    "leader_line",
                    part_type(&bl.parts[*i], "leader_line", Some(false)),
                    vec!["leader_from_frame_border".into()],
                );
            }
            member(b, e, bl, l.head, "arrowhead", "fill", vec![]);
        }
        for (o, l) in &extra {
            for i in &l.lines {
                member(
                    b,
                    e,
                    o,
                    *i,
                    "leader_line",
                    part_type(&o.parts[*i], "leader_line", Some(false)),
                    vec![format!("leader_from_frame_border:{}", bl.root)],
                );
            }
            member(b, e, o, l.head, "arrowhead", "fill", vec![]);
        }
        // placed with the feature it controls: the dimensioned feature when a leader runs along a
        // dimension line (GB/T 1182 8.2), else the point a leader ends on
        if first.is_none() {
            first = feature.or(tips.first().map(|t| t.0)).map(|p| (e, p));
        } else if let Some(p) = feature.or(tips.first().map(|t| t.0)) {
            extra_anchors.push((e, p));
        }
    }
    Ok(first.map(|f| {
        let mut v = vec![f];
        v.extend(extra_anchors);
        v
    }))
}

/// Datum symbols drawn as one block are placed with the feature their connector is aligned with:
/// a connector in line with a dimension line names the dimensioned feature (GB/T 1182 8.2,
/// GB/T 17851), so the symbol belongs to that dimension's view.
fn datum_anchors(cx: &Ctx, b: &mut Builder, anchors: &mut BTreeMap<usize, P>) {
    for i in 0..b.exprs.len() {
        let e = &b.exprs[i];
        if e.kind != "datum_symbol" || anchors.contains_key(&i) {
            continue;
        }
        let roots: BTreeSet<&str> = b
            .members
            .iter()
            .filter(|m| m.expression_id == e.id && m.native_member_id.is_none())
            .map(|m| m.source_handle.as_str())
            .collect();
        let [root] = roots.into_iter().collect::<Vec<_>>()[..] else {
            continue;
        };
        let Some(bl) = cx.blocks.roots.get(root) else {
            continue;
        };
        let [(_, c, r)] = bl.circles()[..] else {
            continue;
        };
        // the connector leaves the circle; its far end is the terminal
        let connector = bl.lines().into_iter().find_map(|(_, p, q)| {
            if (dist(p, c) - r).abs() < 0.1 * r && dist(q, c) > dist(p, c) {
                Some((p, q))
            } else if (dist(q, c) - r).abs() < 0.1 * r && dist(p, c) > dist(q, c) {
                Some((q, p))
            } else {
                None
            }
        });
        let Some((near, far)) = connector else {
            continue;
        };
        if let Some((d, origin)) = along_dimension_line(cx, far, unit(sub(far, near)), r) {
            anchors.insert(i, origin);
            let ev = &mut b.exprs[i].evidence_ids;
            ev.push(format!("connector_along_dimension_line:{d}"));
            ev.push("card:datum_feature_symbol".into());
            ev.push("variant:aligned_with_dimension_line".into());
        }
    }
}

/// The native dimension whose dimension line a leader's head ends on, pointing along it (the
/// frame then concerns the feature of size, GB/T 1182 8.2), with the origin of its first
/// extension line (a point of the dimensioned feature).
fn along_dimension_line(cx: &Ctx, tip: P, dir: P, h: f64) -> Option<(String, P)> {
    let tol = card("gdt_frame_content")
        .and_then(|c| param(c, "dimension_alignment"))
        .ok()?
        * h;
    cx.rec
        .iter()
        .filter(|(_, r)| r.kind == "DIMENSION")
        .find_map(|(id, r)| {
            let dtype = int(r, 70) & 7;
            let u = match dtype {
                0 => {
                    let a = r.number(50).unwrap_or(0.).to_radians();
                    [a.cos(), a.sin()]
                }
                1 => unit(sub(r.xy(14).ok()?, r.xy(13).ok()?)),
                _ => return None,
            };
            let p10 = r.xy(10).ok()?;
            let origin = r.xy(13).ok()?;
            (cross(u, sub(tip, p10)).abs() <= tol && dot(u, dir).abs() > 0.98)
                .then(|| (id.clone(), origin))
        })
}

fn emit_detail_view(
    b: &mut Builder,
    bl: &Block,
    ci: usize,
    ind: &Indicator,
    k: f64,
    hits: usize,
) -> Result<()> {
    let c = card("detail_view")?;
    let classes = geometry_classes(bl, c)?;
    let mut ev = tagged("detail_view", "block_view", &bl.root);
    ev.push(format!(
        "enlargement_measured:{}",
        fmt_num((k * 1000.).round() / 1000.)
    ));
    ev.push(format!("indicator:{}", ind.label));
    ev.push(format!("copy_lines_on_source:{hits}"));
    let rule = "card:detail_view/block_view";
    let mut by_kind: BTreeMap<&str, Vec<Class>> = BTreeMap::new();
    for c in classes {
        by_kind.entry(c.1).or_default().push(c);
    }
    for (kind, parts) in by_kind {
        let e = b.expr(
            kind,
            &format!("局部放大图 {} · {}", ind.label, kind_cn(kind)),
            rule,
            "candidate",
            vec![],
            ev.clone(),
            vec!["Part feature identity of each piece requires projection evidence".into()],
        );
        for (i, _, role, thick) in parts {
            let t = if kind == "visible_contour" {
                thick.or(Some(true))
            } else {
                thick
            };
            member(b, e, bl, i, role, part_type(&bl.parts[i], role, t), vec![]);
        }
    }
    let e = b.expr(
        "view_callout",
        &format!("{} 局部放大图边界", ind.label),
        rule,
        "candidate",
        vec!["detail_boundary".into()],
        ev,
        vec![],
    );
    member(
        b,
        e,
        bl,
        ci,
        "detail_circle",
        part_type(&bl.parts[ci], "detail_circle", Some(false)),
        vec![],
    );
    Ok(())
}

/// A library part drawn as one block (structural lines only, no text), identified by the item
/// number whose dot lies on it and the parts-list row of that number with a standard code.
fn standard_part(
    cx: &Ctx,
    b: &mut Builder,
    bl: &Block,
    items: &[(usize, String, P)],
    lists: &[(usize, Vec<Row>)],
) -> Result<()> {
    let c = card("standard_part")?;
    let codes = values(c, "standard_code");
    let margin = param(c, "dot_margin")?;
    if !bl.texts().is_empty()
        || bl.parts.iter().any(|p| {
            !matches!(
                p.geom,
                Geom::Line(..) | Geom::Arc(..) | Geom::Circle(..) | Geom::Path(..)
            )
        })
    {
        return Ok(());
    }
    let Some(bb) = bl.bbox(|_| true) else {
        return Ok(());
    };
    let size = (bb[2] - bb[0]).max(bb[3] - bb[1]);
    let Some((ie, n, _)) = items
        .iter()
        .find(|(_, _, at)| inside(*at, bb, margin * size))
    else {
        return Ok(());
    };
    let Some((le, row)) = lists.iter().find_map(|(e, rows)| {
        rows.iter()
            .find(|r| r.item.as_deref() == Some(n.as_str()))
            .map(|r| (*e, r))
    }) else {
        return Ok(());
    };
    let code = row
        .cells
        .iter()
        .map(|c| c.1.as_str())
        .find(|t| codes.iter().any(|k| compact(t).starts_with(&compact(k))));
    let Some(code) = code else { return Ok(()) };
    let name = row_name(row);
    let thick_ratio = param(card("view_callout")?, "thick_ratio_min")?;
    let classes = bl.weight_classes(thick_ratio);
    let mut ev = tagged("standard_part", "library_block", &bl.root);
    ev.push(format!("item_number:{n}"));
    ev.push(format!("standard_code:{code}"));
    let e = b.expr(
        "standard_part",
        &format!("{n} {name} {code}"),
        "card:standard_part/library_block",
        "candidate",
        vec![],
        ev,
        vec![
            "Drawn in simplified form from a library; its features are those of the standard"
                .into(),
        ],
    );
    for (i, p) in bl.parts.iter().enumerate() {
        let thick =
            classes.map(|(lo, hi)| p.weight > 0 && (p.weight - hi).abs() < (p.weight - lo).abs());
        let role = match p.pattern {
            Pattern::DashDot => "center_line",
            Pattern::Dashed => "hidden_segment",
            _ if thick == Some(false) => "native_member",
            _ => "contour_segment",
        };
        member(b, e, bl, i, role, part_type(p, role, thick), vec![]);
    }
    b.links.push((e, "named_by_item", *ie));
    b.links.push((e, "row_of", le));
    let _ = cx;
    Ok(())
}

// ---------------------------------------------------------------- masks and references

/// Every WIPEOUT whose polygon follows the outline of a block another expression owns becomes a
/// member of that expression (card annotation_mask): CAD writes the mask just before its object.
pub(super) fn masks(cx: &Ctx, b: &mut Builder) -> Result<()> {
    let c = card("annotation_mask")?;
    let tol_ratio = param(c, "outline_tolerance")?;
    let fraction = param(c, "vertex_fraction_min")?;
    let mut roots: Vec<(&String, &&Record)> = cx.rec.iter().collect();
    roots.sort_by_key(|(_, r)| r.index);
    for (k, (h, r)) in roots.iter().enumerate() {
        if r.kind != "WIPEOUT" || b.used(h) {
            continue;
        }
        let Some(poly) = wipeout_polygon(r) else {
            continue;
        };
        let Some(pb) = bbox_of(&poly) else { continue };
        let tol = tol_ratio * dist([pb[0], pb[1]], [pb[2], pb[3]]) + 1e-6;
        let follows = |bl: &Block| {
            let geo: Vec<&Geom> = bl.parts.iter().map(|p| &p.geom).collect();
            let near = |q: P| {
                geo.iter().any(|g| match g {
                    Geom::Line(a, c) => on_seg(q, *a, *c, tol).is_some(),
                    Geom::Path(v, closed) => {
                        v.windows(2).any(|w| on_seg(q, w[0], w[1], tol).is_some())
                            || (*closed
                                && v.len() > 2
                                && on_seg(q, v[v.len() - 1], v[0], tol).is_some())
                    }
                    Geom::Circle(c, rad) => (dist(q, *c) - rad).abs() <= tol,
                    Geom::Arc(c, rad, s0, sw) => {
                        (dist(q, *c) - rad).abs() <= tol
                            && ((q[1] - c[1]).atan2(q[0] - c[0]) - s0).rem_euclid(TAU)
                                <= sw + tol / rad.max(1e-9)
                    }
                    _ => false,
                })
            };
            poly.iter().filter(|q| near(**q)).count() as f64 >= fraction * poly.len() as f64
        };
        // the object written right after the mask, else any block whose outline it follows
        let next = roots.get(k + 1).map(|x| x.0.as_str());
        let owner = next
            .and_then(|n| cx.blocks.roots.get(n))
            .filter(|bl| b.used(&bl.root) && follows(bl))
            .or_else(|| {
                cx.blocks
                    .roots
                    .values()
                    .filter(|bl| b.used(&bl.root) && follows(bl))
                    .min_by(|x, y| x.index.abs_diff(r.index).cmp(&y.index.abs_diff(r.index)))
            });
        let Some(owner) = owner else { continue };
        let exprs: BTreeSet<String> = b
            .by_source
            .get(&owner.root)
            .map(|ms| {
                ms.iter()
                    .map(|m| b.members[*m].expression_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let idx: Vec<usize> = b
            .exprs
            .iter()
            .enumerate()
            .filter(|(_, e)| exprs.contains(&e.id) && e.kind != "unresolved")
            .map(|(i, _)| i)
            .collect();
        for i in idx {
            b.member(
                i,
                h,
                None,
                [0., 1.],
                "native_member",
                "fill",
                "candidate",
                vec![],
                vec![
                    "card:annotation_mask".into(),
                    "variant:outline_mask".into(),
                    format!("mask_of:{}", owner.root),
                ],
            );
            b.exprs[i].evidence_ids.push(format!("mask:{h}"));
        }
    }
    Ok(())
}

/// A cutting-plane symbol refers to the section view named by its letter (`X—X`); a detail
/// indicator refers to the detail view titled with its label.
pub(super) fn references(
    b: &Builder,
    regions: &[Region],
    refs: &mut BTreeMap<usize, String>,
) -> Result<()> {
    let c = card("view_callout")?;
    let dashes: Vec<&str> = c
        .lexicon
        .iter()
        .flatten()
        .filter(|l| l.entry == "separator")
        .flat_map(|l| l.values.iter().map(String::as_str))
        .collect();
    let section = |label: &str| -> Option<String> {
        let name = label.split_whitespace().next()?;
        let sep = dashes.iter().find(|d| name.contains(**d))?;
        let (l, r) = name.split_once(sep)?;
        (l == r).then(|| l.to_string())
    };
    for (i, e) in b.exprs.iter().enumerate() {
        let key = e.label.split_whitespace().next().unwrap_or("");
        let target = match e.rule.as_str() {
            "card:view_callout/cutting_plane_group" => regions
                .iter()
                .find(|r| r.kind == "section_view" && section(&r.label).as_deref() == Some(key)),
            "card:view_callout/detail_indicator" => regions.iter().find(|r| {
                r.kind == "detail_view" && r.label.split_whitespace().next() == Some(key)
            }),
            _ => continue,
        };
        match target {
            Some(r) => {
                refs.insert(i, r.id.clone());
            }
            None => {
                refs.remove(&i);
            }
        }
    }
    Ok(())
}
