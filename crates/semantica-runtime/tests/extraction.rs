use semantica_contracts::{SemanticExtractionPort, SemanticExtractionRequest};
use semantica_runtime::extraction::{RustPatternExtraction, extract_pattern};
use serde_json::Value;
#[test]
fn original_ner_fallback_five_fixed_outputs() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/original-ner-fallback-oracle.json")).unwrap();
    for case in cases["cases"].as_array().unwrap() {
        assert_eq!(
            extract_pattern(case["text"].as_str().unwrap(), "entities").unwrap()["entities"],
            case["expected"],
            "{}",
            case["text"]
        );
    }
}
#[test]
fn original_direct_pattern_relations_four_fixed_outputs() {
    let cases: Value = serde_json::from_str(include_str!(
        "fixtures/original-pattern-relations-oracle.json"
    ))
    .unwrap();
    for case in cases["cases"].as_array().unwrap() {
        let result = extract_pattern(case["text"].as_str().unwrap(), "relations").unwrap();
        assert_eq!(
            result["relations"], case["expected_relations"],
            "{}",
            case["text"]
        );
        assert_eq!(result["triplets"], case["expected_triplets"]);
    }
}
#[test]
fn extraction_port_states_the_actual_model_condition_and_fails_without_partial_result() {
    let request = SemanticExtractionRequest {
        schema: "semantica.extraction-request/1".into(),
        operation: "entities".into(),
        text: "Alice Wang".into(),
    };
    let output = RustPatternExtraction.extract(&request).unwrap();
    assert_eq!(output.model_execution, "not_run");
    assert!(!output.uncovered.is_empty());
    let over = SemanticExtractionRequest {
        text: "x".repeat(256 * 1024 + 1),
        ..request
    };
    let error = RustPatternExtraction.extract(&over).unwrap_err();
    assert_eq!(error.code, "extraction_failed");
    assert!(!error.message.contains(&"x".repeat(32)));
}
