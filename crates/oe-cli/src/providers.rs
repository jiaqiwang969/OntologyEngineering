//! Explicit provider execution with Harness-owned shared calls and safe pool.
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use oe_contracts::{
    BaseStructuredGenerationPort, BaseStructuredRequest, ProviderTraceContext,
    SharedModelCallPolicy,
};
use oe_core::{SkillRoot, read_regular_bytes, write_new};
use oe_engineering::harness::{
    RustBaseStructuredHarness, SharedModelCallContext, embed_provider, embed_provider_batch,
    embedding_trace, generate_provider, provider_trace,
};
use oe_model_credentials::{CredentialPool, provider_transport::ProviderHttpTransport};
use semantica_contracts::{
    SemanticEmbeddingAttempt, SemanticEmbeddingBatchRequest, SemanticEmbeddingPoolRequest,
    SemanticEmbeddingRequest, SemanticProviderRequest,
};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

#[derive(Subcommand)]
pub enum ProviderCommand {
    /// Extract finite original typed relations from a current registered native window.
    /// OCR requires a borrowed real factory capability through the library API.
    #[command(name = "typed-extract")]
    TypedExtract {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long, conflicts_with = "custom_openai_endpoint")]
        local_synthetic_endpoint: Option<String>,
        #[arg(long)]
        custom_openai_endpoint: Option<String>,
        #[arg(long, default_value_t = 3)]
        max_calls: u64,
    },
    /// Mutate the one caller-owned dynamic registry and provider instance cache.
    /// Built-in binding configuration is separate from SDK availability.
    Factory {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long, default_value_t = 0)]
        max_calls: u64,
    },
    /// Execute original LiteLLM wrapper through an explicit finite HTTP adapter.
    #[command(name = "litellm")]
    LiteLlm {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long, conflicts_with = "custom_openai_endpoint")]
        local_synthetic_endpoint: Option<String>,
        #[arg(long)]
        custom_openai_endpoint: Option<String>,
        #[arg(long, default_value_t = 1)]
        max_calls: u64,
    },
    /// Execute source-pinned native CPU model inference; no network or Python delegate.
    Native {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        model_source_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Existing owner pool is used only as an input/result secrecy guard.
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long, default_value_t = 1)]
        max_calls: u64,
    },
    /// Run original BaseProvider structured retries using one shared call owner.
    BaseStructured {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long, conflicts_with = "custom_openai_endpoint")]
        local_synthetic_endpoint: Option<String>,
        #[arg(long)]
        custom_openai_endpoint: Option<String>,
        #[arg(long, default_value_t = 3)]
        max_calls: u64,
    },
    /// Run original OpenAIStore single or sequential batch embedding protocol.
    Embed {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long, conflicts_with = "custom_openai_endpoint")]
        local_synthetic_endpoint: Option<String>,
        #[arg(long)]
        custom_openai_endpoint: Option<String>,
        #[arg(long, default_value_t = 1)]
        max_calls: u64,
    },
    /// Pool supplied finite 2D vectors; this command does not run a model.
    Pool {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Run OpenAI/Groq generation; synthetic loopback execution stays explicit.
    Generate {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long, conflicts_with = "custom_openai_endpoint")]
        local_synthetic_endpoint: Option<String>,
        #[arg(long)]
        custom_openai_endpoint: Option<String>,
        #[arg(long, default_value_t = 1)]
        max_calls: u64,
        #[arg(long, default_value_t = 1)]
        max_attempts: usize,
    },
}
pub fn run(root: &SkillRoot, command: ProviderCommand) -> Result<(Value, u8)> {
    match command {
        ProviderCommand::TypedExtract {
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
        } => run_typed_extract8(
            root,
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
        ),
        ProviderCommand::Factory {
            input,
            output,
            credential_file,
            max_calls,
        } => run_factory(root, input, output, credential_file, max_calls),
        ProviderCommand::LiteLlm {
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
        } => run_litellm(
            root,
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
        ),
        ProviderCommand::Native {
            input,
            model_source_root,
            output,
            credential_file,
            max_calls,
        } => run_native(
            root,
            input,
            model_source_root,
            output,
            credential_file,
            max_calls,
        ),
        ProviderCommand::BaseStructured {
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
        } => run_base(
            root,
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
        ),
        ProviderCommand::Embed {
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
        } => run_embed(
            root,
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
        ),
        ProviderCommand::Pool { input, output } => {
            let binding = crate::v2::require_execution(root, "embedding_pooling")?;
            let raw = read_regular_bytes(&input, 2 * 1024 * 1024)?;
            let request: SemanticEmbeddingPoolRequest =
                serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
            let result = oe_semantic::providers::pool_embeddings(&request)
                .map_err(|e| anyhow::anyhow!(e.code))?;
            let output = root.new_private_dir(&output)?;
            let value = json!({"schema":"oev2.embedding-pool/1","result":result,"execution_kind":"native_arithmetic",
                "fact_admission":false,"engineering_acceptance":"not_determined","execution_binding":binding});
            write_new(&output.join("embedding-pool.json"), &value)?;
            Ok((value, 0))
        }
        ProviderCommand::Generate {
            input,
            output,
            credential_file,
            local_synthetic_endpoint,
            custom_openai_endpoint,
            max_calls,
            max_attempts,
        } => {
            let binding = crate::v2::require_execution(root, "provider_generation")?;
            let raw = read_regular_bytes(&input, 2 * 1024 * 1024)?;
            let request: SemanticProviderRequest =
                serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
            // Explicit new grammar, same 0600/owner/no-symlink pool and lease.
            let pool = CredentialPool::read_single_http_bearer(&credential_file)?;
            ensure!(!pool.contains_in(&raw), "credential_material_in_request");
            let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls })
                .map_err(|_| anyhow::anyhow!("shared_model_call_policy_invalid"))?;
            let transport = if let Some(endpoint) = local_synthetic_endpoint {
                ProviderHttpTransport::local_synthetic(
                    &pool,
                    &request.provider,
                    &endpoint,
                    request.custom_endpoint,
                    Duration::from_secs(10),
                )
            } else if let Some(endpoint) = custom_openai_endpoint {
                ensure!(
                    request.provider == "openai" && request.custom_endpoint,
                    "provider_transport_scope_invalid"
                );
                ProviderHttpTransport::custom_openai(&pool, &endpoint, Duration::from_secs(60))
            } else {
                ensure!(!request.custom_endpoint, "provider_transport_scope_invalid");
                ProviderHttpTransport::standard(&pool, &request.provider, Duration::from_secs(60))
            }
            .map_err(|_| anyhow::anyhow!("provider_transport_scope_invalid"))?;
            // Preflight output before sending; failures retain only safe metadata.
            let output = root.new_private_dir(&output)?;
            let started = chrono_now();
            match generate_provider(&calls, &request, &transport, &transport, max_attempts) {
                Ok(result) => {
                    let activity = format!(
                        "urn:semantica:provider:call:{}",
                        oe_core::sha256(output.as_os_str().to_string_lossy().as_bytes())
                    );
                    let trace = ProviderTraceContext {
                        graph_id: format!("{activity}:graph"),
                        agent_id: "urn:semantica:software:oev2-provider".into(),
                        record_id: format!("{activity}:record"),
                        output_id: format!("{activity}:output"),
                        activity_id: activity,
                        started_at: Some(started),
                        ended_at: Some(chrono_now()),
                    };
                    let mut session = oe_semantic::providers::DatasetSession::new();
                    let provenance =
                        provider_trace(&mut session, &request, &result, &trace, &transport)?;
                    let snapshot = session
                        .snapshot()
                        .context("provider_trace_snapshot_failed")?;
                    let value = json!({"schema":"oev2.provider-call/1","result":result,
                        "execution_kind":result.transport_kind,"engineering_acceptance":"not_determined",
                        "fact_admission":false,"cost":{"status":"unknown","amount":null,"currency":null},"calls":calls.snapshot(),"provenance":provenance,
                        "provenance_snapshot":snapshot,"execution_binding":binding});
                    let safe = serde_json::to_vec(&value)?;
                    ensure!(!pool.contains_in(&safe), "credential_material_in_response");
                    write_new(&output.join("provider-call.json"), &value)?;
                    Ok((value, 0))
                }
                Err(error) => {
                    let value = json!({"schema":"oev2.provider-call/1","execution":"failed","error":error,
                        "calls":calls.snapshot(),"engineering_acceptance":"not_determined","fact_admission":false,
                        "cost":{"status":"unknown","amount":null,"currency":null},"execution_binding":binding});
                    write_new(&output.join("provider-call.json"), &value)?;
                    Ok((value, 2))
                }
            }
        }
    }
}

fn run_typed_extract8(
    root: &SkillRoot,
    input: PathBuf,
    output: PathBuf,
    credential_file: PathBuf,
    local: Option<String>,
    custom: Option<String>,
    max_calls: u64,
) -> Result<(Value, u8)> {
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    let binding = crate::v2::require_execution(root, "source_bound_typed_extraction8")?;
    let path = root.expand(&input)?;
    let raw = read_regular_bytes(&path, 2 * 1024 * 1024)?;
    let request = oe_knowledge::typed_source8::parse_input8(&oe_core::json::strict_json(&raw)?)?;
    // This CLI never creates or reconstructs an OCR capability from input JSON.
    let source = oe_knowledge::typed_source8::prepare_native8(root, &request, deadline)?;
    let pool = CredentialPool::read_single_http_bearer(&credential_file)?;
    ensure!(!pool.contains_in(&raw), "credential_material_in_request");
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls })
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let remaining = deadline
        .checked_duration_since(std::time::Instant::now())
        .context("typed_extraction8_deadline")?;
    let timeout = remaining.min(Duration::from_secs(if local.is_some() { 10 } else { 30 }));
    let transport = if let Some(endpoint) = local.as_deref() {
        ProviderHttpTransport::local_synthetic(
            &pool,
            &request.generation.provider,
            endpoint,
            request.generation.custom_endpoint,
            timeout,
        )
    } else if let Some(endpoint) = custom.as_deref() {
        ensure!(
            request.generation.provider == "openai" && request.generation.custom_endpoint,
            "provider_transport_scope_invalid"
        );
        ProviderHttpTransport::custom_openai(&pool, endpoint, timeout)
    } else {
        ensure!(
            !request.generation.custom_endpoint,
            "provider_transport_scope_invalid"
        );
        ProviderHttpTransport::standard(&pool, &request.generation.provider, timeout)
    }
    .map_err(|e| anyhow::anyhow!(e.code))?;
    let mut receipt =
        oe_engineering::typed_extraction8::run_source8(&source, &calls, &transport, &transport)?;
    if oe_core::sha256(read_regular_bytes(&path, 2 * 1024 * 1024)?) != oe_core::sha256(&raw) {
        receipt.execution = "failed".into();
        receipt.source_current = false;
        receipt.outcome.failure = Some(semantica_runtime::providers::failure(
            "typed_extraction8_input_file_changed",
            "guard",
        ));
    }
    let code = if receipt.execution == "completed" {
        0
    } else {
        2
    };
    let value = json!({"schema":"oev2.typed-extraction-cli/8","receipt":receipt,"execution_binding":binding,
        "input_sha256":oe_core::sha256(&raw),"fact_admission":false,"engineering_acceptance":"not_determined",
        "external_model_quality":"unknown","cost":{"status":"unknown","amount":null,"currency":null}});
    ensure!(
        !pool.contains_in(&serde_json::to_vec(&value)?),
        "credential_material_in_response"
    );
    let directory = root.new_private_dir(&output)?;
    write_new(&directory.join("typed-extraction8.json"), &value)?;
    Ok((value, code))
}

pub fn run_factory(
    root: &SkillRoot,
    input: PathBuf,
    output: PathBuf,
    credential_file: PathBuf,
    max_calls: u64,
) -> Result<(Value, u8)> {
    let binding = crate::v2::require_execution(root, "provider_factory")?;
    let raw = read_regular_bytes(&input, 2 * 1024 * 1024)?;
    let plan: semantica_contracts::SemanticProviderFactoryPlan =
        serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    ensure!(
        plan.validate().is_ok() && plan.commands.len() <= 4096,
        "factory_plan_invalid"
    );
    let pool = CredentialPool::read_single_http_bearer(&credential_file)?;
    ensure!(!pool.contains_in(&raw), "credential_material_in_request");
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls })
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let constructors = oe_engineering::harness::RustConfiguredProviderConstructors;
    let output = root.new_private_dir(&output)?;
    let mut observations = vec![];
    let mut failed = false;
    for (index, request) in plan.commands.iter().enumerate() {
        match calls.factory_apply(request, &constructors) {
            Ok(result) => observations.push(json!({"index":index,"result":result})),
            Err(failure) => {
                failed = true;
                observations.push(json!({"index":index,"failure":failure}));
            }
        }
    }
    ensure!(
        oe_core::sha256(read_regular_bytes(&input, 2 * 1024 * 1024)?) == oe_core::sha256(&raw),
        "factory_input_source_changed"
    );
    let value = json!({"schema":"oev2.provider-factory/1","observations":observations,"calls":calls.snapshot(),
        "construction_profile":"caller_configured_binding","sdk_initialization":"not_observed",
        "network_dispatches":0,"input_source_sha256":oe_core::sha256(&raw),"execution_binding":binding,
        "fact_admission":false,"engineering_acceptance":"not_determined"});
    ensure!(
        !pool.contains_in(&serde_json::to_vec(&value)?),
        "credential_material_in_response"
    );
    write_new(&output.join("provider-factory.json"), &value)?;
    Ok((value, if failed { 2 } else { 0 }))
}

pub fn run_litellm(
    root: &SkillRoot,
    input: PathBuf,
    output: PathBuf,
    credential_file: PathBuf,
    local: Option<String>,
    custom: Option<String>,
    max_calls: u64,
) -> Result<(Value, u8)> {
    let binding = crate::v2::require_execution(root, "litellm_generation")?;
    let raw = read_regular_bytes(&input, 2 * 1024 * 1024)?;
    let request: semantica_contracts::SemanticLiteLlmRequest =
        serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let pool = CredentialPool::read_single_http_bearer(&credential_file)?;
    ensure!(!pool.contains_in(&raw), "credential_material_in_request");
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls })
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let completion = oe_semantic::providers::litellm::prepare_completion_request(&request)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let wire = oe_semantic::providers::litellm::prepare_protocol_request(&request, &completion)
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let transport = execution_transport(
        &pool,
        &wire.provider,
        wire.custom_endpoint,
        false,
        local.as_deref(),
        custom.as_deref(),
    )?;
    let output = root.new_private_dir(&output)?;
    let started = chrono_now();
    let mut session = oe_semantic::providers::DatasetSession::new();
    let (response, code) =
        match oe_engineering::harness::generate_litellm(&calls, &request, &transport, &transport) {
            Ok(result) => {
                oe_engineering::harness::litellm_trace(
                    &mut session,
                    &request,
                    &result,
                    &trace_context(&output, 0, &started),
                    &transport,
                )?;
                (json!({"result":result}), 0)
            }
            Err(failure) => (json!({"failure":failure,"execution":"failed"}), 2),
        };
    ensure!(
        oe_core::sha256(read_regular_bytes(&input, 2 * 1024 * 1024)?) == oe_core::sha256(&raw),
        "litellm_input_source_changed"
    );
    let value = json!({"schema":"oev2.litellm-call/1","wrapper_profile":"semantica.litellm-wrapper/1",
        "completion_request":completion,"actual_wire_request":wire,"response":response,"calls":calls.snapshot(),
        "input_source_sha256":oe_core::sha256(&raw),"provenance_snapshot":session.snapshot()?,"execution_binding":binding,
        "third_party_sdk_dispatch":"unknown","fact_admission":false,"engineering_acceptance":"not_determined",
        "cost":{"status":"unknown","amount":null,"currency":null}});
    ensure!(
        !pool.contains_in(&serde_json::to_vec(&value)?),
        "credential_material_in_response"
    );
    write_new(&output.join("litellm-call.json"), &value)?;
    Ok((value, code))
}

pub fn run_native(
    root: &SkillRoot,
    input: PathBuf,
    model_source_root: PathBuf,
    output: PathBuf,
    credential_file: PathBuf,
    max_calls: u64,
) -> Result<(Value, u8)> {
    let binding = crate::v2::require_execution(root, "native_model_inference")?;
    let raw = read_regular_bytes(&input, 2 * 1024 * 1024)?;
    let request: semantica_contracts::SemanticNativeRequest =
        serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let pool = CredentialPool::read_single_http_bearer(&credential_file)?;
    ensure!(!pool.contains_in(&raw), "credential_material_in_request");
    let guard = oe_model_credentials::provider_transport::NativeCredentialGuard::new(&pool);
    let source = oe_semantic::native_models::NativeModelSources::new(&model_source_root)?;
    let runtime = oe_semantic::native_models::RustNativeModelRuntime::new(&source);
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls })
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let output = root.new_private_dir(&output)?;
    let started = chrono_now();
    let result = oe_engineering::harness::execute_native_model(&calls, &request, &runtime, &guard);
    let mut session = oe_semantic::providers::DatasetSession::new();
    let (response, code) = match result {
        Ok(result) => {
            oe_engineering::harness::native_model_trace(
                &mut session,
                &request,
                &result,
                &trace_context(&output, 0, &started),
                &guard,
            )?;
            (json!({"result":result}), 0)
        }
        Err(failure) => (json!({"failure":failure,"execution":"failed"}), 2),
    };
    ensure!(
        oe_core::sha256(read_regular_bytes(&input, 2 * 1024 * 1024)?) == oe_core::sha256(&raw),
        "native_input_source_changed"
    );
    let value = json!({"schema":"oev2.native-model-call/1","response":response,"calls":calls.snapshot(),
        "input_source_sha256":oe_core::sha256(&raw),"provenance_snapshot":session.snapshot()?,"execution_binding":binding,
        "execution_kind":"native_cpu","network_dispatches":0,"python_production_delegate":false,
        "fact_admission":false,"engineering_acceptance":"not_determined","cost":{"status":"unknown","amount":null,"currency":null}});
    ensure!(
        !pool.contains_in(&serde_json::to_vec(&value)?),
        "credential_material_in_response"
    );
    write_new(&output.join("native-model-call.json"), &value)?;
    Ok((value, code))
}

fn execution_transport<'a>(
    pool: &'a CredentialPool,
    provider: &str,
    custom: bool,
    embedding: bool,
    local: Option<&str>,
    custom_url: Option<&str>,
) -> Result<ProviderHttpTransport<'a>> {
    let transport = if let Some(endpoint) = local {
        if embedding {
            ProviderHttpTransport::local_synthetic_embedding(
                pool,
                endpoint,
                custom,
                Duration::from_secs(10),
            )
        } else {
            ProviderHttpTransport::local_synthetic(
                pool,
                provider,
                endpoint,
                custom,
                Duration::from_secs(10),
            )
        }
    } else if let Some(endpoint) = custom_url {
        ensure!(
            provider == "openai" && custom,
            "provider_transport_scope_invalid"
        );
        if embedding {
            ProviderHttpTransport::custom_openai_embedding(pool, endpoint, Duration::from_secs(60))
        } else {
            ProviderHttpTransport::custom_openai(pool, endpoint, Duration::from_secs(60))
        }
    } else {
        ensure!(!custom, "provider_transport_scope_invalid");
        if embedding {
            ProviderHttpTransport::standard_embedding(pool, Duration::from_secs(60))
        } else {
            ProviderHttpTransport::standard(pool, provider, Duration::from_secs(60))
        }
    };
    transport.map_err(|_| anyhow::anyhow!("provider_transport_scope_invalid"))
}

fn trace_context(output: &std::path::Path, index: usize, started: &str) -> ProviderTraceContext {
    let root = format!(
        "urn:semantica:provider:call:{}",
        oe_core::sha256(output.as_os_str().to_string_lossy().as_bytes())
    );
    let activity = format!("{root}:attempt:{index}");
    ProviderTraceContext {
        graph_id: format!("{root}:graph"),
        agent_id: "urn:semantica:software:oev2-provider".into(),
        record_id: format!("{activity}:record"),
        output_id: format!("{activity}:output"),
        activity_id: activity,
        started_at: Some(started.into()),
        ended_at: Some(chrono_now()),
    }
}

pub fn run_base(
    root: &SkillRoot,
    input: PathBuf,
    output: PathBuf,
    credential_file: PathBuf,
    local: Option<String>,
    custom: Option<String>,
    max_calls: u64,
) -> Result<(Value, u8)> {
    let binding = crate::v2::require_execution(root, "base_structured_generation")?;
    let raw = read_regular_bytes(&input, 2 * 1024 * 1024)?;
    let request: BaseStructuredRequest = serde_json::from_value(oe_core::json::strict_json(&raw)?)?;
    let pool = CredentialPool::read_single_http_bearer(&credential_file)?;
    ensure!(!pool.contains_in(&raw), "credential_material_in_request");
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls })
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let transport = execution_transport(
        &pool,
        &request.generation.provider,
        request.generation.custom_endpoint,
        false,
        local.as_deref(),
        custom.as_deref(),
    )?;
    let output = root.new_private_dir(&output)?;
    let started = chrono_now();
    let result = RustBaseStructuredHarness::new(&calls, &transport, &transport)
        .generate_structured(&request)
        .map_err(|e| anyhow::anyhow!(e.failure.code))?;
    let mut session = oe_semantic::providers::DatasetSession::new();
    for (index, attempt) in result.attempts.iter().enumerate() {
        if let Some(response) = &attempt.result {
            provider_trace(
                &mut session,
                &attempt.request,
                response,
                &trace_context(&output, index, &started),
                &transport,
            )?;
        }
    }
    let code = if result.execution == "completed" {
        0
    } else {
        2
    };
    let value = json!({"schema":"oev2.base-structured-call/1","result":result,"calls":calls.snapshot(),
        "provenance_snapshot":session.snapshot()?,"execution_binding":binding,"fact_admission":false,
        "engineering_acceptance":"not_determined","cost":{"status":"unknown","amount":null,"currency":null}});
    ensure!(
        !pool.contains_in(&serde_json::to_vec(&value)?),
        "credential_material_in_response"
    );
    write_new(&output.join("base-structured-call.json"), &value)?;
    Ok((value, code))
}

pub fn run_embed(
    root: &SkillRoot,
    input: PathBuf,
    output: PathBuf,
    credential_file: PathBuf,
    local: Option<String>,
    custom: Option<String>,
    max_calls: u64,
) -> Result<(Value, u8)> {
    let binding = crate::v2::require_execution(root, "embedding_generation")?;
    let raw = read_regular_bytes(&input, 2 * 1024 * 1024)?;
    let decoded = oe_core::json::strict_json(&raw)?;
    let is_batch = decoded["schema"] == "semantica.embedding-batch-request/1";
    let requests = if is_batch {
        serde_json::from_value::<SemanticEmbeddingBatchRequest>(decoded.clone())?.requests
    } else {
        vec![serde_json::from_value::<SemanticEmbeddingRequest>(
            decoded.clone(),
        )?]
    };
    let custom_endpoint = requests.first().is_some_and(|r| r.custom_endpoint);
    ensure!(
        requests
            .iter()
            .all(|r| r.provider == "openai" && r.custom_endpoint == custom_endpoint),
        "provider_transport_scope_invalid"
    );
    let pool = CredentialPool::read_single_http_bearer(&credential_file)?;
    ensure!(!pool.contains_in(&raw), "credential_material_in_request");
    let calls = SharedModelCallContext::new(SharedModelCallPolicy { max_calls })
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let transport = execution_transport(
        &pool,
        "openai",
        custom_endpoint,
        true,
        local.as_deref(),
        custom.as_deref(),
    )?;
    let output = root.new_private_dir(&output)?;
    let started = chrono_now();
    let mut responses = vec![];
    let mut value = if is_batch {
        let input: SemanticEmbeddingBatchRequest = serde_json::from_value(decoded)?;
        let batch = embed_provider_batch(&calls, &input, &transport, &transport)
            .map_err(|e| anyhow::anyhow!(e.code))?;
        responses = batch.results.clone();
        json!({"schema":"oev2.embedding-call/1","execution":batch.execution,"batch":batch})
    } else {
        match embed_provider(&calls, &requests[0], &transport, &transport) {
            Ok(result) => {
                responses.push(result.clone());
                let attempt = SemanticEmbeddingAttempt {
                    index: 0,
                    request: requests[0].clone(),
                    result: Some(result.clone()),
                    failure: None,
                    reserved: calls.snapshot().attempts > 0,
                };
                json!({"schema":"oev2.embedding-call/1","execution":"completed","result":result,"attempt":attempt})
            }
            Err(error) => {
                let attempt = SemanticEmbeddingAttempt {
                    index: 0,
                    request: requests[0].clone(),
                    result: None,
                    failure: Some(error.clone()),
                    reserved: calls.snapshot().attempts > 0,
                };
                json!({"schema":"oev2.embedding-call/1","execution":"failed","error":error,"attempt":attempt})
            }
        }
    };
    let mut session = oe_semantic::providers::DatasetSession::new();
    for (index, response) in responses.iter().enumerate() {
        embedding_trace(
            &mut session,
            &requests[index],
            response,
            &trace_context(&output, index, &started),
            &transport,
        )?;
    }
    let code = if value["execution"] == "completed" {
        0
    } else {
        2
    };
    value["calls"] = serde_json::to_value(calls.snapshot())?;
    value["input_source_sha256"] = json!(oe_core::sha256(&raw));
    value["provenance_snapshot"] = serde_json::to_value(session.snapshot()?)?;
    value["execution_binding"] = binding;
    value["fact_admission"] = json!(false);
    value["engineering_acceptance"] = json!("not_determined");
    value["cost"] = json!({"status":"unknown","amount":null,"currency":null});
    ensure!(
        !pool.contains_in(&serde_json::to_vec(&value)?),
        "credential_material_in_response"
    );
    write_new(&output.join("embedding-call.json"), &value)?;
    Ok((value, code))
}
fn chrono_now() -> String {
    // Existing process clock is sufficient; no secret/environment model lookup.
    oe_engineering::harness::provider_call_time()
}
