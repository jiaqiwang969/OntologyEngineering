//! Configuration Management Module for Embeddings (port of `semantica/embeddings/config.py`).
//!
//! Centralised configuration for embedding generation, from config files (YAML, JSON, TOML),
//! `EMBEDDING_*` environment variables and programmatic calls, with the fallback chain
//! config -> environment -> default.
//!
//! ```no_run
//! use sem_embeddings::embeddings_config;
//! use serde_json::json;
//!
//! let model = embeddings_config.get("model", json!("all-MiniLM-L6-v2"));
//! embeddings_config.set("model", json!("sentence-transformers/all-mpnet-base-v2"));
//! let method_config = embeddings_config.get_method_config("text");
//! # let _ = (model, method_config);
//! ```

use std::fmt;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use once_cell::sync::Lazy;
use sem_utils::logging::Logger;
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Number, Value};

use crate::pyconv::{py_float, py_int, py_lower, py_upper};

/// The `EMBEDDING_*` variables with a declared type: `(variable, config key, type)`.
const ENV_MAPPINGS: [(&str, &str, EnvType); 9] = [
    ("EMBEDDING_MODEL", "model", EnvType::Str),
    ("EMBEDDING_BATCH_SIZE", "batch_size", EnvType::Int),
    ("EMBEDDING_DIMENSION", "dimension", EnvType::Int),
    ("EMBEDDING_NORMALIZE", "normalize", EnvType::Bool),
    ("EMBEDDING_DEVICE", "device", EnvType::Str),
    ("EMBEDDING_COMPRESSION_METHOD", "compression_method", EnvType::Str),
    ("EMBEDDING_WINDOW_SIZE", "window_size", EnvType::Int),
    ("EMBEDDING_OVERLAP", "overlap", EnvType::Int),
    ("EMBEDDING_SAMPLE_RATE", "sample_rate", EnvType::Int),
];

#[derive(Clone, Copy)]
enum EnvType {
    Str,
    Int,
    Bool,
}

/// Where environment variables are read from.
#[derive(Clone, Debug)]
enum EnvSource {
    /// The process environment (`os.environ`), read at call time.
    Process,
    /// A fixed snapshot (tests).
    Fixed(Vec<(String, String)>),
}

impl EnvSource {
    fn var(&self, key: &str) -> Option<String> {
        match self {
            EnvSource::Process => {
                std::env::var_os(key).map(|v| v.to_string_lossy().into_owned())
            }
            EnvSource::Fixed(vars) => vars.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
        }
    }

    fn vars(&self) -> Vec<(String, String)> {
        match self {
            EnvSource::Process => std::env::vars_os()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.to_string_lossy().into_owned(),
                    )
                })
                .collect(),
            EnvSource::Fixed(vars) => vars.clone(),
        }
    }
}

#[derive(Default)]
struct ConfigState {
    configs: Map<String, Value>,
    method_configs: Map<String, Value>,
}

/// Configuration manager for embeddings module - supports .env files, environment variables,
/// and programmatic config.
pub struct EmbeddingsConfig {
    logger: Logger,
    env: EnvSource,
    state: Mutex<ConfigState>,
}

impl fmt::Debug for EmbeddingsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddingsConfig")
            .field("config", &self.get_all())
            .finish()
    }
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        EmbeddingsConfig::new(None)
    }
}

fn number_value(f: f64) -> Value {
    Number::from_f64(f).map_or(Value::Null, Value::Number)
}

fn int_value(i: i128) -> Value {
    if let Ok(v) = i64::try_from(i) {
        Value::from(v)
    } else if let Ok(v) = u64::try_from(i) {
        Value::from(v)
    } else {
        number_value(i as f64)
    }
}

impl EmbeddingsConfig {
    /// `EmbeddingsConfig(config_file=None)`: loads the config file (when it exists), then the
    /// `EMBEDDING_*` variables of the process environment.
    pub fn new(config_file: Option<&str>) -> EmbeddingsConfig {
        Self::build(config_file, EnvSource::Process)
    }

    /// Like [`EmbeddingsConfig::new`], reading environment variables from `env` instead of the
    /// process environment (both at construction and in [`EmbeddingsConfig::get`]).
    pub fn with_env<I, K, V>(config_file: Option<&str>, env: I) -> EmbeddingsConfig
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let vars = env.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        Self::build(config_file, EnvSource::Fixed(vars))
    }

    fn build(config_file: Option<&str>, env: EnvSource) -> EmbeddingsConfig {
        let config = EmbeddingsConfig {
            logger: get_logger("embeddings_config"),
            env,
            state: Mutex::new(ConfigState::default()),
        };
        config.load_config_file(config_file);
        config.load_env_vars();
        config
    }

    fn lock(&self) -> MutexGuard<'_, ConfigState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn load_config_file(&self, config_file: Option<&str>) {
        let Some(path) = config_file.filter(|p| !p.is_empty()) else {
            return;
        };
        if !Path::new(path).exists() {
            return;
        }
        let mut state = self.lock();
        match load_file(path, &mut state) {
            Ok(()) => self
                .logger
                .info(format!("Loaded embeddings config from {path}")),
            Err(e) => self
                .logger
                .warning(format!("Failed to load config file {path}: {e}")),
        }
    }

    fn load_env_vars(&self) {
        let mut state = self.lock();
        for (env_key, config_key, kind) in ENV_MAPPINGS {
            let Some(value) = self.env.var(env_key).filter(|v| !v.is_empty()) else {
                continue;
            };
            match kind {
                EnvType::Bool => {
                    let on = matches!(py_lower(&value).as_str(), "true" | "1" | "yes" | "on");
                    state.configs.insert(config_key.to_string(), Value::Bool(on));
                }
                EnvType::Int => match py_int(&value) {
                    Some(i) => {
                        state.configs.insert(config_key.to_string(), int_value(i));
                    }
                    None => self
                        .logger
                        .warning(format!("Failed to parse {env_key}={value}")),
                },
                EnvType::Str => {
                    state
                        .configs
                        .insert(config_key.to_string(), Value::String(value));
                }
            }
        }

        // Also check for any EMBEDDING_ prefixed variables
        let prefix = "EMBEDDING_";
        for (key, value) in self.env.vars() {
            if !key.starts_with(prefix) || ENV_MAPPINGS.iter().any(|(k, _, _)| *k == key) {
                continue;
            }
            let config_key = py_lower(&key[prefix.len()..]);
            let lowered = py_lower(&value);
            let converted = if lowered == "true" || lowered == "false" {
                Value::Bool(lowered == "true")
            } else if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
                py_int(&value).map_or(Value::String(value.clone()), int_value)
            } else {
                match py_float(&value) {
                    Some(f) => number_value(f),
                    None => Value::String(value.clone()),
                }
            };
            state.configs.insert(config_key, converted);
        }
    }

    /// `set(key, value)`: set a configuration value programmatically.
    pub fn set(&self, key: &str, value: impl Into<Value>) {
        self.lock().configs.insert(key.to_string(), value.into());
    }

    /// `get(key, default=None)`: the fallback chain config -> `EMBEDDING_<KEY>` -> default
    /// (`Value::Null` is Python's `None`).
    ///
    /// An environment value is converted by the type of `default`: an `int` (or `bool`, which
    /// Python also treats as `int`) default parses it with `int()`, a `float` default with
    /// `float()`, anything else returns the string; a value that fails to parse gives `default`.
    pub fn get(&self, key: &str, default: impl Into<Value>) -> Value {
        let default = default.into();
        if let Some(value) = self.lock().configs.get(key) {
            return value.clone();
        }
        let env_key = format!("EMBEDDING_{}", py_upper(key));
        if let Some(value) = self.env.var(&env_key).filter(|v| !v.is_empty()) {
            match &default {
                Value::Bool(_) => {
                    if let Some(i) = py_int(&value) {
                        return int_value(i);
                    }
                }
                Value::Number(n) if !n.is_f64() => {
                    if let Some(i) = py_int(&value) {
                        return int_value(i);
                    }
                }
                Value::Number(_) => {
                    if let Some(f) = py_float(&value) {
                        return number_value(f);
                    }
                }
                _ => return Value::String(value),
            }
        }
        default
    }

    /// `set_method_config(method, **config)`: replace the method-specific configuration.
    pub fn set_method_config(&self, method: &str, config: Map<String, Value>) {
        self.lock()
            .method_configs
            .insert(method.to_string(), Value::Object(config));
    }

    /// `get_method_config(method)`: the method-specific configuration (empty when unset; a
    /// non-mapping value loaded from a config file also reads as empty).
    pub fn get_method_config(&self, method: &str) -> Map<String, Value> {
        match self.lock().method_configs.get(method) {
            Some(Value::Object(map)) => map.clone(),
            _ => Map::new(),
        }
    }

    /// `get_all()`: `{"config": {...}, "method_configs": {...}}`.
    pub fn get_all(&self) -> Map<String, Value> {
        let state = self.lock();
        let mut all = Map::new();
        all.insert("config".to_string(), Value::Object(state.configs.clone()));
        all.insert(
            "method_configs".to_string(),
            Value::Object(state.method_configs.clone()),
        );
        all
    }

    /// Runs `f` on the stored method configuration when one exists (Python's
    /// `get_method_config` returns that very dict, so callers mutate it in place), otherwise on
    /// a fresh empty map that is not stored.
    pub(crate) fn with_method_config<R>(
        &self,
        method: &str,
        f: impl FnOnce(&mut Map<String, Value>) -> R,
    ) -> R {
        let mut state = self.lock();
        match state.method_configs.get_mut(method) {
            Some(Value::Object(map)) => f(map),
            _ => {
                let mut fresh = Map::new();
                f(&mut fresh)
            }
        }
    }
}

fn load_file(path: &str, state: &mut ConfigState) -> Result<(), SemanticaError> {
    let read = || std::fs::read_to_string(path).map_err(|e| SemanticaError::from_io(&e, Some(Path::new(path))));
    if path.ends_with(".yaml") || path.ends_with(".yml") {
        let text = read()?;
        let data = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_yaml::from_str::<Value>(&text)
                .map_err(|e| SemanticaError::Other {
                    module: "yaml.parser".to_string(),
                    type_name: "ParserError".to_string(),
                    message: e.to_string(),
                })?
        };
        merge_sections(data, state, false)
    } else if path.ends_with(".json") {
        let text = read()?;
        let data = serde_json::from_str::<Value>(&text)
            .map_err(|e| SemanticaError::JsonDecode(e.to_string()))?;
        merge_sections(data, state, false)
    } else if path.ends_with(".toml") {
        let text = read()?;
        let table = text.parse::<toml::Table>().map_err(|e| SemanticaError::Other {
            module: "toml.decoder".to_string(),
            type_name: "TomlDecodeError".to_string(),
            message: e.to_string(),
        })?;
        merge_sections(toml_to_json(toml::Value::Table(table)), state, true)
    } else {
        Ok(())
    }
}

/// `data = load(f) or {}`; `configs.update(data.get("embeddings", {}))`;
/// `method_configs.update(data.get("embeddings_methods", {}))` (TOML only updates sections that
/// are present).
fn merge_sections(data: Value, state: &mut ConfigState, toml: bool) -> Result<(), SemanticaError> {
    let data = if sem_utils::pycompat::fmt::truthy(&data) {
        data
    } else {
        Value::Object(Map::new())
    };
    let Value::Object(data) = data else {
        return Err(SemanticaError::Attribute(format!(
            "'{}' object has no attribute 'get'",
            sem_utils::pycompat::fmt::type_name(&data)
        )));
    };
    for (section, target) in [("embeddings", 0), ("embeddings_methods", 1)] {
        let value = match data.get(section) {
            Some(v) => v.clone(),
            None if toml => continue,
            None => Value::Object(Map::new()),
        };
        let Value::Object(entries) = value else {
            return Err(SemanticaError::Type(format!(
                "'{}' object is not iterable",
                sem_utils::pycompat::fmt::type_name(&value)
            )));
        };
        let dest = if target == 0 {
            &mut state.configs
        } else {
            &mut state.method_configs
        };
        for (k, v) in entries {
            dest.insert(k, v);
        }
    }
    Ok(())
}

fn toml_to_json(value: toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::from(i),
        toml::Value::Float(f) => number_value(f),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => Value::Array(items.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => Value::Object(
            table
                .into_iter()
                .map(|(k, v)| (k, toml_to_json(v)))
                .collect(),
        ),
    }
}

/// The global `embeddings_config` instance (created on first use from the process
/// environment, as the original creates it at import).
#[allow(non_upper_case_globals)]
pub static embeddings_config: Lazy<EmbeddingsConfig> = Lazy::new(|| EmbeddingsConfig::new(None));
