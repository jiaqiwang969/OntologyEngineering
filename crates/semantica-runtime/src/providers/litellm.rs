//! The original Semantica LiteLLM wrapper, with an injected completion boundary.
//!
//! This module does not implement the LiteLLM SDK dispatcher. Completion request
//! construction retains the original model and all shallow-merged options. The
//! separate protocol adapter covers only the existing OpenAI/Groq protocols.

use semantica_contracts::{
    SemanticLiteLlmFailure, SemanticLiteLlmObjectResponse, SemanticLiteLlmRequest,
    SemanticProviderFailure, SemanticProviderResult, SemanticProviderWireRequest,
    SemanticProviderWireResponse,
};
use serde_json::{Value, json};

use super::MAX_PROVIDER_BYTES;

/// Real Rust object values are distinct from dictionary/string JSON responses.
pub enum ResponseView<'a> {
    Json(&'a Value),
    Object(&'a SemanticLiteLlmObjectResponse),
}

fn wrapper_failure(
    code: &str,
    phase: &str,
    structured: bool,
    detail: &str,
) -> SemanticLiteLlmFailure {
    SemanticLiteLlmFailure {
        code: code.into(),
        phase: phase.into(),
        message: format!(
            "LiteLLM {}generation failed: {detail}",
            if structured { "structured " } else { "" }
        ),
    }
}

/// Availability is supplied by the completion boundary; no SDK import occurs.
pub fn check_available(available: bool) -> Result<(), SemanticLiteLlmFailure> {
    if available {
        Ok(())
    } else {
        Err(SemanticLiteLlmFailure {
            code: "litellm_unavailable".into(),
            phase: "initialize".into(),
            message: "LiteLLM library not installed. Install with: pip install litellm".into(),
        })
    }
}

/// Shallow instance-config/call-kwargs merge and the original completion call.
/// Credentials remain in the existing injected transport, outside wire bodies.
pub fn prepare_completion_request(
    request: &SemanticLiteLlmRequest,
) -> Result<SemanticProviderWireRequest, SemanticLiteLlmFailure> {
    let generation = &request.generation;
    let structured = generation.operation == "generate_structured";
    if request.validate().is_err()
        || generation.provider != "litellm"
        || generation.interface_profile != "llms"
        || generation.prompt.len() > MAX_PROVIDER_BYTES
    {
        return Err(wrapper_failure(
            "litellm_request_invalid",
            "validate",
            structured,
            "Invalid completion request",
        ));
    }
    let (Some(config), Some(kwargs)) = (request.config.as_object(), generation.options.as_object())
    else {
        return Err(wrapper_failure(
            "litellm_options_invalid",
            "validate",
            structured,
            "Completion options must be dictionaries",
        ));
    };
    let mut options = config.clone();
    for (key, value) in kwargs {
        options.insert(key.clone(), value.clone());
    }
    for keyword in ["model", "messages", "api_key"] {
        if options.contains_key(keyword) {
            return Err(wrapper_failure(
                &format!("litellm_duplicate_{keyword}"),
                "validate",
                structured,
                &format!("completion() got multiple values for keyword argument '{keyword}'"),
            ));
        }
    }
    let prompt = if structured {
        format!(
            "{}\n\nReturn the response as valid JSON only.",
            generation.prompt
        )
    } else {
        generation.prompt.clone()
    };
    let mut body = json!({
        "model": generation.model,
        "messages": [{"role": "user", "content": prompt}]
    });
    let fields = body.as_object_mut().expect("constructed object");
    for (key, value) in options {
        fields.insert(key, value);
    }
    let bytes = semantica_contracts::json::ordered_json(&body).map_err(|_| {
        wrapper_failure(
            "litellm_request_invalid",
            "validate",
            structured,
            "Invalid completion request",
        )
    })?;
    if bytes.len() > MAX_PROVIDER_BYTES {
        return Err(wrapper_failure(
            "litellm_request_too_large",
            "validate",
            structured,
            "Completion request exceeds byte limit",
        ));
    }
    Ok(SemanticProviderWireRequest {
        provider: "litellm".into(),
        custom_endpoint: generation.custom_endpoint,
        operation: generation.operation.clone(),
        body,
        body_sha256: crate::sha256(bytes),
    })
}

/// An explicit finite adapter, not a replacement for the SDK's 62 providers.
/// The existing OpenAI/Groq implementation owns protocol request construction.
pub fn prepare_protocol_request(
    request: &SemanticLiteLlmRequest,
    completion: &SemanticProviderWireRequest,
) -> Result<SemanticProviderWireRequest, SemanticProviderFailure> {
    let expected = prepare_completion_request(request)
        .map_err(|error| super::failure(&error.code, &error.phase))?;
    if completion != &expected {
        return Err(super::failure(
            "litellm_completion_binding_invalid",
            "validate",
        ));
    }
    let model = completion
        .body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| super::failure("litellm_sdk_model_uncovered", "validate"))?;
    let (provider, model) = model
        .split_once('/')
        .filter(|(provider, model)| matches!(*provider, "openai" | "groq") && !model.is_empty())
        .ok_or_else(|| super::failure("litellm_sdk_provider_uncovered", "validate"))?;
    // Extra SDK options are preserved at the completion boundary. The finite
    // adapter refuses them instead of silently dropping their original meaning.
    const OPTIONS: &[&str] = &[
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
    let mut options = completion
        .body
        .as_object()
        .expect("bound completion object")
        .clone();
    options.remove("model");
    options.remove("messages");
    if options
        .keys()
        .any(|key| !OPTIONS.contains(&key.as_str()) || (provider == "groq" && key == "logit_bias"))
    {
        return Err(super::failure("litellm_sdk_options_uncovered", "validate"));
    }
    let mut protocol = request.generation.clone();
    protocol.provider = provider.into();
    protocol.model = Some(model.into());
    protocol.options = Value::Object(options);
    // Original LiteLLM structured output only modifies its prompt. It does not
    // request the OpenAI/Groq JSON response_format or run their prompt helpers.
    protocol.operation = "generate".into();
    protocol.prompt = completion.body["messages"][0]["content"]
        .as_str()
        .expect("bound completion prompt")
        .into();
    let mut wire = super::prepare_request(&protocol)?;
    wire.operation = request.generation.operation.clone();
    Ok(wire)
}

fn json_content(
    response: &Value,
    structured: bool,
) -> Result<Option<Value>, SemanticLiteLlmFailure> {
    match response {
        Value::String(_) => Ok(Some(response.clone())),
        Value::Object(fields) => {
            if let Some(choices) = fields.get("choices") {
                // The original tests len() before indexing. Empty dictionaries
                // and strings therefore take the content fallback too.
                let empty_nonarray = matches!(choices,
                    Value::Object(value) if value.is_empty())
                    || matches!(choices, Value::String(value) if value.is_empty());
                let array = match choices.as_array() {
                    Some(array) => Some(array),
                    None if empty_nonarray => None,
                    None => {
                        return Err(wrapper_failure(
                            "litellm_choices_invalid",
                            "parse",
                            structured,
                            "Invalid response choices",
                        ));
                    }
                };
                if let Some(first) = array.and_then(|choices| choices.first()) {
                    return first
                        .get("message")
                        .and_then(|message| message.get("content"))
                        .cloned()
                        .map(Some)
                        .ok_or_else(|| {
                            wrapper_failure(
                                "litellm_content_missing",
                                "parse",
                                structured,
                                "Missing response message content",
                            )
                        });
                }
            }
            Ok(fields.get("content").cloned())
        }
        _ => Ok(None),
    }
}

fn python_type(response: &Value) -> &'static str {
    match response {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(number) if !number.to_string().contains(['.', 'e', 'E']) => "int",
        Value::Number(_) => "float",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

/// First-choice object, dictionary fallback, and direct string extraction.
/// Plain generation retains any actual content value, including JSON null.
pub fn decode_output(
    response: ResponseView<'_>,
    structured: bool,
) -> Result<Value, SemanticLiteLlmFailure> {
    let (content, response_type) = match response {
        ResponseView::Json(value) => (json_content(value, structured)?, python_type(value)),
        ResponseView::Object(value) => (
            value
                .choices
                .first()
                .map(|choice| choice.message.content.clone()),
            "object",
        ),
    };
    if !structured {
        return content.ok_or_else(|| {
            wrapper_failure(
                "litellm_unexpected_response",
                "parse",
                false,
                &format!("Unexpected response format from LiteLLM: <class '{response_type}'>"),
            )
        });
    }
    // The original structured wrapper leaves its initial empty string when no
    // response branch matched, while an extracted None raises a type error.
    match content {
        Some(Value::String(text)) => parse_structured_json(&text),
        Some(value) => Err(wrapper_failure(
            "litellm_structured_content_type",
            "parse",
            true,
            &format!(
                "the JSON object must be str, bytes or bytearray, not {}",
                python_type(&value)
            ),
        )),
        None => parse_structured_json(""),
    }
}

/// Exact original recovery: direct json.loads, then greedy DOTALL object only.
/// Fences/lists/trailing commas/incomplete containers receive no extra repair.
pub fn parse_structured_json(text: &str) -> Result<Value, SemanticLiteLlmFailure> {
    if text.len() > MAX_PROVIDER_BYTES {
        return Err(wrapper_failure(
            "litellm_response_too_large",
            "parse",
            true,
            "Completion response exceeds byte limit",
        ));
    }
    if let Ok(value) = semantica_contracts::json::protocol_json(text, MAX_PROVIDER_BYTES) {
        return Ok(value);
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}'))
        && end > start
    {
        let candidate = &text[start..=end];
        if let Ok(value) = semantica_contracts::json::protocol_json(candidate, MAX_PROVIDER_BYTES) {
            return Ok(value);
        }
        let detail = json_error_category(candidate);
        return Err(wrapper_failure(
            "litellm_structured_json_invalid",
            "parse",
            true,
            detail,
        ));
    }
    Err(wrapper_failure(
        "litellm_structured_json_unrecoverable",
        "parse",
        true,
        "Failed to parse JSON from LiteLLM response",
    ))
}

// Diagnostics contain a source error category only, never response bytes.
// Classification does not modify recovery or provide a second JSON parser.
fn json_error_category(candidate: &str) -> &'static str {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (offset, character) in candidate.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
            continue;
        }
        match character {
            '"' => quoted = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = offset + 1;
                    if !candidate[end..].trim().is_empty()
                        && semantica_contracts::json::protocol_json(
                            &candidate[..end],
                            MAX_PROVIDER_BYTES,
                        )
                        .is_ok()
                    {
                        return "Extra data";
                    }
                }
            }
            _ => {}
        }
    }
    if candidate
        .strip_suffix('}')
        .is_some_and(|prefix| prefix.trim_end().ends_with(','))
    {
        "Expecting property name enclosed in double quotes"
    } else {
        "Invalid JSON response"
    }
}

/// The source wrapper's ProcessingError formatting for completion-boundary
/// failures. The owner must supply guarded failure detail for real transports.
pub fn completion_failure(detail: &str, structured: bool) -> SemanticLiteLlmFailure {
    wrapper_failure("litellm_completion_failed", "transport", structured, detail)
}

/// Decode through existing response binding, JSON owner, result hashes/usage.
/// The transport's credential checks and the common result guard remain above.
pub fn decode_response(
    request: &SemanticLiteLlmRequest,
    completion: &SemanticProviderWireRequest,
    response: &SemanticProviderWireResponse,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    let expected = prepare_completion_request(request)
        .map_err(|error| super::failure(&error.code, &error.phase))?;
    if completion != &expected {
        return Err(super::failure(
            "litellm_completion_binding_invalid",
            "validate",
        ));
    }
    let data = super::checked_response_data(response)?;
    let output = decode_output(
        ResponseView::Json(&data),
        request.generation.operation == "generate_structured",
    )
    .map_err(|error| super::failure(&error.code, &error.phase))?;
    super::protocol_result(
        request,
        "litellm",
        &request.generation.operation,
        completion,
        response,
        &data,
        output,
    )
}

/// Bind both the original completion call and the actual finite HTTP request.
/// The wire identity records sent protocol bytes; model identity records the
/// complete model originally supplied to the Semantica LiteLLM wrapper.
pub fn decode_protocol_response(
    request: &SemanticLiteLlmRequest,
    completion: &SemanticProviderWireRequest,
    protocol: &SemanticProviderWireRequest,
    response: &SemanticProviderWireResponse,
) -> Result<SemanticProviderResult, SemanticProviderFailure> {
    let expected = prepare_protocol_request(request, completion)?;
    if protocol != &expected {
        return Err(super::failure(
            "litellm_protocol_binding_invalid",
            "validate",
        ));
    }
    let data = super::checked_response_data(response)?;
    let output = decode_output(
        ResponseView::Json(&data),
        request.generation.operation == "generate_structured",
    )
    .map_err(|error| super::failure(&error.code, &error.phase))?;
    let mut result = super::protocol_result(
        request,
        &protocol.provider,
        &request.generation.operation,
        protocol,
        response,
        &data,
        output,
    )?;
    result.requested_model = request.generation.model.clone();
    Ok(result)
}
