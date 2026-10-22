//! Frozen original Decimal context, language exceptions and callback observations.
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
fn private_fixture() -> &'static str {
    crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_decimal_trap_query_cases.json"
    )
}
fn receive(id: &str) {
    assert_eq!(
        format!("{:x}", Sha256::digest(private_fixture().as_bytes())),
        "c0e2fe2466920cf74fab37928f89a7cd4a11560d0e4ddfc894b684db4ddc1285"
    );
    let root: Value = serde_json::from_str(private_fixture()).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 93);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
}

#[test]
fn source_decimal_trap_dx01() {
    receive("DX01");
}

#[test]
fn source_decimal_trap_dx02() {
    receive("DX02");
}

#[test]
fn source_decimal_trap_dx03() {
    receive("DX03");
}

#[test]
fn source_decimal_trap_dx04() {
    receive("DX04");
}

#[test]
fn source_decimal_trap_dx05() {
    receive("DX05");
}

#[test]
fn source_decimal_trap_dx06() {
    receive("DX06");
}

#[test]
fn source_decimal_trap_dx07() {
    receive("DX07");
}

#[test]
fn source_decimal_trap_dx08() {
    receive("DX08");
}

#[test]
fn source_decimal_trap_dx09() {
    receive("DX09");
}

#[test]
fn source_decimal_trap_dx10() {
    receive("DX10");
}

#[test]
fn source_decimal_trap_dx11() {
    receive("DX11");
}

#[test]
fn source_decimal_trap_dx12() {
    receive("DX12");
}

#[test]
fn source_decimal_trap_dx13() {
    receive("DX13");
}

#[test]
fn source_decimal_trap_dx14() {
    receive("DX14");
}

#[test]
fn source_decimal_trap_dx15() {
    receive("DX15");
}

#[test]
fn source_decimal_trap_dx16() {
    receive("DX16");
}

#[test]
fn source_decimal_trap_dx17() {
    receive("DX17");
}

#[test]
fn source_decimal_trap_dx18() {
    receive("DX18");
}

#[test]
fn source_decimal_trap_dx19() {
    receive("DX19");
}

#[test]
fn source_decimal_trap_dx20() {
    receive("DX20");
}

#[test]
fn source_decimal_trap_dx21() {
    receive("DX21");
}

#[test]
fn source_decimal_trap_dx22() {
    receive("DX22");
}

#[test]
fn source_decimal_trap_dx23() {
    receive("DX23");
}

#[test]
fn source_decimal_trap_dx24() {
    receive("DX24");
}

#[test]
fn source_decimal_trap_dx25() {
    receive("DX25");
}

#[test]
fn source_decimal_trap_dx26() {
    receive("DX26");
}

#[test]
fn source_decimal_trap_dx27() {
    receive("DX27");
}

#[test]
fn source_decimal_trap_dx28() {
    receive("DX28");
}

#[test]
fn source_decimal_trap_dx29() {
    receive("DX29");
}

#[test]
fn source_decimal_trap_dx30() {
    receive("DX30");
}

#[test]
fn source_decimal_trap_dx31() {
    receive("DX31");
}

#[test]
fn source_decimal_trap_dx32() {
    receive("DX32");
}

#[test]
fn source_decimal_trap_dx33() {
    receive("DX33");
}

#[test]
fn source_decimal_trap_dx34() {
    receive("DX34");
}

#[test]
fn source_decimal_trap_dx35() {
    receive("DX35");
}

#[test]
fn source_decimal_trap_dx36() {
    receive("DX36");
}

#[test]
fn source_decimal_trap_dx37() {
    receive("DX37");
}

#[test]
fn source_decimal_trap_dx38() {
    receive("DX38");
}

#[test]
fn source_decimal_trap_dx39() {
    receive("DX39");
}

#[test]
fn source_decimal_trap_dx40() {
    receive("DX40");
}

#[test]
fn source_decimal_trap_dx41() {
    receive("DX41");
}

#[test]
fn source_decimal_trap_dx42() {
    receive("DX42");
}

#[test]
fn source_decimal_trap_dx43() {
    receive("DX43");
}

#[test]
fn source_decimal_trap_dx44() {
    receive("DX44");
}

#[test]
fn source_decimal_trap_dx45() {
    receive("DX45");
}

#[test]
fn source_decimal_trap_dx46() {
    receive("DX46");
}

#[test]
fn source_decimal_trap_dx47() {
    receive("DX47");
}

#[test]
fn source_decimal_trap_dx48() {
    receive("DX48");
}

#[test]
fn source_decimal_trap_dx49() {
    receive("DX49");
}

#[test]
fn source_decimal_trap_dx50() {
    receive("DX50");
}

#[test]
fn source_decimal_trap_dx51() {
    receive("DX51");
}

#[test]
fn source_decimal_trap_dx52() {
    receive("DX52");
}

#[test]
fn source_decimal_trap_dx53() {
    receive("DX53");
}

#[test]
fn source_decimal_trap_dx54() {
    receive("DX54");
}

#[test]
fn source_decimal_trap_dx55() {
    receive("DX55");
}

#[test]
fn source_decimal_trap_dx56() {
    receive("DX56");
}

#[test]
fn source_decimal_trap_dx57() {
    receive("DX57");
}

#[test]
fn source_decimal_trap_dx58() {
    receive("DX58");
}

#[test]
fn source_decimal_trap_dx59() {
    receive("DX59");
}

#[test]
fn source_decimal_trap_dx60() {
    receive("DX60");
}

#[test]
fn source_decimal_trap_dx61() {
    receive("DX61");
}

#[test]
fn source_decimal_trap_dx62() {
    receive("DX62");
}

#[test]
fn source_decimal_trap_dx63() {
    receive("DX63");
}

#[test]
fn source_decimal_trap_dx64() {
    receive("DX64");
}

#[test]
fn source_decimal_trap_dx65() {
    receive("DX65");
}

#[test]
fn source_decimal_trap_dx66() {
    receive("DX66");
}

#[test]
fn source_decimal_trap_dx67() {
    receive("DX67");
}

#[test]
fn source_decimal_trap_dx68() {
    receive("DX68");
}

#[test]
fn source_decimal_trap_dx69() {
    receive("DX69");
}

#[test]
fn source_decimal_trap_dx70() {
    receive("DX70");
}

#[test]
fn source_decimal_trap_dx71() {
    receive("DX71");
}

#[test]
fn source_decimal_trap_dx72() {
    receive("DX72");
}

#[test]
fn source_decimal_trap_dx73() {
    receive("DX73");
}

#[test]
fn source_decimal_trap_dx74() {
    receive("DX74");
}

#[test]
fn source_decimal_trap_dx75() {
    receive("DX75");
}

#[test]
fn source_decimal_trap_dx76() {
    receive("DX76");
}

#[test]
fn source_decimal_trap_dx77() {
    receive("DX77");
}

#[test]
fn source_decimal_trap_dx78() {
    receive("DX78");
}

#[test]
fn source_decimal_trap_dx79() {
    receive("DX79");
}

#[test]
fn source_decimal_trap_dx80() {
    receive("DX80");
}

#[test]
fn source_decimal_trap_dx81() {
    receive("DX81");
}

#[test]
fn source_decimal_trap_dx82() {
    receive("DX82");
}

#[test]
fn source_decimal_trap_dx83() {
    receive("DX83");
}

#[test]
fn source_decimal_trap_dx84() {
    receive("DX84");
}

#[test]
fn source_decimal_trap_dx85() {
    receive("DX85");
}

#[test]
fn source_decimal_trap_dx86() {
    receive("DX86");
}

#[test]
fn source_decimal_trap_dx87() {
    receive("DX87");
}

#[test]
fn source_decimal_trap_df01() {
    receive("DF01");
}

#[test]
fn source_decimal_trap_df02() {
    receive("DF02");
}

#[test]
fn source_decimal_trap_df03() {
    receive("DF03");
}

#[test]
fn source_decimal_trap_df04() {
    receive("DF04");
}

#[test]
fn source_decimal_trap_df05() {
    receive("DF05");
}

#[test]
fn source_decimal_trap_df06() {
    receive("DF06");
}

fn receive_text(id: &str) {
    let fixture = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_decimal_trap_text_query_cases.json"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture.as_bytes())),
        "c0200ce33b488f818a10d317f1cd7125633f32db27ff94df709b9de90f5c7826"
    );
    let root: Value = serde_json::from_str(fixture).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 3);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
}

#[test]
fn source_decimal_trap_dt01() {
    receive_text("DT01");
}

#[test]
fn source_decimal_trap_dt02() {
    receive_text("DT02");
}

#[test]
fn source_decimal_trap_dt03() {
    receive_text("DT03");
}

#[test]
fn source_decimal_trap_legacy_profile_keeps_core_fault() {
    use crate::numeric::{NumericEvaluationError, NumericUnaryOp, QueryNumericCapabilities};
    use semantica_numeric::{DecimalContext, DecimalSignal, NumericFault, NumericLimits};
    let session = QueryNumericSession::new_with_capabilities(
        DecimalContext::default(),
        NumericLimits::default(),
        None,
        true,
        QueryNumericCapabilities::MixedBinary64V1,
    )
    .unwrap();
    let raw = session
        .construct_literal(
            &Literal::new_typed_literal("sNaN", oxrdf::vocab::xsd::DECIMAL),
            crate::numeric::NumericInputOrigin::QueryTypedExistingLiteral,
        )
        .unwrap();
    assert!(matches!(
        session.evaluate_unary(NumericUnaryOp::Plus, SolutionValue::Rdf(raw)),
        Err(NumericEvaluationError::Fatal(RawNumericFault::Core(
            NumericFault::DecimalTrap {
                primary: DecimalSignal::InvalidOperation,
                ..
            }
        )))
    ));
}

#[test]
fn source_decimal_trap_without_registry_keeps_constructor_and_flags() {
    use crate::numeric::source_exception::{
        LanguageExceptionArgument, SourceLanguageExceptionKind,
    };
    use semantica_numeric::{DecimalContext, DecimalSignal, DecimalSignals};
    for multi in [false, true] {
        let trapped = if multi {
            vec![
                DecimalSignal::Overflow,
                DecimalSignal::Inexact,
                DecimalSignal::Rounded,
            ]
        } else {
            vec![DecimalSignal::Overflow]
        };
        let session = source_bgp_receiving::session_with_decimal(DecimalContext {
            precision: 3,
            emin: (-2).into(),
            emax: 2.into(),
            traps: trapped.iter().copied().collect(),
            ..DecimalContext::default()
        });
        assert!(session.source_functions().is_none());
        let query = numeric_query::prepare_source_graph_numeric_query(
            "SELECT (99.0 * 99.0 AS ?v) WHERE {}",
            source_bgp_receiving::dataset(&session, None),
            Rc::clone(&session),
            Vec::new(),
        )
        .unwrap();
        let mut rows = query.execute_source_presence().unwrap();
        let error = rows.next().unwrap().err().expect("original overflow trap");
        let Some(RawNumericFault::SourceLanguageException(exception)) =
            crate::numeric::raw_numeric_fault(&error)
        else {
            panic!("lost source Decimal trap: {error:?}");
        };
        assert_eq!(
            exception.kind,
            SourceLanguageExceptionKind::Decimal(DecimalSignal::Overflow)
        );
        assert_eq!(
            exception.arguments,
            vec![LanguageExceptionArgument::DecimalSignalClasses(trapped)]
        );
        let all: DecimalSignals = [
            DecimalSignal::Overflow,
            DecimalSignal::Inexact,
            DecimalSignal::Rounded,
        ]
        .into_iter()
        .collect();
        assert_eq!(session.flags(), all);
        for _ in 0..3 {
            assert!(rows.next().is_none());
        }
    }
}

#[test]
fn source_decimal_trap_cannot_swallow_resource_failure() {
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
                                Err(SourceFunctionFailure::Exception(SourceFunctionException {
                                    kind: SourceFunctionExceptionKind::Decimal(
                                        semantica_numeric::DecimalSignal::InvalidOperation,
                                    ),
                                    arguments: vec![SourceFunctionArgument::DecimalSignalClasses(
                                        vec![semantica_numeric::DecimalSignal::InvalidOperation],
                                    )],
                                }))
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

fn receive_str(id: &str) {
    let fixture = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/vendor/spareval/src/eval/fixtures/source_decimal_trap_str_query_cases.json"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture.as_bytes())),
        "c3b62e098bc95b4685ec87b84ebae5e837eddd62e17569f92813b6ec0891660e"
    );
    let root: Value = serde_json::from_str(fixture).unwrap();
    assert_eq!(root["candidate_results_used"], false);
    assert_eq!(root["graph_queries"].as_array().unwrap().len(), 12);
    let case = root["graph_queries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["input"]["id"] == id)
        .unwrap();
    source_function_registry_receiving::receive_case(case, &root["data_inputs"]);
}

#[test]
fn source_decimal_trap_ds01() {
    receive_str("DS01");
}

#[test]
fn source_decimal_trap_ds02() {
    receive_str("DS02");
}

#[test]
fn source_decimal_trap_ds03() {
    receive_str("DS03");
}

#[test]
fn source_decimal_trap_ds04() {
    receive_str("DS04");
}

#[test]
fn source_decimal_trap_ds05() {
    receive_str("DS05");
}

#[test]
fn source_decimal_trap_ds06() {
    receive_str("DS06");
}

#[test]
fn source_decimal_trap_ds07() {
    receive_str("DS07");
}

#[test]
fn source_decimal_trap_ds08() {
    receive_str("DS08");
}

#[test]
fn source_decimal_trap_ds09() {
    receive_str("DS09");
}

#[test]
fn source_decimal_trap_ds10() {
    receive_str("DS10");
}

#[test]
fn source_decimal_trap_ds11() {
    receive_str("DS11");
}

#[test]
fn source_decimal_trap_ds12() {
    receive_str("DS12");
}

#[test]
fn source_decimal_trap_str_keeps_sealed_frontend_closed() {
    let session = source_bgp_receiving::session();
    let query = numeric_query::prepare_source_frontend_numeric_query(
        "SELECT (STR(\"value\") AS ?v) WHERE {}",
        source_bgp_receiving::dataset(&session, None),
        session,
        Vec::new(),
    )
    .unwrap();
    let error = match query.execute_source_presence() {
        Ok(_) => panic!("source STR granted sealed frontend execution"),
        Err(error) => error,
    };
    assert_eq!(error.to_string(), "unsupported nullable expression");
}

// Historical fixtures specify an observable Decimal class, not this crate's
// old Core enum representation. Keep that criterion for both owning paths;
// the new original receivers independently require exact source constructors.
pub(super) fn assert_original_decimal_class(
    error: &QueryEvaluationError,
    expected: semantica_numeric::DecimalSignal,
) {
    use crate::numeric::source_exception::{
        LanguageExceptionArgument, SourceLanguageExceptionKind,
    };
    use semantica_numeric::NumericFault;
    let fault = crate::numeric::raw_numeric_fault(error).expect("typed numerical source failure");
    match fault {
        RawNumericFault::Core(NumericFault::DecimalTrap { primary, signals }) => {
            assert_eq!(*primary, expected);
            assert!(signals.contains(expected), "original trap flag lost");
        }
        RawNumericFault::SourceLanguageException(exception) => {
            assert_eq!(
                exception.kind,
                SourceLanguageExceptionKind::Decimal(expected)
            );
            assert_eq!(
                exception.arguments,
                vec![LanguageExceptionArgument::DecimalSignalClasses(vec![
                    expected
                ])]
            );
        }
        other => panic!("expected original decimal.{expected:?}, received {other:?}"),
    }
}
