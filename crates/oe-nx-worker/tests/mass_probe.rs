use oe_contracts::NxGeometricalMassObservation1;
use oe_nx_worker::geometry::{Parameters, mass_probe};
use serde_json::json;

fn observation() -> NxGeometricalMassObservation1 {
    let mut raw = [0.0; 47];
    raw[1] = 2.3790442631532282e-5;
    let mut accuracy_values = vec![0.; 11];
    accuracy_values[0] = 0.999999;
    NxGeometricalMassObservation1 {
        schema: mass_probe::OBSERVATION_SCHEMA.into(),
        body_tag: 101,
        owning_part_tag: 12,
        analysis_type: 1,
        units: 4,
        density: 0.,
        accuracy_mode: 1,
        accuracy_values,
        return_code: 0,
        raw47_binary64: mass_probe::words(&raw),
        statistics13_binary64: mass_probe::words(&[0.; 13]),
    }
}
#[test]
fn mass_probe_retains_binary64_and_rejects_invalid_measurements() {
    let v = observation();
    assert_eq!(v.raw47_binary64[1], "3ef8f232c4da5913");
    let first = mass_probe::decode(&v).unwrap();
    let mut next = v.clone();
    next.raw47_binary64[1] = "3ef8f232c4da5914".into();
    let serialized = serde_json::to_vec(&next).unwrap();
    let restored: NxGeometricalMassObservation1 = serde_json::from_slice(&serialized).unwrap();
    let second = mass_probe::decode(&restored).unwrap();
    assert_eq!(second.raw47[1].to_bits() - first.raw47[1].to_bits(), 1);
    assert_eq!(first.statistics13, [0.; 13]);
    for bad in [
        "7ff8000000000001",
        "7ff0000000000000",
        "fff0000000000000",
        "000000000000000",
        "3EF8F232C4DA5913",
        "not_binary64_raw",
    ] {
        let mut invalid = v.clone();
        invalid.raw47_binary64[1] = bad.into();
        assert!(mass_probe::decode(&invalid).is_err(), "{bad}");
    }
    for field in [
        "schema",
        "body_tag",
        "owning_part_tag",
        "analysis_type",
        "units",
        "density",
        "accuracy_mode",
        "accuracy_values",
        "return_code",
        "raw47_binary64",
        "statistics13_binary64",
    ] {
        let mut raw = serde_json::to_value(&v).unwrap();
        match field {
            "schema" => raw[field] = json!("other"),
            "body_tag" | "owning_part_tag" => raw[field] = json!(0),
            "analysis_type" | "units" | "accuracy_mode" | "return_code" => raw[field] = json!(7),
            "density" => raw[field] = json!(-0.0),
            _ => {
                raw[field].as_array_mut().unwrap().pop();
            }
        }
        match serde_json::from_value::<NxGeometricalMassObservation1>(raw) {
            Ok(invalid) => assert!(mass_probe::decode(&invalid).is_err(), "{field}"),
            Err(_) => {}
        }
    }
    let mut invalid = v;
    invalid.statistics13_binary64[1] = "7ff0000000000000".into();
    assert!(mass_probe::decode(&invalid).is_err());
}
#[test]
fn diagnostic_actions_require_explicit_schema_and_bounded_repetitions() {
    for (action, schema) in [
        ("probe_assembly_mass_properties", mass_probe::PARAMS_SCHEMA),
        (mass_probe::COPY_ACTION, mass_probe::COPY_PARAMS_SCHEMA),
    ] {
        for n in [0, 1, 2, 4, 5, 1000] {
            let mut value = json!({"action":action,"schema":schema,"budget":{"max_occurrences":16,"max_prototypes":8,"max_depth":4},"repetitions":n});
            if action == mass_probe::COPY_ACTION {
                value["intent"] = json!({"path":"intent.json","sha256":"a".repeat(64)});
            } else {
                value["root_part"] = json!("root.prt");
            }
            let p: Parameters = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(p.validate().is_ok(), (2..=4).contains(&n));
            assert_eq!(p.parameter_schema(), schema);
            value["schema"] = json!("oev2.nx-component-copy-params/1");
            assert!(
                serde_json::from_value::<Parameters>(value)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
    }
    let legacy = json!({"action":"copy_assembly_component","schema":"oev2.nx-component-copy-params/1","intent":{"path":"intent.json","sha256":"a".repeat(64)},"budget":{"max_occurrences":16,"max_prototypes":8,"max_depth":4},"repetitions":2});
    assert!(serde_json::from_value::<Parameters>(legacy).is_err());
}
