//! Exact source-language exception constructors shared by originating owners.
//! Language exceptions are distinct from cancellation, resources and adapters.
//! RDFLib operators.py / CPython behavior; see existing snapshot BSD/PSF terms.
use oxrdf::Variable;
use semantica_numeric::DecimalSignal;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LanguageExceptionArgument {
    None,
    Text(String),
    Variable(Variable),
    DecimalSignalClasses(Vec<DecimalSignal>),
    /// Decimal divmod condition; not an additional sticky context signal.
    DecimalDivisionImpossible,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceLanguageExceptionKind {
    Exception,
    TypeError,
    ValueError,
    OverflowError,
    IndexError,
    RecursionError,
    KeyError,
    AttributeError,
    Decimal(DecimalSignal),
}
impl SourceLanguageExceptionKind {
    pub(crate) fn is_type_error(self) -> bool {
        matches!(
            self,
            Self::TypeError | Self::Decimal(DecimalSignal::FloatOperation)
        )
    }
    pub(crate) fn module(self) -> &'static str {
        if matches!(self, Self::Decimal(_)) {
            "decimal"
        } else {
            "builtins"
        }
    }
    pub(crate) fn class_name(self) -> &'static str {
        match self {
            Self::Exception => "Exception",
            Self::TypeError => "TypeError",
            Self::ValueError => "ValueError",
            Self::OverflowError => "OverflowError",
            Self::IndexError => "IndexError",
            Self::RecursionError => "RecursionError",
            Self::KeyError => "KeyError",
            Self::AttributeError => "AttributeError",
            Self::Decimal(signal) => decimal_signal_name(signal),
        }
    }
}
fn decimal_signal_name(signal: DecimalSignal) -> &'static str {
    match signal {
        DecimalSignal::InvalidOperation => "InvalidOperation",
        DecimalSignal::DivisionByZero => "DivisionByZero",
        DecimalSignal::Overflow => "Overflow",
        DecimalSignal::Underflow => "Underflow",
        DecimalSignal::Subnormal => "Subnormal",
        DecimalSignal::Inexact => "Inexact",
        DecimalSignal::Rounded => "Rounded",
        DecimalSignal::Clamped => "Clamped",
        DecimalSignal::FloatOperation => "FloatOperation",
    }
}
// The originating numeric owner supplies at most the nine supported signals.
// This is the original class-list representation, not a quoted text argument.
pub(crate) fn decimal_signal_classes_repr(signals: &[DecimalSignal]) -> String {
    format!(
        "[{}]",
        signals
            .iter()
            .map(|s| format!("<class 'decimal.{}'>", decimal_signal_name(*s)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceLanguageException {
    pub(crate) kind: SourceLanguageExceptionKind,
    pub(crate) arguments: Vec<LanguageExceptionArgument>,
}
impl SourceLanguageException {
    pub(crate) fn text(kind: SourceLanguageExceptionKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            arguments: vec![LanguageExceptionArgument::Text(value.into())],
        }
    }
}
impl fmt::Display for SourceLanguageException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source {:?}: {:?}", self.kind, self.arguments)
    }
}
impl std::error::Error for SourceLanguageException {}
