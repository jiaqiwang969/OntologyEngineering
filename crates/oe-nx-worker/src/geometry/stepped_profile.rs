//! Analytic, mm/+Z stepped-shaft profile. No serialization or native authority.
//! Axial-region agreement is only one necessary part of a complete native check.
use anyhow::{Result, ensure};

pub const LINEAR_TOLERANCE_MM: f64 = 1e-6;
const MAX_SEGMENTS: usize = 32;
const MAX_SIZE_MM: f64 = 100_000.0;
const MAX_COORDINATE_MM: f64 = 1_000_000.0;
const MIN_SIZE_MM: f64 = 1e-4;

/// Algorithm-local values; a caller must separately establish face ownership,
/// complete queries, axis/frame, topology and original source identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxialRegion {
    pub radius_mm: f64,
    pub z_start_mm: f64,
    pub z_end_mm: f64,
}

/// Validated immutable numerical profile, not a CAD result or engineering decision.
#[derive(Debug, Clone)]
pub struct SteppedProfile {
    regions: Vec<AxialRegion>,
    bounds_mm: [f64; 6],
    volume_mm3: f64,
    surface_area_mm2: f64,
}

impl SteppedProfile {
    /// Ordered (diameter_mm, length_mm), contiguous along absolute part +Z.
    /// Equal adjacent radii are rejected, never silently merged into another intent.
    pub fn new(origin_mm: [f64; 3], segments: &[(f64, f64)]) -> Result<Self> {
        ensure!(
            (2..=MAX_SEGMENTS).contains(&segments.len()),
            "stepped_segment_count"
        );
        ensure!(
            origin_mm
                .iter()
                .all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE_MM),
            "stepped_origin"
        );
        let mut regions: Vec<AxialRegion> = Vec::with_capacity(segments.len());
        let mut z = origin_mm[2];
        let mut length = 0.0;
        let mut max_radius: f64 = 0.0;
        let mut volume_coefficient = 0.0;
        let mut area_coefficient = 0.0;
        for &(diameter, segment_length) in segments {
            ensure!(
                [diameter, segment_length]
                    .iter()
                    .all(|v| v.is_finite() && (MIN_SIZE_MM..=MAX_SIZE_MM).contains(v)),
                "stepped_dimension"
            );
            let radius = diameter / 2.0;
            if let Some(previous) = regions.last() {
                ensure!(
                    (previous.radius_mm - radius).abs() > 2.0 * LINEAR_TOLERANCE_MM,
                    "stepped_indistinguishable_adjacent_radius"
                );
                area_coefficient += (previous.radius_mm.powi(2) - radius.powi(2)).abs();
            } else {
                area_coefficient += radius.powi(2);
            }
            length += segment_length;
            ensure!(length <= MAX_SIZE_MM, "stepped_total_length");
            let end = z + segment_length;
            ensure!(
                end.is_finite() && end > z && end.abs() <= MAX_COORDINATE_MM,
                "stepped_axial_extent"
            );
            regions.push(AxialRegion {
                radius_mm: radius,
                z_start_mm: z,
                z_end_mm: end,
            });
            z = end;
            max_radius = max_radius.max(radius);
            volume_coefficient += radius.powi(2) * segment_length;
            area_coefficient += 2.0 * radius * segment_length;
        }
        // At least two regions were checked above; do not infer internal disks.
        area_coefficient += regions
            .last()
            .expect("validated nonempty profile")
            .radius_mm
            .powi(2);
        let bounds_mm = [
            origin_mm[0] - max_radius,
            origin_mm[1] - max_radius,
            origin_mm[2],
            origin_mm[0] + max_radius,
            origin_mm[1] + max_radius,
            z,
        ];
        ensure!(
            bounds_mm
                .iter()
                .all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE_MM),
            "stepped_radial_extent"
        );
        Ok(Self {
            regions,
            bounds_mm,
            volume_mm3: std::f64::consts::PI * volume_coefficient,
            surface_area_mm2: std::f64::consts::PI * area_coefficient,
        })
    }

    pub fn regions(&self) -> &[AxialRegion] {
        &self.regions
    }
    pub fn bounds_mm(&self) -> [f64; 6] {
        self.bounds_mm
    }
    pub fn volume_mm3(&self) -> f64 {
        self.volume_mm3
    }
    pub fn surface_area_mm2(&self) -> f64 {
        self.surface_area_mm2
    }

    /// Unordered observations require a unique one-to-one radius/span match.
    /// This does not establish that the caller read every native face or edge.
    pub fn verify_axial_regions(&self, observed: &[AxialRegion]) -> Result<()> {
        ensure!(observed.len() == self.regions.len(), "stepped_region_count");
        let mut used = vec![false; self.regions.len()];
        for region in observed {
            ensure!(
                [region.radius_mm, region.z_start_mm, region.z_end_mm]
                    .iter()
                    .all(|v| v.is_finite())
                    && region.radius_mm > 0.0
                    && region.z_end_mm > region.z_start_mm,
                "stepped_invalid_observation"
            );
            let candidates: Vec<usize> = self
                .regions
                .iter()
                .enumerate()
                .filter_map(|(index, expected)| {
                    ((expected.radius_mm - region.radius_mm).abs() <= LINEAR_TOLERANCE_MM
                        && (expected.z_start_mm - region.z_start_mm).abs() <= LINEAR_TOLERANCE_MM
                        && (expected.z_end_mm - region.z_end_mm).abs() <= LINEAR_TOLERANCE_MM)
                        .then_some(index)
                })
                .collect();
            ensure!(
                candidates.len() == 1,
                "stepped_region_mismatch_or_ambiguity"
            );
            let index = candidates[0];
            ensure!(!used[index], "stepped_duplicate_region");
            used[index] = true;
        }
        ensure!(used.iter().all(|v| *v), "stepped_missing_region");
        Ok(())
    }
}
