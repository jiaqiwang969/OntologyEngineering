//! Enhanced Version Managers Module (`semantica/change_management/managers.py`).
//!
//! Version management for knowledge graphs ([`TemporalVersionManager`]) and ontologies
//! ([`OntologyVersionManager`]) on top of a [`VersionStorage`] backend, with checksummed
//! snapshots, standardized change metadata ([`ChangeLogEntry`] validation), entity- and
//! relationship-level diffs, tags, a granular mutation audit trail and guarded restores.
//!
//! [`BaseVersionManager`] is the abstract base: the snapshot/compare contract plus the shared
//! `list_versions` / `get_version` / `verify_checksum` behaviour. Managers take `&self`
//! everywhere and are `Send + Sync`, so one manager can be shared between threads (the
//! storage serialises access).
//!
//! The graph a manager tracks or restores is reached through [`MutationTrackedGraph`]; the
//! triplet store `prune_versions` drops graphs from is reached through [`TripletStoreQuery`].

use std::borrow::Cow;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{
    hash_key, py_eq, py_str, slice_bounds, strip, strip_chars, truthy, type_name,
};
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{ProcessingError, SemanticaError, ValidationError, get_logger};
use serde_json::{Map, Value};

use crate::change_log::{ChangeLogEntry, MutationRecord};
use crate::pycompat::{
    PySet, as_dict, attribute_error, get, py_list_or_empty, quote, sort_desc_by_key,
};
use crate::version_storage::{
    InMemoryVersionStorage, SQLiteVersionStorage, VersionStorage, compute_checksum, verify_checksum,
};

/// Keyword options of `create_snapshot(..., **options)`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapshotOptions {
    /// `metadata`: stored under the snapshot's `metadata` key (default `{}`).
    pub metadata: Option<Value>,
}

impl SnapshotOptions {
    /// Options carrying `metadata`.
    pub fn with_metadata(metadata: Value) -> SnapshotOptions {
        SnapshotOptions {
            metadata: Some(metadata),
        }
    }

    fn metadata_value(&self) -> Value {
        self.metadata
            .clone()
            .unwrap_or_else(|| Value::Object(Map::new()))
    }
}

/// `BaseVersionManager.__init__(storage_path)`: a [`SQLiteVersionStorage`] at `storage_path`,
/// or an [`InMemoryVersionStorage`] when no (or an empty) path is given; the choice is logged
/// at INFO on `logger`.
pub fn init_storage(
    storage_path: Option<&str>,
    logger: &Logger,
) -> Result<Arc<dyn VersionStorage>, SemanticaError> {
    match storage_path.filter(|path| !path.is_empty()) {
        Some(path) => {
            let storage = SQLiteVersionStorage::new(path)?;
            logger.info(format!("Initialized with SQLite storage: {path}"));
            Ok(Arc::new(storage))
        }
        None => {
            let storage = InMemoryVersionStorage::new();
            logger.info("Initialized with in-memory storage");
            Ok(Arc::new(storage))
        }
    }
}

/// Abstract base class for enhanced version managers (`class BaseVersionManager(ABC)`).
///
/// Implementors provide the storage and the two abstract methods; listing, retrieval and
/// checksum verification are shared.
pub trait BaseVersionManager {
    /// `self.storage`: the backend snapshots live in.
    fn storage(&self) -> &Arc<dyn VersionStorage>;

    /// Create a versioned snapshot of the data (abstract).
    fn create_snapshot(
        &self,
        data: &Value,
        version_label: &str,
        author: &str,
        description: &str,
        options: &SnapshotOptions,
    ) -> Result<Map<String, Value>, SemanticaError>;

    /// Compare two versions and return detailed differences (abstract).
    fn compare_versions(
        &self,
        version1: &Value,
        version2: &Value,
    ) -> Result<Map<String, Value>, SemanticaError>;

    /// List all version snapshots (metadata rows).
    fn list_versions(&self) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        self.storage().list_all()
    }

    /// Retrieve a specific version by label.
    fn get_version(&self, label: &str) -> Result<Option<Map<String, Value>>, SemanticaError> {
        self.storage().get(label)
    }

    /// Verify the integrity of a snapshot using its checksum.
    fn verify_checksum(&self, snapshot: &Map<String, Value>) -> bool {
        verify_checksum(snapshot)
    }
}

/// The mutation callback a tracked graph calls for every node/edge change:
/// `(operation, entity_id, payload, version_label)`.
pub type MutationCallback = Arc<
    dyn Fn(&str, &str, &Map<String, Value>, Option<&str>) -> Result<(), SemanticaError>
        + Send
        + Sync,
>;

/// The parts of a graph (`ContextGraph`) that [`TemporalVersionManager::attach_to_graph`] and
/// [`TemporalVersionManager::restore_snapshot`] use.
pub trait MutationTrackedGraph {
    /// `graph.mutation_callback = callback`.
    fn set_mutation_callback(&mut self, callback: MutationCallback);

    /// `getattr(graph, "graph_id", ...)`: `None` when the graph has no id.
    fn graph_id(&self) -> Option<String> {
        None
    }

    /// `getattr(graph, "_suspend_mutation_callback", False)`.
    fn suspend_mutation_callback(&self) -> bool;

    /// `graph._suspend_mutation_callback = value`.
    fn set_suspend_mutation_callback(&mut self, value: bool);

    /// `graph.from_dict(payload)`: replace the graph's content with `{"nodes": [...],
    /// "edges": [...]}`.
    #[allow(clippy::wrong_self_convention)] // the original's method name
    fn from_dict(&mut self, payload: &Map<String, Value>) -> Result<(), SemanticaError>;
}

/// The part of a triplet store [`TemporalVersionManager::prune_versions`] uses.
pub trait TripletStoreQuery {
    /// `triplet_store.execute_query(query)`.
    fn execute_query(&self, query: &str) -> Result<Value, SemanticaError>;
}

/// `_extract_graph_collections(graph)`: the entity and relationship lists of a graph payload
/// with `nodes`/`edges` preferred over the legacy `entities`/`relationships` keys.
fn extract_graph_collections(graph: &Value) -> Result<(Vec<Value>, Vec<Value>), SemanticaError> {
    let Value::Object(graph) = graph else {
        return Err(ValidationError::new("Graph must be provided as a dictionary").into());
    };
    let has_node_schema = graph.contains_key("nodes") || graph.contains_key("edges");
    let has_legacy_schema = graph.contains_key("entities") || graph.contains_key("relationships");
    if !(has_node_schema || has_legacy_schema) {
        return Err(ValidationError::new(
            "Graph dictionary must contain 'nodes'/'edges' or 'entities'/'relationships'",
        )
        .into());
    }
    let mut entities = get(graph, "nodes");
    if entities.is_null() {
        entities = get(graph, "entities");
    }
    let mut relationships = get(graph, "edges");
    if relationships.is_null() {
        relationships = get(graph, "relationships");
    }
    Ok((
        py_list_or_empty(entities)?,
        py_list_or_empty(relationships)?,
    ))
}

/// `_relationship_key(relationship)`: `"{source}|{type}|{target}"`.
fn relationship_key(relationship: &Value) -> Result<String, SemanticaError> {
    let relationship = as_dict(relationship, "get")?;
    let empty = Value::String(String::new());
    let source = relationship.get("source").unwrap_or(&empty);
    let target = relationship.get("target").unwrap_or(&empty);
    let rel_type = relationship
        .get("type")
        .or_else(|| relationship.get("relationship"))
        .unwrap_or(&empty);
    Ok(format!(
        "{}|{}|{}",
        py_str(source),
        py_str(rel_type),
        py_str(target)
    ))
}

/// `_compute_entity_changes` / `_compute_relationship_changes`: `{key: {"from": v1, "to": v2}}`
/// for every key whose values differ (keys of the first record first).
fn compute_record_changes(
    record1: &Value,
    record2: &Value,
) -> Result<Map<String, Value>, SemanticaError> {
    let record1 = as_dict(record1, "keys")?;
    let record2 = as_dict(record2, "keys")?;
    let mut changes = Map::new();
    let keys = record1
        .keys()
        .chain(record2.keys().filter(|k| !record1.contains_key(*k)));
    for key in keys {
        let val1 = get(record1, key);
        let val2 = get(record2, key);
        if !py_eq(val1, val2) {
            let mut change = Map::new();
            change.insert("from".into(), val1.clone());
            change.insert("to".into(), val2.clone());
            changes.insert(key.clone(), Value::Object(change));
        }
    }
    Ok(changes)
}

/// `{n.get("id", str(i)): n for i, n in enumerate(entities)}`: Python-hash key -> (id, node).
fn index_nodes(entities: &[Value]) -> Result<IndexMap<String, (Value, Value)>, SemanticaError> {
    let mut nodes: IndexMap<String, (Value, Value)> = IndexMap::new();
    for (i, node) in entities.iter().enumerate() {
        let map = as_dict(node, "get")?;
        let id = map
            .get("id")
            .cloned()
            .unwrap_or_else(|| Value::String(i.to_string()));
        let key = hash_key(&id)?;
        match nodes.get_mut(&key) {
            Some(slot) => slot.1 = node.clone(),
            None => {
                nodes.insert(key, (id, node.clone()));
            }
        }
    }
    Ok(nodes)
}

/// `{self._relationship_key(e): e for e in relationships}`.
fn index_edges(relationships: &[Value]) -> Result<IndexMap<String, Value>, SemanticaError> {
    let mut edges = IndexMap::new();
    for edge in relationships {
        edges.insert(relationship_key(edge)?, edge.clone());
    }
    Ok(edges)
}

/// Knowledge-graph version management (`class TemporalVersionManager(BaseVersionManager)`):
/// persistent checksummed snapshots, entity-level diffs, tags, a mutation audit trail and
/// guarded restores.
pub struct TemporalVersionManager {
    /// `self.storage`.
    pub storage: Arc<dyn VersionStorage>,
    /// `self.logger` (`semantica.temporalversionmanager`).
    pub logger: Logger,
    /// `self.config` (the `**config` keywords).
    pub config: Map<String, Value>,
}

impl fmt::Debug for TemporalVersionManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TemporalVersionManager")
            .field("logger", &self.logger)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

/// `record_mutation` against a storage backend (shared by the method and the graph callback).
fn record_mutation_in(
    storage: &dyn VersionStorage,
    logger: &Logger,
    operation: &str,
    entity_id: &str,
    payload: &Map<String, Value>,
    version_label: Option<&str>,
) -> Result<(), SemanticaError> {
    let record = MutationRecord::new(
        PyDateTime::now(false).isoformat('T'),
        operation,
        entity_id,
        payload.clone(),
        version_label.map(str::to_string),
    );
    let mut mutation = Map::new();
    mutation.insert("timestamp".into(), Value::String(record.timestamp));
    mutation.insert("operation".into(), Value::String(record.operation));
    mutation.insert("entity_id".into(), Value::String(record.entity_id));
    mutation.insert("payload".into(), Value::Object(record.payload));
    mutation.insert(
        "version_label".into(),
        record
            .version_label
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    storage.save_mutation(&mutation)?;
    logger.debug(format!("Recorded mutation: {operation} on {entity_id}"));
    Ok(())
}

impl TemporalVersionManager {
    /// `TemporalVersionManager(storage_path=None, **config)`: SQLite storage at `storage_path`,
    /// in-memory storage otherwise.
    pub fn new(
        storage_path: Option<&str>,
        config: Option<Map<String, Value>>,
    ) -> Result<TemporalVersionManager, SemanticaError> {
        let logger = get_logger("temporalversionmanager");
        let storage = init_storage(storage_path, &logger)?;
        Ok(TemporalVersionManager {
            storage,
            logger,
            config: config.unwrap_or_default(),
        })
    }

    /// `create_snapshot(graph, version_label, author, description, **options)`: validate the
    /// change metadata, store a checksummed snapshot holding both the `nodes`/`edges` and the
    /// legacy `entities`/`relationships` lists, label the pending mutations with
    /// `version_label`, and return the snapshot.
    ///
    /// `ValidationError` for an invalid author/description, a non-dict graph, a graph without
    /// node or entity collections, or a missing/duplicate label; `ProcessingError` when the
    /// storage fails.
    pub fn create_snapshot(
        &self,
        graph: &Value,
        version_label: &str,
        author: &str,
        description: &str,
        options: &SnapshotOptions,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let change_entry = ChangeLogEntry::new(
            PyDateTime::now(false).isoformat('T'),
            author,
            description,
            None,
            None,
        )?;
        let (entities, relationships) = extract_graph_collections(graph)?;

        let mut snapshot = Map::new();
        snapshot.insert("label".into(), Value::String(version_label.to_string()));
        snapshot.insert("timestamp".into(), Value::String(change_entry.timestamp));
        snapshot.insert("author".into(), Value::String(change_entry.author));
        snapshot.insert(
            "description".into(),
            Value::String(change_entry.description),
        );
        // Both key shapes are stored during the migration window so older readers and newer
        // ContextGraph restore paths both work.
        snapshot.insert("nodes".into(), Value::Array(entities.clone()));
        snapshot.insert("edges".into(), Value::Array(relationships.clone()));
        snapshot.insert("entities".into(), Value::Array(entities));
        snapshot.insert("relationships".into(), Value::Array(relationships));
        snapshot.insert("metadata".into(), options.metadata_value());

        let checksum = compute_checksum(&snapshot);
        snapshot.insert("checksum".into(), Value::String(checksum));
        self.storage.save(&snapshot)?;
        self.storage
            .assign_version_to_unlabeled_mutations(version_label)?;
        self.logger
            .info(format!("Created snapshot '{version_label}' by {author}"));
        Ok(snapshot)
    }

    /// A version given as a label (looked up in storage) or as a snapshot dict.
    fn resolve_version<'a>(&self, version: &'a Value) -> Result<Cow<'a, Value>, SemanticaError> {
        match version {
            Value::String(label) => match self.storage.get(label)? {
                Some(snapshot) if !snapshot.is_empty() => Ok(Cow::Owned(Value::Object(snapshot))),
                _ => Err(ValidationError::new(format!("Version not found: {label}")).into()),
            },
            other => Ok(Cow::Borrowed(other)),
        }
    }

    /// `compare_versions(v1_label_or_dict, v2_label_or_dict, comparison_metrics=None)`: detailed
    /// entity- and relationship-level differences between two versions, each given as a label
    /// (string) or a snapshot dict. `comparison_metrics` is accepted and unused, as in the
    /// original.
    ///
    /// The result holds `version1`, `version2`, a `summary` of counts and the lists
    /// `entities_added/removed/modified`, `relationships_added/removed/modified` and their
    /// `nodes_*`/`edges_*` aliases. Nodes are matched by `id` (their position when absent),
    /// relationships by `source|type|target`.
    pub fn compare_versions(
        &self,
        v1_label_or_dict: &Value,
        v2_label_or_dict: &Value,
        comparison_metrics: Option<&[String]>,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let _ = comparison_metrics;
        let version1 = self.resolve_version(v1_label_or_dict)?;
        let version2 = self.resolve_version(v2_label_or_dict)?;

        let detailed_diff = self.compute_detailed_diff(&version1, &version2)?;

        let mut summary = Map::new();
        for key in [
            "entities_added",
            "entities_removed",
            "entities_modified",
            "relationships_added",
            "relationships_removed",
            "relationships_modified",
            "nodes_added",
            "nodes_removed",
            "nodes_modified",
            "edges_added",
            "edges_removed",
            "edges_modified",
        ] {
            let count = detailed_diff
                .get(key)
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            summary.insert(key.into(), Value::from(count));
        }

        let label_of = |version: &Value| match version {
            Value::Object(map) => map
                .get("label")
                .cloned()
                .unwrap_or_else(|| Value::String("unknown".into())),
            _ => Value::String("unknown".into()),
        };
        let mut result = Map::new();
        result.insert("version1".into(), label_of(&version1));
        result.insert("version2".into(), label_of(&version2));
        result.insert("summary".into(), Value::Object(summary));
        result.extend(detailed_diff);
        Ok(result)
    }

    /// `_compute_detailed_diff(version1, version2)`.
    fn compute_detailed_diff(
        &self,
        version1: &Value,
        version2: &Value,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let (v1_entities, v1_relationships) = extract_graph_collections(version1)?;
        let (v2_entities, v2_relationships) = extract_graph_collections(version2)?;

        let nodes1 = index_nodes(&v1_entities)?;
        let nodes2 = index_nodes(&v2_entities)?;
        let edges1 = index_edges(&v1_relationships)?;
        let edges2 = index_edges(&v2_relationships)?;

        let nodes_added: Vec<Value> = nodes2
            .iter()
            .filter(|(k, _)| !nodes1.contains_key(*k))
            .map(|(_, (_, node))| node.clone())
            .collect();
        let nodes_removed: Vec<Value> = nodes1
            .iter()
            .filter(|(k, _)| !nodes2.contains_key(*k))
            .map(|(_, (_, node))| node.clone())
            .collect();
        let mut nodes_modified = Vec::new();
        for (key, (id, before)) in &nodes1 {
            let Some((_, after)) = nodes2.get(key) else {
                continue;
            };
            if !py_eq(before, after) {
                let changes = compute_record_changes(before, after)?;
                let mut entry = Map::new();
                entry.insert("id".into(), id.clone());
                entry.insert("before".into(), before.clone());
                entry.insert("after".into(), after.clone());
                entry.insert("changes".into(), Value::Object(changes));
                nodes_modified.push(Value::Object(entry));
            }
        }

        let edges_added: Vec<Value> = edges2
            .iter()
            .filter(|(k, _)| !edges1.contains_key(*k))
            .map(|(_, edge)| edge.clone())
            .collect();
        let edges_removed: Vec<Value> = edges1
            .iter()
            .filter(|(k, _)| !edges2.contains_key(*k))
            .map(|(_, edge)| edge.clone())
            .collect();
        let mut edges_modified = Vec::new();
        for (key, before) in &edges1 {
            let Some(after) = edges2.get(key) else {
                continue;
            };
            if !py_eq(before, after) {
                let changes = compute_record_changes(before, after)?;
                let mut entry = Map::new();
                entry.insert("key".into(), Value::String(key.clone()));
                entry.insert("before".into(), before.clone());
                entry.insert("after".into(), after.clone());
                entry.insert("changes".into(), Value::Object(changes));
                edges_modified.push(Value::Object(entry));
            }
        }

        let (nodes_added, nodes_removed, nodes_modified) = (
            Value::Array(nodes_added),
            Value::Array(nodes_removed),
            Value::Array(nodes_modified),
        );
        let (edges_added, edges_removed, edges_modified) = (
            Value::Array(edges_added),
            Value::Array(edges_removed),
            Value::Array(edges_modified),
        );
        let mut diff = Map::new();
        diff.insert("entities_added".into(), nodes_added.clone());
        diff.insert("entities_removed".into(), nodes_removed.clone());
        diff.insert("entities_modified".into(), nodes_modified.clone());
        diff.insert("relationships_added".into(), edges_added.clone());
        diff.insert("relationships_removed".into(), edges_removed.clone());
        diff.insert("relationships_modified".into(), edges_modified.clone());
        diff.insert("nodes_added".into(), nodes_added);
        diff.insert("nodes_removed".into(), nodes_removed);
        diff.insert("nodes_modified".into(), nodes_modified);
        diff.insert("edges_added".into(), edges_added);
        diff.insert("edges_removed".into(), edges_removed);
        diff.insert("edges_modified".into(), edges_modified);
        Ok(diff)
    }

    /// `prune_versions(keep_last_n=5, triplet_store=None)`: delete all but the `keep_last_n`
    /// most recent snapshots (by timestamp; Python slice semantics, so a negative value keeps
    /// all but that many) and, when a triplet store is given, `DROP SILENT GRAPH` each pruned
    /// snapshot's `graph_uri` (failures are logged, not raised).
    ///
    /// Returns `{"pruned_count", "pruned_versions", "retained_count"}`.
    pub fn prune_versions(
        &self,
        keep_last_n: Option<i64>,
        triplet_store: Option<&dyn TripletStoreQuery>,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let keep_last_n = keep_last_n.unwrap_or(5);
        let mut all_versions = self.list_versions()?;
        sort_desc_by_key(&mut all_versions, |v| {
            v.get("timestamp")
                .cloned()
                .unwrap_or_else(|| Value::String(String::new()))
        })?;

        let (lo, hi) = slice_bounds(all_versions.len(), Some(keep_last_n), None);
        let mut deleted_labels = Vec::new();
        for version in all_versions.iter().skip(lo).take(hi.saturating_sub(lo)) {
            let label = get(version, "label");
            let graph_uri = get(version, "graph_uri");
            let deleted = match label {
                Value::Null => false,
                Value::String(s) => self.storage.delete(s)?,
                other => self.storage.delete(&py_str(other))?,
            };
            if deleted {
                deleted_labels.push(label.clone());
                if let Some(store) = triplet_store.filter(|_| truthy(graph_uri)) {
                    let safe_graph_uri = sanitize_graph_uri(graph_uri);
                    match store.execute_query(&format!("DROP SILENT GRAPH <{safe_graph_uri}>")) {
                        Ok(_) => self.logger.info(format!(
                            "Dropped obsolete graph {} from store",
                            py_str(graph_uri)
                        )),
                        Err(e) => self.logger.warning(format!(
                            "Failed to drop graph {} during pruning: {e}",
                            py_str(graph_uri)
                        )),
                    }
                }
            }
        }

        self.logger.info(format!(
            "Pruned {} old versions, kept {keep_last_n}",
            deleted_labels.len()
        ));
        let pruned_count = deleted_labels.len();
        let retained_count = all_versions.len() as i64 - pruned_count as i64;
        let mut result = Map::new();
        result.insert("pruned_count".into(), Value::from(pruned_count));
        result.insert("pruned_versions".into(), Value::Array(deleted_labels));
        result.insert("retained_count".into(), Value::from(retained_count));
        Ok(result)
    }

    /// `attach_to_graph(graph)`: install this manager's [`Self::record_mutation`] as the graph's
    /// mutation callback, so every node/edge change lands in the audit trail.
    pub fn attach_to_graph(&self, graph: &mut dyn MutationTrackedGraph) {
        let storage = Arc::clone(&self.storage);
        let logger = self.logger.clone();
        let callback: MutationCallback = Arc::new(
            move |operation: &str,
                  entity_id: &str,
                  payload: &Map<String, Value>,
                  version_label: Option<&str>| {
                record_mutation_in(
                    storage.as_ref(),
                    &logger,
                    operation,
                    entity_id,
                    payload,
                    version_label,
                )
            },
        );
        graph.set_mutation_callback(callback);
        self.logger.info(format!(
            "Attached mutation tracking to graph: {}",
            graph.graph_id().unwrap_or_else(|| "unknown".to_string())
        ));
    }

    /// `record_mutation(operation, entity_id, payload, version_label=None)`: append one
    /// [`MutationRecord`] (stamped now) to the storage's mutation log.
    pub fn record_mutation(
        &self,
        operation: &str,
        entity_id: &str,
        payload: &Map<String, Value>,
        version_label: Option<&str>,
    ) -> Result<(), SemanticaError> {
        record_mutation_in(
            self.storage.as_ref(),
            &self.logger,
            operation,
            entity_id,
            payload,
            version_label,
        )
    }

    /// `tag_version(version_label, tag_name)`: name a stored version (`ValidationError` if the
    /// version does not exist).
    pub fn tag_version(&self, version_label: &str, tag_name: &str) -> Result<(), SemanticaError> {
        if !self.storage.exists(version_label)? {
            return Err(ValidationError::new(format!(
                "Cannot tag non-existent version: '{version_label}'"
            ))
            .into());
        }
        self.storage.save_tag(tag_name, version_label)?;
        self.logger
            .info(format!("Tagged version '{version_label}' as '{tag_name}'"));
        Ok(())
    }

    /// `list_tags()`: tag name -> version label.
    pub fn list_tags(&self) -> Result<IndexMap<String, String>, SemanticaError> {
        self.storage.list_tags()
    }

    /// `diff(version_a, version_b)`: git-like alias for [`Self::compare_versions`] on labels.
    pub fn diff(
        &self,
        version_a: &str,
        version_b: &str,
    ) -> Result<Map<String, Value>, SemanticaError> {
        self.compare_versions(
            &Value::String(version_a.to_string()),
            &Value::String(version_b.to_string()),
            None,
        )
    }

    /// `get_node_history(node_id)`: the chronological mutation history of a node.
    pub fn get_node_history(
        &self,
        node_id: &str,
    ) -> Result<Vec<Map<String, Value>>, SemanticaError> {
        self.storage.get_entity_history(node_id)
    }

    /// `restore_snapshot(graph, target_version, require_confirmation=True)`: replace the graph's
    /// content with a stored snapshot (mutation tracking suspended meanwhile).
    ///
    /// Unless `require_confirmation` is `Some(false)` this refuses with the
    /// `ProcessingError("Rollback protection active. ...")`; an unknown version is a
    /// `ValidationError`.
    pub fn restore_snapshot(
        &self,
        graph: &mut dyn MutationTrackedGraph,
        target_version: &str,
        require_confirmation: Option<bool>,
    ) -> Result<bool, SemanticaError> {
        if require_confirmation.unwrap_or(true) {
            return Err(ProcessingError::new(
                "Rollback protection active. Explicitly set require_confirmation=False to \
                 overwrite the current graph state.",
            )
            .into());
        }

        let snapshot = match self.storage.get(target_version)? {
            Some(snapshot) if !snapshot.is_empty() => snapshot,
            _ => {
                return Err(ValidationError::new(format!(
                    "Version '{target_version}' not found in storage."
                ))
                .into());
            }
        };

        self.logger.warning(format!(
            "Restoring graph to version '{target_version}' - clearing current state."
        ));

        let (entities, relationships) = extract_graph_collections(&Value::Object(snapshot))?;
        let mut graph_payload = Map::new();
        graph_payload.insert("nodes".into(), Value::Array(entities));
        graph_payload.insert("edges".into(), Value::Array(relationships));

        let previous_state = graph.suspend_mutation_callback();
        graph.set_suspend_mutation_callback(true);
        let restored = graph.from_dict(&graph_payload);
        graph.set_suspend_mutation_callback(previous_state);
        restored?;

        self.logger.info(format!(
            "Successfully restored graph to version '{target_version}'."
        ));
        Ok(true)
    }
}

/// `_sanitize_graph_uri(graph_uri)`: percent-encode unsafe characters before embedding a
/// graph URI in SPARQL.
fn sanitize_graph_uri(graph_uri: &Value) -> String {
    let text = py_str(graph_uri);
    let raw_uri = strip_chars(strip(&text), "<>");
    quote(raw_uri, "/:?&=@[]!$'()*+,%-._~")
}

impl BaseVersionManager for TemporalVersionManager {
    fn storage(&self) -> &Arc<dyn VersionStorage> {
        &self.storage
    }

    fn create_snapshot(
        &self,
        data: &Value,
        version_label: &str,
        author: &str,
        description: &str,
        options: &SnapshotOptions,
    ) -> Result<Map<String, Value>, SemanticaError> {
        TemporalVersionManager::create_snapshot(
            self,
            data,
            version_label,
            author,
            description,
            options,
        )
    }

    fn compare_versions(
        &self,
        version1: &Value,
        version2: &Value,
    ) -> Result<Map<String, Value>, SemanticaError> {
        TemporalVersionManager::compare_versions(self, version1, version2, None)
    }
}

/// `_compare_ontology_structures`: set differences of the `classes`, `properties`,
/// `individuals` and `axioms` of two structure dicts, plus a `summary` of counts.
pub(crate) fn compare_ontology_structures(
    v1_structure: &Value,
    v2_structure: &Value,
) -> Result<Map<String, Value>, SemanticaError> {
    let section = |structure: &Value, key: &str| -> Result<PySet, SemanticaError> {
        match as_dict(structure, "get")?.get(key) {
            Some(items) => PySet::from_iterable(items),
            None => Ok(PySet::default()),
        }
    };
    let mut result = Map::new();
    let mut summary = Map::new();
    for key in ["classes", "properties", "individuals", "axioms"] {
        let v1 = section(v1_structure, key)?;
        let v2 = section(v2_structure, key)?;
        let added = v2.difference(&v1);
        let removed = v1.difference(&v2);
        summary.insert(format!("{key}_added"), Value::from(added.len()));
        summary.insert(format!("{key}_removed"), Value::from(removed.len()));
        result.insert(format!("{key}_added"), Value::Array(added));
        result.insert(format!("{key}_removed"), Value::Array(removed));
    }
    result.insert("summary".into(), Value::Object(summary));
    Ok(result)
}

/// Ontology version management with structural comparison
/// (`class OntologyVersionManager(BaseVersionManager)`).
pub struct OntologyVersionManager {
    /// `self.storage`.
    pub storage: Arc<dyn VersionStorage>,
    /// `self.logger` (`semantica.ontologyversionmanager`).
    pub logger: Logger,
    /// `self.config` (the `**config` keywords).
    pub config: Map<String, Value>,
    versions: Mutex<IndexMap<String, Map<String, Value>>>,
}

impl fmt::Debug for OntologyVersionManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OntologyVersionManager")
            .field("logger", &self.logger)
            .field("config", &self.config)
            .field("versions", &self.versions())
            .finish_non_exhaustive()
    }
}

impl OntologyVersionManager {
    /// `OntologyVersionManager(storage_path=None, **config)`.
    pub fn new(
        storage_path: Option<&str>,
        config: Option<Map<String, Value>>,
    ) -> Result<OntologyVersionManager, SemanticaError> {
        let logger = get_logger("ontologyversionmanager");
        let storage = init_storage(storage_path, &logger)?;
        Ok(OntologyVersionManager {
            storage,
            logger,
            config: config.unwrap_or_default(),
            versions: Mutex::new(IndexMap::new()),
        })
    }

    fn versions_lock(&self) -> MutexGuard<'_, IndexMap<String, Map<String, Value>>> {
        self.versions.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `self.versions`: the snapshots this manager created, by label (in-memory tracking kept
    /// for compatibility).
    pub fn versions(&self) -> IndexMap<String, Map<String, Value>> {
        self.versions_lock().clone()
    }

    /// `create_snapshot(ontology_data, version_label, author, description, **options)`: a
    /// checksummed snapshot of the ontology's `uri` (as `ontology_iri`), `version_info` and
    /// `structure`.
    pub fn create_snapshot(
        &self,
        ontology_data: &Map<String, Value>,
        version_label: &str,
        author: &str,
        description: &str,
        options: &SnapshotOptions,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let change_entry = ChangeLogEntry::new(
            PyDateTime::now(false).isoformat('T'),
            author,
            description,
            None,
            None,
        )?;
        let or_default =
            |key: &str, default: Value| ontology_data.get(key).cloned().unwrap_or(default);

        let mut snapshot = Map::new();
        snapshot.insert("label".into(), Value::String(version_label.to_string()));
        snapshot.insert("timestamp".into(), Value::String(change_entry.timestamp));
        snapshot.insert("author".into(), Value::String(change_entry.author));
        snapshot.insert(
            "description".into(),
            Value::String(change_entry.description),
        );
        snapshot.insert(
            "ontology_iri".into(),
            or_default("uri", Value::String(String::new())),
        );
        snapshot.insert(
            "version_info".into(),
            or_default("version_info", Value::Object(Map::new())),
        );
        snapshot.insert(
            "structure".into(),
            or_default("structure", Value::Object(Map::new())),
        );
        snapshot.insert("metadata".into(), options.metadata_value());

        let checksum = compute_checksum(&snapshot);
        snapshot.insert("checksum".into(), Value::String(checksum));
        self.storage.save(&snapshot)?;
        self.versions_lock()
            .insert(version_label.to_string(), snapshot.clone());
        self.logger.info(format!(
            "Created ontology snapshot '{version_label}' by {author}"
        ));
        Ok(snapshot)
    }

    /// `compare_versions(version1, version2)`: metadata changes (`ontology_iri`,
    /// `version_info`) and structural differences between two stored versions
    /// (`ValidationError("Version not found: ...")` for an unknown label).
    pub fn compare_versions(
        &self,
        version1: &str,
        version2: &str,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let v1_snapshot = self.storage.get(version1)?;
        let v2_snapshot = self.storage.get(version2)?;
        let Some(v1_snapshot) = v1_snapshot.filter(|s| !s.is_empty()) else {
            return Err(ValidationError::new(format!("Version not found: {version1}")).into());
        };
        let Some(v2_snapshot) = v2_snapshot.filter(|s| !s.is_empty()) else {
            return Err(ValidationError::new(format!("Version not found: {version2}")).into());
        };

        let mut metadata_changes = Map::new();
        for key in ["ontology_iri", "version_info"] {
            let (from, to) = (get(&v1_snapshot, key), get(&v2_snapshot, key));
            if !py_eq(from, to) {
                let mut change = Map::new();
                change.insert("from".into(), from.clone());
                change.insert("to".into(), to.clone());
                metadata_changes.insert(key.into(), Value::Object(change));
            }
        }

        let empty = Value::Object(Map::new());
        let structural_diff = compare_ontology_structures(
            v1_snapshot.get("structure").unwrap_or(&empty),
            v2_snapshot.get("structure").unwrap_or(&empty),
        )?;

        let mut result = Map::new();
        result.insert("version1".into(), Value::String(version1.to_string()));
        result.insert("version2".into(), Value::String(version2.to_string()));
        result.insert("metadata_changes".into(), Value::Object(metadata_changes));
        result.extend(structural_diff);
        Ok(result)
    }
}

impl BaseVersionManager for OntologyVersionManager {
    fn storage(&self) -> &Arc<dyn VersionStorage> {
        &self.storage
    }

    /// Ontology data must be a dict (`AttributeError` otherwise, raised after the change
    /// metadata has been validated, as `ontology_data.get` fails).
    fn create_snapshot(
        &self,
        data: &Value,
        version_label: &str,
        author: &str,
        description: &str,
        options: &SnapshotOptions,
    ) -> Result<Map<String, Value>, SemanticaError> {
        match data {
            Value::Object(ontology_data) => OntologyVersionManager::create_snapshot(
                self,
                ontology_data,
                version_label,
                author,
                description,
                options,
            ),
            other => {
                ChangeLogEntry::new(
                    PyDateTime::now(false).isoformat('T'),
                    author,
                    description,
                    None,
                    None,
                )?;
                Err(attribute_error(other, "get"))
            }
        }
    }

    /// Versions are labels; a non-string scalar is looked up by its `str()`, and a list or dict
    /// is unhashable (`TypeError`).
    fn compare_versions(
        &self,
        version1: &Value,
        version2: &Value,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let label = |v: &Value| -> Result<String, SemanticaError> {
            match v {
                Value::String(s) => Ok(s.clone()),
                Value::Array(_) | Value::Object(_) => Err(SemanticaError::Type(format!(
                    "unhashable type: '{}'",
                    type_name(v)
                ))),
                other => Ok(py_str(other)),
            }
        };
        let (v1, v2) = (label(version1)?, label(version2)?);
        OntologyVersionManager::compare_versions(self, &v1, &v2)
    }
}
