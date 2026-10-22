//! Full-history conversation anchor catalog.
//!
//! Only visible user/assistant messages enter the catalog, and every entry is
//! exactly the `SourceAnchor` that `evidence::read_window` returns for the same
//! byte range, so a search hit can be re-read and cited. Builds are bounded,
//! resumable and exclusive; status and search are read-only. Nothing here calls
//! a model or writes experiences.
use crate::contracts::{
    HistoryIndexBuildRequest, HistoryIndexFile, HistoryIndexOversizedLine, HistorySearchHit,
    HistorySearchRequest, SourceAnchor,
};
use crate::evidence::{EvidenceArgs, read_window};
use crate::retrieval::{QueryTerms, text_match};
use crate::source_origin::credential_path;
use crate::store::{atomic_json, contains_credential, private_dir, read_json, sha, shape};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::de::{Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, hash_map::Entry};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const REVISION: &str = "engineering-memory.history-index/1";
/// The evidence reader's window ceiling; a longer line cannot become an anchor.
const WINDOW: u64 = 16 * 1024 * 1024;
const CHUNK: usize = 64 * 1024;
/// Bound the work redone after an interruption inside one large file.
const COMMIT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ROOTS: usize = 64;
const MAX_WORKERS: usize = 6;
const LIST_LIMIT: usize = 100;

fn index_dir(home: &Path) -> PathBuf {
    home.join("history-index")
}
fn manifest_path(dir: &Path, key: &str) -> PathBuf {
    dir.join("files").join(format!("{key}.json"))
}
fn shard_path(dir: &Path, key: &str) -> PathBuf {
    dir.join("shards").join(format!("{key}.jsonl"))
}
fn oversized_path(dir: &Path, key: &str, start: u64) -> PathBuf {
    dir.join("oversized").join(format!("{key}-{start}.json"))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn nanos(time: SystemTime) -> Result<u64> {
    Ok(u64::try_from(time.duration_since(UNIX_EPOCH)?.as_nanos())?)
}
/// Whole seconds and sub-second nanoseconds stay within the 2^53 interop range.
fn split_time(time: SystemTime) -> Result<(u64, u64)> {
    let since = time.duration_since(UNIX_EPOCH)?;
    Ok((since.as_secs(), u64::from(since.subsec_nanos())))
}

struct Root {
    path: PathBuf,
    text: String,
    provider: String,
    profile: String,
}

fn profile_valid(profile: &str) -> bool {
    !profile.trim().is_empty() && profile.len() <= 120 && !profile.chars().any(char::is_control)
}

fn roots(request: &HistoryIndexBuildRequest) -> Result<Vec<Root>> {
    ensure!(
        !request.roots.is_empty() && request.roots.len() <= MAX_ROOTS,
        "history_index_roots_invalid"
    );
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for root in &request.roots {
        let raw = Path::new(&root.path);
        ensure!(raw.is_absolute(), "history_index_root_not_absolute");
        ensure!(
            profile_valid(&root.profile),
            "history_index_profile_invalid"
        );
        let path = raw
            .canonicalize()
            .context("history_index_root_unavailable")?;
        ensure!(
            fs::metadata(&path)?.is_dir(),
            "history_index_root_not_directory"
        );
        ensure!(
            !credential_path(&path),
            "history_index_root_credential_path"
        );
        let text = path
            .to_str()
            .context("history_index_root_not_utf8")?
            .to_owned();
        ensure!(seen.insert(text.clone()), "history_index_duplicate_root");
        result.push(Root {
            path,
            text,
            provider: root.provider.clone(),
            profile: root.profile.clone(),
        });
    }
    Ok(result)
}

struct Candidate {
    path: PathBuf,
    text: String,
    root: usize,
}

#[derive(Default)]
struct Discovery {
    files: Vec<Candidate>,
    symlinks_skipped: u64,
    credential_paths_skipped: u64,
    overlapping_duplicates: u64,
    non_utf8_paths: u64,
    unreadable_directories: u64,
}

fn matches_provider(provider: &str, name: &str) -> bool {
    match provider {
        "codex" => name.starts_with("rollout-") && name.ends_with(".jsonl"),
        _ => name.ends_with(".jsonl"),
    }
}

/// Paths are built from a canonical root and plain directory entries, never by
/// following a link, so every candidate stays inside its declared root.
fn discover(roots: &[Root]) -> Result<Discovery> {
    let mut found: BTreeMap<String, Candidate> = BTreeMap::new();
    let mut result = Discovery::default();
    for (index, root) in roots.iter().enumerate() {
        let mut stack = vec![root.path.clone()];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = fs::read_dir(&directory) else {
                result.unreadable_directories += 1;
                continue;
            };
            for entry in entries {
                let entry = entry?;
                let kind = entry.file_type()?;
                let path = entry.path();
                if kind.is_symlink() {
                    result.symlinks_skipped += 1;
                    continue;
                }
                if credential_path(&path) {
                    result.credential_paths_skipped += 1;
                    continue;
                }
                if kind.is_dir() {
                    stack.push(path);
                    continue;
                }
                if !kind.is_file()
                    || !matches_provider(&root.provider, &entry.file_name().to_string_lossy())
                {
                    continue;
                }
                let Some(text) = path.to_str().map(str::to_owned) else {
                    result.non_utf8_paths += 1;
                    continue;
                };
                if found.contains_key(&text) {
                    result.overlapping_duplicates += 1;
                    continue;
                }
                found.insert(
                    text.clone(),
                    Candidate {
                        path,
                        text,
                        root: index,
                    },
                );
            }
        }
    }
    result.files = found.into_values().collect();
    Ok(result)
}

struct Stat {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: u64,
    modified_nanos: u64,
}

fn stat(meta: &fs::Metadata) -> Result<Stat> {
    #[cfg(unix)]
    let (device, inode) = {
        use std::os::unix::fs::MetadataExt;
        (meta.dev(), meta.ino())
    };
    #[cfg(not(unix))]
    let (device, inode) = (0, 0);
    let (modified_seconds, modified_nanos) = split_time(meta.modified()?)?;
    Ok(Stat {
        device,
        inode,
        size: meta.len(),
        modified_seconds,
        modified_nanos,
    })
}

fn load_manifest(dir: &Path, key: &str, source: &str) -> Result<Option<HistoryIndexFile>> {
    let path = manifest_path(dir, key);
    if !path.exists() {
        return Ok(None);
    }
    let manifest: HistoryIndexFile = read_json(&path)?;
    shape(&manifest)?;
    ensure!(
        manifest.source_path == source,
        "history_index_manifest_identity_changed"
    );
    Ok(Some(manifest))
}

struct Job {
    candidate: Candidate,
    key: String,
    manifest: Option<HistoryIndexFile>,
    reason: &'static str,
}

enum Plan {
    Skip { blocked: bool },
    Work(Box<Job>),
}

fn plan(dir: &Path, candidate: Candidate, verify_all: bool) -> Result<Plan> {
    let key = sha(candidate.text.as_bytes());
    let Some(manifest) = load_manifest(dir, &key, &candidate.text)? else {
        return Ok(Plan::Work(Box::new(Job {
            candidate,
            key,
            manifest: None,
            reason: "new",
        })));
    };
    let current = stat(&fs::symlink_metadata(&candidate.path)?)?;
    let unchanged = manifest.device == current.device
        && manifest.inode == current.inode
        && manifest.source_size == current.size
        && manifest.modified_seconds == current.modified_seconds
        && manifest.modified_nanos == current.modified_nanos;
    let settled = manifest.complete || manifest.blocked_at.is_some();
    if unchanged && settled && !verify_all {
        return Ok(Plan::Skip {
            blocked: manifest.blocked_at.is_some(),
        });
    }
    let reason = match (unchanged, settled) {
        (true, true) => "audit",
        (true, false) => "resume",
        (false, _) => "changed",
    };
    Ok(Plan::Work(Box::new(Job {
        candidate,
        key,
        manifest: Some(manifest),
        reason,
    })))
}

struct Shared<'a> {
    dir: &'a Path,
    roots: &'a [Root],
    deadline: Instant,
    min_free_bytes: u64,
    stop: AtomicBool,
    stop_reason: Mutex<Option<&'static str>>,
}

impl Shared<'_> {
    fn halt(&self, reason: &'static str) {
        let mut current = self.stop_reason.lock().unwrap();
        if current.is_none() {
            *current = Some(reason);
        }
        self.stop.store(true, Ordering::SeqCst);
    }
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
    fn space_available(&self) -> Result<bool> {
        if fs2::available_space(self.dir)? < self.min_free_bytes {
            self.halt("min_free_bytes");
            return Ok(false);
        }
        Ok(true)
    }
}

#[derive(Default)]
struct Outcome {
    path: String,
    reason: &'static str,
    state: &'static str,
    superseded: Option<String>,
    anchors_added: u64,
    bytes_indexed: u64,
    prefix_verified_bytes: u64,
    oversized_skipped: u64,
    credential_screened: u64,
    uncommitted_tail_discarded: u64,
    blocked_at: Option<u64>,
    error: Option<String>,
}

fn supersede(dir: &Path, key: &str, reason: &str) -> Result<String> {
    let name = format!("{key}-{}-{reason}", nanos(SystemTime::now())?);
    let target = dir.join("superseded").join(&name);
    private_dir(&target)?;
    for (from, to) in [
        (shard_path(dir, key), target.join("shard.jsonl")),
        (manifest_path(dir, key), target.join("file.json")),
    ] {
        if from.exists() {
            fs::rename(from, to)?;
        }
    }
    let oversized = dir.join("oversized");
    if oversized.is_dir() {
        let prefix = format!("{key}-");
        for entry in fs::read_dir(&oversized)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                private_dir(&target.join("oversized"))?;
                fs::rename(
                    entry.path(),
                    target.join("oversized").join(entry.file_name()),
                )?;
            }
        }
    }
    Ok(name)
}

fn has_residue(dir: &Path, key: &str) -> Result<bool> {
    if shard_path(dir, key).exists() {
        return Ok(true);
    }
    let oversized = dir.join("oversized");
    if !oversized.is_dir() {
        return Ok(false);
    }
    let prefix = format!("{key}-");
    for entry in fs::read_dir(&oversized)? {
        if entry?.file_name().to_string_lossy().starts_with(&prefix) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Stream `[0, end)` into `hasher` in 64 KiB chunks and return the digest so far.
fn hash_prefix(file: &mut File, end: u64, hasher: &mut Sha256) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut buffer = vec![0; CHUNK];
    let mut remaining = end;
    while remaining > 0 {
        let take = remaining.min(CHUNK as u64) as usize;
        file.read_exact(&mut buffer[..take])?;
        hasher.update(&buffer[..take]);
        remaining -= take as u64;
    }
    Ok(hex::encode(hasher.clone().finalize()))
}

fn fresh_manifest(job: &Job, root: &Root, current: &Stat) -> HistoryIndexFile {
    HistoryIndexFile {
        schema: "engineering-memory.history-index-file/1".into(),
        source_path: job.candidate.text.clone(),
        root: root.text.clone(),
        provider: root.provider.clone(),
        profile: root.profile.clone(),
        device: current.device,
        inode: current.inode,
        source_size: current.size,
        modified_seconds: current.modified_seconds,
        modified_nanos: current.modified_nanos,
        cursor: 0,
        prefix_sha256: hex::encode(Sha256::digest(b"")),
        shard_bytes: 0,
        complete: false,
        anchors: 0,
        excluded_records: 0,
        credential_screened: 0,
        oversized_skipped: 0,
        invalid_json: 0,
        identity_missing: 0,
        blocked_at: None,
        blocked_reason: None,
        indexed_at: now(),
    }
}

/// Shard bytes are appended and synced first; the manifest is replaced after,
/// so an interruption leaves at most an uncommitted shard tail.
fn commit(
    shared: &Shared,
    key: &str,
    manifest: &mut HistoryIndexFile,
    pending: &mut Vec<u8>,
    hasher: &Sha256,
) -> Result<bool> {
    if !shared.space_available()? {
        return Ok(false);
    }
    // Validate the next manifest before any byte is written.
    let mut next = manifest.clone();
    next.shard_bytes += pending.len() as u64;
    next.prefix_sha256 = hex::encode(hasher.clone().finalize());
    next.indexed_at = now();
    shape(&next)?;
    if !pending.is_empty() {
        let path = shard_path(shared.dir, key);
        private_dir(path.parent().context("history_index_shard_parent")?)?;
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        ensure!(
            file.metadata()?.len() == manifest.shard_bytes,
            "history_index_shard_length_changed"
        );
        file.write_all(pending)?;
        file.sync_data()?;
    }
    atomic_json(&manifest_path(shared.dir, key), &next)?;
    *manifest = next;
    pending.clear();
    Ok(true)
}

fn process(shared: &Shared, job: Job) -> Outcome {
    let mut outcome = Outcome {
        path: job.candidate.text.clone(),
        reason: job.reason,
        ..Outcome::default()
    };
    match index_file(shared, &job, &mut outcome) {
        Ok(state) => outcome.state = state,
        Err(error) => {
            outcome.state = "failed";
            outcome.error = Some(format!("{error:#}"));
        }
    }
    outcome
}

fn index_file(shared: &Shared, job: &Job, outcome: &mut Outcome) -> Result<&'static str> {
    let root = &shared.roots[job.candidate.root];
    ensure!(
        !fs::symlink_metadata(&job.candidate.path)?
            .file_type()
            .is_symlink(),
        "history_index_source_symlink"
    );
    let mut file = File::open(&job.candidate.path)?;
    let meta = file.metadata()?;
    ensure!(meta.is_file(), "history_index_source_not_regular");
    let current = stat(&meta)?;
    if !shared.space_available()? {
        return Ok("stopped");
    }
    let mut manifest = job.manifest.clone();
    if let Some(old) = &manifest {
        let replaced = old.device != current.device || old.inode != current.inode;
        if replaced || current.size < old.cursor {
            outcome.superseded = Some(supersede(
                shared.dir,
                &job.key,
                if replaced { "replaced" } else { "truncated" },
            )?);
            manifest = None;
        }
    }
    let mut hasher = Sha256::new();
    if let Some(old) = &manifest {
        let digest = hash_prefix(&mut file, old.cursor, &mut hasher)?;
        outcome.prefix_verified_bytes = old.cursor;
        let shard = shard_path(shared.dir, &job.key);
        let shard_len = fs::metadata(&shard).map(|m| m.len()).unwrap_or(0);
        if digest != old.prefix_sha256 || shard_len < old.shard_bytes {
            let reason = if digest != old.prefix_sha256 {
                "prefix_changed"
            } else {
                "shard_damaged"
            };
            outcome.superseded = Some(supersede(shared.dir, &job.key, reason)?);
            manifest = None;
            hasher = Sha256::new();
        } else if shard_len > old.shard_bytes {
            // Appended before an interrupted manifest commit: never mix it in.
            let handle = OpenOptions::new().write(true).open(&shard)?;
            handle.set_len(old.shard_bytes)?;
            handle.sync_all()?;
            outcome.uncommitted_tail_discarded = shard_len - old.shard_bytes;
        }
    }
    if manifest.is_none() && has_residue(shared.dir, &job.key)? {
        // Files without a manifest were never committed: move them aside, never extend them.
        outcome.superseded = Some(supersede(shared.dir, &job.key, "uncommitted_shard")?);
    }
    let mut state = match manifest {
        Some(mut old) => {
            old.device = current.device;
            old.inode = current.inode;
            old.source_size = current.size;
            old.modified_seconds = current.modified_seconds;
            old.modified_nanos = current.modified_nanos;
            old.complete = false;
            old.blocked_at = None;
            old.blocked_reason = None;
            old
        }
        None => fresh_manifest(job, root, &current),
    };
    let mut pending = Vec::new();
    let mut since_commit = 0u64;
    let mut buffer = vec![0u8; usize::try_from(WINDOW)?];
    // Only bytes present when processing started are indexed; later appends
    // change the size or mtime and are picked up by the next build.
    let limit = current.size;
    let verdict = loop {
        if shared.stopped() {
            break "stopped";
        }
        if Instant::now() >= shared.deadline {
            shared.halt("max_seconds");
            break "stopped";
        }
        if state.cursor >= limit {
            state.complete = true;
            break "complete";
        }
        let want = usize::try_from((limit - state.cursor).min(WINDOW))?;
        file.seek(SeekFrom::Start(state.cursor))?;
        file.read_exact(&mut buffer[..want])?;
        match buffer[..want].iter().rposition(|byte| *byte == b'\n') {
            Some(index) => {
                let boundary = state.cursor + index as u64 + 1;
                let window = read_window(&EvidenceArgs {
                    source: job.candidate.path.clone(),
                    provider: root.provider.clone(),
                    profile: Some(root.profile.clone()),
                    start_byte: state.cursor,
                    end_byte: Some(boundary),
                    max_record_bytes: WINDOW,
                    json: true,
                })?;
                let consumed = window
                    .next_byte
                    .filter(|next| *next > state.cursor && *next < boundary)
                    .unwrap_or(boundary);
                for gap in &window.gaps {
                    match gap.reason.as_str() {
                        "invalid_json" => state.invalid_json += 1,
                        "session_identity_missing" => state.identity_missing += 1,
                        "result_count_budget" => {}
                        other => bail!("history_index_unexpected_gap:{other}"),
                    }
                }
                for record in window.records {
                    ensure!(
                        record.byte_start >= state.cursor && record.byte_end <= consumed,
                        "history_index_anchor_outside_window"
                    );
                    if contains_credential(&record.text) {
                        state.credential_screened += 1;
                        outcome.credential_screened += 1;
                        continue;
                    }
                    serde_json::to_writer(&mut pending, &record)?;
                    pending.push(b'\n');
                    state.anchors += 1;
                    outcome.anchors_added += 1;
                }
                state.excluded_records += window.excluded_records;
                let used = consumed - state.cursor;
                hasher.update(&buffer[..usize::try_from(used)?]);
                outcome.bytes_indexed += used;
                since_commit += used;
                state.cursor = consumed;
            }
            // A final line without a newline is still being written.
            None if (want as u64) < WINDOW => {
                state.complete = true;
                break "complete";
            }
            None => {
                let first = &buffer[..want];
                match oversized(
                    shared,
                    job,
                    root,
                    &mut file,
                    state.cursor,
                    limit,
                    first,
                    &hasher,
                )? {
                    Oversized::Partial => {
                        state.complete = true;
                        break "complete";
                    }
                    Oversized::Skipped { end, prefix } => {
                        hasher = prefix;
                        let used = end - state.cursor;
                        state.oversized_skipped += 1;
                        outcome.oversized_skipped += 1;
                        outcome.bytes_indexed += used;
                        since_commit += used;
                        state.cursor = end;
                    }
                    Oversized::Blocked { reason } => {
                        state.blocked_at = Some(state.cursor);
                        state.blocked_reason = Some(reason.into());
                        outcome.blocked_at = Some(state.cursor);
                        break "blocked";
                    }
                }
            }
        }
        if since_commit >= COMMIT_BYTES {
            if !commit(shared, &job.key, &mut state, &mut pending, &hasher)? {
                return Ok("stopped");
            }
            since_commit = 0;
        }
    };
    if !commit(shared, &job.key, &mut state, &mut pending, &hasher)? {
        return Ok("stopped");
    }
    Ok(verdict)
}

enum Oversized {
    /// No newline before the size seen at start: the line is still being written.
    Partial,
    Skipped {
        end: u64,
        prefix: Sha256,
    },
    Blocked {
        reason: &'static str,
    },
}

#[allow(clippy::too_many_arguments)]
fn oversized(
    shared: &Shared,
    job: &Job,
    root: &Root,
    file: &mut File,
    start: u64,
    limit: u64,
    first: &[u8],
    hasher: &Sha256,
) -> Result<Oversized> {
    let mut prefix = hasher.clone();
    let mut line = Sha256::new();
    prefix.update(first);
    line.update(first);
    let mut position = start + first.len() as u64;
    let mut buffer = vec![0; CHUNK];
    let end = loop {
        if position >= limit {
            return Ok(Oversized::Partial);
        }
        let take = (limit - position).min(CHUNK as u64) as usize;
        file.seek(SeekFrom::Start(position))?;
        file.read_exact(&mut buffer[..take])?;
        if let Some(index) = buffer[..take].iter().position(|byte| *byte == b'\n') {
            prefix.update(&buffer[..=index]);
            line.update(&buffer[..=index]);
            break position + index as u64 + 1;
        }
        prefix.update(&buffer[..take]);
        line.update(&buffer[..take]);
        position += take as u64;
    };
    let digest = hex::encode(line.finalize());
    let (class, head, reread) =
        classify_oversized_line(&job.candidate.path, start, end, &root.provider)?;
    ensure!(reread == digest, "source_changed_during_read");
    let reason = match class {
        LineClass::Message => {
            return Ok(Oversized::Blocked {
                reason: "oversized_visible_message",
            });
        }
        LineClass::Ambiguous => {
            return Ok(Oversized::Blocked {
                reason: "oversized_line_unclassifiable",
            });
        }
        LineClass::InvalidJson => "invalid_json",
        LineClass::NotMessage => "not_visible_message",
    };
    let body = match root.provider.as_str() {
        "codex" => head.payload.as_ref(),
        _ => head.message.as_ref(),
    };
    let receipt = HistoryIndexOversizedLine {
        schema: "engineering-memory.history-index-oversized-line/1".into(),
        source_path: job.candidate.text.clone(),
        byte_start: start,
        byte_end: end,
        sha256: digest,
        reason: reason.into(),
        record_type: head.kind.clone(),
        body_type: body.and_then(|b| b.kind.clone()),
        body_role: body.and_then(|b| b.role.clone()),
    };
    shape(&receipt)?;
    let path = oversized_path(shared.dir, &job.key, start);
    if path.exists() {
        let old: HistoryIndexOversizedLine = read_json(&path)?;
        ensure!(old == receipt, "history_index_oversized_receipt_conflict");
    } else {
        atomic_json(&path, &receipt)?;
    }
    Ok(Oversized::Skipped { end, prefix })
}

/// Hashes exactly the bytes the streaming parser consumes, so the verdict is
/// bound to the same line identity as the prefix digest.
struct HashingReader<R> {
    inner: R,
    hasher: Sha256,
    remaining: u64,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let take = buffer
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        if take == 0 {
            return Ok(0);
        }
        let read = self.inner.read(&mut buffer[..take])?;
        self.hasher.update(&buffer[..read]);
        self.remaining -= read as u64;
        Ok(read)
    }
}

#[derive(Debug, PartialEq)]
enum LineClass {
    Message,
    Ambiguous,
    InvalidJson,
    NotMessage,
}

fn classify_oversized_line(
    path: &Path,
    start: u64,
    end: u64,
    provider: &str,
) -> Result<(LineClass, Head, String)> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut reader = HashingReader {
        inner: BufReader::with_capacity(CHUNK, file),
        hasher: Sha256::new(),
        remaining: end - start,
    };
    let parsed = {
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        Head::deserialize(&mut deserializer).and_then(|head| {
            deserializer.end()?;
            Ok(head)
        })
    };
    std::io::copy(&mut reader, &mut std::io::sink())?;
    let digest = hex::encode(reader.hasher.finalize());
    Ok(match parsed {
        Ok(head) => (head.verdict(provider), head, digest),
        Err(_) => (LineClass::InvalidJson, Head::default(), digest),
    })
}

/// Any JSON value, kept only when it is a string, matching `as_str()`.
enum Scalar {
    Text(String),
    Other,
}

impl Scalar {
    fn text(self) -> Option<String> {
        match self {
            Scalar::Text(value) => Some(value),
            Scalar::Other => None,
        }
    }
}

macro_rules! ignore_scalars {
    ($value:ty, $default:expr) => {
        fn visit_bool<E>(self, _: bool) -> Result<$value, E> {
            Ok($default)
        }
        fn visit_i64<E>(self, _: i64) -> Result<$value, E> {
            Ok($default)
        }
        fn visit_u64<E>(self, _: u64) -> Result<$value, E> {
            Ok($default)
        }
        fn visit_f64<E>(self, _: f64) -> Result<$value, E> {
            Ok($default)
        }
        fn visit_unit<E>(self) -> Result<$value, E> {
            Ok($default)
        }
    };
}

impl<'de> Deserialize<'de> for Scalar {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ScalarVisitor;
        impl<'de> Visitor<'de> for ScalarVisitor {
            type Value = Scalar;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_str<E>(self, value: &str) -> Result<Scalar, E> {
                Ok(Scalar::Text(value.to_owned()))
            }
            fn visit_string<E>(self, value: String) -> Result<Scalar, E> {
                Ok(Scalar::Text(value))
            }
            ignore_scalars!(Scalar, Scalar::Other);
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Scalar, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Scalar::Other)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Scalar, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Scalar::Other)
            }
        }
        deserializer.deserialize_any(ScalarVisitor)
    }
}

/// One content block: visible exactly when `type` is a text kind and `text` is a string.
#[derive(Default)]
struct Block {
    visible: bool,
    ambiguous: bool,
}

impl<'de> Deserialize<'de> for Block {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BlockVisitor;
        impl<'de> Visitor<'de> for BlockVisitor {
            type Value = Block;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a content block")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Block, A::Error> {
                let mut kind: Option<Option<String>> = None;
                let mut text: Option<bool> = None;
                let mut ambiguous = false;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "type" => {
                            let value = map.next_value::<Scalar>()?.text();
                            ambiguous |= kind.replace(value).is_some();
                        }
                        "text" => {
                            let value = matches!(map.next_value::<Scalar>()?, Scalar::Text(_));
                            ambiguous |= text.replace(value).is_some();
                        }
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                let visible = matches!(
                    kind.flatten().as_deref(),
                    Some("text" | "input_text" | "output_text")
                ) && text == Some(true);
                Ok(Block { visible, ambiguous })
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Block, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Block::default())
            }
            fn visit_str<E>(self, _: &str) -> Result<Block, E> {
                Ok(Block::default())
            }
            ignore_scalars!(Block, Block::default());
        }
        deserializer.deserialize_any(BlockVisitor)
    }
}

#[derive(PartialEq)]
enum Content {
    Visible,
    Invisible,
    Ambiguous,
}

impl<'de> Deserialize<'de> for Content {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ContentVisitor;
        impl<'de> Visitor<'de> for ContentVisitor {
            type Value = Content;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("message content")
            }
            // A string content is visible even when empty, as in `visible_text`.
            fn visit_str<E>(self, _: &str) -> Result<Content, E> {
                Ok(Content::Visible)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Content, A::Error> {
                let (mut visible, mut ambiguous) = (false, false);
                while let Some(block) = seq.next_element::<Block>()? {
                    visible |= block.visible;
                    ambiguous |= block.ambiguous;
                }
                Ok(if ambiguous {
                    Content::Ambiguous
                } else if visible {
                    Content::Visible
                } else {
                    Content::Invisible
                })
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Content, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Content::Invisible)
            }
            ignore_scalars!(Content, Content::Invisible);
        }
        deserializer.deserialize_any(ContentVisitor)
    }
}

/// The message body fields the evidence reader's `message()` inspects.
#[derive(Default)]
struct Body {
    kind: Option<String>,
    role: Option<String>,
    channel: Option<String>,
    phase: Option<String>,
    content: Option<Content>,
    ambiguous: bool,
}

impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BodyVisitor;
        impl<'de> Visitor<'de> for BodyVisitor {
            type Value = Body;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a message body")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Body, A::Error> {
                let mut body = Body::default();
                let mut seen = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    let tracked = matches!(
                        key.as_str(),
                        "type" | "role" | "channel" | "phase" | "content"
                    );
                    if tracked && !seen.insert(key.clone()) {
                        body.ambiguous = true;
                    }
                    match key.as_str() {
                        "type" => body.kind = map.next_value::<Scalar>()?.text(),
                        "role" => body.role = map.next_value::<Scalar>()?.text(),
                        "channel" => body.channel = map.next_value::<Scalar>()?.text(),
                        "phase" => body.phase = map.next_value::<Scalar>()?.text(),
                        "content" => body.content = Some(map.next_value::<Content>()?),
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(body)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Body, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Body::default())
            }
            fn visit_str<E>(self, _: &str) -> Result<Body, E> {
                Ok(Body::default())
            }
            ignore_scalars!(Body, Body::default());
        }
        deserializer.deserialize_any(BodyVisitor)
    }
}

/// Only the keys that decide visibility are captured; everything else, which
/// is where an oversized line's bulk lives, is skipped without being stored.
#[derive(Default)]
struct Head {
    kind: Option<String>,
    payload: Option<Body>,
    message: Option<Body>,
    ambiguous: bool,
}

impl<'de> Deserialize<'de> for Head {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HeadVisitor;
        impl<'de> Visitor<'de> for HeadVisitor {
            type Value = Head;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSONL record")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Head, A::Error> {
                let mut head = Head::default();
                let mut seen = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    let tracked = matches!(key.as_str(), "type" | "payload" | "message");
                    if tracked && !seen.insert(key.clone()) {
                        head.ambiguous = true;
                    }
                    match key.as_str() {
                        "type" => head.kind = map.next_value::<Scalar>()?.text(),
                        "payload" => head.payload = Some(map.next_value::<Body>()?),
                        "message" => head.message = Some(map.next_value::<Body>()?),
                        _ => {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                }
                Ok(head)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Head, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Head::default())
            }
            fn visit_str<E>(self, _: &str) -> Result<Head, E> {
                Ok(Head::default())
            }
            ignore_scalars!(Head, Head::default());
        }
        deserializer.deserialize_any(HeadVisitor)
    }
}

impl Head {
    /// Mirrors the evidence reader's `message()`: a duplicate of a deciding key
    /// makes the verdict unknowable, and such a line is never skipped.
    fn verdict(&self, provider: &str) -> LineClass {
        if self.ambiguous {
            return LineClass::Ambiguous;
        }
        let body = match provider {
            "codex" if self.kind.as_deref() == Some("response_item") => self
                .payload
                .as_ref()
                .filter(|body| body.ambiguous || body.kind.as_deref() == Some("message")),
            "claude" if matches!(self.kind.as_deref(), Some("user" | "assistant")) => {
                self.message.as_ref()
            }
            _ => None,
        };
        let Some(body) = body else {
            return LineClass::NotMessage;
        };
        if body.ambiguous {
            return LineClass::Ambiguous;
        }
        if !matches!(body.role.as_deref(), Some("user" | "assistant"))
            || matches!(body.channel.as_deref(), Some("analysis" | "summary"))
            || matches!(body.phase.as_deref(), Some("analysis" | "summary"))
        {
            return LineClass::NotMessage;
        }
        match body.content {
            Some(Content::Visible) => LineClass::Message,
            Some(Content::Ambiguous) => LineClass::Ambiguous,
            _ => LineClass::NotMessage,
        }
    }
}

fn build_lock(dir: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(dir.join("build.lock"))?;
    file.try_lock_exclusive()
        .context("history_index_build_busy")?;
    Ok(file)
}

fn limited<T>(items: Vec<T>) -> (Vec<T>, usize) {
    let total = items.len();
    (items.into_iter().take(LIST_LIMIT).collect(), total)
}

/// Bounded, resumable and exclusive. Every commit covers whole records only.
pub fn build(home: &Path, request: &HistoryIndexBuildRequest) -> Result<Value> {
    build_until(home, request, None)
}

fn build_until(
    home: &Path,
    request: &HistoryIndexBuildRequest,
    deadline: Option<Instant>,
) -> Result<Value> {
    shape(request)?;
    ensure!(
        (1..=100_000).contains(&request.max_files),
        "history_index_max_files_invalid"
    );
    ensure!(
        (1..=86_400).contains(&request.max_seconds),
        "history_index_max_seconds_invalid"
    );
    let roots = roots(request)?;
    let started = Instant::now();
    let started_at = now();
    let dir = index_dir(home);
    private_dir(&dir)?;
    let _lock = build_lock(&dir)?;
    let shared = Shared {
        dir: &dir,
        roots: &roots,
        deadline: deadline.unwrap_or(started + Duration::from_secs(request.max_seconds)),
        min_free_bytes: request.min_free_bytes,
        stop: AtomicBool::new(false),
        stop_reason: Mutex::new(None),
    };
    let discovery = discover(&roots)?;
    let discovered = discovery.files.len();
    let (mut unchanged, mut blocked_unchanged) = (0u64, 0u64);
    let mut work = Vec::new();
    let mut planning_failures = Vec::new();
    for candidate in discovery.files {
        let text = candidate.text.clone();
        match plan(&dir, candidate, request.verify == "all") {
            Ok(Plan::Skip { blocked }) => {
                unchanged += 1;
                blocked_unchanged += u64::from(blocked);
            }
            Ok(Plan::Work(job)) => work.push(job),
            Err(error) => planning_failures.push(json!({"path":text,"error":format!("{error:#}")})),
        }
    }
    let needing_work = work.len();
    let selected = usize::try_from(request.max_files)?.min(needing_work);
    let deferred_by_limit = needing_work - selected;
    let queue = Mutex::new(work.into_iter().take(selected));
    let outcomes = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(1)
        .clamp(1, MAX_WORKERS)
        .min(selected.max(1));
    if shared.space_available()? {
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| {
                    while !shared.stopped() {
                        let Some(job) = queue.lock().unwrap().next() else {
                            break;
                        };
                        let outcome = process(&shared, *job);
                        outcomes.lock().unwrap().push(outcome);
                    }
                });
            }
        });
    }
    let unstarted = queue.into_inner().unwrap().count();
    let mut outcomes = outcomes.into_inner().unwrap();
    outcomes.sort_by(|a, b| a.path.cmp(&b.path));
    let mut totals = BTreeMap::<&str, u64>::new();
    let (mut blocked, mut failed, mut superseded) = (Vec::new(), Vec::new(), Vec::new());
    let mut unfinished = 0u64;
    for outcome in &outcomes {
        *totals.entry(outcome.state).or_default() += 1;
        *totals.entry("anchors_added").or_default() += outcome.anchors_added;
        *totals.entry("bytes_indexed").or_default() += outcome.bytes_indexed;
        *totals.entry("prefix_verified_bytes").or_default() += outcome.prefix_verified_bytes;
        *totals.entry("oversized_skipped").or_default() += outcome.oversized_skipped;
        *totals.entry("credential_screened").or_default() += outcome.credential_screened;
        *totals.entry("uncommitted_tail_discarded").or_default() +=
            outcome.uncommitted_tail_discarded;
        if matches!(outcome.state, "stopped" | "failed") {
            unfinished += 1;
        }
        if let Some(at) = outcome.blocked_at {
            blocked.push(json!({"path":outcome.path,"blocked_at":at}));
        }
        if let Some(error) = &outcome.error {
            failed.push(json!({"path":outcome.path,"reason":outcome.reason,"error":error}));
        }
        if let Some(name) = &outcome.superseded {
            superseded.push(json!({"path":outcome.path,"superseded":name}));
        }
    }
    let stop_reason = shared
        .stop_reason
        .lock()
        .unwrap()
        .unwrap_or(if deferred_by_limit > 0 {
            "max_files"
        } else {
            "complete"
        });
    let pending_files = deferred_by_limit as u64 + unstarted as u64 + unfinished;
    let get = |key: &str| totals.get(key).copied().unwrap_or(0);
    let (blocked, blocked_total) = limited(blocked);
    let (failed, failed_total) = limited(failed);
    let (superseded, superseded_total) = limited(superseded);
    let (planning_failures, planning_failure_total) = limited(planning_failures);
    let build_id = format!("b{}-{}", nanos(SystemTime::now())?, std::process::id());
    let receipt = json!({
        "schema":"engineering-memory.history-index-build/1","revision":REVISION,"build_id":build_id,
        "started_at":started_at,"finished_at":now(),"elapsed_ms":started.elapsed().as_millis() as u64,
        "roots":roots.iter().map(|r| json!({"path":r.text,"provider":r.provider,"profile":r.profile})).collect::<Vec<_>>(),
        "verify":request.verify,"max_files":request.max_files,"max_seconds":request.max_seconds,
        "min_free_bytes":request.min_free_bytes,"workers":workers,
        "discovered_files":discovered,"unchanged_skipped":unchanged,"blocked_unchanged":blocked_unchanged,
        "needing_work":needing_work,"selected_files":selected,"processed_files":outcomes.len(),
        "states":{"complete":get("complete"),"blocked":get("blocked"),"stopped":get("stopped"),"failed":get("failed")},
        "anchors_added":get("anchors_added"),"bytes_indexed":get("bytes_indexed"),
        "prefix_verified_bytes":get("prefix_verified_bytes"),"oversized_skipped":get("oversized_skipped"),
        "credential_screened":get("credential_screened"),
        "uncommitted_tail_discarded":get("uncommitted_tail_discarded"),
        "pending_files":pending_files,"stop_reason":stop_reason,
        "blocked":blocked,"blocked_total":blocked_total,"failed":failed,"failed_total":failed_total,
        "superseded":superseded,"superseded_total":superseded_total,
        "planning_failures":planning_failures,"planning_failure_total":planning_failure_total,
        "discovery":{"symlinks_skipped":discovery.symlinks_skipped,
            "credential_paths_skipped":discovery.credential_paths_skipped,
            "overlapping_duplicates":discovery.overlapping_duplicates,
            "non_utf8_paths":discovery.non_utf8_paths,
            "unreadable_directories":discovery.unreadable_directories},
        "provider_calls":0,"semantic_read":false,"engineering_adoption":"not_granted",
        "scope":"Visible user/assistant anchors only; hits must be re-read with read_window before citing."
    });
    // The receipt is written only when the build itself may still write.
    if fs2::available_space(&dir)? >= request.min_free_bytes {
        atomic_json(
            &dir.join("builds").join(format!("{build_id}.json")),
            &receipt,
        )?;
        atomic_json(&dir.join("last-build.json"), &receipt)?;
    }
    Ok(receipt)
}

fn manifests(dir: &Path) -> Result<Vec<HistoryIndexFile>> {
    let files = dir.join("files");
    let mut result = Vec::new();
    if !files.is_dir() {
        return Ok(result);
    }
    for entry in fs::read_dir(&files)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "json") && path.is_file() {
            let manifest: HistoryIndexFile = read_json(&path)?;
            shape(&manifest)?;
            result.push(manifest);
        }
    }
    result.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    Ok(result)
}

/// Read-only: never creates the catalog directory or takes the build lock.
pub fn status(home: &Path) -> Result<Value> {
    let dir = index_dir(home);
    let files = manifests(&dir)?;
    let mut totals = BTreeMap::<&str, u64>::new();
    let mut blocked = Vec::new();
    for file in &files {
        *totals.entry("anchors").or_default() += file.anchors;
        *totals.entry("shard_bytes").or_default() += file.shard_bytes;
        *totals.entry("indexed_source_bytes").or_default() += file.cursor;
        *totals.entry("source_bytes").or_default() += file.source_size;
        *totals.entry("oversized_skipped").or_default() += file.oversized_skipped;
        *totals.entry("credential_screened").or_default() += file.credential_screened;
        *totals.entry("excluded_records").or_default() += file.excluded_records;
        *totals.entry("invalid_json").or_default() += file.invalid_json;
        *totals.entry("identity_missing").or_default() += file.identity_missing;
        let state = if file.blocked_at.is_some() {
            "blocked_files"
        } else if file.complete {
            "complete_files"
        } else {
            "partial_files"
        };
        *totals.entry(state).or_default() += 1;
        if let Some(at) = file.blocked_at {
            blocked.push(
                json!({"path":file.source_path,"blocked_at":at,"reason":file.blocked_reason}),
            );
        }
    }
    let (blocked, blocked_total) = limited(blocked);
    let last_build_path = dir.join("last-build.json");
    let last_build: Value = if last_build_path.is_file() {
        read_json(&last_build_path)?
    } else {
        Value::Null
    };
    let get = |key: &str| totals.get(key).copied().unwrap_or(0);
    Ok(json!({
        "schema":"engineering-memory.history-index-status/1","revision":REVISION,
        "exists":dir.is_dir(),"files":files.len(),"complete_files":get("complete_files"),
        "partial_files":get("partial_files"),"blocked_files":get("blocked_files"),
        "anchors":get("anchors"),"shard_bytes":get("shard_bytes"),
        "indexed_source_bytes":get("indexed_source_bytes"),"source_bytes":get("source_bytes"),
        "oversized_skipped":get("oversized_skipped"),"credential_screened":get("credential_screened"),
        "excluded_records":get("excluded_records"),"invalid_json":get("invalid_json"),
        "identity_missing":get("identity_missing"),"blocked":blocked,"blocked_total":blocked_total,
        "pending_files_at_last_build":last_build["pending_files"],"last_build":last_build,
        "read_only":true,"provider_calls":0
    }))
}

/// (distinct query terms matched, recall density score, role+text digest).
/// Greater is better; the digest only breaks exact ties, deterministically.
type Rank = (u64, u64, [u8; 32]);

/// One distinct text. Resumed and forked sessions replay their history, so the
/// same message can be stored many times; copies collapse into the earliest.
struct Ranked {
    coverage: u64,
    score: u64,
    key: [u8; 32],
    anchor: SourceAnchor,
    copies: u64,
}

impl Ranked {
    fn rank(&self) -> Rank {
        (self.coverage, self.score, self.key)
    }
    /// Keep the earliest copy (path, then position) as the representative.
    fn absorb(&mut self, anchor: SourceAnchor, copies: u64) {
        self.copies += copies;
        if (anchor.source_path.as_str(), anchor.byte_start)
            < (self.anchor.source_path.as_str(), self.anchor.byte_start)
        {
            self.anchor = anchor;
        }
    }
}

fn text_key(anchor: &SourceAnchor) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(anchor.role.as_bytes());
    hasher.update([0]);
    hasher.update(anchor.text.as_bytes());
    hasher.finalize().into()
}

/// Top-k over distinct texts for one worker. All copies of a text share its
/// rank, so a text in the overall top-k is in the top-k of every worker that
/// saw it, and its copy count there is complete.
struct TopK {
    limit: usize,
    members: HashMap<[u8; 32], Ranked>,
    heap: BinaryHeap<Reverse<Rank>>,
}

impl TopK {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            members: HashMap::new(),
            heap: BinaryHeap::new(),
        }
    }
    /// While full: the lowest (coverage, score) still held. Anything strictly
    /// below it can neither enter nor be a copy of a member.
    fn floor(&self) -> Option<(u64, u64)> {
        if self.heap.len() < self.limit {
            return None;
        }
        self.heap.peek().map(|Reverse((c, s, _))| (*c, *s))
    }
    fn offer(&mut self, coverage: u64, score: u64, anchor: SourceAnchor) {
        let key = text_key(&anchor);
        if let Some(member) = self.members.get_mut(&key) {
            member.absorb(anchor, 1);
            return;
        }
        let rank = (coverage, score, key);
        if self.heap.len() >= self.limit {
            match self.heap.peek() {
                Some(Reverse(worst)) if rank > *worst => {}
                _ => return,
            }
            if let Some(Reverse(evicted)) = self.heap.pop() {
                self.members.remove(&evicted.2);
            }
        }
        self.heap.push(Reverse(rank));
        self.members.insert(
            key,
            Ranked {
                coverage,
                score,
                key,
                anchor,
                copies: 1,
            },
        );
    }
}

type Moment = chrono::DateTime<chrono::FixedOffset>;

struct Filter {
    provider: Option<String>,
    profile: Option<String>,
    session_id: Option<String>,
    since: Option<Moment>,
    until: Option<Moment>,
}

impl Filter {
    fn admits(&self, anchor: &SourceAnchor) -> bool {
        if self
            .provider
            .as_ref()
            .is_some_and(|p| *p != anchor.provider)
            || self
                .profile
                .as_ref()
                .is_some_and(|p| Some(p) != anchor.profile.as_ref())
            || self
                .session_id
                .as_ref()
                .is_some_and(|s| *s != anchor.session_id)
        {
            return false;
        }
        if self.since.is_none() && self.until.is_none() {
            return true;
        }
        let Some(at) = anchor
            .event_at
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        else {
            return false;
        };
        self.since.is_none_or(|since| at >= since) && self.until.is_none_or(|until| at <= until)
    }
}

#[derive(Default)]
struct Scan {
    files: u64,
    anchors: u64,
    admitted: u64,
    matched: u64,
    empty_files: u64,
    unavailable_shards: u64,
}

struct Query<'a> {
    terms: &'a QueryTerms,
    filter: &'a Filter,
    limit: usize,
}

fn scan_shard(
    dir: &Path,
    manifest: &HistoryIndexFile,
    query: &Query,
    top: &mut TopK,
    scan: &mut Scan,
) -> Result<()> {
    if manifest.shard_bytes == 0 {
        // A session without visible messages has no shard to read.
        scan.empty_files += 1;
        return Ok(());
    }
    let key = sha(manifest.source_path.as_bytes());
    let Ok(file) = File::open(shard_path(dir, &key)) else {
        scan.unavailable_shards += 1;
        return Ok(());
    };
    scan.files += 1;
    // Only committed bytes are part of the catalog.
    let mut reader = BufReader::with_capacity(1 << 20, file.take(manifest.shard_bytes));
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let anchor: SourceAnchor =
            serde_json::from_slice(&line).context("history_index_shard_entry_invalid")?;
        scan.anchors += 1;
        if !query.filter.admits(&anchor) {
            continue;
        }
        scan.admitted += 1;
        let Some((coverage, score)) = text_match(query.terms, &anchor.text) else {
            continue;
        };
        scan.matched += 1;
        if top.floor().is_some_and(|floor| (coverage, score) < floor) {
            continue;
        }
        top.offer(coverage, score, anchor);
    }
    Ok(())
}

fn parse_time(value: &Option<String>) -> Result<Option<Moment>> {
    value
        .as_deref()
        .map(|text| {
            chrono::DateTime::parse_from_rfc3339(text).context("history_search_time_invalid")
        })
        .transpose()
}

/// Read-only ranked search over committed shards. Texts that contain more of
/// the query's distinct terms rank first, then recall's density score; copies
/// of one role and text collapse into the earliest. Each hit carries a note:
/// its coverage, how many anchors share its text, and whether it is a
/// host-injected message rather than conversation.
pub fn search(home: &Path, request: &HistorySearchRequest) -> Result<Value> {
    shape(request)?;
    let text = request.query.trim();
    ensure!(
        !text.is_empty() && text.len() <= 4096,
        "history_search_query_invalid"
    );
    ensure!(
        (1..=50).contains(&request.limit),
        "history_search_limit_invalid"
    );
    ensure!(
        request
            .provider
            .as_deref()
            .is_none_or(|p| matches!(p, "codex" | "claude")),
        "history_search_provider_invalid"
    );
    let filter = Filter {
        provider: request.provider.clone(),
        profile: request.profile.clone(),
        session_id: request.session_id.clone(),
        since: parse_time(&request.since)?,
        until: parse_time(&request.until)?,
    };
    if let (Some(since), Some(until)) = (filter.since, filter.until) {
        ensure!(since <= until, "history_search_time_range_invalid");
    }
    let started = Instant::now();
    let terms = QueryTerms::new(text);
    let query = Query {
        terms: &terms,
        filter: &filter,
        limit: usize::try_from(request.limit)?,
    };
    let dir = index_dir(home);
    let files = manifests(&dir)?;
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 8)
        .min(files.len().max(1));
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let (files, query, dir) = (&files, &query, &dir);
                scope.spawn(move || -> Result<(TopK, Scan)> {
                    let mut top = TopK::new(query.limit);
                    let mut scan = Scan::default();
                    for manifest in files.iter().skip(worker).step_by(workers) {
                        scan_shard(dir, manifest, query, &mut top, &mut scan)?;
                    }
                    Ok((top, scan))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("history search worker panicked"))
            .collect::<Result<Vec<_>>>()
    })?;
    let mut total = Scan::default();
    let mut merged: HashMap<[u8; 32], Ranked> = HashMap::new();
    for (top, scan) in results {
        total.files += scan.files;
        total.anchors += scan.anchors;
        total.admitted += scan.admitted;
        total.matched += scan.matched;
        total.empty_files += scan.empty_files;
        total.unavailable_shards += scan.unavailable_shards;
        for (key, item) in top.members {
            match merged.entry(key) {
                Entry::Occupied(mut entry) => entry.get_mut().absorb(item.anchor, item.copies),
                Entry::Vacant(entry) => {
                    entry.insert(item);
                }
            }
        }
    }
    let mut ranked: Vec<Ranked> = merged.into_values().collect();
    ranked.sort_by_key(|item| Reverse(item.rank()));
    ranked.truncate(query.limit);
    // Equal coverage and score read in source order.
    ranked.sort_by(|a, b| {
        (b.coverage, b.score)
            .cmp(&(a.coverage, a.score))
            .then_with(|| {
                (a.anchor.source_path.as_str(), a.anchor.byte_start)
                    .cmp(&(b.anchor.source_path.as_str(), b.anchor.byte_start))
            })
    });
    let notes: Vec<Value> = ranked
        .iter()
        .map(|item| {
            let (injected, checked) = match crate::source_origin::message_origin(&item.anchor) {
                Ok(origin) => (origin.injected, true),
                Err(_) => (None, false),
            };
            json!({"coverage":item.coverage,"same_text_anchors":item.copies,
                "host_injected":injected,"origin_checked":checked})
        })
        .collect();
    let collapsed: u64 = ranked.iter().map(|item| item.copies - 1).sum();
    let hits: Vec<HistorySearchHit> = ranked
        .into_iter()
        .map(|item| HistorySearchHit {
            score: item.score,
            anchor: item.anchor,
        })
        .collect();
    Ok(json!({
        "schema":"engineering-memory.history-search/1","revision":REVISION,"query":text,
        "limit":request.limit,"filters":{"provider":request.provider,"profile":request.profile,
            "session_id":request.session_id,"since":request.since,"until":request.until},
        "hits":hits,"hit_notes":notes,"query_terms":terms.len(),
        "collapsed_copies":collapsed,"scanned_files":total.files,"scanned_anchors":total.anchors,
        "admitted_anchors":total.admitted,"matched_anchors":total.matched,
        "empty_files":total.empty_files,"unavailable_shards":total.unavailable_shards,
        "elapsed_ms":started.elapsed().as_millis() as u64,
        "retrieval":"coverage_then_density/1: distinct query terms matched, then unicode_field_density/2 over one anchor text field; identical role and text collapse into the earliest copy",
        "verification":"call read_window before citing","read_only":true,"provider_calls":0,
        "engineering_adoption":"not_granted"
    }))
}

#[cfg(test)]
mod classify_tests {
    use super::*;
    use crate::contracts::HistoryIndexRoot;

    #[test]
    fn max_seconds_stops_at_a_commit_and_the_next_build_resumes() {
        let temp = tempfile::TempDir::new().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let root = base.join("sessions");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("rollout-resume.jsonl");
        let line = |role: &str, text: &str| {
            json!({"timestamp":"2026-10-01T00:00:00Z","type":"response_item","payload":{
                "type":"message","role":role,"content":[{"type":"input_text","text":text}]}})
            .to_string()
                + "\n"
        };
        fs::write(
            &path,
            json!({"type":"session_meta","payload":{"id":"resume"}}).to_string()
                + "\n"
                + &line("user", "first")
                + &line("assistant", "second"),
        )
        .unwrap();
        let request = HistoryIndexBuildRequest {
            schema: "engineering-memory.history-index-build-request/1".into(),
            roots: vec![HistoryIndexRoot {
                path: root.to_str().unwrap().into(),
                provider: "codex".into(),
                profile: ".codex".into(),
            }],
            max_files: 10,
            max_seconds: 60,
            min_free_bytes: 0,
            verify: "changed".into(),
        };
        let home = base.join("memory");
        let stopped = build_until(&home, &request, Some(Instant::now())).unwrap();
        assert_eq!(stopped["stop_reason"], "max_seconds");
        assert_eq!(stopped["states"]["stopped"], 1);
        assert_eq!(stopped["pending_files"], 1);
        let key = sha(path.to_str().unwrap().as_bytes());
        let manifest: HistoryIndexFile =
            read_json(&manifest_path(&index_dir(&home), &key)).unwrap();
        assert!(!manifest.complete);
        let resumed = build(&home, &request).unwrap();
        assert_eq!(resumed["stop_reason"], "complete");
        assert_eq!(resumed["failed_total"], 0);
        assert_eq!(resumed["anchors_added"], 2);
        assert_eq!(status(&home).unwrap()["complete_files"], 1);
    }

    fn verdict(provider: &str, value: &str) -> LineClass {
        let mut de = serde_json::Deserializer::from_str(value);
        match Head::deserialize(&mut de).and_then(|h| de.end().map(|_| h)) {
            Ok(head) => head.verdict(provider),
            Err(_) => LineClass::InvalidJson,
        }
    }

    #[test]
    fn verdict_matches_reader_message_rules() {
        let codex_user = json!({"type":"response_item","payload":{"type":"message","role":"user",
            "content":[{"type":"input_text","text":"hi"}]}});
        assert_eq!(
            verdict("codex", &codex_user.to_string()),
            LineClass::Message
        );
        let codex_dev = json!({"type":"response_item","payload":{"type":"message","role":"developer",
            "content":[{"type":"input_text","text":"x"}]}});
        assert_eq!(
            verdict("codex", &codex_dev.to_string()),
            LineClass::NotMessage
        );
        let codex_event =
            json!({"type":"event_msg","payload":{"type":"item_completed","text":"x"}});
        assert_eq!(
            verdict("codex", &codex_event.to_string()),
            LineClass::NotMessage
        );
        let codex_image = json!({"type":"response_item","payload":{"type":"message","role":"user",
            "content":[{"type":"input_image","image_url":"data:"}]}});
        assert_eq!(
            verdict("codex", &codex_image.to_string()),
            LineClass::NotMessage
        );
        let codex_analysis = json!({"type":"response_item","payload":{"type":"message",
            "role":"assistant","channel":"analysis","content":[{"type":"output_text","text":"x"}]}});
        assert_eq!(
            verdict("codex", &codex_analysis.to_string()),
            LineClass::NotMessage
        );
        let claude_text = json!({"type":"user","message":{"role":"user","content":"long text"}});
        assert_eq!(
            verdict("claude", &claude_text.to_string()),
            LineClass::Message
        );
        let claude_tool = json!({"type":"user","message":{"role":"user",
            "content":[{"type":"tool_result","content":"huge"}]}});
        assert_eq!(
            verdict("claude", &claude_tool.to_string()),
            LineClass::NotMessage
        );
        let claude_nested = json!({"type":"assistant","message":{"role":"assistant",
            "content":[{"type":"text","text":{"nested":true}}]}});
        assert_eq!(
            verdict("claude", &claude_nested.to_string()),
            LineClass::NotMessage
        );
        assert_eq!(
            verdict("codex", r#"{"type":"response_item","type":"event_msg"}"#),
            LineClass::Ambiguous
        );
        assert_eq!(
            verdict(
                "codex",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","role":"user","content":"x"}}"#
            ),
            LineClass::Ambiguous
        );
        assert_eq!(
            verdict(
                "claude",
                r#"{"type":"user","message":{"role":"user","content":[{"type":"text","type":"text","text":"x"}]}}"#
            ),
            LineClass::Ambiguous
        );
        assert_eq!(verdict("codex", "{not json"), LineClass::InvalidJson);
        assert_eq!(verdict("codex", "[1,2]"), LineClass::NotMessage);
    }
}
