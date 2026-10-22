//! The existing compiler stdin role permits an empty input. Native tools and
//! images retain their separate, correctly nonempty original file verifier.
use anyhow::{Result, ensure};
use oe_contracts::CodeNativeFileBinding;
use oe_core::{fingerprint_file, no_symlinks};
use std::path::Path;

#[cfg(unix)]
fn physical(path: &Path) -> Result<(u64, u64, u64, u64)> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "probe_stdin_not_regular"
    );
    Ok((
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        u64::from(metadata.permissions().mode() & 0o7777),
    ))
}

#[cfg(not(unix))]
fn physical(_: &Path) -> Result<(u64, u64, u64, u64)> {
    anyhow::bail!("probe_stdin_file_platform_uncovered")
}

/// Content verification only: this neither captures a FIFO nor authorizes an
/// execution. The original owner supplies the input limit and exact file role.
pub(crate) fn verify_stdin_file9(
    pin: &CodeNativeFileBinding,
    original_limit: u64,
    expected_mode: u64,
) -> Result<()> {
    pin.validate()
        .map_err(|e| anyhow::anyhow!("probe_stdin_file_contract:{e:?}"))?;
    let path = Path::new(&pin.path);
    ensure!(
        path.is_absolute()
            && (1..=4 * 1024 * 1024).contains(&original_limit)
            && pin.bytes <= original_limit
            && matches!(expected_mode, 0o600 | 0o400)
            && pin.mode == expected_mode,
        "probe_stdin_file_role_or_bounds"
    );
    no_symlinks(path)?;
    let before = physical(path)?;
    let first = fingerprint_file(path, original_limit)?;
    let after = physical(path)?;
    let second = fingerprint_file(path, original_limit)?;
    ensure!(
        before == after
            && after == physical(path)?
            && before.2 == pin.bytes
            && before.3 == expected_mode
            && first == second
            && first.bytes == pin.bytes
            && first.sha256 == pin.sha256,
        "probe_stdin_file_identity_changed"
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    fn fixture(bytes: &[u8]) -> (tempfile::TempDir, CodeNativeFileBinding) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap();
        std::fs::create_dir_all(root.join("var/builds")).unwrap();
        let temp = tempfile::tempdir_in(root.join("var/builds")).unwrap();
        let path = temp.path().join("original-input.bin");
        oe_core::write_new_bytes(&path, bytes).unwrap();
        let pin = crate::native_runtime::file_binding(&path).unwrap();
        (temp, pin)
    }
    #[test]
    fn stdin_role_accepts_actual_empty_capture_and_replay_without_empty_tool_admission() {
        let (_temp, capture) = fixture(b"");
        verify_stdin_file9(&capture, 4 * 1024 * 1024, 0o600).unwrap();
        assert!(
            crate::native_profile::verify_file(&capture)
                .unwrap_err()
                .to_string()
                .contains("native_file_bounds")
        );
        let mut replay = capture.clone();
        std::fs::set_permissions(&replay.path, std::fs::Permissions::from_mode(0o400)).unwrap();
        replay.mode = 0o400;
        verify_stdin_file9(&replay, 4 * 1024 * 1024, 0o400).unwrap();
        assert!(verify_stdin_file9(&capture, 4 * 1024 * 1024, 0o600).is_err());
    }
    #[test]
    fn stdin_role_rejects_changed_content_count_hash_mode_limit_and_source_path() {
        let (temp, pin) = fixture(b"bounded input");
        verify_stdin_file9(&pin, 64, 0o600).unwrap();
        let mut changed = pin.clone();
        changed.sha256 = "b".repeat(64);
        assert!(verify_stdin_file9(&changed, 64, 0o600).is_err());
        changed = pin.clone();
        changed.bytes -= 1;
        assert!(verify_stdin_file9(&changed, 64, 0o600).is_err());
        assert!(verify_stdin_file9(&pin, pin.bytes - 1, 0o600).is_err());
        for (limit, mode) in [
            (0, 0o600),
            (4 * 1024 * 1024 + 1, 0o600),
            (64, 0o700),
            (64, 0o400),
        ] {
            assert!(verify_stdin_file9(&pin, limit, mode).is_err());
        }
        let link = temp.path().join("symlink.bin");
        symlink(&pin.path, &link).unwrap();
        changed = pin.clone();
        changed.path = link.to_str().unwrap().into();
        assert!(verify_stdin_file9(&changed, 64, 0o600).is_err());
        std::fs::write(&pin.path, b"different!!!").unwrap();
        assert!(verify_stdin_file9(&pin, 64, 0o600).is_err());
    }
}
