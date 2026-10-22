//! Graph-read route parsing after the shared finite HTTP framing validation.
use crate::explorer_path_route::{decode, encode_redirect_path};
use semantica_contracts::SemanticExplorerGraphReadQuery;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub enum Parsed {
    Query(SemanticExplorerGraphReadQuery),
    Reply {
        status: u16,
        body: Option<Value>,
        location: Option<String>,
    },
}
fn reply(status: u16, body: Value) -> Parsed {
    Parsed::Reply {
        status,
        body: Some(body),
        location: None,
    }
}
fn route(path: &str) -> Option<(&str, Option<&str>)> {
    match path {
        "/api/graph/nodes" => return Some(("nodes", None)),
        "/api/graph/edges" => return Some(("edges", None)),
        _ => {}
    }
    let rest = path.strip_prefix("/api/graph/node/")?;
    let (operation, id) = rest
        .strip_suffix("/neighbors")
        .map_or(("node", rest), |id| ("neighbors", id));
    if id.is_empty() || id.contains('/') {
        None
    } else {
        Some((operation, Some(id)))
    }
}
pub(super) fn integer(
    fields: &BTreeMap<String, String>,
    name: &str,
    default: u64,
    min: u64,
    max: Option<u64>,
    errors: &mut Vec<Value>,
) -> String {
    let Some(text) = fields.get(name) else {
        return default.to_string();
    };
    let value = match semantica_runtime::schema::parse_query_integer(text) {
        Ok(v) => v,
        Err(code) => {
            errors.push(json!({"type":code,"loc":["query",name],"msg":if code=="int_parsing_size"{
                "Unable to parse input string as an integer, exceeded maximum size"}else{"Input should be a valid integer, unable to parse string as an integer"},"input":text}));
            return default.to_string();
        }
    };
    let number = value.as_number().expect("integer parser returns a number");
    let decimal = number.to_string();
    if decimal.starts_with('-') || number.as_u64().is_some_and(|v| v < min) {
        errors.push(json!({"type":"greater_than_equal","loc":["query",name],"msg":format!("Input should be greater than or equal to {min}"),"input":text,"ctx":{"ge":min}}));
    } else if let Some(max) = max {
        if number.as_u64().is_none_or(|v| v > max) {
            errors.push(json!({"type":"less_than_equal","loc":["query",name],"msg":format!("Input should be less than or equal to {max}"),"input":text,"ctx":{"le":max}}));
        }
    }
    decimal
}
pub fn parse_target(method: &str, target: &str, authority: &str) -> Parsed {
    let (raw_path, raw_query) = target
        .split_once('?')
        .map_or((target, None), |(p, q)| (p, Some(q)));
    let path = decode(raw_path, false);
    let Some((operation, id)) = route(&path) else {
        if path.ends_with('/') && route(path.trim_end_matches('/')).is_some() {
            let query = raw_query.map_or(String::new(), |q| format!("?{q}"));
            return Parsed::Reply {
                status: 307,
                body: None,
                location: Some(format!(
                    "http://{authority}{}{query}",
                    encode_redirect_path(path.trim_end_matches('/'))
                )),
            };
        }
        return reply(404, json!({"detail":"Not Found"}));
    };
    if method != "GET" {
        return reply(405, json!({"detail":"Method Not Allowed"}));
    }
    let mut fields = BTreeMap::new();
    for item in raw_query.unwrap_or("").split('&').filter(|s| !s.is_empty()) {
        let (key, value) = item.split_once('=').unwrap_or((item, ""));
        fields.insert(decode(key, true), decode(value, true));
    }
    let mut errors = Vec::new();
    let mut skip = "0".into();
    let mut limit = "100".into();
    let mut depth = "1".into();
    if ["nodes", "edges"].contains(&operation) {
        skip = integer(&fields, "skip", 0, 0, None, &mut errors);
        limit = integer(&fields, "limit", 100, 1, Some(5000), &mut errors);
    }
    if operation == "neighbors" {
        depth = integer(&fields, "depth", 1, 1, Some(5), &mut errors);
    }
    if !errors.is_empty() {
        return reply(422, json!({"detail":errors}));
    }
    Parsed::Query(SemanticExplorerGraphReadQuery {
        schema: "semantica.explorer-graph-read-query/1".into(),
        operation: operation.into(),
        node_id: id.map(str::to_owned),
        node_type: if operation == "nodes" {
            fields.get("type").cloned()
        } else {
            None
        },
        edge_type: if operation == "edges" {
            fields.get("type").cloned()
        } else {
            None
        },
        source: fields.get("source").cloned(),
        target: fields.get("target").cloned(),
        search: fields.get("search").cloned(),
        bbox: fields.get("bbox").cloned(),
        cursor: fields.get("cursor").cloned(),
        skip,
        limit: limit.parse().expect("bounded integer"),
        depth: depth.parse().expect("bounded integer"),
    })
}
