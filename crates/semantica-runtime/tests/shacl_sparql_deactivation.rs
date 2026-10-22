//! Independent W3C 2017 activation expectations, frozen before the runtime repair.
//! Other checks on the same shape must still produce their original findings.
use semantica_runtime::shacl;
use serde_json::Value;

#[test]
fn constraint_activation_preserves_active_checks_and_parameter_rejections() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/shacl-sparql-deactivation.json")).unwrap();
    let mut failures = Vec::new();
    for case in fixtures["cases"].as_array().unwrap() {
        let expected = &case["expected"];
        let result = shacl::validate(
            case["data"].as_str().unwrap().as_bytes(),
            case["shapes"].as_str().unwrap().as_bytes(),
        );
        let matches = if expected.get("error").is_some() {
            match &result {
                Err(error) => expected["error"]
                    .as_str()
                    .is_none_or(|message| error.to_string() == message),
                Ok(_) => false,
            }
        } else {
            match &result {
                Ok(report) => {
                    report["conforms"] == expected["conforms"]
                        && report["findings"] == expected["findings"]
                        && report["violation_count"]
                            == expected["findings"].as_array().unwrap().len()
                }
                Err(_) => false,
            }
        };
        if !matches {
            failures.push(format!(
                "{}: expected {expected}, actual {result:?}",
                case["id"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
