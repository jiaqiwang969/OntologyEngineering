//! Application-owned private selection; no default credentials, policies or releases.
use anyhow::{Result, ensure};
use clap::Subcommand;
use oe_core::{SkillRoot, write_new};
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum VersionCommand {
    /// Let the existing authority supervise a pre-bound real Rust build.
    SuperviseBuild {
        #[arg(long)]
        input: PathBuf,
    },
    /// Execute an explicitly approved native profile through the existing owner.
    SuperviseNativeBuild {
        #[arg(long)]
        input: PathBuf,
    },
    /// Execute strict full-native3 through the same approved owner context.
    SuperviseFullNativeBuild {
        #[arg(long)]
        input: PathBuf,
    },
    /// Compare two actual full-native3 images through the existing selected owner.
    ReproduceLoader6Native {
        #[arg(long)]
        input: PathBuf,
    },
    /// Inspect retained compiler/source/output and complete /7 input relationships.
    ReproduceCompiler7Native {
        #[arg(long)]
        input: PathBuf,
    },
    SuperviseCompiler7Native {
        #[arg(long)]
        input: PathBuf,
    },
    ReproduceSdk8Native {
        #[arg(long)]
        input: PathBuf,
    },
    SuperviseSdk8Native {
        #[arg(long)]
        input: PathBuf,
    },
    ReproduceProbe9Native {
        #[arg(long)]
        input: PathBuf,
    },
    ReproduceProbe10Native {
        #[arg(long)]
        input: PathBuf,
    },
    ReproduceStream11Native {
        #[arg(long)]
        input: PathBuf,
    },
    PrepareNativeNormalizer {
        #[arg(long)]
        input: PathBuf,
    },
    SuperviseStream11Native {
        #[arg(long)]
        input: PathBuf,
    },
    SuperviseProbe9Native {
        #[arg(long)]
        input: PathBuf,
    },
    SuperviseProbe10Native {
        #[arg(long)]
        input: PathBuf,
    },
    InspectCompilerLinkInputs7 {
        #[arg(long)]
        input: PathBuf,
    },
    /// Inspect source-declared executable and selected SDK library content only.
    InspectSystemLinkInputs8 {
        #[arg(long)]
        input: PathBuf,
    },
    ReproduceFullNative {
        #[arg(long)]
        input: PathBuf,
    },
    /// Inspect the same owner's whole-code qualification boundary; never activates by declaration.
    CodeEligibility {
        #[arg(long)]
        input: PathBuf,
    },
    /// Associate fresh finite evidence with the complete original inventory; no activation.
    CodeAssociations {
        #[arg(long)]
        input: PathBuf,
    },
    Register {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        actor: String,
    },
    Select {
        #[arg(long)]
        input: PathBuf,
    },
    Restore {
        #[arg(long)]
        input: PathBuf,
    },
    Active {
        #[arg(long)]
        binding: String,
    },
    UseMethod {
        #[arg(long)]
        binding: String,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    ReviewRule {
        #[arg(long)]
        binding: String,
        #[arg(long)]
        review_binding: PathBuf,
        #[arg(long)]
        evidence: PathBuf,
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        query: Vec<String>,
        #[arg(long)]
        shape: Vec<String>,
        #[arg(long)]
        output: PathBuf,
    },
}
fn read(root: &SkillRoot, p: &PathBuf) -> Result<Value> {
    Ok(oe_core::json::strict_json(&oe_core::read_regular_bytes(
        &root.expand(p)?,
        16 * 1024 * 1024,
    )?)?)
}
pub fn execute(root: &SkillRoot, c: VersionCommand) -> Result<u8> {
    crate::v2::require_execution(root, "controlled_version")?;
    let result = match c {
        VersionCommand::SuperviseBuild { input } => {
            use oe_contracts::CodeBuildSupervisionPort;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::CodeBuildSupervisionExecutor { root }
                .supervise(&request)
                .map_err(|e| anyhow::anyhow!("code_build_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::SuperviseNativeBuild { input } => {
            use oe_contracts::CodeNativeBuildSupervisionPort;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeCodeBuildSupervisionExecutor { root }
                .supervise_native(&request)
                .map_err(|e| anyhow::anyhow!("native_code_build_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::SuperviseFullNativeBuild { input } => {
            use oe_contracts::CodeNativeBuildSupervision3Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeFullCodeBuildSupervisionExecutor { root }
                .supervise_full_native(&request)
                .map_err(|e| anyhow::anyhow!("full_native_code_build_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::ReproduceFullNative { input } => {
            use oe_contracts::CodeNativeReproduction3Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeFullCodeReproductionExecutor { root }
                .reproduce_full_native(&request)
                .map_err(|e| anyhow::anyhow!("native_full_reproduction_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::ReproduceLoader6Native { input } => {
            use oe_contracts::CodeNativeReproduction6Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeLoader6CodeReproductionExecutor { root }
                .reproduce_full_native(&request)
                .map_err(|e| anyhow::anyhow!("native_full_reproduction_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::ReproduceCompiler7Native { input } => {
            use oe_contracts::CodeNativeReproduction7Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeCompiler7CodeReproductionExecutor { root }
                .reproduce_full_native(&request)
                .map_err(|e| anyhow::anyhow!("compiler7_native_reproduction_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::SuperviseCompiler7Native { input } => {
            use oe_contracts::CodeNativeBuildSupervision7Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result =
                oe_engineering::registry::NativeCompiler7CodeBuildSupervisionExecutor { root }
                    .supervise_full_native(&request)
                    .map_err(|e| anyhow::anyhow!("compiler7_native_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::ReproduceSdk8Native { input } => {
            use oe_contracts::CodeNativeReproduction8Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeSdk8CodeReproductionExecutor { root }
                .reproduce_full_native(&request)
                .map_err(|e| anyhow::anyhow!("sdk8_native_reproduction_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::SuperviseSdk8Native { input } => {
            use oe_contracts::CodeNativeBuildSupervision8Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeSdk8CodeBuildSupervisionExecutor { root }
                .supervise_full_native(&request)
                .map_err(|e| anyhow::anyhow!("sdk8_native_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::ReproduceProbe9Native { input } => {
            use oe_contracts::CodeNativeReproduction9Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeProbe9CodeReproductionExecutor { root }
                .reproduce_full_native(&request)
                .map_err(|e| anyhow::anyhow!("probe9_native_reproduction_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::SuperviseProbe9Native { input } => {
            use oe_contracts::CodeNativeBuildSupervision9Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result =
                oe_engineering::registry::NativeProbe9CodeBuildSupervisionExecutor { root }
                    .supervise_full_native(&request)
                    .map_err(|e| anyhow::anyhow!("probe9_native_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::ReproduceProbe10Native { input } => {
            use oe_contracts::CodeNativeReproduction10Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeProbe10CodeReproductionExecutor { root }
                .reproduce_full_native(&request)
                .map_err(|e| anyhow::anyhow!("probe10_native_reproduction_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::ReproduceStream11Native { input } => {
            use oe_contracts::CodeNativeReproduction11Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeStream11CodeReproductionExecutor { root }
                .reproduce_full_native(&request)
                .map_err(|e| anyhow::anyhow!("stream11_native_reproduction_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::SuperviseProbe10Native { input } => {
            use oe_contracts::CodeNativeBuildSupervision10Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result =
                oe_engineering::registry::NativeProbe10CodeBuildSupervisionExecutor { root }
                    .supervise_full_native(&request)
                    .map_err(|e| anyhow::anyhow!("probe10_native_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::PrepareNativeNormalizer { input } => {
            use oe_contracts::CodeNativeNormalizerSupervision1Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::NativeNormalizerSupervisionExecutor { root }
                .prepare_normalizer(&request)
                .map_err(|e| anyhow::anyhow!("normalizer_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::SuperviseStream11Native { input } => {
            use oe_contracts::CodeNativeBuildSupervision11Port;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result =
                oe_engineering::registry::NativeStream11CodeBuildSupervisionExecutor { root }
                    .supervise_full_native(&request)
                    .map_err(|e| anyhow::anyhow!("stream11_native_supervision_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::InspectCompilerLinkInputs7 { input } => {
            use oe_contracts::CodeNativeCompilerLinkInput7InspectPort;
            let input = root.private_path(&input)?;
            let reference = oe_contracts::ArtifactReference {
                path: input
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("compiler_link_inputs7_non_utf8_path"))?
                    .into(),
                sha256: oe_core::sha256(&oe_core::read_regular_bytes(&input, 16 * 1024 * 1024)?),
            };
            let result =
                oe_evolution::native_compiler_link7::BorrowedCompilerLinkInput7Inspector { root }
                    .inspect_complete_compiler_inputs7(&reference)
                    .map_err(|e| anyhow::anyhow!("compiler_link_inputs7_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::InspectSystemLinkInputs8 { input } => {
            use oe_contracts::CodeNativeSystemInput8InspectPort;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_evolution::native_system_link8::BorrowedSystemInput8Inspector { root }
                .inspect_system_inputs8(&request)
                .map_err(|e| anyhow::anyhow!("system_link_inputs8_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::CodeAssociations { input } => {
            use oe_contracts::CodeScopeAssociationPort;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::CodeArtifactEligibilityExecutor { root }
                .associate(&request)
                .map_err(|e| anyhow::anyhow!("code_scope_association_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::CodeEligibility { input } => {
            use oe_contracts::CodeArtifactEligibilityPort;
            let request = serde_json::from_value(read(root, &input)?)?;
            let result = oe_engineering::registry::CodeArtifactEligibilityExecutor { root }
                .qualify(&request)
                .map_err(|e| anyhow::anyhow!("code_artifact_eligibility_port:{e:?}"))?;
            serde_json::to_value(result)?
        }
        VersionCommand::Register { input, actor } => oe_engineering::registry::register(
            root,
            &serde_json::from_value(read(root, &input)?)?,
            &actor,
        )?,
        VersionCommand::Select { input } => select(root, input, false)?,
        VersionCommand::Restore { input } => select(root, input, true)?,
        VersionCommand::Active { binding } => {
            let (d, h) = oe_engineering::registry::active(root, &binding)?;
            json!({"definition":d,"selection":h,"publication":"not_performed"})
        }
        VersionCommand::UseMethod {
            binding,
            input,
            output,
        } => oe_engineering::registry::use_method(root, &binding, &read(root, &input)?, &output)?,
        VersionCommand::ReviewRule {
            binding,
            review_binding,
            evidence,
            project,
            query,
            shape,
            output,
        } => {
            let (bundle, selection) = oe_engineering::registry::rule_bundle(root, &binding)?;
            let b: oe_kernel::Binding = serde_json::from_value(read(root, &review_binding)?)?;
            let bytes = oe_core::read_regular_bytes(&root.expand(&evidence)?, 16 * 1024 * 1024)?;
            let p = serde_json::from_value(read(root, &project)?)?;
            let request = oe_contracts::ReviewRequest {
                object: oe_contracts::ObjectIdentity {
                    object_id: b.object_id.clone(),
                    revision: b.object_revision.clone(),
                    condition_scope: b.condition_scope.clone(),
                    source_sha256: b.source_sha256.clone(),
                },
                evidence_sha256: oe_core::sha256(&bytes),
                required_checks: b.required_checks.iter().cloned().collect(),
            };
            let port = oe_kernel::port::RustSemanticPort {
                root: root.path().to_owned(),
                binding: b,
                bundle,
                queries: query,
                shapes: shape,
                evidence: bytes.clone(),
            };
            let receipt = oe_kernel::project::review_current(
                root,
                &p,
                &bytes,
                port.execute(&request, None)?,
            )?;
            let (_, head) = oe_engineering::registry::rule_bundle(root, &binding)?;
            ensure!(
                head["event_sha256"] == selection["event_sha256"],
                "version_changed_during_use"
            );
            let result = json!({"review":receipt,"version_selection":selection,"engineering_adoption":"not_established"});
            write_new(&root.private_path(&output)?, &result)?;
            result
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(0)
}
fn select(root: &SkillRoot, input: PathBuf, restore: bool) -> Result<Value> {
    use oe_contracts::ControlledVersionPort;
    let request = serde_json::from_value(read(root, &input)?)?;
    let result = oe_engineering::registry::SelectionExecutor { root, restore }
        .select(&request)
        .map_err(|e| anyhow::anyhow!("controlled_version_port:{e:?}"))?;
    Ok(serde_json::to_value(result)?)
}
