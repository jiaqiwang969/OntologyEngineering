//! v2 entry points share existing core identity/path/process primitives.
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use oe_core::{SkillRoot, read_regular_bytes, write_new, write_new_bytes};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[path = "execution_directory.rs"]
mod execution_directory;
#[path = "source_session.rs"]
mod source_session;

#[derive(Subcommand)]
pub enum DesignCommand {
    SemanticaScope {
        #[arg(long)]
        output: Option<PathBuf>,
        /// Refuse to proceed until complete migration has actual evidence.
        #[arg(long)]
        require_complete: bool,
    },
    Validate {
        #[arg(long, default_value = "specs/engineering.toml")]
        spec: PathBuf,
    },
    Normalize {
        #[arg(long, default_value = "specs/engineering.toml")]
        spec: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Inspect {
        #[arg(long, default_value = "specs/engineering.toml")]
        spec: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Generate {
        #[arg(long, default_value = "specs/engineering.toml")]
        spec: PathBuf,
        #[arg(long)]
        ggen: Option<PathBuf>,
    },
    Check {
        #[arg(long, default_value = "specs/engineering.toml")]
        spec: PathBuf,
        #[arg(long)]
        ggen: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Impact {
        #[arg(long)]
        before: PathBuf,
        #[arg(long)]
        after: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Contract {
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = "Quantity")]
        kind: String,
    },
}
#[derive(Subcommand)]
pub enum KernelCommand {
    Capabilities,
    /// Original snapshot integrity; source checksum is distinct from engineering acceptance.
    SnapshotChecksum {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Original ordered version comparison with explicit seed and read-only storage state.
    TemporalVersionDiff {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Source version record creation, type validation and migration; no persistence.
    TemporalVersionRecords {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Stateful source temporal sequences/cycles with complete action failures.
    TemporalPatterns {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Original time-constrained paths with input-position witnesses.
    TemporalPaths {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Original temporal consistency diagnostics with input-position witnesses.
    TemporalConsistency {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Explicit-clock range union/intersection and bounded calendar evolution.
    TemporalRange {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Explicit-clock fact normalization and point-in-time graph reconstruction.
    TemporalSnapshot {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Source timeline and revision interval classification with original records.
    TemporalFacts {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Deterministic source interval relations, validity, coverage and granularity.
    Temporal {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Bounded Datalog actions with before/after state and explicit failures.
    DatalogSession {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Import a dictionary ContextGraph snapshot into bounded Datalog inference.
    DatalogGraph {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Bounded stateless multi-variable Datalog closure and optional query.
    Datalog {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// One explicitly admitted local source session; no Project/adoption mutation.
    SourceSession {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Internal same-program worker; the host owns the absolute process deadline.
    #[command(hide = true)]
    SourceSessionWorker,
    /// Stateless textual rules with explicit source IDs, confidence and priority.
    Reason {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    QueryOptions {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Query {
        #[arg(long)]
        data: PathBuf,
        #[arg(long)]
        query: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Validate one explicit named graph without merging neighboring graphs.
    ValidateNamed {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Validate {
        #[arg(long)]
        data: PathBuf,
        #[arg(long)]
        shapes: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Rules {
        #[arg(long)]
        facts: PathBuf,
        #[arg(long)]
        rules: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Regress {
        #[arg(long)]
        bundle: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Review {
        #[arg(long)]
        binding: PathBuf,
        #[arg(long)]
        evidence: PathBuf,
        #[arg(long)]
        bundle: String,
        #[arg(long)]
        query: Vec<String>,
        #[arg(long)]
        shape: Vec<String>,
        #[arg(long)]
        prior: Option<PathBuf>,
        /// Check this current Project against the context frozen in the binding.
        #[arg(long)]
        project: Option<PathBuf>,
        /// Deliberately review only the frozen RDF; grants no current-project use.
        #[arg(long, conflicts_with = "project")]
        historical_snapshot: bool,
        #[arg(long)]
        output: PathBuf,
    },
    Transition {
        #[arg(long)]
        ledger: PathBuf,
        #[arg(long)]
        event: PathBuf,
    },
}
#[derive(Subcommand)]
pub enum ProjectCommand {
    /// Preserve SmokingGun/Flameox evidence without running a workload.
    Performance {
        #[command(subcommand)]
        command: crate::performance::PerformanceCommand,
    },
    /// Source-bound electronics evidence projection for the current Project.
    Electronics {
        #[command(subcommand)]
        command: crate::electronics::ElectronicsCommand,
    },
    /// Source/artifact evidence projection for REA, LeanToken and GitContribute.
    SourceProjection {
        #[command(subcommand)]
        command: crate::source_projection::SourceProjectionCommand,
    },
    /// Typed mathematics evidence projection for Jacobian.
    Jacobian {
        #[command(subcommand)]
        command: crate::jacobian::JacobianCommand,
    },
    /// Persistent project commits, source-bound recovery and root-goal learning binding.
    Workspace {
        #[command(subcommand)]
        command: crate::workspace::WorkspaceCommand,
    },
    /// Replay current source-bound reviews; preserve original impact and independent adoption.
    ReconcileReviewed {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Update one decision from registered, re-verified tool results (new Project copy; adoption unchanged).
    Decide {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value = "var/capabilities/registry.json")]
        registry: PathBuf,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        receipt: Option<PathBuf>,
    },
    /// Engineering-memory recall candidates re-verified against the same Project; never decision support.
    MemoryCandidate {
        #[command(subcommand)]
        command: crate::tool_result::MemoryCandidateCommand,
    },
    /// Usage feedback for a registered memory candidate, in memory's session-feedback/1 bound to this Project.
    MemoryFeedback {
        #[arg(long)]
        project: PathBuf,
        /// The registered memory handoff file.
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long, value_parser = ["helped", "harmed", "no_delta", "rejected", "not_used"])]
        outcome: String,
        #[arg(long)]
        result: String,
        /// JSON list of memory SourceAnchor objects (required for helped/harmed).
        #[arg(long)]
        sources: Option<PathBuf>,
        #[arg(long)]
        elapsed_ms: Option<u64>,
        #[arg(long = "unknown")]
        unknowns: Vec<String>,
        /// Only for a bare v2-candidate/1: the memory context that exposed it.
        #[arg(long)]
        context_id: Option<String>,
        #[arg(long)]
        context_sha256: Option<String>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Project records as memory organization artifacts {id, path, sha256}.
    MemoryExport {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Specialist tool results handed back with provenance into the same Project.
    ToolResult {
        #[command(subcommand)]
        command: crate::tool_result::ToolResultCommand,
    },
    /// General source-bound engineering inquiry through Project/work/Harness.
    Explore {
        #[command(subcommand)]
        command: crate::exploration::ExplorationCommand,
    },
    /// Compare controlled designs against the same frozen parent task.
    DesignComparison {
        #[command(subcommand)]
        command: crate::design_comparison::DesignComparisonCommand,
    },
    /// Freeze and record a source-bound design attempt in the existing Project.
    DesignAttempt {
        #[command(subcommand)]
        command: crate::design_attempt::DesignAttemptCommand,
    },
    /// Source-bound inCAD specifications for the same current Project work.
    Incad {
        #[command(subcommand)]
        command: crate::incad::IncadCommand,
    },
    /// Deterministic read-only tasks from the same current Project.
    TaskView {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Re-read all bound inputs before reusing a prior task view.
    VerifyTaskView {
        #[arg(long)]
        input: PathBuf,
    },
    /// Complete-source long-context model advice, separate from finite Decisions judgments and adoption.
    ModelAssistance {
        #[command(subcommand)]
        command: Box<crate::model_assistance::ModelAssistanceCommand>,
    },
    /// Fresh scope-bound review and independent local approval; writes a new project copy.
    Adopt {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Revalidate an adoption against current source bytes, policy and executor.
    VerifyAdoption {
        #[arg(long)]
        input: PathBuf,
    },
    /// Explicit /2 approval of a current promoted package and typed Project.
    AdoptPromoted {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Recheck /2 adoption against the caller's current Project and registry.
    VerifyPromotedAdoption {
        #[arg(long)]
        input: PathBuf,
    },
    /// Agent-maintained content snapshot for current reviews/work, not adoption.
    Context {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        object: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Source-bound engineering gaps, explicit models and CAD/process handoff.
    Work {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        #[command(flatten)]
        tools: crate::KnowledgeTools,
    },
    Validate {
        #[arg(long)]
        input: PathBuf,
    },
    /// Root-goal route anchoring: stage relations to the root object, progress streaks and reviews.
    RouteAudit {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        anchor: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Impact {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        changed: Vec<String>,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Reconcile {
        #[arg(long)]
        before: PathBuf,
        #[arg(long)]
        after: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}
#[derive(Subcommand)]
pub enum MaintainCommand {
    /// Explicit Rust native profile via a frozen independent driver.
    RuntimeNative {
        #[arg(long)]
        input: PathBuf,
    },
    Status,
    Inventory {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Manifest {
        #[arg(long)]
        output: PathBuf,
    },
    Install {
        #[arg(long)]
        inventory: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
}
#[derive(Subcommand)]
pub enum ExperienceCommand {
    Freeze {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Assess {
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Decide {
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long,value_parser=["accept-trial","reject"])]
        action: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        output: PathBuf,
    },
    Apply {
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Recover {
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        output: PathBuf,
    },
    List {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}
pub fn experience(root: &SkillRoot, command: ExperienceCommand) -> Result<Value> {
    use oe_kernel::learning;
    if !matches!(&command, ExperienceCommand::List { .. }) {
        require_execution(root, "experience_evaluation_or_trial")?;
    }
    let (operation, result) = match command {
        ExperienceCommand::Freeze { input, output } => {
            let input = json_input(root, &input)?;
            ensure!(
                input["schema"] == "oev2.experience-freeze/2",
                "experience_new_freeze_requires_protocol_2"
            );
            ("freeze", learning::freeze(root, &input, &output)?)
        }
        ExperienceCommand::Assess { candidate, output } => {
            ("assess", learning::assess(root, &candidate, &output)?)
        }
        ExperienceCommand::Decide {
            candidate,
            action,
            reason,
            output,
        } => {
            let r = learning::decide(root, &candidate, &action, &reason)?;
            write_new(&root.private_path(&output)?, &r)?;
            ("decide", r)
        }
        ExperienceCommand::Apply {
            candidate,
            input,
            output,
        } => (
            "apply",
            learning::apply(root, &candidate, &json_input(root, &input)?, &output)?,
        ),
        ExperienceCommand::Recover {
            candidate,
            reason,
            output,
        } => (
            "recover",
            learning::recover(root, &candidate, &reason, &output)?,
        ),
        ExperienceCommand::List { query, output } => {
            let r = learning::list(root, &query)?;
            if let Some(out) = output {
                write_new(&root.private_path(&out)?, &r)?;
            }
            ("list", r)
        }
    };
    let status = if result["status"] == "failed" {
        "failed"
    } else {
        "recorded"
    };
    Ok(
        json!({"schema":"ontology-engineering.knowledge-cli-response/v1","command":format!("experience-{operation}"),"status":status,"result":result,
        "semantic_review":"scoped Rust receipts where present; no physical acceptance implied","ontology_promotion":"not_run","errors":[]}),
    )
}
pub fn require_execution(root: &SkillRoot, role: &str) -> Result<Value> {
    let source = oe_design::provenance::require(root)?;
    let cli =
        oe_core::json::strict_json(include_bytes!(concat!(env!("OUT_DIR"), "/cli-build.json")))?;
    ensure!(
        cli["generated_contracts_sha256"]
            == oe_core::fingerprint_file(
                &root.path().join("generated/contracts.rs"),
                16 * 1024 * 1024
            )?
            .sha256,
        "compiled_generated_contract_binding_mismatch"
    );
    let semantic_contracts_sha256 = oe_kernel::query::compiled_semantic_contract_sha256();
    ensure!(
        semantic_contracts_sha256
            == oe_core::fingerprint_file(
                &root.path().join("generated/semantic-contracts.rs"),
                16 * 1024 * 1024,
            )?
            .sha256,
        "compiled_semantic_contract_binding_mismatch"
    );
    let binding = json!({"execution_role":role,"normalizer":source,"cli":cli,"semantic_contracts_sha256":semantic_contracts_sha256,"executable_sha256":oe_kernel::executable_digest()?,"proof":"execution input identity at command entry; command outcome requires its own receipt"});
    let path = execution_directory::create(root, role)?;
    write_new(&path.join("binding.json"), &binding)?;
    Ok(binding)
}
fn bytes(root: &SkillRoot, p: &std::path::Path) -> Result<Vec<u8>> {
    read_regular_bytes(&root.data_expand(p)?, oe_kernel::MAX_BYTES)
}
fn software_bytes(root: &SkillRoot, p: &std::path::Path) -> Result<Vec<u8>> {
    read_regular_bytes(&root.expand(p)?, oe_kernel::MAX_BYTES)
}
fn json_input(root: &SkillRoot, p: &std::path::Path) -> Result<Value> {
    oe_wire_json(&bytes(root, p)?)
}
fn oe_wire_json(bytes: &[u8]) -> Result<Value> {
    Ok(oe_core::json::strict_json(bytes)?)
}
fn emit(root: &SkillRoot, value: Value, output: Option<PathBuf>, failed: bool) -> Result<u8> {
    if let Some(p) = output {
        write_new(&root.private_path(&p)?, &value)?;
    }
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(if failed { 1 } else { 0 })
}
fn ggen_path(root: &SkillRoot, p: Option<PathBuf>) -> Result<PathBuf> {
    root.expand(&p.unwrap_or_else(|| PathBuf::from("var/builds/ggen-target/debug/ggen")))
}
// Fixed reasons only. Never emit a debug-formatted FailureRecord or arbitrary
// partial_result text at this public CLI boundary.
const REVIEWED_FAILURE_REASONS: &[&str] = &[
    "reviewed_contract_invalid",
    "reviewed_request_limit_or_schema",
    "reviewed_source_limit",
    "reviewed_duplicate_source_path",
    "reviewed_source_not_declared",
    "reviewed_source_size_overflow",
    "reviewed_total_source_limit",
    "reviewed_source_inventory_incomplete",
    "reviewed_source_budget_contract",
    "reviewed_source_budget_out_of_range",
    "reviewed_source_count_budget",
    "reviewed_source_total_budget",
    "reviewed_stream_source_read",
    "reviewed_reconciliation_schema",
    "reviewed_duplicate_decision",
    "reviewed_decision_not_affected",
    "reviewed_package_changed",
    "reviewed_source_inventory_changed",
    "artifact_contract_invalid",
    "artifact_source_changed",
    "input_exceeds_limit",
    "invalid relative path",
    "path must remain inside its package",
    "invalid path characters",
];

fn reviewed_port_error(error: oe_contracts::FailureRecord) -> anyhow::Error {
    let reason = error.partial_result.as_ref().and_then(|v| {
        (matches!(
            error.phase.as_str(),
            "reviewed_reconciliation" | "reviewed_reconciliation2"
        ) && error.code == oe_contracts::ErrorCode::GovernanceDenied
            && !error.retryable
            && v["execution"] == "failed"
            && v["conclusion"] == "unknown"
            && v["engineering_adoption"] == "not_established")
            .then(|| v["reason"].as_str())
            .flatten()
    });
    let fixed = reason
        .filter(|value| REVIEWED_FAILURE_REASONS.contains(value))
        .unwrap_or("input_or_output_contract_unavailable");
    anyhow::anyhow!(fixed.to_owned())
}

/// Only expose explicit, known reasons. Error details can contain paths or source text.
pub fn command_failure(family: &str, error: &anyhow::Error) -> Value {
    let reasons = [
        "project_home_must_be_absolute",
        "project_home_parent_missing",
        "project_home_parent_not_directory",
        "project_home_not_supported_for_command",
        "project_data_root_absolute_path_required",
        "project_data_root_unavailable",
        "project_data_root_not_directory",
        "project_data_root_contains_software_root",
        "project_data_root_inside_software_sources",
        "harness_input_outside_project_home",
        "workspace_explicit_project_home_required",
        "workspace_contract",
        "workspace_invalid_id",
        "workspace_identity_mismatch",
        "workspace_project_identity_mismatch",
        "workspace_no_committed_state",
        "workspace_head_identity",
        "workspace_commit_identity",
        "workspace_commit_location",
        "workspace_archive_changed",
        "workspace_invalid_route",
        "workspace_request_history_rewritten",
        "workspace_goal_change_needs_source_request",
        "workspace_correction_obligation_dropped",
        "workspace_correction_resolution_missing",
        "workspace_unresolved_correction_closes_acceptance",
        "workspace_stage_must_start_planned",
        "workspace_stage_transition_invalid",
        "workspace_unfinished_action_cannot_disappear",
        "workspace_stale_resume",
        "workspace_not_ready_for_stage",
        "workspace_stage_not_planned",
        "workspace_not_ready_for_learning",
        "workspace_learning_goal_invalid",
        "workspace_learning_root_changed",
        "workspace_learning_acceptance_changed",
        "workspace_learning_binding_changed",
        "workspace_learning_project_mismatch",
        "workspace_learning_object_mismatch",
        "explicit_existing_cdp_endpoint_required_without_profile_discovery",
        "existing_cdp_endpoint_invalid",
        "existing_cdp_loopback_endpoint_required",
        "existing_cdp_listener_not_owned_by_current_chrome",
        "existing_cdp_version_unconfirmed",
        "existing_cdp_websocket_scope_mismatch",
        "peekaboo_normal_browser_window_not_observed",
        "peekaboo_exact_window_identity_required",
        "peekaboo_ax_observation_unconfirmed",
        "current_browser_policy_requires_peekaboo_background_no_legacy_fallback",
        "supplier_requires_explicit_peekaboo_background_page",
        "existing_peekaboo_socket_required",
        "peekaboo_connection_or_operation_unconfirmed",
        "peekaboo_provider_identity_changed",
        "peekaboo_active_application_changed",
        "peekaboo_active_application_changed_since_connection",
        "peekaboo_target_focus_changed",
        "peekaboo_target_source_changed",
        "peekaboo_exact_target_url_mismatch",
        "peekaboo_session_identity_changed",
        "peekaboo_chrome_instance_changed_during_operation",
        "peekaboo_required_permissions_not_granted",
        "peekaboo_permissions_unknown",
        "peekaboo_executable_pin_changed",
        "peekaboo_compiled_transport_assets_changed",
        "peekaboo_real_connection_operation_required",
        "current_task_view_stale",
        "current_task_method_source_refuted",
        "current_task_method_source_freshness_unknown",
        "current_task_method_view_stale",
        "current_task_method_input_schema",
        "current_task_method_input_invalid",
        "current_task_method_view_invalid",
        "current_task_method_project_changed_during_projection",
        "current_task_method_source_changed_during_projection",
        "current_task_source_freshness_unknown",
        "current_task_project_changed_during_projection",
        "current_task_source_changed_during_projection",
        "current_task_work_project_reference_changed",
        "current_task_source_not_project_evidence",
        "current_task_input_invalid",
        "current_task_input_schema",
        "engineering_reference_drift",
        "legacy_skill_root_read_only",
        "invalid_arguments",
        "cad_variant_native_source_drift",
        "cad_variant_native_unit_mismatch",
        "cad_variant_project_changed_during_work",
        "cad_variant_native_source_changed",
        "hole_native_count_mismatch",
        "hole_native_axis_or_diameter_missing_or_ambiguous",
        "hole_native_tolerance_invalid",
        "hole_mating_count_does_not_fit",
        "hole_mating_pitch_mismatch",
        "hole_mating_duplicate_row",
        "hole_row_no_center_fits",
        "hole_row_nonpositive_parameter",
        "hole_decimal_invalid",
        "drawing_packet_inventory_not_closed",
        "drawing_pdf_observation_hash_mismatch",
        "drawing_issue_label_mismatch",
        "drawing_content_observation_missing",
        "motion_history_source_drift",
        "motion_history_source_changed",
        "receiver_source_drift",
        "experience_rejected_hypothesis_needs_new_basis",
        "experience_new_basis_repeats_old_source",
        "experience_prior_rejection_invalid",
        "experience_related_rejections_incomplete",
        "experience_prior_rejection_not_in_registered_history",
        "experience_duplicate_rejection_reference",
        "experience_registered_hypothesis_changed",
        "experience_experiment_registration_changed",
        "experience_experiment_scope_mismatch",
        "experience_retry_protocol_required",
        "experience_new_freeze_requires_protocol_2",
        "experience_new_protocol_needs_new_evaluation",
        "experience_current_context_required",
        "experience_necessary_review_removed",
        "experience_versioned_method_plan_required",
        "engineering_current_project_context_changed_or_missing",
        "experience_candidate_already_exists",
        "experience_reference_drift",
        "experience_frozen_bytes_drift",
        "experience_freeze_identity_drift",
        "experience_frozen_evaluator_executable_drift",
        "experience_compiled_evaluator_changed_new_baseline_required",
        "experience_evaluator_changed_new_baseline_required",
        "experience_rules_changed_new_baseline_required",
        "experience_trial_not_admissible",
        "experience_assessment_not_reproducible",
        "experience_focus_scope_mismatch",
        "experience_project_identity_mismatch",
        "experience_necessary_criteria_changed",
        "experience_necessary_checks_changed",
        "experience_independent_exposure_not_attested",
        "experience_evaluation_budget_exceeded",
        "experience_trial_rule_family_not_frozen",
        "experience_transition_refused",
        "receiver_bytes_mismatch",
        "receiver_target_exists",
        "receiver_v1_read_only",
        "receiver_cannot_write_managed_source",
        "package_inventory_not_sorted_or_duplicate",
        "package_nonportable_path",
        "package_private_or_generated_path",
        "package_required_file_missing",
        "source_symlink_refused",
        "exclusive_directory_rename_failed",
        "exclusive_directory_rename_unsupported_platform",
        "directory_publish_after_rename_identity_changed",
        "windows_path_identity_unavailable",
        "anchored_directory_creation_failed",
        "anchored_directory_creation_platform_unsupported",
        "directory_path_changed",
        "portable_source_path_outside_root",
        "portable_source_path_invalid",
        "portable_source_path_not_utf8",
        "windows_bundle_root_unavailable",
        "windows_bundle_parent_unavailable",
        "windows_bundle_file_unsafe_or_unavailable",
        "windows_bundle_file_missing",
        "windows_bundle_read_failed",
        "semantic_query_thread_unavailable",
        "semantic_query_panicked",
        "cad_instance_correspondence_project_identity_mismatch",
        "cad_instance_correspondence_current_project_missing_change_source",
        "cad_occurrence_assembly_object_source_differs",
        "cad_occurrence_native_result_not_in_project",
        "cad_focus_current_context_mismatch",
        "cad_construction_source_not_in_project",
        "cad_construction_native_result_not_in_project",
        "cad_construction_object_revision_or_conditions_differ",
        "cad_delete_assembly_source_not_in_project",
        "cad_delete_assembly_transition_invalid",
        "cad_delete_changes_missing",
        "cad_delete_correspondence_changed",
        "cad_delete_current_detail_required",
        "cad_delete_current_target_missing",
        "cad_delete_decision_id_missing",
        "cad_delete_definition_assembly_source_differs",
        "cad_delete_definition_transition_invalid",
        "cad_delete_event_missing",
        "cad_delete_event_order_differs",
        "cad_delete_execution_identity_changed",
        "cad_delete_impact_decisions_missing",
        "cad_delete_independent_assembly_process_required",
        "cad_delete_independent_detail_process_required",
        "cad_delete_intent_digest_missing",
        "cad_delete_intent_input_differs",
        "cad_delete_intent_input_missing",
        "cad_delete_inventory_owner_differs",
        "cad_delete_stage_detail_owner_differs",
        "cad_delete_native_action_differs",
        "cad_delete_native_result_not_in_project",
        "cad_delete_original_binding_missing",
        "cad_delete_original_closure_differs",
        "cad_delete_original_declaration_missing",
        "cad_delete_original_definition_differs",
        "cad_delete_original_prototype_missing",
        "cad_delete_original_selector_differs",
        "cad_delete_original_source_differs",
        "cad_delete_parameters_differ",
        "cad_delete_previous_detail_required",
        "cad_delete_project_dependency_missing",
        "cad_delete_project_identity_differs",
        "cad_delete_readback_identity_differs",
        "cad_delete_reference_set_capture_required",
        "cad_delete_reference_sets_changed",
        "cad_delete_request_invalid",
        "cad_delete_save_incomplete",
        "cad_delete_saved_closure_differs",
        "cad_delete_saved_namespace_missing",
        "cad_delete_saved_prototype_differs",
        "cad_delete_saved_prototype_missing",
        "cad_delete_saved_root_differs",
        "cad_delete_schema_or_id_invalid",
        "cad_delete_selection_differs",
        "cad_delete_source_input_missing",
        "cad_delete_source_not_in_project",
        "cad_delete_sources_changed_during_check",
        "cad_delete_sources_changed_during_project_check",
        "cad_copy_current_instance_not_qualified",
        "cad_copy_event_order_differs",
        "cad_copy_execution_identity_changed",
        "cad_copy_impact_decision_id_missing",
        "cad_copy_impact_decisions_missing",
        "cad_copy_independent_process_required",
        "cad_copy_inspected_reference_sets_missing",
        "cad_copy_inspection_digest_missing",
        "cad_copy_inspection_not_in_project",
        "cad_copy_intent_digest_missing",
        "cad_copy_intent_input_differs",
        "cad_copy_intent_input_missing",
        "cad_copy_native_action_differs",
        "cad_copy_native_parameters_differ",
        "cad_copy_native_result_not_in_project",
        "cad_copy_object_or_version_transition_invalid",
        "cad_copy_object_revision_or_conditions_differ",
        "cad_copy_original_closure_differs",
        "cad_copy_original_prototype_missing",
        "cad_copy_original_source_bytes_differ",
        "cad_copy_project_dependency_missing",
        "cad_copy_project_identity_mismatch",
        "cad_copy_readback_identity_differs",
        "cad_copy_reference_set_capture_required",
        "cad_copy_request_invalid",
        "cad_copy_save_incomplete",
        "cad_copy_saved_closure_differs",
        "cad_copy_saved_namespace_missing",
        "cad_copy_saved_prototype_differs",
        "cad_copy_saved_prototype_missing",
        "cad_copy_saved_root_differs",
        "cad_copy_schema_or_source_id_invalid",
        "cad_copy_source_input_missing",
        "cad_copy_source_not_in_project",
        "cad_copy_sources_changed_during_check",
        "cad_copy_sources_changed_during_project_check",
        "project_identity_invalid",
        "project_binding_identity_invalid",
        "project_decision_invalid",
        "project_unknown_cannot_be_adopted",
        "project_adoption_basis_missing",
        "project_identity_cannot_change",
        "ggen_binary_missing",
        "ggen_binary_identity_mismatch",
        "ggen_execution_failed",
        "ggen_binary_changed_during_execution",
        "generation_source_set_changed",
        "generation_source_changed",
        "generation_staged_product_drift",
        "generation_constraint_checker_missing",
        "compiled_source_binding_mismatch",
        "compiled_generated_contract_binding_mismatch",
        "token_calibration_asset_invalid",
        "token_calibration_resource_limit",
        "token_calibration_input_unsupported",
        "token_calibration_arithmetic_overflow",
        "compiled_semantic_contract_binding_mismatch",
        "schema_input_sha_mismatch",
        "schema_input_json_invalid",
        "schema_request_invalid",
        "schema_model_unknown",
        "schema_nonfinite_profile_unavailable",
        "schema_python_representation_profile_unavailable",
        "schema_python_float_overflow",
        "schema_numeric_bound_profile_unavailable",
        "schema_typed_snapshot_unrepresentable",
        "schema_typed_snapshot_identity_changed",
        "schema_typed_source_mismatch",
        "generation_atomic_exchange_failed",
        "atomic_generation_publication_unsupported_platform",
        "design_spec_changed_during_load",
        "design_spec_changed_during_generation",
        "governance_authority_missing",
        "governance_cannot_skip_state",
        "governance_baseline_changed",
        "governance_duplicate_successor",
        "governance_required_evidence_missing_or_duplicate",
        "governance_evidence_drift",
        "governance_current_package_baseline_changed",
        "governance_fresh_regression_not_passed",
        "external_publication_verification_not_implemented",
    ];
    let reason = reasons
        .into_iter()
        .chain(
            if family == "project" {
                REVIEWED_FAILURE_REASONS
            } else {
                &[]
            }
            .iter()
            .copied(),
        )
        .find(|reason| {
            error.chain().any(|e| {
                let text = e.to_string();
                text == *reason
                    || text
                        .split_once(':')
                        .is_some_and(|(head, _)| head == *reason)
            })
        })
        .unwrap_or("input_or_output_contract_unavailable");
    use oe_contracts::ErrorCode;
    let code = if reason.starts_with("governance_") || reason.starts_with("external_publication_") {
        ErrorCode::GovernanceDenied
    } else if [
        "ggen_binary_missing",
        "ggen_binary_identity_mismatch",
        "ggen_binary_changed_during_execution",
    ]
    .contains(&reason)
    {
        ErrorCode::GenerationIdentityMismatch
    } else if reason.starts_with("generation_source_")
        || reason.starts_with("compiled_source_")
        || reason.starts_with("compiled_generated_")
        || reason.starts_with("design_spec_changed_")
        || reason == "receiver_source_drift"
    {
        ErrorCode::BoundInputChanged
    } else if reason.starts_with("generation_")
        || reason == "ggen_execution_failed"
        || reason == "atomic_generation_publication_unsupported_platform"
    {
        ErrorCode::GenerationFailed
    } else {
        ErrorCode::InvalidContract
    };
    json!({"schema":"oev2.command-failure/1","command":family,"execution":"failed","conclusion":"unknown","coverage":[],"engineering_adoption":"blocked","error":{"code":code,"reason":reason},"retryable":false,"next_step":"inspect_declared_contract_and_retained_private_evidence"})
}
pub struct RustReviews;
pub fn review_exit_status(report: &Value) -> u8 {
    let Some(reviews) = report["reviews"].as_array() else {
        return 1;
    };
    if report["status"] == "not_run" && reviews.is_empty() {
        return 0;
    }
    if report["status"] == "completed"
        && !reviews.is_empty()
        && reviews.iter().all(|r| r["review"]["status"] == "clear")
    {
        0
    } else {
        1
    }
}
impl oe_engineering::harness::ReviewExecutor for RustReviews {
    fn execute(
        &self,
        root: &std::path::Path,
        requests: &Value,
        project_id: &str,
        output: &std::path::Path,
    ) -> Result<Value> {
        require_execution(&SkillRoot::new(root)?, "formal_project_review")?;
        oe_semantic::review::execute_reviews_with(
            root,
            requests,
            project_id,
            output,
            &oe_kernel::forward::RustExecutor,
        )
    }

    fn execute_in_context(
        &self,
        root: &SkillRoot,
        requests: &Value,
        project_id: &str,
        output: &std::path::Path,
    ) -> Result<Value> {
        require_execution(root, "formal_project_review")?;
        oe_semantic::review::execute_reviews_in_context_with(
            root,
            requests,
            project_id,
            output,
            &oe_kernel::forward::RustExecutor,
        )
    }
}
pub fn design(root: &SkillRoot, command: DesignCommand) -> Result<u8> {
    let execution = if matches!(
        &command,
        DesignCommand::Validate { .. }
            | DesignCommand::Normalize { .. }
            | DesignCommand::Generate { .. }
            | DesignCommand::Contract { .. }
            | DesignCommand::SemanticaScope { .. }
    ) {
        Some(require_execution(root, "formal_design")?)
    } else {
        None
    };
    match command {
        DesignCommand::SemanticaScope {
            output,
            require_complete,
        } => {
            let mut report = oe_design::migration::audit(root.path())?;
            report["execution_binding"] = serde_json::to_value(&execution)?;
            let incomplete = report["migration_status"] != "complete";
            if let Some(path) = &output {
                write_new(&root.private_path(path)?, &report)?;
            }
            let item_count = report["items"].as_array().map_or(0, Vec::len);
            report
                .as_object_mut()
                .context("migration_scope_report_not_object")?
                .remove("items");
            report["scope_item_count"] = item_count.into();
            report["full_items_output"] = serde_json::to_value(&output)?;
            emit(root, report, None, require_complete && incomplete)
        }
        DesignCommand::Validate { spec } => {
            let path = root.expand(&spec)?;
            let d = oe_design::load_current(root, &path)?;
            let bytes = read_regular_bytes(&path, 4 * 1024 * 1024)?;
            let source = if oe_design::composition::transport::schema(&bytes)
                .ok()
                .as_deref()
                .is_some_and(oe_design::composition::transport::is_composed)
            {
                let loaded = oe_design::composition::load(root.path(), &path)?;
                Some(oe_design::composition::require_current(root, &loaded)?)
            } else {
                None
            };
            let mut value = json!({"status":"valid","version":d.version,"modules":d.modules.len(),"scope":"design validation only","execution_binding":execution});
            if let Some(source) = source {
                value["input_schema"] = source["input_schema"].clone();
                value["composition"] = source;
            }
            emit(root, value, None, false)
        }
        DesignCommand::Normalize { spec, output } => {
            let path = root.expand(&spec)?;
            let d = oe_design::load_current(root, &path)?;
            let bytes = read_regular_bytes(&path, 4 * 1024 * 1024)?;
            let composition = if oe_design::composition::transport::schema(&bytes)
                .ok()
                .as_deref()
                .is_some_and(oe_design::composition::transport::is_composed)
            {
                let loaded = oe_design::composition::load(root.path(), &path)?;
                oe_design::composition::require_current(root, &loaded)?;
                Some(loaded)
            } else {
                None
            };
            let (model, _) = oe_design::model(root.path(), &d)?;
            let output = root.new_private_dir(&output)?;
            write_new_bytes(&output.join("model.ttl"), model.as_bytes())?;
            let normalized = match &composition {
                Some(loaded) => loaded.normalized()?,
                None => d.canonical()?,
            };
            if composition.is_some() {
                ensure!(
                    normalized.len() as u64 <= oe_core::source_limits::PUBLIC_FILE_BYTES,
                    "normalized_composition_artifact_budget"
                );
            }
            write_new_bytes(&output.join("normalized-design.json"), &normalized)?;
            if let Some(loaded) = &composition {
                loaded.verify(root.path())?;
                write_new_bytes(
                    &output.join("design-composition-source.json"),
                    &loaded.source_bytes()?,
                )?;
            }
            oe_core::write_new(
                &output.join("execution-binding.json"),
                execution
                    .as_ref()
                    .context("normalization_execution_binding_missing")?,
            )?;
            emit(
                root,
                json!({"status":"normalized","model_sha256":oe_core::sha256(model.as_bytes()),"output":output,"semantic_review":"not_run"}),
                None,
                false,
            )
        }
        DesignCommand::Inspect { spec, output } => {
            let v = oe_design::inspection::inspect(
                root.path(),
                &oe_design::load(&root.expand(&spec)?)?,
            )?;
            let failed = v["status"] != "no_static_findings";
            emit(root, v, output, failed)
        }
        DesignCommand::Generate { spec, ggen } => {
            let (out, v) = oe_design::generation::run_spec(
                root,
                &root.expand(&spec)?,
                &ggen_path(root, ggen)?,
            )?;
            let binding = oe_design::provenance::execution(root, "generation_publication")?;
            let records = root.unique_private_dir(
                std::path::Path::new("var/maintenance"),
                "compiler-execution",
            )?;
            write_new(&records.join("binding.json"), &binding)?;
            oe_design::generation::publish(root, &out)?;
            emit(root, v, None, false)
        }
        DesignCommand::Check { spec, ggen, output } => {
            let v = oe_design::generation::check_spec(
                root,
                &root.expand(&spec)?,
                &ggen_path(root, ggen)?,
            )?;
            let failed = v["status"] != "consistent";
            emit(root, v, output, failed)
        }
        DesignCommand::Impact {
            before,
            after,
            output,
        } => {
            let a = oe_design::load(&root.expand(&before)?)?;
            let b = oe_design::load(&root.expand(&after)?)?;
            let changed = oe_design::changes(&a, &b)?;
            let affected: BTreeSet<_> = a
                .impact(&changed)
                .union(&b.impact(&changed))
                .cloned()
                .collect();
            emit(
                root,
                json!({"changed":changed,"affected":affected,"scope":"declared dependency closure; undeclared dependencies remain unknown"}),
                output,
                false,
            )
        }
        DesignCommand::Contract { input, kind } => {
            let v = json_input(root, &input)?;
            let result = match kind.as_str() {
                "Quantity" => serde_json::from_value::<oe_contracts::Quantity>(v)?.validate(),
                "ObjectIdentity" => {
                    serde_json::from_value::<oe_contracts::ObjectIdentity>(v)?.validate()
                }
                "SourceCitation" => {
                    serde_json::from_value::<oe_contracts::SourceCitation>(v)?.validate()
                }
                _ => anyhow::bail!("contract_kind_unsupported"),
            };
            let failed = result.is_err();
            emit(
                root,
                json!({"status":if failed{"rejected"}else{"valid"},"failures":result.err().unwrap_or_default()}),
                None,
                failed,
            )
        }
    }
}
pub fn kernel(root: &SkillRoot, command: KernelCommand) -> Result<u8> {
    if !matches!(&command, KernelCommand::Capabilities) {
        require_execution(root, "formal_semantic_execution")?;
    }
    match command {
        KernelCommand::Capabilities => emit(root, oe_kernel::capabilities(), None, false),
        KernelCommand::SnapshotChecksum { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_SNAPSHOT_CHECKSUM_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_snapshot_checksum(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::TemporalVersionDiff { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_VERSION_DIFF_WIRE_BYTES,
            )?;
            let result =
                oe_kernel::rules::execute_local_temporal_version_diff(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::TemporalVersionRecords { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_VERSION_RECORDS_WIRE_BYTES,
            )?;
            let result =
                oe_kernel::rules::execute_local_temporal_version_records(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::TemporalPatterns { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_PATTERNS_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_temporal_patterns(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::TemporalPaths { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_PATHS_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_temporal_paths(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::TemporalConsistency { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_CONSISTENCY_WIRE_BYTES,
            )?;
            let result =
                oe_kernel::rules::execute_local_temporal_consistency(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::TemporalRange { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_RANGE_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_temporal_range(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::TemporalSnapshot { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_SNAPSHOT_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_temporal_snapshot(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::TemporalFacts { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_FACTS_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_temporal_facts(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::Temporal { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_TEMPORAL_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_temporal(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::DatalogSession { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_DATALOG_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_datalog_session(oe_wire_json(&input)?)?;
            let failed = result["all_actions_succeeded"] != true;
            emit(root, result, output, failed)
        }
        KernelCommand::DatalogGraph { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_DATALOG_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_datalog_graph(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::Datalog { input, output } => {
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::rules::MAX_DATALOG_WIRE_BYTES,
            )?;
            let result = oe_kernel::rules::execute_local_datalog(oe_wire_json(&input)?)?;
            emit(root, result, output, false)
        }
        KernelCommand::SourceSession { input, output } => {
            source_session::execute_host(root, input, output)
        }
        KernelCommand::SourceSessionWorker => source_session::execute_worker(),
        KernelCommand::Reason { input, output } => emit(
            root,
            oe_kernel::rules::execute_local_reason(json_input(root, &input)?)?,
            output,
            false,
        ),
        KernelCommand::QueryOptions { input, output } => emit(
            root,
            oe_kernel::query::execute_local_query_options(json_input(root, &input)?)?,
            output,
            false,
        ),
        KernelCommand::Query {
            data,
            query,
            output,
        } => emit(
            root,
            oe_kernel::query::execute_local_query(
                &bytes(root, &data)?,
                std::str::from_utf8(&software_bytes(root, &query)?)?,
            )?,
            output,
            false,
        ),
        KernelCommand::ValidateNamed { input, output } => {
            // The old json_input limit is one 16 MiB file. This explicit wire
            // carries both original files plus escaping; decoded RDF bounds
            // remain with the existing semantic owner.
            let input = read_regular_bytes(
                &root.data_expand(&input)?,
                oe_kernel::shacl::MAX_NAMED_GRAPH_VALIDATION_WIRE_BYTES,
            )?;
            let value =
                oe_kernel::shacl::execute_local_named_graph_validation(oe_wire_json(&input)?)?;
            let failed = value["report"]["conforms"] != true;
            emit(root, value, output, failed)
        }
        KernelCommand::Validate {
            data,
            shapes,
            output,
        } => {
            let v =
                oe_kernel::shacl::validate(&bytes(root, &data)?, &software_bytes(root, &shapes)?)?;
            let failed = v["conforms"] != true;
            emit(root, v, output, failed)
        }
        KernelCommand::Rules {
            facts,
            rules,
            output,
        } => emit(
            root,
            oe_kernel::rules::forward_chain(
                std::str::from_utf8(&bytes(root, &facts)?)?,
                std::str::from_utf8(&software_bytes(root, &rules)?)?,
            )?,
            output,
            false,
        ),
        KernelCommand::Regress { bundle, output } => {
            let v = oe_kernel::packages::regress(root.path(), &bundle)?;
            let failed = v["status"] != "passed";
            emit(root, v, output, failed)
        }
        KernelCommand::Review {
            binding,
            evidence,
            bundle,
            query,
            shape,
            prior,
            project,
            historical_snapshot,
            output,
        } => {
            let binding: oe_kernel::Binding = serde_json::from_value(json_input(root, &binding)?)?;
            let evidence = bytes(root, &evidence)?;
            let loaded = oe_semantic::load_bundle(root.path(), &bundle)?;
            ensure!(
                binding.package_id == loaded.spec["package_id"]
                    && binding.package_version == loaded.spec["package_version"]
                    && binding.package_sha256 == loaded.spec["sha256"],
                "review_package_binding_mismatch"
            );
            let prior: Option<oe_kernel::Receipt> = prior
                .map(|p| json_input(root, &p).and_then(|v| Ok(serde_json::from_value(v)?)))
                .transpose()?;
            let request = oe_contracts::ReviewRequest {
                object: oe_contracts::ObjectIdentity {
                    object_id: binding.object_id.clone(),
                    revision: binding.object_revision.clone(),
                    condition_scope: binding.condition_scope.clone(),
                    source_sha256: binding.source_sha256.clone(),
                },
                evidence_sha256: oe_core::sha256(&evidence),
                required_checks: binding.required_checks.iter().cloned().collect(),
            };
            let port = oe_kernel::port::RustSemanticPort {
                root: root.path().to_owned(),
                binding,
                bundle,
                queries: query,
                shapes: shape,
                evidence,
            };
            let mut receipt = port.execute(&request, prior.as_ref())?;
            if let Some(project_path) = project {
                let project_raw = bytes(root, &project_path)?;
                let project = serde_json::from_value(oe_wire_json(&project_raw)?)?;
                receipt =
                    oe_kernel::project::review_current(root, &project, &port.evidence, receipt)?;
                ensure!(
                    bytes(root, &project_path)? == project_raw,
                    "current_project_changed_during_review"
                );
            } else if !historical_snapshot {
                let mut checks = receipt.checks;
                checks.push(oe_kernel::CheckResult { id:"current_project_context".into(),
                    execution:"not_run".into(), conclusion:"unknown".into(), coverage:vec![],
                    failures:vec!["current_project_reference_missing".into()],
                    result:json!({"current_use":"blocked","historical_checks":"executed and retained",
                        "required":"--project current Project, or explicit --historical-snapshot"}) });
                receipt = oe_kernel::evaluate(receipt.binding, &port.evidence, checks)?;
            }
            let failed = receipt.conclusion != "pass";
            emit(root, serde_json::to_value(receipt)?, Some(output), failed)
        }
        KernelCommand::Transition { ledger, event } => {
            let event = serde_json::from_value(json_input(root, &event)?)?;
            emit(
                root,
                oe_kernel::governance::transition(root, &ledger, event)?,
                None,
                false,
            )
        }
    }
}
pub fn project(root: &SkillRoot, command: ProjectCommand) -> Result<u8> {
    match command {
        ProjectCommand::Electronics { command } => {
            let (value, status) = crate::electronics::run(root, command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(status)
        }
        ProjectCommand::SourceProjection { command } => {
            let (value, status) = crate::source_projection::run(root, command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(status)
        }
        ProjectCommand::Performance { command } => {
            let (value, status) = crate::performance::run(root, command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(status)
        }
        ProjectCommand::Jacobian { command } => {
            let (value, status) = crate::jacobian::run(root, command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(status)
        }
        ProjectCommand::Workspace { command } => crate::workspace::execute(root, command),
        ProjectCommand::ReconcileReviewed { input, output } => {
            require_execution(root, "reviewed_reconciliation")?;
            use oe_contracts::{ReviewedReconciliationPort, ReviewedReconciliationPort2};
            let request = json_input(root, &input)?;
            let owner =
                oe_engineering::reviewed_reconciliation::ReviewedReconciliationExecutor { root };
            match request.get("schema").and_then(serde_json::Value::as_str) {
                Some("oev2.reviewed-reconciliation-request/1") => {
                    let result = owner
                        .reconcile_reviewed(&serde_json::from_value(request)?)
                        .map_err(reviewed_port_error)?;
                    let incomplete = result.status != "review_complete";
                    emit(root, serde_json::to_value(result)?, output, incomplete)
                }
                Some("oev2.reviewed-reconciliation-request/2") => {
                    let result = owner
                        .reconcile_reviewed2(&serde_json::from_value(request)?)
                        .map_err(reviewed_port_error)?;
                    let incomplete = result.status != "review_complete";
                    emit(root, serde_json::to_value(result)?, output, incomplete)
                }
                _ => anyhow::bail!("reviewed_reconciliation_schema"),
            }
        }
        ProjectCommand::Decide {
            project,
            input,
            registry,
            revision,
            output,
            receipt,
        } => {
            let (value, failed) = crate::tool_result::decide(
                root,
                &project,
                &input,
                &registry,
                &revision,
                &output,
                receipt.as_deref(),
            )?;
            emit(root, value, None, failed)
        }
        ProjectCommand::MemoryCandidate { command } => {
            let (value, failed) = crate::tool_result::memory_candidate(root, command)?;
            emit(root, value, None, failed)
        }
        ProjectCommand::MemoryFeedback {
            project,
            input,
            id,
            outcome,
            result,
            sources,
            elapsed_ms,
            unknowns,
            context_id,
            context_sha256,
            output,
        } => {
            let args = crate::tool_result::MemoryFeedbackArgs {
                project,
                input,
                id,
                outcome,
                result,
                sources,
                elapsed_ms,
                unknowns,
                context_id,
                context_sha256,
                output,
            };
            let (value, failed) = crate::tool_result::memory_feedback(root, args)?;
            emit(root, value, None, failed)
        }
        ProjectCommand::MemoryExport { project, output } => {
            let (value, failed) = crate::tool_result::memory_export(root, &project, &output)?;
            emit(root, value, None, failed)
        }
        ProjectCommand::ToolResult { command } => {
            let (value, failed) = crate::tool_result::run(root, command)?;
            emit(root, value, None, failed)
        }
        ProjectCommand::Explore { command } => {
            let value = crate::exploration::run(root, command)?;
            let pending = value["schema"] == "oev2.exploration-host-state/1"
                && value["status"] != "results_recorded";
            emit(root, value, None, pending)
        }
        ProjectCommand::DesignComparison { command } => emit(
            root,
            crate::design_comparison::run(root, command)?,
            None,
            false,
        ),
        ProjectCommand::DesignAttempt { command } => emit(
            root,
            crate::design_attempt::run(root, command)?,
            None,
            false,
        ),
        ProjectCommand::Incad { command } => {
            emit(root, crate::incad::run(root, command)?, None, false)
        }
        ProjectCommand::TaskView { input, output } => {
            let value = json_input(root, &input)?;
            if value["schema"] == oe_engineering::current_task_view::method_use::INPUT_SCHEMA {
                let request = serde_json::from_value(value)?;
                let view = oe_engineering::current_task_view::method_use::derive_with_methods(
                    root, &request,
                )?;
                return emit(root, serde_json::to_value(view)?, output, false);
            }
            let request = serde_json::from_value(value)?;
            let view = oe_engineering::current_task_view::derive(root, &request)?;
            emit(root, serde_json::to_value(view)?, output, false)
        }
        ProjectCommand::VerifyTaskView { input } => {
            let value = json_input(root, &input)?;
            if value["schema"] == oe_engineering::current_task_view::method_use::VIEW_SCHEMA {
                let view = serde_json::from_value(value)?;
                oe_engineering::current_task_view::method_use::verify_method_view(root, &view)?;
                return emit(
                    root,
                    json!({"schema":"oev2.current-task-view-freshness/2","status":"matched","proof_scope":"selected_message_provenance_only","scope":"Original complete Project/work/checks plus explicit intake/archive and selected raw messages; unselected session contents, method truth, unseen benefit and engineering adoption remain unknown","model_execution":"not_run","project_update":"none","engineering_adoption":"not_established"}),
                    None,
                    false,
                );
            }
            let view = serde_json::from_value(value)?;
            oe_engineering::current_task_view::verify_view(root, &view)?;
            emit(
                root,
                json!({"schema":"oev2.current-task-view-freshness/1","status":"matched","scope":"current raw/typed Project and explicit bound inputs only; unsupplied source bodies, engineering adoption and task scheduling remain unknown"}),
                None,
                false,
            )
        }
        ProjectCommand::ModelAssistance { command } => {
            crate::model_assistance::execute(root, *command)
        }
        ProjectCommand::Adopt { input, output } => {
            require_execution(root, "engineering_adoption")?;
            use oe_contracts::EngineeringAdoptionPort;
            let request = serde_json::from_value(json_input(root, &input)?)?;
            let result = oe_engineering::adoption::AdoptionExecutor {
                root,
                output: &output,
            }
            .adopt(&request)
            .map_err(|e| anyhow::anyhow!("engineering_adoption_port:{e:?}"))?;
            emit(root, serde_json::to_value(result)?, None, false)
        }
        ProjectCommand::VerifyAdoption { input } => {
            require_execution(root, "engineering_adoption_verify")?;
            emit(
                root,
                oe_engineering::adoption::verify(root, &json_input(root, &input)?)?,
                None,
                false,
            )
        }
        ProjectCommand::AdoptPromoted { input, output } => {
            require_execution(root, "engineering_promoted_adoption2")?;
            use oe_contracts::EngineeringPromotedAdoptionPort2;
            let request = serde_json::from_value(json_input(root, &input)?)?;
            let result = oe_engineering::adoption::promoted2::PromotedAdoptionExecutor2 {
                root,
                output: &output,
            }
            .adopt(&request)
            .map_err(|e| anyhow::anyhow!("engineering_promoted_adoption2_port:{e:?}"))?;
            emit(root, serde_json::to_value(result)?, None, false)
        }
        ProjectCommand::VerifyPromotedAdoption { input } => {
            require_execution(root, "engineering_promoted_adoption_verify2")?;
            use oe_contracts::EngineeringPromotedAdoptionVerifyPort2;
            let request = serde_json::from_value(json_input(root, &input)?)?;
            let result =
                oe_engineering::adoption::promoted2::PromotedAdoptionVerifyExecutor2 { root }
                    .verify(&request)
                    .map_err(|e| {
                        anyhow::anyhow!("engineering_promoted_adoption_verify2_port:{e:?}")
                    })?;
            emit(root, serde_json::to_value(result)?, None, false)
        }
        ProjectCommand::Context {
            input,
            object,
            output,
        } => {
            require_execution(root, "project_context")?;
            let raw = bytes(root, &input)?;
            let project = serde_json::from_value(oe_wire_json(&raw)?)?;
            let object = serde_json::from_value(json_input(root, &object)?)?;
            let context = oe_kernel::project::context(&project, &object)?;
            ensure!(
                bytes(root, &input)? == raw,
                "current_project_changed_during_review"
            );
            emit(root, serde_json::to_value(context)?, Some(output), false)
        }
        ProjectCommand::Work {
            input,
            output_dir,
            tools,
        } => {
            let binding = require_execution(root, "engineering_work")?;
            let input_path = root.data_expand(&input)?;
            let input_bytes = oe_core::read_regular_bytes(&input_path, 16 * 1024 * 1024)?;
            let work: oe_engineering::workflow::WorkInput =
                serde_json::from_value(oe_core::json::strict_json(&input_bytes)?)?;
            let output = root.new_private_dir(&output_dir)?;
            use oe_contracts::EngineeringWorkPort;
            let resolved_tools = tools.resolve()?;
            let request = oe_contracts::EngineeringWorkRequest {
                input: oe_contracts::ArtifactReference {
                    path: root.portable_data_path(&input_path)?,
                    sha256: oe_core::sha256(&input_bytes),
                },
            };
            let mut result = oe_engineering::ports::WorkExecutor {
                root,
                tools: &resolved_tools,
            }
            .work(&request)
            .map_err(|e| anyhow::anyhow!("engineering_work_port:{e:?}"))?
            .result;
            result["execution_binding"] = binding;
            result["work_input_sha256"] = json!(oe_core::sha256(&input_bytes));
            result["next_review"] = json!({"owner":"host_agent",
                "status":"required","authority":"source inquiry and calculation results require current Project review; no adoption"});
            ensure!(
                oe_core::read_regular_bytes(&input_path, 16 * 1024 * 1024)? == input_bytes,
                "engineering_input_changed_during_work"
            );
            write_new(&output.join("work.json"), &result)?;
            let failed = result["status"] == "partial";
            emit(
                root,
                json!({"status":result["status"],"project_id":result["project_id"],"receipt":root.portable_data_path(&output.join("work.json"))?,"engineering_conclusion":"unknown","adopted":false,"scope":"actual source inquiry and explicit calculations, not physical acceptance"}),
                None,
                failed,
            )
        }
        ProjectCommand::Validate { input } => {
            let p = serde_json::from_value(json_input(root, &input)?)?;
            oe_kernel::project::validate(&p)?;
            emit(
                root,
                json!({"status":"valid","engineering_acceptance":"not_run"}),
                None,
                false,
            )
        }
        ProjectCommand::RouteAudit {
            input,
            anchor,
            output,
        } => {
            let project_bytes = bytes(root, &input)?;
            let anchor_bytes = bytes(root, &anchor)?;
            let project = serde_json::from_value(oe_wire_json(&project_bytes)?)?;
            let anchor_value = oe_wire_json(&anchor_bytes)?;
            let (value, failed) = if anchor_value["schema"]
                == oe_kernel::project::route::ANCHOR2_SCHEMA
            {
                let anchor: oe_contracts::RouteAnchor2 = serde_json::from_value(anchor_value)
                    .context("route_anchor_contract_invalid")?;
                let report = oe_kernel::project::route::audit2(
                    &project,
                    &anchor,
                    &oe_core::sha256(&project_bytes),
                    &oe_core::sha256(&anchor_bytes),
                )?;
                (serde_json::to_value(&report)?, report.status != "anchored")
            } else {
                let anchor: oe_contracts::RouteAnchor = serde_json::from_value(anchor_value)
                    .context("route_anchor_contract_invalid")?;
                let report = oe_kernel::project::route::audit(
                    &project,
                    &anchor,
                    &oe_core::sha256(&project_bytes),
                    &oe_core::sha256(&anchor_bytes),
                )?;
                (serde_json::to_value(&report)?, report.status != "anchored")
            };
            emit(root, value, output, failed)
        }
        ProjectCommand::Impact {
            input,
            changed,
            output,
        } => {
            let p = serde_json::from_value(json_input(root, &input)?)?;
            emit(
                root,
                oe_kernel::project::impact(&p, &changed.into_iter().collect())?,
                output,
                false,
            )
        }
        ProjectCommand::Reconcile {
            before,
            after,
            output,
        } => {
            let a = serde_json::from_value(json_input(root, &before)?)?;
            let b = serde_json::from_value(json_input(root, &after)?)?;
            let v = oe_kernel::project::reconcile(&a, &b)?;
            let failed = !v["stale_adoptions"]
                .as_array()
                .context("impact_status_missing")?
                .is_empty();
            emit(root, v, output, failed)
        }
    }
}
pub fn maintain(root: &SkillRoot, command: MaintainCommand) -> Result<u8> {
    match command {
        MaintainCommand::RuntimeNative { input } => {
            let request: oe_contracts::CodeRustRuntimeDispatchRequest =
                serde_json::from_value(json_input(root, &input)?)?;
            let response = oe_evolution::runtime_identity::dispatch(root, &request)?;
            let failed = response.execution != "completed";
            emit(root, serde_json::to_value(response)?, None, failed)
        }
        MaintainCommand::Status => {
            let p = root.path().join("var/maintenance/status.json");
            let v = if p.is_file() {
                json_input(root, &p)?
            } else {
                json!({"status":"重构中","reason":"full capability and native/receiver evidence not closed"})
            };
            emit(root, v, None, false)
        }
        MaintainCommand::Inventory { output } => emit(
            root,
            serde_json::to_value(oe_design::inspection::inventory(root.path())?)?,
            output,
            false,
        ),
        MaintainCommand::Manifest { output } => emit(
            root,
            serde_json::to_value(oe_dist::release_build::inventory(root)?)?,
            Some(output),
            false,
        ),
        MaintainCommand::Install { inventory, target } => {
            let m = oe_dist::package::read_manifest(&root.expand(&inventory)?)?;
            let target = root.expand(&target)?;
            oe_core::no_symlinks(&target)?;
            ensure!(!target.exists(), "receiver_target_exists");
            crate::distribution::require_outside_v1(root, &target)?;
            ensure!(
                !target.starts_with(root.path()) || target.starts_with(root.path().join("var")),
                "receiver_cannot_write_managed_source"
            );
            // Validate the entire input before staging, then read it back before a single publication.
            for f in &m.files {
                let got = oe_core::fingerprint_file(
                    &root.path().join(&f.path),
                    oe_core::source_limits::public_file_bytes(std::path::Path::new(&f.path)),
                )?;
                ensure!(
                    got.sha256 == f.sha256 && got.bytes == f.bytes,
                    "receiver_source_drift"
                );
            }
            let parent = target.parent().context("receiver_parent_missing")?;
            oe_core::ensure_directory(parent)?;
            let stage = parent.join(format!(
                ".oev2-receiving-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
            oe_core::create_new_private_dir(&stage)?;
            for f in &m.files {
                let bytes = read_regular_bytes(
                    &root.path().join(&f.path),
                    oe_core::source_limits::public_file_bytes(std::path::Path::new(&f.path))
                        as usize,
                )?;
                ensure!(
                    oe_core::sha256(&bytes) == f.sha256 && bytes.len() as u64 == f.bytes,
                    "receiver_source_drift"
                );
                let p = stage.join(&f.path);
                oe_core::ensure_directory(p.parent().context("receiver_parent_missing")?)?;
                write_new_bytes(&p, &bytes)?;
            }
            for f in &m.files {
                let got = oe_core::fingerprint_file(
                    &stage.join(&f.path),
                    oe_core::source_limits::public_file_bytes(std::path::Path::new(&f.path)),
                )?;
                ensure!(
                    got.sha256 == f.sha256 && got.bytes == f.bytes,
                    "receiver_bytes_mismatch"
                );
            }
            oe_core::rename_directory_exclusive(&stage, &target)?;
            emit(
                root,
                json!({"schema":"oev2.receiver/1","status":"source_received","files":m.files.len(),"build":"not_run","target":target,"global_oe":"not_changed"}),
                None,
                false,
            )
        }
    }
}

#[cfg(test)]
mod cad_failure_tests {
    #[test]
    fn reviewed_failure_keeps_closed_reason_and_blocked_state() {
        for phase in ["reviewed_reconciliation", "reviewed_reconciliation2"] {
            for reason in super::REVIEWED_FAILURE_REASONS {
                let error = oe_engineering::artifacts::failure(phase, anyhow::anyhow!(*reason));
                let value = super::command_failure("project", &super::reviewed_port_error(error));
                assert_eq!(value["error"]["reason"], *reason);
                assert_eq!(value["execution"], "failed");
                assert_eq!(value["conclusion"], "unknown");
                assert_eq!(value["engineering_adoption"], "blocked");
                assert_eq!(value["retryable"], false);
            }
        }
    }
    #[test]
    fn reviewed_failure_never_emits_unlisted_or_forged_private_details() {
        for text in [
            "cannot read /private/customer file",
            "reviewed_source_limit:/private/customer file",
            "artifact_source_changed\n/private/customer file",
            "reviewed_source_limit_other",
        ] {
            let error = oe_engineering::artifacts::failure(
                "reviewed_reconciliation2",
                anyhow::anyhow!(text),
            );
            let value = super::command_failure("project", &super::reviewed_port_error(error));
            assert_eq!(
                value["error"]["reason"],
                "input_or_output_contract_unavailable"
            );
            assert!(!value.to_string().contains("/private/"));
        }
        for mutation in [
            "phase",
            "code",
            "retry",
            "execution",
            "conclusion",
            "adoption",
            "missing",
        ] {
            let mut error = oe_engineering::artifacts::failure(
                "reviewed_reconciliation2",
                anyhow::anyhow!("reviewed_source_limit"),
            );
            match mutation {
                "phase" => error.phase = "other".into(),
                "code" => error.code = oe_contracts::ErrorCode::SemanticUnknown,
                "retry" => error.retryable = true,
                "execution" => {
                    error.partial_result.as_mut().unwrap()["execution"] =
                        serde_json::json!("completed")
                }
                "conclusion" => {
                    error.partial_result.as_mut().unwrap()["conclusion"] = serde_json::json!("pass")
                }
                "adoption" => {
                    error.partial_result.as_mut().unwrap()["engineering_adoption"] =
                        serde_json::json!("granted")
                }
                "missing" => error.partial_result = None,
                _ => unreachable!(),
            }
            let value = super::command_failure("project", &super::reviewed_port_error(error));
            assert_eq!(
                value["error"]["reason"], "input_or_output_contract_unavailable",
                "{mutation}"
            );
        }
        let value = super::command_failure(
            "project",
            &anyhow::anyhow!("reviewed_reconciliation_schema"),
        );
        assert_eq!(value["error"]["reason"], "reviewed_reconciliation_schema");
        let other = super::command_failure("cad", &anyhow::anyhow!("reviewed_source_limit"));
        assert_eq!(
            other["error"]["reason"],
            "input_or_output_contract_unavailable"
        );
    }

    #[test]
    fn cad_delete_failure_preserves_fixed_reason_without_private_details() {
        for reason in [
            "cad_delete_selection_differs",
            "cad_delete_inventory_owner_differs",
            "cad_delete_independent_detail_process_required",
            "cad_delete_save_incomplete",
            "cad_delete_source_not_in_project",
        ] {
            let error = anyhow::anyhow!(format!("{reason}:/private/customer part.prt"));
            let value = super::command_failure("cad", &error);
            assert_eq!(value["error"]["reason"], reason);
            assert_eq!(value["retryable"], false);
            assert!(!value.to_string().contains("/private/"));
        }
        for reason in ["cad_delete_selection_differs_other", "cad_delete_unlisted"] {
            let value = super::command_failure(
                "cad",
                &anyhow::anyhow!(format!("{reason}:/private/part.prt")),
            );
            assert_eq!(
                value["error"]["reason"],
                "input_or_output_contract_unavailable"
            );
            assert!(!value.to_string().contains("/private/"));
        }
    }
    #[test]
    fn cad_copy_failure_preserves_fixed_reason_without_details() {
        for reason in [
            "cad_copy_source_not_in_project",
            "cad_copy_native_result_not_in_project",
            "cad_copy_project_identity_mismatch",
            "cad_copy_independent_process_required",
            "cad_copy_save_incomplete",
        ] {
            let error = anyhow::anyhow!(format!("{reason}:/private/customer model.prt"))
                .context("private caller context");
            let value = super::command_failure("cad", &error);
            assert_eq!(value["error"]["reason"], reason);
            assert_eq!(value["engineering_adoption"], "blocked");
            assert_eq!(value["retryable"], false);
            assert!(!value.to_string().contains("/private/"));
            assert!(!value.to_string().contains("private caller context"));
        }
        for unlisted in [
            "cad_copy_source_not_in_project_other:/private/model.prt",
            "cad_copy_project_identity_mismatch/private/model.prt",
            "cad_copy_save_incomplete\n/private/model.prt",
            "cad_copy_unlisted:/private/model.prt",
        ] {
            let value = super::command_failure("cad", &anyhow::anyhow!(unlisted));
            assert_eq!(
                value["error"]["reason"],
                "input_or_output_contract_unavailable"
            );
            assert!(!value.to_string().contains("/private/"));
        }
    }
    #[test]
    fn cad_occurrence_failure_preserves_fixed_reason_without_details() {
        for reason in [
            "cad_instance_correspondence_project_identity_mismatch",
            "cad_instance_correspondence_current_project_missing_change_source",
            "cad_occurrence_assembly_object_source_differs",
            "cad_occurrence_native_result_not_in_project",
            "cad_focus_current_context_mismatch",
            "cad_construction_source_not_in_project",
            "cad_construction_native_result_not_in_project",
            "cad_construction_object_revision_or_conditions_differ",
        ] {
            let error = anyhow::anyhow!(format!("{reason}:/private/customer source"))
                .context("private outer context");
            let value = super::command_failure("cad", &error);
            assert_eq!(value["error"]["reason"], reason);
            assert_eq!(value["error"]["code"], "invalid_contract");
            assert_eq!(value["engineering_adoption"], "blocked");
            assert_eq!(value["retryable"], false);
            assert!(!value.to_string().contains("/private/"));
            assert!(!value.to_string().contains("private outer context"));
        }
        // Prefix resemblance is insufficient; only the exact fixed reason is public.
        let error = anyhow::anyhow!(
            "cad_occurrence_native_result_not_in_project_other:/private/customer source"
        );
        let value = super::command_failure("cad", &error);
        assert_eq!(
            value["error"]["reason"],
            "input_or_output_contract_unavailable"
        );
        assert!(!value.to_string().contains("/private/"));
        let error = anyhow::anyhow!(
            "cad_construction_source_not_in_project_other:/private/customer source"
        );
        let value = super::command_failure("cad", &error);
        assert_eq!(
            value["error"]["reason"],
            "input_or_output_contract_unavailable"
        );
        assert!(!value.to_string().contains("/private/"));
    }
    #[test]
    fn cad_failure_preserves_safe_reason_without_source_path() {
        let known = anyhow::anyhow!("hole_native_count_mismatch:private user path");
        let result = super::command_failure("cad", &known);
        assert_eq!(result["error"]["reason"], "hole_native_count_mismatch");
        assert_eq!(result["engineering_adoption"], "blocked");
        assert!(!result.to_string().contains("private user path"));
        let private = anyhow::anyhow!("cannot read private customer path");
        let result = super::command_failure("cad", &private);
        assert_eq!(
            result["error"]["reason"],
            "input_or_output_contract_unavailable"
        );
        assert!(!result.to_string().contains("private customer path"));
    }

    #[test]
    fn workspace_failure_reports_only_fixed_public_codes() {
        let error = anyhow::anyhow!("workspace_learning_root_changed:/private/project/goal.json");
        let result = super::command_failure("project", &error);
        assert_eq!(result["error"]["reason"], "workspace_learning_root_changed");
        assert!(!result.to_string().contains("/private/"));
        let unknown = anyhow::anyhow!("workspace_unlisted:/private/project/goal.json");
        let result = super::command_failure("project", &unknown);
        assert_eq!(
            result["error"]["reason"],
            "input_or_output_contract_unavailable"
        );
        assert!(!result.to_string().contains("/private/"));
    }
}
