//! Bounded geometric selection in one hash-bound part, never persistent tags.
use anyhow::{Result, ensure};
use oe_contracts::{CircularEdgeIntent, CircularEdgeIntentV2, DrawingReferencePoint};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn vector(value: &Value) -> Option<[f64; 3]> {
    let v = value.as_array()?;
    if v.len() != 3 {
        return None;
    }
    let p = [v[0].as_f64()?, v[1].as_f64()?, v[2].as_f64()?];
    p.iter().all(|n| n.is_finite()).then_some(p)
}
fn norm(v: [f64; 3]) -> f64 {
    v[0].hypot(v[1]).hypot(v[2])
}
/// Protocol-specific selectors share one inventory algorithm. Legacy /2 keeps
/// its common linear tolerance; /3 never substitutes one tolerance for another.
pub enum CircleIntentRef<'a> {
    Legacy(&'a CircularEdgeIntent),
    Separate(&'a CircularEdgeIntentV2),
}
impl<'a> CircleIntentRef<'a> {
    pub fn for_point(schema: &str, point: &'a DrawingReferencePoint) -> Result<Option<Self>> {
        match schema {
            "oev2.drawing-reference-intent/1" => {
                ensure!(
                    point.parent_edge.is_none() && point.parent_edge_v2.is_none(),
                    "drawing_reference_v1_extension_forbidden"
                );
                Ok(None)
            }
            "oev2.drawing-reference-intent/2" => {
                ensure!(
                    point.parent_edge_v2.is_none(),
                    "drawing_reference_v2_extension_forbidden"
                );
                Ok(point.parent_edge.as_ref().map(Self::Legacy))
            }
            "oev2.drawing-reference-intent/3" => {
                ensure!(
                    point.parent_edge.is_none(),
                    "drawing_reference_v3_legacy_selector_forbidden"
                );
                Ok(point.parent_edge_v2.as_ref().map(Self::Separate))
            }
            _ => anyhow::bail!("drawing_reference_schema_unsupported"),
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Legacy(e) => validate(e),
            Self::Separate(e) => validate_v2(e),
        }
    }
    pub fn select(&self, part: &Value) -> Result<Value> {
        match self {
            Self::Legacy(e) => select(e, part),
            Self::Separate(e) => select_v2(e, part),
        }
    }
    pub fn feature_id(&self) -> &str {
        match self {
            Self::Legacy(e) => &e.feature_id,
            Self::Separate(e) => &e.feature_id,
        }
    }
    pub fn center(&self) -> &[f64] {
        match self {
            Self::Legacy(e) => &e.center,
            Self::Separate(e) => &e.center,
        }
    }
    pub fn normal(&self) -> &[f64] {
        match self {
            Self::Legacy(e) => &e.normal,
            Self::Separate(e) => &e.normal,
        }
    }
    pub fn unit(&self) -> &str {
        match self {
            Self::Legacy(e) => &e.unit,
            Self::Separate(e) => &e.unit,
        }
    }
    pub fn local_frame(&self) -> &str {
        match self {
            Self::Legacy(e) => &e.local_frame,
            Self::Separate(e) => &e.local_frame,
        }
    }
    pub fn full_circle(&self) -> bool {
        match self {
            Self::Legacy(e) => e.full_circle,
            Self::Separate(e) => e.full_circle,
        }
    }
    pub fn angular_tolerance(&self) -> f64 {
        match self {
            Self::Legacy(e) => e.angular_tolerance_radians,
            Self::Separate(e) => e.angular_tolerance_radians,
        }
    }
}
/// Preserve the canonical /1 and /2 wire contracts after adding /3's field.
pub fn intent_value(intent: &oe_contracts::DrawingReferenceIntent) -> Result<Value> {
    let mut value = serde_json::to_value(intent)?;
    for (p, raw) in intent
        .points
        .iter()
        .zip(value["points"].as_array_mut().unwrap())
    {
        CircleIntentRef::for_point(&intent.schema, p)?;
        if intent.schema != "oev2.drawing-reference-intent/3" {
            raw.as_object_mut().unwrap().remove("parent_edge_v2");
        }
        if intent.schema == "oev2.drawing-reference-intent/1" {
            raw.as_object_mut().unwrap().remove("parent_edge");
        }
        if intent.schema == "oev2.drawing-reference-intent/3" {
            raw.as_object_mut().unwrap().remove("parent_edge");
        }
    }
    if intent.schema == "oev2.drawing-reference-intent/1" {
        value.as_object_mut().unwrap().remove("dimension_value");
    }
    Ok(value)
}
// Internal compatibility projection only. Radius tolerance is passed
// independently to the algorithm; this is never emitted as a legacy receipt.
fn legacy_shape(intent: &CircularEdgeIntentV2) -> CircularEdgeIntent {
    CircularEdgeIntent {
        feature_id: intent.feature_id.clone(),
        center: intent.center.clone(),
        normal: intent.normal.clone(),
        radius: intent.radius,
        linear_tolerance: intent.center_tolerance,
        angular_tolerance_radians: intent.angular_tolerance_radians,
        unit: intent.unit.clone(),
        local_frame: intent.local_frame.clone(),
        full_circle: intent.full_circle,
    }
}
pub fn validate_v2(intent: &CircularEdgeIntentV2) -> Result<()> {
    intent
        .validate()
        .map_err(|_| anyhow::anyhow!("model_circle_v2_intent_invalid"))?;
    validate(&legacy_shape(intent))?;
    ensure!(
        intent.radius_tolerance.is_finite()
            && intent.radius_tolerance >= 0.0
            && intent.radius_tolerance <= 1e6,
        "model_circle_radius_precision_invalid"
    );
    Ok(())
}
pub fn select_v2(intent: &CircularEdgeIntentV2, part: &Value) -> Result<Value> {
    validate_v2(intent)?;
    let mut output = select_with_tolerances(&legacy_shape(intent), intent.radius_tolerance, part)?;
    output["schema"] = json!("oev2.model-circle-edge-check/2");
    output["tolerance_policy"] = json!({"schema":"oev2.circle-tolerance-policy/1","center_tolerance":intent.center_tolerance,
        "radius_tolerance":intent.radius_tolerance,"angular_tolerance_radians":intent.angular_tolerance_radians,"unit":intent.unit,
        "scope":"independent geometric matching; not manufacturing tolerances or measurement uncertainty"});
    Ok(output)
}
pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn validate(intent: &CircularEdgeIntent) -> Result<()> {
    intent
        .validate()
        .map_err(|_| anyhow::anyhow!("model_circle_intent_invalid"))?;
    ensure!(
        !intent.feature_id.trim().is_empty(),
        "model_circle_identity_missing"
    );
    ensure!(
        intent.center.len() == 3 && intent.center.iter().all(|n| n.is_finite()),
        "model_circle_center_invalid"
    );
    ensure!(
        intent.normal.len() == 3
            && intent.normal.iter().all(|n| n.is_finite())
            && (norm([intent.normal[0], intent.normal[1], intent.normal[2]]) - 1.0).abs() <= 1e-9,
        "model_circle_normal_invalid"
    );
    ensure!(
        intent.radius > 0.0
            && intent.radius <= 1e6
            && intent.linear_tolerance >= 0.0
            && intent.linear_tolerance <= 1e6
            && intent.angular_tolerance_radians >= 0.0
            && intent.angular_tolerance_radians < std::f64::consts::FRAC_PI_2,
        "model_circle_precision_invalid"
    );
    Ok(())
}
fn answer(
    intent: &CircularEdgeIntent,
    conclusion: &str,
    reason: Option<&str>,
    candidates: Vec<u32>,
    details: Value,
) -> Value {
    json!({"schema":"oev2.model-circle-edge-check/1","feature_id":intent.feature_id,
        "conclusion":conclusion,"reason":reason,"candidate_edge_tags":candidates,"details":details,
        "identity_scope":"unique geometric edge in this exact part observation; not a persistent feature identity across versions",
        "engineering_adoption":"not_established"})
}
fn metadata_ok(v: &Value, subtype: i64) -> bool {
    v["type_query"]["return_code"] == 0
        && v["type_query"]["type_raw"] == 70
        && v["type_query"]["subtype_raw"] == subtype
        && v["owning_part_query"]["value_valid"] == true
        && v["owning_part_query"]["matches_expected_owner"] == true
}
fn tag(v: &Value) -> Option<u32> {
    u32::try_from(v.as_u64()?).ok().filter(|n| *n != 0)
}

pub fn select(intent: &CircularEdgeIntent, part: &Value) -> Result<Value> {
    validate(intent)?;
    select_with_tolerances(intent, intent.linear_tolerance, part)
}
fn select_with_tolerances(
    intent: &CircularEdgeIntent,
    radius_tolerance: f64,
    part: &Value,
) -> Result<Value> {
    if !intent.full_circle {
        return Ok(answer(
            intent,
            "unknown",
            Some("model_circle_trimmed_request_unsupported"),
            vec![],
            json!({}),
        ));
    }
    let scope = &part["model_edges"];
    if scope["schema"] != "oev2.nx-model-edge-observation/1"
        || scope["complete"] != true
        || scope["coordinate_basis"] != "UF_EVAL ABS coordinates; single-part solid edges"
        || scope["solid_type_raw"] != 70
        || scope["edge_subtype_raw"] != 3
    {
        return Ok(answer(
            intent,
            "unknown",
            Some("model_circle_scope_incomplete"),
            vec![],
            json!({}),
        ));
    }
    if scope["unit"] != intent.unit || part["units"] != intent.unit {
        return Ok(answer(
            intent,
            "unknown",
            Some("model_circle_unit_mismatch"),
            vec![],
            json!({"required_unit":intent.unit,"native_unit":scope["unit"]}),
        ));
    }
    let (Some(bodies), Some(entries), Some(solid_tags), Some(solid_count)) = (
        scope["bodies"].as_array(),
        scope["entries"].as_array(),
        part["solid_tags"].as_array(),
        part["solid_count"].as_u64(),
    ) else {
        return Ok(answer(
            intent,
            "unknown",
            Some("model_circle_inventory_missing"),
            vec![],
            json!({}),
        ));
    };
    ensure!(
        bodies.len() <= 4096 && entries.len() <= 65536,
        "model_circle_inventory_budget"
    );
    let mut membership = BTreeMap::new();
    let mut body_ids = BTreeSet::new();
    let mut inventory_valid =
        bodies.len() as u64 == solid_count && solid_tags.len() == bodies.len();
    for b in bodies {
        let Some(body_id) = tag(&b["body_tag"]) else {
            inventory_valid = false;
            continue;
        };
        inventory_valid &= body_ids.insert(body_id)
            && metadata_ok(&b["metadata"], 0)
            && b["edge_list_return_code"] == 0
            && b["capture_complete"] == true;
        let Some(edge_tags) = b["edge_tags"].as_array() else {
            inventory_valid = false;
            continue;
        };
        ensure!(edge_tags.len() <= 65536, "model_circle_inventory_budget");
        for t in edge_tags {
            let Some(edge_id) = tag(t) else {
                inventory_valid = false;
                continue;
            };
            inventory_valid &= membership.insert(edge_id, body_id).is_none();
        }
    }
    let declared_body_ids = solid_tags.iter().filter_map(tag).collect::<BTreeSet<_>>();
    inventory_valid &= declared_body_ids.len() == solid_tags.len()
        && declared_body_ids == body_ids
        && membership.len() == entries.len();
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    let mut incomplete = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let q = &e["query"];
        let ids = (tag(&e["edge_tag"]), tag(&e["body_tag"]));
        let complete = ids.0.is_some()
            && ids.1.is_some()
            && seen.insert(ids.0.unwrap())
            && membership.get(&ids.0.unwrap()) == ids.1.as_ref()
            && metadata_ok(&e["metadata"], 3)
            && q["capture_complete"] == true
            && q["wrapper_code"] == 0
            && q["initialize_code"] == 0
            && q["is_arc_code"] == 0
            && q["free_code"] == 0
            && q["is_arc"].as_bool().is_some();
        if !complete {
            incomplete.push(i);
            continue;
        }
        if q["is_arc"] == false {
            continue;
        }
        let a = &e["arc"];
        let vectors = (
            vector(&a["center"]),
            vector(&a["x_axis"]),
            vector(&a["y_axis"]),
        );
        let limits = a["limits"]
            .as_array()
            .filter(|l| l.len() == 2)
            .and_then(|l| Some([l[0].as_f64()?, l[1].as_f64()?]));
        let radius = a["radius"].as_f64();
        let valid = q["arc_code"] == 0
            && a["value_valid"] == true
            && a["is_periodic"].as_bool().is_some()
            && radius.is_some_and(|r| r.is_finite() && r > 0.0)
            && limits.is_some_and(|l| l.iter().all(|n| n.is_finite()) && l[1] > l[0])
            && vectors.0.is_some()
            && vectors.1.is_some()
            && vectors.2.is_some();
        if !valid {
            incomplete.push(i);
            continue;
        }
        let (center, x, y) = (vectors.0.unwrap(), vectors.1.unwrap(), vectors.2.unwrap());
        let normal = cross(x, y);
        if (norm(x) - 1.0).abs() > 1e-9
            || (norm(y) - 1.0).abs() > 1e-9
            || (norm(normal) - 1.0).abs() > 1e-9
        {
            incomplete.push(i);
            continue;
        }
        let span = limits.unwrap()[1] - limits.unwrap()[0];
        if a["is_periodic"] != true || (span - std::f64::consts::TAU).abs() > 1e-8 {
            continue;
        }
        let distance = norm([
            center[0] - intent.center[0],
            center[1] - intent.center[1],
            center[2] - intent.center[2],
        ]);
        let dot = (0..3)
            .map(|i| normal[i] * intent.normal[i])
            .sum::<f64>()
            .abs()
            .min(1.0);
        if distance <= intent.linear_tolerance
            && (radius.unwrap() - intent.radius).abs() <= radius_tolerance
            && dot.acos() <= intent.angular_tolerance_radians
        {
            candidates.push(ids.0.unwrap());
        }
    }
    let details = json!({"body_count":bodies.len(),"edge_count":entries.len(),"inventory_valid":inventory_valid,
        "incomplete_entry_indices":incomplete,"unit":intent.unit,"local_frame":intent.local_frame});
    let (conclusion, reason) = if !inventory_valid || !incomplete.is_empty() {
        ("unknown", Some("model_circle_scope_incomplete"))
    } else if candidates.is_empty() {
        ("violated", Some("model_circle_no_edge_matches"))
    } else if candidates.len() != 1 {
        ("unknown", Some("model_circle_ambiguous"))
    } else {
        ("pass", None)
    };
    Ok(answer(intent, conclusion, reason, candidates, details))
}
