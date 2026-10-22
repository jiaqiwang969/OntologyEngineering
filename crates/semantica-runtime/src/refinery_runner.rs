//! Industry authoring uses the existing Catalog/package loader, DatasetSession
//! and BookRunner. Retained observations never become a release certificate.
use crate::{
    book_runner::{self, BookRunnerState},
    books::{Catalog, SourceProvider},
    dataset::DatasetSession,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};

pub fn run_manifest(
    provider: &dyn SourceProvider,
    manifest: &Value,
    scenario: Option<&str>,
    runtime_source: &Value,
) -> Result<Value> {
    let catalog = Catalog::from_execution_manifest(manifest)?;
    let mut session = DatasetSession::new();
    let mut state = BookRunnerState::default();
    let execution = book_runner::run(
        &mut state,
        &catalog,
        provider,
        &mut session,
        manifest["package_id"].as_str().unwrap_or(""),
        scenario,
        runtime_source,
    )?;
    let verification = book_runner::verify(&state, &mut session, &execution, runtime_source)?;
    let dataset = session.snapshot()?;
    ensure!(
        execution["receipt"]["receipt_sha256"] == verification["receipt_sha256"],
        "refinery_runner_receipt_identity"
    );
    Ok(
        json!({"execution":execution,"verification":verification,"dataset":dataset,
        "profile":"semantica.refinery-rust-subject-observation/1",
        "engineering_adoption":"unknown","promotion":"not_performed"}),
    )
}

/// Produce the complete current receipt while the actual BookRunner and Dataset
/// are still retained together. This constructor has no `passed` input.
pub(crate) fn run_current(
    provider: &dyn SourceProvider,
    manifest: &Value,
    scenario: &str,
    runtime_identity: &Value,
    current_source: &Value,
) -> Result<Value> {
    run_current_optional(
        provider,
        manifest,
        Some(scenario),
        runtime_identity,
        current_source,
    )
}
pub(crate) fn run_current_optional(
    provider: &dyn SourceProvider,
    manifest: &Value,
    scenario: Option<&str>,
    runtime_identity: &Value,
    current_source: &Value,
) -> Result<Value> {
    let catalog = Catalog::from_execution_manifest(manifest)?;
    let mut session = DatasetSession::new();
    let mut state = BookRunnerState::default();
    let actual_run = book_runner::run(
        &mut state,
        &catalog,
        provider,
        &mut session,
        manifest["package_id"].as_str().unwrap_or(""),
        scenario,
        runtime_identity,
    )?;
    book_runner::verify(&state, &mut session, &actual_run, runtime_identity)?;
    let release_content = book_runner::release_content(
        &state,
        &mut session,
        &actual_run,
        runtime_identity,
        current_source,
    )?;
    book_runner::verify_release_content(
        &state,
        &mut session,
        &release_content,
        runtime_identity,
        current_source,
    )?;
    Ok(serde_json::to_value(
        semantica_contracts::SemanticRefineryRustSubjectSnapshot {
            profile: crate::refinery_subject::CURRENT_SNAPSHOT.into(),
            actual_run,
            release_content,
        },
    )?)
}
