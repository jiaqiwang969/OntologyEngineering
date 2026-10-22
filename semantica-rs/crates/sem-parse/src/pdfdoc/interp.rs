//! Content-stream interpretation as pdfminer's `PDFPageInterpreter` + `PDFLayoutAnalyzer`
//! (with `laparams=None`, as pdfplumber runs it): characters (`LTChar`), straight lines,
//! rectangles and curves (`paint_path`) and images (`LTImage` inside `LTFigure`), in content
//! order, with pdfplumber's `top`/`bottom`/`doctop` attributes added.

use std::collections::HashMap;
use std::rc::Rc;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde_json::Value;

use super::font::{Font, as_dict, deref, dget, num_f64, stream_data};
use super::lexer::{ContentParser, Obj};

/// A float transformation matrix `(a, b, c, d, e, f)`.
pub type M = [f64; 6];

const IDENTITY: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `mult_matrix(m1, m0)`.
pub fn mult(m1: &M, m0: &M) -> M {
    let [a1, b1, c1, d1, e1, f1] = *m1;
    let [a0, b0, c0, d0, e0, f0] = *m0;
    [
        a0 * a1 + c0 * b1,
        b0 * a1 + d0 * b1,
        a0 * c1 + c0 * d1,
        b0 * c1 + d0 * d1,
        a0 * e1 + c0 * f1 + e0,
        b0 * e1 + d0 * f1 + f0,
    ]
}

/// `translate_matrix(m, (x, y))`.
fn translate(m: &M, x: f64, y: f64) -> M {
    let [a, b, c, d, e, f] = *m;
    [a, b, c, d, x * a + y * c + e, x * b + y * d + f]
}

/// `apply_matrix_pt(m, (x, y))`.
pub fn apply(m: &M, x: f64, y: f64) -> (f64, f64) {
    let [a, b, c, d, e, f] = *m;
    (a * x + c * y + e, b * x + d * y + f)
}

/// A character (`LTChar`) with pdfplumber's attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct Char {
    pub text: String,
    pub fontname: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub top: f64,
    pub bottom: f64,
    pub doctop: f64,
    pub upright: bool,
    pub size: f64,
    pub adv: f64,
}

/// The kind of a painted path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    Line,
    Rect,
    Curve,
}

/// A painted path (`LTLine`/`LTRect`/`LTCurve`) with pdfplumber's attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct PathObj {
    pub kind: PathKind,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub top: f64,
    pub bottom: f64,
    pub doctop: f64,
    pub width: f64,
    pub height: f64,
    /// Points in pdfplumber's coordinates `(x, top)`.
    pub pts: Vec<(f64, f64)>,
    pub stroke: bool,
    pub fill: bool,
}

/// An image (`LTImage`) with pdfplumber's attributes.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageObj {
    pub name: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub top: f64,
    pub bottom: f64,
    pub doctop: f64,
    pub srcsize: (Value, Value),
    pub imagemask: Value,
    pub bits: Value,
    pub colorspace: Value,
}

/// Everything a page's content produces, in content order per kind.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub chars: Vec<Char>,
    pub paths: Vec<PathObj>,
    pub images: Vec<ImageObj>,
}

#[derive(Clone)]
struct TextState {
    font: Option<Rc<Font>>,
    fontsize: f64,
    charspace: f64,
    wordspace: f64,
    scaling: f64,
    leading: f64,
    rise: f64,
    matrix: M,
    linematrix: (f64, f64),
}

impl Default for TextState {
    fn default() -> Self {
        TextState {
            font: None,
            fontsize: 0.0,
            charspace: 0.0,
            wordspace: 0.0,
            scaling: 100.0,
            leading: 0.0,
            rise: 0.0,
            matrix: IDENTITY,
            linematrix: (0.0, 0.0),
        }
    }
}

#[derive(Clone, Default)]
struct GraphicState {
    linewidth: f64,
}

/// One path segment: operator and operands.
#[derive(Clone, Debug)]
struct Seg {
    op: u8,
    args: Vec<f64>,
}

/// Interpreter shared state across nested forms.
pub struct Interpreter<'a> {
    doc: &'a Document,
    fonts: &'a mut HashMap<ObjectId, Rc<Font>>,
    pub out: Layout,
    pheight: f64,
    initial_doctop: f64,
    depth: usize,
    inline_count: usize,
}

/// Resources of one content stream scope.
struct Scope {
    fonts: HashMap<Vec<u8>, Rc<Font>>,
    xobjects: HashMap<Vec<u8>, Object>,
    colorspaces: HashMap<Vec<u8>, usize>,
    resources: Dictionary,
}

fn predefined_colorspaces() -> HashMap<Vec<u8>, usize> {
    [
        ("DeviceGray", 1),
        ("CalRGB", 3),
        ("CalGray", 1),
        ("Lab", 3),
        ("DeviceRGB", 3),
        ("DeviceCMYK", 4),
        ("Separation", 1),
        ("Indexed", 1),
        ("Pattern", 1),
    ]
    .into_iter()
    .map(|(k, v)| (k.as_bytes().to_vec(), v))
    .collect()
}

fn obj_num(o: &Obj) -> Option<f64> {
    o.num().map(|n| n.f())
}

impl<'a> Interpreter<'a> {
    pub fn new(
        doc: &'a Document,
        fonts: &'a mut HashMap<ObjectId, Rc<Font>>,
        pheight: f64,
        initial_doctop: f64,
    ) -> Self {
        Interpreter {
            doc,
            fonts,
            out: Layout::default(),
            pheight,
            initial_doctop,
            depth: 0,
            inline_count: 0,
        }
    }

    fn init_resources(&mut self, resources: Option<&Dictionary>) -> Scope {
        let mut scope = Scope {
            fonts: HashMap::new(),
            xobjects: HashMap::new(),
            colorspaces: predefined_colorspaces(),
            resources: resources.cloned().unwrap_or_default(),
        };
        let Some(resources) = resources else {
            return scope;
        };
        let doc = self.doc;
        if let Some(fonts) = dget(doc, resources, b"Font").and_then(as_dict) {
            for (fontid, spec) in fonts.iter() {
                let objid = match spec {
                    Object::Reference(id) => Some(*id),
                    _ => None,
                };
                let font = match objid.and_then(|id| self.fonts.get(&id).cloned()) {
                    Some(f) => f,
                    None => {
                        let f = match as_dict(deref(doc, spec)) {
                            Some(d) => Rc::new(Font::from_spec(doc, d)),
                            None => Rc::new(Font::unknown()),
                        };
                        if let Some(id) = objid {
                            self.fonts.insert(id, f.clone());
                        }
                        f
                    }
                };
                scope.fonts.insert(fontid.clone(), font);
            }
        }
        if let Some(spaces) = dget(doc, resources, b"ColorSpace").and_then(as_dict) {
            for (csid, spec) in spaces.iter() {
                let spec = deref(doc, spec);
                let (name, arr) = match spec {
                    Object::Array(a) => (a.first().map(|o| deref(doc, o)), Some(a)),
                    other => (Some(other), None),
                };
                let name = name.and_then(|n| n.as_name().ok()).unwrap_or(b"");
                let n = match (name, arr) {
                    (b"ICCBased", Some(a)) if a.len() >= 2 => as_dict(deref(doc, &a[1]))
                        .and_then(|d| dget(doc, d, b"N"))
                        .and_then(num_f64)
                        .map(|n| n as usize),
                    (b"DeviceN", Some(a)) if a.len() >= 2 => {
                        deref(doc, &a[1]).as_array().ok().map(|x| x.len())
                    }
                    _ => predefined_colorspaces().get(name).copied(),
                };
                if let Some(n) = n {
                    scope.colorspaces.insert(csid.clone(), n);
                }
            }
        }
        if let Some(xobjs) = dget(doc, resources, b"XObject").and_then(as_dict) {
            for (id, obj) in xobjs.iter() {
                scope.xobjects.insert(id.clone(), obj.clone());
            }
        }
        scope
    }

    /// `render_contents(resources, streams, ctm)`.
    pub fn render_contents(&mut self, resources: Option<&Dictionary>, data: &[u8], ctm: M) {
        if self.depth > 32 {
            return;
        }
        self.depth += 1;
        let scope = self.init_resources(resources);
        let mut ctm = ctm;
        let mut ts = TextState::default();
        let mut gs = GraphicState::default();
        let mut gstack: Vec<(M, TextState, GraphicState)> = Vec::new();
        let mut curpath: Vec<Seg> = Vec::new();
        let mut args: Vec<Obj> = Vec::new();
        let mut ncs = 1usize;
        let mut scs = 1usize;
        let mut parser = ContentParser::new(data);
        while let Some(obj) = parser.next_object() {
            let Obj::Kw(kw) = obj else {
                args.push(obj);
                continue;
            };
            // pdfminer's `pop(n)`: the last n operands (fewer when the stack is short, in
            // which case the operator is skipped).
            macro_rules! pop {
                ($n:expr) => {{
                    let n: usize = $n;
                    let start = args.len().saturating_sub(n);
                    let taken: Vec<Obj> = args.split_off(start);
                    if taken.len() == n { Some(taken) } else { None }
                }};
            }
            let nums = |v: &[Obj]| -> Option<Vec<f64>> { v.iter().map(obj_num).collect() };
            match kw.as_slice() {
                b"q" => gstack.push((ctm, ts.clone(), gs.clone())),
                b"Q" => {
                    if let Some((c, t, g)) = gstack.pop() {
                        ctm = c;
                        ts = t;
                        gs = g;
                    }
                }
                b"cm" => {
                    if let Some(v) = pop!(6)
                        && let Some(m) = nums(&v)
                    {
                        ctm = mult(&[m[0], m[1], m[2], m[3], m[4], m[5]], &ctm);
                    }
                }
                b"w" => {
                    if let Some(v) = pop!(1)
                        && let Some(w) = obj_num(&v[0])
                    {
                        gs.linewidth = w;
                    }
                }
                b"J" | b"j" | b"M" | b"ri" | b"i" | b"gs" | b"sh" | b"BMC" | b"MP" | b"CS" | b"cs"
                | b"G" | b"g" => {
                    let v = pop!(1);
                    match kw.as_slice() {
                        b"CS" => {
                            if let Some(n) = v.as_ref().and_then(|v| v[0].name()).and_then(|n| scope.colorspaces.get(n)) {
                                scs = *n;
                            }
                        }
                        b"cs" => {
                            if let Some(n) = v.as_ref().and_then(|v| v[0].name()).and_then(|n| scope.colorspaces.get(n)) {
                                ncs = *n;
                            }
                        }
                        b"G" => scs = 1,
                        b"g" => ncs = 1,
                        _ => {}
                    }
                }
                b"d" | b"BDC" | b"DP" | b"Tf" | b"Td" | b"TD" | b"d0" => {
                    let v = pop!(2);
                    if let Some(v) = v {
                        match kw.as_slice() {
                            b"Tf" => {
                                ts.font = Some(match v[0].name().and_then(|n| scope.fonts.get(n)) {
                                    Some(f) => f.clone(),
                                    None => Rc::new(Font::unknown()),
                                });
                                if let Some(size) = obj_num(&v[1]) {
                                    ts.fontsize = size;
                                }
                            }
                            b"Td" | b"TD" => {
                                if let (Some(tx), Some(ty)) = (obj_num(&v[0]), obj_num(&v[1])) {
                                    let [a, b, c, d, e, f] = ts.matrix;
                                    let e_new = tx * a + ty * c + e;
                                    let f_new = tx * b + ty * d + f;
                                    ts.matrix = [a, b, c, d, e_new, f_new];
                                    if kw.as_slice() == b"TD" {
                                        ts.leading = ty;
                                    }
                                }
                                ts.linematrix = (0.0, 0.0);
                            }
                            _ => {}
                        }
                    }
                }
                b"RG" | b"rg" => {
                    let _ = pop!(3);
                    if kw.as_slice() == b"RG" {
                        scs = 3;
                    } else {
                        ncs = 3;
                    }
                }
                b"K" | b"k" => {
                    let _ = pop!(4);
                    if kw.as_slice() == b"K" {
                        scs = 4;
                    } else {
                        ncs = 4;
                    }
                }
                b"SC" | b"SCN" => {
                    let _ = pop!(scs);
                }
                b"sc" | b"scn" => {
                    let _ = pop!(ncs);
                }
                b"m" | b"l" => {
                    if let Some(v) = pop!(2)
                        && let Some(p) = nums(&v)
                    {
                        curpath.push(Seg { op: kw[0], args: p });
                    }
                }
                b"c" => {
                    if let Some(v) = pop!(6)
                        && let Some(p) = nums(&v)
                    {
                        curpath.push(Seg { op: b'c', args: p });
                    }
                }
                b"v" | b"y" => {
                    if let Some(v) = pop!(4)
                        && let Some(p) = nums(&v)
                    {
                        curpath.push(Seg { op: kw[0], args: p });
                    }
                }
                b"h" => curpath.push(Seg { op: b'h', args: Vec::new() }),
                b"re" => {
                    if let Some(v) = pop!(4)
                        && let Some(p) = nums(&v)
                    {
                        let (x, y, w, h) = (p[0], p[1], p[2], p[3]);
                        curpath.push(Seg { op: b'm', args: vec![x, y] });
                        curpath.push(Seg { op: b'l', args: vec![x + w, y] });
                        curpath.push(Seg { op: b'l', args: vec![x + w, y + h] });
                        curpath.push(Seg { op: b'l', args: vec![x, y + h] });
                        curpath.push(Seg { op: b'h', args: Vec::new() });
                    }
                }
                b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" => {
                    if matches!(kw.as_slice(), b"s" | b"b" | b"b*") {
                        curpath.push(Seg { op: b'h', args: Vec::new() });
                    }
                    let (stroke, fill) = match kw.as_slice() {
                        b"S" | b"s" => (true, false),
                        b"f" | b"F" | b"f*" => (false, true),
                        _ => (true, true),
                    };
                    let path = std::mem::take(&mut curpath);
                    self.paint_path(&ctm, &gs, stroke, fill, &path);
                }
                b"n" => curpath.clear(),
                b"BT" => {
                    ts.matrix = IDENTITY;
                    ts.linematrix = (0.0, 0.0);
                }
                b"Tc" | b"Tw" | b"Tz" | b"TL" | b"Tr" | b"Ts" => {
                    if let Some(v) = pop!(1)
                        && let Some(x) = obj_num(&v[0])
                    {
                        match kw.as_slice() {
                            b"Tc" => ts.charspace = x,
                            b"Tw" => ts.wordspace = x,
                            b"Tz" => ts.scaling = x,
                            b"TL" => ts.leading = -x,
                            b"Ts" => ts.rise = x,
                            _ => {}
                        }
                    }
                }
                b"Tm" => {
                    if let Some(v) = pop!(6)
                        && let Some(m) = nums(&v)
                    {
                        ts.matrix = [m[0], m[1], m[2], m[3], m[4], m[5]];
                        ts.linematrix = (0.0, 0.0);
                    }
                }
                b"T*" => {
                    let [a, b, c, d, e, f] = ts.matrix;
                    ts.matrix = [a, b, c, d, ts.leading * c + e, ts.leading * d + f];
                    ts.linematrix = (0.0, 0.0);
                }
                b"Tj" | b"TJ" => {
                    if let Some(v) = pop!(1) {
                        let seq = match &v[0] {
                            Obj::Array(items) => items.clone(),
                            other => vec![other.clone()],
                        };
                        self.render_string(&mut ts, &ctm, &seq);
                    }
                }
                b"'" => {
                    if let Some(v) = pop!(1) {
                        let [a, b, c, d, e, f] = ts.matrix;
                        ts.matrix = [a, b, c, d, ts.leading * c + e, ts.leading * d + f];
                        ts.linematrix = (0.0, 0.0);
                        self.render_string(&mut ts, &ctm, &v);
                    }
                }
                b"\"" => {
                    if let Some(v) = pop!(3) {
                        if let Some(aw) = obj_num(&v[0]) {
                            ts.wordspace = aw;
                        }
                        if let Some(ac) = obj_num(&v[1]) {
                            ts.charspace = ac;
                        }
                        self.render_string(&mut ts, &ctm, &v[2..]);
                    }
                }
                b"d1" => {
                    let _ = pop!(6);
                }
                b"Do" => {
                    if let Some(v) = pop!(1)
                        && let Some(name) = v[0].name()
                    {
                        let name = name.to_vec();
                        self.do_xobject(&scope, &name, &ctm);
                    }
                }
                b"EI" => {
                    // The inline image itself was pushed as an operand just before `EI`.
                    if let Some(v) = pop!(1)
                        && let Obj::InlineImage(dict, _) = &v[0]
                    {
                        let has = |k: &[u8]| dict.iter().any(|(key, _)| key == k);
                        if has(b"W") && has(b"H") {
                            self.inline_count += 1;
                            let name = format!("inline{}", self.inline_count);
                            self.render_inline_image(&ctm, &name, dict);
                        }
                    }
                }
                b"BI" | b"ID" | b"EMC" | b"ET" | b"W" | b"W*" => {}
                _ => {
                    // Unknown operators leave their operands on the stack (pdfminer).
                }
            }
        }
        let _ = (&scope.resources, gs.linewidth);
        self.depth -= 1;
    }

    fn render_string(&mut self, ts: &mut TextState, ctm: &M, seq: &[Obj]) {
        let Some(font) = ts.font.clone() else {
            return;
        };
        let matrix = mult(&ts.matrix, ctm);
        let fontsize = ts.fontsize;
        let scaling = ts.scaling * 0.01;
        let charspace = ts.charspace * scaling;
        let mut wordspace = ts.wordspace * scaling;
        let rise = ts.rise;
        if font.is_multibyte() {
            wordspace = 0.0;
        }
        let dxscale = 0.001 * fontsize * scaling;
        let (mut x, y) = ts.linematrix;
        let mut needcharspace = false;
        for obj in seq {
            match obj {
                Obj::Num(n) => {
                    x -= n.f() * dxscale;
                    needcharspace = true;
                }
                Obj::Str(s) => {
                    for cid in font.decode(s) {
                        if needcharspace {
                            x += charspace;
                        }
                        let m = translate(&matrix, x, y);
                        x += self.render_char(&m, &font, fontsize, scaling, rise, cid);
                        if cid == 32 && wordspace != 0.0 {
                            x += wordspace;
                        }
                        needcharspace = true;
                    }
                }
                _ => {}
            }
        }
        ts.linematrix = (x, y);
    }

    fn render_char(&mut self, m: &M, font: &Font, fontsize: f64, scaling: f64, rise: f64, cid: u32) -> f64 {
        let text = font.char_text(cid);
        let textwidth = font.char_width(cid);
        let adv = textwidth * fontsize * scaling;
        let descent = font.get_descent() * fontsize;
        let ll = (0.0, descent + rise);
        let ur = (adv, descent + rise + fontsize);
        let [a, b, c, d, _, _] = *m;
        let upright = 0.0 < a * d * scaling && b * c <= 0.0;
        let (mut x0, mut y0) = apply(m, ll.0, ll.1);
        let (mut x1, mut y1) = apply(m, ur.0, ur.1);
        if x1 < x0 {
            std::mem::swap(&mut x0, &mut x1);
        }
        if y1 < y0 {
            std::mem::swap(&mut y0, &mut y1);
        }
        let top = self.pheight - y1;
        self.out.chars.push(Char {
            text,
            fontname: font.fontname.clone(),
            x0,
            y0,
            x1,
            y1,
            top,
            bottom: self.pheight - y0,
            doctop: self.initial_doctop + top,
            upright,
            size: y1 - y0,
            adv,
        });
        adv
    }

    fn paint_path(&mut self, ctm: &M, gs: &GraphicState, stroke: bool, fill: bool, path: &[Seg]) {
        let shape: String = path.iter().map(|s| s.op as char).collect();
        if !shape.starts_with('m') {
            return;
        }
        if shape.matches('m').count() > 1 {
            // Recurse on each `m[^m]+` subpath.
            let mut start = 0;
            for i in 1..=path.len() {
                if i == path.len() || path[i].op == b'm' {
                    if i - start > 1 {
                        self.paint_path(ctm, gs, stroke, fill, &path[start..i]);
                    }
                    start = i;
                }
            }
            return;
        }
        let first = &path[0].args;
        let raw: Vec<(f64, f64)> = path
            .iter()
            .map(|s| {
                if s.op == b'h' || s.args.len() < 2 {
                    (first[0], first[1])
                } else {
                    (s.args[s.args.len() - 2], s.args[s.args.len() - 1])
                }
            })
            .collect();
        let pts: Vec<(f64, f64)> = raw.iter().map(|&(x, y)| apply(ctm, x, y)).collect();
        let kind = if shape == "mlh" || shape == "ml" {
            PathKind::Line
        } else if shape == "mlllh" || shape == "mllll" {
            let (x0, y0) = pts[0];
            let (x1, y1) = pts[1];
            let (x2, y2) = pts[2];
            let (x3, y3) = pts[3];
            let closed = pts[0] == pts[4];
            let square = (x0 == x1 && y1 == y2 && x2 == x3 && y3 == y0)
                || (y0 == y1 && x1 == x2 && y2 == y3 && x3 == x0);
            if closed && square { PathKind::Rect } else { PathKind::Curve }
        } else {
            PathKind::Curve
        };
        let obj_pts: Vec<(f64, f64)> = match kind {
            PathKind::Line => vec![pts[0], pts[1]],
            PathKind::Rect => {
                let (x0, y0) = pts[0];
                let (x1, y1) = pts[2];
                vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            }
            PathKind::Curve => pts.clone(),
        };
        let (mut bx0, mut by0, mut bx1, mut by1) =
            (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for &(x, y) in &obj_pts {
            bx0 = bx0.min(x);
            by0 = by0.min(y);
            bx1 = bx1.max(x);
            by1 = by1.max(y);
        }
        let top = self.pheight - by1;
        self.out.paths.push(PathObj {
            kind,
            x0: bx0,
            y0: by0,
            x1: bx1,
            y1: by1,
            top,
            bottom: self.pheight - by0,
            doctop: self.initial_doctop + top,
            width: bx1 - bx0,
            height: by1 - by0,
            pts: obj_pts.iter().map(|&(x, y)| (x, self.pheight - y)).collect(),
            stroke,
            fill,
        });
    }

    fn figure_bbox(ctm: &M) -> (f64, f64, f64, f64) {
        let mut b = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (p, q) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let (x, y) = apply(ctm, p, q);
            b.0 = b.0.min(x);
            b.1 = b.1.min(y);
            b.2 = b.2.max(x);
            b.3 = b.3.max(y);
        }
        b
    }

    fn push_image(&mut self, ctm: &M, name: String, srcsize: (Value, Value), imagemask: Value, bits: Value, colorspace: Value) {
        let (x0, y0, x1, y1) = Self::figure_bbox(ctm);
        let top = self.pheight - y1;
        self.out.images.push(ImageObj {
            name,
            x0,
            y0,
            x1,
            y1,
            top,
            bottom: self.pheight - y0,
            doctop: self.initial_doctop + top,
            srcsize,
            imagemask,
            bits,
            colorspace,
        });
    }

    fn do_xobject(&mut self, scope: &Scope, name: &[u8], ctm: &M) {
        let doc = self.doc;
        let Some(obj) = scope.xobjects.get(name) else {
            return;
        };
        let obj = deref(doc, obj);
        let Object::Stream(stream) = obj else {
            return;
        };
        let d = &stream.dict;
        let subtype = dget(doc, d, b"Subtype").and_then(|o| o.as_name().ok());
        match subtype {
            Some(b"Form") if d.has(b"BBox") => {
                let matrix: M = dget(doc, d, b"Matrix")
                    .and_then(|o| o.as_array().ok())
                    .and_then(|a| {
                        let v: Vec<f64> = a.iter().filter_map(|o| num_f64(deref(doc, o))).collect();
                        (v.len() == 6).then(|| [v[0], v[1], v[2], v[3], v[4], v[5]])
                    })
                    .unwrap_or(IDENTITY);
                let res = match dget(doc, d, b"Resources").and_then(as_dict) {
                    Some(r) if !r.is_empty() => Some(r.clone()),
                    _ => Some(scope.resources.clone()),
                };
                let data = stream_data(obj).unwrap_or_default();
                let new_ctm = mult(&matrix, ctm);
                self.render_contents(res.as_ref(), &data, new_ctm);
            }
            Some(b"Image") if d.has(b"Width") && d.has(b"Height") => {
                let get = |keys: &[&[u8]]| -> Option<Value> {
                    keys.iter().find_map(|k| dget(doc, d, k)).map(|o| pdf_value(doc, o, 0))
                };
                let srcsize = (
                    get(&[b"W", b"Width"]).unwrap_or(Value::Null),
                    get(&[b"H", b"Height"]).unwrap_or(Value::Null),
                );
                let imagemask = get(&[b"IM", b"ImageMask"]).unwrap_or(Value::Null);
                let bits = get(&[b"BPC", b"BitsPerComponent"]).unwrap_or(Value::from(1));
                let colorspace = match get(&[b"CS", b"ColorSpace"]) {
                    Some(Value::Array(a)) => Value::Array(a),
                    Some(other) => Value::Array(vec![other]),
                    None => Value::Array(vec![Value::Null]),
                };
                self.push_image(
                    ctm,
                    String::from_utf8_lossy(name).into_owned(),
                    srcsize,
                    imagemask,
                    bits,
                    colorspace,
                );
            }
            _ => {}
        }
    }

    fn render_inline_image(&mut self, ctm: &M, name: &str, dict: &[(Vec<u8>, Obj)]) {
        let get = |keys: &[&[u8]]| -> Option<Value> {
            keys.iter()
                .find_map(|k| dict.iter().find(|(key, _)| key == k).map(|(_, v)| v))
                .map(obj_value)
        };
        let srcsize = (
            get(&[b"W", b"Width"]).unwrap_or(Value::Null),
            get(&[b"H", b"Height"]).unwrap_or(Value::Null),
        );
        let imagemask = get(&[b"IM", b"ImageMask"]).unwrap_or(Value::Null);
        let bits = get(&[b"BPC", b"BitsPerComponent"]).unwrap_or(Value::from(1));
        let colorspace = match get(&[b"CS", b"ColorSpace"]) {
            Some(Value::Array(a)) => Value::Array(a),
            Some(other) => Value::Array(vec![other]),
            None => Value::Array(vec![Value::Null]),
        };
        self.push_image(ctm, name.to_string(), srcsize, imagemask, bits, colorspace);
    }
}

/// A content-stream object as JSON (names as their text).
fn obj_value(o: &Obj) -> Value {
    match o {
        Obj::Num(n) => n.to_value(),
        Obj::Bool(b) => Value::Bool(*b),
        Obj::Name(n) | Obj::Str(n) | Obj::Kw(n) => Value::String(String::from_utf8_lossy(n).into_owned()),
        Obj::Array(items) => Value::Array(items.iter().map(obj_value).collect()),
        Obj::Dict(items) | Obj::InlineImage(items, _) => Value::Object(
            items
                .iter()
                .map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), obj_value(v)))
                .collect(),
        ),
    }
}

/// A PDF object as JSON (references followed up to a small depth; names as text).
pub fn pdf_value(doc: &Document, obj: &Object, depth: usize) -> Value {
    if depth > 8 {
        return Value::Null;
    }
    match deref(doc, obj) {
        Object::Null => Value::Null,
        Object::Boolean(b) => Value::Bool(*b),
        Object::Integer(i) => Value::from(*i),
        Object::Real(r) => serde_json::Number::from_f64(super::num::f32_to_f64(*r))
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Object::Name(n) => Value::String(String::from_utf8_lossy(n).into_owned()),
        Object::String(s, _) => Value::String(super::encoding::decode_text(s)),
        Object::Array(items) => Value::Array(items.iter().map(|o| pdf_value(doc, o, depth + 1)).collect()),
        Object::Dictionary(d) => Value::Object(
            d.iter()
                .map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), pdf_value(doc, v, depth + 1)))
                .collect(),
        ),
        Object::Stream(s) => Value::Object(
            s.dict
                .iter()
                .map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), pdf_value(doc, v, depth + 1)))
                .collect(),
        ),
        Object::Reference(_) => Value::Null,
    }
}
