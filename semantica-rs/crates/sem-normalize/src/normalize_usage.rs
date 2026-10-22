//! # Normalize Module Usage Guide
//!
//! The Rust form of `semantica/normalize/normalize_usage.md`: clean, standardize and prepare
//! text and data for downstream processing. Keyword arguments of the Python API are option
//! structs or `serde_json` maps here; defaults are `None` / `Default::default()`.
//!
//! 1. [Quick Start](#quick-start)
//! 2. [Text](#text)
//! 3. [Entities](#entities)
//! 4. [Dates & Time](#dates--time)
//! 5. [Numbers & Quantities](#numbers--quantities)
//! 6. [Data Cleaning](#data-cleaning)
//! 7. [Language](#language)
//! 8. [Encoding](#encoding)
//! 9. [Methods Registry](#methods-registry)
//! 10. [Configuration](#configuration)
//! 11. [Workflows](#workflows)
//!
//! ## Quick Start
//!
//! ```
//! use sem_normalize::text_normalizer::NormalizeTextOptions;
//! use sem_normalize::{DateNormalizer, EntityNormalizer, TextNormalizer};
//!
//! let text_norm = TextNormalizer::default();
//! let entity_norm = EntityNormalizer::default();
//! let date_norm = DateNormalizer::default();
//!
//! let text = text_norm
//!     .normalize_text("Hello   World", &NormalizeTextOptions::with_case("lower"))
//!     .unwrap();
//! assert_eq!(text, "hello world");
//! let entity = entity_norm.normalize_entity("John Doe", Some("Person"), true);
//! let date = date_norm.normalize_date("2023-01-15", Some("ISO8601"), None).unwrap();
//! # let _ = (entity, date);
//! ```
//!
//! ## Text
//!
//! ```
//! use sem_normalize::text_cleaner::CleanOptions;
//! use sem_normalize::text_normalizer::NormalizeTextOptions;
//! use sem_normalize::{
//!     SpecialCharacterProcessor, TextCleaner, TextNormalizer, UnicodeNormalizer,
//!     WhitespaceNormalizer,
//! };
//! use serde_json::{json, Map};
//!
//! // Unicode forms (NFC, NFD, NFKC, NFKD)
//! let unicode_norm = UnicodeNormalizer::default();
//! let nfc = unicode_norm.normalize_unicode("cafe\u{301}", Some("NFC"));
//!
//! // Whitespace, line breaks, indentation
//! let ws = WhitespaceNormalizer::default();
//! let compact = ws.normalize_whitespace("Hello   World\t\n\n\nNext", Some("unix"));
//! let windows = ws.handle_line_breaks("a\nb", Some("windows"));
//! let spaces = ws.process_indentation("\tcode", Some("spaces"));
//!
//! // Case: "lower", "upper", "title" or "preserve"
//! let normalizer = TextNormalizer::default();
//! for case in ["lower", "upper", "title", "preserve"] {
//!     let _ = normalizer.normalize_text("Hello World", &NormalizeTextOptions::with_case(case));
//! }
//!
//! // Special characters: smart punctuation, diacritics
//! let special = SpecialCharacterProcessor::default();
//! let quotes = special.normalize_punctuation("\u{201c}Hello\u{201d} \u{2014} world\u{2026}");
//! let plain = special.process_diacritics("Cr\u{e8}me Br\u{fb}l\u{e9}e", true);
//!
//! // Cleaning: HTML, whitespace, Unicode, special characters, sanitizing
//! let cleaner = TextCleaner::default();
//! let cleaned = cleaner.clean("<p>Hello <b>World</b></p>", &CleanOptions::default());
//! let text_only = cleaner.remove_html("<p>One</p><p>Two</p>", true);
//! let safe = cleaner.sanitize("<script>alert(1)</script>ok", false);
//! let via_normalizer = normalizer
//!     .clean_text("<p>Hi</p>", &json!({"remove_html": true}).as_object().unwrap().clone())
//!     .unwrap();
//!
//! // Batch
//! let batch = normalizer
//!     .process_batch(&["Hello   World", "Test   Text"], &NormalizeTextOptions::with_case("lower"))
//!     .unwrap();
//! let cleaned_batch = cleaner.clean_batch(&["<b>a</b>", "b  c"], &CleanOptions::default());
//! # let _ = (nfc, compact, windows, spaces, quotes, plain, cleaned, text_only, safe,
//! #          via_normalizer, batch, cleaned_batch, Map::<String, serde_json::Value>::new());
//! ```
//!
//! ## Entities
//!
//! ```
//! use sem_normalize::{AliasResolver, EntityDisambiguator, EntityNormalizer, NameVariantHandler};
//! use serde_json::{json, Map};
//!
//! let config = json!({"alias_map": {"j. doe": "John Doe"}}).as_object().unwrap().clone();
//! let normalizer = EntityNormalizer::new(config.clone());
//! let name = normalizer.normalize_entity("  john   doe  ", Some("Person"), true);
//! let canonical = normalizer.resolve_aliases("J. Doe", &Map::new());
//! let resolved = AliasResolver::new(config).resolve_aliases("j. doe");
//!
//! // Disambiguation (context as keyword arguments)
//! let context = json!({"entity_type": "Organization", "context": "technology company"});
//! let result = normalizer.disambiguate_entity("Apple", context.as_object().unwrap());
//! let kind = EntityDisambiguator::default().classify_entity_type("John Doe");
//!
//! // Linking to canonical forms
//! let linked = normalizer.link_entities(&["John Doe", "J. Doe"], Some("Person"), true);
//!
//! // Name variants: titles and formats ("standard", "title", "lower")
//! let handler = NameVariantHandler::default();
//! let standard = handler.normalize_name_format("Dr. John Doe", Some("standard"));
//! let titled = handler.handle_titles_and_honorifics("Mr. John Doe");
//! # let _ = (name, canonical, resolved, result, kind, linked, standard, titled);
//! ```
//!
//! ## Dates & Time
//!
//! ```
//! use sem_normalize::date_normalizer::DateTime;
//! use sem_normalize::{DateNormalizer, RelativeDateProcessor, TemporalExpressionParser, TimeZoneNormalizer};
//!
//! let normalizer = DateNormalizer::default();
//! // Formats: "ISO8601", "date" or any strftime format
//! let iso = normalizer.normalize_date("2023-01-15", Some("ISO8601"), None).unwrap();
//! let day = normalizer.normalize_date("January 15, 2023", Some("date"), None).unwrap();
//! let custom = normalizer.normalize_date("2023-01-15T10:30:00", Some("%d/%m/%Y %H:%M"), None).unwrap();
//! // Time zones (zoneinfo names); naive values are taken as UTC
//! let utc = normalizer.normalize_date("2023-01-15T10:30:00+05:30", None, Some("UTC")).unwrap();
//! let tz = TimeZoneNormalizer::default();
//! let dt = DateTime::from_ymd_hms(2023, 7, 1, 12, 0, 0).unwrap();
//! let local = tz.normalize_timezone(&dt, Some("America/New_York"));
//! // Relative dates and times
//! let yesterday = normalizer.process_relative_date("yesterday", None).unwrap();
//! let earlier = RelativeDateProcessor::default()
//!     .process_relative_expression("3 days ago", None)
//!     .unwrap();
//! let time = normalizer.normalize_time("10:30 PM");
//! // Temporal expressions
//! let parsed = normalizer.parse_temporal_expression("from 2023-01-01 to 2023-01-31");
//! let range = TemporalExpressionParser::default().process_temporal_ranges("between 2023-01-01 and 2023-03-31");
//! # let _ = (iso, day, custom, utc, local, yesterday, earlier, time, parsed, range);
//! ```
//!
//! ## Numbers & Quantities
//!
//! ```
//! use sem_normalize::{CurrencyNormalizer, NumberNormalizer, ScientificNotationHandler, UnitConverter};
//!
//! let normalizer = NumberNormalizer::default();
//! let number = normalizer.normalize_number("1,234.56").unwrap();
//! let percentage = normalizer.normalize_number("50%").unwrap();
//! let scientific = normalizer.normalize_number("1.5e3").unwrap();
//! let quantity = normalizer.normalize_quantity("5 kg").unwrap();
//! let meters = normalizer.convert_units(1.0, "km", "m").unwrap();
//! let pounds = UnitConverter::default().convert_units(100.0, "kilogram", "pound").unwrap();
//! let currency = normalizer.process_currency("$100", None);
//! let euros = CurrencyNormalizer::default().normalize_currency("\u{20ac}50", None);
//! let handler = ScientificNotationHandler::default();
//! let text = handler.convert_to_scientific(1500.0, Some(2));
//! let rounded = handler.normalize_precision(123.456, 2);
//! # let _ = (number, percentage, scientific, quantity, meters, pounds, currency, euros, text, rounded);
//! ```
//!
//! ## Data Cleaning
//!
//! ```
//! use sem_normalize::data_cleaner::PyType;
//! use sem_normalize::{DataCleaner, DataValidator, DuplicateDetector, MissingValueHandler};
//! use serde_json::{json, Map, Value};
//!
//! let to_records = |v: Value| -> Vec<Map<String, Value>> {
//!     v.as_array().unwrap().iter().map(|r| r.as_object().unwrap().clone()).collect()
//! };
//! let dataset = to_records(json!([
//!     {"id": 1, "name": "Alice", "age": 30},
//!     {"id": 2, "name": "Alice", "age": 30},
//!     {"id": 3, "name": "Bob", "age": null}
//! ]));
//! let cleaner = DataCleaner::default();
//! let options = json!({"missing_strategy": "fill", "fill_value": 0,
//!                      "duplicate_criteria": {"key_fields": ["name", "age"]}});
//! let cleaned = cleaner.clean_data(&dataset, true, true, true, options.as_object().unwrap()).unwrap();
//!
//! let duplicates = DuplicateDetector::default().detect_duplicates(&dataset, Some(0.9), None);
//! let schema = json!({"fields": {"name": {"type": "str", "required": true}}});
//! let validation = cleaner.validate_data(&dataset, schema.as_object()).unwrap();
//! let is_int = DataValidator::default().check_data_types(&json!(5), &PyType::Int.into());
//!
//! let handler = MissingValueHandler::default();
//! let info = handler.identify_missing_values(&dataset);
//! let imputed = handler.impute_values(&dataset, Some("mean"));
//! # let _ = (cleaned, duplicates, validation, is_int, info, imputed);
//! ```
//!
//! ## Language
//!
//! ```
//! use sem_normalize::LanguageDetector;
//!
//! let detector = LanguageDetector::default();
//! let language = detector.detect("This is a simple English sentence.");
//! let (lang, confidence) = detector.detect_with_confidence("Ceci est une phrase fran\u{e7}aise simple.");
//! let top = detector.detect_multiple("Dies ist ein einfacher deutscher Satz.", Some(3));
//! let batch = detector.detect_batch(&["Hello there, how are you today?", "Hola"]);
//! let english = detector.is_language("This is definitely an English sentence.", "en", None);
//! let name = detector.get_language_name("fr");
//! # let _ = (language, lang, confidence, top, batch, english, name);
//! ```
//!
//! ## Encoding
//!
//! ```no_run
//! use sem_normalize::EncodingHandler;
//! use std::path::Path;
//!
//! let handler = EncodingHandler::default();
//! let (encoding, confidence) = handler.detect(b"Hello world");
//! let text = handler.convert_to_utf8(&b"caf\xe9"[..], Some("latin-1")).unwrap();
//! let without_bom = handler.remove_bom(&b"\xef\xbb\xbfHello"[..]);
//! let (file_encoding, _) = handler.detect_file("input.txt", None).unwrap();
//! let content = handler.convert_file_to_utf8("input.txt", Some(Path::new("output.txt"))).unwrap();
//! # let _ = (encoding, confidence, text, without_bom, file_encoding, content);
//! ```
//!
//! ## Methods Registry
//!
//! ```
//! use std::sync::Arc;
//! use sem_normalize::{get_normalize_method, list_available_methods, method_registry, normalize_text};
//! use serde_json::{json, Map, Value};
//!
//! // A custom method receives the positional arguments and the keyword arguments.
//! method_registry.register(
//!     "text",
//!     "custom_upper",
//!     Arc::new(|args: &[Value], _kwargs: &Map<String, Value>| {
//!         Ok(json!(args[0].as_str().unwrap_or_default().to_uppercase().trim().to_string()))
//!     }),
//! );
//! let all = list_available_methods(None);
//! let text_methods = method_registry.list_all(Some("text"));
//! let custom = get_normalize_method("text", "custom_upper").unwrap();
//! let direct = custom(&[json!("hello world")], &Map::new()).unwrap();
//! let via_function = normalize_text("hello world", Some("custom_upper"), &Map::new()).unwrap();
//! method_registry.unregister("text", "custom_upper");
//! method_registry.clear(Some("text"));
//! # let _ = (all, text_methods, direct, via_function);
//! ```
//!
//! ## Configuration
//!
//! Environment variables `NORMALIZE_UNICODE_FORM`, `NORMALIZE_CASE`,
//! `NORMALIZE_DATE_FORMAT`, `NORMALIZE_TIMEZONE`, `NORMALIZE_DEFAULT_LANGUAGE`,
//! `NORMALIZE_DEFAULT_ENCODING` (and any other `NORMALIZE_*`) are read when a configuration
//! is created; a YAML, JSON or TOML file provides `normalize` and `normalize_methods`
//! sections:
//!
//! ```yaml
//! normalize:
//!   unicode_form: NFC
//!   case: lower
//! normalize_methods:
//!   text:
//!     case: lower
//! ```
//!
//! ```
//! use sem_normalize::{normalize_config, NormalizeConfig};
//! use serde_json::{json, Value};
//!
//! let unicode_form = normalize_config.get("unicode_form", json!("NFC"));
//! let config = NormalizeConfig::new(Some("config.yaml")); // a missing file is skipped
//! config.set("case", "lower");
//! config.set_method_config("text", json!({"case": "lower"}).as_object().unwrap().clone());
//! let text_config = config.get_method_config("text");
//! let everything = config.get_all();
//! # let _ = (unicode_form, text_config, everything, Value::Null);
//! ```
//!
//! ## Workflows
//!
//! ```
//! use sem_normalize::text_normalizer::NormalizeTextOptions;
//! use sem_normalize::{DataCleaner, DateNormalizer, EncodingHandler, EntityNormalizer,
//!                     NumberNormalizer, TextNormalizer};
//! use serde_json::{json, Map, Value};
//!
//! let text_norm = TextNormalizer::default();
//! let entity_norm = EntityNormalizer::default();
//! let date_norm = DateNormalizer::default();
//! let number_norm = NumberNormalizer::default();
//!
//! let text = text_norm.normalize_text("Hello   World", &NormalizeTextOptions::with_case("lower"));
//! let entities: Vec<String> = ["John Doe", "J. Doe", "Johnny Doe"]
//!     .iter()
//!     .map(|e| entity_norm.normalize_entity(*e, Some("Person"), true))
//!     .collect();
//! let dates: Vec<_> = ["2023-01-15", "yesterday", "3 days ago"]
//!     .iter()
//!     .map(|d| date_norm.normalize_date(*d, None, None))
//!     .collect();
//! let numbers: Vec<_> = ["1,234.56", "50%", "1.5e3"]
//!     .iter()
//!     .map(|n| number_norm.normalize_number(*n))
//!     .collect();
//!
//! // Encoding-aware: detect, convert, then normalize.
//! let handler = EncodingHandler::default();
//! let data = b"\xff\xfeH\x00e\x00l\x00l\x00o\x00";
//! let (encoding, _) = handler.detect(&data[..]);
//! let utf8_text = handler.convert_to_utf8(&data[..], encoding.as_deref()).unwrap();
//! let normalized = text_norm.normalize_text(utf8_text.as_str(), &NormalizeTextOptions::default());
//!
//! // Cleaning pipeline
//! let raw: Vec<Map<String, Value>> = json!([
//!     {"id": 1, "name": "  Alice  ", "date": "2023-01-15"},
//!     {"id": 1, "name": "Alice", "date": "2023-01-15"},
//!     {"id": 3, "name": null, "date": "2023-01-20"}
//! ]).as_array().unwrap().iter().map(|r| r.as_object().unwrap().clone()).collect();
//! let cleaned = DataCleaner::default().clean_data(&raw, true, true, true, &Map::new()).unwrap();
//! # let _ = (text, entities, dates, numbers, normalized, cleaned);
//! ```
//!
//! ## Best Practices
//!
//! 1. Use NFC for most text, NFKC for compatibility matching.
//! 2. Preserve case unless normalization is needed.
//! 3. Pass the entity type when known (`Person` names are title-cased).
//! 4. Prefer ISO 8601 dates and explicit time zones.
//! 5. Mind locale-specific number formats (commas vs periods).
//! 6. Validate data before cleaning and pick a missing-value strategy deliberately.
//! 7. Language detection needs at least 10 characters.
//! 8. Detect an encoding before converting; the fallback chain covers the rest.
//! 9. Use the batch methods for large inputs.
//! 10. Keep settings in configuration files.
//! 11. Handle `ValidationError` / `ProcessingError` (`SemanticaError` variants).
//! 12. Register custom methods for domain-specific normalization.
