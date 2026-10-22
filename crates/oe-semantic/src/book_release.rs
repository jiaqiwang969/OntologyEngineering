//! Source-bound local book candidates. Technical consistency never grants publication.

use crate::{
    release_evidence::{self, EvidenceOptions},
    release_sources::{self, ReleaseSourcesSnapshot},
};
use anyhow::{Context, Result, ensure};
use oe_core::{
    SkillRoot, fingerprint_file, no_symlinks, read_regular_bytes, sha256, write_new,
    write_new_bytes,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SCHEMA: &str = "ontology-engineering.book-release-artifacts/v2";
pub const MANIFEST_PATH: &str = "references/book-release-artifacts.v2.json";
pub const SOURCE_LOCK_PATH: &str = "runtime/semantica-source-lock.json";
pub const VISUAL_REVIEW_PATH: &str = "references/release-evidence/pdf-visual-review.json";
pub const EVIDENCE_PATHS: &[(&str, &str)] = &[
    (
        "authoring_locks",
        "references/release-evidence/v2/authoring-locks.json",
    ),
    (
        "book_source_bindings",
        "references/release-evidence/v2/book-source-bindings.json",
    ),
    (
        "ontology_engineering_regression",
        "references/release-evidence/v2/ontology-engineering-regression.json",
    ),
    ("pdf_qa", "references/release-evidence/v2/pdf-qa.json"),
    ("privacy", "references/release-evidence/v2/privacy.json"),
    (
        "runtime_identity",
        "references/release-evidence/v2/runtime-identity.json",
    ),
    (
        "semantica_regression",
        "references/release-evidence/v2/semantica-regression.json",
    ),
];
pub const GOVERNANCE_PATHS: &[(&str, &str)] = &[
    (
        "publication_approval",
        "references/release-evidence/v2/publication-approval.json",
    ),
    ("rights", "references/release-evidence/v2/rights.json"),
];
pub const REGRESSION_LOG_PATHS: &[(&str, &str)] = &[
    (
        "ontology_engineering_regression",
        "references/release-evidence/v2/ontology-engineering-regression.log",
    ),
    (
        "semantica_regression",
        "references/release-evidence/v2/semantica-regression.log",
    ),
];
#[derive(Debug, Clone, Copy)]
pub struct BookSpec {
    pub volume: &'static str,
    pub title: &'static str,
    pub pdf: &'static str,
    pub lock_prefix: &'static str,
}
pub const BOOK_SPECS: &[BookSpec] = &[
    BookSpec {
        volume: "vol1",
        title: "工程本体论",
        pdf: "references/ontology-engineering-book/handbook/工程本体论-全书.pdf",
        lock_prefix: "references/ontology-engineering-book/",
    },
    BookSpec {
        volume: "vol2",
        title: "产品可信工程",
        pdf: "references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf",
        lock_prefix: "references/product-trustworthiness-book/",
    },
];
pub const JSON_LIMIT: usize = 32 * 1024 * 1024;
pub const ARTIFACT_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
static WRITE_SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn canonical_bytes(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = crate::canonical_json(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn exact_keys(value: &Value, expected: &[&str], label: &str) -> Result<()> {
    let object = value
        .as_object()
        .with_context(|| format!("{label}_not_object"))?;
    let actual: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    ensure!(
        actual == expected.iter().copied().collect(),
        "{label}_keys_differ"
    );
    Ok(())
}

pub fn artifact_ref(root: &SkillRoot, relative: &str) -> Result<Value> {
    crate::safe_relative(relative)?;
    let identity = fingerprint_file(&root.path().join(relative), ARTIFACT_LIMIT)?;
    Ok(json!({"path":relative,"sha256":identity.sha256,"size_bytes":identity.bytes}))
}

pub fn verify_artifact_ref(root: &SkillRoot, value: &Value, label: &str) -> Result<PathBuf> {
    exact_keys(value, &["path", "sha256", "size_bytes"], label)?;
    let relative = value["path"].as_str().context("artifact_path_not_text")?;
    let digest = value["sha256"]
        .as_str()
        .context("artifact_sha256_not_text")?;
    ensure!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "artifact_sha256_invalid"
    );
    ensure!(
        value["size_bytes"].as_u64().is_some(),
        "artifact_size_invalid"
    );
    ensure!(
        artifact_ref(root, relative)? == *value,
        "{label}_identity_changed"
    );
    Ok(root.path().join(relative))
}

pub fn read_document(root: &SkillRoot, relative: &str, canonical: bool) -> Result<Value> {
    Ok(read_bound_document(root, relative, canonical)?.1)
}

fn read_bound_bytes(root: &SkillRoot, relative: &str, limit: usize) -> Result<(Value, Vec<u8>)> {
    crate::safe_relative(relative)?;
    let bytes = read_regular_bytes(&root.path().join(relative), limit)?;
    let reference = json!({"path":relative,"sha256":sha256(&bytes),"size_bytes":bytes.len()});
    Ok((reference, bytes))
}

/// The reference and parsed document always describe the same captured bytes.
/// Callers retaining the document across work must recheck this reference.
pub fn read_bound_document(
    root: &SkillRoot,
    relative: &str,
    canonical: bool,
) -> Result<(Value, Value)> {
    let (reference, bytes) = read_bound_bytes(root, relative, JSON_LIMIT)?;
    let value = crate::strict_json(&bytes)?;
    ensure!(value.is_object(), "book_document_not_object");
    if canonical {
        ensure!(
            canonical_bytes(&value)? == bytes,
            "book_document_not_canonical"
        );
    }
    Ok((reference, value))
}

pub fn evidence_path(kind: &str) -> Result<&'static str> {
    EVIDENCE_PATHS
        .iter()
        .find_map(|(k, p)| (*k == kind).then_some(*p))
        .context("book_evidence_kind_unknown")
}

/// Each file is published separately, with exact before/after bytes retained.
/// The participating-writer lock and drift checks are not a filesystem CAS transaction.
pub fn write_document(root: &SkillRoot, relative: &str, value: &Value) -> Result<()> {
    write_artifact_bytes(root, relative, &canonical_bytes(value)?)
}

pub fn write_artifact_bytes(root: &SkillRoot, relative: &str, bytes: &[u8]) -> Result<()> {
    crate::safe_relative(relative)?;
    ensure!(
        relative == MANIFEST_PATH
            || relative == format!("{MANIFEST_PATH}.sha256")
            || EVIDENCE_PATHS
                .iter()
                .chain(GOVERNANCE_PATHS)
                .chain(REGRESSION_LOG_PATHS)
                .any(|(_, p)| *p == relative),
        "book_publication_path_not_declared"
    );
    ensure!(bytes.len() <= JSON_LIMIT, "book_publication_exceeds_limit");
    let target = root.path().join(relative);
    no_symlinks(&target)?;
    let coordination = root.path().join("var/state");
    no_symlinks(&coordination)?;
    oe_core::ensure_directory(&coordination)?;
    let guard = oe_core::AdvisoryFileLock::acquire(
        &coordination.join("book-release-publication.lock"),
        true,
    )?;
    let before = if target.try_exists()? {
        Some(read_regular_bytes(&target, JSON_LIMIT)?)
    } else {
        None
    };
    let unique = format!(
        "var/maintenance/book-release-write-{}-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        WRITE_SERIAL.fetch_add(1, Ordering::Relaxed)
    );
    let work = root.new_private_dir(Path::new(&unique))?;
    if let Some(original) = &before {
        write_new_bytes(&work.join("before"), original)?;
    }
    write_new_bytes(&work.join("after"), bytes)?;
    let prepared = work.join("prepared");
    write_new_bytes(&prepared, bytes)?;
    write_new(
        &work.join("intent.json"),
        &json!({"path":relative,"before_sha256":before.as_ref().map(sha256),"after_sha256":sha256(bytes),"scope":"one_file_not_atomic_batch"}),
    )?;
    let parent = target.parent().context("publication_parent_missing")?;
    no_symlinks(parent)?;
    oe_core::ensure_directory(parent)?;
    no_symlinks(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            fs::metadata(&work)?.dev() == fs::metadata(parent)?.dev(),
            "book_publication_cross_device"
        );
    }
    guard.ensure_current()?;
    let current = if target.try_exists()? {
        Some(read_regular_bytes(&target, JSON_LIMIT)?)
    } else {
        None
    };
    ensure!(current == before, "book_publication_target_changed");
    no_symlinks(&target)?;
    oe_core::rename_regular_file(&prepared, &target)?;
    ensure!(
        read_regular_bytes(&target, JSON_LIMIT)? == bytes,
        "book_publication_verify_failed"
    );
    guard.ensure_current()?;
    write_new(
        &work.join("published.json"),
        &json!({"path":relative,"sha256":sha256(bytes),"status":"published"}),
    )?;
    Ok(())
}

const RIGHTS_SOURCES: &[&str] = &[
    "docs/PRIVACY-AND-RIGHTS.md",
    "docs/PUBLIC-RELEASE-STATUS.md",
    "references/product-trustworthiness-book/handbook/book-metadata.tex",
];

fn governance_path(kind: &str) -> Result<&'static str> {
    GOVERNANCE_PATHS
        .iter()
        .find_map(|(k, p)| (*k == kind).then_some(*p))
        .context("governance_kind_unknown")
}

pub fn verify_governance(
    root: &SkillRoot,
    kind: &str,
    document: &Value,
    rights_sha256: Option<&str>,
) -> Result<()> {
    if kind == "rights" {
        exact_keys(
            document,
            &[
                "$schema",
                "reason",
                "required_authority",
                "status",
                "sources",
            ],
            "rights",
        )?;
        ensure!(
            document["$schema"] == "ontology-engineering.rights-evidence/v1",
            "rights_schema_invalid"
        );
        let sources = document["sources"]
            .as_array()
            .context("rights_sources_not_array")?;
        ensure!(!sources.is_empty(), "rights_sources_empty");
        let mut names = BTreeSet::new();
        for source in sources {
            verify_artifact_ref(root, source, "rights_source")?;
            ensure!(
                names.insert(
                    source["path"]
                        .as_str()
                        .context("rights_source_path_invalid")?
                ),
                "rights_source_duplicate"
            );
        }
    } else {
        ensure!(kind == "publication_approval", "governance_kind_unknown");
        exact_keys(
            document,
            &[
                "$schema",
                "reason",
                "required_authority",
                "status",
                "rights_evidence_sha256",
                "scope",
            ],
            "publication_approval",
        )?;
        ensure!(
            document["$schema"] == "ontology-engineering.publication-approval/v1",
            "publication_schema_invalid"
        );
        ensure!(
            document["scope"] == "two-book-artifact-publication",
            "publication_scope_invalid"
        );
        ensure!(
            rights_sha256.is_some() && document["rights_evidence_sha256"].as_str() == rights_sha256,
            "publication_rights_binding_invalid"
        );
    }
    ensure!(
        matches!(document["status"].as_str(), Some("pending" | "blocked")),
        "unsigned_governance_cannot_approve"
    );
    for field in ["reason", "required_authority"] {
        ensure!(
            document[field]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty()),
            "governance_text_missing"
        );
    }
    Ok(())
}

pub fn initialize_governance(root: &SkillRoot, reset_existing: bool) -> Result<Value> {
    let rights_path = governance_path("rights")?;
    let approval_path = governance_path("publication_approval")?;
    for p in [rights_path, approval_path] {
        no_symlinks(&root.path().join(p))?;
    }
    let rights_exists = root.path().join(rights_path).try_exists()?;
    let approval_exists = root.path().join(approval_path).try_exists()?;
    ensure!(rights_exists == approval_exists, "governance_pair_partial");
    if rights_exists && !reset_existing {
        let (identity, rights) = read_bound_document(root, rights_path, true)?;
        let (approval_identity, approval) = read_bound_document(root, approval_path, true)?;
        verify_governance(root, "rights", &rights, None)?;
        verify_governance(
            root,
            "publication_approval",
            &approval,
            identity["sha256"].as_str(),
        )?;
        verify_artifact_ref(root, &identity, "rights")?;
        verify_artifact_ref(root, &approval_identity, "publication_approval")?;
        return Ok(
            json!({"passed":true,"publication_approval":approval["status"],"rights":rights["status"],"changed":false}),
        );
    }
    let sources: Vec<Value> = RIGHTS_SOURCES
        .iter()
        .map(|p| artifact_ref(root, p))
        .collect::<Result<_>>()?;
    let rights = json!({"$schema":"ontology-engineering.rights-evidence/v1","reason":"Rights holder, licenses, ISBN/CIP, and public redistribution approval remain unconfirmed; technical integration cannot decide them.","required_authority":"authorized-rights-holder-and-publisher","sources":sources,"status":"pending"});
    let rights_sha = sha256(canonical_bytes(&rights)?);
    let approval = json!({"$schema":"ontology-engineering.publication-approval/v1","reason":"No authorized person has approved public release of the two-book artifact; this record is intentionally pending.","required_authority":"authorized-publication-approver","rights_evidence_sha256":rights_sha,"scope":"two-book-artifact-publication","status":"pending"});
    verify_governance(root, "rights", &rights, None)?;
    verify_governance(root, "publication_approval", &approval, Some(&rights_sha))?;
    // All source documents and both records are prepared before publishing either file.
    // If the second file cannot be published, the first and its recovery record remain.
    write_document(root, rights_path, &rights)?;
    write_document(root, approval_path, &approval)?;
    ensure!(
        read_document(root, rights_path, true)? == rights
            && read_document(root, approval_path, true)? == approval,
        "governance_changed_during_publication"
    );
    verify_governance(root, "rights", &rights, None)?;
    Ok(json!({"passed":true,"publication_approval":"pending","rights":"pending","changed":true}))
}

pub fn compute_release_blockers(manifest: &Value) -> Vec<String> {
    let mut blockers = BTreeSet::new();
    for kind in ["publication_approval", "rights"] {
        blockers.insert(format!("governance.{kind}"));
    }
    if let Some(verification) = manifest["verification"].as_object() {
        for (kind, _) in EVIDENCE_PATHS {
            if verification
                .get(*kind)
                .is_none_or(|value| value["status"] != "passed")
            {
                blockers.insert(format!("verification.{kind}"));
            }
        }
    } else {
        blockers.insert("verification.invalid".into());
    }
    if let Some(packages) = manifest["packages"].as_array().filter(|p| !p.is_empty()) {
        for package in packages {
            if package["status"] != "complete" || package["release_status"] != "complete" {
                blockers.insert(format!(
                    "package.{}",
                    package["package_id"].as_str().unwrap_or("invalid")
                ));
            }
        }
    } else {
        blockers.insert("packages.missing".into());
    }
    // These local provenance records can reference private execution artifacts.
    // A byte-closed local candidate is not a portable public evidence package.
    blockers.insert("distribution.local_candidate_evidence".into());
    blockers.into_iter().collect()
}

#[derive(Debug, Clone)]
pub struct CreateOptions {
    pub oe_source_commit: String,
    pub evidence: EvidenceOptions,
    pub claim_release: bool,
}

#[derive(Debug, Clone)]
pub struct VerifyOptions {
    pub manifest: String,
    pub evidence: EvidenceOptions,
}

fn capture_refs(value: &Value, refs: &mut BTreeMap<String, Value>) -> Result<()> {
    match value {
        Value::Object(map) => {
            if map.len() == 3
                && ["path", "sha256", "size_bytes"]
                    .iter()
                    .all(|k| map.contains_key(*k))
            {
                let path = value["path"]
                    .as_str()
                    .context("book_reference_path_invalid")?
                    .to_owned();
                if let Some(previous) = refs.get(&path) {
                    ensure!(previous == value, "book_conflicting_reference_identities");
                } else {
                    refs.insert(path, value.clone());
                }
            } else {
                for v in map.values() {
                    capture_refs(v, refs)?;
                }
            }
        }
        Value::Array(values) => {
            for v in values {
                capture_refs(v, refs)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn refs_current(root: &SkillRoot, refs: &BTreeMap<String, Value>) -> Result<()> {
    for reference in refs.values() {
        verify_artifact_ref(root, reference, "book_candidate_input")?;
    }
    Ok(())
}

fn capture_evidence_refs(
    root: &SkillRoot,
    kind: &str,
    document: &Value,
    refs: &mut BTreeMap<String, Value>,
) -> Result<()> {
    // Git file identities are relative to their own repository. In particular,
    // source_identity.git.semantica.files is NOT a skill-root artifact namespace.
    // The separate Git source proofs cover those exact bytes and their roots.
    capture_refs(&document["execution"], refs)?;
    capture_refs(&document["source_identity"]["inputs"], refs)?;
    match kind {
        "pdf_qa" => {
            capture_refs(&document["observation"]["visual_review"], refs)?;
            for book in document["observation"]["books"]
                .as_array()
                .context("book_pdf_observations_missing")?
            {
                capture_refs(&book["pdf"], refs)?;
            }
        }
        "ontology_engineering_regression" | "semantica_regression" => {
            capture_refs(&document["observation"]["log"], refs)?;
        }
        "book_source_bindings" => {
            if let Some(inputs) = document["observation"]["input_sha256"].as_object() {
                for (path, expected) in inputs {
                    let reference = artifact_ref(root, path)?;
                    ensure!(
                        reference["sha256"] == *expected,
                        "book_native_input_changed"
                    );
                    capture_refs(&reference, refs)?;
                }
            }
        }
        "authoring_locks" | "runtime_identity" | "privacy" => {}
        _ => anyhow::bail!("book_evidence_kind_unknown"),
    }
    Ok(())
}

struct Assessment {
    manifest: Value,
    sources: ReleaseSourcesSnapshot,
    references: BTreeMap<String, Value>,
    source_state: Value,
    semantica_commit: String,
    semantica_root: PathBuf,
    output: PathBuf,
    runtime_guard: crate::runtime::RuntimeAccessGuard,
}

fn commit_text(value: &str) -> Result<()> {
    ensure!(
        value.len() == 40
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "book_source_commit_invalid"
    );
    Ok(())
}

fn child_options(
    options: &EvidenceOptions,
    output: &Path,
    name: &str,
    commit: &str,
) -> EvidenceOptions {
    let mut child = options.clone();
    child.output = output.join(name);
    child.oe_source_commit = Some(commit.to_owned());
    child
}

fn assessment(root: &SkillRoot, commit: &str, options: &EvidenceOptions) -> Result<Assessment> {
    commit_text(commit)?;
    let semantica_root = options
        .semantica_root
        .as_ref()
        .context("book_semantica_checkout_required")?;
    let semantica_root = root.expand(semantica_root)?;
    no_symlinks(&semantica_root)?;
    let semantica_root = fs::canonicalize(&semantica_root)?;
    let runtime_guard = crate::runtime::RuntimeAccessGuard::shared(root.path())?;
    let sources = ReleaseSourcesSnapshot::capture(root)?;
    let (source_lock_ref, source_lock) = read_bound_document(root, SOURCE_LOCK_PATH, false)?;
    ensure!(
        source_lock_ref == sources.artifact_refs()["source_lock"],
        "book_source_lock_snapshot_changed"
    );
    let semantica_commit = source_lock["source"]["commit"]
        .as_str()
        .context("book_semantica_commit_missing")?
        .to_owned();
    commit_text(&semantica_commit)?;
    let output = root.new_private_dir(&options.output)?;
    let source_state = release_sources::verify_source_boundary(
        root,
        commit,
        &options.git,
        &output.join("source-before"),
    )?;
    release_sources::verify_semantica_checkout(
        root,
        &semantica_root,
        &semantica_commit,
        &options.git,
        &output.join("semantica-before"),
    )?;

    let mut references = BTreeMap::new();
    capture_refs(sources.artifact_refs(), &mut references)?;
    let mut documents = BTreeMap::new();
    let mut verification = serde_json::Map::new();
    for (kind, path) in EVIDENCE_PATHS {
        let (reference, document) = read_bound_document(root, path, true)?;
        release_evidence::validate_document(root, kind, &document)?;
        capture_refs(&reference, &mut references)?;
        capture_evidence_refs(root, kind, &document, &mut references)?;
        verification.insert((*kind).to_owned(), json!({"evidence":reference,"status":if document["passed"] == true {"passed"} else {"blocked"}}));
        documents.insert((*kind).to_owned(), document);
    }
    let rights_path = governance_path("rights")?;
    let (rights_ref, rights) = read_bound_document(root, rights_path, true)?;
    verify_governance(root, "rights", &rights, None)?;
    let mut governance = serde_json::Map::new();
    for (kind, path) in GOVERNANCE_PATHS {
        let (reference, document) = if *kind == "rights" {
            (rights_ref.clone(), rights.clone())
        } else {
            read_bound_document(root, path, true)?
        };
        verify_governance(root, kind, &document, rights_ref["sha256"].as_str())?;
        capture_refs(&reference, &mut references)?;
        capture_refs(&document, &mut references)?;
        governance.insert((*kind).to_owned(), json!({"evidence":reference,"status":document["status"],"required_authority":document["required_authority"]}));
    }

    let pdf = documents
        .get("pdf_qa")
        .context("book_pdf_evidence_missing")?;
    let observations = pdf["observation"]["books"]
        .as_array()
        .context("book_pdf_observations_missing")?;
    ensure!(
        observations.len() == BOOK_SPECS.len(),
        "book_pdf_observations_incomplete"
    );
    let mut books = Vec::new();
    for (spec, observation) in BOOK_SPECS.iter().zip(observations) {
        ensure!(
            observation["volume"] == spec.volume,
            "book_pdf_order_invalid"
        );
        let reference = artifact_ref(root, spec.pdf)?;
        ensure!(
            observation["pdf"] == reference,
            "book_pdf_evidence_identity_changed"
        );
        capture_refs(&reference, &mut references)?;
        let pages = observation["page_count"]
            .as_u64()
            .context("book_pdf_page_count_invalid")?;
        ensure!(pages > 0, "book_pdf_page_count_invalid");
        let locks = sources.authoring_locks()[spec.volume]
            .as_array()
            .context("book_authoring_locks_invalid")?;
        let lock_refs = locks
            .iter()
            .map(|p| artifact_ref(root, p.as_str().context("book_lock_path_invalid")?))
            .collect::<Result<Vec<_>>>()?;
        let mut pdf_record = reference
            .as_object()
            .context("book_pdf_ref_invalid")?
            .clone();
        pdf_record.insert("page_count".into(), json!(pages));
        books.push(json!({"volume":spec.volume,"title":spec.title,"pdf":pdf_record,"authoring_locks":lock_refs}));
    }
    let mut manifest = json!({
        "$schema":SCHEMA,"artifact_status":"candidate","evidence_scope":"local_candidate",
        "books":books,"governance":governance,
        "ontology_engineering":{"repository":"https://github.com/jiaqiwang969/OntologyEngineering.git","source_commit":commit,"source_tree":source_state["source_identity"]["source_tree"]},
        "packages":sources.packages(),
        "release_verdict":{"eligible":false,"blockers":[]},
        "semantica":{"commit":semantica_commit,"repository":source_lock["source"]["canonical_repository"],"version":source_lock["source"]["version"],"source_lock":sources.artifact_refs()["source_lock"],"wheel":sources.artifact_refs()["wheel"]},
        "verification":verification,
    });
    ensure!(
        manifest["ontology_engineering"]["source_tree"]
            .as_str()
            .is_some(),
        "book_source_tree_missing"
    );
    refs_current(root, &references)?;
    sources.ensure_current(root)?;
    write_new(
        &output.join("bound-inputs.json"),
        &json!({"artifact_refs":references,"source_state":source_state,"source_lock":sources.artifact_refs()["source_lock"]}),
    )?;

    let mut replay_options = options.clone();
    replay_options.semantica_root = Some(semantica_root.clone());
    replay_options.visual_review = Some(
        root.path().join(
            pdf["observation"]["visual_review"]["path"]
                .as_str()
                .unwrap_or(VISUAL_REVIEW_PATH),
        ),
    );
    let operations = [
        (
            "static",
            release_evidence::observe_static as fn(&SkillRoot, &EvidenceOptions) -> Result<Value>,
        ),
        ("book-bindings", release_evidence::observe_book_bindings),
        ("regressions", release_evidence::observe_regressions),
        ("pdf-qa", release_evidence::observe_pdf_qa),
    ];
    let mut seen = BTreeSet::new();
    for (label, run) in operations {
        let fresh = run(
            root,
            &child_options(&replay_options, &output, label, commit),
        )?;
        write_new(&output.join(format!("{label}-replay.json")), &fresh)?;
        for (kind, document) in fresh.as_object().context("book_replay_not_object")? {
            ensure!(seen.insert(kind.clone()), "book_replay_kind_duplicate");
            let stored = documents.get(kind).context("book_replay_kind_unknown")?;
            release_evidence::compare_replay(root, kind, stored, document)?;
        }
    }
    ensure!(
        seen == EVIDENCE_PATHS
            .iter()
            .map(|(k, _)| (*k).to_owned())
            .collect(),
        "book_replay_incomplete"
    );
    sources.ensure_current(root)?;
    refs_current(root, &references)?;
    release_sources::ensure_source_unchanged(
        root,
        &source_state,
        &options.git,
        &output.join("source-after"),
    )?;
    release_sources::verify_semantica_checkout(
        root,
        &semantica_root,
        &semantica_commit,
        &options.git,
        &output.join("semantica-after"),
    )?;
    let blockers = compute_release_blockers(&manifest);
    manifest["release_verdict"] = json!({"eligible":false,"blockers":blockers});
    write_new(&output.join("assessed-manifest.json"), &manifest)?;
    runtime_guard.ensure_current()?;
    Ok(Assessment {
        manifest,
        sources,
        references,
        source_state,
        semantica_commit,
        semantica_root,
        output,
        runtime_guard,
    })
}

fn after_publication(
    root: &SkillRoot,
    assessment: &Assessment,
    options: &EvidenceOptions,
) -> Result<()> {
    assessment.runtime_guard.ensure_current()?;
    assessment.sources.ensure_current(root)?;
    refs_current(root, &assessment.references)?;
    release_sources::ensure_source_unchanged(
        root,
        &assessment.source_state,
        &options.git,
        &assessment.output.join("source-final"),
    )?;
    release_sources::verify_semantica_checkout(
        root,
        &assessment.semantica_root,
        &assessment.semantica_commit,
        &options.git,
        &assessment.output.join("semantica-final"),
    )?;
    assessment.sources.ensure_current(root)?;
    refs_current(root, &assessment.references)?;
    assessment.runtime_guard.ensure_current()?;
    Ok(())
}

fn verdict(manifest: &Value, raw: &[u8]) -> Value {
    json!({"passed":true,"artifact_status":"candidate","evidence_scope":"local_candidate","release_eligible":false,"release_blockers":manifest["release_verdict"]["blockers"],"manifest_sha256":sha256(raw),"package_count":manifest["packages"].as_array().map_or(0, Vec::len),"schema_version":SCHEMA})
}

pub fn create_manifest(root: &SkillRoot, options: &CreateOptions) -> Result<Value> {
    ensure!(
        !options.claim_release,
        "book_candidate_cannot_claim_release"
    );
    let assessment = assessment(root, &options.oe_source_commit, &options.evidence)?;
    let bytes = canonical_bytes(&assessment.manifest)?;
    write_artifact_bytes(root, MANIFEST_PATH, &bytes)?;
    let sidecar = format!(
        "{}  {}\n",
        sha256(&bytes),
        Path::new(MANIFEST_PATH)
            .file_name()
            .context("manifest_filename_missing")?
            .to_str()
            .context("manifest_filename_invalid")?
    );
    write_artifact_bytes(root, &format!("{MANIFEST_PATH}.sha256"), sidecar.as_bytes())?;
    ensure!(
        read_regular_bytes(&root.path().join(MANIFEST_PATH), JSON_LIMIT)? == bytes,
        "manifest_changed_during_publication"
    );
    ensure!(
        read_regular_bytes(&root.path().join(format!("{MANIFEST_PATH}.sha256")), 4096)?
            == sidecar.as_bytes(),
        "manifest_sidecar_changed"
    );
    after_publication(root, &assessment, &options.evidence)?;
    ensure!(
        read_regular_bytes(&root.path().join(MANIFEST_PATH), JSON_LIMIT)? == bytes,
        "manifest_changed_during_final_verification"
    );
    ensure!(
        read_regular_bytes(&root.path().join(format!("{MANIFEST_PATH}.sha256")), 4096)?
            == sidecar.as_bytes(),
        "manifest_sidecar_changed_during_final_verification"
    );
    let report = verdict(&assessment.manifest, &bytes);
    write_new(&assessment.output.join("report.json"), &report)?;
    Ok(report)
}

pub fn verify_manifest(root: &SkillRoot, options: &VerifyOptions) -> Result<Value> {
    crate::safe_relative(&options.manifest)?;
    let (input, manifest) = read_bound_document(root, &options.manifest, true)?;
    exact_keys(
        &manifest,
        &[
            "$schema",
            "artifact_status",
            "evidence_scope",
            "books",
            "governance",
            "ontology_engineering",
            "packages",
            "release_verdict",
            "semantica",
            "verification",
        ],
        "book_manifest",
    )?;
    ensure!(
        manifest["$schema"] == SCHEMA,
        "book_manifest_schema_not_current"
    );
    ensure!(
        manifest["artifact_status"] == "candidate"
            && manifest["evidence_scope"] == "local_candidate",
        "book_manifest_scope_invalid"
    );
    let commit = manifest["ontology_engineering"]["source_commit"]
        .as_str()
        .context("book_source_commit_missing")?;
    commit_text(commit)?;
    let sidecar_path = format!("{}.sha256", options.manifest);
    let (sidecar_ref, sidecar_bytes) = read_bound_bytes(root, &sidecar_path, 4096)?;
    let expected = format!(
        "{}  {}\n",
        input["sha256"].as_str().context("manifest_sha_invalid")?,
        Path::new(&options.manifest)
            .file_name()
            .context("manifest_name_missing")?
            .to_str()
            .context("manifest_name_invalid")?
    );
    ensure!(
        sidecar_bytes == expected.as_bytes(),
        "book_manifest_sidecar_invalid"
    );
    let assessment = assessment(root, commit, &options.evidence)?;
    ensure!(
        manifest == assessment.manifest,
        "book_manifest_differs_from_bound_fresh_replay"
    );
    verify_artifact_ref(root, &input, "book_manifest")?;
    verify_artifact_ref(root, &sidecar_ref, "book_manifest_sidecar")?;
    after_publication(root, &assessment, &options.evidence)?;
    verify_artifact_ref(root, &input, "book_manifest")?;
    verify_artifact_ref(root, &sidecar_ref, "book_manifest_sidecar")?;
    let bytes = canonical_bytes(&manifest)?;
    let report = verdict(&manifest, &bytes);
    write_new(&assessment.output.join("report.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
#[path = "book_release_tests.rs"]
mod tests;
