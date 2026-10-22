//! Explicit /2 source-use envelope around the unchanged complete task view /1.
//! Archive text remains evidence. This owner never applies a method or learns a
//! new fact; matched describes selected message provenance, not engineering truth.
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    ArtifactReference, CurrentTaskMethodBinding, CurrentTaskMethodEntryObservation,
    CurrentTaskMethodObservation, CurrentTaskMethodUseInput, CurrentTaskMethodUsePort,
    CurrentTaskMethodUseView, CurrentTaskSourceObservation, FailureRecord,
};
use oe_core::{SkillRoot, json::canonical_json, json::strict_json, sha256};
use oe_kernel::project::{self, Project};
use oe_knowledge::conversation::{self, Entry};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub const INPUT_SCHEMA: &str = "oev2.current-task-view-input/2";
pub const VIEW_SCHEMA: &str = "oev2.current-task-view/2";
const ARCHIVE_LIMIT: u64 = 512 * 1024 * 1024;
const INTAKE_LIMIT: usize = 16 * 1024 * 1024;

fn source_observation(
    reference: &ArtifactReference,
    evidence_ids: Vec<String>,
    actual_sha256: Option<String>,
    reason: Option<String>,
) -> CurrentTaskSourceObservation {
    let status = match actual_sha256.as_deref() {
        Some(actual) if actual == reference.sha256 => "matched",
        Some(_) => "drift",
        None => "unavailable",
    };
    CurrentTaskSourceObservation {
        reference: reference.clone(),
        actual_sha256,
        status: status.into(),
        reason,
        evidence_ids,
    }
}

fn observe_intake(
    root: &SkillRoot,
    project: &Project,
    reference: &ArtifactReference,
) -> (CurrentTaskSourceObservation, Option<Vec<u8>>) {
    let evidence_ids = project
        .evidence
        .iter()
        .filter(|(_, hash)| **hash == reference.sha256)
        .map(|(id, _)| id.clone())
        .collect();
    let attempt = (|| -> Result<Vec<u8>> {
        reference
            .validate()
            .map_err(|_| anyhow::anyhow!("method_intake_reference_invalid"))?;
        oe_semantic::safe_relative(&reference.path)?;
        oe_core::read_regular_bytes(&root.path().join(&reference.path), INTAKE_LIMIT)
    })();
    match attempt {
        Ok(raw) => {
            let hash = sha256(&raw);
            let matched = hash == reference.sha256;
            let observation = source_observation(
                reference,
                evidence_ids,
                Some(hash),
                (!matched).then(|| "method_intake_bytes_changed".into()),
            );
            (observation, matched.then_some(raw))
        }
        Err(error) => (
            source_observation(reference, evidence_ids, None, Some(error.to_string())),
            None,
        ),
    }
}

/// Only this explicit reference is opened. Intake.source_path is compared as
/// data and never becomes an implicit filesystem discovery or fallback path.
fn observe_archive(
    root: &SkillRoot,
    reference: &ArtifactReference,
) -> (CurrentTaskSourceObservation, Option<(PathBuf, u64)>) {
    let attempt = (|| -> Result<(PathBuf, oe_core::FileIdentity)> {
        reference
            .validate()
            .map_err(|_| anyhow::anyhow!("method_archive_reference_invalid"))?;
        let path = root.expand(Path::new(&reference.path))?;
        Ok((
            path.clone(),
            oe_core::fingerprint_file(&path, ARCHIVE_LIMIT)?,
        ))
    })();
    match attempt {
        Ok((path, identity)) => {
            let matched = identity.sha256 == reference.sha256;
            let observation = source_observation(
                reference,
                Vec::new(),
                Some(identity.sha256),
                (!matched).then(|| "method_archive_bytes_changed".into()),
            );
            (observation, matched.then_some((path, identity.bytes)))
        }
        Err(error) => (
            source_observation(reference, Vec::new(), None, Some(error.to_string())),
            None,
        ),
    }
}

/// Recheck the original intake's captured prefix/window without parsing other
/// messages. The full archive reference and selected records are checked apart.
fn range_sha(path: &Path, start: u64, end: u64) -> Result<String> {
    ensure!(
        end >= start && end - start <= ARCHIVE_LIMIT,
        "method_intake_range_invalid"
    );
    let mut file = oe_core::open_regular_file(path, ARCHIVE_LIMIT)?;
    file.seek(SeekFrom::Start(start))?;
    let mut remaining = end - start;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64))?;
        file.read_exact(&mut buffer[..count])?;
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn number(value: &Value, field: &str) -> Result<u64> {
    value[field]
        .as_u64()
        .with_context(|| format!("method_intake_{field}_invalid"))
}

fn snapshot_range(
    root: &SkillRoot,
    intake: &Value,
    path: &Path,
    archive_bytes: u64,
) -> Result<(u64, u64)> {
    ensure!(
        intake["schema"] == conversation::SCHEMA
            && intake["source_kind"] == "retrospective_archive"
            && intake["historical_execution_reconstructed"] == false
            && intake["engineering_acceptance"] == "not_established"
            && intake["ontology_promotion"] == "not_performed"
            && intake["record_memory_guard"] == conversation::MAX_RECORD_BYTES,
        "method_intake_profile_invalid"
    );
    let source_path = intake["source_path"]
        .as_str()
        .context("method_intake_source_path_invalid")?;
    ensure!(
        root.expand(Path::new(source_path))? == path,
        "method_intake_archive_path_mismatch"
    );
    ensure!(
        number(intake, "original_size")? == archive_bytes,
        "method_intake_archive_size_changed"
    );
    let scanned = number(intake, "scanned_bytes")?;
    let (start, limit) = if intake["window"].is_null() {
        (0, archive_bytes)
    } else {
        let window = &intake["window"];
        ensure!(
            window["line_origin"] == "range_relative" && window["outside_window"] == "not_scanned",
            "method_intake_window_profile_invalid"
        );
        (number(window, "byte_start")?, number(window, "byte_end")?)
    };
    let end = start
        .checked_add(scanned)
        .context("method_intake_range_invalid")?;
    ensure!(
        start <= end && end <= limit && limit <= archive_bytes,
        "method_intake_range_invalid"
    );
    let hash = range_sha(path, start, end)?;
    ensure!(
        intake["scanned_range_sha256"] == hash,
        "method_intake_snapshot_changed"
    );
    if intake["window"].is_null() {
        ensure!(
            intake["scanned_prefix_sha256"] == hash,
            "method_intake_snapshot_changed"
        );
    } else {
        ensure!(
            intake["scanned_prefix_sha256"].is_null(),
            "method_intake_window_profile_invalid"
        );
    }
    Ok((start, end))
}

/// verify_entry checks the raw block and declared line/id relation. Also count
/// physical newline boundaries here so changing line and id together cannot
/// impersonate another archive message with the same valid byte range.
fn verify_locator(path: &Path, entry: &Entry, start: u64, end: u64) -> Result<()> {
    ensure!(
        start <= entry.byte_start && entry.byte_end <= end && entry.byte_end > entry.byte_start,
        "method_selected_range_outside_intake"
    );
    ensure!(
        entry.byte_end - entry.byte_start <= conversation::MAX_RECORD_BYTES as u64,
        "method_selected_record_exceeds_original_guard"
    );
    let mut file = oe_core::open_regular_file(path, ARCHIVE_LIMIT)?;
    if start > 0 {
        file.seek(SeekFrom::Start(start - 1))?;
        let mut byte = [0];
        file.read_exact(&mut byte)?;
        ensure!(byte[0] == b'\n', "method_intake_window_not_record_aligned");
    }
    file.seek(SeekFrom::Start(start))?;
    let mut remaining = entry.byte_start - start;
    let mut lines = 1u64;
    let mut last = b'\n';
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64))?;
        file.read_exact(&mut buffer[..count])?;
        lines += buffer[..count]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u64;
        last = buffer[count - 1];
        remaining -= count as u64;
    }
    ensure!(
        last == b'\n' && lines == entry.line,
        "method_selected_line_or_boundary_changed"
    );
    file.seek(SeekFrom::Start(entry.byte_end - 1))?;
    let mut byte = [0];
    file.read_exact(&mut byte)?;
    ensure!(
        byte[0] == b'\n' || entry.byte_end == end,
        "method_selected_end_not_record_aligned"
    );
    conversation::verify_entry(path, entry)
}

fn affected(value: &Value) -> Result<BTreeSet<String>> {
    value["affected"]
        .as_array()
        .context("method_impact_invalid")?
        .iter()
        .map(|v| {
            Ok(v.as_str()
                .context("method_impact_identity_invalid")?
                .to_owned())
        })
        .collect()
}

fn binding_observation(
    root: &SkillRoot,
    project: &Project,
    binding: &CurrentTaskMethodBinding,
) -> Result<CurrentTaskMethodObservation> {
    let (intake_observation, intake_raw) = observe_intake(root, project, &binding.intake);
    let (archive_observation, archive) = if let Some(reference) = &binding.archive {
        let (observation, available) = observe_archive(root, reference);
        (Some(observation), available)
    } else {
        (None, None)
    };
    let mut reasons = Vec::new();
    let mut refuted = false;
    let mut unknown = false;
    for source in std::iter::once(&intake_observation).chain(archive_observation.as_ref()) {
        if source.status == "drift" {
            refuted = true;
        }
        if source.status == "unavailable" {
            unknown = true;
        }
        if let Some(reason) = &source.reason {
            reasons.push(reason.clone());
        }
    }
    if binding.archive.is_none() {
        unknown = true;
        reasons.push("method_original_archive_not_provided".into());
    }
    let mut basis_protocol = None;
    let mut basis_sha256 = project
        .conditions
        .get(&binding.object.condition_scope)
        .map(sha256);
    let mut fact_value = None;
    let mut fact_status = None;
    let mut fact_source_refs = Vec::new();
    let bound = (|| -> Result<()> {
        binding
            .validate()
            .map_err(|_| anyhow::anyhow!("method_binding_contract_invalid"))?;
        binding
            .object
            .validate()
            .map_err(|_| anyhow::anyhow!("method_object_contract_invalid"))?;
        ensure!(
            project.objects.get(&binding.object.object_id) == Some(&binding.object.revision)
                && project
                    .evidence
                    .values()
                    .any(|sha| sha == &binding.object.source_sha256),
            "method_current_object_binding_mismatch"
        );
        let basis = oe_kernel::engineering::basis(project, &binding.object)?
            .context("method_current_basis_missing")?;
        basis_protocol = Some(basis.protocol().into());
        basis_sha256 = Some(basis.condition_sha256().into());
        ensure!(
            basis.condition_sha256() == binding.basis_sha256,
            "method_current_basis_bytes_changed"
        );
        let fact = basis
            .facts
            .iter()
            .find(|fact| fact.property == binding.fact_property)
            .context("method_fact_property_missing")?;
        fact_value = Some(fact.value.clone());
        fact_status = Some(fact.status.clone());
        fact_source_refs = fact.source_refs.clone();
        ensure!(
            fact.value["role"] == "operation_method_instance",
            "method_fact_role_not_method_instance"
        );
        ensure!(
            fact.source_refs.contains(&binding.source_id),
            "method_fact_source_not_bound"
        );
        ensure!(
            project.evidence.get(&binding.source_id) == Some(&binding.intake.sha256),
            "method_intake_not_current_project_evidence"
        );
        Ok(())
    })();
    if let Err(error) = bound {
        refuted = true;
        reasons.push(error.to_string());
    }
    let mut intake_scope = None;
    let mut entries = Vec::new();
    let intake = intake_raw.as_ref().map(|raw| strict_json(raw)).transpose();
    let mut value = None;
    match intake {
        Ok(Some(intake)) => {
            let mut scope = intake.clone();
            if let Some(object) = scope.as_object_mut() {
                object.remove("entries");
            }
            intake_scope = Some(scope);
            value = Some(intake);
        }
        Ok(None) => {}
        Err(error) => {
            refuted = true;
            reasons.push(format!("method_intake_json_invalid:{error}"));
        }
    }
    let mut selected = BTreeMap::new();
    let mut range = None;
    if let Some(intake) = &value {
        let parse = (|| -> Result<()> {
            let rows = intake["entries"]
                .as_array()
                .context("method_intake_entries_invalid")?;
            ensure!(
                number(intake, "entry_count")? == rows.len() as u64,
                "method_intake_entry_count_changed"
            );
            for id in &binding.entry_ids {
                let found: Vec<_> = rows.iter().filter(|row| row["id"] == *id).collect();
                ensure!(
                    found.len() == 1,
                    "method_selected_entry_missing_or_ambiguous"
                );
                selected.insert(id.clone(), found[0].clone());
            }
            if let Some((path, count)) = &archive {
                range = Some(snapshot_range(root, intake, path, *count)?);
            }
            Ok(())
        })();
        if let Err(error) = parse {
            refuted = true;
            reasons.push(error.to_string());
        }
    }
    for id in &binding.entry_ids {
        let row = selected.get(id).cloned();
        let mut entry_reasons = Vec::new();
        let parsed = row
            .as_ref()
            .map(|v| serde_json::from_value::<Entry>(v.clone()))
            .transpose();
        let mut text_sha256 = None;
        let mut status = "unknown";
        match parsed {
            Ok(Some(entry)) => {
                text_sha256 = Some(sha256(&entry.text));
                if let (Some((path, _)), Some((start, end))) = (&archive, range) {
                    let verify = (|| -> Result<()> {
                        ensure!(
                            entry.kind == "message"
                                && matches!(entry.role.as_str(), "user" | "assistant"),
                            "method_selected_entry_not_visible_dialogue_message"
                        );
                        verify_locator(path, &entry, start, end)
                    })();
                    match verify {
                        Ok(()) => status = "matched",
                        Err(error) => {
                            status = "refuted";
                            entry_reasons.push(error.to_string());
                        }
                    }
                } else {
                    entry_reasons.push("method_original_snapshot_or_archive_unavailable".into());
                }
            }
            Ok(None) => {
                status = if value.is_some() {
                    "refuted"
                } else {
                    "unknown"
                };
                entry_reasons.push("method_selected_entry_unavailable".into());
            }
            Err(error) => {
                status = "refuted";
                entry_reasons.push(format!("method_selected_entry_invalid:{error}"));
            }
        }
        refuted |= status == "refuted";
        unknown |= status == "unknown";
        entries.push(CurrentTaskMethodEntryObservation {
            entry_id: id.clone(),
            intake_entry: row,
            text_sha256,
            status: status.into(),
            reasons: entry_reasons,
        });
    }
    let seeds = BTreeSet::from([
        binding.object.object_id.clone(),
        binding.object.condition_scope.clone(),
        binding.fact_property.clone(),
        binding.source_id.clone(),
        binding.intake.sha256.clone(),
    ]);
    let impact = project::impact(project, &seeds)?;
    reasons.sort();
    reasons.dedup();
    let intake_statistics_verification = if intake_scope.is_some() {
        "historical_supplied_declaration"
    } else {
        "unavailable"
    };
    let observation = CurrentTaskMethodObservation {
        binding: binding.clone(),
        basis_protocol,
        basis_sha256,
        fact_value,
        fact_status,
        fact_source_refs,
        intake_observation,
        archive_observation,
        intake_scope,
        intake_statistics_verification: intake_statistics_verification.into(),
        entries,
        status: if refuted {
            "refuted"
        } else if unknown {
            "unknown"
        } else {
            "matched"
        }
        .into(),
        reasons,
        impact,
    };
    observation
        .validate()
        .map_err(|_| anyhow::anyhow!("method_observation_contract_invalid"))?;
    Ok(observation)
}

pub fn derive_with_methods(
    root: &SkillRoot,
    input: &CurrentTaskMethodUseInput,
) -> Result<CurrentTaskMethodUseView> {
    input
        .validate()
        .map_err(|_| anyhow::anyhow!("current_task_method_input_invalid"))?;
    ensure!(
        input.schema == INPUT_SCHEMA,
        "current_task_method_input_schema"
    );
    ensure!(
        !input.methods.is_empty() && input.methods.len() <= 32,
        "current_task_method_binding_count"
    );
    let mut count = 0usize;
    let mut identities = BTreeSet::new();
    for binding in &input.methods {
        ensure!(
            identities.insert((
                &binding.object.object_id,
                &binding.object.condition_scope,
                &binding.fact_property,
                &binding.source_id
            )),
            "current_task_duplicate_method_binding"
        );
        ensure!(
            !binding.entry_ids.is_empty()
                && binding.entry_ids.iter().all(|id| !id.trim().is_empty())
                && binding.entry_ids.iter().collect::<BTreeSet<_>>().len()
                    == binding.entry_ids.len(),
            "current_task_method_entry_selection_invalid"
        );
        count = count
            .checked_add(binding.entry_ids.len())
            .context("current_task_method_entry_count")?;
    }
    ensure!(count <= 64, "current_task_method_entry_count");
    // Run the original complete work/context/kernel checks even if the archive
    // or method binding is missing or forged. Their output is not rewritten.
    let task_view = super::derive(root, &input.task)?;
    let (project, _) = super::load_project(root, &input.task.current)?;
    let methods = input
        .methods
        .iter()
        .map(|binding| binding_observation(root, &project, binding))
        .collect::<Result<Vec<_>>>()?;
    let mut all_affected: BTreeSet<_> = task_view.affected.iter().cloned().collect();
    for method in &methods {
        if method.status != "matched" {
            all_affected.extend(affected(&method.impact)?);
        }
    }
    let freshness = if methods.iter().any(|m| m.status == "refuted")
        || task_view
            .source_observations
            .iter()
            .any(|s| s.status == "drift")
    {
        "refuted"
    } else if methods.iter().any(|m| m.status != "matched")
        || task_view
            .source_observations
            .iter()
            .any(|s| s.status != "matched")
    {
        "unknown"
    } else {
        "matched"
    };
    let view = CurrentTaskMethodUseView {
        schema: VIEW_SCHEMA.into(),
        input: input.clone(),
        input_sha256: sha256(canonical_json(&serde_json::to_value(input)?)?),
        task_view,
        methods,
        affected: all_affected.into_iter().collect(),
        freshness: freshness.into(),
        proof_scope: "selected_message_provenance_only".into(),
    };
    view.validate()
        .map_err(|_| anyhow::anyhow!("current_task_method_output_invalid"))?;
    ensure!(
        super::derive(root, &input.task)? == view.task_view,
        "current_task_method_project_changed_during_projection"
    );
    let after = input
        .methods
        .iter()
        .map(|binding| binding_observation(root, &project, binding))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        after == view.methods,
        "current_task_method_source_changed_during_projection"
    );
    Ok(view)
}

pub fn verify_method_view(root: &SkillRoot, view: &CurrentTaskMethodUseView) -> Result<()> {
    view.validate()
        .map_err(|_| anyhow::anyhow!("current_task_method_view_invalid"))?;
    ensure!(
        view.schema == VIEW_SCHEMA,
        "current_task_method_view_schema"
    );
    let current = derive_with_methods(root, &view.input)?;
    ensure!(
        current.freshness != "refuted",
        "current_task_method_source_refuted"
    );
    ensure!(
        current.freshness == "matched",
        "current_task_method_source_freshness_unknown"
    );
    ensure!(current == *view, "current_task_method_view_stale");
    super::verify_view(root, &view.task_view)
}

pub struct CurrentTaskMethodUseExecutor<'a> {
    root: &'a SkillRoot,
}
impl<'a> CurrentTaskMethodUseExecutor<'a> {
    pub fn new(root: &'a SkillRoot) -> Self {
        Self { root }
    }
}
impl CurrentTaskMethodUsePort for CurrentTaskMethodUseExecutor<'_> {
    fn derive_with_methods(
        &self,
        input: &CurrentTaskMethodUseInput,
    ) -> std::result::Result<CurrentTaskMethodUseView, FailureRecord> {
        derive_with_methods(self.root, input)
            .map_err(|error| crate::artifacts::failure("current_task_method_source", error))
    }
}
