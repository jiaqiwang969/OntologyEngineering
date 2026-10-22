//! Independently declared reference points, not reverse-engineered expectations.
//! /2 adds unique full-circle solid edges in one exact part; no manufacture.
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    DrawingReferenceIntent, DrawingReferencePoint, NativeDrawingObservation, ObjectIdentity,
};
use oe_core::{json::canonical_digest, read_regular_bytes};
use oe_nx_worker::geometry::model_edges::CircleIntentRef;
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};

pub fn validate(intent: &DrawingReferenceIntent) -> Result<()> {
    intent
        .validate()
        .map_err(|_| anyhow::anyhow!("drawing_reference_intent_invalid"))?;
    ensure!(
        intent.position_tolerance >= 0.0,
        "drawing_reference_tolerance_invalid"
    );
    ensure!(
        (1..=8).contains(&intent.points.len()),
        "drawing_reference_point_budget"
    );
    ensure!(
        !intent.source_refs.is_empty()
            && intent.source_refs.iter().all(|s| !s.trim().is_empty())
            && intent.source_refs.iter().collect::<BTreeSet<_>>().len() == intent.source_refs.len(),
        "drawing_reference_sources_invalid"
    );
    let mut ids = BTreeSet::new();
    let mut features = BTreeSet::new();
    for p in &intent.points {
        p.validate()
            .map_err(|_| anyhow::anyhow!("drawing_reference_point_invalid"))?;
        ensure!(
            ids.insert(&p.point_id),
            "drawing_reference_point_id_duplicate"
        );
        ensure!(
            p.position.len() == 3 && p.position.iter().all(|x| x.is_finite()),
            "drawing_reference_position_invalid"
        );
        ensure!(
            [
                p.point_type_raw,
                p.line_type_raw,
                p.parent_type_raw,
                p.parent_subtype_raw
            ]
            .iter()
            .all(|n| i32::try_from(*n).is_ok()),
            "drawing_reference_raw_code_invalid"
        );
        if let Some(edge) = CircleIntentRef::for_point(&intent.schema, p)? {
            edge.validate()?;
            ensure!(
                features.insert(edge.feature_id().to_owned())
                    && edge.unit() == intent.unit
                    && edge.local_frame() == intent.local_frame,
                "drawing_reference_model_edge_identity_or_unit_invalid"
            );
            let c = edge.center();
            let d = (p.position[0] - c[0])
                .hypot(p.position[1] - c[1])
                .hypot(p.position[2] - c[2]);
            ensure!(
                p.point_type_raw == 2
                    && p.line_type_raw == 0
                    && p.parent_type_raw == 70
                    && p.parent_subtype_raw == 3
                    && d <= intent.position_tolerance,
                "drawing_reference_model_edge_center_or_mode_invalid"
            );
        }
    }
    if let Some(value) = &intent.dimension_value {
        ensure!(
            intent.schema != "oev2.drawing-reference-intent/1",
            "drawing_reference_v1_extension_forbidden"
        );
        value
            .validate()
            .map_err(|_| anyhow::anyhow!("drawing_reference_dimension_value_invalid"))?;
        ensure!(
            value.value >= 0.0
                && value.numeric_tolerance >= 0.0
                && value.unit == intent.unit
                && value.dimension_subtype_raw == 3
                && intent.points.len() == 2
                && intent
                    .points
                    .iter()
                    .all(|p| CircleIntentRef::for_point(&intent.schema, p)
                        .ok()
                        .flatten()
                        .is_some()),
            "drawing_reference_dimension_value_scope_unsupported"
        );
        let a = &intent.points[0].position;
        let b = &intent.points[1].position;
        let distance = (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2]);
        ensure!(
            (distance - value.value).abs() <= value.numeric_tolerance,
            "drawing_reference_independent_value_mismatch"
        );
    }
    Ok(())
}
/// Keep /1's canonical wire meaning despite optional /2 fields in the shared DTO.
pub fn intent_value(intent: &DrawingReferenceIntent) -> Result<Value> {
    oe_nx_worker::geometry::model_edges::intent_value(intent)
}
fn result(conclusion: &str, reason: Option<&str>, details: Value) -> Value {
    json!({"schema":"oev2.drawing-reference-check/1","conclusion":conclusion,"reason":reason,
        "details":details,"intended_parent_correctness":"unknown","engineering_adoption":"not_established",
        "scope":"association points/codes/units/owner in one hash-bound part absolute frame; not dimensions, tolerances, unique topology, visual readability or manufacture"})
}
fn point(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let p = [a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?];
    p.iter().all(|x| x.is_finite()).then_some(p)
}
fn entry_complete(e: &Value, unit: &str) -> bool {
    let m = &e["object_1"]["metadata"];
    e["association_position_valid"] == true
        && point(&e["association_position"]).is_some()
        && e["unit"] == unit
        && e["coordinate_basis"] == "UF_DRF assoc_point absolute coordinates"
        && e["point_type_raw"].as_i64().is_some()
        && e["line_type_raw"].as_i64().is_some()
        && e["object_1"]["present"] == true
        && m["type_query"]["return_code"] == 0
        && m["type_query"]["type_raw"].as_i64().is_some()
        && m["type_query"]["subtype_raw"].as_i64().is_some()
        && m["owning_part_query"]["value_valid"] == true
        && m["owning_part_query"]["matches_expected_owner"]
            .as_bool()
            .is_some()
}
fn matches(p: &DrawingReferencePoint, e: &Value, tolerance: f64) -> bool {
    let Some(q) = point(&e["association_position"]) else {
        return false;
    };
    let distance = (p.position[0] - q[0])
        .hypot(p.position[1] - q[1])
        .hypot(p.position[2] - q[2]);
    distance <= tolerance
        && e["point_type_raw"] == p.point_type_raw
        && e["line_type_raw"] == p.line_type_raw
        && e["object_1"]["metadata"]["type_query"]["type_raw"] == p.parent_type_raw
        && e["object_1"]["metadata"]["type_query"]["subtype_raw"] == p.parent_subtype_raw
        && e["object_1"]["metadata"]["owning_part_query"]["matches_expected_owner"] == true
}
// At most eight points. Count up to two bijections; a greedy choice must not
// turn overlapping tolerances or duplicate references into a unique match.
fn assignments(matrix: &[Vec<bool>], row: usize, used: u16) -> usize {
    if row == matrix.len() {
        return 1;
    }
    let mut count = 0;
    for col in 0..matrix.len() {
        if used & (1 << col) == 0 && matrix[row][col] {
            count += assignments(matrix, row + 1, used | (1 << col));
            if count >= 2 {
                return 2;
            }
        }
    }
    count
}

fn check_point_scope(intent: &DrawingReferenceIntent, part: &Value) -> Result<Value> {
    validate(intent)?;
    let a = &part["annotations"];
    if a["schema"] != "oev2.nx-annotation-observation/2" {
        return Ok(result(
            "unknown",
            Some("drawing_reference_observation_unsupported"),
            json!({}),
        ));
    }
    if a["units_query"]["return_code"] != 0 || a["units_query"]["unit"].as_str().is_none() {
        return Ok(result(
            "unknown",
            Some("drawing_reference_unit_unknown"),
            json!({}),
        ));
    }
    if a["units_query"]["unit"] != intent.unit {
        return Ok(result(
            "unknown",
            Some("drawing_reference_unit_mismatch"),
            json!({"required_unit":intent.unit,"native_unit":a["units_query"]["unit"]}),
        ));
    }
    let Some(dimensions) = a["dimensions"].as_array() else {
        return Ok(result(
            "unknown",
            Some("drawing_reference_dimensions_missing"),
            json!({}),
        ));
    };
    ensure!(
        dimensions.len() <= 4096,
        "drawing_reference_dimension_budget"
    );
    let mut candidates = Vec::new();
    let mut incomplete = Vec::new();
    let mut inspected = Vec::new();
    for (index, d) in dimensions.iter().enumerate() {
        let associations = &d["associativity_parents"];
        let Some(entries) = associations["entries"].as_array() else {
            incomplete
                .push(json!({"dimension_index":index,"reason":"association_entries_missing"}));
            continue;
        };
        ensure!(entries.len() <= 256, "drawing_reference_entry_budget");
        if d["retained_query_code"] != 0
            || d["retained"].as_bool().is_none()
            || associations["capture_complete"] != true
            || associations["wrapper_code"] != 0
            || associations["uf_return_code"] != 0
            || entries.is_empty()
            || entries.iter().any(|e| !entry_complete(e, &intent.unit))
        {
            incomplete.push(
                json!({"dimension_index":index,"reason":"native_reference_query_incomplete"}),
            );
            continue;
        }
        let matrix = intent
            .points
            .iter()
            .map(|p| {
                entries
                    .iter()
                    .map(|e| matches(p, e, intent.position_tolerance))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let count = if entries.len() == intent.points.len() {
            assignments(&matrix, 0, 0)
        } else {
            0
        };
        inspected.push(json!({"dimension_index":index,"entry_count":entries.len(),"matching_bijections_up_to_two":count,
            "matching_entry_indices_by_point":matrix.iter().map(|row| row.iter().enumerate()
                .filter_map(|(i,m)| m.then_some(i)).collect::<Vec<_>>()).collect::<Vec<_>>() }));
        if count > 0 {
            candidates.push(json!({"dimension_index":index,"matching_bijections_up_to_two":count,"retained":d["retained"]}));
        }
    }
    let details = json!({"intent_id":intent.intent_id,"intent_sha256":canonical_digest(&intent_value(intent)?)?,
        "dimension_count":dimensions.len(),"candidates":candidates,"incomplete":incomplete,"inspected":inspected,
        "proof_scope":intent.proof_scope,"local_frame":intent.local_frame,"unit":intent.unit});
    let (conclusion, reason) = if !incomplete.is_empty() {
        ("unknown", Some("drawing_reference_incomplete_scope"))
    } else if candidates.is_empty() {
        ("violated", Some("drawing_reference_no_dimension_matches"))
    } else if candidates.len() != 1 || candidates[0]["matching_bijections_up_to_two"] != 1 {
        ("unknown", Some("drawing_reference_ambiguous"))
    } else if candidates[0]["retained"] == true {
        ("violated", Some("drawing_reference_dimension_retained"))
    } else if intent.proof_scope == "unique_model_parent" {
        (
            "unknown",
            Some("drawing_reference_unique_model_parent_unproven"),
        )
    } else {
        ("pass", None)
    };
    let mut output = result(conclusion, reason, details);
    output["reference_point_agreement"] = json!(if incomplete.is_empty()
        && candidates.len() == 1
        && candidates[0]["matching_bijections_up_to_two"] == 1
        && candidates[0]["retained"] == false
    {
        "pass"
    } else {
        conclusion
    });
    Ok(output)
}

pub fn check_points(intent: &DrawingReferenceIntent, part: &Value) -> Result<Value> {
    let mut output = check_point_scope(intent, part)?;
    if intent.schema == "oev2.drawing-reference-intent/1" {
        return Ok(output);
    }
    let mut models = Vec::new();
    for p in &intent.points {
        models.push(match CircleIntentRef::for_point(&intent.schema,p)? {
            Some(e)=>e.select(part)?,
            None=>json!({"conclusion":"unknown","reason":"drawing_reference_model_edge_intent_missing"}),
        });
    }
    output["model_edge_checks"] = json!(models);
    let model_conclusion = if models.iter().any(|r| r["conclusion"] == "violated") {
        "violated"
    } else if models.iter().any(|r| r["conclusion"] != "pass") {
        "unknown"
    } else {
        "pass"
    };
    output["model_edge_agreement"] = json!(model_conclusion);
    output["computed_dimension_agreement"] = json!("not_requested");
    output["scope"] = json!(if intent.schema == "oev2.drawing-reference-intent/2" {
        "/2: full-circle solid-edge selection in the exact part, annotation parent relation and optional ComputedSize; not persistent feature identity, displayed text, manufacturing tolerance, layout or physical adoption"
    } else {
        "/3: independent center/radius tolerances for full-circle solid edges in the exact part, annotation parent relation and optional ComputedSize; not persistent feature identity, displayed text, manufacturing tolerance, layout or physical adoption"
    });
    // Preserve point/query diagnostics even when the additional proof is absent.
    if output["reference_point_agreement"] != "pass" {
        return Ok(output);
    }
    let requested = models.iter().any(|r| r.get("feature_id").is_some())
        || intent.proof_scope == "unique_model_parent";
    if requested && model_conclusion != "pass" {
        output["conclusion"] = json!(model_conclusion);
        output["reason"] = json!(if model_conclusion == "violated" {
            "drawing_reference_model_edge_mismatch"
        } else {
            "drawing_reference_model_edge_unproven"
        });
        return Ok(output);
    }
    if requested {
        let index = output["details"]["candidates"][0]["dimension_index"]
            .as_u64()
            .unwrap() as usize;
        let entries = part["annotations"]["dimensions"][index]["associativity_parents"]["entries"]
            .as_array()
            .unwrap();
        let matrix = intent
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                entries
                    .iter()
                    .map(|e| {
                        matches(p, e, intent.position_tolerance)
                            && e["object_1"]["tag"] == models[i]["candidate_edge_tags"][0]
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if assignments(&matrix, 0, 0) != 1 {
            output["conclusion"] = json!("violated");
            output["reason"] = json!("drawing_reference_model_parent_mismatch");
            output["intended_parent_correctness"] = json!("violated");
            return Ok(output);
        }
        output["intended_parent_correctness"] = json!("pass");
        output["conclusion"] = json!("pass");
        output["reason"] = Value::Null;
    }
    if let Some(required) = &intent.dimension_value {
        let index = output["details"]["candidates"][0]["dimension_index"]
            .as_u64()
            .unwrap() as usize;
        let v = &part["annotations"]["dimensions"][index]["computed_size"];
        let actual = v["value"].as_f64();
        let m = &v["metadata"];
        let complete = v["wrapper_code"] == 0
            && v["value_valid"] == true
            && actual.is_some_and(|n| n.is_finite())
            && v["unit"] == required.unit
            && v["basis"] == "NXOpen::Annotations::Dimension::ComputedSize"
            && m["type_query"]["return_code"] == 0
            && m["type_query"]["type_raw"] == 26
            && m["type_query"]["subtype_raw"].as_i64().is_some()
            && m["owning_part_query"]["value_valid"] == true
            && m["owning_part_query"]["matches_expected_owner"] == true;
        let (conclusion, reason) = if !complete {
            ("unknown", Some("drawing_reference_computed_value_unknown"))
        } else if m["type_query"]["subtype_raw"] != required.dimension_subtype_raw
            || (actual.unwrap() - required.value).abs() > required.numeric_tolerance
        {
            (
                "violated",
                Some("drawing_reference_computed_value_mismatch"),
            )
        } else {
            ("pass", None)
        };
        output["computed_dimension_agreement"] = json!(conclusion);
        output["computed_value_check"] = json!({"required":required,"actual":v});
        if conclusion != "pass" {
            output["conclusion"] = json!(conclusion);
            output["reason"] = json!(reason);
        }
    }
    Ok(output)
}

/// Reuse the completed NX receipt/source check, not a parallel tool engine.
pub fn observe(
    job_root: &Path,
    input: &str,
    intent: &DrawingReferenceIntent,
    native_result_ref: &str,
) -> Result<NativeDrawingObservation> {
    validate(intent)?;
    observe_part(job_root, input, &intent.object, native_result_ref)
}

/// Observe one exact input from a completed inspection, independently of the
/// obligation being checked. The receipt retains its original worker identity.
pub fn observe_part(
    job_root: &Path,
    input: &str,
    object: &ObjectIdentity,
    native_result_ref: &str,
) -> Result<NativeDrawingObservation> {
    object
        .validate()
        .map_err(|_| anyhow::anyhow!("drawing_native_object_invalid"))?;
    ensure!(
        !native_result_ref.trim().is_empty(),
        "drawing_reference_native_source_ref_missing"
    );
    let (native, job) = super::associations::receipt(job_root)?;
    ensure!(
        native.action == "inspect_detailed",
        "drawing_reference_requires_inspected_input"
    );
    let source = job
        .inputs
        .iter()
        .find(|f| f.path.as_str() == input)
        .context("drawing_reference_part_input_missing")?;
    let raw = read_regular_bytes(&super::member(job_root, input)?, 512 * 1024 * 1024)?;
    source.identity.verify(&raw)?;
    ensure!(
        oe_core::sha256(&raw) == object.source_sha256,
        "drawing_reference_cad_source_mismatch"
    );
    let parts = native.readback["parts"]
        .as_array()
        .context("drawing_reference_native_parts_missing")?;
    let suffix = format!("/{}", input.replace('\\', "/"));
    let selected = parts
        .iter()
        .filter(|p| {
            p["path"]
                .as_str()
                .is_some_and(|path| path.replace('\\', "/").ends_with(&suffix))
        })
        .collect::<Vec<_>>();
    ensure!(
        selected.len() == 1,
        "drawing_reference_native_part_ambiguous"
    );
    let result_raw = read_regular_bytes(
        &super::member(job_root, "native-result.json")?,
        16 * 1024 * 1024,
    )?;
    let reread: oe_nx_protocol::NativeResult = oe_nx_protocol::decode(&result_raw)?;
    ensure!(
        oe_nx_protocol::record_digest(&native)? == oe_nx_protocol::record_digest(&reread)?,
        "drawing_reference_native_changed_during_check"
    );
    Ok(NativeDrawingObservation {
        schema: "oev2.native-drawing-observation/1".into(),
        object: object.clone(),
        native_result_ref: native_result_ref.into(),
        native_result_sha256: oe_core::sha256(result_raw),
        part: selected[0].clone(),
        job_directory: None,
        part_input: None,
    })
}

/// A current Project can repeat the same receipt verification without guessing
/// a job from a digest. Keep the /1 diagnostic constructor and wire identity.
pub fn observe_bound(
    job_root: &Path,
    input: &str,
    intent: &DrawingReferenceIntent,
    native_result_ref: &str,
) -> Result<NativeDrawingObservation> {
    validate(intent)?;
    observe_part_bound(job_root, input, &intent.object, native_result_ref)
}

/// Explicit locators are shared by reference dimensions and drafting pages.
pub fn observe_part_bound(
    job_root: &Path,
    input: &str,
    object: &ObjectIdentity,
    native_result_ref: &str,
) -> Result<NativeDrawingObservation> {
    let mut observation = observe_part(job_root, input, object, native_result_ref)?;
    observation.schema = "oev2.native-drawing-observation/2".into();
    observation.job_directory = Some(
        job_root
            .to_str()
            .context("drawing_native_job_path_not_utf8")?
            .into(),
    );
    observation.part_input = Some(input.into());
    Ok(observation)
}

/// Optional /2 locator fields do not silently change frozen /1 JSON bytes.
pub fn observation_value(observation: &NativeDrawingObservation) -> Result<Value> {
    let mut value = serde_json::to_value(observation)?;
    if observation.schema == "oev2.native-drawing-observation/1" {
        ensure!(
            observation.job_directory.is_none() && observation.part_input.is_none(),
            "drawing_native_legacy_locator_forbidden"
        );
        let object = value
            .as_object_mut()
            .context("drawing_native_observation_not_object")?;
        object.remove("job_directory");
        object.remove("part_input");
    }
    Ok(value)
}
