//! Configuration management for ingestion (`semantica/ingest/config.py`).
//!
//! Supports config files (YAML, JSON, TOML), `INGEST_*` / `MCP_*` environment variables and
//! programmatic values, with the fallback chain config -> environment -> default.

use std::path::Path;
use std::sync::RwLock;

use once_cell::sync::Lazy;
use sem_utils::logging::Logger;
use sem_utils::{SemanticaError, get_logger};
use serde_json::{Map, Number, Value};

/// Python `int(text)` for decimal strings (whitespace, sign and single underscores between
/// digits allowed).
pub fn py_int(text: &str) -> Option<i64> {
    let t = text.trim();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    digits.replace('_', "").parse::<i64>().ok().map(|v| v * sign)
}

/// Python `float(text)`.
pub fn py_float(text: &str) -> Option<f64> {
    let t = text.trim();
    let lower = t.to_lowercase();
    let unsigned = lower.trim_start_matches(['+', '-']);
    if matches!(unsigned, "inf" | "infinity" | "nan") {
        let negative = lower.starts_with('-');
        let v = if unsigned == "nan" { f64::NAN } else { f64::INFINITY };
        return Some(if negative { -v } else { v });
    }
    if t.is_empty() || t.contains("__") || t.starts_with('_') || t.ends_with('_') {
        return None;
    }
    let cleaned = t.replace('_', "");
    if !cleaned
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
    {
        return None;
    }
    cleaned.parse::<f64>().ok()
}

fn number_value(f: f64) -> Value {
    Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
}

/// `IngestConfig`: configuration manager for the ingest module.
pub struct IngestConfig {
    logger: Logger,
    configs: RwLock<Map<String, Value>>,
    method_configs: RwLock<Map<String, Value>>,
}

impl std::fmt::Debug for IngestConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IngestConfig")
            .field("configs", &self.configs.read().map(|c| c.clone()).unwrap_or_default())
            .field(
                "method_configs",
                &self.method_configs.read().map(|c| c.clone()).unwrap_or_default(),
            )
            .finish()
    }
}

impl Default for IngestConfig {
    fn default() -> Self {
        IngestConfig::new(None)
    }
}

impl IngestConfig {
    /// `IngestConfig(config_file=None)`.
    pub fn new(config_file: Option<&str>) -> IngestConfig {
        let config = IngestConfig {
            logger: get_logger("ingest_config"),
            configs: RwLock::new(Map::new()),
            method_configs: RwLock::new(Map::new()),
        };
        config.load_config_file(config_file);
        config.load_env_vars();
        config
    }

    fn load_config_file(&self, config_file: Option<&str>) {
        let Some(file) = config_file else { return };
        if !Path::new(file).exists() {
            return;
        }
        let loaded: Result<Value, String> = (|| {
            let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
            if file.ends_with(".yaml") || file.ends_with(".yml") {
                let v: Value = serde_yaml::from_str(&text).map_err(|e| e.to_string())?;
                Ok(if v.is_null() { Value::Object(Map::new()) } else { v })
            } else if file.ends_with(".json") {
                let v: Value = crate::support::pyjson::loads(&text).map_err(|e| e.to_string())?;
                Ok(if v.is_null() { Value::Object(Map::new()) } else { v })
            } else if file.ends_with(".toml") {
                let v: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
                serde_json::to_value(v).map_err(|e| e.to_string())
            } else {
                Ok(Value::Null)
            }
        })();
        match loaded {
            Ok(Value::Null) => {
                self.logger
                    .info(format!("Loaded ingest config from {file}"));
            }
            Ok(data) => {
                if let Some(ingest) = data.get("ingest").and_then(Value::as_object) {
                    if let Ok(mut c) = self.configs.write() {
                        for (k, v) in ingest {
                            c.insert(k.clone(), v.clone());
                        }
                    }
                }
                if let Some(methods) = data.get("ingest_methods").and_then(Value::as_object)
                    && let Ok(mut m) = self.method_configs.write()
                {
                    for (k, v) in methods {
                        m.insert(k.clone(), v.clone());
                    }
                }
                self.logger.info(format!("Loaded ingest config from {file}"));
            }
            Err(e) => {
                self.logger
                    .warning(format!("Failed to load config file {file}: {e}"));
            }
        }
    }

    fn load_env_vars(&self) {
        enum Kind {
            Str,
            Int,
            Bool,
            Float,
        }
        let mappings: [(&str, &str, Kind); 10] = [
            ("INGEST_DEFAULT_SOURCE_TYPE", "default_source_type", Kind::Str),
            ("INGEST_MAX_FILE_SIZE", "max_file_size", Kind::Int),
            ("INGEST_RECURSIVE", "recursive", Kind::Bool),
            ("INGEST_READ_CONTENT", "read_content", Kind::Bool),
            ("INGEST_RATE_LIMIT_DELAY", "rate_limit_delay", Kind::Float),
            ("INGEST_RESPECT_ROBOTS", "respect_robots", Kind::Bool),
            ("INGEST_BATCH_SIZE", "batch_size", Kind::Int),
            ("INGEST_TIMEOUT", "timeout", Kind::Float),
            ("MCP_SERVER_URL", "mcp_server_url", Kind::Str),
            ("MCP_SERVER_TIMEOUT", "mcp_server_timeout", Kind::Float),
        ];
        let Ok(mut configs) = self.configs.write() else { return };
        for (env_key, config_key, kind) in &mappings {
            let Ok(value) = std::env::var(env_key) else { continue };
            if value.is_empty() {
                continue;
            }
            let parsed = match kind {
                Kind::Str => Some(Value::String(value.clone())),
                Kind::Bool => Some(Value::Bool(matches!(
                    value.to_lowercase().as_str(),
                    "true" | "1" | "yes" | "on"
                ))),
                Kind::Int => py_int(&value).map(Value::from),
                Kind::Float => py_float(&value).map(number_value),
            };
            match parsed {
                Some(v) => {
                    configs.insert(config_key.to_string(), v);
                }
                None => self
                    .logger
                    .warning(format!("Failed to parse {env_key}={value}")),
            }
        }
        let mapped: Vec<&str> = mappings.iter().map(|(k, _, _)| *k).collect();
        let convert = |value: &str| -> Value {
            let lower = value.to_lowercase();
            if lower == "true" || lower == "false" {
                Value::Bool(lower == "true")
            } else if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
                py_int(value).map(Value::from).unwrap_or(Value::String(value.to_string()))
            } else if let Some(f) = py_float(value) {
                number_value(f)
            } else {
                Value::String(value.to_string())
            }
        };
        for (key, value) in std::env::vars() {
            if let Some(rest) = key.strip_prefix("INGEST_")
                && !mapped.contains(&key.as_str())
            {
                configs.insert(rest.to_lowercase(), convert(&value));
            }
        }
        for (key, value) in std::env::vars() {
            if let Some(rest) = key.strip_prefix("MCP_")
                && !mapped.contains(&key.as_str())
            {
                configs.insert(format!("mcp_{}", rest.to_lowercase()), convert(&value));
            }
        }
    }

    /// `set(key, value)`.
    pub fn set(&self, key: &str, value: impl Into<Value>) {
        if let Ok(mut c) = self.configs.write() {
            c.insert(key.to_string(), value.into());
        }
    }

    /// `get(key, default=None)`: config, then `INGEST_<KEY>` (converted like `default`), then
    /// `default`.
    pub fn get(&self, key: &str, default: Value) -> Value {
        if let Ok(c) = self.configs.read()
            && let Some(v) = c.get(key)
        {
            return v.clone();
        }
        let env_key = format!("INGEST_{}", key.to_uppercase());
        if let Ok(value) = std::env::var(&env_key)
            && !value.is_empty()
        {
            // bool is an int subclass in Python, so int() is tried first for bool defaults.
            match &default {
                Value::Bool(_) => {
                    if let Some(i) = py_int(&value) {
                        return Value::from(i);
                    }
                    return default;
                }
                Value::Number(n) if n.is_i64() || n.is_u64() => {
                    return py_int(&value).map(Value::from).unwrap_or(default);
                }
                Value::Number(_) => {
                    return py_float(&value).map(number_value).unwrap_or(default);
                }
                _ => return Value::String(value),
            }
        }
        default
    }

    /// `set_method_config(method, **config)`.
    pub fn set_method_config(&self, method: &str, config: Map<String, Value>) {
        if let Ok(mut m) = self.method_configs.write() {
            m.insert(method.to_string(), Value::Object(config));
        }
    }

    /// `get_method_config(method)` (a copy; `{}` when unset).
    pub fn get_method_config(&self, method: &str) -> Map<String, Value> {
        self.method_configs
            .read()
            .ok()
            .and_then(|m| m.get(method).and_then(Value::as_object).cloned())
            .unwrap_or_default()
    }

    /// `config = get_method_config(method); config.update(kwargs)` as the convenience
    /// functions do: a stored method config is the very dict being updated, so the keyword
    /// arguments persist in it; without one a fresh dict is used.
    pub fn merged_method_config(&self, method: &str, kwargs: &Map<String, Value>) -> Map<String, Value> {
        if let Ok(mut m) = self.method_configs.write()
            && let Some(Value::Object(stored)) = m.get_mut(method)
        {
            for (k, v) in kwargs {
                stored.insert(k.clone(), v.clone());
            }
            return stored.clone();
        }
        kwargs.clone()
    }

    /// `get_all()`.
    pub fn get_all(&self) -> Map<String, Value> {
        let mut out = Map::new();
        out.insert(
            "config".to_string(),
            Value::Object(self.configs.read().map(|c| c.clone()).unwrap_or_default()),
        );
        out.insert(
            "method_configs".to_string(),
            Value::Object(self.method_configs.read().map(|c| c.clone()).unwrap_or_default()),
        );
        out
    }
}

static INGEST_CONFIG: Lazy<IngestConfig> = Lazy::new(|| IngestConfig::new(None));

/// `ingest_config`: the global config instance.
pub fn ingest_config() -> &'static IngestConfig {
    &INGEST_CONFIG
}

/// Error helper shared by the ingestors: `ValidationError(message)`.
pub(crate) fn validation_error(message: impl Into<String>) -> SemanticaError {
    sem_utils::ValidationError::new(message).into()
}

/// Error helper shared by the ingestors: `ProcessingError(message)`.
pub(crate) fn processing_error(message: impl Into<String>) -> SemanticaError {
    sem_utils::ProcessingError::new(message).into()
}

/// Error helper shared by the ingestors: `ConfigurationError(message)`.
pub(crate) fn configuration_error(message: impl Into<String>) -> SemanticaError {
    sem_utils::ConfigurationError::new(message).into()
}
