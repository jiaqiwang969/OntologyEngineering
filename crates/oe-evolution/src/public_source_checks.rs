//! Bounded current-file checks, preserving logical error order and final joins.
use anyhow::{Result, ensure};
use serde_json::{Map, Value};
use std::path::Path;

fn check_ordered<T: Sync>(items: &[T], check: impl Fn(&T) -> Result<()> + Sync) -> Result<()> {
    let width = items.len().div_ceil(4).max(1);
    let batches = std::thread::scope(|scope| {
        let workers: Vec<_> = items
            .chunks(width)
            .map(|chunk| {
                let check = &check;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|item| {
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(item)))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        // Join every worker before propagating the first original-order outcome.
        workers
            .into_iter()
            .map(|worker| worker.join())
            .collect::<Vec<_>>()
    });
    for batch in batches {
        let outcomes = match batch {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        for outcome in outcomes {
            match outcome {
                Ok(value) => value?,
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
    }
    Ok(())
}

pub(crate) fn check_files(root: &Path, files: &Map<String, Value>) -> Result<()> {
    check_files_with_profile(
        root,
        files,
        crate::inventory::PublicManifestProfile::CurrentDerived32,
    )
}
pub(crate) fn check_files_with_profile(
    root: &Path,
    files: &Map<String, Value>,
    profile: crate::inventory::PublicManifestProfile,
) -> Result<()> {
    let ordered: Vec<_> = files.iter().collect();
    check_ordered(&ordered, |(path, expected)| {
        let actual =
            oe_core::fingerprint_file(&root.join(path), profile.file_bytes(Path::new(path)))?;
        ensure!(
            actual.sha256 == expected["sha256"] && actual.bytes == expected["bytes"],
            "code_manifest_file_changed:{path}"
        );
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn checks_are_bounded_and_join_before_first_error() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let result = check_ordered(&(0..17).collect::<Vec<_>>(), |i| {
            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(count, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(2));
            active.fetch_sub(1, Ordering::SeqCst);
            done.fetch_add(1, Ordering::SeqCst);
            ensure!(*i != 1, "first");
            ensure!(*i != 12, "later");
            Ok(())
        });
        assert_eq!(result.unwrap_err().to_string(), "first");
        assert_eq!(done.load(Ordering::SeqCst), 17);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert!(peak.load(Ordering::SeqCst) <= 4);
    }
    #[test]
    fn later_panics_do_not_discard_earlier_error() {
        let done = AtomicUsize::new(0);
        let result = check_ordered(&(0..9).collect::<Vec<_>>(), |i| {
            done.fetch_add(1, Ordering::SeqCst);
            ensure!(*i != 0, "original_first_error");
            assert!(*i != 1 && *i != 8, "later panic");
            Ok(())
        });
        assert_eq!(result.unwrap_err().to_string(), "original_first_error");
        assert_eq!(done.load(Ordering::SeqCst), 9);
    }
    #[test]
    fn first_panic_propagates_only_after_all_checks_join() {
        let done = AtomicUsize::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            check_ordered(&(0..9).collect::<Vec<_>>(), |i| {
                done.fetch_add(1, Ordering::SeqCst);
                assert!(*i != 0, "first panic");
                Ok(())
            })
        }));
        assert!(result.is_err());
        assert_eq!(done.load(Ordering::SeqCst), 9);
    }
    #[test]
    fn empty_and_single_input_preserve_outcomes() {
        check_ordered::<usize>(&[], |_| panic!("must not run")).unwrap();
        assert_eq!(
            check_ordered(&[0], |_| Err(anyhow::anyhow!("only")))
                .unwrap_err()
                .to_string(),
            "only"
        );
    }
    #[test]
    fn files_are_rechecked_after_mutation_and_removal() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let path = root.join("item");
        std::fs::write(&path, b"before").unwrap();
        let id = oe_core::fingerprint_file(&path, 1024).unwrap();
        let mut files = Map::new();
        files.insert(
            "item".into(),
            serde_json::json!({"sha256":id.sha256,"bytes":id.bytes}),
        );
        check_files(&root, &files).unwrap();
        std::fs::write(&path, b"after!").unwrap();
        assert_eq!(
            check_files(&root, &files).unwrap_err().to_string(),
            "code_manifest_file_changed:item"
        );
        std::fs::remove_file(path).unwrap();
        assert!(check_files(&root, &files).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_never_reuses_prior_file_acceptance() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let path = root.join("item");
        std::fs::write(&path, b"same").unwrap();
        let id = oe_core::fingerprint_file(&path, 1024).unwrap();
        let mut files = Map::new();
        files.insert(
            "item".into(),
            serde_json::json!({"sha256":id.sha256,"bytes":id.bytes}),
        );
        check_files(&root, &files).unwrap();
        std::fs::rename(&path, root.join("target")).unwrap();
        std::os::unix::fs::symlink("target", path).unwrap();
        assert!(check_files(&root, &files).is_err());
    }
}
