//! Private preparation over the actual parser, planner, evaluator and Dataset.
//! Only this constructor can supply the prepared-query provenance envelope.
use super::*;
use crate::QueryEvaluator;
use crate::dataset::nullable_numeric::{
    CachedInternalTerm, NumericDataset, SourceQueryableDataset,
};
use crate::model::SourcePresenceSolution;
use crate::numeric::{
    NumericInitialBinding, NumericInitialValue, QueryGroupSemantics, QueryLiteralSemantics,
    RawNumericFault,
};
use spargebra::{NumericQueryParseError, Query, SparqlParser};

#[derive(Debug)]
pub(crate) enum NumericPrepareError {
    Syntax(spargebra::SparqlSyntaxError),
    SourceSyntax(spargebra::SourceParserError),
    LiteralConstruction(RawNumericFault),
    InitialLiteralConstruction(RawNumericFault),
    Query(QueryEvaluationError),
}
impl fmt::Display for NumericPrepareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(f),
            Self::SourceSyntax(error) => error.fmt(f),
            Self::LiteralConstruction(error) | Self::InitialLiteralConstruction(error) => {
                error.fmt(f)
            }
            Self::Query(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for NumericPrepareError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Syntax(error) => error,
            Self::SourceSyntax(error) => error,
            Self::LiteralConstruction(error) | Self::InitialLiteralConstruction(error) => error,
            Self::Query(error) => error,
        })
    }
}

pub(crate) struct PreparedNumericInitial<T> {
    values: Rc<FxHashMap<Variable, SolutionValue<T>>>,
    source_order: Option<Rc<[Variable]>>,
}
impl<T: Clone> PreparedNumericInitial<T> {
    pub(crate) fn encode_for(
        &self,
        variables: &[Variable],
    ) -> Result<InternalTuple<T>, QueryEvaluationError> {
        let mut result = InternalTuple::with_capacity(variables.len());
        if let Some(order) = &self.source_order {
            for variable in order.iter() {
                let index = variables
                    .iter()
                    .position(|candidate| candidate == variable)
                    .ok_or_else(|| {
                        QueryEvaluationError::Unexpected(Box::new(
                            RawNumericFault::InvalidSourceOrigin,
                        ))
                    })?;
                let value = self.values.get(variable).ok_or_else(|| {
                    QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
                })?;
                result.set_cell(index, value.clone());
            }
            return Ok(result);
        }
        for (index, variable) in variables.iter().enumerate() {
            if let Some(value) = self.values.get(variable) {
                result.set_cell(index, value.clone());
            }
        }
        // Nonprojected application bindings remain in the evaluator context;
        // Project injects them into its child. They are not illegal SELECT
        // substitutions and are never re-internalized to build the root tuple.
        Ok(result)
    }
}

pub(crate) struct PreparedNumericQuery<'a, D: QueryableDataset<'a>> {
    plan: GraphPattern,
    evaluator: SimpleEvaluator<'a, NumericDataset<'a, D>>,
    initial: PreparedNumericInitial<CachedInternalTerm<D::InternalTerm>>,
    result_form: super::source_runtime_result::ResultForm,
    runtime_result_admitted: bool,
}
pub(super) struct PreparedSourceRows<'a, T> {
    pub(super) codec: Rc<RawTermCodec<'a, T>>,
    pub(super) cancellation: CancellationToken,
    pub(super) rows: InternalTuplesIterator<'a, T>,
    pub(super) variables: Arc<[Variable]>,
    pub(super) initial: Rc<FxHashMap<Variable, SolutionValue<T>>>,
}
impl<'a, D: QueryableDataset<'a>> PreparedNumericQuery<'a, D> {
    #[cfg(test)]
    pub(super) fn source_literal_registry(
        &self,
    ) -> Option<&Rc<crate::numeric::PreparedSourceLiteralRegistry>> {
        self.evaluator
            .dataset
            .source_literal_codec()?
            .prepared_literals
            .as_ref()
    }
    #[cfg(test)]
    pub(super) fn with_source_context_observer(
        mut self,
        observer: source_context_join::SourceContextObserver<
            'a,
            CachedInternalTerm<D::InternalTerm>,
        >,
    ) -> Self {
        if let Some(runtime) = self.evaluator.source_context.take() {
            self.evaluator.source_context =
                Some(Rc::new(runtime.as_ref().clone().with_observer(observer)));
        }
        self
    }
    #[cfg(test)]
    pub(super) fn with_source_order_observer(
        mut self,
        observer: source_ordering::SourceOrderObserver<'a>,
    ) -> Self {
        self.evaluator.source_order_observer = Some(observer);
        self
    }
    /// Consume only the plan and initial context bound by actual preparation.
    /// No caller-supplied AST can borrow this prepared-literal provenance.
    pub(crate) fn execute_presence(
        self,
    ) -> Result<
        impl Iterator<Item = Result<PresenceSolution, QueryEvaluationError>> + 'a,
        QueryEvaluationError,
    > {
        if self
            .evaluator
            .dataset
            .numeric
            .as_ref()
            .is_some_and(|codec| {
                codec.session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1
            })
        {
            return Err(QueryEvaluationError::Unexpected(Box::new(
                RawNumericFault::SourceMetadataWouldBeLost,
            )));
        }
        let mut variables = Vec::new();
        let (plan, _) = self
            .evaluator
            .graph_pattern_evaluator(&self.plan, &mut variables);
        let input = self.initial.encode_for(&variables)?;
        let rows = plan?(input)?;
        Ok(nullable::halt_after_error(decode_presence_bindings(
            self.evaluator.dataset.clone(),
            rows,
            variables.into(),
        )))
    }

    /// Consume the same sealed plan, initial context, Dataset and source session.
    /// Final RDF-cell decoding calls the actual bound raw codec and retains its
    /// original conversion error. No ordinary AST or term-only result can be
    /// promoted to source provenance by using this entry.
    pub(crate) fn execute_source_presence(
        self,
    ) -> Result<
        impl Iterator<Item = Result<SourcePresenceSolution, QueryEvaluationError>> + 'a,
        QueryEvaluationError,
    > {
        if !matches!(
            self.result_form,
            super::source_runtime_result::ResultForm::Select
        ) {
            return Err(QueryEvaluationError::Unexpected(
                "source presence requires a SELECT query".into(),
            ));
        }
        let stream = self.source_rows()?;
        Ok(decode_source_presence_bindings(
            stream.codec,
            stream.cancellation,
            stream.rows,
            stream.variables,
        ))
    }

    /// The original wrapper's result layer consumes this prepared query's
    /// actual form and source values. No public profile is enabled here.
    pub(crate) fn execute_source_runtime_result(
        mut self,
    ) -> Result<serde_json::Value, QueryEvaluationError> {
        if !self.runtime_result_admitted {
            return Err(QueryEvaluationError::Unexpected(
                "source runtime result admission required".into(),
            ));
        }
        let form = std::mem::replace(
            &mut self.result_form,
            super::source_runtime_result::ResultForm::Select,
        );
        super::source_runtime_result::consume(form, self.source_rows()?)
    }

    fn source_rows(self) -> Result<PreparedSourceRows<'a, CachedInternalTerm<D::InternalTerm>>, QueryEvaluationError> {
        self.source_rows_with_projection(true,None)
    }
    pub(super) fn execute_source_update_rows(self) -> Result<PreparedSourceRows<'a, CachedInternalTerm<D::InternalTerm>>, QueryEvaluationError> {
        self.source_rows_with_projection(false,None)
    }
    pub(super) fn source_update_term(&self,raw:RawExpressionValue)->Result<CachedInternalTerm<D::InternalTerm>,QueryEvaluationError> {
        (self.evaluator.dataset.numeric.as_ref().ok_or_else(||QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin)))?.internalize)(raw)
    }
    pub(super) fn execute_source_delete_where_rows(self,parts:Vec<(Option<CachedInternalTerm<D::InternalTerm>>,Vec<spargebra::term::TriplePattern>)>)->Result<PreparedSourceRows<'a,CachedInternalTerm<D::InternalTerm>>,QueryEvaluationError> {
        self.source_rows_with_projection(false,Some(parts))
    }
    fn source_rows_with_projection(
        self,
        project_result: bool,
        direct_bgps:Option<Vec<(Option<CachedInternalTerm<D::InternalTerm>>,Vec<spargebra::term::TriplePattern>)>>,
    ) -> Result<PreparedSourceRows<'a, CachedInternalTerm<D::InternalTerm>>, QueryEvaluationError>
    {
        let codec = self
            .evaluator
            .dataset
            .numeric
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
            })?;
        if codec.session.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1 {
            return Err(QueryEvaluationError::Unexpected(Box::new(
                RawNumericFault::UnsupportedSourceLiteralOperator(
                    "source presence requires SourceLiteralV1",
                ),
            )));
        }
        let cancellation = self.evaluator.dataset.cancellation_token.clone();
        cancellation.ensure_alive()?;
        let source_runtime = self.evaluator.source_context.clone();
        let mut variables = source_runtime
            .as_ref()
            .map_or_else(Vec::new, |runtime| runtime.variables.to_vec());
        let projected = if project_result {
            source_runtime.as_ref().map(|_| source_context::source_query_projection(&self.plan)).transpose()?
        } else { None };
        let plan=if let Some(parts)=direct_bgps {
            let runtime=self.evaluator.source_context_runtime()?;
            let mut plans=Vec::new();
            for (scope,patterns) in parts {
                let child=if let Some(scope)=scope {
                    self.evaluator.source_scoped_evaluator.as_ref().ok_or_else(||QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin)))?(&self.evaluator,scope)?
                } else {self.evaluator.clone()};
                let physical=GraphPattern::from(&spargebra::algebra::GraphPattern::Bgp{patterns:patterns.clone()});
                plans.push(child.source_bgp_evaluator_ordered(&physical,&patterns,&mut variables,false)?);
            }
            let mut plans=plans.into_iter();
            let first=plans.next().ok_or_else(||QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin)))?;
            plans.fold(first,|left,right| source_context_join::source_join_ordered_list_evaluator(runtime.clone(),left,right))
        } else {
            self.evaluator.graph_pattern_evaluator(&self.plan,&mut variables).0?
        };
        if source_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.variables.as_ref() != variables.as_slice())
        {
            return Err(QueryEvaluationError::Unexpected(Box::new(
                RawNumericFault::InvalidSourceOrigin,
            )));
        }
        let input = self.initial.encode_for(&variables)?;
        let input = if let Some(runtime) = &source_runtime {
            let current = input.source_context_from_bindings(Rc::clone(&runtime.initial));
            runtime.observe_query_context_start(&current);
            current
        } else {
            input
        };
        let mut rows = plan(input)?;
        let output_variables = if let Some(projected) = projected {
            let slots: Vec<_> = projected
                .iter()
                .map(|variable| {
                    variables
                        .iter()
                        .position(|candidate| candidate == variable)
                        .ok_or_else(|| {
                            QueryEvaluationError::Unexpected(Box::new(
                                RawNumericFault::InvalidSourceOrigin,
                            ))
                        })
                })
                .collect::<Result<_, _>>()?;
            // This is solely the final column mapping. Every actual compiler
            // consumer retains the one query-wide slot domain and row identity.
            let runtime = source_runtime.ok_or_else(|| {
                QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin))
            })?;
            rows = Box::new(nullable::halt_after_error(rows.map(move |row| {
                let row = row?;
                runtime.observe_output_row(&row);
                for &slot in row.source_local_order() {
                    if !slots.contains(&slot) {
                        return Err(QueryEvaluationError::InvalidSolutionDomain(slot));
                    }
                }
                let mut output = InternalTuple::with_capacity(slots.len());
                for (index, &slot) in slots.iter().enumerate() {
                    if let Some(value) = row.cell(slot) {
                        output.set_cell(index, value.clone());
                    }
                }
                Ok(output)
            })));
            projected
        } else {
            variables
        };
        Ok(PreparedSourceRows {
            codec,
            cancellation,
            rows,
            variables: output_variables.into(),
            initial: self.initial.values,
        })
    }

    // Receiving tests inspect intermediate caches and explicitly exercise
    // both physical join strategies. Production callers cannot replace a plan.
    #[cfg(test)]
    pub(crate) fn plan(&self) -> &GraphPattern {
        &self.plan
    }
    #[cfg(test)]
    pub(crate) fn evaluator(&self) -> &SimpleEvaluator<'a, NumericDataset<'a, D>> {
        &self.evaluator
    }
    #[cfg(test)]
    pub(crate) fn initial(&self) -> &PreparedNumericInitial<CachedInternalTerm<D::InternalTerm>> {
        &self.initial
    }
}

pub(crate) fn prepare_numeric_query<'a, D: QueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    let source_context =
        session.context_semantics() == crate::numeric::QueryContextSemantics::SourceContextV1;
    let constructor_session = Rc::clone(&session);
    let constructor: spargebra::NumericLiteralConstructor =
        Rc::new(move |kind, sign, magnitude| {
            let raw = constructor_session
                .construct_numeric_token(kind, sign, magnitude)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
            match raw.rdf()? {
                Term::Literal(literal) => Ok(literal.clone()),
                _ => Err(Box::new(RawNumericFault::InvalidSourceOrigin)),
            }
        });
    let parsed = if source_context {
        SparqlParser::new().parse_query_rdflib760_source_context(
            query,
            constructor,
            session.group_semantics() == QueryGroupSemantics::SourceAggregateV1,
        )
    } else if session.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1 {
        SparqlParser::new().parse_query_rdflib760_source_literals(
            query,
            constructor,
            session.group_semantics() == QueryGroupSemantics::SourceAggregateV1,
        )
    } else {
        match session.group_semantics() {
            QueryGroupSemantics::LegacyPrivateV1 => {
                SparqlParser::new().parse_query_rdflib760_numeric(query, constructor)
            }
            QueryGroupSemantics::SourceAggregateV1 => {
                SparqlParser::new().parse_query_rdflib760_numeric_aggregates(query, constructor)
            }
        }
    };
    bind_parsed_numeric_query(parsed, dataset, session, initial, None)
}

/// Private source front-end admission. This uses the same bound evaluator;
/// newly translated EXISTS nodes remain explicitly gated until their child
/// execution is independently received. Existing preparation keeps its parser.
pub(crate) fn prepare_source_frontend_numeric_query<'a, D: QueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    prepare_source_frontend_phase(query, dataset, session, initial, false)
}

pub(crate) fn prepare_source_exists_numeric_query<'a, D: QueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    prepare_source_frontend_phase(query, dataset, session, initial, true)
}

/// Source graph execution has its own private admission constructor. The
/// sealed Dataset owns the exact session and actual source objects; a profile,
/// AST, ordinary RDF backend, or equal session configuration cannot grant it.
pub(crate) fn prepare_source_graph_numeric_query<'a, D: SourceQueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    prepare_source_graph_numeric_query_with_constructor(query, dataset, session, initial, None)
}

pub(crate) fn prepare_source_graph_numeric_query_with_constructor<
    'a,
    D: SourceQueryableDataset<'a>,
>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
    constructor: Option<spargebra::SourceBlankNodeConstructor>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    prepare_source_graph_result(
        query,
        dataset,
        session,
        initial,
        constructor,
        false,
        &std::collections::BTreeMap::new(),
    )
}

/// Explicit private admission of the original wrapper's three query forms.
/// It shares source construction, Dataset identity, parser and evaluator with
/// the existing SELECT owner; the older entry retains its original refusal.
pub(crate) fn prepare_source_runtime_query<'a, D: SourceQueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    prepare_source_graph_result(
        query,
        dataset,
        session,
        initial,
        None,
        true,
        &std::collections::BTreeMap::new(),
    )
}

pub(crate) fn prepare_source_runtime_query_with_options<'a, D: SourceQueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
    namespaces: &std::collections::BTreeMap<String, String>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    prepare_source_graph_result(query, dataset, session, initial, None, true, namespaces)
}

fn prepare_source_graph_result<'a, D: SourceQueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
    constructor: Option<spargebra::SourceBlankNodeConstructor>,
    runtime_result_admitted: bool,
    namespaces: &std::collections::BTreeMap<String, String>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    prepare_source_graph_result_constructed(query, dataset, session, initial, constructor,
        runtime_result_admitted, namespaces, None)
}

/// Values came from the actual wrapper constructor on this numeric session.
/// This internal entry avoids constructing an initial literal a second time.
pub(crate) fn prepare_source_runtime_query_with_constructed_options<'a, D: SourceQueryableDataset<'a>>(
    query: &str, dataset: D, session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>, values: Vec<Option<RawExpressionValue>>,
    namespaces: &std::collections::BTreeMap<String, String>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    if initial.len() != values.len() {
        return Err(NumericPrepareError::LiteralConstruction(RawNumericFault::InvalidSourceOrigin));
    }
    prepare_source_graph_result_constructed(query, dataset, session, initial, None,
        true, namespaces, Some(values))
}

fn prepare_source_graph_result_constructed<'a, D: SourceQueryableDataset<'a>>(
    query: &str, dataset: D, session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>, constructor: Option<spargebra::SourceBlankNodeConstructor>,
    runtime_result_admitted: bool, namespaces: &std::collections::BTreeMap<String, String>,
    constructed: Option<Vec<Option<RawExpressionValue>>>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    if session.context_semantics() != crate::numeric::QueryContextSemantics::SourceContextV1
        || session.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1
    {
        return Err(NumericPrepareError::LiteralConstruction(
            RawNumericFault::InvalidSourceOrigin,
        ));
    }
    let dataset = NumericDataset::new_with_source_dataset(dataset, Rc::clone(&session))
        .map_err(NumericPrepareError::Query)?;
    let registry =
        crate::numeric::PreparedSourceLiteralRegistry::new_for_graph_execution(Rc::clone(&session));
    let initial_values = if let Some(values) = constructed {
        Some(values)
    } else if session.admits_source_calendar_duration() {
        Some(
            initial
                .iter()
                .map(|binding| match &binding.value {
                    NumericInitialValue::SourceNone | NumericInitialValue::SourceError(_) => {
                        Ok(None)
                    }
                    value => session
                        .construct_source_graph_initial(value.clone())
                        .map(Some)
                        .map_err(NumericPrepareError::InitialLiteralConstruction),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };
    #[cfg(test)]
    if session.admits_source_calendar_duration() {
        super::source_function_registry_receiving::lifecycle("before_parse");
    }
    let mut parser = if let Some(constructor) = constructor {
        SparqlParser::new().with_source_blank_node_constructor(constructor)
    } else {
        SparqlParser::new()
    };
    for (prefix, iri) in namespaces {
        parser = parser.with_prefix(prefix, iri).map_err(|error| {
            NumericPrepareError::Query(QueryEvaluationError::Unexpected(Box::new(error)))
        })?;
    }
    let parsed = parser.parse_query_rdflib760_source_exists_frontend(
        query,
        registry.hooks(),
        session.group_semantics() == QueryGroupSemantics::SourceAggregateV1,
    );
    #[cfg(test)]
    if parsed.is_ok() {
        super::source_function_registry_receiving::lifecycle("after_parse");
    }
    bind_parsed_numeric_dataset_query_form(
        parsed,
        dataset,
        session,
        initial,
        Some(registry),
        initial_values,
        runtime_result_admitted,
    )
}

/// Private path admission over the same sealed graph, parser and evaluator.
/// Existing graph/EXISTS factories retain their received operator boundaries.
pub(crate) fn prepare_source_path_numeric_query<'a, D: SourceQueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    if session.context_semantics() != crate::numeric::QueryContextSemantics::SourceContextV1
        || session.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1
    {
        return Err(NumericPrepareError::LiteralConstruction(
            RawNumericFault::InvalidSourceOrigin,
        ));
    }
    let dataset = NumericDataset::new_with_source_dataset(dataset, Rc::clone(&session))
        .map_err(NumericPrepareError::Query)?;
    let registry =
        crate::numeric::PreparedSourceLiteralRegistry::new_for_path_execution(Rc::clone(&session));
    let parsed = SparqlParser::new().parse_query_rdflib760_source_exists_frontend(
        query,
        registry.hooks(),
        session.group_semantics() == QueryGroupSemantics::SourceAggregateV1,
    );
    bind_parsed_numeric_dataset_query(parsed, dataset, session, initial, Some(registry), None)
}

fn prepare_source_frontend_phase<'a, D: QueryableDataset<'a>>(
    query: &str,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
    exists_execution: bool,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    if session.context_semantics() != crate::numeric::QueryContextSemantics::SourceContextV1
        || session.literal_semantics() != QueryLiteralSemantics::SourceLiteralV1
    {
        return Err(NumericPrepareError::LiteralConstruction(
            RawNumericFault::InvalidSourceOrigin,
        ));
    }
    let registry = if exists_execution {
        crate::numeric::PreparedSourceLiteralRegistry::new_for_exists_execution(Rc::clone(&session))
    } else {
        crate::numeric::PreparedSourceLiteralRegistry::new(Rc::clone(&session))
    };
    let parsed = SparqlParser::new().parse_query_rdflib760_source_exists_frontend(
        query,
        registry.hooks(),
        session.group_semantics() == QueryGroupSemantics::SourceAggregateV1,
    );
    bind_parsed_numeric_query(parsed, dataset, session, initial, Some(registry))
}

fn bind_parsed_numeric_query<'a, D: QueryableDataset<'a>>(
    parsed: Result<(Query, Option<Vec<Variable>>), NumericQueryParseError>,
    dataset: D,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
    prepared_literals: Option<Rc<crate::numeric::PreparedSourceLiteralRegistry>>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    let dataset = NumericDataset::new_with_literal_semantics(dataset, session.literal_semantics());
    bind_parsed_numeric_dataset_query(parsed, dataset, session, initial, prepared_literals, None)
}

fn bind_parsed_numeric_dataset_query<'a, D: QueryableDataset<'a>>(
    parsed: Result<(Query, Option<Vec<Variable>>), NumericQueryParseError>,
    dataset: NumericDataset<'a, D>,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
    prepared_literals: Option<Rc<crate::numeric::PreparedSourceLiteralRegistry>>,
    initial_values: Option<Vec<Option<crate::numeric::RawExpressionValue>>>,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    bind_parsed_numeric_dataset_query_form(
        parsed,
        dataset,
        session,
        initial,
        prepared_literals,
        initial_values,
        false,
    )
}

fn bind_parsed_numeric_dataset_query_form<'a, D: QueryableDataset<'a>>(
    parsed: Result<(Query, Option<Vec<Variable>>), NumericQueryParseError>,
    dataset: NumericDataset<'a, D>,
    session: Rc<QueryNumericSession>,
    initial: Vec<NumericInitialBinding>,
    prepared_literals: Option<Rc<crate::numeric::PreparedSourceLiteralRegistry>>,
    initial_values: Option<Vec<Option<crate::numeric::RawExpressionValue>>>,
    runtime_result_admitted: bool,
) -> Result<PreparedNumericQuery<'a, D>, NumericPrepareError> {
    let source_context =
        session.context_semantics() == crate::numeric::QueryContextSemantics::SourceContextV1;
    let (query, shadow_targets) = parsed.map_err(|error| match error {
        NumericQueryParseError::Syntax(error) => NumericPrepareError::Syntax(error),
        NumericQueryParseError::SourceSyntax(error) => NumericPrepareError::SourceSyntax(error),
        NumericQueryParseError::LiteralConstruction(error) => {
            match error.downcast::<RawNumericFault>() {
                Ok(error) => NumericPrepareError::LiteralConstruction(*error),
                Err(error) => NumericPrepareError::Query(QueryEvaluationError::Unexpected(error)),
            }
        }
    })?;
    let graph_execution = prepared_literals
        .as_ref()
        .is_some_and(|registry| registry.admits_graph_execution(&session));
    if runtime_result_admitted && (!graph_execution || !source_context) {
        return Err(NumericPrepareError::LiteralConstruction(
            RawNumericFault::InvalidSourceOrigin,
        ));
    }
    use super::source_runtime_result::ResultForm;
    let (pattern, specification, base_iri, result_form) = match query {
        Query::Select {pattern, dataset, base_iri} => (pattern, dataset, base_iri, ResultForm::Select),
        Query::Ask {pattern, dataset, base_iri} if runtime_result_admitted => (pattern, dataset, base_iri, ResultForm::Ask),
        Query::Construct {pattern, dataset, base_iri, template} if runtime_result_admitted => {
            let registry = prepared_literals.as_ref().ok_or(NumericPrepareError::LiteralConstruction(RawNumericFault::InvalidSourceOrigin))?;
            let form = super::source_runtime_result::bind_template(template, registry).map_err(NumericPrepareError::LiteralConstruction)?;
            (pattern, dataset, base_iri, form)
        }
        Query::Describe {..} if runtime_result_admitted => return Err(NumericPrepareError::Query(QueryEvaluationError::Unexpected(
            "SemanticRuntime.query supports SELECT, ASK, and CONSTRUCT; received 'DESCRIBE'. Use update() for SPARQL UPDATE.".into()))),
        _ => return Err(NumericPrepareError::Query(QueryEvaluationError::Unexpected("private numeric receiving currently requires a SELECT query".into()))),
    };
    // This admission executes the owner's verified local default graph. A
    // parsed dataset clause needs separately received dataset semantics; it
    // must not silently select an empty graph or imply external loading.
    if runtime_result_admitted && specification.is_some() {
        return Err(NumericPrepareError::Query(
            QueryEvaluationError::Unexpected("source_query_dataset_clause_not_received".into()),
        ));
    }
    bind_numeric_pattern(pattern, specification, base_iri, dataset, session, initial, prepared_literals, initial_values, shadow_targets, result_form, runtime_result_admitted)
}

/// Bind the exact prepared operation body with the same compiler, registry,
/// context and slot owner used by queries. UPDATE has no SELECT terminal.
pub(super) fn bind_source_update_pattern<'a,D:SourceQueryableDataset<'a>>(
    pattern:spargebra::algebra::GraphPattern,
    base_iri:Option<Iri<String>>,
    dataset:D,
    registry:Rc<crate::numeric::PreparedSourceLiteralRegistry>,
    initial:Vec<NumericInitialBinding>,
    initial_values:Vec<Option<RawExpressionValue>>,
) -> Result<PreparedNumericQuery<'a,D>,NumericPrepareError> {
    let session=registry.session().clone();
    if !registry.admits_graph_execution(&session) || session.context_semantics()!=crate::numeric::QueryContextSemantics::SourceContextV1 {
        return Err(NumericPrepareError::LiteralConstruction(RawNumericFault::InvalidSourceOrigin));
    }
    let dataset=NumericDataset::new_with_source_dataset(dataset,session.clone()).map_err(NumericPrepareError::Query)?;
    bind_numeric_pattern(pattern,None,base_iri,dataset,session,initial,Some(registry),Some(initial_values),None,super::source_runtime_result::ResultForm::Select,false)
}

fn bind_numeric_pattern<'a,D:QueryableDataset<'a>>(
    pattern:spargebra::algebra::GraphPattern,
    specification:Option<spargebra::algebra::QueryDataset>,
    base_iri:Option<Iri<String>>,
    dataset:NumericDataset<'a,D>,
    session:Rc<QueryNumericSession>,
    initial:Vec<NumericInitialBinding>,
    prepared_literals:Option<Rc<crate::numeric::PreparedSourceLiteralRegistry>>,
    initial_values:Option<Vec<Option<RawExpressionValue>>>,
    shadow_targets:Option<Vec<Variable>>,
    result_form:super::source_runtime_result::ResultForm,
    runtime_result_admitted:bool,
) -> Result<PreparedNumericQuery<'a,D>,NumericPrepareError> {
    let source_context=session.context_semantics()==crate::numeric::QueryContextSemantics::SourceContextV1;
    let graph_execution=prepared_literals.as_ref().is_some_and(|registry|registry.admits_graph_execution(&session));
    let admit_initial_cells=source_context && session.literal_semantics()==QueryLiteralSemantics::SourceLiteralV1 && prepared_literals.as_ref().is_some_and(|registry|registry.admits_exists_execution(&session));
    if initial_values.as_ref().is_some_and(|values| {
        values.len() != initial.len()
            || !graph_execution
            || !session.admits_source_calendar_duration()
    }) {
        return Err(NumericPrepareError::LiteralConstruction(
            RawNumericFault::InvalidSourceOrigin,
        ));
    }
    if graph_execution && !dataset.has_source_graph_session(&session) {
        return Err(NumericPrepareError::LiteralConstruction(
            RawNumericFault::InvalidSourceOrigin,
        ));
    }
    let graph_blank_nodes = prepared_literals
        .as_ref()
        .map_or_else(Vec::new, |registry| registry.graph_blank_nodes());
    let query_evaluator = if let Some(value) = dataset.source_cancellation() {
        QueryEvaluator::new().with_cancellation_token(CancellationToken { value })
    } else {
        QueryEvaluator::new()
    }
    .without_optimizations();
    let plan = query_evaluator.plan_graph_pattern(&pattern);
    let mut evaluator = query_evaluator
        .simple_evaluator(
            dataset.clone(),
            specification.map_or_else(QueryDatasetSpecification::new, Into::into),
            &base_iri,
        )
        .map_err(NumericPrepareError::Query)?;
    let internal_dataset = dataset.clone();
    let external_dataset = dataset.clone();
    let graph_dataset = dataset.clone();
    let source_bgp_scan = graph_execution.then(|| {
        let scan: source_bgp::SourceBgpScan<'a, CachedInternalTerm<D::InternalTerm>> =
            Rc::new(move |cells| graph_dataset.source_triples_for_cells(cells));
        scan
    });
    evaluator.dataset.numeric = Some(Rc::new(RawTermCodec {
        session: Rc::clone(&session),
        prepared_literals,
        internalize: Rc::new(move |raw| internal_dataset.internalize_raw(raw)),
        externalize: Rc::new(move |value| external_dataset.externalize_raw(value)),
        source_identity: Rc::new(|value| {
            value
                .source_identity_key()
                .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)))
        }),
        same_transport: CachedInternalTerm::same_transport_instance,
        source_bgp_scan,
    }));
    if graph_execution && dataset.has_source_contexts() {
        evaluator.source_scoped_evaluator=Some(Rc::new(|parent,name| {
            let scoped=parent.dataset.dataset.scoped_source_graph(name)?.ok_or_else(||QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin)))?;
            let old=parent.dataset.numeric.as_ref().ok_or_else(||QueryEvaluationError::Unexpected(Box::new(RawNumericFault::InvalidSourceOrigin)))?;
            let read=scoped.clone();let write=scoped.clone();let scan=scoped.clone();
            let mut child=parent.clone();
            child.dataset.dataset=Rc::new(scoped);
            child.dataset.numeric=Some(Rc::new(RawTermCodec{
                session:old.session.clone(),prepared_literals:old.prepared_literals.clone(),
                internalize:Rc::new(move |raw|write.internalize_raw(raw)),
                externalize:Rc::new(move |value|read.externalize_raw(value)),
                source_identity:old.source_identity.clone(),same_transport:old.same_transport,
                source_bgp_scan:Some(Rc::new(move |cells|scan.source_triples_for_cells(cells))),
            }));
            Ok(child)
        }));
    }
    evaluator.nullable_expressions = true;
    evaluator.initial_shadow_targets =
        Rc::new(shadow_targets.unwrap_or_default().into_iter().collect());
    let initial_order: Rc<[Variable]> = initial
        .iter()
        .map(|binding| binding.variable.clone())
        .collect::<Vec<_>>()
        .into();
    let mut values = FxHashMap::default();
    for (initial_index, binding) in initial.into_iter().enumerate() {
        let value = match binding.value {
            NumericInitialValue::SourceNone if admit_initial_cells => SolutionValue::ExplicitNone,
            NumericInitialValue::SourceError(error)
                if admit_initial_cells
                    && error.source_exception().is_some_and(|source| {
                        use crate::tuple::{SourceExceptionClass, SparqlErrorKind};
                        matches!(
                            (source.class, error.kind()),
                            (SourceExceptionClass::NotBound, SparqlErrorKind::NotBound)
                                | (SourceExceptionClass::SparqlType, SparqlErrorKind::Type)
                                | (SourceExceptionClass::Sparql, SparqlErrorKind::OtherSparql)
                        )
                    }) =>
            {
                SolutionValue::ReturnedError(error)
            }
            NumericInitialValue::SourceNone | NumericInitialValue::SourceError(_) => {
                return Err(NumericPrepareError::LiteralConstruction(
                    RawNumericFault::InvalidSourceOrigin,
                ));
            }
            rdf => {
                let raw = if let Some(raw) = initial_values
                    .as_ref()
                    .and_then(|values| values[initial_index].clone())
                {
                    Ok(raw)
                } else if graph_execution {
                    session.construct_source_graph_initial(rdf)
                } else {
                    session.construct_initial(rdf)
                }
                .map_err(NumericPrepareError::LiteralConstruction)?;
                SolutionValue::Rdf(
                    dataset
                        .internalize_raw(raw)
                        .map_err(NumericPrepareError::Query)?,
                )
            }
        };
        if values.insert(binding.variable, value).is_some() {
            return Err(NumericPrepareError::LiteralConstruction(
                RawNumericFault::InvalidSourceOrigin,
            ));
        }
    }
    let values = Rc::new(values);
    // The full cell map is the initial-value authority. Legacy RDF-only
    // consumers receive its immutable RDF view, never a replacement owner.
    evaluator.initial_bindings = Rc::new(
        values
            .iter()
            .filter_map(|(variable, cell)| cell.rdf().cloned().map(|rdf| (variable.clone(), rdf)))
            .collect(),
    );
    let initial = PreparedNumericInitial {
        values,
        source_order: source_context.then_some(Rc::clone(&initial_order)),
    };
    if source_context {
        let mut variables = source_context::source_query_variables(&plan, &initial_order);
        let mut blank_slots = Vec::new();
        // Aliases are storage handles only. The runtime separately retains
        // each actual BNode key for lookup, equality-domain separation and
        // native hash; no user Variable can acquire a BNode's slot.
        for node in graph_blank_nodes {
            let mut name = format!("__oe_source_bgp_blank_{}", blank_slots.len());
            let alias = loop {
                let alias = Variable::new(&name).map_err(|_| {
                    NumericPrepareError::LiteralConstruction(RawNumericFault::InvalidSourceOrigin)
                })?;
                if !variables.contains(&alias) {
                    break alias;
                }
                name.push('_');
            };
            let slot = variables.len();
            variables.push(alias);
            blank_slots.push((slot, node));
        }
        let variables: Rc<[Variable]> = variables.into();
        let bindings = initial
            .encode_for(&variables)
            .map_err(NumericPrepareError::Query)?;
        let hash =
            session
                .source_hash()
                .cloned()
                .ok_or(NumericPrepareError::LiteralConstruction(
                    RawNumericFault::InvalidSourceOrigin,
                ))?;
        let rdf_hash = Rc::clone(&hash);
        evaluator.source_context = Some(Rc::new(
            source_context_join::SourceContextRuntime::new(
                variables,
                Rc::new(bindings),
                evaluator.dataset.cancellation_token.clone(),
                hash,
                Rc::new(|left, right| match (left, right) {
                    (SolutionValue::Rdf(left), SolutionValue::Rdf(right)) => left
                        .source_context_equal(right)
                        .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error))),
                    _ => Ok(left == right),
                }),
                Rc::new(move |value: &CachedInternalTerm<D::InternalTerm>| {
                    value
                        .source_raw()
                        .and_then(|raw| rdf_hash.term_hash(raw))
                        .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)))
                }),
            )
            .with_graph_blank_nodes(blank_slots),
        ));
    }
    Ok(PreparedNumericQuery {
        plan,
        evaluator,
        initial,
        result_form,
        runtime_result_admitted,
    })
}

/// Source-aware terminal decoder for the actual tuple stream. The outer fuse
/// covers input errors, invalid solution domains, cancellation, and backend
/// externalization faults alike; it prevents any later poll or conversion.
pub(super) fn decode_source_presence_bindings<'a, T: 'a>(
    codec: Rc<RawTermCodec<'a, T>>,
    cancellation: CancellationToken,
    mut input: InternalTuplesIterator<'a, T>,
    variables: Arc<[Variable]>,
) -> impl Iterator<Item = Result<SourcePresenceSolution, QueryEvaluationError>> + 'a {
    nullable::halt_after_error(std::iter::from_fn(move || {
        if let Err(error) = cancellation.ensure_alive() {
            return Some(Err(error));
        }
        let tuple = input.next()?;
        Some((|| {
            let tuple = tuple?;
            let mut values = vec![None; variables.len()];
            for (index, value) in tuple.into_cells().enumerate() {
                if let Some(value) = value {
                    // Reject a present cell outside the declared domain before
                    // performing an external conversion for that cell.
                    let output = values
                        .get_mut(index)
                        .ok_or(QueryEvaluationError::InvalidSolutionDomain(index))?;
                    cancellation.ensure_alive()?;
                    let value = match value {
                        SolutionValue::Rdf(value) => {
                            SolutionValue::Rdf((codec.externalize)(value)?)
                        }
                        SolutionValue::ExplicitNone => SolutionValue::ExplicitNone,
                        SolutionValue::ReturnedError(error) => SolutionValue::ReturnedError(error),
                    };
                    cancellation.ensure_alive()?;
                    *output = Some(value);
                }
            }
            Ok(SourcePresenceSolution {
                variables: Arc::clone(&variables),
                values,
            })
        })())
    }))
}

#[cfg(test)]
mod source_decoder_tests {
    use super::*;
    use crate::numeric::{LiteralSourceState, NumericCacheState, QueryNumericCapabilities};
    use crate::tuple::ErrorValue;
    use semantica_numeric::{DecimalContext, NumericLimits};

    fn codec(calls: Rc<Cell<usize>>) -> Rc<RawTermCodec<'static, RawExpressionValue>> {
        Rc::new(RawTermCodec {
            prepared_literals: None,
            source_bgp_scan: None,
            session: Rc::new(
                QueryNumericSession::new_with_literal_semantics(
                    DecimalContext::default(),
                    NumericLimits::default(),
                    None,
                    true,
                    QueryNumericCapabilities::MixedBinary64V1,
                    QueryGroupSemantics::SourceAggregateV1,
                    QueryLiteralSemantics::SourceLiteralV1,
                )
                .unwrap(),
            ),
            internalize: Rc::new(Ok),
            externalize: Rc::new(move |raw| {
                calls.set(calls.get() + 1);
                Ok(raw)
            }),
            source_identity: Rc::new(|raw| {
                crate::numeric::source_identity_key(raw)
                    .map_err(|error| QueryEvaluationError::Unexpected(Box::new(error)))
            }),
            same_transport: |left, right| std::ptr::eq(left, right),
        })
    }
    #[test]
    fn source_terminal_decoder_preserves_cells_and_stops_after_input_error() {
        // This is a decoder-only author control. Actual Dataset fault identity
        // and actual bound queries are received in nullable_literal_receiving.
        let conversions = Rc::new(Cell::new(0));
        let codec = codec(Rc::clone(&conversions));
        let known = codec
            .session
            .construct_source_direct_string("a", None, Some("EN".into()), true)
            .unwrap();
        let unknown = RawExpressionValue::from_import(Literal::new_simple_literal("a").into());
        let returned = ErrorValue::new("same instance through final decoder");
        let mut tuple = InternalTuple::with_capacity(5);
        tuple.set(0, known.clone());
        tuple.set(1, unknown.clone());
        tuple.set_cell(2, SolutionValue::ExplicitNone);
        tuple.set_cell(3, SolutionValue::ReturnedError(returned.clone()));
        let late = tuple.clone();
        let polls = Rc::new(Cell::new(0));
        let counted = Rc::clone(&polls);
        let input = Box::new(
            vec![
                Ok(tuple),
                Err(QueryEvaluationError::InvalidSolutionDomain(99)),
                Ok(late),
            ]
            .into_iter()
            .inspect(move |_| counted.set(counted.get() + 1)),
        );
        let variables: Arc<[Variable]> = ["known", "unknown", "none", "error", "absent"]
            .map(Variable::new_unchecked)
            .into();
        let mut rows = decode_source_presence_bindings(
            codec,
            CancellationToken::new(),
            input,
            Arc::clone(&variables),
        );
        let row = rows.next().unwrap().unwrap();
        assert!(Arc::ptr_eq(&row.variables, &variables));
        let Some(SolutionValue::Rdf(actual_known)) = &row.values[0] else {
            panic!("known RDF cell")
        };
        assert_eq!(actual_known.literal_source(), known.literal_source());
        assert!(matches!(
            actual_known.numeric(),
            NumericCacheState::NotNumeric
        ));
        let Some(SolutionValue::Rdf(actual_unknown)) = &row.values[1] else {
            panic!("unknown RDF cell")
        };
        assert_eq!(actual_unknown.literal_source(), unknown.literal_source());
        assert!(matches!(
            actual_unknown.literal_source(),
            LiteralSourceState::Literal(_)
        ));
        assert!(matches!(&row.values[2], Some(SolutionValue::ExplicitNone)));
        let Some(SolutionValue::ReturnedError(actual_error)) = &row.values[3] else {
            panic!("returned error cell")
        };
        assert_eq!(
            actual_error, &returned,
            "returned ErrorValue instance preserved"
        );
        assert!(row.values[4].is_none());
        assert_eq!(conversions.get(), 2);
        assert!(matches!(
            rows.next(),
            Some(Err(QueryEvaluationError::InvalidSolutionDomain(99)))
        ));
        assert!(rows.next().is_none());
        assert!(rows.next().is_none());
        assert_eq!(polls.get(), 2, "the third input row must never be polled");
        assert_eq!(conversions.get(), 2);
    }
    #[test]
    fn source_terminal_decoder_checks_domain_and_cancellation_before_conversion() {
        for cancelled in [false, true] {
            let conversions = Rc::new(Cell::new(0));
            let codec = codec(Rc::clone(&conversions));
            let token = CancellationToken::new();
            if cancelled {
                token.cancel();
            }
            let mut tuple = InternalTuple::with_capacity(1);
            tuple.set(
                0,
                RawExpressionValue::from_import(Literal::new_simple_literal("outside").into()),
            );
            let polls = Rc::new(Cell::new(0));
            let counted = Rc::clone(&polls);
            let input = Box::new(std::iter::from_fn(move || {
                counted.set(counted.get() + 1);
                Some(Ok(tuple.clone()))
            }));
            let mut rows = decode_source_presence_bindings(codec, token, input, Arc::from([]));
            if cancelled {
                assert!(matches!(
                    rows.next(),
                    Some(Err(QueryEvaluationError::Cancelled))
                ));
                assert_eq!(polls.get(), 0);
            } else {
                assert!(matches!(
                    rows.next(),
                    Some(Err(QueryEvaluationError::InvalidSolutionDomain(0)))
                ));
                assert_eq!(polls.get(), 1);
            }
            assert_eq!(conversions.get(), 0);
            assert!(rows.next().is_none());
            assert!(rows.next().is_none());
            assert_eq!(polls.get(), usize::from(!cancelled));
        }
    }
}
