//! Content checks and bounded input observation for a separate compiler-probe
//! domain. These functions do not dispatch rustc, attest a living producer,
//! select a version, own a budget or turn retained JSON into execution authority.
use crate::{
    CodeNativeCompilationSourceRoot4, CodeNativeCompilerProbeDescriptor9,
    CodeNativeCompilerProbeDomain9, CodeNativeCompilerProbeStdin9,
};
use anyhow::{Context, Result, ensure};
use oe_core::{no_symlinks, sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
    time::Instant,
};

/// Values borrowed from the original owner's current stage. This content-only
/// view cannot substitute for that owner's source, producer or process checks.
pub struct ProbeInputScope9<'a> {
    pub target_dir: &'a Path,
    pub platform: &'a str,
    pub stage: &'a str,
    pub cargo_out_dir: &'a Path,
    pub encoded_flags: &'a str,
    pub actual_encoded_flags: &'a str,
    pub cwd: &'a Path,
}

fn ordinary_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_)))
        && !path.as_os_str().to_string_lossy().contains('\\')
}

fn output_namespace(package: &str, scope: &ProbeInputScope9<'_>) -> Result<()> {
    ensure!(
        scope.platform == "aarch64-apple-darwin"
            && ["build-normalizer", "build-runner"].contains(&scope.stage)
            && ordinary_absolute(scope.target_dir)
            && ordinary_absolute(scope.cargo_out_dir),
        "probe9_selected_stage_or_path"
    );
    ensure!(
        scope
            .cargo_out_dir
            .file_name()
            .is_some_and(|name| name == "out"),
        "probe9_cargo_out_dir"
    );
    let folder = scope
        .cargo_out_dir
        .parent()
        .context("probe9_output_folder")?;
    let name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .context("probe9_output_encoding")?;
    let fingerprint = name
        .strip_prefix(&format!("{package}-"))
        .context("probe9_output_package")?;
    ensure!(
        fingerprint.len() == 16
            && fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "probe9_output_fingerprint"
    );
    let host = scope.target_dir.join("debug/build");
    let target = scope.target_dir.join(scope.platform).join("debug/build");
    ensure!(
        folder.parent() == Some(host.as_path())
            || (scope.stage == "build-runner" && folder.parent() == Some(target.as_path())),
        "probe9_output_not_same_stage"
    );
    Ok(())
}

struct Parsed {
    source: String,
    ordinal: u64,
    out_dir: String,
    emit: String,
}

fn parse_arguments(
    package: &str,
    arguments: &[String],
    scope: &ProbeInputScope9<'_>,
) -> Result<Parsed> {
    ensure!(
        !arguments.is_empty()
            && arguments.len() <= 128
            && arguments.iter().map(String::len).sum::<usize>() <= 65536
            && arguments.iter().all(|value| !value.contains('\0')),
        "probe9_argument_limit"
    );
    ensure!(
        !scope.encoded_flags.is_empty() && scope.actual_encoded_flags == scope.encoded_flags,
        "probe9_original_flags_changed"
    );
    let flags = scope.encoded_flags.split('\x1f').collect::<Vec<_>>();
    ensure!(
        flags.len() == 2
            && flags[0] == "-C"
            && flags[1].starts_with("linker=/")
            && !flags[1].contains('\\'),
        "probe9_selected_flags_domain"
    );
    let positions = arguments
        .windows(flags.len())
        .enumerate()
        .filter_map(|(index, values)| {
            values
                .iter()
                .map(String::as_str)
                .eq(flags.iter().copied())
                .then_some(index)
        })
        .collect::<Vec<_>>();
    ensure!(positions.len() == 1, "probe9_flags_missing_or_duplicate");
    output_namespace(package, scope)?;
    let mut options = BTreeMap::new();
    let mut inputs = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        if index == positions[0] {
            index += flags.len();
            continue;
        }
        let value = &arguments[index];
        if value == "-" || (!value.starts_with('-') && value.ends_with(".rs")) {
            inputs.push((index as u64, value.clone()));
            index += 1;
            continue;
        }
        let (key, argument, width) = if let Some((key, argument)) = value.split_once('=') {
            (key, argument, 1)
        } else {
            (
                value.as_str(),
                arguments
                    .get(index + 1)
                    .context("probe9_option_value")?
                    .as_str(),
                2,
            )
        };
        ensure!(
            [
                "--cfg",
                "--edition",
                "--crate-name",
                "--crate-type",
                "--cap-lints",
                "--emit",
                "--out-dir",
                "--target"
            ]
            .contains(&key),
            "probe9_option_outside_finite_domain:{key}"
        );
        ensure!(
            !argument.is_empty()
                && !argument.starts_with('-')
                && options
                    .insert(key.to_owned(), argument.to_owned())
                    .is_none(),
            "probe9_option_missing_or_duplicate:{key}"
        );
        index += width;
    }
    ensure!(inputs.len() == 1, "probe9_source_missing_or_duplicate");
    let (ordinal, source) = inputs.remove(0);
    ensure!(
        options.get("--crate-type").map(String::as_str) == Some("lib")
            && options.get("--target").map(String::as_str) == Some(scope.platform),
        "probe9_native_image_or_changed_target"
    );
    let name = options.get("--crate-name").context("probe9_crate_name")?;
    let emit = options.get("--emit").context("probe9_emit")?;
    let out_dir = options.get("--out-dir").context("probe9_out_dir")?;
    if source == "-" {
        let suffix = name
            .strip_prefix("autocfg_")
            .context("probe9_stdin_crate_name")?;
        let (hash, serial) = suffix
            .split_once('_')
            .context("probe9_stdin_crate_suffix")?;
        ensure!(
            hash.len() == 16
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                && !serial.is_empty()
                && serial.bytes().all(|byte| byte.is_ascii_digit())
                && serial.parse::<u64>().is_ok(),
            "probe9_stdin_crate_identity"
        );
        ensure!(
            emit == "llvm-ir"
                && Path::new(out_dir) == scope.cargo_out_dir
                && !options.contains_key("--edition")
                && !options.contains_key("--cfg")
                && !options.contains_key("--cap-lints"),
            "probe9_stdin_emission_namespace"
        );
    } else {
        ensure!(
            name == &package.replace('-', "_")
                && emit == "dep-info,metadata"
                && options.get("--cap-lints").map(String::as_str) == Some("allow")
                && matches!(
                    options.get("--edition").map(String::as_str),
                    Some("2018" | "2021")
                )
                && Path::new(out_dir) == scope.cargo_out_dir.join("probe"),
            "probe9_file_emission_namespace"
        );
        if let Some(cfg) = options.get("--cfg") {
            ensure!(
                (package == "proc-macro2" && cfg == "procmacro2_build_probe")
                    || (package == "anyhow" && cfg == "anyhow_build_probe"),
                "probe9_cfg_outside_original_domain"
            );
        }
    }
    Ok(Parsed {
        source,
        ordinal,
        out_dir: out_dir.clone(),
        emit: emit.clone(),
    })
}

/// Checks original source/argument/output content. A positive result does not
/// grant compiler execution: the same-stage live producer and owner are still
/// mandatory before a future adapter may use these observations.
pub fn check_input_domain9(
    source: &CodeNativeCompilationSourceRoot4,
    arguments: &[String],
    scope: &ProbeInputScope9<'_>,
) -> Result<CodeNativeCompilerProbeDomain9> {
    source
        .validate()
        .map_err(|error| anyhow::anyhow!("probe9_source_contract:{error:?}"))?;
    ensure!(
        scope.cwd == Path::new(&source.root) && ordinary_absolute(scope.cwd),
        "probe9_original_cwd_changed"
    );
    no_symlinks(scope.cwd)?;
    ensure!(
        scope.cwd.canonicalize()? == scope.cwd,
        "probe9_source_root_not_canonical"
    );
    crate::native_profile::verify_file(&source.cargo_manifest)?;
    if let Some(archive) = &source.package_archive {
        crate::native_profile::verify_file(archive)?;
    }
    let parsed = parse_arguments(&source.package_name, arguments, scope)?;
    let source_file = if parsed.source == "-" {
        None
    } else {
        let input = Path::new(&parsed.source);
        let absolute = if input.is_absolute() {
            input.to_path_buf()
        } else {
            ensure!(
                input
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                    && !parsed.source.contains('\\'),
                "probe9_source_escape"
            );
            scope.cwd.join(input)
        };
        ensure!(
            ordinary_absolute(&absolute) && absolute.starts_with(scope.cwd),
            "probe9_source_outside_selected_root"
        );
        no_symlinks(&absolute)?;
        ensure!(
            absolute.canonicalize()? == absolute,
            "probe9_source_not_canonical"
        );
        let files = source
            .files
            .iter()
            .filter(|file| Path::new(&file.path) == absolute)
            .collect::<Vec<_>>();
        ensure!(
            files.len() == 1,
            "probe9_source_not_unique_original_inventory"
        );
        crate::native_profile::verify_file(files[0])?;
        Some(files[0].clone())
    };
    let value = CodeNativeCompilerProbeDomain9 {
        schema: "oev2.native-compiler-probe-domain/9".into(),
        package_id: source.package_id.clone(),
        cwd: source.root.clone(),
        arguments: arguments.to_vec(),
        input_argument_ordinal: parsed.ordinal,
        input_kind: if source_file.is_some() {
            "original_file"
        } else {
            "actual_stdin_required"
        }
        .into(),
        source_file,
        cargo_out_dir: scope
            .cargo_out_dir
            .to_str()
            .context("probe9_output_utf8")?
            .into(),
        out_dir: parsed.out_dir,
        emit: parsed.emit,
        target: scope.platform.into(),
        raw_encoded_rustflags: scope.actual_encoded_flags.into(),
    };
    value
        .validate()
        .map_err(|error| anyhow::anyhow!("probe9_domain_contract:{error:?}"))?;
    Ok(value)
}

pub struct StdinSnapshot9 {
    pub observation: CodeNativeCompilerProbeStdin9,
    pub bytes: Vec<u8>,
}

#[cfg(unix)]
pub(crate) fn descriptor(
    fd: std::os::fd::BorrowedFd<'_>,
) -> Result<CodeNativeCompilerProbeDescriptor9> {
    use std::os::fd::AsRawFd;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    ensure!(
        unsafe { libc::fstat(fd.as_raw_fd(), stat.as_mut_ptr()) } == 0,
        "probe9_stdin_fstat"
    );
    let stat = unsafe { stat.assume_init() };
    Ok(CodeNativeCompilerProbeDescriptor9 {
        device: u64::try_from(stat.st_dev)?,
        inode: stat.st_ino as u64,
        mode: stat.st_mode as u64,
        special_device: u64::try_from(stat.st_rdev)?,
    })
}

/// Reads a real FIFO without changing its flags or ownership. Deadline and byte
/// ceiling are borrowed from the original owner, never renewed here. This is an
/// input observation only; original producer/exec/replay evidence remains due.
#[cfg(unix)]
pub fn capture_stdin9(
    fd: std::os::fd::BorrowedFd<'_>,
    deadline: Instant,
    limit: usize,
) -> Result<StdinSnapshot9> {
    use std::os::fd::AsRawFd;
    ensure!(
        (1..=4 * 1024 * 1024).contains(&limit),
        "probe9_stdin_original_limit"
    );
    let original = descriptor(fd)?;
    ensure!(
        original.mode & libc::S_IFMT as u64 == libc::S_IFIFO as u64,
        "probe9_stdin_not_actual_fifo"
    );
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    ensure!(flags >= 0, "probe9_stdin_flags");
    let mut bytes = Vec::new();
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("probe9_stdin_original_deadline")?;
        let milliseconds = remaining.as_millis().min(50) as i32;
        let mut event = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut event, 1, milliseconds) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error.into());
        }
        if ready == 0 {
            continue;
        }
        ensure!(
            event.revents & (libc::POLLERR | libc::POLLNVAL) == 0,
            "probe9_stdin_poll_error"
        );
        ensure!(deadline > Instant::now(), "probe9_stdin_original_deadline");
        let mut buffer = [0u8; 8192];
        let wanted = buffer.len().min(limit.saturating_sub(bytes.len()) + 1);
        let length = unsafe { libc::read(fd.as_raw_fd(), buffer.as_mut_ptr().cast(), wanted) };
        if length < 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(error.into());
        }
        if length == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..length as usize]);
        ensure!(bytes.len() <= limit, "probe9_stdin_original_limit_exceeded");
    }
    ensure!(
        descriptor(fd)? == original
            && unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) } == flags,
        "probe9_stdin_descriptor_or_flags_changed"
    );
    ensure!(deadline > Instant::now(), "probe9_stdin_original_deadline");
    let observation = CodeNativeCompilerProbeStdin9 {
        schema: "oev2.native-compiler-probe-stdin/9".into(),
        original_descriptor: original,
        sha256: sha256(&bytes),
        bytes: bytes.len() as u64,
        original_byte_limit: limit as u64,
        delivery: "captured_only_no_compiler_exec".into(),
    };
    observation
        .validate()
        .map_err(|error| anyhow::anyhow!("probe9_stdin_contract:{error:?}"))?;
    Ok(StdinSnapshot9 { observation, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arguments() -> Vec<String> {
        [
            "--edition=2018",
            "--crate-name=thiserror",
            "--crate-type=lib",
            "--cap-lints=allow",
            "--emit=dep-info,metadata",
            "--out-dir",
            "/selected/target/debug/build/thiserror-0123456789abcdef/out/probe",
            "build/probe.rs",
            "--target",
            "aarch64-apple-darwin",
            "-C",
            "linker=/selected/linker",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }
    fn scope() -> ProbeInputScope9<'static> {
        ProbeInputScope9 {
            target_dir: Path::new("/selected/target"),
            platform: "aarch64-apple-darwin",
            stage: "build-normalizer",
            cargo_out_dir: Path::new("/selected/target/debug/build/thiserror-0123456789abcdef/out"),
            encoded_flags: "-C\x1flinker=/selected/linker",
            actual_encoded_flags: "-C\x1flinker=/selected/linker",
            cwd: Path::new("/selected/source"),
        }
    }
    #[test]
    fn probe9_rejects_native_images_extra_inputs_flags_and_output_changes() {
        assert!(parse_arguments("thiserror", &arguments(), &scope()).is_ok());
        for (index, bad) in [
            (2, "--crate-type=bin"),
            (4, "--emit=link"),
            (6, "/outside/probe"),
            (9, "x86_64-apple-darwin"),
            (11, "linker=/other/linker"),
        ] {
            let mut args = arguments();
            args[index] = bad.into();
            assert!(
                parse_arguments("thiserror", &args, &scope()).is_err(),
                "{bad}"
            );
        }
        for extra in [
            "other.rs",
            "@args",
            "--extern=other=evil.rlib",
            "--print=sysroot",
            "--crate-type=lib",
            "--emit=llvm-ir",
        ] {
            let mut args = arguments();
            args.push(extra.into());
            assert!(
                parse_arguments("thiserror", &args, &scope()).is_err(),
                "{extra}"
            );
        }
        let mut changed = scope();
        changed.actual_encoded_flags = "";
        assert!(parse_arguments("thiserror", &arguments(), &changed).is_err());
        for path in [
            "/selected/target/debug/build/other-0123456789abcdef/out",
            "/selected/target/debug/build/thiserror-ABCDE12345678901/out",
            "/selected/target/../outside/debug/build/thiserror-0123456789abcdef/out",
        ] {
            let mut changed = scope();
            changed.cargo_out_dir = Path::new(path);
            assert!(parse_arguments("thiserror", &arguments(), &changed).is_err());
        }
    }
    #[cfg(unix)]
    fn pipe() -> (std::fs::File, std::fs::File) {
        use std::os::fd::FromRawFd;
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        unsafe {
            (
                std::fs::File::from_raw_fd(fds[0]),
                std::fs::File::from_raw_fd(fds[1]),
            )
        }
    }
    #[test]
    #[cfg(unix)]
    fn probe9_captures_actual_fifo_bytes_and_preserves_descriptor_flags() {
        use std::{io::Write, os::fd::AsFd, time::Duration};
        let (read, mut write) = pipe();
        let original = vec![0, 255, b'\n', b'x'];
        write.write_all(&original).unwrap();
        drop(write);
        let captured =
            capture_stdin9(read.as_fd(), Instant::now() + Duration::from_secs(1), 16).unwrap();
        assert_eq!(captured.bytes, original);
        assert_eq!(captured.observation.sha256, sha256(&original));
        assert_eq!(captured.observation.bytes, 4);
        assert_eq!(
            captured.observation.delivery,
            "captured_only_no_compiler_exec"
        );
    }
    #[test]
    #[cfg(unix)]
    fn probe9_keeps_original_deadline_limit_and_null_refusal() {
        use std::{io::Write, os::fd::AsFd, time::Duration};
        let (read, mut write) = pipe();
        write.write_all(b"12345").unwrap();
        drop(write);
        assert!(capture_stdin9(read.as_fd(), Instant::now() + Duration::from_secs(1), 4).is_err());
        let (read, _writer) = pipe();
        let before = Instant::now();
        assert!(capture_stdin9(read.as_fd(), before + Duration::from_millis(20), 16).is_err());
        assert!(before.elapsed() < Duration::from_secs(1));
        assert!(capture_stdin9(read.as_fd(), Instant::now(), 16).is_err());
        let null = std::fs::File::open("/dev/null").unwrap();
        assert!(capture_stdin9(null.as_fd(), Instant::now() + Duration::from_secs(1), 16).is_err());
    }
}
