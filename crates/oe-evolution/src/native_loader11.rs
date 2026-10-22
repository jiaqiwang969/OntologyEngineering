//! Full native11 loader binding uses original real receipt/intent checks and
//! bounded paged text. Text PIDs cannot create an actual parent association.
use crate::native_loader11_content::{VerifiedReportedPidIndex11, prepare_loader_stream_content11};
use crate::native_runtime::{read, reference};
use crate::*;

fn bindings(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<(PathBuf, CodeNativeRetainedLoaderObservation11)>> {
    ensure!(
        crate::native_link8::is_adapter8(recipe)?,
        "sdk8_selected_adapter_required"
    );
    let policy = crate::native_loader6::inspect_policy(root, &recipe.cargo_loader_policy)?;
    let mut bindings = Vec::new();
    for stage in ["build-normalizer", "build-runner"] {
        let folder = Path::new(&side.stage_root).join(stage);
        let context = reference(root, &folder.join("stage-context.json"))?;
        let evidence = Path::new(&side.build.evidence_path).join(stage);
        let capture = reference(root, &evidence.join("stream-capture11.json"))?;
        let report = reference(root, &evidence.join("loader-content11.json"))?;
        let index = VerifiedReportedPidIndex11::inspect(root, &report)?;
        ensure!(*index.capture() == capture, "loader11_report_stage_changed");
        let mut calls = std::fs::read_dir(&folder)?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        calls.sort();
        for call in calls.iter().filter(|p| {
            p.file_name()
                .and_then(|p| p.to_str())
                .is_some_and(|n| n.starts_with("call-"))
        }) {
            let (receipt, pid) = crate::native_loader6::retained_call_association(
                root, side, recipe, 8, &policy, &context, call,
            )?;
            let ordinals = index.reported_lines(pid);
            ensure!(
                !ordinals.is_empty(),
                "loader6_actual_rustc_loader_output_missing"
            );
            let observation = CodeNativeRetainedLoaderObservation11 {
                schema: "oev2.native-retained-loader-observation/11".into(),
                native_receipt: receipt,
                stream_capture: capture.clone(),
                diagnostic_report: report.clone(),
                compiler_reported_lines: ordinals.to_vec(),
            };
            observation
                .validate()
                .map_err(|e| anyhow::anyhow!("loader11_observation:{e:?}"))?;
            bindings.push((call.join("retained-loader-observation11.json"), observation));
        }
        // One final full raw/page/source check for the stage, not a raw replay
        // for each invocation. The index never grants dispatch authority.
        index.verify_current()?;
    }
    ensure!(!bindings.is_empty(), "loader6_receipts_missing");
    Ok(bindings)
}

pub(crate) fn prepare_retained(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    for stage in ["build-normalizer", "build-runner"] {
        let evidence = Path::new(&side.build.evidence_path).join(stage);
        prepare_loader_stream_content11(
            root,
            &reference(root, &evidence.join("stream-capture11.json"))?,
        )?;
    }
    let mut references = Vec::new();
    for (path, observation) in bindings(root, side, recipe)? {
        write_new(&path, &serde_json::to_value(observation)?)?;
        references.push(reference(root, &path)?);
    }
    Ok(references)
}

pub(crate) fn verify_retained(
    root: &SkillRoot,
    side: &CodeNativeSideContext6,
    recipe: &CodeNativeBuildRecipe6,
) -> Result<Vec<ArtifactReference>> {
    let mut references = Vec::new();
    for (path, expected) in bindings(root, side, recipe)? {
        let current = reference(root, &path)?;
        ensure!(
            read(root, &current)? == serde_json::to_value(expected)?,
            "loader11_complete_raw_binding_changed"
        );
        references.push(current);
    }
    Ok(references)
}
