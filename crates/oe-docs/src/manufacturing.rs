//! The source-bound manufacturing report contract has one implementation owner.
//! Re-export its exact decimal arithmetic, identity checks, rendering and
//! publication APIs instead of maintaining a second BOM/schema/cost engine.
pub use oe_engineering::manufacturing_report::*;
