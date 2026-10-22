//! Book-source rebinding for Semantica's 29 chapter packages.
//!
//! Ported from scripts/rebind_semantica_books.py. Updates source/TeX digests
//! in an explicit Semantica source checkout. This is a release-engineering tool,
//! not a runtime path. No semantic execution; filesystem reads and JSON writes only.

use anyhow::{Context, Result, bail, ensure};
use oe_core::sha256;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const EXPECTED_PACKAGES: usize = 29;

// Vol1 chapter → directory name
const VOL1_DIRS: &[(&str, &str)] = &[
    ("ch01", "ch01-introduction"),
    ("ch02", "ch02-ontology-foundations"),
    ("ch03", "ch03-ontology-methodology"),
    ("ch04", "ch04-ontology-languages"),
    ("ch05", "ch05-reasoning"),
    ("ch06", "ch06-applications"),
    ("ch07", "ch07-knowledge-graph"),
    ("ch08", "ch08-ontology-llm"),
    ("ch09", "ch09-capstone-manufacturing"),
];

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RebindEntry {
    pub id: String,
    pub changed: bool,
    pub error: Option<String>,
}

pub struct RebindResult {
    pub entries: Vec<RebindEntry>,
    pub changed: Vec<String>,
    pub errors: Vec<String>,
    pub written: bool,
}

impl RebindResult {
    pub fn to_json(&self) -> Value {
        json!({
            "changed_count": self.changed.len(),
            "error_count": self.errors.len(),
            "written": self.written,
            "changed": self.changed,
            "errors": self.errors,
        })
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

fn relative_book_path(book_root: &Path, anchor: &str) -> Result<PathBuf> {
    let relative = Path::new(anchor);
    ensure!(
        !relative.is_absolute(),
        "absolute book anchor is forbidden: {anchor}"
    );

    // Strip leading "ontology-engineering/" prefix if present
    let stripped: PathBuf = {
        let parts: Vec<_> = relative.components().collect();
        if parts.first().and_then(|c| c.as_os_str().to_str()) == Some("ontology-engineering") {
            parts[1..].iter().collect()
        } else {
            relative.to_path_buf()
        }
    };

    let root = book_root.canonicalize().context("book_root canonicalize")?;
    let result = root.join(&stripped);
    let resolved = result
        .canonicalize()
        .with_context(|| format!("anchor not found: {anchor}"))?;

    ensure!(
        resolved.starts_with(&root),
        "book anchor escapes repository: {anchor}"
    );
    Ok(resolved)
}

fn sha_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("cannot read: {}", path.display()))?;
    Ok(sha256(&bytes))
}

fn read_json(path: &Path) -> Result<serde_json::Map<String, Value>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read: {}", path.display()))?;
    let v: Value =
        serde_json::from_str(&text).with_context(|| format!("invalid JSON: {}", path.display()))?;
    v.as_object()
        .cloned()
        .with_context(|| format!("expected JSON object: {}", path.display()))
}

fn write_json(path: &Path, obj: &serde_json::Map<String, Value>) -> Result<()> {
    let encoded = serde_json::to_string_pretty(&Value::Object(obj.clone()))? + "\n";
    std::fs::write(path, encoded.as_bytes())
        .with_context(|| format!("cannot write: {}", path.display()))
}

// ---------------------------------------------------------------------------
// Source binding extraction
// ---------------------------------------------------------------------------

struct SourceBinding {
    source_anchor: String,
    source_sha256: String,
    guide_anchor: String,
    guide_sha256: String,
    tex_anchor: String,
    tex_sha256: String,
}

fn source_binding(
    volume: &str,
    chapter: &str,
    book_root: &Path,
    manifest: &serde_json::Map<String, Value>,
) -> Result<SourceBinding> {
    let (source_anchor, guide_anchor, tex_anchor) = match volume {
        "vol1" => {
            let source_anchor =
                format!("references/ontology-engineering-book/handbook/chapters/{chapter}.tex");
            let guide_dir = VOL1_DIRS
                .iter()
                .find(|(k, _)| *k == chapter)
                .map(|(_, v)| *v)
                .with_context(|| format!("unknown vol1 chapter: {chapter}"))?;
            let guide_anchor =
                format!("references/ontology-engineering-book/{guide_dir}/README.md");
            let tex_anchor = source_anchor.clone();
            (source_anchor, guide_anchor, tex_anchor)
        }
        "vol2" => {
            let current = manifest
                .get("book_source")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let source_anchor = current
                .get("logical_anchor")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let source_parent = Path::new(&source_anchor)
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let guide_anchor = format!("{source_parent}/README.md");
            let tex_anchor = format!(
                "ontology-engineering/references/product-trustworthiness-book/handbook/fragments/readme-{chapter}.tex"
            );
            (source_anchor, guide_anchor, tex_anchor)
        }
        _ => bail!("unsupported volume: {volume}"),
    };

    let source_path = relative_book_path(book_root, &source_anchor)?;
    let guide_path = relative_book_path(book_root, &guide_anchor)?;
    let tex_path = relative_book_path(book_root, &tex_anchor)?;

    ensure!(
        source_path.is_file(),
        "missing authoritative book source: {}",
        source_path.display()
    );
    ensure!(
        tex_path.is_file(),
        "missing maintained TeX source: {}",
        tex_path.display()
    );
    ensure!(
        guide_path.is_file(),
        "missing maintained chapter guide: {}",
        guide_path.display()
    );

    Ok(SourceBinding {
        source_sha256: sha_file(&source_path)?,
        guide_sha256: sha_file(&guide_path)?,
        tex_sha256: sha_file(&tex_path)?,
        source_anchor,
        guide_anchor,
        tex_anchor,
    })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Rebind both maintained book sources to Semantica's 29 chapter packages.
/// `book_root` is the OE repository root; `semantica_root` is the Semantica source checkout.
/// When `write` is false, computes changes without modifying any files.
pub fn rebind(book_root: &Path, semantica_root: &Path, write: bool) -> Result<RebindResult> {
    let package_root = semantica_root.join("semantica").join("chapter_packages");
    let registry_path = package_root.join("registry.yaml");

    // Load registry — it may be YAML or JSON; we need the packages list
    let registry_text = std::fs::read_to_string(&registry_path)
        .with_context(|| format!("cannot read registry: {}", registry_path.display()))?;
    // Parse as JSON (the registry.yaml is expected to be JSON-compatible here)
    let registry: Value =
        serde_json::from_str(&registry_text).with_context(|| "registry must be valid JSON")?;

    let records = registry
        .get("packages")
        .and_then(Value::as_array)
        .with_context(|| "registry.packages array missing")?;

    ensure!(
        records.len() == EXPECTED_PACKAGES,
        "Semantica registry must contain exactly {EXPECTED_PACKAGES} chapter packages, got {}",
        records.len()
    );

    let mut entries = Vec::new();
    let mut changed_ids = Vec::new();
    let mut error_msgs = Vec::new();
    let mut desired_files: Vec<(PathBuf, serde_json::Map<String, Value>)> = Vec::new();

    for record in records {
        let volume = record.get("volume").and_then(Value::as_str).unwrap_or("");
        let chapter = record.get("chapter").and_then(Value::as_str).unwrap_or("");
        let id = format!("{volume}.{chapter}");
        let manifest_path =
            package_root.join(record.get("manifest").and_then(Value::as_str).unwrap_or(""));

        let mut manifest = match read_json(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                let msg = format!("{id}: {e:#}");
                error_msgs.push(msg.clone());
                entries.push(RebindEntry {
                    id,
                    changed: false,
                    error: Some(msg),
                });
                continue;
            }
        };

        let binding = match source_binding(volume, chapter, book_root, &manifest) {
            Ok(b) => b,
            Err(e) => {
                let msg = format!("{id}: {e:#}");
                error_msgs.push(msg.clone());
                entries.push(RebindEntry {
                    id,
                    changed: false,
                    error: Some(msg),
                });
                continue;
            }
        };

        // Find the contract asset
        let assets = manifest
            .get("assets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let contract_asset = assets
            .iter()
            .find(|a| a.get("role").and_then(Value::as_str) == Some("chapter_contract"));
        let contract_rel = match contract_asset
            .and_then(|a| a.get("path"))
            .and_then(Value::as_str)
        {
            Some(p) => p.to_owned(),
            None => {
                let msg = format!("{id}: chapter contract asset is missing");
                error_msgs.push(msg.clone());
                entries.push(RebindEntry {
                    id,
                    changed: false,
                    error: Some(msg),
                });
                continue;
            }
        };
        let contract_path = manifest_path.parent().unwrap().join(&contract_rel);
        let mut contract = match read_json(&contract_path) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("{id}: {e:#}");
                error_msgs.push(msg.clone());
                entries.push(RebindEntry {
                    id,
                    changed: false,
                    error: Some(msg),
                });
                continue;
            }
        };

        // Apply binding to contract
        let external = contract
            .entry("external_specification")
            .or_insert_with(|| Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .cloned()
            .unwrap_or_default();
        let mut new_external = external.clone();
        new_external.insert(
            "source_anchor".into(),
            Value::String(binding.source_anchor.clone()),
        );
        new_external.insert(
            "source_sha256".into(),
            Value::String(binding.source_sha256.clone()),
        );
        new_external.insert(
            "guide_anchor".into(),
            Value::String(binding.guide_anchor.clone()),
        );
        new_external.insert(
            "guide_sha256".into(),
            Value::String(binding.guide_sha256.clone()),
        );
        new_external.insert(
            "tex_anchor".into(),
            Value::String(binding.tex_anchor.clone()),
        );
        new_external.insert(
            "tex_sha256".into(),
            Value::String(binding.tex_sha256.clone()),
        );
        contract.insert("external_specification".into(), Value::Object(new_external));

        // For vol2 update manifest book_source binding too
        if volume == "vol2" {
            let book_source = manifest
                .entry("book_source")
                .or_insert_with(|| Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .cloned()
                .unwrap_or_default();
            let mut new_bs = book_source.clone();
            new_bs.insert(
                "logical_anchor".into(),
                Value::String(binding.source_anchor.clone()),
            );
            new_bs.insert(
                "sha256".into(),
                Value::String(binding.source_sha256.clone()),
            );
            new_bs.insert(
                "guide_anchor".into(),
                Value::String(binding.guide_anchor.clone()),
            );
            new_bs.insert(
                "guide_sha256".into(),
                Value::String(binding.guide_sha256.clone()),
            );
            new_bs.insert(
                "tex_anchor".into(),
                Value::String(binding.tex_anchor.clone()),
            );
            new_bs.insert(
                "tex_sha256".into(),
                Value::String(binding.tex_sha256.clone()),
            );
            manifest.insert("book_source".into(), Value::Object(new_bs));
        }

        let original_contract = std::fs::read(&contract_path).unwrap_or_default();
        let new_contract_bytes =
            (serde_json::to_string_pretty(&Value::Object(contract.clone()))? + "\n").into_bytes();
        let changed = sha256(&original_contract) != sha256(&new_contract_bytes);

        if changed {
            changed_ids.push(id.clone());
            desired_files.push((contract_path.clone(), contract));
            if volume == "vol2" {
                desired_files.push((manifest_path.clone(), manifest));
            }
        }

        entries.push(RebindEntry {
            id,
            changed,
            error: None,
        });
    }

    let mut written = false;
    if write && !changed_ids.is_empty() && error_msgs.is_empty() {
        for (path, obj) in &desired_files {
            write_json(path, obj)?;
        }
        written = true;
    }

    Ok(RebindResult {
        entries,
        changed: changed_ids,
        errors: error_msgs,
        written,
    })
}
