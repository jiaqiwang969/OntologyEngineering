use std::{
    env, fmt,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_CREDENTIAL_BYTES: u64 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CredentialFileError {
    #[error("credential_file_symlink")]
    Symlink,
    #[error("credential_file_requires_owner_and_mode_0600")]
    OwnerAndMode,
    #[error("credential_file_too_large")]
    TooLarge,
    #[error("credential_file_requires_one_key_or_one_key_per_line")]
    UniqueKey,
    #[error("credential_file_unavailable")]
    Unavailable,
    #[error("credential_file_invalid_utf8")]
    InvalidUtf8,
    #[error("credential_home_unavailable")]
    HomeUnavailable,
    #[error("unsupported_credential_user_expansion")]
    UserExpansionUnsupported,
    #[error("unix_credential_permissions_required")]
    UnixRequired,
}

pub(crate) struct Credential(Vec<String>, String);

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credential([REDACTED])")
    }
}

pub(crate) fn home() -> Result<PathBuf, CredentialFileError> {
    env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .ok_or(CredentialFileError::HomeUnavailable)
}

pub(crate) fn expand_user(path: &Path) -> Result<PathBuf, CredentialFileError> {
    let text = path.as_os_str().to_string_lossy();
    if text == "~" {
        return home();
    }
    if let Some(relative) = text.strip_prefix("~/") {
        return Ok(home()?.join(relative));
    }
    if text.starts_with('~') {
        return Err(CredentialFileError::UserExpansionUnsupported);
    }
    Ok(path.to_path_buf())
}

pub(crate) fn symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

#[cfg(unix)]
pub(crate) fn open_owned(path: &Path, exact_mode: bool) -> Result<File, CredentialFileError> {
    use std::os::unix::fs::OpenOptionsExt;
    if symlink(path) {
        return Err(CredentialFileError::Symlink);
    }
    // O_NONBLOCK avoids hanging on a FIFO before fstat can reject non-files.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| CredentialFileError::Unavailable)?;
    let info = file
        .metadata()
        .map_err(|_| CredentialFileError::Unavailable)?;
    // SAFETY: getuid has no inputs, no pointers and no failure return.
    let uid = unsafe { libc::getuid() };
    verify_owner_and_mode(&info, uid, exact_mode)?;
    Ok(file)
}

#[cfg(unix)]
fn verify_owner_and_mode(
    info: &fs::Metadata,
    uid: u32,
    exact_mode: bool,
) -> Result<(), CredentialFileError> {
    use std::os::unix::fs::MetadataExt;
    if !info.is_file() || info.uid() != uid || (exact_mode && info.mode() & 0o7777 != 0o600) {
        Err(CredentialFileError::OwnerAndMode)
    } else {
        Ok(())
    }
}

#[cfg(not(unix))]
pub(crate) fn open_owned(_path: &Path, _exact_mode: bool) -> Result<File, CredentialFileError> {
    Err(CredentialFileError::UnixRequired)
}

pub(crate) fn bounded_read(file: &mut File) -> Result<Vec<u8>, CredentialFileError> {
    let mut raw = Vec::new();
    file.take(MAX_CREDENTIAL_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|_| CredentialFileError::Unavailable)?;
    if raw.len() as u64 > MAX_CREDENTIAL_BYTES {
        return Err(CredentialFileError::TooLarge);
    }
    Ok(raw)
}

impl Credential {
    pub(crate) fn is_openai_single(&self) -> bool {
        self.0.len() == 1 && self.0[0].starts_with("sk-")
    }
    /// Owner-selected OpenAI bearer; exactly one explicit token.
    pub(crate) fn read_openai_bearer(path: &Path) -> Result<Self, CredentialFileError> {
        let credential = Self::read_single_http_bearer(path)?;
        if !credential.0[0].starts_with("sk-") || credential.0[0].len() <= 3 {
            return Err(CredentialFileError::UniqueKey);
        }
        Ok(credential)
    }
    /// Explicit single HTTP bearer input; no prose extraction or account rotation.
    pub(crate) fn read_single_http_bearer(path: &Path) -> Result<Self, CredentialFileError> {
        let path = expand_user(path)?;
        let mut file = open_owned(&path, true)?;
        let raw = bounded_read(&mut file)?;
        let content = std::str::from_utf8(&raw).map_err(|_| CredentialFileError::InvalidUtf8)?;
        let key = content.strip_suffix('\n').unwrap_or(content);
        // RFC6750 b64token; exactly one token, no trimming or prose recovery.
        let stem = key.trim_end_matches('=');
        if stem.is_empty()
            || !stem.bytes().all(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'+' | b'/')
            })
            || !key[stem.len()..].bytes().all(|b| b == b'=')
        {
            return Err(CredentialFileError::UniqueKey);
        }
        Ok(Self(vec![key.to_owned()], sha(&raw)))
    }
    pub(crate) fn contains_in(&self, bytes: &[u8]) -> bool {
        self.0.iter().any(|key| {
            bytes
                .windows(key.len())
                .any(|window| window == key.as_bytes())
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn generation(&self) -> &str {
        &self.1
    }

    pub(crate) fn bearer(&self, index: usize) -> String {
        format!("Bearer {}", self.0[index])
    }
}

pub(crate) fn sha(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
