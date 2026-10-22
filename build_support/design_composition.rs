//! Bounded /2 source transport shared by build capture and the Rust normalizer.
//! It never renders code or creates a parallel semantic entity store.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

pub const ROOT_SCHEMA: &str = "oev2.design/2";
pub const REFERENCE_ROOT_SCHEMA: &str = "oev2.design/3";
pub const GROUPED_ROOT_SCHEMA: &str = "oev2.design/4";
pub const VALIDATION_ENCODING: &str = "oev2.validation-groups/1";
pub fn is_composed(schema: &str) -> bool {
    matches!(
        schema,
        ROOT_SCHEMA | REFERENCE_ROOT_SCHEMA | GROUPED_ROOT_SCHEMA
    )
}
pub const FRAGMENT_SCHEMA: &str = "oev2.design-fragment/2";
pub const GROUPED_FRAGMENT_SCHEMA: &str = "oev2.design-fragment/3";
pub const FILE_BYTES: usize = 4 * 1024 * 1024;
pub const TOTAL_BYTES: usize = 8 * 1024 * 1024;
pub const FILE_COUNT: usize = 32;
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fragment {
    pub path: String,
    pub sha256: String,
    pub format: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub version: String,
    pub problem: String,
    pub purpose: String,
    pub fragments: Vec<Fragment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_encoding: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub manifest: Manifest,
    pub root: String,
    pub files: BTreeMap<String, Vec<u8>>,
    pub hashes: BTreeMap<String, String>,
    pub total_bytes: usize,
}
fn err(s: impl ToString) -> String {
    s.to_string()
}
fn check(ok: bool, token: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(token.into()) }
}
pub fn relative(s: &str) -> Result<(), String> {
    check(
        !s.is_empty()
            && !s.starts_with('/')
            && !s.contains(['\\', ':', '\0'])
            && s.split('/').all(|x| !x.is_empty() && x != "." && x != ".."),
        "design_composition_unsafe_path",
    )
}
/// Normalize lexical components without following a pathname into another
/// source. Anchored reads validate the actual root and every original name.
pub fn absolute(path: &Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    if path
        .components()
        .next()
        .is_some_and(|c| matches!(c, Component::Prefix(_)))
        && !path.has_root()
    {
        return Err("design_composition_unsafe_path".into());
    }
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().map_err(err)?.join(path)
    };
    #[cfg(windows)]
    {
        use std::path::Prefix;
        check(path.is_absolute(), "design_composition_unsafe_path")?;
        check(
            matches!(path.components().next(),
            Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))),
            "design_composition_local_disk_root_required",
        )?;
        for part in path.components() {
            if let Component::Normal(name) = part {
                oe_windows_controller::windows::validate_source_component(name).map_err(err)?;
            }
        }
    }
    let mut value = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => return Err("design_composition_unsafe_path".into()),
            _ => value.push(part.as_os_str()),
        }
    }
    Ok(value)
}
/// Return only the unchanged lexical tail below an admitted root. Windows
/// Disk and VerbatimDisk of the same drive have equivalent prefixes; suffix
/// names remain exact. No canonicalize, case folding or file access is used.
pub fn relative_under(base: &Path, path: &Path) -> Result<PathBuf, String> {
    let base = absolute(base)?;
    let path = absolute(path)?;
    let mut incoming = path.components();
    for expected in base.components() {
        let actual = incoming
            .next()
            .ok_or("design_composition_root_outside_base")?;
        let same = expected == actual;
        #[cfg(windows)]
        let same = same
            || match (expected, actual) {
                (Component::Prefix(a), Component::Prefix(b)) => {
                    use std::path::Prefix;
                    matches!((a.kind(), b.kind()),
                    (Prefix::Disk(a), Prefix::VerbatimDisk(b))
                    | (Prefix::VerbatimDisk(a), Prefix::Disk(b)) if a == b)
                }
                _ => false,
            };
        check(same, "design_composition_root_outside_base")?;
    }
    let mut tail = PathBuf::new();
    for part in incoming {
        match part {
            Component::Normal(name) => tail.push(name),
            _ => return Err("design_composition_unsafe_path".into()),
        }
    }
    Ok(tail)
}
/// Anchor a supported lexical spelling to the original admitted base. Keep
/// the original tail and all link/reparse checks; actual reads remain pinned.
pub fn anchored(base: &Path, path: &Path) -> Result<PathBuf, String> {
    let base = absolute(base)?;
    let incoming = absolute(path)?;
    let tail = relative_under(&base, &incoming)?;
    no_links(&base)?;
    no_links(&incoming)?;
    let path = base.join(tail);
    no_links(&path)?;
    Ok(path)
}
fn no_links(path: &Path) -> Result<(), String> {
    for p in path.ancestors() {
        if p.as_os_str().is_empty() {
            continue;
        }
        let m = fs::symlink_metadata(p).map_err(err)?;
        check(
            !m.file_type().is_symlink(),
            "design_composition_link_unsupported",
        )?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            check(
                m.file_attributes() & 0x400 == 0,
                "design_composition_reparse_unsupported",
            )?;
        }
    }
    Ok(())
}
#[path = "source_reader.rs"]
mod source_reader;
pub fn read(path: &Path) -> Result<Vec<u8>, String> {
    source_reader::read(path, FILE_BYTES)
}
pub fn schema(bytes: &[u8]) -> Result<String, String> {
    let t: toml::Value = toml::from_str(std::str::from_utf8(bytes).map_err(err)?).map_err(err)?;
    t.get("schema")
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "design_schema_missing".into())
}
pub fn snapshot(base: &Path, path: &Path) -> Result<Snapshot, String> {
    let base = absolute(base)?;
    let path = anchored(&base, path)?;
    let root = path
        .strip_prefix(&base)
        .map_err(|_| "design_composition_root_outside_base")?
        .components()
        .map(|p| p.as_os_str().to_str().ok_or("design_composition_path_utf8"))
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    relative(&root)?;
    let bytes = read(&path)?;
    let manifest: Manifest =
        toml::from_str(std::str::from_utf8(&bytes).map_err(err)?).map_err(err)?;
    check(
        is_composed(&manifest.schema),
        "design_composition_schema_unsupported",
    )?;
    check(
        if manifest.schema == GROUPED_ROOT_SCHEMA {
            manifest.validation_encoding.as_deref() == Some(VALIDATION_ENCODING)
        } else {
            manifest.validation_encoding.is_none()
        },
        "design_composition_validation_encoding_unsupported",
    )?;
    check(
        !manifest.fragments.is_empty() && manifest.fragments.len() < FILE_COUNT,
        "design_composition_file_count_budget",
    )?;
    let parent = path.parent().ok_or("design_composition_parent_missing")?;
    let mut total = bytes.len();
    let mut files = BTreeMap::from([(root.clone(), bytes)]);
    let mut logical = BTreeSet::new();
    let mut identities = BTreeSet::from([path.clone()]);
    for fragment in &manifest.fragments {
        relative(&fragment.path)?;
        check(
            logical.insert(fragment.path.clone()),
            "design_composition_duplicate_path",
        )?;
        check(
            fragment.sha256.len() == 64
                && fragment
                    .sha256
                    .bytes()
                    .all(|x| x.is_ascii_hexdigit() && !x.is_ascii_uppercase()),
            "design_composition_sha_invalid",
        )?;
        check(
            ["oev2.design/1", FRAGMENT_SCHEMA].contains(&fragment.format.as_str())
                || (manifest.schema == GROUPED_ROOT_SCHEMA
                    && fragment.format == GROUPED_FRAGMENT_SCHEMA),
            "design_composition_nested_manifest_unsupported",
        )?;
        let p = parent.join(&fragment.path);
        no_links(&p)?;
        let p = absolute(&p)?;
        check(
            p.starts_with(parent) && identities.insert(p.clone()),
            "design_composition_alias_or_outside_path",
        )?;
        let bytes = read(&p)?;
        total = total
            .checked_add(bytes.len())
            .ok_or("design_composition_total_byte_budget")?;
        check(total <= TOTAL_BYTES, "design_composition_total_byte_budget")?;
        check(
            format!("{:x}", Sha256::digest(&bytes)) == fragment.sha256,
            "design_composition_source_sha_mismatch",
        )?;
        check(
            schema(&bytes)? == fragment.format,
            "design_composition_fragment_schema_mismatch",
        )?;
        let name = p
            .strip_prefix(&base)
            .map_err(|_| "design_composition_fragment_outside_base")?
            .components()
            .map(|p| p.as_os_str().to_str().ok_or("design_composition_path_utf8"))
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        check(
            files.insert(name, bytes).is_none(),
            "design_composition_duplicate_path",
        )?;
    }
    let hashes = files
        .iter()
        .map(|(p, b)| (p.clone(), format!("{:x}", Sha256::digest(b))))
        .collect();
    for (name, bytes) in &files {
        check(
            read(&base.join(name))? == *bytes,
            "design_composition_changed_during_load",
        )?;
    }
    Ok(Snapshot {
        manifest,
        root,
        files,
        hashes,
        total_bytes: total,
    })
}
pub fn current_capture(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let p = root.join("specs/engineering.toml");
    // Legacy missing/malformed/oversized /1 roots remain uncaptured. The same
    // anchored, nonblocking reader rejects links and nonregular probe inputs.
    let Some(bytes) = source_reader::probe(&p, FILE_BYTES)? else {
        return Ok(BTreeMap::new());
    };
    if !schema(&bytes).ok().as_deref().is_some_and(is_composed) {
        return Ok(BTreeMap::new());
    }
    Ok(snapshot(root, &p)?.files)
}
