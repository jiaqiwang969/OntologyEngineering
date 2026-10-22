//! Source-equivalent OpenAI/Groq one-call protocol. Transport and result secrecy
//! guards are injected; credentials, retries, budgets and adoption live above.
use semantica_contracts::*;
use serde_json::{Value, json};

pub mod factory;
pub mod litellm;

pub const MAX_PROVIDER_BYTES: usize = 2 * 1024 * 1024;
const OPENAI_OPTIONS: &[&str] = &[
    "temperature",
    "max_completion_tokens",
    "max_tokens",
    "top_p",
    "frequency_penalty",
    "presence_penalty",
    "seed",
    "stop",
    "logit_bias",
    "user",
];

pub fn failure(code: &str, phase: &str) -> SemanticProviderFailure {
    SemanticProviderFailure {
        code: code.into(),
        phase: phase.into(),
        retryable: false,
        retry_after: None,
        http_status: None,
    }
}

/// Logical request identity and exact wire bytes have separate explicit hashes.
pub fn prepare_request(
    request: &SemanticProviderRequest,
) -> Result<SemanticProviderWireRequest, SemanticProviderFailure> {
    request
        .validate()
        .map_err(|_| failure("provider_request_invalid", "validate"))?;
    if request.prompt.len() > MAX_PROVIDER_BYTES
        || !request.options.is_object()
        || (request.provider == "groq" && request.custom_endpoint)
    {
        return Err(failure("provider_request_invalid", "validate"));
    }
    let default = match (
        request.provider.as_str(),
        request.interface_profile.as_str(),
    ) {
        ("openai", _) => "gpt-3.5-turbo",
        ("groq", "llms") => "llama-3.1-8b-instant",
        ("groq", _) => "llama-3.3-70b-versatile",
        _ => return Err(failure("provider_uncovered", "validate")),
    };
    // The original call-level model keyword overrides the instance default.
    let model = request.options.get("model").cloned().unwrap_or_else(|| {
        request
            .model
            .clone()
            .map_or_else(|| json!(default), Value::String)
    });
    if !model.is_null() && !model.is_string() {
        return Err(failure("provider_model_invalid", "validate"));
    }
    let structured = request.operation == "generate_structured";
    let prompt = if structured
        && request.provider == "groq"
        && !request.prompt.to_lowercase().contains("json")
    {
        format!(
            "{}\n\nReturn the response as valid JSON only.",
            request.prompt
        )
    } else {
        request.prompt.clone()
    };
    let mut body = json!({"model": model, "messages": [{"role": "user", "content": prompt}]});
    if structured && (request.provider == "groq" || !request.custom_endpoint) {
        body["response_format"] = json!({"type": "json_object"});
    }
    for key in OPENAI_OPTIONS {
        if request.provider == "groq" && *key == "logit_bias" {
            continue;
        }
        if let Some(value) = request.options.get(*key).filter(|v| !v.is_null()) {
            body[*key] = value.clone();
        }
    }
    let bytes =
        serde_json::to_vec(&body).map_err(|_| failure("provider_request_invalid", "validate"))?;
    if bytes.len() > MAX_PROVIDER_BYTES {
        return Err(failure("provider_request_too_large", "validate"));
    }
    Ok(SemanticProviderWireRequest {
        provider: request.provider.clone(),
        custom_endpoint: request.custom_endpoint,
        operation: request.operation.clone(),
        body,
        body_sha256: crate::sha256(bytes),
    })
}

/// Compatibility path; the neutral contract crate owns JSON meaning for all
/// generated fields, provider protocols and engineering wire adapters.
pub fn parse_protocol_json(text: &str) -> Result<Value, ()> {
    semantica_contracts::json::protocol_json(text, MAX_PROVIDER_BYTES).map_err(|_| ())
}

/// The original JSON recovery algorithm, including scalars and last-key wins.
/// Nonfinite Python JSON remains an explicit uncovered compatibility edge.
pub fn parse_structured_json(text: &str) -> Result<Value, SemanticProviderFailure> {
    if text.is_empty() {
        return Err(failure("empty_response", "parse"));
    }
    if text.len() > MAX_PROVIDER_BYTES {
        return Err(failure("response_too_large", "parse"));
    }
    let mut clean = text.trim();
    if let Some(start) = clean.find("```json") {
        let remaining = &clean[start + 7..];
        clean = remaining.split("```").next().unwrap_or(remaining).trim();
    } else if clean.contains("```") {
        for block in clean.split("```") {
            let block = block.trim();
            if (block.starts_with('{') && block.ends_with('}'))
                || (block.starts_with('[') && block.ends_with(']'))
            {
                clean = block;
                break;
            }
        }
    }
    if let Ok(value) = parse_protocol_json(clean) {
        return Ok(value);
    }
    let start = [clean.find('{'), clean.find('[')]
        .into_iter()
        .flatten()
        .min();
    let end = [clean.rfind('}'), clean.rfind(']')]
        .into_iter()
        .flatten()
        .max();
    if let (Some(start), Some(end)) = (start, end)
        && end > start
    {
        let candidate = &clean[start..=end];
        if let Ok(value) = parse_protocol_json(candidate) {
            return Ok(value);
        }
        let trailing = regex::Regex::new(r",\s*([\]}])").expect("fixed regex");
        let repaired = trailing.replace_all(candidate, "$1");
        if let Ok(value) = parse_protocol_json(&repaired) {
            return Ok(value);
        }
        let candidate = candidate
            .strip_suffix("...")
            .map(str::trim)
            .unwrap_or(candidate);
        let mut closed = candidate.to_owned();
        let braces = candidate
            .matches('{')
            .count()
            .saturating_sub(candidate.matches('}').count());
        let brackets = candidate
            .matches('[')
            .count()
            .saturating_sub(candidate.matches(']').count());
        closed.push_str(&"}".repeat(braces));
        closed.push_str(&"]".repeat(brackets));
        if let Ok(value) = parse_protocol_json(&trailing.replace_all(&closed, "$1")) {
            return Ok(value);
        }
    }
    Err(failure("structured_json_invalid", "parse"))
}

fn token(value: Option<&Value>) -> Option<String> {
    let text = value?.as_number()?.to_string();
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())).then_some(text)
}

pub fn response_usage(data: &Value) -> (SemanticProviderUsage, String) {
    let raw_usage = data.get("usage");
    let usage = SemanticProviderUsage {
        input_tokens: token(raw_usage.and_then(|v| v.get("prompt_tokens"))),
        output_tokens: token(raw_usage.and_then(|v| v.get("completion_tokens"))),
        total_tokens: token(raw_usage.and_then(|v| v.get("total_tokens"))),
    };
    let known = [
        &usage.input_tokens,
        &usage.output_tokens,
        &usage.total_tokens,
    ]
    .iter()
    .filter(|v| v.is_some())
    .count();
    let status = if raw_usage.is_none_or(Value::is_null) {
        "unknown"
    } else if known == 3 {
        "reported"
    } else if raw_usage.is_some_and(|v| {
        ["prompt_tokens", "completion_tokens", "total_tokens"]
            .iter()
            .any(|k| v.get(*k).is_some_and(|v| !v.is_null()) && token(v.get(*k)).is_none())
    }) {
        "invalid"
    } else {
        "partial"
    };
    (usage, status.into())
}

pub fn decode_response(
    request: &SemanticProviderRequest,
    wire: &SemanticProviderWireRequest,
    response: &SemanticProviderWireResponse,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    let data = checked_response_data(response)?;
    let content = data
        .pointer("/choices/0/message/content")
        .ok_or_else(|| failure("provider_content_missing", "parse"))?;
    let output = if request.operation == "generate_structured" {
        parse_structured_json(
            content
                .as_str()
                .ok_or_else(|| failure("empty_response", "parse"))?,
        )?
    } else if content.is_null() || content.is_string() {
        content.clone()
    } else {
        return Err(failure("provider_content_invalid", "parse"));
    };
    protocol_result(
        request,
        &request.provider,
        &request.operation,
        wire,
        response,
        &data,
        output,
    )
}

pub(crate) fn checked_response_data(
    response: &SemanticProviderWireResponse,
) -> Result<Value, SemanticProviderFailure> {
    if response.validate().is_err()
        || response.body.len() > MAX_PROVIDER_BYTES
        || crate::sha256(response.body.as_bytes()) != response.body_sha256
    {
        return Err(failure("provider_response_binding_invalid", "parse"));
    }
    if !(200..300).contains(&response.http_status) {
        return Err(failure("provider_http_failure", "transport"));
    }
    parse_protocol_json(&response.body).map_err(|_| failure("provider_response_invalid", "parse"))
}

pub(crate) fn protocol_result<T: serde::Serialize>(
    request: &T,
    provider: &str,
    operation: &str,
    wire: &SemanticProviderWireRequest,
    response: &SemanticProviderWireResponse,
    data: &Value,
    output: Value,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    let (usage, status) = response_usage(&data);
    let original =
        serde_json::to_vec(request).map_err(|_| failure("provider_request_invalid", "validate"))?;
    Ok(SemanticProviderResult {
        schema: "semantica.provider-result/1".into(),
        execution: "completed".into(),
        provider: provider.into(),
        operation: operation.into(),
        request_sha256: crate::sha256(original),
        wire_request_sha256: wire.body_sha256.clone(),
        response_sha256: response.body_sha256.clone(),
        requested_model: wire
            .body
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        resolved_model: data.get("model").and_then(Value::as_str).map(str::to_owned),
        provider_response_id: data.get("id").and_then(Value::as_str).map(str::to_owned),
        output,
        usage,
        usage_status: status,
        transport_kind: response.transport_kind.clone(),
        transport_identity: response.transport_identity.clone(),
    })
}

/// Original BaseProvider tests uppercase JSON case-sensitively.
pub fn base_structured_prompt(prompt: &str) -> String {
    if prompt.contains("JSON") {
        prompt.into()
    } else {
        format!("{prompt}\n\nReturn the response as valid JSON only.")
    }
}

/// Truthiness of the original json.loads values; missing usage is unrelated.
pub fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_none_or(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}

pub struct RustProviderRuntime<'a> {
    transport: &'a dyn SemanticProviderTransportPort,
    guard: &'a dyn SemanticProviderResultGuardPort,
}
impl<'a> RustProviderRuntime<'a> {
    pub fn new(
        transport: &'a dyn SemanticProviderTransportPort,
        guard: &'a dyn SemanticProviderResultGuardPort,
    ) -> Self {
        Self { transport, guard }
    }
    fn guard_result(
        &self,
        result: SemanticProviderResult,
    ) -> Result<SemanticProviderResult, SemanticProviderFailure> {
        let checked = self.guard.check_result(&result)?;
        if checked.schema != "semantica.provider-result-guard/1"
            || !checked.credential_material_absent
        {
            return Err(failure("credential_material_in_response", "guard"));
        }
        Ok(result)
    }
}
impl SemanticProviderGenerationPort for RustProviderRuntime<'_> {
    fn generate(
        &self,
        input: &SemanticProviderRequest,
    ) -> Result<SemanticProviderResult, SemanticProviderFailure> {
        let wire = prepare_request(input)?;
        let response = self.transport.post(&wire)?;
        let result = decode_response(input, &wire, &response)?;
        self.guard_result(result)
    }
}

impl SemanticEmbeddingGenerationPort for RustProviderRuntime<'_> {
    fn embed(
        &self,
        input: &SemanticEmbeddingRequest,
    ) -> Result<SemanticProviderResult, SemanticProviderFailure> {
        let wire = crate::embeddings::prepare_embedding_request(input)?;
        let response = self.transport.post(&wire)?;
        let result = crate::embeddings::decode_embedding_response(input, &wire, &response)?;
        self.guard_result(result)
    }
}
