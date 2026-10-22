//! A bounded native browser tool inside the engineering workflow.
//!
//! The calling Agent selects observed actions under explicit conditions. Browser execution
//! produces evidence; it does not grant permission or semantic/engineering acceptance.
pub mod backend;
pub mod source;
pub mod task;
pub mod types;

pub use task::{AllowedAction, BrowserTask};
pub use types::{
    Action, ActionKind, BackendKind, Browser, DispatchState, ExecutionReceipt, Observation,
    SessionIdentity, SnapshotSource, fingerprint_page,
};
