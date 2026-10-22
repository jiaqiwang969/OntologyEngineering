//! Parametric design intent and dependent drawing/mating obligations.
//! Exact decimal layout reuses the historical method without Python execution.
use anyhow::{Context, Result, ensure};
use oe_contracts::CenteredHoleRow;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MAX_SCALE: u32 = 18;
const MAX_CENTERS: i128 = 100_000;
/// Bounded finite base-10 input. i128 guards are explicit; a float is never used
/// to choose a hole count at a transition (e.g. 723.999999999 versus 724).
#[derive(Clone, Copy)]
struct Decimal {
    coefficient: i128,
    scale: u32,
}
impl Decimal {
    fn parse(s: &str) -> Result<Self> {
        ensure!(
            !s.is_empty() && s.len() <= 64 && s.trim() == s,
            "hole_decimal_invalid"
        );
        let (negative, rest) = if let Some(x) = s.strip_prefix('-') {
            (true, x)
        } else {
            (false, s.strip_prefix('+').unwrap_or(s))
        };
        let parts: Vec<_> = rest.split(['e', 'E']).collect();
        ensure!(parts.len() <= 2, "hole_decimal_invalid");
        let exponent: i32 = if parts.len() == 2 {
            parts[1].parse().context("hole_decimal_exponent_invalid")?
        } else {
            0
        };
        ensure!(exponent.unsigned_abs() <= 18, "hole_decimal_exponent_bound");
        let mantissa: Vec<_> = parts[0].split('.').collect();
        ensure!(mantissa.len() <= 2, "hole_decimal_invalid");
        let digits = mantissa.concat();
        ensure!(
            !digits.is_empty() && digits.bytes().all(|x| x.is_ascii_digit()),
            "hole_decimal_invalid"
        );
        let fractional = if mantissa.len() == 2 {
            mantissa[1].len() as i32
        } else {
            0
        };
        let mut coefficient: i128 = digits.parse().context("hole_decimal_magnitude_bound")?;
        let scale = fractional - exponent;
        let scale = if scale < 0 {
            coefficient = coefficient
                .checked_mul(
                    10_i128
                        .checked_pow((-scale) as u32)
                        .context("hole_decimal_magnitude_bound")?,
                )
                .context("hole_decimal_magnitude_bound")?;
            0
        } else {
            scale as u32
        };
        ensure!(scale <= MAX_SCALE, "hole_decimal_precision_bound");
        if negative {
            coefficient = -coefficient;
        }
        Ok(Self { coefficient, scale })
    }
    fn scaled(self, scale: u32) -> Result<i128> {
        self.coefficient
            .checked_mul(10_i128.pow(scale - self.scale))
            .context("hole_decimal_magnitude_bound")
    }
}
fn exact(n: i128, scale: u32) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let digits = n.unsigned_abs().to_string();
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let padded = format!("{:0>width$}", digits, width = scale as usize + 1);
    let at = padded.len() - scale as usize;
    let mut value = format!("{sign}{}.{}", &padded[..at], &padded[at..]);
    while value.ends_with('0') {
        value.pop();
    }
    if value.ends_with('.') {
        value.pop();
    }
    value
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HoleLayout {
    pub row_id: String,
    pub unit: String,
    pub local_frame: String,
    pub count: usize,
    pub centers_exact: Vec<String>,
    pub centers: Vec<f64>,
    pub pitch: String,
    pub center: String,
    pub end_margin_exact: String,
    pub end_margin: f64,
    pub diameter: String,
    pub input_sha256: String,
    pub scope: String,
}
fn layout(row: &CenteredHoleRow, imposed: Option<usize>) -> Result<HoleLayout> {
    row.validate()
        .map_err(|e| anyhow::anyhow!("hole_row_invalid:{}", e.join(",")))?;
    ensure!(
        !row.source_refs.is_empty() && row.source_refs.iter().all(|s| !s.trim().is_empty()),
        "hole_row_source_missing"
    );
    let decimals = [
        &row.length,
        &row.pitch,
        &row.minimum_center_to_end,
        &row.center,
        &row.diameter,
    ]
    .map(|s| Decimal::parse(s))
    .into_iter()
    .collect::<Result<Vec<_>>>()?;
    let scale = decimals
        .iter()
        .map(|d| d.scale)
        .max()
        .context("hole_parameters_missing")?;
    let values = decimals
        .into_iter()
        .map(|d| d.scaled(scale))
        .collect::<Result<Vec<_>>>()?;
    let (length, pitch, margin, center, diameter) =
        (values[0], values[1], values[2], values[3], values[4]);
    ensure!(
        length > 0 && pitch > 0 && margin >= 0 && diameter > 0,
        "hole_row_nonpositive_parameter"
    );
    let limit = 1_000_000_i128 * 10_i128.pow(scale);
    ensure!(
        values.iter().all(|v| v.abs() <= limit),
        "hole_row_dimension_bound"
    );
    let usable = length
        .checked_sub(
            margin
                .checked_mul(2)
                .context("hole_decimal_magnitude_bound")?,
        )
        .context("hole_decimal_magnitude_bound")?;
    ensure!(usable >= 0, "hole_row_no_center_fits");
    let maximum = usable / pitch + 1;
    let count = imposed.map(|n| n as i128).unwrap_or(maximum);
    ensure!(count <= MAX_CENTERS, "hole_row_center_budget");
    ensure!(
        count >= 1 && count <= maximum,
        "hole_mating_count_does_not_fit"
    );
    let span = (count - 1)
        .checked_mul(pitch)
        .context("hole_decimal_magnitude_bound")?;
    // Double the numerator, then multiply by five: division by two is exact
    // with one additional decimal place, including half-pitch even rows.
    let centers_exact = (0..count)
        .map(|i| {
            let numerator = center
                .checked_mul(2)
                .and_then(|x| x.checked_add((2 * i - count + 1) * pitch))
                .context("hole_decimal_magnitude_bound")?;
            Ok(exact(
                numerator
                    .checked_mul(5)
                    .context("hole_decimal_magnitude_bound")?,
                scale + 1,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let centers = centers_exact
        .iter()
        .map(|s| s.parse::<f64>().context("hole_native_projection_invalid"))
        .collect::<Result<Vec<_>>>()?;
    let end_margin_exact = exact(
        (length - span)
            .checked_mul(5)
            .context("hole_decimal_magnitude_bound")?,
        scale + 1,
    );
    Ok(HoleLayout{row_id:row.row_id.clone(),unit:row.unit.clone(),local_frame:row.local_frame.clone(),count:count as usize,
        centers_exact,centers,pitch:row.pitch.clone(),center:row.center.clone(),end_margin:end_margin_exact.parse()?,end_margin_exact,
        diameter:row.diameter.clone(),input_sha256:oe_core::json::canonical_digest(&serde_json::to_value(row)?)?,
        scope:"fixed-pitch centered layout only; native shape, edge ligament, strength, mating, drawing and release require separate checks".into()})
}
pub fn plan(row: &CenteredHoleRow) -> Result<HoleLayout> {
    layout(row, None)
}
/// Compare design content, not just object/version hashes. Equivalent decimal
/// spellings have the same meaning; identities, units and source sets do not.
pub fn design_differences(
    expected: &CenteredHoleRow,
    current: &CenteredHoleRow,
) -> Result<Vec<String>> {
    fn normalized(row: &CenteredHoleRow) -> Result<Value> {
        plan(row)?;
        let mut value = serde_json::to_value(row)?;
        for field in [
            "length",
            "pitch",
            "minimum_center_to_end",
            "center",
            "diameter",
        ] {
            let decimal = Decimal::parse(value[field].as_str().context("hole_decimal_invalid")?)?;
            value[field] = json!(exact(decimal.coefficient, decimal.scale));
        }
        let refs: BTreeSet<_> = row.source_refs.iter().collect();
        ensure!(
            refs.len() == row.source_refs.len(),
            "hole_row_duplicate_source"
        );
        value["source_refs"] = serde_json::to_value(refs)?;
        Ok(value)
    }
    let a = normalized(expected)?;
    let b = normalized(current)?;
    Ok(a.as_object()
        .context("hole_row_invalid")?
        .keys()
        .filter(|key| a[*key] != b[*key])
        .cloned()
        .collect())
}
fn follower(
    controller: &CenteredHoleRow,
    primary: &HoleLayout,
    member: &CenteredHoleRow,
) -> Result<HoleLayout> {
    ensure!(
        member.row_id != controller.row_id,
        "hole_mating_duplicate_row"
    );
    ensure!(
        Decimal::parse(&member.pitch)?.scaled(MAX_SCALE)?
            == Decimal::parse(&controller.pitch)?.scaled(MAX_SCALE)?,
        "hole_mating_pitch_mismatch"
    );
    layout(member, Some(primary.count))
}
/// Native verification and group planning share the same count authority.
pub fn plan_follower(controller: &CenteredHoleRow, member: &CenteredHoleRow) -> Result<HoleLayout> {
    follower(controller, &plan(controller)?, member)
}
/// The controller's design count is authoritative within the declared mating
/// group. A longer follower must not independently add unmatched holes.
pub fn plan_mating(controller: &CenteredHoleRow, followers: &[CenteredHoleRow]) -> Result<Value> {
    ensure!(followers.len() <= 64, "hole_mating_budget");
    let primary = plan(controller)?;
    let mut rows = Vec::new();
    let mut identities = BTreeSet::from([controller.row_id.clone()]);
    for member in followers {
        ensure!(
            identities.insert(member.row_id.clone()),
            "hole_mating_duplicate_row"
        );
        let row = follower(controller, &primary, member)?;
        rows.push(json!({"object":member.object,"layout":row,"count_authority":controller.row_id,
            "assembly_axis_agreement":"requires native transformed axes; equal local x values alone are insufficient"}));
    }
    Ok(
        json!({"controller":{"object":controller.object,"layout":primary},"followers":rows,"engineering_adoption":"not_established"}),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeHole {
    pub center: [f64; 3],
    pub axis: [f64; 3],
    pub diameter: f64,
    pub native_face_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigidPlacement {
    pub rotation: [[f64; 3]; 3],
    pub translation: [f64; 3],
}
impl RigidPlacement {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.rotation
                .iter()
                .flatten()
                .chain(self.translation.iter())
                .all(|v| v.is_finite()),
            "hole_transform_nonfinite"
        );
        for i in 0..3 {
            for j in 0..3 {
                let dot: f64 = (0..3)
                    .map(|k| self.rotation[k][i] * self.rotation[k][j])
                    .sum();
                ensure!(
                    (dot - if i == j { 1.0 } else { 0.0 }).abs() <= 1e-10,
                    "hole_transform_not_rigid"
                );
            }
        }
        let r = &self.rotation;
        let determinant = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
            - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
            + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
        ensure!(
            (determinant - 1.0).abs() <= 1e-10,
            "hole_transform_improper_rotation"
        );
        Ok(())
    }
    pub fn point(&self, v: [f64; 3]) -> [f64; 3] {
        let a = self.vector(v);
        std::array::from_fn(|i| a[i] + self.translation[i])
    }
    pub fn vector(&self, v: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| (0..3).map(|j| self.rotation[i][j] * v[j]).sum())
    }
}
/// Native axes are lines: compare perpendicular distance, not arbitrary axial
/// parameter origins on cylindrical faces. Radii and count are separate checks.
pub fn check_native(
    layout: &HoleLayout,
    holes: &[NativeHole],
    origin_yz: [f64; 2],
    placement: &RigidPlacement,
    tolerance_mm: f64,
) -> Result<Value> {
    placement.validate()?;
    ensure!(
        tolerance_mm.is_finite() && tolerance_mm > 0.0 && origin_yz.iter().all(|x| x.is_finite()),
        "hole_native_tolerance_invalid"
    );
    ensure!(
        layout.count == layout.centers.len() && holes.len() == layout.count,
        "hole_native_count_mismatch"
    );
    let diameter = layout.diameter.parse::<f64>()?;
    let expected_axis = placement.vector([0.0, 0.0, 1.0]);
    let mut unused: BTreeSet<usize> = (0..holes.len()).collect();
    let mut mappings = Vec::new();
    for x in &layout.centers {
        let center = placement.point([*x, origin_yz[0], origin_yz[1]]);
        let mut matched = Vec::new();
        for i in &unused {
            let h = &holes[*i];
            ensure!(
                h.center
                    .iter()
                    .chain(h.axis.iter())
                    .chain(std::iter::once(&h.diameter))
                    .all(|v| v.is_finite()),
                "hole_native_nonfinite"
            );
            let norm = h.axis.iter().map(|v| v * v).sum::<f64>().sqrt();
            ensure!(norm > 0.0, "hole_native_axis_invalid");
            let axis = h.axis.map(|v| v / norm);
            let delta: [f64; 3] = std::array::from_fn(|j| h.center[j] - center[j]);
            let along: f64 = delta.iter().zip(expected_axis).map(|(a, b)| a * b).sum();
            let distance = (0..3)
                .map(|j| (delta[j] - along * expected_axis[j]).powi(2))
                .sum::<f64>()
                .sqrt();
            let dot: f64 = axis.iter().zip(expected_axis).map(|(a, b)| a * b).sum();
            if distance <= tolerance_mm
                && (dot.abs() - 1.0).abs() <= 1e-8
                && (h.diameter - diameter).abs() <= tolerance_mm
            {
                matched.push((*i, distance, dot));
            }
        }
        ensure!(
            matched.len() == 1,
            "hole_native_axis_or_diameter_missing_or_ambiguous"
        );
        let (i, distance, dot) = matched[0];
        unused.remove(&i);
        mappings.push(json!({"x":x,"native_face_id":holes[i].native_face_id,"axis_distance_mm":distance,"axis_dot":dot,"diameter_mm":holes[i].diameter}));
    }
    Ok(
        json!({"native_geometry":"pass","mappings":mappings,"layout_sha256":layout.input_sha256,"tolerance_mm":tolerance_mm,"engineering_adoption":"not_established",
        "scope":"caller-bound native periodic face group in a validated rigid assembly transform; no strength or manufacturing approval"}),
    )
}

/// Changes use the existing Project dependency graph at the CLI. These stable
/// seeds name the obligations; they never create a second lifecycle ledger.
pub fn dependency_seeds(row: &CenteredHoleRow) -> Vec<String> {
    [
        "length",
        "hole_count",
        "hole_phase",
        "symmetry",
        "mating",
        "dimensions",
        "views",
        "layout",
        "drawings",
        "bom",
        "inspection",
        "manifest",
    ]
    .into_iter()
    .map(|role| format!("{}:{role}", row.row_id))
    .collect()
}
