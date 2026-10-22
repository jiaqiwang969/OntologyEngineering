//! Explicit Cargo→wrapper(exec)→Rustc→native-adapter association.
//! Original /3 refuses every DYLD variable. This /6 records the original Cargo
//! value, live process identities and selected child removals without granting
//! a registry lease, OS-cache attestation or engineering adoption.
#[cfg(test)]
use oe_core::sha256 as private_fixture_sha256;
#[cfg(test)]
#[path = "../../../build_support/private_test_fixtures.rs"]
mod private_test_fixtures;

use crate::native_runtime::{epoch_ms, read, reference};
use crate::*;
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, write_new};
use std::{
    collections::BTreeSet,
    ffi::{CStr, OsString},
    path::{Path, PathBuf},
};

pub fn inspect_policy(
    root: &SkillRoot,
    r: &ArtifactReference,
) -> Result<CodeNativeCargoLoaderPolicy6> {
    let p: CodeNativeCargoLoaderPolicy6 = serde_json::from_value(read(root, r)?)?;
    p.validate()
        .map_err(|e| anyhow::anyhow!("loader6_policy_contract:{e:?}"))?;
    ensure!(
        p.schema == "oev2.native-cargo-loader-policy/6"
            && [
                "record_cargo_fallback_then_remove_from_native_child/6",
                crate::native_sdkroot6::STRATEGY
            ]
            .contains(&p.child_environment_strategy.as_str())
            && p.fallback_tail == ["/usr/local/lib", "/usr/lib"],
        "loader6_policy_domain"
    );
    crate::native_profile::verify_file(&p.rustc_wrapper)?;
    crate::native_profile::verify_file(&p.compiler)?;
    crate::native_profile::verify_file(&p.cargo)?;
    ensure!(
        p.rustc_wrapper.mode & 0o111 != 0
            && p.compiler.mode & 0o111 != 0
            && p.cargo.mode & 0o111 != 0,
        "loader6_tools_not_executable"
    );
    ensure!(
        read(root, r)? == serde_json::to_value(&p)?,
        "loader6_policy_changed"
    );
    crate::native_sdkroot6::inspect_policy(&p)?;
    if [
        TARGET_INFORMATION_POLICY,
        crate::native_buildscript6::POLICY,
    ]
    .contains(&p.metadata_probe.as_str())
    {
        target_information_policy(&p)?;
    }
    Ok(p)
}
const TARGET_INFORMATION_POLICY: &str = "cargo_1_95_target_information_f2d3ce0bd/2";
const TARGET_INFORMATION_CARGO_SHA256: &str =
    "c512bff73c86143b557463f021d0c3d5b0490d97d65040ba59ea2b3427784758";
fn target_information_policy(policy: &CodeNativeCargoLoaderPolicy6) -> Result<()> {
    ensure!(
        [
            TARGET_INFORMATION_POLICY,
            crate::native_buildscript6::POLICY
        ]
        .contains(&policy.metadata_probe.as_str())
            && policy.cargo.sha256 == TARGET_INFORMATION_CARGO_SHA256
            && policy.cargo.bytes == 31_232_696
            && policy.cargo.mode == 0o755,
        "loader6_target_information_cargo_source_domain_uncovered"
    );
    Ok(())
}
// Cargo reports commit f2d3ce0bd7f24a49f8f72d9000448f8838c4e850.
// Its official TargetInfo::new/discover_crate_type, CompileKind::add_target_arg
// and extra_args define these complete query families. The selected stable
// two-stage builder has no nightly build-std, unknown crate type or argfile
// metadata permission. Matching physical Cargo bytes is not source attestation.
fn target_information_vectors(
    recipe: &CodeNativeBuildRecipe6,
    parent_stage: &str,
) -> Result<Vec<Vec<String>>> {
    let flags = recipe
        .raw_encoded_rustflags
        .split('\u{1f}')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ensure!(
        recipe.target == "aarch64-apple-darwin"
            && flags.len() == 2
            && flags[0] == "-C"
            && flags[1].starts_with("linker="),
        "loader6_target_information_recipe"
    );
    let configurations = match parent_stage {
        "build-normalizer" => vec![(flags, false)],
        "build-runner" => vec![(Vec::new(), false), (flags, true)],
        _ => anyhow::bail!("loader6_target_information_stage"),
    };
    let mut out = Vec::new();
    for (flags, selected_target) in configurations {
        let mut base = vec![
            "-".into(),
            "--crate-name".into(),
            "___".into(),
            "--print=file-names".into(),
        ];
        base.extend(flags);
        if selected_target {
            base.extend(["--target".into(), recipe.target.clone()]);
        }
        let kinds = ["bin", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"];
        let mut aggregate = base.clone();
        for kind in kinds {
            aggregate.extend(["--crate-type".into(), kind.into()]);
        }
        aggregate.extend(
            [
                "--print=sysroot",
                "--print=split-debuginfo",
                "--print=crate-name",
                "--print=cfg",
                "-Wwarnings",
            ]
            .map(str::to_owned),
        );
        out.push(aggregate);
        for kind in kinds.into_iter().chain(std::iter::once("lib")) {
            let mut one = base.clone();
            one.extend(["--crate-type".into(), kind.into()]);
            out.push(one);
        }
    }
    Ok(out)
}
fn compiler_scope(
    arguments: &[String],
    policy: &CodeNativeCargoLoaderPolicy6,
    recipe: &CodeNativeBuildRecipe6,
    parent_stage: &str,
) -> Result<&'static str> {
    if policy.metadata_probe == "cargo_rustc_version_exact_vv/1" {
        return Ok(if is_version_probe(arguments, policy)? {
            "cargo_version_probe"
        } else {
            "cargo_compiler_command"
        });
    }
    target_information_policy(policy)?;
    if arguments == ["-vV"] {
        return Ok("cargo_version_probe");
    }
    Ok(
        if target_information_vectors(recipe, parent_stage)?
            .iter()
            .any(|expected| arguments == expected)
        {
            "cargo_target_information_probe"
        } else {
            "cargo_compiler_command"
        },
    )
}
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct DescriptorFact6 {
    device: u64,
    inode: u64,
    mode: u64,
    special_device: u64,
}
#[cfg(unix)]
fn descriptor_fact6(fd: i32) -> Result<DescriptorFact6> {
    let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
    ensure!(
        unsafe { libc::fstat(fd, st.as_mut_ptr()) } == 0,
        "loader6_stdin_fstat_unavailable"
    );
    let st = unsafe { st.assume_init() };
    Ok(DescriptorFact6 {
        device: u64::try_from(st.st_dev)?,
        inode: st.st_ino as u64,
        mode: st.st_mode as u64,
        special_device: u64::try_from(st.st_rdev)?,
    })
}
#[cfg(unix)]
pub(crate) fn inspect_null_descriptor6(fd: i32) -> Result<DescriptorFact6> {
    use std::os::fd::AsRawFd;
    let expected = std::fs::File::open("/dev/null")?;
    let expected = descriptor_fact6(expected.as_raw_fd())?;
    let before = descriptor_fact6(fd)?;
    let after = descriptor_fact6(fd)?;
    ensure!(
        expected.mode & (libc::S_IFMT as u64) == libc::S_IFCHR as u64
            && before == expected
            && after == before,
        "loader6_metadata_stdin_not_current_dev_null"
    );
    Ok(before)
}
fn is_version_probe(arguments: &[String], policy: &CodeNativeCargoLoaderPolicy6) -> Result<bool> {
    ensure!(
        policy.metadata_probe == "cargo_rustc_version_exact_vv/1",
        "loader6_metadata_probe_policy"
    );
    Ok(arguments == ["-vV"])
}
#[cfg(target_os = "macos")]
#[link(name = "proc")]
unsafe extern "C" {
    fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, size: u32) -> i32;
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn _dyld_image_count() -> u32;
    fn _dyld_get_image_name(index: u32) -> *const std::ffi::c_char;
    fn _dyld_get_image_header(index: u32) -> *const std::ffi::c_void;
}
#[cfg(target_os = "macos")]
pub(crate) fn process_executable(pid: u64) -> Result<CodeNativeFileBinding> {
    let mut buf = vec![0u8; 4096];
    let n = unsafe {
        proc_pidpath(
            i32::try_from(pid)?,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
        )
    };
    ensure!(n > 0, "loader6_actual_process_executable_unavailable");
    let text = CStr::from_bytes_until_nul(&buf)?.to_str()?;
    crate::native_archive::bind_file(Path::new(text))
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn process_executable(_pid: u64) -> Result<CodeNativeFileBinding> {
    anyhow::bail!("loader6_platform_unsupported")
}
pub(crate) fn parent_pid() -> Result<u64> {
    #[cfg(target_os = "macos")]
    {
        let pid = unsafe { libc::getppid() };
        ensure!(pid > 0, "loader6_parent_missing");
        Ok(pid as u64)
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("loader6_platform_unsupported")
    }
}
/// Actual current-process image observation only; absent files do not establish
/// that a path was supplied by the sealed OS shared cache or used by Rustc.
pub fn current_images() -> Result<Vec<CodeNativeLoadedImage6>> {
    #[cfg(target_os = "macos")]
    {
        let count = unsafe { _dyld_image_count() };
        ensure!(count > 0 && count <= 4096, "loader6_image_count");
        let mut out = Vec::new();
        for i in 0..count {
            let name = unsafe { _dyld_get_image_name(i) };
            ensure!(!name.is_null(), "loader6_image_name_missing");
            let path = unsafe { CStr::from_ptr(name) }.to_str()?.to_owned();
            let header = unsafe { _dyld_get_image_header(i) } as usize;
            ensure!(
                header > 0 && Path::new(&path).is_absolute(),
                "loader6_image_identity"
            );
            let physical_file = crate::native_archive::bind_file(Path::new(&path)).ok();
            out.push(CodeNativeLoadedImage6 {
                path,
                header_address: header as u64,
                storage_observation: if physical_file.is_some() {
                    "regular_file"
                } else {
                    "nonphysical_or_unreadable_unknown"
                }
                .into(),
                physical_file,
            });
        }
        ensure!(
            unsafe { _dyld_image_count() } == count,
            "loader6_image_count_changed"
        );
        return Ok(out);
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("loader6_platform_unsupported")
    }
}
/// Fresh full original stage verification, borrowed within one wrapper call.
/// No Clone/Serialize/Deserialize implementation and no retained authority.
pub(crate) struct CheckedCompilerStage6 {
    source_root: PathBuf,
    context_path: PathBuf,
    context_sha256: String,
    context: CodeNativeStageContext6,
    side: CodeNativeSideContext6,
    profile: CodeNativeRuntimeProfile6,
    recipe: CodeNativeBuildRecipe6,
}
fn checked_call_binding6(
    root: &Path,
    selected_root: &Path,
    context_path: &Path,
    selected_context: &Path,
    context_sha256: &str,
    selected_sha256: &str,
    actual_sha256: &str,
    deadline_epoch_ms: u64,
    now_epoch_ms: u64,
) -> Result<()> {
    ensure!(
        root == selected_root
            && context_path == selected_context
            && context_sha256 == selected_sha256
            && context_sha256 == actual_sha256,
        "loader6_checked_call_source_or_context_changed"
    );
    ensure!(
        deadline_epoch_ms > now_epoch_ms,
        "loader6_checked_call_deadline_expired"
    );
    Ok(())
}
impl CheckedCompilerStage6 {
    pub(crate) fn recheck(&self, root: &SkillRoot) -> Result<()> {
        let selected_context = PathBuf::from(std::env::var("OE_NATIVE_STAGE_CONTEXT")?);
        let selected_sha256 = std::env::var("OE_NATIVE_STAGE_CONTEXT_SHA256")?;
        let actual = read_regular_bytes(&self.context_path, 16 * 1024 * 1024)?;
        checked_call_binding6(
            root.path(),
            &self.source_root,
            &self.context_path,
            &selected_context,
            &self.context_sha256,
            &selected_sha256,
            &sha256(&actual),
            self.context.deadline_epoch_ms,
            epoch_ms()?,
        )?;
        ensure!(
            self.context.source_root
                == root.path().to_str().context("loader6_checked_root_utf8")?
                && self.context.recipe == self.profile.recipe,
            "loader6_checked_call_profile_or_root_changed"
        );
        // Recheck exact retained source/recipe references without another full
        // SDK walk. The original full checker already ran before/final here.
        read(root, &self.context.side_context)?;
        read(root, &self.side.source_profile)?;
        read(root, &self.profile.recipe)?;
        Ok(())
    }
    pub(crate) fn facts(
        &self,
    ) -> (
        &CodeNativeStageContext6,
        &CodeNativeSideContext6,
        &CodeNativeBuildRecipe6,
        &CodeNativeRuntimeProfile6,
    ) {
        (&self.context, &self.side, &self.recipe, &self.profile)
    }
    fn into_parts(
        self,
    ) -> (
        CodeNativeStageContext6,
        CodeNativeSideContext6,
        CodeNativeBuildRecipe6,
    ) {
        (self.context, self.side, self.recipe)
    }
}
pub(crate) fn stage(
    root: &SkillRoot,
    context_path: &Path,
) -> Result<(
    CodeNativeStageContext6,
    CodeNativeSideContext6,
    CodeNativeBuildRecipe6,
)> {
    Ok(checked_stage6(root, context_path)?.into_parts())
}
pub(crate) fn checked_stage6(
    root: &SkillRoot,
    context_path: &Path,
) -> Result<CheckedCompilerStage6> {
    let raw = read_regular_bytes(context_path, 16 * 1024 * 1024)?;
    ensure!(
        sha256(&raw) == std::env::var("OE_NATIVE_STAGE_CONTEXT_SHA256")?,
        "loader6_stage_changed"
    );
    let c: CodeNativeStageContext6 = serde_json::from_value(strict_json(&raw)?)?;
    c.validate()
        .map_err(|e| anyhow::anyhow!("loader6_stage_contract:{e:?}"))?;
    let (s, p, r) =
        crate::native_profile6::load_compiler_entry(root, &c.side_context)?.into_parts();
    ensure!(
        c.schema == "oev2.native-build-stage/6"
            && c.deadline_epoch_ms > epoch_ms()?
            && c.source_manifest == public_manifest(root.path())?
            && c.recipe == p.recipe
            && context_path
                == Path::new(&s.stage_root)
                    .join(&c.parent_stage)
                    .join("stage-context.json"),
        "loader6_stage_not_current"
    );
    let checked = CheckedCompilerStage6 {
        source_root: root.path().to_owned(),
        context_path: context_path.to_owned(),
        context_sha256: sha256(&raw),
        context: c,
        side: s,
        profile: p,
        recipe: r,
    };
    checked.recheck(root)?;
    Ok(checked)
}
pub(crate) fn selected_environment(
    side: &CodeNativeSideContext6,
    p: &CodeNativeCargoLoaderPolicy6,
    version_probe: bool,
    sdkroot: Option<&str>,
) -> Result<Vec<CodeNativeEnvironmentEntry>> {
    let mut env = Vec::new();
    for (key, value) in std::env::vars_os() {
        let key = key.to_str().context("loader6_environment_encoding")?;
        if key == "SDKROOT" {
            let value = value.to_str().context("loader6_sdkroot_value_encoding")?;
            ensure!(
                sdkroot == Some(value),
                "loader6_unselected_native_environment:SDKROOT"
            );
            env.push(CodeNativeEnvironmentEntry {
                name: key.into(),
                value: value.into(),
            });
            continue;
        }
        if key.starts_with("DYLD_") {
            ensure!(
                ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_PRINT_LIBRARIES"].contains(&key),
                "loader6_unselected_loader_variable:{key}"
            );
            let value = value.to_str().context("loader6_loader_value_encoding")?;
            if key == "DYLD_PRINT_LIBRARIES" {
                ensure!(value == "1", "loader6_diagnostic_value_changed");
            }
            if key == "DYLD_FALLBACK_LIBRARY_PATH" {
                validate_fallback(value, side, p)?;
            }
            env.push(CodeNativeEnvironmentEntry {
                name: key.into(),
                value: value.into(),
            });
        }
        ensure!(
            ![
                "LIBRARY_PATH",
                "CPATH",
                "C_INCLUDE_PATH",
                "CPLUS_INCLUDE_PATH",
                "OBJC_INCLUDE_PATH",
                "SDKROOT",
                "DEVELOPER_DIR",
                "MACOSX_DEPLOYMENT_TARGET",
                "COMPILER_PATH",
                "CCC_OVERRIDE_OPTIONS",
                "RC_DEBUG_OPTIONS"
            ]
            .contains(&key),
            "loader6_unselected_native_environment:{key}"
        );
    }
    ensure!(
        version_probe || env.iter().any(|e| e.name == "DYLD_FALLBACK_LIBRARY_PATH"),
        "loader6_cargo_fallback_missing"
    );
    env.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(env)
}
pub(crate) fn validate_fallback(
    value: &str,
    side: &CodeNativeSideContext6,
    p: &CodeNativeCargoLoaderPolicy6,
) -> Result<()> {
    let dirs = value.split(':').collect::<Vec<_>>();
    ensure!(
        dirs.len() >= 3 && dirs.len() <= 256 && dirs[dirs.len() - 2..] == p.fallback_tail,
        "loader6_cargo_fallback_tail_changed"
    );
    let target = Path::new(&side.build.target_dir);
    let toolchain = Path::new(&p.compiler.path)
        .parent()
        .and_then(Path::parent)
        .context("loader6_toolchain")?;
    let rustlib = toolchain.join("lib/rustlib");
    let mut seen = BTreeSet::new();
    for d in &dirs[..dirs.len() - 2] {
        let path = Path::new(d);
        ensure!(
            !d.is_empty()
                && path.is_absolute()
                && path.components().all(|c| matches!(
                    c,
                    std::path::Component::RootDir | std::path::Component::Normal(_)
                ))
                && seen.insert(*d)
                && (path.starts_with(target) || path.starts_with(&rustlib)),
            "loader6_cargo_fallback_directory_unselected"
        );
        oe_core::no_symlinks(path)?;
        if path.try_exists()? {
            ensure!(
                path.is_dir() && path.canonicalize()? == path,
                "loader6_cargo_fallback_directory_not_canonical"
            );
        }
    }
    Ok(())
}
fn unknowns() -> Vec<String> {
    vec![
    "Current image path/address and physical bytes are observations, not complete startup/dyld-cache provenance or actual symbol-use attestation".into(),
    "The compiler exec intent precedes exec; a living rustc parent plus original raw PID loader output establishes only the recorded finite process association".into(),
    "Synchronous reads and process check/exec races are not certified hard wall time; original owner deadline remains authoritative".into(),
    "Whole original13494/module19/protocol19/OWNER22, release and engineering adoption remain unknown".into()]
}
/// Cargo passes the exact pinned rustc as argv[1]. exec retains this actual PID;
/// the linker later checks its live parent executable and retained intent.
#[cfg(unix)]
pub fn compiler_exec_entry() -> Result<()> {
    let argv = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        argv.len() >= 2
            && argv.len() <= 10000
            && argv.iter().map(String::len).sum::<usize>() <= 1024 * 1024,
        "loader6_compiler_arguments"
    );
    let context_path = PathBuf::from(std::env::var("OE_NATIVE_STAGE_CONTEXT")?);
    let v = strict_json(&read_regular_bytes(&context_path, 16 * 1024 * 1024)?)?;
    let root = SkillRoot::new(v["source_root"].as_str().context("loader6_root")?)?;
    let checked = checked_stage6(&root, &context_path)?;
    compiler_exec_entry_from_checked_stage6(checked, argv)
}
/// Consume only the fresh nonserialized result from this same invocation.
/// All original argv/flags/parent/source/environment/descriptor/exec guards below
/// remain byte exact; the already completed full SDK stage walk is shared.
#[cfg(unix)]
pub(crate) fn compiler_exec_entry_from_checked_stage6(
    checked: CheckedCompilerStage6,
    argv: Vec<String>,
) -> Result<()> {
    use std::os::unix::process::CommandExt;
    ensure!(
        argv.len() >= 2
            && argv.len() <= 10000
            && argv.iter().map(String::len).sum::<usize>() <= 1024 * 1024,
        "loader6_compiler_arguments"
    );
    let root = SkillRoot::new(&checked.source_root)?;
    checked.recheck(&root)?;
    let context_path = checked.context_path.clone();
    let (c, s, r) = checked.into_parts();
    let p = inspect_policy(&root, &r.cargo_loader_policy)?;
    let actual_encoded_flags = std::env::var("CARGO_ENCODED_RUSTFLAGS")?;
    let sdk8 = crate::native_link8::is_adapter8(&r)?;
    let empty_host_metadata8 = sdk8
        && actual_encoded_flags.is_empty()
        && p.metadata_probe == crate::native_buildscript6::POLICY
        && crate::native_buildscript6::metadata(&argv[1..]).is_ok();
    // Only exact read-only SDK8 metadata may proceed to the independently
    // verified host producer. Every compilation and original6/7 keeps this guard.
    if !empty_host_metadata8 {
        ensure!(
            actual_encoded_flags == r.raw_encoded_rustflags,
            "loader6_actual_encoded_flags_changed"
        );
    }
    ensure!(
        argv[0] == p.compiler.path && argv[0] == s.build.rustc_path,
        "loader6_cargo_rustc_changed"
    );
    let exe = std::env::current_exe()?.canonicalize()?;
    ensure!(
        exe == Path::new(&p.rustc_wrapper.path),
        "loader6_wrapper_changed"
    );
    let own = &crate::driver_root().context("loader6_own_root")?;
    ensure!(
        digest(&public_manifest(own)?)? == compiled()?["manifest_sha256"],
        "loader6_wrapper_source_mismatch"
    );
    let cwd = std::env::current_dir()?;
    let selected_source = if cwd == root.path() {
        None
    } else {
        Some(crate::native_sources6::caller_source(
            &root,
            &s,
            &c.parent_stage,
            &serde_json::to_value(&c)?,
            &cwd,
        )?)
    };
    let parent_pid = parent_pid()?;
    let parent_executable = process_executable(parent_pid)?;
    // A raw observation is retained before the environment predicate can fail.
    // It is never consumed as permission, an exec receipt or owner observation.
    use std::os::unix::ffi::OsStrExt;
    let original_loader = ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_PRINT_LIBRARIES"]
        .into_iter()
        .map(|name| json!({"name":name,"value_bytes":std::env::var_os(name).map(|v|v.as_bytes().to_vec())}))
        .collect::<Vec<_>>();
    let raw_checkpoint = Path::new(&s.stage_root)
        .join(&c.parent_stage)
        .join("loader-events")
        .join(format!("pre-environment-{}.json", std::process::id()));
    let mut raw_observation = json!({
        "schema":"oev2.native-compiler-pre-admission-raw/6",
        "pid":std::process::id(),"parent_pid":parent_pid,"parent_executable":parent_executable,
        "compiler":p.compiler,"wrapper":p.rustc_wrapper,"arguments":argv[1..],"cwd":cwd,
        "stage_context":reference(&root,&context_path)?,"selected_loader_environment_raw":original_loader,
        "stdin_descriptor_raw":descriptor_fact6(0).ok(),
        "boundary":"source/stage and actual parent physical executable observed; parent/environment admission and exec not reached",
        "qualification":"not_established"
    });
    if sdk8 {
        raw_observation["actual_encoded_rustflags8"] = json!(actual_encoded_flags);
    }
    write_new(&root.private_path(&raw_checkpoint)?, &raw_observation)?;
    let (command_scope, build_script_binding) =
        if parent_executable == p.cargo && parent_executable.path == s.build.cargo_path {
            (compiler_scope(&argv[1..], &p, &r, &c.parent_stage)?, None)
        } else {
            ensure!(
                p.metadata_probe == crate::native_buildscript6::POLICY,
                "loader6_parent_not_selected_cargo"
            );
            let selected = selected_source
                .as_ref()
                .context("loader6_build_script_source_not_selected")?;
            let inspect = if crate::native_link8::is_adapter8(&r)? {
                crate::native_buildscript6::inspect_current_sdk8
            } else {
                crate::native_buildscript6::inspect_current
            };
            let binding = inspect(
                &root,
                &s,
                &r,
                &context_path,
                &c,
                &p,
                selected,
                parent_pid,
                &parent_executable,
                &argv[1..],
            )?;
            ("bound_build_script_metadata_probe", Some(binding))
        };
    if command_scope == "cargo_target_information_probe" {
        inspect_null_descriptor6(0)?;
    }
    let metadata_probe = command_scope != "cargo_compiler_command";
    if empty_host_metadata8 {
        ensure!(
            command_scope == "bound_build_script_metadata_probe",
            "sdk8_host_metadata_parent_scope"
        );
        let selected = selected_source
            .as_ref()
            .context("sdk8_host_metadata_source")?;
        let binding = build_script_binding
            .as_ref()
            .context("sdk8_host_metadata_binding")?;
        let observed =
            crate::native_host_metadata8::observe_live(&root, &s, &r, &c, selected, binding)?;
        let path = Path::new(&s.stage_root)
            .join(&c.parent_stage)
            .join("loader-events")
            .join(format!(
                "host-metadata-environment8-{}.json",
                std::process::id()
            ));
        write_new(&root.private_path(&path)?, &serde_json::to_value(observed)?)?;
    }
    let environment = selected_environment(&s, &p, metadata_probe, None)?;
    let intent = CodeNativeCompilerExecIntent6 {
        schema: "oev2.native-compiler-exec-intent/6".into(),
        pid: std::process::id() as u64,
        parent_pid,
        parent_executable: parent_executable.clone(),
        compiler: p.compiler.clone(),
        wrapper: p.rustc_wrapper.clone(),
        arguments: argv[1..].to_vec(),
        command_scope: command_scope.into(),
        cwd: cwd.to_str().context("loader6_cwd")?.into(),
        stage_context: reference(&root, &context_path)?,
        environment,
        loaded_images: current_images()?,
        execution: "before_exec".into(),
        uncovered: unknowns(),
        build_script_binding,
    };
    intent
        .validate()
        .map_err(|e| anyhow::anyhow!("loader6_intent_contract:{e:?}"))?;
    let file = Path::new(&s.stage_root)
        .join(&c.parent_stage)
        .join("loader-events")
        .join(format!("compiler-{}.json", intent.pid));
    write_new(&root.private_path(&file)?, &serde_json::to_value(intent)?)?;
    crate::native_profile::verify_file(&p.compiler)?;
    crate::native_profile::verify_file(&p.rustc_wrapper)?;
    crate::native_profile::verify_file(&parent_executable)?;
    ensure!(
        process_executable(parent_pid)? == parent_executable,
        "loader6_parent_changed_before_exec"
    );
    ensure!(
        c.deadline_epoch_ms > epoch_ms()?,
        "loader6_deadline_before_exec"
    );
    let mut command = std::process::Command::new(&p.compiler.path);
    command.args(&argv[1..]);
    // Only a compiler command declaring a native image requests raw startup
    // diagnostics. All original Cargo argv/flags stay exact; this bounds the
    // same original per-stream capture and avoids spurious every-rlib reports.
    let native_image = argv.windows(2).any(|w| {
        w[0] == "--crate-type"
            && w[1]
                .split(',')
                .any(|x| ["bin", "proc-macro", "cdylib", "dylib"].contains(&x))
    });
    if native_image && !metadata_probe {
        command.env("DYLD_PRINT_LIBRARIES", "1");
    }
    let error = command.exec();
    Err(error.into())
}
// Preserve the original native-link caller partition. Cargo's own compiler
// probes run in the exact selected workspace root; a different directory must
// still be an exact source root in the selected compilation inventory.
fn validate_compiler_caller(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    parent_stage: &str,
    context: &Value,
    cwd: &Path,
) -> Result<()> {
    if cwd != root.path() {
        crate::native_sources6::caller_root(root, side, parent_stage, context, cwd)?;
    }
    Ok(())
}
pub(crate) struct ActualCompilerParent {
    pub intent: ArtifactReference,
    pub pid: u64,
    pub executable: CodeNativeFileBinding,
    pub environment: Vec<CodeNativeEnvironmentEntry>,
    pub sdk_root: Option<CodeNativeCompilerSdkRootBinding6>,
}
/// Recheck the still-borrowed physical parent. Retained JSON cannot construct it.
pub(crate) fn recheck_live_parent(parent: &ActualCompilerParent) -> Result<()> {
    ensure!(
        parent_pid()? == parent.pid && process_executable(parent.pid)? == parent.executable,
        "loader7_actual_compiler_parent_changed"
    );
    crate::native_archive::verify_binding(&parent.executable)
}
#[cfg(unix)]
pub(crate) fn actual_compiler_parent(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    context_path: &Path,
    received: &[String],
) -> Result<ActualCompilerParent> {
    let p = inspect_policy(root, &recipe.cargo_loader_policy)?;
    let pid = parent_pid()?;
    let executable = process_executable(pid)?;
    ensure!(
        executable == p.compiler,
        "loader6_live_parent_not_selected_rustc"
    );
    let stage = context_path.parent().context("loader6_stage_parent")?;
    let intent_ref = reference(
        root,
        &stage
            .join("loader-events")
            .join(format!("compiler-{pid}.json")),
    )?;
    let i: CodeNativeCompilerExecIntent6 = serde_json::from_value(read(root, &intent_ref)?)?;
    i.validate()
        .map_err(|e| anyhow::anyhow!("loader6_intent_contract:{e:?}"))?;
    ensure!(
        i.schema == "oev2.native-compiler-exec-intent/6"
            && i.execution == "before_exec"
            && i.command_scope == "cargo_compiler_command"
            && i.build_script_binding.is_none()
            && i.pid == pid
            && i.compiler == executable
            && i.wrapper == p.rustc_wrapper
            && i.stage_context == reference(root, context_path)?
            && i.parent_executable == p.cargo
            && i.parent_executable.path == side.build.cargo_path,
        "loader6_actual_parent_intent_changed"
    );
    // Capture selected non-secret native environment before refusal. This raw
    // record is diagnostic only, not an execution or owner capability.
    use std::os::unix::ffi::OsStrExt;
    let native_environment_raw = ["SDKROOT", "DEVELOPER_DIR", "MACOSX_DEPLOYMENT_TARGET"]
        .into_iter().map(|name| json!({"name":name,"value_bytes":std::env::var_os(name).map(|v|v.as_bytes().to_vec())})).collect::<Vec<_>>();
    write_new(
        &root.private_path(&stage.join("loader-events").join(format!(
            "native-pre-environment-{}.json",
            std::process::id()
        )))?,
        &json!({"schema":"oev2.native-compiler-child-pre-admission-raw/6", "pid":std::process::id(), "parent_pid":pid,
            "parent_executable":executable,"compiler_intent":intent_ref,"stage_context":reference(root,context_path)?,
            "received_arguments":received,"cwd":std::env::current_dir()?,"native_environment_raw":native_environment_raw,
            "qualification":"not_established","boundary":"Actual live selected Rustc parent and intent compared; SDK/environment admission and native dispatch not reached"}),
    )?;
    if p.child_environment_strategy == crate::native_sdkroot6::STRATEGY {
        let arches = received
            .windows(2)
            .filter(|w| w[0] == "-arch")
            .collect::<Vec<_>>();
        ensure!(
            arches.len() == 1 && arches[0][1] == "arm64",
            "loader6_sdkroot_actual_arch_uncovered"
        );
    }
    let sdk_root = crate::native_sdkroot6::inspect_actual(
        root,
        recipe,
        &p,
        std::env::var_os("SDKROOT").as_deref(),
    )?;
    let environment = selected_environment(
        side,
        &p,
        false,
        sdk_root.as_ref().map(|s| s.raw_value.as_str()),
    )?;
    // Diagnostics are added at exec; the original Cargo fallback must be exact.
    ensure!(
        environment
            .iter()
            .find(|e| e.name == "DYLD_FALLBACK_LIBRARY_PATH")
            == i.environment
                .iter()
                .find(|e| e.name == "DYLD_FALLBACK_LIBRARY_PATH"),
        "loader6_cargo_fallback_changed_after_exec"
    );
    Ok(ActualCompilerParent {
        intent: intent_ref,
        pid,
        executable,
        environment,
        sdk_root,
    })
}
/// Selected removal is an explicit /6 native-child recipe operation. It never
/// changes the already running Rustc/adapter or silently claims loader closure.
pub(crate) fn selected_child_removals() -> Result<Vec<OsString>> {
    for (k, _) in std::env::vars_os() {
        let key = k.to_str().context("loader6_environment_encoding")?;
        if key.starts_with("DYLD_") {
            ensure!(
                ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_PRINT_LIBRARIES"].contains(&key),
                "loader6_child_environment_unselected"
            );
        }
    }
    Ok(vec![
        "DYLD_FALLBACK_LIBRARY_PATH".into(),
        "DYLD_PRINT_LIBRARIES".into(),
    ])
}
fn removed_environment(
    environment: &[CodeNativeEnvironmentEntry],
) -> Vec<CodeNativeEnvironmentEntry> {
    environment
        .iter()
        .filter(|e| {
            ["DYLD_FALLBACK_LIBRARY_PATH", "DYLD_PRINT_LIBRARIES"].contains(&e.name.as_str())
        })
        .cloned()
        .collect()
}
pub(crate) fn retain_native_receipt(
    root: &SkillRoot,
    cwd: &Path,
    parent: &ActualCompilerParent,
    selected_libraries: Vec<CodeNativeSdkSelectedLibrary4>,
    sdk: &CodeNativeSdkLayout4,
) -> Result<()> {
    let r = CodeNativeLoaderReceipt6 {
        schema: "oev2.native-loader-receipt/6".into(),
        compiler_intent: parent.intent.clone(),
        native_adapter_pid: std::process::id() as u64,
        parent_pid: parent.pid,
        parent_executable: parent.executable.clone(),
        loaded_images: current_images()?,
        received_environment: parent.environment.clone(),
        removed_child_environment: removed_environment(&parent.environment),
        sdk_root: parent.sdk_root.clone(),
        selected_libraries,
        tbd_documents: crate::native_sdk4::inspect_tbd_metadata4(sdk)?,
        uncovered: unknowns(),
    };
    r.validate()
        .map_err(|e| anyhow::anyhow!("loader6_receipt_contract:{e:?}"))?;
    write_new(
        &root.private_path(&cwd.join("loader-receipt.json"))?,
        &serde_json::to_value(r)?,
    )
}
pub(crate) fn retained_loader_bindings(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<(PathBuf, CodeNativeRetainedLoaderObservation6)>> {
    retained_loader_bindings_mode(root, side, recipe, 6)
}
fn retained_loader_bindings7(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<(PathBuf, CodeNativeRetainedLoaderObservation6)>> {
    ensure!(
        crate::native_link7::is_adapter7(recipe)?,
        "compiler7_selected_adapter_required"
    );
    retained_loader_bindings_mode(root, side, recipe, 7)
}
/// Shared actual receipt/intent/parent/environment/SDK association. Diagnostic
/// PID text is checked separately and never substitutes for these conditions.
pub(crate) fn retained_call_association(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    revision: u8,
    p: &CodeNativeCargoLoaderPolicy6,
    context: &ArtifactReference,
    call: &Path,
) -> Result<(ArtifactReference, u64)> {
    let rr = reference(root, &call.join("loader-receipt.json"))?;
    let r: CodeNativeLoaderReceipt6 = serde_json::from_value(read(root, &rr)?)?;
    let i: CodeNativeCompilerExecIntent6 = serde_json::from_value(read(root, &r.compiler_intent)?)?;
    r.validate()
        .map_err(|e| anyhow::anyhow!("loader6_receipt_contract:{e:?}"))?;
    i.validate()
        .map_err(|e| anyhow::anyhow!("loader6_intent_contract:{e:?}"))?;
    ensure!(
        r.schema == "oev2.native-loader-receipt/6"
            && i.schema == "oev2.native-compiler-exec-intent/6"
            && i.command_scope == "cargo_compiler_command"
            && r.parent_pid == i.pid
            && r.parent_executable == p.compiler
            && i.compiler == p.compiler
            && i.wrapper == p.rustc_wrapper
            && i.parent_executable == p.cargo
            && i.stage_context == *context,
        "loader6_retained_association_changed"
    );
    let original = i
        .environment
        .iter()
        .find(|e| e.name == "DYLD_FALLBACK_LIBRARY_PATH")
        .context("loader6_retained_fallback_missing")?;
    validate_fallback(&original.value, side, &p)?;
    ensure!(
        r.received_environment
            .iter()
            .find(|e| e.name == original.name)
            == Some(original)
            && r.removed_child_environment == removed_environment(&r.received_environment),
        "loader6_retained_removal_changed"
    );
    let value = read(root, &reference(root, &call.join("invocation.json"))?)?;
    let original_arguments =
        if revision == 8 && value["schema"] == "oev2.native-sdk-link-invocation/8" {
            ensure!(
                crate::native_link8::is_adapter8(recipe)?,
                "sdk8_selected_adapter_required"
            );
            let inv: CodeNativeSdkLinkInvocation8 = serde_json::from_value(value)?;
            inv.validate()
                .map_err(|e| anyhow::anyhow!("sdk8_loader_invocation_contract:{e:?}"))?;
            inv.inputs.original_request.received_arguments
        } else if revision >= 7 && value["schema"] == "oev2.native-compiler-link-invocation/7" {
            ensure!(
                if revision == 8 {
                    crate::native_link8::is_adapter8(recipe)?
                } else {
                    crate::native_link7::is_adapter7(recipe)?
                },
                "compiler7_selected_adapter_required"
            );
            let inv: CodeNativeCompilerLinkInvocation7 = serde_json::from_value(value)?;
            inv.validate()
                .map_err(|e| anyhow::anyhow!("compiler7_loader_invocation_contract:{e:?}"))?;
            inv.inputs.file_inputs.request.received_arguments
        } else {
            let inv: CodeNativeArchiveResponseInvocation = serde_json::from_value(value)?;
            inv.inputs.expanded_arguments
        };
    let retained_sdk_value = r
        .received_environment
        .iter()
        .find(|e| e.name == "SDKROOT")
        .map(|e| std::ffi::OsStr::new(&e.value));
    ensure!(
        crate::native_sdkroot6::inspect_actual(root, recipe, &p, retained_sdk_value)? == r.sdk_root,
        "loader6_retained_sdkroot_changed"
    );
    let layout = crate::native_sdk4::inspect_sdk4(root, &recipe.sdk_layout)?;
    ensure!(
        r.tbd_documents == crate::native_sdk4::inspect_tbd_metadata4(&layout)?,
        "loader6_retained_tbd_snapshot_changed"
    );
    ensure!(
        r.selected_libraries
            == crate::native_sdk4::bind_selected_libraries4(&layout, &original_arguments)?,
        "loader6_selected_libraries_changed"
    );
    Ok((rr, i.pid))
}

fn retained_loader_bindings_mode(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
    revision: u8,
) -> Result<Vec<(PathBuf, CodeNativeRetainedLoaderObservation6)>> {
    let p = inspect_policy(root, &recipe.cargo_loader_policy)?;
    let mut refs = Vec::new();
    for stage in ["build-normalizer", "build-runner"] {
        let folder = Path::new(&side.stage_root).join(stage);
        let context = reference(root, &folder.join("stage-context.json"))?;
        let raw = read_regular_bytes(
            &Path::new(&side.build.evidence_path)
                .join(stage)
                .join("stderr"),
            side.body_budget.output_bytes as usize,
        )?;
        let report = crate::native_runtime5_content::inspect_loader_diagnostic_bytes(
            "host_compilation",
            &raw,
        )?;
        let mut dirs = std::fs::read_dir(&folder)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        dirs.sort();
        for call in dirs.iter().filter(|p| {
            p.file_name()
                .and_then(|p| p.to_str())
                .is_some_and(|n| n.starts_with("call-"))
        }) {
            let (rr, pid) =
                retained_call_association(root, side, recipe, revision, &p, &context, call)?;
            let ordinals = report
                .lines
                .iter()
                .filter(|line| line.reported_pid == Some(pid) && line.event == "uuid_path_report")
                .map(|line| line.ordinal)
                .collect::<Vec<_>>();
            ensure!(
                !ordinals.is_empty(),
                "loader6_actual_rustc_loader_output_missing"
            );
            let observed = CodeNativeRetainedLoaderObservation6 {
                schema: "oev2.native-retained-loader-observation/6".into(),
                native_receipt: rr,
                raw_compiler_stream: reference(
                    root,
                    &Path::new(&side.build.evidence_path)
                        .join(stage)
                        .join("stderr"),
                )?,
                diagnostic_report: report.clone(),
                compiler_reported_lines: ordinals,
            };
            observed
                .validate()
                .map_err(|e| anyhow::anyhow!("loader6_observation_contract:{e:?}"))?;
            refs.push((call.join("retained-loader-observation.json"), observed));
        }
    }
    ensure!(!refs.is_empty(), "loader6_receipts_missing");
    Ok(refs)
}

pub(crate) fn prepare_retained(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    let bindings = retained_loader_bindings(root, side, recipe)?;
    let mut out = Vec::new();
    for (file, observed) in bindings {
        write_new(&file, &serde_json::to_value(observed)?)?;
        out.push(reference(root, &file)?);
    }
    Ok(out)
}
pub(crate) fn verify_retained(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    let mut out = Vec::new();
    for (file, expected) in retained_loader_bindings(root, side, recipe)? {
        let r = reference(root, &file)?;
        ensure!(
            read(root, &r)? == serde_json::to_value(expected)?,
            "loader6_complete_raw_binding_changed"
        );
        out.push(r);
    }
    Ok(out)
}

pub(crate) fn prepare_retained7(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    let bindings = retained_loader_bindings7(root, side, recipe)?;
    let mut out = Vec::new();
    for (file, observed) in bindings {
        write_new(&file, &serde_json::to_value(observed)?)?;
        out.push(reference(root, &file)?);
    }
    Ok(out)
}
pub(crate) fn verify_retained7(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    let mut out = Vec::new();
    for (file, expected) in retained_loader_bindings7(root, side, recipe)? {
        let r = reference(root, &file)?;
        ensure!(
            read(root, &r)? == serde_json::to_value(expected)?,
            "loader6_complete_raw_binding_changed"
        );
        out.push(r);
    }
    Ok(out)
}
fn retained_loader_bindings8(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<(PathBuf, CodeNativeRetainedLoaderObservation6)>> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?,
        "sdk8_selected_adapter_required"
    );
    retained_loader_bindings_mode(root, side, recipe, 8)
}
pub(crate) fn prepare_retained8(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    let bindings = retained_loader_bindings8(root, side, recipe)?;
    let mut out = Vec::new();
    for (file, observed) in bindings {
        write_new(&file, &serde_json::to_value(observed)?)?;
        out.push(reference(root, &file)?);
    }
    Ok(out)
}
pub(crate) fn verify_retained8(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    let mut out = Vec::new();
    for (file, expected) in retained_loader_bindings8(root, side, recipe)? {
        let r = reference(root, &file)?;
        ensure!(
            read(root, &r)? == serde_json::to_value(expected)?,
            "loader6_complete_raw_binding_changed"
        );
        out.push(r);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inputs() -> (
        tempfile::TempDir,
        CodeNativeSideContext6,
        CodeNativeCargoLoaderPolicy6,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let target = root.join("target");
        std::fs::create_dir(&target).unwrap();
        let binding = CodeNativeFileBinding {
            path: root.join("toolchain/bin/rustc").display().to_string(),
            sha256: "0".repeat(64),
            bytes: 1,
            mode: 0o755,
        };
        let reference = ArtifactReference {
            path: "var/content-only.json".into(),
            sha256: "0".repeat(64),
        };
        let side = CodeNativeSideContext6 {
            schema: "oev2.native-side-context/6".into(),
            source_profile: reference.clone(),
            recipe_sha256: "0".repeat(64),
            owner_context: reference,
            side_id: "content-only".into(),
            build: CodeBuildRequest {
                artifact_id: "content-only".into(),
                source_root: root.display().to_string(),
                evidence_path: root.join("evidence").display().to_string(),
                cargo_path: "not-an-execution-pin".into(),
                rustc_path: binding.path.clone(),
                ggen_path: "not-an-execution-pin".into(),
                cargo_home: "content-only".into(),
                target_dir: target.display().to_string(),
                platform: "aarch64-apple-darwin".into(),
            },
            stage_root: root.join("target-native-stage").display().to_string(),
            body_budget: CodeBudget {
                timeout_ms: 1000,
                output_bytes: 4096,
            },
            logical_debug_root: "oe-native-debug".into(),
        };
        let p = CodeNativeCargoLoaderPolicy6 {
            schema: "oev2.native-cargo-loader-policy/6".into(),
            rustc_wrapper: binding.clone(),
            compiler: binding.clone(),
            cargo: binding,
            metadata_probe: "cargo_rustc_version_exact_vv/1".into(),
            fallback_tail: vec!["/usr/local/lib".into(), "/usr/lib".into()],
            child_environment_strategy: "record_cargo_fallback_then_remove_from_native_child/6"
                .into(),
        };
        (dir, side, p)
    }
    #[test]
    fn exact_original_cargo_tail_and_target_scope_are_content_only() {
        let (_d, s, p) = inputs();
        let dirs = format!("{}/debug/deps:/usr/local/lib:/usr/lib", s.build.target_dir);
        validate_fallback(&dirs, &s, &p).unwrap();
        for bad in [
            format!("{}/debug/deps::/usr/local/lib:/usr/lib", s.build.target_dir),
            format!("{}/debug/deps:/usr/lib:/usr/local/lib", s.build.target_dir),
            "/unselected/lib:/usr/local/lib:/usr/lib".into(),
            "relative:/usr/local/lib:/usr/lib".into(),
            format!(
                "{0}/debug/deps:{0}/debug/deps:/usr/local/lib:/usr/lib",
                s.build.target_dir
            ),
        ] {
            assert!(validate_fallback(&bad, &s, &p).is_err(), "{bad}");
        }
    }
    #[test]
    fn directory_symlink_and_parent_escape_do_not_get_cargo_authority() {
        use std::os::unix::fs::symlink;
        let (d, s, p) = inputs();
        let outside = d.path().join("external");
        std::fs::create_dir(&outside).unwrap();
        let alias = Path::new(&s.build.target_dir).join("aliased");
        symlink(&outside, &alias).unwrap();
        assert!(
            validate_fallback(
                &format!("{}:/usr/local/lib:/usr/lib", alias.display()),
                &s,
                &p
            )
            .is_err()
        );
        assert!(
            validate_fallback(
                &format!("{}/../external:/usr/local/lib:/usr/lib", s.build.target_dir),
                &s,
                &p
            )
            .is_err()
        );
    }
    #[test]
    fn native3_wire_identity_is_not_a_silent_loader6_upgrade() {
        let (_, s, _) = inputs();
        let mut v = serde_json::to_value(s).unwrap();
        v["schema"] = json!("oev2.native-side-context/3");
        let typed: CodeNativeSideContext6 = serde_json::from_value(v).unwrap();
        assert!(typed.validate().is_err());
    }
    #[test]
    fn unknown_fields_and_fake_caller_authority_are_not_part_of_new_contract() {
        let (_, s, _) = inputs();
        let mut v = serde_json::to_value(s).unwrap();
        v["owner_observed"] = json!(true);
        assert!(serde_json::from_value::<CodeNativeSideContext6>(v).is_err());
    }
    #[test]
    fn local_parent_executable_is_an_actual_process_observation() {
        let pid = parent_pid().unwrap();
        let pin = process_executable(pid).unwrap();
        assert!(pid > 0);
        crate::native_profile::verify_file(&pin).unwrap();
        assert!(process_executable(u64::MAX).is_err());
    }
    #[test]
    fn local_image_observations_keep_nonphysical_cache_unknown() {
        let images = current_images().unwrap();
        assert!(!images.is_empty());
        for i in images {
            assert!(i.header_address > 0);
            match i.physical_file {
                Some(p) => {
                    assert_eq!(i.storage_observation, "regular_file");
                    crate::native_profile::verify_file(&p).unwrap();
                }
                None => assert_eq!(i.storage_observation, "nonphysical_or_unreadable_unknown"),
            }
        }
        assert!(unknowns().iter().any(|s| s.contains("unknown")));
    }
    #[test]
    fn exact_selected_workspace_caller_is_not_a_package_prefix_permission() {
        let (_d, s, _) = inputs();
        let canonical_root = Path::new(&s.build.source_root);
        std::fs::write(
            canonical_root.join("SKILL.md"),
            b"finite caller content fixture",
        )
        .unwrap();
        let root = SkillRoot::new(canonical_root).unwrap();
        validate_compiler_caller(&root, &s, "build-normalizer", &json!({}), root.path()).unwrap();
        for cwd in [
            root.path().join("unselected"),
            root.path().with_file_name("sibling"),
        ] {
            assert!(
                validate_compiler_caller(&root, &s, "build-normalizer", &json!({}), &cwd).is_err()
            );
        }
    }
    #[test]
    fn compiler_version_probe_is_a_complete_exact_vector_not_compilation_permission() {
        let (_, _, mut policy) = inputs();
        assert!(is_version_probe(&["-vV".into()], &policy).unwrap());
        for arguments in [
            vec![],
            vec!["-Vv".into()],
            vec!["-vV".into(), "extra".into()],
            vec!["--print=file-names".into()],
            vec!["-vV --crate-type bin".into()],
        ] {
            assert!(!is_version_probe(&arguments, &policy).unwrap());
        }
        policy.metadata_probe = "caller_verified".into();
        assert!(is_version_probe(&["-vV".into()], &policy).is_err());
    }

    #[test]
    fn target_information_family_consumes_frozen_source_defined_complete_vectors() {
        let f: Value = serde_json::from_slice(&private_test_fixtures::bytes(
            "FIXED-CARGO-TARGET-QUERY-FAMILY-06/FIXED-INPUTS-AND-EXPECTED-01.json",
        ))
        .unwrap();
        let content: Value = serde_json::from_slice(&private_test_fixtures::bytes(
            "FIXED-CARGO-TARGET-QUERY-FAMILY-06/FIXED-PURE-PREDICATE-CONTEXT-01.json",
        ))
        .unwrap();
        let recipe: CodeNativeBuildRecipe6 =
            serde_json::from_value(content["recipe"].clone()).unwrap();
        let (_, _, mut policy) = inputs();
        policy.metadata_probe = TARGET_INFORMATION_POLICY.into();
        policy.cargo = serde_json::from_value(f["actual_cargo_pin"].clone()).unwrap();
        for case in f["supported"].as_array().unwrap() {
            let args: Vec<String> = serde_json::from_value(case["arguments"].clone()).unwrap();
            assert_eq!(
                compiler_scope(
                    &args,
                    &policy,
                    &recipe,
                    case["parent_stage"].as_str().unwrap()
                )
                .unwrap(),
                case["expected_scope"].as_str().unwrap(),
                "{}",
                case["case_id"]
            );
        }
        for case in f["negative"].as_array().unwrap() {
            let args: Vec<String> = serde_json::from_value(case["arguments"].clone()).unwrap();
            let scope = compiler_scope(
                &args,
                &policy,
                &recipe,
                case["parent_stage"].as_str().unwrap(),
            );
            assert!(
                scope.is_err() || scope.unwrap() == "cargo_compiler_command",
                "{}",
                case["case_id"]
            );
        }
        assert_eq!(
            compiler_scope(&["-vV".into()], &policy, &recipe, "build-normalizer").unwrap(),
            "cargo_version_probe"
        );
        policy.cargo.sha256 = "0".repeat(64);
        assert!(compiler_scope(&["-vV".into()], &policy, &recipe, "build-normalizer").is_err());
        policy.metadata_probe = "cargo_rustc_version_exact_vv/1".into();
        let args: Vec<String> =
            serde_json::from_value(f["supported"][0]["arguments"].clone()).unwrap();
        assert_eq!(
            compiler_scope(&args, &policy, &recipe, "build-normalizer").unwrap(),
            "cargo_compiler_command"
        );
    }
    #[test]
    fn metadata_null_descriptor_is_actual_fd_identity_not_dash_or_caller_bool() {
        use std::os::fd::AsRawFd;
        let null = std::fs::File::open("/dev/null").unwrap();
        let fact = inspect_null_descriptor6(null.as_raw_fd()).unwrap();
        assert_eq!(fact.mode & libc::S_IFMT as u64, libc::S_IFCHR as u64);
        let file = tempfile::tempfile().unwrap();
        assert!(inspect_null_descriptor6(file.as_raw_fd()).is_err());
        let (socket, _) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(inspect_null_descriptor6(socket.as_raw_fd()).is_err());
        let mut fds = [-1, -1];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        assert!(inspect_null_descriptor6(fds[0]).is_err());
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
        assert!(inspect_null_descriptor6(-1).is_err());
    }

    #[test]
    fn source_defined_default_lib_filename_probe_is_not_missing_from_family() {
        let f: Value = serde_json::from_slice(&private_test_fixtures::bytes(
            "FIXED-CARGO-TARGET-QUERY-FAMILY-06/FIXED-LIB-SOURCE-EXPECTED-02.json",
        ))
        .unwrap();
        let content: Value = serde_json::from_slice(&private_test_fixtures::bytes(
            "FIXED-CARGO-TARGET-QUERY-FAMILY-06/FIXED-PURE-PREDICATE-CONTEXT-01.json",
        ))
        .unwrap();
        let recipe: CodeNativeBuildRecipe6 =
            serde_json::from_value(content["recipe"].clone()).unwrap();
        let (_, _, mut policy) = inputs();
        policy.metadata_probe = TARGET_INFORMATION_POLICY.into();
        policy.cargo = crate::native_archive::bind_file(Path::new(env!("CARGO"))).unwrap();
        for case in f["supported"].as_array().unwrap() {
            let args: Vec<String> = serde_json::from_value(case["arguments"].clone()).unwrap();
            assert_eq!(
                compiler_scope(
                    &args,
                    &policy,
                    &recipe,
                    case["parent_stage"].as_str().unwrap()
                )
                .unwrap(),
                "cargo_target_information_probe",
                "{}",
                case["case_id"]
            );
        }
    }
    #[test]
    fn sdkroot_strategy_is_separate_from_original_child_and_metadata_domains() {
        let (_, _, mut p) = inputs();
        crate::native_sdkroot6::inspect_policy(&p).unwrap();
        p.child_environment_strategy = crate::native_sdkroot6::STRATEGY.into();
        assert!(crate::native_sdkroot6::inspect_policy(&p).is_err());
        p.compiler.sha256 =
            "b829b733131d4e1673eeebd1f34d06ae1e9ff4977b051313cf42e2a9e79ecf1c".into();
        p.compiler.bytes = 412_504;
        p.compiler.mode = 0o755;
        crate::native_sdkroot6::inspect_policy(&p).unwrap();
        for bad in [
            "0".repeat(64),
            "b829b733131d4e1673eeebd1f34d06ae1e9ff4977b051313cf42e2a9e79ecf1d".into(),
        ] {
            let mut p = p.clone();
            p.compiler.sha256 = bad;
            assert!(crate::native_sdkroot6::inspect_policy(&p).is_err());
        }
        let mut bad = p.clone();
        bad.compiler.bytes += 1;
        assert!(crate::native_sdkroot6::inspect_policy(&bad).is_err());
        let mut bad = p.clone();
        bad.compiler.mode = 0o700;
        assert!(crate::native_sdkroot6::inspect_policy(&bad).is_err());
        // Metadata policy2 is not changed by selecting the separate native
        // environment domain, and no new native environment permission is
        // derived from the [-vV] metadata predicate.
        assert!(is_version_probe(&["-vV".into()], &p).unwrap());
        assert!(!is_version_probe(&["-vV".into(), "extra".into()], &p).unwrap());
    }
    #[test]
    fn retained_native_sdkroot_is_not_reported_as_removed_child_environment() {
        let env = vec![
            CodeNativeEnvironmentEntry {
                name: "DYLD_FALLBACK_LIBRARY_PATH".into(),
                value: "original".into(),
            },
            CodeNativeEnvironmentEntry {
                name: "SDKROOT".into(),
                value: "original-alias-not-projected".into(),
            },
            CodeNativeEnvironmentEntry {
                name: "DYLD_PRINT_LIBRARIES".into(),
                value: "1".into(),
            },
        ];
        let removed = removed_environment(&env);
        assert_eq!(removed.len(), 2);
        assert!(removed.iter().all(|e| e.name.starts_with("DYLD_")));
        assert_eq!(env[1].value, "original-alias-not-projected");
    }
}

#[cfg(test)]
mod bound_build_script_policy_tests {
    use super::*;
    #[test]
    fn new_policy_is_explicit_and_preserves_original_selected_cargo_pin() {
        let file = CodeNativeFileBinding {
            path: "/content/only/not-executed".into(),
            sha256: TARGET_INFORMATION_CARGO_SHA256.into(),
            bytes: 31_232_696,
            mode: 0o755,
        };
        let mut p = CodeNativeCargoLoaderPolicy6 {
            schema: "oev2.native-cargo-loader-policy/6".into(),
            rustc_wrapper: file.clone(),
            compiler: file.clone(),
            cargo: file,
            metadata_probe: crate::native_buildscript6::POLICY.into(),
            fallback_tail: vec!["/usr/local/lib".into(), "/usr/lib".into()],
            child_environment_strategy: "record_cargo_fallback_then_remove_from_native_child/6"
                .into(),
        };
        target_information_policy(&p).unwrap();
        p.cargo.sha256 = "b".repeat(64);
        assert!(target_information_policy(&p).is_err());
        p.cargo.sha256 = TARGET_INFORMATION_CARGO_SHA256.into();
        p.metadata_probe = "cargo_rustc_version_exact_vv/1".into();
        assert!(target_information_policy(&p).is_err());
        assert!(is_version_probe(&["--version".into()], &p).unwrap() == false);
        p.metadata_probe = TARGET_INFORMATION_POLICY.into();
        target_information_policy(&p).unwrap();
        assert!(is_version_probe(&["-vV".into()], &p).is_err());
    }
}

#[cfg(test)]
mod single_checked_call_tests6 {
    use super::*;
    #[test]
    fn checked_call_refuses_source_context_or_raw_byte_change() {
        let root = Path::new("/fixed/root");
        let context = Path::new("/fixed/root/var/stage-context.json");
        assert!(
            checked_call_binding6(
                root, root, context, context, "fixed", "fixed", "fixed", 20, 10
            )
            .is_ok()
        );
        for (selected_root, selected_context, selected_sha, actual_sha) in [
            (Path::new("/other/root"), context, "fixed", "fixed"),
            (
                root,
                Path::new("/other/stage-context.json"),
                "fixed",
                "fixed",
            ),
            (root, context, "changed", "fixed"),
            (root, context, "fixed", "changed"),
        ] {
            let error = checked_call_binding6(
                root,
                selected_root,
                context,
                selected_context,
                "fixed",
                selected_sha,
                actual_sha,
                20,
                10,
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("loader6_checked_call_source_or_context_changed")
            );
        }
    }
    #[test]
    fn checked_call_refuses_original_deadline_boundary_without_renewal() {
        let root = Path::new("/fixed/root");
        let context = Path::new("/fixed/context");
        for now in [20, 21] {
            let error = checked_call_binding6(
                root, root, context, context, "fixed", "fixed", "fixed", 20, now,
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("loader6_checked_call_deadline_expired")
            );
        }
    }
}

// The /6 live compiler association retains its original Unix descriptors,
// environment bytes and exec ABI. Other hosts cannot substitute Windows handles
// or string encodings for these facts; reject before any context read or spawn.
#[cfg(not(unix))]
pub(crate) fn inspect_null_descriptor6(_fd: i32) -> Result<DescriptorFact6> {
    anyhow::bail!("loader6_platform_unsupported")
}
#[cfg(not(unix))]
pub fn compiler_exec_entry() -> Result<()> {
    anyhow::bail!("loader6_platform_unsupported")
}
#[cfg(not(unix))]
pub(crate) fn compiler_exec_entry_from_checked_stage6(
    _checked: CheckedCompilerStage6,
    _argv: Vec<String>,
) -> Result<()> {
    anyhow::bail!("loader6_platform_unsupported")
}
#[cfg(not(unix))]
pub(crate) fn actual_compiler_parent(
    _root: &SkillRoot,
    _side: &CodeNativeSideContext6,
    _recipe: &CodeNativeBuildRecipe6,
    _context_path: &Path,
    _received: &[String],
) -> Result<ActualCompilerParent> {
    anyhow::bail!("loader6_platform_unsupported")
}
