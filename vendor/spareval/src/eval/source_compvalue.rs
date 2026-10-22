//! parserutils.value's generic Exception is fatal, outside the Expr SPARQL
//! catch. Format the actual ordered mapping and prepared literal occurrences;
//! instance attributes and frozen test answers are never mapping children.
use super::*;
use crate::numeric::PreparedSourceLiteralRegistry;
use semantica_numeric::NumericFault;
use spargebra::{SourceSyntaxNode, SourceSyntaxSnapshot, SourceSyntaxValue};

#[derive(Debug)]
pub(super) struct SourceCompValueError {
    pub(super) module: &'static str,
    pub(super) class: &'static str,
    pub(super) argument: String,
}
impl fmt::Display for SourceCompValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.argument)
    }
}
impl std::error::Error for SourceCompValueError {}

pub(super) fn source_compvalue_error(
    syntax: &SourceSyntaxSnapshot,
    session: &Rc<QueryNumericSession>,
    registry: &PreparedSourceLiteralRegistry,
    check_cancel: &mut impl FnMut() -> Result<(), QueryEvaluationError>,
) -> Result<SourceCompValueError, QueryEvaluationError> {
    let graph = untranslated_graph_node(syntax)?;
    let text = render_mapping(syntax.nodes(), graph, true, session, registry, check_cancel)?;
    let mut argument = String::new();
    append(&mut argument, "What do I do with this CompValue? ")?;
    append(&mut argument, &text)?;
    Ok(SourceCompValueError {
        module: "builtins",
        class: "Exception",
        argument,
    })
}

pub(super) fn untranslated_graph_node(
    syntax: &SourceSyntaxSnapshot,
) -> Result<usize, QueryEvaluationError> {
    let root = syntax
        .root_node()
        .ok_or_else(|| raw_failure(RawNumericFault::InvalidSourceOrigin))?;
    if !matches!(root.name.as_str(), "Builtin_EXISTS" | "Builtin_NOTEXISTS") {
        return Err(raw_failure(RawNumericFault::InvalidSourceOrigin));
    }
    // Expr.__getattr__('graph') reads the ordered mapping, then CompValue
    // __getitem__/_value/parserutils.value rejects that child. The outer Expr
    // and instance-only translated graph are different objects/domains.
    match root.fields.iter().find(|(key, _)| key == "graph") {
        Some((_, SourceSyntaxValue::Node(graph))) if syntax.nodes().get(*graph).is_some() => {
            Ok(*graph)
        }
        _ => Err(raw_failure(RawNumericFault::InvalidSourceOrigin)),
    }
}

fn raw_failure(error: RawNumericFault) -> QueryEvaluationError {
    QueryEvaluationError::Unexpected(Box::new(error))
}
pub(super) fn reserve_syntax_text(
    text: &mut String,
    additional: usize,
) -> Result<(), QueryEvaluationError> {
    text.try_reserve(additional).map_err(|_| {
        raw_failure(
            NumericFault::ResourceLimit {
                operation: "source_compvalue_text",
            }
            .into(),
        )
    })
}
fn append(text: &mut String, part: &str) -> Result<(), QueryEvaluationError> {
    reserve_syntax_text(text, part.len())?;
    text.push_str(part);
    Ok(())
}
fn quote(text: &str) -> Result<String, QueryEvaluationError> {
    semantica_json::python_repr(&serde_json::Value::String(text.to_owned()))
        .map_err(|_| raw_failure(RawNumericFault::InvalidSourceOrigin))
}

pub(super) fn render_mapping(
    nodes: &[SourceSyntaxNode],
    root: usize,
    str_mode: bool,
    session: &Rc<QueryNumericSession>,
    registry: &PreparedSourceLiteralRegistry,
    check_cancel: &mut impl FnMut() -> Result<(), QueryEvaluationError>,
) -> Result<String, QueryEvaluationError> {
    struct Renderer<'a, C> {
        nodes: &'a [SourceSyntaxNode],
        session: &'a QueryNumericSession,
        registry: &'a PreparedSourceLiteralRegistry,
        active: Vec<usize>,
        text: String,
        check_cancel: &'a mut C,
    }
    impl<C: FnMut() -> Result<(), QueryEvaluationError>> Renderer<'_, C> {
        fn push(&mut self, text: &str) -> Result<(), QueryEvaluationError> {
            append(&mut self.text, text)
        }
        fn variable(&mut self, variable: &Variable) -> Result<(), QueryEvaluationError> {
            (self.check_cancel)()?;
            self.push("rdflib.term.Variable(")?;
            self.push(&quote(variable.as_str())?)?;
            self.push(")")
        }
        fn node(
            &mut self,
            id: usize,
            str_mode: bool,
            depth: usize,
        ) -> Result<(), QueryEvaluationError> {
            (self.check_cancel)()?;
            // Parsed mapping fields are acyclic. Repeated aliases are valid,
            // but a fabricated cycle or excessive recursion grants no source
            // repr qualification. This guard is not a Literal output budget.
            if self.active.contains(&id) {
                return Err(raw_failure(RawNumericFault::InvalidSourceOrigin));
            }
            if depth >= 256 {
                return Err(raw_failure(
                    NumericFault::ResourceLimit {
                        operation: "source_compvalue_depth",
                    }
                    .into(),
                ));
            }
            let node = self
                .nodes
                .get(id)
                .ok_or_else(|| raw_failure(RawNumericFault::InvalidSourceOrigin))?;
            self.active.try_reserve(1).map_err(|_| {
                raw_failure(
                    NumericFault::ResourceLimit {
                        operation: "source_compvalue_text",
                    }
                    .into(),
                )
            })?;
            self.active.push(id);
            self.push(&node.name)?;
            self.push("_")?;
            // OrderedDict.__str__ delegates to the overridden CompValue repr.
            // Nested mapping values use repr and therefore have one name.
            if str_mode {
                self.push(&node.name)?;
                self.push("_")?;
            }
            self.push("{")?;
            for (ordinal, (key, value)) in node.fields.iter().enumerate() {
                if ordinal != 0 {
                    self.push(", ")?;
                }
                self.push(&quote(key)?)?;
                self.push(": ")?;
                self.value(value, depth + 1)?;
            }
            self.push("}")?;
            self.active.pop();
            Ok(())
        }
        fn value(
            &mut self,
            value: &SourceSyntaxValue,
            depth: usize,
        ) -> Result<(), QueryEvaluationError> {
            use SourceSyntaxValue as V;
            (self.check_cancel)()?;
            if depth >= 256 {
                return Err(raw_failure(
                    NumericFault::ResourceLimit {
                        operation: "source_compvalue_depth",
                    }
                    .into(),
                ));
            }
            match value {
                V::Node(id) => self.node(*id, false, depth),
                V::List(values) => {
                    self.push("[")?;
                    for (ordinal, value) in values.iter().enumerate() {
                        if ordinal != 0 {
                            self.push(", ")?;
                        }
                        self.value(value, depth + 1)?;
                    }
                    self.push("]")
                }
                V::Variable(variable) => self.variable(variable),
                V::VariableSet(variables) => {
                    if variables.is_empty() {
                        return self.push("set()");
                    }
                    self.push("{")?;
                    // This is the actual same-owner set program's physical
                    // table iteration, never the sorted scope/member view.
                    for (ordinal, variable) in variables.iter().enumerate() {
                        if ordinal != 0 {
                            self.push(", ")?;
                        }
                        self.variable(variable)?;
                    }
                    self.push("}")
                }
                V::Literal(literal) => {
                    let value = self.registry.resolve(literal).map_err(raw_failure)?;
                    let text = self
                        .session
                        .source_repr(&SolutionValue::Rdf(value))
                        .map_err(raw_failure)?;
                    self.push(&text)
                }
                V::NamedNode(node) => {
                    let value = RawExpressionValue::from_import(node.clone().into());
                    let text = self
                        .session
                        .source_repr(&SolutionValue::Rdf(value))
                        .map_err(raw_failure)?;
                    self.push(&text)
                }
                V::BlankNode(node) => {
                    let value = RawExpressionValue::from_import(node.clone().into());
                    let text = self
                        .session
                        .source_repr(&SolutionValue::Rdf(value))
                        .map_err(raw_failure)?;
                    self.push(&text)
                }
                V::NamedNodePattern(NamedNodePattern::NamedNode(node)) => {
                    self.value(&V::NamedNode(node.clone()), depth + 1)
                }
                V::NamedNodePattern(NamedNodePattern::Variable(variable)) => {
                    self.variable(variable)
                }
                V::Text(text) => self.push(&quote(text)?),
                V::Boolean(value) => self.push(if *value { "True" } else { "False" }),
                V::Unsigned(value) => self.push(&value.to_string()),
                V::None => self.push("None"),
                V::ValueRecords { records, .. } => {
                    self.push("[")?;
                    for (row_at, record) in records.iter().enumerate() {
                        (self.check_cancel)()?;
                        if row_at != 0 {
                            self.push(", ")?;
                        }
                        self.push("{")?;
                        for (key_at, (key, value)) in record.iter().enumerate() {
                            if key_at != 0 {
                                self.push(", ")?;
                            }
                            self.variable(key)?;
                            self.push(": ")?;
                            match value {
                                Some(spargebra::SourceValueTerm::Literal(literal)) => {
                                    self.value(&V::Literal(literal.clone()), depth + 1)?
                                }
                                Some(spargebra::SourceValueTerm::NamedNode(node)) => {
                                    self.value(&V::NamedNode(node.clone()), depth + 1)?
                                }
                                None => self.value(&V::Text("UNDEF".into()), depth + 1)?,
                            }
                        }
                        self.push("}")?;
                    }
                    self.push("]")
                }
                // Triples/path/function/native Values containers still need
                // their own source grammar and representation receiving.
                _ => Err(raw_failure(
                    RawNumericFault::UnsupportedSourceLiteralOperator(
                        "source CompValue syntax representation",
                    ),
                )),
            }
        }
    }
    if !registry.admits_exists_execution(session) {
        return Err(raw_failure(RawNumericFault::InvalidSourceOrigin));
    }
    let mut renderer = Renderer {
        nodes,
        session,
        registry,
        active: Vec::new(),
        text: String::new(),
        check_cancel,
    };
    renderer.node(root, str_mode, 0)?;
    Ok(renderer.text)
}
