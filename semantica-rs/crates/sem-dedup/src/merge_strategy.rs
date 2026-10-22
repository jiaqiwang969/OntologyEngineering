//! Merge Strategy Manager Module (`semantica/deduplication/merge_strategy.py`).
//!
//! Strategies for merging duplicate entities (`keep_first`, `keep_last`,
//! `keep_most_complete`, `keep_highest_confidence`, `merge_all`, `custom`), property-specific
//! rules with custom conflict resolution, relationship union (with optional semantic
//! canonicalisation), metadata merging and merge validation.

use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use indexmap::IndexMap;
use sem_utils::exceptions::ValidationError;
use sem_utils::get_logger;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::SemanticaError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::entity::AsEntity;
use crate::kwargs::{Kwargs, merge_config};
use crate::pyops::{
    Cmp, HKey, compare, contains, copy, eq, get, get_or, items, lower, normalize_spaces_lower,
    py_len, py_str, set_item, str_for, truthy, type_error,
};
use crate::similarity_calculator::enabled_tracker;

/// Merge strategy types (`MergeStrategy` enum).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// `keep_first`.
    KeepFirst,
    /// `keep_last`.
    KeepLast,
    /// `keep_most_complete`.
    KeepMostComplete,
    /// `keep_highest_confidence`.
    KeepHighestConfidence,
    /// `merge_all`.
    MergeAll,
    /// `custom`.
    Custom,
}

impl MergeStrategy {
    /// Every member, in definition order.
    pub const ALL: [MergeStrategy; 6] = [
        MergeStrategy::KeepFirst,
        MergeStrategy::KeepLast,
        MergeStrategy::KeepMostComplete,
        MergeStrategy::KeepHighestConfidence,
        MergeStrategy::MergeAll,
        MergeStrategy::Custom,
    ];

    /// The member's value (`"keep_first"`, ...).
    pub fn value(self) -> &'static str {
        match self {
            MergeStrategy::KeepFirst => "keep_first",
            MergeStrategy::KeepLast => "keep_last",
            MergeStrategy::KeepMostComplete => "keep_most_complete",
            MergeStrategy::KeepHighestConfidence => "keep_highest_confidence",
            MergeStrategy::MergeAll => "merge_all",
            MergeStrategy::Custom => "custom",
        }
    }

    /// The member's name (`"KEEP_FIRST"`, ...).
    pub fn name(self) -> &'static str {
        match self {
            MergeStrategy::KeepFirst => "KEEP_FIRST",
            MergeStrategy::KeepLast => "KEEP_LAST",
            MergeStrategy::KeepMostComplete => "KEEP_MOST_COMPLETE",
            MergeStrategy::KeepHighestConfidence => "KEEP_HIGHEST_CONFIDENCE",
            MergeStrategy::MergeAll => "MERGE_ALL",
            MergeStrategy::Custom => "CUSTOM",
        }
    }

    /// `MergeStrategy(value)`: lookup by value (case-sensitive); `ValueError` otherwise.
    pub fn from_value(value: &str) -> Result<MergeStrategy, SemanticaError> {
        MergeStrategy::ALL
            .into_iter()
            .find(|s| s.value() == value)
            .ok_or_else(|| {
                SemanticaError::Value(format!(
                    "{} is not a valid MergeStrategy",
                    sem_utils::pycompat::fmt::str_repr(value)
                ))
            })
    }

    /// `MergeStrategy(value)` for any JSON value (only strings can match).
    pub fn from_json(value: &Value) -> Result<MergeStrategy, SemanticaError> {
        match value {
            Value::String(s) => MergeStrategy::from_value(s),
            other => Err(SemanticaError::Value(format!(
                "{} is not a valid MergeStrategy",
                sem_utils::pycompat::fmt::py_repr(other)
            ))),
        }
    }
}

impl FromStr for MergeStrategy {
    type Err = SemanticaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MergeStrategy::from_value(s)
    }
}

impl fmt::Display for MergeStrategy {
    /// `str(MergeStrategy.KEEP_FIRST)` = `MergeStrategy.KEEP_FIRST`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MergeStrategy.{}", self.name())
    }
}

impl fmt::Debug for MergeStrategy {
    /// `repr(MergeStrategy.KEEP_FIRST)` = `<MergeStrategy.KEEP_FIRST: 'keep_first'>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<MergeStrategy.{}: '{}'>", self.name(), self.value())
    }
}

/// A strategy argument: a [`MergeStrategy`] member or its (case-insensitive) value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyArg {
    /// A member.
    Strategy(MergeStrategy),
    /// A name such as `"keep_first"` (lowercased before lookup).
    Name(String),
}

impl StrategyArg {
    /// A strategy passed in a keyword map: strings are names; other values are not
    /// strategies (the original then uses the default strategy).
    pub fn from_json(value: &Value) -> Option<StrategyArg> {
        value.as_str().map(|s| StrategyArg::Name(s.to_string()))
    }
}

impl From<MergeStrategy> for StrategyArg {
    fn from(value: MergeStrategy) -> Self {
        StrategyArg::Strategy(value)
    }
}

impl From<&str> for StrategyArg {
    fn from(value: &str) -> Self {
        StrategyArg::Name(value.to_string())
    }
}

impl From<String> for StrategyArg {
    fn from(value: String) -> Self {
        StrategyArg::Name(value)
    }
}

/// A custom conflict-resolution function: `(current_value, new_value) -> resolved_value`.
/// An error is logged and the rule's strategy applies instead (as when the Python function
/// raises).
pub type ConflictResolver =
    Arc<dyn Fn(&Value, &Value) -> Result<Value, SemanticaError> + Send + Sync>;

/// Rule for merging one property (`PropertyMergeRule` dataclass).
#[derive(Clone)]
pub struct PropertyMergeRule {
    /// The property (or top-level field) name.
    pub property_name: String,
    /// The strategy for conflicts on this property.
    pub strategy: MergeStrategy,
    /// Custom conflict resolution, tried first.
    pub conflict_resolution: Option<ConflictResolver>,
    /// Rule priority (stored, as in the original).
    pub priority: i64,
}

impl PropertyMergeRule {
    /// `PropertyMergeRule(property_name, strategy, conflict_resolution=None, priority=0)`.
    pub fn new(
        property_name: impl Into<String>,
        strategy: MergeStrategy,
        conflict_resolution: Option<ConflictResolver>,
        priority: i64,
    ) -> PropertyMergeRule {
        PropertyMergeRule {
            property_name: property_name.into(),
            strategy,
            conflict_resolution,
            priority,
        }
    }
}

impl fmt::Debug for PropertyMergeRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PropertyMergeRule")
            .field("property_name", &self.property_name)
            .field("strategy", &self.strategy)
            .field(
                "conflict_resolution",
                &self.conflict_resolution.as_ref().map(|_| "<function>"),
            )
            .field("priority", &self.priority)
            .finish()
    }
}

impl PartialEq for PropertyMergeRule {
    /// Dataclass equality; functions compare by identity.
    fn eq(&self, other: &Self) -> bool {
        self.property_name == other.property_name
            && self.strategy == other.strategy
            && self.priority == other.priority
            && match (&self.conflict_resolution, &other.conflict_resolution) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
}

/// Result of a merge operation (`MergeResult` dataclass).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MergeResult {
    /// The merged entity.
    pub merged_entity: Map<String, Value>,
    /// The (copied) entities that were merged.
    pub merged_entities: Vec<Map<String, Value>>,
    /// Unresolved conflicts (`property`, `values`, `resolution`).
    #[serde(default)]
    pub conflicts: Vec<Map<String, Value>>,
    /// `strategy`.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

/// The outcome of `_resolve_property_conflict`.
struct Resolution {
    resolved: bool,
    value: Value,
    resolution: &'static str,
}

/// Merge strategy management engine (`MergeStrategyManager`).
pub struct MergeStrategyManager {
    logger: Logger,
    /// `config` merged with the extra keyword arguments (`relationship_dedup_mode`,
    /// `predicate_synonym_map`, `literal_normalization_enabled`).
    pub config: Map<String, Value>,
    /// The strategy used when none is given.
    pub default_strategy: MergeStrategy,
    /// Property-specific rules.
    pub property_rules: IndexMap<String, PropertyMergeRule>,
    progress_tracker: Arc<ProgressTracker>,
}

impl fmt::Debug for MergeStrategyManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MergeStrategyManager")
            .field("config", &self.config)
            .field("default_strategy", &self.default_strategy)
            .field("property_rules", &self.property_rules)
            .finish()
    }
}

impl Clone for MergeStrategyManager {
    fn clone(&self) -> Self {
        MergeStrategyManager {
            logger: self.logger.clone(),
            config: self.config.clone(),
            default_strategy: self.default_strategy,
            property_rules: self.property_rules.clone(),
            progress_tracker: self.progress_tracker.clone(),
        }
    }
}

impl Default for MergeStrategyManager {
    fn default() -> Self {
        MergeStrategyManager::new(&Map::new()).expect("default arguments are valid")
    }
}

impl MergeStrategyManager {
    /// `MergeStrategyManager(default_strategy="keep_most_complete", config=None, **kwargs)`,
    /// with the keyword arguments as a map. An invalid default strategy logs a warning and
    /// falls back to `keep_most_complete`.
    pub fn new(kwargs: &Map<String, Value>) -> Result<MergeStrategyManager, SemanticaError> {
        let logger = get_logger("merge_strategy_manager");
        let mut kw = Kwargs::new(kwargs);
        let default_strategy = kw
            .take("default_strategy")
            .unwrap_or(json!("keep_most_complete"));
        let config = kw.take("config");
        let config = merge_config(config, kw.rest())?;

        let strategy_name = config
            .get("default_strategy")
            .cloned()
            .unwrap_or(default_strategy);
        let default_strategy = match MergeStrategy::from_json(&strategy_name) {
            Ok(strategy) => strategy,
            Err(_) => {
                logger.warning(format!(
                    "Invalid default strategy '{}', using 'keep_most_complete'",
                    py_str(&strategy_name)
                ));
                MergeStrategy::KeepMostComplete
            }
        };
        let manager = MergeStrategyManager {
            logger,
            config,
            default_strategy,
            property_rules: IndexMap::new(),
            progress_tracker: enabled_tracker(),
        };
        manager.logger.debug(format!(
            "Merge strategy manager initialized (default: {})",
            manager.default_strategy.value()
        ));
        Ok(manager)
    }

    /// The logger (`semantica.merge_strategy_manager`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `add_property_rule(property_name, strategy, conflict_resolution=None, priority=0)`:
    /// a strategy name is lowercased; an unknown name logs a warning and becomes
    /// `keep_most_complete`.
    pub fn add_property_rule(
        &mut self,
        property_name: &str,
        strategy: impl Into<StrategyArg>,
        conflict_resolution: Option<ConflictResolver>,
        priority: Option<i64>,
    ) {
        let strategy = match strategy.into() {
            StrategyArg::Strategy(s) => s,
            StrategyArg::Name(name) => match MergeStrategy::from_value(&lower(&name)) {
                Ok(s) => s,
                Err(_) => {
                    self.logger.warning(format!(
                        "Invalid property strategy '{name}', defaulting to keep_most_complete"
                    ));
                    MergeStrategy::KeepMostComplete
                }
            },
        };
        let rule = PropertyMergeRule::new(
            property_name,
            strategy,
            conflict_resolution,
            priority.unwrap_or(0),
        );
        self.property_rules.insert(property_name.to_string(), rule);
    }

    /// `merge_entities(entities, strategy=None, **options)`: merge the entities into one
    /// (`id` from the base entity chosen by the strategy, `name`/`type` from the base or the
    /// field's rule, merged properties, the union of relationships, merged metadata,
    /// `merged_from` and `merge_strategy`). An empty list raises `ValidationError`; a single
    /// entity is returned unchanged. `options` are accepted and ignored.
    pub fn merge_entities<E: AsEntity>(
        &self,
        entities: &[E],
        strategy: Option<StrategyArg>,
        options: &Map<String, Value>,
    ) -> Result<MergeResult, SemanticaError> {
        let _ = options;
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("deduplication"),
            Some("MergeStrategyManager"),
            &format!("Merging {} entities", entities.len()),
            None,
        );
        let values: Vec<&Value> = entities.iter().map(AsEntity::entity).collect();
        let result = self.merge_body(&values, strategy, &tracking_id);
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    fn merge_body(
        &self,
        entities: &[&Value],
        strategy: Option<StrategyArg>,
        tracking_id: &str,
    ) -> Result<MergeResult, SemanticaError> {
        let tracker = &self.progress_tracker;
        if entities.is_empty() {
            return Err(ValidationError::new("No entities to merge").into());
        }
        // Pre-process: dicts are copied; anything else is wrapped as `{"_original": entity}`.
        let processed: Vec<Map<String, Value>> = entities
            .iter()
            .map(|entity| match entity {
                Value::Object(map) => map.clone(),
                other => {
                    let mut map = Map::new();
                    map.insert("_original".to_string(), (*other).clone());
                    map
                }
            })
            .collect();
        if processed.len() == 1 {
            tracker.stop_tracking(
                tracking_id,
                Some("completed"),
                "Single entity, no merge needed",
                None,
            );
            return Ok(MergeResult {
                merged_entity: processed[0].clone(),
                merged_entities: processed,
                conflicts: Vec::new(),
                metadata: Map::new(),
            });
        }
        let values: Vec<Value> = processed.iter().cloned().map(Value::Object).collect();

        let strategy = match strategy {
            None => self.default_strategy,
            Some(StrategyArg::Strategy(s)) => s,
            Some(StrategyArg::Name(name)) => match MergeStrategy::from_value(&lower(&name)) {
                Ok(s) => s,
                Err(_) => {
                    self.logger.warning(format!(
                        "Invalid strategy '{name}', using default '{}'",
                        self.default_strategy.value()
                    ));
                    self.default_strategy
                }
            },
        };

        let total_steps = 4;
        tracker.update_progress(
            tracking_id,
            1,
            total_steps,
            &format!(
                "Selecting base entity using {}... (1/{total_steps}, remaining: 3 steps)",
                strategy.value()
            ),
        );
        let base = self.select_base_entity(&values, strategy)?;

        let mut total_properties = 0usize;
        for e in &values {
            total_properties += py_len(&get_or(e, "properties", json!({}))?)?;
        }
        tracker.update_progress(
            tracking_id,
            2,
            total_steps,
            &format!(
                "Merging properties... (2/{total_steps}, {total_properties} properties, remaining: 2 steps)"
            ),
        );
        let (merged_properties, property_conflicts) =
            self.merge_properties(&values, base, strategy, Some(tracking_id))?;

        let mut total_relationships = 0usize;
        for e in &values {
            total_relationships += py_len(&get_or(e, "relationships", json!([]))?)?;
        }
        tracker.update_progress(
            tracking_id,
            3,
            total_steps,
            &format!(
                "Merging relationships... (3/{total_steps}, {total_relationships} relationships, remaining: 1 steps)"
            ),
        );
        let merged_relationships = self.merge_relationships(&values)?;

        tracker.update_progress(
            tracking_id,
            4,
            total_steps,
            &format!("Building merged entity... (4/{total_steps}, remaining: 0 steps)"),
        );
        let base_entity = &values[base];
        let mut merged_entity = Map::new();
        merged_entity.insert(
            "id".to_string(),
            get(base_entity, "id")?.cloned().unwrap_or(Value::Null),
        );
        merged_entity.insert(
            "name".to_string(),
            self.merge_top_level_field("name", &values, base)?,
        );
        merged_entity.insert(
            "type".to_string(),
            self.merge_top_level_field("type", &values, base)?,
        );
        merged_entity.insert("properties".to_string(), merged_properties);
        merged_entity.insert(
            "relationships".to_string(),
            Value::Array(merged_relationships),
        );
        merged_entity.insert("metadata".to_string(), self.merge_metadata(&values, base)?);
        let mut merged_from = Vec::new();
        for e in &values {
            if let Some(id) = get(e, "id")?.filter(|v| truthy(v)) {
                merged_from.push(id.clone());
            }
        }
        merged_entity.insert("merged_from".to_string(), Value::Array(merged_from));
        merged_entity.insert("merge_strategy".to_string(), json!(strategy.value()));

        tracker.stop_tracking(tracking_id, Some("completed"), "Entity merge completed", None);
        let mut metadata = Map::new();
        metadata.insert("strategy".to_string(), json!(strategy.value()));
        Ok(MergeResult {
            merged_entity,
            merged_entities: processed,
            conflicts: property_conflicts,
            metadata,
        })
    }

    /// `_merge_top_level_field(field_name, entities, base_entity)`: the base entity's value,
    /// or with a rule for the field, the values of the entities that have it merged in list
    /// order.
    fn merge_top_level_field(
        &self,
        field_name: &str,
        entities: &[Value],
        base: usize,
    ) -> Result<Value, SemanticaError> {
        let Some(rule) = self.property_rules.get(field_name) else {
            return Ok(get(&entities[base], field_name)?.cloned().unwrap_or(Value::Null));
        };
        let mut valid: Vec<&Value> = Vec::new();
        for e in entities {
            if let Some(v) = get(e, field_name)?.filter(|v| !v.is_null()) {
                valid.push(v);
            }
        }
        let Some(first) = valid.first() else {
            return Ok(Value::Null);
        };
        let mut merged_value = (*first).clone();
        for val in &valid[1..] {
            if !eq(&merged_value, val) {
                let conflict =
                    self.resolve_property_conflict(field_name, merged_value.clone(), val, rule.strategy)?;
                if conflict.resolved {
                    merged_value = conflict.value;
                }
            }
        }
        Ok(merged_value)
    }

    /// `_select_base_entity(entities, strategy)`: the index of the base entity.
    fn select_base_entity(
        &self,
        entities: &[Value],
        strategy: MergeStrategy,
    ) -> Result<usize, SemanticaError> {
        match strategy {
            MergeStrategy::KeepFirst => Ok(0),
            MergeStrategy::KeepLast => Ok(entities.len() - 1),
            MergeStrategy::KeepMostComplete => {
                // `max(entities, key=len(properties) + len(relationships))`: first maximum.
                let mut best: Option<(usize, usize)> = None;
                for (index, e) in entities.iter().enumerate() {
                    let size = py_len(&get_or(e, "properties", json!({}))?)?
                        + py_len(&get_or(e, "relationships", json!([]))?)?;
                    match best {
                        Some((best_size, _)) if size <= best_size => {}
                        _ => best = Some((size, index)),
                    }
                }
                Ok(best.map(|(_, i)| i).unwrap_or(0))
            }
            MergeStrategy::KeepHighestConfidence => {
                // `max(entities, key=confidence)`: Python's `>` decides (first maximum).
                let mut best: Option<(Value, usize)> = None;
                for (index, e) in entities.iter().enumerate() {
                    let confidence = get_or(e, "confidence", json!(0.0))?;
                    match &best {
                        Some((best_value, _)) if !compare(&confidence, best_value, Cmp::Gt)? => {}
                        _ => best = Some((confidence, index)),
                    }
                }
                Ok(best.map(|(_, i)| i).unwrap_or(0))
            }
            _ => Ok(0),
        }
    }

    /// `_merge_properties(entities, base_entity, strategy)`: the base entity's properties,
    /// plus new properties of the other entities, with conflicts resolved.
    fn merge_properties(
        &self,
        entities: &[Value],
        base: usize,
        strategy: MergeStrategy,
        tracking_id: Option<&str>,
    ) -> Result<(Value, Vec<Map<String, Value>>), SemanticaError> {
        let base_entity = &entities[base];
        let mut merged_properties = copy(&get_or(base_entity, "properties", json!({}))?)?;
        let mut conflicts = Vec::new();
        if entities.len() <= 1 {
            return Ok((merged_properties, conflicts));
        }
        for (ent_idx, entity) in entities.iter().enumerate() {
            if eq(entity, base_entity) {
                continue;
            }
            let prop_list = items(&get_or(entity, "properties", json!({}))?)?;
            let total_props = prop_list.len();
            for (prop_idx, (prop_name, prop_value)) in prop_list.into_iter().enumerate() {
                if !contains(&merged_properties, &Value::String(prop_name.clone()))? {
                    set_item(&mut merged_properties, &prop_name, prop_value)?;
                } else {
                    let current = subscript(&merged_properties, &prop_name)?;
                    if !eq(&current, &prop_value) {
                        let conflict =
                            self.resolve_property_conflict(&prop_name, current.clone(), &prop_value, strategy)?;
                        if conflict.resolved {
                            set_item(&mut merged_properties, &prop_name, conflict.value)?;
                        } else {
                            let mut record = Map::new();
                            record.insert("property".to_string(), json!(prop_name));
                            record.insert("values".to_string(), json!([current, prop_value]));
                            record.insert("resolution".to_string(), json!(conflict.resolution));
                            conflicts.push(record);
                        }
                    }
                }
                if let Some(id) = tracking_id
                    && (prop_idx % 10 == 0 || prop_idx + 1 == total_props)
                {
                    self.progress_tracker.update_tracking(
                        id,
                        None,
                        &format!(
                            "Merging properties from entity {}/{}: {prop_name}",
                            ent_idx + 1,
                            entities.len()
                        ),
                    );
                }
            }
        }
        Ok((merged_properties, conflicts))
    }

    /// `_resolve_property_conflict(property_name, value1, value2, default_strategy)`.
    fn resolve_property_conflict(
        &self,
        property_name: &str,
        value1: Value,
        value2: &Value,
        default_strategy: MergeStrategy,
    ) -> Result<Resolution, SemanticaError> {
        let rule = self.property_rules.get(property_name);
        if let Some(resolver) = rule.and_then(|r| r.conflict_resolution.as_ref()) {
            match resolver(&value1, value2) {
                Ok(value) => {
                    return Ok(Resolution {
                        resolved: true,
                        value,
                        resolution: "custom_rule",
                    });
                }
                Err(e) => self
                    .logger
                    .warning(format!("Custom conflict resolution failed: {e}")),
            }
        }
        let strategy = rule.map(|r| r.strategy).unwrap_or(default_strategy);
        Ok(match strategy {
            MergeStrategy::KeepFirst => Resolution {
                resolved: true,
                value: value1,
                resolution: "keep_first",
            },
            MergeStrategy::KeepLast => Resolution {
                resolved: true,
                value: value2.clone(),
                resolution: "keep_last",
            },
            MergeStrategy::MergeAll => match value1 {
                Value::Array(mut list) => {
                    if !list.iter().any(|x| eq(x, value2)) {
                        list.push(value2.clone());
                    }
                    Resolution {
                        resolved: true,
                        value: Value::Array(list),
                        resolution: "merge_all",
                    }
                }
                other => Resolution {
                    resolved: true,
                    value: Value::Array(vec![other, value2.clone()]),
                    resolution: "merge_all",
                },
            },
            _ => Resolution {
                resolved: true,
                value: value1,
                resolution: "default",
            },
        })
    }

    /// `_merge_relationships(entities, base_entity)`: every relationship once, keyed by
    /// `(subject, predicate, object)` (with `relationship_dedup_mode="semantic_v2"`: the
    /// lowercased synonym-mapped predicate and, with `literal_normalization_enabled`, the
    /// whitespace-normalised lowercased object).
    fn merge_relationships(&self, entities: &[Value]) -> Result<Vec<Value>, SemanticaError> {
        let mut all_relationships = Vec::new();
        let mut seen: HashSet<HKey> = HashSet::new();
        let legacy = json!("legacy");
        let semantic_v2 = self
            .config
            .get("relationship_dedup_mode")
            .unwrap_or(&legacy)
            .as_str()
            == Some("semantic_v2");
        let empty = json!({});
        let synonym_map = self.config.get("predicate_synonym_map").unwrap_or(&empty);
        let norm_enabled = truthy(
            self.config
                .get("literal_normalization_enabled")
                .unwrap_or(&Value::Bool(false)),
        );

        for entity in entities {
            let relationships = get_or(entity, "relationships", json!([]))?;
            for rel in crate::pyops::iterate(&relationships)? {
                let subj = get(&rel, "subject")?.cloned().unwrap_or(Value::Null);
                let pred = py_str(&get_or(&rel, "predicate", json!(""))?);
                let obj = get(&rel, "object")?.cloned().unwrap_or(Value::Null);
                let rel_key = if semantic_v2 {
                    let mapped = get(synonym_map, &lower(&pred))?
                        .cloned()
                        .unwrap_or_else(|| Value::String(pred.clone()));
                    let canon_pred = lower(str_for(&mapped, "lower")?);
                    let canon_obj = match &obj {
                        Value::String(s) if norm_enabled => {
                            Value::String(normalize_spaces_lower(s))
                        }
                        other => other.clone(),
                    };
                    HKey::Tuple(vec![
                        HKey::scalar(&subj)?,
                        HKey::Str(canon_pred),
                        HKey::scalar(&canon_obj)?,
                    ])
                } else {
                    HKey::Tuple(vec![
                        HKey::scalar(&subj)?,
                        HKey::Str(pred),
                        HKey::scalar(&obj)?,
                    ])
                };
                if seen.insert(rel_key) {
                    all_relationships.push(rel);
                }
            }
        }
        Ok(all_relationships)
    }

    /// `_merge_metadata(entities, base_entity)`: the base entity's metadata plus the other
    /// entities' new keys; lists are extended and dicts updated.
    fn merge_metadata(&self, entities: &[Value], base: usize) -> Result<Value, SemanticaError> {
        let base_entity = &entities[base];
        let mut merged_metadata = copy(&get_or(base_entity, "metadata", json!({}))?)?;
        for entity in entities {
            if eq(entity, base_entity) {
                continue;
            }
            for (key, value) in items(&get_or(entity, "metadata", json!({}))?)? {
                if !contains(&merged_metadata, &Value::String(key.clone()))? {
                    set_item(&mut merged_metadata, &key, value)?;
                    continue;
                }
                let Value::Object(map) = &mut merged_metadata else {
                    return Err(type_error("list indices must be integers or slices, not str"));
                };
                match (map.get_mut(&key), value) {
                    (Some(Value::Array(existing)), Value::Array(extra)) => existing.extend(extra),
                    (Some(Value::Object(existing)), Value::Object(extra)) => {
                        for (k, v) in extra {
                            existing.insert(k, v);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(merged_metadata)
    }

    /// `validate_merge(merge_result)`: `{"valid", "issues", "quality_score"}` — issues for a
    /// missing name or type and for unresolved conflicts; the score drops 0.1 per issue.
    pub fn validate_merge(&self, merge_result: &MergeResult) -> Map<String, Value> {
        let merged_entity = &merge_result.merged_entity;
        let mut issues: Vec<String> = Vec::new();
        if !merged_entity.get("name").is_some_and(truthy) {
            issues.push("Missing name".to_string());
        }
        if !merged_entity.get("type").is_some_and(truthy) {
            issues.push("Missing type".to_string());
        }
        if !merge_result.conflicts.is_empty() {
            issues.push(format!(
                "{} unresolved conflicts",
                merge_result.conflicts.len()
            ));
        }
        let quality_score = 1.0 - (issues.len() as f64 * 0.1);
        let mut result = Map::new();
        result.insert("valid".to_string(), Value::Bool(issues.is_empty()));
        result.insert("issues".to_string(), json!(issues));
        result.insert(
            "quality_score".to_string(),
            crate::pyops::float_value(quality_score),
        );
        result
    }
}

/// `container[key]` for a JSON container read with a string key.
fn subscript(container: &Value, key: &str) -> Result<Value, SemanticaError> {
    match container {
        Value::Object(map) => map
            .get(key)
            .cloned()
            .ok_or_else(|| SemanticaError::Key(key.to_string())),
        Value::Array(_) => Err(type_error(
            "list indices must be integers or slices, not str",
        )),
        Value::String(_) => Err(type_error("string indices must be integers, not 'str'")),
        other => Err(type_error(format!(
            "'{}' object is not subscriptable",
            sem_utils::pycompat::fmt::type_name(other)
        ))),
    }
}
