//! Exact package inputs, actual-byte identity and retention on the one session.
//! A missing expected SHA is unknown authenticity, not a verified source.
use crate::books::SourceProvider;
use anyhow::{Context, Result, bail, ensure};
use oxigraph::io::RdfFormat;
use semantica_contracts::{
    SemanticPackageAsset, SemanticPackageIdentity, SemanticPackageInput, SemanticPackageLoadResult,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const INPUT_SCHEMA: &str = "semantica.package-input/1";
#[derive(Clone, Copy)]
pub enum AssetPolicy {
    ActualBytes,
    RequireExpectedSha,
}
pub(crate) struct PreparedPackage {
    pub identity: SemanticPackageIdentity,
    pub assets: Vec<SemanticPackageAsset>,
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    let s = v[key]
        .as_str()
        .context(format!("package_text_missing:{key}"))?
        .trim();
    ensure!(!s.is_empty(), "package_text_empty:{key}");
    Ok(s)
}
fn scalar(value: &Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Bool(b) => Ok(if *b { "True" } else { "False" }.into()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Null => Ok("None".into()),
        _ => bail!("package_non_scalar_coercion_unsupported"),
    }
}
fn optional_scalar(v: &Value) -> Result<Option<String>> {
    if v.is_null() {
        Ok(None)
    } else {
        Ok(Some(scalar(v)?))
    }
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::Number(n) => n
            .to_string()
            .parse::<f64>()
            .map(|n| n != 0.0)
            .unwrap_or(true),
    }
}
pub fn normalize_rdf_format(format: &str) -> Result<String> {
    Ok(match format.trim().to_lowercase().as_str() {
        "ttl" | "turtle" => "turtle",
        "nt" | "ntriples" | "n-triples" => "nt",
        "rdfxml" | "rdf/xml" | "xml" => "xml",
        "jsonld" | "json_ld" | "json-ld" => "json-ld",
        "trig" => "trig",
        "nq" | "n-quads" | "nquads" => "nquads",
        "trix" => "trix",
        _ => bail!("package_rdf_format_unsupported"),
    }
    .into())
}
fn inferred(path: &str) -> Option<&'static str> {
    match Path::new(path)
        .extension()?
        .to_str()?
        .to_lowercase()
        .as_str()
    {
        "ttl" => Some("turtle"),
        "nt" => Some("nt"),
        "rdf" | "owl" | "xml" => Some("xml"),
        "jsonld" => Some("json-ld"),
        "trig" => Some("trig"),
        "nq" => Some("nquads"),
        "trix" => Some("trix"),
        _ => None,
    }
}
pub(crate) fn is_rdf(asset: &Value, role: &str) -> bool {
    asset["kind"]
        .as_str()
        .is_some_and(|s| s.trim().eq_ignore_ascii_case("rdf"))
        || matches!(
            role.to_lowercase().as_str(),
            "ontology" | "data" | "instances" | "rdf" | "shapes"
        )
}
pub(crate) fn loaded(asset: &Value, role: &str) -> bool {
    asset
        .get("load_into_dataset")
        .map(truthy)
        .unwrap_or_else(|| {
            matches!(
                role.to_lowercase().as_str(),
                "ontology" | "data" | "instances" | "rdf"
            )
        })
}
pub(crate) fn asset_format(asset: &Value) -> Result<Option<String>> {
    let role = required(asset, "role")?;
    let provided = asset
        .get("format")
        .filter(|v| truthy(v))
        .map(scalar)
        .transpose()?;
    let format = provided.or_else(|| asset["path"].as_str().and_then(inferred).map(str::to_owned));
    format
        .map(|f| {
            if is_rdf(asset, role) {
                normalize_rdf_format(&f)
            } else {
                Ok(f)
            }
        })
        .transpose()
}
pub(crate) fn rdf_format(format: Option<&str>) -> Result<RdfFormat> {
    Ok(match format.unwrap_or("turtle") {
        "turtle" => RdfFormat::Turtle,
        "nt" => RdfFormat::NTriples,
        "xml" => RdfFormat::RdfXml,
        "trig" => RdfFormat::TriG,
        "nquads" => RdfFormat::NQuads,
        "json-ld" | "trix" => bail!("package_rdf_format_execution_not_migrated"),
        _ => bail!("package_rdf_format_unsupported"),
    })
}
fn local_path(path: &str) -> Result<String> {
    ensure!(
        !path.is_empty() && !path.contains('\\') && !path.contains('\0'),
        "package_path_invalid"
    );
    ensure!(
        !Path::new(path).is_absolute(),
        "package_asset_path_absolute"
    );
    let mut parts = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                ensure!(parts.pop().is_some(), "package_asset_path_escapes_root");
            }
            p => parts.push(p),
        }
    }
    ensure!(!parts.is_empty(), "package_path_empty");
    Ok(parts.join("/"))
}
pub(crate) fn source_id(root: &str, path: &str) -> Result<String> {
    let child = local_path(path)?;
    if root.is_empty() {
        Ok(child)
    } else {
        Ok(format!("{}/{}", local_path(root)?, child))
    }
}
pub(crate) fn read_asset(
    provider: &dyn SourceProvider,
    root: Option<&str>,
    asset: &Value,
    inline: &BTreeMap<String, Vec<u8>>,
    policy: AssetPolicy,
) -> Result<SemanticPackageAsset> {
    ensure!(asset.is_object(), "package_asset_not_mapping");
    let id = required(asset, "asset_id")?;
    let role = required(asset, "role")?;
    let has_path = asset.get("path").is_some();
    let has_data = asset.get("data").is_some();
    ensure!(
        has_path != has_data,
        "package_asset_requires_exactly_one_path_or_data"
    );
    let mut format_asset = asset.clone();
    let (content, origin) = if has_data {
        // Native bytes use a separate typed transport map, never JSON arrays.
        let text = asset["data"]
            .as_str()
            .context("package_inline_data_not_text_or_native_bytes")?;
        if let Some(bytes) = inline.get(id) {
            ensure!(
                text.is_empty(),
                "package_native_bytes_requires_empty_text_marker"
            );
            (bytes.clone(), "inline_bytes")
        } else {
            (text.as_bytes().to_vec(), "inline_text")
        }
    } else {
        ensure!(
            !inline.contains_key(id),
            "package_native_bytes_on_path_asset"
        );
        let root = root.context("package_asset_path_requires_root")?;
        let p = scalar(&asset["path"])?;
        let (bytes, resolved_id) = provider.read_scoped_asset(root, &p)?;
        format_asset["path"] = json!(resolved_id);
        (bytes, "path")
    };
    ensure!(
        content.len() <= crate::MAX_BYTES,
        "package_asset_byte_budget"
    );
    let hash = crate::sha256(&content);
    let expected = optional_scalar(&asset["sha256"])?;
    if matches!(policy, AssetPolicy::RequireExpectedSha) {
        ensure!(expected.is_some(), "package_expected_asset_sha_missing");
    }
    if let Some(expected) = &expected {
        ensure!(
            expected.to_lowercase() == hash,
            "package_asset_sha256_mismatch"
        );
    }
    let load = loaded(asset, role);
    ensure!(!load || is_rdf(asset, role), "package_loaded_asset_not_rdf");
    Ok(SemanticPackageAsset {
        asset_id: id.into(),
        role: role.into(),
        content,
        sha256: hash,
        format: asset_format(&format_asset)?,
        graph_name: optional_scalar(&asset["graph_name"])?,
        loaded_into_dataset: load,
        expected_sha256: expected,
        source_authenticity: "unknown".into(),
        data_origin: origin.into(),
    })
}
pub(crate) fn prepare(
    provider: &dyn SourceProvider,
    manifest: &Value,
    root: Option<&str>,
    inline: &BTreeMap<String, Vec<u8>>,
    policy: AssetPolicy,
) -> Result<PreparedPackage> {
    ensure!(manifest.is_object(), "package_manifest_not_mapping");
    ensure!(
        scalar(&manifest["schema_version"])? == "1.0",
        "package_schema_unsupported"
    );
    let id = required(manifest, "package_id")?;
    let version = required(manifest, "version")?;
    let records = manifest["assets"]
        .as_array()
        .filter(|a| !a.is_empty())
        .context("package_assets_missing")?;
    ensure!(records.len() <= 64, "package_asset_count_budget");
    let mut ids = BTreeSet::new();
    let mut assets = Vec::new();
    let mut total = 0usize;
    for raw in records {
        let asset = read_asset(provider, root, raw, inline, policy)?;
        ensure!(
            ids.insert(asset.asset_id.clone()),
            "package_duplicate_asset_id"
        );
        total = total
            .checked_add(asset.content.len())
            .context("package_byte_budget")?;
        ensure!(total <= 64 * 1024 * 1024, "package_byte_budget");
        assets.push(asset);
    }
    ensure!(
        inline.keys().all(|id| ids.contains(id)),
        "package_unused_native_bytes"
    );
    assets.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
    let namespace = optional_scalar(&manifest["namespace"])?;
    // Source/expected hashes are metadata only; actual full content hashes own identity.
    let normalized = json!({"schema_version":"1.0","package_id":id,"version":version,"namespace":namespace,
        "assets":assets.iter().map(|a| json!({"asset_id":a.asset_id,"role":a.role,"sha256":a.sha256,
            "format":a.format,"graph_name":a.graph_name,"load_into_dataset":a.loaded_into_dataset})).collect::<Vec<_>>()});
    let digest = package_digest(&normalized)?;
    if let Some(expected) = optional_scalar(&manifest["digest"])? {
        ensure!(expected.to_lowercase() == digest, "package_digest_mismatch");
    }
    Ok(PreparedPackage {
        identity: SemanticPackageIdentity {
            schema_version: "1.0".into(),
            package_id: id.into(),
            version: version.into(),
            namespace,
            digest,
        },
        assets,
    })
}
pub fn package_digest(manifest: &Value) -> Result<String> {
    Ok(crate::sha256(package_identity_bytes(manifest)?))
}
pub fn package_identity_bytes(manifest: &Value) -> Result<Vec<u8>> {
    let mut assets = Vec::new();
    for a in manifest["assets"]
        .as_array()
        .context("package_assets_missing")?
    {
        let role = required(a, "role")?;
        assets.push(json!({"asset_id":required(a,"asset_id")?,"role":role,"sha256":required(a,"sha256")?,
            "format":asset_format(a)?,"graph_name":a["graph_name"],"loaded_into_dataset":loaded(a,role)}));
    }
    assets.sort_by(|a, b| a["asset_id"].as_str().cmp(&b["asset_id"].as_str()));
    let value = json!({"schema_version":manifest["schema_version"],"package_id":manifest["package_id"],
        "version":manifest["version"],"namespace":manifest["namespace"],"assets":assets});
    let canonical = String::from_utf8(crate::books::canonical(&value)?)?;
    let mut ascii = String::new();
    for c in canonical.chars() {
        if c as u32 <= 0x7e {
            ascii.push(c);
        } else {
            let mut units = [0u16; 2];
            for n in c.encode_utf16(&mut units) {
                ascii.push_str(&format!("\\u{n:04x}"));
            }
        }
    }
    Ok(ascii.into_bytes())
}
pub fn load_package(
    session: &mut crate::dataset::DatasetSession,
    provider: &dyn SourceProvider,
    input: &SemanticPackageInput,
) -> Result<SemanticPackageLoadResult> {
    ensure!(input.schema == INPUT_SCHEMA, "package_input_schema");
    ensure!(
        input.manifest.is_some() != input.manifest_path.is_some(),
        "package_one_manifest_input"
    );
    let (manifest, root) = if let Some(manifest) = &input.manifest {
        (manifest.clone(), input.base_path.clone())
    } else {
        let path = local_path(&provider.resolve_id(input.manifest_path.as_ref().unwrap())?)?;
        ensure!(
            matches!(
                Path::new(&path)
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(str::to_lowercase)
                    .as_deref(),
                Some("json" | "yaml" | "yml")
            ),
            "package_manifest_requires_json_or_yaml"
        );
        let bytes = provider.read(&path)?;
        ensure!(
            bytes.len() <= crate::MAX_BYTES,
            "package_manifest_byte_budget"
        );
        let manifest = provider.metadata_at(&bytes, &path)?;
        // File input uses its actual parent; explicit base_path is ignored.
        (
            manifest,
            Some(path.rsplit_once('/').map(|(p, _)| p).unwrap_or("").into()),
        )
    };
    let prepared = prepare(
        provider,
        &manifest,
        root.as_deref(),
        &input.inline_bytes,
        AssetPolicy::ActualBytes,
    )?;
    session.register_prepared_package(prepared)
}
/// Neutral local provider. Workspace roots are injected; root-containment is
/// checked after real filesystem resolution, including symlinks.
pub struct FileProvider {
    root: PathBuf,
}
impl FileProvider {
    pub fn new(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        ensure!(root.is_dir(), "package_provider_root_not_directory");
        Ok(Self { root })
    }
}
impl SourceProvider for FileProvider {
    fn metadata_at(&self, bytes: &[u8], id: &str) -> Result<Value> {
        let text = std::str::from_utf8(bytes)?;
        match Path::new(id)
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("json") => Ok(serde_json::from_str(text)?),
            Some("yaml" | "yml") => Ok(serde_yaml::from_str(text)?),
            _ => bail!("package_manifest_requires_json_or_yaml"),
        }
    }
    fn resolve_id(&self, id: &str) -> Result<String> {
        let path = self.root.join(local_path(id)?).canonicalize()?;
        ensure!(
            path.starts_with(&self.root),
            "package_resolved_path_escapes_root"
        );
        Ok(path
            .strip_prefix(&self.root)?
            .to_str()
            .context("package_path_utf8")?
            .into())
    }
    fn read_scoped_asset(&self, root: &str, path: &str) -> Result<(Vec<u8>, String)> {
        let package_root = if root.is_empty() {
            self.root.clone()
        } else {
            self.root.join(local_path(root)?).canonicalize()?
        };
        ensure!(
            package_root.starts_with(&self.root),
            "package_base_path_escapes_provider"
        );
        let resolved = package_root.join(local_path(path)?).canonicalize()?;
        ensure!(
            resolved.starts_with(&package_root),
            "package_resolved_asset_escapes_package_root"
        );
        let id = resolved
            .strip_prefix(&self.root)?
            .to_str()
            .context("package_path_utf8")?
            .to_owned();
        Ok((self.read(&id)?, id))
    }
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        let path = self.root.join(local_path(id)?).canonicalize()?;
        ensure!(
            path.starts_with(&self.root),
            "package_resolved_path_escapes_root"
        );
        let meta = std::fs::metadata(&path)?;
        ensure!(
            meta.is_file() && meta.len() <= crate::MAX_BYTES as u64,
            "package_source_file_budget"
        );
        let bytes = std::fs::read(path)?;
        ensure!(
            bytes.len() <= crate::MAX_BYTES,
            "package_source_file_budget"
        );
        Ok(bytes)
    }
    fn metadata(&self, bytes: &[u8]) -> Result<Value> {
        let text = std::str::from_utf8(bytes)?;
        if let Ok(value) = serde_json::from_str(text) {
            return Ok(value);
        }
        Ok(serde_yaml::from_str(text)?)
    }
}
pub fn load_package_file(
    session: &mut crate::dataset::DatasetSession,
    path: &Path,
) -> Result<SemanticPackageLoadResult> {
    let path = path.canonicalize()?;
    ensure!(path.is_file(), "package_manifest_missing");
    let provider = FileProvider::new(path.parent().context("package_manifest_parent_missing")?)?;
    let input = SemanticPackageInput {
        schema: INPUT_SCHEMA.into(),
        manifest: None,
        manifest_path: Some(
            path.file_name()
                .and_then(|s| s.to_str())
                .context("package_path_utf8")?
                .into(),
        ),
        base_path: None,
        inline_bytes: BTreeMap::new(),
    };
    load_package(session, &provider, &input)
}

/// Read a manifest-relative or embedded asset through the existing package
/// byte policy. This is content transport, never lifecycle execution authority.
pub fn read_manifest_asset(
    provider: &dyn SourceProvider,
    asset: &Value,
) -> Result<SemanticPackageAsset> {
    read_asset(
        provider,
        Some(""),
        asset,
        &BTreeMap::new(),
        AssetPolicy::RequireExpectedSha,
    )
}
