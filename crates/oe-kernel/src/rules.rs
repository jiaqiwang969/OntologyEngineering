//! Engineering facade delegates to the unique generic Rust implementation.
pub use semantica_runtime::rules::*;

pub const MAX_SNAPSHOT_CHECKSUM_WIRE_BYTES: usize =
    semantica_runtime::snapshot_checksum::MAX_INPUT_BYTES;
pub fn execute_local_snapshot_checksum(
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticSnapshotChecksumPort, SemanticSnapshotChecksumRequest};
    let input: SemanticSnapshotChecksumRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .snapshot_checksum(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|out| Ok(serde_json::to_value(out)?))
}

pub const MAX_TEMPORAL_VERSION_DIFF_WIRE_BYTES: usize =
    semantica_runtime::temporal_version_diff::MAX_INPUT_BYTES;
pub fn execute_local_temporal_version_diff(
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{
        SemanticTemporalVersionDiffPort, SemanticTemporalVersionDiffRequest,
    };
    let input: SemanticTemporalVersionDiffRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal_version_diff(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|out| Ok(serde_json::to_value(out)?))
}

pub const MAX_TEMPORAL_VERSION_RECORDS_WIRE_BYTES: usize =
    semantica_runtime::temporal_version_records::MAX_INPUT_BYTES;

/// Source record content/type operations, without persistence or engineering adoption.
pub fn execute_local_temporal_version_records(
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{
        SemanticTemporalVersionRecordRequest, SemanticTemporalVersionRecordsPort,
    };
    let input: SemanticTemporalVersionRecordRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal_version_records(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

pub const MAX_TEMPORAL_PATTERNS_WIRE_BYTES: usize =
    semantica_runtime::temporal_patterns::MAX_INPUT_BYTES;

/// Ordered source pattern session; every action retains its result or failure and state.
pub fn execute_local_temporal_patterns(
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticTemporalPatternsPort, SemanticTemporalPatternsRequest};
    let input: SemanticTemporalPatternsRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal_patterns(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

pub const MAX_TEMPORAL_CONSISTENCY_WIRE_BYTES: usize =
    semantica_runtime::temporal_consistency::MAX_INPUT_BYTES;

/// Original source diagnostics; a completed report does not adopt engineering claims.
pub fn execute_local_temporal_consistency(
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{
        SemanticTemporalConsistencyPort, SemanticTemporalConsistencyRequest,
    };
    let input: SemanticTemporalConsistencyRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal_consistency(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

pub const MAX_TEMPORAL_PATHS_WIRE_BYTES: usize = semantica_runtime::temporal_paths::MAX_INPUT_BYTES;

/// Original source paths; a completed report does not adopt engineering claims.
pub fn execute_local_temporal_paths(input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticTemporalPathsPort, SemanticTemporalPathsRequest};
    let input: SemanticTemporalPathsRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal_paths(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

pub const MAX_DATALOG_WIRE_BYTES: usize = semantica_runtime::datalog_port::MAX_INPUT_BYTES;

pub const MAX_TEMPORAL_WIRE_BYTES: usize = semantica_runtime::temporal_intervals::MAX_INPUT_BYTES;

pub const MAX_TEMPORAL_FACTS_WIRE_BYTES: usize = semantica_runtime::temporal_facts::MAX_INPUT_BYTES;

pub const MAX_TEMPORAL_RANGE_WIRE_BYTES: usize = semantica_runtime::temporal_range::MAX_INPUT_BYTES;

/// Complete original range projection; engineering validity remains separately reviewed.
pub fn execute_local_temporal_range(input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticTemporalRangePort, SemanticTemporalRangeRequest};
    let input: SemanticTemporalRangeRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal_range(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

pub const MAX_TEMPORAL_SNAPSHOT_WIRE_BYTES: usize =
    semantica_runtime::temporal_snapshot::MAX_INPUT_BYTES;

/// Explicit-clock snapshot; missing dates do not establish engineering validity.
pub fn execute_local_temporal_snapshot(
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticTemporalSnapshotPort, SemanticTemporalSnapshotRequest};
    let input: SemanticTemporalSnapshotRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal_snapshot(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

/// Source timeline/revision projection; Project owns engineering interpretation.
pub fn execute_local_temporal_facts(input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticTemporalFactsPort, SemanticTemporalFactsRequest};
    let input: SemanticTemporalFactsRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal_facts(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

/// Source interval math; source/version applicability and adoption stay with Project.
pub fn execute_local_temporal(input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticTemporalPort, SemanticTemporalRequest};
    let input: SemanticTemporalRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .temporal(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

/// Bounded continuous actions; retains explicit failures and their actual state.
pub fn execute_local_datalog_session(
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticDatalogSessionPort, SemanticDatalogSessionRequest};
    let input: SemanticDatalogSessionRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .datalog_session(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

/// Immutable ContextGraph snapshot, imported by the existing Datalog owner.
pub fn execute_local_datalog_graph(input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticDatalogGraphPort, SemanticDatalogGraphRequest};
    let input: SemanticDatalogGraphRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .datalog_graph(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

/// Stateless multi-variable Datalog; old Horn and textual rule entries retain
/// their own languages, result meanings and engineering review obligations.
pub fn execute_local_datalog(input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticDatalogPort, SemanticDatalogRequest};
    let input: SemanticDatalogRequest = serde_json::from_value(input)?;
    semantica_runtime::runtime::RustSemanticRuntime
        .datalog(&input)
        .map_err(|e| anyhow::anyhow!("{}:{}:{}", e.phase, e.code, e.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}

/// Explicit structured-rule entry uses the generated neutral port. The existing
/// unary-Horn `forward_chain` entry above keeps its original contract.
pub fn execute_local_reason(input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{SemanticReasonPort, SemanticReasonRequest};
    use semantica_runtime::runtime::RustSemanticRuntime;

    let input: SemanticReasonRequest = serde_json::from_value(input)?;
    RustSemanticRuntime
        .reason(&input)
        .map_err(|error| anyhow::anyhow!("{}:{}:{}", error.phase, error.code, error.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}
