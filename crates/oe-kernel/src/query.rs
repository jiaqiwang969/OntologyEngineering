//! Engineering entry delegates local queries through the generated neutral port.
use anyhow::{Context, Result};
use semantica_contracts::{
    SemanticQueryOptionsPortV2, SemanticQueryOptionsPortV3, SemanticQueryOptionsPortV4,
    SemanticQueryOptionsRequestV2, SemanticQueryOptionsRequestV3, SemanticQueryOptionsRequestV4,
    SemanticQueryPort, SemanticQueryRequest,
};
use semantica_runtime::runtime::RustSemanticRuntime;

/// Identity of the neutral contracts actually used by the engineering adapter.
pub fn compiled_semantic_contract_sha256() -> String {
    oe_core::sha256(semantica_contracts::generated_input())
}

/// Preserve the existing CLI result, with exact source identity validated by the
/// shared library. Engineering conclusions continue through the review entry.
pub fn execute_local_query(source: &[u8], query: &str) -> Result<serde_json::Value> {
    let input = SemanticQueryRequest {
        schema: "semantica.query-request/1".into(),
        data: std::str::from_utf8(source)
            .context("rdf_source_utf8_invalid")?
            .into(),
        data_sha256: oe_core::sha256(source),
        rdf_format: "turtle".into(),
        query: query.into(),
    };
    RustSemanticRuntime
        .query(&input)
        .map(|response| response.result)
        .map_err(|error| anyhow::anyhow!("{}:{}:{}", error.phase, error.code, error.message))
}

/// Explicit options2 input stays on the same generated semantic port owner.
pub fn execute_local_query_options_v2(
    input: &SemanticQueryOptionsRequestV2,
) -> Result<serde_json::Value> {
    RustSemanticRuntime
        .query_with_options_v2(input)
        .map(|response| {
            serde_json::to_value(response).expect("generated query response serialization")
        })
        .map_err(|error| anyhow::anyhow!("{}:{}:{}", error.phase, error.code, error.message))
}

/// Dispatch explicit versions on the same generated semantic port/runtime.
/// Unknown schemas never fall back to a weaker context policy.
pub fn execute_local_query_options(input: serde_json::Value) -> Result<serde_json::Value> {
    match input.get("schema").and_then(serde_json::Value::as_str) {
        Some("semantica.query-options-request/2") => {
            execute_local_query_options_v2(&serde_json::from_value(input)?)
        }
        Some("semantica.query-options-request/3") => {
            let input: SemanticQueryOptionsRequestV3 = serde_json::from_value(input)?;
            RustSemanticRuntime
                .query_with_options_v3(&input)
                .map(|response| {
                    serde_json::to_value(response).expect("generated query response serialization")
                })
                .map_err(|error| {
                    anyhow::anyhow!("{}:{}:{}", error.phase, error.code, error.message)
                })
        }
        Some("semantica.query-options-request/4") => {
            let input: SemanticQueryOptionsRequestV4 = serde_json::from_value(input)?;
            RustSemanticRuntime
                .query_with_options_v4(&input)
                .map(|response| {
                    serde_json::to_value(response).expect("generated query response serialization")
                })
                .map_err(|error| {
                    anyhow::anyhow!("{}:{}:{}", error.phase, error.code, error.message)
                })
        }
        _ => anyhow::bail!("semantic_query_options_schema_unsupported"),
    }
}
