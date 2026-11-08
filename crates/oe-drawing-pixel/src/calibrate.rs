//! Scale calibration (design §2): the nominal values of linear and diameter dimensions against the
//! pixel distance between their two refined extension lines (or two arrow tips) give pixels per
//! drawing unit; the title-block scale note is the cross-check and the fallback.

use crate::geom::{dist, dot, perp, sub, unit};
use crate::observe::{Category, P, Shape};
use crate::refine::{RefinedMember, RefinedProposal};
use crate::scale::calibrate_scale;

/// Pixels per paper millimetre at the page resolution.
pub fn px_per_paper_mm(dpi: f64) -> f64 {
    dpi / 25.4
}

/// Paper millimetres per drawing unit of a scale note `a:b` (`1:1.5` → 1/1.5, `2:1` → 2).
pub fn parse_scale_note(note: &str) -> Option<f64> {
    let cleaned: String = note
        .chars()
        .map(|c| match c {
            '：' => ':',
            '／' | '/' => ':',
            _ => c,
        })
        .filter(|c| !c.is_whitespace())
        .collect();
    let (a, b) = cleaned.split_once(':')?;
    let a: f64 = a.parse().ok()?;
    let b: f64 = b.parse().ok()?;
    (a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite()).then_some(a / b)
}

/// Nominal value of a dimension text: the first number of the first token after the size symbol,
/// a leading count (`2x`, `3-`) skipped; a stacked tolerance (`25 0/-0.50`) is a later token and
/// never joins the nominal. `None` without a number.
pub fn nominal_value(text: &str) -> Option<f64> {
    let first = text
        .split_whitespace()
        .map(|t| t.trim_matches(|c| c == '☆' || c == '(' || c == ')'))
        .find(|t| t.chars().any(|c| c.is_ascii_digit()))?;
    let t: String = first.chars().filter(|c| !c.is_whitespace()).collect();
    let mut s = t.as_str();
    // Count prefix: digits followed by x or - (as in 2x45° or 6-M8).
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0
        && let Some(c) = s[digits..].chars().next()
        && matches!(c, 'x' | 'X' | '×' | '-')
        && s[digits + c.len_utf8()..].chars().next().is_some_and(|n| !n.is_ascii_digit() || c != '-')
    {
        s = &s[digits + c.len_utf8()..];
    }
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let rest = &s[start..];
    let end = rest
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.'))
        .map_or(rest.len(), |(i, _)| i);
    let number = rest[..end].trim_end_matches('.');
    let v: f64 = number.parse().ok()?;
    (v > 0.0 && v.is_finite()).then_some(v)
}

fn segment(m: &RefinedMember) -> Option<(P, P)> {
    match &m.refined {
        Shape::Segment(p, q) if m.supported => Some((*p, *q)),
        _ => None,
    }
}

fn mid(s: (P, P)) -> P {
    ((s.0.0 + s.1.0) * 0.5, (s.0.1 + s.1.1) * 0.5)
}

/// Measured pixel extent of a dimension: the distance between its two refined extension lines
/// along the dimension-line direction, or between its two refined arrow tips.
pub fn measured_extent(members: &[RefinedMember]) -> Option<(f64, &'static str)> {
    let ext: Vec<(P, P)> = members
        .iter()
        .filter(|m| m.role == "extension_line")
        .filter_map(segment)
        .collect();
    let dimension = members
        .iter()
        .filter(|m| m.role == "dimension_line")
        .find_map(segment);
    if ext.len() == 2 {
        let d = match dimension.and_then(|(p, q)| unit(sub(q, p))) {
            Some(d) => d,
            None => {
                // Perpendicular to the extension lines' mean direction.
                let a = unit(sub(ext[0].1, ext[0].0))?;
                perp(a)
            }
        };
        let extent = (dot(sub(mid(ext[0]), mid(ext[1])), d)).abs();
        if extent > 10.0 {
            return Some((extent, "extension_lines"));
        }
    }
    let tips: Vec<P> = members
        .iter()
        .filter(|m| m.role == "arrowhead" && m.supported)
        .filter_map(|m| match m.refined {
            Shape::Tip { approx, .. } => Some(approx),
            _ => None,
        })
        .collect();
    if tips.len() == 2 {
        let extent = dist(tips[0], tips[1]);
        if extent > 10.0 {
            return Some((extent, "arrow_tips"));
        }
    }
    None
}

/// One calibration sample.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub label: String,
    pub kind: String,
    pub nominal: f64,
    pub px: f64,
    pub ratio: f64,
    pub basis: &'static str,
    pub origin: String,
}

/// The page scale and how it was obtained.
#[derive(Clone, Debug, PartialEq)]
pub struct Calibration {
    /// Pixels per drawing unit used for the graph.
    pub px_per_unit: f64,
    /// `dimensions`, `scale_note` or `paper_mm`.
    pub basis: String,
    pub dimension_px_per_unit: Option<f64>,
    /// Median relative residual of the kept samples.
    pub residual: Option<f64>,
    pub samples: Vec<Sample>,
    pub scale_note: Option<String>,
    pub scale_note_px_per_unit: Option<f64>,
    /// `dimension / scale note − 1`, in percent, when both exist.
    pub deviation_percent: Option<f64>,
    pub dpi: f64,
}

impl Calibration {
    /// Drawing coordinates of a page pixel point: x right, y up from the page bottom.
    pub fn to_drawing(&self, p: P, page_h_px: f64) -> P {
        (p.0 / self.px_per_unit, (page_h_px - p.1) / self.px_per_unit)
    }
}

/// Calibrate the page: dimensions when at least three samples agree, else the scale note, else one
/// drawing unit per paper millimetre.
pub fn calibrate(proposals: &[RefinedProposal], scale_note: Option<&str>, dpi: f64) -> Calibration {
    let mut samples = Vec::new();
    for p in proposals {
        if p.proposal.category != Category::Dimension || !["linear", "diameter"].contains(&p.proposal.kind.as_str()) {
            continue;
        }
        let Some(nominal) = nominal_value(&p.proposal.text) else {
            continue;
        };
        let Some((px, basis)) = measured_extent(&p.members) else {
            continue;
        };
        samples.push(Sample {
            label: p.proposal.text.clone(),
            kind: p.proposal.kind.clone(),
            nominal,
            px,
            ratio: px / nominal,
            basis,
            origin: format!("{}#{}", p.proposal.provenance.tile, p.proposal.provenance.item_index),
        });
    }
    let pairs: Vec<(f64, f64)> = samples.iter().map(|s| (s.nominal, s.px)).collect();
    let fitted = if pairs.len() >= 3 { calibrate_scale(&pairs) } else { None };
    let note_px = scale_note
        .and_then(parse_scale_note)
        .map(|paper_mm_per_unit| px_per_paper_mm(dpi) * paper_mm_per_unit);
    let (px_per_unit, basis) = match (fitted, note_px) {
        (Some((s, _)), _) => (s, "dimensions"),
        (None, Some(n)) => (n, "scale_note"),
        (None, None) => (px_per_paper_mm(dpi), "paper_mm"),
    };
    Calibration {
        px_per_unit,
        basis: basis.into(),
        dimension_px_per_unit: fitted.map(|f| f.0),
        residual: fitted.map(|f| f.1),
        samples,
        scale_note: scale_note.map(str::to_owned),
        scale_note_px_per_unit: note_px,
        deviation_percent: match (fitted, note_px) {
            (Some((s, _)), Some(n)) if n > 0.0 => Some((s / n - 1.0) * 100.0),
            _ => None,
        },
        dpi,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::{Proposal, ProposedMember, Provenance};

    #[test]
    fn scale_notes_and_nominal_values_are_parsed() {
        assert!((parse_scale_note("1:1.5").unwrap() - 1.0 / 1.5).abs() < 1e-12);
        assert_eq!(parse_scale_note("2:1"), Some(2.0));
        assert_eq!(parse_scale_note("1：2"), Some(0.5));
        assert_eq!(parse_scale_note(""), None);
        assert_eq!(parse_scale_note("x"), None);
        assert_eq!(nominal_value("☆Ø35 +0.0139/+0.0038"), Some(35.0));
        assert_eq!(nominal_value("Ø31.5"), Some(31.5));
        assert_eq!(nominal_value("2x45°"), Some(45.0));
        assert_eq!(nominal_value("2×Ø5"), Some(5.0));
        assert_eq!(nominal_value("6-M8"), Some(8.0));
        assert_eq!(nominal_value("(211)"), Some(211.0));
        assert_eq!(nominal_value("☆"), None);
        assert_eq!(nominal_value("M8"), Some(8.0));
        assert_eq!(nominal_value("25 0/-0.50"), Some(25.0));
        assert_eq!(nominal_value("☆10 0/-0.036"), Some(10.0));
        assert_eq!(nominal_value("☆ Ø57.8±0.05"), Some(57.8));
        assert_eq!(nominal_value("☆ 10"), Some(10.0));
    }

    fn dimension(text: &str, kind: &str, x0: f64, x1: f64, y: f64) -> RefinedProposal {
        let seg = |p: P, q: P| RefinedMember {
            role: "extension_line".into(),
            text: None,
            proposed: Shape::Segment(p, q),
            refined: Shape::Segment(p, q),
            support_before: 1.0,
            support_after: 1.0,
            moved_px: 0.0,
            width_px: 2.0,
            pattern: None,
            supported: true,
            merged_from: None,
            arrow_base: None,
            arrow_length_px: None,
            discarded_components: 0,
            adjustments: Vec::new(),
        };
        let mut dl = seg((x0, y), (x1, y));
        dl.role = "dimension_line".into();
        let members = vec![dl, seg((x0, y - 60.0), (x0, y + 10.0)), seg((x1, y - 60.0), (x1, y + 10.0))];
        RefinedProposal {
            proposal: Proposal {
                category: Category::Dimension,
                kind: kind.into(),
                text: text.into(),
                centre: ((x0 + x1) * 0.5, y),
                edge_distance: 100.0,
                members: members
                    .iter()
                    .map(|m| ProposedMember {
                        role: m.role.clone(),
                        shape: m.proposed.clone(),
                        text: None,
                    })
                    .collect(),
                provenance: Provenance {
                    tile: "t".into(),
                    tile_sha256: String::new(),
                    level: "L1A".into(),
                    model: "m".into(),
                    prompt_sha256: String::new(),
                    file: "f".into(),
                    item_index: 0,
                    verification: None,
                },
                notes: vec![],
            },
            members,
        }
    }

    #[test]
    fn dimensions_calibrate_the_scale_and_the_note_is_the_fallback() {
        // 7.874 px per unit (1:1.5 at 300 dpi), extents measured to ±1 px, one wrong reading.
        let s = 300.0 / 25.4 / 1.5;
        let dims = vec![
            dimension("Ø35", "diameter", 100.0, 100.0 + 35.0 * s + 1.0, 50.0),
            dimension("72", "linear", 300.0, 300.0 + 72.0 * s - 1.0, 60.0),
            dimension("☆10 0/-0.036", "linear", 900.0, 900.0 + 10.0 * s, 70.0),
            dimension("R36", "radius", 0.0, 500.0, 80.0),
            dimension("211", "linear", 1000.0, 1000.0 + 211.0 * s + 0.5, 90.0),
            dimension("50", "linear", 1000.0, 1000.0 + 25.0 * s, 95.0),
        ];
        let c = calibrate(&dims, Some("1:1.5"), 300.0);
        assert_eq!(c.basis, "dimensions");
        assert_eq!(c.samples.len(), 5, "the radius is not a sample");
        assert!((c.px_per_unit / s - 1.0).abs() < 0.005, "{} vs {s}", c.px_per_unit);
        assert!(c.deviation_percent.unwrap().abs() < 0.5);
        assert!((c.scale_note_px_per_unit.unwrap() - s).abs() < 1e-9);
        let few = calibrate(&dims[..2], Some("1:1.5"), 300.0);
        assert_eq!(few.basis, "scale_note");
        assert!((few.px_per_unit - s).abs() < 1e-9);
        let none = calibrate(&dims[..1], None, 300.0);
        assert_eq!(none.basis, "paper_mm");
        assert!((none.px_per_unit - 300.0 / 25.4).abs() < 1e-9);
        let (x, y) = c.to_drawing((c.px_per_unit * 10.0, 100.0), 100.0 + c.px_per_unit * 20.0);
        assert!((x - 10.0).abs() < 1e-9 && (y - 20.0).abs() < 1e-9);
    }
}
