//! Rust port of Semantica's `semantica.utils` package (see `semantica-rs/scope/sem-utils.json`).
//!
//! Shared utilities for every Semantica crate: logging ([`logging`]), the exception
//! hierarchy ([`exceptions`], with [`SemanticaError`] as the common error type), validators,
//! helpers, constants, type definitions, progress tracking and SKOS checks. The crate root
//! re-exports the names `semantica.utils` exports (`__init__.py`), so
//! `from semantica.utils import X` becomes `use sem_utils::X`.
//!
//! [`pycompat`] holds the Python standard-library behaviour the port needs to keep the
//! original's observable output (`json.dumps`, `yaml.dump`, `str()`/`repr()`, `datetime`,
//! `warnings`).
//!
//! Note: [`Result`] is the original generic result container (`semantica.utils.Result`), not
//! `std::result::Result`; avoid glob-importing this crate.

pub mod constants;
pub mod exceptions;
pub mod helpers;
pub mod logging;
pub mod progress_tracker;
pub mod pycompat;
pub mod skos;
pub mod types;
pub mod validators;

// Logging
pub use logging::{
    get_logger, log_data_quality, log_error, log_execution_time, log_performance, setup_logging,
};

// Exceptions
pub use exceptions::{
    ConfigurationError, ProcessingError, QualityError, SemanticaError, TemporalValidationError,
    ValidationError, format_exception, handle_exception,
};

// Validators
pub use validators::{
    validate_config, validate_data, validate_email, validate_entity, validate_file_path,
    validate_list_constraints, validate_numeric_constraints, validate_relationship,
    validate_required_fields, validate_schema, validate_string_constraints, validate_types,
    validate_url,
};

// Helpers
pub use helpers::{
    chunk_list, clean_text, ensure_directory, flatten_dict, format_data, format_timestamp,
    get_file_size, get_nested_value, hash_data, merge_dicts, normalize_entities,
    normalize_graph_payload, parse_timestamp, read_json_file, retry_on_error, safe_filename,
    safe_import, set_nested_value, write_json_file,
};

// Constants
pub use constants::{
    API_ENDPOINTS, CACHE_CONFIG, DATA_TYPES, DEFAULT_CONFIG, ENTITY_TYPES, ERROR_CODES,
    FILE_SIZE_LIMITS, PERFORMANCE_THRESHOLDS, PROCESSING_STATUS, QUALITY_LEVELS,
    RELATIONSHIP_TYPES, RETRY_CONFIG, SUPPORTED_AUDIO_FORMATS, SUPPORTED_DOCUMENT_FORMATS,
    SUPPORTED_GRAPH_DBS, SUPPORTED_IMAGE_FORMATS, SUPPORTED_RDF_FORMATS, SUPPORTED_VECTOR_STORES,
    SUPPORTED_VIDEO_FORMATS,
};

// Types
pub use types::{
    BatchResult, ConfigDict, DataDict, DataType, Entity, EntityDict, EntityType, JSONType,
    ProcessingResult, ProcessingStatus, QualityLevel, QualityMetrics, Relationship,
    RelationshipDict, RelationshipType, Result, dict_to_entity, dict_to_relationship,
    entity_to_dict, is_entity_dict, is_processing_result, is_relationship_dict,
    relationship_to_dict,
};

// Progress tracking
pub use progress_tracker::{
    ConsoleProgressDisplay, FileProgressDisplay, JupyterProgressDisplay, ModuleDetector,
    ProgressDisplay, ProgressItem, ProgressTracker, get_progress_tracker, track_progress,
};
