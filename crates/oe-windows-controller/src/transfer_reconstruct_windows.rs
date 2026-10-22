//! Fresh unpublished output tree, protected by the existing Windows writer lock.
use crate::{
    adapter,
    transfer_reconstruct::{MAX_RECONSTRUCTED_FILE_BYTES, Target},
    windows::{PinnedDirectory, PinnedFile, WriterLock},
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{BlobIdentity, FileEntry, RootIdentity};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    io::Write,
};
pub struct PinnedTarget<'a, 'root> {
    directory: &'a PinnedDirectory<'root>,
    lock: &'a WriterLock<'root>,
    pins: BTreeMap<String, PinnedFile>,
}
impl<'a, 'root> PinnedTarget<'a, 'root> {
    pub fn new(directory: &'a PinnedDirectory<'root>, lock: &'a WriterLock<'root>) -> Result<Self> {
        ensure!(
            directory.path().starts_with(lock.root().path())
                && directory.path() != lock.root().path(),
            "reconstruction_directory_outside_root"
        );
        let out = Self {
            directory,
            lock,
            pins: BTreeMap::new(),
        };
        out.scope()?;
        Ok(out)
    }
    fn scope(&self) -> Result<()> {
        self.lock.verify()?;
        self.directory.verify()?;
        Ok(())
    }
    fn parent(&self, path: &str, create: bool) -> Result<(PinnedDirectory<'root>, String)> {
        self.scope()?;
        let mut pieces = path.split('/').collect::<Vec<_>>();
        let leaf = pieces
            .pop()
            .context("reconstruction_path_empty")?
            .to_owned();
        // Opening the target via the stable root would only support one level. Keep
        // its existing ancestry pins by descending directly from the supplied target.
        let mut owned: Option<PinnedDirectory<'root>> = None;
        for component in pieces {
            let current = owned.as_ref().unwrap_or(self.directory);
            let next = match current.directory(OsStr::new(component)) {
                Ok(d) => d,
                Err(e) if create && matches!(e.raw_os_error(), Some(2 | 3)) => {
                    current.create_directory_new(OsStr::new(component))?
                }
                Err(e) => return Err(e.into()),
            };
            owned = Some(next);
        }
        // Files at the target root use a retained view of that directory.
        let directory = match owned {
            Some(d) => d,
            None => self.directory.retained_view()?,
        };
        Ok((directory, leaf))
    }
}
impl PinnedTarget<'_, '_> {
    pub fn owner_root(&self) -> Result<RootIdentity> {
        self.scope()?;
        Ok(adapter::root_identity(self.lock.root()))
    }
    pub fn stream_pinned(
        &mut self,
        file: &FileEntry,
        max_segment: u64,
        out: &mut dyn Write,
    ) -> Result<()> {
        use std::io::Read;
        self.scope()?;
        ensure!(max_segment > 0, "native_input_empty_segment");
        let pin = self
            .pins
            .get_mut(file.path.as_str())
            .context("native_input_unpinned_source")?;
        pin.verify()?;
        let mut buffer = vec![0u8; max_segment.min(65536) as usize];
        let reader = pin.reader()?;
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            out.write_all(&buffer[..n])?;
        }
        pin.verify()?;
        self.scope()
    }
}
impl Target for PinnedTarget<'_, '_> {
    fn identity(&self) -> Result<RootIdentity> {
        self.scope()?;
        let mut id = adapter::root_identity(self.lock.root());
        id.volume_id = self.directory.identity().volume_id.clone();
        id.directory_id = self.directory.identity().file_id.clone();
        Ok(id)
    }
    fn verify_empty(&self) -> Result<()> {
        self.scope()?;
        ensure!(
            std::fs::read_dir(self.directory.path())?.next().is_none(),
            "reconstruction_target_not_empty"
        );
        Ok(())
    }
    fn write_new<F: FnOnce(&mut dyn Write) -> Result<()>>(
        &mut self,
        file: &FileEntry,
        write: F,
    ) -> Result<()> {
        let (parent, leaf) = self.parent(file.path.as_str(), true)?;
        parent.create_stream_new(OsStr::new(&leaf), file.identity.bytes, |out| {
            write(out).map_err(std::io::Error::other)
        })?;
        self.scope()
    }
    fn readback(&mut self, file: &FileEntry) -> Result<BlobIdentity> {
        ensure!(
            !self.pins.contains_key(file.path.as_str()),
            "reconstruction_duplicate_readback"
        );
        let (parent, leaf) = self.parent(file.path.as_str(), false)?;
        let mut pin = parent
            .pin_file(OsStr::new(&leaf), MAX_RECONSTRUCTED_FILE_BYTES)?
            .context("reconstruction_output_missing")?;
        let identity = adapter::file_identity(&mut pin)?;
        pin.verify()?;
        self.pins.insert(file.path.as_str().into(), pin);
        self.scope()?;
        Ok(identity)
    }
    fn verify_complete(&self, files: &[FileEntry]) -> Result<()> {
        self.verify_with_control(files, &[])
    }
}
impl PinnedTarget<'_, '_> {
    pub fn verify_with_control(&self, files: &[FileEntry], control: &[FileEntry]) -> Result<()> {
        self.scope()?;
        let pinned: BTreeSet<String> = files.iter().map(|f| f.path.as_str().to_owned()).collect();
        ensure!(
            self.pins.keys().cloned().collect::<BTreeSet<_>>() == pinned,
            "reconstruction_readback_coverage"
        );
        let mut wanted = pinned;
        for entry in control {
            ensure!(
                !entry.path.as_str().contains('/') && wanted.insert(entry.path.as_str().into()),
                "control_namespace_alias"
            );
        }
        let mut directories = BTreeSet::new();
        for path in &wanted {
            let mut prefix = String::new();
            let parts: Vec<_> = path.split('/').collect();
            for part in &parts[..parts.len() - 1] {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                directories.insert(prefix.clone());
            }
        }
        fn walk(
            dir: &PinnedDirectory<'_>,
            prefix: &str,
            files: &BTreeSet<String>,
            directories: &BTreeSet<String>,
            seen: &mut BTreeSet<String>,
        ) -> Result<()> {
            dir.verify()?;
            for entry in std::fs::read_dir(dir.path())? {
                let entry = entry?;
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("reconstruction_nonunicode_entry"))?;
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                ensure!(seen.insert(path.clone()), "reconstruction_duplicate_entry");
                if directories.contains(&path) {
                    walk(
                        &dir.directory(OsStr::new(&name))?,
                        &path,
                        files,
                        directories,
                        seen,
                    )?;
                } else {
                    ensure!(
                        files.contains(&path) && entry.file_type()?.is_file(),
                        "reconstruction_unexpected_entry"
                    );
                }
            }
            dir.verify()?;
            Ok(())
        }
        let mut seen = BTreeSet::new();
        walk(self.directory, "", &wanted, &directories, &mut seen)?;
        ensure!(
            seen == wanted.union(&directories).cloned().collect(),
            "reconstruction_namespace_differs"
        );
        for pin in self.pins.values() {
            pin.verify()?;
        }
        self.scope()
    }
}
