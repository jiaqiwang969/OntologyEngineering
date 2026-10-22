#![allow(clippy::ignored_unit_patterns)]

mod source_collections;
mod source_frontend;
mod source_input;
mod source_update;
use crate::algebra::*;
use crate::query::*;
use crate::term::*;
use crate::update::*;
use crate::source_update::*;
use crate::{SourceLiteralHooks, SourceLiteralInput};
use oxilangtag::LanguageTag;
use oxiri::{Iri, IriParseError};
use oxrdf::vocab::{rdf, xsd};
use peg::parser;
use peg::str::LineCol;
use rand::random;
#[doc(hidden)]
pub use source_collections::{SourceBlankNodeConstructor, source_default_blank_node};
use source_frontend::{
    ParsedAggregate, ParsedHavingClause, ParsedLimitOffset, ParsedLimitValue, ParsedOrderCondition,
    ParsedPattern, SourceArena,
};
#[doc(hidden)]
pub use source_input::SourceParserError;
#[cfg(feature = "standard-unicode-escaping")]
use std::borrow::Cow;
use std::char;
#[cfg(feature = "rdflib760-nullable-internal")]
use std::collections::BTreeSet;
use std::collections::{HashMap, HashSet};
use std::mem::take;
use std::rc::Rc;
#[cfg(feature = "standard-unicode-escaping")]
use std::str::Chars;
use std::str::FromStr;

/// Construction origins for the explicitly selected private numeric parser.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NumericTokenKind {
    Integer,
    Decimal,
    Double,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NumericTokenSign {
    Unsigned,
    Positive,
    Negative,
}

#[doc(hidden)]
pub type NumericLiteralConstructor = Rc<
    dyn Fn(
        NumericTokenKind,
        NumericTokenSign,
        &str,
    ) -> Result<Literal, Box<dyn std::error::Error + Send + Sync>>,
>;

/// A literal constructor failure is not a SPARQL evaluation error or an
/// ordinary lexical parse failure. The caller retains its typed source error.
#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
pub enum NumericQueryParseError {
    #[error(transparent)]
    Syntax(#[from] SparqlSyntaxError),
    #[error(transparent)]
    SourceSyntax(#[from] SourceParserError),
    #[error("Numeric token constructor failed: {0}")]
    LiteralConstruction(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl NumericQueryParseError {
    /// Typed source UPDATE namespace failure; native and other errors stay distinct.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn source_update_namespace_message(&self) -> Option<String> {
        let Self::LiteralConstruction(error) = self else { return None; };
        error.downcast_ref::<crate::source_update::SourceUpdateNamespaceError>()
            .map(ToString::to_string)
    }
}

/// A SPARQL parser
///
/// ```
/// use spargebra::SparqlParser;
///
/// let query_str = "SELECT ?s ?p ?o WHERE { ?s ?p ?o . }";
/// let query = SparqlParser::new().parse_query(query_str)?;
/// assert_eq!(query.to_string(), query_str);
/// # Ok::<_, spargebra::SparqlSyntaxError>(())
/// ```
#[must_use]
#[derive(Clone, Default)]
pub struct SparqlParser {
    base_iri: Option<Iri<String>>,
    prefixes: HashMap<String, String>,
    custom_aggregate_functions: HashSet<NamedNode>,
    source_blank_node_constructor: Option<SourceBlankNodeConstructor>,
}

impl SparqlParser {
    /// Explicit external constructor input, used only by the source graph
    /// frontend. It supplies random construction values, never query answers.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn with_source_blank_node_constructor(
        mut self,
        constructor: SourceBlankNodeConstructor,
    ) -> Self {
        self.source_blank_node_constructor = Some(constructor);
        self
    }

    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Provides an IRI that could be used to resolve the operation relative IRIs.
    ///
    /// ```
    /// use spargebra::SparqlParser;
    ///
    /// let query = SparqlParser::new().with_base_iri("http://example.com/")?.parse_query("SELECT * WHERE { <s> <p> <o> }")?;
    /// assert_eq!(query.to_string(), "BASE <http://example.com/>\nSELECT * WHERE { <http://example.com/s> <http://example.com/p> <http://example.com/o> . }");
    /// # Result::<_, Box<dyn std::error::Error>>::Ok(())
    /// ```
    #[inline]
    pub fn with_base_iri(mut self, base_iri: impl Into<String>) -> Result<Self, IriParseError> {
        self.base_iri = Some(Iri::parse(base_iri.into())?);
        Ok(self)
    }

    /// Set a default IRI prefix used during parsing.
    ///
    /// ```
    /// use spargebra::SparqlParser;
    ///
    /// let query = SparqlParser::new()
    ///     .with_prefix("ex", "http://example.com/")?
    ///     .parse_query("SELECT * WHERE { ex:s ex:p ex:o }")?;
    /// assert_eq!(
    ///     query.to_string(),
    ///     "SELECT * WHERE { <http://example.com/s> <http://example.com/p> <http://example.com/o> . }"
    /// );
    /// # Result::<_, Box<dyn std::error::Error>>::Ok(())
    /// ```
    #[inline]
    pub fn with_prefix(
        mut self,
        prefix_name: impl Into<String>,
        prefix_iri: impl Into<String>,
    ) -> Result<Self, IriParseError> {
        self.prefixes.insert(
            prefix_name.into(),
            Iri::parse(prefix_iri.into())?.into_inner(),
        );
        Ok(self)
    }

    /// Adds a new function to be parsed as a custom aggregate function and not as a regular custom function.
    ///
    /// ```
    /// use oxrdf::NamedNode;
    /// use spargebra::SparqlParser;
    ///
    /// SparqlParser::new()
    ///     .with_custom_aggregate_function(NamedNode::new("http://example.com/concat")?)
    ///     .parse_query(
    ///         "PREFIX ex: <http://example.com/> SELECT (ex:concat(?o) AS ?concat) WHERE { ex:s ex:p ex:o }",
    ///     )?;
    /// # Result::<_, Box<dyn std::error::Error>>::Ok(())
    /// ```
    #[inline]
    pub fn with_custom_aggregate_function(mut self, name: impl Into<NamedNode>) -> Self {
        self.custom_aggregate_functions.insert(name.into());
        self
    }

    /// Parse the given query string using the already set options.
    ///
    /// ```
    /// use spargebra::SparqlParser;
    ///
    /// let query_str = "SELECT ?s ?p ?o WHERE { ?s ?p ?o . }";
    /// let query = SparqlParser::new().parse_query(query_str)?;
    /// assert_eq!(query.to_string(), query_str);
    /// # Ok::<_, spargebra::SparqlSyntaxError>(())
    /// ```
    #[cfg_attr(
        not(feature = "standard-unicode-escaping"),
        expect(clippy::needless_borrow)
    )]
    pub fn parse_query(self, query: &str) -> Result<Query, SparqlSyntaxError> {
        self.parse_query_with_policies(query, false, false)
            .map(|(query, _)| query)
    }

    /// Explicit local RDFLib 7.6 modifier-key compatibility. The returned
    /// variables are genuine DESCRIBE targets, separate from modifier keys.
    pub fn parse_query_rdflib760(
        self,
        query: &str,
    ) -> Result<(Query, Option<Vec<Variable>>), SparqlSyntaxError> {
        self.parse_query_with_policies(query, true, false)
    }

    /// Private nullable-evaluator transport; preserves nonempty arithmetic sequences.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn parse_query_rdflib760_nullable(
        self,
        query: &str,
    ) -> Result<(Query, Option<Vec<Variable>>), SparqlSyntaxError> {
        self.parse_query_with_policies(query, true, true)
    }

    /// Invokes the private numeric constructor at the actual token production.
    /// The callback receives the unsigned magnitude and its syntactic sign;
    /// typed RDF strings bypass it and retain their decoded lexical form.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn parse_query_rdflib760_numeric(
        self,
        query: &str,
        constructor: NumericLiteralConstructor,
    ) -> Result<(Query, Option<Vec<Variable>>), NumericQueryParseError> {
        self.parse_numeric_query(query, constructor, false, false, false)
    }

    /// Preserves original Group keys, aggregate occurrences and implicit SAMPLEs.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn parse_query_rdflib760_numeric_aggregates(
        self,
        query: &str,
        constructor: NumericLiteralConstructor,
    ) -> Result<(Query, Option<Vec<Variable>>), NumericQueryParseError> {
        self.parse_numeric_query(query, constructor, true, false, false)
    }

    /// Preserves actual literal occurrences before RDF 1.1 canonicalization.
    /// Source grouping remains a separate immutable caller policy.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn parse_query_rdflib760_source_literals(
        self,
        query: &str,
        constructor: NumericLiteralConstructor,
        preserve_source_groups: bool,
    ) -> Result<(Query, Option<Vec<Variable>>), NumericQueryParseError> {
        self.parse_numeric_query(query, constructor, preserve_source_groups, true, false)
    }

    /// Preserves original binding-context annotations in the same parsed plan.
    /// The evaluator must independently require its immutable SourceContextV1 axis.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn parse_query_rdflib760_source_context(
        self,
        query: &str,
        constructor: NumericLiteralConstructor,
        preserve_source_groups: bool,
    ) -> Result<(Query, Option<Vec<Variable>>), NumericQueryParseError> {
        self.parse_numeric_query(query, constructor, preserve_source_groups, true, true)
    }

    /// Private source frontend. Every literal is constructed once at its actual
    /// grammar occurrence; all later phases reuse the scoped occurrence handle.
    /// This entry does not grant runtime EXISTS evaluation capability.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn parse_query_rdflib760_source_exists_frontend(
        self,
        query: &str,
        hooks: SourceLiteralHooks,
        preserve_source_groups: bool,
    ) -> Result<(Query, Option<Vec<Variable>>), NumericQueryParseError> {
        let mut state = ParserState::new(
            self.base_iri,
            self.prefixes,
            self.custom_aggregate_functions,
        );
        state.rdflib_describe = true;
        state.preserve_arithmetic_chains = true;
        state.preserve_source_groups = preserve_source_groups;
        state.preserve_source_literals = true;
        state.preserve_source_context = true;
        state.source_blank_node_constructor = self.source_blank_node_constructor;
        state.source_literal_hooks = Some(hooks);
        let source_graph = state.capture_source_graph_syntax();
        let query: std::borrow::Cow<'_, str> = if source_graph {
            source_input::prepare(query)?.into()
        } else {
            query.into()
        };
        #[cfg(feature = "standard-unicode-escaping")]
        let query: std::borrow::Cow<'_, str> = if source_graph {
            query
        } else {
            unescape_unicode_codepoints(&query).into_owned().into()
        };
        let parsed = if source_graph {
            parser::SourceQueryUnit(&query, &mut state)
        } else {
            parser::QueryUnit(&query, &mut state)
        };
        if let Some(error) = state.numeric_construction_error.take() {
            return Err(NumericQueryParseError::LiteralConstruction(error));
        }
        if parsed.is_err() {
            if let Some((at, message)) = state.source_parse_error.take() {
                return Err(SourceParserError::ParseException {
                    input: query.to_string(),
                    location: query[..at].chars().count(),
                    message,
                }
                .into());
            }
        }
        let parsed = parsed
            .map_err(SparqlSyntaxErrorKind::Syntax)
            .map_err(SparqlSyntaxError::from)?;
        Ok((parsed, state.describe_targets))
    }

    #[cfg(feature = "rdflib760-nullable-internal")]
    fn parse_numeric_query(
        self,
        query: &str,
        constructor: NumericLiteralConstructor,
        preserve_source_groups: bool,
        preserve_source_literals: bool,
        preserve_source_context: bool,
    ) -> Result<(Query, Option<Vec<Variable>>), NumericQueryParseError> {
        let mut state = ParserState::new(
            self.base_iri,
            self.prefixes,
            self.custom_aggregate_functions,
        );
        state.rdflib_describe = true;
        state.preserve_arithmetic_chains = true;
        state.preserve_source_groups = preserve_source_groups;
        state.preserve_source_literals = preserve_source_literals;
        state.preserve_source_context = preserve_source_context;
        state.numeric_constructor = Some(constructor);
        #[cfg(feature = "standard-unicode-escaping")]
        let query = unescape_unicode_codepoints(query);
        let parsed = parser::QueryUnit(&query, &mut state);
        if let Some(error) = state.numeric_construction_error {
            return Err(NumericQueryParseError::LiteralConstruction(error));
        }
        let mut parsed = parsed
            .map_err(SparqlSyntaxErrorKind::Syntax)
            .map_err(SparqlSyntaxError::from)?;
        if preserve_source_context {
            let pattern = match &mut parsed {
                Query::Select { pattern, .. }
                | Query::Construct { pattern, .. }
                | Query::Describe { pattern, .. }
                | Query::Ask { pattern, .. } => pattern,
            };
            annotate_source_context(pattern)
                .map_err(SparqlSyntaxErrorKind::SourceContext)
                .map_err(SparqlSyntaxError::from)?;
        }
        Ok((parsed, state.describe_targets))
    }

    #[cfg_attr(
        not(feature = "standard-unicode-escaping"),
        expect(clippy::needless_borrow)
    )]
    fn parse_query_with_policies(
        self,
        query: &str,
        rdflib_describe: bool,
        preserve_arithmetic_chains: bool,
    ) -> Result<(Query, Option<Vec<Variable>>), SparqlSyntaxError> {
        let mut state = ParserState::new(
            self.base_iri,
            self.prefixes,
            self.custom_aggregate_functions,
        );
        state.rdflib_describe = rdflib_describe;
        state.preserve_arithmetic_chains = preserve_arithmetic_chains;
        #[cfg(feature = "standard-unicode-escaping")]
        let query = unescape_unicode_codepoints(query);
        let parsed =
            parser::QueryUnit(&query, &mut state).map_err(SparqlSyntaxErrorKind::Syntax)?;
        Ok((parsed, state.describe_targets))
    }

    /// Private original UPDATE frontend. Parse and translate the entire
    /// request before execution, preserving source occurrences and operations.
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[doc(hidden)]
    pub fn parse_update_rdflib760_source(
        self,
        update: &str,
        hooks: SourceLiteralHooks,
    ) -> Result<crate::SourceUpdate, NumericQueryParseError> {
        let mut state = ParserState::new(self.base_iri, self.prefixes, self.custom_aggregate_functions);
        state.rdflib_describe = true;
        state.preserve_arithmetic_chains = true;
        state.preserve_source_groups = true;
        state.preserve_source_literals = true;
        state.preserve_source_context = true;
        state.source_blank_node_constructor = self.source_blank_node_constructor;
        state.source_literal_hooks = Some(hooks);
        let update = source_input::prepare(update)?;
        state.source_update_mode = true;
        let parsed = parser::SourceUpdateInit(&update, &mut state);
        if let Some(error) = state.numeric_construction_error.take() {
            return Err(NumericQueryParseError::LiteralConstruction(error));
        }
        if parsed.is_err() {
            if let Some((at, message)) = state.source_parse_error.take() {
                return Err(SourceParserError::ParseException {
                    input: update.to_string(),
                    location: update[..at].chars().count(),
                    message,
                }.into());
            }
        }
        let operations = parsed.map_err(SparqlSyntaxErrorKind::Syntax).map_err(SparqlSyntaxError::from)?;
        if let Some(prefix) = state.unresolved_update_prefix.take() {
            return Err(NumericQueryParseError::LiteralConstruction(Box::new(SourceUpdateNamespaceError(prefix))));
        }
        // Original data allows Variables and repeated labeled blank nodes;
        // native check_if_insert_data_are_sharing_blank_nodes is not its rule.
        Ok(crate::SourceUpdate { operations })
    }

    /// Parse the given update string using the already set options.
    ///
    /// ```
    /// use spargebra::SparqlParser;
    ///
    /// let update_str = "CLEAR ALL ;";
    /// let update = SparqlParser::new().parse_update(update_str)?;
    /// assert_eq!(update.to_string().trim(), update_str);
    /// # Ok::<_, spargebra::SparqlSyntaxError>(())
    /// ```
    #[cfg_attr(
        not(feature = "standard-unicode-escaping"),
        expect(clippy::needless_borrow)
    )]
    pub fn parse_update(self, update: &str) -> Result<Update, SparqlSyntaxError> {
        let mut state = ParserState::new(
            self.base_iri,
            self.prefixes,
            self.custom_aggregate_functions,
        );
        #[cfg(feature = "standard-unicode-escaping")]
        let update = unescape_unicode_codepoints(update);
        let operations =
            parser::UpdateInit(&update, &mut state).map_err(SparqlSyntaxErrorKind::Syntax)?;
        check_if_insert_data_are_sharing_blank_nodes(&operations)?;
        Ok(Update {
            operations,
            base_iri: state.base_iri,
        })
    }
}

/// Error returned during SPARQL parsing.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct SparqlSyntaxError {
    #[from]
    kind: SparqlSyntaxErrorKind,
}

impl SparqlSyntaxError {
    pub(crate) fn from_bad_base_iri(e: IriParseError) -> Self {
        SparqlSyntaxErrorKind::InvalidBaseIri(e).into()
    }
}

#[derive(Debug, thiserror::Error)]
enum SparqlSyntaxErrorKind {
    #[error("Invalid SPARQL base IRI provided: {0}")]
    InvalidBaseIri(#[from] IriParseError),
    #[error(transparent)]
    Syntax(#[from] peg::error::ParseError<LineCol>),
    #[error("The blank node {0} cannot be shared by multiple blocks")]
    SharedBlankNode(BlankNode),
    #[cfg(feature = "rdflib760-nullable-internal")]
    #[error("Invalid source context annotation: {0}")]
    SourceContext(&'static str),
}

#[cfg(feature = "standard-unicode-escaping")]
fn unescape_unicode_codepoints(input: &str) -> Cow<'_, str> {
    if needs_unescape_unicode_codepoints(input) {
        UnescapeUnicodeCharIterator::new(input).collect()
    } else {
        input.into()
    }
}

#[cfg(feature = "standard-unicode-escaping")]
fn needs_unescape_unicode_codepoints(input: &str) -> bool {
    let bytes = input.as_bytes();
    for i in 1..bytes.len() {
        if (bytes[i] == b'u' || bytes[i] == b'U') && bytes[i - 1] == b'\\' {
            return true;
        }
    }
    false
}

#[cfg(feature = "standard-unicode-escaping")]
struct UnescapeUnicodeCharIterator<'a> {
    iter: Chars<'a>,
    buffer: String,
}

#[cfg(feature = "standard-unicode-escaping")]
impl<'a> UnescapeUnicodeCharIterator<'a> {
    fn new(string: &'a str) -> Self {
        Self {
            iter: string.chars(),
            buffer: String::with_capacity(9),
        }
    }
}

#[cfg(feature = "standard-unicode-escaping")]
impl Iterator for UnescapeUnicodeCharIterator<'_> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        let c = if self.buffer.is_empty() {
            self.iter.next()?
        } else {
            self.buffer.remove(0)
        };
        match c {
            '\\' => match self.iter.next() {
                Some('u') => {
                    self.buffer.push('u');
                    for _ in 0..4 {
                        if let Some(c) = self.iter.next() {
                            self.buffer.push(c);
                        } else {
                            return Some('\\');
                        }
                    }
                    if let Some(c) = u32::from_str_radix(&self.buffer[1..], 16)
                        .ok()
                        .and_then(char::from_u32)
                    {
                        self.buffer.clear();
                        Some(c)
                    } else {
                        Some('\\')
                    }
                }
                Some('U') => {
                    self.buffer.push('U');
                    for _ in 0..8 {
                        if let Some(c) = self.iter.next() {
                            self.buffer.push(c);
                        } else {
                            return Some('\\');
                        }
                    }
                    if let Some(c) = u32::from_str_radix(&self.buffer[1..], 16)
                        .ok()
                        .and_then(char::from_u32)
                    {
                        self.buffer.clear();
                        Some(c)
                    } else {
                        Some('\\')
                    }
                }
                Some(c) => {
                    self.buffer.push(c);
                    Some('\\')
                }
                None => Some('\\'),
            },
            _ => Some(c),
        }
    }
}

struct ReifiedTerm {
    source: Option<crate::SourceSyntaxValue>,
    term: TermPattern,
    reifiers: Vec<TermPattern>,
}

#[derive(Default)]
struct FocusedTriplePattern<F> {
    focus: F,
    patterns: Vec<TriplePattern>,
}

impl<F> FocusedTriplePattern<F> {
    fn new(focus: impl Into<F>) -> Self {
        Self {
            focus: focus.into(),
            patterns: Vec::new(),
        }
    }
}

impl<F> From<FocusedTriplePattern<F>> for FocusedTriplePattern<Vec<F>> {
    fn from(input: FocusedTriplePattern<F>) -> Self {
        Self {
            focus: vec![input.focus],
            patterns: input.patterns,
        }
    }
}

#[derive(Clone, Debug)]
enum VariableOrPropertyPath {
    Variable(Variable),
    PropertyPath(PropertyPathExpression),
}

impl From<Variable> for VariableOrPropertyPath {
    fn from(var: Variable) -> Self {
        Self::Variable(var)
    }
}

impl From<NamedNodePattern> for VariableOrPropertyPath {
    fn from(pattern: NamedNodePattern) -> Self {
        match pattern {
            NamedNodePattern::NamedNode(node) => PropertyPathExpression::from(node).into(),
            NamedNodePattern::Variable(v) => v.into(),
        }
    }
}

impl From<PropertyPathExpression> for VariableOrPropertyPath {
    fn from(path: PropertyPathExpression) -> Self {
        Self::PropertyPath(path)
    }
}

/// Actual path grammar reductions and their physical expression have one parse
/// owner. Only the source graph frontend requests the syntax; the ordinary
/// parser keeps its existing expansion and does not allocate syntax nodes.
#[derive(Clone, Debug)]
struct ParsedPath {
    expression: PropertyPathExpression,
    syntax: Option<crate::SourceSyntaxValue>,
}

#[derive(Clone, Debug)]
struct ParsedVerb {
    expression: VariableOrPropertyPath,
    syntax: Option<crate::SourceSyntaxValue>,
}
impl From<ParsedVerb> for VariableOrPropertyPath {
    fn from(value: ParsedVerb) -> Self {
        value.expression
    }
}

#[cfg_attr(feature = "sparql-12", expect(clippy::unnecessary_wraps))]
fn add_to_triple_patterns(
    subject: TermPattern,
    predicate: NamedNodePattern,
    object: ReifiedTerm,
    patterns: &mut Vec<TriplePattern>,
) -> Result<(), &'static str> {
    let triple = TriplePattern::new(subject, predicate, object.term);
    #[cfg(feature = "sparql-12")]
    for reifier in object.reifiers {
        patterns.push(TriplePattern {
            subject: reifier.clone(),
            predicate: rdf::REIFIES.into_owned().into(),
            object: triple.clone().into(),
        });
    }
    #[cfg(not(feature = "sparql-12"))]
    if !object.reifiers.is_empty() {
        return Err("Triple terms are only available in SPARQL 1.2");
    }
    patterns.push(triple);
    Ok(())
}

fn add_to_triple_or_path_patterns(
    subject: TermPattern,
    predicate: impl Into<VariableOrPropertyPath>,
    object: ReifiedTerm,
    patterns: &mut Vec<TripleOrPathPattern>,
) -> Result<(), &'static str> {
    match predicate.into() {
        VariableOrPropertyPath::Variable(p) => {
            add_triple_to_triple_or_path_patterns(subject, p, object, patterns)?;
        }
        VariableOrPropertyPath::PropertyPath(p) => match p {
            PropertyPathExpression::NamedNode(p) => {
                add_triple_to_triple_or_path_patterns(subject, p, object, patterns)?;
            }
            PropertyPathExpression::Reverse(p) => add_to_triple_or_path_patterns(
                object.term,
                *p,
                ReifiedTerm {
                    source: None,
                    term: subject,
                    reifiers: object.reifiers,
                },
                patterns,
            )?,
            PropertyPathExpression::Sequence(a, b) => {
                if !object.reifiers.is_empty() {
                    return Err("Reifiers are not allowed on property paths");
                }
                let middle = BlankNode::default();
                add_to_triple_or_path_patterns(
                    subject,
                    *a,
                    ReifiedTerm {
                        source: None,
                        term: middle.clone().into(),
                        reifiers: Vec::new(),
                    },
                    patterns,
                )?;
                add_to_triple_or_path_patterns(
                    middle.into(),
                    *b,
                    ReifiedTerm {
                        source: None,
                        term: object.term,
                        reifiers: Vec::new(),
                    },
                    patterns,
                )?;
            }
            path => {
                if !object.reifiers.is_empty() {
                    return Err("Reifiers are not allowed on property paths");
                }
                patterns.push(TripleOrPathPattern::Path {
                    subject,
                    path,
                    object: object.term,
                })
            }
        },
    }
    Ok(())
}

fn add_source_native_pattern(
    subject: TermPattern,
    predicate: VariableOrPropertyPath,
    object: ReifiedTerm,
    patterns: &mut Vec<TripleOrPathPattern>,
    native: bool,
) -> Result<(), &'static str> {
    if native {
        if let VariableOrPropertyPath::PropertyPath(path) = &predicate {
            if !matches!(path, PropertyPathExpression::NamedNode(_)) {
                if !object.reifiers.is_empty() {
                    return Err("Reifiers are not allowed on property paths");
                }
                patterns.push(TripleOrPathPattern::Path {
                    subject,
                    path: path.clone(),
                    object: object.term,
                });
                return Ok(());
            }
        }
    }
    add_to_triple_or_path_patterns(subject, predicate, object, patterns)
}

#[cfg_attr(feature = "sparql-12", expect(clippy::unnecessary_wraps))]
fn add_triple_to_triple_or_path_patterns(
    subject: TermPattern,
    predicate: impl Into<NamedNodePattern>,
    object: ReifiedTerm,
    patterns: &mut Vec<TripleOrPathPattern>,
) -> Result<(), &'static str> {
    let triple = TriplePattern::new(subject, predicate, object.term);
    #[cfg(feature = "sparql-12")]
    for reifier in object.reifiers {
        patterns.push(
            TriplePattern {
                subject: reifier.clone(),
                predicate: rdf::REIFIES.into_owned().into(),
                object: triple.clone().into(),
            }
            .into(),
        );
    }
    #[cfg(not(feature = "sparql-12"))]
    if !object.reifiers.is_empty() {
        return Err("Triple terms are only available in SPARQL 1.2");
    }
    patterns.push(triple.into());
    Ok(())
}

fn build_bgp(patterns: Vec<TripleOrPathPattern>) -> GraphPattern {
    let mut bgp = Vec::new();
    let mut elements = Vec::with_capacity(patterns.len());
    let mut pending: Vec<_> = patterns.into_iter().rev().collect();
    while let Some(pattern) = pending.pop() {
        match pattern {
            TripleOrPathPattern::SourceRow { physical, .. } => {
                pending.extend(physical.into_iter().rev());
            }
            TripleOrPathPattern::Triple(t) => bgp.push(t),
            TripleOrPathPattern::Path {
                subject,
                path,
                object,
            } => {
                if !bgp.is_empty() {
                    elements.push(GraphPattern::Bgp {
                        patterns: take(&mut bgp),
                    });
                }
                elements.push(GraphPattern::Path {
                    subject,
                    path,
                    object,
                })
            }
        }
    }
    if !bgp.is_empty() {
        elements.push(GraphPattern::Bgp { patterns: bgp });
    }
    elements.into_iter().reduce(new_join).unwrap_or_default()
}

#[derive(Debug)]
enum TripleOrPathPattern {
    Triple(TriplePattern),
    Path {
        subject: TermPattern,
        path: PropertyPathExpression,
        object: TermPattern,
    },
    /// One real TriplesSameSubjectPath reduction, before path expansion.
    /// Commas/semicolons share this flat row; dots delimit separate rows.
    SourceRow {
        terms: Vec<crate::SourceSyntaxValue>,
        physical: Vec<Self>,
    },
}

impl From<TriplePattern> for TripleOrPathPattern {
    fn from(tp: TriplePattern) -> Self {
        Self::Triple(tp)
    }
}

#[derive(Debug, Default)]
struct FocusedTripleOrPathPattern<F> {
    source: Option<crate::SourceSyntaxValue>,
    focus: F,
    patterns: Vec<TripleOrPathPattern>,
}

impl<F> FocusedTripleOrPathPattern<F> {
    fn new(focus: impl Into<F>) -> Self {
        Self {
            source: None,
            focus: focus.into(),
            patterns: Vec::new(),
        }
    }
}

impl<F> From<FocusedTripleOrPathPattern<F>> for FocusedTripleOrPathPattern<Vec<F>> {
    fn from(input: FocusedTripleOrPathPattern<F>) -> Self {
        Self {
            source: input.source,
            focus: vec![input.focus],
            patterns: input.patterns,
        }
    }
}

impl<F, T: From<F>> From<FocusedTriplePattern<F>> for FocusedTripleOrPathPattern<T> {
    fn from(input: FocusedTriplePattern<F>) -> Self {
        Self {
            source: None,
            focus: input.focus.into(),
            patterns: input.patterns.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Hash)]
enum PartialGraphPattern {
    Source(usize),
    Optional(GraphPattern, Option<Expression>),
    #[cfg(feature = "sep-0006")]
    Lateral(GraphPattern),
    Minus(GraphPattern),
    Bind(Expression, Variable),
    Filter(Expression),
    Other(GraphPattern),
}

fn new_join(l: GraphPattern, r: GraphPattern) -> GraphPattern {
    // Avoid to output empty BGPs
    if let GraphPattern::Bgp { patterns: pl } = &l {
        if pl.is_empty() {
            return r;
        }
    }
    if let GraphPattern::Bgp { patterns: pr } = &r {
        if pr.is_empty() {
            return l;
        }
    }

    match (l, r) {
        (GraphPattern::Bgp { patterns: mut pl }, GraphPattern::Bgp { patterns: pr }) => {
            pl.extend(pr);
            GraphPattern::Bgp { patterns: pl }
        }
        (GraphPattern::Bgp { patterns }, other) | (other, GraphPattern::Bgp { patterns })
            if patterns.is_empty() =>
        {
            other
        }
        (l, r) => GraphPattern::Join {
            left: Box::new(l),
            right: Box::new(r),
        },
    }
}

fn not_empty_fold<T>(
    iter: impl Iterator<Item = T>,
    combine: impl Fn(T, T) -> T,
) -> Result<T, &'static str> {
    iter.fold(None, |a, b| match a {
        Some(av) => Some(combine(av, b)),
        None => Some(b),
    })
    .ok_or("The iterator should not be empty")
}

#[derive(Default)]
enum SelectionOption {
    Distinct,
    Reduced,
    #[default]
    Default,
}

enum SelectionMember {
    Variable(Variable),
    Expression(Expression, Variable),
}

#[derive(Default)]
enum SelectionVariables {
    Explicit(Vec<SelectionMember>),
    #[default]
    Star,
}

#[derive(Default)]
struct Selection {
    pub option: SelectionOption,
    pub variables: SelectionVariables,
}

enum ParsedGroupClause {
    Legacy(Vec<Variable>, Vec<(Expression, Variable)>),
    Source(Vec<SourceGroupCondition<Expression>>),
    SourceContext(Vec<(SourceGroupCondition<Expression>, SourceGroupConditionSyntax)>),
}

/// The source `_addVars` result is distinct from both output columns and all
/// expression dependencies. `analyse` consumes the accompanying lazy summary.
#[cfg(feature = "rdflib760-nullable-internal")]
struct SourcePlanSummary {
    variables: BTreeSet<Variable>,
    join_children_lazy: bool,
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn source_scope(variables: &BTreeSet<Variable>) -> SourceVariableScope {
    SourceVariableScope {
        variables: variables.iter().cloned().collect(),
    }
}

/// Read the normalized expression's original `_addVars` role. An actual
/// RelationalExpression contributes no vars; operators.simplify already removed
/// wrappers with no `other`. This never grants permission to execute an expression.
#[cfg(feature = "rdflib760-nullable-internal")]
fn source_expression_scope(
    expression: &mut Expression,
) -> Result<BTreeSet<Variable>, &'static str> {
    let mut variables = BTreeSet::new();
    match expression {
        Expression::NamedNode(_) | Expression::Literal(_) | Expression::SourceLiteral(_) => (),
        Expression::Variable(variable) | Expression::Bound(variable) => {
            variables.insert(variable.clone());
        }
        Expression::Equal(a, b)
        | Expression::SourceNotEqual(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b) => {
            // Still visit nested patterns, but the relation's returned scope is empty.
            source_expression_scope(a)?;
            source_expression_scope(b)?;
        }
        Expression::In(first, tail) => {
            source_expression_scope(first)?;
            for expression in tail {
                source_expression_scope(expression)?;
            }
        }
        Expression::Or(a, b)
        | Expression::And(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => {
            variables.extend(source_expression_scope(a)?);
            variables.extend(source_expression_scope(b)?);
        }
        Expression::UnaryPlus(inner) | Expression::UnaryMinus(inner) | Expression::Not(inner) => {
            variables.extend(source_expression_scope(inner)?);
        }
        Expression::SourceSyntax(_) => {
            return Err("Unfinalized source syntax reached legacy analysis");
        }
        Expression::SourceExists(value) => {
            value.syntax.lookup_used_variables(&mut |variable| {
                variables.insert(variable.clone());
            });
        }
        Expression::Exists(inner) => {
            // The same plan transport remains complete; the numeric/source
            // compiler still rejects EXISTS before executing this nested plan.
            variables.extend(annotate_source_context(inner)?.variables);
        }
        Expression::If(a, b, c) => {
            variables.extend(source_expression_scope(a)?);
            variables.extend(source_expression_scope(b)?);
            variables.extend(source_expression_scope(c)?);
        }
        Expression::Coalesce(arguments)
        | Expression::FunctionCall(_, arguments)
        | Expression::SourceFunction { arguments, .. } => {
            for expression in arguments {
                variables.extend(source_expression_scope(expression)?);
            }
        }
        Expression::AdditiveChain { first, tail } => {
            variables.extend(source_expression_scope(first)?);
            for (_, expression) in tail {
                variables.extend(source_expression_scope(expression)?);
            }
        }
        Expression::MultiplicativeChain { first, tail } => {
            variables.extend(source_expression_scope(first)?);
            for (_, expression) in tail {
                variables.extend(source_expression_scope(expression)?);
            }
        }
    }
    Ok(variables)
}

#[cfg(feature = "rdflib760-nullable-internal")]
fn source_aggregate_scope(
    aggregates: &mut [(Variable, AggregateExpression)],
) -> Result<BTreeSet<Variable>, &'static str> {
    let mut variables = BTreeSet::new();
    for (variable, aggregate) in aggregates {
        variables.insert(variable.clone());
        if let AggregateExpression::FunctionCall { expr, .. } = aggregate {
            variables.extend(source_expression_scope(expr)?);
        }
    }
    Ok(variables)
}

/// The syntax record belongs to the one actual SourceGroup node. In particular,
/// GroupAs(None) must not become an ordinary raw grouping expression.
#[cfg(feature = "rdflib760-nullable-internal")]
fn source_group_context(
    pattern: &mut GraphPattern,
    contexts: &mut [SourceGroupConditionContext],
) -> Result<SourcePlanSummary, &'static str> {
    let GraphPattern::SourceGroup {
        inner,
        grouping,
        aggregates,
    } = pattern
    else {
        return Err("Group context must wrap SourceGroup");
    };
    let mut summary = annotate_source_context(inner)?;
    let conditions = match grouping {
        SourceGrouping::Implicit if contexts.is_empty() => &mut [][..],
        SourceGrouping::Explicit(conditions) if conditions.len() == contexts.len() => {
            conditions.as_mut_slice()
        }
        _ => return Err("Group syntax and conditions disagree"),
    };
    // Translation builds every GroupAs Extend before Group reads any key.
    for (condition, context) in conditions.iter().zip(contexts.iter_mut()) {
        match (condition, context.syntax) {
            (
                SourceGroupCondition::As { variable, .. },
                SourceGroupConditionSyntax::ParenthesizedAlias,
            ) => {
                summary.variables.insert(variable.clone());
                context.assignment_scope = Some(source_scope(&summary.variables));
            }
            (
                SourceGroupCondition::Raw(_),
                SourceGroupConditionSyntax::ParenthesizedWithoutAlias,
            ) => {
                context.assignment_scope = Some(source_scope(&summary.variables));
            }
            (
                SourceGroupCondition::Raw(Expression::Variable(_)),
                SourceGroupConditionSyntax::BareVariable,
            )
            | (SourceGroupCondition::Raw(_), SourceGroupConditionSyntax::BareFunction) => {
                context.assignment_scope = None;
            }
            _ => return Err("Group condition syntax does not match its expression"),
        }
    }
    for (condition, context) in conditions.iter_mut().zip(contexts.iter()) {
        match condition {
            SourceGroupCondition::Raw(expression) => {
                let expression_vars = source_expression_scope(expression)?;
                if matches!(
                    context.syntax,
                    SourceGroupConditionSyntax::BareVariable
                        | SourceGroupConditionSyntax::BareFunction
                ) {
                    summary.variables.extend(expression_vars);
                }
                // ParenthesizedWithoutAlias produces an Extend(None) then a
                // None key. Its expression-only variables stay out of `_vars`.
            }
            SourceGroupCondition::As { expression, .. } => {
                source_expression_scope(expression)?;
            }
        }
    }
    summary
        .variables
        .extend(source_aggregate_scope(aggregates)?);
    Ok(summary)
}

/// One post-parse pass over the actual source algebra, before shared lowering.
/// It preserves field roles from analyse/_addVars instead of reading physical
/// hash keys or planner output-variable walkers. The caller selects the axis.
#[cfg(feature = "rdflib760-nullable-internal")]
fn annotate_source_context(pattern: &mut GraphPattern) -> Result<SourcePlanSummary, &'static str> {
    let mut annotation = None;
    let summary = match pattern {
        GraphPattern::SourceContext {
            inner,
            annotation: SourceContextAnnotation::Group { conditions },
        } => {
            return source_group_context(inner, conditions);
        }
        GraphPattern::SourceContext { .. } => {
            return Err("Source context was annotated more than once");
        }
        GraphPattern::Bgp { .. } | GraphPattern::Path { .. } => {
            let mut variables = BTreeSet::new();
            pattern.on_in_scope_variable(|variable| {
                variables.insert(variable.clone());
            });
            SourcePlanSummary {
                variables,
                join_children_lazy: true,
            }
        }
        GraphPattern::SourceValues {
            variables,
            bindings,
        } => {
            annotation = Some(SourceContextAnnotation::Values {
                form: if variables.is_empty() || bindings.is_empty() {
                    SourceValuesForm::EmptyTranslatedList
                } else {
                    SourceValuesForm::Records
                },
            });
            // Source rows are ordinary dicts, opaque to _traverseAgg/_addVars.
            SourcePlanSummary {
                variables: BTreeSet::new(),
                join_children_lazy: true,
            }
        }
        GraphPattern::Join { left, right } => {
            let mut a = annotate_source_context(left)?;
            let b = annotate_source_context(right)?;
            annotation = Some(SourceContextAnnotation::Join {
                mode: if a.join_children_lazy && b.join_children_lazy {
                    SourceJoinMode::Lazy
                } else {
                    SourceJoinMode::MaterializedRightSet
                },
            });
            a.variables.extend(b.variables);
            a.join_children_lazy = false;
            a
        }
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => {
            let mut a = annotate_source_context(left)?;
            let b = annotate_source_context(right)?;
            annotation = Some(SourceContextAnnotation::LeftJoin {
                left_scope: source_scope(&a.variables),
                unconditional: expression.is_none(),
            });
            a.variables.extend(b.variables);
            a.join_children_lazy &= b.join_children_lazy;
            if let Some(expression) = expression {
                a.variables.extend(source_expression_scope(expression)?);
            }
            a
        }
        GraphPattern::Minus { left, right } => {
            let mut a = annotate_source_context(left)?;
            let b = annotate_source_context(right)?;
            annotation = Some(SourceContextAnnotation::Minus);
            a.variables.extend(b.variables);
            a.join_children_lazy &= b.join_children_lazy;
            a
        }
        GraphPattern::Union { left, right } => {
            let mut a = annotate_source_context(left)?;
            let b = annotate_source_context(right)?;
            a.variables.extend(b.variables);
            a.join_children_lazy &= b.join_children_lazy;
            a
        }
        #[cfg(feature = "sep-0006")]
        GraphPattern::Lateral { left, right } => {
            let mut a = annotate_source_context(left)?;
            let b = annotate_source_context(right)?;
            a.variables.extend(b.variables);
            a.join_children_lazy &= b.join_children_lazy;
            a
        }
        GraphPattern::Filter { inner, expr } => {
            let mut summary = annotate_source_context(inner)?;
            summary.variables.extend(source_expression_scope(expr)?);
            annotation = Some(SourceContextAnnotation::Filter {
                scope: source_scope(&summary.variables),
            });
            summary
        }
        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => {
            let mut summary = annotate_source_context(inner)?;
            source_expression_scope(expression)?;
            summary.variables.insert(variable.clone());
            annotation = Some(SourceContextAnnotation::Extend {
                scope: source_scope(&summary.variables),
            });
            summary
        }
        GraphPattern::Project { inner, variables } => {
            let mut summary = annotate_source_context(inner)?;
            summary.variables.extend(variables.iter().cloned());
            summary
        }
        GraphPattern::OrderBy { inner, expression }
        | GraphPattern::SourceOrderBy { inner, expression } => {
            let mut summary = annotate_source_context(inner)?;
            for expression in expression {
                let (OrderExpression::Asc(expression) | OrderExpression::Desc(expression)) =
                    expression;
                summary
                    .variables
                    .extend(source_expression_scope(expression)?);
            }
            summary
        }
        GraphPattern::Distinct { inner } | GraphPattern::Slice { inner, .. } => {
            let mut summary = annotate_source_context(inner)?;
            summary.join_children_lazy = false;
            summary
        }
        GraphPattern::Reduced { inner } => annotate_source_context(inner)?,
        GraphPattern::Graph { inner, name } | GraphPattern::Service { inner, name, .. } => {
            let mut summary = annotate_source_context(inner)?;
            if let NamedNodePattern::Variable(variable) = name {
                summary.variables.insert(variable.clone());
            }
            summary
        }
        GraphPattern::Group {
            inner,
            variables,
            aggregates,
        } => {
            // The evaluator still gates the legacy Group variant in a source
            // context. Transporting it does not grant aggregate permission.
            let mut summary = annotate_source_context(inner)?;
            summary.variables.extend(variables.iter().cloned());
            summary
                .variables
                .extend(source_aggregate_scope(aggregates)?);
            summary
        }
        GraphPattern::Values { variables, .. } => SourcePlanSummary {
            variables: variables.iter().cloned().collect(),
            join_children_lazy: true,
        },
        GraphPattern::SourceGroup { .. } => {
            return Err("Source context Group lacks occurrence syntax");
        }
    };
    if let Some(annotation) = annotation {
        *pattern = GraphPattern::SourceContext {
            inner: Box::new(take(pattern)),
            annotation,
        };
    }
    Ok(summary)
}

fn build_select(
    select: Selection,
    r#where: impl Into<ParsedPattern>,
    group: Option<ParsedGroupClause>,
    having: Option<ParsedHavingClause>,
    order_by: Option<Vec<ParsedOrderCondition>>,
    offset_limit: Option<ParsedLimitOffset>,
    values: Option<ParsedPattern>,
    state: &mut ParserState,
) -> Result<GraphPattern, &'static str> {
    let aggregates = state.aggregates.pop().unwrap_or_default();
    let r#where = r#where.into();
    if let Some(hooks) = state.source_literal_hooks.clone() {
        if state.numeric_construction_error.is_some() {
            return Err("Source literal construction already failed");
        }
        let ParsedPattern::Source(select) = state.source_arena.select(
            select,
            r#where,
            group,
            having,
            order_by,
            offset_limit,
            values,
            false,
        ) else {
            unreachable!()
        };
        return match state.source_arena.finish(select, &hooks) {
            Ok(pattern) => Ok(pattern),
            Err(error) => {
                if state.numeric_construction_error.is_none() {
                    state.numeric_construction_error = Some(error);
                }
                Err("Source frontend phase failed")
            }
        };
    }
    let having = having.map(ParsedHavingClause::legacy).transpose()?;
    let order_by = order_by.map(|conditions| {
        conditions
            .into_iter()
            .map(ParsedOrderCondition::legacy)
            .collect()
    });
    let offset_limit = offset_limit.map(ParsedLimitOffset::legacy);
    let r#where = r#where.legacy()?;
    let values = values.map(ParsedPattern::legacy).transpose()?;
    if state.preserve_source_groups && (group.is_some() || !aggregates.is_empty()) {
        return build_source_aggregate_select(
            select,
            r#where,
            group,
            having,
            order_by,
            offset_limit,
            values,
            aggregates,
            state.preserve_source_literals,
            state.preserve_source_context,
        );
    }
    let mut group = match group {
        Some(ParsedGroupClause::Legacy(variables, bindings)) => Some((variables, bindings)),
        Some(ParsedGroupClause::Source(_) | ParsedGroupClause::SourceContext(_)) => {
            return Err("Source grouping requires its parser policy");
        }
        None => None,
    };
    let mut p = r#where;
    let mut with_aggregate = false;

    // GROUP BY
    if group.is_none() && !aggregates.is_empty() {
        group = Some((vec![], vec![]));
    }

    if let Some((clauses, binds)) = group {
        for (expression, variable) in binds {
            // The corrected variable-AS path is a genuine assignment, like
            // BIND. External initial values are not query declarations; a
            // target already declared in this query level cannot be replaced.
            if matches!(&expression, Expression::Variable(_)) {
                let mut declared = false;
                p.on_in_scope_variable(|v| declared |= v == &variable);
                if declared {
                    return Err("The GROUP BY alias overrides an existing query variable");
                }
            }
            p = GraphPattern::Extend {
                inner: Box::new(p),
                variable,
                expression,
            };
        }
        p = GraphPattern::Group {
            inner: Box::new(p),
            variables: clauses,
            aggregates,
        };
        with_aggregate = true;
    }

    // HAVING
    if let Some(expr) = having {
        p = GraphPattern::Filter {
            expr,
            inner: Box::new(p),
        };
    }

    // VALUES
    if let Some(data) = values {
        p = new_join(p, data);
    }

    // SELECT
    let mut pv = Vec::new();
    let with_project = match select.variables {
        SelectionVariables::Explicit(sel_items) => {
            let mut visible = HashSet::new();
            p.on_in_scope_variable(|v| {
                visible.insert(v.clone());
            });
            for sel_item in sel_items {
                let v = match sel_item {
                    SelectionMember::Variable(v) => {
                        if with_aggregate && !visible.contains(&v) {
                            // We validate projection variables if there is an aggregate
                            return Err("The SELECT contains a variable that is unbound");
                        }
                        v
                    }
                    SelectionMember::Expression(expression, variable) => {
                        if visible.contains(&variable) {
                            // We disallow to override an existing variable with an expression
                            return Err(
                                "The SELECT overrides an existing variable using an expression",
                            );
                        }
                        if with_aggregate && !are_variables_bound(&expression, &visible) {
                            // We validate projection variables if there is an aggregate
                            return Err(
                                "The SELECT contains an expression with a variable that is unbound",
                            );
                        }
                        p = GraphPattern::Extend {
                            inner: Box::new(p),
                            variable: variable.clone(),
                            expression,
                        };
                        variable
                    }
                };
                if pv.contains(&v) {
                    return Err("Duplicated variable name in SELECT");
                }
                pv.push(v)
            }
            true
        }
        SelectionVariables::Star => {
            if with_aggregate {
                return Err("SELECT * is not authorized with GROUP BY");
            }
            // TODO: is it really useful to do a projection?
            p.on_in_scope_variable(|v| {
                if !pv.contains(v) {
                    pv.push(v.clone());
                }
            });
            pv.sort();
            true
        }
    };

    let mut m = p;

    // ORDER BY
    if let Some(expression) = order_by {
        m = build_order_by(m, expression, state.preserve_source_literals);
    }

    // PROJECT
    if with_project {
        m = GraphPattern::Project {
            inner: Box::new(m),
            variables: pv,
        };
    }
    match select.option {
        SelectionOption::Distinct => m = GraphPattern::Distinct { inner: Box::new(m) },
        SelectionOption::Reduced => m = GraphPattern::Reduced { inner: Box::new(m) },
        SelectionOption::Default => (),
    }

    // OFFSET LIMIT
    if let Some((start, length)) = offset_limit {
        m = GraphPattern::Slice {
            inner: Box::new(m),
            start,
            length,
        }
    }
    Ok(m)
}

#[expect(clippy::too_many_arguments)]
fn build_sub_select(
    select: Selection,
    pattern: ParsedPattern,
    group: Option<ParsedGroupClause>,
    having: Option<ParsedHavingClause>,
    order: Option<Vec<ParsedOrderCondition>>,
    limit: Option<ParsedLimitOffset>,
    values: Option<ParsedPattern>,
    state: &mut ParserState,
) -> Result<ParsedPattern, &'static str> {
    if state.source_literal_hooks.is_some() {
        state.aggregates.pop();
        Ok(state
            .source_arena
            .select(select, pattern, group, having, order, limit, values, true))
    } else {
        build_select(select, pattern, group, having, order, limit, values, state)
            .map(ParsedPattern::Legacy)
    }
}

/// Source translation only; expression evaluation remains in the one compiler.
/// Fixed RDFLib algebra.py _sample/_aggs visits projection expressions, HAVING,
/// aggregate-bearing ORDER BY, then bare projected variables in that order.
struct SourceAggregateCollector {
    handles: HashMap<Variable, AggregateExpression>,
    aggregates: Vec<(Variable, AggregateExpression)>,
}

impl SourceAggregateCollector {
    fn append(&mut self, aggregate: AggregateExpression) -> Variable {
        let target = Variable::new_unchecked(format!("__agg_{}__", self.aggregates.len() + 1));
        self.aggregates.push((target.clone(), aggregate));
        target
    }

    fn sample(&mut self, variable: Variable) -> Variable {
        self.append(AggregateExpression::FunctionCall {
            name: AggregateFunction::Sample,
            expr: Expression::Variable(variable),
            distinct: false,
        })
    }

    fn contains_aggregate(&self, expression: &Expression) -> bool {
        let mut found = false;
        expression.lookup_used_variable(&mut |variable| {
            found |= self.handles.contains_key(variable);
        });
        found
    }

    fn rewrite(
        &mut self,
        expression: &mut Expression,
        excluded: Option<&Variable>,
    ) -> Result<(), &'static str> {
        match expression {
            Expression::Variable(variable) | Expression::Bound(variable) => {
                if let Some(aggregate) = self.handles.get(variable).cloned() {
                    // Aggregate operands are a source traversal boundary.
                    if let AggregateExpression::FunctionCall { expr, .. } = &aggregate {
                        if self.contains_aggregate(expr) {
                            return Err("Nested source aggregates are not supported");
                        }
                    }
                    *variable = self.append(aggregate);
                } else if Some(&*variable) != excluded {
                    *variable = self.sample(variable.clone());
                }
            }
            Expression::NamedNode(_) | Expression::Literal(_) | Expression::SourceLiteral(_) => (),
            Expression::Or(a, b)
            | Expression::And(a, b)
            | Expression::Equal(a, b)
            | Expression::SourceNotEqual(a, b)
            | Expression::SameTerm(a, b)
            | Expression::Greater(a, b)
            | Expression::GreaterOrEqual(a, b)
            | Expression::Less(a, b)
            | Expression::LessOrEqual(a, b)
            | Expression::Add(a, b)
            | Expression::Subtract(a, b)
            | Expression::Multiply(a, b)
            | Expression::Divide(a, b) => {
                self.rewrite(a, excluded)?;
                self.rewrite(b, excluded)?;
            }
            Expression::UnaryPlus(inner)
            | Expression::UnaryMinus(inner)
            | Expression::Not(inner) => {
                self.rewrite(inner, excluded)?;
            }
            Expression::In(first, tail) => {
                self.rewrite(first, excluded)?;
                for expression in tail {
                    self.rewrite(expression, excluded)?;
                }
            }
            Expression::If(condition, yes, no) => {
                self.rewrite(condition, excluded)?;
                self.rewrite(yes, excluded)?;
                self.rewrite(no, excluded)?;
            }
            Expression::Coalesce(expressions)
            | Expression::FunctionCall(_, expressions)
            | Expression::SourceFunction {
                arguments: expressions,
                ..
            } => {
                for expression in expressions {
                    self.rewrite(expression, excluded)?;
                }
            }
            Expression::AdditiveChain { first, tail } => {
                self.rewrite(first, excluded)?;
                for (_, expression) in tail {
                    self.rewrite(expression, excluded)?;
                }
            }
            Expression::MultiplicativeChain { first, tail } => {
                self.rewrite(first, excluded)?;
                for (_, expression) in tail {
                    self.rewrite(expression, excluded)?;
                }
            }
            Expression::Exists(_) | Expression::SourceExists(_) | Expression::SourceSyntax(_) => {
                return Err("Source aggregate rewriting of EXISTS requires its source phase arena");
            }
        }
        Ok(())
    }
}

/// The same modifier location is used for both policies; only source literals
/// retain raw ORDER expressions through the sealed private compiler.
fn build_order_by(
    inner: GraphPattern,
    expression: Vec<OrderExpression>,
    preserve_source_literals: bool,
) -> GraphPattern {
    if preserve_source_literals {
        GraphPattern::SourceOrderBy {
            inner: Box::new(inner),
            expression,
        }
    } else {
        GraphPattern::OrderBy {
            inner: Box::new(inner),
            expression,
        }
    }
}

#[expect(clippy::too_many_arguments)]
fn build_source_aggregate_select(
    mut select: Selection,
    inner: GraphPattern,
    group: Option<ParsedGroupClause>,
    mut having: Option<Expression>,
    mut order_by: Option<Vec<OrderExpression>>,
    offset_limit: Option<(usize, Option<usize>)>,
    values: Option<GraphPattern>,
    aggregates: Vec<(Variable, AggregateExpression)>,
    preserve_source_literals: bool,
    preserve_source_context: bool,
) -> Result<GraphPattern, &'static str> {
    let mut context_conditions = Vec::new();
    let grouping = match group {
        None => SourceGrouping::Implicit,
        Some(ParsedGroupClause::Source(conditions)) => SourceGrouping::Explicit(conditions),
        Some(ParsedGroupClause::SourceContext(conditions)) => {
            let mut grouping = Vec::with_capacity(conditions.len());
            for (condition, syntax) in conditions {
                grouping.push(condition);
                context_conditions.push(SourceGroupConditionContext {
                    syntax,
                    assignment_scope: None,
                });
            }
            SourceGrouping::Explicit(grouping)
        }
        Some(ParsedGroupClause::Legacy(..)) => {
            return Err("Source grouping requires its parser policy");
        }
    };
    let mut collector = SourceAggregateCollector {
        handles: aggregates.into_iter().collect(),
        aggregates: Vec::new(),
    };
    let SelectionVariables::Explicit(members) = &mut select.variables else {
        return Err("SELECT * is not supported with source aggregates");
    };
    for member in members.iter_mut() {
        if let SelectionMember::Expression(expression, target) = member {
            collector.rewrite(expression, Some(target))?;
        }
    }
    // complete=True makes the source HAVING traversal unconditional, even
    // when no explicit aggregate occurs in the HAVING expression.
    if let Some(expression) = &mut having {
        collector.rewrite(expression, None)?;
    }
    if let Some(expressions) = &mut order_by {
        if expressions.iter().any(|expression| {
            let (OrderExpression::Asc(expression) | OrderExpression::Desc(expression)) = expression;
            collector.contains_aggregate(expression)
        }) {
            for expression in expressions {
                let (OrderExpression::Asc(expression) | OrderExpression::Desc(expression)) =
                    expression;
                collector.rewrite(expression, None)?;
            }
        }
    }
    let mut aliases = Vec::new();
    for member in members.iter() {
        if let SelectionMember::Variable(variable) = member {
            aliases.push((collector.sample(variable.clone()), variable.clone()));
        }
    }
    let mut pattern = GraphPattern::SourceGroup {
        inner: Box::new(inner),
        grouping,
        aggregates: collector.aggregates,
    };
    if preserve_source_context {
        // Syntax is captured here; the final whole-plan pass supplies scopes.
        pattern = GraphPattern::SourceContext {
            inner: Box::new(pattern),
            annotation: SourceContextAnnotation::Group {
                conditions: context_conditions,
            },
        };
    }
    for (source, variable) in aliases {
        pattern = GraphPattern::Extend {
            inner: Box::new(pattern),
            variable,
            expression: Expression::Variable(source),
        };
    }
    if let Some(expr) = having {
        pattern = GraphPattern::Filter {
            expr,
            inner: Box::new(pattern),
        };
    }
    if let Some(values) = values {
        pattern = new_join(pattern, values);
    }
    let mut variables = Vec::new();
    for member in members {
        let variable = match member {
            SelectionMember::Variable(variable) => variable.clone(),
            SelectionMember::Expression(expression, variable) => {
                pattern = GraphPattern::Extend {
                    inner: Box::new(pattern),
                    variable: variable.clone(),
                    expression: expression.clone(),
                };
                variable.clone()
            }
        };
        if variables.contains(&variable) {
            return Err("Duplicated variable name in SELECT");
        }
        variables.push(variable);
    }
    if let Some(expression) = order_by {
        pattern = build_order_by(pattern, expression, preserve_source_literals);
    }
    pattern = GraphPattern::Project {
        inner: Box::new(pattern),
        variables,
    };
    match select.option {
        SelectionOption::Distinct => {
            pattern = GraphPattern::Distinct {
                inner: Box::new(pattern),
            }
        }
        SelectionOption::Reduced => {
            pattern = GraphPattern::Reduced {
                inner: Box::new(pattern),
            }
        }
        SelectionOption::Default => (),
    }
    if let Some((start, length)) = offset_limit {
        pattern = GraphPattern::Slice {
            inner: Box::new(pattern),
            start,
            length,
        };
    }
    Ok(pattern)
}

/// One genuine parser lowering. The neutral selection is unchanged; the
/// explicit compatibility profile projects original RDFLib modifier keys but
/// separately preserves exactly the selected Describe target variables.
fn build_describe(
    mut selection: Selection,
    r#where: ParsedPattern,
    group: Option<ParsedGroupClause>,
    having: Option<ParsedHavingClause>,
    order_by: Option<Vec<ParsedOrderCondition>>,
    offset_limit: Option<ParsedLimitOffset>,
    values: Option<ParsedPattern>,
    state: &mut ParserState,
) -> Result<GraphPattern, &'static str> {
    if !state.rdflib_describe {
        return build_select(
            selection,
            r#where,
            group,
            having,
            order_by,
            offset_limit,
            values,
            state,
        );
    }
    let r#where = r#where.legacy()?;
    let mut targets = Vec::new();
    match &selection.variables {
        SelectionVariables::Explicit(members) => {
            for member in members {
                let variable = match member {
                    SelectionMember::Variable(variable)
                    | SelectionMember::Expression(_, variable) => variable,
                };
                if !targets.contains(variable) {
                    targets.push(variable.clone());
                }
            }
        }
        SelectionVariables::Star => r#where.on_in_scope_variable(|variable| {
            if !targets.contains(variable) {
                targets.push(variable.clone());
            }
        }),
    }
    let mut keys = targets.clone();
    let mut pending = vec![&r#where];
    let mut visited = 0usize;
    while let Some(pattern) = pending.pop() {
        visited += 1;
        if visited > 100_000 {
            return Err("DESCRIBE modifier-key AST node budget");
        }
        match pattern {
            GraphPattern::Project { variables, .. } => {
                for variable in variables {
                    if !keys.contains(variable) {
                        keys.push(variable.clone());
                    }
                }
            }
            GraphPattern::Join { left, right }
            | GraphPattern::LeftJoin { left, right, .. }
            | GraphPattern::Union { left, right }
            | GraphPattern::Minus { left, right } => {
                pending.push(right);
                pending.push(left);
            }
            GraphPattern::Filter { inner, .. }
            | GraphPattern::Graph { inner, .. }
            | GraphPattern::Extend { inner, .. }
            | GraphPattern::OrderBy { inner, .. }
            | GraphPattern::SourceOrderBy { inner, .. }
            | GraphPattern::Distinct { inner }
            | GraphPattern::Reduced { inner }
            | GraphPattern::Slice { inner, .. }
            | GraphPattern::Group { inner, .. }
            | GraphPattern::SourceGroup { inner, .. }
            | GraphPattern::Service { inner, .. } => pending.push(inner),
            _ => (),
        }
    }
    let mut members = match selection.variables {
        SelectionVariables::Explicit(members) => members,
        SelectionVariables::Star => targets
            .iter()
            .cloned()
            .map(SelectionMember::Variable)
            .collect(),
    };
    for key in keys {
        if !targets.contains(&key) {
            members.push(SelectionMember::Variable(key));
        }
    }
    selection.variables = SelectionVariables::Explicit(members);
    selection.option = SelectionOption::Distinct;
    state.describe_targets = Some(targets);
    build_select(
        selection,
        r#where,
        group,
        having,
        order_by,
        offset_limit,
        values,
        state,
    )
}

fn are_variables_bound(expression: &Expression, variables: &HashSet<Variable>) -> bool {
    match expression {
        Expression::NamedNode(_)
        | Expression::Literal(_)
        | Expression::SourceLiteral(_)
        | Expression::Bound(_)
        | Expression::Coalesce(_)
        | Expression::Exists(_)
        | Expression::SourceExists(_)
        | Expression::SourceSyntax(_) => true,
        Expression::Variable(var) => variables.contains(var),
        Expression::UnaryPlus(e) | Expression::UnaryMinus(e) | Expression::Not(e) => {
            are_variables_bound(e, variables)
        }
        Expression::Or(a, b)
        | Expression::And(a, b)
        | Expression::Equal(a, b)
        | Expression::SourceNotEqual(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => {
            are_variables_bound(a, variables) && are_variables_bound(b, variables)
        }
        Expression::In(a, b) => {
            are_variables_bound(a, variables) && b.iter().all(|b| are_variables_bound(b, variables))
        }
        Expression::FunctionCall(_, parameters)
        | Expression::SourceFunction {
            arguments: parameters,
            ..
        } => parameters.iter().all(|p| are_variables_bound(p, variables)),
        Expression::If(a, b, c) => {
            are_variables_bound(a, variables)
                && are_variables_bound(b, variables)
                && are_variables_bound(c, variables)
        }
        Expression::AdditiveChain { first, tail } => {
            are_variables_bound(first, variables)
                && tail
                    .iter()
                    .all(|(_, operand)| are_variables_bound(operand, variables))
        }
        Expression::MultiplicativeChain { first, tail } => {
            are_variables_bound(first, variables)
                && tail
                    .iter()
                    .all(|(_, operand)| are_variables_bound(operand, variables))
        }
    }
}

/// Called on every variable defined using "AS" or "VALUES"
#[cfg(feature = "sep-0006")]
fn add_defined_variables<'a>(pattern: &'a GraphPattern, set: &mut HashSet<&'a Variable>) {
    match pattern {
        GraphPattern::Bgp { .. } | GraphPattern::Path { .. } => {}
        GraphPattern::Join { left, right }
        | GraphPattern::LeftJoin { left, right, .. }
        | GraphPattern::Lateral { left, right }
        | GraphPattern::Union { left, right }
        | GraphPattern::Minus { left, right } => {
            add_defined_variables(left, set);
            add_defined_variables(right, set);
        }
        GraphPattern::Graph { inner, .. } => {
            add_defined_variables(inner, set);
        }
        GraphPattern::Extend {
            inner, variable, ..
        } => {
            set.insert(variable);
            add_defined_variables(inner, set);
        }
        GraphPattern::Group {
            variables,
            aggregates,
            inner,
        } => {
            for (v, _) in aggregates {
                set.insert(v);
            }
            let mut inner_variables = HashSet::new();
            add_defined_variables(inner, &mut inner_variables);
            for v in inner_variables {
                if variables.contains(v) {
                    set.insert(v);
                }
            }
        }
        GraphPattern::SourceGroup { aggregates, .. } => {
            for (variable, _) in aggregates {
                set.insert(variable);
            }
        }
        GraphPattern::Values { variables, .. } | GraphPattern::SourceValues { variables, .. } => {
            for v in variables {
                set.insert(v);
            }
        }
        GraphPattern::Project { variables, inner } => {
            let mut inner_variables = HashSet::new();
            add_defined_variables(inner, &mut inner_variables);
            for v in inner_variables {
                if variables.contains(v) {
                    set.insert(v);
                }
            }
        }
        GraphPattern::Service { inner, .. }
        | GraphPattern::Filter { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::SourceOrderBy { inner, .. }
        | GraphPattern::SourceContext { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. } => add_defined_variables(inner, set),
    }
}

fn copy_graph(from: impl Into<GraphName>, to: impl Into<GraphNamePattern>) -> GraphUpdateOperation {
    let bgp = GraphPattern::Bgp {
        patterns: vec![TriplePattern::new(
            Variable::new_unchecked("s"),
            Variable::new_unchecked("p"),
            Variable::new_unchecked("o"),
        )],
    };
    GraphUpdateOperation::DeleteInsert {
        delete: Vec::new(),
        insert: vec![QuadPattern::new(
            Variable::new_unchecked("s"),
            Variable::new_unchecked("p"),
            Variable::new_unchecked("o"),
            to,
        )],
        using: None,
        pattern: Box::new(match from.into() {
            GraphName::NamedNode(from) => GraphPattern::Graph {
                name: from.into(),
                inner: Box::new(bgp),
            },
            GraphName::DefaultGraph => bgp,
        }),
    }
}

fn check_if_insert_data_are_sharing_blank_nodes(
    update: &[GraphUpdateOperation],
) -> Result<(), SparqlSyntaxError> {
    #[cfg(feature = "sparql-12")]
    fn add_triple_blank_nodes<'a>(triple: &'a Triple, bnodes: &mut HashSet<&'a BlankNode>) {
        if let NamedOrBlankNode::BlankNode(bnode) = &triple.subject {
            bnodes.insert(bnode);
        }
        if let Term::BlankNode(bnode) = &triple.object {
            bnodes.insert(bnode);
        } else if let Term::Triple(triple) = &triple.object {
            add_triple_blank_nodes(triple, bnodes);
        }
    }

    if update
        .iter()
        .filter(|op| matches!(op, GraphUpdateOperation::InsertData { .. }))
        .count()
        < 2
    {
        // Fast path, no need to validate
        return Ok(());
    }

    let mut existing_blank_nodes = HashSet::new();
    for operation in update {
        if let GraphUpdateOperation::InsertData { data } = operation {
            let mut new_blank_nodes = HashSet::new();
            for quad in data {
                if let NamedOrBlankNode::BlankNode(bnode) = &quad.subject {
                    new_blank_nodes.insert(bnode);
                }
                if let Term::BlankNode(bnode) = &quad.object {
                    new_blank_nodes.insert(bnode);
                }
                #[cfg(feature = "sparql-12")]
                if let Term::Triple(triple) = &quad.object {
                    add_triple_blank_nodes(triple, &mut new_blank_nodes);
                }
            }
            if let Some(error) = existing_blank_nodes.intersection(&new_blank_nodes).next() {
                return Err(SparqlSyntaxErrorKind::SharedBlankNode((**error).clone()).into());
            }
            existing_blank_nodes.extend(new_blank_nodes);
        }
    }
    Ok(())
}

enum Either<L, R> {
    Left(L),
    Right(R),
}

enum ParsedDataBlockValue {
    Ground(GroundTerm),
    Literal(ParsedSourceLiteral),
}

impl ParsedDataBlockValue {
    fn into_ground(self) -> GroundTerm {
        match self {
            Self::Ground(term) => term,
            Self::Literal(literal) => literal.rdf.into(),
        }
    }

    fn into_source(self) -> Result<SourceValueTerm, &'static str> {
        match self {
            Self::Ground(GroundTerm::NamedNode(node)) => Ok(SourceValueTerm::NamedNode(node)),
            Self::Literal(literal) => Ok(SourceValueTerm::Literal(literal)),
            // Triple values are not qualified by the source identity carrier.
            // Do not discard a nested literal's provenance via GroundTerm.
            Self::Ground(_) => Err("Source literal VALUES require named nodes or literals"),
        }
    }
}

pub struct ParserState {
    base_iri: Option<Iri<String>>,
    prefixes: HashMap<String, String>,
    custom_aggregate_functions: HashSet<NamedNode>,
    used_bnodes: HashSet<BlankNode>,
    currently_used_bnodes: HashSet<BlankNode>,
    aggregates: Vec<Vec<(Variable, AggregateExpression)>>,
    rdflib_describe: bool,
    preserve_arithmetic_chains: bool,
    preserve_source_groups: bool,
    preserve_source_literals: bool,
    preserve_source_context: bool,
    describe_targets: Option<Vec<Variable>>,
    numeric_constructor: Option<NumericLiteralConstructor>,
    source_literal_hooks: Option<SourceLiteralHooks>,
    source_arena: SourceArena,
    source_parse_error: Option<(usize, &'static str)>,
    source_blank_node_constructor: Option<SourceBlankNodeConstructor>,
    source_update_mode: bool,
    unresolved_update_prefix: Option<String>,
    source_predicates: HashMap<usize, VariableOrPropertyPath>,
    source_literal_occurrences: HashMap<(usize, usize), (SourceLiteralInput, ParsedSourceLiteral)>,
    numeric_construction_error: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ParserState {
    pub(crate) fn new(
        base_iri: Option<Iri<String>>,
        prefixes: HashMap<String, String>,
        custom_aggregate_functions: HashSet<NamedNode>,
    ) -> Self {
        Self {
            base_iri,
            prefixes,
            custom_aggregate_functions,
            used_bnodes: HashSet::new(),
            currently_used_bnodes: HashSet::new(),
            aggregates: Vec::new(),
            rdflib_describe: false,
            preserve_arithmetic_chains: false,
            preserve_source_groups: false,
            preserve_source_literals: false,
            preserve_source_context: false,
            describe_targets: None,
            numeric_constructor: None,
            source_literal_hooks: None,
            source_arena: SourceArena::default(),
            source_parse_error: None,
            source_blank_node_constructor: None,
            source_update_mode: false,
            unresolved_update_prefix: None,
            source_predicates: HashMap::new(),
            source_literal_occurrences: HashMap::new(),
            numeric_construction_error: None,
        }
    }

    fn expression_literal(&self, literal: ParsedSourceLiteral) -> Expression {
        if self.preserve_source_literals {
            Expression::SourceLiteral(literal)
        } else {
            Expression::Literal(literal.rdf)
        }
    }

    fn pattern_literal(&self, literal: ParsedSourceLiteral) -> TermPattern {
        // Source occurrence identity belongs to the actual parse owner. The
        // legacy and earlier non-source frontends keep their physical terms.
        if self.source_literal_hooks.is_some() {
            TermPattern::SourceLiteral(literal)
        } else {
            TermPattern::Literal(literal.rdf)
        }
    }

    fn capture_source_graph_syntax(&self) -> bool {
        self.source_literal_hooks
            .as_ref()
            .is_some_and(|hooks| hooks.triples_order.is_some())
    }

    fn source_native_paths(&self) -> bool {
        self.source_literal_hooks
            .as_ref()
            .is_some_and(|hooks| hooks.native_triples_order.is_some())
    }

    fn source_negated_path(&mut self, items: Vec<Either<NamedNode, NamedNode>>) -> ParsedPath {
        let mut direct = Vec::new();
        let mut inverse = Vec::new();
        let mut parts = Vec::new();
        for item in items {
            match item {
                Either::Left(node) => {
                    parts.push(crate::SourceSyntaxValue::NamedNode(node.clone()));
                    direct.push(node);
                }
                Either::Right(node) => {
                    // The source parser's InversePath has no mapping fields.
                    // Its consumed IRI is not a translated InvPath operand.
                    parts.push(self.source_arena.path_node("InversePath", Vec::new()));
                    inverse.push(node);
                }
            }
        }
        let syntax = self.source_path_node(
            "PathNegatedPropertySet",
            vec![("part", crate::SourceSyntaxValue::List(parts))],
        );
        let expression = if inverse.is_empty() {
            PropertyPathExpression::NegatedPropertySet(direct)
        } else if direct.is_empty() {
            PropertyPathExpression::Reverse(Box::new(PropertyPathExpression::NegatedPropertySet(
                inverse,
            )))
        } else {
            PropertyPathExpression::Alternative(
                Box::new(PropertyPathExpression::NegatedPropertySet(direct)),
                Box::new(PropertyPathExpression::Reverse(Box::new(
                    PropertyPathExpression::NegatedPropertySet(inverse),
                ))),
            )
        };
        ParsedPath { expression, syntax }
    }

    fn source_path_node(
        &mut self,
        name: &str,
        fields: Vec<(&str, crate::SourceSyntaxValue)>,
    ) -> Option<crate::SourceSyntaxValue> {
        self.capture_source_graph_syntax()
            .then(|| self.source_arena.path_node(name, fields))
    }

    fn source_graph_term(term: &TermPattern) -> Option<crate::SourceSyntaxValue> {
        use crate::SourceSyntaxValue as V;
        Some(match term {
            TermPattern::Variable(term) => V::Variable(term.clone()),
            TermPattern::NamedNode(term) => V::NamedNode(term.clone()),
            TermPattern::BlankNode(term) => V::BlankNode(term.clone()),
            TermPattern::SourceLiteral(term) => V::Literal(term.clone()),
            // A lexical RDF value cannot supply a missing parse occurrence.
            _ => return None,
        })
    }

    fn source_triples_row(
        &self,
        subject: &TermPattern,
        properties: &[(ParsedVerb, Vec<ReifiedTerm>)],
    ) -> Option<Vec<crate::SourceSyntaxValue>> {
        self.source_expanded_row(subject, None, properties)
    }

    fn source_literal_occurrence(
        &mut self,
        start: usize,
        end: usize,
        input: SourceLiteralInput,
    ) -> Result<ParsedSourceLiteral, &'static str> {
        let Some(hooks) = &self.source_literal_hooks else {
            let SourceLiteralInput::Parsed(literal) = input else {
                return Err("Numeric occurrence requires the source constructor");
            };
            return Ok(literal);
        };
        if self.numeric_construction_error.is_some() {
            return Err("Source literal construction already failed");
        }
        if let Some((previous, literal)) = self.source_literal_occurrences.get(&(start, end)) {
            if previous != &input {
                self.numeric_construction_error = Some(
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "A source literal span was reused with a different constructor input",
                    )
                    .into(),
                );
                return Err("A source literal span was reused with a different constructor input");
            }
            return Ok(literal.clone());
        }
        let index = self.source_literal_occurrences.len();
        index
            .checked_add(1)
            .ok_or("Source literal occurrence index overflow")?;
        let occurrence = hooks.scope.occurrence(index);
        let expected = input.clone();
        match (hooks.construct)(occurrence.clone(), input) {
            Ok(mut literal) => {
                let valid = literal.occurrence.is_none()
                    && match &expected {
                        SourceLiteralInput::Parsed(expected) => &literal == expected,
                        SourceLiteralInput::NumericToken {
                            kind,
                            sign,
                            magnitude,
                        } => {
                            literal.origin
                                == SourceLiteralOrigin::QueryNumericToken {
                                    kind: *kind,
                                    sign: *sign,
                                    magnitude: magnitude.clone(),
                                }
                                && literal.language_original.is_none()
                                && literal.datatype
                                    == SourceDatatype::Specified(
                                        literal.rdf.datatype().into_owned(),
                                    )
                        }
                    };
                if !valid {
                    self.numeric_construction_error = Some(
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Source literal callback returned a mismatched origin or occurrence",
                        )
                        .into(),
                    );
                    return Err(
                        "Source literal callback returned a mismatched origin or occurrence",
                    );
                }
                literal.occurrence = Some(occurrence);
                self.source_literal_occurrences
                    .insert((start, end), (expected, literal.clone()));
                Ok(literal)
            }
            Err(error) => {
                self.numeric_construction_error = Some(error);
                Err("Source literal construction failed")
            }
        }
    }

    fn numeric_literal(
        &mut self,
        kind: NumericTokenKind,
        sign: NumericTokenSign,
        spelling: &str,
        start: usize,
        end: usize,
    ) -> Result<ParsedSourceLiteral, &'static str> {
        if self.source_literal_hooks.is_some() {
            let magnitude = match sign {
                NumericTokenSign::Unsigned => spelling,
                NumericTokenSign::Positive | NumericTokenSign::Negative => {
                    let magnitude = &spelling[1..];
                    if !magnitude
                        .as_bytes()
                        .first()
                        .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
                    {
                        return Err("A signed numeric token cannot contain separating whitespace");
                    }
                    magnitude
                }
            };
            return self.source_literal_occurrence(
                start,
                end,
                SourceLiteralInput::NumericToken {
                    kind,
                    sign,
                    magnitude: magnitude.to_owned(),
                },
            );
        }
        // This is the sole callback invocation for this legacy grammar production.
        let rdf = self.numeric_rdf_literal(kind, sign, spelling)?;
        let magnitude = match sign {
            NumericTokenSign::Unsigned => spelling,
            NumericTokenSign::Positive | NumericTokenSign::Negative => &spelling[1..],
        };
        Ok(ParsedSourceLiteral {
            occurrence: None,
            datatype: SourceDatatype::Specified(rdf.datatype().into_owned()),
            rdf,
            language_original: None,
            origin: SourceLiteralOrigin::QueryNumericToken {
                kind,
                sign,
                magnitude: magnitude.to_owned(),
            },
        })
    }

    fn numeric_rdf_literal(
        &mut self,
        kind: NumericTokenKind,
        sign: NumericTokenSign,
        spelling: &str,
    ) -> Result<Literal, &'static str> {
        let Some(constructor) = &self.numeric_constructor else {
            return Ok(Literal::new_typed_literal(
                spelling,
                match kind {
                    NumericTokenKind::Integer => xsd::INTEGER,
                    NumericTokenKind::Decimal => xsd::DECIMAL,
                    NumericTokenKind::Double => xsd::DOUBLE,
                },
            ));
        };
        if self.numeric_construction_error.is_some() {
            return Err("Numeric token construction already failed");
        }
        let magnitude = match sign {
            NumericTokenSign::Unsigned => spelling,
            NumericTokenSign::Positive | NumericTokenSign::Negative => {
                let magnitude = &spelling[1..];
                // Original signed token productions use leave_whitespace.
                // Unary-expression spacing remains governed by its own rule.
                if !magnitude
                    .as_bytes()
                    .first()
                    .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
                {
                    return Err("A signed numeric token cannot contain separating whitespace");
                }
                magnitude
            }
        };
        match constructor(kind, sign, magnitude) {
            Ok(literal) => Ok(literal),
            Err(error) => {
                // Python constructor TypeError is not pyparsing's recoverable
                // ParseException. Retain the first fatal constructor failure.
                self.numeric_construction_error = Some(error);
                Err("Numeric token literal construction failed")
            }
        }
    }

    fn parse_iri(&self, iri: String) -> Result<Iri<String>, IriParseError> {
        if let Some(base_iri) = &self.base_iri {
            base_iri.resolve(&iri)
        } else {
            Iri::parse(iri)
        }
    }

    fn new_aggregation(&mut self, parsed: ParsedAggregate) -> Result<Variable, &'static str> {
        let agg = parsed.aggregate.clone();
        if self.source_literal_hooks.is_some() {
            let new_var = variable();
            self.source_arena
                .register_aggregate(new_var.clone(), parsed);
            self.aggregates
                .last_mut()
                .ok_or("Unexpected aggregate")?
                .push((new_var.clone(), agg));
            return Ok(new_var);
        }
        let aggregates = self.aggregates.last_mut().ok_or("Unexpected aggregate")?;
        if self.preserve_source_groups {
            let new_var = variable();
            aggregates.push((new_var.clone(), agg));
            return Ok(new_var);
        }
        Ok(aggregates
            .iter()
            .find_map(|(v, a)| (a == &agg).then_some(v))
            .cloned()
            .unwrap_or_else(|| {
                let new_var = variable();
                aggregates.push((new_var.clone(), agg));
                new_var
            }))
    }
}

fn unescape_iriref(mut input: &str) -> Result<String, &'static str> {
    let mut output = String::with_capacity(input.len());
    while let Some((before, after)) = input.split_once('\\') {
        output.push_str(before);
        let mut after = after.chars();
        let (escape, after) = match after.next() {
            Some('u') => read_hex_char::<4>(after.as_str())?,
            Some('U') => read_hex_char::<8>(after.as_str())?,
            Some(_) => {
                return Err(
                    "IRIs are only allowed to contain escape sequences \\uXXXX and \\UXXXXXXXX",
                );
            }
            None => return Err("IRIs are not allowed to end with a '\'"),
        };
        output.push(escape);
        input = after;
    }
    output.push_str(input);
    Ok(output)
}

fn unescape_string(mut input: &str) -> Result<String, &'static str> {
    let mut output = String::with_capacity(input.len());
    while let Some((before, after)) = input.split_once('\\') {
        output.push_str(before);
        let mut after = after.chars();
        let (escape, after) = match after.next() {
            Some('t') => ('\u{0009}', after.as_str()),
            Some('b') => ('\u{0008}', after.as_str()),
            Some('n') => ('\u{000A}', after.as_str()),
            Some('r') => ('\u{000D}', after.as_str()),
            Some('f') => ('\u{000C}', after.as_str()),
            Some('"') => ('\u{0022}', after.as_str()),
            Some('\'') => ('\u{0027}', after.as_str()),
            Some('\\') => ('\u{005C}', after.as_str()),
            Some('u') => read_hex_char::<4>(after.as_str())?,
            Some('U') => read_hex_char::<8>(after.as_str())?,
            Some(_) => return Err("The character that can be escaped in strings are tbnrf\"'\\"),
            None => return Err("strings are not allowed to end with a '\'"),
        };
        output.push(escape);
        input = after;
    }
    output.push_str(input);
    Ok(output)
}

fn read_hex_char<const SIZE: usize>(input: &str) -> Result<(char, &str), &'static str> {
    if let Some(escape) = input.get(..SIZE) {
        if let Some(char) = u32::from_str_radix(escape, 16)
            .ok()
            .and_then(char::from_u32)
        {
            Ok((char, &input[SIZE..]))
        } else {
            Err("\\u escape sequence should be followed by hexadecimal digits")
        }
    } else {
        Err("\\u escape sequence should be followed by hexadecimal digits")
    }
}

fn variable() -> Variable {
    Variable::new_unchecked(format!("{:x}", random::<u128>()))
}

parser! {
    //See https://www.w3.org/TR/turtle/#sec-grammar
    grammar parser(state: &mut ParserState) for str {
        pub rule QueryUnit() -> Query = Query()

        pub rule SourceQueryUnit() -> Query = q:Query() at:position!() SourceEnd(at) { q }
        rule SourceEnd(at: usize) = ![_] / {?
            state.source_parse_error = Some((at, "Expected end of text"));
            Err("end of text")
        }
        rule SourceGraphMode() = quiet!{ {? if state.capture_source_graph_syntax() { Ok(()) } else { Err("source graph mode") } } }
        rule LegacyGraphMode() = quiet!{ {? if !state.capture_source_graph_syntax() { Ok(()) } else { Err("legacy graph mode") } } }

        rule Query() -> Query = _ Prologue() _ q:(SelectQuery() / ConstructQuery() / DescribeQuery() / AskQuery() / SourceMissingQueryForm()) _ {
            state.source_parse_error = None;
            q
        }

        // Actual source grammar checkpoint: no query form starts here. Do not
        // replace a diagnostic inside a recognized form or the native parser.
        rule SourceMissingQueryForm() -> Query = SourceGraphMode() at:position!()
            !(i("SELECT") / i("CONSTRUCT") / i("DESCRIBE") / i("ASK")) {?
                state.source_parse_error = Some((at, "Expected {SelectQuery | ConstructQuery | DescribeQuery | AskQuery}"));
                Err("source query form")
            }

        pub rule UpdateInit() -> Vec<GraphUpdateOperation> = Update()

        // Keep source operations and prologue boundaries intact. The ordinary
        // UpdateInit above retains its native desugaring and validity rules.
        pub rule SourceUpdateInit() -> Vec<SourceUpdateOperation> =
            _ u:(SourceUpdateItem() ** (";" _)) (";" _)? Prologue() _ at:position!() SourceEnd(at) { u }
        rule SourceUpdateItem() -> SourceUpdateOperation =
            Prologue() _ kind:SourceUpdate1() Update_clear() _ {
                SourceUpdateOperation { kind, base_iri: state.base_iri.clone() }
            }
        rule SourceUpdate1() -> SourceUpdateOperationKind =
            SourceUpdateLoad() / SourceUpdateClear() / SourceUpdateDrop() /
            SourceUpdateAdd() / SourceUpdateMove() / SourceUpdateCopy() /
            SourceUpdateCreate() / SourceUpdateInsertData() / SourceUpdateDeleteData() /
            SourceUpdateDeleteWhere() / SourceUpdateModify()
        rule SourceUpdateLoad() -> SourceUpdateOperationKind =
            i("LOAD") _ silent:Update1_silent() _ source:iri() _ destination:Load_to()? {
                SourceUpdateOperationKind::Load { silent, source, destination }
            }
        rule SourceUpdateClear() -> SourceUpdateOperationKind =
            i("CLEAR") _ silent:Update1_silent() _ graph:GraphRefAll() {
                SourceUpdateOperationKind::Clear { silent, graph }
            }
        rule SourceUpdateDrop() -> SourceUpdateOperationKind =
            i("DROP") _ silent:Update1_silent() _ graph:GraphRefAll() {
                SourceUpdateOperationKind::Drop { silent, graph }
            }
        rule SourceUpdateCreate() -> SourceUpdateOperationKind =
            i("CREATE") _ silent:Update1_silent() _ graph:GraphRef() {
                SourceUpdateOperationKind::Create { silent, graph }
            }
        rule SourceUpdateAdd() -> SourceUpdateOperationKind =
            i("ADD") _ silent:Update1_silent() _ from:GraphOrDefault() _ i("TO") _ to:GraphOrDefault() {
                SourceUpdateOperationKind::Add { silent, from, to }
            }
        rule SourceUpdateMove() -> SourceUpdateOperationKind =
            i("MOVE") _ silent:Update1_silent() _ from:GraphOrDefault() _ i("TO") _ to:GraphOrDefault() {
                SourceUpdateOperationKind::Move { silent, from, to }
            }
        rule SourceUpdateCopy() -> SourceUpdateOperationKind =
            i("COPY") _ silent:Update1_silent() _ from:GraphOrDefault() _ i("TO") _ to:GraphOrDefault() {
                SourceUpdateOperationKind::Copy { silent, from, to }
            }
        rule SourceUpdateInsertData() -> SourceUpdateOperationKind =
            i("INSERT") _ i("DATA") _ data:SourceUpdateTemplate() { SourceUpdateOperationKind::InsertData(data) }
        rule SourceUpdateDeleteData() -> SourceUpdateOperationKind =
            i("DELETE") _ i("DATA") _ data:SourceUpdateTemplate() { SourceUpdateOperationKind::DeleteData(data) }
        rule SourceUpdateDeleteWhere() -> SourceUpdateOperationKind =
            i("DELETE") _ i("WHERE") _ data:SourceUpdateTemplate() { SourceUpdateOperationKind::DeleteWhere(data) }
        rule SourceUpdateModify() -> SourceUpdateOperationKind =
            with:Modify_with()? _ clauses:SourceUpdateClauses() _ using:(UsingClause() ** (_)) _
            i("WHERE") _ parsed:GroupGraphPattern() {?
                let pattern = state.source_update_where(parsed)?;
                Ok(SourceUpdateOperationKind::Modify {
                    delete: clauses.0, insert: clauses.1, with, using, pattern: Box::new(pattern)
                })
            }
        rule SourceUpdateClauses() -> (Option<SourceUpdateData>, Option<SourceUpdateData>) =
            d:SourceUpdateDeleteClause() Modify_clear() _ i:SourceUpdateInsertClause()? Modify_clear() { (Some(d), i) } /
            i:SourceUpdateInsertClause() Modify_clear() { (None, Some(i)) }
        rule SourceUpdateDeleteClause() -> SourceUpdateData = i("DELETE") _ data:SourceUpdateTemplate() { data }
        rule SourceUpdateInsertClause() -> SourceUpdateData = i("INSERT") _ data:SourceUpdateTemplate() { data }
        rule SourceUpdateTemplate() -> SourceUpdateData =
            "{" _ first:SourceUpdateTriples()? _ rest:SourceUpdateNamedBlock()* "}" {?
                state.source_update_data(first.unwrap_or_default(), rest)
            }
        rule SourceUpdateNamedBlock() -> (GraphNamePattern, Vec<TriplePattern>, Vec<TriplePattern>) =
            i("GRAPH") _ name:VarOrIri() _ "{" _ triples:SourceUpdateTriples()? _ "}" _ "."? _ tail:SourceUpdateTriples()? _ {
                (name.into(), triples.unwrap_or_default(), tail.unwrap_or_default())
            }
        rule SourceUpdateTriples() -> Vec<TriplePattern> =
            triples:(SourceUpdateTripleRow() ++ (".")) ("." _)? { triples.into_iter().flatten().collect() }
        rule SourceUpdateTripleRow() -> Vec<TriplePattern> =
            _ triples:TriplesSameSubjectPath() _ {? ParserState::source_update_plain_triples(triples) }

        rule Prologue() = (BaseDecl() _ / PrefixDecl() _ / VersionDecl() _)* {}

        rule BaseDecl() = i("BASE") _ i:IRIREF() {
            state.base_iri = Some(i)
        }

        rule PrefixDecl() = i("PREFIX") _ ns:PNAME_NS() _ i:IRIREF() {
            state.prefixes.insert(ns.into(), i.into_inner());
        }

        rule VersionDecl() = i("VERSION") _ VersionSpecifier() {?
            if cfg!(feature = "sparql-12") {
                Ok(())
            } else {
                Err("The VERSION declaration is only supported in SPARQL 1.2")
            }
        }

        rule VersionSpecifier() = STRING_LITERAL1() / STRING_LITERAL2() {}

        rule SelectQuery() -> Query = s:SelectClause() _ d:DatasetClauses() _ w:WhereClause() _ g:GroupClause()? _ h:HavingClause()? _ o:OrderClause()? _ l:LimitOffsetClauses()? _ v:ValuesClause() {?
            Ok(Query::Select {
                dataset: d,
                pattern: build_select(s, w, g, h, o, l, v, state)?,
                base_iri: state.base_iri.clone()
            })
        }

        rule SubSelect() -> ParsedPattern = s:SelectClause() _ w:WhereClause() _ g:GroupClause()? _ h:HavingClause()? _ o:OrderClause()? _ l:LimitOffsetClauses()? _ v:ValuesClause() {?
            build_sub_select(s, w, g, h, o, l, v, state)
        }

        rule SelectClause() -> Selection = i("SELECT") _ Selection_init() o:SelectClause_option() _ v:SelectClause_variables() {
            Selection {
                option: o,
                variables: v
            }
        }
        rule Selection_init() = {
            state.aggregates.push(Vec::new())
        }
        rule SelectClause_option() -> SelectionOption =
            i("DISTINCT") { SelectionOption::Distinct } /
            i("REDUCED") { SelectionOption::Reduced } /
            { SelectionOption::Default }
        rule SelectClause_variables() -> SelectionVariables =
            "*" { SelectionVariables::Star } /
            p:SelectClause_member()+ { SelectionVariables::Explicit(p) }
        rule SelectClause_member() -> SelectionMember =
            v:Var() _ { SelectionMember::Variable(v) } /
            "(" _ e:Expression() _ i("AS") _ v:Var() _ ")" _ { SelectionMember::Expression(e, v) }

        rule ConstructQuery() -> Query =
            i("CONSTRUCT") _ c:ConstructTemplate() ConstructQuery_clear() _ d:DatasetClauses() _ w:WhereClause() _ g:GroupClause()? _ h:HavingClause()? _ o:OrderClause()? _ l:LimitOffsetClauses()? _ v:ValuesClause() {?
                Ok(Query::Construct {
                    template: c,
                    dataset: d,
                    pattern: build_select(Selection::default(), w, g, h, o, l, v, state)?,
                    base_iri: state.base_iri.clone()
                })
            } /
            i("CONSTRUCT") _ d:DatasetClauses() _ i("WHERE") _ "{" _ c:ConstructQuery_optional_triple_template() _ "}" _ g:GroupClause()? _ h:HavingClause()? _ o:OrderClause()? _ l:LimitOffsetClauses()? _ v:ValuesClause() {?
                // The shorthand is also an actual group. A source TriplesBlock
                // used as the group root has no `part` and would become empty.
                let body: ParsedPattern = GraphPattern::Bgp { patterns: c.clone() }.into();
                let body = if state.source_literal_hooks.is_some() {
                    state.source_arena.group(Some(body), Vec::new())
                } else { body };
                Ok(Query::Construct {
                    template: c.clone(),
                    dataset: d,
                    pattern: build_select(
                        Selection::default(),
                        body,
                        g, h, o, l, v, state
                    )?,
                    base_iri: state.base_iri.clone()
                })
            }
        rule ConstructQuery_clear() = {
            state.currently_used_bnodes.clear();
        }

        rule ConstructQuery_optional_triple_template() -> Vec<TriplePattern> = TriplesTemplate() / { Vec::new() }

        rule DescribeQuery() -> Query =
            i("DESCRIBE") _ "*" _ d:DatasetClauses() _ w:WhereClause()? _ g:GroupClause()? _ h:HavingClause()? _ o:OrderClause()? _ l:LimitOffsetClauses()? _ v:ValuesClause() {?
                Ok(Query::Describe {
                    dataset: d,
                    pattern: build_describe(Selection::default(), w.unwrap_or_default(), g, h, o, l, v, state)?,
                    base_iri: state.base_iri.clone()
                })
            } /
            i("DESCRIBE") _ p:DescribeQuery_item()+ _ d:DatasetClauses() _ w:WhereClause()? _ g:GroupClause()? _ h:HavingClause()? _ o:OrderClause()? _ l:LimitOffsetClauses()? _ v:ValuesClause() {?
                Ok(Query::Describe {
                    dataset: d,
                    pattern: build_describe(Selection {
                        option: SelectionOption::Default,
                        variables: SelectionVariables::Explicit(p.into_iter().map(|var_or_iri| match var_or_iri {
                            NamedNodePattern::NamedNode(n) => SelectionMember::Expression(n.into(), variable()),
                            NamedNodePattern::Variable(v) => SelectionMember::Variable(v)
                        }).collect())
                    }, w.unwrap_or_default(), g, h, o, l, v, state)?,
                    base_iri: state.base_iri.clone()
                })
            }
        rule DescribeQuery_item() -> NamedNodePattern = i:VarOrIri() _ { i }

        rule AskQuery() -> Query = i("ASK") _ d:DatasetClauses() _ w:WhereClause() _ g:GroupClause()? _ h:HavingClause()? _ o:OrderClause()? _ l:LimitOffsetClauses()? _ v:ValuesClause() {?
            Ok(Query::Ask {
                dataset: d,
                pattern: build_select(Selection::default(), w, g, h, o, l, v, state)?,
                base_iri: state.base_iri.clone()
            })
        }

        rule DatasetClause() -> (Option<NamedNode>, Option<NamedNode>) = i("FROM") _ d:(DefaultGraphClause() / NamedGraphClause()) { d }
        rule DatasetClauses() -> Option<QueryDataset> = d:DatasetClause() ** (_) {
            if d.is_empty() {
                return None;
            }
            let mut default = Vec::new();
            let mut named = Vec::new();
            for (d, n) in d {
                if let Some(d) = d {
                    default.push(d);
                }
                if let Some(n) = n {
                    named.push(n);
                }
            }
            Some(QueryDataset {
                default, named: Some(named)
            })
        }

        rule DefaultGraphClause() -> (Option<NamedNode>, Option<NamedNode>) = s:SourceSelector() {
            (Some(s), None)
        }

        rule NamedGraphClause() -> (Option<NamedNode>, Option<NamedNode>) = i("NAMED") _ s:SourceSelector() {
            (None, Some(s))
        }

        rule SourceSelector() -> NamedNode = iri()

        rule WhereClause() -> ParsedPattern = i("WHERE")? _ p:GroupGraphPattern() {
            p
        }

        rule GroupClause() -> ParsedGroupClause = i("GROUP") _ i("BY") _ c:GroupCondition_item()+ {
            if state.preserve_source_groups || state.source_literal_hooks.is_some() {
                if state.preserve_source_context {
                    ParsedGroupClause::SourceContext(c.into_iter().map(|(expression, variable, syntax)| {
                        let condition = match variable {
                            Some(variable) => SourceGroupCondition::As { expression, variable },
                            None => SourceGroupCondition::Raw(expression),
                        };
                        (condition, syntax)
                    }).collect())
                } else {
                    ParsedGroupClause::Source(c.into_iter().map(|(expression, variable, _)| {
                        match variable {
                            Some(variable) => SourceGroupCondition::As { expression, variable },
                            None => SourceGroupCondition::Raw(expression),
                        }
                    }).collect())
                }
            } else {
            let mut projections: Vec<(Expression,Variable)> = Vec::new();
            let clauses = c.into_iter().map(|(e, vo, _)| {
                if let (Expression::Variable(v), None) = (&e, &vo) {
                    v.clone()
                } else {
                    let v = vo.unwrap_or_else(variable);
                    projections.push((e, v.clone()));
                    v
                }
            }).collect();
            ParsedGroupClause::Legacy(clauses, projections)
            }
        }
        rule GroupCondition_item() -> (Expression, Option<Variable>, SourceGroupConditionSyntax) = c:GroupCondition() _ { c }

        rule GroupCondition() -> (Expression, Option<Variable>, SourceGroupConditionSyntax) =
            e:BuiltInCall() { (e, None, SourceGroupConditionSyntax::BareFunction) } /
            e:FunctionCall() { (e, None, SourceGroupConditionSyntax::BareFunction) } /
            "(" _ e:Expression() _ v:GroupCondition_as()? ")" {
                let syntax = if v.is_some() { SourceGroupConditionSyntax::ParenthesizedAlias } else { SourceGroupConditionSyntax::ParenthesizedWithoutAlias };
                (e, v, syntax)
            } /
            e:Var() { (e.into(), None, SourceGroupConditionSyntax::BareVariable) }
        rule GroupCondition_as() -> Variable = i("AS") _ v:Var() _ { v }

        rule HavingClause() -> ParsedHavingClause = i("HAVING") _ e:HavingCondition()+ { ParsedHavingClause(e) }

        rule HavingCondition() -> Expression = c:Constraint() _ { c }

        rule OrderClause() -> Vec<ParsedOrderCondition> = i("ORDER") _ i("BY") _ c:OrderClause_item()+ { c }
        rule OrderClause_item() -> ParsedOrderCondition = c:OrderCondition() _ { c }

        rule OrderCondition() -> ParsedOrderCondition =
            i("ASC") _ e:BrackettedExpression() { ParsedOrderCondition{expression:e,order:Some("ASC")} } /
            i("DESC") _ e:BrackettedExpression() { ParsedOrderCondition{expression:e,order:Some("DESC")} } /
            e:Constraint() { ParsedOrderCondition{expression:e,order:None} } /
            v:Var() { ParsedOrderCondition{expression:v.into(),order:None} }

        rule LimitOffsetClauses() -> ParsedLimitOffset =
            l:LimitClause() _ o:OffsetClause()? { ParsedLimitOffset{offset:o,limit:Some(l),limit_first:true} } /
            o:OffsetClause() _ l:LimitClause()? { ParsedLimitOffset{offset:Some(o),limit:l,limit_first:false} }

        rule LimitClause() -> ParsedLimitValue = i("LIMIT") _ start:position!() l:$(INTEGER()) end:position!() {?
            let value=usize::from_str(l).map_err(|_| "The query limit should be a non negative integer")?;
            let literal=if state.source_literal_hooks.is_some(){Some(state.numeric_literal(NumericTokenKind::Integer,NumericTokenSign::Unsigned,l,start,end)?)}else{None};
            Ok(ParsedLimitValue{value,literal})
        }

        rule OffsetClause() -> ParsedLimitValue = i("OFFSET") _ start:position!() o:$(INTEGER()) end:position!() {?
            let value=usize::from_str(o).map_err(|_| "The query offset should be a non negative integer")?;
            let literal=if state.source_literal_hooks.is_some(){Some(state.numeric_literal(NumericTokenKind::Integer,NumericTokenSign::Unsigned,o,start,end)?)}else{None};
            Ok(ParsedLimitValue{value,literal})
        }

        rule ValuesClause() -> Option<ParsedPattern> =
            i("VALUES") _ p:DataBlock() { Some(p) } /
            { None }

        rule Update() -> Vec<GraphUpdateOperation> = _ Prologue() _ u:(Update_item() ** (";" _))  ( ";" _)? { u.into_iter().flatten().collect() }
        rule Update_item() -> Vec<GraphUpdateOperation> = u:Update1() Update_clear() _ { u }
        rule Update_clear() = {
            state.used_bnodes.clear();
            state.currently_used_bnodes.clear();
        }

        rule Update1() -> Vec<GraphUpdateOperation> = Load() / Clear() / Drop() / Add() / Move() / Copy() / Create() / InsertData() / DeleteData() / DeleteWhere() / Modify()
        rule Update1_silent() -> bool = i("SILENT") { true } / { false }

        rule Load() -> Vec<GraphUpdateOperation> = i("LOAD") _ silent:Update1_silent() _ source:iri() _ destination:Load_to()? {
            vec![GraphUpdateOperation::Load { silent, source, destination: destination.map_or(GraphName::DefaultGraph, GraphName::NamedNode) }]
        }
        rule Load_to() -> NamedNode = i("INTO") _ g: GraphRef() { g }

        rule Clear() -> Vec<GraphUpdateOperation> = i("CLEAR") _ silent:Update1_silent() _ graph:GraphRefAll() {
            vec![GraphUpdateOperation::Clear { silent, graph }]
        }

        rule Drop() -> Vec<GraphUpdateOperation> = i("DROP") _ silent:Update1_silent() _ graph:GraphRefAll() {
            vec![GraphUpdateOperation::Drop { silent, graph }]
        }

        rule Create() -> Vec<GraphUpdateOperation> = i("CREATE") _ silent:Update1_silent() _ graph:GraphRef() {
            vec![GraphUpdateOperation::Create { silent, graph }]
        }

        rule Add() -> Vec<GraphUpdateOperation> = i("ADD") _ silent:Update1_silent() _ from:GraphOrDefault() _ i("TO") _ to:GraphOrDefault() {
            // Rewriting defined by https://www.w3.org/TR/sparql11-update/#add
            if from == to {
                Vec::new() // identity case
            } else {
                let bgp = GraphPattern::Bgp { patterns: vec![TriplePattern::new(Variable::new_unchecked("s"), Variable::new_unchecked("p"), Variable::new_unchecked("o"))] };
                vec![copy_graph(from, to)]
            }
        }

        rule Move() -> Vec<GraphUpdateOperation> = i("MOVE") _ silent:Update1_silent() _ from:GraphOrDefault() _ i("TO") _ to:GraphOrDefault() {
            // Rewriting defined by https://www.w3.org/TR/sparql11-update/#move
            if from == to {
                Vec::new() // identity case
            } else {
                let bgp = GraphPattern::Bgp { patterns: vec![TriplePattern::new(Variable::new_unchecked("s"), Variable::new_unchecked("p"), Variable::new_unchecked("o"))] };
                vec![GraphUpdateOperation::Drop { silent: true, graph: to.clone().into() }, copy_graph(from.clone(), to), GraphUpdateOperation::Drop { silent, graph: from.into() }]
            }
        }

        rule Copy() -> Vec<GraphUpdateOperation> = i("COPY") _ silent:Update1_silent() _ from:GraphOrDefault() _ i("TO") _ to:GraphOrDefault() {
            // Rewriting defined by https://www.w3.org/TR/sparql11-update/#copy
            if from == to {
                Vec::new() // identity case
            } else {
                let bgp = GraphPattern::Bgp { patterns: vec![TriplePattern::new(Variable::new_unchecked("s"), Variable::new_unchecked("p"), Variable::new_unchecked("o"))] };
                vec![GraphUpdateOperation::Drop { silent: true, graph: to.clone().into() }, copy_graph(from, to)]
            }
        }

        rule InsertData() -> Vec<GraphUpdateOperation> = i("INSERT") _ i("DATA") _ data:QuadData() {
            vec![GraphUpdateOperation::InsertData { data }]
        }

        rule DeleteData() -> Vec<GraphUpdateOperation> = i("DELETE") _ i("DATA") _ data:GroundQuadData() {
            vec![GraphUpdateOperation::DeleteData { data }]
        }

        rule DeleteWhere() -> Vec<GraphUpdateOperation> = i("DELETE") _ i("WHERE") _ d:QuadPattern() {?
            let pattern = d.iter().map(|q| {
                let bgp = GraphPattern::Bgp { patterns: vec![TriplePattern::new(q.subject.clone(), q.predicate.clone(), q.object.clone())] };
                match &q.graph_name {
                    GraphNamePattern::NamedNode(graph_name) => GraphPattern::Graph { name: graph_name.clone().into(), inner: Box::new(bgp) },
                    GraphNamePattern::DefaultGraph => bgp,
                    GraphNamePattern::Variable(graph_name) => GraphPattern::Graph { name: graph_name.clone().into(), inner: Box::new(bgp) },
                }
            }).reduce(new_join).unwrap_or_default();
            let delete = d.into_iter().map(GroundQuadPattern::try_from).collect::<Result<Vec<_>,_>>().map_err(|()| "Blank nodes are not allowed in DELETE WHERE")?;
            Ok(vec![GraphUpdateOperation::DeleteInsert {
                delete,
                insert: Vec::new(),
                using: None,
                pattern: Box::new(pattern)
            }])
        }

        rule Modify() -> Vec<GraphUpdateOperation> = with:Modify_with()? _ c:Modify_clauses() _ u:(UsingClause() ** (_)) _ i("WHERE") _ pattern:GroupGraphPattern() {?
            let (delete, insert) = c;
            let mut delete = delete.unwrap_or_default();
            let mut insert = insert.unwrap_or_default();
            #[expect(clippy::shadow_same)]
            let mut pattern = pattern.legacy()?;

            let mut using = if u.is_empty() {
                None
            } else {
                let mut default = Vec::new();
                let mut named = Vec::new();
                for (d, n) in u {
                    if let Some(d) = d {
                        default.push(d)
                    }
                    if let Some(n) = n {
                        named.push(n)
                    }
                }
                Some(QueryDataset { default, named: Some(named) })
            };

            if let Some(with) = with {
                // We inject WITH everywhere
                delete = delete.into_iter().map(|q| if q.graph_name == GraphNamePattern::DefaultGraph {
                    GroundQuadPattern {
                        subject: q.subject,
                        predicate: q.predicate,
                        object: q.object,
                        graph_name: with.clone().into()
                    }
                } else {
                    q
                }).collect();
                insert = insert.into_iter().map(|q| if q.graph_name == GraphNamePattern::DefaultGraph {
                    QuadPattern {
                        subject: q.subject,
                        predicate: q.predicate,
                        object: q.object,
                        graph_name: with.clone().into()
                    }
                } else {
                    q
                }).collect();
                if using.is_none() {
                    using = Some(QueryDataset { default: vec![with], named: None });
                }
            }

            Ok(vec![GraphUpdateOperation::DeleteInsert {
                delete,
                insert,
                using,
                pattern: Box::new(pattern)
            }])
        }
        rule Modify_with() -> NamedNode = i("WITH") _ i:iri() _ { i }
        rule Modify_clauses() -> (Option<Vec<GroundQuadPattern>>, Option<Vec<QuadPattern>>) = d:DeleteClause() Modify_clear() _ i:InsertClause()? Modify_clear() {
            (Some(d), i)
        } / i:InsertClause() Modify_clear() {
            (None, Some(i))
        }
        rule Modify_clear() = {
            state.currently_used_bnodes.clear();
        }

        rule DeleteClause() -> Vec<GroundQuadPattern> = i("DELETE") _ q:QuadPattern() {?
            q.into_iter().map(GroundQuadPattern::try_from).collect::<Result<Vec<_>,_>>().map_err(|()| "Blank nodes are not allowed in DELETE WHERE")
        }

        rule InsertClause() -> Vec<QuadPattern> = i("INSERT") _ q:QuadPattern() { q }

        rule UsingClause() -> (Option<NamedNode>, Option<NamedNode>) = i("USING") _ d:(UsingClause_default() / UsingClause_named()) { d }
        rule UsingClause_default() -> (Option<NamedNode>, Option<NamedNode>) = i:iri() {
            (Some(i), None)
        }
        rule UsingClause_named() -> (Option<NamedNode>, Option<NamedNode>) = i("NAMED") _ i:iri() {
            (None, Some(i))
        }

        rule GraphOrDefault() -> GraphName = i("DEFAULT") {
            GraphName::DefaultGraph
        } / (i("GRAPH") _)? g:iri() {
            GraphName::NamedNode(g)
        }

        rule GraphRef() -> NamedNode = i("GRAPH") _ g:iri() { g }

        rule GraphRefAll() -> GraphTarget  = i: GraphRef() { i.into() }
            / i("DEFAULT") { GraphTarget::DefaultGraph }
            / i("NAMED") { GraphTarget::NamedGraphs }
            / i("ALL") { GraphTarget::AllGraphs }

        rule QuadPattern() -> Vec<QuadPattern> = "{" _ q:Quads() _ "}" { q }

        rule QuadData() -> Vec<Quad> = "{" _ q:Quads() _ "}" {?
            q.into_iter().map(Quad::try_from).collect::<Result<Vec<_>, ()>>().map_err(|()| "Variables are not allowed in INSERT DATA")
        }
        rule GroundQuadData() -> Vec<GroundQuad> = "{" _ q:Quads() _ "}" {?
            q.into_iter().map(|q| GroundQuad::try_from(Quad::try_from(q)?)).collect::<Result<Vec<_>, ()>>().map_err(|()| "Variables and blank nodes are not allowed in DELETE DATA")
        }

        rule Quads() -> Vec<QuadPattern> = q:(Quads_TriplesTemplate() / Quads_QuadsNotTriples()) ** (_) {
            q.into_iter().flatten().collect()
        }
        rule Quads_TriplesTemplate() -> Vec<QuadPattern> = t:TriplesTemplate() {
            t.into_iter().map(|t| QuadPattern::new(t.subject, t.predicate, t.object, GraphNamePattern::DefaultGraph)).collect()
        } //TODO: return iter?
        rule Quads_QuadsNotTriples() -> Vec<QuadPattern> = q:QuadsNotTriples() _ "."? { q }

        rule QuadsNotTriples() -> Vec<QuadPattern> = i("GRAPH") _ g:VarOrIri() _ "{" _ t:TriplesTemplate()? _ "}" {
            t.unwrap_or_default().into_iter().map(|t| QuadPattern::new(t.subject, t.predicate, t.object, g.clone())).collect()
        }

        rule TriplesTemplate() -> Vec<TriplePattern> = ts:TriplesTemplate_inner() ++ (".") ("." _)? {
            ts.into_iter().flatten().collect()
        }
        rule TriplesTemplate_inner() -> Vec<TriplePattern> = _ t:TriplesSameSubject() _ { t }

        rule GroupGraphPattern() -> ParsedPattern =
            SourceGraphMode() "{" _ GroupGraphPattern_clear() p:(SubSelect() / GroupGraphPatternSub()) GroupGraphPattern_clear() _ at:position!() SourceGroupClose(at) { p } /
            LegacyGraphMode() "{" _ GroupGraphPattern_clear() p:GroupGraphPatternSub() GroupGraphPattern_clear() _ "}" { p } /
            LegacyGraphMode() "{" _ GroupGraphPattern_clear() p:SubSelect() GroupGraphPattern_clear() _ "}" { p }
        rule SourceGroupClose(at: usize) = "}" / {?
            state.source_parse_error = Some((at, "Expected '}'"));
            Err("}")
        }
        rule GroupGraphPattern_clear() = {
             // We deal with blank nodes aliases rule
            state.used_bnodes.extend(state.currently_used_bnodes.iter().cloned());
            state.currently_used_bnodes.clear();
        }

        rule GroupGraphPatternSub() -> ParsedPattern = a:TriplesBlock()? _ b:GroupGraphPatternSub_item()* {?
            if state.source_literal_hooks.is_some() {
                let initial = a.map(|patterns| state.source_arena.triples_block(patterns));
                return Ok(state.source_arena.group(initial,b.into_iter().flatten().collect()));
            }
            let mut filter: Option<Expression> = None;
            let mut g = a.map_or_else(GraphPattern::default, build_bgp);
            for e in b.into_iter().flatten() {
                match e {
                    PartialGraphPattern::Source(_) => return Err("Source syntax reached the legacy group builder"),
                    PartialGraphPattern::Optional(p, f) => {
                        g = GraphPattern::LeftJoin { left: Box::new(g), right: Box::new(p), expression: f }
                    }
                    #[cfg(feature = "sep-0006")]
                    PartialGraphPattern::Lateral(p) => {
                        let mut defined_variables = HashSet::new();
                        add_defined_variables(&p, &mut defined_variables);
                        let mut contains = false;
                        g.on_in_scope_variable(|v| {
                            if defined_variables.contains(v) {
                                contains = true;
                            }
                        });
                        if contains {
                            return Err("An existing variable is overridden in the right side of LATERAL");
                        }
                        g = GraphPattern::Lateral { left: Box::new(g), right: Box::new(p) }
                    }
                    PartialGraphPattern::Minus(p) => {
                        g = GraphPattern::Minus { left: Box::new(g), right: Box::new(p) }
                    }
                    PartialGraphPattern::Bind(expression, variable) => {
                        let mut contains = false;
                        g.on_in_scope_variable(|v| {
                            if *v == variable {
                                contains = true;
                            }
                        });
                        if contains {
                            return Err("BIND is overriding an existing variable")
                        }
                        g = GraphPattern::Extend { inner: Box::new(g), variable, expression }
                    }
                    PartialGraphPattern::Filter(expr) => filter = Some(if let Some(f) = filter {
                        Expression::And(Box::new(f), Box::new(expr))
                    } else {
                        expr
                    }),
                    PartialGraphPattern::Other(e) => g = new_join(g, e),
                }
            }

            Ok(ParsedPattern::Legacy(if let Some(expr) = filter {
                GraphPattern::Filter { expr, inner: Box::new(g) }
            } else {
                g
            }))
        }
        rule GroupGraphPatternSub_item() -> Vec<PartialGraphPattern> = a:GraphPatternNotTriples() _ ("." _)? b:TriplesBlock()? _ {
            let mut result = vec![a];
            if let Some(v) = b {
                if state.source_literal_hooks.is_some() {
                    let ParsedPattern::Source(id) = state.source_arena.triples_block(v) else { unreachable!() };
                    result.push(PartialGraphPattern::Source(id));
                } else {
                    result.push(PartialGraphPattern::Other(build_bgp(v)));
                }
            }
            result
        }

        rule TriplesBlock() -> Vec<TripleOrPathPattern> = hs:TriplesBlock_inner() ++ (".") ("." _)? {
            hs.into_iter().flatten().collect()
        }
        rule TriplesBlock_inner() -> Vec<TripleOrPathPattern> = _ h:TriplesSameSubjectPath() _ { h }

        rule ReifiedTripleBlock() -> Vec<TriplePattern> = s:ReifiedTriple() _ po:PropertyList() {?
            let mut patterns = po.patterns;
            patterns.extend(s.patterns);
            for (p, os) in po.focus {
                for o in os {
                    add_to_triple_patterns(s.focus.clone(), p.clone(), o, &mut patterns)?;
                }
            }
            Ok(patterns)
        }

        rule ReifiedTripleBlockPath() -> Vec<TripleOrPathPattern> = LegacyGraphMode() s:ReifiedTriple() _ po:PropertyListPath() {?
            let mut patterns = po.patterns;
            patterns.extend(s.patterns.into_iter().map(Into::into));
            for (p, os) in po.focus {
                for o in os {
                    add_to_triple_or_path_patterns(s.focus.clone(), p.clone(), o, &mut patterns)?;
                }
            }
            Ok(patterns)
        }

        rule GraphPatternNotTriples() -> PartialGraphPattern = GroupOrUnionGraphPattern() / OptionalGraphPattern() / LateralGraphPattern() / MinusGraphPattern() / GraphGraphPattern() / ServiceGraphPattern() / Filter() / Bind() / InlineData()

        rule OptionalGraphPattern() -> PartialGraphPattern = i("OPTIONAL") _ p:GroupGraphPattern() {
            match p {
                ParsedPattern::Source(id)=>state.source_arena.unary_part("OptionalGraphPattern",ParsedPattern::Source(id),Vec::new()),
                ParsedPattern::Legacy(GraphPattern::Filter{expr,inner})=>PartialGraphPattern::Optional(*inner,Some(expr)),
                ParsedPattern::Legacy(pattern)=>PartialGraphPattern::Optional(pattern,None),
            }
        }

        rule LateralGraphPattern() -> PartialGraphPattern = i("LATERAL") _ p:GroupGraphPattern() {?
                #[cfg(feature = "sep-0006")]{Ok(PartialGraphPattern::Lateral(p.legacy()?))}
                #[cfg(not(feature = "sep-0006"))]{Err("The LATERAL modifier is not supported")}
        }

        rule GraphGraphPattern() -> PartialGraphPattern = i("GRAPH") _ name:VarOrIri() _ p:GroupGraphPattern() {
            match p {
                ParsedPattern::Source(id)=>state.source_arena.unary_part("GraphGraphPattern",ParsedPattern::Source(id),vec![("term",crate::SourceSyntaxValue::NamedNodePattern(name))]),
                ParsedPattern::Legacy(p)=>PartialGraphPattern::Other(GraphPattern::Graph{name,inner:Box::new(p)}),
            }
        }
        rule ServiceGraphPattern() -> PartialGraphPattern = i("SERVICE") _ silent:(i("SILENT") _ {true})? name:VarOrIri() _ p:GroupGraphPattern() {
            let silent=silent.unwrap_or(false);
            match p {
                ParsedPattern::Source(id)=>state.source_arena.unary_part("ServiceGraphPattern",ParsedPattern::Source(id),vec![("term",crate::SourceSyntaxValue::NamedNodePattern(name)),("silent",crate::SourceSyntaxValue::Boolean(silent))]),
                ParsedPattern::Legacy(p)=>PartialGraphPattern::Other(GraphPattern::Service{name,inner:Box::new(p),silent}),
            }
        }

        rule Bind() -> PartialGraphPattern = i("BIND") _ "(" _ e:Expression() _ i("AS") _ v:Var() _ ")" {
            PartialGraphPattern::Bind(e, v)
        }

        rule InlineData() -> PartialGraphPattern = i("VALUES") _ p:DataBlock() {
            match p { ParsedPattern::Legacy(pattern)=>PartialGraphPattern::Other(pattern),ParsedPattern::Source(id)=>PartialGraphPattern::Source(id) }
        }

        rule DataBlock() -> ParsedPattern = source:((l:InlineDataOneVar() {(l,true)}) / (l:InlineDataFull() {(l,false)})) {?
            let (l,one_variable_form)=source;
            if state.preserve_source_literals {
                let bindings = l.1.into_iter().map(|row| {
                    row.into_iter().map(|value| value.map(ParsedDataBlockValue::into_source).transpose()).collect()
                }).collect::<Result<Vec<_>, &'static str>>()?;
                if state.source_literal_hooks.is_some() {Ok(state.source_arena.data(l.0,bindings,one_variable_form))}
                else {Ok(ParsedPattern::Legacy(GraphPattern::SourceValues { variables: l.0, bindings }))}
            } else {
                Ok(ParsedPattern::Legacy(GraphPattern::Values {
                    variables: l.0,
                    bindings: l.1.into_iter().map(|row| row.into_iter().map(|value| value.map(ParsedDataBlockValue::into_ground)).collect()).collect(),
                }))
            }
        }

        rule InlineDataOneVar() -> (Vec<Variable>, Vec<Vec<Option<ParsedDataBlockValue>>>) = var:Var() _ "{" _ d:InlineDataOneVar_value()* "}" {
            (vec![var], d)
        }
        rule InlineDataOneVar_value() -> Vec<Option<ParsedDataBlockValue>> = t:DataBlockValue() _ { vec![t] }

        rule InlineDataFull() -> (Vec<Variable>, Vec<Vec<Option<ParsedDataBlockValue>>>) = "(" _ vars:InlineDataFull_var()* _ ")" _ "{" _ vals:InlineDataFull_values()* "}" {?
            // RDFLib's source grammar admits duplicate keys and zip-truncated
            // rows. The ordinary SPARQL parser retains its strict contract.
            if state.capture_source_graph_syntax() {
                Ok((vars, vals))
            } else if vars.iter().enumerate().any(|(i, vl)| vars[i+1..].contains(vl)) {
                Err("Repeated variables are not allowed in VALUES clauses.")
            } else if vals.iter().any(|vs| vs.len() != vars.len()) {
                Err("The VALUES clause rows should have exactly the same number of values as there are variables. To set a value to undefined use UNDEF.")
            } else {
                Ok((vars, vals))
            }
        }
        rule InlineDataFull_var() -> Variable = v:Var() _ { v }
        rule InlineDataFull_values() -> Vec<Option<ParsedDataBlockValue>> = "(" _ v:InlineDataFull_value()* _ ")" _ { v }
        rule InlineDataFull_value() -> Option<ParsedDataBlockValue> = v:DataBlockValue() _ { v }

        rule DataBlockValue() -> Option<ParsedDataBlockValue> =
            t:TripleTermData() {?
                #[cfg(feature = "sparql-12")]{Ok(Some(ParsedDataBlockValue::Ground(t.into())))}
                #[cfg(not(feature = "sparql-12"))]{Err("Triple terms are only available in SPARQL 1.2")}
            } /
            i:iri() { Some(ParsedDataBlockValue::Ground(i.into())) } /
            l:RDFLiteral() { Some(ParsedDataBlockValue::Literal(l)) } /
            l:NumericLiteral() { Some(ParsedDataBlockValue::Literal(l)) } /
            l:BooleanLiteral() { Some(ParsedDataBlockValue::Literal(l)) } /
            i("UNDEF") { None }

        rule Reifier() -> TermPattern = "~" _ v:VarOrReifierId()? { v.unwrap_or_else(|| BlankNode::default().into()) }

        rule VarOrReifierId() -> TermPattern =
            v:Var() { v.into() } /
            i:iri() { i.into() } /
            b:BlankNode() { b.into() }

        rule MinusGraphPattern() -> PartialGraphPattern = i("MINUS") _ p:GroupGraphPattern() {
            match p {ParsedPattern::Source(id)=>state.source_arena.unary_part("MinusGraphPattern",ParsedPattern::Source(id),Vec::new()),ParsedPattern::Legacy(pattern)=>PartialGraphPattern::Minus(pattern)}
        }
        rule GroupOrUnionGraphPattern() -> PartialGraphPattern = p:GroupOrUnionGraphPattern_item() **<1,> (i("UNION") _) {?
            if state.source_literal_hooks.is_some() {Ok(state.source_arena.union_part(p))}
            else {not_empty_fold(p.into_iter().map(ParsedPattern::legacy).collect::<Result<Vec<_>,_>>()?.into_iter(),|left,right|GraphPattern::Union{left:Box::new(left),right:Box::new(right)}).map(PartialGraphPattern::Other)}
        }
        rule GroupOrUnionGraphPattern_item() -> ParsedPattern = p:GroupGraphPattern() _ {p}

        rule Filter() -> PartialGraphPattern = i("FILTER") _ c:Constraint() {
            PartialGraphPattern::Filter(c)
        }

        rule Constraint() -> Expression = BrackettedExpression() / FunctionCall() / BuiltInCall()

        rule FunctionCall() -> Expression = f:iri() _ a:ArgList() {?
            if state.custom_aggregate_functions.contains(&f) {
                Err("This custom function is an aggregate function and not a regular function")
            } else {
                Ok(if a.1 {
                    state.source_arena.distinct_function(f, a.0)
                } else {
                    Expression::FunctionCall(Function::Custom(f), a.0)
                })
            }
        }

        rule ArgList() -> (Vec<Expression>, bool) =
            "(" _ distinct:(i("DISTINCT") _)? e:ArgList_item() **<1,> ("," _) _ ")" {?
                if distinct.is_some() && !state.capture_source_graph_syntax() {
                    Err("DISTINCT custom function arguments require the source graph frontend")
                } else {
                    Ok((e, distinct.is_some()))
                }
            } /
            NIL() { (Vec::new(), false) }
        rule ArgList_item() -> Expression = e:Expression() _ { e }

        rule ExpressionList() -> Vec<Expression> =
            "(" _ e:ExpressionList_item() **<1,> ("," _) ")" { e } /
            NIL() { Vec::new() }
        rule ExpressionList_item() -> Expression = e:Expression() _ { e }

        rule ConstructTemplate() -> Vec<TriplePattern> = "{" _ t:ConstructTriples() _ "}" { t }

        rule ConstructTriples() -> Vec<TriplePattern> = p:ConstructTriples_item() ** ("." _) "."? {
            p.into_iter().flatten().collect()
        }
        rule ConstructTriples_item() -> Vec<TriplePattern> = t:TriplesSameSubject() _ { t }

        rule TriplesSameSubject() -> Vec<TriplePattern> =
            ReifiedTripleBlock() /
            s:VarOrTerm() _ po:PropertyListNotEmpty() {?
                let mut patterns = po.patterns;
                for (p, os) in po.focus {
                    for o in os {
                        add_to_triple_patterns(s.clone(), p.clone(), o, &mut patterns)?
                    }
                }
                Ok(patterns)
            } /
            s:TriplesNode() _ po:PropertyList() {?
                let mut patterns = s.patterns;
                patterns.extend(po.patterns);
                for (p, os) in po.focus {
                    for o in os {
                        add_to_triple_patterns(s.focus.clone(), p.clone(), o, &mut patterns)?
                    }
                }
                Ok(patterns)
            }

        rule PropertyList() -> FocusedTriplePattern<Vec<(NamedNodePattern,Vec<ReifiedTerm>)>> =
            PropertyListNotEmpty() /
            { FocusedTriplePattern::default() }

        rule PropertyListNotEmpty() -> FocusedTriplePattern<Vec<(NamedNodePattern,Vec<ReifiedTerm>)>> = hp:Verb() _ ho:ObjectList() _ l:PropertyListNotEmpty_item()* {
            l.into_iter().flatten().fold(FocusedTriplePattern {
                focus: vec![(hp, ho.focus)],
                patterns: ho.patterns
            }, |mut a, b| {
                a.focus.push(b.focus);
                a.patterns.extend(b.patterns);
                a
            })
        }
        rule PropertyListNotEmpty_item() -> Option<FocusedTriplePattern<(NamedNodePattern,Vec<ReifiedTerm>)>> = ";" _ c:PropertyListNotEmpty_item_content()? {
            c
        }
        rule PropertyListNotEmpty_item_content() -> FocusedTriplePattern<(NamedNodePattern,Vec<ReifiedTerm>)> = p:Verb() _ o:ObjectList() _ {
            FocusedTriplePattern {
                focus: (p, o.focus),
                patterns: o.patterns
            }
        }

        rule Verb() -> NamedNodePattern = VarOrIri() / "a" { rdf::TYPE.into_owned().into() }

        rule ObjectList() -> FocusedTriplePattern<Vec<ReifiedTerm >> = o:ObjectList_item() **<1,> ("," _) {
            o.into_iter().fold(FocusedTriplePattern::<Vec<ReifiedTerm >>::default(), |mut a, b| {
                a.focus.push(b.focus);
                a.patterns.extend_from_slice(&b.patterns);
                a
            })
        }
        rule ObjectList_item() -> FocusedTriplePattern<ReifiedTerm> = o:Object() _ { o }

        rule Object() -> FocusedTriplePattern<ReifiedTerm> = g:GraphNode() _ a:Annotation() {
            let mut patterns = g.patterns;
            patterns.extend(a.patterns);
            FocusedTriplePattern {
                focus: ReifiedTerm {
                    source: None,
                    term: g.focus,
                    reifiers: a.focus
                },
                patterns
            }
        }

        rule TriplesSameSubjectPath() -> Vec<TripleOrPathPattern> =
            ReifiedTripleBlockPath() /
            s:VarOrTerm() _ po:PropertyListPathNotEmpty() {?
                if state.capture_source_graph_syntax() {
                    let terms = state.source_triples_row(&s,&po.focus).ok_or("Source row syntax is missing")?;
                    let physical = state.source_physical_row(&terms)?;
                    return Ok(vec![TripleOrPathPattern::SourceRow { terms, physical }]);
                }
                let mut patterns = po.patterns;
                for (p, os) in po.focus {
                    for o in os {
                        add_source_native_pattern(s.clone(),p.expression.clone(),o,&mut patterns,state.source_native_paths())?;
                    }
                }
                Ok(patterns)
            } /
            s:TriplesNodePath() _ po:PropertyListPath() {?
                if state.capture_source_graph_syntax() {
                    let terms = state.source_expanded_row(&s.focus,s.source.as_ref(),&po.focus).ok_or("Source subject row syntax is missing")?;
                    let physical = state.source_physical_row(&terms)?;
                    return Ok(vec![TripleOrPathPattern::SourceRow { terms,physical }]);
                }
                let mut patterns=s.patterns;
                patterns.extend(po.patterns);
                for (p,os) in po.focus {
                    for o in os {
                        add_to_triple_or_path_patterns(s.focus.clone(),p.clone(),o,&mut patterns)?;
                    }
                }
                Ok(patterns)
            }

        rule PropertyListPath() -> FocusedTripleOrPathPattern<Vec<(ParsedVerb,Vec<ReifiedTerm>)>> =
            PropertyListPathNotEmpty() /
            { FocusedTripleOrPathPattern::default() }

        rule PropertyListPathNotEmpty() -> FocusedTripleOrPathPattern<Vec<(ParsedVerb,Vec<ReifiedTerm>)>> = hp:(VerbPath() / VerbSimple()) _ ho:ObjectListPath() _ t:PropertyListPathNotEmpty_item()* {
                t.into_iter().flatten().fold(FocusedTripleOrPathPattern {
                    source: None,
                    focus: vec![(hp, ho.focus)],
                    patterns: ho.patterns
                }, |mut a, b| {
                    a.focus.push(b.focus);
                    a.patterns.extend(b.patterns);
                    a
                })
        }
        rule PropertyListPathNotEmpty_item() -> Option<FocusedTripleOrPathPattern<(ParsedVerb,Vec<ReifiedTerm>)>> = ";" _ c:PropertyListPathNotEmpty_item_content()? {
            c
        }
        rule PropertyListPathNotEmpty_item_content() -> FocusedTripleOrPathPattern<(ParsedVerb,Vec<ReifiedTerm>)> = p:(VerbPath() / VerbSimple()) _ o:ObjectListPath() _ {
            FocusedTripleOrPathPattern {
                    source: None,
                focus: (p, o.focus),
                patterns: o.patterns
            }
        }

        rule VerbPath() -> ParsedVerb = p:Path() {
            let verb=ParsedVerb { expression:p.expression.into(),syntax:p.syntax };
            state.capture_source_predicate(&verb);
            verb
        }

        rule VerbSimple() -> ParsedVerb = v:Var() {
            let syntax = state.capture_source_graph_syntax().then(|| crate::SourceSyntaxValue::Variable(v.clone()));
            ParsedVerb { expression: v.into(), syntax }
        }

        rule ObjectListPath() -> FocusedTripleOrPathPattern<Vec<ReifiedTerm>> = o:ObjectListPath_item() **<1,> ("," _) {
            o.into_iter().fold(FocusedTripleOrPathPattern::<Vec<ReifiedTerm>>::default(), |mut a, b| {
                a.focus.push(b.focus);
                a.patterns.extend(b.patterns);
                a
            })
        }
        rule ObjectListPath_item() -> FocusedTripleOrPathPattern<ReifiedTerm> = o:ObjectPath() _ { o }

        rule ObjectPath() -> FocusedTripleOrPathPattern<ReifiedTerm> = g:GraphNodePath() _ a:AnnotationPath() {
            let source=if state.capture_source_graph_syntax() {
                g.source.or_else(||ParserState::source_graph_term(&g.focus))
            } else {None};
            let mut patterns=g.patterns;
            patterns.extend(a.patterns);
            FocusedTripleOrPathPattern {
                source:None,
                focus:ReifiedTerm {term:g.focus,reifiers:a.focus,source},
                patterns
            }
        }

        rule Path() -> ParsedPath = path:PathAlternative() {?
            // A source graph phase must admit the ORIGINAL path before the
            // ordinary grammar expands Sequence/Reverse into BGP leaves.
            if let Some(check) = state.source_literal_hooks.as_ref().and_then(|hooks| hooks.graph_path_admission.clone()) {
                if let Err(error) = check(&path.expression) {
                    if state.numeric_construction_error.is_none() { state.numeric_construction_error = Some(error); }
                    return Err("Source graph path is not admitted");
                }
            }
            Ok(path)
        }

        rule PathAlternative() -> ParsedPath = p:PathAlternative_item() **<1,> ("|" _) {?
            let syntax = p.iter().map(|p| p.syntax.clone()).collect::<Option<Vec<_>>>()
                .and_then(|parts| state.source_path_node("PathAlternative", vec![("part", crate::SourceSyntaxValue::List(parts))]));
            let expression = not_empty_fold(p.into_iter().map(|p| p.expression), |a, b| {
                PropertyPathExpression::Alternative(Box::new(a), Box::new(b))
            })?;
            Ok(ParsedPath { expression, syntax })
        }
        rule PathAlternative_item() -> ParsedPath = p:PathSequence() _ { p }

        rule PathSequence() -> ParsedPath = p:PathSequence_item() **<1,> ("/" _) {?
            let syntax = p.iter().map(|p| p.syntax.clone()).collect::<Option<Vec<_>>>()
                .and_then(|parts| state.source_path_node("PathSequence", vec![("part", crate::SourceSyntaxValue::List(parts))]));
            let expression = not_empty_fold(p.into_iter().map(|p| p.expression), |a, b| {
                PropertyPathExpression::Sequence(Box::new(a), Box::new(b))
            })?;
            Ok(ParsedPath { expression, syntax })
        }
        rule PathSequence_item() -> ParsedPath = p:PathEltOrInverse() _ { p }

        rule PathElt() -> ParsedPath = p:PathPrimary() _ o:PathElt_op()? {
            let syntax = p.syntax.and_then(|part| {
                let mut fields = vec![("part", part)];
                if let Some(modifier) = o { fields.push(("mod", crate::SourceSyntaxValue::Text(modifier.to_string()))); }
                state.source_path_node("PathElt", fields)
            });
            let expression = match o {
                Some('?') => PropertyPathExpression::ZeroOrOne(Box::new(p.expression)),
                Some('*') => PropertyPathExpression::ZeroOrMore(Box::new(p.expression)),
                Some('+') => PropertyPathExpression::OneOrMore(Box::new(p.expression)),
                Some(_) => unreachable!(),
                None => p.expression
            };
            ParsedPath { expression, syntax }
        }
        rule PathElt_op() -> char =
            "*" { '*' } /
            "+" { '+' } /
            "?" !(['0'..='9'] / PN_CHARS_U()) { '?' } // We mandate that this is not a variable

        rule PathEltOrInverse() -> ParsedPath =
            "^" _ p:PathElt() {
                let syntax = p.syntax.and_then(|part| state.source_path_node("PathEltOrInverse", vec![("part", part)]));
                ParsedPath { expression: PropertyPathExpression::Reverse(Box::new(p.expression)), syntax }
            } /
            PathElt()

        rule PathPrimary() -> ParsedPath =
            v:iri() {
                let syntax = state.capture_source_graph_syntax().then(|| crate::SourceSyntaxValue::NamedNode(v.clone()));
                ParsedPath { expression: v.into(), syntax }
            } /
            "a" {
                let node = rdf::TYPE.into_owned();
                let syntax = state.capture_source_graph_syntax().then(|| crate::SourceSyntaxValue::NamedNode(node.clone()));
                ParsedPath { expression: node.into(), syntax }
            } /
            "!" _ p:PathNegatedPropertySet() { p } /
            "(" _ p:Path() _ ")" { p }

        rule PathNegatedPropertySet() -> ParsedPath =
            "(" _ p:PathNegatedPropertySet_item() **<1,> ("|" _) ")" {
                state.source_negated_path(p)
            } /
            p:PathOneInPropertySet() {
                state.source_negated_path(vec![p])
            }
        rule PathNegatedPropertySet_item() -> Either<NamedNode,NamedNode> = p:PathOneInPropertySet() _ { p }

        rule PathOneInPropertySet() -> Either<NamedNode,NamedNode> =
            "^" _ v:iri() { Either::Right(v) } /
            "^" _ "a" { Either::Right(rdf::TYPE.into()) } /
            v:iri() { Either::Left(v) } /
            "a" { Either::Left(rdf::TYPE.into()) }

        rule TriplesNode() -> FocusedTriplePattern<TermPattern> = Collection() / BlankNodePropertyList()

        rule BlankNodePropertyList() -> FocusedTriplePattern<TermPattern> = "[" _ po:PropertyListNotEmpty() _ "]" {?
            let mut patterns = po.patterns;
            let mut bnode = TermPattern::from(BlankNode::default());
            for (p, os) in po.focus {
                for o in os {
                    add_to_triple_patterns(bnode.clone(), p.clone(), o, &mut patterns)?;
                }
            }
            Ok(FocusedTriplePattern {
                focus: bnode,
                patterns
            })
        }

        rule TriplesNodePath() -> FocusedTripleOrPathPattern<TermPattern> = CollectionPath() / BlankNodePropertyListPath()

        rule BlankNodePropertyListPath() -> FocusedTripleOrPathPattern<TermPattern> = "[" _ po:PropertyListPathNotEmpty() _ "]" {?
            if state.capture_source_graph_syntax() { return state.source_blank_properties(po.focus); }
            let mut patterns = po.patterns;
            let mut bnode = TermPattern::from(BlankNode::default());
            for (p, os) in po.focus {
                for o in os {
                    add_to_triple_or_path_patterns(bnode.clone(), p.clone(), o, &mut patterns)?;
                }
            }
            Ok(FocusedTripleOrPathPattern {
                    source: None,
                focus: bnode,
                patterns
            })
        }

        rule Collection() -> FocusedTriplePattern<TermPattern> = "(" _ o:Collection_item()+ ")" {
            let mut patterns: Vec<TriplePattern> = Vec::new();
            let mut current_list_node = TermPattern::from(rdf::NIL.into_owned());
            for objWithPatterns in o.into_iter().rev() {
                let new_blank_node = TermPattern::from(BlankNode::default());
                patterns.push(TriplePattern::new(new_blank_node.clone(), rdf::FIRST.into_owned(), objWithPatterns.focus.clone()));
                patterns.push(TriplePattern::new(new_blank_node.clone(), rdf::REST.into_owned(), current_list_node));
                current_list_node = new_blank_node;
                patterns.extend_from_slice(&objWithPatterns.patterns);
            }
            FocusedTriplePattern {
                focus: current_list_node,
                patterns
            }
        }
        rule Collection_item() -> FocusedTriplePattern<TermPattern> = o:GraphNode() _ { o }

        rule CollectionPath() -> FocusedTripleOrPathPattern<TermPattern> = "(" _ o:CollectionPath_item()+ _ ")" {?
            if state.capture_source_graph_syntax() { return state.source_collection(o); }
            let mut patterns: Vec<TripleOrPathPattern> = Vec::new();
            let mut current_list_node = TermPattern::from(rdf::NIL.into_owned());
            for objWithPatterns in o.into_iter().rev() {
                let new_blank_node = TermPattern::from(BlankNode::default());
                patterns.push(TriplePattern::new(new_blank_node.clone(), rdf::FIRST.into_owned(), objWithPatterns.focus.clone()).into());
                patterns.push(TriplePattern::new(new_blank_node.clone(), rdf::REST.into_owned(), current_list_node).into());
                current_list_node = new_blank_node;
                patterns.extend(objWithPatterns.patterns);
            }
            Ok(FocusedTripleOrPathPattern {
                    source: None,
                focus: current_list_node,
                patterns
            })
        }
        rule CollectionPath_item() -> FocusedTripleOrPathPattern<TermPattern> = p:GraphNodePath() _ { p }

        rule VarOrTerm() -> TermPattern =
            v:Var() { v.into() } /
            t:TripleTerm() {?
                #[cfg(feature = "sparql-12")]{Ok(t.into())}
                #[cfg(not(feature = "sparql-12"))]{Err("Triple terms are only available in SPARQL 1.2")}
            } /
            i:iri() { i.into() } /
            l:RDFLiteral() { state.pattern_literal(l) } /
            l:NumericLiteral() { state.pattern_literal(l) } /
            l:BooleanLiteral() { state.pattern_literal(l) } /
            b:BlankNode() { b.into() } /
            NIL() { rdf::NIL.into_owned().into() }

        rule AnnotationPath() -> FocusedTripleOrPathPattern<Vec<TermPattern>> = a:(LegacyGraphMode() v:AnnotationPath_e() { v })* {
            let mut output: FocusedTripleOrPathPattern<Vec<TermPattern>> = FocusedTripleOrPathPattern::new(Vec::new());
            for a in a {
                output.focus.push(a.focus);
                output.patterns.extend(a.patterns);
            }
            output
        }
        rule AnnotationPath_e() -> FocusedTripleOrPathPattern<TermPattern> =
            r:Reifier() _ a:AnnotationBlockPath()? _ {?
                let mut output: FocusedTripleOrPathPattern<TermPattern> = FocusedTripleOrPathPattern::new(r);
                if let Some(annotations) = a {
                    for (p, os) in annotations.focus {
                        for o in os {
                            add_to_triple_or_path_patterns(output.focus.clone(), p.clone(), o, &mut output.patterns)?;
                        }
                    }
                    output.patterns.extend(annotations.patterns);
                }
                Ok(output)
            } /
            a:AnnotationBlockPath() _ {?
                let mut output: FocusedTripleOrPathPattern<TermPattern> = FocusedTripleOrPathPattern::new(BlankNode::default());
                for (p, os) in a.focus {
                    for o in os {
                        add_to_triple_or_path_patterns(output.focus.clone(), p.clone(), o, &mut output.patterns)?;
                    }
                }
                output.patterns.extend(a.patterns);
                Ok(output)
            }

        rule AnnotationBlockPath() -> FocusedTripleOrPathPattern<Vec<(ParsedVerb,Vec<ReifiedTerm>)>> = "{|" _ a:PropertyListPathNotEmpty() _ "|}" { a }

        rule Annotation() -> FocusedTriplePattern<Vec<TermPattern>> = a:Annotation_e()* {
            let mut output: FocusedTriplePattern<Vec<TermPattern>> = FocusedTriplePattern::new(Vec::new());
            for a in a {
                output.focus.push(a.focus);
                output.patterns.extend(a.patterns);
            }
            output
        }
        rule Annotation_e() -> FocusedTriplePattern<TermPattern> =
            r:Reifier() _ a:AnnotationBlock()? _ {?
                let mut output: FocusedTriplePattern<TermPattern> = FocusedTriplePattern::new(r);
                if let Some(annotations) = a {
                    for (p, os) in annotations.focus {
                        for o in os {
                            add_to_triple_patterns(output.focus.clone(), p.clone(), o, &mut output.patterns)?;
                        }
                    }
                    output.patterns.extend(annotations.patterns);
                }
                Ok(output)
            } /
            a:AnnotationBlock() _ {?
                let mut output: FocusedTriplePattern<TermPattern> = FocusedTriplePattern::new(BlankNode::default());
                for (p, os) in a.focus {
                    for o in os {
                        add_to_triple_patterns(output.focus.clone(), p.clone(), o, &mut output.patterns)?;
                    }
                }
                output.patterns.extend(a.patterns);
                Ok(output)
            }

        rule AnnotationBlock() -> FocusedTriplePattern<Vec<(NamedNodePattern,Vec<ReifiedTerm>)>> = "{|" _ a:PropertyListNotEmpty() _ "|}" { a }

        rule GraphNode() -> FocusedTriplePattern<TermPattern> =
            ReifiedTriple() /
            t:VarOrTerm() { FocusedTriplePattern::new(t) } /
            TriplesNode()

        rule GraphNodePath() -> FocusedTripleOrPathPattern<TermPattern> =
            t:ReifiedTriple() { t.into() } /
            t:VarOrTerm() { FocusedTripleOrPathPattern::new(t) } /
            TriplesNodePath()

        rule ReifiedTriple() -> FocusedTriplePattern<TermPattern> = "<<" _ s:ReifiedTripleSubject() _ p:Verb() _ o:ReifiedTripleObject() _ r:Reifier()? _ ">>" {?
            #[cfg(feature = "sparql-12")]
            {
                let r = r.unwrap_or_else(|| BlankNode::default().into());
                let mut output = FocusedTriplePattern::new(r.clone());
                output.patterns.push(TriplePattern {
                        subject: r,
                        predicate: rdf::REIFIES.into_owned().into(),
                        object: TriplePattern {
                            subject: s.focus,
                            predicate: p,
                            object: o.focus
                        }.into()
                    });
                output.patterns.extend(s.patterns);
                output.patterns.extend(o.patterns);
                Ok(output)
            }
            #[cfg(not(feature = "sparql-12"))]
            {
                Err("Reified triples are only available in SPARQL 1.2")
            }
        }

        rule ReifiedTripleSubject() -> FocusedTriplePattern<TermPattern> = ReifiedTripleObject()

        rule ReifiedTripleObject() -> FocusedTriplePattern<TermPattern> =
            v:Var() { FocusedTriplePattern::new(v) } /
            t:TripleTerm() {?
                #[cfg(feature = "sparql-12")]{Ok(FocusedTriplePattern::new(t))}
                #[cfg(not(feature = "sparql-12"))]{Err("Triples terms are only available in SPARQL 1.2")}
            } /
            ReifiedTriple() /
            i:iri() { FocusedTriplePattern::new(i) } /
            l:RDFLiteral() { FocusedTriplePattern::new(l.rdf) } /
            l:NumericLiteral() { FocusedTriplePattern::new(l.rdf) } /
            l:BooleanLiteral() { FocusedTriplePattern::new(l.rdf) } /
            b:BlankNode() { FocusedTriplePattern::new(b) }

        rule TripleTerm() -> TriplePattern = "<<(" _ s:TripleTermSubject() _ p:Verb() _ o:TripleTermObject() _ ")>>" {
            TriplePattern {
                subject: s,
                predicate: p,
                object: o
            }
        }

        rule TripleTermSubject() -> TermPattern = TripleTermObject()

        rule TripleTermObject() -> TermPattern =
            v:Var() { v.into() } /
            t:TripleTerm() {?
                #[cfg(feature = "sparql-12")]{Ok(t.into())}
                #[cfg(not(feature = "sparql-12"))]{Err("Triples terms are only available in SPARQL 1.2")}
            } /
            i:iri() { i.into() } /
            l:RDFLiteral() { l.rdf.into() } /
            l:NumericLiteral() { l.rdf.into() } /
            l:BooleanLiteral() { l.rdf.into() } /
            b:BlankNode() { b.into() }

        rule TripleTermData() -> GroundTriple = "<<(" _ s:TripleTermDataSubject() _ p:TripleTermData_p() _ o:TripleTermDataObject() _ ")>>" {?
            Ok(GroundTriple {
                subject: if let GroundTerm::NamedNode(s) = s { s } else { return Err("Literals or triple terms are not allowed in subject position of nested patterns") },
                predicate: p,
                object: o
            })
        }
        rule TripleTermData_p() -> NamedNode = i: iri() { i } / "a" { rdf::TYPE.into() }

        rule TripleTermDataSubject() -> GroundTerm = TripleTermDataObject()

        rule TripleTermDataObject() -> GroundTerm =
            t:TripleTermData() {?
                #[cfg(feature = "sparql-12")]{Ok(t.into())}
                #[cfg(not(feature = "sparql-12"))]{Err("Triples terms are only available in SPARQL 1.2")}
            } /
            i:iri() { i.into() } /
            l:RDFLiteral() { l.rdf.into() } /
            l:NumericLiteral() { l.rdf.into() } /
            l:BooleanLiteral() { l.rdf.into() }

        rule VarOrIri() -> NamedNodePattern =
            v:Var() { v.into() } /
            i:iri() { i.into() }

        rule Var() -> Variable = name:(VAR1() / VAR2()) { Variable::new_unchecked(name) }

        rule Expression() -> Expression = e:ConditionalOrExpression() {e}

        rule ConditionalOrExpression() -> Expression = e:ConditionalOrExpression_item() **<1,> ("||" _) {?
            if state.source_literal_hooks.is_some(){Ok(state.source_arena.boolean_chain("ConditionalOrExpression",e))}else{not_empty_fold(e.into_iter(), |a, b| Expression::Or(Box::new(a), Box::new(b)))}
        }
        rule ConditionalOrExpression_item() -> Expression = e:ConditionalAndExpression() _ { e }

        rule ConditionalAndExpression() -> Expression = e:ConditionalAndExpression_item() **<1,> ("&&" _) {?
            if state.source_literal_hooks.is_some(){Ok(state.source_arena.boolean_chain("ConditionalAndExpression",e))}else{not_empty_fold(e.into_iter(), |a, b| Expression::And(Box::new(a), Box::new(b)))}
        }
        rule ConditionalAndExpression_item() -> Expression = e:ValueLogical() _ { e }

        rule ValueLogical() -> Expression = RelationalExpression()

        rule RelationalExpression() -> Expression = a:NumericExpression() _ o: RelationalExpression_inner()? {
            if state.source_literal_hooks.is_some(){return if let Some((operator,other,list))=o{state.source_arena.relation(a,operator,other,list)}else{a};}
            match o {
            Some(("=", Some(b), None)) => Expression::Equal(Box::new(a), Box::new(b)),
            Some(("!=", Some(b), None)) => Expression::Not(Box::new(Expression::Equal(Box::new(a), Box::new(b)))),
            Some((">", Some(b), None)) => Expression::Greater(Box::new(a), Box::new(b)),
            Some((">=", Some(b), None)) => Expression::GreaterOrEqual(Box::new(a), Box::new(b)),
            Some(("<", Some(b), None)) => Expression::Less(Box::new(a), Box::new(b)),
            Some(("<=", Some(b), None)) => Expression::LessOrEqual(Box::new(a), Box::new(b)),
            Some(("IN", None, Some(l))) => Expression::In(Box::new(a), l),
            Some(("NOT IN", None, Some(l))) => Expression::Not(Box::new(Expression::In(Box::new(a), l))),
            Some(_) => unreachable!(),
            None => a
        } }
        rule RelationalExpression_inner() -> (&'input str, Option<Expression>, Option<Vec<Expression>>) =
            s: $("="  / "!=" / ">=" / ">" / "<=" / "<") _ e:NumericExpression() { (s, Some(e), None) } /
            i("IN") _ l:ExpressionList() { ("IN", None, Some(l)) } /
            i("NOT") _ i("IN") _ l:ExpressionList() { ("NOT IN", None, Some(l)) }

        rule NumericExpression() -> Expression = AdditiveExpression()

        // OE patch 1: SPARQL numeric operators at each precedence level bind left.
        // Fold the original sequence; parenthesized primary expressions stay nested.
        rule AdditiveExpression() -> Expression = a:MultiplicativeExpression() _ o:AdditiveExpression_inner()* {
            if state.preserve_arithmetic_chains && !o.is_empty() {
                Expression::AdditiveChain {
                    first: Box::new(a),
                    tail: o.into_iter().map(|(operator, operand)| (match operator {
                        "+" => AdditiveOperator::Add,
                        "-" => AdditiveOperator::Subtract,
                        _ => unreachable!(),
                    }, operand)).collect(),
                }
            } else {
                o.into_iter().fold(a, |a, (operator, b)| match operator {
                    "+" => Expression::Add(Box::new(a), Box::new(b)),
                    "-" => Expression::Subtract(Box::new(a), Box::new(b)),
                    _ => unreachable!(),
                })
            }
        }
        rule AdditiveExpression_inner() -> (&'input str, Expression) = s: $("+" / "-") _ e:MultiplicativeExpression() _ {
            (s, e)
        }

        rule MultiplicativeExpression() -> Expression = a:UnaryExpression() _ o: MultiplicativeExpression_inner()* {
            if state.preserve_arithmetic_chains && !o.is_empty() {
                Expression::MultiplicativeChain {
                    first: Box::new(a),
                    tail: o.into_iter().map(|(operator, operand)| (match operator {
                        "*" => MultiplicativeOperator::Multiply,
                        "/" => MultiplicativeOperator::Divide,
                        _ => unreachable!(),
                    }, operand)).collect(),
                }
            } else {
                o.into_iter().fold(a, |a, (operator, b)| match operator {
                    "*" => Expression::Multiply(Box::new(a), Box::new(b)),
                    "/" => Expression::Divide(Box::new(a), Box::new(b)),
                    _ => unreachable!(),
                })
            }
        }
        rule MultiplicativeExpression_inner() -> (&'input str, Expression) = s: $("*" / "/") _ e:UnaryExpression() _ {
            (s, e)
        }

        rule UnaryExpression() -> Expression = s: "!" _ e:UnaryExpression() {?
            #[cfg(feature = "sparql-12")]{Ok(Expression::Not(Box::new(e)))}
            #[cfg(not(feature = "sparql-12"))]{Err("Double negation (!!) is only available in SPARQL 1.2")}
        } / s: $("!" / "+" / "-")? _ e:PrimaryExpression() { match s {
            Some("!") => Expression::Not(Box::new(e)),
            Some("+") => Expression::UnaryPlus(Box::new(e)),
            Some("-") => Expression::UnaryMinus(Box::new(e)),
            Some(_) => unreachable!(),
            None => e,
        } }

        rule PrimaryExpression() -> Expression =
            BrackettedExpression() /
            ExprTripleTerm() /
            iriOrFunction() /
            v:Var() { v.into() } /
            l:RDFLiteral() { state.expression_literal(l) } /
            l:NumericLiteral() { state.expression_literal(l) } /
            l:BooleanLiteral() { state.expression_literal(l) } /
            BuiltInCall()

        rule ExprTripleTerm() -> Expression = "<<(" _ s:ExprTripleTermSubject() _ p:Verb() _ o:ExprTripleTermObject() _ ")>>" {?
            #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::Triple, vec![s, p.into(), o]))}
            #[cfg(not(feature = "sparql-12"))]{Err("Triple terms are only available in SPARQL 1.2")}
        }

        rule ExprTripleTermSubject() -> Expression = ExprTripleTermObject()

        rule ExprTripleTermObject() -> Expression =
            ExprTripleTerm() /
            i:iri() { i.into() } /
            l:RDFLiteral() { state.expression_literal(l) } /
            l:NumericLiteral() { state.expression_literal(l) } /
            l:BooleanLiteral() { state.expression_literal(l) } /
            v:Var() { v.into() }

        rule BrackettedExpression() -> Expression = "(" _ e:Expression() _ ")" { e }

        rule BuiltInCall() -> Expression = e:BuiltInCallRaw() { if state.source_literal_hooks.is_some(){state.source_arena.capture_expression(e)}else{e} }

        rule BuiltInCallRaw() -> Expression =
            a:Aggregate() {? state.new_aggregation(a).map(Into::into) } /
            i("STR") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Str, vec![e]) } /
            i("LANG") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Lang, vec![e]) } /
            i("LANGMATCHES") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::LangMatches, vec![a, b]) } /
            i("LANGDIR") _ "(" _ e:Expression() _ ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::LangDir, vec![e]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The LANGDIR function is only available in SPARQL 1.2")}
            } /
            i("DATATYPE") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Datatype, vec![e]) } /
            i("BOUND") _ "(" _ v:Var() _ ")" { Expression::Bound(v) } /
            name:$(i("IRI") / i("URI")) _ "(" _ e:Expression() _ ")" { if state.source_literal_hooks.is_some(){state.source_arena.named_function(Function::Iri,vec![e],if name.eq_ignore_ascii_case("URI"){"Builtin_URI"}else{"Builtin_IRI"})}else{Expression::FunctionCall(Function::Iri, vec![e])} } /
            i("BNODE") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::BNode, vec![e]) } /
            i("BNODE") NIL() { Expression::FunctionCall(Function::BNode, vec![]) }  /
            i("RAND") _ NIL() { Expression::FunctionCall(Function::Rand, vec![]) } /
            i("ABS") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Abs, vec![e]) } /
            i("CEIL") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Ceil, vec![e]) } /
            i("FLOOR") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Floor, vec![e]) } /
            i("ROUND") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Round, vec![e]) } /
            i("CONCAT") e:ExpressionList() { Expression::FunctionCall(Function::Concat, e) } /
            SubstringExpression() /
            i("STRLEN") _ "(" _ e: Expression() _ ")" { Expression::FunctionCall(Function::StrLen, vec![e]) } /
            StrReplaceExpression() /
            i("UCASE") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::UCase, vec![e]) } /
            i("LCASE") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::LCase, vec![e]) } /
            i("ENCODE_FOR_URI") _ "(" _ e: Expression() _ ")" { Expression::FunctionCall(Function::EncodeForUri, vec![e]) } /
            i("CONTAINS") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::Contains, vec![a, b]) } /
            i("STRSTARTS") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::StrStarts, vec![a, b]) } /
            i("STRENDS") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::StrEnds, vec![a, b]) } /
            i("STRBEFORE") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::StrBefore, vec![a, b]) } /
            i("STRAFTER") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::StrAfter, vec![a, b]) } /
            i("YEAR") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Year, vec![e]) } /
            i("MONTH") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Month, vec![e]) } /
            i("DAY") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Day, vec![e]) } /
            i("HOURS") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Hours, vec![e]) } /
            i("MINUTES") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Minutes, vec![e]) } /
            i("SECONDS") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Seconds, vec![e]) } /
            i("TIMEZONE") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Timezone, vec![e]) } /
            i("TZ") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Tz, vec![e]) } /
            i("NOW") _ NIL() { Expression::FunctionCall(Function::Now, vec![]) } /
            i("UUID") _ NIL() { Expression::FunctionCall(Function::Uuid, vec![]) }/
            i("STRUUID") _ NIL() { Expression::FunctionCall(Function::StrUuid, vec![]) } /
            i("MD5") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Md5, vec![e]) } /
            i("SHA1") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Sha1, vec![e]) } /
            i("SHA256") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Sha256, vec![e]) } /
            i("SHA384") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Sha384, vec![e]) } /
            i("SHA512") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::Sha512, vec![e]) } /
            i("COALESCE") e:ExpressionList() { Expression::Coalesce(e) } /
            i("IF") _ "(" _ a:Expression() _ "," _ b:Expression() _ "," _ c:Expression() _ ")" { Expression::If(Box::new(a), Box::new(b), Box::new(c)) } /
            i("STRLANG") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::StrLang, vec![a, b]) }  /
            i("STRLANGDIR") _ "(" _ a:Expression() _ "," _ b:Expression() _ "," _ c:Expression() _ ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::StrLangDir, vec![a, b, c]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The STRLANGDIR function is only available in SPARQL 1.2")}
            } /
            i("STRDT") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::StrDt, vec![a, b]) } /
            i("sameTerm") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::SameTerm(Box::new(a), Box::new(b)) } /
            name:$(i("isIRI") / i("isURI")) _ "(" _ e:Expression() _ ")" { if state.source_literal_hooks.is_some(){state.source_arena.named_function(Function::IsIri,vec![e],if name.eq_ignore_ascii_case("isURI"){"Builtin_isURI"}else{"Builtin_isIRI"})}else{Expression::FunctionCall(Function::IsIri, vec![e])} } /
            i("isBLANK") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::IsBlank, vec![e]) } /
            i("isLITERAL") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::IsLiteral, vec![e]) } /
            i("isNUMERIC") _ "(" _ e:Expression() _ ")" { Expression::FunctionCall(Function::IsNumeric, vec![e]) } /
            i("hasLang") _ "(" _ e:Expression() _ ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::HasLang, vec![e]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The hasLang function is only available in SPARQL 1.2")}
            } /
            i("hasLangDir") _ "(" _ e:Expression() _ ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::HasLangDir, vec![e]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The hasLangDir function is only available in SPARQL 1.2")}
            } /
            RegexExpression() /
            ExistsFunc() /
            NotExistsFunc() /
            i("TRIPLE") _ "(" _ s:Expression() _ "," _ p:Expression() "," _ o:Expression() ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::Triple, vec![s, p, o]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The TRIPLE function is only available in SPARQL 1.2")}
            } /
            i("SUBJECT") _ "(" _ e:Expression() _ ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::Subject, vec![e]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The SUBJECT function is only available in SPARQL 1.2")}
            } /
            i("PREDICATE") _ "(" _ e:Expression() _ ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::Predicate, vec![e]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The PREDICATE function is only available in SPARQL 1.2")}
            } /
            i("OBJECT") _ "(" _ e:Expression() _ ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::Object, vec![e]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The OBJECT function is only available in SPARQL 1.2")}
            } /
            i("isTriple") _ "(" _ e:Expression() _ ")" {?
                #[cfg(feature = "sparql-12")]{Ok(Expression::FunctionCall(Function::IsTriple, vec![e]))}
                #[cfg(not(feature = "sparql-12"))]{Err("The isTriple function is only available in SPARQL 1.2")}
            } /
            i("ADJUST") _ "("  _ a:Expression() _ "," _ b:Expression() _ ")" {?
                #[cfg(feature = "sep-0002")]{Ok(Expression::FunctionCall(Function::Adjust, vec![a, b]))}
                #[cfg(not(feature = "sep-0002"))]{Err("The ADJUST function is only available in SPARQL-dev SEP 0002")}
            }

        rule RegexExpression() -> Expression =
            i("REGEX") _ "(" _ a:Expression() _ "," _ b:Expression() _ "," _ c:Expression() _ ")" { Expression::FunctionCall(Function::Regex, vec![a, b, c]) } /
            i("REGEX") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::Regex, vec![a, b]) }


        rule SubstringExpression() -> Expression =
            i("SUBSTR") _ "(" _ a:Expression() _ "," _ b:Expression() _ "," _ c:Expression() _ ")" { Expression::FunctionCall(Function::SubStr, vec![a, b, c]) } /
            i("SUBSTR") _ "(" _ a:Expression() _ "," _ b:Expression() _ ")" { Expression::FunctionCall(Function::SubStr, vec![a, b]) }


        rule StrReplaceExpression() -> Expression =
            i("REPLACE") _ "(" _ a:Expression() _ "," _ b:Expression() _ "," _ c:Expression() _ "," _ d:Expression() _ ")" { Expression::FunctionCall(Function::Replace, vec![a, b, c, d]) } /
            i("REPLACE") _ "(" _ a:Expression() _ "," _ b:Expression() _ "," _ c:Expression() _ ")" { Expression::FunctionCall(Function::Replace, vec![a, b, c]) }

        rule ExistsFunc() -> Expression = i("EXISTS") _ p:GroupGraphPattern() {
            match p {ParsedPattern::Source(id)=>state.source_arena.exists(ParsedPattern::Source(id),crate::SourceExistsPolarity::Exists),ParsedPattern::Legacy(pattern)=>Expression::Exists(Box::new(pattern))}
        }

        rule NotExistsFunc() -> Expression = i("NOT") _ i("EXISTS") _ p:GroupGraphPattern() {
            match p {ParsedPattern::Source(id)=>state.source_arena.exists(ParsedPattern::Source(id),crate::SourceExistsPolarity::NotExists),ParsedPattern::Legacy(pattern)=>Expression::Not(Box::new(Expression::Exists(Box::new(pattern))))}
        }

        rule Aggregate() -> ParsedAggregate =
            i("COUNT") _ "(" _ i("DISTINCT") _ "*" _ ")" { (AggregateExpression::CountSolutions { distinct: true }).into() } /
            i("COUNT") _ "(" _ i("DISTINCT") _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Count, expr, distinct: true }).into() } /
            i("COUNT") _ "(" _ "*" _ ")" { (AggregateExpression::CountSolutions { distinct: false }).into() } /
            i("COUNT") _ "(" _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Count, expr, distinct: false }).into() } /
            i("SUM") _ "(" _ i("DISTINCT") _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Sum, expr, distinct: true }).into() } /
            i("SUM") _ "(" _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Sum, expr, distinct: false }).into() } /
            i("MIN") _ "(" _ i("DISTINCT") _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Min, expr, distinct: true }).into() } /
            i("MIN") _ "(" _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Min, expr, distinct: false }).into() } /
            i("MAX") _ "(" _ i("DISTINCT") _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Max, expr, distinct: true }).into() } /
            i("MAX") _ "(" _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Max, expr, distinct: false }).into() } /
            i("AVG") _ "(" _ i("DISTINCT") _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Avg, expr, distinct: true }).into() } /
            i("AVG") _ "(" _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Avg, expr, distinct: false }).into() } /
            i("SAMPLE") _ "(" _ i("DISTINCT") _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Sample, expr, distinct: true }).into() } /
            i("SAMPLE") _ "(" _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::Sample, expr, distinct: false }).into() } /
            i("GROUP_CONCAT") _ "(" _ i("DISTINCT") _ expr:Expression() _ ";" _ i("SEPARATOR") _ "=" _ s:AggregateSeparator() _ ")" { ParsedAggregate{aggregate:AggregateExpression::FunctionCall { name: AggregateFunction::GroupConcat { separator: Some(s.0) }, expr, distinct: true },separator:s.1} } /
            i("GROUP_CONCAT") _ "(" _ i("DISTINCT") _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::GroupConcat { separator: None }, expr, distinct: true }).into() } /
            i("GROUP_CONCAT") _ "(" _ expr:Expression() _ ";" _ i("SEPARATOR") _ "=" _ s:AggregateSeparator() _ ")" { ParsedAggregate{aggregate:AggregateExpression::FunctionCall { name: AggregateFunction::GroupConcat { separator: Some(s.0) }, expr, distinct: false },separator:s.1} } /
            i("GROUP_CONCAT") _ "(" _ expr:Expression() _ ")" { (AggregateExpression::FunctionCall { name: AggregateFunction::GroupConcat { separator: None }, expr, distinct: false }).into() } /
            name:iri() _ "(" _ i("DISTINCT") _ expr:Expression() _ ")" {?
                if state.custom_aggregate_functions.contains(&name) {
                    Ok((AggregateExpression::FunctionCall { name: AggregateFunction::Custom(name), expr, distinct: true }).into())
                } else {
                    Err("This custom function is a regular function and not an aggregate function")
                }
            } /
            name:iri() _ "(" _ expr:Expression() _ ")" {?
                if state.custom_aggregate_functions.contains(&name) {
                    Ok((AggregateExpression::FunctionCall { name: AggregateFunction::Custom(name), expr, distinct: false }).into())
                } else {
                    Err("This custom function is a regular function and not an aggregate function")
                }
            }


        rule AggregateSeparator() -> (String,Option<ParsedSourceLiteral>) = start:position!() value:String() end:position!() {?
            let literal=if state.source_literal_hooks.is_some(){
                Some(state.source_literal_occurrence(start,end,SourceLiteralInput::Parsed(ParsedSourceLiteral{
                    rdf:Literal::new_simple_literal(value.clone()),datatype:SourceDatatype::Absent,language_original:None,origin:SourceLiteralOrigin::QueryPlain,occurrence:None,
                }))?)
            }else{None};
            Ok((value,literal))
        }

        rule iriOrFunction() -> Expression = i: iri() _ a: ArgList()? {?
            if let Some(a) = a {
                if state.custom_aggregate_functions.contains(&i) {
                    Err("This custom function is an aggregate function and not a regular function")
                } else {
                    Ok(if a.1 {
                        state.source_arena.distinct_function(i, a.0)
                    } else {
                        Expression::FunctionCall(Function::Custom(i), a.0)
                    })
                }
            } else {
                Ok(i.into())
            }
        }

        rule RDFLiteral() -> ParsedSourceLiteral = start:position!() value:RDFLiteralRaw() end:position!() {?
            state.source_literal_occurrence(start, end, SourceLiteralInput::Parsed(value))
        }

        rule RDFLiteralRaw() -> ParsedSourceLiteral =
            value:String() _ "^^" _ datatype:iri() {
                ParsedSourceLiteral {
                    occurrence: None,
                    rdf: Literal::new_typed_literal(value, datatype.clone()),
                    datatype: SourceDatatype::Specified(datatype),
                    language_original: None,
                    origin: SourceLiteralOrigin::QueryTypedExistingLiteral,
                }
            } /
            value:String() _ language_and_direction:LANGDIR() {?
                let (language, direction, language_original) = language_and_direction;
                if state.preserve_source_literals && direction.is_some() {
                    return Err("Directional language literals are not qualified in source literal mode")
                }
                #[cfg(feature = "sparql-12")]
                if let Some(is_ltr) = direction {
                    return Ok(ParsedSourceLiteral {
                    occurrence: None,
                        rdf: Literal::new_directional_language_tagged_literal_unchecked(value, language, if is_ltr { oxrdf::BaseDirection::Ltr } else { oxrdf::BaseDirection::Rtl }),
                        datatype: SourceDatatype::Absent,
                        language_original: Some(language_original),
                        origin: SourceLiteralOrigin::QueryLanguageExistingLiteral,
                    })
                }
                #[cfg(not(feature = "sparql-12"))]
                if direction.is_some() {
                    return Err("Literal base directions are only supported in SPARQL 1.2")
                }
                Ok(ParsedSourceLiteral {
                    occurrence: None,
                    rdf: Literal::new_language_tagged_literal_unchecked(value, language),
                    datatype: SourceDatatype::Absent,
                    language_original: Some(language_original),
                    origin: SourceLiteralOrigin::QueryLanguageExistingLiteral,
                })
            } /
            value:String() {
                ParsedSourceLiteral {
                    occurrence: None,
                    rdf: Literal::new_simple_literal(value),
                    datatype: SourceDatatype::Absent,
                    language_original: None,
                    origin: SourceLiteralOrigin::QueryPlain,
                }
            }

        rule NumericLiteral() -> ParsedSourceLiteral  = NumericLiteralUnsigned() / NumericLiteralPositive() / NumericLiteralNegative()

        rule NumericLiteralUnsigned() -> ParsedSourceLiteral =
            start:position!() d:$(DOUBLE()) end:position!() {? state.numeric_literal(NumericTokenKind::Double, NumericTokenSign::Unsigned, d, start, end) } /
            start:position!() d:$(DECIMAL()) end:position!() {? state.numeric_literal(NumericTokenKind::Decimal, NumericTokenSign::Unsigned, d, start, end) } /
            start:position!() i:$(INTEGER()) end:position!() {? state.numeric_literal(NumericTokenKind::Integer, NumericTokenSign::Unsigned, i, start, end) }

        rule NumericLiteralPositive() -> ParsedSourceLiteral =
            start:position!() d:$(DOUBLE_POSITIVE()) end:position!() {? state.numeric_literal(NumericTokenKind::Double, NumericTokenSign::Positive, d, start, end) } /
            start:position!() d:$(DECIMAL_POSITIVE()) end:position!() {? state.numeric_literal(NumericTokenKind::Decimal, NumericTokenSign::Positive, d, start, end) } /
            start:position!() i:$(INTEGER_POSITIVE()) end:position!() {? state.numeric_literal(NumericTokenKind::Integer, NumericTokenSign::Positive, i, start, end) }


        rule NumericLiteralNegative() -> ParsedSourceLiteral =
            start:position!() d:$(DOUBLE_NEGATIVE()) end:position!() {? state.numeric_literal(NumericTokenKind::Double, NumericTokenSign::Negative, d, start, end) } /
            start:position!() d:$(DECIMAL_NEGATIVE()) end:position!() {? state.numeric_literal(NumericTokenKind::Decimal, NumericTokenSign::Negative, d, start, end) } /
            start:position!() i:$(INTEGER_NEGATIVE()) end:position!() {? state.numeric_literal(NumericTokenKind::Integer, NumericTokenSign::Negative, i, start, end) }

        rule BooleanLiteral() -> ParsedSourceLiteral = start:position!() value:BooleanLiteralRaw() end:position!() {?
            state.source_literal_occurrence(start, end, SourceLiteralInput::Parsed(value))
        }
        rule BooleanLiteralRaw() -> ParsedSourceLiteral = value:$("true" / "false") {
            ParsedSourceLiteral {
                    occurrence: None,
                rdf: Literal::new_typed_literal(value, xsd::BOOLEAN),
                datatype: SourceDatatype::Specified(xsd::BOOLEAN.into_owned()),
                language_original: None,
                origin: SourceLiteralOrigin::QueryBooleanToken,
            }
        }

        rule String() -> String = STRING_LITERAL_LONG1() / STRING_LITERAL_LONG2() / STRING_LITERAL1() / STRING_LITERAL2()

        rule iri() -> NamedNode = i:(IRIREF() / PrefixedName()) {
            NamedNode::from(i)
        }

        rule PrefixedName() -> Iri<String> = PNAME_LN() /
            ns:PNAME_NS() {? if let Some(iri) = state.prefixes.get(ns).cloned() {
                Iri::parse(iri).map_err(|_| "prefix IRI parsing failed")
            } else {
                if state.source_update_mode { Ok(state.source_update_unresolved_prefix(ns, "")) }
                else { Err("Prefix not found") }
            } }

        rule BlankNode() -> BlankNode = id:BLANK_NODE_LABEL() {?
            let node = BlankNode::new_unchecked(id);
            if state.used_bnodes.contains(&node) {
                Err("Already used blank node id")
            } else {
                state.currently_used_bnodes.insert(node.clone());
                Ok(node)
            }
        } / ANON() {? state.source_blank_node() }

        rule IRIREF() -> Iri<String> = "<" i:$((!['>'] [_])*) ">" {?
            state.parse_iri(unescape_iriref(i)?).map_err(|_| "IRI parsing failed")
        }

        rule PNAME_NS() -> &'input str = ns:$(PN_PREFIX()?) ":" {
            ns
        }

        rule PNAME_LN() -> Iri<String> = ns:PNAME_NS() local:$(PN_LOCAL()) {?
            if let Some(base) = state.prefixes.get(ns) {
                let mut iri = String::with_capacity(base.len() + local.len());
                iri.push_str(base);
                for chunk in local.split('\\') { // We remove \
                    iri.push_str(chunk);
                }
                Iri::parse(iri).map_err(|_| "IRI parsing failed")
            } else {
                if state.source_update_mode { Ok(state.source_update_unresolved_prefix(ns, local)) }
                else { Err("Prefix not found") }
            }
        }

        rule BLANK_NODE_LABEL() -> &'input str = "_:" b:$((['0'..='9'] / PN_CHARS_U()) PN_CHARS()* ("."+ PN_CHARS()+)*) {
            b
        }

        rule VAR1() -> &'input str = "?" v:$(VARNAME()) { v }

        rule VAR2() -> &'input str = "$" v:$(VARNAME()) { v }

        rule LANGDIR() -> (String, Option<bool>, String) = "@" l:$(['a' ..= 'z' | 'A' ..= 'Z']+ ("-" ['a' ..= 'z' | 'A' ..= 'Z' | '0' ..= '9']+)*) d:$("--" ['a' ..= 'z' | 'A' ..= 'Z']+)? {?
            let language = l.to_ascii_lowercase();
            let language = if state.preserve_source_literals {
                // Fixed RDFLib parser.py LANGTAG uses exactly the ASCII grammar
                // above, without BCP47's primary/subtag length constraints.
                // The original spelling stays in the separate occurrence field.
                language
            } else {
                // Every existing parser entry retains its prior validation.
                LanguageTag::parse(language).map_err(|_| "language tag parsing failed")?.into_inner()
            };
            Ok((
                language,
                d.map(|d| match d {
                    "--ltr" => Ok(true),
                    "--rtl" => Ok(false),
                    _ => Err("the only base directions allowed are 'rtl' and 'ltr'")
                }).transpose()?,
                l.to_owned(),
            ))
        }

        rule INTEGER() = ['0'..='9']+

        rule DECIMAL() = ['0'..='9']* "." ['0'..='9']+

        rule DOUBLE() = (['0'..='9']+ "." ['0'..='9']* / "." ['0'..='9']+ / ['0'..='9']+) EXPONENT()

        rule INTEGER_POSITIVE() = "+" _ INTEGER()

        rule DECIMAL_POSITIVE() = "+" _ DECIMAL()

        rule DOUBLE_POSITIVE() = "+" _ DOUBLE()

        rule INTEGER_NEGATIVE() = "-" _ INTEGER()

        rule DECIMAL_NEGATIVE() = "-" _ DECIMAL()

        rule DOUBLE_NEGATIVE() = "-" _ DOUBLE()

        rule EXPONENT() = ['e' | 'E'] ['+' | '-']? ['0'..='9']+

        rule STRING_LITERAL1() -> String = "'" l:$((STRING_LITERAL1_simple_char() / ECHAR() / UCHAR())*) "'" {?
             unescape_string(l)
        }
        rule STRING_LITERAL1_simple_char() = !['\u{27}' | '\u{5C}' | '\u{0A}' | '\u{0D}'] [_]


        rule STRING_LITERAL2() -> String = "\"" l:$((STRING_LITERAL2_simple_char() / ECHAR() / UCHAR())*) "\"" {?
             unescape_string(l)
        }
        rule STRING_LITERAL2_simple_char() = !['\u{22}' | '\u{5C}' | '\u{0A}' | '\u{0D}'] [_]

        rule STRING_LITERAL_LONG1() -> String = "'''" l:$(STRING_LITERAL_LONG1_inner()*) "'''" {?
             unescape_string(l)
        }
        rule STRING_LITERAL_LONG1_inner() = ("''" / "'")? (STRING_LITERAL_LONG1_simple_char() / ECHAR() / UCHAR())
        rule STRING_LITERAL_LONG1_simple_char() = !['\'' | '\\'] [_]

        rule STRING_LITERAL_LONG2() -> String = "\"\"\"" l:$(STRING_LITERAL_LONG2_inner()*) "\"\"\"" {?
             unescape_string(l)
        }
        rule STRING_LITERAL_LONG2_inner() = ("\"\"" / "\"")? (STRING_LITERAL_LONG2_simple_char() / ECHAR() / UCHAR())
        rule STRING_LITERAL_LONG2_simple_char() = !['"' | '\\'] [_]

        rule UCHAR() = "\\u" HEX() HEX() HEX() HEX() / "\\U" HEX() HEX() HEX() HEX() HEX() HEX() HEX() HEX()

        rule ECHAR() = "\\" ['t' | 'b' | 'n' | 'r' | 'f' | '"' |'\'' | '\\']

        rule NIL() = "(" WS()* ")"

        rule WS() = quiet! { ['\u{20}' | '\u{09}' | '\u{0D}' | '\u{0A}'] }

        rule ANON() = "[" WS()* "]"

        rule PN_CHARS_BASE() = ['A' ..= 'Z' | 'a' ..= 'z' | '\u{00C0}'..='\u{00D6}' | '\u{00D8}'..='\u{00F6}' | '\u{00F8}'..='\u{02FF}' | '\u{0370}'..='\u{037D}' | '\u{037F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}']

        rule PN_CHARS_U() = ['_'] / PN_CHARS_BASE()

        rule VARNAME() = (['0'..='9'] / PN_CHARS_U()) (['0' ..= '9' | '\u{00B7}' | '\u{0300}'..='\u{036F}' | '\u{203F}'..='\u{2040}'] / PN_CHARS_U())*

        rule PN_CHARS() = ['-' | '0' ..= '9' | '\u{00B7}' | '\u{0300}'..='\u{036F}' | '\u{203F}'..='\u{2040}'] / PN_CHARS_U()

        rule PN_PREFIX() = PN_CHARS_BASE() PN_CHARS()* ("."+ PN_CHARS()+)*

        rule PN_LOCAL() = (PN_CHARS_U() / [':' | '0'..='9'] / PLX()) (PN_CHARS() / [':'] / PLX())* (['.']+ (PN_CHARS() / [':'] / PLX())+)?

        rule PLX() = PERCENT() / PN_LOCAL_ESC()

        rule PERCENT() = ['%'] HEX() HEX()

        rule HEX() = ['0' ..= '9' | 'A' ..= 'F' | 'a' ..= 'f']

        rule PN_LOCAL_ESC() = ['\\'] ['_' | '~' | '.' | '-' | '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' | '/' | '?' | '#' | '@' | '%']

        //space
        rule _() = quiet! { ([' ' | '\t' | '\n' | '\r'] / comment())* }

        //comment
        rule comment() = quiet! { ['#'] (!['\r' | '\n'] [_])* }

        rule i(literal: &'static str) = input: $([_]*<{literal.len()}>) {?
            if input.eq_ignore_ascii_case(literal) {
                Ok(())
            } else {
                Err(literal)
            }
        }
    }
}

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod nullable_chain_tests {
    use super::*;

    fn projection_expression(query: Query) -> Expression {
        let Query::Select { pattern, .. } = query else {
            panic!("expected SELECT");
        };
        let GraphPattern::Project { inner, .. } = pattern else {
            panic!("expected projection");
        };
        let GraphPattern::Extend { expression, .. } = *inner else {
            panic!("expected expression projection");
        };
        expression
    }

    fn private_expression(source: &str) -> Expression {
        let query = format!("SELECT ({source} AS ?out) WHERE {{}}");
        projection_expression(
            SparqlParser::new()
                .parse_query_rdflib760_nullable(&query)
                .unwrap()
                .0,
        )
    }

    fn integer(value: i64) -> Expression {
        Literal::from(value).into()
    }

    fn variable(name: &str) -> Expression {
        Variable::new(name).unwrap().into()
    }

    fn count_operand(source: &str) -> Expression {
        let query = format!("SELECT (COUNT({source}) AS ?n) WHERE {{ VALUES ?row {{ 1 2 3 }} }}");
        let (Query::Select { pattern, .. }, _) = SparqlParser::new()
            .parse_query_rdflib760_nullable(&query)
            .unwrap()
        else {
            panic!("expected SELECT");
        };
        let GraphPattern::Project { inner, .. } = pattern else {
            panic!("expected projection");
        };
        let GraphPattern::Extend { inner, .. } = *inner else {
            panic!("expected aggregate assignment");
        };
        let GraphPattern::Group { aggregates, .. } = *inner else {
            panic!("expected aggregate group");
        };
        assert_eq!(aggregates.len(), 1);
        let (
            _,
            AggregateExpression::FunctionCall {
                name,
                expr,
                distinct,
            },
        ) = aggregates.into_iter().next().unwrap()
        else {
            panic!("expected COUNT expression");
        };
        assert_eq!(name, AggregateFunction::Count);
        assert!(!distinct);
        expr
    }

    #[test]
    fn nullable_chain_parser_distinguishes_flat_and_nested_sequences() {
        assert_eq!(
            private_expression("1 / 0 * 1"),
            Expression::MultiplicativeChain {
                first: Box::new(integer(1)),
                tail: vec![
                    (MultiplicativeOperator::Divide, integer(0)),
                    (MultiplicativeOperator::Multiply, integer(1)),
                ],
            }
        );
        assert_eq!(
            private_expression("(1 / 0) * 1"),
            Expression::MultiplicativeChain {
                first: Box::new(private_expression("1 / 0")),
                tail: vec![(MultiplicativeOperator::Multiply, integer(1))],
            }
        );
        assert_eq!(
            private_expression("?missing + 1 + ?fatal"),
            Expression::AdditiveChain {
                first: Box::new(variable("missing")),
                tail: vec![
                    (AdditiveOperator::Add, integer(1)),
                    (AdditiveOperator::Add, variable("fatal")),
                ],
            }
        );
        assert_eq!(
            private_expression("(?missing + 1) + ?fatal"),
            Expression::AdditiveChain {
                first: Box::new(private_expression("?missing + 1")),
                tail: vec![(AdditiveOperator::Add, variable("fatal"))],
            }
        );
    }

    #[test]
    fn nullable_chain_parser_keeps_bare_parentheses_transparent() {
        for source in ["?missing", "(?missing)", "((?missing))"] {
            assert_eq!(private_expression(source), variable("missing"));
            assert_eq!(count_operand(source), variable("missing"));
        }
        assert_eq!(
            count_operand("?missing + 0"),
            Expression::AdditiveChain {
                first: Box::new(variable("missing")),
                tail: vec![(AdditiveOperator::Add, integer(0))],
            }
        );
        assert_eq!(private_expression("((1))"), integer(1));
    }

    #[test]
    fn nullable_chain_parser_preserves_left_order_and_precedence() {
        assert_eq!(
            private_expression("10 - 3 - 2"),
            Expression::AdditiveChain {
                first: Box::new(integer(10)),
                tail: vec![
                    (AdditiveOperator::Subtract, integer(3)),
                    (AdditiveOperator::Subtract, integer(2)),
                ],
            }
        );
        assert_eq!(
            private_expression("10 - (3 - 2)"),
            Expression::AdditiveChain {
                first: Box::new(integer(10)),
                tail: vec![(AdditiveOperator::Subtract, private_expression("3 - 2"))],
            }
        );
        assert_eq!(
            private_expression("2 + 3 * 4"),
            Expression::AdditiveChain {
                first: Box::new(integer(2)),
                tail: vec![(AdditiveOperator::Add, private_expression("3 * 4"))],
            }
        );
        assert_eq!(
            private_expression("2 * 3 * 4"),
            Expression::MultiplicativeChain {
                first: Box::new(integer(2)),
                tail: vec![
                    (MultiplicativeOperator::Multiply, integer(3)),
                    (MultiplicativeOperator::Multiply, integer(4)),
                ],
            }
        );
    }

    #[test]
    fn nullable_chain_display_roundtrip_and_sse_preserve_boundaries() {
        for source in [
            "1 / 0 * 1",
            "(1 / 0) * 1",
            "?missing + 1 + ?fatal",
            "(?missing + 1) + ?fatal",
            "10 - 3 - 2",
            "10 - (3 - 2)",
            "2 + 3 * 4",
            "2 * 3 * 4",
            "?a * (?b + ?c) / ?d",
        ] {
            let expression = private_expression(source);
            assert_eq!(private_expression(&expression.to_string()), expression);
        }
        let expression = private_expression("?a * (?b + ?c) / ?d");
        assert_eq!(expression.to_string(), "(?a * (?b + ?c) / ?d)");
        let mut sse = String::new();
        expression.fmt_sse(&mut sse).unwrap();
        assert_eq!(
            sse,
            "(multiplicative-chain ?a (* (additive-chain ?b (+ ?c))) (/ ?d))"
        );
    }

    #[test]
    fn nullable_chain_parser_keeps_default_and_describe_policy_controls() {
        for (source, expected) in [
            (
                "10 - 3 - 2",
                Expression::Subtract(
                    Box::new(Expression::Subtract(
                        Box::new(integer(10)),
                        Box::new(integer(3)),
                    )),
                    Box::new(integer(2)),
                ),
            ),
            (
                "1 / 0 * 1",
                Expression::Multiply(
                    Box::new(Expression::Divide(
                        Box::new(integer(1)),
                        Box::new(integer(0)),
                    )),
                    Box::new(integer(1)),
                ),
            ),
        ] {
            let query = format!("SELECT ({source} AS ?out) WHERE {{}}");
            assert_eq!(
                projection_expression(SparqlParser::new().parse_query(&query).unwrap()),
                expected
            );
            assert_eq!(
                projection_expression(SparqlParser::new().parse_query_rdflib760(&query).unwrap().0),
                expected
            );
        }
        let describe = "DESCRIBE ?s WHERE { ?s ?p ?o } ORDER BY ?o";
        assert_eq!(
            SparqlParser::new()
                .parse_query_rdflib760_nullable(describe)
                .unwrap(),
            SparqlParser::new().parse_query_rdflib760(describe).unwrap()
        );
        let update = SparqlParser::new()
            .parse_update("INSERT { <urn:s> <urn:p> ?out } WHERE { BIND(10 - 3 - 2 AS ?out) }")
            .unwrap();
        let sse = update.to_sse();
        assert!(!sse.contains("-chain"));
        assert!(sse.contains("(- (- "));
    }

    #[test]
    fn nullable_chain_group_projection_checks_every_tail_operand() {
        for source in ["?a + ?b + ?missing", "?a * ?b / ?missing"] {
            let query = format!(
                "SELECT ({source} AS ?out) WHERE {{ VALUES (?a ?b) {{ (1 2) }} }} GROUP BY ?a ?b"
            );
            assert!(
                SparqlParser::new()
                    .parse_query_rdflib760_nullable(&query)
                    .is_err()
            );
        }
        let query = "SELECT (?a * (?b + ?c) / ?d AS ?out) WHERE { VALUES (?a ?b ?c ?d) { (1 2 3 4) } } GROUP BY ?a ?b ?c ?d";
        assert!(
            SparqlParser::new()
                .parse_query_rdflib760_nullable(query)
                .is_ok()
        );
        let query = "SELECT (COUNT(?a) + COUNT(?b) + COUNT(?c) AS ?out) WHERE {}";
        let expression = projection_expression(
            SparqlParser::new()
                .parse_query_rdflib760_nullable(query)
                .unwrap()
                .0,
        );
        let Expression::AdditiveChain { first, tail } = expression else {
            panic!("expected aggregate projection chain");
        };
        assert!(matches!(*first, Expression::Variable(_)));
        assert_eq!(tail.len(), 2);
        assert!(
            tail.iter()
                .all(|(_, operand)| matches!(operand, Expression::Variable(_)))
        );
    }
}

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod numeric_constructor_tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn numeric_constructor_hook_distinguishes_bare_tokens_and_typed_strings() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&calls);
        let constructor: NumericLiteralConstructor = Rc::new(move |kind, sign, magnitude| {
            seen.borrow_mut().push((kind, sign, magnitude.to_owned()));
            let normalized = magnitude.trim_start_matches('0');
            Ok(Literal::new_typed_literal(
                if normalized.is_empty() {
                    "0"
                } else {
                    normalized
                },
                xsd::INTEGER,
            ))
        });
        let (query, _) = SparqlParser::new().parse_query_rdflib760_numeric(
            "SELECT ?x ?y WHERE { VALUES (?x ?y) { (01 \"01\"^^<http://www.w3.org/2001/XMLSchema#integer>) } }",
            constructor,
        ).unwrap();
        assert_eq!(
            &*calls.borrow(),
            &[(
                NumericTokenKind::Integer,
                NumericTokenSign::Unsigned,
                "01".to_owned()
            )]
        );
        let rendered = query.to_string();
        assert!(rendered.contains("\"1\"^^"));
        assert!(rendered.contains("\"01\"^^"));
    }

    #[test]
    fn numeric_constructor_failure_retains_its_distinct_error_phase() {
        let constructor: NumericLiteralConstructor = Rc::new(|kind, sign, magnitude| {
            if kind == NumericTokenKind::Decimal && sign == NumericTokenSign::Negative {
                return Err(Box::new(std::io::Error::other(
                    "source Literal negation TypeError",
                )));
            }
            Ok(Literal::new_typed_literal(magnitude, xsd::DECIMAL))
        });
        let query = "SELECT ?x WHERE { VALUES ?x { -0.0 } }";
        assert!(SparqlParser::new().parse_query(query).is_ok());
        let error = SparqlParser::new()
            .parse_query_rdflib760_numeric(query, constructor)
            .unwrap_err();
        assert!(matches!(
            error,
            NumericQueryParseError::LiteralConstruction(_)
        ));
    }

    #[test]
    fn numeric_signed_token_spacing_is_private_and_unary_spacing_is_separate() {
        let constructor: NumericLiteralConstructor =
            Rc::new(|_, _, magnitude| Ok(Literal::new_typed_literal(magnitude, xsd::INTEGER)));
        let values = "SELECT ?x WHERE { VALUES ?x { + 1 } }";
        assert!(SparqlParser::new().parse_query(values).is_ok());
        assert!(matches!(
            SparqlParser::new().parse_query_rdflib760_numeric(values, Rc::clone(&constructor)),
            Err(NumericQueryParseError::Syntax(_))
        ));
        assert!(
            SparqlParser::new()
                .parse_query_rdflib760_numeric("SELECT (+ 1 AS ?x) WHERE {}", constructor)
                .is_ok()
        );
    }
}

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_group_tests {
    use super::*;

    fn constructor() -> NumericLiteralConstructor {
        Rc::new(|kind, sign, magnitude| {
            let spelling = match sign {
                NumericTokenSign::Unsigned | NumericTokenSign::Positive => magnitude.to_owned(),
                NumericTokenSign::Negative => format!("-{magnitude}"),
            };
            Ok(Literal::new_typed_literal(
                spelling,
                match kind {
                    NumericTokenKind::Integer => xsd::INTEGER,
                    NumericTokenKind::Decimal => xsd::DECIMAL,
                    NumericTokenKind::Double => xsd::DOUBLE,
                },
            ))
        })
    }

    fn parse(text: &str) -> GraphPattern {
        let (Query::Select { pattern, .. }, _) = SparqlParser::new()
            .parse_query_rdflib760_numeric_aggregates(text, constructor())
            .unwrap()
        else {
            panic!("SELECT required")
        };
        pattern
    }

    fn group(pattern: &GraphPattern) -> &GraphPattern {
        match pattern {
            GraphPattern::SourceGroup { .. } | GraphPattern::Group { .. } => pattern,
            GraphPattern::Project { inner, .. }
            | GraphPattern::Extend { inner, .. }
            | GraphPattern::Filter { inner, .. }
            | GraphPattern::OrderBy { inner, .. }
            | GraphPattern::SourceOrderBy { inner, .. }
            | GraphPattern::Slice { inner, .. } => group(inner),
            _ => panic!("expected group in plan: {pattern:?}"),
        }
    }

    #[test]
    fn source_group_parser_keeps_raw_as_and_pre_assignment_inner() {
        let plan = parse(
            "SELECT ?x (SUM(?x) AS ?s) WHERE { VALUES ?x { 1 2 } } GROUP BY (?x + 1) (?x + 2 AS ?key)",
        );
        let GraphPattern::SourceGroup {
            inner,
            grouping: SourceGrouping::Explicit(conditions),
            ..
        } = group(&plan)
        else {
            panic!("source explicit group")
        };
        assert!(matches!(&**inner, GraphPattern::Values { .. }));
        assert!(matches!(
            &conditions[0],
            SourceGroupCondition::Raw(Expression::AdditiveChain { .. })
        ));
        assert!(
            matches!(&conditions[1], SourceGroupCondition::As { expression: Expression::AdditiveChain { .. }, variable } if variable.as_str() == "key")
        );
        assert_eq!(conditions.len(), 2);
        let text = group(&plan).to_string();
        assert_eq!(text.matches("AS ?key").count(), 1);
        assert!(text.contains("GROUP BY"));
    }

    #[test]
    fn source_group_parser_distinguishes_implicit_and_explicit_empty_inputs() {
        let implicit = parse("SELECT (SUM(?x) AS ?s) WHERE { VALUES ?x {} }");
        let explicit = parse("SELECT (SUM(?x) AS ?s) WHERE { VALUES ?x {} } GROUP BY ?x");
        for (plan, is_implicit) in [(&implicit, true), (&explicit, false)] {
            let GraphPattern::SourceGroup {
                inner, grouping, ..
            } = group(plan)
            else {
                panic!("source group")
            };
            assert_eq!(matches!(grouping, SourceGrouping::Implicit), is_implicit);
            assert!(
                matches!(&**inner, GraphPattern::Values { bindings, .. } if bindings.is_empty())
            );
        }
    }

    #[test]
    fn source_group_parser_preserves_occurrences_having_and_projected_sample_order() {
        let plan = parse(
            "SELECT ?g (SUM(?x) AS ?a) (SUM(?x) AS ?b) WHERE { VALUES (?g ?x) { (1 0) (1 2) } } GROUP BY ?g HAVING (?x)",
        );
        let GraphPattern::SourceGroup { aggregates, .. } = group(&plan) else {
            panic!("source group")
        };
        assert_eq!(aggregates.len(), 4);
        assert_eq!(aggregates[0].1, aggregates[1].1);
        assert_ne!(aggregates[0].0, aggregates[1].0);
        for (index, expected_name, expected_operand) in [
            (0, AggregateFunction::Sum, "x"),
            (1, AggregateFunction::Sum, "x"),
            (2, AggregateFunction::Sample, "x"),
            (3, AggregateFunction::Sample, "g"),
        ] {
            assert_eq!(
                aggregates[index].0.as_str(),
                format!("__agg_{}__", index + 1)
            );
            assert!(
                matches!(&aggregates[index].1, AggregateExpression::FunctionCall { name, expr: Expression::Variable(variable), distinct: false } if name == &expected_name && variable.as_str() == expected_operand)
            );
        }
        let GraphPattern::Project { inner, .. } = &plan else {
            panic!()
        };
        let GraphPattern::Extend { inner, .. } = &**inner else {
            panic!()
        };
        let GraphPattern::Extend { inner, .. } = &**inner else {
            panic!()
        };
        let GraphPattern::Filter { expr, inner } = &**inner else {
            panic!()
        };
        assert_eq!(expr, &Expression::Variable(aggregates[2].0.clone()));
        assert!(
            matches!(&**inner, GraphPattern::Extend { variable, expression: Expression::Variable(source), .. } if variable.as_str() == "g" && source == &aggregates[3].0)
        );
    }

    #[test]
    fn source_group_parser_samples_each_unaggregated_occurrence_without_sampling_inside_sum() {
        let plan = parse("SELECT (?x + SUM(?x) + ?x AS ?s) WHERE { VALUES ?x { 1 } } GROUP BY ?x");
        let GraphPattern::SourceGroup { aggregates, .. } = group(&plan) else {
            panic!()
        };
        assert_eq!(aggregates.len(), 3);
        for (index, expected) in [
            AggregateFunction::Sample,
            AggregateFunction::Sum,
            AggregateFunction::Sample,
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                matches!(&aggregates[index].1, AggregateExpression::FunctionCall { name, expr: Expression::Variable(variable), .. } if name == &expected && variable.as_str() == "x")
            );
        }
    }

    #[test]
    fn source_group_parser_opt_in_keeps_legacy_group_and_deduplication() {
        let text = "SELECT (SUM(?x) AS ?s) (SUM(?x) AS ?t) WHERE { VALUES ?x { 1 } }";
        let (
            Query::Select {
                pattern: legacy, ..
            },
            _,
        ) = SparqlParser::new()
            .parse_query_rdflib760_numeric(text, constructor())
            .unwrap()
        else {
            panic!()
        };
        assert!(
            matches!(group(&legacy), GraphPattern::Group { aggregates, .. } if aggregates.len() == 1)
        );
        assert!(
            matches!(group(&parse(text)), GraphPattern::SourceGroup { aggregates, .. } if aggregates.len() == 2)
        );
    }
}

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_literal_parser_tests {
    use super::*;
    use std::cell::Cell;

    fn constructor() -> NumericLiteralConstructor {
        Rc::new(|kind, sign, magnitude| {
            let datatype = match kind {
                NumericTokenKind::Integer => xsd::INTEGER,
                NumericTokenKind::Decimal => xsd::DECIMAL,
                NumericTokenKind::Double => xsd::DOUBLE,
            };
            let sign = match sign {
                NumericTokenSign::Unsigned => "",
                NumericTokenSign::Positive => "+",
                NumericTokenSign::Negative => "-",
            };
            Ok(Literal::new_typed_literal(
                format!("{sign}{magnitude}"),
                datatype,
            ))
        })
    }

    fn parse(text: &str, source_group: bool) -> Query {
        SparqlParser::new()
            .parse_query_rdflib760_source_literals(text, constructor(), source_group)
            .unwrap()
            .0
    }

    fn table(pattern: &GraphPattern) -> &Vec<Vec<Option<SourceValueTerm>>> {
        match pattern {
            GraphPattern::SourceValues { bindings, .. } => bindings,
            GraphPattern::Project { inner, .. }
            | GraphPattern::Extend { inner, .. }
            | GraphPattern::SourceGroup { inner, .. }
            | GraphPattern::Group { inner, .. } => table(inner),
            _ => panic!("expected source table: {pattern:?}"),
        }
    }

    fn pattern(query: &Query) -> &GraphPattern {
        let Query::Select { pattern, .. } = query else {
            panic!("SELECT")
        };
        pattern
    }

    fn literal(value: &Option<SourceValueTerm>) -> &ParsedSourceLiteral {
        let Some(SourceValueTerm::Literal(literal)) = value else {
            panic!("literal")
        };
        literal
    }

    #[test]
    fn source_literal_parser_preserves_occurrences_before_rdf_canonicalization() {
        let query = parse(
            r#"SELECT * WHERE { VALUES ?x { "a" "a"^^<http://www.w3.org/2001/XMLSchema#string> "a"@EN "a"@en UNDEF <urn:item> } }"#,
            false,
        );
        let rows = table(pattern(&query));
        assert_eq!(rows.len(), 6);
        let plain = literal(&rows[0][0]);
        let typed = literal(&rows[1][0]);
        let upper = literal(&rows[2][0]);
        let lower = literal(&rows[3][0]);
        assert_eq!(plain.rdf, typed.rdf);
        assert_eq!(plain.datatype, SourceDatatype::Absent);
        assert_eq!(
            typed.datatype,
            SourceDatatype::Specified(xsd::STRING.into_owned())
        );
        assert_eq!(plain.origin, SourceLiteralOrigin::QueryPlain);
        assert_eq!(typed.origin, SourceLiteralOrigin::QueryTypedExistingLiteral);
        assert_eq!(upper.rdf, lower.rdf);
        assert_eq!(upper.language_original.as_deref(), Some("EN"));
        assert_eq!(lower.language_original.as_deref(), Some("en"));
        assert_eq!(upper.datatype, SourceDatatype::Absent);
        assert!(rows[4][0].is_none());
        assert!(
            matches!(&rows[5][0], Some(SourceValueTerm::NamedNode(node)) if node.as_str() == "urn:item")
        );
        assert_eq!(parse(&query.to_string(), false), query);
    }

    #[test]
    fn source_literal_expression_leaves_and_display_keep_origin() {
        let query = parse(
            r#"SELECT ?p ?t ?l WHERE { BIND("a" AS ?p) BIND("a"^^<http://www.w3.org/2001/XMLSchema#string> AS ?t) BIND("a"@EN AS ?l) }"#,
            false,
        );
        let GraphPattern::Project { inner, .. } = pattern(&query) else {
            panic!()
        };
        let mut next = &**inner;
        for (name, datatype, language) in [
            ("l", SourceDatatype::Absent, Some("EN")),
            (
                "t",
                SourceDatatype::Specified(xsd::STRING.into_owned()),
                None,
            ),
            ("p", SourceDatatype::Absent, None),
        ] {
            let GraphPattern::Extend {
                inner,
                variable,
                expression: Expression::SourceLiteral(literal),
            } = next
            else {
                panic!("source leaf")
            };
            assert_eq!(variable.as_str(), name);
            assert_eq!(literal.datatype, datatype);
            assert_eq!(literal.language_original.as_deref(), language);
            next = inner;
        }
        let formatted = query.to_string();
        assert!(formatted.contains("^^<http://www.w3.org/2001/XMLSchema#string>"));
        assert!(formatted.contains("@EN"));
        assert_eq!(parse(&formatted, false), query);
    }

    #[test]
    fn source_literal_numeric_callback_is_once_and_display_preserves_token_kind() {
        let calls = Rc::new(Cell::new(0));
        let count = Rc::clone(&calls);
        // A syntax fixture, not a second numeric engine: the callback deliberately
        // emits canonical forms whose RDF display cannot reconstruct their token.
        let constructor: NumericLiteralConstructor = Rc::new(move |kind, sign, magnitude| {
            count.set(count.get() + 1);
            assert_eq!(sign, NumericTokenSign::Unsigned);
            Ok(match (kind, magnitude) {
                (NumericTokenKind::Integer, "01") => Literal::new_typed_literal("1", xsd::INTEGER),
                (NumericTokenKind::Double, "1e0") => Literal::new_typed_literal("1.0", xsd::DOUBLE),
                (NumericTokenKind::Double, "1e400") => {
                    Literal::new_typed_literal("inf", xsd::DOUBLE)
                }
                _ => panic!("unexpected source token"),
            })
        });
        let query = SparqlParser::new()
            .parse_query_rdflib760_source_literals(
                "SELECT * WHERE { VALUES ?x { 01 1e0 1e400 } }",
                Rc::clone(&constructor),
                false,
            )
            .unwrap()
            .0;
        assert_eq!(calls.get(), 3);
        for (row, expected) in table(pattern(&query)).iter().zip(["01", "1e0", "1e400"]) {
            assert!(
                matches!(&literal(&row[0]).origin, SourceLiteralOrigin::QueryNumericToken { magnitude, .. } if magnitude == expected)
            );
        }
        let formatted = query.to_string();
        assert!(formatted.contains("1e400"));
        let again = SparqlParser::new()
            .parse_query_rdflib760_source_literals(&formatted, constructor, false)
            .unwrap()
            .0;
        assert_eq!(calls.get(), 6);
        assert_eq!(again, query);
    }

    #[test]
    fn source_literal_tables_survive_empty_rows_and_group_policy_is_independent() {
        for source_group in [false, true] {
            let query = parse(
                "SELECT (COUNT(*) AS ?n) WHERE { VALUES ?x {} }",
                source_group,
            );
            let GraphPattern::Project { inner, .. } = pattern(&query) else {
                panic!()
            };
            let GraphPattern::Extend { inner, .. } = &**inner else {
                panic!()
            };
            assert_eq!(
                matches!(&**inner, GraphPattern::SourceGroup { .. }),
                source_group
            );
            assert!(table(pattern(&query)).is_empty());
        }
        let query = parse("SELECT * WHERE { VALUES () { () () } }", false);
        assert_eq!(table(pattern(&query)), &vec![vec![], vec![]]);
    }

    #[test]
    fn source_literal_opt_in_preserves_default_canonical_ast() {
        let text = r#"SELECT * WHERE { VALUES ?x { "a" "a"^^<http://www.w3.org/2001/XMLSchema#string> "a"@EN "a"@en } }"#;
        let query = SparqlParser::new().parse_query(text).unwrap();
        let GraphPattern::Project { inner, .. } = pattern(&query) else {
            panic!()
        };
        let GraphPattern::Values { bindings, .. } = &**inner else {
            panic!()
        };
        assert_eq!(bindings[0], bindings[1]);
        assert_eq!(bindings[2], bindings[3]);
        let private = SparqlParser::new()
            .parse_query_rdflib760_numeric(text, constructor())
            .unwrap()
            .0;
        assert_eq!(private, query);
    }
}

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_literal_language_parser_tests {
    use super::*;

    fn constructor() -> NumericLiteralConstructor {
        Rc::new(|_, _, _| panic!("language-only query must not construct numeric tokens"))
    }

    fn query(tag: &str) -> String {
        format!("SELECT ?x WHERE {{ VALUES ?x {{ \"a\"@{tag} }} }}")
    }

    fn source_language(query: &Query) -> &ParsedSourceLiteral {
        let Query::Select {
            pattern: GraphPattern::Project { inner, .. },
            ..
        } = query
        else {
            panic!("SELECT project")
        };
        let GraphPattern::SourceValues { bindings, .. } = &**inner else {
            panic!("source values")
        };
        let Some(SourceValueTerm::Literal(literal)) = &bindings[0][0] else {
            panic!("source literal")
        };
        literal
    }

    #[test]
    fn source_literal_language_accepts_original_regex_domain_and_keeps_token_boundary() {
        // Supplemental source expectations LL-Q01..06. LF/CR/space after the
        // token are parser whitespace; unlike direct-string language input,
        // they are not part of its original language metadata.
        for (tag, original) in [
            ("abcdefghi", "abcdefghi"),
            ("en-abcdefghi", "en-abcdefghi"),
            ("EN", "EN"),
            ("en\n", "en"),
            ("en\r", "en"),
            ("en ", "en"),
        ] {
            let parsed = SparqlParser::new()
                .parse_query_rdflib760_source_literals(&query(tag), constructor(), false)
                .unwrap()
                .0;
            let literal = source_language(&parsed);
            assert_eq!(literal.rdf.value(), "a");
            assert_eq!(
                literal.rdf.language(),
                Some(original.to_ascii_lowercase().as_str())
            );
            assert_eq!(literal.language_original.as_deref(), Some(original));
            assert_eq!(literal.datatype, SourceDatatype::Absent);
            assert_eq!(
                literal.origin,
                SourceLiteralOrigin::QueryLanguageExistingLiteral
            );
        }
    }

    #[test]
    fn source_literal_language_rejects_malformed_tokens_and_keeps_legacy_bcp47_validation() {
        // Full VALUES query parsing rejects a valid prefix followed by invalid
        // tag text; acceptance is not based on a partial regex match alone.
        for tag in ["", "1en", "en_US", "en--US", "en-", "-en", "en/US", "é"] {
            let result = SparqlParser::new().parse_query_rdflib760_source_literals(
                &query(tag),
                constructor(),
                false,
            );
            assert!(
                matches!(result, Err(NumericQueryParseError::Syntax(_))),
                "{tag:?}"
            );
        }
        for tag in ["abcdefghi", "en-abcdefghi"] {
            let text = query(tag);
            assert!(SparqlParser::new().parse_query(&text).is_err());
            assert!(matches!(
                SparqlParser::new().parse_query_rdflib760_numeric(&text, constructor()),
                Err(NumericQueryParseError::Syntax(_)),
            ));
        }
        assert!(SparqlParser::new().parse_query(&query("EN")).is_ok());
    }
}

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_order_parser_tests {
    use super::*;

    fn constructor() -> NumericLiteralConstructor {
        Rc::new(|kind, sign, magnitude| {
            let sign = match sign {
                NumericTokenSign::Unsigned => "",
                NumericTokenSign::Positive => "+",
                NumericTokenSign::Negative => "-",
            };
            Ok(Literal::new_typed_literal(
                format!("{sign}{magnitude}"),
                match kind {
                    NumericTokenKind::Integer => xsd::INTEGER,
                    NumericTokenKind::Decimal => xsd::DECIMAL,
                    NumericTokenKind::Double => xsd::DOUBLE,
                },
            ))
        })
    }

    fn parse(text: &str, source: bool, groups: bool) -> GraphPattern {
        let parser = SparqlParser::new();
        let query = if source {
            parser
                .parse_query_rdflib760_source_literals(text, constructor(), groups)
                .unwrap()
                .0
        } else if groups {
            parser
                .parse_query_rdflib760_numeric_aggregates(text, constructor())
                .unwrap()
                .0
        } else {
            parser.parse_query(text).unwrap()
        };
        let Query::Select { pattern, .. } = query else {
            panic!("SELECT required")
        };
        pattern
    }

    fn order(pattern: &GraphPattern) -> &GraphPattern {
        match pattern {
            GraphPattern::SourceOrderBy { .. } | GraphPattern::OrderBy { .. } => pattern,
            GraphPattern::Project { inner, .. }
            | GraphPattern::Slice { inner, .. }
            | GraphPattern::Distinct { inner }
            | GraphPattern::Reduced { inner } => order(inner),
            _ => panic!("ORDER required: {pattern:?}"),
        }
    }

    fn group(pattern: &GraphPattern) -> &Vec<(Variable, AggregateExpression)> {
        match pattern {
            GraphPattern::SourceGroup { aggregates, .. } => aggregates,
            GraphPattern::SourceOrderBy { inner, .. }
            | GraphPattern::Project { inner, .. }
            | GraphPattern::Extend { inner, .. }
            | GraphPattern::Filter { inner, .. } => group(inner),
            _ => panic!("source group required: {pattern:?}"),
        }
    }

    #[test]
    fn source_order_parser_uses_explicit_literal_axis_and_preserves_modifiers() {
        let text =
            "SELECT DISTINCT ?x WHERE { VALUES ?x { 2 1 } } ORDER BY DESC(?x) OFFSET 1 LIMIT 0";
        for (source, groups) in [(false, false), (false, true), (true, false), (true, true)] {
            let pattern = parse(text, source, groups);
            let GraphPattern::Slice {
                inner,
                start: 1,
                length: Some(0),
            } = &pattern
            else {
                panic!("Slice")
            };
            let GraphPattern::Distinct { inner } = &**inner else {
                panic!("Distinct")
            };
            let GraphPattern::Project { inner, variables } = &**inner else {
                panic!("Project")
            };
            assert_eq!(variables, &[Variable::new_unchecked("x")]);
            assert_eq!(
                matches!(&**inner, GraphPattern::SourceOrderBy { .. }),
                source
            );
            assert_eq!(matches!(&**inner, GraphPattern::OrderBy { .. }), !source);
        }
        let grouped = "SELECT (SUM(?x) AS ?s) WHERE { VALUES ?x { 1 } } ORDER BY ?s";
        assert!(matches!(
            order(&parse(grouped, false, true)),
            GraphPattern::OrderBy { .. }
        ));
        assert!(matches!(
            order(&parse(grouped, true, true)),
            GraphPattern::SourceOrderBy { .. }
        ));
    }

    #[test]
    fn source_order_parser_preserves_raw_keys_duplicates_and_nested_chains() {
        let pattern = parse(
            "SELECT ?x WHERE { VALUES ?x { 2 1 } } ORDER BY (?x + 0) DESC((?x + 0) * 2) (?x + 0)",
            true,
            true,
        );
        let GraphPattern::SourceOrderBy { inner, expression } = order(&pattern) else {
            panic!()
        };
        assert!(matches!(&**inner, GraphPattern::SourceValues { .. }));
        assert_eq!(expression.len(), 3);
        assert_eq!(expression[0], expression[2]);
        assert!(matches!(
            &expression[0],
            OrderExpression::Asc(Expression::AdditiveChain { .. })
        ));
        let OrderExpression::Desc(Expression::MultiplicativeChain { first, tail }) = &expression[1]
        else {
            panic!()
        };
        assert!(matches!(&**first, Expression::AdditiveChain { .. }));
        assert_eq!(tail.len(), 1);
    }

    #[test]
    fn source_order_parser_rewrites_all_conditions_only_when_any_contains_aggregate() {
        let plain = parse(
            "SELECT ?g (SUM(?x) AS ?s) WHERE { VALUES (?g ?x) { (1 1) } } GROUP BY ?g ORDER BY ?x",
            true,
            true,
        );
        let GraphPattern::SourceOrderBy { expression, .. } = order(&plain) else {
            panic!()
        };
        assert_eq!(
            expression,
            &[OrderExpression::Asc(Expression::Variable(
                Variable::new_unchecked("x")
            ))]
        );
        assert_eq!(group(&plain).len(), 2);
        let mixed = parse(
            "SELECT ?g (SUM(?x) AS ?s) WHERE { VALUES (?g ?x) { (1 1) } } GROUP BY ?g ORDER BY ?x SUM(?x)",
            true,
            true,
        );
        let aggregates = group(&mixed);
        assert_eq!(aggregates.len(), 4);
        let GraphPattern::SourceOrderBy { expression, .. } = order(&mixed) else {
            panic!()
        };
        assert_eq!(
            expression,
            &[
                OrderExpression::Asc(Expression::Variable(aggregates[1].0.clone())),
                OrderExpression::Asc(Expression::Variable(aggregates[2].0.clone())),
            ]
        );
        assert!(
            matches!(&aggregates[1].1, AggregateExpression::FunctionCall { name: AggregateFunction::Sample, expr: Expression::Variable(v), .. } if v.as_str() == "x")
        );
        assert!(matches!(
            &aggregates[2].1,
            AggregateExpression::FunctionCall {
                name: AggregateFunction::Sum,
                ..
            }
        ));
    }

    #[test]
    fn source_order_parser_keeps_having_order_and_projected_sample_sequence() {
        let plan = parse(
            "SELECT ?g (SUM(?x) AS ?s) WHERE { VALUES (?g ?x) { (1 1) } } GROUP BY ?g HAVING (?x) ORDER BY ?x SUM(?x) SUM(?x)",
            true,
            true,
        );
        let aggregates = group(&plan);
        assert_eq!(aggregates.len(), 6);
        for (index, function, variable) in [
            (0, AggregateFunction::Sum, "x"),
            (1, AggregateFunction::Sample, "x"),
            (2, AggregateFunction::Sample, "x"),
            (3, AggregateFunction::Sum, "x"),
            (4, AggregateFunction::Sum, "x"),
            (5, AggregateFunction::Sample, "g"),
        ] {
            assert_eq!(
                aggregates[index].0.as_str(),
                format!("__agg_{}__", index + 1)
            );
            assert!(
                matches!(&aggregates[index].1, AggregateExpression::FunctionCall { name, expr: Expression::Variable(v), distinct: false } if name == &function && v.as_str() == variable)
            );
        }
        assert_ne!(aggregates[3].0, aggregates[4].0);
        let GraphPattern::SourceOrderBy { expression, .. } = order(&plan) else {
            panic!()
        };
        assert_eq!(
            expression,
            &(2..5)
                .map(|i| OrderExpression::Asc(Expression::Variable(aggregates[i].0.clone())))
                .collect::<Vec<_>>()
        );
    }
}

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_context_parser_tests {
    use super::*;

    fn constructor() -> NumericLiteralConstructor {
        Rc::new(|kind, sign, magnitude| {
            let sign = match sign {
                NumericTokenSign::Unsigned => "",
                NumericTokenSign::Positive => "+",
                NumericTokenSign::Negative => "-",
            };
            Ok(Literal::new_typed_literal(
                format!("{sign}{magnitude}"),
                match kind {
                    NumericTokenKind::Integer => xsd::INTEGER,
                    NumericTokenKind::Decimal => xsd::DECIMAL,
                    NumericTokenKind::Double => xsd::DOUBLE,
                },
            ))
        })
    }
    fn parse(query: &str) -> GraphPattern {
        let (Query::Select { pattern, .. }, _) = SparqlParser::new()
            .parse_query_rdflib760_source_context(query, constructor(), true)
            .unwrap()
        else {
            panic!("SELECT required")
        };
        pattern
    }
    fn annotations<'a>(pattern: &'a GraphPattern, result: &mut Vec<&'a SourceContextAnnotation>) {
        match pattern {
            GraphPattern::SourceContext { inner, annotation } => {
                result.push(annotation);
                annotations(inner, result);
            }
            GraphPattern::Join { left, right }
            | GraphPattern::LeftJoin { left, right, .. }
            | GraphPattern::Minus { left, right }
            | GraphPattern::Union { left, right } => {
                annotations(left, result);
                annotations(right, result);
            }
            GraphPattern::Filter { inner, .. }
            | GraphPattern::Extend { inner, .. }
            | GraphPattern::Project { inner, .. }
            | GraphPattern::Slice { inner, .. }
            | GraphPattern::Distinct { inner }
            | GraphPattern::SourceGroup { inner, .. }
            | GraphPattern::SourceOrderBy { inner, .. } => annotations(inner, result),
            _ => (),
        }
    }
    fn all(pattern: &GraphPattern) -> Vec<&SourceContextAnnotation> {
        let mut result = Vec::new();
        annotations(pattern, &mut result);
        result
    }
    fn names(scope: &SourceVariableScope) -> Vec<&str> {
        scope.variables.iter().map(Variable::as_str).collect()
    }

    #[test]
    fn source_context_parser_preserves_recursive_lazy_join_roles() {
        for (text, expected) in [
            (
                "SELECT ?x WHERE { VALUES ?x {1} VALUES ?y {2} }",
                vec![SourceJoinMode::Lazy],
            ),
            (
                "SELECT ?x WHERE { VALUES ?x {1} VALUES ?y {2} VALUES ?z {3} }",
                vec![SourceJoinMode::MaterializedRightSet, SourceJoinMode::Lazy],
            ),
            (
                "SELECT ?x WHERE { VALUES ?x {1} {SELECT ?y WHERE { VALUES ?y {2} } LIMIT 1} }",
                vec![SourceJoinMode::MaterializedRightSet],
            ),
            (
                "SELECT ?x WHERE { VALUES ?x {1} {SELECT DISTINCT ?y WHERE { VALUES ?y {2} }} }",
                vec![SourceJoinMode::MaterializedRightSet],
            ),
        ] {
            let pattern = parse(text);
            let modes = all(&pattern)
                .into_iter()
                .filter_map(|annotation| match annotation {
                    SourceContextAnnotation::Join { mode } => Some(*mode),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(modes, expected, "{text}");
        }
    }

    #[test]
    fn source_context_parser_values_scope_is_not_declared_columns() {
        for (text, expected_scope) in [
            (
                "SELECT ?x WHERE { VALUES ?x {1} OPTIONAL {VALUES ?y {2}} }",
                vec![],
            ),
            (
                "SELECT ?z WHERE { BIND(2 AS ?z) OPTIONAL {VALUES ?y {3}} }",
                vec!["z"],
            ),
            (
                "SELECT ?x WHERE { {SELECT ?x WHERE { BIND(2 AS ?z) VALUES ?x {1} }} OPTIONAL {VALUES ?y {3}} }",
                vec!["x", "z"],
            ),
        ] {
            let pattern = parse(text);
            let scopes = all(&pattern)
                .into_iter()
                .filter_map(|annotation| match annotation {
                    SourceContextAnnotation::LeftJoin {
                        left_scope,
                        unconditional,
                    } => {
                        assert!(*unconditional);
                        Some(names(left_scope))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(scopes, vec![expected_scope], "{text}");
        }
    }

    #[test]
    fn source_context_parser_filter_and_extend_keep_original_scope_roles() {
        for (expression, expected) in [("?x", vec!["x"]), ("?x = 1", vec![]), ("?x != 1", vec![])] {
            let pattern = parse(&format!(
                "SELECT ?x WHERE {{VALUES ?x {{1}} FILTER({expression})}}"
            ));
            let scopes = all(&pattern)
                .into_iter()
                .filter_map(|annotation| match annotation {
                    SourceContextAnnotation::Filter { scope } => Some(names(scope)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(scopes, vec![expected]);
        }
        let pattern = parse("SELECT ?z WHERE {VALUES ?x {1} BIND(?x AS ?z)}");
        let scopes = all(&pattern)
            .into_iter()
            .filter_map(|annotation| match annotation {
                SourceContextAnnotation::Extend { scope } => Some(names(scope)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(scopes, vec![vec!["z"]]);
    }

    #[test]
    fn source_context_parser_preserves_invalid_values_translation_identity() {
        for text in [
            "SELECT ?x WHERE {VALUES ?x {}}",
            "SELECT ?x WHERE {VALUES () {()}}",
            "SELECT ?x WHERE {VALUES () {}}",
        ] {
            let pattern = parse(text);
            assert!(
                all(&pattern).iter().any(|annotation| matches!(
                    annotation,
                    SourceContextAnnotation::Values {
                        form: SourceValuesForm::EmptyTranslatedList
                    }
                )),
                "{text}"
            );
        }
        let empty_bgp = parse("SELECT ?x WHERE {}");
        assert!(all(&empty_bgp).is_empty());
        let undef = parse("SELECT ?x WHERE {VALUES ?x {UNDEF}}");
        assert!(all(&undef).iter().any(|annotation| matches!(
            annotation,
            SourceContextAnnotation::Values {
                form: SourceValuesForm::Records
            }
        )));
    }

    #[test]
    fn source_context_parser_preserves_group_as_none_and_sequential_scopes() {
        let pattern = parse(
            "SELECT (COUNT(*) AS ?n) WHERE {VALUES ?x {1}} GROUP BY ?x (?x + 1) (?x AS ?g) (?g AS ?h)",
        );
        let contexts = all(&pattern)
            .into_iter()
            .find_map(|annotation| match annotation {
                SourceContextAnnotation::Group { conditions } => Some(conditions),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            contexts
                .iter()
                .map(|condition| condition.syntax)
                .collect::<Vec<_>>(),
            vec![
                SourceGroupConditionSyntax::BareVariable,
                SourceGroupConditionSyntax::ParenthesizedWithoutAlias,
                SourceGroupConditionSyntax::ParenthesizedAlias,
                SourceGroupConditionSyntax::ParenthesizedAlias,
            ]
        );
        assert!(contexts[0].assignment_scope.is_none());
        assert_eq!(
            names(contexts[1].assignment_scope.as_ref().unwrap()),
            Vec::<&str>::new()
        );
        assert_eq!(
            names(contexts[2].assignment_scope.as_ref().unwrap()),
            vec!["g"]
        );
        assert_eq!(
            names(contexts[3].assignment_scope.as_ref().unwrap()),
            vec!["g", "h"]
        );
        let formatted = pattern.to_string();
        assert!(
            formatted.contains("GROUP BY ?x ")
                && formatted.contains("(?x AS ?g)")
                && formatted.contains("(?g AS ?h)"),
            "{formatted}"
        );
    }

    #[test]
    fn source_context_parser_keeps_old_entry_unannotated_and_sse_explicit() {
        let text = "SELECT ?x WHERE {VALUES ?x {1} OPTIONAL {VALUES ?y {2}}}";
        let (
            Query::Select {
                pattern: legacy, ..
            },
            _,
        ) = SparqlParser::new()
            .parse_query_rdflib760_source_literals(text, constructor(), false)
            .unwrap()
        else {
            panic!()
        };
        assert!(all(&legacy).is_empty());
        let pattern = parse(text);
        let mut sse = String::new();
        pattern.fmt_sse(&mut sse).unwrap();
        assert!(sse.contains("source-context LeftJoin"));
        assert!(sse.contains("left_scope: SourceVariableScope { variables: [] }"));
        let mut output = Vec::new();
        pattern.on_in_scope_variable(|variable| output.push(variable.clone()));
        assert_eq!(output, vec![Variable::new_unchecked("x")]);
    }
}
