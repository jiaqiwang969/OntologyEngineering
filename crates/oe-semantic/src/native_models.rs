//! Studio asset transport for the single neutral native model algorithm.
use anyhow::{Result, ensure};
pub use semantica_runtime::books::SourceProvider;
pub use semantica_runtime::native_models::*;
use std::path::{Path, PathBuf};

pub struct NativeModelSources {
    base: PathBuf,
}
impl NativeModelSources {
    pub fn new(base: &Path) -> Result<Self> {
        oe_core::no_symlinks(base)?;
        ensure!(base.is_dir(), "native_model_source_directory_unavailable");
        Ok(Self {
            base: base.to_path_buf(),
        })
    }
}
impl SourceProvider for NativeModelSources {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        semantica_runtime::books::relative(id)?;
        oe_core::read_regular_bytes(&self.base.join(id), 2 * 1024 * 1024 * 1024)
    }
}
