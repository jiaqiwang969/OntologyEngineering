//! Current Rust book-source execution identity. This adapter never creates a
//! legacy RECORD/source-lock identity and never grants engineering adoption.
use anyhow::{Context, Result, ensure};
use oe_contracts::{BookSourceExecution, BookSourceInputBinding};
use oe_core::{SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[path = "../../../../../build_support/book_source.rs"]
mod inventory;

const SCHEMA: &str = "ontology-engineering.book-source-rust-execution/v1";
const LIMIT: usize = 32 * 1024 * 1024;

/// Public source bytes, already part of the skill distribution. Kept in one
/// table so production identity and isolated source-complete roots agree.
/// No private book content or credentials belong here.
pub const IMPLEMENTATION_SOURCES: &[(&str, &[u8])] =
    include!(concat!(env!("OUT_DIR"), "/book-source-inputs.rs"));

fn implementation(root: &SkillRoot) -> Result<Vec<BookSourceInputBinding>> {
    let mut values = BTreeMap::new();
    for &(path, compiled) in IMPLEMENTATION_SOURCES {
        ensure!(
            read_regular_bytes(&root.path().join(path), LIMIT)? == compiled,
            "compiled_book_source_execution_mismatch:{path}"
        );
        ensure!(
            values
                .insert(path.to_owned(), Some(sha256(compiled)))
                .is_none(),
            "book_execution_source_duplicate"
        );
    }
    let current: BTreeSet<_> = inventory::inputs(root.path())?
        .into_iter()
        .map(|path| {
            path.to_str()
                .context("book_compile_path_not_utf8")
                .map(|name| name.replace(std::path::MAIN_SEPARATOR, "/"))
        })
        .collect::<Result<_>>()?;
    ensure!(
        current == values.keys().cloned().collect(),
        "book_execution_source_set_changed"
    );
    Ok(bindings(values))
}
fn bindings(values: BTreeMap<String, Option<String>>) -> Vec<BookSourceInputBinding> {
    values
        .into_iter()
        .map(|(path, sha256)| BookSourceInputBinding { path, sha256 })
        .collect()
}
fn executable() -> Result<oe_core::FileIdentity> {
    fingerprint_file(&std::env::current_exe()?, 512 * 1024 * 1024)
}
fn source_binding(root: &SkillRoot, path: &str) -> Result<Option<String>> {
    crate::safe_relative(path)?;
    let full = root.path().join(path);
    no_symlinks(&full)?;
    Ok(if full.try_exists()? {
        Some(sha256(read_regular_bytes(&full, LIMIT)?))
    } else {
        None
    })
}
fn verify_inputs(root: &SkillRoot, inputs: &[BookSourceInputBinding]) -> Result<()> {
    let mut paths = BTreeSet::new();
    for input in inputs {
        ensure!(paths.insert(&input.path), "book_source_input_duplicate");
        ensure!(
            source_binding(root, &input.path)? == input.sha256,
            "book_source_input_changed:{}",
            input.path
        );
    }
    Ok(())
}
fn result_sha(value: &Value) -> Result<String> {
    Ok(sha256(crate::canonical_json(value)?))
}
fn response(
    root: &SkillRoot,
    result: Value,
    semantic_execution: &str,
    book_root: String,
    inputs: BTreeMap<String, Option<String>>,
    archive_sha: Option<String>,
    error: Option<String>,
    implementation_sources: Vec<BookSourceInputBinding>,
    exe: oe_core::FileIdentity,
) -> Result<Value> {
    let input_bindings = bindings(inputs);
    verify_inputs(root, &input_bindings)?;
    ensure!(
        implementation(root)? == implementation_sources,
        "book_execution_implementation_drift"
    );
    ensure!(executable()? == exe, "book_execution_executable_drift");
    let proof = BookSourceExecution {
        schema: SCHEMA.into(),
        engine: "semantica.rust-runtime".into(),
        semantic_execution: semantic_execution.into(),
        approved_archive_sha256: super::APPROVED_ARCHIVE_SHA256.into(),
        source_archive_sha256: archive_sha.clone(),
        implementation_sources,
        executable_sha256: exe.sha256,
        executable_size_bytes: exe.bytes,
        book_root,
        input_bindings,
        result_sha256: result_sha(&result)?,
        error: error.clone(),
    };
    proof
        .validate()
        .map_err(|e| anyhow::anyhow!("book_source_execution_contract:{e:?}"))?;
    let hashes = proof
        .input_bindings
        .iter()
        .filter_map(|i| i.sha256.as_ref().map(|h| (i.path.clone(), h.clone())))
        .collect::<BTreeMap<_, _>>();
    Ok(json!({
        "schema":"ontology-engineering.book-source-rust-response/v1","operation":"book_verify_sources",
        "status":if semantic_execution=="completed" && result["passed"]==true {"completed"} else {"blocked"},
        "scope":"source-bound Rust book API; project fact adoption requires separate evidence",
        "semantic_execution":semantic_execution,"engine":"semantica.rust-runtime",
        "source_archive_sha256":archive_sha,"rust_execution":proof,
        "input_sha256":hashes,"validation_error":error,
        "fact_admitted":false,"ontology_promotion":"not_performed","publication":"not_performed",
        "result":result
    }))
}

/// Executes the same Catalog checker as verify_sources, with source attempts,
/// current compiled code and executable identity retained independently.
pub fn verify_response(root: &SkillRoot) -> Result<Value> {
    let compiled = implementation(root)?;
    let exe = executable()?;
    let mut inputs = BTreeMap::new();
    for path in [super::ARCHIVE, "sources/continuity.json"] {
        inputs.insert(path.into(), source_binding(root, path)?);
    }
    let archive_sha = inputs[super::ARCHIVE].clone();
    let provider = match super::ArchiveProvider::open(root.path()) {
        Ok(p) => p,
        Err(e) => {
            let error = format!("approved_book_source_unavailable:{e}");
            let result = json!({"schema_version":"1.0","passed":false,"status":"not_run",
                "check_count":0,"checks":[],"reason":error});
            return response(
                root,
                result,
                "not_run",
                ".".into(),
                inputs,
                archive_sha,
                Some(error),
                compiled,
                exe,
            );
        }
    };
    let catalog = provider.catalog()?;
    let authoring = root.path().join("sources/book-authoring");
    no_symlinks(&authoring)?;
    let (book_root, directory) = if authoring.is_dir() {
        ("sources/book-authoring".to_owned(), authoring)
    } else {
        (".".to_owned(), root.path().to_owned())
    };
    let files = super::FileProvider::new(BTreeMap::from([("book".into(), directory)]))?;
    let result = catalog.verify_book_source_bindings(
        &provider,
        &super::BookAnchors(&files),
        None,
        true,
        true,
    )?;
    files.verify()?;
    provider.verify()?;
    for path in files.attempted.borrow().iter() {
        let relative =
            oe_core::portable_source_path(root.path(), path).context("book_input_path_invalid")?;
        crate::safe_relative(&relative)?;
        let expected = files.inputs.borrow().get(path).cloned();
        ensure!(
            inputs.insert(relative, expected).is_none(),
            "book_input_overlaps_prerequisite"
        );
    }
    response(
        root,
        result,
        "completed",
        book_root,
        inputs,
        archive_sha,
        None,
        compiled,
        exe,
    )
}

/// Source inventory consumed by collection and replay. The caller still has
/// to validate the actual receipt; this projection grants no authenticity.
pub fn required_paths(wrapper: &Value) -> Result<BTreeSet<String>> {
    let proof: BookSourceExecution = serde_json::from_value(wrapper["rust_execution"].clone())
        .context("rust_book_execution_required")?;
    proof
        .validate()
        .map_err(|e| anyhow::anyhow!("book_source_execution_contract:{e:?}"))?;
    let mut paths: BTreeSet<String> = IMPLEMENTATION_SOURCES
        .iter()
        .map(|(p, _)| (*p).into())
        .collect();
    for input in &proof.input_bindings {
        crate::safe_relative(&input.path)?;
        paths.insert(input.path.clone());
    }
    Ok(paths)
}

/// Current release gate, including a fresh complete run of the only source
/// checker. Changing a receipt and recomputing its hashes cannot erase checks.
pub fn validate_response(root: &SkillRoot, wrapper: &Value) -> Result<bool> {
    let proof: BookSourceExecution = serde_json::from_value(wrapper["rust_execution"].clone())
        .context("rust_book_execution_required")?;
    proof
        .validate()
        .map_err(|e| anyhow::anyhow!("book_source_execution_contract:{e:?}"))?;
    ensure!(
        proof.approved_archive_sha256 == super::APPROVED_ARCHIVE_SHA256,
        "book_execution_approved_archive_changed"
    );
    ensure!(
        proof.implementation_sources == implementation(root)?,
        "book_execution_implementation_changed"
    );
    let exe = executable()?;
    ensure!(
        proof.executable_sha256 == exe.sha256 && proof.executable_size_bytes == exe.bytes,
        "book_execution_executable_changed"
    );
    verify_inputs(root, &proof.input_bindings)?;
    ensure!(
        proof.result_sha256 == result_sha(&wrapper["result"])?,
        "book_execution_result_sha256_changed"
    );
    let actual = verify_response(root)?;
    ensure!(wrapper == &actual, "book_source_report_not_reproduced");
    let checks = wrapper["result"]["checks"]
        .as_array()
        .context("book_checks_missing")?;
    let passed = proof.semantic_execution == "completed"
        && proof.error.is_none()
        && proof.source_archive_sha256.as_deref() == Some(super::APPROVED_ARCHIVE_SHA256)
        && wrapper["status"] == "completed"
        && wrapper["validation_error"].is_null()
        && wrapper["result"]["passed"] == true
        && wrapper["result"]["status"] == "passed"
        && !checks.is_empty()
        && wrapper["result"]["check_count"] == checks.len()
        && checks.iter().all(|c| c["status"] == "passed")
        && wrapper["fact_admitted"] == false
        && wrapper["ontology_promotion"] == "not_performed"
        && wrapper["publication"] == "not_performed";
    Ok(passed)
}

/// Actual current source/executable observations for a scoped book execution.
/// These pins compare requested bytes; they do not certify who compiled them.
/// The existing version authority must separately supervise origin/adoption.
pub fn runtime_identity(root: &SkillRoot) -> Result<Value> {
    let compiled = implementation(root)?;
    let executable = executable()?;
    let source_sha256 = sha256(crate::canonical_json(&serde_json::to_value(&compiled)?)?);
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("book_observation_clock_invalid")?;
    let created_at = format!(
        "{}Z",
        semantica_runtime::context::observation_time(elapsed.as_secs_f64())?
    );
    ensure!(
        implementation(root)? == compiled,
        "book_execution_implementation_drift"
    );
    ensure!(
        self::executable()? == executable,
        "book_execution_executable_drift"
    );
    Ok(
        json!({"schema":"ontology-engineering.book-task-runtime-observation/1",
        "engine":"semantica.rust-runtime","runtime_version":env!("CARGO_PKG_VERSION"),"created_at":created_at,"runtime_commit":format!("semantica.rust-source/{source_sha256}"),
        "source_sha256":source_sha256,"runtime_artifact_sha256":executable.sha256,
        "runtime_artifact_size_bytes":executable.bytes,"compilation_origin":"unknown",
        "identity_authority":"not_connected","engineering_adoption":"not_established"}),
    )
}
