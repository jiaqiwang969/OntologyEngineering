//! Actual state mutation and readback of the same persistent source owner.
use super::*;
use crate::tuple::SolutionValue;

/// Borrowed observation of an actual retained source object, not a constructor.
pub struct SourceNodeObservation<'a> {
    raw: &'a RawExpressionValue,
    pub term: SourceTermInput<'a>,
    pub hash: i64,
    pub repr: String,
    pub truth: bool,
}
impl SourceNodeObservation<'_> {
    /// Convert the actual retained object through the query result DTO owner.
    /// Conversion remains lazy: metadata-only observers retain prior behavior.
    /// Present witness terms using the same original DTO projection. This does
    /// not establish a source object or bypass runtime_dto's raw provenance.
    pub fn present_source_term(term: SourceTermInput<'_>) -> Result<serde_json::Value, QueryEvaluationError> {
        super::super::source_runtime_result::dto_from_source_term(term)
    }
    pub fn runtime_dto(&self) -> Result<serde_json::Value, QueryEvaluationError> {
        super::super::source_runtime_result::dto(self.raw)
    }
}
pub(super) fn view(raw: &RawExpressionValue) -> Result<SourceTermInput<'_>, QueryEvaluationError> {
    if let Some(v)=raw.source_variable() { return Ok(SourceTermInput::Variable(v.as_str())); }
    Ok(match raw.rdf().map_err(error)? {
        Term::NamedNode(v)=>SourceTermInput::Iri(v.as_str()),
        Term::BlankNode(v)=>SourceTermInput::Blank(v.as_str()),
        Term::Literal(v)=>{
            let LiteralSourceState::Literal(m)=raw.literal_source() else { return Err(error(RawNumericFault::InvalidSourceOrigin)); };
            let dt=match &m.datatype {SourceDatatype::Absent=>None,SourceDatatype::Specified(v)=>Some(v.as_str()),SourceDatatype::Unknown=>return Err(error(RawNumericFault::UnknownSourceLiteralIdentity))};
            let SourceFact::Known(lang)=&m.language_original else {return Err(error(RawNumericFault::UnknownSourceLiteralIdentity));};
            SourceTermInput::Literal{value:v.value(),datatype:dt,language:lang.as_deref()}
        }
    })
}
fn observation<'a>(graph:&SourceMemoryGraph,raw:&'a RawExpressionValue)->Result<SourceNodeObservation<'a>,QueryEvaluationError> {
    let term=view(raw)?;let session=graph.session();
    let truth=match term {SourceTermInput::Literal{..}=>session.source_literal_python_truth(raw).map_err(error)?,_=>!term.cache_parts().1.is_empty()};
    Ok(SourceNodeObservation{raw,term,hash:session.source_hash().ok_or_else(||error(RawNumericFault::InvalidSourceOrigin))?.term_hash(raw).map_err(error)?,repr:session.source_repr(&SolutionValue::Rdf(raw.clone())).map_err(error)?,truth})
}
impl SourceDatasetState {
    /// Execute the exact prepared DATA request; commit the reached source state
    /// even when a later operation fails. Resource refusal remains a separate
    /// declared Rust guard and cannot manufacture an original success.
    pub fn update_data(&mut self,text:&str,namespaces:&BTreeMap<String,String>,max_quads:usize,max_bytes:usize)->Result<(),QueryEvaluationError> {
        let mut graph=self.graph(None)?;
        let prepared=super::super::source_update::prepare(text,namespaces,graph.session().clone()).map_err(error)?;
        let reached=prepared.execute_data(&mut graph);
        self.commit(graph,max_quads,max_bytes)?;
        reached
    }
    /// Enumerate current memberships without rebuilding or reordering the store.
    pub fn visit_quads(&self,mut emit:impl FnMut(SourceQuadInput<'_>)->Result<(),QueryEvaluationError>)->Result<(),QueryEvaluationError> {
        let graph=self.graph(None)?;
        for triple in graph.scan_context(None,[None,None,None]).map_err(error)? {
            let values=triple.each_ref().map(|v|graph.externalize_source(v.clone()));
            let [a,b,c]=values;let values=[a.map_err(error)?,b.map_err(error)?,c.map_err(error)?];
            let terms=[view(&values[0])?,view(&values[1])?,view(&values[2])?];
            for name in graph.triple_contexts(&triple).map_err(error)? {
                let name=graph.externalize_source(name).map_err(error)?;let name=view(&name)?;
                let scope=if matches!(name,SourceTermInput::Iri(DEFAULT)){None}else{Some(name)};
                emit((terms,scope))?;
            }
        }
        Ok(())
    }
    /// Read original term metadata from actual objects. Consumers choose the
    /// presentation; no caller-supplied identifier can create source authority.
    pub fn visit_observations(&mut self,quad:impl FnMut([SourceNodeObservation<'_>;4])->Result<(),QueryEvaluationError>,context:impl FnMut(SourceNodeObservation<'_>)->Result<(),QueryEvaluationError>)->Result<(),QueryEvaluationError> {
        self.visit_observations_with_default_hook(quad,context,||Ok(()))
    }
    /// Dataset.contexts -> graph(default) initializes the owning namespace
    /// manager at this exact point. Failed earlier quad reads must not do so.
    pub fn visit_observations_with_default_hook(&mut self,mut quad:impl FnMut([SourceNodeObservation<'_>;4])->Result<(),QueryEvaluationError>,mut context:impl FnMut(SourceNodeObservation<'_>)->Result<(),QueryEvaluationError>,mut register_default:impl FnMut()->Result<(),QueryEvaluationError>)->Result<(),QueryEvaluationError> {
        let mut graph=self.graph(None)?;
        for triple in graph.scan_context(None,[None,None,None]).map_err(error)? {
            let values=triple.each_ref().map(|v|graph.externalize_source(v.clone()));
            let [a,b,c]=values;let values=[a.map_err(error)?,b.map_err(error)?,c.map_err(error)?];
            for name in graph.triple_contexts(&triple).map_err(error)? {
                let name=graph.externalize_source(name).map_err(error)?;
                quad([observation(&graph,&values[0])?,observation(&graph,&values[1])?,observation(&graph,&values[2])?,observation(&graph,&name)?])?;
            }
        }
        let mut names=graph.context_names().map_err(error)?;let default=graph.default_context().map_err(error)?;
        // Dataset.graphs() yields its registered contexts and ensures the
        // default graph exists. This original readback itself can register it.
        if !names.contains(&default){register_default()?;graph.register_context(&default).map_err(error)?;names.push(default);}
        for name in names {let raw=graph.externalize_source(name).map_err(error)?;context(observation(&graph,&raw)?)?;}
        self.memory=Some(graph.into_state());
        Ok(())
    }
}

#[cfg(test)]
mod receiving {
    use super::*;
    #[test]
    fn source_runtime_data_budget_refusal_preserves_prior_state() {
        let mut state=SourceDatasetState::default();
        state.update_data("INSERT DATA {<urn:s> <urn:p> <urn:old>}",&BTreeMap::new(),100,4096).unwrap();
        let error=state.update_data("INSERT DATA {?x <urn:p> <urn:new>}",&BTreeMap::new(),100,1).unwrap_err();
        assert!(error.to_string().contains("source_persistent_retained_state_budget"));
        assert!(state.matches_projection([([SourceTermInput::Iri("urn:s"),SourceTermInput::Iri("urn:p"),SourceTermInput::Iri("urn:old")],None)]).unwrap());
        assert_eq!(state.query("ASK {<urn:s> <urn:p> <urn:old>}",&BTreeMap::new(),&BTreeMap::new(),None).unwrap()["boolean"],true);
    }
}
