//! Visitor-specific boundaries using tiny byte archives, not source books.
use oe_core::sha256;
use oe_knowledge::source_pack::{ArchiveLimits, visit_archive};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("var/maintenance/rust-rewrite-20260927/phase6-archive-tests")
            .join(format!(
                "{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(path.join("sources")).unwrap();
        fs::write(path.join("SKILL.md"), "archive visitor fixture").unwrap();
        Self(path)
    }
    fn archive(&self, members: &[(&str, &[u8])]) -> (PathBuf, Value) {
        let path = self.0.join("sources/input.tar.xz");
        let mut tar = tar::Builder::new(Vec::new());
        for (name, data) in members {
            let mut header = tar::Header::new_gnu();
            header.set_mode(0o600);
            header.set_size(data.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, name, *data).unwrap();
        }
        let raw = tar.into_inner().unwrap();
        let mut xz = xz2::write::XzEncoder::new(Vec::new(), 1);
        xz.write_all(&raw).unwrap();
        let compressed = xz.finish().unwrap();
        fs::write(&path, &compressed).unwrap();
        let descriptor = json!({
            "path":"input.tar.xz", "sha256":sha256(&compressed), "bytes":compressed.len(),
            "files":members.iter().map(|(path,data)|json!({"path":path,"bytes":data.len(),"sha256":sha256(data)})).collect::<Vec<_>>()
        });
        (path, descriptor)
    }
    fn no_materialized_members(&self) {
        let cache = self.0.join("var/cache/knowledge");
        if cache.exists() {
            assert_eq!(fs::read_dir(cache).unwrap().count(), 0);
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn bounds() -> ArchiveLimits {
    ArchiveLimits {
        max_file_bytes: 1024,
        max_members: 8,
        max_uncompressed_bytes: 8192,
    }
}
fn bind_changed_archive(path: &Path, descriptor: &mut Value, raw: &[u8]) {
    fs::write(path, raw).unwrap();
    descriptor["sha256"] = json!(sha256(raw));
    descriptor["bytes"] = json!(raw.len());
}

#[test]
fn selective_materialization_still_verifies_all_members_and_only_lends_one_file() {
    let f = Fixture::new();
    let members = [
        ("pages/一.pdf", b"exact original first".as_slice()),
        ("pages/unselected.pdf", b"unselected exact bytes".as_slice()),
        ("pages/last.pdf", b"exact original last".as_slice()),
    ];
    let (path, descriptor) = f.archive(&members);
    let selected = BTreeSet::from([members[0].0.to_owned(), members[2].0.to_owned()]);
    let mut observed = Vec::new();
    let report = visit_archive(
        &f.0,
        &path,
        &descriptor,
        &bounds(),
        Some(&selected),
        |item| {
            assert_eq!(fs::read_dir(item.path.parent().unwrap())?.count(), 1);
            assert_eq!(item.sha256, sha256(fs::read(&item.path)?));
            assert_eq!(item.bytes, fs::metadata(&item.path)?.len());
            observed.push((
                item.relative_path.clone(),
                fs::read(&item.path)?,
                item.path.clone(),
            ));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(report.members_verified, 3);
    assert_eq!(report.members_visited, 2);
    assert_eq!(
        report.uncompressed_bytes,
        members.iter().map(|(_, b)| b.len() as u64).sum::<u64>()
    );
    assert_eq!(report.compressed_bytes, fs::metadata(&path).unwrap().len());
    assert_eq!(report.archive_sha256, descriptor["sha256"]);
    assert_eq!(observed[0].0, members[0].0);
    assert_eq!(observed[0].1, members[0].1);
    assert_eq!(observed[1].0, members[2].0);
    assert_eq!(observed[1].1, members[2].1);
    assert!(observed.iter().all(|(_, _, path)| !path.exists()));
    f.no_materialized_members();
}

#[test]
fn empty_selection_is_a_full_integrity_visit_without_materialization() {
    let f = Fixture::new();
    let (path, descriptor) = f.archive(&[("a", b"one"), ("b", b"two")]);
    let mut calls = 0;
    let report = visit_archive(
        &f.0,
        &path,
        &descriptor,
        &bounds(),
        Some(&BTreeSet::new()),
        |_| {
            calls += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls, 0);
    assert_eq!(report.members_verified, 2);
    assert_eq!(report.members_visited, 0);
    assert_eq!(report.uncompressed_bytes, 6);
    f.no_materialized_members();
}

#[test]
fn all_members_selection_preserves_archive_order() {
    let f = Fixture::new();
    let (path, descriptor) = f.archive(&[("z", b"last name first"), ("a", b"first name last")]);
    let mut names = Vec::new();
    let report = visit_archive(&f.0, &path, &descriptor, &bounds(), None, |item| {
        names.push(item.relative_path.clone());
        Ok(())
    })
    .unwrap();
    assert_eq!(names, ["z", "a"]);
    assert_eq!(report.members_visited, 2);
    f.no_materialized_members();
}

#[test]
fn callbacks_are_provisional_when_xz_footer_or_trailing_compressed_data_fails() {
    for footer_damage in [false, true] {
        let f = Fixture::new();
        let (path, mut descriptor) = f.archive(&[("a", b"provisional bytes")]);
        let mut raw = fs::read(&path).unwrap();
        if footer_damage {
            let end = raw.len();
            raw[end - 1] ^= 1;
        } else {
            raw.extend_from_slice(b"undeclared compressed tail");
        }
        bind_changed_archive(&path, &mut descriptor, &raw);
        let provisional = f.0.join("var/provisional");
        fs::create_dir_all(&provisional).unwrap();
        let published = f.0.join("sources/published");
        let mut callbacks = 0;
        let result = visit_archive(&f.0, &path, &descriptor, &bounds(), None, |item| {
            callbacks += 1;
            fs::copy(&item.path, provisional.join("copied-member"))?;
            Ok(())
        });
        if result.is_ok() {
            fs::rename(&provisional, &published).unwrap();
        }
        assert!(result.is_err(), "invalid archive must not commit");
        // The caller has observed a valid member before full-stream validation.
        assert_eq!(callbacks, 1, "footer_damage={footer_damage}");
        assert!(!published.exists());
        assert_eq!(
            fs::read(provisional.join("copied-member")).unwrap(),
            b"provisional bytes"
        );
        f.no_materialized_members();
    }
}

#[test]
fn corrupt_unselected_member_rejects_after_selected_callback() {
    let f = Fixture::new();
    let (path, mut descriptor) = f.archive(&[("chosen", b"chosen exact"), ("unselected", b"bad")]);
    descriptor["files"][1]["sha256"] = json!(sha256(b"expected different source"));
    let selected = BTreeSet::from(["chosen".to_owned()]);
    let mut callbacks = 0;
    let error = visit_archive(&f.0, &path, &descriptor, &bounds(), Some(&selected), |_| {
        callbacks += 1;
        Ok(())
    })
    .unwrap_err();
    assert_eq!(callbacks, 1);
    assert!(
        error
            .to_string()
            .contains("source_restored_page_hash_mismatch")
    );
    f.no_materialized_members();
}

#[test]
fn callback_cannot_modify_or_move_the_borrowed_file() {
    for remove in [false, true] {
        let f = Fixture::new();
        let (path, descriptor) = f.archive(&[("a", b"immutable bytes")]);
        let original = fs::read(&path).unwrap();
        let error = visit_archive(&f.0, &path, &descriptor, &bounds(), None, |item| {
            if remove {
                fs::remove_file(&item.path)?;
            } else {
                fs::write(&item.path, b"different bytes")?;
            }
            Ok(())
        })
        .unwrap_err();
        if !remove {
            assert!(
                error
                    .to_string()
                    .contains("source_archive_member_changed_by_visitor")
            );
        }
        assert_eq!(fs::read(path).unwrap(), original);
        f.no_materialized_members();
    }
}

#[test]
fn callback_error_and_unwind_clean_the_current_member_and_stop_iteration() {
    for unwind in [false, true] {
        let f = Fixture::new();
        let (path, descriptor) = f.archive(&[("a", b"one"), ("b", b"two")]);
        let mut calls = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            visit_archive(&f.0, &path, &descriptor, &bounds(), None, |_| {
                calls += 1;
                assert!(!unwind, "injected consumer unwind");
                anyhow::bail!("injected_consumer_error")
            })
        }));
        if unwind {
            assert!(result.is_err());
        } else {
            assert!(
                result
                    .unwrap()
                    .unwrap_err()
                    .to_string()
                    .contains("injected_consumer_error")
            );
        }
        assert_eq!(calls, 1);
        f.no_materialized_members();
    }
}

#[test]
fn caller_limits_bound_all_declared_bytes_and_members_not_only_selection() {
    let f = Fixture::new();
    let (path, descriptor) = f.archive(&[("chosen", b"four"), ("other", b"eight888")]);
    let selected = BTreeSet::from(["chosen".to_owned()]);
    let mut cases = Vec::new();
    let mut limits = bounds();
    limits.max_file_bytes = 4;
    cases.push(limits);
    let mut limits = bounds();
    limits.max_members = 1;
    cases.push(limits);
    let mut limits = bounds();
    limits.max_uncompressed_bytes = 11;
    cases.push(limits);
    let mut limits = bounds();
    limits.max_members = 0;
    cases.push(limits);
    for limits in cases {
        let mut calls = 0;
        assert!(
            visit_archive(&f.0, &path, &descriptor, &limits, Some(&selected), |_| {
                calls += 1;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(calls, 0);
        f.no_materialized_members();
    }
    let exact = ArchiveLimits {
        max_file_bytes: 8,
        max_members: 2,
        max_uncompressed_bytes: 12,
    };
    let report = visit_archive(
        &f.0,
        &path,
        &descriptor,
        &exact,
        Some(&selected),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(report.uncompressed_bytes, 12);
    assert_eq!(report.members_verified, 2);
    f.no_materialized_members();
}

#[test]
fn selection_outside_declared_inventory_is_rejected_before_callbacks() {
    let f = Fixture::new();
    let (path, descriptor) = f.archive(&[("a", b"one")]);
    let selected = BTreeSet::from(["not-declared".to_owned()]);
    let error = visit_archive(&f.0, &path, &descriptor, &bounds(), Some(&selected), |_| {
        panic!("must not run")
    })
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("source_archive_selection_not_declared")
    );
    f.no_materialized_members();
}
