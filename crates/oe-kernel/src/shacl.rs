//! Engineering facade delegates to the unique generic Rust implementation.
pub use semantica_runtime::shacl::*;

/// Transport allowance covers two original 16 MiB inputs plus the explicit
/// selector and JSON escaping. It does not raise the RDF evaluation limits.
pub use semantica_runtime::runtime::MAX_NAMED_GRAPH_VALIDATION_WIRE_BYTES;

/// Explicit selected-graph input uses the generated neutral port. Existing
/// validate/validate_dataset entry points retain their named-graph refusal.
pub fn execute_local_named_graph_validation(
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    use semantica_contracts::{
        SemanticNamedGraphValidationPort, SemanticNamedGraphValidationRequest,
    };
    use semantica_runtime::runtime::RustSemanticRuntime;

    let input: SemanticNamedGraphValidationRequest = serde_json::from_value(input)?;
    RustSemanticRuntime
        .validate_named_graph(&input)
        .map_err(|error| anyhow::anyhow!("{}:{}:{}", error.phase, error.code, error.message))
        .and_then(|result| Ok(serde_json::to_value(result)?))
}
