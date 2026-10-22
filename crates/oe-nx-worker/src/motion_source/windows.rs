//! Owned assembly copy. Retain original read pins and destination directory
//! pins until native work, save and receipt capture have all finished.
use anyhow::{Context, Result, ensure};
use oe_contracts::NxAssemblySourceClosure;
use oe_windows_controller::{
    adapter,
    windows::{PinnedDirectory, PinnedFile},
};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub struct OwnedAssembly<'root> {
    directories: BTreeMap<String, PinnedDirectory<'root>>,
    sources: Vec<PinnedFile>,
    root_part: String,
    expected_files: Vec<oe_contracts::NxAssemblySourceFile>,
}
impl OwnedAssembly<'_> {
    pub fn verify_copy_files(&self) -> Result<()> {
        self.verify()?;
        let mut files = Vec::new();
        for expected in &self.expected_files {
            let (parent, leaf) = expected
                .path
                .rsplit_once('/')
                .unwrap_or(("", &expected.path));
            let dir = self
                .directories
                .get(parent)
                .context("motion_copy_directory_missing")?;
            let mut pin = dir
                .pin_file(OsStr::new(leaf), expected.bytes)?
                .context("motion_copy_file_missing")?;
            let actual = adapter::file_identity(&mut pin)?;
            ensure!(
                actual.bytes == expected.bytes && actual.sha256.as_str() == expected.sha256,
                "motion_copy_readonly_bytes_changed"
            );
            files.push(pin);
        }
        for file in &files {
            file.verify()?;
        }
        self.verify()
    }
    pub fn namespace(&self) -> &Path {
        self.directories[""].path()
    }
    pub fn root_part(&self) -> PathBuf {
        self.namespace().join(&self.root_part)
    }
    pub fn verify(&self) -> Result<()> {
        for file in &self.sources {
            file.verify()?;
        }
        for directory in self.directories.values() {
            directory.verify()?;
        }
        Ok(())
    }
}
/// Source adapter must select only already-declared job inputs. All bytes are
/// checked before any output namespace is created. Partial copies remain
/// private and cannot be retried onto existing output files.
pub fn copy_owned_source<'root>(
    out: &PinnedDirectory<'root>,
    closure: &NxAssemblySourceClosure,
    source: impl FnMut(&str, u64) -> Result<PinnedFile>,
) -> Result<OwnedAssembly<'root>> {
    super::validate_source_closure(closure)?;
    copy_owned_files(out, &closure.files, &closure.root_part, source)
}
/// Creates the same pinned private namespace, with an absent new assembly root.
pub fn copy_owned_build_source<'root>(
    out: &PinnedDirectory<'root>,
    intent: &oe_contracts::NxAssemblyBuildIntent,
    source: impl FnMut(&str, u64) -> Result<PinnedFile>,
) -> Result<OwnedAssembly<'root>> {
    crate::geometry::source_assembly::validate_source_set(intent)?;
    copy_owned_files(out, &intent.files, &intent.root_part, source)
}
fn copy_owned_files<'root>(
    out: &PinnedDirectory<'root>,
    files: &[oe_contracts::NxAssemblySourceFile],
    root_part: &str,
    mut source: impl FnMut(&str, u64) -> Result<PinnedFile>,
) -> Result<OwnedAssembly<'root>> {
    let directories = super::source_file_directories(files)?;
    let mut sources = Vec::with_capacity(files.len());
    for file in files {
        let mut pin = source(&file.path, file.bytes)?;
        let actual = adapter::file_identity(&mut pin)?;
        ensure!(
            actual.bytes == file.bytes && actual.sha256.as_str() == file.sha256,
            "motion_copy_source_identity_differs"
        );
        sources.push(pin);
    }
    let root = out.create_directory_new(OsStr::new("motion-assembly"))?;
    let mut pins = BTreeMap::from([(String::new(), root)]);
    for name in directories {
        let (parent, leaf) = name.rsplit_once('/').unwrap_or(("", &name));
        let child = pins
            .get(parent)
            .context("motion_copy_parent_directory_missing")?
            .create_directory_new(OsStr::new(leaf))?;
        pins.insert(name, child);
    }
    for (expected, source) in files.iter().zip(&mut sources) {
        let (parent, leaf) = expected
            .path
            .rsplit_once('/')
            .unwrap_or(("", &expected.path));
        let directory = pins
            .get(parent)
            .context("motion_copy_target_directory_missing")?;
        let mut output = directory.create_log_new(OsStr::new(leaf))?;
        let reader = source.reader()?;
        let mut count = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            ensure!(
                n as u64 <= expected.bytes - count,
                "motion_copy_source_grew"
            );
            output.write_all(&buffer[..n])?;
            count += n as u64;
        }
        output.sync_all()?;
        drop(output);
        source.verify()?;
        ensure!(count == expected.bytes, "motion_copy_source_size_differs");
        let mut copied = directory
            .pin_file(OsStr::new(leaf), expected.bytes)?
            .context("motion_copy_output_missing")?;
        let actual = adapter::file_identity(&mut copied)?;
        ensure!(
            actual.bytes == expected.bytes && actual.sha256.as_str() == expected.sha256,
            "motion_copy_readback_differs"
        );
        // Drop only the target read pin: NX must be able to write this private
        // file. Original input pins and every directory remain held.
    }
    let owned = OwnedAssembly {
        directories: pins,
        sources,
        root_part: root_part.to_owned(),
        expected_files: files.to_vec(),
    };
    owned.verify()?;
    Ok(owned)
}
