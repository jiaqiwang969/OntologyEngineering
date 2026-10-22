//! Rust port of Semantica's `semantica.change_management` package (see
//! `semantica-rs/scope/sem-change.json`), plus `semantica.ontology.namespace_manager`, which
//! lives here to break the original's `ontology` <-> `change_management` import cycle.
//!
//! Enhanced change management:
//!
//! - persistent version storage ([`VersionStorage`]: [`InMemoryVersionStorage`],
//!   [`SQLiteVersionStorage`] with the original on-disk schema);
//! - detailed change tracking and diffs ([`TemporalVersionManager`] for knowledge graphs,
//!   [`OntologyVersionManager`] and [`VersionManager`] for ontologies);
//! - standardized change metadata and audit trails ([`ChangeLogEntry`],
//!   [`change_log::MutationRecord`]) and impact analysis ([`change_log::generate_change_report`]);
//! - data integrity with SHA-256 checksums ([`compute_checksum`], [`verify_checksum`]) that are
//!   byte-identical to the original's.
//!
//! The crate root re-exports `__all__` of `semantica/change_management/__init__.py`, so
//! `from semantica.change_management import X` becomes `use sem_change::X`.

pub mod change_log;
pub mod managers;
pub mod namespace_manager;
pub mod ontology_version_manager;
pub mod version_storage;

mod pycompat;

// Change metadata
pub use change_log::ChangeLogEntry;

// Storage backends and integrity utilities
pub use version_storage::{
    InMemoryVersionStorage, SQLiteVersionStorage, VersionStorage, compute_checksum, verify_checksum,
};

// Version managers
pub use managers::{BaseVersionManager, OntologyVersionManager, TemporalVersionManager};

// Ontology version management
pub use ontology_version_manager::{OntologyVersion, VersionManager};

/// `__version__` of the original package.
pub const VERSION: &str = "1.0.0";
/// `__author__` of the original package.
pub const AUTHOR: &str = "Semantica Team";
/// `__description__` of the original package.
pub const DESCRIPTION: &str = "Enhanced Change Management for Semantica";
