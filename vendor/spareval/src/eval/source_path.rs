//! Native source path operators, compiled once over the sealed graph's scan
//! and codec. Execution retains closures and iterator state, not a syntax
//! interpreter or a second graph. Path objects remain non-RDF operands.
use super::source_bgp::SourceBgpScan;
use super::source_compvalue::{SourceCompValueError, render_mapping};
use super::*;
use crate::numeric::{PreparedSourceLiteralRegistry, SourceTermKey};
use crate::tuple::ErrorValue;
use spargebra::{SourcePathDefinition, SourcePathNegatedArgument, SourcePathRepeat};
use std::collections::HashSet;

type Cell<T> = SolutionValue<T>;
type Endpoints<T> = [Option<Cell<T>>; 2];
type Pair<T> = [Cell<T>; 2];
type Rows<'a, T> = Box<dyn Iterator<Item = Result<Pair<T>, QueryEvaluationError>> + 'a>;
type Start<'a, T> = Rc<dyn Fn(Endpoints<T>) -> Result<Rows<'a, T>, QueryEvaluationError> + 'a>;

pub(crate) fn graph_cancellation(value: Option<Arc<AtomicBool>>) -> CancellationToken {
    CancellationToken {
        value: value.unwrap_or_else(|| Arc::new(AtomicBool::new(false))),
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
enum Key {
    Rdf(SourceTermKey),
    None,
    Error(ErrorValue),
}

pub(crate) struct SourcePathRuntime<'a, T> {
    pub(crate) scan: SourceBgpScan<'a, T>,
    pub(crate) read: Rc<dyn Fn(T) -> Result<RawExpressionValue, QueryEvaluationError> + 'a>,
    pub(crate) write: Rc<dyn Fn(RawExpressionValue) -> Result<T, QueryEvaluationError> + 'a>,
    pub(crate) session: Rc<QueryNumericSession>,
    pub(crate) registry: Rc<PreparedSourceLiteralRegistry>,
    pub(crate) token: CancellationToken,
}
fn invalid() -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
}
fn resource(operation: &'static str) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::Core(
        semantica_numeric::NumericFault::ResourceLimit { operation },
    )))
}
fn insert<K: Eq + Hash>(set: &mut HashSet<K>, key: K) -> Result<bool, QueryEvaluationError> {
    set.try_reserve(1)
        .map_err(|_| resource("source_path_membership"))?;
    Ok(set.insert(key))
}
impl<T: Clone> SourcePathRuntime<'_, T> {
    fn key(&self, cell: &Cell<T>) -> Result<Key, QueryEvaluationError> {
        self.token.ensure_alive()?;
        Ok(match cell {
            Cell::Rdf(value) => Key::Rdf(
                crate::numeric::source_identity_key(&(self.read)(value.clone())?)
                    .map_err(|e| QueryEvaluationError::Unexpected(Box::new(e)))?,
            ),
            Cell::ExplicitNone => Key::None,
            Cell::ReturnedError(value) => Key::Error(value.clone()),
        })
    }
    fn equal(&self, a: &Cell<T>, b: &Cell<T>) -> Result<bool, QueryEvaluationError> {
        Ok(self.key(a)? == self.key(b)?)
    }
    fn truth(&self, cell: &Option<Cell<T>>) -> Result<bool, QueryEvaluationError> {
        self.token.ensure_alive()?;
        Ok(match cell {
            None | Some(Cell::ExplicitNone) => false,
            Some(Cell::ReturnedError(_)) => true,
            Some(Cell::Rdf(value)) => {
                let raw = (self.read)(value.clone())?;
                if let Some(v) = raw.source_variable() { return Ok(!v.as_str().is_empty()); }
                match raw.rdf().map_err(|e| QueryEvaluationError::Unexpected(Box::new(e)))? {
                    Term::NamedNode(value) => !value.as_str().is_empty(),
                    Term::BlankNode(value) => !value.as_str().is_empty(),
                    Term::Literal(_) => self
                        .session
                        .source_literal_python_truth(&raw)
                        .map_err(|e| QueryEvaluationError::Unexpected(Box::new(e)))?,
                    #[cfg(feature = "sparql-12")]
                    Term::Triple(_) => return Err(invalid()),
                }
            }
        })
    }
    fn validate(&self, cells: &Endpoints<T>) -> Result<(), QueryEvaluationError> {
        for cell in cells.iter().flatten() {
            self.key(cell)?;
        }
        Ok(())
    }
}

pub(crate) struct CompiledSourcePath<'a, T> {
    runtime: Rc<SourcePathRuntime<'a, T>>,
    start: Start<'a, T>,
    pub(crate) source_class: &'static str,
    pub(crate) source_repr: String,
    pub(crate) source_hash: i64,
}
impl<'a, T: Clone + 'a> CompiledSourcePath<'a, T> {
    #[cfg(test)]
    pub(super) fn runtime(&self) -> &Rc<SourcePathRuntime<'a, T>> {
        &self.runtime
    }
    /// No scan or traversal occurs until the actual iterator's first poll.
    pub(crate) fn start(&self, endpoints: Endpoints<T>) -> Rows<'a, T> {
        guarded(&self.runtime, self.start.clone(), endpoints)
    }
}
struct Guarded<'a, T> {
    runtime: Rc<SourcePathRuntime<'a, T>>,
    pending: Option<(Start<'a, T>, Endpoints<T>)>,
    rows: Option<Rows<'a, T>>,
    terminal: bool,
}
fn guarded<'a, T: Clone + 'a>(
    runtime: &Rc<SourcePathRuntime<'a, T>>,
    start: Start<'a, T>,
    cells: Endpoints<T>,
) -> Rows<'a, T> {
    Box::new(Guarded {
        runtime: runtime.clone(),
        pending: Some((start, cells)),
        rows: None,
        terminal: false,
    })
}
impl<'a, T: Clone + 'a> Iterator for Guarded<'a, T> {
    type Item = Result<Pair<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.terminal {
            return None;
        }
        let result = (|| {
            self.runtime.token.ensure_alive()?;
            if let Some((start, cells)) = self.pending.take() {
                self.runtime.validate(&cells)?;
                self.rows = Some(start(cells)?);
            }
            self.runtime.token.ensure_alive()?;
            self.rows.as_mut().ok_or_else(invalid)?.next().transpose()
        })();
        match result {
            Ok(Some(pair)) => Some(Ok(pair)),
            Ok(None) => {
                self.terminal = true;
                self.rows = None;
                None
            }
            Err(error) => {
                self.terminal = true;
                self.rows = None;
                self.pending = None;
                Some(Err(error))
            }
        }
    }
}

struct FlatMap<'a, T> {
    runtime: Rc<SourcePathRuntime<'a, T>>,
    outer: Rows<'a, T>,
    inner: Option<Rows<'a, T>>,
    next: Start<'a, T>,
    endpoint: Option<Cell<T>>,
    anchor: Option<Cell<T>>,
    backward: bool,
}
impl<'a, T: Clone + 'a> Iterator for FlatMap<'a, T> {
    type Item = Result<Pair<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = (|| loop {
            self.runtime.token.ensure_alive()?;
            if let Some(inner) = &mut self.inner {
                if let Some(pair) = inner.next() {
                    let [s, o] = pair?;
                    let anchor = self.anchor.as_ref().ok_or_else(invalid)?.clone();
                    return Ok(Some(if self.backward {
                        [s, anchor]
                    } else {
                        [anchor, o]
                    }));
                }
                self.inner = None;
            }
            let Some(pair) = self.outer.next() else {
                return Ok(None);
            };
            let [s, o] = pair?;
            let cells = if self.backward {
                self.anchor = Some(o);
                [self.endpoint.clone(), Some(s)]
            } else {
                self.anchor = Some(s);
                [Some(o), self.endpoint.clone()]
            };
            self.inner = Some(guarded(&self.runtime, self.next.clone(), cells));
        })();
        result.transpose()
    }
}
fn forward_sequence<'a, T: Clone + 'a>(
    runtime: &Rc<SourcePathRuntime<'a, T>>,
    args: &[Start<'a, T>],
) -> Result<Start<'a, T>, QueryEvaluationError> {
    let Some(last) = args.last() else {
        return Err(invalid());
    };
    let mut tail = last.clone();
    for head in args[..args.len() - 1].iter().rev() {
        let runtime = runtime.clone();
        let head = head.clone();
        let next = tail;
        tail = Rc::new(move |[s, o]| {
            Ok(Box::new(FlatMap {
                outer: guarded(&runtime, head.clone(), [s, None]),
                inner: None,
                next: next.clone(),
                endpoint: o,
                anchor: None,
                backward: false,
                runtime: runtime.clone(),
            }))
        });
    }
    Ok(tail)
}
struct Alternatives<'a, T> {
    runtime: Rc<SourcePathRuntime<'a, T>>,
    args: Rc<[Start<'a, T>]>,
    cells: Endpoints<T>,
    at: usize,
    rows: Option<Rows<'a, T>>,
}
impl<'a, T: Clone + 'a> Iterator for Alternatives<'a, T> {
    type Item = Result<Pair<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = (|| loop {
            self.runtime.token.ensure_alive()?;
            if let Some(rows) = &mut self.rows {
                if let Some(row) = rows.next() {
                    return row.map(Some);
                }
                self.rows = None;
            }
            let Some(arg) = self.args.get(self.at) else {
                return Ok(None);
            };
            self.at += 1;
            self.rows = Some(guarded(&self.runtime, arg.clone(), self.cells.clone()));
        })();
        result.transpose()
    }
}

struct TraversalFrame<'a, T> {
    rows: Rows<'a, T>,
    anchor: Option<Cell<T>>,
    // A backward recursive call drops the original subject constraint.
    filter: Option<Cell<T>>,
    filter_truth: bool,
    pending: Option<(Cell<T>, Cell<T>)>,
}
struct Traversal<'a, T> {
    runtime: Rc<SourcePathRuntime<'a, T>>,
    inner: Start<'a, T>,
    backward: bool,
    more: bool,
    seen: HashSet<Key>,
    stack: Vec<TraversalFrame<'a, T>>,
}
impl<'a, T: Clone + 'a> Traversal<'a, T> {
    fn new(
        runtime: Rc<SourcePathRuntime<'a, T>>,
        inner: Start<'a, T>,
        cells: Endpoints<T>,
        backward: bool,
        more: bool,
    ) -> Result<Self, QueryEvaluationError> {
        let mut result = Self {
            runtime,
            inner,
            backward,
            more,
            seen: HashSet::new(),
            stack: Vec::new(),
        };
        let [s, o] = cells;
        let (endpoint, filter) = if backward { (o, s) } else { (s, o) };
        result.push(endpoint.ok_or_else(invalid)?, None, filter)?;
        Ok(result)
    }
    fn push(
        &mut self,
        endpoint: Cell<T>,
        anchor: Option<Cell<T>>,
        filter: Option<Cell<T>>,
    ) -> Result<(), QueryEvaluationError> {
        insert(&mut self.seen, self.runtime.key(&endpoint)?)?;
        let filter_truth = self.runtime.truth(&filter)?;
        let cells = if self.backward {
            [None, Some(endpoint)]
        } else {
            [Some(endpoint), None]
        };
        self.stack
            .try_reserve(1)
            .map_err(|_| resource("source_path_traversal_frames"))?;
        self.stack.push(TraversalFrame {
            rows: guarded(&self.runtime, self.inner.clone(), cells),
            anchor,
            filter,
            filter_truth,
            pending: None,
        });
        Ok(())
    }
}
impl<'a, T: Clone + 'a> Iterator for Traversal<'a, T> {
    type Item = Result<Pair<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = (|| loop {
            self.runtime.token.ensure_alive()?;
            let Some(frame) = self.stack.last_mut() else {
                return Ok(None);
            };
            if let Some((endpoint, anchor)) = frame.pending.take() {
                if self.more && !self.seen.contains(&self.runtime.key(&endpoint)?) {
                    let filter = if self.backward {
                        None
                    } else {
                        frame.filter.clone()
                    };
                    self.push(endpoint, Some(anchor), filter)?;
                }
                continue;
            }
            let Some(row) = frame.rows.next() else {
                self.stack.pop();
                continue;
            };
            let [s, o] = row?;
            let anchor = frame
                .anchor
                .clone()
                .unwrap_or_else(|| if self.backward { o.clone() } else { s.clone() });
            let endpoint = if self.backward { s.clone() } else { o.clone() };
            frame.pending = Some((endpoint.clone(), anchor.clone()));
            if !frame.filter_truth
                || self
                    .runtime
                    .equal(&endpoint, frame.filter.as_ref().ok_or_else(invalid)?)?
            {
                return Ok(Some(if self.backward {
                    [s, anchor]
                } else {
                    [anchor, o]
                }));
            }
        })();
        result.transpose()
    }
}

struct AllTraversal<'a, T> {
    runtime: Rc<SourcePathRuntime<'a, T>>,
    inner: Start<'a, T>,
    more: bool,
    zero_rows: Option<Box<dyn Iterator<Item = Result<[T; 3], QueryEvaluationError>> + 'a>>,
    zero_pending: Option<Cell<T>>,
    zero_seen: HashSet<Key>,
    roots: Option<Rows<'a, T>>,
    root_seen: HashSet<Key>,
    materialized: std::vec::IntoIter<Pair<T>>,
}
impl<'a, T: Clone + 'a> Iterator for AllTraversal<'a, T> {
    type Item = Result<Pair<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = (|| loop {
            self.runtime.token.ensure_alive()?;
            if let Some(cell) = self.zero_pending.take() {
                if insert(&mut self.zero_seen, self.runtime.key(&cell)?)? {
                    return Ok(Some([cell.clone(), cell]));
                }
            }
            if let Some(rows) = &mut self.zero_rows {
                if let Some(row) = rows.next() {
                    let [s, _, o] = row?;
                    self.zero_pending = Some(Cell::Rdf(o));
                    let s = Cell::Rdf(s);
                    if insert(&mut self.zero_seen, self.runtime.key(&s)?)? {
                        return Ok(Some([s.clone(), s]));
                    }
                    continue;
                }
                self.zero_rows = None;
            }
            if let Some(row) = self.materialized.next() {
                return Ok(Some(row));
            }
            if self.roots.is_none() {
                self.roots = Some(guarded(&self.runtime, self.inner.clone(), [None, None]));
            }
            let Some(row) = self.roots.as_mut().ok_or_else(invalid)?.next() else {
                return Ok(None);
            };
            let pair = row?;
            if !self.more {
                return Ok(Some(pair));
            }
            let s = pair[0].clone();
            if !insert(&mut self.root_seen, self.runtime.key(&s)?)? {
                continue;
            }
            // Native _all_fwd_paths constructs list(_fwd(s, None, set()))
            // before exposing any member of this subject's closure.
            let traversal = Traversal::new(
                self.runtime.clone(),
                self.inner.clone(),
                [Some(s.clone()), None],
                false,
                true,
            )?;
            let mut rows = Vec::new();
            for row in traversal {
                self.runtime.token.ensure_alive()?;
                let pair = row?;
                if !self.runtime.equal(&pair[0], &s)? {
                    return Err(invalid());
                }
                rows.try_reserve(1)
                    .map_err(|_| resource("source_path_subject_closure"))?;
                rows.push(pair);
            }
            self.materialized = rows.into_iter();
        })();
        result.transpose()
    }
}
struct Repeated<'a, T> {
    runtime: Rc<SourcePathRuntime<'a, T>>,
    initial_zero: Option<Pair<T>>,
    rows: Rows<'a, T>,
    done: HashSet<[Key; 2]>,
}
impl<'a, T: Clone + 'a> Iterator for Repeated<'a, T> {
    type Item = Result<Pair<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = (|| {
            self.runtime.token.ensure_alive()?;
            // Native initial zero is outside done: a later self-cycle can
            // repeat this very pair when at least one endpoint is truthy.
            if let Some(pair) = self.initial_zero.take() {
                return Ok(Some(pair));
            }
            for row in self.rows.by_ref() {
                self.runtime.token.ensure_alive()?;
                let pair = row?;
                let key = [self.runtime.key(&pair[0])?, self.runtime.key(&pair[1])?];
                if insert(&mut self.done, key)? {
                    return Ok(Some(pair));
                }
            }
            Ok(None)
        })();
        result.transpose()
    }
}

enum NegatedArgument<'a, T> {
    Named(T),
    Inverse(Start<'a, T>),
    Untranslated(String),
}
struct Negated<'a, T> {
    runtime: Rc<SourcePathRuntime<'a, T>>,
    args: Rc<[NegatedArgument<'a, T>]>,
    rows: Box<dyn Iterator<Item = Result<[T; 3], QueryEvaluationError>> + 'a>,
}
impl<'a, T: Clone + 'a> Iterator for Negated<'a, T> {
    type Item = Result<Pair<T>, QueryEvaluationError>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = (|| 'row: loop {
            self.runtime.token.ensure_alive()?;
            let Some(row) = self.rows.next() else {
                return Ok(None);
            };
            let [s, p, o] = row?;
            let s = Cell::Rdf(s);
            let o = Cell::Rdf(o);
            let p = Cell::Rdf(p);
            for arg in self.args.iter() {
                self.runtime.token.ensure_alive()?;
                match arg {
                    NegatedArgument::Named(value) => {
                        if self.runtime.equal(&p, &Cell::Rdf(value.clone()))? {
                            continue 'row;
                        }
                    }
                    NegatedArgument::Inverse(start) => {
                        if guarded(
                            &self.runtime,
                            start.clone(),
                            [Some(o.clone()), Some(s.clone())],
                        )
                        .next()
                        .transpose()?
                        .is_some()
                        {
                            continue 'row;
                        }
                    }
                    NegatedArgument::Untranslated(text) => {
                        return Err(QueryEvaluationError::Unexpected(Box::new(
                            SourceCompValueError {
                                module: "builtins",
                                class: "Exception",
                                argument: format!("Invalid path in NegatedPath: {text}"),
                            },
                        )));
                    }
                }
            }
            return Ok(Some([s, o]));
        })();
        result.transpose()
    }
}

fn flattened<'a>(
    definition: &'a SourcePathDefinition,
    sequence: bool,
    depth: usize,
    result: &mut Vec<&'a SourcePathDefinition>,
) -> Result<(), QueryEvaluationError> {
    if depth >= 256 {
        return Err(resource("source_path_compile_depth"));
    }
    match definition {
        SourcePathDefinition::Sequence(args) if sequence => {
            for arg in args {
                flattened(arg, sequence, depth + 1, result)?;
            }
        }
        SourcePathDefinition::Alternative(args) if !sequence => {
            for arg in args {
                flattened(arg, sequence, depth + 1, result)?;
            }
        }
        _ => {
            result
                .try_reserve(1)
                .map_err(|_| resource("source_path_compile_args"))?;
            result.push(definition);
        }
    }
    Ok(())
}

/// Pure constructor representation used by translation-time tuple comparison.
/// No dataset scan, internalization or path execution is performed here.
pub(crate) fn source_path_text(
    definition: &SourcePathDefinition,
    session: &Rc<QueryNumericSession>,
    registry: &PreparedSourceLiteralRegistry,
    check: &mut impl FnMut() -> Result<(), QueryEvaluationError>,
) -> Result<(&'static str, String), QueryEvaluationError> {
    fn text(
        def: &SourcePathDefinition,
        session: &Rc<QueryNumericSession>,
        registry: &PreparedSourceLiteralRegistry,
        check: &mut impl FnMut() -> Result<(), QueryEvaluationError>,
        depth: usize,
    ) -> Result<(&'static str, String), QueryEvaluationError> {
        use SourcePathDefinition as P;
        check()?;
        if depth >= 256 {
            return Err(resource("source_path_compile_depth"));
        }
        Ok(match def {
            P::NamedNode(v) => ("URIRef", v.as_str().to_owned()),
            P::Inverse(v) => (
                "InvPath",
                format!("Path(~{})", text(v, session, registry, check, depth + 1)?.1),
            ),
            P::Sequence(_) | P::Alternative(_) => {
                let sequence = matches!(def, P::Sequence(_));
                let mut flat = Vec::new();
                flattened(def, sequence, depth, &mut flat)?;
                let parts = flat
                    .into_iter()
                    .map(|v| text(v, session, registry, check, depth + 1).map(|v| v.1))
                    .collect::<Result<Vec<_>, _>>()?;
                (
                    if sequence {
                        "SequencePath"
                    } else {
                        "AlternativePath"
                    },
                    format!("Path({})", parts.join(if sequence { " / " } else { " | " })),
                )
            }
            P::Repeat { inner, mode } => (
                "MulPath",
                format!(
                    "Path({}{})",
                    text(inner, session, registry, check, depth + 1)?.1,
                    match mode {
                        SourcePathRepeat::ZeroOrOne => "?",
                        SourcePathRepeat::ZeroOrMore => "*",
                        SourcePathRepeat::OneOrMore => "+",
                    }
                ),
            ),
            P::Negated(args) => {
                let mut parts = Vec::new();
                for arg in args {
                    parts.push(match arg {
                        SourcePathNegatedArgument::NamedNode(v) => v.as_str().to_owned(),
                        SourcePathNegatedArgument::Inverse(v) => {
                            format!("Path(~{})", text(v, session, registry, check, depth + 1)?.1)
                        }
                        SourcePathNegatedArgument::Untranslated(syntax) => render_mapping(
                            syntax.nodes(),
                            syntax.root(),
                            true,
                            session,
                            registry,
                            check,
                        )?,
                    });
                }
                ("NegatedPath", format!("Path(! {})", parts.join(",")))
            }
        })
    }
    text(definition, session, registry, check, 0)
}

pub(crate) fn compile_source_path<'a, T: Clone + 'a>(
    runtime: Rc<SourcePathRuntime<'a, T>>,
    definition: &SourcePathDefinition,
) -> Result<CompiledSourcePath<'a, T>, QueryEvaluationError> {
    fn compile<'a, T: Clone + 'a>(
        runtime: &Rc<SourcePathRuntime<'a, T>>,
        definition: &SourcePathDefinition,
        depth: usize,
    ) -> Result<(Start<'a, T>, &'static str, String), QueryEvaluationError> {
        use SourcePathDefinition as P;
        runtime.token.ensure_alive()?;
        if depth >= 256 {
            return Err(resource("source_path_compile_depth"));
        }
        Ok(match definition {
            P::NamedNode(node) => {
                let predicate =
                    (runtime.write)(RawExpressionValue::from_import(node.clone().into()))?;
                let runtime = runtime.clone();
                let start: Start<'a, T> = Rc::new(move |[s, o]| {
                    let rows = (runtime.scan)([s, Some(Cell::Rdf(predicate.clone())), o])?;
                    Ok(Box::new(rows.map(|row| {
                        row.map(|[s, _, o]| [Cell::Rdf(s), Cell::Rdf(o)])
                    })))
                });
                (start, "URIRef", node.as_str().to_owned())
            }
            P::Inverse(arg) => {
                let (child, _, repr) = compile(runtime, arg, depth + 1)?;
                let runtime = runtime.clone();
                let start: Start<'a, T> = Rc::new(move |[s, o]| {
                    Ok(Box::new(
                        guarded(&runtime, child.clone(), [o, s])
                            .map(|row| row.map(|[s, o]| [o, s])),
                    ))
                });
                (start, "InvPath", format!("Path(~{repr})"))
            }
            P::Sequence(_) | P::Alternative(_) => {
                let sequence = matches!(definition, P::Sequence(_));
                let mut flat = Vec::new();
                flattened(definition, sequence, depth, &mut flat)?;
                let mut args = Vec::new();
                let mut repr = Vec::new();
                args.try_reserve_exact(flat.len())
                    .map_err(|_| resource("source_path_compile_args"))?;
                repr.try_reserve_exact(flat.len())
                    .map_err(|_| resource("source_path_compile_args"))?;
                for arg in flat {
                    let (start, _, text) = compile(runtime, arg, depth + 1)?;
                    args.push(start);
                    repr.push(text);
                }
                if sequence {
                    let forward = forward_sequence(runtime, &args)?;
                    let backward = if args.len() == 1 {
                        args[0].clone()
                    } else {
                        let last = args.last().ok_or_else(invalid)?.clone();
                        let prefix = forward_sequence(runtime, &args[..args.len() - 1])?;
                        let runtime = runtime.clone();
                        let backward: Start<'a, T> = Rc::new(move |[s, o]| {
                            Ok(Box::new(FlatMap {
                                outer: guarded(&runtime, last.clone(), [None, o]),
                                inner: None,
                                next: prefix.clone(),
                                endpoint: s,
                                anchor: None,
                                backward: true,
                                runtime: runtime.clone(),
                            }))
                        });
                        backward
                    };
                    let runtime = runtime.clone();
                    let start: Start<'a, T> = Rc::new(move |cells| {
                        if runtime.truth(&cells[0])? {
                            forward(cells)
                        } else if runtime.truth(&cells[1])? {
                            backward(cells)
                        } else {
                            forward(cells)
                        }
                    });
                    (start, "SequencePath", format!("Path({})", repr.join(" / ")))
                } else {
                    let args: Rc<[_]> = args.into();
                    let runtime = runtime.clone();
                    let start: Start<'a, T> = Rc::new(move |cells| {
                        Ok(Box::new(Alternatives {
                            runtime: runtime.clone(),
                            args: args.clone(),
                            cells,
                            at: 0,
                            rows: None,
                        }))
                    });
                    (
                        start,
                        "AlternativePath",
                        format!("Path({})", repr.join(" | ")),
                    )
                }
            }
            P::Repeat { inner, mode } => {
                let (child, _, repr) = compile(runtime, inner, depth + 1)?;
                let zero = *mode != SourcePathRepeat::OneOrMore;
                let more = *mode != SourcePathRepeat::ZeroOrOne;
                let modifier = match mode {
                    SourcePathRepeat::ZeroOrOne => "?",
                    SourcePathRepeat::ZeroOrMore => "*",
                    SourcePathRepeat::OneOrMore => "+",
                };
                let runtime = runtime.clone();
                let start: Start<'a, T> = Rc::new(move |cells| {
                    let s_truth = runtime.truth(&cells[0])?;
                    let o_truth = runtime.truth(&cells[1])?;
                    let mut initial_zero = None;
                    if zero {
                        if s_truth && o_truth {
                            let s = cells[0].as_ref().ok_or_else(invalid)?;
                            let o = cells[1].as_ref().ok_or_else(invalid)?;
                            if runtime.equal(s, o)? {
                                initial_zero = Some([s.clone(), o.clone()]);
                            }
                        } else if s_truth {
                            let s = cells[0].as_ref().ok_or_else(invalid)?;
                            initial_zero = Some([s.clone(), s.clone()]);
                        } else if o_truth {
                            let o = cells[1].as_ref().ok_or_else(invalid)?;
                            initial_zero = Some([o.clone(), o.clone()]);
                        }
                    }
                    // Traversal setup itself does not scan/poll child rows.
                    let rows: Rows<'a, T> = if s_truth || o_truth {
                        Box::new(Traversal::new(
                            runtime.clone(),
                            child.clone(),
                            cells,
                            !s_truth,
                            more,
                        )?)
                    } else {
                        Box::new(AllTraversal {
                            runtime: runtime.clone(),
                            inner: child.clone(),
                            more,
                            zero_rows: if zero {
                                Some((runtime.scan)([None, None, None])?)
                            } else {
                                None
                            },
                            zero_pending: None,
                            zero_seen: HashSet::new(),
                            roots: None,
                            root_seen: HashSet::new(),
                            materialized: Vec::new().into_iter(),
                        })
                    };
                    Ok(Box::new(Repeated {
                        runtime: runtime.clone(),
                        initial_zero,
                        rows,
                        done: HashSet::new(),
                    }))
                });
                (start, "MulPath", format!("Path({repr}{modifier})"))
            }
            P::Negated(arguments) => {
                let mut args = Vec::new();
                let mut repr = Vec::new();
                args.try_reserve_exact(arguments.len())
                    .map_err(|_| resource("source_path_compile_negated"))?;
                for arg in arguments {
                    match arg {
                        SourcePathNegatedArgument::NamedNode(node) => {
                            args.push(NegatedArgument::Named((runtime.write)(
                                RawExpressionValue::from_import(node.clone().into()),
                            )?));
                            repr.push(node.as_str().to_owned());
                        }
                        SourcePathNegatedArgument::Inverse(inner) => {
                            let (start, _, text) = compile(runtime, inner, depth + 1)?;
                            args.push(NegatedArgument::Inverse(start));
                            repr.push(format!("Path(~{text})"));
                        }
                        SourcePathNegatedArgument::Untranslated(syntax) => {
                            let text = render_mapping(
                                syntax.nodes(),
                                syntax.root(),
                                true,
                                &runtime.session,
                                &runtime.registry,
                                &mut || runtime.token.ensure_alive(),
                            )?;
                            args.push(NegatedArgument::Untranslated(text.clone()));
                            repr.push(text);
                        }
                    }
                }
                let args: Rc<[_]> = args.into();
                let runtime = runtime.clone();
                let start: Start<'a, T> = Rc::new(move |[s, o]| {
                    Ok(Box::new(Negated {
                        rows: (runtime.scan)([s, None, o])?,
                        runtime: runtime.clone(),
                        args: args.clone(),
                    }))
                });
                (start, "NegatedPath", format!("Path(! {})", repr.join(",")))
            }
        })
    }
    if !runtime.registry.admits_graph_execution(&runtime.session) {
        return Err(invalid());
    }
    let (start, source_class, source_str) = compile(&runtime, definition, 0)?;
    if source_path_text(definition, &runtime.session, &runtime.registry, &mut || {
        runtime.token.ensure_alive()
    })? != (source_class, source_str.clone())
    {
        return Err(invalid());
    }
    let source_repr = if let SourcePathDefinition::NamedNode(node) = definition {
        runtime
            .session
            .source_repr(&SolutionValue::Rdf(RawExpressionValue::from_import(
                node.clone().into(),
            )))
            .map_err(|e| QueryEvaluationError::Unexpected(Box::new(e)))?
    } else {
        source_str.clone()
    };
    let source_hash = runtime
        .session
        .source_hash()
        .ok_or_else(invalid)?
        .variable_hash(&source_str);
    Ok(CompiledSourcePath {
        runtime,
        start,
        source_class,
        source_repr,
        source_hash,
    })
}
