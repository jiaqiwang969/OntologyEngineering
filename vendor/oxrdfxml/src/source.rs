//! Facts from actual RDF/XML construction, paired with actual emissions.
use oxrdf::{Literal, Term, Triple};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLiteral {
    /// Actual native construction, before source-specific lexical normalization.
    pub native: Literal,
    pub value: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceTriple {
    pub triple: Triple,
    pub literal: Option<SourceLiteral>,
}
impl SourceTriple {
    pub(crate) fn bound_to_actual(&self) -> bool {
        match (&self.triple.object, &self.literal) {
            (Term::NamedNode(_) | Term::BlankNode(_), None) => true,
            (Term::Literal(actual), Some(s)) => actual == &s.native,
            _ => false,
        }
    }
}
