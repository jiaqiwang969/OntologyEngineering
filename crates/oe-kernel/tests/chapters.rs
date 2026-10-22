use oe_kernel::chapters::{compare_cq_oracle, compare_validation_oracle};
use serde_json::json;

#[test]
fn old_chapter_terms_and_negative_oracles_remain_exact() {
    let expected = json!({"kind":"binding_multiset","variables":["got"],"rows":[{"got":{"value":"95.48","term_type":"literal","datatype":"http://www.w3.org/2001/XMLSchema#decimal","language":null}}]});
    let actual = json!({"columns":["got"],"rows":[{"got":{"kind":"literal","value":"95.48","datatype":"http://www.w3.org/2001/XMLSchema#decimal","language":null}}]});
    assert!(compare_cq_oracle(&actual, &expected).unwrap());
    let mut wrong = actual.clone();
    wrong["rows"][0]["got"]["datatype"] = json!("http://www.w3.org/2001/XMLSchema#string");
    assert!(
        !compare_cq_oracle(&wrong, &expected).unwrap(),
        "equal lexical text cannot erase RDF datatype"
    );
    wrong = actual.clone();
    wrong["rows"]
        .as_array_mut()
        .unwrap()
        .push(actual["rows"][0].clone());
    assert!(
        !compare_cq_oracle(&wrong, &expected).unwrap(),
        "the oracle is an exact multiset"
    );
    let negative =
        json!({"execution":[],"conforms":false,"fault_count":1,"message_contains":"got < need"});
    let observed =
        json!({"conforms":false,"violation_count":1,"findings":[{"message":"got < need"}]});
    assert!(compare_validation_oracle(&observed, &negative).unwrap());
    let mut zero = observed.clone();
    zero["violation_count"] = json!(0);
    assert!(!compare_validation_oracle(&zero, &negative).unwrap());
    let mut unavailable = negative.clone();
    unavailable["unimplemented_mechanism"] = json!(true);
    assert!(
        compare_validation_oracle(&observed, &unavailable).is_err(),
        "unsupported expected behavior stays unknown"
    );
}
