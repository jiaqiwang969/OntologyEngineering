//! Construction facts observed at the actual JSON-LD emission index.
use oxrdf::{Literal, Quad, Term};
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceValue {
    String(String),
    /// Original JSON token, before native RDF numeric canonicalization.
    Number(String),
    Boolean(bool),
    /// Original expanded JSON value events, before native RFC 8785 formatting.
    Json(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLiteral {
    pub native: Literal,
    pub value: SourceValue,
    pub datatype: Option<String>,
    pub language: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceQuad {
    pub quad: Quad,
    pub literal: Option<SourceLiteral>,
}
impl SourceQuad {
    pub(crate) fn bound_to_actual(&self) -> bool {
        match (&self.quad.object, &self.literal) {
            (Term::NamedNode(_) | Term::BlankNode(_), None) => true,
            (Term::Literal(actual), Some(source)) => actual == &source.native,
            _ => false,
        }
    }
}

pub(crate) fn error(message: impl Into<String>) -> crate::JsonLdSyntaxError {
    crate::JsonLdSyntaxError::msg_and_code(message, crate::JsonLdErrorCode::InvalidValueObject)
}
