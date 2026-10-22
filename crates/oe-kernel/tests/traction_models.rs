//! Independent force/power identities and current-premise counterexamples.
//! These synthetic cases do not establish the real film's friction or loading.
use oe_contracts::EngineeringCalculation;
use oe_kernel::{calculation::calculate, engineering, project::Project};
use serde_json::{Value, json};

fn quantity(value: f64, unit: &str) -> Value {
    json!({"value":value,"unit":unit,"precision":"synthetic exact decimal input"})
}
fn calculation(method: &str, inputs: Value) -> EngineeringCalculation {
    serde_json::from_value(json!({
        "calculation_id":"traction-study",
        "object":{"object_id":"roller","revision":"r1","condition_scope":"study","source_sha256":"a".repeat(64)},
        "method":method,"inputs":inputs,
        "assumptions":["Explicit synthetic single driven contact; no physical qualification"],
        "validity_conditions":["Declared force definitions and steady/static regimes must be checked"],
        "invalid_when":["Different contact, force interpretation, dynamics or units"],
        "source_refs":["synthetic-design"]
    })).unwrap()
}
fn require_error(c: &EngineeringCalculation, expected: &str) {
    let error = calculate(c).unwrap_err().to_string();
    assert!(error.contains(expected), "expected {expected}, got {error}");
}

#[test]
fn independent_force_ratio_and_work_over_time_oracles() {
    // 4/10 = 2/5. Over 10 s the travel is 3 m and the work is 12 J,
    // giving 12/10 W without deriving an oracle from the implementation.
    for (method, inputs, symbol, value, unit) in [
        (
            "coulomb-required-friction/1",
            json!({"tangential_force":quantity(4.0,"N"),"normal_force":quantity(10.0,"N")}),
            "required_friction_coefficient",
            0.4,
            "1",
        ),
        (
            "coulomb-single-contact-limit/1",
            json!({"friction_coefficient":quantity(0.6,"1"),"normal_force":quantity(10.0,"N")}),
            "traction_limit",
            6.0,
            "N",
        ),
        (
            "steady-traction-power/1",
            json!({"tangential_force":quantity(4.0,"N"),"linear_speed":quantity(0.3,"m/s")}),
            "mechanical_power",
            1.2,
            "W",
        ),
    ] {
        let result = calculate(&calculation(method, inputs)).unwrap();
        assert_eq!(result["execution"], "completed");
        assert_eq!(result["conclusion"], "model_result_only");
        assert_eq!(result["symbol"], symbol);
        assert_eq!(result["result"]["unit"], unit);
        assert!((result["result"]["value"].as_f64().unwrap() - value).abs() < 1e-12);
        assert_eq!(result["engineering_adoption"], "not_established");
    }
}

#[test]
fn no_friction_default_no_unit_conversion_and_no_second_contact() {
    let c = calculation(
        "coulomb-single-contact-limit/1",
        json!({"normal_force":quantity(10.0,"N"),"friction_coefficient":null}),
    );
    let result = calculate(&c).unwrap();
    assert_eq!(result["execution"], "not_run");
    assert_eq!(result["conclusion"], "unknown");
    assert_eq!(result["missing_inputs"], json!(["friction_coefficient"]));
    assert!(result["result"].is_null());
    let pressure = calculation(
        "coulomb-required-friction/1",
        json!({"tangential_force":quantity(4.0,"N"),"normal_force":quantity(10.0,"kPa")}),
    );
    require_error(&pressure, "engineering_quantity_unit_mismatch");
    let speed = calculation(
        "steady-traction-power/1",
        json!({"tangential_force":quantity(4.0,"N"),"linear_speed":quantity(300.0,"mm/s")}),
    );
    require_error(&speed, "engineering_quantity_unit_mismatch");
    let mut two_contacts = c;
    two_contacts.inputs["contact_count"] = json!(2);
    require_error(&two_contacts, "engineering_traction_input_unsupported");
}

#[test]
fn invalid_present_input_is_rejected_even_with_another_missing_input() {
    for normal in [0.0, -1.0] {
        let c = calculation(
            "coulomb-required-friction/1",
            json!({"normal_force":quantity(normal,"N"),"tangential_force":null}),
        );
        require_error(&c, "engineering_traction_input_outside_model:normal_force");
    }
    let c = calculation(
        "steady-traction-power/1",
        json!({"linear_speed":quantity(-0.3,"m/s"),"tangential_force":null}),
    );
    require_error(&c, "engineering_traction_input_outside_model:linear_speed");
    let c = calculation(
        "coulomb-single-contact-limit/1",
        json!({"friction_coefficient":quantity(-0.1,"1")}),
    );
    require_error(
        &c,
        "engineering_traction_input_outside_model:friction_coefficient",
    );
}

#[test]
fn zero_limits_large_coefficients_and_nonfinite_results_are_distinct() {
    let c = calculation(
        "coulomb-single-contact-limit/1",
        json!({"normal_force":quantity(0.0,"N"),"friction_coefficient":quantity(2.0,"1")}),
    );
    assert_eq!(calculate(&c).unwrap()["result"]["value"], 0.0);
    let c = calculation(
        "coulomb-single-contact-limit/1",
        json!({"normal_force":quantity(3.0,"N"),"friction_coefficient":quantity(2.0,"1")}),
    );
    assert_eq!(calculate(&c).unwrap()["result"]["value"], 6.0);
    let c = calculation(
        "steady-traction-power/1",
        json!({"linear_speed":quantity(0.0,"m/s"),"tangential_force":quantity(4.0,"N")}),
    );
    assert_eq!(calculate(&c).unwrap()["result"]["value"], 0.0);
    let c = calculation(
        "steady-traction-power/1",
        json!({"linear_speed":quantity(1e308,"m/s"),"tangential_force":quantity(1e308,"N")}),
    );
    require_error(&c, "engineering_calculation_nonfinite");
}

fn project(c: &EngineeringCalculation, categories: &[(&str, &str)]) -> (Project, Value) {
    let mut facts = vec![];
    for (property, value) in c.inputs.as_object().unwrap() {
        facts.push(json!({"property":property,"value":value,"status":"declared","source_refs":["synthetic-design"]}));
    }
    for (property, value) in categories {
        facts.push(json!({"property":property,"value":value,"status":"declared","source_refs":["synthetic-design"]}));
    }
    let basis = json!({"schema":"oev2.engineering-basis/1","object_id":"roller","object_revision":"r1","purpose":"study traction under declared premises","acceptance":["real grip and physical safety still require independent evidence"],"facts":facts,"assertions":[{
        "assertion_id":"contact-boundary","kind":"model_boundary","subject":c.calculation_id,
        "requirements":{"method":c.method,"inputs":c.inputs},"source_refs":["synthetic-design"],"affected_decisions":[]
    }]});
    let project = serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"synthetic-traction-model","revision":"r1","purpose":"study traction under declared premises","objects":{"roller":"r1"},"conditions":{"study":serde_json::to_string(&basis).unwrap()},"evidence":{"synthetic-design":"a".repeat(64)},"decisions":[],"dependencies":{}})).unwrap();
    (project, basis)
}

#[test]
fn actual_project_force_meaning_unknown_and_conflict_survive_correct_arithmetic() {
    let c = calculation(
        "coulomb-required-friction/1",
        json!({"tangential_force":quantity(4.0,"N"),"normal_force":quantity(10.0,"N")}),
    );
    let (mut p, mut basis) = project(
        &c,
        &[
            ("contact_model", "single_driven_coulomb_contact"),
            (
                "normal_force_definition",
                "resultant_normal_force_at_driven_contact",
            ),
            (
                "tangential_force_definition",
                "required_tangential_contact_force",
            ),
            ("friction_regime", "static_limit"),
        ],
    );
    assert_eq!(engineering::calculation_check(&p, &c).conclusion, "pass");
    let fact = basis["facts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["property"] == "normal_force_definition")
        .unwrap();
    fact["status"] = json!("unknown");
    fact["value"] = Value::Null;
    p.conditions
        .insert("study".into(), serde_json::to_string(&basis).unwrap());
    assert_eq!(engineering::calculation_check(&p, &c).conclusion, "unknown");
    assert_eq!(calculate(&c).unwrap()["result"]["value"], 0.4);
    let fact = basis["facts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["property"] == "normal_force_definition")
        .unwrap();
    fact["status"] = json!("declared");
    fact["value"] = json!("pressure_over_unspecified_area");
    p.conditions
        .insert("study".into(), serde_json::to_string(&basis).unwrap());
    let before = serde_json::to_value(&p).unwrap();
    assert_eq!(
        engineering::calculation_check(&p, &c).conclusion,
        "violated"
    );
    assert_eq!(serde_json::to_value(&p).unwrap(), before);
}

#[test]
fn capacity_and_power_boundaries_require_their_own_physical_regimes() {
    let c = calculation(
        "coulomb-single-contact-limit/1",
        json!({"normal_force":quantity(10.0,"N"),"friction_coefficient":quantity(0.6,"1")}),
    );
    let (mut p, mut basis) = project(
        &c,
        &[
            ("contact_model", "single_driven_coulomb_contact"),
            (
                "normal_force_definition",
                "resultant_normal_force_at_driven_contact",
            ),
            ("friction_regime", "static_limit"),
        ],
    );
    assert_eq!(engineering::calculation_check(&p, &c).conclusion, "pass");
    let fact = basis["facts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["property"] == "friction_regime")
        .unwrap();
    fact["value"] = json!("sliding_dynamic");
    p.conditions
        .insert("study".into(), serde_json::to_string(&basis).unwrap());
    assert_eq!(
        engineering::calculation_check(&p, &c).conclusion,
        "violated"
    );
    let c = calculation(
        "steady-traction-power/1",
        json!({"linear_speed":quantity(0.3,"m/s"),"tangential_force":quantity(4.0,"N")}),
    );
    let (mut p, mut basis) = project(
        &c,
        &[
            ("motion_regime", "steady_translation"),
            (
                "tangential_force_definition",
                "required_tangential_contact_force",
            ),
            ("power_boundary", "ideal_contact_mechanical_power"),
        ],
    );
    assert_eq!(engineering::calculation_check(&p, &c).conclusion, "pass");
    basis["facts"]
        .as_array_mut()
        .unwrap()
        .retain(|f| f["property"] != "motion_regime");
    p.conditions
        .insert("study".into(), serde_json::to_string(&basis).unwrap());
    assert_eq!(engineering::calculation_check(&p, &c).conclusion, "unknown");
}
