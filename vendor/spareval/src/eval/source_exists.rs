use super::*;
use crate::expression::SourceExistsEvaluator;
use crate::numeric::PreparedSourceLiteralRegistry;
use spargebra::SourceExistsExpression;

impl<'a, D: QueryableDataset<'a>> ExpressionContext<'_, SimpleEvaluator<'a, D>> {
    pub(super) fn source_exists_evaluator(
        &mut self,
        source: &SourceExistsExpression<GraphPattern>,
        session: &Rc<QueryNumericSession>,
        registry: &Rc<PreparedSourceLiteralRegistry>,
    ) -> Result<
        Option<SourceExistsEvaluator<'a, InternalTuple<D::InternalTerm>, QueryEvaluationError>>,
        QueryEvaluationError,
    > {
        let Some(runtime) = self.evaluator.source_context.as_ref() else {
            return Ok(None);
        };
        let Some(codec) = self.evaluator.dataset.numeric.as_ref() else {
            return Ok(None);
        };
        if !Rc::ptr_eq(&codec.session, session)
            || !codec
                .prepared_literals
                .as_ref()
                .is_some_and(|actual| Rc::ptr_eq(actual, registry))
            || !registry.admits_exists_execution(session)
        {
            return Ok(None);
        }
        let runtime = Rc::clone(runtime);
        let node = runtime.allocate_node();
        let child = if let Some(graph) = &source.graph {
            let (child, stats) = self
                .evaluator
                .graph_pattern_evaluator(graph, self.encoded_variables);
            self.stat_children.push(stats);
            Some(child?)
        } else {
            None
        };
        let syntax = source.syntax.clone();
        let session = Rc::clone(session);
        let registry = Rc::clone(registry);
        Ok(Some(Rc::new(move |tuple| {
            runtime.token.ensure_alive().map_err(EvalFailure::Fatal)?;
            let current = tuple.source_thaw_owned().ok_or_else(|| {
                EvalFailure::Fatal(QueryEvaluationError::Unexpected(Box::new(
                    RawNumericFault::InvalidSourceOrigin,
                )))
            })?;
            let Some(child) = &child else {
                return Err(EvalFailure::Fatal(QueryEvaluationError::Unexpected(
                    Box::new(
                        source_compvalue::source_compvalue_error(
                            &syntax,
                            &session,
                            &registry,
                            &mut || runtime.token.ensure_alive(),
                        )
                        .map_err(EvalFailure::Fatal)?,
                    ),
                )));
            };
            source_context_join::source_exists_first_row(&runtime, node, child, current)
                .map_err(nullable::source_exists_failure)
        })))
    }
}
