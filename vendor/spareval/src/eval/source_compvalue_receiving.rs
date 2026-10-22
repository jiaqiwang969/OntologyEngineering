//! Full unchanged original queries, and separately frozen native set protocols.
//! Test adapters and production have one author; no independent review claim.
use super::source_compvalue::{SourceCompValueError, render_mapping, reserve_syntax_text};
use super::source_exists_runtime_receiving::{Backend, case, first_source_expression, session};
use super::*;
use crate::numeric::{
    NumericInitialBinding, NumericInitialValue, PreparedSourceLiteralRegistry, SourceHashPolicy,
    SourceHashRuntime, raw_numeric_fault,
};
use crate::tuple::{
    ErrorValue, SourceException, SourceExceptionArgument, SourceExceptionClass, SparqlErrorKind,
};
use semantica_numeric::{NumericFault, NumericLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use spargebra::{
    SourceSyntaxAttributes, SourceSyntaxNode, SourceSyntaxValue as V, SourceVariableSetStep as S,
};

const FIXTURE: &str = include_str!("fixtures/source_compvalue_protocols.json");
fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "d4a28e64533328a421521360627a510207c6a56e0193cf7a2e788d441b2cca88"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        value["source_record_sha256"],
        "bbf1151918bd72919f40fe808fff2a032b86742f5a43d9be2101af1788f15772"
    );
    assert_eq!(value["candidate_results_used"], false);
    assert_eq!(value["protocols"].as_array().unwrap().len(), 8);
    value
}
fn initial(case: &Value) -> Vec<NumericInitialBinding> {
    case["input"]["initial_bindings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| {
            let v = &input["value"];
            let value = match v["kind"].as_str().unwrap() {
                "none" => NumericInitialValue::SourceNone,
                "error" => {
                    assert_eq!(v["class"], "NotBoundError");
                    assert_eq!(v["argument"], Value::Null);
                    NumericInitialValue::SourceError(ErrorValue::with_source(
                        SparqlErrorKind::NotBound,
                        "None",
                        SourceException {
                            class: SourceExceptionClass::NotBound,
                            argument: SourceExceptionArgument::None,
                        },
                    ))
                }
                "literal" => NumericInitialValue::SourceDirectString {
                    lexical: v["lexical"].as_str().unwrap().into(),
                    datatype: v["datatype"].as_str().map(|v| NamedNode::new(v).unwrap()),
                    language: v["language"].as_str().map(str::to_owned),
                    normalize: v["normalize"].as_bool().unwrap(),
                },
                _ => panic!("unreceived original initial input"),
            };
            NumericInitialBinding {
                variable: Variable::new(input["variable"].as_str().unwrap()).unwrap(),
                value,
            }
        })
        .collect()
}
fn assert_error(case: &Value, error: &QueryEvaluationError) {
    let QueryEvaluationError::Unexpected(error) = error else {
        panic!("generic Exception became a caught/other error: {error:?}");
    };
    let error = error
        .downcast_ref::<SourceCompValueError>()
        .expect("original generic CompValue exception");
    assert_eq!(error.module, case["error"]["module"]);
    assert_eq!(error.class, case["error"]["class"]);
    assert_eq!(json!([error.argument]), case["error"]["args"]);
}
fn receive(id: &str) {
    let case = case(id);
    assert_eq!(case["original_status"], "original_error");
    let session = session(&case, NumericLimits::default());
    let backend = Backend::default();
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        backend.clone(),
        Rc::clone(&session),
        initial(&case),
    )
    .unwrap();
    match case["error_stage"].as_str().unwrap() {
        "evaluate_start" => match prepared.execute_source_presence() {
            Err(error) => assert_error(&case, &error),
            Ok(_) => panic!("{id}: eager failure was deferred"),
        },
        "first_poll" => {
            let mut rows = prepared.execute_source_presence().unwrap();
            match rows.next().unwrap() {
                Err(error) => assert_error(&case, &error),
                Ok(_) => panic!("{id}: swallowed generic exception"),
            }
            assert!(rows.next().is_none());
            assert!(rows.next().is_none());
        }
        _ => panic!("unreceived stage"),
    }
    assert_eq!(
        json!(
            session
                .flags()
                .iter()
                .map(|signal| format!("{signal:?}"))
                .collect::<Vec<_>>()
        ),
        case["flags"]
    );
    assert_eq!(backend.scans.get(), 0);
}
fn protocol(id: &str) {
    let fixture = fixture();
    let protocol = fixture["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["input"]["id"] == id)
        .unwrap();
    let steps: Vec<_> = protocol["input"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| match s["op"].as_str().unwrap() {
            "new" => S::New(
                s["members"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| Variable::new(v.as_str().unwrap()).unwrap())
                    .collect(),
            ),
            "copy" => S::Copy(s["child"].as_u64().unwrap() as usize),
            "union" => S::Union(
                s["children"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as usize)
                    .collect(),
            ),
            _ => panic!("unknown set opcode"),
        })
        .collect();
    let hash = SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap();
    let actual = compile_source_variable_sets(&steps, &hash).unwrap();
    assert_eq!(actual.len(), protocol["expected"].as_array().unwrap().len());
    for (members, expected) in actual.iter().zip(protocol["expected"].as_array().unwrap()) {
        assert_eq!(
            json!(members.iter().map(Variable::as_str).collect::<Vec<_>>()),
            expected["iteration"]
        );
        assert_eq!(
            json!(
                members
                    .iter()
                    .map(|v| json!([v.as_str(), hash.variable_hash(v.as_str())]))
                    .collect::<Vec<_>>()
            ),
            expected["hashes"]
        );
        let session = session(&case("EX01"), NumericLimits::default());
        let registry = PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(&session));
        let nodes = [node(
            "Vars",
            vec![("_vars", V::VariableSet(members.clone()))],
        )];
        let text = render_mapping(&nodes, 0, false, &session, &registry, &mut || Ok(())).unwrap();
        assert_eq!(
            text,
            format!("Vars_{{'_vars': {}}}", expected["repr"].as_str().unwrap())
        );
    }
}
fn node(name: &str, fields: Vec<(&str, V)>) -> SourceSyntaxNode {
    SourceSyntaxNode {
        name: name.into(),
        fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        attributes: SourceSyntaxAttributes::default(),
    }
}

#[test]
fn source_compvalue_original_component_str_repr_and_generic_exception() {
    let case = case("EX03");
    let session = session(&case, NumericLimits::default());
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        Backend::default(),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let Expression::SourceExists(expression) = first_source_expression(prepared.plan()) else {
        panic!("missing EXISTS");
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
    let original = fixture();
    let expected = &original["original_compvalue_representations"];
    for (str_mode, key) in [(false, "repr"), (true, "str")] {
        let text = render_mapping(
            expression.syntax.nodes(),
            source_compvalue::untranslated_graph_node(&expression.syntax).unwrap(),
            str_mode,
            &session,
            registry,
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(text, expected[key]);
    }
    let error = source_compvalue::source_compvalue_error(
        &expression.syntax,
        &session,
        registry,
        &mut || Ok(()),
    )
    .unwrap();
    assert_eq!(error.class, expected["exception"]["class_name"]);
    assert_eq!(error.module, expected["exception"]["module"]);
    assert_eq!(json!([error.argument]), expected["exception"]["args"]);
}

#[test]
fn source_compvalue_controls_quoting_nested_aliases_and_instance_attributes() {
    let session = session(&case("EX01"), NumericLimits::default());
    let registry = PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(&session));
    let mut nodes = vec![
        node(
            "Root",
            vec![
                ("quoted'", V::Text("line\n\"a\"'é".into())),
                ("children", V::List(vec![V::Node(1), V::Node(1)])),
            ],
        ),
        node(
            "Child",
            vec![
                ("none", V::None),
                ("flag", V::Boolean(true)),
                ("n", V::Unsigned(2)),
            ],
        ),
    ];
    nodes[0].attributes.part = Some(vec![V::Node(99)]); // instance-only, not a mapping child
    let repr = render_mapping(&nodes, 0, false, &session, &registry, &mut || Ok(())).unwrap();
    assert_eq!(
        repr,
        "Root_{\"quoted'\": 'line\\n\"a\"\\'é', 'children': [Child_{'none': None, 'flag': True, 'n': 2}, Child_{'none': None, 'flag': True, 'n': 2}]}"
    );
    let text = render_mapping(&nodes, 0, true, &session, &registry, &mut || Ok(())).unwrap();
    assert_eq!(text, format!("Root_{repr}"));
}
#[test]
fn source_compvalue_controls_foreign_owner_missing_node_and_cycle_refuse() {
    let session = session(&case("EX01"), NumericLimits::default());
    let registry = PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(&session));
    let foreign = source_exists_runtime_receiving::session(&case("EX01"), NumericLimits::default());
    let cyclic = [node("Cycle", vec![("next", V::Node(0))])];
    for error in [
        render_mapping(&[], 0, true, &session, &registry, &mut || Ok(())).unwrap_err(),
        render_mapping(&cyclic, 0, true, &session, &registry, &mut || Ok(())).unwrap_err(),
        render_mapping(
            &[node("Root", vec![])],
            0,
            true,
            &foreign,
            &registry,
            &mut || Ok(()),
        )
        .unwrap_err(),
    ] {
        assert!(matches!(
            raw_numeric_fault(&error),
            Some(RawNumericFault::InvalidSourceOrigin)
        ));
    }
}
#[test]
fn source_compvalue_controls_unreceived_container_and_set_program_refuse() {
    let session = session(&case("EX01"), NumericLimits::default());
    let registry = PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(&session));
    let nodes = [node("Root", vec![("triples", V::Triples(Vec::new()))])];
    let error = render_mapping(&nodes, 0, true, &session, &registry, &mut || Ok(())).unwrap_err();
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::UnsupportedSourceLiteralOperator(_))
    ));
    let hash = session.source_hash().unwrap();
    for steps in [vec![S::Copy(0)], vec![S::Union(vec![1])]] {
        assert!(matches!(
            compile_source_variable_sets(&steps, hash),
            Err(RawNumericFault::InvalidSourceOrigin)
        ));
    }
}
#[test]
fn source_compvalue_controls_actual_allocation_depth_and_cancellation() {
    let mut text = String::from("kept");
    let error = reserve_syntax_text(&mut text, usize::MAX).unwrap_err();
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::Core(NumericFault::ResourceLimit {
            operation: "source_compvalue_text"
        }))
    ));
    assert_eq!(text, "kept");
    let session = session(&case("EX01"), NumericLimits::default());
    let registry = PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(&session));
    let nodes: Vec<_> = (0..257)
        .map(|id| {
            node(
                "Deep",
                if id == 256 {
                    vec![]
                } else {
                    vec![("next", V::Node(id + 1))]
                },
            )
        })
        .collect();
    let error = render_mapping(&nodes, 0, true, &session, &registry, &mut || Ok(())).unwrap_err();
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::Core(NumericFault::ResourceLimit {
            operation: "source_compvalue_depth"
        }))
    ));
    let error = render_mapping(&nodes, 0, true, &session, &registry, &mut || {
        Err(QueryEvaluationError::Cancelled)
    })
    .unwrap_err();
    assert!(matches!(error, QueryEvaluationError::Cancelled));
}
#[test]
fn source_compvalue_controls_actual_cancel_precedes_error_and_fuses() {
    let case = case("EX01");
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        Backend::default(),
        session(&case, NumericLimits::default()),
        Vec::new(),
    )
    .unwrap();
    let token = prepared.evaluator().dataset.cancellation_token.clone();
    let mut rows = prepared.execute_source_presence().unwrap();
    token.cancel();
    assert!(matches!(
        rows.next(),
        Some(Err(QueryEvaluationError::Cancelled))
    ));
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
}
#[test]
fn source_compvalue_controls_generic_error_precedes_boolean_result_budget() {
    let case = case("EX01");
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        Backend::default(),
        session(
            &case,
            NumericLimits {
                max_output_characters: 0,
                ..NumericLimits::default()
            },
        ),
        Vec::new(),
    )
    .unwrap();
    let mut rows = prepared.execute_source_presence().unwrap();
    let error = match rows.next().unwrap() {
        Err(error) => error,
        Ok(_) => panic!("generic Exception was swallowed"),
    };
    assert_error(&case, &error);
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
}
#[test]
fn source_compvalue_controls_actual_missing_owner_cannot_make_generic_error() {
    use crate::expression::{NumericExpressionHooks, build_numeric_expression_evaluator};
    let case = case("EX01");
    let session = session(&case, NumericLimits::default());
    let prepared = numeric_query::prepare_source_exists_numeric_query(
        case["input"]["query"].as_str().unwrap(),
        Backend::default(),
        Rc::clone(&session),
        Vec::new(),
    )
    .unwrap();
    let evaluator = prepared.evaluator();
    let codec = evaluator.dataset.numeric.as_ref().unwrap();
    let runtime = evaluator.source_context.as_ref().unwrap();
    let mut variables = runtime.variables.to_vec();
    let mut stats = Vec::new();
    let mut context = ExpressionContext {
        evaluator,
        encoded_variables: &mut variables,
        stat_children: &mut stats,
    };
    let expression = build_numeric_expression_evaluator(
        first_source_expression(prepared.plan()),
        &mut context,
        NumericExpressionHooks {
            session,
            prepared_literals: codec.prepared_literals.clone(),
            externalize: Rc::clone(&codec.externalize),
            failure: |error| QueryEvaluationError::Unexpected(Box::new(error)),
        },
    )
    .unwrap();
    let Err(EvalFailure::Fatal(error)) = expression(&InternalTuple::with_capacity(variables.len()))
    else {
        panic!("missing owner was accepted");
    };
    assert!(matches!(
        raw_numeric_fault(&error),
        Some(RawNumericFault::InvalidSourceOrigin)
    ));
}

#[test]
fn source_compvalue_original_ex01() {
    receive("EX01");
}
#[test]
fn source_compvalue_original_ex02() {
    receive("EX02");
}
#[test]
fn source_compvalue_original_ex03() {
    receive("EX03");
}
#[test]
fn source_compvalue_original_ex04() {
    receive("EX04");
}
#[test]
fn source_compvalue_original_ex05() {
    receive("EX05");
}
#[test]
fn source_compvalue_original_ex06() {
    receive("EX06");
}
#[test]
fn source_compvalue_original_ex07() {
    receive("EX07");
}
#[test]
fn source_compvalue_original_ex08() {
    receive("EX08");
}
#[test]
fn source_compvalue_original_ex09() {
    receive("EX09");
}
#[test]
fn source_compvalue_original_ex10() {
    receive("EX10");
}
#[test]
fn source_compvalue_original_ex11() {
    receive("EX11");
}
#[test]
fn source_compvalue_original_ex12() {
    receive("EX12");
}
#[test]
fn source_compvalue_original_ex13() {
    receive("EX13");
}
#[test]
fn source_compvalue_original_ex14() {
    receive("EX14");
}
#[test]
fn source_compvalue_original_ex15() {
    receive("EX15");
}
#[test]
fn source_compvalue_original_ex18() {
    receive("EX18");
}
#[test]
fn source_compvalue_original_ex19() {
    receive("EX19");
}
#[test]
fn source_compvalue_original_ex20() {
    receive("EX20");
}
#[test]
fn source_compvalue_original_ex21() {
    receive("EX21");
}
#[test]
fn source_compvalue_original_ex22() {
    receive("EX22");
}
#[test]
fn source_compvalue_original_ex24() {
    receive("EX24");
}
#[test]
fn source_compvalue_original_ex26() {
    receive("EX26");
}
#[test]
fn source_compvalue_original_ex27() {
    receive("EX27");
}
#[test]
fn source_compvalue_original_ex28() {
    receive("EX28");
}
#[test]
fn source_compvalue_original_ex29() {
    receive("EX29");
}
#[test]
fn source_compvalue_original_ex30() {
    receive("EX30");
}
#[test]
fn source_compvalue_original_ex31() {
    receive("EX31");
}
#[test]
fn source_compvalue_original_ex32() {
    receive("EX32");
}
#[test]
fn source_compvalue_set_vs01() {
    protocol("VS01");
}
#[test]
fn source_compvalue_set_vs02() {
    protocol("VS02");
}
#[test]
fn source_compvalue_set_vs03() {
    protocol("VS03");
}
#[test]
fn source_compvalue_set_vs04() {
    protocol("VS04");
}
#[test]
fn source_compvalue_set_vs05() {
    protocol("VS05");
}
#[test]
fn source_compvalue_set_vs06() {
    protocol("VS06");
}
#[test]
fn source_compvalue_set_vs07() {
    protocol("VS07");
}
#[test]
fn source_compvalue_set_vs08() {
    protocol("VS08");
}
