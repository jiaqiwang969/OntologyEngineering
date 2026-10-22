//! Active frontend admission controls, not original RDFLib execution oracles.
//! Every source expression below comes from the actual parser and planner.
use super::*;
use crate::expression::{
    ExpressionEvaluationError, NumericExpressionHooks, build_numeric_expression_evaluator,
};
use crate::numeric::{
    PreparedSourceLiteralRegistry, QueryContextSemantics, QueryGroupSemantics,
    QueryNumericCapabilities, SourceHashPolicy, SourceHashRuntime,
};
use semantica_numeric::{DecimalContext, NumericLimits};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::convert::Infallible;

const FIXTURE: &str = include_str!("fixtures/source_exists_frontend_gate_controls.json");

fn cases() -> Vec<Value> {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "56aed5a34a0e3f3daebfb360b08f6ebefc9453596abbee83976adf3581da461c"
    );
    let value: Value = serde_json::from_str(FIXTURE).unwrap();
    value["queries"].as_array().unwrap().clone()
}

fn session() -> Rc<QueryNumericSession> {
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            DecimalContext::default(),
            NumericLimits::default(),
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
        .unwrap(),
    )
}

#[derive(Clone, Default)]
struct Backend {
    scans: Rc<Cell<usize>>,
    internalizations: Rc<Cell<usize>>,
}
impl<'a> QueryableDataset<'a> for Backend {
    type InternalTerm = Term;
    type Error = Infallible;

    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.scans.set(self.scans.get() + 1);
        empty()
    }
    fn internalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        self.internalizations.set(self.internalizations.get() + 1);
        Ok(value)
    }
    fn externalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        Ok(value)
    }
}

fn probe_expression(pattern: &GraphPattern) -> &Expression {
    match pattern {
        GraphPattern::SourceContext { inner, .. } | GraphPattern::Project { inner, .. } => {
            probe_expression(inner)
        }
        GraphPattern::Extend {
            variable,
            expression,
            ..
        } if variable.as_str() == "probe" => expression,
        other => panic!("probe not in actual parsed plan: {other:?}"),
    }
}

#[test]
fn source_exists_frontend_gate_actual_expression_compilers_stop_before_child() {
    for case in cases() {
        let session = session();
        let registry = PreparedSourceLiteralRegistry::new(Rc::clone(&session));
        let (query, _) = spargebra::SparqlParser::new()
            .parse_query_rdflib760_source_exists_frontend(
                case["query"].as_str().unwrap(),
                registry.hooks(),
                true,
            )
            .unwrap();
        let spargebra::Query::Select { pattern, .. } = query else {
            panic!("query changed kind")
        };
        let plan = crate::QueryEvaluator::new()
            .without_optimizations()
            .plan_graph_pattern(&pattern);
        let expression = probe_expression(&plan);
        let Expression::SourceExists(source) = expression else {
            panic!("parser did not retain source expression: {expression:?}")
        };
        assert_eq!(
            source.graph.is_some(),
            case["source_overlay"].as_bool().unwrap()
        );
        assert!(
            source.syntax.root_node().is_some(),
            "source syntax is unfinished"
        );

        for compiler in ["legacy", "raw", "numeric"] {
            let backend = Backend::default();
            let evaluator = crate::QueryEvaluator::new()
                .simple_evaluator(backend.clone(), QueryDatasetSpecification::new(), &None)
                .unwrap();
            let mut variables = Vec::new();
            let mut stats = Vec::new();
            let mut context = ExpressionContext {
                evaluator: &evaluator,
                encoded_variables: &mut variables,
                stat_children: &mut stats,
            };
            let result = match compiler {
                "legacy" => build_expression_evaluator(expression, &mut context).map(|_| ()),
                "raw" => build_raw_expression_evaluator(expression, &mut context).map(|_| ()),
                "numeric" => build_numeric_expression_evaluator(
                    expression,
                    &mut context,
                    NumericExpressionHooks {
                        session: Rc::clone(&session),
                        prepared_literals: Some(Rc::clone(&registry)),
                        externalize: Rc::new(|_| panic!("source child result was externalized")),
                        failure: |error| QueryEvaluationError::Unexpected(Box::new(error)),
                    },
                )
                .map(|_| ()),
                _ => unreachable!(),
            };
            assert!(matches!(
                result,
                Err(ExpressionEvaluationError::UnsupportedNullableExpression)
            ));
            assert!(variables.is_empty());
            assert!(stats.is_empty());
            assert_eq!(backend.internalizations.get(), 0);
            assert_eq!(backend.scans.get(), 0);
        }
    }
}

#[test]
fn source_exists_frontend_gate_sealed_execute_refuses_before_iterator() {
    for case in cases() {
        let backend = Backend::default();
        let prepared = numeric_query::prepare_source_frontend_numeric_query(
            case["query"].as_str().unwrap(),
            backend.clone(),
            session(),
            Vec::new(),
        )
        .unwrap();
        let error = match prepared.execute_source_presence() {
            Err(error) => error,
            Ok(_) => panic!("source EXISTS frontend granted execution"),
        };
        assert!(matches!(&error, QueryEvaluationError::Unexpected(_)));
        // The existing public error conversion deliberately maps the private
        // compiler variant to this stable message; do not invent a typed chain.
        assert_eq!(error.to_string(), "unsupported nullable expression");
        assert_eq!(backend.scans.get(), 0);
    }
}
