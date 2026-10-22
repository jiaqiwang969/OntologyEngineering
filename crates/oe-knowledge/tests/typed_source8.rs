use oe_knowledge::typed_source8::parse_input8;
use serde_json::json;
#[test]
fn caller_report_and_completed_json_cannot_create_source_authority() {
    for value in [
        json!({"schema":"oev2.source-bound-typed-extraction-input/8","producer":{"completed":true}}),
        json!({"schema":"oev2.source-bound-typed-extraction-input/8","channel":"ocr","report":{"passed":true,"model_execution":"completed"}}),
    ] {
        assert!(
            parse_input8(&value)
                .unwrap_err()
                .to_string()
                .contains("typed_source8_input_keys")
        );
    }
}
