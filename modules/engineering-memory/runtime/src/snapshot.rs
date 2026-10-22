//! Fixed historical versions. Native clones avoid rereading a huge mutable prefix.
//! Metadata guards cover the local private snapshot; selected messages still use SHA256.
use crate::contracts::{CaptureRequest, SourceAnchor};
use crate::store::{Store, id, private_dir, read_json, sha, shape, unique, verify_sources};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::{self, File, Metadata};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    device: u64,
    inode: u64,
    length: u64,
    modified_seconds: i64,
    modified_ns: i64,
    changed_seconds: i64,
    changed_ns: i64,
    mode: u32,
}

impl FileIdentity {
    fn from_metadata(metadata: &Metadata) -> Result<Self> {
        ensure!(metadata.is_file(), "capture_source_not_regular");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(Self {
                device: metadata.dev(),
                inode: metadata.ino(),
                length: metadata.len(),
                modified_seconds: metadata.mtime(),
                modified_ns: metadata.mtime_nsec(),
                changed_seconds: metadata.ctime(),
                changed_ns: metadata.ctime_nsec(),
                mode: metadata.mode(),
            })
        }
        #[cfg(not(unix))]
        anyhow::bail!("native_snapshot_platform_unsupported")
    }

    fn at(path: &Path) -> Result<Self> {
        // symlink_metadata rejects a replaced pathname even when its target has matching bytes.
        Self::from_metadata(&fs::symlink_metadata(path)?)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub schema: String,
    pub request: CaptureRequest,
    pub original_path: String,
    pub original_identity: FileIdentity,
    pub archive_path: String,
    pub snapshot_path: String,
    pub snapshot_identity: FileIdentity,
    pub complete_end: u64,
    pub captured_at: String,
    pub mechanism: String,
    pub receipt_sha256: String,
}

impl Capture {
    fn digest(&self) -> Result<String> {
        let mut copy = self.clone();
        copy.receipt_sha256.clear();
        Ok(sha(serde_json::to_vec(&copy)?))
    }

    pub(crate) fn validate(&self) -> Result<()> {
        shape(&self.request)?;
        ensure!(
            self.schema == "engineering-memory.capture/1"
                && self.mechanism == "macos_fclonefileat"
                && self.receipt_sha256 == self.digest()?
                && Path::new(&self.snapshot_path).is_absolute()
                && Path::new(&self.snapshot_path).starts_with(&self.archive_path)
                && self.snapshot_identity.length == self.original_identity.length
                && self.snapshot_identity.length <= self.request.max_logical_bytes
                && self.request.registration.start_byte <= self.complete_end
                && self.complete_end <= self.snapshot_identity.length
                && self.snapshot_identity.mode & 0o7777 == 0o400,
            "capture_receipt_invalid"
        );
        Ok(())
    }

    pub fn verify(&self) -> Result<()> {
        self.validate()?;
        ensure!(
            FileIdentity::at(Path::new(&self.snapshot_path))? == self.snapshot_identity,
            "snapshot_changed"
        );
        Ok(())
    }

    pub fn provenance(&self) -> Value {
        json!({"capture":self,"original_current_state":"not_asserted",
            "whole_file_sha256":null,"integrity":"native fixed clone; metadata guard plus selected-message SHA256",
            "whole_history_reviewed":false})
    }
}

#[cfg(target_os = "macos")]
fn clone_native(source: &File, directory: &File) -> Result<()> {
    rustix::fs::fclonefileat(
        source,
        directory,
        "source.jsonl",
        rustix::fs::CloneFlags::NOOWNERCOPY,
    )
    .context("native_clone_failed_no_copy_fallback")?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn clone_native(_source: &File, _directory: &File) -> Result<()> {
    anyhow::bail!("native_snapshot_platform_unsupported")
}

impl Store {
    /// The capture receipt commits before the source cursor. A retry can finish publication.
    pub fn capture_source(&self, request: &CaptureRequest) -> Result<Value> {
        shape(request)?;
        id(&request.registration.id)?;
        unique(&request.registration.conditions, "condition")?;
        ensure!(
            Path::new(&request.registration.path).is_absolute()
                && Path::new(&request.archive_root).is_absolute()
                && !Path::new(&request.archive_root)
                    .components()
                    .any(|p| p == Component::ParentDir)
                && !Path::new(&request.registration.path)
                    .components()
                    .any(|p| p == Component::ParentDir)
                && request.max_logical_bytes > 0,
            "capture_paths_or_budget_invalid"
        );
        let _lock = self.lock()?;
        ensure!(
            !self
                .directory
                .join("source-aliases")
                .join(format!("{}.json", request.registration.id))
                .exists(),
            "source_registration_conflict"
        );
        let receipt_file = self
            .directory
            .join("captures")
            .join(format!("{}.json", request.registration.id));
        let capture = if receipt_file.exists() {
            let capture: Capture = read_json(&receipt_file)?;
            ensure!(capture.request == *request, "capture_request_conflict");
            capture.verify()?;
            capture
        } else {
            ensure!(
                !self
                    .directory
                    .join("sources")
                    .join(format!("{}.json", request.registration.id))
                    .exists(),
                "source_registration_conflict"
            );
            let original = PathBuf::from(&request.registration.path);
            FileIdentity::at(&original)?;
            let original = original.canonicalize()?;
            let root = PathBuf::from(&request.archive_root);
            let home = self.directory.canonicalize()?;
            ensure!(
                !original.starts_with(&root)
                    && !home.starts_with(&root)
                    && !root.starts_with(&home),
                "archive_directory_overlap"
            );
            // Check canonical overlap before changing permissions on an existing directory.
            if root.exists() {
                ensure!(
                    !fs::symlink_metadata(&root)?.file_type().is_symlink(),
                    "archive_directory_symlink"
                );
                let real = root.canonicalize()?;
                ensure!(
                    !original.starts_with(&real)
                        && !home.starts_with(&real)
                        && !real.starts_with(&home),
                    "archive_directory_overlap"
                );
            }
            let source = File::open(&original)?;
            let before = FileIdentity::from_metadata(&source.metadata()?)?;
            ensure!(
                FileIdentity::at(&original)? == before,
                "capture_source_replaced"
            );
            ensure!(
                before.length <= request.max_logical_bytes,
                "capture_logical_byte_budget"
            );
            private_dir(&root)?;
            let root = root.canonicalize()?;
            ensure!(
                !original.starts_with(&root)
                    && !home.starts_with(&root)
                    && !root.starts_with(&home),
                "archive_directory_overlap"
            );
            let attempt = root.join(format!(
                "{}-{}-{}",
                request.registration.id,
                std::process::id(),
                chrono::Utc::now()
                    .timestamp_nanos_opt()
                    .context("capture_clock")?
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&attempt)?;
            let directory = File::open(&attempt)?;
            clone_native(&source, &directory)?;
            let snapshot = attempt.join("source.jsonl");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&snapshot, fs::Permissions::from_mode(0o400))?;
            }
            File::open(&snapshot)?.sync_all()?;
            directory.sync_all()?;
            File::open(&root)?.sync_all()?;
            ensure!(
                FileIdentity::from_metadata(&source.metadata()?)? == before
                    && FileIdentity::at(&original)? == before,
                "capture_source_changed"
            );
            let identity = FileIdentity::at(&snapshot)?;
            ensure!(identity.length == before.length, "capture_length_changed");
            let complete_end = crate::workflow::boundary(&snapshot, identity.length)?;
            crate::workflow::check_start(&snapshot, request.registration.start_byte, complete_end)?;
            let mut capture = Capture {
                schema: "engineering-memory.capture/1".into(),
                request: request.clone(),
                original_path: original.to_string_lossy().into_owned(),
                original_identity: before,
                archive_path: root.to_string_lossy().into_owned(),
                snapshot_path: snapshot.to_string_lossy().into_owned(),
                snapshot_identity: identity,
                complete_end,
                captured_at: chrono::Utc::now().to_rfc3339(),
                mechanism: "macos_fclonefileat".into(),
                receipt_sha256: String::new(),
            };
            capture.receipt_sha256 = capture.digest()?;
            capture.verify()?;
            self.immutable("captures", &request.registration.id, &capture)?;
            capture
        };
        self.immutable("source-captures", &sha(&capture.snapshot_path), &capture)?;
        let effect = self.register_capture(&capture)?;
        Ok(
            json!({"capture":capture,"effect":effect,"verified_prefix_bytes":0,
            "boundary_probe_limit":crate::store::MAX_BYTES,"whole_file_sha256":null,
            "original_current_state":"not_asserted","curated":false}),
        )
    }

    pub(crate) fn source_captures(&self, sources: &[SourceAnchor]) -> Result<Vec<Capture>> {
        ensure!(sources.len() <= 128, "source_count_budget");
        let mut captures = Vec::new();
        let mut paths = BTreeSet::new();
        for source in sources {
            if !paths.insert(&source.source_path) {
                continue;
            }
            let index = self
                .directory
                .join("source-captures")
                .join(format!("{}.json", sha(&source.source_path)));
            if index.exists() {
                let capture: Capture = read_json(&index)?;
                capture.validate()?;
                ensure!(
                    capture.snapshot_path == source.source_path,
                    "capture_source_index_mismatch"
                );
                captures.push(capture);
            }
        }
        Ok(captures)
    }

    pub(crate) fn verify_record_sources(&self, sources: &[SourceAnchor]) -> Result<Vec<Value>> {
        let captures = self.source_captures(sources)?;
        for capture in &captures {
            capture.verify()?;
        }
        verify_sources(sources)?;
        for capture in &captures {
            capture.verify()?;
        }
        Ok(captures.iter().map(Capture::provenance).collect())
    }
}
