//! Fixed query scopes over one live source Memory owner. A scan owns its
//! current rows; the next recursive scan observes intervening UPDATE writes.
//! No graph projection, sorted rebuild or independent mutable mirror is used.
use super::*;
use std::cell::RefCell;

#[derive(Clone)]
pub(crate) struct SourceGraphView {
    owner: Rc<RefCell<SourceMemoryGraph>>,
    session: Rc<QueryNumericSession>,
    scope: Option<SourceGraphTerm>,
    default_reader: Option<Rc<RefCell<SourceMemoryGraph>>>,
}
impl SourceGraphView {
    pub(crate) fn new(owner: Rc<RefCell<SourceMemoryGraph>>, scope: Option<SourceGraphTerm>) -> Result<Self, SourceGraphError> {
        let session = {
            let graph=owner.borrow();
            graph.default_context()?;
            if let Some(scope)=&scope {graph.check_context(scope)?;}
            graph.session().clone()
        };
        Ok(Self{owner, session, scope, default_reader:None})
    }
    pub(crate) fn with_temporary_default(mut self,reader:Rc<RefCell<SourceMemoryGraph>>)->Result<Self,SourceGraphError> {
        if Rc::ptr_eq(&reader,&self.owner) || !Rc::ptr_eq(reader.borrow().session(),&self.session) {return Err(SourceGraphError::SessionMismatch);}
        self.default_reader=Some(reader);self.scope=None;Ok(self)
    }
    pub(crate) fn session(&self) -> &Rc<QueryNumericSession> {&self.session}
    pub(crate) fn cancellation(&self) -> Option<Arc<AtomicBool>> {self.owner.borrow().cancellation()}
    pub(crate) fn scoped(&self, name: SourceGraphTerm) -> Result<Self,SourceGraphError> {
        // Graph(identifier) retains URIRef/BNode and constructs URIRef(str)
        // for the other Node classes. This does not coerce fact terms.
        let graph=self.owner.borrow();
        let raw=graph.externalize_source(name.clone())?;
        let name=match raw.rdf() {
            Ok(Term::NamedNode(_) | Term::BlankNode(_))=>name,
            Ok(Term::Literal(v))=>graph.admit_source(RawExpressionValue::from_import(oxrdf::NamedNode::new_unchecked(v.value()).into()).with_new_source_object())?,
            Err(_) if raw.source_variable().is_some()=>graph.admit_source(RawExpressionValue::from_import(oxrdf::NamedNode::new_unchecked(raw.source_variable().unwrap().as_str()).into()).with_new_source_object())?,
            Err(e)=>return Err(e.into()),
        };
        graph.check_context(&name)?;
        Ok(Self{owner:self.owner.clone(),session:self.session.clone(),scope:Some(name),default_reader:None})
    }
    pub(crate) fn admit_source(&self, raw:RawExpressionValue)->Result<SourceGraphTerm,SourceGraphError> {self.owner.borrow().admit_source(raw)}
    pub(crate) fn externalize_source(&self, term:SourceGraphTerm)->Result<RawExpressionValue,SourceGraphError> {self.owner.borrow().externalize_source(term)}
    pub(crate) fn source_quads_for_cells<'a>(&self,cells:[Option<SolutionValue<SourceGraphTerm>>;3])->Box<dyn Iterator<Item=Result<InternalQuad<SourceGraphTerm>,SourceGraphError>>+'a> {
        let graph=self.owner.borrow();
        let scanned=if let Some(reader)=&self.default_reader {
            // USING's temporary Graph is an original separate graph. Re-home
            // its transported cells to the dataset owner without inserting
            // them, retaining the same source objects and graph membership.
            let reader=reader.borrow();
            (|| {
                let mut converted=Vec::new();
                for cell in cells {
                    converted.push(match cell {
                        Some(SolutionValue::Rdf(v))=>Some(SolutionValue::Rdf(reader.admit_source(graph.externalize_source(v)?)?)),
                        other=>other,
                    });
                }
                let rows=reader.scan_cells_in(converted.try_into().map_err(|_|SourceGraphError::InvalidTermRole)?,None)?;
                rows.into_iter().map(|row| {
                    let [a,b,c]=row;
                    Ok([graph.admit_source(reader.externalize_source(a)?)?,graph.admit_source(reader.externalize_source(b)?)?,graph.admit_source(reader.externalize_source(c)?)?])
                }).collect::<Result<Vec<_>,SourceGraphError>>()
            })()
        } else {graph.scan_cells_in(cells,self.scope.as_ref())};
        match scanned {
            Ok(rows)=>Box::new(SourceTripleIter{rows:rows.into_iter(),cancellation:graph.cancellation(),terminal:false}),
            Err(e)=>Box::new(std::iter::once(Err(e))),
        }
    }
}
impl<'a> QueryableDataset<'a> for SourceGraphView {
    type InternalTerm=SourceGraphTerm;
    type Error=SourceGraphError;
    fn internal_quads_for_pattern(&self,subject:Option<&SourceGraphTerm>,predicate:Option<&SourceGraphTerm>,object:Option<&SourceGraphTerm>,graph_name:Option<Option<&SourceGraphTerm>>)->impl Iterator<Item=Result<InternalQuad<SourceGraphTerm>,SourceGraphError>>+use<'a> {
        if graph_name==Some(None) {
            self.source_quads_for_cells([subject,predicate,object].map(|v|v.cloned().map(SolutionValue::Rdf)))
        } else {
            // Source GRAPH uses a scoped child compiler; ordinary quad scans
            // cannot acquire that provenance by supplying a graph name.
            Box::new(std::iter::once(Err(SourceGraphError::UnqualifiedGraph)))
        }
    }
    fn internal_named_graphs(&self)->impl Iterator<Item=Result<SourceGraphTerm,SourceGraphError>>+use<'a> {
        let result=(|| {
            let mut graph=self.owner.borrow_mut();
            let default=graph.default_context()?;
            let names=graph.context_names()?;
            // Actual Dataset.contexts observation registers a missing default.
            if !names.contains(&default) {graph.register_context(&default)?;}
            Ok(names.into_iter().filter(|v|v!=&default).map(Ok).collect::<Vec<_>>())
        })();
        match result {Ok(rows)=>rows,Err(e)=>vec![Err(e)]}.into_iter()
    }
    fn contains_internal_graph_name(&self,name:&SourceGraphTerm)->Result<bool,SourceGraphError> {
        let graph=self.owner.borrow();graph.check_context(name)?;
        Ok(graph.context_names()?.contains(name))
    }
    fn internalize_term(&self,term:Term)->Result<SourceGraphTerm,SourceGraphError> {self.admit_source(RawExpressionValue::from_import(term))}
    fn externalize_term(&self,term:SourceGraphTerm)->Result<Term,SourceGraphError> {Ok(self.externalize_source(term)?.rdf()?.clone())}
}
