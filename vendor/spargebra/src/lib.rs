#![cfg_attr(doc, doc = include_str!("../README.md"))]
#![doc(test(attr(deny(warnings))))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_favicon_url = "https://raw.githubusercontent.com/oxigraph/oxigraph/main/logo.svg")]
#![doc(html_logo_url = "https://raw.githubusercontent.com/oxigraph/oxigraph/main/logo.svg")]

pub mod algebra;
mod parser;
mod query;
mod source_exists;
mod source_literal_occurrence;
mod source_path;
mod source_triple_rows;
mod source_update;
pub mod term;
mod update;

#[doc(hidden)]
pub use algebra::{ParsedSourceLiteral, SourceDatatype, SourceLiteralOrigin, SourceValueTerm};
#[doc(hidden)]
pub use parser::{
    NumericLiteralConstructor, NumericQueryParseError, NumericTokenKind, NumericTokenSign,
    SourceBlankNodeConstructor, SourceParserError, source_default_blank_node,
};
pub use parser::{SparqlParser, SparqlSyntaxError};
pub use query::*;
#[doc(hidden)]
pub use source_literal_occurrence::{
    SourceLiteralBridgeError, SourceLiteralConstruct, SourceLiteralHooks, SourceLiteralInput,
    SourceLiteralOccurrenceId, SourceLiteralScope, SourceLiteralTruth, SourceNativeTriplesOrder,
    SourcePathAdmission, SourceTriplesOrder, SourceVariableSetStep, SourceVariableSets,
    SourceVisibleProjection,
};
pub use update::*;

#[cfg(feature = "rdflib760-nullable-internal")]
#[doc(hidden)]
pub use source_update::{SourceUpdate, SourceUpdateData, SourceUpdateOperation, SourceUpdateOperationKind};

#[doc(hidden)]
pub use source_exists::{
    SourceExistsExpression, SourceExistsPolarity, SourceSyntaxAttributes, SourceSyntaxNode,
    SourceSyntaxSnapshot, SourceSyntaxValue,
};

#[doc(hidden)]
pub use source_path::{
    SourcePathDefinition, SourcePathNegatedArgument, SourcePathRepeat, SourceTriplePattern,
    SourceTriplePredicate,
};

#[doc(hidden)]
pub use source_triple_rows::{SourceTripleRow, SourceTripleRowsError, source_group_triple_rows};
