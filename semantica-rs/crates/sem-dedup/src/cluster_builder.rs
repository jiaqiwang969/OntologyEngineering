//! Cluster Builder Module (`semantica/deduplication/cluster_builder.py`).
//!
//! Clusters of similar entities for batch deduplication: graph-based clustering (union-find
//! over the similar pairs of a batch comparison) or hierarchical (agglomerative, average
//! linkage) clustering, size filtering, cluster quality metrics and incremental updates.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use sem_utils::SemanticaError;
use sem_utils::get_logger;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::pycompat::fmt::float_repr;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::entity::{AsEntity, EntityRef, entity_refs, py_id};
use crate::kwargs::{Kwargs, bool_param, float_param, merge_config};
use crate::pyops::{
    Cmp, HKey, compare_f64, float_value, get, mapping, number, py_repr, truthy, tuple_repr,
    type_error,
};
use crate::similarity_calculator::{
    SimilarPair, SimilarityBackend, SimilarityCalculator, enabled_tracker,
};

/// Entity cluster representation (`Cluster` dataclass).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cluster {
    /// `cluster_<n>`.
    pub cluster_id: String,
    /// The clustered entities (the caller's references).
    #[serde(with = "crate::entity::serde_refs")]
    pub entities: Vec<EntityRef>,
    /// Representative entity (never computed by the builder, as in the original).
    #[serde(default)]
    pub centroid: Option<Value>,
    /// Quality score (set by the quality metrics).
    #[serde(default)]
    pub quality_score: f64,
    /// `similarity_scores` (graph-based: a dict keyed by the `repr()` of the
    /// `(entity1_id, entity2_id)` tuple) or `merge_similarity` (hierarchical).
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

impl Cluster {
    /// `Cluster(cluster_id, entities)` with the dataclass defaults.
    pub fn new(cluster_id: impl Into<String>, entities: Vec<EntityRef>) -> Cluster {
        Cluster {
            cluster_id: cluster_id.into(),
            entities,
            centroid: None,
            quality_score: 0.0,
            metadata: Map::new(),
        }
    }
}

/// Cluster building result (`ClusterResult` dataclass).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClusterResult {
    /// Clusters within the size limits.
    pub clusters: Vec<Cluster>,
    /// Input entities in none of those clusters.
    #[serde(default, with = "crate::entity::serde_refs")]
    pub unclustered: Vec<EntityRef>,
    /// `average_size`, `average_quality`, `total_clusters`, `high_quality_clusters`.
    #[serde(default)]
    pub quality_metrics: Map<String, Value>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

/// Cluster building engine for entity clustering (`ClusterBuilder`).
pub struct ClusterBuilder {
    logger: Logger,
    /// `config` merged with the extra keyword arguments.
    pub config: Map<String, Value>,
    /// The similarity engine (a [`SimilarityCalculator`] built from `config["similarity"]`;
    /// replaceable, as the original tests patch it).
    pub similarity_calculator: Box<dyn SimilarityBackend>,
    /// Minimum similarity for entities to share a cluster.
    pub similarity_threshold: f64,
    /// Minimum cluster size kept.
    pub min_cluster_size: f64,
    /// Maximum cluster size kept.
    pub max_cluster_size: f64,
    /// Hierarchical instead of graph-based clustering.
    pub use_hierarchical: bool,
    progress_tracker: Arc<ProgressTracker>,
}

impl fmt::Debug for ClusterBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClusterBuilder")
            .field("config", &self.config)
            .field("similarity_threshold", &self.similarity_threshold)
            .field("min_cluster_size", &self.min_cluster_size)
            .field("max_cluster_size", &self.max_cluster_size)
            .field("use_hierarchical", &self.use_hierarchical)
            .finish()
    }
}

impl Default for ClusterBuilder {
    fn default() -> Self {
        ClusterBuilder::new(&Map::new()).expect("default arguments are valid")
    }
}

const SIMILARITY_CALCULATOR: &str = "semantica.deduplication.similarity_calculator.SimilarityCalculator";

/// The key Python's dicts and sets use for an entity: `entity.get("id") or id(entity)`.
struct EntityKey {
    /// The raw id (for `repr()` in score keys).
    raw: Value,
    hash: Option<HKey>,
}

impl EntityKey {
    /// Read the raw key (`AttributeError` for a non-dict entity).
    fn of(entity: &EntityRef) -> Result<EntityKey, SemanticaError> {
        let id = get(entity, "id")?;
        Ok(match id {
            Some(v) if truthy(v) => EntityKey {
                raw: v.clone(),
                hash: None,
            },
            _ => {
                let address = py_id(entity);
                EntityKey {
                    raw: Value::from(address),
                    hash: Some(HKey::Int(i128::from(address))),
                }
            }
        })
    }

    /// The hashable form (`TypeError` for an unhashable id).
    fn hashed(&self) -> Result<HKey, SemanticaError> {
        match &self.hash {
            Some(h) => Ok(h.clone()),
            None => HKey::scalar(&self.raw),
        }
    }
}

/// `(entity1_id, entity2_id)` as Python prints the tuple.
fn pair_repr(a: &EntityKey, b: &EntityKey) -> String {
    tuple_repr(&[py_repr(&a.raw), py_repr(&b.raw)])
}

impl ClusterBuilder {
    /// `ClusterBuilder(similarity_threshold=0.7, min_cluster_size=2, max_cluster_size=100,
    /// use_hierarchical=False, config=None, **kwargs)`, with the keyword arguments as a map
    /// (`similarity` in the config configures the [`SimilarityCalculator`]).
    pub fn new(kwargs: &Map<String, Value>) -> Result<ClusterBuilder, SemanticaError> {
        let logger = get_logger("cluster_builder");
        let mut kw = Kwargs::new(kwargs);
        let similarity_threshold = kw.take("similarity_threshold");
        let min_cluster_size = kw.take("min_cluster_size");
        let max_cluster_size = kw.take("max_cluster_size");
        let use_hierarchical = kw.take("use_hierarchical");
        let config = kw.take("config");
        let config = merge_config(config, kw.rest())?;

        let similarity_config = config.get("similarity").cloned().unwrap_or(json!({}));
        let similarity_calculator =
            SimilarityCalculator::new(&mapping(SIMILARITY_CALCULATOR, &similarity_config)?)?;
        let builder = ClusterBuilder {
            similarity_threshold: float_param("similarity_threshold", similarity_threshold, 0.7)?,
            min_cluster_size: float_param("min_cluster_size", min_cluster_size, 2.0)?,
            max_cluster_size: float_param("max_cluster_size", max_cluster_size, 100.0)?,
            use_hierarchical: bool_param(use_hierarchical, false),
            config,
            similarity_calculator: Box::new(similarity_calculator),
            progress_tracker: enabled_tracker(),
            logger,
        };
        builder.logger.debug(format!(
            "Cluster builder initialized: threshold={}, size_range=[{}, {}], hierarchical={}",
            float_repr(builder.similarity_threshold),
            float_repr(builder.min_cluster_size),
            float_repr(builder.max_cluster_size),
            if builder.use_hierarchical {
                "True"
            } else {
                "False"
            }
        ));
        Ok(builder)
    }

    /// The logger (`semantica.cluster_builder`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `build_clusters(entities, **options)`: cluster, keep clusters within the size limits,
    /// list the unclustered entities and compute quality metrics. Option `threshold`
    /// overrides `similarity_threshold`.
    pub fn build_clusters<E: AsEntity>(
        &self,
        entities: &[E],
        options: &Map<String, Value>,
    ) -> Result<ClusterResult, SemanticaError> {
        let entities = entity_refs(entities);
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            Some("deduplication"),
            Some("ClusterBuilder"),
            &format!("Building clusters from {} entities", entities.len()),
            None,
        );
        let result = self.build_body(&entities, options, &tracking_id);
        if let Err(e) = &result {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    fn build_body(
        &self,
        entities: &[EntityRef],
        options: &Map<String, Value>,
        tracking_id: &str,
    ) -> Result<ClusterResult, SemanticaError> {
        let tracker = &self.progress_tracker;
        let threshold = options
            .get("threshold")
            .cloned()
            .unwrap_or_else(|| float_value(self.similarity_threshold));
        let total_steps = 4;

        tracker.update_progress(
            tracking_id,
            1,
            total_steps,
            &format!(
                "Clustering {} entities... (1/{total_steps}, remaining: 3 steps)",
                entities.len()
            ),
        );
        let clusters = if self.use_hierarchical {
            self.hierarchical(entities, &threshold, Some(tracking_id))?
        } else {
            self.graph_based(entities, &threshold, Some(tracking_id))?
        };

        tracker.update_progress(
            tracking_id,
            2,
            total_steps,
            &format!(
                "Filtering clusters by size... (2/{total_steps}, {} clusters, remaining: 2 steps)",
                clusters.len()
            ),
        );
        let mut valid_clusters: Vec<Cluster> = clusters
            .into_iter()
            .filter(|c| {
                let size = c.entities.len() as f64;
                self.min_cluster_size <= size && size <= self.max_cluster_size
            })
            .collect();

        tracker.update_progress(
            tracking_id,
            3,
            total_steps,
            &format!("Finding unclustered entities... (3/{total_steps}, remaining: 1 steps)"),
        );
        let mut clustered_entity_ids: HashSet<HKey> = HashSet::new();
        for cluster in &valid_clusters {
            for entity in &cluster.entities {
                clustered_entity_ids.insert(EntityKey::of(entity)?.hashed()?);
            }
        }
        let mut unclustered = Vec::new();
        for entity in entities {
            if !clustered_entity_ids.contains(&EntityKey::of(entity)?.hashed()?) {
                unclustered.push(entity.clone());
            }
        }

        tracker.update_progress(
            tracking_id,
            4,
            total_steps,
            &format!(
                "Calculating cluster quality metrics... (4/{total_steps}, {} clusters, remaining: 0 steps)",
                valid_clusters.len()
            ),
        );
        let quality_metrics = self.calculate_cluster_quality(&mut valid_clusters)?;

        tracker.stop_tracking(
            tracking_id,
            Some("completed"),
            &format!("Built {} clusters", valid_clusters.len()),
            None,
        );
        Ok(ClusterResult {
            clusters: valid_clusters,
            unclustered,
            quality_metrics,
            metadata: Map::new(),
        })
    }

    /// `_graph_based_clustering(entities, threshold)`: union-find over the similar pairs of
    /// a batch comparison; clusters are `cluster_0`, `cluster_1`, ... in creation order.
    pub fn graph_based_clustering<E: AsEntity>(
        &self,
        entities: &[E],
        threshold: f64,
    ) -> Result<Vec<Cluster>, SemanticaError> {
        self.graph_based(&entity_refs(entities), &float_value(threshold), None)
    }

    fn graph_based(
        &self,
        entities: &[EntityRef],
        threshold: &Value,
        tracking_id: Option<&str>,
    ) -> Result<Vec<Cluster>, SemanticaError> {
        // `batch_calculate_similarity(entities, threshold=threshold)` applies
        // `threshold or self.similarity_threshold`.
        let batch_threshold = if truthy(threshold) {
            Some(number(threshold).ok_or_else(|| {
                type_error(format!(
                    "'>=' not supported between instances of 'float' and '{}'",
                    sem_utils::pycompat::fmt::type_name(threshold)
                ))
            })?)
        } else {
            None
        };
        let similarity_pairs: Vec<SimilarPair> = self
            .similarity_calculator
            .batch_calculate_similarity(entities, batch_threshold, &Map::new())?;
        let tracker = &self.progress_tracker;
        if let Some(id) = tracking_id {
            tracker.update_tracking(
                id,
                None,
                &format!(
                    "Building clusters from {} similarity pairs...",
                    similarity_pairs.len()
                ),
            );
        }

        let mut entity_to_cluster: HashMap<HKey, String> = HashMap::new();
        let mut clusters_dict: IndexMap<String, Cluster> = IndexMap::new();
        let mut cluster_id_counter = 0usize;

        let total_pairs = similarity_pairs.len();
        let update_interval = if total_pairs <= 10 {
            1
        } else {
            (total_pairs / 100).clamp(1, 10)
        };
        if let Some(id) = tracking_id
            && total_pairs > 0
        {
            tracker.update_progress(
                id,
                0,
                total_pairs as i64,
                &format!(
                    "Building clusters from similarity pairs... 0/{total_pairs} (remaining: {total_pairs})"
                ),
            );
        }

        for (processed_pairs, (entity1, entity2, score)) in similarity_pairs.into_iter().enumerate() {
            let key1 = EntityKey::of(&entity1)?;
            let key2 = EntityKey::of(&entity2)?;
            let hash1 = key1.hashed()?;
            let cluster1 = entity_to_cluster.get(&hash1).cloned();
            let hash2 = key2.hashed()?;
            let cluster2 = entity_to_cluster.get(&hash2).cloned();
            let missing = |id: &str| SemanticaError::Key(id.to_string());

            match (cluster1, cluster2) {
                (None, None) => {
                    let cluster_id = format!("cluster_{cluster_id_counter}");
                    cluster_id_counter += 1;
                    let mut scores = Map::new();
                    scores.insert(pair_repr(&key1, &key2), float_value(score));
                    let mut cluster = Cluster::new(cluster_id.clone(), vec![entity1, entity2]);
                    cluster
                        .metadata
                        .insert("similarity_scores".to_string(), Value::Object(scores));
                    clusters_dict.insert(cluster_id.clone(), cluster);
                    entity_to_cluster.insert(hash1, cluster_id.clone());
                    entity_to_cluster.insert(hash2, cluster_id);
                }
                (Some(c1), None) => {
                    clusters_dict
                        .get_mut(&c1)
                        .ok_or_else(|| missing(&c1))?
                        .entities
                        .push(entity2);
                    entity_to_cluster.insert(hash2, c1);
                }
                (None, Some(c2)) => {
                    clusters_dict
                        .get_mut(&c2)
                        .ok_or_else(|| missing(&c2))?
                        .entities
                        .push(entity1);
                    entity_to_cluster.insert(hash1, c2);
                }
                (Some(c1), Some(c2)) if c1 != c2 => {
                    let absorbed = clusters_dict.get(&c2).ok_or_else(|| missing(&c2))?.clone();
                    let survivor = clusters_dict.get_mut(&c1).ok_or_else(|| missing(&c1))?;
                    survivor.entities.extend(absorbed.entities.iter().cloned());
                    let absorbed_scores = match absorbed.metadata.get("similarity_scores") {
                        Some(Value::Object(scores)) => scores.clone(),
                        _ => Map::new(),
                    };
                    if let Some(Value::Object(scores)) =
                        survivor.metadata.get_mut("similarity_scores")
                    {
                        for (k, v) in absorbed_scores {
                            scores.insert(k, v);
                        }
                    }
                    match survivor.metadata.get_mut("similarity_scores") {
                        Some(Value::Object(scores)) => {
                            scores.insert(pair_repr(&key1, &key2), float_value(score));
                        }
                        Some(other) => {
                            crate::pyops::set_item(
                                other,
                                &pair_repr(&key1, &key2),
                                float_value(score),
                            )?;
                        }
                        None => return Err(SemanticaError::Key("similarity_scores".to_string())),
                    }
                    for entity in &absorbed.entities {
                        entity_to_cluster.insert(EntityKey::of(entity)?.hashed()?, c1.clone());
                    }
                    clusters_dict.shift_remove(&c2);
                }
                _ => {}
            }

            let done = processed_pairs + 1;
            if let Some(id) = tracking_id
                && (done % update_interval == 0 || done == total_pairs || done == 1 || total_pairs <= 10)
            {
                tracker.update_progress(
                    id,
                    done as i64,
                    total_pairs as i64,
                    &format!(
                        "Building clusters from similarity pairs... {done}/{total_pairs} (remaining: {})",
                        total_pairs - done
                    ),
                );
            }
        }
        Ok(clusters_dict.into_values().collect())
    }

    /// `_hierarchical_clustering(entities, threshold)`: start from singletons and repeatedly
    /// merge the pair of clusters with the highest average similarity (the last best pair on
    /// ties) while it reaches the threshold.
    pub fn hierarchical_clustering<E: AsEntity>(
        &self,
        entities: &[E],
        threshold: f64,
    ) -> Result<Vec<Cluster>, SemanticaError> {
        self.hierarchical(&entity_refs(entities), &float_value(threshold), None)
    }

    fn hierarchical(
        &self,
        entities: &[EntityRef],
        threshold: &Value,
        tracking_id: Option<&str>,
    ) -> Result<Vec<Cluster>, SemanticaError> {
        let tracker = &self.progress_tracker;
        let mut clusters: Vec<Cluster> = entities
            .iter()
            .enumerate()
            .map(|(i, e)| Cluster::new(format!("cluster_{i}"), vec![e.clone()]))
            .collect();

        let mut iteration = 0usize;
        let total_iterations = entities.len();
        let update_interval = if total_iterations <= 10 {
            1
        } else {
            (total_iterations / 20).max(1)
        };

        let mut merged = true;
        while merged {
            merged = false;
            let mut best_merge: Option<(usize, usize)> = None;
            let mut best_similarity = threshold.clone();

            let total_comparisons = clusters.len() * clusters.len().saturating_sub(1) / 2;
            let mut processed_comparisons = 0usize;
            let comparison_update_interval = if total_comparisons <= 10 {
                1
            } else {
                (total_comparisons / 20).max(1)
            };
            if let Some(id) = tracking_id
                && total_comparisons > 0
            {
                tracker.update_progress(
                    id,
                    0,
                    total_comparisons as i64,
                    &format!(
                        "Comparing clusters... 0/{total_comparisons} (remaining: {total_comparisons})"
                    ),
                );
            }

            for i in 0..clusters.len() {
                for j in i + 1..clusters.len() {
                    let similarity = self.cluster_similarity(&clusters[i], &clusters[j])?;
                    if compare_f64(similarity, &best_similarity, Cmp::Ge)? {
                        best_similarity = float_value(similarity);
                        best_merge = Some((i, j));
                    }
                    processed_comparisons += 1;
                    if let Some(id) = tracking_id
                        && (processed_comparisons % comparison_update_interval == 0
                            || processed_comparisons == total_comparisons
                            || processed_comparisons == 1
                            || total_comparisons <= 10)
                    {
                        tracker.update_progress(
                            id,
                            processed_comparisons as i64,
                            total_comparisons as i64,
                            &format!(
                                "Comparing clusters... {processed_comparisons}/{total_comparisons} (remaining: {})",
                                total_comparisons - processed_comparisons
                            ),
                        );
                    }
                }
            }

            if let Some((i, j)) = best_merge {
                let mut entities = clusters[i].entities.clone();
                entities.extend(clusters[j].entities.iter().cloned());
                let mut merged_cluster = Cluster::new(clusters[i].cluster_id.clone(), entities);
                merged_cluster
                    .metadata
                    .insert("merge_similarity".to_string(), best_similarity.clone());
                clusters[i] = merged_cluster;
                clusters.remove(j);
                merged = true;

                iteration += 1;
                if let Some(id) = tracking_id
                    && (iteration % update_interval == 0
                        || iteration == total_iterations
                        || iteration == 1
                        || total_iterations <= 10)
                {
                    tracker.update_progress(
                        id,
                        iteration as i64,
                        total_iterations as i64,
                        &format!(
                            "Hierarchical clustering iteration {iteration}/{total_iterations}... {} clusters remaining (remaining: {} iterations)",
                            clusters.len(),
                            total_iterations as i64 - iteration as i64
                        ),
                    );
                }
            }
        }
        Ok(clusters)
    }

    /// `_cluster_similarity(cluster1, cluster2)`: the mean similarity over all cross pairs.
    fn cluster_similarity(&self, cluster1: &Cluster, cluster2: &Cluster) -> Result<f64, SemanticaError> {
        let mut similarities = Vec::new();
        for entity1 in &cluster1.entities {
            for entity2 in &cluster2.entities {
                similarities.push(
                    self.similarity_calculator
                        .calculate_similarity(entity1, entity2, true, &Map::new())?
                        .score,
                );
            }
        }
        Ok(mean(&similarities))
    }

    /// `_calculate_cluster_quality(clusters)`: sets each cluster's quality score and returns
    /// the summary metrics.
    fn calculate_cluster_quality(
        &self,
        clusters: &mut [Cluster],
    ) -> Result<Map<String, Value>, SemanticaError> {
        let mut metrics = Map::new();
        if clusters.is_empty() {
            metrics.insert("average_size".to_string(), json!(0));
            metrics.insert("average_quality".to_string(), json!(0.0));
            metrics.insert("total_clusters".to_string(), json!(0));
            return Ok(metrics);
        }
        for cluster in clusters.iter_mut() {
            cluster.quality_score = self.cluster_quality_score(cluster)?;
        }
        let n = clusters.len();
        let avg_quality = clusters.iter().fold(0.0, |acc, c| acc + c.quality_score) / n as f64;
        let size_sum: usize = clusters.iter().map(|c| c.entities.len()).sum();
        let avg_size = size_sum as f64 / n as f64;
        metrics.insert("average_size".to_string(), float_value(avg_size));
        metrics.insert("average_quality".to_string(), float_value(avg_quality));
        metrics.insert("total_clusters".to_string(), json!(n));
        metrics.insert(
            "high_quality_clusters".to_string(),
            json!(clusters.iter().filter(|c| c.quality_score >= 0.7).count()),
        );
        Ok(metrics)
    }

    /// `_cluster_quality_score(cluster)`: mean pairwise similarity, × 0.8 outside the size
    /// limits; 0.0 below two entities.
    fn cluster_quality_score(&self, cluster: &Cluster) -> Result<f64, SemanticaError> {
        let n = cluster.entities.len();
        if n < 2 {
            return Ok(0.0);
        }
        let mut similarities = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                similarities.push(
                    self.similarity_calculator
                        .calculate_similarity(
                            &cluster.entities[i],
                            &cluster.entities[j],
                            true,
                            &Map::new(),
                        )?
                        .score,
                );
            }
        }
        let avg_similarity = mean(&similarities);
        let size = n as f64;
        let size_factor = if size < self.min_cluster_size || size > self.max_cluster_size {
            0.8
        } else {
            1.0
        };
        Ok(avg_similarity * size_factor)
    }

    /// `update_clusters(existing_clusters, new_entities, **options)`: add each new entity to
    /// the existing cluster it is most similar to (at least `threshold`, the last best on
    /// ties) or to a new singleton cluster — `existing_clusters` is updated in place, as in
    /// the original — then rebuild clusters from all their entities.
    pub fn update_clusters<E: AsEntity>(
        &self,
        existing_clusters: &mut Vec<Cluster>,
        new_entities: &[E],
        options: &Map<String, Value>,
    ) -> Result<ClusterResult, SemanticaError> {
        let threshold = options
            .get("threshold")
            .cloned()
            .unwrap_or_else(|| float_value(self.similarity_threshold));
        for entity in entity_refs(new_entities) {
            let mut best_cluster: Option<usize> = None;
            let mut best_similarity = threshold.clone();
            for (index, cluster) in existing_clusters.iter().enumerate() {
                let similarity = self.entity_cluster_similarity(&entity, cluster)?;
                if compare_f64(similarity, &best_similarity, Cmp::Ge)? {
                    best_similarity = float_value(similarity);
                    best_cluster = Some(index);
                }
            }
            match best_cluster {
                Some(index) => existing_clusters[index].entities.push(entity),
                None => {
                    let cluster_id = format!("cluster_{}", existing_clusters.len());
                    existing_clusters.push(Cluster::new(cluster_id, vec![entity]));
                }
            }
        }
        let all_entities: Vec<EntityRef> = existing_clusters
            .iter()
            .flat_map(|c| c.entities.iter().cloned())
            .collect();
        self.build_clusters(&all_entities, options)
    }

    /// `_entity_cluster_similarity(entity, cluster)`: the mean similarity to the cluster's
    /// entities (0.0 for an empty cluster).
    fn entity_cluster_similarity(
        &self,
        entity: &EntityRef,
        cluster: &Cluster,
    ) -> Result<f64, SemanticaError> {
        if cluster.entities.is_empty() {
            return Ok(0.0);
        }
        let mut similarities = Vec::with_capacity(cluster.entities.len());
        for e in &cluster.entities {
            similarities.push(
                self.similarity_calculator
                    .calculate_similarity(entity, e, true, &Map::new())?
                    .score,
            );
        }
        Ok(mean(&similarities))
    }
}

/// `sum(values) / len(values) if values else 0.0` (left-to-right sum).
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().fold(0.0, |acc, v| acc + v) / values.len() as f64
    }
}
