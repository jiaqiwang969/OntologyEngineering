//! Rust port of Semantica's `semantica.pipeline` package (see
//! `semantica-rs/scope/sem-pipeline.json`): pipeline construction, execution, failure
//! handling, parallelism, resource scheduling, validation and templates.
//!
//! The crate root re-exports what `semantica/pipeline/__init__.py` exports (`__all__`), so
//! `from semantica.pipeline import X` becomes `use sem_pipeline::X`. Module files mirror the
//! original: [`pipeline_builder`], [`execution_engine`], [`failure_handler`],
//! [`parallelism_manager`], [`resource_scheduler`], [`pipeline_validator`],
//! [`pipeline_templates`] and [`pipeline_provenance`]. [`kwargs`] holds the Python
//! `**kwargs` model the API is built on: configuration dicts and execution options are
//! [`Kwargs`] maps whose values ([`KwargValue`]) can be data, [`StepHandler`]s or opaque
//! objects ([`PyObject`]), and the data flowing through a pipeline is a
//! `serde_json::Value`.
//!
//! ```
//! use sem_pipeline::{kwargs, ExecutionEngine, PipelineBuilder, StepHandler};
//! use serde_json::json;
//!
//! let mut builder = PipelineBuilder::default();
//! let double = StepHandler::new("double", |data, _| Ok(json!(data.as_i64().unwrap_or(0) * 2)));
//! builder.add_step("double", "math", kwargs! {"handler" => double});
//! let mut pipeline = builder.build(Some("demo")).unwrap();
//! let result = ExecutionEngine::default().execute_pipeline(&mut pipeline, json!(21), &kwargs! {});
//! assert!(result.success);
//! assert_eq!(result.output, json!(42));
//! ```

/// A Python `Enum` whose members have string values: `value()`, `name()`, `from_value()`
/// (`Enum(value)`), `Display` = `str(member)` and `Debug` = `repr(member)`.
macro_rules! py_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = ($member:literal, $value:literal) ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// The members in definition order (`list(Enum)`).
            pub const MEMBERS: &'static [$name] = &[$($name::$variant),+];

            /// `member.value`.
            pub fn value(self) -> &'static str {
                match self {
                    $($name::$variant => $value),+
                }
            }

            /// `member.name`.
            pub fn name(self) -> &'static str {
                match self {
                    $($name::$variant => $member),+
                }
            }

            /// `Enum(value)`: the member with this value, or Python's `ValueError`.
            pub fn from_value(value: &str) -> Result<$name, sem_utils::SemanticaError> {
                match value {
                    $($value => Ok($name::$variant),)+
                    _ => Err(sem_utils::SemanticaError::Value(format!(
                        "{} is not a valid {}",
                        sem_utils::pycompat::fmt::str_repr(value),
                        stringify!($name)
                    ))),
                }
            }
        }

        impl std::fmt::Display for $name {
            /// `str(member)`: `Enum.NAME`.
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}.{}", stringify!($name), self.name())
            }
        }

        impl std::fmt::Debug for $name {
            /// `repr(member)`: `<Enum.NAME: 'value'>`.
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "<{}.{}: {}>",
                    stringify!($name),
                    self.name(),
                    sem_utils::pycompat::fmt::str_repr(self.value())
                )
            }
        }
    };
}

pub mod kwargs;

pub mod execution_engine;
pub mod failure_handler;
pub mod parallelism_manager;
pub mod pipeline_builder;
pub mod pipeline_provenance;
pub mod pipeline_templates;
pub mod pipeline_validator;
pub mod resource_scheduler;

// Execution
pub use execution_engine::{ExecutionEngine, ExecutionResult, PipelineStatus, ProgressTracker};

// Failure handling
pub use failure_handler::{
    ErrorRecovery, ErrorSeverity, FailureHandler, FailureRecovery, FallbackHandler, RetryHandler,
    RetryPolicy, RetryStrategy,
};

// Parallelism
pub use parallelism_manager::{
    ParallelExecutionResult, ParallelExecutor, ParallelismManager, Task,
};

// Pipeline construction
pub use pipeline_builder::{
    Pipeline, PipelineBuilder, PipelineSerializer, PipelineStep, StepStatus,
};

// Templates
pub use pipeline_templates::{PipelineTemplate, PipelineTemplateManager};

// Validation
pub use pipeline_validator::{PipelineValidator, ValidationResult};

// Resource management
pub use resource_scheduler::{Resource, ResourceAllocation, ResourceScheduler, ResourceType};

// Rust-side companions of the exported API: the `**kwargs` model, handler types, result
// records the original returns as dicts, and the collaborator traits.
pub use execution_engine::{DeltaTripletStore, VersionManager};
pub use failure_handler::{
    ErrorClassification, ErrorRecoveryReport, RecoveryAction, StepFailureRecovery,
};
pub use kwargs::{KwargValue, Kwargs, PyObject, StepHandler, kwargs_from_json};
pub use parallelism_manager::TaskHandler;
pub use pipeline_builder::SerializedPipeline;
pub use pipeline_provenance::{
    PipelineWithProvenance, PipelineWithProvenanceOptions, ProvenanceImportBlock,
    block_provenance_import,
};
pub use pipeline_validator::{
    ConstructTemplateRegistry, DependencyCheck, PipelineLike, TemplateParameterSpec,
    ValidateStepOptions,
};
pub use resource_scheduler::{RLock, RLockGuard};

/// `__all__` of `semantica.pipeline`.
pub const ALL: [&str; 29] = [
    // Pipeline construction
    "PipelineBuilder",
    "Pipeline",
    "PipelineStep",
    "StepStatus",
    "PipelineSerializer",
    // Execution
    "ExecutionEngine",
    "ExecutionResult",
    "PipelineStatus",
    "ProgressTracker",
    // Failure handling
    "FailureHandler",
    "RetryHandler",
    "FallbackHandler",
    "ErrorRecovery",
    "RetryPolicy",
    "RetryStrategy",
    "ErrorSeverity",
    "FailureRecovery",
    // Parallelism
    "ParallelismManager",
    "ParallelExecutor",
    "Task",
    "ParallelExecutionResult",
    // Resource management
    "ResourceScheduler",
    "Resource",
    "ResourceAllocation",
    "ResourceType",
    // Validation
    "PipelineValidator",
    "ValidationResult",
    // Templates
    "PipelineTemplateManager",
    "PipelineTemplate",
];

/// `get_progress_tracker()` followed by the original's "ensure progress tracker is enabled".
pub(crate) fn enabled_progress_tracker() -> std::sync::Arc<sem_utils::ProgressTracker> {
    let tracker = sem_utils::get_progress_tracker();
    if !tracker.enabled() {
        tracker.set_enabled(true);
    }
    tracker
}

/// `time.time()`.
pub(crate) fn time_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// `time.sleep(seconds)` (non-positive or non-finite values return at once).
pub(crate) fn sleep_secs(seconds: f64) {
    if seconds.is_finite() && seconds > 0.0 {
        std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
    }
}
