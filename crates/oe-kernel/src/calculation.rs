//! Shared deterministic engineering models; premises and numeric acceptance are separate.
use anyhow::{Context, Result, bail, ensure};
use oe_contracts::{EngineeringCalculation, Quantity};
use serde_json::{Value, json};
fn generated(r: Result<(), Vec<String>>) -> Result<()> {
    r.map_err(|e| anyhow::anyhow!("engineering_contract_invalid:{}", e.join(",")))
}
fn stated(values: &[String]) -> bool {
    !values.is_empty() && values.iter().all(|s| !s.trim().is_empty())
}
fn quantity(value: &Value, unit: &str) -> Result<Option<f64>> {
    if value.is_null() {
        return Ok(None);
    }
    let q: Quantity =
        serde_json::from_value(value.clone()).context("engineering_quantity_invalid")?;
    generated(q.validate())?;
    ensure!(q.unit == unit, "engineering_quantity_unit_mismatch");
    Ok(Some(q.value))
}
/// These narrow models accept forces, never a pressure relabeled as a force.
/// Validate every supplied value even when another necessary input is absent.
fn traction_inputs(c: &EngineeringCalculation) -> Result<()> {
    let fields: &[(&str, &str, bool)] = match c.method.as_str() {
        "coulomb-single-contact-limit/1" => &[
            ("friction_coefficient", "1", false),
            ("normal_force", "N", false),
        ],
        "coulomb-required-friction/1" => &[
            ("tangential_force", "N", false),
            ("normal_force", "N", true),
        ],
        "steady-traction-power/1" => &[
            ("tangential_force", "N", false),
            ("linear_speed", "m/s", false),
        ],
        _ => return Ok(()),
    };
    let input = c
        .inputs
        .as_object()
        .context("engineering_traction_inputs_invalid")?;
    ensure!(
        input
            .keys()
            .all(|key| fields.iter().any(|(name, _, _)| name == key)),
        "engineering_traction_input_unsupported"
    );
    for (name, unit, positive) in fields {
        if let Some(value) = quantity(&c.inputs[*name], unit)? {
            ensure!(
                if *positive { value > 0.0 } else { value >= 0.0 },
                "engineering_traction_input_outside_model:{name}"
            );
        }
    }
    Ok(())
}
/// These are explicit models, not material-property lookup or certification.
pub fn calculate(c: &EngineeringCalculation) -> Result<Value> {
    generated(c.validate())?;
    ensure!(
        stated(&c.assumptions)
            && stated(&c.validity_conditions)
            && stated(&c.invalid_when)
            && stated(&c.source_refs),
        "engineering_calculation_boundary_missing"
    );
    traction_inputs(c)?;
    let mut missing = Vec::new();
    let mut get = |name: &str, unit: &str| -> Result<f64> {
        match quantity(&c.inputs[name], unit)? {
            Some(v) => Ok(v),
            None => {
                missing.push(name.to_owned());
                Ok(0.0)
            }
        }
    };
    let (symbol, unit, value, formula) = match c.method.as_str() {
        "coulomb-single-contact-limit/1" => {
            let coefficient = get("friction_coefficient", "1")?;
            let normal_force = get("normal_force", "N")?;
            (
                "traction_limit",
                "N",
                coefficient * normal_force,
                "mu_s N; static-friction upper bound at one explicitly defined driven contact, not actual traction",
            )
        }
        "coulomb-required-friction/1" => {
            let tangential_force = get("tangential_force", "N")?;
            let normal_force = get("normal_force", "N")?;
            (
                "required_friction_coefficient",
                "1",
                tangential_force / normal_force,
                "F_required / N; required single-contact coefficient, not a measured material property",
            )
        }
        "steady-traction-power/1" => {
            let tangential_force = get("tangential_force", "N")?;
            let speed = get("linear_speed", "m/s")?;
            (
                "mechanical_power",
                "W",
                tangential_force * speed,
                "F_required v; ideal contact mechanical power in steady translation, excluding drivetrain losses and acceleration",
            )
        }
        "beam-cantilever-end-load/1" | "beam-simple-center-load/1" => {
            let force = get("force", "N")?;
            let span = get("span", "mm")?;
            let modulus = get("elastic_modulus", "MPa")?;
            let inertia = get("second_moment", "mm4")?;
            if missing.is_empty() {
                ensure!(
                    force >= 0.0 && span > 0.0 && modulus > 0.0 && inertia > 0.0,
                    "engineering_beam_input_outside_model"
                );
            }
            let divisor = if c.method == "beam-cantilever-end-load/1" {
                3.0
            } else {
                48.0
            };
            (
                "deflection",
                "mm",
                force * span.powi(3) / (divisor * modulus * inertia),
                if divisor == 3.0 {
                    "F L^3 / (3 E I)"
                } else {
                    "F L^3 / (48 E I)"
                },
            )
        }
        "round-shaft-second-moment/1" => {
            let diameter = get("diameter", "mm")?;
            if missing.is_empty() {
                ensure!(diameter > 0.0, "engineering_shaft_input_outside_model");
            }
            (
                "second_moment",
                "mm4",
                std::f64::consts::PI * diameter.powi(4) / 64.0,
                "pi d^4 / 64",
            )
        }
        "radial-error-2d/1" => {
            let axial = get("axial_error", "mm")?;
            let circumferential = get("circumferential_error", "mm")?;
            (
                "radial_error",
                "mm",
                axial.hypot(circumferential),
                "sqrt(axial_error^2 + circumferential_error^2); one explicitly defined 2D datum and state",
            )
        }
        "linear-error-worst-case/1" => {
            let mut total = 0.0;
            if c.inputs["components"].is_null() {
                missing.push("components".into());
            } else {
                let values = c.inputs["components"]
                    .as_array()
                    .context("engineering_error_components_invalid")?;
                ensure!(
                    !values.is_empty() && values.len() <= 32,
                    "engineering_error_component_budget"
                );
                for value in values {
                    if let Some(v) = quantity(value, "mm")? {
                        ensure!(v >= 0.0, "engineering_error_component_negative");
                        total += v;
                    } else {
                        missing.push("component".into());
                    }
                }
            }
            (
                "worst_case_error",
                "mm",
                total,
                "sum of declared nonnegative error bounds; no statistical independence assumed",
            )
        }
        _ => bail!("engineering_calculation_method_unsupported"),
    };
    if !missing.is_empty() {
        return Ok(
            json!({"calculation_id":c.calculation_id,"execution":"not_run","conclusion":"unknown","missing_inputs":missing,"result":null,"engineering_adoption":"not_established"}),
        );
    }
    ensure!(value.is_finite(), "engineering_calculation_nonfinite");
    Ok(
        json!({"calculation_id":c.calculation_id,"object":c.object,"execution":"completed","conclusion":"model_result_only","symbol":symbol,
        "result":{"value":value,"unit":unit,"precision":"f64 computation; input precision and material/geometry uncertainty retained separately"},"formula":formula,
        "inputs_sha256":oe_core::json::canonical_digest(&c.inputs)?,"assumptions":c.assumptions,"validity_conditions":c.validity_conditions,"invalid_when":c.invalid_when,"source_refs":c.source_refs,"engineering_adoption":"not_established"}),
    )
}
