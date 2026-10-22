//! Recipient execution of a frozen method capsule through native Semantica.
//!
//! This module is bookkeeping and authorization orchestration. It does not own
//! a registry, evaluate semantic rules, import publisher decisions, or establish
//! physical acceptance. Project adoption is a separate, exactly authorized act.

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use oe_core::json::{canonical_json, strict_json};
use oe_core::{SkillRoot, create_new_private_dir, read_regular_bytes, sha256, write_new_bytes};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use crate::{Payload, load_bundle, safe_relative, validate_data_archive};

pub const LOCK: &str = "runtime/semantic-bootstrap.json";
pub const DEFAULT_BUNDLE: &str = "engineering-judgment-intake";
/// Full provenance reports can exhaust the native 8 MiB output cap well below
/// its 64-scenario API maximum. Bootstrap never trims a report or raises a cap.
pub const AUTHORING_BATCH_SCENARIOS: usize = 4;
const LIMIT: usize = 64 * 1024 * 1024;
const EMPTY: &str = "fbb81100bfb26f5706a810bbf7b6ac574868558183c2e4b8e95d1fdb89613947";
const CATEGORIES: [&str; 8] = [
    "ontology",
    "competency_questions",
    "shapes",
    "queries",
    "rules",
    "cases",
    "contract",
    "provenance",
];
const CAPS: [&str; 4] = [
    "semantic.package.load",
    "sparql.select",
    "shacl.validate",
    "rule.forward_chain",
];
const SCOPE: [&str; 2] = [
    "receive-frozen-method-commit",
    "receive-frozen-method-promotion",
];
const STATES: [&str; 6] = [
    "candidate",
    "proposed",
    "committed",
    "regression_passed",
    "release_complete",
    "promoted",
];
const CODE: [&str; 19] = [
    "Cargo.toml",
    "Cargo.lock",
    "crates/oe-core/Cargo.toml",
    "crates/oe-core/src/lib.rs",
    "crates/oe-core/src/json.rs",
    "crates/oe-wire/Cargo.toml",
    "crates/oe-wire/src/lib.rs",
    "crates/oe-wire/src/json.rs",
    "crates/oe-wire/src/unicode_casefold.rs",
    "crates/oe-semantic/Cargo.toml",
    "crates/oe-semantic/src/lib.rs",
    "crates/oe-semantic/src/bootstrap.rs",
    "crates/oe-semantic/src/contracts.rs",
    "crates/oe-semantic/src/delta.rs",
    "crates/oe-semantic/src/lifecycle.rs",
    "crates/oe-semantic/src/native.rs",
    "crates/oe-semantic/src/runtime.rs",
    "crates/oe-cli/Cargo.toml",
    "crates/oe-cli/src/main.rs",
];

#[derive(Debug, Clone)]
pub struct LoadedCapsule {
    pub spec: Value,
    pub payload: Payload,
    pub capsule: Value,
}

#[derive(Debug, Clone)]
pub struct PrepareOptions {
    pub project: String,
    pub domain: String,
    pub actor: String,
    pub fact_authority: String,
    pub evidence_root: String,
    pub name: String,
}

/// Typed local invocation; paths are retained before any native state mutation.
pub struct Invocation<'a> {
    pub binding: &'a Path,
    pub task: &'a Path,
    pub workspace: &'a Path,
    pub candidate: &'a str,
    pub delta: Option<&'a Path>,
    pub engagement: Option<&'a Path>,
    pub authorization: Option<&'a Path>,
}

/// The production implementation only invokes source-locked native APIs.
/// Injection is for transport/orchestration tests, never evidence of semantics.
pub trait BootstrapExecutor {
    fn verify_runtime(&mut self) -> Result<()>;
    fn discover(&mut self, binding: Option<&Path>, workspace: Option<&Path>) -> Result<Value>;
    fn invoke(&mut self, action: &str, request: &Invocation<'_>) -> Result<Value>;
    fn history(&mut self, binding: &Path, workspace: &Path, candidate: &str) -> Result<Value>;
    fn check_events(
        &mut self,
        binding: &Path,
        step_dir: &Path,
        events: &[Value],
        candidate: &str,
    ) -> Result<()>;
    fn authoring_checks(&mut self, manifest: &Path, scenarios: &[String]) -> Result<Vec<Value>>;
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    oe_core::text(&value[key], key)
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value[key]
        .as_array()
        .with_context(|| format!("bootstrap_invalid_{key}"))
}
fn read(path: &Path) -> Result<Value> {
    Ok(strict_json(&read_regular_bytes(path, LIMIT)?)?)
}
fn now() -> String {
    DateTime::<Utc>::from(SystemTime::now()).to_rfc3339()
}
fn unique() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    sha256(format!(
        "{}:{}:{}",
        now(),
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))[..32]
        .into()
}

/// Python-compatible sorted, two-space JSON including the final newline.
/// Structural whitespace is added to the shared canonical token stream, so
/// arbitrary integers, floating-point spelling, Unicode and string escapes keep
/// their shared identity rather than going through a second numeric formatter.
pub fn encoded(value: &Value) -> Result<Vec<u8>> {
    let input = canonical_json(value)?;
    let mut out = Vec::new();
    let mut depth = 0;
    let mut string = false;
    let mut escaped = false;
    for (index, byte) in input.iter().copied().enumerate() {
        if string {
            out.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                string = false;
            }
            continue;
        }
        match byte {
            b'"' => {
                string = true;
                out.push(byte);
            }
            b'{' | b'[' => {
                out.push(byte);
                depth += 1;
                if !matches!(input.get(index + 1), Some(b'}' | b']')) {
                    out.push(b'\n');
                    out.extend(std::iter::repeat_n(b' ', depth * 2));
                }
            }
            b'}' | b']' => {
                depth -= 1;
                if !matches!(input.get(index.wrapping_sub(1)), Some(b'{' | b'[')) {
                    out.push(b'\n');
                    out.extend(std::iter::repeat_n(b' ', depth * 2));
                }
                out.push(byte);
            }
            b',' => {
                out.extend_from_slice(b",\n");
                out.extend(std::iter::repeat_n(b' ', depth * 2));
            }
            b':' => out.extend_from_slice(b": "),
            _ => out.push(byte),
        }
    }
    out.push(b'\n');
    Ok(out)
}

fn retain(path: &Path, value: &Value) -> Result<PathBuf> {
    let bytes = encoded(value)?;
    retain_bytes(path, &bytes)
}
fn retain_bytes(path: &Path, bytes: &[u8]) -> Result<PathBuf> {
    ensure!(
        bytes.len() <= LIMIT,
        "bootstrap_retained_part_exceeds_limit"
    );
    match fs::symlink_metadata(path) {
        Ok(_) => ensure!(
            read_regular_bytes(path, LIMIT)? == bytes,
            "bootstrap_retained_input_differs"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => write_new_bytes(path, bytes)?,
        Err(error) => return Err(error.into()),
    }
    Ok(path.to_path_buf())
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_file() && !meta.file_type().is_symlink(),
                "bootstrap_bookkeeping_not_regular"
            );
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            oe_core::no_symlinks(path)?;
            ensure!(meta.is_dir(), "bootstrap_not_directory");
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_new_private_dir(path),
        Err(error) => Err(error.into()),
    }
}
fn code_identity(root: &Path) -> Result<Value> {
    let mut result = serde_json::Map::new();
    for name in CODE {
        result.insert(
            name.into(),
            json!(sha256(read_regular_bytes(&root.join(name), LIMIT)?)),
        );
    }
    Ok(Value::Object(result))
}
fn executable_sha256() -> Result<String> {
    Ok(sha256(read_regular_bytes(
        &std::env::current_exe()?,
        256 * 1024 * 1024,
    )?))
}

pub fn load_capsule(root: &Path, name: &str) -> Result<LoadedCapsule> {
    let root = SkillRoot::new(root)?;
    let lock = read(&root.path().join(LOCK))?;
    ensure!(
        lock["schema"] == "ontology-engineering.method-bootstrap-lock/v1",
        "bootstrap_lock_schema"
    );
    let spec = lock["capsules"]
        .get(name)
        .context("bootstrap_capsule_not_found")?
        .clone();
    let bundle = load_bundle(root.path(), name)?;
    for key in ["package_id", "package_version", "package_sha256", "runtime"] {
        ensure!(
            spec.get(key).is_some() && spec[key] == bundle.spec[key],
            "bootstrap_active_bundle_differs"
        );
    }
    let path = safe_relative(text(&spec, "path")?)?;
    let payload =
        validate_data_archive(&read_regular_bytes(&root.path().join(path), LIMIT)?, &spec)?;
    let capsule = payload_json(&payload, "capsule.json")?;
    validate_capsule(&spec, &payload, &capsule)?;
    Ok(LoadedCapsule {
        spec,
        payload,
        capsule,
    })
}

fn payload_json(payload: &Payload, path: &str) -> Result<Value> {
    safe_relative(path)?;
    Ok(strict_json(
        payload
            .get(path)
            .context("bootstrap_capsule_member_missing")?,
    )?)
}
fn validate_capsule(spec: &Value, payload: &Payload, capsule: &Value) -> Result<()> {
    ensure!(
        capsule["schema"] == "semantica.method-bootstrap-capsule/v1",
        "bootstrap_capsule_schema"
    );
    for (left, right) in [
        ("package_id", "package_id"),
        ("target_version", "package_version"),
        ("target_package_sha256", "package_sha256"),
        ("runtime", "runtime"),
    ] {
        ensure!(
            capsule.get(left).is_some() && capsule[left] == spec[right],
            "bootstrap_capsule_target_differs"
        );
    }
    ensure!(
        crate::contracts::authority_token(&capsule["publisher_fact_authority"])?
            == text(capsule, "native_source_authority")?,
        "bootstrap_source_authority_differs"
    );
    let steps = array(capsule, "steps")?;
    ensure!(
        !steps.is_empty() && steps.len() <= 64,
        "bootstrap_step_count"
    );
    let mut previous = ("0".to_owned(), EMPTY.to_owned());
    let mut inventory = BTreeSet::from(["capsule.json".to_owned()]);
    let mut versions = BTreeSet::new();
    for step in steps {
        ensure!(
            versions.insert(text(step, "version")?),
            "bootstrap_duplicate_version"
        );
        let delta_path = safe_relative(text(step, "delta_path")?)?;
        let delta = crate::delta::validate_native_delta(&payload_json(payload, delta_path)?)?;
        ensure!(
            (
                text(&delta, "base_version")?,
                text(&delta, "base_package_sha256")?
            ) == (previous.0.as_str(), previous.1.as_str()),
            "bootstrap_predecessor_differs"
        );
        ensure!(
            delta["package_id"] == capsule["package_id"]
                && delta["target_version"] == step["version"]
                && delta["delta_sha256"] == step["delta_sha256"]
                && delta["created_by"] == capsule["native_source_authority"],
            "bootstrap_delta_identity_differs"
        );
        let manifest_path = safe_relative(text(step, "manifest_path")?)?;
        let manifest = payload_json(payload, manifest_path)?;
        ensure!(
            manifest["package_id"] == delta["package_id"]
                && manifest["version"] == delta["target_version"],
            "bootstrap_manifest_differs"
        );
        ensure!(
            inventory.insert(delta_path.to_owned()) && inventory.insert(manifest_path.to_owned()),
            "bootstrap_reused_step_member"
        );
        let prefix = Path::new(manifest_path)
            .parent()
            .context("bootstrap_manifest_parent")?;
        for asset in array(&manifest, "assets")? {
            let member = prefix.join(safe_relative(text(asset, "path")?)?);
            let member = member.to_str().context("bootstrap_non_utf8_member")?;
            ensure!(
                sha256(payload.get(member).context("bootstrap_asset_missing")?)
                    == text(asset, "sha256")?,
                "bootstrap_asset_differs"
            );
            ensure!(
                inventory.insert(member.to_owned()),
                "bootstrap_duplicate_asset_path"
            );
        }
        let registry_path = safe_relative(text(step, "scenario_registry")?)?;
        let registry = payload_json(payload, registry_path)?;
        ensure!(
            inventory.contains(registry_path)
                && registry["package_id"] == delta["package_id"]
                && !array(&registry, "scenarios")?.is_empty(),
            "bootstrap_scenario_registry_differs"
        );
        let mut scenarios = BTreeSet::new();
        for scenario in array(&registry, "scenarios")? {
            ensure!(
                scenarios.insert(text(scenario, "id")?),
                "bootstrap_duplicate_scenario"
            );
        }
        let source_prefix = format!("{}/", text(capsule, "source_logical_root")?);
        for source in array(&delta, "source_evidence")? {
            let member = format!("sources/{}.json", text(source, "sha256")?);
            ensure!(
                sha256(payload.get(&member).context("bootstrap_source_missing")?)
                    == text(source, "sha256")?
                    && text(source, "uri")?.starts_with(&source_prefix),
                "bootstrap_source_identity_differs"
            );
            inventory.insert(member);
        }
        previous = (
            text(step, "version")?.to_owned(),
            text(step, "package_sha256")?.to_owned(),
        );
    }
    ensure!(
        (previous.0.as_str(), previous.1.as_str())
            == (
                text(spec, "package_version")?,
                text(spec, "package_sha256")?
            )
            && inventory == payload.keys().cloned().collect(),
        "bootstrap_chain_or_inventory_differs"
    );
    Ok(())
}

fn library_binding(plan: &Value, capsule: &Value) -> Value {
    json!({"$schema":"ontology-engineering.semantic-project-binding/v1", "binding_id":format!("{}-0", plan["workspace_id"].as_str().unwrap_or_default()), "project":{"project_id":plan["project"],"domain":plan["domain"]}, "semantic_target":{"kind":"workspace","workspace_id":plan["workspace_id"],"package_id":plan["package_id"]}, "baseline":plan["empty_baseline"], "evidence":{"logical_root":capsule["source_logical_root"]}, "authority":{"fact":capsule["publisher_fact_authority"],"decision":{"authority_id":plan["actor"],"scope":SCOPE}}, "allowed_actions":["open","discover","run","review","propose","commit","verify","history","promote"], "lifecycle_actions":STATES, "promotion":{"target":"industry-registry","requires_decision_authority":true},"created_at":plan["created_at"],"semantic_api":"semantica.ontology.refinery/v1"})
}
fn project_binding(library: &Value, plan: &Value) -> Value {
    let mut result = library.clone();
    result["binding_id"] = json!(format!(
        "{}-project",
        plan["plan_id"].as_str().unwrap_or_default()
    ));
    result["evidence"] = json!({"logical_root":plan["evidence_root"]});
    result["authority"]["fact"] =
        json!({"authority_id":plan["fact_authority"],"scope":["controlled-project-records"]});
    result["allowed_actions"] = json!(["open", "discover", "run", "review", "history", "propose"]);
    result["lifecycle_actions"] = json!(["candidate", "proposed"]);
    result
}
fn validate_binding(value: &Value) -> Result<()> {
    crate::contracts::parse_project_binding(&encoded(value)?)?;
    Ok(())
}

pub fn prepare_with_executor(
    root: &Path,
    destination: &Path,
    options: &PrepareOptions,
    executor: &mut dyn BootstrapExecutor,
) -> Result<Value> {
    let root = SkillRoot::new(root)?;
    let destination = root.private_path(destination)?;
    ensure!(
        fs::symlink_metadata(&destination).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "bootstrap_plan_requires_new_directory"
    );
    for value in [
        &options.project,
        &options.domain,
        &options.actor,
        &options.fact_authority,
        &options.evidence_root,
        &options.name,
    ] {
        ensure!(
            !value.trim().is_empty(),
            "bootstrap_recipient_identity_required"
        );
    }
    let loaded = load_capsule(root.path(), &options.name)?;
    let discovery = executor.discover(None, None)?;
    ensure!(
        discovery["command_verdict"] == "passed",
        "bootstrap_discovery_did_not_pass"
    );
    let empty = &discovery["corpus_found"]["native_workspace_bootstrap"];
    ensure!(
        empty["baseline_version"] == "0" && empty["baseline_digest"] == EMPTY,
        "bootstrap_empty_baseline_differs"
    );
    let plan = json!({"schema":"ontology-engineering.method-bootstrap-plan/v1","plan_id":unique(),"directory":destination,"workspace":destination.join("registry"),"bundle":options.name,"capsule_sha256":loaded.spec["sha256"],"package_id":loaded.spec["package_id"],"package_version":loaded.spec["package_version"],"package_sha256":loaded.spec["package_sha256"],"runtime":loaded.spec["runtime"],"code":code_identity(root.path())?,"executable_sha256":executable_sha256()?,"project":options.project,"domain":options.domain,"actor":options.actor,"fact_authority":options.fact_authority,"evidence_root":options.evidence_root,"workspace_id":format!("method-library-{}",unique()),"empty_baseline":{"version":empty["baseline_version"],"digest":empty["baseline_digest"]},"created_at":now(),"scope":"Re-execute and locally commit/promote the frozen method chain, saving each library successor binding separately. Project adoption is a separate action; no physical or model-quality approval."});
    let initial = library_binding(&plan, &loaded.capsule);
    let project = project_binding(&initial, &plan);
    validate_binding(&initial)?;
    validate_binding(&project)?;
    create_new_private_dir(&destination)?;
    retain(&destination.join("plan.json"), &plan)?;
    retain(&destination.join("initial-binding.json"), &initial)?;
    retain(&destination.join("project-binding-template.json"), &project)?;
    Ok(
        json!({"plan":destination.join("plan.json"),"plan_sha256":sha256(encoded(&plan)?),"status":"prepared","workspace_created":false,"project_adopted":false}),
    )
}

fn load_plan(root: &Path, destination: &Path) -> Result<(PathBuf, Value, LoadedCapsule)> {
    let root = SkillRoot::new(root)?;
    let destination = root.private_path(destination)?;
    let plan = read(&destination.join("plan.json"))?;
    ensure!(
        plan["schema"] == "ontology-engineering.method-bootstrap-plan/v1"
            && plan["directory"] == json!(destination)
            && plan["workspace"] == json!(destination.join("registry")),
        "bootstrap_plan_directory_or_schema_differs"
    );
    let loaded = load_capsule(root.path(), text(&plan, "bundle")?)?;
    ensure!(
        plan["capsule_sha256"] == loaded.spec["sha256"]
            && plan["runtime"] == loaded.spec["runtime"]
            && plan["code"] == code_identity(root.path())?
            && plan["executable_sha256"] == executable_sha256()?,
        "bootstrap_configuration_changed_prepare_separate_plan"
    );
    for key in ["package_id", "package_version", "package_sha256"] {
        ensure!(
            plan[key] == loaded.spec[key],
            "bootstrap_plan_target_differs"
        );
    }
    Ok((destination, plan, loaded))
}
fn authorization(
    path: &Path,
    plan: &Value,
    action: &str,
    binding_sha: Option<&str>,
) -> Result<Value> {
    let value = read(path)?;
    let mut keys: BTreeSet<_> = [
        "schema",
        "action",
        "actor_id",
        "plan_sha256",
        "reason",
        "issued_at",
    ]
    .into_iter()
    .collect();
    if binding_sha.is_some() {
        keys.insert("binding_sha256");
    }
    ensure!(
        value
            .as_object()
            .context("bootstrap_authorization_not_object")?
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == keys
            && value["schema"] == "ontology-engineering.method-bootstrap-authorization/v1",
        "bootstrap_authorization_fields"
    );
    ensure!(
        value["action"] == action
            && value["actor_id"] == plan["actor"]
            && value["plan_sha256"] == sha256(encoded(plan)?),
        "bootstrap_authorization_plan_action_actor_differs"
    );
    text(&value, "reason")?;
    DateTime::parse_from_rfc3339(text(&value, "issued_at")?)
        .context("bootstrap_authorization_requires_rfc3339_timezone")?;
    if let Some(hash) = binding_sha {
        ensure!(
            value["binding_sha256"] == hash,
            "bootstrap_authorization_binding_differs"
        );
    }
    Ok(value)
}

/// The descriptor remains owned for the operation, including native recovery.
#[cfg(unix)]
struct Exclusive(fs::File);
#[cfg(unix)]
impl Exclusive {
    fn acquire(path: &Path) -> Result<Self> {
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::ffi::OsStrExt;
        // Open every parent without following links, then anchor the lock with
        // openat. Parent replacement cannot redirect creation into another tree.
        let initial = unsafe {
            libc::open(
                c"/".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        ensure!(initial >= 0, "bootstrap_lock_root_open_failed");
        let mut dir = unsafe { fs::File::from_raw_fd(initial) };
        for part in path.components() {
            match part {
                std::path::Component::RootDir => continue,
                std::path::Component::Normal(name) => {
                    let name = std::ffi::CString::new(name.as_bytes())?;
                    let fd = unsafe {
                        libc::openat(
                            dir.as_raw_fd(),
                            name.as_ptr(),
                            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                        )
                    };
                    ensure!(fd >= 0, "bootstrap_lock_parent_open_failed");
                    dir = unsafe { fs::File::from_raw_fd(fd) };
                }
                _ => bail!("bootstrap_lock_invalid_parent"),
            }
        }
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                c".bootstrap.lock".as_ptr(),
                libc::O_RDWR
                    | libc::O_CREAT
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | libc::O_CLOEXEC,
                0o600,
            )
        };
        ensure!(fd >= 0, "bootstrap_lock_open_failed");
        let file = unsafe { fs::File::from_raw_fd(fd) };
        ensure!(file.metadata()?.is_file(), "bootstrap_lock_not_regular");
        ensure!(
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "bootstrap_already_running"
        );
        Ok(Self(file))
    }
}
#[cfg(unix)]
impl Drop for Exclusive {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
#[cfg(not(unix))]
struct Exclusive;
#[cfg(not(unix))]
impl Exclusive {
    fn acquire(_: &Path) -> Result<Self> {
        bail!("bootstrap_exclusive_lock_requires_unix")
    }
}

fn task(plan: &Value, delta: &Value, action: &str) -> Value {
    json!({"$schema":"ontology-engineering.semantic-task-envelope/v1","task_id":unique(),"task_kind":"receive-frozen-engineering-methods","intent":"Execute this locally authorized step of the exact frozen method chain","project":plan["project"],"domain":plan["domain"],"requested_decision":"Local method package lifecycle; no project fact acceptance","actor_id":plan["actor"],"requested_actions":[action],"required_capabilities":CAPS,"evidence":delta["source_evidence"],"created_at":now()})
}
fn step_authorization(
    plan: &Value,
    capsule: &Value,
    delta: &Value,
    action: &str,
    basis: &Value,
) -> Result<Value> {
    let mut decisions = Vec::new();
    if action == "commit" {
        for category in CATEGORIES {
            for asset in array(delta, category)? {
                if matches!(asset["operation"].as_str(), Some("replace" | "remove")) {
                    decisions.push(json!({"category":asset["category"],"asset_id":asset["asset_id"],"operation":asset["operation"],"replaces_sha256":asset["replaces_sha256"],"verdict":"approve","reason":"Replay the frozen method successor under this local initialization authorization; preserve predecessor history."}));
                }
            }
        }
    }
    Ok(
        json!({"$schema":"ontology-engineering.refinery-authorization/v1","authorization_id":unique(),"action":action,"actor_id":plan["actor"],"authority_id":plan["actor"],"authority_scope":SCOPE,"package_id":delta["package_id"],"delta_sha256":delta["delta_sha256"],"promotion_target":"industry-registry","reason":format!("Recipient frozen-chain action under exact local plan: {}",text(basis,"reason")?),"source":{"source_id":"recipient-initialization-authorization","uri":format!("{}/recipient-authorization",text(capsule,"source_logical_root")?),"sha256":sha256(encoded(basis)?),"media_type":"application/json","captured_at":basis["issued_at"]},"issued_at":now(),"decisions":decisions}),
    )
}
fn engagement(checks: &[Value]) -> Result<Value> {
    let hash = checks_sha256(checks)?;
    let receipts: Vec<Value> = checks.iter().map(|check| json!({"package_id":check["execution"]["package_id"],"package_version":check["execution"]["package_version"],"package_digest":check["execution"]["package_digest"],"receipt_sha256":check["execution"]["receipt"]["receipt_sha256"]})).collect();
    let mut value = json!({"engagement_id":unique(),"learning":{"status":"candidate","rationale":"Frozen reusable method chain; all authoring scenarios executed and receipts verified locally."},"execution_receipts":receipts,"created_at":now()});
    for phase in ["execution", "receipt", "regression", "release"] {
        value[phase] = json!({"name":phase,"status":"passed","required_capabilities":CAPS,"observed_capabilities":CAPS,"evidence_sha256":hash,"details":{"scope":"Actual recipient authoring-manifest executions and receipt checks. Native committed-subject gates are derived separately; no physical acceptance."}});
    }
    Ok(value)
}
/// Hash exactly encoded(all_checks), with bounded per-check formatting rather
/// than constructing the hundreds-of-MiB combined pretty-printed JSON string.
fn checks_sha256(checks: &[Value]) -> Result<String> {
    let mut hash = Sha256::new();
    if checks.is_empty() {
        hash.update(b"[]\n");
    } else {
        hash.update(b"[\n");
        for (index, check) in checks.iter().enumerate() {
            let bytes = encoded(check)?;
            let mut lines = bytes[..bytes.len() - 1].split(|b| *b == b'\n').peekable();
            while let Some(line) = lines.next() {
                hash.update(b"  ");
                hash.update(line);
                if lines.peek().is_some() {
                    hash.update(b"\n");
                }
            }
            hash.update(if index + 1 == checks.len() {
                b"\n".as_slice()
            } else {
                b",\n".as_slice()
            });
        }
        hash.update(b"]\n");
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn scenario_list_sha256(checks: &[Value]) -> Result<String> {
    let ids = checks
        .iter()
        .map(|c| c["execution"]["scenario_id"].clone())
        .collect::<Vec<_>>();
    Ok(sha256(canonical_json(&json!(ids))?))
}
fn retain_checks(step_dir: &Path, stem: &str, checks: &[Value]) -> Result<()> {
    safe_relative(stem)?;
    let part_dir = step_dir.join(stem);
    directory(&part_dir)?;
    let mut parts = Vec::new();
    for (index, chunk) in checks.chunks(AUTHORING_BATCH_SCENARIOS).enumerate() {
        let name = format!("{stem}/part-{index:04}.json");
        let bytes = encoded(&json!(chunk))?;
        ensure!(
            bytes.len() <= LIMIT,
            "bootstrap_authoring_part_exceeds_limit"
        );
        retain_bytes(&step_dir.join(&name), &bytes)?;
        parts.push(json!({"path":name,"sha256":sha256(&bytes),"count":chunk.len(),"start":index*AUTHORING_BATCH_SCENARIOS}));
    }
    retain(
        &step_dir.join(format!("{stem}.json")),
        &json!({"schema":"ontology-engineering.authoring-checks-index/v1","parts":parts,"scenario_count":checks.len(),"scenario_ids_sha256":scenario_list_sha256(checks)?,"checks_sha256":checks_sha256(checks)?}),
    )?;
    Ok(())
}
fn exact_keys(value: &Value, expected: &[&str]) -> Result<()> {
    ensure!(
        value
            .as_object()
            .context("bootstrap_index_not_object")?
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == expected.iter().copied().collect(),
        "bootstrap_authoring_index_fields"
    );
    Ok(())
}
fn read_checks(step_dir: &Path, loaded: &LoadedCapsule, step: &Value) -> Result<Vec<Value>> {
    let index = read(&step_dir.join("authoring-checks.json"))?;
    exact_keys(
        &index,
        &[
            "schema",
            "parts",
            "scenario_count",
            "scenario_ids_sha256",
            "checks_sha256",
        ],
    )?;
    ensure!(
        index["schema"] == "ontology-engineering.authoring-checks-index/v1",
        "bootstrap_authoring_index_schema"
    );
    let scenarios = authoring_scenarios(loaded, step)?;
    ensure!(
        index["scenario_count"] == json!(scenarios.len())
            && index["scenario_ids_sha256"] == sha256(canonical_json(&json!(scenarios))?),
        "bootstrap_authoring_index_scenarios_differ"
    );
    let parts = array(&index, "parts")?;
    ensure!(
        parts.len() == scenarios.len().div_ceil(AUTHORING_BATCH_SCENARIOS),
        "bootstrap_authoring_part_count"
    );
    let mut checks = Vec::new();
    for (ordinal, part) in parts.iter().enumerate() {
        exact_keys(part, &["path", "sha256", "count", "start"])?;
        let expected_count = (scenarios.len() - checks.len()).min(AUTHORING_BATCH_SCENARIOS);
        let expected_name = format!("authoring-checks/part-{ordinal:04}.json");
        ensure!(
            part["path"] == expected_name
                && part["start"] == json!(checks.len())
                && part["count"] == json!(expected_count),
            "bootstrap_authoring_part_order_or_count"
        );
        let bytes = read_regular_bytes(&step_dir.join(&expected_name), LIMIT)?;
        ensure!(
            part["sha256"] == sha256(&bytes),
            "bootstrap_authoring_part_hash_differs"
        );
        let chunk = strict_json(&bytes)?;
        let chunk = chunk
            .as_array()
            .context("bootstrap_authoring_part_not_array")?;
        ensure!(
            chunk.len() == expected_count,
            "bootstrap_authoring_part_coverage"
        );
        checks.extend(chunk.iter().cloned());
    }
    ensure!(
        index["checks_sha256"] == checks_sha256(&checks)?
            && index["scenario_ids_sha256"] == scenario_list_sha256(&checks)?,
        "bootstrap_authoring_complete_hash_differs"
    );
    validate_authoring_checks(loaded, step, &checks)?;
    Ok(checks)
}
fn authoring_scenarios(loaded: &LoadedCapsule, step: &Value) -> Result<Vec<String>> {
    let registry = payload_json(&loaded.payload, text(step, "scenario_registry")?)?;
    array(&registry, "scenarios")?
        .iter()
        .map(|v| text(v, "id").map(str::to_owned))
        .collect()
}
fn valid_sha(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
/// Recheck recipient evidence coverage and byte identities on both initial
/// execution and resume. This checks the native reports' retained bindings;
/// it does not substitute a Rust verdict for the native receipt verifier.
fn validate_authoring_checks(loaded: &LoadedCapsule, step: &Value, checks: &[Value]) -> Result<()> {
    let scenarios = authoring_scenarios(loaded, step)?;
    ensure!(
        checks.len() == scenarios.len(),
        "bootstrap_authoring_scenario_count"
    );
    let manifest = payload_json(&loaded.payload, text(step, "manifest_path")?)?;
    let mut asset_hashes = serde_json::Map::new();
    for asset in array(&manifest, "assets")? {
        ensure!(
            asset_hashes
                .insert(text(asset, "asset_id")?.into(), asset["sha256"].clone())
                .is_none(),
            "bootstrap_authoring_duplicate_asset"
        );
    }
    let asset_hashes = Value::Object(asset_hashes);
    let package_digest =
        &checks.first().context("bootstrap_authoring_checks_empty")?["execution"]["package_digest"];
    ensure!(
        valid_sha(package_digest),
        "bootstrap_authoring_package_digest"
    );
    for (check, scenario) in checks.iter().zip(&scenarios) {
        let execution = &check["execution"];
        let receipt = &execution["receipt"];
        ensure!(
            execution["scenario_id"] == *scenario,
            "bootstrap_authoring_scenario_order_or_identity"
        );
        ensure!(
            execution["status"] == "passed"
                && check["verification"]["status"] == "complete"
                && execution["release_verdict"]["status"] == "complete",
            "bootstrap_authoring_execution_did_not_pass"
        );
        ensure!(
            execution["runner_contract"] == "semantica.chapter_packages.SemanticPackageRunner/v1"
                && execution["package_id"] == loaded.spec["package_id"]
                && execution["package_version"] == step["version"]
                && execution["package_digest"] == *package_digest,
            "bootstrap_authoring_identity_differs"
        );
        for key in ["package_id", "package_version", "package_digest"] {
            ensure!(
                receipt[key] == execution[key],
                "bootstrap_authoring_receipt_package_differs"
            );
        }
        ensure!(
            receipt["asset_hashes"] == asset_hashes,
            "bootstrap_authoring_receipt_assets_differ"
        );
        for (key, expected) in [
            ("runtime_commit", "commit"),
            ("runtime_version", "version"),
            ("runtime_artifact_sha256", "wheel_sha256"),
        ] {
            ensure!(
                receipt[key] == loaded.spec["runtime"][expected],
                "bootstrap_authoring_runtime_differs"
            );
        }
        ensure!(
            valid_sha(&receipt["receipt_sha256"])
                && check["verification"]["receipt_sha256"] == receipt["receipt_sha256"]
                && execution["release_verdict"]["receipt_sha256"] == receipt["receipt_sha256"],
            "bootstrap_authoring_receipt_reference_differs"
        );
    }
    Ok(())
}
fn validate_authoring_engagement(
    loaded: &LoadedCapsule,
    step: &Value,
    checks: &[Value],
    receipt: &Value,
) -> Result<()> {
    validate_authoring_checks(loaded, step, checks)?;
    text(receipt, "engagement_id")?;
    DateTime::parse_from_rfc3339(text(receipt, "created_at")?)
        .context("bootstrap_engagement_timestamp")?;
    let mut expected = engagement(checks)?;
    expected["engagement_id"] = receipt["engagement_id"].clone();
    expected["created_at"] = receipt["created_at"].clone();
    ensure!(
        *receipt == expected,
        "bootstrap_authoring_engagement_differs"
    );
    Ok(())
}
fn materialize(destination: &Path, payload: &Payload) -> Result<PathBuf> {
    let base = destination.join("authoring-inputs");
    directory(&base)?;
    for (name, bytes) in payload {
        let path = base.join(safe_relative(name)?);
        directory(path.parent().context("bootstrap_asset_parent")?)?;
        if exists(&path)? {
            ensure!(
                read_regular_bytes(&path, LIMIT)? == *bytes,
                "bootstrap_materialized_input_changed"
            );
        } else {
            write_new_bytes(&path, bytes)?;
        }
    }
    Ok(base)
}
fn checked_authoring(
    destination: &Path,
    step_dir: &Path,
    loaded: &LoadedCapsule,
    step: &Value,
    executor: &mut dyn BootstrapExecutor,
) -> Result<Value> {
    let base = materialize(destination, &loaded.payload)?;
    let scenarios = authoring_scenarios(loaded, step)?;
    let checks = executor.authoring_checks(&base.join(text(step, "manifest_path")?), &scenarios)?;
    if let Err(error) = validate_authoring_checks(loaded, step, &checks) {
        retain_checks(
            step_dir,
            &format!("authoring-failure-{}", unique()),
            &checks,
        )?;
        return Err(error);
    }
    // Detect changed retained inputs even if execution succeeded against a
    // different snapshot. Their exact source is the verified capsule payload.
    materialize(destination, &loaded.payload)?;
    let receipt = engagement(&checks)?;
    retain_checks(step_dir, "authoring-checks", &checks)?;
    retain(&step_dir.join("engagement.json"), &receipt)?;
    Ok(receipt)
}
fn passed(step_dir: &Path, action: &str, value: &Value) -> Result<()> {
    if value["command_verdict"] != "passed" {
        retain(&step_dir.join(format!("failure-{}.json", unique())), value)?;
        bail!("bootstrap_native_{action}_did_not_pass");
    }
    Ok(())
}

pub fn apply_with_executor(
    root: &Path,
    destination: &Path,
    authorization_path: &Path,
    executor: &mut dyn BootstrapExecutor,
) -> Result<Value> {
    let root = SkillRoot::new(root)?;
    let (destination, plan, loaded) = load_plan(root.path(), destination)?;
    let authorization_path = root.private_path(authorization_path)?;
    let basis = authorization(
        &authorization_path,
        &plan,
        "initialize-frozen-methods",
        None,
    )?;
    let _exclusive = Exclusive::acquire(&destination)?;
    executor.verify_runtime()?;
    retain(
        &destination.join("initialization-authorization.json"),
        &basis,
    )?;
    let mut binding_path = retain(
        &destination.join("initial-binding.json"),
        &library_binding(&plan, &loaded.capsule),
    )?;
    let workspace = PathBuf::from(text(&plan, "workspace")?);
    for (index, step) in array(&loaded.capsule, "steps")?.iter().enumerate() {
        eprintln!("bootstrap step {index}: {}", text(step, "version")?);
        let step_dir = destination.join(format!("step-{index:03}"));
        directory(&step_dir)?;
        binding_path = retain(&step_dir.join("binding.json"), &read(&binding_path)?)?;
        let delta = payload_json(&loaded.payload, text(step, "delta_path")?)?;
        let candidate = text(step, "delta_sha256")?;
        let mut projected = delta.clone();
        projected["created_by"] =
            loaded.capsule["publisher_fact_authority"]["authority_id"].clone();
        let delta_path = retain(&step_dir.join("delta-input.json"), &projected)?;
        let binding = crate::contracts::read_project_binding(&binding_path)?;
        ensure!(
            crate::delta::read_delta(&delta_path, &binding)?["delta_sha256"] == candidate,
            "bootstrap_projected_delta_identity_differs"
        );
        if exists(&step_dir.join("engagement.json"))? {
            validate_authoring_engagement(
                &loaded,
                step,
                &read_checks(&step_dir, &loaded, step)?,
                &read(&step_dir.join("engagement.json"))?,
            )?;
        }
        // Promotion owns native cross-ledger recovery. Its exact request must
        // be replayed before history inspection if its response was not saved.
        let promote_auth = step_dir.join("authorization-promote.json");
        let promote_path = step_dir.join("promote.json");
        if exists(&promote_auth)? && !exists(&promote_path)? {
            let result = executor.invoke(
                "promote",
                &Invocation {
                    binding: &binding_path,
                    task: &step_dir.join("task-promote.json"),
                    workspace: &workspace,
                    candidate,
                    delta: None,
                    engagement: None,
                    authorization: Some(&promote_auth),
                },
            )?;
            passed(&step_dir, "promote_recovery", &result)?;
            retain(&promote_path, &result)?;
        }
        let mut promoted = None;
        for action in ["open", "propose", "commit", "verify", "promote"] {
            eprintln!("bootstrap {}: {action}", text(step, "version")?);
            let task_path = step_dir.join(format!("task-{action}.json"));
            let task_existed = exists(&task_path)?;
            if !task_existed {
                retain(&task_path, &task(&plan, &delta, action))?;
            }
            let result_path = step_dir.join(format!("{action}.json"));
            if action == "open" {
                if index == 0 && !task_existed {
                    ensure!(
                        fs::symlink_metadata(&workspace)
                            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
                        "bootstrap_workspace_already_exists"
                    );
                }
                if !exists(&result_path)? {
                    let result = executor.invoke(
                        action,
                        &Invocation {
                            binding: &binding_path,
                            task: &task_path,
                            workspace: &workspace,
                            candidate,
                            delta: None,
                            engagement: None,
                            authorization: None,
                        },
                    )?;
                    // A newly opened engagement is deliberately blocked until
                    // real evidence exists. Only the native receipt delivery
                    // succeeded; this grants no later lifecycle authority.
                    let receipt_sha = result["receipt"]["receipt_sha256"].as_str();
                    let opened = result["native_execution"]["semantic_execution"] == "completed"
                        && result["execution"]["operation"] == "native_open_engagement"
                        && result["execution"]["mutation_performed"] == true
                        && result["receipt"]["status"] == "verified"
                        && receipt_sha.is_some_and(|sha| {
                            sha.len() == 64
                                && sha
                                    .bytes()
                                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        });
                    if !opened {
                        retain(
                            &step_dir.join(format!("open-failure-{}.json", unique())),
                            &result,
                        )?;
                        bail!("bootstrap_open_receipt_not_established");
                    }
                    retain(&result_path, &result)?;
                }
                continue;
            }
            let mut events = Vec::new();
            if exists(&step_dir.join("propose.json"))?
                || exists(&step_dir.join("task-commit.json"))?
                || action != "propose"
            {
                let history = executor.history(&binding_path, &workspace, candidate)?;
                passed(&step_dir, "history", &history)?;
                events = array(&history["execution"], "events")?.clone();
            }
            executor.check_events(&binding_path, &step_dir, &events, candidate)?;
            let target = match action {
                "propose" => "proposed",
                "commit" => "committed",
                "verify" => "release_complete",
                _ => "promoted",
            };
            if action != "promote"
                && let Some(event) = events.iter().find(|event| event["state"] == target)
            {
                if !exists(&result_path)? {
                    retain(
                        &result_path,
                        &json!({"status":"recovered_from_native_history","event":event}),
                    )?;
                }
                continue;
            }
            let engagement_path = step_dir.join("engagement.json");
            let auth_path = step_dir.join(format!("authorization-{action}.json"));
            if action == "propose" {
                if !exists(&engagement_path)? {
                    checked_authoring(&destination, &step_dir, &loaded, step, executor)?;
                } else {
                    let checks = read_checks(&step_dir, &loaded, step)?;
                    let receipt = read(&engagement_path)?;
                    validate_authoring_engagement(&loaded, step, &checks, &receipt)?;
                }
            } else if matches!(action, "commit" | "promote") && !exists(&auth_path)? {
                retain(
                    &auth_path,
                    &step_authorization(&plan, &loaded.capsule, &delta, action, &basis)?,
                )?;
            }
            let result = executor.invoke(
                action,
                &Invocation {
                    binding: &binding_path,
                    task: &task_path,
                    workspace: &workspace,
                    candidate,
                    delta: (action == "propose").then_some(delta_path.as_path()),
                    engagement: (action == "propose").then_some(engagement_path.as_path()),
                    authorization: matches!(action, "commit" | "promote")
                        .then_some(auth_path.as_path()),
                },
            )?;
            passed(&step_dir, action, &result)?;
            if !exists(&result_path)? {
                retain(&result_path, &result)?;
            }
            if action == "promote" {
                promoted = Some(result);
            }
        }
        let result = promoted.context("bootstrap_promotion_not_observed")?;
        let next = &result["learning"]["next_binding"];
        ensure!(
            next["auto_applied"] == false
                && next["document"]["baseline"]
                    == json!({"version":step["version"],"digest":step["package_sha256"]}),
            "bootstrap_successor_identity_differs"
        );
        validate_binding(&next["document"])?;
        binding_path = retain(
            &step_dir.join("library-adopted-binding.json"),
            &next["document"],
        )?;
        retain(
            &step_dir.join("library-adoption.json"),
            &json!({"actor":plan["actor"],"basis_sha256":sha256(encoded(&basis)?),"binding_sha256":sha256(read_regular_bytes(&binding_path,LIMIT)?),"scope":"Local method-library continuation only; separate project adoption required."}),
        )?;
    }
    let proposed = project_binding(&read(&binding_path)?, &plan);
    validate_binding(&proposed)?;
    let path = retain(
        &destination.join("proposed-project-binding.json"),
        &proposed,
    )?;
    Ok(
        json!({"status":"promoted_local_method_library","package_version":plan["package_version"],"package_sha256":plan["package_sha256"],"workspace":workspace,"proposed_binding":path,"binding_sha256":sha256(read_regular_bytes(&path,LIMIT)?),"project_adopted":false}),
    )
}

pub fn adopt_with_executor(
    root: &Path,
    destination: &Path,
    authorization_path: &Path,
    executor: &mut dyn BootstrapExecutor,
) -> Result<Value> {
    let root = SkillRoot::new(root)?;
    let (destination, plan, loaded) = load_plan(root.path(), destination)?;
    let authorization_path = root.private_path(authorization_path)?;
    let _exclusive = Exclusive::acquire(&destination)?;
    let proposal_path = destination.join("proposed-project-binding.json");
    let proposal_bytes = read_regular_bytes(&proposal_path, LIMIT)?;
    let proposal = strict_json(&proposal_bytes)?;
    let proposal_sha = sha256(&proposal_bytes);
    let expected = json!({"project":{"project_id":plan["project"],"domain":plan["domain"]},"semantic_target":{"kind":"workspace","workspace_id":plan["workspace_id"],"package_id":plan["package_id"]},"baseline":{"version":plan["package_version"],"digest":plan["package_sha256"]},"authority":{"fact":{"authority_id":plan["fact_authority"],"scope":["controlled-project-records"]},"decision":{"authority_id":plan["actor"],"scope":SCOPE}}});
    for key in ["project", "semantic_target", "baseline", "authority"] {
        ensure!(
            proposal[key] == expected[key],
            "bootstrap_project_adoption_exceeds_exact_plan"
        );
    }
    let last = array(&loaded.capsule, "steps")?.len() - 1;
    let library = read(&destination.join(format!("step-{last:03}/library-adopted-binding.json")))?;
    ensure!(
        proposal == project_binding(&library, &plan),
        "bootstrap_proposed_binding_differs_from_initialization"
    );
    validate_binding(&proposal)?;
    let basis = authorization(
        &authorization_path,
        &plan,
        "adopt-project-binding",
        Some(&proposal_sha),
    )?;
    executor.verify_runtime()?;
    let workspace = PathBuf::from(text(&plan, "workspace")?);
    ensure!(
        read_regular_bytes(&proposal_path, LIMIT)? == proposal_bytes,
        "bootstrap_proposal_changed_before_discovery"
    );
    let discovery = executor.discover(Some(&proposal_path), Some(&workspace))?;
    ensure!(
        read_regular_bytes(&proposal_path, LIMIT)? == proposal_bytes,
        "bootstrap_proposal_changed_during_discovery"
    );
    passed(&destination, "adoption_discovery", &discovery)?;
    if discovery["binding"]["source_sha256"] != proposal_sha {
        retain(
            &destination.join(format!(
                "adoption-discovery-identity-failure-{}.json",
                unique()
            )),
            &discovery,
        )?;
        bail!("bootstrap_discovery_binding_snapshot_differs");
    }
    let path = retain_bytes(&destination.join("project-binding.json"), &proposal_bytes)?;
    ensure!(
        read_regular_bytes(&path, LIMIT)? == proposal_bytes,
        "bootstrap_adopted_binding_changed"
    );
    retain(
        &destination.join("project-adoption.json"),
        &json!({"authorization":basis,"binding_sha256":proposal_sha,"discovery":discovery,"scope":"Local project use of frozen methods; no project fact, physical, model-quality or publication approval."}),
    )?;
    Ok(
        json!({"status":"project_binding_adopted","binding":path,"workspace":workspace,"package_sha256":plan["package_sha256"]}),
    )
}

pub struct SourceLockedExecutor {
    root: PathBuf,
}
fn collect_authoring_batches(
    scenarios: &[String],
    mut execute: impl FnMut(usize, &[String]) -> Result<Vec<Value>>,
) -> Result<Vec<Value>> {
    let mut checks = Vec::new();
    for (index, chunk) in scenarios.chunks(AUTHORING_BATCH_SCENARIOS).enumerate() {
        let batch = execute(index, chunk)?;
        ensure!(
            batch.len() == chunk.len(),
            "bootstrap_authoring_result_count"
        );
        for (check, scenario) in batch.iter().zip(chunk) {
            ensure!(
                check["execution"]["scenario_id"] == *scenario,
                "bootstrap_authoring_batch_scenario_differs"
            );
        }
        checks.extend(batch);
    }
    Ok(checks)
}
impl SourceLockedExecutor {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            root: SkillRoot::new(root)?.path().to_path_buf(),
        })
    }
    fn native(&self) -> Result<crate::refinery_runtime::RustRuntime> {
        crate::lifecycle::default_runtime(&self.root)
    }
}
impl BootstrapExecutor for SourceLockedExecutor {
    fn verify_runtime(&mut self) -> Result<()> {
        self.native()?.source()?;
        Ok(())
    }
    fn discover(&mut self, binding: Option<&Path>, workspace: Option<&Path>) -> Result<Value> {
        crate::lifecycle::discover(&self.root, binding, workspace)
    }
    fn invoke(&mut self, action: &str, request: &Invocation<'_>) -> Result<Value> {
        let result = match action {
            "open" => crate::lifecycle::open(
                &self.root,
                request.binding,
                request.task,
                request.workspace,
                None,
            ),
            "propose" => crate::lifecycle::propose(
                &self.root,
                request.binding,
                request.task,
                request.workspace,
                request.delta.context("bootstrap_delta_required")?,
                request
                    .engagement
                    .context("bootstrap_engagement_required")?,
            ),
            "commit" | "verify" | "promote" => crate::lifecycle::transition(
                &self.root,
                action,
                request.binding,
                request.task,
                request.workspace,
                request.candidate,
                request.authorization,
            ),
            _ => bail!("bootstrap_unknown_operation"),
        };
        // Lifecycle errors retain full native observations in its stable error
        // response. Persist that envelope before propagating the failure.
        match result {
            Ok(value) => Ok(value),
            Err(error) => {
                let path = request
                    .task
                    .parent()
                    .context("bootstrap_task_parent")?
                    .join(format!("native-failure-{}.json", unique()));
                retain(&path, &crate::lifecycle::error_response(action, &error))?;
                Err(error)
            }
        }
    }
    fn history(&mut self, binding: &Path, workspace: &Path, candidate: &str) -> Result<Value> {
        crate::lifecycle::history(&self.root, binding, workspace, candidate)
    }
    fn check_events(
        &mut self,
        binding_path: &Path,
        step_dir: &Path,
        events: &[Value],
        candidate: &str,
    ) -> Result<()> {
        let binding = crate::contracts::read_project_binding(binding_path)?;
        let native_binding = crate::contracts::native_binding(&binding)?;
        for event in events {
            let state = text(event, "state")?;
            let command = match state {
                "candidate" | "proposed" => "propose",
                "committed" => "commit",
                "regression_passed" | "release_complete" => "verify",
                "promoted" => "promote",
                _ => bail!("bootstrap_unknown_native_event"),
            };
            let task = crate::contracts::read_task_envelope(
                &step_dir.join(format!("task-{command}.json")),
                &binding,
            )?;
            crate::contracts::authorize_task(&binding, &task, command)?;
            let request = crate::native::TransitionRequest {
                registry: crate::native::RegistryRequest {
                    workspace: step_dir
                        .parent()
                        .context("bootstrap_step_parent")?
                        .join("registry"),
                    binding: native_binding.clone(),
                },
                delta_sha256: candidate.into(),
                envelope: crate::contracts::native_task(&task, state)?,
                recorded_at: None,
            };
            let result = crate::native::expected_transition_context(&request, state)?;
            ensure!(
                event["payload"]["transition_context_sha256"] == result["context_sha256"],
                "bootstrap_retained_task_differs_from_native_event"
            );
            if matches!(command, "commit" | "promote") {
                let projected = crate::delta::read_authorization(
                    &step_dir.join(format!("authorization-{command}.json")),
                    &binding,
                    command,
                    candidate,
                )?;
                let result = crate::delta::validate_native_authorization(&projected)?;
                ensure!(
                    event["payload"]["authorization_sha256"] == result["authorization_sha256"],
                    "bootstrap_retained_authorization_differs_from_native_event"
                );
            }
        }
        Ok(())
    }
    fn authoring_checks(&mut self, manifest: &Path, scenarios: &[String]) -> Result<Vec<Value>> {
        let parent = manifest.parent().context("bootstrap_manifest_parent")?;
        // Bootstrap batch limits are explicit. Every requested scenario is sent
        // exactly once; neither execution nor result lists are truncated.
        collect_authoring_batches(scenarios, |index, chunk| {
            eprintln!(
                "bootstrap authoring chunk {}: {} scenarios",
                index + 1,
                chunk.len()
            );
            let observation = self.native()?.run_manifest_batch(manifest, chunk, None);
            retain(
                &parent.join(format!("native-authoring-{}.json", unique())),
                &serde_json::to_value(&observation)?,
            )?;
            let result = observation.into_result()?;
            Ok(array(&result, "checks")?.clone())
        })
    }
}

pub fn prepare(root: &Path, directory: &Path, options: &PrepareOptions) -> Result<Value> {
    prepare_with_executor(
        root,
        directory,
        options,
        &mut SourceLockedExecutor::new(root)?,
    )
}
pub fn apply(root: &Path, directory: &Path, authorization: &Path) -> Result<Value> {
    apply_with_executor(
        root,
        directory,
        authorization,
        &mut SourceLockedExecutor::new(root)?,
    )
}
pub fn adopt(root: &Path, directory: &Path, authorization: &Path) -> Result<Value> {
    adopt_with_executor(
        root,
        directory,
        authorization,
        &mut SourceLockedExecutor::new(root)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }
    #[test]
    fn malformed_capsule_cannot_add_publisher_decisions_or_swap_identity() {
        let original = load_capsule(&root(), DEFAULT_BUNDLE).unwrap();
        let mut payload = original.payload.clone();
        payload.insert("publisher-authorization.json".into(), b"{}".to_vec());
        assert!(validate_capsule(&original.spec, &payload, &original.capsule).is_err());
        for field in [
            "package_id",
            "target_version",
            "target_package_sha256",
            "runtime",
            "native_source_authority",
        ] {
            let mut capsule = original.capsule.clone();
            capsule[field] = json!("changed");
            assert!(
                validate_capsule(&original.spec, &original.payload, &capsule).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn malformed_capsule_paths_missing_sources_and_asset_bytes_are_rejected() {
        let original = load_capsule(&root(), DEFAULT_BUNDLE).unwrap();
        for field in ["delta_path", "manifest_path", "scenario_registry"] {
            let mut capsule = original.capsule.clone();
            capsule["steps"][0][field] = json!("../escape.json");
            assert!(
                validate_capsule(&original.spec, &original.payload, &capsule).is_err(),
                "{field}"
            );
        }
        let mut payload = original.payload.clone();
        let source = payload
            .keys()
            .find(|name| name.starts_with("sources/"))
            .unwrap()
            .clone();
        payload.remove(&source);
        assert!(validate_capsule(&original.spec, &payload, &original.capsule).is_err());
        let mut payload = original.payload.clone();
        let asset = payload
            .keys()
            .find(|name| name.starts_with("steps/000/assets/"))
            .unwrap()
            .clone();
        payload.insert(asset, b"changed bytes".to_vec());
        assert!(validate_capsule(&original.spec, &payload, &original.capsule).is_err());
    }
    #[test]
    fn malformed_capsule_chain_duplicate_versions_and_delta_hash_are_rejected() {
        let original = load_capsule(&root(), DEFAULT_BUNDLE).unwrap();
        let mut capsule = original.capsule.clone();
        capsule["steps"].as_array_mut().unwrap().swap(0, 1);
        assert!(validate_capsule(&original.spec, &original.payload, &capsule).is_err());
        let mut capsule = original.capsule.clone();
        capsule["steps"][1]["version"] = capsule["steps"][0]["version"].clone();
        assert!(validate_capsule(&original.spec, &original.payload, &capsule).is_err());
        let mut payload = original.payload.clone();
        let path = text(&original.capsule["steps"][0], "delta_path").unwrap();
        let mut delta = payload_json(&payload, path).unwrap();
        delta["rationale"] = json!("tampered source");
        payload.insert(path.into(), encoded(&delta).unwrap());
        assert!(validate_capsule(&original.spec, &payload, &original.capsule).is_err());
    }
    #[test]
    fn empty_native_baseline_hash_matches_shared_contract() {
        assert_eq!(crate::delta::empty_package_sha256().unwrap(), EMPTY);
    }
    #[test]
    fn streamed_all_checks_hash_matches_exact_original_pretty_bytes() {
        for checks in [
            vec![],
            vec![json!({})],
            vec![json!([]), json!([1, 2])],
            vec![
                json!({"nested":[{"a":[],"b":{}}],"string":"Unicode 中文\nquote\""}),
                strict_json(
                    b"{\"number\":1e-7,\"integer\":10000000000000000000000000000000000000000}",
                )
                .unwrap(),
            ],
        ] {
            assert_eq!(
                checks_sha256(&checks).unwrap(),
                sha256(encoded(&json!(checks)).unwrap())
            );
        }
    }
    #[test]
    fn retained_shards_preserve_more_than_one_read_limit_without_a_monolithic_file() {
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let scratch = Scratch(root().join(format!(
            "var/maintenance/rust-tests/bootstrap-large-shards-{}",
            unique()
        )));
        create_new_private_dir(&scratch.0).unwrap();
        // Only opaque fixture output, with no supplied PASS or semantic claim.
        let checks=(0..55).map(|i|json!({"execution":{"scenario_id":format!("case-{i}"),"fixture_output":"x".repeat(5*1024*1024/4)}})).collect::<Vec<_>>();
        retain_checks(&scratch.0, "authoring-checks", &checks).unwrap();
        let index_bytes =
            read_regular_bytes(&scratch.0.join("authoring-checks.json"), LIMIT).unwrap();
        assert!(index_bytes.len() < 64 * 1024);
        let index = strict_json(&index_bytes).unwrap();
        let mut total = 0;
        let mut count = 0;
        for part in index["parts"].as_array().unwrap() {
            let bytes =
                read_regular_bytes(&scratch.0.join(part["path"].as_str().unwrap()), LIMIT).unwrap();
            assert_eq!(part["sha256"], sha256(&bytes));
            total += bytes.len();
            count += strict_json(&bytes).unwrap().as_array().unwrap().len();
        }
        assert!(total > LIMIT);
        assert_eq!(count, 55);
        assert_eq!(index["checks_sha256"], checks_sha256(&checks).unwrap());
    }
    #[test]
    fn full_large_reports_are_partitioned_below_the_unchanged_native_output_cap() {
        let scenarios = (0..55).map(|i| format!("scenario-{i}")).collect::<Vec<_>>();
        let report = |scenario: &String| json!({"execution":{"scenario_id":scenario,"fixture_report_bytes":"x".repeat(192*1024)}});
        let full = scenarios.iter().map(report).collect::<Vec<_>>();
        assert!(
            serde_json::to_vec(&json!({"checks":full})).unwrap().len()
                > crate::native::MAX_NATIVE_BYTES
        );
        let mut calls = Vec::new();
        let checked = collect_authoring_batches(&scenarios, |_, chunk| {
            assert!(chunk.len() <= 4 && chunk.len() <= 64);
            let result = chunk.iter().map(report).collect::<Vec<_>>();
            ensure!(
                serde_json::to_vec(&json!({"checks":result}))?.len()
                    <= crate::native::MAX_NATIVE_BYTES,
                "native_process_output_limit"
            );
            calls.extend_from_slice(chunk);
            Ok(result)
        })
        .unwrap();
        assert_eq!(calls, scenarios);
        assert_eq!(checked.len(), 55);
        assert_eq!(checked, full);
    }
    #[test]
    fn smaller_batches_keep_output_failure_explicit_without_sending_later_scenarios() {
        let scenarios = (0..9).map(|i| format!("scenario-{i}")).collect::<Vec<_>>();
        let mut submitted = Vec::new();
        let error = collect_authoring_batches(&scenarios, |index, chunk| {
            submitted.extend_from_slice(chunk);
            if index == 1 {
                bail!("native_process_output_limit");
            }
            Ok(chunk
                .iter()
                .map(|id| json!({"execution":{"scenario_id":id}}))
                .collect())
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "native_process_output_limit");
        assert_eq!(submitted, scenarios[..8]);
    }
}
