//! Internal fixed experiment executable. The user skill entry remains oev2.
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, read_regular_bytes, sha256, write_new};
use oe_evolution::{
    CodeBuildRequest, CodeExperiment, CodeSourceArtifact, FixedExperiment, binary_reference, build,
    digest,
};
use oe_wire::json::strict_json;
use serde_json::{Value, json};
use std::path::Path;
fn value(path: &str) -> Result<Value> {
    Ok(strict_json(&read_regular_bytes(
        Path::new(path),
        16 * 1024 * 1024,
    )?)?)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() >= 2,
        "usage: internal driver build request | prepare root baseline candidate oracle protocol evaluator id output | run root experiment expected-sha output"
    );
    match args[1].as_str() {
        "runtime-build2" => {
            ensure!(args.len() == 3, "runtime2_build_arguments");
            let request: oe_evolution::CodeRustRuntimeReproductionRequest2 =
                serde_json::from_value(value(&args[2])?)?;
            let result = oe_evolution::runtime_reproduction2::reproduce(&request)?;
            println!("{}", serde_json::to_string(&result)?);
            if result.base_observation.execution != "completed"
                || result.base_observation.reproduction != "equal"
            {
                std::process::exit(1);
            }
        }
        "build-full-native-budgeted" => {
            ensure!(args.len() == 3, "native3_budgeted_build_arguments");
            let request: oe_evolution::CodeNativeBudgetedBuildRequest3 =
                serde_json::from_value(value(&args[2])?)?;
            let capture = oe_evolution::native_runtime3::build_full_native_budgeted(&request)?;
            let root = SkillRoot::new(&request.build.source_root)?;
            let path = root.private_path(
                &Path::new(&request.build.evidence_path).join("native-full-capture.json"),
            )?;
            let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
            ensure!(
                strict_json(&bytes)? == serde_json::to_value(&capture)?,
                "native3_driver_capture_changed"
            );
            use std::io::Write;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        "build-loader6-native-budgeted" => {
            ensure!(args.len() == 3, "native3_budgeted_build_arguments");
            let request: oe_evolution::CodeNativeBudgetedBuildRequest6 =
                serde_json::from_value(value(&args[2])?)?;
            let capture = oe_evolution::native_runtime6::build_full_native_budgeted(&request)?;
            let root = SkillRoot::new(&request.build.source_root)?;
            let path = root.private_path(
                &Path::new(&request.build.evidence_path).join("native-loader6-capture.json"),
            )?;
            let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
            ensure!(
                strict_json(&bytes)? == serde_json::to_value(&capture)?,
                "native3_driver_capture_changed"
            );
            use std::io::Write;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        "build-compiler7-native-budgeted" => {
            ensure!(args.len() == 3, "native3_budgeted_build_arguments");
            let request: oe_evolution::CodeNativeBudgetedBuildRequest7 =
                serde_json::from_value(value(&args[2])?)?;
            let capture = oe_evolution::native_runtime7::build_full_native_budgeted(&request)?;
            let root = SkillRoot::new(&request.build.source_root)?;
            let path = root.private_path(
                &Path::new(&request.build.evidence_path).join("native-compiler7-capture.json"),
            )?;
            let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
            ensure!(
                strict_json(&bytes)? == serde_json::to_value(&capture)?,
                "native3_driver_capture_changed"
            );
            use std::io::Write;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        "build-sdk8-native-budgeted" => {
            ensure!(args.len() == 3, "native3_budgeted_build_arguments");
            let request: oe_evolution::CodeNativeBudgetedBuildRequest8 =
                serde_json::from_value(value(&args[2])?)?;
            let capture = oe_evolution::native_runtime8::build_full_native_budgeted(&request)?;
            let root = SkillRoot::new(&request.build.source_root)?;
            let path = root.private_path(
                &Path::new(&request.build.evidence_path).join("native-sdk8-capture.json"),
            )?;
            let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
            ensure!(
                strict_json(&bytes)? == serde_json::to_value(&capture)?,
                "native3_driver_capture_changed"
            );
            use std::io::Write;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        "build-probe9-native-budgeted" => {
            ensure!(args.len() == 3, "native3_budgeted_build_arguments");
            let request: oe_evolution::CodeNativeBudgetedBuildRequest9 =
                serde_json::from_value(value(&args[2])?)?;
            let capture = oe_evolution::native_runtime9::build_full_native_budgeted(&request)?;
            let root = SkillRoot::new(&request.build.source_root)?;
            let path = root.private_path(
                &Path::new(&request.build.evidence_path).join("native-probe9-capture.json"),
            )?;
            let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
            ensure!(
                strict_json(&bytes)? == serde_json::to_value(&capture)?,
                "native3_driver_capture_changed"
            );
            use std::io::Write;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        "build-probe10-native-budgeted" => {
            ensure!(args.len() == 3, "native3_budgeted_build_arguments");
            let request: oe_evolution::CodeNativeBudgetedBuildRequest10 =
                serde_json::from_value(value(&args[2])?)?;
            let capture = oe_evolution::native_runtime10::build_full_native_budgeted(&request)?;
            let root = SkillRoot::new(&request.build.source_root)?;
            let path = root.private_path(
                &Path::new(&request.build.evidence_path).join("native-probe10-capture.json"),
            )?;
            let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
            ensure!(
                strict_json(&bytes)? == serde_json::to_value(&capture)?,
                "native3_driver_capture_changed"
            );
            use std::io::Write;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        "prepare-stream11-normalizer" => {
            ensure!(args.len() == 3, "normalizer_preparation_arguments");
            let request: oe_evolution::CodeNativeNormalizerPreparationRequest1 =
                serde_json::from_value(value(&args[2])?)?;
            let result = oe_evolution::native_normalizer::prepare_normalizer(&request)?;
            let root = SkillRoot::new(&request.request.build.source_root)?;
            let path = root.private_path(
                &Path::new(&request.request.build.evidence_path)
                    .join(oe_evolution::native_normalizer::FILE),
            )?;
            let bytes = read_regular_bytes(&path, request.request.budget.output_bytes as usize)?;
            ensure!(
                strict_json(&bytes)? == serde_json::to_value(&result)?,
                "normalizer_preparation_changed"
            );
            use std::io::Write;
            std::io::stdout().lock().write_all(&bytes)?;
        }
        "build-stream11-native-budgeted" => {
            ensure!(args.len() == 3, "native11_budgeted_build_arguments");
            let request: oe_evolution::CodeNativeBudgetedBuildRequest11 =
                serde_json::from_value(value(&args[2])?)?;
            let capture = oe_evolution::native_runtime11::build_full_native_budgeted(&request)?;
            let root = SkillRoot::new(&request.build.source_root)?;
            let path = root.private_path(
                &Path::new(&request.build.evidence_path).join("native-stream11-capture.json"),
            )?;
            let bytes = read_regular_bytes(&path, 16 * 1024 * 1024)?;
            ensure!(
                strict_json(&bytes)? == serde_json::to_value(&capture)?,
                "native11_driver_capture_changed"
            );
            let receipt = oe_evolution::CodeNativeBuildDispatchReceipt11 {
                schema: "oev2.native-build-dispatch-receipt/11".into(),
                capture: oe_evolution::ArtifactReference {
                    path: path.to_str().context("native11_capture_path_utf8")?.into(),
                    sha256: oe_core::sha256(&bytes),
                },
                resources: capture.resources.clone(),
            };
            receipt
                .validate()
                .map_err(|e| anyhow::anyhow!("native11_dispatch_receipt:{e:?}"))?;
            let receipt_path = root.private_path(
                &Path::new(&request.build.evidence_path)
                    .join("native-stream11-dispatch-receipt.json"),
            )?;
            oe_core::write_new(&receipt_path, &serde_json::to_value(&receipt)?)?;
            let receipt_bytes =
                read_regular_bytes(&receipt_path, request.budget.output_bytes as usize)?;
            ensure!(
                strict_json(&receipt_bytes)? == serde_json::to_value(&receipt)?,
                "native11_dispatch_receipt_changed"
            );
            use std::io::Write;
            std::io::stdout().lock().write_all(&receipt_bytes)?;
        }
        "build-native-budgeted" => {
            ensure!(args.len() == 3, "native_budgeted_build_arguments");
            let request: oe_evolution::CodeNativeBudgetedBuildRequest =
                serde_json::from_value(value(&args[2])?)?;
            println!(
                "{}",
                serde_json::to_string(&oe_evolution::native_runtime::build_native_budgeted(
                    &request
                )?)?
            );
        }
        "native-link" => {
            std::process::exit(oe_evolution::native_runtime::native_link_arguments(
                args[2..].to_vec(),
            )?);
        }
        "runtime-inspect" => {
            ensure!(args.len() == 3, "rust_runtime_inspect_arguments");
            let request = serde_json::from_value(value(&args[2])?)?;
            println!(
                "{}",
                serde_json::to_string(&oe_evolution::runtime_identity::inspect(&request)?)?
            );
        }
        "runtime-probe" | "runtime-reopen" => {
            ensure!(args.len() == 3, "rust_runtime_probe_arguments");
            let request: oe_evolution::CodeRustRuntimeProbeRequest =
                serde_json::from_value(value(&args[2])?)?;
            ensure!(
                request.phase
                    == if args[1] == "runtime-reopen" {
                        "reopen"
                    } else {
                        "probe"
                    },
                "rust_runtime_command_phase_mismatch"
            );
            let result = oe_evolution::runtime_identity::probe(&request)?;
            println!("{}", serde_json::to_string(&result)?);
            ensure!(
                result.execution == "completed" && result.identity == "bound",
                "rust_runtime_observation_not_bound"
            );
        }
        "runtime-build" => {
            ensure!(args.len() == 3, "rust_runtime_reproduction_arguments");
            let request = serde_json::from_value(value(&args[2])?)?;
            let result = oe_evolution::runtime_identity::reproduce(&request)?;
            println!("{}", serde_json::to_string(&result)?);
            ensure!(
                result.execution == "completed" && result.reproduction == "equal",
                "rust_runtime_reproduction_not_established"
            );
        }
        "scope-plan" => {
            ensure!(args.len() == 3, "scope_plan_arguments");
            println!(
                "{}",
                serde_json::to_string(&oe_evolution::scope_witness::unknown_bindings(Path::new(
                    &args[2]
                ))?)?
            );
        }
        "scope-witness" => {
            ensure!(args.len() == 7, "scope_witness_arguments");
            let root = SkillRoot::new(&args[2])?;
            let experiment: CodeExperiment = serde_json::from_value(value(&args[3])?)?;
            ensure!(
                digest(&serde_json::to_value(&experiment)?)? == args[4],
                "code_freeze_identity_changed"
            );
            let request: oe_evolution::CodeScopeWitnessRequest =
                serde_json::from_value(value(&args[5])?)?;
            let fixed = FixedExperiment::freeze(experiment)?;
            println!(
                "{}",
                serde_json::to_string(
                    &oe_evolution::scope_witness::run_selected(
                        &fixed,
                        &request,
                        &root
                            .private_path(Path::new(&args[6]))?
                            .strip_prefix(root.path())?
                    )?
                    .value()?
                )?
            );
        }
        "identity" => println!("{}", serde_json::to_string(&oe_evolution::compiled()?)?),
        "source-check" => {
            // The installed release keeps its source as skill/ next to bin/; a mismatch exits 1.
            let root = oe_evolution::driver_root()?;
            let actual = oe_evolution::digest(&oe_evolution::public_manifest(&root)?)?;
            let compiled = oe_evolution::compiled()?["manifest_sha256"].clone();
            let matched = compiled == serde_json::json!(actual);
            println!(
                "{}",
                serde_json::json!({"schema": "oev2.driver-source-check/1", "root": root, "manifest_sha256": actual,
                                   "compiled_manifest_sha256": compiled, "matched": matched})
            );
            if !matched {
                std::process::exit(1);
            }
        }
        "build-budgeted" => {
            ensure!(args.len() == 3, "code_budgeted_build_arguments");
            let request: oe_evolution::CodeBudgetedBuildRequest =
                serde_json::from_value(value(&args[2])?)?;
            println!(
                "{}",
                serde_json::to_string(&oe_evolution::build_budgeted(&request)?)?
            );
        }
        "build" => {
            ensure!(args.len() == 3, "code_build_arguments");
            let request: CodeBuildRequest = serde_json::from_value(value(&args[2])?)?;
            println!("{}", serde_json::to_string(&build(&request)?)?);
        }
        "prepare" => {
            ensure!(args.len() == 11, "code_prepare_arguments");
            let root = SkillRoot::new(&args[2])?;
            let baseline: CodeSourceArtifact = serde_json::from_value(value(&args[3])?)?;
            let candidate: CodeSourceArtifact = serde_json::from_value(value(&args[4])?)?;
            let oracle = read_regular_bytes(Path::new(&args[5]), 4 * 1024 * 1024)?;
            let cases: Vec<oe_evolution::CodeReplayCase> =
                serde_json::from_value(strict_json(&oracle)?)?;
            let protocol = read_regular_bytes(Path::new(&args[6]), 4 * 1024 * 1024)?;
            let protocol_doc = strict_json(&protocol)?;
            let ids = protocol_doc["cases"]
                .as_array()
                .context("code_protocol_cases")?
                .iter()
                .map(|x| {
                    x["case_id"]
                        .as_str()
                        .map(str::to_owned)
                        .context("code_protocol_case_id")
                })
                .collect::<Result<Vec<_>>>()?;
            let experiment=CodeExperiment{schema:"oev2.code-experiment/1".into(),experiment_id:args[8].clone(),
                driver:binary_reference(&std::env::current_exe()?,root.path())?,
                evaluator:binary_reference(Path::new(&args[7]),root.path())?,
                oracle_path:Path::new(&args[5]).canonicalize()?.to_str().context("code_oracle_path")?.into(),oracle_sha256:sha256(&oracle),
                case_set_sha256:digest(&serde_json::to_value(&cases)?)?,
                goal_sha256:sha256(b"Fixed local deterministic production API comparison; no claimed industrial benefit"),
                conditions_sha256:cases.first().context("code_cases_empty")?.conditions_sha256.clone(),
                scope_sha256:sha256(read_regular_bytes(&root.path().join("runtime/semantica-scope.json"),16*1024*1024)?),
                baseline,candidate,cases,protocol_case_ids:ids,protocol_expectations_sha256:sha256(protocol),
                protocol_expectations_path:Path::new(&args[6]).canonicalize()?.to_str().context("code_protocol_path")?.into(),
                uncovered:vec!["19 frozen protocol requirements are a separate denominator from finite runtime cases".into(),
                    "Synthetic development replay; no real 169 engineering delivery, stochastic transfer or full source behavior migration".into(),
                    "Same-user subprocess identity checks, not hostile-code OS sandbox containment".into()],
                owner_connection:"not_connected".into()};
            let fixed = FixedExperiment::freeze(experiment)?;
            let folder = root.new_private_dir(Path::new(&args[9]))?;
            write_new(
                &folder.join("experiment.json"),
                &serde_json::to_value(fixed.experiment())?,
            )?;
            write_new(
                &folder.join("freeze.json"),
                &json!({"schema":"oev2.code-experiment-freeze/1","experiment_sha256":fixed.identity(),"immutable_refs":true,
                "version_selection":"not_connected","engineering_adoption":"unknown"}),
            )?;
            // A separately chosen output file receives the small freeze reference.
            write_new(
                &root.private_path(Path::new(&args[10]))?,
                &json!({"experiment":folder.join("experiment.json"),"sha256":fixed.identity()}),
            )?;
            println!(
                "{}",
                serde_json::to_string(
                    &json!({"experiment_sha256":fixed.identity(),"path":folder.join("experiment.json")})
                )?
            );
        }
        "run" => {
            ensure!(args.len() == 6, "code_run_arguments");
            let root = SkillRoot::new(&args[2])?;
            let experiment: CodeExperiment = serde_json::from_value(value(&args[3])?)?;
            ensure!(
                digest(&serde_json::to_value(&experiment)?)? == args[4],
                "code_freeze_identity_changed"
            );
            let fixed = FixedExperiment::freeze(experiment)?;
            println!(
                "{}",
                serde_json::to_string(&fixed.run(&root.private_path(Path::new(&args[5]))?)?)?
            );
        }
        _ => anyhow::bail!("code_driver_command_unsupported"),
    }
    Ok(())
}
