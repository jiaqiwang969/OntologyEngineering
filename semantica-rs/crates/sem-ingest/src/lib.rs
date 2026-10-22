//! Rust port of Semantica's `ingest` scope (see semantica-rs/scope/sem-ingest.json).

pub mod api_ingestor;
pub mod config;
pub mod feed_ingestor;
pub mod file_ingestor;
pub mod mcp_client;
pub mod mcp_ingestor;
pub mod methods;
pub mod public_api_ingestor;
pub mod registry;
pub mod repo_ingestor;
pub mod ssrf;
pub mod stream_ingestor;
pub mod support;
pub mod web_ingestor;
