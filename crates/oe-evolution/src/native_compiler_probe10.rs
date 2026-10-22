//! Content checks and bounded input observation for a separate compiler-probe
//! domain. These functions do not dispatch rustc, attest a living producer,
//! select a version, own a budget or turn retained JSON into execution authority.
use crate::{CodeNativeCompilationSourceRoot4, CodeNativeCompilerProbeDomain10};
use anyhow::{Context, Result, ensure};
use oe_core::no_symlinks;
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

/// Values borrowed from the original owner's current stage. This content-only
/// view cannot substitute for that owner's source, producer or process checks.
pub struct ProbeInputScope10<'a> {
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

fn output_namespace(package: &str, scope: &ProbeInputScope10<'_>) -> Result<()> {
    ensure!(
        scope.platform == "aarch64-apple-darwin"
            && ["build-normalizer", "build-runner"].contains(&scope.stage)
            && ordinary_absolute(scope.target_dir)
            && ordinary_absolute(scope.cargo_out_dir),
        "probe10_selected_stage_or_path"
    );
    ensure!(
        scope
            .cargo_out_dir
            .file_name()
            .is_some_and(|name| name == "out"),
        "probe10_cargo_out_dir"
    );
    let folder = scope
        .cargo_out_dir
        .parent()
        .context("probe10_output_folder")?;
    let name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .context("probe10_output_encoding")?;
    let fingerprint = name
        .strip_prefix(&format!("{package}-"))
        .context("probe10_output_package")?;
    ensure!(
        fingerprint.len() == 16
            && fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "probe10_output_fingerprint"
    );
    let host = scope.target_dir.join("debug/build");
    let target = scope.target_dir.join(scope.platform).join("debug/build");
    ensure!(
        folder.parent() == Some(host.as_path())
            || (scope.stage == "build-runner" && folder.parent() == Some(target.as_path())),
        "probe10_output_not_same_stage"
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
    scope: &ProbeInputScope10<'_>,
) -> Result<Parsed> {
    ensure!(
        !arguments.is_empty()
            && arguments.len() <= 128
            && arguments.iter().map(String::len).sum::<usize>() <= 65536
            && arguments.iter().all(|value| !value.contains('\0')),
        "probe10_argument_limit"
    );
    ensure!(
        !scope.encoded_flags.is_empty()
            && (scope.actual_encoded_flags == scope.encoded_flags
                || scope.actual_encoded_flags.is_empty()),
        "probe10_original_flags_changed"
    );
    let flags = scope.encoded_flags.split('\x1f').collect::<Vec<_>>();
    ensure!(
        flags.len() == 2
            && flags[0] == "-C"
            && flags[1].starts_with("linker=/")
            && !flags[1].contains('\\'),
        "probe10_selected_flags_domain"
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
    ensure!(
        if scope.actual_encoded_flags.is_empty() {
            positions.is_empty()
        } else {
            positions.len() == 1
        },
        "probe10_flags_missing_or_duplicate"
    );
    output_namespace(package, scope)?;
    let mut options = BTreeMap::new();
    let mut inputs = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        if positions.first().is_some_and(|position| index == *position) {
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
                    .context("probe10_option_value")?
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
            "probe10_option_outside_finite_domain:{key}"
        );
        ensure!(
            !argument.is_empty()
                && !argument.starts_with('-')
                && options
                    .insert(key.to_owned(), argument.to_owned())
                    .is_none(),
            "probe10_option_missing_or_duplicate:{key}"
        );
        index += width;
    }
    ensure!(inputs.len() == 1, "probe10_source_missing_or_duplicate");
    let (ordinal, source) = inputs.remove(0);
    ensure!(
        options.get("--crate-type").map(String::as_str) == Some("lib")
            && options.get("--target").map(String::as_str) == Some(scope.platform),
        "probe10_native_image_or_changed_target"
    );
    let name = options.get("--crate-name").context("probe10_crate_name")?;
    let emit = options.get("--emit").context("probe10_emit")?;
    let out_dir = options.get("--out-dir").context("probe10_out_dir")?;
    if source == "-" {
        let suffix = name
            .strip_prefix("autocfg_")
            .context("probe10_stdin_crate_name")?;
        let (hash, serial) = suffix
            .split_once('_')
            .context("probe10_stdin_crate_suffix")?;
        ensure!(
            hash.len() == 16
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                && !serial.is_empty()
                && serial.bytes().all(|byte| byte.is_ascii_digit())
                && serial.parse::<u64>().is_ok(),
            "probe10_stdin_crate_identity"
        );
        ensure!(
            emit == "llvm-ir"
                && Path::new(out_dir) == scope.cargo_out_dir
                && !options.contains_key("--edition")
                && !options.contains_key("--cfg")
                && !options.contains_key("--cap-lints"),
            "probe10_stdin_emission_namespace"
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
            "probe10_file_emission_namespace"
        );
        if let Some(cfg) = options.get("--cfg") {
            ensure!(
                (package == "proc-macro2" && cfg == "procmacro2_build_probe")
                    || (package == "anyhow" && cfg == "anyhow_build_probe"),
                "probe10_cfg_outside_original_domain"
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
pub fn check_input_domain10(
    source: &CodeNativeCompilationSourceRoot4,
    arguments: &[String],
    scope: &ProbeInputScope10<'_>,
) -> Result<CodeNativeCompilerProbeDomain10> {
    source
        .validate()
        .map_err(|error| anyhow::anyhow!("probe10_source_contract:{error:?}"))?;
    ensure!(
        scope.cwd == Path::new(&source.root) && ordinary_absolute(scope.cwd),
        "probe10_original_cwd_changed"
    );
    no_symlinks(scope.cwd)?;
    ensure!(
        scope.cwd.canonicalize()? == scope.cwd,
        "probe10_source_root_not_canonical"
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
                "probe10_source_escape"
            );
            scope.cwd.join(input)
        };
        ensure!(
            ordinary_absolute(&absolute) && absolute.starts_with(scope.cwd),
            "probe10_source_outside_selected_root"
        );
        no_symlinks(&absolute)?;
        ensure!(
            absolute.canonicalize()? == absolute,
            "probe10_source_not_canonical"
        );
        let files = source
            .files
            .iter()
            .filter(|file| Path::new(&file.path) == absolute)
            .collect::<Vec<_>>();
        ensure!(
            files.len() == 1,
            "probe10_source_not_unique_original_inventory"
        );
        crate::native_profile::verify_file(files[0])?;
        Some(files[0].clone())
    };
    let value = CodeNativeCompilerProbeDomain10 {
        schema: "oev2.native-compiler-probe-domain/10".into(),
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
            .context("probe10_output_utf8")?
            .into(),
        out_dir: parsed.out_dir,
        emit: parsed.emit,
        target: scope.platform.into(),
        raw_encoded_rustflags: scope.actual_encoded_flags.into(),
        selected_target_flags: scope.encoded_flags.into(),
        configuration_kind: if scope.actual_encoded_flags.is_empty() {
            "actual_host_empty"
        } else {
            "selected_target"
        }
        .into(),
    };
    value
        .validate()
        .map_err(|error| anyhow::anyhow!("probe10_domain_contract:{error:?}"))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope(actual: &str) -> ProbeInputScope10<'_> {
        ProbeInputScope10 {
            target_dir: Path::new("/selected/target"),
            platform: "aarch64-apple-darwin",
            stage: "build-runner",
            cargo_out_dir: Path::new(
                "/selected/target/debug/build/proc-macro2-0123456789abcdef/out",
            ),
            encoded_flags: "-C\x1flinker=/selected/linker",
            actual_encoded_flags: actual,
            cwd: Path::new("/selected/source"),
        }
    }
    fn host_arguments() -> Vec<String> {
        [
            "--cfg=procmacro2_build_probe",
            "--edition=2021",
            "--crate-name=proc_macro2",
            "--crate-type=lib",
            "--cap-lints=allow",
            "--emit=dep-info,metadata",
            "--out-dir",
            "/selected/target/debug/build/proc-macro2-0123456789abcdef/out/probe",
            "src/probe/proc_macro_span.rs",
            "--target",
            "aarch64-apple-darwin",
        ]
        .map(str::to_owned)
        .into()
    }
    #[test]
    fn host10_actual_flags_keep_original_finite_arguments_and_target_controls() {
        let host = host_arguments();
        parse_arguments("proc-macro2", &host, &scope("")).unwrap();
        assert!(
            parse_arguments(
                "proc-macro2",
                &host,
                &scope("-C\x1flinker=/selected/linker")
            )
            .is_err()
        );
        let mut target = host.clone();
        target.extend(["-C".into(), "linker=/selected/linker".into()]);
        parse_arguments(
            "proc-macro2",
            &target,
            &scope("-C\x1flinker=/selected/linker"),
        )
        .unwrap();
        assert!(parse_arguments("proc-macro2", &target, &scope("")).is_err());
        for flags in ["-C\x1flinker=/other/linker", "-C\x1fdebuginfo=2"] {
            assert!(parse_arguments("proc-macro2", &host, &scope(flags)).is_err());
        }
        for extra in [
            "other.rs",
            "@args",
            "-C",
            "linker=/selected/linker",
            "--emit=link",
            "--extern=evil=other.rlib",
            "--print=sysroot",
        ] {
            let mut args = host.clone();
            args.push(extra.into());
            assert!(
                parse_arguments("proc-macro2", &args, &scope("")).is_err(),
                "{extra}"
            );
        }
        for (ordinal, changed) in [
            (3, "--crate-type=bin"),
            (5, "--emit=link"),
            (7, "/foreign/probe"),
            (10, "x86_64-apple-darwin"),
        ] {
            let mut args = host.clone();
            args[ordinal] = changed.into();
            assert!(parse_arguments("proc-macro2", &args, &scope("")).is_err());
        }
        let stdin = [
            "--crate-name=autocfg_0123456789abcdef_0",
            "--crate-type=lib",
            "--emit=llvm-ir",
            "--out-dir",
            "/selected/target/debug/build/proc-macro2-0123456789abcdef/out",
            "-",
            "--target",
            "aarch64-apple-darwin",
        ]
        .map(str::to_owned)
        .to_vec();
        parse_arguments("proc-macro2", &stdin, &scope("")).unwrap();
    }
}
