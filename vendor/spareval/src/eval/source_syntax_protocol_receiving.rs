//! Frozen original syntax observations. Nondeterministic constructor replay
//! is explicitly scoped; it never changes an original expected outcome.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_syntax_protocol_query_cases.json"
    )
}
fn receive(id: &str) {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_fixture().as_bytes())),
        "24c6bf443497e7311b660deada16af9bc49f12ec7bf5344cf5679d19710c6536"
    );
    let root: Value = serde_json::from_str(private_fixture()).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    let cases = root["graph_queries"].as_array().unwrap();
    assert_eq!(cases.len(), 56);
    let case = cases.iter().find(|v| v["input"]["id"] == id).unwrap();
    receive_case(case, &root["data_inputs"]);
}

pub(super) fn receive_case(case: &Value, data_inputs: &Value) {
    let id = case["input"]["id"].as_str().unwrap();
    // Original trace distinguishes explicit BNode(label) from BNode(None).
    // Replaying recorded constructor results conditions nondeterminism; neither
    // the candidate nor this adapter rewrites an expected error or result.
    let mut generated = Vec::new();
    let mut call = None;
    for event in case["construction_events"].as_array().unwrap() {
        if event["stage"] != "parse_query" || event["name"] != "BNode" {
            continue;
        }
        match event["event"].as_str().unwrap() {
            "call" => {
                assert!(call.is_none());
                call = Some(event["value"].is_null());
            }
            "return" => {
                if call.take().unwrap() {
                    assert_eq!(event["value"]["kind"], "BNode");
                    generated.push(
                        oxrdf::BlankNode::new(event["value"]["value"].as_str().unwrap()).unwrap(),
                    );
                }
            }
            _ => panic!("unreceived original BNode construction trace"),
        }
    }
    assert!(call.is_none());
    if generated.is_empty() {
        source_bgp_receiving::receive_query_case(case, data_inputs);
    } else {
        let remaining = Rc::new(std::cell::RefCell::new(std::collections::VecDeque::from(
            generated,
        )));
        let next = Rc::clone(&remaining);
        let construct: spargebra::SourceBlankNodeConstructor = Rc::new(move || {
            next.borrow_mut().pop_front().ok_or_else(|| {
                Box::new(RawNumericFault::InvalidSourceOrigin)
                    as spargebra::SourceLiteralBridgeError
            })
        });
        source_bgp_receiving::receive_query_case_with_constructor(case, data_inputs, construct);
        assert!(
            remaining.borrow().is_empty(),
            "{id} original construction count/order not consumed"
        );
    }
}

#[test]
fn source_syntax_sc01() {
    receive("SC01");
}

#[test]
fn source_syntax_sc02() {
    receive("SC02");
}

#[test]
fn source_syntax_sc03() {
    receive("SC03");
}

#[test]
fn source_syntax_sc04() {
    receive("SC04");
}

#[test]
fn source_syntax_sc05() {
    receive("SC05");
}

#[test]
fn source_syntax_sc06() {
    receive("SC06");
}

#[test]
fn source_syntax_sc07() {
    receive("SC07");
}

#[test]
fn source_syntax_sc08() {
    receive("SC08");
}

#[test]
fn source_syntax_sc09() {
    receive("SC09");
}

#[test]
fn source_syntax_sc10() {
    receive("SC10");
}

#[test]
fn source_syntax_sc11() {
    receive("SC11");
}

#[test]
fn source_syntax_sc12() {
    receive("SC12");
}

#[test]
fn source_syntax_sc13() {
    receive("SC13");
}

#[test]
fn source_syntax_sc14() {
    receive("SC14");
}

#[test]
fn source_syntax_sc15() {
    receive("SC15");
}

#[test]
fn source_syntax_sc16() {
    receive("SC16");
}

#[test]
fn source_syntax_sc17() {
    receive("SC17");
}

#[test]
fn source_syntax_sc18() {
    receive("SC18");
}

#[test]
fn source_syntax_sc19() {
    receive("SC19");
}

#[test]
fn source_syntax_sc20() {
    receive("SC20");
}

#[test]
fn source_syntax_sw01() {
    receive("SW01");
}

#[test]
fn source_syntax_sw02() {
    receive("SW02");
}

#[test]
fn source_syntax_sw03() {
    receive("SW03");
}

#[test]
fn source_syntax_sw04() {
    receive("SW04");
}

#[test]
fn source_syntax_sw05() {
    receive("SW05");
}

#[test]
fn source_syntax_sw06() {
    receive("SW06");
}

#[test]
fn source_syntax_sw07() {
    receive("SW07");
}

#[test]
fn source_syntax_sw08() {
    receive("SW08");
}

#[test]
fn source_syntax_sp01() {
    receive("SP01");
}

#[test]
fn source_syntax_sp02() {
    receive("SP02");
}

#[test]
fn source_syntax_sp03() {
    receive("SP03");
}

#[test]
fn source_syntax_sp04() {
    receive("SP04");
}

#[test]
fn source_syntax_sp05() {
    receive("SP05");
}

#[test]
fn source_syntax_sp06() {
    receive("SP06");
}

#[test]
fn source_syntax_sp07() {
    receive("SP07");
}

#[test]
fn source_syntax_sp08() {
    receive("SP08");
}

#[test]
fn source_syntax_sp09() {
    receive("SP09");
}

#[test]
fn source_syntax_sp10() {
    receive("SP10");
}

#[test]
fn source_syntax_sp11() {
    receive("SP11");
}

#[test]
fn source_syntax_sp12() {
    receive("SP12");
}

#[test]
fn source_syntax_sp13() {
    receive("SP13");
}

#[test]
fn source_syntax_sp14() {
    receive("SP14");
}

#[test]
fn source_syntax_sp15() {
    receive("SP15");
}

#[test]
fn source_syntax_sp16() {
    receive("SP16");
}

#[test]
fn source_syntax_sp17() {
    receive("SP17");
}

#[test]
fn source_syntax_sp18() {
    receive("SP18");
}

#[test]
fn source_syntax_sp19() {
    receive("SP19");
}

#[test]
fn source_syntax_sp20() {
    receive("SP20");
}

#[test]
fn source_syntax_sp21() {
    receive("SP21");
}

#[test]
fn source_syntax_sp22() {
    receive("SP22");
}

#[test]
fn source_syntax_sp23() {
    receive("SP23");
}

#[test]
fn source_syntax_sp24() {
    receive("SP24");
}

#[test]
fn source_syntax_sp25() {
    receive("SP25");
}

#[test]
fn source_syntax_sp26() {
    receive("SP26");
}

#[test]
fn source_syntax_sp27() {
    receive("SP27");
}

#[test]
fn source_syntax_sp28() {
    receive("SP28");
}

fn control_graph(
    session: &Rc<QueryNumericSession>,
) -> crate::dataset::source_memory::SourceMemoryGraph {
    let root: Value = serde_json::from_str(private_fixture()).unwrap();
    source_bgp_receiving::dataset_inputs(session, None, &root["data_inputs"])
}

fn control_collection_error(constructor: Option<spargebra::SourceBlankNodeConstructor>) -> String {
    let session = source_bgp_receiving::session();
    let prepared = numeric_query::prepare_source_graph_numeric_query_with_constructor(
        "SELECT (EXISTS { ?s <urn:p> (1 2) } AS ?e) WHERE {}",
        control_graph(&session),
        session,
        Vec::new(),
        constructor,
    )
    .unwrap_or_else(|e| panic!("preparation: {e:?}"));
    let mut rows = prepared.execute_source_presence().unwrap();
    let error = rows
        .next()
        .unwrap()
        .err()
        .expect("original CompValue error");
    let QueryEvaluationError::Unexpected(error) = error else {
        panic!("error class")
    };
    let error = error
        .downcast_ref::<source_compvalue::SourceCompValueError>()
        .unwrap();
    assert_eq!((error.module, error.class), ("builtins", "Exception"));
    error.argument.clone()
}

#[test]
fn source_syntax_control_legacy_constructor_not_called() {
    let calls = Rc::new(std::cell::Cell::new(0));
    let counter = Rc::clone(&calls);
    let constructor: spargebra::SourceBlankNodeConstructor = Rc::new(move || {
        counter.set(counter.get() + 1);
        Err(Box::new(RawNumericFault::InvalidSourceOrigin))
    });
    assert!(
        spargebra::SparqlParser::new()
            .with_source_blank_node_constructor(constructor)
            .parse_query("SELECT * WHERE { ?s <urn:p> [] }")
            .is_ok()
    );
    assert_eq!(calls.get(), 0);
}

#[derive(Debug)]
struct OriginalConstructorFailure;
impl std::fmt::Display for OriginalConstructorFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("external constructor failed")
    }
}
impl std::error::Error for OriginalConstructorFailure {}

#[test]
fn source_syntax_control_constructor_failure_not_relabelled() {
    let session = source_bgp_receiving::session();
    let constructor: spargebra::SourceBlankNodeConstructor =
        Rc::new(|| Err(Box::new(OriginalConstructorFailure)));
    // The missing final brace must not replace the constructor failure that
    // already happened during source parsing.
    let error = match numeric_query::prepare_source_graph_numeric_query_with_constructor(
        "SELECT * WHERE { ?s <urn:p> (1 2)",
        control_graph(&session),
        session,
        Vec::new(),
        Some(constructor),
    ) {
        Err(e) => e,
        Ok(_) => panic!("failed constructor accepted"),
    };
    let numeric_query::NumericPrepareError::Query(QueryEvaluationError::Unexpected(error)) = error
    else {
        panic!("constructor was relabelled: {error:?}")
    };
    assert!(error.downcast_ref::<OriginalConstructorFailure>().is_some());
}

#[test]
fn source_syntax_control_failure_then_new_scope() {
    let session = source_bgp_receiving::session();
    let count = Rc::new(std::cell::Cell::new(0));
    let counter = Rc::clone(&count);
    let constructor: spargebra::SourceBlankNodeConstructor = Rc::new(move || {
        let value = counter.get();
        counter.set(value + 1);
        if value == 0 {
            Ok(oxrdf::BlankNode::new("Nrecorded").unwrap())
        } else {
            Err(Box::new(OriginalConstructorFailure))
        }
    });
    assert!(
        numeric_query::prepare_source_graph_numeric_query_with_constructor(
            "SELECT * WHERE { ?s <urn:p> (1 2) }",
            control_graph(&session),
            Rc::clone(&session),
            Vec::new(),
            Some(constructor),
        )
        .is_err()
    );
    assert_eq!(count.get(), 2);
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        "SELECT ?s WHERE { ?s <urn:ex:q> <urn:ex:b> }",
        control_graph(&session),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let mut rows = prepared.execute_source_presence().unwrap();
    let row = rows.next().unwrap().unwrap();
    assert_eq!(row.variables.len(), 1);
    let Some(SolutionValue::Rdf(value)) = &row.values[0] else {
        panic!("RDF cell")
    };
    assert_eq!(
        value.rdf().unwrap(),
        &oxrdf::Term::NamedNode(oxrdf::NamedNode::new("urn:ex:a").unwrap())
    );
    assert!(rows.next().is_none());
    assert_eq!(
        session.flags(),
        semantica_numeric::DecimalSignals::default()
    );
}

fn control_labels(text: &str) -> Vec<String> {
    text.split("rdflib.term.BNode('")
        .skip(1)
        .map(|v| v.split_once("')").unwrap().0.to_owned())
        .collect()
}

#[test]
fn source_syntax_control_fresh_identity_domain() {
    let first = control_labels(&control_collection_error(None));
    let second = control_labels(&control_collection_error(None));
    let a: std::collections::BTreeSet<_> = first.iter().collect();
    let b: std::collections::BTreeSet<_> = second.iter().collect();
    assert_eq!(a.len(), 2);
    assert_eq!(b.len(), 2);
    assert_ne!(a, b, "fresh construction returned a fixed replay");
    for value in first.iter().chain(&second) {
        assert_eq!(value.len(), 33);
        assert!(value.starts_with('N'));
        assert!(
            value[1..]
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(value.as_bytes()[13], b'4');
        assert!(matches!(value.as_bytes()[17], b'8' | b'9' | b'a' | b'b'));
    }
}

#[test]
fn source_syntax_control_explicit_name_does_not_draw() {
    let root: Value = serde_json::from_str(private_fixture()).unwrap();
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == "SC15")
        .unwrap();
    source_bgp_receiving::receive_query_case_with_constructor(
        case,
        &root["data_inputs"],
        Rc::new(|| panic!("explicit source BNode(label) requested random construction")),
    );
}

#[test]
fn source_syntax_control_unicode_unreceived_is_distinct() {
    let session = source_bgp_receiving::session();
    let result = numeric_query::prepare_source_graph_numeric_query(
        r#"SELECT * WHERE { ?s <urn:p> "\uD800" }"#,
        control_graph(&session),
        session,
        Vec::new(),
    );
    assert!(matches!(
        result,
        Err(numeric_query::NumericPrepareError::SourceSyntax(
            spargebra::SourceParserError::Unreceived("isolated Unicode surrogate")
        ))
    ));
}

#[test]
fn source_syntax_control_repeated_constructor_value_keeps_alias() {
    let calls = Rc::new(std::cell::Cell::new(0));
    let counter = Rc::clone(&calls);
    let constructor: spargebra::SourceBlankNodeConstructor = Rc::new(move || {
        counter.set(counter.get() + 1);
        Ok(oxrdf::BlankNode::new("Nfixed").unwrap())
    });
    let text = control_collection_error(Some(constructor));
    assert_eq!(calls.get(), 2, "constructor calls were coalesced");
    let labels = control_labels(&text);
    assert!(labels.len() > 2);
    assert!(labels.iter().all(|value| value == "Nfixed"));
}
