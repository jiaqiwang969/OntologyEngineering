//! Exact bounded regular-file stdin replay for the existing ProcessTree.
//! A configured descriptor does not prove target consumption or semantics.
use crate::{FileIdentity, open_regular_file, sha256, write_new_bytes};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Stdio,
};

fn metadata(value: &fs::Metadata) -> Result<Value> {
    let mut result = json!({"bytes":value.len(),"regular":value.is_file(),
        "modified":format!("{:?}",value.modified()?),
        "readonly":value.permissions().readonly()});
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        result["unix"] = json!({"device":value.dev(),"inode":value.ino(),
            "mode":value.mode(),"uid":value.uid(),"gid":value.gid(),
            "links":value.nlink(),"changed_seconds":value.ctime(),
            "changed_nanoseconds":value.ctime_nsec()});
    }
    Ok(result)
}

fn read(path: &Path, expected: &FileIdentity, limit: usize) -> Result<(Vec<u8>, Value)> {
    ensure!(
        local_absolute_path(path),
        "external_stdin_path_not_canonical"
    );
    ensure!(
        (1..=256 * 1024 * 1024).contains(&limit) && expected.bytes <= limit as u64,
        "external_stdin_original_limit"
    );
    ensure!(
        expected.sha256.len() == 64
            && expected
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "external_stdin_digest"
    );
    let file = open_regular_file(path, limit as u64)?;
    let before = metadata(&file.metadata()?)?;
    let mut bytes = Vec::new();
    (&file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "external_stdin_original_limit_exceeded"
    );
    ensure!(
        bytes.len() as u64 == expected.bytes && sha256(&bytes) == expected.sha256,
        "external_stdin_exact_bytes_mismatch"
    );
    let after = metadata(&file.metadata()?)?;
    let reopened = open_regular_file(path, limit as u64)?;
    ensure!(
        before == after && metadata(&reopened.metadata()?)? == before,
        "external_stdin_source_changed_during_capture"
    );
    Ok((bytes, before))
}

// Check the supplied spelling before filesystem access; do not canonicalize
// away traversal. Windows canonicalize() returns a valid verbatim disk prefix.
// UNC and device namespaces remain outside this local input owner.
fn local_absolute_path(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        let mut parts = path.components();
        matches!(parts.next(), Some(Component::Prefix(p))
            if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
            && matches!(parts.next(), Some(Component::RootDir))
            && parts.all(|part| match part {
                Component::Normal(name) => {
                    oe_windows_controller::windows::validate_source_component(name).is_ok()
                }
                _ => false,
            })
    }
    #[cfg(not(windows))]
    {
        path.components()
            .all(|p| matches!(p, Component::RootDir | Component::Normal(_)))
    }
}

pub(super) struct CheckedInput {
    source: PathBuf,
    original: Value,
    expected: FileIdentity,
    limit: usize,
    version2: bool,
    bytes: Vec<u8>,
}
impl CheckedInput {
    pub(super) fn open(
        path: &Path,
        expected: &FileIdentity,
        limit: usize,
        version2: bool,
    ) -> Result<Self> {
        let (bytes, original) = read(path, expected, limit)?;
        Ok(Self {
            source: path.to_path_buf(),
            original,
            expected: expected.clone(),
            limit,
            version2,
            bytes,
        })
    }
    pub(super) fn snapshot(self, evidence: &Path) -> Result<PreparedInput> {
        let path = evidence.join("stdin");
        write_new_bytes(&path, &self.bytes)?;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions)?;
        let (_, original) = read(&path, &self.expected, self.limit)?;
        let file = open_regular_file(&path, self.limit as u64)?;
        ensure!(
            metadata(&file.metadata()?)? == original,
            "external_stdin_snapshot_changed_before_open"
        );
        Ok(PreparedInput {
            source: self,
            path,
            original,
            file,
        })
    }
}

pub(super) struct PreparedInput {
    source: CheckedInput,
    path: PathBuf,
    original: Value,
    file: fs::File,
}
impl PreparedInput {
    fn unchanged(&self) -> Result<()> {
        let (_, source) = read(
            &self.source.source,
            &self.source.expected,
            self.source.limit,
        )?;
        let (_, snapshot) = read(&self.path, &self.source.expected, self.source.limit)?;
        ensure!(
            source == self.source.original
                && snapshot == self.original
                && metadata(&self.file.metadata()?)? == self.original,
            "external_stdin_original_identity_changed"
        );
        Ok(())
    }
    pub(super) fn stdio(&self) -> Result<Stdio> {
        self.unchanged()?;
        Ok(Stdio::from(self.file.try_clone()?))
    }
    pub(super) fn observation(&self, spawned: bool) -> Value {
        let recheck = self.unchanged();
        json!({"schema":if self.source.version2 {"oe-core.process-stdin-file/2"} else {"oe-core.process-stdin-file/1"},
            "source":{"path":self.source.source,"identity":self.source.expected,
                "physical":self.source.original},
            "snapshot":{"path":"stdin","identity":self.source.expected,
                "physical":self.original},
            "transport":"explicit_private_regular_file_snapshot",
            "configured_for_spawn":true,"process_spawned":spawned,
            "unchanged":recheck.is_ok(),
            "recheck_error":recheck.err().map(|e|format!("{e:#}")),
            "original_byte_limit":self.source.limit,
            "target_consumption":"not_inferred","semantic_support":"not_inferred",
            "scope":"Same original ProcessTree, caller deadline, argv and stream limits; file snapshot is distinct from any original FIFO observation",
            "uncovered":"No live-producer or engineering authority; hostile transient changes between observations and hard filesystem deadlines are not certified"})
    }
}
