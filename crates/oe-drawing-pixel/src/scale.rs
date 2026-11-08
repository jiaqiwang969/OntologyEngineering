//! Drawing scale from dimension values against measured pixel extents.

use crate::geom::median;

/// Robust pixels-per-unit scale from `(nominal value, measured px)` samples.
///
/// The scale is the median of the per-sample ratios `measured / nominal`; samples whose ratio is
/// further than `max(3 · 1.4826 · MAD, 2 %)` from that median are rejected as outliers (wrong
/// dimension reading, wrong pairing); the survivors are refit by least squares through the origin,
/// which weights long dimensions more because pixel measurement error is roughly constant in pixels.
/// Returns `(px per unit, median relative residual of the kept samples)`, or `None` when fewer than
/// two usable samples remain. Samples with non-positive or non-finite values are ignored.
pub fn calibrate_scale(samples: &[(f64, f64)]) -> Option<(f64, f64)> {
    let usable: Vec<(f64, f64, f64)> = samples
        .iter()
        .filter(|(n, m)| n.is_finite() && m.is_finite() && *n > 0.0 && *m > 0.0)
        .map(|&(n, m)| (m / n, n, m))
        .collect();
    if usable.len() < 2 {
        return None;
    }
    let mut ratios: Vec<f64> = usable.iter().map(|u| u.0).collect();
    let med = median(&mut ratios)?;
    let mut dev: Vec<f64> = usable.iter().map(|u| (u.0 - med).abs()).collect();
    let mad = median(&mut dev)?;
    let tol = (3.0 * 1.4826 * mad).max(0.02 * med);
    let kept: Vec<&(f64, f64, f64)> = usable.iter().filter(|u| (u.0 - med).abs() <= tol).collect();
    if kept.len() < 2 {
        return None;
    }
    let (mut num, mut den) = (0.0, 0.0);
    for &&(_, n, m) in &kept {
        num += m * n;
        den += n * n;
    }
    let scale = num / den;
    if !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    let mut rel: Vec<f64> = kept
        .iter()
        .map(|&&(_, n, m)| (m / (scale * n) - 1.0).abs())
        .collect();
    let residual = median(&mut rel)?;
    Some((scale, residual))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unusable_input() {
        assert!(calibrate_scale(&[]).is_none());
        assert!(calibrate_scale(&[(10.0, 40.0)]).is_none());
        assert!(calibrate_scale(&[(0.0, 40.0), (10.0, 40.0)]).is_none());
        assert!(calibrate_scale(&[(f64::NAN, 40.0), (10.0, 40.0), (20.0, 80.0)]).is_some());
    }

    #[test]
    fn exact_samples_give_exact_scale() {
        let (s, r) = calibrate_scale(&[(10.0, 25.0), (20.0, 50.0), (30.0, 75.0)]).unwrap();
        assert!((s - 2.5).abs() < 1e-12);
        assert!(r < 1e-12);
    }
}
