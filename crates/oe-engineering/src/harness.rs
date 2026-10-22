//! Immutable forward decisions and inverse candidate dossiers. This adapter does
//! not perform engineering actions, admit facts, create PackageDelta or promote.
use anyhow::{Context, Result, ensure};
use oe_core::json::{ordered_json, strict_json};
use oe_core::{SkillRoot, read_regular_bytes, sha256, text, write_new, write_new_bytes};
use oe_semantic::review::pointer_value;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

mod checkpoint;
mod model_calls;
pub use model_calls::{
    RustBaseStructuredHarness, RustConfiguredProviderConstructors, SharedModelCallContext,
    embed_provider, embed_provider_batch, embedding_trace, execute_native_model,
    factory_provider_request, generate_base_structured, generate_base_structured_with_delay,
    generate_factory_provider, generate_litellm, generate_provider, litellm_trace,
    native_model_trace, provider_call_time, provider_trace,
};
pub mod context;

pub use checkpoint::{CHECKPOINT_INPUT_SCHEMA, CHECKPOINT_STEP_SCHEMA};

pub const STEP_SCHEMA: &str = "ontology-engineering.harness-step/v1";
pub const DOSSIER_SCHEMA: &str = "ontology-engineering.consolidation/v1";
pub const CURRENT_DOSSIER_SCHEMA: &str = "oev2.consolidation/2";
pub const CONSOLIDATION_INPUT_SCHEMA: &str = "ontology-engineering.consolidation-input/v1";
pub const SHARED_CONSOLIDATION_INPUT_SCHEMA: &str = "oev2.consolidation-input/2";
pub const ARCHIVE_CONSOLIDATION_INPUT_SCHEMA: &str = "oev2.consolidation-input/3";
pub const MAX_CONSOLIDATION_BYTES: usize = 512_000;
const MAX_EMPTY_ARCHIVE_BYTES: usize = 16_777_216;
const MAX_EMPTY_ARCHIVE_TRAJECTORIES: usize = 16;
const MAX_EMPTY_ARCHIVE_ARTIFACTS: usize = 256;
const MAX_EMPTY_ARCHIVE_NODES: usize = 200_000;
const EXPANDED_EMPTY_ARCHIVE_NODES: usize = 524_288;
const MAX_EMPTY_ARCHIVE_DEPTH: usize = 96;
const MAX_EMPTY_ARCHIVE_DOCUMENT_BYTES: usize = 1_048_576;
/// A read is rejected, never truncated. Project artifacts stay under ROOT/var.
pub const MAX_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;

/// Public diagnostics carry only recognized contract codes, never arbitrary
/// parser fragments, source paths, provider messages or caller text.
pub fn safe_failure(error: &anyhow::Error) -> Option<Value> {
    let known = [
        "harness_reference_changed",
        "harness_artifact_changed",
        "harness_summary_mismatch",
        "harness_context_archive_mismatch",
        "harness_previous_snapshot_mismatch",
        "invalid_harness_context_policy",
        "changed_state_requires_reason",
        "changed_evaluation_baseline_requires_reason",
        "previous_evaluation_baseline_not_established_requires_migration_reason",
        "previous_step_project_id_mismatch",
        "previous_step_decision_id_mismatch",
        "previous_step_object_id_mismatch",
        "review_subject_revision_mismatch",
        "review_budget_exceeded",
        "invalid_review_requests",
        "invalid_attempt_limit",
        "semantic_modules_have_two_owners",
        "harness_shared_reference_missing",
        "harness_shared_context_cycle",
        "harness_shared_reference_digest_mismatch",
        "harness_context_expansion_mismatch",
        "harness_context_structure_budget_exceeded",
        "harness_context_expansion_budget_exceeded",
        "invalid_harness_context_rows",
        "invalid_harness_shared_context",
        "invalid_harness_shared_reference",
        "invalid_consolidation_archive_input",
        "invalid_consolidation_archive_policy",
        "consolidation_archive_requires_empty_candidates",
        "consolidation_archive_byte_budget_exceeded",
        "consolidation_archive_trajectory_budget_exceeded",
        "consolidation_archive_artifact_budget_exceeded",
        "consolidation_archive_structure_budget_exceeded",
        "consolidation_archive_document_budget_exceeded",
        "credential_material_in_trajectory",
        "invalid_harness_checkpoint_input",
        "invalid_harness_checkpoint_policy",
        "harness_checkpoint_authority_mismatch",
        "harness_checkpoint_document_budget_exceeded",
        "harness_checkpoint_byte_budget_exceeded",
        "harness_checkpoint_artifact_budget_exceeded",
        "harness_checkpoint_structure_budget_exceeded",
        "harness_checkpoint_json_source_invalid",
    ];
    for cause in error.chain() {
        let reason = if let Some(value) = cause.downcast_ref::<oe_core::json::JsonError>() {
            Some(value.to_string())
        } else {
            let message = cause.to_string();
            known
                .iter()
                .find(|code| message == **code || message == format!("invalid_{code}"))
                .map(|code| (*code).to_owned())
        };
        if let Some(reason) = reason {
            return Some(
                json!({"code":reason,"reason":reason,"kind":"recognized_contract_failure"}),
            );
        }
    }
    None
}

fn review_failure_feedback(requests: &Value, error: &anyhow::Error, stage: &str) -> Value {
    json!({"schema":"ontology-engineering.semantic-feedback/v1","status":"blocked","reviews":[],
        "error_type":"review_input_or_execution_error","outstanding_requests":model_projection(requests),
        "reason":"review_input_or_execution_failed_inspect_bound_records_and_partial_artifacts",
        "failure_stage":stage,"diagnostic":safe_failure(error).unwrap_or(json!({"code":"review_execution_contract_failed","kind":"unclassified_safe_failure"})),
        "review_execution":if stage=="review_context_association" {"not_started"}else{"partial_or_failed"},
        "partial_review_artifacts":if stage=="review_context_association" {"not_created_before_association_completed"}else{"inspect_bound_semantic_feedback_if_present"},
        "dependent_adoption":"blocked","fact_admitted":false})
}

/// A review adapter must return actual scoped feedback or an error. Fixtures
/// retain their execution identity inside that feedback; they are not native PASS.
pub trait ReviewExecutor {
    fn execute(
        &self,
        root: &Path,
        requests: &Value,
        project_id: &str,
        output: &Path,
    ) -> Result<Value>;
}

pub struct NativeReviews;
impl ReviewExecutor for NativeReviews {
    fn execute(
        &self,
        root: &Path,
        requests: &Value,
        project_id: &str,
        output: &Path,
    ) -> Result<Value> {
        oe_semantic::review::execute_reviews(root, requests, project_id, output)
    }
}

fn exact_keys(value: &Value, required: &[&str], optional: &[&str], error: &str) -> Result<()> {
    let map = value.as_object().with_context(|| error.to_owned())?;
    ensure!(
        required.iter().all(|k| map.contains_key(*k))
            && map
                .keys()
                .all(|k| required.contains(&k.as_str()) || optional.contains(&k.as_str())),
        "{error}"
    );
    Ok(())
}

fn strings(value: &Value, name: &str) -> Result<Value> {
    let values = value
        .as_array()
        .with_context(|| format!("invalid_{name}"))?;
    for value in values {
        text(value, name)?;
    }
    Ok(value.clone())
}

fn identity(harness: &Value) -> Result<Value> {
    let fields = [
        "project_id",
        "decision_id",
        "object_id",
        "revision",
        "problem_class",
        "goal",
    ];
    exact_keys(
        harness,
        &[
            "project_id",
            "decision_id",
            "object_id",
            "revision",
            "problem_class",
            "goal",
            "conditions",
            "constraints",
            "unknowns",
        ],
        &[
            "semantic_modules",
            "review_requests",
            "observations",
            "previous",
            "change_reason",
            "current_project",
            "project_context",
            "context_policy",
            "checkpoint_policy",
        ],
        "invalid_harness_state",
    )?;
    let mut state = Map::new();
    for field in fields {
        text(&harness[field], field)?;
        state.insert(field.into(), harness[field].clone());
    }
    for field in ["conditions", "constraints", "unknowns"] {
        state.insert(field.into(), strings(&harness[field], field)?);
    }
    Ok(Value::Object(state))
}

fn read_ref(root: &SkillRoot, reference: &Value) -> Result<(PathBuf, Vec<u8>)> {
    exact_keys(
        reference,
        &["path", "sha256"],
        &[],
        "invalid_harness_reference",
    )?;
    let path = root.private_path(Path::new(text(
        &reference["path"],
        "harness_reference_path",
    )?))?;
    let raw = read_regular_bytes(&path, MAX_ARTIFACT_BYTES)?;
    ensure!(
        sha256(&raw) == reference["sha256"],
        "harness_reference_changed"
    );
    Ok((path, raw))
}

fn artifact_path(root: &SkillRoot, directory: &Path, relative: &str) -> Result<PathBuf> {
    // Do not normalize aliases or backslashes into another archived identity.
    oe_semantic::safe_relative(relative).context("invalid_harness_artifact_path")?;
    let target = root.private_path(&directory.join(relative))?;
    ensure!(target.starts_with(directory), "harness_artifact_changed");
    Ok(target)
}

fn read_artifact(
    root: &SkillRoot,
    directory: &Path,
    relative: &str,
    expected: &Value,
) -> Result<Vec<u8>> {
    let target = artifact_path(root, directory, relative)?;
    let raw = read_regular_bytes(&target, MAX_ARTIFACT_BYTES)?;
    ensure!(sha256(&raw) == *expected, "harness_artifact_changed");
    Ok(raw)
}

/// Verify the exact historical record and every declared artifact. No previous
/// verdict is reinterpreted as current evidence or acceptance.
pub fn load_step(root: &SkillRoot, reference: &Value) -> Result<Value> {
    let (path, raw) = read_ref(root, reference)?;
    let result = strict_json(&raw)?;
    ensure!(
        result["schema"] == CHECKPOINT_STEP_SCHEMA,
        "not_a_harness_checkpoint"
    );
    ensure!(
        result["engineering_acceptance"] == "not_determined"
            && result["ontology_promotion"] == "not_performed"
            && result["engineering_execution"] == "agent_and_tool_adapters",
        "harness_authority_mismatch"
    );
    let artifacts = result["artifacts"]
        .as_object()
        .context("invalid_harness_artifacts")?;
    for (relative, expected) in artifacts {
        read_artifact(
            root,
            path.parent().context("missing_parent")?,
            relative,
            expected,
        )?;
    }
    // Hashes establish byte identity, not authorship. Also require one internally
    // consistent archive: a rewritten summary cannot override bound originals.
    let original = |name: &str| -> Result<Value> {
        let expected = artifacts
            .get(name)
            .context("harness_required_artifact_missing")?;
        Ok(strict_json(&read_artifact(
            root,
            path.parent().context("missing_parent")?,
            name,
            expected,
        )?)?)
    };
    checkpoint::load(root, &path, &result, &original)
}

/// Review methods can be selected without premise snapshots. Bind their locked
/// package and evaluator separately from changing project records or sources.
fn review_baseline(root: &SkillRoot, requests: &Value) -> Value {
    let Some(requests) = requests.as_array() else {
        return json!({"status":"not_established"});
    };
    if requests.is_empty() {
        return json!({"status":"not_requested"});
    }
    if requests.len() > oe_semantic::review::MAX_REVIEWS {
        return json!({"status":"not_established", "reason":"review_budget_exceeded"});
    }
    let runtime = match oe_semantic::runtime::read_runtime_source_lock(root.path(), false) {
        Ok(lock) => {
            json!({"status":"source_lock_only_not_execution", "commit":lock.commit, "version":lock.version, "artifact_sha256":lock.artifact_sha256})
        }
        Err(_) => json!({"status":"not_established"}),
    };
    let mut methods = BTreeMap::new();
    for request in requests {
        let name = request
            .get("bundle")
            .cloned()
            .unwrap_or(json!(oe_semantic::review::DEFAULT_BUNDLE));
        let package = name
            .as_str()
            .and_then(|name| oe_semantic::load_bundle(root.path(), name).ok())
            .map(|bundle| {
                let mut identity = Map::new();
                for field in [
                    "package_id",
                    "package_version",
                    "package_sha256",
                    "candidate_sha256",
                    "sha256",
                    "manifest_sha256",
                    "runtime",
                ] {
                    identity.insert(field.into(), bundle.spec[field].clone());
                }
                Value::Object(identity)
            })
            .unwrap_or(json!({"status":"not_established"}));
        let binding = read_ref(
            root,
            &json!({"path":request["binding"], "sha256":request["binding_sha256"]}),
        )
        .and_then(|(_, bytes)| Ok(strict_json(&bytes)?));
        let declared = match binding {
            Ok(binding) => {
                json!({"package_id":binding["semantic_target"]["package_id"], "baseline":binding["baseline"], "semantic_api":binding["semantic_api"]})
            }
            Err(_) => json!({"status":"not_established"}),
        };
        let method =
            json!({"bundle":name, "package":package, "binding_evaluation_context":declared});
        methods.insert(
            oe_core::json::canonical_digest(&method).expect("JSON method identity"),
            method,
        );
    }
    json!({"status":"declared_methods_not_execution", "runtime_source":runtime, "methods":methods.into_values().collect::<Vec<_>>()})
}

fn observations(root: &SkillRoot, value: &Value) -> Result<(Value, Vec<Value>)> {
    let values = value.as_array().context("invalid_observations")?;
    let mut result = Vec::new();
    let mut refs = Vec::new();
    for item in values {
        exact_keys(
            item,
            &["kind", "statement", "evidence"],
            &[],
            "invalid_observation",
        )?;
        ensure!(
            item["kind"].as_str().is_some_and(|kind| [
                "reported",
                "observed",
                "hypothesis",
                "decision"
            ]
            .contains(&kind)),
            "invalid_observation_kind"
        );
        text(&item["statement"], "observation_statement")?;
        let evidence = item["evidence"]
            .as_array()
            .context("invalid_observation_evidence")?;
        let mut hashes = Vec::new();
        for reference in evidence {
            let (_, raw) = read_ref(root, reference)?;
            hashes.push(sha256(raw));
            refs.push(reference.clone());
        }
        result.push(json!({"kind":item["kind"], "statement":item["statement"],
            "evidence_sha256":hashes, "truth_status":"source_statement_not_admitted"}));
    }
    Ok((json!(result), refs))
}

fn collect_files(
    root: &SkillRoot,
    output: &Path,
    directory: &Path,
    paths: &mut Vec<String>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = root.private_path(&entry.path())?;
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "harness_artifact_symlink");
        if kind.is_dir() {
            collect_files(root, output, &path, paths)?;
        } else {
            ensure!(kind.is_file(), "harness_artifact_not_regular");
            let parts: Result<Vec<_>> = path
                .strip_prefix(output)?
                .components()
                .map(|component| match component {
                    Component::Normal(value) => value.to_str().context("non_utf8_artifact_path"),
                    _ => anyhow::bail!("invalid_harness_artifact_path"),
                })
                .collect();
            let relative = parts?.join("/");
            oe_semantic::safe_relative(&relative)?;
            paths.push(relative);
        }
    }
    Ok(())
}

fn artifact_map(root: &SkillRoot, output: &Path) -> Result<Value> {
    let mut paths = Vec::new();
    collect_files(root, output, output, &mut paths)?;
    paths.sort();
    let mut result = Map::new();
    for relative in paths {
        let path = artifact_path(root, output, &relative)?;
        result.insert(
            relative,
            json!(sha256(read_regular_bytes(&path, MAX_ARTIFACT_BYTES)?)),
        );
    }
    Ok(Value::Object(result))
}

/// Known locator fields remain exact in the private archive; only their opaque
/// hashes enter model state. Free text (including diagnostic stderr) is retained,
/// not comprehensively path-sanitized. Semantic statements remain unchanged.
pub fn model_projection(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(model_projection).collect()),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(key, value)| {
                    let projected = if let Some(locator) = value.as_str().filter(|_| {
                        [
                            "path",
                            "record",
                            "binding",
                            "workspace",
                            "evidence_root",
                            "skill_path",
                        ]
                        .contains(&key.as_str())
                            || key.ends_with("_path")
                    }) {
                        // Historical digest(str) hashes JSON string bytes, including
                        // quotes/escaping, rather than the raw UTF-8 locator.
                        json!(format!(
                            "local-ref:{}",
                            sha256(ordered_json(&json!(locator)).expect("JSON string encoding"))
                        ))
                    } else {
                        model_projection(value)
                    };
                    (key.clone(), projected)
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn associate_reviews(
    root: &SkillRoot,
    requests: &Value,
    state: &Value,
    bindings: &mut Vec<Value>,
) -> Result<()> {
    let requests = requests.as_array().context("invalid_review_requests")?;
    ensure!(
        requests.len() <= oe_semantic::review::MAX_REVIEWS,
        "review_budget_exceeded"
    );
    for request in requests {
        let (_, raw) = read_ref(
            root,
            &json!({"path":request["record"], "sha256":request["record_sha256"]}),
        )?;
        let record = strict_json(&raw)?;
        ensure!(
            record["claim"]["subject_revision"] == state["revision"],
            "review_subject_revision_mismatch"
        );
        let mut association = json!({"id":request["id"], "subject_revision":"matched",
            "subject_id":"not_established", "decision_id":"not_established"});
        for (field, expected) in [
            ("subject_id", &state["object_id"]),
            ("decision_id", &state["decision_id"]),
        ] {
            let reference = &record["facts"][field];
            if reference.is_null() {
                continue;
            }
            let source = record["sources"]
                .as_array()
                .context("invalid_review_sources")?
                .iter()
                .find(|s| s["id"] == reference["source_id"])
                .context("review_source_missing")?;
            let evidence_root =
                root.private_path(Path::new(text(&request["evidence_root"], "evidence_root")?))?;
            let relative = text(&source["path"], "review_source_path")?;
            oe_semantic::safe_relative(relative).context("review_source_escapes_evidence_root")?;
            let path = evidence_root.join(relative);
            let (_, bytes) = read_ref(root, &json!({"path":path, "sha256":source["sha256"]}))?;
            let facts = strict_json(&bytes)?;
            ensure!(
                pointer_value(
                    &facts,
                    reference["pointer"]
                        .as_str()
                        .context("invalid_review_pointer")?
                )? == expected,
                "review_{field}_mismatch"
            );
            association[field] = json!("matched_source_statement");
        }
        bindings.push(association);
    }
    Ok(())
}

fn implementation() -> Result<Value> {
    Ok(
        json!({"language":"rust", "version":env!("CARGO_PKG_VERSION"),
        "source_sha256":sha256(include_bytes!("harness.rs")),
        "executable_sha256":sha256(fs::read(std::env::current_exe()?)?)}),
    )
}

/// Execute requested scoped native reviews before bounded capability judgment.
/// None transport is recorded as unavailable; credentials belong to the caller.
pub fn step(root: &SkillRoot, document: &Value, output: &Path) -> Result<Value> {
    step_with_reviewer(root, document, output, &NativeReviews)
}

pub fn step_with_reviewer(
    root: &SkillRoot,
    document: &Value,
    output: &Path,
    reviewer: &dyn ReviewExecutor,
) -> Result<Value> {
    let mut doc = document.clone();
    let harness = doc
        .as_object_mut()
        .context("invalid_context_input")?
        .shift_remove("harness")
        .unwrap_or(Value::Null);
    let state = identity(&harness)?;
    let checkpoint_policy = checkpoint::policy(document)?;
    ensure!(checkpoint_policy.is_some(), "checkpoint_input_required");
    if let Some(policy) = &checkpoint_policy {
        checkpoint::preflight_sources(root, document, policy)?;
    }
    ensure!(
        checkpoint_policy.is_some() || harness.get("checkpoint_policy").is_none(),
        "invalid_harness_checkpoint_policy"
    );
    let (current_check, engineering_checks) = if let Some(reference) =
        harness.get("current_project")
    {
        let (_, raw) = read_ref(root, reference)?;
        let project: oe_kernel::project::Project = serde_json::from_value(strict_json(&raw)?)?;
        let context: Option<oe_contracts::ProjectReviewContext> = harness
            .get("project_context")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?;
        let object = oe_contracts::ObjectIdentity {
            object_id: state["object_id"].as_str().unwrap().into(),
            revision: state["revision"].as_str().unwrap().into(),
            condition_scope: context
                .as_ref()
                .map(|c| c.condition_scope.clone())
                .unwrap_or_default(),
            source_sha256: context
                .as_ref()
                .map(|c| c.source_sha256.clone())
                .unwrap_or_default(),
        };
        let mut check =
            oe_kernel::project::check_current_context(&project, &object, context.as_ref());
        if state["project_id"] != project.project_id
            || project
                .conditions
                .get(&object.condition_scope)
                .is_none_or(|content| state["conditions"] != json!([content]))
        {
            check.conclusion = "violated".into();
            check
                .failures
                .push("harness_conditions_do_not_match_current_project".into());
            check.result["current_use"] = json!("blocked");
        }
        let engineering_checks = oe_kernel::engineering::checks_current(root, &project, &object);
        (Some(check), engineering_checks)
    } else {
        (None, Vec::new())
    };
    ensure!(
        doc.get("semantic_modules").is_none(),
        "semantic_modules_have_two_owners"
    );
    let empty = json!([]);
    let (observations, observation_refs) =
        observations(root, harness.get("observations").unwrap_or(&empty))?;
    let previous_ref = harness.get("previous").unwrap_or(&Value::Null);
    let mut previous = Value::Null;
    let mut changes = Map::new();
    if !previous_ref.is_null() {
        previous = load_step(root, previous_ref)?;
        for field in ["project_id", "decision_id", "object_id"] {
            ensure!(
                previous["state"][field] == state[field],
                "previous_step_{field}_mismatch"
            );
        }
        for (field, value) in state.as_object().unwrap() {
            if previous["state"][field] != *value {
                changes.insert(
                    field.clone(),
                    json!({"before":previous["state"][field], "after":value}),
                );
            }
        }
        if !changes.is_empty() {
            text(&harness["change_reason"], "changed_state_requires_reason")?;
        }
    }
    let premises = oe_semantic::build_premises(
        root.path(),
        harness.get("semantic_modules").unwrap_or(&empty),
    )?;
    // Contract identity deliberately excludes executable identity: a recompile
    // alone is not a new evaluation baseline. Historical Python records without
    // this field require an explicit migration reason rather than guessed parity.
    let baseline = json!({"schema":"oev2.harness-checkpoint-baseline/1",
        "semantic_premises_sha256":premises["snapshot_sha256"],
        "review_methods":review_baseline(root, harness.get("review_requests").unwrap_or(&empty)),
        "checkpoint_policy":checkpoint_policy,"model_execution":"not_run"});
    let mut baseline_changes = Map::new();
    if !previous.is_null() {
        if let Some(prior) = previous.get("baseline").filter(|v| v.is_object()) {
            let fields: BTreeSet<_> = baseline
                .as_object()
                .unwrap()
                .keys()
                .chain(prior.as_object().unwrap().keys())
                .collect();
            for field in fields {
                let value = &baseline[field];
                if prior[field] != *value {
                    baseline_changes
                        .insert(field.clone(), json!({"before":prior[field], "after":value}));
                }
            }
            if !baseline_changes.is_empty() {
                text(
                    &harness["change_reason"],
                    "changed_evaluation_baseline_requires_reason",
                )?;
            }
        } else {
            text(
                &harness["change_reason"],
                "previous_evaluation_baseline_not_established_requires_migration_reason",
            )?;
            baseline_changes.insert(
                "baseline".into(),
                json!({"before":{"status":"not_established"}, "after":baseline}),
            );
        }
    }
    let output = root.new_private_dir(output)?;
    write_new(&output.join("input.json"), document)?;
    write_new(&output.join("premises.json"), &premises)?;
    if checkpoint_policy.is_some() {
        if let Some(reference) = harness.get("current_project") {
            let (_, raw) = read_ref(root, reference)?;
            write_new_bytes(&output.join("current-project.json"), &raw)?;
        }
    }
    if !previous_ref.is_null() {
        let (_, raw) = read_ref(root, previous_ref)?;
        write_new_bytes(&output.join("previous-iteration.json"), &raw)?;
    }
    if !observation_refs.is_empty() {
        let snapshots = root.new_private_dir(&output.join("observations"))?;
        let mut copied = BTreeSet::new();
        for reference in &observation_refs {
            let (_, raw) = read_ref(root, reference)?;
            let hash = sha256(&raw);
            if copied.insert(hash.clone()) {
                write_new_bytes(&snapshots.join(hash), &raw)?;
            }
        }
    }
    let requests = harness.get("review_requests").unwrap_or(&empty);
    let mut review_bindings = Vec::new();
    let mut feedback = match associate_reviews(root, requests, &state, &mut review_bindings) {
        Ok(()) => match reviewer.execute(
            root.path(),
            requests,
            state["project_id"].as_str().unwrap(),
            &output.join("semantic-feedback"),
        ) {
            Ok(feedback) => feedback,
            Err(error) => review_failure_feedback(requests, &error, "scoped_review_execution"),
        },
        Err(error) => review_failure_feedback(requests, &error, "review_context_association"),
    };
    feedback["current_project_check"] = json!(current_check);
    feedback["engineering_checks"] = json!(engineering_checks);
    let current_support = current_check
        .as_ref()
        .is_some_and(|c| c.conclusion == "pass")
        && engineering_checks.iter().all(|c| c.conclusion == "pass");
    feedback["current_project_use"] = json!(if current_support {
        "content_bound_engineering_acceptance_separate"
    } else {
        "blocked_or_historical_only"
    });
    if !current_support {
        feedback["dependent_adoption"] = json!("blocked");
    }
    write_new(&output.join("feedback.json"), &feedback)?;
    let mut semantic_state = json!({"premises":premises, "current_feedback":feedback, "project_state":state,
        "baseline":baseline, "baseline_changes":baseline_changes,
        "observations":observations, "previous":null, "changes":changes, "review_context_association":review_bindings,
        "association_meaning":"Missing subject/decision bindings remain unestablished; scoped review is not current object acceptance."});
    semantic_state["current_project_check"] = json!(current_check);
    semantic_state["engineering_checks"] = json!(engineering_checks);
    semantic_state["current_use"] = json!(if current_support {
        "content_bound; native/physical acceptance separate"
    } else {
        "blocked_or_historical_only"
    });
    if !previous.is_null() {
        semantic_state["previous"] = previous_context(&previous, previous_ref);
    }
    write_new(&output.join("semantic-state.json"), &semantic_state)?;
    if let Some(checkpoint_policy) = &checkpoint_policy {
        let result = json!({"schema":CHECKPOINT_STEP_SCHEMA,"task_id":doc["task_id"],"state":state,
            "status":"recorded","model_execution":"not_run","model_calls":0,
            "candidate_routes":[],"uncertain_routes":[],
            "observations":observations,"observation_refs":observation_refs,"premises":premises,"feedback":feedback,
            "review_context_association":review_bindings,"previous":previous_ref,"changes":changes,"change_reason":harness["change_reason"],
            "current_project_check":current_check,"engineering_checks":engineering_checks,
            "current_use":if current_support {"content_bound; engineering acceptance separate"} else {"blocked_or_historical_only"},
            "baseline":baseline,"baseline_changes":baseline_changes,"checkpoint_policy":checkpoint_policy,
            "engineering_execution":"agent_and_tool_adapters","engineering_acceptance":"not_determined","ontology_promotion":"not_performed",
            "harness_code_sha256":sha256(include_bytes!("harness.rs")),"implementation":implementation()?,
            "artifacts":artifact_map(root,&output)?});
        checkpoint::validate_stored(root, &output, &result, checkpoint_policy, None)?;
        write_new(&output.join("iteration.json"), &result)?;
        return Ok(result);
    }
    anyhow::bail!("checkpoint_input_required")
}

fn previous_context(previous: &Value, reference: &Value) -> Value {
    json!({"iteration_id":previous["task_id"], "sha256":reference["sha256"],
        "state":previous["state"], "observations":previous["observations"], "candidate_routes":previous["candidate_routes"],
        "feedback":previous["feedback"], "review_context_association":previous["review_context_association"],
        "baseline":previous.get("baseline").cloned().unwrap_or(json!({"status":"not_established"})),
        "applicability":"historical_only_requires_current_evidence", "adoption_inherited":false})
}

/// The load_step caller has already bound feedback to its original artifact.
/// A parse failure is data only for exact failed-review path/hash pairs, never
/// for normal artifacts or a review also recorded as completed.
fn failed_artifact_bindings(
    root: &SkillRoot,
    episode: &Value,
    directory: &Path,
) -> Result<BTreeMap<String, Value>> {
    let empty = json!([]);
    let failures = episode["feedback"]
        .get("failed_reviews")
        .unwrap_or(&empty)
        .as_array()
        .context("invalid_failed_reviews")?;
    let mut allowed = BTreeMap::new();
    if failures.is_empty() {
        return Ok(allowed);
    }
    let mut completed_paths = BTreeSet::new();
    for review in episode["feedback"]["reviews"]
        .as_array()
        .context("invalid_completed_reviews")?
    {
        if let Some(artifacts) = review.get("artifacts") {
            for reference in artifacts
                .as_object()
                .context("invalid_completed_review_artifacts")?
                .values()
            {
                completed_paths.insert(root.private_path(Path::new(text(
                    &reference["path"],
                    "completed_review_artifact_path",
                )?))?);
            }
        }
    }
    for failure in failures {
        let id = text(&failure["id"], "failed_review_id")?;
        ensure!(
            id.len() <= 80
                && id.as_bytes()[0].is_ascii_alphanumeric()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
            "invalid_failed_review_id"
        );
        ensure!(
            failure["status"] == "blocked",
            "invalid_failed_review_status"
        );
        let failed_directory = directory
            .join("semantic-feedback")
            .join(format!("review-{id}"));
        for (name, reference) in failure["artifacts"]
            .as_object()
            .context("invalid_failed_review_artifacts")?
        {
            exact_keys(
                reference,
                &["path", "sha256"],
                &[],
                "invalid_failed_review_artifact_reference",
            )?;
            let target = root.private_path(Path::new(text(
                &reference["path"],
                "failed_review_artifact_path",
            )?))?;
            ensure!(
                target.parent() == Some(failed_directory.as_path())
                    && target.file_name().and_then(|v| v.to_str()) == Some(name.as_str()),
                "failed_review_artifact_path_mismatch"
            );
            ensure!(
                !completed_paths.contains(&target),
                "failed_review_artifact_also_completed"
            );
            let relative = format!("semantic-feedback/review-{id}/{name}");
            ensure!(
                artifact_path(root, directory, &relative)? == target,
                "failed_review_artifact_path_mismatch"
            );
            ensure!(
                episode["artifacts"].get(&relative) == Some(&reference["sha256"]),
                "failed_review_artifact_hash_mismatch"
            );
            allowed.insert(relative, reference["sha256"].clone());
        }
    }
    Ok(allowed)
}

struct EmptyArchiveBudget {
    limit: usize,
    node_limit: usize,
    bytes: usize,
    artifacts: usize,
    nodes: usize,
}
fn archive_has_credential_marker(bytes: &[u8]) -> bool {
    bytes.windows(7).any(|v| v == b"apikey_")
        || bytes
            .windows(b"PRIVATE KEY-----".len())
            .any(|v| v == b"PRIVATE KEY-----")
}
impl EmptyArchiveBudget {
    fn read(&mut self, path: &Path) -> Result<Vec<u8>> {
        let remaining = self.limit.saturating_sub(self.bytes);
        let size = fs::symlink_metadata(path)?.len();
        ensure!(
            size <= remaining as u64,
            "consolidation_archive_byte_budget_exceeded"
        );
        let bytes = read_regular_bytes(path, remaining)?;
        self.bytes += bytes.len();
        ensure!(
            !archive_has_credential_marker(&bytes),
            "credential_material_in_trajectory"
        );
        Ok(bytes)
    }
    fn structure(&mut self, value: &Value, depth: usize) -> Result<()> {
        self.nodes += 1;
        ensure!(
            self.nodes <= self.node_limit && depth <= MAX_EMPTY_ARCHIVE_DEPTH,
            "consolidation_archive_structure_budget_exceeded"
        );
        match value {
            Value::Array(items) => {
                for item in items {
                    self.structure(item, depth + 1)?;
                }
            }
            Value::Object(items) => {
                for (key, item) in items {
                    ensure!(
                        !archive_has_credential_marker(key.as_bytes()),
                        "credential_material_in_trajectory"
                    );
                    self.structure(item, depth + 1)?;
                }
            }
            Value::String(value) => ensure!(
                !archive_has_credential_marker(value.as_bytes()),
                "credential_material_in_trajectory"
            ),
            _ => {}
        }
        Ok(())
    }
}

/// An explicit, bounded archive of an author's empty candidate declaration.
/// All original checks retain their historical meaning; none is re-executed or
/// relabelled as current engineering acceptance. No model request is constructed.
fn archive_empty_checkpoint(
    root: &SkillRoot,
    document: &Value,
    output: &Path,
    current: bool,
) -> Result<Value> {
    exact_keys(
        document,
        &[
            "schema",
            "checkpoint_id",
            "problem_class",
            "reason",
            "iterations",
            "candidates",
            "archive_policy",
        ],
        &[],
        "invalid_consolidation_archive_input",
    )?;
    for field in ["checkpoint_id", "problem_class", "reason"] {
        text(&document[field], field)?;
    }
    ensure!(
        ordered_json(document)?.len() <= MAX_EMPTY_ARCHIVE_DOCUMENT_BYTES,
        "consolidation_archive_document_budget_exceeded"
    );
    ensure!(
        document["candidates"].as_array().is_some_and(Vec::is_empty),
        "consolidation_archive_requires_empty_candidates"
    );
    exact_keys(
        &document["archive_policy"],
        &["schema", "mode", "byte_budget"],
        &[],
        "invalid_consolidation_archive_policy",
    )?;
    let policy: oe_contracts::ConsolidationArchivePolicy =
        serde_json::from_value(document["archive_policy"].clone())
            .context("invalid_consolidation_archive_policy")?;
    policy
        .validate()
        .map_err(|_| anyhow::anyhow!("invalid_consolidation_archive_policy"))?;
    ensure!(
        (1..=MAX_EMPTY_ARCHIVE_BYTES as u64).contains(&policy.byte_budget),
        "invalid_consolidation_archive_policy"
    );
    let refs = document["iterations"]
        .as_array()
        .filter(|items| !items.is_empty())
        .context("consolidation_requires_trajectories")?;
    ensure!(
        refs.len() <= MAX_EMPTY_ARCHIVE_TRAJECTORIES,
        "consolidation_archive_trajectory_budget_exceeded"
    );
    let mut budget = EmptyArchiveBudget {
        limit: policy.byte_budget as usize,
        node_limit: if policy.schema == "oev2.consolidation-archive-policy/2" {
            EXPANDED_EMPTY_ARCHIVE_NODES
        } else {
            MAX_EMPTY_ARCHIVE_NODES
        },
        bytes: 0,
        artifacts: 0,
        nodes: 0,
    };
    budget.structure(document, 0)?;
    let mut seen = BTreeSet::new();
    let mut copies = Vec::new();
    let mut trajectories = Vec::new();
    for (index, reference) in refs.iter().enumerate() {
        exact_keys(
            reference,
            &["path", "sha256"],
            &[],
            "invalid_harness_reference",
        )?;
        ensure!(
            seen.insert(text(&reference["sha256"], "trajectory_sha256")?),
            "duplicate_consolidation_episode"
        );
        let path = root.private_path(Path::new(text(
            &reference["path"],
            "harness_reference_path",
        )?))?;
        let raw = budget.read(&path)?;
        ensure!(
            sha256(&raw) == reference["sha256"],
            "harness_reference_changed"
        );
        let original = strict_json(&raw)?;
        budget.structure(&original, 0)?;
        let artifacts = original["artifacts"]
            .as_object()
            .context("invalid_harness_artifacts")?;
        budget.artifacts += artifacts.len();
        ensure!(
            budget.artifacts <= MAX_EMPTY_ARCHIVE_ARTIFACTS,
            "consolidation_archive_artifact_budget_exceeded"
        );
        let directory = path.parent().context("missing_parent")?;
        let mut parsed_artifacts = Vec::new();
        let step_copy = format!("trajectories/{index:02}/iteration.json");
        copies.push((step_copy.clone(), path.clone(), raw));
        for (relative, expected) in artifacts {
            ensure!(
                relative != "iteration.json",
                "invalid_harness_artifact_path"
            );
            let source = artifact_path(root, directory, relative)?;
            let raw = budget.read(&source)?;
            ensure!(sha256(&raw) == *expected, "harness_artifact_changed");
            // Content, not the artifact's extension, determines JSON resource
            // accounting. Observation snapshots are deliberately hash-named.
            match strict_json(&raw) {
                Ok(value) => {
                    budget.structure(&value, 0)?;
                }
                Err(_)
                    if relative.ends_with(".json")
                        || raw
                            .iter()
                            .copied()
                            .find(|b| !b.is_ascii_whitespace())
                            .is_some_and(|b| b == b'{' || b == b'[') =>
                {
                    // Only an exact failed-review path/hash may preserve an
                    // unparseable JSON source as opaque historical evidence.
                    parsed_artifacts.push((relative.clone(), expected.clone()));
                }
                Err(_) => {} // Original binary/text bytes still count in full.
            }
            copies.push((format!("trajectories/{index:02}/{relative}"), source, raw));
        }
        // Resource preflight never substitutes for the original archive checks.
        let episode = load_step(root, reference)?;
        ensure!(episode == original, "harness_reference_changed");
        ensure!(
            episode["state"]["problem_class"] == document["problem_class"],
            "consolidation_problem_class_mismatch"
        );
        let failed = failed_artifact_bindings(root, &episode, directory)?;
        for (name, expected) in parsed_artifacts {
            ensure!(
                failed.get(&name) == Some(&expected),
                "invalid_json_in_consolidation_archive"
            );
        }
        trajectories.push(json!({"original":reference,"archived_step":step_copy,
            "status":episode["status"],"current_project_check":episode["current_project_check"],
            "engineering_checks":episode["engineering_checks"],"current_use":episode["current_use"],
            "meaning":"historical_checks_not_reexecuted"}));
    }
    // Every validation completes before output creation. A later I/O failure
    // may leave partial files, but never a successful terminal receipt.
    let output = root.new_private_dir(output)?;
    write_new(&output.join("input.json"), document)?;
    let mut manifest = Vec::new();
    for (relative, source, raw) in copies {
        let destination = artifact_path(root, &output, &relative)?;
        oe_core::ensure_directory(destination.parent().context("missing_parent")?)?;
        write_new_bytes(&destination, &raw)?;
        let digest = sha256(&raw);
        ensure!(
            sha256(read_regular_bytes(&destination, raw.len())?) == digest,
            "harness_artifact_changed"
        );
        manifest.push(
            json!({"source":source,"archived_path":relative,"sha256":digest,"bytes":raw.len()}),
        );
    }
    write_new(
        &output.join("archive-manifest.json"),
        &json!({"schema":"oev2.consolidation-archive-manifest/1",
        "files":manifest,"trajectories":trajectories,"total_original_bytes":budget.bytes,
        "artifact_count":budget.artifacts,"json_nodes":budget.nodes,"policy":document["archive_policy"],
        "identity":"original source bytes preserved; no historical receipt is re-signed"}),
    )?;
    write_new(
        &output.join("response.json"),
        &json!({"execution_kind":"not_run","model_calls":0,
        "reason":"empty_author_declaration_does_not_require_model_judgment"}),
    )?;
    let result = json!({"schema":DOSSIER_SCHEMA,"input_protocol":ARCHIVE_CONSOLIDATION_INPUT_SCHEMA,
        "checkpoint_id":document["checkpoint_id"],"problem_class":document["problem_class"],"reason":document["reason"],
        "trajectory_refs":refs,"candidate_proposals":[],"judgments":{},"unanswered":[],
        "status":"no_delta","execution_kind":"not_run","model_calls":0,"request_bytes":0,
        "authority":"author_declaration_only","historical_checks":"historical_checks_not_reexecuted",
        "engineering_acceptance":"not_determined","engineering_adoption":"not_performed",
        "ontology_promotion":"not_performed","formal_candidate":"not_created",
        "archive_policy":document["archive_policy"],"archived_original_bytes":budget.bytes,
        "handoff":{"current_review_entrypoint":"oev2 kernel review","required":"new current checks and evidence before engineering adoption; nonempty candidates require explicit Agent review and current Project evidence"},
        "harness_code_sha256":sha256(include_bytes!("harness.rs")),"implementation":implementation()?,
        "artifacts":artifact_map(root, &output)?});
    let result = current_dossier(result, current, document)?;
    write_new(&output.join("consolidation.json"), &result)?;
    Ok(result)
}

/// Historical data profile, preserving the original wire behavior and tests.
/// Current CLI callers use `consolidate_current`.
pub fn consolidate(root: &SkillRoot, document: &Value, output: &Path) -> Result<Value> {
    consolidate_profile(root, document, output, false)
}

/// Assess complete selected trajectories under the current Rust handoff contract.
/// Produces a dossier; the same owner must construct and verify any PackageDelta.
pub fn consolidate_current(root: &SkillRoot, document: &Value, output: &Path) -> Result<Value> {
    consolidate_profile(root, document, output, true)
}

fn current_dossier(mut result: Value, current: bool, document: &Value) -> Result<Value> {
    if !current {
        return Ok(result);
    }
    let next_action = match result["status"].as_str() {
        Some("no_delta") => "retain_no_delta",
        Some("review_required")
            if result["judgments"].as_object().is_some_and(|answers| {
                answers
                    .values()
                    .any(|answer| answer["choice"] == "semantic_candidate")
            }) =>
        {
            "review_candidates_and_construct_delta"
        }
        Some("review_required") => "review_judgments_and_choose_asset_kind",
        _ => "complete_missing_judgments",
    };
    let handoff = oe_contracts::ConsolidationHandoff {
        schema: "oev2.consolidation-handoff/1".into(),
        entrypoint: "SKILL.md".into(),
        native_proposal_entrypoint: "oev2 semantics propose".into(),
        current_review_entrypoint: "oev2 kernel review".into(),
        native_proposal_migration: "current_registry_not_verified".into(),
        entrypoint_usage: "current_reference_only_not_invoked".into(),
        next_action: next_action.into(),
        required: [
            "reviewed_reusable_gap",
            "scope_and_counterexamples",
            "nonempty_native_PackageDelta",
            "project_binding_and_fresh_task",
            "actual_rust_execution_receipts",
            "native_regression_and_governance",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    };
    handoff
        .validate()
        .map_err(|_| anyhow::anyhow!("invalid_current_consolidation_handoff"))?;
    result["historical_handoff"] = result["handoff"].clone();
    result["historical_handoff"]["meaning"] =
        json!("historical_reference_only_not_current_execution_authority");
    result["handoff"] = serde_json::to_value(handoff)?;
    result["schema"] = json!(CURRENT_DOSSIER_SCHEMA);
    result["input_protocol"] = document["schema"].clone();
    Ok(result)
}

fn consolidate_profile(
    root: &SkillRoot,
    document: &Value,
    output: &Path,
    current: bool,
) -> Result<Value> {
    ensure!(
        document["schema"] == ARCHIVE_CONSOLIDATION_INPUT_SCHEMA,
        "verified_archive_input_required"
    );
    archive_empty_checkpoint(root, document, output, current)
}

pub mod typed;
pub use typed::RustTypedDispatchHarness;
