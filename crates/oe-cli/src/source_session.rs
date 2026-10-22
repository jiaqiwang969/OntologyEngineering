//! Bounded same-program transport for the explicit local source-session port.
//! The worker owns one ephemeral DatasetSession; no existing service is mutated.
use anyhow::{Context, Result, ensure};
use oe_core::{
    FileIdentity, SkillRoot, process::CommandSpec, read_regular_bytes, write_new, write_new_bytes,
};
use oe_kernel::source_session::{DEADLINE_MS, MAX_OUTPUT_BYTES, MAX_WIRE_BYTES};
use semantica_contracts::{
    SemanticRuntimeFailure, SemanticSourceSessionRequest, SemanticSourceSessionRequest2,
    SemanticSourceSessionRequest3, SemanticSourceSessionRequest4, SemanticSourceSessionResult,
    SemanticSourceSessionResult2, SemanticSourceSessionResult3, SemanticSourceSessionResult4,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

enum Request {
    V1(SemanticSourceSessionRequest),
    V2(SemanticSourceSessionRequest2),
    V3(SemanticSourceSessionRequest3),
    V4(SemanticSourceSessionRequest4),
}
impl Request {
    fn worker_schema(&self) -> &'static str {
        match self {
            Self::V1(_) => "semantica.source-session-worker/1",
            Self::V2(_) => "semantica.source-session-worker/2",
            Self::V3(_) => "semantica.source-session-worker/3",
            Self::V4(_) => "semantica.source-session-worker/4",
        }
    }
    fn host_schema(&self) -> &'static str {
        match self {
            Self::V1(_) => "oev2.kernel-source-session-execution/1",
            Self::V2(_) => "oev2.kernel-source-session-execution/2",
            Self::V3(_) => "oev2.kernel-source-session-execution/3",
            Self::V4(_) => "oev2.kernel-source-session-execution/4",
        }
    }
}
fn semantic_success(value: &Value) -> bool {
    let version = value["schema"] == "semantica.source-session-worker/1"
        || (value["schema"] == "semantica.source-session-worker/2"
            && value["result"]["schema"] == "semantica.source-session-result/2"
            && value["result"]["profile"] == "semantica.rdflib760-source-session/2")
        || (value["schema"] == "semantica.source-session-worker/3"
            && value["result"]["schema"] == "semantica.source-session-result/3"
            && value["result"]["profile"] == "semantica.rdflib760-source-session/3")
        || (value["schema"] == "semantica.source-session-worker/4"
            && value["result"]["schema"] == "semantica.source-session-result/4"
            && value["result"]["profile"] == "semantica.rdflib760-source-session/4");
    version && value["failure"].is_null() && value["result"]["status"] == "completed"
}
fn input(raw: &[u8]) -> Result<Request> {
    ensure!(raw.len() <= MAX_WIRE_BYTES, "source_session_wire_budget");
    let value = oe_core::json::strict_json(raw)?;
    match value["schema"].as_str() {
        Some("semantica.source-session-request/1") => {
            Ok(Request::V1(serde_json::from_value(value)?))
        }
        Some("semantica.source-session-request/2") => {
            Ok(Request::V2(serde_json::from_value(value)?))
        }
        Some("semantica.source-session-request/3") => {
            Ok(Request::V3(serde_json::from_value(value)?))
        }
        Some("semantica.source-session-request/4") => {
            Ok(Request::V4(serde_json::from_value(value)?))
        }
        _ => anyhow::bail!("source_session_schema_unsupported"),
    }
}

pub(super) fn execute_worker() -> Result<u8> {
    let mut raw = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_WIRE_BYTES as u64 + 1)
        .read_to_end(&mut raw)?;
    let request = input(&raw)?;
    let outcome = match &request {
        Request::V1(r) => {
            oe_kernel::source_session::execute_local_source_session(r).map(|v| json!(v))
        }
        Request::V2(r) => {
            oe_kernel::source_session::execute_local_source_session2(r).map(|v| json!(v))
        }
        Request::V3(r) => {
            oe_kernel::source_session::execute_local_source_session3(r).map(|v| json!(v))
        }
        Request::V4(r) => {
            oe_kernel::source_session::execute_local_source_session4(r).map(|v| json!(v))
        }
    };
    let value = match outcome {
        Ok(result) => {
            json!({"schema":request.worker_schema(),"input_sha256":oe_core::sha256(&raw),"result":result,"failure":null})
        }
        Err(error) => {
            json!({"schema":request.worker_schema(),"input_sha256":oe_core::sha256(&raw),"result":null,"failure":error})
        }
    };
    let output = serde_json::to_vec(&value)?;
    ensure!(
        output.len() <= MAX_OUTPUT_BYTES,
        "source_session_worker_output_budget"
    );
    std::io::stdout().lock().write_all(&output)?;
    // A complete error envelope is a completed transport, never semantic pass.
    Ok(0)
}

fn received(
    request: &Request,
    raw: &[u8],
    expected_input: &str,
    deadline: Instant,
) -> Result<Value> {
    if matches!(request, Request::V3(_) | Request::V4(_)) {
        ensure!(
            Instant::now() < deadline,
            "source_session_receiving_deadline"
        );
    }
    let value = oe_core::json::strict_json(raw)?;
    let object = value
        .as_object()
        .context("source_session_worker_object_required")?;
    ensure!(
        object.len() == 4
            && object
                .keys()
                .all(|k| ["schema", "input_sha256", "result", "failure"].contains(&k.as_str())),
        "source_session_worker_fields"
    );
    ensure!(
        value["schema"] == request.worker_schema() && value["input_sha256"] == expected_input,
        "source_session_worker_input_identity"
    );
    if value["failure"].is_null() {
        match request {
            Request::V1(request) => {
                let result: SemanticSourceSessionResult =
                    serde_json::from_value(value["result"].clone())?;
                oe_kernel::source_session::validate_received(request, &result)?;
            }
            Request::V2(request) => {
                let result: SemanticSourceSessionResult2 =
                    serde_json::from_value(value["result"].clone())?;
                oe_kernel::source_session::validate_received2(request, &result)?;
            }
            Request::V3(request) => {
                let result: SemanticSourceSessionResult3 =
                    serde_json::from_value(value["result"].clone())?;
                oe_kernel::source_session::validate_received3_until(request, &result, deadline)?;
            }
            Request::V4(request) => {
                let result: SemanticSourceSessionResult4 =
                    serde_json::from_value(value["result"].clone())?;
                oe_kernel::source_session::validate_received4_until(request, &result, deadline)?;
            }
        }
    } else {
        ensure!(
            value["result"].is_null(),
            "source_session_worker_ambiguous_failure"
        );
        let error: SemanticRuntimeFailure = serde_json::from_value(value["failure"].clone())?;
        ensure!(
            error.validate().is_ok() && error.phase == "admission" && !error.retryable,
            "source_session_worker_admission_failure_invalid"
        );
    }
    if matches!(request, Request::V3(_) | Request::V4(_)) {
        ensure!(
            Instant::now() < deadline,
            "source_session_receiving_deadline"
        );
    }
    Ok(value)
}
fn same_execution(before: &Value, after: &Value) -> bool {
    [
        "normalizer",
        "cli",
        "semantic_contracts_sha256",
        "executable_sha256",
    ]
    .iter()
    .all(|k| before[*k] == after[*k])
}

pub(super) fn execute_host(root: &SkillRoot, path: PathBuf, output: Option<PathBuf>) -> Result<u8> {
    let deadline = Instant::now() + Duration::from_millis(DEADLINE_MS);
    let execution_before = super::require_execution(root, "source_session_host")?;
    let source = root.expand(&path)?;
    let raw = read_regular_bytes(&source, MAX_WIRE_BYTES)?;
    let identity = FileIdentity {
        bytes: raw.len() as u64,
        sha256: oe_core::sha256(&raw),
    };
    let request = input(&raw)?;
    let evidence = root.unique_private_dir(Path::new("var/maintenance"), "source-session")?;
    let replay = evidence.join("input.json");
    write_new_bytes(&replay, &raw)?;
    let executable = std::env::current_exe()?;
    let spec = CommandSpec {
        executable,
        arguments: vec![
            "--root".into(),
            root.path().as_os_str().into(),
            "kernel".into(),
            "source-session-worker".into(),
        ],
        cwd: root.path().into(),
        environment: BTreeMap::new(),
        remove_environment: vec![],
        timeout: Duration::from_millis(DEADLINE_MS),
        output_limit: MAX_OUTPUT_BYTES,
    };
    let attempt = oe_core::process::run_command_with_bounded_stdin_file_deadline(
        &spec,
        &evidence.join("process"),
        deadline,
        &replay,
        &identity,
        MAX_WIRE_BYTES,
    );
    // Re-read both original request and current compiled execution closure even
    // after timeout, semantic failure, or invalid output. Keep all observations.
    let source_after = read_regular_bytes(&source, MAX_WIRE_BYTES)
        .map(|v| v.len() as u64 == identity.bytes && oe_core::sha256(&v) == identity.sha256);
    let execution_after = super::require_execution(root, "source_session_host_after");
    let source_matched = matches!(&source_after, Ok(true));
    let executable_matched = execution_after
        .as_ref()
        .is_ok_and(|after| same_execution(&execution_before, after));
    let mut worker = Value::Null;
    let mut failure = Value::Null;
    let process;
    let mut transport_complete = false;
    match attempt {
        Ok(observation) => {
            process = json!({"status":observation.status,"exit_code":observation.exit_code,"report":observation.report});
            if observation.status == "completed"
                && observation.exit_code == Some(0)
                && observation.report["capture_complete"] == true
                && Instant::now() < deadline
            {
                match received(&request, &observation.stdout, &identity.sha256, deadline) {
                    Ok(value) => {
                        worker = value;
                        transport_complete = true;
                    }
                    Err(error) => {
                        failure = json!({"phase":"receive","message":format!("{error:#}")})
                    }
                }
            } else {
                failure = json!({"phase":"process","message":"source_session_process_incomplete"});
            }
        }
        Err(error) => {
            process = Value::Null;
            failure = json!({"phase":"process_setup","message":format!("{error:#}")});
        }
    }
    let deadline_matched = Instant::now() < deadline;
    let passed = transport_complete
        && source_matched
        && executable_matched
        && deadline_matched
        && semantic_success(&worker);
    let value = json!({
        "schema":request.host_schema(),
        "status":if passed {"completed"} else {"failed"},
        "transport_complete":transport_complete,"within_deadline":deadline_matched,
        "source":{"path":source,"bytes":identity.bytes,"sha256":identity.sha256},
        "source_recheck":{"matched":source_matched,"error":source_after.err().map(|e|format!("{e:#}"))},
        "execution_before":execution_before,
        "execution_after":execution_after.as_ref().ok(),
        "execution_recheck":{"matched":executable_matched,"error":execution_after.as_ref().err().map(|e|format!("{e:#}"))},
        "worker":worker,"failure":failure,"process":process,"evidence_directory":evidence,
        "engineering_adoption":"not_assessed","whole_semantica_complete":false
    });
    write_new(&evidence.join("receipt.json"), &value)?;
    super::emit(root, value, output, !passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_session4_receiving_keeps_identity_deadline_and_failures() {
        let raw = serde_json::to_vec(&json!({"schema":"semantica.source-session-request/4",
            "profile":"semantica.rdflib760-source-session/4","cache_queries":true,
            "created_at":"2026-10-17T00:00:00Z","resources":[],
            "actions":[{"id":"empty-turtle","operation":"serialize","resource_id":null,
                "query":null,"namespaces":{},"bindings":[],"base_uri":null,"graph_name":null,
                "use_cache":false,"serialize_request":{"format":"turtle","base_uri":null,"graph_name":null},
                "before_snapshot_action_id":null,"after_snapshot_action_id":null}]})).unwrap();
        let request = input(&raw).unwrap();
        let Request::V4(r) = &request else {
            panic!("version4 dispatch");
        };
        let actual = oe_kernel::source_session::execute_local_source_session4(r).unwrap();
        assert_eq!(actual.steps[0].result, json!("\n"));
        let digest = oe_core::sha256(&raw);
        let envelope = json!({"schema":request.worker_schema(),"input_sha256":digest,"result":actual,"failure":null});
        let bytes = serde_json::to_vec(&envelope).unwrap();
        received(
            &request,
            &bytes,
            &digest,
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap();
        assert!(semantic_success(&envelope));
        assert!(received(&request, &bytes, &digest, Instant::now()).is_err());
        assert!(
            received(
                &request,
                &bytes,
                &"0".repeat(64),
                Instant::now() + Duration::from_secs(60)
            )
            .is_err()
        );
        for (pointer, replacement) in [
            ("/schema", json!("semantica.source-session-worker/3")),
            ("/result/schema", json!("semantica.source-session-result/3")),
            (
                "/result/profile",
                json!("semantica.rdflib760-source-session/3"),
            ),
            ("/result/status", json!("partial")),
            ("/result/status", json!("failed")),
        ] {
            let mut v = envelope.clone();
            *v.pointer_mut(pointer).unwrap() = replacement;
            assert!(!semantic_success(&v));
            assert!(
                received(
                    &request,
                    &serde_json::to_vec(&v).unwrap(),
                    &digest,
                    Instant::now() + Duration::from_secs(60)
                )
                .is_err()
            );
        }
    }
    #[test]
    fn source_session_worker_status_is_not_semantic_success() {
        let mut v = json!({"schema":"semantica.source-session-worker/1","failure":null,"result":{"status":"failed"}});
        assert!(!semantic_success(&v));
        v["result"]["status"] = json!("partial");
        assert!(!semantic_success(&v));
        v["result"]["status"] = json!("completed");
        assert!(semantic_success(&v));
        v["failure"] = json!({"code":"source_identity"});
        assert!(!semantic_success(&v));
        v["schema"] = json!("unknown");
        assert!(!semantic_success(&v));
    }
    #[test]
    fn source_session_storage_requires_explicit_protocol2() {
        let mut value = json!({"schema":"semantica.source-session-worker/2","failure":null,
            "result":{"schema":"semantica.source-session-result/2","profile":"semantica.rdflib760-source-session/2","status":"completed"}});
        assert!(semantic_success(&value));
        value["result"]["schema"] = json!("semantica.source-session-result/1");
        assert!(!semantic_success(&value));
        value["result"]["schema"] = json!("semantica.source-session-result/2");
        value["result"]["status"] = json!("partial");
        assert!(!semantic_success(&value));
    }
    #[test]
    fn source_session_storage_requires_explicit_protocol3() {
        let mut value = json!({"schema":"semantica.source-session-worker/3","failure":null,
            "result":{"schema":"semantica.source-session-result/3","profile":"semantica.rdflib760-source-session/3","status":"completed"}});
        assert!(semantic_success(&value));
        value["result"]["schema"] = json!("semantica.source-session-result/2");
        assert!(!semantic_success(&value));
        value["result"]["schema"] = json!("semantica.source-session-result/3");
        value["result"]["status"] = json!("partial");
        assert!(!semantic_success(&value));
    }

    #[test]
    fn source_session3_receiving_keeps_host_deadline() {
        let r: SemanticSourceSessionRequest3 = serde_json::from_value(json!({
            "schema":"semantica.source-session-request/3",
            "profile":"semantica.rdflib760-source-session/3",
            "cache_queries":true,"created_at":"2026-10-14T12:00:00Z","resources":[],
            "actions":[{"id":"initial-snapshot","operation":"snapshot",
                "resource_id":null,"query":null,"namespaces":{},"bindings":[],
                "base_uri":null,"graph_name":null,"use_cache":false,
                "serialize_request":null,"before_snapshot_action_id":null,
                "after_snapshot_action_id":null}]
        }))
        .unwrap();
        let raw_input = serde_json::to_vec(&r).unwrap();
        let digest = oe_core::sha256(&raw_input);
        let actual = oe_kernel::source_session::execute_local_source_session3(&r).unwrap();
        assert_eq!(actual.status, "completed");
        let raw = serde_json::to_vec(&json!({"schema":"semantica.source-session-worker/3",
            "input_sha256":digest,"result":actual,"failure":null}))
        .unwrap();
        let request = Request::V3(r);
        received(
            &request,
            &raw,
            &digest,
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap();
        let expired = Instant::now();
        assert_eq!(
            received(&request, &raw, &digest, expired)
                .unwrap_err()
                .to_string(),
            "source_session_receiving_deadline"
        );
    }
}
