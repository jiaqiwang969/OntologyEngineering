//! One native entry point. Unmigrated commands are not delegated to legacy code.
mod book_release;
mod books;
mod browser;
mod cad;
mod design_attempt;
mod design_comparison;
mod distribution;
mod exploration;
mod incad;
mod installation;
mod manufacturing;
mod memory;
mod model_assistance;
mod presentation;
mod providers;
mod runtime;
mod schemas;
mod services;
mod supplier;
mod tool_result;
mod v2;
mod versions;
mod workspace;
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Parser, Subcommand};
use oe_core::json::strict_json;
use oe_core::{SkillRoot, write_new};
use oe_engineering::harness;
use serde_json::{Value, json};
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "oev2",
    version,
    about = "Engineering ontology v2 Rust CLI (refactoring in progress)"
)]
struct Cli {
    /// Installed skill root; originals/indexes stay in sources/, private work in var/.
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    /// Explicit project artifact directory; installed source identity stays under --root.
    #[arg(long, global = true)]
    project_home: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize a new installation identity under this root.
    Setup,
    /// Unified engineering experience storage and project-bound feedback.
    #[command(disable_help_flag = true)]
    Memory {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<std::ffi::OsString>,
    },
    /// Source-bound finite-state engineering HTML/SVG presentation.
    Presentation {
        #[command(subcommand)]
        command: presentation::PresentationCommand,
    },
    /// Current Rust runtime owner and read-only historical source qualification.
    Runtime {
        #[command(subcommand)]
        command: runtime::RuntimeCommand,
    },
    /// Complete pinned neutral model schema, defaults, aliases and validation.
    Schemas {
        #[command(subcommand)]
        command: schemas::SchemaCommand,
    },
    /// Native source-bound semantic service and explicit MCP stdio profiles.
    Services {
        #[command(subcommand)]
        command: services::ServiceCommand,
    },
    /// Governed private method/rule selection, actual use and explicit restoration.
    Versions {
        #[command(subcommand)]
        command: versions::VersionCommand,
    },
    /// Source-bound native OpenAI/Groq protocol with explicit upper credentials.
    Providers {
        #[command(subcommand)]
        command: providers::ProviderCommand,
    },
    /// Annotated design, actual ggen generation, consistency and impact checks.
    Design {
        #[command(subcommand)]
        command: v2::DesignCommand,
    },
    /// Formal Rust RDF/SPARQL/SHACL/rules and new-identity semantic review.
    Kernel {
        #[command(subcommand)]
        command: v2::KernelCommand,
    },
    /// Source-bound project records and change impact.
    Project {
        #[command(subcommand)]
        command: v2::ProjectCommand,
    },
    /// Whole-project inventory, findings and receiving installation.
    Maintain {
        #[command(subcommand)]
        command: v2::MaintainCommand,
    },
    /// Transport an explicit source inventory and verify exact received bytes.
    Dist {
        #[command(subcommand)]
        command: distribution::DistributionCommand,
    },
    /// Source-bound manufacturing report delivery and verification.
    Manufacturing {
        #[command(subcommand)]
        command: manufacturing::ManufacturingCommand,
    },
    /// Native CAD task preparation, transport and source-bound results.
    Cad {
        #[command(subcommand)]
        command: Box<cad::CadCommand>,
    },
    /// Purpose-driven supplier resources, exact account and part evidence, and file handoff.
    Supplier {
        #[command(subcommand)]
        command: Box<supplier::SupplierCommand>,
    },
    /// Observe the native browser with explicit source-bound receipts.
    Browser {
        #[command(subcommand)]
        command: Box<browser::BrowserCommand>,
    },
    /// Scan candidate files and live SQLite text using bounded native Rust checks.
    Privacy {
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "git")]
        git: PathBuf,
        #[arg(long, conflicts_with = "include_ignored")]
        tracked_only: bool,
        #[arg(long)]
        include_ignored: bool,
    },
    /// Native book assets, exact source checks and explicit authoring maintenance.
    Books {
        #[command(subcommand)]
        command: Box<books::BookCommand>,
    },
    /// Source-bound engineering knowledge; retrieval never promotes ontology facts.
    Knowledge {
        #[command(subcommand)]
        command: Box<KnowledgeCommand>,
    },
    /// Advance one project step or consolidate stage evidence without promotion.
    Harness {
        #[command(subcommand)]
        command: HarnessCommand,
    },
    /// Verify source-locked data or request an explicitly selected native review.
    Semantics {
        #[command(subcommand)]
        command: Box<SemanticCommand>,
    },
}

#[derive(Args, Clone)]
struct KnowledgeTools {
    /// Explicit native Poppler executable; omit to resolve pdfinfo through PATH.
    #[arg(long)]
    pdfinfo: Option<PathBuf>,
    #[arg(long)]
    pdftotext: Option<PathBuf>,
    #[arg(long)]
    pdftoppm: Option<PathBuf>,
}
impl KnowledgeTools {
    fn resolve(&self) -> Result<oe_knowledge::poppler::PopplerTools> {
        let mut tools = oe_knowledge::poppler::PopplerTools::default();
        for (selected, target) in [
            (&self.pdfinfo, &mut tools.pdfinfo),
            (&self.pdftotext, &mut tools.pdftotext),
            (&self.pdftoppm, &mut tools.pdftoppm),
        ] {
            if let Some(path) = selected {
                ensure!(path.is_absolute(), "knowledge_tool_path_must_be_absolute");
                *target = path.clone();
            }
        }
        Ok(tools)
    }
}

#[derive(Args, Clone)]
struct KnowledgeContext {
    #[arg(long, conflicts_with = "project_context")]
    context: Option<PathBuf>,
    /// Explicit mappings from current project source records.
    #[arg(long)]
    project_context: Option<PathBuf>,
}

#[derive(Args)]
struct KnowledgeProviders {
    #[arg(long,default_value="all",value_parser=["all","local-pdf","misumi"])]
    provider: String,
    #[arg(long)]
    local_index: Option<PathBuf>,
    #[arg(long)]
    misumi_index: Option<PathBuf>,
}
impl KnowledgeProviders {
    fn specs(&self) -> Result<Vec<oe_knowledge::query::ProviderSpec>> {
        ensure!(
            self.provider != "misumi" || self.local_index.is_none(),
            "unused_local_index"
        );
        ensure!(
            self.provider != "local-pdf" || self.misumi_index.is_none(),
            "unused_misumi_index"
        );
        let mut result = Vec::new();
        if self.provider != "misumi" {
            result.push(oe_knowledge::query::ProviderSpec {
                kind: "local_pdf".into(),
                index: self.local_index.clone(),
            });
        }
        if self.provider != "local-pdf" {
            result.push(oe_knowledge::query::ProviderSpec {
                kind: "misumi".into(),
                index: self.misumi_index.clone(),
            });
        }
        Ok(result)
    }
}

#[derive(Subcommand)]
enum KnowledgeCommand {
    /// Freeze, evaluate, trial and recover a source-bound experience candidate.
    Experience {
        #[command(subcommand)]
        command: v2::ExperienceCommand,
    },
    /// Resolve and hash-check the methodological sources used by current rules.
    Continuity {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Read the complete bounded visible causal archive with exact raw locators.
    Intake {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long, default_value_t = 536870912)]
        max_bytes: u64,
        #[arg(long, requires = "end_byte")]
        start_byte: Option<u64>,
        #[arg(long, requires = "start_byte")]
        end_byte: Option<u64>,
        #[arg(long)]
        output: PathBuf,
    },
    /// Acquire and hash-check private historical sources; original root stays read-only.
    Continue {
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Supplement verified historical method guides in an existing source root.
        #[arg(long)]
        methods: bool,
    },
    /// Rebuild supplier text/navigation indexes from verified original sources.
    BuildIndex {
        #[arg(
            long,
            conflicts_with = "source_pack",
            required_unless_present = "source_pack"
        )]
        archive: Option<PathBuf>,
        #[arg(long, conflicts_with = "archive", required_unless_present = "archive")]
        source_pack: Option<PathBuf>,
        #[arg(long, default_value = "sources/.indexes/misumi")]
        index: PathBuf,
        #[arg(long)]
        replace: bool,
        #[command(flatten)]
        tools: KnowledgeTools,
    },
    /// Install, register and verify local supplier originals in this skill.
    Library {
        #[command(subcommand)]
        command: KnowledgeLibraryCommand,
    },
    /// Check registered indexes without downloading, extracting or using a model.
    Status {
        #[command(flatten)]
        providers: KnowledgeProviders,
    },
    /// Explicitly register rooted original PDFs with bounded native Poppler extraction.
    Register {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long, default_value = "sources/.indexes/local-pdf")]
        index: PathBuf,
        /// Replace the complete catalog; omitted source IDs leave this index.
        #[arg(long, conflicts_with = "merge")]
        replace: bool,
        /// Add/update supplied IDs while preserving all other registered sources.
        #[arg(long, conflicts_with = "replace")]
        merge: bool,
        /// Verify source identities and preview changes without extracting or publishing.
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        tools: KnowledgeTools,
    },
    /// Understand this problem, navigate sources, then return bounded page candidates.
    Query {
        #[arg(long)]
        query: String,
        #[command(flatten)]
        context: KnowledgeContext,
        /// Rooted JSON file containing an array of explicit {path,sha256} history references.
        #[arg(long)]
        history: Vec<PathBuf>,
        #[command(flatten)]
        providers: KnowledgeProviders,
        #[arg(long, default_value_t = 5)]
        top: usize,
        #[arg(long, default_value_t = 24)]
        candidates: usize,
        /// Experimental pool priority; applicability and mandatory review unchanged.
        #[arg(long, default_value="context_first", value_parser=["context_first","query_first"])]
        retrieval_order: String,
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long)]
        full_text: bool,
        #[command(flatten)]
        tools: KnowledgeTools,
    },
    /// Render one exact indexed original page, independent of search ranking.
    View {
        #[arg(long,value_parser=["local-pdf","misumi"])]
        provider: String,
        #[arg(long)]
        page_id: String,
        #[arg(long)]
        index: Option<PathBuf>,
        #[arg(long,default_value_t=144,value_parser=clap::value_parser!(u32).range(72..=300))]
        dpi: u32,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[command(flatten)]
        tools: KnowledgeTools,
    },
    VerifyPacket {
        #[arg(long)]
        packet: PathBuf,
        #[command(flatten)]
        context: KnowledgeContext,
        #[arg(long)]
        output_dir: Option<PathBuf>,
        #[command(flatten)]
        tools: KnowledgeTools,
    },
    Feedback {
        #[arg(long)]
        packet: PathBuf,
        #[command(flatten)]
        context: KnowledgeContext,
        #[arg(long)]
        candidate: String,
        #[arg(long)]
        unit_id: Vec<String>,
        #[arg(long,value_parser=["adopted","deferred","rejected"])]
        disposition: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        next_check: String,
        #[arg(long)]
        result_evidence: Vec<String>,
        #[arg(long)]
        previous: Option<PathBuf>,
        /// New JSON feedback file in the same original project's var/projects directory.
        #[arg(long)]
        output: PathBuf,
        #[command(flatten)]
        tools: KnowledgeTools,
    },
    ProjectContext {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum KnowledgeLibraryCommand {
    /// Manage the complete source-delivery manifest (books and supplier originals).
    SourceDelivery {
        #[command(subcommand)]
        command: SourceDeliveryCommand,
    },
    /// Preserve compressed pages; install declared metadata and index only.
    InstallPack {
        #[arg(long)]
        downloads: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
    /// Link an existing verified original archive and index without copying PDFs.
    Register {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        index: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
    /// Copy and verify an expanded archive and index into a new local library.
    Import {
        #[arg(long)]
        archive: PathBuf,
        #[arg(long)]
        index: PathBuf,
        #[arg(long)]
        target: PathBuf,
    },
    Verify {
        #[arg(long)]
        target: PathBuf,
    },
    /// Explicitly restore full PDFs from the verified compressed book archive.
    RestoreBooks {
        #[arg(long)]
        target: PathBuf,
    },
}

#[derive(Subcommand)]
enum SourceDeliveryCommand {
    List {
        #[arg(long, default_value = "runtime/source-delivery.json")]
        manifest: PathBuf,
        #[arg(long)]
        select: Option<String>,
    },
    Verify {
        #[arg(long, default_value = "runtime/source-delivery.json")]
        manifest: PathBuf,
        #[arg(long)]
        select: Option<String>,
    },
    Import {
        #[arg(long, default_value = "runtime/source-delivery.json")]
        manifest: PathBuf,
        #[arg(long)]
        from: PathBuf,
        #[arg(long)]
        select: Option<String>,
    },
    Fetch {
        #[arg(long, default_value = "runtime/source-delivery.json")]
        manifest: PathBuf,
        #[arg(long)]
        select: String,
        /// Explicit gws executable; otherwise resolve it through PATH.
        #[arg(long, default_value = "gws")]
        gws: PathBuf,
        #[arg(long, default_value_t = 1800, value_parser = clap::value_parser!(u64).range(1..=1800))]
        timeout_seconds: u64,
    },
    Register {
        #[arg(long, default_value = "runtime/source-delivery.json")]
        manifest: PathBuf,
        #[arg(long)]
        select: Option<String>,
        /// Replace the complete catalog with the selected books.
        #[arg(long, visible_alias = "replace-index", conflicts_with = "merge")]
        replace: bool,
        /// Merge selected books, retaining other registered source IDs.
        #[arg(long, conflicts_with = "replace")]
        merge: bool,
        /// Preview source changes without extraction or index publication.
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        tools: KnowledgeTools,
    },
}

#[derive(Args)]
struct InputArgs {
    #[arg(long)]
    input: PathBuf,
    /// New directory under this skill's var/. Omit to print prepared data only.
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Subcommand)]
enum SemanticCommand {
    /// Source-bound current typed Project plus actual promoted semantic review.
    TypedProjectReview {
        #[arg(long)]
        input: PathBuf,
    },
    /// Review exact task-bound RDF using a promoted package; full semantic response.
    ProjectReview {
        #[command(flatten)]
        args: SemanticTaskArgs,
        #[arg(long)]
        evidence_file: PathBuf,
        #[arg(long)]
        source_id: String,
        #[arg(long, visible_alias = "format")]
        evidence_format: String,
        #[arg(long, visible_alias = "scope")]
        scope_id: String,
        #[arg(long, visible_alias = "focus")]
        focus_iri: String,
        #[arg(long, visible_alias = "focus-type")]
        focus_type_iri: String,
        #[arg(long, visible_alias = "query-asset")]
        query_asset_id: String,
        #[arg(long, visible_alias = "shape-asset")]
        shape_asset_id: String,
        #[arg(long)]
        recorded_at: Option<String>,
    },
    /// Inspect native packages and optionally an exact project baseline.
    Discover {
        #[arg(long)]
        binding: Option<PathBuf>,
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Execute a bound built-in or current promoted package with the native runner.
    Run(SemanticExecutionArgs),
    /// Record one current task's native engagement boundary.
    Open {
        #[command(flatten)]
        args: SemanticTaskArgs,
        #[arg(long)]
        engagement: Option<PathBuf>,
    },
    /// Retain a nonauthoritative candidate; does not commit or promote it.
    Propose {
        #[command(flatten)]
        args: SemanticTaskArgs,
        #[arg(long)]
        delta: PathBuf,
        #[arg(long)]
        engagement: PathBuf,
        #[arg(long)]
        recorded_at: Option<String>,
    },
    /// Commit only the exact locally authorized candidate.
    Commit(SemanticMutationArgs),
    /// Verify a built-in package, or a specific committed workspace candidate.
    Verify {
        #[command(flatten)]
        args: SemanticExecutionArgs,
        #[arg(long)]
        candidate: Option<String>,
    },
    /// Return verified immutable events; history errors never mean absence.
    History {
        #[arg(long)]
        binding: PathBuf,
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        candidate: String,
    },
    /// Promote with exact authority and return an unapplied successor binding.
    Promote(SemanticMutationArgs),
    /// Initialize frozen native methods, then separately adopt them to a project.
    Bootstrap {
        #[command(subcommand)]
        command: BootstrapCommand,
    },
    /// Fresh scoped method-record checks through source-locked Semantica.
    Review {
        /// JSON object with project_id and review_requests; saved results are not input.
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Diagnose the locked runtime and native registries; no project execution.
    Doctor {
        #[arg(long, conflicts_with = "identity_only")]
        binding: Option<PathBuf>,
        /// Check only installed runtime identity, without native registry diagnostics.
        #[arg(long)]
        identity_only: bool,
        /// External Semantica interpreter; defaults to this skill's runtime/.venv.
        #[arg(long, requires = "identity_only")]
        python: Option<PathBuf>,
        /// Select a candidate identity under var/ without changing the formal lock.
        #[arg(long, requires = "identity_only")]
        staging_descriptor: Option<PathBuf>,
    },
    Bundle {
        #[arg(long)]
        name: String,
    },
    /// Input is the selected semantic_modules array, with explicit applicability.
    Premises(InputArgs),
    /// Audit Semantica's 29-chapter package registry; delegate all contract validation to native.
    ValidateChapterContracts {
        /// Exit 1 if any package is not release_ready (implies structurally valid).
        #[arg(long)]
        release: bool,
    },
    /// Rebind both maintained book sources to Semantica's 29 chapter packages.
    /// Release-engineering tool: updates source/TeX digests in a Semantica checkout.
    RebindBooks {
        /// OE repository root (book source files).
        #[arg(long)]
        book_root: PathBuf,
        /// Semantica source checkout root.
        #[arg(long)]
        semantica_root: PathBuf,
        /// Write changes; omit for dry run.
        #[arg(long)]
        write: bool,
    },
}

#[derive(Args)]
struct SemanticTaskArgs {
    #[arg(long)]
    binding: PathBuf,
    #[arg(long)]
    task: PathBuf,
    #[arg(long)]
    workspace: PathBuf,
}

#[derive(Args)]
struct SemanticMutationArgs {
    #[command(flatten)]
    args: SemanticTaskArgs,
    #[arg(long)]
    candidate: String,
    #[arg(long)]
    authorization: PathBuf,
    #[arg(long)]
    recorded_at: Option<String>,
}

#[derive(Args)]
struct SemanticExecutionArgs {
    #[arg(long)]
    binding: PathBuf,
    #[arg(long)]
    task: Option<PathBuf>,
    #[arg(long)]
    workspace: Option<PathBuf>,
    #[arg(long)]
    scenario: Option<String>,
    #[arg(long)]
    recorded_at: Option<String>,
}

impl SemanticExecutionArgs {
    fn request(&self) -> oe_semantic::lifecycle::ExecutionRequest<'_> {
        oe_semantic::lifecycle::ExecutionRequest {
            binding: &self.binding,
            task: self.task.as_deref(),
            workspace: self.workspace.as_deref(),
            scenario_id: self.scenario.as_deref(),
            recorded_at: self.recorded_at.as_deref(),
        }
    }
}

#[derive(Subcommand)]
enum BootstrapCommand {
    Prepare {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        project: String,
        #[arg(long)]
        domain: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        fact_authority: String,
        #[arg(long)]
        evidence_root: String,
        #[arg(long, default_value = "engineering-judgment-intake")]
        bundle: String,
    },
    Apply {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        authorization: PathBuf,
    },
    Adopt {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        authorization: PathBuf,
    },
}

fn semantic_delivery(command: &str, result: Result<Value>) -> Result<u8> {
    let response =
        result.unwrap_or_else(|error| oe_semantic::lifecycle::error_response(command, &error));
    // Established semantic protocol: delivery and semantic acceptance are
    // independent. One JSON value is emitted even for a blocked lifecycle.
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(0)
}

fn semantic_argument_error(message: &'static str) -> anyhow::Error {
    oe_semantic::contracts::ContractError {
        kind: oe_semantic::contracts::ContractErrorKind::BindingValidationError,
        message: message.into(),
    }
    .into()
}

fn verify_execution(
    root: &Path,
    args: &SemanticExecutionArgs,
    candidate: Option<&str>,
) -> Result<Value> {
    if let Some(candidate) = candidate {
        if args.scenario.is_some() {
            return Err(semantic_argument_error(
                "candidate verification does not accept a scenario selector",
            ));
        }
        let task = args
            .task
            .as_deref()
            .ok_or_else(|| semantic_argument_error("candidate verification requires a task"))?;
        let workspace = args.workspace.as_deref().ok_or_else(|| {
            semantic_argument_error("candidate verification requires a workspace")
        })?;
        oe_semantic::lifecycle::transition_at(
            root,
            "verify",
            &args.binding,
            task,
            workspace,
            candidate,
            None,
            args.recorded_at.as_deref(),
        )
    } else {
        if args.workspace.is_some() {
            return Err(semantic_argument_error(
                "workspace candidate verification requires a candidate identity",
            ));
        }
        oe_semantic::lifecycle::verify_package(root, &args.request())
    }
}

#[derive(Args)]
struct HarnessArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    /// Explicit source-bound premise selections (selections array JSON).
    /// Merged into harness.semantic_modules before step execution.
    #[arg(long)]
    premises: Option<PathBuf>,
}

#[derive(Subcommand)]
enum HarnessCommand {
    Step {
        #[command(flatten)]
        args: HarnessArgs,
    },
    Consolidate(HarnessArgs),
}

fn harness_summary_value(result: &Value) -> Value {
    let mut summary = serde_json::Map::new();
    for key in [
        "status",
        "task_id",
        "candidate_routes",
        "uncertain_routes",
        "checkpoint_id",
        "unanswered",
        "formal_candidate",
        "ontology_promotion",
        "handoff",
        "engineering_acceptance",
        "context_transport",
        "checkpoint_policy",
        "model_execution",
        "model_calls",
        "transport_initialization",
        "execution_binding",
    ] {
        if let Some(value) = result.get(key) {
            summary.insert(key.to_owned(), value.clone());
        }
    }
    if let Some(feedback) = result.get("feedback") {
        summary.insert(
            "semantic_feedback_status".to_owned(),
            feedback["status"].clone(),
        );
        let reviews = feedback["reviews"].as_array().map(|reviews| reviews.iter().map(|item| {
            json!({"id": item["id"], "status": item["review"].get("status").cloned().unwrap_or(json!("unknown")),
                "findings": item["review"]["findings"].as_array().map(Vec::len),
                "violations": item["review"]["violations"].as_array().map(Vec::len)})
        }).collect::<Vec<_>>()).unwrap_or_default();
        summary.insert("semantic_reviews".to_owned(), json!(reviews));
    }
    Value::Object(summary)
}

fn harness_summary(result: &Value) -> Result<u8> {
    println!("{}", harness_summary_value(result));
    Ok(
        if matches!(
            result["status"].as_str(),
            Some("unavailable" | "partial" | "incomplete" | "blocked")
        ) {
            1
        } else {
            0
        },
    )
}

fn locate_root(explicit: Option<&Path>) -> Result<SkillRoot> {
    if let Some(path) = explicit {
        let root = SkillRoot::new(path)?;
        ensure!(
            !declared_v1_root(root.path())?,
            "legacy_skill_root_read_only"
        );
        return Ok(root);
    }
    // macOS can report the invoked symlink as current_exe. Resolve it so a
    // PATH entry outside the skill can still find the installed binary's root.
    let executable = std::fs::canonicalize(std::env::current_exe()?)?;
    // Release programs live outside the source root so their bytes and the
    // transport manifest do not alter the compiled public-source identity.
    if let Some(bin) = executable.parent()
        && bin.file_name().is_some_and(|name| name == "bin")
        && let Some(bundle) = bin.parent()
        && (bundle.join("RELEASE-MANIFEST.json").exists() || bundle.join("skill/SKILL.md").exists())
    {
        let skill = bundle.join("skill");
        ensure!(
            bundle.join("RELEASE-MANIFEST.json").is_file()
                && skill.join("SKILL.md").is_file()
                && skill.join("runtime/semantic-bundles.json").is_file()
                && !declared_v1_root(&skill)?,
            "release_skill_root_incomplete"
        );
        return SkillRoot::new(&skill);
    }
    for base in [std::env::current_dir()?, executable] {
        for path in base.ancestors() {
            if path.join("SKILL.md").is_file()
                && path.join("runtime/semantic-bundles.json").is_file()
                && !declared_v1_root(path)?
            {
                return SkillRoot::new(path);
            }
        }
    }
    bail!("skill_root_required")
}
fn declared_v1_root(path: &Path) -> Result<bool> {
    let bytes = oe_core::read_regular_bytes(&path.join("SKILL.md"), 1024 * 1024)?;
    Ok(oe_dist::package::declared_skill_name(&bytes)? == Some("ontology-engineering"))
}

fn read_input(path: &Path) -> Result<Value> {
    const LIMIT: u64 = 8 * 1024 * 1024;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Opening a FIFO must not block before we can reject its file type.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).context("input_unavailable")?;
    ensure!(file.metadata()?.is_file(), "input_not_regular_file");
    let mut raw = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut raw)?;
    ensure!(raw.len() <= LIMIT as usize, "input_exceeds_limit");
    Ok(strict_json(&raw)?)
}

fn deliver(root: &SkillRoot, output: Option<&Path>, filename: &str, result: &Value) -> Result<()> {
    if let Some(path) = output {
        let directory = root.new_private_dir(path)?;
        write_new(&directory.join(filename), result)?;
        println!(
            "{}",
            json!({"status": "prepared", "artifact": directory.join(filename), "semantic_execution": "not_run"})
        );
    } else {
        println!("{}", serde_json::to_string_pretty(result)?);
    }
    Ok(())
}

fn knowledge_relative(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    let path = root.expand(path)?;
    ensure!(
        path.starts_with(root.path()) && path != root.path(),
        "knowledge_input_outside_root"
    );
    oe_core::no_symlinks(&path)?;
    Ok(path.strip_prefix(root.path())?.to_path_buf())
}

fn knowledge_json(root: &SkillRoot, path: &Path) -> Result<Value> {
    let relative = knowledge_relative(root, path)?;
    let raw = oe_core::read_regular_bytes(&root.path().join(relative), 8 * 1024 * 1024)?;
    Ok(oe_core::json::strict_json(&raw)?)
}

fn knowledge_context(
    root: &SkillRoot,
    args: &KnowledgeContext,
    required: bool,
) -> Result<Option<Value>> {
    let result = if let Some(path) = &args.project_context {
        Some(oe_knowledge::context::project(
            root,
            &knowledge_json(root, path)?,
        )?)
    } else if let Some(path) = &args.context {
        Some(oe_knowledge::context::validate(&knowledge_json(
            root, path,
        )?)?)
    } else {
        None
    };
    ensure!(
        !required || result.is_some(),
        "knowledge_current_context_required"
    );
    Ok(result)
}

fn knowledge_write(
    root: &SkillRoot,
    output: Option<&Path>,
    name: &str,
    result: &Value,
) -> Result<Option<PathBuf>> {
    if let Some(path) = output {
        let dir = root.new_private_dir(path)?;
        let path = dir.join(name);
        write_new(&path, result)?;
        Ok(Some(path.strip_prefix(root.path())?.to_path_buf()))
    } else {
        Ok(None)
    }
}

fn knowledge_error(command: &str, code: &str) -> Value {
    json!({"schema":"ontology-engineering.knowledge-cli-response/v1","command":command,"status":"blocked","result":null,"errors":[{"code":code}],"model_execution":"not_confirmed","semantic_review":"not_run","ontology_promotion":"not_run"})
}

fn knowledge_result(command: &str, status: &str, result: Value) -> Value {
    json!({"schema":"ontology-engineering.knowledge-cli-response/v1","command":command,"status":status,"result":result,"errors":[],"semantic_review":"not_run","ontology_promotion":"not_run"})
}

fn knowledge_execute(root: &SkillRoot, command: KnowledgeCommand) -> Result<Value> {
    use oe_knowledge::{context, feedback, packet, query, sources::Provider};
    match command {
        KnowledgeCommand::Experience { command } => {
            return v2::experience(root, command);
        }
        KnowledgeCommand::Continuity { output } => {
            let record = oe_knowledge::continuity::mapped_sources(root)?;
            if let Some(path) = output {
                write_new(&root.private_path(&path)?, &record)?;
            }
            Ok(knowledge_result(
                "continuity",
                record["status"].as_str().unwrap_or("incomplete"),
                record.clone(),
            ))
        }
        KnowledgeCommand::Intake {
            archive,
            max_bytes,
            start_byte,
            end_byte,
            output,
        } => {
            oe_design::provenance::require(root)?;
            let record = match (start_byte, end_byte) {
                (Some(start), Some(end)) => {
                    oe_knowledge::conversation::read_window(&archive, start, end)?
                }
                (None, None) => oe_knowledge::conversation::read_archive(&archive, max_bytes)?,
                _ => anyhow::bail!("archive_window_requires_both_boundaries"),
            };
            write_new(&root.private_path(&output)?, &record)?;
            Ok(knowledge_result(
                "intake",
                record["status"].as_str().unwrap_or("partial"),
                json!({"entries":record["entries"].as_array().map(Vec::len),"issues":record["issues"],"causal_interpretation":"not_established","historical_execution":"not_relabelled"}),
            ))
        }
        KnowledgeCommand::Continue {
            from,
            output,
            methods,
        } => {
            oe_design::provenance::require(root)?;
            let record = if methods {
                oe_knowledge::continuity::receive_methods(root, &from)?
            } else {
                oe_knowledge::continuity::receive(root, &from)?
            };
            write_new(&root.private_path(&output)?, &record)?;
            Ok(knowledge_result(
                "continue",
                "received",
                json!({"source_files":record["source_files"].as_array().map(Vec::len),"originals_modified":false,"source_root":"sources","semantic_execution":"not_run"}),
            ))
        }
        KnowledgeCommand::BuildIndex {
            archive,
            source_pack,
            index,
            replace,
            tools,
        } => {
            use oe_knowledge::supplier_index::{BuildInput, BuildOptions, build};
            let input = match (archive, source_pack) {
                (Some(path), None) => BuildInput::Archive(path),
                (None, Some(path)) => BuildInput::SourcePack(path),
                _ => anyhow::bail!("exactly_one_supplier_source_required"),
            };
            let result = build(
                root.path(),
                &input,
                &BuildOptions {
                    data_root: index,
                    replace,
                    tools: tools.resolve()?,
                    limits: Default::default(),
                },
            )?;
            Ok(knowledge_result("build-index", "indexed", result))
        }
        KnowledgeCommand::Library { command } => {
            use oe_knowledge::supplier_library as library;
            let (operation, result) = match command {
                KnowledgeLibraryCommand::InstallPack { downloads, target } => (
                    "install-pack",
                    library::install_pack(root, &downloads, &target)?,
                ),
                KnowledgeLibraryCommand::SourceDelivery { command } => {
                    use oe_knowledge::source_library as delivery;
                    let (operation, result) = match command {
                        SourceDeliveryCommand::List { manifest, select } => (
                            "source-delivery-list",
                            delivery::list(root, &manifest, select.as_deref(), false)?,
                        ),
                        SourceDeliveryCommand::Verify { manifest, select } => (
                            "source-delivery-verify",
                            delivery::list(root, &manifest, select.as_deref(), true)?,
                        ),
                        SourceDeliveryCommand::Import {
                            manifest,
                            from,
                            select,
                        } => (
                            "source-delivery-import",
                            delivery::import(root, &manifest, select.as_deref(), &from)?,
                        ),
                        SourceDeliveryCommand::Fetch {
                            manifest,
                            select,
                            gws,
                            timeout_seconds,
                        } => (
                            "source-delivery-fetch",
                            delivery::fetch(
                                root,
                                &manifest,
                                &select,
                                &delivery::FetchOptions {
                                    executable: gws,
                                    timeout: Duration::from_secs(timeout_seconds),
                                },
                            )?,
                        ),
                        SourceDeliveryCommand::Register {
                            manifest,
                            select,
                            replace,
                            merge,
                            dry_run,
                            tools,
                        } => (
                            "source-delivery-register",
                            delivery::register_with_options(
                                root,
                                &manifest,
                                select.as_deref(),
                                &oe_knowledge::registration::RegistrationOptions {
                                    replace,
                                    merge,
                                    dry_run,
                                    tools: tools.resolve()?,
                                    ..Default::default()
                                },
                            )?,
                        ),
                    };
                    let status = result["status"].as_str().unwrap_or("complete").to_owned();
                    return Ok(knowledge_result(operation, &status, result));
                }
                KnowledgeLibraryCommand::Register {
                    archive,
                    index,
                    target,
                } => (
                    "register",
                    library::register(root, &archive, &index, &target)?,
                ),
                KnowledgeLibraryCommand::Import {
                    archive,
                    index,
                    target,
                } => (
                    "import",
                    library::import_data(root, &archive, &index, &target)?,
                ),
                KnowledgeLibraryCommand::Verify { target } => {
                    ("verify", library::verify(root, &target)?)
                }
                KnowledgeLibraryCommand::RestoreBooks { target } => {
                    ("restore-books", library::restore_books(root, &target)?)
                }
            };
            let status = result["status"].as_str().unwrap_or("complete").to_owned();
            Ok(knowledge_result(operation, &status, result))
        }
        KnowledgeCommand::Status { providers } => {
            let rows=providers.specs()?.into_iter().map(|spec|match Provider::open(root,&spec.kind,spec.index.as_deref()) {
                Ok(provider)=>provider.status(),
                Err(_)=>json!({"provider":spec.kind,"status":"not_ready","errors":[{"code":"source_index_missing_invalid_or_changed"}]})
            }).collect::<Vec<_>>();
            let status = if rows.iter().all(|r| r["status"] == "ready") {
                "ready"
            } else {
                "not_ready"
            };
            Ok(knowledge_result(
                "status",
                status,
                json!({"providers":rows,"network_requests":0,"ocr":"not_run","originals_modified":false}),
            ))
        }
        KnowledgeCommand::Register {
            manifest,
            index,
            replace,
            merge,
            dry_run,
            tools,
        } => {
            let manifest = root.path().join(knowledge_relative(root, &manifest)?);
            let result = oe_knowledge::registration::register(
                root.path(),
                &manifest,
                &oe_knowledge::registration::RegistrationOptions {
                    data_root: index,
                    replace,
                    merge,
                    dry_run,
                    tools: tools.resolve()?,
                    limits: Default::default(),
                },
            )?;
            let status = result["status"].as_str().unwrap_or("registered").to_owned();
            Ok(knowledge_result("register", &status, result))
        }
        KnowledgeCommand::Query {
            query: question,
            context: context_args,
            history,
            providers,
            top,
            candidates,
            retrieval_order,
            output_dir,
            full_text,
            tools,
        } => {
            let current = knowledge_context(root, &context_args, false)?;
            let mut refs = Vec::new();
            ensure!(history.len() <= 16, "knowledge_history_limit");
            for path in history {
                let file = knowledge_json(root, &path)?;
                let array = file
                    .as_array()
                    .context("knowledge_history_array_required")?;
                ensure!(refs.len() + array.len() <= 16, "knowledge_history_limit");
                refs.extend(array.iter().cloned());
            }
            let tools = tools.resolve()?;
            let options = query::QueryOptions {
                query: question,
                context: current,
                history: refs,
                providers: providers.specs()?,
                candidates,
                top,
                retrieval_order,
                output: root.private_path(&output_dir)?,
                full_text: true,
                tools,
            };
            let report = query::run(root, &options)?;
            let status = report["status"].as_str().unwrap_or("unknown").to_owned();
            let packet_result = (|| -> Result<Value> {
                let packet = packet::make_with_tools(
                    root,
                    &report,
                    &report["context_snapshot"],
                    &options.tools,
                )?;
                let path = options.output.join("packet.json");
                write_new(&path, &packet)?;
                Ok(
                    json!({"status":"created","path":path.strip_prefix(root.path())?,"sha256":oe_core::fingerprint_file(&path,64*1024*1024)?.sha256}),
                )
            })();
            let mut display = report;
            if !full_text {
                for page in display["results"].as_array_mut().into_iter().flatten() {
                    if let Some(object) = page.as_object_mut() {
                        object.remove("native_text");
                    }
                }
            }
            let mut response = knowledge_result("query", &status, display);
            match packet_result {
                Ok(packet) => response["packet"] = packet,
                Err(_) => {
                    response["packet"] = json!({"status":"not_created"});
                    response["errors"] = json!([{"code":"packet_source_or_handoff_not_verified","query_record_retained":true}]);
                    if status == "completed" {
                        response["status"] = json!("partial");
                    }
                }
            }
            response["display"] = json!({"full_native_text":full_text,"original_report_preserves_full_native_text":true});
            Ok(response)
        }
        KnowledgeCommand::View {
            provider,
            page_id,
            index,
            dpi,
            output_dir,
            tools,
        } => {
            let tools = tools.resolve()?;
            let provider = Provider::open(root, &provider, index.as_deref())?;
            let page = provider.page(&page_id)?;
            let source = provider.source_result(&page, "", &tools)?;
            let result = oe_knowledge::citations::view(root.path(), &source, dpi, &tools)?;
            let artifact = knowledge_write(root, output_dir.as_deref(), "view.json", &result)?;
            let mut response = knowledge_result("view", "rendered", result);
            response["artifact"] = json!(artifact);
            Ok(response)
        }
        KnowledgeCommand::VerifyPacket {
            packet: path,
            context: context_args,
            output_dir,
            tools,
        } => {
            let current = knowledge_context(root, &context_args, true)?.expect("required context");
            let result = packet::verify_with_tools(
                root,
                &knowledge_json(root, &path)?,
                &current,
                &tools.resolve()?,
            )?;
            let artifact =
                knowledge_write(root, output_dir.as_deref(), "verification.json", &result)?;
            let mut response = knowledge_result(
                "verify-packet",
                result["status"].as_str().unwrap_or("unknown"),
                result.clone(),
            );
            response["artifact"] = json!(artifact);
            Ok(response)
        }
        KnowledgeCommand::Feedback {
            packet,
            context: context_args,
            candidate,
            unit_id,
            disposition,
            reason,
            next_check,
            result_evidence,
            previous,
            output,
            tools,
        } => {
            let current = knowledge_context(root, &context_args, true)?.expect("required context");
            let mut request = json!({"packet_path":knowledge_relative(root,&packet)?,"context":current,"candidate_id":candidate,"disposition":disposition,"reason":reason,"next_check":next_check,"result_evidence":result_evidence,"output":knowledge_relative(root,&output)?});
            if !unit_id.is_empty() {
                request["unit_ids"] = json!(unit_id);
            }
            if let Some(path) = previous {
                request["previous"] = json!(knowledge_relative(root, &path)?);
            }
            let result = feedback::append_with_tools(root, &request, &tools.resolve()?)?;
            Ok(knowledge_result("feedback", "recorded", result))
        }
        KnowledgeCommand::ProjectContext { input, output_dir } => {
            let result = context::project(root, &knowledge_json(root, &input)?)?;
            let freshness = context::verify_sources(root, &result)?;
            let artifact = knowledge_write(root, output_dir.as_deref(), "context.json", &result)?;
            let status = if freshness["status"] == "stale" {
                "stale_context"
            } else {
                "projected"
            };
            let mut response = knowledge_result("project-context", status, result);
            response["freshness"] = freshness;
            response["artifact"] = json!(artifact);
            Ok(response)
        }
    }
}

/// Validate the future directory before creating any of it. Canonicalizing the
/// existing ancestor also catches filesystem aliases of the source root.
fn create_project_home(root: &SkillRoot, home: &Path) -> Result<PathBuf> {
    ensure!(home.is_absolute(), "project_home_must_be_absolute");
    oe_core::no_symlinks(home)?;
    let mut existing = home;
    while !existing.try_exists()? {
        existing = existing.parent().context("project_home_parent_missing")?;
    }
    ensure!(existing.is_dir(), "project_home_parent_not_directory");
    let target = std::fs::canonicalize(existing)?.join(home.strip_prefix(existing)?);
    ensure!(
        !root.path().starts_with(&target),
        "project_data_root_contains_software_root"
    );
    let private = root.path().join("var");
    ensure!(
        !target.starts_with(root.path()) || (target.starts_with(&private) && target != private),
        "project_data_root_inside_software_sources"
    );
    oe_core::ensure_directory(&target)?;
    Ok(target)
}

fn harness_input_path(root: &SkillRoot, path: &Path) -> Result<PathBuf> {
    // The historical Harness CLI accepted cwd-relative command input paths.
    // An explicit project home selects project-relative inputs without changing
    // that legacy invocation contract for callers that did not opt in.
    if root.data_path() == root.path() {
        Ok(path.to_path_buf())
    } else {
        let path = root.data_expand(path)?;
        ensure!(
            path.starts_with(root.data_path()),
            "harness_input_outside_project_home"
        );
        oe_core::no_symlinks(&path)?;
        Ok(path)
    }
}

fn execute(cli: Cli) -> Result<u8> {
    let mut root = locate_root(cli.root.as_deref())?;
    if let Some(home) = cli.project_home.as_deref() {
        let supported = matches!(
            &cli.command,
            Command::Project { .. }
                | Command::Harness { .. }
                | Command::Kernel { .. }
                | Command::Memory { .. }
        ) || matches!(&cli.command, Command::Knowledge { command } if matches!(command.as_ref(), KnowledgeCommand::Experience { .. }));
        ensure!(supported, "project_home_not_supported_for_command");
        if matches!(
            &cli.command,
            Command::Project {
                command: v2::ProjectCommand::Workspace {
                    command: workspace::WorkspaceCommand::Init { .. }
                }
            }
        ) {
            let target = create_project_home(&root, home)?;
            root = root.with_project_data_root(&target)?;
        } else {
            root = root.with_project_data_root(home)?;
        }
    }
    match cli.command {
        Command::Setup => return installation::execute(&root),
        Command::Memory { args } => return memory::execute(&root, args),
        Command::Presentation { command } => {
            let value = presentation::run(&root, command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(0);
        }
        Command::Runtime { command } => return runtime::execute(&root, command),
        Command::Schemas { command } => return schemas::execute(&root, command),
        Command::Services { command } => return services::execute(&root, command),
        Command::Providers { command } => {
            let (value, code) = providers::run(&root, command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(code);
        }
        Command::Design { command } => return v2::design(&root, command),
        Command::Kernel { command } => return v2::kernel(&root, command),
        Command::Project { command } => return v2::project(&root, command),
        Command::Versions { command } => return versions::execute(&root, command),
        Command::Maintain { command } => return v2::maintain(&root, command),
        Command::Dist { command } => {
            let value = distribution::run(&root, command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(0);
        }
        Command::Manufacturing { command } => {
            let (value, code) = manufacturing::run(&root, command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(code);
        }
        Command::Cad { command } => {
            let (value, code) = match cad::run(&root, *command) {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("{}", v2::command_failure("cad", &error));
                    return Ok(2);
                }
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(code);
        }
        Command::Supplier { command } => {
            let (value, code) = supplier::run(&root, *command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(code);
        }
        Command::Browser { command } => {
            let (value, code) = browser::run(&root, *command)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(code);
        }
        Command::Privacy {
            output,
            git,
            tracked_only,
            include_ignored,
        } => {
            let report = oe_core::privacy::scan(
                &root,
                &oe_core::privacy::ScanOptions {
                    output,
                    git,
                    tracked_only,
                    include_ignored,
                },
            )?;
            let code = if report["ok"] == true { 0 } else { 1 };
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(code);
        }
        Command::Books { command } => return books::execute(&root, *command),
        Command::Knowledge { command } => {
            let response = knowledge_execute(&root, *command)?;
            let status = response["status"].as_str().unwrap_or("unknown");
            let code = if matches!(
                status,
                "ready"
                    | "registered"
                    | "preview"
                    | "unchanged"
                    | "completed"
                    | "rendered"
                    | "recorded"
                    | "projected"
                    | "ready_for_agent_review"
                    | "indexed"
                    | "installed"
                    | "imported"
                    | "verified"
                    | "restored"
                    | "listed"
                    | "placed"
                    | "received"
                    | "complete"
                    | "window_complete"
            ) {
                0
            } else {
                1
            };
            println!("{}", serde_json::to_string_pretty(&response)?);
            return Ok(code);
        }
        Command::Harness { command } => {
            let execution_binding = v2::require_execution(&root, "formal_harness")?;
            let (args, attempts) = match command {
                HarnessCommand::Step { args } => (args, Some(())),
                HarnessCommand::Consolidate(args) => (args, None),
            };
            let mut input = read_input(&harness_input_path(&root, &args.input)?)?;
            // If --premises supplied, merge selections into harness.semantic_modules
            if let Some(premises_path) = &args.premises {
                let premises_bytes = oe_core::read_regular_bytes(
                    &harness_input_path(&root, premises_path)?,
                    4 * 1024 * 1024,
                )
                .context("cannot read premises file")?;
                let premises_doc: Value = strict_json(&premises_bytes)?;
                // A bare array or a full advice object must explicitly declare selections.
                let selections = premises_doc
                    .as_array()
                    .or_else(|| premises_doc.get("selections").and_then(Value::as_array))
                    .context("premises file must contain a selections array")?;
                let harness_obj = input
                    .as_object_mut()
                    .context("input must be a JSON object")?
                    .entry("harness")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()))
                    .as_object_mut()
                    .context("harness field must be an object")?;
                let mut merged = match harness_obj.get("semantic_modules") {
                    Some(modules) => modules
                        .as_array()
                        .context("harness semantic_modules must be an array")?
                        .clone(),
                    None => Vec::new(),
                };
                // Preserve distinct purposes and applicability assertions even for
                // the same bundle. The shared snapshot builder caches each bundle
                // once and validates all selections before any execution or output.
                for sel in selections {
                    if !merged.contains(sel) {
                        merged.push(sel.clone());
                    }
                }
                harness_obj.insert("semantic_modules".into(), Value::Array(merged));
            }
            let mut result = if attempts.is_some() {
                harness::step_with_reviewer(&root, &input, &args.output, &v2::RustReviews)?
            } else {
                harness::consolidate_current(&root, &input, &args.output)?
            };
            result["execution_binding"] = execution_binding;
            return harness_summary(&result);
        }
        Command::Semantics { command } => match *command {
            SemanticCommand::TypedProjectReview { input } => {
                use oe_contracts::TypedPromotedProjectReviewPort1;
                let request = serde_json::from_value(read_input(&input)?)?;
                let executor =
                    oe_kernel::project::TypedPromotedProjectReviewExecutor { root: &root };
                let response = match executor.review(&request) {
                    Ok(receipt) => serde_json::to_value(receipt)?,
                    Err(failure) => serde_json::to_value(failure)?,
                };
                println!("{}", serde_json::to_string_pretty(&response)?);
                return Ok(0);
            }
            SemanticCommand::ProjectReview {
                args,
                evidence_file,
                source_id,
                evidence_format,
                scope_id,
                focus_iri,
                focus_type_iri,
                query_asset_id,
                shape_asset_id,
                recorded_at,
            } => {
                let request = oe_semantic::lifecycle::ProjectReviewRequest {
                    binding: &args.binding,
                    workspace: &args.workspace,
                    task: &args.task,
                    evidence_file: &evidence_file,
                    source_id: &source_id,
                    evidence_format: &evidence_format,
                    scope_id: &scope_id,
                    focus_iri: &focus_iri,
                    focus_type_iri: &focus_type_iri,
                    query_asset_id: &query_asset_id,
                    shape_asset_id: &shape_asset_id,
                };
                return semantic_delivery(
                    "review",
                    oe_semantic::lifecycle::project_review_at(
                        root.path(),
                        &request,
                        recorded_at.as_deref(),
                    ),
                );
            }
            SemanticCommand::Discover { binding, workspace } => {
                return semantic_delivery(
                    "discover",
                    oe_semantic::lifecycle::discover(
                        root.path(),
                        binding.as_deref(),
                        workspace.as_deref(),
                    ),
                );
            }
            SemanticCommand::Run(args) => {
                return semantic_delivery(
                    "run",
                    oe_semantic::lifecycle::run(root.path(), &args.request()),
                );
            }
            SemanticCommand::Open { args, engagement } => {
                return semantic_delivery(
                    "open",
                    oe_semantic::lifecycle::open(
                        root.path(),
                        &args.binding,
                        &args.task,
                        &args.workspace,
                        engagement.as_deref(),
                    ),
                );
            }
            SemanticCommand::Propose {
                args,
                delta,
                engagement,
                recorded_at,
            } => {
                return semantic_delivery(
                    "propose",
                    oe_semantic::lifecycle::propose_at(
                        root.path(),
                        &args.binding,
                        &args.task,
                        &args.workspace,
                        &delta,
                        &engagement,
                        recorded_at.as_deref(),
                    ),
                );
            }
            SemanticCommand::Commit(args) => {
                return semantic_delivery(
                    "commit",
                    oe_semantic::lifecycle::transition_at(
                        root.path(),
                        "commit",
                        &args.args.binding,
                        &args.args.task,
                        &args.args.workspace,
                        &args.candidate,
                        Some(&args.authorization),
                        args.recorded_at.as_deref(),
                    ),
                );
            }
            SemanticCommand::Verify { args, candidate } => {
                return semantic_delivery(
                    "verify",
                    verify_execution(root.path(), &args, candidate.as_deref()),
                );
            }
            SemanticCommand::History {
                binding,
                workspace,
                candidate,
            } => {
                return semantic_delivery(
                    "history",
                    oe_semantic::lifecycle::history(root.path(), &binding, &workspace, &candidate),
                );
            }
            SemanticCommand::Promote(args) => {
                return semantic_delivery(
                    "promote",
                    oe_semantic::lifecycle::transition_at(
                        root.path(),
                        "promote",
                        &args.args.binding,
                        &args.args.task,
                        &args.args.workspace,
                        &args.candidate,
                        Some(&args.authorization),
                        args.recorded_at.as_deref(),
                    ),
                );
            }
            SemanticCommand::Bootstrap { command } => {
                use oe_semantic::bootstrap;
                let result = match command {
                    BootstrapCommand::Prepare {
                        directory,
                        project,
                        domain,
                        actor,
                        fact_authority,
                        evidence_root,
                        bundle,
                    } => bootstrap::prepare(
                        root.path(),
                        &directory,
                        &bootstrap::PrepareOptions {
                            project,
                            domain,
                            actor,
                            fact_authority,
                            evidence_root,
                            name: bundle,
                        },
                    ),
                    BootstrapCommand::Apply {
                        directory,
                        authorization,
                    } => bootstrap::apply(root.path(), &directory, &authorization),
                    BootstrapCommand::Adopt {
                        directory,
                        authorization,
                    } => bootstrap::adopt(root.path(), &directory, &authorization),
                };
                let (value, code) = match result {
                    Ok(value) => (value, 0),
                    Err(error) => (
                        json!({"status":"blocked","error_type":"BootstrapError","message":error.to_string()}),
                        1,
                    ),
                };
                println!("{}", serde_json::to_string_pretty(&value)?);
                return Ok(code);
            }
            SemanticCommand::Review { input, output } => {
                let input = read_input(&input)?;
                let object = input.as_object().context("review_input_requires_object")?;
                ensure!(
                    object.len() == 2
                        && object.contains_key("project_id")
                        && object.contains_key("review_requests"),
                    "review_input_contract_mismatch"
                );
                let project_id = oe_core::text(&input["project_id"], "project_id")?;
                let result = oe_semantic::review::execute_reviews_with(
                    root.path(),
                    &input["review_requests"],
                    project_id,
                    &output,
                    &oe_kernel::forward::RustExecutor,
                )?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                // Execution may complete while the actual verdict remains blocked.
                return Ok(v2::review_exit_status(&result));
            }
            SemanticCommand::Doctor {
                binding,
                identity_only,
                python,
                staging_descriptor,
            } => {
                if !identity_only {
                    return semantic_delivery(
                        "doctor",
                        oe_semantic::lifecycle::doctor(root.path(), binding.as_deref()),
                    );
                }
                let interpreter = python.unwrap_or_else(|| {
                    root.path().join(if cfg!(windows) {
                        "runtime/.venv/Scripts/python.exe"
                    } else {
                        "runtime/.venv/bin/python"
                    })
                });
                let staging = staging_descriptor
                    .as_deref()
                    .map(|path| root.private_path(path))
                    .transpose()?;
                match oe_semantic::runtime::verify_runtime_source_identity(
                    root.path(),
                    staging.as_deref(),
                    &interpreter,
                ) {
                    Ok(report) => println!(
                        "{}",
                        json!({"status": "passed", "scope": "external_runtime_identity", "verification": report})
                    ),
                    Err(_) => {
                        println!(
                            "{}",
                            json!({"status": "blocked", "scope": "external_runtime_identity",
                            "reason": "runtime_identity_not_verified", "semantic_execution": "not_run"})
                        );
                        return Ok(1);
                    }
                }
            }
            SemanticCommand::Bundle { name } => println!(
                "{}",
                serde_json::to_string_pretty(
                    &oe_semantic::load_bundle(root.path(), &name)?.report
                )?
            ),
            SemanticCommand::Premises(args) => {
                let result = oe_semantic::build_premises(root.path(), &read_input(&args.input)?)?;
                deliver(&root, args.output.as_deref(), "premises.json", &result)?;
            }
            SemanticCommand::ValidateChapterContracts { release } => {
                let runtime = oe_semantic::lifecycle::default_runtime(root.path())?;
                let report = oe_semantic::chapter_contracts::validate(&runtime)?;
                println!("{}", serde_json::to_string_pretty(&report.to_json())?);
                if release {
                    ensure!(report.release_ready(), "chapter_registry_not_release_ready");
                } else {
                    ensure!(
                        report.structurally_valid(),
                        "chapter_registry_not_structurally_valid"
                    );
                }
            }
            SemanticCommand::RebindBooks {
                book_root,
                semantica_root,
                write,
            } => {
                let result = oe_semantic::rebind_books::rebind(&book_root, &semantica_root, write)?;
                println!("{}", serde_json::to_string_pretty(&result.to_json())?);
                ensure!(
                    result.errors.is_empty(),
                    "rebind_errors: {:?}",
                    result.errors
                );
            }
        },
    }
    Ok(0)
}

fn semantic_protocol_name(name: &str) -> Option<&'static str> {
    match name {
        "doctor" => Some("doctor"),
        "discover" => Some("discover"),
        "run" => Some("run"),
        "open" => Some("open"),
        "propose" => Some("propose"),
        "commit" => Some("commit"),
        "verify" => Some("verify"),
        "history" => Some("history"),
        "promote" => Some("promote"),
        "project-review" | "typed-project-review" => Some("review"),
        _ => None,
    }
}

// Read only the command prefix and known global option positions. An input
// value containing "semantics" must not change another command's protocol.
fn semantic_argument_command(arguments: &[std::ffi::OsString]) -> Option<&'static str> {
    let mut index = 1;
    let mut semantics = false;
    while let Some(argument) = arguments.get(index) {
        if argument == "--root" || argument == "--project-home" {
            arguments.get(index + 1)?;
            index += 2;
            continue;
        }
        let value = argument.to_str()?;
        if value.starts_with("--root=") || value.starts_with("--project-home=") {
            index += 1;
            continue;
        }
        if semantics {
            return semantic_protocol_name(value);
        }
        if value != "semantics" {
            return None;
        }
        semantics = true;
        index += 1;
    }
    None
}

fn semantic_parsed_command(cli: &Cli) -> Option<&'static str> {
    let Command::Semantics { command } = &cli.command else {
        return None;
    };
    match command.as_ref() {
        SemanticCommand::Doctor {
            identity_only: true,
            ..
        } => None,
        SemanticCommand::Doctor { .. } => Some("doctor"),
        SemanticCommand::Discover { .. } => Some("discover"),
        SemanticCommand::Run(_) => Some("run"),
        SemanticCommand::Open { .. } => Some("open"),
        SemanticCommand::Propose { .. } => Some("propose"),
        SemanticCommand::Commit(_) => Some("commit"),
        SemanticCommand::Verify { .. } => Some("verify"),
        SemanticCommand::History { .. } => Some("history"),
        SemanticCommand::Promote(_) => Some("promote"),
        SemanticCommand::ProjectReview { .. } | SemanticCommand::TypedProjectReview { .. } => {
            Some("review")
        }
        _ => None,
    }
}

fn knowledge_argument_command(arguments: &[std::ffi::OsString]) -> Option<&'static str> {
    let mut index = 1;
    let mut knowledge = false;
    while let Some(argument) = arguments.get(index) {
        if argument == "--root" || argument == "--project-home" {
            arguments.get(index + 1)?;
            index += 2;
            continue;
        }
        let value = argument.to_str()?;
        if value.starts_with("--root=") || value.starts_with("--project-home=") {
            index += 1;
            continue;
        }
        if knowledge {
            return match value {
                "status" => Some("status"),
                "register" => Some("register"),
                "query" => Some("query"),
                "view" => Some("view"),
                "verify-packet" => Some("verify-packet"),
                "feedback" => Some("feedback"),
                "project-context" => Some("project-context"),
                _ => Some("knowledge"),
            };
        }
        if value != "knowledge" {
            return None;
        }
        knowledge = true;
        index += 1;
    }
    knowledge.then_some("knowledge")
}

fn maintenance_argument_family(arguments: &[std::ffi::OsString]) -> Option<&'static str> {
    let mut index = 1;
    while let Some(argument) = arguments.get(index) {
        if argument == "--root" || argument == "--project-home" {
            arguments.get(index + 1)?;
            index += 2;
            continue;
        }
        if argument.to_str()?.starts_with("--root=")
            || argument.to_str()?.starts_with("--project-home=")
        {
            index += 1;
            continue;
        }
        return match argument.to_str()? {
            "providers" => Some("providers"),
            "books" => Some("books"),
            "runtime" => Some("runtime"),
            "privacy" => Some("privacy"),
            "browser" => Some("browser"),
            "supplier" => Some("supplier"),
            "route" => Some("route"),
            "harness" => Some("harness"),
            "design" => Some("design"),
            "kernel" => Some("kernel"),
            "project" => Some("project"),
            "maintain" => Some("maintain"),
            _ => None,
        };
    }
    None
}

fn maintenance_error(family: &str, code: &str) -> Value {
    // These two entry points retain the established stdout/error/exit-code
    // contract. Never echo clap diagnostics, parser fragments or input paths.
    match family {
        "design" | "kernel" | "project" | "maintain" => {
            return v2::command_failure(family, &anyhow::anyhow!(code.to_owned()));
        }
        "route" => {
            return json!({"status":"input_or_output_error", "model_execution":"not_confirmed",
            "next_step":"inspect_input_shape_and_select_new_output_directory"});
        }
        "harness" => {
            return json!({"status":"input_or_execution_error", "ontology_promotion":"not_performed",
            "next_step":"inspect_exact_inputs_and_partial_output_before_retry"});
        }
        _ => {}
    }
    json!({"schema":"ontology-engineering.maintenance-command-error/v1", "command":family,
        "status":"blocked", "error":{"code":code}, "semantic_execution":"not_confirmed",
        "publication":"not_performed", "next_step":"inspect_root_arguments_and_retained_private_operation_reports"})
}

fn main() -> ExitCode {
    let arguments = std::env::args_os().collect::<Vec<_>>();
    let semantic_command = semantic_argument_command(&arguments);
    let knowledge_command = knowledge_argument_command(&arguments);
    let maintenance_family = maintenance_argument_family(&arguments);
    let cli = match Cli::try_parse_from(&arguments) {
        Ok(cli) => cli,
        Err(error) => {
            if error.use_stderr()
                && let Some(family) = maintenance_family
            {
                println!("{}", maintenance_error(family, "invalid_arguments"));
                return ExitCode::from(2);
            }
            if error.use_stderr()
                && let Some(command) = knowledge_command
            {
                println!(
                    "{}",
                    knowledge_error(command, "invalid_knowledge_arguments")
                );
                return ExitCode::from(2);
            }
            if error.use_stderr()
                && let Some(command) = semantic_command
            {
                let error =
                    semantic_argument_error("invalid or unsupported semantic command arguments");
                println!(
                    "{}",
                    oe_semantic::lifecycle::error_response(command, &error)
                );
                return ExitCode::SUCCESS;
            }
            error.exit();
        }
    };
    let v2_family = match &cli.command {
        Command::Design { .. } => Some("design"),
        Command::Kernel { .. } => Some("kernel"),
        Command::Schemas { .. } => Some("schemas"),
        Command::Project { .. } => Some("project"),
        Command::Maintain { .. } => Some("maintain"),
        _ => None,
    };
    let experience_command = matches!(&cli.command, Command::Knowledge { command }
        if matches!(command.as_ref(), KnowledgeCommand::Experience { .. }));
    let semantic_command = semantic_parsed_command(&cli);
    match execute(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) if v2_family.is_some() => {
            eprintln!("{}", v2::command_failure(v2_family.unwrap(), &error));
            ExitCode::from(2)
        }
        Err(error) if maintenance_family == Some("harness") => {
            let mut response =
                maintenance_error("harness", "root_input_or_output_contract_unavailable");
            response["error"] = harness::safe_failure(&error)
                .unwrap_or_else(|| v2::command_failure("harness", &error)["error"].clone());
            println!("{}", response);
            ExitCode::from(2)
        }
        Err(_) if maintenance_family.is_some() => {
            println!(
                "{}",
                maintenance_error(
                    maintenance_family.unwrap(),
                    "root_input_or_output_contract_unavailable"
                )
            );
            ExitCode::from(2)
        }
        Err(_) if semantic_command.is_some() => {
            let error = semantic_argument_error("semantic root or input contract unavailable");
            println!(
                "{}",
                oe_semantic::lifecycle::error_response(semantic_command.unwrap(), &error)
            );
            ExitCode::SUCCESS
        }
        Err(error) if knowledge_command.is_some() => {
            let mut response = knowledge_error(
                knowledge_command.unwrap(),
                "knowledge_root_input_source_or_output_contract_unavailable",
            );
            if experience_command {
                response["errors"][0]["reason"] =
                    v2::command_failure("knowledge-experience", &error)["error"]["reason"].clone();
            }
            println!("{}", response);
            ExitCode::from(2)
        }
        Err(_) => {
            // Paths, parser fragments and provider bodies can contain private data.
            eprintln!(
                "{}",
                json!({"status": "input_or_output_error", "model_execution": "not_confirmed",
                "next_step": "check_root_input_contract_source_locks_and_new_private_output_directory"})
            );
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_project_home_is_a_global_argument_and_not_a_command_literal() {
        let cli = Cli::try_parse_from([
            "oev2",
            "--root",
            "/software",
            "--project-home",
            "/project",
            "project",
            "workspace",
            "resume",
        ])
        .unwrap();
        assert_eq!(cli.root.as_deref(), Some(Path::new("/software")));
        assert_eq!(cli.project_home.as_deref(), Some(Path::new("/project")));
        assert!(matches!(
            cli.command,
            Command::Project {
                command: v2::ProjectCommand::Workspace {
                    command: workspace::WorkspaceCommand::Resume { output: None }
                }
            }
        ));
        let args = [
            "oev2",
            "--project-home",
            "semantics",
            "knowledge",
            "experience",
            "list",
        ]
        .map(std::ffi::OsString::from);
        assert_eq!(semantic_argument_command(&args), None);
        assert_eq!(knowledge_argument_command(&args), Some("knowledge"));
        let args =
            ["oev2", "--project-home=/project", "harness", "step"].map(std::ffi::OsString::from);
        assert_eq!(maintenance_argument_family(&args), Some("harness"));
    }

    #[test]
    fn project_home_creation_checks_scope_before_any_directory_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(directory.path()).unwrap();
        let installation = base.join("software");
        std::fs::create_dir(&installation).unwrap();
        std::fs::write(installation.join("SKILL.md"), "fixture").unwrap();
        let root = SkillRoot::new(&installation).unwrap();
        for forbidden in [
            installation.clone(),
            base.clone(),
            installation.join("sources/new"),
            installation.join("generated/new"),
            installation.join("var"),
            installation.join("var/../sources/new"),
            PathBuf::from("relative-home"),
        ] {
            assert!(
                create_project_home(&root, &forbidden).is_err(),
                "{}",
                forbidden.display()
            );
        }
        for untouched in ["sources", "generated", "var"] {
            assert!(!installation.join(untouched).exists());
        }
        let external = base.join("new/project");
        assert_eq!(create_project_home(&root, &external).unwrap(), external);
        let legacy_private = installation.join("var/projects/explicit");
        assert_eq!(
            create_project_home(&root, &legacy_private).unwrap(),
            legacy_private
        );
        #[cfg(unix)]
        {
            let alias = base.join("alias");
            std::os::unix::fs::symlink(&external, &alias).unwrap();
            assert!(create_project_home(&root, &alias.join("uncreated")).is_err());
            assert!(!external.join("uncreated").exists());
        }
    }

    #[test]
    fn harness_inputs_preserve_legacy_literals_and_bind_explicit_project_paths() {
        let installation = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let source = std::fs::canonicalize(installation.path()).unwrap();
        let home = std::fs::canonicalize(project.path()).unwrap();
        std::fs::write(source.join("SKILL.md"), "fixture").unwrap();
        let legacy = SkillRoot::new(&source).unwrap();
        assert_eq!(
            harness_input_path(&legacy, Path::new("input.json")).unwrap(),
            PathBuf::from("input.json")
        );
        let bound = legacy.with_project_data_root(&home).unwrap();
        assert_eq!(
            harness_input_path(&bound, Path::new("input.json")).unwrap(),
            home.join("input.json")
        );
        assert!(harness_input_path(&bound, &source.join("input.json")).is_err());
        assert!(harness_input_path(&bound, Path::new("../input.json")).is_err());
    }

    #[test]
    fn semantic_argument_protocol_uses_command_positions_not_input_values() {
        for (arguments, expected) in [
            (
                vec!["oe", "--root", "/skill", "semantics", "doctor"],
                Some("doctor"),
            ),
            (
                vec!["oe", "semantics", "--root", "/skill", "run"],
                Some("run"),
            ),
            (
                vec!["oe", "--root=/skill", "semantics", "project-review"],
                Some("review"),
            ),
            (vec!["oe", "--root", "semantics", "route", "prepare"], None),
            (
                vec!["oe", "route", "prepare", "--input", "semantics", "doctor"],
                None,
            ),
            (vec!["oe", "semantics", "review", "--input", "doctor"], None),
        ] {
            let arguments = arguments
                .into_iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>();
            assert_eq!(semantic_argument_command(&arguments), expected);
        }
    }

    #[test]
    fn execution_modes_preserve_optional_inputs_and_legacy_review_option_names() {
        let cli = Cli::try_parse_from([
            "oe",
            "semantics",
            "run",
            "--binding",
            "binding.json",
            "--recorded-at",
            "2026-09-28T00:00:00Z",
        ])
        .unwrap();
        let Command::Semantics { command } = cli.command else {
            panic!("run parsed as another command")
        };
        let SemanticCommand::Run(args) = *command else {
            panic!("run parsed as another semantic operation")
        };
        assert!(args.scenario.is_none());
        assert!(args.task.is_none());
        assert!(args.workspace.is_none());
        assert_eq!(args.recorded_at.as_deref(), Some("2026-09-28T00:00:00Z"));
        let cli = Cli::try_parse_from([
            "oe",
            "semantics",
            "project-review",
            "--binding",
            "binding.json",
            "--task",
            "task.json",
            "--workspace",
            "registry",
            "--evidence-file",
            "evidence.ttl",
            "--source-id",
            "source",
            "--format",
            "turtle",
            "--scope",
            "scope",
            "--focus",
            "urn:focus",
            "--focus-type",
            "urn:type",
            "--query-asset",
            "query",
            "--shape-asset",
            "shape",
            "--recorded-at",
            "2026-09-28T00:00:00Z",
        ])
        .unwrap();
        let Command::Semantics { command } = cli.command else {
            panic!("review parsed as another command")
        };
        let SemanticCommand::ProjectReview {
            evidence_format,
            scope_id,
            recorded_at,
            ..
        } = *command
        else {
            panic!("review parsed as another semantic operation")
        };
        assert_eq!(evidence_format, "turtle");
        assert_eq!(scope_id, "scope");
        assert_eq!(recorded_at.as_deref(), Some("2026-09-28T00:00:00Z"));
    }

    #[test]
    fn completed_execution_keeps_blocked_semantic_result_visible() {
        let summary = harness_summary_value(&json!({"status":"completed", "feedback":{
            "status":"completed", "reviews":[
                {"id":"old-revision", "review":{"status":"blocked", "findings":[{"reason":"revision_mismatch"}],"violations":[]}},
                {"id":"missing-status", "review":{}}
            ]}, "engineering_acceptance":"not_determined"}));
        assert_eq!(summary["status"], "completed");
        assert_eq!(summary["semantic_reviews"][0]["status"], "blocked");
        assert_eq!(summary["semantic_reviews"][0]["findings"], 1);
        assert_eq!(summary["semantic_reviews"][1]["status"], "unknown");
        assert!(summary["semantic_reviews"][1]["findings"].is_null());
        assert_eq!(summary["engineering_acceptance"], "not_determined");
    }
}
