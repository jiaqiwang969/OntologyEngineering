//! Source-only shape, text and symbol helpers for expression discovery.
use crate::native_readback::{Drawing, Record};
use anyhow::{Result, ensure};
type P = [f64; 2];
fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn len(a: P) -> f64 {
    a[0].hypot(a[1])
}
fn dist(a: P, b: P) -> f64 {
    len(sub(a, b))
}
fn mid(a: P, b: P) -> P {
    [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.]
}
fn near(a: P, b: P) -> bool {
    dist(a, b) < 1e-5
}
fn unit(a: P) -> P {
    let n = len(a);
    [a[0] / n, a[1] / n]
}
#[derive(Clone, Copy)]
struct Transform([f64; 6]);
impl Transform {
    fn identity() -> Self {
        Self([1., 0., 0., 1., 0., 0.])
    }
    fn p(self, p: P) -> P {
        let m = self.0;
        [
            m[0] * p[0] + m[2] * p[1] + m[4],
            m[1] * p[0] + m[3] * p[1] + m[5],
        ]
    }
    fn then(self, n: Self) -> Self {
        let a = self.0;
        let b = n.0;
        Self([
            a[0] * b[0] + a[2] * b[1],
            a[1] * b[0] + a[3] * b[1],
            a[0] * b[2] + a[2] * b[3],
            a[1] * b[2] + a[3] * b[3],
            a[0] * b[4] + a[2] * b[5] + a[4],
            a[1] * b[4] + a[3] * b[5] + a[5],
        ])
    }
}
#[derive(Clone, Default)]
pub(crate) struct Shape {
    pub(crate) lines: Vec<(P, P)>,
    pub(crate) circles: Vec<(P, f64)>,
    pub(crate) texts: Vec<(String, P, f64)>,
    pub(crate) paths: Vec<Vec<P>>,
    pub(crate) curved: bool,
    /// Sampled points of each ARC (used for symbol signatures; not part of the line geometry).
    pub(crate) arcs: Vec<Vec<P>>,
}
pub(crate) fn raw_text(r: &Record) -> String {
    r.tags
        .iter()
        .filter(|(c, _)| *c == 1 || (*c == 3 && r.kind == "MTEXT"))
        .map(|(_, s)| s.as_str())
        .collect()
}
pub(crate) fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(k) = chars.next() {
                if k == 'P' {
                    out.push(' ')
                } else if matches!(k, 'H' | 'W' | 'A' | 'C' | 'F' | 'f' | 'T' | 'Q') {
                    for x in chars.by_ref() {
                        if x == ';' {
                            break;
                        }
                    }
                } else {
                    out.push(k)
                }
            }
        } else if !matches!(c, '{' | '}') {
            out.push(c)
        }
    }
    out.replace("%%c", "Ø")
        .replace("%%C", "Ø")
        .replace("%%d", "°")
        .replace("%%p", "±")
        .trim()
        .into()
}
fn number(s: &str) -> bool {
    let s = plain(s);
    !s.is_empty()
        && s.chars().any(|c| c.is_ascii_digit())
        && s.chars()
            .all(|c| c.is_ascii_digit() || " .+-±ØφRr°×xX/*^".contains(c))
}
fn letter(s: &str) -> bool {
    let s = plain(s);
    !s.is_empty() && s.len() <= 5 && s.chars().all(|c| c.is_ascii_uppercase() || c == '-')
}
fn vertices(r: &Record) -> Vec<P> {
    let mut points = vec![];
    let mut x = None;
    for (c, v) in &r.tags {
        if *c == 10 {
            x = v.parse::<f64>().ok()
        }
        if *c == 20
            && let (Some(a), Ok(b)) = (x.take(), v.parse::<f64>())
        {
            points.push([a, b])
        }
    }
    points
}
fn shape(d: &Drawing, r: &Record, tr: Transform, depth: usize, out: &mut Shape) -> Result<()> {
    ensure!(
        depth < 12 && out.lines.len() < 2000,
        "discipline_shape_budget"
    );
    if r.get(210)
        .is_some_and(|_| r.number(210).unwrap_or(1.) != 0.)
        || r.get(220)
            .is_some_and(|_| r.number(220).unwrap_or(1.) != 0.)
        || r.get(230)
            .is_some_and(|_| r.number(230).unwrap_or(0.) != 1.)
    {
        return Ok(());
    }
    match r.kind.as_str() {
        "LINE" => {
            if let (Ok(a), Ok(b)) = (r.xy(10), r.xy(11)) {
                out.lines.push((tr.p(a), tr.p(b)))
            }
        }
        "CIRCLE" => {
            if let (Ok(c), Ok(rad)) = (r.xy(10), r.number(40)) {
                let sx = len([tr.0[0], tr.0[1]]);
                let sy = len([tr.0[2], tr.0[3]]);
                if (sx - sy).abs() < 1e-7 {
                    out.circles.push((tr.p(c), rad * sx))
                }
            }
        }
        "ARC" => {
            out.curved = true;
            if let (Ok(c), Ok(rad)) = (r.xy(10), r.number(40)) {
                let a0 = r.number(50).unwrap_or(0.);
                let mut a1 = r.number(51).unwrap_or(360.);
                if a1 <= a0 {
                    a1 += 360.;
                }
                out.arcs.push(
                    (0..=8)
                        .map(|i| {
                            let a = (a0 + (a1 - a0) * f64::from(i) / 8.).to_radians();
                            tr.p([c[0] + rad * a.cos(), c[1] + rad * a.sin()])
                        })
                        .collect(),
                );
            }
        }
        "ELLIPSE" | "SPLINE" => out.curved = true,
        // A filled triangle (arrowhead) is kept as a closed three-vertex path; it carries no
        // line geometry of its own.
        "SOLID" | "TRACE" => {
            let mut p: Vec<P> = Vec::new();
            for c in 10..=13 {
                if let Ok(v) = r.xy(c) {
                    let v = tr.p(v);
                    if !p.iter().any(|q| near(*q, v)) {
                        p.push(v)
                    }
                }
            }
            if p.len() == 3 {
                p.push(p[0]);
                out.paths.push(p)
            }
        }
        "TEXT" | "MTEXT" | "ATTRIB" => {
            let p = if r.get(72).is_some_and(|v| v != "0") {
                r.xy(11).or_else(|_| r.xy(10))
            } else {
                r.xy(10)
            };
            if let Ok(p) = p {
                out.texts.push((
                    plain(&raw_text(r)),
                    tr.p(p),
                    r.number(40).unwrap_or(1.) * len([tr.0[0], tr.0[1]]),
                ))
            }
        }
        "LWPOLYLINE" => {
            let mut p = vertices(r).into_iter().map(|p| tr.p(p)).collect::<Vec<_>>();
            if r.get(70).and_then(|x| x.parse::<i32>().ok()).unwrap_or(0) & 1 != 0 && !p.is_empty()
            {
                p.push(p[0])
            }
            if r.tags
                .iter()
                .any(|(c, v)| *c == 42 && v.parse::<f64>().is_ok_and(|v| v != 0.))
            {
                out.curved = true
            } else {
                for w in p.windows(2) {
                    out.lines.push((w[0], w[1]))
                }
            }
            out.paths.push(p)
        }
        "POLYLINE" => {
            let mut p = vec![];
            for child in d.records.iter().skip(r.index + 1) {
                if child.kind == "SEQEND" {
                    break;
                }
                if child.kind != "VERTEX" {
                    break;
                }
                if let Ok(v) = child.xy(10) {
                    p.push(tr.p(v))
                }
                if child.get(42).is_some_and(|v| v != "0.0" && v != "0") {
                    out.curved = true
                }
            }
            if r.get(70).and_then(|x| x.parse::<i32>().ok()).unwrap_or(0) & 1 != 0 && !p.is_empty()
            {
                p.push(p[0])
            }
            if !out.curved {
                for w in p.windows(2) {
                    out.lines.push((w[0], w[1]))
                }
            }
            out.paths.push(p)
        }
        "INSERT" => {
            if let Some(name) = r.get(2) {
                let base = d
                    .records
                    .iter()
                    .find(|x| x.kind == "BLOCK" && x.get(2) == Some(name))
                    .and_then(|x| x.xy(10).ok())
                    .unwrap_or([0., 0.]);
                let p = r.xy(10).unwrap_or([0., 0.]);
                let sx = r.number(41).unwrap_or(1.);
                let sy = r.number(42).unwrap_or(1.);
                let a = r.number(50).unwrap_or(0.).to_radians();
                let (c, s) = (a.cos(), a.sin());
                let n = Transform([
                    c * sx,
                    s * sx,
                    -s * sy,
                    c * sy,
                    p[0] - c * sx * base[0] + s * sy * base[1],
                    p[1] - s * sx * base[0] - c * sy * base[1],
                ]);
                for ch in d.records.iter().filter(|x| {
                    x.section == "BLOCKS"
                        && x.block == name
                        && !matches!(x.kind.as_str(), "BLOCK" | "ENDBLK" | "VERTEX" | "SEQEND")
                }) {
                    shape(d, ch, tr.then(n), depth + 1, out)?
                }
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn getshape(d: &Drawing, r: &Record) -> Result<Shape> {
    let mut s = Shape::default();
    shape(d, r, Transform::identity(), 0, &mut s)?;
    Ok(s)
}

pub(crate) fn rough(s: &Shape) -> bool {
    if s.lines.len() < 2
        || s.lines.len() > 8
        || s.texts.is_empty()
        || !s.texts.iter().any(|(t, _, _)| number(t))
    {
        return false;
    }
    for (i, (a, b)) in s.lines.iter().enumerate() {
        for (c, d) in s.lines.iter().skip(i + 1) {
            for (p, x, q, y) in [
                (*a, *b, *c, *d),
                (*a, *b, *d, *c),
                (*b, *a, *c, *d),
                (*b, *a, *d, *c),
            ] {
                if !near(p, q) {
                    continue;
                }
                let u = sub(x, p);
                let v = sub(y, q);
                let (l, m) = (len(u), len(v));
                if l < 1e-6 || m < 1e-6 {
                    continue;
                }
                let ratio = l.max(m) / l.min(m);
                let cosine = dot(u, v) / (l * m);
                if (1.5..3.).contains(&ratio) && (cosine - 0.5).abs() < 0.025 {
                    return true;
                }
            }
        }
    }
    false
}
pub(crate) fn datum(s: &Shape) -> bool {
    s.circles.len() == 1 && s.texts.len() == 1 && letter(&s.texts[0].0) && !s.lines.is_empty() && {
        let (c, r) = s.circles[0];
        dist(s.texts[0].1, c) < r * 1.2
            && s.lines
                .iter()
                .any(|(a, b)| ((dist(*a, c) - r).abs().min((dist(*b, c) - r).abs())) < r * 0.05)
    }
}

pub(crate) fn arrow(r: &Record) -> Option<(P, P, f64)> {
    if !matches!(r.kind.as_str(), "SOLID" | "TRACE") {
        return None;
    }
    let mut pts = vec![];
    for c in 10..=13 {
        if let Ok(p) = r.xy(c)
            && !pts.iter().any(|q| near(p, *q))
        {
            pts.push(p)
        }
    }
    if pts.len() != 3 {
        return None;
    }
    for i in 0..3 {
        let tip = pts[i];
        let (a, b) = (pts[(i + 1) % 3], pts[(i + 2) % 3]);
        let base = dist(a, b);
        let height = dist(tip, mid(a, b));
        if base > 1e-6
            && (2.0..12.).contains(&(height / base))
            && (dist(tip, a) - dist(tip, b)).abs() < base * 0.05
        {
            return Some((tip, unit(sub(tip, mid(a, b))), height));
        }
    }
    None
}
