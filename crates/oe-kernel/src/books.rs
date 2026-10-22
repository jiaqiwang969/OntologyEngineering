//! Engineering-facing facade. Source transport and publication stay in the
//! single oe-semantic adapter; generic book algorithms live in Semantica Rust.
pub use oe_semantic::books::rust::{
    asset_text, chapter_asset_text, engrave, execute, migration_map, resolve_successor,
    validate_chapter_registry, verify_sources,
};
pub use oe_semantic::books::rust::{
    execute_chapter_query, list_chapter_packages, list_domain_packages, load_chapter_package,
    load_domain_package, read_chapter_manifest, read_chapter_policy, read_domain_manifest,
};
pub use oe_semantic::books::{BookRequest, EngraveOptions};
