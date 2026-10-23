// One full public inventory algorithm reused at build time and by the driver.
// Private source archives, state/builds, and VCS internals stay outside capture;
// only the new, explicit profile binds the public sources/README.md exception.
#[path = "source_limits.rs"]
mod source_limits;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::Path,
};
pub fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
/// Historical domains keep their exact traversal and serialized identity.
/// New captures use SourceReadmeDerived32, never reinterpret CurrentDerived32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicManifestProfile {
    SourceReadmeDerived32,
    CurrentDerived32,
    Legacy16,
}
impl PublicManifestProfile {
    pub fn file_bytes(self, relative: &Path) -> u64 {
        match self {
            Self::SourceReadmeDerived32 | Self::CurrentDerived32 => {
                source_limits::public_file_bytes(relative)
            }
            Self::Legacy16 => 16 * 1024 * 1024,
        }
    }
}
pub fn manifest(root: &Path) -> io::Result<Value> {
    manifest_with_profile(root, PublicManifestProfile::SourceReadmeDerived32)
}
pub fn manifest_with_profile(root: &Path, profile: PublicManifestProfile) -> io::Result<Value> {
    fn record(
        root: &Path,
        path: &Path,
        out: &mut BTreeMap<String, Value>,
        total: &mut u64,
        profile: PublicManifestProfile,
    ) -> io::Result<()> {
        let rel = path.strip_prefix(root).map_err(io::Error::other)?;
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::other("code_public_symlink"));
        }
        if !metadata.is_file() || metadata.len() > profile.file_bytes(rel) || out.len() >= 10_000 {
            return Err(io::Error::other("code_public_file_budget"));
        }
        *total = total
            .checked_add(metadata.len())
            .ok_or_else(|| io::Error::other("code_public_size_overflow"))?;
        if *total > 256 * 1024 * 1024 {
            return Err(io::Error::other("code_public_total_budget"));
        }
        let limit = profile.file_bytes(rel);
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(limit + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > limit {
            return Err(io::Error::other("code_public_changed"));
        }
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o7777
        };
        #[cfg(not(unix))]
        let mode = if metadata.permissions().readonly() {
            0o444u32
        } else {
            0o644u32
        };
        let name = rel
            .to_str()
            .ok_or_else(|| io::Error::other("code_public_non_utf8"))?
            .replace(std::path::MAIN_SEPARATOR, "/");
        out.insert(
            name,
            json!({"sha256":hash(bytes),"bytes":metadata.len(),"mode":mode}),
        );
        Ok(())
    }
    fn visit(
        root: &Path,
        path: &Path,
        out: &mut BTreeMap<String, Value>,
        total: &mut u64,
        profile: PublicManifestProfile,
    ) -> io::Result<()> {
        for entry in std::fs::read_dir(path)? {
            let p = entry?.path();
            let rel = p.strip_prefix(root).map_err(io::Error::other)?;
            if profile == PublicManifestProfile::SourceReadmeDerived32
                && rel.components().count() == 1
                && rel
                    .to_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("sources"))
            {
                if rel != Path::new("sources") {
                    return Err(io::Error::other("code_public_sources_case_mismatch"));
                }
                let metadata = std::fs::symlink_metadata(&p)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(io::Error::other("code_public_sources_not_directory"));
                }
                // Inspect only entry names to enforce exact casing even on a
                // case-insensitive filesystem. Never stat/read private children.
                for entry in std::fs::read_dir(&p)? {
                    let entry = entry?;
                    if entry.file_name() == std::ffi::OsStr::new("README.md") {
                        record(root, &entry.path(), out, total, profile)?;
                    }
                }
                continue;
            }
            if rel.components().count() == 1
                && ["var", "sources", ".git"].contains(&rel.to_str().unwrap_or(""))
            {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&p)?;
            if metadata.file_type().is_symlink() {
                return Err(io::Error::other("code_public_symlink"));
            }
            if metadata.is_dir() {
                visit(root, &p, out, total, profile)?;
            } else {
                record(root, &p, out, total, profile)?;
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    let mut total = 0;
    visit(root, root, &mut files, &mut total, profile)?;
    // Construct the exact original seven-key domain; never remove unknown fields
    // from an incoming claim. The profile already governed every actual read.
    let mut result = json!({"schema":"oev2.code-public-manifest/1","files":files,"file_count":files.len(),
        "total_bytes":total,"excluded_private_roots":["var","sources",".git"],
        "uncovered":["external Cargo source bytes and resolved transitive feature graph","native SDK/runtime/provider closure","unlisted compile environment"],"status":"bounded_public_identity"});
    if profile != PublicManifestProfile::Legacy16 {
        result["resource_profile"] = json!(source_limits::PROFILE);
        result["file_byte_limits"] = json!({"default":source_limits::PUBLIC_FILE_BYTES,"generated/model.ttl":source_limits::DERIVED_MODEL_BYTES});
    }
    if profile == PublicManifestProfile::SourceReadmeDerived32 {
        result["public_sources"] = json!(["sources/README.md"]);
    }
    Ok(result)
}
pub fn capture_for_cargo() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&dir).parent().unwrap().parent().unwrap();
    let first = manifest(root).unwrap();
    assert_eq!(
        first,
        manifest(root).unwrap(),
        "code_source_changed_during_capture"
    );
    for key in first["files"].as_object().unwrap().keys() {
        println!("cargo:rerun-if-changed={}", root.join(key).display());
    }
    // Track addition/removal of this one public file without watching private data.
    println!(
        "cargo:rerun-if-changed={}",
        root.join("sources/README.md").display()
    );
    for key in [
        "crates",
        "generated",
        "runtime",
        "specs",
        "templates",
        "build_support",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(key).display());
    }
    for entry in std::fs::read_dir(root).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir()
            && !["var", "sources", ".git"].contains(&p.file_name().unwrap().to_str().unwrap())
        {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }
    let compiler = std::process::Command::new(std::env::var("RUSTC").unwrap())
        .arg("-Vv")
        .output()
        .unwrap();
    assert!(compiler.status.success());
    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(k, _)| k.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    let value = json!({"manifest":first,"manifest_sha256":hash(oe_wire::json::canonical_json(&first).unwrap()),
        "compiler":String::from_utf8(compiler.stdout).unwrap(),"target":std::env::var("TARGET").unwrap(),
        "features":features,"profile":std::env::var("PROFILE").unwrap(),
        "language":"rust","package_name":std::env::var("CARGO_PKG_NAME").unwrap(),
        "package_version":std::env::var("CARGO_PKG_VERSION").unwrap(),
        "rustflags_sha256":hash(std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default())});
    std::fs::write(
        Path::new(&std::env::var("OUT_DIR").unwrap()).join("code-source.json"),
        oe_wire::json::canonical_json(&value).unwrap(),
    )
    .unwrap();
}
