//! Native installation and registration of lossless supplier source libraries.
//! Source mappings are checked against the index; registration never grants
//! engineering authority and ordinary installation does not expand PDF pages.
use crate::{
    index::{MAX_INDEX_BYTES, ReadIndex},
    publication,
    source_pack::{self, ArchiveLimits, Stage, file_hash, relative_path, within_root},
    supplier_index::{ArchiveMetadata, parse_metadata_directory},
};
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, create_new_private_dir, create_new_regular_file, json::strict_json};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const DATA_SCHEMA: &str = "ontology-engineering.misumi-data/v1";
const LOCATION_SCHEMA: &str = "ontology-engineering.misumi-source-location/v1";
const MAX_ARCHIVE_BYTES: u64 = 34 * 1024 * 1024 * 1024;
const MAX_METADATA_FILE: u64 = 128 * 1024 * 1024;
const MAX_METADATA_BYTES: u64 = 256 * 1024 * 1024;
const MAX_METADATA_MEMBERS: usize = 4096;

#[derive(Clone)]
struct BoundFile {
    path: String,
    sha: String,
    bytes: u64,
}

impl BoundFile {
    fn parse(value: &Value, limit: u64) -> Result<Self> {
        let path = value["path"]
            .as_str()
            .context("supplier_file_path_invalid")?;
        relative_path(path)?;
        let sha = value["sha256"]
            .as_str()
            .context("supplier_file_hash_invalid")?;
        let bytes = value["bytes"]
            .as_u64()
            .context("supplier_file_size_invalid")?;
        ensure!(
            source_pack::valid_sha(sha) && bytes <= limit,
            "supplier_file_identity_invalid"
        );
        Ok(Self {
            path: path.into(),
            sha: sha.into(),
            bytes,
        })
    }

    fn value(&self) -> Value {
        json!({"path":self.path,"sha256":self.sha,"bytes":self.bytes})
    }
}

struct Pack {
    manifest: PathBuf,
    raw: Vec<u8>,
    value: Value,
    metadata: BTreeMap<String, BoundFile>,
}

impl Pack {
    fn load(root: &SkillRoot, path: &Path) -> Result<Self> {
        let manifest = source_path(root, path)?;
        let raw = oe_core::read_regular_bytes(&manifest, source_pack::MAX_MANIFEST_BYTES)?;
        let value = source_pack::load_manifest(root.path(), &manifest)?;
        ensure!(strict_json(&raw)? == value, "source_pack_manifest_changed");
        let meta = &value["metadata"];
        BoundFile::parse(meta, MAX_ARCHIVE_BYTES)?;
        let entries = meta["files"]
            .as_array()
            .context("metadata_inventory_invalid")?;
        ensure!(
            !entries.is_empty() && entries.len() <= MAX_METADATA_MEMBERS,
            "metadata_inventory_invalid"
        );
        let mut metadata = BTreeMap::new();
        let mut total = 0u64;
        for value in entries {
            let entry = BoundFile::parse(value, MAX_METADATA_FILE)?;
            ensure!(
                entry.path == "catalog.sqlite3"
                    || entry.path.starts_with("archive/") && entry.path.ends_with(".json"),
                "metadata_inventory_invalid"
            );
            total = total
                .checked_add(entry.bytes)
                .context("metadata_size_overflow")?;
            ensure!(
                metadata.insert(entry.path.clone(), entry).is_none(),
                "duplicate_metadata_path"
            );
        }
        ensure!(
            total <= MAX_METADATA_BYTES
                && metadata.get("catalog.sqlite3").map(BoundFile::value)
                    == Some(BoundFile::parse(&value["index"], MAX_INDEX_BYTES)?.value()),
            "metadata_index_identity_mismatch"
        );
        let pack = Self {
            manifest,
            raw,
            value,
            metadata,
        };
        pack.full_archive()?;
        pack.verify_manifest()?;
        Ok(pack)
    }

    fn base(&self) -> &Path {
        self.manifest.parent().expect("manifest parent")
    }

    fn verify_manifest(&self) -> Result<()> {
        ensure!(
            oe_core::read_regular_bytes(&self.manifest, source_pack::MAX_MANIFEST_BYTES)?
                == self.raw,
            "source_pack_manifest_changed"
        );
        Ok(())
    }

    fn full_archive(&self) -> Result<Option<(Value, BTreeMap<String, BoundFile>)>> {
        let Some(archive) = self.value.get("full_pdfs_archive") else {
            return Ok(None);
        };
        BoundFile::parse(archive, MAX_ARCHIVE_BYTES)?;
        let assemblies = self.value["assembled_pdfs"]
            .as_array()
            .context("source_pack_assembled_inventory_missing")?;
        let mut expected = BTreeMap::new();
        for a in assemblies {
            let binding = BoundFile::parse(a, MAX_ARCHIVE_BYTES)?;
            let name = a["download_path"]
                .as_str()
                .context("full_pdf_archive_inventory_mismatch")?;
            relative_path(name)?;
            ensure!(
                name.starts_with("full/")
                    && expected
                        .insert(name.to_owned(), (binding.sha, binding.bytes))
                        .is_none(),
                "full_pdf_archive_inventory_mismatch"
            );
        }
        let mut files = BTreeMap::new();
        for value in archive["files"]
            .as_array()
            .context("full_pdf_archive_inventory_mismatch")?
        {
            let entry = BoundFile::parse(value, MAX_ARCHIVE_BYTES)?;
            ensure!(
                expected.get(&entry.path) == Some(&(entry.sha.clone(), entry.bytes))
                    && files.insert(entry.path.clone(), entry).is_none(),
                "full_pdf_archive_assembly_mismatch"
            );
        }
        ensure!(
            !files.is_empty() && files.len() == expected.len(),
            "full_pdf_archive_inventory_mismatch"
        );
        Ok(Some((archive.clone(), files)))
    }
}

fn source_path(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = within_root(root.path(), path)?;
    ensure!(
        path.starts_with(root.path().join("sources")) && path != root.path().join("sources"),
        "supplier_source_outside_sources"
    );
    Ok(path)
}

fn target_path(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = source_path(root, path)?;
    let base = root.path().join("sources/.indexes");
    ensure!(
        path.starts_with(&base) && path != base,
        "supplier_registration_outside_indexes"
    );
    Ok(path)
}

fn under(root: &SkillRoot, base: &Path, relative: &str) -> Result<PathBuf> {
    relative_path(relative)?;
    within_root(root.path(), &base.join(relative))
}

fn require_pack_archive(pack: &Pack, archive: &Path) -> Result<()> {
    ensure!(
        archive == pack.base().join("cache/archive"),
        "registered_source_pack_archive_mismatch"
    );
    Ok(())
}

#[cfg(unix)]
fn parents(path: &Path) -> Result<()> {
    let parent = path.parent().context("supplier_parent_missing")?;
    drop(source_pack::directory_fd(parent, true)?);
    Ok(())
}

#[cfg(not(unix))]
fn parents(_path: &Path) -> Result<()> {
    anyhow::bail!("safe_supplier_copy_requires_unix_directory_identity")
}

#[cfg(all(test, not(unix)))]
mod platform_tests {
    use super::*;
    #[test]
    fn non_unix_copy_refuses_before_creating_any_destination() -> Result<()> {
        let root = SkillRoot::new(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()?,
        )?;
        let test = root.new_private_dir(Path::new(&format!(
            "var/maintenance/platform-tests/supplier-copy-{}",
            std::process::id()
        )))?;
        let input = test.join("input.pdf");
        let bytes = b"synthetic supplier source, no private material";
        std::fs::write(&input, bytes)?;
        let destination = test.join("receiver/nested/output.pdf");
        let binding = BoundFile {
            path: "output.pdf".into(),
            sha: oe_core::sha256(bytes),
            bytes: bytes.len() as u64,
        };
        let error = copy_bound(&input, &destination, &binding).unwrap_err();
        assert_eq!(
            error.to_string(),
            "safe_supplier_copy_requires_unix_directory_identity"
        );
        assert!(!test.join("receiver").exists());
        assert_eq!(std::fs::read(&input)?, bytes);
        Ok(())
    }
}

fn exact_file(path: &Path, file: &BoundFile) -> Result<()> {
    ensure!(
        file_hash(path, file.bytes)? == (file.sha.clone(), file.bytes),
        "supplier_source_hash_mismatch"
    );
    Ok(())
}

fn copy_bound(source: &Path, destination: &Path, binding: &BoundFile) -> Result<()> {
    let mut input = oe_core::open_regular_file(source, binding.bytes)?;
    let before = input.metadata()?;
    ensure!(
        before.len() == binding.bytes,
        "downloaded_file_identity_mismatch"
    );
    parents(destination)?;
    let mut output = create_new_regular_file(destination)?;
    let mut count = 0u64;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count = count
            .checked_add(n as u64)
            .context("supplier_size_overflow")?;
        ensure!(count <= binding.bytes, "downloaded_file_identity_mismatch");
        output.write_all(&buffer[..n])?;
        hash.update(&buffer[..n]);
    }
    output.sync_all()?;
    let after = input.metadata()?;
    ensure!(
        count == binding.bytes
            && format!("{:x}", hash.finalize()) == binding.sha
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "downloaded_file_identity_mismatch"
    );
    exact_file(source, binding)?;
    exact_file(destination, binding)
}

struct Required {
    source: PathBuf,
    relative: String,
    sha: String,
    bytes: Option<u64>,
    role: &'static str,
}

struct Closure {
    required: Vec<Required>,
    identity: Value,
    index_identity: Value,
    source_binding: Option<Value>,
}

fn require_source_mode(root: &SkillRoot, closure: &Closure, pack: Option<&Pack>) -> Result<()> {
    let Some(binding) = &closure.source_binding else {
        return Ok(());
    };
    if let Some(pack) = pack {
        ensure!(
            binding["kind"] == "local_lossless_shards"
                && binding["source_pack_manifest"].as_str()
                    == pack.manifest.strip_prefix(root.path())?.to_str()
                && binding["source_pack_manifest_sha256"] == oe_core::sha256(&pack.raw),
            "index_source_pack_binding_mismatch"
        );
    } else {
        // Expanded originals may be explicitly moved by register/import. Their
        // exact metadata/page/full-book hashes retain identity; the old build
        // path is provenance, not a permanent storage location requirement.
        ensure!(
            binding["kind"] == "expanded_archive",
            "index_source_mode_downgrade"
        );
    }
    Ok(())
}

fn mapping(root: &SkillRoot, archive: &Path, index: &Path, page_files: bool) -> Result<Closure> {
    let metadata = parse_metadata_directory(root.path(), archive)?;
    mapping_with_metadata(root, archive, index, page_files, &metadata)
}

fn mapping_with_metadata(
    root: &SkillRoot,
    archive: &Path,
    index_path: &Path,
    page_files: bool,
    metadata: &ArchiveMetadata,
) -> Result<Closure> {
    let index = ReadIndex::open_source(root, index_path, "misumi")?;
    let catalogs = index.rows("SELECT code,page_count,pdf_relative_path,pdf_sha256,metadata_sha256,manifest_sha256 FROM catalogs ORDER BY code", &[], 64)?;
    ensure!(
        catalogs.len() == metadata.catalogs.len(),
        "source_catalog_identity_mismatch"
    );
    let mut required = vec![Required {
        source: index_path.into(),
        relative: "catalog.sqlite3".into(),
        sha: index.identity()["sha256"]
            .as_str()
            .context("index_identity_missing")?
            .into(),
        bytes: index.identity()["bytes"].as_u64(),
        role: "index",
    }];
    for file in &metadata.files {
        let role = if file.path.ends_with("/metadata.json") {
            "ebook_metadata"
        } else if file.path.ends_with("-manifest.json") {
            "page_manifest"
        } else if file.path.ends_with("-assembly.json") {
            "assembly_record"
        } else {
            "assembly_qc"
        };
        required.push(Required {
            source: under(root, archive, &file.path)?,
            relative: format!("archive/{}", file.path),
            sha: file.sha256.clone(),
            bytes: Some(file.bytes),
            role,
        });
    }
    for catalog in &metadata.catalogs {
        let row = catalogs
            .iter()
            .find(|r| r["code"] == catalog.code)
            .context("missing_catalog")?;
        ensure!(
            row["page_count"] == catalog.page_count
                && row["pdf_relative_path"] == catalog.pdf_relative_path
                && row["pdf_sha256"] == catalog.pdf_sha256
                && row["metadata_sha256"] == catalog.metadata_sha256
                && row["manifest_sha256"] == catalog.manifest_sha256,
            "source_catalog_identity_mismatch"
        );
        required.push(Required {
            source: under(root, archive, &catalog.pdf_relative_path)?,
            relative: format!("archive/{}", catalog.pdf_relative_path),
            sha: catalog.pdf_sha256.clone(),
            bytes: None,
            role: "assembled_pdf",
        });
        let mut seen = BTreeSet::new();
        let pages: BTreeMap<_, _> = catalog.pages.iter().map(|p| (p.number, p)).collect();
        let mut offset = 0i64;
        loop {
            let rows = index.rows("SELECT pdf_page,page_id,source_relative_path,source_sha256,source_bytes FROM pages WHERE catalog=? ORDER BY pdf_page,id LIMIT 512 OFFSET ?", &[&catalog.code,&offset], 512)?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let number = row["pdf_page"]
                    .as_u64()
                    .context("source_page_mapping_mismatch")?;
                ensure!(seen.insert(number), "source_page_mapping_mismatch");
                let page = pages.get(&number).context("source_page_mapping_mismatch")?;
                ensure!(
                    row["page_id"] == page.page_id
                        && row["source_relative_path"] == page.relative_path
                        && row["source_sha256"] == page.sha256
                        && row["source_bytes"] == page.bytes,
                    "source_page_identity_mismatch"
                );
                let source = under(root, archive, &page.relative_path)?;
                if page_files {
                    ensure!(
                        oe_core::open_regular_file(&source, page.bytes)?
                            .metadata()?
                            .len()
                            == page.bytes,
                        "source_page_size_mismatch"
                    );
                }
                required.push(Required {
                    source,
                    relative: format!("archive/{}", page.relative_path),
                    sha: page.sha256.clone(),
                    bytes: Some(page.bytes),
                    role: "source_page_pdf",
                });
            }
            offset += 512;
        }
        ensure!(
            seen.len() == catalog.pages.len(),
            "source_page_count_mismatch"
        );
    }
    let page_count = index.rows("SELECT COUNT(*) AS total FROM pages", &[], 1)?;
    ensure!(
        page_count[0]["total"].as_u64()
            == Some(metadata.catalogs.iter().map(|c| c.pages.len() as u64).sum()),
        "source_page_count_mismatch"
    );
    ensure!(
        index.meta["source_identity"] == metadata.source_identity(),
        "index_source_identity_mismatch"
    );
    let mut seen = BTreeSet::new();
    ensure!(
        required.iter().all(|f| seen.insert(f.relative.clone())),
        "duplicate_data_path"
    );
    index.verify()?;
    let identity = json!({"source_identity":metadata.source_identity(),"counts":index.meta["counts"],"index_fingerprint":index.meta["fingerprint"]});
    Ok(Closure {
        required,
        identity,
        index_identity: index.identity(),
        source_binding: index.meta.get("source_binding").cloned(),
    })
}

fn verify_pack_mapping(
    root: &SkillRoot,
    pack: &Pack,
    metadata_base: &Path,
    index: &Path,
    bind_delivered_index: bool,
) -> Result<Closure> {
    let closure = mapping(root, &metadata_base.join("archive"), index, false)?;
    require_source_mode(root, &closure, Some(pack))?;
    if bind_delivered_index {
        let entry = BoundFile::parse(&pack.value["index"], MAX_INDEX_BYTES)?;
        ensure!(
            closure.index_identity == json!({"sha256":entry.sha,"bytes":entry.bytes}),
            "source_pack_index_changed"
        );
    }
    let mut declared = pack.metadata.clone();
    for segment in pack.value["segments"]
        .as_array()
        .context("source_pack_empty_segments")?
    {
        for file in segment["files"]
            .as_array()
            .context("source_pack_invalid_segment")?
        {
            let mut entry = BoundFile::parse(file, source_pack::MAX_SOURCE_BYTES)?;
            entry.path = format!("archive/{}", entry.path);
            ensure!(
                declared.insert(entry.path.clone(), entry).is_none(),
                "source_pack_metadata_closure_mismatch"
            );
        }
    }
    for file in pack.value["assembled_pdfs"]
        .as_array()
        .context("source_pack_assembled_inventory_missing")?
    {
        let mut entry = BoundFile::parse(file, MAX_ARCHIVE_BYTES)?;
        entry.path = format!("archive/{}", entry.path);
        ensure!(
            declared.insert(entry.path.clone(), entry).is_none(),
            "source_pack_metadata_closure_mismatch"
        );
    }
    ensure!(
        declared.keys().cloned().collect::<BTreeSet<_>>()
            == closure
                .required
                .iter()
                .map(|f| f.relative.clone())
                .collect(),
        "source_pack_metadata_closure_mismatch"
    );
    for required in &closure.required {
        if required.role == "index" && !bind_delivered_index {
            continue;
        }
        let file = &declared[&required.relative];
        ensure!(
            file.sha == required.sha && required.bytes.is_none_or(|b| b == file.bytes),
            "source_pack_metadata_index_mismatch"
        );
    }
    if let Some(identity) = pack.value.get("source_catalogs") {
        // A newly built index has its own counts/fingerprint; the original source
        // identity remains bound to the unmodified supplier transport manifest.
        ensure!(
            identity["source_identity"] == closure.identity["source_identity"],
            "source_pack_catalog_identity_mismatch"
        );
        if bind_delivered_index {
            ensure!(
                identity == &closure.identity,
                "source_pack_catalog_identity_mismatch"
            );
        }
    }
    for catalog in &parse_metadata_directory(root.path(), &metadata_base.join("archive"))?.catalogs
    {
        let matches = pack.value["assembled_pdfs"]
            .as_array()
            .expect("checked")
            .iter()
            .filter(|a| {
                a["catalog"] == catalog.code
                    && a["path"] == catalog.pdf_relative_path
                    && a["sha256"] == catalog.pdf_sha256
            })
            .count();
        ensure!(matches == 1, "source_pack_assembled_identity_mismatch");
    }
    pack.verify_manifest()?;
    Ok(closure)
}

fn extract_metadata(root: &SkillRoot, pack: &Pack, destination: &Path, index: bool) -> Result<()> {
    let descriptor = &pack.value["metadata"];
    let archive = under(
        root,
        pack.base(),
        descriptor["path"]
            .as_str()
            .context("metadata_inventory_invalid")?,
    )?;
    let selected: BTreeSet<String> = pack
        .metadata
        .keys()
        .filter(|p| index || p.as_str() != "catalog.sqlite3")
        .cloned()
        .collect();
    if !archive.try_exists()? {
        // Legacy installs retain expanded metadata, not its downloaded tar.
        // Only absence enables this path; an existing corrupt tar must fail.
        for entry in pack.metadata.values() {
            let source = under(root, pack.base(), &entry.path)?;
            exact_file(&source, entry)?;
            if selected.contains(&entry.path) {
                copy_bound(&source, &under(root, destination, &entry.path)?, entry)?;
            }
        }
        return pack.verify_manifest();
    }
    source_pack::visit_archive(
        root.path(),
        &archive,
        descriptor,
        &ArchiveLimits {
            max_file_bytes: MAX_METADATA_FILE,
            max_members: MAX_METADATA_MEMBERS,
            max_uncompressed_bytes: MAX_METADATA_BYTES,
        },
        Some(&selected),
        |member| {
            let entry = pack
                .metadata
                .get(&member.relative_path)
                .context("metadata_inventory_mismatch")?;
            copy_bound(&member.path, &under(root, destination, &entry.path)?, entry)
        },
    )?;
    pack.verify_manifest()
}

fn verify_optional_metadata_archive(root: &SkillRoot, pack: &Pack) -> Result<()> {
    let descriptor = &pack.value["metadata"];
    let archive = under(
        root,
        pack.base(),
        descriptor["path"]
            .as_str()
            .context("metadata_inventory_invalid")?,
    )?;
    if archive.try_exists()? {
        source_pack::visit_archive(
            root.path(),
            &archive,
            descriptor,
            &ArchiveLimits {
                max_file_bytes: MAX_METADATA_FILE,
                max_members: MAX_METADATA_MEMBERS,
                max_uncompressed_bytes: MAX_METADATA_BYTES,
            },
            Some(&BTreeSet::new()),
            |_| anyhow::bail!("unexpected_metadata_materialization"),
        )?;
    }
    pack.verify_manifest()
}

fn library_layout(target: &Path) -> Result<&'static str> {
    let mut found = None;
    for name in ["source.json", "source-pack.json", "data-manifest.json"] {
        if target.join(name).try_exists()? {
            ensure!(found.is_none(), "supplier_library_ambiguous_layout");
            found = Some(name);
        }
    }
    found.context("supplier_library_manifest_missing")
}

fn verify_pack_archives(root: &SkillRoot, pack: &Pack) -> Result<()> {
    for value in pack.value["segments"]
        .as_array()
        .context("source_pack_empty_segments")?
    {
        let entry = BoundFile::parse(value, MAX_ARCHIVE_BYTES)?;
        exact_file(&under(root, pack.base(), &entry.path)?, &entry)?;
    }
    if let Some((value, _)) = pack.full_archive()? {
        let entry = BoundFile::parse(&value, MAX_ARCHIVE_BYTES)?;
        exact_file(&under(root, pack.base(), &entry.path)?, &entry)?;
    }
    pack.verify_manifest()
}

/// Install locally downloaded immutable shards and metadata into a new index
/// directory. No networking and no page expansion occur.
pub fn install_pack(root: &SkillRoot, downloads: &Path, target: &Path) -> Result<Value> {
    let downloads = source_path(root, downloads)?;
    let target = target_path(root, target)?;
    ensure!(
        !target.try_exists()? && !target.starts_with(&downloads) && !downloads.starts_with(&target),
        "data_root_already_exists_or_overlaps_source"
    );
    let pack = Pack::load(root, &downloads.join("source-pack.json"))?;
    let stage = Stage::new(root.path(), "supplier-install")?;
    let built = stage.path.join("library");
    create_new_private_dir(&built)?;
    extract_metadata(root, &pack, &built, true)?;
    for segment in pack.value["segments"].as_array().expect("validated") {
        let entry = BoundFile::parse(segment, MAX_ARCHIVE_BYTES)?;
        copy_bound(
            &under(root, &downloads, &entry.path)?,
            &under(root, &built, &entry.path)?,
            &entry,
        )?;
    }
    let full = pack.full_archive()?;
    if let Some((value, _)) = &full {
        let entry = BoundFile::parse(value, MAX_ARCHIVE_BYTES)?;
        copy_bound(
            &under(root, &downloads, &entry.path)?,
            &under(root, &built, &entry.path)?,
            &entry,
        )?;
    }
    let mut installed_full = 0;
    if full.is_none() {
        for value in pack.value["assembled_pdfs"]
            .as_array()
            .context("source_pack_assembled_inventory_missing")?
        {
            if let Some(download) = value["download_path"].as_str() {
                let source = under(root, &downloads, download)?;
                if source.try_exists()? {
                    let entry = BoundFile::parse(value, MAX_ARCHIVE_BYTES)?;
                    copy_bound(
                        &source,
                        &under(root, &built, &format!("cache/archive/{}", entry.path))?,
                        &entry,
                    )?;
                    installed_full += 1;
                }
            }
        }
    }
    parents(&built.join("cache/archive/placeholder"))?;
    oe_core::write_new_bytes(&built.join("source-pack.json"), &pack.raw)?;
    let closure = verify_pack_mapping(root, &pack, &built, &built.join("catalog.sqlite3"), true)?;
    for entry in pack.metadata.values() {
        exact_file(&under(root, &built, &entry.path)?, entry)?;
    }
    publication::publish_directory(
        stage,
        &built,
        &target,
        None,
        |_| Ok(Value::Null),
        || {
            pack.verify_manifest()?;
            let metadata_archive = BoundFile::parse(&pack.value["metadata"], MAX_ARCHIVE_BYTES)?;
            let archive = under(root, pack.base(), &metadata_archive.path)?;
            if archive.try_exists()? {
                exact_file(&archive, &metadata_archive)?;
            } else {
                for entry in pack.metadata.values() {
                    exact_file(&under(root, pack.base(), &entry.path)?, entry)?;
                }
            }
            verify_pack_archives(root, &pack)
        },
    )?;
    Ok(
        json!({"status":"installed","segments":pack.value["segments"].as_array().expect("validated").len(),"pages_available":closure.required.iter().filter(|f|f.role=="source_page_pdf").count(),"pages_expanded":0,"full_pdfs_installed":installed_full,"full_pdfs_archive_preserved":full.is_some(),"storage":"compressed_shards_plus_index; selected_pages_restored_at_use"}),
    )
}

fn relative_to(base: &Path, target: &Path) -> Result<String> {
    let a: Vec<_> = base.components().collect();
    let b: Vec<_> = target.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    ensure!(common > 0, "source_registration_path_invalid");
    let mut parts = vec!["..".to_string(); a.len() - common];
    for c in &b[common..] {
        let Component::Normal(s) = c else {
            anyhow::bail!("source_registration_path_invalid")
        };
        parts.push(
            s.to_str()
                .context("source_registration_path_invalid")?
                .into(),
        );
    }
    ensure!(!parts.is_empty(), "source_registration_path_invalid");
    Ok(parts.join("/"))
}

fn resolve_registration(root: &SkillRoot, base: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(
        !relative.is_empty()
            && !relative.contains(['\\', ':'])
            && !relative.chars().any(char::is_control)
            && !Path::new(relative).is_absolute(),
        "source_registration_path_invalid"
    );
    let mut path = base.to_path_buf();
    for part in relative.split('/') {
        ensure!(
            !part.is_empty() && part != ".",
            "source_registration_path_invalid"
        );
        if part == ".." {
            ensure!(path.pop(), "source_registration_path_invalid");
        } else {
            path.push(part);
        }
    }
    source_path(root, &path)
}

fn read_registration(path: &Path) -> Result<(Vec<u8>, Value)> {
    let raw = oe_core::read_regular_bytes(path, source_pack::MAX_MANIFEST_BYTES)?;
    let value = strict_json(&raw)?;
    let object = value.as_object().context("invalid_source_registration")?;
    let required = [
        "schema",
        "archive_root",
        "index_path",
        "index_sha256",
        "source_catalogs",
        "verification",
    ];
    let allowed = [
        "schema",
        "archive_root",
        "index_path",
        "index_sha256",
        "source_catalogs",
        "verification",
        "source_pack_manifest",
        "source_pack_manifest_sha256",
    ];
    ensure!(
        value["schema"] == LOCATION_SCHEMA
            && required.iter().all(|k| object.contains_key(*k))
            && object.keys().all(|k| allowed.contains(&k.as_str())),
        "invalid_source_registration"
    );
    ensure!(
        value["index_sha256"]
            .as_str()
            .is_some_and(source_pack::valid_sha)
            && value["verification"]
                .as_str()
                .is_some_and(|s| !s.is_empty()),
        "invalid_source_registration"
    );
    ensure!(
        object.contains_key("source_pack_manifest")
            == object.contains_key("source_pack_manifest_sha256"),
        "invalid_source_registration"
    );
    Ok((raw, value))
}

/// Register existing expanded sources without copying PDF bytes.
pub fn register(root: &SkillRoot, archive: &Path, index: &Path, target: &Path) -> Result<Value> {
    let archive = source_path(root, archive)?;
    let index = source_path(root, index)?;
    let target = target_path(root, target)?;
    ensure!(
        !target.join("source.json").try_exists()?,
        "source_registration_already_exists"
    );
    ensure!(
        !target.join("source-pack.json").try_exists()?
            && !target.join("data-manifest.json").try_exists()?,
        "source_registration_conflicts_existing_library"
    );
    let closure = mapping(root, &archive, &index, true)?;
    require_source_mode(root, &closure, None)?;
    let location = json!({"schema":LOCATION_SCHEMA,"archive_root":relative_to(&target,&archive)?,"index_path":relative_to(&target,&index)?,"index_sha256":closure.index_identity["sha256"],"source_catalogs":closure.identity,"verification":"source_mapping_checked; source_bytes_checked_at_use_or_verify"});
    let stage = Stage::new(root.path(), "supplier-register")?;
    let file = stage.path.join("source.json");
    oe_core::write_new(&file, &location)?;
    ensure!(
        mapping(root, &archive, &index, true)?.index_identity == closure.index_identity,
        "registered_index_changed"
    );
    if !target.try_exists()? {
        create_new_private_dir(&target)?;
    }
    let (sha, size) = file_hash(&file, source_pack::MAX_MANIFEST_BYTES as u64)?;
    source_pack::publish_same_or_new(&file, &target.join("source.json"), &sha, size)?;
    Ok(json!({"status":"registered","copied_source_files":0,"source_catalogs":closure.identity}))
}

/// Import an expanded archive and its existing index into a fresh closed library.
pub fn import_data(root: &SkillRoot, archive: &Path, index: &Path, target: &Path) -> Result<Value> {
    let archive = source_path(root, archive)?;
    let index = source_path(root, index)?;
    let target = target_path(root, target)?;
    ensure!(
        !target.try_exists()? && !target.starts_with(&archive) && !archive.starts_with(&target),
        "data_root_already_exists_or_overlaps_source"
    );
    let closure = mapping(root, &archive, &index, true)?;
    require_source_mode(root, &closure, None)?;
    let stage = Stage::new(root.path(), "supplier-import")?;
    let built = stage.path.join("library");
    create_new_private_dir(&built)?;
    let mut files = Vec::new();
    for required in &closure.required {
        let bytes = oe_core::open_regular_file(&required.source, MAX_ARCHIVE_BYTES)?
            .metadata()?
            .len();
        ensure!(
            required.bytes.is_none_or(|n| n == bytes),
            "source_page_size_mismatch"
        );
        let entry = BoundFile {
            path: required.relative.clone(),
            sha: required.sha.clone(),
            bytes,
        };
        copy_bound(&required.source, &under(root, &built, &entry.path)?, &entry)?;
        let media = if entry.path.ends_with(".pdf") {
            "application/pdf"
        } else if entry.path.ends_with(".json") {
            "application/json"
        } else {
            "application/vnd.sqlite3"
        };
        let mut value = entry.value();
        value["media_type"] = json!(media);
        value["role"] = json!(required.role);
        files.push(value);
    }
    let manifest = json!({"schema":DATA_SCHEMA,"source_catalogs":closure.identity,"files":files,"scope":"private development candidate; supplier source material retains its rights; no project records or credentials"});
    oe_core::write_new(&built.join("data-manifest.json"), &manifest)?;
    verify_expanded(root, &built, &manifest)?;
    publication::publish_directory(
        stage,
        &built,
        &target,
        None,
        |_| Ok(Value::Null),
        || {
            ensure!(
                mapping(root, &archive, &index, true)?.index_identity == closure.index_identity,
                "registered_index_changed"
            );
            Ok(())
        },
    )?;
    Ok(
        json!({"status":"imported","files":files.len(),"bytes":files.iter().map(|v|v["bytes"].as_u64().expect("bound")).sum::<u64>(),"source_catalogs":closure.identity}),
    )
}

fn verify_expanded(root: &SkillRoot, target: &Path, manifest: &Value) -> Result<Value> {
    ensure!(
        manifest["schema"] == DATA_SCHEMA,
        "data_manifest_schema_mismatch"
    );
    let mut declared = BTreeMap::new();
    for value in manifest["files"]
        .as_array()
        .context("data_manifest_files_missing")?
    {
        let entry = BoundFile::parse(value, MAX_ARCHIVE_BYTES)?;
        exact_file(&under(root, target, &entry.path)?, &entry)?;
        ensure!(
            declared.insert(entry.path.clone(), entry).is_none(),
            "duplicate_data_path"
        );
    }
    let closure = mapping(
        root,
        &target.join("archive"),
        &target.join("catalog.sqlite3"),
        true,
    )?;
    require_source_mode(root, &closure, None)?;
    ensure!(
        declared.keys().cloned().collect::<BTreeSet<_>>()
            == closure
                .required
                .iter()
                .map(|f| f.relative.clone())
                .collect()
            && manifest["source_catalogs"] == closure.identity,
        "data_manifest_closure_mismatch"
    );
    for required in closure.required {
        let value = &declared[&required.relative];
        ensure!(
            value.sha == required.sha && required.bytes.is_none_or(|b| b == value.bytes),
            "data_source_hash_mismatch"
        );
    }
    Ok(json!({"status":"verified","files":declared.len(),"source_catalogs":closure.identity}))
}

/// Stream-hash source identities and recheck metadata/index/member mappings.
pub fn verify(root: &SkillRoot, target: &Path) -> Result<Value> {
    let target = target_path(root, target)?;
    let layout = library_layout(&target)?;
    if layout == "source-pack.json" {
        let pack = Pack::load(root, &target.join("source-pack.json"))?;
        for entry in pack.metadata.values() {
            exact_file(&under(root, &target, &entry.path)?, entry)?;
        }
        verify_pack_mapping(root, &pack, &target, &target.join("catalog.sqlite3"), true)?;
        verify_pack_archives(root, &pack)?;
        verify_optional_metadata_archive(root, &pack)?;
        return Ok(
            json!({"status":"verified","segments":pack.value["segments"].as_array().expect("validated").len(),"scope":"metadata_index_and_compressed_shard_closure; requested_page_bytes_reverified_at_use","expanded_all_pages":false}),
        );
    }
    let path = target.join("source.json");
    if path.try_exists()? {
        let (raw, location) = read_registration(&path)?;
        let index = resolve_registration(
            root,
            &target,
            location["index_path"]
                .as_str()
                .context("invalid_source_registration")?,
        )?;
        ensure!(
            file_hash(&index, MAX_INDEX_BYTES)?.0 == location["index_sha256"],
            "registered_index_changed"
        );
        let archive = resolve_registration(
            root,
            &target,
            location["archive_root"]
                .as_str()
                .context("invalid_source_registration")?,
        )?;
        let closure = if let Some(p) = location.get("source_pack_manifest") {
            let pack = Pack::load(
                root,
                &resolve_registration(
                    root,
                    &target,
                    p.as_str().context("invalid_source_registration")?,
                )?,
            )?;
            ensure!(
                oe_core::sha256(&pack.raw) == location["source_pack_manifest_sha256"],
                "registered_source_pack_changed"
            );
            require_pack_archive(&pack, &archive)?;
            let stage = Stage::new(root.path(), "supplier-verify")?;
            extract_metadata(root, &pack, &stage.path, false)?;
            let closure = verify_pack_mapping(root, &pack, &stage.path, &index, false)?;
            verify_pack_archives(root, &pack)?;
            closure
        } else {
            let closure = mapping(root, &archive, &index, true)?;
            require_source_mode(root, &closure, None)?;
            for f in &closure.required {
                ensure!(
                    file_hash(&f.source, MAX_ARCHIVE_BYTES)?.0 == f.sha,
                    "source_hash_mismatch"
                );
            }
            closure
        };
        ensure!(
            closure.identity == location["source_catalogs"],
            "registered_source_identity_changed"
        );
        ensure!(
            oe_core::read_regular_bytes(&path, source_pack::MAX_MANIFEST_BYTES)? == raw,
            "source_registration_changed"
        );
        return Ok(
            json!({"status":"verified","files":closure.required.len(),"source_catalogs":closure.identity,"storage":"registered_source_archive; no copied PDFs in skill"}),
        );
    }
    let path = target.join("data-manifest.json");
    let raw = oe_core::read_regular_bytes(&path, source_pack::MAX_MANIFEST_BYTES)?;
    let result = verify_expanded(root, &target, &strict_json(&raw)?)?;
    ensure!(
        oe_core::read_regular_bytes(&path, source_pack::MAX_MANIFEST_BYTES)? == raw,
        "data_manifest_changed"
    );
    Ok(result)
}

/// Explicit restoration of full original books; all archive members are
/// verified before any new final book is published. Existing bytes must match.
pub fn restore_books(root: &SkillRoot, target: &Path) -> Result<Value> {
    let target = target_path(root, target)?;
    let layout = library_layout(&target)?;
    let mut registration = None;
    let (pack, destination_base) = if layout == "source-pack.json" {
        let pack = Pack::load(root, &target.join("source-pack.json"))?;
        for entry in pack.metadata.values() {
            exact_file(&under(root, &target, &entry.path)?, entry)?;
        }
        verify_pack_mapping(root, &pack, &target, &target.join("catalog.sqlite3"), true)?;
        verify_optional_metadata_archive(root, &pack)?;
        (pack, target.join("cache/archive"))
    } else {
        let path = target.join("source.json");
        let (raw, location) = read_registration(&path)?;
        let manifest = resolve_registration(
            root,
            &target,
            location["source_pack_manifest"]
                .as_str()
                .context("full_pdf_archive_not_declared")?,
        )?;
        let pack = Pack::load(root, &manifest)?;
        ensure!(
            oe_core::sha256(&pack.raw) == location["source_pack_manifest_sha256"],
            "registered_source_pack_changed"
        );
        let destination = resolve_registration(
            root,
            &target,
            location["archive_root"]
                .as_str()
                .context("invalid_source_registration")?,
        )?;
        require_pack_archive(&pack, &destination)?;
        let index = resolve_registration(
            root,
            &target,
            location["index_path"]
                .as_str()
                .context("invalid_source_registration")?,
        )?;
        ensure!(
            file_hash(&index, MAX_INDEX_BYTES)?.0 == location["index_sha256"],
            "registered_index_changed"
        );
        let metadata = Stage::new(root.path(), "supplier-books-metadata")?;
        extract_metadata(root, &pack, &metadata.path, false)?;
        let closure = verify_pack_mapping(root, &pack, &metadata.path, &index, false)?;
        ensure!(
            closure.identity == location["source_catalogs"],
            "registered_source_identity_changed"
        );
        registration = Some((path, raw));
        (pack, destination)
    };
    let (descriptor, files) = pack
        .full_archive()?
        .context("full_pdf_archive_not_declared")?;
    let binding = BoundFile::parse(&descriptor, MAX_ARCHIVE_BYTES)?;
    let archive = under(root, pack.base(), &binding.path)?;
    exact_file(&archive, &binding)?;
    let mut destinations = BTreeMap::new();
    for assembly in pack.value["assembled_pdfs"].as_array().expect("validated") {
        let name = assembly["download_path"]
            .as_str()
            .context("full_pdf_archive_inventory_mismatch")?;
        let target = under(
            root,
            &destination_base,
            assembly["path"]
                .as_str()
                .context("source_pack_invalid_file")?,
        )?;
        if target.try_exists()? {
            exact_file(&target, &files[name])?;
        }
        destinations.insert(name.to_owned(), target);
    }
    let verify_registration = || -> Result<()> {
        if let Some((path, raw)) = &registration {
            ensure!(
                oe_core::read_regular_bytes(path, source_pack::MAX_MANIFEST_BYTES)? == *raw,
                "source_registration_changed"
            );
        }
        pack.verify_manifest()
    };
    if destinations.values().all(|p| p.is_file()) {
        verify_registration()?;
        return Ok(
            json!({"status":"verified","restored":0,"full_pdfs":destinations.values().collect::<Vec<_>>()}),
        );
    }
    let stage = Stage::new(root.path(), "supplier-books")?;
    let total = files.values().try_fold(0u64, |n, f| {
        n.checked_add(f.bytes)
            .context("full_pdf_archive_size_overflow")
    })?;
    source_pack::visit_archive(
        root.path(),
        &archive,
        &descriptor,
        &ArchiveLimits {
            max_file_bytes: MAX_ARCHIVE_BYTES,
            max_members: files.len(),
            max_uncompressed_bytes: total,
        },
        None,
        |member| {
            copy_bound(
                &member.path,
                &under(root, &stage.path, &member.relative_path)?,
                &files[&member.relative_path],
            )
        },
    )?;
    verify_registration()?;
    let mut restored = 0;
    for (name, destination) in &destinations {
        let entry = &files[name];
        let existed = destination.try_exists()?;
        source_pack::publish_same_or_new(
            &under(root, &stage.path, name)?,
            destination,
            &entry.sha,
            entry.bytes,
        )?;
        if !existed {
            restored += 1;
        }
    }
    Ok(
        json!({"status":"restored","restored":restored,"full_pdfs":destinations.values().collect::<Vec<_>>() }),
    )
}
