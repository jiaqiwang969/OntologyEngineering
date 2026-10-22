//! v2 source transport and private publication adapter for the neutral library.
//! The frozen wheel is read as a data archive; none of its Python is executed.
pub mod source_execution;
use super::{BookRequest, EngraveOptions, SCHEMA};
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256};
use semantica_runtime::books::{Catalog, MigrationEncoding, NormativeInput, SourceProvider};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const ARCHIVE: &str = "sources/history/runtime/vendor/semantica-0.6.5+oe.7-py3-none-any.whl";
/// Approved oe.7 book data edition, independent of the mutable continuity
/// receipt. A different release needs an explicit version migration and pin.
pub const APPROVED_ARCHIVE_SHA256: &str =
    "142ddd6fe788b3a716646babc410628aa7606e44ed63b98f5227863f9ea7ba15";
const PREFIX: &str = "semantica/chapter_packages";
const PART1: &str =
    "part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json";
const PART3: &str =
    "part-03-concept-phase/native-full/ISO 26262-3-2018/auto/ISO 26262-3-2018_content_list_v2.json";
const GLOSSARY: &str = "appendices/appendix-c-glossary.md";
const MAX_FILE: usize = 16 * 1024 * 1024;

pub fn archive_bytes(root: &Path) -> Result<Vec<u8>> {
    let raw = read_regular_bytes(&root.join(ARCHIVE), 32 * 1024 * 1024)?;
    ensure!(
        sha256(&raw) == APPROVED_ARCHIVE_SHA256,
        "book_archive_not_approved_source_release"
    );
    let continuity = crate::strict_json(&read_regular_bytes(
        &root.join("sources/continuity.json"),
        4 * 1024 * 1024,
    )?)?;
    let matches: Vec<_> = continuity["source_files"]
        .as_array()
        .context("chapter_source_inventory_missing")?
        .iter()
        .filter(|r| r["path"] == ARCHIVE.trim_start_matches("sources/"))
        .collect();
    ensure!(
        matches.len() == 1 && matches[0]["sha256"] == sha256(&raw),
        "chapter_archive_identity_drift_or_not_registered"
    );
    Ok(raw)
}

pub struct ArchiveProvider {
    root: PathBuf,
    archive_sha: String,
    zip: RefCell<zip::ZipArchive<Cursor<Vec<u8>>>>,
}
impl ArchiveProvider {
    pub fn open(root: &Path) -> Result<Self> {
        let raw = archive_bytes(root)?;
        let hash = sha256(&raw);
        let mut zip = zip::ZipArchive::new(Cursor::new(raw))?;
        let mut names = std::collections::BTreeSet::new();
        ensure!(zip.len() <= 8192, "book_archive_entry_budget");
        for i in 0..zip.len() {
            let entry = zip.by_index(i)?;
            ensure!(
                names.insert(entry.name().to_owned()),
                "book_archive_duplicate_entry"
            );
            if entry.name().starts_with(&format!("{PREFIX}/")) && !entry.is_dir() {
                semantica_runtime::books::relative(entry.name())?;
                ensure!(
                    entry.unix_mode().is_none_or(|m| m & 0o170000 != 0o120000),
                    "book_archive_symlink"
                );
            }
        }
        Ok(Self {
            root: root.to_owned(),
            archive_sha: hash,
            zip: RefCell::new(zip),
        })
    }
    pub fn verify(&self) -> Result<()> {
        ensure!(
            sha256(archive_bytes(&self.root)?) == self.archive_sha,
            "book_archive_changed_during_call"
        );
        Ok(())
    }
    pub fn catalog(&self) -> Result<Catalog> {
        Catalog::read(self, &format!("{PREFIX}/registry.yaml"))
    }
    pub fn archive_sha256(&self) -> &str {
        &self.archive_sha
    }
}
impl SourceProvider for ArchiveProvider {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        semantica_runtime::books::relative(id)?;
        ensure!(
            id.starts_with(&format!("{PREFIX}/")),
            "book_archive_read_outside_catalog"
        );
        let mut zip = self.zip.borrow_mut();
        let mut file = zip.by_name(id)?;
        ensure!(
            !file.is_dir() && file.size() <= MAX_FILE as u64,
            "book_archive_asset_budget_or_directory"
        );
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_FILE, "book_archive_asset_budget");
        Ok(bytes)
    }
    fn metadata(&self, bytes: &[u8]) -> Result<Value> {
        // Real YAML parsing is required by the released gloss seed. JSON
        // manifests are also accepted by the same safe Rust decoder.
        let value: serde_yaml::Value = serde_yaml::from_slice(bytes)?;
        ensure!(!contains_tag(&value), "book_metadata_yaml_tags_unsupported");
        Ok(serde_json::to_value(value)?)
    }
}
fn contains_tag(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Tagged(_) => true,
        serde_yaml::Value::Sequence(a) => a.iter().any(contains_tag),
        serde_yaml::Value::Mapping(m) => m.iter().any(|(k, v)| contains_tag(k) || contains_tag(v)),
        _ => false,
    }
}

pub(crate) struct FileProvider {
    roots: BTreeMap<String, PathBuf>,
    inputs: RefCell<BTreeMap<PathBuf, String>>,
    attempted: RefCell<BTreeSet<PathBuf>>,
}
impl FileProvider {
    pub(crate) fn new(roots: BTreeMap<String, PathBuf>) -> Result<Self> {
        for root in roots.values() {
            no_symlinks(root)?;
            ensure!(root.is_dir(), "book_input_directory_missing");
        }
        Ok(Self {
            roots,
            inputs: RefCell::new(BTreeMap::new()),
            attempted: RefCell::new(BTreeSet::new()),
        })
    }
    pub(crate) fn verify(&self) -> Result<()> {
        for (path, hash) in self.inputs.borrow().iter() {
            ensure!(
                sha256(read_regular_bytes(path, MAX_FILE)?) == *hash,
                "book_input_changed_during_call"
            );
        }
        for path in self.attempted.borrow().iter() {
            if !self.inputs.borrow().contains_key(path) {
                no_symlinks(path)?;
                ensure!(
                    !path.try_exists()?,
                    "book_missing_input_changed_during_call"
                );
            }
        }
        Ok(())
    }
    fn hashes(&self) -> Value {
        json!(
            self.inputs
                .borrow()
                .iter()
                .map(|(p, h)| (p.to_string_lossy().to_string(), h.clone()))
                .collect::<BTreeMap<_, _>>()
        )
    }
}
impl SourceProvider for FileProvider {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        let (group, relative) = id
            .split_once('/')
            .context("book_file_source_group_missing")?;
        semantica_runtime::books::relative(relative)?;
        let root = self
            .roots
            .get(group)
            .context("book_file_source_group_unknown")?;
        let path = root.join(relative);
        self.attempted.borrow_mut().insert(path.clone());
        no_symlinks(&path)?;
        let bytes = read_regular_bytes(&path, MAX_FILE)?;
        let hash = sha256(&bytes);
        let mut inputs = self.inputs.borrow_mut();
        if let Some(old) = inputs.get(&path) {
            ensure!(old == &hash, "book_input_changed_during_call");
        }
        ensure!(inputs.len() < 8192, "book_input_file_budget");
        inputs.insert(path, hash);
        Ok(bytes)
    }
}
struct BookAnchors<'a>(&'a FileProvider);
impl SourceProvider for BookAnchors<'_> {
    fn read(&self, anchor: &str) -> Result<Vec<u8>> {
        let anchor = anchor
            .strip_prefix("ontology-engineering/")
            .unwrap_or(anchor);
        semantica_runtime::books::relative(anchor)?;
        ensure!(
            anchor.starts_with("references/ontology-engineering-book/")
                || anchor.starts_with("references/product-trustworthiness-book/"),
            "book_anchor_outside_authoring_corpus"
        );
        self.0.read(&format!("book/{anchor}"))
    }
}
fn map(catalog: &Catalog, provider: &ArchiveProvider, volume: &str) -> Result<Value> {
    let (path, encoding) = match volume {
        "vol1" => ("vol1/migration-map.yaml", MigrationEncoding::LegacyAssets),
        "vol2" => (
            "vol2/migration-map.json",
            MigrationEncoding::SuccessorMappings,
        ),
        _ => anyhow::bail!("book_volume_not_registered"),
    };
    catalog.read_migration_map(provider, volume, &format!("{PREFIX}/{path}"), encoding)
}
pub fn migration_map(root: &Path, volume: &str) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let result = map(&catalog, &provider, volume)?;
    provider.verify()?;
    Ok(result)
}
pub fn resolve_successor(root: &Path, old_path: &str, volume: Option<&str>) -> Result<Value> {
    crate::safe_relative(old_path)?;
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let maps = volume
        .map_or_else(|| vec!["vol1", "vol2"], |v| vec![v])
        .into_iter()
        .map(|v| map(&catalog, &provider, v))
        .collect::<Result<Vec<_>>>()?;
    let result = Catalog::resolve_successor(old_path, &maps)?;
    provider.verify()?;
    Ok(result)
}
pub fn asset_text(root: &Path, package: &str, asset: &str) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let result = if catalog.read_manifest(package).is_ok() {
        catalog.package_asset_text(&provider, package, asset)?
    } else {
        // Current native Rust package assets use the same exact lookup contract.
        let lock = crate::strict_json(&read_regular_bytes(
            &root.join(crate::LOCK_PATH),
            4 * 1024 * 1024,
        )?)?;
        let names: Vec<_> = lock["bundles"]
            .as_object()
            .context("book_bundle_inventory_missing")?
            .iter()
            .filter(|(_, spec)| spec["package_id"] == package)
            .map(|(name, _)| name)
            .collect();
        ensure!(names.len() == 1, "book_package_not_uniquely_registered");
        let bundle = crate::load_bundle(root, names[0])?;
        let selected: Vec<_> = bundle.manifest["assets"]
            .as_array()
            .context("book_assets_missing")?
            .iter()
            .filter(|a| a["asset_id"] == asset)
            .collect();
        ensure!(selected.len() == 1, "book_asset_not_uniquely_declared");
        let path = selected[0]["path"]
            .as_str()
            .context("book_asset_path_missing")?;
        let source = bundle.payload.get(path).context("book_asset_unknown")?;
        let value = std::str::from_utf8(source).context("book_asset_not_utf8")?;
        ensure!(!value.contains('\0'), "book_asset_binary");
        let result = json!({"package_id":package,"asset_id":asset,"text":value,"sha256":sha256(source),"size_bytes":source.len()});
        ensure!(
            crate::load_bundle(root, names[0])?.spec == bundle.spec,
            "book_bundle_source_changed_during_call"
        );
        result
    };
    provider.verify()?;
    Ok(result)
}
pub fn chapter_asset_text(root: &Path, volume: &str, chapter: &str, asset: &str) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let result = catalog.chapter_asset_text(&provider, volume, chapter, asset)?;
    provider.verify()?;
    Ok(result)
}
pub fn verify_sources(root: &Path) -> Result<Value> {
    verify_sources_with_volume(root, None)
}
pub fn verify_sources_with_volume(root: &Path, volume: Option<&str>) -> Result<Value> {
    ensure!(
        volume.is_none_or(|v| ["vol1", "vol2"].contains(&v)),
        "book_volume_not_registered"
    );
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let books = root.join("sources/book-authoring");
    let books = if books.is_dir() {
        books
    } else {
        root.to_owned()
    };
    let files = FileProvider::new(BTreeMap::from([("book".into(), books)]))?;
    let result =
        catalog.verify_book_source_bindings(&provider, &BookAnchors(&files), volume, true, true)?;
    files.verify()?;
    provider.verify()?;
    Ok(result)
}
pub fn validate_chapter_registry(root: &Path, require_complete: bool) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let issues = catalog.validate_chapter_registry(&provider, require_complete)?;
    let result = json!({"schema_version":"1.0","issues":issues,"valid":issues.is_empty(),"chapters":catalog.list_chapter_packages(None),"domains":catalog.list_domain_packages(),"release_status":"not_inferred_from_registry_validity"});
    provider.verify()?;
    Ok(result)
}
pub fn list_chapter_packages(root: &Path, volume: Option<&str>) -> Result<Vec<Value>> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let result = catalog.list_chapter_packages(volume);
    provider.verify()?;
    Ok(result)
}
pub fn list_domain_packages(root: &Path) -> Result<Vec<Value>> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let result = catalog.list_domain_packages();
    provider.verify()?;
    Ok(result)
}
pub fn read_chapter_manifest(root: &Path, volume: &str, chapter: &str) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let result = catalog
        .read_manifest(catalog.chapter_package_id(volume, chapter)?)?
        .clone();
    provider.verify()?;
    Ok(result)
}
pub fn read_domain_manifest(root: &Path, package_id: &str) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    ensure!(
        catalog
            .list_domain_packages()
            .iter()
            .any(|d| d["package_id"] == package_id),
        "book_domain_not_registered"
    );
    let result = catalog.read_manifest(package_id)?.clone();
    provider.verify()?;
    Ok(result)
}
pub fn read_chapter_policy(root: &Path) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let result = catalog.read_policy().clone();
    provider.verify()?;
    Ok(result)
}
pub fn load_chapter_package(
    root: &Path,
    volume: &str,
    chapter: &str,
    session: &mut semantica_runtime::dataset::DatasetSession,
) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let mut staged = session.clone();
    let result = catalog.load_chapter_package(&provider, volume, chapter, &mut staged)?;
    provider.verify()?;
    *session = staged;
    Ok(result)
}
pub fn load_domain_package(
    root: &Path,
    package: &str,
    session: &mut semantica_runtime::dataset::DatasetSession,
) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let mut staged = session.clone();
    let result = catalog.load_domain_package(&provider, package, &mut staged)?;
    provider.verify()?;
    *session = staged;
    Ok(result)
}
pub fn execute_chapter_query(
    root: &Path,
    volume: &str,
    chapter: &str,
    asset: &str,
    session: &mut semantica_runtime::dataset::DatasetSession,
    namespaces: &BTreeMap<String, String>,
    bindings: &BTreeMap<String, semantica_runtime::books::RdfTerm>,
) -> Result<Value> {
    let provider = ArchiveProvider::open(root)?;
    let catalog = provider.catalog()?;
    let mut staged = session.clone();
    let result = catalog.execute_chapter_query(
        &provider,
        volume,
        chapter,
        asset,
        &mut staged,
        namespaces,
        bindings,
    )?;
    provider.verify()?;
    *session = staged;
    Ok(result)
}
fn response(operation: &str, result: Value, archive: &str) -> Value {
    json!({"schema":SCHEMA,"operation":operation,"status":if result.get("passed").is_some_and(|v|v!=true){"blocked"}else{"completed"},"scope":"source-bound Rust book API; project fact adoption requires separate evidence","semantic_execution":"completed","engine":"semantica.rust-runtime","source_archive_sha256":archive,"fact_admitted":false,"ontology_promotion":"not_performed","publication":"not_performed","result":result})
}
pub fn execute(root: &SkillRoot, request: &BookRequest) -> Result<Value> {
    let (operation, result) = match request {
        BookRequest::MigrationMap { volume } => {
            ("book_migration_map", migration_map(root.path(), volume)?)
        }
        BookRequest::ResolveSuccessor { old_path, volume } => (
            "book_resolve_successor",
            resolve_successor(root.path(), old_path, volume.as_deref())?,
        ),
        BookRequest::AssetText {
            package_id,
            asset_id,
        } => (
            "book_asset_text",
            asset_text(root.path(), package_id, asset_id)?,
        ),
        BookRequest::ChapterAssetText {
            volume,
            chapter,
            asset_id,
        } => (
            "book_chapter_asset_text",
            chapter_asset_text(root.path(), volume, chapter, asset_id)?,
        ),
        BookRequest::VerifyBookSources => return source_execution::verify_response(root),
        BookRequest::Engrave(options) => return engrave(root, options),
    };
    Ok(response(
        operation,
        result,
        &sha256(archive_bytes(root.path())?),
    ))
}

fn overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
fn directory(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.expand(path)?;
    no_symlinks(&path)?;
    ensure!(
        path.starts_with(root.path()) && path.is_dir(),
        "book_input_directory_outside_root_or_missing"
    );
    Ok(path)
}
static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn engrave(root: &SkillRoot, options: &EngraveOptions) -> Result<Value> {
    let source = directory(root, &options.source_root)?;
    ensure!(
        source.starts_with(root.path().join("var"))
            || source.starts_with(root.path().join("sources")),
        "controlled_sources_must_be_private_or_source_library"
    );
    let book = directory(root, &options.book_root)?;
    let output = root.private_path(&options.output)?;
    ensure!(
        !overlap(&output, &source)
            && !overlap(&output, &book)
            && !overlap(&output, &root.path().join("sources/history")),
        "engraving_output_overlaps_input"
    );
    let parent = output.parent().context("engraving_output_parent_missing")?;
    no_symlinks(parent)?;
    ensure!(parent.is_dir(), "engraving_output_parent_missing");
    ensure!(
        !output.exists() || (options.overwrite && output.is_dir()),
        "engraving_existing_output_requires_overwrite"
    );
    let provider = ArchiveProvider::open(root.path())?;
    let catalog = provider.catalog()?;
    let files = FileProvider::new(BTreeMap::from([
        ("source".into(), source),
        ("book".into(), book),
    ]))?;
    let input = NormativeInput {
        package_id: "semantica.chapter_packages.vol2.normative".into(),
        extracts: BTreeMap::from([
            (1, format!("source/{PART1}")),
            (3, format!("source/{PART3}")),
        ]),
        glossary_source: Some(format!("book/{GLOSSARY}")),
        parts: options.parts.clone(),
    };
    let mut prepared =
        semantica_runtime::books::prepare_normative_engraving(&catalog, &provider, &files, &input)?;
    files.verify()?;
    provider.verify()?;
    // No output bytes are written until every input and prospective RDF file
    // has passed actual Rust parsing and the entire target set is preflighted.
    for name in prepared.artifacts.keys() {
        let path = output.join(name);
        no_symlinks(&path)?;
        ensure!(
            !path.exists() || (options.overwrite && std::fs::symlink_metadata(&path)?.is_file()),
            "engraving_target_not_replaceable"
        );
    }
    let stage = parent.join(format!(
        ".engraving-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    oe_core::create_new_private_dir(&stage)?;
    let staged = (|| -> Result<()> {
        for (name, bytes) in &prepared.artifacts {
            oe_core::write_new_bytes(&stage.join(name), bytes)?;
        }
        files.verify()?;
        provider.verify()?;
        if !output.exists() {
            oe_core::rename_directory_exclusive(&stage, &output)?;
        } else {
            ensure!(
                options.overwrite,
                "engraving_existing_output_requires_overwrite"
            );
            no_symlinks(&output)?;
            ensure!(output.is_dir(), "engraving_output_not_directory");
            // The completion report is replaced last. Per-file publication
            // preserves unrelated files and makes no atomic-batch claim.
            for name in prepared
                .artifacts
                .keys()
                .filter(|n| n.as_str() != "engraving-report.json")
            {
                oe_core::rename_regular_file(&stage.join(name), &output.join(name))?;
            }
            oe_core::rename_regular_file(
                &stage.join("engraving-report.json"),
                &output.join("engraving-report.json"),
            )?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&stage);
    staged?;
    for a in prepared.result["artifacts"]
        .as_array()
        .context("engraving_receipt_artifacts_missing")?
    {
        let name = a["relative_path"]
            .as_str()
            .context("engraving_receipt_path_missing")?;
        let bytes = read_regular_bytes(&output.join(name), MAX_FILE)?;
        ensure!(
            a["sha256"] == sha256(&bytes) && a["size_bytes"].as_u64() == Some(bytes.len() as u64),
            "engraving_publication_readback_mismatch"
        );
    }
    prepared.result["output_directory"] = json!(output);
    let mut receipt = response("book_engrave", prepared.result, provider.archive_sha256());
    receipt["input_sha256"] = files.hashes();
    receipt["output"] = json!(output);
    receipt["output_state"] = json!("source_verified_artifacts_written_and_read_back");
    Ok(receipt)
}
