//! Path-route decoding after the existing HTTP framing owner validates the target.
//! This layer does not authorize, charge a query, admit graph IDs, or execute paths.
use semantica_contracts::SemanticExplorerPathQuery;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Debug)]
pub enum Parsed {
    Query(SemanticExplorerPathQuery),
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
fn digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
pub(super) fn decode(raw: &str, plus: bool) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (digit(bytes[i + 1]), digit(bytes[i + 2])) {
                out.push(a * 16 + b);
                i += 3;
                continue;
            }
        }
        out.push(if plus && bytes[i] == b'+' {
            b' '
        } else {
            bytes[i]
        });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn path_source(path: &str) -> Option<Option<&str>> {
    if path == "/api/graph/path" {
        return Some(None);
    }
    let node = path
        .strip_prefix("/api/graph/node/")?
        .strip_suffix("/path")?;
    if node.is_empty() || node.contains('/') {
        None
    } else {
        Some(Some(node))
    }
}
pub(super) fn encode_redirect_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/:@!$&'()*+,;=-._~%".contains(&b) {
            out.push(char::from(b));
        } else {
            use std::fmt::Write;
            write!(&mut out, "%{b:02X}").expect("string write");
        }
    }
    out
}
/// Target and authority must come from parse_header_with_body's finite framing
/// validation, not arbitrary trusted configuration. Query extras are ignored as
/// upstream: they never become resource/work selections. Duplicate keys use last.
pub fn parse_target(method: &str, target: &str, authority: &str) -> Parsed {
    let (raw_path, raw_query) = target
        .split_once('?')
        .map_or((target, None), |(p, q)| (p, Some(q)));
    let path = decode(raw_path, false);
    let Some(node) = path_source(&path) else {
        if path.ends_with('/') && path_source(path.trim_end_matches('/')).is_some() {
            let suffix = raw_query.map_or(String::new(), |q| format!("?{q}"));
            return Parsed::Reply {
                status: 307,
                body: None,
                location: Some(format!(
                    "http://{authority}{}{suffix}",
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
    for part in raw_query.unwrap_or("").split('&').filter(|s| !s.is_empty()) {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        fields.insert(decode(key, true), decode(value, true));
    }
    let mut errors = Vec::new();
    let source = node
        .map(str::to_owned)
        .or_else(|| fields.get("source").cloned());
    let target = fields.get("target").cloned();
    for (name, value) in [("source", &source), ("target", &target)] {
        if value.is_none() {
            errors.push(
                json!({"type":"missing","loc":["query",name],"msg":"Field required","input":null}),
            );
        }
    }
    let algorithm = fields.get("algorithm").map(String::as_str).unwrap_or("bfs");
    if !["bfs", "dijkstra"].contains(&algorithm) {
        errors.push(json!({"type":"enum","loc":["query","algorithm"],"msg":"Input should be 'bfs' or 'dijkstra'","input":algorithm,"ctx":{"expected":"'bfs' or 'dijkstra'"}}));
    }
    let directed = match fields.get("directed").map(|v| v.to_ascii_lowercase()) {
        None => Some(true),
        Some(v) => match v.as_str() {
            "1" | "true" | "on" | "yes" | "t" => Some(true),
            "0" | "false" | "off" | "no" | "f" => Some(false),
            _ => None,
        },
    };
    if directed.is_none() {
        errors.push(json!({"type":"bool_parsing","loc":["query","directed"],"msg":"Input should be a valid boolean, unable to interpret input","input":fields["directed"]}));
    }
    if !errors.is_empty() {
        return reply(422, json!({"detail":errors}));
    }
    Parsed::Query(SemanticExplorerPathQuery {
        schema: "semantica.explorer-path-query/1".into(),
        source: source.expect("checked missing source"),
        target: target.expect("checked missing target"),
        algorithm: algorithm.into(),
        directed: directed.expect("checked bool"),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_tcp_route_parameters_match_recorded_cases() {
        let cases: Value =
            serde_json::from_str(include_str!("../tests/fixtures/path_parameter_tcp.json"))
                .unwrap();
        assert_eq!(cases.as_array().unwrap().len(), 29);
        for case in cases.as_array().unwrap() {
            match parse_target(
                case["method"].as_str().unwrap(),
                case["target"].as_str().unwrap(),
                "testserver",
            ) {
                Parsed::Query(q) => {
                    assert_eq!(case["status"], 200, "{}", case["name"]);
                    assert_eq!(
                        json!({"source":q.source,"target":q.target,"algorithm":q.algorithm,"directed":q.directed}),
                        case["observed_query"],
                        "{}",
                        case["name"]
                    );
                }
                Parsed::Reply {
                    status,
                    body,
                    location,
                } => {
                    assert_eq!(
                        status,
                        case["status"].as_u64().unwrap() as u16,
                        "{}",
                        case["name"]
                    );
                    assert_eq!(
                        body.unwrap_or(Value::Null),
                        case["body"],
                        "{}",
                        case["name"]
                    );
                    assert_eq!(
                        serde_json::to_value(location).unwrap(),
                        case["location"],
                        "{}",
                        case["name"]
                    );
                }
            }
        }
    }
    #[test]
    fn decoding_occurs_once_and_query_authority_is_not_imported() {
        let Parsed::Query(q) = parse_target(
            "GET",
            "/api/graph/node/a%252Fb/path?source=ignored&target=%252F&work_limit=999999999&resource_profile=forged",
            "testserver",
        ) else {
            panic!("expected query")
        };
        assert_eq!(q.source, "a%2Fb");
        assert_eq!(q.target, "%2F");
        assert_eq!(
            serde_json::to_value(&q).unwrap().as_object().unwrap().len(),
            5
        );
        let Parsed::Reply { status, body, .. } = parse_target(
            "GET",
            "/api/graph/path?algorithm=other&directed=bad",
            "testserver",
        ) else {
            panic!("expected errors")
        };
        assert_eq!(status, 422);
        assert_eq!(body.unwrap()["detail"].as_array().unwrap().len(), 4);
    }
}
