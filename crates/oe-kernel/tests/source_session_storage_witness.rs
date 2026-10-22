//! Frozen ST10 result faults and complete witness relation controls. These
//! finite mutations do not replace independent original49/884/18 or provenance.
use oe_kernel::source_session::{execute_local_source_session3, validate_received3};
use semantica_contracts::{SemanticSourceSessionRequest3, SemanticSourceSessionResult3};
use serde_json::{Value, json};
fn action(id: &str, operation: &str) -> Value {
    json!({"id":id,"operation":operation,"resource_id":null,"query":null,
        "namespaces":{},"bindings":[],"base_uri":null,"graph_name":null,"use_cache":false,
        "serialize_request":null,"before_snapshot_action_id":null,"after_snapshot_action_id":null})
}
fn update(id: &str, query: &str) -> Value {
    let mut a = action(id, "update");
    a["query"] = json!(query);
    a
}
fn serialize(id: &str, format: &str, graph: Option<&str>) -> Value {
    let mut a = action(id, "serialize");
    a["serialize_request"] = json!({"format":format,"graph_name":graph,"base_uri":null});
    a
}
fn diff(id: &str, before: &str, after: Option<&str>) -> Value {
    let mut a = action(id, "diff");
    a["before_snapshot_action_id"] = json!(before);
    a["after_snapshot_action_id"] = json!(after);
    a
}
fn request(data: &str, mut actions: Vec<Value>) -> SemanticSourceSessionRequest3 {
    let mut load = action("load", "load");
    load["resource_id"] = json!("urn:source");
    actions.insert(0, load);
    serde_json::from_value(json!({"schema":"semantica.source-session-request/3",
        "profile":"semantica.rdflib760-source-session/3","cache_queries":true,
        "created_at":"2026-10-14T12:00:00Z",
        "resources":[{"binding":{"schema":"semantica.source-binding/1","source_id":"urn:source",
            "byte_count":data.len(),"source_sha256":semantica_runtime::sha256(data.as_bytes())},
            "data":data,"rdf_format":"turtle"}],"actions":actions}))
    .unwrap()
}
const A: &str = "<urn:diff:a> <urn:p> \"left\" .\n";
const B: &str = "INSERT DATA {<urn:diff:b> <urn:p> \"right\"}";

fn ordered() -> SemanticSourceSessionRequest3 {
    request(
        A,
        vec![
            action("before", "snapshot"),
            serialize("export-a", "nt", None),
            update("update-b", B),
            diff("delta-current", "before", None),
            action("after", "snapshot"),
            diff("delta-explicit", "before", Some("after")),
            serialize("export-ab", "nquads", None),
            action("clear", "clear"),
            serialize("export-empty", "nt", None),
            diff("delta-empty", "after", None),
        ],
    )
}
const MUTATIONS: &str = r###"[
  {
    "id": "guard-explicit-before-sha",
    "edits": [
      {
        "pointer": "/steps/6/result/before_sha256",
        "original": "b89ebd6122534c2504dfe7d263f1a37f857456f2409ec4b7ac45d0675011b4a9",
        "replacement": "0000000000000000000000000000000000000000000000000000000000000000"
      }
    ]
  },
  {
    "id": "guard-explicit-after-sha",
    "edits": [
      {
        "pointer": "/steps/6/result/after_sha256",
        "original": "d0bcf3462e477698577bdca4c2c1da03a84314037fea06bb6143ed01129015b6",
        "replacement": "0000000000000000000000000000000000000000000000000000000000000000"
      }
    ]
  },
  {
    "id": "explicit-added-missing",
    "edits": [
      {
        "pointer": "/steps/6/result/added_quads",
        "original": [
          "<urn:diff:b> <urn:p> \"right\" ."
        ],
        "replacement": []
      }
    ]
  },
  {
    "id": "explicit-added-foreign",
    "edits": [
      {
        "pointer": "/steps/6/result/added_quads",
        "original": [
          "<urn:diff:b> <urn:p> \"right\" ."
        ],
        "replacement": [
          "<urn:forged> <urn:p> \"foreign\" ."
        ]
      }
    ]
  },
  {
    "id": "explicit-removed-invented",
    "edits": [
      {
        "pointer": "/steps/6/result/removed_quads",
        "original": [],
        "replacement": [
          "<urn:diff:a> <urn:p> \"left\" ."
        ]
      }
    ]
  },
  {
    "id": "explicit-unchanged-count",
    "edits": [
      {
        "pointer": "/steps/6/result/unchanged_count",
        "original": 1,
        "replacement": 99
      }
    ]
  },
  {
    "id": "explicit-changed-false",
    "edits": [
      {
        "pointer": "/steps/6/result/changed",
        "original": true,
        "replacement": false
      }
    ]
  },
  {
    "id": "explicit-whole-no-change",
    "edits": [
      {
        "pointer": "/steps/6/result",
        "original": {
          "added_quads": [
            "<urn:diff:b> <urn:p> \"right\" ."
          ],
          "after_sha256": "d0bcf3462e477698577bdca4c2c1da03a84314037fea06bb6143ed01129015b6",
          "before_sha256": "b89ebd6122534c2504dfe7d263f1a37f857456f2409ec4b7ac45d0675011b4a9",
          "changed": true,
          "created_at": "2026-10-14T12:00:00Z",
          "removed_quads": [],
          "schema_version": "1.0",
          "unchanged_count": 1
        },
        "replacement": {
          "added_quads": [],
          "after_sha256": "d0bcf3462e477698577bdca4c2c1da03a84314037fea06bb6143ed01129015b6",
          "before_sha256": "b89ebd6122534c2504dfe7d263f1a37f857456f2409ec4b7ac45d0675011b4a9",
          "changed": false,
          "created_at": "2026-10-14T12:00:00Z",
          "removed_quads": [],
          "schema_version": "1.0",
          "unchanged_count": 2
        }
      }
    ]
  },
  {
    "id": "implicit-current-after-sha",
    "edits": [
      {
        "pointer": "/steps/4/result/after_sha256",
        "original": "d0bcf3462e477698577bdca4c2c1da03a84314037fea06bb6143ed01129015b6",
        "replacement": "b89ebd6122534c2504dfe7d263f1a37f857456f2409ec4b7ac45d0675011b4a9"
      }
    ]
  },
  {
    "id": "implicit-current-whole-no-change",
    "edits": [
      {
        "pointer": "/steps/4/result",
        "original": {
          "added_quads": [
            "<urn:diff:b> <urn:p> \"right\" ."
          ],
          "after_sha256": "d0bcf3462e477698577bdca4c2c1da03a84314037fea06bb6143ed01129015b6",
          "before_sha256": "b89ebd6122534c2504dfe7d263f1a37f857456f2409ec4b7ac45d0675011b4a9",
          "changed": true,
          "created_at": "2026-10-14T12:00:00Z",
          "removed_quads": [],
          "schema_version": "1.0",
          "unchanged_count": 1
        },
        "replacement": {
          "added_quads": [],
          "after_sha256": "d0bcf3462e477698577bdca4c2c1da03a84314037fea06bb6143ed01129015b6",
          "before_sha256": "b89ebd6122534c2504dfe7d263f1a37f857456f2409ec4b7ac45d0675011b4a9",
          "changed": false,
          "created_at": "2026-10-14T12:00:00Z",
          "removed_quads": [],
          "schema_version": "1.0",
          "unchanged_count": 2
        }
      }
    ]
  },
  {
    "id": "implicit-cleared-removed-missing",
    "edits": [
      {
        "pointer": "/steps/10/result/removed_quads",
        "original": [
          "<urn:diff:a> <urn:p> \"left\" .",
          "<urn:diff:b> <urn:p> \"right\" ."
        ],
        "replacement": [
          "<urn:diff:a> <urn:p> \"left\" ."
        ]
      }
    ]
  },
  {
    "id": "serialize-wrong-term",
    "edits": [
      {
        "pointer": "/steps/2/result/text",
        "original": "<urn:diff:a> <urn:p> \"left\" .\n",
        "replacement": "<urn:diff:a> <urn:p> \"wrong\" .\n"
      }
    ]
  },
  {
    "id": "serialize-missing-data",
    "edits": [
      {
        "pointer": "/steps/2/result/text",
        "original": "<urn:diff:a> <urn:p> \"left\" .\n",
        "replacement": ""
      }
    ]
  },
  {
    "id": "serialize-invented-warning",
    "edits": [
      {
        "pointer": "/steps/2/result/warnings",
        "original": [],
        "replacement": [
          "NTSerializer does not support base."
        ]
      }
    ]
  },
  {
    "id": "serialize-nonempty-after-clear",
    "edits": [
      {
        "pointer": "/steps/9/result/text",
        "original": "",
        "replacement": "<urn:diff:a> <urn:p> \"left\" .\n"
      }
    ]
  },
  {
    "id": "serialize-lost-final-newline",
    "edits": [
      {
        "pointer": "/steps/2/result/text",
        "original": "<urn:diff:a> <urn:p> \"left\" .\n",
        "replacement": "<urn:diff:a> <urn:p> \"left\" ."
      }
    ]
  }
]"###;

#[test]
fn source_session3_rejects_all_frozen_storage_counterexamples() {
    let input = ordered();
    let actual = execute_local_source_session3(&input).unwrap();
    assert_eq!(actual.status, "completed");
    assert_eq!(actual.steps[2].result, json!({"text":A,"warnings":[]}));
    assert_eq!(actual.steps[4].result, actual.steps[6].result);
    assert_eq!(
        actual.steps[4].result["added_quads"],
        json!(["<urn:diff:b> <urn:p> \"right\" ."])
    );
    validate_received3(&input, &actual).unwrap();
    let mutations: Vec<Value> = serde_json::from_str(MUTATIONS).unwrap();
    assert_eq!(mutations.len(), 16);
    for mutation in mutations {
        let mut bad = serde_json::to_value(&actual).unwrap();
        for edit in mutation["edits"].as_array().unwrap() {
            let slot = bad.pointer_mut(edit["pointer"].as_str().unwrap()).unwrap();
            assert_eq!(
                *slot, edit["original"],
                "original independent value: {}",
                mutation["id"]
            );
            *slot = edit["replacement"].clone();
        }
        let bad: SemanticSourceSessionResult3 = serde_json::from_value(bad).unwrap();
        assert!(
            validate_received3(&input, &bad).is_err(),
            "{}",
            mutation["id"]
        );
    }
}
#[test]
fn source_session3_rejects_missing_or_inconsistent_witnesses() {
    let input = ordered();
    let actual = execute_local_source_session3(&input).unwrap();
    validate_received3(&input, &actual).unwrap();
    let mut bad = actual.clone();
    bad.storage_witnesses.remove(0);
    assert!(validate_received3(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.storage_witnesses.push(bad.storage_witnesses[0].clone());
    assert!(validate_received3(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.storage_witnesses[0].action_id = "before".into();
    assert!(validate_received3(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.storage_witnesses[0].serialization = None;
    assert!(validate_received3(&input, &bad).is_err());
    // Even coherently changing both text and its witness cannot change one
    // selected quad while preserving only the count in the actual source state.
    let mut bad = actual.clone();
    bad.storage_witnesses[0]
        .serialization
        .as_mut()
        .unwrap()
        .rows[0]
        .object
        .value = "wrong".into();
    bad.steps[2].result["text"] = json!("<urn:diff:a> <urn:p> \"wrong\" .\n");
    assert!(
        validate_received3(&input, &bad)
            .unwrap_err()
            .to_string()
            .contains("source_coverage")
    );
    let mut bad = actual.clone();
    bad.storage_witnesses[1].current_source = None;
    assert!(validate_received3(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.storage_witnesses[1]
        .current_source
        .as_mut()
        .unwrap()
        .quads[0]
        .object
        .value = "wrong".into();
    assert!(
        validate_received3(&input, &bad)
            .unwrap_err()
            .to_string()
            .contains("source_coverage")
    );
    // Keeping the complete typed source but replacing a canonical endpoint with
    // another valid, same-count snapshot is checked by the original algorithm.
    let mut bad = actual.clone();
    let current = bad.storage_witnesses[1].current_snapshot.as_mut().unwrap();
    current.canonical_nquads = current.canonical_nquads.replace("left", "fake");
    current.dataset_sha256 = semantica_runtime::sha256(current.canonical_nquads.as_bytes());
    assert!(
        validate_received3(&input, &bad)
            .unwrap_err()
            .to_string()
            .contains("snapshot_content")
    );
    let mut bad = actual.clone();
    bad.storage_witnesses[1]
        .current_source
        .as_mut()
        .unwrap()
        .contexts
        .clear();
    assert!(validate_received3(&input, &bad).is_err());
    let mut bad = actual.clone();
    bad.storage_witnesses[3]
        .serialization
        .as_mut()
        .unwrap()
        .rows
        .swap(0, 1);
    assert!(validate_received3(&input, &bad).is_err());
    let mut bad = actual.clone();
    let row = bad.storage_witnesses[0]
        .serialization
        .as_mut()
        .unwrap()
        .rows[0]
        .clone();
    bad.storage_witnesses[0]
        .serialization
        .as_mut()
        .unwrap()
        .rows
        .push(row);
    assert!(validate_received3(&input, &bad).is_err());
}


#[test]
fn source_session3_receiving_consumes_original_deadline() {
    use std::time::{Duration, Instant};
    let input = ordered();
    let actual = execute_local_source_session3(&input).unwrap();
    // No sleep or wall-clock race: an already elapsed absolute deadline must
    // reject this otherwise valid result, rather than allocate another60s.
    let expired = Instant::now();
    let error = oe_kernel::source_session::validate_received3_until(&input, &actual, expired)
        .unwrap_err();
    assert_eq!(error.to_string(), "source_session_receiving_deadline");
    oe_kernel::source_session::validate_received3_until(
        &input, &actual, Instant::now() + Duration::from_secs(60),
    ).unwrap();
}

#[test]
fn source_session3_implicit_snapshot_binds_explicit_time() {
    for timestamp in ["2026-10-14T12:00:00Z", "original-explicit-time-label"] {
        let mut input = ordered();
        input.created_at = timestamp.into();
        let actual = execute_local_source_session3(&input).unwrap();
        assert_eq!(actual.status, "completed");
        assert_eq!(actual.storage_witnesses[1].current_snapshot.as_ref().unwrap().created_at, timestamp);
        validate_received3(&input, &actual).unwrap();
        let mut bad = actual.clone();
        // Only the implicit endpoint observation time changes. Its complete
        // graph, content SHA, revision and the diff result remain untouched.
        bad.storage_witnesses[1].current_snapshot.as_mut().unwrap().created_at =
            "2026-10-15T12:00:00Z".into();
        let error = validate_received3(&input, &bad).unwrap_err();
        assert_eq!(error.to_string(), "source_session_diff_current_time_identity");
    }
}
