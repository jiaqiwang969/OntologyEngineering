//! Parallel execution (`semantica/pipeline/parallelism_manager.py`): running tasks on a
//! worker pool, executing pipeline steps in parallel, and grouping steps by dependency level.
//!
//! The pool mirrors `concurrent.futures.ThreadPoolExecutor`: tasks are submitted in priority
//! order to a FIFO queue served by at most `max_workers` threads, and results are collected
//! in completion order (`as_completed`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{float_repr, py_repr, str_repr};
use sem_utils::{ProcessingError, ProgressTracker, SemanticaError, get_logger};
use serde_json::{Map, Value};

use crate::enabled_progress_tracker;
use crate::kwargs::{
    KwargValue, Kwargs, StepHandler, bind_keyword, bool_repr, init_config, kwargs_repr,
    merge_call_kwargs, split_config,
};
use crate::pipeline_builder::{Pipeline, PipelineStep};

/// The function name CPython prints for `ParallelismManager(...)` in call errors.
const PARALLELISM_MANAGER_FUNC: &str = "semantica.pipeline.parallelism_manager.ParallelismManager";
/// The function name CPython prints for `ParallelExecutor(...)` in call errors.
const PARALLEL_EXECUTOR_FUNC: &str = "semantica.pipeline.parallelism_manager.ParallelExecutor";

/// The signature of a task handler: `handler(*args, **kwargs)`.
pub type TaskFn = dyn Fn(&[Value], &Kwargs) -> Result<Value, SemanticaError> + Send + Sync;

/// A task's callable (the `handler` of a [`Task`]).
#[derive(Clone)]
pub struct TaskHandler {
    name: Arc<str>,
    func: Arc<TaskFn>,
}

impl TaskHandler {
    /// A handler called `name`.
    pub fn new<F>(name: impl Into<String>, func: F) -> Self
    where
        F: Fn(&[Value], &Kwargs) -> Result<Value, SemanticaError> + Send + Sync + 'static,
    {
        TaskHandler {
            name: Arc::from(name.into()),
            func: Arc::new(func),
        }
    }

    /// The function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `handler(*args, **kwargs)`.
    pub fn call(&self, args: &[Value], kwargs: &Kwargs) -> Result<Value, SemanticaError> {
        (self.func)(args, kwargs)
    }
}

impl From<StepHandler> for TaskHandler {
    /// A step handler called as `handler(data, **kwargs)`: `args` must be exactly `(data,)`.
    fn from(handler: StepHandler) -> Self {
        let name = handler.name().to_string();
        let func_name = name.clone();
        TaskHandler::new(name, move |args: &[Value], kwargs: &Kwargs| match args {
            [data] => handler.call(data.clone(), kwargs),
            [] => Err(SemanticaError::Type(format!(
                "{func_name}() missing 1 required positional argument: 'data'"
            ))),
            more => Err(SemanticaError::Type(format!(
                "{func_name}() takes 1 positional argument but {} were given",
                more.len()
            ))),
        })
    }
}

impl fmt::Debug for TaskHandler {
    /// `<function name at 0x...>`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<function {} at {:#x}>",
            self.name,
            Arc::as_ptr(&self.func) as *const () as usize
        )
    }
}

/// Parallel task definition (a dataclass in the original).
#[derive(Clone)]
pub struct Task {
    /// Task identifier.
    pub task_id: String,
    /// The callable.
    pub handler: TaskHandler,
    /// Positional arguments.
    pub args: Vec<Value>,
    /// Keyword arguments.
    pub kwargs: Kwargs,
    /// Priority: higher runs first.
    pub priority: i64,
}

impl fmt::Debug for Task {
    /// The dataclass `repr` (`args` is a tuple).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let args: Vec<String> = self.args.iter().map(py_repr).collect();
        let args = match args.len() {
            1 => format!("({},)", args[0]),
            _ => format!("({})", args.join(", ")),
        };
        write!(
            f,
            "Task(task_id={}, handler={:?}, args={args}, kwargs={}, priority={})",
            str_repr(&self.task_id),
            self.handler,
            kwargs_repr(&self.kwargs),
            self.priority
        )
    }
}

impl Task {
    /// `Task(task_id, handler)` with the dataclass defaults (no arguments, priority 0).
    pub fn new(task_id: impl Into<String>, handler: TaskHandler) -> Self {
        Task {
            task_id: task_id.into(),
            handler,
            args: Vec::new(),
            kwargs: Kwargs::new(),
            priority: 0,
        }
    }
}

/// Parallel execution result (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct ParallelExecutionResult {
    /// The task's identifier.
    pub task_id: String,
    /// Whether the handler returned normally.
    pub success: bool,
    /// The handler's return value (`Value::Null` on failure).
    pub result: Value,
    /// The handler's error.
    pub error: Option<SemanticaError>,
    /// Seconds spent retrieving the finished result (as measured by the original, which
    /// starts its clock after the task has completed).
    pub execution_time: f64,
}

impl fmt::Debug for ParallelExecutionResult {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error = self
            .error
            .as_ref()
            .map_or_else(|| "None".to_string(), |e| format!("{e:?}"));
        write!(
            f,
            "ParallelExecutionResult(task_id={}, success={}, result={}, error={error}, \
             execution_time={})",
            str_repr(&self.task_id),
            bool_repr(self.success),
            py_repr(&self.result),
            float_repr(self.execution_time)
        )
    }
}

impl ParallelExecutionResult {
    /// `ParallelExecutionResult(task_id, success)` with the dataclass defaults.
    pub fn new(task_id: impl Into<String>, success: bool) -> Self {
        ParallelExecutionResult {
            task_id: task_id.into(),
            success,
            result: Value::Null,
            error: None,
            execution_time: 0.0,
        }
    }
}

/// `os.cpu_count()`.
fn cpu_count() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// The worker count `ThreadPoolExecutor(max_workers)` / `ProcessPoolExecutor(max_workers)`
/// ends up with, or its `ValueError`.
fn resolve_max_workers(value: &KwargValue, processes: bool) -> Result<usize, SemanticaError> {
    let workers = match value {
        KwargValue::None => {
            return Ok(if processes {
                cpu_count()
            } else {
                (cpu_count() + 4).min(32)
            });
        }
        other => match other.as_f64() {
            Some(n) => n,
            None => {
                return Err(SemanticaError::Type(format!(
                    "'<=' not supported between instances of '{}' and 'int'",
                    other.type_name()
                )));
            }
        },
    };
    if workers <= 0.0 {
        return Err(SemanticaError::Value(
            "max_workers must be greater than 0".to_string(),
        ));
    }
    Ok(workers.ceil() as usize)
}

/// Run one task, turning a panic into a `RuntimeError`.
fn run_task(task: &Task) -> Result<Value, SemanticaError> {
    match catch_unwind(AssertUnwindSafe(|| {
        task.handler.call(&task.args, &task.kwargs)
    })) {
        Ok(outcome) => outcome,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "task panicked".to_string());
            Err(SemanticaError::Runtime(message))
        }
    }
}

/// Parallelism management system.
///
/// • Parallel task execution and coordination
/// • Load balancing over a bounded worker pool
/// • Dependency-level grouping of pipeline steps
pub struct ParallelismManager {
    /// Configuration (`config` merged with the keyword arguments).
    pub config: Kwargs,
    /// The shared progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    /// `max_workers` (default 4; `None` lets the pool choose, as `ThreadPoolExecutor` does).
    pub max_workers: KwargValue,
    /// `use_processes` (default `false`); see `PORT-NOTES.md`.
    pub use_processes: bool,
    #[allow(dead_code)]
    logger: Logger,
}

impl fmt::Debug for ParallelismManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParallelismManager")
            .field("config", &self.config)
            .field("max_workers", &self.max_workers)
            .field("use_processes", &self.use_processes)
            .finish_non_exhaustive()
    }
}

impl Default for ParallelismManager {
    fn default() -> Self {
        ParallelismManager::new(None, Kwargs::new())
    }
}

impl ParallelismManager {
    /// `ParallelismManager(config=None, **kwargs)`; reads `max_workers` and `use_processes`.
    pub fn new(config: Option<Kwargs>, kwargs: Kwargs) -> Self {
        let config = init_config(config, kwargs);
        let max_workers = config
            .get("max_workers")
            .cloned()
            .unwrap_or(KwargValue::Int(4));
        let use_processes = config.get("use_processes").is_some_and(KwargValue::truthy);
        ParallelismManager {
            config,
            progress_tracker: enabled_progress_tracker(),
            max_workers,
            use_processes,
            logger: get_logger("parallelism_manager"),
        }
    }

    /// `execute_parallel(tasks, pipeline_id=None, **options)`: run the tasks (highest
    /// priority submitted first, ties in list order) on the worker pool; results come in
    /// completion order. `options["max_workers"]` overrides the pool size.
    pub fn execute_parallel(
        &self,
        tasks: &[Task],
        pipeline_id: Option<&str>,
        options: &Kwargs,
    ) -> Result<Vec<ParallelExecutionResult>, SemanticaError> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("ParallelismManager"),
            &format!("Executing {} tasks in parallel", tasks.len()),
            pipeline_id,
        );

        if tasks.is_empty() {
            tracker.stop_tracking(&tracking_id, Some("completed"), "No tasks to execute", None);
            return Ok(Vec::new());
        }

        tracker.update_tracking(&tracking_id, None, "Sorting tasks by priority...");
        let mut sorted_tasks: Vec<Task> = tasks.to_vec();
        // sorted(..., reverse=True) is stable: equal priorities keep their order.
        sorted_tasks.sort_by(|a, b| b.priority.cmp(&a.priority));

        tracker.update_tracking(
            &tracking_id,
            None,
            &format!(
                "Executing tasks using {}...",
                if self.use_processes {
                    "processes"
                } else {
                    "threads"
                }
            ),
        );
        let results = match self.execute_with_pool(sorted_tasks, options) {
            Ok(results) => results,
            Err(e) => {
                tracker.stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
                return Err(e);
            }
        };

        let success_count = results.iter().filter(|r| r.success).count();
        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Executed {} tasks: {success_count} succeeded, {} failed",
                tasks.len(),
                tasks.len() - success_count
            ),
            None,
        );
        Ok(results)
    }

    /// `_execute_with_threads` / `_execute_with_processes`: both run on threads here.
    fn execute_with_pool(
        &self,
        tasks: Vec<Task>,
        options: &Kwargs,
    ) -> Result<Vec<ParallelExecutionResult>, SemanticaError> {
        let max_workers = options.get("max_workers").unwrap_or(&self.max_workers);
        let max_workers = resolve_max_workers(max_workers, self.use_processes)?;
        let worker_count = max_workers.min(tasks.len()).max(1);
        let queue: Mutex<VecDeque<Task>> = Mutex::new(tasks.into_iter().collect());
        let (sender, receiver) = mpsc::channel::<(String, Result<Value, SemanticaError>)>();

        let mut results = Vec::new();
        std::thread::scope(|scope| {
            for _ in 0..worker_count {
                let sender = sender.clone();
                let queue = &queue;
                scope.spawn(move || {
                    loop {
                        let next = queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
                        let Some(task) = next else { break };
                        let outcome = run_task(&task);
                        if sender.send((task.task_id, outcome)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(sender);

            // as_completed: one result per finished task, in completion order.
            for (task_id, outcome) in receiver {
                let start_time = Instant::now();
                let result = match outcome {
                    Ok(value) => ParallelExecutionResult {
                        result: value,
                        execution_time: start_time.elapsed().as_secs_f64(),
                        ..ParallelExecutionResult::new(task_id, true)
                    },
                    Err(error) => ParallelExecutionResult {
                        error: Some(error),
                        execution_time: start_time.elapsed().as_secs_f64(),
                        ..ParallelExecutionResult::new(task_id, false)
                    },
                };
                results.push(result);
            }
        });
        Ok(results)
    }

    /// `execute_pipeline_steps_parallel(steps, data, **options)`: run every step's handler
    /// (or the identity) on `data` with the step configuration as keyword arguments, in
    /// parallel; the results come back in step order.
    ///
    /// Raises `ProcessingError: Step <name> failed: <error>` for the first failed step.
    /// `options["pipeline_id"]` is the progress-tracking pipeline id.
    pub fn execute_pipeline_steps_parallel(
        &self,
        steps: &[PipelineStep],
        data: Value,
        options: &Kwargs,
    ) -> Result<Vec<Value>, SemanticaError> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("ParallelismManager"),
            &format!("Executing {} pipeline steps in parallel", steps.len()),
            None,
        );
        let result: Result<Vec<Value>, SemanticaError> = (|| {
            tracker.update_tracking(&tracking_id, None, "Creating tasks from steps...");
            let tasks: Vec<Task> = steps
                .iter()
                .map(|step| Task {
                    task_id: step.name.clone(),
                    handler: TaskHandler::from(
                        step.handler.clone().unwrap_or_else(StepHandler::identity),
                    ),
                    args: vec![data.clone()],
                    kwargs: step.config.clone(),
                    priority: 0,
                })
                .collect();

            tracker.update_tracking(&tracking_id, None, "Executing tasks in parallel...");
            let mut options = options.clone();
            let pipeline_id = match options.shift_remove("pipeline_id") {
                Some(KwargValue::None) | None => None,
                Some(other) => Some(other.py_str()),
            };
            let results = self.execute_parallel(&tasks, pipeline_id.as_deref(), &options)?;

            tracker.update_tracking(&tracking_id, None, "Mapping results to steps...");
            // {r.task_id: r for r in results}: the last result for an id wins.
            let mut result_map: HashMap<&str, &ParallelExecutionResult> = HashMap::new();
            for r in &results {
                result_map.insert(r.task_id.as_str(), r);
            }
            let mut step_results = Vec::new();
            for step in steps {
                match result_map.get(step.name.as_str()) {
                    Some(r) if r.success => step_results.push(r.result.clone()),
                    other => {
                        let reason = match other {
                            Some(r) => r
                                .error
                                .as_ref()
                                .map_or_else(|| "None".to_string(), ToString::to_string),
                            None => "Unknown error".to_string(),
                        };
                        return Err(ProcessingError::new(format!(
                            "Step {} failed: {reason}",
                            step.name
                        ))
                        .into());
                    }
                }
            }
            tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Executed {} steps in parallel", steps.len()),
                None,
            );
            Ok(step_results)
        })();
        if let Err(e) = &result {
            tracker.stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `identify_parallelizable_steps(pipeline)`: the steps grouped by dependency level
    /// (level 0 = no dependencies, else one more than the deepest dependency), groups in
    /// level order, steps in pipeline order.
    ///
    /// Raises `KeyError` for a dependency on an unknown step and `RecursionError` for a
    /// dependency cycle, as the original's recursion does.
    pub fn identify_parallelizable_steps<'a>(
        &self,
        pipeline: &'a Pipeline,
    ) -> Result<Vec<Vec<&'a PipelineStep>>, SemanticaError> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("ParallelismManager"),
            &format!(
                "Identifying parallelizable steps for pipeline: {}",
                pipeline.name
            ),
            None,
        );
        let result: Result<Vec<Vec<&'a PipelineStep>>, SemanticaError> = (|| {
            tracker.update_tracking(&tracking_id, None, "Analyzing step dependencies...");
            let step_map: HashMap<&str, &PipelineStep> = pipeline
                .steps
                .iter()
                .map(|s| (s.name.as_str(), s))
                .collect();
            let mut levels: HashMap<String, usize> = HashMap::new();
            let mut in_progress: HashSet<String> = HashSet::new();
            for step in &pipeline.steps {
                get_level(&step.name, &step_map, &mut levels, &mut in_progress)?;
            }

            tracker.update_tracking(&tracking_id, None, "Grouping steps by dependency level...");
            let mut level_groups: IndexMap<usize, Vec<&'a PipelineStep>> = IndexMap::new();
            for step in &pipeline.steps {
                let level = levels[step.name.as_str()];
                level_groups.entry(level).or_default().push(step);
            }
            level_groups.sort_keys();
            let groups: Vec<Vec<&'a PipelineStep>> = level_groups.into_values().collect();
            tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Identified {} parallelizable step groups", groups.len()),
                None,
            );
            Ok(groups)
        })();
        if let Err(e) = &result {
            tracker.stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `optimize_parallel_execution(pipeline, available_workers)`: an execution plan with one
    /// entry per dependency level (`steps`, `parallel`, `worker_count`), plus `total_groups`,
    /// `max_parallelism` and `estimated_workers`.
    pub fn optimize_parallel_execution(
        &self,
        pipeline: &Pipeline,
        available_workers: i64,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("ParallelismManager"),
            &format!(
                "Optimizing parallel execution for pipeline: {}",
                pipeline.name
            ),
            None,
        );
        let result: Result<Map<String, Value>, SemanticaError> = (|| {
            tracker.update_tracking(&tracking_id, None, "Identifying parallelizable steps...");
            let parallel_groups = self.identify_parallelizable_steps(pipeline)?;

            tracker.update_tracking(&tracking_id, None, "Calculating execution plan...");
            let mut execution_plan = Vec::new();
            let mut estimated_workers: i64 = 0;
            for group in &parallel_groups {
                let worker_count = (group.len() as i64).min(available_workers);
                estimated_workers += worker_count;
                let mut plan = Map::new();
                plan.insert(
                    "steps".into(),
                    Value::from(group.iter().map(|s| s.name.clone()).collect::<Vec<_>>()),
                );
                plan.insert("parallel".into(), Value::from(group.len() > 1));
                plan.insert("worker_count".into(), Value::from(worker_count));
                execution_plan.push(Value::Object(plan));
            }
            let max_parallelism = parallel_groups.iter().map(Vec::len).max().unwrap_or(1);

            let mut result = Map::new();
            result.insert("execution_plan".into(), Value::Array(execution_plan));
            result.insert("total_groups".into(), Value::from(parallel_groups.len()));
            result.insert("max_parallelism".into(), Value::from(max_parallelism));
            result.insert("estimated_workers".into(), Value::from(estimated_workers));
            tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!(
                    "Optimization complete: {} groups, max parallelism: {max_parallelism}",
                    parallel_groups.len()
                ),
                None,
            );
            Ok(result)
        })();
        if let Err(e) = &result {
            tracker.stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }
}

/// `get_level(step_name)` of `identify_parallelizable_steps`.
fn get_level(
    step_name: &str,
    step_map: &HashMap<&str, &PipelineStep>,
    levels: &mut HashMap<String, usize>,
    in_progress: &mut HashSet<String>,
) -> Result<usize, SemanticaError> {
    if let Some(level) = levels.get(step_name) {
        return Ok(*level);
    }
    let Some(step) = step_map.get(step_name) else {
        return Err(SemanticaError::Key(step_name.to_string()));
    };
    if !in_progress.insert(step_name.to_string()) {
        // The original recurses until Python's recursion limit.
        return Err(SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "RecursionError".to_string(),
            message: "maximum recursion depth exceeded".to_string(),
        });
    }
    let level = if step.dependencies.is_empty() {
        0
    } else {
        let mut deepest = 0;
        for dep in &step.dependencies {
            deepest = deepest.max(get_level(dep, step_map, levels, in_progress)?);
        }
        deepest + 1
    };
    in_progress.remove(step_name);
    levels.insert(step_name.to_string(), level);
    Ok(level)
}

/// Parallel executor for pipeline tasks.
#[derive(Debug)]
pub struct ParallelExecutor {
    /// The underlying manager.
    pub parallelism_manager: ParallelismManager,
}

impl Default for ParallelExecutor {
    fn default() -> Self {
        ParallelExecutor::new(None, Kwargs::new())
            .expect("no keyword arguments can collide with the defaults")
    }
}

impl ParallelExecutor {
    /// `ParallelExecutor(max_workers=4, **config)`. A `max_workers` entry in `config` binds
    /// to the parameter, as the keyword would in Python (both at once is Python's
    /// `TypeError`).
    pub fn new(max_workers: Option<i64>, config: Kwargs) -> Result<Self, SemanticaError> {
        let mut config = config;
        let max_workers = bind_keyword(
            PARALLEL_EXECUTOR_FUNC,
            max_workers.map(KwargValue::Int),
            &mut config,
            "max_workers",
        )?
        .unwrap_or(KwargValue::Int(4));
        let mut explicit = Kwargs::new();
        explicit.insert("max_workers".into(), max_workers);
        let merged = merge_call_kwargs(PARALLELISM_MANAGER_FUNC, &[&explicit, &config])?;
        let (config, kwargs) = split_config(&merged);
        Ok(ParallelExecutor {
            parallelism_manager: ParallelismManager::new(config, kwargs),
        })
    }

    /// `execute_parallel(tasks)`.
    pub fn execute_parallel(
        &self,
        tasks: &[Task],
    ) -> Result<Vec<ParallelExecutionResult>, SemanticaError> {
        self.parallelism_manager
            .execute_parallel(tasks, None, &Kwargs::new())
    }
}
