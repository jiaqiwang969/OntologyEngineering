//! Real CPU model inference. Source transport and execution authority are injected.
//! No model download, Python SDK, environment credential or global model registry.
use crate::{
    books::{SourceProvider, relative},
    providers::{failure, parse_protocol_json},
};
use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::{
    generation::{LogitsProcessor, Sampling},
    models::{bert, llama},
};
use semantica_contracts::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

const MAX_ASSET: u64 = 2 * 1024 * 1024 * 1024;
pub const NATIVE_PROFILE: &str = "semantica.native-candle-f32-cpu/1";
type NativeResult<T> = Result<T, SemanticProviderFailure>;
fn reject(code: &str) -> SemanticProviderFailure {
    failure(code, "native")
}
fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
fn encoded<T: serde::Serialize>(v: &T) -> NativeResult<Vec<u8>> {
    serde_json::to_vec(v).map_err(|_| reject("native_input_invalid"))
}
fn parsed(bytes: &[u8]) -> NativeResult<Value> {
    let text = std::str::from_utf8(bytes).map_err(|_| reject("native_metadata_invalid"))?;
    parse_protocol_json(text).map_err(|_| reject("native_metadata_invalid"))
}
fn compute<T>(v: candle_core::Result<T>) -> NativeResult<T> {
    v.map_err(|_| reject("native_tensor_or_model_failure"))
}
fn integer(v: &Value, key: &str) -> NativeResult<usize> {
    let n = v[key]
        .as_u64()
        .ok_or_else(|| reject("native_configuration_invalid"))?;
    usize::try_from(n).map_err(|_| reject("native_configuration_invalid"))
}
pub fn validate_native_request(input: &SemanticNativeRequest) -> NativeResult<()> {
    if input.validate().is_err()
        || input.model.validate().is_err()
        || input.model.model_id.is_empty()
        || input.model.revision.is_empty()
        || input.texts.len() > 64
        || !input.options.is_object()
        || input.texts.iter().map(String::len).sum::<usize>() > 2 * 1024 * 1024
    {
        return Err(reject("native_input_invalid"));
    }
    validate_model_assets(&input.model)?;
    if input.operation == "embed"
        && input.interface_profile == "text_embedder"
        && (input.options.as_object().is_some_and(|v| {
            v.keys().any(|k| {
                !["batch_size", "device", "precision", "output_value"].contains(&k.as_str())
            })
        }) || input.options.get("device").is_some_and(|v| v != "cpu")
            || input
                .options
                .get("precision")
                .is_some_and(|v| v != "float32")
            || input
                .options
                .get("output_value")
                .is_some_and(|v| v != "sentence_embedding"))
    {
        return Err(reject("native_embedding_options_unsupported"));
    }
    match (
        input.operation.as_str(),
        input.interface_profile.as_str(),
        input.model.architecture.as_str(),
    ) {
        ("embed", "text_embedder" | "bge_store", "bert") => {
            if !input.batch
                && (input.texts.len() != 1
                    || (input.interface_profile == "text_embedder"
                        && input.texts[0].trim_matches(python_whitespace).is_empty()))
            {
                return Err(reject("native_embedding_text_invalid"));
            }
        }
        ("generate" | "generate_structured", "huggingface_llm", "llama")
        | ("causal_logits" | "argmax_validation", "native_validation", "llama") => {
            if input.batch || input.texts.len() != 1 {
                return Err(reject("native_causal_batch_not_supported"));
            }
        }
        _ => return Err(reject("native_profile_architecture_mismatch")),
    }
    Ok(())
}

fn validate_model_assets(spec: &SemanticNativeModelSpec) -> NativeResult<()> {
    if spec.validate().is_err()
        || spec.device != "cpu"
        || !["bert", "llama"].contains(&spec.architecture.as_str())
        || spec.assets.len() > 8
        || spec.assets.len() < 4
    {
        return Err(reject("native_model_asset_contract_invalid"));
    }
    let mut roles = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for asset in &spec.assets {
        if asset.validate().is_err()
            || relative(&asset.source_id).is_err()
            || ![
                "config",
                "weights",
                "tokenizer",
                "tokenizer_config",
                "modules",
                "pooling",
                "sentence_config",
                "generation_config",
            ]
            .contains(&asset.role.as_str())
            || asset.byte_count == 0
            || asset.byte_count > MAX_ASSET
            || !roles.insert(asset.role.as_str())
            || !ids.insert(&asset.source_id)
            || (asset.role != "weights" && asset.byte_count > 8 * 1024 * 1024)
        {
            return Err(reject("native_model_asset_contract_invalid"));
        }
    }
    let mandatory = if spec.architecture == "bert" {
        &[
            "config",
            "weights",
            "tokenizer",
            "tokenizer_config",
            "modules",
            "pooling",
            "sentence_config",
        ][..]
    } else {
        &[
            "config",
            "weights",
            "tokenizer",
            "tokenizer_config",
            "generation_config",
        ][..]
    };
    if mandatory.iter().any(|role| !roles.contains(role)) {
        return Err(reject("native_required_asset_missing"));
    }
    Ok(())
}
/// SHA is exact source identity. The caller retains the authority to choose it.
/// Validate the entire source contract before invoking any source transport.
pub fn verify_model_assets(
    provider: &dyn SourceProvider,
    spec: &SemanticNativeModelSpec,
) -> NativeResult<BTreeMap<String, Vec<u8>>> {
    validate_model_assets(spec)?;
    let mut out = BTreeMap::new();
    for asset in &spec.assets {
        let raw = provider
            .read(&asset.source_id)
            .map_err(|_| failure("native_asset_unavailable", "source"))?;
        if raw.len() as u64 != asset.byte_count || digest(&raw) != asset.sha256 {
            return Err(failure("native_asset_identity_mismatch", "source"));
        }
        out.insert(asset.role.clone(), raw);
    }
    Ok(out)
}
fn asset<'a>(assets: &'a BTreeMap<String, Vec<u8>>, role: &str) -> NativeResult<&'a [u8]> {
    assets
        .get(role)
        .map(Vec::as_slice)
        .ok_or_else(|| reject("native_required_asset_missing"))
}
pub struct RustNativeModelRuntime<'a> {
    source: &'a dyn SourceProvider,
}
impl<'a> RustNativeModelRuntime<'a> {
    pub fn new(source: &'a dyn SourceProvider) -> Self {
        Self { source }
    }
}
impl SemanticNativeInferencePort for RustNativeModelRuntime<'_> {
    fn execute(&self, input: &SemanticNativeRequest) -> NativeResult<SemanticNativeResult> {
        execute_native(self.source, input)
    }
}

pub fn execute_native(
    provider: &dyn SourceProvider,
    input: &SemanticNativeRequest,
) -> NativeResult<SemanticNativeResult> {
    validate_native_request(input)?;
    let mut result = SemanticNativeResult {
        schema: "semantica.native-result/1".into(),
        execution: "completed".into(),
        profile: NATIVE_PROFILE.into(),
        request_sha256: digest(encoded(input)?),
        model_sha256: digest(encoded(&input.model)?),
        parameters: json!({}),
        token_inputs: json!([]),
        output: Value::Null,
        usage: SemanticProviderUsage {
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
        },
        usage_status: "unknown".into(),
        execution_identity: "candle-0.9.1/tokenizers-0.21.4/cpu-f32/1".into(),
    };
    if input.operation == "embed" && input.batch && input.texts.is_empty() {
        result.output = json!({"dtype":"float64","shape":[0],"bits":[]});
        result.parameters = json!({"inference_performed":false,"reason":"original_empty_batch"});
        return Ok(result);
    }
    let mut assets = verify_model_assets(provider, &input.model)?;
    let config = parsed(asset(&assets, "config")?)?;
    if config["model_type"] != input.model.architecture {
        return Err(reject("native_model_architecture_mismatch"));
    }
    if input.model.architecture == "bert" {
        execute_bert(input, &mut assets, &config, &mut result)?;
    } else {
        execute_llama(input, &mut assets, &config, &mut result)?;
    }
    // No successful receipt is returned if any source changed after the forward.
    drop(verify_model_assets(provider, &input.model)?);
    Ok(result)
}
fn tensor_output(tensor: &Tensor) -> NativeResult<Value> {
    let values = compute(tensor.flatten_all())?;
    let values = compute(values.to_vec1::<f32>())?;
    if values.iter().any(|v| !v.is_finite()) {
        return Err(reject("native_nonfinite_output"));
    }
    Ok(
        json!({"dtype":"float32","shape":tensor.dims(),"bits":values.iter().map(|v|u64::from(v.to_bits())).collect::<Vec<_>>()}),
    )
}
fn normalize(tensor: Tensor) -> NativeResult<Tensor> {
    let norms = compute(compute(compute(tensor.sqr())?.sum_keepdim(1))?.sqrt())?;
    compute(tensor.broadcast_div(&compute(norms.clamp(1e-12, f32::MAX as f64))?))
}

fn execute_bert(
    input: &SemanticNativeRequest,
    assets: &mut BTreeMap<String, Vec<u8>>,
    config: &Value,
    result: &mut SemanticNativeResult,
) -> NativeResult<()> {
    let hidden = integer(config, "hidden_size")?;
    let heads = integer(config, "num_attention_heads")?;
    let layers = integer(config, "num_hidden_layers")?;
    let positions = integer(config, "max_position_embeddings")?;
    if hidden == 0
        || hidden > 2048
        || heads == 0
        || hidden % heads != 0
        || layers == 0
        || layers > 64
        || positions < 2
        || positions > 8192
        || integer(config, "intermediate_size")? > 16384
        || integer(config, "intermediate_size")? == 0
        || integer(config, "vocab_size")? < 2
        || integer(config, "vocab_size")? > 100000
        || integer(config, "type_vocab_size")? == 0
        || integer(config, "type_vocab_size")? > 64
        || config["position_embedding_type"] != "absolute"
    {
        return Err(reject("native_bert_configuration_unsupported"));
    }
    let modules = parsed(asset(assets, "modules")?)?;
    let modules = modules
        .as_array()
        .ok_or_else(|| reject("native_pipeline_invalid"))?;
    if !(modules.len() == 2 || modules.len() == 3)
        || modules[0]["type"] != "sentence_transformers.models.Transformer"
        || modules[1]["type"] != "sentence_transformers.models.Pooling"
        || (modules.len() == 3 && modules[2]["type"] != "sentence_transformers.models.Normalize")
    {
        return Err(reject("native_pipeline_unsupported"));
    }
    let intrinsic_normalize = modules.len() == 3;
    let pool = parsed(asset(assets, "pooling")?)?;
    let cls = pool["pooling_mode_cls_token"].as_bool() == Some(true);
    let mean = pool["pooling_mode_mean_tokens"].as_bool() == Some(true);
    if cls == mean
        || integer(&pool, "word_embedding_dimension")? != hidden
        || [
            "pooling_mode_max_tokens",
            "pooling_mode_mean_sqrt_len_tokens",
            "pooling_mode_weightedmean_tokens",
            "pooling_mode_lasttoken",
        ]
        .iter()
        .any(|key| pool[*key].as_bool() == Some(true))
    {
        return Err(reject("native_pooling_unsupported"));
    }
    let sentence = parsed(asset(assets, "sentence_config")?)?;
    let limit = integer(&sentence, "max_seq_length")?;
    if limit < 2 || limit > positions {
        return Err(reject("native_embedding_options_unsupported"));
    }
    let batch_size = if input.interface_profile == "bge_store" {
        32
    } else {
        match input.options.get("batch_size") {
            None => 32,
            Some(v) => v
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| *n > 0 && *n <= 64)
                .ok_or_else(|| reject("native_batch_size_invalid"))?,
        }
    };
    let token_config = parsed(asset(assets, "tokenizer_config")?)?;
    let pad_token = token_config["pad_token"]
        .as_str()
        .ok_or_else(|| reject("native_pad_token_missing"))?;
    let mut tokenizer = Tokenizer::from_bytes(asset(assets, "tokenizer")?)
        .map_err(|_| reject("native_tokenizer_invalid"))?;
    let pad_id = tokenizer
        .token_to_id(pad_token)
        .ok_or_else(|| reject("native_pad_token_missing"))?;
    if u64::from(pad_id) != config["pad_token_id"].as_u64().unwrap_or(u64::MAX) {
        return Err(reject("native_pad_token_mismatch"));
    }
    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::BatchLongest,
        pad_id,
        pad_token: pad_token.into(),
        ..Default::default()
    }));
    tokenizer
        .with_truncation(Some(TruncationParams {
            max_length: limit,
            ..Default::default()
        }))
        .map_err(|_| reject("native_tokenizer_limit_invalid"))?;
    let cfg: bert::Config = serde_json::from_value(config.clone())
        .map_err(|_| reject("native_bert_configuration_invalid"))?;
    let device = Device::Cpu;
    let weights = assets
        .remove("weights")
        .ok_or_else(|| reject("native_required_asset_missing"))?;
    let vb = compute(VarBuilder::from_buffered_safetensors(
        weights,
        DType::F32,
        &device,
    ))?;
    let model = compute(bert::BertModel::load(vb, &cfg))?;
    let mut outputs = vec![];
    let mut input_ids = vec![];
    let mut masks = vec![];
    let mut type_ids = vec![];
    let mut input_tokens = 0u64;
    for chunk in input.texts.chunks(batch_size) {
        let encoded = tokenizer
            .encode_batch(chunk.to_vec(), true)
            .map_err(|_| reject("native_tokenizer_encode_failed"))?;
        let ids = encoded
            .iter()
            .map(|e| e.get_ids().to_vec())
            .collect::<Vec<_>>();
        let attention = encoded
            .iter()
            .map(|e| e.get_attention_mask().to_vec())
            .collect::<Vec<_>>();
        let types = encoded
            .iter()
            .map(|e| e.get_type_ids().to_vec())
            .collect::<Vec<_>>();
        input_tokens += attention
            .iter()
            .flatten()
            .map(|v| u64::from(*v))
            .sum::<u64>();
        let ids_t = compute(Tensor::new(ids.clone(), &device))?;
        let mask_t = compute(Tensor::new(attention.clone(), &device))?;
        let types_t = compute(Tensor::new(types.clone(), &device))?;
        let hidden = compute(model.forward(&ids_t, &types_t, Some(&mask_t)))?;
        let mut pooled = if cls {
            compute(hidden.i((.., 0, ..)))?
        } else {
            let mask = compute(mask_t.to_dtype(DType::F32))?;
            let expanded = compute(mask.unsqueeze(2))?;
            let total = compute(compute(hidden.broadcast_mul(&expanded))?.sum(1))?;
            compute(total.broadcast_div(&compute(mask.sum_keepdim(1))?))?
        };
        let effective_normalize = input.interface_profile == "bge_store" || input.normalize;
        if intrinsic_normalize || effective_normalize {
            pooled = normalize(pooled)?;
        }
        outputs.push(pooled);
        input_ids.extend(ids);
        masks.extend(attention);
        type_ids.extend(types);
    }
    let refs = outputs.iter().collect::<Vec<_>>();
    let pooled = compute(Tensor::cat(&refs, 0))?;
    result.output = tensor_output(&if input.batch {
        pooled
    } else {
        compute(pooled.squeeze(0))?
    })?;
    result.token_inputs =
        json!({"input_ids":input_ids,"attention_mask":masks,"token_type_ids":type_ids});
    result.parameters = json!({"architecture":"bert","pooling":if cls{"cls"}else{"masked_mean"},"intrinsic_normalize":intrinsic_normalize,
        "ignored_options":input.interface_profile=="bge_store","requested_normalize":input.normalize,"wrapper_normalize":input.interface_profile=="bge_store" || input.normalize,"max_seq_length":limit,"batch_size":batch_size,"dtype":"float32","device":"cpu","inference_performed":true});
    result.usage.input_tokens = Some(input_tokens.to_string());
    result.usage_status = "partial".into();
    Ok(())
}

/// The original own wrapper accepts only these generation parameters.
pub fn hf_generation_parameters(
    options: &Value,
    input_len: usize,
    default_length: usize,
) -> NativeResult<Value> {
    let temperature = options.get("temperature").map_or(Ok(0.7), |v| {
        v.as_f64()
            .ok_or_else(|| reject("native_temperature_invalid"))
    })?;
    if !temperature.is_finite() || temperature <= 0.0 {
        return Err(reject("native_temperature_invalid"));
    }
    let length = |key: &str| -> NativeResult<usize> {
        options[key]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or_else(|| reject("native_generation_length_invalid"))
    };
    let (max_new, source) = if options.get("max_length").is_some() {
        (
            length("max_length")?
                .checked_sub(input_len)
                .filter(|v| *v > 0)
                .ok_or_else(|| reject("native_generation_length_invalid"))?,
            "max_length",
        )
    } else if options.get("max_new_tokens").is_some() {
        (length("max_new_tokens")?, "max_new_tokens")
    } else if options.get("max_tokens").is_some() {
        (length("max_tokens")?, "max_tokens")
    } else {
        (
            default_length
                .checked_sub(input_len)
                .filter(|v| *v > 0)
                .ok_or_else(|| reject("native_generation_length_invalid"))?,
            "generation_config/default_max_length",
        )
    };
    if max_new > 512 {
        return Err(reject("native_generation_budget_exceeded"));
    }
    Ok(
        json!({"temperature":temperature,"do_sample":true,"max_new_tokens":max_new,"length_source":source}),
    )
}
fn python_whitespace(c: char) -> bool {
    c.is_whitespace() || ('\u{001c}'..='\u{001f}').contains(&c)
}
pub fn hf_decoded_continuation(decoded: &str, prompt: &str) -> String {
    decoded
        .chars()
        .skip(prompt.chars().count())
        .collect::<String>()
        .trim_matches(python_whitespace)
        .to_owned()
}
pub fn hf_structured_response(text: &str) -> NativeResult<Value> {
    if let Ok(v) = parse_protocol_json(text) {
        return Ok(v);
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}'))
        && end > start
    {
        return parse_protocol_json(&text[start..=end])
            .map_err(|_| reject("native_hf_structured_parse_failed"));
    }
    Err(reject("native_hf_structured_parse_failed"))
}
fn execute_llama(
    input: &SemanticNativeRequest,
    assets: &mut BTreeMap<String, Vec<u8>>,
    config: &Value,
    result: &mut SemanticNativeResult,
) -> NativeResult<()> {
    let raw: llama::LlamaConfig = serde_json::from_value(config.clone())
        .map_err(|_| reject("native_llama_configuration_invalid"))?;
    if raw.hidden_size == 0
        || raw.hidden_size > 2048
        || raw.num_attention_heads == 0
        || raw.hidden_size % raw.num_attention_heads != 0
        || raw.num_key_value_heads() == 0
        || raw.num_attention_heads % raw.num_key_value_heads() != 0
        || raw.num_hidden_layers == 0
        || raw.num_hidden_layers > 64
        || raw.vocab_size < 2
        || raw.vocab_size > 100000
        || raw.intermediate_size == 0
        || raw.intermediate_size > 16384
        || raw.max_position_embeddings < 2
        || raw.max_position_embeddings > 8192
        || raw.hidden_size / raw.num_attention_heads % 2 != 0
        || !raw.rope_theta.is_finite()
        || raw.rope_theta <= 0.0
        || raw.rope_scaling.is_some()
        || config
            .get("head_dim")
            .is_some_and(|v| v.as_u64() != Some((raw.hidden_size / raw.num_attention_heads) as u64))
        || !raw.rms_norm_eps.is_finite()
        || raw.rms_norm_eps <= 0.0
        || config["hidden_act"] != "silu"
        || config["attention_bias"].as_bool() == Some(true)
        || config["mlp_bias"].as_bool() == Some(true)
    {
        return Err(reject("native_llama_configuration_unsupported"));
    }
    let cfg = raw.into_config(false);
    let device = Device::Cpu;
    let mut tokenizer = Tokenizer::from_bytes(asset(assets, "tokenizer")?)
        .map_err(|_| reject("native_tokenizer_invalid"))?;
    tokenizer
        .with_padding(None)
        .with_truncation(None)
        .map_err(|_| reject("native_tokenizer_invalid"))?;
    let token_config = parsed(asset(assets, "tokenizer_config")?)?;
    let generation = parsed(asset(assets, "generation_config")?)?;
    // Only configurations whose own generation defaults are implemented may
    // reach the forward; ignore caller kwargs exactly where the old wrapper did.
    let supported_defaults = [
        "_from_model_config",
        "transformers_version",
        "bos_token_id",
        "eos_token_id",
        "pad_token_id",
        "max_length",
        "top_k",
        "top_p",
        "do_sample",
        "temperature",
    ];
    if !generation.is_object()
        || generation
            .as_object()
            .is_some_and(|v| v.keys().any(|k| !supported_defaults.contains(&k.as_str())))
        || ["bos_token_id", "eos_token_id", "pad_token_id"]
            .iter()
            .any(|k| {
                generation
                    .get(*k)
                    .is_some_and(|v| !v.is_null() && v != &config[*k])
            })
    {
        return Err(reject("native_generation_config_unsupported"));
    }
    let ids = tokenizer
        .encode(input.texts[0].clone(), true)
        .map_err(|_| reject("native_tokenizer_encode_failed"))?
        .get_ids()
        .to_vec();
    if ids.is_empty() || ids.len() >= cfg.max_position_embeddings {
        return Err(reject("native_causal_input_limit"));
    }
    let mut params = if input.operation == "causal_logits" {
        json!({"max_new_tokens":0,"sampling":"none/weighted-logit-validation"})
    } else {
        hf_generation_parameters(
            &input.options,
            ids.len(),
            generation["max_length"].as_u64().unwrap_or(20) as usize,
        )?
    };
    if input.operation == "argmax_validation" {
        params["do_sample"] = false.into();
        params["sampling"] = "explicit_argmax_validation".into();
    } else if input.operation != "causal_logits" {
        params["sampling"] = "candle-0.9.1-StdRng-TopK/1".into();
    }
    let max_new = params["max_new_tokens"].as_u64().unwrap_or(0) as usize;
    if ids
        .len()
        .checked_add(max_new)
        .is_none_or(|n| n > cfg.max_position_embeddings)
    {
        return Err(reject("native_causal_input_limit"));
    }
    let weights = assets
        .remove("weights")
        .ok_or_else(|| reject("native_required_asset_missing"))?;
    let vb = compute(VarBuilder::from_buffered_safetensors(
        weights,
        DType::F32,
        &device,
    ))?;
    // Upstream Llama::load unwraps each block load. Validate every block tensor
    // through the same VarBuilder first, so malformed assets return a safe error.
    for i in 0..cfg.num_hidden_layers {
        let prefix = format!("model.layers.{i}");
        let h = cfg.hidden_size;
        let m = cfg.intermediate_size;
        let kv = h / cfg.num_attention_heads * cfg.num_key_value_heads;
        for (suffix, shape) in [
            ("self_attn.q_proj.weight", vec![h, h]),
            ("self_attn.k_proj.weight", vec![kv, h]),
            ("self_attn.v_proj.weight", vec![kv, h]),
            ("self_attn.o_proj.weight", vec![h, h]),
            ("mlp.gate_proj.weight", vec![m, h]),
            ("mlp.up_proj.weight", vec![m, h]),
            ("mlp.down_proj.weight", vec![h, m]),
            ("input_layernorm.weight", vec![h]),
            ("post_attention_layernorm.weight", vec![h]),
        ] {
            let _ = compute(vb.get(shape, &format!("{prefix}.{suffix}")))?;
        }
    }
    let model = compute(llama::Llama::load(vb, &cfg))?;
    let mut cache = compute(llama::Cache::new(true, DType::F32, &cfg, &device))?;
    let initial = compute(Tensor::new(ids.as_slice(), &device))?;
    let mut logits = compute(model.forward(&compute(initial.unsqueeze(0))?, 0, &mut cache))?;
    logits = compute(logits.squeeze(0))?;
    let mut generated = ids.clone();
    let mut new_ids = vec![];
    if input.operation == "causal_logits" {
        result.output = tensor_output(&logits)?;
    } else {
        let top_k = generation["top_k"].as_u64().unwrap_or(50) as usize;
        if top_k == 0 || top_k > cfg.vocab_size || generation.get("top_p").is_some_and(|v| v != 1.0)
        {
            return Err(reject("native_generation_config_unsupported"));
        }
        params["top_k"] = top_k.into();
        let sampling = if input.operation == "argmax_validation" {
            Sampling::ArgMax
        } else {
            Sampling::TopK {
                k: top_k,
                temperature: params["temperature"].as_f64().unwrap_or(0.7),
            }
        };
        let mut sampler = LogitsProcessor::from_sampling(input.seed, sampling);
        for _ in 0..max_new {
            let next = compute(sampler.sample(&logits))?;
            generated.push(next);
            new_ids.push(next);
            let eos = match &cfg.eos_token_id {
                Some(llama::LlamaEosToks::Single(id)) => next == *id,
                Some(llama::LlamaEosToks::Multiple(ids)) => ids.contains(&next),
                None => false,
            };
            if eos || new_ids.len() == max_new {
                break;
            }
            let token = compute(Tensor::new(&[next], &device))?;
            logits = compute(
                compute(model.forward(
                    &compute(token.unsqueeze(0))?,
                    generated.len() - 1,
                    &mut cache,
                ))?
                .squeeze(0),
            )?;
        }
        let mut decoded = tokenizer
            .decode(&generated, true)
            .map_err(|_| reject("native_tokenizer_decode_failed"))?;
        if token_config["clean_up_tokenization_spaces"].as_bool() == Some(true) {
            for (from, to) in [
                (" .", "."),
                (" ?", "?"),
                (" !", "!"),
                (" ,", ","),
                (" ' ", "'"),
                (" n't", "n't"),
                (" 'm", "'m"),
                (" 's", "'s"),
                (" 've", "'ve"),
                (" 're", "'re"),
            ] {
                decoded = decoded.replace(from, to);
            }
        }
        let text = hf_decoded_continuation(&decoded, &input.texts[0]);
        result.output = if input.operation == "generate_structured" {
            hf_structured_response(&text)?
        } else {
            text.into()
        };
        params["generated_ids"] = json!(new_ids);
        params["decoded_full_sequence"] = decoded.into();
    }
    result.parameters = params;
    result.token_inputs = json!({"input_ids":[ids],"attention_mask":[vec![1u32;generated.len()-new_ids.len()]],"token_type_ids":[]});
    let input_count = generated.len() - new_ids.len();
    result.usage.input_tokens = Some(input_count.to_string());
    if input.operation != "causal_logits" {
        result.usage.output_tokens = Some(new_ids.len().to_string());
        result.usage.total_tokens = Some(generated.len().to_string());
        result.usage_status = "known".into();
    } else {
        result.usage_status = "partial".into();
    }
    Ok(())
}
