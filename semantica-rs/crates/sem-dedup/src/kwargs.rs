//! Python keyword-argument binding over `serde_json` maps.
//!
//! The original classes are built from nested configuration dicts
//! (`EntityMerger(detector={...})` calls `DuplicateDetector(**detector_config)`), so the Rust
//! constructors take the keyword arguments as a map and bind named parameters the way Python
//! does: a named parameter is taken out of the map, everything else lands in `**kwargs`.

use sem_utils::SemanticaError;
use sem_utils::pycompat::fmt::type_name;
use serde_json::{Map, Value};

use crate::pyops::{attr_error, number, py_repr, truthy, type_error};

/// Keyword arguments being bound to a signature.
#[derive(Debug, Clone, Default)]
pub(crate) struct Kwargs(Map<String, Value>);

impl Kwargs {
    pub(crate) fn new(map: &Map<String, Value>) -> Kwargs {
        Kwargs(map.clone())
    }

    /// Bind the named parameter `name` (removing it from `**kwargs`).
    pub(crate) fn take(&mut self, name: &str) -> Option<Value> {
        self.0.shift_remove(name)
    }

    /// The remaining `**kwargs`.
    pub(crate) fn rest(self) -> Map<String, Value> {
        self.0
    }
}

/// `self.config = config or {}; self.config.update(kwargs)`.
pub(crate) fn merge_config(
    config: Option<Value>,
    kwargs: Map<String, Value>,
) -> Result<Map<String, Value>, SemanticaError> {
    let mut merged = match config {
        Some(Value::Object(map)) => map,
        Some(v) if truthy(&v) => return Err(attr_error(&v, "update")),
        _ => Map::new(),
    };
    for (k, v) in kwargs {
        merged.insert(k, v);
    }
    Ok(merged)
}

/// A float parameter: Python numbers (and bools) convert; `None` and other types are a
/// `TypeError` (the original stores them and fails later, at first use).
pub(crate) fn float_param(
    name: &str,
    value: Option<Value>,
    default: f64,
) -> Result<f64, SemanticaError> {
    match value {
        None => Ok(default),
        Some(v) => number(&v).ok_or_else(|| {
            type_error(format!(
                "{name} must be a number, got {} ({})",
                py_repr(&v),
                type_name(&v)
            ))
        }),
    }
}

/// A boolean parameter (Python truthiness).
pub(crate) fn bool_param(value: Option<Value>, default: bool) -> bool {
    value.map(|v| truthy(&v)).unwrap_or(default)
}

/// An optional float threshold: `None`/`null` stays unset.
pub(crate) fn opt_float_param(
    name: &str,
    value: Option<Value>,
) -> Result<Option<f64>, SemanticaError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(v) => float_param(name, Some(v), 0.0).map(Some),
    }
}
