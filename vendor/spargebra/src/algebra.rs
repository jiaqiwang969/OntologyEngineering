//! [SPARQL 1.1 Query Algebra](https://www.w3.org/TR/sparql11-query/#sparqlQuery) representation.

use crate::term::*;
use oxrdf::LiteralRef;
use std::fmt;
use std::hash::{Hash, Hasher};

/// A [property path expression](https://www.w3.org/TR/sparql11-query/#defn_PropertyPathExpr).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum PropertyPathExpression {
    NamedNode(NamedNode),
    Reverse(Box<Self>),
    Sequence(Box<Self>, Box<Self>),
    Alternative(Box<Self>, Box<Self>),
    ZeroOrMore(Box<Self>),
    OneOrMore(Box<Self>),
    ZeroOrOne(Box<Self>),
    NegatedPropertySet(Vec<NamedNode>),
}

impl PropertyPathExpression {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::NamedNode(p) => write!(f, "{p}"),
            Self::Reverse(p) => {
                f.write_str("(reverse ")?;
                p.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Alternative(a, b) => {
                f.write_str("(alt ")?;
                a.fmt_sse(f)?;
                f.write_str(" ")?;
                b.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Sequence(a, b) => {
                f.write_str("(seq ")?;
                a.fmt_sse(f)?;
                f.write_str(" ")?;
                b.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::ZeroOrMore(p) => {
                f.write_str("(path* ")?;
                p.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::OneOrMore(p) => {
                f.write_str("(path+ ")?;
                p.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::ZeroOrOne(p) => {
                f.write_str("(path? ")?;
                p.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::NegatedPropertySet(p) => {
                f.write_str("(notoneof")?;
                for p in p {
                    write!(f, " {p}")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl fmt::Display for PropertyPathExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NamedNode(p) => p.fmt(f),
            Self::Reverse(p) => write!(f, "^({p})"),
            Self::Sequence(a, b) => write!(f, "({a} / {b})"),
            Self::Alternative(a, b) => write!(f, "({a} | {b})"),
            Self::ZeroOrMore(p) => write!(f, "({p})*"),
            Self::OneOrMore(p) => write!(f, "({p})+"),
            Self::ZeroOrOne(p) => write!(f, "({p})?"),
            Self::NegatedPropertySet(p) => {
                f.write_str("!(")?;
                for (i, c) in p.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" | ")?;
                    }
                    write!(f, "{c}")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl From<NamedNode> for PropertyPathExpression {
    fn from(p: NamedNode) -> Self {
        Self::NamedNode(p)
    }
}

/// An [expression](https://www.w3.org/TR/sparql11-query/#expressions).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum Expression {
    NamedNode(NamedNode),
    Literal(Literal),
    Variable(Variable),
    /// [Logical-or](https://www.w3.org/TR/sparql11-query/#func-logical-or).
    Or(Box<Self>, Box<Self>),
    /// [Logical-and](https://www.w3.org/TR/sparql11-query/#func-logical-and).
    And(Box<Self>, Box<Self>),
    /// [RDFterm-equal](https://www.w3.org/TR/sparql11-query/#func-RDFterm-equal) and all the XSD equalities.
    Equal(Box<Self>, Box<Self>),
    /// [sameTerm](https://www.w3.org/TR/sparql11-query/#func-sameTerm).
    SameTerm(Box<Self>, Box<Self>),
    /// [op:numeric-greater-than](https://www.w3.org/TR/xpath-functions-31/#func-numeric-greater-than) and other XSD greater than operators.
    Greater(Box<Self>, Box<Self>),
    GreaterOrEqual(Box<Self>, Box<Self>),
    /// [op:numeric-less-than](https://www.w3.org/TR/xpath-functions-31/#func-numeric-less-than) and other XSD greater than operators.
    Less(Box<Self>, Box<Self>),
    LessOrEqual(Box<Self>, Box<Self>),
    /// [IN](https://www.w3.org/TR/sparql11-query/#func-in)
    In(Box<Self>, Vec<Self>),
    /// [op:numeric-add](https://www.w3.org/TR/xpath-functions-31/#func-numeric-add) and other XSD additions.
    Add(Box<Self>, Box<Self>),
    /// [op:numeric-subtract](https://www.w3.org/TR/xpath-functions-31/#func-numeric-subtract) and other XSD subtractions.
    Subtract(Box<Self>, Box<Self>),
    /// [op:numeric-multiply](https://www.w3.org/TR/xpath-functions-31/#func-numeric-multiply) and other XSD multiplications.
    Multiply(Box<Self>, Box<Self>),
    /// [op:numeric-divide](https://www.w3.org/TR/xpath-functions-31/#func-numeric-divide) and other XSD divides.
    Divide(Box<Self>, Box<Self>),
    /// [op:numeric-unary-plus](https://www.w3.org/TR/xpath-functions-31/#func-numeric-unary-plus) and other XSD unary plus.
    UnaryPlus(Box<Self>),
    /// [op:numeric-unary-minus](https://www.w3.org/TR/xpath-functions-31/#func-numeric-unary-minus) and other XSD unary minus.
    UnaryMinus(Box<Self>),
    /// [fn:not](https://www.w3.org/TR/xpath-functions-31/#func-not).
    Not(Box<Self>),
    /// [EXISTS](https://www.w3.org/TR/sparql11-query/#func-filter-exists).
    Exists(Box<GraphPattern>),
    /// [BOUND](https://www.w3.org/TR/sparql11-query/#func-bound).
    Bound(Variable),
    /// [IF](https://www.w3.org/TR/sparql11-query/#func-if).
    If(Box<Self>, Box<Self>, Box<Self>),
    /// [COALESCE](https://www.w3.org/TR/sparql11-query/#func-coalesce).
    Coalesce(Vec<Self>),
    /// A regular function call.
    FunctionCall(Function, Vec<Self>),
    /// A source-ordered, nonempty additive sequence for the private nullable evaluator.
    /// The parser guarantees a nonempty tail; consumers must reject manually empty chains.
    #[doc(hidden)]
    AdditiveChain {
        first: Box<Self>,
        tail: Vec<(AdditiveOperator, Self)>,
    },
    /// A source-ordered, nonempty multiplicative sequence for the private nullable evaluator.
    /// The parser guarantees a nonempty tail; consumers must reject manually empty chains.
    #[doc(hidden)]
    MultiplicativeChain {
        first: Box<Self>,
        tail: Vec<(MultiplicativeOperator, Self)>,
    },
    /// Actual literal-construction occurrence for the private source identity mode.
    /// Appended so existing expression hashes retain their variant discriminants.
    #[doc(hidden)]
    SourceLiteral(ParsedSourceLiteral),
    /// Finalized private source EXISTS syntax/overlay; execution is separately gated.
    #[doc(hidden)]
    SourceExists(crate::SourceExistsExpression<GraphPattern>),
    /// Private grammar capture. A finalized source frontend removes every occurrence.
    #[doc(hidden)]
    SourceSyntax(crate::SourceSyntaxSnapshot),
    /// Original Function metadata; appended without changing old enum hashes.
    #[doc(hidden)]
    SourceFunction {
        function: Function,
        arguments: Vec<Self>,
        syntax: crate::SourceSyntaxSnapshot,
    },
    /// Original !=, distinct from logical NOT of equality on errors.
    #[doc(hidden)]
    SourceNotEqual(Box<Self>, Box<Self>),
}

/// Datatype presence in the source Literal object, independent of RDF 1.1
/// canonicalization of plain strings to xsd:string.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourceDatatype {
    Unknown,
    Absent,
    Specified(NamedNode),
}

/// Actual construction path; no origin is inferred from a canonical RDF value.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourceLiteralOrigin {
    QueryPlain,
    QueryTypedExistingLiteral,
    QueryLanguageExistingLiteral,
    /// The callback has already executed. Consumers must not run it again.
    /// The magnitude also makes formatting round-trip tokens whose normalized
    /// RDF spelling is not a token of the same kind (1e0 -> 1.0, 1e400 -> inf).
    QueryNumericToken {
        kind: crate::NumericTokenKind,
        sign: crate::NumericTokenSign,
        magnitude: String,
    },
    QueryBooleanToken,
}

/// Parser-owned occurrence payload. The RDF term retains its ordinary physical
/// identity; source datatype/language facts are transported separately.
#[doc(hidden)]
#[derive(Clone, Eq, PartialEq)]
pub struct ParsedSourceLiteral {
    pub rdf: Literal,
    pub datatype: SourceDatatype,
    pub language_original: Option<String>,
    pub origin: SourceLiteralOrigin,
    /// Present only for an actual occurrence in a private source frontend.
    #[doc(hidden)]
    pub occurrence: Option<crate::SourceLiteralOccurrenceId>,
}

impl Hash for ParsedSourceLiteral {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // None writes exactly the original derived four-field sequence.
        self.rdf.hash(state);
        self.datatype.hash(state);
        self.language_original.hash(state);
        self.origin.hash(state);
        if let Some(occurrence) = &self.occurrence {
            "source-literal-occurrence/1".hash(state);
            occurrence.hash(state);
        }
    }
}
impl fmt::Debug for ParsedSourceLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ParsedSourceLiteral");
        debug
            .field("rdf", &self.rdf)
            .field("datatype", &self.datatype)
            .field("language_original", &self.language_original)
            .field("origin", &self.origin);
        if let Some(occurrence) = &self.occurrence {
            debug.field("occurrence", occurrence);
        }
        debug.finish()
    }
}

impl fmt::Display for ParsedSourceLiteral {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.origin {
            SourceLiteralOrigin::QueryNumericToken {
                sign, magnitude, ..
            } => {
                let sign = match sign {
                    crate::NumericTokenSign::Unsigned => "",
                    crate::NumericTokenSign::Positive => "+",
                    crate::NumericTokenSign::Negative => "-",
                };
                write!(f, "{sign}{magnitude}")
            }
            SourceLiteralOrigin::QueryBooleanToken => f.write_str(self.rdf.value()),
            _ => {
                // Reuse oxrdf's escaping, but never its canonical datatype or
                // language formatting: that would erase source identity.
                write!(f, "{}", Literal::new_simple_literal(self.rdf.value()))?;
                if let Some(language) = &self.language_original {
                    write!(f, "@{language}")
                } else {
                    match &self.datatype {
                        SourceDatatype::Absent => Ok(()),
                        SourceDatatype::Specified(datatype) => write!(f, "^^{datatype}"),
                        SourceDatatype::Unknown => Err(fmt::Error),
                    }
                }
            }
        }
    }
}

/// A VALUES cell with its actual literal occurrence, including original case.
/// It deliberately has no infallible conversion into GroundTerm.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourceValueTerm {
    NamedNode(NamedNode),
    Literal(ParsedSourceLiteral),
}

impl fmt::Display for SourceValueTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NamedNode(value) => value.fmt(f),
            Self::Literal(value) => value.fmt(f),
        }
    }
}

/// An operator in a private source-ordered additive sequence.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AdditiveOperator {
    Add,
    Subtract,
}

/// An operator in a private source-ordered multiplicative sequence.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MultiplicativeOperator {
    Multiply,
    Divide,
}

impl Expression {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::NamedNode(node) => write!(f, "{node}"),
            Self::Literal(l) => write!(f, "{l}"),
            Self::SourceLiteral(l) => write!(f, "{l}"),
            Self::SourceSyntax(value) => write!(f, "(source-syntax {value:?})"),
            Self::SourceExists(value) => {
                write!(f, "(source-exists {:?} {:?})", value.polarity, value.syntax)
            }
            Self::Variable(var) => write!(f, "{var}"),
            Self::Or(a, b) => fmt_sse_binary_expression(f, "||", a, b),
            Self::And(a, b) => fmt_sse_binary_expression(f, "&&", a, b),
            Self::Equal(a, b) => fmt_sse_binary_expression(f, "=", a, b),
            Self::SourceNotEqual(a, b) => fmt_sse_binary_expression(f, "!=", a, b),
            Self::SameTerm(a, b) => fmt_sse_binary_expression(f, "sameTerm", a, b),
            Self::Greater(a, b) => fmt_sse_binary_expression(f, ">", a, b),
            Self::GreaterOrEqual(a, b) => fmt_sse_binary_expression(f, ">=", a, b),
            Self::Less(a, b) => fmt_sse_binary_expression(f, "<", a, b),
            Self::LessOrEqual(a, b) => fmt_sse_binary_expression(f, "<=", a, b),
            Self::In(a, b) => {
                f.write_str("(in ")?;
                a.fmt_sse(f)?;
                for p in b {
                    f.write_str(" ")?;
                    p.fmt_sse(f)?;
                }
                f.write_str(")")
            }
            Self::Add(a, b) => fmt_sse_binary_expression(f, "+", a, b),
            Self::Subtract(a, b) => fmt_sse_binary_expression(f, "-", a, b),
            Self::Multiply(a, b) => fmt_sse_binary_expression(f, "*", a, b),
            Self::Divide(a, b) => fmt_sse_binary_expression(f, "/", a, b),
            Self::AdditiveChain { first, tail } => {
                f.write_str("(additive-chain ")?;
                first.fmt_sse(f)?;
                for (operator, operand) in tail {
                    f.write_str(match operator {
                        AdditiveOperator::Add => " (+ ",
                        AdditiveOperator::Subtract => " (- ",
                    })?;
                    operand.fmt_sse(f)?;
                    f.write_str(")")?;
                }
                f.write_str(")")
            }
            Self::MultiplicativeChain { first, tail } => {
                f.write_str("(multiplicative-chain ")?;
                first.fmt_sse(f)?;
                for (operator, operand) in tail {
                    f.write_str(match operator {
                        MultiplicativeOperator::Multiply => " (* ",
                        MultiplicativeOperator::Divide => " (/ ",
                    })?;
                    operand.fmt_sse(f)?;
                    f.write_str(")")?;
                }
                f.write_str(")")
            }
            Self::UnaryPlus(e) => fmt_sse_unary_expression(f, "+", e),
            Self::UnaryMinus(e) => fmt_sse_unary_expression(f, "-", e),
            Self::Not(e) => fmt_sse_unary_expression(f, "!", e),
            Self::FunctionCall(function, parameters)
            | Self::SourceFunction {
                function,
                arguments: parameters,
                ..
            } => {
                f.write_str("( ")?;
                function.fmt_sse(f)?;
                for p in parameters {
                    f.write_str(" ")?;
                    p.fmt_sse(f)?;
                }
                f.write_str(")")
            }
            Self::Exists(p) => {
                f.write_str("(exists ")?;
                p.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Bound(v) => {
                write!(f, "(bound {v})")
            }
            Self::If(a, b, c) => {
                f.write_str("(if ")?;
                a.fmt_sse(f)?;
                f.write_str(" ")?;
                b.fmt_sse(f)?;
                f.write_str(" ")?;
                c.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Coalesce(parameters) => {
                f.write_str("(coalesce")?;
                for p in parameters {
                    f.write_str(" ")?;
                    p.fmt_sse(f)?;
                }
                f.write_str(")")
            }
        }
    }

    fn walk<'a>(&'a self, callback: &mut impl FnMut(&'a Self)) {
        callback(self);
        match self {
            Self::Variable(_)
            | Self::Bound(_)
            | Self::NamedNode(_)
            | Self::Literal(_)
            | Self::SourceLiteral(_)
            | Self::Exists(_)
            | Self::SourceExists(_)
            | Self::SourceSyntax(_) => (),
            Self::UnaryPlus(i) | Self::UnaryMinus(i) | Self::Not(i) => i.walk(callback),
            Self::Or(l, r)
            | Self::And(l, r)
            | Self::Equal(l, r)
            | Self::SourceNotEqual(l, r)
            | Self::SameTerm(l, r)
            | Self::Greater(l, r)
            | Self::GreaterOrEqual(l, r)
            | Self::Less(l, r)
            | Self::LessOrEqual(l, r)
            | Self::Add(l, r)
            | Self::Subtract(l, r)
            | Self::Multiply(l, r)
            | Self::Divide(l, r) => {
                l.walk(callback);
                r.walk(callback);
            }
            Self::If(c, l, r) => {
                c.walk(callback);
                l.walk(callback);
                r.walk(callback);
            }
            Self::Coalesce(l)
            | Self::FunctionCall(_, l)
            | Self::SourceFunction { arguments: l, .. } => {
                for e in l {
                    e.walk(callback);
                }
            }
            Self::In(l, r) => {
                l.walk(callback);
                for e in r {
                    e.walk(callback);
                }
            }
            Self::AdditiveChain { first, tail } => {
                first.walk(callback);
                for (_, operand) in tail {
                    operand.walk(callback);
                }
            }
            Self::MultiplicativeChain { first, tail } => {
                first.walk(callback);
                for (_, operand) in tail {
                    operand.walk(callback);
                }
            }
        }
    }

    pub(crate) fn lookup_used_variable<'a>(&'a self, callback: &mut impl FnMut(&'a Variable)) {
        self.walk(&mut |e| {
            if let Self::Variable(v) | Self::Bound(v) = e {
                callback(v)
            } else if let Self::Exists(p) = e {
                p.lookup_used_variables(callback)
            } else if let Self::SourceSyntax(value) = e {
                value.lookup_used_variables(callback)
            } else if let Self::SourceExists(value) = e {
                value.syntax.lookup_used_variables(callback)
            }
        })
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NamedNode(node) => node.fmt(f),
            Self::Literal(l) => l.fmt(f),
            Self::SourceLiteral(l) => l.fmt(f),
            Self::SourceSyntax(value) => write!(f, "# unfinalized source syntax {value:?}"),
            Self::SourceExists(value) => {
                let prefix = if value.polarity == crate::SourceExistsPolarity::NotExists {
                    "NOT EXISTS"
                } else {
                    "EXISTS"
                };
                if let Some(graph) = &value.graph {
                    write!(f, "{prefix} {{ {graph} }}")
                } else {
                    write!(
                        f,
                        "{prefix} {{ # untranslated source syntax {:?}\n }}",
                        value.syntax
                    )
                }
            }
            Self::Variable(var) => var.fmt(f),
            Self::Or(a, b) => write!(f, "({a} || {b})"),
            Self::And(a, b) => write!(f, "({a} && {b})"),
            Self::SourceNotEqual(a, b) => write!(f, "({a} != {b})"),
            Self::Equal(a, b) => {
                write!(f, "({a} = {b})")
            }
            Self::SameTerm(a, b) => {
                write!(f, "sameTerm({a}, {b})")
            }
            Self::Greater(a, b) => {
                write!(f, "({a} > {b})")
            }
            Self::GreaterOrEqual(a, b) => write!(f, "({a} >= {b})"),
            Self::Less(a, b) => {
                write!(f, "({a} < {b})")
            }
            Self::LessOrEqual(a, b) => write!(f, "({a} <= {b})"),
            Self::In(a, b) => {
                write!(f, "({a} IN ")?;
                write_arg_list(b, f)?;
                f.write_str(")")
            }
            Self::Add(a, b) => {
                write!(f, "{a} + {b}")
            }
            Self::Subtract(a, b) => {
                write!(f, "{a} - {b}")
            }
            Self::Multiply(a, b) => {
                write!(f, "{a} * {b}")
            }
            Self::Divide(a, b) => {
                write!(f, "{a} / {b}")
            }
            Self::AdditiveChain { first, tail } => {
                write!(f, "({first}")?;
                for (operator, operand) in tail {
                    let operator = match operator {
                        AdditiveOperator::Add => '+',
                        AdditiveOperator::Subtract => '-',
                    };
                    write!(f, " {operator} {operand}")?;
                }
                f.write_str(")")
            }
            Self::MultiplicativeChain { first, tail } => {
                write!(f, "({first}")?;
                for (operator, operand) in tail {
                    let operator = match operator {
                        MultiplicativeOperator::Multiply => '*',
                        MultiplicativeOperator::Divide => '/',
                    };
                    write!(f, " {operator} {operand}")?;
                }
                f.write_str(")")
            }
            Self::UnaryPlus(e) => write!(f, "+{e}"),
            Self::UnaryMinus(e) => write!(f, "-{e}"),
            Self::Not(e) => match e.as_ref() {
                Self::Exists(p) => write!(f, "NOT EXISTS {{ {p} }}"),
                e => write!(f, "!{e}"),
            },
            Self::FunctionCall(function, parameters)
            | Self::SourceFunction {
                function,
                arguments: parameters,
                ..
            } => {
                write!(f, "{function}")?;
                write_arg_list(parameters, f)
            }
            Self::Bound(v) => write!(f, "BOUND({v})"),
            Self::Exists(p) => write!(f, "EXISTS {{ {p} }}"),
            Self::If(a, b, c) => write!(f, "IF({a}, {b}, {c})"),
            Self::Coalesce(parameters) => {
                f.write_str("COALESCE")?;
                write_arg_list(parameters, f)
            }
        }
    }
}

impl From<NamedNode> for Expression {
    fn from(p: NamedNode) -> Self {
        Self::NamedNode(p)
    }
}

impl From<Literal> for Expression {
    fn from(p: Literal) -> Self {
        Self::Literal(p)
    }
}

impl From<Variable> for Expression {
    fn from(v: Variable) -> Self {
        Self::Variable(v)
    }
}

impl From<NamedNodePattern> for Expression {
    fn from(p: NamedNodePattern) -> Self {
        match p {
            NamedNodePattern::NamedNode(p) => p.into(),
            NamedNodePattern::Variable(p) => p.into(),
        }
    }
}

fn write_arg_list(
    params: impl IntoIterator<Item = impl fmt::Display>,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    f.write_str("(")?;
    let mut cont = false;
    for p in params {
        if cont {
            f.write_str(", ")?;
        }
        p.fmt(f)?;
        cont = true;
    }
    f.write_str(")")
}

/// A function name.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum Function {
    Str,
    Lang,
    LangMatches,
    Datatype,
    Iri,
    BNode,
    Rand,
    Abs,
    Ceil,
    Floor,
    Round,
    Concat,
    SubStr,
    StrLen,
    Replace,
    UCase,
    LCase,
    EncodeForUri,
    Contains,
    StrStarts,
    StrEnds,
    StrBefore,
    StrAfter,
    Year,
    Month,
    Day,
    Hours,
    Minutes,
    Seconds,
    Timezone,
    Tz,
    Now,
    Uuid,
    StrUuid,
    Md5,
    Sha1,
    Sha256,
    Sha384,
    Sha512,
    StrLang,
    StrDt,
    IsIri,
    IsBlank,
    IsLiteral,
    IsNumeric,
    Regex,
    #[cfg(feature = "sparql-12")]
    Triple,
    #[cfg(feature = "sparql-12")]
    Subject,
    #[cfg(feature = "sparql-12")]
    Predicate,
    #[cfg(feature = "sparql-12")]
    Object,
    #[cfg(feature = "sparql-12")]
    IsTriple,
    #[cfg(feature = "sparql-12")]
    LangDir,
    #[cfg(feature = "sparql-12")]
    HasLang,
    #[cfg(feature = "sparql-12")]
    HasLangDir,
    #[cfg(feature = "sparql-12")]
    StrLangDir,
    #[cfg(feature = "sep-0002")]
    Adjust,
    Custom(NamedNode),
}

impl Function {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::Str => f.write_str("str"),
            Self::Lang => f.write_str("lang"),
            Self::LangMatches => f.write_str("langmatches"),
            Self::Datatype => f.write_str("datatype"),
            Self::Iri => f.write_str("iri"),
            Self::BNode => f.write_str("bnode"),
            Self::Rand => f.write_str("rand"),
            Self::Abs => f.write_str("abs"),
            Self::Ceil => f.write_str("ceil"),
            Self::Floor => f.write_str("floor"),
            Self::Round => f.write_str("round"),
            Self::Concat => f.write_str("concat"),
            Self::SubStr => f.write_str("substr"),
            Self::StrLen => f.write_str("strlen"),
            Self::Replace => f.write_str("replace"),
            Self::UCase => f.write_str("ucase"),
            Self::LCase => f.write_str("lcase"),
            Self::EncodeForUri => f.write_str("encode_for_uri"),
            Self::Contains => f.write_str("contains"),
            Self::StrStarts => f.write_str("strstarts"),
            Self::StrEnds => f.write_str("strends"),
            Self::StrBefore => f.write_str("strbefore"),
            Self::StrAfter => f.write_str("strafter"),
            Self::Year => f.write_str("year"),
            Self::Month => f.write_str("month"),
            Self::Day => f.write_str("day"),
            Self::Hours => f.write_str("hours"),
            Self::Minutes => f.write_str("minutes"),
            Self::Seconds => f.write_str("seconds"),
            Self::Timezone => f.write_str("timezone"),
            Self::Tz => f.write_str("tz"),
            Self::Now => f.write_str("now"),
            Self::Uuid => f.write_str("uuid"),
            Self::StrUuid => f.write_str("struuid"),
            Self::Md5 => f.write_str("md5"),
            Self::Sha1 => f.write_str("sha1"),
            Self::Sha256 => f.write_str("sha256"),
            Self::Sha384 => f.write_str("sha384"),
            Self::Sha512 => f.write_str("sha512"),
            Self::StrLang => f.write_str("strlang"),
            Self::StrDt => f.write_str("strdt"),
            Self::IsIri => f.write_str("isiri"),
            Self::IsBlank => f.write_str("isblank"),
            Self::IsLiteral => f.write_str("isliteral"),
            Self::IsNumeric => f.write_str("isnumeric"),
            Self::Regex => f.write_str("regex"),
            #[cfg(feature = "sparql-12")]
            Self::Triple => f.write_str("triple"),
            #[cfg(feature = "sparql-12")]
            Self::Subject => f.write_str("subject"),
            #[cfg(feature = "sparql-12")]
            Self::Predicate => f.write_str("predicate"),
            #[cfg(feature = "sparql-12")]
            Self::Object => f.write_str("object"),
            #[cfg(feature = "sparql-12")]
            Self::IsTriple => f.write_str("istriple"),
            #[cfg(feature = "sparql-12")]
            Function::LangDir => f.write_str("langdir"),
            #[cfg(feature = "sparql-12")]
            Function::HasLang => f.write_str("haslang"),
            #[cfg(feature = "sparql-12")]
            Function::HasLangDir => f.write_str("haslangdir"),
            #[cfg(feature = "sparql-12")]
            Function::StrLangDir => f.write_str("strlangdir"),
            #[cfg(feature = "sep-0002")]
            Self::Adjust => f.write_str("adjust"),
            Self::Custom(iri) => write!(f, "{iri}"),
        }
    }
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Str => f.write_str("STR"),
            Self::Lang => f.write_str("LANG"),
            Self::LangMatches => f.write_str("LANGMATCHES"),
            Self::Datatype => f.write_str("DATATYPE"),
            Self::Iri => f.write_str("IRI"),
            Self::BNode => f.write_str("BNODE"),
            Self::Rand => f.write_str("RAND"),
            Self::Abs => f.write_str("ABS"),
            Self::Ceil => f.write_str("CEIL"),
            Self::Floor => f.write_str("FLOOR"),
            Self::Round => f.write_str("ROUND"),
            Self::Concat => f.write_str("CONCAT"),
            Self::SubStr => f.write_str("SUBSTR"),
            Self::StrLen => f.write_str("STRLEN"),
            Self::Replace => f.write_str("REPLACE"),
            Self::UCase => f.write_str("UCASE"),
            Self::LCase => f.write_str("LCASE"),
            Self::EncodeForUri => f.write_str("ENCODE_FOR_URI"),
            Self::Contains => f.write_str("CONTAINS"),
            Self::StrStarts => f.write_str("STRSTARTS"),
            Self::StrEnds => f.write_str("STRENDS"),
            Self::StrBefore => f.write_str("STRBEFORE"),
            Self::StrAfter => f.write_str("STRAFTER"),
            Self::Year => f.write_str("YEAR"),
            Self::Month => f.write_str("MONTH"),
            Self::Day => f.write_str("DAY"),
            Self::Hours => f.write_str("HOURS"),
            Self::Minutes => f.write_str("MINUTES"),
            Self::Seconds => f.write_str("SECONDS"),
            Self::Timezone => f.write_str("TIMEZONE"),
            Self::Tz => f.write_str("TZ"),
            Self::Now => f.write_str("NOW"),
            Self::Uuid => f.write_str("UUID"),
            Self::StrUuid => f.write_str("STRUUID"),
            Self::Md5 => f.write_str("MD5"),
            Self::Sha1 => f.write_str("SHA1"),
            Self::Sha256 => f.write_str("SHA256"),
            Self::Sha384 => f.write_str("SHA384"),
            Self::Sha512 => f.write_str("SHA512"),
            Self::StrLang => f.write_str("STRLANG"),
            Self::StrDt => f.write_str("STRDT"),
            Self::IsIri => f.write_str("isIRI"),
            Self::IsBlank => f.write_str("isBLANK"),
            Self::IsLiteral => f.write_str("isLITERAL"),
            Self::IsNumeric => f.write_str("isNUMERIC"),
            Self::Regex => f.write_str("REGEX"),
            #[cfg(feature = "sparql-12")]
            Self::Triple => f.write_str("TRIPLE"),
            #[cfg(feature = "sparql-12")]
            Self::Subject => f.write_str("SUBJECT"),
            #[cfg(feature = "sparql-12")]
            Self::Predicate => f.write_str("PREDICATE"),
            #[cfg(feature = "sparql-12")]
            Self::Object => f.write_str("OBJECT"),
            #[cfg(feature = "sparql-12")]
            Self::IsTriple => f.write_str("isTRIPLE"),
            #[cfg(feature = "sparql-12")]
            Function::LangDir => f.write_str("LANGDIR"),
            #[cfg(feature = "sparql-12")]
            Function::HasLang => f.write_str("hasLANG"),
            #[cfg(feature = "sparql-12")]
            Function::HasLangDir => f.write_str("hasLANGDIR"),
            #[cfg(feature = "sparql-12")]
            Function::StrLangDir => f.write_str("STRLANGDIR"),
            #[cfg(feature = "sep-0002")]
            Self::Adjust => f.write_str("ADJUST"),
            Self::Custom(iri) => iri.fmt(f),
        }
    }
}

/// A SPARQL query [graph pattern](https://www.w3.org/TR/sparql11-query/#sparqlQuery).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum GraphPattern {
    /// A [basic graph pattern](https://www.w3.org/TR/sparql11-query/#defn_BasicGraphPattern).
    Bgp { patterns: Vec<TriplePattern> },
    /// A [property path pattern](https://www.w3.org/TR/sparql11-query/#defn_evalPP_predicate).
    Path {
        subject: TermPattern,
        path: PropertyPathExpression,
        object: TermPattern,
    },
    /// [Join](https://www.w3.org/TR/sparql11-query/#defn_algJoin).
    Join { left: Box<Self>, right: Box<Self> },
    /// [LeftJoin](https://www.w3.org/TR/sparql11-query/#defn_algLeftJoin).
    LeftJoin {
        left: Box<Self>,
        right: Box<Self>,
        expression: Option<Expression>,
    },
    /// Lateral join i.e. evaluate right for all result row of left
    #[cfg(feature = "sep-0006")]
    Lateral { left: Box<Self>, right: Box<Self> },
    /// [Filter](https://www.w3.org/TR/sparql11-query/#defn_algFilter).
    Filter { expr: Expression, inner: Box<Self> },
    /// [Union](https://www.w3.org/TR/sparql11-query/#defn_algUnion).
    Union { left: Box<Self>, right: Box<Self> },
    Graph {
        name: NamedNodePattern,
        inner: Box<Self>,
    },
    /// [Extend](https://www.w3.org/TR/sparql11-query/#defn_extend).
    Extend {
        inner: Box<Self>,
        variable: Variable,
        expression: Expression,
    },
    /// [Minus](https://www.w3.org/TR/sparql11-query/#defn_algMinus).
    Minus { left: Box<Self>, right: Box<Self> },
    /// A table used to provide inline values
    Values {
        variables: Vec<Variable>,
        bindings: Vec<Vec<Option<GroundTerm>>>,
    },
    /// [OrderBy](https://www.w3.org/TR/sparql11-query/#defn_algOrdered).
    OrderBy {
        inner: Box<Self>,
        expression: Vec<OrderExpression>,
    },
    /// [Project](https://www.w3.org/TR/sparql11-query/#defn_algProjection).
    Project {
        inner: Box<Self>,
        variables: Vec<Variable>,
    },
    /// [Distinct](https://www.w3.org/TR/sparql11-query/#defn_algDistinct).
    Distinct { inner: Box<Self> },
    /// [Reduced](https://www.w3.org/TR/sparql11-query/#defn_algReduced).
    Reduced { inner: Box<Self> },
    /// [Slice](https://www.w3.org/TR/sparql11-query/#defn_algSlice).
    Slice {
        inner: Box<Self>,
        start: usize,
        length: Option<usize>,
    },
    /// [Group](https://www.w3.org/TR/sparql11-query/#aggregateAlgebra).
    Group {
        inner: Box<Self>,
        variables: Vec<Variable>,
        aggregates: Vec<(Variable, AggregateExpression)>,
    },
    /// [Service](https://www.w3.org/TR/sparql11-federated-query/#defn_evalService).
    Service {
        name: NamedNodePattern,
        inner: Box<Self>,
        silent: bool,
    },
    /// Source aggregate grouping for the explicitly prepared private evaluator.
    /// Appended to preserve all existing variant hash discriminants.
    #[doc(hidden)]
    SourceGroup {
        inner: Box<Self>,
        grouping: SourceGrouping<Expression>,
        aggregates: Vec<(Variable, AggregateExpression)>,
    },
    /// VALUES occurrences for the explicitly selected source literal mode.
    /// Appended to preserve all existing graph-pattern hash discriminants.
    #[doc(hidden)]
    SourceValues {
        variables: Vec<Variable>,
        bindings: Vec<Vec<Option<SourceValueTerm>>>,
    },
    /// Raw source ORDER conditions for the sealed source-literal evaluator.
    /// Appended so all earlier graph-pattern hash discriminants stay unchanged.
    #[doc(hidden)]
    SourceOrderBy {
        inner: Box<Self>,
        expression: Vec<OrderExpression>,
    },
    /// Source binding-context annotations. The wrapped node remains the only
    /// owner of its children, expressions and data. Append-only for old hashes.
    #[doc(hidden)]
    SourceContext {
        inner: Box<Self>,
        annotation: SourceContextAnnotation,
    },
}

/// Original `_vars`, separate from output columns and expression dependencies.
/// Canonical ordering describes set membership, never Python set iteration.
#[doc(hidden)]
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct SourceVariableScope {
    pub variables: Vec<Variable>,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SourceJoinMode {
    Lazy,
    MaterializedRightSet,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SourceValuesForm {
    Records,
    /// translateValues returned a list, not a `values` CompValue. Execution
    /// must preserve that source failure instead of treating it as zero rows.
    EmptyTranslatedList,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SourceGroupConditionSyntax {
    BareVariable,
    BareFunction,
    ParenthesizedWithoutAlias,
    ParenthesizedAlias,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SourceGroupConditionContext {
    pub syntax: SourceGroupConditionSyntax,
    /// Some for the original GroupAs -> Extend, including target None.
    pub assignment_scope: Option<SourceVariableScope>,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourceContextAnnotation {
    Join {
        mode: SourceJoinMode,
    },
    LeftJoin {
        left_scope: SourceVariableScope,
        unconditional: bool,
    },
    Minus,
    Filter {
        scope: SourceVariableScope,
    },
    Extend {
        scope: SourceVariableScope,
    },
    Values {
        form: SourceValuesForm,
    },
    Group {
        conditions: Vec<SourceGroupConditionContext>,
    },
    /// Actual overlay operator without analyse/_addVars mapping fields.
    #[doc(hidden)]
    Unanalyzed {
        no_isolated_scope: bool,
    },
    /// Preserves the source operator boundary; does not grant execution rights.
    #[doc(hidden)]
    ToMultiSet,
    /// Actual source ordered triples; execution requires a sealed Dataset.
    /// Appended to preserve the prior annotation hash discriminants.
    #[doc(hidden)]
    Bgp {
        patterns: Vec<TriplePattern>,
    },
    /// A source GRAPH boundary; it does not grant a dataset to a single graph.
    #[doc(hidden)]
    Graph {
        name: NamedNodePattern,
    },
    /// True native path execution has its own same-session admission.
    #[doc(hidden)]
    PathBgp {
        patterns: Vec<crate::SourceTriplePattern>,
    },
}

/// Original grouping syntax, before raw expressions become synthetic aliases.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourceGrouping<E> {
    Implicit,
    /// Actual parser productions always contain at least one condition.
    Explicit(Vec<SourceGroupCondition<E>>),
}

/// AS assignments execute before any grouping keys are read. Raw expressions
/// are evaluated as keys without passing through assignment's error handling.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourceGroupCondition<E> {
    Raw(E),
    As { expression: E, variable: Variable },
}

#[cfg(test)]
mod source_group_tests {
    use super::*;
    use std::hash::{Hash, Hasher};

    #[test]
    fn source_group_scope_separates_outputs_from_expression_dependencies() {
        let variable = |name: &str| Variable::new_unchecked(name);
        let pattern = GraphPattern::SourceGroup {
            inner: Box::new(GraphPattern::default()),
            grouping: SourceGrouping::Explicit(vec![
                SourceGroupCondition::Raw(Expression::Variable(variable("raw"))),
                SourceGroupCondition::As {
                    expression: Expression::Variable(variable("input")),
                    variable: variable("alias"),
                },
            ]),
            aggregates: vec![(
                variable("result"),
                AggregateExpression::FunctionCall {
                    name: AggregateFunction::Sample,
                    expr: Expression::Variable(variable("sampled")),
                    distinct: false,
                },
            )],
        };
        let mut outputs = Vec::new();
        pattern.lookup_in_scope_variables(&mut |variable| outputs.push(variable.clone()));
        assert_eq!(outputs, [variable("result")]);
        let mut dependencies = Vec::new();
        pattern.lookup_used_variables(&mut |variable| dependencies.push(variable.clone()));
        assert_eq!(
            dependencies,
            [
                variable("raw"),
                variable("input"),
                variable("alias"),
                variable("result"),
                variable("sampled")
            ]
        );
    }

    #[test]
    fn source_group_append_preserves_legacy_group_service_hash_bytes() {
        #[derive(Default)]
        struct RecordingHasher(Vec<u8>);
        impl Hasher for RecordingHasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        let inner = Box::new(GraphPattern::default());
        let old_group_tag = 15_isize + isize::from(cfg!(feature = "sep-0006"));
        for (pattern, tag) in [
            (
                GraphPattern::Group {
                    inner: inner.clone(),
                    variables: vec![Variable::new_unchecked("key")],
                    aggregates: vec![(
                        Variable::new_unchecked("count"),
                        AggregateExpression::CountSolutions { distinct: false },
                    )],
                },
                old_group_tag,
            ),
            (
                GraphPattern::Service {
                    name: NamedNodePattern::NamedNode(NamedNode::new("urn:service").unwrap()),
                    inner,
                    silent: false,
                },
                old_group_tag + 1,
            ),
        ] {
            let mut actual = RecordingHasher::default();
            pattern.hash(&mut actual);
            let mut previous = RecordingHasher::default();
            tag.hash(&mut previous);
            match &pattern {
                GraphPattern::Group {
                    inner,
                    variables,
                    aggregates,
                } => {
                    inner.hash(&mut previous);
                    variables.hash(&mut previous);
                    aggregates.hash(&mut previous);
                }
                GraphPattern::Service {
                    name,
                    inner,
                    silent,
                } => {
                    name.hash(&mut previous);
                    inner.hash(&mut previous);
                    silent.hash(&mut previous);
                }
                _ => unreachable!(),
            }
            assert_eq!(actual.0, previous.0);
        }
    }
}

impl fmt::Display for GraphPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceContext { inner, annotation } => {
                if let (
                    SourceContextAnnotation::Group {
                        conditions: contexts,
                    },
                    Self::SourceGroup {
                        inner,
                        grouping,
                        aggregates,
                    },
                ) = (annotation, inner.as_ref())
                {
                    f.write_str("{SELECT")?;
                    for (variable, aggregate) in aggregates {
                        write!(f, " ({aggregate} AS {variable})")?;
                    }
                    if aggregates.is_empty() {
                        f.write_str(" *")?;
                    }
                    write!(f, " WHERE {{ {inner} }}")?;
                    if let SourceGrouping::Explicit(conditions) = grouping {
                        if conditions.len() != contexts.len() {
                            return Err(fmt::Error);
                        }
                        f.write_str(" GROUP BY")?;
                        for (condition, context) in conditions.iter().zip(contexts) {
                            match (condition, context.syntax) {
                                (
                                    SourceGroupCondition::Raw(expr),
                                    SourceGroupConditionSyntax::BareVariable
                                    | SourceGroupConditionSyntax::BareFunction,
                                ) => write!(f, " {expr}")?,
                                (
                                    SourceGroupCondition::Raw(expr),
                                    SourceGroupConditionSyntax::ParenthesizedWithoutAlias,
                                ) => write!(f, " ({expr})")?,
                                (
                                    SourceGroupCondition::As {
                                        expression,
                                        variable,
                                    },
                                    SourceGroupConditionSyntax::ParenthesizedAlias,
                                ) => write!(f, " ({expression} AS {variable})")?,
                                _ => return Err(fmt::Error),
                            }
                        }
                    } else if !contexts.is_empty() {
                        return Err(fmt::Error);
                    }
                    f.write_str("}")
                } else {
                    inner.fmt(f)
                }
            }
            Self::Bgp { patterns } => {
                for pattern in patterns {
                    write!(f, "{pattern} .")?
                }
                Ok(())
            }
            Self::Path {
                subject,
                path,
                object,
            } => write!(f, "{subject} {path} {object} ."),
            Self::Join { left, right } => {
                match right.as_ref() {
                    Self::SourceContext { .. }
                    | Self::LeftJoin { .. }
                    | Self::Minus { .. }
                    | Self::Extend { .. }
                    | Self::Filter { .. } => {
                        // The second block might be considered as a modification of the first one.
                        write!(f, "{left} {{ {right} }}")
                    }
                    #[cfg(feature = "sep-0006")]
                    Self::Lateral { .. } => {
                        write!(f, "{left} {{ {right} }}")
                    }
                    _ => write!(f, "{left} {right}"),
                }
            }
            Self::LeftJoin {
                left,
                right,
                expression,
            } => {
                if let Some(expr) = expression {
                    write!(f, "{left} OPTIONAL {{ {right} FILTER({expr}) }}")
                } else {
                    write!(f, "{left} OPTIONAL {{ {right} }}")
                }
            }
            #[cfg(feature = "sep-0006")]
            Self::Lateral { left, right } => {
                write!(f, "{left} LATERAL {{ {right} }}")
            }
            Self::Filter { expr, inner } => {
                write!(f, "{inner} FILTER({expr})")
            }
            Self::Union { left, right } => write!(f, "{{ {left} }} UNION {{ {right} }}"),
            Self::Graph { name, inner } => {
                write!(f, "GRAPH {name} {{ {inner} }}")
            }
            Self::Extend {
                inner,
                variable,
                expression,
            } => write!(f, "{inner} BIND({expression} AS {variable})"),
            Self::Minus { left, right } => write!(f, "{left} MINUS {{ {right} }}"),
            Self::Service {
                name,
                inner,
                silent,
            } => {
                if *silent {
                    write!(f, "SERVICE SILENT {name} {{ {inner} }}")
                } else {
                    write!(f, "SERVICE {name} {{ {inner} }}")
                }
            }
            Self::Values {
                variables,
                bindings,
            } => {
                f.write_str("VALUES ( ")?;
                for var in variables {
                    write!(f, "{var} ")?;
                }
                f.write_str(") { ")?;
                for row in bindings {
                    f.write_str("( ")?;
                    for val in row {
                        match val {
                            Some(val) => write!(f, "{val} "),
                            None => f.write_str("UNDEF "),
                        }?;
                    }
                    f.write_str(") ")?;
                }
                f.write_str(" }")
            }
            Self::SourceValues {
                variables,
                bindings,
            } => {
                f.write_str("VALUES ( ")?;
                for var in variables {
                    write!(f, "{var} ")?;
                }
                f.write_str(") { ")?;
                for row in bindings {
                    f.write_str("( ")?;
                    for val in row {
                        match val {
                            Some(val) => write!(f, "{val} "),
                            None => f.write_str("UNDEF "),
                        }?;
                    }
                    f.write_str(") ")?;
                }
                f.write_str(" }")
            }
            Self::Group {
                inner,
                variables,
                aggregates,
            } => {
                f.write_str("{SELECT")?;
                for (a, v) in aggregates {
                    write!(f, " ({v} AS {a})")?;
                }
                for b in variables {
                    write!(f, " {b}")?;
                }
                write!(f, " WHERE {{ {inner} }}")?;
                if !variables.is_empty() {
                    f.write_str(" GROUP BY")?;
                    for v in variables {
                        write!(f, " {v}")?;
                    }
                }
                f.write_str("}")
            }
            Self::SourceGroup {
                inner,
                grouping,
                aggregates,
            } => {
                f.write_str("{SELECT")?;
                for (variable, aggregate) in aggregates {
                    write!(f, " ({aggregate} AS {variable})")?;
                }
                if aggregates.is_empty() {
                    f.write_str(" *")?;
                }
                write!(f, " WHERE {{ {inner} }}")?;
                if let SourceGrouping::Explicit(conditions) = grouping {
                    f.write_str(" GROUP BY")?;
                    for condition in conditions {
                        match condition {
                            SourceGroupCondition::Raw(expression) => {
                                write!(f, " ({expression})")?;
                            }
                            SourceGroupCondition::As {
                                expression,
                                variable,
                            } => {
                                write!(f, " ({expression} AS {variable})")?;
                            }
                        }
                    }
                }
                f.write_str("}")
            }
            p => write!(
                f,
                "{{ {} }}",
                SparqlGraphRootPattern {
                    pattern: p,
                    dataset: None
                }
            ),
        }
    }
}

impl Default for GraphPattern {
    fn default() -> Self {
        Self::Bgp {
            patterns: Vec::new(),
        }
    }
}

impl GraphPattern {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::SourceContext { inner, annotation } => {
                write!(f, "(source-context {annotation:?} ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Bgp { patterns } => {
                f.write_str("(bgp")?;
                for pattern in patterns {
                    f.write_str(" ")?;
                    pattern.fmt_sse(f)?;
                }
                f.write_str(")")
            }
            Self::Path {
                subject,
                path,
                object,
            } => {
                f.write_str("(path ")?;
                subject.fmt_sse(f)?;
                f.write_str(" ")?;
                path.fmt_sse(f)?;
                f.write_str(" ")?;
                object.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Join { left, right } => {
                f.write_str("(join ")?;
                left.fmt_sse(f)?;
                f.write_str(" ")?;
                right.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::LeftJoin {
                left,
                right,
                expression,
            } => {
                f.write_str("(leftjoin ")?;
                left.fmt_sse(f)?;
                f.write_str(" ")?;
                right.fmt_sse(f)?;
                if let Some(expr) = expression {
                    f.write_str(" ")?;
                    expr.fmt_sse(f)?;
                }
                f.write_str(")")
            }
            #[cfg(feature = "sep-0006")]
            Self::Lateral { left, right } => {
                f.write_str("(lateral ")?;
                left.fmt_sse(f)?;
                f.write_str(" ")?;
                right.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Filter { expr, inner } => {
                f.write_str("(filter ")?;
                expr.fmt_sse(f)?;
                f.write_str(" ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Union { left, right } => {
                f.write_str("(union ")?;
                left.fmt_sse(f)?;
                f.write_str(" ")?;
                right.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Graph { name, inner } => {
                f.write_str("(graph ")?;
                name.fmt_sse(f)?;
                f.write_str(" ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Extend {
                inner,
                variable,
                expression,
            } => {
                write!(f, "(extend (({variable} ")?;
                expression.fmt_sse(f)?;
                f.write_str(")) ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Minus { left, right } => {
                f.write_str("(minus ")?;
                left.fmt_sse(f)?;
                f.write_str(" ")?;
                right.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Service {
                name,
                inner,
                silent,
            } => {
                f.write_str("(service ")?;
                if *silent {
                    f.write_str("silent ")?;
                }
                name.fmt_sse(f)?;
                f.write_str(" ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Group {
                inner,
                variables,
                aggregates,
            } => {
                f.write_str("(group (")?;
                for (i, v) in variables.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str(") (")?;
                for (i, (v, a)) in aggregates.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    f.write_str("(")?;
                    a.fmt_sse(f)?;
                    write!(f, " {v})")?;
                }
                f.write_str(") ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::SourceGroup {
                inner,
                grouping,
                aggregates,
            } => {
                f.write_str("(source-group ")?;
                match grouping {
                    SourceGrouping::Implicit => f.write_str("implicit")?,
                    SourceGrouping::Explicit(conditions) => {
                        f.write_str("(explicit")?;
                        for condition in conditions {
                            match condition {
                                SourceGroupCondition::Raw(expression) => {
                                    f.write_str(" (raw ")?;
                                    expression.fmt_sse(f)?;
                                    f.write_str(")")?;
                                }
                                SourceGroupCondition::As {
                                    expression,
                                    variable,
                                } => {
                                    f.write_str(" (as ")?;
                                    expression.fmt_sse(f)?;
                                    write!(f, " {variable})")?;
                                }
                            }
                        }
                        f.write_str(")")?;
                    }
                }
                f.write_str(" (")?;
                for (index, (variable, aggregate)) in aggregates.iter().enumerate() {
                    if index > 0 {
                        f.write_str(" ")?;
                    }
                    f.write_str("(")?;
                    aggregate.fmt_sse(f)?;
                    write!(f, " {variable})")?;
                }
                f.write_str(") ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Values {
                variables,
                bindings,
            } => {
                f.write_str("(table (vars")?;
                for var in variables {
                    write!(f, " {var}")?;
                }
                f.write_str(")")?;
                for row in bindings {
                    f.write_str(" (row")?;
                    for (value, var) in row.iter().zip(variables) {
                        if let Some(value) = value {
                            write!(f, " ({var} {value})")?;
                        }
                    }
                    f.write_str(")")?;
                }
                f.write_str(")")
            }
            Self::SourceValues {
                variables,
                bindings,
            } => {
                f.write_str("(source-table (vars")?;
                for var in variables {
                    write!(f, " {var}")?;
                }
                f.write_str(")")?;
                for row in bindings {
                    f.write_str(" (row")?;
                    for (value, var) in row.iter().zip(variables) {
                        if let Some(value) = value {
                            write!(f, " ({var} {value})")?;
                        }
                    }
                    f.write_str(")")?;
                }
                f.write_str(")")
            }
            Self::OrderBy { inner, expression } | Self::SourceOrderBy { inner, expression } => {
                f.write_str(if matches!(self, Self::SourceOrderBy { .. }) {
                    "(source-order ("
                } else {
                    "(order ("
                })?;
                for (i, c) in expression.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    c.fmt_sse(f)?;
                }
                f.write_str(") ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Project { inner, variables } => {
                f.write_str("(project (")?;
                for (i, v) in variables.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str(") ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Distinct { inner } => {
                f.write_str("(distinct ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Reduced { inner } => {
                f.write_str("(reduced ")?;
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Slice {
                inner,
                start,
                length,
            } => {
                if let Some(length) = length {
                    write!(f, "(slice {start} {length} ")?;
                } else {
                    write!(f, "(slice {start} _ ")?;
                }
                inner.fmt_sse(f)?;
                f.write_str(")")
            }
        }
    }

    /// Calls `callback` on each [in-scope variable](https://www.w3.org/TR/sparql11-query/#variableScope) occurrence.
    pub fn on_in_scope_variable<'a>(&'a self, mut callback: impl FnMut(&'a Variable)) {
        self.lookup_in_scope_variables(&mut callback)
    }

    fn lookup_in_scope_variables<'a>(&'a self, callback: &mut impl FnMut(&'a Variable)) {
        match self {
            Self::Bgp { patterns } => {
                for pattern in patterns {
                    lookup_triple_pattern_variables(pattern, callback)
                }
            }
            Self::Path {
                subject, object, ..
            } => {
                if let TermPattern::Variable(s) = subject {
                    callback(s);
                }
                #[cfg(feature = "sparql-12")]
                if let TermPattern::Triple(s) = subject {
                    lookup_triple_pattern_variables(s, callback)
                }
                if let TermPattern::Variable(o) = object {
                    callback(o);
                }
                #[cfg(feature = "sparql-12")]
                if let TermPattern::Triple(o) = object {
                    lookup_triple_pattern_variables(o, callback)
                }
            }
            Self::Join { left, right }
            | Self::LeftJoin { left, right, .. }
            | Self::Union { left, right } => {
                left.lookup_in_scope_variables(callback);
                right.lookup_in_scope_variables(callback);
            }
            #[cfg(feature = "sep-0006")]
            Self::Lateral { left, right } => {
                left.lookup_in_scope_variables(callback);
                right.lookup_in_scope_variables(callback);
            }
            Self::Graph { name, inner } => {
                if let NamedNodePattern::Variable(g) = &name {
                    callback(g);
                }
                inner.lookup_in_scope_variables(callback);
            }
            Self::Extend {
                inner, variable, ..
            } => {
                callback(variable);
                inner.lookup_in_scope_variables(callback);
            }
            Self::Minus { left, .. } => left.lookup_in_scope_variables(callback),
            Self::Group {
                variables,
                aggregates,
                ..
            } => {
                for v in variables {
                    callback(v);
                }
                for (v, _) in aggregates {
                    callback(v);
                }
            }
            Self::SourceGroup { aggregates, .. } => {
                for (variable, _) in aggregates {
                    callback(variable);
                }
            }
            Self::Values { variables, .. }
            | Self::SourceValues { variables, .. }
            | Self::Project { variables, .. } => {
                for v in variables {
                    callback(v);
                }
            }
            Self::Service { inner, .. }
            | Self::Filter { inner, .. }
            | Self::OrderBy { inner, .. }
            | Self::SourceOrderBy { inner, .. }
            | Self::SourceContext { inner, .. }
            | Self::Distinct { inner }
            | Self::Reduced { inner }
            | Self::Slice { inner, .. } => inner.lookup_in_scope_variables(callback),
        }
    }

    fn lookup_used_variables<'a>(&'a self, callback: &mut impl FnMut(&'a Variable)) {
        match self {
            Self::Bgp { patterns } => {
                for pattern in patterns {
                    lookup_triple_pattern_variables(pattern, callback)
                }
            }
            Self::Path {
                subject, object, ..
            } => {
                if let TermPattern::Variable(s) = subject {
                    callback(s);
                }
                #[cfg(feature = "sparql-12")]
                if let TermPattern::Triple(s) = subject {
                    lookup_triple_pattern_variables(s, callback)
                }
                if let TermPattern::Variable(o) = object {
                    callback(o);
                }
                #[cfg(feature = "sparql-12")]
                if let TermPattern::Triple(o) = object {
                    lookup_triple_pattern_variables(o, callback)
                }
            }
            Self::Join { left, right }
            | Self::Minus { left, right }
            | Self::Union { left, right } => {
                left.lookup_used_variables(callback);
                right.lookup_used_variables(callback);
            }
            Self::LeftJoin {
                left,
                right,
                expression,
            } => {
                left.lookup_used_variables(callback);
                right.lookup_used_variables(callback);
                if let Some(expr) = expression {
                    expr.lookup_used_variable(callback);
                }
            }
            #[cfg(feature = "sep-0006")]
            Self::Lateral { left, right } => {
                left.lookup_used_variables(callback);
                right.lookup_used_variables(callback);
            }
            Self::Graph { name, inner } => {
                if let NamedNodePattern::Variable(g) = &name {
                    callback(g);
                }
                inner.lookup_used_variables(callback);
            }
            Self::Extend {
                inner,
                variable,
                expression,
            } => {
                callback(variable);
                expression.lookup_used_variable(callback);
                inner.lookup_used_variables(callback);
            }
            Self::Group {
                inner,
                variables,
                aggregates,
            } => {
                inner.lookup_used_variables(callback);
                for v in variables {
                    callback(v);
                }
                for (v, expr) in aggregates {
                    callback(v);
                    expr.lookup_used_variables(callback);
                }
            }
            Self::SourceGroup {
                inner,
                grouping,
                aggregates,
            } => {
                inner.lookup_used_variables(callback);
                if let SourceGrouping::Explicit(conditions) = grouping {
                    for condition in conditions {
                        match condition {
                            SourceGroupCondition::Raw(expression) => {
                                expression.lookup_used_variable(callback)
                            }
                            SourceGroupCondition::As {
                                expression,
                                variable,
                            } => {
                                expression.lookup_used_variable(callback);
                                callback(variable);
                            }
                        }
                    }
                }
                for (variable, aggregate) in aggregates {
                    callback(variable);
                    aggregate.lookup_used_variables(callback);
                }
            }
            Self::Values { variables, .. }
            | Self::SourceValues { variables, .. }
            | Self::Project { variables, .. } => {
                for v in variables {
                    callback(v);
                }
            }
            Self::Filter { inner, expr } => {
                expr.lookup_used_variable(callback);
                inner.lookup_used_variables(callback);
            }
            Self::Service { inner, name, .. } => {
                if let NamedNodePattern::Variable(s) = &name {
                    callback(s);
                }
                inner.lookup_used_variables(callback);
            }
            Self::OrderBy { inner, expression } | Self::SourceOrderBy { inner, expression } => {
                for e in expression {
                    e.lookup_used_variables(callback);
                }
                inner.lookup_used_variables(callback);
            }
            Self::Distinct { inner }
            | Self::Reduced { inner }
            | Self::Slice { inner, .. }
            | Self::SourceContext { inner, .. } => inner.lookup_used_variables(callback),
        }
    }
}

pub(crate) fn lookup_triple_pattern_variables<'a>(
    pattern: &'a TriplePattern,
    callback: &mut impl FnMut(&'a Variable),
) {
    if let TermPattern::Variable(s) = &pattern.subject {
        callback(s);
    }
    #[cfg(feature = "sparql-12")]
    if let TermPattern::Triple(s) = &pattern.subject {
        lookup_triple_pattern_variables(s, callback)
    }
    if let NamedNodePattern::Variable(p) = &pattern.predicate {
        callback(p);
    }
    if let TermPattern::Variable(o) = &pattern.object {
        callback(o);
    }
    #[cfg(feature = "sparql-12")]
    if let TermPattern::Triple(o) = &pattern.object {
        lookup_triple_pattern_variables(o, callback)
    }
}

pub(crate) struct SparqlGraphRootPattern<'a> {
    pattern: &'a GraphPattern,
    dataset: Option<&'a QueryDataset>,
}

impl<'a> SparqlGraphRootPattern<'a> {
    pub fn new(pattern: &'a GraphPattern, dataset: Option<&'a QueryDataset>) -> Self {
        Self { pattern, dataset }
    }
}

impl fmt::Display for SparqlGraphRootPattern<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut distinct = false;
        let mut reduced = false;
        let mut order = None;
        let mut start = 0;
        let mut length = None;
        let mut project = Vec::new();

        let mut child = self.pattern;
        loop {
            match child {
                GraphPattern::SourceContext {
                    inner,
                    annotation: SourceContextAnnotation::Extend { .. },
                } => {
                    child = inner;
                }
                GraphPattern::OrderBy { inner, expression }
                | GraphPattern::SourceOrderBy { inner, expression } => {
                    order = Some(expression);
                    child = inner;
                }
                GraphPattern::Project { inner, variables } if project.is_empty() => {
                    project.extend(variables.iter().map(|v| (v, None)));
                    child = inner;
                }
                GraphPattern::Distinct { inner } => {
                    distinct = true;
                    child = inner;
                }
                GraphPattern::Reduced { inner } => {
                    reduced = true;
                    child = inner;
                }
                GraphPattern::Slice {
                    inner,
                    start: s,
                    length: l,
                } => {
                    start = *s;
                    length = *l;
                    child = inner;
                }
                GraphPattern::Extend {
                    inner,
                    expression,
                    variable,
                } if project.iter().any(|(v, _)| *v == variable)
                    && !project.iter().any(|(_, expr)| {
                        expr.is_some_and(|expr: &Expression| {
                            let mut found = false;
                            expr.lookup_used_variable(&mut |v| found |= v == variable);
                            found
                        })
                    }) =>
                {
                    // This simplification only works if the extended variable is in the projection and not used in another expression of the projection.
                    project
                        .iter_mut()
                        .find(|(v, _)| *v == variable)
                        .ok_or(fmt::Error)?
                        .1 = Some(expression);
                    child = inner
                }
                p => {
                    f.write_str("SELECT")?;
                    if distinct {
                        f.write_str(" DISTINCT")?;
                    }
                    if reduced {
                        f.write_str(" REDUCED")?;
                    }
                    if project.is_empty() {
                        f.write_str(" *")?;
                    } else {
                        for (variable, expr) in project {
                            if let Some(expr) = expr {
                                write!(f, " ({expr} AS {variable})")
                            } else {
                                write!(f, " {variable}")
                            }?;
                        }
                    }
                    if let Some(dataset) = self.dataset {
                        write!(f, " {dataset}")?;
                    }
                    write!(f, " WHERE {{ {p} }}")?;
                    if let Some(order) = order {
                        f.write_str(" ORDER BY")?;
                        for c in order {
                            write!(f, " {c}")?;
                        }
                    }
                    if start > 0 {
                        write!(f, " OFFSET {start}")?;
                    }
                    if let Some(length) = length {
                        write!(f, " LIMIT {length}")?;
                    }
                    return Ok(());
                }
            }
        }
    }
}

/// A set function used in aggregates (c.f. [`GraphPattern::Group`]).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum AggregateExpression {
    /// [Count](https://www.w3.org/TR/sparql11-query/#defn_aggCount) with *.
    CountSolutions { distinct: bool },
    FunctionCall {
        name: AggregateFunction,
        expr: Expression,
        distinct: bool,
    },
}

impl AggregateExpression {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::CountSolutions { distinct } => {
                f.write_str("(count")?;
                if *distinct {
                    f.write_str(" distinct")?;
                }
                f.write_str(")")
            }
            Self::FunctionCall {
                name:
                    AggregateFunction::GroupConcat {
                        separator: Some(separator),
                    },
                expr,
                distinct,
            } => {
                f.write_str("(group_concat ")?;
                if *distinct {
                    f.write_str("distinct ")?;
                }
                expr.fmt_sse(f)?;
                write!(f, " {})", LiteralRef::new_simple_literal(separator))
            }
            Self::FunctionCall {
                name,
                expr,
                distinct,
            } => {
                f.write_str("(")?;
                name.fmt_sse(f)?;
                f.write_str(" ")?;
                if *distinct {
                    f.write_str("distinct ")?;
                }
                expr.fmt_sse(f)?;
                f.write_str(")")
            }
        }
    }

    fn lookup_used_variables<'a>(&'a self, callback: &mut impl FnMut(&'a Variable)) {
        if let Self::FunctionCall { expr, .. } = self {
            expr.lookup_used_variable(callback);
        }
    }
}

impl fmt::Display for AggregateExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CountSolutions { distinct } => {
                if *distinct {
                    f.write_str("COUNT(DISTINCT *)")
                } else {
                    f.write_str("COUNT(*)")
                }
            }
            Self::FunctionCall {
                name:
                    AggregateFunction::GroupConcat {
                        separator: Some(separator),
                    },
                expr,
                distinct,
            } => {
                if *distinct {
                    write!(
                        f,
                        "GROUP_CONCAT(DISTINCT {}; SEPARATOR = {})",
                        expr,
                        LiteralRef::new_simple_literal(separator)
                    )
                } else {
                    write!(
                        f,
                        "GROUP_CONCAT({}; SEPARATOR = {})",
                        expr,
                        LiteralRef::new_simple_literal(separator)
                    )
                }
            }
            Self::FunctionCall {
                name,
                expr,
                distinct,
            } => {
                if *distinct {
                    write!(f, "{name}(DISTINCT {expr})")
                } else {
                    write!(f, "{name}({expr})")
                }
            }
        }
    }
}

/// An aggregate function name.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum AggregateFunction {
    /// [Count](https://www.w3.org/TR/sparql11-query/#defn_aggCount) with *.
    Count,
    /// [Sum](https://www.w3.org/TR/sparql11-query/#defn_aggSum).
    Sum,
    /// [Avg](https://www.w3.org/TR/sparql11-query/#defn_aggAvg).
    Avg,
    /// [Min](https://www.w3.org/TR/sparql11-query/#defn_aggMin).
    Min,
    /// [Max](https://www.w3.org/TR/sparql11-query/#defn_aggMax).
    Max,
    /// [GroupConcat](https://www.w3.org/TR/sparql11-query/#defn_aggGroupConcat).
    GroupConcat {
        separator: Option<String>,
    },
    /// [Sample](https://www.w3.org/TR/sparql11-query/#defn_aggSample).
    Sample,
    Custom(NamedNode),
}

impl AggregateFunction {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::Count => f.write_str("count"),
            Self::Sum => f.write_str("sum"),
            Self::Avg => f.write_str("avg"),
            Self::Min => f.write_str("min"),
            Self::Max => f.write_str("max"),
            Self::GroupConcat { .. } => f.write_str("group_concat"),
            Self::Sample => f.write_str("sample"),
            Self::Custom(iri) => write!(f, "{iri}"),
        }
    }
}

impl fmt::Display for AggregateFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Count => f.write_str("COUNT"),
            Self::Sum => f.write_str("SUM"),
            Self::Avg => f.write_str("AVG"),
            Self::Min => f.write_str("MIN"),
            Self::Max => f.write_str("MAX"),
            Self::GroupConcat { .. } => f.write_str("GROUP_CONCAT"),
            Self::Sample => f.write_str("SAMPLE"),
            Self::Custom(iri) => iri.fmt(f),
        }
    }
}

/// An ordering comparator used by [`GraphPattern::OrderBy`].
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum OrderExpression {
    /// Ascending order
    Asc(Expression),
    /// Descending order
    Desc(Expression),
}

impl OrderExpression {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::Asc(e) => {
                f.write_str("(asc ")?;
                e.fmt_sse(f)?;
                f.write_str(")")
            }
            Self::Desc(e) => {
                f.write_str("(desc ")?;
                e.fmt_sse(f)?;
                f.write_str(")")
            }
        }
    }

    fn lookup_used_variables<'a>(&'a self, callback: &mut impl FnMut(&'a Variable)) {
        let (Self::Asc(e) | Self::Desc(e)) = self;
        e.lookup_used_variable(callback);
    }
}

impl fmt::Display for OrderExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Asc(e) => write!(f, "ASC({e})"),
            Self::Desc(e) => write!(f, "DESC({e})"),
        }
    }
}

/// A SPARQL query [dataset specification](https://www.w3.org/TR/sparql11-query/#specifyingDataset).
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub struct QueryDataset {
    pub default: Vec<NamedNode>,
    pub named: Option<Vec<NamedNode>>,
}

impl QueryDataset {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        f.write_str("(")?;
        for (i, graph_name) in self.default.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{graph_name}")?;
        }
        if let Some(named) = &self.named {
            for (i, graph_name) in named.iter().enumerate() {
                if !self.default.is_empty() || i > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "(named {graph_name})")?;
            }
        }
        f.write_str(")")
    }
}

impl fmt::Display for QueryDataset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for g in &self.default {
            write!(f, " FROM {g}")?;
        }
        if let Some(named) = &self.named {
            for g in named {
                write!(f, " FROM NAMED {g}")?;
            }
        }
        Ok(())
    }
}

/// A target RDF graph for update operations.
///
/// Could be a specific graph, all named graphs or the complete dataset.
#[derive(Eq, PartialEq, Debug, Clone, Hash)]
pub enum GraphTarget {
    NamedNode(NamedNode),
    DefaultGraph,
    NamedGraphs,
    AllGraphs,
}

impl GraphTarget {
    /// Formats using the [SPARQL S-Expression syntax](https://jena.apache.org/documentation/notes/sse.html).
    pub(crate) fn fmt_sse(&self, f: &mut impl fmt::Write) -> fmt::Result {
        match self {
            Self::NamedNode(node) => write!(f, "{node}"),
            Self::DefaultGraph => f.write_str("default"),
            Self::NamedGraphs => f.write_str("named"),
            Self::AllGraphs => f.write_str("all"),
        }
    }
}

impl fmt::Display for GraphTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NamedNode(node) => write!(f, "GRAPH {node}"),
            Self::DefaultGraph => f.write_str("DEFAULT"),
            Self::NamedGraphs => f.write_str("NAMED"),
            Self::AllGraphs => f.write_str("ALL"),
        }
    }
}

impl From<NamedNode> for GraphTarget {
    fn from(node: NamedNode) -> Self {
        Self::NamedNode(node)
    }
}

impl From<GraphName> for GraphTarget {
    fn from(graph_name: GraphName) -> Self {
        match graph_name {
            GraphName::NamedNode(node) => Self::NamedNode(node),
            GraphName::DefaultGraph => Self::DefaultGraph,
        }
    }
}

#[inline]
fn fmt_sse_unary_expression(f: &mut impl fmt::Write, name: &str, e: &Expression) -> fmt::Result {
    write!(f, "({name} ")?;
    e.fmt_sse(f)?;
    f.write_str(")")
}

#[inline]
fn fmt_sse_binary_expression(
    f: &mut impl fmt::Write,
    name: &str,
    a: &Expression,
    b: &Expression,
) -> fmt::Result {
    write!(f, "({name} ")?;
    a.fmt_sse(f)?;
    f.write_str(" ")?;
    b.fmt_sse(f)?;
    f.write_str(")")
}

#[cfg(test)]
mod nullable_chain_tests {
    use super::*;
    use std::hash::{Hash, Hasher};

    #[derive(Default)]
    struct HashBytes(Vec<u8>);

    impl Hasher for HashBytes {
        fn finish(&self) -> u64 {
            0
        }

        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }

    #[test]
    fn nullable_chain_variants_preserve_all_legacy_expression_hash_bytes() {
        // The pre-chain enum's discriminants were 0..=23, in this order.
        // Compare the full derived byte stream to that frozen tag and the
        // original field order, without introducing a second expression enum.
        macro_rules! check {
            ($tag:expr, $expression:expr, $($field:expr),+ $(,)?) => {{
                let expression = $expression;
                let mut actual = HashBytes::default();
                expression.hash(&mut actual);
                let mut expected = HashBytes::default();
                ($tag as isize).hash(&mut expected);
                $($field.hash(&mut expected);)+
                assert_eq!(actual.0, expected.0, "legacy tag {}: {expression:?}", $tag);
            }};
        }

        let node = NamedNode::new("urn:hash-control").unwrap();
        let literal = Literal::from(7_i64);
        let variable = Variable::new("hash_control").unwrap();
        let first = Expression::Variable(variable.clone());
        let second = Expression::Literal(literal.clone());
        let third = Expression::NamedNode(node.clone());
        let arguments = vec![first.clone(), second.clone()];
        let graph = GraphPattern::default();

        check!(0, Expression::NamedNode(node.clone()), node);
        check!(1, Expression::Literal(literal.clone()), literal);
        check!(2, Expression::Variable(variable.clone()), variable);
        for (tag, constructor) in [
            (
                3,
                Expression::Or as fn(Box<Expression>, Box<Expression>) -> Expression,
            ),
            (4, Expression::And),
            (5, Expression::Equal),
            (6, Expression::SameTerm),
            (7, Expression::Greater),
            (8, Expression::GreaterOrEqual),
            (9, Expression::Less),
            (10, Expression::LessOrEqual),
            (12, Expression::Add),
            (13, Expression::Subtract),
            (14, Expression::Multiply),
            (15, Expression::Divide),
        ] {
            check!(
                tag,
                constructor(Box::new(first.clone()), Box::new(second.clone())),
                first,
                second
            );
        }
        check!(
            11,
            Expression::In(Box::new(first.clone()), arguments.clone()),
            first,
            arguments
        );
        check!(16, Expression::UnaryPlus(Box::new(first.clone())), first);
        check!(17, Expression::UnaryMinus(Box::new(first.clone())), first);
        check!(18, Expression::Not(Box::new(first.clone())), first);
        check!(19, Expression::Exists(Box::new(graph.clone())), graph);
        check!(20, Expression::Bound(variable.clone()), variable);
        check!(
            21,
            Expression::If(
                Box::new(first.clone()),
                Box::new(second.clone()),
                Box::new(third.clone())
            ),
            first,
            second,
            third
        );
        check!(22, Expression::Coalesce(arguments.clone()), arguments);
        check!(
            23,
            Expression::FunctionCall(Function::Str, arguments.clone()),
            Function::Str,
            arguments
        );
    }

    #[test]
    fn nullable_chain_variable_visitor_preserves_every_operand_in_source_order() {
        let variables: Vec<_> = ["a", "b", "c", "d"]
            .into_iter()
            .map(|name| Variable::new(name).unwrap())
            .collect();
        let expression = Expression::MultiplicativeChain {
            first: Box::new(variables[0].clone().into()),
            tail: vec![
                (
                    MultiplicativeOperator::Multiply,
                    Expression::AdditiveChain {
                        first: Box::new(variables[1].clone().into()),
                        tail: vec![(AdditiveOperator::Add, variables[2].clone().into())],
                    },
                ),
                (MultiplicativeOperator::Divide, variables[3].clone().into()),
            ],
        };
        let mut visited = Vec::new();
        expression.lookup_used_variable(&mut |variable| visited.push(variable.clone()));
        assert_eq!(visited, variables);
    }
}

#[cfg(test)]
mod source_literal_hash_tests {
    use super::*;
    use std::hash::{Hash, Hasher};

    #[test]
    fn source_literal_append_retains_previous_source_group_and_chain_hash_bytes() {
        #[derive(Default)]
        struct RecordingHasher(Vec<u8>);
        impl Hasher for RecordingHasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        let first = Box::new(Expression::Literal(1.into()));
        let tail = vec![(
            MultiplicativeOperator::Multiply,
            Expression::Literal(2.into()),
        )];
        let chain = Expression::MultiplicativeChain {
            first: first.clone(),
            tail: tail.clone(),
        };
        let mut actual = RecordingHasher::default();
        chain.hash(&mut actual);
        let mut previous = RecordingHasher::default();
        25_isize.hash(&mut previous);
        first.hash(&mut previous);
        tail.hash(&mut previous);
        assert_eq!(actual.0, previous.0);

        let inner = Box::new(GraphPattern::default());
        let grouping = SourceGrouping::<Expression>::Implicit;
        let aggregates = vec![(
            Variable::new_unchecked("n"),
            AggregateExpression::CountSolutions { distinct: false },
        )];
        let group = GraphPattern::SourceGroup {
            inner: inner.clone(),
            grouping: grouping.clone(),
            aggregates: aggregates.clone(),
        };
        let mut actual = RecordingHasher::default();
        group.hash(&mut actual);
        let mut previous = RecordingHasher::default();
        (17_isize + isize::from(cfg!(feature = "sep-0006"))).hash(&mut previous);
        inner.hash(&mut previous);
        grouping.hash(&mut previous);
        aggregates.hash(&mut previous);
        assert_eq!(actual.0, previous.0);
    }
}

#[cfg(test)]
mod source_order_transport_tests {
    use super::*;

    #[test]
    fn source_order_scope_separates_key_dependencies_from_outputs() {
        let row = Variable::new_unchecked("row");
        let key = Variable::new_unchecked("key_only");
        let pattern = GraphPattern::SourceOrderBy {
            inner: Box::new(GraphPattern::Values {
                variables: vec![row.clone()],
                bindings: vec![],
            }),
            expression: vec![OrderExpression::Asc(Expression::Variable(key.clone()))],
        };
        let mut scope = Vec::new();
        pattern.lookup_in_scope_variables(&mut |v| scope.push(v.clone()));
        assert_eq!(scope, [row.clone()]);
        let mut used = Vec::new();
        pattern.lookup_used_variables(&mut |v| used.push(v.clone()));
        assert_eq!(used, [key, row]);
    }

    #[test]
    fn source_order_display_and_sse_retain_raw_conditions() {
        let literal = ParsedSourceLiteral {
            occurrence: None,
            rdf: Literal::new_language_tagged_literal("a", "EN").unwrap(),
            datatype: SourceDatatype::Absent,
            language_original: Some("EN".into()),
            origin: SourceLiteralOrigin::QueryLanguageExistingLiteral,
        };
        let term = Expression::SourceLiteral(literal);
        let pattern = GraphPattern::SourceOrderBy {
            inner: Box::new(GraphPattern::Values {
                variables: vec![Variable::new_unchecked("x")],
                bindings: vec![],
            }),
            expression: vec![
                OrderExpression::Asc(term.clone()),
                OrderExpression::Desc(term),
            ],
        };
        let text = pattern.to_string();
        assert!(text.contains("ORDER BY"));
        assert!(text.contains("ASC(\"a\"@EN)"));
        assert!(text.contains("DESC(\"a\"@EN)"));
        let mut sse = String::new();
        pattern.fmt_sse(&mut sse).unwrap();
        assert!(sse.starts_with("(source-order ((asc "));
        assert!(sse.contains("(desc "));
        assert_eq!(sse.matches("@EN").count(), 2);
    }
}

#[cfg(test)]
mod source_order_hash_tests {
    use super::*;
    use std::hash::{Hash, Hasher};

    #[test]
    fn source_order_append_preserves_prior_graph_hash_bytes() {
        #[derive(Default)]
        struct RecordingHasher(Vec<u8>);
        impl Hasher for RecordingHasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        macro_rules! check {
            ($tag:expr, $pattern:expr, $($field:expr),+ $(,)?) => {{
                let mut actual = RecordingHasher::default();
                $pattern.hash(&mut actual);
                let mut previous = RecordingHasher::default();
                ($tag + isize::from(cfg!(feature = "sep-0006"))).hash(&mut previous);
                $($field.hash(&mut previous);)+
                assert_eq!(actual.0, previous.0);
            }};
        }
        let inner = Box::new(GraphPattern::default());
        let variables = vec![Variable::new_unchecked("row")];
        let expression = vec![OrderExpression::Desc(Expression::Variable(
            variables[0].clone(),
        ))];
        let aggregates = vec![(
            Variable::new_unchecked("count"),
            AggregateExpression::CountSolutions { distinct: false },
        )];
        let grouping = SourceGrouping::<Expression>::Implicit;
        let name = NamedNodePattern::NamedNode(NamedNode::new_unchecked("urn:service"));
        let bindings: Vec<Vec<Option<SourceValueTerm>>> = vec![vec![None]];
        check!(
            10_isize,
            GraphPattern::OrderBy {
                inner: inner.clone(),
                expression: expression.clone()
            },
            inner,
            expression
        );
        check!(
            15_isize,
            GraphPattern::Group {
                inner: inner.clone(),
                variables: variables.clone(),
                aggregates: aggregates.clone()
            },
            inner,
            variables,
            aggregates
        );
        check!(
            16_isize,
            GraphPattern::Service {
                name: name.clone(),
                inner: inner.clone(),
                silent: false
            },
            name,
            inner,
            false
        );
        check!(
            17_isize,
            GraphPattern::SourceGroup {
                inner: inner.clone(),
                grouping: grouping.clone(),
                aggregates: aggregates.clone()
            },
            inner,
            grouping,
            aggregates
        );
        check!(
            18_isize,
            GraphPattern::SourceValues {
                variables: variables.clone(),
                bindings: bindings.clone()
            },
            variables,
            bindings
        );
    }
}

#[cfg(test)]
mod source_context_hash_tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    #[test]
    fn source_context_append_preserves_source_order_hash_and_output_domain() {
        #[derive(Default)]
        struct RecordingHasher(Vec<u8>);
        impl Hasher for RecordingHasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }
        let inner = Box::new(GraphPattern::default());
        let expression = vec![OrderExpression::Asc(Expression::Variable(
            Variable::new_unchecked("x"),
        ))];
        let pattern = GraphPattern::SourceOrderBy {
            inner: inner.clone(),
            expression: expression.clone(),
        };
        let mut actual = RecordingHasher::default();
        pattern.hash(&mut actual);
        let mut expected = RecordingHasher::default();
        (19_isize + isize::from(cfg!(feature = "sep-0006"))).hash(&mut expected);
        inner.hash(&mut expected);
        expression.hash(&mut expected);
        assert_eq!(actual.0, expected.0);
        let mut actual = DefaultHasher::new();
        pattern.hash(&mut actual);
        let mut expected = DefaultHasher::new();
        (19_isize + isize::from(cfg!(feature = "sep-0006"))).hash(&mut expected);
        inner.hash(&mut expected);
        expression.hash(&mut expected);
        assert_eq!(actual.finish(), expected.finish());
        let annotated = GraphPattern::SourceContext {
            inner: Box::new(GraphPattern::Project {
                inner,
                variables: vec![Variable::new_unchecked("visible")],
            }),
            annotation: SourceContextAnnotation::Filter {
                scope: SourceVariableScope {
                    variables: vec![Variable::new_unchecked("scope_only")],
                },
            },
        };
        // A walker cannot promote annotation members to solution columns.
        let mut output = Vec::new();
        annotated.on_in_scope_variable(|variable| output.push(variable.clone()));
        assert_eq!(output, vec![Variable::new_unchecked("visible")]);
    }
}
