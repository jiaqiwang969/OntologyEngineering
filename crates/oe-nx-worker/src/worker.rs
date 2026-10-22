use crate::{
    geometry::{self, Parameters, Workspace},
    native::Native,
};
use anyhow::{Context, Result, ensure};
use oe_nx_protocol as p;
use oe_windows_controller::{
    adapter,
    windows::{PinnedDirectory, PinnedFile, StableRoot},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn pending_observed(directory: &PinnedDirectory<'_>) -> Result<bool> {
    match directory.read_record(OsStr::new(oe_windows_controller::windows::PENDING_NAME)) {
        Ok(value) => Ok(value.is_some()),
        // The controller flushes a newly created marker with share=0. That
        // bounded writer interval is pending, not proof of a missing marker.
        Err(error) if error.raw_os_error() == Some(32) => Ok(true),
        Err(error) => Err(error.into()),
    }
}

fn await_running(
    directory: &PinnedDirectory<'_>,
    job: &p::JobManifest,
    intent: &p::DispatchIntent,
    process: &p::ProcessIdentity,
    origin: Option<&oe_windows_controller::published_origin::PublishedOrigin>,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        ensure!(Instant::now() < deadline, "native_gate_running_deadline");
        if pending_observed(directory)? {
            std::thread::sleep(Duration::from_millis(25));
            continue;
        }
        let raw = directory
            .read_record(OsStr::new("state.json"))?
            .context("native_gate_state_missing")?;
        let state: p::StateRecord = p::decode(&raw)?;
        state.validate(job)?;
        if let Some(origin) = origin {
            origin.validate_state(job, &state)?;
            origin.verify_directory(directory)?;
        }
        ensure!(
            state.dispatch.as_ref() == Some(intent),
            "native_gate_dispatch_differs"
        );
        match state.state {
            p::State::Running => {
                ensure!(
                    state.process.as_ref() == Some(process),
                    "native_gate_process_identity_differs"
                );
                // A pending write may start after our first observation. Only
                // admit a stable RUNNING record between absent pending markers.
                if !pending_observed(directory)?
                    && directory.read_record(OsStr::new("state.json"))?.as_deref()
                        == Some(raw.as_slice())
                    && !pending_observed(directory)?
                {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            p::State::Starting => {
                std::thread::sleep(Duration::from_millis(25));
            }
            _ => anyhow::bail!("native_gate_state_forbids_execution"),
        }
    }
}

fn initial_output_directories(
    out: &PinnedDirectory<'_>,
    receipts: &PinnedDirectory<'_>,
) -> Result<()> {
    out.verify()?;
    ensure!(
        std::fs::read_dir(out.path())?.next().is_none(),
        "native_output_directory_not_initially_empty"
    );
    receipts.verify()?;
    for entry in std::fs::read_dir(receipts.path())? {
        let entry = entry?;
        let name = entry.file_name();
        ensure!(
            ["supervisor-process.json", "worker.stdout", "worker.stderr"]
                .iter()
                .any(|allowed| name == OsStr::new(allowed)),
            "unknown_initial_controller_receipt"
        );
        let metadata = std::fs::symlink_metadata(entry.path())?;
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.is_file() && metadata.file_attributes() & 0x400 == 0,
            "initial_controller_receipt_not_regular"
        );
    }
    out.verify()?;
    receipts.verify()?;
    Ok(())
}

fn vendor_path(path: &Path) -> Result<PathBuf> {
    let s = path.to_str().context("native_path_not_unicode")?;
    Ok(PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(s)))
}
fn safe_error(e: &anyhow::Error) -> String {
    format!("{e}")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(240)
        .collect()
}
fn bytes<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    let raw = serde_json::to_vec(value)?;
    ensure!(
        raw.len() <= p::MAX_RECORD_BYTES,
        "native_record_exceeds_bound"
    );
    Ok(raw)
}
fn read_pinned<T: serde::de::DeserializeOwned>(
    dir: &PinnedDirectory<'_>,
    name: &str,
    pins: &mut Vec<PinnedFile>,
) -> Result<T> {
    let mut file = dir
        .pin_file(OsStr::new(name), p::MAX_RECORD_BYTES as u64)?
        .context("native_control_record_missing")?;
    let data = file.read_all(p::MAX_RECORD_BYTES as u64)?;
    let result = p::decode(&data)?;
    pins.push(file);
    Ok(result)
}
fn gather(
    dir: &PinnedDirectory<'_>,
    prefix: &str,
    job: &p::JobManifest,
    output: bool,
    entries: &mut Vec<p::FileEntry>,
    pins: &mut Vec<PinnedFile>,
) -> Result<()> {
    let (observed, retained) =
        adapter::inventory_job_tree(dir, &format!("{prefix}/"), job, output)?;
    entries.extend(observed);
    pins.extend(retained);
    job.validate_inventory_capacity(entries, output)
}
struct TaskWorkspace<'a, 'root> {
    job: &'a p::JobManifest,
    job_dir: &'a PinnedDirectory<'root>,
    out: &'a PinnedDirectory<'a>,
    receipts: &'a PinnedDirectory<'a>,
    input_paths: BTreeMap<String, PathBuf>,
    motion_evidence_dir: Option<PinnedDirectory<'a>>,
    motion_copy: Option<crate::motion_source::windows::OwnedAssembly<'a>>,
    event_count: u32,
    events: Vec<Value>,
}
impl TaskWorkspace<'_, '_> {
    fn write_record(
        &mut self,
        phase: &str,
        observation: Value,
    ) -> Result<oe_contracts::ArtifactReference> {
        p::checked_token(phase)?;
        let observation = if self.job.schema == "oev2.nx-native-job/3"
            && serde_json::to_vec(&observation)?.len() > p::MAX_RECORD_BYTES - 1024
        {
            let job = self.job.job_id.as_str().to_owned();
            let field = crate::event_index::persist_field(
                &job,
                "observation",
                &observation,
                &mut |chunk| self.write_record("readback_field_chunk", chunk),
            )?;
            json!({"schema":crate::event_index::OBSERVATION_SCHEMA,"job_id":job,"field":field})
        } else {
            observation
        };

        let sequence = self
            .event_count
            .checked_add(1)
            .context("native_event_count_overflow")?;
        let record = json!({"schema":"cad-agent.nx-native-event/v2","job_id":self.job.job_id,"sequence":sequence,"phase":phase,"observation":observation});
        let raw = bytes(&record)?;
        let name = format!("{sequence:04}-{phase}.json");
        self.receipts
            .create_file_new(OsStr::new(&name), &raw, p::MAX_RECORD_BYTES as u64)?;
        self.event_count = sequence;
        self.events.push(if matches!(phase,"native_load_status"|"source_assembly_before_save"|"source_assembly_after_save"|"readback_field_chunk") {
            // Full diagnostics are already durable. Keep an exact reference in
            // partial readback instead of duplicating large arrays into the result.
            json!({"schema":"cad-agent.nx-native-event-reference/v2","job_id":self.job.job_id,"sequence":sequence,"phase":phase,"record":{"path":format!("receipts/{name}"),"identity":p::BlobIdentity::of(&raw)}})
        } else { record });
        Ok(oe_contracts::ArtifactReference {
            path: format!("receipts/{name}"),
            sha256: p::BlobIdentity::of(&raw).sha256.as_str().to_owned(),
        })
    }
    fn event_references(&self) -> Result<Vec<Value>> {
        self.events.iter().map(|record| {
            if record["schema"] == "cad-agent.nx-native-event-reference/v2" {
                return Ok(record.clone());
            }
            let sequence = record["sequence"].as_u64().context("event_sequence_missing")?;
            let phase = record["phase"].as_str().context("event_phase_missing")?;
            Ok(json!({"schema":"cad-agent.nx-native-event-reference/v2","job_id":self.job.job_id,"sequence":sequence,"phase":phase,"record":{"path":format!("receipts/{sequence:04}-{phase}.json"),"identity":p::BlobIdentity::of(&bytes(record)?)}}))
        }).collect()
    }
}
impl Workspace for TaskWorkspace<'_, '_> {
    fn motion_output_bounds(&self) -> Result<(u64, u64, usize)> {
        self.job.inventory_bounds(true)
    }
    fn verify_motion_copy_files(&self) -> Result<()> {
        self.motion_copy
            .as_ref()
            .context("motion_owned_copy_not_prepared")?
            .verify_copy_files()
    }
    fn motion_parameters_reference(&self) -> Result<oe_contracts::ArtifactReference> {
        Ok(oe_contracts::ArtifactReference {
            path: self.job.params.path.as_str().to_owned(),
            sha256: self.job.params.identity.sha256.as_str().to_owned(),
        })
    }
    fn motion_job_id(&self) -> Result<String> {
        Ok(self.job.job_id.as_str().to_owned())
    }
    fn create_motion_evidence(
        &mut self,
        budget: oe_contracts::NxMotionEvidenceBudget,
        binding: oe_contracts::NxMotionEvidenceBinding,
    ) -> Result<Box<dyn crate::motion_evidence::sink::EvidenceSink + '_>> {
        ensure!(
            self.motion_evidence_dir.is_none(),
            "motion_evidence_already_created"
        );
        ensure!(
            binding.job_id == self.job.job_id.as_str(),
            "motion_evidence_wrong_job"
        );
        crate::motion_evidence::validate_stream_inputs(&budget, &binding)?;
        self.verify_motion_source()?;
        self.motion_evidence_dir = Some(
            self.out
                .create_directory_new(OsStr::new("motion-evidence"))?,
        );
        crate::motion_evidence::windows::create_owned_sink(
            self.motion_evidence_dir.as_ref().unwrap(),
            budget,
            binding,
        )
    }

    fn prepare_assembly_build(
        &mut self,
        intent: &oe_contracts::NxAssemblyBuildIntent,
    ) -> Result<(PathBuf, PathBuf)> {
        ensure!(self.motion_copy.is_none(), "motion_copy_already_prepared");
        crate::geometry::source_assembly::validate_source_set(intent)?;
        self.record("assembly_build_copy_pending", serde_json::to_value(intent)?)?;
        let allowed = &self.input_paths;
        let job_dir = self.job_dir;
        let owned = crate::motion_source::windows::copy_owned_build_source(
            self.out,
            intent,
            |name, bound| {
                let path = geometry::input_relative(name)?;
                ensure!(
                    allowed.contains_key(path.as_str()),
                    "motion_source_not_in_job_inputs"
                );
                adapter::pin_relative_file(job_dir, &path, bound)
            },
        )?;
        let root = vendor_path(&owned.root_part())?;
        let namespace = vendor_path(owned.namespace())?;
        self.motion_copy = Some(owned);
        self.record("assembly_build_copy_verified",json!({"root_part":root,"namespace":namespace,"files":intent.files.len(),
            "source_read_pins_retained":true,"output_directory_pins_retained":true,"native_reference_closure":"not_yet_checked"}))?;
        Ok((root, namespace))
    }
    fn prepare_motion_source(
        &mut self,
        closure: &oe_contracts::NxAssemblySourceClosure,
    ) -> Result<(PathBuf, PathBuf)> {
        ensure!(self.motion_copy.is_none(), "motion_copy_already_prepared");
        crate::motion_source::validate_source_closure(closure)?;
        self.record("motion_copy_pending", serde_json::to_value(closure)?)?;
        let allowed = &self.input_paths;
        let job_dir = self.job_dir;
        let owned =
            crate::motion_source::windows::copy_owned_source(self.out, closure, |name, bound| {
                let path = geometry::input_relative(name)?;
                ensure!(
                    allowed.contains_key(path.as_str()),
                    "motion_source_not_in_job_inputs"
                );
                adapter::pin_relative_file(job_dir, &path, bound)
            })?;
        let root = vendor_path(&owned.root_part())?;
        let namespace = vendor_path(owned.namespace())?;
        self.motion_copy = Some(owned);
        self.record("motion_copy_verified",json!({"root_part":root,"namespace":namespace,"files":closure.files.len(),
            "source_read_pins_retained":true,"output_directory_pins_retained":true,"native_reference_closure":"not_yet_checked"}))?;
        Ok((root, namespace))
    }
    fn verify_motion_source(&self) -> Result<()> {
        self.motion_copy
            .as_ref()
            .context("motion_owned_copy_not_prepared")?
            .verify()
    }
    fn input(&self, relative: &p::RelativePath) -> Result<PathBuf> {
        self.input_paths
            .get(relative.as_str())
            .cloned()
            .context("requested_input_not_in_manifest")
    }
    fn output(&self, name: &str) -> Result<PathBuf> {
        let relative = p::RelativePath::new(format!("out/{name}"))?;
        ensure!(!name.contains('/'), "native_output_must_be_leaf");
        self.out.verify()?;
        let path = self.out.path().join(name);
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => anyhow::bail!("native_output_already_exists"),
        }
        ensure!(relative.as_str().starts_with("out/"), "native_output_scope");
        vendor_path(&path)
    }
    fn require_regular_output(&self, name: &str) -> Result<Vec<u8>> {
        let relative = p::RelativePath::new(format!("out/{name}"))?;
        let mut pin = adapter::pin_relative_file(self.job_dir, &relative, p::MAX_PAYLOAD_BYTES)?;
        pin.read_all(p::MAX_PAYLOAD_BYTES).map_err(Into::into)
    }
    fn optional_regular_output(&self, name: &str) -> Result<Option<Vec<u8>>> {
        p::RelativePath::new(format!("out/{name}"))?;
        ensure!(!name.contains('/'), "native_output_must_be_leaf");
        let mut pin = match self.out.pin_file(OsStr::new(name), p::MAX_PAYLOAD_BYTES) {
            Ok(Some(pin)) => pin,
            Ok(None) => return Ok(None),
            // The translator may still have the output open for writing.
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        Ok(Some(pin.read_all(p::MAX_PAYLOAD_BYTES)?))
    }
    fn record(&mut self, phase: &str, observation: Value) -> Result<()> {
        self.write_record(phase, observation).map(|_| ())
    }
    fn record_bound(
        &mut self,
        phase: &str,
        observation: Value,
    ) -> Result<oe_contracts::ArtifactReference> {
        self.write_record(phase, observation)
    }
}

pub fn main_entry() -> Result<()> {
    let argv: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        argv.len() == 4 && argv[0] == "--job-dir" && argv[2] == "--nx-root",
        "usage: oe-nx-worker --job-dir ABS --nx-root ABS"
    );
    let job_path = PathBuf::from(&argv[1]);
    let nx_path = PathBuf::from(&argv[3]);
    ensure!(
        job_path.is_absolute() && nx_path.is_absolute(),
        "native_paths_must_be_absolute"
    );
    let root = StableRoot::open(job_path.parent().context("job_root_missing")?, None)?;
    let job_dir = root.directory(job_path.file_name().context("job_name_missing")?)?;
    let mut control_pins = Vec::new();
    let job: p::JobManifest = read_pinned(&job_dir, "job.json", &mut control_pins)?;
    job.validate()?;
    ensure!(
        job.job_id.as_str()
            == job_path
                .file_name()
                .and_then(OsStr::to_str)
                .context("job_id_not_unicode")?
            && job.root == adapter::root_identity(&root),
        "actual_native_root_or_job_differs"
    );
    crate::identity::validate_worker_identity(&job.worker)?;
    let intent: p::DispatchIntent = read_pinned(&job_dir, "dispatch.json", &mut control_pins)?;
    intent.validate(&job)?;
    // Do not retain a read-only sharing lock on state.json: controller must
    // publish RUNNING. The sole execution gate below requires our exact PID.
    // intent.controller identifies the request process; that process may have
    // returned while the separately launched supervisor owns the writer lock.
    let mut payload = Vec::new();
    let mut payload_pins = Vec::new();
    for directory in ["worker", "sources", "build", "inputs"] {
        let path = job_dir.path().join(directory);
        match std::fs::symlink_metadata(&path) {
            Ok(_) => gather(
                &job_dir.directory(OsStr::new(directory))?,
                directory,
                &job,
                false,
                &mut payload,
                &mut payload_pins,
            )?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && directory == "inputs" => {}
            Err(e) => return Err(e.into()),
        }
    }
    let mut params_pin = job_dir
        .pin_file(OsStr::new("params.json"), p::MAX_RECORD_BYTES as u64)?
        .context("params_missing")?;
    let params_bytes = params_pin.read_all(p::MAX_RECORD_BYTES as u64)?;
    payload.push(p::FileEntry {
        path: p::RelativePath::new("params.json")?,
        identity: adapter::file_identity(&mut params_pin)?,
    });
    payload_pins.push(params_pin);
    job.verify_payload(&payload)?;
    let value = job.validate_parameters(&params_bytes)?;
    let params: Parameters = serde_json::from_value(value)?;
    params.validate()?;
    ensure!(
        params.action() == job.action && job.params_schema == params.parameter_schema(),
        "native_action_parameters_differ"
    );
    let expected_exe = job_dir.path().join(job.worker.executable.path.as_str());
    let actual_exe = std::fs::canonicalize(std::env::current_exe()?)?;
    ensure!(
        actual_exe
            .to_string_lossy()
            .eq_ignore_ascii_case(&std::fs::canonicalize(&expected_exe)?.to_string_lossy()),
        "current_executable_is_not_manifest_worker"
    );
    let process = adapter::current_process_identity(job.worker.executable.identity.clone())?;
    let origin = if job.schema == p::LARGE_JOB_SCHEMA {
        let state: p::StateRecord = p::decode(
            &job_dir
                .read_record(OsStr::new("state.json"))?
                .context("published_worker_state_missing")?,
        )?;
        Some(
            oe_windows_controller::published_origin::PublishedOrigin::observe(
                &root, &job_dir, &job, &state,
            )?,
        )
    } else {
        None
    };
    if let Err(error) = await_running(&job_dir, &job, &intent, &process, origin.as_ref()) {
        job_dir.create_file_new(
            OsStr::new("worker-pre-native-unknown.json"),
            &bytes(&json!({"schema":"cad-agent.nx-pre-native-unknown/v2","job_id":job.job_id,"dispatch_id":intent.dispatch_id,"process":process,"reason":safe_error(&error),"nx_library_loaded":false,"initialize_calls":0,"part_api_calls":0}))?,
            p::MAX_RECORD_BYTES as u64,
        )?;
        return Err(error);
    }
    if let Some(origin) = origin {
        origin.verify_directory(&job_dir)?;
        control_pins.extend(origin.into_pins());
    }
    let sdk = StableRoot::open(&nx_path, None)?;
    let sdk_bin = sdk.directory(OsStr::new("NXBIN"))?;
    let mut runtime_pins = Vec::new();
    for file in &job.worker.required_runtime.libraries {
        let leaf = file
            .path
            .as_str()
            .strip_prefix("NXBIN/")
            .context("runtime_library_outside_NXBIN")?;
        ensure!(!leaf.contains('/'), "nested_runtime_library_unsupported");
        let mut pin = sdk_bin
            .pin_file(OsStr::new(leaf), 64 * 1024 * 1024)?
            .context("required_runtime_missing")?;
        ensure!(
            adapter::file_identity(&mut pin)? == file.identity,
            "required_runtime_bytes_differ"
        );
        runtime_pins.push(pin);
    }
    let shim = job
        .worker
        .runtime_files
        .iter()
        .find(|f| f.path.as_str() == "worker/nx_shim.dll")
        .context("required_native_shim_missing")?;
    let out = job_dir.directory(OsStr::new("out"))?;
    let receipts = job_dir.directory(OsStr::new("receipts"))?;
    initial_output_directories(&out, &receipts)?;
    let mut inputs = BTreeMap::new();
    for entry in &job.inputs {
        inputs.insert(
            entry.path.as_str().to_owned(),
            vendor_path(&job_dir.path().join(entry.path.as_str()))?,
        );
    }
    let mut workspace = TaskWorkspace {
        job: &job,
        job_dir: &job_dir,
        out: &out,
        receipts: &receipts,
        input_paths: inputs,
        motion_copy: None,
        motion_evidence_dir: None,
        event_count: 0,
        events: Vec::new(),
    };
    workspace.record("source_and_dispatch_verified",json!({"manifest_digest":job.digest()?,"worker_manifest_digest":job.worker.digest()?,"input_digest":job.input_digest()?,"dispatch_id":intent.dispatch_id,"process":process,"runtime_files_verified":job.worker.required_runtime,"all_dependency_identities_verified":false}))?;
    let loaded = Native::load(
        &vendor_path(sdk.path())?,
        &vendor_path(&job_dir.path().join(shim.path.as_str()))?,
        &shim.identity,
        &job.worker.required_runtime,
        &mut |phase, observation| workspace.record(phase, observation),
    );
    let mut api = match loaded {
        Ok(api) => api,
        Err(error) => {
            // This records the observed failure at its real execution boundary.
            // A partially verified loader is not a verified native outcome;
            // leave reconciliation UNKNOWN rather than invent a NativeResult.
            let recorded = workspace.record(
                "native_library_setup_failed",
                json!({
                    "dispatch_id":intent.dispatch_id,
                    "process":process,
                    "error":safe_error(&error),
                    "explicit_uf_initialize_calls":0,
                    "part_api_calls":0,
                    "vendor_loader_code_may_have_executed":true,
                    "native_setup_completed":false,
                    "loaded_module_identity_scope":"per_library_events",
                    "transitive_dll_identity_scope":"not_verified",
                    "native_result":"not_established",
                    "automatic_retry":false
                }),
            );
            if let Err(record_error) = recorded {
                return Err(error.context(format!(
                    "setup_failure_record_not_persisted: {record_error:#}"
                )));
            }
            return Err(error);
        }
    };
    workspace.record("compiled_abi_verified", api.abi_observation())?;
    if matches!(&params, Parameters::RunAssemblyMotion { schema, .. } if schema == geometry::motion_task::capture2::PARAMS_SCHEMA)
    {
        workspace.record(
            "motion_compiled_abi2",
            serde_json::to_value(api.motion_abi_observation2())?,
        )?;
    }
    workspace.record(
        "initialize_intent",
        json!({"call_count":1,"part_write_authority":"manifest_capability_and_fresh_out_only"}),
    )?;
    let initialization = api.initialize();
    let operation = match initialization {
        Ok(code) => workspace
            .record(
                "initialize_returned",
                json!({"return_code":code,"scope":"one_observed_execution"}),
            )
            .and_then(|()| geometry::execute(&mut api, &mut workspace, &params)),
        Err(e) => Err(e),
    };
    // A successful operation cannot bypass the held source/directory checks.
    let operation = operation.and_then(|value| {
        if let Some(copy) = workspace.motion_copy.as_ref() {
            copy.verify()?;
        }
        Ok(value)
    });
    let observations = api.take_load_observations();
    let recorded = (|| -> Result<()> {
        for observation in observations {
            workspace.record("native_load_status", observation)?;
        }
        Ok(())
    })();
    let operation_error = operation.as_ref().err().map(safe_error);
    let diagnostic_persistence_error = recorded.as_ref().err().map(safe_error);
    let mut readback = match operation {
        Ok(value) => value,
        Err(_) => {
            json!({"action":job.action,"partial_observations":workspace.event_references()?,"partial_observations_form":"source_bound_event_references","engineering_verdict":"not_assessed"})
        }
    };
    readback["load_status_records"] = json!(
        workspace
            .event_references()?
            .into_iter()
            .filter(|record| record["phase"] == "native_load_status")
            .collect::<Vec<_>>()
    );
    let indexed_readback =
        job.schema == "oev2.nx-native-job/3" && crate::event_index::requires_indexing(&readback)?;
    if indexed_readback {
        let references = workspace.event_references()?;
        let index = crate::event_index::persist(job.job_id.as_str(), &references, &mut |page| {
            workspace.record_bound("event_reference_index", page)
        })?;
        let object = readback
            .as_object_mut()
            .context("native_readback_not_object")?;
        object.remove("load_status_records");
        if object.remove("partial_observations").is_some() {
            object.insert(
                "partial_observations_form".into(),
                json!("complete_event_reference_index"),
            );
        }
        object.insert("event_reference_index".into(), index);
        crate::event_index::persist_fields(job.job_id.as_str(), &mut readback, &mut |chunk| {
            workspace.record_bound("readback_field_chunk", chunk)
        })?;
    }
    workspace.record(
        "operation_finished",
        json!({"error":operation_error,"diagnostic_persistence_error":diagnostic_persistence_error,"has_native_observations":true}),
    )?;
    job_dir.create_file_new(OsStr::new("native-result.pending.json"),&bytes(&json!({"schema":"cad-agent.nx-native-pending/v2","job_id":job.job_id,"dispatch_id":intent.dispatch_id,"process":process,"operation_error":operation_error,"diagnostic_persistence_error":diagnostic_persistence_error,"readback":readback,"termination":"not_yet_observed"}))?,p::MAX_RECORD_BYTES as u64)?;
    let terminate_attempted = api.was_initialized();
    let mut terminate_return = None;
    let termination = if terminate_attempted {
        // Even a disk-full event cannot leave an initialized session intentionally unreleased.
        let durable = workspace.record("terminate_intent", json!({"call_count":1}));
        let result = api.terminate();
        durable?;
        match result {
            Ok(code) => {
                terminate_return = Some(code);
                workspace.record("terminate_returned", json!({"return_code":code}))?;
                if code == 0 {
                    Ok(code)
                } else {
                    Err(anyhow::anyhow!("UF_terminate_code_{code}"))
                }
            }
            Err(e) => Err(e),
        }
    } else {
        Ok(0)
    };
    let mut errors = Vec::new();
    if let Some(error) = operation_error {
        errors.push(error)
    }
    if let Some(error) = diagnostic_persistence_error {
        errors.push(format!("native_load_status_not_persisted: {error}"))
    }
    if let Err(error) = termination {
        errors.push(safe_error(&error))
    }
    for pin in control_pins
        .iter()
        .chain(payload_pins.iter())
        .chain(runtime_pins.iter())
    {
        pin.verify()?;
    }
    root.verify()?;
    job_dir.verify()?;
    sdk.verify()?;
    let mut artifact_pins = Vec::new();
    let mut artifacts = Vec::new();
    gather(&out, "out", &job, true, &mut artifacts, &mut artifact_pins)?;
    job.validate_inventory_capacity(&artifacts, true)?;
    crate::event_index::finish_lifecycle(
        &mut readback,
        json!({"terminate_attempted":terminate_attempted,"terminate_return_observed":terminate_return.is_some(),"terminate_return":terminate_return,"pending_record_is_not_final_result":true,"runtime_scope":"explicitly_pinned_libraries_only"}),
    )?;
    let result = p::NativeResult {
        schema: p::RESULT_SCHEMA.into(),
        job_id: job.job_id.clone(),
        root: job.root.clone(),
        manifest_digest: job.digest()?,
        dispatch_id: intent.dispatch_id.clone(),
        worker_manifest_digest: job.worker.digest()?,
        worker: job.worker.executable.identity.clone(),
        input_digest: job.input_digest()?,
        process: process.clone(),
        runtime: job.worker.required_runtime.clone(),
        action: job.action.clone(),
        outcome: if errors.is_empty() {
            p::ResultOutcome::Succeeded
        } else {
            p::ResultOutcome::Failed
        },
        readback_schema: if indexed_readback {
            crate::event_index::READBACK_SCHEMA
        } else {
            geometry::READBACK_SCHEMA
        }
        .into(),
        readback,
        artifacts: artifacts.clone(),
        errors,
        unknowns: vec![],
    };
    let evidence = p::CompletionEvidence {
        process: process.clone(),
        exit_code: None,
        runtime: job.worker.required_runtime.clone(),
        staged_payload: payload,
        artifacts,
    };
    result.validate_binding(&job, &intent, &process, &evidence)?;
    job_dir.create_file_new(
        OsStr::new("native-result.json"),
        &bytes(&result)?,
        p::MAX_RECORD_BYTES as u64,
    )?;
    ensure!(
        result.outcome == p::ResultOutcome::Succeeded,
        "native_operation_failed_with_durable_result"
    );
    Ok(())
}
