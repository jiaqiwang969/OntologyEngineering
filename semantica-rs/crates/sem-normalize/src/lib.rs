//! Rust port of Semantica's `semantica.normalize` package (see
//! `semantica-rs/scope/sem-normalize.json`).
//!
//! Data normalization and cleaning: text (Unicode forms, whitespace, punctuation, case,
//! HTML), entity names (aliases, titles, variants), dates and times (a port of
//! `dateutil.parser`, `zoneinfo` time zones, relative expressions), numbers and quantities
//! (units, currencies, scientific notation), datasets (duplicates, validation, missing
//! values), language detection and text encodings. The crate root re-exports the names of the
//! package's `__all__`, so `from semantica.normalize import X` becomes
//! `use sem_normalize::X`.
//!
//! Module map: `text_normalizer`, `text_cleaner`, `entity_normalizer`, `date_normalizer`,
//! `number_normalizer`, `data_cleaner`, `language_detector`, `encoding_handler`, `methods`,
//! `registry`, `config` and `normalize_provenance` mirror the original modules; `datetime` holds the Python
//! `datetime` model shared by the date functions. The usage guide (`normalize_usage.md`) is
//! the documentation module [`normalize_usage`].
//!
//! ```
//! use serde_json::Map;
//! use sem_normalize::{normalize_text, NumberNormalizer};
//!
//! assert_eq!(normalize_text("Hello   World", None, &Map::new()).unwrap(), "Hello World");
//! let number = NumberNormalizer::default().normalize_number("1,234.56").unwrap();
//! assert_eq!(number, 1234.56);
//! ```

pub mod config;
pub mod data_cleaner;
pub mod date_normalizer;
pub mod datetime;
pub mod encoding_handler;
pub mod entity_normalizer;
pub mod language_detector;
pub mod methods;
pub mod normalize_provenance;
pub mod normalize_usage;
pub mod number_normalizer;
pub mod registry;
pub mod text_cleaner;
pub mod text_normalizer;

mod chardet;
mod codecs;
mod dateutil;
mod htmltext;
mod kwargs;
mod pystr;
mod zoneinfo;

// Text normalization
pub use text_normalizer::{
    SpecialCharacterProcessor, TextNormalizer, UnicodeNormalizer, WhitespaceNormalizer,
};
// Entity normalization
pub use entity_normalizer::{
    AliasResolver, EntityDisambiguator, EntityNormalizer, NameVariantHandler,
};
// Date/time normalization
pub use date_normalizer::{
    DateNormalizer, RelativeDateProcessor, TemporalExpressionParser, TimeZoneNormalizer,
};
// Number/quantity normalization
pub use number_normalizer::{
    CurrencyNormalizer, NumberNormalizer, ScientificNotationHandler, UnitConverter,
};
// Data cleaning
pub use data_cleaner::{
    DataCleaner, DataValidator, DuplicateDetector, DuplicateGroup, MissingValueHandler,
    ValidationResult,
};
// Text cleaning
pub use text_cleaner::TextCleaner;
// Language detection
pub use language_detector::LanguageDetector;
// Encoding handling
pub use encoding_handler::EncodingHandler;
// Registry and methods
pub use methods::{
    clean_data, clean_text, detect_duplicates, detect_language, disambiguate_entity,
    get_normalize_method, handle_encoding, list_available_methods, normalize_date,
    normalize_entity, normalize_number, normalize_quantity, normalize_text, normalize_time,
    resolve_aliases,
};
pub use registry::{MethodRegistry, method_registry};
// Configuration
pub use config::{NormalizeConfig, normalize_config};
