//! Constants and default configuration (`semantica/utils/constants.py`).
//!
//! Lists become `&[&str]` constants; dictionaries with uniform value types become ordered
//! [`IndexMap`]s (key order as in the original); dictionaries mixing value types stay
//! [`serde_json::Map`]s so integers and floats keep their Python identity.

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use serde_json::{Map, Value, json};

/// Supported document formats.
pub const SUPPORTED_DOCUMENT_FORMATS: &[&str] = &[
    "pdf", "docx", "doc", "txt", "html", "xml", "json", "csv", "xlsx", "pptx", "parquet", "pq",
    "arrow", "feather", "ipc",
];

/// Supported image formats.
pub const SUPPORTED_IMAGE_FORMATS: &[&str] =
    &["jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp", "svg"];

/// Supported audio formats.
pub const SUPPORTED_AUDIO_FORMATS: &[&str] = &["mp3", "wav", "flac", "aac", "ogg", "wma", "m4a"];

/// Supported video formats.
pub const SUPPORTED_VIDEO_FORMATS: &[&str] = &["mp4", "avi", "mov", "wmv", "flv", "webm", "mkv"];

/// Supported RDF serialization formats.
pub const SUPPORTED_RDF_FORMATS: &[&str] = &["turtle", "rdfxml", "jsonld", "n3", "ntriples"];

/// Supported vector store backends.
pub const SUPPORTED_VECTOR_STORES: &[&str] =
    &["faiss", "weaviate", "qdrant", "milvus", "chroma", "sqlite"];

/// Supported graph database backends.
pub const SUPPORTED_GRAPH_DBS: &[&str] = &[
    "neo4j",
    "blazegraph",
    "apache_jena",
    "allegrograph",
    "amazon_neptune",
];

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => unreachable!("constant literal is an object"),
    }
}

/// Default framework configuration (`logging`, `processing`, `quality`, `security`).
pub static DEFAULT_CONFIG: Lazy<Map<String, Value>> = Lazy::new(|| {
    object(json!({
        "logging": {
            "level": "INFO",
            "format": "%(asctime)s - %(name)s - %(levelname)s - %(message)s",
            "file": "semantica.log",
        },
        "processing": {"batch_size": 100, "max_workers": 4, "timeout": 300},
        "quality": {"min_confidence": 0.7, "max_errors": 10, "validation_enabled": true},
        "security": {
            "encryption_enabled": false,
            "access_control_enabled": true,
            "audit_logging_enabled": true,
        },
    }))
});

/// Error code per error kind (`VALIDATION_ERROR` -> `SEM001`, ...).
pub static ERROR_CODES: Lazy<IndexMap<&'static str, &'static str>> = Lazy::new(|| {
    IndexMap::from([
        ("VALIDATION_ERROR", "SEM001"),
        ("PROCESSING_ERROR", "SEM002"),
        ("CONFIGURATION_ERROR", "SEM003"),
        ("QUALITY_ERROR", "SEM004"),
        ("SECURITY_ERROR", "SEM005"),
        ("NETWORK_ERROR", "SEM006"),
        ("STORAGE_ERROR", "SEM007"),
        ("PERMISSION_ERROR", "SEM008"),
    ])
});

/// Performance limits (integers except `min_quality_score`).
pub static PERFORMANCE_THRESHOLDS: Lazy<Map<String, Value>> = Lazy::new(|| {
    object(json!({
        "max_processing_time": 3600,
        "max_memory_usage": 8192,
        "max_file_size": 104857600,
        "max_batch_size": 1000,
        "min_quality_score": 0.7,
    }))
});

/// Quality score thresholds.
pub static QUALITY_LEVELS: Lazy<IndexMap<&'static str, f64>> =
    Lazy::new(|| IndexMap::from([("HIGH", 0.9), ("MEDIUM", 0.7), ("LOW", 0.5), ("POOR", 0.3)]));

/// Entity type names.
pub const ENTITY_TYPES: &[&str] = &[
    "PERSON",
    "ORGANIZATION",
    "LOCATION",
    "EVENT",
    "PRODUCT",
    "CONCEPT",
    "DATE",
    "TIME",
    "MONEY",
    "PERCENT",
    "QUANTITY",
    "ORDINAL",
    "CARDINAL",
];

/// Relationship type names.
pub const RELATIONSHIP_TYPES: &[&str] = &[
    "WORKS_FOR",
    "LOCATED_IN",
    "PART_OF",
    "RELATED_TO",
    "CAUSES",
    "AFFECTS",
    "BEFORE",
    "AFTER",
    "DURING",
    "SAME_AS",
    "DIFFERENT_FROM",
    "SIMILAR_TO",
];

/// Processing status values.
pub static PROCESSING_STATUS: Lazy<IndexMap<&'static str, &'static str>> = Lazy::new(|| {
    IndexMap::from([
        ("PENDING", "pending"),
        ("PROCESSING", "processing"),
        ("COMPLETED", "completed"),
        ("FAILED", "failed"),
        ("CANCELLED", "cancelled"),
    ])
});

/// Data type values.
pub static DATA_TYPES: Lazy<IndexMap<&'static str, &'static str>> = Lazy::new(|| {
    IndexMap::from([
        ("TEXT", "text"),
        ("IMAGE", "image"),
        ("AUDIO", "audio"),
        ("VIDEO", "video"),
        ("DOCUMENT", "document"),
        ("STRUCTURED", "structured"),
        ("UNSTRUCTURED", "unstructured"),
    ])
});

/// API endpoint configuration (`BASE_URL`, `VERSION`, nested `ENDPOINTS`).
pub static API_ENDPOINTS: Lazy<Map<String, Value>> = Lazy::new(|| {
    object(json!({
        "BASE_URL": "https://api.semantica.dev",
        "VERSION": "v1",
        "ENDPOINTS": {
            "PROCESS": "/process",
            "EXTRACT": "/extract",
            "ANALYZE": "/analyze",
            "EXPORT": "/export",
            "STATUS": "/status",
        },
    }))
});

/// File size limits in bytes (and `MAX_BATCH_SIZE` in items).
pub static FILE_SIZE_LIMITS: Lazy<IndexMap<&'static str, u64>> = Lazy::new(|| {
    IndexMap::from([
        ("MAX_DOCUMENT_SIZE", 104857600),
        ("MAX_IMAGE_SIZE", 52428800),
        ("MAX_AUDIO_SIZE", 104857600),
        ("MAX_VIDEO_SIZE", 524288000),
        ("MAX_BATCH_SIZE", 1000),
    ])
});

/// Retry configuration (delays in seconds).
pub static RETRY_CONFIG: Lazy<IndexMap<&'static str, u64>> = Lazy::new(|| {
    IndexMap::from([
        ("MAX_RETRIES", 3),
        ("RETRY_DELAY", 1),
        ("BACKOFF_FACTOR", 2),
        ("MAX_DELAY", 60),
    ])
});

/// Cache configuration (`ENABLED` is a bool, the rest are integers).
pub static CACHE_CONFIG: Lazy<Map<String, Value>> = Lazy::new(|| {
    object(json!({
        "ENABLED": true,
        "TTL": 3600,
        "MAX_SIZE": 1000,
        "CLEANUP_INTERVAL": 300,
    }))
});
