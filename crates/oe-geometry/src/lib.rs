//! Development geometry intake, adapter transport and source-bound readback.
//! Rust owns bounded execution and exact receipt identities. Existing Python
//! geometry tools do not implement this adapter protocol. Native OCCT binding,
//! SDK/runtime lock verification and geometry parity remain unaccepted; wrapping
//! owned Python algorithms is not completion of the Rust rewrite.

pub mod acceptance;
pub mod evidence;
pub mod intake;
pub mod protocol;

pub mod presentation;
