//! Vectorised ink: primitives, patterns and strokes.

use crate::geom::{P, dist};

/// A geometric primitive in pixel-centre coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    /// Straight segment from `p` to `q`.
    Line { p: (f64, f64), q: (f64, f64) },
    /// Circular arc with centre `c` and radius `r`, starting at angle `start_rad` (`atan2(dy, dx)` in
    /// image coordinates, in `[0, 2π)`) and sweeping `sweep_rad` (signed; `±2π` for a full circle).
    Arc {
        c: (f64, f64),
        r: f64,
        start_rad: f64,
        sweep_rad: f64,
    },
}

impl Primitive {
    /// Length along the primitive in pixels.
    pub fn length(&self) -> f64 {
        match *self {
            Primitive::Line { p, q } => dist(p, q),
            Primitive::Arc { r, sweep_rad, .. } => r * sweep_rad.abs(),
        }
    }

    /// Point at normalised parameter `t ∈ [0, 1]` along the primitive.
    pub fn point_at(&self, t: f64) -> (f64, f64) {
        match *self {
            Primitive::Line { p, q } => (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t),
            Primitive::Arc {
                c,
                r,
                start_rad,
                sweep_rad,
            } => {
                let a = start_rad + sweep_rad * t;
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            }
        }
    }

    /// Start point.
    pub fn start(&self) -> (f64, f64) {
        self.point_at(0.0)
    }

    /// End point.
    pub fn end(&self) -> (f64, f64) {
        self.point_at(1.0)
    }

    /// Midpoint along the primitive.
    pub fn mid(&self) -> (f64, f64) {
        self.point_at(0.5)
    }
}

/// Line pattern inferred from collinear / co-circular pieces with regular gaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// A single uninterrupted stroke (or pieces that touch).
    Continuous,
    /// Regular dashes of similar length separated by regular gaps.
    Dashed,
    /// Alternating long dashes and short dots separated by regular gaps (centre lines).
    DashDot,
    /// Fragmented collinear / co-circular run without a regular pattern.
    Unknown,
}

impl Pattern {
    /// Union used when strokes are merged: the more specific pattern wins
    /// (`Unknown < Continuous < Dashed < DashDot`).
    pub fn union(self, other: Pattern) -> Pattern {
        fn rank(p: Pattern) -> u8 {
            match p {
                Pattern::Unknown => 0,
                Pattern::Continuous => 1,
                Pattern::Dashed => 2,
                Pattern::DashDot => 3,
            }
        }
        if rank(other) > rank(self) {
            other
        } else {
            self
        }
    }
}

/// A vectorised stroke with its measurements and evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// Geometry of the stroke.
    pub primitive: Primitive,
    /// Median stroke thickness in pixels, measured perpendicular to the skeleton.
    pub width_px: f64,
    /// Line pattern this stroke belongs to.
    pub pattern: Pattern,
    /// Fraction of 1-px samples along `primitive` that have ink within 1.5 px.
    pub support: f64,
    /// Number of skeleton pixels that produced the stroke (summed when strokes are merged).
    pub pixels: usize,
}

/// Direction of a line primitive as a unit vector, `None` for arcs or degenerate lines.
pub(crate) fn line_direction(p: &Primitive) -> Option<P> {
    match *p {
        Primitive::Line { p, q } => crate::geom::unit(crate::geom::sub(q, p)),
        Primitive::Arc { .. } => None,
    }
}
