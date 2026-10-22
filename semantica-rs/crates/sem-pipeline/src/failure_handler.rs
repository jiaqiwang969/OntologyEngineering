//! Failure handling and retries (`semantica/pipeline/failure_handler.py`): retry policies
//! (linear, exponential, fixed), error severity classification, recovery actions, fallback
//! strategies.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{float_repr, str_repr};
use sem_utils::{ProgressTracker, SemanticaError, get_logger};
use serde_json::{Map, Value};

use crate::kwargs::{
    KwargValue, Kwargs, bind_keyword, bool_repr, format_exc, init_config, json_map_repr,
    merge_call_kwargs, opt_str_repr, split_config,
};
use crate::pipeline_builder::PipelineStep;
use crate::{enabled_progress_tracker, sleep_secs, time_now};

/// The function name CPython prints for `FailureHandler(...)` in call errors.
const FAILURE_HANDLER_FUNC: &str = "semantica.pipeline.failure_handler.FailureHandler";
/// The function name CPython prints for `RetryHandler(...)` in call errors.
const RETRY_HANDLER_FUNC: &str = "semantica.pipeline.failure_handler.RetryHandler";

py_enum! {
    /// Error severity levels.
    pub enum ErrorSeverity {
        /// `low`.
        Low = ("LOW", "low"),
        /// `medium`.
        Medium = ("MEDIUM", "medium"),
        /// `high`.
        High = ("HIGH", "high"),
        /// `critical`.
        Critical = ("CRITICAL", "critical"),
    }
}

py_enum! {
    /// Retry strategies.
    pub enum RetryStrategy {
        /// `linear`: `initial_delay * attempt`.
        Linear = ("LINEAR", "linear"),
        /// `exponential`: `initial_delay * backoff_factor ** (attempt - 1)`.
        Exponential = ("EXPONENTIAL", "exponential"),
        /// `fixed`: `initial_delay`.
        Fixed = ("FIXED", "fixed"),
    }
}

/// `isinstance(error, <class named class_name>)` for the exception classes the port models.
pub(crate) fn is_instance(error: &SemanticaError, class_name: &str) -> bool {
    let type_name = error.type_name();
    let other = |names: &[&str]| names.contains(&type_name.as_str());
    match class_name {
        "BaseException" | "Exception" => true,
        "SemanticaError" => error.is_semantica_error(),
        "ValidationError" => error.is_validation_error(),
        "ValueError" => {
            error.is_value_error()
                || other(&["UnicodeError", "UnicodeDecodeError", "UnicodeEncodeError"])
        }
        "LookupError" => matches!(error, SemanticaError::Key(_) | SemanticaError::Index(_)),
        "ArithmeticError" => {
            matches!(error, SemanticaError::Overflow(_))
                || other(&["ZeroDivisionError", "FloatingPointError"])
        }
        "RuntimeError" => {
            matches!(
                error,
                SemanticaError::Runtime(_) | SemanticaError::NotImplemented(_)
            ) || other(&["RecursionError"])
        }
        "ImportError" => {
            matches!(error, SemanticaError::Import(_)) || other(&["ModuleNotFoundError"])
        }
        "OSError" | "IOError" | "EnvironmentError" => matches!(error, SemanticaError::Os(_)),
        name => type_name == name,
    }
}

/// Retry policy configuration (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct RetryPolicy {
    /// Maximum number of retries.
    pub max_retries: i64,
    /// Exponential backoff base.
    pub backoff_factor: f64,
    /// First delay, in seconds.
    pub initial_delay: f64,
    /// Upper bound of any delay, in seconds.
    pub max_delay: f64,
    /// How delays grow.
    pub strategy: RetryStrategy,
    /// Exception classes (by name, e.g. `"ValueError"`, matched with `isinstance`) worth a
    /// retry; empty means every error.
    pub retryable_errors: Vec<String>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_retries: 3,
            backoff_factor: 2.0,
            initial_delay: 1.0,
            max_delay: 60.0,
            strategy: RetryStrategy::Exponential,
            retryable_errors: Vec::new(),
        }
    }
}

impl fmt::Debug for RetryPolicy {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let classes: Vec<String> = self
            .retryable_errors
            .iter()
            .map(|c| format!("<class {}>", str_repr(c)))
            .collect();
        write!(
            f,
            "RetryPolicy(max_retries={}, backoff_factor={}, initial_delay={}, max_delay={}, \
             strategy={:?}, retryable_errors=[{}])",
            self.max_retries,
            float_repr(self.backoff_factor),
            float_repr(self.initial_delay),
            float_repr(self.max_delay),
            self.strategy,
            classes.join(", ")
        )
    }
}

/// Failure recovery result (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct FailureRecovery {
    /// Whether to retry.
    pub should_retry: bool,
    /// Delay before the retry, in seconds.
    pub retry_delay: f64,
    /// Recovery action name.
    pub recovery_action: Option<String>,
    /// Extra information.
    pub metadata: Map<String, Value>,
}

impl fmt::Debug for FailureRecovery {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FailureRecovery(should_retry={}, retry_delay={}, recovery_action={}, metadata={})",
            bool_repr(self.should_retry),
            float_repr(self.retry_delay),
            opt_str_repr(self.recovery_action.as_deref()),
            json_map_repr(&self.metadata)
        )
    }
}

impl FailureRecovery {
    /// `FailureRecovery(should_retry)` with the dataclass defaults.
    pub fn new(should_retry: bool) -> Self {
        FailureRecovery {
            should_retry,
            retry_delay: 0.0,
            recovery_action: None,
            metadata: Map::new(),
        }
    }
}

/// The classification `classify_error` returns (a dict in the original).
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorClassification {
    /// `type(error).__name__`.
    pub error_type: String,
    /// Severity.
    pub severity: ErrorSeverity,
    /// `str(error)`.
    pub message: String,
    /// `traceback.format_exc()` at classification time: `NoneType: None\n` outside error
    /// handling (frameless, see `PORT-NOTES.md`).
    pub traceback: String,
}

/// What `handle_step_failure` returns (a dict in the original).
#[derive(Debug, Clone, PartialEq)]
pub struct StepFailureRecovery {
    /// `retry`: whether the step should be retried.
    pub retry: bool,
    /// `retry_delay`, in seconds.
    pub retry_delay: f64,
    /// `error_classification`.
    pub error_classification: ErrorClassification,
    /// `recovery_action`.
    pub recovery_action: Option<String>,
}

/// What `ErrorRecovery.recover_from_error` returns (a dict in the original).
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorRecoveryReport {
    /// `recovery_action`.
    pub recovery_action: Option<String>,
    /// `classification`.
    pub classification: ErrorClassification,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Failure handling and recovery system.
///
/// • Error detection and classification
/// • Retry mechanisms and strategies
/// • Error reporting and logging
pub struct FailureHandler {
    /// Configuration (`config` merged with the keyword arguments).
    pub config: Kwargs,
    /// The shared progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    /// `default_max_retries` (default 3) of the fallback policy.
    pub default_max_retries: i64,
    /// `default_backoff_factor` (default 2.0) of the fallback policy.
    pub default_backoff_factor: f64,
    retry_policies: Mutex<IndexMap<String, RetryPolicy>>,
    error_history: Mutex<Vec<Map<String, Value>>>,
    logger: Logger,
}

impl fmt::Debug for FailureHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FailureHandler")
            .field("config", &self.config)
            .field("default_max_retries", &self.default_max_retries)
            .field("default_backoff_factor", &self.default_backoff_factor)
            .field("retry_policies", &*lock(&self.retry_policies))
            .finish_non_exhaustive()
    }
}

impl Default for FailureHandler {
    fn default() -> Self {
        FailureHandler::new(None, Kwargs::new())
    }
}

impl FailureHandler {
    /// `FailureHandler(config=None, **kwargs)`; reads `default_max_retries` and
    /// `default_backoff_factor`.
    pub fn new(config: Option<Kwargs>, kwargs: Kwargs) -> Self {
        let config = init_config(config, kwargs);
        let default_max_retries = config
            .get("default_max_retries")
            .and_then(KwargValue::as_f64)
            .map_or(3, |n| n.ceil() as i64);
        let default_backoff_factor = config
            .get("default_backoff_factor")
            .and_then(KwargValue::as_f64)
            .unwrap_or(2.0);
        FailureHandler {
            config,
            progress_tracker: enabled_progress_tracker(),
            default_max_retries,
            default_backoff_factor,
            retry_policies: Mutex::new(IndexMap::new()),
            error_history: Mutex::new(Vec::new()),
            logger: get_logger("failure_handler"),
        }
    }

    /// `handle_step_failure(step, error, **options)`: classify the error, look up the step
    /// type's retry policy, decide whether to retry (and after which delay), log the failure
    /// and record it in the error history.
    pub fn handle_step_failure(
        &self,
        step: &PipelineStep,
        error: &SemanticaError,
        _options: &Kwargs,
    ) -> StepFailureRecovery {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("FailureHandler"),
            &format!("Handling step failure: {}", step.name),
            None,
        );

        tracker.update_tracking(&tracking_id, None, "Classifying error...");
        let error_classification = self.classify_error(error);

        tracker.update_tracking(&tracking_id, None, "Getting retry policy...");
        let retry_policy = self.get_retry_policy(&step.step_type);

        tracker.update_tracking(&tracking_id, None, "Checking if error is retryable...");
        let should_retry = self.should_retry(error, &retry_policy);

        let retry_delay = if should_retry {
            self.calculate_retry_delay(&step.name, &retry_policy, 1)
        } else {
            0.0
        };

        self.logger
            .error(format!("Step '{}' failed: {error}", step.name));

        tracker.update_tracking(&tracking_id, None, "Recording error history...");
        let mut record = Map::new();
        record.insert("step_name".into(), Value::from(step.name.clone()));
        record.insert("step_type".into(), Value::from(step.step_type.clone()));
        record.insert("error".into(), Value::from(error.to_string()));
        record.insert("error_type".into(), Value::from(error.type_name()));
        record.insert(
            "severity".into(),
            Value::from(error_classification.severity.value()),
        );
        record.insert("timestamp".into(), Value::from(time_now()));
        record.insert("retryable".into(), Value::from(should_retry));
        lock(&self.error_history).push(record);

        let recovery_action = self.determine_recovery_action(error, &error_classification);
        let result = StepFailureRecovery {
            retry: should_retry,
            retry_delay,
            error_classification,
            recovery_action: Some(recovery_action.to_string()),
        };

        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Failure handled: {}",
                if should_retry { "Retry" } else { "No retry" }
            ),
            None,
        );
        result
    }

    /// `classify_error(error)`: `ValidationError` is low severity, `ProcessingError` high;
    /// otherwise a message mentioning timeout/connection is medium and one mentioning
    /// memory/resource high; anything else medium.
    pub fn classify_error(&self, error: &SemanticaError) -> ErrorClassification {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("FailureHandler"),
            &format!("Classifying error: {}", error.type_name()),
            None,
        );
        let error_message = error.to_string();

        tracker.update_tracking(&tracking_id, None, "Determining error severity...");
        let lower = error_message.to_lowercase();
        let severity = if error.is_validation_error() {
            ErrorSeverity::Low
        } else if matches!(error, SemanticaError::Processing(_)) {
            ErrorSeverity::High
        } else if lower.contains("timeout") || lower.contains("connection") {
            ErrorSeverity::Medium
        } else if lower.contains("memory") || lower.contains("resource") {
            ErrorSeverity::High
        } else {
            ErrorSeverity::Medium
        };

        let result = ErrorClassification {
            error_type: error.type_name(),
            severity,
            message: error_message,
            traceback: format_exc(),
        };
        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!("Error classified: {}", severity.value()),
            None,
        );
        result
    }

    /// `set_retry_policy(step_type, policy)`.
    pub fn set_retry_policy(&self, step_type: &str, policy: RetryPolicy) {
        let message = format!("Set retry policy for {step_type}: {policy:?}");
        lock(&self.retry_policies).insert(step_type.to_string(), policy);
        self.logger.debug(message);
    }

    /// `get_retry_policy(step_type)`: the policy set for this step type, else a default
    /// policy with `default_max_retries` and `default_backoff_factor`.
    pub fn get_retry_policy(&self, step_type: &str) -> RetryPolicy {
        lock(&self.retry_policies)
            .get(step_type)
            .cloned()
            .unwrap_or_else(|| RetryPolicy {
                max_retries: self.default_max_retries,
                backoff_factor: self.default_backoff_factor,
                ..RetryPolicy::default()
            })
    }

    /// The retry policies set so far (`retry_policies`).
    pub fn retry_policies(&self) -> IndexMap<String, RetryPolicy> {
        lock(&self.retry_policies).clone()
    }

    /// `_should_retry(error, policy)`: false only when the policy lists retryable errors and
    /// this error is none of them.
    fn should_retry(&self, error: &SemanticaError, policy: &RetryPolicy) -> bool {
        if !policy.retryable_errors.is_empty()
            && !policy
                .retryable_errors
                .iter()
                .any(|class| is_instance(error, class))
        {
            return false;
        }
        true
    }

    /// `_calculate_retry_delay(step_name, policy, attempt=1)`.
    fn calculate_retry_delay(&self, _step_name: &str, policy: &RetryPolicy, attempt: i64) -> f64 {
        let delay = match policy.strategy {
            RetryStrategy::Linear => policy.initial_delay * attempt as f64,
            RetryStrategy::Exponential => {
                policy.initial_delay * policy.backoff_factor.powf((attempt - 1) as f64)
            }
            RetryStrategy::Fixed => policy.initial_delay,
        };
        py_min(delay, policy.max_delay)
    }

    /// `_determine_recovery_action(error, classification)`.
    fn determine_recovery_action(
        &self,
        _error: &SemanticaError,
        classification: &ErrorClassification,
    ) -> &'static str {
        match classification.severity {
            ErrorSeverity::Low => "retry",
            ErrorSeverity::Medium => "retry_with_backoff",
            ErrorSeverity::High => "skip_step",
            ErrorSeverity::Critical => "abort_pipeline",
        }
    }

    /// `retry_failed_step(step, error, **options)`: handle the failure, then either re-raise
    /// `error` (not retryable) or sleep the retry delay and return the recovery.
    pub fn retry_failed_step(
        &self,
        step: &PipelineStep,
        error: &SemanticaError,
        options: &Kwargs,
    ) -> Result<StepFailureRecovery, SemanticaError> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("FailureHandler"),
            &format!("Retrying failed step: {}", step.name),
            None,
        );
        let recovery = self.handle_step_failure(step, error, options);
        if !recovery.retry {
            tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                "Step not retryable, raising error",
                None,
            );
            tracker.stop_tracking(&tracking_id, Some("failed"), &error.to_string(), None);
            return Err(error.clone());
        }
        if recovery.retry_delay > 0.0 {
            tracker.update_tracking(
                &tracking_id,
                None,
                &format!(
                    "Waiting {}s before retry...",
                    float_repr(recovery.retry_delay)
                ),
            );
            sleep_secs(recovery.retry_delay);
        }
        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!("Retry scheduled for step: {}", step.name),
            None,
        );
        Ok(recovery)
    }

    /// `get_error_history(step_name=None)`: the recorded failures (`step_name`, `step_type`,
    /// `error`, `error_type`, `severity`, `timestamp`, `retryable`), optionally for one step.
    pub fn get_error_history(&self, step_name: Option<&str>) -> Vec<Map<String, Value>> {
        let history = lock(&self.error_history);
        match step_name.filter(|s| !s.is_empty()) {
            Some(name) => history
                .iter()
                .filter(|e| e.get("step_name").and_then(Value::as_str) == Some(name))
                .cloned()
                .collect(),
            None => history.clone(),
        }
    }

    /// `clear_error_history()`.
    pub fn clear_error_history(&self) {
        lock(&self.error_history).clear();
    }

    /// `handle_failure(error, policy, retry_count=0)`: retry while `retry_count <
    /// max_retries` (and the error is retryable), with the strategy's delay for attempt
    /// `retry_count + 1`, capped at `max_delay`.
    pub fn handle_failure(
        &self,
        error: &SemanticaError,
        policy: &RetryPolicy,
        retry_count: Option<i64>,
    ) -> RecoveryAction {
        let retry_count = retry_count.unwrap_or(0);
        let should_retry = retry_count < policy.max_retries && self.should_retry(error, policy);
        let retry_delay = if should_retry {
            let attempt = retry_count + 1;
            let delay = match policy.strategy {
                RetryStrategy::Linear => policy.initial_delay * attempt as f64,
                RetryStrategy::Exponential => {
                    policy.initial_delay * policy.backoff_factor.powf(retry_count as f64)
                }
                RetryStrategy::Fixed => policy.initial_delay,
            };
            py_min(delay, policy.max_delay)
        } else {
            0.0
        };
        RecoveryAction::new(should_retry, Some(retry_delay))
    }
}

/// Python's `min(a, b)`: `a` unless `b < a`.
fn py_min(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// Recovery action result from `handle_failure`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecoveryAction {
    /// Whether to retry.
    pub should_retry: bool,
    /// Delay before the retry, in seconds.
    pub retry_delay: f64,
}

impl RecoveryAction {
    /// `RecoveryAction(should_retry, retry_delay=0.0)`.
    pub fn new(should_retry: bool, retry_delay: Option<f64>) -> Self {
        RecoveryAction {
            should_retry,
            retry_delay: retry_delay.unwrap_or(0.0),
        }
    }
}

/// Retry handler for failed steps.
#[derive(Debug)]
pub struct RetryHandler {
    /// The underlying failure handler.
    pub failure_handler: FailureHandler,
}

impl Default for RetryHandler {
    fn default() -> Self {
        RetryHandler::new(None, None, Kwargs::new())
            .expect("no keyword arguments can collide with the defaults")
    }
}

impl RetryHandler {
    /// `RetryHandler(max_retries=3, backoff_factor=2.0, **config)`: a failure handler with
    /// these defaults. Raises CPython's `TypeError` when `config` repeats
    /// `default_max_retries`/`default_backoff_factor`.
    pub fn new(
        max_retries: Option<i64>,
        backoff_factor: Option<f64>,
        config: Kwargs,
    ) -> Result<Self, SemanticaError> {
        let mut config = config;
        let max_retries = bind_keyword(
            RETRY_HANDLER_FUNC,
            max_retries.map(KwargValue::Int),
            &mut config,
            "max_retries",
        )?
        .unwrap_or(KwargValue::Int(3));
        let backoff_factor = bind_keyword(
            RETRY_HANDLER_FUNC,
            backoff_factor.map(KwargValue::Float),
            &mut config,
            "backoff_factor",
        )?
        .unwrap_or(KwargValue::Float(2.0));
        let mut explicit = Kwargs::new();
        explicit.insert("default_max_retries".into(), max_retries);
        explicit.insert("default_backoff_factor".into(), backoff_factor);
        let merged = merge_call_kwargs(FAILURE_HANDLER_FUNC, &[&explicit, &config])?;
        let (config, kwargs) = split_config(&merged);
        Ok(RetryHandler {
            failure_handler: FailureHandler::new(config, kwargs),
        })
    }

    /// `retry_failed_step(step, error)`.
    pub fn retry_failed_step(
        &self,
        step: &PipelineStep,
        error: &SemanticaError,
    ) -> Result<StepFailureRecovery, SemanticaError> {
        self.failure_handler
            .retry_failed_step(step, error, &Kwargs::new())
    }

    /// `set_retry_policy(step_type, policy)`.
    pub fn set_retry_policy(&self, step_type: &str, policy: RetryPolicy) {
        self.failure_handler.set_retry_policy(step_type, policy);
    }
}

/// Fallback handler for service failures.
#[derive(Debug, Clone)]
pub struct FallbackHandler {
    /// Configuration (`**config`).
    pub config: Kwargs,
    /// Strategy per service name (`"default"` applies to every other service).
    pub fallback_strategies: IndexMap<String, String>,
    #[allow(dead_code)]
    logger: Logger,
}

impl Default for FallbackHandler {
    fn default() -> Self {
        FallbackHandler::new(Kwargs::new())
    }
}

impl FallbackHandler {
    /// `FallbackHandler(**config)`.
    pub fn new(config: Kwargs) -> Self {
        FallbackHandler {
            config,
            fallback_strategies: IndexMap::new(),
            logger: get_logger("fallback_handler"),
        }
    }

    /// `set_fallback_strategy(strategy)`: the default strategy.
    pub fn set_fallback_strategy(&mut self, strategy: &str) {
        self.fallback_strategies
            .insert("default".to_string(), strategy.to_string());
    }

    /// `handle_service_failure(service_name)`: `{"strategy": ..., "service": ...}` with the
    /// service's strategy, else the default one, else `"abort"`.
    pub fn handle_service_failure(&self, service_name: &str) -> Map<String, Value> {
        let strategy = self
            .fallback_strategies
            .get(service_name)
            .or_else(|| self.fallback_strategies.get("default"))
            .map_or("abort", String::as_str);
        let mut result = Map::new();
        result.insert("strategy".into(), Value::from(strategy));
        result.insert("service".into(), Value::from(service_name));
        result
    }

    /// `switch_to_backup(primary_failed)`: whether to switch (= `primary_failed`).
    pub fn switch_to_backup(&self, primary_failed: bool) -> bool {
        primary_failed
    }
}

/// Error recovery system.
#[derive(Debug)]
pub struct ErrorRecovery {
    /// Configuration (`**config`).
    pub config: Kwargs,
    /// The failure handler used for classification.
    pub failure_handler: FailureHandler,
    #[allow(dead_code)]
    logger: Logger,
}

impl Default for ErrorRecovery {
    fn default() -> Self {
        ErrorRecovery::new(Kwargs::new())
    }
}

impl ErrorRecovery {
    /// `ErrorRecovery(**config)`.
    pub fn new(config: Kwargs) -> Self {
        let (handler_config, handler_kwargs) = split_config(&config);
        ErrorRecovery {
            failure_handler: FailureHandler::new(handler_config, handler_kwargs),
            config,
            logger: get_logger("error_recovery"),
        }
    }

    /// `analyze_error(error)`: the failure handler's classification.
    pub fn analyze_error(&self, error: &SemanticaError) -> ErrorClassification {
        self.failure_handler.classify_error(error)
    }

    /// `recover_from_error(error, context)`: the recovery action for the error's severity and
    /// the classification.
    pub fn recover_from_error(
        &self,
        error: &SemanticaError,
        _context: &Kwargs,
    ) -> ErrorRecoveryReport {
        let classification = self.analyze_error(error);
        ErrorRecoveryReport {
            recovery_action: Some(
                self.failure_handler
                    .determine_recovery_action(error, &classification)
                    .to_string(),
            ),
            classification,
        }
    }
}
