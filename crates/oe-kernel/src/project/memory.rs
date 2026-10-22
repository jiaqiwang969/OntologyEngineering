//! Engineering-memory recall candidates in the same Project (`oev2.memory-candidate-check/1`).
//!
//! A memory handoff is re-verified natively before it enters the Project: the non-adoption flags it
//! carries, the experience record digest, every source window's bytes (and, where readable, its role
//! and visible text) and its Project binding. Registration adds only `memory-candidate:<id>@r<revision>`
//! to a new Project copy. Memory evidence never supports a decision, closes an acceptance item or
//! stands in for a tool record; usage feedback is written in memory's own contract.

use super::{Project, tool_result, validate};
use anyhow::{Context, Result, ensure};
use oe_contracts::{ArtifactReference, MemoryCandidateCheck};
use oe_core::{SkillRoot, sha256};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const CHECK_SCHEMA: &str = "oev2.memory-candidate-check/1";
pub const HANDOFF_SCHEMA: &str = "engineering-memory.session-handoff/1";
pub const CANDIDATE_SCHEMA: &str = "engineering-memory.v2-candidate/1";
pub const SESSION_FEEDBACK_SCHEMA: &str = "engineering-memory.session-feedback/1";
pub const FEEDBACK_SCHEMA: &str = "engineering-memory.feedback/1";
pub const EVIDENCE_PREFIX: &str = "memory-candidate:";
const SCOPE: &str = "Provenance and Project binding of a memory recall candidate only; never decision support, acceptance closure, adoption or proof of its claims.";
const CANDIDATE_LIMIT: usize = 16 * 1024 * 1024;
const WINDOW_LIMIT: u64 = 16 * 1024 * 1024;
const TOTAL_LIMIT: u64 = 64 * 1024 * 1024;
const MAX_SOURCES: usize = 128;
const EXPORT_FILE_LIMIT: u64 = 64 * 1024 * 1024;
const EXPORT_FILES: usize = 20_000;
const OUTCOMES: [&str; 5] = ["helped", "harmed", "no_delta", "rejected", "not_used"];

/// Memory-derived evidence: native candidates and the ids of memory's own v2-prepare route
/// (`memory-<16 hex>-<n>`).
pub fn is_memory_evidence(id: &str) -> bool {
    if id.starts_with(EVIDENCE_PREFIX) {
        return true;
    }
    let Some(rest) = id.strip_prefix("memory-") else {
        return false;
    };
    let mut parts = rest.splitn(2, '-');
    matches!((parts.next(), parts.next()), (Some(h), Some(n))
        if h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit())
            && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                out.insert(k.clone(), sorted(&m[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        x => x.clone(),
    }
}

/// Memory's record digest: SHA-256 of the compact JSON with object keys in byte order.
pub fn record_digest(record: &Value) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(&sorted(record))?))
}

fn window(path: &Path, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || end > meta.len() {
        return Err(std::io::Error::other("source_not_regular_or_short"));
    }
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(start))?;
    let mut b = vec![0; usize::try_from(end - start).map_err(std::io::Error::other)?];
    f.read_exact(&mut b)?;
    Ok(b)
}

/// Re-read one memory source window: its exact bytes and, where the window parses as a
/// transcript record, the role and visible text it claims.
fn check_anchor(
    i: usize,
    a: &Value,
    total: &mut u64,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let invalid = || format!("source_invalid:{i}");
    let path = Path::new(a["source_path"].as_str().ok_or_else(invalid)?);
    let start = a["byte_start"].as_u64().ok_or_else(invalid)?;
    let end = a["byte_end"].as_u64().ok_or_else(invalid)?;
    let digest = a["sha256"].as_str().ok_or_else(invalid)?;
    if !path.is_absolute() || end <= start || end - start > WINDOW_LIMIT {
        return Err(invalid());
    }
    *total += end - start;
    if *total > TOTAL_LIMIT {
        return Err("sources_over_budget".into());
    }
    let bytes = window(path, start, end).map_err(|_| format!("source_unreadable:{i}"))?;
    if sha256(&bytes) != digest {
        return Err(format!("source_changed:{i}"));
    }
    if end - start > oe_knowledge::conversation::MAX_RECORD_BYTES as u64 {
        warnings.push(format!("source_text_unchecked:{i}"));
        return Ok(());
    }
    let read = oe_knowledge::conversation::read_window(path, start, end)
        .map_err(|_| format!("source_unparsed:{i}"))?;
    let rows: Vec<&Value> = read["entries"]
        .as_array()
        .map(|e| e.iter().filter(|r| r["kind"] == "message").collect())
        .unwrap_or_default();
    let texts: Vec<&str> = rows.iter().filter_map(|r| r["text"].as_str()).collect();
    if rows.is_empty()
        || rows
            .iter()
            .any(|r| r["role"] != a["role"] || r["raw_sha256"] != a["sha256"])
        || json!(texts.join("\n")) != a["text"]
    {
        return Err(format!("source_text_mismatch:{i}"));
    }
    Ok(())
}

fn binding(handoff: &Value, project: &Project, warnings: &mut Vec<String>) -> &'static str {
    let sp = &handoff["project"];
    if sp.is_null() {
        return "unbound";
    }
    if sp["project_id"] != json!(project.project_id) {
        return "project_mismatch";
    }
    let o = &sp["object"];
    if !o.is_null() {
        let id = o["object_id"].as_str().unwrap_or_default();
        if project.objects.get(id).map(String::as_str) != o["object_revision"].as_str() {
            return "object_changed";
        }
        let scope = o["condition_scope"].as_str().unwrap_or_default();
        let current = project.conditions.get(scope).map(|c| sha256(c.as_bytes()));
        if current.as_deref() != o["condition_sha256"].as_str() {
            return "condition_changed";
        }
    }
    if sp["revision"] != json!(project.revision) {
        warnings.push("project_revised_since_handoff".into());
    }
    "matched"
}

fn parse(bytes: &[u8]) -> Result<(Value, bool)> {
    let value = oe_core::json::strict_json(bytes)?;
    let wrapper = value["schema"] == HANDOFF_SCHEMA;
    Ok((value, wrapper))
}

fn identity(candidate: &Value) -> Option<(String, u64)> {
    let r = &candidate["record"];
    Some((
        r["id"]
            .as_str()
            .filter(|s| !s.trim().is_empty())?
            .to_owned(),
        r["revision"].as_u64()?,
    ))
}

/// Verify a memory handoff (`session-handoff/1`, or a bare `v2-candidate/1` as unbound) against the Project.
pub fn verify(
    root: &SkillRoot,
    project: &Project,
    project_sha256: &str,
    candidate: &ArtifactReference,
) -> Result<MemoryCandidateCheck> {
    validate(project)?;
    let mut check = MemoryCandidateCheck {
        schema: CHECK_SCHEMA.into(),
        status: "rejected".into(),
        candidate: candidate.clone(),
        record: None,
        record_sha256: None,
        project_binding: "unchecked".into(),
        sources_verified: 0,
        memory_scope: None,
        missing_conditions: vec![],
        evidence_id: None,
        project_revision: project.revision.clone(),
        project_sha256: project_sha256.into(),
        failures: vec![],
        warnings: vec![],
        scope: SCOPE.into(),
    };
    let (value, wrapper) = match tool_result::read_reference(root, candidate, CANDIDATE_LIMIT)
        .and_then(|b| parse(&b))
    {
        Ok(v) => v,
        Err(e) => {
            check.failures.push(format!("candidate_unreadable:{e}"));
            return Ok(check);
        }
    };
    let cand = if wrapper { &value["candidate"] } else { &value };
    let (mut fail, mut warn) = (Vec::new(), Vec::new());
    if (!wrapper && value["schema"] != CANDIDATE_SCHEMA) || cand["schema"] != CANDIDATE_SCHEMA {
        fail.push("candidate_schema".into());
    }
    if [&value, cand].iter().any(|v| {
        v["engineering_adoption"] != "not_granted"
            || v["project_write"] != false
            || v["rrsi_validation"] != "not_run"
    }) {
        fail.push("candidate_claims_authority".into());
    }
    let record = &cand["record"];
    match identity(cand) {
        Some((id, r)) => {
            check.record = Some(format!("{id}@r{r}"));
            check.evidence_id = Some(format!("{EVIDENCE_PREFIX}{id}@r{r}"));
        }
        None => fail.push("record_identity_missing".into()),
    }
    if record["status"] == "rejected" {
        fail.push("record_rejected".into());
    }
    let digest = record_digest(record)?;
    if cand["record_sha256"].as_str() != Some(digest.as_str()) {
        fail.push("record_digest_mismatch".into());
    }
    check.record_sha256 = Some(digest);
    let sources = record["sources"].as_array().cloned().unwrap_or_default();
    if sources.is_empty() {
        fail.push("sources_missing".into());
    } else if sources.len() > MAX_SOURCES {
        fail.push("sources_over_limit".into());
    } else {
        let mut total = 0;
        for (i, a) in sources.iter().enumerate() {
            match check_anchor(i, a, &mut total, &mut warn) {
                Ok(()) => check.sources_verified += 1,
                Err(f) => fail.push(f),
            }
        }
    }
    let bound = if wrapper {
        binding(&value, project, &mut warn)
    } else {
        match cand["target_project"].as_str() {
            Some(p) if p != project.project_id => "project_mismatch",
            _ => "unbound",
        }
    };
    match bound {
        "matched" => {}
        "unbound" => warn.push("project_unbound".into()),
        other => fail.push(format!("project_binding:{other}")),
    }
    check.project_binding = bound.into();
    let review = &cand["semantic_review"];
    check.memory_scope = review["scope_status"].as_str().map(str::to_owned);
    check.missing_conditions = review["missing_conditions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if check.memory_scope.as_deref() != Some("scope_matched") {
        warn.push("memory_scope_review_required".into());
    }
    check.status = if fail.is_empty() {
        "accepted"
    } else {
        "rejected"
    }
    .into();
    check.failures = fail;
    check.warnings = warn;
    Ok(check)
}

/// Register a verified candidate: a new Project copy whose only change is the candidate's evidence id.
pub fn register(
    root: &SkillRoot,
    project_bytes: &[u8],
    candidate: &ArtifactReference,
    revision: &str,
) -> Result<(Option<Vec<u8>>, MemoryCandidateCheck)> {
    let project: Project = serde_json::from_value(oe_core::json::strict_json(project_bytes)?)?;
    let mut check = verify(root, &project, &sha256(project_bytes), candidate)?;
    if revision.trim().is_empty() || revision == project.revision {
        check.failures.push("project_revision_not_new".into());
    }
    if let Some(id) = &check.evidence_id {
        if project.evidence.contains_key(id) {
            check.failures.push("candidate_already_registered".into());
        }
    }
    if !check.failures.is_empty() {
        check.status = "refused".into();
        return Ok((None, check));
    }
    let mut next = project.clone();
    next.revision = revision.into();
    next.evidence.insert(
        check.evidence_id.clone().context("evidence_id")?,
        candidate.sha256.clone(),
    );
    validate(&next)?;
    let mut bytes = serde_json::to_vec_pretty(&next)?;
    bytes.push(b'\n');
    check.status = "registered".into();
    check.project_revision = revision.into();
    check.project_sha256 = sha256(&bytes);
    Ok((Some(bytes), check))
}

/// What the agent observed about one use of a registered candidate.
pub struct FeedbackRequest {
    pub id: String,
    pub outcome: String,
    pub result: String,
    pub sources: Vec<Value>,
    pub elapsed_ms: Option<u64>,
    pub unknowns: Vec<String>,
    /// Required only for a bare `v2-candidate/1`: the memory context that exposed it.
    pub context: Option<(String, String)>,
}

/// Usage feedback in memory's own `engineering-memory.session-feedback/1`, bound to the current Project file.
/// Memory re-checks exposure, revision and the Project binding when it receives it.
pub fn feedback(
    root: &SkillRoot,
    project_path: &Path,
    project_bytes: &[u8],
    candidate: &ArtifactReference,
    req: &FeedbackRequest,
) -> Result<Value> {
    let project: Project = serde_json::from_value(oe_core::json::strict_json(project_bytes)?)?;
    validate(&project)?;
    let (value, wrapper) = parse(&tool_result::read_reference(
        root,
        candidate,
        CANDIDATE_LIMIT,
    )?)?;
    let cand = if wrapper { &value["candidate"] } else { &value };
    let (id, revision) = identity(cand).context("record_identity_missing")?;
    ensure!(
        project
            .evidence
            .get(&format!("{EVIDENCE_PREFIX}{id}@r{revision}"))
            == Some(&candidate.sha256),
        "memory_candidate_not_registered"
    );
    ensure!(
        OUTCOMES.contains(&req.outcome.as_str()),
        "feedback_outcome_invalid"
    );
    ensure!(
        !req.id.trim().is_empty() && !req.result.trim().is_empty(),
        "feedback_id_or_result_missing"
    );
    ensure!(
        !matches!(req.outcome.as_str(), "helped" | "harmed") || !req.sources.is_empty(),
        "feedback_sources_required_for_outcome"
    );
    let (mut total, mut warn) = (0, Vec::new());
    for (i, a) in req.sources.iter().enumerate() {
        check_anchor(i, a, &mut total, &mut warn).map_err(|f| anyhow::anyhow!("feedback_{f}"))?;
    }
    let (context_id, context_sha256) = if wrapper {
        (
            value["context_id"]
                .as_str()
                .context("context_missing")?
                .to_owned(),
            value["context_sha256"]
                .as_str()
                .context("context_missing")?
                .to_owned(),
        )
    } else {
        req.context
            .clone()
            .context("context_required_for_unbound_candidate")?
    };
    let bound = wrapper && !value["project"].is_null();
    let result_project = if bound {
        let o = &value["project"]["object"];
        let object = if o.is_null() {
            Value::Null
        } else {
            let oid = o["object_id"].as_str().context("object_missing")?;
            let scope = o["condition_scope"].as_str().context("object_missing")?;
            json!({"object_id": oid, "object_revision": project.objects.get(oid).context("object_missing")?,
                   "condition_scope": scope,
                   "condition_sha256": sha256(project.conditions.get(scope).context("condition_missing")?.as_bytes())})
        };
        json!({"path": project_path.to_str().context("project_path_not_utf8")?, "project_id": project.project_id,
               "revision": project.revision, "sha256": sha256(project_bytes), "object": object})
    } else {
        Value::Null
    };
    let used = matches!(req.outcome.as_str(), "helped" | "harmed" | "no_delta");
    Ok(
        json!({"schema": SESSION_FEEDBACK_SCHEMA, "context_id": context_id, "context_sha256": context_sha256,
        "feedback": {"schema": FEEDBACK_SCHEMA, "id": req.id, "record_id": id, "revision": revision,
                     "project_id": if bound { json!(project.project_id) } else { Value::Null },
                     "used": used, "outcome": req.outcome, "result": req.result, "sources": req.sources,
                     "elapsed_ms": req.elapsed_ms, "unknowns": req.unknowns},
        "result_project": result_project}),
    )
}

fn index(dir: &Path, out: &mut BTreeMap<String, String>, budget: &mut usize) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let meta = std::fs::symlink_metadata(e.path())?;
        if meta.is_dir() {
            index(&e.path(), out, budget)?;
        } else if meta.is_file()
            && meta.len() <= EXPORT_FILE_LIMIT
            && e.path().extension() == Some(std::ffi::OsStr::new("json"))
        {
            ensure!(*budget > 0, "export_file_budget");
            *budget -= 1;
            let path = e
                .path()
                .to_str()
                .context("export_path_not_utf8")?
                .to_owned();
            out.entry(sha256(&std::fs::read(e.path())?)).or_insert(path);
        }
    }
    Ok(())
}

/// Project records as memory organization artifacts `{id, path, sha256}`: the Project file, every registered
/// tool result and memory candidate found by digest under the Project directory, and the decision requests and
/// receipts kept there (refusals included). Memory re-hashes each artifact when it uses them.
pub fn export(project_path: &Path, project_bytes: &[u8]) -> Result<Value> {
    let project: Project = serde_json::from_value(oe_core::json::strict_json(project_bytes)?)?;
    validate(&project)?;
    let dir = project_path.parent().context("project_directory_missing")?;
    let mut by_digest = BTreeMap::new();
    let mut budget = EXPORT_FILES;
    index(dir, &mut by_digest, &mut budget)?;
    let path = project_path.to_str().context("project_path_not_utf8")?;
    let mut artifacts = vec![
        json!({"id": format!("project:{}", project.revision), "path": path, "sha256": sha256(project_bytes)}),
    ];
    let mut missing = Vec::new();
    for (id, digest) in &project.evidence {
        if id.starts_with("tool-result:") || id.starts_with(EVIDENCE_PREFIX) {
            match by_digest.get(digest) {
                Some(p) => artifacts.push(json!({"id": id, "path": p, "sha256": digest})),
                None => missing.push(id.clone()),
            }
        }
    }
    for sub in ["decisions", "receipts"] {
        let d = dir.join(sub);
        if !d.is_dir() {
            continue;
        }
        let mut files: Vec<_> = std::fs::read_dir(&d)?.collect::<std::io::Result<_>>()?;
        files.sort_by_key(|e| e.file_name());
        for e in files {
            let meta = std::fs::symlink_metadata(e.path())?;
            if meta.is_file()
                && meta.len() <= EXPORT_FILE_LIMIT
                && e.path().extension() == Some(std::ffi::OsStr::new("json"))
            {
                let name = e
                    .file_name()
                    .to_str()
                    .context("export_path_not_utf8")?
                    .to_owned();
                artifacts.push(json!({"id": format!("{sub}:{name}"), "path": e.path().to_str().context("export_path_not_utf8")?,
                                      "sha256": sha256(&std::fs::read(e.path())?)}));
            }
        }
    }
    Ok(json!({"artifacts": artifacts, "missing": missing}))
}
