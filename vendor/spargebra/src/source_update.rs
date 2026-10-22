//! Original source UPDATE syntax, produced by the same PEG and literal owner.
//! Graph management remains an operation; it is not rewritten to native
//! DELETE/INSERT, whose graph scopes and failure order have different meaning.
use crate::algebra::{GraphPattern, GraphTarget};
use crate::term::{GraphName, GraphNamePattern, NamedNode, TriplePattern};
use oxiri::Iri;

#[derive(Debug, thiserror::Error)]
#[error("Unknown namespace prefix : {0}")]
pub(crate) struct SourceUpdateNamespaceError(pub String);

#[derive(Debug, Clone)]
pub struct SourceUpdate {
    pub operations: Vec<SourceUpdateOperation>,
}

#[derive(Debug, Clone)]
pub struct SourceUpdateOperation {
    pub kind: SourceUpdateOperationKind,
    /// The prologue in force for this operation, before a later BASE change.
    pub base_iri: Option<Iri<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct SourceUpdateData {
    pub triples: Vec<TriplePattern>,
    /// Original insertion-ordered graph dictionary. Repeated graph blocks
    /// append their independently ordered triples to the first graph entry.
    pub quads: Vec<(GraphNamePattern, Vec<TriplePattern>)>,
}

#[derive(Debug, Clone)]
pub enum SourceUpdateOperationKind {
    InsertData(SourceUpdateData),
    DeleteData(SourceUpdateData),
    DeleteWhere(SourceUpdateData),
    Modify {
        delete: Option<SourceUpdateData>,
        insert: Option<SourceUpdateData>,
        with: Option<NamedNode>,
        /// In request order; default/named resources are not an RDF Dataset
        /// specification or authorization to fetch anything.
        using: Vec<(Option<NamedNode>, Option<NamedNode>)>,
        pattern: Box<GraphPattern>,
    },
    Load {
        silent: bool,
        source: NamedNode,
        destination: Option<NamedNode>,
    },
    Clear {
        silent: bool,
        graph: GraphTarget,
    },
    Drop {
        silent: bool,
        graph: GraphTarget,
    },
    Create {
        silent: bool,
        graph: NamedNode,
    },
    Add {
        silent: bool,
        from: GraphName,
        to: GraphName,
    },
    Move {
        silent: bool,
        from: GraphName,
        to: GraphName,
    },
    Copy {
        silent: bool,
        from: GraphName,
        to: GraphName,
    },
}
