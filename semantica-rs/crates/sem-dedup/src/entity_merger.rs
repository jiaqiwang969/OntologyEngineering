//! Entity Merger Module (`semantica/deduplication/entity_merger.py`).
//!
//! Detects duplicate groups and merges each with a [`MergeStrategyManager`], recording
//! provenance (`metadata.provenance.merged_from` / `merge_count`) and a merge history.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use sem_utils::SemanticaError;
use sem_utils::exceptions::ValidationError;
use sem_utils::get_logger;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::format_fixed;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::duplicate_detector::{DuplicateDetector, entity_value, similarity_scores_value};
use crate::entity::{AsEntity, EntityRef, entity_refs, py_id};
use crate::kwargs::{Kwargs, bool_param, merge_config, opt_float_param};
use crate::merge_strategy::{MergeResult, MergeStrategyManager, StrategyArg};
use crate::pyops::{HKey, contains, get, mapping, py_str, set_item, truthy};
use crate::similarity_calculator::enabled_tracker;

/// Entity merge operation representation (`MergeOperation` dataclass).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeOperation {
    /// The entities that were merged (the caller's references).
    #[serde(with = "crate::entity::serde_refs")]
    pub source_entities: Vec<EntityRef>,
    /// The merged entity.
    pub merged_entity: Map<String, Value>,
    /// The strategy manager's result.
    pub merge_result: MergeResult,
    /// Timestamp (never set by the merger, as in the original).
    #[serde(default)]
    pub timestamp: Option<String>,
    /// `group_confidence`, `similarity_scores`, `strategy` (duplicate merges) or `strategy`,
    /// `conflicts` (group merges).
    #[serde(default)]
    pub metadata: Option<Map<String, Value>>,
}

/// Entity merging engine for knowledge graphs (`EntityMerger`).
pub struct EntityMerger {
    logger: Logger,
    /// `config` merged with the extra keyword arguments.
    pub config: Map<String, Value>,
    /// Built from `config["detector"]`.
    pub duplicate_detector: DuplicateDetector,
    /// Built from `config["strategy"]`.
    pub merge_strategy_manager: MergeStrategyManager,
    /// Every merge operation performed, in order.
    pub merge_history: Vec<MergeOperation>,
    /// Whether merged entities record their sources.
    pub preserve_provenance: bool,
    progress_tracker: Arc<ProgressTracker>,
}

impl fmt::Debug for EntityMerger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntityMerger")
            .field("config", &self.config)
            .field("duplicate_detector", &self.duplicate_detector)
            .field("merge_strategy_manager", &self.merge_strategy_manager)
            .field("merge_history", &self.merge_history.len())
            .field("preserve_provenance", &self.preserve_provenance)
            .finish()
    }
}

impl Default for EntityMerger {
    fn default() -> Self {
        EntityMerger::new(&Map::new()).expect("default arguments are valid")
    }
}

const DUPLICATE_DETECTOR: &str = "semantica.deduplication.duplicate_detector.DuplicateDetector";
const MERGE_STRATEGY_MANAGER: &str = "semantica.deduplication.merge_strategy.MergeStrategyManager";

/// Bind a `threshold` keyword argument (`None`/absent: the detector's default).
fn take_threshold(kw: &mut Kwargs) -> Result<Option<f64>, SemanticaError> {
    opt_float_param("threshold", kw.take("threshold"))
}

/// `entity.get("id") or id(entity)` as a set member.
fn processed_key(entity: &EntityRef) -> Result<(HKey, String), SemanticaError> {
    match get(entity, "id")? {
        Some(id) if truthy(id) => Ok((HKey::scalar(id)?, py_str(id))),
        _ => {
            let address = py_id(entity);
            Ok((HKey::Int(i128::from(address)), address.to_string()))
        }
    }
}

impl EntityMerger {
    /// `EntityMerger(preserve_provenance=True, config=None, **kwargs)`, with the keyword
    /// arguments as a map (`detector` and `strategy` in the config configure the
    /// [`DuplicateDetector`] and the [`MergeStrategyManager`]).
    pub fn new(kwargs: &Map<String, Value>) -> Result<EntityMerger, SemanticaError> {
        let logger = get_logger("entity_merger");
        let mut kw = Kwargs::new(kwargs);
        let preserve_provenance = bool_param(kw.take("preserve_provenance"), true);
        let config = kw.take("config");
        let config = merge_config(config, kw.rest())?;

        let detector_config = config.get("detector").cloned().unwrap_or(json!({}));
        let strategy_config = config.get("strategy").cloned().unwrap_or(json!({}));
        let duplicate_detector = DuplicateDetector::new(&mapping(DUPLICATE_DETECTOR, &detector_config)?)?;
        let merge_strategy_manager =
            MergeStrategyManager::new(&mapping(MERGE_STRATEGY_MANAGER, &strategy_config)?)?;

        let merger = EntityMerger {
            logger,
            config,
            duplicate_detector,
            merge_strategy_manager,
            merge_history: Vec::new(),
            preserve_provenance,
            progress_tracker: enabled_tracker(),
        };
        merger.logger.debug(format!(
            "Entity merger initialized (preserve_provenance: {})",
            if preserve_provenance { "True" } else { "False" }
        ));
        Ok(merger)
    }

    /// The logger (`semantica.entity_merger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `merge_duplicates(entities, strategy=None, **options)`: detect duplicate groups
    /// (option `threshold`), merge every group of two or more entities, add provenance when
    /// enabled and record each operation in the history.
    pub fn merge_duplicates<E: AsEntity>(
        &mut self,
        entities: &[E],
        strategy: Option<StrategyArg>,
        options: &Map<String, Value>,
    ) -> Result<Vec<MergeOperation>, SemanticaError> {
        let entities = entity_refs(entities);
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("deduplication"),
            Some("EntityMerger"),
            &format!("Merging duplicates from {} entities", entities.len()),
            None,
        );
        let result = self.merge_duplicates_body(&entities, strategy, options, &tracking_id);
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    fn merge_duplicates_body(
        &mut self,
        entities: &[EntityRef],
        strategy: Option<StrategyArg>,
        options: &Map<String, Value>,
        tracking_id: &str,
    ) -> Result<Vec<MergeOperation>, SemanticaError> {
        let tracker = self.progress_tracker.clone();
        self.logger
            .info(format!("Merging duplicates from {} entities", entities.len()));
        tracker.update_tracking(
            tracking_id,
            Some("running"),
            &format!("Starting merge process for {} entities...", entities.len()),
        );
        tracker.update_tracking(tracking_id, None, "Detecting duplicate groups...");
        let mut kw = Kwargs::new(options);
        let threshold = take_threshold(&mut kw)?;
        let duplicate_groups =
            self.duplicate_detector
                .detect_duplicate_groups(entities, threshold, &kw.rest())?;
        self.logger.debug(format!(
            "Found {} duplicate group(s)",
            duplicate_groups.len()
        ));

        tracker.update_tracking(
            tracking_id,
            None,
            &format!("Merging {} groups...", duplicate_groups.len()),
        );
        let mut merge_operations = Vec::new();
        let mergeable: Vec<_> = duplicate_groups
            .into_iter()
            .filter(|g| g.entities.len() >= 2)
            .collect();
        let total = mergeable.len();
        let update_interval = if total <= 10 {
            1
        } else {
            (total / 100).clamp(1, 5)
        };
        tracker.update_progress(
            tracking_id,
            0,
            total as i64,
            &format!("Starting merge operations... 0/{total} (remaining: {total})"),
        );

        for (i, group) in mergeable.iter().enumerate() {
            self.logger.debug(format!(
                "Merging group of {} entities (confidence: {})",
                group.entities.len(),
                format_fixed(group.confidence, 2)
            ));
            let mut merge_result = self.merge_strategy_manager.merge_entities(
                &group.entities,
                strategy.clone(),
                options,
            )?;
            if self.preserve_provenance {
                self.add_provenance(&mut merge_result.merged_entity, &group.entities)?;
            }
            let mut metadata = Map::new();
            metadata.insert("group_confidence".to_string(), crate::pyops::float_value(group.confidence));
            metadata.insert(
                "similarity_scores".to_string(),
                similarity_scores_value(&group.similarity_scores),
            );
            metadata.insert(
                "strategy".to_string(),
                merge_result
                    .metadata
                    .get("strategy")
                    .cloned()
                    .unwrap_or(json!("default")),
            );
            let operation = MergeOperation {
                source_entities: group.entities.clone(),
                merged_entity: merge_result.merged_entity.clone(),
                merge_result,
                timestamp: None,
                metadata: Some(metadata),
            };
            merge_operations.push(operation.clone());
            self.merge_history.push(operation);

            let done = i + 1;
            if done % update_interval == 0 || done == total || i == 0 || total <= 10 {
                tracker.update_progress(
                    tracking_id,
                    done as i64,
                    total as i64,
                    &format!("Merging groups... {done}/{total} (remaining: {})", total - done),
                );
            }
        }

        self.logger.info(format!(
            "Completed merging: {} merge operation(s) performed",
            merge_operations.len()
        ));
        tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Completed {} merge operations", merge_operations.len()),
            None,
        );
        Ok(merge_operations)
    }

    /// `merge_entity_group(entities, strategy=None, **options)`: merge a known group (at
    /// least two entities, else `ValidationError`) without duplicate detection.
    pub fn merge_entity_group<E: AsEntity>(
        &mut self,
        entities: &[E],
        strategy: Option<StrategyArg>,
        options: &Map<String, Value>,
    ) -> Result<MergeOperation, SemanticaError> {
        let entities = entity_refs(entities);
        if entities.len() < 2 {
            return Err(ValidationError::new(format!(
                "Need at least 2 entities to merge, got {}",
                entities.len()
            ))
            .into());
        }
        let strategy_val = match &strategy {
            Some(StrategyArg::Strategy(s)) => s.value().to_string(),
            Some(StrategyArg::Name(name)) if !name.is_empty() => name.clone(),
            _ => "default".to_string(),
        };
        self.logger.debug(format!(
            "Merging group of {} entities (strategy: {strategy_val})",
            entities.len()
        ));

        let mut merge_result =
            self.merge_strategy_manager
                .merge_entities(&entities, strategy, options)?;
        if self.preserve_provenance {
            self.add_provenance(&mut merge_result.merged_entity, &entities)?;
        }
        let mut metadata = Map::new();
        metadata.insert(
            "strategy".to_string(),
            merge_result
                .metadata
                .get("strategy")
                .cloned()
                .unwrap_or(json!("default")),
        );
        metadata.insert(
            "conflicts".to_string(),
            json!(merge_result.conflicts.len()),
        );
        let operation = MergeOperation {
            source_entities: entities.clone(),
            merged_entity: merge_result.merged_entity.clone(),
            merge_result,
            timestamp: None,
            metadata: Some(metadata),
        };
        self.merge_history.push(operation.clone());
        self.logger.info(format!(
            "Successfully merged {} entities ({} conflict(s))",
            entities.len(),
            operation.merge_result.conflicts.len()
        ));
        Ok(operation)
    }

    /// `incremental_merge(new_entities, existing_entities, **options)`: merge each new
    /// entity with its best existing duplicate (incremental detection, option `threshold`;
    /// option `strategy`), each entity at most once.
    pub fn incremental_merge<N: AsEntity, X: AsEntity>(
        &mut self,
        new_entities: &[N],
        existing_entities: &[X],
        options: &Map<String, Value>,
    ) -> Result<Vec<MergeOperation>, SemanticaError> {
        self.logger.info(format!(
            "Incremental merge: {} new entities vs {} existing entities",
            new_entities.len(),
            existing_entities.len()
        ));
        let mut kw = Kwargs::new(options);
        let threshold = take_threshold(&mut kw)?;
        let candidates = self.duplicate_detector.incremental_detect(
            new_entities,
            existing_entities,
            threshold,
            &kw.rest(),
        )?;
        self.logger
            .debug(format!("Found {} duplicate candidate(s)", candidates.len()));

        // `merge_entity_group([...], **options)`: a `strategy` option binds the strategy.
        let mut group_kw = Kwargs::new(options);
        let strategy = group_kw
            .take("strategy")
            .as_ref()
            .and_then(StrategyArg::from_json);
        let group_options = group_kw.rest();

        let mut merge_operations = Vec::new();
        let mut processed_new: HashSet<HKey> = HashSet::new();
        let mut processed_existing: HashSet<HKey> = HashSet::new();
        for candidate in candidates {
            let (new_key, new_text) = processed_key(&candidate.entity1)?;
            let (existing_key, existing_text) = processed_key(&candidate.entity2)?;
            if processed_new.contains(&new_key) || processed_existing.contains(&existing_key) {
                self.logger.debug(format!(
                    "Skipping merge: entity already processed (new: {new_text}, existing: {existing_text})"
                ));
                continue;
            }
            let operation = self.merge_entity_group(
                &[candidate.entity1.clone(), candidate.entity2.clone()],
                strategy.clone(),
                &group_options,
            )?;
            merge_operations.push(operation);
            processed_new.insert(new_key);
            processed_existing.insert(existing_key);
        }
        self.logger.info(format!(
            "Incremental merge completed: {} merge operation(s)",
            merge_operations.len()
        ));
        Ok(merge_operations)
    }

    /// `_add_provenance(merged_entity, source_entities)`: record `merged_from` (id, name and
    /// `metadata.source` of each source) and `merge_count` under `metadata.provenance`.
    fn add_provenance(
        &self,
        merged_entity: &mut Map<String, Value>,
        source_entities: &[EntityRef],
    ) -> Result<(), SemanticaError> {
        let metadata = merged_entity
            .entry("metadata")
            .or_insert_with(|| json!({}));
        if !contains(metadata, &json!("provenance"))? {
            set_item(metadata, "provenance", json!({}))?;
        }
        let provenance = subscript_mut(metadata, "provenance")?;
        let mut merged_from = Vec::with_capacity(source_entities.len());
        for e in source_entities.iter() {
            let source = match e.as_ref() {
                Value::Object(_) => {
                    let entity_metadata = entity_value(e, "metadata", json!({}));
                    get(&entity_metadata, "source")?
                        .cloned()
                        .unwrap_or(Value::Null)
                }
                _ => Value::Null,
            };
            let mut record = Map::new();
            record.insert("id".to_string(), entity_value(e, "id", Value::Null));
            record.insert("name".to_string(), entity_value(e, "name", Value::Null));
            record.insert("source".to_string(), source);
            merged_from.push(Value::Object(record));
        }
        set_item(provenance, "merged_from", Value::Array(merged_from))?;
        set_item(provenance, "merge_count", json!(source_entities.len()))?;
        self.logger.debug(format!(
            "Added provenance for merge of {} entity(ies)",
            source_entities.len()
        ));
        Ok(())
    }

    /// `get_merge_history()`: a copy of the merge history.
    pub fn get_merge_history(&self) -> Vec<MergeOperation> {
        self.merge_history.clone()
    }

    /// `validate_merge_quality(merge_operation)`: the strategy manager's validation of the
    /// operation's merge result.
    pub fn validate_merge_quality(&self, merge_operation: &MergeOperation) -> Map<String, Value> {
        self.merge_strategy_manager
            .validate_merge(&merge_operation.merge_result)
    }
}
