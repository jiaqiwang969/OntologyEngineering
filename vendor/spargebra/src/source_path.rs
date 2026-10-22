//! Constructor facts for a native source path, lowered once by the private
//! executor. These are query operands, never RDF predicate identities.
use crate::SourceSyntaxSnapshot;
use crate::algebra::{GraphPattern, PropertyPathExpression};
use crate::term::NamedNode;
use crate::term::{NamedNodePattern, TermPattern};

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SourcePathRepeat {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourcePathDefinition {
    NamedNode(NamedNode),
    Inverse(Box<Self>),
    Sequence(Vec<Self>),
    Alternative(Vec<Self>),
    Repeat {
        inner: Box<Self>,
        mode: SourcePathRepeat,
    },
    Negated(Vec<SourcePathNegatedArgument>),
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourcePathNegatedArgument {
    NamedNode(NamedNode),
    Inverse(Box<SourcePathDefinition>),
    /// Source translation can leave an actual CompValue in NegatedPath.args.
    /// Retain its ordered fields; standard SPARQL inverse expansion is not
    /// evidence for a different constructor result.
    Untranslated(SourceSyntaxSnapshot),
}

/// Same-parse physical operands alongside the actual translated source
/// predicate. Physical paths retain plan ownership/variables; sealed source
/// execution compiles the native definition instead.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourceTriplePredicate {
    Rdf(NamedNodePattern),
    Path {
        definition: SourcePathDefinition,
        physical: PropertyPathExpression,
    },
}
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SourceTriplePattern {
    pub subject: TermPattern,
    pub predicate: SourceTriplePredicate,
    pub object: TermPattern,
}
impl SourceTriplePattern {
    #[doc(hidden)]
    pub fn physical_pattern(&self) -> GraphPattern {
        match &self.predicate {
            SourceTriplePredicate::Rdf(predicate) => GraphPattern::Bgp {
                patterns: vec![crate::term::TriplePattern::new(
                    self.subject.clone(),
                    predicate.clone(),
                    self.object.clone(),
                )],
            },
            SourceTriplePredicate::Path { physical, .. } => GraphPattern::Path {
                subject: self.subject.clone(),
                path: physical.clone(),
                object: self.object.clone(),
            },
        }
    }
    #[doc(hidden)]
    pub fn lookup_variables<'a>(&'a self, callback: &mut impl FnMut(&'a crate::term::Variable)) {
        if let TermPattern::Variable(v) = &self.subject {
            callback(v);
        }
        if let SourceTriplePredicate::Rdf(NamedNodePattern::Variable(v)) = &self.predicate {
            callback(v);
        }
        if let TermPattern::Variable(v) = &self.object {
            callback(v);
        }
    }
}
