use anyhow::{Context, Result};
use semantica_runtime::{
    book_runner::{self, BookRunnerState},
    books::{Catalog, SourceProvider},
    dataset::DatasetSession,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

struct Source(BTreeMap<String, Vec<u8>>);
impl SourceProvider for Source {
    fn read(&self, id: &str) -> Result<Vec<u8>> {
        self.0.get(id).cloned().context("test source missing")
    }
}
fn fixture(vol2: bool, operation: &str) -> Source {
    let mut files=BTreeMap::from([
        ("p/positive.ttl".into(),b"@prefix x:<urn:x:>. x:a a x:Type; x:name \"ready\" .".to_vec()),
        ("p/negative.ttl".into(),b"@prefix x:<urn:x:>. x:a a x:Type .".to_vec()),
        ("p/query.rq".into(),b"PREFIX x:<urn:x:> SELECT ?value WHERE { ?s x:name ?value }".to_vec()),
        ("p/shapes.ttl".into(),b"@prefix x:<urn:x:>. @prefix sh:<http://www.w3.org/ns/shacl#>. x:Shape a sh:NodeShape; sh:targetClass x:Type; sh:property [sh:path x:name; sh:minCount 1; sh:message \"required field\"] .".to_vec()),
        ("p/facts.txt".into(),b"# original text boundary\nA(a)\n".to_vec()),
        ("p/rules.txt".into(),b"IF A(?x) THEN B(?x)\nIF B(?x) THEN C(?x)\n".to_vec()),
        ("p/contract.json".into(),b"{\"status\":\"partial\"}".to_vec()),
    ]);
    let scenario = if operation == "forward_chain" {
        json!({"id":"s","inputs":["facts","rules"],"execution":{"operation":"forward_chain"},"oracle":{"status":"ready","expected_conclusions":["B(a)","C(a)"]}})
    } else if operation == "validate" {
        json!({"id":"s","inputs":["positive","negative"],"execution":{"operation":"validate","shape_asset_id":"shapes"},"oracle":{"status":"ready","positive":{"conforms":true},"single_fault_negative":{"conforms":false,"violation_count":1,"message_contains":"required"}}})
    } else {
        json!({"id":"s","inputs":["positive","negative"],"execution":{"operation":"select_exact_multiset","query_asset_id":"query"},"oracle":{"status":"ready","positive":{"row_count":1,"bindings":{"value":["ready"]}},"single_fault_negative":{"row_count":0}}})
    };
    files.insert(
        "p/scenarios.json".into(),
        serde_json::to_vec(
            &json!({"runner_contract":book_runner::RUNNER_CONTRACT,"scenarios":[scenario]}),
        )
        .unwrap(),
    );
    files.insert("p/cqs.json".into(),serde_json::to_vec(&if vol2{json!({"runner_contract":{"requested_api":"SemanticPackageRunner.run_scenario","fail_closed":true},"competency_questions":[{"id":"cq","scenario_id":"s","execution":{"steps":[{"operation":"load_package","package_id":"p","version":"1"},{"operation":"load_asset","asset_id":"positive"},{"operation":"select","asset_id":"query"}]},"oracles":{"cq_exact":{"kind":"binding_multiset","variables":["value"],"rows":[{"value":{"value":"ready","term_type":"literal","datatype":null,"language":null}}]},"positive_path":{"dataset_asset":"positive","shapes_asset":"shapes","conforms":true},"single_fault_negative":{"dataset_asset":"negative","shapes_asset":"shapes","conforms":false,"fault_count":1,"required_message_contains":"required"}}}]})}else{json!({"competency_questions":[{"id":"cq","scenario_ids":["s"]}]})}).unwrap());
    let assets:Vec<_>=[("positive","data","positive.ttl","turtle"),("negative","data","negative.ttl","turtle"),("query","sparql","query.rq","sparql"),("shapes","shapes","shapes.ttl","turtle"),("facts","facts","facts.txt","semantica-fact"),("rules","rules","rules.txt","semantica-rule"),("contract","chapter_contract","contract.json","json"),("scenarios","scenario_registry","scenarios.json","json"),("cqs","cq_registry","cqs.json","json")].into_iter().map(|(id,role,path,format)|json!({"asset_id":id,"role":role,"path":path,"format":format,"load_into_dataset":false,"sha256":semantica_runtime::sha256(&files[&format!("p/{path}")])})).collect();
    let mut manifest = json!({"schema_version":"1.0","package_id":"p","version":"1","namespace":"urn:x:","release_status":"partial","assets":assets});
    if vol2 {
        manifest["scenarios"] = json!([{"scenario_id":"s","cq_registry_asset":"cqs"}]);
    } else {
        manifest["execution"] =
            json!({"scenario_registry_asset_id":"scenarios","cq_registry_asset_id":"cqs"});
    }
    files.insert(
        "p/manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    files.insert("registry.json".into(),serde_json::to_vec(&json!({"schema_version":"1.0","policy":"policy.json","packages":[{"volume":"v","chapter":"c","package_id":"p","version":"1","manifest":"p/manifest.json"}],"domain_packages":[]})).unwrap());
    files.insert("policy.json".into(), b"{}".to_vec());
    Source(files)
}
fn identity() -> Value {
    json!({"runtime_version":"observed-rust","runtime_commit":"semantica.rust-source/observed","runtime_artifact_sha256":"a".repeat(64),"created_at":"2000-01-01T00:00:00Z","compilation_origin":"unknown","identity_authority":"not_connected"})
}

#[test]
fn original_known_preconditions_preserve_contract_checks() {
    let frozen: Value = serde_json::from_str(FROZEN_PRECONDITIONS).unwrap();
    for case in frozen["cases"].as_array().unwrap() {
        let mut source = fixture(false, "select");
        source.0.insert(
            "p/scenarios.json".into(),
            serde_json::to_vec(&json!({"runner_contract":book_runner::RUNNER_CONTRACT,"scenarios":[case["scenario"]]})).unwrap(),
        );
        let mut manifest: Value = serde_json::from_slice(&source.0["p/manifest.json"]).unwrap();
        for asset in manifest["assets"].as_array_mut().unwrap() {
            if asset["asset_id"] == "scenarios" {
                asset["sha256"] = json!(semantica_runtime::sha256(&source.0["p/scenarios.json"]));
            }
        }
        source.0.insert(
            "p/manifest.json".into(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let catalog = Catalog::read(&source, "registry.json").unwrap();
        let run = book_runner::run(
            &mut BookRunnerState::default(),
            &catalog,
            &source,
            &mut DatasetSession::new(),
            "p",
            case["scenario_id"].as_str(),
            &identity(),
        )
        .unwrap();
        assert_eq!(run["status"], case["expected_status"]);
        assert_eq!(
            run["oracle_checks"],
            json!([case["expected_check"]]),
            "original {}",
            case["scenario_id"]
        );
        assert_eq!(
            run["cq_report"]["payload"]["reason"],
            case["expected_contract_reason"]
        );
        assert_eq!(run["operations"], json!([]));
    }
}

#[test]
fn retained_receipt_detects_caller_dataset_mutation() {
    let source = fixture(false, "select");
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = DatasetSession::new();
    let mut state = BookRunnerState::default();
    let run = book_runner::run(
        &mut state,
        &catalog,
        &source,
        &mut session,
        "p",
        None,
        &identity(),
    )
    .unwrap();
    assert_eq!(run["status"], "passed");
    let original_receipt = semantica_contracts::json::canonical_json(&run["receipt"]).unwrap();
    session
        .load(
            b"<urn:added> <urn:predicate> <urn:value> .",
            oxigraph::io::RdfFormat::NTriples,
            None,
            None,
        )
        .unwrap();
    let verdict = book_runner::verify(&state, &mut session, &run, &identity()).unwrap();
    let checks = verdict["checks"].as_array().unwrap();
    assert_eq!(
        checks
            .iter()
            .find(|v| v["check_id"] == "dataset.integrity")
            .unwrap()["passed"],
        false
    );
    assert_eq!(
        checks
            .iter()
            .find(|v| v["check_id"] == "receipt.integrity")
            .unwrap()["passed"],
        true
    );
    assert_eq!(
        original_receipt,
        semantica_contracts::json::canonical_json(&run["receipt"]).unwrap()
    );
}

#[test]
fn original_chapter_pure_comparison_frozen_oracles() {
    let fixture: Value = serde_json::from_str(FROZEN_COMPARISONS).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let declaration = &case["declaration"];
        let result = &case["result"];
        let result_option = if result.is_null() { None } else { Some(result) };
        let actual = match case["function"].as_str().unwrap() {
            "cq" => book_runner::compare_binding_multiset(declaration, result),
            "ask" => book_runner::compare_ask_oracle("oracle.ask", declaration, result),
            "select" => {
                book_runner::compare_vol1_select_oracle("select", declaration, result_option)
            }
            "validation" => {
                let mut result = result.clone();
                if result.is_object() {
                    result["violation_count"] = json!(result["messages"].as_array().unwrap().len());
                }
                book_runner::compare_validation_oracle(
                    "validation",
                    declaration,
                    if result.is_null() {
                        None
                    } else {
                        Some(&result)
                    },
                )
            }
            "reason" => {
                book_runner::compare_reason_oracle("oracle.reason", declaration, result_option)
            }
            "boolean" => book_runner::compare_boolean("oracle.boolean", declaration, result_option),
            _ => panic!("unknown frozen operation"),
        };
        if case["expected"]["error_class"] == "TypeError" {
            assert!(
                actual
                    .unwrap_err()
                    .to_string()
                    .contains("book_oracle_noniterable_bindings")
            );
        } else {
            let actual = actual.unwrap();
            assert_eq!(
                semantica_contracts::json::canonical_json(&actual).unwrap(),
                semantica_contracts::json::canonical_json(&case["expected"]).unwrap(),
                "original case {}",
                case["id"]
            );
        }
    }
}
#[test]
fn vol1_real_select_uses_isolated_negative_branch_and_same_instance_receipt() {
    let source = fixture(false, "select");
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = DatasetSession::new();
    let mut state = BookRunnerState::default();
    let run = book_runner::run(
        &mut state,
        &catalog,
        &source,
        &mut session,
        "p",
        None,
        &identity(),
    )
    .unwrap();
    assert_eq!(run["status"], "passed");
    assert_eq!(run["operations"].as_array().unwrap().len(), 4);
    assert_eq!(run["operations"][1]["payload"]["row_count"], 1);
    assert_eq!(run["operations"][3]["payload"]["row_count"], 0);
    assert_eq!(session.quad_count(), 2);
    assert_eq!(run["release_verdict"]["status"], "blocked");
    let verdict = book_runner::verify(&state, &mut session, &run, &identity()).unwrap();
    assert_eq!(verdict, run["release_verdict"]);
    let foreign =
        book_runner::verify(&BookRunnerState::default(), &mut session, &run, &identity()).unwrap();
    assert_eq!(foreign["reasons"], json!(["runner.execution_context"]));
    let mut forged = run.clone();
    forged["receipt"]["cq_report"]["status"] = json!("failed");
    assert!(
        book_runner::verify(&state, &mut session, &forged, &identity()).unwrap()["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "receipt.integrity")
    );
    assert!(
        book_runner::run(
            &mut state,
            &catalog,
            &source,
            &mut session,
            "p",
            None,
            &identity()
        )
        .unwrap_err()
        .to_string()
        .contains("empty_staging")
    );
}
#[test]
fn vol1_real_shacl_checks_positive_and_single_fault_negative() {
    let source = fixture(false, "validate");
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let run = book_runner::run(
        &mut BookRunnerState::default(),
        &catalog,
        &source,
        &mut DatasetSession::new(),
        "p",
        None,
        &identity(),
    )
    .unwrap();
    assert_eq!(run["status"], "passed");
    assert_eq!(run["operations"][1]["payload"]["violation_count"], 0);
    assert_eq!(run["operations"][3]["payload"]["violation_count"], 1);
    assert_eq!(run["oracle_checks"][1]["status"], "passed");
}
#[test]
fn vol2_real_steps_exact_multiset_and_compact_shacl_branches() {
    let source = fixture(true, "select");
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let run = book_runner::run(
        &mut BookRunnerState::default(),
        &catalog,
        &source,
        &mut DatasetSession::new(),
        "p",
        Some("s"),
        &identity(),
    )
    .unwrap();
    assert_eq!(run["status"], "passed");
    assert_eq!(run["oracle_checks"].as_array().unwrap().len(), 3);
    assert!(
        run["oracle_checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["status"] == "passed")
    );
    assert_eq!(run["operations"].as_array().unwrap().len(), 7);
}
#[test]
fn actual_textual_forward_chain_has_same_pass_details_and_convergence() {
    let source = fixture(false, "forward_chain");
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = DatasetSession::new();
    let run = book_runner::run(
        &mut BookRunnerState::default(),
        &catalog,
        &source,
        &mut session,
        "p",
        None,
        &identity(),
    )
    .unwrap();
    assert_eq!(run["status"], "passed");
    assert_eq!(
        run["operations"][0]["payload"]["conclusions"],
        json!(["B(a)", "C(a)"])
    );
    assert_eq!(
        run["operations"][0]["payload"]["inferences"][1]["premises"],
        json!(["B(a)"])
    );
    assert_eq!(
        run["operations"][0]["payload"]["inferences"][0]["explanation"],
        "Given the premises: A(a), we conclude: B(a) using rule 'rule_1'."
    );
    assert_eq!(session.quad_count(), 0);
}
#[test]
fn malformed_oracle_and_missing_execution_remain_blocked() {
    let mut source = fixture(false, "select");
    let mut registry: Value = serde_json::from_slice(&source.0["p/scenarios.json"]).unwrap();
    registry["scenarios"][0]["execution"]["operation"] = json!("SWRL");
    let bytes = serde_json::to_vec(&registry).unwrap();
    let mut manifest: Value = serde_json::from_slice(&source.0["p/manifest.json"]).unwrap();
    let a = manifest["assets"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["asset_id"] == "scenarios")
        .unwrap();
    a["sha256"] = json!(semantica_runtime::sha256(&bytes));
    source.0.insert("p/scenarios.json".into(), bytes);
    source.0.insert(
        "p/manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let run = book_runner::run(
        &mut BookRunnerState::default(),
        &catalog,
        &source,
        &mut DatasetSession::new(),
        "p",
        None,
        &identity(),
    )
    .unwrap();
    assert_eq!(run["status"], "blocked");
    assert_eq!(run["release_verdict"]["status"], "blocked");
    assert_ne!(run["oracle_report"]["status"], "passed");
}

#[test]
fn actual_ask_uses_the_shared_graph_result_encoding() {
    let mut source = fixture(true, "select");
    let query = b"PREFIX x:<urn:x:> ASK { ?s x:name ?value }".to_vec();
    source.0.insert("p/query.rq".into(), query.clone());
    let mut cqs: Value = serde_json::from_slice(&source.0["p/cqs.json"]).unwrap();
    cqs["competency_questions"][0]["execution"]["steps"][2]["operation"] = json!("ask");
    cqs["competency_questions"][0]["oracles"]["cq_exact"] = json!({"boolean":true});
    let cqs = serde_json::to_vec(&cqs).unwrap();
    source.0.insert("p/cqs.json".into(), cqs.clone());
    let mut manifest: Value = serde_json::from_slice(&source.0["p/manifest.json"]).unwrap();
    for asset in manifest["assets"].as_array_mut().unwrap() {
        if asset["asset_id"] == "query" {
            asset["sha256"] = json!(semantica_runtime::sha256(&query));
        }
        if asset["asset_id"] == "cqs" {
            asset["sha256"] = json!(semantica_runtime::sha256(&cqs));
        }
    }
    source.0.insert(
        "p/manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let run = book_runner::run(
        &mut BookRunnerState::default(),
        &catalog,
        &source,
        &mut DatasetSession::new(),
        "p",
        None,
        &identity(),
    )
    .unwrap();
    assert_eq!(run["operations"][2]["status"], "passed");
    assert_eq!(run["operations"][2]["payload"]["boolean"], true);
    assert_eq!(run["status"], "passed");
}

// The original v2 expectation bytes are appended unchanged by the acceptance
// fixture preparation script; these are independent original Python AST data.
const FROZEN_COMPARISONS: &str = r#####"{
  "profile": "original.chapter-pure-comparison-finite-json/1",
  "cases": [
    {
      "id": "cq-missing",
      "function": "cq",
      "declaration": null,
      "result": {
        "kind": "select",
        "variables": [],
        "rows": []
      },
      "expected": [
        {
          "check_id": "oracle.cq_exact",
          "status": "blocked",
          "expected": {
            "oracle": "binding_multiset required"
          },
          "actual": {
            "oracle": "missing"
          },
          "message": "CQ binding multiset oracle is missing"
        }
      ]
    },
    {
      "id": "cq-kind",
      "function": "cq",
      "declaration": {
        "kind": "unsupported"
      },
      "result": {
        "kind": "select",
        "variables": [],
        "rows": []
      },
      "expected": [
        {
          "check_id": "oracle.cq_exact",
          "status": "blocked",
          "expected": {
            "kind": "binding_multiset"
          },
          "actual": {
            "kind": "unsupported"
          },
          "message": "CQ oracle kind is unsupported"
        }
      ]
    },
    {
      "id": "cq-malformed",
      "function": "cq",
      "declaration": {
        "kind": "binding_multiset",
        "variables": "x",
        "rows": null
      },
      "result": {
        "kind": "select",
        "variables": [],
        "rows": []
      },
      "expected": [
        {
          "check_id": "oracle.cq_exact",
          "status": "blocked",
          "expected": {
            "variables": "list",
            "rows": "list"
          },
          "actual": {
            "variables": "str",
            "rows": "NoneType"
          },
          "message": "CQ oracle variables/rows are malformed"
        }
      ]
    },
    {
      "id": "cq-row-order-not-significant-duplicates-counted",
      "function": "cq",
      "declaration": {
        "kind": "binding_multiset",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {
            "x": {
              "value": "urn:y",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "result": {
        "kind": "select",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {
            "x": {
              "value": "urn:y",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "expected": [
        {
          "check_id": "oracle.cq_exact",
          "status": "passed",
          "expected": {
            "variables": [
              "x"
            ],
            "rows": [
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              },
              {
                "x": {
                  "value": "urn:y",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              },
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              }
            ]
          },
          "actual": {
            "variables": [
              "x"
            ],
            "rows": [
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              },
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              },
              {
                "x": {
                  "value": "urn:y",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              }
            ]
          },
          "message": "exact SELECT variables and binding multiset match"
        }
      ]
    },
    {
      "id": "cq-duplicate-count-differs",
      "function": "cq",
      "declaration": {
        "kind": "binding_multiset",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "result": {
        "kind": "select",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "expected": [
        {
          "check_id": "oracle.cq_exact",
          "status": "failed",
          "expected": {
            "variables": [
              "x"
            ],
            "rows": [
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              }
            ]
          },
          "actual": {
            "variables": [
              "x"
            ],
            "rows": [
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              },
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              }
            ]
          },
          "message": "SELECT variables or binding multiset differs from the exact oracle"
        }
      ]
    },
    {
      "id": "cq-variable-order-differs",
      "function": "cq",
      "declaration": {
        "kind": "binding_multiset",
        "variables": [
          "x",
          "y"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            },
            "y": {
              "value": "urn:y",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "result": {
        "kind": "select",
        "variables": [
          "y",
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            },
            "y": {
              "value": "urn:y",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "expected": [
        {
          "check_id": "oracle.cq_exact",
          "status": "failed",
          "expected": {
            "variables": [
              "x",
              "y"
            ],
            "rows": [
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                },
                "y": {
                  "value": "urn:y",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              }
            ]
          },
          "actual": {
            "variables": [
              "y",
              "x"
            ],
            "rows": [
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                },
                "y": {
                  "value": "urn:y",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              }
            ]
          },
          "message": "SELECT variables or binding multiset differs from the exact oracle"
        }
      ]
    },
    {
      "id": "cq-unbound-and-extra-row-fields",
      "function": "cq",
      "declaration": {
        "kind": "binding_multiset",
        "variables": [
          "x",
          "y"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {}
        ]
      },
      "result": {
        "kind": "select",
        "variables": [
          "x",
          "y"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            },
            "ignored": {
              "value": "urn:y",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {
            "ignored": {
              "value": "urn:y",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "expected": [
        {
          "check_id": "oracle.cq_exact",
          "status": "passed",
          "expected": {
            "variables": [
              "x",
              "y"
            ],
            "rows": [
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              },
              {}
            ]
          },
          "actual": {
            "variables": [
              "x",
              "y"
            ],
            "rows": [
              {
                "x": {
                  "value": "urn:x",
                  "term_type": "iri",
                  "datatype": null,
                  "language": null
                }
              },
              {}
            ]
          },
          "message": "exact SELECT variables and binding multiset match"
        }
      ]
    },
    {
      "id": "cq-canonical-integer-float-lexical-distinction",
      "function": "cq",
      "declaration": {
        "kind": "binding_multiset",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": 1.0,
              "term_type": "literal",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "result": {
        "kind": "select",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": 1,
              "term_type": "literal",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "expected": [
        {
          "check_id": "oracle.cq_exact",
          "status": "failed",
          "expected": {
            "variables": [
              "x"
            ],
            "rows": [
              {
                "x": {
                  "value": 1.0,
                  "term_type": "literal",
                  "datatype": null,
                  "language": null
                }
              }
            ]
          },
          "actual": {
            "variables": [
              "x"
            ],
            "rows": [
              {
                "x": {
                  "value": 1,
                  "term_type": "literal",
                  "datatype": null,
                  "language": null
                }
              }
            ]
          },
          "message": "SELECT variables or binding multiset differs from the exact oracle"
        }
      ]
    },
    {
      "id": "ask-integer-not-boolean",
      "function": "ask",
      "declaration": {
        "boolean": 1
      },
      "result": {
        "kind": "ask",
        "boolean": true
      },
      "expected": {
        "check_id": "oracle.ask",
        "status": "blocked",
        "expected": {
          "boolean": "required"
        },
        "actual": {
          "boolean": true
        },
        "message": "ASK oracle boolean is missing"
      }
    },
    {
      "id": "ask-boolean-passed",
      "function": "ask",
      "declaration": {
        "boolean": false
      },
      "result": {
        "kind": "ask",
        "boolean": false
      },
      "expected": {
        "check_id": "oracle.ask",
        "status": "passed",
        "expected": {
          "boolean": false
        },
        "actual": {
          "boolean": false
        },
        "message": "ASK boolean matches"
      }
    },
    {
      "id": "ask-identity-integer-result-differs",
      "function": "ask",
      "declaration": {
        "boolean": true
      },
      "result": {
        "kind": "ask",
        "boolean": 1
      },
      "expected": {
        "check_id": "oracle.ask",
        "status": "failed",
        "expected": {
          "boolean": true
        },
        "actual": {
          "boolean": 1
        },
        "message": "ASK boolean differs"
      }
    },
    {
      "id": "select-missing",
      "function": "select",
      "declaration": null,
      "result": null,
      "expected": [
        {
          "check_id": "oracle.select",
          "status": "blocked",
          "expected": {
            "oracle": "required"
          },
          "actual": {
            "oracle": "missing"
          },
          "message": "SELECT oracle is missing"
        }
      ]
    },
    {
      "id": "select-unavailable",
      "function": "select",
      "declaration": {
        "row_count": 0
      },
      "result": null,
      "expected": [
        {
          "check_id": "oracle.select",
          "status": "blocked",
          "expected": {
            "row_count": 0
          },
          "actual": {
            "result": "unavailable"
          },
          "message": "SELECT result is unavailable"
        }
      ]
    },
    {
      "id": "select-wrong-result-kind",
      "function": "select",
      "declaration": {
        "row_count": 0
      },
      "result": {
        "kind": "ask",
        "boolean": false
      },
      "expected": [
        {
          "check_id": "oracle.select",
          "status": "blocked",
          "expected": {
            "row_count": 0
          },
          "actual": {
            "result": "unavailable"
          },
          "message": "SELECT result is unavailable"
        }
      ]
    },
    {
      "id": "select-only-declared-lexical-bindings",
      "function": "select",
      "declaration": {
        "row_count": 2,
        "bindings": {
          "x": [
            "a",
            "z"
          ]
        }
      },
      "result": {
        "kind": "select",
        "variables": [
          "ignored",
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "z",
              "term_type": "literal",
              "datatype": "urn:int",
              "language": null
            }
          },
          {
            "x": {
              "value": "a",
              "term_type": "iri",
              "datatype": null,
              "language": null
            },
            "ignored": {
              "value": "urn:y",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "expected": [
        {
          "check_id": "oracle.select",
          "status": "passed",
          "expected": {
            "row_count": 2,
            "bindings": {
              "x": [
                "a",
                "z"
              ]
            }
          },
          "actual": {
            "row_count": 2,
            "bindings": {
              "x": [
                "a",
                "z"
              ]
            },
            "variables": [
              "ignored",
              "x"
            ]
          },
          "message": "exact SELECT row count and bindings match"
        }
      ]
    },
    {
      "id": "select-bool-row-count-original-is-int",
      "function": "select",
      "declaration": {
        "row_count": true
      },
      "result": {
        "kind": "select",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "urn:x",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "expected": [
        {
          "check_id": "oracle.select",
          "status": "passed",
          "expected": {
            "row_count": true,
            "bindings": {}
          },
          "actual": {
            "row_count": 1,
            "bindings": {},
            "variables": [
              "x"
            ]
          },
          "message": "exact SELECT row count and bindings match"
        }
      ]
    },
    {
      "id": "select-string-binding-values-iterate-characters",
      "function": "select",
      "declaration": {
        "row_count": 2,
        "bindings": {
          "x": "ba"
        }
      },
      "result": {
        "kind": "select",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "b",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {
            "x": {
              "value": "a",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          }
        ]
      },
      "expected": [
        {
          "check_id": "oracle.select",
          "status": "passed",
          "expected": {
            "row_count": 2,
            "bindings": {
              "x": [
                "a",
                "b"
              ]
            }
          },
          "actual": {
            "row_count": 2,
            "bindings": {
              "x": [
                "a",
                "b"
              ]
            },
            "variables": [
              "x"
            ]
          },
          "message": "exact SELECT row count and bindings match"
        }
      ]
    },
    {
      "id": "select-missing-unbound-values",
      "function": "select",
      "declaration": {
        "row_count": 2,
        "bindings": {
          "x": [
            "a",
            "a"
          ]
        }
      },
      "result": {
        "kind": "select",
        "variables": [
          "x"
        ],
        "rows": [
          {
            "x": {
              "value": "a",
              "term_type": "iri",
              "datatype": null,
              "language": null
            }
          },
          {}
        ]
      },
      "expected": [
        {
          "check_id": "oracle.select",
          "status": "failed",
          "expected": {
            "row_count": 2,
            "bindings": {
              "x": [
                "a",
                "a"
              ]
            }
          },
          "actual": {
            "row_count": 2,
            "bindings": {
              "x": [
                "a"
              ]
            },
            "variables": [
              "x"
            ]
          },
          "message": "SELECT row count or bindings differ from the oracle"
        }
      ]
    },
    {
      "id": "select-malformed-count",
      "function": "select",
      "declaration": {
        "row_count": "1",
        "bindings": {}
      },
      "result": {
        "kind": "select",
        "variables": [],
        "rows": []
      },
      "expected": [
        {
          "check_id": "oracle.select",
          "status": "blocked",
          "expected": {
            "row_count": "1",
            "bindings": {}
          },
          "actual": {
            "row_count": 0,
            "bindings": {},
            "variables": []
          },
          "message": "SELECT oracle is malformed"
        }
      ]
    },
    {
      "id": "select-noniterable-bindings-value-raises",
      "function": "select",
      "declaration": {
        "row_count": 0,
        "bindings": {
          "x": 1
        }
      },
      "result": {
        "kind": "select",
        "variables": [
          "x"
        ],
        "rows": []
      },
      "expected": {
        "error_class": "TypeError",
        "error_message": "'int' object is not iterable"
      }
    },
    {
      "id": "validation-missing",
      "function": "validation",
      "declaration": null,
      "result": null,
      "expected": [
        {
          "check_id": "oracle.validation",
          "status": "blocked",
          "expected": {
            "oracle": "required"
          },
          "actual": {
            "oracle": "missing"
          },
          "message": "validation oracle is missing"
        }
      ]
    },
    {
      "id": "validation-unavailable",
      "function": "validation",
      "declaration": {
        "conforms": true
      },
      "result": null,
      "expected": [
        {
          "check_id": "oracle.validation",
          "status": "blocked",
          "expected": {
            "conforms": true
          },
          "actual": {
            "result": "unavailable"
          },
          "message": "SHACL validation result is unavailable"
        }
      ]
    },
    {
      "id": "validation-alias-count-and-message",
      "function": "validation",
      "declaration": {
        "conforms": false,
        "fault_count": 1,
        "required_message_contains": "required"
      },
      "result": {
        "kind": "validation",
        "conforms": false,
        "messages": [
          "name is required"
        ]
      },
      "expected": [
        {
          "check_id": "oracle.validation",
          "status": "passed",
          "expected": {
            "conforms": false,
            "violation_count": 1,
            "message_contains": "required"
          },
          "actual": {
            "conforms": false,
            "violation_count": 1,
            "messages": [
              "name is required"
            ]
          },
          "message": "SHACL result matches the declared oracle"
        }
      ]
    },
    {
      "id": "validation-explicit-null-disables-alias",
      "function": "validation",
      "declaration": {
        "conforms": true,
        "violation_count": null,
        "fault_count": 99,
        "message_contains": null,
        "required_message_contains": "absent"
      },
      "result": {
        "kind": "validation",
        "conforms": true,
        "messages": []
      },
      "expected": [
        {
          "check_id": "oracle.validation",
          "status": "passed",
          "expected": {
            "conforms": true
          },
          "actual": {
            "conforms": true,
            "violation_count": 0,
            "messages": []
          },
          "message": "SHACL result matches the declared oracle"
        }
      ]
    },
    {
      "id": "validation-bool-count-original-is-int",
      "function": "validation",
      "declaration": {
        "conforms": false,
        "violation_count": true
      },
      "result": {
        "kind": "validation",
        "conforms": false,
        "messages": [
          "fault"
        ]
      },
      "expected": [
        {
          "check_id": "oracle.validation",
          "status": "passed",
          "expected": {
            "conforms": false,
            "violation_count": true
          },
          "actual": {
            "conforms": false,
            "violation_count": 1,
            "messages": [
              "fault"
            ]
          },
          "message": "SHACL result matches the declared oracle"
        }
      ]
    },
    {
      "id": "validation-empty-message-requires-a-violation",
      "function": "validation",
      "declaration": {
        "conforms": true,
        "message_contains": ""
      },
      "result": {
        "kind": "validation",
        "conforms": true,
        "messages": []
      },
      "expected": [
        {
          "check_id": "oracle.validation",
          "status": "failed",
          "expected": {
            "conforms": true,
            "message_contains": ""
          },
          "actual": {
            "conforms": true,
            "violation_count": 0,
            "messages": []
          },
          "message": "SHACL result differs from the declared oracle"
        }
      ]
    },
    {
      "id": "validation-malformed-count-and-message",
      "function": "validation",
      "declaration": {
        "conforms": false,
        "violation_count": "1",
        "message_contains": 7
      },
      "result": {
        "kind": "validation",
        "conforms": false,
        "messages": [
          "7"
        ]
      },
      "expected": [
        {
          "check_id": "oracle.validation",
          "status": "blocked",
          "expected": {
            "conforms": false,
            "violation_count": "1",
            "message_contains": 7
          },
          "actual": {
            "conforms": false,
            "violation_count": 1,
            "messages": [
              "7"
            ]
          },
          "message": "SHACL oracle is malformed"
        }
      ]
    },
    {
      "id": "reason-missing-declaration",
      "function": "reason",
      "declaration": {},
      "result": null,
      "expected": {
        "check_id": "oracle.reason",
        "status": "blocked",
        "expected": {
          "conclusions": "required list"
        },
        "actual": {
          "result": "unavailable"
        },
        "message": "forward-chain oracle is missing or malformed"
      }
    },
    {
      "id": "reason-unavailable-original-str-coercion",
      "function": "reason",
      "declaration": [
        null,
        true,
        1,
        {
          "b": 2,
          "a": 1
        }
      ],
      "result": null,
      "expected": {
        "check_id": "oracle.reason",
        "status": "blocked",
        "expected": {
          "conclusions": [
            "1",
            "None",
            "True",
            "{'b': 2, 'a': 1}"
          ]
        },
        "actual": {
          "result": "unavailable"
        },
        "message": "forward-chain result is unavailable"
      }
    },
    {
      "id": "reason-order-insensitive-duplicates-retained",
      "function": "reason",
      "declaration": [
        "B",
        "A",
        "A"
      ],
      "result": {
        "kind": "reason",
        "conclusions": [
          "A",
          "B",
          "A"
        ]
      },
      "expected": {
        "check_id": "oracle.reason",
        "status": "passed",
        "expected": {
          "conclusions": [
            "A",
            "A",
            "B"
          ]
        },
        "actual": {
          "conclusions": [
            "A",
            "A",
            "B"
          ]
        },
        "message": "forward-chain conclusions match exactly"
      }
    },
    {
      "id": "reason-duplicate-count-differs",
      "function": "reason",
      "declaration": [
        "A",
        "A"
      ],
      "result": {
        "kind": "reason",
        "conclusions": [
          "A"
        ]
      },
      "expected": {
        "check_id": "oracle.reason",
        "status": "failed",
        "expected": {
          "conclusions": [
            "A",
            "A"
          ]
        },
        "actual": {
          "conclusions": [
            "A"
          ]
        },
        "message": "forward-chain conclusions differ from the exact oracle"
      }
    },
    {
      "id": "boolean-integer-declaration-blocked",
      "function": "boolean",
      "declaration": 1,
      "result": {
        "kind": "ask",
        "boolean": true
      },
      "expected": {
        "check_id": "oracle.boolean",
        "status": "blocked",
        "expected": {
          "boolean": "required"
        },
        "actual": {
          "result": "unavailable"
        },
        "message": "boolean oracle is missing"
      }
    },
    {
      "id": "boolean-unavailable",
      "function": "boolean",
      "declaration": true,
      "result": null,
      "expected": {
        "check_id": "oracle.boolean",
        "status": "blocked",
        "expected": {
          "boolean": true
        },
        "actual": {
          "result": "unavailable"
        },
        "message": "ASK result is unavailable"
      }
    },
    {
      "id": "boolean-identity-integer-result-differs",
      "function": "boolean",
      "declaration": true,
      "result": {
        "kind": "ask",
        "boolean": 1
      },
      "expected": {
        "check_id": "oracle.boolean",
        "status": "failed",
        "expected": {
          "boolean": true
        },
        "actual": {
          "boolean": 1
        },
        "message": "ASK boolean differs"
      }
    },
    {
      "id": "boolean-pass",
      "function": "boolean",
      "declaration": true,
      "result": {
        "kind": "ask",
        "boolean": true
      },
      "expected": {
        "check_id": "oracle.boolean",
        "status": "passed",
        "expected": {
          "boolean": true
        },
        "actual": {
          "boolean": true
        },
        "message": "ASK boolean matches"
      }
    }
  ]
}
"#####;
const FROZEN_RUNTIME_REASON: &str = r#####"{
  "schema": "original-runtime-reason-string-assets/1",
  "cases": [
    {
      "id": "same-pass-chain",
      "facts": "A(a)",
      "rules": "IF A(?x) THEN B(?x)\nIF B(?x) THEN C(?x)",
      "max_iterations": 50,
      "expected": {
        "input_facts": [
          "A(a)"
        ],
        "rules": [
          {
            "rule_id": "rule_1",
            "text": "IF A(?x) THEN B(?x)",
            "confidence": 1.0,
            "priority": 0
          },
          {
            "rule_id": "rule_2",
            "text": "IF B(?x) THEN C(?x)",
            "confidence": 1.0,
            "priority": 0
          }
        ],
        "inferences": [
          {
            "conclusion": "B(a)",
            "rule_id": "rule_1",
            "rule_text": "IF A(?x) THEN B(?x)",
            "premises": [
              "A(a)"
            ],
            "confidence": 1.0,
            "explanation": "Given the premises: A(a), we conclude: B(a) using rule 'rule_1'."
          },
          {
            "conclusion": "C(a)",
            "rule_id": "rule_2",
            "rule_text": "IF B(?x) THEN C(?x)",
            "premises": [
              "B(a)"
            ],
            "confidence": 1.0,
            "explanation": "Given the premises: B(a), we conclude: C(a) using rule 'rule_2'."
          }
        ],
        "all_facts": [
          "A(a)",
          "B(a)",
          "C(a)"
        ],
        "max_iterations": 50,
        "semantics": "semantica.textual-forward-chain.v1",
        "complete": true
      }
    },
    {
      "id": "same-pass-premise-merge",
      "facts": "A(a)\nB(a)",
      "rules": "IF A(?x) THEN C(?x)\nIF B(?x) THEN C(?x)",
      "max_iterations": 50,
      "expected": {
        "input_facts": [
          "A(a)",
          "B(a)"
        ],
        "rules": [
          {
            "rule_id": "rule_1",
            "text": "IF A(?x) THEN C(?x)",
            "confidence": 1.0,
            "priority": 0
          },
          {
            "rule_id": "rule_2",
            "text": "IF B(?x) THEN C(?x)",
            "confidence": 1.0,
            "priority": 0
          }
        ],
        "inferences": [
          {
            "conclusion": "C(a)",
            "rule_id": "rule_1",
            "rule_text": "IF A(?x) THEN C(?x)",
            "premises": [
              "A(a)",
              "B(a)"
            ],
            "confidence": 1.0,
            "explanation": "Given the premises: A(a), B(a), we conclude: C(a) using rule 'rule_1'."
          }
        ],
        "all_facts": [
          "A(a)",
          "B(a)",
          "C(a)"
        ],
        "max_iterations": 50,
        "semantics": "semantica.textual-forward-chain.v1",
        "complete": true
      }
    },
    {
      "id": "original-prefix-substitution",
      "facts": "P(A,B)",
      "rules": "IF P(?x,?xy) THEN Result(?x,?xy)",
      "max_iterations": 50,
      "expected": {
        "input_facts": [
          "P(A,B)"
        ],
        "rules": [
          {
            "rule_id": "rule_1",
            "text": "IF P(?x,?xy) THEN Result(?x,?xy)",
            "confidence": 1.0,
            "priority": 0
          }
        ],
        "inferences": [
          {
            "conclusion": "Result(A,Ay)",
            "rule_id": "rule_1",
            "rule_text": "IF P(?x,?xy) THEN Result(?x,?xy)",
            "premises": [
              "P(A,B)"
            ],
            "confidence": 1.0,
            "explanation": "Given the premises: P(A,B), we conclude: Result(A,Ay) using rule 'rule_1'."
          }
        ],
        "all_facts": [
          "P(A,B)",
          "Result(A,Ay)"
        ],
        "max_iterations": 50,
        "semantics": "semantica.textual-forward-chain.v1",
        "complete": true
      }
    },
    {
      "id": "one-pass-chain-converges",
      "facts": "A(a)",
      "rules": "IF A(?x) THEN B(?x)\nIF B(?x) THEN C(?x)",
      "max_iterations": 1,
      "expected": {
        "input_facts": [
          "A(a)"
        ],
        "rules": [
          {
            "rule_id": "rule_1",
            "text": "IF A(?x) THEN B(?x)",
            "confidence": 1.0,
            "priority": 0
          },
          {
            "rule_id": "rule_2",
            "text": "IF B(?x) THEN C(?x)",
            "confidence": 1.0,
            "priority": 0
          }
        ],
        "inferences": [
          {
            "conclusion": "B(a)",
            "rule_id": "rule_1",
            "rule_text": "IF A(?x) THEN B(?x)",
            "premises": [
              "A(a)"
            ],
            "confidence": 1.0,
            "explanation": "Given the premises: A(a), we conclude: B(a) using rule 'rule_1'."
          },
          {
            "conclusion": "C(a)",
            "rule_id": "rule_2",
            "rule_text": "IF B(?x) THEN C(?x)",
            "premises": [
              "B(a)"
            ],
            "confidence": 1.0,
            "explanation": "Given the premises: B(a), we conclude: C(a) using rule 'rule_2'."
          }
        ],
        "all_facts": [
          "A(a)",
          "B(a)",
          "C(a)"
        ],
        "max_iterations": 1,
        "semantics": "semantica.textual-forward-chain.v1",
        "complete": true
      }
    },
    {
      "id": "second-closure-probe-rejects",
      "facts": "A(a)",
      "rules": "IF B(?x) THEN C(?x)\nIF A(?x) THEN B(?x)",
      "max_iterations": 1,
      "expected": {
        "error_class": "SemanticRuntimeError",
        "error_message": "forward reasoning did not converge within max_iterations=1"
      }
    },
    {
      "id": "semantic-duplicates-rejected",
      "facts": "A(a)",
      "rules": "IF A(?x) THEN B(?x)\nif A(?x) then B(?x)",
      "max_iterations": 50,
      "expected": {
        "error_class": "SemanticInputError",
        "error_message": "duplicate forward rule conditions/conclusion are ambiguous"
      }
    },
    {
      "id": "unbound-conclusion-rejected",
      "facts": "A(a)",
      "rules": "IF A(?x) THEN B(?y)",
      "max_iterations": 50,
      "expected": {
        "error_class": "SemanticInputError",
        "error_message": "forward rule conclusion contains unbound variables: ?y"
      }
    },
    {
      "id": "ground-fact-required",
      "facts": "A(?x)",
      "rules": "IF A(?x) THEN B(?x)",
      "max_iterations": 50,
      "expected": {
        "error_class": "SemanticInputError",
        "error_message": "forward facts must be ground atoms without variables: 'A(?x)'"
      }
    },
    {
      "id": "swrl-outside-profile",
      "facts": "A(a)",
      "rules": "IF A(?x) THEN swrlb:greaterThan(?x)",
      "max_iterations": 50,
      "expected": {
        "error_class": "SemanticInputError",
        "error_message": "SWRL syntax and built-ins are outside rule.forward_chain; provide an explicit semantica-rule adaptation"
      }
    },
    {
      "id": "normalization-comment-strip-sort-dedup",
      "facts": "# note\n A(z)\n A(a)\nA(a)\n",
      "rules": "# comment\nIF A(?x) THEN B(?x)\n",
      "max_iterations": 50,
      "expected": {
        "input_facts": [
          "A(a)",
          "A(z)"
        ],
        "rules": [
          {
            "rule_id": "rule_1",
            "text": "IF A(?x) THEN B(?x)",
            "confidence": 1.0,
            "priority": 0
          }
        ],
        "inferences": [
          {
            "conclusion": "B(a)",
            "rule_id": "rule_1",
            "rule_text": "IF A(?x) THEN B(?x)",
            "premises": [
              "A(a)"
            ],
            "confidence": 1.0,
            "explanation": "Given the premises: A(a), we conclude: B(a) using rule 'rule_1'."
          },
          {
            "conclusion": "B(z)",
            "rule_id": "rule_1",
            "rule_text": "IF A(?x) THEN B(?x)",
            "premises": [
              "A(z)"
            ],
            "confidence": 1.0,
            "explanation": "Given the premises: A(z), we conclude: B(z) using rule 'rule_1'."
          }
        ],
        "all_facts": [
          "A(a)",
          "A(z)",
          "B(a)",
          "B(z)"
        ],
        "max_iterations": 50,
        "semantics": "semantica.textual-forward-chain.v1",
        "complete": true
      }
    }
  ],
  "frozen_before_execution": true
}
"#####;

#[test]
fn original_runtime_reason_ten_frozen_oracles() {
    let fixture: Value = serde_json::from_str(FROZEN_RUNTIME_REASON).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let actual = book_runner::runtime_reason(
            case["facts"].as_str().unwrap(),
            case["rules"].as_str().unwrap(),
            case["max_iterations"].as_u64().unwrap(),
        );
        if case["expected"].get("error_class").is_some() {
            let error = actual.unwrap_err().to_string();
            if case["expected"]["error_class"] == "SemanticRuntimeError" {
                assert!(error.contains("runtime_reason_convergence"));
            } else {
                assert!(error.contains("runtime_reason_input"));
            }
        } else {
            assert_eq!(
                semantica_contracts::json::canonical_json(&actual.unwrap()).unwrap(),
                semantica_contracts::json::canonical_json(&case["expected"]).unwrap(),
                "original runtime.reason {}",
                case["id"]
            );
        }
    }
}

const FROZEN_PRECONDITIONS: &str = r###"{
  "profile": "original.chapter-known-precondition/1",
  "cases": [
    {
      "package_id": "semantica.chapter_packages.vol1.ch01",
      "manifest_path": "semantica/chapter_packages/vol1/ch01/manifest.yaml",
      "manifest_sha256": "c79cd4aba1ea63fb1cb415000fb5a38eb4c2a8534aab9205a69d5a5b4bce10bd",
      "scenario_id": "OE-V1-CH01-SCN-001",
      "scenario": {
        "id": "OE-V1-CH01-SCN-001",
        "status": "absent",
        "execution": {
          "operation": "requirements_specification_report",
          "provider": "unimplemented"
        },
        "inputs": [
          "book-roadmap",
          "philosophy-to-engineering"
        ],
        "oracle": {
          "id": "OE-V1-CH01-ORC-001",
          "status": "absent",
          "gap": "尚无机器可判定的目的、范围、共同体与概念化边界报告。"
        }
      },
      "expected_status": "blocked",
      "expected_contract_reason": "scenario has no machine-checkable ready oracle",
      "expected_check": {
        "check_id": "oracle.contract",
        "status": "blocked",
        "expected": {
          "runner_contract": "semantica.chapter_packages.SemanticPackageRunner/v1"
        },
        "actual": {
          "error_code": "missing_oracle"
        },
        "message": "scenario has no machine-checkable ready oracle"
      }
    },
    {
      "package_id": "semantica.chapter_packages.vol1.ch06",
      "manifest_path": "semantica/chapter_packages/vol1/ch06/manifest.yaml",
      "manifest_sha256": "d9504cabd7155b2a71eb55094956c14e9cf3e609c2fcc96de85fb6ce6fb4ce8d",
      "scenario_id": "OE-V1-CH06-SCN-END-TO-END-001",
      "scenario": {
        "id": "OE-V1-CH06-SCN-END-TO-END-001",
        "status": "absent",
        "execution": {
          "provider": "SemanticRuntime",
          "operation": "domain_case_end_to_end"
        },
        "inputs": [
          "autonomous-driving",
          "bim-ontology",
          "aerospace-fmea",
          "manufacturing-scheduling"
        ],
        "oracle": {
          "id": "OE-V1-CH06-ORC-001",
          "status": "absent",
          "gap": "书中案例仍是文本模式，尚无可执行端到端 oracle。"
        }
      },
      "expected_status": "blocked",
      "expected_contract_reason": "scenario has no machine-checkable ready oracle",
      "expected_check": {
        "check_id": "oracle.contract",
        "status": "blocked",
        "expected": {
          "runner_contract": "semantica.chapter_packages.SemanticPackageRunner/v1"
        },
        "actual": {
          "error_code": "missing_oracle"
        },
        "message": "scenario has no machine-checkable ready oracle"
      }
    },
    {
      "package_id": "semantica.chapter_packages.vol1.ch08",
      "manifest_path": "semantica/chapter_packages/vol1/ch08/manifest.yaml",
      "manifest_sha256": "7a837030ed48ad707d207609d32ad1b1310278cb4c73db0b218818dca33ca074",
      "scenario_id": "OE-V1-CH08-SCN-GUARDRAIL-001",
      "scenario": {
        "id": "OE-V1-CH08-SCN-GUARDRAIL-001",
        "status": "adapter",
        "execution": {
          "provider": "legacy_book_case_reference",
          "operation": "propose_validate_execute_audit",
          "runner_status": "blocked_unsupported_operation_contract"
        },
        "inputs": [
          "ontology-guided-agent"
        ],
        "oracle": {
          "id": "OE-V1-CH08-ORC-001",
          "status": "ready_reference_only",
          "verdicts": [
            {
              "request": "把3号车床转入维护",
              "expected": "APPROVED"
            },
            {
              "request": "让7号铣床直接开工",
              "expected": "REJECTED"
            },
            {
              "request": "给幻影设备派活",
              "expected": "REJECTED"
            }
          ]
        }
      },
      "expected_status": "blocked",
      "expected_contract_reason": "scenario has no machine-checkable ready oracle",
      "expected_check": {
        "check_id": "oracle.contract",
        "status": "blocked",
        "expected": {
          "runner_contract": "semantica.chapter_packages.SemanticPackageRunner/v1"
        },
        "actual": {
          "error_code": "missing_oracle"
        },
        "message": "scenario has no machine-checkable ready oracle"
      }
    }
  ]
}
"###;

#[test]
fn current_same_runner_and_dataset() {
    use semantica_runtime::execution_provenance as prov;
    let source = fixture(false, "select");
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = DatasetSession::new();
    let mut state = BookRunnerState::default();
    let mut observed = identity();
    observed["runtime_commit"] = json!(format!("semantica.rust-source/{}", "c".repeat(64)));
    let run = book_runner::run(
        &mut state,
        &catalog,
        &source,
        &mut session,
        "p",
        Some("s"),
        &observed,
    )
    .unwrap();
    assert_eq!(run["status"], "passed");
    let full = book_runner::execution_provenance(&state, &mut session, &run, &observed).unwrap();
    let receipt = serde_json::to_value(&full.receipt).unwrap();
    assert!(prov::verify_receipt(&receipt).unwrap());
    assert!(prov::verify_receipt_bindings(&receipt).unwrap());
    assert_eq!(receipt["dataset_sha256"], run["receipt"]["dataset_sha256"]);
    assert_eq!(receipt["runtime_commit"], observed["runtime_commit"]);
    assert_eq!(receipt["oracle_report"], run["oracle_report"]);
    assert!(
        receipt["provenance_bundle"]["records"]
            .as_array()
            .unwrap()
            .len()
            > 9
    );
    assert!(
        !receipt["capability_report"]["payload"]["unknown"]
            .as_array()
            .unwrap()
            .contains(&json!("provenance.bundle"))
    );
    assert!(
        receipt["capability_report"]["payload"]["unknown"]
            .as_array()
            .unwrap()
            .contains(&json!("semantic.release.verify"))
    );
    assert_eq!(full.runtime_origin, "unknown");
    assert_eq!(full.source_authenticity, "unknown");
    assert_eq!(full.engineering_adoption, "unknown");
    assert_eq!(full.release_status, "blocked");
    assert!(
        book_runner::execution_provenance(
            &BookRunnerState::default(),
            &mut session,
            &run,
            &observed
        )
        .is_err()
    );
    let mut changed = run.clone();
    changed["receipt"]["cq_report"]["status"] = json!("passed-forged");
    assert!(book_runner::execution_provenance(&state, &mut session, &changed, &observed).is_err());
    let mut identity_changed = observed.clone();
    identity_changed["runtime_commit"] = json!("c".repeat(40));
    assert!(
        book_runner::execution_provenance(&state, &mut session, &run, &identity_changed).is_err()
    );
    session
        .load(
            b"<urn:changed> <urn:p> <urn:o> .",
            oxigraph::io::RdfFormat::NTriples,
            None,
            None,
        )
        .unwrap();
    assert!(book_runner::execution_provenance(&state, &mut session, &run, &observed).is_err());
}

fn release_source_observation() -> (Value, Value) {
    let source_sha256 = semantica_runtime::sha256(include_bytes!("../src/book_runner.rs"));
    let executable = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    let executable_sha256 = semantica_runtime::sha256(&executable);
    let source = json!({
        "schema":"semantica.refinery-rust-source/1", "source_sha256":source_sha256,
        "executable_sha256":executable_sha256, "executable_size_bytes":executable.len(),
        "status":"source_bound", "compilation_origin":"unknown", "engineering_adoption":"unknown"
    });
    let observed = json!({
        "runtime_version":"observed-rust", "runtime_commit":format!("semantica.rust-source/{source_sha256}"),
        "runtime_artifact_sha256":executable_sha256, "created_at":"2000-01-01T00:00:00Z",
        "compilation_origin":"unknown", "identity_authority":"not_connected"
    });
    (source, observed)
}

#[test]
fn current_release_content_from_actual_retained_execution() {
    use semantica_runtime::execution_provenance as prov;
    let mut source = fixture(false, "select");
    let mut manifest: Value = serde_json::from_slice(&source.0["p/manifest.json"]).unwrap();
    manifest["release_status"] = json!("complete");
    source.0.insert(
        "p/manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = DatasetSession::new();
    let mut state = BookRunnerState::default();
    let (runtime_source, observed) = release_source_observation();
    let run = book_runner::run(
        &mut state,
        &catalog,
        &source,
        &mut session,
        "p",
        Some("s"),
        &observed,
    )
    .unwrap();
    assert_eq!(run["status"], "passed");
    let original = semantica_contracts::json::canonical_json(&run).unwrap();
    let full = book_runner::release_content(&state, &mut session, &run, &observed, &runtime_source)
        .unwrap();
    assert_eq!(full.content_verdict.status, "complete");
    assert_eq!(full.content_verdict.checks.len(), 17);
    assert!(full.content_verdict.checks.iter().all(|v| v.passed));
    assert!(
        full.content_verdict
            .checks
            .windows(2)
            .all(|v| v[0].check_id < v[1].check_id)
    );
    assert_eq!(full.runner_execution, "retained");
    assert_eq!(full.formal_release, "not_published");
    assert_eq!(full.runtime_origin, "unknown");
    assert_eq!(full.source_authenticity, "unknown");
    assert_eq!(full.engineering_adoption, "unknown");
    assert_eq!(
        full.source_run_receipt_sha256,
        run["receipt"]["receipt_sha256"]
    );
    let receipt = serde_json::to_value(&full.receipt).unwrap();
    assert!(prov::verify_receipt(&receipt).unwrap());
    assert!(prov::verify_receipt_bindings(&receipt).unwrap());
    assert_eq!(receipt["capability_report"]["status"], "passed");
    assert_eq!(
        receipt["capability_report"]["payload"]["unknown"],
        json!([])
    );
    assert_eq!(receipt["oracle_report"], run["oracle_report"]);
    book_runner::verify_release_content(&state, &mut session, &full, &observed, &runtime_source)
        .unwrap();
    assert_eq!(
        semantica_contracts::json::canonical_json(&run).unwrap(),
        original
    );

    assert!(
        book_runner::release_content(
            &BookRunnerState::default(),
            &mut session,
            &run,
            &observed,
            &runtime_source
        )
        .is_err()
    );
    assert!(
        book_runner::verify_release_content(
            &BookRunnerState::default(),
            &mut session,
            &full,
            &observed,
            &runtime_source
        )
        .is_err()
    );
    let mut changed = run.clone();
    changed["receipt"]["cq_report"]["status"] = json!("failed");
    changed["receipt"]
        .as_object_mut()
        .unwrap()
        .remove("receipt_sha256");
    changed["receipt"]["receipt_sha256"] = json!(semantica_runtime::sha256(
        &semantica_contracts::json::canonical_json(&changed["receipt"]).unwrap()
    ));
    assert!(
        book_runner::release_content(&state, &mut session, &changed, &observed, &runtime_source)
            .is_err()
    );
    let mut changed_observation = full.clone();
    changed_observation.content_verdict.status = "blocked".into();
    assert!(
        book_runner::verify_release_content(
            &state,
            &mut session,
            &changed_observation,
            &observed,
            &runtime_source
        )
        .is_err()
    );
    let mut changed_source = runtime_source.clone();
    changed_source["source_sha256"] = json!("f".repeat(64));
    assert!(
        book_runner::verify_release_content(
            &state,
            &mut session,
            &full,
            &observed,
            &changed_source
        )
        .is_err()
    );
    let mut changed_identity = observed.clone();
    changed_identity["runtime_version"] = json!("different-observation");
    assert!(
        book_runner::release_content(
            &state,
            &mut session,
            &run,
            &changed_identity,
            &runtime_source
        )
        .is_err()
    );
    session
        .load(
            b"<urn:changed> <urn:p> <urn:o> .",
            oxigraph::io::RdfFormat::NTriples,
            None,
            None,
        )
        .unwrap();
    assert!(
        book_runner::verify_release_content(
            &state,
            &mut session,
            &full,
            &observed,
            &runtime_source
        )
        .is_err()
    );
}

#[test]
fn current_release_partial_manifest_stays_blocked() {
    let source = fixture(false, "select");
    let catalog = Catalog::read(&source, "registry.json").unwrap();
    let mut session = DatasetSession::new();
    let mut state = BookRunnerState::default();
    let (runtime_source, observed) = release_source_observation();
    let mut run = book_runner::run(
        &mut state,
        &catalog,
        &source,
        &mut session,
        "p",
        Some("s"),
        &observed,
    )
    .unwrap();
    assert_eq!(run["status"], "passed");
    // Caller status is not used instead of the private actual execution.
    run["status"] = json!("failed");
    let full = book_runner::release_content(&state, &mut session, &run, &observed, &runtime_source)
        .unwrap();
    assert_eq!(full.content_verdict.status, "blocked");
    assert_eq!(
        full.content_verdict.reasons,
        vec!["package.declared_release_status"]
    );
    assert_eq!(full.formal_release, "not_published");
    book_runner::verify_release_content(&state, &mut session, &full, &observed, &runtime_source)
        .unwrap();
}
