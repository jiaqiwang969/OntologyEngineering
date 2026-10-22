//! Public synthetic controls for the private acceptance-input loader.
//! These tests need no private payloads and never modify the process environment.
use semantica_runtime::sha256 as private_fixture_sha256;

#[cfg(unix)]
mod loader {
    use std::fs;
    include!("../../../build_support/private_test_fixtures.rs");

    const ABC: FixtureBinding = FixtureBinding {
        path: "synthetic/sample.txt",
        bytes: 3,
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    };

    fn fixture() -> (tempfile::TempDir, FixtureRoot) {
        let temporary = tempfile::tempdir().unwrap();
        // macOS's system temporary directory may have a symlink ancestor.
        // The explicit test root is its actual canonical path.
        let canonical = temporary.path().canonicalize().unwrap();
        fs::create_dir(canonical.join("synthetic")).unwrap();
        let root = FixtureRoot::from_path(Some(canonical.as_os_str())).unwrap();
        (temporary, root)
    }

    #[test]
    fn missing_relative_and_nonexistent_roots_refuse_without_fallback() {
        assert!(
            FixtureRoot::from_path(None)
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_root_required")
        );
        assert!(
            FixtureRoot::from_path(Some(OsStr::new("relative")))
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_root_invalid")
        );
        let (_temporary, root) = fixture();
        assert!(FixtureRoot::from_path(Some(root.root.join("missing").as_os_str())).is_err());
        let invalid = root.root.join("synthetic/../synthetic");
        assert!(
            FixtureRoot::from_path(Some(invalid.as_os_str()))
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_path_invalid")
        );
    }

    #[test]
    fn verified_bytes_are_returned_and_same_length_replacement_is_rejected() {
        let (_temporary, root) = fixture();
        let path = root.root.join(ABC.path);
        fs::write(&path, b"abc").unwrap();
        let verified = root.read_binding(&ABC).unwrap();
        let replacement = path.with_extension("replacement");
        fs::write(&replacement, b"xyz").unwrap();
        fs::rename(replacement, &path).unwrap();
        assert_eq!(verified, b"abc");
        assert!(
            root.read_binding(&ABC)
                .unwrap_err()
                .to_string()
                .contains("private_fixture_digest_mismatch")
        );
    }

    #[test]
    fn missing_truncated_growing_and_nonregular_payloads_refuse() {
        let (_temporary, root) = fixture();
        let path = root.root.join(ABC.path);
        assert!(
            root.read_binding(&ABC)
                .unwrap_err()
                .to_string()
                .contains("private_fixture_missing_or_unreadable")
        );
        for bad in [b"ab".as_slice(), b"abcd".as_slice()] {
            fs::write(&path, bad).unwrap();
            assert!(
                root.read_binding(&ABC)
                    .unwrap_err()
                    .to_string()
                    .contains("private_fixture_size_mismatch")
            );
        }
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(
            root.read_binding(&ABC)
                .unwrap_err()
                .to_string()
                .contains("private_fixture_not_regular")
        );
    }

    #[test]
    fn unknown_ids_and_catalog_path_escape_never_select_arbitrary_input() {
        let (_temporary, root) = fixture();
        for unknown in ["synthetic/sample.txt", "../escape", "/absolute"] {
            assert!(
                root.read(unknown)
                    .unwrap_err()
                    .to_string()
                    .contains("private_fixture_unknown_id")
            );
        }
        for bad in ["../escape", "/absolute", "synthetic/../escape", ""] {
            let binding = FixtureBinding { path: bad, ..ABC };
            assert!(
                root.read_binding(&binding)
                    .unwrap_err()
                    .to_string()
                    .contains("private_fixture_path_invalid")
            );
        }
        // The catalog is frozen metadata, not an on-disk manifest supplied by
        // the same caller who supplied the data. No missing original is skipped.
        let known = root
            .read("original/ORIGINAL-FROZEN-INPUTS.json")
            .unwrap_err();
        assert!(!known.to_string().contains("private_fixture_unknown_id"));
    }

    #[cfg(unix)]
    #[test]
    fn root_ancestor_and_payload_symlinks_are_not_private_input_authority() {
        use std::os::unix::fs::symlink;
        let (_temporary, root) = fixture();
        let outside = tempfile::tempdir().unwrap();
        let outside = outside.path().canonicalize().unwrap();
        let root_alias = outside.join("root-alias");
        symlink(&root.root, &root_alias).unwrap();
        assert!(
            FixtureRoot::from_path(Some(root_alias.as_os_str()))
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_directory_not_regular")
        );
        let path = root.root.join(ABC.path);
        let outside_file = outside.join("sample.txt");
        fs::write(&outside_file, b"abc").unwrap();
        symlink(&outside_file, &path).unwrap();
        assert!(
            root.read_binding(&ABC)
                .unwrap_err()
                .to_string()
                .contains("private_fixture_not_regular")
        );
        fs::remove_file(path).unwrap();
        fs::remove_dir(root.root.join("synthetic")).unwrap();
        symlink(&outside, root.root.join("synthetic")).unwrap();
        assert!(
            root.read_binding(&ABC)
                .unwrap_err()
                .to_string()
                .contains("private_fixture_directory_not_regular")
        );
    }

    #[test]
    fn explicit_environment_entry_has_no_fallback() {
        const PROBE: &str = "OE_PRIVATE_FIXTURE_LOADER_SYNTHETIC_PROBE";
        if std::env::var_os(PROBE).as_deref() == Some(OsStr::new("child")) {
            // Reaches the same entry used by all actual private acceptance tests.
            // Parent selects only missing-input cases; this must panic, not skip.
            let _ = text("original/ORIGINAL-FROZEN-INPUTS.json");
            panic!("private fixture unexpectedly accepted");
        }
        let (_temporary, root) = fixture();
        for (path, expected) in [
            (None, "private_fixture_root_required"),
            (
                Some(PathBuf::from("relative")),
                "private_fixture_root_invalid",
            ),
            (
                Some(root.root.clone()),
                "private_fixture_directory_missing_or_unreadable",
            ),
        ] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "loader::explicit_environment_entry_has_no_fallback",
                    "--nocapture",
                ])
                .env(PROBE, "child")
                .env_remove(ROOT_ENV);
            if let Some(path) = path {
                command.env(ROOT_ENV, path);
            }
            let output = command.output().unwrap();
            assert!(!output.status.success());
            let output = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.contains(expected), "{output}");
        }
    }

    #[test]
    fn fixed_resource_budgets_reject_before_open_or_allocation() {
        let huge = PathBuf::from(format!("/{}", "x".repeat(MAX_ROOT_BYTES)));
        assert!(
            FixtureRoot::from_path(Some(huge.as_os_str()))
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_root_budget")
        );
        let deep = PathBuf::from(format!("/{}", vec!["x"; MAX_ROOT_COMPONENTS].join("/")));
        assert!(
            FixtureRoot::from_path(Some(deep.as_os_str()))
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_root_budget")
        );
        let (_temporary, root) = fixture();
        let oversized = FixtureBinding {
            bytes: MAX_FIXTURE_BYTES + 1,
            ..ABC
        };
        assert!(
            root.read_binding(&oversized)
                .unwrap_err()
                .to_string()
                .contains("private_fixture_size_budget")
        );
    }

    #[test]
    fn held_directory_anchors_do_not_follow_replaced_parent_or_root() {
        use std::os::unix::fs::symlink;
        let (_temporary, root) = fixture();
        let path = root.root.join(ABC.path);
        fs::write(&path, b"abc").unwrap();
        let held = relative_directory(&root.directory, Path::new("synthetic")).unwrap();
        let outside_temporary = tempfile::tempdir().unwrap();
        let outside = outside_temporary.path().canonicalize().unwrap();
        fs::write(outside.join("sample.txt"), b"xyz").unwrap();
        fs::rename(root.root.join("synthetic"), root.root.join("retained")).unwrap();
        symlink(&outside, root.root.join("synthetic")).unwrap();
        // This is the exact leaf-open primitive used by read_binding. An opened
        // directory remains anchored even after its former name is redirected.
        let mut bytes = Vec::new();
        regular_file_at(&held, OsStr::new("sample.txt"))
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"abc");
        assert!(
            root.read_binding(&ABC)
                .unwrap_err()
                .to_string()
                .contains("private_fixture_directory_not_regular")
        );
        let root_name = root.root.clone();
        let parked = outside.join("parked-root");
        fs::rename(&root_name, &parked).unwrap();
        fs::create_dir(&root_name).unwrap();
        assert!(
            root.read_binding(&ABC)
                .unwrap_err()
                .to_string()
                .contains("private_fixture_root_replaced")
        );
    }

    #[test]
    fn substituted_fifo_open_is_nonblocking_in_a_deadline_supervised_child() {
        const PROBE: &str = "OE_PRIVATE_FIXTURE_FIFO_SYNTHETIC_PROBE";
        if std::env::var_os(PROBE).as_deref() == Some(OsStr::new("child")) {
            use std::os::unix::ffi::OsStrExt;
            let (_temporary, root) = fixture();
            let path = root.root.join(ABC.path);
            fs::write(&path, b"abc").unwrap();
            assert!(fs::symlink_metadata(&path).unwrap().is_file());
            let parent = relative_directory(&root.directory, Path::new("synthetic")).unwrap();
            // Deterministically place a FIFO after the old-style path precheck,
            // before the actual production leaf-open primitive. No writer exists.
            fs::remove_file(&path).unwrap();
            let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            // SAFETY: c_path remains valid; mkfifo creates this owned temp path.
            assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
            assert!(
                regular_file_at(&parent, OsStr::new("sample.txt"))
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("private_fixture_not_regular")
            );
            assert!(
                root.read_binding(&ABC)
                    .unwrap_err()
                    .to_string()
                    .contains("private_fixture_not_regular")
            );
            println!("fifo_rejected_without_writer");
            return;
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "loader::substituted_fifo_open_is_nonblocking_in_a_deadline_supervised_child",
                "--nocapture",
            ])
            .env(PROBE, "child")
            .env_remove(ROOT_ENV)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("private fixture FIFO open exceeded five-second child deadline");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("fifo_rejected_without_writer"));
    }
}

#[cfg(windows)]
mod windows_loader {
    use std::{
        fs,
        os::windows::{
            fs::{OpenOptionsExt, symlink_dir, symlink_file},
            io::AsRawHandle,
        },
    };
    include!("../../../build_support/private_test_fixtures.rs");

    const ABC: FixtureBinding = FixtureBinding {
        path: "synthetic/sample.txt",
        bytes: 3,
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    };

    fn fixture() -> (tempfile::TempDir, FixtureRoot) {
        let temporary = tempfile::tempdir().unwrap();
        let canonical = temporary.path().canonicalize().unwrap();
        fs::create_dir(canonical.join("synthetic")).unwrap();
        let root = FixtureRoot::from_path(Some(canonical.as_os_str())).unwrap();
        (temporary, root)
    }

    fn refuses(result: io::Result<Vec<u8>>, code: &str) {
        let error = result.unwrap_err().to_string();
        assert!(error.contains(code), "{error}");
    }

    // Native unprivileged directory-junction fixture. Never invoke cmd, never
    // follow/copy another directory tree, and remove the junction before cleanup.
    fn junction(link: &Path, target: &Path) {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn DeviceIoControl(
                handle: std::os::windows::io::RawHandle,
                code: u32,
                input: *const std::ffi::c_void,
                input_bytes: u32,
                output: *mut std::ffi::c_void,
                output_bytes: u32,
                returned_bytes: *mut u32,
                overlapped: *mut std::ffi::c_void,
            ) -> i32;
        }
        fs::create_dir(link).unwrap();
        let target = target.canonicalize().unwrap();
        let display = target.to_str().unwrap();
        let print = display.strip_prefix("\\\\?\\").unwrap_or(display);
        let substitute: Vec<u16> = format!("\\??\\{print}").encode_utf16().collect();
        let printed: Vec<u16> = print.encode_utf16().collect();
        let mut data = Vec::new();
        data.extend_from_slice(&0xA0000003u32.to_le_bytes());
        data.extend_from_slice(
            &u16::try_from(8 + (substitute.len() + printed.len() + 2) * 2)
                .unwrap()
                .to_le_bytes(),
        );
        data.extend_from_slice(&0u16.to_le_bytes());
        for value in [
            0,
            substitute.len() * 2,
            (substitute.len() + 1) * 2,
            printed.len() * 2,
        ] {
            data.extend_from_slice(&u16::try_from(value).unwrap().to_le_bytes());
        }
        for value in substitute.into_iter().chain([0]).chain(printed).chain([0]) {
            data.extend_from_slice(&value.to_le_bytes());
        }
        let handle = fs::OpenOptions::new()
            .access_mode(0x40000000)
            .share_mode(7)
            .custom_flags(0x02000000 | 0x00200000)
            .open(link)
            .unwrap();
        let mut returned = 0;
        // SAFETY: the owned handle is a newly created directory; data is a fully
        // initialized bounded MOUNT_POINT reparse buffer, valid during this call.
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    handle.as_raw_handle(),
                    0x000900A4,
                    data.as_ptr().cast(),
                    data.len() as u32,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            },
            0,
            "junction creation: {}",
            io::Error::last_os_error()
        );
    }

    #[test]
    fn missing_relative_and_nonexistent_roots_refuse_without_fallback() {
        assert!(
            FixtureRoot::from_path(None)
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_root_required")
        );
        for path in ["relative", "C:relative", "\\rooted-without-drive"] {
            assert!(
                FixtureRoot::from_path(Some(OsStr::new(path)))
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("private_fixture_root_invalid")
            );
        }
        let (_temporary, root) = fixture();
        assert!(FixtureRoot::from_path(Some(root.root.join("missing").as_os_str())).is_err());
        // PathBuf::join normalizes ParentDir for a verbatim Windows prefix.
        // Supply the actual traversal text to the reader, before path cleanup.
        let mut invalid = root.root.as_os_str().to_os_string();
        invalid.push("\\synthetic\\..\\synthetic");
        assert!(
            FixtureRoot::from_path(Some(invalid.as_os_str()))
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_path_invalid")
        );
    }

    #[test]
    fn local_drive_forms_accept_and_unc_device_stream_aliases_refuse() {
        let (_temporary, root) = fixture();
        let canonical = root.root.to_str().unwrap();
        let ordinary = canonical.strip_prefix("\\\\?\\").unwrap_or(canonical);
        assert!(FixtureRoot::from_path(Some(OsStr::new(ordinary))).is_ok());
        for path in [
            "\\\\server\\share\\inputs",
            "\\\\?\\UNC\\server\\share\\inputs",
            "\\\\.\\pipe\\fixture",
            "\\\\?\\GLOBALROOT\\Device\\HarddiskVolume1\\inputs",
        ] {
            assert!(
                FixtureRoot::from_path(Some(OsStr::new(path))).is_err(),
                "{path}"
            );
        }
        for suffix in [
            "synthetic:stream",
            "NUL",
            "con.txt",
            "COM1",
            "trailing.",
            "trailing ",
        ] {
            assert!(
                FixtureRoot::from_path(Some(root.root.join(suffix).as_os_str()))
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("private_fixture_path_invalid"),
                "{suffix}"
            );
        }
    }

    #[test]
    fn native_volume_policy_rejects_remote_flag_and_unknown_device() {
        // Frozen native values: FILE_DEVICE_DISK=7, FILE_REMOTE_DEVICE=0x10.
        // These are policy controls, not a claim of an actual SMB mapping test.
        assert!(windows_fixture::validate_local_device(7, 0).is_ok());
        assert!(windows_fixture::validate_local_device(7, 0x20).is_ok());
        for characteristics in [0x10, 0x30, 0xFFFF_FFFF] {
            assert!(
                windows_fixture::validate_local_device(7, characteristics)
                    .unwrap_err()
                    .to_string()
                    .contains("private_fixture_remote_volume")
            );
        }
        for device_type in [0, 1, 0x14, 0x28, 0x2e, u32::MAX] {
            assert!(
                windows_fixture::validate_local_device(device_type, 0)
                    .unwrap_err()
                    .to_string()
                    .contains("private_fixture_volume_not_local_disk")
            );
        }
    }

    #[test]
    fn verified_bytes_are_returned_and_same_length_replacement_is_rejected() {
        let (_temporary, root) = fixture();
        let path = root.root.join(ABC.path);
        fs::write(&path, b"abc").unwrap();
        let verified = root.read_binding(&ABC).unwrap();
        fs::write(&path, b"xyz").unwrap();
        assert_eq!(verified, b"abc");
        refuses(root.read_binding(&ABC), "private_fixture_digest_mismatch");
    }

    #[test]
    fn missing_truncated_growing_and_nonregular_payloads_refuse() {
        let (_temporary, root) = fixture();
        let path = root.root.join(ABC.path);
        refuses(
            root.read_binding(&ABC),
            "private_fixture_missing_or_unreadable",
        );
        for bad in [b"ab".as_slice(), b"abcd".as_slice()] {
            fs::write(&path, bad).unwrap();
            refuses(root.read_binding(&ABC), "private_fixture_size_mismatch");
        }
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        refuses(root.read_binding(&ABC), "private_fixture_not_regular");
    }

    #[test]
    fn unknown_ids_and_catalog_path_escape_never_select_arbitrary_input() {
        let (_temporary, root) = fixture();
        for id in ["synthetic/sample.txt", "../escape", "C:\\absolute"] {
            refuses(root.read(id), "private_fixture_unknown_id");
        }
        for bad in [
            "../escape",
            "C:\\absolute",
            "synthetic/../escape",
            "",
            "synthetic/sample.txt:stream",
            "synthetic/NUL",
        ] {
            refuses(
                root.read_binding(&FixtureBinding { path: bad, ..ABC }),
                "private_fixture_path_invalid",
            );
        }
        let known = root
            .read("original/ORIGINAL-FROZEN-INPUTS.json")
            .unwrap_err();
        assert!(!known.to_string().contains("private_fixture_unknown_id"));
    }

    #[test]
    fn root_and_ancestor_junctions_are_rejected_without_following() {
        let (_temporary, root) = fixture();
        let outside = tempfile::tempdir().unwrap();
        let root_alias = outside.path().join("root-alias");
        junction(&root_alias, &root.root);
        assert!(
            FixtureRoot::from_path(Some(root_alias.as_os_str()))
                .err()
                .unwrap()
                .to_string()
                .contains("private_fixture_directory_not_regular")
        );
        fs::remove_dir(&root_alias).unwrap();
        fs::write(outside.path().join("sample.txt"), b"abc").unwrap();
        fs::remove_dir(root.root.join("synthetic")).unwrap();
        junction(&root.root.join("synthetic"), outside.path());
        refuses(
            root.read_binding(&ABC),
            "private_fixture_directory_not_regular",
        );
        fs::remove_dir(root.root.join("synthetic")).unwrap();
    }

    #[test]
    fn root_directory_and_payload_symlinks_are_rejected() {
        let (_temporary, root) = fixture();
        let outside = tempfile::tempdir().unwrap();
        let root_alias = outside.path().join("root-alias");
        // Creating these controls requires Windows symlink authority. Its absence
        // is a failed/unperformed receiving obligation, never an automatic pass.
        symlink_dir(&root.root, &root_alias)
            .expect("Windows symlink control requires symlink authority");
        assert!(FixtureRoot::from_path(Some(root_alias.as_os_str())).is_err());
        fs::remove_dir(root_alias).unwrap();
        let external = outside.path().join("sample.txt");
        fs::write(&external, b"abc").unwrap();
        symlink_file(&external, root.root.join(ABC.path)).unwrap();
        refuses(root.read_binding(&ABC), "private_fixture_not_regular");
        fs::remove_file(root.root.join(ABC.path)).unwrap();
        fs::remove_dir(root.root.join("synthetic")).unwrap();
        symlink_dir(outside.path(), root.root.join("synthetic")).unwrap();
        refuses(
            root.read_binding(&ABC),
            "private_fixture_directory_not_regular",
        );
        fs::remove_dir(root.root.join("synthetic")).unwrap();
    }

    #[test]
    fn held_directory_anchors_survive_parent_rename_and_detect_root_replacement() {
        let (_temporary, root) = fixture();
        fs::write(root.root.join(ABC.path), b"abc").unwrap();
        let held =
            windows_fixture::relative_directory(&root.directory, Path::new("synthetic")).unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("sample.txt"), b"xyz").unwrap();
        fs::rename(root.root.join("synthetic"), root.root.join("retained")).unwrap();
        junction(&root.root.join("synthetic"), outside.path());
        let mut bytes = Vec::new();
        windows_fixture::regular_file_at(&held, OsStr::new("sample.txt"))
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"abc");
        refuses(
            root.read_binding(&ABC),
            "private_fixture_directory_not_regular",
        );
        fs::remove_dir(root.root.join("synthetic")).unwrap();
        // Windows may refuse renaming an ancestor of an open descendant. The
        // parent-anchor obligation above has completed; retain the root handle
        // itself while separately exercising a real root-name replacement.
        drop(held);
        let parked = root.root.with_extension("parked");
        fs::rename(&root.root, &parked).unwrap();
        fs::create_dir(&root.root).unwrap();
        refuses(root.read_binding(&ABC), "private_fixture_root_replaced");
        drop(root);
        fs::remove_dir_all(parked).unwrap();
    }

    #[test]
    fn acquired_payload_replacement_and_in_place_change_fail_identity_recheck() {
        let (_temporary, root) = fixture();
        let relative = Path::new(ABC.path);
        let path = root.root.join(relative);
        fs::write(&path, b"abc").unwrap();
        let parent =
            windows_fixture::relative_directory(&root.directory, Path::new("synthetic")).unwrap();
        let file = windows_fixture::regular_file_at(&parent, OsStr::new("sample.txt")).unwrap();
        let identity = windows_fixture::identity(&file, false).unwrap();
        // Same bytes and size cannot hide a changed 128-bit file identity.
        fs::rename(&path, path.with_extension("retained")).unwrap();
        fs::write(&path, b"abc").unwrap();
        let error = windows_fixture::verify_after(
            &root.root,
            &root.directory,
            relative,
            &parent,
            &file,
            &identity,
        )
        .unwrap_err();
        assert!(error.to_string().contains("private_fixture_replaced"));
        let file = windows_fixture::regular_file_at(&parent, OsStr::new("sample.txt")).unwrap();
        let identity = windows_fixture::identity(&file, false).unwrap();
        fs::write(&path, b"abcd").unwrap();
        let error = windows_fixture::verify_after(
            &root.root,
            &root.directory,
            relative,
            &parent,
            &file,
            &identity,
        )
        .unwrap_err();
        assert!(error.to_string().contains("private_fixture_replaced"));
    }

    #[test]
    fn acquired_parent_replacement_fails_directory_identity_recheck() {
        let (_temporary, root) = fixture();
        let relative = Path::new(ABC.path);
        fs::write(root.root.join(relative), b"abc").unwrap();
        let parent =
            windows_fixture::relative_directory(&root.directory, Path::new("synthetic")).unwrap();
        // Place the replacement after parent acquisition, before leaf open: a
        // Windows directory with an already-open descendant cannot always be
        // renamed. This still exercises the actual anchored leaf primitive and
        // the same post-read directory identity check with a real replacement.
        fs::rename(root.root.join("synthetic"), root.root.join("retained")).unwrap();
        fs::create_dir(root.root.join("synthetic")).unwrap();
        fs::write(root.root.join(relative), b"abc").unwrap();
        let file = windows_fixture::regular_file_at(&parent, OsStr::new("sample.txt")).unwrap();
        let identity = windows_fixture::identity(&file, false).unwrap();
        let error = windows_fixture::verify_after(
            &root.root,
            &root.directory,
            relative,
            &parent,
            &file,
            &identity,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("private_fixture_directory_replaced")
        );
        let mut retained = Vec::new();
        (&file).read_to_end(&mut retained).unwrap();
        assert_eq!(retained, b"abc");
    }

    #[test]
    fn fixed_resource_budgets_reject_before_open_or_allocation() {
        for huge in [
            format!("C:\\{}", "x".repeat(MAX_ROOT_BYTES)),
            format!("C:\\{}", vec!["x"; MAX_ROOT_COMPONENTS].join("\\")),
        ] {
            assert!(
                FixtureRoot::from_path(Some(OsStr::new(&huge)))
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("private_fixture_root_budget")
            );
        }
        let (_temporary, root) = fixture();
        refuses(
            root.read_binding(&FixtureBinding {
                bytes: MAX_FIXTURE_BYTES + 1,
                ..ABC
            }),
            "private_fixture_size_budget",
        );
    }

    #[test]
    fn explicit_environment_entry_has_no_fallback() {
        const PROBE: &str = "OE_PRIVATE_FIXTURE_WINDOWS_SYNTHETIC_PROBE";
        if std::env::var_os(PROBE).as_deref() == Some(OsStr::new("child")) {
            let _ = text("original/ORIGINAL-FROZEN-INPUTS.json");
            panic!("private fixture unexpectedly accepted");
        }
        let (_temporary, root) = fixture();
        for (path, expected) in [
            (None, "private_fixture_root_required"),
            (
                Some(PathBuf::from("relative")),
                "private_fixture_root_invalid",
            ),
            (
                Some(root.root.clone()),
                "private_fixture_directory_missing_or_unreadable",
            ),
        ] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "windows_loader::explicit_environment_entry_has_no_fallback",
                    "--nocapture",
                ])
                .env(PROBE, "child")
                .env_remove(ROOT_ENV);
            if let Some(path) = path {
                command.env(ROOT_ENV, path);
            }
            let output = command.output().unwrap();
            assert!(!output.status.success());
            let output = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.contains(expected), "{output}");
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod unsupported {
    include!("../../../build_support/private_test_fixtures.rs");

    #[test]
    fn nonunix_private_inputs_have_an_explicit_unsupported_result() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().canonicalize().unwrap();
        let error = FixtureRoot::from_path(Some(path.as_os_str()))
            .err()
            .unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
        assert!(
            error
                .to_string()
                .contains("private_fixture_anchored_open_platform_unsupported")
        );
    }
}
