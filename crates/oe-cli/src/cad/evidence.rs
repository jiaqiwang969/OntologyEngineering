//! Local source projections. No Python, NX, transport or semantic executor.
use anyhow::{Result, ensure};
use clap::{Args, Subcommand};
use oe_cad::{evidence, handoff};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Args)]
pub struct Input {
    #[arg(long)]
    packet: PathBuf,
    /// Source paths in the packet are relative to this directory within the skill.
    #[arg(long)]
    project_root: PathBuf,
}
#[derive(Args)]
pub struct ProjectionOptions {
    #[command(flatten)]
    input: Input,
    /// Optional earlier packet, verified against the same project source archive.
    #[arg(long)]
    previous_packet: Option<PathBuf>,
    /// A new directory beneath this skill's var/; existing output is refused.
    #[arg(long)]
    output: PathBuf,
}
#[derive(Subcommand)]
pub enum EvidenceCommand {
    /// Receive actual source-bound hole addition and independent saved reinspection.
    VerifyAddition {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Receive actual target-feature deletion, independent reopen and Project impact.
    VerifyDeletion {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Bind a recorded native copy to both versions and an independent saved reopen.
    VerifyCopy {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Bind native construction/save evidence to an independent current assembly inspection.
    VerifyConstruction {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Explicit source-bound cross-version assembly instance correspondence.
    CorrespondInstances {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Bind prototype geometry to source-backed current assembly instances.
    BindOccurrence {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Check source-bound feature correspondence and reuse Project change review.
    Correspond {
        #[arg(long)]
        input: PathBuf,
        /// New private directory for original input and checked report.
        #[arg(long)]
        output: PathBuf,
    },
    /// Check source bytes, IDs and declared native observations without writing.
    Verify {
        #[command(flatten)]
        input: Input,
    },
    /// Freeze a packet, source audit, ABox and optional dependency impact.
    Project {
        #[command(flatten)]
        request: ProjectionOptions,
    },
}
#[derive(Subcommand)]
pub enum HandoffCommand {
    /// Check source-bound CAD/process records without writing.
    Verify {
        #[command(flatten)]
        input: Input,
    },
    /// Freeze a handoff, ABox, bidirectional requests and optional impact.
    Project {
        #[command(flatten)]
        request: ProjectionOptions,
    },
    /// Project a named claim into the proposed Q21 authoring vocabulary.
    Q21 {
        #[command(flatten)]
        input: Input,
        #[arg(long)]
        claim_id: String,
        #[arg(long)]
        output: PathBuf,
    },
}

fn inside(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.expand(path)?;
    ensure!(
        path.starts_with(root.path()),
        "CAD evidence input outside skill root"
    );
    oe_core::no_symlinks(&path)?;
    Ok(path)
}
fn paths(root: &SkillRoot, input: &Input) -> Result<(PathBuf, PathBuf)> {
    Ok((
        inside(root, &input.packet)?,
        inside(root, &input.project_root)?,
    ))
}
fn snapshot(path: &Path, audit: &Value) -> Result<Vec<u8>> {
    let bytes = read_regular_bytes(path, 16 * 1024 * 1024)?;
    ensure!(
        audit["packet_sha256"] == sha256(&bytes),
        "packet changed after verification"
    );
    Ok(bytes)
}
fn finish(
    output: &Path,
    packet: &[u8],
    graph: &[u8],
    mut audit: Value,
    name: &str,
) -> Result<Value> {
    let graph_path = output.join(name);
    let audit_path = output.join("integrity.json");
    audit["abox_path"] = json!(graph_path);
    audit["abox_sha256"] = json!(sha256(graph));
    write_new_bytes(&output.join("packet.json"), packet)?;
    write_new_bytes(&graph_path, graph)?;
    write_new(&audit_path, &audit)?;
    Ok(
        json!({"status":"projected","packet_sha256":audit["packet_sha256"],
        "abox_path":graph_path,"audit_path":audit_path,"semantic_execution":"not_run",
        "engineering_verdict":"not_assessed"}),
    )
}

pub fn run_evidence(root: &SkillRoot, command: EvidenceCommand) -> Result<(Value, u8)> {
    let value = match command {
        EvidenceCommand::VerifyAddition { input, output } => {
            return super::feature_add::run(root, &input, &output);
        }
        EvidenceCommand::VerifyDeletion { input, output } => {
            return super::feature_delete::run(root, &input, &output);
        }
        EvidenceCommand::VerifyCopy { input, output } => {
            return super::component_copy::run(root, &input, &output);
        }
        EvidenceCommand::VerifyConstruction { input, output } => {
            return super::construction::run(root, &input, &output);
        }
        EvidenceCommand::CorrespondInstances { input, output } => {
            return super::instances::run(root, &input, &output);
        }
        EvidenceCommand::BindOccurrence { input, output } => {
            return super::occurrence::run(root, &input, &output);
        }
        EvidenceCommand::Correspond { input, output } => {
            return super::correspondence::run(root, &input, &output);
        }
        EvidenceCommand::Verify { input } => {
            let (packet, project) = paths(root, &input)?;
            evidence::verify(&packet, &project)?.audit().clone()
        }
        EvidenceCommand::Project { request } => {
            let (packet, project) = paths(root, &request.input)?;
            let verified = evidence::verify(&packet, &project)?;
            let bytes = snapshot(&packet, verified.audit())?;
            let previous = request
                .previous_packet
                .as_ref()
                .map(|p| evidence::verify(&inside(root, p)?, &project))
                .transpose()?;
            let impact = previous
                .as_ref()
                .map(|p| evidence::compare_packets(p.packet(), verified.packet()))
                .transpose()?;
            let graph = verified.project_abox();
            let output = root.new_private_dir(&request.output)?;
            let mut audit = verified.audit().clone();
            if let (Some(mut impact), Some(previous)) = (impact, previous) {
                impact["previous_packet_sha256"] = previous.audit()["packet_sha256"].clone();
                impact["current_packet_sha256"] = audit["packet_sha256"].clone();
                let path = output.join("impact.json");
                write_new(&path, &impact)?;
                audit["impact_path"] = json!(path);
            }
            finish(&output, &bytes, &graph, audit, "evidence.nt")?
        }
    };
    Ok((value, 0))
}

pub fn run_handoff(root: &SkillRoot, command: HandoffCommand) -> Result<(Value, u8)> {
    let value = match command {
        HandoffCommand::Verify { input } => {
            let (packet, project) = paths(root, &input)?;
            handoff::verify(&packet, &project)?.audit().clone()
        }
        HandoffCommand::Project { request } => {
            let (packet, project) = paths(root, &request.input)?;
            let verified = handoff::verify(&packet, &project)?;
            let bytes = snapshot(&packet, verified.audit())?;
            let previous = request
                .previous_packet
                .as_ref()
                .map(|p| handoff::verify(&inside(root, p)?, &project))
                .transpose()?;
            let impact = previous
                .as_ref()
                .map(|p| handoff::compare_packets(p.packet(), verified.packet()))
                .transpose()?;
            let graph = verified.project_abox();
            let cad = verified.requests(handoff::QuestionDirection::ProcessToCad);
            let process = verified.requests(handoff::QuestionDirection::CadToProcess);
            let output = root.new_private_dir(&request.output)?;
            let mut audit = verified.audit().clone();
            let cad_path = output.join("cad-requests.json");
            let process_path = output.join("process-requests.json");
            write_new(&cad_path, &cad)?;
            write_new(&process_path, &process)?;
            audit["cad_requests_path"] = json!(cad_path);
            audit["process_requests_path"] = json!(process_path);
            if let (Some(mut impact), Some(previous)) = (impact, previous) {
                impact["previous_packet_sha256"] = previous.audit()["packet_sha256"].clone();
                impact["current_packet_sha256"] = audit["packet_sha256"].clone();
                let path = output.join("impact.json");
                write_new(&path, &impact)?;
                audit["impact_path"] = json!(path);
            }
            finish(&output, &bytes, &graph, audit, "handoff.nt")?
        }
        HandoffCommand::Q21 {
            input,
            claim_id,
            output,
        } => {
            let (packet, project) = paths(root, &input)?;
            let verified = handoff::verify(&packet, &project)?;
            let bytes = snapshot(&packet, verified.audit())?;
            let projection = verified.project_q21(&claim_id)?;
            let output = root.new_private_dir(&output)?;
            let view = output.join("q21.ttl");
            let audit_path = output.join("q21.json");
            let mut audit = projection.audit;
            audit["view_path"] = json!(view);
            write_new_bytes(&output.join("packet.json"), &bytes)?;
            write_new(&output.join("source-integrity.json"), verified.audit())?;
            write_new_bytes(&view, &projection.abox)?;
            write_new(&audit_path, &audit)?;
            json!({"status":"projected_for_authoring_only","view_path":view,
                "audit_path":audit_path,"semantic_execution":"not_run","engineering_verdict":"not_assessed"})
        }
    };
    Ok((value, 0))
}
