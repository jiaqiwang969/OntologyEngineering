//! Windows platform primitives only; protocol adjudication lives in oe-nx-protocol.
#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub mod adapter;
#[cfg(windows)]
pub mod controller;

#[cfg(windows)]
pub mod legacy_review;

#[cfg(windows)]
pub mod wmi;

#[cfg(windows)]
pub mod dispatch_review;

/// Portable finite build planning and explicitly unauthenticated record packaging.
pub mod build;

/// Bounded source archive verification, without filesystem or compilation.
pub mod build_source;

/// Source-bound v2 execution plans and portable encoding, without execution.
pub mod build_execution;

/// Finite Cargo configuration policy for verified source, without execution.
pub mod build_config;

/// Source materialization and retained input handles; no compiler execution.
#[cfg(windows)]
pub mod build_windows;
pub mod launch_environment;

/// Explicit segmented byte storage, without dispatch authority.
pub mod transfer_store;

#[cfg(windows)]
pub mod transfer_store_windows;

#[cfg(windows)]
pub mod transfer_endpoint;

pub mod transfer_reconstruct;
#[cfg(windows)]
pub mod transfer_reconstruct_windows;

#[cfg(windows)]
pub mod transfer_publication;

pub mod native_input_copy;
#[cfg(windows)]
pub mod native_input_copy_windows;

pub mod native_payload_copy;

#[cfg(windows)]
pub mod native_stage;

#[cfg(windows)]
pub mod published_origin;

#[cfg(windows)]
pub mod published_stage_endpoint;

#[cfg(windows)]
pub mod large_fetch;
