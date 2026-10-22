//! All 884 frozen UPDATE wrapper actions. Expected data is consulted only after
//! execution of every input action. No expected result controls the live state.
use super::source_failure::SourceFailure;
use super::source_update_loader::{SourceUpdateBytesLoader, SourceUpdateResource};
use super::*;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use spareval::{QueryEvaluationError, SourceNodeObservation, SourceTermInput, SourceUpdateLoader};
use std::collections::{BTreeMap, BTreeSet};

#[path = "source_update_comparison.rs"]
mod comparison;
use comparison::{GlobalComparison, sort_observation};

#[path = "source_update_trace.rs"]
mod loader_trace;
use loader_trace::LoaderTrace;

const ORIGINAL_SHA: &str = "7bb3c73aebd8725aaaafa1bebfcf54fb1d1c0f1ed3ed27d0bcf0f9673d258895";
const PLAN01_SHA: &str = "9f9e246af0ad6b525049066dba4fa2b8c2a8d2a3b7317c321904bd19930a31b5";
const PLAN02_SHA: &str = "c77a3857ac7c51e52ea1f6b64b8b19c35832d76941a3c6cb6673d3ab28c8cdfe";

struct Resources {
    turtle: &'static str,
    expected_sha: String,
    present_iri: String,
    absent_iri: String,
    plans: [Value; 2],
}
fn file_iri(path: &str) -> Result<String> {
    ensure!(
        path.starts_with('/'),
        "original_resource_requires_absolute_posix_path"
    );
    let mut out = String::from("file://");
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(&mut out, "%{byte:02X}")?;
        }
    }
    Ok(out)
}
impl Resources {
    fn read() -> Result<Self> {
        // The shared private reader independently enforces catalog identity,
        // regular files, root containment and bounded input size. These paths
        // are relative IDs; actual private source IRIs come only from the plan.
        let plan01 = crate::private_test_fixtures::text_ref!(
            "source-update-resources-08/SOURCE-UPDATE-OBSERVATION-PLAN-01.json"
        );
        let plan02 = crate::private_test_fixtures::text_ref!(
            "source-update-resources-08/SOURCE-UPDATE-OBSERVATION-PLAN-02.json"
        );
        let turtle = crate::private_test_fixtures::text_ref!(
            "source-update-resources-08/source-update-local-authorized-01.ttl"
        );
        ensure!(
            crate::sha256(plan01) == PLAN01_SHA && crate::sha256(plan02) == PLAN02_SHA,
            "original_update_plan_identity"
        );
        let plans: [Value; 2] = [serde_json::from_str(plan01)?, serde_json::from_str(plan02)?];
        let resource = &plans[0]["local_resource"];
        let expected_sha = resource["sha256"]
            .as_str()
            .context("resource_digest_missing")?
            .to_owned();
        ensure!(
            resource["bytes"].as_u64() == Some(turtle.len() as u64),
            "resource_size_mismatch"
        );
        ensure!(
            crate::sha256(turtle) == expected_sha,
            "resource_identity_mismatch"
        );
        let present_iri = file_iri(resource["path"].as_str().context("resource_path_missing")?)?;
        let absent_iri = file_iri(
            plans[0]["absent_local_resource"]
                .as_str()
                .context("absence_authority_missing")?,
        )?;
        ensure!(present_iri != absent_iri, "resource_authority_collision");
        ensure!(
            plans[0]["candidate_results_used"] == false
                && plans[1]["candidate_results_used"] == false,
            "original_plan_provenance"
        );
        Ok(Self {
            turtle,
            expected_sha,
            present_iri,
            absent_iri,
            plans,
        })
    }
    fn loader(&self, trace: LoaderTrace) -> Result<RecordingLoader<'_>> {
        // A new production loader is deliberately created for EVERY UPDATE.
        // Its resource/emission accounting never accidentally crosses tasks.
        let resources = BTreeMap::from([
            (
                self.present_iri.clone(),
                SourceUpdateResource::Turtle {
                    bytes: self.turtle.as_bytes(),
                    sha256: &self.expected_sha,
                },
            ),
            (
                self.absent_iri.clone(),
                SourceUpdateResource::ObservedAbsent,
            ),
        ]);
        Ok(RecordingLoader {
            inner: SourceUpdateBytesLoader::new(resources, 1024 * 1024, 100_000)?,
            trace,
        })
    }
}

struct RecordingLoader<'a> {
    inner: SourceUpdateBytesLoader<'a>,
    trace: LoaderTrace,
}
fn input_term(value: SourceTermInput<'_>) -> Value {
    // This is a loader input trace, not a runtime DTO or a supplied answer.
    match value {
        SourceTermInput::Iri(v) => json!({"kind":"iri","value":v}),
        SourceTermInput::Blank(v) => json!({"kind":"blank","value":v}),
        SourceTermInput::Variable(v) => json!({"kind":"variable","value":v}),
        SourceTermInput::Literal {
            value,
            datatype,
            language,
        } => json!({"kind":"literal","value":value,"datatype":datatype,"language":language}),
    }
}
impl SourceUpdateLoader for RecordingLoader<'_> {
    fn load(
        &mut self,
        iri: &str,
        emit: &mut dyn FnMut(
            [SourceTermInput<'_>; 3],
        ) -> std::result::Result<(), QueryEvaluationError>,
    ) -> std::result::Result<bool, QueryEvaluationError> {
        let call = self.trace.begin(iri);
        let result = self.inner.load(iri, &mut |triple| {
            let terms = json!(triple.map(input_term));
            call.emission(terms, || emit(triple))
        });
        call.finish(&result);
        result
    }
}

use super::source_observation::state;

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .with_context(|| format!("input_string_required:{key}"))
}
fn optional_text<'a>(value: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match value.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(v)) => Ok(Some(v)),
        _ => anyhow::bail!("input_optional_string_required:{key}"),
    }
}
fn boolean(value: &Value, key: &str) -> Result<bool> {
    value[key]
        .as_bool()
        .with_context(|| format!("input_boolean_required:{key}"))
}
fn execute(
    session: &mut DatasetSession,
    input: &Value,
    resources: &Resources,
    loader_trace: &LoaderTrace,
) -> Result<Value> {
    Ok(match text(input, "operation")? {
        "new" => {
            ensure!(
                text(input, "profile")? == "rdf",
                "original_update_profile_not_received"
            );
            let cache = boolean(input, "cache_queries")?;
            *session = DatasetSession::new_source_update_internal(cache);
            json!({"profile":"rdf","cache_queries":cache})
        }
        "load" => {
            ensure!(
                text(input, "format")? == "nquads",
                "original_update_load_format_not_received"
            );
            serde_json::to_value(session.load_rdflib_with_base(
                text(input, "data")?.as_bytes(),
                RdfFormat::NQuads,
                optional_text(input, "base_uri")?,
                optional_text(input, "graph_name")?,
            )?)?
        }
        "select" => {
            let result = session.helper_source_internal(
                "select",
                text(input, "query")?,
                &serde_json::from_value(input["namespaces"].clone())?,
                &serde_json::from_value(input["bindings"].clone())?,
                boolean(input, "use_cache")?,
            )?;
            let mut value = result.result;
            value["revision"] = json!(result.revision);
            value["cache_hit"] = json!(result.cache_hit);
            value
        }
        "update" => {
            let namespaces = serde_json::from_value(input["namespaces"].clone())?;
            let bindings = serde_json::from_value(input["bindings"].clone())?;
            let query = input.get("query").context("update_query_field_missing")?; // includes actual JSON null.
            let mut loader = resources.loader(loader_trace.clone())?;
            let result =
                session.update_source_full_internal(query, &namespaces, &bindings, &mut loader);
            let result = result?;
            json!({"added":result.added,"removed":result.removed,"quad_count":result.quad_count,"revision":result.revision})
        }
        "snapshot" => serde_json::to_value(session.snapshot_rdflib(text(input, "created_at")?)?)?,
        "clear" => {
            serde_json::to_value(session.clear_rdflib(optional_text(input, "graph_name")?)?)?
        }
        op => anyhow::bail!("original_update_operation_not_received:{op}"),
    })
}
fn panic_text(error: &(dyn std::any::Any + Send)) -> String {
    error
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| error.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_else(|| "non_string_panic_payload".into())
}
fn observe_action(session: &mut DatasetSession, input: &Value, resources: &Resources) -> Value {
    observe_action_with(session, input, |session, trace| execute(session, input, resources, trace))
}
fn observe_action_with(
    session: &mut DatasetSession,
    input: &Value,
    action: impl FnOnce(&mut DatasetSession, &LoaderTrace) -> Result<Value>,
) -> Value {
    // The trace outlives execute and its loader, including unwinding.
    let loader_trace = LoaderTrace::default();
    let mut actual = json!({"id":input["id"],"operation":input["operation"]});
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        action(session, &loader_trace)
    }));
    match outcome {
        Ok(Ok(raw)) => {
            actual["status"] = json!("completed");
            actual["raw"] = raw;
        }
        Ok(Err(error)) => match error.downcast_ref::<SourceFailure>() {
            Some(error) => {
                actual["status"] = json!("original_error");
                actual["error"] =
                    json!({"module":error.module,"type":error.class,"message":error.to_string()});
            }
            None => {
                actual["status"] = json!("unreceived_Rust_error");
                actual["diagnostic"] = json!(format!("{error:#}"));
            }
        },
        Err(error) => {
            actual["status"] = json!("unreceived_Rust_panic");
            actual["diagnostic"] = json!(panic_text(error.as_ref()));
        }
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state(session))) {
        Ok(Ok(state)) => actual["state"] = state,
        Ok(Err(error)) => {
            actual["state"] = Value::Null;
            actual["state_error"] = json!(format!("{error:#}"));
        }
        Err(error) => {
            actual["state"] = Value::Null;
            actual["state_panic"] = json!(panic_text(error.as_ref()));
        }
    }
    json!({"actual":actual,"loader_calls":loader_trace.snapshot()})
}

#[test]
fn source_update_wrapper_receiving_all_884_original_actions() {
    let bytes = crate::private_test_fixtures::text_ref!(
        "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-update-original.json"
    );
    assert_eq!(crate::sha256(bytes), ORIGINAL_SHA);
    let original: Value = serde_json::from_str(bytes).unwrap();
    let resources = Resources::read().unwrap();
    let sources = original["source_sets"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    let mut inputs = vec![];
    let mut expected = vec![];
    let mut ids = BTreeSet::new();
    let mut counts = BTreeMap::<String, usize>::new();
    for (index, source) in sources.iter().enumerate() {
        let actions = source["input"].as_array().unwrap();
        let observed = source["actual"].as_array().unwrap();
        assert_eq!(actions.len(), observed.len());
        assert_eq!(
            source["plan"]["sha256"],
            if index == 0 { PLAN01_SHA } else { PLAN02_SHA }
        );
        assert_eq!(source["input"], resources.plans[index]["actions"]);
        for (input, answer) in actions.iter().zip(observed) {
            assert_eq!(input["id"], answer["id"]);
            assert_eq!(input["operation"], answer["operation"]);
            assert!(ids.insert(text(input, "id").unwrap().to_owned()));
            *counts
                .entry(text(input, "operation").unwrap().to_owned())
                .or_default() += 1;
            inputs.push(input);
            expected.push(answer);
        }
    }
    assert_eq!(inputs.len(), 884);
    assert_eq!(
        counts,
        BTreeMap::from([
            ("new".into(), 104),
            ("load".into(), 104),
            ("select".into(), 182),
            ("snapshot".into(), 286),
            ("update".into(), 104),
            ("clear".into(), 104)
        ])
    );
    assert_eq!(
        expected
            .iter()
            .filter(|v| v["status"] == "original_error")
            .count(),
        17
    );
    // Execution receives only the original inputs and explicit resource owner.
    // Every action runs before any expected record is offered to comparison.
    let mut session = DatasetSession::new_source_update_internal(true);
    let mut records = Vec::with_capacity(inputs.len());
    for input in &inputs {
        let record = observe_action(&mut session, input, &resources);
        println!("OE_SOURCE_UPDATE_ACTUAL {}", record);
        records.push(record);
    }
    let mut comparator = GlobalComparison::default(); // NEVER recreated at new or source-set boundaries.
    let mut mismatches = vec![];
    let mut observed_errors = vec![];
    for ((input, answer), record) in inputs.iter().zip(&expected).zip(&records) {
        let actual = &record["actual"];
        if actual["status"] == "original_error" {
            observed_errors.push(actual["id"].clone());
        }
        let comparison = comparator.compare(answer, actual);
        if comparison["matched"] != true {
            mismatches.push(input["id"].clone());
        }
        println!(
            "OE_SOURCE_UPDATE_COMPARISON {}",
            json!({"id":input["id"],"comparison":comparison,"expected":answer,"actual":actual})
        );
    }
    println!(
        "OE_SOURCE_UPDATE_SUMMARY {}",
        json!({"schema":"private.source-update-wrapper-receiving/1","original_sha256":ORIGINAL_SHA,"records":records.len(),"operation_counts":counts,"expected_original_errors":17,"observed_original_errors":observed_errors,"mismatch_ids":mismatches,"comparison":comparator.summary(),"exposure":"development_replay","method_benefit":"unknown","engineering_adoption":false,"formal_version_switched":false})
    );
    assert_eq!(records.len(), 884);
    assert!(
        mismatches.is_empty(),
        "complete original wrapper mismatches: {mismatches:?}"
    );
}

mod additional_originals;

mod source_storage;
