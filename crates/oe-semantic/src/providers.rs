//! Engineering-facing facade for the unique neutral provider implementation.
//! No credential, provider cache, call ledger or second provenance state.
pub use semantica_runtime::dataset::DatasetSession;
pub use semantica_runtime::embeddings::{
    RustEmbeddingPooling, decode_embedding_response, float32_tensor, pool_embeddings,
    prepare_embedding_request, stack_embedding_results,
};
pub use semantica_runtime::provenance::load_records;
pub use semantica_runtime::providers::{
    RustProviderRuntime, base_structured_prompt, decode_response, failure, json_truthy,
    parse_protocol_json, parse_structured_json, prepare_request, response_usage,
};
pub use semantica_runtime::providers::{factory, litellm};
