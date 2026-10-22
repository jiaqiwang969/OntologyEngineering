//! Original output frozen before Rust writer implementation. No oracle repair.
use super::super::RdfFormat;
use super::*;
use serde_json::{Value, json};
fn req(a: &Value) -> SemanticSourceSerializeRequest {
    SemanticSourceSerializeRequest {
        format: a["format"].as_str().unwrap().into(),
        graph_name: a["graph_name"].as_str().map(str::to_owned),
        base_uri: a["base_uri"].as_str().map(str::to_owned),
    }
}
fn receive(cases: &[Value]) -> (usize, usize, Vec<String>) {
    let mut actions = 0;
    let mut texts = 0;
    let mut failures = Vec::new();
    for case in cases {
        let mut s = DatasetSession::new_source_update_internal(true);
        for (i, row) in case["rows"].as_array().unwrap().iter().enumerate() {
            let a = &row["action"];
            let before = (
                s.rdflib_projection.quads.clone(),
                s.revision,
                s.query_cache.len(),
            );
            let result = (|| -> Result<Value> {
                Ok(match a["op"].as_str().unwrap() {
                    "construct" => serde_json::to_value(s.load_construct_source_internal(
                        a,
                        None,
                        a["graph_name"].as_str(),
                    )?)?,
                    "load" => serde_json::to_value(s.load_source_internal(
                        a["data"].as_str().unwrap().as_bytes(),
                        if a["format"] == "turtle" {
                            RdfFormat::Turtle
                        } else {
                            RdfFormat::NTriples
                        },
                        None,
                        a["graph_name"].as_str(),
                    )?)?,
                    "clear" => serde_json::to_value(s.clear_rdflib(None)?)?,
                    "serialize" => {
                        texts += 1;
                        json!(s.serialize_source_turtle_internal(&req(a))?)
                    }
                    _ => panic!("frozen action"),
                })
            })();
            let actual = match result {
                Ok(value) => json!({"status":"completed","value":value}),
                Err(e) => {
                    if let Some(e) = e.downcast_ref::<SourceFailure>() {
                        json!({"status":"original_error","error":{"module":e.module,"type":e.class,"message":e.to_string()}})
                    } else {
                        json!({"status":"rust_error","message":format!("{e:#}")})
                    }
                }
            };
            if a["op"] == "serialize" {
                assert_eq!(
                    (
                        s.rdflib_projection.quads.clone(),
                        s.revision,
                        s.query_cache.len()
                    ),
                    before,
                    "serialization mutation {}",
                    case["id"]
                );
            }
            if actual != row["result"]
                || json!(s.source_namespaces.bindings()) != row["bindings"]
                || json!(s.revision) != row["revision"]
            {
                failures.push(format!(
                    "{} action{} expected={} actual={} bindings_expected={} bindings_actual={}",
                    case["id"],
                    i,
                    row["result"],
                    actual,
                    row["bindings"],
                    json!(s.source_namespaces.bindings())
                ));
            }
            println!(
                "OE_TURTLE44 {}",
                json!({"case":case["id"],"action":i,"matched":actual==row["result"],"actual":actual,"revision":s.revision})
            );
            actions += 1;
        }
    }
    (actions, texts, failures)
}
#[test]
fn source_turtle_writer_original_48_cases() {
    let text = crate::private_test_fixtures::text_ref!("source-turtle-44/original.json");
    assert_eq!(
        crate::sha256(text.as_bytes()),
        "4dcc176e4e3846ea1473df506f64f3a43ca6a797c73db2d8baf69c92c7d7cb05"
    );
    let f: Value = serde_json::from_str(text).unwrap();
    let (actions, texts, failures) = receive(f["cases"].as_array().unwrap());
    assert_eq!(actions, 102);
    assert_eq!(texts, 52);
    assert!(
        failures.is_empty(),
        "{} frozen mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
#[test]
fn source_turtle_writer_previous_wrapper_full_text() {
    let f: Value = serde_json::from_str(crate::private_test_fixtures::text_ref!(
        "source-turtle-43/original.json"
    ))
    .unwrap();
    let (actions, texts, failures) = receive(f["wrapper_cases"].as_array().unwrap());
    assert_eq!(actions, 13);
    assert_eq!(texts, 8);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
#[test]
fn source_turtle_writer_original_lifecycle_and_guard_premise() {
    let text = crate::private_test_fixtures::text_ref!("source-turtle-44/supplement.json");
    assert_eq!(
        crate::sha256(text.as_bytes()),
        "aaf87cae24139e14395a90d2d9fc74cb87167fe9610c59c70d94adaaa6cdef95"
    );
    let f: Value = serde_json::from_str(text).unwrap();
    let (actions, texts, failures) = receive(f["cases"].as_array().unwrap());
    assert_eq!(actions, 13);
    assert_eq!(texts, 7);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
fn iri(s: &str) -> Value {
    json!({"term_type":"iri","value":s})
}
fn blank(s: &str) -> Value {
    json!({"term_type":"blank_node","value":s})
}
fn triple(s: Value, p: Value, o: Value) -> Value {
    json!({"subject":s,"predicate":p,"object":o})
}
fn request() -> SemanticSourceSerializeRequest {
    req(&json!({"format":"turtle"}))
}
#[test]
fn source_turtle_writer_cycles_and_local_work_are_explicit_refusals() {
    let mut s = DatasetSession::new_source_update_internal(true);
    let rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    s.load_construct_source_internal(
        &json!({"triples":[
            // A one-reference prefix enters the cycle. The former two-node
            // graph is independently frozen above as a finite original case.
            triple(iri("urn:s"),iri("urn:p"),blank("x")),
            triple(blank("x"),iri(&format!("{rdf}first")),iri("urn:item")),
            triple(blank("x"),iri(&format!("{rdf}rest")),blank("a")),
            triple(blank("a"),iri(&format!("{rdf}first")),iri("urn:item")),
            triple(blank("a"),iri(&format!("{rdf}rest")),blank("b")),
            triple(blank("b"),iri(&format!("{rdf}first")),iri("urn:item")),
            triple(blank("b"),iri(&format!("{rdf}rest")),blank("a"))
        ]}),
        None,
        None,
    )
    .unwrap();
    let before = (s.rdflib_projection.quads.clone(), s.revision);
    let error = s.serialize_source_turtle_internal(&request()).unwrap_err();
    assert!(
        error.to_string().contains("source_turtle_cyclic_list"),
        "{error}"
    );
    assert!(error.downcast_ref::<SourceFailure>().is_none());
    assert_eq!((s.rdflib_projection.quads.clone(), s.revision), before);
    let g = s
        .rdflib_projection
        .source_state
        .as_mut()
        .unwrap()
        .turtle_view(None, 100, 1_000_000, 1)
        .unwrap();
    assert!(
        g.scan([None, None, None])
            .err()
            .expect("bounded scan")
            .to_string()
            .contains("source_turtle_work_budget")
    );
}
#[test]
fn source_turtle_writer_output_and_depth_are_bounded() {
    let mut s = DatasetSession::new_source_update_internal(true);
    let mut triples = vec![triple(iri("urn:s"), iri("urn:p"), blank("b0"))];
    for i in 0..130 {
        triples.push(triple(
            blank(&format!("b{i}")),
            iri("urn:p"),
            blank(&format!("b{}", i + 1)),
        ));
    }
    s.load_construct_source_internal(&json!({"triples":triples}), None, None)
        .unwrap();
    let before = (s.rdflib_projection.quads.clone(), s.revision);
    let error = s.serialize_source_turtle_internal(&request()).unwrap_err();
    assert!(
        error.to_string().contains("source_turtle_depth_budget"),
        "{error}"
    );
    let prepared = s.prepare_source_turtle_internal(&request()).unwrap();
    let graph = s
        .rdflib_projection
        .source_state
        .as_mut()
        .unwrap()
        .turtle_view(None, 1000, 1_000_000, 100000)
        .unwrap();
    let error = super::super::source_turtle_writer::Writer::new(
        prepared,
        graph,
        &mut s.source_namespaces,
        false,
        128,
        1000,
    )
    .render()
    .unwrap_err();
    assert!(
        error.to_string().contains("source_turtle_output_budget"),
        "{error}"
    );
    assert_eq!((s.rdflib_projection.quads.clone(), s.revision), before);
}
