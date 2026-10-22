//! Source installation, retrieval and indexing share the knowledge source entry.
//! Book authoring boundaries retain the established Semantica source APIs.
pub use oe_knowledge::source_library::{FetchOptions, fetch, import, list, register};
pub use oe_semantic::{book_sources, release_sources};
