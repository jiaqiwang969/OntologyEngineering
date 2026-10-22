use oe_design::json_value_type;
#[test]
fn json_leaf_metadata_is_structural_and_preserves_named_contract_boundaries() {
    for value in [
        "serde_json::Value",
        "Vec<serde_json::Value>",
        "Option<serde_json::Value>",
        "Vec<Option<Vec<serde_json::Value>>>",
    ] {
        assert!(json_value_type(value), "{value}");
    }
    for value in [
        "String",
        "Value",
        "DrawingDimensionValue",
        "Option<DrawingDimensionValue>",
        "Vec<SemanticQueryResult>",
        "serde_json::ValueExtra",
        "Vec<serde_json::Value>junk",
        "std::collections::BTreeMap<String, Vec<u8>>",
    ] {
        assert!(!json_value_type(value), "{value}");
    }
    assert!(!json_value_type(&"Vec<".repeat(1000)));
}
