//! Whole original wrapper sequences; expected output is never candidate output.
use super::*;
use serde_json::json;

fn run(case_id: &str) {
    let f: Value = serde_json::from_str(crate::private_test_fixtures::text_ref!("release-inputs-03/crates/semantica-runtime/tests/fixtures/source-persistent-session-original.json"))
    .unwrap();
    assert_eq!(f["original_observations"]["candidate_results_used"], false);
    let case = f["original_plan"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == case_id)
        .unwrap();
    let expected = f["original_observations"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == case_id)
        .unwrap();
    let actions = case["actions"].as_array().unwrap();
    let observations = expected["actions"].as_array().unwrap();
    assert_eq!(actions.len(), observations.len());
    let mut s = DatasetSession::new_source_internal(true);
    let mut failed = Vec::new();
    for (a, e) in actions.iter().zip(observations) {
        let result = (|| -> anyhow::Result<Value> {
            Ok(match a["op"].as_str().unwrap() {
                "load" => {
                    let format = match a["format"].as_str().unwrap() {
                        "nt" => RdfFormat::NTriples,
                        "nquads" => RdfFormat::NQuads,
                        "turtle" => RdfFormat::Turtle,
                        "xml" => RdfFormat::RdfXml,
                        "json-ld" => RdfFormat::JsonLd {
                            profile: Default::default(),
                        },
                        _ => unreachable!(),
                    };
                    serde_json::to_value(s.load_rdflib(
                        a["data"].as_str().unwrap().as_bytes(),
                        format,
                        a["graph"].as_str(),
                    )?)?
                }
                "clear" => serde_json::to_value(s.clear_rdflib(a["graph"].as_str())?)?,
                "delete" => {
                    let r = s.update_rdflib(a["query"].as_str().unwrap())?;
                    json!({"revision":r.revision,"quad_count":r.quad_count,"added":r.added,"removed":r.removed})
                }
                "query" => {
                    let r = s.query_source_internal(
                        a["query"].as_str().unwrap(),
                        &BTreeMap::new(),
                        &serde_json::from_value(a["bindings"].clone())?,
                        a["use_cache"].as_bool().unwrap(),
                        None,
                    )?;
                    let mut result = r.result;
                    result["revision"] = json!(r.revision);
                    result["cache_hit"] = json!(r.cache_hit);
                    result
                }
                _ => unreachable!(),
            })
        })();
        let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if e["status"] == "completed" {
                assert_eq!(
                    result
                        .as_ref()
                        .unwrap_or_else(|err| panic!("{}: {err:#}", e["id"])),
                    &e["raw"],
                    "{}",
                    e["id"]
                );
            } else {
                assert!(
                    result.is_err(),
                    "{}: original error unexpectedly succeeded",
                    e["id"]
                );
            }
            assert_eq!(json!(s.revision), e["revision"], "{} revision", e["id"]);
            assert_eq!(
                json!(s.query_cache.len()),
                e["cache_entries"],
                "{} cache",
                e["id"]
            );
            assert_eq!(
                json!(s.rdflib_projection.quads.as_ref().unwrap().len()),
                e["quad_count"],
                "{} count",
                e["id"]
            );
        }));
        println!(
            "OE_SOURCE_PERSISTENT {} {}",
            e["id"].as_str().unwrap(),
            if checked.is_ok() { "ok" } else { "failed" }
        );
        if let Err(err) = &result {
            println!(
                "OE_SOURCE_PERSISTENT_DIAGNOSTIC {} {}",
                e["id"].as_str().unwrap(),
                serde_json::to_string(&format!("{err:#}")).unwrap()
            );
        }
        if checked.is_err() {
            failed.push(e["id"].clone());
        }
    }
    assert!(failed.is_empty(), "{case_id}: {failed:?}");
}
macro_rules! cases {($($name:ident => $id:literal),* $(,)?)=>{$(#[test]fn $name(){run($id);})*};}
cases! {
 source_persistent_ps01=>"PS01",source_persistent_ps02=>"PS02",source_persistent_ps03=>"PS03",
 source_persistent_ps04=>"PS04",source_persistent_ps05=>"PS05",source_persistent_ps06=>"PS06",
 source_persistent_ps07=>"PS07",source_persistent_ps08=>"PS08",source_persistent_ps09=>"PS09",
 source_persistent_ps10=>"PS10",source_persistent_ps11=>"PS11",source_persistent_ps12=>"PS12",
}

#[test]
fn source_persistent_session_moves_threads_and_clones_independently() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<DatasetSession>();
    let mut parent = DatasetSession::new_source_internal(true);
    parent
        .load_rdflib(
            b"<urn:s> <urn:p> \"z\" .\n<urn:s> <urn:p> \"a\" .",
            RdfFormat::NTriples,
            None,
        )
        .unwrap();
    let q = "SELECT ?o WHERE {<urn:s> <urn:p> ?o}";
    let before = parent
        .query_source_internal(q, &BTreeMap::new(), &BTreeMap::new(), true, None)
        .unwrap()
        .result;
    let mut branch = parent.clone();
    let changed = std::thread::spawn(move || {
        branch.clear_rdflib(None).unwrap();
        branch
            .load_rdflib(b"<urn:s> <urn:p> \"new\" .", RdfFormat::NTriples, None)
            .unwrap();
        branch
            .query_source_internal(q, &BTreeMap::new(), &BTreeMap::new(), false, None)
            .unwrap()
            .result
    })
    .join()
    .unwrap();
    assert_ne!(before, changed);
    assert_eq!(
        parent
            .query_source_internal(q, &BTreeMap::new(), &BTreeMap::new(), false, None)
            .unwrap()
            .result,
        before
    );
}
#[test]
fn source_persistent_cancel_and_state_binding_preserve_previous_session() {
    let mut s = DatasetSession::new_source_internal(true);
    s.load_rdflib(b"<urn:s> <urn:p> \"z\" .", RdfFormat::NTriples, None)
        .unwrap();
    let q = "SELECT ?o WHERE {<urn:s> <urn:p> ?o}";
    let expected = s
        .query_source_internal(q, &BTreeMap::new(), &BTreeMap::new(), true, None)
        .unwrap()
        .result;
    let revision = s.revision;
    let cached = s.query_cache.len();
    let token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    assert!(
        s.query_source_internal(q, &BTreeMap::new(), &BTreeMap::new(), true, Some(token))
            .is_err()
    );
    assert_eq!(s.revision, revision);
    assert_eq!(s.query_cache.len(), cached);
    assert_eq!(
        s.query_source_internal(q, &BTreeMap::new(), &BTreeMap::new(), false, None)
            .unwrap()
            .result,
        expected
    );
    let valid = s.rdflib_projection.source_state.clone();
    s.rdflib_projection.source_state = Some(Default::default());
    assert!(
        s.query_source_internal(q, &BTreeMap::new(), &BTreeMap::new(), true, None)
            .unwrap_err()
            .to_string()
            .contains("source_persistent_projection_mismatch")
    );
    s.rdflib_projection.source_state = valid;
    assert!(
        s.query_source_internal(q, &BTreeMap::new(), &BTreeMap::new(), true, None)
            .unwrap()
            .cache_hit
    );
}
