use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer, Serialize, de};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Digest(String);
impl Digest {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        ensure!(
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid sha256"
        );
        Ok(Self(value))
    }
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct JobId(String);
impl JobId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        ensure!(
            !value.is_empty()
                && value.len() <= 80
                && value.as_bytes()[0].is_ascii_alphanumeric()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid job id"
        );
        // IDs become Windows directories. Reject reserved basenames as well.
        validate_component(&value)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for JobId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(de::Error::custom)
    }
}

/// Preserve original UTF-8 part names: renaming could break native assembly
/// references. Portable prechecks complement target-volume handle/file-ID checks.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct RelativePath(String);
impl RelativePath {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        ensure!(
            !value.is_empty() && value.encode_utf16().count() <= 240,
            "invalid relative path"
        );
        for component in value.split('/') {
            validate_component(component)?;
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(de::Error::custom)
    }
}
fn validate_component(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.encode_utf16().count() <= 200
            && value != "."
            && value != ".."
            && !value.ends_with(['.', ' '])
            && !value.starts_with(' ')
            && !value
                .chars()
                .any(|c| c.is_control() || "<>:\"\\|?*/".contains(c)),
        "unsafe Windows path component"
    );
    let stem = value.split('.').next().unwrap().trim_end().to_uppercase();
    ensure!(
        !["CON", "PRN", "AUX", "NUL", "CLOCK$"].contains(&stem.as_str())
            && !["COM", "LPT"]
                .iter()
                .any(
                    |prefix| stem.strip_prefix(prefix).is_some_and(|suffix| matches!(
                        suffix,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    ))
                ),
        "reserved Windows path"
    );
    Ok(())
}

pub fn checked_label(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control),
        "invalid identity label"
    );
    Ok(())
}
pub fn checked_token(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b)),
        "invalid capability token"
    );
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlobIdentity {
    pub sha256: Digest,
    pub bytes: u64,
}
impl BlobIdentity {
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            sha256: Digest::of(bytes),
            bytes: bytes.len() as u64,
        }
    }
    pub fn verify(&self, bytes: &[u8]) -> Result<()> {
        ensure!(*self == Self::of(bytes), "file byte identity differs");
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub path: RelativePath,
    pub identity: BlobIdentity,
}
impl FileEntry {
    pub fn from_bytes(path: &str, bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            path: RelativePath::new(path)?,
            identity: BlobIdentity::of(bytes),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Digest ---

    #[test]
    fn digest_new_accepts_64_lowercase_hex() {
        let s = "a".repeat(64);
        assert!(Digest::new(&s).is_ok());
    }

    #[test]
    fn digest_new_rejects_uppercase() {
        let s = "A".repeat(64);
        assert!(Digest::new(s).is_err());
    }

    #[test]
    fn digest_new_rejects_wrong_length() {
        assert!(Digest::new("a".repeat(63)).is_err());
        assert!(Digest::new("a".repeat(65)).is_err());
    }

    #[test]
    fn digest_of_is_deterministic() {
        let d1 = Digest::of(b"hello");
        let d2 = Digest::of(b"hello");
        assert_eq!(d1, d2);
        assert_eq!(d1.as_str().len(), 64);
    }

    #[test]
    fn digest_of_different_inputs_differ() {
        assert_ne!(Digest::of(b"hello"), Digest::of(b"world"));
    }

    // --- JobId ---

    #[test]
    fn job_id_accepts_simple_alphanumeric() {
        assert!(JobId::new("hpx001-job-01").is_ok());
        assert!(JobId::new("abc123").is_ok());
    }

    #[test]
    fn job_id_rejects_empty() {
        assert!(JobId::new("").is_err());
    }

    #[test]
    fn job_id_rejects_leading_hyphen() {
        assert!(JobId::new("-abc").is_err());
    }

    #[test]
    fn job_id_rejects_reserved_windows_name() {
        assert!(JobId::new("CON").is_err());
        assert!(JobId::new("NUL").is_err());
        assert!(JobId::new("COM1").is_err());
    }

    #[test]
    fn job_id_rejects_path_separators() {
        assert!(JobId::new("a/b").is_err());
        assert!(JobId::new("a\\b").is_err());
    }

    // --- RelativePath ---

    #[test]
    fn relative_path_accepts_valid_unix_path() {
        assert!(RelativePath::new("parts/housing/cover.stp").is_ok());
    }

    #[test]
    fn relative_path_rejects_dotdot() {
        assert!(RelativePath::new("../escape").is_err());
    }

    #[test]
    fn relative_path_rejects_control_chars() {
        assert!(RelativePath::new("file\x00name").is_err());
    }

    #[test]
    fn relative_path_rejects_windows_reserved() {
        assert!(RelativePath::new("COM1.stp").is_err());
        assert!(RelativePath::new("nul").is_err());
    }

    #[test]
    fn relative_path_rejects_trailing_dot_or_space() {
        assert!(RelativePath::new("file.").is_err());
        assert!(RelativePath::new("file ").is_err());
    }

    // --- BlobIdentity ---

    #[test]
    fn blob_identity_of_matches_verify() {
        let data = b"some geometry output bytes";
        let id = BlobIdentity::of(data);
        assert!(id.verify(data).is_ok());
    }

    #[test]
    fn blob_identity_verify_fails_on_modified_bytes() {
        let id = BlobIdentity::of(b"original");
        assert!(id.verify(b"modified").is_err());
    }

    // --- validate_inventory ---

    #[test]
    fn validate_inventory_empty_is_ok() {
        assert!(validate_inventory(&[]).is_ok());
    }

    #[test]
    fn validate_inventory_rejects_duplicate_paths() {
        let e1 = FileEntry::from_bytes("parts/a.stp", b"data1").unwrap();
        let e2 = FileEntry::from_bytes("parts/a.stp", b"data2").unwrap();
        assert!(validate_inventory(&[e1, e2]).is_err());
    }

    #[test]
    fn validate_inventory_accepts_distinct_paths() {
        let e1 = FileEntry::from_bytes("parts/a.stp", b"data1").unwrap();
        let e2 = FileEntry::from_bytes("parts/b.stp", b"data2").unwrap();
        assert!(validate_inventory(&[e1, e2]).is_ok());
    }
}

pub fn validate_inventory(files: &[FileEntry]) -> Result<()> {
    validate_inventory_with_limit(files, crate::MAX_PAYLOAD_BYTES)
}

pub(crate) fn validate_inventory_with_limit(files: &[FileEntry], limit: u64) -> Result<()> {
    ensure!(files.len() <= crate::MAX_FILES, "file count exceeds bound");
    let mut names = BTreeSet::new();
    let mut compatible_names = BTreeSet::new();
    let mut total = 0u64;
    for file in files {
        let key = path_collision_key(file.path.as_str());
        ensure!(names.insert(key), "duplicate or case-aliased file");
        ensure!(
            compatible_names.insert(compatibility_collision_key(file.path.as_str())),
            "Windows compatible path collision"
        );
        total = total
            .checked_add(file.identity.bytes)
            .ok_or_else(|| anyhow::anyhow!("payload byte overflow"))?;
        ensure!(total <= limit, "payload exceeds bound");
    }
    for name in names.iter().chain(compatible_names.iter()) {
        for (i, _) in name.match_indices('/') {
            ensure!(
                !names.contains(&name[..i]) && !compatible_names.contains(&name[..i]),
                "file/directory collision"
            );
        }
    }
    Ok(())
}

/// Conservatively reject normalized/casefold aliases across receiving systems.
/// This uses the existing complete casefold table, not a hand-maintained subset.
pub fn path_collision_key(value: &str) -> String {
    crate::unicode_casefold::casefold(&value.nfc().collect::<String>())
        .nfc()
        .collect()
}
/// The same conservative compatibility key used by job inventories.
/// Build-source inventories reuse it with their own, separately bounded budget.
pub fn compatibility_collision_key(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_uppercase)
        .collect::<String>()
        .nfkc()
        .collect()
}

/// The adapter must enumerate/hash ordinary files without following links, and
/// reject all unaccounted members. These arrays represent its observations.
pub fn verify_inventory(expected: &[FileEntry], observed: &[FileEntry]) -> Result<()> {
    validate_inventory(expected)?;
    validate_inventory(observed)?;
    let mut expected = expected.to_vec();
    expected.sort_by(|a, b| a.path.cmp(&b.path));
    let mut observed = observed.to_vec();
    observed.sort_by(|a, b| a.path.cmp(&b.path));
    ensure!(expected == observed, "file closure differs");
    Ok(())
}
pub fn inventory_digest(files: &[FileEntry]) -> Result<Digest> {
    validate_inventory(files)?;
    let mut files = files.to_vec();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    crate::record_digest(&files)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootIdentity {
    pub host_id: String,
    pub volume_id: String,
    pub directory_id: String,
}
impl RootIdentity {
    pub fn validate(&self) -> Result<()> {
        for value in [&self.host_id, &self.volume_id, &self.directory_id] {
            checked_label(value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub host_id: String,
    pub pid: u32,
    pub creation_time_100ns: u64,
    pub image: BlobIdentity,
}
impl ProcessIdentity {
    pub fn validate(&self) -> Result<()> {
        checked_label(&self.host_id)?;
        ensure!(
            self.pid != 0 && self.creation_time_100ns != 0 && self.image.bytes != 0,
            "invalid process identity"
        );
        Ok(())
    }
}

/// This identifies the explicitly pinned SDK libraries, not every DLL loaded
/// by NX or the operating system. The adapter must observe actual loaded bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeIdentity {
    pub product: String,
    pub product_label: String,
    pub architecture: String,
    pub libraries: Vec<FileEntry>,
}
impl RuntimeIdentity {
    pub fn validate(&self) -> Result<()> {
        for value in [&self.product, &self.product_label, &self.architecture] {
            checked_label(value)?;
        }
        ensure!(!self.libraries.is_empty(), "no pinned native library");
        // Installed vendor DLLs are not limited by the 100 MiB job-transfer cap.
        ensure!(
            self.libraries.len() <= 128,
            "too many pinned runtime libraries"
        );
        let mut names = BTreeSet::new();
        for file in &self.libraries {
            ensure!(
                file.identity.bytes > 0 && names.insert(path_collision_key(file.path.as_str())),
                "invalid runtime library identity"
            );
        }
        Ok(())
    }
}
