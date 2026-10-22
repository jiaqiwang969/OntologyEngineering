//! The `semantica.utils` export surface (`__init__.py`): every name in `__all__` is reachable
//! from the crate root and behaves as the original (expected values from the oracle).

mod common;

use std::sync::Mutex;

use common::{obj, oracle};
use indexmap::IndexMap;
use serde_json::{Value, json};

static LOGGING: Mutex<()> = Mutex::new(());

/// `semantica.utils.__all__`, in order.
const EXPORTS: &[&str] = &[
    "setup_logging",
    "get_logger",
    "log_performance",
    "log_error",
    "log_data_quality",
    "log_execution_time",
    "SemanticaError",
    "ValidationError",
    "TemporalValidationError",
    "ProcessingError",
    "ConfigurationError",
    "QualityError",
    "handle_exception",
    "format_exception",
    "validate_data",
    "validate_config",
    "validate_schema",
    "validate_types",
    "validate_required_fields",
    "validate_string_constraints",
    "validate_numeric_constraints",
    "validate_list_constraints",
    "validate_entity",
    "validate_relationship",
    "validate_file_path",
    "validate_url",
    "validate_email",
    "format_data",
    "clean_text",
    "normalize_entities",
    "normalize_graph_payload",
    "hash_data",
    "safe_filename",
    "ensure_directory",
    "read_json_file",
    "write_json_file",
    "get_file_size",
    "format_timestamp",
    "parse_timestamp",
    "merge_dicts",
    "chunk_list",
    "flatten_dict",
    "get_nested_value",
    "set_nested_value",
    "retry_on_error",
    "safe_import",
    "SUPPORTED_DOCUMENT_FORMATS",
    "SUPPORTED_IMAGE_FORMATS",
    "SUPPORTED_AUDIO_FORMATS",
    "SUPPORTED_VIDEO_FORMATS",
    "SUPPORTED_RDF_FORMATS",
    "SUPPORTED_VECTOR_STORES",
    "SUPPORTED_GRAPH_DBS",
    "DEFAULT_CONFIG",
    "ERROR_CODES",
    "PERFORMANCE_THRESHOLDS",
    "QUALITY_LEVELS",
    "ENTITY_TYPES",
    "RELATIONSHIP_TYPES",
    "PROCESSING_STATUS",
    "DATA_TYPES",
    "API_ENDPOINTS",
    "FILE_SIZE_LIMITS",
    "RETRY_CONFIG",
    "CACHE_CONFIG",
    "JSONType",
    "EntityDict",
    "RelationshipDict",
    "ConfigDict",
    "DataDict",
    "EntityType",
    "DataType",
    "ProcessingStatus",
    "QualityLevel",
    "RelationshipType",
    "Entity",
    "Relationship",
    "ProcessingResult",
    "QualityMetrics",
    "Result",
    "BatchResult",
    "is_entity_dict",
    "is_relationship_dict",
    "is_processing_result",
    "entity_to_dict",
    "dict_to_entity",
    "relationship_to_dict",
    "dict_to_relationship",
    "ProgressTracker",
    "ProgressDisplay",
    "ConsoleProgressDisplay",
    "JupyterProgressDisplay",
    "FileProgressDisplay",
    "ProgressItem",
    "ModuleDetector",
    "get_progress_tracker",
    "track_progress",
];

#[test]
fn test_export_list_matches_oracle() {
    assert_eq!(json!(EXPORTS), *oracle("exports"));
}

#[test]
fn test_logging_exports() {
    let _guard = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let kwargs = obj(&json!({"file": "", "console": false, "level": "WARNING"}));
    let root = sem_utils::setup_logging(None, &kwargs).unwrap();
    assert_eq!(root.name(), "semantica");
    assert!(root.handlers().is_empty());
    let logger = sem_utils::get_logger("init_test");
    assert_eq!(logger.name(), "semantica.init_test");
    sem_utils::log_performance("f", 0.5, &obj(&json!({"cache_hit": true}))).unwrap();
    let err: sem_utils::SemanticaError = sem_utils::ValidationError::new("bad").into();
    sem_utils::log_error(&err, None, &serde_json::Map::new()).unwrap();
    sem_utils::log_data_quality(
        &obj(&json!({"score": 0.9})),
        &sem_utils::logging::DataQualityOptions::default(),
    )
    .unwrap();
    let value =
        sem_utils::log_execution_time("g", || Ok::<_, sem_utils::SemanticaError>(3)).unwrap();
    assert_eq!(value, 3);
    root.set_level(sem_utils::logging::LogLevel::NOTSET);
    root.set_propagate(true);
}

#[test]
fn test_exception_exports() {
    use sem_utils::{
        ConfigurationError, ProcessingError, QualityError, SemanticaError, TemporalValidationError,
        ValidationError, format_exception, handle_exception,
    };
    let errors: Vec<SemanticaError> = vec![
        SemanticaError::new("base"),
        ValidationError::new("v").into(),
        TemporalValidationError::new("t").into(),
        ProcessingError::new("p").into(),
        ConfigurationError::new("c").into(),
        QualityError::new("q").into(),
    ];
    let codes: Vec<&str> = errors.iter().map(|e| e.error_code().unwrap()).collect();
    assert_eq!(
        codes,
        ["SEM000", "SEM001", "SEM001T", "SEM002", "SEM003", "SEM004"]
    );
    let case = &oracle("exceptions")[0];
    assert_eq!(
        format_exception(&errors[0], &Default::default()),
        case["format_default"].as_str().unwrap()
    );
    let handled = handle_exception(
        &errors[0],
        Some(&obj(&json!({"caller": "x"}))),
        &obj(&json!({"log_error": false})),
    )
    .unwrap();
    assert_eq!(Value::Object(handled), case["handle"]);
}

#[test]
fn test_validator_exports() {
    use sem_utils::validators::{ConfigOptions, Constraints, PyType};
    let entity = obj(&json!({"id": "e1", "text": "Alice", "type": "Person"}));
    assert_eq!(sem_utils::validate_entity(&entity), (true, None));
    let rel = obj(&json!({"id": "r", "source_id": "a"}));
    assert_eq!(
        sem_utils::validate_relationship(&rel),
        (
            false,
            Some("Missing required fields: target_id, type".into())
        )
    );
    assert!(sem_utils::validate_url("https://example.com").0);
    assert!(sem_utils::validate_email("test@example.com").0);
    assert!(sem_utils::validate_file_path("data.json", None, None).0);
    let c = Constraints::default();
    assert_eq!(
        sem_utils::validate_data(&json!(1), None, &c).unwrap(),
        (true, None)
    );
    assert_eq!(
        sem_utils::validate_config(&json!({"a": 1}), Some(&["a"]), &ConfigOptions::default()),
        (true, None)
    );
    assert_eq!(
        sem_utils::validate_schema(&json!("x"), &obj(&json!({"type": "string"}))).unwrap(),
        (true, None)
    );
    assert_eq!(
        sem_utils::validate_types(
            &obj(&json!({"a": 1})),
            &IndexMap::from([("a".to_string(), PyType::Int)])
        ),
        (true, None)
    );
    assert_eq!(
        sem_utils::validate_required_fields(&obj(&json!({"a": 1})), &["a"]),
        (true, None)
    );
    assert_eq!(
        sem_utils::validate_string_constraints("x", &c).unwrap(),
        (true, None)
    );
    assert_eq!(
        sem_utils::validate_numeric_constraints(&json!(1), &c).unwrap(),
        (true, None)
    );
    assert_eq!(
        sem_utils::validate_list_constraints(&[json!(1)], &c).unwrap(),
        (true, None)
    );
}

#[test]
fn test_helper_exports() {
    let _guard = LOGGING.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    let case = &oracle("format_data")[0];
    assert_eq!(
        sem_utils::format_data(&case["data"], Some("json")).unwrap(),
        case["ok"].as_str().unwrap()
    );
    assert_eq!(sem_utils::clean_text("  a  b ", None), "a b");
    let normalized = sem_utils::normalize_entities(&[obj(&json!({"entity_id": "e"}))]);
    assert_eq!(normalized[0]["id"], json!("e"));
    let payload = sem_utils::normalize_graph_payload(&json!({"nodes": [{"id": "n1"}]})).unwrap();
    assert_eq!(payload.entities.len(), 1);
    let hash = &oracle("hash_data")[0];
    assert_eq!(
        sem_utils::hash_data("hello"),
        hash["expected"].as_str().unwrap()
    );
    assert_eq!(sem_utils::safe_filename("my file.txt", None), "my_file.txt");
    let dir = sem_utils::ensure_directory(tmp.path().join("d")).unwrap();
    let file = dir.join("x.json");
    sem_utils::write_json_file(&json!({"a": 1}), &file, None, None).unwrap();
    assert_eq!(sem_utils::read_json_file(&file).unwrap(), json!({"a": 1}));
    assert_eq!(sem_utils::get_file_size(&file).unwrap(), 12);
    let ts = sem_utils::format_timestamp(Some(0i64.into()), None, None).unwrap();
    assert_eq!(ts, "1970-01-01 00:00:00");
    assert_eq!(
        sem_utils::parse_timestamp(&ts, None).unwrap().to_string(),
        "1970-01-01 00:00:00"
    );
    let a = obj(&json!({"a": {"x": 1}}));
    let b = obj(&json!({"a": {"y": 2}}));
    assert_eq!(
        Value::Object(sem_utils::merge_dicts(&[&a, &b], None)),
        json!({"a": {"x": 1, "y": 2}})
    );
    assert_eq!(
        sem_utils::chunk_list(&[1, 2, 3], 2).unwrap(),
        vec![vec![1, 2], vec![3]]
    );
    assert_eq!(
        Value::Object(sem_utils::flatten_dict(&a, None, None).unwrap()),
        json!({"a.x": 1})
    );
    assert_eq!(
        sem_utils::get_nested_value(&a, "a.x", None, None).unwrap(),
        json!(1)
    );
    let mut c = obj(&json!({}));
    sem_utils::set_nested_value(&mut c, "p.q", json!(true), None).unwrap();
    assert_eq!(Value::Object(c), json!({"p": {"q": true}}));
    let policy = sem_utils::retry_on_error(Some(0), Some(0.0), None);
    assert_eq!(policy.call(|| Ok::<_, ()>(5)), Ok(5));
    assert!(sem_utils::safe_import("json", None, None, None).1);
}

#[test]
fn test_constant_exports() {
    let constants = oracle("constants");
    assert_eq!(
        json!(sem_utils::SUPPORTED_DOCUMENT_FORMATS),
        constants["SUPPORTED_DOCUMENT_FORMATS"]
    );
    assert_eq!(
        json!(sem_utils::SUPPORTED_IMAGE_FORMATS),
        constants["SUPPORTED_IMAGE_FORMATS"]
    );
    assert_eq!(
        json!(sem_utils::SUPPORTED_AUDIO_FORMATS),
        constants["SUPPORTED_AUDIO_FORMATS"]
    );
    assert_eq!(
        json!(sem_utils::SUPPORTED_VIDEO_FORMATS),
        constants["SUPPORTED_VIDEO_FORMATS"]
    );
    assert_eq!(
        json!(sem_utils::SUPPORTED_RDF_FORMATS),
        constants["SUPPORTED_RDF_FORMATS"]
    );
    assert_eq!(
        json!(sem_utils::SUPPORTED_VECTOR_STORES),
        constants["SUPPORTED_VECTOR_STORES"]
    );
    assert_eq!(
        json!(sem_utils::SUPPORTED_GRAPH_DBS),
        constants["SUPPORTED_GRAPH_DBS"]
    );
    assert_eq!(
        Value::Object(sem_utils::DEFAULT_CONFIG.clone()),
        constants["DEFAULT_CONFIG"]
    );
    assert_eq!(json!(*sem_utils::ERROR_CODES), constants["ERROR_CODES"]);
    assert_eq!(
        Value::Object(sem_utils::PERFORMANCE_THRESHOLDS.clone()),
        constants["PERFORMANCE_THRESHOLDS"]
    );
    assert_eq!(
        json!(*sem_utils::QUALITY_LEVELS),
        constants["QUALITY_LEVELS"]
    );
    assert_eq!(json!(sem_utils::ENTITY_TYPES), constants["ENTITY_TYPES"]);
    assert_eq!(
        json!(sem_utils::RELATIONSHIP_TYPES),
        constants["RELATIONSHIP_TYPES"]
    );
    assert_eq!(
        json!(*sem_utils::PROCESSING_STATUS),
        constants["PROCESSING_STATUS"]
    );
    assert_eq!(json!(*sem_utils::DATA_TYPES), constants["DATA_TYPES"]);
    assert_eq!(
        Value::Object(sem_utils::API_ENDPOINTS.clone()),
        constants["API_ENDPOINTS"]
    );
    assert_eq!(
        json!(*sem_utils::FILE_SIZE_LIMITS),
        constants["FILE_SIZE_LIMITS"]
    );
    assert_eq!(json!(*sem_utils::RETRY_CONFIG), constants["RETRY_CONFIG"]);
    assert_eq!(
        Value::Object(sem_utils::CACHE_CONFIG.clone()),
        constants["CACHE_CONFIG"]
    );
}

#[test]
fn test_type_exports() {
    use sem_utils::{
        BatchResult, ConfigDict, DataDict, DataType, Entity, EntityDict, EntityType, JSONType,
        ProcessingResult, ProcessingStatus, QualityLevel, QualityMetrics, Relationship,
        RelationshipDict, RelationshipType, Result,
    };
    let json_value: JSONType = json!([1, "a"]);
    let entity_dict: EntityDict = obj(&json!({"id": "e", "text": "t", "type": "PERSON"}));
    let relationship_dict: RelationshipDict =
        obj(&json!({"id": "r", "source_id": "a", "target_id": "b", "type": "REL"}));
    let config: ConfigDict = obj(&json!({"k": 1}));
    let data: DataDict = obj(&json!({"d": 2}));
    assert!(json_value.is_array() && !config.is_empty() && !data.is_empty());
    assert_eq!(EntityType::Person.value(), "PERSON");
    assert_eq!(DataType::Text.to_string(), "DataType.TEXT");
    assert_eq!(ProcessingStatus::Paused.value(), "paused");
    assert_eq!(QualityLevel::Poor.name(), "POOR");
    assert_eq!(RelationshipType::SameAs.value(), "SAME_AS");
    let entity: Entity = sem_utils::dict_to_entity(&entity_dict).unwrap();
    assert_eq!(sem_utils::entity_to_dict(&entity)["confidence"], json!(1.0));
    let rel: Relationship = sem_utils::dict_to_relationship(&relationship_dict).unwrap();
    assert_eq!(sem_utils::relationship_to_dict(&rel)["type"], json!("REL"));
    assert!(sem_utils::is_entity_dict(&Value::Object(entity_dict)));
    assert!(sem_utils::is_relationship_dict(&Value::Object(
        relationship_dict
    )));
    assert!(sem_utils::is_processing_result(
        &json!({"status": "failed"})
    ));
    let result = ProcessingResult::new(ProcessingStatus::Failed);
    assert_eq!(result.error, None);
    assert_eq!(QualityMetrics::new(0.5).score, 0.5);
    let batch = BatchResult::new(vec![Result::ok(1), Result::err("e")]);
    assert_eq!(batch.success_rate(), 0.5);
}

#[test]
fn test_progress_exports() {
    use sem_utils::{
        ConsoleProgressDisplay, FileProgressDisplay, JupyterProgressDisplay, ModuleDetector,
        ProgressDisplay, ProgressItem, ProgressTracker,
    };
    use std::sync::Arc;
    let tracker = ProgressTracker::new(true, false, 0.0);
    let displays: Vec<Arc<dyn ProgressDisplay>> = vec![
        Arc::new(ConsoleProgressDisplay::with_writer(
            false,
            0.0,
            Box::new(std::io::sink()),
        )),
        Arc::new(JupyterProgressDisplay::new(false)),
        Arc::new(FileProgressDisplay::new(None)),
    ];
    tracker.set_displays(displays);
    let id = tracker.start_tracking(None, Some("ingest"), Some("I"), "", None);
    tracker.stop_tracking(&id, None, "", None);
    let items: Vec<ProgressItem> = tracker.items();
    assert_eq!(items[0].status, "completed");
    assert_eq!(
        ModuleDetector::detect_from_call_stack(0).0.as_deref(),
        Some("utils")
    );
    let global = sem_utils::get_progress_tracker();
    assert!(Arc::ptr_eq(&global, &sem_utils::get_progress_tracker()));
    let decorator = sem_utils::track_progress(Some("f.txt"), Some("m"), Some("S"));
    assert_eq!(decorator.file.as_deref(), Some("f.txt"));
}
