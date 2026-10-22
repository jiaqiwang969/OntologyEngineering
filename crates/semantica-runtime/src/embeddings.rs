//! Original OpenAIStore float32 protocol and finite provided-vector arithmetic.
//! Models, credential authority, retries and shared call budgets are injected above.
use crate::providers::{MAX_PROVIDER_BYTES, checked_response_data, failure, protocol_result};
use semantica_contracts::*;
use serde_json::{Value, json};

const MAX_COMPONENTS: usize = 262_144;

pub fn prepare_embedding_request(
    input: &SemanticEmbeddingRequest,
) -> Result<SemanticProviderWireRequest, SemanticProviderFailure> {
    if input.validate().is_err()
        || !input.options.is_object()
        || input.input.len() > MAX_PROVIDER_BYTES
    {
        return Err(failure("embedding_request_invalid", "validate"));
    }
    // OpenAIStore forwards only model and the single input text. kwargs.get
    // preserves an explicit null model; every other option is ignored.
    let model = input.options.get("model").cloned().unwrap_or_else(|| {
        input
            .model
            .clone()
            .map_or_else(|| json!("text-embedding-3-small"), Value::String)
    });
    if !model.is_null() && !model.is_string() {
        return Err(failure("embedding_model_invalid", "validate"));
    }
    let body = json!({"model":model,"input":input.input});
    let raw =
        serde_json::to_vec(&body).map_err(|_| failure("embedding_request_invalid", "validate"))?;
    if raw.len() > MAX_PROVIDER_BYTES {
        return Err(failure("embedding_request_too_large", "validate"));
    }
    Ok(SemanticProviderWireRequest {
        provider: input.provider.clone(),
        custom_endpoint: input.custom_endpoint,
        operation: "embed".into(),
        body,
        body_sha256: crate::sha256(raw),
    })
}

/// Explicit bit encoding preserves binary32 casts, negative zero and nonfinite
/// values without pretending JSON has IEEE NaN/Infinity numeric literals.
pub fn float32_tensor(value: &Value) -> Result<SemanticEmbeddingTensor, SemanticProviderFailure> {
    fn scalar(value: &Value) -> Result<u64, SemanticProviderFailure> {
        let number = match value {
            Value::Null => f64::NAN,
            Value::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Number(v) => v
                .as_f64()
                .ok_or_else(|| failure("embedding_float_conversion_failed", "parse"))?,
            Value::String(v) => {
                let v = v.trim();
                match v.to_ascii_lowercase().as_str() {
                    "nan" | "+nan" | "-nan" => f64::NAN,
                    "inf" | "+inf" | "infinity" | "+infinity" => f64::INFINITY,
                    "-inf" | "-infinity" => f64::NEG_INFINITY,
                    _ => v
                        .parse::<f64>()
                        .map_err(|_| failure("embedding_float_conversion_failed", "parse"))?,
                }
            }
            _ => return Err(failure("embedding_float_conversion_failed", "parse")),
        };
        Ok(u64::from((number as f32).to_bits()))
    }
    fn visit(
        value: &Value,
        depth: usize,
        bits: &mut Vec<u64>,
    ) -> Result<Vec<u64>, SemanticProviderFailure> {
        if depth > 16 || bits.len() >= MAX_COMPONENTS {
            return Err(failure("embedding_tensor_too_large", "parse"));
        }
        if let Value::Array(values) = value {
            if values.len() > MAX_COMPONENTS {
                return Err(failure("embedding_tensor_too_large", "parse"));
            }
            let mut child_shape = None;
            for value in values {
                let shape = visit(value, depth + 1, bits)?;
                if child_shape.as_ref().is_some_and(|old| old != &shape) {
                    return Err(failure("embedding_array_ragged", "parse"));
                }
                child_shape = Some(shape);
            }
            let mut shape = vec![values.len() as u64];
            shape.extend(child_shape.unwrap_or_default());
            Ok(shape)
        } else {
            bits.push(scalar(value)?);
            Ok(vec![])
        }
    }
    let mut bits = vec![];
    let shape = visit(value, 0, &mut bits)?;
    Ok(SemanticEmbeddingTensor {
        dtype: "float32".into(),
        shape,
        bits,
    })
}

pub fn decode_embedding_response(
    input: &SemanticEmbeddingRequest,
    wire: &SemanticProviderWireRequest,
    response: &SemanticProviderWireResponse,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    if prepare_embedding_request(input)? != *wire {
        return Err(failure("embedding_request_binding_invalid", "parse"));
    }
    let data = checked_response_data(response)?;
    let embedding = data
        .pointer("/data/0/embedding")
        .ok_or_else(|| failure("embedding_data_missing", "parse"))?;
    let tensor = float32_tensor(embedding)?;
    let output =
        serde_json::to_value(tensor).map_err(|_| failure("embedding_tensor_invalid", "parse"))?;
    protocol_result(input, "openai", "embed", wire, response, &data, output)
}

/// Mirrors np.array([self.embed(text) for text in texts]) under NumPy >= 2:
/// empty => float64 [0]; otherwise shape/dtype agree, and ragged results fail.
pub fn stack_embedding_results(
    results: &[SemanticProviderResult],
) -> Result<SemanticEmbeddingTensor, SemanticProviderFailure> {
    if results.is_empty() {
        return Ok(SemanticEmbeddingTensor {
            dtype: "float64".into(),
            shape: vec![0],
            bits: vec![],
        });
    }
    let mut shape = None;
    let mut bits = vec![];
    for result in results {
        if result.provider != "openai"
            || result.operation != "embed"
            || result.execution != "completed"
        {
            return Err(failure("embedding_batch_result_invalid", "parse"));
        }
        let tensor: SemanticEmbeddingTensor = serde_json::from_value(result.output.clone())
            .map_err(|_| failure("embedding_tensor_invalid", "parse"))?;
        let count = tensor.shape.iter().try_fold(1u64, |n, d| n.checked_mul(*d));
        if tensor.dtype != "float32"
            || count != Some(tensor.bits.len() as u64)
            || tensor.bits.iter().any(|n| *n > u64::from(u32::MAX))
        {
            return Err(failure("embedding_tensor_invalid", "parse"));
        }
        if shape.as_ref().is_some_and(|old| old != &tensor.shape) {
            return Err(failure("embedding_array_ragged", "parse"));
        }
        shape = Some(tensor.shape);
        bits.extend(tensor.bits);
        if bits.len() > MAX_COMPONENTS {
            return Err(failure("embedding_tensor_too_large", "parse"));
        }
    }
    let mut batch_shape = vec![results.len() as u64];
    batch_shape.extend(shape.unwrap_or_default());
    Ok(SemanticEmbeddingTensor {
        dtype: "float32".into(),
        shape: batch_shape,
        bits,
    })
}

pub struct RustEmbeddingPooling;
impl SemanticEmbeddingPoolingPort for RustEmbeddingPooling {
    fn pool(
        &self,
        input: &SemanticEmbeddingPoolRequest,
    ) -> Result<SemanticEmbeddingPoolResult, SemanticProviderFailure> {
        pool_embeddings(input)
    }
}

/// Documented finite f64 2D domain only. Other original NumPy ranks/dtypes and
/// nonfinite propagation remain explicit coverage gaps, never silent success.
pub fn pool_embeddings(
    input: &SemanticEmbeddingPoolRequest,
) -> Result<SemanticEmbeddingPoolResult, SemanticProviderFailure> {
    if input.validate().is_err() {
        return Err(failure("embedding_pool_request_invalid", "validate"));
    }
    let rows = &input.embeddings;
    let width = rows.first().map(Vec::len).unwrap_or(0);
    if rows.is_empty() {
        return Err(failure("cannot_pool_empty_embeddings", "validate"));
    }
    if width == 0
        || rows
            .len()
            .checked_mul(width)
            .is_none_or(|n| n > MAX_COMPONENTS)
        || rows
            .iter()
            .any(|r| r.len() != width || r.iter().any(|v| !v.is_finite()))
    {
        return Err(failure(
            "embedding_pool_shape_or_domain_invalid",
            "validate",
        ));
    }
    let mean = |rows: &[Vec<f64>]| {
        let mut output = vec![0.0; width];
        for row in rows {
            for (sum, value) in output.iter_mut().zip(row) {
                *sum += value;
            }
        }
        for value in &mut output {
            *value /= rows.len() as f64;
        }
        output
    };
    let strategy = input.strategy.to_lowercase();
    let output = match strategy.as_str() {
        "mean" => mean(rows),
        "max" => {
            let mut output = rows[0].clone();
            for row in &rows[1..] {
                for (out, value) in output.iter_mut().zip(row) {
                    *out = out.max(*value);
                }
            }
            output
        }
        "cls" => rows[0].clone(),
        "attention" => {
            let query = mean(rows);
            let scores: Vec<f64> = rows
                .iter()
                .map(|r| r.iter().zip(&query).map(|(a, b)| a * b).sum())
                .collect();
            let max = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let weights: Vec<f64> = scores.iter().map(|s| (s - max).exp()).collect();
            let total: f64 = weights.iter().sum();
            let mut output = vec![0.0; width];
            for (row, weight) in rows.iter().zip(weights) {
                for (out, value) in output.iter_mut().zip(row) {
                    *out += value * (weight / total);
                }
            }
            output
        }
        "hierarchical" => {
            let size = usize::try_from(input.chunk_size.unwrap_or(10))
                .map_err(|_| failure("pool_chunk_size_invalid", "validate"))?;
            if size == 0 {
                return Err(failure("pool_chunk_size_invalid", "validate"));
            }
            let chunks: Vec<Vec<f64>> = rows.chunks(size).map(mean).collect();
            mean(&chunks)
        }
        _ => return Err(failure("unsupported_pooling_strategy", "validate")),
    };
    if output.iter().any(|v| !v.is_finite()) {
        return Err(failure("embedding_pool_arithmetic_overflow", "execute"));
    }
    let raw = serde_json::to_vec(input)
        .map_err(|_| failure("embedding_pool_request_invalid", "validate"))?;
    Ok(SemanticEmbeddingPoolResult {
        schema: "semantica.embedding-pool-result/1".into(),
        execution: "completed".into(),
        profile: "semantica.provided-vectors-finite-f64-2d/1".into(),
        strategy,
        request_sha256: crate::sha256(raw),
        output,
    })
}
