use anyhow::{Context, Result, bail, ensure};
use oe_nx_protocol::RelativePath;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
pub mod component_copy;
pub mod drafting;
pub mod drawing_annotation;
pub mod drawing_display;
pub mod drawing_identification;
pub mod drawing_inventory;
pub mod drawing_pdf;
pub mod drawing_retirement;
pub mod drawing_update;
pub mod feature_add;
pub mod feature_delete;
pub mod mass_probe;
pub mod model_edges;
pub mod motion_task;
pub mod position_reopen;
pub mod rotating_part;
pub mod sheet_diameter;
pub mod sheet_dimensions;
pub mod source_assembly;
pub mod stepped_profile;
pub mod stepped_shaft;

pub const PARAMS_SCHEMA: &str = "cad-agent.nx-native-acceptance-params/v2";
pub const CONTROLLED_REFSET: &str = "OE_CONTROLLED_BODY";
pub const READBACK_SCHEMA: &str = "cad-agent.nx-native-readback/v2";
pub const REPRESENTATION_BASELINE: &str = "solid-with-current-associated-jt/v1";
pub const JT_MAPPING_SCHEMA: &str = "cad-agent.nx-body-jt-association/v1";
fn deserialize_capture_flag<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<bool>, D::Error> {
    bool::deserialize(deserializer).map(Some)
}
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Parameters {
    ExportDisplayDrawingPdf {
        schema: String,
        part: String,
        intent: oe_contracts::DrawingDisplayPdfIntent,
    },
    ExportIdentifiedDrawingPdf {
        schema: String,
        part: String,
        intent: oe_contracts::DrawingIdentifiedPdfIntent,
    },
    ExportDrawingPdf {
        schema: String,
        part: String,
        intent: oe_contracts::DrawingPdfIntent,
    },
    RetireDrawingDimensions {
        schema: String,
        part: String,
        intent: oe_contracts::DrawingRetirementIntent,
    },
    AddHoleFeature {
        schema: String,
        intent: oe_contracts::ArtifactReference,
        budget: oe_contracts::NxAssemblyBudget,
    },
    DeleteHoleFeature {
        schema: String,
        intent: oe_contracts::ArtifactReference,
        budget: oe_contracts::NxAssemblyBudget,
    },
    ProbeAssemblyMassProperties {
        schema: String,
        root_part: String,
        budget: oe_contracts::NxAssemblyBudget,
        repetitions: u32,
    },
    ProbeComponentCopyMassProperties {
        schema: String,
        intent: oe_contracts::ArtifactReference,
        budget: oe_contracts::NxAssemblyBudget,
        repetitions: u32,
    },
    CopyAssemblyComponent {
        schema: String,
        intent: oe_contracts::ArtifactReference,
        budget: oe_contracts::NxAssemblyBudget,
    },
    CreateSourceAssembly {
        schema: String,
        intent: oe_contracts::ArtifactReference,
        budget: oe_contracts::NxAssemblyBudget,
    },
    InspectAssemblyPositions {
        schema: String,
        source: oe_contracts::ArtifactReference,
        expected: oe_contracts::ArtifactReference,
        budget: oe_contracts::NxAssemblyBudget,
    },
    RunAssemblyMotion {
        schema: String,
        intent: oe_contracts::ArtifactReference,
        budget: oe_contracts::NxAssemblyBudget,
        evidence: oe_contracts::NxMotionEvidenceBudget,
    },
    CreateSheetDiameterDimension {
        schema: String,
        part: String,
        intent: oe_contracts::SheetDiameterIntent,
    },
    CreateSheetReferenceDimension {
        schema: String,
        part: String,
        sheet: oe_contracts::NativeDraftingPlan,
        reference: oe_contracts::DrawingReferenceIntent,
        text_origin_drawing: [f64; 2],
    },
    CreateSteppedShaft {
        schema: String,
        intent: oe_contracts::ArtifactReference,
    },
    InspectSteppedShaft {
        schema: String,
        part: String,
        intent: oe_contracts::ArtifactReference,
    },
    CreateCylinderSolid {
        schema: String,
        intent: oe_contracts::ArtifactReference,
    },
    InspectCylinderSolid {
        schema: String,
        part: String,
        intent: oe_contracts::ArtifactReference,
    },
    ImportStepAssembly {
        schema: String,
        step: String,
        budget: oe_contracts::NxAssemblyBudget,
    },
    CreateAssemblyFixture {
        schema: String,
        intent: oe_contracts::NxAssemblyFixtureIntent,
        budget: oe_contracts::NxAssemblyBudget,
    },
    CreateAssemblyFixtureNxopen {
        schema: String,
        intent: oe_contracts::NxAssemblyFixtureIntent,
        budget: oe_contracts::NxAssemblyBudget,
        construction_profile: oe_contracts::NxAssemblyConstructionProfile,
    },
    InspectAssembly {
        schema: String,
        root_part: String,
        budget: oe_contracts::NxAssemblyBudget,
        source_role: String,
        #[serde(default, deserialize_with = "deserialize_capture_flag")]
        capture_reference_sets: Option<bool>,
    },
    /// A prototype geometry sheet and one explicit named model view, on a copy.
    CreateDraftingSheet {
        schema: String,
        part: String,
        plan: oe_contracts::NativeDraftingPlan,
    },
    /// Add one diagnostic, associated model-space dimension to a job-owned copy.
    /// Sheet layout, tolerances, fabrication and physical adoption are separate.
    CreateReferenceDimension {
        schema: String,
        part: String,
        intent: oe_contracts::DrawingReferenceIntent,
        plane_origin: [f64; 3],
        plane_x_axis: [f64; 3],
        plane_y_axis: [f64; 3],
        text_origin_wcs: [f64; 3],
    },
    /// Fresh controlled plate from an explicit Rust design-intent projection.
    CreateHolePlate {
        schema: String,
        length_mm: f64,
        width_mm: f64,
        height_mm: f64,
        hole_centers_x_mm: Vec<f64>,
        hole_y_mm: f64,
        hole_diameter_mm: f64,
        design_input_sha256: String,
    },
    InspectDetailed {
        schema: String,
        parts: Vec<String>,
    },
    /// Maintain explicit job-owned copies only; never save an input asset.
    MaintainDrawings {
        schema: String,
        parts: Vec<String>,
        refresh_views: bool,
    },
    Create {
        schema: String,
        #[serde(default = "initial")]
        initial_length_mm: f64,
        #[serde(default = "updated")]
        updated_length_mm: f64,
        #[serde(default = "width")]
        width_mm: f64,
        #[serde(default = "height")]
        height_mm: f64,
        #[serde(default = "spacing")]
        spacing_mm: f64,
    },
    Inspect {
        schema: String,
        parts: Vec<String>,
    },
    ImportStep {
        schema: String,
        step: String,
    },
    /// Bounded observation of a real single-part multi-body STEP. The original
    /// controlled single-solid round-trip acceptance remains a separate action.
    ImportStepBodies {
        schema: String,
        step: String,
        expected_units: String,
        maximum_bodies: u32,
    },
    ImportParasolid {
        schema: String,
        parasolid: String,
    },
}
fn initial() -> f64 {
    40.0
}
fn updated() -> f64 {
    55.0
}
fn width() -> f64 {
    20.0
}
fn height() -> f64 {
    10.0
}
fn spacing() -> f64 {
    80.0
}
impl Parameters {
    pub fn action(&self) -> &'static str {
        match self {
            Self::AddHoleFeature { .. } => feature_add::ACTION,
            Self::DeleteHoleFeature { .. } => feature_delete::ACTION,
            Self::ProbeAssemblyMassProperties { .. } => "probe_assembly_mass_properties",
            Self::ProbeComponentCopyMassProperties { .. } => mass_probe::COPY_ACTION,
            Self::CopyAssemblyComponent { .. } => "copy_assembly_component",
            Self::InspectAssemblyPositions { .. } => "inspect_assembly_positions",
            Self::RunAssemblyMotion { .. } => "run_assembly_motion",
            Self::CreateSourceAssembly { .. } => "create_source_assembly",
            Self::ExportDisplayDrawingPdf { .. } => "export_display_drawing_pdf",
            Self::ExportIdentifiedDrawingPdf { .. } => "export_identified_drawing_pdf",
            Self::ExportDrawingPdf { .. } => "export_drawing_pdf",
            Self::RetireDrawingDimensions { .. } => "retire_drawing_dimensions",
            Self::CreateSheetDiameterDimension { .. } => "create_sheet_diameter_dimension",
            Self::CreateSheetReferenceDimension { .. } => "create_sheet_reference_dimension",
            Self::CreateSteppedShaft { .. } => "create_stepped_shaft",
            Self::InspectSteppedShaft { .. } => "inspect_stepped_shaft",
            Self::CreateCylinderSolid { .. } => "create_cylinder_solid",
            Self::InspectCylinderSolid { .. } => "inspect_cylinder_solid",
            Self::ImportStepAssembly { .. } => "import_step_assembly",
            Self::CreateAssemblyFixture { .. } => "create_assembly_fixture",
            Self::CreateAssemblyFixtureNxopen { .. } => "create_assembly_fixture_nxopen",
            Self::InspectAssembly { .. } => "inspect_assembly",
            Self::CreateDraftingSheet { .. } => "create_drafting_sheet",
            Self::CreateReferenceDimension { .. } => "create_reference_dimension",
            Self::CreateHolePlate { .. } => "create_hole_plate",
            Self::InspectDetailed { .. } => "inspect_detailed",
            Self::MaintainDrawings { .. } => "maintain_drawings",
            Self::Create { .. } => "create",
            Self::Inspect { .. } => "inspect",
            Self::ImportStep { .. } => "import_step",
            Self::ImportStepBodies { .. } => "import_step_bodies",
            Self::ImportParasolid { .. } => "import_parasolid",
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::ExportDisplayDrawingPdf {
                schema,
                part,
                intent,
            } => {
                ensure!(
                    schema == drawing_display::params_schema(intent),
                    "display_pdf_params_schema_mismatch"
                );
                input_relative(part)?;
                ensure!(
                    part.to_ascii_lowercase().ends_with(".prt"),
                    "display_pdf_native_prt_required"
                );
                return drawing_display::validate(intent);
            }
            Self::ExportIdentifiedDrawingPdf {
                schema,
                part,
                intent,
            } => {
                ensure!(
                    schema == drawing_identification::params_schema(intent),
                    "identified_pdf_params_schema_mismatch"
                );
                input_relative(part)?;
                ensure!(
                    part.to_ascii_lowercase().ends_with(".prt"),
                    "identified_pdf_native_prt_required"
                );
                return drawing_identification::validate(intent);
            }
            Self::ExportDrawingPdf {
                schema,
                part,
                intent,
            } => {
                ensure!(
                    schema == drawing_pdf::params_schema(intent),
                    "drawing_pdf_params_schema_mismatch"
                );
                input_relative(part)?;
                ensure!(
                    part.to_ascii_lowercase().ends_with(".prt"),
                    "drawing_pdf_native_prt_required"
                );
                return drawing_pdf::validate(intent);
            }
            Self::RetireDrawingDimensions {
                schema,
                part,
                intent,
            } => {
                ensure!(
                    schema == drawing_retirement::PARAMS_SCHEMA,
                    "retirement_params_schema_mismatch"
                );
                input_relative(part)?;
                ensure!(
                    part.to_ascii_lowercase().ends_with(".prt"),
                    "retirement_native_prt_required"
                );
                return drawing_retirement::validate(intent);
            }
            Self::AddHoleFeature {
                schema,
                intent,
                budget,
            } => {
                ensure!(
                    schema == feature_add::PARAMS_SCHEMA,
                    "add_params_schema_invalid"
                );
                motion_task::reference_valid(intent)?;
                return crate::assembly::budget_valid(budget);
            }
            Self::DeleteHoleFeature {
                schema,
                intent,
                budget,
            } => {
                ensure!(
                    schema == feature_delete::PARAMS_SCHEMA,
                    "delete_params_schema_invalid"
                );
                motion_task::reference_valid(intent)?;
                return crate::assembly::budget_valid(budget);
            }
            Self::ProbeAssemblyMassProperties {
                schema,
                root_part,
                budget,
                repetitions,
            } => {
                ensure!(
                    schema == mass_probe::PARAMS_SCHEMA,
                    "mass_probe_params_schema_invalid"
                );
                input_relative(root_part)?;
                ensure!(
                    root_part.to_ascii_lowercase().ends_with(".prt"),
                    "mass_probe_native_prt_required"
                );
                mass_probe::repetitions_valid(*repetitions)?;
                return crate::assembly::budget_valid(budget);
            }
            Self::ProbeComponentCopyMassProperties {
                schema,
                intent,
                budget,
                repetitions,
            } => {
                ensure!(
                    schema == mass_probe::COPY_PARAMS_SCHEMA,
                    "mass_probe_copy_params_schema_invalid"
                );
                motion_task::reference_valid(intent)?;
                mass_probe::repetitions_valid(*repetitions)?;
                return crate::assembly::budget_valid(budget);
            }
            Self::CopyAssemblyComponent {
                schema,
                intent,
                budget,
            } => {
                ensure!(
                    schema == component_copy::PARAMS_SCHEMA,
                    "copy_params_schema_invalid"
                );
                motion_task::reference_valid(intent)?;
                return crate::assembly::budget_valid(budget);
            }
            Self::InspectAssemblyPositions {
                schema,
                source,
                expected,
                budget,
            } => {
                ensure!(
                    schema == position_reopen::PARAMS_SCHEMA,
                    "position_reopen_params_schema_mismatch"
                );
                motion_task::reference_valid(source)?;
                motion_task::reference_valid(expected)?;
                return crate::assembly::budget_valid(budget);
            }
            Self::CreateSourceAssembly {
                schema,
                intent,
                budget,
            } => {
                ensure!(
                    schema == source_assembly::PARAMS_SCHEMA,
                    "build_params_schema_invalid"
                );
                motion_task::reference_valid(intent)?;
                return crate::assembly::budget_valid(budget);
            }
            Self::RunAssemblyMotion {
                schema,
                intent,
                budget,
                evidence,
            } => {
                ensure!(
                    matches!(
                        schema.as_str(),
                        motion_task::PARAMS_SCHEMA | motion_task::capture2::PARAMS_SCHEMA
                    ),
                    "motion_params_schema_mismatch"
                );
                motion_task::reference_valid(intent)?;
                crate::assembly::budget_valid(budget)?;
                if schema == motion_task::capture2::PARAMS_SCHEMA {
                    oe_contracts::NxAssemblyMotionParams2 {
                        schema: schema.clone(),
                        action: "run_assembly_motion".into(),
                        intent: intent.clone(),
                        budget: budget.clone(),
                        evidence: evidence.clone(),
                    }
                    .validate()
                    .map_err(|_| anyhow::anyhow!("motion_params2_generated_invalid"))?;
                }
                ensure!(
                    evidence.max_bytes <= oe_nx_protocol::MAX_PAYLOAD_BYTES,
                    "motion_legacy_delivery_budget_exceeded"
                );
                return Ok(());
            }
            Self::CreateSheetDiameterDimension {
                schema,
                part,
                intent,
            } => {
                ensure!(
                    schema == sheet_diameter::params_schema(intent),
                    "diameter_params_schema_mismatch"
                );
                input_relative(part)?;
                ensure!(
                    part.to_ascii_lowercase().ends_with(".prt"),
                    "diameter_native_prt_required"
                );
                return sheet_diameter::validate(intent);
            }
            Self::CreateSheetReferenceDimension {
                schema,
                part,
                sheet,
                reference,
                text_origin_drawing,
            } => {
                ensure!(
                    schema == "oev2.nx-sheet-dimension-params/1",
                    "sheet_dimension_params_schema_mismatch"
                );
                input_relative(part)?;
                ensure!(
                    part.to_ascii_lowercase().ends_with(".prt"),
                    "sheet_dimension_native_prt_required"
                );
                return sheet_dimensions::validate(sheet, reference, *text_origin_drawing);
            }
            Self::CreateSteppedShaft { schema, intent }
            | Self::InspectSteppedShaft { schema, intent, .. } => {
                ensure!(
                    schema == stepped_shaft::PARAMS_SCHEMA,
                    "stepped_params_schema_mismatch"
                );
                intent
                    .validate()
                    .map_err(|_| anyhow::anyhow!("stepped_reference_invalid"))?;
                ensure!(
                    !intent.path.starts_with("inputs/"),
                    "stepped_unprefixed_member_required"
                );
                input_relative(&intent.path)?;
                if let Self::InspectSteppedShaft { part, .. } = self {
                    input_relative(part)?;
                    ensure!(
                        part.to_ascii_lowercase().ends_with(".prt"),
                        "stepped_inspect_prt_required"
                    );
                }
                return Ok(());
            }
            Self::CreateCylinderSolid { schema, intent }
            | Self::InspectCylinderSolid { schema, intent, .. } => {
                ensure!(
                    schema == rotating_part::PARAMS_SCHEMA,
                    "cylinder_params_schema_mismatch"
                );
                intent
                    .validate()
                    .map_err(|_| anyhow::anyhow!("cylinder_intent_reference_invalid"))?;
                ensure!(
                    !intent.path.starts_with("inputs/"),
                    "cylinder_intent_requires_unprefixed_worker_member"
                );
                input_relative(&intent.path)?;
                if let Self::InspectCylinderSolid { part, .. } = self {
                    input_relative(part)?;
                    ensure!(
                        part.to_ascii_lowercase().ends_with(".prt"),
                        "cylinder_inspect_prt_required"
                    );
                }
                return Ok(());
            }
            Self::CreateAssemblyFixtureNxopen {
                schema,
                intent,
                budget,
                construction_profile,
            } => {
                ensure!(
                    schema == crate::assembly::NXOPEN_PARAMS_SCHEMA,
                    "nxopen_assembly_params_schema_mismatch"
                );
                crate::assembly::validate_construction_profile(construction_profile)?;
                crate::assembly::validate_fixture_intent(intent)?;
                for e in &intent.instances {
                    crate::assembly::nxopen_column_rotation(&e.rotation)?;
                }
                return crate::assembly::budget_valid(budget);
            }
            Self::ImportStepAssembly {
                schema,
                step,
                budget,
            } => {
                ensure!(
                    schema == crate::assembly::PARAMS_SCHEMA,
                    "assembly_params_schema_mismatch"
                );
                input_relative(step)?;
                ensure!(
                    step.to_ascii_lowercase().ends_with(".step")
                        || step.to_ascii_lowercase().ends_with(".stp"),
                    "assembly_step_name_required"
                );
                return crate::assembly::budget_valid(budget);
            }
            Self::CreateAssemblyFixture {
                schema,
                intent,
                budget,
            } => {
                ensure!(
                    schema == crate::assembly::PARAMS_SCHEMA,
                    "assembly_params_schema_mismatch"
                );
                crate::assembly::validate_fixture_intent(intent)?;
                return crate::assembly::budget_valid(budget);
            }
            Self::InspectAssembly {
                schema,
                root_part,
                budget,
                source_role,
                capture_reference_sets,
            } => {
                ensure!(
                    schema
                        == if *capture_reference_sets == Some(true) {
                            crate::assembly_reference_sets::PARAMS_SCHEMA
                        } else {
                            crate::assembly::PARAMS_SCHEMA
                        },
                    "assembly_params_schema_mismatch"
                );
                ensure!(
                    capture_reference_sets.is_none() || *capture_reference_sets == Some(true),
                    "assembly_reference_set_capture_must_be_explicit_true"
                );
                input_relative(root_part)?;
                ensure!(
                    root_part.to_ascii_lowercase().ends_with(".prt"),
                    "assembly_prt_name_required"
                );
                ensure!(
                    matches!(
                        source_role.as_str(),
                        "reference_observation" | "independent_software_fixture"
                    ),
                    "assembly_source_role_unknown"
                );
                return crate::assembly::budget_valid(budget);
            }
            _ => {}
        }
        let schema = match self {
            Self::AddHoleFeature { .. }
            | Self::DeleteHoleFeature { .. }
            | Self::ProbeAssemblyMassProperties { .. }
            | Self::ProbeComponentCopyMassProperties { .. }
            | Self::CopyAssemblyComponent { .. }
            | Self::CreateSourceAssembly { .. }
            | Self::InspectAssemblyPositions { .. }
            | Self::RunAssemblyMotion { .. }
            | Self::ExportDisplayDrawingPdf { .. }
            | Self::ExportIdentifiedDrawingPdf { .. }
            | Self::ExportDrawingPdf { .. }
            | Self::RetireDrawingDimensions { .. }
            | Self::CreateSheetDiameterDimension { .. }
            | Self::CreateSheetReferenceDimension { .. }
            | Self::CreateSteppedShaft { .. }
            | Self::InspectSteppedShaft { .. }
            | Self::CreateCylinderSolid { .. }
            | Self::InspectCylinderSolid { .. }
            | Self::ImportStepAssembly { .. }
            | Self::CreateAssemblyFixture { .. }
            | Self::CreateAssemblyFixtureNxopen { .. }
            | Self::InspectAssembly { .. } => unreachable!(),
            Self::CreateDraftingSheet { schema, .. }
            | Self::CreateReferenceDimension { schema, .. }
            | Self::CreateHolePlate { schema, .. }
            | Self::InspectDetailed { schema, .. }
            | Self::MaintainDrawings { schema, .. }
            | Self::Create { schema, .. }
            | Self::Inspect { schema, .. }
            | Self::ImportStep { schema, .. }
            | Self::ImportStepBodies { schema, .. }
            | Self::ImportParasolid { schema, .. } => schema,
        };
        ensure!(schema == PARAMS_SCHEMA, "native_params_schema_mismatch");
        match self {
            Self::AddHoleFeature { .. }
            | Self::DeleteHoleFeature { .. }
            | Self::ProbeAssemblyMassProperties { .. }
            | Self::ProbeComponentCopyMassProperties { .. }
            | Self::CopyAssemblyComponent { .. }
            | Self::CreateSourceAssembly { .. }
            | Self::InspectAssemblyPositions { .. }
            | Self::RunAssemblyMotion { .. }
            | Self::ExportDisplayDrawingPdf { .. }
            | Self::ExportIdentifiedDrawingPdf { .. }
            | Self::ExportDrawingPdf { .. }
            | Self::RetireDrawingDimensions { .. }
            | Self::CreateSheetDiameterDimension { .. }
            | Self::CreateSheetReferenceDimension { .. }
            | Self::CreateSteppedShaft { .. }
            | Self::InspectSteppedShaft { .. }
            | Self::CreateCylinderSolid { .. }
            | Self::InspectCylinderSolid { .. }
            | Self::ImportStepAssembly { .. }
            | Self::CreateAssemblyFixture { .. }
            | Self::CreateAssemblyFixtureNxopen { .. }
            | Self::InspectAssembly { .. } => unreachable!(),
            Self::CreateDraftingSheet { part, plan, .. } => {
                input_relative(part)?;
                ensure!(
                    part.to_ascii_lowercase().ends_with(".prt"),
                    "drafting_requires_native_prt"
                );
                drafting::validate(plan)?;
            }
            Self::CreateReferenceDimension {
                part,
                intent,
                plane_origin,
                plane_x_axis,
                plane_y_axis,
                text_origin_wcs,
                ..
            } => {
                input_relative(part)?;
                ensure!(
                    part.to_ascii_lowercase().ends_with(".prt"),
                    "reference_dimension_requires_native_prt"
                );
                intent
                    .validate()
                    .map_err(|_| anyhow::anyhow!("reference_dimension_intent_invalid"))?;
                ensure!(
                    matches!(
                        intent.schema.as_str(),
                        "oev2.drawing-reference-intent/2" | "oev2.drawing-reference-intent/3"
                    ) && intent.proof_scope == "unique_model_parent"
                        && intent.points.len() == 2
                        && intent.position_tolerance >= 0.0
                        && intent.position_tolerance.is_finite()
                        && !intent.source_refs.is_empty(),
                    "reference_dimension_intent_scope_invalid"
                );
                let value = intent
                    .dimension_value
                    .as_ref()
                    .context("reference_dimension_value_missing")?;
                value
                    .validate()
                    .map_err(|_| anyhow::anyhow!("reference_dimension_value_invalid"))?;
                ensure!(
                    value.metric == "computed_linear"
                        && value.value > 0.0
                        && value.numeric_tolerance >= 0.0
                        && value.unit == intent.unit
                        && value.dimension_subtype_raw == 3,
                    "reference_dimension_value_scope_invalid"
                );
                ensure!(
                    plane_origin
                        .iter()
                        .chain(plane_x_axis.iter())
                        .chain(plane_y_axis.iter())
                        .chain(text_origin_wcs.iter())
                        .all(|n| n.is_finite()),
                    "reference_dimension_plane_nonfinite"
                );
                let norm = |v: [f64; 3]| v[0].hypot(v[1]).hypot(v[2]);
                let normal = model_edges::cross(*plane_x_axis, *plane_y_axis);
                ensure!(
                    (norm(*plane_x_axis) - 1.0).abs() <= 1e-9
                        && (norm(*plane_y_axis) - 1.0).abs() <= 1e-9
                        && (norm(normal) - 1.0).abs() <= 1e-9,
                    "reference_dimension_plane_not_orthonormal"
                );
                for p in &intent.points {
                    p.validate()
                        .map_err(|_| anyhow::anyhow!("reference_dimension_point_invalid"))?;
                    let e = model_edges::CircleIntentRef::for_point(&intent.schema, p)?
                        .context("reference_dimension_parent_edge_missing")?;
                    e.validate()?;
                    ensure!(
                        e.full_circle()
                            && e.unit() == intent.unit
                            && e.local_frame() == intent.local_frame
                            && p.position.len() == 3
                            && p.position.iter().all(|n| n.is_finite())
                            && p.parent_type_raw == 70
                            && p.parent_subtype_raw == 3
                            && p.point_type_raw == 2
                            && p.line_type_raw == 0,
                        "reference_dimension_parent_edge_scope_invalid"
                    );
                    let center = e.center();
                    let edge_normal = e.normal();
                    let distance = norm([
                        p.position[0] - center[0],
                        p.position[1] - center[1],
                        p.position[2] - center[2],
                    ]);
                    let dot = (0..3)
                        .map(|i| normal[i] * edge_normal[i])
                        .sum::<f64>()
                        .abs()
                        .min(1.0);
                    let plane_distance = (0..3)
                        .map(|i| normal[i] * (p.position[i] - plane_origin[i]))
                        .sum::<f64>()
                        .abs();
                    ensure!(
                        distance <= intent.position_tolerance
                            && plane_distance <= intent.position_tolerance
                            && dot.acos() <= e.angular_tolerance(),
                        "reference_dimension_plane_or_center_mismatch"
                    );
                }
                let a = &intent.points[0].position;
                let b = &intent.points[1].position;
                let distance = (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2]);
                ensure!(
                    intent.points[0].point_id != intent.points[1].point_id
                        && model_edges::CircleIntentRef::for_point(
                            &intent.schema,
                            &intent.points[0]
                        )?
                        .unwrap()
                        .feature_id()
                            != model_edges::CircleIntentRef::for_point(
                                &intent.schema,
                                &intent.points[1]
                            )?
                            .unwrap()
                            .feature_id()
                        && (distance - value.value).abs() <= value.numeric_tolerance,
                    "reference_dimension_independent_value_mismatch"
                );
            }
            Self::CreateHolePlate {
                length_mm,
                width_mm,
                height_mm,
                hole_centers_x_mm,
                hole_y_mm,
                hole_diameter_mm,
                design_input_sha256,
                ..
            } => {
                for v in [length_mm, width_mm, height_mm, hole_diameter_mm] {
                    ensure!(
                        v.is_finite() && *v > 0.0 && *v <= 1e6,
                        "hole_native_dimensions_invalid"
                    );
                }
                ensure!(
                    hole_y_mm.is_finite()
                        && !hole_centers_x_mm.is_empty()
                        && hole_centers_x_mm.len() <= 128,
                    "hole_native_center_budget"
                );
                let r = hole_diameter_mm / 2.0;
                ensure!(
                    hole_y_mm.abs() + r < *width_mm / 2.0
                        && hole_centers_x_mm
                            .iter()
                            .all(|x| x.is_finite() && x.abs() + r < *length_mm / 2.0),
                    "hole_native_cylinder_outside_plate"
                );
                ensure!(
                    design_input_sha256.len() == 64
                        && design_input_sha256
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                    "hole_native_design_digest_invalid"
                );
            }
            Self::Create {
                initial_length_mm,
                updated_length_mm,
                width_mm,
                height_mm,
                spacing_mm,
                ..
            } => {
                for x in [initial_length_mm, updated_length_mm, width_mm, height_mm] {
                    ensure!(
                        x.is_finite() && *x > 0.0 && *x <= 1.0e6,
                        "invalid_mm_dimension"
                    );
                }
                ensure!(
                    spacing_mm.is_finite() && spacing_mm.abs() <= 1.0e6,
                    "invalid_occurrence_spacing"
                );
            }
            Self::Inspect { parts, .. }
            | Self::InspectDetailed { parts, .. }
            | Self::MaintainDrawings { parts, .. } => {
                ensure!(
                    !parts.is_empty() && parts.len() <= 128,
                    "invalid_inspect_part_count"
                );
                let mut names = std::collections::BTreeSet::new();
                for name in parts {
                    let p = input_relative(name)?;
                    ensure!(
                        name.to_ascii_lowercase().ends_with(".prt"),
                        "inspect_requires_native_prt"
                    );
                    ensure!(
                        names.insert(oe_nx_protocol::path_collision_key(p.as_str())),
                        "duplicate_inspect_part"
                    );
                }
            }
            Self::ImportStep { step, .. } | Self::ImportStepBodies { step, .. } => {
                input_relative(step)?;
                ensure!(
                    [".step", ".stp"]
                        .iter()
                        .any(|s| step.to_ascii_lowercase().ends_with(s)),
                    "import_requires_step_name"
                );
                if let Self::ImportStepBodies {
                    expected_units,
                    maximum_bodies,
                    ..
                } = self
                {
                    ensure!(
                        expected_units == "mm" && (1..=128).contains(maximum_bodies),
                        "step_bodies_observation_scope_invalid"
                    );
                }
            }
            Self::ImportParasolid { parasolid, .. } => {
                input_relative(parasolid)?;
                ensure!(
                    [".x_b", ".x_t"]
                        .iter()
                        .any(|suffix| parasolid.to_ascii_lowercase().ends_with(suffix)),
                    "import_requires_parasolid_name"
                );
            }
        }
        Ok(())
    }
    pub fn parameter_schema(&self) -> &'static str {
        match self {
            Self::AddHoleFeature { .. } => feature_add::PARAMS_SCHEMA,
            Self::DeleteHoleFeature { .. } => feature_delete::PARAMS_SCHEMA,
            Self::ProbeAssemblyMassProperties { .. } => mass_probe::PARAMS_SCHEMA,
            Self::ProbeComponentCopyMassProperties { .. } => mass_probe::COPY_PARAMS_SCHEMA,
            Self::CopyAssemblyComponent { .. } => component_copy::PARAMS_SCHEMA,
            Self::InspectAssemblyPositions { .. } => position_reopen::PARAMS_SCHEMA,
            Self::RunAssemblyMotion { schema, .. }
                if schema == motion_task::capture2::PARAMS_SCHEMA =>
            {
                motion_task::capture2::PARAMS_SCHEMA
            }
            Self::RunAssemblyMotion { .. } => motion_task::PARAMS_SCHEMA,
            Self::CreateSourceAssembly { .. } => source_assembly::PARAMS_SCHEMA,
            Self::ExportDisplayDrawingPdf { intent, .. } => drawing_display::params_schema(intent),
            Self::ExportIdentifiedDrawingPdf { intent, .. } => {
                drawing_identification::params_schema(intent)
            }
            Self::ExportDrawingPdf { intent, .. } => drawing_pdf::params_schema(intent),
            Self::RetireDrawingDimensions { .. } => drawing_retirement::PARAMS_SCHEMA,
            Self::CreateSheetDiameterDimension { intent, .. } => {
                sheet_diameter::params_schema(intent)
            }
            Self::CreateSheetReferenceDimension { .. } => "oev2.nx-sheet-dimension-params/1",
            Self::CreateSteppedShaft { .. } | Self::InspectSteppedShaft { .. } => {
                stepped_shaft::PARAMS_SCHEMA
            }
            Self::CreateCylinderSolid { .. } | Self::InspectCylinderSolid { .. } => {
                rotating_part::PARAMS_SCHEMA
            }
            Self::CreateAssemblyFixtureNxopen { .. } => crate::assembly::NXOPEN_PARAMS_SCHEMA,
            Self::InspectAssembly {
                capture_reference_sets: Some(true),
                ..
            } => crate::assembly_reference_sets::PARAMS_SCHEMA,
            Self::ImportStepAssembly { .. }
            | Self::CreateAssemblyFixture { .. }
            | Self::InspectAssembly { .. } => crate::assembly::PARAMS_SCHEMA,
            Self::CreateDraftingSheet { .. }
            | Self::CreateReferenceDimension { .. }
            | Self::CreateHolePlate { .. }
            | Self::InspectDetailed { .. }
            | Self::MaintainDrawings { .. }
            | Self::Create { .. }
            | Self::Inspect { .. }
            | Self::ImportStep { .. }
            | Self::ImportStepBodies { .. }
            | Self::ImportParasolid { .. } => PARAMS_SCHEMA,
        }
    }
}
pub fn input_relative(name: &str) -> Result<RelativePath> {
    ensure!(name.is_ascii(), "controlled_case_requires_ascii_path");
    RelativePath::new(format!("inputs/{name}"))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expression {
    pub name: String,
    pub rhs: String,
    pub value: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub prototype_path: String,
    pub reference_set: String,
    pub origin: [f64; 3],
    pub matrix: [f64; 9],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlledReferenceSet {
    pub name: String,
    pub found: bool,
    pub reference_set_tag: u32,
    pub member_tags: Vec<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JtMapping {
    pub schema: String,
    pub owner_tag: u32,
    pub solid_tag: u32,
    pub reference_set_tag: u32,
    pub member_tags_before: Vec<u32>,
    pub member_tags_after: Option<Vec<u32>>,
    pub reference_set_after: Option<u32>,
    pub wrapper_code: i32,
    pub call_entered: bool,
    pub call_returned: bool,
    pub complete: bool,
    pub exception_code: Option<i32>,
    pub solid_owner_tag: Option<u32>,
    pub jt_tag: Option<u32>,
    pub up_to_date: Option<bool>,
    pub type_query_code: Option<i32>,
    pub jt_type: Option<i32>,
    pub jt_subtype: Option<i32>,
    pub owner_query_code: Option<i32>,
    pub jt_owner_tag: Option<u32>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartInfo {
    pub path: String,
    pub part_tag: u32,
    pub representation_baseline: String,
    pub controlled_jt_mapping: Option<JtMapping>,
    pub units: String,
    pub solid_count: usize,
    pub solid_tags: Vec<u32>,
    pub controlled_reference_set: ControlledReferenceSet,
    pub sheet_count: usize,
    pub bbox: Vec<[f64; 6]>,
    pub bbox_role: String,
    pub planar_vertex_bounds: Vec<Option<[f64; 6]>>,
    #[serde(default)]
    pub exact_work_csys_bounds: Vec<ExactBounds>,
    pub expressions: Vec<Expression>,
    pub components: Vec<Component>,
}
/// NX's accurate CSYS-aligned box, retaining the frame and all eight corners.
/// This is not a measured tolerance or a proof of a detailed winding model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExactBounds {
    pub min_corner: [f64; 3],
    pub directions: [[f64; 3]; 3],
    pub distances: [f64; 3],
    pub absolute_axis_bounds: [f64; 6],
}
impl ExactBounds {
    pub fn new(
        min_corner: [f64; 3],
        directions: [[f64; 3]; 3],
        distances: [f64; 3],
    ) -> Result<Self> {
        ensure!(
            min_corner
                .iter()
                .chain(directions.iter().flatten())
                .chain(distances.iter())
                .all(|x| x.is_finite()),
            "native_exact_bounds_nonfinite"
        );
        ensure!(
            distances.iter().all(|x| *x >= 0.0),
            "native_exact_bounds_negative_distance"
        );
        for (i, direction) in directions.iter().enumerate() {
            for (j, other) in directions.iter().enumerate() {
                let dot: f64 = direction.iter().zip(other).map(|(a, b)| a * b).sum();
                ensure!(
                    (dot - if i == j { 1.0 } else { 0.0 }).abs() <= 1e-8,
                    "native_exact_bounds_frame_invalid"
                );
            }
        }
        let mut lower = [f64::INFINITY; 3];
        let mut upper = [f64::NEG_INFINITY; 3];
        for mask in 0..8 {
            let mut corner = min_corner;
            for (axis, direction) in directions.iter().enumerate() {
                if mask & (1 << axis) != 0 {
                    for (coordinate, component) in corner.iter_mut().zip(direction) {
                        *coordinate += component * distances[axis];
                    }
                }
            }
            ensure!(
                corner.iter().all(|x| x.is_finite()),
                "native_exact_bounds_corner_overflow"
            );
            for i in 0..3 {
                lower[i] = lower[i].min(corner[i]);
                upper[i] = upper[i].max(corner[i]);
            }
        }
        Ok(Self {
            min_corner,
            directions,
            distances,
            absolute_axis_bounds: [lower[0], lower[1], lower[2], upper[0], upper[1], upper[2]],
        })
    }
}
pub trait NativeApi {
    fn feature_inventory(
        &mut self,
        _part: u64,
        _body: u64,
    ) -> Result<oe_contracts::NxFeatureInventory1> {
        bail!("native_feature_inventory_unsupported")
    }
    fn delete_feature(
        &mut self,
        _part: u64,
        _body: u64,
        _feature: u64,
    ) -> Result<oe_contracts::NxFeatureDeleteOutcome1> {
        bail!("native_feature_delete_unsupported")
    }
    fn geometrical_mass_properties(
        &mut self,
        _body: u64,
        _owning_part: u64,
    ) -> Result<oe_contracts::NxGeometricalMassObservation1> {
        bail!("native_geometrical_mass_probe_unsupported")
    }
    fn copy_assembly_component(
        &mut self,
        _parent: u32,
        _source: &oe_contracts::NxAssemblyOccurrenceObservation,
        _name: &str,
    ) -> Result<oe_contracts::NxComponentCopyOutcome1> {
        bail!("native_component_copy_unsupported")
    }
    fn persist_load_diagnostics(
        &mut self,
        _record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<()> {
        bail!("native_diagnostic_stream_unsupported")
    }
    fn loaded_assembly_files(&mut self, _namespace: &Path) -> Result<Vec<String>> {
        bail!("native_loaded_part_inventory_unsupported")
    }
    fn assembly_positions(
        &mut self,
        _part: u32,
        _namespace: &Path,
        _budget: &oe_contracts::NxAssemblyBudget,
        _role: &str,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        bail!("native_assembly_positions_unsupported")
    }
    fn move_assembly_sample2(
        &mut self,
        _part: u32,
        _namespace: &Path,
        _budget: &oe_contracts::NxAssemblyBudget,
        _sample: &oe_contracts::NxAssemblyMotionSample,
        _record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        bail!("native_assembly_motion_capture2_unsupported")
    }
    fn check_assembly_pairs2(
        &mut self,
        _part: u32,
        _namespace: &Path,
        _budget: &oe_contracts::NxAssemblyBudget,
        _motion_budget: &oe_contracts::NxAssemblyMotionBudget,
        _total_samples: u64,
        _sample_index: u64,
        _sample_id: &str,
        _record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<Value> {
        bail!("native_assembly_interference_capture2_unsupported")
    }
    fn save_all_owned_assembly2(
        &mut self,
        _namespace: &Path,
        _sink: &mut dyn motion_task::save_capture2::SaveCaptureSink2,
    ) -> Result<Value> {
        bail!("native_full_assembly_save_capture2_unsupported")
    }
    fn move_assembly_sample(
        &mut self,
        _part: u32,
        _namespace: &Path,
        _budget: &oe_contracts::NxAssemblyBudget,
        _sample: &oe_contracts::NxAssemblyMotionSample,
        _record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<oe_contracts::NxAssemblyPositionObservation> {
        bail!("native_assembly_motion_unsupported")
    }
    fn check_assembly_pairs(
        &mut self,
        _part: u32,
        _namespace: &Path,
        _budget: &oe_contracts::NxAssemblyBudget,
        _motion_budget: &oe_contracts::NxAssemblyMotionBudget,
        _total_samples: u64,
        _record: &mut dyn FnMut(Value) -> Result<()>,
    ) -> Result<Value> {
        bail!("native_assembly_interference_unsupported")
    }

    fn drawing_display_inventory(&mut self, _part: u32, _view: u32) -> Result<Value> {
        bail!("native_drawing_display_inventory_unsupported")
    }
    fn restore_drawing_wcs_identity(&mut self, _part: u32, _original: u32) -> Result<Value> {
        bail!("native_drawing_wcs_identity_restore_unsupported")
    }
    fn drawing_csys_blank(&mut self, _part: u32, _tag: u32, _blank: bool) -> Result<Value> {
        bail!("native_drawing_csys_blank_unsupported")
    }
    fn dimension_annotation_layout(
        &mut self,
        _part: u32,
        _tag: u32,
        _origin: Option<[f64; 3]>,
        _centered: Option<bool>,
    ) -> Result<Value> {
        bail!("native_dimension_annotation_layout_unsupported")
    }
    fn dimension_label_lines_updated(
        &mut self,
        _part: u32,
        _dimension: u32,
        _label: &str,
    ) -> Result<Value> {
        bail!("native_dimension_local_update_unsupported")
    }
    fn dimension_label_lines(
        &mut self,
        _part: u32,
        _tag: u32,
        _label: Option<&str>,
    ) -> Result<Value> {
        bail!("native_dimension_label_lines_unsupported")
    }
    fn dimension_label(
        &mut self,
        _part: u32,
        _dimension: u32,
        _label: Option<&str>,
    ) -> Result<Value> {
        bail!("native_dimension_label_unsupported")
    }
    fn drawing_wcs(&mut self, _part: u32, _visible: Option<bool>) -> Result<Value> {
        bail!("native_drawing_wcs_unsupported")
    }
    fn export_drawing_pdf(&mut self, _part: u32, _sheet: u32, _output: &Path) -> Result<Value> {
        bail!("native_drawing_pdf_unsupported")
    }
    fn retire_retained_dimension(&mut self, _part: u32, _dimension: u32) -> Result<Value> {
        bail!("native_drawing_retirement_unsupported")
    }
    fn create_sheet_diameter(
        &mut self,
        _sheet: u32,
        _view: u32,
        _edge: u32,
        _text: [f64; 2],
    ) -> Result<Value> {
        bail!("native_sheet_diameter_unsupported")
    }
    fn create_sheet_parallel_reference(
        &mut self,
        _sheet: u32,
        _view: u32,
        _edges: [u32; 2],
        _text: [f64; 2],
    ) -> Result<Value> {
        bail!("native_sheet_reference_dimension_unsupported")
    }
    fn import_assembly_preserved(&mut self, _source: &Path, _target: &Path) -> Result<()> {
        bail!("native_assembly_import_unsupported")
    }
    fn create_assembly_units_part(&mut self, _path: &Path, _unit: &str) -> Result<u32> {
        bail!("native_assembly_units_part_unsupported")
    }
    fn prepare_owned_assembly_context(&mut self, _part: u32, _namespace: &Path) -> Result<()> {
        bail!("native_owned_assembly_context_unsupported")
    }
    fn add_assembly_entire_instance(
        &mut self,
        _parent: u32,
        _child: u32,
        _path: &Path,
        _name: &str,
        _origin: [f64; 3],
        _rotation: &[f64],
    ) -> Result<()> {
        bail!("native_entire_assembly_instance_unsupported")
    }
    fn add_assembly_entire_instance_nxopen(
        &mut self,
        _parent: u32,
        _child: u32,
        _path: &Path,
        _name: &str,
        _origin: [f64; 3],
        _rotation: &[f64],
    ) -> Result<()> {
        bail!("native_nxopen_entire_instance_unsupported")
    }
    fn full_assembly_info(
        &mut self,
        _root: u32,
        _namespace: &Path,
        _budget: &oe_contracts::NxAssemblyBudget,
        _role: &str,
    ) -> Result<oe_contracts::NxAssemblyObservation> {
        bail!("native_full_assembly_read_unsupported")
    }
    fn assembly_reference_sets(
        &mut self,
        _assembly: &oe_contracts::NxAssemblyObservation,
    ) -> Result<oe_contracts::NxAssemblyReferenceSets2> {
        bail!("native_assembly_reference_sets_unsupported")
    }
    fn save_all_owned_assembly(&mut self, _namespace: &Path) -> Result<Value> {
        bail!("native_full_assembly_save_unsupported")
    }
    fn create_drafting_sheet(
        &mut self,
        _part: u32,
        _plan: &oe_contracts::NativeDraftingPlan,
    ) -> Result<Value> {
        bail!("native_drafting_creation_unsupported")
    }
    fn create_parallel_reference(
        &mut self,
        _edges: [u32; 2],
        _origin: [f64; 3],
        _matrix: [f64; 9],
        _text_origin: [f64; 3],
    ) -> Result<Value> {
        bail!("native_reference_dimension_unsupported")
    }
    fn set_work_part(&mut self, _part: u32) -> Result<()> {
        bail!("native_work_part_selection_unsupported")
    }
    fn create_subtractive_cylinder(
        &mut self,
        _origin: [f64; 3],
        _height: f64,
        _diameter: f64,
    ) -> Result<u32> {
        bail!("native_hole_creation_unsupported")
    }
    fn add_hole_feature(
        &mut self,
        _part: u64,
        _body: u64,
        _origin: [f64; 3],
        _height: f64,
        _diameter: f64,
    ) -> Result<oe_contracts::NxFeatureAddOutcome1> {
        bail!("native_source_bound_hole_creation_unsupported")
    }
    fn feature_body(&mut self, _feature: u32) -> Result<u32> {
        bail!("native_feature_body_unsupported")
    }
    fn unite_bodies(&mut self, _target: u32, _tool: u32) -> Result<()> {
        bail!("native_explicit_unite_unsupported")
    }
    fn stepped_part_info(&mut self, _part: u32) -> Result<Value> {
        bail!("native_stepped_face_edges_unsupported")
    }
    fn create_cylinder_solid(
        &mut self,
        _origin: [f64; 3],
        _length: f64,
        _diameter: f64,
    ) -> Result<u32> {
        bail!("native_new_cylinder_creation_unsupported")
    }
    fn rotating_part_info(&mut self, part: u32) -> Result<Value> {
        self.detailed_info(part, false)
    }
    fn detailed_info(&mut self, _part: u32, _refresh_views: bool) -> Result<Value> {
        bail!("native_detailed_readback_unsupported")
    }
    /// Observe an already prepared drawing without entering display/drawing
    /// context or refreshing views. Unsupported implementations must not fall
    /// back to detailed_info, whose legacy contract includes context changes.
    fn observe_prepared_drawing(&mut self, _part: u32, _sheet: u32) -> Result<Value> {
        bail!("native_prepared_drawing_observation_unsupported")
    }
    fn ensure_empty_session(&mut self) -> Result<()>;
    fn create_mm_part(&mut self, path: &Path) -> Result<u32>;
    fn open_part(&mut self, path: &Path) -> Result<u32>;
    fn create_mm_expression(&mut self, part: u32, formula: &str) -> Result<u32>;
    fn create_block(&mut self, corner: [f64; 3], lengths: [String; 3]) -> Result<u32>;
    fn edit_expression_update(&mut self, part: u32, expression: u32, formula: &str) -> Result<i32>;
    fn prepare_controlled_representation(&mut self, part: u32) -> Result<Value>;
    fn save_work_part(&mut self) -> Result<()>;
    fn add_component(
        &mut self,
        parent: u32,
        prototype_part: u32,
        prototype: &Path,
        name: &str,
        origin: [f64; 3],
    ) -> Result<()>;
    fn part_info(&mut self, part: u32) -> Result<PartInfo>;
    fn export_ap214(&mut self, input: &Path, output: &Path) -> Result<()>;
    fn import_step_new_part(&mut self, input: &Path, output: &Path) -> Result<()>;
    fn import_parasolid(&mut self, input: &Path, part: u32) -> Result<Vec<u32>>;
}
pub trait Workspace {
    fn record_bound(
        &mut self,
        _phase: &str,
        _observation: Value,
    ) -> Result<oe_contracts::ArtifactReference> {
        bail!("bound_record_unsupported")
    }
    /// Legacy default for fixtures/adapters; actual workers use validated job output capacity.
    fn motion_output_bounds(&self) -> Result<(u64, u64, usize)> {
        Ok((
            oe_nx_protocol::MAX_PAYLOAD_BYTES,
            oe_nx_protocol::MAX_PAYLOAD_BYTES,
            oe_nx_protocol::MAX_FILES,
        ))
    }

    fn verify_motion_copy_files(&self) -> Result<()> {
        bail!("motion_copy_readonly_verification_unsupported")
    }
    /// Exact already-pinned job params entry, never a reserialized estimate.
    fn motion_parameters_reference(&self) -> Result<oe_contracts::ArtifactReference> {
        bail!("motion_parameters_identity_unavailable")
    }
    fn motion_job_id(&self) -> Result<String> {
        bail!("motion_job_identity_unavailable")
    }
    fn create_motion_evidence(
        &mut self,
        _budget: oe_contracts::NxMotionEvidenceBudget,
        _binding: oe_contracts::NxMotionEvidenceBinding,
    ) -> Result<Box<dyn crate::motion_evidence::sink::EvidenceSink + '_>> {
        bail!("motion_evidence_sink_unsupported")
    }

    fn prepare_assembly_build(
        &mut self,
        _intent: &oe_contracts::NxAssemblyBuildIntent,
    ) -> Result<(PathBuf, PathBuf)> {
        bail!("assembly_build_owned_copy_unsupported")
    }
    fn prepare_motion_source(
        &mut self,
        _closure: &oe_contracts::NxAssemblySourceClosure,
    ) -> Result<(PathBuf, PathBuf)> {
        bail!("motion_owned_copy_unsupported")
    }
    fn verify_motion_source(&self) -> Result<()> {
        bail!("motion_owned_copy_not_prepared")
    }
    fn input(&self, relative: &RelativePath) -> Result<PathBuf>;
    fn output(&self, name: &str) -> Result<PathBuf>;
    fn require_regular_output(&self, name: &str) -> Result<Vec<u8>>;
    fn optional_regular_output(&self, name: &str) -> Result<Option<Vec<u8>>>;
    fn record(&mut self, phase: &str, observation: Value) -> Result<()>;
}

pub fn await_step_export(
    workspace: &dyn Workspace,
    name: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<Vec<u8>> {
    const END: &[u8] = b"END-ISO-10303-21;";
    const BREP: &[u8] = b"MANIFOLD_SOLID_BREP";
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(bytes) = workspace.optional_regular_output(name)?
            && bytes.trim_ascii_end().ends_with(END)
        {
            ensure!(bytes.starts_with(b"ISO-10303-21;"), "step_header_missing");
            ensure!(
                bytes.windows(BREP.len()).any(|window| window == BREP),
                "step_solid_brep_marker_missing"
            );
            return Ok(bytes);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("step_export_incomplete_after_timeout");
        }
        std::thread::sleep(poll_interval.min(remaining));
    }
}
fn near(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-6_f64.max(1e-9 * a.abs().max(b.abs()))
}
/// Validate the controlled all-planar block against independently specified mm
/// dimensions. This is also usable for an independently reopened saved part.
pub fn require_block(info: &PartInfo, length: f64, width: f64, height: f64) -> Result<()> {
    ensure!(
        [length, width, height]
            .into_iter()
            .all(|v| v.is_finite() && v > 0.0 && v <= 1.0e6),
        "native_block_expected_dimensions_invalid"
    );
    ensure!(
        info.units == "mm"
            && info.solid_count == 1
            && info.sheet_count == 0
            && info.bbox.len() == 1
            && info.planar_vertex_bounds.len() == 1,
        "native_block_representation_differs"
    );
    // UF_MODL_ask_bounding_box favors speed over accuracy and does not
    // guarantee a minimum box (NX 2412 uf_modl_utilities.h:502-504).
    // For this controlled axis-aligned all-planar block, measure the
    // observed vertices; retain the UF box only as an enclosing check.
    let vertices =
        info.planar_vertex_bounds[0].context("native_block_planar_vertices_not_observed")?;
    let envelope = &info.bbox[0];
    for (i, expected) in [length, width, height].into_iter().enumerate() {
        for bounds in [&vertices, envelope] {
            ensure!(
                bounds[i].is_finite() && bounds[i + 3].is_finite() && bounds[i] <= bounds[i + 3],
                "native_block_bounds_invalid_axis_{i}"
            );
        }
        ensure!(
            near(vertices[i + 3] - vertices[i], expected),
            "native_block_extent_differs_axis_{i}"
        );
        ensure!(
            envelope[i] <= vertices[i] + 1e-6 && envelope[i + 3] >= vertices[i + 3] - 1e-6,
            "native_block_envelope_excludes_vertices_axis_{i}"
        );
    }
    let expression = info
        .expressions
        .iter()
        .find(|e| e.name == "cad_length")
        .context("cad_length_not_observed")?;
    ensure!(
        near(expression.value, length),
        "native_expression_value_differs"
    );
    Ok(())
}
pub fn require_controlled_refset(info: &PartInfo) -> Result<()> {
    let set = &info.controlled_reference_set;
    ensure!(
        info.solid_count == 1
            && info.solid_tags.len() == 1
            && info.solid_tags[0] != 0
            && info.part_tag != 0
            && info.representation_baseline == REPRESENTATION_BASELINE,
        "controlled_refset_requires_unique_current_body"
    );
    ensure!(
        set.found && set.reference_set_tag != 0 && set.name == CONTROLLED_REFSET,
        "controlled_refset_exact_members_differ"
    );
    if set.member_tags == info.solid_tags {
        ensure!(info.controlled_jt_mapping.is_none(), "unneeded_jt_mapping");
        return Ok(());
    }
    // The raw set is never filtered. This is an explicit new representation
    // baseline: exactly one solid, optionally its current associated JT only.
    let m = info
        .controlled_jt_mapping
        .as_ref()
        .context("controlled_refset_exact_members_differ")?;
    let jt = m.jt_tag.context("associated_jt_not_observed")?;
    let solid = info.solid_tags[0];
    let mut expected = vec![solid, jt];
    expected.sort_unstable();
    let mut actual = set.member_tags.clone();
    actual.sort_unstable();
    let mut after = m
        .member_tags_after
        .clone()
        .context("refset_after_mapping_not_observed")?;
    after.sort_unstable();
    ensure!(
        jt != 0 && jt != solid && actual == expected && after == expected,
        "controlled_refset_unaccounted_members"
    );
    ensure!(
        m.schema == JT_MAPPING_SCHEMA
            && m.wrapper_code == 0
            && m.complete
            && m.call_entered
            && m.call_returned
            && m.exception_code.is_none()
            && m.error.is_none()
            && m.owner_tag == info.part_tag
            && m.solid_tag == solid
            && m.solid_owner_tag == Some(info.part_tag)
            && m.reference_set_tag == set.reference_set_tag
            && m.reference_set_after == Some(set.reference_set_tag)
            && m.member_tags_before == set.member_tags
            && m.up_to_date == Some(true)
            && m.type_query_code == Some(0)
            && m.jt_type == Some(139)
            && m.jt_subtype == Some(3)
            && m.owner_query_code == Some(0)
            && m.jt_owner_tag == Some(info.part_tag),
        "controlled_refset_jt_mapping_invalid"
    );
    Ok(())
}
pub fn execute(
    api: &mut dyn NativeApi,
    workspace: &mut dyn Workspace,
    params: &Parameters,
) -> Result<Value> {
    params.validate()?;
    let stepped_source = match params {
        Parameters::CreateSteppedShaft { intent, .. }
        | Parameters::InspectSteppedShaft { intent, .. } => {
            Some(stepped_shaft::source(workspace, intent)?)
        }
        _ => None,
    };
    let cylinder_source = match params {
        Parameters::CreateCylinderSolid { intent, .. }
        | Parameters::InspectCylinderSolid { intent, .. } => {
            Some(rotating_part::source(workspace, intent)?)
        }
        _ => None,
    };
    let motion_source = match params {
        Parameters::RunAssemblyMotion { intent, .. } => {
            Some(motion_task::source(workspace, intent)?)
        }
        _ => None,
    };
    let position_source = match params {
        Parameters::InspectAssemblyPositions {
            source,
            expected,
            budget,
            ..
        } => Some(position_reopen::source(
            workspace, source, expected, budget,
        )?),
        _ => None,
    };
    let build_source = match params {
        Parameters::CreateSourceAssembly { intent, budget, .. } => {
            Some(source_assembly::source(workspace, intent, budget)?)
        }
        _ => None,
    };
    let copy_source = match params {
        Parameters::CopyAssemblyComponent { intent, budget, .. }
        | Parameters::ProbeComponentCopyMassProperties { intent, budget, .. } => {
            Some(component_copy::source(workspace, intent, budget)?)
        }
        _ => None,
    };
    let add_source = match params {
        Parameters::AddHoleFeature { intent, budget, .. } => {
            Some(feature_add::source(workspace, intent, budget)?)
        }
        _ => None,
    };
    let delete_source = match params {
        Parameters::DeleteHoleFeature { intent, budget, .. } => {
            Some(feature_delete::source(workspace, intent, budget)?)
        }
        _ => None,
    };
    api.ensure_empty_session()?;
    match params {
        Parameters::AddHoleFeature { intent, budget, .. } => {
            feature_add::execute(api, workspace, intent, add_source.as_ref().unwrap(), budget)
        }
        Parameters::DeleteHoleFeature { intent, budget, .. } => feature_delete::execute(
            api,
            workspace,
            intent,
            delete_source.as_ref().unwrap(),
            budget,
        ),
        Parameters::ProbeAssemblyMassProperties {
            root_part,
            budget,
            repetitions,
            ..
        } => mass_probe::execute_unchanged(api, workspace, root_part, budget, *repetitions),
        Parameters::ProbeComponentCopyMassProperties {
            intent,
            budget,
            repetitions,
            ..
        } => component_copy::execute_probed(
            api,
            workspace,
            intent,
            copy_source.as_ref().unwrap(),
            budget,
            *repetitions,
        ),
        Parameters::CopyAssemblyComponent { intent, budget, .. } => component_copy::execute(
            api,
            workspace,
            intent,
            copy_source.as_ref().unwrap(),
            budget,
        ),
        Parameters::CreateSourceAssembly { intent, budget, .. } => source_assembly::execute(
            api,
            workspace,
            intent,
            build_source.as_ref().unwrap(),
            budget,
        ),
        Parameters::InspectAssemblyPositions {
            source,
            expected,
            budget,
            ..
        } => {
            let (closure, observation) = position_source.as_ref().unwrap();
            position_reopen::execute(
                api,
                workspace,
                source,
                expected,
                closure,
                observation,
                budget,
            )
        }
        Parameters::RunAssemblyMotion {
            schema,
            intent,
            budget,
            evidence,
        } => {
            let (design, closure) = motion_source.as_ref().unwrap();
            if schema == motion_task::capture2::PARAMS_SCHEMA {
                let parameters = workspace.motion_parameters_reference()?;
                motion_task::capture2::execute(
                    api,
                    workspace,
                    &parameters,
                    intent,
                    design,
                    closure,
                    budget,
                    evidence,
                )
            } else {
                motion_task::execute(api, workspace, intent, design, closure, budget, evidence)
            }
        }
        Parameters::CreateSteppedShaft { intent, .. } => {
            let (design, raw) = stepped_source.as_ref().unwrap();
            stepped_shaft::create(api, workspace, intent, design, raw)
        }
        Parameters::InspectSteppedShaft { part, intent, .. } => {
            let (design, raw) = stepped_source.as_ref().unwrap();
            stepped_shaft::inspect(api, workspace, part, intent, design, raw)
        }
        Parameters::CreateCylinderSolid { intent, .. } => {
            let (design, raw) = cylinder_source.as_ref().unwrap();
            rotating_part::create(api, workspace, intent, design, raw)
        }
        Parameters::InspectCylinderSolid { part, intent, .. } => {
            let (design, raw) = cylinder_source.as_ref().unwrap();
            rotating_part::inspect(api, workspace, part, intent, design, raw)
        }
        Parameters::ImportStepAssembly { step, budget, .. } => {
            let source = workspace.input(&input_relative(step)?)?;
            let target = workspace.output("assembly_root.prt")?;
            let namespace = target.parent().context("assembly_output_parent_missing")?;
            api.import_assembly_preserved(&source, &target)?;
            workspace.require_regular_output("assembly_root.prt")?;
            let root = api.open_part(&target)?;
            let before =
                api.full_assembly_info(root, namespace, budget, "reference_observation")?;
            workspace.record("full_assembly_before_save", serde_json::to_value(&before)?)?;
            ensure!(
                !before.occurrences.is_empty(),
                "imported_reference_has_no_native_assembly_instances"
            );
            let saved = api.save_all_owned_assembly(namespace)?;
            workspace.record("full_assembly_save_returned", saved.clone())?;
            let after = api.full_assembly_info(root, namespace, budget, "reference_observation")?;
            workspace.record("full_assembly_after_save", serde_json::to_value(&after)?)?;
            crate::assembly::same_geometry(&before, &after)?;
            Ok(
                json!({"action":"import_step_assembly","before_save":before,"after_save":after,"save":saved,
                "independent_reopen":"required_separate_process","engineering_adoption":"not_assessed","source_role":"reference_observation"}),
            )
        }
        Parameters::CreateAssemblyFixture { intent, budget, .. } => {
            crate::assembly::create_fixture(api, workspace, intent, budget)
        }
        Parameters::CreateAssemblyFixtureNxopen {
            intent,
            budget,
            construction_profile,
            ..
        } => crate::assembly::create_fixture_nxopen(
            api,
            workspace,
            intent,
            budget,
            construction_profile,
        ),
        Parameters::InspectAssembly {
            root_part,
            budget,
            source_role,
            capture_reference_sets,
            ..
        } => {
            let source = workspace.input(&input_relative(root_part)?)?;
            let namespace = source.parent().context("assembly_input_parent_missing")?;
            let root = api.open_part(&source)?;
            let observed = api.full_assembly_info(root, namespace, budget, source_role)?;
            workspace.record(
                "full_assembly_fresh_process_read",
                serde_json::to_value(&observed)?,
            )?;
            let mut result = json!({"action":"inspect_assembly","assembly":observed,"save_requested":false,
                "engineering_adoption":"not_assessed","source_role":source_role});
            if *capture_reference_sets == Some(true) {
                let references = api.assembly_reference_sets(&observed)?;
                crate::assembly_reference_sets::validate(&references, &observed)?;
                let again = api.full_assembly_info(root, namespace, budget, source_role)?;
                ensure!(
                    again == observed,
                    "assembly_changed_during_reference_set_read"
                );
                let references_again = api.assembly_reference_sets(&again)?;
                ensure!(
                    references_again == references,
                    "assembly_reference_sets_changed_during_read"
                );
                crate::assembly_reference_sets::validate(&references_again, &again)?;
                workspace.record(
                    "full_assembly_reference_sets_read",
                    serde_json::to_value(&references)?,
                )?;
                result["reference_sets"] = serde_json::to_value(references)?;
            }
            Ok(result)
        }
        Parameters::CreateDraftingSheet {
            part: name, plan, ..
        } => {
            let source = workspace.input(&input_relative(name)?)?;
            let raw = std::fs::read(&source)?;
            ensure!(
                oe_nx_protocol::BlobIdentity::of(&raw).sha256.as_str() == plan.object.source_sha256,
                "drafting_cad_source_mismatch"
            );
            let target = workspace.output(name)?;
            std::fs::copy(&source, &target)?;
            ensure!(
                workspace.require_regular_output(name)? == raw,
                "drafting_copied_cad_mismatch"
            );
            let tag = api.open_part(&target)?;
            api.set_work_part(tag)?;
            let before = api.detailed_info(tag, false)?;
            workspace.record("drafting_before", before.clone())?;
            let created = api.create_drafting_sheet(tag, plan)?;
            workspace.record("drafting_creation", created.clone())?;
            if created["complete"] != true {
                // Keep the partial native state for diagnosis. It cannot
                // authorize save or replace the original creation failure.
                match api.detailed_info(tag, false) {
                    Ok(observed) => {
                        workspace.record("drafting_partial_after_creation", observed)?;
                    }
                    Err(error) => {
                        workspace.record(
                            "drafting_partial_observation_failed",
                            json!({"error":error.to_string()}),
                        )?;
                    }
                }
            }
            ensure!(
                created["complete"] == true,
                "drafting_native_creation_incomplete"
            );
            let after = api.detailed_info(tag, true)?;
            workspace.record("drafting_after", after.clone())?;
            let checked = drafting::check(plan, &after);
            workspace.record("drafting_plan_check", checked.clone())?;
            ensure!(
                checked["conclusion"] == "pass",
                "drafting_native_plan_unproven"
            );
            api.save_work_part()?;
            workspace.require_regular_output(name)?;
            let saved = api.detailed_info(tag, false)?;
            workspace.record("drafting_saved", saved.clone())?;
            let saved_check = drafting::check(plan, &saved);
            ensure!(
                saved_check["conclusion"] == "pass",
                "drafting_saved_plan_unproven"
            );
            ensure!(std::fs::read(&source)? == raw, "drafting_input_changed");
            Ok(
                json!({"action":"create_drafting_sheet","plan":plan,"creation":created,
                "parts":[{"input":name,"before":before,"after":after,"after_save":saved}],"plan_check":saved_check,
                "independent_reopen":"required_separate_process","engineering_adoption":"not_established",
                "manufacturing_drawing":"not_established","scope":"prototype geometry sheet and single named model view; tolerances, datum chain, material, dimensions, visual review and release separate"}),
            )
        }
        Parameters::ExportDisplayDrawingPdf { part, intent, .. } => {
            drawing_display::execute(api, workspace, part, intent)
        }
        Parameters::ExportIdentifiedDrawingPdf { part, intent, .. } => {
            drawing_identification::execute(api, workspace, part, intent)
        }
        Parameters::ExportDrawingPdf { part, intent, .. } => {
            drawing_pdf::execute(api, workspace, part, intent)
        }
        Parameters::RetireDrawingDimensions { part, intent, .. } => {
            drawing_retirement::execute(api, workspace, part, intent)
        }
        Parameters::CreateSheetDiameterDimension { part, intent, .. } => {
            sheet_diameter::execute(api, workspace, part, intent)
        }
        Parameters::CreateSheetReferenceDimension {
            part: name,
            sheet,
            reference,
            text_origin_drawing,
            ..
        } => {
            let source = workspace.input(&input_relative(name)?)?;
            let raw = std::fs::read(&source)?;
            ensure!(
                oe_nx_protocol::BlobIdentity::of(&raw).sha256.as_str()
                    == sheet.object.source_sha256,
                "sheet_dimension_source_mismatch"
            );
            let target = workspace.output(name)?;
            std::fs::copy(&source, &target)?;
            ensure!(
                workspace.require_regular_output(name)? == raw,
                "sheet_dimension_copy_mismatch"
            );
            let part = api.open_part(&target)?;
            api.set_work_part(part)?;
            let before = api.detailed_info(part, false)?;
            workspace.record("sheet_dimension_before", before.clone())?;
            let prepared =
                sheet_dimensions::prepare(sheet, reference, *text_origin_drawing, &raw, &before)?;
            workspace.record("sheet_dimension_prepared", prepared.clone())?;
            let tag = |v: &Value| -> Result<u32> {
                Ok(u32::try_from(
                    v.as_u64().context("sheet_dimension_tag_missing")?,
                )?)
            };
            let created = api.create_sheet_parallel_reference(
                tag(&prepared["sheet_tag"])?,
                tag(&prepared["view_tag"])?,
                [
                    tag(&prepared["edge_tags"][0])?,
                    tag(&prepared["edge_tags"][1])?,
                ],
                *text_origin_drawing,
            )?;
            workspace.record("sheet_dimension_creation", created.clone())?;
            ensure!(
                created["complete"] == true,
                "sheet_dimension_native_creation_incomplete"
            );
            let dimension = tag(&created["dimension_tag"])?;
            let after = api.detailed_info(part, false)?;
            workspace.record("sheet_dimension_after", after.clone())?;
            sheet_dimensions::check_addition(&before, &after, dimension)?;
            let check = sheet_dimensions::check_created(
                sheet,
                reference,
                *text_origin_drawing,
                &raw,
                &after,
                dimension,
            )?;
            workspace.record("sheet_dimension_check", check)?;
            api.save_work_part()?;
            workspace.require_regular_output(name)?;
            let saved = api.detailed_info(part, false)?;
            workspace.record("sheet_dimension_saved", saved.clone())?;
            sheet_dimensions::check_addition(&before, &saved, dimension)?;
            let saved_check = sheet_dimensions::check_created(
                sheet,
                reference,
                *text_origin_drawing,
                &raw,
                &saved,
                dimension,
            )?;
            ensure!(
                std::fs::read(&source)? == raw,
                "sheet_dimension_input_changed"
            );
            Ok(
                json!({"action":"create_sheet_reference_dimension","sheet":sheet,"reference":model_edges::intent_value(reference)?,
                "creation":created,"parts":[{"input":name,"before":before,"after":after,"after_save":saved}],"check":saved_check,
                "independent_reopen":"required_separate_process","visual_readability":"not_assessed",
                "manufacturing_drawing":"not_established","engineering_adoption":"not_established"}),
            )
        }
        Parameters::CreateReferenceDimension {
            part: name,
            intent,
            plane_origin,
            plane_x_axis,
            plane_y_axis,
            text_origin_wcs,
            ..
        } => {
            let source = workspace.input(&input_relative(name)?)?;
            let raw = std::fs::read(&source)?;
            ensure!(
                oe_nx_protocol::BlobIdentity::of(&raw).sha256.as_str()
                    == intent.object.source_sha256,
                "reference_dimension_cad_source_mismatch"
            );
            let target = workspace.output(name)?;
            std::fs::copy(&source, &target)?;
            ensure!(
                workspace.require_regular_output(name)? == raw,
                "reference_dimension_copied_cad_mismatch"
            );
            let tag = api.open_part(&target)?;
            api.set_work_part(tag)?;
            let before = api.detailed_info(tag, false)?;
            workspace.record("reference_dimension_before", before.clone())?;
            let mut selected = Vec::new();
            let mut tags = Vec::new();
            for p in &intent.points {
                let choice = model_edges::CircleIntentRef::for_point(&intent.schema, p)?
                    .unwrap()
                    .select(&before)?;
                selected.push(choice.clone());
                if choice["conclusion"] == "pass" {
                    tags.push(u32::try_from(
                        choice["candidate_edge_tags"][0]
                            .as_u64()
                            .context("reference_dimension_parent_tag_missing")?,
                    )?);
                }
            }
            workspace.record(
                "reference_dimension_parent_selection",
                json!({"selected":selected}),
            )?;
            ensure!(
                tags.len() == 2 && tags[0] != tags[1],
                "reference_dimension_unique_parents_unproven"
            );
            let normal = model_edges::cross(*plane_x_axis, *plane_y_axis);
            let matrix = [
                plane_x_axis[0],
                plane_x_axis[1],
                plane_x_axis[2],
                plane_y_axis[0],
                plane_y_axis[1],
                plane_y_axis[2],
                normal[0],
                normal[1],
                normal[2],
            ];
            let created = api.create_parallel_reference(
                [tags[0], tags[1]],
                *plane_origin,
                matrix,
                *text_origin_wcs,
            )?;
            workspace.record("reference_dimension_creation", created.clone())?;
            ensure!(
                created["complete"] == true,
                "reference_dimension_native_creation_incomplete"
            );
            let after = api.detailed_info(tag, false)?;
            workspace.record("reference_dimension_after", after.clone())?;
            api.save_work_part()?;
            workspace.require_regular_output(name)?;
            let saved = api.detailed_info(tag, false)?;
            workspace.record("reference_dimension_saved", saved.clone())?;
            ensure!(
                std::fs::read(&source)? == raw,
                "reference_dimension_input_changed"
            );
            Ok(
                json!({"action":"create_reference_dimension","intent":model_edges::intent_value(intent)?,"selection":selected,"creation":created,
                "parts":[{"input":name,"before":before,"after":after,"after_save":saved}],
                "independent_reopen":"required_separate_process","drawing_sheet_generation":"not_performed",
                "display_and_manufacturing_tolerance":"not_assessed","engineering_verdict":"not_assessed"}),
            )
        }
        Parameters::CreateHolePlate {
            length_mm,
            width_mm,
            height_mm,
            hole_centers_x_mm,
            hole_y_mm,
            hole_diameter_mm,
            design_input_sha256,
            ..
        } => {
            let target = workspace.output("hole_plate.prt")?;
            let part = api.create_mm_part(&target)?;
            api.set_work_part(part)?;
            api.create_block(
                [-length_mm / 2.0, -width_mm / 2.0, 0.0],
                [
                    length_mm.to_string(),
                    width_mm.to_string(),
                    height_mm.to_string(),
                ],
            )?;
            let mut features = Vec::new();
            for x in hole_centers_x_mm {
                features.push(api.create_subtractive_cylinder(
                    [*x, *hole_y_mm, -1.0],
                    height_mm + 2.0,
                    *hole_diameter_mm,
                )?);
            }
            let before = api.detailed_info(part, false)?;
            workspace.record("hole_plate_before_save", before.clone())?;
            api.save_work_part()?;
            workspace.require_regular_output("hole_plate.prt")?;
            let after = api.detailed_info(part, false)?;
            workspace.record("hole_plate_after_save", after.clone())?;
            Ok(
                json!({"action":"create_hole_plate","design_input_sha256":design_input_sha256,"created_hole_features":features,"before_save":before,"after_save":after,
                "native_operation":"fresh bounded plate regeneration; not in-place repair of a customer feature tree",
                "engineering_verdict":"not_assessed"}),
            )
        }
        Parameters::InspectDetailed { parts, .. } => {
            let mut reports = Vec::new();
            for name in parts {
                let tag = api.open_part(&workspace.input(&input_relative(name)?)?)?;
                api.set_work_part(tag)?;
                let report = api.detailed_info(tag, false)?;
                workspace.record("native_detailed_read", report.clone())?;
                reports.push(report);
            }
            Ok(
                json!({"action":"fresh_process_detailed_readback","parts":reports,"save_requested":false,"engineering_verdict":"not_assessed"}),
            )
        }
        Parameters::MaintainDrawings {
            parts,
            refresh_views,
            ..
        } => {
            let mut reports = Vec::new();
            for name in parts {
                let source = workspace.input(&input_relative(name)?)?;
                let target = workspace.output(name)?;
                std::fs::copy(source, &target)?;
                workspace.require_regular_output(name)?;
                let tag = api.open_part(&target)?;
                api.set_work_part(tag)?;
                let before = api.detailed_info(tag, false)?;
                workspace.record("drawing_saved_copy_before", before.clone())?;
                let updated = api.detailed_info(tag, *refresh_views)?;
                workspace.record("drawing_saved_copy_updated", updated.clone())?;
                api.save_work_part()?;
                workspace.require_regular_output(name)?;
                let after = api.detailed_info(tag, false)?;
                workspace.record("drawing_saved_copy_after", after.clone())?;
                reports.push(
                    json!({"input":name,"before":before,"updated":updated,"after_save":after}),
                );
            }
            Ok(
                json!({"action":"maintain_job_owned_drawing_copies","parts":reports,"independent_reopen":"required_separate_process",
                "dimension_reassociation":"not_performed","layout_mutation":"not_performed","visual_readability":"not_assessed","engineering_verdict":"not_assessed"}),
            )
        }
        Parameters::Inspect { parts, .. } => {
            let mut reports = Vec::new();
            for name in parts {
                let path = workspace.input(&input_relative(name)?)?;
                let tag = api.open_part(&path)?;
                let report = api.part_info(tag)?;
                workspace.record("native_part_read", serde_json::to_value(&report)?)?;
                reports.push(report);
            }
            Ok(
                json!({"action":"fresh_process_readback","parts":reports,"save_requested":false,"engineering_verdict":"not_assessed"}),
            )
        }
        Parameters::ImportStep { step, .. } => {
            let source = workspace.input(&input_relative(step)?)?;
            let target = workspace.output("step_roundtrip.prt")?;
            api.import_step_new_part(&source, &target)?;
            workspace.require_regular_output("step_roundtrip.prt")?;
            let part = api.open_part(&target)?;
            let report = api.part_info(part)?;
            workspace.record("step_import_native_read", serde_json::to_value(&report)?)?;
            ensure!(
                report.solid_count == 1 && report.sheet_count == 0 && report.units == "mm",
                "step_import_representation_or_units_differ"
            );
            api.save_work_part()?;
            Ok(
                json!({"action":"import_step","part":report,"source_identity":"job-bound STEP bytes; product/version resolved by Project sources","supplier_identity":"not_assessed","representation_scope":"millimetre single solid; other representations remain unsupported","engineering_verdict":"not_assessed"}),
            )
        }
        Parameters::ImportStepBodies {
            step,
            expected_units,
            maximum_bodies,
            ..
        } => {
            let source = workspace.input(&input_relative(step)?)?;
            let target = workspace.output("step_bodies.prt")?;
            api.import_step_new_part(&source, &target)?;
            workspace.require_regular_output("step_bodies.prt")?;
            let part = api.open_part(&target)?;
            let report = api.part_info(part)?;
            workspace.record("step_bodies_native_read", serde_json::to_value(&report)?)?;
            require_body_observation(&report, expected_units, *maximum_bodies)?;
            api.save_work_part()?;
            workspace.require_regular_output("step_bodies.prt")?;
            Ok(
                json!({"action":"import_step_bodies","part":report,"coverage":"single-part solid bodies and exact native bounds; assembly prototypes and functional features not verified",
                "source_identity":"job-bound STEP; Project source/version review required","engineering_verdict":"not_assessed"}),
            )
        }
        Parameters::ImportParasolid { parasolid, .. } => {
            let source = workspace.input(&input_relative(parasolid)?)?;
            let target = workspace.output("parasolid_import.prt")?;
            let part = api.create_mm_part(&target)?;
            workspace.record("parasolid_import_intent", json!({"source":source,"part_tag":part,"target":target,"target_units_requested":"mm","source_declared_units":"not_independently_known"}))?;
            let imported_tags = api.import_parasolid(&source, part)?;
            workspace.record(
                "parasolid_import_returned",
                json!({"body_tags":imported_tags}),
            )?;
            let report = api.part_info(part)?;
            workspace.record(
                "parasolid_import_native_read",
                serde_json::to_value(&report)?,
            )?;
            ensure!(
                report.units == "mm"
                    && report.solid_count == 1
                    && report.sheet_count == 0
                    && report.solid_tags == imported_tags
                    && imported_tags.len() == 1
                    && imported_tags[0] != 0
                    && report.components.is_empty(),
                "parasolid_single_body_import_or_units_differ"
            );
            ensure!(
                report.exact_work_csys_bounds.len() == 1,
                "parasolid_exact_bounds_missing"
            );
            let bounds = &report.exact_work_csys_bounds[0];
            ensure!(
                ExactBounds::new(bounds.min_corner, bounds.directions, bounds.distances)?
                    .absolute_axis_bounds
                    == bounds.absolute_axis_bounds,
                "parasolid_exact_bounds_invalid"
            );
            api.save_work_part()?;
            workspace.require_regular_output("parasolid_import.prt")?;
            Ok(
                json!({"action":"import_parasolid","part":report,"representation_scope":"received_supplier_file_single_body_import","source_declared_units":"not_independently_known","target_units":"mm","engineering_verdict":"not_assessed"}),
            )
        }
        Parameters::Create {
            initial_length_mm,
            updated_length_mm,
            width_mm,
            height_mm,
            spacing_mm,
            ..
        } => {
            let part_path = workspace.output("parametric_block.prt")?;
            let part = api.create_mm_part(&part_path)?;
            let expression =
                api.create_mm_expression(part, &format!("cad_length={initial_length_mm}"))?;
            api.create_block(
                [0.0; 3],
                [
                    "cad_length".into(),
                    width_mm.to_string(),
                    height_mm.to_string(),
                ],
            )?;
            let before = api.part_info(part)?;
            workspace.record("before_expression_edit", serde_json::to_value(&before)?)?;
            require_block(&before, *initial_length_mm, *width_mm, *height_mm)?;
            let update_errors =
                api.edit_expression_update(part, expression, &updated_length_mm.to_string())?;
            workspace.record("native_update", json!({"error_count":update_errors}))?;
            ensure!(update_errors == 0, "native_update_reports_errors");
            let after = api.part_info(part)?;
            workspace.record("after_expression_edit", serde_json::to_value(&after)?)?;
            require_block(&after, *updated_length_mm, *width_mm, *height_mm)?;
            let refset_prepared = api.prepare_controlled_representation(part)?;
            workspace.record("controlled_refset_prepared", refset_prepared)?;
            let with_refset = api.part_info(part)?;
            require_block(&with_refset, *updated_length_mm, *width_mm, *height_mm)?;
            require_controlled_refset(&with_refset)?;
            ensure!(
                with_refset.solid_tags == after.solid_tags,
                "controlled_refset_changed_body_identity"
            );
            workspace.record(
                "controlled_refset_before_save",
                serde_json::to_value(&with_refset)?,
            )?;
            api.save_work_part()?;
            workspace.require_regular_output("parametric_block.prt")?;
            let saved_refset = api.part_info(part)?;
            // Record the complete observation even when an unchanged gate below
            // rejects it. This phase is evidence, not the accepted-after-save phase.
            workspace.record(
                "controlled_refset_after_save_observed",
                serde_json::to_value(&saved_refset)?,
            )?;
            require_block(&saved_refset, *updated_length_mm, *width_mm, *height_mm)?;
            require_controlled_refset(&saved_refset)?;
            ensure!(
                saved_refset.controlled_reference_set.reference_set_tag
                    == with_refset.controlled_reference_set.reference_set_tag
                    && saved_refset.solid_tags == with_refset.solid_tags,
                "reference_set_changed_during_save"
            );
            workspace.record(
                "controlled_refset_after_save",
                serde_json::to_value(&saved_refset)?,
            )?;
            let assembly_path = workspace.output("two_instances.prt")?;
            let assembly = api.create_mm_part(&assembly_path)?;
            api.add_component(assembly, part, &part_path, "BLOCK_A", [0.0; 3])?;
            api.add_component(
                assembly,
                part,
                &part_path,
                "BLOCK_B",
                [*spacing_mm, 0.0, 0.0],
            )?;
            api.save_work_part()?;
            let assembly_info = api.part_info(assembly)?;
            workspace.record(
                "assembly_native_read",
                serde_json::to_value(&assembly_info)?,
            )?;
            ensure!(
                assembly_info.units == "mm" && assembly_info.components.len() == 2,
                "assembly_occurrences_or_units_differ"
            );
            for (name, x) in [("BLOCK_A", 0.0), ("BLOCK_B", *spacing_mm)] {
                let c = assembly_info
                    .components
                    .iter()
                    .find(|c| c.name == name)
                    .context("assembly_occurrence_name_missing")?;
                ensure!(
                    c.reference_set == CONTROLLED_REFSET,
                    "assembly_reference_set_differs"
                );
                for (i, target) in [x, 0.0, 0.0].into_iter().enumerate() {
                    ensure!(near(c.origin[i], target), "assembly_origin_differs");
                }
                for (i, v) in c.matrix.iter().enumerate() {
                    ensure!(
                        near(*v, if i % 4 == 0 { 1.0 } else { 0.0 }),
                        "assembly_rotation_differs"
                    );
                }
                ensure!(
                    c.prototype_path
                        .replace('/', "\\")
                        .eq_ignore_ascii_case(&part_path.to_string_lossy().replace('/', "\\")),
                    "assembly_prototype_differs"
                );
            }
            workspace.require_regular_output("two_instances.prt")?;
            let step = workspace.output("parametric_block.step")?;
            api.export_ap214(&part_path, &step)?;
            let bytes = await_step_export(
                workspace,
                "parametric_block.step",
                Duration::from_secs(120),
                Duration::from_millis(250),
            )?;
            workspace.record("step_export_completed", json!({"bytes":bytes.len()}))?;
            Ok(
                json!({"action":"create","before":before,"after":after,"saved_controlled_representation":saved_refset,"assembly":assembly_info,"update_error_count":update_errors,"step_header_and_brep_marker_present":true,"step_roundtrip":"requires_separate_native_process","joint_constraints":"not_created_or_verified","contact_and_accuracy":"not_assessed","engineering_verdict":"not_assessed"}),
            )
        }
    }
}

pub fn require_body_observation(report: &PartInfo, units: &str, maximum_bodies: u32) -> Result<()> {
    ensure!(
        units == "mm" && (1..=128).contains(&maximum_bodies),
        "step_bodies_observation_scope_invalid"
    );
    ensure!(
        report.units == units
            && report.solid_count > 0
            && report.solid_count <= maximum_bodies as usize
            && report.sheet_count == 0
            && report.components.is_empty(),
        "step_bodies_representation_outside_scope"
    );
    ensure!(
        report.solid_tags.len() == report.solid_count as usize
            && report.exact_work_csys_bounds.len() == report.solid_count as usize,
        "step_bodies_native_bounds_incomplete"
    );
    for b in &report.exact_work_csys_bounds {
        ensure!(
            ExactBounds::new(b.min_corner, b.directions, b.distances)?.absolute_axis_bounds
                == b.absolute_axis_bounds,
            "step_bodies_exact_bounds_invalid"
        );
    }
    Ok(())
}
