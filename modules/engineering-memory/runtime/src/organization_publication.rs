//! Explicit host publication of a closed organization's current conclusions as
//! one experience revision. Closing never writes records; only this action does,
//! after re-checking the organization snapshot, the cited findings and anchors.
use super::*;
use crate::contracts::{ExperiencePublicationRequest, ExperienceRecord};
use crate::store::{
    Store, contains_credential, contains_host_injection, unique, validate_experience,
};

/// Largest single hook candidate one publication may add to the daily context.
pub const BRIEF_MAX_BYTES: u64 = 6144;

fn publication_dir(dir: &Path, publication: &str) -> Result<PathBuf> {
    Ok(dir.join("publications").join(id(publication)?))
}

/// Global reservation binding one record revision to exactly one publication.
fn reservation_path(home: &Path, record_id: &str, revision: u64) -> Result<PathBuf> {
    Ok(home
        .join("experience-publications")
        .join(id(record_id)?)
        .join(format!("r{revision}.json")))
}

/// Whole-token match: identifier characters ([A-Za-z0-9_.-]) may not touch the
/// token on either side, except one trailing sentence period.
pub(super) fn mentions(text: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let idc = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.');
    text.match_indices(token).any(|(i, _)| {
        let before = text[..i].chars().next_back().is_none_or(|c| !idc(c));
        let mut after = text[i + token.len()..].chars();
        let after = match after.next() {
            None => true,
            Some('.') => after.next().is_none_or(|c| !idc(c)),
            Some(c) => !idc(c),
        };
        before && after
    })
}

/// Every host-authored or quoted text a publication carries into recall.
fn record_texts(r: &ExperienceRecord) -> Vec<&str> {
    let mut texts = vec![r.purpose.as_str()];
    texts.extend(r.conditions.iter().map(String::as_str));
    texts.extend(r.unknowns.iter().map(String::as_str));
    texts.extend(r.timeline.iter().map(|e| e.description.as_str()));
    for claim in &r.causal_claims {
        texts.push(&claim.statement);
        texts.extend(claim.confounders.iter().map(String::as_str));
        texts.extend(claim.unknowns.iter().map(String::as_str));
    }
    if let Some(m) = &r.method {
        texts.extend([
            m.purpose.as_str(),
            m.mechanism.as_str(),
            m.validation.as_str(),
        ]);
    }
    texts.extend(r.sources.iter().map(|s| s.text.as_str()));
    texts
}

/// Request-only checks; nothing is locked or written. Returns hook brief bytes.
fn check_request(store: &Store, req: &ExperiencePublicationRequest) -> Result<u64> {
    shape(req)?;
    id(&req.id)?;
    id(&req.source.id)?;
    ensure!(
        req.source.kind == "organization",
        "publication_source_kind_not_supported:{}",
        req.source.kind
    );
    ensure!(
        !req.source.cited_ids.is_empty(),
        "publication_cited_ids_required"
    );
    for cited in &req.source.cited_ids {
        id(cited)?;
    }
    unique(&req.source.cited_ids, "publication_cited_id")?;
    let record = &req.record;
    validate_experience(record)?;
    ensure!(
        matches!(record.status.as_str(), "draft" | "reviewed"),
        "publication_status_not_publishable"
    );
    ensure!(
        record.sources.iter().any(|s| s.role == "user")
            && record.sources.iter().any(|s| s.role == "assistant"),
        "publication_requires_user_goal_and_assistant_report_anchors"
    );
    ensure!(
        !record_texts(record)
            .into_iter()
            .chain([req.summary.as_str()])
            .any(contains_host_injection),
        "publication_contains_host_injection_requires_review"
    );
    ensure!(
        !contains_credential(&req.summary),
        "publication_contains_credential"
    );
    ensure!(
        record
            .timeline
            .iter()
            .any(|e| mentions(&e.description, &req.source.id)),
        "publication_record_must_name_organization"
    );
    artifact(&req.review, false)?;
    // Re-reads every anchor exactly as recall and the daily hook will.
    let bytes = store.hook_brief_bytes(record)?;
    ensure!(
        bytes <= BRIEF_MAX_BYTES,
        "publication_brief_budget_exceeded:required={bytes}:max={BRIEF_MAX_BYTES}"
    );
    Ok(bytes)
}

/// Closed snapshot, citation and disclosure rules. Returns cited and unresolved rows.
fn check_organization(
    p: &OrganizationPlan,
    s: &State,
    current: &Value,
    req: &ExperiencePublicationRequest,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let close = &current["close"];
    ensure!(!close.is_null(), "publication_requires_closed_organization");
    ensure!(
        disk::key(p)? == req.source.expected_plan_sha256,
        "publication_plan_changed"
    );
    ensure!(
        current["heads_sha256"] == req.source.expected_state_sha256,
        "publication_heads_changed"
    );
    let outcome = close["request"]["outcome"]
        .as_str()
        .context("publication_close_outcome_missing")?;
    let record = &req.record;
    ensure!(
        record.status != "reviewed" || outcome == "completed",
        "publication_reviewed_requires_completed_organization"
    );
    ensure!(
        record.project_id == p.project_id,
        "publication_project_mismatch"
    );
    let mut cited = Vec::new();
    for finding_id in &req.source.cited_ids {
        let stored = s
            .findings
            .get(finding_id)
            .with_context(|| format!("publication_finding_unknown:{finding_id}"))?;
        let claim = stored["finding"]["claim_id"]
            .as_str()
            .context("claim_id_missing")?;
        ensure!(
            s.heads.get(claim) == Some(finding_id),
            "publication_finding_not_current_head:{finding_id}"
        );
        ensure!(
            s.states.get(claim).map(String::as_str) == Some("supported"),
            "publication_finding_not_supported:{finding_id}"
        );
        cited.push(json!({"finding_id":finding_id,"claim_id":claim,
            "finding_sha256":stored["finding_sha256"]}));
    }
    let mut unresolved = Vec::new();
    for c in &p.claims {
        let state = s.states[&c.id].as_str();
        if state == "supported" {
            continue;
        }
        if c.required || state == "contradicted" {
            ensure!(
                record.unknowns.iter().any(|u| mentions(u, &c.id)),
                "publication_unknowns_missing_claim:{}",
                c.id
            );
        }
        unresolved.push(json!({"claim_id":c.id,"required":c.required,"state":state}));
    }
    if p.execution_mode == "development_replay" {
        ensure!(
            record
                .conditions
                .iter()
                .chain(&record.unknowns)
                .any(|t| mentions(t, "development_replay")),
            "publication_development_replay_undisclosed"
        );
    }
    Ok((cited, unresolved))
}

/// Ok(true) when the file exists with exactly the expected value.
fn same_or_absent(path: &Path, expected: &Value, code: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let actual: Value = disk::read_json(path)?;
    ensure!(actual == *expected, "{code}");
    Ok(true)
}

fn reservation(
    org: &str,
    publication: &str,
    intent_sha256: &str,
    r: &ExperienceRecord,
) -> Result<Value> {
    Ok(
        json!({"schema":"engineering-memory.experience-publication-reservation/1",
        "source_kind":"organization","source_id":org,"publication_id":publication,
        "intent_sha256":intent_sha256,"record_id":r.id,"revision":r.revision,
        "record_sha256":sha(serde_json::to_vec(r)?)}),
    )
}

fn visibility(r: &ExperienceRecord) -> &'static str {
    if r.project_id.is_none() {
        "unbound_candidate_review_required"
    } else {
        "activated_project_session_only"
    }
}

fn receipt(
    org: &str,
    publication: &str,
    intent_sha256: &str,
    reservation: &Value,
    r: &ExperienceRecord,
) -> Result<Value> {
    Ok(
        json!({"schema":"engineering-memory.organization-publication-receipt/1",
        "publication_id":publication,"organization_id":org,"intent_sha256":intent_sha256,
        "reservation_sha256":disk::key(reservation)?,
        "record":{"id":r.id,"revision":r.revision,"sha256":sha(serde_json::to_vec(r)?)},
        "history":format!("records/{}.json", r.id),"status":r.status,"project_id":r.project_id,
        "daily_visibility":visibility(r),"engineering_adoption":"not_granted"}),
    )
}

/// Host-invoked publication. Lock order: organization owner (fail fast), then store writer.
pub(super) fn publish(home: &Path, req: ExperiencePublicationRequest) -> Result<Value> {
    let store = Store::new(home);
    let brief_bytes = check_request(&store, &req)?;
    let dir = organization_dir(home, &req.source.id)?;
    // Checked before locking so an unknown organization creates no directory.
    ensure!(dir.join("plan.json").is_file(), "organization_missing");
    let _owner = disk::lock(&dir, "owner")?;
    let (_, p, m) = load_plan(home, &req.source.id)?;
    let s = load_state(&dir, &p)?;
    let current = status(&dir, &p, &s)?;
    let (cited, unresolved) = check_organization(&p, &s, &current, &req)?;
    let record = &req.record;
    let record_sha256 = sha(serde_json::to_vec(record)?);
    let intent = json!({
        "schema":"engineering-memory.organization-publication-intent/1",
        "request":req,"request_sha256":disk::key(&req)?,
        "organization_id":p.id,"plan_sha256":disk::key(&p)?,"heads_sha256":current["heads_sha256"],
        "close_sha256":disk::key(&current["close"])?,"outcome":current["close"]["request"]["outcome"],
        "execution_mode":p.execution_mode,
        "method":{"id":m.id,"revision":m.revision,"sha256":p.method_sha256},
        "cited":cited,"unresolved_claims":unresolved,
        "record":{"id":record.id,"revision":record.revision,"sha256":record_sha256},
        "brief_max_bytes":BRIEF_MAX_BYTES
    });
    let intent_sha256 = disk::key(&intent)?;
    let reservation = reservation(&p.id, &req.id, &intent_sha256, record)?;
    let receipt = receipt(&p.id, &req.id, &intent_sha256, &reservation, record)?;
    let folder = publication_dir(&dir, &req.id)?;
    let intent_path = folder.join("intent.json");
    let receipt_path = folder.join("receipt.json");
    let reservation_path = reservation_path(home, &record.id, record.revision)?;
    // Compare every present piece before writing any missing piece.
    let replayed = same_or_absent(&intent_path, &intent, "publication_identity_reused")?;
    let receipt_present = same_or_absent(&receipt_path, &receipt, "publication_receipt_changed")?;
    ensure!(replayed || !receipt_present, "publication_intent_missing");
    let written = {
        let _writer = store.lock()?;
        let reserved = same_or_absent(
            &reservation_path,
            &reservation,
            "publication_record_revision_reserved",
        )?;
        ensure!(replayed || !reserved, "publication_intent_missing");
        let fresh = store.preflight(record)?;
        // A revision written by an ordinary record call is not this publication.
        ensure!(fresh || replayed, "publication_record_revision_exists");
        disk::immutable(&intent_path, &intent)?;
        disk::immutable(&reservation_path, &reservation)?;
        store.record_locked(record)?["effect"] == true
    };
    disk::immutable(&receipt_path, &receipt)?;
    let event_id = disk::event(
        &dir,
        "organization_experience_published",
        json!({"publication_id":req.id,"record_id":record.id,"revision":record.revision,
            "record_sha256":record_sha256,"intent_sha256":intent_sha256}),
    )?;
    Ok(json!({
        "schema":"engineering-memory.organization-publication-result/1",
        "publication_id":req.id,"organization_id":p.id,"intent_sha256":intent_sha256,
        "receipt":receipt,"event_id":event_id,
        "record":{"id":record.id,"revision":record.revision,"sha256":record_sha256},
        "replayed":replayed,"record_written_by_this_call":written,
        "brief_bytes":brief_bytes,"brief_max_bytes":BRIEF_MAX_BYTES,
        "daily_visibility":visibility(record),
        "automatic_on_close":false,"provider_calls":0,"engineering_adoption":"not_granted",
        "next_action":"Check the record with the daily runtime (get, verify, recall); publication grants no engineering adoption."
    }))
}

/// Read-only state of one publication; shared organization lock, no writes.
pub(super) fn publication_status(
    home: &Path,
    organization: &str,
    publication: &str,
) -> Result<Value> {
    id(organization)?;
    id(publication)?;
    let dir = organization_dir(home, organization)?;
    ensure!(dir.join("plan.json").is_file(), "organization_missing");
    let lock_path = dir.join("owner.lock");
    ensure!(
        fs::symlink_metadata(&lock_path)?.file_type().is_file(),
        "organization_lock_not_regular_file"
    );
    let owner = fs::File::open(lock_path)?;
    fs2::FileExt::try_lock_shared(&owner).context("organization_owner_busy")?;
    let (_, p, _) = load_plan(home, organization)?;
    let folder = publication_dir(&dir, publication)?;
    let intent_path = folder.join("intent.json");
    ensure!(intent_path.is_file(), "publication_missing");
    let intent: Value = disk::read_json(&intent_path)?;
    let request: ExperiencePublicationRequest = serde_json::from_value(intent["request"].clone())?;
    let record = &request.record;
    let record_sha256 = sha(serde_json::to_vec(record)?);
    ensure!(
        request.id == publication
            && request.source.id == organization
            && intent["organization_id"] == organization
            && intent["request_sha256"] == disk::key(&request)?
            && intent["record"]["sha256"] == record_sha256,
        "publication_intent_changed"
    );
    let intent_sha256 = disk::key(&intent)?;
    let expected_reservation = reservation(organization, publication, &intent_sha256, record)?;
    let reservation_path = reservation_path(home, &record.id, record.revision)?;
    let reservation_state = if !reservation_path.exists() {
        "absent"
    } else if disk::read_json::<Value>(&reservation_path)? == expected_reservation {
        "present"
    } else {
        "conflict"
    };
    let expected_receipt = receipt(
        organization,
        publication,
        &intent_sha256,
        &expected_reservation,
        record,
    )?;
    let receipt_path = folder.join("receipt.json");
    let receipt_state = if !receipt_path.exists() {
        "absent"
    } else if disk::read_json::<Value>(&receipt_path)? == expected_receipt {
        "present"
    } else {
        "conflict"
    };
    let store = Store::new(home);
    let history = home.join("records").join(format!("{}.json", record.id));
    let (record_state, latest, sources) = if !history.exists() {
        ("absent", None, json!({"checked":false}))
    } else {
        match store.get(&record.id, Some(record.revision)) {
            Ok(stored) if stored == *record => {
                let latest = store.get(&record.id, None)?.revision;
                let check = store.verify_record_sources(&stored.sources);
                (
                    "matches_intent",
                    Some(latest),
                    json!({"checked":true,"matched":check.is_ok(),"error":check.err().map(|e| e.to_string())}),
                )
            }
            Ok(_) => ("conflict", None, json!({"checked":false})),
            Err(error) if error.to_string().contains("experience_revision_missing") => {
                ("absent", None, json!({"checked":false}))
            }
            Err(error) => return Err(error),
        }
    };
    let organization_check = load_state(&dir, &p).and_then(|s| status(&dir, &p, &s));
    let state = if [reservation_state, receipt_state, record_state].contains(&"conflict") {
        "record_conflict"
    } else if reservation_state == "present"
        && receipt_state == "present"
        && record_state == "matches_intent"
    {
        "published"
    } else {
        "pending_publication"
    };
    Ok(json!({
        "schema":"engineering-memory.organization-publication-status/1",
        "organization_id":organization,"publication_id":publication,"state":state,
        "intent_sha256":intent_sha256,
        "record":{"id":record.id,"revision":record.revision,"sha256":record_sha256},
        "reservation":reservation_state,"receipt":receipt_state,"record_state":record_state,
        "latest_revision":latest,"superseded":latest.is_some_and(|l| l > record.revision),
        "current_sources":sources,
        "organization_current_verified":organization_check.is_ok(),
        "organization_issue":organization_check.err().map(|e| e.to_string()),
        "read_only":true,"provider_calls":0,
        "next_action":"pending_publication: replay the identical publish request; record_conflict: inspect the other writer before any new revision."
    }))
}

#[cfg(test)]
mod tests {
    use super::mentions;

    #[test]
    fn mentions_requires_whole_identifier_tokens() {
        assert!(mentions("label", "label"));
        assert!(mentions("命题 label 未支持", "label"));
        assert!(mentions("未支持：label。", "label"));
        assert!(mentions("claims label, unit", "label"));
        assert!(mentions("ends with label.", "label"));
        assert!(mentions("(label)", "label"));
        assert!(!mentions("labels", "label"));
        assert!(!mentions("label-x", "label"));
        assert!(!mentions("x-label", "label"));
        assert!(!mentions("label.x", "label"));
        assert!(!mentions("label_1", "label"));
        assert!(!mentions("", "label"));
        assert!(!mentions("label", ""));
        assert!(mentions("organization org-1.v2 closed", "org-1.v2"));
        assert!(!mentions("organization org-1.v20", "org-1.v2"));
    }
}
