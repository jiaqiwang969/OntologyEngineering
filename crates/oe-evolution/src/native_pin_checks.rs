//! Bounded current native pin checks; logical outcomes retain input order.
use anyhow::Result;
fn check_ordered<T: Sync>(items: &[T], check: impl Fn(&T) -> Result<()> + Sync) -> Result<()> {
    let batches = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..items.len().min(4))
            .map(|lane| {
                let check = &check;
                scope.spawn(move || {
                    items
                        .iter()
                        .enumerate()
                        .skip(lane)
                        .step_by(4)
                        .map(|(i, item)| {
                            (
                                i,
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    check(item)
                                })),
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join()).collect::<Vec<_>>()
    });
    let mut ordered = Vec::with_capacity(items.len());
    for batch in batches {
        match batch {
            Ok(rows) => ordered.extend(rows),
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    ordered.sort_by_key(|(i, _)| *i);
    for (_, outcome) in ordered {
        match outcome {
            Ok(value) => value?,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    #[test]
    fn original_order_across_lanes_and_all_joins() {
        let done = AtomicUsize::new(0);
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let err = check_ordered(&(0..35).collect::<Vec<_>>(), |i| {
            let n = active.fetch_add(1, SeqCst) + 1;
            peak.fetch_max(n, SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(1));
            active.fetch_sub(1, SeqCst);
            done.fetch_add(1, SeqCst);
            anyhow::ensure!(*i != 1, "first");
            anyhow::ensure!(*i != 4, "later");
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "first");
        assert_eq!(done.load(SeqCst), 35);
        assert_eq!(active.load(SeqCst), 0);
        assert!(peak.load(SeqCst) <= 4);
    }
    #[test]
    fn earlier_error_beats_other_lane_panic() {
        let done = AtomicUsize::new(0);
        let err = check_ordered(&(0..12).collect::<Vec<_>>(), |i| {
            done.fetch_add(1, SeqCst);
            anyhow::ensure!(*i != 1, "first");
            assert!(*i != 4, "later panic");
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err.to_string(), "first");
        assert_eq!(done.load(SeqCst), 12);
    }
    #[test]
    fn earlier_panic_beats_other_lane_error_after_joins() {
        let done = AtomicUsize::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            check_ordered(&(0..12).collect::<Vec<_>>(), |i| {
                done.fetch_add(1, SeqCst);
                assert!(*i != 2, "first panic");
                anyhow::ensure!(*i != 4, "later error");
                Ok(())
            })
        }));
        assert_eq!(
            result.unwrap_err().downcast_ref::<&str>(),
            Some(&"first panic")
        );
        assert_eq!(done.load(SeqCst), 12);
    }
    #[test]
    fn empty_single_and_uneven_exactly_once() {
        for n in [0, 1, 2, 3, 4, 5, 35] {
            let counts: Vec<_> = (0..n).map(|_| AtomicUsize::new(0)).collect();
            check_ordered(&(0..n).collect::<Vec<_>>(), |i| {
                counts[*i].fetch_add(1, SeqCst);
                Ok(())
            })
            .unwrap();
            assert!(counts.iter().all(|c| c.load(SeqCst) == 1));
        }
    }
}

pub(crate) fn verify_files(pins: &[&oe_contracts::CodeNativeFileBinding]) -> anyhow::Result<()> {
    check_ordered(pins, |pin| crate::native_profile::verify_file(pin))
}
#[cfg(all(test, unix))]
mod file_boundaries {
    use super::*;
    use oe_contracts::CodeNativeFileBinding;
    use std::{
        os::unix::fs::{PermissionsExt, symlink},
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn fixture() -> (Temp, CodeNativeFileBinding) {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oe-private-pin-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("input");
        std::fs::write(&path, b"abc").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        (
            Temp(root),
            CodeNativeFileBinding {
                path: path.to_str().unwrap().into(),
                bytes: 3,
                mode: 0o600,
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
            },
        )
    }
    fn inspect(pin: &CodeNativeFileBinding) -> Result<()> {
        verify_files(&[pin; 9])
    }
    #[test]
    fn changed_content_and_deletion_never_reuse_prior_success() {
        let (_t, p) = fixture();
        inspect(&p).unwrap();
        std::fs::write(&p.path, b"abd").unwrap();
        assert!(inspect(&p).is_err());
        std::fs::write(&p.path, b"abc").unwrap();
        inspect(&p).unwrap();
        std::fs::remove_file(&p.path).unwrap();
        assert!(inspect(&p).is_err());
    }
    #[test]
    fn changed_permissions_are_rejected() {
        let (_t, p) = fixture();
        inspect(&p).unwrap();
        std::fs::set_permissions(&p.path, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert!(inspect(&p).is_err());
    }
    #[test]
    fn same_bytes_symlink_replacement_is_rejected() {
        let (t, p) = fixture();
        inspect(&p).unwrap();
        let target = t.0.join("target");
        std::fs::rename(&p.path, &target).unwrap();
        symlink(&target, &p.path).unwrap();
        assert!(inspect(&p).is_err());
    }
}
