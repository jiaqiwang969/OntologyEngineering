//! Frozen original query inputs and outcomes; development replay, not independent acceptance.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_function_runtime_query_cases.json"
    )
}
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_fixture().as_bytes())),
        "476c6ddb222c75b8f86820d2da81b36b6497bfad7ed3277e9df856006d31e59a"
    );
    let value: Value = serde_json::from_str(private_fixture()).unwrap();
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(value["graph_queries"].as_array().unwrap().len(), 87);
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
    let guard = crate::numeric::source_function::observation::Guard::start();
    source_bgp_receiving::receive_query_case(case, &root["data_inputs"]);
    receive_function_trace(case, guard.take());
}

pub(super) fn receive_function_trace(
    case: &Value,
    actual: Vec<crate::numeric::source_function::observation::Event>,
) {
    use crate::numeric::source_function::observation::Event;
    use crate::tuple::{SourceExceptionArgument, SourceExceptionClass};
    use serde_json::json;
    let id = case["input"]["id"].as_str().unwrap();
    let mut expected = Vec::new();
    let mut stack = Vec::<(String, bool)>::new();
    let mut returned_objects = Vec::new();
    for event in case["trace"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["name"] == "Function")
    {
        match event["event"].as_str().unwrap() {
            "call" => {
                let iri = event["iri"]["value"].as_str().unwrap().to_owned();
                expected.push(json!({"event":"call", "iri":iri}));
                stack.push((iri, false));
            }
            "exception" => {
                let (iri, failed) = stack.last_mut().unwrap();
                assert!(!*failed, "unreceived repeated source exception");
                *failed = true;
                expected.push(json!({"event":"error", "iri":iri, "value":event["exception"]}));
            }
            "return" => {
                let (iri, failed) = stack.pop().unwrap();
                if failed {
                    assert!(event["result"].is_null());
                } else {
                    let mut value = event["result"].clone();
                    returned_objects.push((
                        expected.len(),
                        value["object_token"].as_str().unwrap().to_owned(),
                    ));
                    value.as_object_mut().unwrap().remove("object_token");
                    expected.push(json!({"event":"return", "iri":iri, "value":value}));
                }
            }
            "line" => (),
            other => panic!("unreceived Function trace event: {other}"),
        }
    }
    assert!(stack.is_empty());
    let session = source_bgp_receiving::session();
    let encoded = actual.iter().map(|event| match event {
        Event::Call(iri) => json!({"event":"call", "iri":iri.as_str()}),
        Event::Return(iri, SolutionValue::Rdf(value)) => json!({"event":"return", "iri":iri.as_str(),
            "value":source_bgp_receiving::term_json(value, &session)}),
        Event::Return(iri, SolutionValue::ReturnedError(error)) => {
            let error = error.source_exception().expect("original exception constructor");
            let class = match error.class {
                SourceExceptionClass::Sparql => "SPARQLError",
                SourceExceptionClass::SparqlType => "SPARQLTypeError",
                SourceExceptionClass::NotBound => "NotBoundError",
                SourceExceptionClass::AlreadyBound => "AlreadyBound",
            };
            let argument = match &error.argument {
                SourceExceptionArgument::None => Value::Null,
                SourceExceptionArgument::Text(text) => json!(text),
                SourceExceptionArgument::DecimalSignalClasses(signals) => source_function_registry_receiving::decimal_classes_json(signals),
            };
            json!({"event":"error", "iri":iri.as_str(), "value":{
                "kind":"Exception", "module":"rdflib.plugins.sparql.sparql", "class_name":class, "args":[argument]}})
        }
        Event::Return(_, SolutionValue::ExplicitNone) => panic!("stock Function returned None"),
    }).collect::<Vec<_>>();
    assert_eq!(
        encoded, expected,
        "{id} actual Function call/result/error order"
    );
    for (a, wa) in &returned_objects {
        for (b, wb) in &returned_objects {
            let Event::Return(_, SolutionValue::Rdf(a)) = &actual[*a] else {
                panic!("result event");
            };
            let Event::Return(_, SolutionValue::Rdf(b)) = &actual[*b] else {
                panic!("result event");
            };
            assert_eq!(
                a.same_source_object(b),
                wa == wb,
                "{id} Function return aliases"
            );
        }
    }
}

#[test]
fn source_function_runtime_fu01() {
    receive("FU01");
}

#[test]
fn source_function_runtime_fu02() {
    receive("FU02");
}

#[test]
fn source_function_runtime_fu03() {
    receive("FU03");
}

#[test]
fn source_function_runtime_fu04() {
    receive("FU04");
}

#[test]
fn source_function_runtime_fu05() {
    receive("FU05");
}

#[test]
fn source_function_runtime_fu06() {
    receive("FU06");
}

#[test]
fn source_function_runtime_fstring01() {
    receive("FSTRING01");
}

#[test]
fn source_function_runtime_fstring02() {
    receive("FSTRING02");
}

#[test]
fn source_function_runtime_fstring03() {
    receive("FSTRING03");
}

#[test]
fn source_function_runtime_fstring04() {
    receive("FSTRING04");
}

#[test]
fn source_function_runtime_fstring05() {
    receive("FSTRING05");
}

#[test]
fn source_function_runtime_fstring06() {
    receive("FSTRING06");
}

#[test]
fn source_function_runtime_fstring07() {
    receive("FSTRING07");
}

#[test]
fn source_function_runtime_fstring08() {
    receive("FSTRING08");
}

#[test]
fn source_function_runtime_fstringzero() {
    receive("FSTRINGZERO");
}

#[test]
fn source_function_runtime_fstringmany() {
    receive("FSTRINGMANY");
}

#[test]
fn source_function_runtime_fstringunbound_many() {
    receive("FSTRINGUNBOUND_MANY");
}

#[test]
fn source_function_runtime_finteger01() {
    receive("FINTEGER01");
}

#[test]
fn source_function_runtime_finteger02() {
    receive("FINTEGER02");
}

#[test]
fn source_function_runtime_finteger03() {
    receive("FINTEGER03");
}

#[test]
fn source_function_runtime_finteger04() {
    receive("FINTEGER04");
}

#[test]
fn source_function_runtime_finteger05() {
    receive("FINTEGER05");
}

#[test]
fn source_function_runtime_finteger06() {
    receive("FINTEGER06");
}

#[test]
fn source_function_runtime_finteger07() {
    receive("FINTEGER07");
}

#[test]
fn source_function_runtime_finteger08() {
    receive("FINTEGER08");
}

#[test]
fn source_function_runtime_finteger09() {
    receive("FINTEGER09");
}

#[test]
fn source_function_runtime_finteger10() {
    receive("FINTEGER10");
}

#[test]
fn source_function_runtime_finteger11() {
    receive("FINTEGER11");
}

#[test]
fn source_function_runtime_fintegerzero() {
    receive("FINTEGERZERO");
}

#[test]
fn source_function_runtime_fintegermany() {
    receive("FINTEGERMANY");
}

#[test]
fn source_function_runtime_fintegerunbound_many() {
    receive("FINTEGERUNBOUND_MANY");
}

#[test]
fn source_function_runtime_fdecimal01() {
    receive("FDECIMAL01");
}

#[test]
fn source_function_runtime_fdecimal02() {
    receive("FDECIMAL02");
}

#[test]
fn source_function_runtime_fdecimal03() {
    receive("FDECIMAL03");
}

#[test]
fn source_function_runtime_fdecimal04() {
    receive("FDECIMAL04");
}

#[test]
fn source_function_runtime_fdecimal05() {
    receive("FDECIMAL05");
}

#[test]
fn source_function_runtime_fdecimal06() {
    receive("FDECIMAL06");
}

#[test]
fn source_function_runtime_fdecimal07() {
    receive("FDECIMAL07");
}

#[test]
fn source_function_runtime_fdecimal08() {
    receive("FDECIMAL08");
}

#[test]
fn source_function_runtime_fdecimal09() {
    receive("FDECIMAL09");
}

#[test]
fn source_function_runtime_fdecimal10() {
    receive("FDECIMAL10");
}

#[test]
fn source_function_runtime_fdecimal11() {
    receive("FDECIMAL11");
}

#[test]
fn source_function_runtime_fdecimalzero() {
    receive("FDECIMALZERO");
}

#[test]
fn source_function_runtime_fdecimalmany() {
    receive("FDECIMALMANY");
}

#[test]
fn source_function_runtime_fdecimalunbound_many() {
    receive("FDECIMALUNBOUND_MANY");
}

#[test]
fn source_function_runtime_ffloat01() {
    receive("FFLOAT01");
}

#[test]
fn source_function_runtime_ffloat02() {
    receive("FFLOAT02");
}

#[test]
fn source_function_runtime_ffloat03() {
    receive("FFLOAT03");
}

#[test]
fn source_function_runtime_ffloat04() {
    receive("FFLOAT04");
}

#[test]
fn source_function_runtime_ffloat05() {
    receive("FFLOAT05");
}

#[test]
fn source_function_runtime_ffloat06() {
    receive("FFLOAT06");
}

#[test]
fn source_function_runtime_ffloat07() {
    receive("FFLOAT07");
}

#[test]
fn source_function_runtime_ffloatzero() {
    receive("FFLOATZERO");
}

#[test]
fn source_function_runtime_ffloatmany() {
    receive("FFLOATMANY");
}

#[test]
fn source_function_runtime_ffloatunbound_many() {
    receive("FFLOATUNBOUND_MANY");
}

#[test]
fn source_function_runtime_fdouble01() {
    receive("FDOUBLE01");
}

#[test]
fn source_function_runtime_fdouble02() {
    receive("FDOUBLE02");
}

#[test]
fn source_function_runtime_fdouble03() {
    receive("FDOUBLE03");
}

#[test]
fn source_function_runtime_fdouble04() {
    receive("FDOUBLE04");
}

#[test]
fn source_function_runtime_fdouble05() {
    receive("FDOUBLE05");
}

#[test]
fn source_function_runtime_fdouble06() {
    receive("FDOUBLE06");
}

#[test]
fn source_function_runtime_fdouble07() {
    receive("FDOUBLE07");
}

#[test]
fn source_function_runtime_fdoublezero() {
    receive("FDOUBLEZERO");
}

#[test]
fn source_function_runtime_fdoublemany() {
    receive("FDOUBLEMANY");
}

#[test]
fn source_function_runtime_fdoubleunbound_many() {
    receive("FDOUBLEUNBOUND_MANY");
}

#[test]
fn source_function_runtime_fboolean01() {
    receive("FBOOLEAN01");
}

#[test]
fn source_function_runtime_fboolean02() {
    receive("FBOOLEAN02");
}

#[test]
fn source_function_runtime_fboolean03() {
    receive("FBOOLEAN03");
}

#[test]
fn source_function_runtime_fboolean04() {
    receive("FBOOLEAN04");
}

#[test]
fn source_function_runtime_fboolean05() {
    receive("FBOOLEAN05");
}

#[test]
fn source_function_runtime_fboolean06() {
    receive("FBOOLEAN06");
}

#[test]
fn source_function_runtime_fboolean07() {
    receive("FBOOLEAN07");
}

#[test]
fn source_function_runtime_fboolean08() {
    receive("FBOOLEAN08");
}

#[test]
fn source_function_runtime_fboolean09() {
    receive("FBOOLEAN09");
}

#[test]
fn source_function_runtime_fbooleanzero() {
    receive("FBOOLEANZERO");
}

#[test]
fn source_function_runtime_fbooleanmany() {
    receive("FBOOLEANMANY");
}

#[test]
fn source_function_runtime_fbooleanunbound_many() {
    receive("FBOOLEANUNBOUND_MANY");
}

#[test]
fn source_function_runtime_fdatetime01() {
    receive("FDATETIME01");
}

#[test]
fn source_function_runtime_fdatetime02() {
    receive("FDATETIME02");
}

#[test]
fn source_function_runtime_fdatetime03() {
    receive("FDATETIME03");
}

#[test]
fn source_function_runtime_fdatetime04() {
    receive("FDATETIME04");
}

#[test]
fn source_function_runtime_fdatetime05() {
    receive("FDATETIME05");
}

#[test]
fn source_function_runtime_fdatetimezero() {
    receive("FDATETIMEZERO");
}

#[test]
fn source_function_runtime_fdatetimemany() {
    receive("FDATETIMEMANY");
}

#[test]
fn source_function_runtime_fdatetimeunbound_many() {
    receive("FDATETIMEUNBOUND_MANY");
}

#[test]
fn source_function_runtime_fstringdistinct() {
    receive("FSTRINGDISTINCT");
}

#[test]
fn source_function_runtime_fnested() {
    receive("FNESTED");
}

fn private_coalesce_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_coalesce_query_cases.json"
    )
}
fn receive_coalesce(id: &str) {
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(private_coalesce_fixture().as_bytes())
        ),
        "5e81b804d7475c03eeadbfb35f5e03165237cfcac9c69739dd1429872292e6d1"
    );
    let root: Value = serde_json::from_str(private_coalesce_fixture()).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 12);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    let guard = crate::numeric::source_function::observation::Guard::start();
    source_bgp_receiving::receive_query_case(case, &root["data_inputs"]);
    receive_function_trace(case, guard.take());
}

#[test]
fn source_function_runtime_coa01() {
    receive_coalesce("COA01");
}

#[test]
fn source_function_runtime_coa02() {
    receive_coalesce("COA02");
}

#[test]
fn source_function_runtime_coa03() {
    receive_coalesce("COA03");
}

#[test]
fn source_function_runtime_coa04() {
    receive_coalesce("COA04");
}

#[test]
fn source_function_runtime_coa05() {
    receive_coalesce("COA05");
}

#[test]
fn source_function_runtime_coa06() {
    receive_coalesce("COA06");
}

#[test]
fn source_function_runtime_coa07() {
    receive_coalesce("COA07");
}

#[test]
fn source_function_runtime_coa08() {
    receive_coalesce("COA08");
}

#[test]
fn source_function_runtime_coa09() {
    receive_coalesce("COA09");
}

#[test]
fn source_function_runtime_coa10() {
    receive_coalesce("COA10");
}

#[test]
fn source_function_runtime_coa11() {
    receive_coalesce("COA11");
}

#[test]
fn source_function_runtime_coa12() {
    receive_coalesce("COA12");
}

#[test]
fn source_function_control_earlier_factory_keeps_gate() {
    let session = source_bgp_receiving::session();
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        "SELECT (<http://www.w3.org/2001/XMLSchema#string>(1) AS ?v) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        session,
        Vec::new(),
    )
    .unwrap();
    assert!(prepared.execute_source_presence().is_err());
}

#[test]
fn source_function_control_mixed_language_requires_constructor() {
    use crate::numeric::source_function::SourceFunctionKind;
    use crate::numeric::{SourceDatatype, SourceFact, SourceLiteralCacheKind};
    use oxrdf::vocab::xsd;
    let session = source_bgp_receiving::session();
    let input = session
        .construct_source_direct_string("a", None, Some("EN".into()), true)
        .unwrap();
    let output = session
        .source_default_cast(SourceFunctionKind::String, &SolutionValue::Rdf(input))
        .unwrap();
    let crate::numeric::LiteralSourceState::Literal(meta) = output.literal_source() else {
        panic!("metadata");
    };
    assert_eq!(
        meta.datatype,
        SourceDatatype::Specified(xsd::STRING.into_owned())
    );
    assert_eq!(meta.language_original, SourceFact::Known(Some("EN".into())));
    assert_eq!(meta.ill_typed, SourceFact::Known(None));
    assert_eq!(
        meta.cached_value_kind,
        SourceFact::Known(SourceLiteralCacheKind::String)
    );
    let clone = output.clone();
    assert!(clone.same_source_object(&output));
    let (rdf, cache, meta) = output.clone().into_source_parts().unwrap();
    assert!(matches!(
        RawExpressionValue::from_source_parts(rdf, cache, meta),
        Err(RawNumericFault::InvalidSourceOrigin)
    ));
    let mut graph = source_bgp_receiving::dataset(&session, None);
    let term = graph.admit_source(output.clone()).unwrap();
    let transported = graph.externalize_source(term).unwrap();
    assert!(output.same_source_object(&transported));
    assert_eq!(
        source_bgp_receiving::term_json(&output, &session),
        source_bgp_receiving::term_json(&transported, &session)
    );
}

#[test]
fn source_function_control_language_identity_keeps_datatype() {
    use crate::numeric::source_function::SourceFunctionKind;
    use crate::numeric::source_identity_key;
    let session = source_bgp_receiving::session();
    let plain = session
        .construct_source_direct_string("a", None, Some("EN".into()), true)
        .unwrap();
    let low = session
        .construct_source_direct_string("a", None, Some("en".into()), true)
        .unwrap();
    let cast = session
        .source_default_cast(
            SourceFunctionKind::String,
            &SolutionValue::Rdf(plain.clone()),
        )
        .unwrap();
    let cast_low = session
        .source_default_cast(SourceFunctionKind::String, &SolutionValue::Rdf(low))
        .unwrap();
    let text = session
        .construct_source_direct_string(
            "a",
            Some(oxrdf::vocab::xsd::STRING.into_owned()),
            None,
            true,
        )
        .unwrap();
    assert_ne!(
        source_identity_key(&plain).unwrap(),
        source_identity_key(&cast).unwrap()
    );
    assert_ne!(
        source_identity_key(&text).unwrap(),
        source_identity_key(&cast).unwrap()
    );
    assert_eq!(
        source_identity_key(&cast).unwrap(),
        source_identity_key(&cast_low).unwrap()
    );
    assert!(!cast.same_source_object(&cast_low));
}

#[test]
fn source_function_control_cancel_precedes_dispatch() {
    let session = source_bgp_receiving::session();
    let flag = Arc::new(AtomicBool::new(false));
    let guard = crate::numeric::source_function::observation::Guard::start();
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        "SELECT (<http://www.w3.org/2001/XMLSchema#string>(1) AS ?v) WHERE {}",
        source_bgp_receiving::dataset(&session, Some(flag.clone())),
        session,
        Vec::new(),
    )
    .unwrap();
    assert!(guard.take().is_empty());
    drop(guard);
    let guard = crate::numeric::source_function::observation::Guard::start();
    flag.store(true, atomic::Ordering::Relaxed);
    assert!(matches!(
        prepared.execute_source_presence().err().unwrap(),
        QueryEvaluationError::Cancelled
    ));
    assert!(guard.take().is_empty());
}

#[test]
fn source_function_control_output_limit_remains_fatal() {
    let session = source_exists_runtime_receiving::session(
        &serde_json::json!({"input":{"decimal":{"precision":28,"trap_float_operation":false}}}),
        semantica_numeric::NumericLimits {
            max_output_characters: 4,
            ..Default::default()
        },
    );
    let prepared = numeric_query::prepare_source_graph_numeric_query(
        "SELECT (COALESCE(<http://www.w3.org/2001/XMLSchema#boolean>(\"0\"), 1) AS ?v) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        session,
        Vec::new(),
    )
    .unwrap();
    let mut rows = prepared.execute_source_presence().unwrap();
    let Err(error) = rows.next().unwrap() else {
        panic!("fatal budget converted into a value");
    };
    assert!(matches!(
        crate::numeric::raw_numeric_fault(&error),
        Some(RawNumericFault::Core(
            semantica_numeric::NumericFault::ResourceLimit {
                operation: "output_characters"
            }
        ))
    ));
    assert!(rows.next().is_none() && rows.next().is_none());
}

#[test]
fn source_function_control_unknown_source_facts_remain_fatal() {
    use crate::numeric::source_function::SourceFunctionKind;
    let session = source_bgp_receiving::session();
    let imported = RawExpressionValue::from_import(Literal::new_simple_literal("1").into());
    for kind in [
        SourceFunctionKind::Integer,
        SourceFunctionKind::String,
        SourceFunctionKind::Boolean,
    ] {
        assert!(matches!(
            session.source_default_cast(kind, &SolutionValue::Rdf(imported.clone())),
            Err(crate::numeric::NumericEvaluationError::Fatal(
                RawNumericFault::UnknownSourceLiteralIdentity
            ))
        ));
    }
}
