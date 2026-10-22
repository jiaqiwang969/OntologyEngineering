//! Original decision route order; causal-distance remains a literal decision ID
//! in this compatibility entry. Corrected causal analysis requires its own contract.
use crate::{
    explorer_path_route::{decode, encode_redirect_path},
    explorer_read_route::integer,
};
use semantica_contracts::{SemanticExplorerCausalDistanceQuery, SemanticExplorerDecisionQuery};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub enum Parsed {
    Query(SemanticExplorerDecisionQuery),
    Causal(SemanticExplorerCausalDistanceQuery),
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
pub(super) fn corrected_target(target: &str) -> bool {
    decode(target.split('?').next().unwrap_or(target), false).trim_end_matches('/')
        == "/api/v2/decisions/causal-distance"
}
fn route(path: &str) -> Option<(&str, Option<&str>)> {
    if path == "/api/v2/decisions/causal-distance" {
        return Some(("causal-distance", None));
    }
    if path == "/api/decisions" {
        return Some(("list", None));
    }
    let rest = path.strip_prefix("/api/decisions/")?;
    let parts = rest.split('/').collect::<Vec<_>>();
    if parts[0].is_empty() {
        return None;
    }
    match parts.as_slice() {
        [id] => Some(("get", Some(id))),
        [id, op] if ["chain", "precedents", "compliance"].contains(op) => Some((op, Some(id))),
        _ => None,
    }
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
        let (k, v) = item.split_once('=').unwrap_or((item, ""));
        fields.insert(decode(k, true), decode(v, true));
    }
    if operation == "causal-distance" {
        let missing = ["source", "target"]
            .into_iter()
            .filter(|k| !fields.contains_key(*k))
            .map(
                |k| json!({"type":"missing","loc":["query",k],"msg":"Field required","input":null}),
            )
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return reply(422, json!({"detail":missing}));
        }
        return Parsed::Causal(SemanticExplorerCausalDistanceQuery {
            schema: "semantica.explorer-causal-distance-query/2".into(),
            source_id: Some(fields["source"].clone()),
            target_id: Some(fields["target"].clone()),
        });
    }
    let mut errors = vec![];
    let mut skip = "0".into();
    let mut limit = "50".into();
    if operation == "list" {
        skip = integer(&fields, "skip", 0, 0, None, &mut errors);
        limit = integer(&fields, "limit", 50, 1, Some(500), &mut errors);
    }
    if operation == "precedents" {
        limit = integer(&fields, "limit", 10, 1, Some(100), &mut errors);
    }
    if !errors.is_empty() {
        return reply(422, json!({"detail":errors}));
    }
    Parsed::Query(SemanticExplorerDecisionQuery {
        schema: "semantica.explorer-decision-query/1".into(),
        operation: operation.into(),
        decision_id: id.map(str::to_owned),
        category: if operation == "list" {
            fields.get("category").cloned()
        } else {
            None
        },
        skip,
        limit: limit.parse().expect("bounded integer"),
    })
}
