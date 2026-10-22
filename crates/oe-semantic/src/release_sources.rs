//! Byte-closed book package inventories and read-only Git source proofs.
//! These are provenance checks, never a second semantic engine or publication gate.
use crate::{
    book_release,
    runtime::{RuntimeSourceLock, VerifiedWheel, read_wheel_identity},
    safe_relative, strict_json,
};
use anyhow::{Context, Result, ensure};
use oe_core::process::{CommandSpec, run_command};
use oe_core::{
    SkillRoot, create_new_private_dir, fingerprint_file, no_symlinks, read_regular_bytes, sha256,
    write_new, write_new_bytes,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const LOCK_LIMIT: usize = 8 * 1024 * 1024;
const CONTINUITY_PATH: &str = "sources/continuity.json";
const SOURCE_FILE_LIMIT: u64 = 128 * 1024 * 1024;
const SOURCE_TOTAL_LIMIT: u64 = 512 * 1024 * 1024;
const SOURCE_FILE_COUNT: usize = 20_000;
const VOL1_LOCK: &str = "references/ontology-engineering-book/authoring-sources.sha256";
const RETIRED_VOL1_LOCK: &str =
    "references/ontology-engineering-book/handbook/authoring-sources.sha256";
const VOL2_LOCKS: [&str; 3] = [
    "references/product-trustworthiness-book/handbook/authoring-assets.sha256",
    "references/product-trustworthiness-book/handbook/current-source.sha256",
    "references/product-trustworthiness-book/handbook/formal-search-guides.sha256",
];
const LEGACY_GENERATED: [&str; 13] = [
    "references/book-release-artifacts.json",
    "references/book-release-artifacts.json.sha256",
    "references/release-evidence/authoring-locks.json",
    "references/release-evidence/book-source-bindings.json",
    "references/release-evidence/ontology-engineering-regression.json",
    "references/release-evidence/pdf-qa.json",
    "references/release-evidence/privacy.json",
    "references/release-evidence/runtime-identity.json",
    "references/release-evidence/semantica-regression.json",
    "references/release-evidence/publication-approval.json",
    "references/release-evidence/rights.json",
    "references/release-evidence/ontology-engineering-regression.log",
    "references/release-evidence/semantica-regression.log",
];
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    let value = value
        .get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("release_missing_string:{name}"))?;
    ensure!(!value.is_empty(), "release_empty_string:{name}");
    Ok(value)
}
fn reference(path: &str, bytes: &[u8]) -> Value {
    json!({"path":path,"sha256":sha256(bytes),"size_bytes":bytes.len()})
}
fn has_registered_archive(root: &SkillRoot) -> Result<bool> {
    // A present registration, even malformed, or a current archive directory
    // selects the registered source boundary. Missing registration must never
    // silently select a retired artifact when current source data is present.
    for relative in [CONTINUITY_PATH, "sources/history/runtime/vendor"] {
        let path = root.path().join(relative);
        no_symlinks(&path)?;
        match fs::symlink_metadata(path) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}
fn ensure_registration(root: &SkillRoot, registration: Option<&[u8]>) -> Result<()> {
    if let Some(raw) = registration {
        let provider = crate::books::rust::ArchiveProvider::open(root.path())?;
        provider.verify()?;
        ensure!(
            read_regular_bytes(&root.path().join(CONTINUITY_PATH), LOCK_LIMIT)? == raw,
            "release_source_registration_changed"
        );
    } else {
        ensure!(
            !has_registered_archive(root)?,
            "release_source_registration_changed"
        );
    }
    Ok(())
}
fn wheel_snapshot(root: &SkillRoot) -> Result<(Vec<u8>, String, VerifiedWheel, Option<Vec<u8>>)> {
    let raw = read_regular_bytes(
        &root.path().join(book_release::SOURCE_LOCK_PATH),
        LOCK_LIMIT,
    )?;
    let value = strict_json(&raw)?;
    ensure!(
        value["$schema"] == crate::runtime::SOURCE_LOCK_SCHEMA,
        "release_source_lock_schema"
    );
    let lock = RuntimeSourceLock {
        commit: text(&value["source"], "commit")?.into(),
        version: text(&value["source"], "version")?.into(),
        artifact_filename: text(&value["artifact"], "filename")?.into(),
        artifact_sha256: text(&value["artifact"], "sha256")?.into(),
    };
    // Use the existing source-lock validator without its retired vendor reader.
    // Preserve all original commit/version/filename/SHA identity requirements.
    ensure!(
        crate::runtime::read_runtime_source_lock(root.path(), false)? == lock,
        "release_source_lock_changed"
    );
    let registration = if has_registered_archive(root)? {
        Some(read_regular_bytes(
            &root.path().join(CONTINUITY_PATH),
            LOCK_LIMIT,
        )?)
    } else {
        None
    };
    let path = if registration.is_some() {
        let approved = crate::books::rust::ARCHIVE;
        ensure!(
            Path::new(approved).file_name().and_then(|v| v.to_str())
                == Some(lock.artifact_filename.as_str()),
            "release_registered_source_lock_filename_mismatch"
        );
        ensure!(
            lock.artifact_sha256 == crate::books::rust::APPROVED_ARCHIVE_SHA256,
            "release_registered_source_lock_sha256_mismatch"
        );
        ensure_registration(root, registration.as_deref())?;
        approved.to_owned()
    } else {
        // Explicit unregistered historical-static roots retain their original
        // wheel and RECORD contract, including isolated synthetic fixtures.
        format!("runtime/vendor/{}", lock.artifact_filename)
    };
    safe_relative(&path)?;
    let wheel = read_wheel_identity(&root.path().join(&path), &lock)?;
    wheel.verify_complete_record()?;
    ensure!(
        read_regular_bytes(
            &root.path().join(book_release::SOURCE_LOCK_PATH),
            LOCK_LIMIT
        )? == raw,
        "release_source_lock_changed"
    );
    ensure_registration(root, registration.as_deref())?;
    Ok((raw, path, wheel, registration))
}

/// Read the same locked, approved archive used by production inventory, with
/// full original wheel/RECORD validation. No caller-selected path is accepted.
pub fn read_locked_package_wheel(root: &SkillRoot) -> Result<VerifiedWheel> {
    let (_, path, wheel, _) = wheel_snapshot(root)?;
    ensure!(
        fingerprint_file(&root.path().join(path), crate::MAX_ARCHIVE_BYTES as u64)?.sha256
            == wheel.identity().artifact_sha256,
        "release_wheel_changed"
    );
    Ok(wheel)
}
fn packages(wheel: &VerifiedWheel) -> Result<Value> {
    let members = wheel.members();
    let expected: BTreeSet<String> = (1..=9)
        .map(|i| format!("semantica/chapter_packages/vol1/ch{i:02}/manifest.yaml"))
        .chain((1..=20).map(|i| format!("semantica/chapter_packages/vol2/ch{i:02}/manifest.yaml")))
        .collect();
    let actual: BTreeSet<String> = members
        .keys()
        .filter(|p| {
            let parts: Vec<_> = p.split('/').collect();
            parts.len() == 5
                && parts[0] == "semantica"
                && parts[1] == "chapter_packages"
                && matches!(parts[2], "vol1" | "vol2")
                && parts[3].len() == 4
                && parts[3].starts_with("ch")
                && parts[3][2..].bytes().all(|b| b.is_ascii_digit())
                && parts[4] == "manifest.yaml"
        })
        .cloned()
        .collect();
    ensure!(
        actual == expected,
        "release_expected_exact_29_package_coordinates"
    );
    let mut packages = Vec::new();
    for member in expected {
        let raw = &members[&member];
        let document =
            strict_json(raw).context("release_manifest_must_be_strict_json_compatible_yaml")?;
        ensure!(document.is_object(), "release_manifest_not_object");
        let parts: Vec<_> = member.split('/').collect();
        let (volume, chapter) = (parts[2], parts[3]);
        let expected_id = format!("semantica.chapter_packages.{volume}.{chapter}");
        ensure!(
            text(&document, "package_id")? == expected_id,
            "release_package_id_coordinate_mismatch"
        );
        let chapter_identity = &document["chapter"];
        let (declared_volume, declared_chapter) = if chapter_identity.is_object() {
            (&chapter_identity["volume"], &chapter_identity["chapter"])
        } else {
            (&document["volume"], chapter_identity)
        };
        ensure!(
            declared_volume == volume && declared_chapter == chapter,
            "release_chapter_coordinate_mismatch"
        );
        let prefix = member.strip_suffix("manifest.yaml").unwrap();
        let mut ids = BTreeSet::new();
        let mut declared = BTreeSet::from([member.clone()]);
        let mut assets = Vec::new();
        for asset in document["assets"]
            .as_array()
            .context("release_assets_not_array")?
        {
            let id = text(asset, "asset_id")?;
            let role = text(asset, "role")?;
            let path = text(asset, "path")?;
            safe_relative(path).context("release_unsafe_asset_path")?;
            ensure!(ids.insert(id), "release_duplicate_asset_id");
            let asset_member = format!("{prefix}{path}");
            ensure!(
                declared.insert(asset_member.clone()),
                "release_duplicate_asset_member"
            );
            let bytes = members
                .get(&asset_member)
                .context("release_asset_missing")?;
            let hash = sha256(bytes);
            ensure!(asset["sha256"] == hash, "release_asset_sha256_mismatch");
            assets.push(json!({"asset_id":id,"member":asset_member,"role":role,"sha256":hash}));
        }
        // Include descendants too: a recorded hidden subdirectory is not a
        // loophole in the manifest's declared asset closure.
        let physical: BTreeSet<String> = members
            .keys()
            .filter(|p| p.starts_with(prefix))
            .cloned()
            .collect();
        ensure!(physical == declared, "release_undeclared_package_member");
        assets.sort_by(|a, b| a["asset_id"].as_str().cmp(&b["asset_id"].as_str()));
        packages.push(json!({"assets":assets,"chapter":chapter,"manifest":{"member":member,"sha256":sha256(raw)},
            "package_id":expected_id,"release_status":text(&document,"release_status")?,"status":text(&document,"status")?,"version":text(&document,"version")?,"volume":volume}));
    }
    packages.sort_by(|a, b| a["package_id"].as_str().cmp(&b["package_id"].as_str()));
    Ok(Value::Array(packages))
}
pub fn package_inventory(root: &SkillRoot) -> Result<Value> {
    let (raw, path, wheel, registration) = wheel_snapshot(root)?;
    let result = packages(&wheel)?;
    ensure!(
        read_regular_bytes(
            &root.path().join(book_release::SOURCE_LOCK_PATH),
            LOCK_LIMIT
        )? == raw,
        "release_source_lock_changed"
    );
    ensure!(
        fingerprint_file(&root.path().join(path), crate::MAX_ARCHIVE_BYTES as u64)?.sha256
            == wheel.identity().artifact_sha256,
        "release_wheel_changed"
    );
    ensure_registration(root, registration.as_deref())?;
    Ok(result)
}
pub fn authoring_lock_paths(root: &SkillRoot) -> Result<Value> {
    match fs::symlink_metadata(root.path().join(RETIRED_VOL1_LOCK)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
        Ok(_) => anyhow::bail!("release_retired_vol1_authoring_lock_present"),
    }
    // No symlink, FIFO or outside-root aliases are acceptable authorities.
    for path in std::iter::once(VOL1_LOCK).chain(VOL2_LOCKS) {
        read_regular_bytes(&root.path().join(path), LOCK_LIMIT)?;
    }
    Ok(json!({"vol1":[VOL1_LOCK],"vol2":VOL2_LOCKS}))
}
#[derive(Debug, Clone)]
pub struct ReleaseSourcesSnapshot {
    packages: Value,
    authoring_locks: Value,
    artifact_refs: Value,
    bytes: BTreeMap<String, Vec<u8>>,
    wheel_path: String,
    wheel_sha256: String,
}
impl ReleaseSourcesSnapshot {
    pub fn capture(root: &SkillRoot) -> Result<Self> {
        let (raw, path, wheel, registration) = wheel_snapshot(root)?;
        let package_inventory = packages(&wheel)?;
        let locks = authoring_lock_paths(root)?;
        let mut bytes = BTreeMap::from([(book_release::SOURCE_LOCK_PATH.into(), raw.clone())]);
        if let Some(registration) = registration {
            bytes.insert(CONTINUITY_PATH.into(), registration);
        }
        let mut refs = Vec::new();
        for path in std::iter::once(VOL1_LOCK).chain(VOL2_LOCKS) {
            let data = read_regular_bytes(&root.path().join(path), LOCK_LIMIT)?;
            refs.push(reference(path, &data));
            bytes.insert(path.into(), data);
        }
        refs.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
        let wheel_ref = json!({"path":path,"sha256":wheel.identity().artifact_sha256,"size_bytes":wheel.archive_size()});
        let mut artifact_refs = json!({"source_lock":reference(book_release::SOURCE_LOCK_PATH,&raw),"wheel":wheel_ref,"authoring_locks":refs});
        if let Some(registration) = bytes.get(CONTINUITY_PATH) {
            artifact_refs["source_registration"] = reference(CONTINUITY_PATH, registration);
        }
        let result = Self {
            packages: package_inventory,
            authoring_locks: locks,
            artifact_refs,
            bytes,
            wheel_path: path,
            wheel_sha256: wheel.identity().artifact_sha256.clone(),
        };
        result.ensure_current(root)?;
        Ok(result)
    }
    pub fn packages(&self) -> &Value {
        &self.packages
    }
    pub fn authoring_locks(&self) -> &Value {
        &self.authoring_locks
    }
    pub fn artifact_refs(&self) -> &Value {
        &self.artifact_refs
    }
    pub fn ensure_current(&self, root: &SkillRoot) -> Result<()> {
        ensure!(
            authoring_lock_paths(root)? == self.authoring_locks,
            "release_authoring_lock_set_changed"
        );
        for (path, raw) in &self.bytes {
            ensure!(
                &read_regular_bytes(&root.path().join(path), LOCK_LIMIT)? == raw,
                "release_source_snapshot_changed:{path}"
            );
        }
        ensure!(
            fingerprint_file(
                &root.path().join(&self.wheel_path),
                crate::MAX_ARCHIVE_BYTES as u64
            )?
            .sha256
                == self.wheel_sha256,
            "release_wheel_changed"
        );
        ensure_registration(root, self.bytes.get(CONTINUITY_PATH).map(Vec::as_slice))?;
        Ok(())
    }
}

pub fn allowed_generated_paths() -> BTreeSet<String> {
    let mut paths: BTreeSet<String> = LEGACY_GENERATED.iter().map(|s| (*s).into()).collect();
    paths.insert(book_release::MANIFEST_PATH.into());
    paths.insert(format!("{}.sha256", book_release::MANIFEST_PATH));
    paths.insert(book_release::VISUAL_REVIEW_PATH.into());
    paths.extend(
        book_release::EVIDENCE_PATHS
            .iter()
            .chain(book_release::GOVERNANCE_PATHS)
            .chain(book_release::REGRESSION_LOG_PATHS)
            .map(|(_, p)| (*p).into()),
    );
    paths.extend(book_release::BOOK_SPECS.iter().map(|b| b.pdf.into()));
    paths
}
fn valid_commit(commit: &str) -> Result<()> {
    ensure!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "release_source_commit_invalid"
    );
    Ok(())
}
struct GitProof<'a> {
    git: &'a Path,
    checkout: PathBuf,
    output: PathBuf,
    steps: Vec<Value>,
    snapshots: Vec<Value>,
}
impl GitProof<'_> {
    fn run(&mut self, label: &str, args: &[&str], absent_allowed: bool) -> Result<Vec<u8>> {
        let environment: BTreeMap<OsString, OsString> = [
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("GIT_NO_LAZY_FETCH", "1"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_NO_REPLACE_OBJECTS", "1"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        let remove_environment = std::env::vars_os()
            .filter_map(|(k, _)| {
                (k.to_string_lossy().starts_with("GIT_") && !environment.contains_key(&k))
                    .then_some(k)
            })
            .collect();
        let mut arguments: Vec<OsString> = [
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "core.trustctime=true",
            "-c",
            "core.checkStat=default",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        arguments.extend(args.iter().map(OsString::from));
        let evidence = self
            .output
            .join("commands")
            .join(format!("{:03}-{label}", self.steps.len() + 1));
        let observed = run_command(
            &CommandSpec {
                executable: self.git.into(),
                arguments,
                cwd: self.checkout.clone(),
                environment,
                remove_environment,
                timeout: Duration::from_secs(300),
                output_limit: 8 * 1024 * 1024,
            },
            &evidence,
        )?;
        self.steps
            .push(json!({"step":label,"evidence_dir":evidence,"observation":observed.report}));
        let absent = absent_allowed
            && observed.status == "process_failed"
            && observed.exit_code == Some(1)
            && observed.stdout.is_empty()
            && observed.stderr.is_empty();
        ensure!(
            (observed.status == "completed" && observed.exit_code == Some(0)) || absent,
            "release_git_step_failed:{label}"
        );
        Ok(observed.stdout)
    }
    fn string(&mut self, label: &str, args: &[&str], absent: bool) -> Result<String> {
        Ok(String::from_utf8(self.run(label, args, absent)?)
            .context("release_git_output_not_utf8")?
            .trim_end_matches('\n')
            .into())
    }
    fn paths(&mut self, label: &str, args: &[&str]) -> Result<Vec<String>> {
        let raw = self.run(label, args, false)?;
        ensure!(
            raw.is_empty() || raw.ends_with(&[0]),
            "release_git_path_output_not_nul_terminated"
        );
        let paths = raw
            .split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| {
                let p = std::str::from_utf8(s).context("release_git_path_not_utf8")?;
                safe_relative(p)?;
                Ok(p.into())
            })
            .collect::<Result<Vec<String>>>()?;
        ensure!(
            paths.len() <= SOURCE_FILE_COUNT,
            "release_git_path_count_exceeded"
        );
        Ok(paths)
    }
    fn tree(&mut self, commit: &str) -> Result<BTreeMap<String, (String, String)>> {
        let raw = self.run(
            "commit-files",
            &["ls-tree", "-r", "-z", "--full-tree", commit],
            false,
        )?;
        ensure!(
            raw.is_empty() || raw.ends_with(&[0]),
            "release_git_tree_not_nul_terminated"
        );
        let mut tree = BTreeMap::new();
        for row in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let row = std::str::from_utf8(row)?;
            let (header, path) = row
                .split_once('\t')
                .context("release_git_tree_row_invalid")?;
            safe_relative(path)?;
            let header: Vec<_> = header.split(' ').collect();
            ensure!(
                header.len() == 3
                    && matches!(header[0], "100644" | "100755")
                    && header[1] == "blob",
                "release_git_tree_nonregular_source"
            );
            valid_commit(header[2])?;
            ensure!(
                tree.insert(path.into(), (header[0].into(), header[2].into()))
                    .is_none(),
                "release_git_tree_duplicate_path"
            );
            ensure!(tree.len() <= SOURCE_FILE_COUNT, "release_git_tree_limit");
        }
        Ok(tree)
    }
    fn inventory(
        &mut self,
        allowed: &BTreeSet<String>,
        tree: &BTreeMap<String, (String, String)>,
        capture: bool,
    ) -> Result<Vec<Value>> {
        let flags = self.run("index-flags", &["ls-files", "-v", "-z"], false)?;
        for row in flags.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            ensure!(
                row.len() > 2 && row[1] == b' ' && row[0] == b'H',
                "release_hidden_index_entry"
            );
        }
        let files = self.paths("tracked-files", &["ls-files", "-z"])?;
        let unique: BTreeSet<_> = files.iter().collect();
        ensure!(unique.len() == files.len(), "release_unmerged_index");
        ensure!(
            files
                .iter()
                .filter(|p| !allowed.contains(*p))
                .collect::<BTreeSet<_>>()
                == tree.keys().filter(|p| !allowed.contains(*p)).collect(),
            "release_source_path_set_differs_from_commit"
        );
        let mut total = 0u64;
        let mut result = Vec::new();
        let mut objects = Vec::new();
        for relative in files {
            if allowed.contains(&relative) {
                continue;
            }
            let path = self.checkout.join(&relative);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let executable = fs::symlink_metadata(&path)?.permissions().mode() & 0o111 != 0;
                ensure!(
                    executable == (tree[&relative].0 == "100755"),
                    "release_source_mode_differs_from_commit"
                );
            }
            let id = if capture {
                let raw = read_regular_bytes(&path, SOURCE_FILE_LIMIT as usize)?;
                let id = oe_core::FileIdentity {
                    bytes: raw.len() as u64,
                    sha256: sha256(&raw),
                };
                ensure!(
                    total
                        .checked_add(id.bytes)
                        .is_some_and(|n| n <= SOURCE_TOTAL_LIMIT),
                    "release_source_total_exceeded"
                );
                let snapshot = self
                    .output
                    .join("source-snapshots")
                    .join(format!("{:05}.raw", objects.len()));
                write_new_bytes(&snapshot, &raw)?;
                self.snapshots.push(json!({"source_path":relative,"snapshot":snapshot,"sha256":id.sha256,"size_bytes":id.bytes,"commit_blob":tree[&relative].1}));
                objects.push((snapshot, tree[&relative].1.clone()));
                id
            } else {
                fingerprint_file(&path, SOURCE_FILE_LIMIT)?
            };
            total = total
                .checked_add(id.bytes)
                .context("release_source_size_overflow")?;
            ensure!(total <= SOURCE_TOTAL_LIMIT, "release_source_total_exceeded");
            result.push(json!({"path":relative,"sha256":id.sha256,"size_bytes":id.bytes}));
        }
        // The object hash uses the exact captured bytes, with no clean filter
        // or line-ending conversion that could hide executable/source drift.
        for (index, chunk) in objects.chunks(128).enumerate() {
            let paths = chunk
                .iter()
                .map(|(p, _)| p.to_str().context("release_snapshot_path_not_utf8"))
                .collect::<Result<Vec<_>>>()?;
            let mut args = vec!["hash-object", "--no-filters", "--"];
            args.extend(paths);
            let observed = self.string(&format!("captured-objects-{index:04}"), &args, false)?;
            ensure!(
                observed
                    .lines()
                    .eq(chunk.iter().map(|(_, oid)| oid.as_str())),
                "release_source_bytes_differ_from_commit_blob"
            );
        }
        if capture {
            for snapshot in &self.snapshots {
                let path = Path::new(text(snapshot, "snapshot")?);
                let id = fingerprint_file(path, SOURCE_FILE_LIMIT)?;
                ensure!(
                    snapshot["sha256"] == id.sha256 && snapshot["size_bytes"] == id.bytes,
                    "release_captured_source_changed"
                );
            }
        }
        result.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
        Ok(result)
    }
    fn observe(&mut self, commit: &str, semantica: bool) -> Result<Value> {
        let top = self.string("root", &["rev-parse", "--show-toplevel"], false)?;
        ensure!(
            Path::new(&top) == self.checkout,
            "release_not_checkout_root"
        );
        self.run(
            "commit",
            &["cat-file", "-e", &format!("{commit}^{{commit}}")],
            false,
        )?;
        let head = self.string("head", &["rev-parse", "HEAD"], false)?;
        valid_commit(&head)?;
        self.run(
            "ancestor",
            &["merge-base", "--is-ancestor", commit, &head],
            false,
        )?;
        if semantica {
            ensure!(head == commit, "release_semantica_head_differs_from_lock")
        }
        ensure!(
            self.string("shallow", &["rev-parse", "--is-shallow-repository"], false)? == "false",
            "release_source_shallow"
        );
        let sparse = self.string(
            "sparse",
            &["config", "--bool", "--get", "core.sparseCheckout"],
            true,
        )?;
        ensure!(
            sparse.is_empty() || sparse == "false",
            "release_source_sparse"
        );
        ensure!(
            self.string(
                "partial",
                &[
                    "config",
                    "--get-regexp",
                    "^(extensions\\.partialclone|remote\\..*\\.promisor)$"
                ],
                true
            )?
            .is_empty(),
            "release_source_partial"
        );
        self.run("fsck", &["fsck", "--full", "--no-progress"], false)?;
        ensure!(
            self.string(
                "filters",
                &["config", "--get-regexp", "^filter\\..*\\.(clean|process)$"],
                true
            )?
            .is_empty(),
            "release_source_clean_filter_not_permitted"
        );
        let tree = self.string("tree", &["rev-parse", &format!("{commit}^{{tree}}")], false)?;
        valid_commit(&tree)?;
        let allowed = if semantica {
            BTreeSet::new()
        } else {
            allowed_generated_paths()
        };
        let commit_files = self.tree(commit)?;
        let tracked = self.paths(
            "tracked-delta",
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--ignore-submodules=none",
                "--name-only",
                "-z",
                commit,
                "--",
            ],
        )?;
        let untracked = self.paths(
            "untracked-delta",
            &["ls-files", "-z", "--others", "--exclude-standard"],
        )?;
        for relative in tracked.iter().chain(&untracked) {
            ensure!(
                allowed.contains(relative),
                "release_source_outside_generated_artifact_set:{relative}"
            );
            let path = self.checkout.join(relative);
            no_symlinks(&path)?;
            match fs::symlink_metadata(&path) {
                Ok(m) => ensure!(m.is_file(), "release_generated_artifact_not_regular"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        let files = self.inventory(&allowed, &commit_files, true)?;
        ensure!(
            self.string("final-head", &["rev-parse", "HEAD"], false)? == head,
            "release_head_changed"
        );
        ensure!(
            self.paths(
                "final-tracked-delta",
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-renames",
                    "--ignore-submodules=none",
                    "--name-only",
                    "-z",
                    commit,
                    "--"
                ]
            )? == tracked,
            "release_tracked_delta_changed"
        );
        ensure!(
            self.paths(
                "final-untracked-delta",
                &["ls-files", "-z", "--others", "--exclude-standard"]
            )? == untracked,
            "release_untracked_delta_changed"
        );
        ensure!(
            self.inventory(&allowed, &commit_files, false)? == files,
            "release_source_bytes_changed"
        );
        Ok(
            json!({"source_identity":{"source_commit":commit,"source_tree":tree,"head":head,"files":files},"tracked_delta":tracked,"untracked_delta":untracked,"allowed_generated_paths":allowed}),
        )
    }
}
fn verify_git(
    root: &SkillRoot,
    checkout: &Path,
    commit: &str,
    git: &Path,
    output: &Path,
    semantica: bool,
) -> Result<Value> {
    valid_commit(commit)?;
    let checkout = root.expand(checkout)?;
    no_symlinks(&checkout)?;
    let checkout = fs::canonicalize(checkout)?;
    ensure!(checkout.is_dir(), "release_checkout_missing");
    let output = root.private_path(output)?;
    if semantica {
        ensure!(
            !output.starts_with(&checkout) && !checkout.starts_with(&output),
            "release_checkout_output_overlap"
        )
    }
    let output = root.new_private_dir(&output)?;
    create_new_private_dir(&output.join("commands"))?;
    create_new_private_dir(&output.join("source-snapshots"))?;
    let mut proof = GitProof {
        git,
        checkout,
        output,
        steps: Vec::new(),
        snapshots: Vec::new(),
    };
    let lock = if semantica {
        Some(read_regular_bytes(
            &root.path().join(book_release::SOURCE_LOCK_PATH),
            LOCK_LIMIT,
        )?)
    } else {
        None
    };
    let outcome = (|| {
        if let Some(bytes) = &lock {
            let parsed = strict_json(bytes)?;
            let identity = crate::runtime::read_runtime_source_lock(root.path(), false)?;
            ensure!(
                parsed["$schema"] == crate::runtime::SOURCE_LOCK_SCHEMA
                    && parsed["source"]["version"] == identity.version
                    && parsed["artifact"]["filename"] == identity.artifact_filename
                    && parsed["artifact"]["sha256"] == identity.artifact_sha256,
                "release_source_lock_identity_changed"
            );
            ensure!(
                parsed["source"]["commit"] == commit && identity.commit == commit,
                "release_semantica_commit_differs_from_source_lock"
            )
        }
        let value = proof.observe(commit, semantica)?;
        if let Some(bytes) = &lock {
            ensure!(
                &read_regular_bytes(
                    &root.path().join(book_release::SOURCE_LOCK_PATH),
                    LOCK_LIMIT
                )? == bytes,
                "release_source_lock_changed"
            )
        }
        Ok::<_, anyhow::Error>(value)
    })();
    let mut report = json!({"schema":"ontology-engineering.release-source-boundary/v1","status":if outcome.is_ok(){"passed"}else{"blocked"},
        "kind":if semantica{"semantica"}else{"ontology_engineering"},"repository":if semantica{"https://github.com/jiaqiwang969/semantica.git"}else{"https://github.com/jiaqiwang969/OntologyEngineering.git"},
        "checkout":proof.checkout,"steps":proof.steps,"source_snapshots":proof.snapshots,"scope":"tracked-and-unignored-worktree","semantic_execution":"not_run"});
    match &outcome {
        Ok(value) => {
            for (k, v) in value.as_object().unwrap() {
                report[k] = v.clone()
            }
        }
        Err(e) => report["error"] = e.to_string().into(),
    }
    write_new(&proof.output.join("source-boundary.json"), &report)?;
    outcome?;
    Ok(report)
}
pub fn verify_source_boundary(
    root: &SkillRoot,
    commit: &str,
    git: &Path,
    output: &Path,
) -> Result<Value> {
    verify_git(root, root.path(), commit, git, output, false)
}
pub fn verify_semantica_checkout(
    root: &SkillRoot,
    checkout: &Path,
    commit: &str,
    git: &Path,
    output: &Path,
) -> Result<Value> {
    verify_git(root, checkout, commit, git, output, true)
}
/// Reobserve source bytes after replay; generated outputs do not replace the
/// pinned source identity and cannot mask an unrelated tracked/untracked delta.
pub fn ensure_source_unchanged(
    root: &SkillRoot,
    before: &Value,
    git: &Path,
    output: &Path,
) -> Result<Value> {
    ensure!(
        before["status"] == "passed",
        "release_previous_source_proof_not_passed"
    );
    let commit = text(&before["source_identity"], "source_commit")?;
    let after = match before["kind"].as_str() {
        Some("ontology_engineering") => verify_source_boundary(root, commit, git, output)?,
        Some("semantica") => verify_semantica_checkout(
            root,
            Path::new(text(before, "checkout")?),
            commit,
            git,
            output,
        )?,
        _ => anyhow::bail!("release_source_kind_invalid"),
    };
    ensure!(
        before["source_identity"] == after["source_identity"],
        "release_source_identity_changed_after_replay"
    );
    Ok(after)
}
