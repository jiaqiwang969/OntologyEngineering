use semantica_contracts::{
    SemanticExplorerSparqlExecution, SemanticExplorerSparqlJob, SemanticRuntimeFailure,
};
use semantica_runtime::{dataset::DatasetSession, sha256};
use semantica_service::{McpService, NativeServiceBudget, load_context_graph};
use serde_json::json;
use std::time::{Duration, Instant};

fn dataset() -> DatasetSession {
    let mut session = DatasetSession::new();
    load_context_graph(
        &mut session,
        &json!({"nodes":[{"id":"a","type":"Part"}],"edges":[]}),
    )
    .unwrap();
    session
}
// A contract-level executor control, not evidence of subprocess cancellation.
fn completed(job: &SemanticExplorerSparqlJob) -> SemanticExplorerSparqlExecution {
    SemanticExplorerSparqlExecution {
        schema: "semantica.explorer-sparql-execution/1".into(),
        input_sha256: sha256(serde_json::to_vec(job).unwrap()),
        dataset_sha256: job.expected_dataset_sha256.clone(),
        result: json!({"columns":["result"],"rows":[{"result":"true"}],"total":1,
                      "truncated":false,"error":null,"error_line":null,"error_column":null}),
    }
}

#[test]
fn borrowed_session_and_budget_are_retained_across_queries() {
    let mut session = dataset();
    let before = session.snapshot().unwrap();
    let budget = NativeServiceBudget::new(1, 1000, 1000);
    {
        let service = McpService::new(&mut session, &budget);
        let deadline = Instant::now() + Duration::from_secs(2);
        let result = service
            .execute_explorer_sparql("ASK {?s ?p ?o}", deadline, |job, seen| {
                assert_eq!(seen, deadline);
                assert_eq!(job.expected_dataset_sha256, before.dataset_sha256);
                assert_eq!(job.row_limit, 5000);
                let mut child = DatasetSession::new();
                child
                    .load(
                        job.dataset_nquads.as_bytes(),
                        oxigraph::io::RdfFormat::NQuads,
                        None,
                        None,
                    )
                    .unwrap();
                assert_eq!(
                    child.snapshot().unwrap().dataset_sha256,
                    before.dataset_sha256
                );
                Ok(completed(job))
            })
            .unwrap();
        assert_eq!(result.result["rows"][0]["result"], "true");
        let error = service
            .execute_explorer_sparql("ASK {?s ?p ?o}", deadline, |_, _| {
                panic!("budget exhausted before dispatch")
            })
            .unwrap_err();
        assert_eq!(error.code, "service_budget_exceeded");
    }
    assert_eq!(session.snapshot().unwrap(), before);
}

#[test]
fn mismatched_input_or_dataset_results_are_refused() {
    for field in ["input", "dataset"] {
        let mut session = dataset();
        let before = session.snapshot().unwrap();
        let budget = NativeServiceBudget::bounded();
        let error = McpService::new(&mut session, &budget)
            .execute_explorer_sparql(
                "ASK {?s ?p ?o}",
                Instant::now() + Duration::from_secs(2),
                |job, _| {
                    let mut result = completed(job);
                    if field == "input" {
                        result.input_sha256 = "0".repeat(64)
                    } else {
                        result.dataset_sha256 = "0".repeat(64)
                    }
                    Ok(result)
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "explorer_worker_result_invalid");
        assert_eq!(session.snapshot().unwrap(), before);
    }
}

#[test]
fn admission_and_expired_deadline_do_not_dispatch() {
    let mut session = dataset();
    let budget = NativeServiceBudget::bounded();
    let service = McpService::new(&mut session, &budget);
    for (query, deadline, code) in [
        (
            "INSERT DATA {<urn:a> <urn:b> <urn:c>}",
            Instant::now() + Duration::from_secs(2),
            "explorer_worker_read_only_required",
        ),
        (
            "ASK {?s ?p ?o}",
            Instant::now() - Duration::from_secs(1),
            "explorer_query_deadline_exceeded",
        ),
    ] {
        assert_eq!(
            service
                .execute_explorer_sparql(query, deadline, |_, _| panic!("must not dispatch"))
                .unwrap_err()
                .code,
            code
        );
    }
}

#[test]
fn late_success_and_child_failure_do_not_publish_or_change_session() {
    let mut session = dataset();
    let before = session.snapshot().unwrap();
    let budget = NativeServiceBudget::bounded();
    {
        let service = McpService::new(&mut session, &budget);
        let deadline = Instant::now() + Duration::from_millis(80);
        let error = service
            .execute_explorer_sparql("ASK {?s ?p ?o}", deadline, |job, seen| {
                std::thread::sleep(
                    seen.saturating_duration_since(Instant::now()) + Duration::from_millis(5),
                );
                Ok(completed(job))
            })
            .unwrap_err();
        assert_eq!(error.code, "explorer_query_deadline_exceeded");
        let error = service
            .execute_explorer_sparql(
                "ASK {?s ?p ?o}",
                Instant::now() + Duration::from_secs(2),
                |_, _| {
                    Err(SemanticRuntimeFailure {
                        code: "explorer_query_timed_out".into(),
                        phase: "process".into(),
                        message: "test control".into(),
                        retryable: false,
                    })
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "explorer_query_timed_out");
    }
    assert_eq!(session.snapshot().unwrap(), before);
}

#[test]
fn resource_envelope_binds_profile_and_retains_parent_session() {
    for profile in ["semantica.dataset-default/1", "semantica.context-50k/1"] {
        let selection = semantica_contracts::SemanticDatasetResourceSelection {
            schema: "semantica.dataset-resource-selection/1".into(),
            profile: profile.into(),
        };
        let mut session = DatasetSession::with_resources(&selection).unwrap();
        load_context_graph(
            &mut session,
            &json!({"nodes":[{"id":"a","type":"Part"}],"edges":[]}),
        )
        .unwrap();
        let before = session.snapshot().unwrap();
        let budget = NativeServiceBudget::bounded();
        let deadline = Instant::now() + Duration::from_secs(2);
        {
            let service = McpService::new(&mut session, &budget);
            service
                .execute_explorer_sparql_v2("ASK {?s ?p ?o}", deadline, |envelope, seen| {
                    assert_eq!(seen, deadline);
                    assert_eq!(envelope.schema, "semantica.explorer-sparql-job/2");
                    assert_eq!(envelope.resources.profile, profile);
                    let mut child = DatasetSession::with_resources(&envelope.resources).unwrap();
                    child
                        .load(
                            envelope.job.dataset_nquads.as_bytes(),
                            oxigraph::io::RdfFormat::NQuads,
                            None,
                            None,
                        )
                        .unwrap();
                    assert_eq!(
                        child.snapshot().unwrap().dataset_sha256,
                        before.dataset_sha256
                    );
                    let mut response = completed(&envelope.job);
                    response.input_sha256 = sha256(serde_json::to_vec(envelope).unwrap());
                    Ok(response)
                })
                .unwrap();
            let wrong = service
                .execute_explorer_sparql_v2("ASK {?s ?p ?o}", deadline, |envelope, _| {
                    Ok(completed(&envelope.job))
                })
                .unwrap_err();
            assert_eq!(wrong.code, "explorer_worker_result_invalid");
            if profile == "semantica.context-50k/1" {
                assert_eq!(
                    service
                        .execute_explorer_sparql("ASK {?s ?p ?o}", deadline, |_, _| panic!(
                            "profile must not disappear"
                        ))
                        .unwrap_err()
                        .code,
                    "explorer_worker_resource_profile_mismatch"
                );
            }
        }
        assert_eq!(session.snapshot().unwrap(), before);
        assert_eq!(session.resource_profile(), profile);
    }
}
