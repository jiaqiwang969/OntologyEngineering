//! Explicit reference-only association in the existing design-attempt owner.
//! Formal review/native adapters remain separate unexecuted obligations.
use super::*;
use oe_contracts::{EngineeringDesignAttemptRecordRequest2, EngineeringDesignAttemptRecordResult2};
use oe_kernel::project::cad_focus::{ValidatedCadFocus, bind_cad_focus, check_declaration};

struct Checked {
    project_bytes: Vec<u8>,
    work_bytes: Vec<u8>,
    artifact_bytes: Vec<Vec<u8>>,
    focus: ValidatedCadFocus,
    result: EngineeringDesignAttemptRecordResult2,
}

fn check(root: &SkillRoot, request: &EngineeringDesignAttemptRecordRequest2) -> Result<Checked> {
    generated(request.validate())?;
    ensure!(
        request.schema == "oev2.design-attempt-record/2"
            && text(&request.attempt_id)
            && strings(&request.notes),
        "cad_binding_request_invalid"
    );
    let binding = &request.binding;
    generated(binding.validate())?;
    ensure!(
        binding.product_state == "reference_only"
            && binding.evaluation_scope == "development_replay"
            && binding.independent_intent == "unknown",
        "cad_binding_reference_scope_required"
    );
    let project_bytes = read(root, &binding.project)?;
    let project: Project = serde_json::from_value(json(&project_bytes)?)?;
    let work_bytes = read(root, &binding.work)?;
    let work: WorkInput = serde_json::from_value(json(&work_bytes)?)?;
    ensure!(
        work.schema == "oev2.engineering-work/3",
        "cad_binding_work3_required"
    );
    crate::workflow::verify_project_binding(&project, &work)?;
    ensure!(
        work.project.path == Path::new(&binding.project.path)
            && work.project.sha256 == binding.project.sha256,
        "cad_binding_project_reference_mismatch"
    );
    ensure!(
        work.object == binding.object
            && work.project_context.as_ref() == Some(&binding.project_context),
        "cad_binding_object_context_mismatch"
    );
    let condition = project
        .conditions
        .get(&work.object.condition_scope)
        .context("cad_binding_condition_missing")?;
    ensure!(
        sha256(condition.as_bytes()) == binding.condition_sha256
            && binding.project_context.condition_sha256 == binding.condition_sha256,
        "cad_binding_condition_content_mismatch"
    );
    ensure!(
        work.acceptance == binding.acceptance,
        "cad_binding_full_acceptance_mismatch"
    );
    let focus = bind_cad_focus(&project_bytes, &binding.object, &binding.project_context)?;
    if let Some(supplied) = &binding.focus_iri {
        ensure!(supplied == focus.iri(), "cad_binding_focus_mismatch");
    }
    exposure(&request.reference_exposure, &project, false)?;
    ensure!(
        !request.tool_plan.is_empty() && request.tool_plan.len() <= 32,
        "cad_binding_tool_plan_budget"
    );
    let mut step_ids = BTreeSet::new();
    for step in &request.tool_plan {
        generated(step.validate())?;
        ensure!(
            text(&step.step_id)
                && step_ids.insert(&step.step_id)
                && !step.acceptance.is_empty()
                && !step.required_evidence.is_empty()
                && strings(&step.required_evidence),
            "cad_binding_tool_step_invalid"
        );
        ensure!(
            step.acceptance
                .iter()
                .all(|value| binding.acceptance.contains(value)),
            "cad_binding_step_acceptance_changed"
        );
    }
    ensure!(
        binding.acceptance.iter().all(|value| request
            .tool_plan
            .iter()
            .any(|step| step.acceptance.contains(value))),
        "cad_binding_acceptance_unplanned"
    );
    for activity in [
        "work",
        "harness",
        "cad_evidence",
        "handoff",
        "kernel_review",
        "feedback",
    ] {
        ensure!(
            request
                .tool_plan
                .iter()
                .any(|step| step.activity == activity),
            "cad_binding_required_step_missing:{activity}"
        );
    }
    ensure!(
        !request.artifacts.is_empty()
            && request.artifacts.len() <= 32
            && !request.claims.is_empty()
            && request.claims.len() <= 128,
        "cad_binding_artifact_or_claim_budget"
    );
    let mut total = project_bytes
        .len()
        .checked_add(work_bytes.len())
        .context("cad_binding_input_budget")?;
    let mut artifacts = Vec::new();
    let mut artifact_bytes = Vec::new();
    let mut unique = BTreeSet::new();
    for artifact in &request.artifacts {
        generated(artifact.validate())?;
        ensure!(
            matches!(artifact.role.as_str(), "cad_evidence" | "handoff"),
            "cad_binding_original_cad_role_required"
        );
        ensure!(
            request
                .tool_plan
                .iter()
                .any(|step| step.step_id == artifact.step_id && step.activity == artifact.role),
            "cad_binding_artifact_step_mismatch"
        );
        ensure!(
            unique.insert((&artifact.artifact.path, &artifact.artifact.sha256)),
            "cad_binding_artifact_duplicate"
        );
        let raw = read(root, &artifact.artifact)?;
        total = total
            .checked_add(raw.len())
            .context("cad_binding_input_budget")?;
        ensure!(total <= TOTAL, "cad_binding_input_budget");
        let original = json(&raw)?;
        ensure!(
            original["project"]["configuration_id"] == binding.object.condition_scope
                && original["project"]["product_state"] == "reference_only",
            "cad_binding_packet_scope_or_state_mismatch"
        );
        let verification = cad_artifact_check(root, artifact, &original, &work, &project)?;
        artifacts.push(json!({"reference":artifact,"original":original,"source_integrity":verification,"association_authority":"current source-bound declaration; not native observation or adoption"}));
        artifact_bytes.push(raw);
    }
    if let Some(handoff) = &work.handoff {
        ensure!(
            request.artifacts.iter().any(|a| a.role == "handoff"
                && Path::new(&a.artifact.path) == handoff.path
                && a.artifact.sha256 == handoff.sha256),
            "cad_binding_work_handoff_not_retained"
        );
    }
    let mut claims = Vec::new();
    let mut claim_bytes = 0usize;
    let mut selected = BTreeSet::new();
    for claim in &request.claims {
        generated(claim.validate())?;
        let artifact = artifacts
            .get(
                usize::try_from(claim.artifact_index)
                    .context("cad_binding_claim_index_out_of_range")?,
            )
            .context("cad_binding_claim_artifact_missing")?;
        let acceptance = binding
            .acceptance
            .get(
                usize::try_from(claim.acceptance_index)
                    .context("cad_binding_claim_index_out_of_range")?,
            )
            .context("cad_binding_claim_acceptance_missing")?;
        ensure!(
            acceptance == &claim.acceptance,
            "cad_binding_claim_acceptance_changed"
        );
        ensure!(
            claim.source_pointer.starts_with('/')
                && !claim.source_pointer.ends_with('/')
                && selected.insert((
                    claim.artifact_index,
                    &claim.source_pointer,
                    claim.acceptance_index
                )),
            "cad_binding_claim_pointer_invalid_or_duplicate"
        );
        let original = artifact["original"]
            .pointer(&claim.source_pointer)
            .context("cad_binding_claim_pointer_missing")?;
        // Count a borrowed envelope before cloning the selected subtree. The
        // bounded writer stops at16MiB, and cumulative claim envelopes cannot
        // exceed the final result's per-file limit. No source is truncated.
        #[derive(serde::Serialize)]
        struct ClaimEnvelope<'a> {
            binding: &'a oe_contracts::DesignAttemptCadClaim2,
            original_value: &'a Value,
            claim_authority: &'static str,
            engineering_acceptance: &'static str,
            native_adapter: &'static str,
        }
        let envelope = ClaimEnvelope {
            binding: claim,
            original_value: original,
            claim_authority: "source-supplied declaration only",
            engineering_acceptance: "not_established",
            native_adapter: "not_run",
        };
        let encoded = bounded_json(&envelope)?;
        claim_bytes = claim_bytes
            .checked_add(encoded.len())
            .context("cad_binding_claim_output_budget")?;
        ensure!(claim_bytes <= LIMIT, "cad_binding_claim_output_budget");
        claims.push(json(&encoded)?);
    }
    let graph = focus.declaration_graph()?;
    let declaration_check = check_declaration(&focus, &graph);
    ensure!(
        declaration_check.conclusion == "pass",
        "cad_binding_focus_declaration_failed"
    );
    let checks = oe_kernel::engineering::checks_current(root, &project, &work.object);
    let result = EngineeringDesignAttemptRecordResult2 {
        schema: "oev2.design-attempt-recorded/2".into(),
        binding: binding.clone(),
        focus_context: focus.context().clone(),
        focus_iri: focus.iri().into(),
        association: json!({"status":"verified_source_association_only","attempt_id":request.attempt_id,"artifacts":artifacts,"selected_claims":claims,"focus_declaration_check":declaration_check,"engineering_checks":checks,
            "condition_text":condition,"complete_original_work":json(&work_bytes)?,"complete_original_project":json(&project_bytes)?,
            "remaining_acceptance":binding.acceptance.iter().map(|a|json!({"acceptance":a,"engineering_acceptance":"not_established"})).collect::<Vec<_>>(),"tool_plan":request.tool_plan,"reference_exposure":request.reference_exposure,"notes":request.notes,
            "work_execution":"not_run","native_execution":"not_run","native_adapter":"not_run","formal_semantic_review":"not_run","independent_intent":"unknown","design_complete":false,"project_mutated":false}),
        engineering_conclusion: "unknown".into(),
        adopted: false,
    };
    generated(result.validate())?;
    Ok(Checked {
        project_bytes,
        work_bytes,
        artifact_bytes,
        focus,
        result,
    })
}

/// Serialize the exact existing pretty-JSON/newline wire with a bounded sink.
/// This is also used by the CLI /2 attachment path, rather than another renderer.
pub fn bounded_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    struct Sink(Vec<u8>);
    impl std::io::Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self
                .0
                .len()
                .checked_add(bytes.len())
                .is_none_or(|n| n > LIMIT)
            {
                return Err(std::io::Error::other("cad_binding_output_file_budget"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Sink(Vec::new());
    serde_json::to_writer_pretty(&mut sink, value)?;
    use std::io::Write;
    sink.write_all(b"\n")?;
    Ok(sink.0)
}

const OUTER_NAMES: [&str; 4] = [
    "cli-completed.json",
    "cli-input.json",
    "execution-binding.json",
    "execution-recheck.json",
];

fn collection_budget(files: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let mut total = 0usize;
    for bytes in files.values() {
        ensure!(bytes.len() <= LIMIT, "cad_binding_output_file_budget");
        total = total
            .checked_add(bytes.len())
            .context("cad_binding_storage_budget")?;
        ensure!(total <= TOTAL, "cad_binding_storage_budget");
    }
    Ok(())
}

fn core_files(
    request: &EngineeringDesignAttemptRecordRequest2,
    checked: &Checked,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::from([
        ("record-request.json".into(), bounded_json(request)?),
        ("project.json".into(), checked.project_bytes.clone()),
        ("work-input.json".into(), checked.work_bytes.clone()),
        (
            "association-declarations.nt".into(),
            checked.focus.declaration_graph()?,
        ),
        ("result.json".into(), bounded_json(&checked.result)?),
    ]);
    for (i, bytes) in checked.artifact_bytes.iter().enumerate() {
        files.insert(format!("artifact-{i:03}.json"), bytes.clone());
    }
    collection_budget(&files)?;
    let inventory: BTreeMap<_, _> = files
        .iter()
        .map(|(name, bytes)| (name.clone(), sha256(bytes)))
        .collect();
    let terminal = json!({"schema":"oev2.design-attempt-record-completed/2","status":"recorded","files":inventory,
        "engineering_conclusion":"unknown","native_adapter":"not_run","formal_semantic_review":"not_run","design_complete":false,"adopted":false,"project_mutated":false});
    files.insert("completed.json".into(), bounded_json(&terminal)?);
    // The terminal is part of the64MiB collection, not an unmetered append.
    collection_budget(&files)?;
    Ok(files)
}

/// A private Rust plan, not a deserializable success state. Both library and
/// CLI publish the same exact core plan; the CLI adds its four bounded files.
pub struct PreparedRecord {
    request: EngineeringDesignAttemptRecordRequest2,
    checked: Checked,
    files: BTreeMap<String, Vec<u8>>,
}
impl PreparedRecord {
    pub fn core_completed_sha256(&self) -> String {
        sha256(&self.files["completed.json"])
    }
    pub fn result(&self) -> Result<Value> {
        Ok(serde_json::to_value(&self.checked.result)?)
    }
    pub fn publish(
        self,
        root: &SkillRoot,
        output: &Path,
        outer: BTreeMap<String, Vec<u8>>,
        completion_guard: impl FnOnce() -> Result<()>,
    ) -> Result<Value> {
        if !outer.is_empty() {
            check_outer(root, &self.request, &self.files, &outer)?;
        }
        let mut files = self.files.clone();
        for (name, bytes) in outer {
            ensure!(
                files.insert(name, bytes).is_none(),
                "cad_binding_outer_overwrites_core"
            );
        }
        // Includes all core copies/result/terminal and all four CLI files.
        collection_budget(&files)?;
        let current = check(root, &self.request)?;
        self.checked.focus.ensure_current(&current.project_bytes)?;
        ensure!(
            core_files(&self.request, &current)? == self.files,
            "cad_binding_sources_changed_before_publication"
        );
        let dir = root.new_private_dir(output)?;
        for (name, bytes) in &files {
            if name != "completed.json" && name != "cli-completed.json" {
                write_new_bytes(&dir.join(name), bytes)?;
            }
        }
        // No terminal is published after a source/CLI identity drift. The
        // original failed partial directory is retained and never reused.
        let current = check(root, &self.request)?;
        ensure!(
            core_files(&self.request, &current)? == self.files,
            "cad_binding_sources_changed_before_completion"
        );
        completion_guard()?;
        for (name, bytes) in &files {
            if name != "completed.json" && name != "cli-completed.json" {
                ensure!(
                    read_regular_bytes(&dir.join(name), LIMIT)? == *bytes,
                    "cad_binding_published_file_changed:{name}"
                );
            }
        }
        write_new_bytes(&dir.join("completed.json"), &files["completed.json"])?;
        if let Some(bytes) = files.get("cli-completed.json") {
            write_new_bytes(&dir.join("cli-completed.json"), bytes)?;
        }
        self.result()
    }
}

pub fn prepare_record(
    root: &SkillRoot,
    request: &EngineeringDesignAttemptRecordRequest2,
) -> Result<PreparedRecord> {
    let checked = check(root, request)?;
    let files = core_files(request, &checked)?;
    Ok(PreparedRecord {
        request: request.clone(),
        checked,
        files,
    })
}

/// Existing record entry explicitly dispatches here for reference-only /2.
pub fn record(
    root: &SkillRoot,
    request: &EngineeringDesignAttemptRecordRequest2,
    output: &Path,
) -> Result<Value> {
    prepare_record(root, request)?.publish(root, output, BTreeMap::new(), || Ok(()))
}

fn check_outer(
    root: &SkillRoot,
    request: &EngineeringDesignAttemptRecordRequest2,
    core: &BTreeMap<String, Vec<u8>>,
    outer: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    ensure!(
        outer.keys().map(String::as_str).eq(OUTER_NAMES),
        "cad_binding_cli_attachment_set_mismatch"
    );
    let parsed: EngineeringDesignAttemptRecordRequest2 =
        serde_json::from_value(json(&outer["cli-input.json"])?)?;
    ensure!(&parsed == request, "cad_binding_cli_input_mismatch");
    let terminal = json(&outer["cli-completed.json"])?;
    let input: ArtifactReference = serde_json::from_value(terminal["input"].clone())?;
    ensure!(
        read(root, &input)? == outer["cli-input.json"],
        "cad_binding_cli_original_input_changed"
    );
    let expected = json!({"schema":"oev2.design-attempt-cli-completed/1","status":"completed","operation":"record","input":input,
        "core_completed":{"path":"completed.json","sha256":sha256(&core["completed.json"])},
        "input_copy":{"path":"cli-input.json","sha256":sha256(&outer["cli-input.json"])},
        "execution_binding":{"path":"execution-binding.json","sha256":sha256(&outer["execution-binding.json"])},
        "execution_recheck":{"path":"execution-recheck.json","sha256":sha256(&outer["execution-recheck.json"])},
        "design_complete":false,"adopted":false,"project_mutated":false});
    ensure!(
        terminal == expected,
        "cad_binding_cli_terminal_semantics_changed"
    );
    let start = json(&outer["execution-binding.json"])?;
    let finish = json(&outer["execution-recheck.json"])?;
    for key in [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ] {
        ensure!(
            start.get(key).is_some_and(|v| !v.is_null()) && start[key] == finish[key],
            "cad_binding_cli_recorded_identity_mismatch:{key}"
        );
    }
    // These are supplied CLI identity records, not standalone authentication.
    // The real CLI additionally uses its current execution owner before/after
    // publication; association verification does not claim executable authority.
    Ok(())
}

pub fn verify_record(
    root: &SkillRoot,
    request: &EngineeringDesignAttemptRecordRequest2,
    recorded: &Path,
) -> Result<Value> {
    let current = check(root, request)?;
    let expected = core_files(request, &current)?;
    let dir = local(root, recorded)?;
    let mut observed_names = BTreeSet::new();
    let mut observed = BTreeMap::new();
    let mut observed_bytes = 0usize;
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("cad_binding_record_filename_invalid")?
            .to_owned();
        ensure!(
            expected.contains_key(&name) || OUTER_NAMES.contains(&name.as_str()),
            "cad_binding_record_unexpected_file:{name}"
        );
        ensure!(
            observed_names.insert(name.clone()),
            "cad_binding_record_file_duplicate"
        );
        let bytes = read_regular_bytes(&path, LIMIT)?;
        observed_bytes = observed_bytes
            .checked_add(bytes.len())
            .context("cad_binding_storage_budget")?;
        ensure!(observed_bytes <= TOTAL, "cad_binding_storage_budget");
        observed.insert(name, bytes);
    }
    collection_budget(&observed)?;
    for (name, bytes) in &expected {
        ensure!(
            observed.get(name) == Some(bytes),
            "cad_binding_record_original_or_terminal_changed:{name}"
        );
    }
    let outer: BTreeMap<_, _> = observed
        .into_iter()
        .filter(|(name, _)| !expected.contains_key(name))
        .collect();
    if !outer.is_empty() {
        check_outer(root, request, &expected, &outer)?;
    }
    Ok(
        json!({"schema":"oev2.design-attempt-record-revalidation/2","status":"current_source_association_verified","focus_iri":current.focus.iri(),
        "engineering_conclusion":"unknown","native_adapter":"not_run","formal_semantic_review":"not_run","adopted":false,"project_mutated":false,
        "inventory_scope":"exact core files plus either no CLI attachments or the complete four original CLI attachment records; complete observed collection metered",
        "cli_identity_authority":"recorded source consistency only; this library verifier does not authenticate a supplied executable receipt"}),
    )
}
