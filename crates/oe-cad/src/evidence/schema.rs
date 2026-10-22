//! A deliberately closed validator for the three embedded CAD schemas. This is
//! not a general JSON Schema engine or an executable semantic authority.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

pub(crate) const CAD: &str =
    include_str!("../../../../skills/cad-agent/contracts/cad-evidence.v1.schema.json");
pub(crate) const HANDOFF: &str =
    include_str!("../../../../skills/cad-agent/contracts/cad-process-handoff.v1.schema.json");
pub(crate) const ANALYSIS: &str =
    include_str!("../../../../skills/cad-agent/contracts/analysis-record.v1.schema.json");

pub(crate) fn validate(value: &Value, source: &str) -> Result<()> {
    let schema = oe_wire::json::strict_json(source.as_bytes())?;
    check(value, &schema, &schema, "", 0)
}
fn check(v: &Value, s: &Value, root: &Value, path: &str, depth: usize) -> Result<()> {
    ensure!(depth <= 96, "schema_depth_limit");
    let map = s.as_object().context("schema_object_required")?;
    for key in map.keys() {
        ensure!(
            [
                "$schema",
                "$id",
                "title",
                "$defs",
                "$ref",
                "type",
                "const",
                "enum",
                "required",
                "properties",
                "additionalProperties",
                "minLength",
                "minItems",
                "minProperties",
                "uniqueItems",
                "items",
                "pattern",
                "propertyNames"
            ]
            .contains(&key.as_str()),
            "unsupported_embedded_schema_keyword:{key}"
        );
    }
    if let Some(reference) = s.get("$ref") {
        let pointer = reference
            .as_str()
            .context("schema_ref_string")?
            .strip_prefix('#')
            .context("external_schema_ref_forbidden")?;
        return check(
            v,
            root.pointer(pointer).context("schema_ref_missing")?,
            root,
            path,
            depth + 1,
        );
    }
    if let Some(t) = s.get("type") {
        let accepts = |name: &str| match name {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "number" => v.is_number(),
            "null" => v.is_null(),
            _ => false,
        };
        let valid = if let Some(name) = t.as_str() {
            accepts(name)
        } else {
            t.as_array()
                .context("schema_types")?
                .iter()
                .any(|n| n.as_str().is_some_and(accepts))
        };
        ensure!(valid, "schema:{path}:type");
    }
    if let Some(c) = s.get("const") {
        ensure!(v == c, "schema:{path}:const");
    }
    if let Some(e) = s.get("enum") {
        ensure!(
            e.as_array().context("schema_enum")?.contains(v),
            "schema:{path}:enum"
        );
    }
    if let Some(text) = v.as_str() {
        if let Some(n) = s.get("minLength") {
            ensure!(
                text.chars().count() >= n.as_u64().context("schema_length")? as usize,
                "schema:{path}:minLength"
            );
        }
        if let Some(p) = s.get("pattern") {
            let valid = match p.as_str().context("schema_pattern")? {
                "^[A-Za-z][A-Za-z0-9_.:-]*$" => {
                    text.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                        && text
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
                }
                "^[0-9a-f]{64}$" => {
                    text.len() == 64
                        && text
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }
                "^/" => text.starts_with('/'),
                other => bail!("unsupported_embedded_schema_pattern:{other}"),
            };
            ensure!(valid, "schema:{path}:pattern");
        }
    }
    if let Some(a) = v.as_array() {
        if let Some(n) = s.get("minItems") {
            ensure!(
                a.len() >= n.as_u64().context("schema_items")? as usize,
                "schema:{path}:minItems"
            );
        }
        if s.get("uniqueItems") == Some(&Value::Bool(true)) {
            let mut unique = std::collections::BTreeSet::new();
            for item in a {
                ensure!(
                    unique.insert(oe_wire::json::canonical_json(item)?),
                    "schema:{path}:uniqueItems"
                );
            }
        }
        if let Some(item_schema) = s.get("items") {
            for (i, item) in a.iter().enumerate() {
                check(item, item_schema, root, &format!("{path}/{i}"), depth + 1)?;
            }
        }
    }
    if let Some(o) = v.as_object() {
        if let Some(n) = s.get("minProperties") {
            ensure!(
                o.len() >= n.as_u64().context("schema_properties")? as usize,
                "schema:{path}:minProperties"
            );
        }
        if let Some(required) = s.get("required") {
            for key in required.as_array().context("schema_required")? {
                let key = key.as_str().context("schema_key")?;
                ensure!(o.contains_key(key), "schema:{path}:required:{key}");
            }
        }
        for (key, item) in o {
            if let Some(names) = s.get("propertyNames") {
                check(&Value::String(key.clone()), names, root, path, depth + 1)?;
            }
            if let Some(child) = s.get("properties").and_then(|p| p.get(key)) {
                check(item, child, root, &format!("{path}/{key}"), depth + 1)?;
            } else if let Some(extra) = s.get("additionalProperties") {
                match extra {
                    Value::Bool(false) => bail!("schema:{path}:additional_property:{key}"),
                    Value::Bool(true) => (),
                    _ => check(item, extra, root, &format!("{path}/{key}"), depth + 1)?,
                }
            }
        }
    }
    Ok(())
}
