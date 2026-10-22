//! Book execution reuses the owned compiler inventory, adding its actual
//! specification/templates/contracts. Build and runtime use the same member set.
#[path = "provenance.rs"]
mod inventory;
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

pub fn inputs(root: &Path) -> io::Result<BTreeSet<PathBuf>> {
    let mut inputs: BTreeSet<_> = inventory::inputs(root)?.into_iter().collect();
    for relative in [
        "specs/engineering.toml",
        "generated/contracts.rs",
        "generated/semantic-contracts.rs",
        "generated/generation.json",
    ] {
        inputs.insert(relative.into());
    }
    fn templates(root: &Path, directory: &Path, inputs: &mut BTreeSet<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(io::Error::other("book_compile_input_symlink"));
            }
            if metadata.is_dir() {
                templates(root, &path, inputs)?;
            } else {
                if !metadata.is_file() {
                    return Err(io::Error::other("book_compile_input_not_file"));
                }
                inputs.insert(
                    path.strip_prefix(root)
                        .map_err(io::Error::other)?
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
    templates(root, &root.join("templates"), &mut inputs)?;
    Ok(inputs)
}
