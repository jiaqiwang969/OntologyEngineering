use semantica_json::python_set::{PythonSet, SetContext, SetError, python_hash};
use serde_json::{Value, json};
fn exact(a: &Value, b: &Value, name: &str) {
    assert_eq!(
        serde_json::to_string(a).unwrap(),
        serde_json::to_string(b).unwrap(),
        "{name}"
    );
}
#[test]
fn pinned_hashes_and_ordered_sets_across_seeds() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/python-set-original.json")).unwrap();
    for observation in fixture["observed"].as_array().unwrap() {
        let seed = observation["seed"].as_u64().unwrap() as u32;
        for (i, key) in fixture["inputs"]["scalars"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            exact(
                &json!(python_hash(key, seed).unwrap()),
                &observation["hashes"][i],
                &format!("hash {seed} {key}"),
            );
        }
        for (i, case) in fixture["inputs"]["sets"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let ctx = SetContext::new(seed, 8_000_000);
            let expected = &observation["sets"][i];
            let left = PythonSet::from_values(&ctx, case["left"].as_array().unwrap().clone());
            if let Some(err) = expected.get("error") {
                assert_eq!(
                    left.unwrap_err().to_string(),
                    err["message"].as_str().unwrap()
                );
                continue;
            }
            let left = left.unwrap();
            let right =
                PythonSet::from_values(&ctx, case["right"].as_array().unwrap().clone()).unwrap();
            for (name, actual) in [
                ("left", left.values()),
                ("right", right.values()),
                ("copy", left.copy().unwrap().values()),
                ("union", left.union(&right).unwrap().values()),
                ("intersection", left.intersection(&right).unwrap().values()),
                ("difference", left.difference(&right).unwrap().values()),
                (
                    "reverse_difference",
                    right.difference(&left).unwrap().values(),
                ),
            ] {
                exact(
                    &json!(actual),
                    &expected[name],
                    &format!("seed={seed} {} {name}", case["id"]),
                );
            }
        }
    }
}
#[test]
fn explicit_budget_and_context_reject_instead_of_returning_partial_results() {
    let ctx = SetContext::new(0, 1);
    assert_eq!(
        PythonSet::from_values(&ctx, [json!(1)]).unwrap_err(),
        SetError::ResourceLimit
    );
    let a = PythonSet::empty(&SetContext::new(0, 100));
    let b = PythonSet::empty(&SetContext::new(1, 100));
    assert_eq!(a.union(&b).unwrap_err(), SetError::DifferentContext);
    assert_eq!(
        python_hash(&json!({}), 0).unwrap_err(),
        SetError::Unhashable("dict")
    );
}

#[test]
fn same_set_operand_uses_source_copy_short_circuit() {
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/python-set-self12.json")).unwrap();
    let ctx = SetContext::new(0, 10000);
    let a = PythonSet::from_values(&ctx, (0..12).map(|i| json!(format!("key-{i}")))).unwrap();
    exact(
        &json!(a.union(&a).unwrap().values()),
        &expected["union"],
        "same union",
    );
    exact(
        &json!(a.intersection(&a).unwrap().values()),
        &expected["intersection"],
        "same intersection",
    );
}
