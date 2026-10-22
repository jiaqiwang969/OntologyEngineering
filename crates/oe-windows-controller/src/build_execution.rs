//! Portable, source-bound execution planning. Nothing here creates a directory,
//! starts a process, loads NX or authenticates an external execution record.
use crate::{
    build::{BuildSpec, PlannedCommand, normalize, parent, windows_path},
    build_source::{SourceAssessment, VerifiedSourceArchive},
};
use anyhow::{Result, ensure};
use oe_nx_protocol::{
    BlobIdentity, Digest, JobId, MAX_RECORD_BYTES, RootIdentity, compatibility_collision_key,
    record_digest,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EXECUTION_SPEC_SCHEMA: &str = "cad-agent.nx-worker-execution-spec/v2";
pub const EXECUTION_PLAN_SCHEMA: &str = "cad-agent.nx-worker-execution-plan/v2";
const MAX_WINDOWS_STRING_UNITS: usize = 32_767;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// Cargo resolution is offline. Build scripts and compilers are not in an
    /// OS network sandbox, and this planner does not observe network activity.
    CargoOfflineOnly,
    /// Unsupported: do not silently downgrade this requirement to Cargo flags.
    RequireOsDenied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSpecV2 {
    pub schema: String,
    /// Reuse the bounded v1 input declarations, not its historical execution
    /// plan or network_allowed field. No automatic v1-to-v2 conversion exists.
    pub build: BuildSpec,
    pub network_policy: NetworkPolicy,
}
impl ExecutionSpecV2 {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == EXECUTION_SPEC_SCHEMA,
            "execution schema differs"
        );
        self.build.validate()?;
        ensure!(
            self.network_policy == NetworkPolicy::CargoOfflineOnly,
            "OS network denial is not provided by this planner"
        );
        Ok(())
    }
    pub fn digest(&self) -> Result<Digest> {
        self.validate()?;
        record_digest(self)
    }
}

/// Serializable facts about verified source bytes. Decoding this record does
/// not create a VerifiedSourceArchive or a PreparedExecutionPlan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceBinding {
    pub archive: BlobIdentity,
    pub inventory: BlobIdentity,
    pub cargo_lock: BlobIdentity,
    pub root_cargo_manifest: BlobIdentity,
    pub expanded_files: u32,
    pub expanded_bytes: u64,
    pub workspace_members: Vec<String>,
    pub lock_packages: usize,
    pub registry_packages: usize,
}
impl SourceBinding {
    fn verified(value: &SourceAssessment) -> Self {
        Self {
            archive: value.archive.clone(),
            inventory: value.manifest.clone(),
            cargo_lock: value.cargo_lock.clone(),
            root_cargo_manifest: value.root_cargo_manifest.clone(),
            expanded_files: value.expanded_files,
            expanded_bytes: value.expanded_bytes,
            workspace_members: value.workspace_members.clone(),
            lock_packages: value.lock_packages,
            registry_packages: value.registry_packages,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanningAssessment {
    pub source_bytes: String,
    pub filesystem_materialization: String,
    pub cargo_resolution: String,
    pub installed_tools_sdk_and_root: String,
    pub process_execution: String,
    pub os_network_enforcement: String,
    pub network_activity: String,
    pub native_execution: String,
    pub engineering_acceptance: String,
}
fn assessment() -> PlanningAssessment {
    PlanningAssessment {
        source_bytes: "verified_in_memory_against_bound_inventory".into(),
        filesystem_materialization: "not_performed".into(),
        cargo_resolution: "not_executed".into(),
        installed_tools_sdk_and_root: "declared_identities_not_observed".into(),
        process_execution: "not_started".into(),
        os_network_enforcement: "not_provided".into(),
        network_activity: "not_observed".into(),
        native_execution: "not_requested".into(),
        engineering_acceptance: "not_evaluated".into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowsCommandLine {
    /// Future CreateProcessW lpApplicationName, never a shell executable.
    pub application_name: String,
    /// CRT-style quoted argv0 and fixed arguments, including the terminal NUL.
    /// This is portable encoding, not a claim of Windows execution acceptance.
    pub command_line_utf16: Vec<u16>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCommand {
    pub command: PlannedCommand,
    pub windows: WindowsCommandLine,
}

/// Strict but untrusted serialized plan. Revalidation recomputes every field
/// against actual verified source bytes; it never authenticates process claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPlanRecord {
    pub schema: String,
    pub spec_digest: Digest,
    pub build_declarations_digest: Digest,
    pub build_id: JobId,
    pub root_identity: RootIdentity,
    pub source: SourceBinding,
    pub source_directory: String,
    pub new_output_directory: String,
    pub commands: Vec<ExecutionCommand>,
    pub max_seconds: u16,
    pub output_must_be_new: bool,
    pub network_policy: NetworkPolicy,
    pub assessment: PlanningAssessment,
}
impl ExecutionPlanRecord {
    pub fn validate_against(
        &self,
        spec: &ExecutionSpecV2,
        source: &VerifiedSourceArchive,
    ) -> Result<()> {
        ensure!(
            self == prepare_plan(spec, source)?.record(),
            "external execution plan differs from recomputed source-bound plan"
        );
        Ok(())
    }
}

/// Proof of planning over verified bytes only. Private construction, no
/// Deserialize, and no execution-success constructor or ExecutedBuild type.
pub struct PreparedExecutionPlan {
    record: ExecutionPlanRecord,
}
impl PreparedExecutionPlan {
    pub fn record(&self) -> &ExecutionPlanRecord {
        &self.record
    }
    pub fn digest(&self) -> Result<Digest> {
        record_digest(&self.record)
    }
}

pub fn prepare_plan(
    spec: &ExecutionSpecV2,
    verified_source: &VerifiedSourceArchive,
) -> Result<PreparedExecutionPlan> {
    spec.validate()?;
    let build = &spec.build;
    let source = SourceBinding::verified(verified_source.assessment());
    ensure!(
        source.archive == build.source_archive.identity
            && source.inventory == build.source_inventory.identity
            && source.cargo_lock == build.cargo_lock.identity
            && source.expanded_files == build.expanded_source_files
            && source.expanded_bytes == build.expanded_source_bytes,
        "verified source differs from execution declarations"
    );
    let output = format!(
        "{}/build-{}",
        normalize(&build.root_directory),
        build.build_id.as_str()
    );
    let workspace = format!("{output}/source/workspace");
    // These are future new-directory requirements, not filesystem observations.
    // An installed input cannot also lie in the as-yet-uncreated output tree.
    for path in [
        &build.tools.cargo.path,
        &build.tools.rustc.path,
        &build.tools.gnu_linker.path,
        &build.tools.msvc_cl.path,
        &build.tools.msvc_link.path,
        &build.tools.windows_directory,
        &build.sdk.root,
    ]
    .into_iter()
    .chain(build.tools.include_directories.iter())
    .chain(build.tools.library_directories.iter())
    {
        ensure!(
            !within(path, &output),
            "installed input overlaps new build output"
        );
    }
    let mut paths: Vec<String> = Vec::new();
    // Deliberate search order: MSVC's link.exe must precede unrelated link.exe.
    // Explicit RUSTC/GNU linker paths still bind Cargo's primary tools.
    for path in [
        parent(&build.tools.msvc_cl.path),
        parent(&build.tools.gnu_linker.path),
        parent(&build.tools.rustc.path),
        parent(&build.tools.cargo.path),
        format!("{}/System32", normalize(&build.tools.windows_directory)),
    ] {
        if !paths.iter().any(|known| key(known) == key(&path)) {
            paths.push(path);
        }
    }
    let environment = BTreeMap::from([
        ("CARGO_HOME".into(), format!("{output}/cargo-home")),
        ("CARGO_TARGET_DIR".into(), format!("{output}/target")),
        ("CARGO_NET_OFFLINE".into(), "true".into()),
        ("RUSTC".into(), normalize(&build.tools.rustc.path)),
        (
            "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER".into(),
            normalize(&build.tools.gnu_linker.path),
        ),
        (
            "SystemRoot".into(),
            normalize(&build.tools.windows_directory),
        ),
        ("PATH".into(), paths.join(";")),
        ("TEMP".into(), format!("{output}/temp")),
        ("TMP".into(), format!("{output}/temp")),
    ]);
    let commands = build
        .fixed_commands(&output, &workspace, environment)
        .into_iter()
        .map(|command| {
            windows_path(&command.working_directory)?;
            windows_path(&command.expected_output)?;
            environment_bound(&command.environment)?;
            let application_name = normalize(&command.executable.path);
            let command_line_utf16 = encode_command_line(&application_name, &command.arguments)?;
            Ok(ExecutionCommand {
                windows: WindowsCommandLine {
                    application_name,
                    command_line_utf16,
                },
                command,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let record = ExecutionPlanRecord {
        schema: EXECUTION_PLAN_SCHEMA.into(),
        spec_digest: spec.digest()?,
        build_declarations_digest: build.digest()?,
        build_id: build.build_id.clone(),
        root_identity: build.root_identity.clone(),
        source,
        source_directory: workspace,
        new_output_directory: output,
        commands,
        max_seconds: build.max_seconds,
        output_must_be_new: true,
        network_policy: spec.network_policy,
        assessment: assessment(),
    };
    ensure!(
        serde_json::to_vec(&record)?.len() <= MAX_RECORD_BYTES,
        "execution plan exceeds record bound"
    );
    Ok(PreparedExecutionPlan { record })
}

fn key(path: &str) -> String {
    compatibility_collision_key(&normalize(path))
}
fn within(path: &str, directory: &str) -> bool {
    let (path, directory) = (key(path), key(directory));
    path == directory || path.starts_with(&(directory + "/"))
}
fn environment_bound(environment: &BTreeMap<String, String>) -> Result<()> {
    let mut units = 1_usize; // final NUL after the last key=value NUL
    for (name, value) in environment {
        ensure!(
            !name.is_empty()
                && !name.contains('=')
                && !name.chars().chain(value.chars()).any(char::is_control),
            "invalid finite environment string"
        );
        units = units
            .checked_add(name.encode_utf16().count() + value.encode_utf16().count() + 2)
            .ok_or_else(|| anyhow::anyhow!("environment size overflow"))?;
    }
    ensure!(
        units <= MAX_WINDOWS_STRING_UNITS,
        "environment exceeds bound"
    );
    Ok(())
}

// Windows CRT command-line encoding, not shell quoting. Always quote each
// argument; backslashes before quotes and trailing backslashes are doubled.
// No public caller-provided argv execution API is exposed by this helper.
fn encode_command_line(program: &str, arguments: &[String]) -> Result<Vec<u16>> {
    let mut encoded = Vec::new();
    for (index, argument) in std::iter::once(program)
        .chain(arguments.iter().map(String::as_str))
        .enumerate()
    {
        ensure!(
            !argument.chars().any(char::is_control),
            "control character in Windows argument"
        );
        if index != 0 {
            encoded.push(b' ' as u16);
        }
        encoded.push(b'"' as u16);
        let mut slashes = 0;
        for unit in argument.encode_utf16() {
            if unit == b'\\' as u16 {
                slashes += 1;
                continue;
            }
            let count = if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            };
            encoded.extend(std::iter::repeat_n(b'\\' as u16, count));
            slashes = 0;
            encoded.push(unit);
        }
        encoded.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
        encoded.push(b'"' as u16);
        ensure!(
            encoded.len() < MAX_WINDOWS_STRING_UNITS,
            "Windows command line exceeds bound"
        );
    }
    encoded.push(0);
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_encoding_matches_literal_crt_vectors_not_a_self_decoder() {
        for (argument, expected) in [
            ("", "\"tool.exe\" \"\""),
            ("a b", "\"tool.exe\" \"a b\""),
            ("a\"b", "\"tool.exe\" \"a\\\"b\""),
            ("a\\b", "\"tool.exe\" \"a\\b\""),
            ("a\\\"b", "\"tool.exe\" \"a\\\\\\\"b\""),
            ("tail\\", "\"tool.exe\" \"tail\\\\\""),
            ("tail\\\\", "\"tool.exe\" \"tail\\\\\\\\\""),
            ("C:\\目录 空间\\", "\"tool.exe\" \"C:\\目录 空间\\\\\""),
            ("%TEMP%&literal", "\"tool.exe\" \"%TEMP%&literal\""),
        ] {
            let expected: Vec<u16> = expected.encode_utf16().chain([0]).collect();
            assert_eq!(
                encode_command_line("tool.exe", &[argument.into()]).unwrap(),
                expected
            );
        }
        assert_eq!(
            encode_command_line("C:/编译 😀/cl.exe", &[]).unwrap(),
            vec![
                34, 67, 58, 47, 32534, 35793, 32, 55357, 56832, 47, 99, 108, 46, 101, 120, 101, 34,
                0
            ]
        );
    }

    #[test]
    fn windows_encoding_rejects_controls_and_counts_utf16_terminal_nul() {
        for bad in ["nul\0x", "line\nx", "tab\tx", "\u{7f}"] {
            assert!(encode_command_line("tool.exe", &[bad.into()]).is_err());
            assert!(encode_command_line(bad, &[]).is_err());
        }
        // Two quotes + argument + terminal NUL = 32767, counted in UTF-16.
        assert_eq!(
            encode_command_line(&"x".repeat(32_764), &[]).unwrap().len(),
            32_767
        );
        assert!(encode_command_line(&"x".repeat(32_765), &[]).is_err());
        assert!(encode_command_line(&"😀".repeat(16_383), &[]).is_err());
    }

    #[test]
    fn environment_bound_includes_equals_and_both_null_terminators() {
        let at_limit = BTreeMap::from([("TEMP".into(), "x".repeat(32_760))]);
        environment_bound(&at_limit).unwrap();
        let too_large = BTreeMap::from([("TEMP".into(), "x".repeat(32_761))]);
        assert!(environment_bound(&too_large).is_err());
        for (name, value) in [("TEMP", "a\0b"), ("BAD=KEY", "value"), ("", "value")] {
            assert!(environment_bound(&BTreeMap::from([(name.into(), value.into())])).is_err());
        }
    }
}
