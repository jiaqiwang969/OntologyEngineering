//! Overlay rendering of a pixel graph: one `<g data-entity>` per entity in drawing coordinates, with
//! the frame attributes of the DXF route's rendering (`viewBox`, `data-drawing-origin-x`,
//! `data-drawing-top-y`), so the review page can lay the page raster under it. No image, script or
//! link is ever emitted.

use std::fmt::Write;

/// A rendering frame: the page in drawing units, origin at the page's bottom-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub width_units: f64,
    pub height_units: f64,
}

impl Frame {
    /// SVG user coordinates of a drawing point (x right, y down from the top edge).
    pub fn svg(&self, p: (f64, f64)) -> (f64, f64) {
        (p.0, self.height_units - p.1)
    }
}

fn n(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_owned() }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;")
}

/// Stroke style of a class of entity.
pub fn style(class: &str) -> (&'static str, f64, &'static str) {
    match class {
        "visible_contour" => ("#1a1a1a", 0.5, ""),
        "hidden_contour" => ("#555555", 0.25, " stroke-dasharray=\"2,1\""),
        "center_axis" => ("#c0007a", 0.18, " stroke-dasharray=\"6,1,1,1\""),
        "annotation" => ("#0057b8", 0.18, ""),
        "sheet" => ("#777777", 0.25, ""),
        "unresolved" => ("#e07b00", 0.18, " stroke-dasharray=\"1,1\""),
        "break_line" => ("#8a5a00", 0.25, ""),
        _ => ("#0057b8", 0.18, ""),
    }
}

/// Collected SVG body of an overlay.
#[derive(Default)]
pub struct Overlay {
    body: String,
    pub entities: usize,
}

impl Overlay {
    fn open(&mut self, id: &str) {
        let _ = write!(self.body, "<g data-entity=\"{}\">", esc(id));
        self.entities += 1;
    }

    pub fn line(&mut self, f: &Frame, id: &str, class: &str, p: (f64, f64), q: (f64, f64)) {
        let (colour, width, extra) = style(class);
        let (a, b) = (f.svg(p), f.svg(q));
        self.open(id);
        let _ = write!(
            self.body,
            "<polyline fill=\"none\" stroke=\"{colour}\" stroke-width=\"{}\" stroke-linecap=\"round\"{extra} points=\"{},{} {},{}\"/></g>",
            n(width), n(a.0), n(a.1), n(b.0), n(b.1)
        );
    }

    pub fn polyline(&mut self, f: &Frame, id: &str, class: &str, points: &[(f64, f64)], closed: bool) {
        let (colour, width, extra) = style(class);
        self.open(id);
        let pts: Vec<String> = points
            .iter()
            .map(|p| {
                let s = f.svg(*p);
                format!("{},{}", n(s.0), n(s.1))
            })
            .collect();
        let tag = if closed { "polygon" } else { "polyline" };
        let _ = write!(
            self.body,
            "<{tag} fill=\"none\" stroke=\"{colour}\" stroke-width=\"{}\" stroke-linejoin=\"round\"{extra} points=\"{}\"/></g>",
            n(width), pts.join(" ")
        );
    }

    /// Arc in drawing coordinates: centre, radius, start angle and sweep in degrees (y up).
    #[allow(clippy::too_many_arguments)]
    pub fn arc(&mut self, f: &Frame, id: &str, class: &str, c: (f64, f64), r: f64, start_deg: f64, sweep_deg: f64) {
        let (colour, width, extra) = style(class);
        self.open(id);
        if sweep_deg.abs() >= 360.0 - 1e-6 {
            let s = f.svg(c);
            let _ = write!(
                self.body,
                "<circle fill=\"none\" stroke=\"{colour}\" stroke-width=\"{}\"{extra} cx=\"{}\" cy=\"{}\" r=\"{}\"/></g>",
                n(width), n(s.0), n(s.1), n(r)
            );
            return;
        }
        let point = |deg: f64| {
            let a = deg.to_radians();
            f.svg((c.0 + r * a.cos(), c.1 + r * a.sin()))
        };
        let a = point(start_deg);
        let b = point(start_deg + sweep_deg);
        let large = u8::from(sweep_deg.abs() > 180.0);
        // A positive sweep is counter-clockwise with y up, which is the SVG negative-angle direction.
        let sweep = u8::from(sweep_deg < 0.0);
        let _ = write!(
            self.body,
            "<path fill=\"none\" stroke=\"{colour}\" stroke-width=\"{}\" stroke-linecap=\"round\"{extra} d=\"M{} {}A{} {} 0 {large} {sweep} {} {}\"/></g>",
            n(width), n(a.0), n(a.1), n(r), n(r), n(b.0), n(b.1)
        );
    }

    /// Text box of a recovered text: insertion point, height, rotation (0 or 90) and total width.
    #[allow(clippy::too_many_arguments)]
    pub fn text_box(&mut self, f: &Frame, id: &str, origin: (f64, f64), height: f64, rotation_deg: f64, width: f64, doubtful: bool) {
        let rad = rotation_deg.to_radians();
        let (c, s) = (rad.cos(), rad.sin());
        let corners = [
            origin,
            (origin.0 + width * c, origin.1 + width * s),
            (origin.0 + width * c - height * s, origin.1 + width * s + height * c),
            (origin.0 - height * s, origin.1 + height * c),
        ];
        let colour = if doubtful { "#d00000" } else { "#008a3e" };
        self.open(id);
        let pts: Vec<String> = corners
            .iter()
            .map(|p| {
                let q = f.svg(*p);
                format!("{},{}", n(q.0), n(q.1))
            })
            .collect();
        let _ = write!(
            self.body,
            "<polygon fill=\"none\" stroke=\"{colour}\" stroke-width=\"0.18\" points=\"{}\"/></g>",
            pts.join(" ")
        );
    }

    /// Filled arrowhead at `tip` pointing along `along` (drawing coordinates), `length` long.
    pub fn arrowhead(&mut self, f: &Frame, id: &str, tip: (f64, f64), along: (f64, f64), length: f64) {
        let nrm = along.0.hypot(along.1);
        let a = if nrm > 1e-9 { (along.0 / nrm, along.1 / nrm) } else { (1.0, 0.0) };
        let base = (tip.0 - a.0 * length, tip.1 - a.1 * length);
        let half = length * 0.18;
        let p1 = (base.0 - a.1 * half, base.1 + a.0 * half);
        let p2 = (base.0 + a.1 * half, base.1 - a.0 * half);
        let (t, u, v) = (f.svg(tip), f.svg(p1), f.svg(p2));
        self.open(id);
        let _ = write!(
            self.body,
            "<path fill=\"#ff0000\" d=\"M{} {}L{} {}L{} {}Z\"/></g>",
            n(t.0), n(t.1), n(u.0), n(u.1), n(v.0), n(v.1)
        );
    }

    /// A point marker (datum terminal).
    pub fn point(&mut self, f: &Frame, id: &str, p: (f64, f64), r: f64) {
        let s = f.svg(p);
        self.open(id);
        let _ = write!(
            self.body,
            "<circle fill=\"#ff0000\" cx=\"{}\" cy=\"{}\" r=\"{}\"/></g>",
            n(s.0), n(s.1), n(r)
        );
    }

    /// The complete SVG document.
    pub fn finish(&self, f: &Frame, width_px: u32) -> String {
        let height_px = ((f64::from(width_px) * f.height_units / f.width_units).round() as u32).max(1);
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" data-drawing-origin-x=\"0\" data-drawing-top-y=\"{}\" width=\"{width_px}\" height=\"{height_px}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/>{}</svg>",
            n(f.width_units),
            n(f.height_units),
            n(f.height_units),
            self.body
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_has_the_frame_attributes_and_one_group_per_entity() {
        let f = Frame {
            width_units: 100.0,
            height_units: 50.0,
        };
        let mut o = Overlay::default();
        o.line(&f, "px:line:0001", "visible_contour", (0.0, 0.0), (10.0, 0.0));
        o.arc(&f, "px:arc:0001", "visible_contour", (20.0, 20.0), 5.0, 0.0, 90.0);
        o.arc(&f, "px:circle:0001", "visible_contour", (20.0, 20.0), 5.0, 0.0, 360.0);
        o.text_box(&f, "px:text:0001", (1.0, 1.0), 3.5, 90.0, 7.0, false);
        o.arrowhead(&f, "px:solid:0001", (10.0, 0.0), (1.0, 0.0), 3.0);
        let svg = o.finish(&f, 1600);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 50\" data-drawing-origin-x=\"0\" data-drawing-top-y=\"50\" width=\"1600\" height=\"800\">"));
        assert_eq!(svg.matches("<g data-entity=").count(), 5);
        // The line at drawing y = 0 lies on the bottom edge, SVG y = 50.
        assert!(svg.contains("points=\"0,50 10,50\""));
        // The quarter arc from (25, 20) to (20, 25): SVG (25, 30) to (20, 25), small, counter-clockwise.
        assert!(svg.contains("d=\"M25 30A5 5 0 0 0 20 25\""), "{svg}");
        assert!(!svg.contains("<image") && !svg.contains("<script") && !svg.contains("href"));
    }
}
