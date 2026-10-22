use semantica_contracts::{
    SemanticDatasetResourceSelection, SemanticExplorerPathDetailsExecution,
    SemanticExplorerPathDetailsJob, SemanticRuntimeFailure, SemanticServiceBudgetCharge,
};
use semantica_runtime::{dataset::DatasetSession, sha256};
use semantica_service::{McpService, NativeServiceBudget, load_context_graph};
use serde_json::json;
use std::time::{Duration, Instant};
fn session(profile: &str) -> DatasetSession {
    let mut s = DatasetSession::with_resources(&SemanticDatasetResourceSelection {
        schema: "semantica.dataset-resource-selection/1".into(),
        profile: profile.into(),
    })
    .unwrap();
    load_context_graph(&mut s,&json!({"nodes":[{"id":"a","type":"Part"},{"id":"z","type":"Part"}],"edges":[{"id":"az","source":"a","target":"z","weight":2.0}]})).unwrap();
    s
}
fn raw(target: &str) -> Vec<u8> {
    let text=serde_json::to_string_pretty(&json!({"schema":"semantica.explorer-path-query/1","source":"a","target":target,"algorithm":"dijkstra","directed":true})).unwrap();
    format!(" \n{text}\n ").into_bytes()
}
// Real core on a reconstructed snapshot, but a contract executor control, not a subprocess.
fn response(job: &SemanticExplorerPathDetailsJob) -> SemanticExplorerPathDetailsExecution {
    let mut child = DatasetSession::with_resources(&job.resources).unwrap();
    child
        .load(
            job.dataset_nquads.as_bytes(),
            semantica_service::rdf_format("nquads").unwrap(),
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        child.snapshot().unwrap().dataset_sha256,
        job.expected_dataset_sha256
    );
    let computed = semantica_runtime::explorer_path_details::execute(
        &child,
        &job.query,
        job.work_limit,
        Instant::now() + Duration::from_secs(5),
    );
    let (outcome, result, failure) = match computed {
        Ok(v) => ("completed", Some(v), None),
        Err(e) => (
            "rejected",
            None,
            Some(SemanticRuntimeFailure {
                code: e.to_string(),
                phase: "path".into(),
                message: "fixture query rejected".into(),
                retryable: false,
            }),
        ),
    };
    SemanticExplorerPathDetailsExecution {
        schema: "semantica.explorer-path-details-execution/1".into(),
        input_sha256: sha256(serde_json::to_vec(job).unwrap()),
        dataset_sha256: job.expected_dataset_sha256.clone(),
        outcome: outcome.into(),
        result,
        failure,
    }
}
#[test]
fn path_details_transport_keeps_resources_raw_budget_and_parent_identity() {
    for profile in ["semantica.dataset-default/1", "semantica.context-50k/1"] {
        let mut s = session(profile);
        let before = s.snapshot().unwrap();
        let revision = s.revision();
        let raw = raw("z");
        let budget = NativeServiceBudget::new(1, raw.len() as u64, 10_000);
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = McpService::new(&mut s, &budget)
            .execute_explorer_path_details(&raw, 1000, deadline, |job, seen| {
                assert_eq!(seen, deadline);
                assert_eq!(job.resources.profile, profile);
                assert_eq!(job.work_limit, 1000);
                Ok(response(job))
            })
            .unwrap();
        assert_eq!(result.result.unwrap().result.path, vec!["a", "z"]);
        let charged = budget
            .charge_claim(&SemanticServiceBudgetCharge {
                kind: "framing".into(),
                bytes: 0,
            })
            .unwrap();
        assert_eq!(charged.input_bytes, raw.len() as u64);
        assert_eq!(charged.calls, 1);
        assert_eq!(charged.output_bytes, 0);
        assert_eq!(s.snapshot().unwrap(), before);
        assert_eq!(s.revision(), revision);
    }
}
#[test]
fn path_details_transport_refuses_forged_results_and_preserves_typed_failures() {
    for bad in [
        "input", "dataset", "shape", "endpoint", "work", "metric", "query",
    ] {
        let mut s = session("semantica.dataset-default/1");
        let before = s.snapshot().unwrap();
        let budget = NativeServiceBudget::bounded();
        let error = McpService::new(&mut s, &budget)
            .execute_explorer_path_details(
                &raw("z"),
                1000,
                Instant::now() + Duration::from_secs(5),
                |job, _| {
                    let mut r = response(job);
                    match bad {
                        "input" => r.input_sha256 = "0".repeat(64),
                        "dataset" => r.dataset_sha256 = "0".repeat(64),
                        "shape" => r.outcome = "rejected".into(),
                        "work" => r.result.as_mut().unwrap().work_limit = 1001,
                        "metric" => {
                            r.result.as_mut().unwrap().semantic_similarity.value = Some(0.5)
                        }
                        "query" => r.result.as_mut().unwrap().query.source = "forged".into(),
                        _ => r.result.as_mut().unwrap().result.path[1] = "other".into(),
                    };
                    Ok(r)
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "explorer_worker_result_invalid");
        assert_eq!(s.snapshot().unwrap(), before);
    }
    let mut s = session("semantica.dataset-default/1");
    let before = s.snapshot().unwrap();
    let budget = NativeServiceBudget::bounded();
    let error = McpService::new(&mut s, &budget)
        .execute_explorer_path_details(
            &raw("missing"),
            1000,
            Instant::now() + Duration::from_secs(5),
            |job, _| Ok(response(job)),
        )
        .unwrap_err();
    assert_eq!(error.code, "explorer_path_target_missing");
    assert_eq!(s.snapshot().unwrap(), before);
}
#[test]
fn path_details_transport_refuses_admission_budget_and_late_results() {
    let mut s = session("semantica.dataset-default/1");
    let before = s.snapshot().unwrap();
    let raw = raw("z");
    let budget = NativeServiceBudget::new(1, raw.len() as u64 - 1, 10_000);
    assert_eq!(
        McpService::new(&mut s, &budget)
            .execute_explorer_path_details(
                &raw,
                1000,
                Instant::now() + Duration::from_secs(1),
                |_, _| panic!("budget before dispatch")
            )
            .unwrap_err()
            .code,
        "service_budget_exceeded"
    );
    let budget = NativeServiceBudget::bounded();
    for bytes in [
        b"{\"schema\":\"a\",\"schema\":\"b\"}".to_vec(),
        b"{}".to_vec(),
    ] {
        assert!(
            McpService::new(&mut s, &budget)
                .execute_explorer_path_details(
                    &bytes,
                    1000,
                    Instant::now() + Duration::from_secs(1),
                    |_, _| panic!("admission before dispatch")
                )
                .is_err()
        );
    }
    assert_eq!(
        McpService::new(&mut s, &budget)
            .execute_explorer_path_details(
                &raw,
                1000,
                Instant::now() - Duration::from_secs(1),
                |_, _| panic!("expired before dispatch")
            )
            .unwrap_err()
            .code,
        "explorer_query_deadline_exceeded"
    );
    let deadline = Instant::now() + Duration::from_millis(30);
    assert_eq!(
        McpService::new(&mut s, &budget)
            .execute_explorer_path_details(&raw, 1000, deadline, |job, seen| {
                let r = response(job);
                std::thread::sleep(
                    seen.saturating_duration_since(Instant::now()) + Duration::from_millis(3),
                );
                Ok(r)
            })
            .unwrap_err()
            .code,
        "explorer_query_deadline_exceeded"
    );
    assert_eq!(s.snapshot().unwrap(), before);
}

#[test]
fn path_details_transport_keeps_work_allowance_out_of_query() {
    let mut s = session("semantica.dataset-default/1");
    let budget = NativeServiceBudget::bounded();
    let mut input: serde_json::Value = serde_json::from_slice(&raw("z")).unwrap();
    input["work_limit"] = json!(10_000_000);
    assert!(
        McpService::new(&mut s, &budget)
            .execute_explorer_path_details(
                &serde_json::to_vec(&input).unwrap(),
                1000,
                Instant::now() + Duration::from_secs(1),
                |_, _| panic!("client cannot choose work allowance")
            )
            .is_err()
    );
    for work in [0, 10_000_001] {
        let error = McpService::new(&mut s, &budget)
            .execute_explorer_path_details(
                &raw("z"),
                work,
                Instant::now() + Duration::from_secs(1),
                |_, _| panic!("invalid host allowance"),
            )
            .unwrap_err();
        assert_eq!(error.code, "explorer_path_details_budget");
    }
}
