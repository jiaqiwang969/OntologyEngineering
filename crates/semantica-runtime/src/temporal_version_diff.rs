//! Source version differences, including original ordered set operations and
//! explicit read-only in-memory storage state. Project decisions stay with Project.
use crate::runtime::RustSemanticRuntime;
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticTemporalVersionDiffPort, SemanticTemporalVersionDiffRequest,
    SemanticTemporalVersionDiffResult,
};
use semantica_json::python_set::{PythonSet, SetContext, SetError};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_WORK: usize = 16_777_216;
pub const MAX_COLLECTION_ITEMS: usize = 16384;
type Result<T> = std::result::Result<T, SemanticRuntimeFailure>;
fn error(code: &str, message: impl ToString) -> SemanticRuntimeFailure {
    SemanticRuntimeFailure {
        code: format!("temporal_version_diff_{code}"),
        phase: "execution".into(),
        message: message.to_string(),
        retryable: false,
    }
}
fn source(kind: &str, message: impl ToString) -> SemanticRuntimeFailure {
    error("source_failure", format!("{kind}: {}", message.to_string()))
}
fn set_error(e: SetError) -> SemanticRuntimeFailure {
    match e {
        SetError::Unhashable(_) => source("builtins.TypeError", e),
        SetError::ResourceLimit => error("resource_limit", e),
        _ => error("invalid", e),
    }
}
fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
        Value::Number(n) if n.to_string().contains(['.', 'e', 'E']) => "float",
        Value::Number(_) => "int",
    }
}
fn object(v: &Value) -> Result<&Map<String, Value>> {
    v.as_object().ok_or_else(|| {
        source(
            "builtins.AttributeError",
            format!("'{}' object has no attribute 'get'", kind(v)),
        )
    })
}
fn truthy(v: &Value) -> Result<bool> {
    Ok(match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(_) => {
            !semantica_json::python_equal(v, &json!(0)).map_err(|e| error("invalid", e))?
        }
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    })
}
fn resolve<'a>(
    value: &'a Value,
    stored: &'a Map<String, Value>,
    reads: &mut Vec<String>,
) -> Result<&'a Value> {
    if let Some(label) = value.as_str() {
        reads.push(label.to_owned());
        let snapshot = stored.get(label).unwrap_or(&Value::Null);
        if !truthy(snapshot)? {
            return Err(source(
                "semantica.utils.exceptions.ValidationError",
                format!("Version not found: {label}"),
            ));
        }
        Ok(snapshot)
    } else {
        Ok(value)
    }
}
fn iterable(v: &Value) -> Result<Vec<Value>> {
    let values = match v {
        Value::Array(a) => a.clone(),
        Value::Object(o) => o.keys().map(|s| json!(s)).collect(),
        Value::String(s) => s.chars().map(|c| json!(c.to_string())).collect(),
        _ => {
            return Err(source(
                "builtins.TypeError",
                format!("'{}' object is not iterable", kind(v)),
            ));
        }
    };
    if values.len() > MAX_COLLECTION_ITEMS {
        return Err(error(
            "resource_limit",
            "Source collection exceeds16384items",
        ));
    }
    Ok(values)
}
struct SourceMap {
    entries: Vec<(Value, Value)>,
    buckets: HashMap<i64, Vec<usize>>,
}
impl SourceMap {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            buckets: HashMap::new(),
        }
    }
    fn index(&self, ctx: &SetContext, key: &Value, hash: i64) -> Result<Option<usize>> {
        if let Some(indices) = self.buckets.get(&hash) {
            for i in indices {
                if ctx.equal(&self.entries[*i].0, key).map_err(set_error)? {
                    return Ok(Some(*i));
                }
            }
        }
        Ok(None)
    }
    fn insert(&mut self, ctx: &SetContext, key: Value, value: Value) -> Result<()> {
        let hash = ctx.hash(&key).map_err(set_error)?;
        if let Some(i) = self.index(ctx, &key, hash)? {
            self.entries[i].1 = value;
        } else {
            self.buckets
                .entry(hash)
                .or_default()
                .push(self.entries.len());
            self.entries.push((key, value));
        }
        Ok(())
    }
    fn get(&self, ctx: &SetContext, key: &Value) -> Result<&Value> {
        let hash = ctx.hash(key).map_err(set_error)?;
        self.index(ctx, key, hash)?
            .map(|i| &self.entries[i].1)
            .ok_or_else(|| error("internal", "Source set key absent from its dictionary"))
    }
    fn keys(&self, ctx: &SetContext) -> Result<PythonSet> {
        PythonSet::from_values(ctx, self.entries.iter().map(|(k, _)| k.clone())).map_err(set_error)
    }
}
fn relationship_key(o: &Map<String, Value>) -> Result<Value> {
    let empty = json!("");
    let items = [
        o.get("source").unwrap_or(&empty),
        o.get("type")
            .unwrap_or_else(|| o.get("relationship").unwrap_or(&empty)),
        o.get("target").unwrap_or(&empty),
    ];
    let parts = items
        .iter()
        .map(|v| semantica_json::python_str(v).map_err(|e| error("invalid", e)))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!(parts.join("|")))
}
fn collection(ctx: &SetContext, version: &Value, relationships: bool) -> Result<SourceMap> {
    let o = object(version)?;
    let empty = json!([]);
    let items = iterable(
        o.get(if relationships {
            "relationships"
        } else {
            "entities"
        })
        .unwrap_or(&empty),
    )?;
    let mut out = SourceMap::new();
    for (i, item) in items.into_iter().enumerate() {
        let o = object(&item)?;
        let key = if relationships {
            relationship_key(o)?
        } else {
            o.get("id").cloned().unwrap_or_else(|| json!(i.to_string()))
        };
        out.insert(ctx, key, item)?;
    }
    Ok(out)
}
fn changes(ctx: &SetContext, before: &Value, after: &Value) -> Result<Value> {
    let a = object(before)?;
    let b = object(after)?;
    let ak = PythonSet::from_values(ctx, a.keys().map(|k| json!(k))).map_err(set_error)?;
    let bk = PythonSet::from_values(ctx, b.keys().map(|k| json!(k))).map_err(set_error)?;
    let mut out = Map::new();
    for key in ak.union(&bk).map_err(set_error)?.values() {
        let key = key
            .as_str()
            .ok_or_else(|| error("internal", "Nonstring JSON member"))?;
        let av = a.get(key).unwrap_or(&Value::Null);
        let bv = b.get(key).unwrap_or(&Value::Null);
        if !ctx.equal(av, bv).map_err(set_error)? {
            out.insert(key.into(), json!({"from":av,"to":bv}));
        }
    }
    Ok(Value::Object(out))
}
fn diff(ctx: &SetContext, before: &Value, after: &Value) -> Result<Value> {
    // Retain source comprehension evaluation order, including pre-diff errors.
    let e1 = collection(ctx, before, false)?;
    let e2 = collection(ctx, after, false)?;
    let r1 = collection(ctx, before, true)?;
    let r2 = collection(ctx, after, true)?;
    let mut detail = Map::new();
    let mut summary = Map::new();
    for (prefix, left, right, id_name) in [
        ("entities", &e1, &e2, "id"),
        ("relationships", &r1, &r2, "key"),
    ] {
        let a = left.keys(ctx)?;
        let b = right.keys(ctx)?;
        for (suffix, keys, map) in [
            ("added", b.difference(&a).map_err(set_error)?, right),
            ("removed", a.difference(&b).map_err(set_error)?, left),
        ] {
            let values = keys
                .values()
                .iter()
                .map(|k| map.get(ctx, k).cloned())
                .collect::<Result<Vec<_>>>()?;
            summary.insert(format!("{prefix}_{suffix}"), json!(values.len()));
            detail.insert(format!("{prefix}_{suffix}"), json!(values));
        }
        let mut modified = Vec::new();
        for key in a.intersection(&b).map_err(set_error)?.values() {
            let av = left.get(ctx, &key)?;
            let bv = right.get(ctx, &key)?;
            if !ctx.equal(av, bv).map_err(set_error)? {
                let delta = changes(ctx, av, bv)?;
                // Dynamic id/key member retains the source member insertion order.
                let mut item = Map::new();
                item.insert(id_name.into(), key);
                item.insert("before".into(), av.clone());
                item.insert("after".into(), bv.clone());
                item.insert("changes".into(), delta);
                modified.push(Value::Object(item));
            }
        }
        summary.insert(format!("{prefix}_modified"), json!(modified.len()));
        detail.insert(format!("{prefix}_modified"), json!(modified));
    }
    let mut out = Map::new();
    out.insert(
        "version1".into(),
        object(before)?
            .get("label")
            .cloned()
            .unwrap_or(json!("unknown")),
    );
    out.insert(
        "version2".into(),
        object(after)?
            .get("label")
            .cloned()
            .unwrap_or(json!("unknown")),
    );
    out.insert("summary".into(), Value::Object(summary));
    out.extend(detail);
    Ok(Value::Object(out))
}
impl SemanticTemporalVersionDiffPort for RustSemanticRuntime {
    fn temporal_version_diff(
        &self,
        request: &SemanticTemporalVersionDiffRequest,
    ) -> Result<SemanticTemporalVersionDiffResult> {
        crate::datalog_port::check_serialized(request, MAX_INPUT_BYTES, false)
            .map_err(|_| error("invalid", "Complete request exceeds1MiB"))?;
        if request.schema != "semantica.temporal-version-diff-request/1"
            || request.hash_seed > u32::MAX as u64
        {
            return Err(error(
                "invalid",
                "Unknown comparison schema or invalid Python hash seed",
            ));
        }
        let value = serde_json::to_value(request).map_err(|e| error("invalid", e))?;
        crate::temporal_version_records::original_json_ingress(&value)
            .map_err(|e| error("ingress_failure", e.message))?;
        let stored = request
            .stored_versions
            .as_object()
            .ok_or_else(|| error("invalid", "Stored versions must be an explicit dictionary"))?;
        let ctx = SetContext::new(request.hash_seed as u32, MAX_WORK);
        let mut reads = Vec::new();
        let observed = (|| {
            let before = resolve(&request.version1, stored, &mut reads)?;
            let after = resolve(&request.version2, stored, &mut reads)?;
            diff(&ctx, before, after)
        })();
        let (result, failure) = match observed {
            Ok(v) => (Some(v), None),
            Err(e) if e.code == "temporal_version_diff_source_failure" => (None, Some(e)),
            Err(e) => return Err(e),
        };
        let out = SemanticTemporalVersionDiffResult {
            schema: "semantica.temporal-version-diff-result/1".into(),
            profile: "semantica.source-temporal-version-diff-cpython313/1".into(),
            request_sha256: crate::sha256(
                semantica_json::canonical_json(&value).map_err(|e| error("invalid", e))?,
            ),
            hash_seed: request.hash_seed,
            result,
            failure,
            storage_reads: reads,
        };
        crate::datalog_port::check_serialized(&out, MAX_OUTPUT_BYTES - 1, true)
            .map_err(|_| error("resource_limit", "Complete result exceeds16MiB"))?;
        Ok(out)
    }
}
