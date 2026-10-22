//! Fixed engineering counterexamples. Numbers are from ordinary independent
//! balance arithmetic/norms; synthetic fixtures do not assert physical facts.
use oe_contracts::ObjectIdentity;
use oe_kernel::{engineering, project::Project};
use serde_json::{Value, json};

fn q(v: f64, unit: &str) -> Value {
    json!({"value":v,"unit":unit,"precision":"declared fixture only"})
}
fn fixture() -> (Project, ObjectIdentity, Value) {
    let o = ObjectIdentity {
        object_id: "guide".into(),
        revision: "r1".into(),
        condition_scope: "loaded".into(),
        source_sha256: "a".repeat(64),
    };
    let inputs = json!({"force":q(20.0,"N"),"span":q(100.0,"mm"),"elastic_modulus":q(200000.0,"MPa"),"second_moment":q(1000.0,"mm4")});
    let calc = json!({"calculation_id":"beam","object":o,"method":"beam-cantilever-end-load/1","inputs":inputs,
        "assumptions":["linear elastic"],"validity_conditions":["fixed-free end load; small displacement"],"invalid_when":["unverified boundary or geometry"],"source_refs":["fixture"]});
    let req = json!({"schema":"oev2.numeric-acceptance/1","requirement_id":"deflection","requirement_property":"acceptance:deflection","object":o,"state":"loaded","datum":"guide-frame",
        "metric":"scalar","relation":"maximum","upper":q(0.02,"mm"),"lower":null,"target":null,"tolerance":null,
        "definition_status":"confirmed","coverage_status":"declared_complete","evidence_role":"calculated_model","source_refs":["fixture"]});
    let mut facts:Vec<Value> = inputs.as_object().unwrap().iter().map(|(property,value)|json!({"property":property,"value":value,"status":"declared","source_refs":["fixture"]})).collect();
    for (property, value) in [
        ("support", json!("cantilever_fixed_free")),
        ("load_position", json!("end")),
        ("regime", json!("linear_elastic_small_deflection")),
        ("state", json!("loaded")),
        ("datum", json!("guide-frame")),
        ("acceptance:deflection", req.clone()),
    ] {
        facts.push(json!({"property":property,"value":value,"status":"declared","source_refs":["fixture"]}));
    }
    let snapshot: serde_json::Map<String, Value> = facts
        .iter()
        .map(|f| (f["property"].as_str().unwrap().into(), f["value"].clone()))
        .collect();
    let assessment = json!({"schema":"oev2.numeric-assessment/1","requirement":req,"calculation":calc,"observation":null,
        "condition_values":snapshot,"source_versions":{"fixture":"a".repeat(64)}});
    let b = json!({"schema":"oev2.engineering-basis/1","object_id":"guide","object_revision":"r1","purpose":"bounded guide study","acceptance":["deflection <= 0.02 mm"],"facts":facts,
        "assertions":[{"assertion_id":"boundary","kind":"model_boundary","subject":"beam","requirements":{"method":calc["method"],"inputs":inputs},"source_refs":["fixture"],"affected_decisions":[]},
        {"assertion_id":"numeric","kind":"numeric_acceptance","subject":"deflection","requirements":assessment,"source_refs":["fixture"],"affected_decisions":["route"]}]});
    let p:Project=serde_json::from_value(json!({"schema":"oev2.project/1","project_id":"numeric-fixture","revision":"r1","purpose":b["purpose"],"objects":{"guide":"r1"},"conditions":{"loaded":serde_json::to_string(&b).unwrap()},"evidence":{"fixture":"a".repeat(64)},
        "decisions":[{"id":"route","object_id":"guide","object_revision":"r1","condition_scope":"loaded","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["deflection"]},
        {"id":"inspection","object_id":"guide","object_revision":"r1","condition_scope":"loaded","source_sha256":"a".repeat(64),"conclusion":"unknown","adopted":false,"basis":["route"]}],
        "dependencies":{"inspection":["route"]}})).unwrap();
    (p, o, b)
}
fn set(p: &mut Project, b: &Value) {
    p.conditions
        .insert("loaded".into(), serde_json::to_string(b).unwrap());
}
fn sync_snapshot(b: &mut Value) {
    let snapshot: serde_json::Map<String, Value> = b["facts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["property"].as_str().unwrap().into(), f["value"].clone()))
        .collect();
    b["assertions"][1]["requirements"]["condition_values"] = json!(snapshot);
}
fn change_fact(b: &mut Value, key: &str, value: Value) {
    let fact = b["facts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|f| f["property"] == key)
        .unwrap();
    fact["value"] = value;
}
fn update_req(b: &mut Value) {
    let req = b["assertions"][1]["requirements"]["requirement"].clone();
    change_fact(b, "acceptance:deflection", req);
    sync_snapshot(b);
}
fn numeric(p: &Project, o: &ObjectIdentity) -> oe_kernel::CheckResult {
    engineering::checks(p, o).pop().unwrap()
}

#[test]
fn correct_model_and_arithmetic_can_exceed_numeric_requirement() {
    let (mut p, o, mut b) = fixture();
    let checks = engineering::checks(&p, &o);
    assert_eq!(checks[1].conclusion, "pass");
    let c = &checks[2];
    assert_eq!(c.conclusion, "violated");
    assert_eq!(c.failures, ["numeric_requirement_exceeded"]);
    let d = &c.result["details"];
    assert_eq!(d["arithmetic"]["execution"], "completed");
    assert!((d["numeric_acceptance"]["value"].as_f64().unwrap() - 1.0 / 30.0).abs() < 1e-14);
    assert_eq!(d["affected_decisions"], json!(["inspection", "route"]));
    assert_eq!(c.result["engineering_adoption"], "not_established");
    // Independent balanced positive reference: 10*100^3/(3*200000*1000)=1/60.
    change_fact(&mut b, "force", q(10.0, "N"));
    b["assertions"][0]["requirements"]["inputs"]["force"] = q(10.0, "N");
    b["assertions"][1]["requirements"]["calculation"]["inputs"]["force"] = q(10.0, "N");
    sync_snapshot(&mut b);
    set(&mut p, &b);
    assert_eq!(numeric(&p, &o).conclusion, "pass");
}
#[test]
fn changed_conditions_or_sources_preserve_diagnostic_arithmetic_without_current_support() {
    let (mut p, o, mut b) = fixture();
    change_fact(&mut b, "force", q(30.0, "N"));
    set(&mut p, &b);
    let c = numeric(&p, &o);
    assert_eq!(c.conclusion, "unknown");
    assert!(c.failures[0].contains("numeric_current_condition_changed:force"));
    assert_eq!(c.result["details"]["arithmetic"]["execution"], "completed");
    sync_snapshot(&mut b);
    set(&mut p, &b);
    let c = numeric(&p, &o);
    assert_eq!(c.conclusion, "violated");
    assert_eq!(
        c.result["details"]["model_applicability"]["conclusion"],
        "violated"
    );
    let (mut p, o, _) = fixture();
    p.evidence.insert("fixture".into(), "b".repeat(64));
    let c = numeric(&p, &o);
    assert_eq!(c.conclusion, "unknown");
    assert!(c.failures[0].contains("numeric_source_version_stale"));
}
#[test]
fn unit_unknown_input_and_unsupported_models_do_not_become_pass() {
    for (method, key, value, expected) in [
        (
            "beam-cantilever-end-load/1",
            "span",
            q(100.0, "m"),
            "unit_mismatch",
        ),
        (
            "beam-cantilever-end-load/1",
            "force",
            Value::Null,
            "engineering_condition_unknown:force",
        ),
        ("unimplemented-beam/1", "span", q(100.0, "mm"), "supported"),
    ] {
        let (mut p, o, mut b) = fixture();
        b["assertions"][0]["requirements"]["method"] = json!(method);
        b["assertions"][1]["requirements"]["calculation"]["method"] = json!(method);
        b["assertions"][0]["requirements"]["inputs"][key] = value.clone();
        b["assertions"][1]["requirements"]["calculation"]["inputs"][key] = value.clone();
        change_fact(&mut b, key, value);
        sync_snapshot(&mut b);
        set(&mut p, &b);
        let c = numeric(&p, &o);
        assert_ne!(c.conclusion, "pass");
        let all = serde_json::to_string(&c).unwrap();
        assert!(all.contains(expected), "{all}");
        if key == "force" {
            assert_eq!(c.result["details"]["arithmetic"]["execution"], "not_run");
            assert_eq!(
                c.result["details"]["arithmetic"]["missing_inputs"],
                json!(["force"])
            );
            assert_eq!(
                c.result["details"]["numeric_acceptance"]["conclusion"],
                "unknown"
            );
        }
    }
}
fn observed(b: &mut Value, metric: &str, values: Value, u: Value) {
    let a = &mut b["assertions"][1]["requirements"];
    a["requirement"]["metric"] = json!(metric);
    a["requirement"]["evidence_role"] = json!("measurement");
    a["requirement"]["upper"] = q(0.03, "mm");
    let names = if metric == "radial_2d" {
        json!(["axial", "circumferential"])
    } else {
        json!(["axis"])
    };
    a["observation"] = json!({"object":a["requirement"]["object"],"state":"loaded","datum":"guide-frame","metric":metric,"evidence_role":"measurement",
        "values":values,"component_names":names,"expanded_uncertainty":u,"uncertainty_basis":"synthetic explicitly bounded fixture, not physical calibration","source_refs":["fixture"]});
    a["calculation"] = Value::Null;
    update_req(b);
}
#[test]
fn single_axis_and_two_dimensional_acceptance_are_different() {
    let (mut p, o, mut b) = fixture();
    observed(&mut b, "scalar", json!([q(0.025, "mm")]), q(0.0, "mm"));
    set(&mut p, &b);
    assert_eq!(numeric(&p, &o).conclusion, "pass");
    observed(
        &mut b,
        "radial_2d",
        json!([q(0.025, "mm"), q(0.025, "mm")]),
        q(0.0, "mm"),
    );
    set(&mut p, &b);
    let c = numeric(&p, &o);
    assert_eq!(c.conclusion, "violated");
    assert!(
        (c.result["details"]["numeric_acceptance"]["value"]
            .as_f64()
            .unwrap()
            - 2.0_f64.sqrt() * 0.025)
            .abs()
            < 1e-14
    );
    b["assertions"][1]["requirements"]["observation"]["metric"] = json!("scalar");
    set(&mut p, &b);
    assert!(numeric(&p, &o).failures[0].contains("numeric_observation_metric_mismatch"));
}
#[test]
fn uncertainty_coverage_drafts_and_local_proxy_cannot_certify_final_acceptance() {
    let (mut p, o, mut b) = fixture();
    observed(&mut b, "absolute", json!([q(0.028, "mm")]), q(0.004, "mm"));
    set(&mut p, &b);
    let c = numeric(&p, &o);
    assert_eq!(c.conclusion, "unknown");
    assert_eq!(c.failures, ["numeric_uncertainty_straddles_limit"]);
    b["assertions"][1]["requirements"]["observation"]["expanded_uncertainty"] = Value::Null;
    set(&mut p, &b);
    assert!(numeric(&p, &o).failures[0].contains("numeric_uncertainty_missing"));
    observed(&mut b, "absolute", json!([q(0.01, "mm")]), q(0.001, "mm"));
    b["assertions"][1]["requirements"]["requirement"]["definition_status"] = json!("draft");
    update_req(&mut b);
    set(&mut p, &b);
    let c = numeric(&p, &o);
    assert_eq!(c.conclusion, "unknown");
    assert_eq!(
        c.result["details"]["numeric_acceptance"]["conditional_comparison"],
        "pass"
    );
    b["assertions"][1]["requirements"]["requirement"]["definition_status"] = json!("confirmed");
    b["assertions"][1]["requirements"]["requirement"]["coverage_status"] = json!("unknown");
    update_req(&mut b);
    set(&mut p, &b);
    assert_eq!(numeric(&p, &o).conclusion, "unknown");
    b["assertions"][1]["requirements"]["requirement"]["coverage_status"] =
        json!("declared_complete");
    update_req(&mut b);
    b["assertions"][1]["requirements"]["observation"]["evidence_role"] = json!("analytic_proxy");
    set(&mut p, &b);
    assert!(numeric(&p, &o).failures[0].contains("numeric_evidence_role_mismatch"));
}
#[test]
fn current_object_state_requirement_and_tolerance_have_specific_diagnostics() {
    let (mut p, o, mut b) = fixture();
    b["assertions"][1]["requirements"]["requirement"]["object"]["revision"] = json!("old");
    set(&mut p, &b);
    assert!(numeric(&p, &o).failures[0].contains("numeric_object_version_or_source_mismatch"));
    let (mut p, o, mut b) = fixture();
    change_fact(&mut b, "state", json!("after-modification"));
    set(&mut p, &b);
    assert!(numeric(&p, &o).failures[0].contains("numeric_current_state_or_datum_mismatch"));
    let (mut p, o, mut b) = fixture();
    let mut r = b["assertions"][1]["requirements"]["requirement"].clone();
    r["upper"] = q(0.04, "mm");
    change_fact(&mut b, "acceptance:deflection", r);
    set(&mut p, &b);
    assert!(numeric(&p, &o).failures[0].contains("numeric_current_requirement_changed"));
    let (mut p, o, mut b) = fixture();
    observed(&mut b, "scalar", json!([q(5.05, "mm")]), q(0.01, "mm"));
    let r = &mut b["assertions"][1]["requirements"]["requirement"];
    r["relation"] = json!("target_tolerance");
    r["upper"] = Value::Null;
    r["target"] = q(5.0, "mm");
    r["tolerance"] = q(0.1, "mm");
    update_req(&mut b);
    set(&mut p, &b);
    assert_eq!(numeric(&p, &o).conclusion, "pass");
    b["assertions"][1]["requirements"]["requirement"]["tolerance"] = q(-0.1, "mm");
    update_req(&mut b);
    set(&mut p, &b);
    assert!(numeric(&p, &o).failures[0].contains("numeric_tolerance_negative"));
}

#[test]
fn absent_result_is_specific_unknown_and_does_not_become_numeric_pass() {
    let (mut p, o, mut b) = fixture();
    b["assertions"][1]["requirements"]["calculation"] = Value::Null;
    b["assertions"][1]["requirements"]["observation"] = Value::Null;
    set(&mut p, &b);
    let c = numeric(&p, &o);
    assert_eq!(c.conclusion, "unknown");
    assert_eq!(c.failures, ["numeric_result_missing"]);
    assert_eq!(c.result["details"]["arithmetic"]["execution"], "not_run");
    assert_eq!(
        c.result["details"]["numeric_acceptance"]["conclusion"],
        "unknown"
    );
}

fn strict_observed(value: f64, uncertainty: f64) -> (Project, ObjectIdentity, Value) {
    let (mut p, o, mut b) = fixture();
    observed(
        &mut b,
        "absolute",
        json!([q(value, "mm")]),
        q(uncertainty, "mm"),
    );
    let reference = json!({"object_id":"lens-A","revision":"lens-r1","condition_scope":"loaded","source_sha256":"b".repeat(64)});
    let assessment = &mut b["assertions"][1]["requirements"];
    assessment["schema"] = json!("oev2.numeric-assessment/2");
    assessment["requirement"]["schema"] = json!("oev2.numeric-acceptance/2");
    assessment["requirement"]["relation"] = json!("strict_maximum");
    assessment["requirement"]["reference_object"] = reference.clone();
    assessment["requirement"]["source_refs"] = json!(["fixture", "reference-fixture"]);
    assessment["observation"]["reference_object"] = reference;
    assessment["observation"]["source_refs"] = json!(["fixture", "reference-fixture"]);
    assessment["source_versions"]["reference-fixture"] = json!("b".repeat(64));
    p.objects.insert("lens-A".into(), "lens-r1".into());
    p.evidence
        .insert("reference-fixture".into(), "b".repeat(64));
    update_req(&mut b);
    set(&mut p, &b);
    (p, o, b)
}

#[test]
fn strict_upper_boundary_and_old_inclusive_wire_are_distinct() {
    for (value, expected) in [(0.029, "pass"), (0.03, "violated"), (0.031, "violated")] {
        let (p, o, _) = strict_observed(value, 0.0);
        let c = numeric(&p, &o);
        assert_eq!(c.conclusion, expected, "value={value}");
        assert_eq!(
            c.result["details"]["numeric_acceptance"]["relation"],
            "strict_maximum"
        );
        assert_eq!(
            c.result["details"]["numeric_acceptance"]["upper_inclusive"],
            false
        );
        assert_eq!(
            c.result["details"]["numeric_acceptance"]["requirement_protocol"],
            "oev2.numeric-acceptance/2"
        );
        assert_eq!(c.result["engineering_adoption"], "not_established");
    }
    let (mut p, o, mut b) = fixture();
    observed(&mut b, "absolute", json!([q(0.03, "mm")]), q(0.0, "mm"));
    set(&mut p, &b);
    assert_eq!(numeric(&p, &o).conclusion, "pass");
    let r: oe_contracts::NumericAcceptance =
        serde_json::from_value(b["assertions"][1]["requirements"]["requirement"].clone()).unwrap();
    assert_eq!(
        serde_json::to_value(r).unwrap(),
        b["assertions"][1]["requirements"]["requirement"]
    );
    b["assertions"][1]["requirements"]["requirement"]["relation"] = json!("strict_maximum");
    update_req(&mut b);
    set(&mut p, &b);
    assert_eq!(numeric(&p, &o).conclusion, "unknown");
    assert!(numeric(&p, &o).failures[0].contains("NumericAcceptance.relation"));
}

#[test]
fn strict_interval_boundary_is_unknown_until_complete_coverage() {
    for (value, u, expected) in [
        (0.02, 0.005, "pass"),
        (0.02, 0.01, "unknown"),
        (0.03, 0.001, "unknown"),
        (0.04, 0.01, "violated"),
    ] {
        let (p, o, _) = strict_observed(value, u);
        assert_eq!(numeric(&p, &o).conclusion, expected, "value={value},u={u}");
    }
    for (field, value) in [
        ("coverage_status", "unknown"),
        ("coverage_status", "partial"),
        ("definition_status", "draft"),
    ] {
        let (mut p, o, mut b) = strict_observed(0.01, 0.001);
        b["assertions"][1]["requirements"]["requirement"][field] = json!(value);
        update_req(&mut b);
        set(&mut p, &b);
        let c = numeric(&p, &o);
        assert_eq!(c.conclusion, "unknown");
        assert_eq!(
            c.result["details"]["numeric_acceptance"]["conditional_comparison"],
            "pass"
        );
    }
}

#[test]
fn strict_distance_requires_current_both_endpoints_and_sources() {
    for (kind, expected) in [
        ("reference_revision", "numeric_reference_not_current"),
        ("reference_source", "numeric_reference_not_current"),
        (
            "reference_scope",
            "numeric_reference_condition_scope_mismatch",
        ),
        (
            "observation_reference",
            "numeric_observation_reference_mismatch",
        ),
        ("missing_reference", "numeric_assessment_v2_invalid"),
        (
            "reference_source_version",
            "numeric_source_version_missing:reference-fixture",
        ),
        ("observation_unit", "numeric_unit_mismatch"),
        ("uncertainty", "numeric_uncertainty_missing"),
    ] {
        let (mut p, o, mut b) = strict_observed(0.01, 0.001);
        let a = &mut b["assertions"][1]["requirements"];
        match kind {
            "reference_revision" => {
                p.objects.insert("lens-A".into(), "lens-r2".into());
            }
            "reference_source" => {
                p.evidence
                    .insert("reference-fixture".into(), "c".repeat(64));
            }
            "reference_scope" => {
                a["requirement"]["reference_object"]["condition_scope"] = json!("other-state");
            }
            "observation_reference" => {
                a["observation"]["reference_object"]["object_id"] = json!("lens-B");
            }
            "missing_reference" => {
                a["requirement"]
                    .as_object_mut()
                    .unwrap()
                    .remove("reference_object");
            }
            "reference_source_version" => {
                a["source_versions"]
                    .as_object_mut()
                    .unwrap()
                    .remove("reference-fixture");
            }
            "observation_unit" => {
                a["observation"]["values"][0]["unit"] = json!("m");
            }
            "uncertainty" => {
                a["observation"]["expanded_uncertainty"] = Value::Null;
            }
            _ => unreachable!(),
        }
        update_req(&mut b);
        set(&mut p, &b);
        let c = numeric(&p, &o);
        assert_eq!(c.conclusion, "unknown", "{kind}");
        assert!(c.failures[0].contains(expected), "{kind}: {:?}", c.failures);
    }
    let (mut p, o, mut b) = strict_observed(0.01, 0.0);
    let a = &mut b["assertions"][1]["requirements"];
    a["requirement"]["metric"] = json!("radial_2d");
    a["observation"]["metric"] = json!("radial_2d");
    a["observation"]["values"] = json!([q(0.025, "mm"), q(0.025, "mm")]);
    a["observation"]["component_names"] = json!(["horizontal", "vertical"]);
    update_req(&mut b);
    set(&mut p, &b);
    assert_eq!(numeric(&p, &o).conclusion, "violated");
}

#[test]
fn strict_distance_keeps_each_crosshair_independent() {
    // Independent arithmetic: (0.01+0.04)/2=0.025, below0.03. Each is checked.
    let (mut p1, o1, _) = strict_observed(0.01, 0.0);
    let (mut p2, mut o2, mut b2) = strict_observed(0.04, 0.0);
    o2.object_id = "mark-B".into();
    o2.condition_scope = "loaded-B".into();
    p2.objects.remove("guide");
    p2.objects.insert("mark-B".into(), "r1".into());
    b2["object_id"] = json!("mark-B");
    b2["assertions"][1]["requirements"]["requirement"]["object"] = json!(o2);
    b2["assertions"][1]["requirements"]["observation"]["object"] = json!(o2);
    b2["assertions"][1]["requirements"]["requirement"]["reference_object"]["condition_scope"] =
        json!("loaded-B");
    b2["assertions"][1]["requirements"]["observation"]["reference_object"]["condition_scope"] =
        json!("loaded-B");
    for d in &mut p2.decisions {
        d.id = format!("{}-B", d.id);
        d.object_id = "mark-B".into();
        d.condition_scope = "loaded-B".into();
        for id in &mut d.basis {
            if id == "route" {
                *id = "route-B".into();
            }
        }
    }
    b2["assertions"][1]["affected_decisions"] = json!(["route-B"]);
    update_req(&mut b2);
    set(&mut p2, &b2);
    p1.objects.extend(p2.objects);
    p1.decisions.extend(p2.decisions);
    p1.conditions
        .insert("loaded-B".into(), serde_json::to_string(&b2).unwrap());
    p1.dependencies.insert(
        "inspection-B".into(),
        ["route-B".into()].into_iter().collect(),
    );
    assert_eq!(numeric(&p1, &o1).conclusion, "pass");
    assert_eq!(numeric(&p1, &o2).conclusion, "violated");
    assert_eq!(
        numeric(&p1, &o2).result["details"]["numeric_acceptance"]["aggregation"],
        "this_object_only; every other object requires its own check"
    );
}
