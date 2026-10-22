//! Portable source-delivery manifest handling.
//!
//! This is deliberately separate from supplier source-pack installation: the
//! delivery manifest covers every declared original (books and supplier data),
//! while registration only hands declared books to the existing local-PDF
//! indexer. Network access is never implicit.
use crate::{registration, source_pack};
use anyhow::{Context, Result, bail, ensure};
use oe_core::{SkillRoot, json::strict_json};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const SCHEMA: &str = "ontology-engineering.source-delivery/v1";
const MAX_MANIFEST: usize = 16 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024 * 1024;

pub struct FetchOptions {
    pub executable: PathBuf,
    pub timeout: Duration,
}
impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            executable: "gws".into(),
            timeout: Duration::from_secs(1800),
        }
    }
}

struct Delivery {
    path: PathBuf,
    raw: Vec<u8>,
    value: Value,
}
impl Delivery {
    fn verify_current(&self) -> Result<()> {
        ensure!(
            oe_core::read_regular_bytes(&self.path, MAX_MANIFEST)? == self.raw,
            "source_library_manifest_changed"
        );
        Ok(())
    }
}

fn relative(value: &str) -> Result<&Path> {
    source_pack::relative_path(value).context("source_library_invalid_relative_path")
}

fn root_path(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        root.path().join(path)
    };
    ensure!(
        path.starts_with(root.path()) && path != root.path(),
        "source_root_must_be_inside_skill"
    );
    oe_core::no_symlinks(&path)?;
    Ok(path)
}

fn file_entry(value: &Value) -> Result<()> {
    let path = value["relative_path"]
        .as_str()
        .context("source_library_path_missing")?;
    relative(path)?;
    let hash = value["sha256"]
        .as_str()
        .context("source_library_hash_missing")?;
    ensure!(source_pack::valid_sha(hash), "source_library_hash_invalid");
    let bytes = value["bytes"]
        .as_u64()
        .context("source_library_bytes_missing")?;
    ensure!(bytes <= MAX_SOURCE_BYTES, "source_library_bytes_invalid");
    ensure!(
        value.get("drive").is_none_or(Value::is_object),
        "source_library_drive_invalid"
    );
    ensure!(
        value.get("registration").is_none_or(Value::is_object),
        "source_library_registration_invalid"
    );
    Ok(())
}

fn load(root: &SkillRoot, manifest: &Path) -> Result<Delivery> {
    let path = root_path(root, manifest)?;
    let raw = oe_core::read_regular_bytes(&path, MAX_MANIFEST)?;
    let document = strict_json(&raw)?;
    ensure!(
        document["schema"] == SCHEMA,
        "source_library_manifest_schema"
    );
    let entries = document["files"]
        .as_array()
        .context("source_library_files_missing")?;
    ensure!(
        !entries.is_empty() && entries.len() <= 4096,
        "source_library_empty_manifest"
    );
    let mut paths = BTreeSet::new();
    for entry in entries {
        file_entry(entry)?;
        ensure!(
            paths.insert(entry["relative_path"].as_str().unwrap()),
            "source_library_duplicate_path"
        );
    }
    Ok(Delivery {
        path,
        raw,
        value: document,
    })
}

fn selected(document: &Value, selection: Option<&str>) -> Result<Vec<Value>> {
    let selection = selection.unwrap_or("all");
    let entries = document["files"]
        .as_array()
        .context("source_library_files_missing")?;
    let result = entries
        .iter()
        .filter(|entry| {
            let path = entry["relative_path"].as_str().unwrap_or("");
            selection == "all"
                || selection == "books" && path.starts_with("books/")
                || selection == "misumi" && path.starts_with("misumi/")
                || entry["registration"]["id"].as_str() == Some(selection)
                || path == selection
        })
        .cloned()
        .collect::<Vec<_>>();
    ensure!(!result.is_empty(), "source_library_unknown_selection");
    Ok(result)
}

fn destination(root: &SkillRoot, source_root: &Path, entry: &Value) -> Result<PathBuf> {
    let rel = relative(
        entry["relative_path"]
            .as_str()
            .context("source_library_path_missing")?,
    )?;
    let path = root_path(root, &source_root.join(rel))?;
    ensure!(
        path.starts_with(source_root),
        "source_library_destination_outside_sources"
    );
    Ok(path)
}

fn status(path: &Path, entry: &Value) -> Result<&'static str> {
    oe_core::no_symlinks(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok("missing"),
        Err(e) => return Err(e.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok("not_regular_file");
    }
    let expected = entry["bytes"]
        .as_u64()
        .context("source_library_bytes_missing")?;
    if metadata.len() != expected {
        return Ok("size_mismatch");
    }
    let (hash, bytes) = source_pack::file_hash(path, expected)?;
    if bytes != expected {
        return Ok("size_mismatch");
    }
    Ok(if hash == entry["sha256"].as_str().unwrap() {
        "verified"
    } else {
        "sha256_mismatch"
    })
}

pub fn list(
    root: &SkillRoot,
    manifest: &Path,
    selection: Option<&str>,
    verify: bool,
) -> Result<Value> {
    let document = load(root, manifest)?;
    let source_root = root.path().join("sources");
    let mut files = Vec::new();
    let entries = selected(&document.value, selection)?;
    for entry in &entries {
        let path = destination(root, &source_root, entry)?;
        let state = if verify {
            status(&path, entry)?
        } else if path.is_file() {
            "present_unverified"
        } else {
            "missing"
        };
        files.push(json!({"relative_path":entry["relative_path"],"bytes":entry["bytes"],"status":state,"path":path,"download_url":entry["drive"]["view_url"],"book_id":entry["registration"]["id"]}));
    }
    let ready = files.iter().all(|x| x["status"] == "verified");
    document.verify_current()?;
    Ok(
        json!({"status":if verify {if ready {"verified"} else {"not_ready"}} else {"listed"},"sources_root":source_root,"files":files,"bytes":entries.iter().map(|e|e["bytes"].as_u64().unwrap_or(0)).sum::<u64>(),"network_requests":0}),
    )
}

fn copy_verified(source: &Path, stage: &Path, entry: &Value) -> Result<()> {
    let expected = entry["bytes"]
        .as_u64()
        .context("source_library_bytes_missing")?;
    let mut input = oe_core::open_regular_file(source, expected)?;
    let before = input.metadata()?;
    ensure!(
        before.len() == expected,
        "source_library_download_identity_mismatch"
    );
    let mut out = oe_core::create_new_regular_file(stage)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        ensure!(
            count <= expected,
            "source_library_download_identity_mismatch"
        );
        hash.update(&buffer[..n]);
        out.write_all(&buffer[..n])?;
    }
    out.sync_all()?;
    let after = input.metadata()?;
    ensure!(
        count == expected
            && format!("{:x}", hash.finalize()) == entry["sha256"].as_str().unwrap()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "source_library_download_identity_mismatch"
    );
    ensure!(
        status(source, entry)? == "verified" && status(stage, entry)? == "verified",
        "source_library_download_identity_mismatch"
    );
    Ok(())
}

fn download(
    root: &SkillRoot,
    id: &str,
    stage: &source_pack::Stage,
    options: &FetchOptions,
) -> Result<PathBuf> {
    let params = serde_json::to_string(&json!({"fileId":id,"alt":"media"}))?;
    let temporary = stage.path.join("download");
    let mut arguments = ["drive", "files", "get", "--params", &params, "--output"]
        .iter()
        .map(std::ffi::OsString::from)
        .collect::<Vec<_>>();
    arguments.push(temporary.clone().into_os_string());
    // Authentication stays with gws. Suppress persisted/returned raw streams;
    // use the shared process-tree deadline and bound in-memory diagnostics.
    let process = oe_core::process::run_command_memory(
        &oe_core::process::CommandSpec {
            executable: options.executable.clone(),
            arguments,
            cwd: root.path().to_owned(),
            environment: Default::default(),
            remove_environment: Vec::new(),
            timeout: options.timeout,
            output_limit: 64 * 1024,
        },
        &stage.path.join("process"),
    )
    .map_err(|_| anyhow::anyhow!("source_library_gws_unavailable"))?;
    if process.status == "timed_out" {
        bail!("source_library_gws_download_timeout");
    }
    ensure!(
        process.status == "completed" && process.exit_code == Some(0),
        "source_library_gws_download_failed_check_login_access_and_network"
    );
    Ok(temporary)
}

pub fn import(
    root: &SkillRoot,
    manifest: &Path,
    selection: Option<&str>,
    incoming: &Path,
) -> Result<Value> {
    place(
        root,
        manifest,
        selection,
        Some(incoming),
        &FetchOptions::default(),
    )
}

pub fn fetch(
    root: &SkillRoot,
    manifest: &Path,
    selection: &str,
    options: &FetchOptions,
) -> Result<Value> {
    ensure!(
        !selection.is_empty(),
        "source_library_fetch_selection_required"
    );
    ensure!(
        options.timeout > Duration::ZERO && options.timeout <= Duration::from_secs(1800),
        "source_library_download_timeout_invalid"
    );
    place(root, manifest, Some(selection), None, options)
}

fn place(
    root: &SkillRoot,
    manifest: &Path,
    selection: Option<&str>,
    incoming: Option<&Path>,
    options: &FetchOptions,
) -> Result<Value> {
    let source_root = root_path(root, Path::new("sources"))?;
    let document = load(root, manifest)?;
    let entries = selected(&document.value, selection)?;
    let incoming = incoming
        .map(|path| {
            if path.is_absolute() {
                Ok(path.to_owned())
            } else {
                std::env::current_dir().map(|dir| dir.join(path))
            }
        })
        .transpose()?;
    if let Some(path) = &incoming {
        oe_core::no_symlinks(path)?;
    }
    let mut reports = Vec::new();
    let mut names = BTreeSet::new();
    for entry in &entries {
        ensure!(
            names.insert(
                Path::new(entry["relative_path"].as_str().unwrap())
                    .file_name()
                    .unwrap()
                    .to_owned()
            ),
            "source_library_ambiguous_download_names"
        );
    }
    for entry in entries {
        let target = destination(root, &source_root, &entry)?;
        if fs::symlink_metadata(&target).is_ok() {
            ensure!(
                status(&target, &entry)? == "verified",
                "source_library_existing_file_mismatch"
            );
            reports
                .push(json!({"relative_path":entry["relative_path"],"status":"already_verified"}));
            continue;
        }
        let stage = source_pack::Stage::new(root.path(), "source-delivery")?;
        let source = if let Some(incoming) = &incoming {
            let rel = Path::new(entry["relative_path"].as_str().unwrap());
            let flat = incoming.join(rel.file_name().unwrap());
            let nested = incoming.join(rel);
            oe_core::no_symlinks(&nested)?;
            if fs::symlink_metadata(&nested).is_ok() {
                nested
            } else {
                flat
            }
        } else {
            let id = entry["drive"]["file_id"]
                .as_str()
                .context("source_library_drive_id_missing")?;
            ensure!(
                id.len() >= 10
                    && id.len() <= 200
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
                "source_library_drive_id_invalid"
            );
            download(root, id, &stage, options)?
        };
        let verified = stage.path.join("verified");
        copy_verified(&source, &verified, &entry)?;
        document.verify_current()?;
        source_pack::publish_same_or_new(
            &verified,
            &target,
            entry["sha256"].as_str().unwrap(),
            entry["bytes"].as_u64().unwrap(),
        )?;
        reports.push(json!({"relative_path":entry["relative_path"],"status":"verified"}));
    }
    document.verify_current()?;
    Ok(
        json!({"status":"placed","files":reports,"sources_root":source_root,"network_download":if incoming.is_some(){"not_run"}else{"gws_explicit_fetch"},"originals_modified":false}),
    )
}

pub fn register(
    root: &SkillRoot,
    manifest: &Path,
    selection: Option<&str>,
    replace: bool,
    tools: crate::poppler::PopplerTools,
) -> Result<Value> {
    let source_root = root_path(root, Path::new("sources"))?;
    let document = load(root, manifest)?;
    let entries = selected(&document.value, Some(selection.unwrap_or("books")))?;
    ensure!(
        entries.iter().all(|e| e["registration"].is_object()),
        "source_library_register_select_books_only"
    );
    for entry in &entries {
        ensure!(
            status(&destination(root, &source_root, entry)?, entry)? == "verified",
            "source_library_books_not_verified"
        );
    }
    let stage = source_pack::Stage::new(root.path(), "delivery-registration")?;
    let manifest_path = stage.path.join("manifest.json");
    let sources = entries.iter().map(|e| Ok(json!({"id":e["registration"]["id"],"title":e["title"],"edition":e["registration"]["edition"],"path":destination(root,&source_root,e)?,"sha256":e["sha256"],"chapters":e["registration"]["chapters"]}))).collect::<Result<Vec<_>>>()?;
    oe_core::write_new(
        &manifest_path,
        &json!({"schema":"ontology-engineering.local-pdf-manifest/v1","sources":sources}),
    )?;
    document.verify_current()?;
    registration::register(
        root.path(),
        &manifest_path,
        &registration::RegistrationOptions {
            data_root: "sources/.indexes/local-pdf".into(),
            replace,
            tools,
            limits: Default::default(),
        },
    )
}
