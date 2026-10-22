//! Synthetic construction boundaries test the unchanged original factory
//! selection/reuse contract, not provider SDK initialization or inference.
use semantica_contracts::*;
use semantica_runtime::providers::factory::*;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct ConstructionBoundary {
    calls: RefCell<Vec<SemanticProviderConstructionRequest>>,
    failing: Cell<bool>,
    invalid: Cell<bool>,
    available: Cell<Option<bool>>,
}
impl SemanticProviderConstructionPort for ConstructionBoundary {
    fn construct(
        &self,
        input: &SemanticProviderConstructionRequest,
    ) -> Result<SemanticProviderInstance, SemanticProviderFactoryFailure> {
        self.calls.borrow_mut().push(input.clone());
        if self.failing.get() {
            return Err(failure(
                "injected_constructor_failed",
                "construct",
                "RuntimeError",
            ));
        }
        let field = if input.backend == "huggingface_llm" {
            "model_name"
        } else {
            "model"
        };
        let model = match input.kwargs.get(field) {
            None => builtin_default_model(&input.backend).map(str::to_owned),
            Some(Value::Null) => None,
            Some(Value::String(value)) => Some(value.clone()),
            _ => None, // arbitrary constructor/model domains are not certified
        };
        Ok(SemanticProviderInstance {
            instance_id: if self.invalid.get() {
                "wrong-instance".into()
            } else {
                input.instance_id.clone()
            },
            backend: input.backend.clone(),
            constructor_id: input.constructor_id.clone(),
            provider_name: input.name.clone(),
            default_model: model,
            available: self.available.get(),
            configuration_sha256: input.configuration_sha256.clone(),
        })
    }
}
fn create(
    factory: &RustProviderFactory<'_>,
    name: &str,
    kwargs: Value,
) -> SemanticProviderInstance {
    factory
        .create_provider(&json!(name), &kwargs, "json/1", true)
        .unwrap()
        .0
}
fn scalar(value: Value) -> Value {
    json!({"kind":"scalar","value":value})
}

#[test]
fn original_factory_registry_and_pool() {
    // Fifteen observations from the original independently frozen receipt.
    let state = RefCell::new(empty_state());
    let boundary = ConstructionBoundary::default();
    let factory = RustProviderFactory::new(&state, &boundary);
    let first = create(
        &factory,
        "openai",
        json!({"api_key":"synthetic-one","model":"model"}),
    );
    assert_eq!(
        first.instance_id,
        create(
            &factory,
            "openai",
            json!({"model":"model","api_key":"synthetic-one"})
        )
        .instance_id
    );
    let nested = create(
        &factory,
        "openai",
        json!({"api_key":"synthetic-one","nested":{"b":[1,2],"a":{"y":2,"x":1}}}),
    );
    assert_eq!(
        nested.instance_id,
        create(
            &factory,
            "openai",
            json!({"nested":{"a":{"x":1,"y":2},"b":[1,2]},"api_key":"synthetic-one"})
        )
        .instance_id
    );
    assert_ne!(
        first.instance_id,
        create(
            &factory,
            "openai",
            json!({"api_key":"synthetic-one","model":"model","irrelevant":1})
        )
        .instance_id
    );
    assert_ne!(
        first.instance_id,
        create(
            &factory,
            "openai",
            json!({"api_key":"synthetic-two","model":"model"})
        )
        .instance_id
    );
    let no_pool = factory
        .create_provider(
            &json!("openai"),
            &json!({"api_key":"synthetic-one","model":"model"}),
            "json/1",
            false,
        )
        .unwrap()
        .0;
    assert_ne!(first.instance_id, no_pool.instance_id);
    assert_ne!(
        first.instance_id,
        create(
            &factory,
            "OPENAI",
            json!({"api_key":"synthetic-one","model":"model"})
        )
        .instance_id
    );
    let list = create(
        &factory,
        "openai",
        json!({"api_key":"synthetic-one","extra":[1,2]}),
    );
    let tuple=factory.pool_get(&json!("openai"),&json!({"api_key":scalar(json!("synthetic-one")),"extra":{"kind":"tuple","items":[scalar(json!(1)),scalar(json!(2))]}}),"python-values/1").unwrap();
    assert!(tuple.1);
    assert_eq!(list.instance_id, tuple.0.instance_id);
    factory.clear().unwrap();
    assert_ne!(
        first.instance_id,
        create(
            &factory,
            "openai",
            json!({"api_key":"synthetic-one","model":"model"})
        )
        .instance_id
    );
    factory.register(&json!("openai"), "custom").unwrap();
    factory.clear().unwrap();
    let custom = create(&factory, "openai", json!({"marker":1}));
    assert_eq!(custom.backend, "custom");
    assert_eq!(custom.constructor_id.as_deref(), Some("custom"));
    assert_eq!(
        create(&factory, "OPENAI", json!({"api_key":"synthetic-one"})).backend,
        "openai"
    );
    factory.unregister(&json!("openai")).unwrap();
    assert_eq!(
        custom.instance_id,
        create(&factory, "openai", json!({"marker":1})).instance_id
    );
    factory.register(&json!("failure"), "failing").unwrap();
    boundary.failing.set(true);
    let calls = boundary.calls.borrow().len();
    for _ in 0..2 {
        assert_eq!(
            factory
                .pool_get(&json!("failure"), &json!({}), "json/1")
                .unwrap_err()
                .python_exception,
            "RuntimeError"
        );
    }
    assert_eq!(boundary.calls.borrow().len(), calls + 2);
    assert!(
        !state
            .borrow()
            .cache
            .iter()
            .any(|entry| entry.instance.provider_name == json!("failure"))
    );
    boundary.failing.set(false);
    factory.register(&json!("unavailable"), "custom").unwrap();
    boundary.available.set(Some(false));
    let unavailable = create(&factory, "unavailable", json!({}));
    assert_eq!(unavailable.backend, "custom");
    assert_eq!(unavailable.available, Some(false));
    assert_eq!(
        factory
            .pool_get(&json!("absent"), &json!({}), "json/1")
            .unwrap_err()
            .python_exception,
        "ValueError"
    );
    assert_eq!(
        factory
            .pool_get(&json!(1), &json!({}), "json/1")
            .unwrap_err()
            .python_exception,
        "AttributeError"
    );

    // Rust equivalents of all three assertions in the untouched original
    // TestSemanticExtractImprovements.test_provider_pool (run separately in Python).
    let one = create(
        &factory,
        "openai",
        json!({"api_key":"test","model_name":"gpt-4"}),
    );
    let two = create(
        &factory,
        "openai",
        json!({"api_key":"test","model_name":"gpt-4"}),
    );
    let three = create(
        &factory,
        "openai",
        json!({"api_key":"test","model_name":"gpt-3.5"}),
    );
    let four = factory
        .create_provider(
            &json!("openai"),
            &json!({"api_key":"test","model_name":"gpt-4"}),
            "json/1",
            false,
        )
        .unwrap()
        .0;
    assert_eq!(one.instance_id, two.instance_id);
    assert_ne!(one.instance_id, three.instance_id);
    assert_ne!(one.instance_id, four.instance_id);
    let encoded = serde_json::to_string(&*state.borrow()).unwrap();
    assert!(!encoded.contains("synthetic-one"));
    assert!(!encoded.contains("synthetic-two"));
}

#[test]
fn registry_order_builtin_selection_and_generated_port_share_one_state() {
    let state = RefCell::new(empty_state());
    let boundary = ConstructionBoundary::default();
    let factory = RustProviderFactory::new(&state, &boundary);
    let command = |operation: &str, name: Value, id: Option<&str>| SemanticProviderFactoryRequest {
        schema: "semantica.provider-factory-request/1".into(),
        operation: operation.into(),
        name,
        constructor_id: id.map(str::to_owned),
        kwargs: json!({}),
        kwargs_profile: "json/1".into(),
        use_pool: true,
    };
    for (name, id) in [("a", "first"), ("b", "second"), ("a", "replace")] {
        factory
            .apply(&command("register", json!(name), Some(id)))
            .unwrap();
    }
    assert_eq!(
        factory
            .apply(&command("list_all", Value::Null, None))
            .unwrap()
            .names,
        vec![json!("a"), json!("b")]
    );
    assert_eq!(
        factory
            .apply(&command("get", json!("a"), None))
            .unwrap()
            .binding
            .unwrap()
            .constructor_id,
        "replace"
    );
    factory
        .apply(&command("unregister", json!("a"), None))
        .unwrap();
    factory.register(&json!("a"), "tail").unwrap();
    assert_eq!(factory.list_all().unwrap(), vec![json!("b"), json!("a")]);
    factory.apply(&command("clear", Value::Null, None)).unwrap();
    assert_eq!(factory.list_all().unwrap(), vec![json!("b"), json!("a")]);
    factory.unregister(&json!("missing")).unwrap();
    assert_eq!(factory.get_binding(&json!("missing")).unwrap(), None);
    for backend in BUILTINS {
        let instance = create(&factory, &backend.to_uppercase(), json!({}));
        assert_eq!(&instance.backend, backend);
        assert_eq!(instance.constructor_id, None);
        assert_eq!(
            instance.default_model.as_deref(),
            builtin_default_model(backend)
        );
        assert_eq!(instance.available, None);
    }
    assert_eq!(boundary.calls.borrow().len(), 8); // actual injected constructor calls
    let hf = create(
        &factory,
        "huggingface_llm",
        json!({"model_name":"original-hf"}),
    );
    assert_eq!(hf.default_model.as_deref(), Some("original-hf"));
    let null_model = create(&factory, "openai", json!({"model":null}));
    assert_eq!(null_model.default_model, None);
    assert_eq!(builtin_default_model("custom"), None);
}

#[test]
fn supplemental_original_pool_repr_and_collisions() {
    let original =
        semantica_contracts::json::protocol_json(SUPPLEMENTAL_ORACLE, 2 * 1024 * 1024).unwrap();
    let state = RefCell::new(empty_state());
    let boundary = ConstructionBoundary::default();
    let factory = RustProviderFactory::new(&state, &boundary);
    factory.register(&json!("oracle"), "injected").unwrap();
    let mut instances = std::collections::BTreeMap::new();
    for case in original["cases"].as_array().unwrap() {
        let profile = case["profile"].as_str().unwrap();
        let id = case["id"].as_str().unwrap();
        let key = configuration_key(&json!("oracle"), &case["kwargs"], profile).unwrap();
        assert_eq!(key, case["key_sha256"].as_str().unwrap(), "{id}");
        let first = factory
            .pool_get(&json!("oracle"), &case["kwargs"], profile)
            .unwrap()
            .0;
        let again = factory
            .pool_get(&json!("oracle"), &case["kwargs"], profile)
            .unwrap();
        assert!(again.1);
        assert_eq!(first.instance_id, again.0.instance_id);
        instances.insert(id, first.instance_id);
    }
    assert_eq!(original["cases"].as_array().unwrap().len(), 17);
    for pair in original["collisions"].as_array().unwrap() {
        assert_eq!(
            instances[pair[0].as_str().unwrap()],
            instances[pair[1].as_str().unwrap()]
        );
    }
    for pair in original["distinct"].as_array().unwrap() {
        assert_ne!(
            instances[pair[0].as_str().unwrap()],
            instances[pair[1].as_str().unwrap()]
        );
    }
}

#[test]
fn borrowed_state_conflicts_fail_without_mutation_or_panic() {
    let state = RefCell::new(empty_state());
    let boundary = ConstructionBoundary::default();
    let factory = RustProviderFactory::new(&state, &boundary);
    let borrowed = state.borrow_mut();
    assert_eq!(
        factory.register(&json!("a"), "ctor").unwrap_err().code,
        "factory_state_borrow_conflict"
    );
    assert_eq!(
        factory.get_binding(&json!("a")).unwrap_err().code,
        "factory_state_borrow_conflict"
    );
    assert_eq!(
        factory.list_all().unwrap_err().code,
        "factory_state_borrow_conflict"
    );
    assert_eq!(
        factory.unregister(&json!("a")).unwrap_err().code,
        "factory_state_borrow_conflict"
    );
    assert_eq!(
        factory.clear().unwrap_err().code,
        "factory_state_borrow_conflict"
    );
    assert_eq!(
        factory
            .pool_get(&json!("openai"), &json!({}), "json/1")
            .unwrap_err()
            .code,
        "factory_state_borrow_conflict"
    );
    assert!(borrowed.bindings.is_empty());
    assert!(borrowed.cache.is_empty());
    assert!(boundary.calls.borrow().is_empty());
    drop(borrowed);
    assert!(
        factory
            .pool_get(&json!("openai"), &json!({}), "json/1")
            .is_ok()
    );
}

#[test]
fn registry_python_numeric_names_and_constructor_rejection() {
    let state = RefCell::new(empty_state());
    let boundary = ConstructionBoundary::default();
    let factory = RustProviderFactory::new(&state, &boundary);
    factory.register(&json!(true), "bool").unwrap();
    factory.register(&json!(1), "int").unwrap();
    assert_eq!(factory.list_all().unwrap(), vec![json!(true)]);
    assert_eq!(
        factory
            .get_binding(&json!(1.0))
            .unwrap()
            .unwrap()
            .constructor_id,
        "int"
    );
    factory
        .register(&json!(1152921504606846976u64), "large-int")
        .unwrap();
    assert_eq!(
        factory
            .get_binding(&json!(1152921504606846976.0))
            .unwrap()
            .unwrap()
            .constructor_id,
        "large-int"
    );
    factory
        .register(&json!(9007199254740993u64), "not-rounded")
        .unwrap();
    assert!(
        factory
            .get_binding(&json!(9007199254740992.0))
            .unwrap()
            .is_none()
    );
    for name in [json!([]), json!({})] {
        assert_eq!(
            factory
                .register(&name, "ctor")
                .unwrap_err()
                .python_exception,
            "TypeError"
        );
    }
    boundary.invalid.set(true);
    assert_eq!(
        factory
            .pool_get(&json!("openai"), &json!({}), "json/1")
            .unwrap_err()
            .code,
        "factory_constructor_identity_invalid"
    );
    assert!(state.borrow().cache.is_empty());
    assert_eq!(state.borrow().next_instance, 1);
    boundary.invalid.set(false);
    assert_eq!(
        create(&factory, "openai", json!({})).instance_id,
        "provider-instance:1"
    );
}

#[test]
fn unknown_python_values_are_explicitly_uncovered_and_json_markers_remain_data() {
    let name = json!("oracle");
    assert_eq!(
        configuration_key(
            &name,
            &json!({"v":{"kind":"set","items":[]}}),
            "python-values/1"
        )
        .unwrap_err()
        .code,
        "factory_python_value_uncovered"
    );
    assert_eq!(
        configuration_key(
            &name,
            &json!({"v":{"kind":"scalar","value":[]}}),
            "python-values/1"
        )
        .unwrap_err()
        .code,
        "factory_typed_value_invalid"
    );
    assert!(configuration_key(&name, &json!({"v":{"kind":"tuple","items":[]}}), "json/1").is_ok());
    assert_ne!(
        configuration_key(&name, &json!({"v":{"kind":"tuple","items":[]}}), "json/1").unwrap(),
        configuration_key(
            &name,
            &json!({"v":{"kind":"tuple","items":[]}}),
            "python-values/1"
        )
        .unwrap()
    );
}

// Exact acceptance-only frozen input/expectation bytes are appended below.
const SUPPLEMENTAL_ORACLE: &str = r###"
{
  "schema": "original-pool-supplemental-expected/1",
  "cases": [
    {
      "id": "empty",
      "profile": "json/1",
      "kwargs": {},
      "repr": "('oracle',)",
      "key_sha256": "65438b86dedb1567feb034edb79fa22e3ba28cd77196a037148610c41554e917"
    },
    {
      "id": "list",
      "profile": "json/1",
      "kwargs": {
        "extra": [
          1,
          2
        ]
      },
      "repr": "('oracle', ('extra', (1, 2)))",
      "key_sha256": "02aaeb521f193b60a58cfdd49aea1ece9f8dd821063dc42a3ae3669c68c6ff0b"
    },
    {
      "id": "tuple",
      "profile": "python-values/1",
      "kwargs": {
        "extra": {
          "kind": "tuple",
          "items": [
            {
              "kind": "scalar",
              "value": 1
            },
            {
              "kind": "scalar",
              "value": 2
            }
          ]
        }
      },
      "repr": "('oracle', ('extra', (1, 2)))",
      "key_sha256": "02aaeb521f193b60a58cfdd49aea1ece9f8dd821063dc42a3ae3669c68c6ff0b"
    },
    {
      "id": "dict_pairs",
      "profile": "json/1",
      "kwargs": {
        "extra": {
          "a": 1
        }
      },
      "repr": "('oracle', ('extra', (('a', 1),)))",
      "key_sha256": "bc1f4a9a570520b56b4611a4d6e5abbff3f9e04fecfa7f9a235de5937c781a03"
    },
    {
      "id": "tuple_pairs",
      "profile": "python-values/1",
      "kwargs": {
        "extra": {
          "kind": "tuple",
          "items": [
            {
              "kind": "tuple",
              "items": [
                {
                  "kind": "scalar",
                  "value": "a"
                },
                {
                  "kind": "scalar",
                  "value": 1
                }
              ]
            }
          ]
        }
      },
      "repr": "('oracle', ('extra', (('a', 1),)))",
      "key_sha256": "bc1f4a9a570520b56b4611a4d6e5abbff3f9e04fecfa7f9a235de5937c781a03"
    },
    {
      "id": "nested_sorted",
      "profile": "json/1",
      "kwargs": {
        "nested": {
          "b": [
            1,
            2
          ],
          "a": null
        }
      },
      "repr": "('oracle', ('nested', (('a', None), ('b', (1, 2)))))",
      "key_sha256": "dd03602fa4fc104ccdb2ce5448f06f1390869847312da0dfae65a4894d11546b"
    },
    {
      "id": "tuple_dict_raw_ba",
      "profile": "python-values/1",
      "kwargs": {
        "extra": {
          "kind": "tuple",
          "items": [
            {
              "kind": "dict",
              "items": {
                "b": {
                  "kind": "scalar",
                  "value": 2
                },
                "a": {
                  "kind": "scalar",
                  "value": 1
                }
              }
            }
          ]
        }
      },
      "repr": "('oracle', ('extra', ({'b': 2, 'a': 1},)))",
      "key_sha256": "c719a23368af9ad75124e827cf38aeb23515f4dae0e579c9cf35f78df9ea2e6c"
    },
    {
      "id": "tuple_dict_raw_ab",
      "profile": "python-values/1",
      "kwargs": {
        "extra": {
          "kind": "tuple",
          "items": [
            {
              "kind": "dict",
              "items": {
                "a": {
                  "kind": "scalar",
                  "value": 1
                },
                "b": {
                  "kind": "scalar",
                  "value": 2
                }
              }
            }
          ]
        }
      },
      "repr": "('oracle', ('extra', ({'a': 1, 'b': 2},)))",
      "key_sha256": "202961b7366fbb0f8fb63cd3c1674969c466b1a9608f8d035e225a950ff90d8f"
    },
    {
      "id": "tuple_list_raw",
      "profile": "python-values/1",
      "kwargs": {
        "extra": {
          "kind": "tuple",
          "items": [
            {
              "kind": "list",
              "items": [
                {
                  "kind": "scalar",
                  "value": 1
                },
                {
                  "kind": "scalar",
                  "value": 2
                }
              ]
            }
          ]
        }
      },
      "repr": "('oracle', ('extra', ([1, 2],)))",
      "key_sha256": "0a2060aa9731248316c6c4ad1b10b6f26b988b15e32179c0c6d659d0ecd24bc9"
    },
    {
      "id": "list_list_normalized",
      "profile": "json/1",
      "kwargs": {
        "extra": [
          [
            1,
            2
          ]
        ]
      },
      "repr": "('oracle', ('extra', ((1, 2),)))",
      "key_sha256": "11307a2bc2d0e2914e580bb97eb1633d67956d3175097308c61a4a78b981d827"
    },
    {
      "id": "bool_and_int",
      "profile": "json/1",
      "kwargs": {
        "a": true,
        "b": 1,
        "c": false,
        "d": null
      },
      "repr": "('oracle', ('a', True), ('b', 1), ('c', False), ('d', None))",
      "key_sha256": "1813a4f70178ff3f8ebdaf34da59aee926eec486a3555cf204e135531c6905a2"
    },
    {
      "id": "finite_float",
      "profile": "json/1",
      "kwargs": {
        "a": -0.0,
        "b": 1.0,
        "c": 1e-07,
        "d": 1e+16
      },
      "repr": "('oracle', ('a', -0.0), ('b', 1.0), ('c', 1e-07), ('d', 1e+16))",
      "key_sha256": "e32c40f74a718b11ad36a2178a9535d16dc409dd2fd638a70bff73f8fa4fe5fc"
    },
    {
      "id": "apostrophe",
      "profile": "json/1",
      "kwargs": {
        "value": "'"
      },
      "repr": "('oracle', ('value', \"'\"))",
      "key_sha256": "e3b999cb7a7510342043f0f38ac6f6057b51e8ef669f057e79da013091294d63"
    },
    {
      "id": "both_quotes",
      "profile": "json/1",
      "kwargs": {
        "value": "'\""
      },
      "repr": "('oracle', ('value', '\\'\"'))",
      "key_sha256": "1f9219b27f89963e164cf5b4b1dde09c5d77f4e39edb6097a6d02b43e0d6bd88"
    },
    {
      "id": "unicode_printable",
      "profile": "json/1",
      "kwargs": {
        "value": "é中文😀é"
      },
      "repr": "('oracle', ('value', 'é中文😀é'))",
      "key_sha256": "a6912fc4f71fa31a13b9c45e808e19bf51c353d207847b8c7dd7446181929136"
    },
    {
      "id": "unicode_nonprintable",
      "profile": "json/1",
      "kwargs": {
        "value": "\u0000\b\t\n\r\u001f\u007f\u0085 ​

😀"
      },
      "repr": "('oracle', ('value', '\\x00\\x08\\t\\n\\r\\x1f\\x7f\\x85\\xa0\\u200b\\u2028\\u2029\\ue000😀'))",
      "key_sha256": "19bfa7d0c62cc00193cb12b8fdd1e9db4f71be7e6d35cc753dd6aa5c5481c67a"
    },
    {
      "id": "unicode_15_boundary",
      "profile": "json/1",
      "kwargs": {
        "value": "🫨🫩􏿿"
      },
      "repr": "('oracle', ('value', '🫨\\U0001fae9\\U0010ffff'))",
      "key_sha256": "b39d3e5bec9a5628cb8c7157b5859f4ad8906fbf4e215f63cc4442cf62e88513"
    }
  ],
  "collisions": [
    [
      "list",
      "tuple"
    ],
    [
      "dict_pairs",
      "tuple_pairs"
    ]
  ],
  "distinct": [
    [
      "tuple_dict_raw_ba",
      "tuple_dict_raw_ab"
    ],
    [
      "tuple_list_raw",
      "list_list_normalized"
    ]
  ],
  "generated_before_execution": true,
  "expectation_source": "literal Python repr expectations; no Rust implementation consulted"
}
"###;
