use semantica_contracts::*;
use semantica_runtime::providers::*;
use serde_json::{Value, json};
use std::cell::Cell;
fn request(provider: &str, structured: bool) -> SemanticProviderRequest {
    serde_json::from_value(json!({"schema":"semantica.provider-request/1","provider":provider,
        "interface_profile":"semantic_extract","operation":if structured {"generate_structured"} else {"generate"},
        "model":null,"custom_endpoint":false,"prompt":"Find entities.","options":{}})).unwrap()
}
#[test]
fn original_openai_and_groq_payloads_preserve_protocol() {
    let mut input = request("openai", true);
    input.options = json!({"temperature":null,"max_completion_tokens":65536,"top_p":0.9,
        "unknown":9,"logit_bias":{"1":1},"model":"call-model","max_tokens":2048,"stop":["a"]});
    let standard = prepare_request(&input).unwrap();
    assert_eq!(
        standard.body,
        json!({"model":"call-model","messages":[{"role":"user","content":"Find entities."}],
        "response_format":{"type":"json_object"},"max_completion_tokens":65536,"max_tokens":2048,"top_p":0.9,"stop":["a"],"logit_bias":{"1":1}})
    );
    input.custom_endpoint = true;
    let custom = prepare_request(&input).unwrap();
    assert!(custom.body.get("response_format").is_none());
    assert!(custom.body.get("temperature").is_none());
    assert_eq!(custom.body["max_completion_tokens"], 65536);
    assert_eq!(
        custom.body_sha256,
        semantica_runtime::sha256(serde_json::to_vec(&custom.body).unwrap())
    );
    let mut groq = request("groq", true);
    groq.options = json!({"logit_bias":{"1":1},"temperature":0.4,"max_tokens":12345});
    let wire = prepare_request(&groq).unwrap();
    assert_eq!(
        wire.body,
        json!({"model":"llama-3.3-70b-versatile","messages":[{"role":"user","content":"Find entities.\n\nReturn the response as valid JSON only."}],
        "response_format":{"type":"json_object"},"temperature":0.4,"max_tokens":12345})
    );
    groq.interface_profile = "llms".into();
    groq.prompt = "Return JsOn please".into();
    let wire = prepare_request(&groq).unwrap();
    assert_eq!(wire.body["model"], "llama-3.1-8b-instant");
    assert_eq!(wire.body["messages"][0]["content"], "Return JsOn please");
    let plain = prepare_request(&request("openai", false)).unwrap();
    assert_eq!(plain.body["model"], "gpt-3.5-turbo");
    assert!(plain.body.get("response_format").is_none());
}
#[test]
fn original_json_recovery_oracles_keep_scalars_and_failure_boundary() {
    let cases = [
        (
            "```json\n[{\"label\":\"中文\"}]\n```",
            json!([{"label":"中文"}]),
        ),
        ("Prelude {\"items\":[1,2,],} after", json!({"items":[1,2]})),
        ("Prelude {\"items\":[1,2]", json!({"items":[1,2]})),
        ("false", json!(false)),
        ("42", json!(42)),
        ("null", Value::Null),
        ("{\"x\":1,\"x\":2}", json!({"x":2})),
    ];
    for (source, expected) in cases {
        assert_eq!(parse_structured_json(source).unwrap(), expected, "{source}");
    }
    for source in [
        "",
        "<html>not json</html>",
        "{\"items\":1",
        "NaN",
        "Infinity",
    ] {
        assert!(parse_structured_json(source).is_err(), "{source}");
    }
}
struct Fixture {
    calls: Cell<usize>,
    body: String,
}
impl SemanticProviderTransportPort for Fixture {
    fn post(
        &self,
        _: &SemanticProviderWireRequest,
    ) -> Result<SemanticProviderWireResponse, SemanticProviderFailure> {
        self.calls.set(self.calls.get() + 1);
        Ok(SemanticProviderWireResponse {
            http_status: 200,
            body: self.body.clone(),
            body_sha256: semantica_runtime::sha256(self.body.as_bytes()),
            transport_kind: "injected_fixture".into(),
            transport_identity: "provider-test/1".into(),
        })
    }
}
impl SemanticProviderResultGuardPort for Fixture {
    fn check_result(
        &self,
        _: &SemanticProviderResult,
    ) -> Result<SemanticProviderResultGuard, SemanticProviderFailure> {
        Ok(SemanticProviderResultGuard {
            schema: "semantica.provider-result-guard/1".into(),
            credential_material_absent: true,
        })
    }
}
#[test]
fn one_call_keeps_nullable_empty_scalar_unknown_and_large_usage() {
    for (operation, content, expected) in [
        (false, Value::Null, Value::Null),
        (false, json!(""), json!("")),
        (true, json!("false"), json!(false)),
    ] {
        let fixture = Fixture {
            calls: Cell::new(0),
            body: json!({"choices":[{"message":{"content":content}}]}).to_string(),
        };
        let result = RustProviderRuntime::new(&fixture, &fixture)
            .generate(&request("openai", operation))
            .unwrap();
        assert_eq!(result.output, expected);
        assert_eq!(result.usage_status, "unknown");
        assert_eq!(result.usage.input_tokens, None);
        assert_eq!(fixture.calls.get(), 1);
        assert_eq!(result.transport_kind, "injected_fixture");
    }
    let fixture = Fixture { calls:Cell::new(0),body:r#"{"model":"resolved","choices":[{"message":{"content":"ok"}}],"usage":{"prompt_tokens":340282366920938463463374607431768211456,"completion_tokens":0,"total_tokens":340282366920938463463374607431768211456}}"#.into() };
    let result = RustProviderRuntime::new(&fixture, &fixture)
        .generate(&request("groq", false))
        .unwrap();
    assert_eq!(
        result.usage.input_tokens.as_deref(),
        Some("340282366920938463463374607431768211456")
    );
    assert_eq!(result.usage.output_tokens.as_deref(), Some("0"));
    assert_eq!(result.usage_status, "reported");
    assert_eq!(
        result.requested_model.as_deref(),
        Some("llama-3.3-70b-versatile")
    );
    assert_eq!(result.resolved_model.as_deref(), Some("resolved"));
    assert_eq!(fixture.calls.get(), 1);
}
#[test]
fn invalid_usage_preserves_partial_knowledge_without_zero_estimates() {
    let (usage, status) = response_usage(
        &json!({"usage":{"prompt_tokens":-1,"completion_tokens":5,"total_tokens":null}}),
    );
    assert_eq!(usage.input_tokens, None);
    assert_eq!(usage.output_tokens.as_deref(), Some("5"));
    assert_eq!(status, "invalid");
    let (usage, status) = response_usage(&json!({"usage":{"prompt_tokens":0}}));
    assert_eq!(usage.input_tokens.as_deref(), Some("0"));
    assert_eq!(status, "partial");
}
#[test]
fn malformed_request_and_changed_response_binding_fail_explicitly() {
    let mut input = request("openai", false);
    input.options = json!([]);
    assert_eq!(
        prepare_request(&input).unwrap_err().code,
        "provider_request_invalid"
    );
    let input = request("openai", false);
    let wire = prepare_request(&input).unwrap();
    let response = SemanticProviderWireResponse {
        http_status: 200,
        body: "{}".into(),
        body_sha256: "0".repeat(64),
        transport_kind: "fixture".into(),
        transport_identity: "test".into(),
    };
    assert_eq!(
        decode_response(&input, &wire, &response).unwrap_err().code,
        "provider_response_binding_invalid"
    );
}

#[test]
fn original_generic_fence_and_closure_order_do_not_infer_new_success() {
    assert!(parse_structured_json("Prelude {\"items\":[{\"x\":1}").is_err());
    assert_eq!(
        parse_structured_json("```\n{\"broken\":\n```\n```\n{\"ok\":1}\n```").unwrap(),
        json!({"ok":1})
    );
}

#[test]
fn original_json_literal_object_cannot_impersonate_internal_number() {
    let expected = json!({"$serde_json::private::Number":"42"});
    let result = parse_structured_json(r#"{"$serde_json::private::Number":"42"}"#).unwrap();
    assert_eq!(result, expected);
}
