//! Source ORDER on the existing compiled plan. Rows, keys and comparisons are
//! distinct phases; the primitive sorts indices and never reconstructs a row.
use super::source_sort::{SourceSortError, source_sort_indices};
use super::*;
use crate::expression::build_source_order_key_evaluator;
#[cfg(test)]
use crate::numeric::SourceOrderKey;
use semantica_numeric::NumericFault;

#[cfg(test)]
#[derive(Clone, Debug)]
pub(super) enum SourceOrderEvent {
    KeyStart {
        condition_index: usize,
        original_row_index: usize,
    },
    Key {
        condition_index: usize,
        original_row_index: usize,
        key: SourceOrderKey,
    },
    Compare {
        condition_index: usize,
        left_original: usize,
        right_original: usize,
    },
    Compared {
        condition_index: usize,
        left_original: usize,
        right_original: usize,
        less: bool,
    },
}
#[cfg(test)]
pub(super) type SourceOrderObserver<'a> = Rc<dyn Fn(SourceOrderEvent) + 'a>;

fn order_resource(operation: &'static str) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::Core(
        NumericFault::ResourceLimit { operation },
    )))
}

pub(super) fn reserve_order_buffer<T>(
    buffer: &mut Vec<T>,
    additional: usize,
) -> Result<(), QueryEvaluationError> {
    buffer
        .try_reserve(additional)
        .map_err(|_| order_resource("source_order_buffer"))
}

impl<'a, D: QueryableDataset<'a>> SimpleEvaluator<'a, D> {
    pub(super) fn source_order_evaluator(
        &self,
        inner: &GraphPattern,
        expressions: &[OrderExpression],
        variables: &mut Vec<Variable>,
        stats: &mut Vec<Rc<EvalNodeWithStats>>,
    ) -> Result<InternalTupleEvaluator<'a, D::InternalTerm>, QueryEvaluationError> {
        let codec = self
            .dataset
            .source_literal_codec()
            .cloned()
            .ok_or_else(|| {
                QueryEvaluationError::Unexpected(Box::new(
                    RawNumericFault::UnsupportedSourceLiteralOperator(
                        "SourceOrderBy without source session",
                    ),
                ))
            })?;
        let (child, child_stats) = self.graph_pattern_evaluator(inner, variables);
        stats.push(child_stats);
        let child = child?;
        let mut by = Vec::new();
        reserve_order_buffer(&mut by, expressions.len())?;
        for expression in expressions {
            let (expression, descending) = match expression {
                OrderExpression::Asc(expression) => (expression, false),
                OrderExpression::Desc(expression) => (expression, true),
            };
            let mut context = ExpressionContext {
                evaluator: self,
                encoded_variables: variables,
                stat_children: stats,
            };
            let hooks = self.dataset.numeric_hooks().ok_or_else(|| {
                QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
            })?;
            by.push((
                descending,
                build_source_order_key_evaluator(expression, &mut context, hooks)?,
            ));
        }
        let token = self.dataset.cancellation_token.clone();
        #[cfg(test)]
        let observer = self.source_order_observer.clone();
        Ok(Rc::new(move |from| {
            token.ensure_alive()?;
            let mut rows = Vec::new();
            // No key evaluation, even a constant, precedes complete child
            // collection. A late child error therefore discards this prefix.
            for row in child(from)? {
                let row = row?;
                token.ensure_alive()?;
                reserve_order_buffer(&mut rows, 1)?;
                rows.push(row);
            }
            let mut order = Vec::new();
            reserve_order_buffer(&mut order, rows.len())?;
            for index in 0..rows.len() {
                order.push(index);
            }
            let mut keys = Vec::new();
            let mut indices = Vec::new();
            let mut next_order = Vec::new();
            reserve_order_buffer(&mut keys, rows.len())?;
            reserve_order_buffer(&mut indices, rows.len())?;
            reserve_order_buffer(&mut next_order, rows.len())?;
            // RDFLib applies one stable sort per condition, last condition
            // first. Each pass evaluates keys in the previous pass's row order.
            for (_condition_index, (descending, evaluate)) in by.iter().enumerate().rev() {
                keys.clear();
                indices.clear();
                next_order.clear();
                for &original_row_index in &order {
                    token.ensure_alive()?;
                    #[cfg(test)]
                    if let Some(observer) = &observer {
                        observer(SourceOrderEvent::KeyStart {
                            condition_index: _condition_index,
                            original_row_index,
                        });
                    }
                    let key = evaluate(&rows[original_row_index]).map_err(nullable::uncaught)?;
                    #[cfg(test)]
                    if let Some(observer) = &observer {
                        observer(SourceOrderEvent::Key {
                            condition_index: _condition_index,
                            original_row_index,
                            key: key.clone(),
                        });
                    }
                    keys.push(key);
                }
                for index in 0..rows.len() {
                    indices.push(index);
                }
                source_sort_indices(
                    &mut indices,
                    *descending,
                    |left, right| {
                        #[cfg(test)]
                        if let Some(observer) = &observer {
                            observer(SourceOrderEvent::Compare {
                                condition_index: _condition_index,
                                left_original: order[left],
                                right_original: order[right],
                            });
                        }
                        let less = codec
                            .session
                            .source_order_key_less(&keys[left], &keys[right])
                            .map_err(|error| {
                                nullable::uncaught(nullable::numeric_failure(error))
                            })?;
                        #[cfg(test)]
                        if let Some(observer) = &observer {
                            observer(SourceOrderEvent::Compared {
                                condition_index: _condition_index,
                                left_original: order[left],
                                right_original: order[right],
                                less,
                            });
                        }
                        Ok(less)
                    },
                    || token.ensure_alive(),
                )
                .map_err(|error| match error {
                    SourceSortError::Callback(error) => error,
                    SourceSortError::Allocation => order_resource("source_order_sort_allocation"),
                    SourceSortError::IndexOverflow => order_resource("source_order_sort_index"),
                })?;
                for &index in &indices {
                    token.ensure_alive()?;
                    next_order.push(order[index]);
                }
                std::mem::swap(&mut order, &mut next_order);
            }
            // Preserve the actual internal row objects and all source/cache
            // metadata. Final Dataset externalization remains the decoder's job.
            let mut slots = Vec::new();
            reserve_order_buffer(&mut slots, rows.len())?;
            for row in rows {
                slots.push(Some(row));
            }
            let mut result = Vec::new();
            reserve_order_buffer(&mut result, order.len())?;
            for index in order {
                token.ensure_alive()?;
                result.push(slots[index].take().expect("sort indices are a permutation"));
            }
            Ok(Box::new(result.into_iter().map(Ok)))
        }))
    }
}
