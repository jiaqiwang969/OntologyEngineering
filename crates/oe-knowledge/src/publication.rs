//! Shared directory publication for source/index snapshots. Callers establish
//! ownership and validate their format; this module only moves complete trees.
use crate::source_pack::Stage;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use anyhow::Context;
use anyhow::{Result, ensure};
use serde_json::Value;
use std::path::Path;

// Once the original has moved, even unwinding must not let Stage::drop remove it.
struct PublicationStage {
    stage: Option<Stage>,
    preserve: bool,
}
impl Drop for PublicationStage {
    fn drop(&mut self) {
        if self.preserve
            && let Some(stage) = self.stage.take()
        {
            std::mem::forget(stage);
        }
    }
}

/// Publish a complete staged directory with an exclusive rename. Replacement
/// requires the caller's exact previous snapshot, checked on both sides of its
/// move. The stage is consumed so blocked rollback cannot accidentally delete
/// the original. Successful replacement retains the existing cleanup behavior;
/// this is not an archive/history store or a power-loss transaction protocol.
///
/// `snapshot` must validate caller-owned contents and return their byte identity.
/// `before_publish` performs final input checks; it does not grant permissions.
/// The built directory must be inside `stage`, and the destination outside it.
pub(crate) fn publish_directory<S, F>(
    stage: Stage,
    built: &Path,
    target: &Path,
    previous: Option<&Value>,
    snapshot: S,
    before_publish: F,
) -> Result<()>
where
    S: Fn(&Path) -> Result<Value>,
    F: FnOnce() -> Result<()>,
{
    let backup = stage.path.join("previous");
    ensure!(
        [built, target, stage.path.as_path()].iter().all(|path| {
            path.is_absolute()
                && !path.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                })
        }) && built.starts_with(&stage.path)
            && built != stage.path
            && !target.starts_with(&stage.path)
            && !stage.path.starts_with(target)
            && !built.starts_with(&backup)
            && !backup.starts_with(built),
        "publication_invalid_stage_layout"
    );
    // Parent traversal for the actual move is also no-follow, using directory fds.
    let check_built = || {
        oe_core::no_symlinks(built)?;
        ensure!(built.is_dir(), "publication_built_directory_required");
        Ok::<(), anyhow::Error>(())
    };
    check_built()?;
    ensure!(
        std::fs::symlink_metadata(&backup).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "publication_backup_already_exists"
    );
    let mut guard = PublicationStage {
        stage: Some(stage),
        preserve: false,
    };
    if let Some(expected) = previous {
        ensure!(
            snapshot(target)? == *expected,
            "publication_existing_snapshot_changed"
        );
        rename_exclusive(target, &backup)?;
        guard.preserve = true;
        let prepared = (|| {
            ensure!(
                snapshot(&backup)? == *expected,
                "publication_existing_snapshot_changed"
            );
            before_publish()?;
            ensure!(
                snapshot(&backup)? == *expected,
                "publication_existing_snapshot_changed"
            );
            check_built()?;
            rename_exclusive(built, target)
        })();
        if let Err(error) = prepared {
            // Never overwrite a destination created by another writer. The
            // guard preserves old and new trees, including during unwinding.
            if let Err(rollback) = rename_exclusive(&backup, target) {
                anyhow::bail!(
                    "publication_rollback_blocked; original={} new={} error={error:#} rollback={rollback:#}",
                    backup.display(),
                    built.display()
                );
            }
            guard.preserve = false;
            return Err(error);
        }
        guard.preserve = false;
    } else {
        before_publish()?;
        check_built()?;
        rename_exclusive(built, target)?;
    }
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn rename_exclusive(from: &Path, to: &Path) -> Result<()> {
    use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
    let old = crate::source_pack::directory_fd(from.parent().context("rename_parent")?, false)?;
    let new = crate::source_pack::directory_fd(to.parent().context("rename_parent")?, true)?;
    let a = std::ffi::CString::new(from.file_name().context("rename_name")?.as_bytes())?;
    let b = std::ffi::CString::new(to.file_name().context("rename_name")?.as_bytes())?;
    #[cfg(target_os = "macos")]
    let status = unsafe {
        libc::renameatx_np(
            old.as_raw_fd(),
            a.as_ptr(),
            new.as_raw_fd(),
            b.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    #[cfg(target_os = "linux")]
    let status = unsafe {
        libc::renameat2(
            old.as_raw_fd(),
            a.as_ptr(),
            new.as_raw_fd(),
            b.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if status != 0 {
        return Err(std::io::Error::last_os_error()).context("directory_publish_failed");
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn rename_exclusive(_from: &Path, _to: &Path) -> Result<()> {
    anyhow::bail!("exclusive_directory_rename_unsupported")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{collections::BTreeSet, fs, path::PathBuf};

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
    }

    struct Case {
        _destination_owner: Stage,
        stage: Option<Stage>,
        staged_path: PathBuf,
        built: PathBuf,
        target: PathBuf,
        backup: PathBuf,
    }
    impl Case {
        fn new() -> Self {
            let outer = Stage::new(root(), "publication-test-destination").unwrap();
            let stage = Stage::new(root(), "publication-test-built").unwrap();
            let built = stage.path.join("index");
            owned(&built, b"new exact index bytes");
            Self {
                target: outer.path.join("index"),
                backup: stage.path.join("previous"),
                built,
                staged_path: stage.path.clone(),
                stage: Some(stage),
                _destination_owner: outer,
            }
        }
        fn publish<F: FnOnce() -> Result<()>>(
            &mut self,
            previous: Option<&Value>,
            hook: F,
        ) -> Result<()> {
            publish_directory(
                self.stage.take().unwrap(),
                &self.built,
                &self.target,
                previous,
                snapshot,
                hook,
            )
        }
    }
    impl Drop for Case {
        fn drop(&mut self) {
            // Only this test's exclusively allocated staging directory. Production
            // deliberately leaves this directory intact on blocked rollback.
            if self.stage.is_none() {
                let _ = fs::remove_dir_all(&self.staged_path);
            }
        }
    }
    fn owned(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("catalog.sqlite3"), bytes).unwrap();
        oe_core::write_new(
            &path.join("registration.json"),
            &json!({"index_sha256":oe_core::sha256(bytes)}),
        )
        .unwrap();
    }
    fn snapshot(path: &Path) -> Result<Value> {
        oe_core::no_symlinks(path)?;
        let names = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        ensure!(
            names == ["catalog.sqlite3".into(), "registration.json".into()].into(),
            "unowned_directory"
        );
        let index = oe_core::read_regular_bytes(&path.join("catalog.sqlite3"), 1024)?;
        let raw = oe_core::read_regular_bytes(&path.join("registration.json"), 1024)?;
        let registration = oe_core::json::strict_json(&raw)?;
        let hash = oe_core::sha256(index);
        ensure!(
            registration["index_sha256"] == hash,
            "index_identity_mismatch"
        );
        Ok(json!({"index":hash,"registration":oe_core::sha256(raw)}))
    }

    #[test]
    fn publication_new_directory_moves_complete_pair_and_cleans_stage() {
        let mut case = Case::new();
        let expected = snapshot(&case.built).unwrap();
        case.publish(None, || Ok(())).unwrap();
        assert_eq!(snapshot(&case.target).unwrap(), expected);
        assert!(!case.staged_path.exists());
    }

    #[test]
    fn publication_explicit_replacement_moves_complete_pair() {
        let mut case = Case::new();
        owned(&case.target, b"old");
        let old = snapshot(&case.target).unwrap();
        let new = snapshot(&case.built).unwrap();
        case.publish(Some(&old), || Ok(())).unwrap();
        assert_eq!(snapshot(&case.target).unwrap(), new);
        assert!(!case.staged_path.exists());
    }

    #[test]
    fn publication_failure_after_old_move_restores_exact_old_pair() {
        let mut case = Case::new();
        owned(&case.target, b"old exact index bytes");
        let old = snapshot(&case.target).unwrap();
        let error = case
            .publish(Some(&old), || anyhow::bail!("injected_publish_failure"))
            .unwrap_err();
        assert!(error.to_string().contains("injected_publish_failure"));
        assert_eq!(snapshot(&case.target).unwrap(), old);
        assert!(!case.staged_path.exists());
    }

    #[test]
    fn publication_concurrent_target_preserves_original_and_new_after_return() {
        let mut case = Case::new();
        owned(&case.target, b"old");
        let old = snapshot(&case.target).unwrap();
        let new = snapshot(&case.built).unwrap();
        let target = case.target.clone();
        let error = case
            .publish(Some(&old), || {
                fs::create_dir(&target)?;
                fs::write(target.join("concurrent"), b"preserve")?;
                Ok(())
            })
            .unwrap_err();
        assert!(error.to_string().contains("publication_rollback_blocked"));
        assert!(error.to_string().contains(case.backup.to_str().unwrap()));
        assert_eq!(snapshot(&case.backup).unwrap(), old);
        assert_eq!(snapshot(&case.built).unwrap(), new);
        assert_eq!(
            fs::read(case.target.join("concurrent")).unwrap(),
            b"preserve"
        );
    }

    #[test]
    fn publication_new_destination_race_does_not_overwrite() {
        let mut case = Case::new();
        let target = case.target.clone();
        assert!(
            case.publish(None, || {
                fs::create_dir(&target)?;
                fs::write(target.join("concurrent"), b"keep")?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(case.target.join("concurrent")).unwrap(), b"keep");
        assert!(!case.staged_path.exists());
    }

    #[test]
    fn publication_changed_original_before_move_is_not_replaced() {
        let mut case = Case::new();
        owned(&case.target, b"old");
        let old = snapshot(&case.target).unwrap();
        fs::remove_dir_all(&case.target).unwrap();
        owned(&case.target, b"changed coherent snapshot");
        let changed = snapshot(&case.target).unwrap();
        let error = case.publish(Some(&old), || Ok(())).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("publication_existing_snapshot_changed")
        );
        assert_eq!(snapshot(&case.target).unwrap(), changed);
        assert!(!case.staged_path.exists());
    }

    #[test]
    fn publication_changed_backup_during_final_check_is_not_silently_removed() {
        let mut case = Case::new();
        owned(&case.target, b"old");
        let old = snapshot(&case.target).unwrap();
        let backup = case.backup.clone();
        let error = case
            .publish(Some(&old), || {
                fs::write(backup.join("catalog.sqlite3"), b"changed")?;
                Ok(())
            })
            .unwrap_err();
        assert!(error.to_string().contains("index_identity_mismatch"));
        assert_eq!(
            fs::read(case.target.join("catalog.sqlite3")).unwrap(),
            b"changed"
        );
        assert!(!case.staged_path.exists());
    }

    #[test]
    fn publication_unwind_after_old_move_retains_recoverable_trees() {
        let mut case = Case::new();
        owned(&case.target, b"old");
        let old = snapshot(&case.target).unwrap();
        let new = snapshot(&case.built).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            case.publish(Some(&old), || panic!("injected unwind"))
        }));
        assert!(result.is_err());
        assert!(!case.target.exists());
        assert_eq!(snapshot(&case.backup).unwrap(), old);
        assert_eq!(snapshot(&case.built).unwrap(), new);
    }

    #[test]
    fn publication_rejects_destination_inside_consumed_stage() {
        let mut case = Case::new();
        case.target = case.staged_path.join("target");
        let error = case.publish(None, || Ok(())).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("publication_invalid_stage_layout")
        );
    }

    #[cfg(unix)]
    #[test]
    fn publication_symlinked_destination_parent_cannot_redirect_write() {
        use std::os::unix::fs::symlink;
        let mut case = Case::new();
        let redirected = case._destination_owner.path.join("redirected");
        fs::create_dir(&redirected).unwrap();
        let link = case._destination_owner.path.join("link");
        symlink(&redirected, &link).unwrap();
        case.target = link.join("index");
        assert!(case.publish(None, || Ok(())).is_err());
        assert_eq!(fs::read_dir(redirected).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn publication_staged_directory_substitution_cannot_publish_symlink() {
        use std::os::unix::fs::symlink;
        let mut case = Case::new();
        owned(&case.target, b"old");
        let old = snapshot(&case.target).unwrap();
        let built = case.built.clone();
        let unrelated = case._destination_owner.path.join("unrelated");
        owned(&unrelated, b"untouched");
        let untouched = snapshot(&unrelated).unwrap();
        assert!(
            case.publish(Some(&old), || {
                fs::remove_dir_all(&built)?;
                symlink(&unrelated, &built)?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(snapshot(&case.target).unwrap(), old);
        assert_eq!(snapshot(&unrelated).unwrap(), untouched);
        assert!(!case.staged_path.exists());
    }
}
