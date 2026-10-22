//! Digests of the Semantica port's migration ledger and recorded evidence directories.
//! Shared by the oe-design build script (which compiles them in as pins) and the runtime
//! audit, so both compute exactly the same identity.
use sha2::{Digest, Sha256};
use std::{fs, io, path::Path};

/// Ledger directory relative to the skill root.
pub const LEDGER_DIR: &str = "semantica-rs/ledger";
/// Recorded evidence directory relative to the skill root.
pub const EVIDENCE_DIR: &str = "semantica-rs/evidence";
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;

/// SHA-256 over the sorted lines `<name>\0<sha256 of file>\n` of the regular files directly
/// inside `dir`. `None` when the directory does not exist. Subdirectories, symlinks, oversized
/// files and non-UTF-8 names are refused rather than skipped.
pub fn directory_digest(dir: &Path) -> io::Result<Option<String>> {
    match fs::symlink_metadata(dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
        Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
            return Err(io::Error::other("migration_pin_directory_not_plain"));
        }
        Ok(_) => {}
    }
    let mut rows = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if !kind.is_file() || kind.is_symlink() {
            return Err(io::Error::other("migration_pin_non_regular_entry"));
        }
        if entry.metadata()?.len() > MAX_FILE_BYTES {
            return Err(io::Error::other("migration_pin_file_too_large"));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| io::Error::other("migration_pin_name_not_utf8"))?;
        if name.contains(['\0', '\n', '/']) {
            return Err(io::Error::other("migration_pin_unsafe_name"));
        }
        let bytes = fs::read(entry.path())?;
        rows.push(format!("{name}\0{:x}\n", Sha256::digest(&bytes)));
        if rows.len() > MAX_ENTRIES {
            return Err(io::Error::other("migration_pin_too_many_entries"));
        }
    }
    rows.sort();
    Ok(Some(format!(
        "{:x}",
        Sha256::digest(rows.concat().as_bytes())
    )))
}
