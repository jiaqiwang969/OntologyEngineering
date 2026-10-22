//! Fixed source arithmetic/callback observations; no candidate-derived expectations.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_function_exception_query_cases.json"
    )
}
fn receive(id: &str) {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_fixture().as_bytes())),
        "998dc1f356afba6edd28989f4d4685a5fc700fc1bf6977d55c21132beb1bbd56"
    );
    let root: Value = serde_json::from_str(private_fixture()).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 36);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
}

#[test]
fn source_function_exception_fx01() {
    receive("FX01");
}

#[test]
fn source_function_exception_fx02() {
    receive("FX02");
}

#[test]
fn source_function_exception_fx03() {
    receive("FX03");
}

#[test]
fn source_function_exception_fx04() {
    receive("FX04");
}

#[test]
fn source_function_exception_fx05() {
    receive("FX05");
}

#[test]
fn source_function_exception_fx06() {
    receive("FX06");
}

#[test]
fn source_function_exception_fx07() {
    receive("FX07");
}

#[test]
fn source_function_exception_fx08() {
    receive("FX08");
}

#[test]
fn source_function_exception_fx09() {
    receive("FX09");
}

#[test]
fn source_function_exception_fx10() {
    receive("FX10");
}

#[test]
fn source_function_exception_fx11() {
    receive("FX11");
}

#[test]
fn source_function_exception_fx12() {
    receive("FX12");
}

#[test]
fn source_function_exception_fx13() {
    receive("FX13");
}

#[test]
fn source_function_exception_fx14() {
    receive("FX14");
}

#[test]
fn source_function_exception_fx15() {
    receive("FX15");
}

#[test]
fn source_function_exception_fx16() {
    receive("FX16");
}

#[test]
fn source_function_exception_fx17() {
    receive("FX17");
}

#[test]
fn source_function_exception_fx18() {
    receive("FX18");
}

#[test]
fn source_function_exception_fx19() {
    receive("FX19");
}

#[test]
fn source_function_exception_fx20() {
    receive("FX20");
}

#[test]
fn source_function_exception_fx21() {
    receive("FX21");
}

#[test]
fn source_function_exception_fx22() {
    receive("FX22");
}

#[test]
fn source_function_exception_fx23() {
    receive("FX23");
}

#[test]
fn source_function_exception_fx24() {
    receive("FX24");
}

#[test]
fn source_function_exception_fx25() {
    receive("FX25");
}

#[test]
fn source_function_exception_fx26() {
    receive("FX26");
}

#[test]
fn source_function_exception_fx27() {
    receive("FX27");
}

#[test]
fn source_function_exception_fx28() {
    receive("FX28");
}

#[test]
fn source_function_exception_fx29() {
    receive("FX29");
}

#[test]
fn source_function_exception_fx30() {
    receive("FX30");
}

#[test]
fn source_function_exception_fx31() {
    receive("FX31");
}

#[test]
fn source_function_exception_fx32() {
    receive("FX32");
}

#[test]
fn source_function_exception_fx33() {
    receive("FX33");
}

#[test]
fn source_function_exception_fx34() {
    receive("FX34");
}

#[test]
fn source_function_exception_fx35() {
    receive("FX35");
}

#[test]
fn source_function_exception_fx36() {
    receive("FX36");
}

#[test]
fn source_function_exception_legacy_profile_keeps_original_fault() {
    use crate::numeric::{
        NumericBinaryOp, NumericChainKind, NumericEvaluationError, QueryNumericCapabilities,
    };
    use spargebra::{NumericTokenKind, NumericTokenSign};
    let session = QueryNumericSession::new_with_capabilities(
        semantica_numeric::DecimalContext::default(),
        semantica_numeric::NumericLimits::default(),
        None,
        true,
        QueryNumericCapabilities::MixedBinary64V1,
    )
    .unwrap();
    let value = |kind, text| {
        SolutionValue::Rdf(
            session
                .construct_numeric_token(kind, NumericTokenSign::Unsigned, text)
                .unwrap(),
        )
    };
    for operator in [NumericBinaryOp::Multiply, NumericBinaryOp::Divide] {
        assert!(matches!(
            session.evaluate_chain(
                NumericChainKind::Multiplicative,
                value(NumericTokenKind::Integer, "1"),
                vec![
                    (
                        NumericBinaryOp::Multiply,
                        value(NumericTokenKind::Double, "2e0")
                    ),
                    (operator, value(NumericTokenKind::Decimal, "3.0")),
                ],
            ),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::MixedNumericTypeError
            ))
        ));
    }
}

#[test]
fn source_function_exception_without_registry_keeps_constructor() {
    use crate::numeric::source_exception::{
        LanguageExceptionArgument, SourceLanguageExceptionKind,
    };
    for operator in ["*", "/"] {
        let session = source_bgp_receiving::session();
        assert!(session.source_functions().is_none());
        let query = numeric_query::prepare_source_graph_numeric_query(
            &format!("SELECT (1 * 2e0 {operator} 3.0 AS ?v) WHERE {{}}"),
            source_bgp_receiving::dataset(&session, None),
            session,
            Vec::new(),
        )
        .unwrap();
        let mut rows = query.execute_source_presence().unwrap();
        let error = rows
            .next()
            .unwrap()
            .err()
            .expect("source arithmetic must fail");
        let Some(RawNumericFault::SourceLanguageException(exception)) =
            crate::numeric::raw_numeric_fault(&error)
        else {
            panic!("source arithmetic lost its originating exception: {error:?}");
        };
        assert_eq!(exception.kind, SourceLanguageExceptionKind::TypeError);
        assert_eq!(
            exception.arguments,
            vec![LanguageExceptionArgument::Text(format!(
                "unsupported operand type(s) for {operator}=: 'float' and 'decimal.Decimal'"
            ))]
        );
        for _ in 0..3 {
            assert!(rows.next().is_none());
        }
    }
}

#[test]
fn source_function_exception_cannot_swallow_resource_failure() {
    use crate::numeric::source_registry::*;
    use crate::numeric::{
        QueryContextSemantics, QueryGroupSemantics, QueryNumericCapabilities, SourceHashPolicy,
        SourceHashRuntime,
    };
    use semantica_numeric::{DecimalContext, NumericFault, NumericLimits};
    use std::cell::Cell;
    for substitute_exception in [false, true] {
        let registry = Rc::new(SourceFunctionRegistry::stock());
        let session = Rc::new(
            QueryNumericSession::new_with_context_semantics(
                DecimalContext::default(),
                NumericLimits {
                    max_output_characters: 3,
                    ..NumericLimits::default()
                },
                None,
                true,
                QueryNumericCapabilities::MixedBinary64V1,
                QueryGroupSemantics::SourceAggregateV1,
                QueryLiteralSemantics::SourceLiteralV1,
                QueryContextSemantics::SourceContextV1,
                Some(Rc::new(
                    SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap(),
                )),
            )
            .unwrap()
            .with_source_functions(Rc::clone(&registry))
            .unwrap(),
        );
        let called = Rc::new(Cell::new(false));
        let observed = Rc::clone(&called);
        registry
            .register(
                NamedNode::new("urn:f").unwrap(),
                SourceFunctionDefinition {
                    raw: true,
                    callable: SourceCallable::Native {
                        qualified_name: Rc::from("receiving.resource_swallow"),
                        callback: Rc::new(move |invocation| {
                            observed.set(true);
                            let SourceFunctionInvocation::Raw(ctx) = invocation else {
                                panic!("raw callback");
                            };
                            assert!(matches!(ctx.arguments(), Err(SourceFunctionFailure::Host)));
                            if substitute_exception {
                                Err(SourceFunctionFailure::Exception(
                                    SourceFunctionException::text(
                                        SourceFunctionExceptionKind::TypeError,
                                        "replacement",
                                    ),
                                ))
                            } else {
                                Ok(SolutionValue::ExplicitNone)
                            }
                        }),
                    },
                },
                false,
            )
            .unwrap();
        let query = numeric_query::prepare_source_graph_numeric_query(
            "SELECT (<urn:f>(99 * 99) AS ?v) WHERE {}",
            source_bgp_receiving::dataset(&session, None),
            session,
            Vec::new(),
        )
        .unwrap();
        let mut rows = query.execute_source_presence().unwrap();
        let error = rows
            .next()
            .unwrap()
            .err()
            .expect("callback hid a host resource limit");
        assert!(
            called.get(),
            "resource failure did not originate in the callback read"
        );
        assert!(
            matches!(
                crate::numeric::raw_numeric_fault(&error),
                Some(RawNumericFault::Core(NumericFault::ResourceLimit {
                    operation: "Decimal coefficient or output digits"
                }))
            ),
            "original resource failure changed: {error:?}"
        );
        for _ in 0..3 {
            assert!(rows.next().is_none());
        }
    }
}
