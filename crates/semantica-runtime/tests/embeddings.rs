//! Fixed original source-derived arithmetic/protocol oracles, not inference proof.
use semantica_contracts::*;
use semantica_runtime::{embeddings::*, providers::*};
use serde_json::{Value, json};
fn request() -> SemanticEmbeddingRequest {
    serde_json::from_value(json!({"schema":"semantica.embedding-request/1","provider":"openai",
        "interface_profile":"provider_store","model":null,"custom_endpoint":false,"input":"literal input",
        "options":{"model":"synthetic-embed","ignored":42}})).unwrap()
}
fn response(body: &str) -> SemanticProviderWireResponse {
    SemanticProviderWireResponse {
        http_status: 200,
        body: body.into(),
        body_sha256: semantica_runtime::sha256(body.as_bytes()),
        transport_kind: "injected_fixture".into(),
        transport_identity: "embedding-source-oracles/1".into(),
    }
}
#[test]
fn original_openai_store_payload_and_binary32_casts_are_exact() {
    let input = request();
    let wire = prepare_embedding_request(&input).unwrap();
    assert_eq!(
        wire.body,
        json!({"model":"synthetic-embed","input":"literal input"})
    );
    assert_eq!(wire.operation, "embed");
    let raw = r#"{"model":"synthetic-resolved","data":[{"embedding":[0.1,-0.2,0.25,-0.0,1e100,"NaN","-Infinity",null,true]},{"embedding":[42]}],"usage":{"prompt_tokens":3,"total_tokens":3}}"#;
    let result = decode_embedding_response(&input, &wire, &response(raw)).unwrap();
    let tensor: SemanticEmbeddingTensor = serde_json::from_value(result.output.clone()).unwrap();
    assert_eq!(tensor.dtype, "float32");
    assert_eq!(tensor.shape, vec![9]);
    // Independent IEEE754 bits frozen before implementation, not f64 approximations.
    assert_eq!(
        tensor.bits,
        vec![
            1036831949, 3192704205, 1048576000, 2147483648, 2139095040, 2143289344, 4286578688,
            2143289344, 1065353216
        ]
    );
    assert_eq!(result.requested_model.as_deref(), Some("synthetic-embed"));
    assert_eq!(result.resolved_model.as_deref(), Some("synthetic-resolved"));
    assert_eq!(result.usage.input_tokens.as_deref(), Some("3"));
    assert_eq!(result.usage.output_tokens, None);
    assert_eq!(result.usage_status, "partial");
    let mut input = input;
    input.options = json!({"model":null,"dimensions":8});
    assert_eq!(
        prepare_embedding_request(&input).unwrap().body,
        json!({"model":null,"input":"literal input"})
    );
    input.options = json!({});
    input.input = "".into();
    assert_eq!(
        prepare_embedding_request(&input).unwrap().body,
        json!({"model":"text-embedding-3-small","input":""})
    );
}
#[test]
fn embedding_reply_and_request_bytes_cannot_drift_or_hide_invalid_casts() {
    let input = request();
    let wire = prepare_embedding_request(&input).unwrap();
    let mut reply = response(r#"{"data":[{"embedding":[0.25]}]}"#);
    reply.body.push(' ');
    assert_eq!(
        decode_embedding_response(&input, &wire, &reply)
            .unwrap_err()
            .code,
        "provider_response_binding_invalid"
    );
    let mut changed = wire.clone();
    changed.body["input"] = json!("other");
    changed.body_sha256 = semantica_runtime::sha256(serde_json::to_vec(&changed.body).unwrap());
    assert_eq!(
        decode_embedding_response(
            &input,
            &changed,
            &response(r#"{"data":[{"embedding":[0.25]}]}"#)
        )
        .unwrap_err()
        .code,
        "embedding_request_binding_invalid"
    );
    for raw in [
        r#"{"data":[]}"#,
        r#"{"data":[{"embedding":["not a number"]}]}"#,
        r#"{"data":[{"embedding":[[1],[2,3]]}]}"#,
    ] {
        assert!(decode_embedding_response(&input, &wire, &response(raw)).is_err());
    }
    // Explicit remaining parser gap; refusal is not original NaN-literal parity.
    assert!(
        decode_embedding_response(
            &input,
            &wire,
            &response(r#"{"data":[{"embedding":[NaN]}]}"#)
        )
        .is_err()
    );
    assert!(
        decode_embedding_response(
            &input,
            &wire,
            &response(r#"{"data":[{"embedding":[1e999]}]}"#)
        )
        .is_err()
    );
}
#[test]
fn original_sequential_array_stacking_keeps_empty_dtype_and_ragged_failure() {
    let empty = stack_embedding_results(&[]).unwrap();
    assert_eq!(empty.dtype, "float64");
    assert_eq!(empty.shape, vec![0]);
    assert!(empty.bits.is_empty());
    let input = request();
    let wire = prepare_embedding_request(&input).unwrap();
    let a = decode_embedding_response(
        &input,
        &wire,
        &response(r#"{"data":[{"embedding":[0.25,-0.0]}]}"#),
    )
    .unwrap();
    let b = decode_embedding_response(
        &input,
        &wire,
        &response(r#"{"data":[{"embedding":[-0.25,1]}]}"#),
    )
    .unwrap();
    let tensor = stack_embedding_results(&[a.clone(), b]).unwrap();
    assert_eq!(tensor.shape, vec![2, 2]);
    assert_eq!(tensor.dtype, "float32");
    assert_eq!(
        tensor.bits,
        vec![1048576000, 2147483648, 3196059648, 1065353216]
    );
    let c = decode_embedding_response(&input, &wire, &response(r#"{"data":[{"embedding":[1]}]}"#))
        .unwrap();
    assert_eq!(
        stack_embedding_results(&[a, c]).unwrap_err().code,
        "embedding_array_ragged"
    );
}
fn pool_request(
    strategy: &str,
    embeddings: Vec<Vec<f64>>,
    chunk: Option<u64>,
) -> SemanticEmbeddingPoolRequest {
    SemanticEmbeddingPoolRequest {
        schema: "semantica.embedding-pool-request/1".into(),
        strategy: strategy.into(),
        embeddings,
        chunk_size: chunk,
    }
}
#[test]
fn original_five_pool_oracles_keep_short_final_chunk_equally_weighted() {
    let rows = vec![vec![1.0, 2.0], vec![3.0, 4.0], vec![9.0, 10.0]];
    for (name, expected) in [
        ("mean", vec![13.0 / 3.0, 16.0 / 3.0]),
        ("max", vec![9.0, 10.0]),
        ("cls", vec![1.0, 2.0]),
        ("Hierarchical", vec![5.5, 6.5]),
    ] {
        let input = pool_request(name, rows.clone(), Some(2));
        let result = pool_embeddings(&input).unwrap();
        assert_eq!(result.output, expected);
        assert_eq!(result.profile, "semantica.provided-vectors-finite-f64-2d/1");
        assert_eq!(
            result.request_sha256,
            semantica_runtime::sha256(serde_json::to_vec(&input).unwrap())
        );
    }
    let attention = pool_embeddings(&pool_request(
        "attention",
        vec![vec![1.0, 0.0], vec![0.0, 2.0]],
        None,
    ))
    .unwrap();
    for (actual, expected) in attention
        .output
        .iter()
        .zip([0.18242552380635635, 1.6351489523872873])
    {
        assert!((actual - expected).abs() < 1e-14);
    }
}
#[test]
fn original_pool_failure_boundaries_and_explicit_finite_domain_are_checked() {
    for strategy in ["mean", "max", "cls", "attention", "hierarchical"] {
        assert_eq!(
            pool_embeddings(&pool_request(strategy, vec![], None))
                .unwrap_err()
                .code,
            "cannot_pool_empty_embeddings"
        );
    }
    assert_eq!(
        pool_embeddings(&pool_request("hierarchical", vec![vec![1.0]], Some(0)))
            .unwrap_err()
            .code,
        "pool_chunk_size_invalid"
    );
    assert_eq!(
        pool_embeddings(&pool_request("other", vec![vec![1.0]], None))
            .unwrap_err()
            .code,
        "unsupported_pooling_strategy"
    );
    for rows in [
        vec![vec![1.0], vec![2.0, 3.0]],
        vec![vec![f64::NAN]],
        vec![vec![f64::INFINITY]],
        vec![vec![]],
    ] {
        assert!(pool_embeddings(&pool_request("mean", rows, None)).is_err());
    }
    assert!(
        pool_embeddings(&pool_request(
            "attention",
            vec![vec![f64::MAX], vec![f64::MAX]],
            None
        ))
        .is_err()
    );
}
#[test]
fn original_base_prompt_is_case_sensitive_and_json_truthiness_is_preserved() {
    assert_eq!(
        base_structured_prompt("please json"),
        "please json\n\nReturn the response as valid JSON only."
    );
    assert_eq!(base_structured_prompt("please JSON"), "please JSON");
    for value in [
        Value::Null,
        json!(false),
        json!(0),
        json!(-0.0),
        json!(""),
        json!([]),
        json!({}),
    ] {
        assert!(!json_truthy(&value));
    }
    for value in [
        json!(true),
        json!(1),
        json!(-2),
        json!("x"),
        json!([0]),
        json!({"x":false}),
    ] {
        assert!(json_truthy(&value));
    }
}
