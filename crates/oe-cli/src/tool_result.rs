//! `oev2 project tool-result`: the one door through which specialist tool
//! results return to the same Project, with provenance re-verified by Rust.
use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_contracts::{CapabilityRegistry, ToolComparisonRequest};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes};
use oe_kernel::project::{Project, memory, tool_result};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const LIMIT: usize = 64 * 1024 * 1024;

#[derive(Subcommand)]
pub enum ToolResultCommand {
    /// Re-verify one result: Project binding, registered tool, original bytes, findings and coverage.
    Verify {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = tool_result::DEFAULT_REGISTRY)]
        registry: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Register an accepted result into a new Project copy (evidence and decision dependencies only).
    Register {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = tool_result::DEFAULT_REGISTRY)]
        registry: PathBuf,
        /// New Project revision identifier.
        #[arg(long)]
        revision: String,
        /// New Project file; must not exist.
        #[arg(long)]
        output: PathBuf,
        /// Registration receipt; must not exist.
        #[arg(long)]
        receipt: Option<PathBuf>,
    },
    /// Compare two registered results: same tool, metric, environment and input set; baseline first.
    Compare {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Re-hash every registered executable.
    Registry {
        #[arg(long, default_value = tool_result::DEFAULT_REGISTRY)]
        registry: PathBuf,
    },
}

fn inside(root: &SkillRoot, p: &Path) -> Result<PathBuf> {
    let path = root.data_expand(p)?;
    ensure!(
        path.starts_with(root.data_path()),
        "tool_result_cli_path_outside_root"
    );
    oe_core::no_symlinks(&path)?;
    Ok(path)
}

fn registry(root: &SkillRoot, p: &Path) -> Result<CapabilityRegistry> {
    // Relative registry paths retain their installation-relative meaning. A caller
    // may explicitly select a project-local snapshot by its absolute path; never
    // silently substitute one registry for another when a project home is bound.
    let path = root.expand(p)?;
    ensure!(
        path.starts_with(root.path()) || path.starts_with(root.data_path()),
        "tool_result_registry_outside_selected_roots"
    );
    oe_core::no_symlinks(&path)?;
    let raw = read_regular_bytes(&path, 4 * 1024 * 1024)?;
    Ok(serde_json::from_value(oe_core::json::strict_json(&raw)?)?)
}

fn project(root: &SkillRoot, p: &Path) -> Result<(Vec<u8>, Project)> {
    let raw = read_regular_bytes(&inside(root, p)?, LIMIT)?;
    let parsed: Project = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    Ok((raw, parsed))
}

/// Returns the JSON result and whether the command outcome is a failure exit.
pub fn run(root: &SkillRoot, command: ToolResultCommand) -> Result<(Value, bool)> {
    let binding = crate::v2::require_execution(root, "tool_result_handback")?;
    let (mut value, failed, output) = match command {
        ToolResultCommand::Verify {
            project: p,
            input,
            registry: g,
            output,
        } => {
            let (_, prj) = project(root, &p)?;
            let reg = registry(root, &g)?;
            let raw = read_regular_bytes(&inside(root, &input)?, LIMIT)?;
            let v = tool_result::verify(root, &prj, &reg, &raw)?;
            let failed = v.check.status != "accepted";
            let mut out = serde_json::to_value(&v.check)?;
            out["evidence_if_registered"] = json!(v.evidence);
            (out, failed, output)
        }
        ToolResultCommand::Register {
            project: p,
            input,
            registry: g,
            revision,
            output,
            receipt,
        } => {
            let (raw_project, _) = project(root, &p)?;
            let reg = registry(root, &g)?;
            let target = inside(root, &output)?;
            ensure!(!target.exists(), "tool_result_project_output_exists");
            let raw = read_regular_bytes(&inside(root, &input)?, LIMIT)?;
            let portable = root.portable_data_path(&target)?;
            let (bytes, registration) =
                tool_result::register(root, &raw_project, &reg, &raw, &revision, &portable)?;
            if let Some(bytes) = &bytes {
                // The original Project must not have changed while the copy was prepared.
                ensure!(
                    sha256(&read_regular_bytes(&inside(root, &p)?, LIMIT)?)
                        == registration.previous_project_sha256,
                    "tool_result_project_changed_during_registration"
                );
                write_new_bytes(&target, bytes)?;
                ensure!(
                    sha256(&read_regular_bytes(&target, LIMIT)?) == registration.project_sha256,
                    "tool_result_project_write_mismatch"
                );
            }
            (
                serde_json::to_value(&registration)?,
                bytes.is_none(),
                receipt,
            )
        }
        ToolResultCommand::Compare {
            project: p,
            input,
            output,
        } => {
            let (_, prj) = project(root, &p)?;
            let raw = read_regular_bytes(&inside(root, &input)?, 1024 * 1024)?;
            let request: ToolComparisonRequest =
                serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
            let report = tool_result::compare(root, &prj, &request)?;
            let failed = report.verdict == "invalid";
            (serde_json::to_value(&report)?, failed, output)
        }
        ToolResultCommand::Registry { registry: g } => {
            let reg = registry(root, &g)?;
            let status = tool_result::registry_status(&reg);
            let failed = status["status"] != "matched";
            (status, failed, None)
        }
    };
    value["execution_binding"] = binding;
    if let Some(p) = &output {
        let target = inside(root, p)?;
        write_new(&target, &value)?;
    }
    Ok((value, failed))
}

/// `project decide`: one decision update from registered results into a new Project copy.
pub fn decide(
    root: &SkillRoot,
    p: &Path,
    input: &Path,
    g: &Path,
    revision: &str,
    output: &Path,
    receipt: Option<&Path>,
) -> Result<(Value, bool)> {
    let binding = crate::v2::require_execution(root, "decision_update")?;
    let (raw_project, _) = project(root, p)?;
    let reg = registry(root, g)?;
    let target = inside(root, output)?;
    ensure!(!target.exists(), "decision_update_project_output_exists");
    let raw = read_regular_bytes(&inside(root, input)?, 4 * 1024 * 1024)?;
    let request: oe_contracts::DecisionUpdate =
        serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let portable = root.portable_data_path(&target)?;
    let (bytes, rec) = oe_kernel::project::decide::decide(
        root,
        &raw_project,
        &reg,
        &request,
        revision,
        &portable,
    )?;
    if let Some(bytes) = &bytes {
        ensure!(
            sha256(&read_regular_bytes(&inside(root, p)?, LIMIT)?) == rec.previous_project_sha256,
            "decision_update_project_changed"
        );
        write_new_bytes(&target, bytes)?;
        ensure!(
            sha256(&read_regular_bytes(&target, LIMIT)?) == rec.project_sha256,
            "decision_update_write_mismatch"
        );
    }
    let mut value = serde_json::to_value(&rec)?;
    value["execution_binding"] = binding;
    if let Some(r) = receipt {
        write_new(&inside(root, r)?, &value)?;
    }
    Ok((value, bytes.is_none()))
}

/// `oev2 project memory-candidate`: engineering-memory recall candidates re-verified against the same Project.
#[derive(Subcommand)]
pub enum MemoryCandidateCommand {
    /// Re-verify a memory handoff: non-adoption flags, record digest, source windows and Project binding.
    Verify {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Register an accepted candidate into a new Project copy as one non-decisive evidence item.
    Register {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        input: PathBuf,
        /// New Project revision identifier.
        #[arg(long)]
        revision: String,
        /// New Project file; must not exist.
        #[arg(long)]
        output: PathBuf,
        /// Registration receipt; must not exist.
        #[arg(long)]
        receipt: Option<PathBuf>,
    },
}

fn artifact(root: &SkillRoot, input: &Path) -> Result<oe_contracts::ArtifactReference> {
    let path = inside(root, input)?;
    let raw = read_regular_bytes(&path, 16 * 1024 * 1024)?;
    Ok(oe_contracts::ArtifactReference {
        path: root.portable_data_path(&path)?,
        sha256: sha256(&raw),
    })
}

pub fn memory_candidate(
    root: &SkillRoot,
    command: MemoryCandidateCommand,
) -> Result<(Value, bool)> {
    let binding = crate::v2::require_execution(root, "memory_candidate")?;
    let (mut value, failed, output) = match command {
        MemoryCandidateCommand::Verify {
            project: p,
            input,
            output,
        } => {
            let (raw, prj) = project(root, &p)?;
            let check = memory::verify(root, &prj, &sha256(&raw), &artifact(root, &input)?)?;
            (
                serde_json::to_value(&check)?,
                check.status != "accepted",
                output,
            )
        }
        MemoryCandidateCommand::Register {
            project: p,
            input,
            revision,
            output,
            receipt,
        } => {
            let (raw, _) = project(root, &p)?;
            let target = inside(root, &output)?;
            ensure!(!target.exists(), "memory_candidate_project_output_exists");
            let portable = root.portable_data_path(&target)?;
            let (bytes, check) = memory::register(root, &raw, &artifact(root, &input)?, &revision)?;
            if let Some(bytes) = &bytes {
                ensure!(
                    sha256(&read_regular_bytes(&inside(root, &p)?, LIMIT)?) == sha256(&raw),
                    "memory_candidate_project_changed_during_registration"
                );
                write_new_bytes(&target, bytes)?;
                ensure!(
                    sha256(&read_regular_bytes(&target, LIMIT)?) == check.project_sha256,
                    "memory_candidate_project_write_mismatch"
                );
            }
            let mut v = serde_json::to_value(&check)?;
            v["project_output"] = json!(bytes.as_ref().map(|_| portable));
            (v, bytes.is_none(), receipt)
        }
    };
    value["execution_binding"] = binding;
    if let Some(o) = output {
        write_new(&inside(root, &o)?, &value)?;
    }
    Ok((value, failed))
}

/// Arguments of `oev2 project memory-feedback`.
pub struct MemoryFeedbackArgs {
    pub project: PathBuf,
    pub input: PathBuf,
    pub id: String,
    pub outcome: String,
    pub result: String,
    pub sources: Option<PathBuf>,
    pub elapsed_ms: Option<u64>,
    pub unknowns: Vec<String>,
    pub context_id: Option<String>,
    pub context_sha256: Option<String>,
    pub output: PathBuf,
}

/// Usage feedback for a registered candidate, written in memory's own session-feedback/1.
pub fn memory_feedback(root: &SkillRoot, a: MemoryFeedbackArgs) -> Result<(Value, bool)> {
    let binding = crate::v2::require_execution(root, "memory_feedback")?;
    let path = inside(root, &a.project)?;
    let (raw, _) = project(root, &a.project)?;
    let sources: Vec<Value> = match &a.sources {
        Some(s) => serde_json::from_value(oe_core::json::strict_json(&read_regular_bytes(
            &inside(root, s)?,
            16 * 1024 * 1024,
        )?)?)?,
        None => vec![],
    };
    let context = match (a.context_id, a.context_sha256) {
        (Some(i), Some(s)) => Some((i, s)),
        (None, None) => None,
        _ => anyhow::bail!("memory_feedback_context_incomplete"),
    };
    let request = memory::FeedbackRequest {
        id: a.id,
        outcome: a.outcome,
        result: a.result,
        sources,
        elapsed_ms: a.elapsed_ms,
        unknowns: a.unknowns,
        context,
    };
    let feedback = memory::feedback(root, &path, &raw, &artifact(root, &a.input)?, &request)?;
    let target = inside(root, &a.output)?;
    write_new(&target, &feedback)?;
    Ok((
        json!({"status": "written", "output": root.portable_data_path(&target)?,
               "sha256": sha256(&read_regular_bytes(&target, LIMIT)?), "feedback": feedback,
               "next": "engineering-memory: engineering session feedback --input <output> (memory re-checks exposure, revision and Project binding)",
               "execution_binding": binding}),
        false,
    ))
}

/// Project records as memory organization artifacts `{id, path, sha256}`.
pub fn memory_export(root: &SkillRoot, p: &Path, output: &Path) -> Result<(Value, bool)> {
    let binding = crate::v2::require_execution(root, "memory_export")?;
    let path = inside(root, p)?;
    let (raw, _) = project(root, p)?;
    let exported = if root.data_path() == root.path() {
        memory::export(&path, &raw)?
    } else {
        memory::export_in(root.data_path(), &path, &raw)?
    };
    let target = inside(root, output)?;
    write_new(&target, &exported["artifacts"])?;
    let missing = exported["missing"].as_array().map_or(0, Vec::len);
    Ok((
        json!({"status": if missing == 0 { "exported" } else { "exported_with_missing" },
               "artifacts": exported["artifacts"].as_array().map_or(0, Vec::len), "missing": exported["missing"],
               "output": root.portable_data_path(&target)?, "execution_binding": binding}),
        missing != 0,
    ))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn project_artifacts_and_machine_registry_have_distinct_explicit_roots() {
        let install = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let install = std::fs::canonicalize(install.path()).unwrap();
        let home = std::fs::canonicalize(home.path()).unwrap();
        std::fs::write(install.join("SKILL.md"), "path fixture").unwrap();
        std::fs::create_dir_all(install.join("var/capabilities")).unwrap();
        let bytes = br#"{"schema":"oev2.capability-registry/1","capabilities":[]}"#;
        std::fs::write(install.join(tool_result::DEFAULT_REGISTRY), bytes).unwrap();
        std::fs::create_dir_all(home.join("var/capabilities")).unwrap();
        std::fs::write(
            home.join(tool_result::DEFAULT_REGISTRY),
            b"not the selected registry",
        )
        .unwrap();
        let legacy = SkillRoot::new(&install).unwrap();
        let bound = legacy.clone().with_project_data_root(&home).unwrap();
        assert_eq!(
            inside(&legacy, Path::new("project.json")).unwrap(),
            install.join("project.json")
        );
        assert_eq!(
            inside(&bound, Path::new("project.json")).unwrap(),
            home.join("project.json")
        );
        assert!(inside(&bound, &install.join("project.json")).is_err());
        assert!(inside(&bound, Path::new("../escaped.json")).is_err());
        assert!(registry(&bound, Path::new(tool_result::DEFAULT_REGISTRY)).is_ok());
        std::fs::write(home.join("registry.json"), bytes).unwrap();
        assert!(registry(&bound, &home.join("registry.json")).is_ok());
        let foreign = std::fs::canonicalize(outside.path())
            .unwrap()
            .join("registry.json");
        std::fs::write(&foreign, bytes).unwrap();
        assert!(registry(&bound, &foreign).is_err());
    }
}
