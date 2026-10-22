//! Duplicate Detector Module (`semantica/deduplication/duplicate_detector.py`).
//!
//! Duplicate candidates from batch similarity plus confidence scoring (exact name match,
//! matching property values, same type), result limiting (`min_similarity`, `sort_by`,
//! `top_k_per_entity`, `max_results`), duplicate groups by union-find, incremental
//! (new × existing) detection and relationship (triplet) duplicate detection.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use sem_utils::SemanticaError;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::get_logger;
use sem_utils::pycompat::fmt::{float_repr, str_repr};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::entity::{AsEntity, EntityRef, entity_refs, py_id, same_or_equal};
use crate::kwargs::{Kwargs, bool_param, float_param, merge_config};
use crate::pyops::{
    Cmp, HKey, compare, compare_f64, eq, get, is_int, keys, lower, lower_strip, mapping,
    normalize_spaces_lower, number, py_len, py_repr, py_str, set_item, truthy,
};
use crate::similarity_calculator::{
    SimilarityCalculator, enabled_tracker, string_similarity, string_similarity_values,
};

/// Duplicate candidate representation (`DuplicateCandidate` dataclass).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateCandidate {
    /// First entity (the caller's reference).
    #[serde(with = "crate::entity::serde_ref")]
    pub entity1: EntityRef,
    /// Second entity (the caller's reference).
    #[serde(with = "crate::entity::serde_ref")]
    pub entity2: EntityRef,
    /// Similarity score.
    pub similarity_score: f64,
    /// Confidence (similarity plus boosts, capped at 1.0).
    pub confidence: f64,
    /// Why the pair is a duplicate (`exact_name_match`, `<n>_property_matches`, `same_type`).
    #[serde(default)]
    pub reasons: Vec<String>,
    /// `name_match`, `common_properties`, `type_match`.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl DuplicateCandidate {
    /// `DuplicateCandidate(entity1, entity2, similarity_score, confidence)`.
    pub fn new(
        entity1: EntityRef,
        entity2: EntityRef,
        similarity_score: f64,
        confidence: f64,
    ) -> DuplicateCandidate {
        DuplicateCandidate {
            entity1,
            entity2,
            similarity_score,
            confidence,
            reasons: Vec::new(),
            metadata: Map::new(),
        }
    }
}

/// Group of duplicate entities (`DuplicateGroup` dataclass).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// The duplicate entities (the caller's references).
    #[serde(with = "crate::entity::serde_refs")]
    pub entities: Vec<EntityRef>,
    /// Similarity score per `(entity1_id, entity2_id)` pair of normalised ids.
    #[serde(default, with = "pair_scores")]
    pub similarity_scores: IndexMap<(String, String), f64>,
    /// Representative entity (the most complete one).
    #[serde(default, with = "crate::entity::serde_opt_ref")]
    pub representative: Option<EntityRef>,
    /// Group confidence.
    #[serde(default)]
    pub confidence: f64,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl DuplicateGroup {
    /// `DuplicateGroup(entities)` with the dataclass defaults.
    pub fn new(entities: Vec<EntityRef>) -> DuplicateGroup {
        DuplicateGroup {
            entities,
            ..DuplicateGroup::default()
        }
    }
}

/// The Python `repr()` of a `(id1, id2)` key (`('a1', 'a2')`), the form tuple-keyed score
/// dicts take in JSON.
pub fn pair_key_repr(key: &(String, String)) -> String {
    format!("({}, {})", str_repr(&key.0), str_repr(&key.1))
}

/// Tuple-keyed similarity scores as a JSON object keyed by [`pair_key_repr`].
pub fn similarity_scores_value(scores: &IndexMap<(String, String), f64>) -> Value {
    Value::Object(
        scores
            .iter()
            .map(|(k, v)| (pair_key_repr(k), crate::pyops::float_value(*v)))
            .collect(),
    )
}

/// Parse a Python string literal as `repr()` writes it; returns the value and the rest.
fn parse_str_literal(s: &str) -> Option<(String, &str)> {
    let mut chars = s.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        if c == quote {
            return Some((out, &s[i + c.len_utf8()..]));
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        let (_, e) = chars.next()?;
        let mut hex = |n: usize| -> Option<char> {
            let digits: String = (0..n).filter_map(|_| chars.next().map(|(_, d)| d)).collect();
            char::from_u32(u32::from_str_radix(&digits, 16).ok()?)
        };
        out.push(match e {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'x' => hex(2)?,
            'u' => hex(4)?,
            'U' => hex(8)?,
            other => other,
        });
    }
    None
}

/// Serde for tuple-keyed score maps (a JSON object keyed by `('a', 'b')` reprs).
pub(crate) mod pair_scores {
    use super::*;
    use serde::de::Error as _;
    use serde::{Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(
        scores: &IndexMap<(String, String), f64>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let map: IndexMap<String, f64> = scores
            .iter()
            .map(|(k, v)| (pair_key_repr(k), *v))
            .collect();
        map.serialize(s)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<IndexMap<(String, String), f64>, D::Error> {
        let map = IndexMap::<String, f64>::deserialize(d)?;
        map.into_iter()
            .map(|(k, v)| {
                parse_pair_key(&k)
                    .map(|key| (key, v))
                    .ok_or_else(|| D::Error::custom(format!("invalid pair key {k:?}")))
            })
            .collect()
    }
}

/// Parse a [`pair_key_repr`] key back into its two ids.
pub fn parse_pair_key(key: &str) -> Option<(String, String)> {
    let rest = key.strip_prefix('(')?;
    let (first, rest) = parse_str_literal(rest)?;
    let rest = rest.strip_prefix(", ")?;
    let (second, rest) = parse_str_literal(rest)?;
    (rest == ")").then_some((first, second))
}

/// `_get_entity_value(entity, key, default)`: `entity.get(key, default)` for a dict, the
/// default for anything else.
pub(crate) fn entity_value(entity: &Value, key: &str, default: Value) -> Value {
    match entity {
        Value::Object(map) => map.get(key).cloned().unwrap_or(default),
        _ => default,
    }
}

/// `_normalize_entity_id(entity)`: `str(entity["id"])`, or `str(id(entity))` without an id.
pub(crate) fn normalize_entity_id(entity: &EntityRef) -> String {
    match entity_value(entity, "id", Value::Null) {
        Value::Null => py_id(entity).to_string(),
        raw => py_str(&raw),
    }
}

/// Duplicate detection engine for knowledge graphs (`DuplicateDetector`).
pub struct DuplicateDetector {
    logger: Logger,
    /// `config` merged with the extra keyword arguments.
    pub config: Map<String, Value>,
    /// Built from `config["similarity"]`.
    pub similarity_calculator: SimilarityCalculator,
    /// Minimum similarity to consider a pair.
    pub similarity_threshold: f64,
    /// Minimum confidence of a returned candidate.
    pub confidence_threshold: f64,
    /// `use_clustering` (kept for compatibility; unused, as in the original).
    pub use_clustering: bool,
    /// Global cap on returned candidates.
    pub max_results: Option<i64>,
    /// Per-entity cap (a candidate is kept while either entity is under quota).
    pub top_k_per_entity: Option<i64>,
    /// Extra similarity floor.
    pub min_similarity: Option<f64>,
    /// `"confidence"` or `"similarity_score"`.
    pub sort_by: String,
    progress_tracker: Arc<ProgressTracker>,
}

impl fmt::Debug for DuplicateDetector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DuplicateDetector")
            .field("config", &self.config)
            .field("similarity_calculator", &self.similarity_calculator)
            .field("similarity_threshold", &self.similarity_threshold)
            .field("confidence_threshold", &self.confidence_threshold)
            .field("use_clustering", &self.use_clustering)
            .field("max_results", &self.max_results)
            .field("top_k_per_entity", &self.top_k_per_entity)
            .field("min_similarity", &self.min_similarity)
            .field("sort_by", &self.sort_by)
            .finish()
    }
}

impl Clone for DuplicateDetector {
    fn clone(&self) -> Self {
        DuplicateDetector {
            logger: self.logger.clone(),
            config: self.config.clone(),
            similarity_calculator: self.similarity_calculator.clone(),
            similarity_threshold: self.similarity_threshold,
            confidence_threshold: self.confidence_threshold,
            use_clustering: self.use_clustering,
            max_results: self.max_results,
            top_k_per_entity: self.top_k_per_entity,
            min_similarity: self.min_similarity,
            sort_by: self.sort_by.clone(),
            progress_tracker: self.progress_tracker.clone(),
        }
    }
}

impl Default for DuplicateDetector {
    fn default() -> Self {
        DuplicateDetector::new(&Map::new()).expect("default arguments are valid")
    }
}

const SIMILARITY_CALCULATOR: &str = "semantica.deduplication.similarity_calculator.SimilarityCalculator";

fn value_error(message: String) -> SemanticaError {
    SemanticaError::Value(message)
}

impl DuplicateDetector {
    /// `DuplicateDetector(similarity_threshold=0.7, confidence_threshold=0.6,
    /// use_clustering=True, config=None, max_results=None, top_k_per_entity=None,
    /// min_similarity=None, sort_by="confidence", **kwargs)`, with the keyword arguments as a
    /// map (`similarity` in the config configures the [`SimilarityCalculator`]).
    ///
    /// Raises `ValueError` like the original for a negative or non-integer `max_results` /
    /// `top_k_per_entity`, a `min_similarity` outside `[0, 1]` and an unknown `sort_by`.
    pub fn new(kwargs: &Map<String, Value>) -> Result<DuplicateDetector, SemanticaError> {
        let logger = get_logger("duplicate_detector");
        let mut kw = Kwargs::new(kwargs);
        let similarity_threshold = kw.take("similarity_threshold");
        let confidence_threshold = kw.take("confidence_threshold");
        let use_clustering = kw.take("use_clustering");
        let config = kw.take("config");
        let max_results = kw.take("max_results").filter(|v| !v.is_null());
        let top_k_per_entity = kw.take("top_k_per_entity").filter(|v| !v.is_null());
        let min_similarity = kw.take("min_similarity").filter(|v| !v.is_null());
        let sort_by = kw.take("sort_by").unwrap_or(json!("confidence"));
        let config = merge_config(config, kw.rest())?;

        let similarity_config = config.get("similarity").cloned().unwrap_or(json!({}));
        let similarity_calculator =
            SimilarityCalculator::new(&mapping(SIMILARITY_CALCULATOR, &similarity_config)?)?;

        // Result limiting / ranking options — validated at construction time.
        if let Some(v) = &max_results
            && (!is_int(v) || compare(v, &json!(0), Cmp::Lt)?)
        {
            return Err(value_error(format!(
                "max_results must be None or a non-negative int, got {}",
                py_repr(v)
            )));
        }
        if let Some(v) = &top_k_per_entity
            && (!is_int(v) || compare(v, &json!(0), Cmp::Lt)?)
        {
            return Err(value_error(format!(
                "top_k_per_entity must be None or a non-negative int, got {}",
                py_repr(v)
            )));
        }
        if let Some(v) = &min_similarity
            && !(compare(&json!(0.0), v, Cmp::Le)? && compare(v, &json!(1.0), Cmp::Le)?)
        {
            return Err(value_error(format!(
                "min_similarity must be None or a float in [0.0, 1.0], got {}",
                py_repr(v)
            )));
        }
        if !(eq(&sort_by, &json!("confidence")) || eq(&sort_by, &json!("similarity_score"))) {
            return Err(value_error(format!(
                "sort_by must be 'confidence' or 'similarity_score', got {}",
                py_repr(&sort_by)
            )));
        }

        let detector = DuplicateDetector {
            similarity_threshold: float_param("similarity_threshold", similarity_threshold, 0.7)?,
            confidence_threshold: float_param("confidence_threshold", confidence_threshold, 0.6)?,
            use_clustering: bool_param(use_clustering, true),
            max_results: max_results.as_ref().and_then(number).map(|n| n as i64),
            top_k_per_entity: top_k_per_entity.as_ref().and_then(number).map(|n| n as i64),
            min_similarity: min_similarity.as_ref().and_then(number),
            sort_by: sort_by.as_str().unwrap_or("confidence").to_string(),
            config,
            similarity_calculator,
            progress_tracker: enabled_tracker(),
            logger,
        };
        detector.logger.debug(format!(
            "Duplicate detector initialized: similarity_threshold={}, confidence_threshold={}, \
             max_results={}, top_k_per_entity={}, min_similarity={}, sort_by={}",
            float_repr(detector.similarity_threshold),
            float_repr(detector.confidence_threshold),
            opt_repr(&max_results),
            opt_repr(&top_k_per_entity),
            opt_repr(&min_similarity),
            py_repr(&sort_by)
        ));
        Ok(detector)
    }

    /// The logger (`semantica.duplicate_detector`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    fn fail<T>(&self, tracking_id: &str, result: Result<T, SemanticaError>) -> Result<T, SemanticaError> {
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `detect_duplicates(entities, threshold=None, **options)`: candidates above the
    /// similarity threshold and the confidence threshold, ranked by `sort_by` (highest first)
    /// and capped by `top_k_per_entity` and `max_results`. `options` are accepted and ignored,
    /// as in the original.
    pub fn detect_duplicates<E: AsEntity>(
        &self,
        entities: &[E],
        threshold: Option<f64>,
        options: &Map<String, Value>,
    ) -> Result<Vec<DuplicateCandidate>, SemanticaError> {
        self.detect_refs(&entity_refs(entities), threshold, options)
    }

    fn detect_refs(
        &self,
        entities: &[EntityRef],
        threshold: Option<f64>,
        _options: &Map<String, Value>,
    ) -> Result<Vec<DuplicateCandidate>, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("deduplication"),
            Some("DuplicateDetector"),
            &format!("Detecting duplicates in {} entities", entities.len()),
            None,
        );
        let result = self.detect_body(entities, threshold, &tracking_id);
        self.fail(&tracking_id, result)
    }

    fn detect_body(
        &self,
        entities: &[EntityRef],
        threshold: Option<f64>,
        tracking_id: &str,
    ) -> Result<Vec<DuplicateCandidate>, SemanticaError> {
        let tracker = &self.progress_tracker;
        let detection_threshold = threshold.unwrap_or(self.similarity_threshold);
        self.logger.info(format!(
            "Detecting duplicates in {} entities (threshold: {})",
            entities.len(),
            float_repr(detection_threshold)
        ));
        tracker.update_tracking(tracking_id, None, "Calculating similarities...");
        let similarities = self.similarity_calculator.batch_refs(
            entities,
            Some(detection_threshold),
            &Map::new(),
        )?;
        self.logger.debug(format!(
            "Found {} similar pairs above threshold",
            similarities.len()
        ));

        tracker.update_tracking(tracking_id, None, "Creating duplicate candidates...");
        let mut candidates = Vec::new();
        let total = similarities.len();
        let update_interval = if total <= 10 {
            1
        } else {
            (total / 100).clamp(1, 10)
        };
        tracker.update_progress(
            tracking_id,
            0,
            total as i64,
            &format!("Creating duplicate candidates... 0/{total} (remaining: {total})"),
        );
        for (i, (entity1, entity2, score)) in similarities.iter().enumerate() {
            let candidate = self.create_duplicate_candidate(entity1, entity2, *score)?;
            if candidate.confidence >= self.confidence_threshold {
                candidates.push(candidate);
            }
            let done = i + 1;
            if done % update_interval == 0 || done == total || i == 0 || total <= 10 {
                tracker.update_progress(
                    tracking_id,
                    done as i64,
                    total as i64,
                    &format!(
                        "Creating duplicate candidates... {done}/{total} (remaining: {})",
                        total - done
                    ),
                );
            }
        }

        let candidates = self.apply_result_limits(candidates);
        self.logger.info(format!(
            "Detected {} duplicate candidate(s) (confidence >= {})",
            candidates.len(),
            float_repr(self.confidence_threshold)
        ));
        tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Detected {} duplicate candidates", candidates.len()),
            None,
        );
        Ok(candidates)
    }

    /// `detect_duplicate_groups(entities, threshold=None, **options)`: duplicate candidates
    /// joined into groups (union-find), each with its confidence and representative.
    pub fn detect_duplicate_groups<E: AsEntity>(
        &self,
        entities: &[E],
        threshold: Option<f64>,
        options: &Map<String, Value>,
    ) -> Result<Vec<DuplicateGroup>, SemanticaError> {
        let entities = entity_refs(entities);
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("deduplication"),
            Some("DuplicateDetector"),
            &format!("Detecting duplicate groups from {} entities", entities.len()),
            None,
        );
        let result = self.groups_body(&entities, threshold, options, &tracking_id);
        self.fail(&tracking_id, result)
    }

    fn groups_body(
        &self,
        entities: &[EntityRef],
        threshold: Option<f64>,
        options: &Map<String, Value>,
        tracking_id: &str,
    ) -> Result<Vec<DuplicateGroup>, SemanticaError> {
        let tracker = &self.progress_tracker;
        self.logger.info(format!(
            "Detecting duplicate groups from {} entities",
            entities.len()
        ));
        tracker.update_tracking(
            tracking_id,
            Some("running"),
            &format!(
                "Starting duplicate detection for {} entities...",
                entities.len()
            ),
        );
        let candidates = self.detect_refs(entities, threshold, options)?;

        tracker.update_tracking(tracking_id, None, "Building duplicate groups...");
        let mut groups = self.build_duplicate_groups(&candidates);
        self.logger
            .debug(format!("Built {} duplicate group(s)", groups.len()));

        tracker.update_tracking(tracking_id, None, "Calculating group metrics...");
        let total = groups.len();
        let update_interval = if total <= 10 {
            1
        } else {
            (total / 100).clamp(1, 5)
        };
        tracker.update_progress(
            tracking_id,
            0,
            total as i64,
            &format!("Calculating group metrics... 0/{total} (remaining: {total})"),
        );
        for (i, group) in groups.iter_mut().enumerate() {
            group.confidence = calculate_group_confidence(group);
            group.representative = select_representative(group)?;
            let done = i + 1;
            if done % update_interval == 0 || done == total || i == 0 || total <= 10 {
                tracker.update_progress(
                    tracking_id,
                    done as i64,
                    total as i64,
                    &format!(
                        "Calculating group metrics... {done}/{total} (remaining: {})",
                        total - done
                    ),
                );
            }
        }

        let entity_count: usize = groups.iter().map(|g| g.entities.len()).sum();
        self.logger.info(format!(
            "Detected {} duplicate group(s) with {entity_count} total entities",
            groups.len()
        ));
        tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Detected {} duplicate groups", groups.len()),
            None,
        );
        Ok(groups)
    }

    /// `detect_relationship_duplicates(relationships, **options)`: duplicate relationship
    /// pairs.
    ///
    /// Options: `threshold` (default 0.9), `relationship_dedup_mode` (`legacy`: identical
    /// triples or predicate string similarity ≥ threshold; `semantic_v2`: canonical
    /// `(subject, predicate, object)` signatures, else the semantic score
    /// `0.6 * predicate + 0.4 * object` with the same subject), `predicate_synonym_map`,
    /// `literal_normalization_enabled`. A semantic match writes `semantic_match_score` into
    /// both relationships' `metadata`; the returned pairs show the relationships as they are
    /// at the end of the call (Python returns the mutated objects).
    pub fn detect_relationship_duplicates(
        &self,
        relationships: &[Value],
        options: &Map<String, Value>,
    ) -> Result<Vec<(Value, Value)>, SemanticaError> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("deduplication"),
            Some("DuplicateDetector"),
            &format!(
                "Detecting duplicate relationships in {} relations",
                relationships.len()
            ),
            None,
        );
        let result = self.relationship_body(relationships, options, &tracking_id);
        self.fail(&tracking_id, result)
    }

    fn relationship_body(
        &self,
        relationships: &[Value],
        options: &Map<String, Value>,
        tracking_id: &str,
    ) -> Result<Vec<(Value, Value)>, SemanticaError> {
        let mut rels: Vec<Value> = relationships.to_vec();
        let mut duplicates: Vec<(usize, usize)> = Vec::new();
        let threshold = options.get("threshold").cloned().unwrap_or(json!(0.9));
        let legacy = json!("legacy");
        let mode = options.get("relationship_dedup_mode").unwrap_or(&legacy);
        let semantic_v2 = mode.as_str() == Some("semantic_v2");

        let mut canon_sigs: Vec<HKey> = Vec::new();
        if semantic_v2 {
            let empty = json!({});
            let synonym_map = options.get("predicate_synonym_map").unwrap_or(&empty);
            let norm_enabled = truthy(
                options
                    .get("literal_normalization_enabled")
                    .unwrap_or(&Value::Bool(false)),
            );
            for rel in &rels {
                let subj = py_str(&get(rel, "subject")?.cloned().unwrap_or(json!("")));
                let pred = lower(&py_str(&get(rel, "predicate")?.cloned().unwrap_or(json!(""))));
                let mut obj = py_str(&get(rel, "object")?.cloned().unwrap_or(json!("")));
                let canon_pred = get(synonym_map, &pred)?
                    .cloned()
                    .unwrap_or(Value::String(pred.clone()));
                if norm_enabled {
                    obj = normalize_spaces_lower(&obj);
                }
                canon_sigs.push(HKey::Tuple(vec![
                    HKey::Str(subj),
                    HKey::scalar(&canon_pred)?,
                    HKey::Str(obj),
                ]));
            }
        }

        let total_rels = rels.len();
        let total_pairs = total_rels * total_rels.saturating_sub(1) / 2;
        let mut processed = 0usize;
        let update_interval = if total_pairs <= 10 {
            1
        } else {
            (total_pairs / 100).clamp(1, 100)
        };
        for i in 0..total_rels {
            for j in i + 1..total_rels {
                let is_duplicate = if semantic_v2 && canon_sigs[i] == canon_sigs[j] {
                    true
                } else {
                    self.relationships_are_duplicates(&mut rels, i, j, &threshold, mode, options)?
                };
                if is_duplicate {
                    duplicates.push((i, j));
                }
                processed += 1;
                if processed % update_interval == 0 || processed == total_pairs {
                    self.progress_tracker.update_progress(
                        tracking_id,
                        processed as i64,
                        total_pairs as i64,
                        &format!("Checking relationships... {processed}/{total_pairs}"),
                    );
                }
            }
        }

        self.progress_tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Detected {} duplicate relationships", duplicates.len()),
            None,
        );
        Ok(duplicates
            .into_iter()
            .map(|(i, j)| (rels[i].clone(), rels[j].clone()))
            .collect())
    }

    /// `_relationships_are_duplicates(rel1, rel2, threshold, mode, options)` on `rels[i]`,
    /// `rels[j]` (which a semantic match annotates in place).
    fn relationships_are_duplicates(
        &self,
        rels: &mut [Value],
        i: usize,
        j: usize,
        threshold: &Value,
        mode: &Value,
        options: &Map<String, Value>,
    ) -> Result<bool, SemanticaError> {
        fn rel_val<'a>(rel: &'a Value, key: &str) -> Option<&'a Value> {
            match rel {
                Value::Object(map) => map.get(key),
                _ => None,
            }
        }
        fn str_or_empty(value: Option<&Value>) -> String {
            match value {
                Some(v) if truthy(v) => py_str(v),
                _ => String::new(),
            }
        }
        let null = Value::Null;
        let subj1 = rel_val(&rels[i], "subject").unwrap_or(&null).clone();
        let subj2 = rel_val(&rels[j], "subject").unwrap_or(&null).clone();
        let pred1 = str_or_empty(rel_val(&rels[i], "predicate"));
        let pred2 = str_or_empty(rel_val(&rels[j], "predicate"));
        let mut obj1 = str_or_empty(rel_val(&rels[i], "object"));
        let mut obj2 = str_or_empty(rel_val(&rels[j], "object"));

        if mode.as_str() == Some("legacy") {
            if eq(&subj1, &subj2) && pred1 == pred2 && obj1 == obj2 {
                return Ok(true);
            }
            let similarity = string_similarity(&pred1, &pred2, "jaro_winkler");
            return compare_f64(similarity, threshold, Cmp::Ge);
        }

        if !eq(&subj1, &subj2) {
            return Ok(false);
        }
        let empty = json!({});
        let synonyms = options.get("predicate_synonym_map").unwrap_or(&empty);
        let c_pred1 = get(synonyms, &lower(&pred1))?
            .cloned()
            .unwrap_or_else(|| Value::String(lower(&pred1)));
        let c_pred2 = get(synonyms, &lower(&pred2))?
            .cloned()
            .unwrap_or_else(|| Value::String(lower(&pred2)));
        let pred_sim = if eq(&c_pred1, &c_pred2) {
            1.0
        } else {
            string_similarity_values(&c_pred1, &c_pred2, "jaro_winkler")?
        };
        if truthy(
            options
                .get("literal_normalization_enabled")
                .unwrap_or(&Value::Bool(false)),
        ) {
            obj1 = normalize_spaces_lower(&obj1);
            obj2 = normalize_spaces_lower(&obj2);
        }
        let obj_sim = if obj1 == obj2 {
            1.0
        } else {
            string_similarity(&obj1, &obj2, "jaro_winkler")
        };

        // Weighted composition: predicate is 60% of the match, object literal is 40%.
        let semantic_score = (pred_sim * 0.6) + (obj_sim * 0.4);
        if compare_f64(semantic_score, threshold, Cmp::Ge)?
            && rels[i].is_object()
            && rels[j].is_object()
        {
            for index in [i, j] {
                let Value::Object(rel) = &mut rels[index] else {
                    continue;
                };
                let metadata = rel.entry("metadata").or_insert_with(|| json!({}));
                set_item(
                    metadata,
                    "semantic_match_score",
                    crate::pyops::float_value(semantic_score),
                )?;
            }
        }
        compare_f64(semantic_score, threshold, Cmp::Ge)
    }

    /// `incremental_detect(new_entities, existing_entities, threshold=None, **options)`:
    /// candidates between each new entity and each existing entity (never new × new), with
    /// the same confidence filter and result limits as [`Self::detect_duplicates`].
    pub fn incremental_detect<N: AsEntity, X: AsEntity>(
        &self,
        new_entities: &[N],
        existing_entities: &[X],
        threshold: Option<f64>,
        options: &Map<String, Value>,
    ) -> Result<Vec<DuplicateCandidate>, SemanticaError> {
        let _ = options;
        let new_entities = entity_refs(new_entities);
        let existing_entities = entity_refs(existing_entities);
        let detection_threshold = threshold.unwrap_or(self.similarity_threshold);
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("deduplication"),
            Some("DuplicateDetector"),
            &format!(
                "Incremental detection: {} new vs {} existing",
                new_entities.len(),
                existing_entities.len()
            ),
            None,
        );
        let result = self.incremental_body(
            &new_entities,
            &existing_entities,
            detection_threshold,
            &tracking_id,
        );
        self.fail(&tracking_id, result)
    }

    fn incremental_body(
        &self,
        new_entities: &[EntityRef],
        existing_entities: &[EntityRef],
        detection_threshold: f64,
        tracking_id: &str,
    ) -> Result<Vec<DuplicateCandidate>, SemanticaError> {
        let tracker = &self.progress_tracker;
        self.logger.info(format!(
            "Incremental detection: {} new entities vs {} existing entities",
            new_entities.len(),
            existing_entities.len()
        ));
        let mut candidates = Vec::new();
        let total = new_entities.len() * existing_entities.len();
        let mut processed = 0usize;
        let update_interval = if total <= 10 {
            1
        } else {
            (total / 100).clamp(1, 10)
        };
        tracker.update_progress(
            tracking_id,
            0,
            total as i64,
            &format!("Starting incremental detection... 0/{total} (remaining: {total})"),
        );

        for new_entity in new_entities {
            for existing_entity in existing_entities {
                let similarity = self.similarity_calculator.calculate_similarity(
                    new_entity,
                    existing_entity,
                    false,
                    &Map::new(),
                )?;
                if similarity.score >= detection_threshold {
                    let candidate =
                        self.create_duplicate_candidate(new_entity, existing_entity, similarity.score)?;
                    if candidate.confidence >= self.confidence_threshold {
                        candidates.push(candidate);
                    }
                }
                processed += 1;
                if processed % update_interval == 0
                    || processed == total
                    || processed == 1
                    || total <= 10
                {
                    tracker.update_progress(
                        tracking_id,
                        processed as i64,
                        total as i64,
                        &format!(
                            "Comparing entities... {processed}/{total} (remaining: {})",
                            total - processed
                        ),
                    );
                }
            }
        }

        let candidates = self.apply_result_limits(candidates);
        self.logger.info(format!(
            "Incremental detection found {} duplicate candidate(s)",
            candidates.len()
        ));
        tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Found {} duplicate candidates", candidates.len()),
            None,
        );
        Ok(candidates)
    }

    /// `_apply_result_limits(candidates)`: `min_similarity` filter, stable descending sort by
    /// `sort_by`, `top_k_per_entity` (keep while either entity is under quota), then
    /// `max_results`.
    fn apply_result_limits(&self, mut candidates: Vec<DuplicateCandidate>) -> Vec<DuplicateCandidate> {
        if let Some(min_similarity) = self.min_similarity {
            candidates.retain(|c| c.similarity_score >= min_similarity);
        }
        let by_similarity = self.sort_by == "similarity_score";
        let key = |c: &DuplicateCandidate| {
            if by_similarity {
                c.similarity_score
            } else {
                c.confidence
            }
        };
        candidates.sort_by(|a, b| key(b).partial_cmp(&key(a)).unwrap_or(Ordering::Equal));

        if let Some(k) = self.top_k_per_entity {
            let mut entity_counts: HashMap<String, i64> = HashMap::new();
            let mut kept = Vec::new();
            for c in candidates {
                let nid1 = normalize_entity_id(&c.entity1);
                let nid2 = normalize_entity_id(&c.entity2);
                let count1 = entity_counts.get(&nid1).copied().unwrap_or(0);
                let count2 = entity_counts.get(&nid2).copied().unwrap_or(0);
                if count1 < k || count2 < k {
                    entity_counts.insert(nid1, count1 + 1);
                    entity_counts.insert(nid2, count2 + 1);
                    kept.push(c);
                }
            }
            candidates = kept;
        }

        if let Some(max_results) = self.max_results {
            candidates.truncate(max_results.max(0) as usize);
        }
        candidates
    }

    /// `_create_duplicate_candidate(entity1, entity2, similarity_score)`: confidence =
    /// similarity, +0.1 for an exact (case-insensitive) name match, +0.05 per matching
    /// property value, +0.05 for the same type, capped at 1.0.
    fn create_duplicate_candidate(
        &self,
        entity1: &EntityRef,
        entity2: &EntityRef,
        similarity_score: f64,
    ) -> Result<DuplicateCandidate, SemanticaError> {
        let mut reasons = Vec::new();
        let mut confidence = similarity_score;

        let name1 = lower_strip(&py_str(&entity_value(entity1, "name", json!(""))));
        let name2 = lower_strip(&py_str(&entity_value(entity2, "name", json!(""))));
        if name1 == name2 && !name1.is_empty() {
            reasons.push("exact_name_match".to_string());
            confidence += 0.1;
        }

        let props1 = entity_value(entity1, "properties", json!({}));
        let props2 = entity_value(entity2, "properties", json!({}));
        let keys1 = keys(&props1)?;
        let keys2: HashSet<String> = keys(&props2)?.into_iter().collect();
        let common_props: Vec<&String> = keys1.iter().filter(|k| keys2.contains(*k)).collect();
        if !common_props.is_empty() {
            let prop_matches = common_props
                .iter()
                .filter(|p| eq(&props1[p.as_str()], &props2[p.as_str()]))
                .count();
            if prop_matches > 0 {
                reasons.push(format!("{prop_matches}_property_matches"));
                confidence += 0.05 * prop_matches as f64;
            }
        }

        let type1 = entity_value(entity1, "type", Value::Null);
        let type2 = entity_value(entity2, "type", Value::Null);
        if truthy(&type1) && truthy(&type2) && eq(&type1, &type2) {
            reasons.push("same_type".to_string());
            confidence += 0.05;
        }

        let confidence = if confidence < 1.0 { confidence } else { 1.0 };
        let mut metadata = Map::new();
        metadata.insert("name_match".to_string(), Value::Bool(name1 == name2));
        metadata.insert(
            "common_properties".to_string(),
            Value::from(common_props.len()),
        );
        metadata.insert("type_match".to_string(), Value::Bool(eq(&type1, &type2)));
        Ok(DuplicateCandidate {
            entity1: entity1.clone(),
            entity2: entity2.clone(),
            similarity_score,
            confidence,
            reasons,
            metadata,
        })
    }

    /// `_build_duplicate_groups(candidates)`: union-find over the candidates' normalised ids
    /// (a pair joining two groups merges them; the absorbed group's ids move to the survivor).
    pub fn build_duplicate_groups(&self, candidates: &[DuplicateCandidate]) -> Vec<DuplicateGroup> {
        // Groups live in an arena so that ids still pointing at an absorbed group behave as
        // Python's references to the removed group object do.
        let mut arena: Vec<DuplicateGroup> = Vec::new();
        let mut groups: Vec<usize> = Vec::new();
        let mut entity_to_group: HashMap<String, usize> = HashMap::new();

        for candidate in candidates {
            let entity1_id = normalize_entity_id(&candidate.entity1);
            let entity2_id = normalize_entity_id(&candidate.entity2);
            let group1 = entity_to_group.get(&entity1_id).copied();
            let group2 = entity_to_group.get(&entity2_id).copied();
            let key = (entity1_id.clone(), entity2_id.clone());

            match (group1, group2) {
                (None, None) => {
                    let mut group = DuplicateGroup::new(vec![
                        candidate.entity1.clone(),
                        candidate.entity2.clone(),
                    ]);
                    group
                        .similarity_scores
                        .insert(key, candidate.similarity_score);
                    arena.push(group);
                    let index = arena.len() - 1;
                    groups.push(index);
                    entity_to_group.insert(entity1_id, index);
                    entity_to_group.insert(entity2_id, index);
                }
                (Some(g1), None) => {
                    let group = &mut arena[g1];
                    if !group
                        .entities
                        .iter()
                        .any(|e| same_or_equal(e, &candidate.entity2))
                    {
                        group.entities.push(candidate.entity2.clone());
                    }
                    group
                        .similarity_scores
                        .insert(key, candidate.similarity_score);
                    entity_to_group.insert(entity2_id, g1);
                }
                (None, Some(g2)) => {
                    let group = &mut arena[g2];
                    if !group
                        .entities
                        .iter()
                        .any(|e| same_or_equal(e, &candidate.entity1))
                    {
                        group.entities.push(candidate.entity1.clone());
                    }
                    group
                        .similarity_scores
                        .insert(key, candidate.similarity_score);
                    entity_to_group.insert(entity1_id, g2);
                }
                (Some(g1), Some(g2)) if g1 != g2 => {
                    let absorbed = arena[g2].clone();
                    let added: Vec<EntityRef> = absorbed
                        .entities
                        .iter()
                        .filter(|e| !arena[g1].entities.iter().any(|x| same_or_equal(x, e)))
                        .cloned()
                        .collect();
                    let survivor = &mut arena[g1];
                    survivor.entities.extend(added);
                    for (k, v) in &absorbed.similarity_scores {
                        survivor.similarity_scores.insert(k.clone(), *v);
                    }
                    survivor
                        .similarity_scores
                        .insert(key, candidate.similarity_score);
                    for entity in &absorbed.entities {
                        entity_to_group.insert(normalize_entity_id(entity), g1);
                    }
                    if let Some(pos) = groups.iter().position(|&g| g == g2) {
                        groups.remove(pos);
                    }
                }
                _ => {}
            }
        }
        groups.into_iter().map(|g| arena[g].clone()).collect()
    }
}

fn opt_repr(value: &Option<Value>) -> String {
    value.as_ref().map(py_repr).unwrap_or_else(|| "None".to_string())
}

/// `_calculate_group_confidence(group)`: mean pair score × `(0.8 + 0.2 × min(1, size / 5))`.
fn calculate_group_confidence(group: &DuplicateGroup) -> f64 {
    if group.similarity_scores.is_empty() {
        return 0.0;
    }
    let scores: Vec<f64> = group.similarity_scores.values().copied().collect();
    let avg_score = scores.iter().fold(0.0, |acc, s| acc + s) / scores.len() as f64;
    let ratio = group.entities.len() as f64 / 5.0;
    let size_factor = if ratio < 1.0 { ratio } else { 1.0 };
    avg_score * (0.8 + 0.2 * size_factor)
}

/// `_select_representative(group)`: the first entity with the most properties plus
/// relationships.
fn select_representative(group: &DuplicateGroup) -> Result<Option<EntityRef>, SemanticaError> {
    let mut best: Option<(usize, &EntityRef)> = None;
    for entity in &group.entities {
        let size = py_len(&entity_value(entity, "properties", json!({})))?
            + py_len(&entity_value(entity, "relationships", json!([])))?;
        match best {
            Some((best_size, _)) if size <= best_size => {}
            _ => best = Some((size, entity)),
        }
    }
    Ok(best.map(|(_, e)| e.clone()))
}
