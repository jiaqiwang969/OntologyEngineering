//! Public independent frozen mathematical oracle; real model assets are private.
use semantica_contracts::*;
use semantica_runtime::{books::SourceProvider, native_models::*};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    path::{Path, PathBuf},
};
fn oracle() -> Value {
    let raw = include_bytes!("fixtures/native-cpu-oracles.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "87942a1f76104b6ad4b545db1e25b660674fde8c6f9ecd25c566745d200329ee"
    );
    serde_json::from_slice(raw).unwrap()
}
fn root() -> PathBuf {
    std::env::var_os("SEMANTICA_NATIVE_FIXTURE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("sources/native-models")
        })
}
struct Files;
impl SourceProvider for Files {
    fn read(&self, id: &str) -> anyhow::Result<Vec<u8>> {
        Ok(std::fs::read(root().join(id))?)
    }
}
fn request(
    name: &str,
    operation: &str,
    profile: &str,
    texts: Vec<String>,
    batch: bool,
) -> SemanticNativeRequest {
    SemanticNativeRequest {
        schema: "semantica.native-request/1".into(),
        model: serde_json::from_value(oracle()["model_specs"][name].clone()).unwrap(),
        operation: operation.into(),
        interface_profile: profile.into(),
        batch,
        texts,
        normalize: true,
        options: json!({}),
        seed: 77,
    }
}
fn values(output: &Value) -> Vec<f32> {
    output["bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| f32::from_bits(v.as_u64().unwrap() as u32))
        .collect()
}
fn compare(values: &[f32], gold: &[f64], tolerance: f64) {
    assert_eq!(values.len(), gold.len());
    let max = values
        .iter()
        .zip(gold)
        .map(|(a, b)| (*a as f64 - b).abs())
        .fold(0.0, f64::max);
    println!(
        "weighted comparison: count={}, max_abs={max}, frozen_tolerance={tolerance}",
        values.len()
    );
    assert!(
        max <= tolerance,
        "independent fixed tolerance exceeded: {max} > {tolerance}"
    );
}
fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()))
        .collect()
}

#[test]
fn native_source_contract_refuses_missing_pin_before_read() {
    struct Never;
    impl SourceProvider for Never {
        fn read(&self, _: &str) -> anyhow::Result<Vec<u8>> {
            panic!("invalid pin must fail before source read")
        }
    }
    let mut spec = request(
        "bge-small",
        "embed",
        "text_embedder",
        vec!["text".into()],
        false,
    )
    .model;
    spec.assets[0].sha256 = String::new();
    assert!(verify_model_assets(&Never, &spec).is_err());
    let mut late = request(
        "bge-small",
        "embed",
        "text_embedder",
        vec!["valid".into()],
        false,
    )
    .model;
    late.assets.last_mut().unwrap().sha256 = String::new();
    assert!(verify_model_assets(&Never, &late).is_err());
    let mut wrong_device = request(
        "bge-small",
        "embed",
        "text_embedder",
        vec!["valid".into()],
        false,
    );
    wrong_device.model.device = "cuda".into();
    assert!(execute_native(&Never, &wrong_device).is_err());
    let mut unimplemented = request(
        "bge-small",
        "embed",
        "text_embedder",
        vec!["valid".into()],
        false,
    );
    unimplemented.options = json!({"prompt":"cannot silently discard this original API option"});
    assert_eq!(
        execute_native(&Never, &unimplemented).unwrap_err().code,
        "native_embedding_options_unsupported"
    );
    let input = request(
        "bge-small",
        "embed",
        "text_embedder",
        vec![" \t".into()],
        false,
    );
    assert_eq!(
        execute_native(&Never, &input).unwrap_err().code,
        "native_embedding_text_invalid"
    );
    let empty = request("bge-small", "embed", "text_embedder", vec![], true);
    assert_eq!(
        execute_native(&Never, &empty).unwrap().output,
        json!({"dtype":"float64","shape":[0],"bits":[]})
    );
}
#[test]
fn original_hf_kwargs_unicode_and_structured_parse_are_exact() {
    let p = hf_generation_parameters(
        &json!({"max_length":7,"max_new_tokens":3,"max_tokens":9}),
        4,
        20,
    )
    .unwrap();
    assert_eq!(p["max_new_tokens"], 3);
    assert_eq!(p["length_source"], "max_length");
    assert_eq!(p["temperature"], 0.7);
    assert_eq!(p["do_sample"], true);
    assert_eq!(
        hf_generation_parameters(&json!({"max_new_tokens":4,"max_tokens":9}), 2, 20).unwrap()["max_new_tokens"],
        4
    );
    assert_eq!(
        hf_generation_parameters(&json!({"max_tokens":3}), 2, 20).unwrap()["max_new_tokens"],
        3
    );
    assert_eq!(
        hf_generation_parameters(&json!({}), 2, 20).unwrap()["max_new_tokens"],
        18
    );
    assert!(hf_generation_parameters(&json!({"max_length":2}), 2, 20).is_err());
    assert!(hf_generation_parameters(&json!({"temperature":0}), 2, 20).is_err());
    assert_eq!(hf_decoded_continuation("中 Xmore", "中X"), "Xmore");
    assert_eq!(
        hf_structured_response("answer {\"x\":1} trailing").unwrap(),
        json!({"x":1})
    );
    assert_eq!(hf_structured_response("[1,2]").unwrap(), json!([1, 2]));
    assert!(hf_structured_response("preface [1,2] trailing").is_err());
    assert!(hf_structured_response("{\"x\":1,}").is_err());
}
#[test]
#[ignore = "requires registered exact public model bytes; explicitly run for native acceptance"]
fn actual_bert_weights_token_masks_and_intrinsic_normalize_match_frozen_reference() {
    let gold = oracle();
    let texts = gold["models"]["bge-small"]["texts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    let mut req = request("bge-small", "embed", "text_embedder", texts, true);
    for normalize in [true, false] {
        req.normalize = normalize;
        let result = execute_native(&Files, &req).unwrap();
        assert_eq!(result.output["shape"], json!([4, 384]));
        assert_eq!(
            result.token_inputs["input_ids"],
            gold["models"]["bge-small"]["input_ids"]
        );
        assert_eq!(
            result.token_inputs["attention_mask"],
            gold["models"]["bge-small"]["attention_mask"]
        );
        assert_eq!(
            result.token_inputs["token_type_ids"],
            gold["models"]["bge-small"]["token_type_ids"]
        );
        assert_eq!(result.parameters["intrinsic_normalize"], true);
        compare(
            &values(&result.output),
            &floats(&gold["models"]["bge-small"]["sentence_embeddings_normalized"]),
            gold["tolerance"]["bert_embedding_abs"].as_f64().unwrap(),
        );
        assert_eq!(result.usage_status, "partial");
        assert!(result.usage.output_tokens.is_none());
    }
}
#[test]
#[ignore = "requires registered exact public model bytes; explicitly run for native acceptance"]
fn native_asset_sha_and_end_drift_are_refused() {
    struct Drift {
        reads: Cell<usize>,
    }
    impl SourceProvider for Drift {
        fn read(&self, id: &str) -> anyhow::Result<Vec<u8>> {
            let mut bytes = Files.read(id)?;
            if id == "bge-small/config.json" {
                let n = self.reads.get();
                self.reads.set(n + 1);
                if n > 0 {
                    bytes.push(b' ');
                }
            }
            Ok(bytes)
        }
    }
    let req = request(
        "bge-small",
        "embed",
        "text_embedder",
        vec!["actual weighted forward".into()],
        false,
    );
    let source = Drift {
        reads: Cell::new(0),
    };
    let error = execute_native(&source, &req).unwrap_err();
    assert_eq!(error.code, "native_asset_identity_mismatch");
    assert_eq!(source.reads.get(), 2);
    let mut changed = req.clone();
    changed.model.assets[0].sha256 = "0".repeat(64);
    assert_eq!(
        execute_native(&Files, &changed).unwrap_err().code,
        "native_asset_identity_mismatch"
    );
}
#[test]
#[ignore = "requires registered exact public model bytes; explicitly run for native acceptance"]
fn actual_llama_weighted_logits_and_unicode_controls_match_frozen_reference() {
    let gold = oracle();
    for case in gold["models"]["smollm-135m"]["cases"].as_array().unwrap() {
        let prompt = case["prompt"].as_str().unwrap();
        let req = request(
            "smollm-135m",
            "causal_logits",
            "native_validation",
            vec![prompt.into()],
            false,
        );
        let result = execute_native(&Files, &req).unwrap();
        assert_eq!(result.token_inputs["input_ids"], json!([case["input_ids"]]));
        let expected = case["logits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>();
        compare(
            &values(&result.output),
            &expected,
            gold["tolerance"]["llama_logits_abs"].as_f64().unwrap(),
        );
        let mut req = request(
            "smollm-135m",
            "argmax_validation",
            "native_validation",
            vec![prompt.into()],
            false,
        );
        req.options = json!({"max_new_tokens":3});
        let result = execute_native(&Files, &req).unwrap();
        assert_eq!(result.output, case["python_character_continuation"]);
        assert_eq!(
            result.parameters["generated_ids"],
            json!(
                case["argmax_control_ids"].as_array().unwrap()
                    [case["input_ids"].as_array().unwrap().len()..]
            )
        );
        assert_eq!(result.parameters["do_sample"], false);
        assert_eq!(result.parameters["sampling"], "explicit_argmax_validation");
    }
}
#[test]
#[ignore = "requires registered exact public model bytes; explicitly run for native acceptance"]
fn actual_causal_sampling_keeps_original_top_k_and_new_rng_identity() {
    let gold = oracle();
    let case = &gold["models"]["smollm-135m"]["cases"][0];
    let mut req = request(
        "smollm-135m",
        "generate",
        "huggingface_llm",
        vec![case["prompt"].as_str().unwrap().into()],
        false,
    );
    req.options = json!({"max_new_tokens":2,"do_sample":false,"top_k":1}); // original wrapper ignores these two overrides
    let result = execute_native(&Files, &req).unwrap();
    assert_eq!(result.parameters["do_sample"], true);
    assert_eq!(result.parameters["top_k"], 50);
    assert_eq!(result.parameters["sampling"], "candle-0.9.1-StdRng-TopK/1");
    let id = result.parameters["generated_ids"][0].as_u64().unwrap() as usize;
    let expected = case["logits"].as_array().unwrap();
    let mut ordered = (0..expected.len()).collect::<Vec<_>>();
    ordered.sort_by(|a, b| {
        expected[*b]
            .as_f64()
            .unwrap()
            .total_cmp(&expected[*a].as_f64().unwrap())
    });
    assert!(ordered[..50].contains(&id));
    assert_eq!(result.usage_status, "known");
}

struct Patched(std::collections::BTreeMap<String, Vec<u8>>);
impl SourceProvider for Patched {
    fn read(&self, id: &str) -> anyhow::Result<Vec<u8>> {
        if let Some(bytes) = self.0.get(id) {
            Ok(bytes.clone())
        } else {
            Files.read(id)
        }
    }
}
fn replace_asset(req: &mut SemanticNativeRequest, role: &str, value: &Value, source: &mut Patched) {
    let entry = req
        .model
        .assets
        .iter_mut()
        .find(|a| a.role == role)
        .unwrap();
    let bytes = serde_json::to_vec(value).unwrap();
    entry.byte_count = bytes.len() as u64;
    entry.sha256 = format!("{:x}", Sha256::digest(&bytes));
    source.0.insert(entry.source_id.clone(), bytes);
    req.model.model_id = "synthetic-control/bge-actual-weights-pipeline".into();
    req.model.revision = "frozen-operator-control/1".into();
}
#[test]
#[ignore = "requires registered exact public model bytes; explicitly run for native acceptance"]
fn actual_bge_store_forces_normalize_without_intrinsic_pipeline_layer() {
    let gold = oracle();
    let texts = gold["models"]["bge-small"]["texts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    let mut req = request("bge-small", "embed", "bge_store", texts, true);
    req.normalize = false;
    req.options = json!({"precision":"int8","batch_size":1,"prompt":"ignored original BGEStore option","normalize_embeddings":false});
    let modules: Value =
        serde_json::from_slice(&Files.read("bge-small/modules.json").unwrap()).unwrap();
    let mut source = Patched(Default::default());
    replace_asset(
        &mut req,
        "modules",
        &json!(modules.as_array().unwrap()[..2]),
        &mut source,
    );
    let result = execute_native(&source, &req).unwrap();
    assert_eq!(result.parameters["intrinsic_normalize"], false);
    assert_eq!(result.parameters["batch_size"], 32);
    compare(
        &values(&result.output),
        &floats(&gold["models"]["bge-small"]["sentence_embeddings_normalized"]),
        gold["tolerance"]["bert_embedding_abs"].as_f64().unwrap(),
    );
    req.interface_profile = "text_embedder".into();
    req.options = json!({});
    let result = execute_native(&source, &req).unwrap();
    compare(
        &values(&result.output),
        &floats(&gold["models"]["bge-small"]["raw_cls"]),
        gold["tolerance"]["bert_embedding_abs"].as_f64().unwrap(),
    );
}
#[test]
#[ignore = "requires registered exact public model bytes; explicitly run for native acceptance"]
fn actual_masked_mean_pooling_matches_independent_weighted_reference() {
    let gold = oracle();
    let texts = gold["models"]["bge-small"]["texts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    let mut req = request("bge-small", "embed", "text_embedder", texts, true);
    let mut pool: Value =
        serde_json::from_slice(&Files.read("bge-small/1_Pooling/config.json").unwrap()).unwrap();
    pool["pooling_mode_cls_token"] = false.into();
    pool["pooling_mode_mean_tokens"] = true.into();
    let mut source = Patched(Default::default());
    replace_asset(&mut req, "pooling", &pool, &mut source);
    let result = execute_native(&source, &req).unwrap();
    assert_eq!(result.parameters["pooling"], "masked_mean");
    compare(
        &values(&result.output),
        &floats(&gold["models"]["bge-small"]["masked_mean_normalized"]),
        gold["tolerance"]["bert_embedding_abs"].as_f64().unwrap(),
    );
}
