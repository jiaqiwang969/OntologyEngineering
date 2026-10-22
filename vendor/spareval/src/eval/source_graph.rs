//! Original evalGraph control flow over the existing source context/compiler.
//! Graph scope belongs to each compiled child; no mutable global current graph.
use super::*;

fn invalid()->QueryEvaluationError {QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))}
impl<'a,D:QueryableDataset<'a>> SimpleEvaluator<'a,D> {
    pub(super) fn source_named_graph_evaluator(&self,inner:&GraphPattern,name:&NamedNodePattern,variables:&mut Vec<Variable>)->Result<InternalTupleEvaluator<'a,D::InternalTerm>,QueryEvaluationError> {
        let runtime=self.source_context_runtime()?;
        if runtime.variables.as_ref()!=variables.as_slice() {return Err(invalid());}
        let factory=self.source_scoped_evaluator.clone().ok_or_else(invalid)?;
        let codec=self.dataset.source_literal_codec().cloned().ok_or_else(invalid)?;
        let slot=match name {
            NamedNodePattern::Variable(v)=>Some(runtime.variables.iter().position(|a|a==v).ok_or_else(invalid)?),
            _=>None,
        };
        let constant=match name {
            NamedNodePattern::NamedNode(v)=>Some((codec.internalize)(RawExpressionValue::from_import(v.clone().into()).with_new_source_object())?),
            _=>None,
        };
        let evaluator=self.clone();let inner=inner.clone();let name=name.clone();
        Ok(Rc::new(move |from| {
            runtime.token.ensure_alive()?;
            // QueryContext.clone preserves actual bindings, independently of
            // a FrozenBindings row's local mapping.
            let current=from.source_push(&runtime.initial);
            let bindings=current.source_context_bindings().ok_or_else(invalid)?;
            let graph=constant.clone().or_else(||slot.and_then(|slot|bindings.cell(slot).and_then(SolutionValue::rdf).cloned()));
            let unbound=graph.is_none();
            let names:Box<dyn Iterator<Item=Result<D::InternalTerm,QueryEvaluationError>>+'a>=match graph {
                Some(graph)=>Box::new(std::iter::once(Ok(graph))),
                None=>evaluator.dataset.internal_named_graphs(),
            };
            let mut names=names;let mut rows:Option<InternalTuplesIterator<'a,D::InternalTerm>>=None;
            let mut right:Option<InternalTuple<D::InternalTerm>>=None;
            let evaluator=evaluator.clone();let runtime=runtime.clone();let factory=factory.clone();let inner=inner.clone();let name=name.clone();
            Ok(Box::new(nullable::halt_after_error(std::iter::from_fn(move || {
                (|| {
                    runtime.token.ensure_alive()?;
                    loop {
                        if let Some(child)=&mut rows {
                            if let Some(row)=child.next() {
                                let row=row?;
                                if let Some(right)=&right {
                                    if runtime.compatible(&row,right)? {return Ok(Some(row.source_merge(right)));}
                                } else {return Ok(Some(row));}
                                continue;
                            }
                            rows=None;
                        }
                        let Some(graph)=names.next().transpose()? else {return Ok(None)};
                        let mut child=factory(&evaluator,graph.clone())?;
                        child.source_graph_scope=Some(name.clone());
                        let mut child_variables=runtime.variables.to_vec();
                        let (plan,_)=child.graph_pattern_evaluator(&inner,&mut child_variables);
                        if child_variables.as_slice()!=runtime.variables.as_ref(){return Err(invalid());}
                        let context=current.source_push(&runtime.initial);
                        right=if unbound {
                            let mut row=InternalTuple::with_capacity(runtime.variables.len());
                            row.set_cell(slot.ok_or_else(invalid)?,SolutionValue::Rdf(graph));
                            Some(row.source_with_context_of(&context))
                        } else {None};
                        rows=Some(plan?(context)?);
                    }
                })().transpose()
            }))))
        }))
    }
}
