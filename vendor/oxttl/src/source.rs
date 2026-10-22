//! Internal source observations emitted by the same Turtle/TriG recognizer.
use oxrdf::Quad;

/// Literal facts observed before the native RDF representation erases them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceLiteral {
    pub value: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
}

/// One actual parser output and its associated literal construction facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceQuad {
    pub quad: Quad,
    pub literal: Option<SourceLiteral>,
}

#[cfg(feature = "source-literal-internal")]
mod parser {
    use super::*;
    use crate::{TurtleSyntaxError, terse::TriGRecognizer, toolkit::Parser};
    use oxrdf::{Literal, NamedNode, Term};

    #[derive(Debug, thiserror::Error)]
    pub enum SourceParseError {
        #[error(transparent)]
        Syntax(#[from] TurtleSyntaxError),
        #[error("source_rdf_input_byte_budget")]
        InputBudget,
        #[error("source_rdf_parser_fact_binding")]
        FactBinding,
        #[error("source_rdf_base_iri: {0}")]
        Base(String),
        #[error("source_rdf_extended_term_not_received")]
        ExtendedTerm,
    }

    /// The native parser cursor owns the source observation cursor as well.
    /// A failure is terminal for this private iterator; a loader stages results
    /// before publication. Ordinary native iterators keep their recovery rules.
    pub struct SourceRdfParser<'a> {
        parser: Parser<&'a [u8], TriGRecognizer>,
        ended: bool,
    }
    impl<'a> SourceRdfParser<'a> {
        pub fn new(
            input: &'a [u8],
            with_graph_name: bool,
            base_iri: Option<&str>,
        ) -> Result<Self, SourceParseError> {
            if input.len() > 16 * 1024 * 1024 {
                return Err(SourceParseError::InputBudget);
            }
            let base = base_iri
                .map(|v| oxiri::Iri::parse(v.to_owned()))
                .transpose()
                .map_err(|e| SourceParseError::Base(e.to_string()))?;
            Ok(Self {
                parser: TriGRecognizer::new_source_parser(input, with_graph_name, base),
                ended: false,
            })
        }

        fn receive(&mut self, native: Quad) -> Result<SourceQuad, SourceParseError> {
            let observed = self
                .parser
                .context
                .source_quads
                .as_mut()
                .and_then(Vec::pop)
                .ok_or(SourceParseError::FactBinding)?;
            if native != observed.quad {
                return Err(SourceParseError::FactBinding);
            }
            match (&native.object, &observed.literal) {
                (Term::NamedNode(_), None) | (Term::BlankNode(_), None) => {}
                (Term::Literal(l), Some(s)) => {
                    let expected = match (&s.datatype, &s.language) {
                        (None, Some(language)) => Literal::new_language_tagged_literal_unchecked(
                            &s.value,
                            language.to_ascii_lowercase(),
                        ),
                        (Some(datatype), None) => {
                            Literal::new_typed_literal(&s.value, NamedNode::new_unchecked(datatype))
                        }
                        (None, None) => Literal::new_simple_literal(&s.value),
                        _ => return Err(SourceParseError::FactBinding),
                    };
                    if l != &expected {
                        return Err(SourceParseError::FactBinding);
                    }
                }
                #[cfg(feature = "rdf-12")]
                (Term::Triple(_), _) => return Err(SourceParseError::ExtendedTerm),
                _ => return Err(SourceParseError::FactBinding),
            };
            // The existing DatasetSession owns unique-quad and exact serialized
            // dataset byte limits. Repeated parser emissions do not change those
            // limits; the loader incrementally budgets only new native quads.
            Ok(observed)
        }
    }
    impl Iterator for SourceRdfParser<'_> {
        type Item = Result<SourceQuad, SourceParseError>;
        fn next(&mut self) -> Option<Self::Item> {
            if self.ended {
                return None;
            }
            let result = match self.parser.parse_next() {
                Some(Ok(quad)) => self.receive(quad),
                Some(Err(e)) => Err(e.into()),
                None => {
                    self.ended = true;
                    return if self
                        .parser
                        .context
                        .source_quads
                        .as_ref()
                        .is_some_and(|v| !v.is_empty())
                    {
                        Some(Err(SourceParseError::FactBinding))
                    } else {
                        None
                    };
                }
            };
            if result.is_err() {
                self.ended = true;
                if let Some(v) = &mut self.parser.context.source_quads {
                    v.clear();
                }
            }
            Some(result)
        }
    }
}
#[cfg(feature = "source-literal-internal")]
pub use parser::{SourceParseError, SourceRdfParser};
