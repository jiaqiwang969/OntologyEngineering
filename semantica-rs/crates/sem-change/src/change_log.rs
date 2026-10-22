//! Change Log Module (`semantica/change_management/change_log.py`).
//!
//! Standardized metadata structures for version changes across both ontology and knowledge
//! graph versioning systems:
//!
//! - [`ChangeLogEntry`]: standard, validated metadata for a version change (ISO 8601 timestamp,
//!   author e-mail, description of at most 500 characters);
//! - [`MutationRecord`]: granular log of one node/edge change (audit trail);
//! - [`ChangeLogAnalyzer`] / [`ImpactReport`] / [`generate_change_report`]: impact
//!   classification of ontology diffs ([`Severity`], [`ChangeCategory`]).
//!
//! ```
//! use sem_change::ChangeLogEntry;
//!
//! let entry = ChangeLogEntry::new(
//!     "2024-01-15T10:30:00Z",
//!     "alice@company.com",
//!     "Added Customer entity",
//!     None,
//!     None,
//! )
//! .unwrap();
//! assert!(entry.related_changes.is_empty());
//! ```

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use sem_utils::pycompat::fmt::{py_str, strip};
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{SemanticaError, ValidationError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::pycompat::{PySet, as_dict, fromisoformat, get, py_list};

/// `^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$` with Python's `$` (which also matches
/// before a final newline).
static EMAIL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}\n?\z").expect("valid regex")
});

/// Standard metadata for version changes (`@dataclass ChangeLogEntry`).
///
/// Provides a consistent structure for tracking changes across both ontology and knowledge
/// graph versioning systems. Construction validates the fields like the dataclass's
/// `__post_init__`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeLogEntry {
    /// ISO 8601 timestamp of the change.
    pub timestamp: String,
    /// E-mail address of the change author.
    pub author: String,
    /// Description of the change (max 500 characters).
    pub description: String,
    /// Optional unique identifier for the change.
    pub change_id: Option<String>,
    /// Related change identifiers.
    pub related_changes: Vec<String>,
}

impl ChangeLogEntry {
    /// `ChangeLogEntry(timestamp, author, description, change_id=None, related_changes=[])`.
    ///
    /// Fails with `ValidationError` when the timestamp is not ISO 8601 (it must contain `T` and
    /// parse with `datetime.fromisoformat` after `Z` -> `+00:00`), the author is not an e-mail
    /// address, or the description is longer than 500 characters or blank.
    pub fn new(
        timestamp: impl Into<String>,
        author: impl Into<String>,
        description: impl Into<String>,
        change_id: Option<String>,
        related_changes: Option<Vec<String>>,
    ) -> Result<ChangeLogEntry, SemanticaError> {
        let entry = ChangeLogEntry {
            timestamp: timestamp.into(),
            author: author.into(),
            description: description.into(),
            change_id,
            related_changes: related_changes.unwrap_or_default(),
        };
        entry.validate_timestamp()?;
        entry.validate_author()?;
        entry.validate_description()?;
        Ok(entry)
    }

    /// `_validate_timestamp`.
    fn validate_timestamp(&self) -> Result<(), SemanticaError> {
        let valid = self.timestamp.contains('T')
            && fromisoformat(&self.timestamp.replace('Z', "+00:00")).is_ok();
        if valid {
            Ok(())
        } else {
            Err(ValidationError::new(format!(
                "Invalid timestamp format: {}. Expected ISO 8601 format.",
                self.timestamp
            ))
            .into())
        }
    }

    /// `_validate_author`.
    fn validate_author(&self) -> Result<(), SemanticaError> {
        if EMAIL_PATTERN.is_match(&self.author) {
            Ok(())
        } else {
            Err(ValidationError::new(format!("Invalid email format: {}", self.author)).into())
        }
    }

    /// `_validate_description`.
    fn validate_description(&self) -> Result<(), SemanticaError> {
        let length = self.description.chars().count();
        if length > 500 {
            return Err(ValidationError::new(format!(
                "Description too long: {length} characters (max 500)"
            ))
            .into());
        }
        if strip(&self.description).is_empty() {
            return Err(ValidationError::new("Description cannot be empty").into());
        }
        Ok(())
    }

    /// `ChangeLogEntry.create_now(author, description, change_id=None, related_changes=None)`:
    /// an entry stamped with the current local time (`datetime.now().isoformat()`).
    pub fn create_now(
        author: impl Into<String>,
        description: impl Into<String>,
        change_id: Option<String>,
        related_changes: Option<Vec<String>>,
    ) -> Result<ChangeLogEntry, SemanticaError> {
        ChangeLogEntry::new(
            PyDateTime::now(false).isoformat('T'),
            author,
            description,
            change_id,
            Some(related_changes.unwrap_or_default()),
        )
    }
}

/// Granular record of a single change to a graph entity (`@dataclass MutationRecord`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MutationRecord {
    /// ISO 8601 timestamp of the mutation.
    pub timestamp: String,
    /// `ADD_NODE`, `UPDATE_NODE`, `REMOVE_NODE`, `ADD_EDGE`, `UPDATE_EDGE` or `REMOVE_EDGE`.
    pub operation: String,
    /// The id of the affected node or edge.
    pub entity_id: String,
    /// The state of the entity after the mutation.
    pub payload: Map<String, Value>,
    /// Optional association with a saved snapshot version.
    pub version_label: Option<String>,
}

impl MutationRecord {
    /// `MutationRecord(timestamp, operation, entity_id, payload, version_label=None)`.
    pub fn new(
        timestamp: impl Into<String>,
        operation: impl Into<String>,
        entity_id: impl Into<String>,
        payload: Map<String, Value>,
        version_label: Option<String>,
    ) -> MutationRecord {
        MutationRecord {
            timestamp: timestamp.into(),
            operation: operation.into(),
            entity_id: entity_id.into(),
            payload,
            version_label,
        }
    }
}

/// Impact severity (`class Severity(Enum)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// `"critical"`.
    Critical,
    /// `"high"`.
    High,
    /// `"medium"`.
    Medium,
    /// `"low"`.
    Low,
    /// `"info"`.
    Info,
}

impl Severity {
    /// `member.value`.
    pub fn value(self) -> &'static str {
        match self {
            Severity::Critical => "critical",
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
            Severity::Info => "info",
        }
    }

    /// `member.name`.
    pub fn name(self) -> &'static str {
        match self {
            Severity::Critical => "CRITICAL",
            Severity::High => "HIGH",
            Severity::Medium => "MEDIUM",
            Severity::Low => "LOW",
            Severity::Info => "INFO",
        }
    }
}

impl fmt::Display for Severity {
    /// `str(member)`: `Severity.CRITICAL`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Severity.{}", self.name())
    }
}

/// Change category (`class ChangeCategory(Enum)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeCategory {
    /// `"breaking"`.
    Breaking,
    /// `"potentially_breaking"`.
    PotentiallyBreaking,
    /// `"non_breaking"`.
    NonBreaking,
    /// `"unknown"`.
    Unknown,
}

impl ChangeCategory {
    /// `member.value`.
    pub fn value(self) -> &'static str {
        match self {
            ChangeCategory::Breaking => "breaking",
            ChangeCategory::PotentiallyBreaking => "potentially_breaking",
            ChangeCategory::NonBreaking => "non_breaking",
            ChangeCategory::Unknown => "unknown",
        }
    }

    /// `member.name`.
    pub fn name(self) -> &'static str {
        match self {
            ChangeCategory::Breaking => "BREAKING",
            ChangeCategory::PotentiallyBreaking => "POTENTIALLY_BREAKING",
            ChangeCategory::NonBreaking => "NON_BREAKING",
            ChangeCategory::Unknown => "UNKNOWN",
        }
    }
}

impl fmt::Display for ChangeCategory {
    /// `str(member)`: `ChangeCategory.BREAKING`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChangeCategory.{}", self.name())
    }
}

/// Structured impact analysis report (`@dataclass ImpactReport`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImpactReport {
    /// Creation time (`datetime.now().isoformat()` by default).
    pub timestamp: String,
    /// Summary counters (`{"total_changes": n}` or `{"error": ...}`).
    pub summary: Map<String, Value>,
    /// Breaking changes.
    pub breaking_changes: Vec<Map<String, Value>>,
    /// Potentially breaking changes.
    pub potentially_breaking: Vec<Map<String, Value>>,
    /// Safe (non-breaking or unknown) changes.
    pub safe_changes: Vec<Map<String, Value>>,
    /// Recommendations derived from the classification.
    pub recommendations: Vec<String>,
}

impl Default for ImpactReport {
    /// `ImpactReport()`: empty, stamped with the current local time.
    fn default() -> Self {
        ImpactReport {
            timestamp: PyDateTime::now(false).isoformat('T'),
            summary: Map::new(),
            breaking_changes: Vec::new(),
            potentially_breaking: Vec::new(),
            safe_changes: Vec::new(),
            recommendations: Vec::new(),
        }
    }
}

impl ImpactReport {
    /// `ImpactReport()`.
    pub fn new() -> ImpactReport {
        ImpactReport::default()
    }

    /// `report.to_dict()`.
    pub fn to_dict(&self) -> Map<String, Value> {
        let objects = |items: &[Map<String, Value>]| {
            Value::Array(items.iter().cloned().map(Value::Object).collect())
        };
        let mut classification = Map::new();
        classification.insert("breaking".into(), objects(&self.breaking_changes));
        classification.insert(
            "potentially_breaking".into(),
            objects(&self.potentially_breaking),
        );
        classification.insert("safe".into(), objects(&self.safe_changes));
        let mut out = Map::new();
        out.insert("timestamp".into(), Value::String(self.timestamp.clone()));
        out.insert("summary".into(), Value::Object(self.summary.clone()));
        out.insert(
            "impact_classification".into(),
            Value::Object(classification),
        );
        out.insert(
            "recommendations".into(),
            Value::Array(
                self.recommendations
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
        out
    }
}

/// One flattened change from a diff (`all_changes` items in `analyze`).
struct Change {
    uri: Value,
    entity_type: &'static str,
    change_type: &'static str,
    changes: Value,
}

/// Analyzes ontology diffs and classifies impact severity (`class ChangeLogAnalyzer`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeLogAnalyzer;

impl ChangeLogAnalyzer {
    /// `VALIDITY_CONSTRAINTS`: field changes that can restrict valid data.
    pub const VALIDITY_CONSTRAINTS: [&'static str; 4] =
        ["domain", "range", "cardinality", "max_cardinality"];
    /// `STRUCTURAL_FIELDS`: field changes that modify the hierarchy.
    pub const STRUCTURAL_FIELDS: [&'static str; 4] = [
        "subclasses",
        "superclasses",
        "equivalent_to",
        "disjoint_with",
    ];

    /// `ChangeLogAnalyzer()`.
    pub fn new() -> ChangeLogAnalyzer {
        ChangeLogAnalyzer
    }

    /// `analyze(diff)`: classify every added/removed/changed class and property of a diff (as
    /// produced by [`crate::VersionManager::diff_ontologies`]) into an [`ImpactReport`].
    ///
    /// An empty diff yields a report whose summary is `{"error": "Empty diff provided"}`.
    /// Malformed items fail like Python does (`AttributeError` for non-dict items, `TypeError`
    /// for non-iterable sections or unhashable constraint values).
    pub fn analyze(&self, diff: &Map<String, Value>) -> Result<ImpactReport, SemanticaError> {
        let mut report = ImpactReport::new();
        if diff.is_empty() {
            let mut summary = Map::new();
            summary.insert("error".into(), Value::String("Empty diff provided".into()));
            report.summary = summary;
            return Ok(report);
        }

        let mut all_changes = Vec::new();
        for (key, entity_type, change_type) in [
            ("added_classes", "class", "added"),
            ("added_properties", "property", "added"),
            ("removed_classes", "class", "removed"),
            ("removed_properties", "property", "removed"),
            ("changed_classes", "class", "modified"),
            ("changed_properties", "property", "modified"),
        ] {
            let items = match diff.get(key) {
                Some(section) => py_list(section)?,
                None => Vec::new(),
            };
            for item in &items {
                let item = as_dict(item, "get")?;
                let uri = match item.get("uri") {
                    Some(uri) => uri.clone(),
                    None => item
                        .get("name")
                        .cloned()
                        .unwrap_or_else(|| Value::String("unknown".into())),
                };
                let changes = item
                    .get("changes")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(Map::new()));
                all_changes.push(Change {
                    uri,
                    entity_type,
                    change_type,
                    changes,
                });
            }
        }

        let mut summary = Map::new();
        summary.insert("total_changes".into(), Value::from(all_changes.len()));
        report.summary = summary;

        for change in &all_changes {
            let (severity, category, description, mitigation) = self.classify_change(change)?;
            let mut entry = Map::new();
            entry.insert("entity_uri".into(), change.uri.clone());
            entry.insert(
                "entity_type".into(),
                Value::String(change.entity_type.into()),
            );
            entry.insert(
                "change_type".into(),
                Value::String(change.change_type.into()),
            );
            entry.insert("description".into(), Value::String(description));
            entry.insert("severity".into(), Value::String(severity.value().into()));
            entry.insert("mitigation".into(), Value::String(mitigation.into()));
            match category {
                ChangeCategory::Breaking => report.breaking_changes.push(entry),
                ChangeCategory::PotentiallyBreaking => report.potentially_breaking.push(entry),
                _ => report.safe_changes.push(entry),
            }
        }

        self.generate_recommendations(&mut report);
        Ok(report)
    }

    /// `_classify_change(change)`.
    fn classify_change(
        &self,
        change: &Change,
    ) -> Result<(Severity, ChangeCategory, String, &'static str), SemanticaError> {
        let uri = py_str(&change.uri);
        match change.change_type {
            "removed" => {
                if change.entity_type == "class" {
                    Ok((
                        Severity::Critical,
                        ChangeCategory::Breaking,
                        format!("Class {uri} removed."),
                        "Migrate orphaned instances.",
                    ))
                } else {
                    Ok((
                        Severity::Critical,
                        ChangeCategory::Breaking,
                        format!("Property {uri} removed."),
                        "Migrate property values.",
                    ))
                }
            }
            "added" => Ok((
                Severity::Info,
                ChangeCategory::NonBreaking,
                format!("New {} {uri} added.", change.entity_type),
                "No action required.",
            )),
            "modified" => self.analyze_field_changes(&uri, &change.changes),
            _ => Ok((
                Severity::Low,
                ChangeCategory::Unknown,
                format!("Unknown change for {uri}"),
                "Manual review required.",
            )),
        }
    }

    /// `_analyze_field_changes(uri, field_changes)`.
    fn analyze_field_changes(
        &self,
        uri: &str,
        field_changes: &Value,
    ) -> Result<(Severity, ChangeCategory, String, &'static str), SemanticaError> {
        let mut has_restriction = false;
        let mut has_structural = false;

        for (field, vals) in as_dict(field_changes, "items")? {
            if Self::VALIDITY_CONSTRAINTS.contains(&field.as_str()) {
                let vals = as_dict(vals, "get")?;
                let (old_val, new_val) = (get(vals, "old"), get(vals, "new"));
                if old_val.is_null() || new_val.is_null() {
                    has_restriction = true;
                    continue;
                }
                // If the new constraint set is smaller, it is a restriction.
                let as_set = |value: &Value| match value {
                    Value::Array(items) => PySet::from_items(items.iter()),
                    single => PySet::from_items(std::iter::once(single)),
                };
                let old_set = as_set(old_val)?;
                let new_set = as_set(new_val)?;
                if new_set.is_proper_subset(&old_set) {
                    has_restriction = true;
                }
            } else if Self::STRUCTURAL_FIELDS.contains(&field.as_str()) {
                has_structural = true;
            }
        }

        if has_restriction {
            return Ok((
                Severity::High,
                ChangeCategory::Breaking,
                format!("Domain/range restricted on {uri}"),
                "Validate existing data against new constraints.",
            ));
        }
        if has_structural {
            return Ok((
                Severity::Medium,
                ChangeCategory::PotentiallyBreaking,
                format!("Hierarchy modified for {uri}"),
                "Check dependent reasoning chains.",
            ));
        }
        Ok((
            Severity::Low,
            ChangeCategory::NonBreaking,
            format!("Safe annotations updated for {uri}"),
            "No action required.",
        ))
    }

    /// `_generate_recommendations(report)`.
    fn generate_recommendations(&self, report: &mut ImpactReport) {
        if !report.breaking_changes.is_empty() {
            report
                .recommendations
                .push("[BREAKING] Schedule downtime or validate existing data.".into());
        }
        if !report.potentially_breaking.is_empty() {
            report
                .recommendations
                .push("[WARNING] Run full regression tests on queries.".into());
        }
        if report.breaking_changes.is_empty() && report.potentially_breaking.is_empty() {
            report
                .recommendations
                .push("[SAFE] Minor version bump sufficient.".into());
        }
    }
}

/// `generate_change_report(diff)`: the public API for impact reports,
/// `ChangeLogAnalyzer().analyze(diff).to_dict()`.
pub fn generate_change_report(
    diff: &Map<String, Value>,
) -> Result<Map<String, Value>, SemanticaError> {
    Ok(ChangeLogAnalyzer::new().analyze(diff)?.to_dict())
}
