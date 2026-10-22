//! Frozen original observations through the actual private source query factory.
//! Root-authored differential receiving; independent acceptance remains separate.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_native_containers_query_cases.json"
    )
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_fixture().as_bytes())),
        "e3cf6d83abdcd67c022793a55507555561fff0541915b4e88754b823fcafea77"
    );
    let value: Value = serde_json::from_str(private_fixture()).unwrap();
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(value["graph_queries"].as_array().unwrap().len(), 36);
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
fn source_native_container_vc01() {
    receive("VC01");
}

#[test]
fn source_native_container_vc02() {
    receive("VC02");
}

#[test]
fn source_native_container_vc03() {
    receive("VC03");
}

#[test]
fn source_native_container_vc04() {
    receive("VC04");
}

#[test]
fn source_native_container_vc05() {
    receive("VC05");
}

#[test]
fn source_native_container_vc06() {
    receive("VC06");
}

#[test]
fn source_native_container_vc07() {
    receive("VC07");
}

#[test]
fn source_native_container_vc08() {
    receive("VC08");
}

#[test]
fn source_native_container_vc09() {
    receive("VC09");
}

#[test]
fn source_native_container_vc10() {
    receive("VC10");
}

#[test]
fn source_native_container_vc11() {
    receive("VC11");
}

#[test]
fn source_native_container_vc12() {
    receive("VC12");
}

#[test]
fn source_native_container_vc13() {
    receive("VC13");
}

#[test]
fn source_native_container_vc14() {
    receive("VC14");
}

#[test]
fn source_native_container_vc15() {
    receive("VC15");
}

#[test]
fn source_native_container_vc16() {
    receive("VC16");
}

#[test]
fn source_native_container_vc17() {
    receive("VC17");
}

#[test]
fn source_native_container_vc18() {
    receive("VC18");
}

#[test]
fn source_native_container_vc19() {
    receive("VC19");
}

#[test]
fn source_native_container_fc01() {
    receive("FC01");
}

#[test]
fn source_native_container_fc02() {
    receive("FC02");
}

#[test]
fn source_native_container_fc03() {
    receive("FC03");
}

#[test]
fn source_native_container_fc04() {
    receive("FC04");
}

#[test]
fn source_native_container_fc05() {
    receive("FC05");
}

#[test]
fn source_native_container_fc06() {
    receive("FC06");
}

#[test]
fn source_native_container_fc07() {
    receive("FC07");
}

#[test]
fn source_native_container_fc08() {
    receive("FC08");
}

#[test]
fn source_native_container_fc09() {
    receive("FC09");
}

#[test]
fn source_native_container_lc01() {
    receive("LC01");
}

#[test]
fn source_native_container_lc02() {
    receive("LC02");
}

#[test]
fn source_native_container_lc03() {
    receive("LC03");
}

#[test]
#[ignore = "Required LC04 collection source representation remains unimplemented; run explicitly in pending-original-queries"]
fn source_native_container_lc04() {
    receive("LC04");
}

#[test]
fn source_native_container_lc05() {
    receive("LC05");
}

#[test]
fn source_native_container_lc06() {
    receive("LC06");
}

#[test]
fn source_native_container_lc07() {
    receive("LC07");
}

#[test]
fn source_native_container_lc08() {
    receive("LC08");
}

#[test]
fn source_native_preserves_default_and_earlier_exists_admission() {
    let root = fixture();
    for id in [
        "VC04", "VC05", "VC06", "VC15", "VC17", "VC18", "FC03", "FC09",
    ] {
        let case = root["graph_queries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["input"]["id"] == id)
            .unwrap();
        let query = case["input"]["query"].as_str().unwrap();
        assert!(
            spargebra::SparqlParser::new().parse_query(query).is_err(),
            "{id} default admission changed"
        );
        let session = source_bgp_receiving::session();
        assert!(
            numeric_query::prepare_source_exists_numeric_query(
                query,
                source_bgp_receiving::dataset(&session, None),
                session,
                Vec::new()
            )
            .is_err(),
            "{id} earlier source admission changed"
        );
    }
}

fn values_prepared() -> (
    Rc<QueryNumericSession>,
    numeric_query::PreparedNumericQuery<'static, crate::dataset::source_memory::SourceMemoryGraph>,
    Value,
) {
    let root = fixture();
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == "VC09")
        .unwrap()
        .clone();
    let session = source_bgp_receiving::session();
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        source_bgp_receiving::dataset(&session, None),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    (session, prepared, case)
}

#[test]
fn source_native_dictionary_mapping_matches_original_instance() {
    let (session, prepared, case) = values_prepared();
    let Expression::SourceExists(expression) =
        source_exists_runtime_receiving::first_source_expression(prepared.plan())
    else {
        panic!("missing source expression")
    };
    let nodes = expression.syntax.nodes();
    let at = nodes.iter().position(|node| node.name == "values").unwrap();
    let registry = prepared.source_literal_registry().unwrap();
    let actual =
        source_compvalue::render_mapping(nodes, at, false, &session, registry, &mut || Ok(()))
            .unwrap();
    let original = case["exists_instance_nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "values")
        .unwrap();
    assert_eq!(actual, original["repr"].as_str().unwrap());
    assert!(
        nodes[at].fields.iter().any(|(key, v)| key == "res"
            && matches!(v, spargebra::SourceSyntaxValue::ValueRecords { .. }))
    );
    assert!(
        !nodes[at].fields.iter().any(|(key, _)| key == "_vars"),
        "instance-only source mapping must not gain analyzed fields"
    );
}

#[test]
fn source_native_dictionary_cancel_stops_actual_record_visit() {
    let (session, prepared, _) = values_prepared();
    let Expression::SourceExists(expression) =
        source_exists_runtime_receiving::first_source_expression(prepared.plan())
    else {
        panic!("missing source expression")
    };
    let nodes = expression.syntax.nodes();
    let at = nodes.iter().position(|node| node.name == "values").unwrap();
    let mut visits = 0;
    let error = source_compvalue::render_mapping(
        nodes,
        at,
        false,
        &session,
        prepared.source_literal_registry().unwrap(),
        &mut || {
            visits += 1;
            if visits == 3 {
                Err(QueryEvaluationError::Cancelled)
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err();
    assert!(matches!(error, QueryEvaluationError::Cancelled));
    assert_eq!(visits, 3);
    assert_eq!(
        session.flags(),
        semantica_numeric::DecimalSignals::default()
    );
}

#[test]
fn source_native_dictionary_owner_mismatch_is_rejected() {
    let (_, prepared, _) = values_prepared();
    let Expression::SourceExists(expression) =
        source_exists_runtime_receiving::first_source_expression(prepared.plan())
    else {
        panic!("missing source expression")
    };
    let nodes = expression.syntax.nodes();
    let at = nodes.iter().position(|node| node.name == "values").unwrap();
    let other = source_bgp_receiving::session();
    let error = source_compvalue::render_mapping(
        nodes,
        at,
        false,
        &other,
        prepared.source_literal_registry().unwrap(),
        &mut || Ok(()),
    )
    .unwrap_err();
    assert!(matches!(
        crate::numeric::raw_numeric_fault(&error),
        Some(RawNumericFault::InvalidSourceOrigin)
    ));
}
