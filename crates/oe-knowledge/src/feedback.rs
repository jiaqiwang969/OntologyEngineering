//! Append-only scoped project decisions and explicit historical observations.
use crate::{
    context::{
        self, digest, exact, get_pointer, hash_string, object, owner, project_directory,
        read_project, relative_project, text,
    },
    packet,
    poppler::PopplerTools,
};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use oe_core::json::strict_json;
use oe_core::{SkillRoot, create_new_private_dir, no_symlinks, sha256, write_new};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

pub const SCHEMA: &str = "ontology-engineering.supplier-knowledge-feedback/v1";
static IDS: AtomicU64 = AtomicU64::new(0);
fn required<'a>(v: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    text(
        v.get(key).context("knowledge_feedback_missing_field")?,
        max,
        "knowledge_feedback_invalid_text",
    )
}
fn same_owner(path: &str, owner: &str, code: &str) -> Result<()> {
    ensure!(project_directory(path)? == owner, "{code}");
    Ok(())
}
fn read_json(root: &SkillRoot, path: &str) -> Result<(Vec<u8>, Value)> {
    let raw = read_project(root, path)?;
    let value = strict_json(&raw).context("knowledge_context_source_json_invalid")?;
    Ok((raw, value))
}
fn verify_prior(prior: &Value, current: &Value) -> Result<()> {
    let mut identity = object(prior, "knowledge_feedback_previous_binding_mismatch")?.clone();
    identity.remove("feedback_sha256");
    ensure!(
        prior["schema"] == SCHEMA
            && prior["feedback_sha256"] == digest(&Value::Object(identity))?
            && prior["project"]["id"] == current["project"]["id"]
            && prior["object"]["id"] == current["object"]["id"]
            && prior["decision_id"] == current["decision_id"],
        "knowledge_feedback_previous_binding_mismatch"
    );
    ensure!(
        prior["feedback_id"].as_str().is_some_and(|s| !s.is_empty())
            && ["adopted", "deferred", "rejected"]
                .contains(&prior["disposition"].as_str().unwrap_or_default()),
        "knowledge_feedback_previous_binding_mismatch"
    );
    ensure!(
        prior["packet_adoption"] == "pending_agent_review"
            && prior["engineering_verdict"] == "not_run"
            && prior["semantic_review"] == "not_run"
            && prior["ontology_promotion"] == "not_run",
        "knowledge_feedback_unreviewed_assertion"
    );
    for key in ["reason", "next_check"] {
        ensure!(
            !required(prior, key, 3000)?.trim().is_empty(),
            "knowledge_feedback_reason_and_next_check_required"
        );
    }
    let evidence = prior["result_evidence"]
        .as_array()
        .context("knowledge_feedback_result_evidence_invalid")?;
    ensure!(
        prior["result_evidence_status"]
            == if evidence.is_empty() {
                "not_yet_provided"
            } else {
                "referenced_not_evaluated"
            },
        "knowledge_feedback_unreviewed_assertion"
    );
    Ok(())
}
fn evidence(root: &SkillRoot, references: &Value, owner: &str) -> Result<Value> {
    let a = references
        .as_array()
        .context("knowledge_feedback_result_evidence_invalid")?;
    ensure!(a.len() <= 16, "knowledge_feedback_result_evidence_invalid");
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for r in a {
        let r = text(r, 1600, "knowledge_feedback_result_evidence_invalid")?;
        let (path, ptr) = match r.split_once('#') {
            Some((p, s)) => (p, Some(s)),
            None => (r, None),
        };
        let rel = relative_project(root, path)?;
        same_owner(
            &rel,
            owner,
            "knowledge_feedback_result_evidence_other_project",
        )?;
        ensure!(
            seen.insert((rel.clone(), ptr)),
            "knowledge_feedback_duplicate_result_evidence"
        );
        let raw = read_project(root, &rel)?;
        let mut item = json!({"path":rel,"sha256":sha256(&raw)});
        if let Some(ptr) = ptr {
            item["pointer"] = json!(ptr);
            item["value_sha256"] = json!(digest(get_pointer(
                &strict_json(&raw).context("knowledge_context_source_json_invalid")?,
                ptr
            )?)?);
        }
        out.push(item);
    }
    Ok(json!(out))
}
fn candidate_reference(candidate: &Value, ids: Option<&Value>) -> Result<Value> {
    let units = candidate["knowledge_units"]
        .as_array()
        .context("knowledge_feedback_unit_missing_or_ambiguous")?;
    let mut names = BTreeSet::new();
    for unit in units {
        let id = unit["id"]
            .as_str()
            .context("knowledge_feedback_unit_missing_or_ambiguous")?;
        ensure!(
            names.insert(id),
            "knowledge_feedback_unit_missing_or_ambiguous"
        );
    }
    let selection = if let Some(ids) = ids {
        let items = ids
            .as_array()
            .context("knowledge_feedback_unit_missing_or_ambiguous")?;
        ensure!(
            !items.is_empty(),
            "knowledge_feedback_unit_missing_or_ambiguous"
        );
        let mut selected = BTreeSet::new();
        for id in items {
            let id = id
                .as_str()
                .context("knowledge_feedback_unit_missing_or_ambiguous")?;
            ensure!(
                names.contains(id) && selected.insert(id),
                "knowledge_feedback_unit_missing_or_ambiguous"
            );
        }
        Some(selected)
    } else {
        None
    };
    let units = units
        .iter()
        .filter(|u| {
            selection
                .as_ref()
                .is_none_or(|s| s.contains(u["id"].as_str().unwrap_or_default()))
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"id":candidate["id"],"sha256":digest(candidate)?,"unit_ids":units.iter().map(|u|u["id"].clone()).collect::<Vec<_>>(),"source":candidate["source"],"spans":units.iter().map(|u|json!({"id":u["id"],"source_span":u["source_span"]})).collect::<Vec<_>>()}),
    )
}
fn find_candidate<'a>(packet: &'a Value, id: &str) -> Result<&'a Value> {
    let candidates = packet["evidence_candidates"]
        .as_array()
        .context("knowledge_feedback_candidate_missing_or_ambiguous")?
        .iter()
        .filter(|c| c["id"] == id)
        .collect::<Vec<_>>();
    ensure!(
        candidates.len() == 1,
        "knowledge_feedback_candidate_missing_or_ambiguous"
    );
    Ok(candidates[0])
}
fn require_current(
    root: &SkillRoot,
    packet: &Value,
    current: &Value,
    tools: &PopplerTools,
    require_current_registration: bool,
) -> Result<Value> {
    let checked = packet::verify_with_tools(root, packet, current, tools)?;
    ensure!(
        checked["status"] == "ready_for_agent_review"
            && checked["project_source_freshness"]["status"] == "current"
            && checked["packet_source_freshness"]["status"] == "current",
        "knowledge_feedback_current_bound_context_required"
    );
    ensure!(
        !require_current_registration
            || checked["source_registration_freshness"]["status"] == "current",
        "knowledge_feedback_current_source_registration_required"
    );
    Ok(checked)
}
fn make_parent(root: &SkillRoot, path: &str, owner: &str) -> Result<()> {
    let base = root.path().join(owner);
    no_symlinks(&base)?;
    ensure!(base.is_dir(), "knowledge_feedback_project_owner_missing");
    let full = root.path().join(path);
    let parent = full.parent().context("knowledge_feedback_output_invalid")?;
    let suffix = parent
        .strip_prefix(&base)
        .context("knowledge_feedback_must_remain_in_original_project")?;
    let mut directory = base;
    for component in suffix.components() {
        directory.push(component);
        match std::fs::symlink_metadata(&directory) {
            Ok(m) => ensure!(
                m.is_dir() && !m.file_type().is_symlink(),
                "knowledge_feedback_output_parent_invalid"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Err(err) = create_new_private_dir(&directory) {
                    no_symlinks(&directory)?;
                    if !directory.is_dir() {
                        return Err(err);
                    }
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Request fields match the existing feedback CLI contract. No source records change.
pub fn append(root: &SkillRoot, request: &Value) -> Result<Value> {
    append_with_tools(root, request, &PopplerTools::default())
}

pub fn append_with_tools(root: &SkillRoot, request: &Value, tools: &PopplerTools) -> Result<Value> {
    let fields = object(request, "knowledge_feedback_request_invalid")?;
    let allowed = [
        "packet_path",
        "context",
        "candidate_id",
        "disposition",
        "reason",
        "next_check",
        "output",
        "result_evidence",
        "previous",
        "unit_ids",
        "require_current_registration",
    ];
    ensure!(
        fields.keys().all(|k| allowed.contains(&k.as_str())),
        "knowledge_feedback_request_invalid"
    );
    let require_current_registration = request
        .get("require_current_registration")
        .map(|v| {
            v.as_bool()
                .context("knowledge_feedback_registration_requirement_invalid")
        })
        .transpose()?
        .unwrap_or(false);
    let current = context::validate(&request["context"])?;
    let packet_path = relative_project(root, required(request, "packet_path", 600)?)?;
    let (raw, packet) = read_json(root, &packet_path)?;
    let verification =
        require_current(root, &packet, &current, tools, require_current_registration)?;
    let owner = owner(&current)?;
    let output = relative_project(root, required(request, "output", 600)?)?;
    for path in [&packet_path, &output] {
        same_owner(
            path,
            &owner,
            "knowledge_feedback_must_remain_in_original_project",
        )?;
    }
    let candidate = find_candidate(&packet, required(request, "candidate_id", 1000)?)?;
    let disposition = required(request, "disposition", 50)?;
    ensure!(
        ["adopted", "deferred", "rejected"].contains(&disposition),
        "knowledge_feedback_disposition_invalid"
    );
    let reason = required(request, "reason", 3000)?;
    let next = required(request, "next_check", 3000)?;
    ensure!(
        !reason.trim().is_empty() && !next.trim().is_empty(),
        "knowledge_feedback_reason_and_next_check_required"
    );
    let candidate_ref =
        candidate_reference(candidate, request.get("unit_ids").filter(|v| !v.is_null()))?;
    let packet_ref = json!({"path":packet_path,"sha256":sha256(&raw)});
    let mut previous_ref = Value::Null;
    let mut previous_changed = Vec::new();
    let mut prior_snapshot = None;
    if let Some(previous) = request.get("previous").filter(|v| !v.is_null()) {
        let path = relative_project(
            root,
            text(previous, 600, "knowledge_feedback_previous_invalid")?,
        )?;
        same_owner(&path, &owner, "knowledge_feedback_previous_other_project")?;
        let (bytes, prior) = read_json(root, &path)?;
        verify_prior(&prior, &current)?;
        previous_ref =
            json!({"path":path,"sha256":sha256(&bytes),"feedback_id":prior["feedback_id"]});
        for (field, old, new) in [
            (
                "task_id",
                prior["task_id"].clone(),
                current["task_id"].clone(),
            ),
            (
                "context_sha256",
                prior["context_sha256"].clone(),
                json!(digest(&current)?),
            ),
            ("packet", prior["packet"].clone(), packet_ref.clone()),
            (
                "candidate",
                prior["candidate"].clone(),
                candidate_ref.clone(),
            ),
            (
                "project.revision",
                prior["project"]["revision"].clone(),
                current["project"]["revision"].clone(),
            ),
            (
                "object.revision",
                prior["object"]["revision"].clone(),
                current["object"]["revision"].clone(),
            ),
        ] {
            if old != new {
                previous_changed.push(field);
            }
        }
        prior_snapshot = Some((path, sha256(bytes)));
    }
    let refs = request
        .get("result_evidence")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or(json!([]));
    let result_evidence = evidence(root, &refs, &owner)?;
    let run = json!({"id":packet["run_id"],"search_record":packet["search_record"]});
    ensure!(
        run.as_object()
            .unwrap()
            .values()
            .any(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())),
        "knowledge_feedback_query_run_reference_required"
    );
    let now = SystemTime::now();
    let stamp = DateTime::<Utc>::from(now).to_rfc3339();
    let feedback_id = sha256(format!(
        "{}:{:?}:{}",
        std::process::id(),
        now,
        IDS.fetch_add(1, Ordering::Relaxed)
    ))[..32]
        .to_owned();
    let mut record = json!({"schema":SCHEMA,"feedback_id":feedback_id,"recorded_at":stamp,"task_id":current["task_id"],"project":current["project"],"object":current["object"],"decision_id":current["decision_id"],"context_sha256":digest(&current)?,"project_source_bindings":current["source_bindings"],"packet":packet_ref,"run":run,"candidate":candidate_ref,"disposition":disposition,"reason":reason,"next_check":next,"result_evidence":result_evidence,"result_evidence_status":if result_evidence.as_array().is_some_and(|a|!a.is_empty()){"referenced_not_evaluated"}else{"not_yet_provided"},"previous_feedback":previous_ref,"previous_binding_changed":!previous_changed.is_empty(),"previous_changed_fields":previous_changed,"query_status":packet["status"],"packet_adoption":"pending_agent_review","engineering_verdict":"not_run","semantic_review":"not_run","ontology_promotion":"not_run","meaning":"Scoped agent work-decision feedback only. Adopted does not accept a source assertion as a Semantica fact, establish engineering success, or authorize execution or promotion."});
    record["source_registration_freshness"] = verification["source_registration_freshness"].clone();
    record["require_current_registration"] = json!(require_current_registration);
    record["feedback_sha256"] = json!(digest(&record)?);
    ensure!(
        sha256(read_project(root, &packet_path)?) == packet_ref["sha256"],
        "knowledge_feedback_binding_changed_during_recording"
    );
    let final_verification =
        require_current(root, &packet, &current, tools, require_current_registration)?;
    ensure!(
        final_verification["source_registration_freshness"]
            == record["source_registration_freshness"],
        "knowledge_feedback_registration_changed_during_recording"
    );
    ensure!(
        evidence(root, &refs, &owner)? == result_evidence,
        "knowledge_feedback_result_evidence_changed_during_recording"
    );
    if let Some((path, sha)) = prior_snapshot {
        ensure!(
            sha256(read_project(root, &path)?) == sha,
            "knowledge_feedback_previous_changed_during_recording"
        );
    }
    make_parent(root, &output, &owner)?;
    write_new(&root.path().join(&output), &record)?;
    Ok(record)
}

/// Explicit, hash-pinned history. The returned model observation contains no paths.
/// Prior project records may be stale after a revision; they never become current facts.
pub fn load_reference(root: &SkillRoot, reference: &Value, current: &Value) -> Result<Value> {
    load_reference_with_tools(root, reference, current, &PopplerTools::default())
}

pub fn load_reference_with_tools(
    root: &SkillRoot,
    reference: &Value,
    current: &Value,
    tools: &PopplerTools,
) -> Result<Value> {
    exact(
        reference,
        &["path", "sha256"],
        "knowledge_feedback_reference_invalid",
    )?;
    ensure!(
        hash_string(&reference["sha256"]),
        "knowledge_feedback_reference_invalid"
    );
    let current = context::validate(current)?;
    for name in ["project", "object"] {
        for field in ["id", "revision"] {
            ensure!(
                current[name][field]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty()),
                "knowledge_feedback_current_bound_context_required"
            );
        }
    }
    ensure!(
        current["decision_id"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty()),
        "knowledge_feedback_current_bound_context_required"
    );
    ensure!(
        context::verify_sources(root, &current)?["status"] == "current",
        "knowledge_feedback_current_bound_context_required"
    );
    let owner = owner(&current)?;
    let path = relative_project(
        root,
        text(
            &reference["path"],
            600,
            "knowledge_feedback_reference_invalid",
        )?,
    )?;
    same_owner(&path, &owner, "knowledge_feedback_previous_other_project")?;
    let (raw, record) = read_json(root, &path)?;
    ensure!(
        sha256(&raw) == reference["sha256"],
        "knowledge_feedback_reference_hash_mismatch"
    );
    verify_prior(&record, &current)?;
    let packet_ref = &record["packet"];
    exact(
        packet_ref,
        &["path", "sha256"],
        "knowledge_feedback_packet_reference_invalid",
    )?;
    let packet_path = relative_project(
        root,
        text(
            &packet_ref["path"],
            600,
            "knowledge_feedback_packet_reference_invalid",
        )?,
    )?;
    same_owner(
        &packet_path,
        &owner,
        "knowledge_feedback_must_remain_in_original_project",
    )?;
    let (packet_raw, packet) = read_json(root, &packet_path)?;
    ensure!(
        sha256(&packet_raw) == packet_ref["sha256"],
        "knowledge_feedback_packet_hash_mismatch"
    );
    let snapshot = context::validate(&packet["context_snapshot"])?;
    let verification = packet::verify_with_tools(root, &packet, &snapshot, tools)?;
    ensure!(
        record["context_sha256"] == digest(&snapshot)?
            && record["project_source_bindings"] == snapshot["source_bindings"],
        "knowledge_feedback_previous_binding_mismatch"
    );
    for key in ["task_id", "project", "object", "decision_id"] {
        ensure!(
            record[key] == snapshot[key],
            "knowledge_feedback_previous_binding_mismatch"
        );
    }
    ensure!(
        record["run"] == json!({"id":packet["run_id"],"search_record":packet["search_record"]})
            && record["query_status"] == packet["status"],
        "knowledge_feedback_previous_binding_mismatch"
    );
    let candidate = find_candidate(&packet, required(&record["candidate"], "id", 1000)?)?;
    let units = &record["candidate"]["unit_ids"];
    let ids = if units.as_array().is_some_and(Vec::is_empty)
        && candidate["knowledge_units"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        None
    } else {
        Some(units)
    };
    ensure!(
        candidate_reference(candidate, ids)? == record["candidate"],
        "knowledge_feedback_candidate_binding_mismatch"
    );
    let mut refs = Vec::new();
    for item in record["result_evidence"]
        .as_array()
        .context("knowledge_feedback_result_evidence_invalid")?
    {
        let item_path = text(
            &item["path"],
            600,
            "knowledge_feedback_result_evidence_invalid",
        )?;
        let ref_string = if let Some(ptr) = item.get("pointer") {
            format!(
                "{item_path}#{}",
                text(ptr, 1000, "knowledge_context_pointer_invalid")?
            )
        } else {
            item_path.to_owned()
        };
        refs.push(json!(ref_string));
    }
    ensure!(
        evidence(root, &json!(refs), &owner)? == record["result_evidence"],
        "knowledge_feedback_result_evidence_changed"
    );
    if !record["previous_feedback"].is_null() {
        let previous = &record["previous_feedback"];
        let previous_path = relative_project(
            root,
            text(
                &previous["path"],
                600,
                "knowledge_feedback_previous_invalid",
            )?,
        )?;
        same_owner(
            &previous_path,
            &owner,
            "knowledge_feedback_previous_other_project",
        )?;
        let (bytes, prior) = read_json(root, &previous_path)?;
        ensure!(
            sha256(bytes) == previous["sha256"] && prior["feedback_id"] == previous["feedback_id"],
            "knowledge_feedback_previous_binding_mismatch"
        );
        verify_prior(&prior, &current)?;
    }
    ensure!(
        sha256(read_project(root, &path)?) == reference["sha256"]
            && sha256(read_project(root, &packet_path)?) == packet_ref["sha256"]
            && context::verify_sources(root, &current)?["status"] == "current",
        "knowledge_feedback_binding_changed_during_reading"
    );
    let changed = ["task_id", "project", "object", "decision_id"]
        .into_iter()
        .filter(|k| record[*k] != current[*k])
        .collect::<Vec<_>>();
    let observation = json!({"kind":"historical_agent_work_decision","feedback_id":record["feedback_id"],"feedback_sha256":record["feedback_sha256"],"record_sha256":reference["sha256"],"project":record["project"],"object":record["object"],"decision_id":record["decision_id"],"source_context_sha256":record["context_sha256"],"current_context_sha256":digest(&current)?,"disposition":record["disposition"],"reason":record["reason"],"next_check":record["next_check"],"result_evidence_status":record["result_evidence_status"],"historical_only":true,"engineering_verdict":"not_run","semantic_review":"not_run","ontology_promotion":"not_run","meaning":"Historical work decision under its original context. No prior acceptance or applicability is inherited by this turn."});
    Ok(
        json!({"schema":"ontology-engineering.supplier-feedback-reference/v1","status":"history_verified","reference":{"path":path,"sha256":reference["sha256"]},"feedback_id":record["feedback_id"],"historical_context_source_freshness":verification["packet_source_freshness"],"current_context_sha256":digest(&current)?,"changed_identity_fields":changed,"model_observation":observation,"adoption":"pending_agent_review","semantic_review":"not_run","ontology_promotion":"not_run"}),
    )
}
