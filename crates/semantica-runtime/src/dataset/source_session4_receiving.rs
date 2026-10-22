//! The fixture was observed with the pinned original before this implementation.
use super::*;

fn corpus() -> Value {
    let raw = crate::private_test_fixtures::text_ref!("source-session-48/original.json");
    assert_eq!(
        crate::sha256(raw.as_bytes()),
        "5cf9d5eb482a901a50aef3fe56c8984496965b5a954894f2854a89d0c46a0651"
    );
    serde_json::from_str(raw).unwrap()
}
fn request(case: &Value) -> Request4 {
    let mut resources = Vec::new();
    let mut actions = Vec::new();
    for (i, row) in case["rows"].as_array().unwrap().iter().enumerate() {
        let old = &row["action"];
        let mut a = json!({"id": old.get("id").cloned().unwrap_or(json!(format!("action-{i}"))),
            "operation":old["op"],"resource_id":null,"query":null,"namespaces":{},
            "bindings":[],"base_uri":null,"graph_name":null,"use_cache":false,
            "serialize_request":null,"before_snapshot_action_id":null,"after_snapshot_action_id":null});
        match old["op"].as_str().unwrap() {
            "load" => {
                let data = old["data"].as_str().unwrap();
                let id = format!("urn:session48:resource:{i}");
                resources.push(json!({"binding":{"schema":"semantica.source-binding/1","source_id":id,
                    "byte_count":data.len(),"source_sha256":crate::sha256(data.as_bytes())},"data":data,"rdf_format":"ntriples"}));
                a["resource_id"] = json!(id);
                a["graph_name"] = old["graph_name"].clone();
            }
            "query" | "update" => {
                a["query"] = old["sparql"].clone();
                a["use_cache"] =
                    json!(old["op"] == "query" && old["use_cache"].as_bool().unwrap_or(true));
            }
            "clear" => a["graph_name"] = old["graph_name"].clone(),
            "serialize" => {
                a["serialize_request"] = json!({"format":old["format"],"graph_name":old["graph_name"],"base_uri":old["base_uri"]})
            }
            "diff" => {
                a["before_snapshot_action_id"] = old["before"].clone();
                a["after_snapshot_action_id"] = old["after"].clone();
            }
            "snapshot" => {}
            _ => panic!("unfrozen operation"),
        }
        actions.push(a);
    }
    serde_json::from_value(json!({"schema":"semantica.source-session-request/4",
        "profile":"semantica.rdflib760-source-session/4","cache_queries":true,
        "created_at":"2026-10-17T00:00:00Z","resources":resources,"actions":actions}))
    .unwrap()
}
fn checked(r: &Request4, result: &Result4) -> Result<()> {
    validate_relations4_until(r, result, Instant::now() + Duration::from_secs(60))
}
#[test]
fn source_session48_original_histories() {
    let corpus = corpus();
    let mut counts = [0; 3];
    let mut differences = Vec::new();
    for case in corpus["cases"].as_array().unwrap() {
        let r = request(case);
        let actual = execute4(&r).unwrap();
        checked(&r, &actual).unwrap_or_else(|e| panic!("{} receiving: {e:#}", case["id"]));
        assert_eq!(
            json!(actual.trace.initial_before.state),
            case["initial_before"]
        );
        assert_eq!(
            json!(actual.trace.initial_after.state),
            case["initial_after"]
        );
        assert_eq!(actual.initial_state, case["initial_state"]["value"]);
        for (i, ((step, trace), row)) in actual
            .steps
            .iter()
            .zip(&actual.trace.steps)
            .zip(case["rows"].as_array().unwrap())
            .enumerate()
        {
            let mut observed = json!({"action":row["action"],"status":step.status,
                "revision_before":step.revision_before,"revision_after":step.revision_after});
            let counter = match step.status.as_str() {
                "completed" => 0,
                "failed" => 1,
                "skipped" => 2,
                _ => panic!("status"),
            };
            counts[counter] += 1;
            if step.status != "skipped" {
                let result = if step.status == "failed" {
                    let f = step.failure.as_ref().unwrap();
                    json!({"status":"original_error","error":{"module":f.original_module,"type":f.original_class,"message":f.message}})
                } else {
                    let mut v = step.result.clone();
                    if step.operation == "query" {
                        v = step.result["result"].clone();
                        v["revision"] = step.result["revision"].clone();
                        v["cache_hit"] = step.result["cache_hit"].clone();
                    } else if step.operation == "serialize" && !v.is_string() {
                        v = v["text"].clone();
                    } else if step.operation == "update" {
                        // Existing public mutation DTO adds the action kind;
                        // the frozen original observer exposes its four values.
                        assert_eq!(v["operation"], "update");
                        v.as_object_mut().unwrap().remove("operation");
                    }
                    json!({"status":"completed","value":v})
                };
                observed["result"] = result;
                observed["before"] = json!(trace.before.as_ref().unwrap().state);
                observed["after"] = json!(trace.after_action.as_ref().unwrap().state);
                observed["after_observation"] =
                    json!(trace.after_observation.as_ref().unwrap().state);
                observed["source_state"] = json!({"status":"completed","value":step.source_state});
                observed["query_cache_entries"] = step.source_state["query_cache_entries"].clone();
            }
            if observed != *row {
                let keys: Vec<_> = observed
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|k| observed[*k] != row[*k])
                    .cloned()
                    .collect();
                differences.push(format!("{} action{i}: {keys:?}", case["id"]));
            }
            println!(
                "OE_SESSION48 {}",
                json!({"case":case["id"],"action":i,"matched":observed==*row,"actual":observed})
            );
        }
    }
    assert_eq!(counts, [58, 5, 5]);
    assert!(differences.is_empty(), "{differences:?}");
}

#[test]
fn source_session48_rejects_tampering() {
    let corpus = corpus();
    let r = request(&corpus["cases"][1]);
    let good = execute4(&r).unwrap();
    checked(&r, &good).unwrap();
    let base = json!(good);
    let edits = [
        ("/schema", json!("semantica.source-session-result/3")),
        ("/request_sha256", json!("0".repeat(64))),
        ("/source_bindings_sha256", json!("0".repeat(64))),
        ("/trace/steps", json!([])),
        ("/trace/steps/1/action_id", json!("foreign")),
        ("/trace/steps/1/source", Value::Null),
        ("/trace/steps/1/turtle", Value::Null),
        (
            "/trace/steps/1/before",
            base["trace"]["steps"][1]["after_action"].clone(),
        ),
        (
            "/trace/steps/2/before",
            base["trace"]["initial_before"].clone(),
        ),
        ("/steps/1/result", json!("@prefix forged: <urn:forged> .\n")),
        ("/trace/steps/1/turtle/namespace_bytes_after", json!(0)),
        ("/trace/steps/1/turtle/reads", json!([])),
        ("/trace/steps/1/source/quads", json!([])),
        ("/steps/1/source_state/quads", json!([])),
        ("/trace/steps/1/action_started", json!(false)),
        ("/trace/steps/1/complete", json!(false)),
        ("/trace/initial_source", Value::Null),
        (
            "/trace/final_namespace",
            base["trace"]["initial_before"].clone(),
        ),
    ];
    for (pointer, replacement) in edits {
        let mut bad = base.clone();
        *bad.pointer_mut(pointer)
            .unwrap_or_else(|| panic!("missing frozen target {pointer}")) = replacement;
        assert!(bad != base, "noop {pointer}");
        let rejected = serde_json::from_value::<Result4>(bad)
            .map(|v| checked(&r, &v).is_err())
            .unwrap_or(true);
        assert!(rejected, "accepted tamper {pointer}");
        println!("OE_SESSION48_CONTROL {pointer} rejected");
    }
    let mut extra = good.clone();
    extra.trace.steps.push(extra.trace.steps[0].clone());
    assert!(checked(&r, &extra).is_err());
    let failed_request = request(&corpus["cases"][3]);
    let mut failed = execute4(&failed_request).unwrap();
    checked(&failed_request, &failed).unwrap();
    failed.trace.steps[1].turtle = good.trace.steps[1].turtle.clone();
    assert!(checked(&failed_request, &failed).is_err());
    let mut unknown = base;
    unknown["trace"]["unreviewed"] = json!(true);
    assert!(serde_json::from_value::<Result4>(unknown).is_err());
}

#[test]
fn source_session48_capture_and_deadline() {
    let corpus = corpus();
    let mut r = request(&corpus["cases"][1]);
    assert!(execute4_bounded(&r, Instant::now(), MAX_RESULT_BYTES).is_err());
    let good = execute4(&r).unwrap();
    assert!(validate_relations4_until(&r, &good, Instant::now()).is_err());
    let partial = execute4_bounded(&r, Instant::now() + Duration::from_secs(60), 1).unwrap();
    assert_eq!(partial.status, "partial");
    assert!(partial.steps.iter().all(|s| s.status == "skipped"));
    checked(&r, &partial).unwrap();
    let mut false_success = partial.clone();
    false_success.status = "completed".into();
    assert!(checked(&r, &false_success).is_err());
    // The original long-literal attempt reaches the unchanged numeric input
    // guard before mutation. Retain that refusal; it is not a capture-after-write.
    let data = format!("<urn:s> <http://one/p> \"{}\" .", "x".repeat(20_000));
    r.resources[0].data = data.clone();
    r.resources[0].binding.byte_count = data.len() as u64;
    r.resources[0].binding.source_sha256 = crate::sha256(data.as_bytes());
    let partial = execute4_bounded(&r, Instant::now() + Duration::from_secs(60), 30_000).unwrap();
    println!(
        "OE_SESSION48_CAPTURE {}",
        json!({"status":partial.status,"step0":partial.steps[0],"trace0":partial.trace.steps[0],"failure":partial.failure})
    );
    assert_eq!(partial.status, "partial");
    assert!(partial.trace.steps[0].action_started);
    assert!(partial.trace.steps[0].complete);
    assert_eq!(partial.steps[0].revision_after, 0);
    assert_eq!(
        partial.steps[0].failure.as_ref().unwrap().message,
        "numeric resource limit: input_characters"
    );
    checked(&r, &partial).unwrap();
    // Many short admitted terms exercise the intended post-mutation trace limit.
    let data = (0..100)
        .map(|i| format!("<urn:s{i}> <http://one/p> \"x\" .\n"))
        .collect::<String>();
    r.resources[0].data = data.clone();
    r.resources[0].binding.byte_count = data.len() as u64;
    r.resources[0].binding.source_sha256 = crate::sha256(data.as_bytes());
    let partial = execute4_bounded(&r, Instant::now() + Duration::from_secs(60), 30_000).unwrap();
    assert_eq!(partial.status, "partial");
    assert!(partial.trace.steps[0].action_started);
    assert!(!partial.trace.steps[0].complete);
    assert_eq!(partial.steps[0].revision_after, 1);
    assert!(partial.steps[1..].iter().all(|s| s.status == "skipped"));
    checked(&r, &partial).unwrap();
    // The old public versions retain their format refusal.
    let mut old = json!(request(&corpus["cases"][0]));
    old["schema"] = json!("semantica.source-session-request/3");
    old["profile"] = json!("semantica.rdflib760-source-session/3");
    let old: SemanticSourceSessionRequest3 = serde_json::from_value(old).unwrap();
    assert_eq!(
        crate::runtime::RustSemanticRuntime
            .execute_source_session3(&old)
            .unwrap()
            .steps[0]
            .status,
        "failed"
    );
}
