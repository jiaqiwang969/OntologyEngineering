//! Resumable, budgeted old-history and live-tail intake. Reading is not curation.
use crate::contracts::{
    ExperienceRecord, SourceAliasRequest, SourceAnchor, SourceRegistration, SyncBudget,
};
use crate::evidence::{EvidenceArgs, read_window};
use crate::generated::packet_transition;
use crate::snapshot::Capture;
use crate::store::{
    MAX_BYTES, Store, atomic_json, catalog, id, read_bytes, read_json, sha, shape, unique,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

const WINDOW_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Stamp {
    length: u64,
    modified_ns: u128,
}

fn stamp(path: &Path) -> Result<Stamp> {
    let metadata = path.metadata()?;
    ensure!(metadata.is_file(), "source_not_regular");
    Ok(Stamp {
        length: metadata.len(),
        modified_ns: metadata.modified()?.duration_since(UNIX_EPOCH)?.as_nanos(),
    })
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceState {
    schema: String,
    registration: SourceRegistration,
    baseline_end: u64,
    backfill_cursor: u64,
    live_cursor: u64,
    sealed_end: u64,
    sealed_sha256: String,
    registered_at: String,
    last_stamp: Stamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    capture: Option<Capture>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceAlias {
    schema: String,
    registration: SourceRegistration,
    source_id: String,
    source_registration: SourceRegistration,
    retired_state_sha256: Option<String>,
    verified_prefix_bytes: u64,
    linked_at: String,
}

fn same_scan_scope(a: &SourceRegistration, b: &SourceRegistration) -> bool {
    a.path == b.path
        && a.provider == b.provider
        && a.project_id == b.project_id
        && a.start_byte == b.start_byte
        && a.conditions.iter().collect::<BTreeSet<_>>()
            == b.conditions.iter().collect::<BTreeSet<_>>()
}

fn verify_alias_prefixes(
    owner: &SourceState,
    retired: Option<&SourceState>,
    budget: u64,
) -> Result<u64> {
    let path = Path::new(&owner.registration.path);
    ensure!(path.canonicalize()? == path, "source_alias_path_changed");
    let before = stamp(path)?;
    let mut used = 0_u64;
    let mut verify = || -> Result<()> {
        for state in std::iter::once(owner).chain(retired) {
            ensure!(before.length >= state.sealed_end, "source_truncated");
            verify_prefix(
                path,
                state.sealed_end,
                &state.sealed_sha256,
                budget.saturating_sub(used),
            )?;
            used = used
                .checked_add(state.sealed_end)
                .context("source_alias_byte_overflow")?;
        }
        Ok(())
    };
    verify()?;
    if stamp(path)? != before {
        ensure!(
            stamp(path)?.length >= before.length,
            "source_changed_during_alias"
        );
        verify()?;
        ensure!(
            stamp(path)?.length >= before.length,
            "source_changed_during_alias"
        );
    }
    Ok(used)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    pub schema: String,
    pub id: String,
    pub registration: SourceRegistration,
    pub lane: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub window_sha256: String,
    pub sources: Vec<SourceAnchor>,
    pub gaps: Vec<Value>,
    pub input_sha256: String,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<Capture>,
}

impl Packet {
    pub fn input_digest(&self) -> Result<String> {
        let mut input = json!([
            self.schema,
            self.id,
            self.registration,
            self.lane,
            self.byte_start,
            self.byte_end,
            self.window_sha256,
            self.sources,
            self.gaps
        ]);
        if let Some(capture) = &self.capture {
            input
                .as_array_mut()
                .unwrap()
                .push(serde_json::to_value(capture)?);
        }
        Ok(sha(serde_json::to_vec(&input)?))
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            ((self.schema == "engineering-memory.packet/1" && self.capture.is_none())
                || (self.schema == "engineering-memory.packet/2" && self.capture.is_some()))
                && self.input_sha256 == self.input_digest()?,
            "packet_input_mismatch"
        );
        if let Some(capture) = &self.capture {
            capture.validate()?;
            let mut expected = capture.request.registration.clone();
            expected.path = capture.snapshot_path.clone();
            ensure!(
                self.registration == expected
                    && self.lane == "backfill"
                    && self.byte_start >= expected.start_byte
                    && self.byte_end <= capture.complete_end,
                "packet_capture_mismatch"
            );
        }
        ensure!(
            ["pending", "committed", "no_delta", "rejected"].contains(&self.state.as_str()),
            "packet_state_invalid"
        );
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct MessageVariant {
    sha256: String,
    anchors: Vec<SourceAnchor>,
    packet_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct MessageIdentity {
    identity: String,
    variants: Vec<MessageVariant>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IgnoredSource {
    pub sha256: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub schema: String,
    pub packet_id: String,
    pub input_sha256: String,
    pub action: String,
    pub actor: String,
    pub reason: String,
    pub records: Vec<ExperienceRecord>,
    pub ignored: Vec<IgnoredSource>,
}

/// Hash a bounded prefix with constant memory. Never skip integrity on budget exhaustion.
pub fn verify_prefix(path: &Path, end: u64, expected: &str, budget: u64) -> Result<Sha256> {
    ensure!(end <= budget, "prefix_verification_budget");
    let hasher = hash_prefix(path, end)?;
    ensure!(
        hex::encode(hasher.clone().finalize()) == expected,
        "source_prefix_changed"
    );
    Ok(hasher)
}

fn hash_prefix(path: &Path, end: u64) -> Result<Sha256> {
    let mut hasher = Sha256::new();
    extend_hash(path, 0, end, &mut hasher)?;
    Ok(hasher)
}

fn extend_hash(path: &Path, start: u64, end: u64, hasher: &mut Sha256) -> Result<()> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut remaining = end - start;
    let mut buffer = [0u8; 65536];
    while remaining > 0 {
        let size = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..size])
            .context("source_truncated")?;
        hasher.update(&buffer[..size]);
        remaining -= size as u64;
    }
    Ok(())
}

pub(crate) fn boundary(path: &Path, end: u64) -> Result<u64> {
    if end == 0 {
        return Ok(0);
    }
    let mut file = File::open(path)?;
    let lower = end.saturating_sub(MAX_BYTES);
    let mut cursor = end;
    let mut bytes = [0u8; 65536];
    while cursor > lower {
        let start = cursor.saturating_sub(bytes.len() as u64).max(lower);
        let count = (cursor - start) as usize;
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut bytes[..count])?;
        if let Some(index) = bytes[..count].iter().rposition(|c| *c == b'\n') {
            return Ok(start + index as u64 + 1);
        }
        cursor = start;
    }
    ensure!(lower == 0, "tail_record_exceeds_window_budget");
    Ok(0)
}

pub(crate) fn check_start(path: &Path, start: u64, end: u64) -> Result<()> {
    ensure!(start <= end, "registration_range_invalid");
    if start > 0 {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(start - 1))?;
        let mut previous = [0];
        file.read_exact(&mut previous)?;
        ensure!(previous[0] == b'\n', "start_not_record_boundary");
    }
    Ok(())
}

/// Find a complete window without repeatedly parsing the same image payload.
/// Probe reads have an independent aggregate allowance equal to max_bytes;
/// selected scan bytes and prefix verification remain separately accounted.
fn scan_end(
    path: &Path,
    start: u64,
    available: u64,
    allowance: u64,
    probe_bytes: &mut u64,
) -> Result<(u64, &'static str)> {
    let limit = (available - start).min(MAX_BYTES).min(allowance);
    let ordinary = limit.min(WINDOW_BYTES);
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut buffer = [0u8; 65536];
    let mut offset = 0;
    let mut complete = None;
    while offset < limit {
        let target = if offset < ordinary { ordinary } else { limit };
        let count = (target - offset).min(buffer.len() as u64) as usize;
        // Failed or shortened reads also consume probe fuel.
        *probe_bytes += count as u64;
        file.read_exact(&mut buffer[..count])
            .context("source_truncated")?;
        if offset < ordinary {
            if let Some(index) = buffer[..count].iter().rposition(|b| *b == b'\n') {
                complete = Some(start + offset + index as u64 + 1);
            }
        } else if let Some(index) = buffer[..count].iter().position(|b| *b == b'\n') {
            // Only the leading oversized record needs the larger window.
            return Ok((start + offset + index as u64 + 1, "complete"));
        }
        offset += count as u64;
        if offset == ordinary
            && let Some(end) = complete
        {
            return Ok((end, "complete"));
        }
    }
    Ok((
        start,
        if limit == MAX_BYTES {
            "next_record_exceeds_window_limit"
        } else {
            "window_budget_below_next_record"
        },
    ))
}

fn byte_digest(path: &Path, start: u64, end: u64) -> Result<String> {
    ensure!(
        end >= start && end - start <= MAX_BYTES,
        "packet_window_budget"
    );
    let mut hasher = Sha256::new();
    extend_hash(path, start, end, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

impl Store {
    /// Called under the shared writer lock after the immutable capture receipt.
    pub(crate) fn register_capture(&self, capture: &Capture) -> Result<bool> {
        ensure!(
            !self
                .source_aliases()?
                .contains_key(&capture.request.registration.id),
            "source_registration_conflict"
        );
        capture.verify()?;
        let mut registration = capture.request.registration.clone();
        registration.path = capture.snapshot_path.clone();
        let file = self
            .directory
            .join("sources")
            .join(format!("{}.json", id(&registration.id)?));
        if file.exists() {
            let old: SourceState = read_json(&file)?;
            ensure!(
                old.schema == "engineering-memory.source/2"
                    && old.registration == registration
                    && old.capture.as_ref() == Some(capture),
                "source_registration_conflict"
            );
            return Ok(false);
        }
        let state = SourceState {
            schema: "engineering-memory.source/2".into(),
            baseline_end: capture.complete_end,
            backfill_cursor: registration.start_byte,
            live_cursor: capture.complete_end,
            sealed_end: 0,
            sealed_sha256: String::new(),
            registration,
            registered_at: capture.captured_at.clone(),
            last_stamp: stamp(Path::new(&capture.snapshot_path))?,
            capture: Some(capture.clone()),
        };
        atomic_json(&file, &state)?;
        Ok(true)
    }

    fn source_file(&self, name: &str) -> Result<PathBuf> {
        Ok(self
            .directory
            .join("sources")
            .join(format!("{}.json", id(name)?)))
    }

    fn source_aliases(&self) -> Result<BTreeMap<String, SourceAlias>> {
        let mut aliases = BTreeMap::new();
        for file in catalog(&self.directory.join("source-aliases"), 100_000)? {
            let alias: SourceAlias = read_json(&file)?;
            shape(&alias.registration)?;
            shape(&alias.source_registration)?;
            id(&alias.source_id)?;
            let name = id(&alias.registration.id)?;
            ensure!(
                alias.schema == "engineering-memory.source-alias/1"
                    && file.file_stem().is_some_and(|stem| stem == name)
                    && name != alias.source_id
                    && alias.source_registration.id == alias.source_id
                    && same_scan_scope(&alias.registration, &alias.source_registration),
                "source_alias_invalid"
            );
            let owner: SourceState = read_json(&self.source_file(&alias.source_id)?)?;
            ensure!(
                owner.schema == "engineering-memory.source/1"
                    && owner.capture.is_none()
                    && owner.registration == alias.source_registration,
                "source_alias_owner_changed"
            );
            let old = self.source_file(name)?;
            if let Some(expected) = &alias.retired_state_sha256 {
                let raw = read_bytes(&old)?;
                ensure!(sha(&raw) == *expected, "source_alias_retired_state_changed");
                let state: SourceState = serde_json::from_slice(&raw)?;
                ensure!(
                    state.registration == alias.registration,
                    "source_alias_retired_registration_changed"
                );
            } else {
                ensure!(!old.exists(), "source_alias_conflicting_state");
            }
            aliases.insert(name.to_owned(), alias);
        }
        ensure!(
            aliases
                .values()
                .all(|a| !aliases.contains_key(&a.source_id)),
            "source_alias_chain"
        );
        Ok(aliases)
    }

    fn link_source_locked(
        &self,
        registration: &SourceRegistration,
        source_id: &str,
        budget: u64,
    ) -> Result<Value> {
        ensure!(registration.id != source_id, "source_alias_self");
        let aliases = self.source_aliases()?;
        ensure!(!aliases.contains_key(source_id), "source_alias_chain");
        ensure!(
            !aliases.values().any(|a| a.source_id == registration.id),
            "source_alias_has_dependents"
        );
        let owner: SourceState = read_json(&self.source_file(source_id)?)?;
        ensure!(
            owner.schema == "engineering-memory.source/1"
                && owner.capture.is_none()
                && owner.registration.id == source_id
                && same_scan_scope(&owner.registration, registration),
            "source_alias_scope_mismatch"
        );
        let retired_path = self.source_file(&registration.id)?;
        let retired_bytes = if retired_path.exists() {
            Some(read_bytes(&retired_path)?)
        } else {
            None
        };
        let retired: Option<SourceState> = retired_bytes
            .as_deref()
            .map(serde_json::from_slice)
            .transpose()?;
        if let Some(state) = &retired {
            ensure!(
                state.schema == "engineering-memory.source/1"
                    && state.capture.is_none()
                    && state.registration == *registration
                    && state.backfill_cursor == registration.start_byte
                    && state.live_cursor == state.baseline_end
                    && state.sealed_end == state.baseline_end,
                "source_alias_scan_already_started"
            );
            for file in catalog(&self.directory.join("packets"), 100_000)? {
                let packet: Packet = read_json(&file)?;
                ensure!(
                    packet.registration.id != registration.id,
                    "source_alias_issued_packet"
                );
            }
            for file in catalog(&self.directory.join("scans"), 100_000)? {
                let scan: Value = read_json(&file)?;
                ensure!(
                    scan["registration_id"] != registration.id,
                    "source_alias_issued_scan"
                );
            }
        }
        let verified = verify_alias_prefixes(&owner, retired.as_ref(), budget)?;
        if let Some(alias) = aliases.get(&registration.id) {
            ensure!(
                alias.registration == *registration && alias.source_id == source_id,
                "source_alias_conflict"
            );
            return Ok(
                json!({"alias":alias,"effect":false,"verified_prefix_bytes":verified,"curated":false}),
            );
        }
        let alias = SourceAlias {
            schema: "engineering-memory.source-alias/1".into(),
            registration: registration.clone(),
            source_id: source_id.into(),
            source_registration: owner.registration,
            retired_state_sha256: retired_bytes.as_ref().map(sha),
            verified_prefix_bytes: verified,
            linked_at: chrono::Utc::now().to_rfc3339(),
        };
        self.immutable("source-aliases", &registration.id, &alias)?;
        Ok(json!({"alias":alias,"effect":true,"verified_prefix_bytes":verified,"curated":false}))
    }

    pub fn alias_source(&self, request: &SourceAliasRequest) -> Result<Value> {
        shape(request)?;
        let _lock = self.lock()?;
        let aliases = self.source_aliases()?;
        let registration = if let Some(alias) = aliases.get(&request.alias_id) {
            alias.registration.clone()
        } else {
            read_json::<SourceState>(&self.source_file(&request.alias_id)?)?.registration
        };
        ensure!(
            registration.id == request.alias_id,
            "source_alias_registration_identity"
        );
        self.link_source_locked(&registration, &request.source_id, request.max_verify_bytes)
    }

    pub fn register(
        &self,
        mut registration: SourceRegistration,
        max_verify_bytes: u64,
    ) -> Result<Value> {
        shape(&registration)?;
        id(&registration.id)?;
        unique(&registration.conditions, "condition")?;
        let path = PathBuf::from(&registration.path).canonicalize()?;
        registration.path = path.to_string_lossy().into_owned();
        let _lock = self.lock()?;
        let aliases = self.source_aliases()?;
        if let Some(alias) = aliases.get(&registration.id) {
            ensure!(
                alias.registration == registration,
                "source_registration_conflict"
            );
            return Ok(
                json!({"registration":registration,"alias":alias,"effect":false,
                "source_reverified":false,"curated":false}),
            );
        }
        let file = self
            .directory
            .join("sources")
            .join(format!("{}.json", registration.id));
        if file.exists() {
            let old: SourceState = read_json(&file)?;
            ensure!(
                old.schema == "engineering-memory.source/1"
                    && old.capture.is_none()
                    && old.registration == registration,
                "source_registration_conflict"
            );
            return Ok(json!({"registration":registration,"effect":false}));
        }
        let mut owners = Vec::new();
        for existing in catalog(&self.directory.join("sources"), 100_000)? {
            let state: SourceState = read_json(&existing)?;
            if state.schema == "engineering-memory.source/1"
                && state.capture.is_none()
                && !aliases.contains_key(&state.registration.id)
                && same_scan_scope(&state.registration, &registration)
            {
                owners.push(state);
            }
        }
        owners.sort_by(|a, b| {
            (&a.registered_at, &a.registration.id).cmp(&(&b.registered_at, &b.registration.id))
        });
        if let Some(owner) = owners.first() {
            return self.link_source_locked(
                &registration,
                &owner.registration.id,
                max_verify_bytes,
            );
        }
        let before = stamp(&path)?;
        let end = boundary(&path, before.length)?;
        ensure!(
            registration.start_byte <= end && end <= max_verify_bytes,
            "registration_range_or_verification_budget"
        );
        check_start(&path, registration.start_byte, end)?;
        let digest = hex::encode(hash_prefix(&path, end)?.finalize());
        let mut verified_prefix_bytes = end;
        if stamp(&path)? != before {
            ensure!(
                stamp(&path)?.length >= before.length,
                "source_changed_during_registration"
            );
            // The current conversation can append while its old prefix is
            // hashed. Confirm that exact sealed prefix again, charging both
            // passes; a rewritten/truncated prefix is still refused.
            verify_prefix(&path, end, &digest, max_verify_bytes.saturating_sub(end))?;
            verified_prefix_bytes += end;
            ensure!(
                stamp(&path)?.length >= before.length,
                "source_changed_during_registration"
            );
        }
        let state = SourceState {
            schema: "engineering-memory.source/1".into(),
            baseline_end: end,
            backfill_cursor: registration.start_byte,
            live_cursor: end,
            sealed_end: end,
            sealed_sha256: digest,
            registration: registration.clone(),
            registered_at: chrono::Utc::now().to_rfc3339(),
            last_stamp: before,
            capture: None,
        };
        atomic_json(&file, &state)?;
        Ok(
            json!({"registration":registration,"effect":true,"baseline_end":end,
            "verified_prefix_bytes":verified_prefix_bytes,
            "skipped_prefix_bytes":registration.start_byte,"curated":false}),
        )
    }

    fn packet_file(&self, packet_id: &str) -> Result<PathBuf> {
        Ok(self
            .directory
            .join("packets")
            .join(format!("{}.json", id(packet_id)?)))
    }

    pub fn packet(&self, packet_id: &str) -> Result<Packet> {
        let packet: Packet = read_json(&self.packet_file(packet_id)?)?;
        packet.validate()?;
        ensure!(packet.id == packet_id, "packet_identity_mismatch");
        Ok(packet)
    }

    fn message_file(&self, source: &SourceAnchor) -> Result<(PathBuf, String)> {
        let identity = sha(serde_json::to_vec(&json!([
            source.provider,
            source.session_id,
            source.message_id.as_ref().unwrap_or(&source.sha256)
        ]))?);
        Ok((
            self.directory
                .join("messages")
                .join(&identity[..2])
                .join(format!("{identity}.json")),
            identity,
        ))
    }

    /// Commit the packet before indices/cursor. Replaying after any crash recovers the same packet.
    fn intake_window(
        &self,
        state: &SourceState,
        lane: &str,
        start: u64,
        requested_end: u64,
    ) -> Result<(u64, Value)> {
        if let Some(capture) = &state.capture {
            capture.verify()?;
        }
        let window = read_window(&EvidenceArgs {
            source: state.registration.path.clone().into(),
            provider: state.registration.provider.clone(),
            profile: state.registration.profile.clone(),
            start_byte: start,
            end_byte: Some(requested_end),
            max_record_bytes: MAX_BYTES,
            json: true,
        })?;
        let mut end = window
            .next_byte
            .filter(|n| *n < requested_end)
            .unwrap_or(requested_end);
        // Source review accepts at most 128 anchors. Keep the remainder at
        // its exact boundary for the next packet instead of publishing a
        // packet which no reviewer can resolve.
        if let Some(next) = window.records.get(128) {
            end = end.min(next.byte_start);
        }
        ensure!(end > start, "intake_no_progress");
        let digest = byte_digest(Path::new(&state.registration.path), start, end)?;
        let packet_id = sha(serde_json::to_vec(&json!([
            state.registration.id,
            lane,
            start,
            end,
            digest
        ]))?);
        let file = self.packet_file(&packet_id)?;
        let mut fresh: Vec<SourceAnchor> = Vec::new();
        let mut aliases = 0;
        let mut forks = 0;
        let mut messages = std::collections::BTreeMap::<PathBuf, MessageIdentity>::new();
        for source in &window.records {
            if source.byte_end > end {
                continue;
            }
            let (file, identity) = self.message_file(source)?;
            if !messages.contains_key(&file) {
                let message = if file.exists() {
                    read_json(&file)?
                } else {
                    MessageIdentity {
                        identity,
                        variants: vec![],
                    }
                };
                messages.insert(file.clone(), message);
            }
            let message = messages.get_mut(&file).expect("message inserted");
            if let Some(existing) = message
                .variants
                .iter_mut()
                .find(|v| v.sha256 == source.sha256)
            {
                if !existing.anchors.contains(source) {
                    ensure!(existing.anchors.len() < 128, "message_alias_budget");
                    existing.anchors.push(source.clone());
                }
                if existing.packet_id == packet_id {
                    if !fresh.iter().any(|s| s.sha256 == source.sha256) {
                        fresh.push(source.clone());
                    }
                } else {
                    aliases += 1;
                }
            } else {
                if !message.variants.is_empty() {
                    forks += 1;
                }
                ensure!(message.variants.len() < 128, "message_fork_budget");
                message.variants.push(MessageVariant {
                    sha256: source.sha256.clone(),
                    anchors: vec![source.clone()],
                    packet_id: packet_id.clone(),
                });
                fresh.push(source.clone());
            }
        }
        let gaps: Vec<_> = window
            .gaps
            .into_iter()
            .filter(|g| g.byte_start < end && g.reason != "result_count_budget")
            .map(|g| serde_json::to_value(g).expect("gap serialization"))
            .collect();
        let has_packet = !fresh.is_empty() || !gaps.is_empty();
        if has_packet {
            if file.exists() {
                let old = self.packet(&packet_id)?;
                ensure!(
                    old.window_sha256 == digest && old.sources == fresh,
                    "packet_replay_conflict"
                );
            } else {
                if let Some(capture) = &state.capture {
                    capture.verify()?;
                }
                let mut packet = Packet {
                    schema: if state.capture.is_some() {
                        "engineering-memory.packet/2"
                    } else {
                        "engineering-memory.packet/1"
                    }
                    .into(),
                    id: packet_id.clone(),
                    registration: state.registration.clone(),
                    lane: lane.into(),
                    byte_start: start,
                    byte_end: end,
                    window_sha256: digest.clone(),
                    sources: fresh.clone(),
                    gaps,
                    input_sha256: String::new(),
                    state: "pending".into(),
                    capture: state.capture.clone(),
                };
                packet.input_sha256 = packet.input_digest()?;
                atomic_json(&file, &packet)?;
            }
        }
        for (file, message) in messages {
            atomic_json(&file, &message)?;
        }
        let receipt = json!({"schema":"engineering-memory.scan/1","id":packet_id,"registration_id":state.registration.id,
            "lane":lane,"byte_start":start,"byte_end":end,"window_sha256":digest,"new_messages":fresh.len(),
            "aliases":aliases,"forks":forks,"excluded_records":window.excluded_records,"packet_created":has_packet,
            "capture":state.capture,"curated":false});
        // A replay may change alias counters, so the first durable receipt is authoritative.
        let receipt_file = self
            .directory
            .join("scans")
            .join(format!("{packet_id}.json"));
        if !receipt_file.exists() {
            atomic_json(&receipt_file, &receipt)?;
        }
        Ok((end, receipt))
    }

    pub fn sync(&self, budget: &SyncBudget) -> Result<Value> {
        self.sync_with_pending_limit(budget, None)
    }

    pub(crate) fn sync_with_pending_limit(
        &self,
        budget: &SyncBudget,
        pending_limit: Option<usize>,
    ) -> Result<Value> {
        shape(budget)?;
        ensure!(
            budget.max_bytes > 0 && budget.max_windows > 0 && budget.max_seconds > 0,
            "sync_budget_invalid"
        );
        let _lock = self.lock()?;
        let started = Instant::now();
        // Count and publish while holding the same writer lock: concurrent
        // ticks cannot both reserve the last queue slot. Foreground sync keeps
        // its own explicit budget and reports its own additions separately.
        let pending_count = if let Some(limit) = pending_limit {
            ensure!((1..=100).contains(&limit), "pending_limit_invalid");
            self.pending(limit)?["packets"]
                .as_array()
                .context("pending_output_invalid")?
                .len()
        } else {
            0
        };
        let window_limit = pending_limit.map_or(budget.max_windows, |limit| {
            budget
                .max_windows
                .min(limit.saturating_sub(pending_count) as u64)
        });
        let mut files = catalog(&self.directory.join("sources"), 100_000)?;
        let aliases = self.source_aliases()?;
        files.retain(|file| {
            !file
                .file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| aliases.contains_key(s))
        });
        let scheduler_path = self.directory.join("scheduler.json");
        let last: Option<String> = if scheduler_path.exists() {
            read_json::<Value>(&scheduler_path)?["last_source"]
                .as_str()
                .map(str::to_owned)
        } else {
            None
        };
        if let Some(last) = last {
            let pivot = files
                .iter()
                .position(|p| p.file_stem().is_some_and(|s| s > last.as_str()))
                .unwrap_or(0);
            files.rotate_left(pivot);
        }
        let mut read_bytes = 0;
        let mut window_probe_bytes = 0;
        let mut verified_bytes = 0;
        let mut windows = Vec::new();
        let mut problems = Vec::new();
        let mut waiting = Vec::new();
        for file in files {
            if read_bytes >= budget.max_bytes
                || window_probe_bytes >= budget.max_bytes
                || windows.len() as u64 >= window_limit
                || started.elapsed().as_secs() >= budget.max_seconds
            {
                break;
            }
            let mut state: SourceState = read_json(&file)?;
            let before_state = state.clone();
            let outcome = (|| -> Result<()> {
                ensure!(
                    (state.schema == "engineering-memory.source/1" && state.capture.is_none())
                        || (state.schema == "engineering-memory.source/2"
                            && state.capture.is_some()),
                    "source_state_invalid"
                );
                if let Some(capture) = &state.capture {
                    capture.verify()?;
                    let mut expected = capture.request.registration.clone();
                    expected.path = capture.snapshot_path.clone();
                    ensure!(
                        state.registration == expected
                            && state.baseline_end == capture.complete_end
                            && state.backfill_cursor >= expected.start_byte
                            && state.backfill_cursor <= capture.complete_end
                            && state.live_cursor == capture.complete_end
                            && state.sealed_end == 0
                            && state.sealed_sha256.is_empty(),
                        "snapshot_state_invalid"
                    );
                }
                let path = PathBuf::from(&state.registration.path);
                let before = stamp(&path)?;
                ensure!(before.length >= state.sealed_end, "source_truncated");
                let live_end = if let Some(capture) = &state.capture {
                    capture.complete_end
                } else {
                    boundary(&path, before.length)?
                };
                let changed = before != state.last_stamp;
                let work =
                    state.backfill_cursor < state.baseline_end || state.live_cursor < live_end;
                if !work && !changed {
                    return Ok(());
                }
                let mut hasher = if state.capture.is_none() {
                    ensure!(
                        state.sealed_end <= budget.max_verify_bytes.saturating_sub(verified_bytes),
                        "prefix_verification_budget"
                    );
                    verified_bytes += state.sealed_end;
                    Some(verify_prefix(
                        &path,
                        state.sealed_end,
                        &state.sealed_sha256,
                        state.sealed_end,
                    )?)
                } else {
                    None
                };
                // Live tail first, then one historical window. Old backlog cannot delay new discussion indefinitely.
                for lane in ["live", "backfill"] {
                    if read_bytes >= budget.max_bytes
                        || window_probe_bytes >= budget.max_bytes
                        || windows.len() as u64 >= window_limit
                        || started.elapsed().as_secs() >= budget.max_seconds
                    {
                        break;
                    }
                    let (start, available) = if lane == "live" {
                        (state.live_cursor, live_end)
                    } else {
                        (state.backfill_cursor, state.baseline_end)
                    };
                    if start >= available {
                        continue;
                    }
                    let allowance =
                        (budget.max_bytes - read_bytes).min(budget.max_bytes - window_probe_bytes);
                    let (end, reason) =
                        scan_end(&path, start, available, allowance, &mut window_probe_bytes)?;
                    if end <= start {
                        waiting.push(json!({"source":state.registration.id,"lane":lane,"reason":reason,
                            "byte_start":start,"max_record_bytes":MAX_BYTES,"window_allowance":allowance}));
                        continue;
                    }
                    // Check before any publication; read_window also checks mutation during its read.
                    ensure!(
                        stamp(&path)?.length >= before.length,
                        "source_changed_during_sync"
                    );
                    // Even a failed parse or a shortened result consumed this
                    // input window. Retries cannot silently exceed read fuel.
                    read_bytes += end - start;
                    let (end, receipt) = self.intake_window(&state, lane, start, end)?;
                    if let Some(hasher) = &mut hasher {
                        if end > state.sealed_end {
                            extend_hash(&path, state.sealed_end, end, hasher)?;
                            state.sealed_end = end;
                            state.sealed_sha256 = hex::encode(hasher.clone().finalize());
                        }
                        if stamp(&path)? != before {
                            ensure!(
                                state.sealed_end
                                    <= budget.max_verify_bytes.saturating_sub(verified_bytes),
                                "prefix_verification_budget"
                            );
                            verified_bytes += state.sealed_end;
                            verify_prefix(
                                &path,
                                state.sealed_end,
                                &state.sealed_sha256,
                                state.sealed_end,
                            )?;
                        }
                    } else if let Some(capture) = &state.capture {
                        capture.verify()?;
                    }
                    ensure!(
                        stamp(&path)?.length >= before.length,
                        "source_changed_during_sync"
                    );
                    if lane == "live" {
                        state.live_cursor = end;
                    } else {
                        state.backfill_cursor = end;
                    }
                    windows.push(receipt);
                    state.last_stamp = before.clone();
                    atomic_json(&file, &state)?;
                }
                if live_end < before.length {
                    waiting.push(json!({"source":state.registration.id,
                        "reason":if state.capture.is_some() { "captured_partial_tail_requires_new_version" } else { "partial_tail_waiting" },
                        "byte_start":live_end,"byte_end":before.length}));
                }
                Ok(())
            })();
            if let Err(error) = outcome {
                problems.push(
                    json!({"source":before_state.registration.id,"reason":error.to_string()}),
                );
                if error.to_string() == "prefix_verification_budget" && verified_bytes > 0 {
                    // Resume at this source next time; earlier sources must not exhaust its budget forever.
                    break;
                }
            }
            atomic_json(
                &scheduler_path,
                &json!({"last_source":before_state.registration.id}),
            )?;
        }
        let result = json!({"schema":"engineering-memory.sync/1","read_bytes":read_bytes,"verified_prefix_bytes":verified_bytes,
            "window_probe_bytes":window_probe_bytes,"window_probe_budget":budget.max_bytes,
            "windows":windows,"problems":problems,"waiting":waiting,"budget":budget,
            "pending_limit":pending_limit,"pending_count_at_least":pending_count,"effective_max_windows":window_limit,
            "backpressure":pending_limit.is_some() && window_limit == 0,
            "elapsed_ms":started.elapsed().as_millis(),"curated_by_scan":0,"whole_history_reviewed":false});
        let run_id = sha(format!(
            "{}:{}",
            std::process::id(),
            chrono::Utc::now().to_rfc3339()
        ));
        self.immutable("sync-runs", &run_id, &result)?;
        atomic_json(&self.directory.join("last-sync.json"), &result)?;
        Ok(result)
    }

    pub fn pending(&self, limit: usize) -> Result<Value> {
        ensure!((1..=100).contains(&limit), "pending_limit_invalid");
        let mut pending = Vec::new();
        for file in catalog(&self.directory.join("packets"), 100_000)? {
            let packet: Packet = read_json(&file)?;
            packet.validate()?;
            if packet.state == "pending" {
                pending.push(json!({"id":packet.id,"source":packet.registration.id,"lane":packet.lane,
                    "messages":packet.sources.len(),"gaps":packet.gaps.len(),"input_sha256":packet.input_sha256}));
                if pending.len() == limit {
                    break;
                }
            }
        }
        Ok(
            json!({"packets":pending,"limit":limit,"scope":"bounded pending page; inspect packet before curating"}),
        )
    }

    pub fn status(&self) -> Result<Value> {
        let mut sources = Vec::new();
        let aliases = self.source_aliases()?;
        let mut active_scan_sources = 0;
        for file in catalog(&self.directory.join("sources"), 100_000)? {
            let state: SourceState = read_json(&file)?;
            let alias = aliases.get(&state.registration.id);
            if alias.is_none() {
                active_scan_sources += 1;
            }
            sources.push(json!({"id":state.registration.id,"profile":state.registration.profile,"project_id":state.registration.project_id,
                "scan_owner":alias.map(|a| &a.source_id).unwrap_or(&state.registration.id),"scheduled":alias.is_none(),
                "backfill_cursor":state.backfill_cursor,"baseline_end":state.baseline_end,"live_cursor":state.live_cursor,
                "skipped_prefix_bytes":state.registration.start_byte,"historical_bytes_remaining":state.baseline_end-state.backfill_cursor,
                "capture":state.capture,"source_schema":state.schema}));
        }
        let mut states = std::collections::BTreeMap::<String, usize>::new();
        let mut gap_count = 0;
        for file in catalog(&self.directory.join("packets"), 100_000)? {
            let packet: Packet = read_json(&file)?;
            packet.validate()?;
            *states.entry(packet.state).or_default() += 1;
            gap_count += packet.gaps.len();
        }
        let last_sync: Value = if self.directory.join("last-sync.json").exists() {
            read_json(&self.directory.join("last-sync.json"))?
        } else {
            Value::Null
        };
        Ok(
            json!({"schema":"engineering-memory.status/1","sources":sources,"source_aliases":aliases.values().collect::<Vec<_>>(),
            "active_scan_sources":active_scan_sources,"packet_states":states,"retained_gaps":gap_count,
            "experiences":catalog(&self.directory.join("records"),crate::store::MAX_RECORDS)?.len(),
            "feedback":catalog(&self.directory.join("feedback"),100_000)?.len(),"last_sync":last_sync,
            "whole_history_reviewed":false,"rrsi_gain":"not_measured"}),
        )
    }

    pub fn resolve(&self, resolution: &Resolution) -> Result<Value> {
        ensure!(
            resolution.schema == "engineering-memory.resolution/1"
                && !resolution.actor.trim().is_empty()
                && !resolution.reason.trim().is_empty(),
            "resolution_annotations_required"
        );
        let _lock = self.lock()?;
        let mut packet = self.packet(&resolution.packet_id)?;
        ensure!(
            resolution.input_sha256 == packet.input_sha256,
            "resolution_input_mismatch"
        );
        let intent_file = self
            .directory
            .join("resolutions")
            .join(format!("{}.json", packet.id));
        if intent_file.exists() {
            ensure!(
                read_json::<Resolution>(&intent_file)? == *resolution,
                "resolution_conflict"
            );
        }
        if packet.state != "pending" {
            ensure!(intent_file.exists(), "packet_resolution_missing");
            return Ok(json!({"packet_id":packet.id,"state":packet.state,"effect":false}));
        }
        let (next, guard) = packet_transition(&packet.state, &resolution.action)
            .context("packet_transition_invalid")?;
        ensure!(resolution.records.len() <= 32, "resolution_record_budget");
        unique(
            &resolution
                .records
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>(),
            "resolution_record",
        )?;
        unique(
            &resolution
                .ignored
                .iter()
                .map(|r| r.sha256.clone())
                .collect::<Vec<_>>(),
            "ignored_source",
        )?;
        ensure!(
            resolution
                .ignored
                .iter()
                .all(|i| !i.reason.trim().is_empty()),
            "ignored_reason_required"
        );
        if guard != "explicit_reason" {
            if let Some(capture) = &packet.capture {
                capture.verify()?;
            }
            ensure!(packet.gaps.is_empty(), "packet_has_unresolved_gaps");
            ensure!(
                byte_digest(
                    Path::new(&packet.registration.path),
                    packet.byte_start,
                    packet.byte_end
                )? == packet.window_sha256,
                "packet_source_changed"
            );
            self.verify_record_sources(&packet.sources)?;
        }
        ensure!(
            (resolution.action == "commit") != resolution.records.is_empty(),
            "resolution_action_records_conflict"
        );
        let mut used = BTreeSources::new();
        for record in &resolution.records {
            self.preflight(record)?;
            let previous = if record.revision > 1 {
                Some(self.get(&record.id, Some(record.revision - 1))?)
            } else {
                None
            };
            for source in &record.sources {
                if packet.sources.contains(source) {
                    used.insert(source.sha256.clone());
                } else {
                    ensure!(
                        previous
                            .as_ref()
                            .is_some_and(|r| r.sources.contains(source)),
                        "resolution_source_outside_input"
                    );
                }
            }
            ensure!(
                record.sources.iter().any(|s| packet.sources.contains(s)),
                "resolution_without_packet_evidence"
            );
        }
        for ignored in &resolution.ignored {
            ensure!(
                packet.sources.iter().any(|s| s.sha256 == ignored.sha256)
                    && !used.contains(&ignored.sha256),
                "ignored_source_invalid_or_used"
            );
            used.insert(ignored.sha256.clone());
        }
        if resolution.action != "reject" {
            ensure!(
                packet.sources.iter().all(|s| used.contains(&s.sha256)),
                "packet_source_unaccounted"
            );
        }
        if !intent_file.exists() {
            atomic_json(&intent_file, resolution)?;
        }
        // Durable intent allows exact replay after interruption between record writes and state commit.
        for record in &resolution.records {
            self.record_locked(record)?;
        }
        packet.state = next.into();
        atomic_json(&self.packet_file(&packet.id)?, &packet)?;
        Ok(
            json!({"packet_id":packet.id,"state":packet.state,"effect":true,"guard":guard,
            "records":resolution.records.iter().map(|r| json!({"id":r.id,"revision":r.revision})).collect::<Vec<_>>(),
            "engineering_adoption":"not_granted"}),
        )
    }
}

type BTreeSources = std::collections::BTreeSet<String>;
