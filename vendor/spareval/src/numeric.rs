//! Private RDF numeric values, constructor policies, and arithmetic consumers.
//! This module evaluates already-computed operands, never an expression AST.
use crate::QueryEvaluationError;
use crate::tuple::{
    ErrorValue, SolutionValue, SourceException, SourceExceptionArgument, SourceExceptionClass,
    SparqlErrorKind,
};
use oxrdf::{Literal, NamedNode, Term};
use semantica_numeric::{
    BigInt, Binary64BinaryOp, Binary64Fault, Binary64UnaryOp, DecimalContext, DecimalOp,
    DecimalSignal, DecimalSignals, IntegerOp, NumericComparisonResult, NumericFault, NumericLimits,
    NumericOutcome, NumericValue, apply_binary64_binary, apply_binary64_unary, apply_integer,
    binary64_from_decimal, binary64_from_integer, decimal_from_integer, format_decimal_fixed,
    format_integer, parse_integer,
};
use spargebra::{NumericTokenKind, NumericTokenSign};
use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
mod aggregate;
mod prepared_literal;
pub(crate) use prepared_literal::{PreparedSourceLiteralRegistry, resolve_source_literal};
pub(crate) mod source_bgp_order;
mod source_boolean;
mod source_context;
mod source_datatype;
mod source_datetime;
mod source_duration;
mod source_ebv;
pub(crate) mod source_exception;
pub(crate) mod source_function;
mod source_literal;
#[cfg(test)]
mod source_literal_truth_receiving;
mod source_order;
pub(crate) mod source_registry;
mod source_relational;
pub(crate) use source_relational::SourceRelationalOrder;
mod source_text;
mod source_variable;
#[cfg(test)]
mod source_variable_receiving;
pub(crate) use aggregate::NumericAggregateState;
pub(crate) use source_context::{
    QueryContextSemantics, SourceContextFault, SourceHashPolicy, SourceHashRuntime,
};
pub(crate) use source_literal::{
    LiteralSourceState, QueryLiteralSemantics, SourceFact, SourceLiteralCacheKind,
    SourceLiteralMetadata, SourceTermKey, source_context_equal, source_identity_key,
};
pub(crate) use source_order::{SourceExtremumKind, SourceOrderKey};
pub(crate) use spargebra::{ParsedSourceLiteral, SourceDatatype, SourceLiteralOrigin};

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NumericKind {
    Integer,
    Decimal,
    Float,
    Double,
}

fn numeric_kind(datatype: &NamedNode) -> Option<NumericKind> {
    Some(match datatype.as_str().strip_prefix(XSD)? {
        "integer" | "nonPositiveInteger" | "negativeInteger" | "nonNegativeInteger"
        | "positiveInteger" | "unsignedLong" | "unsignedInt" | "unsignedShort" | "unsignedByte"
        | "long" | "int" | "short" | "byte" => NumericKind::Integer,
        "decimal" => NumericKind::Decimal,
        "float" => NumericKind::Float,
        "double" => NumericKind::Double,
        _ => return None,
    })
}
fn datatype(kind: NumericKind) -> NamedNode {
    let name = match kind {
        NumericKind::Integer => "integer",
        NumericKind::Decimal => "decimal",
        NumericKind::Float => "float",
        NumericKind::Double => "double",
    };
    NamedNode::new_unchecked(format!("{XSD}{name}"))
}
fn value_kind(value: &NumericValue) -> NumericKind {
    match value {
        NumericValue::Integer(_) => NumericKind::Integer,
        NumericValue::Decimal(_) => NumericKind::Decimal,
        NumericValue::Binary64(_) => NumericKind::Double,
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NumericValueOrigin {
    QueryTypedExistingLiteral,
    PreparedQueryLiteral,
    QueryNumericToken,
    DirectString,
    KnownCached,
    Arithmetic,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NumericInputOrigin {
    QueryTypedExistingLiteral,
    // Only the sealed actual-parser entry may grant this origin. The token
    // constructor's integer/Decimal lexical closure is a checked precondition.
    PreparedQueryLiteral,
    DirectString { normalize: bool },
    UnknownNormalized,
}
#[derive(Clone, Debug)]
pub(crate) struct NumericLiteralCache {
    value: NumericValue,
    datatype: NamedNode,
    origin: NumericValueOrigin,
}
impl NumericLiteralCache {
    pub(crate) fn value(&self) -> &NumericValue {
        &self.value
    }
    pub(crate) fn datatype(&self) -> &NamedNode {
        &self.datatype
    }
    pub(crate) fn origin(&self) -> NumericValueOrigin {
        self.origin
    }
}
#[derive(Clone, Debug)]
pub(crate) enum NumericCacheState {
    NotNumeric,
    Known(Arc<NumericLiteralCache>),
    // Literal(error) retains the actual exception object, independently of
    // its RDF/source-identity lexical representation.
    ErrorObject(ErrorValue),
    CastFailed,
    UnknownNormalized,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceBooleanConstructor { Object, DirectString, ExistingLiteral }

#[derive(Clone, Debug, Eq, PartialEq)]
enum RawSourceTerm {
    Rdf(Term),
    Variable(oxrdf::Variable),
}
#[derive(Clone, Debug)]
pub(crate) struct RawExpressionValue {
    source_term: RawSourceTerm,
    numeric: NumericCacheState,
    literal_source: LiteralSourceState,
    source_object: Option<SourceObjectToken>,
    // The stored datatype URI is a separate source object. RDF equality does
    // not imply aliasing; clones transport this identity with the literal.
    source_datatype_object: Option<SourceObjectToken>,
    // A constructor-owned seal, not forgeable from RDF spelling/metadata.
    source_boolean_constructor: SourceBooleanConstructor,
    // Only the actual Literal(existing, datatype=string) constructor sets this.
    source_string_cast_language: bool,
}
// Object identity travels with an evaluated source value, independently of
// its RDF equality, numeric cache, or the wrapper allocated by a codec.
#[derive(Clone)]
struct SourceObjectToken(Arc<()>);
impl fmt::Debug for SourceObjectToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SourceObjectToken")
    }
}
impl PartialEq for RawExpressionValue {
    fn eq(&self, other: &Self) -> bool {
        self.source_term == other.source_term
    }
}
impl Eq for RawExpressionValue {}
impl Hash for RawExpressionValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.source_term {
            // Keep the established RDF byte/hash protocol exactly.
            RawSourceTerm::Rdf(rdf) => rdf.hash(state),
            RawSourceTerm::Variable(variable) => { "source.Variable".hash(state); variable.hash(state); }
        }
    }
}
impl RawExpressionValue {
    pub(crate) fn with_new_source_object(mut self) -> Self {
        self.source_object = Some(SourceObjectToken(Arc::new(())));
        if self.source_datatype_object.is_none()
            && matches!(&self.literal_source, LiteralSourceState::Literal(metadata)
                if matches!(metadata.datatype, SourceDatatype::Specified(_)))
        {
            self.source_datatype_object = Some(SourceObjectToken(Arc::new(())));
        }
        self
    }
    pub(crate) fn ensure_source_object(self) -> Self {
        if self.source_object.is_some() {
            self
        } else {
            self.with_new_source_object()
        }
    }
    pub(crate) fn same_source_object(&self, other: &Self) -> bool {
        match (&self.source_object, &other.source_object) {
            (Some(left), Some(right)) => Arc::ptr_eq(&left.0, &right.0),
            _ => false,
        }
    }
    pub(crate) fn rdf(&self) -> Result<&Term, RawNumericFault> {
        match &self.source_term {
            RawSourceTerm::Rdf(rdf) => Ok(rdf),
            RawSourceTerm::Variable(_) => Err(RawNumericFault::SourceTermNotRdf("Variable")),
        }
    }
    pub(crate) fn source_variable(&self) -> Option<&oxrdf::Variable> {
        match &self.source_term { RawSourceTerm::Variable(v) => Some(v), _ => None }
    }
    pub(crate) fn literal(&self) -> Option<&Literal> {
        match &self.source_term { RawSourceTerm::Rdf(Term::Literal(v)) => Some(v), _ => None }
    }
    pub(crate) fn source_spelling_len(&self) -> usize {
        match &self.source_term {
            RawSourceTerm::Rdf(v) => v.to_string().len(),
            RawSourceTerm::Variable(v) => v.as_str().len().saturating_add(1),
        }
    }
    pub(crate) fn numeric(&self) -> &NumericCacheState {
        &self.numeric
    }
    pub(crate) fn into_parts(self) -> Result<(Term, NumericCacheState), RawNumericFault> {
        match self.source_term {
            RawSourceTerm::Rdf(rdf) => Ok((rdf, self.numeric)),
            RawSourceTerm::Variable(_) => Err(RawNumericFault::SourceTermNotRdf("Variable")),
        }
    }
    pub(crate) fn from_parts(rdf: Term, numeric: NumericCacheState) -> Self {
        let literal_source = LiteralSourceState::for_import(&rdf);
        Self {
            source_term: RawSourceTerm::Rdf(rdf),
            numeric,
            literal_source,
            source_object: None,
            source_datatype_object: None,
            source_boolean_constructor: SourceBooleanConstructor::Object,
            source_string_cast_language: false,
        }
    }
    /// A plain RDF import has no construction provenance, even when its text
    /// looks like a perfectly valid numeric literal.
    pub(crate) fn from_import(rdf: Term) -> Self {
        let numeric = if matches!(&rdf, Term::Literal(l) if numeric_kind(&l.datatype().into_owned()).is_some())
        {
            NumericCacheState::UnknownNormalized
        } else {
            NumericCacheState::NotNumeric
        };
        Self::from_parts(rdf, numeric)
    }
}
#[derive(Clone, Debug)]
pub(crate) enum NumericInitialValue {
    /// Present source cells, admitted only by the sealed source execution
    /// preparation. They never pass through an RDF constructor or Dataset.
    SourceNone,
    SourceError(ErrorValue),
    /// An already constructed source name; do not run Variable(str) twice.
    SourceVariable(oxrdf::Variable),
    DirectString {
        lexical: String,
        datatype: NamedNode,
        normalize: bool,
    },
    KnownCached {
        rdf: Term,
        value: NumericValue,
    },
    UnknownNormalized(Term),
    SourceDirectString {
        lexical: String,
        datatype: Option<NamedNode>,
        language: Option<String>,
        normalize: bool,
    },
    SourceKnownCached {
        rdf: Term,
        value: NumericValue,
        metadata: SourceLiteralMetadata,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct NumericInitialBinding {
    pub(crate) variable: oxrdf::Variable,
    pub(crate) value: NumericInitialValue,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RawNumericFault {
    UnknownNumericCache,
    LossyNumericBackend,
    Core(NumericFault),
    UnsupportedNumericFamily,
    UnsupportedLiteralFallback,
    InvalidSourceOrigin,
    ParserLiteralType,
    Binary64(Binary64Fault),
    MixedNumericTypeError,
    AggregateDatatypeAccess,
    AggregateTypePromotion,
    UnknownSourceLiteralIdentity,
    SourceMetadataWouldBeLost,
    SourceTermNotRdf(&'static str),
    UnsupportedSourceLiteralOperator(&'static str),
    SourceLiteralLanguageValueError,
    UnknownSourceErrorText,
    SourceComparisonTypeError,
    SourceContext(SourceContextFault),
    SourceParserLiteralType { argument: String },
    SourceFunctionException(source_registry::SourceFunctionException),
    SourceLanguageException(source_exception::SourceLanguageException),
}
impl fmt::Display for RawNumericFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNumericCache => {
                f.write_str("numeric cache construction provenance is unknown")
            }
            Self::LossyNumericBackend => {
                f.write_str("dataset changed numeric RDF identity during internalization")
            }
            Self::Core(error) => write!(f, "{error}"),
            Self::UnsupportedNumericFamily => {
                f.write_str("private query binary64 arithmetic is not yet supported")
            }
            Self::UnsupportedLiteralFallback => {
                f.write_str("original Literal value fallback is not implemented")
            }
            Self::InvalidSourceOrigin => {
                f.write_str("numeric construction origin or representation is invalid")
            }
            Self::ParserLiteralType => {
                f.write_str("original numeric token Literal construction raises TypeError")
            }
            Self::Binary64(error) => write!(f, "{error}"),
            Self::MixedNumericTypeError => {
                f.write_str("original float and Decimal arithmetic raises TypeError")
            }
            Self::AggregateDatatypeAccess => {
                f.write_str("aggregate value has no datatype attribute")
            }
            Self::AggregateTypePromotion => f.write_str("aggregate cannot combine these datatypes"),
            Self::UnknownSourceLiteralIdentity => f.write_str("unknown_source_literal_identity"),
            Self::SourceTermNotRdf(kind) => write!(f, "source {kind} has no native RDF representation"),
            Self::SourceMetadataWouldBeLost => f.write_str("source literal metadata would be lost"),
            Self::UnsupportedSourceLiteralOperator(operation) => {
                write!(f, "source literal operation is not qualified: {operation}")
            }
            Self::SourceLiteralLanguageValueError => {
                f.write_str("original Literal language construction raises ValueError")
            }
            Self::UnknownSourceErrorText => f.write_str("unknown_source_error_text"),
            Self::SourceComparisonTypeError => {
                f.write_str("source rich comparison raises TypeError")
            }
            Self::SourceContext(error) => error.fmt(f),
            Self::SourceParserLiteralType { argument } => f.write_str(argument),
            Self::SourceFunctionException(error) | Self::SourceLanguageException(error) => {
                error.fmt(f)
            }
        }
    }
}
impl RawNumericFault {
    /// Only an originating owner can attach a received language constructor.
    /// No host diagnostic, unknown operator or resource failure is inferred.
    pub(crate) fn source_language_exception(
        &self,
    ) -> Option<&source_exception::SourceLanguageException> {
        match self {
            Self::SourceFunctionException(error) | Self::SourceLanguageException(error) => {
                Some(error)
            }
            _ => None,
        }
    }
}
impl std::error::Error for RawNumericFault {}
impl From<NumericFault> for RawNumericFault {
    fn from(error: NumericFault) -> Self {
        Self::Core(error)
    }
}
impl From<Binary64Fault> for RawNumericFault {
    fn from(error: Binary64Fault) -> Self {
        match error {
            // Resource/representation failures keep the existing fatal domain.
            Binary64Fault::Core(error) => Self::Core(error),
            error => Self::Binary64(error),
        }
    }
}
pub(crate) fn raw_numeric_fault(error: &QueryEvaluationError) -> Option<&RawNumericFault> {
    if let QueryEvaluationError::Unexpected(error) = error {
        error.downcast_ref()
    } else {
        None
    }
}
#[derive(Debug)]
pub(crate) enum NumericEvaluationError {
    Sparql {
        kind: SparqlErrorKind,
        diagnostic: Cow<'static, str>,
        source: Option<SourceException>,
    },
    Fatal(RawNumericFault),
}
impl NumericEvaluationError {
    /// One conversion preserves source constructor facts at both expression
    /// and aggregate boundaries. Each conversion allocates a new error object;
    /// subsequent clones retain that object's identity.
    pub(crate) fn into_error_value(self) -> Result<ErrorValue, RawNumericFault> {
        match self {
            Self::Sparql {
                kind,
                diagnostic,
                source,
            } => Ok(match source {
                Some(source) => ErrorValue::with_source(kind, &diagnostic, source),
                None => ErrorValue::with_kind(kind, &diagnostic),
            }),
            Self::Fatal(error) => Err(error),
        }
    }
}
impl From<RawNumericFault> for NumericEvaluationError {
    fn from(error: RawNumericFault) -> Self {
        Self::Fatal(error)
    }
}
fn type_error(diagnostic: &'static str) -> NumericEvaluationError {
    NumericEvaluationError::Sparql {
        kind: SparqlErrorKind::Type,
        diagnostic: Cow::Borrowed(diagnostic),
        source: None,
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NumericUnaryOp {
    Plus,
    Minus,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NumericBinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NumericChainKind {
    Additive,
    Multiplicative,
}

/// Arithmetic permission is immutable for a prepared query's shared session.
/// Both capabilities retain existing float construction, transport and EBV.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QueryNumericCapabilities {
    IntegerDecimalV1,
    MixedBinary64V1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QueryGroupSemantics {
    LegacyPrivateV1,
    SourceAggregateV1,
}

#[derive(Debug)]
pub(crate) struct QueryNumericSession {
    decimal: DecimalContext,
    limits: NumericLimits,
    integer_text_limit: Option<usize>,
    normalize_literals: bool,
    capabilities: QueryNumericCapabilities,
    group_semantics: QueryGroupSemantics,
    literal_semantics: QueryLiteralSemantics,
    context_semantics: QueryContextSemantics,
    source_hash: Option<Rc<SourceHashRuntime>>,
    source_functions: Option<Rc<source_registry::SourceFunctionRegistry>>,
    // Explicit additional private construction capability. Existing factories
    // retain their received refusal until their caller opts in.
    source_time_values_v1: bool,
    source_time_comparisons_v1: bool,
    source_calendar_duration_v1: bool,
    // Namespace lookups are shared within the prepared source session, unlike
    // independently constructed typed-literal URI objects. This is not an
    // interning table for arbitrary equal IRIs or a process-global identity.
    source_string_datatype_object: SourceObjectToken,
    source_language_datatype_object: SourceObjectToken,
    flags: Cell<DecimalSignals>,
}
/// Immutable namespace-object aliases transported with an owned source store.
/// No caller strings can manufacture these tokens, and no evaluator callbacks
/// or query-local flags cross the owning thread boundary.
#[derive(Clone, Debug)]
pub(crate) struct SourceDatatypeObjects {
    string: SourceObjectToken,
    language: SourceObjectToken,
}
impl Default for SourceDatatypeObjects {
    fn default() -> Self {
        Self { string: SourceObjectToken(Arc::new(())), language: SourceObjectToken(Arc::new(())) }
    }
}
impl QueryNumericSession {
    pub(crate) fn with_source_datatype_objects(mut self, objects: &SourceDatatypeObjects) -> Self {
        self.source_string_datatype_object = objects.string.clone();
        self.source_language_datatype_object = objects.language.clone();
        self
    }

    pub(crate) fn new(
        decimal: DecimalContext,
        limits: NumericLimits,
        integer_text_limit: Option<usize>,
        normalize_literals: bool,
    ) -> Result<Self, RawNumericFault> {
        Self::new_with_capabilities(
            decimal,
            limits,
            integer_text_limit,
            normalize_literals,
            QueryNumericCapabilities::IntegerDecimalV1,
        )
    }
    pub(crate) fn new_with_capabilities(
        decimal: DecimalContext,
        limits: NumericLimits,
        integer_text_limit: Option<usize>,
        normalize_literals: bool,
        capabilities: QueryNumericCapabilities,
    ) -> Result<Self, RawNumericFault> {
        Self::new_with_group_semantics(
            decimal,
            limits,
            integer_text_limit,
            normalize_literals,
            capabilities,
            QueryGroupSemantics::LegacyPrivateV1,
        )
    }
    pub(crate) fn new_with_group_semantics(
        decimal: DecimalContext,
        limits: NumericLimits,
        integer_text_limit: Option<usize>,
        normalize_literals: bool,
        capabilities: QueryNumericCapabilities,
        group_semantics: QueryGroupSemantics,
    ) -> Result<Self, RawNumericFault> {
        Self::new_with_literal_semantics(
            decimal,
            limits,
            integer_text_limit,
            normalize_literals,
            capabilities,
            group_semantics,
            QueryLiteralSemantics::RdfIdentityV1,
        )
    }
    pub(crate) fn new_with_literal_semantics(
        decimal: DecimalContext,
        limits: NumericLimits,
        integer_text_limit: Option<usize>,
        normalize_literals: bool,
        capabilities: QueryNumericCapabilities,
        group_semantics: QueryGroupSemantics,
        literal_semantics: QueryLiteralSemantics,
    ) -> Result<Self, RawNumericFault> {
        Self::new_with_context_semantics(
            decimal,
            limits,
            integer_text_limit,
            normalize_literals,
            capabilities,
            group_semantics,
            literal_semantics,
            QueryContextSemantics::LegacyPrivateV1,
            None,
        )
    }
    pub(crate) fn new_with_context_semantics(
        decimal: DecimalContext,
        limits: NumericLimits,
        integer_text_limit: Option<usize>,
        normalize_literals: bool,
        capabilities: QueryNumericCapabilities,
        group_semantics: QueryGroupSemantics,
        literal_semantics: QueryLiteralSemantics,
        context_semantics: QueryContextSemantics,
        source_hash: Option<Rc<SourceHashRuntime>>,
    ) -> Result<Self, RawNumericFault> {
        if (context_semantics == QueryContextSemantics::SourceContextV1
            && literal_semantics != QueryLiteralSemantics::SourceLiteralV1)
            || (context_semantics == QueryContextSemantics::SourceContextV1)
                != source_hash.is_some()
        {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        // Valid exact construction checks the context and configured budgets
        // without arithmetic rounding or any constructor signal.
        decimal.parse("0", &limits)?;
        Ok(Self {
            decimal,
            limits,
            integer_text_limit,
            normalize_literals,
            capabilities,
            group_semantics,
            literal_semantics,
            context_semantics,
            source_hash,
            source_functions: None,
            source_time_values_v1: false,
            source_time_comparisons_v1: false,
            source_calendar_duration_v1: false,
            source_string_datatype_object: SourceObjectToken(Arc::new(())),
            source_language_datatype_object: SourceObjectToken(Arc::new(())),
            flags: Cell::new(DecimalSignals::EMPTY),
        })
    }
    pub(crate) fn capabilities(&self) -> QueryNumericCapabilities {
        self.capabilities
    }
    pub(crate) fn group_semantics(&self) -> QueryGroupSemantics {
        self.group_semantics
    }
    pub(crate) fn literal_semantics(&self) -> QueryLiteralSemantics {
        self.literal_semantics
    }
    pub(crate) fn context_semantics(&self) -> QueryContextSemantics {
        self.context_semantics
    }
    pub(crate) fn source_hash(&self) -> Option<&Rc<SourceHashRuntime>> {
        self.source_hash.as_ref()
    }
    pub(crate) fn flags(&self) -> DecimalSignals {
        self.flags.get()
    }
    pub(crate) fn decimal_context(&self) -> &DecimalContext {
        &self.decimal
    }
    pub(crate) fn limits(&self) -> &NumericLimits {
        &self.limits
    }
    fn observe_comparison(
        &self,
        comparison: NumericComparisonResult,
    ) -> Result<bool, RawNumericFault> {
        // A reached FloatOperation survives even if exact conversion later
        // refuses a resource budget, as well as on an ordinary decimal trap.
        self.flags.set(self.flags.get() | comparison.signals);
        comparison.result.map_err(RawNumericFault::Core)
    }
    fn observe<T>(
        &self,
        result: Result<NumericOutcome<T>, NumericFault>,
    ) -> Result<T, NumericFault> {
        match result {
            Ok(outcome) => {
                self.flags.set(self.flags.get() | outcome.signals);
                Ok(outcome.value)
            }
            Err(error) => {
                if let NumericFault::DecimalTrap { signals, .. } = &error {
                    self.flags.set(self.flags.get() | *signals);
                }
                Err(error)
            }
        }
    }
    /// Translate only a reached Decimal trap at a qualified source arithmetic
    /// owner. The numerical core and legacy profiles keep their own fault.
    /// All flags were recorded by observe; exception args contain only traps.
    fn source_arithmetic_fault(&self, error: NumericFault) -> RawNumericFault {
        if self.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1 {
            if let NumericFault::DecimalTrap { primary, signals } = &error {
                use source_exception::{
                    LanguageExceptionArgument, SourceLanguageException, SourceLanguageExceptionKind,
                };
                return RawNumericFault::SourceLanguageException(SourceLanguageException {
                    kind: SourceLanguageExceptionKind::Decimal(*primary),
                    arguments: vec![LanguageExceptionArgument::DecimalSignalClasses(
                        signals
                            .iter()
                            .filter(|s| self.decimal.traps.contains(*s))
                            .collect(),
                    )],
                });
            }
        }
        RawNumericFault::Core(error)
    }
    fn decimal_from_float_observed(
        &self,
        value: f64,
    ) -> Result<semantica_numeric::DecimalValue, RawNumericFault> {
        // Reached constructor signal survives even a later resource refusal.
        self.flags
            .set(self.flags.get() | DecimalSignals::from(DecimalSignal::FloatOperation));
        self.observe(self.decimal.from_binary64(value, &self.limits))
            .map_err(|error| self.source_arithmetic_fault(error))
    }
    fn check_text(&self, text: &str) -> Result<(), RawNumericFault> {
        if text.chars().count() > self.limits.max_input_characters {
            Err(NumericFault::ResourceLimit {
                operation: "input_characters",
            }
            .into())
        } else {
            Ok(())
        }
    }
    fn parse_value(
        &self,
        text: &str,
        kind: NumericKind,
    ) -> Result<Option<NumericValue>, RawNumericFault> {
        self.check_text(text)?;
        match kind {
            NumericKind::Integer => {
                match parse_integer(text, &self.limits, self.integer_text_limit) {
                    Ok(value) => Ok(Some(NumericValue::Integer(value))),
                    // term._castLexicalToPython catches the Python conversion error.
                    Err(NumericFault::InvalidLexical | NumericFault::IntegerTextLimit) => Ok(None),
                    Err(error) => Err(error.into()),
                }
            }
            NumericKind::Decimal => match self.observe(self.decimal.parse(text, &self.limits)) {
                Ok(value) => Ok(Some(NumericValue::Decimal(value))),
                Err(NumericFault::DecimalTrap {
                    primary: DecimalSignal::InvalidOperation,
                    ..
                })
                | Err(NumericFault::InvalidLexical) => Ok(None),
                Err(error) => Err(error.into()),
            },
            NumericKind::Float | NumericKind::Double => {
                // Reuse the fixed Python float lexical owner. This is leaf
                // construction, not permission to use mixed float arithmetic.
                Ok(
                    semantica_json::python_builtin_float(&serde_json::Value::String(
                        text.to_owned(),
                    ))
                    .ok()
                    .map(NumericValue::Binary64),
                )
            }
        }
    }
    fn format_value(&self, value: &NumericValue) -> Result<String, RawNumericFault> {
        let text = match value {
            NumericValue::Integer(value) => {
                format_integer(value, &self.limits, self.integer_text_limit)?
            }
            NumericValue::Decimal(value) => format_decimal_fixed(value, &self.limits)?,
            NumericValue::Binary64(value) => semantica_json::python_float_repr(*value)
                .map_err(|_| RawNumericFault::InvalidSourceOrigin)?,
        };
        if text.chars().count() > self.limits.max_output_characters {
            return Err(NumericFault::ResourceLimit {
                operation: "output_characters",
            }
            .into());
        }
        Ok(text)
    }
    pub(crate) fn construct_literal(
        &self,
        literal: &Literal,
        origin: NumericInputOrigin,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        if origin == NumericInputOrigin::UnknownNormalized {
            return Ok(RawExpressionValue::from_import(literal.clone().into()));
        }
        let dt = literal.datatype().into_owned();
        // Source term.Literal.__new__ (term.py:710-715) normalizes these two
        // datatypes after every constructor branch, including an existing
        // Literal and normalize=False. They are outside this numeric owner;
        // preserving their input bytes would claim a wrong RDF identity.
        if dt
            .as_str()
            .strip_prefix(XSD)
            .is_some_and(|name| matches!(name, "normalizedString" | "token"))
        {
            return Err(RawNumericFault::UnsupportedLiteralFallback);
        }
        let Some(kind) = numeric_kind(&dt) else {
            if let NumericInputOrigin::DirectString { normalize: true } = origin {
                if dt.as_str() == format!("{XSD}boolean") {
                    let value =
                        matches!(literal.value().to_ascii_lowercase().as_str(), "1" | "true");
                    return Ok(RawExpressionValue::from_parts(
                        Literal::new_typed_literal(if value { "true" } else { "false" }, dt).into(),
                        NumericCacheState::NotNumeric,
                    ));
                }
                if dt
                    .as_str()
                    .strip_prefix(XSD)
                    .is_some_and(|name| !matches!(name, "string" | "anyURI"))
                {
                    return Err(RawNumericFault::UnsupportedLiteralFallback);
                }
            }
            return Ok(RawExpressionValue::from_parts(
                literal.clone().into(),
                NumericCacheState::NotNumeric,
            ));
        };
        let Some(value) = self.parse_value(literal.value(), kind)? else {
            return Ok(RawExpressionValue::from_parts(
                literal.clone().into(),
                NumericCacheState::CastFailed,
            ));
        };
        let (normalize, cache_origin) = match origin {
            NumericInputOrigin::QueryTypedExistingLiteral => {
                (false, NumericValueOrigin::QueryTypedExistingLiteral)
            }
            NumericInputOrigin::PreparedQueryLiteral => {
                (false, NumericValueOrigin::PreparedQueryLiteral)
            }
            NumericInputOrigin::DirectString { normalize } => {
                (normalize, NumericValueOrigin::DirectString)
            }
            NumericInputOrigin::UnknownNormalized => {
                return Err(RawNumericFault::InvalidSourceOrigin);
            }
        };
        let rdf = if normalize {
            Literal::new_typed_literal(self.format_value(&value)?, dt.clone())
        } else {
            literal.clone()
        };
        Ok(RawExpressionValue::from_parts(
            rdf.into(),
            NumericCacheState::Known(Arc::new(NumericLiteralCache {
                value,
                datatype: dt,
                origin: cache_origin,
            })),
        ))
    }
    pub(crate) fn known_cached(
        &self,
        rdf: Term,
        value: NumericValue,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        let Term::Literal(literal) = &rdf else {
            return Err(RawNumericFault::InvalidSourceOrigin);
        };
        self.check_text(literal.value())?;
        let dt = literal.datatype().into_owned();
        if numeric_kind(&dt).is_none() {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        // Explicit trusted source object: never reconstruct its value from the
        // RDF lexical string. This factory does not establish that provenance.
        // term.py:695-705 retains an object's value even when an explicitly
        // supplied numeric datatype has a different usual Python value type.
        Ok(RawExpressionValue::from_parts(
            rdf,
            NumericCacheState::Known(Arc::new(NumericLiteralCache {
                value,
                datatype: dt,
                origin: NumericValueOrigin::KnownCached,
            })),
        ))
    }
    pub(crate) fn construct_result(
        &self,
        value: NumericValue,
        specified_datatype: Option<NamedNode>,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        let dt = specified_datatype.unwrap_or_else(|| datatype(value_kind(&value)));
        if numeric_kind(&dt).is_none() {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let rdf = Literal::new_typed_literal(self.format_value(&value)?, dt.clone());
        let raw = RawExpressionValue::from_parts(
            rdf.into(),
            NumericCacheState::Known(Arc::new(NumericLiteralCache {
                value,
                datatype: dt.clone(),
                origin: NumericValueOrigin::Arithmetic,
            })),
        );
        if self.literal_semantics == QueryLiteralSemantics::SourceLiteralV1 {
            raw.with_source_metadata(SourceLiteralMetadata {
                datatype: SourceDatatype::Specified(dt),
                language_original: SourceFact::Known(None),
                ill_typed: SourceFact::Known(None),
                cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
            })
        } else {
            Ok(raw)
        }
    }
    pub(crate) fn construct_initial(
        &self,
        input: NumericInitialValue,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        match input {
            NumericInitialValue::SourceNone | NumericInitialValue::SourceError(_) => {
                Err(RawNumericFault::InvalidSourceOrigin)
            }
            NumericInitialValue::DirectString {
                lexical,
                datatype,
                normalize,
            } => self.construct_literal(
                &Literal::new_typed_literal(lexical, datatype),
                NumericInputOrigin::DirectString { normalize },
            ),
            NumericInitialValue::KnownCached { rdf, value } => self.known_cached(rdf, value),
            NumericInitialValue::SourceVariable(name) => self.source_variable_from_name(name),
            NumericInitialValue::UnknownNormalized(rdf) => Ok(RawExpressionValue::from_import(rdf)),
            NumericInitialValue::SourceDirectString {
                lexical,
                datatype,
                language,
                normalize,
            } => self.construct_source_direct_string(&lexical, datatype, language, normalize),
            NumericInitialValue::SourceKnownCached {
                rdf,
                value,
                metadata,
            } => self.known_source_cached(rdf, value, metadata),
        }
    }
    pub(crate) fn construct_numeric_token(
        &self,
        kind: NumericTokenKind,
        sign: NumericTokenSign,
        magnitude: &str,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.construct_numeric_token_with_source_error(kind, sign, magnitude, false)
    }
    fn construct_numeric_token_with_source_error(
        &self,
        kind: NumericTokenKind,
        sign: NumericTokenSign,
        magnitude: &str,
        source_error: bool,
    ) -> Result<RawExpressionValue, RawNumericFault> {
        self.check_text(magnitude)?;
        if !valid_magnitude(kind, magnitude) {
            return Err(RawNumericFault::InvalidSourceOrigin);
        }
        let dt = datatype(match kind {
            NumericTokenKind::Integer => NumericKind::Integer,
            NumericTokenKind::Decimal => NumericKind::Decimal,
            NumericTokenKind::Double => NumericKind::Double,
        });
        let mut inner = self.construct_literal(
            &Literal::new_typed_literal(magnitude, dt.clone()),
            NumericInputOrigin::DirectString {
                normalize: self.normalize_literals,
            },
        )?;
        match (sign, kind) {
            (NumericTokenSign::Negative, NumericTokenKind::Decimal) => {
                if source_error {
                    // Preserve the actual already constructed operand in the
                    // Literal.__neg__ TypeError. No repeated constructor,
                    // reparsed cache, extra decimal flag or frozen answer.
                    let inner = inner.with_source_metadata(SourceLiteralMetadata {
                        datatype: SourceDatatype::Specified(dt),
                        language_original: SourceFact::Known(None),
                        ill_typed: SourceFact::Known(Some(false)),
                        cached_value_kind: SourceFact::Known(SourceLiteralCacheKind::Numeric),
                    })?;
                    let repr = self.source_repr(&SolutionValue::Rdf(inner))?;
                    return Err(RawNumericFault::SourceParserLiteralType {
                        argument: format!("Not a number; {repr}"),
                    });
                }
                return Err(RawNumericFault::ParserLiteralType);
            }
            (NumericTokenSign::Positive, NumericTokenKind::Integer) => {
                let Term::Literal(literal) = inner.rdf()? else {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                };
                inner = self.construct_literal(
                    &Literal::new_typed_literal(format!("+{}", literal.value()), dt),
                    NumericInputOrigin::DirectString {
                        normalize: self.normalize_literals,
                    },
                )?;
            }
            (NumericTokenSign::Negative, _) => {
                let NumericCacheState::Known(cache) = inner.numeric() else {
                    return Err(RawNumericFault::ParserLiteralType);
                };
                let value = match cache.value() {
                    NumericValue::Integer(value) => NumericValue::Integer(apply_integer(
                        IntegerOp::Minus,
                        value,
                        None,
                        &self.limits,
                    )?),
                    NumericValue::Binary64(value) => NumericValue::Binary64(-*value),
                    NumericValue::Decimal(_) => return Err(RawNumericFault::ParserLiteralType),
                };
                let negated = self.construct_result(value, None)?;
                let Term::Literal(literal) = negated.rdf()? else {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                };
                inner = self.construct_literal(
                    &Literal::new_typed_literal(literal.value(), dt),
                    NumericInputOrigin::QueryTypedExistingLiteral,
                )?;
            }
            _ => (),
        }
        if let NumericCacheState::Known(cache) = &mut inner.numeric {
            Arc::make_mut(cache).origin = NumericValueOrigin::QueryNumericToken;
        }
        Ok(inner)
    }
    fn require_numeric(
        &self,
        value: &SolutionValue<RawExpressionValue>,
    ) -> Result<(NumericValue, NamedNode), NumericEvaluationError> {
        let SolutionValue::Rdf(raw) = value else {
            return Err(self.numeric_type_error(value, false));
        };
        let Some(literal) = raw.literal() else {
            return Err(self.numeric_type_error(value, false));
        };
        let dt = literal.datatype().into_owned();
        if numeric_kind(&dt).is_none() {
            return Err(self.numeric_type_error(value, true));
        }
        match raw.numeric() {
            NumericCacheState::Known(cache) if cache.datatype() == &dt => {
                Ok((cache.value().clone(), dt))
            }
            NumericCacheState::Known(_)
            | NumericCacheState::NotNumeric
            | NumericCacheState::ErrorObject(_) => Err(RawNumericFault::InvalidSourceOrigin.into()),
            NumericCacheState::UnknownNormalized => {
                Err(RawNumericFault::UnknownNumericCache.into())
            }
            NumericCacheState::CastFailed => {
                Err(RawNumericFault::UnsupportedLiteralFallback.into())
            }
        }
    }
    pub(crate) fn typed_ebv(
        &self,
        raw: &RawExpressionValue,
    ) -> Result<bool, NumericEvaluationError> {
        let Some(literal) = raw.literal() else {
            return Err(type_error("Only literals have Boolean values"));
        };
        let dt = literal.datatype();
        if dt.as_str() == format!("{XSD}boolean") {
            return Ok(matches!(
                literal.value().to_ascii_lowercase().as_str(),
                "1" | "true"
            ));
        }
        if dt.as_str() == format!("{XSD}string") || literal.language().is_some() {
            return Ok(!literal.value().is_empty());
        }
        if numeric_kind(&dt.into_owned()).is_some() {
            match raw.numeric() {
                NumericCacheState::Known(cache) => {
                    let dtype = literal.datatype().into_owned();
                    if cache.datatype() != &dtype {
                        return Err(RawNumericFault::InvalidSourceOrigin.into());
                    }
                    return Ok(match cache.value() {
                        NumericValue::Integer(value) => value != &BigInt::from(0),
                        NumericValue::Decimal(value) => value.python_truth(),
                        NumericValue::Binary64(value) => *value != 0.0,
                    });
                }
                NumericCacheState::CastFailed => {
                    return Err(type_error(
                        "Could not determine literal effective Boolean value",
                    ));
                }
                NumericCacheState::UnknownNormalized => {
                    return Err(RawNumericFault::UnknownNumericCache.into());
                }
                NumericCacheState::NotNumeric | NumericCacheState::ErrorObject(_) => {
                    return Err(RawNumericFault::InvalidSourceOrigin.into());
                }
            }
        }
        Err(RawNumericFault::UnsupportedLiteralFallback.into())
    }
    pub(crate) fn evaluate_unary(
        &self,
        op: NumericUnaryOp,
        value: SolutionValue<RawExpressionValue>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        let (value, _) = self.require_numeric(&value)?;
        let value = match value {
            NumericValue::Integer(value) => NumericValue::Integer(
                apply_integer(
                    if op == NumericUnaryOp::Plus {
                        IntegerOp::Plus
                    } else {
                        IntegerOp::Minus
                    },
                    &value,
                    None,
                    &self.limits,
                )
                .map_err(RawNumericFault::from)?,
            ),
            NumericValue::Decimal(value) => NumericValue::Decimal(
                self.observe(self.decimal.apply(
                    if op == NumericUnaryOp::Plus {
                        DecimalOp::Plus
                    } else {
                        DecimalOp::Minus
                    },
                    &value,
                    None,
                    &self.limits,
                ))
                .map_err(|error| self.source_arithmetic_fault(error))?,
            ),
            NumericValue::Binary64(value) => {
                if self.capabilities() == QueryNumericCapabilities::IntegerDecimalV1 {
                    return Err(RawNumericFault::UnsupportedNumericFamily.into());
                }
                NumericValue::Binary64(apply_binary64_unary(
                    if op == NumericUnaryOp::Plus {
                        Binary64UnaryOp::Plus
                    } else {
                        Binary64UnaryOp::Minus
                    },
                    value,
                ))
            }
        };
        self.construct_result(value, None).map_err(Into::into)
    }
    pub(crate) fn evaluate_conjunction(
        &self,
        values: Vec<SolutionValue<RawExpressionValue>>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        // The compiler has already evaluated the complete source operand list.
        // operators.py:928-941 only short-circuits the subsequent EBV loop.
        let mut result = true;
        for value in values {
            let SolutionValue::Rdf(value) = value else {
                return Err(type_error("Only literals have Boolean values"));
            };
            if !self.typed_ebv(&value)? {
                result = false;
                break;
            }
        }
        Ok(RawExpressionValue::from_import(
            Literal::from(result).into(),
        ))
    }
    pub(crate) fn evaluate_chain(
        &self,
        kind: NumericChainKind,
        first: SolutionValue<RawExpressionValue>,
        tail: Vec<(NumericBinaryOp, SolutionValue<RawExpressionValue>)>,
    ) -> Result<RawExpressionValue, NumericEvaluationError> {
        if tail.is_empty()
            || tail.iter().any(|(op, _)| {
                !matches!(
                    (kind, op),
                    (
                        NumericChainKind::Additive,
                        NumericBinaryOp::Add | NumericBinaryOp::Subtract
                    ) | (
                        NumericChainKind::Multiplicative,
                        NumericBinaryOp::Multiply | NumericBinaryOp::Divide
                    )
                )
            })
        {
            return Err(RawNumericFault::InvalidSourceOrigin.into());
        }
        let (mut value, mut dt) = self.require_numeric(&first)?;
        if kind == NumericChainKind::Multiplicative {
            value = match value {
                NumericValue::Integer(value) => NumericValue::Decimal(
                    decimal_from_integer(&value, &self.limits).map_err(RawNumericFault::from)?,
                ),
                NumericValue::Decimal(value) => NumericValue::Decimal(value),
                NumericValue::Binary64(value) => {
                    // This constructor is inside the source try, whose catch
                    // does not include FloatOperation. The old capability also
                    // retains its constructor signal before arithmetic refusal.
                    // Context and limits were validated when this immutable
                    // session was constructed. Record the reached signal here
                    // even if exact conversion later fails its resource budget.
                    let value = self.decimal_from_float_observed(value)?;
                    if self.capabilities() == QueryNumericCapabilities::IntegerDecimalV1 {
                        return Err(RawNumericFault::UnsupportedNumericFamily.into());
                    }
                    NumericValue::Decimal(value)
                }
            };
        }
        for (op, rhs) in tail {
            let (mut rhs, rhs_dt) = self.require_numeric(&rhs)?;
            if self.capabilities() == QueryNumericCapabilities::IntegerDecimalV1
                && (matches!(&value, NumericValue::Binary64(_))
                    || matches!(&rhs, NumericValue::Binary64(_)))
            {
                return Err(RawNumericFault::UnsupportedNumericFamily.into());
            }
            if kind == NumericChainKind::Additive {
                // operators.py:834-839 converts Decimal/float pairs in these
                // two directions before promoting the independent RDF datatype.
                if matches!(&value, NumericValue::Binary64(_)) {
                    if let NumericValue::Decimal(decimal) = &rhs {
                        rhs = NumericValue::Binary64(
                            binary64_from_decimal(decimal, &self.limits)
                                .map_err(RawNumericFault::from)?,
                        );
                    }
                }
                if matches!(&rhs, NumericValue::Binary64(_)) {
                    if let NumericValue::Decimal(decimal) = &value {
                        value = NumericValue::Binary64(
                            binary64_from_decimal(decimal, &self.limits)
                                .map_err(RawNumericFault::from)?,
                        );
                    }
                }
                dt = promote(&dt, &rhs_dt)?;
            } else if matches!(&rhs, NumericValue::Binary64(_)) {
                // Multiplicative only converts the accumulator when the rhs
                // is float. A later Decimal rhs must not receive this coercion.
                if let NumericValue::Decimal(decimal) = &value {
                    value = NumericValue::Binary64(
                        binary64_from_decimal(decimal, &self.limits)
                            .map_err(RawNumericFault::from)?,
                    );
                }
            }
            value = match (value, rhs) {
                (NumericValue::Binary64(a), NumericValue::Binary64(b)) => {
                    binary64_operation(kind, op, a, b, self.literal_semantics())?
                }
                (NumericValue::Binary64(a), NumericValue::Integer(b)) => {
                    let b =
                        binary64_from_integer(&b, &self.limits).map_err(RawNumericFault::from)?;
                    binary64_operation(kind, op, a, b, self.literal_semantics())?
                }
                (NumericValue::Integer(a), NumericValue::Binary64(b)) => {
                    let a =
                        binary64_from_integer(&a, &self.limits).map_err(RawNumericFault::from)?;
                    binary64_operation(kind, op, a, b, self.literal_semantics())?
                }
                (NumericValue::Binary64(_), NumericValue::Decimal(_)) => {
                    // Source MultiplicativeExpression performs inplace *= or
                    // /= on an already promoted Python float. It catches only
                    // InvalidOperation/ZeroDivisionError, not this TypeError.
                    // Preserve its constructor at the operation; a callback
                    // cannot reconstruct the operator from the old fault tag.
                    if self.literal_semantics() == QueryLiteralSemantics::SourceLiteralV1
                        && kind == NumericChainKind::Multiplicative
                    {
                        use source_exception::{
                            SourceLanguageException, SourceLanguageExceptionKind,
                        };
                        let operator = match op {
                            NumericBinaryOp::Multiply => "*=",
                            NumericBinaryOp::Divide => "/=",
                            _ => return Err(RawNumericFault::InvalidSourceOrigin.into()),
                        };
                        return Err(RawNumericFault::SourceLanguageException(SourceLanguageException::text(
                            SourceLanguageExceptionKind::TypeError,
                            format!("unsupported operand type(s) for {operator}: 'float' and 'decimal.Decimal'"),
                        )).into());
                    }
                    return Err(RawNumericFault::MixedNumericTypeError.into());
                }
                (NumericValue::Decimal(_), NumericValue::Binary64(_)) => {
                    return Err(RawNumericFault::MixedNumericTypeError.into());
                }
                (NumericValue::Integer(a), NumericValue::Integer(b)) => NumericValue::Integer(
                    apply_integer(
                        match op {
                            NumericBinaryOp::Add => IntegerOp::Add,
                            NumericBinaryOp::Subtract => IntegerOp::Subtract,
                            _ => return Err(RawNumericFault::InvalidSourceOrigin.into()),
                        },
                        &a,
                        Some(&b),
                        &self.limits,
                    )
                    .map_err(RawNumericFault::from)?,
                ),
                (a, b) => {
                    let a = exact_decimal(a, &self.limits)?;
                    let b = exact_decimal(b, &self.limits)?;
                    let result = self.observe(self.decimal.apply(
                        match op {
                            NumericBinaryOp::Add => DecimalOp::Add,
                            NumericBinaryOp::Subtract => DecimalOp::Subtract,
                            NumericBinaryOp::Multiply => DecimalOp::Multiply,
                            NumericBinaryOp::Divide => DecimalOp::Divide,
                        },
                        &a,
                        Some(&b),
                        &self.limits,
                    ));
                    match result {
                        Ok(value) => NumericValue::Decimal(value),
                        Err(NumericFault::DecimalTrap {
                            primary: DecimalSignal::InvalidOperation | DecimalSignal::DivisionByZero,
                            ..
                        }) if kind == NumericChainKind::Multiplicative => {
                            return Err(divide_by_zero_error(self.literal_semantics()));
                        }
                        Err(error) => return Err(self.source_arithmetic_fault(error).into()),
                    }
                }
            };
        }
        self.construct_result(value, (kind == NumericChainKind::Additive).then_some(dt))
            .map_err(Into::into)
    }
}

fn binary64_operation(
    kind: NumericChainKind,
    op: NumericBinaryOp,
    left: f64,
    right: f64,
    literal_semantics: QueryLiteralSemantics,
) -> Result<NumericValue, NumericEvaluationError> {
    let op = match op {
        NumericBinaryOp::Add => Binary64BinaryOp::Add,
        NumericBinaryOp::Subtract => Binary64BinaryOp::Subtract,
        NumericBinaryOp::Multiply => Binary64BinaryOp::Multiply,
        NumericBinaryOp::Divide => Binary64BinaryOp::Divide,
    };
    match apply_binary64_binary(op, left, right) {
        Ok(value) => Ok(NumericValue::Binary64(value)),
        Err(Binary64Fault::ZeroDivision) if kind == NumericChainKind::Multiplicative => {
            Err(divide_by_zero_error(literal_semantics))
        }
        Err(error) => Err(RawNumericFault::from(error).into()),
    }
}

fn divide_by_zero_error(literal_semantics: QueryLiteralSemantics) -> NumericEvaluationError {
    // operators.py:769-772 constructs this exact single-argument exception for
    // the two caught Decimal traps and Python float ZeroDivisionError.
    NumericEvaluationError::Sparql {
        kind: SparqlErrorKind::OtherSparql,
        diagnostic: Cow::Borrowed("divide by 0"),
        source: (literal_semantics == QueryLiteralSemantics::SourceLiteralV1).then(|| {
            SourceException {
                class: SourceExceptionClass::Sparql,
                argument: SourceExceptionArgument::Text("divide by 0".to_owned()),
            }
        }),
    }
}

fn exact_decimal(
    value: NumericValue,
    limits: &NumericLimits,
) -> Result<semantica_numeric::DecimalValue, NumericEvaluationError> {
    match value {
        NumericValue::Integer(value) => {
            decimal_from_integer(&value, limits).map_err(|e| RawNumericFault::Core(e).into())
        }
        NumericValue::Decimal(value) => Ok(value),
        NumericValue::Binary64(_) => Err(RawNumericFault::UnsupportedNumericFamily.into()),
    }
}
fn promote(a: &NamedNode, b: &NamedNode) -> Result<NamedNode, NumericEvaluationError> {
    let rank = |dt| {
        numeric_kind(dt).map(|kind| match kind {
            NumericKind::Integer => 0,
            NumericKind::Decimal => 1,
            NumericKind::Float => 2,
            NumericKind::Double => 3,
        })
    };
    let (Some(a), Some(b)) = (rank(a), rank(b)) else {
        return Err(RawNumericFault::InvalidSourceOrigin.into());
    };
    Ok(datatype(match a.max(b) {
        0 => NumericKind::Integer,
        1 => NumericKind::Decimal,
        2 => NumericKind::Float,
        _ => NumericKind::Double,
    }))
}
fn valid_magnitude(kind: NumericTokenKind, text: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    match kind {
        NumericTokenKind::Integer => digits(text),
        NumericTokenKind::Decimal => text
            .split_once('.')
            .is_some_and(|(a, b)| (a.is_empty() || digits(a)) && digits(b)),
        NumericTokenKind::Double => text.split_once(['e', 'E']).is_some_and(|(m, e)| {
            let e = e.strip_prefix(['+', '-']).unwrap_or(e);
            digits(e)
                && if let Some((a, b)) = m.split_once('.') {
                    (digits(a) && (b.is_empty() || digits(b))) || (a.is_empty() && digits(b))
                } else {
                    digits(m)
                }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    fn session() -> QueryNumericSession {
        QueryNumericSession::new(
            DecimalContext::default(),
            NumericLimits::default(),
            Some(4300),
            true,
        )
        .unwrap()
    }
    fn literal(text: &str, kind: NumericKind) -> Literal {
        Literal::new_typed_literal(text, datatype(kind))
    }
    fn known(value: &RawExpressionValue) -> &NumericValue {
        if let NumericCacheState::Known(cache) = value.numeric() {
            cache.value()
        } else {
            panic!("expected known cache")
        }
    }
    fn lexical(value: &RawExpressionValue) -> &str {
        if let Term::Literal(l) = value.rdf().unwrap() {
            l.value()
        } else {
            panic!("expected literal")
        }
    }
    #[test]
    fn typed_query_literal_and_direct_string_keep_distinct_constructor_paths() {
        let s = session();
        let l = literal("1E+2", NumericKind::Decimal);
        let query = s
            .construct_literal(&l, NumericInputOrigin::QueryTypedExistingLiteral)
            .unwrap();
        let input = s
            .construct_literal(&l, NumericInputOrigin::DirectString { normalize: true })
            .unwrap();
        assert_eq!(lexical(&query), "1E+2");
        assert_eq!(lexical(&input), "100");
        assert_eq!(known(&query), known(&input));
        let numerator = s
            .construct_numeric_token(NumericTokenKind::Integer, NumericTokenSign::Unsigned, "100")
            .unwrap();
        let result = s
            .evaluate_chain(
                NumericChainKind::Multiplicative,
                SolutionValue::Rdf(numerator),
                vec![(NumericBinaryOp::Divide, SolutionValue::Rdf(input))],
            )
            .unwrap();
        assert_eq!(lexical(&result), "1.00");
    }
    #[test]
    fn known_object_cache_type_is_independent_of_explicit_numeric_datatype() {
        // term.py:695-705: Literal(1, datatype=XSD.decimal) keeps Python int 1.
        // operators.py:830-848 computes int+int, then supplies the promoted
        // datatype to Literal without re-parsing that arithmetic value.
        let s = session();
        let first = s
            .known_cached(
                literal("1", NumericKind::Decimal).into(),
                NumericValue::Integer(BigInt::from(1)),
            )
            .unwrap();
        assert!(s.typed_ebv(&first).unwrap());
        let rhs = s
            .construct_numeric_token(NumericTokenKind::Integer, NumericTokenSign::Unsigned, "1")
            .unwrap();
        let result = s
            .evaluate_chain(
                NumericChainKind::Additive,
                SolutionValue::Rdf(first),
                vec![(NumericBinaryOp::Add, SolutionValue::Rdf(rhs))],
            )
            .unwrap();
        assert_eq!(
            result.rdf().unwrap(),
            &Term::Literal(literal("2", NumericKind::Decimal))
        );
        assert_eq!(known(&result), &NumericValue::Integer(BigInt::from(2)));
        let unary = s
            .evaluate_unary(NumericUnaryOp::Plus, SolutionValue::Rdf(result))
            .unwrap();
        assert_eq!(
            unary.rdf().unwrap(),
            &Term::Literal(literal("2", NumericKind::Integer))
        );

        let scaled = semantica_numeric::parse_decimal_exact("1.00", s.limits()).unwrap();
        let raw = s
            .known_cached(
                literal("1.00", NumericKind::Integer).into(),
                NumericValue::Decimal(scaled.clone()),
            )
            .unwrap();
        assert_eq!(known(&raw), &NumericValue::Decimal(scaled));
        assert!(s.typed_ebv(&raw).unwrap());
    }
    #[test]
    fn equal_rdf_values_retain_distinct_numeric_cache_and_hash_only_rdf() {
        let s = session();
        let a = s
            .construct_literal(
                &literal("1E+2", NumericKind::Decimal),
                NumericInputOrigin::DirectString { normalize: true },
            )
            .unwrap();
        let b = s
            .construct_literal(
                &literal("100", NumericKind::Decimal),
                NumericInputOrigin::DirectString { normalize: true },
            )
            .unwrap();
        assert_eq!(a, b);
        assert_ne!(known(&a), known(&b));
        let mut x = DefaultHasher::new();
        let mut y = DefaultHasher::new();
        a.hash(&mut x);
        a.rdf().unwrap().hash(&mut y);
        assert_eq!(x.finish(), y.finish());
    }
    #[test]
    fn token_constructor_normalizes_integer_but_preserves_typed_lexical() {
        let s = session();
        let token = s
            .construct_numeric_token(NumericTokenKind::Integer, NumericTokenSign::Unsigned, "01")
            .unwrap();
        let typed = s
            .construct_literal(
                &literal("01", NumericKind::Integer),
                NumericInputOrigin::QueryTypedExistingLiteral,
            )
            .unwrap();
        assert_eq!(lexical(&token), "1");
        assert_eq!(lexical(&typed), "01");
        assert_ne!(token, typed);
        assert_eq!(known(&token), known(&typed));
    }
    #[test]
    fn negative_decimal_token_fails_but_explicit_negative_zero_survives() {
        let s = session();
        assert!(matches!(
            s.construct_numeric_token(NumericTokenKind::Decimal, NumericTokenSign::Negative, "0.0"),
            Err(RawNumericFault::ParserLiteralType)
        ));
        let explicit = s
            .construct_literal(
                &literal("-0.00", NumericKind::Decimal),
                NumericInputOrigin::QueryTypedExistingLiteral,
            )
            .unwrap();
        assert_eq!(lexical(&explicit), "-0.00");
        let result = s
            .evaluate_unary(NumericUnaryOp::Minus, SolutionValue::Rdf(explicit))
            .unwrap();
        assert_eq!(lexical(&result), "0.00");
    }
    #[test]
    fn numeric_unknown_ebv_and_arithmetic_are_fatal() {
        let s = session();
        let value = RawExpressionValue::from_import(literal("100", NumericKind::Decimal).into());
        assert!(matches!(
            s.typed_ebv(&value),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnknownNumericCache
            ))
        ));
        assert!(matches!(
            s.evaluate_unary(NumericUnaryOp::Plus, SolutionValue::Rdf(value)),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnknownNumericCache
            ))
        ));
    }
    #[test]
    fn multiplication_catches_only_original_decimal_traps() {
        let s = session();
        let one = s
            .construct_result(NumericValue::Integer(BigInt::from(1)), None)
            .unwrap();
        let zero = s
            .construct_result(NumericValue::Integer(BigInt::from(0)), None)
            .unwrap();
        assert!(matches!(
            s.evaluate_chain(
                NumericChainKind::Multiplicative,
                SolutionValue::Rdf(one),
                vec![(NumericBinaryOp::Divide, SolutionValue::Rdf(zero))]
            ),
            Err(NumericEvaluationError::Sparql {
                kind: SparqlErrorKind::OtherSparql,
                ..
            })
        ));
        let nan = s
            .construct_literal(
                &literal("sNaN", NumericKind::Decimal),
                NumericInputOrigin::QueryTypedExistingLiteral,
            )
            .unwrap();
        assert_eq!(s.typed_ebv(&nan).unwrap(), true);
        assert!(matches!(
            s.evaluate_unary(NumericUnaryOp::Plus, SolutionValue::Rdf(nan)),
            Err(NumericEvaluationError::Fatal(RawNumericFault::Core(
                NumericFault::DecimalTrap {
                    primary: DecimalSignal::InvalidOperation,
                    ..
                }
            )))
        ));
    }
    #[test]
    fn float_leaf_construction_does_not_grant_float_arithmetic() {
        let s = session();
        let value = s
            .construct_numeric_token(NumericTokenKind::Double, NumericTokenSign::Unsigned, "1e0")
            .unwrap();
        assert_eq!(lexical(&value), "1.0");
        assert!(matches!(
            s.evaluate_unary(NumericUnaryOp::Plus, SolutionValue::Rdf(value)),
            Err(NumericEvaluationError::Fatal(
                RawNumericFault::UnsupportedNumericFamily
            ))
        ));
    }
    #[test]
    fn explicit_binary64_capability_retains_default_constructor_boundary() {
        for capabilities in [
            QueryNumericCapabilities::IntegerDecimalV1,
            QueryNumericCapabilities::MixedBinary64V1,
        ] {
            let s = QueryNumericSession::new_with_capabilities(
                DecimalContext::default(),
                NumericLimits::default(),
                Some(4300),
                true,
                capabilities,
            )
            .unwrap();
            assert_eq!(s.capabilities(), capabilities);
            let first = s
                .construct_numeric_token(
                    NumericTokenKind::Double,
                    NumericTokenSign::Unsigned,
                    "0.5e0",
                )
                .unwrap();
            // Existing float leaf EBV is independent of arithmetic permission.
            assert!(s.typed_ebv(&first).unwrap());
            let rhs = s
                .construct_numeric_token(NumericTokenKind::Integer, NumericTokenSign::Unsigned, "1")
                .unwrap();
            let result = s.evaluate_chain(
                NumericChainKind::Multiplicative,
                SolutionValue::Rdf(first),
                vec![(NumericBinaryOp::Multiply, SolutionValue::Rdf(rhs))],
            );
            assert_eq!(
                s.flags(),
                DecimalSignals::from(DecimalSignal::FloatOperation)
            );
            match capabilities {
                QueryNumericCapabilities::IntegerDecimalV1 => assert!(matches!(
                    result,
                    Err(NumericEvaluationError::Fatal(
                        RawNumericFault::UnsupportedNumericFamily
                    ))
                )),
                QueryNumericCapabilities::MixedBinary64V1 => assert_eq!(
                    result.unwrap().rdf().unwrap(),
                    &Term::Literal(literal("0.5", NumericKind::Decimal))
                ),
            }
        }
        assert_eq!(
            session().capabilities(),
            QueryNumericCapabilities::IntegerDecimalV1
        );
    }
    #[test]
    fn explicit_binary64_unary_preserves_negative_zero_and_default_datatype() {
        let s = QueryNumericSession::new_with_capabilities(
            DecimalContext::default(),
            NumericLimits::default(),
            Some(4300),
            true,
            QueryNumericCapabilities::MixedBinary64V1,
        )
        .unwrap();
        let value = s
            .construct_literal(
                &literal("-0.0", NumericKind::Float),
                NumericInputOrigin::QueryTypedExistingLiteral,
            )
            .unwrap();
        assert!(!s.typed_ebv(&value).unwrap());
        let result = s
            .evaluate_unary(NumericUnaryOp::Plus, SolutionValue::Rdf(value))
            .unwrap();
        assert_eq!(
            result.rdf().unwrap(),
            &Term::Literal(literal("-0.0", NumericKind::Double))
        );
        let NumericValue::Binary64(value) = known(&result) else {
            panic!("expected Binary64 cache");
        };
        assert_eq!(value.to_bits(), 0x8000_0000_0000_0000);
        assert_eq!(s.flags(), DecimalSignals::EMPTY);
    }
    #[test]
    fn untrapped_bad_decimal_constructor_keeps_nan_cache_and_sticky_signal() {
        let mut context = DecimalContext::default();
        context.traps = DecimalSignals::EMPTY;
        let s = QueryNumericSession::new(context, NumericLimits::default(), None, true).unwrap();
        let value = s
            .construct_literal(
                &literal("bad", NumericKind::Decimal),
                NumericInputOrigin::QueryTypedExistingLiteral,
            )
            .unwrap();
        assert_eq!(lexical(&value), "bad");
        assert!(matches!(
            known(&value),
            NumericValue::Decimal(semantica_numeric::DecimalValue::NaN { .. })
        ));
        assert!(s.flags().contains(DecimalSignal::InvalidOperation));
    }
    #[test]
    fn unimplemented_string_identity_normalization_rejects_constructor_origins() {
        let s = session();
        for name in ["normalizedString", "token"] {
            let literal = Literal::new_typed_literal(
                " a\t b\n ",
                NamedNode::new_unchecked(format!("{XSD}{name}")),
            );
            for origin in [
                NumericInputOrigin::QueryTypedExistingLiteral,
                NumericInputOrigin::PreparedQueryLiteral,
                NumericInputOrigin::DirectString { normalize: false },
                NumericInputOrigin::DirectString { normalize: true },
            ] {
                assert!(matches!(
                    s.construct_literal(&literal, origin),
                    Err(RawNumericFault::UnsupportedLiteralFallback)
                ));
            }
            // An imported RDF term already has its own identity. Import does
            // not claim to reproduce a source Literal constructor.
            let imported = s
                .construct_literal(&literal, NumericInputOrigin::UnknownNormalized)
                .unwrap();
            assert_eq!(imported.rdf().unwrap(), &Term::Literal(literal));
        }
    }
}
