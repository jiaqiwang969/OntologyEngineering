//! Owned control-plane contracts. Validation binds declared authority and exact
//! input bytes; it does not admit facts or replace native Semantica execution.
use anyhow::Result;
use chrono::DateTime;
use oe_core::json::{canonical_json, strict_json};
use oe_core::{read_regular_bytes, sha256};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

pub const BINDING_SCHEMA: &str = "ontology-engineering.semantic-project-binding/v1";
pub const TASK_SCHEMA: &str = "ontology-engineering.semantic-task-envelope/v1";
pub const AUTHORIZATION_SCHEMA: &str = "ontology-engineering.refinery-authorization/v1";
pub const RESPONSE_SCHEMA: &str = "ontology-engineering.semantic-engagement-response/v1";
pub const NATIVE_REFINERY_CONTRACT: &str = "semantica.ontology.refinery/v1";
pub const PROMOTION_TARGET: &str = "industry-registry";
pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
pub const KNOWN_ACTIONS: &[&str] = &[
    "doctor", "discover", "open", "run", "review", "propose", "commit", "verify", "history",
    "promote",
];
pub const STATE_MODEL: &[&str] = &[
    "candidate",
    "proposed",
    "committed",
    "regression_passed",
    "release_complete",
    "promoted",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractErrorKind {
    BindingValidationError,
    ActionNotAllowedError,
    BaselineMismatchError,
    CapabilityUnavailableError,
}
impl ContractErrorKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::BindingValidationError => "BindingValidationError",
            Self::ActionNotAllowedError => "ActionNotAllowedError",
            Self::BaselineMismatchError => "BaselineMismatchError",
            Self::CapabilityUnavailableError => "CapabilityUnavailableError",
        }
    }
}
#[derive(Debug, Clone)]
pub struct ContractError {
    pub kind: ContractErrorKind,
    pub message: String,
}
impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.name(), self.message)
    }
}
impl std::error::Error for ContractError {}
pub(crate) fn check(ok: bool, kind: ContractErrorKind, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(ContractError {
            kind,
            message: message.into(),
        }
        .into())
    }
}
pub(crate) fn valid(ok: bool, message: impl Into<String>) -> Result<()> {
    check(ok, ContractErrorKind::BindingValidationError, message)
}
pub(crate) fn invalid(message: impl Into<String>) -> anyhow::Error {
    ContractError {
        kind: ContractErrorKind::BindingValidationError,
        message: message.into(),
    }
    .into()
}
pub(crate) fn keys(
    value: &Value,
    required: &[&str],
    optional: &[&str],
    location: &str,
) -> Result<()> {
    let map = value
        .as_object()
        .ok_or_else(|| invalid(format!("{location} must be an object")))?;
    valid(
        required.iter().all(|k| map.contains_key(*k))
            && map
                .keys()
                .all(|k| required.contains(&k.as_str()) || optional.contains(&k.as_str())),
        format!("{location} contains missing or unsupported fields"),
    )
}
pub(crate) fn text(value: &Value, name: &str) -> Result<String> {
    let text = value
        .as_str()
        .ok_or_else(|| invalid(format!("{name} must be non-empty text")))?
        .trim();
    valid(!text.is_empty(), format!("{name} must be non-empty text"))?;
    Ok(text.into())
}
pub(crate) fn exact_text(value: &Value, name: &str) -> Result<String> {
    let result = text(value, name)?;
    valid(
        value.as_str() == Some(result.as_str()),
        format!("{name} must not contain surrounding whitespace"),
    )?;
    Ok(result)
}
pub(crate) fn hash(value: &Value, name: &str) -> Result<String> {
    let result = text(value, name)?;
    valid(
        result.len() == 64
            && result
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        format!("{name} must be a lowercase SHA-256 digest"),
    )?;
    Ok(result)
}
/// The Rust review contract already requires timezone-explicit RFC 3339.
/// ISO week/basic/date-separator aliases from Python are intentionally rejected.
pub(crate) fn timestamp(value: &Value, name: &str) -> Result<String> {
    let result = text(value, name)?;
    valid(
        DateTime::parse_from_rfc3339(&result).is_ok(),
        format!("{name} requires a timezone-explicit RFC3339 timestamp"),
    )?;
    Ok(result)
}
pub(crate) fn unique_texts(
    value: &Value,
    nonempty: bool,
    trim: bool,
    name: &str,
) -> Result<Vec<String>> {
    let array = value
        .as_array()
        .ok_or_else(|| invalid(format!("{name} must be a text list")))?;
    valid(
        !nonempty || !array.is_empty(),
        format!("{name} must be non-empty"),
    )?;
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for value in array {
        let normalized = text(value, name)?;
        let item = if trim {
            normalized
        } else {
            value.as_str().expect("checked string").into()
        };
        valid(seen.insert(item.clone()), format!("{name} must be unique"))?;
        result.push(item);
    }
    Ok(result)
}
pub(crate) fn forbid_selection(value: &Value) -> Result<()> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let normalized = key.to_lowercase().replace('-', "_");
                valid(
                    !normalized.contains("backend") && !normalized.contains("fallback"),
                    "forbidden runtime-selection field",
                )?;
                forbid_selection(child)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                forbid_selection(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) fn decode(raw: &[u8]) -> Result<Value> {
    valid(
        raw.len() <= MAX_DOCUMENT_BYTES,
        "control-plane document exceeds byte limit",
    )?;
    strict_json(raw).map_err(|_| invalid("invalid strict UTF-8 JSON"))
}
pub(crate) fn read(path: &Path) -> Result<Vec<u8>> {
    let path = if let Ok(tail) = path.strip_prefix("~") {
        PathBuf::from(std::env::var_os("HOME").ok_or_else(|| invalid("home unavailable"))?)
            .join(tail)
    } else {
        path.to_owned()
    };
    read_regular_bytes(&path, MAX_DOCUMENT_BYTES)
        .map_err(|_| invalid("input must be a bounded regular non-symbolic-link file"))
}
pub(crate) fn authority(value: &Value) -> Result<Value> {
    keys(value, &["authority_id", "scope"], &[], "authority")?;
    let id = text(&value["authority_id"], "authority_id")?;
    let mut scope = unique_texts(&value["scope"], true, true, "authority.scope")?;
    scope.sort();
    Ok(json!({"authority_id":id,"scope":scope}))
}
pub fn authority_token(value: &Value) -> Result<String> {
    Ok(String::from_utf8(canonical_json(&authority(value)?)?)?)
}

#[derive(Debug, Clone)]
pub struct ProjectBinding {
    pub binding_id: String,
    pub project_id: String,
    pub domain: String,
    pub target_kind: String,
    pub package_id: String,
    pub package_version: Option<String>,
    pub workspace_id: Option<String>,
    pub baseline_version: String,
    pub baseline_digest: String,
    pub evidence_logical_root: String,
    pub fact_authority: Value,
    pub decision_authority: Value,
    pub allowed_actions: Vec<String>,
    pub lifecycle_actions: Vec<String>,
    pub promotion_target: String,
    pub created_at: String,
    pub semantic_api: String,
    pub predecessor: Option<Value>,
    pub source_sha256: String,
}
impl ProjectBinding {
    pub fn as_dict(&self) -> Value {
        let mut target = json!({"kind":self.target_kind,"package_id":self.package_id});
        if let Some(value) = &self.package_version {
            target["package_version"] = json!(value);
        }
        if let Some(value) = &self.workspace_id {
            target["workspace_id"] = json!(value);
        }
        let mut result = json!({"binding_id":self.binding_id,"project":{"project_id":self.project_id,"domain":self.domain},
            "semantic_target":target,"baseline":{"version":self.baseline_version,"digest":self.baseline_digest},
            "evidence":{"logical_root":self.evidence_logical_root},"authority":{"fact":self.fact_authority,"decision":self.decision_authority},
            "allowed_actions":self.allowed_actions,"lifecycle_actions":self.lifecycle_actions,
            "promotion":{"target":self.promotion_target,"requires_decision_authority":true},"created_at":self.created_at,
            "semantic_api":self.semantic_api,"source_sha256":self.source_sha256});
        if let Some(value) = &self.predecessor {
            result["predecessor"] = value.clone();
        }
        result
    }
    pub fn document(&self) -> Value {
        let mut result = self.as_dict();
        result
            .as_object_mut()
            .unwrap()
            .shift_remove("source_sha256");
        result["$schema"] = json!(BINDING_SCHEMA);
        result
    }
}
pub fn read_project_binding(path: &Path) -> Result<ProjectBinding> {
    parse_project_binding(&read(path)?)
}
pub fn parse_project_binding(raw: &[u8]) -> Result<ProjectBinding> {
    let value = decode(raw)?;
    forbid_selection(&value)?;
    keys(
        &value,
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
        "binding",
    )?;
    valid(
        value["$schema"] == BINDING_SCHEMA,
        "binding schema unsupported",
    )?;
    keys(
        &value["project"],
        &["project_id", "domain"],
        &[],
        "binding.project",
    )?;
    let target = &value["semantic_target"];
    let kind = text(&target["kind"], "target.kind")?;
    let (package_version, workspace_id) = match kind.as_str() {
        "package" => {
            keys(
                target,
                &["kind", "package_id", "package_version"],
                &[],
                "target",
            )?;
            (
                Some(text(&target["package_version"], "package_version")?),
                None,
            )
        }
        "workspace" => {
            keys(
                target,
                &["kind", "package_id", "workspace_id"],
                &[],
                "target",
            )?;
            (None, Some(text(&target["workspace_id"], "workspace_id")?))
        }
        _ => return Err(invalid("target kind must be package or workspace")),
    };
    let package_id = text(&target["package_id"], "package_id")?;
    valid(
        package_id.as_bytes()[0].is_ascii_alphanumeric()
            && package_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
        "invalid package_id syntax",
    )?;
    keys(&value["baseline"], &["version", "digest"], &[], "baseline")?;
    let baseline_version = text(&value["baseline"]["version"], "baseline.version")?;
    let baseline_digest = hash(&value["baseline"]["digest"], "baseline.digest")?;
    valid(
        package_version
            .as_ref()
            .is_none_or(|version| version == &baseline_version),
        "package target version differs from baseline",
    )?;
    keys(
        &value["evidence"],
        &["logical_root"],
        &[],
        "binding.evidence",
    )?;
    let evidence_logical_root = text(&value["evidence"]["logical_root"], "logical_root")?;
    let (scheme, suffix) = evidence_logical_root
        .split_once(':')
        .ok_or_else(|| invalid("invalid logical evidence URI"))?;
    valid(
        !scheme.is_empty()
            && scheme.as_bytes()[0].is_ascii_lowercase()
            && scheme
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b))
            && !suffix.is_empty()
            && !evidence_logical_root.chars().any(char::is_whitespace)
            && !["file", "http", "https"].contains(&scheme),
        "evidence root must be a non-filesystem logical URI",
    )?;
    keys(
        &value["authority"],
        &["fact", "decision"],
        &[],
        "binding.authority",
    )?;
    let fact_authority = authority(&value["authority"]["fact"])?;
    let decision_authority = authority(&value["authority"]["decision"])?;
    let mut allowed_actions =
        unique_texts(&value["allowed_actions"], true, false, "allowed_actions")?;
    valid(
        allowed_actions
            .iter()
            .all(|action| KNOWN_ACTIONS.contains(&action.as_str())),
        "unknown allowed action",
    )?;
    allowed_actions.sort();
    let lifecycle_actions = unique_texts(
        &value["lifecycle_actions"],
        false,
        false,
        "lifecycle_actions",
    )?;
    valid(
        lifecycle_actions.len() <= STATE_MODEL.len()
            && lifecycle_actions
                .iter()
                .map(String::as_str)
                .eq(STATE_MODEL[..lifecycle_actions.len()].iter().copied()),
        "lifecycle must be ordered prefix",
    )?;
    if kind == "workspace" {
        valid(
            !lifecycle_actions.is_empty(),
            "workspace requires candidate lifecycle action",
        )?;
    } else {
        valid(
            lifecycle_actions.is_empty()
                && allowed_actions.iter().all(|action| {
                    ["doctor", "discover", "run", "verify"].contains(&action.as_str())
                }),
            "built-in package bindings are read-only",
        )?;
    }
    for (index, _) in lifecycle_actions.iter().enumerate() {
        valid(
            allowed_actions.iter().any(|action| {
                action
                    == [
                        "propose", "propose", "commit", "verify", "verify", "promote",
                    ][index]
            }),
            "lifecycle action lacks command authority",
        )?;
    }
    keys(
        &value["promotion"],
        &["target", "requires_decision_authority"],
        &[],
        "promotion",
    )?;
    valid(
        value["promotion"]["requires_decision_authority"] == true,
        "promotion must require decision authority",
    )?;
    let promotion_target = text(&value["promotion"]["target"], "promotion.target")?;
    valid(
        promotion_target == PROMOTION_TARGET,
        "unsupported promotion target",
    )?;
    let semantic_api = text(&value["semantic_api"], "semantic_api")?;
    valid(
        semantic_api == NATIVE_REFINERY_CONTRACT,
        "no compatibility or fallback lifecycle is selectable",
    )?;
    let predecessor = value.get("predecessor").map(|previous| -> Result<Value> {
        keys(previous, &["binding_sha256","promotion_record_sha256"], &[], "predecessor")?;
        Ok(json!({"binding_sha256":hash(&previous["binding_sha256"],"predecessor.binding_sha256")?,
            "promotion_record_sha256":hash(&previous["promotion_record_sha256"],"predecessor.promotion_record_sha256")?}))
    }).transpose()?;
    Ok(ProjectBinding {
        binding_id: text(&value["binding_id"], "binding_id")?,
        project_id: text(&value["project"]["project_id"], "project_id")?,
        domain: text(&value["project"]["domain"], "domain")?,
        target_kind: kind,
        package_id,
        package_version,
        workspace_id,
        baseline_version,
        baseline_digest,
        evidence_logical_root,
        fact_authority,
        decision_authority,
        allowed_actions,
        lifecycle_actions,
        promotion_target,
        created_at: timestamp(&value["created_at"], "binding.created_at")?,
        semantic_api,
        predecessor,
        source_sha256: sha256(raw),
    })
}

#[derive(Debug, Clone)]
pub struct SemanticTaskEnvelope {
    pub task_id: String,
    pub task_kind: String,
    pub intent: String,
    pub project: String,
    pub domain: String,
    pub requested_decision: String,
    pub evidence: Vec<Value>,
    pub requested_actions: Vec<String>,
    pub actor_id: String,
    pub required_capabilities: Vec<String>,
    pub created_at: String,
    pub source_sha256: String,
}
impl SemanticTaskEnvelope {
    pub fn as_dict(&self) -> Value {
        json!({"task_id":self.task_id,"task_kind":self.task_kind,"intent":self.intent,"project":self.project,
        "domain":self.domain,"requested_decision":self.requested_decision,"evidence":self.evidence,"requested_actions":self.requested_actions,
        "actor_id":self.actor_id,"required_capabilities":self.required_capabilities,"created_at":self.created_at,"source_sha256":self.source_sha256})
    }
    pub fn document(&self) -> Value {
        let mut result = self.as_dict();
        result
            .as_object_mut()
            .unwrap()
            .shift_remove("source_sha256");
        result["$schema"] = json!(TASK_SCHEMA);
        result
    }
}
pub(crate) fn evidence_ref(value: &Value, root: Option<&str>) -> Result<Value> {
    keys(
        value,
        &["source_id", "uri", "sha256", "media_type", "captured_at"],
        &[],
        "source evidence",
    )?;
    let uri = text(&value["uri"], "source.uri")?;
    if let Some(root) = root {
        let prefix = root.trim_end_matches('/');
        valid(
            uri == prefix || uri.starts_with(&format!("{prefix}/")),
            "source URI escapes logical evidence root",
        )?;
    }
    Ok(
        json!({"source_id":text(&value["source_id"],"source_id")?,"uri":uri,"sha256":hash(&value["sha256"],"source.sha256")?,
        "media_type":text(&value["media_type"],"media_type")?,"captured_at":timestamp(&value["captured_at"],"captured_at")?}),
    )
}
pub fn read_task_envelope(path: &Path, binding: &ProjectBinding) -> Result<SemanticTaskEnvelope> {
    parse_task_envelope(&read(path)?, binding)
}
pub fn parse_task_envelope(raw: &[u8], binding: &ProjectBinding) -> Result<SemanticTaskEnvelope> {
    let value = decode(raw)?;
    forbid_selection(&value)?;
    keys(
        &value,
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
        "task",
    )?;
    valid(value["$schema"] == TASK_SCHEMA, "unsupported task schema")?;
    let project = text(&value["project"], "task.project")?;
    let domain = text(&value["domain"], "task.domain")?;
    valid(
        project == binding.project_id && domain == binding.domain,
        "task project/domain differs from binding",
    )?;
    let raw_evidence = value["evidence"]
        .as_array()
        .ok_or_else(|| invalid("task evidence must be an array"))?;
    valid(
        !raw_evidence.is_empty(),
        "task requires source-bound evidence identities",
    )?;
    let mut evidence = Vec::new();
    let mut ids = BTreeSet::new();
    let mut uris = BTreeSet::new();
    for value in raw_evidence {
        let item = evidence_ref(value, Some(&binding.evidence_logical_root))?;
        valid(
            ids.insert(item["source_id"].as_str().unwrap().to_owned())
                && uris.insert(item["uri"].as_str().unwrap().to_owned()),
            "task source IDs and URIs must be unique",
        )?;
        evidence.push(item);
    }
    let mut requested_actions = unique_texts(
        &value["requested_actions"],
        true,
        false,
        "task.requested_actions",
    )?;
    valid(
        requested_actions
            .iter()
            .all(|action| KNOWN_ACTIONS.contains(&action.as_str())),
        "unknown task action",
    )?;
    check(
        requested_actions
            .iter()
            .all(|action| binding.allowed_actions.contains(action)),
        ContractErrorKind::ActionNotAllowedError,
        "task requests action outside binding",
    )?;
    requested_actions.sort();
    let mut required_capabilities = unique_texts(
        &value["required_capabilities"],
        true,
        false,
        "required_capabilities",
    )?;
    required_capabilities.sort();
    Ok(SemanticTaskEnvelope {
        task_id: text(&value["task_id"], "task_id")?,
        task_kind: text(&value["task_kind"], "task_kind")?,
        intent: text(&value["intent"], "intent")?,
        project,
        domain,
        requested_decision: text(&value["requested_decision"], "requested_decision")?,
        evidence,
        requested_actions,
        actor_id: text(&value["actor_id"], "actor_id")?,
        required_capabilities,
        created_at: timestamp(&value["created_at"], "task.created_at")?,
        source_sha256: sha256(raw),
    })
}
pub fn authorize(binding: &ProjectBinding, action: &str) -> Result<()> {
    check(
        KNOWN_ACTIONS.contains(&action)
            && binding.allowed_actions.iter().any(|value| value == action),
        ContractErrorKind::ActionNotAllowedError,
        "action is outside binding authority",
    )
}
pub fn authorize_task(
    binding: &ProjectBinding,
    task: &SemanticTaskEnvelope,
    command: &str,
) -> Result<()> {
    authorize(binding, command)?;
    valid(
        task.project == binding.project_id && task.domain == binding.domain,
        "task project/domain differs from binding",
    )?;
    check(
        task.requested_actions.iter().any(|value| value == command),
        ContractErrorKind::ActionNotAllowedError,
        "current action not requested by task",
    )
}
pub fn native_binding(binding: &ProjectBinding) -> Result<Value> {
    valid(
        binding.target_kind == "workspace" && binding.workspace_id.is_some(),
        "native lifecycle requires workspace binding",
    )?;
    check(
        binding.semantic_api == NATIVE_REFINERY_CONTRACT,
        ContractErrorKind::CapabilityUnavailableError,
        "native binding requires exact refinery contract",
    )?;
    Ok(
        json!({"schema_version":"1.0","binding_id":binding.binding_id,"project_id":binding.project_id,"domain":binding.domain,
        "package_id":binding.package_id,"workspace_id":binding.workspace_id,"baseline_version":binding.baseline_version,"baseline_package_sha256":binding.baseline_digest,
        "evidence_root":binding.evidence_logical_root,"fact_authorities":[authority_token(&binding.fact_authority)?],"decision_authorities":[authority_token(&binding.decision_authority)?],
        "allowed_actions":binding.lifecycle_actions,"semantic_api_contract":binding.semantic_api,"promotion_target":binding.promotion_target,"created_at":binding.created_at}),
    )
}
pub fn native_task(task: &SemanticTaskEnvelope, action: &str) -> Result<Value> {
    let command = match action {
        "engagement" => "open",
        "candidate" | "proposed" => "propose",
        "committed" => "commit",
        "promoted" => "promote",
        "execute_candidate"
        | "derive_regression_gate"
        | "regression_passed"
        | "derive_release_gate"
        | "release_complete" => "verify",
        "run" => "run",
        "review" => "review",
        _ => return Err(invalid("unsupported native task action")),
    };
    check(
        task.requested_actions.iter().any(|value| value == command),
        ContractErrorKind::ActionNotAllowedError,
        "native action has no current task authority",
    )?;
    Ok(
        json!({"schema_version":"1.0","task_id":task.task_id,"task_kind":task.task_kind,"project_id":task.project,"domain":task.domain,
        "intent":task.intent,"requested_decision":task.requested_decision,"actor_id":task.actor_id,"requested_actions":[action],
        "required_capabilities":task.required_capabilities,"evidence":task.evidence,"created_at":task.created_at}),
    )
}
pub fn next_binding_projection(binding: &ProjectBinding, descriptor: &Value) -> Result<Value> {
    valid(
        binding.target_kind == "workspace" && binding.workspace_id.is_some(),
        "successor requires workspace binding",
    )?;
    for field in [
        "package_id",
        "version",
        "package_sha256",
        "manifest_sha256",
        "promotion_record_sha256",
        "promoted_at",
    ] {
        check(
            descriptor.get(field).is_some(),
            ContractErrorKind::CapabilityUnavailableError,
            format!("promotion descriptor missing {field}"),
        )?;
    }
    check(
        descriptor["package_id"] == binding.package_id,
        ContractErrorKind::BaselineMismatchError,
        "promotion package differs from binding",
    )?;
    let version = text(&descriptor["version"], "promotion version")?;
    let package_sha = hash(&descriptor["package_sha256"], "promotion package SHA")?;
    let manifest_sha = hash(&descriptor["manifest_sha256"], "promotion manifest SHA")?;
    let promotion_sha = hash(
        &descriptor["promotion_record_sha256"],
        "promotion record SHA",
    )?;
    let promoted_at = timestamp(&descriptor["promoted_at"], "promoted_at")?;
    check(
        version != binding.baseline_version || package_sha != binding.baseline_digest,
        ContractErrorKind::BaselineMismatchError,
        "promotion did not produce successor baseline",
    )?;
    let mut document = binding.document();
    document["binding_id"] = json!(format!("binding:promotion:{promotion_sha}"));
    document["baseline"] = json!({"version":version,"digest":package_sha});
    document["created_at"] = json!(promoted_at);
    document["predecessor"] =
        json!({"binding_sha256":binding.source_sha256,"promotion_record_sha256":promotion_sha});
    Ok(
        json!({"status":"proposed","requires_control_plane_approval":true,"auto_applied":false,
        "predecessor_binding_sha256":binding.source_sha256,"promotion_record_sha256":promotion_sha,
        "registry_coordinates":{"workspace_id":binding.workspace_id,"package_id":binding.package_id,"version":version,
            "package_sha256":package_sha,"manifest_sha256":manifest_sha},"projection_sha256":sha256(canonical_json(&document)?),"document":document}),
    )
}
