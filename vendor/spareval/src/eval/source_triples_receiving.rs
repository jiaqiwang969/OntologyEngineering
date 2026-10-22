//! Frozen original queries use the actual graph factory, parser, compiler and
//! iterator. Root-authored receiving is separate from independent acceptance.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use spargebra::SourceSyntaxValue as V;

const FIXTURE: &str = include_str!("fixtures/source_triples_query_cases.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "2fe7882c9855cfbf78a0eb906572b51d6b3a3274ee7c418f7e0a9f3d504fc1f4"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(value["graph_queries"].as_array().unwrap().len(), 21);
    assert_eq!(
        value["source_records"]["projection"],
        "4f356070da677f7caf46ba73bb53fcdc79d6dbd4ad9519eedf65db05792e7865"
    );
    assert_eq!(
        value["source_records"]["execution"],
        "360d96d8dc3368aaec9dd3baa64a4b0dd6ab5e8973474777dbb23d4f8119aa10"
    );
    value
}
fn receive(id: &str) {
    let root = fixture();
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    source_bgp_receiving::receive_query_case(case, &root["data_inputs"]);
}

#[test]
fn source_triples_old_exists_frontend_keeps_its_unqualified_container() {
    let session = source_bgp_receiving::session();
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        "SELECT (EXISTS { ?s <urn:ex:p> ?o } AS ?e) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        session,
        Vec::new(),
    )
    .unwrap();
    let Expression::SourceExists(expression) =
        source_exists_runtime_receiving::first_source_expression(prepared.plan())
    else {
        panic!("actual source expression missing");
    };
    assert!(expression.syntax.nodes().iter().any(|node| {
        node.name == "TriplesBlock"
            && node
                .fields
                .iter()
                .any(|(key, value)| key == "triples" && matches!(value, V::Triples(_)))
    }));
    let mut rows = prepared.execute_source_presence().unwrap();
    let error = rows
        .next()
        .unwrap()
        .err()
        .expect("unreceived old syntax must fail");
    assert!(matches!(
        crate::numeric::raw_numeric_fault(&error),
        Some(RawNumericFault::UnsupportedSourceLiteralOperator(
            "source CompValue syntax representation"
        ))
    ));
    assert!(rows.next().is_none());
}

#[test]
fn source_triples_true_path_operators_remain_gated_before_expansion() {
    for path in [
        "<urn:ex:p>/<urn:ex:q>",
        "<urn:ex:p>|<urn:ex:q>",
        "^<urn:ex:p>",
        "<urn:ex:p>?",
        "<urn:ex:p>*",
        "<urn:ex:p>+",
        "!<urn:ex:p>",
    ] {
        let session = source_bgp_receiving::session();
        let result = numeric_query::prepare_source_graph_numeric_query(
            &format!("SELECT * WHERE {{ ?s {path} ?o }}"),
            source_bgp_receiving::dataset(&session, None),
            session,
            Vec::new(),
        );
        assert!(
            matches!(
                result,
                Err(numeric_query::NumericPrepareError::LiteralConstruction(
                    RawNumericFault::UnsupportedSourceLiteralOperator("source graph path operator")
                ))
            ),
            "source operator passed ordinary lowering: {path}"
        );
    }
}

#[test]
fn source_triples_actual_graph_cancel_precedes_error_and_fuses() {
    let session = source_bgp_receiving::session();
    let flag = Arc::new(AtomicBool::new(false));
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        "SELECT (EXISTS { ?s <urn:ex:p> ?o; <urn:ex:q> ?z } AS ?e) WHERE {}",
        source_bgp_receiving::dataset(&session, Some(flag.clone())),
        session,
        Vec::new(),
    )
    .unwrap();
    let mut rows = prepared.execute_source_presence().unwrap();
    flag.store(true, atomic::Ordering::Relaxed);
    assert!(matches!(
        rows.next().unwrap(),
        Err(QueryEvaluationError::Cancelled)
    ));
    flag.store(false, atomic::Ordering::Relaxed);
    for _ in 0..3 {
        assert!(rows.next().is_none());
    }
}

#[test]
fn source_triples_renderer_cancels_while_visiting_actual_path_mapping() {
    let session = source_bgp_receiving::session();
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        "SELECT (EXISTS { ?s (<urn:ex:p>) ?o } AS ?e) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let Expression::SourceExists(expression) =
        source_exists_runtime_receiving::first_source_expression(prepared.plan())
    else {
        panic!("missing actual expression");
    };
    let registry = prepared
        .evaluator()
        .dataset
        .numeric
        .as_ref()
        .unwrap()
        .prepared_literals
        .as_ref()
        .unwrap();
    let mut visits = 0;
    let error = source_compvalue::render_mapping(
        expression.syntax.nodes(),
        source_compvalue::untranslated_graph_node(&expression.syntax).unwrap(),
        true,
        &session,
        registry,
        &mut || {
            visits += 1;
            if visits == 12 {
                Err(QueryEvaluationError::Cancelled)
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert!(matches!(error, QueryEvaluationError::Cancelled));
    assert_eq!(visits, 12);
    assert_eq!(
        session.flags(),
        semantica_numeric::DecimalSignals::default()
    );
}

#[test]
fn source_triples_equal_rdf_terms_keep_distinct_actual_literal_occurrences() {
    let session = source_bgp_receiving::session();
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        "PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> SELECT (EXISTS { ?s <urn:ex:p> \"1\"^^xsd:integer, 1; <urn:ex:q> \"a\"@ZH-cn } AS ?e) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        Rc::clone(&session),
        Vec::new(),
    ).unwrap();
    let Expression::SourceExists(expression) =
        source_exists_runtime_receiving::first_source_expression(prepared.plan())
    else {
        panic!("missing actual expression");
    };
    let block = expression
        .syntax
        .nodes()
        .iter()
        .find(|node| node.name == "TriplesBlock")
        .unwrap();
    let (_, V::List(rows)) = block
        .fields
        .iter()
        .find(|(key, _)| key == "triples")
        .unwrap()
    else {
        panic!("no original flat rows");
    };
    assert_eq!(rows.len(), 1);
    let V::List(row) = &rows[0] else {
        panic!("not a row");
    };
    assert_eq!(row.len(), 9);
    assert_eq!(
        row[1], row[4],
        "comma shares the actual predicate reduction"
    );
    let (V::Literal(first), V::Literal(second), V::Literal(language)) = (&row[2], &row[5], &row[8])
    else {
        panic!("lost actual literal reductions");
    };
    assert_ne!(first.occurrence, second.occurrence);
    assert_eq!(first.rdf, second.rdf);
    assert_eq!(language.language_original.as_deref(), Some("ZH-cn"));
    let registry = prepared
        .evaluator()
        .dataset
        .numeric
        .as_ref()
        .unwrap()
        .prepared_literals
        .as_ref()
        .unwrap();
    let first = registry.resolve(first).unwrap();
    let second = registry.resolve(second).unwrap();
    // Equal RDF spelling and cache value still do not merge real constructor
    // occurrences. TR11 separately retains the original unequal 01/1 spelling.
    assert_eq!(first.rdf().unwrap(), second.rdf().unwrap());
    assert!(!first.same_source_object(&second));
}

#[test]
fn source_triples_query_gt10() {
    receive("GT10");
}

#[test]
fn source_triples_query_tr01() {
    receive("TR01");
}

#[test]
fn source_triples_query_tr02() {
    receive("TR02");
}

#[test]
fn source_triples_query_tr03() {
    receive("TR03");
}

#[test]
fn source_triples_query_tr04() {
    receive("TR04");
}

#[test]
fn source_triples_query_tr05() {
    receive("TR05");
}

#[test]
fn source_triples_query_tr06() {
    receive("TR06");
}

#[test]
fn source_triples_query_tr07() {
    receive("TR07");
}

#[test]
fn source_triples_query_tr08() {
    receive("TR08");
}

#[test]
fn source_triples_query_tr09() {
    receive("TR09");
}

#[test]
fn source_triples_query_tr10() {
    receive("TR10");
}

#[test]
fn source_triples_query_tr11() {
    receive("TR11");
}

#[test]
fn source_triples_query_tr12() {
    receive("TR12");
}

#[test]
fn source_triples_query_tr13() {
    receive("TR13");
}

#[test]
fn source_triples_query_tr14() {
    receive("TR14");
}

#[test]
fn source_triples_query_tr15() {
    receive("TR15");
}

#[test]
fn source_triples_query_tr16() {
    receive("TR16");
}

#[test]
fn source_triples_query_tr17() {
    receive("TR17");
}

#[test]
fn source_triples_query_tr18() {
    receive("TR18");
}

#[test]
fn source_triples_query_tr19() {
    receive("TR19");
}

#[test]
fn source_triples_query_tr20() {
    receive("TR20");
}
