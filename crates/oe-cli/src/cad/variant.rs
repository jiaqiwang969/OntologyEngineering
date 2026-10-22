//! Parameter/drawing operations with current executable and optional Project binding.
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use oe_cad::{drawing, variant};
use oe_contracts::CenteredHoleRow;
use oe_core::{SkillRoot, read_regular_bytes, write_new};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub enum VariantCommand {
    /// Exact fixed-pitch centered row; absent Project means layout math only.
    PlanRow {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, requires = "project_context")]
        project: Option<PathBuf>,
        #[arg(long, requires = "project")]
        project_context: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
    /// One controller count shared with followers; native axes checked separately.
    PlanMating {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, requires = "project_context")]
        project: Option<PathBuf>,
        #[arg(long, requires = "project")]
        project_context: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compare source-bound native periodic faces through a rigid placement.
    CheckNative {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}
#[derive(Subcommand)]
pub enum DrawingCommand {
    /// Receive native PDF source/output identity; visual review remains separate.
    ReceivePdf {
        #[arg(long)]
        job_directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Source-bound complete dimension observations; no deletion or adoption.
    DimensionInventory {
        #[arg(long)]
        job_directory: PathBuf,
        #[arg(long)]
        part_input: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Receive explicit retained-dimension retirement and independent reopening.
    CompareRetirement {
        #[arg(long)]
        saved_job: PathBuf,
        #[arg(long)]
        reopened_job: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Audit a complete historical or current delivery packet, without re-rendering.
    AuditPacket {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compare current native association observations after saving and a separate reopen.
    CompareAssociations {
        #[arg(long)]
        saved_job: PathBuf,
        #[arg(long)]
        reopened_job: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify newly created sheet reference dimensions after saving and independent reopening.
    CompareDiameter {
        #[arg(long)]
        saved_job: PathBuf,
        #[arg(long)]
        reopened_job: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify two-center linear dimensions; the single-circle diameter contract is separate.
    CompareSheetReference {
        #[arg(long)]
        saved_job: PathBuf,
        #[arg(long)]
        reopened_job: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Receive a created prototype page after saving and independent reopening; no Project adoption.
    CompareDrafting {
        #[arg(long)]
        saved_job: PathBuf,
        #[arg(long)]
        reopened_job: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Independently declared reference obligations against a completed native job and current Project.
    CheckReferences {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        project_context: PathBuf,
        /// Explicit /2 consumer of freshly checked feature correspondence.
        #[arg(long)]
        correspondence: Option<PathBuf>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify one prototype drawing page against a completed inspection and current Project.
    CheckDrafting {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        project_context: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrawingReferenceInput {
    intent: oe_contracts::DrawingReferenceIntent,
    job_directory: PathBuf,
    part_input: String,
    native_result_ref: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DrawingDraftingInput {
    plan: oe_contracts::NativeDraftingPlan,
    job_directory: PathBuf,
    part_input: String,
    native_result_ref: String,
}
#[derive(Subcommand)]
pub enum MotionCommand {
    /// Verify original motion job, stream and current Project binding.
    Receive {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Compare a separately dispatched reopening of the exact saved bytes.
    ReceiveReopen {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    CheckDrive {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Current Rust projection of verified historical bytes, preserving unknown guards.
    ProjectV262 {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MotionProjection {
    source: oe_engineering::workflow::Reference,
    object: oe_contracts::ObjectIdentity,
    source_ref: String,
    limits: oe_cad::motion::history::ProjectionLimits,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mating {
    controller: CenteredHoleRow,
    followers: Vec<CenteredHoleRow>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeCheck {
    row: CenteredHoleRow,
    /// Explicit count authority for a member of a mating group.
    #[serde(default)]
    count_controller: Option<CenteredHoleRow>,
    readback: oe_engineering::workflow::Reference,
    /// Pointer into the verified raw native part observation.
    part_pointer: String,
    origin_yz: [f64; 2],
    placement: variant::RigidPlacement,
    tolerance_mm: f64,
}
fn raw(root: &SkillRoot, path: &Path) -> Result<Vec<u8>> {
    let path = root.expand(path)?;
    ensure!(
        path.starts_with(root.path()),
        "cad_variant_input_outside_root"
    );
    read_regular_bytes(&path, 16 * 1024 * 1024)
}
fn project_binding(
    root: &SkillRoot,
    row: &CenteredHoleRow,
    p: Option<&Path>,
    c: Option<&Path>,
) -> Result<Value> {
    match (p, c) {
        (Some(p), Some(c)) => {
            let bytes = raw(root, p)?;
            let project: oe_kernel::project::Project =
                serde_json::from_value(oe_core::json::strict_json(&bytes)?)?;
            let context: oe_contracts::ProjectReviewContext =
                serde_json::from_value(oe_core::json::strict_json(&raw(root, c)?)?)?;
            let supplied_object = oe_contracts::ObjectIdentity {
                object_id: context.object_id.clone(),
                revision: context.object_revision.clone(),
                condition_scope: context.condition_scope.clone(),
                source_sha256: context.source_sha256.clone(),
            };
            let parent_check = oe_kernel::project::check_current_context(
                &project,
                &supplied_object,
                Some(&context),
            );
            // One mating command may span several objects. The supplied
            // context binds the whole Project content; derive member contexts
            // only after its original object's current identity also passes.
            let check = if supplied_object == row.object || parent_check.conclusion != "pass" {
                oe_kernel::project::check_current_context(&project, &row.object, Some(&context))
            } else {
                let member = oe_kernel::project::context(&project, &row.object)?;
                oe_kernel::project::check_current_context(&project, &row.object, Some(&member))
            };
            let intent = oe_kernel::cad_intent::check(&project, row);
            let usable = check.conclusion == "pass" && intent.conclusion == "pass";
            let member_context = check.result.get("context").cloned().unwrap_or(Value::Null);
            let impact = oe_kernel::project::impact(
                &project,
                &variant::dependency_seeds(row)
                    .into_iter()
                    .collect::<BTreeSet<_>>(),
            )?;
            ensure!(
                raw(root, p)? == bytes,
                "cad_variant_project_changed_during_work"
            );
            Ok(
                json!({"current_use":if usable {"content_bound_design_intent"} else {"blocked"},
                    "project_context":member_context,"supplied_project_context":context,"supplied_project_check":parent_check,"current_project_check":check,"parameter_binding":intent,
                    "impact_if_parameters_change":impact,"engineering_adoption":"not_established"}),
            )
        }
        (None, None) => {
            Ok(json!({"current_use":"layout_math_only","engineering_conclusion":"unknown"}))
        }
        _ => anyhow::bail!("cad_variant_project_context_pair_missing"),
    }
}
pub fn run(root: &SkillRoot, command: VariantCommand) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_variant")?;
    let (value, output, code) = match command {
        VariantCommand::PlanRow {
            input,
            project,
            project_context,
            output,
        } => {
            let row: CenteredHoleRow =
                serde_json::from_value(oe_core::json::strict_json(&raw(root, &input)?)?)?;
            let value = json!({"schema":"oev2.cad-variant-result/1","layout":variant::plan(&row)?,
                "dependency_seeds":variant::dependency_seeds(&row),"project":project_binding(root,&row,project.as_deref(),project_context.as_deref())?,
                "source_input":input,"input_sha256":oe_core::sha256(raw(root,&input)?),"execution_binding":execution,
                "native_tool_execution":"not_run","engineering_adoption":"not_established"});
            let code = u8::from(value["project"]["current_use"] == "blocked");
            (value, output, code)
        }
        VariantCommand::PlanMating {
            input,
            project,
            project_context,
            output,
        } => {
            let m: Mating =
                serde_json::from_value(oe_core::json::strict_json(&raw(root, &input)?)?)?;
            let binding = project_binding(
                root,
                &m.controller,
                project.as_deref(),
                project_context.as_deref(),
            )?;
            let members = m.followers.iter().map(|row| Ok(json!({"row_id":row.row_id,
                "binding":project_binding(root,row,project.as_deref(),project_context.as_deref())?})))
                .collect::<Result<Vec<Value>>>()?;
            let code = u8::from(
                binding["current_use"] == "blocked"
                    || members
                        .iter()
                        .any(|v| v["binding"]["current_use"] == "blocked"),
            );
            (
                json!({"schema":"oev2.cad-mating-result/1","design":variant::plan_mating(&m.controller,&m.followers)?,"project":binding,"member_bindings":members,
                "execution_binding":execution,"native_tool_execution":"not_run","engineering_adoption":"not_established"}),
                output,
                code,
            )
        }
        VariantCommand::CheckNative { input, output } => {
            let n: NativeCheck =
                serde_json::from_value(oe_core::json::strict_json(&raw(root, &input)?)?)?;
            let bytes = raw(root, &n.readback.path)?;
            ensure!(
                oe_core::sha256(&bytes) == n.readback.sha256,
                "cad_variant_native_source_drift"
            );
            let readback = oe_core::json::strict_json(&bytes)?;
            let part = readback
                .pointer(&n.part_pointer)
                .context("cad_variant_native_part_pointer_missing")?;
            ensure!(part["units"] == "mm", "cad_variant_native_unit_mismatch");
            let faces = part["cylindrical_faces"]
                .as_array()
                .context("cad_variant_cylindrical_faces_missing")?;
            let diameter = n.row.diameter.parse::<f64>()?;
            let mut holes = Vec::new();
            for face in faces {
                let radius = face["radius"]
                    .as_f64()
                    .context("cad_variant_cylinder_radius_missing")?;
                if (radius * 2.0 - diameter).abs() > n.tolerance_mm {
                    continue;
                }
                let center: [f64; 3] = serde_json::from_value(face["axis_point"].clone())?;
                let axis: [f64; 3] = serde_json::from_value(face["axis_direction"].clone())?;
                // Selection is explicit in local coordinates. Other rows and
                // countersinks retain their original observations in readback.
                if (center[1] - n.origin_yz[0]).abs() > n.tolerance_mm {
                    continue;
                }
                holes.push(variant::NativeHole {
                    center: n.placement.point(center),
                    axis: n.placement.vector(axis),
                    diameter: radius * 2.0,
                    native_face_id: face["face_tag"].to_string(),
                });
            }
            let layout = match &n.count_controller {
                Some(controller) => variant::plan_follower(controller, &n.row)?,
                None => variant::plan(&n.row)?,
            };
            let checked =
                variant::check_native(&layout, &holes, n.origin_yz, &n.placement, n.tolerance_mm)?;
            ensure!(
                raw(root, &n.readback.path)? == bytes,
                "cad_variant_native_source_changed"
            );
            (
                json!({"schema":"oev2.cad-native-row-check/1","result":checked,"source":n.readback,
                "count_authority":n.count_controller,"layout":layout,
                "native_observation_identity":part["path"],"execution_binding":execution,"engineering_adoption":"not_established"}),
                output,
                0,
            )
        }
    };
    write_new(&root.private_path(&output)?, &value)?;
    Ok((value, code))
}
pub fn run_drawing(root: &SkillRoot, command: DrawingCommand) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_drawing")?;
    let (mut value, output, code) = match command {
        DrawingCommand::CheckDrafting {
            input,
            project,
            project_context,
            output,
        } => {
            let input_raw = raw(root, &input)?;
            let request: DrawingDraftingInput =
                serde_json::from_value(oe_core::json::strict_json(&input_raw)?)?;
            let project_raw = raw(root, &project)?;
            let p: oe_kernel::project::Project =
                serde_json::from_value(oe_core::json::strict_json(&project_raw)?)?;
            let context_raw = raw(root, &project_context)?;
            let context: oe_contracts::ProjectReviewContext =
                serde_json::from_value(oe_core::json::strict_json(&context_raw)?)?;
            let current =
                oe_kernel::project::check_current_context(&p, &request.plan.object, Some(&context));
            let declared = oe_kernel::drafting::check(&p, &request.plan);
            let native_source = oe_kernel::drafting::source_check(root, &p, &request.plan);
            // An unavailable source must not hide the declared plan or current
            // condition diagnoses. A fresh successful read is still mandatory.
            let observed_attempt = (|| -> Result<_> {
                let observation = drawing::drafting::observe(
                    &root.private_path(&request.job_directory)?,
                    &request.part_input,
                    &request.plan,
                    &request.native_result_ref,
                )?;
                let checked = drawing::drafting::check(&request.plan, &observation.part)?;
                Ok((observation, checked))
            })();
            let (observation, observed) = match observed_attempt {
                Ok((observation, checked)) => (Some(observation), checked),
                Err(error) => (
                    None,
                    json!({"schema":"oev2.native-drafting-check/1","conclusion":"unknown",
                    "reason":"drafting_native_job_observation_failed","error":error.to_string(),
                    "manufacturing_drawing":"not_established","engineering_adoption":"not_established"}),
                ),
            };
            let bound = observation.as_ref().is_some_and(|observation| {
                oe_kernel::engineering::basis(&p, &request.plan.object)
                    .ok()
                    .flatten()
                    .is_some_and(|b| {
                        b.facts.iter().any(|f| {
                            f.property == format!("native_drafting:{}", request.plan.sheet_name)
                                && f.status == "declared"
                                && f.value
                                    == drawing::intent::observation_value(observation).expect("DTO")
                        })
                    })
            });
            let usable = current.conclusion == "pass"
                && declared.conclusion == "pass"
                && native_source.conclusion == "pass"
                && observed["conclusion"] == "pass"
                && bound;
            ensure!(
                raw(root, &input)? == input_raw
                    && raw(root, &project)? == project_raw
                    && raw(root, &project_context)? == context_raw,
                "drafting_inputs_changed_during_check"
            );
            (
                json!({"schema":"oev2.current-drafting-sheet/1","plan":request.plan,
                "observation":observation,"native_drafting_check":observed,
                "current_project_check":current,"declared_plan_check":declared,"native_source_check":native_source,
                "project_observation_matches_native_job":bound,
                "current_use":if usable {"content_bound_prototype_drawing_observation"}else{"blocked_or_historical_only"},
                "input_sha256":oe_core::sha256(input_raw),"project_input_sha256":oe_core::sha256(project_raw),
                "manufacturing_drawing":"not_established","engineering_adoption":"not_established"}),
                output,
                if usable { 0 } else { 2 },
            )
        }
        DrawingCommand::CheckReferences {
            input,
            project,
            project_context,
            correspondence,
            output,
        } => {
            let input_raw = raw(root, &input)?;
            let request: DrawingReferenceInput =
                serde_json::from_value(oe_core::json::strict_json(&input_raw)?)?;
            let project_raw = raw(root, &project)?;
            let p: oe_kernel::project::Project =
                serde_json::from_value(oe_core::json::strict_json(&project_raw)?)?;
            let context_raw = raw(root, &project_context)?;
            let context: oe_contracts::ProjectReviewContext =
                serde_json::from_value(oe_core::json::strict_json(&context_raw)?)?;
            let current = oe_kernel::project::check_current_context(
                &p,
                &request.intent.object,
                Some(&context),
            );
            let observation = drawing::intent::observe_bound(
                &root.private_path(&request.job_directory)?,
                &request.part_input,
                &request.intent,
                &request.native_result_ref,
            )?;
            let observed = drawing::intent::check_points(&request.intent, &observation.part)?;
            let declared = oe_kernel::drawing_intent::check(&p, &request.intent);
            let native_source = oe_kernel::drawing_intent::source_check(root, &p, &request.intent);
            // The native job cannot be substituted for the observation already
            // declared in this Project. Preserve both diagnostics when stale.
            let bound = oe_kernel::engineering::basis(&p, &request.intent.object)
                .ok()
                .flatten()
                .is_some_and(|b| {
                    b.facts.iter().any(|f| {
                        f.property == format!("native_drawing:{}", request.intent.intent_id)
                            && f.status == "declared"
                            && f.value
                                == drawing::intent::observation_value(&observation).expect("DTO")
                    })
                });
            let correspondence_check = correspondence.as_ref().map(|path| {
                super::correspondence::drawing_check(
                    root,
                    path,
                    &project_raw,
                    &request.intent,
                    &observation,
                )
            });
            let usable = current.conclusion == "pass"
                && declared.conclusion == "pass"
                && native_source.conclusion == "pass"
                && observed["conclusion"] == "pass"
                && bound
                && correspondence_check
                    .as_ref()
                    .is_none_or(|check| check["conclusion"] == "pass");
            ensure!(
                raw(root, &input)? == input_raw && raw(root, &project)? == project_raw,
                "drawing_reference_inputs_changed_during_check"
            );
            let mut report = json!({"schema":"oev2.current-drawing-reference/1","intent":drawing::intent::intent_value(&request.intent)?,
                "observation":observation,"native_reference_check":observed,
                "current_project_check":current,"declared_intent_check":declared,"native_source_check":native_source,
                "project_observation_matches_native_job":bound,
                "current_use":if usable {"content_bound_reference_observation"}else{"blocked_or_historical_only"},
                "input_sha256":oe_core::sha256(input_raw),"project_input_sha256":oe_core::sha256(project_raw),
                "engineering_adoption":"not_established"});
            if let Some(mut check) = correspondence_check {
                ensure!(
                    raw(root, &project_context)? == context_raw,
                    "drawing_correspondence_context_changed_during_check"
                );
                let completion =
                    crate::v2::require_execution(root, "cad_drawing_correspondence_completion")?;
                for key in [
                    "normalizer",
                    "cli",
                    "semantic_contracts_sha256",
                    "executable_sha256",
                ] {
                    ensure!(
                        execution[key] == completion[key],
                        "drawing_correspondence_execution_identity_changed"
                    );
                }
                check["execution_recheck"] = completion;
                report["schema"] = json!("oev2.current-drawing-reference/2");
                report["correspondence_check"] = check;
            }
            (report, output, if usable { 0 } else { 2 })
        }
        DrawingCommand::AuditPacket { directory, output } => {
            let directory = if directory.is_absolute() {
                directory
            } else {
                root.expand(&directory)?
            };
            let mut value = drawing::audit_packet(&directory)?;
            value["source_directory"] = json!(directory);
            (value, output, 0)
        }
        DrawingCommand::ReceivePdf {
            job_directory,
            output,
        } => {
            let dir = root.private_path(&job_directory)?;
            (drawing::pdf::receive(&dir)?, output, 0)
        }
        DrawingCommand::DimensionInventory {
            job_directory,
            part_input,
            output,
        } => {
            let dir = root.private_path(&job_directory)?;
            (
                drawing::retirement::source_inventory(&dir, &part_input)?,
                output,
                0,
            )
        }
        DrawingCommand::CompareRetirement {
            saved_job,
            reopened_job,
            output,
        } => {
            let saved = root.private_path(&saved_job)?;
            let reopened = root.private_path(&reopened_job)?;
            let mut value = drawing::retirement::compare(&saved, &reopened)?;
            value["saved_job"] = json!(saved);
            value["reopened_job"] = json!(reopened);
            (value, output, 0)
        }
        DrawingCommand::CompareDrafting {
            saved_job,
            reopened_job,
            output,
        } => {
            let saved = root.private_path(&saved_job)?;
            let reopened = root.private_path(&reopened_job)?;
            let mut value = drawing::drafting::compare_saved_reopened(&saved, &reopened)?;
            let code = if value["conclusion"] == "pass" { 0 } else { 2 };
            value["saved_job"] = json!(saved);
            value["reopened_job"] = json!(reopened);
            (value, output, code)
        }
        DrawingCommand::CompareDiameter {
            saved_job,
            reopened_job,
            output,
        } => {
            let saved = root.private_path(&saved_job)?;
            let reopened = root.private_path(&reopened_job)?;
            let mut value = drawing::diameter::compare(&saved, &reopened)?;
            value["saved_job"] = json!(saved);
            value["reopened_job"] = json!(reopened);
            (value, output, 0)
        }
        DrawingCommand::CompareSheetReference {
            saved_job,
            reopened_job,
            output,
        } => {
            let saved = root.private_path(&saved_job)?;
            let reopened = root.private_path(&reopened_job)?;
            let mut value = drawing::associations::compare_sheet_reference(&saved, &reopened)?;
            let usable = value["parts"].as_array().is_some_and(|a| {
                !a.is_empty()
                    && a.iter().all(|p| {
                        p["observation_persistence"] == "matches"
                            && p["sheet_reference_check"]["conclusion"] == "pass"
                    })
            });
            value["saved_job"] = json!(saved);
            value["reopened_job"] = json!(reopened);
            (value, output, if usable { 0 } else { 2 })
        }
        DrawingCommand::CompareAssociations {
            saved_job,
            reopened_job,
            output,
        } => {
            let saved = root.private_path(&saved_job)?;
            let reopened = root.private_path(&reopened_job)?;
            let mut value = drawing::associations::compare(&saved, &reopened)?;
            let code = if value["parts"]
                .as_array()
                .is_some_and(|a| a.iter().all(|p| p["observation_persistence"] == "matches"))
            {
                0
            } else {
                2
            };
            value["saved_job"] = json!(saved);
            value["reopened_job"] = json!(reopened);
            (value, output, code)
        }
    };
    value["execution_binding"] = execution;
    write_new(&root.private_path(&output)?, &value)?;
    Ok((value, code))
}
pub fn run_motion(root: &SkillRoot, command: MotionCommand) -> Result<(Value, u8)> {
    let execution = crate::v2::require_execution(root, "cad_motion")?;
    let (mut result, output, code) = match command {
        MotionCommand::Receive { input, output } => {
            let (value, code) = super::motion_receiving::run(root, &input, false)?;
            (value, output, code)
        }
        MotionCommand::ReceiveReopen { input, output } => {
            let (value, code) = super::motion_receiving::run(root, &input, true)?;
            (value, output, code)
        }
        MotionCommand::CheckDrive { input, output } => {
            let contract: oe_contracts::DriveContract =
                serde_json::from_value(oe_core::json::strict_json(&raw(root, &input)?)?)?;
            (oe_cad::motion::check_drive(&contract)?, output, 0)
        }
        MotionCommand::ProjectV262 { input, output } => {
            let p: MotionProjection =
                serde_json::from_value(oe_core::json::strict_json(&raw(root, &input)?)?)?;
            let bytes = raw(root, &p.source.path)?;
            ensure!(
                oe_core::sha256(&bytes) == p.source.sha256,
                "motion_history_source_drift"
            );
            let source = oe_core::json::strict_json(&bytes)?;
            let mut value = oe_cad::motion::history::project_v262(
                &source,
                &p.object,
                &p.source_ref,
                &p.limits,
            )?;
            value["source"] = serde_json::to_value(&p.source)?;
            ensure!(
                raw(root, &p.source.path)? == bytes,
                "motion_history_source_changed"
            );
            (value, output, 0)
        }
    };
    result["execution_binding"] = execution;
    write_new(&root.private_path(&output)?, &result)?;
    Ok((result, code))
}
