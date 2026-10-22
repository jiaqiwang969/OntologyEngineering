//! Python number semantics for the PDF geometry: pdfminer computes with Python `int`s and
//! `float`s, and integer coordinates stay `int` through `+`, `-` and `*` (so pdfplumber
//! reports `612`, not `612.0`, for an integral MediaBox). [`Num`] keeps that distinction.

use std::ops::{Add, Mul, Neg, Sub};

use serde_json::Value;

/// A Python `int` or `float`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    /// A Python `int` (kept while it fits in `i64`).
    Int(i64),
    /// A Python `float`.
    Float(f64),
}

impl Num {
    /// The value as `f64` (`float(x)`).
    pub fn f(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }

    /// `x == 0`.
    pub fn is_zero(self) -> bool {
        self.f() == 0.0
    }

    /// The JSON form (`int` stays integral).
    pub fn to_value(self) -> Value {
        match self {
            Num::Int(i) => Value::from(i),
            Num::Float(f) => serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        }
    }

    /// `min(a, b)` as Python computes it (first argument wins ties).
    pub fn min(self, other: Num) -> Num {
        if other.f() < self.f() { other } else { self }
    }

    /// `max(a, b)` as Python computes it (first argument wins ties).
    pub fn max(self, other: Num) -> Num {
        if other.f() > self.f() { other } else { self }
    }
}

impl From<i64> for Num {
    fn from(v: i64) -> Self {
        Num::Int(v)
    }
}

impl From<f64> for Num {
    fn from(v: f64) -> Self {
        Num::Float(v)
    }
}

impl Add for Num {
    type Output = Num;
    fn add(self, rhs: Num) -> Num {
        match (self, rhs) {
            (Num::Int(a), Num::Int(b)) => a
                .checked_add(b)
                .map(Num::Int)
                .unwrap_or(Num::Float(a as f64 + b as f64)),
            (a, b) => Num::Float(a.f() + b.f()),
        }
    }
}

impl Sub for Num {
    type Output = Num;
    fn sub(self, rhs: Num) -> Num {
        match (self, rhs) {
            (Num::Int(a), Num::Int(b)) => a
                .checked_sub(b)
                .map(Num::Int)
                .unwrap_or(Num::Float(a as f64 - b as f64)),
            (a, b) => Num::Float(a.f() - b.f()),
        }
    }
}

impl Mul for Num {
    type Output = Num;
    fn mul(self, rhs: Num) -> Num {
        match (self, rhs) {
            (Num::Int(a), Num::Int(b)) => a
                .checked_mul(b)
                .map(Num::Int)
                .unwrap_or(Num::Float(a as f64 * b as f64)),
            (a, b) => Num::Float(a.f() * b.f()),
        }
    }
}

impl Neg for Num {
    type Output = Num;
    fn neg(self) -> Num {
        match self {
            Num::Int(a) => a.checked_neg().map(Num::Int).unwrap_or(Num::Float(-(a as f64))),
            Num::Float(f) => Num::Float(-f),
        }
    }
}

/// A PDF transformation matrix `(a, b, c, d, e, f)`.
pub type Matrix = [Num; 6];

/// `MATRIX_IDENTITY`.
pub const IDENTITY: Matrix = [
    Num::Int(1),
    Num::Int(0),
    Num::Int(0),
    Num::Int(1),
    Num::Int(0),
    Num::Int(0),
];

/// pdfminer's `mult_matrix(m1, m0)`: `m1` applied first, then `m0`.
pub fn mult_matrix(m1: &Matrix, m0: &Matrix) -> Matrix {
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

/// pdfminer's `translate_matrix(m, (x, y))`.
pub fn translate_matrix(m: &Matrix, x: Num, y: Num) -> Matrix {
    let [a, b, c, d, e, f] = *m;
    [a, b, c, d, x * a + y * c + e, x * b + y * d + f]
}

/// pdfminer's `apply_matrix_pt(m, (x, y))`.
pub fn apply_matrix_pt(m: &Matrix, x: Num, y: Num) -> (Num, Num) {
    let [a, b, c, d, e, f] = *m;
    (a * x + c * y + e, b * x + d * y + f)
}

/// The shortest decimal text of an `f32` read back as `f64`: lopdf stores PDF reals as
/// `f32`, pdfminer as Python floats parsed from the same decimal text.
pub fn f32_to_f64(v: f32) -> f64 {
    format!("{v}").parse().unwrap_or(v as f64)
}
