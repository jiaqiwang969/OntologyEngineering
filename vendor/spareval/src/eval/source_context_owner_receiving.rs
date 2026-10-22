//! Actual sealed-query owner receiving. Independent source expectations were
//! frozen before this adapter; production and this adapter share an author.
//! No replacement AST, query engine, backend conversion or guessed owner token.
use super::numeric_query::prepare_numeric_query;
use super::source_context_join::{SourceChildSide, SourceContextEvent};
use super::*;
use crate::dataset::nullable_numeric::CachedInternalTerm;
use crate::numeric::{
    NumericInitialBinding, NumericInitialValue, QueryContextSemantics, QueryGroupSemantics,
    QueryLiteralSemantics, QueryNumericCapabilities, SourceHashPolicy, SourceHashRuntime,
};
use semantica_numeric::{DecimalContext, DecimalSignals, NumericLimits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;

type Row = InternalTuple<CachedInternalTerm<Term>>;
type Event = SourceContextEvent<CachedInternalTerm<Term>>;
const FIXTURE: &str = include_str!("fixtures/source_context_owner_query_cases.json");

fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "f865bd7423b742c198fa90fa75419b96ee8f068c56058e5799fbae172a536d22"
    );
    let f: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        f["source_record_sha256"],
        "cd748af00421d75621c9972262397fc1439fea644c6bfec99ebaf537ca5f7670"
    );
    f
}
fn case(id: &str) -> Value {
    let f = fixture();
    ["source_query_cases", "active_contract_cases"]
        .into_iter()
        .flat_map(|field| f[field].as_array().unwrap())
        .find(|c| c["id"] == id)
        .unwrap()
        .clone()
}
fn session() -> Rc<QueryNumericSession> {
    Rc::new(
        QueryNumericSession::new_with_context_semantics(
            DecimalContext::default(),
            NumericLimits {
                max_input_characters: 4096,
                max_coefficient_digits: 4096,
                max_output_characters: 4096,
                max_materialized_power_of_ten: 4096,
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
        .unwrap(),
    )
}
fn initial(c: &Value) -> Vec<NumericInitialBinding> {
    c["input"]["initial_bindings"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| {
            assert_eq!(value["kind"], "NamedNode");
            NumericInitialBinding {
                variable: Variable::new(name).unwrap(),
                value: NumericInitialValue::UnknownNormalized(
                    NamedNode::new(value["iri"].as_str().unwrap())
                        .unwrap()
                        .into(),
                ),
            }
        })
        .collect()
}
fn term(value: &Term) -> Value {
    match value {
        Term::NamedNode(value) => json!({"kind":"NamedNode","iri":value.as_str()}),
        Term::Literal(value) => {
            json!({"kind":"Literal","lexical":value.value(),"datatype":value.datatype().as_str()})
        }
        other => panic!("unfrozen term {other:?}"),
    }
}
fn bindings(row: &Row, variables: &[Variable]) -> Value {
    Value::Object(
        row.source_local_order()
            .iter()
            .map(|&slot| {
                let SolutionValue::Rdf(value) = row.cell(slot).unwrap() else {
                    panic!("owner whole-query fixture expects RDF or absent cells")
                };
                (variables[slot].as_str().to_owned(), term(value.rdf()))
            })
            .collect(),
    )
}

#[derive(Debug)]
struct FailureIdentity;
#[derive(Debug)]
struct ConversionFailure(Arc<FailureIdentity>);
impl fmt::Display for ConversionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("owner receiving Dataset error")
    }
}
impl std::error::Error for ConversionFailure {}
#[derive(Default)]
struct BackendTrace {
    internal: Cell<usize>,
    external: Cell<usize>,
    scans: Cell<usize>,
    armed: Cell<bool>,
    failed: Cell<usize>,
}
impl BackendTrace {
    fn counts(&self) -> (usize, usize, usize) {
        (self.internal.get(), self.external.get(), self.scans.get())
    }
}
#[derive(Clone)]
struct Backend {
    trace: Rc<BackendTrace>,
    identity: Arc<FailureIdentity>,
}
impl Default for Backend {
    fn default() -> Self {
        Self {
            trace: Rc::default(),
            identity: Arc::new(FailureIdentity),
        }
    }
}
impl<'a> QueryableDataset<'a> for Backend {
    type InternalTerm = Term;
    type Error = ConversionFailure;
    fn internal_quads_for_pattern(
        &self,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<&Term>,
        _: Option<Option<&Term>>,
    ) -> impl Iterator<Item = Result<InternalQuad<Term>, Self::Error>> + use<'a> {
        self.trace.scans.set(self.trace.scans.get() + 1);
        empty()
    }
    fn internalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        self.trace.internal.set(self.trace.internal.get() + 1);
        Ok(value)
    }
    fn externalize_term(&self, value: Term) -> Result<Term, Self::Error> {
        self.trace.external.set(self.trace.external.get() + 1);
        if self.trace.armed.get() {
            self.trace.failed.set(self.trace.failed.get() + 1);
            return Err(ConversionFailure(Arc::clone(&self.identity)));
        }
        Ok(value)
    }
}

fn receive(id: &str) {
    let c = case(id);
    let expected = &c["expected"];
    let numeric = session();
    let backend = Backend::default();
    let prepared = prepare_numeric_query(
        c["input"]["query"].as_str().unwrap(),
        backend.clone(),
        Rc::clone(&numeric),
        initial(&c),
    )
    .unwrap();
    let runtime = prepared.evaluator().source_context.as_ref().unwrap();
    let variables = Rc::clone(&runtime.variables);
    let original_initial = Rc::clone(&runtime.initial);
    if let Some(lazy) = expected["required_join_lazy"].as_bool() {
        use spargebra::algebra::{SourceContextAnnotation, SourceJoinMode};
        let GraphPattern::Project { inner, .. } = prepared.plan() else {
            panic!("projection")
        };
        let GraphPattern::SourceContext {
            annotation: SourceContextAnnotation::Join { mode },
            ..
        } = inner.as_ref()
        else {
            panic!("actual annotated join")
        };
        assert_eq!(*mode == SourceJoinMode::Lazy, lazy);
    }
    let events: Rc<RefCell<Vec<Event>>> = Rc::default();
    let sink = Rc::clone(&events);
    let prepared =
        prepared.with_source_context_observer(Rc::new(move |e| sink.borrow_mut().push(e)));
    let decoded = prepared
        .execute_source_presence()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let actual: Vec<Value> = decoded
        .iter()
        .map(|row| {
            Value::Object(
                row.variables
                    .iter()
                    .zip(&row.values)
                    .filter_map(|(variable, cell)| {
                        cell.as_ref().map(|cell| {
                            let SolutionValue::Rdf(value) = cell else {
                                panic!("unexpected cell")
                            };
                            (variable.as_str().to_owned(), term(value.rdf().unwrap()))
                        })
                    })
                    .collect(),
            )
        })
        .collect();
    assert_eq!(json!(actual), expected["rows"]);
    let events = events.borrow();
    let roots: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::QueryContextStart { current } => Some(current),
            _ => None,
        })
        .collect();
    assert_eq!(roots.len(), 1);
    let root = roots[0];
    let output: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::OutputRow { row, .. } => Some(row),
            _ => None,
        })
        .collect();
    assert_eq!(output.len(), actual.len());
    for (index, row) in output.iter().enumerate() {
        assert!(row.source_has_context_owner());
        assert!(row.source_context_initial_shared(root));
        assert_eq!(bindings(row, &variables), expected["rows"][index]);
        assert_eq!(
            bindings(&row.source_context_bindings().unwrap(), &variables),
            expected["output_owner_bindings"][index]
        );
        if expected["output_owner_is_root"] == true {
            assert!(row.source_context_owner_same(root));
        }
        if expected["output_owner_is_any_child_values_owner"] == false {
            let children: Vec<_> = events
                .iter()
                .filter_map(|e| match e {
                    Event::ValuesSolution { row } => Some(row),
                    _ => None,
                })
                .collect();
            assert_eq!(children.len(), 2, "observe the real Group input producers");
            assert!(
                children
                    .iter()
                    .all(|child| !row.source_context_owner_same(child))
            );
        }
        if let Some(role) = expected["output_owner_role"].as_str() {
            let wanted = if role == "right_values_owner" {
                SourceChildSide::Right
            } else {
                assert_eq!(role, "left_values_owner");
                SourceChildSide::Left
            };
            assert!(events.iter().any(|e| matches!(e,
                Event::ChildRow { side, row: child, .. }
                if *side == wanted && row.source_context_owner_same(child))));
        }
    }
    if expected["output_owners_pairwise_distinct"] == true {
        for (index, row) in output.iter().enumerate() {
            assert!(!row.source_context_owner_same(root));
            for other in output.iter().skip(index + 1) {
                assert!(!row.source_context_owner_same(other));
            }
        }
    }
    assert_eq!(
        bindings(&original_initial, &variables),
        expected["original_initial_unchanged"]
    );
    assert_eq!(
        bindings(&root.source_context_bindings().unwrap(), &variables),
        expected["original_initial_unchanged"]
    );
    assert_eq!(numeric.flags(), DecimalSignals::EMPTY);
    assert_eq!(backend.trace.scans.get(), 0);
}

fn receive_active(id: &str) {
    let c = case(id);
    let backend = Backend::default();
    let prepared = prepare_numeric_query(
        c["input"]["query"].as_str().unwrap(),
        backend.clone(),
        session(),
        initial(&c),
    )
    .unwrap();
    let initial = Rc::clone(
        &prepared
            .evaluator()
            .source_context
            .as_ref()
            .unwrap()
            .initial,
    );
    let token = prepared.evaluator().dataset.cancellation_token.clone();
    let root: Rc<RefCell<Option<Row>>> = Rc::default();
    let observed = Rc::new(Cell::new(0));
    let boundary_counts = Rc::new(Cell::new(None));
    let (sink, counter, counts, trace) = (
        Rc::clone(&root),
        Rc::clone(&observed),
        Rc::clone(&boundary_counts),
        Rc::clone(&backend.trace),
    );
    let cancel = id == "COQA01";
    let prepared = prepared.with_source_context_observer(Rc::new(move |event| match event {
        Event::QueryContextStart { current } => {
            assert!(sink.borrow().is_none());
            *sink.borrow_mut() = Some(current);
        }
        Event::OutputRow { row, .. } => {
            assert_eq!(
                counter.replace(counter.get() + 1),
                0,
                "first actual output boundary only"
            );
            assert!(row.source_has_context_owner());
            let before = trace.counts();
            if cancel {
                token.cancel();
            } else {
                let projected = row.source_project(&[]);
                let forgotten = row.source_forget(sink.borrow().as_ref().unwrap(), &initial, &[]);
                let thawed = projected.source_thaw_owned().unwrap();
                assert!(projected.source_context_owner_same(&row));
                assert!(forgotten.source_context_owner_same(&row));
                assert!(!thawed.source_context_owner_same(&row));
                assert!(thawed.source_context_initial_shared(&row));
                assert_eq!(
                    trace.counts(),
                    before,
                    "owner operations do not call the Dataset"
                );
                assert!(!trace.armed.replace(true));
            }
            counts.set(Some(before));
        }
        _ => (),
    }));
    let mut rows = prepared.execute_source_presence().unwrap();
    let error = match rows.next().unwrap() {
        Err(error) => error,
        Ok(_) => panic!("active boundary must return the original failure"),
    };
    assert_eq!(observed.get(), 1);
    let at_boundary = boundary_counts.get().unwrap();
    if cancel {
        assert!(matches!(error, QueryEvaluationError::Cancelled));
        assert_eq!(backend.trace.counts(), at_boundary);
    } else {
        let QueryEvaluationError::Dataset(error) = error else {
            panic!("must preserve Dataset error")
        };
        assert!(Arc::ptr_eq(
            &error.downcast_ref::<ConversionFailure>().unwrap().0,
            &backend.identity
        ));
        assert_eq!(backend.trace.failed.get(), 1);
        assert_eq!(
            backend.trace.counts(),
            (at_boundary.0, at_boundary.1 + 1, at_boundary.2)
        );
    }
    let after = backend.trace.counts();
    assert!(rows.next().is_none());
    assert!(rows.next().is_none());
    assert_eq!(backend.trace.counts(), after);
    assert_eq!(observed.get(), 1);
}

#[test]
fn source_context_owner_query_fixture_identity() {
    let f = fixture();
    assert_eq!(f["source_query_cases"].as_array().unwrap().len(), 7);
    assert_eq!(f["active_contract_cases"].as_array().unwrap().len(), 2);
    assert!(!FIXTURE.contains("/Users/"));
}
#[test]
fn source_context_owner_query_coq01() {
    receive("COQ01");
}
#[test]
fn source_context_owner_query_coq02() {
    receive("COQ02");
}
#[test]
fn source_context_owner_query_coq03() {
    receive("COQ03");
}
#[test]
fn source_context_owner_query_coq04() {
    receive("COQ04");
}
#[test]
fn source_context_owner_query_coq05() {
    receive("COQ05");
}
#[test]
fn source_context_owner_query_coq06() {
    receive("COQ06");
}
#[test]
fn source_context_owner_query_coq07() {
    receive("COQ07");
}
#[test]
fn source_context_owner_query_coqa01() {
    receive_active("COQA01");
}
#[test]
fn source_context_owner_query_coqa02() {
    receive_active("COQA02");
}
