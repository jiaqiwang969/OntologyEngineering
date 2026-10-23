//! Explicit-directory PDF intake and stored-text coverage audit. No source mutation.
use crate::sources::Provider;
use anyhow::{Context, Result, ensure};
use oe_contracts::PdfLibraryAuditReport;
use oe_core::{FileIdentity, SkillRoot, fingerprint_file, no_symlinks};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

pub const SCHEMA: &str = "ontology-engineering.pdf-library-audit/v1";
pub const MAX_DEPTH: usize = 8;
pub const MAX_ENTRIES: usize = 4096;
pub const MAX_PDF_FILES: usize = 512;
pub const MAX_HASH_BYTES: u64 = 8 * 1024 * 1024 * 1024;

pub(crate) fn relative(root: &SkillRoot, path: &Path) -> Result<String> {
    let mut names = Vec::new();
    for part in path
        .strip_prefix(root.path())
        .context("source_audit_path_outside_root")?
        .components()
    {
        let Component::Normal(name) = part else {
            anyhow::bail!("source_audit_path_invalid")
        };
        names.push(name.to_str().context("source_audit_path_not_utf8")?);
    }
    ensure!(!names.is_empty(), "source_audit_path_invalid");
    Ok(names.join("/"))
}

fn same_metadata(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    let same = a.file_type() == b.file_type()
        && a.len() == b.len()
        && a.modified().ok().is_some()
        && a.modified().ok() == b.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        same && a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        same
    }
}

struct Observation {
    identity: Option<FileIdentity>,
    value: Value,
}
struct Scan<'a> {
    root: &'a SkillRoot,
    registered: BTreeMap<PathBuf, Vec<String>>,
    index_ready: bool,
    files: Vec<Value>,
    issues: Vec<Value>,
    directories: Vec<(PathBuf, fs::Metadata)>,
    observations: BTreeMap<PathBuf, Observation>,
    entries: usize,
    pdf_files: usize,
    hash_bytes: u64,
    reserved_bytes: u64,
}

impl Scan<'_> {
    fn issue(&mut self, path: &Path, reason: &str) {
        self.issues
            .push(json!({"path":relative(self.root,path).ok(),"reason":reason}));
    }

    fn observe(&mut self, path: &Path) {
        if self.observations.contains_key(path) {
            return;
        }
        if self.observations.len() >= MAX_PDF_FILES {
            self.issue(path, "pdf_file_count_budget_exhausted");
            self.observations.insert(path.to_owned(), Observation {
                identity: None,
                value: json!({"status":"unavailable","reason":"pdf_file_count_budget_exhausted"}),
            });
            return;
        }
        let outcome = (|| -> Result<Observation> {
            no_symlinks(path)?;
            let metadata = match fs::symlink_metadata(path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Observation {
                        identity: None,
                        value: json!({"status":"missing"}),
                    });
                }
                result => result?,
            };
            ensure!(metadata.is_file(), "source_audit_original_not_regular");
            let bytes = metadata.len();
            ensure!(
                bytes <= crate::citations::MAX_SOURCE_BYTES,
                "source_audit_file_size_budget"
            );
            let reservation = bytes.checked_mul(2).context("source_audit_hash_budget")?;
            ensure!(
                reservation
                    <= MAX_HASH_BYTES
                        .saturating_sub(self.hash_bytes)
                        .saturating_sub(self.reserved_bytes),
                "source_audit_hash_budget"
            );
            self.hash_bytes += bytes;
            let identity = fingerprint_file(path, bytes)?;
            let after = fs::symlink_metadata(path)?;
            ensure!(
                same_metadata(&metadata, &after),
                "source_audit_original_changed_during_hash"
            );
            self.reserved_bytes += bytes;
            Ok(Observation {
                value: json!({"status":"verified","sha256":identity.sha256,"bytes":identity.bytes}),
                identity: Some(identity),
            })
        })();
        let observation = match outcome {
            Ok(observation) => observation,
            Err(error) => {
                let reason = match error.to_string().as_str() {
                    "source_audit_file_size_budget" => "file_size_budget_exhausted",
                    "source_audit_hash_budget" => "hash_bytes_budget_exhausted",
                    "source_audit_original_not_regular" => "original_not_regular",
                    _ => "original_unreadable_unsafe_or_changed",
                };
                self.issue(path, reason);
                Observation {
                    identity: None,
                    value: json!({"status":"unavailable","reason":reason}),
                }
            }
        };
        self.observations.insert(path.to_owned(), observation);
    }

    fn walk(&mut self, path: &Path, depth: usize) {
        if depth > MAX_DEPTH {
            self.issue(path, "directory_depth_budget_exhausted");
            return;
        }
        if self.entries >= MAX_ENTRIES {
            self.issue(path, "directory_entry_budget_exhausted");
            return;
        }
        let read = (|| -> Result<(fs::Metadata, Vec<PathBuf>, bool)> {
            no_symlinks(path)?;
            let before = fs::symlink_metadata(path)?;
            ensure!(before.is_dir(), "source_audit_directory_not_regular");
            let mut entries = Vec::new();
            let mut truncated = false;
            for entry in fs::read_dir(path)? {
                if entries.len() >= MAX_ENTRIES.saturating_sub(self.entries) {
                    truncated = true;
                    break;
                }
                entries.push(entry?.path());
            }
            no_symlinks(path)?;
            ensure!(
                same_metadata(&before, &fs::symlink_metadata(path)?),
                "source_audit_directory_changed"
            );
            entries.sort();
            Ok((before, entries, truncated))
        })();
        let (metadata, entries, truncated) = match read {
            Ok(value) => value,
            Err(_) => {
                self.issue(path, "directory_unreadable_unsafe_or_changed");
                return;
            }
        };
        self.directories.push((path.to_owned(), metadata));
        if truncated {
            self.issue(path, "directory_entry_budget_exhausted");
        }
        for child in entries {
            if self.entries >= MAX_ENTRIES {
                self.issue(path, "directory_entry_budget_exhausted");
                break;
            }
            self.entries += 1;
            let name = match relative(self.root, &child) {
                Ok(name) => name,
                Err(_) => {
                    self.issue(path, "entry_path_not_utf8");
                    continue;
                }
            };
            let metadata = fs::symlink_metadata(&child);
            let mut row = json!({"path":name,"format_validation":"not_run","source_ids":[],"registration":"not_applicable","original":{"status":"not_checked"}});
            match metadata {
                Ok(m) if m.file_type().is_symlink() => {
                    row["kind"] = json!("symlink");
                    row["original"] =
                        json!({"status":"unavailable","reason":"symlink_not_followed"});
                    self.issue(&child, "symlink_not_followed");
                }
                Ok(m) if m.is_dir() => {
                    self.walk(&child, depth + 1);
                    continue;
                }
                Ok(m) if m.is_file() => {
                    let ids = self.registered.get(&child).cloned().unwrap_or_default();
                    let pdf = child
                        .extension()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
                        || !ids.is_empty();
                    row["kind"] = json!(if pdf { "pdf" } else { "non_pdf" });
                    row["bytes"] = json!(m.len());
                    if pdf {
                        self.pdf_files += 1;
                        row["registration"] = json!(if !self.index_ready {
                            "unknown"
                        } else if ids.is_empty() {
                            "unregistered"
                        } else {
                            "registered"
                        });
                        row["source_ids"] = json!(ids);
                        if self.pdf_files <= MAX_PDF_FILES {
                            self.observe(&child);
                        } else {
                            self.issue(&child, "pdf_file_count_budget_exhausted");
                            row["original"] = json!({"status":"unavailable","reason":"pdf_file_count_budget_exhausted"});
                        }
                    }
                }
                Ok(_) => {
                    row["kind"] = json!("special");
                    row["original"] =
                        json!({"status":"unavailable","reason":"special_file_not_opened"});
                    self.issue(&child, "special_file_not_opened");
                }
                Err(_) => {
                    row["kind"] = json!("unreadable");
                    row["original"] =
                        json!({"status":"unavailable","reason":"entry_metadata_unavailable"});
                    self.issue(&child, "entry_metadata_unavailable");
                }
            }
            self.files.push(row);
        }
    }

    fn recheck(&mut self) {
        let paths = self.observations.keys().cloned().collect::<Vec<_>>();
        for path in paths {
            let observed = &self.observations[&path];
            let mismatch = if let Some(identity) = &observed.identity {
                self.hash_bytes += identity.bytes;
                self.reserved_bytes -= identity.bytes;
                !fingerprint_file(&path, identity.bytes).is_ok_and(|current| current == *identity)
            } else if observed.value["status"] == "missing" {
                !matches!(fs::symlink_metadata(&path),Err(error) if error.kind()==std::io::ErrorKind::NotFound)
            } else {
                false
            };
            if mismatch {
                self.issue(&path, "original_changed_during_audit");
                let observation = self.observations.get_mut(&path).expect("known observation");
                observation.identity = None;
                observation.value =
                    json!({"status":"unavailable","reason":"original_changed_during_audit"});
            }
        }
        let changed = self
            .directories
            .iter()
            .filter_map(|(path, metadata)| {
                let same = no_symlinks(path).is_ok()
                    && fs::symlink_metadata(path).is_ok_and(|now| same_metadata(metadata, &now));
                (!same).then_some(path.clone())
            })
            .collect::<Vec<_>>();
        for path in changed {
            self.issue(&path, "directory_changed_during_audit");
        }
    }
}

/// Audit one explicitly selected sources subtree against one explicit PDF index.
/// Temporary provider snapshots are cleaned by the existing read-only index owner.
pub fn audit(root: &SkillRoot, directory: &Path, index: &Path) -> Result<Value> {
    let directory = root.expand(directory)?;
    no_symlinks(&directory)?;
    ensure!(
        directory.starts_with(root.path().join("sources"))
            && directory != root.path().join("sources")
            && !directory.starts_with(root.path().join("sources/.indexes")),
        "source_audit_directory_outside_source_subtree"
    );
    ensure!(
        fs::symlink_metadata(&directory)?.is_dir(),
        "source_audit_directory_not_directory"
    );
    let directory_name = relative(root, &directory)?;
    let index = root.expand(index)?;
    ensure!(
        index.starts_with(root.path().join("sources/.indexes"))
            && index != root.path().join("sources/.indexes"),
        "source_audit_index_outside_registered_sources"
    );
    let index_name = relative(root, &index)?;
    let provider = Provider::open(root, "local_pdf", Some(&index)).ok();
    let descriptors = provider.as_ref().and_then(|p| p.audit_sources().ok());
    let observed_identity = provider.as_ref().and_then(|p| p.audit_identity().ok());
    let ready = descriptors.is_some() && observed_identity.is_some();
    let all = if ready {
        descriptors.unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut index_report =
        json!({"path":index_name,"status":if ready {"ready"} else {"unavailable"}});
    if let Some(identity) = observed_identity {
        index_report["index_identity"] = identity["index_identity"].clone();
        index_report["registration_identity"] = identity["registration_identity"].clone();
    }
    if !ready {
        index_report["reason"] = json!("registered_index_missing_invalid_or_unavailable");
    }
    let selected = all
        .iter()
        .filter(|(_, path)| path.starts_with(&directory))
        .collect::<Vec<_>>();
    let mut registered: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for (source, path) in &selected {
        registered.entry(path.clone()).or_default().push(
            source["id"]
                .as_str()
                .context("source_audit_id_invalid")?
                .to_owned(),
        );
    }
    let mut scan = Scan {
        root,
        registered,
        index_ready: ready,
        files: vec![],
        issues: vec![],
        directories: vec![],
        observations: BTreeMap::new(),
        entries: 0,
        pdf_files: 0,
        hash_bytes: 0,
        reserved_bytes: 0,
    };
    scan.walk(&directory, 0);
    let mut sources = Vec::new();
    for (source, path) in &selected {
        // Missing originals are not discovered by directory traversal, but remain
        // part of the explicitly selected registered source inventory.
        scan.observe(path);
        let coverage = match provider
            .as_ref()
            .expect("selected implies provider")
            .audit_coverage(source)
        {
            Ok(value) => value,
            Err(_) => {
                json!({"status":"unavailable","reason":"stored_page_sequence_identity_or_text_invalid"})
            }
        };
        sources.push(json!({"id":source["id"],"title":source["title"],"edition":source["edition"],"path":relative(root,path)?,"expected_sha256":source["sha256"],"expected_bytes":source["bytes"],"expected_pages":source["page_count"],"registration":"registered","coverage":coverage}));
    }
    scan.recheck();
    let final_ready = ready
        && provider
            .as_ref()
            .is_some_and(|p| p.verify_snapshot().is_ok());
    if ready && !final_ready {
        index_report["status"] = json!("unavailable");
        index_report["reason"] = json!("registered_index_changed_during_audit");
    }
    for source in &mut sources {
        let path = root
            .path()
            .join(source["path"].as_str().expect("source path"));
        let observation = &scan.observations[&path];
        source["original"] = observation.value.clone();
        if let Some(identity) = &observation.identity {
            if source["expected_sha256"] != identity.sha256
                || source["expected_bytes"].as_u64() != Some(identity.bytes)
            {
                source["original"]["status"] = json!("changed");
            }
        }
        if !final_ready {
            source["registration"] = json!("unknown");
            source["coverage"] =
                json!({"status":"unavailable","reason":"registered_index_changed_during_audit"});
        }
    }
    let mut groups: BTreeMap<(String, u64), Vec<String>> = BTreeMap::new();
    for row in &mut scan.files {
        let path = root.path().join(row["path"].as_str().expect("file path"));
        if let Some(observation) = scan.observations.get(&path) {
            row["original"] = observation.value.clone();
            if row["original"]["status"] == "verified"
                && sources.iter().any(|source| {
                    source["path"] == row["path"] && source["original"]["status"] == "changed"
                })
            {
                row["original"]["status"] = json!("changed");
            }
            if let Some(identity) = &observation.identity {
                groups
                    .entry((identity.sha256.clone(), identity.bytes))
                    .or_default()
                    .push(row["path"].as_str().expect("file path").to_owned());
            }
        }
        if row["kind"] == "pdf" && !final_ready {
            row["registration"] = json!("unknown");
            row["source_ids"] = json!([]);
        }
    }
    let duplicates = groups.into_iter().filter(|(_,paths)|paths.len()>1).map(|((sha256,bytes),paths)|json!({"sha256":sha256,"bytes":bytes,"paths":paths,"meaning":"Same observed bytes in this scanned scope only; no deletion, registration or edition inference."})).collect::<Vec<_>>();
    let count = |key: &str, value: &str| scan.files.iter().filter(|f| f[key] == value).count();
    let summary = json!({"scan_status":if scan.issues.is_empty() {"complete"} else {"partial"},"issues":scan.issues,
        "entries_examined":scan.entries,"pdf_files":count("kind","pdf"),"non_pdf_files":count("kind","non_pdf"),
        "unregistered_pdf_files":count("registration","unregistered"),"unknown_registration_pdf_files":count("registration","unknown"),
        "registered_sources_in_scope":if final_ready {json!(sources.len())} else {Value::Null},
        "index_registered_sources":if final_ready {json!(all.len())} else {Value::Null},
        "outside_scope_registered_sources":if final_ready {json!(all.len()-selected.len())} else {Value::Null},
        "hashed_bytes_including_recheck":scan.hash_bytes,
        "budgets":{"max_depth":MAX_DEPTH,"max_entries":MAX_ENTRIES,"max_pdf_files":MAX_PDF_FILES,"max_hash_bytes":MAX_HASH_BYTES},
        "pdf_detection":"case-insensitive .pdf extension or an explicitly registered original path; PDF validity not examined",
        "coverage_scope":"only stored pages of registered sources whose original paths are in the selected directory"});
    let report = PdfLibraryAuditReport { schema:SCHEMA.into(),status:"audited".into(),directory:directory_name,index:index_report,
        files:scan.files,sources,duplicates,summary,
        meaning:"Read-only observations in the explicitly selected directory and index. Audited does not mean complete intake, verified PDF format, current publication, OCR coverage, source correctness or engineering adoption. Original verification is byte integrity only; missing/changed originals and stored-text coverage are independent.".into() };
    ensure!(
        report.validate().is_ok(),
        "source_audit_report_contract_invalid"
    );
    Ok(serde_json::to_value(report)?)
}
