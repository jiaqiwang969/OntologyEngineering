//! Small deterministic geometry helpers: vectors, robust statistics, orthogonal line fit and
//! geometric circle fit. Everything is plain `f64` arithmetic in fixed evaluation order.

use std::f64::consts::{PI, TAU};

/// A point or vector in pixel-centre coordinates.
pub(crate) type P = (f64, f64);

#[inline]
pub(crate) fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}

#[inline]
pub(crate) fn add(a: P, b: P) -> P {
    (a.0 + b.0, a.1 + b.1)
}

#[inline]
pub(crate) fn mul(a: P, k: f64) -> P {
    (a.0 * k, a.1 * k)
}

#[inline]
pub(crate) fn dot(a: P, b: P) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

#[inline]
pub(crate) fn norm(a: P) -> f64 {
    a.0.hypot(a.1)
}

#[inline]
pub(crate) fn dist(a: P, b: P) -> f64 {
    norm(sub(a, b))
}

/// Left normal `(-dy, dx)` of a direction.
#[inline]
pub(crate) fn perp(a: P) -> P {
    (-a.1, a.0)
}

/// Unit vector, or `None` for a (numerically) zero vector.
pub(crate) fn unit(a: P) -> Option<P> {
    let n = norm(a);
    if n > 1e-12 && n.is_finite() {
        Some((a.0 / n, a.1 / n))
    } else {
        None
    }
}

/// Euclidean distance from `p` to the closed segment `a`–`b`.
pub(crate) fn dist_point_segment(p: P, a: P, b: P) -> f64 {
    let ab = sub(b, a);
    let l2 = dot(ab, ab);
    if l2 <= 1e-18 {
        return dist(p, a);
    }
    let t = (dot(sub(p, a), ab) / l2).clamp(0.0, 1.0);
    dist(p, add(a, mul(ab, t)))
}

/// Median of a slice (sorted in place). `None` for an empty slice.
pub(crate) fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    Some(if n % 2 == 1 {
        values[n / 2]
    } else {
        0.5 * (values[n / 2 - 1] + values[n / 2])
    })
}

/// Wrap an angle into `(-π, π]`.
pub(crate) fn wrap_pi(a: f64) -> f64 {
    let mut a = a % TAU;
    if a <= -PI {
        a += TAU;
    } else if a > PI {
        a -= TAU;
    }
    a
}

/// Smallest angle between two undirected directions (mod π), in `[0, π/2]`.
pub(crate) fn undirected_angle(a: P, b: P) -> f64 {
    let c = dot(a, b).abs() / (norm(a) * norm(b));
    c.clamp(-1.0, 1.0).acos()
}

/// Orthogonal (total least squares) line through a point set.
pub(crate) struct LineFit {
    /// Centroid of the points (a point on the line).
    pub c: P,
    /// Unit direction (principal axis); the sign is arbitrary.
    pub d: P,
}

/// Principal-axis line fit. `None` with fewer than two points or when all points coincide.
pub(crate) fn fit_line(pts: &[P]) -> Option<LineFit> {
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let nf = n as f64;
    let (mut sx, mut sy) = (0.0, 0.0);
    for p in pts {
        sx += p.0;
        sy += p.1;
    }
    let c = (sx / nf, sy / nf);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in pts {
        let u = p.0 - c.0;
        let v = p.1 - c.1;
        sxx += u * u;
        sxy += u * v;
        syy += v * v;
    }
    if sxx + syy <= 1e-18 {
        return None;
    }
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    Some(LineFit {
        c,
        d: (theta.cos(), theta.sin()),
    })
}

/// Geometric circle fit.
pub(crate) struct CircleFit {
    pub c: P,
    pub r: f64,
    /// Largest radial deviation `| |p - c| - r |`.
    pub max_dev: f64,
    /// Root mean square radial deviation.
    pub rms: f64,
}

/// Kåsa algebraic fit on centred data followed by Gauss–Newton refinement of the geometric
/// (radial) residuals. `None` for fewer than three points or (near) collinear points.
pub(crate) fn fit_circle(pts: &[P]) -> Option<CircleFit> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let nf = n as f64;
    let (mut mx, mut my) = (0.0, 0.0);
    for p in pts {
        mx += p.0;
        my += p.1;
    }
    mx /= nf;
    my /= nf;
    let (mut suu, mut suv, mut svv) = (0.0, 0.0, 0.0);
    let (mut suuu, mut svvv, mut suvv, mut svuu) = (0.0, 0.0, 0.0, 0.0);
    for p in pts {
        let u = p.0 - mx;
        let v = p.1 - my;
        suu += u * u;
        suv += u * v;
        svv += v * v;
        suuu += u * u * u;
        svvv += v * v * v;
        suvv += u * v * v;
        svuu += v * u * u;
    }
    let det = suu * svv - suv * suv;
    let scale = (suu + svv).max(1e-300);
    if det.abs() <= 1e-9 * scale * scale {
        return None;
    }
    let b1 = 0.5 * (suuu + suvv);
    let b2 = 0.5 * (svvv + svuu);
    let uc = (b1 * svv - b2 * suv) / det;
    let vc = (suu * b2 - suv * b1) / det;
    let mut a = mx + uc;
    let mut b = my + vc;
    let mut r = (uc * uc + vc * vc + (suu + svv) / nf).sqrt();
    if !(r.is_finite() && a.is_finite() && b.is_finite()) {
        return None;
    }
    for _ in 0..25 {
        let mut jtj = [[0.0f64; 3]; 3];
        let mut jtf = [0.0f64; 3];
        for p in pts {
            let dx = p.0 - a;
            let dy = p.1 - b;
            let d = dx.hypot(dy);
            if d < 1e-9 {
                continue;
            }
            let j = [-dx / d, -dy / d, -1.0];
            let f = d - r;
            for i in 0..3 {
                for k in 0..3 {
                    jtj[i][k] += j[i] * j[k];
                }
                jtf[i] += j[i] * f;
            }
        }
        let Some(delta) = solve3(jtj, [-jtf[0], -jtf[1], -jtf[2]]) else {
            break;
        };
        let step = delta.iter().fold(0.0f64, |m, x| m.max(x.abs()));
        if !step.is_finite() {
            break;
        }
        a += delta[0];
        b += delta[1];
        r += delta[2];
        if step < 1e-7 {
            break;
        }
    }
    if !(r.is_finite() && r > 0.0 && a.is_finite() && b.is_finite()) {
        return None;
    }
    let mut max_dev: f64 = 0.0;
    let mut ss = 0.0;
    for p in pts {
        let e = (dist(*p, (a, b)) - r).abs();
        max_dev = max_dev.max(e);
        ss += e * e;
    }
    Some(CircleFit {
        c: (a, b),
        r,
        max_dev,
        rms: (ss / nf).sqrt(),
    })
}

/// Solve a 3×3 linear system by Gaussian elimination with partial pivoting.
pub(crate) fn solve3(mut m: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let mut piv = col;
        for row in col + 1..3 {
            if m[row][col].abs() > m[piv][col].abs() {
                piv = row;
            }
        }
        if m[piv][col].abs() < 1e-12 {
            return None;
        }
        if piv != col {
            m.swap(piv, col);
            b.swap(piv, col);
        }
        let pivot_row = m[col];
        for row in col + 1..3 {
            let f = m[row][col] / pivot_row[col];
            for (mk, pk) in m[row].iter_mut().zip(pivot_row.iter()).skip(col) {
                *mk -= f * pk;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0.0; 3];
    for i in (0..3).rev() {
        let mut s = b[i];
        for k in i + 1..3 {
            s -= m[i][k] * x[k];
        }
        x[i] = s / m[i][i];
    }
    Some(x)
}

/// Parameter range `[t0, t1]` for which `c + t·d` lies inside the box `[x0, x1] × [y0, y1]`.
/// `None` when the line misses the box.
pub(crate) fn clip_line_to_box(
    c: P,
    d: P,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> Option<(f64, f64)> {
    let mut lo = f64::NEG_INFINITY;
    let mut hi = f64::INFINITY;
    for (cc, dd, a, b) in [(c.0, d.0, x0, x1), (c.1, d.1, y0, y1)] {
        if dd.abs() < 1e-12 {
            if cc < a || cc > b {
                return None;
            }
        } else {
            let ta = (a - cc) / dd;
            let tb = (b - cc) / dd;
            lo = lo.max(ta.min(tb));
            hi = hi.min(ta.max(tb));
        }
    }
    if lo.is_infinite() || hi.is_infinite() || lo > hi {
        return None;
    }
    Some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_fit_recovers_exact_circle() {
        let pts: Vec<P> = (0..36)
            .map(|i| {
                let a = i as f64 * 10f64.to_radians();
                (50.0 + 20.0 * a.cos(), 30.0 + 20.0 * a.sin())
            })
            .collect();
        let f = fit_circle(&pts).unwrap();
        assert!((f.c.0 - 50.0).abs() < 1e-6 && (f.c.1 - 30.0).abs() < 1e-6);
        assert!((f.r - 20.0).abs() < 1e-6);
        assert!(f.max_dev < 1e-6);
    }

    #[test]
    fn line_fit_handles_vertical_lines() {
        let pts: Vec<P> = (0..20).map(|i| (7.0, i as f64)).collect();
        let f = fit_line(&pts).unwrap();
        assert!(f.d.0.abs() < 1e-9 && (f.d.1.abs() - 1.0).abs() < 1e-9);
        assert!((f.c.0 - 7.0).abs() < 1e-9);
    }

    #[test]
    fn collinear_points_are_not_a_circle() {
        let pts: Vec<P> = (0..10).map(|i| (i as f64, 2.0 * i as f64)).collect();
        assert!(fit_circle(&pts).is_none());
    }
}
