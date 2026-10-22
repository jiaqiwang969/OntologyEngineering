//! Ontology Version Manager Module (`semantica/change_management/ontology_version_manager.py`).
//!
//! Ontology versioning following best practices: the ontology IRI carries the version
//! (`<base>/v<version>`), element IRIs stay version-less, `owl:versionInfo` is recorded, several
//! versions can coexist, versioned imports are resolved, and versions can be compared, diffed
//! and migrated ([`VersionManager`], [`OntologyVersion`]).
//!
//! ```
//! use sem_change::VersionManager;
//! use serde_json::{Map, json};
//!
//! let config = json!({"base_uri": "https://example.org/ontology/"});
//! let mut manager = VersionManager::new(config.as_object().cloned(), None);
//! let mut ontology = Map::new();
//! let version = manager.create_version("1.0", &mut ontology, &Default::default()).unwrap();
//! assert_eq!(version.ontology_iri, "https://example.org/ontology/v1.0");
//! assert_eq!(ontology["versionInfo"], json!("1.0"));
//! ```

use std::sync::{Arc, LazyLock};

use indexmap::IndexMap;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{hash_key, py_eq, py_str, truthy, type_name};
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{
    ProgressTracker, SemanticaError, ValidationError, get_logger, get_progress_tracker,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::managers::compare_ontology_structures;
use crate::namespace_manager::NamespaceManager;
use crate::pycompat::{PySet, attribute_error, get, py_list};

/// `re.search(r"/v(\d+\.\d+)", import_uri)`.
static VERSIONED_IMPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/v(\d+\.\d+)").expect("valid regex"));

/// Ontology version information (`@dataclass OntologyVersion`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OntologyVersion {
    /// The version string (e.g. `"1.0"`).
    pub version: String,
    /// The versioned ontology IRI.
    pub ontology_iri: String,
    /// The `owl:versionInfo` value.
    pub version_info: String,
    /// Creation time (`datetime.now().isoformat()`).
    pub created_at: String,
    /// Change descriptions.
    pub changes: Vec<String>,
    /// Additional metadata (`structure` and `imports` are used by comparisons and import
    /// resolution).
    pub metadata: Map<String, Value>,
}

impl OntologyVersion {
    /// `OntologyVersion(version, ontology_iri, version_info, created_at, changes=[],
    /// metadata={})`.
    pub fn new(
        version: impl Into<String>,
        ontology_iri: impl Into<String>,
        version_info: impl Into<String>,
        created_at: impl Into<String>,
        changes: Option<Vec<String>>,
        metadata: Option<Map<String, Value>>,
    ) -> OntologyVersion {
        OntologyVersion {
            version: version.into(),
            ontology_iri: ontology_iri.into(),
            version_info: version_info.into(),
            created_at: created_at.into(),
            changes: changes.unwrap_or_default(),
            metadata: metadata.unwrap_or_default(),
        }
    }
}

/// Keyword options of `create_version(version, ontology, **options)`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateVersionOptions {
    /// `changes`: list of change descriptions (default `[]`).
    pub changes: Option<Vec<String>>,
    /// `metadata`: additional metadata (default `{}`).
    pub metadata: Option<Map<String, Value>>,
}

/// `_generate_versioned_iri(base_uri, version)`: `<base without trailing />/v<version>`.
fn generate_versioned_iri(base_uri: &str, version: &str) -> String {
    let base_uri = base_uri.trim_end_matches('/');
    let suffix = format!("/v{version}");
    if base_uri.ends_with(&suffix) {
        base_uri.to_string()
    } else {
        format!("{base_uri}{suffix}")
    }
}

/// The `uri`/`name` key of a diffed ontology element, its item, in first-seen order.
type ElementMap = IndexMap<String, (Value, Map<String, Value>)>;

/// The keyed elements of one section of an ontology dict (`_compute_section_diff` maps):
/// dicts by `uri` (else `name`), strings as `{"uri": s}`, anything else ignored.
fn section_elements(section: &Value) -> Result<ElementMap, SemanticaError> {
    let mut elements: ElementMap = IndexMap::new();
    for item in py_list(section)? {
        let (key, element) = match item {
            Value::Object(map) => {
                let uri = get(&map, "uri");
                let key = if truthy(uri) {
                    uri.clone()
                } else {
                    get(&map, "name").clone()
                };
                if !truthy(&key) {
                    continue;
                }
                (key, map)
            }
            Value::String(s) => {
                let mut map = Map::new();
                map.insert("uri".into(), Value::String(s.clone()));
                (Value::String(s), map)
            }
            _ => continue,
        };
        let hashed = hash_key(&key)?;
        match elements.get_mut(&hashed) {
            Some(slot) => slot.1 = element,
            None => {
                elements.insert(hashed, (key, element));
            }
        }
    }
    Ok(elements)
}

/// `(added, removed, changed)` items of one ontology section.
type SectionDiff = (Vec<Value>, Vec<Value>, Vec<Value>);

/// `_compute_section_diff(base_list, target_list)`: `(added, removed, changed)`.
fn compute_section_diff(
    base_list: &Value,
    target_list: &Value,
) -> Result<SectionDiff, SemanticaError> {
    let base_map = section_elements(base_list)?;
    let target_map = section_elements(target_list)?;
    let (mut added, mut removed, mut changed) = (Vec::new(), Vec::new(), Vec::new());

    // Find added and changed.
    for (hashed, (key, t_item)) in &target_map {
        let Some((_, b_item)) = base_map.get(hashed) else {
            added.push(Value::Object(t_item.clone()));
            continue;
        };
        let mut changes = Map::new();
        let fields = b_item
            .keys()
            .chain(t_item.keys().filter(|k| !b_item.contains_key(*k)));
        for field in fields {
            if field == "uri" || field == "name" {
                continue;
            }
            let b_val = get(b_item, field);
            let t_val = get(t_item, field);
            // Lists compare as sets of their items' str() (order-insensitive).
            let differs = match (b_val, t_val) {
                (Value::Array(b), Value::Array(t)) => {
                    let as_strings = |items: &[Value]| -> Result<PySet, SemanticaError> {
                        let strings: Vec<Value> =
                            items.iter().map(|x| Value::String(py_str(x))).collect();
                        PySet::from_items(strings.iter())
                    };
                    as_strings(b)?.ne(&as_strings(t)?)
                }
                _ => !py_eq(b_val, t_val),
            };
            if differs {
                let mut change = Map::new();
                change.insert("old".into(), b_val.clone());
                change.insert("new".into(), t_val.clone());
                changes.insert(field.clone(), Value::Object(change));
            }
        }
        if !changes.is_empty() {
            let or_key = |field: &str| {
                let v = get(t_item, field);
                if truthy(v) { v.clone() } else { key.clone() }
            };
            let mut entry = Map::new();
            entry.insert("uri".into(), or_key("uri"));
            entry.insert("name".into(), or_key("name"));
            entry.insert("changes".into(), Value::Object(changes));
            changed.push(Value::Object(entry));
        }
    }

    // Find deleted.
    for (hashed, (_, b_item)) in &base_map {
        if !target_map.contains_key(hashed) {
            removed.push(Value::Object(b_item.clone()));
        }
    }
    Ok((added, removed, changed))
}

/// Ontology version management system (`class VersionManager`).
///
/// Generates versioned ontology IRIs (`<base>/v<version>`) and version-less element IRIs,
/// records versions, compares and diffs them, migrates ontology dicts between versions and
/// resolves versioned imports.
pub struct VersionManager {
    /// `self.logger` (`semantica.version_manager`).
    pub logger: Logger,
    /// `self.config` (merged keyword arguments).
    pub config: Map<String, Value>,
    /// `self.progress_tracker` (the global tracker, enabled on construction).
    pub progress_tracker: Arc<ProgressTracker>,
    /// `self.namespace_manager`: the given manager, or one built from `config`.
    pub namespace_manager: NamespaceManager,
    /// `self.versions`: version string -> record, in creation order.
    pub versions: IndexMap<String, OntologyVersion>,
    /// `self.latest_version`.
    pub latest_version: Option<String>,
}

impl std::fmt::Debug for VersionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VersionManager")
            .field("config", &self.config)
            .field("namespace_manager", &self.namespace_manager)
            .field("versions", &self.versions)
            .field("latest_version", &self.latest_version)
            .finish_non_exhaustive()
    }
}

impl Default for VersionManager {
    fn default() -> Self {
        VersionManager::new(None, None)
    }
}

impl VersionManager {
    /// `VersionManager(config=None, **kwargs)`; pass the keyword arguments merged into
    /// `config` (`base_uri`, `version`, `use_speaking_iris`, ...). The `namespace_manager`
    /// keyword is the separate `namespace_manager` argument; without one, a
    /// [`NamespaceManager`] is built from the same configuration.
    ///
    /// Like the original, construction enables the global progress tracker.
    pub fn new(
        config: Option<Map<String, Value>>,
        namespace_manager: Option<NamespaceManager>,
    ) -> VersionManager {
        let logger = get_logger("version_manager");
        let config = config.unwrap_or_default();

        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }

        let namespace_manager =
            namespace_manager.unwrap_or_else(|| NamespaceManager::new(Some(config.clone())));
        VersionManager {
            logger,
            config,
            progress_tracker,
            namespace_manager,
            versions: IndexMap::new(),
            latest_version: None,
        }
    }

    /// `create_version(version, ontology, **options)`: record a new version whose IRI is the
    /// ontology's `uri` (or the namespace base URI) with `/v<version>` appended, and update the
    /// ontology dict in place (`version`, `uri`, `versionInfo`).
    pub fn create_version(
        &mut self,
        version: &str,
        ontology: &mut Map<String, Value>,
        options: &CreateVersionOptions,
    ) -> Result<OntologyVersion, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("ontology"),
            Some("VersionManager"),
            &format!("Creating ontology version {version}"),
            None,
        );
        match self.create_version_tracked(&tracking_id, version, ontology, options) {
            Ok(record) => {
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("completed"),
                    &format!("Created ontology version {version}"),
                    None,
                );
                Ok(record)
            }
            Err(e) => {
                self.progress_tracker.stop_tracking(
                    &tracking_id,
                    Some("failed"),
                    &e.to_string(),
                    None,
                );
                Err(e)
            }
        }
    }

    fn create_version_tracked(
        &mut self,
        tracking_id: &str,
        version: &str,
        ontology: &mut Map<String, Value>,
        options: &CreateVersionOptions,
    ) -> Result<OntologyVersion, SemanticaError> {
        // Generate versioned ontology IRI.
        self.progress_tracker
            .update_tracking(tracking_id, None, "Generating versioned IRI...");
        let uri = get(ontology, "uri");
        let base_uri = if truthy(uri) {
            match uri {
                Value::String(s) => s.clone(),
                other => return Err(attribute_error(other, "rstrip")),
            }
        } else {
            self.namespace_manager.get_base_uri()?
        };
        let versioned_iri = generate_versioned_iri(&base_uri, version);

        // Create version record.
        self.progress_tracker
            .update_tracking(tracking_id, None, "Creating version record...");
        let version_record = OntologyVersion::new(
            version,
            versioned_iri.clone(),
            version,
            PyDateTime::now(false).isoformat('T'),
            Some(options.changes.clone().unwrap_or_default()),
            Some(options.metadata.clone().unwrap_or_default()),
        );

        self.versions
            .insert(version.to_string(), version_record.clone());
        self.latest_version = Some(version.to_string());

        // Update ontology with version info.
        ontology.insert("version".into(), Value::String(version.to_string()));
        ontology.insert("uri".into(), Value::String(versioned_iri));
        ontology.insert(
            "versionInfo".into(),
            Value::String(version_record.version_info.clone()),
        );

        self.logger
            .info(format!("Created ontology version: {version}"));
        Ok(version_record)
    }

    /// `generate_element_iri(element_name, element_type="class")`: a version-less IRI via the
    /// namespace manager (`class`, `property`, anything else = individual).
    pub fn generate_element_iri(
        &self,
        element_name: &str,
        element_type: Option<&str>,
    ) -> Result<String, SemanticaError> {
        // The original computes (and discards) the base URI first; it can fail the same way.
        let _base_uri = self.namespace_manager.get_base_uri()?;
        match element_type.unwrap_or("class") {
            "class" => self.namespace_manager.generate_class_iri(element_name),
            "property" => self.namespace_manager.generate_property_iri(element_name),
            _ => self.namespace_manager.generate_individual_iri(element_name),
        }
    }

    /// `compare_versions(version1, version2)`: metadata changes (`ontology_iri`,
    /// `version_info`) and the structural differences recorded in each version's
    /// `metadata["structure"]`.
    pub fn compare_versions(
        &self,
        version1: &str,
        version2: &str,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let Some(v1) = self.versions.get(version1) else {
            return Err(ValidationError::new(format!("Version not found: {version1}")).into());
        };
        let Some(v2) = self.versions.get(version2) else {
            return Err(ValidationError::new(format!("Version not found: {version2}")).into());
        };

        let mut metadata_changes = Map::new();
        for (key, from, to) in [
            ("ontology_iri", &v1.ontology_iri, &v2.ontology_iri),
            ("version_info", &v1.version_info, &v2.version_info),
        ] {
            if from != to {
                let mut change = Map::new();
                change.insert("from".into(), Value::String(from.clone()));
                change.insert("to".into(), Value::String(to.clone()));
                metadata_changes.insert(key.into(), Value::Object(change));
            }
        }

        let empty = Value::Object(Map::new());
        let structural_diff = compare_ontology_structures(
            v1.metadata.get("structure").unwrap_or(&empty),
            v2.metadata.get("structure").unwrap_or(&empty),
        )?;

        let mut result = Map::new();
        result.insert("version1".into(), Value::String(version1.to_string()));
        result.insert("version2".into(), Value::String(version2.to_string()));
        result.insert("metadata_changes".into(), Value::Object(metadata_changes));
        result.extend(structural_diff);
        Ok(result)
    }

    /// `diff_ontologies(base, target)`: added/removed/changed classes, properties, individuals
    /// and axioms between two ontology dicts. Elements are matched by `uri` (else `name`;
    /// plain strings are URIs); list-valued fields compare as sets.
    pub fn diff_ontologies(
        &self,
        base: &Map<String, Value>,
        target: &Map<String, Value>,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let empty = Value::Array(Vec::new());
        let mut sections = Vec::new();
        for key in ["classes", "properties", "individuals", "axioms"] {
            sections.push(compute_section_diff(
                base.get(key).unwrap_or(&empty),
                target.get(key).unwrap_or(&empty),
            )?);
        }
        let mut result = Map::new();
        for (name, (added, removed, changed)) in ["classes", "properties", "individuals", "axioms"]
            .into_iter()
            .zip(sections)
        {
            result.insert(format!("added_{name}"), Value::Array(added));
            result.insert(format!("removed_{name}"), Value::Array(removed));
            result.insert(format!("changed_{name}"), Value::Array(changed));
        }
        Ok(result)
    }

    /// `get_version(version)`.
    pub fn get_version(&self, version: &str) -> Option<&OntologyVersion> {
        self.versions.get(version)
    }

    /// `get_latest_version()`.
    pub fn get_latest_version(&self) -> Option<&OntologyVersion> {
        match &self.latest_version {
            Some(latest) if !latest.is_empty() => self.versions.get(latest),
            _ => None,
        }
    }

    /// `list_versions()`: all version strings, in creation order.
    pub fn list_versions(&self) -> Vec<String> {
        self.versions.keys().cloned().collect()
    }

    /// `migrate_ontology(from_version, to_version, ontology)`: point the ontology dict at
    /// `to_version` (`version`, `uri`, `versionInfo`, updated in place) and return it.
    pub fn migrate_ontology(
        &self,
        from_version: &str,
        to_version: &str,
        ontology: &mut Map<String, Value>,
    ) -> Result<Map<String, Value>, SemanticaError> {
        if !self.versions.contains_key(from_version) {
            return Err(
                ValidationError::new(format!("Source version not found: {from_version}")).into(),
            );
        }
        let Some(target) = self.versions.get(to_version) else {
            return Err(
                ValidationError::new(format!("Target version not found: {to_version}")).into(),
            );
        };
        ontology.insert("version".into(), Value::String(to_version.to_string()));
        ontology.insert("uri".into(), Value::String(target.ontology_iri.clone()));
        ontology.insert(
            "versionInfo".into(),
            Value::String(target.version_info.clone()),
        );
        self.logger.info(format!(
            "Migrated ontology from {from_version} to {to_version}"
        ));
        Ok(ontology.clone())
    }

    /// `resolve_versioned_imports(ontology)`: the ontology's `imports` plus, for each import
    /// whose URI names a known version (`/v<major>.<minor>`), that version's
    /// `metadata["imports"]`. Members are unique (set semantics, first-seen order).
    pub fn resolve_versioned_imports(
        &self,
        ontology: &Map<String, Value>,
    ) -> Result<Vec<Value>, SemanticaError> {
        let imports = match ontology.get("imports") {
            Some(imports) => py_list(imports)?,
            None => Vec::new(),
        };
        let mut resolved = PySet::from_items(imports.iter())?;

        // Resolve transitive imports (basic implementation).
        for import_uri in &imports {
            let Value::String(import_uri) = import_uri else {
                return Err(SemanticaError::Type(format!(
                    "argument of type '{}' is not iterable",
                    type_name(import_uri)
                )));
            };
            if !import_uri.contains("/v") {
                continue;
            }
            let Some(captures) = VERSIONED_IMPORT.captures(import_uri) else {
                continue;
            };
            let version = &captures[1];
            if let Some(version_imports) = self
                .versions
                .get(version)
                .and_then(|record| record.metadata.get("imports"))
            {
                resolved.update(py_list(version_imports)?.iter())?;
            }
        }

        Ok(resolved.into_values())
    }
}
