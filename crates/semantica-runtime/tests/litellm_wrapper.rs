//! Original Semantica wrapper observations, frozen before Rust implementation.
//! SDK dispatch is an injected synthetic boundary; no SDK or network is used.

use semantica_contracts::*;
use semantica_runtime::providers::litellm::*;
use serde_json::{Value, json};

fn request(structured: bool) -> SemanticLiteLlmRequest {
    SemanticLiteLlmRequest {
        generation: SemanticProviderRequest {
            schema: "semantica.provider-request/1".into(),
            provider: "litellm".into(),
            interface_profile: "llms".into(),
            operation: if structured {
                "generate_structured"
            } else {
                "generate"
            }
            .into(),
            model: Some("openai/model".into()),
            custom_endpoint: false,
            prompt: "prompt".into(),
            options: json!({}),
        },
        config: json!({}),
    }
}

struct Case {
    id: &'static str,
    structured: bool,
    response: Value,
    expected: Option<Value>,
    error_contains: Option<&'static str>,
}

#[test]
fn original_frozen_twenty_wrapper_observations() {
    // This table follows FROZEN-ORACLE-EXPECTED.json and the original Python
    // 3.12 receipt, rather than BaseProvider's broader recovery algorithm.
    let cases = [
        Case {
            id: "dict_first_choice",
            structured: false,
            response: json!({"choices":[{"message":{"content":"dict"}}]}),
            expected: Some(json!("dict")),
            error_contains: None,
        },
        Case {
            id: "dict_empty_choices_content",
            structured: false,
            response: json!({"choices":[],"content":"fallback"}),
            expected: Some(json!("fallback")),
            error_contains: None,
        },
        Case {
            id: "string_response",
            structured: false,
            response: json!("text"),
            expected: Some(json!("text")),
            error_contains: None,
        },
        Case {
            id: "null_content_returned",
            structured: false,
            response: json!({"content":null}),
            expected: Some(Value::Null),
            error_contains: None,
        },
        Case {
            id: "unexpected_response",
            structured: false,
            response: json!([]),
            expected: None,
            error_contains: Some(
                "LiteLLM generation failed: Unexpected response format from LiteLLM: <class 'list'>",
            ),
        },
        Case {
            id: "structured_object",
            structured: true,
            response: json!("{\"a\":1}"),
            expected: Some(json!({"a":1})),
            error_contains: None,
        },
        Case {
            id: "structured_list",
            structured: true,
            response: json!("[1,2]"),
            expected: Some(json!([1, 2])),
            error_contains: None,
        },
        Case {
            id: "structured_scalar",
            structured: true,
            response: json!("false"),
            expected: Some(json!(false)),
            error_contains: None,
        },
        Case {
            id: "structured_fenced_object",
            structured: true,
            response: json!("```json\n{\"a\":1}\n```"),
            expected: Some(json!({"a":1})),
            error_contains: None,
        },
        Case {
            id: "structured_fenced_list_rejected",
            structured: true,
            response: json!("```json\n[1,2]\n```"),
            expected: None,
            error_contains: Some("Failed to parse JSON from LiteLLM response"),
        },
        Case {
            id: "greedy_multiple_objects_rejected",
            structured: true,
            response: json!("prefix {\"a\":1} and {\"b\":2} end"),
            expected: None,
            error_contains: Some("Extra data"),
        },
        Case {
            id: "trailing_comma_not_repaired",
            structured: true,
            response: json!("{\"a\":1,}"),
            expected: None,
            error_contains: Some("Expecting property name enclosed in double quotes"),
        },
        Case {
            id: "duplicate_json_keys_last_wins",
            structured: true,
            response: json!("{\"a\":1,\"a\":2}"),
            expected: Some(json!({"a":2})),
            error_contains: None,
        },
        Case {
            id: "structured_none_is_type_error",
            structured: true,
            response: json!({"content":null}),
            expected: None,
            error_contains: Some("must be str, bytes or bytearray, not NoneType"),
        },
    ];
    let mut checked = 0;
    for case in cases {
        let input = request(case.structured);
        let completion = prepare_completion_request(&input).unwrap();
        assert_eq!(completion.body["model"], "openai/model", "{}", case.id);
        assert_eq!(
            completion.body["messages"][0]["role"], "user",
            "{}",
            case.id
        );
        let result = decode_output(ResponseView::Json(&case.response), case.structured);
        match (case.expected, case.error_contains) {
            (Some(expected), None) => assert_eq!(result.unwrap(), expected, "{}", case.id),
            (None, Some(fragment)) => {
                let failure = result.unwrap_err();
                assert!(
                    failure.message.contains(fragment),
                    "{}: {}",
                    case.id,
                    failure.message
                );
            }
            _ => panic!("invalid frozen case {}", case.id),
        }
        checked += 1;
    }

    let mut config_override = request(false);
    config_override.config = json!({"temperature":0.8,"max_tokens":12});
    config_override.generation.options = json!({"temperature":0.2});
    let completion = prepare_completion_request(&config_override).unwrap();
    assert_eq!(
        completion.body,
        json!({"model":"openai/model",
        "messages":[{"role":"user","content":"prompt"}],"temperature":0.2,"max_tokens":12})
    );
    assert_eq!(
        decode_output(ResponseView::Json(&json!({"content":"ok"})), false).unwrap(),
        "ok"
    );
    checked += 1;

    let object_response = SemanticLiteLlmObjectResponse {
        choices: vec![
            SemanticLiteLlmObjectChoice {
                message: SemanticLiteLlmObjectMessage {
                    content: json!("first"),
                },
            },
            SemanticLiteLlmObjectChoice {
                message: SemanticLiteLlmObjectMessage {
                    content: json!("second"),
                },
            },
        ],
    };
    assert_eq!(
        decode_output(ResponseView::Object(&object_response), false).unwrap(),
        "first"
    );
    checked += 1;

    let rejected = completion_failure("synthetic SDK rejection", false);
    assert!(
        rejected
            .message
            .contains("LiteLLM generation failed: synthetic SDK rejection")
    );
    checked += 1;

    let mut duplicate = request(false);
    duplicate.generation.options = json!({"model":"other"});
    let mut dispatches = 0;
    let result = prepare_completion_request(&duplicate).map(|_| {
        dispatches += 1;
    });
    assert!(
        result
            .unwrap_err()
            .message
            .contains("multiple values for keyword argument 'model'")
    );
    assert_eq!(dispatches, 0);
    checked += 1;

    let mut json_prompt = request(true);
    json_prompt.generation.prompt = "JSON original".into();
    let completion = prepare_completion_request(&json_prompt).unwrap();
    assert_eq!(
        completion.body["messages"][0]["content"],
        "JSON original\n\nReturn the response as valid JSON only."
    );
    assert_eq!(
        decode_output(ResponseView::Json(&json!("{}")), true).unwrap(),
        json!({})
    );
    checked += 1;

    let unavailable = check_available(false).unwrap_err();
    assert_eq!(
        unavailable.message,
        "LiteLLM library not installed. Install with: pip install litellm"
    );
    assert_eq!(unavailable.code, "litellm_unavailable");
    assert!(check_available(true).is_ok());
    checked += 1;
    assert_eq!(checked, 20);
}

#[test]
fn shallow_merge_preserves_nested_values_nulls_and_rejects_reserved_keywords() {
    let mut input = request(false);
    input.config = json!({"nested":{"from_config":1,"replace":true},"temperature":0.8});
    input.generation.options =
        json!({"nested":{"from_call":2},"temperature":null,"unknown":[1,false]});
    let completion = prepare_completion_request(&input).unwrap();
    assert_eq!(completion.body["nested"], json!({"from_call":2}));
    assert_eq!(completion.body["temperature"], Value::Null);
    assert_eq!(completion.body["unknown"], json!([1, false]));
    assert_eq!(
        completion.body_sha256,
        semantica_runtime::sha256(
            semantica_contracts::json::ordered_json(&completion.body).unwrap()
        )
    );
    for keyword in ["model", "messages", "api_key"] {
        for configured in [false, true] {
            let mut input = request(false);
            if configured {
                input.config[keyword] = Value::Null;
            } else {
                input.generation.options[keyword] = Value::Null;
            }
            let failure = prepare_completion_request(&input).unwrap_err();
            assert_eq!(failure.code, format!("litellm_duplicate_{keyword}"));
            assert!(
                failure
                    .message
                    .contains(&format!("multiple values for keyword argument '{keyword}'"))
            );
        }
    }
}

#[test]
fn object_and_dictionary_response_branches_retain_their_original_difference() {
    let object = SemanticLiteLlmObjectResponse { choices: vec![] };
    let dictionary = json!({"choices":[],"content":"fallback"});
    assert_eq!(
        decode_output(ResponseView::Json(&dictionary), false).unwrap(),
        "fallback"
    );
    for choices in [json!({}), json!("")] {
        let plain = json!({"choices": choices, "content": "fallback"});
        assert_eq!(
            decode_output(ResponseView::Json(&plain), false).unwrap(),
            "fallback"
        );
        let structured = json!({"choices": choices, "content": "{\"a\":1}"});
        assert_eq!(
            decode_output(ResponseView::Json(&structured), true).unwrap(),
            json!({"a":1})
        );
    }
    for choices in [Value::Null, json!({"a":1}), json!("nonempty")] {
        let dictionary = json!({"choices": choices, "content": "fallback"});
        assert_eq!(
            decode_output(ResponseView::Json(&dictionary), false)
                .unwrap_err()
                .code,
            "litellm_choices_invalid"
        );
    }
    assert_eq!(
        decode_output(ResponseView::Object(&object), false)
            .unwrap_err()
            .code,
        "litellm_unexpected_response"
    );
    assert_eq!(
        decode_output(ResponseView::Object(&object), true)
            .unwrap_err()
            .code,
        "litellm_structured_json_unrecoverable"
    );
    let object = SemanticLiteLlmObjectResponse {
        choices: vec![SemanticLiteLlmObjectChoice {
            message: SemanticLiteLlmObjectMessage {
                content: Value::Null,
            },
        }],
    };
    assert_eq!(
        decode_output(ResponseView::Object(&object), false).unwrap(),
        Value::Null
    );
    assert_eq!(
        decode_output(ResponseView::Object(&object), true)
            .unwrap_err()
            .code,
        "litellm_structured_content_type"
    );
    let dictionary = json!({"choices":[{"message":{"content":"first"}}],"content":"fallback"});
    assert_eq!(
        decode_output(ResponseView::Json(&dictionary), false).unwrap(),
        "first"
    );
    assert_eq!(
        decode_output(
            ResponseView::Json(&json!({"choices":[{}],"content":"fallback"})),
            false
        )
        .unwrap_err()
        .code,
        "litellm_content_missing"
    );
}

#[test]
fn wrapper_structured_recovery_does_not_use_baseprovider_repairs() {
    for (text, expected) in [
        ("[1,2]", json!([1, 2])),
        ("false", json!(false)),
        ("null", Value::Null),
        ("42", json!(42)),
        ("\"hello\"", json!("hello")),
        ("preface\n{\"a\":1}\nafter", json!({"a":1})),
        ("```anything\n{\"a\":1}\n```", json!({"a":1})),
        ("{\"a\":1,\"a\":2}", json!({"a":2})),
    ] {
        assert_eq!(parse_structured_json(text).unwrap(), expected, "{text}");
    }
    for text in [
        "```json\n[1,2]\n```",
        "prefix [1,2] end",
        "{\"a\":1,}",
        "{\"a\":[1,2,]}",
        "prefix {\"a\":1} and {\"b\":2} end",
        "prefix {\"a\":[1,2]",
        "",
    ] {
        assert!(parse_structured_json(text).is_err(), "{text}");
    }
}

#[test]
fn completion_and_finite_http_protocol_have_separate_model_and_option_boundaries() {
    for provider in ["openai", "groq"] {
        let mut input = request(true);
        input.generation.model = Some(format!("{provider}/model"));
        input.config = json!({"temperature":0.3,"max_tokens":12});
        let completion = prepare_completion_request(&input).unwrap();
        assert_eq!(completion.body["model"], format!("{provider}/model"));
        let protocol = prepare_protocol_request(&input, &completion).unwrap();
        assert_eq!(protocol.provider, provider);
        assert_eq!(protocol.body["model"], "model");
        assert_eq!(protocol.body["messages"], completion.body["messages"]);
        assert!(protocol.body.get("response_format").is_none());
        assert_eq!(protocol.operation, "generate_structured");
    }
    let mut unsupported = request(false);
    unsupported.generation.model = Some("anthropic/model".into());
    let completion = prepare_completion_request(&unsupported).unwrap();
    assert_eq!(completion.body["model"], "anthropic/model");
    assert_eq!(
        prepare_protocol_request(&unsupported, &completion)
            .unwrap_err()
            .code,
        "litellm_sdk_provider_uncovered"
    );
    unsupported.generation.model = Some("openai/model".into());
    unsupported.generation.options = json!({"sdk_option":true});
    let completion = prepare_completion_request(&unsupported).unwrap();
    assert_eq!(completion.body["sdk_option"], true);
    assert_eq!(
        prepare_protocol_request(&unsupported, &completion)
            .unwrap_err()
            .code,
        "litellm_sdk_options_uncovered"
    );
    let mut mismatched = completion;
    mismatched.body["model"] = json!("openai/other");
    assert_eq!(
        prepare_protocol_request(&unsupported, &mismatched)
            .unwrap_err()
            .code,
        "litellm_completion_binding_invalid"
    );
}

#[test]
fn wrapper_result_uses_common_response_binding_and_preserves_unknown_usage() {
    let input = request(false);
    let completion = prepare_completion_request(&input).unwrap();
    let body = "{\"content\":null}".to_owned();
    let mut response = SemanticProviderWireResponse {
        body_sha256: semantica_runtime::sha256(body.as_bytes()),
        body,
        http_status: 200,
        transport_kind: "injected_fixture".into(),
        transport_identity: "litellm-wrapper-test/1".into(),
    };
    let output = decode_response(&input, &completion, &response).unwrap();
    assert_eq!(output.output, Value::Null);
    assert_eq!(output.provider, "litellm");
    assert_eq!(output.requested_model.as_deref(), Some("openai/model"));
    assert_eq!(output.usage_status, "unknown");
    assert_eq!(output.wire_request_sha256, completion.body_sha256);
    let protocol = prepare_protocol_request(&input, &completion).unwrap();
    let actual = decode_protocol_response(&input, &completion, &protocol, &response).unwrap();
    assert_eq!(actual.provider, protocol.provider);
    assert_eq!(completion.provider, "litellm");
    assert_eq!(actual.requested_model.as_deref(), Some("openai/model"));
    assert_eq!(actual.wire_request_sha256, protocol.body_sha256);
    assert_ne!(actual.wire_request_sha256, completion.body_sha256);

    // These replacements are individually self-consistent requests, including
    // their hashes, but are not the requests prepared for this logical input.
    let mut replaced_completion = completion.clone();
    replaced_completion.body["model"] = json!("openai/other");
    replaced_completion.body_sha256 = semantica_runtime::sha256(
        semantica_contracts::json::ordered_json(&replaced_completion.body).unwrap(),
    );
    assert_eq!(
        decode_response(&input, &replaced_completion, &response)
            .unwrap_err()
            .code,
        "litellm_completion_binding_invalid"
    );
    assert_eq!(
        decode_protocol_response(&input, &replaced_completion, &protocol, &response)
            .unwrap_err()
            .code,
        "litellm_completion_binding_invalid"
    );
    let mut replaced_protocol = protocol.clone();
    replaced_protocol.body["model"] = json!("other");
    replaced_protocol.body_sha256 = semantica_runtime::sha256(
        semantica_contracts::json::ordered_json(&replaced_protocol.body).unwrap(),
    );
    assert_eq!(
        decode_protocol_response(&input, &completion, &replaced_protocol, &response)
            .unwrap_err()
            .code,
        "litellm_protocol_binding_invalid"
    );

    response.body_sha256 = "0".repeat(64);
    assert_eq!(
        decode_response(&input, &completion, &response)
            .unwrap_err()
            .code,
        "provider_response_binding_invalid"
    );
    assert_eq!(
        completion_failure("safe transport category", false).message,
        "LiteLLM generation failed: safe transport category"
    );
}
