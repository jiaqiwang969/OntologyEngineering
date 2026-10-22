//! Configuration Management Module for Normalize (`semantica/normalize/config.py`).
//!
//! Configuration from a YAML/JSON/TOML file (`normalize` and `normalize_methods` sections),
//! `NORMALIZE_*` environment variables and programmatic settings, plus the global
//! [`normalize_config`] instance.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use once_cell::sync::Lazy;
use sem_utils::get_logger;
use sem_utils::logging::Logger;
use serde_json::{Map, Value, json};

use crate::pystr;

#[derive(Debug, Default)]
struct ConfigState {
    configs: Map<String, Value>,
    method_configs: Map<String, Value>,
}

/// Configuration manager for the normalize module (`NormalizeConfig(config_file=None)`).
pub struct NormalizeConfig {
    logger: Logger,
    state: Mutex<ConfigState>,
}

impl std::fmt::Debug for NormalizeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalizeConfig")
            .field("state", &*self.lock())
            .finish()
    }
}

impl Default for NormalizeConfig {
    fn default() -> Self {
        NormalizeConfig::new(None)
    }
}

const ENV_MAPPINGS: [(&str, &str); 6] = [
    ("NORMALIZE_UNICODE_FORM", "unicode_form"),
    ("NORMALIZE_CASE", "case"),
    ("NORMALIZE_DATE_FORMAT", "date_format"),
    ("NORMALIZE_TIMEZONE", "timezone"),
    ("NORMALIZE_DEFAULT_LANGUAGE", "default_language"),
    ("NORMALIZE_DEFAULT_ENCODING", "default_encoding"),
];

fn update(target: &mut Map<String, Value>, source: Option<&Value>) -> Result<(), String> {
    match source {
        None => Ok(()),
        Some(Value::Object(map)) => {
            for (k, v) in map {
                target.insert(k.clone(), v.clone());
            }
            Ok(())
        }
        Some(other) => Err(format!(
            "dictionary update sequence element #0 has the wrong type ({})",
            sem_utils::pycompat::fmt::type_name(other)
        )),
    }
}

impl NormalizeConfig {
    /// `NormalizeConfig(config_file)`: load the file (when it exists), then the environment.
    pub fn new(config_file: Option<&str>) -> NormalizeConfig {
        let config = NormalizeConfig {
            logger: get_logger("normalize_config"),
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
        let Some(config_file) = config_file.filter(|f| !f.is_empty()) else {
            return;
        };
        if !Path::new(config_file).exists() {
            return;
        }
        let result = (|| -> Result<(), String> {
            let text = std::fs::read_to_string(config_file).map_err(|e| e.to_string())?;
            let mut state = self.lock();
            if config_file.ends_with(".yaml") || config_file.ends_with(".yml") {
                let data: Value = serde_yaml::from_str::<Option<Value>>(&text)
                    .map_err(|e| e.to_string())?
                    .unwrap_or(Value::Null);
                let data = if sem_utils::pycompat::fmt::truthy(&data) {
                    data
                } else {
                    json!({})
                };
                let Value::Object(data) = data else {
                    return Err(format!(
                        "'{}' object has no attribute 'get'",
                        sem_utils::pycompat::fmt::type_name(&data)
                    ));
                };
                update(
                    &mut state.configs,
                    Some(data.get("normalize").unwrap_or(&json!({}))),
                )?;
                update(
                    &mut state.method_configs,
                    Some(data.get("normalize_methods").unwrap_or(&json!({}))),
                )?;
            } else if config_file.ends_with(".json") {
                let data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                let data = if sem_utils::pycompat::fmt::truthy(&data) {
                    data
                } else {
                    json!({})
                };
                let Value::Object(data) = data else {
                    return Err(format!(
                        "'{}' object has no attribute 'get'",
                        sem_utils::pycompat::fmt::type_name(&data)
                    ));
                };
                update(
                    &mut state.configs,
                    Some(data.get("normalize").unwrap_or(&json!({}))),
                )?;
                update(
                    &mut state.method_configs,
                    Some(data.get("normalize_methods").unwrap_or(&json!({}))),
                )?;
            } else if config_file.ends_with(".toml") {
                let data: toml::Value = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
                let data = serde_json::to_value(data).map_err(|e| e.to_string())?;
                if let Value::Object(data) = data {
                    update(&mut state.configs, data.get("normalize"))?;
                    update(&mut state.method_configs, data.get("normalize_methods"))?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => self
                .logger
                .info(format!("Loaded normalize config from {config_file}")),
            Err(e) => self
                .logger
                .warning(format!("Failed to load config file {config_file}: {e}")),
        }
    }

    fn load_env_vars(&self) {
        let mut state = self.lock();
        for (env_key, config_key) in ENV_MAPPINGS {
            if let Ok(value) = std::env::var(env_key)
                && !value.is_empty()
            {
                state.configs.insert(config_key.to_string(), json!(value));
            }
        }
        for (key, value) in std::env::vars() {
            let Some(rest) = key.strip_prefix("NORMALIZE_") else {
                continue;
            };
            if ENV_MAPPINGS.iter().any(|(k, _)| *k == key) {
                continue;
            }
            let config_key = pystr::lower(rest);
            let lowered = pystr::lower(&value);
            let parsed = if lowered == "true" || lowered == "false" {
                json!(lowered == "true")
            } else if pystr::str_is_digit(&value) {
                match pystr::py_int(&value) {
                    Ok(i) => i64::try_from(i).map(|i| json!(i)).unwrap_or(json!(value)),
                    Err(_) => json!(value),
                }
            } else {
                match pystr::py_float(&value) {
                    Ok(f) => serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or(json!(value)),
                    Err(_) => json!(value),
                }
            };
            state.configs.insert(config_key, parsed);
        }
    }

    /// `set(key, value)`.
    pub fn set(&self, key: &str, value: impl Into<Value>) {
        self.lock().configs.insert(key.to_string(), value.into());
    }

    /// `get(key, default=None)`: the configured value, else `NORMALIZE_<KEY>` from the
    /// environment (converted like `default`: `int`/`bool` defaults parse an integer, `float`
    /// defaults a float; a failed conversion gives `default`), else `default`.
    pub fn get(&self, key: &str, default: Value) -> Value {
        if let Some(value) = self.lock().configs.get(key) {
            return value.clone();
        }
        let env_key = format!("NORMALIZE_{}", pystr::upper(key));
        let Ok(value) = std::env::var(&env_key) else {
            return default;
        };
        if value.is_empty() {
            return default;
        }
        match &default {
            // `isinstance(default, int)` is also true for bools, so they parse as ints too.
            Value::Bool(_) => match pystr::py_int(&value) {
                Ok(i) => i64::try_from(i).map(|i| json!(i)).unwrap_or(default),
                Err(_) => default,
            },
            Value::Number(n) if !n.is_f64() => match pystr::py_int(&value) {
                Ok(i) => i64::try_from(i).map(|i| json!(i)).unwrap_or(default),
                Err(_) => default,
            },
            Value::Number(_) => match pystr::py_float(&value) {
                Ok(f) => serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(default),
                Err(_) => default,
            },
            _ => json!(value),
        }
    }

    /// `get(key)` with `default=None`.
    pub fn get_value(&self, key: &str) -> Option<Value> {
        match self.get(key, Value::Null) {
            Value::Null => None,
            v => Some(v),
        }
    }

    /// `set_method_config(method, **config)`.
    pub fn set_method_config(&self, method: &str, config: Map<String, Value>) {
        self.lock()
            .method_configs
            .insert(method.to_string(), Value::Object(config));
    }

    /// `get_method_config(method)`: a copy of the method's configuration (`{}` when unset).
    pub fn get_method_config(&self, method: &str) -> Map<String, Value> {
        match self.lock().method_configs.get(method) {
            Some(Value::Object(map)) => map.clone(),
            _ => Map::new(),
        }
    }

    /// What `config = get_method_config(method); config.update(kwargs)` does in the
    /// convenience functions: Python gets the stored dict itself, so the keyword arguments
    /// persist in a method configuration that exists. Returns the merged configuration.
    pub(crate) fn merge_method_config(
        &self,
        method: &str,
        kwargs: &Map<String, Value>,
    ) -> Map<String, Value> {
        let mut state = self.lock();
        match state.method_configs.get_mut(method) {
            Some(Value::Object(stored)) => {
                for (k, v) in kwargs {
                    stored.insert(k.clone(), v.clone());
                }
                stored.clone()
            }
            _ => kwargs.clone(),
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

    /// The logger (`semantica.normalize_config`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

/// `normalize_config`: the global configuration instance (created on first use).
#[allow(non_upper_case_globals)]
pub static normalize_config: Lazy<NormalizeConfig> = Lazy::new(|| NormalizeConfig::new(None));
