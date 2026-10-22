//! Pooling strategies for Semantica framework (port of
//! `semantica/embeddings/pooling_strategies.py`).
//!
//! This module provides various pooling methods for embedding generation and aggregation:
//! mean, max, CLS-token, attention-weighted and hierarchical pooling. The arithmetic runs in the
//! input's dtype with numpy's evaluation order (see [`crate::ndarray`]).

use std::fmt;

use sem_utils::logging::Logger;
use sem_utils::{ProcessingError, SemanticaError, get_logger};
use serde_json::{Map, Value};

use crate::ndarray::{Float, NdArray, add_reduce, maximum, sum_axis0, with_float};
use crate::pyconv::py_index;

/// `len(embeddings)`: a 0-D value (a numpy scalar) has no length.
fn checked_len(embeddings: &NdArray) -> Result<usize, SemanticaError> {
    if embeddings.ndim() == 0 {
        return Err(SemanticaError::Type(format!(
            "object of type 'numpy.{}' has no len()",
            embeddings.dtype()
        )));
    }
    Ok(embeddings.len())
}

fn ensure_not_empty(embeddings: &NdArray) -> Result<(), SemanticaError> {
    if checked_len(embeddings)? == 0 {
        return Err(ProcessingError::new("Cannot pool empty embeddings").into());
    }
    Ok(())
}

/// Base class for pooling strategies.
///
/// The provided [`PoolingStrategy::pool`] is the base implementation: mean pooling for 2-D
/// input, other input returned unchanged. [`BasePoolingStrategy`] instantiates the base class
/// itself (`PoolingStrategy()`).
pub trait PoolingStrategy: Send + Sync + fmt::Debug {
    /// Pool embeddings `(n_embeddings, dim)` into one embedding `(dim,)`.
    fn pool(
        &self,
        embeddings: &NdArray,
        options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        let _ = options;
        // Base implementation - should be overridden. Default: return mean pooling
        if embeddings.ndim() == 2 {
            return embeddings.mean_axis0();
        }
        Ok(embeddings.clone())
    }
}

/// `PoolingStrategy()`: the base class used directly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BasePoolingStrategy;

impl PoolingStrategy for BasePoolingStrategy {}

/// Mean pooling strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MeanPooling;

impl PoolingStrategy for MeanPooling {
    /// Apply mean pooling (`np.mean(embeddings, axis=0)`).
    fn pool(
        &self,
        embeddings: &NdArray,
        _options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        ensure_not_empty(embeddings)?;
        embeddings.mean_axis0()
    }
}

/// Max pooling strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaxPooling;

impl PoolingStrategy for MaxPooling {
    /// Apply max pooling (`np.max(embeddings, axis=0)`).
    fn pool(
        &self,
        embeddings: &NdArray,
        _options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        ensure_not_empty(embeddings)?;
        embeddings.max_axis0()
    }
}

/// CLS token pooling strategy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CLSPooling;

impl PoolingStrategy for CLSPooling {
    /// Apply CLS token pooling (returns the first embedding).
    fn pool(
        &self,
        embeddings: &NdArray,
        _options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        ensure_not_empty(embeddings)?;
        embeddings.row(0)
    }
}

/// Attention-based pooling strategy.
pub struct AttentionPooling {
    logger: Logger,
    /// `self.config` (the constructor's keyword arguments).
    pub config: Map<String, Value>,
}

impl fmt::Debug for AttentionPooling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttentionPooling")
            .field("config", &self.config)
            .finish()
    }
}

impl Default for AttentionPooling {
    fn default() -> Self {
        AttentionPooling::new(Map::new())
    }
}

impl AttentionPooling {
    /// `AttentionPooling(**config)`.
    pub fn new(config: Map<String, Value>) -> AttentionPooling {
        AttentionPooling {
            logger: get_logger("attention_pooling"),
            config,
        }
    }

    /// The `attention_pooling` logger.
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

/// Softmax weights over dot-product scores with the mean embedding as query, then the
/// weighted sum (2-D input).
fn attention_2d<T: Float>(data: &[T], rows: usize, cols: usize) -> Vec<T> {
    // query = np.mean(embeddings, axis=0)
    let query: Vec<T> = sum_axis0(data, rows, cols)
        .into_iter()
        .map(|s| s.div_count(rows))
        .collect();
    // scores = np.dot(embeddings, query); scores = scores - np.max(scores)
    let scores: Vec<T> = (0..rows)
        .map(|r| T::dot(&data[r * cols..(r + 1) * cols], &query))
        .collect();
    let weights = softmax(&scores);
    // np.sum(embeddings * weights[:, np.newaxis], axis=0)
    let weighted: Vec<T> = (0..rows)
        .flat_map(|r| {
            let w = weights[r];
            data[r * cols..(r + 1) * cols].iter().map(move |&x| x * w)
        })
        .collect();
    sum_axis0(&weighted, rows, cols)
}

/// `weights = np.exp(scores - max) / np.sum(np.exp(scores - max))`.
fn softmax<T: Float>(scores: &[T]) -> Vec<T> {
    let max = scores[1..]
        .iter()
        .fold(scores[0], |m, &s| maximum(m, s));
    let exps: Vec<T> = scores.iter().map(|&s| (s - max).exp()).collect();
    let total = add_reduce(&exps);
    exps.into_iter().map(|e| e / total).collect()
}

/// The same computation for a 1-D input, where numpy's broadcasting makes the weighted product
/// `embeddings * weights[:, None]` an `(n, n)` matrix.
fn attention_1d<T: Float>(x: &[T]) -> Vec<T> {
    let n = x.len();
    let query = add_reduce(x).div_count(n);
    let scores: Vec<T> = x.iter().map(|&v| v * query).collect();
    let weights = softmax(&scores);
    let product: Vec<T> = weights
        .iter()
        .flat_map(|&w| x.iter().map(move |&v| v * w))
        .collect();
    sum_axis0(&product, n, n)
}

impl PoolingStrategy for AttentionPooling {
    /// Apply attention-based pooling.
    fn pool(
        &self,
        embeddings: &NdArray,
        _options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        ensure_not_empty(embeddings)?;
        let shape = embeddings.shape().to_vec();
        Ok(with_float!(embeddings, |v: T| {
            if shape.len() == 1 {
                T::wrap(shape.clone(), attention_1d(v))
            } else {
                T::wrap(vec![shape[1]], attention_2d(v, shape[0], shape[1]))
            }
        }))
    }
}

/// Hierarchical pooling strategy.
pub struct HierarchicalPooling {
    logger: Logger,
    /// `self.config` (the constructor's keyword arguments).
    pub config: Map<String, Value>,
    /// First-level (per chunk) pooling.
    pub first_level: MeanPooling,
    /// Second-level (over chunks) pooling.
    pub second_level: MeanPooling,
}

impl fmt::Debug for HierarchicalPooling {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HierarchicalPooling")
            .field("config", &self.config)
            .finish()
    }
}

impl Default for HierarchicalPooling {
    fn default() -> Self {
        HierarchicalPooling::new(Map::new())
    }
}

impl HierarchicalPooling {
    /// `HierarchicalPooling(**config)`.
    pub fn new(config: Map<String, Value>) -> HierarchicalPooling {
        HierarchicalPooling {
            logger: get_logger("hierarchical_pooling"),
            config,
            first_level: MeanPooling,
            second_level: MeanPooling,
        }
    }

    /// The `hierarchical_pooling` logger.
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

impl PoolingStrategy for HierarchicalPooling {
    /// Apply hierarchical pooling: mean-pool chunks of `chunk_size` (option, default 10), then
    /// mean-pool the chunk results.
    fn pool(
        &self,
        embeddings: &NdArray,
        options: &Map<String, Value>,
    ) -> Result<NdArray, SemanticaError> {
        ensure_not_empty(embeddings)?;
        let chunk_size = match options.get("chunk_size") {
            Some(value) => py_index(value)?,
            None => 10,
        };
        if chunk_size == 0 {
            return Err(SemanticaError::Value(
                "range() arg 3 must not be zero".to_string(),
            ));
        }
        let n = embeddings.len();
        let mut chunked = Vec::new();
        if chunk_size > 0 {
            let step = chunk_size as usize;
            let mut i = 0;
            while i < n {
                let chunk = embeddings.slice_rows(i, i.saturating_add(step));
                chunked.push(self.first_level.pool(&chunk, &Map::new())?);
                i = i.saturating_add(step);
            }
        }
        let chunked_array = NdArray::stack(&chunked)?;
        self.second_level.pool(&chunked_array, &Map::new())
    }
}

/// Factory for creating pooling strategies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PoolingStrategyFactory;

impl PoolingStrategyFactory {
    /// `PoolingStrategyFactory.create(strategy, **config)`: `"mean"`, `"max"`, `"cls"`,
    /// `"attention"` or `"hierarchical"` (any case).
    ///
    /// Mean, max and CLS pooling take no arguments: a non-empty `config` is the `TypeError`
    /// Python raises for `MeanPooling(**config)`.
    pub fn create(
        strategy: &str,
        config: &Map<String, Value>,
    ) -> Result<Box<dyn PoolingStrategy>, SemanticaError> {
        let no_arguments = |name: &str| -> Result<(), SemanticaError> {
            if config.is_empty() {
                Ok(())
            } else {
                Err(SemanticaError::Type(format!("{name}() takes no arguments")))
            }
        };
        match strategy.to_lowercase().as_str() {
            "mean" => {
                no_arguments("MeanPooling")?;
                Ok(Box::new(MeanPooling))
            }
            "max" => {
                no_arguments("MaxPooling")?;
                Ok(Box::new(MaxPooling))
            }
            "cls" => {
                no_arguments("CLSPooling")?;
                Ok(Box::new(CLSPooling))
            }
            "attention" => Ok(Box::new(AttentionPooling::new(config.clone()))),
            "hierarchical" => Ok(Box::new(HierarchicalPooling::new(config.clone()))),
            _ => Err(ProcessingError::new(format!("Unsupported pooling strategy: {strategy}")).into()),
        }
    }
}
