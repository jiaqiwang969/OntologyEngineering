//! Characterisation of `semantica/utils/constants.py`: every constant equals the original's
//! value (dumped from the oracle), including key order.

mod common;

use common::oracle;
use indexmap::IndexMap;
use sem_utils::constants::*;
use serde_json::{Map, Value, json};

fn list(items: &[&str]) -> Value {
    json!(items)
}

fn str_map(map: &IndexMap<&'static str, &'static str>) -> Value {
    Value::Object(map.iter().map(|(k, v)| (k.to_string(), json!(v))).collect())
}

fn num_map<T: Into<Value> + Copy>(map: &IndexMap<&'static str, T>) -> Value {
    Value::Object(
        map.iter()
            .map(|(k, v)| (k.to_string(), (*v).into()))
            .collect(),
    )
}

fn object(map: &Map<String, Value>) -> Value {
    Value::Object(map.clone())
}

fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn test_constants_match_oracle() {
    let expected = oracle("constants");
    let actual: Vec<(&str, Value)> = vec![
        (
            "SUPPORTED_DOCUMENT_FORMATS",
            list(SUPPORTED_DOCUMENT_FORMATS),
        ),
        ("SUPPORTED_IMAGE_FORMATS", list(SUPPORTED_IMAGE_FORMATS)),
        ("SUPPORTED_AUDIO_FORMATS", list(SUPPORTED_AUDIO_FORMATS)),
        ("SUPPORTED_VIDEO_FORMATS", list(SUPPORTED_VIDEO_FORMATS)),
        ("SUPPORTED_RDF_FORMATS", list(SUPPORTED_RDF_FORMATS)),
        ("SUPPORTED_VECTOR_STORES", list(SUPPORTED_VECTOR_STORES)),
        ("SUPPORTED_GRAPH_DBS", list(SUPPORTED_GRAPH_DBS)),
        ("DEFAULT_CONFIG", object(&DEFAULT_CONFIG)),
        ("ERROR_CODES", str_map(&ERROR_CODES)),
        ("PERFORMANCE_THRESHOLDS", object(&PERFORMANCE_THRESHOLDS)),
        ("QUALITY_LEVELS", num_map(&QUALITY_LEVELS)),
        ("ENTITY_TYPES", list(ENTITY_TYPES)),
        ("RELATIONSHIP_TYPES", list(RELATIONSHIP_TYPES)),
        ("PROCESSING_STATUS", str_map(&PROCESSING_STATUS)),
        ("DATA_TYPES", str_map(&DATA_TYPES)),
        ("API_ENDPOINTS", object(&API_ENDPOINTS)),
        ("FILE_SIZE_LIMITS", num_map(&FILE_SIZE_LIMITS)),
        ("RETRY_CONFIG", num_map(&RETRY_CONFIG)),
        ("CACHE_CONFIG", object(&CACHE_CONFIG)),
    ];
    assert_eq!(actual.len(), expected.as_object().unwrap().len());
    for (name, value) in actual {
        assert_eq!(value, expected[name], "{name}");
        assert_eq!(keys(&value), keys(&expected[name]), "{name} key order");
    }
    // Typed access, as the original's dict lookups.
    assert_eq!(ERROR_CODES["VALIDATION_ERROR"], "SEM001");
    assert_eq!(PERFORMANCE_THRESHOLDS["max_processing_time"], json!(3600));
    assert_eq!(DEFAULT_CONFIG["processing"]["batch_size"], json!(100));
    assert!(SUPPORTED_DOCUMENT_FORMATS.contains(&"pdf"));
}
