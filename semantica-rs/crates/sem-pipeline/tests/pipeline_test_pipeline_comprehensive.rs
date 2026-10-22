//! Port of `tests/pipeline/test_pipeline_comprehensive.py` (`TestPipelineComprehensive`).
//!
//! The original's `setUp` patches `semantica.utils.progress_tracker.get_progress_tracker`
//! and `semantica.utils.logging.get_logger`; the pipeline modules had already imported those
//! functions, so the patches never reached them and the real tracker and loggers ran. The
//! port uses them too.
#![allow(non_snake_case)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{FakeTripletStore, FakeVersionManager, side_effect_handler, ts_value, value_error, vm_value};
use sem_pipeline::{
    ExecutionEngine, FailureHandler, ParallelismManager, PipelineBuilder, PipelineValidator,
    RetryPolicy, RetryStrategy, StepHandler, Task, TaskHandler, kwargs,
};
use sem_utils::SemanticaError;
use serde_json::{Value, json};

// --- Failure Handler Tests ---

#[test]
fn test_pipeline_comprehensive__test_failure_handler_retry_policy() {
    let handler = FailureHandler::default();
    let policy = RetryPolicy {
        max_retries: 3,
        strategy: RetryStrategy::Linear,
        backoff_factor: 1.0,
        initial_delay: 0.1,
        ..RetryPolicy::default()
    };

    // Test retry decision
    let recovery = handler.handle_failure(&value_error("Test"), &policy, Some(0));
    assert!(recovery.should_retry);
    assert_eq!(recovery.retry_delay, 0.1);

    // Test max retries
    let recovery = handler.handle_failure(&value_error("Test"), &policy, Some(3));
    assert!(!recovery.should_retry);
}

#[test]
fn test_pipeline_comprehensive__test_failure_handler_exponential_backoff() {
    let handler = FailureHandler::default();
    let policy = RetryPolicy {
        max_retries: 3,
        strategy: RetryStrategy::Exponential,
        backoff_factor: 2.0,
        initial_delay: 1.0,
        ..RetryPolicy::default()
    };

    // First retry: delay = 1.0 * (2^0) = 1.0
    let recovery = handler.handle_failure(&value_error("Test"), &policy, Some(0));
    assert_eq!(recovery.retry_delay, 1.0);

    // Second retry: delay = 1.0 * (2^1) = 2.0
    let recovery = handler.handle_failure(&value_error("Test"), &policy, Some(1));
    assert_eq!(recovery.retry_delay, 2.0);

    // Third retry: delay = 1.0 * (2^2) = 4.0
    let recovery = handler.handle_failure(&value_error("Test"), &policy, Some(2));
    assert_eq!(recovery.retry_delay, 4.0);
}

// --- Parallelism Manager Tests ---

fn task_handler() -> TaskHandler {
    TaskHandler::new("task_handler", |args, _| {
        Ok(json!(args[0].as_i64().unwrap() * 2))
    })
}

#[test]
fn test_pipeline_comprehensive__test_parallelism_manager_execution() {
    let manager = ParallelismManager::new(None, kwargs! {"max_workers" => 2});

    let tasks = vec![
        Task {
            args: vec![json!(1)],
            ..Task::new("t1", task_handler())
        },
        Task {
            args: vec![json!(2)],
            ..Task::new("t2", task_handler())
        },
        Task {
            args: vec![json!(3)],
            ..Task::new("t3", task_handler())
        },
    ];

    let mut results = manager.execute_parallel(&tasks, None, &kwargs! {}).unwrap();

    assert_eq!(results.len(), 3);

    // Sort results by task_id to ensure order
    results.sort_by(|a, b| a.task_id.cmp(&b.task_id));

    assert_eq!(results[0].result, json!(2));
    assert_eq!(results[1].result, json!(4));
    assert_eq!(results[2].result, json!(6));
}

#[test]
fn test_pipeline_comprehensive__test_parallelism_identify_steps() {
    // A -> B, A -> C, B -> D, C -> D: B and C can run in parallel
    let mut builder = PipelineBuilder::default();
    builder.add_step("A", "dummy", kwargs! {});
    builder.add_step("B", "dummy", kwargs! {"dependencies" => ["A"]});
    builder.add_step("C", "dummy", kwargs! {"dependencies" => ["A"]});
    builder.add_step("D", "dummy", kwargs! {"dependencies" => ["B", "C"]});

    let pipeline = builder.build(Some("parallel_pipeline")).unwrap();

    let manager = ParallelismManager::default();
    let groups = manager.identify_parallelizable_steps(&pipeline).unwrap();

    // Flatten names for checking
    let group_names: Vec<Vec<&str>> = groups
        .iter()
        .map(|group| group.iter().map(|s| s.name.as_str()).collect())
        .collect();

    assert!(
        group_names
            .iter()
            .any(|g| g.contains(&"B") && g.contains(&"C"))
    );
}

// --- Pipeline Validator Tests ---

#[test]
fn test_pipeline_comprehensive__test_pipeline_validator_cycles() {
    let mut builder = PipelineBuilder::default();
    builder.add_step("A", "dummy", kwargs! {});
    builder.add_step("B", "dummy", kwargs! {});

    // A -> B -> A
    builder.connect_steps("A", "B", &kwargs! {}).unwrap();

    let attempt = (|| -> Result<(), SemanticaError> {
        builder.connect_steps("B", "A", &kwargs! {})?;
        // If this doesn't raise, then we check validator
        let pipeline = builder.build(Some("cycle_pipeline"))?;
        let validator = PipelineValidator::default();
        let result = validator.validate(&pipeline, None);
        assert!(!result.valid);
        assert!(format!("{:?}", result.errors).contains("Cycle detected"));
        Ok(())
    })();
    // If builder raises, that's also good. (It does: build() fails validation.)
    if let Err(e) = attempt {
        assert_eq!(e.type_name(), "ValidationError");
    }
}

#[test]
fn test_pipeline_comprehensive__test_pipeline_validator_missing_deps() {
    let mut builder = PipelineBuilder::default();
    builder.add_step("A", "dummy", kwargs! {});
    let step_b = builder.add_step("B", "dummy", kwargs! {});

    // Manually add a non-existent dependency
    step_b.dependencies.push("NON_EXISTENT".to_string());

    // Validate the builder directly (build() raises due to missing dep)
    let validator = PipelineValidator::default();
    let result = validator.validate(&builder, None);

    assert!(!result.valid);
    assert!(result.errors.iter().any(|e| e.contains("Missing dependency")));
}

// --- Execution Engine Advanced Tests ---

#[test]
fn test_pipeline_comprehensive__test_execution_engine_data_flow() {
    let step1 = StepHandler::new("step1", |_, _| Ok(json!({"val": 10})));
    let step2 = StepHandler::new("step2", |data, _| {
        let val = data.get("val").and_then(Value::as_i64).unwrap_or(0);
        Ok(json!({"val": val + 5}))
    });
    let step3 = StepHandler::new("step3", |data, _| {
        let val = data.get("val").and_then(Value::as_i64).unwrap_or(0);
        Ok(json!({"result": val * 2}))
    });

    let mut builder = PipelineBuilder::default();
    builder.add_step("s1", "op", kwargs! {"handler" => step1});
    builder.add_step("s2", "op", kwargs! {"handler" => step2, "dependencies" => ["s1"]});
    builder.add_step("s3", "op", kwargs! {"handler" => step3, "dependencies" => ["s2"]});

    let mut pipeline = builder.build(Some("data_flow")).unwrap();
    let engine = ExecutionEngine::default();

    let result = engine.execute_pipeline(&mut pipeline, json!({}), &kwargs! {});

    assert!(result.success);
    assert_eq!(result.output.get("result"), Some(&json!(30))); // (10 + 5) * 2 = 30
}

#[test]
fn test_pipeline_comprehensive__test_execution_engine_retry_integration() {
    // Handler that fails twice then succeeds
    let calls = Arc::new(AtomicUsize::new(0));
    let mock_handler = side_effect_handler(
        "mock_handler",
        &[value_error("Fail 1"), value_error("Fail 2")],
        json!("Success"),
        &calls,
    );

    let mut builder = PipelineBuilder::default();
    builder.add_step("flaky", "flaky_type", kwargs! {"handler" => mock_handler});
    let mut pipeline = builder.build(Some("retry_pipeline")).unwrap();

    let engine = ExecutionEngine::default();
    // Configure retry policy for 'flaky_type'
    engine.failure_handler.set_retry_policy(
        "flaky_type",
        RetryPolicy {
            max_retries: 3,
            strategy: RetryStrategy::Fixed,
            initial_delay: 0.01,
            ..RetryPolicy::default()
        },
    );

    let result = engine.execute_pipeline(&mut pipeline, json!({}), &kwargs! {});

    assert!(result.success);
    assert_eq!(result.output, json!("Success"));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn test_pipeline_comprehensive__test_execution_engine_delta_mode() {
    let mock_version_manager = FakeVersionManager::new(json!({
        "v1": {"version_id": "v1", "graph_uri": "urn:graph:v1"},
        "v2": {"version_id": "v2", "graph_uri": "urn:graph:v2"},
    }));

    let expected_delta_payload = json!({
        "old_graph_uri": "urn:graph:v1",
        "new_graph_uri": "urn:graph:v2",
        "added_triples": ["<urn:s> <urn:p> <urn:o>"],
        "removed_triples": [],
        "added_count": 1,
        "removed_count": 0,
    });
    let mock_triplet_store = FakeTripletStore::new(Some(expected_delta_payload.clone()));
    let delta_aware_handler = StepHandler::new("delta_aware_handler", |data, _| Ok(data));

    let mut builder = PipelineBuilder::default();
    builder.add_step(
        "incremental_validation",
        "validation",
        kwargs! {
            "handler" => delta_aware_handler,
            "delta_mode" => true,
            "base_version_id" => "v1",
            "target_version_id" => "v2",
        },
    );

    let mut pipeline = builder.build(Some("delta_pipeline")).unwrap();
    let engine = ExecutionEngine::default();

    let initial_data = json!({"full_graph": "huge_amount_of_data"});

    let result = engine.execute_pipeline(
        &mut pipeline,
        initial_data.clone(),
        &kwargs! {
            "version_manager" => vm_value(&mock_version_manager),
            "triplet_store" => ts_value(&mock_triplet_store),
        },
    );

    assert!(result.success);

    // get_version.assert_any_call("v1") / ("v2")
    let vm_calls = mock_version_manager.calls.lock().unwrap().clone();
    assert!(vm_calls.contains(&"v1".to_string()));
    assert!(vm_calls.contains(&"v2".to_string()));

    // compute_delta.assert_called_once_with("urn:graph:v1", "urn:graph:v2",
    //     version_manager=mock_version_manager, triplet_store=mock_triplet_store)
    let ts_calls = mock_triplet_store.calls.lock().unwrap().clone();
    assert_eq!(ts_calls.len(), 1);
    assert_eq!(ts_calls[0][0], json!("urn:graph:v1"));
    assert_eq!(ts_calls[0][1], json!("urn:graph:v2"));
    let options = mock_triplet_store.options.lock().unwrap()[0].clone();
    assert_eq!(options.len(), 2);
    assert_eq!(options["version_manager"], vm_value(&mock_version_manager));
    assert_eq!(options["triplet_store"], ts_value(&mock_triplet_store));

    assert_eq!(result.output, expected_delta_payload);
    assert_ne!(result.output, initial_data);
}
