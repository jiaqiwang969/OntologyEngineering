use anyhow::{Result, ensure};
use oe_nx_protocol::{segmented::TransferPlan, *};
use oe_windows_controller::transfer_store::{Session, Store};
use std::collections::BTreeMap;
fn plan() -> TransferPlan {
    let v = serde_json::json!({"schema":"oev2.nx-segmented-transfer/1","job_id":"store-test-01","root":{"host_id":"host","volume_id":"volume","directory_id":"directory"},"direction":"upload","source_manifest":Digest::of(b"source"),"budget":{"max_total_bytes":104857601,"max_chunk_bytes":3,"max_files":10,"max_chunks":10},"files":[FileEntry::from_bytes("inputs/a",b"abcd").unwrap()],"spans":[{"file_index":0,"offset":0,"bytes":3},{"file_index":0,"offset":3,"bytes":1}]});
    decode(&serde_json::to_vec(&v).unwrap()).unwrap()
}
#[derive(Default)]
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
    fail: Option<String>,
    short: bool,
    wrong_root: bool,
    writes: usize,
}
impl Store for Memory {
    fn root(&self) -> Result<RootIdentity> {
        let mut r = plan().root;
        if self.wrong_root {
            r.host_id = "wrong".into();
        }
        Ok(r)
    }
    fn names(&self, limit: usize) -> Result<Vec<String>> {
        ensure!(self.files.len() <= limit, "bound");
        Ok(self.files.keys().cloned().collect())
    }
    fn read(&mut self, n: &str, limit: u64) -> Result<Vec<u8>> {
        let b = self
            .files
            .get(n)
            .ok_or_else(|| anyhow::anyhow!("missing"))?;
        ensure!(b.len() as u64 <= limit, "bound");
        Ok(b.clone())
    }
    fn create_sync(&mut self, n: &str, b: &[u8]) -> Result<()> {
        ensure!(!self.files.contains_key(n), "exists");
        self.writes += 1;
        if self.fail.as_deref() == Some(n) {
            if self.short {
                self.files.insert(n.into(), b[..b.len() / 2].to_vec());
            }
            anyhow::bail!("injected durable write failure")
        }
        self.files.insert(n.into(), b.into());
        Ok(())
    }
    fn verify(&self) -> Result<()> {
        Ok(())
    }
}
#[test]
fn resume_reconstructs_and_finish_rereads() {
    let p = plan();
    let id = p.binding().unwrap();
    let mut s = Memory::default();
    {
        let mut r = Session::open(&mut s, p.clone(), false).unwrap();
        r.receive(&id, 0, b"abc").unwrap();
    }
    let mut r = Session::open(&mut s, p, false).unwrap();
    assert_eq!(r.next_index(), 1);
    r.receive(&id, 1, b"d").unwrap();
    assert_eq!(r.finish().unwrap().bytes, 4);
    assert_eq!(s.files.len(), 7);
}
#[test]
fn complete_chunk_needs_explicit_commit_and_partial_write_stays_held() {
    for partial in [false, true] {
        let mut s = Memory {
            fail: Some(
                if partial {
                    "chunk-00000.bin"
                } else {
                    "chunk-00000.receipt.json"
                }
                .into(),
            ),
            short: partial,
            ..Default::default()
        };
        let p = plan();
        let id = p.binding().unwrap();
        {
            let mut r = Session::open(&mut s, p.clone(), false).unwrap();
            assert!(r.receive(&id, 0, b"abc").is_err());
            assert!(r.receive(&id, 0, b"abc").is_err());
        }
        s.fail = None;
        let original = s.files.clone();
        assert!(Session::open(&mut s, p.clone(), false).is_err());
        assert_eq!(s.files, original);
        if partial {
            assert!(Session::open(&mut s, p, true).is_err());
            assert_eq!(s.files, original);
        } else {
            let mut r = Session::open(&mut s, p, true).unwrap();
            assert_eq!(r.next_index(), 1);
            r.receive(&id, 1, b"d").unwrap();
            r.finish().unwrap();
        }
    }
}
#[test]
fn tampering_unknown_members_and_wrong_root_reject_before_recovery_writes() {
    let p = plan();
    let id = p.binding().unwrap();
    let mut s = Memory::default();
    {
        let mut r = Session::open(&mut s, p.clone(), false).unwrap();
        r.receive(&id, 0, b"abc").unwrap();
    }
    let good = s.files.clone();
    for mode in 0..5 {
        s.files = good.clone();
        s.wrong_root = false;
        match mode {
            0 => {
                s.files.insert("chunk-00000.bin".into(), b"XYZ".to_vec());
            }
            1 => {
                s.files.insert("unexpected".into(), vec![]);
            }
            2 => {
                s.files.remove("chunk-00000.intent.json");
            }
            3 => {
                s.files
                    .insert("chunk-00000.receipt.json".into(), b"{}".to_vec());
            }
            _ => s.wrong_root = true,
        }
        let before = s.files.clone();
        assert!(Session::open(&mut s, p.clone(), true).is_err());
        assert_eq!(s.files, before);
    }
}
#[cfg(windows)]
#[test]
fn pinned_windows_store_reopens_and_denies_change_to_received_bytes() {
    use oe_windows_controller::{
        adapter, transfer_store_windows::PinnedStore, windows::StableRoot,
    };
    use std::ffi::OsStr;
    let path = std::env::temp_dir().join(format!(
        "oe-transfer-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let dir = root
            .create_directory_new(OsStr::new("transfer-fixture"))
            .unwrap();
        let mut p = plan();
        p.root = adapter::root_identity(&root);
        let id = p.binding().unwrap();
        {
            let mut store = PinnedStore::new(&dir, &lock).unwrap();
            let mut session = Session::open(&mut store, p.clone(), false).unwrap();
            session.receive(&id, 0, b"abc").unwrap();
            drop(session);
            assert!(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(dir.path().join("chunk-00000.bin"))
                    .is_err()
            );
        }
        {
            let mut store = PinnedStore::new(&dir, &lock).unwrap();
            let mut session = Session::open(&mut store, p.clone(), false).unwrap();
            assert_eq!(session.next_index(), 1);
            session.receive(&id, 1, b"d").unwrap();
            assert_eq!(session.finish().unwrap().bytes, 4);
        }
        std::fs::write(dir.path().join("chunk-00000.bin"), b"XYZ").unwrap();
        let mut store = PinnedStore::new(&dir, &lock).unwrap();
        assert!(Session::open(&mut store, p, false).is_err());
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn batch_preflight_and_partial_failure_preserve_resume() {
    let p = plan();
    let id = p.binding().unwrap();
    for (count, data) in [
        (0, b"".as_slice()),
        (3, b"abcd".as_slice()),
        (2, b"abc".as_slice()),
    ] {
        let mut s = Memory::default();
        {
            let mut r = Session::open(&mut s, p.clone(), false).unwrap();
            assert!(r.receive_batch(&id, 0, count, data).is_err());
            assert!(r.receive_batch(&id, 0, 2, b"abcd").is_err());
        }
        assert_eq!(s.files.len(), 1);
        assert_eq!(s.writes, 1);
    }
    let mut s = Memory::default();
    {
        let mut r = Session::open(&mut s, p.clone(), false).unwrap();
        assert!(r.receive_batch(&id, 0, 2, b"abcX").is_err());
    }
    assert_eq!(s.files.len(), 4); // Only the first span was durably acknowledged.
    {
        let mut r = Session::open(&mut s, p.clone(), false).unwrap();
        assert_eq!(r.next_index(), 1);
        r.receive_batch(&id, 1, 1, b"d").unwrap();
        assert_eq!(r.finish().unwrap().bytes, 4);
    }
    let mut s = Memory {
        fail: Some("chunk-00001.receipt.json".into()),
        ..Default::default()
    };
    {
        let mut r = Session::open(&mut s, p.clone(), false).unwrap();
        assert!(r.receive_batch(&id, 0, 2, b"abcd").is_err());
    }
    s.fail = None;
    assert!(Session::open(&mut s, p.clone(), false).is_err());
    let r = Session::open(&mut s, p.clone(), true).unwrap();
    assert_eq!(r.next_index(), 2);
    assert_eq!(r.finish().unwrap().bytes, 4);
    // Batch spans can cross files, including a zero-byte file.
    let files = vec![
        FileEntry::from_bytes("a", b"ab").unwrap(),
        FileEntry::from_bytes("b", b"").unwrap(),
        FileEntry::from_bytes("c", b"cd").unwrap(),
    ];
    let p = TransferPlan::new(
        p.job_id,
        p.root,
        "upload".into(),
        p.source_manifest,
        p.budget,
        files,
    )
    .unwrap();
    let id = p.binding().unwrap();
    let mut s = Memory::default();
    let mut r = Session::open(&mut s, p, false).unwrap();
    r.receive_batch(&id, 0, 3, b"abcd").unwrap();
    let result = r.finish().unwrap();
    assert_eq!(result.files, 3);
    assert_eq!(result.bytes, 4);
}
#[test]
fn batch_scope_checks_are_linear_and_final_identity_failure_rejects() {
    #[cfg(windows)]
    check_windows_previous_file_link();
    use std::cell::Cell;
    struct Checked {
        inner: Memory,
        visits: Cell<usize>,
        scopes: Cell<usize>,
        changed: bool,
        inject: bool,
    }
    impl Store for Checked {
        fn root(&self) -> Result<RootIdentity> {
            self.inner.root()
        }
        fn names(&self, n: usize) -> Result<Vec<String>> {
            self.inner.names(n)
        }
        fn read(&mut self, n: &str, l: u64) -> Result<Vec<u8>> {
            self.verify_scope()?;
            self.inner.read(n, l)
        }
        fn create_sync(&mut self, n: &str, b: &[u8]) -> Result<()> {
            self.verify_scope()?;
            self.inner.create_sync(n, b)?;
            if self.inject && n == "chunk-00001.bin" {
                self.changed = true;
            }
            Ok(())
        }
        fn verify_scope(&self) -> Result<()> {
            self.scopes.set(self.scopes.get() + 1);
            Ok(())
        }
        fn verify(&self) -> Result<()> {
            self.visits.set(self.visits.get() + self.inner.files.len());
            ensure!(!self.changed, "previous_file_identity_changed");
            Ok(())
        }
    }
    for n in [16, 128] {
        let base = plan();
        let mut budget = base.budget;
        budget.max_files = 1000;
        budget.max_chunks = 1000;
        let p = TransferPlan::new(
            base.job_id,
            base.root,
            "upload".into(),
            base.source_manifest,
            budget,
            (0..n)
                .map(|i| FileEntry::from_bytes(&format!("f{i:04}"), b"x").unwrap())
                .collect(),
        )
        .unwrap();
        let id = p.binding().unwrap();
        for inject in [false, true] {
            let mut store = Checked {
                inner: Memory::default(),
                visits: Cell::new(0),
                scopes: Cell::new(0),
                changed: false,
                inject,
            };
            {
                let mut session = Session::open(&mut store, p.clone(), false).unwrap();
                let result = session.receive_batch(&id, 0, n, &vec![b'x'; n]);
                if inject {
                    assert!(result.is_err());
                    assert!(session.finish().is_err());
                } else {
                    result.unwrap();
                    assert_eq!(session.finish().unwrap().files, n);
                }
            }
            assert!(
                store.visits.get() <= 10 * n + 10,
                "full checks became quadratic: {}",
                store.visits.get()
            );
            assert!(store.scopes.get() >= 2 * n);
        }
    }
}
#[cfg(windows)]
fn check_windows_previous_file_link() {
    use oe_windows_controller::{
        adapter, transfer_store_windows::PinnedStore, windows::StableRoot,
    };
    use std::ffi::OsStr;
    struct Interference<'a, 'root> {
        inner: PinnedStore<'a, 'root>,
        source: std::path::PathBuf,
        alias: std::path::PathBuf,
        linked: bool,
    }
    impl Store for Interference<'_, '_> {
        fn root(&self) -> Result<RootIdentity> {
            self.inner.root()
        }
        fn names(&self, n: usize) -> Result<Vec<String>> {
            self.inner.names(n)
        }
        fn read(&mut self, n: &str, l: u64) -> Result<Vec<u8>> {
            self.inner.read(n, l)
        }
        fn create_sync(&mut self, n: &str, b: &[u8]) -> Result<()> {
            self.inner.create_sync(n, b)?;
            if n == "chunk-00001.bin" {
                self.linked = std::fs::hard_link(&self.source, &self.alias).is_ok();
            }
            Ok(())
        }
        fn verify_scope(&self) -> Result<()> {
            self.inner.verify_scope()
        }
        fn verify(&self) -> Result<()> {
            self.inner.verify()
        }
    }
    let path = std::env::temp_dir().join(format!(
        "oe-transfer-batch-link-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    {
        let root = StableRoot::open(&path, None).unwrap();
        let lock = root.lock().unwrap();
        let dir = root.create_directory_new(OsStr::new("receive")).unwrap();
        let mut p = plan();
        p.root = adapter::root_identity(&root);
        let id = p.binding().unwrap();
        let mut store = Interference {
            inner: PinnedStore::new(&dir, &lock).unwrap(),
            source: dir.path().join("chunk-00000.bin"),
            alias: path.join("old-chunk-alias"),
            linked: false,
        };
        let result = {
            let mut session = Session::open(&mut store, p, false).unwrap();
            session.receive_batch(&id, 0, 2, b"abcd")
        };
        if store.linked {
            assert!(result.is_err(), "changed old file was acknowledged");
            println!("windows_batch_link_change=created_then_rejected_before_reply");
        } else {
            result.unwrap();
            println!("windows_batch_link_change=blocked_by_pinned_handle");
        }
    }
    std::fs::remove_dir_all(path).unwrap();
}
