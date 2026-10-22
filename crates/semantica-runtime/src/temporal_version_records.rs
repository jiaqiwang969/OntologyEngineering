//! Immediate source version-record operations. A type-valid snapshot is not
//! checksum-verified, persisted, selected, or accepted as engineering evidence.
use crate::{runtime::RustSemanticRuntime, temporal_intervals as time};
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalVersionRecordRequest,
    SemanticTemporalVersionRecordResult, SemanticTemporalVersionRecordsPort,
};
use serde_json::{Map, Value, json};

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;
const FIELDS: &[&str] = &[
    "format_version",
    "label",
    "timestamp",
    "author",
    "description",
    "entities",
    "relationships",
    "checksum",
];
fn error(code: &str, message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: format!("temporal_version_records_{code}"),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}
fn invalid(message: impl ToString) -> SemanticRuntimeFailure {
    error("invalid", message)
}
fn source(kind: &str, message: impl ToString) -> SemanticRuntimeFailure {
    error(
        "source_failure",
        format!("builtins.{kind}: {}", message.to_string()),
    )
}
fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}
fn object(v: &Value) -> Result<&Map<String, Value>> {
    v.as_object().ok_or_else(|| {
        source(
            "AttributeError",
            format!("'{}' object has no attribute 'get'", kind(v)),
        )
    })
}
fn truthy(v: &Value) -> Result<bool> {
    Ok(match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(_) => !semantica_json::python_equal(v, &json!(0)).map_err(invalid)?,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    })
}
fn copied(v: Option<&Value>) -> Result<Value> {
    match v {
        None => Ok(json!([])),
        Some(v) if v.is_array() || v.is_object() => Ok(v.clone()),
        Some(v) => Err(source(
            "AttributeError",
            format!("'{}' object has no attribute 'copy'", kind(v)),
        )),
    }
}
fn create(args: &Map<String, Value>, clock: &str, clock_calls: &mut u64) -> Result<Value> {
    let timestamp = args.get("timestamp").unwrap_or(&Value::Null);
    let timestamp = if truthy(timestamp)? {
        timestamp.clone()
    } else {
        *clock_calls += 1;
        json!(clock)
    };
    // Match dict-literal evaluation order: label is evaluated before graph.get.
    let label = args.get("version_label").unwrap_or(&Value::Null);
    let label = if truthy(label)? {
        label.clone()
    } else {
        json!(format!(
            "version_{}",
            semantica_json::python_str(&timestamp).map_err(invalid)?
        ))
    };
    let graph = object(&args["graph"])?;
    let entities = copied(graph.get("entities"))?;
    let relationships = copied(graph.get("relationships"))?;
    let metadata = args.get("metadata").unwrap_or(&Value::Null);
    let metadata = if truthy(metadata)? {
        metadata.clone()
    } else {
        json!({})
    };
    Ok(
        json!({"label":label,"timestamp":timestamp,"entities":entities,"relationships":relationships,"metadata":metadata}),
    )
}
fn validate(snapshot: &Value) -> Result<Value> {
    let mut valid = true;
    for field in FIELDS {
        let value = match snapshot {
            Value::Object(o) => o.get(*field),
            Value::Array(a) => {
                if a.iter().any(|v| v.as_str() == Some(*field)) {
                    return Err(source(
                        "TypeError",
                        "list indices must be integers or slices, not str",
                    ));
                }
                None
            }
            Value::String(s) => {
                if s.contains(*field) {
                    return Err(source(
                        "TypeError",
                        "string indices must be integers, not 'str'",
                    ));
                }
                None
            }
            v => {
                return Err(source(
                    "TypeError",
                    format!("argument of type '{}' is not iterable", kind(v)),
                ));
            }
        };
        valid &= value.is_some_and(|v| {
            if matches!(*field, "entities" | "relationships") {
                v.is_array()
            } else {
                v.is_string()
            }
        });
    }
    Ok(json!(valid))
}
fn migrate(snapshot: &Value) -> Result<Value> {
    let mut out = object(snapshot)?.clone();
    if out.get("format_version").and_then(Value::as_str) == Some("1.0") {
        return Ok(Value::Object(out));
    }
    out.insert("format_version".into(), json!("1.0"));
    for field in &FIELDS[1..] {
        out.entry((*field).to_string()).or_insert(Value::Null);
    }
    Ok(Value::Object(out))
}
fn local_clock(raw: &str) -> Result<()> {
    if !matches!(raw.len(), 19 | 26) || !raw.is_ascii() {
        return Err(invalid(
            "Clock must be canonical naive local datetime.isoformat() text",
        ));
    }
    let dt = time::timestamp(raw).map_err(|e| invalid(e.message))?;
    let mut canonical = time::text(dt);
    canonical.pop(); // Source local clock has no timezone suffix.
    if canonical != raw {
        return Err(invalid(
            "Clock must be canonical naive local datetime.isoformat() text",
        ));
    }
    Ok(())
}
pub(crate) fn original_json_ingress(arguments: &Value) -> Result<()> {
    let mut pending = vec![arguments];
    while let Some(value) = pending.pop() {
        match value {
            Value::Number(n) => {
                let token = n.to_string();
                if !token.contains(['.', 'e', 'E']) {
                    semantica_json::original_json::check_original_integer_digits(
                        token.trim_start_matches('-').len(),
                    )
                    .map_err(|e| error("ingress_failure", format!("builtins.ValueError: {e}")))?;
                }
            }
            Value::Array(values) => pending.extend(values.iter()),
            Value::Object(values) => pending.extend(values.values()),
            _ => (),
        }
    }
    Ok(())
}
impl SemanticTemporalVersionRecordsPort for RustSemanticRuntime {
    fn temporal_version_records(
        &self,
        r: &SemanticTemporalVersionRecordRequest,
    ) -> Result<SemanticTemporalVersionRecordResult> {
        crate::datalog_port::check_serialized(r, MAX_INPUT_BYTES, false)
            .map_err(|_| invalid("Complete request exceeds1MiB"))?;
        // The generic JSON carrier supports larger exact integers. This source
        // ingress profile retains the pinned Python decoder's original limit,
        // including values the selected operation would otherwise ignore.
        original_json_ingress(&r.arguments)?;
        if r.schema != "semantica.temporal-version-records-request/1" {
            return Err(invalid("Unknown version-record schema"));
        }
        let args = r
            .arguments
            .as_object()
            .ok_or_else(|| invalid("Arguments must be a dictionary"))?;
        match r.operation.as_str() {
            "create_version" if args.contains_key("graph") => (),
            "validate_snapshot" | "migrate_snapshot"
                if args.len() == 1 && args.contains_key("snapshot") =>
            {
                ()
            }
            _ => return Err(invalid("Unknown operation or required argument contract")),
        }
        local_clock(&r.clock_local)?;
        let mut clock_calls = 0;
        let observed = match r.operation.as_str() {
            "create_version" => create(args, &r.clock_local, &mut clock_calls),
            "validate_snapshot" => validate(&args["snapshot"]),
            "migrate_snapshot" => migrate(&args["snapshot"]),
            _ => unreachable!(),
        };
        let (result, failure) = match observed {
            Ok(value) => (Some(value), None),
            Err(e) => (None, Some(e)),
        };
        let out = SemanticTemporalVersionRecordResult {
            schema: "semantica.temporal-version-records-result/1".into(),
            profile: "semantica.source-temporal-version-records/1".into(),
            request_sha256: crate::sha256(
                semantica_json::canonical_json(&serde_json::to_value(r).map_err(invalid)?)
                    .map_err(invalid)?,
            ),
            operation: r.operation.clone(),
            clock_local: r.clock_local.clone(),
            clock_fallback_count: clock_calls,
            result,
            failure,
        };
        crate::datalog_port::check_serialized(&out, MAX_OUTPUT_BYTES - 1, true)
            .map_err(|_| invalid("Complete result exceeds16MiB"))?;
        Ok(out)
    }
}
