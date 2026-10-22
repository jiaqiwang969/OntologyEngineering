//! Source-only reductions of the existing PEG grammar. No query evaluation.
use super::*;
use crate::{
    SourceExistsExpression, SourceExistsPolarity, SourceSyntaxAttributes, SourceSyntaxNode,
    SourceSyntaxSnapshot, SourceSyntaxValue as V,
};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone)]
pub(super) enum ParsedPattern {
    Legacy(GraphPattern),
    Source(usize),
}
impl Default for ParsedPattern {
    fn default() -> Self {
        Self::Legacy(GraphPattern::default())
    }
}
impl From<GraphPattern> for ParsedPattern {
    fn from(value: GraphPattern) -> Self {
        Self::Legacy(value)
    }
}
impl ParsedPattern {
    pub(super) fn legacy(self) -> Result<GraphPattern, &'static str> {
        match self {
            Self::Legacy(value) => Ok(value),
            Self::Source(_) => Err("Source syntax requires its source frontend phases"),
        }
    }
}

#[derive(Clone)]
pub(super) struct ParsedAggregate {
    pub(super) aggregate: AggregateExpression,
    pub(super) separator: Option<ParsedSourceLiteral>,
}
impl From<AggregateExpression> for ParsedAggregate {
    fn from(aggregate: AggregateExpression) -> Self {
        Self {
            aggregate,
            separator: None,
        }
    }
}
pub(super) struct ParsedHavingClause(pub(super) Vec<Expression>);
impl ParsedHavingClause {
    pub(super) fn legacy(self) -> Result<Expression, &'static str> {
        not_empty_fold(self.0.into_iter(), |a, b| {
            Expression::And(Box::new(a), Box::new(b))
        })
    }
}
pub(super) struct ParsedOrderCondition {
    pub(super) expression: Expression,
    pub(super) order: Option<&'static str>,
}
impl ParsedOrderCondition {
    pub(super) fn legacy(self) -> OrderExpression {
        if self.order == Some("DESC") {
            OrderExpression::Desc(self.expression)
        } else {
            OrderExpression::Asc(self.expression)
        }
    }
}
pub(super) struct ParsedLimitValue {
    pub(super) value: usize,
    pub(super) literal: Option<ParsedSourceLiteral>,
}
pub(super) struct ParsedLimitOffset {
    pub(super) offset: Option<ParsedLimitValue>,
    pub(super) limit: Option<ParsedLimitValue>,
    pub(super) limit_first: bool,
}
impl ParsedLimitOffset {
    pub(super) fn legacy(self) -> (usize, Option<usize>) {
        (
            self.offset.map_or(0, |v| v.value),
            self.limit.map(|v| v.value),
        )
    }
}

#[derive(Default)]
pub(super) struct SourceArena {
    nodes: Vec<SourceSyntaxNode>,
    handles: HashMap<Variable, ParsedAggregate>,
    functions: HashMap<usize, Function>,
    aggregate_functions: HashMap<usize, AggregateFunction>,
    group_sources: HashMap<usize, (usize, Vec<V>, Vec<Option<usize>>)>,
    /// Physical leaves of the very same captured TriplesBlock reduction. This
    /// is temporary parse-to-plan metadata; it owns no graph facts or runtime
    /// expression interpreter, and is discarded after query translation.
    parsed_triples: HashMap<usize, Vec<TriplePattern>>,
    parsed_mixed: HashMap<usize, Vec<TripleOrPathPattern>>,
    source_graph_bgp: bool,
}
impl SourceArena {
    fn node(&mut self, name: &str, fields: Vec<(&str, V)>) -> usize {
        let id = self.nodes.len();
        self.nodes.push(SourceSyntaxNode {
            name: name.to_owned(),
            fields: fields
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
            attributes: SourceSyntaxAttributes::default(),
        });
        id
    }
    pub(super) fn path_node(&mut self, name: &str, fields: Vec<(&str, V)>) -> V {
        V::Node(self.node(name, fields))
    }

    pub(super) fn triples_block(&mut self, patterns: Vec<TripleOrPathPattern>) -> ParsedPattern {
        // Nested collections/reifiers and true path operators require their
        // own original container receiving. Never invent their parsed rows
        // from the expanded graph, while retaining their existing plan path.
        let captured = !patterns.is_empty()
            && patterns.iter().all(|pattern| match pattern {
                TripleOrPathPattern::SourceRow { physical, .. } => physical.iter().all(|pattern| {
                    matches!(
                        pattern,
                        TripleOrPathPattern::Triple(_) | TripleOrPathPattern::Path { .. }
                    )
                }),
                _ => false,
            });
        if !captured {
            return ParsedPattern::Source(self.leaf(build_bgp(patterns)));
        }
        let mut rows = Vec::new();
        let mut triples = Vec::new();
        let mut mixed = Vec::new();
        for pattern in patterns {
            let TripleOrPathPattern::SourceRow { terms, physical } = pattern else {
                unreachable!()
            };
            rows.push(V::List(terms));
            for pattern in physical {
                if let TripleOrPathPattern::Triple(triple) = &pattern {
                    triples.push(triple.clone());
                }
                mixed.push(pattern);
            }
        }
        let id = self.node("TriplesBlock", vec![("triples", V::List(rows))]);
        self.parsed_triples.insert(id, triples);
        self.parsed_mixed.insert(id, mixed);
        ParsedPattern::Source(id)
    }

    /// Source translatePath is a postorder mapping/list walk over WHERE only.
    /// It does not visit projection expressions or instance attributes.
    fn translate_native_paths(
        &mut self,
        value: V,
        depth: usize,
    ) -> Result<V, crate::SourceLiteralBridgeError> {
        use crate::{
            SourcePathDefinition as P, SourcePathNegatedArgument as A, SourcePathRepeat as M,
            SourceTriplePattern, SourceTriplePredicate,
        };
        if depth >= 256 {
            return Err("Source path translation depth exceeded".into());
        }
        let as_path = |value: V| -> Result<P, crate::SourceLiteralBridgeError> {
            match value {
                V::NamedNode(v) => Ok(P::NamedNode(v)),
                V::NativePath(v) => Ok(v),
                _ => Err("Source path child was not translated".into()),
            }
        };
        Ok(match value {
            V::List(values) => V::List(
                values
                    .into_iter()
                    .map(|v| self.translate_native_paths(v, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            V::Node(id) => {
                for at in 0..self.nodes[id].fields.len() {
                    let child = self.nodes[id].fields[at].1.clone();
                    self.nodes[id].fields[at].1 = self.translate_native_paths(child, depth + 1)?;
                }
                match self.nodes[id].name.as_str() {
                    "PathAlternative" | "PathSequence" => {
                        let V::List(parts) = self.field(id, "part") else {
                            return Err("Source path parts absent".into());
                        };
                        if parts.len() == 1 {
                            parts.into_iter().next().unwrap()
                        } else {
                            let parts = parts
                                .into_iter()
                                .map(as_path)
                                .collect::<Result<Vec<_>, _>>()?;
                            V::NativePath(if self.nodes[id].name == "PathSequence" {
                                P::Sequence(parts)
                            } else {
                                P::Alternative(parts)
                            })
                        }
                    }
                    "PathElt" => {
                        let part = self.field(id, "part");
                        match self.field(id, "mod") {
                            V::None => part,
                            V::Text(modifier) => V::NativePath(P::Repeat {
                                inner: Box::new(as_path(part)?),
                                mode: match modifier.as_str() {
                                    "?" => M::ZeroOrOne,
                                    "*" => M::ZeroOrMore,
                                    "+" => M::OneOrMore,
                                    _ => return Err("Source path modifier invalid".into()),
                                },
                            }),
                            _ => return Err("Source path modifier invalid".into()),
                        }
                    }
                    "PathEltOrInverse" => {
                        V::NativePath(P::Inverse(Box::new(as_path(self.field(id, "part"))?)))
                    }
                    "PathNegatedPropertySet" => {
                        let V::List(parts) = self.field(id, "part") else {
                            return Err("Source negated parts absent".into());
                        };
                        let mut args = Vec::new();
                        for part in parts {
                            args.push(match part {
                                V::NamedNode(node) => A::NamedNode(node),
                                V::Node(root) if self.nodes[root].name == "InversePath" => {
                                    A::Untranslated(SourceSyntaxSnapshot::finalized(
                                        Arc::from(self.nodes.clone()),
                                        root,
                                    ))
                                }
                                _ => return Err("Source negated argument invalid".into()),
                            });
                        }
                        V::NativePath(P::Negated(args))
                    }
                    "TriplesBlock" => {
                        let V::List(rows) = self.field(id, "triples") else {
                            return Err("Source native triples lack parsed rows".into());
                        };
                        let terms = Self::group_triple_rows(&rows)?;
                        let physical = self
                            .parsed_mixed
                            .remove(&id)
                            .ok_or("Source native triples lack same-parse physical operands")?;
                        if terms.len() != physical.len() {
                            return Err(
                                "Source native row length differs from physical reduction".into()
                            );
                        }
                        let mut triples = Vec::new();
                        for (row, physical) in terms.iter().zip(physical) {
                            let (subject, predicate, object) = match physical {
                                TripleOrPathPattern::Triple(t) => {
                                    (t.subject, SourceTriplePredicate::Rdf(t.predicate), t.object)
                                }
                                TripleOrPathPattern::Path {
                                    subject,
                                    path,
                                    object,
                                } => {
                                    let V::NativePath(definition) = &row[1] else {
                                        return Err(
                                            "Source path reduction lacks native predicate".into()
                                        );
                                    };
                                    (
                                        subject,
                                        SourceTriplePredicate::Path {
                                            definition: definition.clone(),
                                            physical: path,
                                        },
                                        object,
                                    )
                                }
                                _ => return Err("Nested source row was not reduced".into()),
                            };
                            if ParserState::source_graph_term(&subject).as_ref() != Some(&row[0])
                                || ParserState::source_graph_term(&object).as_ref() != Some(&row[2])
                            {
                                return Err("Same-parse source endpoints drifted".into());
                            }
                            triples.push(SourceTriplePattern {
                                subject,
                                predicate,
                                object,
                            });
                        }
                        self.put(id, "triples", V::NativeTriples(triples));
                        V::Node(id)
                    }
                    _ => V::Node(id),
                }
            }
            value => value,
        })
    }
    fn group_triple_rows(rows: &[V]) -> Result<Vec<[V; 3]>, crate::SourceLiteralBridgeError> {
        let mut borrowed = Vec::new();
        borrowed
            .try_reserve_exact(rows.len())
            .map_err(|_| crate::SourceTripleRowsError::ResourceLimit)?;
        for row in rows {
            let V::List(terms) = row else {
                return Err("Source parsed triple row invalid".into());
            };
            borrowed.push(crate::SourceTripleRow::List(terms.as_slice()));
        }
        Ok(crate::source_group_triple_rows(&borrowed)?)
    }

    fn field(&self, id: usize, key: &str) -> V {
        self.nodes[id]
            .fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or(V::None)
    }
    /// Query/projection dot reads consult the instance assignment before the
    /// mapping fallback. Generic source traversals use fields directly and
    /// must not visit these overrides.
    fn property(&self, id: usize, key: &str) -> V {
        let attributes = &self.nodes[id].attributes;
        let value = match key {
            "expr" => attributes.expr.as_ref(),
            "having" => attributes.having.as_ref(),
            "orderby" => attributes.orderby.as_ref(),
            "where" => attributes.where_clause.as_ref(),
            _ => None,
        };
        value.cloned().unwrap_or_else(|| self.field(id, key))
    }
    fn put(&mut self, id: usize, key: &str, value: V) {
        if let Some((_, old)) = self.nodes[id]
            .fields
            .iter_mut()
            .find(|(name, _)| name == key)
        {
            *old = value;
        } else {
            self.nodes[id].fields.push((key.to_owned(), value));
        }
    }
    pub(super) fn register_aggregate(&mut self, variable: Variable, aggregate: ParsedAggregate) {
        self.handles.insert(variable, aggregate);
    }
    fn aggregate(&mut self, parsed: ParsedAggregate) -> V {
        let ParsedAggregate {
            aggregate,
            separator,
        } = parsed;
        match aggregate {
            AggregateExpression::CountSolutions { distinct } => V::Node(self.node(
                "Aggregate_Count",
                vec![
                    (
                        "distinct",
                        if distinct {
                            V::Text("DISTINCT".into())
                        } else {
                            V::List(Vec::new())
                        },
                    ),
                    ("vars", V::Text("*".into())),
                ],
            )),
            AggregateExpression::FunctionCall {
                name,
                expr,
                distinct,
            } => {
                let expression = self.expression(expr);
                let label = match &name {
                    AggregateFunction::Count => "Aggregate_Count",
                    AggregateFunction::Sum => "Aggregate_Sum",
                    AggregateFunction::Avg => "Aggregate_Avg",
                    AggregateFunction::Min => "Aggregate_Min",
                    AggregateFunction::Max => "Aggregate_Max",
                    AggregateFunction::Sample => "Aggregate_Sample",
                    AggregateFunction::GroupConcat { .. } => "Aggregate_GroupConcat",
                    AggregateFunction::Custom(_) => "Aggregate_Custom",
                };
                let mut fields = vec![
                    (
                        "distinct",
                        if distinct {
                            V::Text("DISTINCT".into())
                        } else {
                            V::List(Vec::new())
                        },
                    ),
                    ("vars", expression),
                ];
                if let Some(separator) = separator {
                    fields.push(("separator", V::Literal(separator)));
                }
                let id = self.node(label, fields);
                self.aggregate_functions.insert(id, name);
                V::Node(id)
            }
        }
    }
    fn expression(&mut self, expression: Expression) -> V {
        let (name, fields) = match expression {
            Expression::SourceLiteral(value) => return V::Literal(value),
            Expression::Variable(value) => {
                return if let Some(aggregate) = self.handles.get(&value).cloned() {
                    self.aggregate(aggregate)
                } else {
                    V::Variable(value)
                };
            }
            Expression::NamedNode(value) => return V::NamedNode(value),
            Expression::Literal(value) => {
                return V::Literal(ParsedSourceLiteral {
                    rdf: value,
                    datatype: SourceDatatype::Unknown,
                    language_original: None,
                    origin: SourceLiteralOrigin::QueryPlain,
                    occurrence: None,
                });
            }
            Expression::SourceFunction { syntax, .. } => {
                assert!(
                    syntax.is_captured(),
                    "finalized Function cannot reenter capture"
                );
                return V::Node(syntax.root());
            }
            Expression::SourceSyntax(value) => {
                return V::Node(value.root());
            }
            Expression::SourceExists(value) => {
                assert!(value.syntax.is_captured());
                return V::Node(value.syntax.root());
            }
            Expression::Exists(_) => unreachable!("legacy EXISTS cannot enter the source grammar"),
            Expression::Bound(value) => ("Builtin_BOUND", vec![("arg", V::Variable(value))]),
            Expression::UnaryPlus(value) => ("UnaryPlus", vec![("expr", self.expression(*value))]),
            Expression::UnaryMinus(value) => {
                ("UnaryMinus", vec![("expr", self.expression(*value))])
            }
            Expression::Not(value) => ("UnaryNot", vec![("expr", self.expression(*value))]),
            Expression::Or(a, b) => (
                "ConditionalOrExpression",
                vec![
                    ("expr", self.expression(*a)),
                    ("other", V::List(vec![self.expression(*b)])),
                ],
            ),
            Expression::And(a, b) => (
                "ConditionalAndExpression",
                vec![
                    ("expr", self.expression(*a)),
                    ("other", V::List(vec![self.expression(*b)])),
                ],
            ),
            Expression::Equal(a, b) => return self.binary("RelationalExpression", "=", *a, *b),
            Expression::SourceNotEqual(a, b) => {
                return self.binary("RelationalExpression", "!=", *a, *b);
            }
            Expression::Greater(a, b) => return self.binary("RelationalExpression", ">", *a, *b),
            Expression::GreaterOrEqual(a, b) => {
                return self.binary("RelationalExpression", ">=", *a, *b);
            }
            Expression::Less(a, b) => return self.binary("RelationalExpression", "<", *a, *b),
            Expression::LessOrEqual(a, b) => {
                return self.binary("RelationalExpression", "<=", *a, *b);
            }
            Expression::SameTerm(a, b) => (
                "Builtin_sameTerm",
                vec![("arg1", self.expression(*a)), ("arg2", self.expression(*b))],
            ),
            Expression::In(first, tail) => (
                "RelationalExpression",
                vec![
                    ("expr", self.expression(*first)),
                    ("op", V::Text("IN".into())),
                    (
                        "other",
                        V::List(
                            tail.into_iter()
                                .map(|value| self.expression(value))
                                .collect(),
                        ),
                    ),
                ],
            ),
            Expression::Add(a, b) => return self.binary("AdditiveExpression", "+", *a, *b),
            Expression::Subtract(a, b) => return self.binary("AdditiveExpression", "-", *a, *b),
            Expression::Multiply(a, b) => {
                return self.binary("MultiplicativeExpression", "*", *a, *b);
            }
            Expression::Divide(a, b) => {
                return self.binary("MultiplicativeExpression", "/", *a, *b);
            }
            Expression::AdditiveChain { first, tail } => (
                "AdditiveExpression",
                vec![
                    ("expr", self.expression(*first)),
                    (
                        "op",
                        V::List(
                            tail.iter()
                                .map(|(op, _)| {
                                    V::Text(
                                        if *op == AdditiveOperator::Add {
                                            "+"
                                        } else {
                                            "-"
                                        }
                                        .into(),
                                    )
                                })
                                .collect(),
                        ),
                    ),
                    (
                        "other",
                        V::List(
                            tail.into_iter()
                                .map(|(_, expression)| self.expression(expression))
                                .collect(),
                        ),
                    ),
                ],
            ),
            Expression::MultiplicativeChain { first, tail } => (
                "MultiplicativeExpression",
                vec![
                    ("expr", self.expression(*first)),
                    (
                        "op",
                        V::List(
                            tail.iter()
                                .map(|(op, _)| {
                                    V::Text(
                                        if *op == MultiplicativeOperator::Multiply {
                                            "*"
                                        } else {
                                            "/"
                                        }
                                        .into(),
                                    )
                                })
                                .collect(),
                        ),
                    ),
                    (
                        "other",
                        V::List(
                            tail.into_iter()
                                .map(|(_, expression)| self.expression(expression))
                                .collect(),
                        ),
                    ),
                ],
            ),
            Expression::If(a, b, c) => (
                "Builtin_IF",
                vec![
                    ("arg1", self.expression(*a)),
                    ("arg2", self.expression(*b)),
                    ("arg3", self.expression(*c)),
                ],
            ),
            Expression::Coalesce(values) => (
                "Builtin_COALESCE",
                vec![(
                    "arg",
                    V::List(
                        values
                            .into_iter()
                            .map(|value| self.expression(value))
                            .collect(),
                    ),
                )],
            ),
            Expression::FunctionCall(function, values) => {
                return self.function(function, values, None);
            }
        };
        V::Node(self.node(name, fields))
    }
    pub(super) fn capture_expression(&mut self, expression: Expression) -> Expression {
        match self.expression(expression) {
            V::Node(id) => Expression::SourceSyntax(SourceSyntaxSnapshot::captured(id)),
            V::Literal(value) => Expression::SourceLiteral(value),
            V::Variable(value) => Expression::Variable(value),
            V::NamedNode(value) => Expression::NamedNode(value),
            _ => unreachable!("expression reduction always yields an expression"),
        }
    }
    pub(super) fn boolean_chain(&mut self, name: &str, values: Vec<Expression>) -> Expression {
        if values.len() == 1 {
            return values.into_iter().next().unwrap();
        }
        let mut values = values
            .into_iter()
            .map(|value| self.expression(value))
            .collect::<Vec<_>>();
        let first = values.remove(0);
        let id = self.node(name, vec![("expr", first), ("other", V::List(values))]);
        Expression::SourceSyntax(SourceSyntaxSnapshot::captured(id))
    }
    pub(super) fn relation(
        &mut self,
        first: Expression,
        operator: &str,
        other: Option<Expression>,
        list: Option<Vec<Expression>>,
    ) -> Expression {
        let first = self.expression(first);
        let other = if let Some(other) = other {
            self.expression(other)
        } else {
            V::List(
                list.unwrap_or_default()
                    .into_iter()
                    .map(|value| self.expression(value))
                    .collect(),
            )
        };
        let id = self.node(
            "RelationalExpression",
            vec![
                ("expr", first),
                ("op", V::Text(operator.into())),
                ("other", other),
            ],
        );
        Expression::SourceSyntax(SourceSyntaxSnapshot::captured(id))
    }
    pub(super) fn named_function(
        &mut self,
        function: Function,
        values: Vec<Expression>,
        name: &str,
    ) -> Expression {
        let V::Node(id) = self.function(function, values, Some(name)) else {
            unreachable!()
        };
        Expression::SourceSyntax(SourceSyntaxSnapshot::captured(id))
    }
    pub(super) fn distinct_function(
        &mut self,
        iri: NamedNode,
        values: Vec<Expression>,
    ) -> Expression {
        let V::Node(id) = self.function(Function::Custom(iri), values, None) else {
            unreachable!()
        };
        let (_, distinct) = self.nodes[id]
            .fields
            .iter_mut()
            .find(|(name, _)| name == "distinct")
            .expect("DISTINCT grammar requires at least one argument");
        *distinct = V::Text("DISTINCT".into());
        Expression::SourceSyntax(SourceSyntaxSnapshot::captured(id))
    }
    fn function(&mut self, function: Function, values: Vec<Expression>, alias: Option<&str>) -> V {
        let values = values
            .into_iter()
            .map(|value| self.expression(value))
            .collect::<Vec<_>>();
        let (name, keys): (&str, &[&str]) = match &function {
            Function::Str => ("Builtin_STR", &["arg"]),
            Function::Lang => ("Builtin_LANG", &["arg"]),
            Function::LangMatches => ("Builtin_LANGMATCHES", &["arg1", "arg2"]),
            Function::Datatype => ("Builtin_DATATYPE", &["arg"]),
            Function::Iri => ("Builtin_IRI", &["arg"]),
            Function::BNode => ("Builtin_BNODE", &["arg"]),
            Function::Rand => ("Builtin_RAND", &[]),
            Function::Abs => ("Builtin_ABS", &["arg"]),
            Function::Ceil => ("Builtin_CEIL", &["arg"]),
            Function::Floor => ("Builtin_FLOOR", &["arg"]),
            Function::Round => ("Builtin_ROUND", &["arg"]),
            Function::Concat => ("Builtin_CONCAT", &["arg"]),
            Function::SubStr => ("Builtin_SUBSTR", &["arg", "start", "length"]),
            Function::StrLen => ("Builtin_STRLEN", &["arg"]),
            Function::Replace => (
                "Builtin_REPLACE",
                &["arg", "pattern", "replacement", "flags"],
            ),
            Function::UCase => ("Builtin_UCASE", &["arg"]),
            Function::LCase => ("Builtin_LCASE", &["arg"]),
            Function::EncodeForUri => ("Builtin_ENCODE_FOR_URI", &["arg"]),
            Function::Contains => ("Builtin_CONTAINS", &["arg1", "arg2"]),
            Function::StrStarts => ("Builtin_STRSTARTS", &["arg1", "arg2"]),
            Function::StrEnds => ("Builtin_STRENDS", &["arg1", "arg2"]),
            Function::StrBefore => ("Builtin_STRBEFORE", &["arg1", "arg2"]),
            Function::StrAfter => ("Builtin_STRAFTER", &["arg1", "arg2"]),
            Function::Year => ("Builtin_YEAR", &["arg"]),
            Function::Month => ("Builtin_MONTH", &["arg"]),
            Function::Day => ("Builtin_DAY", &["arg"]),
            Function::Hours => ("Builtin_HOURS", &["arg"]),
            Function::Minutes => ("Builtin_MINUTES", &["arg"]),
            Function::Seconds => ("Builtin_SECONDS", &["arg"]),
            Function::Timezone => ("Builtin_TIMEZONE", &["arg"]),
            Function::Tz => ("Builtin_TZ", &["arg"]),
            Function::Now => ("Builtin_NOW", &[]),
            Function::Uuid => ("Builtin_UUID", &[]),
            Function::StrUuid => ("Builtin_STRUUID", &[]),
            Function::Md5 => ("Builtin_MD5", &["arg"]),
            Function::Sha1 => ("Builtin_SHA1", &["arg"]),
            Function::Sha256 => ("Builtin_SHA256", &["arg"]),
            Function::Sha384 => ("Builtin_SHA384", &["arg"]),
            Function::Sha512 => ("Builtin_SHA512", &["arg"]),
            Function::StrLang => ("Builtin_STRLANG", &["arg1", "arg2"]),
            Function::StrDt => ("Builtin_STRDT", &["arg1", "arg2"]),
            Function::IsIri => ("Builtin_isIRI", &["arg"]),
            Function::IsBlank => ("Builtin_isBLANK", &["arg"]),
            Function::IsLiteral => ("Builtin_isLITERAL", &["arg"]),
            Function::IsNumeric => ("Builtin_isNUMERIC", &["arg"]),
            Function::Regex => ("Builtin_REGEX", &["text", "pattern", "flags"]),
            Function::Custom(_) => ("Function", &[]),
            #[cfg(feature = "sparql-12")]
            Function::Triple
            | Function::Subject
            | Function::Predicate
            | Function::Object
            | Function::IsTriple
            | Function::LangDir
            | Function::HasLang
            | Function::HasLangDir
            | Function::StrLangDir => ("ExtensionFunction", &["arg"]),
            #[cfg(feature = "sep-0002")]
            Function::Adjust => ("ExtensionFunction", &["arg"]),
        };
        let fields = if let Function::Custom(iri) = &function {
            let mut fields = vec![("iri", V::NamedNode(iri.clone()))];
            if !values.is_empty() {
                fields.push(("distinct", V::List(Vec::new())));
                fields.push(("expr", V::List(values)));
            }
            fields
        } else if matches!(function, Function::Concat) {
            vec![("arg", V::List(values))]
        } else {
            keys.iter().copied().zip(values).collect()
        };
        let id = self.node(alias.unwrap_or(name), fields);
        self.functions.insert(id, function);
        V::Node(id)
    }
    fn binary(&mut self, name: &str, operator: &str, a: Expression, b: Expression) -> V {
        let a = self.expression(a);
        let b = self.expression(b);
        V::Node(self.node(
            name,
            vec![("expr", a), ("op", V::Text(operator.into())), ("other", b)],
        ))
    }
    fn pattern(&mut self, pattern: ParsedPattern) -> usize {
        match pattern {
            ParsedPattern::Source(id) => id,
            ParsedPattern::Legacy(pattern) => self.leaf(pattern),
        }
    }
    fn leaf(&mut self, pattern: GraphPattern) -> usize {
        match pattern {
            GraphPattern::Bgp { patterns } => {
                self.node("TriplesBlock", vec![("triples", V::Triples(patterns))])
            }
            GraphPattern::Path {
                subject,
                path,
                object,
            } => self.node(
                "Path",
                vec![(
                    "path",
                    V::Path {
                        subject,
                        path,
                        object,
                    },
                )],
            ),
            GraphPattern::SourceValues {
                variables,
                bindings,
            } => self.node(
                "InlineData",
                vec![(
                    "records",
                    V::Values {
                        variables,
                        bindings,
                    },
                )],
            ),
            GraphPattern::Join { left, right } => {
                let a = self.leaf(*left);
                let b = self.leaf(*right);
                self.node(
                    "GraphSequence",
                    vec![("part", V::List(vec![V::Node(a), V::Node(b)]))],
                )
            }
            _ => unreachable!("only grammar leaf patterns enter the source arena"),
        }
    }
    pub(super) fn exists(
        &mut self,
        pattern: ParsedPattern,
        polarity: SourceExistsPolarity,
    ) -> Expression {
        let graph = self.pattern(pattern);
        let node = self.node(
            if polarity == SourceExistsPolarity::Exists {
                "Builtin_EXISTS"
            } else {
                "Builtin_NOTEXISTS"
            },
            vec![("graph", V::Node(graph))],
        );
        Expression::SourceExists(SourceExistsExpression {
            polarity,
            syntax: SourceSyntaxSnapshot::captured(node),
            graph: None,
            no_isolated_scope: false,
        })
    }
    pub(super) fn group(
        &mut self,
        initial: Option<ParsedPattern>,
        parts: Vec<PartialGraphPattern>,
    ) -> ParsedPattern {
        let mut children = Vec::new();
        if let Some(initial) = initial {
            children.push(V::Node(self.pattern(initial)));
        }
        for part in parts {
            let node = match part {
                PartialGraphPattern::Source(id) => id,
                PartialGraphPattern::Bind(expression, variable) => {
                    let expression = self.expression(expression);
                    self.node(
                        "Bind",
                        vec![("expr", expression), ("var", V::Variable(variable))],
                    )
                }
                PartialGraphPattern::Filter(expression) => {
                    let expression = self.expression(expression);
                    self.node("Filter", vec![("expr", expression)])
                }
                PartialGraphPattern::Other(pattern) => self.leaf(pattern),
                _ => unreachable!("source grammar children must retain their syntax"),
            };
            children.push(V::Node(node));
        }
        let fields = if children.is_empty() {
            Vec::new()
        } else {
            vec![("part", V::List(children))]
        };
        ParsedPattern::Source(self.node("GroupGraphPatternSub", fields))
    }
    pub(super) fn unary_part(
        &mut self,
        name: &str,
        pattern: ParsedPattern,
        extra: Vec<(&str, V)>,
    ) -> PartialGraphPattern {
        let graph = self.pattern(pattern);
        let mut fields = extra;
        fields.push(("graph", V::Node(graph)));
        PartialGraphPattern::Source(self.node(name, fields))
    }
    pub(super) fn union_part(&mut self, patterns: Vec<ParsedPattern>) -> PartialGraphPattern {
        let graphs = patterns
            .into_iter()
            .map(|pattern| V::Node(self.pattern(pattern)))
            .collect();
        PartialGraphPattern::Source(
            self.node("GroupOrUnionGraphPattern", vec![("graph", V::List(graphs))]),
        )
    }
}

impl SourceArena {
    pub(super) fn select(
        &mut self,
        select: Selection,
        pattern: ParsedPattern,
        group: Option<ParsedGroupClause>,
        having: Option<ParsedHavingClause>,
        order: Option<Vec<ParsedOrderCondition>>,
        limit: Option<ParsedLimitOffset>,
        values: Option<ParsedPattern>,
        subquery: bool,
    ) -> ParsedPattern {
        let mut fields = Vec::new();
        match select.option {
            SelectionOption::Default => (),
            SelectionOption::Distinct => fields.push(("modifier", V::Text("DISTINCT".into()))),
            SelectionOption::Reduced => fields.push(("modifier", V::Text("REDUCED".into()))),
        }
        if let SelectionVariables::Explicit(members) = select.variables {
            let mut projection = Vec::new();
            for member in members {
                let fields = match member {
                    SelectionMember::Variable(variable) => vec![("var", V::Variable(variable))],
                    SelectionMember::Expression(expression, variable) => vec![
                        ("expr", self.expression(expression)),
                        ("evar", V::Variable(variable)),
                    ],
                };
                projection.push(V::Node(self.node("vars", fields)));
            }
            fields.push(("projection", V::List(projection)));
        }
        let pattern = self.pattern(pattern);
        fields.push(("where", V::Node(pattern)));
        if let Some(group) = group {
            let conditions = match group {
                ParsedGroupClause::SourceContext(conditions) => conditions,
                ParsedGroupClause::Source(conditions) => conditions
                    .into_iter()
                    .map(|condition| {
                        let syntax = match &condition {
                            SourceGroupCondition::As { .. } => {
                                SourceGroupConditionSyntax::ParenthesizedAlias
                            }
                            SourceGroupCondition::Raw(Expression::Variable(_)) => {
                                SourceGroupConditionSyntax::BareVariable
                            }
                            _ => SourceGroupConditionSyntax::BareFunction,
                        };
                        (condition, syntax)
                    })
                    .collect(),
                ParsedGroupClause::Legacy(..) => {
                    unreachable!("source group syntax policy required")
                }
            };
            let mut values = Vec::new();
            for (condition, syntax) in conditions {
                let (expression, variable) = match condition {
                    SourceGroupCondition::Raw(expression) => (expression, None),
                    SourceGroupCondition::As {
                        expression,
                        variable,
                    } => (expression, Some(variable)),
                };
                let expression = self.expression(expression);
                values.push(
                    if matches!(
                        syntax,
                        SourceGroupConditionSyntax::ParenthesizedAlias
                            | SourceGroupConditionSyntax::ParenthesizedWithoutAlias
                    ) {
                        let mut fields = vec![("expr", expression)];
                        if let Some(variable) = variable {
                            fields.push(("var", V::Variable(variable)));
                        }
                        V::Node(self.node("GroupAs", fields))
                    } else {
                        expression
                    },
                );
            }
            let group = self.node("GroupClause", vec![("condition", V::List(values))]);
            fields.push(("groupby", V::Node(group)));
        }
        if let Some(having) = having {
            let expressions = having
                .0
                .into_iter()
                .map(|value| self.expression(value))
                .collect();
            let id = self.node("HavingClause", vec![("condition", V::List(expressions))]);
            fields.push(("having", V::Node(id)));
        }
        if let Some(order) = order {
            let mut conditions = Vec::new();
            for condition in order {
                let mut fields = Vec::new();
                if let Some(order) = condition.order {
                    fields.push(("order", V::Text(order.into())));
                }
                fields.push(("expr", self.expression(condition.expression)));
                conditions.push(V::Node(self.node("OrderCondition", fields)));
            }
            let id = self.node("OrderClause", vec![("condition", V::List(conditions))]);
            fields.push(("orderby", V::Node(id)));
        }
        if let Some(limit) = limit {
            let mut limits = Vec::new();
            let ordered = if limit.limit_first {
                [("limit", limit.limit), ("offset", limit.offset)]
            } else {
                [("offset", limit.offset), ("limit", limit.limit)]
            };
            for (name, value) in ordered {
                if let Some(value) = value {
                    limits.push((
                        name,
                        value
                            .literal
                            .map(V::Literal)
                            .unwrap_or(V::Unsigned(value.value)),
                    ));
                }
            }
            let id = self.node("LimitOffsetClauses", limits);
            fields.push(("limitoffset", V::Node(id)));
        }
        if let Some(values) = values {
            let id = self.pattern(values);
            fields.push(("valuesClause", V::Node(id)));
        }
        ParsedPattern::Source(self.node(if subquery { "SubSelect" } else { "SelectQuery" }, fields))
    }
    fn and(&mut self, mut expressions: Vec<V>) -> V {
        if expressions.len() == 1 {
            return expressions.pop().unwrap();
        }
        if expressions.is_empty() {
            return V::None;
        }
        let first = expressions.remove(0);
        V::Node(self.node(
            "ConditionalAndExpression",
            vec![("expr", first), ("other", V::List(expressions))],
        ))
    }
    fn truth(
        &self,
        value: &V,
        hooks: &SourceLiteralHooks,
    ) -> Result<bool, crate::SourceLiteralBridgeError> {
        Ok(match value {
            V::None => false,
            V::Boolean(value) => *value,
            V::Text(value) => !value.is_empty(),
            V::Unsigned(value) => *value != 0,
            V::Variable(value) => !value.as_str().is_empty(),
            V::NamedNode(value) => !value.as_str().is_empty(),
            V::Literal(value) => {
                let occurrence = value
                    .occurrence
                    .as_ref()
                    .ok_or("A source literal truth read lacks its occurrence")?;
                (hooks.python_truth)(occurrence)?
            }
            V::List(values) => !values.is_empty(),
            V::Node(id) => !self.nodes[*id].fields.is_empty(),
            _ => true,
        })
    }
    fn map_children(
        &mut self,
        id: usize,
        mut visit: impl FnMut(&mut Self, V) -> Result<V, crate::SourceLiteralBridgeError>,
    ) -> Result<(), crate::SourceLiteralBridgeError> {
        // Read one link at a time: visiting a shared child can mutate that node.
        let count = self.nodes[id].fields.len();
        for index in 0..count {
            let value = self.nodes[id].fields[index].1.clone();
            let value = visit(self, value)?;
            self.nodes[id].fields[index].1 = value;
        }
        Ok(())
    }
    fn translate_exists(
        &mut self,
        value: V,
        hooks: &SourceLiteralHooks,
    ) -> Result<V, crate::SourceLiteralBridgeError> {
        match value {
            V::List(values) => Ok(V::List(
                values
                    .into_iter()
                    .map(|value| self.translate_exists(value, hooks))
                    .collect::<Result<_, _>>()?,
            )),
            V::Node(id) => {
                self.map_children(id, |arena, value| arena.translate_exists(value, hooks))?;
                if matches!(
                    self.nodes[id].name.as_str(),
                    "Builtin_EXISTS" | "Builtin_NOTEXISTS"
                ) {
                    let graph = self.nodes[id]
                        .attributes
                        .graph
                        .map(V::Node)
                        .unwrap_or_else(|| self.field(id, "graph"));
                    let V::Node(graph) = graph else {
                        return Err("EXISTS graph syntax is not a node".into());
                    };
                    let graph = self.translate_group(graph, hooks)?;
                    if self.nodes[graph].name == "Filter" {
                        self.nodes[graph].attributes.no_isolated_scope = Some(true);
                    }
                    self.nodes[id].attributes.graph = Some(graph);
                }
                Ok(V::Node(id))
            }
            value => Ok(value),
        }
    }
    pub(super) fn data(
        &mut self,
        variables: Vec<Variable>,
        bindings: Vec<Vec<Option<SourceValueTerm>>>,
        one_variable_form: bool,
    ) -> ParsedPattern {
        let var = variables.into_iter().map(V::Variable).collect::<Vec<_>>();
        let mut values = Vec::new();
        for row in bindings {
            let row = row
                .into_iter()
                .map(|value| match value {
                    Some(SourceValueTerm::NamedNode(node)) => V::NamedNode(node),
                    Some(SourceValueTerm::Literal(literal)) => V::Literal(literal),
                    None => V::Text("UNDEF".into()),
                })
                .collect::<Vec<_>>();
            if one_variable_form {
                values.extend(row);
            } else {
                values.push(V::List(row));
            }
        }
        let mut fields = Vec::new();
        if !var.is_empty() {
            fields.push(("var", V::List(var)));
        }
        if !values.is_empty() {
            fields.push(("value", V::List(values)));
        }
        ParsedPattern::Source(self.node("InlineData", fields))
    }
    fn translate_values(&mut self, id: usize) -> usize {
        let V::List(variables) = self.field(id, "var") else {
            return self.node("ToMultiSet", vec![("p", V::List(Vec::new()))]);
        };
        let V::List(values) = self.field(id, "value") else {
            return self.node("ToMultiSet", vec![("p", V::List(Vec::new()))]);
        };
        if variables.is_empty() || values.is_empty() {
            return self.node("ToMultiSet", vec![("p", V::List(Vec::new()))]);
        }
        let variables = variables
            .into_iter()
            .map(|value| match value {
                V::Variable(value) => value,
                _ => unreachable!("translated VALUES variables are syntax variables"),
            })
            .collect::<Vec<_>>();
        let mut bindings = Vec::new();
        for value in values {
            let row = match value {
                V::List(values) => values,
                value => vec![value],
            };
            bindings.push(
                row.into_iter()
                    .map(|value| match value {
                        V::Literal(value) => Some(SourceValueTerm::Literal(value)),
                        V::NamedNode(value) => Some(SourceValueTerm::NamedNode(value)),
                        V::Text(value) if value == "UNDEF" => None,
                        _ => unreachable!("VALUES contains a non-term syntax leaf"),
                    })
                    .collect(),
            );
        }
        let res = if self.source_graph_bgp {
            let mut columns = Vec::new();
            for variable in &variables {
                if !columns.contains(variable) {
                    columns.push(variable.clone());
                }
            }
            let records = bindings
                .into_iter()
                .map(|row| {
                    let mut record: Vec<(Variable, Option<SourceValueTerm>)> = Vec::new();
                    for (key, value) in variables.iter().cloned().zip(row) {
                        if let Some((_, existing)) = record.iter_mut().find(|(v, _)| v == &key) {
                            *existing = value;
                        } else {
                            record.push((key, value));
                        }
                    }
                    record
                })
                .collect();
            V::ValueRecords {
                variables: columns,
                records,
            }
        } else {
            V::Values {
                variables,
                bindings,
            }
        };
        let values = self.node("values", vec![("res", res)]);
        self.node("ToMultiSet", vec![("p", V::Node(values))])
    }
    fn translate_group(
        &mut self,
        id: usize,
        hooks: &SourceLiteralHooks,
    ) -> Result<usize, crate::SourceLiteralBridgeError> {
        if self.nodes[id].attributes.translated == Some(true) {
            return Ok(id);
        }
        if self.nodes[id].name == "SubSelect" {
            let inner = self.translate_select(id, hooks)?;
            return Ok(self.node("ToMultiSet", vec![("p", V::Node(inner))]));
        }
        // RDFLib assigns graphPattern.part = [] as an instance attribute when
        // the mapping field is absent/false; it does not create a mapping key.
        let (parts, part_in_mapping) = match self.field(id, "part") {
            V::List(parts) if !parts.is_empty() => (parts, true),
            V::List(_) | V::None => {
                self.nodes[id].attributes.part = Some(Vec::new());
                (Vec::new(), false)
            }
            _ => return Err("Group syntax part is not a list".into()),
        };
        let mut filters = Vec::new();
        let mut remaining = Vec::new();
        for part in parts {
            let V::Node(part) = part else {
                return Err("Group part is not a node".into());
            };
            if self.nodes[part].name == "Filter" {
                let expression = self.field(part, "expr");
                filters.push(self.translate_exists(expression, hooks)?);
            } else {
                remaining.push(V::Node(part));
            }
        }
        if part_in_mapping {
            self.put(id, "part", V::List(remaining.clone()));
        } else {
            self.nodes[id].attributes.part = Some(remaining.clone());
        }
        let filters = self.and(filters);
        let mut merged: Vec<V> = Vec::new();
        for part in remaining {
            if let V::Node(id) = part {
                if self.nodes[id].name == "TriplesBlock" {
                    if let Some(order) = &hooks.native_triples_order {
                        let triples = match self.field(id, "triples") {
                            V::NativeTriples(triples) => triples,
                            V::Triples(triples) => triples
                                .into_iter()
                                .map(|t| crate::SourceTriplePattern {
                                    subject: t.subject,
                                    predicate: crate::SourceTriplePredicate::Rdf(t.predicate),
                                    object: t.object,
                                })
                                .collect(),
                            _ => return Err("Source native TriplesBlock was not translated".into()),
                        };
                        let mut triples = order(&triples)?;
                        if let Some(V::Node(previous)) = merged.last() {
                            if self.nodes[*previous].name == "BGP" {
                                let V::NativeTriples(mut old) = self.field(*previous, "triples")
                                else {
                                    return Err("Source native BGP merge domain differs".into());
                                };
                                old.append(&mut triples);
                                self.put(*previous, "triples", V::NativeTriples(old));
                                continue;
                            }
                        }
                        merged.push(V::Node(
                            self.node("BGP", vec![("triples", V::NativeTriples(triples))]),
                        ));
                        continue;
                    }
                    let mut triples = match self.field(id, "triples") {
                        V::Triples(triples) => triples,
                        V::List(rows) => {
                            let grouped = Self::group_triple_rows(&rows)?;
                            let triples = self
                                .parsed_triples
                                .get(&id)
                                .ok_or("Captured TriplesBlock lacks its physical reduction")?;
                            if grouped.len() != triples.len()
                                || grouped.iter().zip(triples).any(|(row, triple)| {
                                    ParserState::source_graph_term(&triple.subject).as_ref()
                                        != Some(&row[0])
                                        || ParserState::source_graph_term(&triple.object).as_ref()
                                            != Some(&row[2])
                                })
                            {
                                return Err("Same-parse source triple rows drifted".into());
                            }
                            triples.clone()
                        }
                        _ => return Err("TriplesBlock has no triples".into()),
                    };
                    if let Some(order) = &hooks.triples_order {
                        triples = order(&triples)?;
                    }
                    if let Some(V::Node(previous)) = merged.last() {
                        if self.nodes[*previous].name == "BGP" {
                            let V::Triples(mut old) = self.field(*previous, "triples") else {
                                unreachable!()
                            };
                            old.append(&mut triples);
                            self.put(*previous, "triples", V::Triples(old));
                            continue;
                        }
                    }
                    merged.push(V::Node(
                        self.node("BGP", vec![("triples", V::Triples(triples))]),
                    ));
                    continue;
                }
            }
            merged.push(part);
        }
        let mut current = self.node("BGP", vec![("triples", V::Triples(Vec::new()))]);
        for part in merged {
            let V::Node(part) = part else { unreachable!() };
            match self.nodes[part].name.as_str() {
                "Bind" => {
                    let expression = self.field(part, "expr");
                    let expression = self.translate_exists(expression, hooks)?;
                    current = self.node(
                        "Extend",
                        vec![
                            ("p", V::Node(current)),
                            ("expr", expression),
                            ("var", self.field(part, "var")),
                        ],
                    );
                }
                "OptionalGraphPattern" => {
                    let V::Node(graph) = self.field(part, "graph") else {
                        return Err("Optional graph is missing".into());
                    };
                    let right = self.translate_group(graph, hooks)?;
                    let (right, expression) = if self.nodes[right].name == "Filter" {
                        let V::Node(inner) = self.field(right, "p") else {
                            return Err("Filter inner is missing".into());
                        };
                        (inner, self.field(right, "expr"))
                    } else {
                        (right, V::Node(self.node("TrueFilter", Vec::new())))
                    };
                    current = self.node(
                        "LeftJoin",
                        vec![
                            ("p1", V::Node(current)),
                            ("p2", V::Node(right)),
                            ("expr", expression),
                        ],
                    );
                }
                "MinusGraphPattern" => {
                    let V::Node(graph) = self.field(part, "graph") else {
                        return Err("Minus graph is missing".into());
                    };
                    let right = self.translate_group(graph, hooks)?;
                    current = self.node(
                        "Minus",
                        vec![("p1", V::Node(current)), ("p2", V::Node(right))],
                    );
                }
                "GroupOrUnionGraphPattern" => {
                    let V::List(graphs) = self.field(part, "graph") else {
                        return Err("Union graphs are missing".into());
                    };
                    let mut union = None;
                    for graph in graphs {
                        let V::Node(graph) = graph else {
                            return Err("Union graph is missing".into());
                        };
                        let graph = self.translate_group(graph, hooks)?;
                        union = Some(if let Some(left) = union {
                            self.node("Union", vec![("p1", V::Node(left)), ("p2", V::Node(graph))])
                        } else {
                            graph
                        });
                    }
                    let right = union.ok_or("Empty source union")?;
                    current = self.node(
                        "Join",
                        vec![("p1", V::Node(current)), ("p2", V::Node(right))],
                    );
                }
                "InlineData" => {
                    let right = self.translate_values(part);
                    current = self.node(
                        "Join",
                        vec![("p1", V::Node(current)), ("p2", V::Node(right))],
                    );
                }
                "BGP" => {
                    let right = part;
                    current = self.node(
                        "Join",
                        vec![("p1", V::Node(current)), ("p2", V::Node(right))],
                    );
                }
                "GraphGraphPattern" => {
                    let V::Node(graph) = self.field(part, "graph") else {
                        return Err("Graph child is missing".into());
                    };
                    let graph = self.translate_group(graph, hooks)?;
                    let graph = self.node(
                        "Graph",
                        vec![("term", self.field(part, "term")), ("p", V::Node(graph))],
                    );
                    current = self.node(
                        "Join",
                        vec![("p1", V::Node(current)), ("p2", V::Node(graph))],
                    );
                }
                name => {
                    return Err(
                        format!("Source frontend group part is not implemented: {name}").into(),
                    );
                }
            }
        }
        if self.truth(&filters, hooks)? {
            current = self.node("Filter", vec![("expr", filters), ("p", V::Node(current))]);
        }
        self.nodes[current].attributes.translated = Some(true);
        Ok(current)
    }
}

impl SourceArena {
    fn has_aggregate(&self, value: &V) -> bool {
        match value {
            V::Node(id) => {
                self.nodes[*id].name.starts_with("Aggregate_")
                    || self.nodes[*id]
                        .fields
                        .iter()
                        .any(|(_, value)| self.has_aggregate(value))
            }
            V::List(values) => values.iter().any(|value| self.has_aggregate(value)),
            _ => false,
        }
    }
    fn sample(&mut self, value: V, excluded: Option<&Variable>) -> V {
        match value {
            V::Variable(variable) if Some(&variable) != excluded => {
                V::Node(self.node("Aggregate_Sample", vec![("vars", V::Variable(variable))]))
            }
            V::Node(id) if !self.nodes[id].name.starts_with("Aggregate_") => {
                for index in 0..self.nodes[id].fields.len() {
                    let value = self.nodes[id].fields[index].1.clone();
                    self.nodes[id].fields[index].1 = self.sample(value, excluded);
                }
                V::Node(id)
            }
            V::List(values) => V::List(
                values
                    .into_iter()
                    .map(|value| self.sample(value, excluded))
                    .collect(),
            ),
            value => value,
        }
    }
    fn collect_aggregates(&mut self, value: V, aggregates: &mut Vec<V>) -> V {
        match value {
            V::Node(id) if self.nodes[id].name.starts_with("Aggregate_") => {
                aggregates.push(V::Node(id));
                let variable = Variable::new_unchecked(format!("__agg_{}__", aggregates.len()));
                self.put(id, "res", V::Variable(variable.clone()));
                V::Variable(variable)
            }
            V::Node(id) => {
                for index in 0..self.nodes[id].fields.len() {
                    let value = self.nodes[id].fields[index].1.clone();
                    self.nodes[id].fields[index].1 = self.collect_aggregates(value, aggregates);
                }
                V::Node(id)
            }
            V::List(values) => V::List(
                values
                    .into_iter()
                    .map(|value| self.collect_aggregates(value, aggregates))
                    .collect(),
            ),
            value => value,
        }
    }
    fn find_variables(&self, value: &V, variables: &mut Vec<Variable>) {
        match value {
            V::Variable(variable) => {
                variables.push(variable.clone());
            }
            V::Node(id) => {
                if self.nodes[*id].name == "Bind" {
                    if let V::Variable(variable) = self.field(*id, "var") {
                        variables.push(variable);
                    }
                    return;
                }
                if self.nodes[*id].name == "SubSelect" {
                    if let V::List(projections) = self.field(*id, "projection") {
                        for projection in projections {
                            if let V::Node(projection) = projection {
                                for field in ["var", "evar"] {
                                    if let V::Variable(variable) = self.field(projection, field) {
                                        variables.push(variable);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                for (_, value) in &self.nodes[*id].fields {
                    self.find_variables(value, variables);
                }
            }
            V::List(values) => {
                for value in values {
                    self.find_variables(value, variables);
                }
            }
            V::NamedNodePattern(NamedNodePattern::Variable(variable)) => {
                variables.push(variable.clone());
            }
            V::Triples(patterns) => {
                for pattern in patterns {
                    crate::algebra::lookup_triple_pattern_variables(pattern, &mut |variable| {
                        variables.push(variable.clone());
                    });
                }
            }
            V::NativeTriples(patterns) => {
                for pattern in patterns {
                    pattern.lookup_variables(&mut |v| variables.push(v.clone()));
                }
            }
            V::Path {
                subject, object, ..
            } => {
                for term in [subject, object] {
                    if let TermPattern::Variable(variable) = term {
                        variables.push(variable.clone());
                    }
                }
            }
            _ => (),
        }
    }
    fn first_child_projections(
        &self,
        id: usize,
        out: &mut Vec<Vec<Variable>>,
    ) -> Result<(), crate::SourceLiteralBridgeError> {
        // Original _find_first_child_projections visits mapping CompValue
        // values only, stops at Project, and never visits lists/attributes.
        for (_, value) in &self.nodes[id].fields {
            if let V::Node(child) = value {
                if self.nodes[*child].name == "Project" {
                    let V::List(values) = self.field(*child, "PV") else {
                        return Err("Child source Project lacks PV".into());
                    };
                    let columns = values
                        .into_iter()
                        .map(|v| match v {
                            V::Variable(v) => Ok(v),
                            _ => Err(crate::SourceLiteralBridgeError::from(
                                "Child source PV is not a variable",
                            )),
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    out.push(columns);
                } else {
                    self.first_child_projections(*child, out)?;
                }
            }
        }
        Ok(())
    }
    fn translate_select(
        &mut self,
        id: usize,
        hooks: &SourceLiteralHooks,
    ) -> Result<usize, crate::SourceLiteralBridgeError> {
        let where_value = self.property(id, "where");
        let where_value = if hooks.native_triples_order.is_some() {
            self.translate_native_paths(where_value, 0)?
        } else {
            where_value
        };
        // The admitted grammar has already lowered its paths. translatePath
        // keeps this group root, and q.where is an instance assignment. This
        // alias does not qualify complete source path translation/reordering.
        self.nodes[id].attributes.where_clause = Some(where_value.clone());
        let V::Node(where_id) = where_value else {
            return Err("Source SELECT has no WHERE node".into());
        };
        let mut visible = Vec::new();
        self.find_variables(&V::Node(where_id), &mut visible);
        let mut current = self.translate_group(where_id, hooks)?;
        let grouping = self.property(id, "groupby");
        let mut aggregate = false;
        if let V::Node(group) = grouping {
            let V::List(conditions) = self.property(group, "condition") else {
                return Err("GROUP BY lacks its conditions".into());
            };
            let base = current;
            let mut keys = Vec::new();
            let mut assignments = Vec::new();
            for condition in &conditions {
                if let V::Node(condition) = condition {
                    if self.nodes[*condition].name == "GroupAs" {
                        let expression = self.property(*condition, "expr");
                        let variable = self.property(*condition, "var");
                        current = self.node(
                            "Extend",
                            vec![
                                ("p", V::Node(current)),
                                ("expr", expression),
                                ("var", variable.clone()),
                            ],
                        );
                        keys.push(variable);
                        assignments.push(Some(current));
                        continue;
                    }
                }
                keys.push(condition.clone());
                assignments.push(None);
            }
            current = self.node(
                "Group",
                vec![("p", V::Node(current)), ("expr", V::List(keys))],
            );
            self.group_sources
                .insert(current, (base, conditions, assignments));
            aggregate = true;
        } else {
            let projection = self.property(id, "projection");
            let projected_aggregate = if let V::List(members) = projection {
                members.iter().any(|member|matches!(member,V::Node(member) if self.has_aggregate(&self.property(*member,"expr"))))
            } else {
                false
            };
            if self.has_aggregate(&self.property(id, "having"))
                || self.has_aggregate(&self.property(id, "orderby"))
                || projected_aggregate
            {
                current = self.node("Group", vec![("p", V::Node(current)), ("expr", V::None)]);
                aggregate = true;
            }
        }
        let mut aliases = Vec::new();
        if aggregate {
            let mut aggregates = Vec::new();
            if let V::List(members) = self.property(id, "projection") {
                for member in members {
                    if let V::Node(member) = member {
                        if let V::Variable(target) = self.property(member, "evar") {
                            let expression = self.property(member, "expr");
                            let expression = self.sample(expression, Some(&target));
                            self.nodes[member].attributes.expr = Some(expression);
                            let expression = self.property(member, "expr");
                            let expression = self.collect_aggregates(expression, &mut aggregates);
                            // v.expr does not replace v["expr"]. In particular,
                            // the mapping may keep the shared Aggregate node
                            // whose res was just mutated by _aggs.
                            self.nodes[member].attributes.expr = Some(expression);
                        }
                    }
                }
            }
            // traverse(q.having, _hasAggregate, complete=True) returns True
            // both on finding an aggregate and on completing the traversal,
            // including None. Only the sample result is assigned; the _aggs
            // traversal mutates shared mapping children and discards its root.
            let having = self.property(id, "having");
            let having = self.sample(having, None);
            self.nodes[id].attributes.having = Some(having);
            let having = self.property(id, "having");
            let _ = self.collect_aggregates(having, &mut aggregates);
            if self.has_aggregate(&self.property(id, "orderby")) {
                let order = self.property(id, "orderby");
                let order = self.sample(order, None);
                self.nodes[id].attributes.orderby = Some(order);
                let order = self.property(id, "orderby");
                let _ = self.collect_aggregates(order, &mut aggregates);
            }
            if let V::List(members) = self.property(id, "projection") {
                for member in members {
                    if let V::Node(member) = member {
                        if let V::Variable(variable) = self.property(member, "var") {
                            let node = self.node(
                                "Aggregate_Sample",
                                vec![("vars", V::Variable(variable.clone()))],
                            );
                            let value = self.collect_aggregates(V::Node(node), &mut aggregates);
                            aliases.push((value, variable));
                        }
                    }
                }
            }
            current = self.node(
                "AggregateJoin",
                vec![("A", V::List(aggregates)), ("p", V::Node(current))],
            );
        }
        for (expression, variable) in aliases {
            current = self.node(
                "Extend",
                vec![
                    ("p", V::Node(current)),
                    ("expr", expression),
                    ("var", V::Variable(variable)),
                ],
            );
        }
        if let V::Node(having) = self.property(id, "having") {
            if let V::List(conditions) = self.property(having, "condition") {
                let expression = self.and(conditions);
                current = self.node(
                    "Filter",
                    vec![("expr", expression), ("p", V::Node(current))],
                );
            }
        }
        if let V::Node(values) = self.property(id, "valuesClause") {
            let right = self.translate_values(values);
            current = self.node(
                "Join",
                vec![("p1", V::Node(current)), ("p2", V::Node(right))],
            );
        }
        let mut variables = Vec::new();
        if let V::List(members) = self.property(id, "projection") {
            let mut extensions = Vec::new();
            for member in members {
                let V::Node(member) = member else {
                    return Err("Invalid source projection member".into());
                };
                if let V::Variable(variable) = self.property(member, "var") {
                    variables.push(V::Variable(variable));
                } else if let V::Variable(variable) = self.property(member, "evar") {
                    variables.push(V::Variable(variable.clone()));
                    extensions.push((self.property(member, "expr"), variable));
                } else {
                    return Err("Source projection member has no variable".into());
                }
            }
            for (expression, variable) in extensions {
                current = self.node(
                    "Extend",
                    vec![
                        ("p", V::Node(current)),
                        ("expr", expression),
                        ("var", V::Variable(variable)),
                    ],
                );
            }
        } else {
            if let Some(project) = &hooks.visible_projection {
                let mut children = Vec::new();
                self.first_child_projections(current, &mut children)?;
                variables.extend(project(&visible, &children)?.into_iter().map(V::Variable));
            } else {
                // The pre-graph phase retains its previous sorted membership
                // projection. Native VS order requires its separate hook.
                variables.extend(
                    visible
                        .into_iter()
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .map(V::Variable),
                );
            }
        }
        if let V::Node(order) = self.property(id, "orderby") {
            let V::List(old_conditions) = self.property(order, "condition") else {
                return Err("ORDER lacks source conditions".into());
            };
            let mut conditions = Vec::new();
            for condition in old_conditions {
                let V::Node(condition) = condition else {
                    return Err("ORDER condition is not a node".into());
                };
                let fields = vec![
                    ("expr", self.property(condition, "expr")),
                    ("order", self.property(condition, "order")),
                ];
                conditions.push(V::Node(self.node("OrderCondition", fields)));
            }
            let conditions = V::List(conditions);
            current = self.node(
                "OrderBy",
                vec![("p", V::Node(current)), ("expr", conditions)],
            );
        }
        current = self.node(
            "Project",
            vec![("p", V::Node(current)), ("PV", V::List(variables))],
        );
        if let V::Text(modifier) = self.property(id, "modifier") {
            current = self.node(
                if modifier == "DISTINCT" {
                    "Distinct"
                } else {
                    "Reduced"
                },
                vec![("p", V::Node(current))],
            );
        }
        if let V::Node(limit) = self.property(id, "limitoffset") {
            let mut fields = vec![
                ("p", V::Node(current)),
                (
                    "start",
                    V::Unsigned(self.source_limit(limit, "offset")?.unwrap_or(0)),
                ),
            ];
            if !matches!(self.property(limit, "limit"), V::None) {
                fields.push((
                    "length",
                    V::Unsigned(self.source_limit(limit, "limit")?.ok_or("Missing LIMIT")?),
                ));
            }
            current = self.node("Slice", fields);
        }
        Ok(current)
    }
    fn source_limit(
        &self,
        id: usize,
        key: &str,
    ) -> Result<Option<usize>, crate::SourceLiteralBridgeError> {
        match self.field(id, key) {
            V::None => Ok(None),
            V::Unsigned(value) => Ok(Some(value)),
            V::Literal(value) => Ok(Some(value.rdf.value().parse::<usize>()?)),
            _ => Err("Source limit is not an integer literal".into()),
        }
    }
    fn simplify(
        &mut self,
        value: V,
        hooks: &SourceLiteralHooks,
    ) -> Result<V, crate::SourceLiteralBridgeError> {
        Ok(match value {
            V::List(values) => V::List(
                values
                    .into_iter()
                    .map(|value| self.simplify(value, hooks))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            V::Node(id) => {
                // Original simplify is postorder, and never visits attributes.
                for index in 0..self.nodes[id].fields.len() {
                    let value = self.nodes[id].fields[index].1.clone();
                    self.nodes[id].fields[index].1 = self.simplify(value, hooks)?;
                }
                if self.nodes[id].name == "Join" {
                    let left = self.field(id, "p1");
                    let right = self.field(id, "p2");
                    if let V::Node(left) = left {
                        if self.nodes[left].name == "BGP"
                            && source_empty_triples(&self.field(left, "triples"))
                        {
                            return Ok(right);
                        }
                    }
                    if let V::Node(right) = right {
                        if self.nodes[right].name == "BGP"
                            && source_empty_triples(&self.field(right, "triples"))
                        {
                            return Ok(left);
                        }
                    }
                }
                if self.nodes[id].name == "BGP" {
                    if let V::NativeTriples(triples) = self.field(id, "triples") {
                        let order = hooks
                            .native_triples_order
                            .as_ref()
                            .ok_or("Native triples have no source ordering owner")?;
                        self.put(id, "triples", V::NativeTriples(order(&triples)?));
                    } else if let Some(order) = &hooks.triples_order {
                        let V::Triples(triples) = self.field(id, "triples") else {
                            return Err("BGP triples missing during source simplify".into());
                        };
                        self.put(id, "triples", V::Triples(order(&triples)?));
                    }
                }
                V::Node(id)
            }
            value => value,
        })
    }
    fn analyse(&mut self, value: &V) -> bool {
        match value {
            V::Node(id) => {
                let children = self.nodes[*id]
                    .fields
                    .iter()
                    .map(|(_, value)| value.clone())
                    .collect::<Vec<_>>();
                let mut all = true;
                for child in children {
                    all &= self.analyse(&child);
                }
                match self.nodes[*id].name.as_str() {
                    "Join" => {
                        self.nodes[*id].attributes.lazy = Some(all);
                        self.put(*id, "lazy", V::Boolean(all));
                        false
                    }
                    "Distinct" | "Slice" => false,
                    _ => all,
                }
            }
            V::List(values) => {
                for value in values {
                    self.analyse(value);
                }
                // _traverseAgg visits children, but analyse(non-CompValue) returns True.
                true
            }
            _ => true,
        }
    }
    fn add_variables(
        &mut self,
        value: &V,
        program: &mut Vec<crate::SourceVariableSetStep>,
        captures: &mut Vec<(usize, usize)>,
    ) -> (BTreeSet<Variable>, usize) {
        use crate::SourceVariableSetStep as S;
        let mut variables = BTreeSet::new();
        let step = match value {
            V::Variable(variable) => {
                variables.insert(variable.clone());
                S::New(vec![variable.clone()])
            }
            V::List(values) => {
                let mut children = Vec::new();
                for value in values {
                    let (members, child) = self.add_variables(value, program, captures);
                    variables.extend(members);
                    children.push(child);
                }
                S::Union(children)
            }
            V::Node(id) => {
                let fields = self.nodes[*id].fields.clone();
                let name = self.nodes[*id].name.clone();
                let mut visited = Vec::new();
                for (_, value) in &fields {
                    if !matches!(value, V::None) {
                        visited.push(self.add_variables(value, program, captures));
                    }
                }
                let mut children = Vec::new();
                // Original _traverseAgg skips None while _addVars zips the
                // children with mapping keys; retain that exact boundary.
                for ((members, child), (key, _)) in visited.into_iter().zip(&fields) {
                    if name != "Extend" || key != "expr" {
                        variables.extend(members);
                        children.push(child);
                    }
                }
                let set = if name == "RelationalExpression" {
                    variables.clear();
                    push_variable_step(program, S::New(Vec::new()))
                } else {
                    let union = push_variable_step(program, S::Union(children));
                    if name == "Extend" {
                        union
                    } else {
                        push_variable_step(program, S::Copy(union))
                    }
                };
                self.nodes[*id].attributes.variables = Some(variables.iter().cloned().collect());
                self.put(
                    *id,
                    "_vars",
                    V::VariableSet(variables.iter().cloned().collect()),
                );
                captures.push((*id, set));
                if name == "SubSelect" {
                    variables.clear();
                    let mut projection = Vec::new();
                    if let V::List(members) = self.field(*id, "projection") {
                        for member in members {
                            if let V::Node(member) = member {
                                let value = match self.field(member, "var") {
                                    V::Variable(variable) => V::Variable(variable),
                                    _ => self.field(member, "evar"),
                                };
                                if let V::Variable(variable) = value {
                                    variables.insert(variable.clone());
                                    projection.push(variable);
                                }
                            }
                        }
                    }
                    return (variables, push_variable_step(program, S::New(projection)));
                }
                return (variables, set);
            }
            V::NamedNodePattern(NamedNodePattern::Variable(variable)) => {
                variables.insert(variable.clone());
                S::New(vec![variable.clone()])
            }
            V::Triples(patterns) => {
                // Ordered tuple/list traversal uses the existing source set
                // owner. Literal occurrences are terms, never scope members.
                // Parsed row/path representation and source triple ordering
                // remain separate qualifications from this membership fix.
                let values = patterns
                    .iter()
                    .map(|pattern| {
                        V::List(vec![
                            source_pattern_variable(&pattern.subject),
                            match &pattern.predicate {
                                NamedNodePattern::Variable(variable) => {
                                    V::Variable(variable.clone())
                                }
                                NamedNodePattern::NamedNode(_) => V::None,
                            },
                            source_pattern_variable(&pattern.object),
                        ])
                    })
                    .collect();
                return self.add_variables(&V::List(values), program, captures);
            }
            V::NativeTriples(patterns) => {
                let values = patterns
                    .iter()
                    .map(|pattern| {
                        V::List(vec![
                            source_pattern_variable(&pattern.subject),
                            match &pattern.predicate {
                                crate::SourceTriplePredicate::Rdf(NamedNodePattern::Variable(
                                    v,
                                )) => V::Variable(v.clone()),
                                _ => V::None,
                            },
                            source_pattern_variable(&pattern.object),
                        ])
                    })
                    .collect();
                return self.add_variables(&V::List(values), program, captures);
            }
            V::Path {
                subject, object, ..
            } => {
                return self.add_variables(
                    &V::List(vec![
                        source_pattern_variable(subject),
                        source_pattern_variable(object),
                    ]),
                    program,
                    captures,
                );
            }
            _ => S::Union(Vec::new()),
        };
        (variables, push_variable_step(program, step))
    }
}

fn source_pattern_variable(term: &TermPattern) -> V {
    match term {
        TermPattern::Variable(variable) => V::Variable(variable.clone()),
        _ => V::None,
    }
}

fn source_empty_triples(value: &V) -> bool {
    match value {
        V::Triples(v) => v.is_empty(),
        V::NativeTriples(v) => v.is_empty(),
        _ => false,
    }
}

impl SourceArena {
    fn value_expression(
        &self,
        value: V,
        snapshot: &Arc<[SourceSyntaxNode]>,
    ) -> Result<Expression, crate::SourceLiteralBridgeError> {
        Ok(match value {
            V::Variable(variable) => Expression::Variable(variable),
            V::Literal(value) => Expression::SourceLiteral(value),
            V::NamedNode(value) => Expression::NamedNode(value),
            V::Node(id) => {
                let unary = |key| {
                    self.value_expression(self.field(id, key), snapshot)
                        .map(Box::new)
                };
                let arguments = |key| -> Result<Vec<Expression>, crate::SourceLiteralBridgeError> {
                    let V::List(values) = self.field(id, key) else {
                        return Err("Expression arguments are not a list".into());
                    };
                    values
                        .into_iter()
                        .map(|value| self.value_expression(value, snapshot))
                        .collect()
                };
                match self.nodes[id].name.as_str() {
                    "Builtin_EXISTS" | "Builtin_NOTEXISTS" => {
                        let graph = self.nodes[id].attributes.graph;
                        Expression::SourceExists(SourceExistsExpression {
                            polarity: if self.nodes[id].name == "Builtin_EXISTS" {
                                SourceExistsPolarity::Exists
                            } else {
                                SourceExistsPolarity::NotExists
                            },
                            syntax: SourceSyntaxSnapshot::finalized(Arc::clone(snapshot), id),
                            graph: graph
                                .map(|graph| self.lower_graph(graph, snapshot).map(Box::new))
                                .transpose()?,
                            no_isolated_scope: graph.is_some_and(|graph| {
                                self.nodes[graph].attributes.no_isolated_scope == Some(true)
                            }),
                        })
                    }
                    "Builtin_BOUND" => {
                        let V::Variable(variable) = self.field(id, "arg") else {
                            return Err("BOUND has a non-variable argument".into());
                        };
                        Expression::Bound(variable)
                    }
                    "UnaryPlus" => Expression::UnaryPlus(unary("expr")?),
                    "UnaryMinus" => Expression::UnaryMinus(unary("expr")?),
                    "UnaryNot" => Expression::Not(unary("expr")?),
                    "ConditionalAndExpression" | "ConditionalOrExpression" => {
                        let mut result = self.value_expression(self.field(id, "expr"), snapshot)?;
                        for expression in arguments("other")? {
                            result = if self.nodes[id].name == "ConditionalAndExpression" {
                                Expression::And(Box::new(result), Box::new(expression))
                            } else {
                                Expression::Or(Box::new(result), Box::new(expression))
                            };
                        }
                        result
                    }
                    "RelationalExpression" => {
                        let a = unary("expr")?;
                        let V::Text(operator) = self.field(id, "op") else {
                            return Err("Relation has no operator".into());
                        };
                        if operator == "IN" || operator == "NOT IN" {
                            let value = Expression::In(a, arguments("other")?);
                            if operator == "NOT IN" {
                                Expression::Not(Box::new(value))
                            } else {
                                value
                            }
                        } else {
                            let b = unary("other")?;
                            match operator.as_str() {
                                "=" => Expression::Equal(a, b),
                                "!=" => Expression::SourceNotEqual(a, b),
                                ">" => Expression::Greater(a, b),
                                ">=" => Expression::GreaterOrEqual(a, b),
                                "<" => Expression::Less(a, b),
                                "<=" => Expression::LessOrEqual(a, b),
                                _ => return Err("Unknown source relation".into()),
                            }
                        }
                    }
                    "AdditiveExpression" | "MultiplicativeExpression" => {
                        let first = unary("expr")?;
                        let operators = match self.field(id, "op") {
                            V::List(values) => values,
                            value => vec![value],
                        };
                        let operands = match self.field(id, "other") {
                            V::List(values) => values,
                            value => vec![value],
                        };
                        if operators.len() != operands.len() || operators.is_empty() {
                            return Err("Arithmetic sequence shape mismatch".into());
                        }
                        if self.nodes[id].name == "AdditiveExpression" {
                            let mut tail = Vec::new();
                            for (operator, value) in operators.into_iter().zip(operands) {
                                let operator = match operator {
                                    V::Text(op) if op == "+" => AdditiveOperator::Add,
                                    V::Text(op) if op == "-" => AdditiveOperator::Subtract,
                                    _ => return Err("Unknown additive operator".into()),
                                };
                                tail.push((operator, self.value_expression(value, snapshot)?));
                            }
                            Expression::AdditiveChain { first, tail }
                        } else {
                            let mut tail = Vec::new();
                            for (operator, value) in operators.into_iter().zip(operands) {
                                let operator = match operator {
                                    V::Text(op) if op == "*" => MultiplicativeOperator::Multiply,
                                    V::Text(op) if op == "/" => MultiplicativeOperator::Divide,
                                    _ => return Err("Unknown multiplicative operator".into()),
                                };
                                tail.push((operator, self.value_expression(value, snapshot)?));
                            }
                            Expression::MultiplicativeChain { first, tail }
                        }
                    }
                    "Builtin_sameTerm" => Expression::SameTerm(unary("arg1")?, unary("arg2")?),
                    "Builtin_IF" => Expression::If(unary("arg1")?, unary("arg2")?, unary("arg3")?),
                    "Builtin_COALESCE" => Expression::Coalesce(arguments("arg")?),
                    _ if self.functions.contains_key(&id) => {
                        let function = self.functions[&id].clone();
                        let mut values = Vec::new();
                        for (key, value) in &self.nodes[id].fields {
                            if matches!(key.as_str(), "_vars" | "iri" | "distinct") {
                                continue;
                            }
                            if let V::List(items) = value {
                                for value in items {
                                    values.push(self.value_expression(value.clone(), snapshot)?);
                                }
                            } else {
                                values.push(self.value_expression(value.clone(), snapshot)?);
                            }
                        }
                        if matches!(function, Function::Custom(_) | Function::Str | Function::Datatype | Function::IsLiteral | Function::IsIri | Function::IsBlank | Function::IsNumeric | Function::Lang | Function::StrLen | Function::UCase | Function::LCase) {
                            Expression::SourceFunction {
                                function,
                                arguments: values,
                                syntax: SourceSyntaxSnapshot::finalized(Arc::clone(snapshot), id),
                            }
                        } else {
                            Expression::FunctionCall(function, values)
                        }
                    }
                    name => {
                        return Err(format!("Source expression cannot be lowered: {name}").into());
                    }
                }
            }
            _ => return Err("Source expression is not an expression value".into()),
        })
    }
    fn node_field(&self, id: usize, key: &str) -> Result<usize, crate::SourceLiteralBridgeError> {
        match self.field(id, key) {
            V::Node(node) => Ok(node),
            _ => Err(format!("{} has no node field {key}", self.nodes[id].name).into()),
        }
    }
    fn variable_field(
        &self,
        id: usize,
        key: &str,
    ) -> Result<Variable, crate::SourceLiteralBridgeError> {
        match self.field(id, key) {
            V::Variable(variable) => Ok(variable),
            _ => Err(format!("{} has no variable field {key}", self.nodes[id].name).into()),
        }
    }
    fn scope_of(&self, id: usize) -> Option<SourceVariableScope> {
        self.nodes[id]
            .attributes
            .variables
            .clone()
            .map(|variables| SourceVariableScope { variables })
    }
    fn aggregate_expression(
        &self,
        id: usize,
        snapshot: &Arc<[SourceSyntaxNode]>,
    ) -> Result<AggregateExpression, crate::SourceLiteralBridgeError> {
        let distinct = self.field(id, "distinct") == V::Text("DISTINCT".into());
        if self.nodes[id].name == "Aggregate_Count" && self.field(id, "vars") == V::Text("*".into())
        {
            return Ok(AggregateExpression::CountSolutions { distinct });
        }
        let name = match self.nodes[id].name.as_str() {
            "Aggregate_Count" => AggregateFunction::Count,
            "Aggregate_Sum" => AggregateFunction::Sum,
            "Aggregate_Avg" => AggregateFunction::Avg,
            "Aggregate_Min" => AggregateFunction::Min,
            "Aggregate_Max" => AggregateFunction::Max,
            "Aggregate_Sample" => AggregateFunction::Sample,
            _ => self
                .aggregate_functions
                .get(&id)
                .cloned()
                .ok_or("Aggregate lowering tag is absent")?,
        };
        Ok(AggregateExpression::FunctionCall {
            name,
            expr: self.value_expression(self.field(id, "vars"), snapshot)?,
            distinct,
        })
    }
    fn lower_group(
        &self,
        id: usize,
        snapshot: &Arc<[SourceSyntaxNode]>,
    ) -> Result<GraphPattern, crate::SourceLiteralBridgeError> {
        let group = self.node_field(id, "p")?;
        if self.nodes[group].name != "Group" {
            return Err("AggregateJoin does not wrap Group".into());
        }
        let mut contexts = Vec::new();
        let (inner, grouping) = if let Some((_base, conditions, assignments)) =
            self.group_sources.get(&group)
        {
            let mut grouping = Vec::new();
            for (condition, assignment) in conditions.iter().zip(assignments) {
                if let V::Node(condition) = condition {
                    if self.nodes[*condition].name == "GroupAs" {
                        let expression =
                            self.value_expression(self.field(*condition, "expr"), snapshot)?;
                        let variable = match self.field(*condition, "var") {
                            V::Variable(variable) => Some(variable),
                            V::None => None,
                            _ => return Err("GroupAs target has changed to a non-variable".into()),
                        };
                        let syntax = if variable.is_some() {
                            SourceGroupConditionSyntax::ParenthesizedAlias
                        } else {
                            SourceGroupConditionSyntax::ParenthesizedWithoutAlias
                        };
                        grouping.push(if let Some(variable) = variable {
                            SourceGroupCondition::As {
                                expression,
                                variable,
                            }
                        } else {
                            SourceGroupCondition::Raw(expression)
                        });
                        contexts.push(SourceGroupConditionContext {
                            syntax,
                            assignment_scope: assignment.and_then(|id| self.scope_of(id)),
                        });
                        continue;
                    }
                }
                let expression = self.value_expression(condition.clone(), snapshot)?;
                let syntax = if matches!(expression, Expression::Variable(_)) {
                    SourceGroupConditionSyntax::BareVariable
                } else {
                    SourceGroupConditionSyntax::BareFunction
                };
                grouping.push(SourceGroupCondition::Raw(expression));
                contexts.push(SourceGroupConditionContext {
                    syntax,
                    assignment_scope: None,
                });
            }
            // The saved base predates simplify/analyse. Reusing it resurrects
            // removed empty-BGP joins and can materialize/deduplicate/reorder
            // VALUES before the real group owner sees them. Follow the current
            // Group.p, stripping only the generated assignments that the owner
            // replays once in source order. Never run another simplify pass.
            let mut base = self.node_field(group, "p")?;
            for assignment in assignments.iter().flatten().rev() {
                if base != *assignment || self.nodes[base].name != "Extend" {
                    return Err("GroupAs assignment chain differs from its current source".into());
                }
                base = self.node_field(base, "p")?;
            }
            (base, SourceGrouping::Explicit(grouping))
        } else {
            (self.node_field(group, "p")?, SourceGrouping::Implicit)
        };
        let V::List(values) = self.field(id, "A") else {
            return Err("AggregateJoin has no ordered aggregates".into());
        };
        let mut aggregates = Vec::new();
        for value in values {
            let V::Node(value) = value else {
                return Err("AggregateJoin contains a non-node".into());
            };
            aggregates.push((
                self.variable_field(value, "res")?,
                self.aggregate_expression(value, snapshot)?,
            ));
        }
        let pattern = GraphPattern::SourceGroup {
            inner: Box::new(self.lower_graph(inner, snapshot)?),
            grouping,
            aggregates,
        };
        // Group syntax remains attached for the existing source grouping compiler.
        let pattern = GraphPattern::SourceContext {
            inner: Box::new(pattern),
            annotation: SourceContextAnnotation::Group {
                conditions: contexts,
            },
        };
        Ok(if self.scope_of(id).is_none() {
            GraphPattern::SourceContext {
                inner: Box::new(pattern),
                annotation: SourceContextAnnotation::Unanalyzed {
                    no_isolated_scope: self.nodes[id].attributes.no_isolated_scope == Some(true),
                },
            }
        } else {
            pattern
        })
    }
    fn lower_graph(
        &self,
        id: usize,
        snapshot: &Arc<[SourceSyntaxNode]>,
    ) -> Result<GraphPattern, crate::SourceLiteralBridgeError> {
        let node = &self.nodes[id];
        let inner = |key| {
            self.lower_graph(self.node_field(id, key)?, snapshot)
                .map(Box::new)
        };
        let expression = |key| self.value_expression(self.field(id, key), snapshot);
        let mut annotation = None;
        let pattern = match node.name.as_str() {
            "BGP" => match self.field(id, "triples") {
                V::NativeTriples(patterns) => {
                    let physical = patterns
                        .iter()
                        .map(crate::SourceTriplePattern::physical_pattern)
                        .reduce(new_join)
                        .unwrap_or_default();
                    annotation = Some(SourceContextAnnotation::PathBgp { patterns });
                    physical
                }
                V::Triples(patterns) => {
                    if self.source_graph_bgp {
                        annotation = Some(SourceContextAnnotation::Bgp {
                            patterns: patterns.clone(),
                        });
                    }
                    GraphPattern::Bgp { patterns }
                }
                _ => return Err("BGP triples are missing".into()),
            },
            "Join" => {
                if let Some(lazy) = node.attributes.lazy {
                    annotation = Some(SourceContextAnnotation::Join {
                        mode: if lazy {
                            SourceJoinMode::Lazy
                        } else {
                            SourceJoinMode::MaterializedRightSet
                        },
                    });
                }
                GraphPattern::Join {
                    left: inner("p1")?,
                    right: inner("p2")?,
                }
            }
            "LeftJoin" => {
                let unconditional = matches!(self.field(id,"expr"),V::Node(expression) if self.nodes[expression].name=="TrueFilter");
                if let Some(left_scope) = self.scope_of(self.node_field(id, "p1")?) {
                    annotation = Some(SourceContextAnnotation::LeftJoin {
                        left_scope,
                        unconditional,
                    });
                }
                GraphPattern::LeftJoin {
                    left: inner("p1")?,
                    right: inner("p2")?,
                    expression: if unconditional {
                        None
                    } else {
                        Some(expression("expr")?)
                    },
                }
            }
            "Minus" => {
                annotation = Some(SourceContextAnnotation::Minus);
                GraphPattern::Minus {
                    left: inner("p1")?,
                    right: inner("p2")?,
                }
            }
            "Union" => GraphPattern::Union {
                left: inner("p1")?,
                right: inner("p2")?,
            },
            "Filter" => {
                if let Some(scope) = self.scope_of(id) {
                    annotation = Some(SourceContextAnnotation::Filter { scope });
                }
                GraphPattern::Filter {
                    inner: inner("p")?,
                    expr: expression("expr")?,
                }
            }
            "Extend" => {
                if let Some(scope) = self.scope_of(id) {
                    annotation = Some(SourceContextAnnotation::Extend { scope });
                }
                GraphPattern::Extend {
                    inner: inner("p")?,
                    variable: self.variable_field(id, "var")?,
                    expression: expression("expr")?,
                }
            }
            "Project" => {
                let V::List(values) = self.field(id, "PV") else {
                    return Err("Project lacks its columns".into());
                };
                let variables = values
                    .into_iter()
                    .map(|value| match value {
                        V::Variable(value) => Ok(value),
                        _ => Err("Project column is not a variable".into()),
                    })
                    .collect::<Result<_, crate::SourceLiteralBridgeError>>()?;
                GraphPattern::Project {
                    inner: inner("p")?,
                    variables,
                }
            }
            "OrderBy" => {
                let V::List(values) = self.field(id, "expr") else {
                    return Err("ORDER lacks its conditions".into());
                };
                let mut expression = Vec::new();
                for value in values {
                    let V::Node(condition) = value else {
                        return Err("ORDER condition is not a node".into());
                    };
                    let value = self.value_expression(self.field(condition, "expr"), snapshot)?;
                    expression.push(
                        if self.field(condition, "order") == V::Text("DESC".into()) {
                            OrderExpression::Desc(value)
                        } else {
                            OrderExpression::Asc(value)
                        },
                    );
                }
                GraphPattern::SourceOrderBy {
                    inner: inner("p")?,
                    expression,
                }
            }
            "Distinct" => GraphPattern::Distinct { inner: inner("p")? },
            "Reduced" => GraphPattern::Reduced { inner: inner("p")? },
            "Slice" => {
                let V::Unsigned(start) = self.field(id, "start") else {
                    return Err("Slice lacks its offset".into());
                };
                let length = match self.field(id, "length") {
                    V::Unsigned(value) => Some(value),
                    V::None => None,
                    _ => return Err("Slice length is invalid".into()),
                };
                GraphPattern::Slice {
                    inner: inner("p")?,
                    start,
                    length,
                }
            }
            "Graph" => {
                let V::NamedNodePattern(name) = self.field(id, "term") else {
                    return Err("GRAPH lacks its term".into());
                };
                if self.source_graph_bgp {
                    annotation = Some(SourceContextAnnotation::Graph { name: name.clone() });
                }
                GraphPattern::Graph {
                    inner: inner("p")?,
                    name,
                }
            }
            "ToMultiSet" => {
                let pattern = match self.field(id, "p") {
                    V::Node(child) => self.lower_graph(child, snapshot)?,
                    V::List(values) if values.is_empty() => GraphPattern::SourceContext {
                        inner: Box::new(GraphPattern::SourceValues {
                            variables: Vec::new(),
                            bindings: Vec::new(),
                        }),
                        annotation: SourceContextAnnotation::Values {
                            form: SourceValuesForm::EmptyTranslatedList,
                        },
                    },
                    _ => return Err("ToMultiSet has an unsupported source input".into()),
                };
                let pattern = GraphPattern::SourceContext {
                    inner: Box::new(pattern),
                    annotation: SourceContextAnnotation::ToMultiSet,
                };
                return Ok(if self.scope_of(id).is_none() {
                    GraphPattern::SourceContext {
                        inner: Box::new(pattern),
                        annotation: SourceContextAnnotation::Unanalyzed {
                            no_isolated_scope: node.attributes.no_isolated_scope == Some(true),
                        },
                    }
                } else {
                    pattern
                });
            }
            "values" => {
                let (variables, bindings) = match self.field(id, "res") {
                    V::Values {
                        variables,
                        bindings,
                    } => (variables, bindings),
                    V::ValueRecords { variables, records } => {
                        // Only the executable tuple projection merges absent
                        // and UNDEF cells. The source dictionary stays intact.
                        let bindings = records
                            .into_iter()
                            .map(|record| {
                                variables
                                    .iter()
                                    .map(|key| {
                                        record
                                            .iter()
                                            .find(|(v, _)| v == key)
                                            .and_then(|(_, value)| value.clone())
                                    })
                                    .collect()
                            })
                            .collect();
                        (variables, bindings)
                    }
                    _ => return Err("VALUES lacks its records".into()),
                };
                annotation = Some(SourceContextAnnotation::Values {
                    form: if variables.is_empty() || bindings.is_empty() {
                        SourceValuesForm::EmptyTranslatedList
                    } else {
                        SourceValuesForm::Records
                    },
                });
                GraphPattern::SourceValues {
                    variables,
                    bindings,
                }
            }
            "AggregateJoin" => return self.lower_group(id, snapshot),
            name => return Err(format!("Source algebra cannot be lowered: {name}").into()),
        };
        if node.attributes.variables.is_none()
            && !matches!(
                annotation,
                Some(
                    SourceContextAnnotation::Bgp { .. }
                        | SourceContextAnnotation::PathBgp { .. }
                        | SourceContextAnnotation::Graph { .. }
                )
            )
        {
            annotation = Some(SourceContextAnnotation::Unanalyzed {
                no_isolated_scope: node.attributes.no_isolated_scope == Some(true),
            });
        }
        Ok(if let Some(annotation) = annotation {
            GraphPattern::SourceContext {
                inner: Box::new(pattern),
                annotation,
            }
        } else {
            pattern
        })
    }
    /// translateUpdate1 calls translateGroupGraphPattern directly. Unlike
    /// translateQuery it does not simplify, analyse lazy joins, or add _vars.
    /// Retain these absent facts as Unanalyzed source-context annotations.
    pub(super) fn finish_update_where(
        &mut self,
        group: usize,
        hooks: &SourceLiteralHooks,
    ) -> Result<GraphPattern, crate::SourceLiteralBridgeError> {
        self.source_graph_bgp = hooks.triples_order.is_some();
        let translated = self.translate_group(group, hooks)?;
        let snapshot = Arc::from(self.nodes.clone());
        self.lower_graph(translated, &snapshot)
    }

    pub(super) fn finish(
        &mut self,
        select: usize,
        hooks: &SourceLiteralHooks,
    ) -> Result<GraphPattern, crate::SourceLiteralBridgeError> {
        self.source_graph_bgp = hooks.triples_order.is_some();
        let translated = self.translate_select(select, hooks)?;
        let V::Node(translated) = self.simplify(V::Node(translated), hooks)? else {
            return Err("Source query has no graph result".into());
        };
        self.analyse(&V::Node(translated));
        let mut variable_program = Vec::new();
        let mut variable_captures = Vec::new();
        self.add_variables(
            &V::Node(translated),
            &mut variable_program,
            &mut variable_captures,
        );
        if let Some(construct) = &hooks.variable_sets {
            let sets = construct(&variable_program)?;
            if sets.len() != variable_program.len() {
                return Err("Source variable-set program returned the wrong domain".into());
            }
            for (node, set) in variable_captures {
                let actual = sets
                    .get(set)
                    .ok_or("Source variable-set occurrence is absent")?;
                let expected = self.nodes[node]
                    .attributes
                    .variables
                    .as_ref()
                    .ok_or("Source variable-set members are absent")?;
                if actual.iter().cloned().collect::<BTreeSet<_>>()
                    != expected.iter().cloned().collect()
                {
                    return Err("Source variable-set membership drift".into());
                }
                self.put(node, "_vars", V::VariableSet(actual.clone()));
            }
        }
        let snapshot = Arc::from(self.nodes.clone());
        self.lower_graph(translated, &snapshot)
    }
}

fn push_variable_step(
    program: &mut Vec<crate::SourceVariableSetStep>,
    step: crate::SourceVariableSetStep,
) -> usize {
    let id = program.len();
    program.push(step);
    id
}

#[cfg(test)]
mod triple_row_tests {
    use super::*;

    #[test]
    fn source_triple_rows_actual_translation_gates() {
        for native in [false, true] {
            let mut arena = SourceArena::default();
            let block = arena.node(
                "TriplesBlock",
                vec![("triples", V::List(vec![V::List(vec![V::None; 4])]))],
            );
            let error = if native {
                arena.translate_native_paths(V::Node(block), 0).unwrap_err()
            } else {
                let ParsedPattern::Source(group) =
                    arena.group(Some(ParsedPattern::Source(block)), Vec::new())
                else {
                    panic!()
                };
                let hooks = SourceLiteralHooks {
                    scope: crate::SourceLiteralScope::new(),
                    construct: std::rc::Rc::new(|_, _| {
                        panic!("row validation must precede construction")
                    }),
                    python_truth: std::rc::Rc::new(|_| {
                        panic!("row validation does not evaluate expressions")
                    }),
                    variable_sets: None,
                    triples_order: None,
                    native_triples_order: None,
                    graph_path_admission: None,
                    visible_projection: None,
                };
                arena.translate_group(group, &hooks).unwrap_err()
            };
            assert_eq!(
                error.downcast_ref::<crate::SourceTripleRowsError>(),
                Some(&crate::SourceTripleRowsError::InvalidLength),
                "native={native}: row error must precede missing physical-reduction error"
            );
        }
    }
}
