//! Optional native v2 orchestration. Memory owns original provenance; v2 owns
//! current Project semantics. Only the receiving agent edits the Project.
use crate::contracts::{SessionProject, V2PrepareRequest, V2Receiver, V2VerifyRequest};
use crate::exchange::{encoded, publish};
use crate::store::{MAX_BYTES, Store, id, private_dir, read_bytes, read_json, sha, shape};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn absolute(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "receiver_absolute_path_required");
    ensure!(
        path.components().all(|c| matches!(
            c,
            Component::RootDir | Component::Prefix(_) | Component::Normal(_)
        )),
        "receiver_path_traversal"
    );
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "receiver_symlink"
        );
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> Result<String> {
    let value = path
        .strip_prefix(root)
        .context("receiver_project_outside_root")?;
    ensure!(
        !value.as_os_str().is_empty()
            && value
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "receiver_relative_path_invalid"
    );
    Ok(value.to_str().context("receiver_path_encoding")?.into())
}

fn input_file(root: &Path, raw: &str) -> Result<PathBuf> {
    let path = root.join(raw);
    ensure!(
        relative(root, &path)? == raw,
        "receiver_artifact_path_invalid"
    );
    absolute(&path)?;
    ensure!(path.is_file(), "receiver_artifact_not_regular");
    Ok(path)
}

fn binary_digest(path: &Path) -> Result<String> {
    absolute(path)?;
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= 1024 * 1024 * 1024,
        "receiver_binary_budget"
    );
    let mut digest = Sha256::new();
    let count = std::io::copy(
        &mut Read::by_ref(&mut file).take(before.len() + 1),
        &mut digest,
    )?;
    let after = fs::metadata(path)?;
    ensure!(
        count == before.len()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "receiver_binary_changed"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            before.dev() == after.dev()
                && before.ino() == after.ino()
                && before.ctime() == after.ctime()
                && before.ctime_nsec() == after.ctime_nsec(),
            "receiver_binary_replaced"
        );
    }
    Ok(hex::encode(digest.finalize()))
}

struct Receiver {
    root: PathBuf,
    program: PathBuf,
    expected: String,
}

impl Receiver {
    fn open(spec: &V2Receiver) -> Result<Self> {
        shape(spec)?;
        let root = PathBuf::from(&spec.root);
        absolute(&root)?;
        ensure!(root.is_dir(), "receiver_root_not_directory");
        let installation = root.join("var/install/INSTALLATION.json");
        let selected = if installation.exists() {
            let path = input_file(&root, "var/install/INSTALLATION.json")?;
            let manifest: Value = read_json(&path)?;
            ensure!(
                manifest["schema"] == "oev2.installation/1"
                    && manifest["root"].as_str() == root.to_str(),
                "receiver_installation_identity_mismatch"
            );
            let programs = manifest["programs"]
                .as_array()
                .context("receiver_installation_programs_missing")?;
            let selected: Vec<_> = programs.iter().filter(|p| p["name"] == "oev2").collect();
            ensure!(
                selected.len() == 1,
                "receiver_installation_program_not_unique"
            );
            selected[0].clone()
        } else {
            // Explicit old installations remain readable. An invalid new manifest
            // is always rejected and never falls back to historical selection.
            let status_path = input_file(&root, "var/maintenance/status.json")?;
            let status: Value = read_json(&status_path)?;
            status["current_cli"]["default"].clone()
        };
        ensure!(
            selected["sha256"] == spec.cli_sha256,
            "receiver_selected_identity_mismatch"
        );
        let program = PathBuf::from(
            selected["path"]
                .as_str()
                .context("receiver_program_missing")?,
        );
        let program = if program.is_absolute() {
            program
        } else {
            root.join(program)
        };
        ensure!(
            binary_digest(&program)? == spec.cli_sha256,
            "receiver_binary_digest_mismatch"
        );
        Ok(Self {
            root,
            program,
            expected: spec.cli_sha256.clone(),
        })
    }

    fn verify(&self) -> Result<()> {
        let current = Self::open(&V2Receiver {
            root: self.root.to_str().context("receiver_root_encoding")?.into(),
            cli_sha256: self.expected.clone(),
        })?;
        ensure!(
            current.program == self.program,
            "receiver_program_selection_changed"
        );
        Ok(())
    }
}

struct Run {
    path: PathBuf,
    started: Instant,
    seconds: u64,
}

impl Run {
    fn create(
        store: &Store,
        receiver: &Receiver,
        name: &str,
        started: Instant,
        seconds: u64,
    ) -> Result<Self> {
        let namespace = sha(store.directory.canonicalize()?.to_string_lossy().as_bytes());
        let parent = receiver
            .root
            .join("var/engineering-memory")
            .join(&namespace[..16])
            .join(id(name)?);
        // Check the deepest existing parent before creating private directories.
        let existing = parent
            .ancestors()
            .find(|p| p.exists())
            .context("receiver_parent_missing")?;
        absolute(existing)?;
        private_dir(&parent)?;
        absolute(&parent)?;
        let attempt = format!(
            "attempt-{}-{}",
            std::process::id(),
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .context("receiver_clock_range")?
        );
        let path = parent.join(attempt);
        fs::create_dir(&path)?;
        private_dir(&path)?;
        Ok(Self {
            path,
            started,
            seconds,
        })
    }

    fn budget(&self) -> Result<()> {
        ensure!(
            self.started.elapsed() < Duration::from_secs(self.seconds),
            "receiver_time_budget"
        );
        Ok(())
    }

    fn call(&self, receiver: &Receiver, args: &[String], label: &str) -> Result<Value> {
        self.budget()?;
        let stdout = self.path.join(format!("{label}.stdout.json"));
        let stderr = self.path.join(format!("{label}.stderr.log"));
        let create = |p: &Path| -> Result<File> {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            Ok(options.open(p)?)
        };
        let mut child = Command::new(&receiver.program)
            .arg("--root")
            .arg(&receiver.root)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("NO_COLOR", "1")
            .current_dir(&receiver.root)
            .stdin(Stdio::null())
            .stdout(create(&stdout)?)
            .stderr(create(&stderr)?)
            .spawn()
            .context("receiver_spawn_failed")?;
        let outcome = (|| -> Result<Value> {
            loop {
                self.budget()?;
                ensure!(
                    stdout.metadata()?.len() <= MAX_BYTES && stderr.metadata()?.len() <= MAX_BYTES,
                    "receiver_output_budget"
                );
                if let Some(status) = child.try_wait()? {
                    ensure!(
                        stdout.metadata()?.len() <= MAX_BYTES
                            && stderr.metadata()?.len() <= MAX_BYTES,
                        "receiver_output_budget"
                    );
                    ensure!(
                        status.success(),
                        "receiver_command_failed:{label}:{status}; see {}",
                        stderr.display()
                    );
                    return read_json(&stdout).context("receiver_output_json_invalid");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        if outcome.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        outcome
    }

    fn finish(&self, result: &Result<Value>) -> Result<()> {
        let value = match result {
            Ok(value) => json!({"status":"completed","result":value}),
            Err(error) => {
                json!({"status":"failed","error":format!("{error:#}"),"old_success_reused":false})
            }
        };
        publish(
            &self.path.join("result.json"),
            &encoded(&json!({"schema":"engineering-memory.v2-run/1",
            "finished_at":chrono::Utc::now().to_rfc3339(),"elapsed_ms":self.started.elapsed().as_millis(),
            "execution":value,"project_write":false,"engineering_adoption":"not_granted"}))?,
        )
    }
}

fn project(handoff: &Value) -> Result<SessionProject> {
    let binding: SessionProject = serde_json::from_value(handoff["project"].clone())?;
    ensure!(binding.object.is_some(), "receiver_object_binding_required");
    Ok(binding)
}

fn handoff(export: &Value) -> Result<Value> {
    let manifest = &export["manifest"];
    let path = Path::new(
        manifest["handoff_path"]
            .as_str()
            .context("receiver_handoff_path")?,
    );
    absolute(path)?;
    let bytes = read_bytes(path)?;
    ensure!(
        json!(sha(&bytes)) == manifest["handoff_sha256"],
        "receiver_handoff_changed"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

fn selected_entries(intake: &Value, archive: &Value) -> Result<Vec<String>> {
    ensure!(
        intake["schema"] == "ontology-engineering.conversation-source/v1"
            && intake["status"] == "complete"
            && intake["issue_count"] == 0
            && intake["source_path"] == archive["archive_path"]
            && intake["scanned_prefix_sha256"] == archive["archive_sha256"]
            && intake["original_size"] == archive["archive_bytes"]
            && intake["metadata"]["session_id"] == archive["original"]["session_id"],
        "receiver_intake_scope_mismatch"
    );
    let original = &archive["original"];
    let derived = &archive["derived"];
    let rows = intake["entries"]
        .as_array()
        .context("receiver_intake_entries_missing")?;
    ensure!(
        intake["entry_count"] == rows.len(),
        "receiver_intake_entry_count"
    );
    let mut ids = Vec::new();
    let mut texts = Vec::new();
    for row in rows.iter().filter(|v| v["kind"] == "message") {
        ensure!(
            row["role"] == original["role"]
                && row["raw_sha256"] == original["sha256"]
                && row["byte_start"] == derived["byte_start"]
                && row["byte_end"] == derived["byte_end"]
                && row["timestamp"] == original["event_at"],
            "receiver_selected_message_mismatch"
        );
        ids.push(row["id"].as_str().context("receiver_entry_id")?.to_string());
        texts.push(row["text"].as_str().context("receiver_entry_text")?);
    }
    ensure!(
        !ids.is_empty()
            && ids.iter().collect::<BTreeSet<_>>().len() == ids.len()
            && json!(texts.join("\n")) == original["text"],
        "receiver_selected_text_mismatch"
    );
    Ok(ids)
}

fn compatible_projects(before: &SessionProject, current: &SessionProject) -> Result<()> {
    let a = before
        .object
        .as_ref()
        .context("receiver_object_binding_required")?;
    let b = current
        .object
        .as_ref()
        .context("receiver_object_binding_required")?;
    ensure!(
        before.path == current.path
            && before.project_id == current.project_id
            && a.object_id == b.object_id
            && a.object_revision == b.object_revision
            && a.condition_scope == b.condition_scope,
        "receiver_target_project_changed"
    );
    Ok(())
}

impl Store {
    fn receiver_intent(
        &self,
        folder: &str,
        name: &str,
        request: &impl serde::Serialize,
    ) -> Result<()> {
        let _lock = self.lock()?;
        self.immutable(folder, name, request)?;
        Ok(())
    }

    pub fn v2_prepare(&self, request: &V2PrepareRequest) -> Result<Value> {
        let started = Instant::now();
        shape(request)?;
        id(&request.id)?;
        ensure!(
            (1..=120).contains(&request.max_seconds)
                && !request.fact_property.trim().is_empty()
                && request.fact_property.len() <= 256,
            "receiver_request_budget"
        );
        let lease = Store::new(self.directory.join("v2-receiver-lease"));
        let _lease = lease.lock()?;
        let receiver = Receiver::open(&request.receiver)?;
        let exported = self.session_export(&request.export)?;
        let handoff = handoff(&exported)?;
        let binding = project(&handoff)?;
        relative(&receiver.root, Path::new(&binding.path))?;
        let current: Value = read_json(Path::new(&binding.path))?;
        ensure!(
            current["evidence"]
                .as_object()
                .context("receiver_evidence_missing")?
                .values()
                .any(|v| v == &request.object_source_sha256),
            "receiver_object_source_missing"
        );
        let archives = exported["manifest"]["archives"]
            .as_array()
            .context("receiver_archives_missing")?;
        ensure!(
            !archives.is_empty() && archives.len() <= 32,
            "receiver_binding_budget"
        );
        self.receiver_intent("v2-prepare-intents", &request.id, request)?;
        let saved_path = self
            .directory
            .join("v2-preparations")
            .join(format!("{}.json", request.id));
        let previous: Option<Value> = if saved_path.exists() {
            Some(read_json(&saved_path)?)
        } else {
            None
        };
        let run = Run::create(self, &receiver, &request.id, started, request.max_seconds)?;
        let result = (|| -> Result<Value> {
            publish(
                &run.path.join("request.json"),
                &encoded(&serde_json::to_value(request)?)?,
            )?;
            let mut intakes = Vec::new();
            let mut entry_count = 0;
            for (index, archive) in archives.iter().enumerate() {
                let output = run.path.join(format!("intake-{index:03}.json"));
                run.call(
                    &receiver,
                    &[
                        "knowledge".into(),
                        "intake".into(),
                        "--archive".into(),
                        archive["archive_path"]
                            .as_str()
                            .context("receiver_archive_path")?
                            .into(),
                        "--output".into(),
                        relative(&receiver.root, &output)?,
                    ],
                    &format!("intake-{index:03}"),
                )?;
                let raw = read_bytes(&output)?;
                let intake: Value = serde_json::from_slice(&raw)?;
                let entries = selected_entries(&intake, archive)?;
                entry_count += entries.len();
                ensure!(entry_count <= 64, "receiver_entry_budget");
                let artifact = if let Some(old) = &previous {
                    let artifact = &old["intakes"][index]["intake"];
                    let old_path = input_file(
                        &receiver.root,
                        artifact["path"]
                            .as_str()
                            .context("receiver_saved_intake_path")?,
                    )?;
                    ensure!(
                        read_bytes(&old_path)? == raw && artifact["sha256"] == sha(&raw),
                        "receiver_saved_intake_changed"
                    );
                    artifact.clone()
                } else {
                    json!({"path":relative(&receiver.root,&output)?,"sha256":sha(&raw)})
                };
                let source_id = format!("memory-{}-{index:03}", &sha(request.id.as_bytes())[..16]);
                intakes.push(json!({"source_id":source_id,"intake":artifact,
                    "archive":{"path":archive["archive_path"],"sha256":archive["archive_sha256"]},"entry_ids":entries}));
            }
            let source_refs: Vec<_> = intakes.iter().map(|v| v["source_id"].clone()).collect();
            let evidence: serde_json::Map<String, Value> = intakes
                .iter()
                .map(|v| {
                    (
                        v["source_id"].as_str().unwrap().to_string(),
                        v["intake"]["sha256"].clone(),
                    )
                })
                .collect();
            let proposal = json!({"fact":{"property":request.fact_property,"status":"declared",
                "value":{"role":"operation_method_instance","experience":handoff["candidate"]["record"],
                    "experience_sha256":handoff["candidate"]["record_sha256"],"source_captures":handoff["candidate"]["source_captures"],
                    "use_state":"candidate; provenance only; engineering applicability and benefit require separate evidence"},"source_refs":source_refs},
                "evidence":evidence,"project_write":false,"engineering_adoption":"not_granted"});
            let prepared = json!({"schema":"engineering-memory.v2-preparation/1","request":request,
                "project":binding,"archives":archives,"record_sha256":exported["manifest"]["record_sha256"],
                "intakes":intakes,"proposal":proposal,"project_write":false,"rrsi_validation":"not_run",
                "engineering_adoption":"not_granted"});
            self.session_export_verify(&request.export.id)?;
            receiver.verify()?;
            run.budget()?;
            if let Some(old) = &previous {
                ensure!(*old == prepared, "receiver_preparation_changed");
            }
            self.receiver_intent("v2-preparations", &request.id, &prepared)?;
            publish(&run.path.join("proposal.json"), &encoded(&proposal)?)?;
            Ok(
                json!({"status":"prepared","preparation":prepared,"run":run.path,"project_write":false,
                "required_next":"Receiving agent maintains current Project using the candidate; prepare a fresh memory context, then call session v2-verify. No automatic adoption."}),
            )
        })();
        run.finish(&result)?;
        result.with_context(|| format!("v2_preparation_failed; see {}", run.path.display()))
    }

    pub fn v2_verify(&self, request: &V2VerifyRequest) -> Result<Value> {
        let started = Instant::now();
        shape(request)?;
        id(&request.id)?;
        id(&request.preparation_id)?;
        ensure!(
            (1..=120).contains(&request.max_seconds) && request.sources.len() <= 128,
            "receiver_request_budget"
        );
        let lease = Store::new(self.directory.join("v2-receiver-lease"));
        let _lease = lease.lock()?;
        let saved_path = self
            .directory
            .join("v2-preparations")
            .join(format!("{}.json", request.preparation_id));
        absolute(&saved_path)?;
        let saved_raw = read_bytes(&saved_path)?;
        let prepared: Value = serde_json::from_slice(&saved_raw)?;
        ensure!(
            prepared["schema"] == "engineering-memory.v2-preparation/1",
            "receiver_preparation_schema"
        );
        let setup: V2PrepareRequest = serde_json::from_value(prepared["request"].clone())?;
        shape(&setup)?;
        ensure!(
            setup.id == request.preparation_id,
            "receiver_preparation_identity"
        );
        let intent: V2PrepareRequest = read_json(
            &self
                .directory
                .join("v2-prepare-intents")
                .join(format!("{}.json", setup.id)),
        )?;
        ensure!(intent == setup, "receiver_preparation_intent_changed");
        let receiver = Receiver::open(&setup.receiver)?;
        let exported = self.session_export(&request.export)?;
        ensure!(
            exported["manifest"]["archives"] == prepared["archives"]
                && exported["manifest"]["record_sha256"] == prepared["record_sha256"],
            "receiver_experience_changed"
        );
        let handoff = handoff(&exported)?;
        let binding = project(&handoff)?;
        compatible_projects(
            &serde_json::from_value(prepared["project"].clone())?,
            &binding,
        )?;
        let native_path = relative(&receiver.root, Path::new(&binding.path))?;
        let current_raw = read_bytes(Path::new(&binding.path))?;
        let current: Value = serde_json::from_slice(&current_raw)?;
        let object = binding
            .object
            .as_ref()
            .context("receiver_object_binding_required")?;
        let basis: Value = serde_json::from_str(
            current["conditions"][&object.condition_scope]
                .as_str()
                .context("receiver_basis_missing")?,
        )?;
        let found: Vec<_> = basis["facts"]
            .as_array()
            .context("receiver_facts_missing")?
            .iter()
            .filter(|v| v["property"] == setup.fact_property)
            .collect();
        ensure!(
            found.len() == 1,
            "receiver_method_fact_missing_or_ambiguous"
        );
        let fact = found[0];
        ensure!(
            fact["value"]["role"] == "operation_method_instance"
                && fact["value"]["experience"] == handoff["candidate"]["record"]
                && fact["value"]["experience_sha256"] == handoff["candidate"]["record_sha256"]
                && fact["value"]["source_captures"] == handoff["candidate"]["source_captures"],
            "receiver_complete_experience_fact_mismatch"
        );
        let intakes = prepared["intakes"]
            .as_array()
            .context("receiver_prepared_intakes_missing")?;
        let archives = prepared["archives"]
            .as_array()
            .context("receiver_prepared_archives_missing")?;
        ensure!(
            intakes.len() == archives.len() && !intakes.is_empty() && intakes.len() <= 32,
            "receiver_complete_intake_coverage"
        );
        let mut sources: Vec<Value> = request
            .sources
            .iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<_, _>>()?;
        for artifact in request
            .previous
            .iter()
            .chain(request.work.iter())
            .chain(request.sources.iter())
        {
            let path = input_file(&receiver.root, &artifact.path)?;
            ensure!(
                sha(read_bytes(&path)?) == artifact.sha256,
                "receiver_extra_input_changed"
            );
        }
        let mut methods = Vec::new();
        let mut ids = BTreeSet::new();
        let mut entry_count = 0;
        for (intake, archive) in intakes.iter().zip(archives) {
            let source_id = intake["source_id"].as_str().context("receiver_source_id")?;
            ensure!(
                ids.insert(source_id)
                    && fact["source_refs"]
                        .as_array()
                        .context("receiver_fact_source_refs")?
                        .contains(&json!(source_id))
                    && current["evidence"][source_id] == intake["intake"]["sha256"],
                "receiver_project_intake_missing"
            );
            ensure!(
                intake["archive"]["path"] == archive["archive_path"]
                    && intake["archive"]["sha256"] == archive["archive_sha256"],
                "receiver_archive_binding_changed"
            );
            let path = input_file(
                &receiver.root,
                intake["intake"]["path"]
                    .as_str()
                    .context("receiver_intake_path")?,
            )?;
            let raw = read_bytes(&path)?;
            ensure!(
                json!(sha(&raw)) == intake["intake"]["sha256"],
                "receiver_intake_digest_changed"
            );
            let entries = selected_entries(&serde_json::from_slice(&raw)?, archive)?;
            ensure!(
                serde_json::to_value(&entries)? == intake["entry_ids"],
                "receiver_entry_selection_changed"
            );
            entry_count += entries.len();
            ensure!(entry_count <= 64, "receiver_entry_budget");
            if !sources.contains(&intake["intake"]) {
                sources.push(intake["intake"].clone());
            }
            methods.push(json!({"object":{"object_id":object.object_id,"revision":object.object_revision,
                "condition_scope":object.condition_scope,"source_sha256":setup.object_source_sha256},
                "basis_sha256":object.condition_sha256,"fact_property":setup.fact_property,"source_id":source_id,
                "intake":intake["intake"],"archive":intake["archive"],"entry_ids":entries}));
        }
        let input = json!({"schema":"oev2.current-task-view-input/2","task":{
            "schema":"oev2.current-task-view-input/1","current":{"path":native_path,"sha256":binding.sha256},
            "previous":request.previous,"work":request.work,"sources":sources},"methods":methods});
        self.receiver_intent("v2-verify-intents", &request.id, request)?;
        let run = Run::create(self, &receiver, &request.id, started, request.max_seconds)?;
        let result = (|| -> Result<Value> {
            let input_path = run.path.join("task-input.json");
            let output = run.path.join("task-view.json");
            publish(&input_path, &encoded(&input)?)?;
            publish(
                &run.path.join("request.json"),
                &encoded(&serde_json::to_value(request)?)?,
            )?;
            run.call(
                &receiver,
                &[
                    "project".into(),
                    "task-view".into(),
                    "--input".into(),
                    relative(&receiver.root, &input_path)?,
                    "--output".into(),
                    relative(&receiver.root, &output)?,
                ],
                "task-view",
            )?;
            let view_raw = read_bytes(&output)?;
            let view: Value = serde_json::from_slice(&view_raw)?;
            ensure!(
                view["schema"] == "oev2.current-task-view/2"
                    && view["input"] == input
                    && view["freshness"] == "matched"
                    && view["proof_scope"] == "selected_message_provenance_only",
                "receiver_native_view_not_matched"
            );
            let observations = view["methods"]
                .as_array()
                .context("receiver_native_methods_missing")?;
            ensure!(
                observations.len() == intakes.len()
                    && observations.iter().all(|v| v["status"] == "matched"),
                "receiver_native_method_not_matched"
            );
            let freshness = run.call(
                &receiver,
                &[
                    "project".into(),
                    "verify-task-view".into(),
                    "--input".into(),
                    relative(&receiver.root, &output)?,
                ],
                "verify-task-view",
            )?;
            ensure!(
                freshness["status"] == "matched"
                    && freshness["proof_scope"] == "selected_message_provenance_only",
                "receiver_native_freshness_not_matched"
            );
            let memory = self.session_export_verify(&request.export.id)?;
            ensure!(
                memory["manifest_sha256"] == exported["manifest_sha256"],
                "receiver_export_changed_during_use"
            );
            ensure!(
                read_bytes(&saved_path)? == saved_raw
                    && read_bytes(&output)? == view_raw
                    && read_bytes(Path::new(&binding.path))? == current_raw,
                "receiver_input_changed_during_use"
            );
            receiver.verify()?;
            run.budget()?;
            Ok(
                json!({"schema":"engineering-memory.v2-verification/1","status":"matched","request":request,
                "receiver":{"root":receiver.root,"program":receiver.program,"cli_sha256":receiver.expected},
                "project":binding,"memory_manifest_sha256":exported["manifest_sha256"],
                "preparation_sha256":sha(&saved_raw),"native_view":{"path":output,"sha256":sha(&view_raw)},
                "native_freshness":freshness,"selected_sources":intakes.len(),"selected_entries":entry_count,
                "run":run.path,"project_write":false,"engineering_adoption":"not_granted","rrsi_validation":"not_run",
                "proof_scope":"selected_original_messages_and_current_project_binding; not method truth or benefit"}),
            )
        })();
        run.finish(&result)?;
        result.with_context(|| format!("v2_verification_failed; see {}", run.path.display()))
    }
}

#[cfg(test)]
mod installation_tests {
    use super::*;

    #[test]
    fn new_installation_is_bound_and_invalid_manifest_never_uses_legacy_status() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let program = root.join("fixture-program-bytes");
        fs::write(&program, b"not executed; identity validation fixture").unwrap();
        let expected = binary_digest(&program).unwrap();
        let spec = V2Receiver {
            root: root.to_str().unwrap().into(),
            cli_sha256: expected.clone(),
        };
        fs::create_dir_all(root.join("var/install")).unwrap();
        fs::create_dir_all(root.join("var/maintenance")).unwrap();
        fs::write(
            root.join("var/maintenance/status.json"),
            serde_json::to_vec(&json!({
                "current_cli":{"default":{"path":program,"sha256":expected}}
            }))
            .unwrap(),
        )
        .unwrap();
        let manifest_path = root.join("var/install/INSTALLATION.json");
        let manifest = json!({"schema":"oev2.installation/1","root":root,
            "programs":[{"name":"oev2","path":program,"sha256":expected}]});
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let selected = Receiver::open(&spec).unwrap();
        assert_eq!(selected.program, program);
        for key in ["schema", "root"] {
            let mut bad = manifest.clone();
            bad[key] = json!("different");
            fs::write(&manifest_path, serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(Receiver::open(&spec).is_err(), "{key}");
        }
        let mut duplicate = manifest.clone();
        duplicate["programs"]
            .as_array_mut()
            .unwrap()
            .push(manifest["programs"][0].clone());
        fs::write(&manifest_path, serde_json::to_vec(&duplicate).unwrap()).unwrap();
        assert!(Receiver::open(&spec).is_err());
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        fs::write(&program, b"changed executable bytes").unwrap();
        assert!(selected.verify().is_err());
    }
}
