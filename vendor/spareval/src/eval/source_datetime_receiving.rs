//! Frozen original observations; development replay, not independent acceptance.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_query_cases.json"
    )
}
fn receive(id: &str) {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_fixture().as_bytes())),
        "82791ae7821ba85f4be7a882fa3dc5b472cc8b6475a6b9c4eaf37c38d80e9117"
    );
    let root: Value = serde_json::from_str(private_fixture()).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    let cases = root["graph_queries"].as_array().unwrap();
    assert_eq!(cases.len(), 83);
    let case = cases.iter().find(|v| v["input"]["id"] == id).unwrap();
    let guard = crate::numeric::source_function::observation::Guard::start();
    source_bgp_receiving::receive_query_case(case, &root["data_inputs"]);
    source_function_runtime_receiving::receive_function_trace(case, guard.take());
}

#[test]
fn source_datetime_dt01() {
    receive("DT01");
}

#[test]
fn source_datetime_dt02() {
    receive("DT02");
}

#[test]
fn source_datetime_dt03() {
    receive("DT03");
}

#[test]
fn source_datetime_dt04() {
    receive("DT04");
}

#[test]
fn source_datetime_dt05() {
    receive("DT05");
}

#[test]
fn source_datetime_dt06() {
    receive("DT06");
}

#[test]
fn source_datetime_dt07() {
    receive("DT07");
}

#[test]
fn source_datetime_dt08() {
    receive("DT08");
}

#[test]
fn source_datetime_dt09() {
    receive("DT09");
}

#[test]
fn source_datetime_dt10() {
    receive("DT10");
}

#[test]
fn source_datetime_dt11() {
    receive("DT11");
}

#[test]
fn source_datetime_dt12() {
    receive("DT12");
}

#[test]
fn source_datetime_dt13() {
    receive("DT13");
}

#[test]
fn source_datetime_dt14() {
    receive("DT14");
}

#[test]
fn source_datetime_dt15() {
    receive("DT15");
}

#[test]
fn source_datetime_dt16() {
    receive("DT16");
}

#[test]
fn source_datetime_dt17() {
    receive("DT17");
}

#[test]
fn source_datetime_dt18() {
    receive("DT18");
}

#[test]
fn source_datetime_dt19() {
    receive("DT19");
}

#[test]
fn source_datetime_dt20() {
    receive("DT20");
}

#[test]
fn source_datetime_dt21() {
    receive("DT21");
}

#[test]
fn source_datetime_dt22() {
    receive("DT22");
}

#[test]
fn source_datetime_dt23() {
    receive("DT23");
}

#[test]
fn source_datetime_dt24() {
    receive("DT24");
}

#[test]
fn source_datetime_dt25() {
    receive("DT25");
}

#[test]
fn source_datetime_dt26() {
    receive("DT26");
}

#[test]
fn source_datetime_dt27() {
    receive("DT27");
}

#[test]
fn source_datetime_dt28() {
    receive("DT28");
}

#[test]
fn source_datetime_dt29() {
    receive("DT29");
}

#[test]
fn source_datetime_dt30() {
    receive("DT30");
}

#[test]
fn source_datetime_dt31() {
    receive("DT31");
}

#[test]
fn source_datetime_dt32() {
    receive("DT32");
}

#[test]
fn source_datetime_dt33() {
    receive("DT33");
}

#[test]
fn source_datetime_dt34() {
    receive("DT34");
}

#[test]
fn source_datetime_dt35() {
    receive("DT35");
}

#[test]
fn source_datetime_dt36() {
    receive("DT36");
}

#[test]
fn source_datetime_dt37() {
    receive("DT37");
}

#[test]
fn source_datetime_dt38() {
    receive("DT38");
}

#[test]
fn source_datetime_dt39() {
    receive("DT39");
}

#[test]
fn source_datetime_dt40() {
    receive("DT40");
}

#[test]
fn source_datetime_dt41() {
    receive("DT41");
}

#[test]
fn source_datetime_dt42() {
    receive("DT42");
}

#[test]
fn source_datetime_dt43() {
    receive("DT43");
}

#[test]
fn source_datetime_dt44() {
    receive("DT44");
}

#[test]
fn source_datetime_dt45() {
    receive("DT45");
}

#[test]
fn source_datetime_dt46() {
    receive("DT46");
}

#[test]
fn source_datetime_dt47() {
    receive("DT47");
}

#[test]
fn source_datetime_dt48() {
    receive("DT48");
}

#[test]
fn source_datetime_dt49() {
    receive("DT49");
}

#[test]
fn source_datetime_dt50() {
    receive("DT50");
}

#[test]
fn source_datetime_dt51() {
    receive("DT51");
}

#[test]
fn source_datetime_dtn01() {
    receive("DTN01");
}

#[test]
fn source_datetime_dtn02() {
    receive("DTN02");
}

#[test]
fn source_datetime_dtn03() {
    receive("DTN03");
}

#[test]
fn source_datetime_dtn04() {
    receive("DTN04");
}

#[test]
fn source_datetime_dtc01() {
    receive("DTC01");
}

#[test]
fn source_datetime_dtc02() {
    receive("DTC02");
}

#[test]
fn source_datetime_dtc03() {
    receive("DTC03");
}

#[test]
fn source_datetime_dtc04() {
    receive("DTC04");
}

#[test]
fn source_datetime_dtc05() {
    receive("DTC05");
}

#[test]
fn source_datetime_dtc06() {
    receive("DTC06");
}

#[test]
fn source_datetime_dtc07() {
    receive("DTC07");
}

#[test]
fn source_datetime_dtc08() {
    receive("DTC08");
}

#[test]
fn source_datetime_dtc09() {
    receive("DTC09");
}

#[test]
fn source_datetime_dtc10() {
    receive("DTC10");
}

#[test]
fn source_datetime_dtc11() {
    receive("DTC11");
}

#[test]
fn source_datetime_dtc12() {
    receive("DTC12");
}

#[test]
fn source_datetime_dtc13() {
    receive("DTC13");
}

#[test]
fn source_datetime_dtc14() {
    receive("DTC14");
}

#[test]
fn source_datetime_dtc15() {
    receive("DTC15");
}

#[test]
fn source_datetime_dtc16() {
    receive("DTC16");
}

#[test]
fn source_datetime_dtc17() {
    receive("DTC17");
}

#[test]
fn source_datetime_dtc18() {
    receive("DTC18");
}

#[test]
fn source_datetime_dtc19() {
    receive("DTC19");
}

#[test]
fn source_datetime_dtc20() {
    receive("DTC20");
}

#[test]
fn source_datetime_dtebv01() {
    receive("DTEBV01");
}

#[test]
fn source_datetime_dtebv02() {
    receive("DTEBV02");
}

#[test]
fn source_datetime_dti01() {
    receive("DTI01");
}

#[test]
fn source_datetime_dti02() {
    receive("DTI02");
}

#[test]
fn source_datetime_dti03() {
    receive("DTI03");
}

#[test]
fn source_datetime_dti04() {
    receive("DTI04");
}

#[test]
fn source_datetime_dtorder01() {
    receive("DTORDER01");
}

#[test]
fn source_datetime_dtgroup01() {
    receive("DTGROUP01");
}

#[test]
fn source_datetime_control_earlier_factories_keep_gate() {
    use crate::numeric::NumericInitialValue;
    use oxrdf::vocab::xsd;
    let session = source_bgp_receiving::session();
    let query =
        "SELECT (\"2020-01-02\"^^<http://www.w3.org/2001/XMLSchema#dateTime> AS ?v) WHERE {}";
    assert!(
        numeric_query::prepare_source_exists_numeric_query(
            query,
            source_bgp_receiving::dataset(&session, None),
            session.clone(),
            Vec::new()
        )
        .is_err()
    );
    assert!(
        session
            .construct_initial(NumericInitialValue::SourceDirectString {
                lexical: "2020-01-02".into(),
                datatype: Some(xsd::DATE_TIME.into_owned()),
                language: None,
                normalize: true,
            })
            .is_err()
    );
    assert!(
        session
            .construct_source_direct_string(
                "2020-01-02",
                Some(xsd::DATE_TIME.into_owned()),
                None,
                true
            )
            .is_err()
    );
}

fn temporal_value() -> RawExpressionValue {
    use crate::numeric::NumericInitialValue;
    source_bgp_receiving::session()
        .construct_source_graph_initial(NumericInitialValue::SourceDirectString {
            lexical: "2020-01-02T03:04:05Z".into(),
            datatype: Some(oxrdf::vocab::xsd::DATE_TIME.into_owned()),
            language: None,
            normalize: false,
        })
        .unwrap()
}

#[test]
fn source_datetime_control_cache_metadata_cannot_mismatch() {
    use crate::numeric::{LiteralSourceState, SourceFact, SourceLiteralCacheKind};
    let (rdf, cache, meta) = temporal_value().into_source_parts().unwrap();
    let LiteralSourceState::Literal(mut m) = meta.clone() else {
        panic!("metadata");
    };
    m.cached_value_kind = SourceFact::Known(SourceLiteralCacheKind::DateTime(None));
    assert!(matches!(
        RawExpressionValue::from_source_parts(
            rdf.clone(),
            cache.clone(),
            LiteralSourceState::Literal(m)
        ),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    let wrong_rdf =
        Literal::new_typed_literal("2020-01-03T03:04:05Z", oxrdf::vocab::xsd::DATE_TIME).into();
    assert!(matches!(
        RawExpressionValue::from_source_parts(wrong_rdf, cache.clone(), meta.clone()),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    let LiteralSourceState::Literal(mut m) = meta else {
        panic!("metadata");
    };
    m.ill_typed = SourceFact::Known(Some(true));
    assert!(matches!(
        RawExpressionValue::from_source_parts(rdf, cache, LiteralSourceState::Literal(m)),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
}

#[test]
fn source_datetime_control_graph_transport_retains_value_and_identity() {
    let session = source_bgp_receiving::session();
    let raw = temporal_value();
    let mut graph = source_bgp_receiving::dataset(&session, None);
    let stored = graph.admit_source(raw.clone()).unwrap();
    let value = graph.externalize_source(stored).unwrap();
    assert!(value.same_source_object(&raw));
    assert_eq!(
        source_bgp_receiving::term_json(&value, &session),
        source_bgp_receiving::term_json(&raw, &session)
    );
}

#[test]
fn source_datetime_control_unknown_import_stays_unknown() {
    use crate::numeric::source_function::SourceFunctionKind;
    let session = source_bgp_receiving::session();
    let raw = RawExpressionValue::from_import(
        Literal::new_typed_literal("2020-01-02", oxrdf::vocab::xsd::DATE_TIME).into(),
    );
    let value = SolutionValue::Rdf(raw);
    assert!(session.source_effective_boolean_value(&value).is_err());
    assert!(matches!(
        session.source_default_cast(SourceFunctionKind::DateTime, &value),
        Err(crate::numeric::NumericEvaluationError::Fatal(_))
    ));
}

#[test]
fn source_datetime_control_output_limit_stays_fatal() {
    // Successful object construction must not reinterpret its longer output
    // as a fresh input or eagerly format the error-only source repr.
    let input = source_bgp_receiving::session()
        .construct_source_direct_string("20200102", None, None, false)
        .unwrap();
    let limited = source_exists_runtime_receiving::session(
        &serde_json::json!({"input":{"decimal":{"precision":28,"trap_float_operation":false}}}),
        semantica_numeric::NumericLimits {
            max_input_characters: 8,
            max_output_characters: 19,
            ..Default::default()
        },
    );
    let result = limited
        .source_default_cast(
            crate::numeric::source_function::SourceFunctionKind::DateTime,
            &SolutionValue::Rdf(input),
        )
        .unwrap();
    let Term::Literal(result) = result.rdf().unwrap() else {
        panic!("datetime literal");
    };
    assert_eq!(result.value(), "2020-01-02T00:00:00");
    use crate::numeric::NumericInitialValue;
    let session = source_exists_runtime_receiving::session(
        &serde_json::json!({"input":{"decimal":{"precision":28,"trap_float_operation":false}}}),
        semantica_numeric::NumericLimits {
            max_output_characters: 10,
            ..Default::default()
        },
    );
    let error = session
        .construct_source_graph_initial(NumericInitialValue::SourceDirectString {
            lexical: "2020-01-02".into(),
            datatype: Some(oxrdf::vocab::xsd::DATE_TIME.into_owned()),
            language: None,
            normalize: true,
        })
        .unwrap_err();
    assert!(matches!(
        error,
        RawNumericFault::Core(semantica_numeric::NumericFault::ResourceLimit {
            operation: "output_characters"
        })
    ));
}

#[test]
fn source_datetime_control_comparison_errors_keep_source_arguments() {
    use crate::numeric::{NumericEvaluationError, NumericInitialValue};
    use crate::tuple::{SourceExceptionArgument, SourceExceptionClass};
    let session = source_bgp_receiving::session();
    let value = |s: &str| {
        SolutionValue::Rdf(
            session
                .construct_source_graph_initial(NumericInitialValue::SourceDirectString {
                    lexical: s.into(),
                    datatype: Some(oxrdf::vocab::xsd::DATE_TIME.into_owned()),
                    language: None,
                    normalize: false,
                })
                .unwrap(),
        )
    };
    let NumericEvaluationError::Sparql {
        source: Some(error),
        ..
    } = session
        .source_relational_equal(&value("bad"), &value("other"))
        .unwrap_err()
    else {
        panic!("original SPARQLError");
    };
    assert_eq!(error.class, SourceExceptionClass::Sparql);
    assert_eq!(error.argument, SourceExceptionArgument::Text("I cannot know that these two lexical forms do not map to the same value: bad and other".into()));
}

fn receive_group_base(id: &str) {
    fn private_group_fixture() -> &'static str {
        crate::private_test_fixtures::text_ref!(
            "release-inputs-03/vendor/spareval/src/eval/fixtures/source_group_base_query_cases.json"
        )
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(private_group_fixture().as_bytes())),
        "baa9b4d1f3ab2cd0fc2734fc1935fc4bba29c3a02255656c8aae38fc7b34774c"
    );
    let root: Value = serde_json::from_str(private_group_fixture()).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    let cases = root["graph_queries"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let case = cases.iter().find(|v| v["input"]["id"] == id).unwrap();
    let guard = crate::numeric::source_function::observation::Guard::start();
    source_bgp_receiving::receive_query_case(case, &root["data_inputs"]);
    source_function_runtime_receiving::receive_function_trace(case, guard.take());
}

#[test]
fn source_datetime_group_base_sg01() {
    receive_group_base("SG01");
}

#[test]
fn source_datetime_group_base_sg02() {
    receive_group_base("SG02");
}

#[test]
fn source_datetime_group_base_sg03() {
    receive_group_base("SG03");
}

#[test]
fn source_datetime_group_base_sg04() {
    receive_group_base("SG04");
}

#[test]
fn source_datetime_group_base_sg05() {
    receive_group_base("SG05");
}

#[test]
fn source_datetime_group_base_sg06() {
    receive_group_base("SG06");
}

#[test]
fn source_datetime_group_base_sg07() {
    receive_group_base("SG07");
}

#[test]
fn source_datetime_group_base_sg08() {
    receive_group_base("SG08");
}
