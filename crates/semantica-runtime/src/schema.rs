//! Neutral model behavior. All fields, aliases/defaults, shapes and rules are
//! loaded from the genuine-ggen catalog; no Project or model budget lives here.
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use semantica_contracts::*;
use serde_json::{Map, Number, Value, json};
use std::collections::BTreeMap;
use std::str::FromStr;

const MAX_DEPTH: usize = 128;
const MAX_INPUT_BYTES: usize = 2 * 1024 * 1024;

pub struct RustSchemaRuntime {
    models: BTreeMap<String, SemanticModelDefinition>,
}
fn object_schema(model: &SemanticModelDefinition) -> Result<&Value, SemanticSchemaFailure> {
    if let Some(reference) = model.json_schema["$ref"].as_str() {
        let pointer = reference
            .strip_prefix('#')
            .ok_or_else(|| failure("schema_external_reference_unknown"))?;
        model
            .json_schema
            .pointer(pointer)
            .ok_or_else(|| failure("schema_root_reference_unknown"))
    } else {
        Ok(&model.json_schema)
    }
}
fn failure(code: &str) -> SemanticSchemaFailure {
    SemanticSchemaFailure {
        code: code.into(),
        phase: "schema".into(),
        message: code.into(),
        retryable: false,
    }
}
fn issue(issues: &mut Vec<Value>, kind: &str, loc: &[Value]) {
    let message = match kind {
        "missing" => "Field required",
        "string_type" => "Input should be a valid string",
        "int_type" => "Input should be a valid integer",
        "int_parsing" => "Input should be a valid integer, unable to parse string as an integer",
        "int_parsing_size" => "Unable to parse input string as an integer, exceeded maximum size",
        "int_from_float" => "Input should be a valid integer, got a number with a fractional part",
        "float_type" => "Input should be a valid number",
        "float_parsing" => "Input should be a valid number, unable to parse string as a number",
        "bool_type" => "Input should be a valid boolean",
        "bool_parsing" => "Input should be a valid boolean, unable to interpret input",
        "list_type" => "Input should be a valid list",
        "tuple_type" => "Input should be a valid tuple",
        "dict_type" => "Input should be a valid dictionary",
        "datetime_type" => "Input should be a valid datetime",
        "none_required" => "Input should be None",
        "extra_forbidden" => "Extra inputs are not permitted",
        _ => kind,
    };
    issues.push(json!({"type":kind,"kind":kind,"loc":loc,"location":loc,"message":message}));
}
fn issue_message(issues: &mut Vec<Value>, kind: &str, loc: &[Value], message: String) {
    issue(issues, kind, loc);
    if let Some(last) = issues.last_mut() {
        last["message"] = json!(message);
    }
}
fn python_repr(value: &Value) -> Result<String, SemanticSchemaFailure> {
    semantica_contracts::json::python_repr(value)
        .map_err(|_| failure("schema_python_representation_profile_unavailable"))
}
fn literal_message(values: &[Value]) -> Result<String, SemanticSchemaFailure> {
    let mut parts = values
        .iter()
        .map(python_repr)
        .collect::<Result<Vec<_>, _>>()?;
    let last = parts.pop().unwrap_or_default();
    Ok(format!(
        "Input should be {}",
        if parts.is_empty() {
            last
        } else {
            format!("{} or {}", parts.join(", "), last)
        }
    ))
}
fn integer(value: &Value) -> Option<Value> {
    if integer_string_too_large(value) {
        return None;
    }
    match value {
        Value::Bool(v) => Some(json!(if *v { 1 } else { 0 })),
        Value::Number(n) => {
            let s = n.to_string();
            if !s.contains(['.', 'e', 'E']) {
                return Some(value.clone());
            }
            let f = n.as_f64()?;
            if !f.is_finite() || f.fract() != 0. {
                return None;
            }
            Number::from_str(&format!("{f:.0}")).ok().map(Value::Number)
        }
        Value::String(s) => {
            let s = s.trim();
            let s = numeric_underscores(s)?;
            let (sign, digits) = if let Some(v) = s.strip_prefix('-') {
                ("-", v)
            } else {
                ("", s.strip_prefix('+').unwrap_or(&s))
            };
            let digits = if let Some((whole, fraction)) = digits.split_once('.') {
                if fraction.is_empty() || !fraction.bytes().all(|b| b == b'0') {
                    return None;
                }
                whole
            } else {
                digits
            };
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let digits = digits.trim_start_matches('0');
            let digits = if digits.is_empty() { "0" } else { digits };
            Number::from_str(&format!("{sign}{digits}"))
                .ok()
                .map(Value::Number)
        }
        _ => None,
    }
}
/// Reuse the original scalar integer parser for HTTP query strings. This is
/// parsing only; route-specific bounds and error locations belong to the router.
pub fn parse_query_integer(text: &str) -> Result<Value, &'static str> {
    let input = Value::String(text.into());
    if integer_string_too_large(&input) {
        return Err("int_parsing_size");
    }
    let value = integer(&input).ok_or("int_parsing")?;
    if value.as_number().is_some_and(|n| n.to_string() == "-0") {
        Ok(serde_json::json!(0))
    } else {
        Ok(value)
    }
}
fn integer_string_too_large(value: &Value) -> bool {
    let Some(text) = value.as_str().and_then(|s| numeric_underscores(s.trim())) else {
        return false;
    };
    let text = text.strip_prefix(['+', '-']).unwrap_or(&text);
    let digits = if let Some((whole, fraction)) = text.split_once('.') {
        if fraction.is_empty() || !fraction.bytes().all(|b| b == b'0') {
            return false;
        }
        whole
    } else {
        text
    };
    digits.bytes().all(|b| b.is_ascii_digit()) && digits.trim_start_matches('0').len() > 4300
}
fn integer_order(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
    fn parts(value: &Value) -> Option<(bool, String)> {
        let text = value.as_number()?.to_string();
        let negative = text.starts_with('-');
        let digits = text.strip_prefix('-').unwrap_or(&text);
        if !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let digits = digits.trim_start_matches('0');
        Some((
            negative && !digits.is_empty(),
            if digits.is_empty() {
                "0".into()
            } else {
                digits.into()
            },
        ))
    }
    let (left_negative, left) = parts(left)?;
    let (right_negative, right) = parts(right)?;
    Some(match (left_negative, right_negative) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => {
            let order = left.len().cmp(&right.len()).then_with(|| left.cmp(&right));
            if left_negative {
                order.reverse()
            } else {
                order
            }
        }
    })
}
fn datetime_string(value: &Value) -> Result<String, &'static str> {
    if value.is_boolean() || value.is_null() || value.is_array() || value.is_object() {
        return Err("datetime_type");
    }
    if let Some(text) = value.as_str() {
        if let Ok(date) = DateTime::parse_from_rfc3339(text) {
            let precision = if date.timestamp_subsec_micros() == 0 {
                SecondsFormat::Secs
            } else {
                SecondsFormat::Micros
            };
            return Ok(date.to_rfc3339_opts(precision, true));
        }
        for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
            if let Ok(date) = NaiveDateTime::parse_from_str(text, format) {
                return Ok(date
                    .format(if date.and_utc().timestamp_subsec_micros() == 0 {
                        "%Y-%m-%dT%H:%M:%S"
                    } else {
                        "%Y-%m-%dT%H:%M:%S%.6f"
                    })
                    .to_string());
            }
        }
        if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
            return Ok(format!("{}T00:00:00", date.format("%Y-%m-%d")));
        }
    }
    if let Some(mut number) = float(value).filter(|n| n.is_finite()) {
        if number.abs() > 20_000_000_000. {
            number /= 1000.;
        }
        let seconds = number.floor();
        let micros = ((number - seconds) * 1_000_000.).round() as i64;
        let seconds = seconds as i64 + micros / 1_000_000;
        let micros = (micros % 1_000_000) as u32;
        if let Some(date) = Utc
            .timestamp_opt(seconds, micros * 1000)
            .single()
            .filter(|d| (1..=9999).contains(&d.year()))
        {
            return Ok(date.to_rfc3339_opts(
                if micros == 0 {
                    SecondsFormat::Secs
                } else {
                    SecondsFormat::Micros
                },
                true,
            ));
        }
        return Err("datetime_parsing");
    }
    Err("datetime_from_date_parsing")
}
fn numeric_underscores(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'_'
            && (i == 0
                || i + 1 == bytes.len()
                || !bytes[i - 1].is_ascii_digit()
                || !bytes[i + 1].is_ascii_digit())
        {
            return None;
        }
    }
    Some(text.replace('_', ""))
}
fn float(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(v) => Some(if *v { 1. } else { 0. }),
        Value::Number(n) => n
            .as_f64()
            .filter(|x| n.to_string().contains(['.', 'e', 'E']) || x.is_finite()),
        Value::String(s) => numeric_underscores(s.trim())?.parse().ok(),
        _ => None,
    }
}
impl RustSchemaRuntime {
    pub fn new() -> Result<Self, SemanticSchemaFailure> {
        let models: Vec<SemanticModelDefinition> =
            serde_json::from_str(semantic_model_catalog_json())
                .map_err(|_| failure("schema_generated_catalog_invalid"))?;
        let mut owned = BTreeMap::new();
        for model in models {
            if model.validate().is_err() || owned.contains_key(&model.id) {
                return Err(failure("schema_generated_model_invalid"));
            }
            owned.insert(model.id.clone(), model);
        }
        Ok(Self { models: owned })
    }
    pub fn validate_typed<T: SemanticModelSnapshot>(
        &self,
        input: &Value,
    ) -> Result<T, SemanticSchemaFailure> {
        let model = self
            .models
            .get(T::MODEL_ID)
            .ok_or_else(|| failure("schema_model_unknown"))?;
        if model.source_sha256 != T::SOURCE_SHA256 || model.class_sha256 != T::CLASS_SHA256 {
            return Err(failure("schema_typed_source_mismatch"));
        }
        let result = self.validate_model(T::MODEL_ID, input)?;
        if result["status"] != "validated" {
            return Err(failure("schema_model_validation_failed"));
        }
        serde_json::from_value(result["output"].clone())
            .map_err(|_| failure("schema_typed_snapshot_unrepresentable"))
    }
    pub fn validate_snapshot(
        &self,
        id: &str,
        input: &Value,
    ) -> Result<SemanticRegisteredModelSnapshot, SemanticSchemaFailure> {
        let result = self.validate_model(id, input)?;
        if result["status"] != "validated" {
            return Err(failure("schema_model_validation_failed"));
        }
        self.normalized_snapshot(id, &result["output"])
    }
    fn normalized_snapshot(
        &self,
        id: &str,
        output: &Value,
    ) -> Result<SemanticRegisteredModelSnapshot, SemanticSchemaFailure> {
        let model = self
            .models
            .get(id)
            .ok_or_else(|| failure("schema_model_unknown"))?;
        let snapshot = semantic_model_snapshot_from_normalized_json(id, output.clone())
            .map_err(|_| failure("schema_typed_snapshot_unrepresentable"))?;
        let (source, class) = snapshot.source_identity();
        if source != model.source_sha256 || class != model.class_sha256 {
            return Err(failure("schema_typed_source_mismatch"));
        }
        Ok(snapshot)
    }
    pub fn model_ids(&self) -> impl Iterator<Item = &str> {
        self.models.keys().map(String::as_str)
    }
    /// Pinned manual repair prompt diagnostics. This is presentation of
    /// independently validated issues, never a source of model truth/adoption.
    pub fn validation_error_text(
        &self,
        id: &str,
        input: &Value,
    ) -> Result<String, SemanticProviderFailure> {
        let model = self
            .models
            .get(id)
            .ok_or_else(|| crate::providers::failure("typed_model_unknown", "validate"))?;
        let result = self
            .validate_model(id, input)
            .map_err(|x| crate::providers::failure(&x.code, "schema"))?;
        let issues = result["issues"]
            .as_array()
            .ok_or_else(|| crate::providers::failure("typed_schema_issues_invalid", "schema"))?;
        let mut summary = format!(
            "{} validation error{} for {}",
            issues.len(),
            if issues.len() == 1 { "" } else { "s" },
            model.name
        );
        for issue in issues {
            let loc = issue["loc"].as_array().ok_or_else(|| {
                crate::providers::failure("typed_schema_location_invalid", "schema")
            })?;
            let kind = issue["type"].as_str().unwrap_or("unknown");
            let mut value = &result["input_after"];
            let traverse = if kind == "missing" {
                &loc[..loc.len().saturating_sub(1)]
            } else {
                loc.as_slice()
            };
            for key in traverse {
                value = if let Some(index) = key.as_u64() {
                    value.get(index as usize)
                } else {
                    key.as_str().and_then(|key| value.get(key))
                }
                .ok_or_else(|| {
                    crate::providers::failure("typed_schema_diagnostic_input_unknown", "schema")
                })?;
            }
            let type_name = match value {
                Value::Null => "NoneType",
                Value::Bool(_) => "bool",
                Value::String(_) => "str",
                Value::Array(_) => "list",
                Value::Object(_) => "dict",
                Value::Number(n) => {
                    if n.to_string().contains(['.', 'e', 'E']) {
                        "float"
                    } else {
                        "int"
                    }
                }
            };
            if !loc.is_empty() {
                summary.push('\n');
                summary.push_str(
                    &loc.iter()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string())
                        })
                        .collect::<Vec<_>>()
                        .join("."),
                );
            }
            let message = if kind == "model_type" {
                format!(
                    "Input should be a valid dictionary or instance of {}",
                    model.name
                )
            } else {
                issue["message"].as_str().unwrap_or(kind).into()
            };
            summary.push_str(&format!("\n  {} [type={}, input_value={}, input_type={}]\n    For further information visit https://errors.pydantic.dev/2.13/v/{}",message,kind,python_repr(value).map_err(|x| crate::providers::failure(&x.code,"schema"))?,type_name,kind));
        }
        Ok(summary)
    }
    pub fn model_json_schema(&self, id: &str) -> Result<Value, SemanticSchemaFailure> {
        self.models
            .get(id)
            .map(|m| m.json_schema.clone())
            .ok_or_else(|| failure("schema_model_unknown"))
    }
    pub fn validate_constructor(
        &self,
        id: &str,
        input: &Value,
    ) -> Result<Value, SemanticSchemaFailure> {
        let mut result = self.evaluate_model(id, input, false)?;
        if let (Some(before), Some(after)) = (input.as_object(), result["input_after"].as_object())
        {
            let values = before
                .keys()
                .map(|key| {
                    (
                        key.clone(),
                        after
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| before[key].clone()),
                    )
                })
                .collect::<Map<_, _>>();
            result["input_after"] = Value::Object(values);
            result["input_mutated"] = json!(result["input_after"] != *input);
        }
        Ok(result)
    }
    /// Execution-only projection for the original search route's unused float.
    /// Raw input remains in its generated request contract. This is NOT a full
    /// normalized SearchRequest snapshot, and never relaxes validate_model.
    pub(crate) fn validate_search_execution_fields(
        &self,
        input: &Value,
    ) -> Result<Value, SemanticSchemaFailure> {
        const ID: &str = "semantica.explorer.schemas::SearchRequest";
        let mut projected = input.clone();
        let unused = input
            .get("min_semantic_similarity")
            .cloned()
            .unwrap_or(Value::Null);
        if float(&unused).is_some_and(|v| !v.is_finite()) {
            let model = self
                .models
                .get(ID)
                .ok_or_else(|| failure("schema_model_unknown"))?;
            // The locked original class has an unconstrained Optional[float].
            // A changed class must be reviewed, not silently retain this omission.
            if model.class_sha256
                != "f815ca7e7c4a9d08cc4bc60a496ab8ae3173ce1bb0b06c07a7f1767292568e30"
            {
                return Err(failure("schema_search_execution_class_changed"));
            }
            projected["min_semantic_similarity"] = Value::Null;
        }
        let mut result = self.validate_model(ID, &projected)?;
        if let Some(output) = result.get_mut("output").and_then(Value::as_object_mut) {
            output.remove("min_semantic_similarity");
        }
        result["unused_min_semantic_similarity_input"] = unused;
        result["projection"] = json!("semantica.search-execution-fields/1");
        Ok(result)
    }
    pub fn validate_model(&self, id: &str, input: &Value) -> Result<Value, SemanticSchemaFailure> {
        self.evaluate_model(id, input, false)
    }
    /// Like original model_construct, this preserves defaults but does not run
    /// validators/coercion. It never represents successful validation.
    pub fn construct_model(&self, id: &str, input: &Value) -> Result<Value, SemanticSchemaFailure> {
        self.evaluate_model(id, input, true)
    }
    pub fn property_on_output(
        &self,
        id: &str,
        output: &Value,
        name: &str,
    ) -> Result<Value, SemanticSchemaFailure> {
        let model = self
            .models
            .get(id)
            .ok_or_else(|| failure("schema_model_unknown"))?;
        let field = model
            .properties
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| failure("schema_property_unknown"))?;
        output
            .get(field)
            .cloned()
            .ok_or_else(|| failure("schema_property_field_missing"))
    }
    pub fn model_property(
        &self,
        id: &str,
        input: &Value,
        name: &str,
    ) -> Result<Value, SemanticSchemaFailure> {
        let result = self.validate_model(id, input)?;
        result["properties"]
            .get(name)
            .cloned()
            .ok_or_else(|| failure("schema_property_unknown"))
    }
    fn evaluate_model(
        &self,
        id: &str,
        input: &Value,
        construct: bool,
    ) -> Result<Value, SemanticSchemaFailure> {
        let model = self
            .models
            .get(id)
            .ok_or_else(|| failure("schema_model_unknown"))?;
        let mut source = input.clone();
        let mut issues = vec![];
        let raw_output = self.model(model, &mut source, &mut issues, &[], 0, construct)?;
        let output = if construct {
            self.construct_dump(model, &raw_output)?
        } else {
            raw_output.clone()
        };
        let by_alias = if construct {
            let mut aliases = Map::new();
            for field in &model.fields {
                if let Some(value) = output.get(&field.name) {
                    aliases.insert(
                        field.alias.clone().unwrap_or_else(|| field.name.clone()),
                        value.clone(),
                    );
                }
            }
            Value::Object(aliases)
        } else {
            self.alias_dump(model, &output)?
        };
        let properties = model
            .properties
            .as_object()
            .ok_or_else(|| failure("schema_properties_invalid"))?
            .iter()
            .filter_map(|(name, field)| {
                output
                    .get(field.as_str()?)
                    .map(|v| (name.clone(), v.clone()))
            })
            .collect::<Map<_, _>>();
        Ok(
            json!({"status":if issues.is_empty() {if construct {"constructed"} else {"validated"}}else{"error"},"validation_executed":!construct,
            "output":output,"output_by_alias":by_alias,"properties":properties,"issues":issues,
            "input_after":source,"input_mutated":source!=*input}),
        )
    }
    fn construct_dump(
        &self,
        model: &SemanticModelDefinition,
        value: &Value,
    ) -> Result<Value, SemanticSchemaFailure> {
        let Some(object) = value.as_object() else {
            return Ok(value.clone());
        };
        let mut output = object.clone();
        for field in &model.fields {
            if let Some(value) = object.get(&field.name) {
                let shape = &object_schema(model)?["properties"]
                    [field.alias.as_deref().unwrap_or(&field.name)];
                output.insert(field.name.clone(), Self::construct_dump_shape(shape, value));
            }
        }
        Ok(Value::Object(output))
    }
    fn construct_dump_shape(shape: &Value, value: &Value) -> Value {
        if shape["type"] == "number" && value.is_number() {
            return value
                .as_f64()
                .filter(|n| n.is_finite())
                .map_or_else(|| value.clone(), |n| json!(n));
        }
        if let Some(alternatives) = shape["anyOf"].as_array() {
            if !value.is_null() {
                if let Some(s) = alternatives.iter().find(|s| s["type"] != "null") {
                    return Self::construct_dump_shape(s, value);
                }
            }
        }
        if shape.get("$ref").is_none() {
            if let Some(items) = value.as_array() {
                return Value::Array(
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            Self::construct_dump_shape(
                                shape["prefixItems"].get(i).unwrap_or(&shape["items"]),
                                v,
                            )
                        })
                        .collect(),
                );
            }
            if let (Some(object), Some(sub)) = (
                value.as_object(),
                shape.get("additionalProperties").filter(|x| x.is_object()),
            ) {
                return Value::Object(
                    object
                        .iter()
                        .map(|(k, v)| (k.clone(), Self::construct_dump_shape(sub, v)))
                        .collect(),
                );
            }
        }
        value.clone()
    }
    fn alias_dump(
        &self,
        model: &SemanticModelDefinition,
        output: &Value,
    ) -> Result<Value, SemanticSchemaFailure> {
        let Some(object) = output.as_object() else {
            return Ok(output.clone());
        };
        let mut aliases = Map::new();
        for field in &model.fields {
            if let Some(value) = object.get(&field.name) {
                let shape = &object_schema(model)?["properties"]
                    [field.alias.as_deref().unwrap_or(&field.name)];
                let value = self.alias_shape(model, shape, value)?;
                aliases.insert(
                    field.alias.clone().unwrap_or_else(|| field.name.clone()),
                    value,
                );
            }
        }
        Ok(Value::Object(aliases))
    }
    fn alias_shape(
        &self,
        owner: &SemanticModelDefinition,
        shape: &Value,
        value: &Value,
    ) -> Result<Value, SemanticSchemaFailure> {
        if let Some(reference) = shape["$ref"].as_str() {
            let target = self.reference(owner, reference)?;
            return self.alias_dump(target, value);
        }
        if let Some(alternatives) = shape["anyOf"].as_array() {
            if value.is_null() {
                return Ok(Value::Null);
            }
            if let Some(shape) = alternatives.iter().find(|s| s["type"] != "null") {
                return self.alias_shape(owner, shape, value);
            }
        }
        if let Some(values) = value.as_array() {
            return Ok(Value::Array(
                values
                    .iter()
                    .map(|v| self.alias_shape(owner, &shape["items"], v))
                    .collect::<Result<_, _>>()?,
            ));
        }
        Ok(value.clone())
    }
    fn reference<'a>(
        &'a self,
        owner: &SemanticModelDefinition,
        reference: &str,
    ) -> Result<&'a SemanticModelDefinition, SemanticSchemaFailure> {
        let name = reference
            .rsplit('/')
            .next()
            .ok_or_else(|| failure("schema_reference_unknown"))?;
        let namespace = owner
            .id
            .split("::")
            .next()
            .ok_or_else(|| failure("schema_namespace_invalid"))?;
        self.models
            .get(&format!("{namespace}::{name}"))
            .ok_or_else(|| failure("schema_reference_unknown"))
    }
    fn model(
        &self,
        model: &SemanticModelDefinition,
        input: &mut Value,
        issues: &mut Vec<Value>,
        loc: &[Value],
        depth: usize,
        construct: bool,
    ) -> Result<Value, SemanticSchemaFailure> {
        if depth > MAX_DEPTH {
            return Err(failure("schema_depth_profile_exceeded"));
        }
        let Some(object) = input.as_object_mut() else {
            issue_message(
                issues,
                "model_type",
                loc,
                format!(
                    "Input should be a valid dictionary or instance of {}",
                    model.name
                ),
            );
            return Ok(Value::Null);
        };
        if !construct {
            for validator in &model.validators {
                if validator != "aliases_before" {
                    return Err(failure("schema_validator_unknown"));
                }
                for (target, sources) in model
                    .aliases
                    .as_object()
                    .ok_or_else(|| failure("schema_aliases_invalid"))?
                {
                    if !object.contains_key(target) {
                        for source in sources
                            .as_array()
                            .ok_or_else(|| failure("schema_aliases_invalid"))?
                        {
                            if let Some(value) = object
                                .get(
                                    source
                                        .as_str()
                                        .ok_or_else(|| failure("schema_aliases_invalid"))?,
                                )
                                .cloned()
                            {
                                object.insert(target.clone(), value);
                                break;
                            }
                        }
                    }
                }
            }
        }
        let mut output = Map::new();
        for field in &model.fields {
            let key = field
                .alias
                .as_deref()
                .filter(|a| object.contains_key(*a))
                .unwrap_or(&field.name);
            let mut field_loc = loc.to_vec();
            field_loc.push(json!(field.alias.as_deref().unwrap_or(&field.name)));
            if let Some(value) = object.get_mut(key) {
                if construct {
                    output.insert(field.name.clone(), value.clone());
                    continue;
                }
                let mut transformed = value.clone();
                let prior_issues = issues.len();
                for rule in &field.validators {
                    match rule.as_str() {
                        "python_string_strip" => {
                            let text = semantica_contracts::json::python_str(&transformed)
                                .map_err(|_| {
                                    failure("schema_python_representation_profile_unavailable")
                                })?;
                            transformed = json!(semantica_contracts::json::python_strip(&text));
                        }
                        "confidence_09" | "confidence_0" => {
                            let default = if rule == "confidence_0" { 0. } else { 0.9 };
                            let number = match semantica_contracts::json::python_builtin_float(
                                &transformed,
                            ) {
                                Ok(number) => number,
                                Err(
                                    semantica_contracts::json::PythonFloatError::IntegerOverflow,
                                ) => return Err(failure("schema_python_float_overflow")),
                                Err(semantica_contracts::json::PythonFloatError::InvalidInput) => {
                                    default
                                }
                            };
                            let normalized = if number.is_nan() || number >= 1. {
                                1.
                            } else if number <= 0. {
                                0.
                            } else {
                                number
                            };
                            transformed = json!(normalized);
                        }
                        "decision_timestamp" => {
                            if transformed.is_null() || transformed.is_string() {
                            } else if transformed.is_number() {
                                let number = semantica_json::python_builtin_float(&transformed)
                                    .map_err(|e| {
                                        failure(match e {
                                            semantica_json::PythonFloatError::IntegerOverflow => {
                                                "schema_python_float_overflow"
                                            }
                                            semantica_json::PythonFloatError::InvalidInput => {
                                                "schema_numeric_profile_unavailable"
                                            }
                                        })
                                    })?;
                                // Original datetime.fromtimestamp rounds the signed fractional
                                // second to microseconds, ties to even, then carries.
                                // Truncation retains negative fractional precision.
                                let seconds = number.trunc();
                                let micros =
                                    ((number - seconds) * 1_000_000.).round_ties_even() as i64;
                                let date = (seconds as i64)
                                    .checked_add(micros.div_euclid(1_000_000))
                                    .and_then(|whole| {
                                        Utc.timestamp_opt(
                                            whole,
                                            micros.rem_euclid(1_000_000) as u32 * 1000,
                                        )
                                        .single()
                                    });
                                if let Some(date) = date.filter(|d| (1..=9999).contains(&d.year()))
                                {
                                    let precision = if date.timestamp_subsec_micros() == 0 {
                                        SecondsFormat::Secs
                                    } else {
                                        SecondsFormat::Micros
                                    };
                                    transformed = json!(date.to_rfc3339_opts(precision, false));
                                } else {
                                    let detail = if let Some(date) = date {
                                        format!("year {} is out of range", date.year())
                                    } else {
                                        format!(
                                            "timestamp {} is out of the representable epoch range",
                                            python_repr(&transformed)?
                                        )
                                    };
                                    issue_message(
                                        issues,
                                        "value_error",
                                        &field_loc,
                                        format!("Value error, {detail}"),
                                    );
                                    continue;
                                }
                            } else {
                                let kind = if transformed.is_boolean() {
                                    "bool"
                                } else if transformed.is_array() {
                                    "list"
                                } else {
                                    "dict"
                                };
                                issue_message(
                                    issues,
                                    "value_error",
                                    &field_loc,
                                    format!(
                                        "Value error, timestamp must be None, a string, a datetime, or a numeric epoch; got '{kind}'"
                                    ),
                                );
                                continue;
                            }
                        }
                        _ => return Err(failure("schema_field_validator_unknown")),
                    }
                }
                if issues.len() != prior_issues {
                    continue;
                }
                let shape = &object_schema(model)?["properties"]
                    [field.alias.as_deref().unwrap_or(&field.name)];
                let validated = self.shape(
                    model,
                    shape,
                    &mut transformed,
                    issues,
                    &field_loc,
                    depth + 1,
                )?;
                // Original before-model alias validators mutate the incoming
                // dictionary, while field validators transform output only.
                if shape.get("$ref").is_some()
                    || shape["type"] == "array"
                    || shape["anyOf"].is_array()
                {
                    self.copy_nested_alias_mutations(model, shape, value, depth + 1)?;
                }
                output.insert(field.name.clone(), validated);
            } else if field.required {
                if !construct {
                    issue(issues, "missing", &field_loc);
                }
            } else {
                output.insert(field.name.clone(), field.default["value"].clone());
            }
        }
        if model.config["extra"] == "allow" {
            for (key, value) in object.iter() {
                if !model
                    .fields
                    .iter()
                    .any(|f| f.name == *key || f.alias.as_deref() == Some(key))
                {
                    output.insert(key.clone(), value.clone());
                }
            }
        } else if model.config["extra"] == "forbid" && !construct {
            for key in object.keys() {
                if !model
                    .fields
                    .iter()
                    .any(|f| f.name == *key || f.alias.as_deref() == Some(key))
                {
                    let mut extra = loc.to_vec();
                    extra.push(json!(key));
                    issue(issues, "extra_forbidden", &extra);
                }
            }
        }
        Ok(Value::Object(output))
    }
    fn copy_nested_alias_mutations(
        &self,
        owner: &SemanticModelDefinition,
        shape: &Value,
        input: &mut Value,
        depth: usize,
    ) -> Result<(), SemanticSchemaFailure> {
        let mut ignored = vec![];
        if let Some(reference) = shape["$ref"].as_str() {
            let target = self.reference(owner, reference)?;
            let _ = self.model(target, input, &mut ignored, &[], depth, false)?;
        } else if let Some(items) = input.as_array_mut() {
            for value in items {
                self.copy_nested_alias_mutations(owner, &shape["items"], value, depth + 1)?;
            }
        } else if let Some(alternatives) = shape["anyOf"].as_array() {
            if let Some(shape) = alternatives.iter().find(|s| s["type"] != "null") {
                self.copy_nested_alias_mutations(owner, shape, input, depth + 1)?;
            }
        }
        Ok(())
    }
    fn shape(
        &self,
        owner: &SemanticModelDefinition,
        shape: &Value,
        input: &mut Value,
        issues: &mut Vec<Value>,
        loc: &[Value],
        depth: usize,
    ) -> Result<Value, SemanticSchemaFailure> {
        if depth > MAX_DEPTH {
            return Err(failure("schema_depth_profile_exceeded"));
        }
        if let Some(reference) = shape["$ref"].as_str() {
            return self.model(
                self.reference(owner, reference)?,
                input,
                issues,
                loc,
                depth + 1,
                false,
            );
        }
        if let Some(alternatives) = shape["anyOf"].as_array() {
            if input.is_null() && alternatives.iter().any(|s| s["type"] == "null") {
                return Ok(Value::Null);
            }
            let selected = alternatives
                .iter()
                .find(|s| s["type"] != "null")
                .ok_or_else(|| failure("schema_union_unknown"))?;
            return self.shape(owner, selected, input, issues, loc, depth + 1);
        }
        if let Some(values) = shape["enum"].as_array() {
            if !values.contains(input) {
                issue_message(issues, "literal_error", loc, literal_message(values)?);
            }
            return Ok(input.clone());
        }
        if let Some(value) = shape.get("const") {
            if input != value {
                issue_message(
                    issues,
                    "literal_error",
                    loc,
                    literal_message(std::slice::from_ref(value))?,
                );
            }
            return Ok(input.clone());
        }
        let result = match shape["type"].as_str() {
            None => input.clone(),
            Some("null") => {
                if !input.is_null() {
                    issue(issues, "none_required", loc);
                }
                input.clone()
            }
            Some("string") => {
                if shape["format"] == "date-time" {
                    match datetime_string(input) {
                        Ok(text) => json!(text),
                        Err(code) => {
                            if code == "datetime_from_date_parsing" {
                                let reason = if input.as_str() == Some("") {
                                    "input is too short"
                                } else {
                                    "invalid character in year"
                                };
                                issue_message(
                                    issues,
                                    code,
                                    loc,
                                    format!("Input should be a valid datetime or date, {reason}"),
                                );
                            } else {
                                issue(issues, code, loc);
                            }
                            return Ok(Value::Null);
                        }
                    }
                } else {
                    let Some(text) = input.as_str() else {
                        issue(issues, "string_type", loc);
                        return Ok(Value::Null);
                    };
                    if let Some(max) = shape["maxLength"].as_u64() {
                        if text.chars().count() as u64 > max {
                            issue_message(
                                issues,
                                "string_too_long",
                                loc,
                                format!("String should have at most {max} characters"),
                            );
                        }
                    }
                    if let Some(min) = shape["minLength"].as_u64() {
                        if (text.chars().count() as u64) < min {
                            issue_message(
                                issues,
                                "string_too_short",
                                loc,
                                format!("String should have at least {min} characters"),
                            );
                        }
                    }
                    input.clone()
                }
            }
            Some("integer") => {
                if let Some(value) = integer(input) {
                    value
                } else {
                    issue(
                        issues,
                        if integer_string_too_large(input) {
                            "int_parsing_size"
                        } else if input.is_string() {
                            "int_parsing"
                        } else if input.is_number() {
                            "int_from_float"
                        } else {
                            "int_type"
                        },
                        loc,
                    );
                    Value::Null
                }
            }
            Some("number") => {
                if let Some(number) = float(input) {
                    if !number.is_finite() {
                        for (key, kind, lower) in [
                            ("maximum", "less_than_equal", false),
                            ("exclusiveMaximum", "less_than", false),
                            ("minimum", "greater_than_equal", true),
                            ("exclusiveMinimum", "greater_than", true),
                        ] {
                            if let Some(bound) = shape[key].as_f64() {
                                if number.is_nan()
                                    || (lower && number.is_sign_negative())
                                    || (!lower && number.is_sign_positive())
                                {
                                    issue_message(
                                        issues,
                                        kind,
                                        loc,
                                        format!(
                                            "Input should be {} {bound}",
                                            match kind {
                                                "less_than_equal" => "less than or equal to",
                                                "greater_than_equal" => "greater than or equal to",
                                                "less_than" => "less than",
                                                _ => "greater than",
                                            }
                                        ),
                                    );
                                    return Ok(Value::Null);
                                }
                            }
                        }
                        return Err(failure("schema_nonfinite_profile_unavailable"));
                    }
                    json!(number)
                } else {
                    issue(
                        issues,
                        if input.is_string() {
                            "float_parsing"
                        } else {
                            "float_type"
                        },
                        loc,
                    );
                    Value::Null
                }
            }
            Some("boolean") => {
                if input.is_boolean() {
                    input.clone()
                } else if input.as_f64() == Some(0.) {
                    json!(false)
                } else if input.as_f64() == Some(1.) {
                    json!(true)
                } else if let Some(s) = input.as_str() {
                    match s.to_lowercase().as_str() {
                        "true" | "t" | "yes" | "y" | "on" | "1" => json!(true),
                        "false" | "f" | "no" | "n" | "off" | "0" => json!(false),
                        _ => {
                            issue(issues, "bool_parsing", loc);
                            Value::Null
                        }
                    }
                } else {
                    issue(
                        issues,
                        if input.is_number() && input.as_f64().is_some_and(|n| n.fract() == 0.) {
                            "bool_parsing"
                        } else {
                            "bool_type"
                        },
                        loc,
                    );
                    Value::Null
                }
            }
            Some("array") => {
                let Some(items) = input.as_array_mut() else {
                    issue(
                        issues,
                        if shape["prefixItems"].is_array() {
                            "tuple_type"
                        } else {
                            "list_type"
                        },
                        loc,
                    );
                    return Ok(Value::Null);
                };
                let prefix = shape["prefixItems"].as_array();
                if let Some(prefix) = prefix {
                    if items.len() > prefix.len() {
                        issue(issues, "too_long", loc);
                    }
                    if items.len() < prefix.len() {
                        for i in items.len()..prefix.len() {
                            let mut child = loc.to_vec();
                            child.push(json!(i));
                            issue(issues, "missing", &child);
                        }
                    }
                }
                let mut output = vec![];
                for (i, value) in items.iter_mut().enumerate() {
                    let mut child = loc.to_vec();
                    child.push(json!(i));
                    let sub = prefix.and_then(|p| p.get(i)).unwrap_or(&shape["items"]);
                    output.push(self.shape(owner, sub, value, issues, &child, depth + 1)?);
                }
                Value::Array(output)
            }
            Some("object") => {
                let Some(object) = input.as_object_mut() else {
                    issue(issues, "dict_type", loc);
                    return Ok(Value::Null);
                };
                if let Some(sub) = shape.get("additionalProperties").filter(|x| x.is_object()) {
                    let mut output = Map::new();
                    for (key, value) in object.iter_mut() {
                        let mut child = loc.to_vec();
                        child.push(json!(key));
                        output.insert(
                            key.clone(),
                            self.shape(owner, sub, value, issues, &child, depth + 1)?,
                        );
                    }
                    Value::Object(output)
                } else {
                    input.clone()
                }
            }
            _ => return Err(failure("schema_shape_unknown")),
        };
        if result.is_number() {
            for (key, kind, exclusive, lower) in [
                ("minimum", "greater_than_equal", false, true),
                ("maximum", "less_than_equal", false, false),
                ("exclusiveMinimum", "greater_than", true, true),
                ("exclusiveMaximum", "less_than", true, false),
            ] {
                let Some(bound) = shape.get(key) else {
                    continue;
                };
                let order = if shape["type"] == "integer" {
                    integer_order(&result, bound).or_else(|| {
                        result
                            .as_f64()
                            .zip(bound.as_f64())
                            .and_then(|(a, b)| a.partial_cmp(&b))
                    })
                } else {
                    result
                        .as_f64()
                        .zip(bound.as_f64())
                        .and_then(|(a, b)| a.partial_cmp(&b))
                }
                .ok_or_else(|| failure("schema_numeric_bound_profile_unavailable"))?;
                let bad = if lower {
                    order.is_lt() || (exclusive && order.is_eq())
                } else {
                    order.is_gt() || (exclusive && order.is_eq())
                };
                if bad {
                    let rendered = bound
                        .as_f64()
                        .map(|n| n.to_string())
                        .unwrap_or_else(|| bound.to_string());
                    issue_message(
                        issues,
                        kind,
                        loc,
                        format!(
                            "Input should be {} {rendered}",
                            match kind {
                                "greater_than_equal" => "greater than or equal to",
                                "less_than_equal" => "less than or equal to",
                                "greater_than" => "greater than",
                                _ => "less than",
                            }
                        ),
                    );
                }
            }
        }
        Ok(result)
    }
}
impl SemanticSchemaValidationPort for RustSchemaRuntime {
    fn validate_schema(
        &self,
        input: &SemanticSchemaValidationRequest,
    ) -> Result<SemanticSchemaValidationResult, SemanticSchemaFailure> {
        if input.validate().is_err()
            || input.schema != "semantica.schema-request/1"
            || input.profile != "semantica.pydantic-json/1"
            || input.input_json.len() > MAX_INPUT_BYTES
        {
            return Err(failure("schema_request_invalid"));
        }
        if crate::sha256(input.input_json.as_bytes()) != input.input_sha256 {
            return Err(failure("schema_input_sha_mismatch"));
        }
        let value = semantica_contracts::json::protocol_json(&input.input_json, MAX_INPUT_BYTES)
            .map_err(|_| failure("schema_input_json_invalid"))?;
        let construct = input.operation == "model_construct";
        let result = if construct {
            self.construct_model(&input.model_id, &value)?
        } else {
            self.validate_model(&input.model_id, &value)?
        };
        if !construct && result["status"] == "validated" {
            let snapshot = self.normalized_snapshot(&input.model_id, &result["output"])?;
            let output = serde_json::to_value(snapshot)
                .map_err(|_| failure("schema_typed_snapshot_unrepresentable"))?;
            if output != result["output"] {
                return Err(failure("schema_typed_snapshot_identity_changed"));
            }
        }
        let issues = result["issues"]
            .as_array()
            .ok_or_else(|| failure("schema_issues_invalid"))?
            .iter()
            .map(|x| SemanticSchemaIssue {
                kind: x["type"].as_str().unwrap_or("unknown").into(),
                location: x["loc"].as_array().cloned().unwrap_or_default(),
                message: x["message"].as_str().unwrap_or("unknown").into(),
            })
            .collect();
        Ok(SemanticSchemaValidationResult {
            schema: "semantica.schema-result/1".into(),
            execution: "completed".into(),
            model_id: input.model_id.clone(),
            profile: input.profile.clone(),
            input_sha256: input.input_sha256.clone(),
            validation_executed: !construct,
            valid: !construct && result["status"] == "validated",
            output: result["output"].clone(),
            output_by_alias: result["output_by_alias"].clone(),
            properties: result["properties"].clone(),
            input_after: result["input_after"].clone(),
            input_mutated: result["input_mutated"].as_bool().unwrap_or(false),
            issues,
        })
    }
}
