//! Execute the existing frozen sort schedule receiver through a workspace
//! consumer. The sorter and its original expectations remain single-owned.
mod source_sort {
    pub use spareval::{SourceSortError, source_sort_indices};
}
#[path = "../../../vendor/spareval/src/eval/source_sort_receiving.rs"]
mod original_receiving;
