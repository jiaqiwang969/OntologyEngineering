//! The sole owner of engineering experience. CM and the skill call this code.
use crate::contracts::{
    ExperienceRecord, MethodComparison, SourceAnchor, UsageFeedback, ValidateShape,
};
use crate::evidence::{EvidenceArgs, read_window};
use crate::generated::SCOPE_RULE;
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Listing bound for records/. The former 10,000 made recall fail outright
/// once automatic curation filled the store.
pub const MAX_RECORDS: usize = 1_000_000;
/// Machine process drafts that recall scores, newest first. Older drafts stay
/// searchable through history-search, whose catalog holds their verbatim text.
pub const RECALL_DRAFT_WINDOW: usize = 300;

/// One line of `recall-manifest.jsonl`: what recall needs to know about a
/// record history without parsing it, valid while the file's length and
/// modification time are unchanged. A cache only: unknown files are parsed.
#[derive(Serialize, Deserialize, Clone)]
struct ManifestEntry {
    id: String,
    revision: u64,
    status: String,
    process: bool,
    recorded_at: String,
    len: u64,
    mtime_ns: u64,
}

fn file_identity(path: &Path) -> Result<(u64, u64)> {
    let meta = fs::metadata(path)?;
    let mtime = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    Ok((meta.len(), mtime))
}

pub fn sha(bytes: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(bytes.as_ref()))
}

/// Block recognizable credentials at derived-memory and external-input boundaries.
/// This is a conservative marker check, not a claim to detect every secret.
pub fn contains_credential(value: &str) -> bool {
    credential_marker(value)
        || match serde_json::from_str::<Value>(value) {
            Ok(value) => credential_json(&value),
            Err(_) => explicit_credential_assignment(value),
        }
}

fn credential_marker(value: &str) -> bool {
    ["sk-", "apikey_"].iter().any(|prefix| {
        value.match_indices(prefix).any(|(offset, _)| {
            // "sk-" inside a word ("task-", "risk-") is not a key prefix.
            let inside_word = *prefix == "sk-"
                && value[..offset]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric());
            !inside_word
                && value[offset + prefix.len()..]
                    .bytes()
                    .take_while(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(byte))
                    .take(16)
                    .count()
                    == 16
        })
    }) || [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----BEGIN ENCRYPTED PRIVATE KEY-----",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

const SECRET_KEYS: &[&str] = &["password", "passwd", "passphrase", "pwd", "密码", "口令"];

/// Inspect one object field without parsing or traversing its value.
pub(crate) fn credential_field(key: &str, value: &Value) -> bool {
    let key = key.to_ascii_lowercase();
    let secret_key = SECRET_KEYS.iter().any(|secret| {
        key == *secret
            || key.strip_suffix(secret).is_some_and(|prefix| {
                prefix.ends_with(['_', '-']) || matches!(prefix, "登录" | "登陆" | "访问")
            })
    });
    secret_key
        && match value {
            Value::String(text) => credential_value(text),
            Value::Number(_) => true,
            _ => false,
        }
}

fn credential_json(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| credential_field(key, value) || credential_json(value)),
        Value::Array(values) => values.iter().any(credential_json),
        // Nested JSON text occurs in source anchors and serialized requests.
        Value::String(text) => contains_credential(text),
        _ => false,
    }
}

/// Recognize explicit assignments, including conditional Chinese login instructions.
/// All slicing uses char boundaries; quoted values can include spaces and Unicode.
fn explicit_credential_assignment(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    SECRET_KEYS.iter().any(|key| {
        lower.match_indices(key).any(|(offset, _)| {
            let tail = &value[offset + key.len()..];
            if key.is_ascii() {
                let word = |c: char| c.is_alphanumeric() || c == '_';
                if value[..offset]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric())
                    || tail.chars().next().is_some_and(word)
                {
                    return false;
                }
            }
            // Markdown/JSON key delimiters are not part of the value.
            let tail = tail.trim_start_matches(|c: char| {
                c.is_whitespace() || matches!(c, '\'' | '"' | '`' | '*' | '”' | '’')
            });
            if ["为什么", "为啥", "为何", "是什么", "是不是", "是否"]
                .iter()
                .any(|prefix| tail.starts_with(prefix))
            {
                return false;
            }
            let assignment = ["：", ":", "=", "就是", "是", "为"]
                .iter()
                .find_map(|prefix| tail.strip_prefix(prefix));
            let assignment = assignment.or_else(|| {
                // A bounded grammar, not a search for arbitrary later prose:
                // e.g. 密码如果问就是 / 口令若需要输入则为.
                let conditional = ["如果", "若", "如需"]
                    .iter()
                    .find_map(|prefix| tail.strip_prefix(prefix))?;
                for (index, c) in conditional.char_indices().take(12) {
                    if let Some(value) = conditional[index..]
                        .strip_prefix('是')
                        .or_else(|| conditional[index..].strip_prefix('为'))
                    {
                        return Some(value);
                    }
                    if !"问询需要求输入提示用到的话就则填写登录登陆".contains(c)
                        && !c.is_whitespace()
                    {
                        break;
                    }
                }
                None
            });
            assignment.is_some_and(|tail| {
                let tail = tail.trim_start();
                let Some(first) = tail.chars().next() else {
                    return false;
                };
                let closing = match first {
                    '\'' | '"' | '`' => Some(first),
                    '“' => Some('”'),
                    '‘' => Some('’'),
                    _ => None,
                };
                let value = if let Some(closing) = closing {
                    let inner = &tail[first.len_utf8()..];
                    inner.split(closing).next().unwrap_or(inner)
                } else {
                    tail.split(|c: char| {
                        c.is_whitespace() || matches!(c, ',' | '，' | '。' | ';' | '；')
                    })
                    .next()
                    .unwrap_or_default()
                };
                credential_value(value)
            })
        })
    })
}

fn credential_value(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    // Only explicit redaction/placeholders and references are exempt. A weak
    // literal such as "password" is still a credential, not a placeholder.
    let placeholder = matches!(
        lower.as_str(),
        "redacted"
            | "[redacted]"
            | "[hidden]"
            | "[removed]"
            | "null"
            | "none"
            | "unknown"
            | "your_password"
            | "your_passphrase"
            | "your_pwd"
            | "未提供"
            | "未知"
            | "已隐藏"
            | "已脱敏"
            | "不记录"
            | "不要输出"
            | "不要记录"
            | "不允许输出"
            | "不允许记录"
            | "未设置"
            // "密码为空" states an empty value; nothing secret is disclosed.
            | "空"
    ) || (value.starts_with('<') && value.ends_with('>'))
        || value
            .chars()
            .all(|c| matches!(c, '*' | '•' | '●' | '.' | '…'))
        || lower.starts_with("${")
        || (value.starts_with('$')
            && value[1..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'))
        || [
            "env:",
            "env.",
            "os.environ",
            "process.env.",
            "getenv(",
            "file:",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        || value.starts_with('/')
        || value.starts_with("~/")
        || value.starts_with("./")
        || value.starts_with("../")
        || (value.len() > 2 && value.as_bytes()[1] == b':' && value.as_bytes()[2] == b'\\');
    !placeholder
}

/// Host-generated instructions are a separate source-qualification issue.
/// Ordinary discussions of hooks are unaffected; marker lines require review.
pub fn contains_host_injection(value: &str) -> bool {
    value.lines().any(|line| {
        let line = line.trim_start().to_ascii_lowercase();
        let stop_hook = [
            "a session-scoped stop hook is now active with condition:",
            "stop hook feedback:",
            "warning: the maximum number of unified exec processes",
            "this session is being continued from a previous conversation that ran out of context.",
            "<turn_aborted>",
            "<local-command-caveat>",
        ]
        .iter()
        .any(|marker| line.starts_with(marker));
        let mailbox_relay = ["总控新消息 ", "总控新消息：", "总控新消息:"]
            .iter()
            .any(|prefix| line.starts_with(prefix))
            && line.contains("请读取 omx mailbox");
        let cross_team_relay = line.strip_prefix("跨队 ").is_some_and(|rest| {
            rest.split_once(" 新消息：")
                .is_some_and(|(team, instruction)| {
                    !team.is_empty()
                        && team.len() <= 64
                        && team
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                        && instruction.starts_with("请读取 omx mailbox")
                })
        });
        stop_hook || mailbox_relay || cross_team_relay
    })
}

pub fn id(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty()
            && value.len() <= 120
            && value.as_bytes()[0].is_ascii_alphanumeric()
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)),
        "invalid_record_id"
    );
    Ok(value)
}

pub fn unique(values: &[String], label: &str) -> Result<()> {
    ensure!(
        values.iter().collect::<BTreeSet<_>>().len() == values.len(),
        "duplicate_{label}"
    );
    Ok(())
}

pub fn shape(value: &impl ValidateShape) -> Result<()> {
    value.validate_shape().map_err(anyhow::Error::msg)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&read_bytes(path)?)?)
}

pub fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut file =
        File::open(path).with_context(|| format!("read_unavailable:{}", path.display()))?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= MAX_BYTES,
        "input_budget_or_file_type"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    ensure!(
        bytes.len() as u64 == before.len()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "input_changed_during_read"
    );
    Ok(bytes)
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

pub fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "store_directory_symlink"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// A unique staging file and durable rename; an interrupted staging file is retained.
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("store_parent_missing")?;
    private_dir(parent)?;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    ensure!(bytes.len() as u64 <= MAX_BYTES, "record_budget_exceeded");
    let mut stage = None;
    for attempt in 0..1000 {
        let name = parent.join(format!(
            ".{}.{}.{}.pending",
            path.file_name().unwrap().to_string_lossy(),
            std::process::id(),
            attempt
        ));
        match private_options().create_new(true).open(&name) {
            Ok(file) => {
                stage = Some((name, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let (name, mut file) = stage.context("staging_name_budget")?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&name, path)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn catalog(path: &Path, maximum: usize) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        ensure!(paths.len() < maximum, "catalog_budget_exceeded");
        if entry.path().extension().is_some_and(|x| x == "json") {
            ensure!(entry.file_type()?.is_file(), "catalog_non_regular");
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn verify_sources(sources: &[SourceAnchor]) -> Result<()> {
    ensure!(sources.len() <= 128, "source_count_budget");
    let mut bytes = 0u64;
    for source in sources {
        shape(source)?;
        ensure!(
            source.byte_end > source.byte_start && source.byte_end - source.byte_start <= MAX_BYTES,
            "source_anchor_budget_or_range"
        );
        bytes += source.byte_end - source.byte_start;
        ensure!(bytes <= MAX_BYTES, "source_verification_budget");
        let window = read_window(&EvidenceArgs {
            source: source.source_path.clone().into(),
            provider: source.provider.clone(),
            profile: source.profile.clone(),
            start_byte: source.byte_start,
            end_byte: Some(source.byte_end),
            max_record_bytes: MAX_BYTES,
            json: true,
        })?;
        ensure!(
            window.complete_visible_window
                && window.records.as_slice() == std::slice::from_ref(source),
            "source_anchor_mismatch"
        );
    }
    Ok(())
}

/// A source-window draft named `process-<packet>`, before any host review. Recall ranks
/// these after every curated record; a reviewed revision is curated again.
pub fn process_draft(record: &ExperienceRecord) -> bool {
    record.status == "draft" && record.id.starts_with("process-")
}

pub fn validate_experience(record: &ExperienceRecord) -> Result<()> {
    ensure!(
        !contains_credential(&serde_json::to_string(record)?),
        "experience_contains_credential"
    );
    shape(record)?;
    id(&record.id)?;
    ensure!(
        record.revision > 0
            && record.supersedes == (record.revision > 1).then(|| record.revision - 1),
        "invalid_revision_chain"
    );
    chrono::DateTime::parse_from_rfc3339(&record.recorded_at).context("invalid_recorded_at")?;
    ensure!(
        !record.sources.is_empty() && !record.timeline.is_empty(),
        "experience_requires_sources_and_timeline"
    );
    unique(
        &record
            .sources
            .iter()
            .map(|s| s.sha256.clone())
            .collect::<Vec<_>>(),
        "source",
    )?;
    unique(&record.conditions, "condition")?;
    let sources: BTreeSet<_> = record.sources.iter().map(|s| &s.sha256).collect();
    let references = |values: &[String]| -> Result<()> {
        ensure!(
            !values.is_empty() && values.iter().all(|v| sources.contains(v)),
            "source_reference_missing"
        );
        unique(values, "source_reference")
    };
    let mut seen = BTreeSet::new();
    for event in &record.timeline {
        id(&event.id)?;
        ensure!(!seen.contains(&event.id), "duplicate_timeline_id");
        ensure!(
            event.corrects.iter().all(|v| seen.contains(v)),
            "correction_target_missing_or_forward"
        );
        unique(&event.corrects, "correction")?;
        ensure!(
            (event.kind == "correction") != event.corrects.is_empty(),
            "correction_relation_required"
        );
        references(&event.source_sha256)?;
        seen.insert(&event.id);
    }
    for claim in &record.causal_claims {
        references(&claim.source_sha256)?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct History {
    schema: String,
    revisions: Vec<ExperienceRecord>,
}

pub struct Store {
    pub directory: PathBuf,
}

impl Store {
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// Read-only admission diagnosis; never emit raw text or matched values.
    pub fn source_eligibility(&self, packet_id: &str) -> Result<Value> {
        let packet = self.packet(packet_id)?;
        if let Some(capture) = &packet.capture {
            capture.verify()?;
        }
        self.verify_record_sources(&packet.sources)?;
        let mut findings = Vec::new();
        for source in &packet.sources {
            if contains_credential(&source.text) {
                findings.push(
                    json!({"source_sha256":source.sha256,"reason":"credential_requires_review"}),
                );
            }
            if contains_host_injection(&source.text) {
                findings.push(json!({"source_sha256":source.sha256,"reason":"host_runtime_injection_requires_review"}));
            }
        }
        for condition in &packet.registration.conditions {
            if contains_credential(condition) || contains_host_injection(condition) {
                findings.push(
                    json!({"condition_sha256":sha(condition),"reason":"condition_requires_review"}),
                );
            }
        }
        Ok(json!({
            "schema":"engineering-memory.source-eligibility/1",
            "packet_id":packet.id,
            "source_verified":true,
            "eligible":findings.is_empty() && packet.gaps.is_empty() && !packet.sources.is_empty(),
            "gap_count":packet.gaps.len(),
            "source_count":packet.sources.len(),
            "findings":findings,
            "new_model_calls":0,
            "scope":"source admission only; not policy authorization or memory quality"
        }))
    }

    pub fn lock(&self) -> Result<File> {
        private_dir(&self.directory)?;
        let file = private_options()
            .create(true)
            .truncate(false)
            .open(self.directory.join("writer.lock"))?;
        let start = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && start.elapsed() < Duration::from_secs(10) =>
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error).context("store_busy_or_lock_failed"),
            }
        }
    }

    pub(crate) fn try_lock(&self) -> Result<Option<File>> {
        private_dir(&self.directory)?;
        let file = private_options()
            .create(true)
            .truncate(false)
            .open(self.directory.join("writer.lock"))?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error).context("store_lock_failed"),
        }
    }

    fn history(&self, record_id: &str) -> Result<History> {
        let file = self
            .directory
            .join("records")
            .join(format!("{}.json", id(record_id)?));
        if !file.exists() {
            return Ok(History {
                schema: "engineering-memory.history/1".into(),
                revisions: vec![],
            });
        }
        let history: History = read_json(&file)?;
        ensure!(
            history.schema == "engineering-memory.history/1" && !history.revisions.is_empty(),
            "history_corrupt"
        );
        for (index, record) in history.revisions.iter().enumerate() {
            validate_experience(record)?;
            ensure!(
                record.id == record_id && record.revision == index as u64 + 1,
                "history_identity_or_order_mismatch"
            );
            if index > 0 {
                Self::preserve(&history.revisions[index - 1], record)?;
            }
        }
        Ok(history)
    }

    /// The latest revision of a history without re-validating it. Only for
    /// recall, on a file the manifest refresh validated with the same identity.
    fn latest_unchecked(&self, record_id: &str) -> Result<ExperienceRecord> {
        let file = self
            .directory
            .join("records")
            .join(format!("{}.json", id(record_id)?));
        let history: History = read_json(&file)?;
        ensure!(
            history.schema == "engineering-memory.history/1",
            "history_corrupt"
        );
        let record = history
            .revisions
            .into_iter()
            .next_back()
            .context("history_corrupt")?;
        ensure!(record.id == record_id, "history_identity_or_order_mismatch");
        Ok(record)
    }

    pub fn get(&self, record_id: &str, revision: Option<u64>) -> Result<ExperienceRecord> {
        let history = self.history(record_id)?;
        history
            .revisions
            .into_iter()
            .rev()
            .find(|r| revision.is_none_or(|v| r.revision == v))
            .context("experience_revision_missing")
    }

    fn preserve(previous: &ExperienceRecord, record: &ExperienceRecord) -> Result<()> {
        ensure!(
            record.timeline.starts_with(&previous.timeline),
            "timeline_rewrite_requires_appended_correction"
        );
        ensure!(
            previous.sources.iter().all(|s| record.sources.contains(s)),
            "source_history_cannot_be_removed"
        );
        Ok(())
    }

    pub fn preflight(&self, record: &ExperienceRecord) -> Result<bool> {
        validate_experience(record)?;
        self.verify_record_sources(&record.sources)?;
        let history = self.history(&record.id)?;
        if let Some(existing) = history
            .revisions
            .iter()
            .find(|r| r.revision == record.revision)
        {
            ensure!(
                existing == record,
                "revision_already_exists_with_other_content"
            );
            return Ok(false);
        }
        ensure!(
            record.revision == history.revisions.len() as u64 + 1,
            "revision_conflict"
        );
        if let Some(previous) = history.revisions.last() {
            Self::preserve(previous, record)?;
        }
        Ok(true)
    }

    pub(crate) fn record_locked(&self, record: &ExperienceRecord) -> Result<Value> {
        let effect = self.preflight(record)?;
        if effect {
            let mut history = self.history(&record.id)?;
            history.revisions.push(record.clone());
            atomic_json(
                &self
                    .directory
                    .join("records")
                    .join(format!("{}.json", record.id)),
                &history,
            )?;
        }
        Ok(json!({"record": record, "effect": effect}))
    }

    fn manifest_path(&self) -> PathBuf {
        self.directory.join("recall-manifest.jsonl")
    }

    /// The manifest entry for a parsed record history with the given identity.
    fn manifest_entry(record: &ExperienceRecord, identity: (u64, u64)) -> ManifestEntry {
        ManifestEntry {
            id: record.id.clone(),
            revision: record.revision,
            status: record.status.clone(),
            process: process_draft(record),
            recorded_at: record.recorded_at.clone(),
            len: identity.0,
            mtime_ns: identity.1,
        }
    }

    /// The newest manifest entry per record id. Unreadable lines are skipped.
    fn load_manifest(&self) -> BTreeMap<String, ManifestEntry> {
        let mut entries = BTreeMap::new();
        if let Ok(text) = fs::read_to_string(self.manifest_path()) {
            for line in text.lines() {
                if let Ok(entry) = serde_json::from_str::<ManifestEntry>(line) {
                    entries.insert(entry.id.clone(), entry);
                }
            }
        }
        entries
    }

    /// Bring the recall manifest up to date with records/ (zero model calls).
    /// Only the tick maintains it, so a record write has no side file: a history
    /// the manifest has not seen yet (new, or written by another runtime) is
    /// parsed here, at most `max_parse` per call, without the writer lock; the
    /// entries are appended under it. A manifest much longer than records/ is
    /// rewritten with the newest entry per record.
    pub fn recall_manifest_refresh(&self, max_parse: usize) -> Result<Value> {
        let manifest = self.load_manifest();
        let files = catalog(&self.directory.join("records"), MAX_RECORDS)?;
        let (mut entries, mut unreadable, mut waiting) = (Vec::new(), 0, 0);
        for file in &files {
            let record_id = file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .context("invalid_record_id")?;
            let identity = file_identity(file)?;
            if manifest
                .get(record_id)
                .is_some_and(|e| (e.len, e.mtime_ns) == identity)
            {
                continue;
            }
            if entries.len() + unreadable >= max_parse {
                waiting += 1;
                continue;
            }
            match self.get(record_id, None) {
                // A history rewritten while it was read waits for the next call.
                Ok(record) if file_identity(file)? == identity => {
                    entries.push(Self::manifest_entry(&record, identity));
                }
                Ok(_) => waiting += 1,
                Err(_) => unreadable += 1,
            }
        }
        let _lock = self.lock()?;
        let noted = entries.len();
        if !entries.is_empty() {
            let mut lines = Vec::new();
            for entry in &entries {
                lines.extend(serde_json::to_vec(entry)?);
                lines.push(b'\n');
            }
            let mut out = private_options()
                .create(true)
                .append(true)
                .open(self.manifest_path())?;
            out.write_all(&lines)?;
        }
        let lines = fs::read_to_string(self.manifest_path())
            .map(|text| text.lines().count())
            .unwrap_or(0);
        let compacted = lines > 2 * files.len() + 64;
        if compacted {
            let manifest = self.load_manifest();
            let mut text = Vec::new();
            for file in &files {
                let record_id = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if let Some(entry) = manifest.get(record_id)
                    && (entry.len, entry.mtime_ns) == file_identity(file)?
                {
                    text.extend(serde_json::to_vec(entry)?);
                    text.push(b'\n');
                }
            }
            let staged = self.directory.join(".recall-manifest.jsonl.pending");
            let mut out = private_options()
                .create(true)
                .truncate(true)
                .open(&staged)?;
            out.write_all(&text)?;
            out.sync_all()?;
            fs::rename(&staged, self.manifest_path())?;
        }
        Ok(
            json!({"records":files.len(),"noted":noted,"unreadable":unreadable,"waiting":waiting,
            "manifest_lines_before":lines,"compacted":compacted,"new_model_calls":0}),
        )
    }

    pub fn record(&self, record: &ExperienceRecord) -> Result<Value> {
        let _lock = self.lock()?;
        self.record_locked(record)
    }

    pub fn verify_record(&self, record_id: &str, revision: Option<u64>) -> Result<Value> {
        let record = self.get(record_id, revision)?;
        let captures = self.verify_record_sources(&record.sources)?;
        Ok(
            json!({"id":record.id,"revision":record.revision,"source_status":"matched","source_captures":captures,
            "verified_at":chrono::Utc::now().to_rfc3339(),"engineering_adoption":"not_granted"}),
        )
    }

    pub(crate) fn immutable(
        &self,
        folder: &str,
        record_id: &str,
        value: &impl Serialize,
    ) -> Result<bool> {
        let file = self
            .directory
            .join(folder)
            .join(format!("{}.json", id(record_id)?));
        if file.exists() {
            ensure!(
                read_json::<Value>(&file)? == serde_json::to_value(value)?,
                "immutable_record_conflict"
            );
            return Ok(false);
        }
        atomic_json(&file, value)?;
        Ok(true)
    }

    pub fn recall(
        &self,
        query: &str,
        project: Option<&str>,
        conditions: &[String],
        limit: usize,
        include_other: bool,
    ) -> Result<Value> {
        self.recall_with_draft_window(
            query,
            project,
            conditions,
            limit,
            include_other,
            RECALL_DRAFT_WINDOW,
        )
    }

    /// `recall` with an explicit process-draft window (tests use a small one).
    #[doc(hidden)]
    pub fn recall_with_draft_window(
        &self,
        query: &str,
        project: Option<&str>,
        conditions: &[String],
        limit: usize,
        include_other: bool,
        draft_window: usize,
    ) -> Result<Value> {
        ensure!(
            (1..=50).contains(&limit) && query.len() <= 4096,
            "recall_limit_invalid"
        );
        // Every curated history is scored. Machine process drafts known from
        // the manifest are scored newest first up to the window; a history the
        // manifest has not seen (or that changed since) is always parsed.
        let manifest = self.load_manifest();
        // (record id, validated by the manifest refresh with this exact identity)
        let mut selected = Vec::new();
        let mut drafts = Vec::new();
        for file in catalog(&self.directory.join("records"), MAX_RECORDS)? {
            let record_id = file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .context("invalid_record_id")?
                .to_owned();
            let known = manifest.get(&record_id).filter(|entry| {
                file_identity(&file).is_ok_and(|identity| identity == (entry.len, entry.mtime_ns))
            });
            match known {
                Some(entry) if entry.process => {
                    drafts.push((entry.recorded_at.clone(), record_id));
                }
                Some(_) => selected.push((record_id, true)),
                None => selected.push((record_id, false)),
            }
        }
        drafts.sort_by(|a, b| b.cmp(a));
        let older_drafts = drafts.len().saturating_sub(draft_window);
        let scored_drafts = drafts.len() - older_drafts;
        selected.extend(
            drafts
                .into_iter()
                .take(draft_window)
                .map(|(_, id)| (id, true)),
        );
        let mut records = Vec::new();
        // One unreadable history must not hide every other experience from
        // recall. get/verify stay strict; recall reports the record instead.
        let mut unreadable = Vec::new();
        for (record_id, validated) in &selected {
            let record_id = record_id.as_str();
            // A history the refresh validated, unchanged since (same length and
            // modification time), is only parsed; any other is validated in full.
            let loaded = if *validated {
                self.latest_unchecked(record_id)
            } else {
                self.get(record_id, None)
            };
            let record = match loaded {
                Ok(record) => record,
                Err(error) => {
                    unreadable.push(json!({"id":record_id,"error":error.to_string()}));
                    continue;
                }
            };
            if record.project_id.is_some()
                && project != record.project_id.as_deref()
                && !include_other
            {
                continue;
            }
            records.push(record);
        }
        let ranked = crate::retrieval::lexical_candidates(query, records);
        // Machine process drafts (verbatim window candidates) never displace
        // curated experience: they keep their own order after every other match.
        let (curated, process): (Vec<_>, Vec<_>) = ranked
            .into_iter()
            .partition(|(_, record)| !process_draft(record));
        let ranked: Vec<_> = curated.into_iter().chain(process).collect();
        let total = ranked.len();
        let mut candidates = Vec::new();
        for (score, record) in ranked.into_iter().take(limit) {
            let verification = self.verify_record_sources(&record.sources);
            let captures = verification.as_ref().ok().cloned().unwrap_or_default();
            let review = review_scope(&record, project, conditions, verification.is_ok())?;
            candidates.push(
                json!({"record":record,"score":score,"scope_status":review["scope_status"],
                "reasons":review["reasons"],"missing_conditions":review["missing_conditions"],
                "semantic_review":review,"source_error":verification.err().map(|e| e.to_string()),"source_captures":captures,
                "engineering_adoption":"not_granted"}),
            );
        }
        Ok(
            json!({"schema":"engineering-memory.recall/1","snapshot_sha256":sha(serde_json::to_vec(&candidates)?),
            "candidates":candidates,"total_matches":total,"unreadable_records":unreadable,
            "query":query,"project_id":project,"conditions":conditions,
            "include_other_projects":include_other,"limit":limit,"source_revalidation":"performed_for_selected_candidates",
            "retrieval":"unicode_field_density/2","process_drafts":"ranked_after_curated",
            "draft_window":{"window":draft_window,"scored_drafts":scored_drafts,"older_drafts_not_scored":older_drafts,
                "next":"history-search finds the original text of older windows"},
            "scope":"candidate recall; Semantica scope review does not grant engineering adoption"}),
        )
    }

    pub fn feedback(&self, feedback: &UsageFeedback) -> Result<Value> {
        let _lock = self.lock()?;
        self.feedback_locked(feedback)
    }

    pub(crate) fn feedback_locked(&self, feedback: &UsageFeedback) -> Result<Value> {
        self.preflight_feedback(feedback)?;
        let effect = self.immutable("feedback", &feedback.id, feedback)?;
        Ok(json!({"feedback":feedback,"effect":effect}))
    }

    /// Validate before a caller publishes a durable multi-file operation intent.
    pub(crate) fn preflight_feedback(&self, feedback: &UsageFeedback) -> Result<()> {
        shape(feedback)?;
        id(&feedback.id)?;
        self.get(&feedback.record_id, Some(feedback.revision))?;
        ensure!(
            feedback.used == matches!(feedback.outcome.as_str(), "helped" | "harmed" | "no_delta"),
            "feedback_use_outcome_conflict"
        );
        ensure!(
            !matches!(feedback.outcome.as_str(), "helped" | "harmed")
                || !feedback.sources.is_empty(),
            "feedback_evidence_missing"
        );
        self.verify_record_sources(&feedback.sources)?;
        let existing = self
            .directory
            .join("feedback")
            .join(format!("{}.json", feedback.id));
        if existing.exists() {
            ensure!(
                read_json::<UsageFeedback>(&existing)? == *feedback,
                "immutable_record_conflict"
            );
        }
        Ok(())
    }

    pub fn compare(&self, comparison: &MethodComparison) -> Result<Value> {
        shape(comparison)?;
        id(&comparison.id)?;
        ensure!(
            !comparison.transferable.is_empty()
                && !comparison.non_transferable.is_empty()
                && !comparison.required_validation.is_empty(),
            "comparison_boundaries_and_validation_required"
        );
        let _lock = self.lock()?;
        let left = self.get(&comparison.left.id, Some(comparison.left.revision))?;
        let right = self.get(&comparison.right.id, Some(comparison.right.revision))?;
        ensure!(
            left.id != right.id && left.method.is_some() && right.method.is_some(),
            "comparison_requires_two_methods"
        );
        self.verify_record_sources(&left.sources)?;
        self.verify_record_sources(&right.sources)?;
        let effect = self.immutable("comparisons", &comparison.id, comparison)?;
        Ok(
            json!({"comparison":comparison,"effect":effect,"transfer_validation":"not_run","engineering_adoption":"not_granted"}),
        )
    }

    /// A portable, source-bound suggestion. The receiving v2 Project owns adoption.
    pub fn handoff(
        &self,
        record_id: &str,
        revision: u64,
        project: Option<&str>,
        conditions: &[String],
    ) -> Result<Value> {
        let record = self.get(record_id, Some(revision))?;
        let captures = self.verify_record_sources(&record.sources)?;
        let review = review_scope(&record, project, conditions, true)?;
        let mut feedback = Vec::new();
        for file in catalog(&self.directory.join("feedback"), 10_000)? {
            let item: UsageFeedback = read_json(&file)?;
            if item.record_id == record_id && item.revision == revision {
                feedback.push(item);
            }
        }
        Ok(
            json!({"schema":"engineering-memory.v2-candidate/1","record":record,"record_sha256":sha(serde_json::to_vec(&record)?),
            "target_project":project,"semantic_review":review,"feedback":feedback,"project_write":false,"source_captures":captures,
            "rrsi_validation":"not_run","engineering_adoption":"not_granted",
            "required_next":"Bind this candidate as an external source in the receiving Project, then independently evaluate applicability and any proposed change"}),
        )
    }
}

/// Actual v2 unary Horn execution, with observable premises and provenance.
pub fn review_scope(
    record: &ExperienceRecord,
    project: Option<&str>,
    conditions: &[String],
    source_matched: bool,
) -> Result<Value> {
    let same_project = project.is_some() && project == record.project_id.as_deref();
    let missing: Vec<_> = record
        .conditions
        .iter()
        .filter(|c| !conditions.contains(c))
        .collect();
    let mut facts = Vec::new();
    let mut reasons = Vec::new();
    if same_project {
        facts.push("project_match(record)");
    } else {
        reasons.push(if project.is_none() || record.project_id.is_none() {
            "project_unbound".into()
        } else {
            "different_project".into()
        });
    }
    if record.conditions.is_empty() {
        reasons.push("conditions_undeclared".into());
    } else if missing.is_empty() {
        facts.push("conditions_match(record)");
    } else {
        reasons.push("conditions_unconfirmed".into());
    }
    if record.status == "reviewed" {
        facts.push("reviewed(record)");
    } else {
        reasons.push(format!("status_{}", record.status));
    }
    if source_matched {
        facts.push("source_matched(record)");
    } else {
        reasons.push("source_not_matched".into());
    }
    let execution = semantica_runtime::rules::forward_chain(&facts.join("\n"), SCOPE_RULE)?;
    let matched = execution["conclusions"]
        .as_array()
        .context("semantic_output_invalid")?
        .iter()
        .any(|v| v == "scope_matched(record)");
    if matched && !reasons.is_empty() {
        bail!("semantic_scope_inconsistent");
    }
    Ok(
        json!({"scope_status":if matched {"scope_matched"} else {"review_required"},"reasons":reasons,
        "missing_conditions":missing,"engine":semantica_runtime::ENGINE_ID,"rule":SCOPE_RULE,"facts":facts,
        "execution":execution,"engineering_adoption":"not_granted"}),
    )
}
