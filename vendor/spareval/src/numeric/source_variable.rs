//! Fixed term.Variable constructor, sharing the existing raw value owner.
//! A stored Variable is a Node, not an IRI and not an absent solution cell.
use super::*;
use oxrdf::Variable;
impl QueryNumericSession {
    pub(crate) fn construct_source_variable(&self, input: &str) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        self.check_text(input)?;
        if input.is_empty() {
            return Err(RawNumericFault::SourceLanguageException(source_exception::SourceLanguageException::text(
                source_exception::SourceLanguageExceptionKind::Exception,
                "Attempted to create variable with empty string as name!",
            )));
        }
        self.source_variable_from_name(Variable::new_unchecked(input.strip_prefix('?').unwrap_or(input)))
    }
    pub(super) fn source_variable_from_name(&self, name: Variable) -> Result<RawExpressionValue, RawNumericFault> {
        self.require_source_literal_semantics()?;
        self.check_text(name.as_str())?;
        Ok(RawExpressionValue {
            source_term: RawSourceTerm::Variable(name),
            numeric: NumericCacheState::NotNumeric,
            literal_source: LiteralSourceState::NotLiteral,
            source_object: None,
            source_datatype_object: None,
            source_boolean_constructor: SourceBooleanConstructor::Object,
            source_string_cast_language: false,
        }.with_new_source_object())
    }
}
