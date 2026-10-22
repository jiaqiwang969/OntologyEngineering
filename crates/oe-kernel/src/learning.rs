//! Bounded method experiments share the project model and formal Rust kernel.
//! Immutable events select a private trial plan, never a skill/package release.
use anyhow::{Context, Result, ensure};
use oe_contracts::{EvolutionAssessment, ExperienceCandidate, ExperienceState};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes};
use oe_wire::json::{canonical_json, strict_json};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Instant,
};

const LIMIT: usize = 16 * 1024 * 1024;
const MAX_HISTORY_EVENTS: usize = 256;
const HARD: [&str; 7] = [
    "identity",
    "units",
    "conditions",
    "source",
    "unknown_state",
    "necessary_acceptance",
    "evaluator_integrity",
];
const EVALUATOR: &str = "crates/oe-kernel/src/learning.rs";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub case_id: String,
    pub source_family: String,
    pub exposure: String,
    pub project: Reference,
    pub binding: Reference,
    pub evidence: Reference,
    pub bundle: String,
    pub query_ids: Vec<String>,
    pub shape_ids: Vec<String>,
    pub expected_conclusion: String,
    pub required_reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub current_use: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval: Option<RetrievalCase>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalCase {
    pub replay: Reference,
    pub inapplicable_page_ids: Vec<String>,
    pub necessary_page_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Contract {
    schema: String,
    goal_sha256: String,
    model: String,
    repetitions: u64,
    budget_max_checks: u64,
    hard_conditions: Vec<String>,
    cases: Vec<Case>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FreezeInput {
    schema: String,
    candidate: ExperienceCandidate,
    baseline: Reference,
    candidate_plan: Reference,
    goal: Reference,
    evaluation: Reference,
    prior_rejection: Option<Reference>,
    new_basis: Vec<Reference>,
    #[serde(default)]
    retry: Option<RetryInput>,
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetryInput {
    experiment: Reference,
    hypothesis_ids: Vec<String>,
    prior_rejections: Vec<Reference>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Experiment {
    schema: String,
    experiment_id: String,
    project_id: String,
    object_id: String,
    condition_scope: String,
    kind: String,
    applicability: Vec<String>,
    hypotheses: BTreeMap<String, String>,
    goal_sha256: String,
}

/// Version 2 keys retries by a registered experiment scope, never free hypothesis text.
fn check_retry_history(
    root: &SkillRoot,
    input: &FreezeInput,
    pool: &Path,
) -> Result<Option<Value>> {
    let Some(retry) = &input.retry else {
        return Ok(None);
    };
    ensure!(
        input.schema == "oev2.experience-freeze/2" && input.prior_rejection.is_none(),
        "experience_retry_protocol_mismatch"
    );
    let raw = read(root, &retry.experiment)?;
    let experiment: Experiment = serde_json::from_value(strict_json(&raw)?)?;
    ensure!(
        experiment.schema == "oev2.experience-experiment/2"
            && valid_id(&experiment.experiment_id)
            && experiment.object_id == input.candidate.object.object_id
            && experiment.project_id == input.candidate.project_id
            && experiment.condition_scope == input.candidate.object.condition_scope
            && experiment.kind == input.candidate.kind
            && experiment.applicability == input.candidate.applicability
            && experiment.goal_sha256 == input.goal.sha256,
        "experience_experiment_scope_mismatch"
    );
    ensure!(
        !retry.hypothesis_ids.is_empty()
            && retry.hypothesis_ids.len() <= 3
            && retry.hypothesis_ids.iter().all(|id| valid_id(id))
            && retry.hypothesis_ids.iter().collect::<BTreeSet<_>>().len()
                == retry.hypothesis_ids.len(),
        "experience_hypothesis_identity_invalid"
    );
    let descriptions: Vec<_> = retry
        .hypothesis_ids
        .iter()
        .map(|id| {
            experiment
                .hypotheses
                .get(id)
                .cloned()
                .context("experience_registered_hypothesis_missing")
        })
        .collect::<Result<_>>()?;
    ensure!(
        descriptions == input.candidate.hypotheses,
        "experience_registered_hypothesis_changed"
    );
    let experiments = root.private_path(Path::new("var/state/knowledge/experience-experiments"))?;
    oe_core::ensure_directory(&experiments)?;
    let registration = experiments.join(format!("{}.json", experiment.experiment_id));
    if registration.try_exists()? {
        ensure!(
            read_regular_bytes(&registration, LIMIT)? == raw,
            "experience_experiment_registration_changed"
        );
    }
    let mut required = BTreeMap::new();
    let mut old_basis: BTreeSet<String> = BTreeSet::new();
    for item in std::fs::read_dir(pool)? {
        let path = item?.path();
        oe_core::no_symlinks(&path)?;
        if !path.is_dir() {
            continue;
        }
        let events = history(&path)?;
        let Some(first) = events.first() else {
            continue;
        };
        let prior = historical_candidate(root, first)?;
        // Conservative family link survives renamed hypotheses and new experiment IDs.
        let related = (prior.project_id == experiment.project_id
            && prior.object.object_id == experiment.object_id
            && prior.object.condition_scope == experiment.condition_scope
            && prior.kind == experiment.kind)
            || prior.hypotheses.iter().any(|h| descriptions.contains(h));
        if !related {
            continue;
        }
        old_basis.extend(prior.sources.iter().map(|s| s.source_sha256.clone()));
        let dir = root.private_path(Path::new(
            first["body"]["candidate_root"]
                .as_str()
                .context("experience_previous_root_missing")?,
        ))?;
        let manifest = strict_json(&read_regular_bytes(&dir.join("freeze.json"), LIMIT)?)?;
        if let Some(files) = manifest["files"].as_object() {
            old_basis.extend(
                files
                    .iter()
                    .filter(|(name, _)| name.starts_with("new-basis/"))
                    .filter_map(|(_, value)| value["sha256"].as_str().map(str::to_owned)),
            );
        }
        for (i, event) in events
            .iter()
            .enumerate()
            .filter(|(_, e)| e["event"] == "Reject" && e["state"] == "rejected")
        {
            let event_path = path.join(format!("{i:04}.json"));
            required.insert(
                oe_core::portable_source_path(root.data_path(), &event_path)?,
                (
                    sha256(read_regular_bytes(&event_path, LIMIT)?),
                    event.clone(),
                ),
            );
        }
    }
    let mut provided = BTreeSet::new();
    for reference in &retry.prior_rejections {
        let path = root.data_expand(Path::new(&reference.path))?;
        let relative = oe_core::portable_source_path(root.data_path(), &path)?;
        ensure!(
            provided.insert(relative.clone()),
            "experience_duplicate_rejection_reference"
        );
        let (digest, event) = required
            .get(&relative)
            .context("experience_prior_rejection_not_in_registered_history")?;
        ensure!(
            *digest == reference.sha256 && strict_json(&read(root, reference)?)? == *event,
            "experience_prior_rejection_invalid"
        );
    }
    ensure!(
        provided == required.keys().cloned().collect(),
        "experience_related_rejections_incomplete"
    );
    if !required.is_empty() {
        ensure!(
            !input.new_basis.is_empty(),
            "experience_rejected_hypothesis_needs_new_basis"
        );
        let mut seen = BTreeSet::new();
        for basis in &input.new_basis {
            read(root, basis)?;
            ensure!(
                seen.insert(&basis.sha256) && !old_basis.contains(&basis.sha256),
                "experience_new_basis_repeats_old_source"
            );
        }
    }
    let identity = oe_contracts::RetryIdentity {
        protocol: "oev2.retry-identity/2".into(),
        experiment_id: experiment.experiment_id.clone(),
        hypothesis_ids: retry.hypothesis_ids.clone(),
        goal_sha256: experiment.goal_sha256.clone(),
        registration_sha256: sha256(&raw),
        scope_sha256: sha256(canonical_json(
            &json!({"project_id":experiment.project_id,"object_id":experiment.object_id,
            "condition_scope":experiment.condition_scope,"kind":experiment.kind,"applicability":experiment.applicability}),
        )?),
    };
    identity
        .validate()
        .map_err(|_| anyhow::anyhow!("experience_retry_identity_invalid"))?;
    Ok(Some(
        json!({"schema":"oev2.retry-history/2","identity":identity,"experiment":experiment,"registration":retry.experiment,
        "hypothesis_ids":retry.hypothesis_ids,"prior_rejections":retry.prior_rejections,"new_basis":input.new_basis,
        "registration_path":oe_core::portable_source_path(root.data_path(),&registration)?,"registration_bytes_sha256":sha256(raw),
        "related_rejection_count":required.len(),"basis_meaning":"verified new bytes only; engineering causal value requires separate evaluation"}),
    ))
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    mode: String,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    retrieval_order: Option<String>,
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn read(root: &SkillRoot, r: &Reference) -> Result<Vec<u8>> {
    let p = root.data_expand(Path::new(&r.path))?;
    oe_core::portable_source_path(root.data_path(), &p)?;
    let bytes = read_regular_bytes(&p, LIMIT)?;
    ensure!(sha256(&bytes) == r.sha256, "experience_reference_drift");
    Ok(bytes)
}
fn snapshot(
    root: &SkillRoot,
    dir: &Path,
    r: &Reference,
    name: &str,
    files: &mut BTreeMap<String, Value>,
) -> Result<Reference> {
    let bytes = read(root, r)?;
    let dest = dir.join(name);
    oe_core::ensure_directory(dest.parent().context("snapshot_parent")?)?;
    write_new_bytes(&dest, &bytes)?;
    files.insert(
        name.into(),
        json!({"sha256":sha256(&bytes),"bytes":bytes.len(),"original":r}),
    );
    Ok(Reference {
        path: name.into(),
        sha256: sha256(bytes),
    })
}
fn frozen_read(dir: &Path, r: &Reference) -> Result<Vec<u8>> {
    oe_semantic::safe_relative(&r.path)?;
    let bytes = read_regular_bytes(&dir.join(&r.path), LIMIT)?;
    ensure!(sha256(&bytes) == r.sha256, "experience_frozen_bytes_drift");
    Ok(bytes)
}
fn full_check_budget(dir: &Path, c: &Contract) -> Result<()> {
    if c.schema != "oev2.experience-evaluation/3" {
        return Ok(());
    }
    let total = c
        .cases
        .iter()
        .map(|case| -> Result<u64> {
            let binding: crate::Binding =
                serde_json::from_value(strict_json(&frozen_read(dir, &case.binding)?)?)?;
            Ok(binding.required_checks.len() as u64)
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .sum::<u64>();
    ensure!(
        total * c.repetitions <= c.budget_max_checks,
        "experience_evaluation_budget_exceeded"
    );
    Ok(())
}
fn contract_valid(c: &Contract) -> Result<()> {
    ensure!(
        [
            "oev2.experience-evaluation/1",
            "oev2.experience-evaluation/2",
            "oev2.experience-evaluation/3"
        ]
        .contains(&c.schema.as_str())
            && c.model == format!("{}@{}", crate::ENGINE_ID, crate::ENGINE_VERSION),
        "experience_evaluator_model_mismatch"
    );
    ensure!(
        (1..=5).contains(&c.repetitions) && !c.cases.is_empty() && c.cases.len() <= 16,
        "experience_evaluation_budget_invalid"
    );
    let hard: BTreeSet<_> = c.hard_conditions.iter().map(String::as_str).collect();
    ensure!(
        hard == HARD.into_iter().collect() && hard.len() == c.hard_conditions.len(),
        "experience_necessary_criteria_changed"
    );
    let mut ids = BTreeSet::new();
    let mut checks = 0;
    for case in &c.cases {
        ensure!(
            c.schema == "oev2.experience-evaluation/1" || case.current_use,
            "experience_current_context_required"
        );
        ensure!(
            valid_id(&case.case_id) && ids.insert(&case.case_id) && !case.source_family.is_empty(),
            "experience_case_identity_invalid"
        );
        // This bounded pilot deliberately has no blind-task attestation capability.
        ensure!(
            case.exposure == "development_replay",
            "experience_independent_exposure_not_attested"
        );
        ensure!(
            ["pass", "unknown", "violated"].contains(&case.expected_conclusion.as_str()),
            "experience_expected_conclusion_invalid"
        );
        ensure!(
            !case.query_ids.is_empty() && !case.shape_ids.is_empty(),
            "experience_required_checks_empty"
        );
        checks +=
            case.query_ids.len() + case.shape_ids.len() + 1 + 2 * usize::from(case.current_use);
    }
    ensure!(
        checks as u64 * c.repetitions <= c.budget_max_checks && c.budget_max_checks <= 256,
        "experience_evaluation_budget_exceeded"
    );
    Ok(())
}
fn events_dir(root: &SkillRoot, id: &str) -> Result<PathBuf> {
    ensure!(valid_id(id), "experience_candidate_identity_invalid");
    let p = root.private_path(Path::new(&format!(
        "var/state/knowledge/experience-events/{id}"
    )))?;
    oe_core::ensure_directory(&p)?;
    Ok(p)
}
fn history(dir: &Path) -> Result<Vec<Value>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.extension().is_some_and(|e| e == "json") {
            paths.push(p)
        }
    }
    paths.sort();
    ensure!(
        paths.len() <= MAX_HISTORY_EVENTS,
        "experience_history_budget"
    );
    let mut out = Vec::new();
    let mut previous = Value::Null;
    for (i, p) in paths.iter().enumerate() {
        ensure!(
            p.file_name().and_then(|s| s.to_str()) == Some(format!("{i:04}.json").as_str()),
            "experience_history_gap"
        );
        let bytes = read_regular_bytes(p, LIMIT)?;
        let record = strict_json(&bytes)?;
        ensure!(
            record["previous_sha256"] == previous,
            "experience_history_tampered"
        );
        previous = json!(sha256(bytes));
        out.push(record);
    }
    Ok(out)
}
fn historical_candidate(root: &SkillRoot, first: &Value) -> Result<ExperienceCandidate> {
    let relative = first["body"]["candidate_root"]
        .as_str()
        .context("experience_previous_root_missing")?;
    let dir = root.private_path(Path::new(relative))?;
    let raw = read_regular_bytes(&dir.join("freeze.json"), LIMIT)?;
    ensure!(
        first["body"]["freeze_sha256"] == sha256(&raw),
        "experience_freeze_identity_drift"
    );
    let manifest = strict_json(&raw)?;
    let digest = manifest["files"]["experience.json"]["sha256"]
        .as_str()
        .context("experience_file_digest")?;
    Ok(serde_json::from_slice(&frozen_read(
        &dir,
        &Reference {
            path: "experience.json".into(),
            sha256: digest.into(),
        },
    )?)?)
}
fn history_capacity(events: usize, next: ExperienceState) -> Result<()> {
    // A trial or rejected plan must still be able to append its Restore event.
    // The restored baseline may use the final slot; the read bound stays 256.
    let restore_slot = usize::from(matches!(
        next,
        ExperienceState::TrialActive | ExperienceState::Rejected
    ));
    ensure!(
        events + 1 + restore_slot <= MAX_HISTORY_EVENTS,
        "experience_history_budget"
    );
    Ok(())
}

/// Keep the participating writer lock from capacity/state preflight through
/// output preparation and event commit. Capacity or contention must not first
/// be discovered after a trial or recovery output has already been created.
struct HistoryWriter {
    id: String,
    dir: PathBuf,
    lock: oe_core::AdvisoryFileLock,
    old: Vec<Value>,
}

impl HistoryWriter {
    fn acquire(root: &SkillRoot, id: &str) -> Result<Self> {
        let dir = events_dir(root, id)?;
        let lock = oe_core::AdvisoryFileLock::acquire(&dir.join(".lock"), true)?;
        let old = history(&dir)?;
        Ok(Self {
            id: id.into(),
            dir,
            lock,
            old,
        })
    }

    fn prepare(&self, state: ExperienceState, event: &str) -> Result<()> {
        if let Some(parent) = self.old.last() {
            let from: ExperienceState = serde_json::from_value(parent["state"].clone())?;
            ensure!(
                from.transition(event)
                    .is_some_and(|(next, _)| next == state),
                "experience_transition_refused"
            );
        } else {
            ensure!(
                event == "Freeze" && state == ExperienceState::Frozen,
                "experience_history_root_invalid"
            );
        }
        history_capacity(self.old.len(), state)?;
        self.lock.ensure_current()
    }

    fn commit(
        self,
        root: &SkillRoot,
        state: ExperienceState,
        event: &str,
        body: Value,
    ) -> Result<Value> {
        self.prepare(state, event)?;
        let previous = self
            .old
            .last()
            .map(|_| {
                oe_core::fingerprint_file(
                    &self.dir.join(format!("{:04}.json", self.old.len() - 1)),
                    LIMIT as u64,
                )
                .map(|i| i.sha256)
            })
            .transpose()?;
        let record = json!({"schema":"oev2.experience-event/1","candidate_id":self.id,"sequence":self.old.len(),"state":state,"event":event,
            "previous_sha256":previous,"body":body,"skill_working_version":"unchanged","ontology_promotion":"not_performed","publication":"not_performed"});
        self.lock.ensure_current()?;
        let path = self.dir.join(format!("{:04}.json", self.old.len()));
        write_new(&path, &record)?;
        Ok(
            json!({"event":record,"path":oe_core::portable_source_path(root.data_path(),&path)?,"sha256":oe_core::fingerprint_file(&path,LIMIT as u64)?.sha256}),
        )
    }
}

fn append(
    root: &SkillRoot,
    id: &str,
    state: ExperienceState,
    event: &str,
    body: Value,
) -> Result<Value> {
    HistoryWriter::acquire(root, id)?.commit(root, state, event, body)
}
pub fn freeze(root: &SkillRoot, input: &Value, output: &Path) -> Result<Value> {
    // Plans, examples and history belong to the selected project store. Rules
    // and evaluator code keep their software-installation identity; a project
    // must not replace either by providing similarly named data files.
    let software = SkillRoot::new(root.path())?;
    let input: FreezeInput = serde_json::from_value(input.clone())?;
    ensure!(
        ["oev2.experience-freeze/1", "oev2.experience-freeze/2"].contains(&input.schema.as_str()),
        "experience_freeze_schema"
    );
    input
        .candidate
        .validate()
        .map_err(|e| anyhow::anyhow!("experience_candidate_contract:{e:?}"))?;
    ensure!(
        valid_id(&input.candidate.candidate_id)
            && !input.candidate.hypotheses.is_empty()
            && input.candidate.hypotheses.len() <= 3
            && input
                .candidate
                .hypotheses
                .iter()
                .all(|h| !h.trim().is_empty())
            && input
                .candidate
                .hypotheses
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == input.candidate.hypotheses.len(),
        "experience_hypothesis_budget"
    );
    ensure!(
        !input.candidate.sources.is_empty()
            && !input.candidate.applicability.is_empty()
            && !input.candidate.counterexamples.is_empty()
            && !input.candidate.verification_requirements.is_empty(),
        "experience_scope_incomplete"
    );
    ensure!(
        input
            .candidate
            .sources
            .iter()
            .any(|s| s.source_sha256 == input.candidate.object.source_sha256),
        "experience_origin_source_mismatch"
    );
    if let Some(prior) = &input.prior_rejection {
        let v = strict_json(&read(root, prior)?)?;
        ensure!(
            v["event"]["state"] == "rejected" || v["state"] == "rejected",
            "experience_prior_rejection_invalid"
        );
        ensure!(
            !input.new_basis.is_empty(),
            "experience_rejected_hypothesis_needs_new_basis"
        );
    }
    let ledger = events_dir(root, &input.candidate.candidate_id)?;
    ensure!(
        history(&ledger)?.is_empty(),
        "experience_candidate_already_exists"
    );
    let pool = ledger.parent().context("experience_pool_missing")?;
    ensure!(
        input.schema != "oev2.experience-freeze/2" || input.retry.is_some(),
        "experience_retry_protocol_required"
    );
    let _retry_lock = if input.schema == "oev2.experience-freeze/2" {
        Some(oe_core::AdvisoryFileLock::acquire(
            &pool.join(".retry.lock"),
            true,
        )?)
    } else {
        None
    };
    let retry_history = check_retry_history(root, &input, pool)?;
    if input.schema == "oev2.experience-freeze/1" {
        for item in std::fs::read_dir(pool)? {
            let path = item?.path();
            oe_core::no_symlinks(&path)?;
            if !path.is_dir() || path == ledger {
                continue;
            }
            let old = history(&path)?;
            let rejected = old.iter().find(|e| e["state"] == "rejected");
            if let (Some(first), Some(rejection)) = (old.first(), rejected) {
                let prior = historical_candidate(root, first)?;
                if prior
                    .hypotheses
                    .iter()
                    .any(|h| input.candidate.hypotheses.contains(h))
                {
                    let r = input
                        .prior_rejection
                        .as_ref()
                        .context("experience_rejected_hypothesis_needs_new_basis")?;
                    let evidence = strict_json(&read(root, r)?)?;
                    let event = evidence
                        .get("event")
                        .filter(|v| v.is_object())
                        .unwrap_or(&evidence);
                    ensure!(
                        event == rejection && !input.new_basis.is_empty(),
                        "experience_rejected_hypothesis_needs_new_basis"
                    );
                    ensure!(
                        input
                            .new_basis
                            .iter()
                            .any(|r| !prior.sources.iter().any(|s| s.source_sha256 == r.sha256)),
                        "experience_new_basis_repeats_old_source"
                    );
                }
            }
        }
    }
    let dir = root.new_private_dir(output)?;
    let mut files = BTreeMap::new();
    let baseline = snapshot(
        root,
        &dir,
        &input.baseline,
        "baseline-plan.json",
        &mut files,
    )?;
    let candidate_plan = snapshot(
        root,
        &dir,
        &input.candidate_plan,
        "candidate-plan.json",
        &mut files,
    )?;
    let goal = snapshot(root, &dir, &input.goal, "goal.json", &mut files)?;
    let evaluation = snapshot(
        root,
        &dir,
        &input.evaluation,
        "evaluation-original.json",
        &mut files,
    )?;
    let mut contract: Contract = serde_json::from_slice(&frozen_read(&dir, &evaluation)?)?;
    contract_valid(&contract)?;
    ensure!(
        input.schema != "oev2.experience-freeze/2"
            || [
                "oev2.experience-evaluation/2",
                "oev2.experience-evaluation/3"
            ]
            .contains(&contract.schema.as_str()),
        "experience_new_protocol_needs_new_evaluation"
    );
    if contract.schema != "oev2.experience-evaluation/1" {
        let before = plan(&read(root, &input.baseline)?)?;
        let after = plan(&read(root, &input.candidate_plan)?)?;
        ensure!(
            before.mode == "required_semantic_review" && after.mode == "required_semantic_review",
            "experience_necessary_review_removed"
        );
        ensure!(
            before.schema.as_deref() == Some("oev2.method-plan/2")
                && after.schema.as_deref() == Some("oev2.method-plan/2"),
            "experience_versioned_method_plan_required"
        );
    }
    ensure!(
        contract.goal_sha256 == goal.sha256,
        "experience_goal_binding_mismatch"
    );
    for (i, case) in contract.cases.iter_mut().enumerate() {
        case.project = snapshot(
            root,
            &dir,
            &case.project,
            &format!("cases/{i}/project.json"),
            &mut files,
        )?;
        case.binding = snapshot(
            root,
            &dir,
            &case.binding,
            &format!("cases/{i}/binding.json"),
            &mut files,
        )?;
        case.evidence = snapshot(
            root,
            &dir,
            &case.evidence,
            &format!("cases/{i}/evidence.ttl"),
            &mut files,
        )?;
        if let Some(retrieval) = &mut case.retrieval {
            retrieval.replay = snapshot(
                root,
                &dir,
                &retrieval.replay,
                &format!("cases/{i}/retrieval.json"),
                &mut files,
            )?;
        }
        let bundle = oe_semantic::load_bundle(root.path(), &case.bundle)?;
        let path = bundle.spec["path"]
            .as_str()
            .context("experience_bundle_path_missing")?;
        let r = Reference {
            path: path.into(),
            sha256: bundle.spec["sha256"]
                .as_str()
                .context("experience_bundle_sha_missing")?
                .into(),
        };
        snapshot(
            &software,
            &dir,
            &r,
            &format!("cases/{i}/rules.zip"),
            &mut files,
        )?;
    }
    full_check_budget(&dir, &contract)?;
    for (i, citation) in input.candidate.sources.iter().enumerate() {
        citation
            .validate()
            .map_err(|_| anyhow::anyhow!("experience_source_citation_invalid"))?;
        snapshot(
            root,
            &dir,
            &Reference {
                path: citation.source_uri.clone(),
                sha256: citation.source_sha256.clone(),
            },
            &format!("sources/{i}"),
            &mut files,
        )?;
    }
    for (i, r) in input.new_basis.iter().enumerate() {
        snapshot(root, &dir, r, &format!("new-basis/{i}"), &mut files)?;
    }
    if let Some(retry) = &input.retry {
        snapshot(root, &dir, &retry.experiment, "experiment.json", &mut files)?;
        for (i, r) in retry.prior_rejections.iter().enumerate() {
            snapshot(
                root,
                &dir,
                r,
                &format!("prior-rejections/{i}.json"),
                &mut files,
            )?;
        }
    }
    let evaluator_raw = read_regular_bytes(&root.path().join(EVALUATOR), LIMIT)?;
    let evaluator = snapshot(
        &software,
        &dir,
        &Reference {
            path: EVALUATOR.into(),
            sha256: sha256(&evaluator_raw),
        },
        "evaluator.rs",
        &mut files,
    )?;
    let raw = canonical_json(&serde_json::to_value(&input.candidate)?)?;
    write_new_bytes(&dir.join("experience.json"), &raw)?;
    files.insert(
        "experience.json".into(),
        json!({"sha256":sha256(&raw),"bytes":raw.len()}),
    );
    let raw = canonical_json(&serde_json::to_value(&contract)?)?;
    write_new_bytes(&dir.join("evaluation-resolved.json"), &raw)?;
    files.insert(
        "evaluation-resolved.json".into(),
        json!({"sha256":sha256(&raw),"bytes":raw.len()}),
    );
    // The evaluator includes its compiled dependencies, not only this Rust file.
    // Keep actual executable bytes; recompilation requires a distinct baseline.
    let producer_raw = read_regular_bytes(&std::env::current_exe()?, 512 * 1024 * 1024)?;
    let producer_sha = sha256(&producer_raw);
    write_new_bytes(&dir.join("evaluator-executable.bin"), &producer_raw)?;
    let manifest = json!({"schema":"oev2.frozen-experience/1","candidate_id":input.candidate.candidate_id,"root":oe_core::portable_source_path(root.data_path(),&dir)?,
        "files":files,"baseline_sha256":baseline.sha256,"candidate_sha256":candidate_plan.sha256,"goal_sha256":goal.sha256,"evaluation_sha256":evaluation.sha256,
        "evaluator_sha256":evaluator.sha256,"producer_executable_sha256":producer_sha,"producer_executable_bytes":producer_raw.len(),"prior_rejection":input.prior_rejection,
        "retry_history":retry_history,"protocol":input.schema,
        "project_state":"existing project references; no duplicate project store","transfer":"development_replay"});
    // Commit marker last. Incomplete snapshots have no usable freeze manifest.
    for (name, f) in &files {
        if let Some(r) = f.get("original") {
            let r: Reference = serde_json::from_value(r.clone())?;
            if name == "evaluator.rs" || name.ends_with("/rules.zip") {
                read(&software, &r)?;
            } else {
                read(root, &r)?;
            }
        }
    }
    write_new(&dir.join("freeze.json"), &manifest)?;
    if let Some(retry) = &input.retry {
        let experiment: Experiment =
            serde_json::from_value(strict_json(&read(root, &retry.experiment)?)?)?;
        let registration = root.private_path(Path::new(&format!(
            "var/state/knowledge/experience-experiments/{}.json",
            experiment.experiment_id
        )))?;
        if !registration.try_exists()? {
            write_new_bytes(&registration, &read(root, &retry.experiment)?)?;
        }
    }
    append(
        root,
        &input.candidate.candidate_id,
        ExperienceState::Frozen,
        "Freeze",
        json!({"candidate_root":manifest["root"],"freeze_sha256":oe_core::fingerprint_file(&dir.join("freeze.json"),LIMIT as u64)?.sha256,"selected_plan":"baseline-plan.json","manifest":manifest}),
    )
}
fn checked(
    root: &SkillRoot,
    path: &Path,
) -> Result<(PathBuf, Value, ExperienceCandidate, Contract)> {
    let dir = root.private_path(path)?;
    let raw = read_regular_bytes(&dir.join("freeze.json"), LIMIT)?;
    let m = strict_json(&raw)?;
    let id = m["candidate_id"]
        .as_str()
        .context("experience_identity_missing")?;
    let h = history(&events_dir(root, id)?)?;
    let first = h.first().context("experience_freeze_event_missing")?;
    ensure!(
        first["body"]["freeze_sha256"] == sha256(&raw)
            && first["body"]["candidate_root"]
                == oe_core::portable_source_path(root.data_path(), &dir)?,
        "experience_freeze_identity_drift"
    );
    for (p, v) in m["files"].as_object().context("experience_files_missing")? {
        frozen_read(
            &dir,
            &Reference {
                path: p.clone(),
                sha256: v["sha256"]
                    .as_str()
                    .context("experience_file_digest")?
                    .into(),
            },
        )?;
    }
    ensure!(
        oe_core::fingerprint_file(&dir.join("evaluator-executable.bin"), 512 * 1024 * 1024)?.sha256
            == m["producer_executable_sha256"],
        "experience_frozen_evaluator_executable_drift"
    );
    ensure!(
        crate::executable_digest()? == m["producer_executable_sha256"],
        "experience_compiled_evaluator_changed_new_baseline_required"
    );
    ensure!(
        oe_core::fingerprint_file(&root.path().join(EVALUATOR), LIMIT as u64)?.sha256
            == m["evaluator_sha256"],
        "experience_evaluator_changed_new_baseline_required"
    );
    let candidate =
        serde_json::from_slice(&read_regular_bytes(&dir.join("experience.json"), LIMIT)?)?;
    let contract: Contract = serde_json::from_slice(&read_regular_bytes(
        &dir.join("evaluation-resolved.json"),
        LIMIT,
    )?)?;
    contract_valid(&contract)?;
    full_check_budget(&dir, &contract)?;
    for (i, case) in contract.cases.iter().enumerate() {
        let bundle = oe_semantic::load_bundle(root.path(), &case.bundle)?;
        ensure!(
            m["files"][format!("cases/{i}/rules.zip")]["sha256"] == bundle.spec["sha256"],
            "experience_rules_changed_new_baseline_required"
        );
    }
    Ok((dir, m, candidate, contract))
}
fn plan(raw: &[u8]) -> Result<Plan> {
    let p: Plan = serde_json::from_value(strict_json(raw)?)?;
    ensure!(
        ["no_semantic_review", "required_semantic_review"].contains(&p.mode.as_str()),
        "experience_plan_mode_unsupported"
    );
    if let Some(schema) = &p.schema {
        let contract = oe_contracts::MethodPlan {
            schema: schema.clone(),
            mode: p.mode.clone(),
            retrieval_order: p
                .retrieval_order
                .clone()
                .context("experience_retrieval_order_missing")?,
        };
        contract
            .validate()
            .map_err(|_| anyhow::anyhow!("experience_versioned_method_plan_invalid"))?;
    } else {
        ensure!(
            p.retrieval_order.is_none(),
            "experience_retrieval_order_requires_versioned_plan"
        );
    }
    Ok(p)
}
fn run_case(
    root: &SkillRoot,
    dir: &Path,
    case: &Case,
    p: &Plan,
    evaluation_schema: &str,
) -> Result<Value> {
    let project: crate::project::Project =
        serde_json::from_value(strict_json(&frozen_read(dir, &case.project)?)?)?;
    crate::project::validate(&project)?;
    let binding: crate::Binding =
        serde_json::from_value(strict_json(&frozen_read(dir, &case.binding)?)?)?;
    binding.validate()?;
    let evidence = frozen_read(dir, &case.evidence)?;
    let bundle = oe_semantic::load_bundle(root.path(), &case.bundle)?;
    let linked_origin = evaluation_schema == "oev2.experience-evaluation/3";
    ensure!(
        binding.project_id == project.project_id
            && project.objects.get(&binding.object_id) == Some(&binding.object_revision)
            && project
                .evidence
                .values()
                .any(|v| v == &binding.source_sha256)
            && (linked_origin || binding.source_sha256 == sha256(&evidence))
            && project.conditions.contains_key(&binding.condition_scope),
        "experience_project_identity_mismatch"
    );
    ensure!(
        binding.package_id == bundle.spec["package_id"]
            && binding.package_version == bundle.spec["package_version"]
            && binding.package_sha256 == bundle.spec["sha256"],
        "experience_package_identity_mismatch"
    );
    let mut required: BTreeSet<_> = case
        .query_ids
        .iter()
        .chain(&case.shape_ids)
        .cloned()
        .chain(["bound_focus_context".into()])
        .collect();
    if linked_origin {
        required.extend([
            "current_project_context".into(),
            "current_condition_model".into(),
        ]);
        let object = oe_contracts::ObjectIdentity {
            object_id: binding.object_id.clone(),
            revision: binding.object_revision.clone(),
            condition_scope: binding.condition_scope.clone(),
            source_sha256: binding.source_sha256.clone(),
        };
        required.extend(
            crate::engineering::checks_current(root, &project, &object)
                .into_iter()
                .map(|check| check.id),
        );
    }
    ensure!(
        required == binding.required_checks,
        "experience_necessary_checks_changed"
    );
    let start = Instant::now();
    let mut receipt = if p.mode == "required_semantic_review" {
        let request = oe_contracts::ReviewRequest {
            object: oe_contracts::ObjectIdentity {
                object_id: binding.object_id.clone(),
                revision: binding.object_revision.clone(),
                condition_scope: binding.condition_scope.clone(),
                source_sha256: binding.source_sha256.clone(),
            },
            evidence_sha256: sha256(&evidence),
            required_checks: binding.required_checks.iter().cloned().collect(),
        };
        let port = crate::port::RustSemanticPort {
            root: root.path().into(),
            binding: binding.clone(),
            bundle: case.bundle.clone(),
            queries: case.query_ids.clone(),
            shapes: case.shape_ids.clone(),
            evidence: evidence.clone(),
        };
        let receipt = port.execute(&request, None)?;
        ensure!(
            receipt
                .checks
                .iter()
                .any(|c| c.id == "bound_focus_context" && c.conclusion == "pass"),
            "experience_focus_scope_mismatch"
        );
        receipt
    } else {
        crate::evaluate(binding, &evidence, Vec::new())?
    };
    if case.current_use {
        receipt = crate::project::review_current(root, &project, &evidence, receipt)?;
    }
    let retrieval = if let Some(test) = &case.retrieval {
        let raw = frozen_read(dir, &test.replay)?;
        let replay: oe_knowledge::retrieval::Replay = serde_json::from_value(strict_json(&raw)?)?;
        replay.validate()?;
        let all: BTreeSet<_> = replay
            .pools
            .iter()
            .flat_map(|pool| pool.pages.iter())
            .filter_map(|page| page["id"].as_str())
            .collect();
        ensure!(
            test.inapplicable_page_ids
                .iter()
                .chain(&test.necessary_page_ids)
                .all(|id| all.contains(id.as_str()))
                && !test
                    .inapplicable_page_ids
                    .iter()
                    .any(|id| test.necessary_page_ids.contains(id)),
            "experience_retrieval_oracle_not_in_frozen_sources"
        );
        let start = Instant::now();
        let selected = oe_knowledge::retrieval::select_retrieval_pools(
            &replay,
            p.retrieval_order
                .as_deref()
                .context("experience_retrieval_plan_missing")?,
        )?;
        let ids: BTreeSet<_> = selected
            .iter()
            .filter_map(|page| page["id"].as_str().map(str::to_owned))
            .collect();
        json!({"execution":"completed","replay_sha256":test.replay.sha256,"index_identity":replay.index_identity,
            "query":replay.query,"order":p.retrieval_order,"selected_page_ids":ids,
            "inapplicable_selected":test.inapplicable_page_ids.iter().filter(|id|ids.contains(*id)).count(),
            "necessary_pages_retained":!test.necessary_page_ids.is_empty() && test.necessary_page_ids.iter().all(|id|ids.contains(id)),
            "unknown_applicability":"all pages outside the frozen rejection/necessary-reference inventory remain unverified",
            "wall_time_ns":start.elapsed().as_nanos().to_string(),"model_calls":0,"tokens":0,
            "cost_scope":"selection replay only; source IO and whole engineering/model cost separate","engineering_adoption":"not_established"})
    } else {
        json!({"execution":"not_run","cost":"unknown"})
    };
    let check_count = receipt.checks.len();
    let mut reasons = BTreeSet::new();
    for check in &receipt.checks {
        if let Some(rows) = check.result["rows"].as_array() {
            for row in rows {
                if let Some(reason) = row["reason"]["value"].as_str() {
                    reasons.insert(reason.to_owned());
                }
            }
        }
    }
    let matched = receipt.conclusion == case.expected_conclusion
        && case.required_reasons.iter().all(|r| reasons.contains(r));
    Ok(
        json!({"case_id":case.case_id,"source_family":case.source_family,"exposure":case.exposure,"required_condition_diagnosis":if matched{"matched"}else{"not_matched"},
        "receipt":receipt,"observed_reasons":reasons,"top_level_checks":check_count,"wall_time_ns":start.elapsed().as_nanos().to_string(),"engineering_adoption":"blocked_or_separate",
        "retrieval":retrieval,
        "current_use":if case.current_use {"explicit_current_project_check_required"} else {"historical_snapshot_only_current_conditions_unverified"}}),
    )
}
fn assessment(root: &SkillRoot, path: &Path) -> Result<Value> {
    let (dir, m, candidate, contract) = checked(root, path)?;
    let baseline = plan(&read_regular_bytes(&dir.join("baseline-plan.json"), LIMIT)?)?;
    let candidate_plan = plan(&read_regular_bytes(
        &dir.join("candidate-plan.json"),
        LIMIT,
    )?);
    let mut baseline_runs = Vec::new();
    let mut candidate_runs = Vec::new();
    let mut critics = Vec::new();
    if candidate_plan.is_err() {
        critics.push("unsupported_plan_fields_or_mode: closed plan cannot override answers, units, inputs or evaluator".to_owned());
    }
    let failure = |case: &Case, e: String| json!({"case_id":case.case_id,"source_family":case.source_family,"exposure":case.exposure,"execution":"failed","conclusion":"unknown","required_condition_diagnosis":"not_matched","reason":e,"top_level_checks":null,"wall_time_ns":null,"engineering_adoption":"blocked"});
    for _ in 0..contract.repetitions {
        for case in &contract.cases {
            baseline_runs.push(
                run_case(root, &dir, case, &baseline, &contract.schema)
                    .unwrap_or_else(|e| failure(case, format!("{e:#}"))),
            );
            candidate_runs.push(match &candidate_plan {
                Ok(p) => run_case(root, &dir, case, p, &contract.schema)
                    .unwrap_or_else(|e| failure(case, format!("{e:#}"))),
                Err(_) => failure(case, "critic_refused_plan_before_execution".into()),
            });
        }
    }
    let gains = |runs: &Vec<Value>| {
        runs.iter()
            .filter(|r| r["required_condition_diagnosis"] == "matched")
            .count()
    };
    let before = gains(&baseline_runs);
    let after = gains(&candidate_runs);
    let complete = candidate_runs.len() == baseline_runs.len();
    let failed = baseline_runs
        .iter()
        .chain(&candidate_runs)
        .any(|r| r["execution"] == "failed");
    let all_matched = candidate_runs
        .iter()
        .all(|r| r["required_condition_diagnosis"] == "matched");
    let current_guards_clear = contract.schema == "oev2.experience-evaluation/1"
        || baseline_runs.iter().chain(&candidate_runs).all(|run| {
            ["current_project_context", "current_condition_model"]
                .iter()
                .all(|id| {
                    run["receipt"]["checks"].as_array().is_some_and(|checks| {
                        checks.iter().any(|check| {
                            check["id"] == *id
                                && check["execution"] == "completed"
                                && check["conclusion"] == "pass"
                        }) && checks
                            .iter()
                            .filter(|check| {
                                check["id"].as_str().is_some_and(|id| {
                                    id.starts_with("engineering:") || id.starts_with("engineering_")
                                })
                            })
                            .all(|check| {
                                check["execution"] == "completed" && check["conclusion"] == "pass"
                            })
                    })
                })
        });
    let retrieval_improved = baseline_runs
        .iter()
        .zip(&candidate_runs)
        .all(|(before, after)| {
            after["retrieval"]["execution"] == "completed"
                && before["retrieval"]["execution"] == "completed"
                && before["retrieval"]["necessary_pages_retained"] == true
                && after["retrieval"]["necessary_pages_retained"] == true
                && after["retrieval"]["inapplicable_selected"].as_u64()
                    <= before["retrieval"]["inapplicable_selected"].as_u64()
        })
        && baseline_runs
            .iter()
            .zip(&candidate_runs)
            .any(|(before, after)| {
                after["retrieval"]["inapplicable_selected"].as_u64()
                    < before["retrieval"]["inapplicable_selected"].as_u64()
            });
    let verdict = if !critics.is_empty() {
        "rejected"
    } else if failed || candidate.kind == "implementation_fix" {
        "inconclusive"
    } else if candidate.kind == "ontology_knowledge" && candidate.ontology_delta == "no_delta" {
        "no_delta"
    } else if complete
        && all_matched
        && current_guards_clear
        && ((after > before) || (before == after && retrieval_improved))
    {
        "admissible_trial"
    } else {
        "no_delta"
    };
    let mut reasons = critics.clone();
    if !current_guards_clear {
        reasons.push("current project/model identity not established on both sides; gain cannot override this necessary condition".into());
    }
    reasons.push(format!(
        "bounded condition diagnoses: baseline {before}, candidate {after}; fixed denominator {}",
        baseline_runs.len()
    ));
    reasons.push("physical mechanism, engineering cost and independent transfer not established; novelty grants no improvement".into());
    if retrieval_improved {
        reasons.push("same frozen source pools and necessary reviews: fewer independently rejected pages selected while necessary references retained; bounded retrieval replay only".into());
    }
    if candidate.kind == "implementation_fix" {
        reasons.push("method evaluator cannot verify a Rust implementation repair; use frozen compiler/regression contract separately".into());
    }
    let dto=EvolutionAssessment {candidate_id:candidate.candidate_id,verdict:verdict.into(),baseline_sha256:m["baseline_sha256"].as_str().context("baseline_sha")?.into(),candidate_sha256:m["candidate_sha256"].as_str().context("candidate_sha")?.into(),
        evaluation_sha256:m["evaluation_sha256"].as_str().context("evaluation_sha")?.into(),cost_status:if failed{"unknown"}else{"measured_execution_only"}.into(),transfer:"development_replay".into(),hard_conditions:HARD.iter().map(|s|s.to_string()).collect(),reasons,
        scope:"private diagnostic trial; exact frozen inputs/rules preserved; project unknowns retained; no skill switch, industry promotion or publication".into()};
    dto.validate()
        .map_err(|_| anyhow::anyhow!("experience_assessment_contract"))?;
    Ok(
        json!({"schema":"oev2.experience-assessment/1","assessment":dto,"model":contract.model,"repetitions":contract.repetitions,"budget_max_checks":contract.budget_max_checks,"goal_sha256":m["goal_sha256"],"evaluator_sha256":m["evaluator_sha256"],
        "critic":{"status":if critics.is_empty(){"bounded_plan_checks_clear"}else{"rejected"},"findings":critics,"scope":"closed declarative plans plus exact inputs/evaluator identity; arbitrary code critique not implemented"},
        "baseline":baseline_runs,"candidate":candidate_runs,"engineering_cost":{"status":"unknown"},"model_calls":0,"transfer":"development_replay","engineering_acceptance":"not_established","executable_sha256":crate::executable_digest()?}),
    )
}
pub fn assess(root: &SkillRoot, path: &Path, output: &Path) -> Result<Value> {
    let (_, _, c, _) = checked(root, path)?;
    let writer = HistoryWriter::acquire(root, &c.candidate_id)?;
    ensure!(
        writer.old.last().is_some_and(|e| e["state"] == "frozen"),
        "experience_already_assessed"
    );
    writer.prepare(ExperienceState::Assessed, "Evaluate")?;
    let record = assessment(root, path)?;
    let (_, m, _, _) = checked(root, path)?;
    let dest = root.private_path(output)?;
    write_new(&dest, &record)?;
    let event = writer.commit(
        root,
        ExperienceState::Assessed,
        "Evaluate",
        json!({"candidate_root":m["root"],"assessment":Reference{path:oe_core::portable_source_path(root.data_path(),&dest)?,sha256:oe_core::fingerprint_file(&dest,LIMIT as u64)?.sha256},"selected_plan":"baseline-plan.json","verdict":record["assessment"]["verdict"]}),
    )?;
    Ok(json!({"assessment":record["assessment"],"event":event}))
}
fn stable(v: &Value) -> Value {
    let mut v = v.clone();
    for side in ["baseline", "candidate"] {
        if let Some(runs) = v[side].as_array_mut() {
            for r in runs {
                r.as_object_mut()
                    .expect("run object")
                    .remove("wall_time_ns");
                if let Some(retrieval) = r["retrieval"].as_object_mut() {
                    retrieval.remove("wall_time_ns");
                }
            }
        }
    }
    v
}
pub fn decide(root: &SkillRoot, path: &Path, action: &str, reason: &str) -> Result<Value> {
    ensure!(
        !reason.trim().is_empty(),
        "experience_decision_reason_missing"
    );
    let (_, m, c, _) = checked(root, path)?;
    let writer = HistoryWriter::acquire(root, &c.candidate_id)?;
    let last = writer.old.last().context("experience_history_missing")?;
    ensure!(last["state"] == "assessed", "experience_not_assessed");
    let r: Reference = serde_json::from_value(last["body"]["assessment"].clone())?;
    let saved = strict_json(&read(root, &r)?)?;
    let current = assessment(root, path)?;
    ensure!(
        stable(&saved) == stable(&current),
        "experience_assessment_not_reproducible"
    );
    let (state, event, selected) = match action {
        "accept-trial" => {
            ensure!(
                current["assessment"]["verdict"] == "admissible_trial",
                "experience_trial_not_admissible"
            );
            (
                ExperienceState::TrialActive,
                "AcceptTrial",
                "candidate-plan.json",
            )
        }
        "reject" => (ExperienceState::Rejected, "Reject", "baseline-plan.json"),
        _ => anyhow::bail!("experience_action_unsupported"),
    };
    writer.prepare(state, event)?;
    writer.commit(
        root,
        state,
        event,
        json!({"candidate_root":m["root"],"assessment":r,"selected_plan":selected,"selected_sha256":m["files"][selected]["sha256"],"reason":reason,"authority":"local private method trial only"}),
    )
}
/// Revalidate a registered method against its original frozen evaluator and trial.
/// Baseline selection grants no improvement claim. Candidates need actual gain.
pub fn registered_plan(root: &SkillRoot, path: &Path, role: &str) -> Result<Value> {
    let (dir, m, c, _) = checked(root, path)?;
    ensure!(
        m["protocol"] == "oev2.experience-freeze/2",
        "registered_method_requires_protocol_two"
    );
    let mut current_assessment = None;
    let selected = match role {
        "method_baseline" => "baseline-plan.json",
        "operation_method" => {
            let h = history(&events_dir(root, &c.candidate_id)?)?;
            ensure!(
                h.last().is_some_and(|e| e["state"] == "trial_active"),
                "registered_method_trial_not_active"
            );
            let current = assessment(root, path)?;
            ensure!(
                current["assessment"]["verdict"] == "admissible_trial",
                "registered_method_not_admissible"
            );
            current_assessment = Some(current);
            "candidate-plan.json"
        }
        _ => anyhow::bail!("registered_method_kind_unsupported"),
    };
    let p = plan(&read_regular_bytes(&dir.join(selected), LIMIT)?)?;
    ensure!(
        p.mode == "required_semantic_review" && p.schema.as_deref() == Some("oev2.method-plan/2"),
        "registered_method_cannot_disable_review"
    );
    let assurance = if role == "method_baseline" {
        json!({"path":oe_core::portable_source_path(root.data_path(), &dir.join("freeze.json"))?,
            "sha256":oe_core::sha256(read_regular_bytes(&dir.join("freeze.json"), LIMIT)?)})
    } else {
        let h = history(&events_dir(root, &c.candidate_id)?)?;
        let acceptance = h
            .iter()
            .rev()
            .find(|e| e["event"] == "AcceptTrial")
            .context("registered_method_acceptance_missing")?;
        let reference: Reference =
            serde_json::from_value(acceptance["body"]["assessment"].clone())?;
        let saved = strict_json(&read(root, &reference)?)?;
        ensure!(
            stable(&saved)
                == stable(
                    current_assessment
                        .as_ref()
                        .context("registered_method_assessment_missing")?
                ),
            "registered_method_assurance_changed"
        );
        serde_json::to_value(reference)?
    };
    Ok(
        json!({"schema":"oev2.registered-method-check/1","candidate_id":c.candidate_id,"assurance":assurance,
        "selected_plan":selected,"plan":oe_contracts::MethodPlan { schema:p.schema.unwrap(), mode:p.mode,
            retrieval_order:p.retrieval_order.context("registered_method_order_missing")? },
        "freeze_sha256":oe_core::sha256(read_regular_bytes(&dir.join("freeze.json"), LIMIT)?),
        "selected_sha256":m["files"][selected]["sha256"],"role":role,
        "scope":"bounded private diagnostic and retrieval method; no physical adoption"}),
    )
}

/// Used by the application version registry, reusing the same real trial executor.
pub fn apply_registered(
    root: &SkillRoot,
    path: &Path,
    role: &str,
    input: &Value,
    output: &Path,
) -> Result<Value> {
    let verified = registered_plan(root, path, role)?;
    apply_selected(
        root,
        path,
        input,
        output,
        Some(verified["selected_plan"].as_str().unwrap()),
        None,
    )
}

/// The application owner passes an exact selected event reference; it grants no authority by itself.
pub fn apply_registered_bound(
    root: &SkillRoot,
    path: &Path,
    role: &str,
    input: &Value,
    output: &Path,
    selection: &oe_contracts::ArtifactReference,
) -> Result<Value> {
    selection
        .validate()
        .map_err(|_| anyhow::anyhow!("registered_selection_reference_invalid"))?;
    root.private_path(Path::new(&selection.path))?;
    let reference = Reference {
        path: selection.path.clone(),
        sha256: selection.sha256.clone(),
    };
    let event = strict_json(&read(root, &reference)?)?;
    ensure!(
        event["schema"] == "oev2.version-selection-event/1"
            && event["kind"] == role
            && event["state"] == "active",
        "registered_selection_reference_mismatch"
    );
    let verified = registered_plan(root, path, role)?;
    apply_selected(
        root,
        path,
        input,
        output,
        Some(
            verified["selected_plan"]
                .as_str()
                .context("registered_selected_plan_missing")?,
        ),
        Some(selection),
    )
}

pub fn apply(root: &SkillRoot, path: &Path, input: &Value, output: &Path) -> Result<Value> {
    apply_selected(root, path, input, output, None, None)
}
fn apply_selected(
    root: &SkillRoot,
    path: &Path,
    input: &Value,
    output: &Path,
    registered: Option<&str>,
    version_selection: Option<&oe_contracts::ArtifactReference>,
) -> Result<Value> {
    let (dir, m, c, mut contract) = checked(root, path)?;
    let writer = HistoryWriter::acquire(root, &c.candidate_id)?;
    let last = writer.old.last().context("experience_history_missing")?;
    ensure!(
        registered.is_some()
            || ["trial_active", "restored", "rejected"]
                .contains(&last["state"].as_str().unwrap_or("")),
        "experience_trial_state_invalid"
    );
    let selected = match registered {
        Some(name) => name.to_owned(),
        None => last["body"]["selected_plan"]
            .as_str()
            .context("experience_selected_plan_missing")?
            .to_owned(),
    };
    let state: ExperienceState = serde_json::from_value(last["state"].clone())?;
    if registered.is_some() {
        ensure!(
            selected == "baseline-plan.json" || state == ExperienceState::TrialActive,
            "registered_candidate_trial_changed"
        );
    }
    let inherited_assessment = last["body"].get("assessment").cloned();
    if state == ExperienceState::Assessed {
        ensure!(
            registered == Some("baseline-plan.json")
                && inherited_assessment.as_ref().is_some_and(|v| !v.is_null()),
            "registered_assessment_context_missing"
        );
    }
    writer.prepare(state, "Feedback")?;
    let p = plan(&read_regular_bytes(&dir.join(&selected), LIMIT)?)?;
    let case: Case = serde_json::from_value(input.clone())?;
    ensure!(
        case.exposure == "development_replay",
        "experience_independent_exposure_not_attested"
    );
    ensure!(
        contract
            .cases
            .iter()
            .any(|original| original.bundle == case.bundle
                && original.query_ids == case.query_ids
                && original.shape_ids == case.shape_ids),
        "experience_trial_rule_family_not_frozen"
    );
    contract.cases = vec![case.clone()];
    contract.repetitions = 1;
    contract_valid(&contract)?;
    let run_root = root.new_private_dir(output)?;
    let mut files = BTreeMap::new();
    let mut case = case;
    case.project = snapshot(root, &run_root, &case.project, "project.json", &mut files)?;
    case.binding = snapshot(root, &run_root, &case.binding, "binding.json", &mut files)?;
    case.evidence = snapshot(root, &run_root, &case.evidence, "evidence.ttl", &mut files)?;
    if let Some(retrieval) = &mut case.retrieval {
        retrieval.replay = snapshot(
            root,
            &run_root,
            &retrieval.replay,
            "retrieval.json",
            &mut files,
        )?;
    }
    let mut trial_budget = contract.clone();
    trial_budget.cases = vec![case.clone()];
    full_check_budget(&run_root, &trial_budget)?;
    // Expectations belong to the feedback evaluator. The trial plan consumes only mode.
    let result = match run_case(root, &run_root, &case, &p, &contract.schema) {
        Ok(v) => v,
        Err(e) => {
            json!({"execution":"failed","conclusion":"unknown","coverage":[],"reason":format!("{e:#}"),"engineering_adoption":"blocked"})
        }
    };
    let record = json!({"schema":"oev2.experience-trial/1","candidate_id":c.candidate_id,"freeze_sha256":writer.old[0]["body"]["freeze_sha256"],"selected_plan":selected,"selected_sha256":m["files"][&selected]["sha256"],
        "task":case,"input_files":files,"status":if result["execution"]=="failed"{"failed"}else{"completed"},"result":result,"source_family":case.source_family,"transfer":"development_replay","feedback":"source-bound required-condition diagnosis; physical adoption and transfer remain unverified"});
    write_new(&run_root.join("trial.json"), &record)?;
    if let Some(selection) = version_selection {
        read(
            root,
            &Reference {
                path: selection.path.clone(),
                sha256: selection.sha256.clone(),
            },
        )?;
    }
    let mut body = json!({"candidate_root":m["root"],"selected_plan":selected,"selected_sha256":m["files"][&selected]["sha256"],
        "trial":Reference {path:oe_core::portable_source_path(root.data_path(),&run_root.join("trial.json"))?,sha256:oe_core::fingerprint_file(&run_root.join("trial.json"),LIMIT as u64)?.sha256},
        "feedback_status":record["status"],"exposure":case.exposure});
    if let Some(assessment) = inherited_assessment {
        body["assessment"] = assessment;
    }
    if let Some(selection) = version_selection {
        body["version_selection"] = serde_json::to_value(selection)?;
    }
    let event = writer.commit(root, state, "Feedback", body)?;
    Ok(json!({"status":record["status"],"trial":record,"event":event}))
}
pub fn recover(root: &SkillRoot, path: &Path, reason: &str, output: &Path) -> Result<Value> {
    let (dir, m, c, _) = checked(root, path)?;
    ensure!(
        !reason.trim().is_empty(),
        "experience_recovery_reason_missing"
    );
    let writer = HistoryWriter::acquire(root, &c.candidate_id)?;
    ensure!(
        writer
            .old
            .last()
            .is_some_and(|e| e["state"] == "trial_active"
                || (m["protocol"] == "oev2.experience-freeze/2" && e["state"] == "rejected")),
        "experience_no_active_trial_to_restore"
    );
    writer.prepare(ExperienceState::Restored, "Restore")?;
    let dest = root.new_private_dir(output)?;
    let bytes = read_regular_bytes(&dir.join("baseline-plan.json"), LIMIT)?;
    write_new_bytes(&dest.join("baseline-plan.json"), &bytes)?;
    ensure!(
        oe_core::fingerprint_file(&dest.join("baseline-plan.json"), LIMIT as u64)?.sha256
            == m["baseline_sha256"],
        "experience_recovery_identity_mismatch"
    );
    writer.commit(
        root,
        ExperienceState::Restored,
        "Restore",
        json!({"candidate_root":m["root"],"selected_plan":"baseline-plan.json","selected_sha256":sha256(bytes),"recovered_file":oe_core::portable_source_path(root.data_path(),&dest.join("baseline-plan.json"))?,"reason":reason}),
    )
}
pub fn list(root: &SkillRoot, query: &str) -> Result<Value> {
    let base = root.private_path(Path::new("var/state/knowledge/experience-events"))?;
    let mut rows = Vec::new();
    if base.is_dir() {
        for item in std::fs::read_dir(base)? {
            let p = item?.path();
            oe_core::no_symlinks(&p)?;
            if !p.is_dir() {
                continue;
            }
            let h = history(&p)?;
            if let Some(first) = h.first() {
                let candidate_path = first["body"]["candidate_root"]
                    .as_str()
                    .context("experience_root_missing")?;
                let candidate = serde_json::to_value(historical_candidate(root, first)?)?;
                if query.is_empty() || candidate.to_string().contains(query) {
                    rows.push(json!({"candidate":candidate,"state":h.last().map(|v|&v["state"]),"candidate_root":candidate_path,"event_count":h.len(),"proof":"immutable candidate query; no adoption implied"}));
                }
            }
        }
    }
    Ok(json!({"schema":"oev2.experience-query/1","results":rows,"promotion":"not_performed"}))
}

/// Same-Project controlled design comparisons; old retrieval protocols remain unchanged.
pub mod design_comparison;
