//! Durable adapter to the existing caller-selected IndustryOntologyRegistry.
//! Neutral semantics own documents/events. This adapter owns only private paths,
//! participating-writer locks and byte publication, never a parallel registry.
use anyhow::{Context, Result, ensure};
use oe_core::{
    AdvisoryFileLock, SkillRoot, create_new_private_dir, ensure_directory, no_symlinks,
    read_regular_bytes, rename_directory_exclusive, rename_regular_file, write_new_bytes,
};
use semantica_runtime::refinery::{RefineryEngine, RefineryStore};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const LIMIT: usize = 16 * 1024 * 1024;
static SERIAL: AtomicU64 = AtomicU64::new(0);
pub struct FileStore {
    root: PathBuf,
    anchor: PathBuf,
    lock: AdvisoryFileLock,
}
impl FileStore {
    pub fn open(
        skill: &SkillRoot,
        workspace: &Path,
        registry_id: &str,
        created_at: &str,
        create: bool,
    ) -> Result<Self> {
        semantica_runtime::refinery_contracts::opaque_identifier(&serde_json::json!(registry_id))?;
        semantica_runtime::refinery_contracts::timestamp(&serde_json::json!(created_at))?;
        let root = skill.private_path(workspace)?;
        let parent = root.parent().context("refinery_parent_missing")?;
        ensure_directory(parent)?;
        no_symlinks(&root)?;
        if !root.try_exists()? {
            ensure!(create, "refinery_workspace_missing");
            let name = root
                .file_name()
                .and_then(|v| v.to_str())
                .context("refinery_workspace_name_not_utf8")?;
            let init_lock = AdvisoryFileLock::acquire(
                &parent.join(format!(".{name}.semantica-refinery-init.lock")),
                true,
            )?;
            ensure!(!root.try_exists()?, "refinery_workspace_exists");
            let stage = parent.join(format!(
                ".{name}.semantica-refinery-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            create_new_private_dir(&stage)?;
            let initialized = (|| -> Result<()> {
                let store = Self::existing(stage.clone())?;
                RefineryEngine::initialize(&store, registry_id, created_at)?;
                store.check()?;
                init_lock.ensure_current()?;
                drop(store);
                rename_directory_exclusive(&stage, &root)?;
                sync_directory(parent)
            })();
            if let Err(error) = initialized {
                if stage.try_exists()? {
                    if let Err(cleanup) = remove_unpublished_stage(&stage) {
                        return Err(error.context(format!(
                            "refinery_initialization_cleanup_unverified:{cleanup:#}"
                        )));
                    }
                }
                return Err(error);
            }
        }
        let store = Self::existing(root)?;
        RefineryEngine::new(&store, registry_id)?;
        Ok(store)
    }
    fn existing(root: PathBuf) -> Result<Self> {
        no_symlinks(&root)?;
        ensure!(
            fs::symlink_metadata(&root)?.is_dir(),
            "refinery_workspace_not_directory"
        );
        let anchor = root.canonicalize()?;
        let lock = AdvisoryFileLock::acquire(&root.join(".refinery.lock"), true)?;
        Ok(Self { root, anchor, lock })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn check(&self) -> Result<()> {
        no_symlinks(&self.root)?;
        ensure!(
            fs::symlink_metadata(&self.root)?.is_dir() && self.root.canonicalize()? == self.anchor,
            "refinery_workspace_anchor_changed"
        );
        self.lock.ensure_current()
    }
    fn path(&self, relative: &str) -> Result<PathBuf> {
        self.check()?;
        crate::safe_relative(relative)?;
        let path = self.root.join(relative);
        no_symlinks(&path)?;
        Ok(path)
    }
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::File::open(path)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        anyhow::bail!("refinery_directory_fsync_platform_unverified");
    }
    Ok(())
}
fn remove_unpublished_stage(stage: &Path) -> Result<()> {
    no_symlinks(stage)?;
    ensure!(
        fs::symlink_metadata(stage)?.is_dir(),
        "refinery_stage_not_directory"
    );
    fn visit(path: &Path, count: &mut usize) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            *count += 1;
            ensure!(*count <= 8192, "refinery_stage_cleanup_budget");
            let metadata = fs::symlink_metadata(entry.path())?;
            ensure!(!metadata.file_type().is_symlink(), "refinery_stage_symlink");
            if metadata.is_dir() {
                visit(&entry.path(), count)?;
            } else {
                ensure!(metadata.is_file(), "refinery_stage_nonregular");
            }
        }
        Ok(())
    }
    visit(stage, &mut 0)?;
    no_symlinks(stage)?;
    fs::remove_dir_all(stage)?;
    sync_directory(stage.parent().context("refinery_stage_parent_missing")?)
}
impl RefineryStore for FileStore {
    fn read(&self, relative: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path(relative)?;
        match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
            Ok(m) => {
                ensure!(
                    m.is_file() && !m.file_type().is_symlink(),
                    "refinery_read_not_regular"
                );
                let bytes = read_regular_bytes(&path, LIMIT)?;
                self.check()?;
                Ok(Some(bytes))
            }
        }
    }
    fn write_new(&self, relative: &str, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() <= LIMIT, "refinery_write_budget");
        let path = self.path(relative)?;
        write_new_bytes(&path, bytes)?;
        sync_directory(path.parent().context("refinery_write_parent_missing")?)?;
        ensure!(
            self.read(relative)?.as_deref() == Some(bytes),
            "refinery_write_changed"
        );
        Ok(())
    }
    fn replace(&self, relative: &str, expected: &[u8], bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() <= LIMIT, "refinery_replace_budget");
        let path = self.path(relative)?;
        ensure!(
            self.read(relative)?.as_deref() == Some(expected),
            "refinery_compare_replace_changed"
        );
        let temp = path.with_file_name(format!(
            ".refinery-replace-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        write_new_bytes(&temp, bytes)?;
        self.check()?;
        ensure!(
            self.read(relative)?.as_deref() == Some(expected),
            "refinery_compare_replace_changed"
        );
        rename_regular_file(&temp, &path)?;
        sync_directory(path.parent().context("refinery_replace_parent_missing")?)?;
        ensure!(
            self.read(relative)?.as_deref() == Some(bytes),
            "refinery_replace_changed"
        );
        Ok(())
    }
    fn remove(&self, relative: &str, expected: &[u8]) -> Result<()> {
        let path = self.path(relative)?;
        ensure!(
            self.read(relative)?.as_deref() == Some(expected),
            "refinery_compare_remove_changed"
        );
        fs::remove_file(&path)?;
        sync_directory(path.parent().context("refinery_remove_parent_missing")?)?;
        self.check()
    }
    fn list(&self, relative: &str) -> Result<Vec<String>> {
        let path = self.path(relative)?;
        ensure!(
            fs::symlink_metadata(&path)?.is_dir(),
            "refinery_list_not_directory"
        );
        let mut names = Vec::new();
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let m = fs::symlink_metadata(entry.path())?;
            ensure!(
                m.is_file() && !m.file_type().is_symlink(),
                "refinery_list_nonregular_member"
            );
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("refinery_member_not_utf8"))?;
            names.push(name);
            ensure!(names.len() <= 1024, "refinery_member_budget");
        }
        names.sort();
        self.check()?;
        Ok(names)
    }
    fn create_dir(&self, relative: &str) -> Result<()> {
        let path = self.path(relative)?;
        create_new_private_dir(&path)?;
        sync_directory(path.parent().context("refinery_directory_parent_missing")?)?;
        self.check()
    }
    fn ensure_dir(&self, relative: &str) -> Result<()> {
        let path = self.path(relative)?;
        ensure_directory(&path)?;
        self.check()
    }
    fn publish_dir(&self, stage: &str, destination: &str) -> Result<()> {
        let stage = self.path(stage)?;
        let destination = self.path(destination)?;
        rename_directory_exclusive(&stage, &destination)?;
        sync_directory(
            destination
                .parent()
                .context("refinery_publish_parent_missing")?,
        )?;
        self.check()
    }
    fn discard_stage(&self, stage: &str) -> Result<()> {
        ensure!(
            stage.starts_with("refinements/.staging-") && stage.split('/').count() == 2,
            "refinery_not_owned_stage"
        );
        let path = self.path(stage)?;
        if path.try_exists()? {
            remove_unpublished_stage(&path)?;
        }
        self.check()
    }
}
