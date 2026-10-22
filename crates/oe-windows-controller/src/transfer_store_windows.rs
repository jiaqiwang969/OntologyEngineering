//! Retain the existing Windows root writer lock and every read file handle.
//! Caller supplies a dedicated transfer directory; this does not admit a job.
use super::{
    adapter,
    transfer_store::Store,
    windows::{PinnedDirectory, PinnedFile, WriterLock},
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol::{MAX_PAYLOAD_BYTES, RootIdentity};
use std::{collections::BTreeMap, ffi::OsStr};
pub struct PinnedStore<'a, 'root> {
    directory: &'a PinnedDirectory<'root>,
    lock: &'a WriterLock<'root>,
    pins: BTreeMap<String, PinnedFile>,
}
impl<'a, 'root> PinnedStore<'a, 'root> {
    pub fn new(directory: &'a PinnedDirectory<'root>, lock: &'a WriterLock<'root>) -> Result<Self> {
        ensure!(
            directory.path().starts_with(lock.root().path())
                && directory.path() != lock.root().path(),
            "transfer_store_directory_outside_root"
        );
        let out = Self {
            directory,
            lock,
            pins: BTreeMap::new(),
        };
        out.verify()?;
        Ok(out)
    }
}
impl Store for PinnedStore<'_, '_> {
    fn root(&self) -> Result<RootIdentity> {
        self.verify()?;
        Ok(adapter::root_identity(self.lock.root()))
    }
    fn names(&self, limit: usize) -> Result<Vec<String>> {
        self.verify()?;
        let mut names = Vec::new();
        for entry in std::fs::read_dir(self.directory.path())? {
            ensure!(names.len() < limit, "transfer_store_directory_budget");
            let e = entry?;
            names.push(
                e.file_name()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("transfer_store_nonunicode_name"))?,
            );
        }
        names.sort();
        self.verify()?;
        Ok(names)
    }
    fn read(&mut self, name: &str, limit: u64) -> Result<Vec<u8>> {
        self.verify_scope()?;
        if !self.pins.contains_key(name) {
            let pin = self
                .directory
                .pin_file(OsStr::new(name), limit)?
                .context("transfer_store_file_missing")?;
            self.pins.insert(name.into(), pin);
        }
        let pin = self.pins.get_mut(name).unwrap();
        let raw = pin.read_all(limit)?;
        pin.verify()?;
        Ok(raw)
    }
    fn create_sync(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        self.verify_scope()?;
        self.directory
            .create_file_new(OsStr::new(name), bytes, MAX_PAYLOAD_BYTES)?;
        self.verify_scope()
    }
    fn verify_scope(&self) -> Result<()> {
        self.lock.verify()?;
        self.directory.verify()?;
        Ok(())
    }
    fn verify(&self) -> Result<()> {
        self.verify_scope()?;
        for pin in self.pins.values() {
            pin.verify()?;
        }
        Ok(())
    }
}
