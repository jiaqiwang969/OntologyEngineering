//! One bounded framed request or a detached, persistent single-use supervisor.
//! All transition and result judgments are delegated to oe-nx-protocol.
use crate::{adapter::*, windows::*};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol as p;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub struct Config {
    pub root: PathBuf,
    pub nx_root: PathBuf,
    pub expected: Option<p::RootIdentity>,
    pub review: Option<(PathBuf, p::Digest)>,
    pub dispatch_writer_review: Option<(PathBuf, p::Digest)>,
    pub supervise: Option<(p::JobId, p::Digest)>,
}
impl Config {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let mut map = BTreeMap::new();
        let mut args = args.into_iter();
        let mut serve = false;
        while let Some(key) = args.next() {
            let key = key
                .into_string()
                .map_err(|_| anyhow::anyhow!("argument key encoding"))?;
            if key == "--serve-one" {
                ensure!(!serve, "duplicate serve flag");
                serve = true;
                continue;
            }
            ensure!(
                [
                    "--root",
                    "--nx-root",
                    "--expected-host",
                    "--expected-volume",
                    "--expected-directory",
                    "--legacy-review",
                    "--legacy-review-sha256",
                    "--dispatch-writer-review",
                    "--dispatch-writer-review-sha256",
                    "--supervise",
                    "--dispatch-id"
                ]
                .contains(&key.as_str()),
                "unknown argument"
            );
            let value = args.next().context("argument value missing")?;
            ensure!(map.insert(key, value).is_none(), "duplicate argument");
        }
        fn text(map: &mut BTreeMap<String, OsString>, key: &str) -> Result<Option<String>> {
            map.remove(key)
                .map(|s| {
                    s.into_string()
                        .map_err(|_| anyhow::anyhow!("argument encoding"))
                })
                .transpose()
        }
        let root = PathBuf::from(map.remove("--root").context("root required")?);
        let nx_root = PathBuf::from(map.remove("--nx-root").context("nx root required")?);
        let expected = match (
            text(&mut map, "--expected-host")?,
            text(&mut map, "--expected-volume")?,
            text(&mut map, "--expected-directory")?,
        ) {
            (None, None, None) => None,
            (Some(host_id), Some(volume_id), Some(directory_id)) => Some(p::RootIdentity {
                host_id,
                volume_id,
                directory_id,
            }),
            _ => anyhow::bail!("complete root identity required"),
        };
        let review = match (
            map.remove("--legacy-review"),
            text(&mut map, "--legacy-review-sha256")?,
        ) {
            (None, None) => None,
            (Some(path), Some(hash)) => Some((PathBuf::from(path), p::Digest::new(hash)?)),
            _ => anyhow::bail!("complete review pin required"),
        };
        let dispatch_writer_review = match (
            map.remove("--dispatch-writer-review"),
            text(&mut map, "--dispatch-writer-review-sha256")?,
        ) {
            (None, None) => None,
            (Some(path), Some(hash)) => Some((PathBuf::from(path), p::Digest::new(hash)?)),
            _ => anyhow::bail!("complete independent dispatch review pin required"),
        };
        let supervise = match (
            text(&mut map, "--supervise")?,
            text(&mut map, "--dispatch-id")?,
        ) {
            (None, None) => None,
            (Some(job), Some(hash)) => Some((p::JobId::new(job)?, p::Digest::new(hash)?)),
            _ => anyhow::bail!("supervisor identity required"),
        };
        ensure!(
            serve != supervise.is_some(),
            "exactly one controller mode required"
        );
        Ok(Self {
            root,
            nx_root,
            expected,
            review,
            dispatch_writer_review,
            supervise,
        })
    }
    fn args(&self) -> Vec<OsString> {
        let mut a = vec![
            "--root".into(),
            self.root.clone().into_os_string(),
            "--nx-root".into(),
            self.nx_root.clone().into_os_string(),
        ];
        if let Some(e) = &self.expected {
            for (k, v) in [
                ("--expected-host", &e.host_id),
                ("--expected-volume", &e.volume_id),
                ("--expected-directory", &e.directory_id),
            ] {
                a.push(k.into());
                a.push(v.into());
            }
        }
        if let Some((path, hash)) = &self.review {
            a.push("--legacy-review".into());
            a.push(path.clone().into_os_string());
            a.push("--legacy-review-sha256".into());
            a.push(hash.as_str().into());
        }
        if let Some((path, hash)) = &self.dispatch_writer_review {
            a.push("--dispatch-writer-review".into());
            a.push(path.clone().into_os_string());
            a.push("--dispatch-writer-review-sha256".into());
            a.push(hash.as_str().into());
        }
        a
    }
}
fn open_root(config: &Config, require_pin: bool) -> Result<StableRoot> {
    if require_pin {
        ensure!(config.expected.is_some(), "probe and pin stable root first");
    }
    let expected = config.expected.as_ref().map(|i| RootIdentity {
        host_id: i.host_id.clone(),
        volume_id: i.volume_id.clone(),
        directory_id: i.directory_id.clone(),
    });
    Ok(StableRoot::open(&config.root, expected.as_ref())?)
}
fn read_required(dir: &PinnedDirectory<'_>, name: &str) -> Result<Vec<u8>> {
    dir.read_record(OsStr::new(name))?
        .context("required record missing")
}
fn record<T: serde::Serialize>(dir: &PinnedDirectory<'_>, name: &str, value: &T) -> Result<()> {
    dir.create_file_new(
        OsStr::new(name),
        &serde_json::to_vec(value)?,
        p::MAX_RECORD_BYTES as u64,
    )?;
    Ok(())
}
fn read_external(path: &Path, max: u64) -> Result<(Vec<u8>, PinnedFile)> {
    // Pin the parent through its own parent, retaining every ancestor in PinnedFile.
    let parent = path.parent().context("file parent")?;
    let above = StableRoot::open(parent.parent().context("file grandparent")?, None)?;
    let dir = above.directory(parent.file_name().context("parent name")?)?;
    let mut file = dir
        .pin_file(path.file_name().context("file name")?, max)?
        .context("file missing")?;
    let bytes = file.read_all(max)?;
    Ok((bytes, file))
}
fn observed_self() -> Result<(p::ProcessIdentity, PinnedFile)> {
    let path = std::env::current_exe()?;
    let (_, mut pin) = read_external(&path, p::MAX_PAYLOAD_BYTES)?;
    let image = file_identity(&mut pin)?;
    Ok((current_process_identity(image)?, pin))
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SupervisorReceipt {
    schema: String,
    dispatch_id: p::Digest,
    process: p::ProcessIdentity,
    launch_method: String,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminationReceipt {
    schema: String,
    dispatch_id: p::Digest,
    process: p::ProcessIdentity,
    observer: p::ProcessIdentity,
    exit_code: i32,
}
fn watched_identity(watch: &ProcessWatch, host: &str) -> Result<p::ProcessIdentity> {
    let path = watch.image_path()?;
    let (_, mut pin) = read_external(&path, p::MAX_PAYLOAD_BYTES)?;
    let result = p::ProcessIdentity {
        host_id: host.into(),
        pid: watch.identity().pid,
        creation_time_100ns: watch.identity().creation_time_100ns,
        image: file_identity(&mut pin)?,
    };
    result.validate()?;
    Ok(result)
}
fn reviewed_assessment(
    config: &Config,
    root: &StableRoot,
    id: &p::JobId,
    input: &p::HoldInput,
    writer_lock_held: bool,
) -> Result<(p::HoldAssessment, Option<Value>)> {
    let assessment = p::classify_job_hold(input, id, &root_identity(root));
    let Some((path, digest)) = &config.review else {
        return Ok((assessment, None));
    };
    let (bytes, _review_pin) = read_external(path, p::MAX_RECORD_BYTES as u64)?;
    ensure!(
        p::Digest::of(&bytes) == *digest,
        "control-plane review digest differs"
    );
    let envelope: Value = p::decode(&bytes)?;
    if envelope.get("schema").and_then(Value::as_str) == Some(p::ROOT_LEGACY_REVIEW_SCHEMA) {
        // Unrelated v2 states retain their ordinary rules. In particular a
        // reviewed old directory cannot exempt any new unresolved v2 job.
        let has_state_bytes = matches!(input, p::HoldInput::StateBytes(_));
        let reviewed_terminal = assessment.legacy_v1
            && matches!(
                assessment
                    .observed_record
                    .as_ref()
                    .and_then(|s| s.get("state"))
                    .and_then(Value::as_str),
                Some("COMPLETED" | "FAILED")
            );
        if !assessment.blocks_writes || (has_state_bytes && !reviewed_terminal) {
            return Ok((assessment, None));
        }
        let _status_lock = if writer_lock_held {
            None
        } else {
            Some(root.lock()?)
        };
        let (assessed, evidence) = crate::legacy_review::assess(root, id, input, &bytes, digest)?;
        return Ok((assessed, Some(evidence)));
    }
    let review: p::LegacyReconciliationReview = p::decode(&bytes)?;
    if review.job_id != *id {
        return Ok((assessment, None));
    }
    let dir = root.directory(OsStr::new(id.as_str()))?;
    let mut observed = Vec::new();
    let mut pins = Vec::new();
    for entry in &review.references {
        let mut pin = pin_relative_file(&dir, &entry.path, p::MAX_PAYLOAD_BYTES)?;
        observed.push(p::FileEntry {
            path: entry.path.clone(),
            identity: file_identity(&mut pin)?,
        });
        pins.push(pin);
    }
    let assessed =
        p::classify_reviewed_legacy(input, id, &root_identity(root), &bytes, digest, &observed)?;
    let evidence = json!({"job_id":id,"review_sha256":digest,"snapshot_digest":p::inventory_digest(&observed)?,"original_state":match input {p::HoldInput::StateBytes(b)=>Some(p::BlobIdentity::of(b)),_=>None},"result":"released_writer_hold_only","legacy_single_job_review":true});
    Ok((assessed, Some(evidence)))
}
struct DispatchAdmissionGuard {
    container: Option<PinnedFile>,
    reviews: Vec<DispatchAdmissionMember>,
}
struct DispatchAdmissionMember {
    review: p::VerifiedDispatchWriterReview,
    pin: PinnedFile,
    snapshot: Option<crate::dispatch_review::DispatchSnapshotGuard>,
}
impl DispatchAdmissionGuard {
    fn verify(&self, root: &StableRoot) -> Result<()> {
        if let Some(pin) = &self.container {
            pin.verify()?;
        }
        for member in &self.reviews {
            member.pin.verify()?;
            if let Some(snapshot) = &member.snapshot {
                snapshot.verify(root, member.review.job_id())?;
            }
        }
        Ok(())
    }
}
fn admit(
    config: &Config,
    lock: &WriterLock<'_>,
    except: Option<&p::JobId>,
    operation: &str,
    target: &p::JobId,
) -> Result<DispatchAdmissionGuard> {
    crate::transfer_endpoint::ensure_native_root(lock.root())?;
    let operation_kind = match operation {
        "stage" => p::WriterAdmissionOperation::Stage,
        "run" => p::WriterAdmissionOperation::Run,
        "supervise" => p::WriterAdmissionOperation::Supervise,
        _ => anyhow::bail!("unsupported writer-admission operation"),
    };
    // Mandatory binding BEFORE scan_holds and BEFORE the supervise except.
    let mut guard = DispatchAdmissionGuard {
        container: None,
        reviews: Vec::new(),
    };
    if let Some((path, digest)) = &config.dispatch_writer_review {
        let (bytes, pin) = read_external(path, p::MAX_RECORD_BYTES as u64)?;
        let value: Value = p::decode(&bytes)?;
        if value["schema"] == p::DISPATCH_WRITER_REVIEW_SET_SCHEMA {
            let set = p::VerifiedDispatchWriterReleaseSet::new(
                &bytes,
                digest,
                &root_identity(lock.root()),
            )?;
            let parent = path
                .parent()
                .context("dispatch review set parent missing")?;
            for entry in set.reviews() {
                // RelativePath forbids escapes. read_external retains normal-file
                // and ancestor identities; byte identity is checked independently.
                let (original, member_pin) = read_external(
                    &parent.join(entry.path.as_str()),
                    p::MAX_RECORD_BYTES as u64,
                )?;
                entry.identity.verify(&original)?;
                let review = p::VerifiedDispatchWriterReview::new(
                    &original,
                    &entry.identity.sha256,
                    &root_identity(lock.root()),
                )?;
                guard.reviews.push(DispatchAdmissionMember {
                    review,
                    pin: member_pin,
                    snapshot: None,
                });
            }
            guard.container = Some(pin);
            p::bind_dispatch_writer_review_set(
                operation_kind,
                target,
                &guard
                    .reviews
                    .iter()
                    .map(|member| &member.review)
                    .collect::<Vec<_>>(),
            )?;
        } else {
            // The original single-review protocols keep their own validation.
            let review =
                p::VerifiedDispatchWriterReview::new(&bytes, digest, &root_identity(lock.root()))?;
            review.bind_admission(operation_kind, target)?;
            guard.reviews.push(DispatchAdmissionMember {
                review,
                pin,
                snapshot: None,
            });
        }
    }
    let mut observations = Vec::new();
    let mut blocked = None;
    for (facts, raw) in scan_holds(lock)? {
        if except.is_some_and(|id| facts.name == OsStr::new(id.as_str())) {
            continue;
        }
        let (assessment, evidence) = if let Some(name) = facts.name.to_str() {
            if let Ok(id) = p::JobId::new(name) {
                match reviewed_assessment(config, lock.root(), &id, &hold_input(&facts), true) {
                    Ok(value) => value,
                    Err(error) => {
                        let explanation: String = format!("{error:#}").chars().take(240).collect();
                        (
                            raw,
                            Some(
                                json!({"job_id":id,"result":"review_not_granted","error":explanation}),
                            ),
                        )
                    }
                }
            } else {
                (raw, None)
            }
        } else {
            (raw, None)
        };
        let mut effective_blocks = assessment.blocks_writes;
        let mut dispatch_release = None;
        if let Some(member) = guard
            .reviews
            .iter_mut()
            .find(|member| facts.name == OsStr::new(member.review.job_id().as_str()))
            && effective_blocks
        {
            let review = &member.review;
            let observed = (|| -> Result<_> {
                let snapshot =
                    crate::dispatch_review::DispatchSnapshotGuard::observe(lock, review)?;
                let release =
                    review.assess(operation_kind, target, &hold_input(&facts), &snapshot.facts)?;
                Ok((snapshot, release))
            })();
            match observed {
                Ok((snapshot, release)) => {
                    effective_blocks = false;
                    dispatch_release = Some(serde_json::to_value(release)?);
                    member.snapshot = Some(snapshot);
                }
                Err(error) => {
                    let explanation: String = format!("{error:#}").chars().take(240).collect();
                    dispatch_release = Some(json!({"result":"not_released","error":explanation}));
                }
            }
        }
        observations.push(json!({"directory":facts.name.to_string_lossy(),"blocks_writes":effective_blocks,"original_blocks_writes":assessment.blocks_writes,"reason":assessment.reason,"reviewed_hold":evidence,"dispatch_writer_release":dispatch_release}));
        if effective_blocks {
            blocked = Some(format!(
                "root writer hold: {:?}: {}",
                facts.name, assessment.reason
            ));
            break;
        }
    }
    if guard.reviews.iter().any(|member| member.snapshot.is_none()) && blocked.is_none() {
        blocked = Some(
            if guard.container.is_some() {
                "explicit dispatch writer review did not observe and release every exact job"
            } else {
                "explicit dispatch writer review did not observe and release its exact job"
            }
            .into(),
        );
    }
    guard.verify(lock.root())?;
    // The service's inherited stderr is already retained by the transport.
    // Never proceed with Stage/Run side effects if this evidence cannot flush.
    // A WMI supervisor has no guaranteed stderr; its existing durable process
    // and termination records bind the unchanged repeated admission logic.
    if operation != "supervise" {
        let (controller, _pin) = observed_self()?;
        let receipt = json!({"schema":"cad-agent.nx-root-admission/v1","scope":"writer_lock_admission_only","operation":operation,"target_job_id":target,"root":root_identity(lock.root()),"controller":controller,"review_sha256":config.review.as_ref().map(|(_,d)|d),"dispatch_writer_review_sha256":config.dispatch_writer_review.as_ref().map(|(_,d)|d),"result":if blocked.is_some(){"blocked"}else{"admitted"},"observations":observations});
        let mut stream = std::io::stderr().lock();
        serde_json::to_writer(&mut stream, &receipt)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
    }
    if let Some(reason) = blocked {
        anyhow::bail!(reason);
    }
    Ok(guard)
}
fn payload(
    root: &StableRoot,
    dir: &PinnedDirectory<'_>,
    job: &p::JobManifest,
) -> Result<(Vec<p::FileEntry>, Vec<PinnedFile>)> {
    let (_, file_bound, _) = job.inventory_bounds(false)?;
    let mut entries = Vec::new();
    let mut pins = Vec::new();
    let origin = if job.schema == p::LARGE_JOB_SCHEMA {
        let state: p::StateRecord = p::decode(&read_required(dir, STATE_NAME)?)?;
        Some(crate::published_origin::PublishedOrigin::observe(
            root, dir, job, &state,
        )?)
    } else {
        None
    };
    for entry in job.payload_files() {
        let mut pin = pin_relative_file(dir, &entry.path, file_bound)?;
        entries.push(p::FileEntry {
            path: entry.path,
            identity: file_identity(&mut pin)?,
        });
        pins.push(pin);
    }
    job.verify_payload(&entries)?;
    // Whole payload trees are inventoried to reject undeclared additional members.
    for prefix in ["worker", "sources", "build", "inputs"] {
        let expected: Vec<_> = entries
            .iter()
            .filter(|e| e.path.as_str().starts_with(&format!("{prefix}/")))
            .cloned()
            .collect();
        let child = match dir.directory(OsStr::new(prefix)) {
            Ok(v) => v,
            Err(e) if e.raw_os_error() == Some(2) && expected.is_empty() => continue,
            Err(e) => return Err(e.into()),
        };
        let (actual, extra_pins) = inventory_job_tree(&child, &format!("{prefix}/"), job, false)?;
        let mut expected = expected;
        let mut actual = actual;
        expected.sort_by(|a, b| a.path.cmp(&b.path));
        actual.sort_by(|a, b| a.path.cmp(&b.path));
        ensure!(expected == actual, "payload_directory_inventory_differs");
        pins.extend(extra_pins);
    }

    for child in std::fs::read_dir(dir.path())? {
        let child = child?;
        let name = child.file_name();
        let name = name.to_str().context("job member encoding")?;
        ensure!(
            [
                "worker",
                "sources",
                "build",
                "inputs",
                "params.json",
                "job.json",
                "state.json",
                "stage-receipt.json",
                "dispatch.json",
                "native-result.json",
                "native-result.pending.json",
                "worker-pre-native-unknown.json",
                "out",
                "receipts"
            ]
            .contains(&name)
                || (job.schema == p::LARGE_JOB_SCHEMA && name == "source-plan.json")
                || origin.as_ref().is_some_and(|o| o.admits_history(name)),
            "undeclared job root member"
        );
    }
    if let Some(origin) = origin {
        origin.verify_directory(dir)?;
        pins.extend(origin.into_pins());
    }
    Ok((entries, pins))
}
fn runtime(config: &Config, job: &p::JobManifest) -> Result<(p::RuntimeIdentity, Vec<PinnedFile>)> {
    let root = StableRoot::open(&config.nx_root, None)?;
    let mut runtime = job.worker.required_runtime.clone();
    let mut observed = Vec::new();
    let mut pins = Vec::new();
    for entry in &runtime.libraries {
        let parts: Vec<_> = entry.path.as_str().split('/').collect();
        ensure!(parts.len() >= 2, "runtime entry requires subdirectory");
        let dir = root.directory(OsStr::new(parts[0]))?;
        let relative = p::RelativePath::new(parts[1..].join("/"))?;
        let mut pin = pin_relative_file(&dir, &relative, 2 * 1024 * 1024 * 1024)?;
        observed.push(p::FileEntry {
            path: entry.path.clone(),
            identity: file_identity(&mut pin)?,
        });
        pins.push(pin);
    }
    ensure!(
        observed == runtime.libraries,
        "installed runtime bytes differ"
    );
    runtime.libraries = observed;
    runtime.validate()?;
    Ok((runtime, pins))
}
/// Reconcile only the same dispatched worker from durable native-handle exit
/// evidence and its bound supervisor/source. Missing evidence remains a hold.
/// Pending-write recovery is deliberately not inferred or auto-deleted here.
fn reconcile_same_job(config: &Config, root: &StableRoot, id: &p::JobId) -> Result<bool> {
    let lock = match root.lock() {
        Ok(v) => v,
        Err(e) if e.raw_os_error() == Some(32) => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    let dir = root.directory(OsStr::new(id.as_str()))?;
    let facts = observe_job_records(root, OsStr::new(id.as_str()));
    if facts.pending.is_some()
        || facts.error.is_some()
        || !facts.unexpected_control_files.is_empty()
    {
        return Ok(false);
    }
    let old = read_required(&dir, STATE_NAME)?;
    let Ok(mut state) = p::decode::<p::StateRecord>(&old) else {
        return Ok(false);
    };
    if !matches!(
        state.state,
        p::State::Running | p::State::UnknownAfterDispatch
    ) {
        return Ok(false);
    }
    let job: p::JobManifest = p::decode(&read_required(&dir, "job.json")?)?;
    state.validate(&job)?;
    ensure!(
        job.job_id == *id && job.root == root_identity(root),
        "reconciliation root/job differs"
    );
    let Some(process) = state.process.clone() else {
        return Ok(false);
    };
    let intent = state.dispatch.as_ref().context("dispatch absent")?;
    let physical_intent: p::DispatchIntent = p::decode(&read_required(&dir, "dispatch.json")?)?;
    ensure!(
        &physical_intent == intent,
        "reconciliation physical dispatch differs"
    );
    let receipts = dir.directory(OsStr::new("receipts"))?;
    let Some(terminated) = receipts.read_record(OsStr::new("worker-termination.json"))? else {
        return Ok(false);
    };
    let termination: TerminationReceipt = p::decode(&terminated)?;
    let supervisor: SupervisorReceipt =
        p::decode(&read_required(&receipts, "supervisor-process.json")?)?;
    supervisor.process.validate()?;
    termination.process.validate()?;
    termination.observer.validate()?;
    ensure!(
        supervisor.launch_method == "native_COM_Win32_Process_Create",
        "supervisor launch source differs"
    );
    ensure!(
        termination.schema == "cad-agent.nx-native-termination/v2"
            && supervisor.schema == "cad-agent.nx-supervisor-process/v2"
            && termination.dispatch_id == intent.dispatch_id
            && supervisor.dispatch_id == intent.dispatch_id
            && termination.process == process
            && termination.observer == supervisor.process
            && termination.observer.host_id == job.root.host_id
            && termination.observer.image == intent.controller.image,
        "termination observer/source binding differs"
    );
    let (passed_payload, _pins) = payload(&root, &dir, &job)?;
    let (installed_runtime, _runtime_pins) = runtime(config, &job)?;
    let out = dir.directory(OsStr::new("out"))?;
    let (artifacts, _artifact_pins) = inventory_job_tree(&out, "out/", &job, true)?;
    let result: p::NativeResult = p::decode(&read_required(&dir, "native-result.json")?)?;
    let evidence = p::CompletionEvidence {
        process,
        exit_code: Some(termination.exit_code),
        runtime: installed_runtime,
        staged_payload: passed_payload,
        artifacts,
    };
    if state.state == p::State::Running {
        state.finish(&job, &result, &evidence)?;
    } else if !state.reconcile(&job, &result, &evidence)? {
        return Ok(false);
    }
    persist_state(&dir, &lock, &job, Some(&old), &state)?;
    Ok(true)
}
fn status(config: &Config, root: &StableRoot, id: &p::JobId) -> Result<Value> {
    // Errors never grant a transition; report the still-persisted source state.
    let _ = reconcile_same_job(config, root, id);
    let unknown = |bytes: Option<&[u8]>, reason: &str| -> Result<Value> {
        Ok(serde_json::to_value(p::UnknownStatusResponse {
            schema: p::UNKNOWN_STATUS_RESPONSE_SCHEMA.into(),
            job_id: id.clone(),
            root: root_identity(root),
            state_file: bytes.map(p::BlobIdentity::of),
            blocks_writes: true,
            reason: reason.into(),
            automatic_retry: false,
        })?)
    };
    let facts = observe_job_records(root, OsStr::new(id.as_str()));
    let observed = facts.state.as_deref();
    let input = hold_input(&facts);
    // A missing job record remains unknown. Root admission is a distinct,
    // structured observation emitted by Stage/Run; never invent a job state.
    let assessment = match reviewed_assessment(config, root, id, &input, false) {
        Ok((v, _)) => v,
        Err(_) => return unknown(observed, "historical_review_observation_incomplete"),
    };
    let Some(raw) = assessment.observed_record.clone() else {
        return unknown(observed, &assessment.reason);
    };
    let bytes = observed.context("known state lacks source bytes")?;
    let response = p::StatusResponse {
        schema: p::STATUS_RESPONSE_SCHEMA.into(),
        job_id: id.clone(),
        root: root_identity(root),
        state: raw,
        state_file: p::BlobIdentity::of(bytes),
        blocks_writes: assessment.blocks_writes,
        hold_reason: assessment.reason,
        legacy_read_only: assessment.legacy_v1,
        automatic_retry: false,
    };
    if response.validate_binding(id, &root_identity(root)).is_err() {
        return unknown(Some(bytes), "state_record_binding_invalid");
    }
    Ok(serde_json::to_value(response)?)
}
fn stage(
    config: &Config,
    root: &StableRoot,
    job: &p::JobManifest,
    archive: &p::BlobIdentity,
    bytes: &[u8],
) -> Result<Value> {
    ensure!(job.root == root_identity(root), "stage root differs");
    archive.verify(bytes)?;
    let package = p::bundle::from_archive(bytes)?;
    ensure!(package.job == *job, "stage manifest differs");
    let lock = root.lock()?;
    let admission = admit(config, &lock, None, "stage", &job.job_id)?;
    admission.verify(root)?;
    let dir = root.create_directory_new(OsStr::new(job.job_id.as_str()))?;
    let mut state = p::StateRecord::prepared(job)?;
    persist_state(&dir, &lock, job, None, &state)?;
    for (name, bytes) in &package.members {
        create_relative_new(&dir, &p::RelativePath::new(name.clone())?, bytes)?;
    }
    let (observed, _pins) = payload(root, &dir, job)?;
    job.verify_payload(&observed)?;
    let receipt = p::StageReceipt {
        schema: p::STAGE_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: root_identity(root),
        manifest_digest: job.digest()?,
        payload_digest: p::inventory_digest(&observed)?,
        archive: archive.clone(),
    };
    record(&dir, "stage-receipt.json", &receipt)?;
    let previous = read_required(&dir, STATE_NAME)?;
    state.mark_staged(job, &receipt, archive)?;
    persist_state(&dir, &lock, job, Some(&previous), &state)?;
    Ok(
        json!({"schema":"cad-agent.nx-stage-response/v2","receipt":receipt,"state":state,"state_file":p::BlobIdentity::of(&read_required(&dir,STATE_NAME)?)}),
    )
}
fn dispatch(
    config: &Config,
    root: &StableRoot,
    id: &p::JobId,
    digest: &p::Digest,
    wait: u16,
) -> Result<Value> {
    let lock = root.lock()?;
    let admission = admit(config, &lock, None, "run", id)?;
    let dir = root.directory(OsStr::new(id.as_str()))?;
    let job: p::JobManifest = p::decode(&read_required(&dir, "job.json")?)?;
    ensure!(job.digest()? == *digest, "run manifest differs");
    let old = read_required(&dir, STATE_NAME)?;
    let mut state: p::StateRecord = p::decode(&old)?;
    state.validate(&job)?;
    let (_observed, _pins) = payload(&root, &dir, &job)?;
    let (_runtime, _runtime_pins) = runtime(config, &job)?;
    let (controller, _self_pin) = observed_self()?;
    let dispatch_id = p::Digest::of(&random_nonce()?);
    let intent = p::DispatchIntent::new(&job, controller, dispatch_id.clone())?;
    let origin = if job.schema == p::LARGE_JOB_SCHEMA {
        let origin = crate::published_origin::PublishedOrigin::observe(root, &dir, &job, &state)?;
        origin.begin_dispatch(&dir, &job, &mut state, intent.clone())?;
        Some(origin)
    } else {
        state.begin_dispatch(&job, intent.clone())?;
        None
    };
    admission.verify(root)?;
    persist_state(&dir, &lock, &job, Some(&old), &state)?;
    if let Some(origin) = origin.as_ref() {
        origin.verify_directory(&dir)?;
    }
    let attempt = (|| -> Result<()> {
        record(&dir, "dispatch.json", &intent)?;
        let receipts = dir.create_directory_new(OsStr::new("receipts"))?;
        dir.create_directory_new(OsStr::new("out"))?;
        let mut arguments = config.args();
        arguments.extend([
            "--supervise".into(),
            id.as_str().into(),
            "--dispatch-id".into(),
            dispatch_id.as_str().into(),
        ]);
        let watch =
            crate::wmi::spawn_supervisor(&std::env::current_exe()?, &arguments, dir.path())?;
        let process = watched_identity(&watch, &job.root.host_id)?;
        ensure!(
            process.image == intent.controller.image,
            "supervisor executable differs"
        );
        record(
            &receipts,
            "supervisor-process.json",
            &SupervisorReceipt {
                schema: "cad-agent.nx-supervisor-process/v2".into(),
                dispatch_id: dispatch_id.clone(),
                process,
                launch_method: "native_COM_Win32_Process_Create".into(),
            },
        )?;
        Ok(())
    })();
    if attempt.is_err() {
        let old = read_required(&dir, STATE_NAME)?;
        state.mark_unknown("supervisor_dispatch_or_receipt_uncertain")?;
        persist_state(&dir, &lock, &job, Some(&old), &state)?;
        return status(config, root, id);
    }
    drop(lock);
    let start = Instant::now();
    loop {
        let value = status(config, root, id)?;
        let tag = value.pointer("/state/state").and_then(Value::as_str);
        if !matches!(tag, Some("STARTING" | "RUNNING"))
            || start.elapsed() >= Duration::from_secs(wait.into())
        {
            return Ok(value);
        }
        thread::sleep(Duration::from_millis(100));
    }
}
fn supervise(config: &Config, id: &p::JobId, nonce: &p::Digest) -> Result<()> {
    let root = open_root(config, true)?;
    let start = Instant::now();
    let lock = loop {
        match root.lock() {
            Ok(lock) => break lock,
            Err(e) if e.raw_os_error() == Some(32) && start.elapsed() < Duration::from_secs(30) => {
                thread::sleep(Duration::from_millis(20))
            }
            Err(e) => return Err(e.into()),
        }
    };
    let admission = admit(config, &lock, Some(id), "supervise", id)?;
    let dir = root.directory(OsStr::new(id.as_str()))?;
    let job: p::JobManifest = p::decode(&read_required(&dir, "job.json")?)?;
    let mut state: p::StateRecord = p::decode(&read_required(&dir, STATE_NAME)?)?;
    state.validate(&job)?;
    ensure!(
        state.state == p::State::Starting
            && state
                .dispatch
                .as_ref()
                .is_some_and(|i| &i.dispatch_id == nonce),
        "supervisor single-use intent differs"
    );
    let intent: p::DispatchIntent = p::decode(&read_required(&dir, "dispatch.json")?)?;
    ensure!(
        Some(&intent) == state.dispatch.as_ref(),
        "durable dispatch differs"
    );
    ensure!(
        dir.read_record(OsStr::new(PENDING_NAME))?.is_none(),
        "pending write before native dispatch"
    );
    ensure!(
        job.root == root_identity(&root) && job.job_id == *id,
        "supervisor job root differs"
    );
    let receipts = dir.directory(OsStr::new("receipts"))?;
    let supervisor: SupervisorReceipt =
        p::decode(&read_required(&receipts, "supervisor-process.json")?)?;
    let (observer, _observer_pin) = observed_self()?;
    ensure!(
        supervisor.schema == "cad-agent.nx-supervisor-process/v2"
            && supervisor.dispatch_id == *nonce
            && supervisor.process == observer
            && observer.image == intent.controller.image,
        "supervisor durable process/source binding differs"
    );
    let run = (|| -> Result<()> {
        let (observed, pins) = payload(&root, &dir, &job)?;
        let (actual_runtime, runtime_pins) = runtime(config, &job)?;
        let receipts = dir.directory(OsStr::new("receipts"))?;
        let worker = pin_relative_file(&dir, &job.worker.executable.path, p::MAX_PAYLOAD_BYTES)?;
        admission.verify(&root)?;
        let worker_path = crate::launch_environment::worker_search_path(
            &config.nx_root,
            std::env::var_os("PATH"),
        )?;
        let launch_environment = serde_json::json!({
            "schema":"oev2.nx-worker-launch-environment/1","nx_bin":config.nx_root.join("NXBIN"),
            "child_path_sha256":p::BlobIdentity::of(worker_path.to_string_lossy().as_bytes()).sha256,
            "job_id":job.job_id,"dispatch_id":intent.dispatch_id,
            "scope":"worker child PATH only; parent unchanged; NXBIN variable inherited; persisted after worker exit to retain the closed initial-receipt contract",
            "root_cause":"launch-environment hypothesis; sole cause not established"
        });
        let mut child = Command::new(worker.path())
            .env("PATH", worker_path)
            .arg("--job-dir")
            .arg(dir.path())
            .arg("--nx-root")
            .arg(&config.nx_root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                receipts.create_log_new(OsStr::new("worker.stdout"))?,
            ))
            .stderr(Stdio::from(
                receipts.create_log_new(OsStr::new("worker.stderr"))?,
            ))
            .spawn()?;
        let watch = ProcessWatch::from_child(&child)?;
        let native = watch.identity();
        let process = p::ProcessIdentity {
            host_id: root_identity(&root).host_id,
            pid: native.pid,
            creation_time_100ns: native.creation_time_100ns,
            image: job.worker.executable.identity.clone(),
        };
        let old = read_required(&dir, STATE_NAME)?;
        state.mark_running(&job, process.clone())?;
        persist_state(&dir, &lock, &job, Some(&old), &state)?;
        let code = loop {
            match watch.poll()? {
                ProcessObservation::Running => thread::sleep(Duration::from_millis(100)),
                ProcessObservation::Exited { exit_code } => break exit_code,
            }
        };
        let _status = child.wait()?;
        record(
            &receipts,
            "worker-launch-environment.json",
            &launch_environment,
        )?;
        record(
            &receipts,
            "worker-termination.json",
            &TerminationReceipt {
                schema: "cad-agent.nx-native-termination/v2".into(),
                dispatch_id: nonce.clone(),
                process: process.clone(),
                observer: observer.clone(),
                exit_code: code as i32,
            },
        )?;
        for pin in pins.iter().chain(runtime_pins.iter()) {
            pin.verify()?;
        }
        let out = dir.directory(OsStr::new("out"))?;
        let (artifacts, _artifact_pins) = inventory_job_tree(&out, "out/", &job, true)?;
        let result: p::NativeResult = p::decode(&read_required(&dir, "native-result.json")?)?;
        let evidence = p::CompletionEvidence {
            process,
            exit_code: Some(code as i32),
            runtime: actual_runtime,
            staged_payload: observed,
            artifacts,
        };
        let old = read_required(&dir, STATE_NAME)?;
        state.finish(&job, &result, &evidence)?;
        persist_state(&dir, &lock, &job, Some(&old), &state)?;
        Ok(())
    })();
    if let Err(error) = run {
        if let Ok(receipts) = dir.directory(OsStr::new("receipts")) {
            let message = error.to_string();
            let message: String = message.chars().take(2000).collect();
            let _ = record(
                &receipts,
                "supervisor-error.json",
                &json!({"schema":"cad-agent.nx-supervisor-error/v2","dispatch_id":nonce,"error":message,"worker_killed":false,"automatic_retry":false}),
            );
        }
        if matches!(state.state, p::State::Starting | p::State::Running) {
            let old = read_required(&dir, STATE_NAME)?;
            state.mark_unknown("native_execution_or_completion_observation_uncertain")?;
            persist_state(&dir, &lock, &job, Some(&old), &state)?;
        }
        return Err(error);
    }
    Ok(())
}
fn stage_published(
    config: &Config,
    root: &StableRoot,
    request: &p::published_stage::PublishedStageRequest,
) -> Result<Value> {
    request.validate()?;
    ensure!(
        request.job.root == root_identity(root),
        "published_stage_root_differs"
    );
    let lock = root.lock()?;
    let admission = if request.operation == p::published_stage::StageOperation::Inspect {
        None
    } else {
        Some(admit(
            config,
            &lock,
            if request.operation == p::published_stage::StageOperation::Recover {
                Some(&request.job.job_id)
            } else {
                None
            },
            "stage",
            &request.job.job_id,
        )?)
    };
    if let Some(guard) = &admission {
        guard.verify(root)?;
    }
    let response = crate::published_stage_endpoint::execute(root, &lock, request)?;
    if let Some(guard) = &admission {
        guard.verify(root)?;
    }
    Ok(serde_json::to_value(response)?)
}
pub fn main_entry() -> Result<()> {
    let config = Config::parse(std::env::args_os().skip(1))?;
    if let Some((id, nonce)) = &config.supervise {
        return supervise(&config, id, nonce);
    }
    let (request, body) = p::read_request(&mut std::io::stdin().lock())?;
    let result = (|| -> Result<p::Frame> {
        let root = open_root(&config, !matches!(request, p::Request::Probe {}))?;
        if let p::Request::FetchBatch { request_record } = &request {
            return crate::large_fetch::batch(&root, &p::decode(request_record.as_bytes())?);
        }
        if let p::Request::FetchSnapshot {
            job_id,
            manifest_digest,
        } = &request
        {
            return crate::large_fetch::snapshot(&root, job_id, manifest_digest);
        }
        if let p::Request::FetchRange { request_record } = &request {
            return crate::large_fetch::range(&root, &p::decode(request_record.as_bytes())?);
        }
        let mut payload_bytes = Vec::new();
        let response = match request {
            p::Request::FetchSnapshot { .. }
            | p::Request::FetchRange { .. }
            | p::Request::FetchBatch { .. } => unreachable!("read-only fetch handled above"),
            p::Request::StagePublished { request_record } => stage_published(
                &config,
                &root,
                &p::published_stage::decode_request(&request_record)?,
            )?,
            request @ (p::Request::TransferBegin { .. }
            | p::Request::TransferPublish { .. }
            | p::Request::TransferInspectPublication { .. }
            | p::Request::TransferRecoverPublication { .. }
            | p::Request::TransferReconstruct { .. }
            | p::Request::TransferPut { .. }
            | p::Request::TransferPutBatch { .. }
            | p::Request::TransferInspect { .. }
            | p::Request::TransferCommit { .. }
            | p::Request::TransferVerify { .. }) => {
                serde_json::to_value(crate::transfer_endpoint::execute(&root, &request, &body)?)?
            }
            p::Request::Probe {} => {
                json!({"schema":"cad-agent.nx-probe/v2","root":root_identity(&root),"controller":observed_self()?.0,"native_execution_started":false,"stage_run_requires_expected_root":true})
            }
            p::Request::Stage { job, archive } => stage(&config, &root, &job, &archive, &body)?,
            p::Request::Run {
                job_id,
                manifest_digest,
                wait_seconds,
            } => dispatch(&config, &root, &job_id, &manifest_digest, wait_seconds)?,
            p::Request::Status { job_id } => status(&config, &root, &job_id)?,
            p::Request::Fetch { job_id } => {
                // Read-only fetch must not pin mutable state during a live writer.
                let _snapshot_lock = root.lock()?;
                let dir = root.directory(OsStr::new(job_id.as_str()))?;
                let (mut entries, mut pins) = inventory_tree(&dir, "", p::MAX_PAYLOAD_BYTES)?;
                let mut members = BTreeMap::new();
                for (entry, pin) in entries.iter().zip(pins.iter_mut()) {
                    members.insert(
                        entry.path.as_str().to_owned(),
                        pin.read_all(p::MAX_PAYLOAD_BYTES)?,
                    );
                }
                let state_file = members
                    .get(STATE_NAME)
                    .map(|bytes| p::BlobIdentity::of(bytes));
                let envelope = serde_json::to_vec(
                    &json!({"schema":"oev2.nx-directory-envelope/1","job_id":job_id,"root":root_identity(&root),
                    "directories":directory_layout(&dir)?,"origin":"read_only_pinned_controller_observation","native_state_modified":false}),
                )?;
                p::bundle::insert(
                    &mut members,
                    "oev2-directory-envelope.json",
                    envelope.clone(),
                )?;
                entries.push(p::FileEntry::from_bytes(
                    "oev2-directory-envelope.json",
                    &envelope,
                )?);
                entries.sort_by(|a, b| a.path.as_str().cmp(b.path.as_str()));
                payload_bytes = p::bundle::archive_members(&members)?;
                json!({"schema":"cad-agent.nx-fetch/v2","job_id":job_id,"root":root_identity(&root),"archive":p::BlobIdentity::of(&payload_bytes),"members":entries,"state_file":state_file})
            }
        };
        Ok(p::Frame {
            record: response,
            payload: payload_bytes,
        })
    })();
    match result {
        Ok(frame) => p::write_frame(&mut std::io::stdout().lock(), &frame),
        Err(error) => {
            let mut label = String::new();
            for ch in error.to_string().chars() {
                let ch = if ch.is_control() { ' ' } else { ch };
                if label.len() + ch.len_utf8() > 240 {
                    break;
                }
                label.push(ch);
            }
            p::write_frame(
                &mut std::io::stdout().lock(),
                &p::Frame {
                    record: json!({"schema":"cad-agent.nx-error/v2","error":label,"automatic_retry":false}),
                    payload: Vec::new(),
                },
            )?;
            Ok(())
        }
    }
}
