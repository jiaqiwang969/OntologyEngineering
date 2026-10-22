//! Rust port of Semantica's `semantica.provenance` package (see
//! `semantica-rs/scope/sem-provenance.json`): audit-grade, W3C PROV-O compliant provenance
//! tracking for high-stakes domains (blue finance, healthcare, legal, pharma).
//!
//! The crate root re-exports what `semantica/provenance/__init__.py` exports (`__all__`), so
//! `from semantica.provenance import X` becomes `use sem_provenance::X`. Module files mirror
//! the original: [`schemas`], [`storage`], [`manager`], [`integrity`], [`bridge_axiom`].
//! [`clock`], [`isoformat`] and [`rdf`] hold the Python/rdflib behaviour the port needs
//! (`datetime.utcnow`, `datetime.fromisoformat`, rdflib serialisation).
//!
//! The usage guide (the port of `provenance_usage.md`) follows.
//!
#![doc = include_str!("../provenance_usage.md")]

pub mod bridge_axiom;
pub mod clock;
pub mod integrity;
pub mod isoformat;
pub mod manager;
pub mod rdf;
pub mod schemas;
pub mod storage;

// Core schemas
pub use schemas::{ActivityRecord, AgentRecord, Invalidation, ProvenanceEntry, SourceReference};

// Storage backends
pub use storage::{InMemoryStorage, ProvenanceStorage, SQLiteStorage};

// Manager
pub use manager::{ProvenanceManager, default_storage_path};

// Utilities
pub use integrity::{compute_checksum, verify_checksum};

// Rust-side companions of the exported API (keyword-argument structs, transaction handles).
pub use manager::{
    AuditLog, DEFAULT_BASE_URI, DefaultStoragePathGuard, ProvenanceManagerOptions,
    TrackChunkOptions, TrackEntityOptions, TrackKwargs,
};
pub use schemas::PropertySource;
pub use storage::{ProvenanceStorageExt, StorageConnection};

/// `__all__` of `semantica.provenance`.
pub const ALL: [&str; 12] = [
    "ProvenanceEntry",
    "SourceReference",
    "AgentRecord",
    "ActivityRecord",
    "Invalidation",
    "ProvenanceStorage",
    "InMemoryStorage",
    "SQLiteStorage",
    "ProvenanceManager",
    "default_storage_path",
    "compute_checksum",
    "verify_checksum",
];

/// `__version__`.
pub const VERSION: &str = "1.0.0";
/// `__author__`.
pub const AUTHOR: &str = "Semantica Team";
/// `__description__`.
pub const DESCRIPTION: &str = "Audit-Grade Provenance Tracking for Semantica";
