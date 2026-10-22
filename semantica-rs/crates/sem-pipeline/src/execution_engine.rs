//! Pipeline execution (`semantica/pipeline/execution_engine.py`): runs a pipeline's steps in
//! dependency order with retries, pause/resume/stop, resource allocation, incremental
//! (delta) steps and progress reporting.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::format_fixed;
use sem_utils::pycompat::time::PyDateTime;
use sem_utils::{
    ProcessingError, ProgressTracker as UtilsProgressTracker, SemanticaError, ValidationError,
    get_logger,
};
use serde_json::{Map, Value};

use crate::failure_handler::FailureHandler;
use crate::kwargs::{
    KwargValue, Kwargs, PyObject, bool_repr, handling, init_config, json_map_repr,
    merge_call_kwargs, split_config,
};
use crate::parallelism_manager::ParallelismManager;
use crate::pipeline_builder::{Pipeline, PipelineStep, StepStatus};
use crate::resource_scheduler::ResourceScheduler;
use crate::{enabled_progress_tracker, sleep_secs, time_now};

py_enum! {
    /// Pipeline execution status.
    pub enum PipelineStatus {
        /// `pending`.
        Pending = ("PENDING", "pending"),
        /// `running`.
        Running = ("RUNNING", "running"),
        /// `paused`.
        Paused = ("PAUSED", "paused"),
        /// `completed`.
        Completed = ("COMPLETED", "completed"),
        /// `failed`.
        Failed = ("FAILED", "failed"),
        /// `stopped`.
        Stopped = ("STOPPED", "stopped"),
    }
}

/// Pipeline execution result (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct ExecutionResult {
    /// Whether no step ended in the failed state.
    pub success: bool,
    /// The last step's output (`Value::Null` for `None`).
    pub output: Value,
    /// `pipeline_id` and `execution_time` (empty when execution raised).
    pub metadata: Map<String, Value>,
    /// `execution_time`, `steps_executed`, `steps_failed` (empty when execution raised).
    pub metrics: Map<String, Value>,
    /// The error message when execution raised.
    pub errors: Vec<String>,
}

impl fmt::Debug for ExecutionResult {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ExecutionResult(success={}, output={}, metadata={}, metrics={}, errors={})",
            bool_repr(self.success),
            sem_utils::pycompat::fmt::py_repr(&self.output),
            json_map_repr(&self.metadata),
            json_map_repr(&self.metrics),
            sem_utils::pycompat::fmt::str_list_repr(&self.errors)
        )
    }
}

impl ExecutionResult {
    /// `ExecutionResult(success, output)` with the dataclass defaults.
    pub fn new(success: bool, output: Value) -> Self {
        ExecutionResult {
            success,
            output,
            metadata: Map::new(),
            metrics: Map::new(),
            errors: Vec::new(),
        }
    }
}

/// What a delta-mode step needs from the `version_manager` option: `get_version(id)`, the
/// stored snapshot (with its `graph_uri`) or `None`.
pub trait VersionManager: Send + Sync {
    /// The snapshot stored under `version_id`.
    fn get_version(&self, version_id: &str) -> Result<Option<Map<String, Value>>, SemanticaError>;
}

/// What a delta-mode step needs from the `triplet_store` option: `compute_delta(old_uri,
/// new_uri, **options)`, whose result becomes the step's input.
pub trait DeltaTripletStore: Send + Sync {
    /// The changes between two named graphs.
    fn compute_delta(
        &self,
        old_graph_uri: &str,
        new_graph_uri: &str,
        options: &Kwargs,
    ) -> Result<Value, SemanticaError>;
}

impl From<Arc<dyn VersionManager>> for KwargValue {
    /// Pass a version manager as the `version_manager` option.
    fn from(value: Arc<dyn VersionManager>) -> Self {
        KwargValue::Object(PyObject::with_type_name("VersionManager", value))
    }
}

impl From<Arc<dyn DeltaTripletStore>> for KwargValue {
    /// Pass a triplet store as the `triplet_store` option.
    fn from(value: Arc<dyn DeltaTripletStore>) -> Self {
        KwargValue::Object(PyObject::with_type_name("TripletStore", value))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// `options.get(key) or self.config.get(key)`.
fn option_or_config<'a>(
    options: &'a Kwargs,
    config: &'a Kwargs,
    key: &str,
) -> Option<&'a KwargValue> {
    options
        .get(key)
        .filter(|v| v.truthy())
        .or_else(|| config.get(key).filter(|v| v.truthy()))
}

/// `obj.<method>` on a value that is not the expected collaborator.
fn no_method(value: &KwargValue, method: &str) -> SemanticaError {
    SemanticaError::Attribute(format!(
        "'{}' object has no attribute '{method}'",
        value.type_name()
    ))
}

/// A URI as passed on: strings as they are, other values as their `str()`.
fn uri_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => sem_utils::pycompat::fmt::py_str(other),
    }
}

/// Pipeline execution engine.
///
/// • Pipeline execution and orchestration
/// • Task scheduling (dependency order) and management (pause, resume, stop)
/// • Resource allocation and monitoring
/// • Error handling and recovery (retries)
pub struct ExecutionEngine {
    /// Configuration (`config` merged with the keyword arguments); also passed to the
    /// failure handler, parallelism manager and resource scheduler, and the fallback for the
    /// `version_manager`/`triplet_store` options.
    pub config: Kwargs,
    /// Retry policies and failure handling.
    pub failure_handler: FailureHandler,
    /// Parallel execution helper.
    pub parallelism_manager: ParallelismManager,
    /// Resource allocation for each run.
    pub resource_scheduler: ResourceScheduler,
    /// The shared progress tracker.
    pub progress_tracker: Arc<UtilsProgressTracker>,
    running_pipelines: Mutex<IndexMap<String, Arc<Mutex<Pipeline>>>>,
    pipeline_status: Mutex<IndexMap<String, PipelineStatus>>,
    logger: Logger,
}

impl fmt::Debug for ExecutionEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = lock(&self.pipeline_status).clone();
        f.debug_struct("ExecutionEngine")
            .field("config", &self.config)
            .field("pipeline_status", &status)
            .finish_non_exhaustive()
    }
}

impl Default for ExecutionEngine {
    fn default() -> Self {
        ExecutionEngine::new(None, Kwargs::new())
    }
}

impl ExecutionEngine {
    /// `ExecutionEngine(config=None, **kwargs)`.
    pub fn new(config: Option<Kwargs>, kwargs: Kwargs) -> Self {
        let config = init_config(config, kwargs);
        let (sub_config, sub_kwargs) = split_config(&config);
        ExecutionEngine {
            failure_handler: FailureHandler::new(sub_config.clone(), sub_kwargs.clone()),
            parallelism_manager: ParallelismManager::new(sub_config.clone(), sub_kwargs.clone()),
            resource_scheduler: ResourceScheduler::new(sub_config, sub_kwargs),
            config,
            progress_tracker: enabled_progress_tracker(),
            running_pipelines: Mutex::new(IndexMap::new()),
            pipeline_status: Mutex::new(IndexMap::new()),
            logger: get_logger("execution_engine"),
        }
    }

    /// `execute_pipeline(pipeline, data=None, **options)`: run the steps in dependency order,
    /// each handler receiving the previous step's output, under resources allocated from
    /// `options` (`cpu_cores`, `memory_gb`, `gpu_device`).
    ///
    /// A failing step is retried according to its step type's retry policy; when it still
    /// fails (or anything else raises) the result has `success == false`, no output, empty
    /// metadata/metrics and the error message in `errors`. Step status, result and error are
    /// recorded on `pipeline`.
    pub fn execute_pipeline(
        &self,
        pipeline: &mut Pipeline,
        data: Value,
        options: &Kwargs,
    ) -> ExecutionResult {
        let pipeline_id = pipeline.name.clone();
        let start_time = time_now();
        let tracker = &self.progress_tracker;
        let pipeline_tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("ExecutionEngine"),
            &format!("Executing pipeline: {pipeline_id}"),
            Some(&pipeline_id),
        );

        match self.run_pipeline(pipeline, data, options, start_time, &pipeline_tracking_id) {
            Ok(result) => result,
            Err(e) => {
                tracker.stop_tracking(&pipeline_tracking_id, Some("failed"), &e.to_string(), None);
                tracker.clear_pipeline_context(&pipeline_id);
                self.logger.error(format!("Pipeline execution failed: {e}"));
                lock(&self.pipeline_status).insert(pipeline_id, PipelineStatus::Failed);
                ExecutionResult {
                    errors: vec![e.to_string()],
                    ..ExecutionResult::new(false, Value::Null)
                }
            }
        }
    }

    fn run_pipeline(
        &self,
        pipeline: &mut Pipeline,
        data: Value,
        options: &Kwargs,
        start_time: f64,
        pipeline_tracking_id: &str,
    ) -> Result<ExecutionResult, SemanticaError> {
        let pipeline_id = pipeline.name.clone();
        let tracker = &self.progress_tracker;
        self.logger
            .info(format!("Executing pipeline: {pipeline_id}"));

        // Register the pipeline's modules (step names) for progress tracking.
        let mut module_list: Vec<String> = Vec::new();
        let mut module_order: IndexMap<String, i64> = IndexMap::new();
        for (idx, step) in pipeline.steps.iter().enumerate() {
            let module_name = if step.name.is_empty() {
                format!("{step:?}")
            } else {
                step.name.clone()
            };
            if !module_name.is_empty() && !module_list.contains(&module_name) {
                module_order.insert(module_name.clone(), idx as i64);
                module_list.push(module_name);
            }
        }
        if module_list.is_empty() {
            module_list = [
                "ingest",
                "parse",
                "normalize",
                "semantic_extract",
                "kg",
                "embeddings",
            ]
            .iter()
            .map(|m| m.to_string())
            .collect();
            module_order = module_list
                .iter()
                .enumerate()
                .map(|(idx, m)| (m.clone(), idx as i64))
                .collect();
        }
        tracker.register_pipeline_modules(&pipeline_id, &module_list, Some(&module_order));

        // The engine keeps the running pipeline (here: a live copy) for `get_progress`.
        let mirror = Arc::new(Mutex::new(pipeline.clone()));
        {
            let mut status = lock(&self.pipeline_status);
            status.insert(pipeline_id.clone(), PipelineStatus::Running);
            lock(&self.running_pipelines).insert(pipeline_id.clone(), Arc::clone(&mirror));
        }

        let resources = self
            .resource_scheduler
            .allocate_resources(pipeline, options)?;

        let outcome = (|| {
            let result = self.execute_steps(pipeline, &mirror, data, options)?;

            let execution_time = time_now() - start_time;
            let steps_executed = pipeline
                .steps
                .iter()
                .filter(|s| s.status == StepStatus::Completed)
                .count();
            let steps_failed = pipeline
                .steps
                .iter()
                .filter(|s| s.status == StepStatus::Failed)
                .count();
            let mut metrics = Map::new();
            metrics.insert("execution_time".into(), Value::from(execution_time));
            metrics.insert("steps_executed".into(), Value::from(steps_executed));
            metrics.insert("steps_failed".into(), Value::from(steps_failed));

            lock(&self.pipeline_status).insert(
                pipeline_id.clone(),
                if steps_failed == 0 {
                    PipelineStatus::Completed
                } else {
                    PipelineStatus::Failed
                },
            );

            tracker.stop_tracking(
                pipeline_tracking_id,
                Some(if steps_failed == 0 {
                    "completed"
                } else {
                    "failed"
                }),
                &format!(
                    "Executed {steps_executed} steps in {}s",
                    format_fixed(execution_time, 2)
                ),
                None,
            );
            tracker.clear_pipeline_context(&pipeline_id);

            let mut metadata = Map::new();
            metadata.insert("pipeline_id".into(), Value::from(pipeline_id.clone()));
            metadata.insert("execution_time".into(), Value::from(execution_time));
            Ok(ExecutionResult {
                success: steps_failed == 0,
                output: result,
                metadata,
                metrics,
                errors: Vec::new(),
            })
        })();

        // finally: release the resources.
        self.resource_scheduler.release_resources(&resources);
        outcome
    }

    /// Copy a step's run state into the live copy `get_progress` reads.
    fn sync_step(mirror: &Mutex<Pipeline>, index: usize, step: &PipelineStep) {
        if let Some(copy) = lock(mirror).steps.get_mut(index) {
            copy.status = step.status;
            copy.result = step.result.clone();
            copy.error = step.error.clone();
        }
    }

    fn status_of(&self, pipeline_id: &str) -> Option<PipelineStatus> {
        lock(&self.pipeline_status).get(pipeline_id).copied()
    }

    /// `_execute_steps(pipeline, data, **options)`.
    fn execute_steps(
        &self,
        pipeline: &mut Pipeline,
        mirror: &Mutex<Pipeline>,
        data: Value,
        options: &Kwargs,
    ) -> Result<Value, SemanticaError> {
        let order = self.topological_sort(&pipeline.steps)?;
        let mut current_data = data;
        let total_steps = order.len();
        let tracker = &self.progress_tracker;
        let pipeline_id = pipeline.name.clone();

        for (step_idx, &index) in order.iter().enumerate() {
            if self.status_of(&pipeline_id) == Some(PipelineStatus::Stopped) {
                break;
            }
            while self.status_of(&pipeline_id) == Some(PipelineStatus::Paused) {
                sleep_secs(0.1);
            }

            let step = &mut pipeline.steps[index];
            let submodule = if step.step_type.is_empty() {
                step.name.clone()
            } else {
                step.step_type.clone()
            };
            let step_tracking_id = tracker.start_tracking(
                None,
                Some("pipeline"),
                Some(&submodule),
                &format!("Step {}/{total_steps}: {}", step_idx + 1, step.name),
                None,
            );

            step.status = StepStatus::Running;
            Self::sync_step(mirror, index, step);
            match self.execute_step(step, current_data.clone(), options) {
                Ok(step_result) => {
                    step.status = StepStatus::Completed;
                    step.result = step_result.clone();
                    Self::sync_step(mirror, index, step);
                    current_data = step_result;
                    tracker.stop_tracking(
                        &step_tracking_id,
                        Some("completed"),
                        &format!("Completed step: {}", step.name),
                        None,
                    );
                }
                Err(first_error) => {
                    step.status = StepStatus::Failed;
                    step.error = Some(first_error.clone());
                    Self::sync_step(mirror, index, step);

                    // Inside `except Exception as e:` — the retry loop.
                    let _except = handling(&first_error);
                    let mut error = first_error;
                    let retry_policy = self.failure_handler.get_retry_policy(&step.step_type);
                    let max_retries = retry_policy.max_retries;
                    let mut retry_count = 0;
                    let mut success = false;

                    while retry_count < max_retries {
                        let recovery =
                            self.failure_handler
                                .handle_step_failure(step, &error, &Kwargs::new());
                        if !recovery.retry {
                            break;
                        }
                        if recovery.retry_delay > 0.0 {
                            sleep_secs(recovery.retry_delay);
                        }
                        tracker.update_tracking(
                            &step_tracking_id,
                            Some("running"),
                            &format!("Retrying step: {} (attempt {})", step.name, retry_count + 1),
                        );
                        step.status = StepStatus::Running;
                        Self::sync_step(mirror, index, step);
                        match self.execute_step(step, current_data.clone(), options) {
                            Ok(step_result) => {
                                step.status = StepStatus::Completed;
                                step.result = step_result.clone();
                                Self::sync_step(mirror, index, step);
                                current_data = step_result;
                                success = true;
                                break;
                            }
                            Err(retry_error) => {
                                step.status = StepStatus::Failed;
                                step.error = Some(retry_error.clone());
                                Self::sync_step(mirror, index, step);
                                error = retry_error;
                                retry_count += 1;
                            }
                        }
                    }

                    if success {
                        tracker.stop_tracking(
                            &step_tracking_id,
                            Some("completed"),
                            &format!("Retry successful: {}", step.name),
                            None,
                        );
                    } else {
                        tracker.stop_tracking(
                            &step_tracking_id,
                            Some("failed"),
                            &error.to_string(),
                            None,
                        );
                        return Err(error);
                    }
                }
            }
        }
        Ok(current_data)
    }

    /// `_execute_step(step, data, **options)`: in delta mode the step's input is the delta
    /// between its base and target versions; the handler (if any) is called as
    /// `handler(data, **step.config, **options)`, otherwise the input is returned.
    fn execute_step(
        &self,
        step: &PipelineStep,
        data: Value,
        options: &Kwargs,
    ) -> Result<Value, SemanticaError> {
        let mut data = data;
        if step.delta_mode {
            self.logger.info(format!(
                "Executing step '{}' in incremental delta mode.",
                step.name
            ));
            let version_manager = option_or_config(options, &self.config, "version_manager");
            let triplet_store = option_or_config(options, &self.config, "triplet_store");
            let (Some(version_manager), Some(triplet_store)) = (version_manager, triplet_store)
            else {
                return Err(ProcessingError::new(format!(
                    "Step '{}' requires 'version_manager' and 'triplet_store' in execution \
                     options for delta processing.",
                    step.name
                ))
                .into());
            };

            let base_id = step.base_version_id.as_deref().filter(|s| !s.is_empty());
            let target_id = step.target_version_id.as_deref().filter(|s| !s.is_empty());
            let (Some(base_id), Some(target_id)) = (base_id, target_id) else {
                return Err(ValidationError::new(format!(
                    "Step '{}' in delta_mode requires 'base_version_id' and \
                     'target_version_id' to be set.",
                    step.name
                ))
                .into());
            };

            let manager = version_manager
                .downcast_ref::<Arc<dyn VersionManager>>()
                .ok_or_else(|| no_method(version_manager, "get_version"))?;
            let base_snap = manager.get_version(base_id)?;
            let target_snap = manager.get_version(target_id)?;

            let Some(base_snap) = base_snap.filter(|s| !s.is_empty()) else {
                return Err(ValidationError::new(format!(
                    "Base version '{base_id}' not found in storage."
                ))
                .into());
            };
            let Some(target_snap) = target_snap.filter(|s| !s.is_empty()) else {
                return Err(ValidationError::new(format!(
                    "Target version '{target_id}' not found in storage."
                ))
                .into());
            };

            let truthy_uri = |snap: &Map<String, Value>| {
                snap.get("graph_uri")
                    .filter(|v| sem_utils::pycompat::fmt::truthy(v))
                    .map(uri_text)
            };
            let (Some(base_uri), Some(target_uri)) =
                (truthy_uri(&base_snap), truthy_uri(&target_snap))
            else {
                return Err(ValidationError::new(
                    "Both base and target snapshots must contain a 'graph_uri' to compute \
                     native store deltas.",
                )
                .into());
            };

            self.logger.debug(format!(
                "Computing delta between {base_uri} and {target_uri}"
            ));
            let store = triplet_store
                .downcast_ref::<Arc<dyn DeltaTripletStore>>()
                .ok_or_else(|| no_method(triplet_store, "compute_delta"))?;
            data = store.compute_delta(&base_uri, &target_uri, options)?;
        }

        match &step.handler {
            Some(handler) => {
                let kwargs = merge_call_kwargs(handler.name(), &[&step.config, options])?;
                handler.call(data, &kwargs)
            }
            None => Ok(data),
        }
    }

    /// `_topological_sort(steps)`: indices of the steps in dependency order (Kahn's
    /// algorithm with a FIFO queue, ties in step order).
    ///
    /// Raises `ValidationError: Circular dependency detected in pipeline` when some step can
    /// never run (a cycle, or a dependency on an unknown step).
    fn topological_sort(&self, steps: &[PipelineStep]) -> Result<Vec<usize>, SemanticaError> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("ExecutionEngine"),
            &format!("Sorting {} steps by dependencies", steps.len()),
            None,
        );

        tracker.update_tracking(&tracking_id, None, "Building dependency graph...");
        // {step.name: len(step.dependencies)}: a later step with the same name wins.
        let mut in_degree: HashMap<&str, i64> = HashMap::new();
        for step in steps {
            in_degree.insert(step.name.as_str(), step.dependencies.len() as i64);
        }

        tracker.update_tracking(&tracking_id, None, "Finding entry points...");
        let mut queue: VecDeque<usize> = steps
            .iter()
            .enumerate()
            .filter(|(_, s)| in_degree[s.name.as_str()] == 0)
            .map(|(i, _)| i)
            .collect();
        let mut sorted_steps = Vec::new();

        tracker.update_tracking(&tracking_id, None, "Performing topological sort...");
        while let Some(index) = queue.pop_front() {
            sorted_steps.push(index);
            let name = steps[index].name.as_str();
            for (other_index, other) in steps.iter().enumerate() {
                if other.dependencies.iter().any(|d| d == name) {
                    let degree = in_degree
                        .get_mut(other.name.as_str())
                        .expect("every step has an in-degree");
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(other_index);
                    }
                }
            }
        }

        tracker.update_tracking(&tracking_id, None, "Checking for circular dependencies...");
        if sorted_steps.len() != steps.len() {
            let error: SemanticaError =
                ValidationError::new("Circular dependency detected in pipeline").into();
            tracker.stop_tracking(&tracking_id, Some("failed"), &error.to_string(), None);
            return Err(error);
        }
        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!("Sorted {} steps by dependencies", steps.len()),
            None,
        );
        Ok(sorted_steps)
    }

    /// `pause_pipeline(pipeline_id)`: a running pipeline waits before its next step.
    pub fn pause_pipeline(&self, pipeline_id: &str) {
        let mut status = lock(&self.pipeline_status);
        if status.get(pipeline_id) == Some(&PipelineStatus::Running) {
            status.insert(pipeline_id.to_string(), PipelineStatus::Paused);
            self.logger.info(format!("Paused pipeline: {pipeline_id}"));
        }
    }

    /// `resume_pipeline(pipeline_id)`: a paused pipeline continues.
    pub fn resume_pipeline(&self, pipeline_id: &str) {
        let mut status = lock(&self.pipeline_status);
        if status.get(pipeline_id) == Some(&PipelineStatus::Paused) {
            status.insert(pipeline_id.to_string(), PipelineStatus::Running);
            self.logger.info(format!("Resumed pipeline: {pipeline_id}"));
        }
    }

    /// `stop_pipeline(pipeline_id)`: a known pipeline stops before its next step.
    pub fn stop_pipeline(&self, pipeline_id: &str) {
        let mut status = lock(&self.pipeline_status);
        if status.contains_key(pipeline_id) {
            status.insert(pipeline_id.to_string(), PipelineStatus::Stopped);
            self.logger.info(format!("Stopped pipeline: {pipeline_id}"));
        }
    }

    /// `get_pipeline_status(pipeline_id)`.
    pub fn get_pipeline_status(&self, pipeline_id: &str) -> Option<PipelineStatus> {
        self.status_of(pipeline_id)
    }

    /// `get_progress(pipeline_id)`: `total_steps`, `completed_steps`, `progress_percentage`
    /// and `status` of a pipeline this engine has run (empty for unknown pipelines).
    pub fn get_progress(&self, pipeline_id: &str) -> Map<String, Value> {
        let Some(pipeline) = lock(&self.running_pipelines).get(pipeline_id).cloned() else {
            return Map::new();
        };
        let (total_steps, completed_steps) = {
            let pipeline = lock(&pipeline);
            (
                pipeline.steps.len(),
                pipeline
                    .steps
                    .iter()
                    .filter(|s| s.status == StepStatus::Completed)
                    .count(),
            )
        };
        let status = self
            .status_of(pipeline_id)
            .unwrap_or(PipelineStatus::Pending);
        let mut progress = Map::new();
        progress.insert("total_steps".into(), Value::from(total_steps));
        progress.insert("completed_steps".into(), Value::from(completed_steps));
        progress.insert(
            "progress_percentage".into(),
            Value::from(if total_steps > 0 {
                completed_steps as f64 / total_steps as f64 * 100.0
            } else {
                0.0
            }),
        );
        progress.insert("status".into(), Value::from(status.value()));
        progress
    }

    /// `pipeline_status`: the status of every pipeline this engine has seen.
    pub fn pipeline_status(&self) -> IndexMap<String, PipelineStatus> {
        lock(&self.pipeline_status).clone()
    }

    /// `running_pipelines[pipeline_id]`: the engine's copy of a pipeline it has run (kept
    /// after the run, as in the original).
    pub fn running_pipeline(&self, pipeline_id: &str) -> Option<Pipeline> {
        let pipeline = lock(&self.running_pipelines).get(pipeline_id).cloned()?;
        let copy = lock(&pipeline).clone();
        Some(copy)
    }

    /// The ids in `running_pipelines`.
    pub fn running_pipeline_ids(&self) -> Vec<String> {
        lock(&self.running_pipelines).keys().cloned().collect()
    }
}

/// Progress tracking for pipeline execution (per pipeline and step).
#[derive(Debug, Clone)]
pub struct ProgressTracker {
    /// Configuration (`**config`).
    pub config: Kwargs,
    /// Per pipeline, per step: `{"progress": float, "timestamp": isoformat}`.
    pub tracking_data: IndexMap<String, IndexMap<String, Map<String, Value>>>,
    #[allow(dead_code)]
    logger: Logger,
}

impl Default for ProgressTracker {
    fn default() -> Self {
        ProgressTracker::new(Kwargs::new())
    }
}

impl ProgressTracker {
    /// `ProgressTracker(**config)`.
    pub fn new(config: Kwargs) -> Self {
        ProgressTracker {
            config,
            tracking_data: IndexMap::new(),
            logger: get_logger("progress_tracker"),
        }
    }

    /// `track_progress(pipeline_id, step_name, progress)`: record a step's progress with the
    /// current local time.
    pub fn track_progress(&mut self, pipeline_id: &str, step_name: &str, progress: f64) {
        let mut record = Map::new();
        record.insert("progress".into(), Value::from(progress));
        record.insert(
            "timestamp".into(),
            Value::from(PyDateTime::now(false).isoformat('T')),
        );
        self.tracking_data
            .entry(pipeline_id.to_string())
            .or_default()
            .insert(step_name.to_string(), record);
    }

    /// `get_completion_percentage(pipeline_id)`: the mean recorded progress (0.0 when
    /// nothing was recorded).
    pub fn get_completion_percentage(&self, pipeline_id: &str) -> f64 {
        let Some(steps) = self.tracking_data.get(pipeline_id) else {
            return 0.0;
        };
        if steps.is_empty() {
            return 0.0;
        }
        let total: f64 = steps
            .values()
            .map(|s| s.get("progress").and_then(Value::as_f64).unwrap_or(0.0))
            .sum();
        total / steps.len() as f64
    }

    /// `estimate_remaining_time(pipeline_id, start_time)`: `elapsed / completion - elapsed`
    /// (never negative), `None` before any progress. `start_time` is a `time.time()` value.
    pub fn estimate_remaining_time(&self, pipeline_id: &str, start_time: f64) -> Option<f64> {
        let completion = self.get_completion_percentage(pipeline_id);
        if completion == 0.0 {
            return None;
        }
        let elapsed = time_now() - start_time;
        let estimated_total = elapsed / completion;
        let remaining = estimated_total - elapsed;
        Some(if remaining > 0.0 { remaining } else { 0.0 })
    }
}
