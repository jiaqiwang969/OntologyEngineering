//! Configuration management for the parse module (`semantica/parse/config.py`).
//!
//! [`ParseConfig`] merges a config file (YAML, JSON or TOML: the `parse` and
//! `parse_methods` sections), `PARSE_*` environment variables and programmatic settings.
//! [`parse_config`] is the global instance, created (and reading the environment) on first
//! use, as the original creates it at import time.

use std::path::Path;
use std::sync::Mutex;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use serde_json::{Map, Value};

use sem_utils::logging::{Logger, get_logger};

use crate::common::{DecodeErrors, Options, py_exists, py_read_text};

#[derive(Debug, Default)]
struct ConfigState {
    configs: Map<String, Value>,
    method_configs: IndexMap<String, Value>,
}

/// `ParseConfig(config_file=None)`: configuration manager for the parse module.
#[derive(Debug)]
pub struct ParseConfig {
    /// `self.logger` (`semantica.parse_config`).
    pub logger: Logger,
    state: Mutex<ConfigState>,
}

/// The `PARSE_*` variables with a fixed key and type (`env_mappings`).
const ENV_MAPPINGS: &[(&str, &str, bool)] = &[
    ("PARSE_DEFAULT_ENCODING", "default_encoding", false),
    ("PARSE_OCR_LANGUAGE", "ocr_language", false),
    ("PARSE_EXTRACT_TABLES", "extract_tables", true),
    ("PARSE_EXTRACT_IMAGES", "extract_images", true),
    ("PARSE_EXTRACT_METADATA", "extract_metadata", true),
    ("PARSE_JS_RENDERING", "js_rendering", true),
];

impl ParseConfig {
    /// `ParseConfig(config_file)`: load the config file (if it exists), then the
    /// environment.
    pub fn new(config_file: Option<&str>) -> Self {
        Self::with_environment(config_file, std::env::vars().collect())
    }

    /// `ParseConfig(config_file)` reading `environ` instead of the process environment
    /// (the original reads `os.environ`; this makes the environment explicit for tests).
    pub fn with_environment(config_file: Option<&str>, environ: Vec<(String, String)>) -> Self {
        let config = ParseConfig {
            logger: get_logger("parse_config"),
            state: Mutex::new(ConfigState::default()),
        };
        config.load_config_file(config_file);
        config.load_env_vars(&environ);
        config
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ConfigState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn load_config_file(&self, config_file: Option<&str>) {
        let Some(config_file) = config_file.filter(|f| !f.is_empty()) else {
            return;
        };
        // `Path(config_file).exists()` (an OSError here propagates in Python; a config file
        // name long enough to raise is treated as missing).
        if !py_exists(Path::new(config_file)).unwrap_or(false) {
            return;
        }
        let result: Result<(), String> = (|| {
            let is_yaml = config_file.ends_with(".yaml") || config_file.ends_with(".yml");
            let is_json = config_file.ends_with(".json");
            let is_toml = config_file.ends_with(".toml");
            if !(is_yaml || is_json || is_toml) {
                return Ok(());
            }
            let text = py_read_text(Path::new(config_file), "utf-8", DecodeErrors::Strict)
                .map_err(|e| e.to_string())?;
            if is_yaml {
                let data = crate::structured_data_parser::yaml_safe_load(&text)
                    .map_err(|e| e.to_string())?;
                // `yaml.safe_load(f) or {}`
                let data = if sem_utils::pycompat::fmt::truthy(&data) {
                    data
                } else {
                    Value::Object(Map::new())
                };
                self.update_from_mapping(&data, true)
            } else if is_json {
                let data = crate::json_parser::py_json_loads(&text).map_err(|e| e.to_string())?;
                let data = if sem_utils::pycompat::fmt::truthy(&data) {
                    data
                } else {
                    Value::Object(Map::new())
                };
                self.update_from_mapping(&data, true)
            } else {
                let data = toml_loads(&text)?;
                self.update_from_mapping(&data, false)
            }
        })();
        match result {
            Ok(()) => self
                .logger
                .info(format!("Loaded parse config from {config_file}")),
            Err(e) => self
                .logger
                .warning(format!("Failed to load config file {config_file}: {e}")),
        }
    }

    /// `self._configs.update(data.get("parse", {}))` and the same for `parse_methods`; with
    /// `via_get = false` the TOML form (`if "parse" in data: ...`).
    fn update_from_mapping(&self, data: &Value, via_get: bool) -> Result<(), String> {
        let Value::Object(map) = data else {
            return Err(format!(
                "'{}' object has no attribute {}",
                sem_utils::pycompat::fmt::type_name(data),
                if via_get { "'get'" } else { "'__contains__'" }
            ));
        };
        let empty = Value::Object(Map::new());
        let parse = map.get("parse");
        if via_get || parse.is_some() {
            let parse = parse.unwrap_or(&empty);
            let updates = dict_update_items(parse)?;
            let mut state = self.state();
            for (k, v) in updates {
                state.configs.insert(k, v);
            }
        }
        let methods = map.get("parse_methods");
        if via_get || methods.is_some() {
            let methods = methods.unwrap_or(&empty);
            let updates = dict_update_items(methods)?;
            let mut state = self.state();
            for (k, v) in updates {
                state.method_configs.insert(k, v);
            }
        }
        Ok(())
    }

    fn load_env_vars(&self, environ: &[(String, String)]) {
        // `os.environ` is a mapping: one value per name, in first-seen order.
        let mut env: IndexMap<&str, &str> = IndexMap::new();
        for (k, v) in environ {
            env.insert(k.as_str(), v.as_str());
        }
        let mut state = self.state();
        for (env_key, config_key, is_bool) in ENV_MAPPINGS {
            if let Some(value) = env.get(env_key).copied().filter(|v| !v.is_empty()) {
                let parsed = if *is_bool {
                    Value::Bool(matches!(
                        value.to_lowercase().as_str(),
                        "true" | "1" | "yes" | "on"
                    ))
                } else {
                    Value::String(value.to_string())
                };
                state.configs.insert(config_key.to_string(), parsed);
            }
        }
        for (key, value) in &env {
            if key.starts_with("PARSE_") && !ENV_MAPPINGS.iter().any(|(k, _, _)| k == key) {
                let config_key = key["PARSE_".len()..].to_lowercase();
                let lower = value.to_lowercase();
                let parsed = if lower == "true" || lower == "false" {
                    Value::Bool(lower == "true")
                } else if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
                    py_int(value).unwrap_or_else(|| Value::String(value.to_string()))
                } else {
                    match py_float(value) {
                        Some(f) => float_value(f),
                        None => Value::String(value.to_string()),
                    }
                };
                state.configs.insert(config_key, parsed);
            }
        }
    }

    /// `set(key, value)`.
    pub fn set(&self, key: &str, value: Value) {
        self.state().configs.insert(key.to_string(), value);
    }

    /// `get(key, default=None)`: the configured value, else `PARSE_<KEY>` from the
    /// environment converted like `default` (int, float, bool or str), else `default`.
    /// `Value::Null` stands for `None`.
    pub fn get(&self, key: &str, default: Value) -> Value {
        self.get_with_environment(key, default, |name| std::env::var(name).ok())
    }

    /// [`Self::get`] reading the environment through `getenv`.
    pub fn get_with_environment(
        &self,
        key: &str,
        default: Value,
        getenv: impl Fn(&str) -> Option<String>,
    ) -> Value {
        if let Some(v) = self.state().configs.get(key) {
            return v.clone();
        }
        let env_key = format!("PARSE_{}", key.to_uppercase());
        if let Some(value) = getenv(&env_key).filter(|v| !v.is_empty()) {
            // `isinstance(default, int)` is also true for bools, so a bool default goes
            // through `int(value)` (and falls back to the default when that fails).
            match &default {
                Value::Bool(_) => {
                    if let Some(v) = py_int(&value) {
                        return v;
                    }
                }
                Value::Number(n) if !n.is_f64() => {
                    if let Some(v) = py_int(&value) {
                        return v;
                    }
                }
                Value::Number(_) => {
                    if let Some(f) = py_float(&value) {
                        return float_value(f);
                    }
                }
                _ => return Value::String(value),
            }
        }
        default
    }

    /// `set_method_config(method, **config)`.
    pub fn set_method_config(&self, method: &str, config: Options) {
        self.state()
            .method_configs
            .insert(method.to_string(), Value::Object(config));
    }

    /// `get_method_config(method)`: the method's configuration (`{}` when unset). A
    /// non-mapping value loaded from a config file is returned as an empty map.
    pub fn get_method_config(&self, method: &str) -> Options {
        match self.state().method_configs.get(method) {
            Some(Value::Object(m)) => m.clone(),
            _ => Options::new(),
        }
    }

    /// `config = parse_config.get_method_config(method); config.update(kwargs)` as the
    /// convenience functions do it: when the method has a stored configuration, the
    /// original updates that stored dictionary in place (it is returned by reference), so
    /// the keywords persist for later calls. Returns the merged configuration.
    pub(crate) fn method_config_updated(&self, method: &str, kwargs: &Options) -> Options {
        let mut state = self.state();
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

    /// `get_all()`: copies of `{"config": ..., "method_configs": ...}`.
    pub fn get_all(&self) -> Map<String, Value> {
        let state = self.state();
        let mut out = Map::new();
        out.insert("config".to_string(), Value::Object(state.configs.clone()));
        out.insert(
            "method_configs".to_string(),
            Value::Object(
                state
                    .method_configs
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        );
        out
    }
}

/// The key/value pairs `dict.update(value)` would apply (a mapping, or an iterable of
/// pairs); other values raise like Python does.
fn dict_update_items(value: &Value) -> Result<Vec<(String, Value)>, String> {
    match value {
        Value::Object(map) => Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        Value::Array(items) => {
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                match item {
                    Value::Array(pair) if pair.len() == 2 => {
                        let key = match &pair[0] {
                            Value::String(s) => s.clone(),
                            other => sem_utils::pycompat::fmt::py_str(other),
                        };
                        out.push((key, pair[1].clone()));
                    }
                    Value::Array(pair) => {
                        return Err(format!(
                            "dictionary update sequence element #{i} has length {}; 2 is required",
                            pair.len()
                        ));
                    }
                    Value::String(s) if s.chars().count() == 2 => {
                        let mut chars = s.chars();
                        let k = chars.next().unwrap_or_default().to_string();
                        let v = chars.next().unwrap_or_default().to_string();
                        out.push((k, Value::String(v)));
                    }
                    Value::String(s) => {
                        return Err(format!(
                            "dictionary update sequence element #{i} has length {}; 2 is required",
                            s.chars().count()
                        ));
                    }
                    other => {
                        return Err(format!(
                            "cannot convert dictionary update sequence element #{i} to a sequence{}",
                            if matches!(other, Value::Null) { "" } else { "" }
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::Null => Err("'NoneType' object is not iterable".to_string()),
        Value::Bool(_) => Err("'bool' object is not iterable".to_string()),
        Value::Number(n) if n.is_f64() => Err("'float' object is not iterable".to_string()),
        Value::Number(_) => Err("'int' object is not iterable".to_string()),
        Value::String(s) if s.is_empty() => Ok(Vec::new()),
        Value::String(_) => {
            Err("dictionary update sequence element #0 has length 1; 2 is required".to_string())
        }
    }
}

/// `int(value)` for a string: optional sign, ASCII digits with single underscores between
/// them, surrounding whitespace allowed.
pub(crate) fn py_int(value: &str) -> Option<Value> {
    let s = value.trim_matches(|c: char| c.is_whitespace());
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    let clean: String = digits.chars().filter(|c| *c != '_').collect();
    let n: i128 = clean.parse().ok()?;
    Some(int_value(if neg { -n } else { n }))
}

fn int_value(n: i128) -> Value {
    if let Ok(v) = i64::try_from(n) {
        Value::from(v)
    } else if let Ok(v) = u64::try_from(n) {
        Value::from(v)
    } else {
        float_value(n as f64)
    }
}

fn float_value(f: f64) -> Value {
    serde_json::Number::from_f64(f)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// `float(value)` for a string: decimal/exponent forms, `inf`/`nan` spellings, underscores
/// between digits, surrounding whitespace.
pub(crate) fn py_float(value: &str) -> Option<f64> {
    let s = value.trim_matches(|c: char| c.is_whitespace());
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    let body = lower.trim_start_matches(['+', '-']);
    if lower.len() - body.len() > 1 {
        return None;
    }
    let neg = lower.starts_with('-');
    if body == "inf" || body == "infinity" {
        return Some(if neg {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        });
    }
    if body == "nan" {
        return Some(f64::NAN);
    }
    // Underscores are allowed only between digits.
    let bytes = body.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'_' {
            let prev = i.checked_sub(1).map(|j| bytes[j]);
            let next = bytes.get(i + 1).copied();
            if !(prev.is_some_and(|p| p.is_ascii_digit())
                && next.is_some_and(|n| n.is_ascii_digit()))
            {
                return None;
            }
        } else if !(b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'+' || b == b'-') {
            return None;
        }
    }
    let clean: String = lower.chars().filter(|c| *c != '_').collect();
    // Rust accepts forms Python rejects ("1e", ".", "e5"); check the Python grammar.
    let re = regex::Regex::new(r"^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$").expect("valid regex");
    if !re.is_match(&clean) {
        return None;
    }
    clean.parse().ok()
}

/// A TOML document as JSON-like data, keeping the document's key order (datetimes become
/// their TOML text).
fn toml_loads(text: &str) -> Result<Value, String> {
    let value: Value = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(fix_toml_datetimes(value))
}

fn fix_toml_datetimes(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            if map.len() == 1
                && let Some(Value::String(dt)) = map.get("$__toml_private_datetime")
            {
                return Value::String(dt.clone());
            }
            Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, fix_toml_datetimes(v)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(fix_toml_datetimes).collect()),
        other => other,
    }
}

/// `parse_config`: the global configuration instance.
#[allow(non_upper_case_globals)]
pub static parse_config: Lazy<ParseConfig> = Lazy::new(|| ParseConfig::new(None));
