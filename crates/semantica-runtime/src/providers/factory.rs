//! Original dynamic provider selection/reuse, borrowing the caller's one state.
//! No global pool, credentials, call ledger, graph or inference engine.
use semantica_contracts::*;
use serde_json::Value;
use std::{cell::RefCell, fmt::Write};
#[path = "python_repr_data.rs"]
mod python_repr_data;

pub const BUILTINS: &[&str] = &[
    "openai",
    "gemini",
    "groq",
    "anthropic",
    "ollama",
    "huggingface_llm",
    "deepseek",
    "novita",
];
/// Original constructor defaults only; no SDK initialization or inference.
pub fn builtin_default_model(backend: &str) -> Option<&'static str> {
    match backend {
        "openai" => Some("gpt-3.5-turbo"),
        "gemini" => Some("gemini-pro"),
        "groq" => Some("llama-3.3-70b-versatile"),
        "anthropic" => Some("claude-3-sonnet-20240229"),
        "ollama" => Some("llama2"),
        "huggingface_llm" => Some("gpt2"),
        "deepseek" => Some("deepseek-chat"),
        "novita" => Some("deepseek/deepseek-v3.2"),
        _ => None,
    }
}
pub fn failure(code: &str, phase: &str, exception: &str) -> SemanticProviderFactoryFailure {
    SemanticProviderFactoryFailure {
        code: code.into(),
        phase: phase.into(),
        python_exception: exception.into(),
    }
}
pub fn empty_state() -> SemanticProviderFactoryState {
    SemanticProviderFactoryState {
        schema: "semantica.provider-factory-state/1".into(),
        bindings: vec![],
        cache: vec![],
        next_instance: 1,
    }
}
fn invalid_value() -> SemanticProviderFactoryFailure {
    failure("factory_typed_value_invalid", "validate", "TypeError")
}

fn string_repr(text: &str, out: &mut String) {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    out.push(quote);
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => {
                let codepoint = c as u32;
                let ranges = python_repr_data::NONPRINTABLE;
                let index = ranges.partition_point(|(start, _)| *start <= codepoint);
                let nonprintable = index > 0 && ranges[index - 1].1 >= codepoint;
                if !nonprintable {
                    out.push(c);
                } else if codepoint <= 0xff {
                    write!(out, "\\x{codepoint:02x}").expect("String write");
                } else if codepoint <= 0xffff {
                    write!(out, "\\u{codepoint:04x}").expect("String write");
                } else {
                    write!(out, "\\U{codepoint:08x}").expect("String write");
                }
            }
        }
    }
    out.push(quote);
}
fn scalar_repr(value: &Value, out: &mut String) -> Result<(), SemanticProviderFactoryFailure> {
    match value {
        Value::Null => out.push_str("None"),
        Value::Bool(true) => out.push_str("True"),
        Value::Bool(false) => out.push_str("False"),
        Value::String(text) => string_repr(text, out),
        Value::Number(_) => {
            // The neutral owner already implements finite Python numeric
            // rendering. Repr business syntax is not another JSON writer.
            let bytes = semantica_contracts::json::canonical_json(value).map_err(|_| {
                failure(
                    "factory_nonfinite_value_uncovered",
                    "validate",
                    "ValueError",
                )
            })?;
            out.push_str(std::str::from_utf8(&bytes).expect("numeric ASCII"));
        }
        _ => return Err(invalid_value()),
    }
    Ok(())
}
fn sequence_repr(
    items: &[Value],
    profile: &str,
    tuple: bool,
    normalize_children: bool,
    depth: usize,
    out: &mut String,
) -> Result<(), SemanticProviderFactoryFailure> {
    out.push(if tuple { '(' } else { '[' });
    for (index, value) in items.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        value_repr(value, profile, normalize_children, depth + 1, out)?;
    }
    if tuple && items.len() == 1 {
        out.push(',');
    }
    out.push(if tuple { ')' } else { ']' });
    Ok(())
}
fn dict_repr(
    items: &serde_json::Map<String, Value>,
    profile: &str,
    normalize: bool,
    depth: usize,
    out: &mut String,
) -> Result<(), SemanticProviderFactoryFailure> {
    let mut fields: Vec<_> = items.iter().collect();
    if normalize {
        fields.sort_by(|a, b| a.0.cmp(b.0));
    }
    out.push(if normalize { '(' } else { '{' });
    for (index, (key, value)) in fields.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        if normalize {
            out.push('(');
        }
        string_repr(key, out);
        out.push_str(if normalize { ", " } else { ": " });
        value_repr(value, profile, normalize, depth + 1, out)?;
        if normalize {
            out.push(')');
        }
    }
    if normalize && fields.len() == 1 {
        out.push(',');
    }
    out.push(if normalize { ')' } else { '}' });
    Ok(())
}
/// make_hashable recursively transforms dict/list, but returns existing tuples
/// unchanged. Nested lists/dicts in tuples therefore retain raw repr and order.
fn value_repr(
    value: &Value,
    profile: &str,
    normalize: bool,
    depth: usize,
    out: &mut String,
) -> Result<(), SemanticProviderFactoryFailure> {
    if depth > 128 {
        return Err(failure(
            "factory_config_depth_exceeded",
            "validate",
            "ValueError",
        ));
    }
    if profile == "python-values/1" {
        match value
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(invalid_value)?
        {
            "list" => sequence_repr(
                value
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or_else(invalid_value)?,
                profile,
                normalize,
                normalize,
                depth,
                out,
            )?,
            "tuple" => sequence_repr(
                value
                    .get("items")
                    .and_then(Value::as_array)
                    .ok_or_else(invalid_value)?,
                profile,
                true,
                false,
                depth,
                out,
            )?,
            "dict" => dict_repr(
                value
                    .get("items")
                    .and_then(Value::as_object)
                    .ok_or_else(invalid_value)?,
                profile,
                normalize,
                depth,
                out,
            )?,
            "scalar" => scalar_repr(value.get("value").ok_or_else(invalid_value)?, out)?,
            _ => {
                return Err(failure(
                    "factory_python_value_uncovered",
                    "validate",
                    "TypeError",
                ));
            }
        }
    } else {
        match value {
            Value::Array(items) => sequence_repr(items, profile, normalize, normalize, depth, out)?,
            Value::Object(items) => dict_repr(items, profile, normalize, depth, out)?,
            _ => scalar_repr(value, out)?,
        }
    }
    if out.len() > super::MAX_PROVIDER_BYTES {
        return Err(failure(
            "factory_config_too_large",
            "validate",
            "ValueError",
        ));
    }
    Ok(())
}
/// Hash of the original str(tuple(key_parts)), not a tagged JSON identity. All
/// kwargs and exact name case affect reuse. Raw repr/API keys are never saved.
pub fn configuration_key(
    name: &Value,
    kwargs: &Value,
    profile: &str,
) -> Result<String, SemanticProviderFactoryFailure> {
    if !matches!(profile, "json/1" | "python-values/1") {
        return Err(failure(
            "factory_profile_uncovered",
            "validate",
            "ValueError",
        ));
    }
    let fields = kwargs
        .as_object()
        .ok_or_else(|| failure("factory_kwargs_invalid", "validate", "TypeError"))?;
    let mut fields: Vec<_> = fields.iter().collect();
    fields.sort_by(|a, b| a.0.cmp(b.0));
    let mut repr = String::from("(");
    value_repr(name, "json/1", false, 0, &mut repr)?;
    for (key, value) in &fields {
        repr.push_str(", (");
        string_repr(key, &mut repr);
        repr.push_str(", ");
        value_repr(value, profile, true, 0, &mut repr)?;
        repr.push(')');
    }
    if fields.is_empty() {
        repr.push(',');
    }
    repr.push(')');
    if repr.len() > super::MAX_PROVIDER_BYTES {
        return Err(failure(
            "factory_config_too_large",
            "validate",
            "ValueError",
        ));
    }
    Ok(crate::sha256(repr.as_bytes()))
}
fn validate_name(name: &Value, phase: &str) -> Result<(), SemanticProviderFactoryFailure> {
    if matches!(name, Value::Array(_) | Value::Object(_)) {
        return Err(failure("factory_unhashable_name", phase, "TypeError"));
    }
    scalar_repr(name, &mut String::new())
}
fn numeric_identity(value: &Value) -> Option<String> {
    match value {
        Value::Bool(v) => Some(if *v { "1" } else { "0" }.into()),
        Value::Number(number) => {
            let text = number.to_string();
            if text.contains(['.', 'e', 'E']) {
                let v = number.as_f64().filter(|v| v.is_finite())?;
                if v.fract() == 0.0 {
                    Some(if v == 0.0 {
                        "0".into()
                    } else {
                        format!("{v:.0}")
                    })
                } else {
                    Some(format!("float:{:016x}", v.to_bits()))
                }
            } else {
                Some(if text == "-0" { "0".into() } else { text })
            }
        }
        _ => None,
    }
}
fn same_name(left: &Value, right: &Value) -> bool {
    left == right
        || matches!((numeric_identity(left),numeric_identity(right)),(Some(a),Some(b)) if a==b)
}
fn borrow_failure() -> SemanticProviderFactoryFailure {
    failure("factory_state_borrow_conflict", "state", "RuntimeError")
}

/// Borrow the caller's single RefCell. Borrow conflicts fail, without a panic,
/// second registry/cache, or mutable global state.
pub struct RustProviderFactory<'a> {
    state: &'a RefCell<SemanticProviderFactoryState>,
    constructors: &'a dyn SemanticProviderConstructionPort,
}
impl<'a> RustProviderFactory<'a> {
    pub fn new(
        state: &'a RefCell<SemanticProviderFactoryState>,
        constructors: &'a dyn SemanticProviderConstructionPort,
    ) -> Self {
        Self {
            state,
            constructors,
        }
    }
    pub fn register(&self, name: &Value, id: &str) -> Result<(), SemanticProviderFactoryFailure> {
        validate_name(name, "register")?;
        let mut state = self.state.try_borrow_mut().map_err(|_| borrow_failure())?;
        if let Some(old) = state.bindings.iter_mut().find(|x| same_name(&x.name, name)) {
            old.constructor_id = id.into();
        } else {
            state.bindings.push(SemanticProviderBinding {
                name: name.clone(),
                constructor_id: id.into(),
            });
        }
        Ok(())
    }
    pub fn get_binding(
        &self,
        name: &Value,
    ) -> Result<Option<SemanticProviderBinding>, SemanticProviderFactoryFailure> {
        validate_name(name, "lookup")?;
        let state = self.state.try_borrow().map_err(|_| borrow_failure())?;
        Ok(state
            .bindings
            .iter()
            .find(|x| same_name(&x.name, name))
            .cloned())
    }
    pub fn list_all(&self) -> Result<Vec<Value>, SemanticProviderFactoryFailure> {
        let state = self.state.try_borrow().map_err(|_| borrow_failure())?;
        Ok(state.bindings.iter().map(|x| x.name.clone()).collect())
    }
    pub fn unregister(&self, name: &Value) -> Result<(), SemanticProviderFactoryFailure> {
        validate_name(name, "unregister")?;
        self.state
            .try_borrow_mut()
            .map_err(|_| borrow_failure())?
            .bindings
            .retain(|x| !same_name(&x.name, name));
        Ok(()) // cache intentionally survives
    }
    pub fn clear(&self) -> Result<(), SemanticProviderFactoryFailure> {
        self.state
            .try_borrow_mut()
            .map_err(|_| borrow_failure())?
            .cache
            .clear();
        Ok(())
    }
    pub fn pool_get(
        &self,
        name: &Value,
        kwargs: &Value,
        profile: &str,
    ) -> Result<(SemanticProviderInstance, bool), SemanticProviderFactoryFailure> {
        self.create_provider(name, kwargs, profile, true)
    }
    pub fn create_provider(
        &self,
        name: &Value,
        kwargs: &Value,
        profile: &str,
        use_pool: bool,
    ) -> Result<(SemanticProviderInstance, bool), SemanticProviderFactoryFailure> {
        let key = configuration_key(name, kwargs, profile)?;
        let mut state = self.state.try_borrow_mut().map_err(|_| borrow_failure())?;
        if use_pool && let Some(entry) = state.cache.iter().find(|x| x.key_sha256 == key) {
            return Ok((entry.instance.clone(), true));
        }
        validate_name(name, "lookup")?;
        let binding = state.bindings.iter().find(|x| same_name(&x.name, name));
        let (backend, constructor) = if let Some(binding) = binding {
            ("custom".to_owned(), Some(binding.constructor_id.clone()))
        } else {
            let name = name.as_str().ok_or_else(|| {
                failure("factory_name_missing_lower", "construct", "AttributeError")
            })?;
            let lower = name.to_lowercase();
            if !BUILTINS.contains(&lower.as_str()) {
                return Err(failure(
                    "factory_unknown_provider",
                    "construct",
                    "ValueError",
                ));
            }
            (lower, None)
        };
        let next = state
            .next_instance
            .checked_add(1)
            .ok_or_else(|| failure("factory_identity_exhausted", "construct", "ValueError"))?;
        let instance_id = format!("provider-instance:{}", state.next_instance);
        let input = SemanticProviderConstructionRequest {
            name: name.clone(),
            backend: backend.clone(),
            constructor_id: constructor.clone(),
            kwargs: kwargs.clone(),
            kwargs_profile: profile.into(),
            instance_id: instance_id.clone(),
            configuration_sha256: key.clone(),
        };
        let instance = self.constructors.construct(&input)?;
        if instance.validate().is_err()
            || instance.instance_id != instance_id
            || instance.configuration_sha256 != key
            || instance.provider_name != *name
            || instance.backend != backend
            || instance.constructor_id != constructor
        {
            return Err(failure(
                "factory_constructor_identity_invalid",
                "construct",
                "ValueError",
            ));
        }
        state.next_instance = next;
        if use_pool {
            state.cache.push(SemanticProviderCacheEntry {
                key_sha256: key,
                instance: instance.clone(),
            });
        }
        Ok((instance, false))
    }
}
impl SemanticProviderFactoryPort for RustProviderFactory<'_> {
    fn apply(
        &self,
        input: &SemanticProviderFactoryRequest,
    ) -> Result<SemanticProviderFactoryResult, SemanticProviderFactoryFailure> {
        input
            .validate()
            .map_err(|_| failure("factory_request_invalid", "validate", "ValueError"))?;
        let mut out = SemanticProviderFactoryResult {
            schema: "semantica.provider-factory-result/1".into(),
            operation: input.operation.clone(),
            instance: None,
            binding: None,
            names: vec![],
            cache_hit: false,
        };
        match input.operation.as_str() {
            "register" => self.register(
                &input.name,
                input.constructor_id.as_deref().ok_or_else(|| {
                    failure("factory_constructor_missing", "register", "TypeError")
                })?,
            )?,
            "get" => out.binding = self.get_binding(&input.name)?,
            "list_all" => out.names = self.list_all()?,
            "unregister" => self.unregister(&input.name)?,
            "clear" => self.clear()?,
            "create_provider" => {
                let (instance, hit) = self.create_provider(
                    &input.name,
                    &input.kwargs,
                    &input.kwargs_profile,
                    input.use_pool,
                )?;
                out.instance = Some(instance);
                out.cache_hit = hit;
            }
            _ => {
                return Err(failure(
                    "factory_operation_invalid",
                    "validate",
                    "ValueError",
                ));
            }
        }
        Ok(out)
    }
}
