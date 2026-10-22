//! Source-bound native stdio/query composition; no model or engineering adoption effects.
use anyhow::{Result, bail, ensure};
use clap::Subcommand;
use oe_core::{SkillRoot, read_regular_bytes};
use semantica_contracts::SemanticServiceQueryRequest;
use semantica_runtime::dataset::DatasetSession;
use semantica_service::{
    McpService, NativeServiceBudget, StdioProfile, load_context_graph, serve_stdio,
};
use std::path::PathBuf;
mod http;
mod path;
mod path_details;
mod sparql;

#[derive(Subcommand)]
pub enum ServiceCommand {
    /// Source-bound Explorer query under one process deadline; no engineering adoption.
    ExplorerQuery(sparql::ExplorerQueryArgs),
    ExplorerQueryV2(sparql::ExplorerQueryV2Args),
    /// Native source-bound path query JSON under the existing process supervisor.
    ExplorerPath(sparql::ExplorerQueryV2Args),
    #[command(hide = true)]
    PathWorker,
    ExplorerPathDetails(path_details::ExplorerPathDetailsArgs),
    #[command(hide = true)]
    PathDetailsWorker,
    /// Internal source-bound child; process supervision is supplied by its host.
    #[command(hide = true)]
    SparqlWorker,
    #[command(hide = true)]
    SparqlWorkerV2,
    /// Serve the existing graph statistics profile through a bounded loopback host.
    HttpGraphStats(http::HttpGraphStatsArgs),
    HttpGraphRead(http::HttpGraphReadArgs),
    /// Original search POST on the same source-bound finite HTTP host.
    HttpSearch(http::HttpGraphReadArgs),
    HttpDecisions(http::HttpGraphReadArgs),
    HttpSparql(http::HttpSparqlArgs),
    HttpSparqlV2(http::HttpSparqlV2Args),
    HttpPath(http::HttpPathArgs),
    /// Real native MCP stdio. Unsupported tools stay unavailable; no HTTP server is advertised.
    McpStdio {
        /// Expose the existing registered book catalogue and its manifest resources.
        #[arg(long)]
        book_catalog: bool,
        #[arg(long)]
        context_graph: Option<PathBuf>,
        #[arg(long,default_value="canonical",value_parser=["canonical","compatibility-batch"])]
        wire_profile: String,
        /// Explicitly select graph mutations and finite native tool business.
        /// Original retains canonical errors; native declares corrections and backend gaps.
        #[arg(long,default_value="read-only",value_parser=["read-only","original","native"])]
        tool_profile: String,
    },
    /// Read-only query against an explicit, byte-verified RDF source using the same service/session port.
    Query {
        #[arg(long)]
        data: PathBuf,
        #[arg(long)]
        data_sha256: String,
        #[arg(long,default_value="ntriples",value_parser=["turtle","ntriples","rdfxml","trig","nquads"])]
        rdf_format: String,
        #[arg(long)]
        input: PathBuf,
    },
}
pub fn execute(root: &SkillRoot, command: ServiceCommand) -> Result<u8> {
    match execute_bound(root, command) {
        Ok(code) => Ok(code),
        Err(error) => {
            let codes = [
                "compiled_source_binding_mismatch",
                "compiled_generated_contract_binding_mismatch",
                "compiled_semantic_contract_binding_mismatch",
                "service_source_identity_mismatch",
                "service_budget_exceeded",
                "service_frame_budget_exceeded",
                "service_revision_mismatch",
                "service_dataset_identity_mismatch",
                "service_query_failed",
                "explorer_search_integer_float_overflow",
                "explorer_search_tags_not_iterable",
                "explorer_search_deadline_exceeded",
                "explorer_search_cache_budget",
                "explorer_path_source_missing",
                "explorer_path_target_missing",
                "explorer_path_negative_weight",
                "explorer_path_weight_invalid",
                "explorer_path_cost_overflow",
                "explorer_path_deadline_exceeded",
                "explorer_path_identifier_budget",
                "explorer_path_query_contract",
                "explorer_path_execution_failed",
                "explorer_path_details_budget",
                "context_projection_unavailable",
                "explorer_worker_resource_profile_mismatch",
                "explorer_worker_input_budget",
                "explorer_worker_contract_invalid",
                "explorer_worker_query_budget",
                "explorer_worker_read_only_required",
                "explorer_worker_dataset_changed",
                "explorer_worker_result_invalid",
                "explorer_worker_output_budget",
                "explorer_query_deadline_exceeded",
                "explorer_query_timed_out",
                "explorer_query_child_incomplete",
                "explorer_query_evidence_outside_private_root",
            ];
            let code = codes
                .into_iter()
                .find(|code| error.chain().any(|cause| cause.to_string() == *code))
                .unwrap_or("service_input_or_output_error");
            eprintln!(
                "{}",
                serde_json::json!({"schema":"semantica.service-cli-error/1","execution":"failed","error":{"code":code},"model_execution":"not_run","engineering_adoption":"not_established"})
            );
            Ok(2)
        }
    }
}
fn execute_bound(root: &SkillRoot, command: ServiceCommand) -> Result<u8> {
    let _binding = crate::v2::require_execution(root, "semantic_service")?;
    let mut session = DatasetSession::new();
    let budget = NativeServiceBudget::bounded();
    match command {
        ServiceCommand::ExplorerQuery(args) => {
            sparql::execute_host(root, args, &mut session, &budget)
        }
        ServiceCommand::PathWorker => path::execute_worker(),
        ServiceCommand::PathDetailsWorker => path_details::execute_worker(),
        ServiceCommand::ExplorerPathDetails(args) => {
            let mut selected = DatasetSession::with_resources(
                &semantica_contracts::SemanticDatasetResourceSelection {
                    schema: "semantica.dataset-resource-selection/1".into(),
                    profile: args.query.resource_profile,
                },
            )?;
            path_details::execute_host(
                root,
                args.query.query,
                args.work_limit,
                &mut selected,
                &budget,
            )
        }
        ServiceCommand::ExplorerPath(args) => {
            let mut selected = DatasetSession::with_resources(
                &semantica_contracts::SemanticDatasetResourceSelection {
                    schema: "semantica.dataset-resource-selection/1".into(),
                    profile: args.resource_profile,
                },
            )?;
            path::execute_host(root, args.query, &mut selected, &budget)
        }
        ServiceCommand::SparqlWorker => sparql::execute_worker(),
        ServiceCommand::SparqlWorkerV2 => sparql::execute_worker_v2(),
        ServiceCommand::ExplorerQueryV2(args) => {
            let mut selected = DatasetSession::with_resources(
                &semantica_contracts::SemanticDatasetResourceSelection {
                    schema: "semantica.dataset-resource-selection/1".into(),
                    profile: args.resource_profile,
                },
            )?;
            sparql::execute_host_v2(root, args.query, &mut selected, &budget)
        }
        ServiceCommand::HttpPath(args) => {
            let mut selected = DatasetSession::with_resources(
                &semantica_contracts::SemanticDatasetResourceSelection {
                    schema: "semantica.dataset-resource-selection/1".into(),
                    profile: args.resource_profile,
                },
            )?;
            http::execute_path(root, args.query, args.work_limit, &mut selected, &budget)
        }
        ServiceCommand::HttpSparqlV2(args) => {
            let mut selected = DatasetSession::with_resources(
                &semantica_contracts::SemanticDatasetResourceSelection {
                    schema: "semantica.dataset-resource-selection/1".into(),
                    profile: args.resource_profile,
                },
            )?;
            http::execute_sparql_v2(root, args.query, &mut selected, &budget)
        }
        ServiceCommand::HttpSparql(args) => http::execute_sparql(root, args, &mut session, &budget),
        ServiceCommand::HttpDecisions(args) => {
            let mut selected = DatasetSession::with_resources(
                &semantica_contracts::SemanticDatasetResourceSelection {
                    schema: "semantica.dataset-resource-selection/1".into(),
                    profile: args.resource_profile.clone(),
                },
            )?;
            http::execute_decisions(root, args, &mut selected, &budget)
        }
        ServiceCommand::HttpSearch(args) => {
            let mut selected = DatasetSession::with_resources(
                &semantica_contracts::SemanticDatasetResourceSelection {
                    schema: "semantica.dataset-resource-selection/1".into(),
                    profile: args.resource_profile.clone(),
                },
            )?;
            http::execute_search(root, args, &mut selected, &budget)
        }
        ServiceCommand::HttpGraphRead(args) => {
            let mut selected = DatasetSession::with_resources(
                &semantica_contracts::SemanticDatasetResourceSelection {
                    schema: "semantica.dataset-resource-selection/1".into(),
                    profile: args.resource_profile.clone(),
                },
            )?;
            http::execute_read(root, args, &mut selected, &budget)
        }
        ServiceCommand::HttpGraphStats(args) => http::execute(root, args, &mut session, &budget),
        ServiceCommand::McpStdio {
            book_catalog,
            context_graph,
            wire_profile,
            tool_profile,
        } => {
            if let Some(path) = context_graph {
                let bytes = read_regular_bytes(&root.expand(&path)?, semantica_runtime::MAX_BYTES)?;
                let graph = oe_core::json::strict_json(&bytes)?;
                load_context_graph(&mut session, &graph)?;
            }
            let books = if book_catalog {
                Some(oe_semantic::books::mcp::RegisteredBookCatalog::open(
                    root.path(),
                )?)
            } else {
                None
            };
            let mut service = McpService::new(&mut session, &budget);
            service = match tool_profile.as_str() {
                "native" => service.with_native_tools(),
                "original" => service.with_original_tools(),
                _ => service,
            };
            if let Some(books) = &books {
                service = service.with_book_catalog(books);
            }
            let profile = match wire_profile.as_str() {
                "canonical" => StdioProfile::Canonical,
                "compatibility-batch" => StdioProfile::CompatibilityBatch,
                _ => bail!("service_wire_profile_unsupported"),
            };
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let stderr = std::io::stderr();
            serve_stdio(
                &service,
                &mut stdin.lock(),
                &mut stdout.lock(),
                &mut stderr.lock(),
                profile,
            )?;
            Ok(0)
        }
        ServiceCommand::Query {
            data,
            data_sha256,
            rdf_format,
            input,
        } => {
            let bytes = read_regular_bytes(&root.expand(&data)?, semantica_runtime::MAX_BYTES)?;
            ensure!(
                semantica_runtime::sha256(&bytes) == data_sha256,
                "service_source_identity_mismatch"
            );
            session.load(
                &bytes,
                semantica_service::rdf_format(&rdf_format)?,
                None,
                None,
            )?;
            let input = oe_core::json::strict_json(&read_regular_bytes(
                &root.expand(&input)?,
                semantica_service::MAX_FRAME,
            )?)?;
            let input: SemanticServiceQueryRequest = serde_json::from_value(input)?;
            let service = McpService::new(&mut session, &budget);
            let response = service
                .execute_query(&input)
                .map_err(|e| anyhow::anyhow!(e.code))?;
            println!("{}", serde_json::to_string_pretty(&response)?);
            Ok(0)
        }
    }
}
