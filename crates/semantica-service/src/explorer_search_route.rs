//! Original POST route/model diagnostics. Session and permission stay in the host.
use crate::explorer_path_route::{decode, encode_redirect_path};
use anyhow::{Context, Result};
use serde_json::{Value, json};
pub enum Target {
    Search,
    Reply {
        status: u16,
        body: Option<Value>,
        location: Option<String>,
    },
}
pub fn parse_target(method: &str, target: &str, authority: &str) -> Target {
    let (raw, query) = target
        .split_once('?')
        .map_or((target, None), |(a, b)| (a, Some(b)));
    let path = decode(raw, false);
    if path != "/api/graph/search" {
        if path.ends_with('/') && path.trim_end_matches('/') == "/api/graph/search" {
            return Target::Reply {
                status: 307,
                body: None,
                location: Some(format!(
                    "http://{authority}{}{}",
                    encode_redirect_path(path.trim_end_matches('/')),
                    query.map_or(String::new(), |q| format!("?{q}"))
                )),
            };
        }
        return Target::Reply {
            status: 404,
            body: Some(json!({"detail":"Not Found"})),
            location: None,
        };
    }
    if method != "POST" {
        return Target::Reply {
            status: 405,
            body: Some(json!({"detail":"Method Not Allowed"})),
            location: None,
        };
    }
    Target::Search
}
/// Convert the original model's issues to FastAPI request diagnostics. Numeric
/// bounds come from the same generated model, not a second admission rule set.
pub fn validation_error(body: &Value) -> Result<Option<Value>> {
    if body.is_null() {
        return Ok(Some(
            json!({"detail":[{"type":"missing","loc":["body"],"msg":"Field required","input":null}]}),
        ));
    }
    if !body.is_object() {
        return Ok(Some(
            json!({"detail":[{"type":"model_attributes_type","loc":["body"],"msg":"Input should be a valid dictionary or object to extract fields from","input":body}]}),
        ));
    }
    let validation = semantica_runtime::explorer_search::validate_request(body)?;
    if validation["status"] == "validated" {
        return Ok(None);
    }
    let schema = semantica_runtime::schema::RustSchemaRuntime::new()
        .map_err(|e| anyhow::anyhow!("{}", e.code))?
        .model_json_schema("semantica.explorer.schemas::SearchRequest")
        .map_err(|e| anyhow::anyhow!("{}", e.code))?;
    let mut errors = vec![];
    for issue in validation["issues"]
        .as_array()
        .context("search_validation_issues_missing")?
    {
        let kind = issue["type"]
            .as_str()
            .context("search_validation_kind_missing")?;
        let loc = issue["loc"]
            .as_array()
            .context("search_validation_location_missing")?;
        let mut value = body;
        let mut shape = &schema;
        for (i, key) in loc.iter().enumerate() {
            if kind == "missing" && i + 1 == loc.len() {
                break;
            }
            if let Some(key) = key.as_str() {
                value = value.get(key).context("search_validation_input_missing")?;
                shape = &shape["properties"][key];
            } else if let Some(index) = key.as_u64() {
                value = value
                    .get(index as usize)
                    .context("search_validation_index_missing")?;
                shape = &shape["items"];
            }
        }
        let mut location = vec![json!("body")];
        location.extend(loc.iter().cloned());
        let mut error = json!({"type":kind,"loc":location,"msg":issue["message"],"input":value});
        match kind {
            "greater_than_equal" | "less_than_equal" | "greater_than" | "less_than" => {
                let (ctx, key) = match kind {
                    "greater_than_equal" => ("ge", "minimum"),
                    "less_than_equal" => ("le", "maximum"),
                    "greater_than" => ("gt", "exclusiveMinimum"),
                    _ => ("lt", "exclusiveMaximum"),
                };
                error["ctx"] =
                    json!({ctx:shape.get(key).context("search_validation_bound_missing")?});
            }
            "literal_error" => {
                error["ctx"] = json!({"expected":issue["message"].as_str().context("search_validation_message_missing")?.strip_prefix("Input should be ").context("search_validation_literal_message")?})
            }
            _ => {}
        }
        errors.push(error);
    }
    Ok(Some(json!({"detail":errors})))
}
