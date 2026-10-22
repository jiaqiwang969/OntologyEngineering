//! Fresh, source-bound projections with an explicitly injected v2 executor.
//! This module validates transport and authority; oe-kernel executes semantics.
use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use oe_core::json::{canonical_json, strict_json};
use oe_core::{SkillRoot, no_symlinks, sha256, write_new_bytes};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const FEEDBACK_SCHEMA: &str = "ontology-engineering.semantic-feedback/v1";
pub const MAX_REVIEWS: usize = 16;
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const DEFAULT_BUNDLE: &str = "engineering-judgment-intake";
const REVIEW_CONTRACT: &str = "semantica.ontology.decision-review/v1";
const REFINERY_CONTRACT: &str = "semantica.ontology.refinery/v1";

/// Prepared file identities. The production executor reads these current files,
/// not an imported PASS/status or saved semantic receipt.
#[derive(Debug, Clone)]
pub struct NativeReviewInput {
    pub binding: PathBuf,
    pub workspace: PathBuf,
    pub task: PathBuf,
    pub evidence_file: PathBuf,
    pub source_id: String,
    pub scope_id: String,
    pub focus_iri: String,
    pub focus_type_iri: String,
    pub query_asset_id: String,
    pub shape_asset_id: String,
}

/// Dependency injection is explicit: fixtures are supplied by trusted code,
/// never selected by a request, binding or data file.
pub trait NativeReviewExecutor {
    fn review(&self, root: &Path, input: &NativeReviewInput) -> Result<Value>;
}

pub struct SourceLockedExecutor;

impl NativeReviewExecutor for SourceLockedExecutor {
    fn review(&self, root: &Path, input: &NativeReviewInput) -> Result<Value> {
        let binding_raw = read(&input.binding)?;
        let binding = strict_json(&binding_raw)?;
        let native_binding = validate_binding(&binding, None, None)?;
        let task_raw = read(&input.task)?;
        let task = strict_json(&task_raw)?;
        validate_task(&task, &binding, input)?;
        let evidence = read(&input.evidence_file)?;
        ensure!(
            sha256(&evidence) == task["evidence"][0]["sha256"],
            "review_evidence_hash_mismatch"
        );
        let destination = input.task.parent().context("review_output_missing")?;
        let python = if cfg!(windows) {
            "runtime/.venv/Scripts/python.exe"
        } else {
            "runtime/.venv/bin/python"
        };
        let runtime = match crate::native::NativeRuntime::new(root, &root.join(python)) {
            Ok(runtime) => runtime,
            Err(error) => {
                write_json(
                    &destination.join("native-execution.json"),
                    &json!({
                        "operation":"decision_review", "semantic_execution":"not_run",
                        "error":"native_runtime_unavailable", "fact_admitted":false
                    }),
                )?;
                return Err(error).context("native_runtime_unavailable");
            }
        };
        let request = crate::native::DecisionReviewRequest {
            workspace: input.workspace.clone(),
            binding: native_binding,
            evidence_sha256: sha256(&evidence),
            evidence,
            evidence_format: "turtle".into(),
            evidence_id: input.source_id.clone(),
            evidence_uri: text(&task["evidence"][0]["uri"])?.into(),
            evidence_captured_at: text(&task["evidence"][0]["captured_at"])?.into(),
            scope_id: input.scope_id.clone(),
            focus_iri: input.focus_iri.clone(),
            focus_type_iri: input.focus_type_iri.clone(),
            query_asset_id: input.query_asset_id.clone(),
            shape_asset_id: input.shape_asset_id.clone(),
            created_at: None,
        };
        let execution = runtime.decision_review(&request);
        let execution_value = serde_json::to_value(&execution)?;
        write_json(&destination.join("native-execution.json"), &execution_value)?;
        let native = execution.into_result()?;
        ensure!(
            read(&input.binding)? == binding_raw && read(&input.task)? == task_raw,
            "review_execution_input_changed"
        );
        Ok(json!({
            "$schema":"ontology-engineering.semantic-engagement-response/v1", "command":"review",
            "execution":{"status":if native["review"]["status"] == "clear" {"passed"} else {"blocked"},
                "operation":"native_decision_review", "review":native["review"], "task_sha256":sha256(&task_raw)},
            "corpus_found":{"status":"found", "subject":native["subject"], "baseline_subject_verified":true},
            "native_execution":execution_value, "fact_admitted":false
        }))
    }
}

fn text(value: &Value) -> Result<&str> {
    let value = nonempty(value)?;
    ensure!(!value.chars().any(|c| c < '\u{20}'), "review_text_invalid");
    Ok(value)
}

fn nonempty(value: &Value) -> Result<&str> {
    let value = value.as_str().context("review_text_required")?;
    ensure!(!value.trim().is_empty(), "review_text_invalid");
    Ok(value)
}

fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<()> {
    let value = value.as_object().context("review_object_required")?;
    ensure!(
        required.iter().all(|key| value.contains_key(*key))
            && value
                .keys()
                .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str())),
        "review_fields_invalid"
    );
    Ok(())
}

fn digest(value: &Value) -> Result<&str> {
    let value = value.as_str().context("review_sha256_required")?;
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "review_sha256_invalid"
    );
    Ok(value)
}

fn read(path: &Path) -> Result<Vec<u8>> {
    oe_core::read_regular_bytes(path, MAX_INPUT_BYTES)
}

fn bound(path: &Path, expected: &str) -> Result<Vec<u8>> {
    digest(&Value::String(expected.into()))?;
    let raw = read(path)?;
    ensure!(sha256(&raw) == expected, "review_input_hash_mismatch");
    Ok(raw)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = canonical_json(value)?;
    bytes.push(b'\n');
    write_new_bytes(path, &bytes)
}

fn private(root: &SkillRoot, value: &Value, directory: bool) -> Result<PathBuf> {
    let path = root.private_path(Path::new(text(value)?))?;
    let metadata = fs::symlink_metadata(&path).context("review_input_missing")?;
    ensure!(
        if directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        },
        "review_input_type_invalid"
    );
    Ok(path)
}

fn relative(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)
        .context("review_path_outside_root")?
        .to_str()
        .context("review_path_not_utf8")?
        .replace('\\', "/"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DirectoryIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}
fn directory_identity(path: &Path) -> Result<DirectoryIdentity> {
    no_symlinks(path)?;
    let metadata = fs::symlink_metadata(path).context("review_directory_missing")?;
    ensure!(metadata.is_dir(), "review_directory_invalid");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(DirectoryIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    Ok(DirectoryIdentity {})
}

/// RFC 6901 pointer, including strict array indices and escape validation.
pub fn pointer_value<'a>(document: &'a Value, pointer: &str) -> Result<&'a Value> {
    ensure!(
        pointer.is_empty() || pointer.starts_with('/'),
        "review_pointer_invalid"
    );
    let mut value = document;
    for raw in pointer.split('/').skip(1) {
        let mut chars = raw.chars();
        let mut part = String::new();
        while let Some(ch) = chars.next() {
            if ch == '~' {
                part.push(match chars.next() {
                    Some('0') => '~',
                    Some('1') => '/',
                    _ => bail!("review_pointer_escape_invalid"),
                });
            } else {
                part.push(ch);
            }
        }
        value = if let Some(array) = value.as_array() {
            ensure!(
                part == "0"
                    || (!part.starts_with('0')
                        && !part.is_empty()
                        && part.bytes().all(|b| b.is_ascii_digit())),
                "review_array_pointer_invalid"
            );
            array
                .get(
                    part.parse::<usize>()
                        .context("review_array_pointer_invalid")?,
                )
                .context("review_pointer_missing")?
        } else {
            value
                .as_object()
                .and_then(|map| map.get(&part))
                .context("review_pointer_missing")?
        };
    }
    Ok(value)
}

fn percent_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").expect("string write");
        }
    }
    encoded
}

fn literal(value: &Value) -> Result<String> {
    Ok(String::from_utf8(canonical_json(value)?)?)
}

#[derive(Debug, Clone)]
struct Source {
    id: String,
    path: PathBuf,
    sha256: String,
}

#[derive(Debug, Clone)]
pub struct MethodProjection {
    pub rdf: Vec<u8>,
    pub audit: Value,
    pub record: Value,
    pub resolved_facts: Value,
    sources: Vec<Source>,
}

/// Source-byte and pointer projection only. Fields and types come from the
/// selected locked package, and are never interpreted as engineering truth.
pub fn project_record(
    root: &Path,
    record_path: &Path,
    evidence_root: &Path,
    bundle_name: &str,
) -> Result<MethodProjection> {
    let root = SkillRoot::new(root)?;
    let record_path = private(&root, &json!(record_path), false)?;
    let evidence_root = private(&root, &json!(evidence_root), true)?;
    let raw = read(&record_path)?;
    let bundle = crate::load_bundle(root.path(), bundle_name)?;
    project(&root, &raw, &evidence_root, &bundle)
}

fn project(
    root: &SkillRoot,
    raw: &[u8],
    evidence_root: &Path,
    bundle: &crate::LoadedBundle,
) -> Result<MethodProjection> {
    let record = strict_json(raw)?;
    keys(
        &record,
        &["schema", "record_id", "claim", "method", "sources", "facts"],
        &[],
    )?;
    ensure!(
        record["schema"] == "ontology-engineering.method-evidence/v1",
        "method_evidence_schema_invalid"
    );
    nonempty(&record["record_id"])?;
    keys(
        &record["claim"],
        &["id", "statement", "subject_revision", "scope"],
        &[],
    )?;
    for value in record["claim"]
        .as_object()
        .expect("validated object")
        .values()
    {
        nonempty(value)?;
    }
    let asset = bundle.manifest["assets"]
        .as_array()
        .context("method_profiles_missing")?
        .iter()
        .find(|asset| asset["asset_id"] == "input-profiles")
        .context("method_profiles_missing")?;
    let registry = strict_json(
        bundle
            .payload
            .get(text(&asset["path"])?)
            .context("method_profiles_missing")?,
    )?;
    let method = text(&record["method"])?;
    let profile = registry["profiles"]
        .get(method)
        .context("method_not_in_locked_package")?;
    let fields = profile["fields"]
        .as_object()
        .context("method_fields_invalid")?;
    let raw_sources = record["sources"]
        .as_array()
        .context("method_sources_required")?;
    ensure!(!raw_sources.is_empty(), "method_sources_required");
    let mut documents = BTreeMap::new();
    let mut sources = Vec::new();
    for source in raw_sources {
        keys(source, &["id", "path", "sha256", "media_type"], &[])?;
        let id = nonempty(&source["id"])?;
        ensure!(!documents.contains_key(id), "duplicate_method_source");
        ensure!(
            source["media_type"] == "application/json",
            "method_source_media_type_invalid"
        );
        let path = evidence_root.join(crate::safe_relative(text(&source["path"])?)?);
        let path = private(root, &json!(path), false)?;
        let sha = digest(&source["sha256"])?;
        let bytes = bound(&path, sha)?;
        documents.insert(id.to_owned(), (strict_json(&bytes)?, sha.to_owned()));
        sources.push(Source {
            id: id.into(),
            path,
            sha256: sha.into(),
        });
    }
    let facts = record["facts"]
        .as_object()
        .context("method_facts_invalid")?;
    ensure!(
        facts.keys().all(|field| fields.contains_key(field)),
        "method_fact_unknown"
    );
    let mut resolved = Map::new();
    let mut lineage = Map::new();
    for (field, reference) in facts {
        keys(reference, &["source_id", "pointer"], &[])?;
        let source_id = nonempty(&reference["source_id"])?;
        let (document, source_sha) = documents
            .get(source_id)
            .context("method_fact_source_unknown")?;
        let pointer = reference["pointer"]
            .as_str()
            .context("review_pointer_invalid")?;
        let value = pointer_value(document, pointer)?;
        let valid = match fields[field].as_str() {
            Some("string") => value.as_str().is_some_and(|value| !value.trim().is_empty()),
            Some("boolean") => value.is_boolean(),
            Some("number") => value.is_number() && value.as_f64().is_some_and(f64::is_finite),
            Some("strings") => value.as_array().is_some_and(|values| {
                let strings = values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>();
                strings.len() == values.len()
                    && strings.iter().all(|value| !value.trim().is_empty())
            }),
            _ => false,
        };
        ensure!(valid, "method_fact_type_mismatch");
        resolved.insert(field.clone(), value.clone());
        lineage.insert(
            field.clone(),
            json!({"source_id":source_id,"pointer":pointer,"source_sha256":source_sha}),
        );
    }
    let namespace = text(&registry["namespace"])?;
    let focus = format!(
        "{namespace}claim/{}",
        percent_component(nonempty(&record["claim"]["id"])?)
    );
    let mut rows = vec![
        format!("<{focus}> a <{}> .", text(&registry["focus_type"])?),
        format!(
            "<{focus}> <{namespace}method> {} .",
            literal(&record["method"])?
        ),
        format!(
            "<{focus}> <{namespace}subjectRevision> {} .",
            literal(&record["claim"]["subject_revision"])?
        ),
        format!(
            "<{focus}> <{namespace}scope> {} .",
            literal(&record["claim"]["scope"])?
        ),
        format!(
            "<{focus}> <https://product-trustworthiness.local/claim#claimStatement> {} .",
            literal(&record["claim"]["statement"])?
        ),
    ];
    for field in resolved.keys().collect::<BTreeSet<_>>() {
        let value = &resolved[field];
        let values = value
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_else(|| std::slice::from_ref(value));
        for item in values {
            rows.push(format!(
                "<{focus}> <{namespace}{field}> {} .",
                literal(item)?
            ));
        }
        if value.is_array() {
            rows.push(format!("<{focus}> <{namespace}{field}Complete> true ."));
        }
        let origin = &lineage[field];
        let eid = format!(
            "{namespace}source/{}/{}",
            text(&origin["source_sha256"])?,
            percent_component(field)
        );
        rows.push(format!("<{focus}> <{namespace}fieldEvidence> <{eid}> ."));
        rows.push(format!("<{eid}> a <{namespace}MethodEvidence> ; <{namespace}fieldName> {} ; <{namespace}sourceSha256> {} ; <{namespace}sourcePointer> {} .", literal(&json!(field))?, literal(&origin["source_sha256"])?, literal(&origin["pointer"])?));
    }
    let rdf = (rows.join("\n") + "\n").into_bytes();
    let spec = &bundle.spec;
    let audit = json!({"schema":"ontology-engineering.method-projection/v1", "record_sha256":sha256(raw),
        "projection_sha256":sha256(&rdf), "source_integrity":"verified", "field_lineage":lineage,
        "focus":focus, "focus_type":registry["focus_type"], "scope":record["claim"]["scope"], "method":method,
        "query_asset":format!("findings-{method}"), "shape_asset":format!("shape-{method}"),
        "package":{"package_id":spec["package_id"],"package_version":spec["package_version"],"package_sha256":spec["package_sha256"],"sha256":spec["sha256"]},
        "semantic_execution":"not_run", "engineering_verdict":"not_assessed",
        "meaning":"Source bytes and pointers match. Field meaning and truth remain evidence assertions for Semantica and engineering review."});
    Ok(MethodProjection {
        rdf,
        audit,
        record,
        resolved_facts: Value::Object(resolved),
        sources,
    })
}

fn forbid_selection(value: &Value) -> Result<()> {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            let normalized = key.to_lowercase().replace('-', "_");
            ensure!(
                !normalized.contains("backend") && !normalized.contains("fallback"),
                "review_runtime_selection_forbidden"
            );
            forbid_selection(value)?;
        }
    } else if let Some(values) = value.as_array() {
        for value in values {
            forbid_selection(value)?;
        }
    }
    Ok(())
}

fn unique_texts(value: &Value, nonempty: bool) -> Result<Vec<&str>> {
    let values = value.as_array().context("review_list_invalid")?;
    let texts = values.iter().map(text).collect::<Result<Vec<_>>>()?;
    ensure!(
        (!nonempty || !texts.is_empty())
            && texts.iter().collect::<BTreeSet<_>>().len() == texts.len(),
        "review_list_invalid"
    );
    Ok(texts)
}

fn authority(value: &Value) -> Result<Value> {
    keys(value, &["authority_id", "scope"], &[])?;
    let id = text(&value["authority_id"])?.trim();
    let mut scopes = unique_texts(&value["scope"], true)?
        .iter()
        .map(|value| value.trim())
        .collect::<Vec<_>>();
    ensure!(
        scopes.iter().collect::<BTreeSet<_>>().len() == scopes.len(),
        "review_authority_scope_invalid"
    );
    scopes.sort();
    Ok(json!({"authority_id":id,"scope":scopes}))
}

/// The migration deliberately accepts timezone-explicit RFC 3339 timestamps.
/// Python's additional ISO week/basic/date-separator spellings are rejected.
fn timestamp(value: &Value) -> Result<&str> {
    let timestamp = text(value)?.trim();
    DateTime::parse_from_rfc3339(timestamp).context("review_timestamp_requires_rfc3339")?;
    Ok(timestamp)
}

/// Validate the existing OE workspace authorization and construct the exact
/// native binding DTO, without granting extra lifecycle actions.
pub fn validate_binding(
    binding: &Value,
    project_id: Option<&str>,
    spec: Option<&Value>,
) -> Result<Value> {
    forbid_selection(binding)?;
    keys(
        binding,
        &[
            "$schema",
            "binding_id",
            "project",
            "semantic_target",
            "baseline",
            "evidence",
            "authority",
            "allowed_actions",
            "lifecycle_actions",
            "promotion",
            "created_at",
            "semantic_api",
        ],
        &["predecessor"],
    )?;
    ensure!(
        binding["$schema"] == "ontology-engineering.semantic-project-binding/v1",
        "review_binding_schema_invalid"
    );
    keys(&binding["project"], &["project_id", "domain"], &[])?;
    let project = text(&binding["project"]["project_id"])?.trim();
    ensure!(
        project_id.is_none_or(|expected| expected == project),
        "review_project_mismatch"
    );
    keys(
        &binding["semantic_target"],
        &["kind", "workspace_id", "package_id"],
        &[],
    )?;
    ensure!(
        binding["semantic_target"]["kind"] == "workspace",
        "review_requires_workspace"
    );
    let package_id = text(&binding["semantic_target"]["package_id"])?.trim();
    ensure!(
        package_id.as_bytes()[0].is_ascii_alphanumeric()
            && package_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
        "review_package_id_invalid"
    );
    keys(&binding["baseline"], &["version", "digest"], &[])?;
    text(&binding["baseline"]["version"])?;
    digest(&binding["baseline"]["digest"])?;
    if let Some(spec) = spec {
        ensure!(
            binding["semantic_target"]["package_id"] == spec["package_id"]
                && binding["baseline"]
                    == json!({"version":spec["package_version"],"digest":spec["package_sha256"]}),
            "review_bundle_binding_mismatch"
        );
    }
    keys(&binding["evidence"], &["logical_root"], &[])?;
    let logical = text(&binding["evidence"]["logical_root"])?.trim();
    let (scheme, suffix) = logical
        .split_once(':')
        .context("review_evidence_logical_root_invalid")?;
    ensure!(
        !scheme.is_empty()
            && scheme.as_bytes()[0].is_ascii_lowercase()
            && scheme
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b))
            && !suffix.is_empty()
            && !logical.chars().any(char::is_whitespace)
            && !["file", "http", "https"].contains(&scheme),
        "review_evidence_logical_root_invalid"
    );
    keys(&binding["authority"], &["fact", "decision"], &[])?;
    let fact = authority(&binding["authority"]["fact"])?;
    let decision = authority(&binding["authority"]["decision"])?;
    let actions = unique_texts(&binding["allowed_actions"], true)?;
    let known = [
        "doctor", "discover", "open", "run", "review", "propose", "commit", "verify", "history",
        "promote",
    ];
    ensure!(
        actions.iter().all(|action| known.contains(action)) && actions.contains(&"review"),
        "review_action_not_allowed"
    );
    let lifecycle = unique_texts(&binding["lifecycle_actions"], true)?;
    let states = [
        "candidate",
        "proposed",
        "committed",
        "regression_passed",
        "release_complete",
        "promoted",
    ];
    ensure!(
        lifecycle.len() <= states.len() && lifecycle == states[..lifecycle.len()],
        "review_lifecycle_prefix_invalid"
    );
    let commands = [
        "propose", "propose", "commit", "verify", "verify", "promote",
    ];
    ensure!(
        commands[..lifecycle.len()]
            .iter()
            .all(|command| actions.contains(command)),
        "review_lifecycle_unauthorized"
    );
    keys(
        &binding["promotion"],
        &["target", "requires_decision_authority"],
        &[],
    )?;
    ensure!(
        binding["promotion"]["target"] == "industry-registry"
            && binding["promotion"]["requires_decision_authority"] == true,
        "review_promotion_authority_invalid"
    );
    ensure!(
        binding["semantic_api"] == REFINERY_CONTRACT,
        "review_semantic_contract_invalid"
    );
    if let Some(previous) = binding.get("predecessor") {
        keys(
            previous,
            &["binding_sha256", "promotion_record_sha256"],
            &[],
        )?;
        digest(&previous["binding_sha256"])?;
        digest(&previous["promotion_record_sha256"])?;
    }
    Ok(
        json!({"schema_version":"1.0", "binding_id":text(&binding["binding_id"])?.trim(), "project_id":project,
        "domain":text(&binding["project"]["domain"])?.trim(), "package_id":package_id,
        "workspace_id":text(&binding["semantic_target"]["workspace_id"])?.trim(),
        "baseline_version":text(&binding["baseline"]["version"])?.trim(), "baseline_package_sha256":binding["baseline"]["digest"],
        "evidence_root":logical, "fact_authorities":[literal(&fact)?], "decision_authorities":[literal(&decision)?],
        "allowed_actions":lifecycle, "semantic_api_contract":REFINERY_CONTRACT,
        "promotion_target":"industry-registry", "created_at":timestamp(&binding["created_at"])?}),
    )
}

fn validate_task(task: &Value, binding: &Value, input: &NativeReviewInput) -> Result<()> {
    keys(
        task,
        &[
            "$schema",
            "task_id",
            "task_kind",
            "intent",
            "project",
            "domain",
            "requested_decision",
            "evidence",
            "requested_actions",
            "actor_id",
            "required_capabilities",
            "created_at",
        ],
        &[],
    )?;
    ensure!(
        task["$schema"] == "ontology-engineering.semantic-task-envelope/v1"
            && task["project"] == binding["project"]["project_id"]
            && task["domain"] == binding["project"]["domain"]
            && task["requested_actions"] == json!(["review"])
            && task["required_capabilities"] == json!([REVIEW_CONTRACT]),
        "review_task_identity_mismatch"
    );
    for key in [
        "task_id",
        "task_kind",
        "intent",
        "requested_decision",
        "actor_id",
    ] {
        text(&task[key])?;
    }
    timestamp(&task["created_at"])?;
    ensure!(
        task["evidence"]
            .as_array()
            .is_some_and(|items| items.len() == 1),
        "review_task_evidence_invalid"
    );
    let evidence = &task["evidence"][0];
    keys(
        evidence,
        &["source_id", "uri", "sha256", "media_type", "captured_at"],
        &[],
    )?;
    let expected_uri = format!(
        "{}/{}",
        text(&binding["evidence"]["logical_root"])?.trim_end_matches('/'),
        input.source_id
    );
    ensure!(
        evidence["source_id"] == input.source_id
            && evidence["uri"] == expected_uri
            && evidence["media_type"] == "text/turtle",
        "review_task_evidence_invalid"
    );
    timestamp(&evidence["captured_at"])?;
    digest(&evidence["sha256"])?;
    Ok(())
}

struct Prepared {
    id: String,
    bundle: String,
    package: Value,
    request_sha256: String,
    actor: String,
    record_path: PathBuf,
    binding_path: PathBuf,
    evidence_root: PathBuf,
    workspace: PathBuf,
    evidence_identity: DirectoryIdentity,
    workspace_identity: DirectoryIdentity,
    record_sha256: String,
    binding_sha256: String,
    binding: Value,
    projection: MethodProjection,
    asset_sha256: BTreeMap<String, String>,
}

impl Prepared {
    fn recheck(&self, root: &Path) -> Result<()> {
        bound(&self.record_path, &self.record_sha256)?;
        bound(&self.binding_path, &self.binding_sha256)?;
        for source in &self.projection.sources {
            bound(&source.path, &source.sha256)?;
        }
        ensure!(
            directory_identity(&self.evidence_root)? == self.evidence_identity
                && directory_identity(&self.workspace)? == self.workspace_identity,
            "review_directory_identity_changed"
        );
        ensure!(
            crate::identity(&crate::load_bundle(root, &self.bundle)?.spec)? == self.package,
            "review_package_identity_changed"
        );
        Ok(())
    }
}

/// Execute at most 16 freshly prepared method records against their promoted
/// native registry. An empty request is not_run and performs no filesystem I/O.
pub fn execute_reviews(
    root: &Path,
    requests: &Value,
    project_id: &str,
    output: &Path,
) -> Result<Value> {
    execute_reviews_with(root, requests, project_id, output, &SourceLockedExecutor)
}

pub fn execute_reviews_with(
    root: &Path,
    requests: &Value,
    project_id: &str,
    output: &Path,
    executor: &dyn NativeReviewExecutor,
) -> Result<Value> {
    text(&json!(project_id))?;
    let requests = requests.as_array().context("review_requests_invalid")?;
    ensure!(requests.len() <= MAX_REVIEWS, "review_budget_exceeded");
    let mut report = json!({"schema":FEEDBACK_SCHEMA,"project_id":project_id,"status":"not_run","reviews":[],"fact_admitted":false,
        "meaning":"Fresh scoped native record checks only; no physical acceptance, fact admission, project promotion, or future applicability follows."});
    if requests.is_empty() {
        report["feedback_sha256"] = sha256(canonical_json(&report)?).into();
        return Ok(report);
    }
    let root = SkillRoot::new(root)?;
    let output = root.private_path(output)?;
    ensure!(!output.try_exists()?, "review_output_exists");
    let mut prepared = Vec::new();
    let mut ids = BTreeSet::new();
    let mut bundles = BTreeMap::new();
    for request in requests {
        keys(
            request,
            &[
                "id",
                "record",
                "record_sha256",
                "evidence_root",
                "binding",
                "binding_sha256",
                "workspace",
                "actor",
            ],
            &["bundle"],
        )?;
        let id = text(&request["id"])?;
        ensure!(
            id.len() <= 80
                && id.as_bytes()[0].is_ascii_alphanumeric()
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                && ids.insert(id.to_owned()),
            "review_id_invalid_or_duplicate"
        );
        let actor = text(&request["actor"])?;
        let name = request
            .get("bundle")
            .map(text)
            .transpose()?
            .unwrap_or(DEFAULT_BUNDLE);
        let record_path = private(&root, &request["record"], false)?;
        let binding_path = private(&root, &request["binding"], false)?;
        let evidence_root = private(&root, &request["evidence_root"], true)?;
        let workspace = private(&root, &request["workspace"], true)?;
        for path in [&record_path, &binding_path, &evidence_root, &workspace] {
            ensure!(
                !path.starts_with(&output) && !output.starts_with(path),
                "review_output_overlaps_input"
            );
        }
        let record_sha = digest(&request["record_sha256"])?;
        let binding_sha = digest(&request["binding_sha256"])?;
        let raw = bound(&record_path, record_sha)?;
        let binding = strict_json(&bound(&binding_path, binding_sha)?)?;
        if !bundles.contains_key(name) {
            bundles.insert(name.to_owned(), crate::load_bundle(root.path(), name)?);
        }
        let bundle = &bundles[name];
        validate_binding(&binding, Some(project_id), Some(&bundle.spec))?;
        let projection = project(&root, &raw, &evidence_root, bundle)?;
        let mut asset_sha256 = BTreeMap::new();
        for key in ["query_asset", "shape_asset"] {
            let asset_id = text(&projection.audit[key])?;
            let asset = bundle.manifest["assets"]
                .as_array()
                .context("review_assets_missing")?
                .iter()
                .find(|asset| asset["asset_id"] == asset_id)
                .context("review_asset_missing")?;
            asset_sha256.insert(asset_id.to_owned(), digest(&asset["sha256"])?.to_owned());
        }
        prepared.push(Prepared {
            id: id.into(),
            bundle: name.into(),
            package: crate::identity(&bundle.spec)?,
            request_sha256: sha256(canonical_json(request)?),
            actor: actor.into(),
            evidence_identity: directory_identity(&evidence_root)?,
            workspace_identity: directory_identity(&workspace)?,
            record_path,
            binding_path,
            evidence_root,
            workspace,
            record_sha256: record_sha.into(),
            binding_sha256: binding_sha.into(),
            binding,
            projection,
            asset_sha256,
        });
    }
    root.new_private_dir(&output)?;
    report["failed_reviews"] = json!([]);
    for item in prepared {
        let destination = output.join(format!("review-{}", item.id));
        let mut phase = "prepare_execution";
        let mut executor_called = false;
        let outcome: Result<Value> = (|| {
            item.recheck(root.path())?;
            root.new_private_dir(&destination)?;
            let audit = &item.projection.audit;
            write_new_bytes(&destination.join("projection.ttl"), &item.projection.rdf)?;
            write_json(&destination.join("projection.json"), audit)?;
            let captured = DateTime::<Utc>::from(std::time::SystemTime::now()).to_rfc3339();
            let source_id = format!("projection-{}", item.record_sha256);
            let task = json!({"$schema":"ontology-engineering.semantic-task-envelope/v1", "task_id":format!("method-review-{}",item.record_sha256),
            "task_kind":"source-bound-method-review", "intent":"Review one declared evidence obligation", "project":project_id,
            "domain":item.binding["project"]["domain"], "requested_decision":"Scoped record consistency only; no physical acceptance",
            "actor_id":item.actor, "requested_actions":["review"], "required_capabilities":[REVIEW_CONTRACT],
            "evidence":[{"source_id":source_id,"uri":format!("{}/{}",text(&item.binding["evidence"]["logical_root"])?.trim_end_matches('/'),source_id),
                "sha256":sha256(&item.projection.rdf),"media_type":"text/turtle","captured_at":captured}], "created_at":captured});
            write_json(&destination.join("task.json"), &task)?;
            let input = NativeReviewInput {
                binding: item.binding_path.clone(),
                workspace: item.workspace.clone(),
                task: destination.join("task.json"),
                evidence_file: destination.join("projection.ttl"),
                source_id,
                scope_id: text(&audit["scope"])?.into(),
                focus_iri: text(&audit["focus"])?.into(),
                focus_type_iri: text(&audit["focus_type"])?.into(),
                query_asset_id: text(&audit["query_asset"])?.into(),
                shape_asset_id: text(&audit["shape_asset"])?.into(),
            };
            let task_sha = sha256(read(&input.task)?);
            let audit_sha = sha256(read(&destination.join("projection.json"))?);
            phase = "native_execution";
            executor_called = true;
            let execution = executor.review(root.path(), &input);
            // Retain even an invalid returned response, without presenting it as a validated review.
            if let Ok(raw) = &execution {
                write_json(&destination.join("returned-review.json"), raw)?;
            }
            phase = "post_execution_identity";
            item.recheck(root.path())?;
            ensure!(
                sha256(read(&input.task)?) == task_sha
                    && sha256(read(&input.evidence_file)?) == audit["projection_sha256"]
                    && sha256(read(&destination.join("projection.json"))?) == audit_sha,
                "review_generated_artifact_changed"
            );
            phase = "native_execution";
            let result = execution?;
            phase = "native_validation";
            let native = result
                .get("execution")
                .and_then(|value| value.get("review"))
                .context("fresh_native_review_missing")?;
            validate_native(native, &item, &task)?;
            phase = "record_validated_review";
            write_json(&destination.join("review.json"), &result)?;
            Ok(native.clone())
        })();
        let mut artifacts = Map::new();
        let mut unavailable_artifacts = Vec::new();
        for name in [
            "projection.ttl",
            "projection.json",
            "task.json",
            "returned-review.json",
            "review.json",
            "native-execution.json",
        ] {
            let path = destination.join(name);
            let artifact: Result<Option<Value>> = (|| {
                if !path.try_exists()? {
                    return Ok(None);
                }
                Ok(Some(
                    json!({"path":relative(root.path(), &path)?,"sha256":sha256(read(&path)?)}),
                ))
            })();
            match artifact {
                Ok(Some(value)) => {
                    artifacts.insert(name.into(), value);
                }
                Ok(None) => {}
                Err(_) => unavailable_artifacts.push(name),
            }
        }
        let sources = item.projection.sources.iter().map(|source| Ok(json!({"id":source.id,"path":relative(root.path(), &source.path)?,"sha256":source.sha256}))).collect::<Result<Vec<_>>>()?;
        let mut entry = json!({"id":item.id,"bundle":item.bundle,"package":item.package,"actor":item.actor,
            "request_sha256":item.request_sha256,"inputs":{"record":{"path":relative(root.path(),&item.record_path)?,"sha256":item.record_sha256},
                "binding":{"path":relative(root.path(),&item.binding_path)?,"sha256":item.binding_sha256},
                "workspace":relative(root.path(),&item.workspace)?,"evidence_root":relative(root.path(),&item.evidence_root)?,"sources":sources},
            "claim":item.projection.record["claim"],"method":item.projection.audit["method"],"artifacts":artifacts,"fact_admitted":false});
        // A failed artifact read cannot be promoted to a completed review either.
        let outcome = outcome.and_then(|native| {
            ensure!(
                unavailable_artifacts.is_empty(),
                "review_artifact_unavailable"
            );
            Ok(native)
        });
        match outcome {
            Ok(native) => {
                entry["review"] = native;
                entry["execution_origin"] = "fresh_native_review".into();
                report["reviews"]
                    .as_array_mut()
                    .expect("initialized array")
                    .push(entry);
            }
            Err(error) => {
                entry["status"] = "blocked".into();
                entry["error_type"] = "review_item_failed".into();
                entry["reason"] = error.to_string().into();
                entry["failure_phase"] = phase.into();
                entry["executor_called"] = executor_called.into();
                entry["unavailable_artifacts"] = json!(unavailable_artifacts);
                entry["dependent_adoption"] = "blocked".into();
                report["failed_reviews"]
                    .as_array_mut()
                    .expect("initialized array")
                    .push(entry);
            }
        }
    }
    let failures = report["failed_reviews"]
        .as_array()
        .expect("initialized array");
    report["status"] = if failures.is_empty() {
        "completed"
    } else if report["reviews"]
        .as_array()
        .expect("initialized array")
        .is_empty()
    {
        "blocked"
    } else {
        "partial"
    }
    .into();
    if report["status"] != "completed"
        || report["reviews"]
            .as_array()
            .expect("initialized array")
            .iter()
            .any(|r| r["review"]["status"] != "clear")
    {
        report["dependent_adoption"] = "blocked".into();
    }
    report["feedback_sha256"] = sha256(canonical_json(&report)?).into();
    write_json(&output.join("feedback.json"), &report)?;
    Ok(report)
}

fn validate_native(native: &Value, item: &Prepared, task: &Value) -> Result<()> {
    validate_native_review(
        native,
        &item.projection.audit,
        &item.package,
        &item.asset_sha256,
        task,
    )
}

/// Validate a fresh native review against its prepared projection and locked
/// package assets. Callers must derive these expectations from verified source
/// inputs, never from the returned report. This verifies the transport contract;
/// it does not execute or replace Semantica rules.
pub fn validate_native_review(
    native: &Value,
    audit: &Value,
    package_identity: &Value,
    asset_sha256: &BTreeMap<String, String>,
    task: &Value,
) -> Result<()> {
    ensure!(
        native["contract"] == REVIEW_CONTRACT
            && matches!(native["status"].as_str(), Some("clear" | "blocked")),
        "native_review_contract_invalid"
    );
    for (native_key, audit_key) in [
        ("scope_id", "scope"),
        ("focus_iri", "focus"),
        ("focus_type_iri", "focus_type"),
    ] {
        ensure!(
            native[native_key] == audit[audit_key],
            "actual_review_identity_mismatch"
        );
    }
    ensure!(
        native["evidence"]["sha256"] == audit["projection_sha256"]
            && native["evidence"]["id"] == task["evidence"][0]["source_id"]
            && native["evidence"]["uri"] == task["evidence"][0]["uri"]
            && native["evidence"]["captured_at"] == task["evidence"][0]["captured_at"]
            && native["evidence"]["format"] == "turtle",
        "actual_review_evidence_mismatch"
    );
    let package = &native["package"];
    ensure!(
        package["id"] == package_identity["package_id"]
            && package["version"] == package_identity["package_version"]
            && package["registry_package_sha256"] == package_identity["package_sha256"]
            && package["origin"] == "promoted_registry",
        "actual_review_package_mismatch"
    );
    for (key, audit_key) in [
        ("query_asset", "query_asset"),
        ("shape_asset", "shape_asset"),
    ] {
        let expected_id = text(&audit[audit_key])?;
        ensure!(
            package[format!("{key}_id")] == expected_id
                && package[format!("{key}_sha256")]
                    == *asset_sha256
                        .get(expected_id)
                        .context("prepared_review_asset_missing")?,
            "actual_review_asset_mismatch"
        );
    }
    ensure!(
        native["findings"].is_array() && native["violations"].is_array(),
        "native_review_findings_invalid"
    );
    // This is the external DTO's structural invariant, not a second domain
    // judgment. Never label a report clear when the report itself says otherwise.
    if native["status"] == "clear" {
        ensure!(
            native["focus_present"] == true
                && native["focus_type_present"] == true
                && native["focus_instance_count"] == 1
                && native["shape_conforms"] == true
                && native["evidence"]["input_quad_count"]
                    .as_u64()
                    .is_some_and(|count| count > 0)
                && native["findings"].as_array().is_some_and(Vec::is_empty)
                && native["violations"].as_array().is_some_and(Vec::is_empty),
            "native_clear_report_inconsistent"
        );
    }
    let expected_sha = digest(&native["report_sha256"])?;
    let mut content = native.clone();
    content
        .as_object_mut()
        .context("native_review_invalid")?
        .remove("report_sha256");
    ensure!(
        sha256(canonical_json(&content)?) == expected_sha,
        "native_review_report_hash_mismatch"
    );
    Ok(())
}
