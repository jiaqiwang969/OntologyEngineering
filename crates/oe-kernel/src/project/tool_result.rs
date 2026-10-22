//! Tool results handed back with provenance into the same Project.
//!
//! A result is accepted only when its Project binding, registered tool
//! identity, original records and finding bases re-verify. Registration writes
//! a new Project copy that adds evidence and decision dependencies; it never
//! changes a conclusion or grants adoption. Comparisons accept two registered
//! results measured under the same tool, metric, environment and input set,
//! with the baseline finished before the candidate started.
use super::{Project, impact, validate};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CapabilityRegistry, RegisteredCapability, ToolComparisonReport,
    ToolComparisonRequest, ToolResult, ToolResultCheck, ToolResultRegistration,
};
use oe_core::{SkillRoot, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const REGISTRY_SCHEMA: &str = "oev2.capability-registry/1";
pub const RESULT_SCHEMA: &str = "oev2.tool-result/1";
pub const CHECK_SCHEMA: &str = "oev2.tool-result-check/1";
pub const REGISTRATION_SCHEMA: &str = "oev2.tool-result-registration/1";
pub const COMPARISON_SCHEMA: &str = "oev2.tool-comparison/1";
pub const COMPARISON_REPORT_SCHEMA: &str = "oev2.tool-comparison-report/1";
pub const MANIFEST_SCHEMA: &str = "oev2.subject-manifest/1";
pub const DEFAULT_REGISTRY: &str = "var/capabilities/registry.json";

const RECORD_LIMIT: usize = 64 * 1024 * 1024;
const TOTAL_LIMIT: usize = 256 * 1024 * 1024;
const EXECUTABLE_LIMIT: u64 = 1024 * 1024 * 1024;
const MANIFEST_FILES: usize = 4096;
const CHECK_SCOPE: &str = "Project binding, registered tool identity and current executable bytes, original record bytes, finding bases and declared coverage only; does not prove the tool's conclusions, engineering adoption or method benefit.";
const REGISTRATION_SCOPE: &str = "New Project copy with evidence and decision dependencies only; conclusions and adoption are unchanged and the affected decisions require review.";
const COMPARISON_SCOPE: &str = "One baseline-first comparison of two registered results; does not prove general performance, adoption or method benefit.";

fn generated(r: std::result::Result<(), Vec<String>>, out: &mut Vec<String>, what: &str) {
    if let Err(f) = r {
        out.extend(
            f.into_iter()
                .map(|x| format!("contract_invalid:{what}:{x}")),
        );
    }
}

/// Read a skill-root relative reference and check its bytes.
pub fn read_reference(root: &SkillRoot, r: &ArtifactReference, limit: usize) -> Result<Vec<u8>> {
    let path = root.expand(Path::new(&r.path))?;
    ensure!(
        path.starts_with(root.path()),
        "tool_result_reference_outside_root"
    );
    ensure!(
        oe_core::portable_source_path(root.path(), &path)? == r.path,
        "tool_result_reference_not_portable"
    );
    let bytes = read_regular_bytes(&path, limit)?;
    ensure!(sha256(&bytes) == r.sha256, "tool_result_reference_drift");
    Ok(bytes)
}

/// UTC RFC3339 `YYYY-MM-DDTHH:MM:SS[.fraction]Z`, ordered as a tuple.
pub fn utc(text: &str) -> Option<(u32, u32, u32, u32, u32, u32, u32)> {
    let b = text.as_bytes();
    if b.len() < 20 || *b.last()? != b'Z' {
        return None;
    }
    let num = |s: &str| -> Option<u32> {
        (!s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let (date, rest) = text.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, day) = (d.next()?, d.next()?, d.next()?);
    if d.next().is_some() || y.len() != 4 || m.len() != 2 || day.len() != 2 {
        return None;
    }
    let time = rest.strip_suffix('Z')?;
    let (hms, frac) = match time.split_once('.') {
        Some((h, f)) if (1..=9).contains(&f.len()) => (h, f),
        Some(_) => return None,
        None => (time, ""),
    };
    let mut t = hms.split(':');
    let (hh, mm, ss) = (t.next()?, t.next()?, t.next()?);
    if t.next().is_some() || hh.len() != 2 || mm.len() != 2 || ss.len() != 2 {
        return None;
    }
    let nanos = if frac.is_empty() {
        0
    } else {
        num(&format!("{frac:0<9}"))?
    };
    let v = (
        num(y)?,
        num(m)?,
        num(day)?,
        num(hh)?,
        num(mm)?,
        num(ss)?,
        nanos,
    );
    ((1..=12).contains(&v.1) && (1..=31).contains(&v.2) && v.3 < 24 && v.4 < 60 && v.5 <= 60)
        .then_some(v)
}

/// Registry entries keep unique names; each executable is re-hashed after resolving links.
pub fn registry_status(registry: &CapabilityRegistry) -> Value {
    let mut rows = Vec::new();
    let mut names = BTreeSet::new();
    let mut ok = registry.schema == REGISTRY_SCHEMA
        && registry.validate().is_ok()
        && !registry.capabilities.is_empty();
    for cap in &registry.capabilities {
        let unique = names.insert(cap.name.clone());
        let (status, actual) = executable_status(cap);
        let valid = cap.validate().is_ok();
        ok &= unique && valid && status == "matched";
        rows.push(
            json!({"name":cap.name,"version":cap.version,"status":cap.status,"executable":status,
            "actual_sha256":actual,"contract_valid":valid,"unique":unique}),
        );
    }
    json!({"schema":"oev2.capability-registry-status/1","status":if ok {"matched"} else {"mismatch"},
        "capabilities":rows,"scope":"registered executable bytes only; qualification records are external"})
}

fn executable_status(cap: &RegisteredCapability) -> (&'static str, Option<String>) {
    let Ok(path) = std::fs::canonicalize(&cap.executable) else {
        return ("unavailable", None);
    };
    match oe_core::fingerprint_file(&path, EXECUTABLE_LIMIT) {
        Ok(id) if id.sha256 == cap.executable_sha256 => ("matched", Some(id.sha256)),
        Ok(id) => ("changed", Some(id.sha256)),
        Err(_) => ("unavailable", None),
    }
}

pub struct Verified {
    pub result: Option<ToolResult>,
    pub check: ToolResultCheck,
    /// Evidence ids and digests this result contributes when registered.
    pub evidence: BTreeMap<String, String>,
}

fn rejected_contract(result_sha256: &str, failure: String) -> Verified {
    Verified {
        result: None,
        evidence: BTreeMap::new(),
        check: ToolResultCheck {
            schema: CHECK_SCHEMA.into(),
            status: "rejected".into(),
            result_id: "unparsed".into(),
            result_sha256: result_sha256.into(),
            project_binding: "project_mismatch".into(),
            project_revised_since_run: false,
            tool_status: "unregistered".into(),
            subject_status: "not_declared".into(),
            records_verified: 0,
            findings_observed: 0,
            findings_inferred: 0,
            findings_imported: 0,
            findings_unknown: 0,
            decision_support: "none".into(),
            failures: vec![failure],
            warnings: vec![],
            scope: CHECK_SCOPE.into(),
        },
    }
}

/// Verify one tool result against the current Project and the local registry.
pub fn verify(
    root: &SkillRoot,
    project: &Project,
    registry: &CapabilityRegistry,
    result_bytes: &[u8],
) -> Result<Verified> {
    validate(project)?;
    let result_sha256 = sha256(result_bytes);
    let value = match oe_core::json::strict_json(result_bytes) {
        Ok(v) => v,
        Err(e) => {
            return Ok(rejected_contract(
                &result_sha256,
                format!("tool_result_json_invalid:{e}"),
            ));
        }
    };
    let r: ToolResult = match serde_json::from_value(value) {
        Ok(r) => r,
        Err(e) => {
            return Ok(rejected_contract(
                &result_sha256,
                format!("tool_result_contract_invalid:{e}"),
            ));
        }
    };
    let mut fail = Vec::new();
    let mut warn = Vec::new();
    generated(r.validate(), &mut fail, "result");
    generated(r.project.validate(), &mut fail, "project");
    generated(r.tool.validate(), &mut fail, "tool");
    generated(r.subject.validate(), &mut fail, "subject");
    generated(r.inputs.validate(), &mut fail, "inputs");
    generated(r.execution.validate(), &mut fail, "execution");
    generated(r.coverage.validate(), &mut fail, "coverage");
    r.records
        .iter()
        .for_each(|x| generated(x.validate(), &mut fail, "record"));
    r.findings
        .iter()
        .for_each(|x| generated(x.validate(), &mut fail, "finding"));
    r.measurements
        .iter()
        .for_each(|x| generated(x.validate(), &mut fail, "measurement"));
    if r.schema != RESULT_SCHEMA {
        fail.push("tool_result_schema".into());
    }
    let id_ok = |s: &str| {
        !s.is_empty()
            && s.len() <= 96
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
    };
    if !id_ok(&r.result_id) {
        fail.push("tool_result_id_invalid".into());
    }

    // Project binding: object revision, condition content and source must be unchanged.
    let p = &r.project;
    let binding =
        if p.schema != "oev2.project-review-context/1" || p.project_id != project.project_id {
            "project_mismatch"
        } else if project.objects.get(&p.object_id) != Some(&p.object_revision) {
            "object_changed"
        } else if project
            .conditions
            .get(&p.condition_scope)
            .map(|c| sha256(c.as_bytes()))
            != Some(p.condition_sha256.clone())
        {
            "condition_changed"
        } else if !project.evidence.values().any(|s| s == &p.source_sha256) {
            "source_changed"
        } else {
            "matched"
        };
    if binding != "matched" {
        fail.push(format!("project_binding:{binding}"));
    }
    let mut revised = false;
    if binding == "matched" {
        let identity = oe_contracts::ObjectIdentity {
            object_id: p.object_id.clone(),
            revision: p.object_revision.clone(),
            condition_scope: p.condition_scope.clone(),
            source_sha256: p.source_sha256.clone(),
        };
        let now = super::context(project, &identity)?;
        revised =
            now.project_sha256 != p.project_sha256 || now.project_revision != p.project_revision;
        if revised {
            warn.push("project_revised_since_run".into());
        }
    }
    let decisions: BTreeSet<&str> = project.decisions.iter().map(|d| d.id.as_str()).collect();
    for d in &r.decisions {
        if !decisions.contains(d.as_str()) {
            fail.push(format!("decision_unknown:{d}"));
        }
    }

    // Tool identity against the registry and the current executable bytes.
    let tool_status = match registry.capabilities.iter().find(|c| c.name == r.tool.name) {
        None => "unregistered",
        Some(cap)
            if cap.version != r.tool.version
                || cap.executable_sha256 != r.tool.executable_sha256 =>
        {
            "version_mismatch"
        }
        Some(cap) => match executable_status(cap).0 {
            "matched" => match cap.status.as_str() {
                "qualified" => "qualified",
                "registered_unqualified" => "registered_unqualified",
                _ => "retired",
            },
            "changed" => "executable_changed",
            _ => "executable_unavailable",
        },
    };
    match tool_status {
        "qualified" => {}
        "registered_unqualified" => warn.push("tool_registered_unqualified".into()),
        other => fail.push(format!("tool:{other}")),
    }
    if registry.schema != REGISTRY_SCHEMA {
        fail.push("registry_schema".into());
    }

    // Original bytes: subject manifest, input files and records.
    let mut evidence = BTreeMap::new();
    let mut total = 0usize;
    let mut read = |a: &ArtifactReference, what: &str, fail: &mut Vec<String>| -> Option<Vec<u8>> {
        match read_reference(root, a, RECORD_LIMIT) {
            Ok(b) => {
                total = total.saturating_add(b.len());
                if total > TOTAL_LIMIT {
                    fail.push("tool_result_total_bytes".into());
                }
                Some(b)
            }
            Err(e) => {
                fail.push(format!("{what}:{e}"));
                None
            }
        }
    };
    let subject_status = match &r.subject.manifest {
        None => {
            if r.subject.kind != "none" {
                warn.push("subject_manifest_not_declared".into());
            }
            "not_declared"
        }
        Some(m) => match read(m, "subject_manifest", &mut fail) {
            None => "manifest_unavailable",
            Some(bytes) => {
                evidence.insert(format!("tool-subject:{}", r.result_id), m.sha256.clone());
                manifest_status(&bytes)
            }
        },
    };
    if subject_status == "manifest_changed" {
        warn.push("subject_changed_since_run".into());
    }
    for (i, f) in r.inputs.files.iter().enumerate() {
        if read(f, &format!("input_file:{i}"), &mut fail).is_some() {
            evidence.insert(format!("tool-input:{}:{i}", r.result_id), f.sha256.clone());
        }
    }
    for s in [&r.inputs.environment_sha256, &r.inputs.input_set_sha256]
        .into_iter()
        .flatten()
    {
        if s.len() != 64
            || !s
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            fail.push("inputs_digest_invalid".into());
        }
    }
    if r.records.is_empty() {
        fail.push("records_missing".into());
    }
    let mut records = BTreeSet::new();
    let mut verified = 0u64;
    for rec in &r.records {
        if !id_ok(&rec.record_id) || !records.insert(rec.record_id.clone()) {
            fail.push(format!("record_id_invalid_or_duplicate:{}", rec.record_id));
        }
        // A registered memory handoff cannot re-enter the Project as a tool's raw record.
        if project
            .evidence
            .iter()
            .any(|(k, v)| super::memory::is_memory_evidence(k) && *v == rec.artifact.sha256)
        {
            fail.push(format!("record_is_memory_candidate:{}", rec.record_id));
        }
        if read(
            &rec.artifact,
            &format!("record:{}", rec.record_id),
            &mut fail,
        )
        .is_some()
        {
            verified += 1;
            evidence.insert(
                format!("tool-record:{}:{}", r.result_id, rec.record_id),
                rec.artifact.sha256.clone(),
            );
        }
    }

    // Findings: observed and imported cite records; inferred cites records or earlier findings.
    let mut findings = BTreeSet::new();
    let (mut obs, mut inf, mut imp, mut unk) = (0u64, 0u64, 0u64, 0u64);
    for f in &r.findings {
        if !id_ok(&f.finding_id)
            || records.contains(&f.finding_id)
            || findings.contains(&f.finding_id)
        {
            fail.push(format!("finding_id_invalid_or_duplicate:{}", f.finding_id));
        }
        let cites_records = !f.basis.is_empty() && f.basis.iter().all(|b| records.contains(b));
        let cites_any = !f.basis.is_empty()
            && f.basis
                .iter()
                .all(|b| records.contains(b) || findings.contains(b));
        match f.kind.as_str() {
            "observed" => {
                obs += 1;
                if !cites_records {
                    fail.push(format!("observed_without_record_basis:{}", f.finding_id));
                }
            }
            "imported" => {
                imp += 1;
                if !cites_records {
                    fail.push(format!("imported_without_record_basis:{}", f.finding_id));
                }
            }
            "inferred" => {
                inf += 1;
                if !cites_any {
                    fail.push(format!("inferred_without_basis:{}", f.finding_id));
                }
            }
            _ => unk += 1,
        }
        findings.insert(f.finding_id.clone());
    }
    let mut measurements = BTreeSet::new();
    for m in &r.measurements {
        if !id_ok(&m.measurement_id) || !measurements.insert(m.measurement_id.clone()) {
            fail.push(format!(
                "measurement_id_invalid_or_duplicate:{}",
                m.measurement_id
            ));
        }
        if !records.contains(&m.record_id) {
            fail.push(format!("measurement_without_record:{}", m.measurement_id));
        }
        if !m.value.is_finite() || m.samples == 0 {
            fail.push(format!("measurement_invalid:{}", m.measurement_id));
        }
    }
    if !r.measurements.is_empty() && r.behavior == "not_applicable" {
        warn.push("measurements_without_behavior_check".into());
    }
    if r.coverage.covered.iter().all(|s| s.trim().is_empty()) {
        fail.push("coverage_missing".into());
    }
    if r.unknowns.is_empty() {
        warn.push("no_unknowns_declared".into());
    }
    match (utc(&r.execution.started_at), utc(&r.execution.finished_at)) {
        (Some(a), Some(b)) if a <= b => {}
        (Some(_), Some(_)) => fail.push("execution_time_order".into()),
        _ => fail.push("execution_time_format".into()),
    }
    if r.execution.status == "failed"
        && r.execution
            .failure
            .as_deref()
            .is_none_or(|s| s.trim().is_empty())
    {
        fail.push("failed_without_failure_text".into());
    }
    if r.execution.cleanup.trim().is_empty() {
        fail.push("cleanup_not_stated".into());
    }

    let accepted = fail.is_empty();
    if accepted {
        evidence.insert(
            format!("tool-result:{}", r.result_id),
            result_sha256.clone(),
        );
    }
    let support = accepted
        && obs > 0
        && matches!(r.execution.status.as_str(), "completed" | "partial")
        && tool_status == "qualified"
        && matches!(subject_status, "manifest_matched" | "not_declared");
    let check = ToolResultCheck {
        schema: CHECK_SCHEMA.into(),
        status: if accepted { "accepted" } else { "rejected" }.into(),
        result_id: r.result_id.clone(),
        result_sha256,
        project_binding: binding.into(),
        project_revised_since_run: revised,
        tool_status: tool_status.into(),
        subject_status: subject_status.into(),
        records_verified: verified,
        findings_observed: obs,
        findings_inferred: inf,
        findings_imported: imp,
        findings_unknown: unk,
        decision_support: if support { "observed_findings" } else { "none" }.into(),
        failures: fail,
        warnings: warn,
        scope: CHECK_SCOPE.into(),
    };
    check
        .validate()
        .map_err(|f| anyhow::anyhow!("tool_result_check_invalid:{}", f.join(",")))?;
    Ok(Verified {
        result: Some(r),
        check,
        evidence,
    })
}

/// `oev2.subject-manifest/1`: `{"schema":…,"files":[{"path":abs,"sha256":…}]}` re-hashed now.
fn manifest_status(bytes: &[u8]) -> &'static str {
    let Ok(v) = oe_core::json::strict_json(bytes) else {
        return "manifest_unavailable";
    };
    let Some(files) = v["files"].as_array() else {
        return "manifest_unavailable";
    };
    if v["schema"] != MANIFEST_SCHEMA || files.is_empty() || files.len() > MANIFEST_FILES {
        return "manifest_unavailable";
    }
    for f in files {
        let (Some(path), Some(want)) = (f["path"].as_str(), f["sha256"].as_str()) else {
            return "manifest_unavailable";
        };
        let p = Path::new(path);
        if !p.is_absolute() {
            return "manifest_unavailable";
        }
        let Ok(p) = std::fs::canonicalize(p) else {
            return "manifest_changed";
        };
        match oe_core::fingerprint_file(&p, EXECUTABLE_LIMIT) {
            Ok(id) if id.sha256 == want => {}
            _ => return "manifest_changed",
        }
    }
    "manifest_matched"
}

/// Register an accepted result: new Project copy with evidence and decision dependencies.
pub fn register(
    root: &SkillRoot,
    project_bytes: &[u8],
    registry: &CapabilityRegistry,
    result_bytes: &[u8],
    revision: &str,
    output: &str,
) -> Result<(Option<Vec<u8>>, ToolResultRegistration)> {
    let project: Project = serde_json::from_value(oe_core::json::strict_json(project_bytes)?)?;
    let previous = sha256(project_bytes);
    let v = verify(root, &project, registry, result_bytes)?;
    let refused = |check: ToolResultCheck, reason: Option<String>| {
        let mut check = check;
        if let Some(r) = reason {
            check.status = "rejected".into();
            check.failures.push(r);
        }
        ToolResultRegistration {
            schema: REGISTRATION_SCHEMA.into(),
            status: "refused".into(),
            check,
            previous_project_sha256: previous.clone(),
            project_revision: project.revision.clone(),
            project_output: output.into(),
            project_sha256: previous.clone(),
            evidence_added: vec![],
            dependencies_added: vec![],
            affected_decisions: vec![],
            scope: REGISTRATION_SCOPE.into(),
        }
    };
    if v.check.status != "accepted" {
        return Ok((None, refused(v.check, None)));
    }
    let r = v.result.context("tool_result_missing_after_accept")?;
    if revision.trim().is_empty() || revision == project.revision {
        return Ok((
            None,
            refused(v.check, Some("project_revision_not_new".into())),
        ));
    }
    let mut next = project.clone();
    next.revision = revision.into();
    let mut added = Vec::new();
    for (id, sha) in &v.evidence {
        match next.evidence.get(id) {
            Some(old) if old == sha => {}
            Some(_) => {
                return Ok((
                    None,
                    refused(v.check, Some(format!("evidence_id_conflict:{id}"))),
                ));
            }
            None => {
                next.evidence.insert(id.clone(), sha.clone());
                added.push(id.clone());
            }
        }
    }
    let anchor = format!("tool-result:{}", r.result_id);
    let mut deps = Vec::new();
    for d in &r.decisions {
        if next
            .dependencies
            .entry(d.clone())
            .or_default()
            .insert(anchor.clone())
        {
            deps.push(format!("{d}<-{anchor}"));
        }
    }
    validate(&next)?;
    let affected = impact(&next, &BTreeSet::from([anchor.clone()]))?;
    let affected_decisions: Vec<String> = affected["decisions"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|d| d["decision_id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut bytes = serde_json::to_vec_pretty(&next)?;
    bytes.push(b'\n');
    let reg = ToolResultRegistration {
        schema: REGISTRATION_SCHEMA.into(),
        status: "registered".into(),
        check: v.check,
        previous_project_sha256: previous,
        project_revision: revision.into(),
        project_output: output.into(),
        project_sha256: sha256(&bytes),
        evidence_added: added,
        dependencies_added: deps,
        affected_decisions,
        scope: REGISTRATION_SCOPE.into(),
    };
    reg.validate()
        .map_err(|f| anyhow::anyhow!("tool_result_registration_invalid:{}", f.join(",")))?;
    Ok((Some(bytes), reg))
}

/// Baseline-first comparison of two registered results.
pub fn compare(
    root: &SkillRoot,
    project: &Project,
    request: &ToolComparisonRequest,
) -> Result<ToolComparisonReport> {
    validate(project)?;
    let mut reasons = Vec::new();
    let mut verdict = "invalid";
    let (mut bv, mut cv, mut rel) = (None, None, None);
    let mut contract = Vec::new();
    generated(request.validate(), &mut contract, "comparison");
    reasons.extend(contract);
    if request.schema != COMPARISON_SCHEMA {
        reasons.push("comparison_schema".into());
    }
    if !(0.0..1.0).contains(&request.minimum_improvement) || request.minimum_samples == 0 {
        reasons.push("comparison_thresholds_invalid".into());
    }
    let load =
        |a: &ArtifactReference, side: &str, reasons: &mut Vec<String>| -> Option<ToolResult> {
            let bytes = match read_reference(root, a, RECORD_LIMIT) {
                Ok(b) => b,
                Err(e) => {
                    reasons.push(format!("{side}_unreadable:{e}"));
                    return None;
                }
            };
            if !project.evidence.values().any(|s| *s == a.sha256) {
                reasons.push(format!("{side}_not_registered_in_project"));
                return None;
            }
            match oe_core::json::strict_json(&bytes)
                .ok()
                .and_then(|v| serde_json::from_value::<ToolResult>(v).ok())
            {
                Some(r) => Some(r),
                None => {
                    reasons.push(format!("{side}_contract_invalid"));
                    None
                }
            }
        };
    let base = load(&request.baseline, "baseline", &mut reasons);
    let cand = load(&request.candidate, "candidate", &mut reasons);
    if let (Some(b), Some(c)) = (&base, &cand) {
        if b.tool.name != c.tool.name || b.tool.version != c.tool.version {
            reasons.push("different_tool".into());
        }
        let pick = |r: &ToolResult| -> Option<(f64, u64)> {
            let m: Vec<_> = r
                .measurements
                .iter()
                .filter(|m| m.metric == request.metric && m.unit == request.unit)
                .collect();
            (m.len() == 1).then(|| (m[0].value, m[0].samples))
        };
        let (pb, pc) = (pick(b), pick(c));
        if pb.is_none() || pc.is_none() {
            reasons.push("metric_unit_not_exactly_once".into());
        }
        let same = |x: &Option<String>, y: &Option<String>| x.is_some() && x == y;
        if !same(&b.inputs.environment_sha256, &c.inputs.environment_sha256) {
            reasons.push("environment_not_same".into());
        }
        if !same(&b.inputs.input_set_sha256, &c.inputs.input_set_sha256) {
            reasons.push("input_set_not_same".into());
        }
        match (utc(&b.execution.finished_at), utc(&c.execution.started_at)) {
            (Some(x), Some(y)) if x < y => {}
            _ => reasons.push("baseline_not_first".into()),
        }
        if b.project.project_id != project.project_id || c.project.project_id != project.project_id
        {
            reasons.push("project_mismatch".into());
        }
        if reasons.is_empty() {
            let ((b0, bs), (c0, cs)) = (pb.unwrap(), pc.unwrap());
            bv = Some(b0);
            cv = Some(c0);
            if b0 == 0.0 {
                reasons.push("zero_baseline".into());
            } else {
                let r = match request.direction.as_str() {
                    "lower_is_better" => (b0 - c0) / b0.abs(),
                    _ => (c0 - b0) / b0.abs(),
                };
                rel = r.is_finite().then_some(r);
                if b.behavior != "passed" || c.behavior != "passed" {
                    reasons.push("behavior_not_passed_on_both".into());
                    verdict = "inconclusive";
                } else if bs < request.minimum_samples || cs < request.minimum_samples {
                    reasons.push("samples_below_minimum".into());
                    verdict = "inconclusive";
                } else if r >= request.minimum_improvement {
                    verdict = "improved";
                } else {
                    verdict = "not_improved";
                }
            }
        }
    }
    let report = ToolComparisonReport {
        schema: COMPARISON_REPORT_SCHEMA.into(),
        comparison_id: request.comparison_id.clone(),
        verdict: verdict.into(),
        baseline_value: bv,
        candidate_value: cv,
        relative_improvement: rel,
        reasons,
        scope: COMPARISON_SCOPE.into(),
    };
    report
        .validate()
        .map_err(|f| anyhow::anyhow!("tool_comparison_report_invalid:{}", f.join(",")))?;
    Ok(report)
}
