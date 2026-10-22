//! Development adapter geometry projections. No native OCCT calls or acceptance.
//! This schema is distinct from the existing Python extractor's output. A supplied
//! source digest is a caller assertion until bound to an actual dispatch receipt.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;

pub const EVIDENCE_SCHEMA: &str = "ontology-engineering.geometry-evidence/v1";
pub const MAX_EVIDENCE_BYTES: usize = 32 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Surface types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CylinderFact {
    /// Point on the axis in local coordinates (mm)
    pub p: [f64; 3],
    /// Unit axis direction vector
    pub d: [f64; 3],
    /// Radius in mm
    pub r: f64,
    /// V (axial) parameter interval [v_min, v_max]
    pub v_interval: [f64; 2],
    /// U (circumferential) angular span in radians
    pub u_span: f64,
    /// Surface area in mm²
    pub area: f64,
}

#[derive(Debug, Clone)]
pub struct PlaneFact {
    /// Point on the plane in local coordinates (mm)
    pub p: [f64; 3],
    /// Unit normal vector
    pub n: [f64; 3],
    /// Reversed orientation flag
    pub reversed: bool,
    /// Surface area in mm²
    pub area: f64,
}

#[derive(Debug, Clone)]
pub struct ConeFact {
    /// Apex location approximation in local coordinates (mm)
    pub apex: [f64; 3],
    /// Unit axis direction
    pub d: [f64; 3],
    /// Half-angle in radians
    pub half_angle: f64,
    /// Reference radius at v=0 in mm
    pub ref_radius: f64,
    /// V interval
    pub v_interval: [f64; 2],
    /// Surface area in mm²
    pub area: f64,
}

// ---------------------------------------------------------------------------
// Part geometry record
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PartGeometry {
    /// Definition name (from XCAF label)
    pub definition_name: String,
    pub cylinders: Vec<CylinderFact>,
    pub planes: Vec<PlaneFact>,
    pub cones: Vec<ConeFact>,
    /// SHA-256 of the source STEP bytes that produced this record
    pub source_sha256: String,
}

impl PartGeometry {
    pub fn cylinder_count(&self) -> usize {
        self.cylinders.len()
    }
    pub fn plane_count(&self) -> usize {
        self.planes.len()
    }
    pub fn cone_count(&self) -> usize {
        self.cones.len()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "definition_name": self.definition_name,
            "source_sha256": self.source_sha256,
            "cylinder_count": self.cylinder_count(),
            "plane_count": self.plane_count(),
            "cone_count": self.cone_count(),
            "cylinders": self.cylinders.iter().map(|c| json!({
                "p": c.p, "d": c.d, "r": c.r,
                "v_interval": c.v_interval, "u_span": c.u_span, "area": c.area
            })).collect::<Vec<_>>(),
            "planes": self.planes.iter().map(|p| json!({
                "p": p.p, "n": p.n, "reversed": p.reversed, "area": p.area
            })).collect::<Vec<_>>(),
            "cones": self.cones.iter().map(|c| json!({
                "apex": c.apex, "d": c.d, "half_angle": c.half_angle,
                "ref_radius": c.ref_radius, "v_interval": c.v_interval, "area": c.area
            })).collect::<Vec<_>>(),
        })
    }
}

// ---------------------------------------------------------------------------
// Evidence package
// ---------------------------------------------------------------------------

pub struct EvidencePackage {
    pub source_sha256: String,
    pub parts: Vec<PartGeometry>,
    pub missing_definitions: Vec<String>,
    pub extra_definitions: Vec<String>,
    pub extraction_failures: std::collections::BTreeMap<String, String>,
    source_binding: &'static str,
}

impl EvidencePackage {
    pub fn to_json(&self) -> Value {
        json!({
            "schema": EVIDENCE_SCHEMA,
            "source_sha256": self.source_sha256,
            "part_count": self.parts.len(),
            "missing_definitions": self.missing_definitions,
            "extra_definitions": self.extra_definitions,
            "extraction_failures": self.extraction_failures,
            "source_binding": self.source_binding,
            "semantic_inference": false,
            "note": "source-bound geometry facts only; no engineering verdict",
            "parts": self.parts.iter().map(PartGeometry::to_json).collect::<Vec<_>>(),
        })
    }
}

// ---------------------------------------------------------------------------
// Load evidence from subprocess output JSON
// ---------------------------------------------------------------------------

pub fn load_evidence(output_path: &Path, step_sha256: &str) -> Result<EvidencePackage> {
    ensure!(
        step_sha256.len() == 64
            && step_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid_source_sha256"
    );
    let bytes = oe_core::read_regular_bytes(output_path, MAX_EVIDENCE_BYTES)
        .with_context(|| format!("cannot read evidence: {}", output_path.display()))?;
    ensure!(!bytes.is_empty(), "geometry evidence file is empty");

    let doc = oe_core::json::strict_json(&bytes)?;
    ensure!(
        doc.get("schema").and_then(Value::as_str) == Some(EVIDENCE_SCHEMA),
        "unsupported_geometry_adapter_schema; native_extractor_parity_not_accepted"
    );

    let parts_json = doc
        .get("parts")
        .and_then(Value::as_object)
        .context("evidence missing parts object")?;

    let mut parts = Vec::new();
    let mut extraction_failures = std::collections::BTreeMap::new();
    for (name, part_val) in parts_json {
        if let Some(error) = part_val.get("error") {
            let error = error
                .as_str()
                .filter(|s| !s.is_empty())
                .context("geometry_extraction_error_must_be_text")?;
            extraction_failures.insert(name.clone(), error.to_owned());
            continue;
        }
        let cylinders = parse_cylinders(required_array(part_val, "cylinders")?)?;
        let planes = parse_planes(required_array(part_val, "planes")?)?;
        let cones = parse_cones(required_array(part_val, "cones")?)?;
        parts.push(PartGeometry {
            definition_name: name.clone(),
            cylinders,
            planes,
            cones,
            source_sha256: step_sha256.to_owned(),
        });
    }

    let missing = definition_names(&doc, "missing_definitions")?;
    let extra = definition_names(&doc, "extra_definitions")?;

    Ok(EvidencePackage {
        source_sha256: step_sha256.to_owned(),
        parts,
        missing_definitions: missing,
        extra_definitions: extra,
        extraction_failures,
        source_binding: "caller_supplied_not_verified",
    })
}

/// Only an unchanged, completed dispatch can bind this projection's source.
pub fn load_verified_evidence(
    job: &crate::protocol::JobResult,
    relative: &Path,
) -> Result<EvidencePackage> {
    ensure!(
        crate::protocol::safe_relative(relative),
        "geometry_evidence_path_must_be_relative"
    );
    ensure!(
        relative
            .components()
            .next()
            .is_some_and(|c| c.as_os_str() != "_oe"),
        "geometry_evidence_path_is_metadata"
    );
    ensure!(
        crate::acceptance::verify_job_result(job)?.passed(),
        "geometry_job_readback_not_accepted"
    );
    let mut package = load_evidence(&job.output_dir.join(relative), &job.input_sha256)?;
    ensure!(
        crate::acceptance::verify_job_result(job)?.passed(),
        "geometry_job_changed_during_projection"
    );
    package.source_binding = "exact_dispatch_readback";
    Ok(package)
}

fn required_array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .with_context(|| format!("{key}_must_be_array"))
}
fn definition_names(value: &Value, key: &str) -> Result<Vec<String>> {
    required_array(value, key)?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .with_context(|| format!("{key}_must_contain_nonempty_names"))
        })
        .collect()
}
fn finite(value: &Value, key: &str) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .with_context(|| format!("{key}_must_be_finite_number"))
}

fn parse_vec3(v: &Value, key: &str) -> Result<[f64; 3]> {
    let arr = v
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("missing vec3 field: {key}"))?;
    ensure!(arr.len() == 3, "vec3 must have 3 elements: {key}");
    Ok([
        finite(&arr[0], key)?,
        finite(&arr[1], key)?,
        finite(&arr[2], key)?,
    ])
}

fn parse_interval(v: &Value, key: &str) -> Result<[f64; 2]> {
    let arr = v
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("missing interval field: {key}"))?;
    ensure!(arr.len() == 2, "interval must have 2 elements: {key}");
    Ok([finite(&arr[0], key)?, finite(&arr[1], key)?])
}

fn parse_cylinders(arr: &[Value]) -> Result<Vec<CylinderFact>> {
    let mut result = Vec::new();
    for item in arr {
        result.push(CylinderFact {
            p: parse_vec3(item, "p")?,
            d: parse_vec3(item, "d")?,
            r: finite(&item["r"], "cylinder.r")?,
            v_interval: parse_interval(item, "v_interval")?,
            u_span: finite(&item["u_span"], "cylinder.u_span")?,
            area: finite(&item["area"], "cylinder.area")?,
        });
    }
    Ok(result)
}

fn parse_planes(arr: &[Value]) -> Result<Vec<PlaneFact>> {
    let mut result = Vec::new();
    for item in arr {
        result.push(PlaneFact {
            p: parse_vec3(item, "p")?,
            n: parse_vec3(item, "n")?,
            reversed: item
                .get("reversed")
                .and_then(Value::as_bool)
                .context("plane.reversed_must_be_boolean")?,
            area: finite(&item["area"], "plane.area")?,
        });
    }
    Ok(result)
}

fn parse_cones(arr: &[Value]) -> Result<Vec<ConeFact>> {
    let mut result = Vec::new();
    for item in arr {
        result.push(ConeFact {
            apex: parse_vec3(item, "apex")?,
            d: parse_vec3(item, "d")?,
            half_angle: finite(&item["half_angle"], "cone.half_angle")?,
            ref_radius: finite(&item["ref_radius"], "cone.ref_radius")?,
            v_interval: parse_interval(item, "v_interval")?,
            area: finite(&item["area"], "cone.area")?,
        });
    }
    Ok(result)
}
