//! Reading an existing engineering drawing: the reverse side of the CAD route.
//!
//! Every source entity is kept with its native members; source uses are nonexclusive and carry bounded
//! intervals; complete expressions are classified on four independent axes (discipline kind, line type,
//! native primitive, view); structure topology gives every free outline end a typed cause and joins
//! connected pieces into structural lines. Part feature targets stay unknown. Everything is derived from
//! the drawing bytes only, in an isolated worker under the current Project.
pub(crate) mod discipline_discovery;
pub mod engineering_expression;
pub mod expression_text;
pub(crate) mod native_layers;
pub(crate) mod native_readback;
pub mod page;
pub mod review;
pub mod revisions;
pub mod correction;
pub mod drafting_issues;
pub mod route;
pub mod usage_discovery;
