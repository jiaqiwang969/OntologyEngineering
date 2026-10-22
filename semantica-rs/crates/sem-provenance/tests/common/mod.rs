//! Shared support for the provenance test suite (the counterpart of `tests/provenance/__init__.py`).
//!
//! Test coverage: W3C PROV-O schema compliance, storage backends (InMemory, SQLite),
//! ProvenanceManager functionality, integrity verification, bridge axiom tracking, backward
//! compatibility and performance (batched transactions).
//!
//! `fixtures/oracle.json` holds expected values produced by running the original Python module
//! (`fixtures/generate_oracle.py`, executed in the oracle environment). The oracle replaces
//! `datetime.utcnow()` with `2026-01-01T00:00:00 + n * 1.25 s`; [`fixed_clock`] installs the
//! same sequence for the Rust side.
#![allow(dead_code)]

use std::collections::BTreeMap;

use chrono::{NaiveDate, TimeDelta};
use once_cell::sync::Lazy;
use sem_provenance::ProvenanceEntry;
use sem_provenance::clock::{ClockGuard, fixed_clock as install_fixed_clock};
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

static ORACLE: Lazy<Value> = Lazy::new(|| {
    serde_json::from_str(include_str!("../fixtures/oracle.json")).expect("oracle fixture parses")
});

/// One section of the oracle fixture.
pub fn oracle(key: &str) -> &'static Value {
    let value = &ORACLE[key];
    assert!(!value.is_null(), "oracle fixture has no section {key}");
    value
}

/// The oracle's deterministic `datetime.utcnow()` sequence, installed for this thread.
pub fn fixed_clock() -> ClockGuard {
    let start = NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    install_fixed_clock(start, TimeDelta::microseconds(1_250_000))
}

/// Like [`fixed_clock`], also counting the clock readings (the oracle records how many times
/// `datetime.utcnow()` was called, which pins down when the port reads the clock).
pub struct CountingClock {
    count: std::rc::Rc<std::cell::Cell<i32>>,
    previous: Option<Option<sem_provenance::clock::ClockFn>>,
}

impl CountingClock {
    pub fn new() -> Self {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let count = std::rc::Rc::new(std::cell::Cell::new(0));
        let counter = count.clone();
        let previous = sem_provenance::clock::set_thread_clock(Some(Box::new(move || {
            let n = counter.get();
            counter.set(n + 1);
            start + TimeDelta::microseconds(1_250_000) * n
        })));
        CountingClock {
            count,
            previous: Some(previous),
        }
    }

    /// Number of readings so far.
    pub fn calls(&self) -> i64 {
        i64::from(self.count.get())
    }
}

impl Drop for CountingClock {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            sem_provenance::clock::set_thread_clock(previous);
        }
    }
}

/// A JSON object literal as a map.
pub fn obj(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("expected a JSON object, got {other}"),
    }
}

/// Exact JSON equality: same keys in the same order, same values, and ints stay distinct from
/// floats (`1` vs `1.0`), which `Value`'s order-insensitive `==` would not check.
#[track_caller]
pub fn assert_json_eq(actual: &Value, expected: &Value) {
    let a = serde_json::to_string_pretty(actual).unwrap();
    let e = serde_json::to_string_pretty(expected).unwrap();
    if a != e {
        panic!("JSON differs\n--- actual ---\n{a}\n--- expected ---\n{e}");
    }
}

/// The `{"error": type, "message": str}` form the fixture uses for exceptions.
pub fn err_json(err: &SemanticaError) -> Value {
    json!({"error": err.type_name(), "message": err.to_string()})
}

/// `{"ok": value}` or the error form, as the fixture's `call(...)` records results.
pub fn outcome<T: Into<Value>>(result: Result<T, SemanticaError>) -> Value {
    match result {
        Ok(value) => json!({"ok": value.into()}),
        Err(err) => err_json(&err),
    }
}

/// `[e.to_dict() for e in entries]`.
pub fn dicts(entries: &[ProvenanceEntry]) -> Value {
    Value::Array(entries.iter().map(|e| Value::Object(e.to_dict())).collect())
}

/// `[e.entity_id for e in entries]`.
pub fn ids(entries: &[ProvenanceEntry]) -> Vec<String> {
    entries.iter().map(|e| e.entity_id.clone()).collect()
}

/// `pytest.approx(expected, rel=rel)`.
pub fn approx(actual: f64, expected: f64, rel: f64) -> bool {
    (actual - expected).abs() <= (rel * expected.abs()).max(1e-12)
}

/// Replace every blank-node label (`_:N<hex>`) with `_:B` so statement sets compare across
/// runs (rdflib and the port both mint random labels).
pub fn mask_bnodes(text: &str) -> String {
    let re = regex::Regex::new(r"_:N[0-9a-f]{32}").unwrap();
    re.replace_all(text, "_:B").into_owned()
}

/// N-Triples output as a sorted multiset of lines with blank-node labels masked (rdflib's
/// statement order is random).
pub fn nt_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = mask_bnodes(text).lines().map(str::to_string).collect();
    lines.sort();
    lines
}

/// JSON-LD output as a sorted list of canonical node strings with blank-node ids masked
/// (rdflib's node order follows Python set iteration).
pub fn jsonld_nodes(text: &str) -> Vec<String> {
    let doc: Value = serde_json::from_str(&mask_bnodes(text)).expect("JSON-LD parses");
    let mut nodes: Vec<String> = doc
        .as_array()
        .expect("JSON-LD top level is a list")
        .iter()
        .map(|node| canonical(node).to_string())
        .collect();
    nodes.sort();
    nodes
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: BTreeMap<&String, Value> =
                map.iter().map(|(k, v)| (k, canonical(v))).collect();
            serde_json::to_value(sorted).unwrap()
        }
        Value::Array(items) => {
            let mut items: Vec<Value> = items.iter().map(canonical).collect();
            items.sort_by_key(|v| v.to_string());
            Value::Array(items)
        }
        other => other.clone(),
    }
}

/// Mask values that differ between runs: `chain_id` (uuid4).
pub fn mask_chain_id(mut value: Value) -> Value {
    if let Some(map) = value.as_object_mut()
        && map.contains_key("chain_id")
    {
        map.insert("chain_id".into(), json!("<uuid>"));
    }
    value
}

/// Parse RDF text with oxigraph into a canonicalised graph (blank nodes relabelled
/// canonically), for isomorphism checks.
pub fn canonical_graph(text: &str, format: oxigraph::io::RdfFormat) -> oxigraph::model::Graph {
    let mut graph = oxigraph::model::Graph::new();
    for quad in oxigraph::io::RdfParser::from_format(format)
        .lenient()
        .for_slice(text.as_bytes())
    {
        let quad = quad.expect("serialisation parses back");
        graph.insert(oxigraph::model::TripleRef::new(
            &quad.subject,
            &quad.predicate,
            &quad.object,
        ));
    }
    graph.canonicalize(oxigraph::model::dataset::CanonicalizationAlgorithm::Unstable);
    graph
}

/// RDF/XML output with blank-node ids masked and `rdf:Description` blocks sorted (rdflib
/// writes subjects in set order).
pub fn xml_blocks(text: &str) -> (String, Vec<String>) {
    let re = regex::Regex::new(r#"nodeID="N[0-9a-f]{32}""#).unwrap();
    let masked = re.replace_all(text, "nodeID=\"B\"").into_owned();
    let body_start = masked.find("  <rdf:Description").unwrap_or(masked.len());
    let header = masked[..body_start].to_string();
    let mut blocks: Vec<String> = masked[body_start..]
        .split("  </rdf:Description>\n")
        .map(str::to_string)
        .collect();
    blocks.sort();
    (header, blocks)
}

/// Compare `manager.export_prov(...)` outputs with the oracle's `{"turtle", "ntriples",
/// "jsonld"}` results: Turtle exactly (or, with `turtle_isomorphic`, as canonicalised graphs —
/// for graphs whose sibling blank nodes rdflib orders by random ids), N-Triples and JSON-LD as
/// statement/node sets with blank-node labels masked, errors exactly.
pub fn assert_exports_match(
    manager: &sem_provenance::ProvenanceManager,
    base_uri: Option<&str>,
    expected: &Value,
    turtle_isomorphic: bool,
) {
    for format in ["turtle", "ntriples", "jsonld"] {
        let got = manager.export_prov(format, base_uri);
        let want = &expected[format];
        match (got, want) {
            (Err(err), want) => assert_eq!(err_json(&err), *want, "{format}"),
            (Ok(text), Value::String(want)) => match format {
                "turtle" if turtle_isomorphic => assert_eq!(
                    canonical_graph(&text, oxigraph::io::RdfFormat::Turtle),
                    canonical_graph(want, oxigraph::io::RdfFormat::Turtle),
                    "turtle graphs differ\n--- actual ---\n{text}\n--- expected ---\n{want}"
                ),
                "turtle" => assert_eq!(text, *want, "turtle"),
                "ntriples" => assert_eq!(nt_lines(&text), nt_lines(want), "ntriples"),
                _ => assert_eq!(jsonld_nodes(&text), jsonld_nodes(want), "jsonld"),
            },
            (Ok(text), want) => panic!("{format}: got output {text:?}, oracle has {want}"),
        }
    }
}
