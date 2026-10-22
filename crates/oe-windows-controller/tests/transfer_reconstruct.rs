use anyhow::{Result, ensure};
use oe_nx_protocol::{segmented::TransferPlan, *};
use oe_windows_controller::{
    transfer_reconstruct::{Target, reconstruct},
    transfer_store::{Session, Store},
};
use std::{collections::BTreeMap, io::Write};
fn plan() -> TransferPlan {
    decode(&serde_json::to_vec(&serde_json::json!({"schema":"oev2.nx-segmented-transfer/1","job_id":"reconstruct-test","root":{"host_id":"h","volume_id":"v","directory_id":"source"},"direction":"upload","source_manifest":Digest::of(b"source"),"budget":{"max_total_bytes":104857601,"max_chunk_bytes":3,"max_files":10,"max_chunks":10},"files":[FileEntry::from_bytes("a/轴.prt",b"abcd").unwrap(),FileEntry::from_bytes("empty.prt",b"").unwrap()],"spans":[{"file_index":0,"offset":0,"bytes":3},{"file_index":0,"offset":3,"bytes":1},{"file_index":1,"offset":0,"bytes":0}]})).unwrap()).unwrap()
}
struct Source {
    root: RootIdentity,
    data: BTreeMap<String, Vec<u8>>,
    reads: usize,
    corrupt_copy: bool,
}
impl Store for Source {
    fn root(&self) -> Result<RootIdentity> {
        Ok(self.root.clone())
    }
    fn names(&self, l: usize) -> Result<Vec<String>> {
        ensure!(self.data.len() <= l, "bound");
        Ok(self.data.keys().cloned().collect())
    }
    fn read(&mut self, n: &str, l: u64) -> Result<Vec<u8>> {
        let mut b = self
            .data
            .get(n)
            .ok_or_else(|| anyhow::anyhow!("missing"))?
            .clone();
        ensure!(b.len() as u64 <= l, "bound");
        if n == "chunk-00000.bin" {
            self.reads += 1;
            if self.corrupt_copy && self.reads == 3 {
                b[0] = b'X';
            }
        }
        Ok(b)
    }
    fn create_sync(&mut self, n: &str, b: &[u8]) -> Result<()> {
        ensure!(!self.data.contains_key(n), "existing");
        self.data.insert(n.into(), b.into());
        Ok(())
    }
    fn verify(&self) -> Result<()> {
        Ok(())
    }
}
fn source(p: &TransferPlan, complete: bool) -> Source {
    let mut s = Source {
        root: p.root.clone(),
        data: BTreeMap::new(),
        reads: 0,
        corrupt_copy: false,
    };
    {
        let mut session = Session::open(&mut s, p.clone(), false).unwrap();
        session.receive(&p.binding().unwrap(), 0, b"abc").unwrap();
        if complete {
            session
                .receive_batch(&p.binding().unwrap(), 1, 2, b"d")
                .unwrap();
        }
    }
    s.reads = 0;
    s
}
#[derive(Default)]
struct Sink {
    data: BTreeMap<String, Vec<u8>>,
    corrupt: bool,
    fail_write: bool,
    max_write: usize,
}
impl Target for Sink {
    fn identity(&self) -> Result<RootIdentity> {
        Ok(RootIdentity {
            host_id: "h".into(),
            volume_id: "v".into(),
            directory_id: "destination".into(),
        })
    }
    fn verify_empty(&self) -> Result<()> {
        ensure!(self.data.is_empty(), "occupied");
        Ok(())
    }
    fn write_new<F: FnOnce(&mut dyn Write) -> Result<()>>(
        &mut self,
        file: &FileEntry,
        write: F,
    ) -> Result<()> {
        ensure!(!self.data.contains_key(file.path.as_str()), "exists");
        self.data.insert(file.path.as_str().into(), vec![]);
        struct Stream<'a> {
            out: &'a mut Vec<u8>,
            fail: bool,
            max: &'a mut usize,
        }
        impl Write for Stream<'_> {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                *self.max = (*self.max).max(b.len());
                if self.fail {
                    if self.out.len() >= 2 {
                        return Err(std::io::Error::other("injected write failure"));
                    }
                    let n = b.len().min(2 - self.out.len());
                    self.out.extend_from_slice(&b[..n]);
                    Ok(n)
                } else {
                    self.out.extend_from_slice(b);
                    Ok(b.len())
                }
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let out = self.data.get_mut(file.path.as_str()).unwrap();
        write(&mut Stream {
            out,
            fail: self.fail_write,
            max: &mut self.max_write,
        })?;
        if self.corrupt && !out.is_empty() {
            out[0] = b'X';
        }
        Ok(())
    }
    fn readback(&mut self, file: &FileEntry) -> Result<BlobIdentity> {
        Ok(BlobIdentity::of(self.data.get(file.path.as_str()).unwrap()))
    }
    fn verify_complete(&self, files: &[FileEntry]) -> Result<()> {
        ensure!(files.len() == self.data.len(), "extra files");
        for f in files {
            f.identity.verify(self.data.get(f.path.as_str()).unwrap())?;
        }
        Ok(())
    }
}
#[test]
fn reconstruction_streams_and_rechecks_without_publication() {
    let p = plan();
    let mut s = source(&p, true);
    let mut out = Sink::default();
    let r = reconstruct(&mut s, p.clone(), &mut out).unwrap();
    assert_eq!(r.files, 2);
    assert_eq!(r.bytes, 4);
    assert!(!r.published && !r.native_execution_started);
    assert_eq!(out.max_write, 3);
    assert_eq!(out.data["a/轴.prt"], b"abcd");
    let before = out.data.clone();
    assert!(reconstruct(&mut s, p, &mut out).is_err());
    assert_eq!(out.data, before);
}
#[test]
fn reconstruction_rejects_missing_chunks_corruption_and_partial_write() {
    let p = plan();
    let mut absent = Source {
        root: p.root.clone(),
        data: BTreeMap::new(),
        reads: 0,
        corrupt_copy: false,
    };
    let mut untouched = Sink::default();
    assert!(reconstruct(&mut absent, p.clone(), &mut untouched).is_err());
    assert!(absent.data.is_empty() && untouched.data.is_empty());
    let mut s = source(&p, false);
    let mut out = Sink::default();
    assert!(reconstruct(&mut s, p.clone(), &mut out).is_err());
    assert!(out.data.is_empty());
    for mode in 0..3 {
        let mut s = source(&p, true);
        s.corrupt_copy = mode == 2;
        let mut out = Sink {
            corrupt: mode == 0,
            fail_write: mode == 1,
            ..Default::default()
        };
        assert!(reconstruct(&mut s, p.clone(), &mut out).is_err());
        assert!(!out.data.is_empty());
        let before = out.data.clone();
        out.fail_write = false;
        out.corrupt = false;
        s.corrupt_copy = false;
        assert!(reconstruct(&mut s, p.clone(), &mut out).is_err());
        assert_eq!(out.data, before);
    }
}
#[cfg(windows)]
#[test]
fn windows_reconstruction_reopens_complete_files_and_denies_overwrite() {
    use oe_windows_controller::{
        adapter, transfer_reconstruct_windows::PinnedTarget, windows::StableRoot,
    };
    use std::ffi::OsStr;
    let path = std::env::temp_dir().join(format!(
        "oe-reconstruct-{}-{}",
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
            .create_directory_new(OsStr::new("unpublished"))
            .unwrap();
        let mut p = plan();
        p.root = adapter::root_identity(&root);
        let mut s = source(&p, true);
        {
            let mut target = PinnedTarget::new(&dir, &lock).unwrap();
            let receipt = reconstruct(&mut s, p.clone(), &mut target).unwrap();
            assert_eq!(receipt.bytes, 4);
            assert!(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(dir.path().join("a/轴.prt"))
                    .is_err()
            );
            assert!(reconstruct(&mut s, p.clone(), &mut target).is_err());
            std::fs::write(dir.path().join("unexpected"), b"extra").unwrap();
            assert!(target.verify_complete(&p.files).is_err());
        }
        assert_eq!(std::fs::read(dir.path().join("a/轴.prt")).unwrap(), b"abcd");
        assert!(
            std::fs::read(dir.path().join("empty.prt"))
                .unwrap()
                .is_empty()
        );
        assert!(
            dir.create_stream_new(OsStr::new("overflow"), 1, |out| out.write_all(b"xx"))
                .is_err()
        );
        assert!(
            dir.create_stream_new(OsStr::new("short"), 2, |out| out.write_all(b"x"))
                .is_err()
        );
        assert!(
            dir.create_stream_new(OsStr::new("short"), 1, |out| out.write_all(b"x"))
                .is_err()
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn reconstruction_rejects_oversized_file_before_access() {
    let mut p = plan();
    let mut file = p.files[0].clone();
    file.identity.bytes = 9 * 1024 * 1024 * 1024;
    p.budget.max_total_bytes = 64 * 1024 * 1024 * 1024;
    p.budget.max_chunk_bytes = MAX_PAYLOAD_BYTES;
    p.budget.max_chunks = 1000;
    let p = TransferPlan::new(
        p.job_id,
        p.root,
        "upload".into(),
        p.source_manifest,
        p.budget,
        vec![file],
    )
    .unwrap();
    let mut s = Source {
        root: p.root.clone(),
        data: BTreeMap::new(),
        reads: 0,
        corrupt_copy: false,
    };
    let mut out = Sink::default();
    assert_eq!(
        reconstruct(&mut s, p, &mut out).unwrap_err().to_string(),
        "reconstruction_file_bound"
    );
    assert!(s.data.is_empty() && out.data.is_empty());
}
