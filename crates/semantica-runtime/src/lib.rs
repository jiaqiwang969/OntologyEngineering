//! Generic Rust semantic capabilities supporting ontology-guided applications.
//! Engineering adoption, project truth, credentials and tool execution are injected above this layer.
pub mod books;
pub mod datalog;
pub mod datalog_port;
pub mod datalog_session;
pub mod dataset;
pub mod graph;
pub mod packages;
pub mod provenance;
pub mod rules;
pub mod runtime;
pub mod shacl;

pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const ENGINE_ID: &str = "semantica.rust-runtime";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Actual Cargo feature keys of this compiled dependency, not the caller's own
/// features or a request-controlled value. These do not grant runtime admission.
pub fn compiled_cargo_features() -> Vec<String> {
    include_str!(concat!(env!("OUT_DIR"), "/compiled-features.txt"))
        .lines()
        .map(str::to_owned)
        .collect()
}

pub fn sha256(bytes: impl AsRef<[u8]>) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

pub mod embeddings;
pub mod native_models;
pub mod providers;
pub mod refinery;
pub mod refinery_contracts;
pub mod refinery_history;
pub mod refinery_runner;
pub mod schema;

pub mod typed;
pub mod typed_relations8;

pub mod context;
mod context_native;
pub mod extraction;
pub mod reasoner;

pub mod book_runner;
pub mod release_validation;

pub mod execution_provenance;
pub mod rdflib_snapshot;

mod rdflib_literal_normalizers;
mod rdflib_xml_html;

pub mod refinery_engagement;
pub mod refinery_gate_algorithms;
pub mod refinery_promotion;
pub mod refinery_registry_algorithms;
pub mod refinery_subject;

/// Current promoted task/evidence observations; no Project adoption.
pub mod promoted_review;

/// Exact source PNG and fixed TSV content; no OCR/model or engineering authority.
pub mod page_content3;

/// Exact TSV source content; execution authority remains in the existing adapter.
pub mod page_content5;

/// Explorer migration algorithms; HTTP and deadline ownership remain pending.
pub mod explorer_sparql;

pub mod explorer_path;

pub mod explorer_path_count;

pub mod explorer_path_similarity;

pub mod explorer_path_bottleneck;

pub mod explorer_path_details;

pub mod explorer_read;

pub mod explorer_search_index;

mod explorer_search_unicode;

pub mod explorer_search;

pub mod explorer_decisions;

pub mod explorer_causal_distance;

pub mod explorer_mutation;

pub mod native_values;

pub mod temporal_consistency;
pub mod temporal_facts;
pub mod temporal_intervals;
pub mod temporal_paths;
pub mod temporal_patterns;
pub mod temporal_range;
pub mod temporal_snapshot;
pub mod temporal_version_records;

mod explorer_index_text;

#[cfg(test)]
use crate::sha256 as private_fixture_sha256;
#[cfg(test)]
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

pub mod temporal_version_diff;

pub mod version_storage_codec;
pub mod snapshot_checksum;
