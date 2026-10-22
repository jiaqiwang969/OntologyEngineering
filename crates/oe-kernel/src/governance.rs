//! Govern immutable package identities using a separate, locally controlled authority policy.
//! Policy ownership is a local trust boundary, not a cryptographic or external publication claim.
use anyhow::{Context, Result, ensure};
use oe_contracts::PackageState;
use oe_core::{SkillRoot, read_regular_bytes, write_new_bytes};
use oe_wire::json::{canonical_json, strict_json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub candidate_sha256: String,
    pub previous_sha256: Option<String>,
    pub from: String,
    pub to: String,
    pub actor: String,
    /// An assertion from the request; it is intersected with the trusted policy.
    pub authority_scope: Vec<String>,
    pub evidence_sha256: Vec<String>,
    pub package_binding_sha256: String,
    pub event_sha256: String,
    #[serde(default)]
    pub policy_sha256: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    path: String,
    sha256: String,
    role: String,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    schema: String,
    ledger: String,
    bundle: String,
    candidate_sha256: String,
    package_binding_sha256: String,
    actor_scopes: BTreeMap<String, BTreeSet<String>>,
    required_evidence: BTreeMap<String, BTreeSet<String>>,
    release_artifacts: Vec<Artifact>,
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn event_digest(event: &Event) -> Result<String> {
    let mut v = serde_json::to_value(event)?;
    v.as_object_mut()
        .context("event_object_missing")?
        .remove("event_sha256");
    Ok(oe_core::sha256(canonical_json(&v)?))
}
fn allowed_transition(from: &str, to: &str) -> Result<()> {
    let state: PackageState =
        serde_json::from_value(json!(from)).context("governance_state_unknown")?;
    let target: PackageState =
        serde_json::from_value(json!(to)).context("governance_state_unknown")?;
    let event = match to {
        "proposed" => "Propose",
        "committed" => "Commit",
        "regression_passed" => "Verify",
        "release_complete" => "Release",
        "promoted" => "Promote",
        "published" => "Publish",
        _ => anyhow::bail!("governance_state_unknown"),
    };
    ensure!(
        state
            .transition(event)
            .is_some_and(|(next, _)| next == target),
        "governance_cannot_skip_state"
    );
    Ok(())
}
fn validate_history(events: &[Event]) -> Result<()> {
    let mut roots = BTreeSet::new();
    let mut successors = BTreeSet::new();
    for old in events {
        allowed_transition(&old.from, &old.to)?;
        ensure!(
            valid_hash(&old.policy_sha256) && old.event_sha256 == event_digest(old)?,
            "governance_history_tampered"
        );
        if let Some(parent) = &old.previous_sha256 {
            ensure!(successors.insert(parent), "governance_history_fork");
            let p = events
                .iter()
                .find(|e| &e.event_sha256 == parent)
                .context("governance_history_parent_missing")?;
            ensure!(
                p.to == old.from
                    && p.candidate_sha256 == old.candidate_sha256
                    && p.package_binding_sha256 == old.package_binding_sha256
                    && p.policy_sha256 == old.policy_sha256,
                "governance_history_baseline_changed"
            );
        } else {
            ensure!(
                old.from == "candidate" && roots.insert(&old.candidate_sha256),
                "governance_history_roots_invalid"
            );
        }
    }
    Ok(())
}
fn regression_contract(receipt: &Value) -> Result<Value> {
    // Original SELECT oracles are multisets; raw unordered row order and anonymous
    // SHACL node labels are observations, not receipt equivalence criteria.
    let mut cases = BTreeMap::new();
    for s in receipt["results"]
        .as_array()
        .context("governance_regression_results_missing")?
    {
        let id = s["scenario_id"]
            .as_str()
            .context("governance_regression_scenario_missing")?;
        let v = &s["verification"];
        let inputs=v["cases"].as_array().map(|inputs|inputs.iter().map(|i|json!({"role":i["role"],"input_id":i["input_id"],"input_sha256":i["input_sha256"],"expected":i["expected"],"regression":i["regression"]})).collect::<Vec<_>>());
        ensure!(cases.insert(id,json!({"execution":s["execution"],"regression":v["regression"],"oracle":v["oracle"],"inputs":v["inputs"],"cases":inputs})).is_none(),"governance_regression_duplicate_scenario");
    }
    Ok(
        json!({"schema":receipt["schema"],"engine_id":receipt["engine_id"],"engine_version":receipt["engine_version"],"executable_sha256":receipt["executable_sha256"],"package":receipt["package"],"status":receipt["status"],"scenario_count":receipt["scenario_count"],"passed":receipt["passed"],"failed":receipt["failed"],"unknown":receipt["unknown"],"cases":cases}),
    )
}
/// Fresh read-only eligibility gate for application-controlled rule selection.
/// A serialized `promoted` label does not authorize itself.
pub fn verified_promoted(root: &SkillRoot, bundle_name: &str) -> Result<Value> {
    verified_promoted_at(
        root,
        bundle_name,
        Path::new("var/state/governance-authority.json"),
    )
}
pub fn verified_promoted_at(
    root: &SkillRoot,
    bundle_name: &str,
    policy_path: &Path,
) -> Result<Value> {
    let policy_path = root.private_path(policy_path)?;
    let policy_bytes = read_regular_bytes(&policy_path, 1024 * 1024)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            std::fs::metadata(&policy_path)?.permissions().mode() & 0o077 == 0,
            "governance_policy_permissions_not_private"
        );
    }
    let policy: Policy = serde_json::from_value(strict_json(&policy_bytes)?)?;
    ensure!(
        policy.schema == "oev2.governance-policy/1" && policy.bundle == bundle_name,
        "governance_current_policy_bundle_mismatch"
    );
    let policy_sha = oe_core::sha256(&policy_bytes);
    let ledger = root.private_path(Path::new(&policy.ledger))?;
    let mut events = Vec::new();
    for entry in std::fs::read_dir(&ledger)? {
        let p = entry?.path();
        if p.extension().is_some_and(|e| e == "json") {
            let event: Event =
                serde_json::from_value(strict_json(&read_regular_bytes(&p, 1024 * 1024)?)?)?;
            ensure!(
                p.file_stem().and_then(|s| s.to_str()) == Some(&event.event_sha256),
                "governance_history_name_tampered"
            );
            events.push(event);
        }
    }
    validate_history(&events)?;
    for event in &events {
        ensure!(
            event.policy_sha256 == policy_sha
                && event.candidate_sha256 == policy.candidate_sha256
                && event.package_binding_sha256 == policy.package_binding_sha256
                && event.authority_scope.contains(&event.to)
                && policy
                    .actor_scopes
                    .get(&event.actor)
                    .is_some_and(|s| s.contains(&event.to)),
            "governance_current_authority_changed"
        );
        if event.to == "regression_passed" {
            let previous = events
                .iter()
                .find(|e| Some(&e.event_sha256) == event.previous_sha256.as_ref())
                .context("governance_previous_missing")?;
            ensure!(
                previous.actor != event.actor,
                "governance_independent_verifier_required"
            );
        }
    }
    let promoted = events
        .iter()
        .find(|e| e.to == "promoted")
        .context("governance_rule_not_promoted")?;
    let required = policy
        .required_evidence
        .get("promoted")
        .context("governance_required_evidence_missing")?;
    let supplied: BTreeSet<_> = promoted.evidence_sha256.iter().cloned().collect();
    ensure!(
        !required.is_empty() && required.is_subset(&supplied),
        "governance_required_evidence_missing"
    );
    let bundle = oe_semantic::load_bundle(root.path(), bundle_name)?;
    let binding = oe_core::sha256(canonical_json(
        &json!({"package_id":bundle.spec["package_id"],
        "package_version":bundle.spec["package_version"],"archive_sha256":bundle.spec["sha256"]}),
    )?);
    ensure!(
        bundle.spec["sha256"] == policy.candidate_sha256
            && binding == policy.package_binding_sha256,
        "governance_current_package_baseline_changed"
    );
    let regression = crate::packages::regress(root.path(), bundle_name)?;
    ensure!(
        regression["status"] == "passed"
            && regression["scenario_count"].as_u64().is_some_and(|n| n > 0),
        "governance_fresh_regression_not_passed"
    );
    let expected = regression_contract(&regression)?;
    let mut reproduced = false;
    for digest in &supplied {
        let raw = read_regular_bytes(
            &ledger.join("evidence").join(format!("{digest}.json")),
            crate::MAX_BYTES,
        )?;
        ensure!(
            oe_core::sha256(&raw) == *digest,
            "governance_evidence_drift"
        );
        reproduced |= regression_contract(&strict_json(&raw)?).is_ok_and(|v| v == expected);
    }
    ensure!(reproduced, "governance_regression_receipt_not_reproducible");
    let mut roles = BTreeSet::new();
    for artifact in &policy.release_artifacts {
        oe_semantic::safe_relative(&artifact.path)?;
        ensure!(
            oe_core::sha256(read_regular_bytes(
                &root.path().join(&artifact.path),
                crate::MAX_BYTES
            )?) == artifact.sha256,
            "governance_release_artifact_drift"
        );
        roles.insert(artifact.role.as_str());
    }
    ensure!(
        ["package_archive", "license", "interface", "origin"]
            .iter()
            .all(|r| roles.contains(r)),
        "governance_release_inventory_incomplete"
    );
    ensure!(
        read_regular_bytes(&policy_path, 1024 * 1024)? == policy_bytes,
        "governance_policy_changed_during_execution"
    );
    Ok(
        json!({"schema":"oev2.governed-rule-check/1","execution":"completed","bundle":bundle_name,
        "package":bundle.spec,"promoted_event":promoted.event_sha256,"policy_sha256":policy_sha,
        "fresh_regression":regression,"engineering_adoption":"not_established"}),
    )
}

pub fn transition(root: &SkillRoot, ledger: &Path, mut event: Event) -> Result<Value> {
    let ledger = root.private_path(ledger)?;
    allowed_transition(&event.from, &event.to)?;
    ensure!(
        event.to != "published",
        "external_publication_verification_not_implemented"
    );
    ensure!(
        valid_hash(&event.candidate_sha256)
            && valid_hash(&event.package_binding_sha256)
            && event.evidence_sha256.iter().all(|s| valid_hash(s)),
        "governance_digest_invalid"
    );
    let policy_path = root.path().join("var/state/governance-authority.json");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            std::fs::metadata(&policy_path)?.permissions().mode() & 0o077 == 0,
            "governance_policy_permissions_not_private"
        );
    }
    let policy_bytes = read_regular_bytes(&policy_path, 1024 * 1024)?;
    let policy: Policy = serde_json::from_value(strict_json(&policy_bytes)?)?;
    ensure!(
        policy.schema == "oev2.governance-policy/1"
            && root.private_path(Path::new(&policy.ledger))? == ledger,
        "governance_policy_scope_mismatch"
    );
    ensure!(
        policy.candidate_sha256 == event.candidate_sha256
            && policy.package_binding_sha256 == event.package_binding_sha256,
        "governance_policy_binding_mismatch"
    );
    ensure!(
        !event.actor.trim().is_empty()
            && event.authority_scope.contains(&event.to)
            && policy
                .actor_scopes
                .get(&event.actor)
                .is_some_and(|s| s.contains(&event.to)),
        "governance_authority_missing"
    );
    let required = policy
        .required_evidence
        .get(&event.to)
        .context("governance_policy_evidence_role_missing")?;
    let supplied: BTreeSet<_> = event.evidence_sha256.iter().cloned().collect();
    ensure!(
        !required.is_empty()
            && required.is_subset(&supplied)
            && supplied.len() == event.evidence_sha256.len(),
        "governance_required_evidence_missing_or_duplicate"
    );
    let bundle = oe_semantic::load_bundle(root.path(), &policy.bundle)?;
    let binding = oe_core::sha256(canonical_json(
        &json!({"package_id":bundle.spec["package_id"],"package_version":bundle.spec["package_version"],"archive_sha256":bundle.spec["sha256"]}),
    )?);
    ensure!(
        bundle.spec["sha256"] == event.candidate_sha256 && binding == event.package_binding_sha256,
        "governance_current_package_baseline_changed"
    );
    oe_core::ensure_directory(&ledger)?;
    let _lock = oe_core::AdvisoryFileLock::acquire(&ledger.join("events.lock"), true)?;
    let mut events = Vec::new();
    for entry in std::fs::read_dir(&ledger)? {
        let p = entry?.path();
        if p.extension().is_some_and(|s| s == "json") {
            let old: Event =
                serde_json::from_value(strict_json(&read_regular_bytes(&p, 1024 * 1024)?)?)?;
            ensure!(
                p.file_stem().and_then(|s| s.to_str()) == Some(&old.event_sha256),
                "governance_history_name_tampered"
            );
            events.push(old);
        }
    }
    validate_history(&events)?;
    let policy_sha = oe_core::sha256(&policy_bytes);
    if let Some(previous) = &event.previous_sha256 {
        let old = events
            .iter()
            .find(|e| &e.event_sha256 == previous)
            .context("governance_previous_missing")?;
        ensure!(
            old.to == event.from
                && old.candidate_sha256 == event.candidate_sha256
                && old.package_binding_sha256 == event.package_binding_sha256
                && old.policy_sha256 == policy_sha,
            "governance_baseline_changed"
        );
        ensure!(
            !events
                .iter()
                .any(|e| e.previous_sha256.as_ref() == Some(previous)),
            "governance_duplicate_successor"
        );
        if event.to == "regression_passed" {
            ensure!(
                old.actor != event.actor,
                "governance_independent_verifier_required"
            );
        }
    } else {
        ensure!(
            event.from == "candidate"
                && !events
                    .iter()
                    .any(|e| e.candidate_sha256 == event.candidate_sha256),
            "governance_history_not_initial"
        );
    }
    let mut evidence = Vec::new();
    for digest in &supplied {
        let bytes = read_regular_bytes(
            &ledger.join("evidence").join(format!("{digest}.json")),
            crate::MAX_BYTES,
        )?;
        ensure!(
            oe_core::sha256(&bytes) == *digest,
            "governance_evidence_drift"
        );
        evidence.push(strict_json(&bytes)?);
    }
    let mut gate = json!({"execution":"not_run","scope":"policy-bound proposal/conflict evidence"});
    if ["regression_passed", "release_complete", "promoted"].contains(&event.to.as_str()) {
        // Rerun the actual immutable oracles. Serialized success cannot authorize itself.
        let regression = crate::packages::regress(root.path(), &policy.bundle)?;
        ensure!(
            regression["status"] == "passed"
                && regression["scenario_count"].as_u64().is_some_and(|n| n > 0),
            "governance_fresh_regression_not_passed"
        );
        let expected = regression_contract(&regression)?;
        ensure!(
            evidence
                .iter()
                .any(|v| regression_contract(v).is_ok_and(|v| v == expected)),
            "governance_regression_receipt_not_reproducible"
        );
        let bytes = canonical_json(&regression)?;
        let digest = oe_core::sha256(&bytes);
        let verification = ledger.join("verification");
        oe_core::ensure_directory(&verification)?;
        let file = verification.join(format!("{digest}.json"));
        if !file.exists() {
            write_new_bytes(&file, &bytes)?;
        }
        gate = json!({"execution":"completed","scope":"immutable semantic package oracles; no engineering adoption","fresh_regression_sha256":digest});
    }
    if ["release_complete", "promoted"].contains(&event.to.as_str()) {
        let mut paths = BTreeSet::new();
        let mut roles = BTreeSet::new();
        for a in &policy.release_artifacts {
            oe_semantic::safe_relative(&a.path)?;
            ensure!(
                paths.insert(&a.path) && valid_hash(&a.sha256),
                "governance_artifact_invalid_or_duplicate"
            );
            ensure!(
                oe_core::sha256(read_regular_bytes(
                    &root.path().join(&a.path),
                    crate::MAX_BYTES
                )?) == a.sha256,
                "governance_release_artifact_drift"
            );
            roles.insert(a.role.as_str());
        }
        ensure!(
            ["package_archive", "license", "interface", "origin"]
                .iter()
                .all(|r| roles.contains(r)),
            "governance_release_inventory_incomplete"
        );
    }
    ensure!(
        read_regular_bytes(&policy_path, 1024 * 1024)? == policy_bytes,
        "governance_policy_changed_during_execution"
    );
    let after = oe_semantic::load_bundle(root.path(), &policy.bundle)?;
    ensure!(
        after.spec == bundle.spec,
        "governance_package_changed_during_execution"
    );
    event.policy_sha256 = policy_sha;
    event.event_sha256 = event_digest(&event)?;
    write_new_bytes(
        &ledger.join(format!("{}.json", event.event_sha256)),
        &canonical_json(&serde_json::to_value(&event)?)?,
    )?;
    Ok(
        json!({"schema":"oev2.governance-transition/1","event":event,"gate":gate,"project_binding":"successor proposal only; old binding not changed","publication":"not_executed; external publication verification unsupported"}),
    )
}
