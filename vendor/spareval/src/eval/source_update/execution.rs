//! Original UPDATE operations over the same live source owner and compiler.
use super::*;
use crate::dataset::source_memory::{SourceGraphTerm,SourceGraphView,SourceMemoryGraph};
use crate::dataset::nullable_numeric::CachedInternalTerm;
use crate::eval::numeric_query::{bind_source_update_pattern,PreparedSourceRows};
use crate::eval::source_persistent::SourceUpdateLoader;
use crate::numeric::{NumericInitialBinding,NumericInitialValue,RawExpressionValue};
use crate::tuple::InternalTuple;
use oxrdf::{BlankNode,NamedNode,Term,Variable};
use spargebra::{SourceUpdateData,SourceUpdateOperation,SourceUpdateOperationKind as Op};
use spargebra::term::{TermPattern,NamedNodePattern,GraphNamePattern,GraphName,TriplePattern};
use spargebra::algebra::{GraphPattern,GraphTarget,SourceContextAnnotation};
use std::{cell::RefCell,collections::HashMap};

type Carrier=CachedInternalTerm<SourceGraphTerm>;
type Row=InternalTuple<Carrier>;
fn error(e:impl std::error::Error+Send+Sync+'static)->QueryEvaluationError {QueryEvaluationError::Unexpected(Box::new(e))}
fn backend(message:impl Into<String>)->QueryEvaluationError {
    error(crate::eval::source_compvalue::SourceCompValueError{module:"builtins",class:"Exception",argument:message.into()})
}
fn can_suppress(e:&QueryEvaluationError)->bool {
    matches!(e,QueryEvaluationError::Unexpected(e) if e.downcast_ref::<crate::eval::source_compvalue::SourceCompValueError>().is_some())
}
#[derive(Clone)]
pub(in crate::eval) struct PreparedUpdateBindings {
    pub(in crate::eval) initial:Vec<NumericInitialBinding>,
    pub(in crate::eval) values:Vec<Option<RawExpressionValue>>,
}

impl PreparedSourceUpdate {
    pub(in crate::eval) fn execute_all(&self,owner:Rc<RefCell<SourceMemoryGraph>>,bindings:&PreparedUpdateBindings,loader:&mut dyn SourceUpdateLoader)->Result<(),QueryEvaluationError> {
        if !Rc::ptr_eq(owner.borrow().session(),self.registry.session()) {return Err(error(RawNumericFault::InvalidSourceOrigin));}
        if self.request.operations.is_empty() {
            return Err(error(crate::eval::source_compvalue::SourceCompValueError {
                module: "builtins", class: "AttributeError",
                argument: "'list' object has no attribute 'algebra'".into(),
            }));
        }
        let mut execution=Execution{prepared:self,owner,bindings,loader};
        for operation in &self.request.operations {
            let result=execution.operation(operation);
            let silent=match &operation.kind {
                Op::Load{silent,..}|Op::Clear{silent,..}|Op::Drop{silent,..}|Op::Create{silent,..}|Op::Add{silent,..}|Op::Move{silent,..}|Op::Copy{silent,..}=>*silent,
                _=>false,
            };
            if let Err(e)=result {if !(silent && can_suppress(&e)){return Err(e);}}
        }
        Ok(())
    }
}
struct Execution<'a> {
    prepared:&'a PreparedSourceUpdate,
    owner:Rc<RefCell<SourceMemoryGraph>>,
    bindings:&'a PreparedUpdateBindings,
    loader:&'a mut dyn SourceUpdateLoader,
}
impl Execution<'_> {
    fn fresh_blank(&self)->Result<BlankNode,QueryEvaluationError> {
        match &self.prepared.update_blank_constructor {
            Some(construct)=>construct().map_err(QueryEvaluationError::Unexpected),
            None=>Ok(spargebra::source_default_blank_node()),
        }
    }
    fn term(&self,value:&TermPattern)->Result<RawExpressionValue,QueryEvaluationError> {
        Ok(match value {
            TermPattern::NamedNode(v)=>RawExpressionValue::from_import(v.clone().into()).with_new_source_object(),
            TermPattern::BlankNode(v)=>RawExpressionValue::from_import(v.clone().into()).with_new_source_object(),
            TermPattern::Variable(v)=>self.prepared.registry.session().construct_initial(NumericInitialValue::SourceVariable(v.clone())).map_err(error)?,
            TermPattern::SourceLiteral(v)=>self.prepared.registry.resolve(v).map_err(error)?,
            _=>return Err(error(RawNumericFault::InvalidSourceOrigin)),
        })
    }
    fn named(&self,name:&NamedNode)->Result<SourceGraphTerm,QueryEvaluationError> {
        self.owner.borrow().admit_source(RawExpressionValue::from_import(name.clone().into()).with_new_source_object()).map_err(error)
    }
    fn graph_identifier(&self,value:Option<RawExpressionValue>)->Result<SourceGraphTerm,QueryEvaluationError> {
        let raw=match value {
            None=>RawExpressionValue::from_import(self.fresh_blank()?.into()).with_new_source_object(),
            Some(v)=>match v.rdf() {
                Ok(Term::NamedNode(_) | Term::BlankNode(_))=>v,
                Ok(Term::Literal(l))=>RawExpressionValue::from_import(NamedNode::new_unchecked(l.value()).into()).with_new_source_object(),
                Err(_) if v.source_variable().is_some()=>RawExpressionValue::from_import(NamedNode::new_unchecked(v.source_variable().unwrap().as_str()).into()).with_new_source_object(),
                Err(e)=>return Err(error(e)),
            },
        };
        self.owner.borrow().admit_source(raw).map_err(error)
    }
    fn graph_pattern(&self,name:&GraphNamePattern,row:Option<(&Row,&PreparedSourceRows<'_,Carrier>)>)->Result<SourceGraphTerm,QueryEvaluationError> {
        match name {
            GraphNamePattern::DefaultGraph=>self.owner.borrow().default_context().map_err(error),
            GraphNamePattern::NamedNode(v)=>self.named(v),
            GraphNamePattern::Variable(v)=>{
                let raw=if let Some((row,stream))=row {lookup(v,row,stream)?} else {Some(self.term(&TermPattern::Variable(v.clone()))?)};
                self.graph_identifier(raw)
            }
        }
    }
    fn scope(&self,name:&GraphName)->Result<Option<SourceGraphTerm>,QueryEvaluationError> {
        match name {
            GraphName::DefaultGraph=>Ok(None),
            GraphName::NamedNode(v)=>self.named(v).map(Some),
        }
    }
    fn data(&mut self,data:&SourceUpdateData,delete:bool)->Result<(),QueryEvaluationError> {
        self.data_triples(&data.triples,None,delete)?;
        for (name,triples) in &data.quads {let scope=self.graph_pattern(name,None)?;self.data_triples(triples,Some(scope),delete)?;}
        Ok(())
    }
    fn data_triples(&self,triples:&[TriplePattern],scope:Option<SourceGraphTerm>,delete:bool)->Result<(),QueryEvaluationError> {
        let mut graph=self.owner.borrow_mut();
        let target=scope.clone().unwrap_or(graph.default_context().map_err(error)?);
        for triple in triples {
            let p=predicate(&triple.predicate);
            let row=[graph.admit_source(self.term(&triple.subject)?).map_err(error)?,graph.admit_source(self.term(&p)?).map_err(error)?,graph.admit_source(self.term(&triple.object)?).map_err(error)?];
            if delete {graph.remove_context(scope.as_ref(),row.map(Some)).map_err(error)?;}
            else {graph.insert_context(&target,row).map_err(error)?;}
        }
        Ok(())
    }
    fn target_scopes(&self,target:&GraphTarget)->Result<Vec<Option<SourceGraphTerm>>,QueryEvaluationError> {
        match target {
            GraphTarget::DefaultGraph=>Ok(vec![None]),
            GraphTarget::NamedNode(v)=>Ok(vec![Some(self.named(v)?)]),
            GraphTarget::NamedGraphs|GraphTarget::AllGraphs=>{
                let mut graph=self.owner.borrow_mut();let default=graph.default_context().map_err(error)?;
                let mut names=graph.context_names().map_err(error)?;
                if !names.contains(&default){graph.register_context(&default).map_err(error)?;names.push(default.clone());}
                Ok(names.into_iter().filter(|name|!matches!(target,GraphTarget::NamedGraphs)||name!=&default).map(Some).collect())
            }
        }
    }
    fn clear(&mut self,target:&GraphTarget,drop:bool)->Result<(),QueryEvaluationError> {
        for scope in self.target_scopes(target)? {
            let mut graph=self.owner.borrow_mut();
            if drop {let name=scope.unwrap_or(graph.default_context().map_err(error)?);graph.drop_context(&name).map_err(error)?;}
            else {graph.remove_context(scope.as_ref(),[None,None,None]).map_err(error)?;}
        }
        Ok(())
    }
    fn transfer(&mut self,from:&GraphName,to:&GraphName,clear:bool,remove:bool)->Result<(),QueryEvaluationError> {
        let src=self.scope(from)?;let dst=self.scope(to)?;
        let mut graph=self.owner.borrow_mut();let default=graph.default_context().map_err(error)?;
        let src_id=src.as_ref().unwrap_or(&default);let dst_id=dst.as_ref().unwrap_or(&default);
        if src_id==dst_id{return Ok(());}
        if clear {graph.remove_context(dst.as_ref(),[None,None,None]).map_err(error)?;}
        // COPY/MOVE clear destination before evaluating the source iterator.
        for row in graph.scan_context(src.as_ref(),[None,None,None]).map_err(error)? {graph.insert_context(dst_id,row).map_err(error)?;}
        if remove {graph.drop_context(src_id).map_err(error)?;}
        Ok(())
    }
    fn load(&mut self,iri:&NamedNode,target:Rc<RefCell<SourceMemoryGraph>>,scope:Option<SourceGraphTerm>)->Result<(),QueryEvaluationError> {
        let session=self.prepared.registry.session().clone();
        let result=self.loader.load(iri.as_str(),&mut |triple| {
            let mut graph=target.borrow_mut();let scope=scope.clone().unwrap_or(graph.default_context().map_err(error)?);
            let mut row=Vec::new();
            for input in triple {let raw=session.construct_source_graph_initial(input.initial(true)?).map_err(error)?;row.push(graph.admit_source(raw).map_err(error)?);}
            graph.insert_context(&scope,row.try_into().map_err(|_|error(RawNumericFault::InvalidSourceOrigin))?).map_err(error)
        })?;
        if result {Ok(())} else {Err(backend(format!("Could not load {} as either RDF/XML, N3 or NTriples",iri.as_str())))}
    }
    fn prepared_rows(&self,pattern:GraphPattern,base:Option<oxiri::Iri<String>>,view:SourceGraphView)->Result<crate::eval::numeric_query::PreparedNumericQuery<'static,SourceGraphView>,QueryEvaluationError> {
        bind_source_update_pattern(pattern,base,view,self.prepared.registry.clone(),self.bindings.initial.clone(),self.bindings.values.clone()).map_err(error)
    }
    fn apply_template(&self,triples:&[TriplePattern],scope:Option<SourceGraphTerm>,row:&Row,stream:&PreparedSourceRows<'_,Carrier>,delete:bool)->Result<(),QueryEvaluationError> {
        let mut blanks:HashMap<BlankNode,RawExpressionValue>=HashMap::new();
        let target=scope.clone().unwrap_or(self.owner.borrow().default_context().map_err(error)?);
        for triple in triples {
            let p=predicate(&triple.predicate);let mut output=Vec::new();
            for term in [&triple.subject,&p,&triple.object] {
                let value=match term {
                    TermPattern::Variable(v)=>lookup(v,row,stream)?,
                    TermPattern::BlankNode(v)=>Some(match blanks.entry(v.clone()) {
                        std::collections::hash_map::Entry::Occupied(entry)=>entry.get().clone(),
                        std::collections::hash_map::Entry::Vacant(entry)=>entry.insert(
                            RawExpressionValue::from_import(self.fresh_blank()?.into()).with_new_source_object()
                        ).clone(),
                    }),
                    other=>Some(self.term(other)?),
                };
                if let Some(value)=value {output.push(value);} else {break;}
            }
            if output.len()!=3{continue;}
            let mut graph=self.owner.borrow_mut();
            let output=output.into_iter().map(|v|graph.admit_source(v).map_err(error)).collect::<Result<Vec<_>,_>>()?;
            let triple:[SourceGraphTerm;3]=output.try_into().map_err(|_|error(RawNumericFault::InvalidSourceOrigin))?;
            if delete {graph.remove_context(scope.as_ref(),triple.map(Some)).map_err(error)?;} else {graph.insert_context(&target,triple).map_err(error)?;}
        }
        Ok(())
    }
    fn delete_where(&mut self,data:&SourceUpdateData,base:Option<oxiri::Iri<String>>)->Result<(),QueryEvaluationError> {
        // This aggregate BGP allocates the one binding domain only. Actual
        // execution below uses each original DATA block, in its original order.
        let patterns=data.triples.iter().chain(data.quads.iter().flat_map(|(_,v)|v)).cloned().collect::<Vec<_>>();
        let pattern=GraphPattern::SourceContext{inner:Box::new(GraphPattern::Bgp{patterns:patterns.clone()}),annotation:SourceContextAnnotation::Bgp{patterns}};
        let view=SourceGraphView::new(self.owner.clone(),None).map_err(error)?;
        let prepared=self.prepared_rows(pattern,base,view)?;
        let mut parts=vec![(None,data.triples.clone())];
        for (name,triples) in &data.quads {
            let term=self.graph_pattern(name,None)?;let raw=self.owner.borrow().externalize_source(term).map_err(error)?;
            parts.push((Some(prepared.source_update_term(raw)?),triples.clone()));
        }
        let mut stream=prepared.execute_source_delete_where_rows(parts)?;
        let mut count=0;
        while let Some(row)=stream.rows.next() {
            count+=1;if count>100_000{return Err(QueryEvaluationError::Unexpected("source_update_solution_budget".into()));}
            let row=row?;self.apply_template(&data.triples,None,&row,&stream,true)?;
            for (name,triples) in &data.quads {let scope=self.graph_pattern(name,Some((&row,&stream)))?;self.apply_template(triples,Some(scope),&row,&stream,true)?;}
        }
        Ok(())
    }
    fn modify(&mut self,delete:&Option<SourceUpdateData>,insert:&Option<SourceUpdateData>,with:&Option<NamedNode>,using:&[(Option<NamedNode>,Option<NamedNode>)],pattern:&GraphPattern,base:Option<oxiri::Iri<String>>)->Result<(),QueryEvaluationError> {
        let mut view=SourceGraphView::new(self.owner.clone(),None).map_err(error)?;
        let mut temporary=None;
        for (default,named) in using {
            if let Some(iri)=default {
                if temporary.is_none(){temporary=Some(Rc::new(RefCell::new(SourceMemoryGraph::new_dataset(self.prepared.registry.session().clone(),None,NamedNode::new_unchecked("urn:x-rdflib:default").into()).map_err(error)?)));}
                self.load(iri,temporary.as_ref().unwrap().clone(),None)?;
            } else if let Some(iri)=named {let scope=self.named(iri)?;self.load(iri,self.owner.clone(),Some(scope))?;}
        }
        if let Some(temp)=temporary {view=view.with_temporary_default(temp).map_err(error)?;}
        else if using.is_empty() {if let Some(iri)=with {view=view.scoped(self.named(iri)?).map_err(error)?;}}
        let mut stream=self.prepared_rows(pattern.clone(),base,view)?.execute_source_update_rows()?;
        let mut rows=Vec::new();
        for row in stream.rows.by_ref() {if rows.len()>=100_000{return Err(QueryEvaluationError::Unexpected("source_update_solution_budget".into()));}rows.push(row?);}
        let target=Some(match with {Some(iri)=>self.named(iri)?,None=>self.owner.borrow().default_context().map_err(error)?});
        for row in rows {
            for (data,remove) in [(delete,true),(insert,false)] {
                if let Some(data)=data {
                    self.apply_template(&data.triples,target.clone(),&row,&stream,remove)?;
                    for (name,triples) in &data.quads {let scope=self.graph_pattern(name,Some((&row,&stream)))?;self.apply_template(triples,Some(scope),&row,&stream,remove)?;}
                }
            }
        }
        Ok(())
    }
    fn operation(&mut self,operation:&SourceUpdateOperation)->Result<(),QueryEvaluationError> {
        match &operation.kind {
            Op::InsertData(data)=>self.data(data,false),Op::DeleteData(data)=>self.data(data,true),
            Op::DeleteWhere(data)=>self.delete_where(data,operation.base_iri.clone()),
            Op::Modify{delete,insert,with,using,pattern}=>self.modify(delete,insert,with,using,pattern,operation.base_iri.clone()),
            Op::Clear{graph,..}=>self.clear(graph,false),Op::Drop{graph,..}=>self.clear(graph,true),
            Op::Create{graph,..}=>{let scope=self.named(graph)?;if self.owner.borrow().len_context(Some(&scope)).map_err(error)?>0{Err(backend(format!("Graph {} already exists.",graph.as_str())))}else{Err(backend("Create not implemented!"))}},
            Op::Add{from,to,..}=>self.transfer(from,to,false,false),Op::Copy{from,to,..}=>self.transfer(from,to,true,false),Op::Move{from,to,..}=>self.transfer(from,to,true,true),
            Op::Load{source,destination,..}=>{let scope=destination.as_ref().map(|v|self.named(v)).transpose()?;self.load(source,self.owner.clone(),scope)},
        }
    }
}
fn predicate(value:&NamedNodePattern)->TermPattern {match value {NamedNodePattern::NamedNode(v)=>TermPattern::NamedNode(v.clone()),NamedNodePattern::Variable(v)=>TermPattern::Variable(v.clone())}}
fn lookup(variable:&Variable,row:&Row,stream:&PreparedSourceRows<'_,Carrier>)->Result<Option<RawExpressionValue>,QueryEvaluationError> {
    let local=stream.variables.iter().position(|v|v==variable).and_then(|slot|row.cell(slot));
    match local.or_else(||stream.initial.get(variable)) {
        Some(crate::tuple::SolutionValue::Rdf(value))=>(stream.codec.externalize)(value.clone()).map(Some),
        None|Some(crate::tuple::SolutionValue::ExplicitNone)=>Ok(None),
        Some(crate::tuple::SolutionValue::ReturnedError(_))=>Err(QueryEvaluationError::UnsupportedSourceRuntimeTerm("SPARQLError")),
    }
}
