//! Native task transport; a completed process is separate from engineering acceptance.
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use oe_cad::nx::{
    self, InputBinding, PrepareRequest,
    client::{Client, DispatchUncertain},
    transport::RemoteProfile,
};
use oe_core::SkillRoot;
use oe_nx_protocol::{
    JobId, MAX_RECORD_BYTES, RelativePath, State, StateRecord, StatusObservation,
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
mod component_copy;
mod construction;
mod correspondence;
mod evidence;
mod feature_add;
mod feature_delete;
mod instances;
mod motion_receiving;
mod occurrence;
mod variant;

#[derive(Subcommand)]
pub enum CadCommand {
    /// SolidWorks third-party source evidence; Rust native execution remains separate.
    Solidworks {
        #[command(subcommand)]
        command: SolidWorksCommand,
    },
    /// Parameter intent, mating axes and current Project change dependencies.
    Variant {
        #[command(subcommand)]
        command: variant::VariantCommand,
    },
    /// Complete sheet/PDF/issue delivery joins; native and visual scopes separate.
    Drawing {
        #[command(subcommand)]
        command: variant::DrawingCommand,
    },
    /// Source-bound motion contracts and versioned historical projections.
    Motion {
        #[command(subcommand)]
        command: variant::MotionCommand,
    },
    /// Verify and project source-bound CAD evidence; no semantic execution.
    Evidence {
        #[command(subcommand)]
        command: evidence::EvidenceCommand,
    },
    /// Project CAD/process questions, dependencies and authoring views.
    Handoff {
        #[command(subcommand)]
        command: evidence::HandoffCommand,
    },
    /// Source-bound Windows NX jobs. Unknown dispatches are never automatically retried.
    Nx {
        #[command(subcommand)]
        command: NxCommand,
    },
}

#[derive(Subcommand)]
pub enum SolidWorksCommand {
    /// Show actual implemented actions and unqualified native scope.
    Capabilities,
    /// Verify a source-bound existing CAD packet without connecting to SolidWorks.
    Verify {
        #[arg(long)]
        input: PathBuf,
    },
    /// Check original Rust worker record consistency; native authenticity and engineering adoption remain separate.
    VerifyNativeRecord {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        observation: PathBuf,
    },
    /// Check two original records for distinct owned CAD processes and the same saved file.
    VerifyNativeReopen {
        #[arg(long)]
        saved_request: PathBuf,
        #[arg(long)]
        saved_observation: PathBuf,
        #[arg(long)]
        reopen_request: PathBuf,
        #[arg(long)]
        reopen_observation: PathBuf,
    },
}

#[derive(Args)]
pub struct Connection {
    /// Explicit Fleet/SSH route, installed controller digest and expected root identity.
    #[arg(long)]
    profile: PathBuf,
    /// New private directory beneath the selected skill's var/.
    #[arg(long)]
    output: PathBuf,
}

#[derive(Subcommand)]
pub enum NxCommand {
    /// Reverify local job bytes against a frozen fetch snapshot; no remote call.
    VerifyFetched {
        #[arg(long)]
        snapshot: PathBuf,
        #[arg(long)]
        directory: PathBuf,
    },
    /// Retrieve an entire observed job in bounded chunks and verify full files.
    FetchLarge {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        job_id: String,
        #[arg(long)]
        manifest_digest: String,
        #[arg(long)]
        destination: PathBuf,
        #[arg(long, default_value_t = 67108864)]
        chunk_bytes: u64,
    },
    /// Prepare, inspect or explicitly recover a complete published-source job.
    StagePublished {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        request: PathBuf,
    },
    /// Perform one versioned transfer-only operation; never run an NX job.
    Transfer {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        payload: Option<PathBuf>,
    },
    /// Inspect a local worker package without a remote profile or native execution.
    Worker {
        #[command(subcommand)]
        command: NxWorkerCommand,
    },
    /// Observe the installed controller and remote directory identity; no native job.
    Probe {
        #[command(flatten)]
        connection: Connection,
    },
    /// Validate and package an explicit worker, parameters and inputs locally.
    Prepare {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        request: PathBuf,
    },
    /// Transfer a prepared job once; does not run the worker.
    Stage {
        #[command(flatten)]
        connection: Connection,
        /// The prepared output's job/ directory, retaining every original member.
        #[arg(long)]
        job_directory: PathBuf,
    },
    /// Dispatch an unused staged job once. Exit 3 means pending or unknown.
    Run {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        job_directory: PathBuf,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u16).range(1..=300))]
        wait_seconds: u16,
    },
    /// Observe/reconcile the same job from existing evidence; never redispatch.
    Status {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        job_id: String,
    },
    /// Verify and retain original result/archive bytes, including unknown/legacy state.
    Fetch {
        #[command(flatten)]
        connection: Connection,
        #[arg(long)]
        job_id: String,
        /// Another new private directory for the verified archive and extracted bytes.
        #[arg(long)]
        destination: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum NxWorkerCommand {
    /// Produce source-bound stepped-shaft parameters locally; no CAD connection.
    SteppedShaftPlan {
        #[arg(long)]
        intent: PathBuf,
        #[arg(long)]
        input_member: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Produce source-bound parameters for one controlled new cylinder; no connection.
    CylinderPlan {
        #[arg(long)]
        intent: PathBuf,
        #[arg(long)]
        input_member: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Check manifest, exact file closure and hashes; build/native claims remain unevaluated.
    Verify {
        #[arg(long)]
        worker_directory: PathBuf,
    },
    /// Check current native role/path/declared Windows target offline; no native qualification.
    VerifyCurrentRole {
        #[arg(long)]
        worker_directory: PathBuf,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preparation {
    worker_directory: PathBuf,
    parameter_file: PathBuf,
    inputs: Vec<Input>,
    job_id: JobId,
    action: String,
    parameter_schema: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    source: PathBuf,
    member: RelativePath,
}

fn input<T: DeserializeOwned>(root: &SkillRoot, path: &Path) -> Result<T> {
    let path = root.expand(path)?;
    ensure!(path.starts_with(root.path()), "NX input outside skill root");
    oe_core::no_symlinks(&path)?;
    oe_nx_protocol::decode(&oe_core::read_regular_bytes(&path, MAX_RECORD_BYTES)?)
}

fn dispatch_exit(state: Option<State>) -> u8 {
    match state {
        Some(State::Completed) => 0,
        Some(State::Failed) => 1,
        _ => 3,
    }
}

pub fn run(root: &SkillRoot, command: CadCommand) -> Result<(Value, u8)> {
    let command = match command {
        CadCommand::Solidworks { command } => {
            let value = match command {
                SolidWorksCommand::Capabilities => oe_cad::solidworks::capabilities(),
                SolidWorksCommand::VerifyNativeReopen {
                    saved_request,
                    saved_observation,
                    reopen_request,
                    reopen_observation,
                } => {
                    let binding =
                        crate::v2::require_execution(root, "solidworks_native_reopen_records")?;
                    let paths = [
                        saved_request,
                        saved_observation,
                        reopen_request,
                        reopen_observation,
                    ]
                    .map(|p| root.expand(&p));
                    let paths = paths.into_iter().collect::<Result<Vec<_>>>()?;
                    ensure!(
                        paths.iter().all(|p| p.starts_with(root.path().join("var"))),
                        "solidworks_private_records_required"
                    );
                    let raw = paths
                        .iter()
                        .map(|p| oe_core::read_regular_bytes(p, 4 * 1024 * 1024))
                        .collect::<Result<Vec<_>>>()?;
                    let mut result = oe_cad::solidworks::verify_native_reopen([
                        &raw[0], &raw[1], &raw[2], &raw[3],
                    ])?;
                    for (p, r) in paths.iter().zip(&raw) {
                        ensure!(
                            oe_core::read_regular_bytes(p, 4 * 1024 * 1024)? == *r,
                            "solidworks_native_record_drift"
                        );
                    }
                    result["execution_binding"] = binding;
                    result
                }
                SolidWorksCommand::VerifyNativeRecord {
                    request,
                    observation,
                } => {
                    let binding = crate::v2::require_execution(root, "solidworks_native_record")?;
                    let request = root.expand(&request)?;
                    let observation = root.expand(&observation)?;
                    ensure!(
                        [&request, &observation]
                            .iter()
                            .all(|p| p.starts_with(root.path().join("var"))),
                        "solidworks_private_records_required"
                    );
                    let r = oe_core::read_regular_bytes(&request, 4 * 1024 * 1024)?;
                    let o = oe_core::read_regular_bytes(&observation, 4 * 1024 * 1024)?;
                    let mut result = oe_cad::solidworks::verify_native_record(&r, &o)?;
                    ensure!(
                        oe_core::read_regular_bytes(&request, 4 * 1024 * 1024)? == r
                            && oe_core::read_regular_bytes(&observation, 4 * 1024 * 1024)? == o,
                        "solidworks_native_record_drift"
                    );
                    result["execution_binding"] = binding;
                    result
                }
                SolidWorksCommand::Verify { input } => {
                    let binding =
                        crate::v2::require_execution(root, "solidworks_external_evidence")?;
                    let input = root.expand(&input)?;
                    ensure!(
                        input.starts_with(root.path()),
                        "solidworks_request_outside_root"
                    );
                    let raw = oe_core::read_regular_bytes(&input, 16 * 1024 * 1024)?;
                    let request = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
                    let mut result = oe_cad::solidworks::verify(root, &request)?;
                    ensure!(
                        oe_core::read_regular_bytes(&input, 16 * 1024 * 1024)? == raw,
                        "solidworks_request_changed"
                    );
                    result["execution_binding"] = binding;
                    result
                }
            };
            return Ok((value, 0));
        }
        CadCommand::Variant { command } => return variant::run(root, command),
        CadCommand::Drawing { command } => return variant::run_drawing(root, command),
        CadCommand::Motion { command } => return variant::run_motion(root, command),
        CadCommand::Evidence { command } => return evidence::run_evidence(root, command),
        CadCommand::Handoff { command } => return evidence::run_handoff(root, command),
        CadCommand::Nx { command } => command,
    };
    let connection = match &command {
        NxCommand::VerifyFetched {
            snapshot,
            directory,
        } => {
            let snapshot_path = root.expand(&snapshot)?;
            let directory = root.expand(&directory)?;
            let bytes = oe_core::read_regular_bytes(
                &snapshot_path,
                oe_nx_protocol::large_fetch::MAX_SNAPSHOT_BYTES,
            )?;
            let snapshot = oe_nx_protocol::large_fetch::FetchSnapshot::decode(&bytes)?;
            nx::client::verify_received_tree(&directory, &snapshot)?;
            return Ok((
                serde_json::json!({"snapshot_digest":snapshot.digest()?,"qualification":"verified_job_bytes_only","source_current_state":"not_observed_by_local_verify"}),
                0,
            ));
        }

        NxCommand::Worker {
            command:
                NxWorkerCommand::SteppedShaftPlan {
                    intent,
                    input_member,
                    output,
                },
        } => {
            let report = oe_cad::stepped_shaft::plan(root, intent, input_member)?;
            if let Some(path) = output {
                oe_core::write_new(&root.private_path(path)?, &report["parameters"])?;
            }
            return Ok((report, 0));
        }
        NxCommand::Worker {
            command:
                NxWorkerCommand::CylinderPlan {
                    intent,
                    input_member,
                    output,
                },
        } => {
            let report = oe_cad::rotating_part::plan(root, intent, input_member)?;
            if let Some(path) = output {
                oe_core::write_new(&root.private_path(path)?, &report["parameters"])?;
            }
            return Ok((report, 0));
        }
        NxCommand::Worker {
            command: NxWorkerCommand::Verify { worker_directory },
        } => {
            return Ok((
                serde_json::to_value(nx::verify_worker(root, worker_directory)?)?,
                0,
            ));
        }
        NxCommand::Worker {
            command: NxWorkerCommand::VerifyCurrentRole { worker_directory },
        } => {
            return Ok((nx::verify_current_worker_role(root, worker_directory)?, 0));
        }
        NxCommand::FetchLarge { connection, .. }
        | NxCommand::StagePublished { connection, .. }
        | NxCommand::Transfer { connection, .. }
        | NxCommand::Probe { connection }
        | NxCommand::Prepare { connection, .. }
        | NxCommand::Stage { connection, .. }
        | NxCommand::Run { connection, .. }
        | NxCommand::Status { connection, .. }
        | NxCommand::Fetch { connection, .. } => connection,
    };
    let profile: RemoteProfile = input(root, &connection.profile)?;
    profile.validate()?;
    let client = Client {
        root,
        profile: &profile,
    };
    let mut code = 0;
    let value = match command {
        NxCommand::FetchLarge {
            connection,
            job_id,
            manifest_digest,
            destination,
            chunk_bytes,
        } => {
            let result = client.fetch_large(
                &oe_nx_protocol::JobId::new(job_id)?,
                &oe_nx_protocol::Digest::new(manifest_digest)?,
                &connection.output,
                &destination,
                chunk_bytes,
            )?;
            serde_json::json!({"directory":result,"qualification":"verified_job_bytes_only"})
        }
        NxCommand::StagePublished {
            connection,
            request,
        } => {
            let request: oe_nx_protocol::published_stage::PublishedStageRequest =
                input(root, &request)?;
            serde_json::to_value(client.stage_published(&request, &connection.output)?)?
        }
        NxCommand::Transfer {
            connection,
            request,
            payload,
        } => {
            let request: oe_nx_protocol::Request = input(root, &request)?;
            oe_nx_protocol::transfer::validate_request(&request)?;
            let body = if let Some(path) = payload {
                let path = root.expand(&path)?;
                ensure!(
                    path.starts_with(root.path()),
                    "NX transfer payload outside skill root"
                );
                oe_core::no_symlinks(&path)?;
                oe_core::read_regular_bytes(&path, oe_nx_protocol::MAX_PAYLOAD_BYTES as usize)?
            } else {
                Vec::new()
            };
            serde_json::to_value(client.transfer(&request, &body, &connection.output)?)?
        }
        NxCommand::VerifyFetched { .. } => {
            unreachable!("local verification returned before transport")
        }
        NxCommand::Worker { .. } => {
            unreachable!("local worker command returned before transport setup")
        }
        NxCommand::Probe { connection } => serde_json::to_value(client.probe(&connection.output)?)?,
        NxCommand::Prepare {
            connection,
            request,
        } => {
            let spec: Preparation = input(root, &request)?;
            let package = nx::prepare(
                root,
                &PrepareRequest {
                    worker_directory: spec.worker_directory,
                    parameter_file: spec.parameter_file,
                    inputs: spec
                        .inputs
                        .into_iter()
                        .map(|v| InputBinding {
                            source: v.source,
                            member: v.member,
                        })
                        .collect(),
                    job_id: spec.job_id,
                    root_identity: profile
                        .expected_root
                        .clone()
                        .context("probe and pin remote root before preparation")?,
                    action: spec.action,
                    parameter_schema: spec.parameter_schema,
                },
            )?;
            let directory = nx::publish(&package, root, &connection.output)?;
            json!({"schema":"cad-agent.nx-local-preparation/v2","job_id":package.job.job_id,
                "manifest_digest":package.job.digest()?,"output":directory,"native_execution":false})
        }
        NxCommand::Stage {
            connection,
            job_directory,
        } => {
            let package = nx::from_directory(root, &job_directory)?;
            serde_json::to_value(client.stage(&package, &connection.output)?)?
        }
        NxCommand::Run {
            connection,
            job_directory,
            wait_seconds,
        } => {
            let job = nx::dispatch_job(root, &job_directory)?;
            let observation = match client.run(&job, wait_seconds, &connection.output) {
                Ok(observation) => observation,
                Err(error) => {
                    if let Some(unknown) = error.downcast_ref::<DispatchUncertain>() {
                        return Ok((
                            json!({
                                "schema":"cad-agent.nx-local-dispatch-observation/v2",
                                "scope":"local_transport_observation",
                                "status":"dispatch_result_unknown",
                                "job_id":unknown.job_id,
                                "manifest_digest":unknown.manifest_digest,
                                "evidence":unknown.evidence,
                                "reason":unknown.reason,
                                "remote_state":null,
                                "automatic_retry":false,
                                "next_step":"query_same_job_status",
                                "engineering_acceptance":"not_evaluated"
                            }),
                            3,
                        ));
                    }
                    return Err(error);
                }
            };
            code = match &observation {
                StatusObservation::Unknown(_) => dispatch_exit(None),
                StatusObservation::Known(known) => {
                    let state: StateRecord = serde_json::from_value(known.state.clone())?;
                    dispatch_exit(Some(state.state))
                }
            };
            serde_json::to_value(observation)?
        }
        NxCommand::Status { connection, job_id } => {
            serde_json::to_value(client.status(&JobId::new(job_id)?, &connection.output)?)?
        }
        NxCommand::Fetch {
            connection,
            job_id,
            destination,
        } => {
            let directory = client.fetch(&JobId::new(job_id)?, &connection.output, &destination)?;
            json!({"schema":"cad-agent.nx-local-fetch/v2","output":directory,
                "engineering_acceptance":"not_evaluated"})
        }
    };
    Ok((value, code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_transport_never_promotes_incomplete_or_unknown_dispatch() {
        for state in [
            None,
            Some(State::Prepared),
            Some(State::Staged),
            Some(State::Starting),
            Some(State::Running),
            Some(State::UnknownAfterDispatch),
        ] {
            assert_eq!(dispatch_exit(state), 3, "{state:?}");
        }
        assert_eq!(dispatch_exit(Some(State::Completed)), 0);
        assert_eq!(dispatch_exit(Some(State::Failed)), 1);
    }
}
