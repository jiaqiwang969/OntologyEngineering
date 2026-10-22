//! Similarity Calculator Module (`semantica/deduplication/similarity_calculator.py`).
//!
//! Multi-factor similarity between entities: string similarity of the names (Levenshtein,
//! Jaro-Winkler, bigram "cosine"), property similarity, relationship (Jaccard) similarity and
//! embedding (cosine) similarity, aggregated with normalised weights. Batch comparison uses a
//! blocking strategy (`legacy` first-letter blocks, `blocking_v2`/`hybrid_v2` token, type and
//! phonetic blocks), an optional per-entity candidate cap and an optional fast prefilter.
//!
//! Scores reproduce the original formulas operation for operation (same floating-point
//! evaluation order).

use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::{IndexMap, IndexSet};
use sem_utils::SemanticaError;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::{format_fixed, type_name};
use sem_utils::{get_logger, get_progress_tracker};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::entity::{AsEntity, EntityRef, entity_refs};
use crate::kwargs::{Kwargs, float_param, merge_config};
use crate::pyops::{
    Cmp, HKey, add, attr_error, compare, compare_f64, contains, float_value, get, get_or,
    has_key, is_int, iterate, keys, lower, lower_strip, mul, mul_float, name_or_text, number,
    prefix_chars, py_len, py_str, split_ws, str_for, sub, truthy, type_error, upper,
};

/// Similarity calculation result (`SimilarityResult` dataclass).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarityResult {
    /// Overall similarity score.
    pub score: f64,
    /// How the score was obtained (`multi_factor`, `short_circuit`, `prefiltered`, ...).
    pub method: String,
    /// Component scores (`string`, `property`, `relationship`, `embedding`), if computed.
    pub components: Option<IndexMap<String, f64>>,
    /// Extra information (`weights`, `score_breakdown`, `rejection_reason`).
    pub metadata: Option<Map<String, Value>>,
}

impl SimilarityResult {
    /// `SimilarityResult(score, method)` (no components, no metadata).
    pub fn new(score: f64, method: impl Into<String>) -> SimilarityResult {
        SimilarityResult {
            score,
            method: method.into(),
            components: None,
            metadata: None,
        }
    }
}

/// One similar pair from [`SimilarityCalculator::batch_calculate_similarity`]: the two input
/// entities (the caller's references) and their score.
pub type SimilarPair = (EntityRef, EntityRef, f64);

/// The similarity operations the cluster builder delegates to
/// (`ClusterBuilder.similarity_calculator`). [`SimilarityCalculator`] is the implementation;
/// tests substitute fakes where the original tests patch these methods.
pub trait SimilarityBackend: Send + Sync {
    /// `calculate_similarity(entity1, entity2, track=True, **options)`.
    fn calculate_similarity(
        &self,
        entity1: &Value,
        entity2: &Value,
        track: bool,
        options: &Map<String, Value>,
    ) -> Result<SimilarityResult, SemanticaError>;

    /// `batch_calculate_similarity(entities, threshold=None, **kwargs)`.
    fn batch_calculate_similarity(
        &self,
        entities: &[EntityRef],
        threshold: Option<f64>,
        options: &Map<String, Value>,
    ) -> Result<Vec<SimilarPair>, SemanticaError>;
}

/// An entity as `calculate_similarity` sees it: the (possibly pre-processed) dict and, for
/// batch pre-processing, the precomputed `_hashable_rels` set.
#[derive(Clone, Copy)]
pub(crate) struct Prepared<'a> {
    pub(crate) value: &'a Value,
    pub(crate) hashable_rels: Option<&'a HashSet<HKey>>,
}

impl<'a> Prepared<'a> {
    pub(crate) fn raw(value: &'a Value) -> Prepared<'a> {
        Prepared {
            value,
            hashable_rels: None,
        }
    }
}

/// Similarity calculation engine for entity comparison (`SimilarityCalculator`).
pub struct SimilarityCalculator {
    logger: Logger,
    /// `config` merged with the extra keyword arguments.
    pub config: Map<String, Value>,
    /// Weight of the embedding component.
    pub embedding_weight: f64,
    /// Weight of the string (name) component.
    pub string_weight: f64,
    /// Weight of the property component.
    pub property_weight: f64,
    /// Weight of the relationship component.
    pub relationship_weight: f64,
    /// Default similarity threshold.
    pub similarity_threshold: f64,
    /// `config["prefilter_enabled"]` (default `False`; read with Python truthiness).
    pub prefilter_enabled: Value,
    score_breakdown_enabled: Mutex<Value>,
    /// `config["prefilter_thresholds"]` (default
    /// `{"min_length_ratio": 0.3, "min_token_overlap_ratio": 0.0, "required_shared_token": False}`).
    pub prefilter_thresholds: Value,
    progress_tracker: Arc<ProgressTracker>,
}

impl fmt::Debug for SimilarityCalculator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimilarityCalculator")
            .field("config", &self.config)
            .field("embedding_weight", &self.embedding_weight)
            .field("string_weight", &self.string_weight)
            .field("property_weight", &self.property_weight)
            .field("relationship_weight", &self.relationship_weight)
            .field("similarity_threshold", &self.similarity_threshold)
            .field("prefilter_enabled", &self.prefilter_enabled)
            .field("score_breakdown_enabled", &*self.breakdown())
            .field("prefilter_thresholds", &self.prefilter_thresholds)
            .finish()
    }
}

impl Clone for SimilarityCalculator {
    fn clone(&self) -> Self {
        SimilarityCalculator {
            logger: self.logger.clone(),
            config: self.config.clone(),
            embedding_weight: self.embedding_weight,
            string_weight: self.string_weight,
            property_weight: self.property_weight,
            relationship_weight: self.relationship_weight,
            similarity_threshold: self.similarity_threshold,
            prefilter_enabled: self.prefilter_enabled.clone(),
            score_breakdown_enabled: Mutex::new(self.score_breakdown_enabled()),
            prefilter_thresholds: self.prefilter_thresholds.clone(),
            progress_tracker: self.progress_tracker.clone(),
        }
    }
}

impl Default for SimilarityCalculator {
    fn default() -> Self {
        SimilarityCalculator::new(&Map::new()).expect("default arguments are valid")
    }
}

/// The tracker every component uses, enabled as the original constructors ensure.
pub(crate) fn enabled_tracker() -> Arc<ProgressTracker> {
    let tracker = get_progress_tracker();
    if !tracker.enabled() {
        tracker.set_enabled(true);
    }
    tracker
}

impl SimilarityCalculator {
    /// `SimilarityCalculator(embedding_weight=0.0, string_weight=0.6, property_weight=0.2,
    /// relationship_weight=0.2, similarity_threshold=0.7, config=None, **kwargs)`, with the
    /// keyword arguments as a map. Weights that Python cannot add raise its `TypeError`.
    pub fn new(kwargs: &Map<String, Value>) -> Result<SimilarityCalculator, SemanticaError> {
        let logger = get_logger("similarity_calculator");
        let mut kw = Kwargs::new(kwargs);
        let embedding_weight = kw.take("embedding_weight").unwrap_or(json!(0.0));
        let string_weight = kw.take("string_weight").unwrap_or(json!(0.6));
        let property_weight = kw.take("property_weight").unwrap_or(json!(0.2));
        let relationship_weight = kw.take("relationship_weight").unwrap_or(json!(0.2));
        let similarity_threshold = kw.take("similarity_threshold");
        let config = kw.take("config");
        let config = merge_config(config, kw.rest())?;

        let prefilter_enabled = config
            .get("prefilter_enabled")
            .cloned()
            .unwrap_or(Value::Bool(false));
        let score_breakdown_enabled = config
            .get("score_breakdown_enabled")
            .cloned()
            .unwrap_or(Value::Bool(false));
        let prefilter_thresholds = config.get("prefilter_thresholds").cloned().unwrap_or(json!({
            "min_length_ratio": 0.3,
            "min_token_overlap_ratio": 0.0,
            "required_shared_token": false
        }));

        // total_weight = embedding + string + property + relationship (Python arithmetic).
        let total_weight = add(
            &add(&add(&embedding_weight, &string_weight)?, &property_weight)?,
            &relationship_weight,
        )?;
        let deviation = sub(&total_weight, &json!(1.0))?.abs();
        if deviation > 0.01 {
            logger.debug(format!(
                "Weights sum to {}, will be normalized during calculation",
                format_fixed(number(&total_weight).unwrap_or(f64::NAN), 2)
            ));
        }
        let weight = |v: &Value| number(v).unwrap_or(f64::NAN);
        let calculator = SimilarityCalculator {
            embedding_weight: weight(&embedding_weight),
            string_weight: weight(&string_weight),
            property_weight: weight(&property_weight),
            relationship_weight: weight(&relationship_weight),
            similarity_threshold: float_param("similarity_threshold", similarity_threshold, 0.7)?,
            config,
            prefilter_enabled,
            score_breakdown_enabled: Mutex::new(score_breakdown_enabled),
            prefilter_thresholds,
            progress_tracker: enabled_tracker(),
            logger,
        };
        calculator.logger.debug("Similarity calculator initialized");
        Ok(calculator)
    }

    fn breakdown(&self) -> MutexGuard<'_, Value> {
        self.score_breakdown_enabled
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// `score_breakdown_enabled` (updated by every `calculate_similarity` call that passes the
    /// option, as in the original).
    pub fn score_breakdown_enabled(&self) -> Value {
        self.breakdown().clone()
    }

    /// Set `score_breakdown_enabled`.
    pub fn set_score_breakdown_enabled(&self, value: Value) {
        *self.breakdown() = value;
    }

    /// The logger (`semantica.similarity_calculator`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `_prefilter_pair(entity1, entity2, thresholds)`: `(passed, rejection_reason)`.
    fn prefilter_pair(
        &self,
        entity1: &Value,
        entity2: &Value,
        thresholds: &Value,
    ) -> Result<(bool, String), SemanticaError> {
        let type1 = get(entity1, "type")?;
        let type2 = get(entity2, "type")?;
        if let (Some(t1), Some(t2)) = (type1, type2)
            && truthy(t1)
            && truthy(t2)
            && !crate::pyops::eq(t1, t2)
        {
            return Ok((false, "type_mismatch".to_string()));
        }

        let name1 = prefilter_name(entity1)?;
        let name2 = prefilter_name(entity2)?;
        if !truthy(&name1) || !truthy(&name2) {
            return Ok((true, String::new()));
        }

        let len1 = py_len(&name1)?;
        let len2 = py_len(&name2)?;
        if len1 > 0 || len2 > 0 {
            let length_ratio = len1.min(len2) as f64 / len1.max(len2) as f64;
            let min_length_ratio = get_or(thresholds, "min_length_ratio", json!(0.3))?;
            if compare_f64(length_ratio, &min_length_ratio, Cmp::Lt)? {
                return Ok((
                    false,
                    format!("length_ratio_below_{}", py_str(&min_length_ratio)),
                ));
            }
        }

        let require_shared = get_or(thresholds, "require_shared_token", json!(false))?;
        let token_gate = truthy(&require_shared) || {
            let ratio = get_or(thresholds, "min_token_overlap_ratio", json!(0.0))?;
            compare(&ratio, &json!(0), Cmp::Gt)?
        };
        if token_gate {
            let tokens1 = prefilter_tokens(&name1)?;
            let tokens2 = prefilter_tokens(&name2)?;
            if !tokens1.is_empty() && !tokens2.is_empty() {
                let overlap = tokens1.intersection(&tokens2).count();
                let min_overlap_ratio = get_or(thresholds, "min_token_overlap_ratio", json!(0.0))?;
                if compare(&min_overlap_ratio, &json!(0), Cmp::Gt)? {
                    let max_possible_overlap = tokens1.len().min(tokens2.len());
                    let overlap_ratio = overlap as f64 / max_possible_overlap as f64;
                    if compare_f64(overlap_ratio, &min_overlap_ratio, Cmp::Lt)? {
                        return Ok((
                            false,
                            format!("token_overlap_below_{}", py_str(&min_overlap_ratio)),
                        ));
                    }
                } else if truthy(&get_or(thresholds, "require_shared_token", json!(false))?)
                    && overlap == 0
                {
                    return Ok((false, "no_shared_tokens".to_string()));
                }
            }
        }
        Ok((true, String::new()))
    }

    /// `calculate_similarity(entity1, entity2, track=True, **options)`: the multi-factor score.
    ///
    /// With the prefilter enabled (`prefilter_enabled` in the config or options) an obvious
    /// non-match returns score `0.0` and method `prefiltered`; a pair whose names are too
    /// dissimilar returns early with method `short_circuit`.
    pub fn calculate_similarity(
        &self,
        entity1: &Value,
        entity2: &Value,
        track: bool,
        options: &Map<String, Value>,
    ) -> Result<SimilarityResult, SemanticaError> {
        self.similarity_prepared(Prepared::raw(entity1), Prepared::raw(entity2), track, options)
    }

    pub(crate) fn similarity_prepared(
        &self,
        entity1: Prepared<'_>,
        entity2: Prepared<'_>,
        track: bool,
        options: &Map<String, Value>,
    ) -> Result<SimilarityResult, SemanticaError> {
        let tracking_id = if track {
            self.progress_tracker.start_tracking(
                None,
                Some("deduplication"),
                Some("SimilarityCalculator"),
                "Calculating similarity between entities",
                None,
            )
        } else {
            String::new()
        };
        let result = self.similarity_body(entity1, entity2, &tracking_id, options);
        if let Err(e) = &result
            && !tracking_id.is_empty()
        {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    fn update(&self, tracking_id: &str, message: &str) {
        if !tracking_id.is_empty() {
            self.progress_tracker
                .update_tracking(tracking_id, None, message);
        }
    }

    fn similarity_body(
        &self,
        entity1: Prepared<'_>,
        entity2: Prepared<'_>,
        tracking_id: &str,
        options: &Map<String, Value>,
    ) -> Result<SimilarityResult, SemanticaError> {
        // Fast prefilter
        let mut merged = self.config.clone();
        for (k, v) in options {
            merged.insert(k.clone(), v.clone());
        }
        let prefilter_enabled = merged
            .get("prefilter_enabled")
            .unwrap_or(&self.prefilter_enabled);
        if truthy(prefilter_enabled) {
            let thresholds = merged
                .get("prefilter_thresholds")
                .unwrap_or(&self.prefilter_thresholds);
            let (passed, reason) = self.prefilter_pair(entity1.value, entity2.value, thresholds)?;
            if !passed {
                if !tracking_id.is_empty() {
                    self.progress_tracker.stop_tracking(
                        tracking_id,
                        Some("completed"),
                        &format!("Rejected by prefilter: {reason}"),
                        None,
                    );
                }
                let mut metadata = Map::new();
                metadata.insert("rejection_reason".to_string(), Value::String(reason));
                return Ok(SimilarityResult {
                    score: 0.0,
                    method: "prefiltered".to_string(),
                    components: None,
                    metadata: Some(metadata),
                });
            }
        }

        let mut components: IndexMap<String, f64> = IndexMap::new();

        // String similarity
        self.update(tracking_id, "Calculating string similarity...");
        let name1 = similarity_name(entity1.value)?;
        let name2 = similarity_name(entity2.value)?;
        let string_score = string_similarity_values(&name1, &name2, "jaro_winkler")?;
        components.insert("string".to_string(), string_score);

        // Short-circuit if string similarity is too low
        if self.string_weight > 0.5
            && string_score < 0.3
            && !(has_key(entity1.value, "embedding")? && has_key(entity2.value, "embedding")?)
        {
            let overall_score = string_score * self.string_weight;
            let default_threshold = float_value(self.similarity_threshold);
            let threshold = crate::pyops::or(merged.get("threshold"), &default_threshold);
            if overall_score < mul_float(threshold, 0.5)? {
                return Ok(SimilarityResult {
                    score: overall_score,
                    method: "short_circuit".to_string(),
                    components: Some(components),
                    metadata: None,
                });
            }
        }

        // Property similarity
        self.update(tracking_id, "Calculating property similarity...");
        let property_score = self.calculate_property_similarity(entity1.value, entity2.value)?;
        components.insert("property".to_string(), property_score);

        // Relationship similarity
        self.update(tracking_id, "Calculating relationship similarity...");
        let relationship_score = relationship_similarity(entity1, entity2)?;
        components.insert("relationship".to_string(), relationship_score);

        // Embedding similarity
        let mut embedding_score = 0.0;
        if has_key(entity1.value, "embedding")? && has_key(entity2.value, "embedding")? {
            self.update(tracking_id, "Calculating embedding similarity...");
            embedding_score = embedding_similarity_values(
                &entity1.value["embedding"],
                &entity2.value["embedding"],
            )?;
            components.insert("embedding".to_string(), embedding_score);
        }

        // Weighted aggregation
        self.update(tracking_id, "Aggregating similarity scores...");
        let mut weights: IndexMap<&str, f64> = IndexMap::from([
            ("string", self.string_weight),
            ("property", self.property_weight),
            ("relationship", self.relationship_weight),
            (
                "embedding",
                if embedding_score > 0.0 {
                    self.embedding_weight
                } else {
                    0.0
                },
            ),
        ]);
        let total_weight = weights
            .iter()
            .filter(|(k, _)| components.contains_key(**k))
            .fold(0.0, |acc, (_, w)| acc + w);
        if total_weight > 0.0 {
            weights = weights
                .into_iter()
                .filter(|(k, _)| components.contains_key(*k))
                .map(|(k, w)| (k, w / total_weight))
                .collect();
        }
        let overall_score = weights.iter().fold(0.0, |acc, (k, w)| {
            acc + components.get(*k).copied().unwrap_or(0.0) * w
        });

        // Explainability metadata
        let mut metadata = Map::new();
        metadata.insert(
            "weights".to_string(),
            Value::Object(
                weights
                    .iter()
                    .map(|(k, w)| (k.to_string(), float_value(*w)))
                    .collect(),
            ),
        );
        let breakdown = merged
            .get("score_breakdown_enabled")
            .cloned()
            .unwrap_or_else(|| self.score_breakdown_enabled());
        self.set_score_breakdown_enabled(breakdown.clone());
        if truthy(&breakdown) {
            metadata.insert("score_breakdown".to_string(), components_value(&components));
        }

        if !tracking_id.is_empty() {
            self.progress_tracker.stop_tracking(
                tracking_id,
                Some("completed"),
                &format!("Similarity score: {}", format_fixed(overall_score, 2)),
                None,
            );
        }
        Ok(SimilarityResult {
            score: overall_score,
            method: "multi_factor".to_string(),
            components: Some(components),
            metadata: Some(metadata),
        })
    }

    /// `calculate_string_similarity(str1, str2, method="jaro_winkler")`: `levenshtein`,
    /// `jaro_winkler` or `cosine` (character bigrams) over the lowercased, stripped strings;
    /// any other method is Levenshtein.
    pub fn calculate_string_similarity(&self, str1: &str, str2: &str, method: Option<&str>) -> f64 {
        string_similarity(str1, str2, method.unwrap_or("jaro_winkler"))
    }

    /// `calculate_property_similarity(entity1, entity2)`: the mean per-property agreement
    /// (string similarity for two strings, 1.0 for equal values, 0.5 for a missing or a
    /// differing value); 1.0 when neither entity has properties.
    pub fn calculate_property_similarity(
        &self,
        entity1: &Value,
        entity2: &Value,
    ) -> Result<f64, SemanticaError> {
        let props1 = get_or(entity1, "properties", json!({}))?;
        let props2 = get_or(entity2, "properties", json!({}))?;
        if !truthy(&props1) && !truthy(&props2) {
            return Ok(1.0);
        }
        // `set(props1.keys()) | set(props2.keys())`, iterated in first-seen order.
        let mut all_keys: IndexSet<String> = keys(&props1)?.into_iter().collect();
        all_keys.extend(keys(&props2)?);
        if all_keys.is_empty() {
            return Ok(0.0);
        }

        let mut matches = 0.0;
        let mut total = 0usize;
        for key in &all_keys {
            let val1 = props1.get(key).filter(|v| !v.is_null());
            let val2 = props2.get(key).filter(|v| !v.is_null());
            let (Some(val1), Some(val2)) = (val1, val2) else {
                // Missing value in one entity is not a mismatch, but lack of evidence.
                matches += 0.5;
                total += 1;
                continue;
            };
            if let (Value::String(s1), Value::String(s2)) = (val1, val2) {
                matches += string_similarity(s1, s2, "jaro_winkler");
            } else if crate::pyops::eq(val1, val2) {
                matches += 1.0;
            } else {
                matches += 0.5; // Partial match
            }
            total += 1;
        }
        Ok(if total > 0 {
            matches / total as f64
        } else {
            0.0
        })
    }

    /// `calculate_relationship_similarity(entity1, entity2)`: Jaccard similarity of the
    /// relationship sets (0.5 when neither has relationships, 0.0 when one has none).
    pub fn calculate_relationship_similarity(
        &self,
        entity1: &Value,
        entity2: &Value,
    ) -> Result<f64, SemanticaError> {
        relationship_similarity(Prepared::raw(entity1), Prepared::raw(entity2))
    }

    /// `calculate_embedding_similarity(embedding1, embedding2)`: cosine similarity mapped to
    /// `[0, 1]` (`(cos + 1) / 2`); 0.0 for different lengths or a zero vector.
    pub fn calculate_embedding_similarity(&self, embedding1: &[f64], embedding2: &[f64]) -> f64 {
        if embedding1.len() != embedding2.len() {
            return 0.0;
        }
        let dot_product = embedding1
            .iter()
            .zip(embedding2)
            .fold(0.0, |acc, (a, b)| acc + a * b);
        let magnitude1 = embedding1.iter().fold(0.0, |acc, a| acc + a * a).sqrt();
        let magnitude2 = embedding2.iter().fold(0.0, |acc, b| acc + b * b).sqrt();
        if magnitude1 == 0.0 || magnitude2 == 0.0 {
            return 0.0;
        }
        let cosine_sim = dot_product / (magnitude1 * magnitude2);
        (cosine_sim + 1.0) / 2.0
    }

    /// `calculate_embedding_similarity` for JSON vectors (Python arithmetic and errors).
    pub fn calculate_embedding_similarity_values(
        &self,
        embedding1: &Value,
        embedding2: &Value,
    ) -> Result<f64, SemanticaError> {
        embedding_similarity_values(embedding1, embedding2)
    }

    /// `batch_calculate_similarity(entities, threshold=None, **kwargs)`: every candidate pair
    /// produced by the blocking strategy whose score reaches `threshold` (`None`/`0` use
    /// `similarity_threshold`), in ascending `(i, j)` order.
    ///
    /// Options (merged over the config): `candidate_strategy` (`legacy`, `blocking_v2`,
    /// `hybrid_v2`), `blocking_keys`, `enable_phonetic_blocking`, `max_candidates_per_entity`.
    pub fn batch_calculate_similarity<E: AsEntity>(
        &self,
        entities: &[E],
        threshold: Option<f64>,
        options: &Map<String, Value>,
    ) -> Result<Vec<SimilarPair>, SemanticaError> {
        self.batch_refs(&entity_refs(entities), threshold, options)
    }

    pub(crate) fn batch_refs(
        &self,
        entities: &[EntityRef],
        threshold: Option<f64>,
        options: &Map<String, Value>,
    ) -> Result<Vec<SimilarPair>, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("deduplication"),
            Some("SimilarityCalculator"),
            &format!("Calculating similarity for {} entities", entities.len()),
            None,
        );
        let result = self.batch_body(entities, threshold, options, &tracking_id);
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    fn batch_body(
        &self,
        entities: &[EntityRef],
        threshold: Option<f64>,
        kwargs: &Map<String, Value>,
        tracking_id: &str,
    ) -> Result<Vec<SimilarPair>, SemanticaError> {
        // `threshold or self.similarity_threshold`
        let threshold = match threshold {
            Some(t) if t != 0.0 => t,
            _ => self.similarity_threshold,
        };
        let mut results = Vec::new();
        let mut options = self.config.clone();
        for (k, v) in kwargs {
            options.insert(k.clone(), v.clone());
        }

        let mut processed: Vec<Value> = Vec::with_capacity(entities.len());
        let mut rel_sets: Vec<HashSet<HKey>> = Vec::with_capacity(entities.len());
        for entity in entities {
            let (value, rels) = prepare_entity(entity)?;
            processed.push(value);
            rel_sets.push(rels);
        }

        let blocks = self.build_block_indexes(&processed, &options)?;
        let mut candidate_pairs = generate_candidate_pairs(&blocks);
        if let Some(max_candidates) = options
            .get("max_candidates_per_entity")
            .filter(|v| !v.is_null())
        {
            candidate_pairs = cap_candidate_pairs(candidate_pairs, max_candidates)?;
        }

        let total_pairs = candidate_pairs.len();
        let mut processed_count = 0usize;
        let update_interval = if total_pairs <= 10 {
            1
        } else {
            (total_pairs / 100).clamp(1, 100)
        };
        self.progress_tracker.update_tracking(
            tracking_id,
            Some("running"),
            &format!("Comparing {total_pairs} generated pairs..."),
        );

        let mut call_options = Map::new();
        call_options.insert("threshold".to_string(), float_value(threshold));
        for (i, j) in candidate_pairs {
            let similarity = self.similarity_prepared(
                Prepared {
                    value: &processed[i],
                    hashable_rels: Some(&rel_sets[i]),
                },
                Prepared {
                    value: &processed[j],
                    hashable_rels: Some(&rel_sets[j]),
                },
                false,
                &call_options,
            )?;
            if similarity.score >= threshold {
                results.push((entities[i].clone(), entities[j].clone(), similarity.score));
            }
            processed_count += 1;
            if processed_count % update_interval == 0 || processed_count == total_pairs {
                self.progress_tracker.update_progress(
                    tracking_id,
                    processed_count as i64,
                    total_pairs as i64,
                    &format!("Comparing candidates... {processed_count}/{total_pairs}"),
                );
            }
        }

        self.progress_tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!(
                "Found {} similar pairs out of {total_pairs} candidates",
                results.len()
            ),
            None,
        );
        Ok(results)
    }

    /// `_build_block_indexes(processed_entities, options)`: block key -> entity indices.
    fn build_block_indexes(
        &self,
        processed: &[Value],
        options: &Map<String, Value>,
    ) -> Result<IndexMap<String, Vec<usize>>, SemanticaError> {
        let mut blocks: IndexMap<String, Vec<usize>> = IndexMap::new();
        let legacy = json!("legacy");
        let strategy = options.get("candidate_strategy").unwrap_or(&legacy);
        let strategy = strategy.as_str();
        for (idx, entity) in processed.iter().enumerate() {
            let name = entity
                .get("_lower_name")
                .and_then(Value::as_str)
                .unwrap_or("");
            if name.is_empty() {
                blocks.entry("___empty___".to_string()).or_default().push(idx);
                continue;
            }
            match strategy {
                Some("legacy") => {
                    let first = name.chars().next().map(String::from).unwrap_or_default();
                    blocks.entry(first).or_default().push(idx);
                }
                Some("blocking_v2") | Some("hybrid_v2") => {
                    let mut keys_to_add: IndexSet<String> = IndexSet::new();
                    let tokens = name_tokens(name);
                    for t in &tokens {
                        keys_to_add.insert(format!("tok:{}", prefix_chars(t, 4)));
                    }
                    let default_keys = json!(["prefix", "token"]);
                    let blocking_keys = options.get("blocking_keys").unwrap_or(&default_keys);
                    if contains(blocking_keys, &json!("type"))? {
                        let e_type = lower(&py_str(
                            entity.get("type").unwrap_or(&json!("unknown")),
                        ));
                        if let Some(first) = tokens.first() {
                            keys_to_add.insert(format!("type:{e_type}:{}", prefix_chars(first, 4)));
                        }
                    }
                    if truthy(
                        options
                            .get("enable_phonetic_blocking")
                            .unwrap_or(&Value::Bool(false)),
                    ) {
                        for t in &tokens {
                            keys_to_add.insert(format!("pho:{}", soundex(t)));
                        }
                    }
                    for k in keys_to_add {
                        blocks.entry(k).or_default().push(idx);
                    }
                }
                // Any other strategy puts named entities in no block.
                _ => {}
            }
        }
        Ok(blocks)
    }

    /// `_soundex(word)`: the phonetic key used by phonetic blocking.
    pub fn soundex(&self, word: &str) -> String {
        soundex(word)
    }
}

impl SimilarityBackend for SimilarityCalculator {
    fn calculate_similarity(
        &self,
        entity1: &Value,
        entity2: &Value,
        track: bool,
        options: &Map<String, Value>,
    ) -> Result<SimilarityResult, SemanticaError> {
        SimilarityCalculator::calculate_similarity(self, entity1, entity2, track, options)
    }

    fn batch_calculate_similarity(
        &self,
        entities: &[EntityRef],
        threshold: Option<f64>,
        options: &Map<String, Value>,
    ) -> Result<Vec<SimilarPair>, SemanticaError> {
        self.batch_refs(entities, threshold, options)
    }
}

/// The components as a JSON object.
pub(crate) fn components_value(components: &IndexMap<String, f64>) -> Value {
    Value::Object(
        components
            .iter()
            .map(|(k, v)| (k.clone(), float_value(*v)))
            .collect(),
    )
}

/// `entity.get("_lower_name") or str(entity.get("name") or entity.get("text") or "").lower().strip()`.
fn prefilter_name(entity: &Value) -> Result<Value, SemanticaError> {
    if let Some(v) = get(entity, "_lower_name")?.filter(|v| truthy(v)) {
        return Ok(v.clone());
    }
    let name = name_or_text(entity)?;
    Ok(Value::String(lower_strip(&py_str(&name))))
}

/// The prefilter's token set: words longer than two characters once `_`/`-` became spaces.
fn prefilter_tokens(name: &Value) -> Result<HashSet<String>, SemanticaError> {
    Ok(long_tokens(str_for(name, "replace")?).into_iter().collect())
}

/// `[t for t in name.replace("_", " ").replace("-", " ").split() if len(t) > 2]`.
fn long_tokens(name: &str) -> Vec<String> {
    let replaced = name.replace('_', " ").replace('-', " ");
    split_ws(&replaced)
        .into_iter()
        .filter(|t| t.chars().count() > 2)
        .map(str::to_string)
        .collect()
}

/// The blocking tokens: [`long_tokens`], or `[name]` when there are none.
fn name_tokens(name: &str) -> Vec<String> {
    let tokens = long_tokens(name);
    if tokens.is_empty() {
        vec![name.to_string()]
    } else {
        tokens
    }
}

/// The name `calculate_similarity` compares: `_lower_name` unless it is `None`, else
/// `name or text or ""`.
fn similarity_name(entity: &Value) -> Result<Value, SemanticaError> {
    match get(entity, "_lower_name")? {
        Some(v) if !v.is_null() => Ok(v.clone()),
        _ => name_or_text(entity),
    }
}

/// `calculate_string_similarity` on two strings.
pub(crate) fn string_similarity(str1: &str, str2: &str, method: &str) -> f64 {
    if str1.is_empty() || str2.is_empty() {
        return 0.0;
    }
    let s1 = lower_strip(str1);
    let s2 = lower_strip(str2);
    if s1 == s2 {
        return 1.0;
    }
    match method {
        "jaro_winkler" => jaro_winkler_similarity(&s1, &s2),
        "cosine" => bigram_similarity(&s1, &s2),
        _ => levenshtein_similarity(&s1, &s2),
    }
}

/// `calculate_string_similarity` on arbitrary values: falsy values score 0.0; other
/// non-strings raise the `AttributeError` of `.lower()`.
pub(crate) fn string_similarity_values(
    value1: &Value,
    value2: &Value,
    method: &str,
) -> Result<f64, SemanticaError> {
    if !truthy(value1) || !truthy(value2) {
        return Ok(0.0);
    }
    let s1 = str_for(value1, "lower")?;
    let s2 = str_for(value2, "lower")?;
    Ok(string_similarity(s1, s2, method))
}

/// `_levenshtein_similarity(s1, s2)`: `1 - distance / max_len`.
fn levenshtein_similarity(s1: &str, s2: &str) -> f64 {
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    if s1 == s2 {
        return 1.0;
    }
    // Edit distance over code points (the original's dynamic programme gives the same count).
    let distance = strsim::levenshtein(s1, s2);
    let max_len = s1.chars().count().max(s2.chars().count());
    if max_len > 0 {
        1.0 - (distance as f64 / max_len as f64)
    } else {
        0.0
    }
}

/// `_jaro_winkler_similarity(s1, s2)`: Jaro plus `min(prefix, 4) * 0.1 * (1 - jaro)` (the
/// prefix bonus is applied whatever the Jaro score).
fn jaro_winkler_similarity(s1: &str, s2: &str) -> f64 {
    if s1 == s2 {
        return 1.0;
    }
    let a: Vec<char> = s1.chars().collect();
    let b: Vec<char> = s2.chars().collect();
    let jaro = jaro_similarity(&a, &b);
    let mut prefix_len = 0usize;
    for i in 0..a.len().min(b.len()) {
        if a[i] == b[i] {
            prefix_len += 1;
        } else {
            break;
        }
    }
    let prefix_bonus = prefix_len.min(4) as f64 * 0.1;
    jaro + prefix_bonus * (1.0 - jaro)
}

/// `_jaro_similarity(s1, s2)` (match window `max(len) // 2 - 1`, half transpositions).
fn jaro_similarity(s1: &[char], s2: &[char]) -> f64 {
    if s1 == s2 {
        return 1.0;
    }
    let match_window = (s1.len().max(s2.len()) / 2).saturating_sub(1);
    let mut s1_matches = vec![false; s1.len()];
    let mut s2_matches = vec![false; s2.len()];
    let mut matches = 0usize;
    let mut transpositions = 0usize;

    for i in 0..s1.len() {
        let start = i.saturating_sub(match_window);
        let end = (i + match_window + 1).min(s2.len());
        for j in start..end {
            if s2_matches[j] || s1[i] != s2[j] {
                continue;
            }
            s1_matches[i] = true;
            s2_matches[j] = true;
            matches += 1;
            break;
        }
    }
    if matches == 0 {
        return 0.0;
    }

    let mut k = 0usize;
    for i in 0..s1.len() {
        if !s1_matches[i] {
            continue;
        }
        while !s2_matches[k] {
            k += 1;
        }
        if s1[i] != s2[k] {
            transpositions += 1;
        }
        k += 1;
    }

    let m = matches as f64;
    (m / s1.len() as f64 + m / s2.len() as f64 + (m - transpositions as f64 / 2.0) / m) / 3.0
}

/// `_cosine_similarity(s1, s2)`: Jaccard similarity of the character bigram sets (1.0 when
/// both strings are too short to have bigrams).
fn bigram_similarity(s1: &str, s2: &str) -> f64 {
    fn bigrams(text: &str) -> HashSet<(char, char)> {
        let chars: Vec<char> = text.chars().collect();
        chars.windows(2).map(|w| (w[0], w[1])).collect()
    }
    let bigrams1 = bigrams(s1);
    let bigrams2 = bigrams(s2);
    if bigrams1.is_empty() && bigrams2.is_empty() {
        return 1.0;
    }
    let intersection = bigrams1.intersection(&bigrams2).count();
    let union = bigrams1.union(&bigrams2).count();
    if union > 0 {
        intersection as f64 / union as f64
    } else {
        0.0
    }
}

/// The relationship collection `calculate_relationship_similarity` compares.
enum Rels<'a> {
    Set(Cow<'a, HashSet<HKey>>),
    /// A user-supplied `_hashable_rels` value.
    Raw(&'a Value),
}

impl Rels<'_> {
    fn truthy(&self) -> bool {
        match self {
            Rels::Set(set) => !set.is_empty(),
            Rels::Raw(v) => truthy(v),
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            Rels::Set(_) => "set",
            Rels::Raw(v) => type_name(v),
        }
    }
}

fn rels_of(entity: Prepared<'_>) -> Result<Rels<'_>, SemanticaError> {
    if let Some(set) = entity.hashable_rels {
        return Ok(Rels::Set(Cow::Borrowed(set)));
    }
    match entity.value {
        Value::Object(map) => {
            if let Some(raw) = map.get("_hashable_rels") {
                return Ok(Rels::Raw(raw));
            }
            let rels = map.get("relationships").cloned().unwrap_or(json!([]));
            Ok(Rels::Set(Cow::Owned(
                iterate(&rels)?.iter().map(HKey::hashable).collect(),
            )))
        }
        other => {
            if contains(other, &json!("_hashable_rels"))? {
                Err(type_error(match other {
                    Value::String(_) => "string indices must be integers, not 'str'",
                    _ => "list indices must be integers or slices, not str",
                }))
            } else {
                Err(attr_error(other, "get"))
            }
        }
    }
}

fn relationship_similarity(
    entity1: Prepared<'_>,
    entity2: Prepared<'_>,
) -> Result<f64, SemanticaError> {
    let rels1 = rels_of(entity1)?;
    let rels2 = rels_of(entity2)?;
    if !rels1.truthy() && !rels2.truthy() {
        return Ok(0.5);
    }
    if !rels1.truthy() || !rels2.truthy() {
        return Ok(0.0);
    }
    match (&rels1, &rels2) {
        (Rels::Set(a), Rels::Set(b)) => {
            let intersection = a.intersection(b).count();
            let union = a.union(b).count();
            Ok(if union > 0 {
                intersection as f64 / union as f64
            } else {
                0.0
            })
        }
        (Rels::Raw(a), Rels::Raw(b)) if is_int(a) && is_int(b) => {
            // `int & int` works; `len()` of the result does not.
            let kind = if a.is_boolean() && b.is_boolean() {
                "bool"
            } else {
                "int"
            };
            Err(type_error(format!("object of type '{kind}' has no len()")))
        }
        _ => Err(type_error(format!(
            "unsupported operand type(s) for &: '{}' and '{}'",
            rels1.type_name(),
            rels2.type_name()
        ))),
    }
}

/// `calculate_embedding_similarity` on JSON values, with Python's arithmetic (exact integer
/// products and sums) and errors.
fn embedding_similarity_values(
    embedding1: &Value,
    embedding2: &Value,
) -> Result<f64, SemanticaError> {
    if py_len(embedding1)? != py_len(embedding2)? {
        return Ok(0.0);
    }
    let items1 = iterate(embedding1)?;
    let items2 = iterate(embedding2)?;
    let mut dot_product = json!(0);
    for (a, b) in items1.iter().zip(&items2) {
        dot_product = add(&dot_product, &mul(a, b)?)?;
    }
    let magnitude = |items: &[Value]| -> Result<f64, SemanticaError> {
        let mut sum = json!(0);
        for a in items {
            sum = add(&sum, &mul(a, a)?)?;
        }
        // `math.sqrt(sum)`: the squares of numbers always sum to a number.
        Ok(number(&sum).unwrap_or(f64::NAN).sqrt())
    };
    let magnitude1 = magnitude(&items1)?;
    let magnitude2 = magnitude(&items2)?;
    if magnitude1 == 0.0 || magnitude2 == 0.0 {
        return Ok(0.0);
    }
    let dot = number(&dot_product).unwrap_or(f64::NAN);
    let cosine_sim = dot / (magnitude1 * magnitude2);
    Ok((cosine_sim + 1.0) / 2.0)
}

/// The batch pre-processing of one entity: a dict copy carrying `_lower_name`, plus the
/// `_hashable_rels` set (from `relationships`, else `metadata.relationships`).
fn prepare_entity(entity: &Value) -> Result<(Value, HashSet<HKey>), SemanticaError> {
    let mut processed = match entity {
        Value::Object(map) => map.clone(),
        other => {
            let mut map = Map::new();
            map.insert("_original".to_string(), other.clone());
            map
        }
    };
    let mut rels = processed
        .get("relationships")
        .filter(|v| !v.is_null())
        .cloned();
    if rels.is_none()
        && let Some(metadata) = processed.get("metadata")
    {
        rels = get(metadata, "relationships")?
            .filter(|v| !v.is_null())
            .cloned();
    }
    let hashable_rels: HashSet<HKey> = match rels {
        Some(r) if truthy(&r) => iterate(&r)?.iter().map(HKey::hashable).collect(),
        _ => HashSet::new(),
    };
    processed.remove("_hashable_rels");
    let processed_value = Value::Object(processed);
    let name = name_or_text(&processed_value)?;
    let lower_name = lower_strip(str_for(&name, "lower")?);
    let Value::Object(mut processed) = processed_value else {
        unreachable!("processed entities are dicts")
    };
    processed.insert("_lower_name".to_string(), Value::String(lower_name));
    Ok((Value::Object(processed), hashable_rels))
}

/// `_generate_candidate_pairs(blocks)`: every `(i, j)` with `i < j` sharing a block, in
/// sorted order.
fn generate_candidate_pairs(blocks: &IndexMap<String, Vec<usize>>) -> BTreeSet<(usize, usize)> {
    let mut pairs = BTreeSet::new();
    for indices in blocks.values() {
        for (pos, &i) in indices.iter().enumerate() {
            for &j in &indices[pos + 1..] {
                pairs.insert((i.min(j), i.max(j)));
            }
        }
    }
    pairs
}

/// `_cap_candidate_pairs(candidate_pairs, max_candidates)`: each entity keeps its
/// `max_candidates` lowest-index neighbours.
fn cap_candidate_pairs(
    candidate_pairs: BTreeSet<(usize, usize)>,
    max_candidates: &Value,
) -> Result<BTreeSet<(usize, usize)>, SemanticaError> {
    if !truthy(max_candidates) || compare(max_candidates, &json!(0), Cmp::Le)? {
        return Ok(candidate_pairs);
    }
    let mut connections: IndexMap<usize, Vec<usize>> = IndexMap::new();
    for &(i, j) in &candidate_pairs {
        connections.entry(i).or_default().push(j);
        connections.entry(j).or_default().push(i);
    }
    let mut capped = BTreeSet::new();
    for (entity_idx, mut neighbors) in connections {
        if !is_int(max_candidates) {
            return Err(type_error(
                "slice indices must be integers or None or have an __index__ method",
            ));
        }
        let limit = number(max_candidates).unwrap_or(0.0).max(0.0) as usize;
        neighbors.sort_unstable();
        for n in neighbors.into_iter().take(limit) {
            capped.insert((entity_idx.min(n), entity_idx.max(n)));
        }
    }
    Ok(capped)
}

/// `_soundex(word)`.
fn soundex(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }
    const MAPPING: [(&str, char); 6] = [
        ("BFPV", '1'),
        ("CGJKQSXZ", '2'),
        ("DT", '3'),
        ("L", '4'),
        ("MN", '5'),
        ("R", '6'),
    ];
    let word: Vec<char> = upper(word).chars().collect();
    let mut result: Vec<char> = vec![word[0]];
    for &c in &word[1..] {
        for (key, val) in MAPPING {
            if key.contains(c) {
                if Some(&val) != result.last() {
                    result.push(val);
                }
                break;
            }
        }
    }
    let mut out: String = result[..1].iter().collect();
    out.extend(result[1..].iter().filter(|c| c.is_ascii_digit()));
    out.push_str("000");
    out.chars().take(4).collect()
}
