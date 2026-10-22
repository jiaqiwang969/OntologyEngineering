//! Namespace and IRI Manager Module (`semantica/ontology/namespace_manager.py`).
//!
//! Manages ontology namespaces and generates consistent, human-readable IRIs: PascalCase for
//! classes, camelCase for properties, cleaned names for individuals, version-aware base URIs,
//! hash-based IRIs when speaking IRIs are disabled, and the standard namespaces (RDF, RDFS,
//! OWL, XSD, SKOS, DC, DCTERMS).
//!
//! The module belongs to `semantica.ontology`; it lives in this crate to break the original's
//! `ontology` <-> `change_management` import cycle (the ontology crate re-exports it). IRIs are
//! joined with a port of Python's `urllib.parse.urljoin`, so results match the original even for
//! unusual base URIs.
//!
//! ```
//! use sem_change::namespace_manager::NamespaceManager;
//! use serde_json::json;
//!
//! let config = json!({"base_uri": "https://example.org/ontology/", "version": "1.0"});
//! let manager = NamespaceManager::new(config.as_object().cloned());
//! assert_eq!(
//!     manager.generate_class_iri("person record").unwrap(),
//!     "https://example.org/ontology/PersonRecord"
//! );
//! assert_eq!(
//!     manager.generate_property_iri("has_name").unwrap(),
//!     "https://example.org/ontology/hasName"
//! );
//! ```

use std::fmt;
use std::sync::{Arc, LazyLock};

use indexmap::IndexMap;
use md5::{Digest, Md5};
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{py_str, slice_chars, truthy};
use sem_utils::{ProgressTracker, SemanticaError, get_logger, get_progress_tracker};
use serde_json::{Map, Value};

use crate::pycompat::{urljoin, urlparse};

/// `re.findall(r"[a-zA-Z0-9]+", name)`.
static WORDS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-zA-Z0-9]+").expect("valid"));
/// `r"[^a-zA-Z0-9]+"` (concept scheme slugs).
static NON_ALNUM_RUNS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^a-zA-Z0-9]+").expect("valid"));

/// The default `base_uri`.
pub const DEFAULT_BASE_URI: &str = "https://semantica.dev/ontology/";

/// `str.capitalize()` for an ASCII word: first character upper case, the rest lower case.
fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let mut out = first.to_ascii_uppercase().to_string();
            out.push_str(&chars.as_str().to_ascii_lowercase());
            out
        }
        None => String::new(),
    }
}

/// The first 8 hex digits of `hashlib.md5(text.encode()).hexdigest()`.
fn md5_prefix(text: &str) -> String {
    let digest = hex::encode(Md5::digest(text.as_bytes()));
    digest[..8].to_string()
}

/// Namespace and IRI management for ontologies (`class NamespaceManager`).
#[derive(Clone)]
pub struct NamespaceManager {
    /// `self.logger` (`semantica.namespace_manager`).
    pub logger: Logger,
    /// `self.config`: the configuration dict (merged keyword arguments).
    pub config: Map<String, Value>,
    /// `self.progress_tracker` (the global tracker, enabled on construction).
    pub progress_tracker: Arc<ProgressTracker>,
    /// `self.base_uri` (default `https://semantica.dev/ontology/`).
    pub base_uri: String,
    /// `self.version` (default `"1.0"`; any JSON value, as in the original).
    pub version: Value,
    /// `self.use_speaking_iris` (default `true`): human-readable instead of hash-based IRIs.
    pub use_speaking_iris: bool,
    /// `self.namespaces`: prefix -> namespace URI, standard prefixes first.
    pub namespaces: IndexMap<String, String>,
}

impl fmt::Debug for NamespaceManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NamespaceManager")
            .field("config", &self.config)
            .field("base_uri", &self.base_uri)
            .field("version", &self.version)
            .field("use_speaking_iris", &self.use_speaking_iris)
            .field("namespaces", &self.namespaces)
            .finish_non_exhaustive()
    }
}

impl Default for NamespaceManager {
    fn default() -> Self {
        NamespaceManager::new(None)
    }
}

impl NamespaceManager {
    /// `NamespaceManager(config=None, **kwargs)`; pass the keyword arguments merged into
    /// `config`. Recognised keys: `base_uri`, `version`, `use_speaking_iris`.
    ///
    /// Like the original, construction enables the global progress tracker.
    pub fn new(config: Option<Map<String, Value>>) -> NamespaceManager {
        let logger = get_logger("namespace_manager");
        let config = config.unwrap_or_default();

        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }

        let base_uri = match config.get("base_uri") {
            None => DEFAULT_BASE_URI.to_string(),
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) => String::new(),
            Some(other) => py_str(other),
        };
        let version = config
            .get("version")
            .cloned()
            .unwrap_or_else(|| Value::String("1.0".into()));
        let use_speaking_iris = config.get("use_speaking_iris").map(truthy).unwrap_or(true);

        let namespaces = [
            ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
            ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
            ("owl", "http://www.w3.org/2002/07/owl#"),
            ("xsd", "http://www.w3.org/2001/XMLSchema#"),
            ("skos", "http://www.w3.org/2004/02/skos/core#"),
            ("dc", "http://purl.org/dc/elements/1.1/"),
            ("dcterms", "http://purl.org/dc/terms/"),
        ]
        .into_iter()
        .map(|(prefix, uri)| (prefix.to_string(), uri.to_string()))
        .collect();

        NamespaceManager {
            logger,
            config,
            progress_tracker,
            base_uri,
            version,
            use_speaking_iris,
            namespaces,
        }
    }

    /// `get_base_uri()`: the base URI, with a `v{version}/` segment appended when the version
    /// is set and is not `"1.0"`.
    pub fn get_base_uri(&self) -> Result<String, SemanticaError> {
        let default_version = matches!(&self.version, Value::String(v) if v == "1.0");
        if truthy(&self.version) && !default_version {
            return urljoin(
                &format!("{}/", self.base_uri.trim_end_matches('/')),
                &format!("v{}/", py_str(&self.version)),
            );
        }
        Ok(self.base_uri.clone())
    }

    /// `generate_class_iri(class_name)`: `<base>PascalCaseName` (or `<base>class/<md5[:8]>`).
    pub fn generate_class_iri(&self, class_name: &str) -> Result<String, SemanticaError> {
        let class_name = self.to_pascal_case(class_name);
        if self.use_speaking_iris {
            urljoin(&self.get_base_uri()?, &class_name)
        } else {
            let hash_id = md5_prefix(&class_name);
            urljoin(&self.get_base_uri()?, &format!("class/{hash_id}"))
        }
    }

    /// `generate_property_iri(property_name)`: `<base>camelCaseName` (or
    /// `<base>property/<md5[:8]>`).
    pub fn generate_property_iri(&self, property_name: &str) -> Result<String, SemanticaError> {
        let property_name = self.to_camel_case(property_name);
        if self.use_speaking_iris {
            urljoin(&self.get_base_uri()?, &property_name)
        } else {
            let hash_id = md5_prefix(&property_name);
            urljoin(&self.get_base_uri()?, &format!("property/{hash_id}"))
        }
    }

    /// `generate_individual_iri(individual_name)`: `<base>individual/<alphanumerics>` (or
    /// `<base>individual/<md5[:8]>`).
    pub fn generate_individual_iri(&self, individual_name: &str) -> Result<String, SemanticaError> {
        let individual_name: String = individual_name
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();
        if self.use_speaking_iris {
            urljoin(
                &self.get_base_uri()?,
                &format!("individual/{individual_name}"),
            )
        } else {
            let hash_id = md5_prefix(&individual_name);
            urljoin(&self.get_base_uri()?, &format!("individual/{hash_id}"))
        }
    }

    /// `register_namespace(prefix, uri)`.
    pub fn register_namespace(&mut self, prefix: &str, uri: &str) {
        self.namespaces.insert(prefix.to_string(), uri.to_string());
        self.logger
            .debug(format!("Registered namespace: {prefix} -> {uri}"));
    }

    /// `get_namespace(prefix)`.
    pub fn get_namespace(&self, prefix: &str) -> Option<String> {
        self.namespaces.get(prefix).cloned()
    }

    /// `get_all_namespaces()`: a copy of the prefix -> URI mapping.
    pub fn get_all_namespaces(&self) -> IndexMap<String, String> {
        self.namespaces.clone()
    }

    /// `get_skos_uri(local_name)`: the SKOS namespace plus `local_name` (`KeyError('skos')` if
    /// the prefix was removed from [`Self::namespaces`]).
    pub fn get_skos_uri(&self, local_name: &str) -> Result<String, SemanticaError> {
        let skos_ns = self
            .namespaces
            .get("skos")
            .ok_or_else(|| SemanticaError::Key("skos".to_string()))?;
        Ok(format!("{skos_ns}{local_name}"))
    }

    /// `build_concept_scheme_uri(name)`: `<base>vocab/<slug>`, the slug being the name with
    /// non-alphanumeric runs replaced by `-`, trimmed of `-` and lower-cased.
    pub fn build_concept_scheme_uri(&self, name: &str) -> Result<String, SemanticaError> {
        let slug = NON_ALNUM_RUNS.replace_all(name, "-");
        let slug = slug.trim_matches('-').to_ascii_lowercase();
        urljoin(&self.get_base_uri()?, &format!("vocab/{slug}"))
    }

    /// `get_alignment_predicates()`: the OWL and SKOS alignment predicate URIs.
    pub fn get_alignment_predicates(&self) -> IndexMap<String, String> {
        let owl_ns = self
            .get_namespace("owl")
            .unwrap_or_else(|| "None".to_string());
        let skos_ns = self
            .get_namespace("skos")
            .unwrap_or_else(|| "None".to_string());
        [
            ("equivalentClass", &owl_ns),
            ("equivalentProperty", &owl_ns),
            ("sameAs", &owl_ns),
            ("exactMatch", &skos_ns),
            ("closeMatch", &skos_ns),
            ("broadMatch", &skos_ns),
            ("narrowMatch", &skos_ns),
            ("relatedMatch", &skos_ns),
        ]
        .into_iter()
        .map(|(name, ns)| (name.to_string(), format!("{ns}{name}")))
        .collect()
    }

    /// `_to_pascal_case(name)`.
    fn to_pascal_case(&self, name: &str) -> String {
        let words: Vec<&str> = WORDS.find_iter(name).map(|m| m.as_str()).collect();
        if words.is_empty() {
            return "Entity".to_string();
        }
        words.iter().map(|w| capitalize(w)).collect()
    }

    /// `_to_camel_case(name)`: names that already look camelCase are kept as they are.
    fn to_camel_case(&self, name: &str) -> String {
        if let Some(first) = name.chars().next()
            && first.is_lowercase()
            && name.chars().any(char::is_uppercase)
            && !name.contains(' ')
            && !name.contains('_')
        {
            return name.to_string();
        }
        let words: Vec<&str> = WORDS.find_iter(name).map(|m| m.as_str()).collect();
        let Some((first, rest)) = words.split_first() else {
            return "hasProperty".to_string();
        };
        let mut out = first.to_ascii_lowercase();
        for word in rest {
            out.push_str(&capitalize(word));
        }
        out
    }

    /// `validate_iri(iri)`: `true` when `urlparse(iri)` has both a scheme and a network
    /// location (`false` when parsing fails).
    pub fn validate_iri(&self, iri: &str) -> bool {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("ontology"),
            Some("NamespaceManager"),
            &format!("Validating IRI: {}...", slice_chars(iri, None, Some(50))),
            None,
        );
        match urlparse(iri, "") {
            Ok(parsed) => {
                let is_valid = !parsed.scheme.is_empty() && !parsed.netloc.is_empty();
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    &format!(
                        "IRI validation: {}",
                        if is_valid { "Valid" } else { "Invalid" }
                    ),
                    None,
                );
                is_valid
            }
            Err(e) => {
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("failed"),
                    &e.to_string(),
                    None,
                );
                false
            }
        }
    }
}
