//! Exact selected source records for a bounded receiver. Derived positions never
//! replace original positions; use requires both this check and the receiver's.
use crate::contracts::{ExperienceRecord, SessionExportRequest, SourceAnchor};
use crate::evidence::{EvidenceArgs, read_window};
use crate::store::{MAX_BYTES, Store, id, private_dir, read_bytes, read_json, sha, shape};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const HEADER_BYTES: u64 = 64 * 1024;
const RECEIVER_RECORD_BYTES: u64 = 2 * 1024 * 1024;

pub(crate) fn encoded(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    ensure!(bytes.len() as u64 <= MAX_BYTES, "export_metadata_budget");
    Ok(bytes)
}

fn regular(path: &Path) -> Result<File> {
    ensure!(path.is_absolute(), "export_path_not_absolute");
    ensure!(path.canonicalize()? == path, "export_path_alias_or_symlink");
    ensure!(fs::symlink_metadata(path)?.is_file(), "export_not_regular");
    Ok(File::open(path)?)
}

fn exact_file(path: &Path, expected: &[u8]) -> Result<()> {
    let _file = regular(path)?;
    ensure!(
        read_bytes(path)? == expected,
        "export_file_changed:{}",
        path.display()
    );
    Ok(())
}

/// Publish only absent files. A durable intent is written first. Staging files
/// left by interruption are retained; a different existing artifact is refused.
pub(crate) fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.try_exists()? {
        return exact_file(path, bytes);
    }
    let parent = path.parent().context("export_parent_missing")?;
    private_dir(parent)?;
    ensure!(
        parent.canonicalize()? == parent,
        "export_parent_alias_or_symlink"
    );
    let mut stage = None;
    for number in 0..1000 {
        let candidate = parent.join(format!(".export-{}-{number}.pending", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(file) => {
                stage = Some((candidate, file));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    let (stage_path, mut file) = stage.context("export_staging_budget")?;
    file.write_all(bytes)?;
    file.sync_all()?;
    // Hard-link publication cannot replace an existing destination. Both paths
    // are local private files; retaining the stage also preserves failed work.
    match fs::hard_link(&stage_path, path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => exact_file(path, bytes)?,
        Err(e) => return Err(e.into()),
    }
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn header(file: &mut File, source: &SourceAnchor) -> Result<(Vec<u8>, Value)> {
    if source.provider != "codex" {
        return Ok((Vec::new(), Value::Null));
    }
    file.seek(SeekFrom::Start(0))?;
    let mut prefix = Vec::new();
    Read::by_ref(file)
        .take(HEADER_BYTES)
        .read_to_end(&mut prefix)?;
    let mut start = 0u64;
    for raw in prefix.split_inclusive(|b| *b == b'\n') {
        let end = start + raw.len() as u64;
        if let Ok(value) = serde_json::from_slice::<Value>(raw)
            && value["type"] == "session_meta"
        {
            ensure!(
                raw.last() == Some(&b'\n'),
                "export_unfinished_session_header"
            );
            ensure!(
                value["payload"]["id"] == source.session_id,
                "export_header_session_mismatch"
            );
            return Ok((
                raw.to_vec(),
                json!({"source_path":source.source_path,
                "byte_start":start,"byte_end":end,"sha256":sha(raw),
                "export_byte_start":0,"export_byte_end":raw.len()}),
            ));
        }
        start = end;
    }
    anyhow::bail!("export_original_session_header_missing")
}

struct Plan {
    files: Vec<(PathBuf, Vec<u8>)>,
    manifest: Value,
}

fn plan(folder: &Path, request: &SessionExportRequest, handoff: &Value) -> Result<Plan> {
    let record: ExperienceRecord = serde_json::from_value(handoff["candidate"]["record"].clone())?;
    let mut files = Vec::new();
    let mut archives = Vec::new();
    let mut total = 0u64;
    for (index, source) in record.sources.iter().enumerate() {
        let len = source
            .byte_end
            .checked_sub(source.byte_start)
            .context("export_source_range")?;
        ensure!(
            len > 0 && len <= RECEIVER_RECORD_BYTES,
            "export_receiver_record_budget"
        );
        let mut input = regular(Path::new(&source.source_path))?;
        let (prefix, identity) = header(&mut input, source)?;
        total += prefix.len() as u64 + len;
        ensure!(total <= request.max_bytes, "export_total_budget");
        let mut raw = vec![0; usize::try_from(len)?];
        input.seek(SeekFrom::Start(source.byte_start))?;
        input.read_exact(&mut raw)?;
        ensure!(sha(&raw) == source.sha256, "export_original_bytes_changed");
        ensure!(
            raw.last() == Some(&b'\n'),
            "export_unfinished_source_record"
        );
        let bytes = [prefix, raw].concat();
        // Native intake identifies the archive path. Keep this path stable when
        // the receiving Project gains evidence and a new context is prepared;
        // otherwise intake hashes and Project-bound handoffs form a cycle.
        let store_root = folder
            .parent()
            .and_then(Path::parent)
            .context("export_store_root")?;
        let archive = store_root
            .join("source-exports")
            .join(sha(&bytes))
            .join("source.jsonl");
        let mut derived = source.clone();
        derived.source_path = archive.to_str().context("export_path_encoding")?.into();
        derived.byte_start = bytes.len() as u64 - len;
        derived.byte_end = derived.byte_start + len;
        archives.push(
            json!({"source_index":index,"archive_path":archive,"archive_sha256":sha(&bytes),
            "archive_bytes":bytes.len(),"original":source,"derived":derived,
            "original_session_header":identity,
            "scope":"selected_record_and_identity_header; other history not exported"}),
        );
        files.push((archive, bytes));
    }
    let handoff_path = folder.join("handoff.json");
    let handoff_bytes = encoded(handoff)?;
    let manifest = json!({"schema":"engineering-memory.session-export/1","request":request,
        "handoff_path":handoff_path,"handoff_sha256":sha(&handoff_bytes),
        "archives":archives,"archive_bytes":total,"source_count":record.sources.len(),
        "record_sha256":handoff["candidate"]["record_sha256"],
        "project_write":false,"engineering_adoption":"not_granted","rrsi_validation":"not_run",
        "receiver_verification":"required_separately",
        "proof_scope":"selected_original_record_bytes_and_session_binding",
        "required_next":"Receiver must verify native intake and current Project; reverify this export before reuse. Derived archive provenance alone does not verify the original mapping."});
    files.push((handoff_path, handoff_bytes));
    // Fail before publishing even the intent if the complete mapping is too large.
    encoded(&manifest)?;
    Ok(Plan { files, manifest })
}

fn compare_handoff(saved: &Value, current: &Value) -> Result<()> {
    let mut saved = saved.clone();
    let mut current = current.clone();
    // Later feedback doesn't rewrite the historical exposure. Everything else,
    // including complete record, Project and captured-source identity, must match.
    saved["candidate"]["feedback"] = Value::Null;
    current["candidate"]["feedback"] = Value::Null;
    ensure!(saved == current, "export_handoff_changed");
    Ok(())
}

impl Store {
    pub fn session_export(&self, request: &SessionExportRequest) -> Result<Value> {
        shape(request)?;
        id(&request.id)?;
        ensure!(
            (1..=MAX_BYTES).contains(&request.max_bytes),
            "export_invalid_budget"
        );
        let _lock = self.lock()?;
        let current = self.session_handoff_locked(
            &request.context_id,
            &request.context_sha256,
            &request.record_id,
            request.revision,
        )?;
        let folder = self
            .directory
            .canonicalize()?
            .join("session-exports")
            .join(&request.id);
        let intent_path = folder.join("intent.json");
        let intent = if intent_path.try_exists()? {
            regular(&intent_path)?;
            let intent: Value = read_json(&intent_path)?;
            ensure!(
                intent["schema"] == "engineering-memory.session-export-intent/1",
                "export_intent_schema"
            );
            ensure!(
                intent["request"] == serde_json::to_value(request)?,
                "export_request_conflict"
            );
            compare_handoff(&intent["handoff"], &current)?;
            intent
        } else {
            json!({"schema":"engineering-memory.session-export-intent/1","request":request,"handoff":current})
        };
        let planned = plan(&folder, request, &intent["handoff"])?;
        let intent_bytes = encoded(&intent)?;
        // Recheck original bytes/capture and Project after preparing derived bytes.
        let current = self.session_handoff_locked(
            &request.context_id,
            &request.context_sha256,
            &request.record_id,
            request.revision,
        )?;
        compare_handoff(&intent["handoff"], &current)?;
        publish(&intent_path, &intent_bytes)?;
        for (path, bytes) in &planned.files {
            publish(path, bytes)?;
        }
        let manifest_path = folder.join("manifest.json");
        publish(&manifest_path, &encoded(&planned.manifest)?)?;
        self.verify_export_locked(request, &folder, &intent["handoff"])
    }

    fn verify_export_locked(
        &self,
        request: &SessionExportRequest,
        folder: &Path,
        handoff: &Value,
    ) -> Result<Value> {
        let planned = plan(folder, request, handoff)?;
        for (path, bytes) in &planned.files {
            exact_file(path, bytes)?;
        }
        for item in planned.manifest["archives"]
            .as_array()
            .context("export_archives_missing")?
        {
            let derived: SourceAnchor = serde_json::from_value(item["derived"].clone())?;
            let window = read_window(&EvidenceArgs {
                source: derived.source_path.clone().into(),
                provider: derived.provider.clone(),
                profile: derived.profile.clone(),
                start_byte: derived.byte_start,
                end_byte: Some(derived.byte_end),
                max_record_bytes: RECEIVER_RECORD_BYTES,
                json: true,
            })?;
            ensure!(
                window.complete_visible_window && window.records == [derived],
                "export_derived_message_mismatch"
            );
        }
        let current = self.session_handoff_locked(
            &request.context_id,
            &request.context_sha256,
            &request.record_id,
            request.revision,
        )?;
        compare_handoff(handoff, &current)?;
        let manifest_path = folder.join("manifest.json");
        let bytes = encoded(&planned.manifest)?;
        exact_file(&manifest_path, &bytes)?;
        Ok(
            json!({"schema":"engineering-memory.session-export-verification/1",
            "status":"matched","manifest_path":manifest_path,"manifest_sha256":sha(&bytes),
            "manifest":planned.manifest,"project_write":false,
            "engineering_adoption":"not_granted","proof_scope":"selected_original_record_bytes_and_session_binding"}),
        )
    }

    pub fn session_export_verify(&self, export_id: &str) -> Result<Value> {
        id(export_id)?;
        let _lock = self.lock()?;
        let folder = self
            .directory
            .canonicalize()?
            .join("session-exports")
            .join(export_id);
        let path = folder.join("intent.json");
        regular(&path)?;
        let intent: Value = read_json(&path)?;
        ensure!(
            intent["schema"] == "engineering-memory.session-export-intent/1",
            "export_intent_schema"
        );
        let request: SessionExportRequest = serde_json::from_value(intent["request"].clone())?;
        shape(&request)?;
        ensure!(
            request.id == export_id && (1..=MAX_BYTES).contains(&request.max_bytes),
            "export_intent_identity"
        );
        self.verify_export_locked(&request, &folder, &intent["handoff"])
    }
}
