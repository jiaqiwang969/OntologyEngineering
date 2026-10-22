use anyhow::{Result, ensure};
use oe_nx_protocol::{segmented::TransferPlan, transfer::SourcePublication, *};
use oe_windows_controller::{
    native_input_copy::{Destination, Source as CopySource, prepare},
    transfer_reconstruct::Target,
};
use serde_json::json;
use std::{collections::BTreeMap, io::Write};
fn file(path: &str, content: &[u8]) -> FileEntry {
    FileEntry::from_bytes(path, content).unwrap()
}
fn root() -> RootIdentity {
    RootIdentity {
        host_id: "fixture-host".into(),
        volume_id: "volume-1".into(),
        directory_id: "directory-1".into(),
    }
}
fn runtime() -> RuntimeIdentity {
    RuntimeIdentity {
        product: "fixture-native-runtime".into(),
        product_label: "not-an-NX-acceptance".into(),
        architecture: "x64".into(),
        libraries: vec![file("fixture.dll", b"fixture DLL identity")],
    }
}
fn job() -> JobManifest {
    JobManifest {
        capacity: None,
        auxiliary_capacity: None,
        schema: JOB_SCHEMA.into(),
        job_id: JobId::new("fixture-job").unwrap(),
        root: root(),
        worker: WorkerManifest {
            schema: WORKER_SCHEMA.into(),
            worker_id: "fixture-worker".into(),
            version: "1".into(),
            target: "x86_64-pc-windows-gnu".into(),
            executable: file("worker/fixture.exe", b"not an executable"),
            runtime_files: vec![file("worker/helper.dll", b"not a DLL")],
            source_files: vec![file("sources/main.rs", b"fixture source")],
            build_recipe: file("build/recipe.json", b"fixture recipe"),
            toolchain: "fixture-toolchain".into(),
            required_runtime: runtime(),
            capabilities: vec![
                Capability {
                    action: "inspect".into(),
                    params_schema: "fixture.inspect/v1".into(),
                },
                Capability {
                    action: "custom.thread_gauge".into(),
                    params_schema: "fixture.gauge/v1".into(),
                },
            ],
        },
        action: "inspect".into(),
        params_schema: "fixture.inspect/v1".into(),
        params: file(
            "params.json",
            br#"{"schema":"fixture.inspect/v1","units":"mm"}"#,
        ),
        inputs: vec![
            file("inputs/主体.prt", b"not a PRT"),
            file("inputs/子装配/轴.prt", b"not another PRT"),
        ],
    }
}

fn capacity() -> NativeCapacity {
    serde_json::from_value(json!({"max_input_bytes":268435456,"max_input_file_bytes":209715200,"max_output_bytes":536870912,"max_output_file_bytes":268435456,"max_input_files":10000,"max_output_files":10000,"max_segment_bytes":104857600})).unwrap()
}
fn large() -> JobManifest {
    let mut j = job();
    j.schema = LARGE_JOB_SCHEMA.into();
    j.capacity = Some(capacity());
    j
}
fn plan() -> TransferPlan {
    decode(&serde_json::to_vec(&serde_json::json!({"schema":"oev2.nx-segmented-transfer/1","job_id":"reconstruct-test","root":{"host_id":"h","volume_id":"v","directory_id":"source"},"direction":"upload","source_manifest":Digest::of(b"source"),"budget":{"max_total_bytes":104857601,"max_chunk_bytes":3,"max_files":10,"max_chunks":10},"files":[FileEntry::from_bytes("a/轴.prt",b"abcd").unwrap(),FileEntry::from_bytes("empty.prt",b"").unwrap()],"spans":[{"file_index":0,"offset":0,"bytes":3},{"file_index":0,"offset":3,"bytes":1},{"file_index":1,"offset":0,"bytes":0}]})).unwrap()).unwrap()
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

impl Destination for Sink {
    fn root_identity(&self) -> Result<RootIdentity> {
        Ok(RootIdentity {
            host_id: "h".into(),
            volume_id: "v".into(),
            directory_id: "native-root".into(),
        })
    }
}
struct Source {
    p: TransferPlan,
    pub_record: SourcePublication,
    corrupt: bool,
    fail_after: bool,
    streamed: bool,
}
impl CopySource for Source {
    fn publication(&self) -> &SourcePublication {
        &self.pub_record
    }
    fn files(&self) -> &[FileEntry] {
        &self.p.files
    }
    fn verify(&self) -> Result<()> {
        ensure!(
            !(self.fail_after && self.streamed),
            "source_changed_after_copy"
        );
        Ok(())
    }
    fn stream(&mut self, file: &FileEntry, _: u64, out: &mut dyn Write) -> Result<()> {
        self.streamed = true;
        let b = if file.identity.bytes == 0 {
            b"".as_slice()
        } else if self.corrupt {
            b"bad!".as_slice()
        } else {
            b"abcd".as_slice()
        };
        out.write_all(b)?;
        Ok(())
    }
}
fn fixture() -> (JobManifest, Source) {
    let p = plan();
    let mut target = p.root.clone();
    target.directory_id = "published-tree".into();
    let mut meta = p.root.clone();
    meta.directory_id = "publication".into();
    let pub_record = SourcePublication {
        schema: "oev2.nx-source-publication/1".into(),
        source_root: p.root.clone(),
        job_id: p.job_id.clone(),
        plan_digest: p.binding().unwrap(),
        output_id: JobId::new("published-tree").unwrap(),
        publication_id: JobId::new("publication").unwrap(),
        publication_root: meta,
        target,
        inventory_digest: record_digest(&p.files).unwrap(),
        files: p.files.len(),
        bytes: 4,
        state: "published".into(),
        native_execution_started: false,
    };
    let mut j = large();
    j.root = Sink::default().root_identity().unwrap();
    j.inputs = p
        .files
        .iter()
        .map(|f| FileEntry {
            path: RelativePath::new(format!("inputs/{}", f.path.as_str())).unwrap(),
            identity: f.identity.clone(),
        })
        .collect();
    (
        j,
        Source {
            p,
            pub_record,
            corrupt: false,
            fail_after: false,
            streamed: false,
        },
    )
}
#[test]
fn native_inputs_copy_binds_whole_job_and_preserves_failures() {
    let (job, mut source) = fixture();
    let mut target = Sink::default();
    let r = prepare(&job, &mut source, &mut target).unwrap();
    assert_eq!(r.bytes, 4);
    assert_eq!(r.files, 2);
    assert_eq!(r.job_manifest_digest, job.digest().unwrap());
    assert!(!r.job_staged && !r.native_execution_started);
    assert_eq!(target.data["inputs/a/轴.prt"], b"abcd");
    assert!(target.data["inputs/empty.prt"].is_empty());
    assert!(prepare(&job, &mut source, &mut target).is_err());
    for mode in 0..4 {
        let (job, mut source) = fixture();
        source.corrupt = mode == 0;
        source.fail_after = mode == 1;
        let mut target = Sink {
            fail_write: mode == 2,
            corrupt: mode == 3,
            ..Default::default()
        };
        assert!(prepare(&job, &mut source, &mut target).is_err());
        assert!(!target.data.is_empty());
        let before = target.data.clone();
        target.fail_write = false;
        target.corrupt = false;
        source.corrupt = false;
        source.fail_after = false;
        assert!(prepare(&job, &mut source, &mut target).is_err());
        assert_eq!(target.data, before);
    }
    for mode in 0..5 {
        let (mut job, mut source) = fixture();
        match mode {
            0 => job.inputs.pop().map(|_| ()).unwrap(),
            1 => source.pub_record.state = "pending".into(),
            2 => job.root.directory_id = "wrong-root".into(),
            3 => source.pub_record.bytes += 1,
            _ => job.schema = JOB_SCHEMA.into(),
        };
        let mut target = Sink::default();
        assert!(prepare(&job, &mut source, &mut target).is_err());
        assert!(target.data.is_empty());
    }
}
#[cfg(windows)]
#[test]
fn windows_native_inputs_copy_keeps_sources_pinned() {
    use oe_windows_controller::{
        adapter,
        native_input_copy_windows::PublishedSource,
        transfer_publication::{Operation, publish},
        transfer_reconstruct_windows::PinnedTarget,
        windows::StableRoot,
    };
    use std::ffi::OsStr;
    let path = std::env::temp_dir().join(format!(
        "oe-native-input-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(path.join("source")).unwrap();
    std::fs::create_dir(path.join("native")).unwrap();
    {
        let sr = StableRoot::open(&path.join("source"), None).unwrap();
        let nr = StableRoot::open(&path.join("native"), None).unwrap();
        let sl = sr.lock().unwrap();
        let nl = nr.lock().unwrap();
        let output = sr.create_directory_new(OsStr::new("output")).unwrap();
        let nested = output.create_directory_new(OsStr::new("a")).unwrap();
        nested
            .create_file_new(OsStr::new("轴.prt"), b"abcd", 16)
            .unwrap();
        output
            .create_file_new(OsStr::new("empty.prt"), b"", 16)
            .unwrap();
        let mut p = plan();
        p.root = adapter::root_identity(&sr);
        let tid = PinnedTarget::new(&output, &sl).unwrap().identity().unwrap();
        let publication = publish(
            &sr,
            &sl,
            &p,
            &JobId::new("output").unwrap(),
            &JobId::new("published").unwrap(),
            &tid,
            Operation::Publish,
        )
        .unwrap();
        let metadata = sr.directory(OsStr::new("published")).unwrap();
        let mut source =
            PublishedSource::open(&sr, &sl, &output, &metadata, &p, &publication).unwrap();
        let (mut j, _) = fixture();
        j.root = adapter::root_identity(&nr);
        let destination = nr
            .create_directory_new(OsStr::new(j.job_id.as_str()))
            .unwrap();
        let mut target = PinnedTarget::new(&destination, &nl).unwrap();
        let r = prepare(&j, &mut source, &mut target).unwrap();
        assert_eq!(r.bytes, 4);
        assert!(!r.job_staged && !r.native_execution_started);
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(output.path().join("a/轴.prt"))
                .is_err()
        );
        assert!(
            std::fs::OpenOptions::new()
                .write(true)
                .open(destination.path().join("inputs/a/轴.prt"))
                .is_err()
        );
        assert!(prepare(&j, &mut source, &mut target).is_err());
        assert_eq!(
            std::fs::read(destination.path().join("inputs/a/轴.prt")).unwrap(),
            b"abcd"
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
