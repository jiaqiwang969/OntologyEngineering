//! Immutable source syntax and instance attributes for the private EXISTS frontend.
//! Ordered mapping fields and instance attributes are separate traversal domains.
use crate::algebra::{
    AggregateFunction, Function, ParsedSourceLiteral, PropertyPathExpression, SourceValueTerm,
};
use crate::term::{BlankNode, NamedNode, NamedNodePattern, TermPattern, TriplePattern, Variable};
use std::sync::Arc;

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SourceExistsPolarity {
    Exists,
    NotExists,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum SourceSyntaxValue {
    Node(usize),
    List(Vec<Self>),
    Variable(Variable),
    /// An unordered Python set, distinct from an ordered syntax list.
    /// The private execution hook retains actual table iteration for repr;
    /// instance variables remain a separately derived sorted membership view.
    VariableSet(Vec<Variable>),
    Literal(ParsedSourceLiteral),
    NamedNode(NamedNode),
    NamedNodePattern(NamedNodePattern),
    Text(String),
    Boolean(bool),
    Unsigned(usize),
    None,
    Function(Function),
    AggregateFunction(AggregateFunction),
    Triples(Vec<TriplePattern>),
    Path {
        subject: TermPattern,
        path: PropertyPathExpression,
        object: TermPattern,
    },
    /// A source Values list contains ordinary dicts, opaque to source traversals.
    Values {
        variables: Vec<Variable>,
        bindings: Vec<Vec<Option<SourceValueTerm>>>,
    },
    /// A parsed blank node is a graph term, not a variable of the same name.
    /// Appended so earlier private syntax hash domains keep their discriminants.
    BlankNode(BlankNode),
    NativePath(crate::SourcePathDefinition),
    NativeTriples(Vec<crate::SourceTriplePattern>),
    /// Ordered ordinary dictionaries produced by source dict(zip(vars, row)).
    /// Missing keys and explicit UNDEF are distinct; duplicate keys retain
    /// their first position and last value. Opaque to source AST traversals.
    /// Appended to preserve all earlier private syntax hash discriminants.
    ValueRecords {
        variables: Vec<Variable>,
        records: Vec<Vec<(Variable, Option<SourceValueTerm>)>>,
    },
}

#[doc(hidden)]
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct SourceSyntaxAttributes {
    /// Query translation assigns these through CompValue dot attributes. Their
    /// roots may differ from the same-named ordered mapping fields, while Node
    /// values still refer to the shared arena nodes.
    pub expr: Option<SourceSyntaxValue>,
    pub having: Option<SourceSyntaxValue>,
    pub orderby: Option<SourceSyntaxValue>,
    pub where_clause: Option<SourceSyntaxValue>,
    /// Instance-only empty group parts created by translateGroupGraphPattern.
    pub part: Option<Vec<SourceSyntaxValue>>,
    pub graph: Option<usize>,
    pub translated: Option<bool>,
    pub no_isolated_scope: Option<bool>,
    pub variables: Option<Vec<Variable>>,
    pub lazy: Option<bool>,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SourceSyntaxNode {
    pub name: String,
    pub fields: Vec<(String, SourceSyntaxValue)>,
    pub attributes: SourceSyntaxAttributes,
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SourceSyntaxSnapshot {
    nodes: Arc<[SourceSyntaxNode]>,
    root: usize,
}
impl SourceSyntaxSnapshot {
    /// Construct ordered operand metadata. This carries no execution admission
    /// or literal provenance; the private compiler validates referenced nodes
    /// and resolves literal occurrences through its own sealed registry.
    #[doc(hidden)]
    pub fn from_ordered_nodes(nodes: Vec<SourceSyntaxNode>, root: usize) -> Option<Self> {
        (root < nodes.len()).then(|| Self {
            nodes: nodes.into(),
            root,
        })
    }

    pub fn nodes(&self) -> &[SourceSyntaxNode] {
        &self.nodes
    }
    pub fn root(&self) -> usize {
        self.root
    }
    pub fn root_node(&self) -> Option<&SourceSyntaxNode> {
        self.nodes.get(self.root)
    }
    pub(crate) fn captured(root: usize) -> Self {
        Self {
            nodes: Arc::from([]),
            root,
        }
    }
    pub(crate) fn finalized(nodes: Arc<[SourceSyntaxNode]>, root: usize) -> Self {
        Self { nodes, root }
    }
    pub(crate) fn is_captured(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn lookup_used_variables<'a>(&'a self, callback: &mut impl FnMut(&'a Variable)) {
        fn visit<'a>(
            nodes: &'a [SourceSyntaxNode],
            value: &'a SourceSyntaxValue,
            callback: &mut impl FnMut(&'a Variable),
        ) {
            match value {
                SourceSyntaxValue::Variable(value) => callback(value),
                SourceSyntaxValue::Node(id) => {
                    if let Some(node) = nodes.get(*id) {
                        for (_, value) in &node.fields {
                            visit(nodes, value, callback);
                        }
                    }
                }
                SourceSyntaxValue::List(values) => {
                    for value in values {
                        visit(nodes, value, callback);
                    }
                }
                SourceSyntaxValue::NamedNodePattern(NamedNodePattern::Variable(variable)) => {
                    callback(variable);
                }
                SourceSyntaxValue::Triples(patterns) => {
                    for pattern in patterns {
                        crate::algebra::lookup_triple_pattern_variables(pattern, callback);
                    }
                }
                SourceSyntaxValue::NativeTriples(patterns) => {
                    for pattern in patterns {
                        pattern.lookup_variables(callback);
                    }
                }
                SourceSyntaxValue::Path {
                    subject, object, ..
                } => {
                    for term in [subject, object] {
                        if let TermPattern::Variable(variable) = term {
                            callback(variable);
                        }
                        #[cfg(feature = "sparql-12")]
                        if let TermPattern::Triple(triple) = term {
                            crate::algebra::lookup_triple_pattern_variables(triple, callback);
                        }
                    }
                }
                // Ordinary dicts in Values are deliberately opaque.
                _ => (),
            }
        }
        if let Some(node) = self.root_node() {
            for (_, value) in &node.fields {
                visit(&self.nodes, value, callback);
            }
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SourceExistsExpression<G> {
    pub polarity: SourceExistsPolarity,
    /// The actual ordered mapping, after the source frontend phases. Attributes
    /// in this snapshot never become children of the mapping visitor.
    pub syntax: SourceSyntaxSnapshot,
    /// None is an untranslated source expression, not an empty graph.
    pub graph: Option<Box<G>>,
    pub no_isolated_scope: bool,
}
impl<G> SourceExistsExpression<G> {
    pub fn map_graph<H>(&self, map: impl FnOnce(&G) -> H) -> SourceExistsExpression<H> {
        SourceExistsExpression {
            polarity: self.polarity,
            syntax: self.syntax.clone(),
            graph: self.graph.as_deref().map(|graph| Box::new(map(graph))),
            no_isolated_scope: self.no_isolated_scope,
        }
    }
}
