//! CAD observation to visual projection command.
//!
//! This command only consumes source-bound Rust/NX observation JSON. It does
//! not start NX or a legacy cad-agent process. The output is private `var/`
//! evidence and may be handed to the existing presentation/Anatomy renderer.
use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_contracts::{
    CadPresentationProjectionRequest1, NxAssemblyObservation, NxAssemblyOrderedScene,
};
use oe_core::{SkillRoot, no_symlinks, read_regular_bytes, sha256, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const INPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Subcommand)]
pub enum CadPresentationCommand {
    /// Project one verified observation into CAD provenance and a bounded renderer proxy.
    Project {
        /// Request containing Project/object/condition and the native observation artifact reference.
        #[arg(long)]
        request: PathBuf,
        /// Complete NxAssemblyObservation JSON, normally produced by CAD evidence inspection.
        #[arg(long)]
        observation: PathBuf,
        /// Optional NxAssemblyOrderedScene JSON. Ordinals remain discrete source samples, never time.
        #[arg(long)]
        ordered_scene: Option<PathBuf>,
        /// New private output directory beneath this installation's var/.
        #[arg(long)]
        output: PathBuf,
    },
}

fn private_input(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.expand(path)?;
    ensure!(
        path.starts_with(root.path().join("var")),
        "cad_projection_private_input_required"
    );
    no_symlinks(&path)?;
    Ok(path)
}

fn decode<T: serde::de::DeserializeOwned>(path: &Path) -> Result<(Vec<u8>, T)> {
    let bytes = read_regular_bytes(path, INPUT_BYTES)?;
    let value = serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
    Ok((bytes, value))
}

fn decode_observation(path: &Path) -> Result<(Vec<u8>, NxAssemblyObservation)> {
    let bytes = read_regular_bytes(path, INPUT_BYTES)?;
    let value = oe_core::json::strict_json(&bytes)?;
    // A direct worker observation is accepted for development fixtures. The
    // production occurrence report wraps the same observation at
    // /assembly/assembly and keeps its own report SHA as the source identity.
    // Require the explicit outer schema when consuming the report so an
    // arbitrary nested JSON object cannot masquerade as a verified handoff.
    let candidate = match value.get("schema").and_then(Value::as_str) {
        Some("oev2.nx-native-assembly-observation/1") => &value,
        Some("oev2.cad-occurrence-binding-report/1") => value
            .pointer("/assembly/assembly")
            .ok_or_else(|| anyhow::anyhow!("cad_projection_report_assembly_missing"))?,
        Some(_) => anyhow::bail!("cad_projection_observation_schema_invalid"),
        None => anyhow::bail!("cad_projection_observation_schema_missing"),
    };
    let observation = serde_json::from_value(candidate.clone())?;
    Ok((bytes, observation))
}

fn artifact_matches(
    root: &SkillRoot,
    reference: &oe_contracts::ArtifactReference,
    path: &Path,
    bytes: &[u8],
) -> Result<()> {
    let relative = oe_core::portable_source_path(root.path(), path)?;
    ensure!(
        relative == reference.path,
        "cad_projection_source_path_mismatch"
    );
    ensure!(
        sha256(bytes) == reference.sha256,
        "cad_projection_source_digest_mismatch"
    );
    Ok(())
}

pub fn run(root: &SkillRoot, command: CadPresentationCommand) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_presentation_projection")?;
    match command {
        CadPresentationCommand::Project {
            request,
            observation,
            ordered_scene,
            output,
        } => {
            let request_path = private_input(root, &request)?;
            let observation_path = private_input(root, &observation)?;
            let (request_bytes, request_value): (Vec<u8>, CadPresentationProjectionRequest1) =
                decode(&request_path)?;
            let (observation_bytes, observation_value) = decode_observation(&observation_path)?;
            artifact_matches(
                root,
                &request_value.native_observation,
                &observation_path,
                &observation_bytes,
            )?;

            let mut ordered_bytes_out = None;
            let ordered = if let Some(path) = ordered_scene {
                let ordered_path = private_input(root, &path)?;
                let (ordered_bytes, ordered_value): (Vec<u8>, NxAssemblyOrderedScene) =
                    decode(&ordered_path)?;
                let reference = request_value
                    .ordered_scene
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("cad_projection_ordered_scene_not_declared"))?;
                artifact_matches(root, reference, &ordered_path, &ordered_bytes)?;
                ordered_bytes_out = Some(ordered_bytes);
                Some(ordered_value)
            } else {
                ensure!(
                    request_value.ordered_scene.is_none(),
                    "cad_projection_ordered_scene_missing"
                );
                None
            };

            let projection = oe_geometry::cad_projection::project(
                request_value,
                &observation_value,
                ordered.as_ref(),
            )?;
            if let Some(scene) = &projection.renderer_proxy {
                oe_geometry::presentation::evaluate(scene)?;
            }
            let out = root.new_private_dir(&output)?;
            write_new_bytes(&out.join("request.json"), &request_bytes)?;
            write_new_bytes(&out.join("observation.json"), &observation_bytes)?;
            if let Some(bytes) = ordered_bytes_out {
                write_new_bytes(&out.join("ordered-scene.json"), &bytes)?;
            }
            let projection_value = serde_json::to_value(&projection)?;
            write_new(&out.join("projection.json"), &projection_value)?;
            if let Some(scene) = &projection.renderer_proxy {
                write_new(&out.join("source.json"), &json!({"scene": scene}))?;
            }
            let projection_bytes = read_regular_bytes(&out.join("projection.json"), INPUT_BYTES)?;
            let receipt = json!({
                "schema": "oev2.cad-presentation-projection-receipt/1",
                "request_sha256": sha256(&request_bytes),
                "native_observation_sha256": sha256(&observation_bytes),
                "projection_sha256": sha256(&projection_bytes),
                "projection": "cad_provenance_plus_analytic_renderer_proxy",
                "renderer_source": if projection.renderer_proxy.is_some() { "source.json" } else { "not_available" },
                "engineering_adoption": "not_assessed",
                "execution_binding": execution.clone(),
                "output": out,
            });
            write_new(&out.join("receipt.json"), &receipt)?;
            Ok((
                json!({
                    "status": "projected",
                    "directory": out,
                    "projection": "projection.json",
                    "renderer_source": if projection.renderer_proxy.is_some() { Some("source.json") } else { None::<&str> },
                    "receipt": "receipt.json",
                    "engineering_adoption": "not_assessed",
                    "execution_binding": execution,
                }),
                0,
            ))
        }
    }
}
