//! Frozen original text/error expectations plus independent altered-result controls.
use super::*;
use crate::dataset::{RdfFormat, source_failure::SourceFailure};
use serde_json::{Value, json};
use std::time::Duration;
fn until() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
fn request() -> Request {
    Request {
        format: "turtle".into(),
        graph_name: None,
        base_uri: None,
    }
}
fn quads(s: &DatasetSession) -> Vec<Row> {
    let mut rows = Vec::new();
    s.rdflib_projection
        .source_state
        .as_ref()
        .unwrap()
        .visit_quads(|(t, g)| {
            rows.push(owned_row(t, g));
            Ok(())
        })
        .unwrap();
    rows
}
fn state(s: &DatasetSession) -> Value {
    serde_json::to_value(
        s.source_namespaces
            .capture_until(CAPTURE_BYTES, until())
            .unwrap(),
    )
    .unwrap()
}
fn receive(cases: &[Value]) -> (usize, usize, usize) {
    let (mut actions, mut complete, mut original_errors) = (0, 0, 0);
    for case in cases {
        let mut s = DatasetSession::new_source_update_internal(true);
        for (i, r) in case["rows"].as_array().unwrap().iter().enumerate() {
            let a = &r["action"];
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
                        let req = Request {
                            format: a["format"].as_str().unwrap().into(),
                            graph_name: a["graph_name"].as_str().map(str::to_owned),
                            base_uri: a["base_uri"].as_str().map(str::to_owned),
                        };
                        let at = until();
                        let (text, w) = s.capture_turtle(&req, CAPTURE_BYTES, at)?;
                        let after = state(&s);
                        let rows = quads(&s);
                        validate_turtle_witness(&req, &text, &w, &rows, CAPTURE_BYTES, at)
                            .unwrap_or_else(|e| panic!("{} action{i} reception:{e:#}", case["id"]));
                        assert_eq!(state(&s), after, "receiver changed live namespace");
                        complete += 1;
                        json!(text)
                    }
                    _ => panic!("frozen original operation"),
                })
            })();
            let actual = match result {
                Ok(v) => json!({"status":"completed","value":v}),
                Err(e) => {
                    if let Some(e) = e.downcast_ref::<SourceFailure>() {
                        original_errors += 1;
                        json!({"status":"original_error","error":{"module":e.module,"type":e.class,"message":e.to_string()}})
                    } else {
                        json!({"status":"rust_error","message":format!("{e:#}")})
                    }
                }
            };
            assert_eq!(actual, r["result"], "{} action{i}", case["id"]);
            assert_eq!(
                json!(s.source_namespaces.bindings()),
                r["bindings"],
                "{} action{i} bindings",
                case["id"]
            );
            assert_eq!(json!(s.revision), r["revision"]);
            if a["op"] == "serialize" {
                assert_eq!(
                    (
                        s.rdflib_projection.quads.clone(),
                        s.revision,
                        s.query_cache.len()
                    ),
                    before
                );
            }
            actions += 1;
        }
    }
    (actions, complete, original_errors)
}
#[test]
fn source_turtle47_original_text_and_transcript() {
    let originals = [
        (
            crate::private_test_fixtures::text_ref!("source-turtle-44/original.json"),
            "cases",
            "4dcc176e4e3846ea1473df506f64f3a43ca6a797c73db2d8baf69c92c7d7cb05",
        ),
        (
            crate::private_test_fixtures::text_ref!("source-turtle-44/supplement.json"),
            "cases",
            "aaf87cae24139e14395a90d2d9fc74cb87167fe9610c59c70d94adaaa6cdef95",
        ),
        (
            crate::private_test_fixtures::text_ref!("source-turtle-43/original.json"),
            "wrapper_cases",
            "36e8735cd217a4b5804907a166cf393d5e073a918580ffd25c2a9566ffd5ff02",
        ),
    ];
    let (mut actions, mut complete, mut errors) = (0, 0, 0);
    for (text, key, hash) in originals {
        assert_eq!(crate::sha256(text.as_bytes()), hash);
        let f: Value = serde_json::from_str(text).unwrap();
        let (a, b, c) = receive(f[key].as_array().unwrap());
        actions += a;
        complete += b;
        errors += c;
    }
    assert_eq!(actions, 128);
    println!(
        "OE_TURTLE47_ORIGINAL {}",
        json!({"actions":actions,"complete_witnesses":complete,"original_errors":errors})
    );
}
fn sample(value: &str) -> DatasetSession {
    let mut s = DatasetSession::new_source_update_internal(true);
    let i = |v: &str| json!({"term_type":"iri","value":v});
    let b = |v: &str| json!({"term_type":"blank_node","value":v});
    let l = |v: &str| json!({"term_type":"literal","value":v,"datatype":"http://www.w3.org/2001/XMLSchema#integer"});
    let t = |s, p, o| json!({"subject":s,"predicate":p,"object":o});
    s.load_construct_source_internal(&json!({"triples":[
        t(i("urn:s"),i("urn:p"),l(value)),t(i("urn:s"),i("urn:p"),l("1")),
        t(i("urn:s"),i("urn:list"),b("head")),
        t(b("head"),i("http://www.w3.org/1999/02/22-rdf-syntax-ns#first"),i("urn:item")),
        t(b("head"),i("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest"),i("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil")),
    ]}),None,None).unwrap();
    s
}
#[test]
fn source_turtle47_rejects_frozen_result_and_read_tampering() {
    let mut s = sample("2");
    let req = request();
    let at = until();
    let (text, w) = s.capture_turtle(&req, CAPTURE_BYTES, at).unwrap();
    let rows = quads(&s);
    validate_turtle_witness(&req, &text, &w, &rows, CAPTURE_BYTES, at).unwrap();
    let wire = serde_json::to_vec(&w).unwrap();
    let decoded: Witness = serde_json::from_slice(&wire).unwrap();
    assert_eq!(decoded, w);
    validate_turtle_witness(&req, &text, &decoded, &rows, CAPTURE_BYTES, at).unwrap();
    let mut unknown = serde_json::to_value(&w).unwrap();
    unknown["uncontracted_claim"] = json!(true);
    assert!(serde_json::from_value::<Witness>(unknown).is_err());
    let mut mutations: Vec<(&str, Witness)> = Vec::new();
    macro_rules! alter {
        ($name:expr,$f:expr) => {{
            let mut m = w.clone();
            ($f)(&mut m);
            mutations.push(($name, m));
        }};
    }
    alter!("schema", |m: &mut Witness| m.schema.push('x'));
    alter!("request", |m: &mut Witness| m.request_sha256 =
        "0".repeat(64));
    alter!("text_digest", |m: &mut Witness| m.text_sha256 =
        "0".repeat(64));
    alter!("missing_read", |m: &mut Witness| {
        m.reads.remove(0);
    });
    alter!("extra_read", |m: &mut Witness| m
        .reads
        .push(m.reads.last().unwrap().clone()));
    alter!("read_order", |m: &mut Witness| m.reads.swap(0, 1));
    alter!("pattern", |m: &mut Witness| m.reads[0].subject =
        Some(owned_term(SourceTermInput::Iri("urn:wrong"))));
    alter!("missing_row", |m: &mut Witness| {
        m.reads[0].rows.pop();
    });
    alter!("extra_row", |m: &mut Witness| {
        let row = m.reads[0].rows[0].clone();
        m.reads[0].rows.push(row);
    });
    alter!("row_order", |m: &mut Witness| m.reads[0].rows.reverse());
    alter!("row_graph", |m: &mut Witness| m.reads[0].rows[0].graph =
        Some(owned_term(SourceTermInput::Iri("urn:wrong"))));
    alter!("read_kind", |m: &mut Witness| m.reads[0].operation =
        "truth".into());
    alter!("scan_answer", |m: &mut Witness| m.reads[0].answer =
        Some(false));
    alter!("cached_fact", |m: &mut Witness| {
        let r = m
            .reads
            .iter_mut()
            .find(|r| r.operation == "cached")
            .unwrap();
        r.answer = Some(!r.answer.unwrap());
    });
    alter!("comparison", |m: &mut Witness| {
        let r = m.reads.iter_mut().find(|r| r.operation == "less").unwrap();
        r.answer = Some(!r.answer.unwrap());
    });
    alter!("truth", |m: &mut Witness| {
        let r = m.reads.iter_mut().find(|r| r.operation == "truth").unwrap();
        r.answer = Some(!r.answer.unwrap());
    });
    alter!("scalar_argument", |m: &mut Witness| {
        let r = m
            .reads
            .iter_mut()
            .find(|r| r.operation == "cached")
            .unwrap();
        r.arguments[0].value = "999".into();
    });
    alter!("preparation", |m: &mut Witness| {
        m.preparation_rows.pop();
    });
    alter!("registration", |m: &mut Witness| m
        .default_registered_before =
        !m.default_registered_before);
    alter!("prepared_names", |m: &mut Witness| m
        .namespace_prepared
        .bindings[0]
        .namespace
        .push('x'));
    alter!("after_names", |m: &mut Witness| m
        .namespace_after
        .bindings
        .last_mut()
        .unwrap()
        .namespace
        .push('x'));
    alter!("charge", |m: &mut Witness| m.namespace_bytes_after += 1);
    for (name, m) in &mutations {
        assert!(
            validate_turtle_witness(&req, &text, m, &rows, CAPTURE_BYTES, at).is_err(),
            "accepted {name}"
        );
    }
    let altered = format!("{text} ");
    let mut m = w.clone();
    m.text_sha256 = crate::sha256(altered.as_bytes());
    assert!(validate_turtle_witness(&req, &altered, &m, &rows, CAPTURE_BYTES, at).is_err());
    let mut different = req.clone();
    different.base_uri = Some("urn:".into());
    assert!(validate_turtle_witness(&different, &text, &w, &rows, CAPTURE_BYTES, at).is_err());
    let mut incomplete = rows.clone();
    incomplete.pop();
    assert!(validate_turtle_witness(&req, &text, &w, &incomplete, CAPTURE_BYTES, at).is_err());
    println!(
        "OE_TURTLE47_TAMPER {}",
        json!({"rejected":mutations.len()+3})
    );
}
#[test]
fn source_turtle47_limits_and_partial_failures() {
    let mut s = sample("2");
    let req = request();
    let before = (quads(&s), s.revision, state(&s));
    assert!(
        s.capture_turtle(&req, CAPTURE_BYTES, Instant::now())
            .is_err()
    );
    assert!(s.capture_turtle(&req, 0, until()).is_err());
    assert_eq!((quads(&s), s.revision, state(&s)), before);
    assert!(s.capture_turtle(&req, 1500, until()).is_err());
    assert_eq!((quads(&s), s.revision), (before.0.clone(), before.1));
    // The actual preparation ran before capture refused. Do not restore old state.
    assert_ne!(state(&s), before.2);
    let at = until();
    let (text, w) = s.capture_turtle(&req, CAPTURE_BYTES, at).unwrap();
    let rows = quads(&s);
    assert!(
        validate_turtle_witness(&req, &text, &w, &rows, CAPTURE_BYTES, Instant::now()).is_err()
    );
    assert!(validate_turtle_witness(&req, &text, &w, &rows, 128, at).is_err());
    validate_turtle_witness(&req, &text, &w, &rows, CAPTURE_BYTES, at).unwrap();
    let replay = Replay {
        reads: &w.reads,
        rows: &w.preparation_rows,
        cursor: Cell::new(0),
        work: Cell::new(1),
        at,
    };
    assert!(
        (&replay)
            .scan([None, None, None])
            .unwrap_err()
            .to_string()
            .contains("source_turtle_work_budget")
    );

    let i = |v: &str| json!({"term_type":"iri","value":v});
    let b = |v: &str| json!({"term_type":"blank_node","value":v});
    let t = |s, p, o| json!({"subject":s,"predicate":p,"object":o});
    let first = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    let rest = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    let cycle = vec![
        t(i("urn:s"), i("urn:p"), b("x")),
        t(b("x"), i(first), i("urn:item")),
        t(b("x"), i(rest), b("a")),
        t(b("a"), i(first), i("urn:item")),
        t(b("a"), i(rest), b("b")),
        t(b("b"), i(first), i("urn:item")),
        t(b("b"), i(rest), b("a")),
    ];
    let mut deep = vec![t(i("urn:s"), i("urn:p"), b("b0"))];
    for n in 0..130 {
        deep.push(t(
            b(&format!("b{n}")),
            i("urn:p"),
            b(&format!("b{}", n + 1)),
        ));
    }
    for (triples, code) in [
        (cycle, "source_turtle_cyclic_list"),
        (deep, "source_turtle_depth_budget"),
    ] {
        let mut s = DatasetSession::new_source_update_internal(true);
        s.load_construct_source_internal(&json!({"triples":triples}), None, None)
            .unwrap();
        let before = (quads(&s), s.revision);
        let error = s.capture_turtle(&req, CAPTURE_BYTES, until()).unwrap_err();
        assert!(error.to_string().contains(code), "{error:#}");
        assert_eq!((quads(&s), s.revision), before);
    }
}
#[test]
fn source_turtle47_consistency_is_not_execution_provenance() {
    let req = request();
    let at = until();
    let mut a = sample("2");
    let mut b = sample("9");
    let (ta, wa) = a.capture_turtle(&req, CAPTURE_BYTES, at).unwrap();
    let (tb, wb) = b.capture_turtle(&req, CAPTURE_BYTES, at).unwrap();
    let qa = quads(&a);
    let qb = quads(&b);
    assert_ne!(ta, tb);
    validate_turtle_witness(&req, &ta, &wa, &qa, CAPTURE_BYTES, at).unwrap();
    assert!(validate_turtle_witness(&req, &tb, &wb, &qa, CAPTURE_BYTES, at).is_err());
    // A coherently substituted entire observation can be consistent. Only the
    // actual authenticated worker/request/source stream can distinguish origin.
    validate_turtle_witness(&req, &tb, &wb, &qb, CAPTURE_BYTES, at).unwrap();
}
