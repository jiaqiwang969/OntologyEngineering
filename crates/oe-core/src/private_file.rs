//! Read private input with content and access policy bound to one file object.
//! The caller retains the snapshot through its observation and rechecks it.
use anyhow::{Result, ensure};
#[cfg(unix)]
use std::io::Read;
use std::path::Path;

#[cfg(windows)]
mod windows;

pub struct Snapshot {
    pub bytes: Vec<u8>,
    pub identity: crate::FileIdentity,
    binding: Vec<u8>,
}

impl Snapshot {
    pub fn recheck(&self, path: &Path, limit: usize) -> Result<()> {
        let now = read(path, limit)?;
        ensure!(
            now.identity == self.identity && now.binding == self.binding,
            "private_identity_changed_during_check"
        );
        Ok(())
    }
}

pub fn read(path: &Path, limit: usize) -> Result<Snapshot> {
    #[cfg(windows)]
    {
        return windows::read(path, limit);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mut file = crate::open_regular_file(path, limit as u64)?;
        let binding = |file: &std::fs::File| -> Result<Vec<u8>> {
            let m = file.metadata()?;
            ensure!(
                m.mode() & 0o077 == 0,
                "private_identity_file_permissions_required"
            );
            Ok(format!(
                "{}:{}:{}:{}:{}",
                m.dev(),
                m.ino(),
                m.mode(),
                m.uid(),
                m.gid()
            )
            .into_bytes())
        };
        let before = binding(&file)?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(
                (limit as u64)
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("invalid_read_limit"))?,
            )
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= limit, "input_exceeds_limit");
        ensure!(
            binding(&file)? == before,
            "private_identity_changed_during_read"
        );
        let current = crate::open_regular_file(path, limit as u64)?;
        ensure!(
            binding(&current)? == before,
            "private_identity_changed_during_read"
        );
        let identity = crate::FileIdentity {
            bytes: bytes.len() as u64,
            sha256: crate::sha256(&bytes),
        };
        ensure!(
            crate::fingerprint_file(path, limit as u64)? == identity,
            "private_identity_changed_during_read"
        );
        Ok(Snapshot {
            bytes,
            identity,
            binding: before,
        })
    }
    #[cfg(not(any(unix, windows)))]
    anyhow::bail!("private_identity_permissions_not_verified_on_platform")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn private_input_rechecks_permissions_and_same_content_replacement() {
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oe-private-unix-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("input");
        std::fs::write(&path, b"synthetic").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let first = read(&path, 64).unwrap();
        first.recheck(&path, 64).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(first.recheck(&path, 64).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let replacement = dir.join("replacement");
        std::fs::write(&replacement, b"synthetic").unwrap();
        std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::rename(replacement, &path).unwrap();
        assert!(first.recheck(&path, 64).is_err());
        assert!(read(&path, 2).is_err());
        let link = dir.join("symlink");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read(&link, 64).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
