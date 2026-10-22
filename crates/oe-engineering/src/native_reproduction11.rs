//! Two current segmented Cargo stream /11 builds through the existing selected build owner.
//! Source-profile6 and complete SDK8 artifacts stay shared facts; original pair deadline and full-byte comparisons remain.
//! Full file equality is content evidence, never release or engineering adoption.
use crate::{artifacts, code_build_supervision, registry};
use anyhow::{Context, Result, ensure};
use oe_contracts::{
    CodeBuildSupervisionContext, CodeNativeBuildCapture11, CodeNativeBuildSupervisionResult11,
    CodeNativeImageReproductionObservation11, CodeNativeReproductionObservation11,
    CodeNativeReproductionRequest11, CodeSourceArtifact,
};
use oe_core::{SkillRoot, write_new};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Compare all bytes from current verified file references. This pure content
/// operation confers no execution proof, caller permission or adoption authority.
use crate::native_reproduction::full_file_equal;

fn artifact(
    root: &SkillRoot,
    context: &CodeBuildSupervisionContext,
    request: &oe_contracts::CodeNativeBuildSupervisionRequest11,
    result: &CodeNativeBuildSupervisionResult11,
) -> Result<CodeSourceArtifact> {
    ensure!(
        result.execution == "completed" && result.compilation_origin == "owner_observed",
        "native_reproduction_owner_execution_not_completed"
    );
    result
        .validate()
        .map_err(|e| anyhow::anyhow!("stream11_reproduction_supervision_result:{e:?}"))?;
    oe_evolution::native_stream11::verify_resources11(&result.resources)?;
    ensure!(
        result.schema == "oev2.native-build-supervision-result/11"
            && result.owner_request == request.owner_request
            && result.resources == request.resources,
        "stream11_reproduction_owner_or_resources_changed"
    );
    let artifact_ref = result
        .artifact
        .as_ref()
        .context("native_reproduction_artifact_missing")?;
    let capture_ref = result
        .capture
        .as_ref()
        .context("native_reproduction_capture_missing")?;
    let capture: CodeNativeBuildCapture11 =
        serde_json::from_value(code_build_supervision::native_json(root, capture_ref)?)?;
    let content: CodeSourceArtifact =
        serde_json::from_value(code_build_supervision::native_json(root, artifact_ref)?)?;
    oe_evolution::native_runtime11::verify_full_native_capture(
        root,
        &context.source_profile,
        &capture.side_context,
        artifact_ref,
        &capture,
    )?;
    ensure!(
        capture.resources == request.resources
            && capture.artifact == *artifact_ref
            && capture.driver == context.builder
            && content.artifact_id == context.build.artifact_id
            && content.source_root == context.build.source_root,
        "native_reproduction_owner_artifact_changed"
    );
    oe_evolution::verify_artifact(&content)?;
    Ok(content)
}

fn paths(
    root: &SkillRoot,
    contexts: &[&CodeBuildSupervisionContext],
    evidence: &str,
) -> Result<()> {
    let mut paths = vec![root.private_path(Path::new(evidence))?];
    for c in contexts {
        let target = root.private_path(Path::new(&c.build.target_dir))?;
        let stage = target.with_file_name(format!(
            "{}-native-stage",
            target
                .file_name()
                .and_then(|x| x.to_str())
                .context("native_reproduction_target_leaf")?
        ));
        paths.extend([
            target,
            stage,
            root.private_path(Path::new(&c.build.evidence_path))?,
        ]);
    }
    for (i, p) in paths.iter().enumerate() {
        ensure!(!p.exists(), "native_reproduction_path_not_fresh");
        for q in paths.iter().skip(i + 1) {
            ensure!(
                !p.starts_with(q) && !q.starts_with(p),
                "native_reproduction_paths_overlap"
            );
        }
    }
    Ok(())
}

pub fn reproduce_stream11_native(
    root: &SkillRoot,
    input: &CodeNativeReproductionRequest11,
) -> Result<CodeNativeReproductionObservation11> {
    let started = Instant::now();
    input
        .validate()
        .map_err(|e| anyhow::anyhow!("native_reproduction_contract:{e:?}"))?;
    ensure!(
        input.schema == "oev2.native-reproduction-request/11",
        "native_reproduction_schema"
    );
    for request in [&input.first, &input.second] {
        request
            .validate()
            .map_err(|e| anyhow::anyhow!("stream11_reproduction_request:{e:?}"))?;
        request
            .owner_request
            .validate()
            .map_err(|e| anyhow::anyhow!("stream11_reproduction_owner_request:{e:?}"))?;
        ensure!(
            request.schema == "oev2.native-build-supervision-request/11"
                && request.owner_request.schema == "oev2.code-build-supervision-request/1",
            "stream11_reproduction_nested_schema"
        );
    }
    // Both explicit resource choices are checked before any owner or output is opened.
    oe_evolution::native_stream11::verify_resources11(&input.first.resources)?;
    oe_evolution::native_stream11::verify_resources11(&input.second.resources)?;
    ensure!(
        input.first.resources == input.second.resources,
        "stream11_reproduction_resource_baselines_differ"
    );
    let first = registry::build_context(root, &input.first.owner_request)?;
    let second = registry::build_context(root, &input.second.owner_request)?;
    ensure!(
        input.first.schema == "oev2.native-build-supervision-request/11"
            && input.second.schema == "oev2.native-build-supervision-request/11"
            && input.first.owner_request.actor == input.second.owner_request.actor
            && input.first.owner_request.scope_id == input.second.owner_request.scope_id,
        "native_reproduction_owner_domains_differ"
    );
    ensure!(
        first.0.schema == "oev2.code-build-supervision-context/2"
            && second.0.schema == first.0.schema
            && first.1 == second.1
            && first.0.dispatch_budget == second.0.dispatch_budget
            && first.0.source_profile == second.0.source_profile
            && first.0.builder == second.0.builder
            && first.0.expected_tools == second.0.expected_tools
            && first.0.build.source_root == second.0.build.source_root
            && first.0.build.cargo_home == second.0.build.cargo_home
            && first.0.build.platform == second.0.build.platform,
        "native_reproduction_selected_conditions_differ"
    );
    let budget = &first.0.dispatch_budget;
    ensure!(
        (251..=3_600_000).contains(&budget.timeout_ms)
            && (1..=4 * 1024 * 1024).contains(&budget.output_bytes),
        "native_reproduction_selected_budget"
    );
    let deadline = started
        .checked_add(Duration::from_millis(budget.timeout_ms))
        .context("native_reproduction_deadline_overflow")?;
    ensure!(
        Instant::now() < deadline,
        "native_reproduction_deadline_exhausted"
    );
    // Shared source/profile6 conditions inspection precedes creation of any reproduction directory.
    oe_evolution::native_profile6::inspect_profile(root, &first.0.source_profile)?;
    paths(root, &[&first.0, &second.0], &input.evidence_path)?;
    let source = oe_evolution::public_manifest(root.path())?;
    let folder = root.new_private_dir(Path::new(&input.evidence_path))?;
    write_new(&folder.join("request.json"), &serde_json::to_value(input)?)?;
    let mut result = CodeNativeReproductionObservation11 {
        schema:"oev2.native-reproduction-observation/11".into(), first:None, second:None,
        images:vec![],execution:"failed".into(),reproduction:"unknown".into(),
        staging_descriptor:None,operation_error:None,
        resources:input.first.resources.clone(),
        uncovered:vec!["Full byte equality does not establish whole13494/module19/protocol19/OWNER22 or original retired14 compatibility".into(),
            "Original1/2 profiles and failed pair remain unchanged; current11 execution identity and selected Cargo resources with original SDK8 artifacts are independent".into(),
            "Selection, activation, use-feedback-recovery, release/publication and engineering adoption are not performed".into(),
            "One existing context2 budget is borrowed across both selected contexts of the same registry owner; synchronous I/O and cleanup are not hard-wall certified".into()],
    };
    let operation = (|| -> Result<()> {
        result.first = Some(
            code_build_supervision::supervise_stream11_native_with_deadline(
                root,
                &input.first,
                deadline,
            )?,
        );
        let left = artifact(root, &first.0, &input.first, result.first.as_ref().unwrap())?;
        ensure!(
            Instant::now() < deadline,
            "native_reproduction_deadline_exhausted"
        );
        result.second = Some(
            code_build_supervision::supervise_stream11_native_with_deadline(
                root,
                &input.second,
                deadline,
            )?,
        );
        let right = artifact(
            root,
            &second.0,
            &input.second,
            result.second.as_ref().unwrap(),
        )?;
        ensure!(
            registry::build_context(root, &input.first.owner_request)? == first
                && registry::build_context(root, &input.second.owner_request)? == second
                && oe_evolution::public_manifest(root.path())? == source,
            "native_reproduction_sources_or_owner_changed"
        );
        ensure!(
            left.tools.len() == 4 && right.tools.len() == 4,
            "native_reproduction_normalizer_missing"
        );
        for (role, a, b) in [
            ("normalizer", &left.tools[3], &right.tools[3]),
            ("runner", &left.runner, &right.runner),
        ] {
            let equal = full_file_equal(a, b)?;
            result
                .images
                .push(CodeNativeImageReproductionObservation11 {
                    image_role: role.into(),
                    first: a.clone(),
                    second: b.clone(),
                    full_bytes_equal: equal,
                });
        }
        // The first full capture must remain current while the second build and
        // both complete image comparisons run. Neither a stale report nor a
        // caller-supplied completed result can stand in for this final check.
        ensure!(
            artifact(root, &first.0, &input.first, result.first.as_ref().unwrap())? == left
                && artifact(
                    root,
                    &second.0,
                    &input.second,
                    result.second.as_ref().unwrap()
                )? == right,
            "stream11_reproduction_captures_changed_after_comparison"
        );
        ensure!(
            registry::build_context(root, &input.first.owner_request)? == first
                && registry::build_context(root, &input.second.owner_request)? == second
                && oe_evolution::public_manifest(root.path())? == source,
            "native_reproduction_sources_or_owner_changed_after_comparison"
        );
        ensure!(
            Instant::now() < deadline,
            "native_reproduction_deadline_exhausted"
        );
        result.execution = "completed".into();
        let equal =
            left.compiled == right.compiled && result.images.iter().all(|x| x.full_bytes_equal);
        result.reproduction = if equal { "equal" } else { "different" }.into();
        if equal {
            // Local descriptor only. The existing owner has NOT selected or adopted it.
            let p = folder.join("staging-descriptor.json");
            write_new(
                &p,
                &serde_json::json!({"schema":"oev2.native-reproduction-staging/11",
                "first":result.first,"second":result.second,"images":result.images,"resources":result.resources,
                "source_manifest_sha256":oe_evolution::digest(&source)?,
                "publication":"not_performed","release_eligibility":"unknown","engineering_adoption":"not_established"}),
            )?;
            ensure!(
                Instant::now() < deadline,
                "native_reproduction_deadline_exhausted"
            );
            result.staging_descriptor = Some(artifacts::reference(root, &p)?);
        }
        Ok(())
    })();
    if let Err(e) = operation {
        result.execution = "failed".into();
        result.reproduction = "unknown".into();
        result.staging_descriptor = None;
        result.operation_error = Some(format!("{e:#}"));
    }
    result
        .validate()
        .map_err(|e| anyhow::anyhow!("native_reproduction_output:{e:?}"))?;
    write_new(
        &folder.join("reproduction.json"),
        &serde_json::to_value(&result)?,
    )?;
    // Caller-visible success cannot escape expiry during terminal retention.
    if Instant::now() >= deadline {
        result.execution = "failed".into();
        result.reproduction = "unknown".into();
        result.staging_descriptor = None;
        result.operation_error =
            Some("native_reproduction_deadline_exhausted_after_retention".into());
        write_new(
            &folder.join("deadline-rejection.json"),
            &serde_json::to_value(&result)?,
        )?;
    }
    Ok(result)
}
