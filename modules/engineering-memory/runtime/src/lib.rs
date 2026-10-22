//! One application owner for history, experience and actual-use feedback.
pub mod evidence;
pub use evidence::contracts;
#[rustfmt::skip]
pub mod generated;
pub mod exchange;
// --- track D: history anchor catalog ---
pub mod history_index;
pub mod hooks;
pub mod organization;
mod organization_disk;
pub mod receiver;
mod retrieval;
pub mod session;
pub mod snapshot;
pub mod source_origin;
pub mod store;
pub mod worker;
pub mod workflow;

use anyhow::{Context, Result};
use contracts::{
    CaptureRequest, ExperienceRecord, MethodComparison, SessionExportRequest, SessionFeedback,
    SessionRequest, SourceAliasRequest, SourceRegistration, SyncBudget, TickPolicy, UsageFeedback,
};
use contracts::{V2PrepareRequest, V2VerifyRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use store::Store;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Organization {
        action: String,
        input: Value,
    },
    AliasSource {
        request: SourceAliasRequest,
    },
    V2Prepare {
        request: V2PrepareRequest,
    },
    V2Verify {
        request: V2VerifyRequest,
    },
    Capabilities,
    SessionPrepare {
        request: SessionRequest,
    },
    SessionGet {
        id: String,
    },
    SessionVerify {
        id: String,
    },
    SessionActivate {
        id: String,
        context_sha256: String,
    },
    HookIntake {
        max_sources: usize,
        max_verify_bytes: u64,
    },
    SessionHandoff {
        context_id: String,
        context_sha256: String,
        id: String,
        revision: u64,
    },
    SessionFeedback {
        input: SessionFeedback,
    },
    SessionExport {
        request: SessionExportRequest,
    },
    SessionExportVerify {
        id: String,
    },
    Validate {
        record: ExperienceRecord,
    },
    Record {
        record: ExperienceRecord,
    },
    Get {
        id: String,
        revision: Option<u64>,
    },
    Verify {
        id: String,
        revision: Option<u64>,
    },
    Recall {
        query: String,
        project: Option<String>,
        conditions: Vec<String>,
        limit: usize,
        include_other_projects: bool,
    },
    Feedback {
        feedback: UsageFeedback,
    },
    Compare {
        comparison: MethodComparison,
    },
    Handoff {
        id: String,
        revision: u64,
        project: Option<String>,
        conditions: Vec<String>,
    },
    Register {
        registration: SourceRegistration,
        max_verify_bytes: u64,
    },
    CaptureSource {
        request: CaptureRequest,
    },
    Sync {
        budget: SyncBudget,
    },
    Status,
    Pending {
        limit: usize,
    },
    Packet {
        id: String,
    },
    SourceEligibility {
        packet_id: String,
    },
    Resolve {
        resolution: workflow::Resolution,
    },
    ConfigureExtractor {
        config: worker::Extractor,
    },
    Run {
        budget: SyncBudget,
        max_packets: usize,
    },
    Tick {
        policy: TickPolicy,
    },
    // --- track D: history anchor catalog ---
    HistoryIndexBuild {
        request: contracts::HistoryIndexBuildRequest,
    },
    HistoryIndexStatus,
    HistorySearch {
        request: contracts::HistorySearchRequest,
    },
}

pub fn capabilities() -> Value {
    json!({"schema":"engineering-memory.runtime/1", "implementation":"engineering-memory-rust",
      "version":env!("CARGO_PKG_VERSION"),
      "operations":["record","get","verify","recall","feedback","compare","handoff","register","alias_source","capture_source","sync","status","pending","packet","source_eligibility","resolve","configure_extractor","run","tick","session_prepare","session_get","session_verify","session_activate","session_handoff","session_feedback","session_export","session_export_verify","v2_prepare","v2_verify","hook","hook_intake","organization","history_index_build","history_index_status","history_search"],
      "organization":{"automatic_model_dispatch":false,"semantic_acceptance":"explicit host-reviewed evidence","actions":["method-register","method-list","create","list","status","frontier","packet","record","inspect-claim","close","handoff","handoff-status","publish","publication-status"]},
      "retrieval":"unicode_field_density/2", "semantic_engine":semantic_identity(),
      "semantic_execution":"unary Horn scope rule from shared v2 implementation and generated contract",
      "source_reader":"CASS-derived native Rust source-window parser", "v2_project_write":false,
      "automated_backfill":"bounded incremental intake; explicit agent review or configured local extractor produces drafts",
      "built_in_model_provider":false,"causal_validation":false,"rrsi_gain":"not_measured",
      "engineering_adoption":"not_granted"})
}

pub fn execute(store: &Store, request: Request) -> Result<Value> {
    match request {
        Request::Organization { action, input } => {
            organization::execute(&store.directory, &action, input)
        }
        Request::AliasSource { request } => store.alias_source(&request),
        Request::V2Prepare { request } => store.v2_prepare(&request),
        Request::V2Verify { request } => store.v2_verify(&request),
        Request::Capabilities => Ok(capabilities()),
        Request::SessionPrepare { request } => store.session_prepare(&request),
        Request::SessionGet { id } => store.session_get(&id),
        Request::SessionVerify { id } => store.session_verify(&id),
        Request::SessionActivate { id, context_sha256 } => {
            store.session_activate(&id, &context_sha256)
        }
        Request::HookIntake {
            max_sources,
            max_verify_bytes,
        } => store.ingest_hook_sources(max_sources, max_verify_bytes),
        Request::SessionHandoff {
            context_id,
            context_sha256,
            id,
            revision,
        } => store.session_handoff(&context_id, &context_sha256, &id, revision),
        Request::SessionFeedback { input } => store.session_feedback(&input),
        Request::SessionExport { request } => store.session_export(&request),
        Request::SessionExportVerify { id } => store.session_export_verify(&id),
        Request::Validate { record } => {
            store::validate_experience(&record)?;
            Ok(serde_json::to_value(record)?)
        }
        Request::Record { record } => store.record(&record),
        Request::Get { id, revision } => Ok(serde_json::to_value(store.get(&id, revision)?)?),
        Request::Verify { id, revision } => store.verify_record(&id, revision),
        Request::Recall {
            query,
            project,
            conditions,
            limit,
            include_other_projects,
        } => store.recall(
            &query,
            project.as_deref(),
            &conditions,
            limit,
            include_other_projects,
        ),
        Request::Feedback { feedback } => store.feedback(&feedback),
        Request::Compare { comparison } => store.compare(&comparison),
        Request::Handoff {
            id,
            revision,
            project,
            conditions,
        } => store.handoff(&id, revision, project.as_deref(), &conditions),
        Request::Register {
            registration,
            max_verify_bytes,
        } => store.register(registration, max_verify_bytes),
        Request::CaptureSource { request } => store.capture_source(&request),
        Request::Sync { budget } => store.sync(&budget),
        Request::Status => store.status(),
        Request::Pending { limit } => store.pending(limit),
        Request::Packet { id } => Ok(serde_json::to_value(store.packet(&id)?)?),
        Request::SourceEligibility { packet_id } => store.source_eligibility(&packet_id),
        Request::Resolve { resolution } => store.resolve(&resolution),
        Request::ConfigureExtractor { config } => store.configure_extractor(&config),
        Request::Run {
            budget,
            max_packets,
        } => store.run(&budget, max_packets),
        Request::Tick { policy } => store.tick(&policy),
        // --- track D: history anchor catalog ---
        Request::HistoryIndexBuild { request } => history_index::build(&store.directory, &request),
        Request::HistoryIndexStatus => history_index::status(&store.directory),
        Request::HistorySearch { request } => history_index::search(&store.directory, &request),
    }
    .context("engineering_memory_request_failed")
}

pub fn semantic_identity() -> &'static str {
    semantica_runtime::ENGINE_ID
}
