//! Pipeline construction (`semantica/pipeline/pipeline_builder.py`): the builder DSL, the
//! [`Pipeline`] and [`PipelineStep`] records, and [`PipelineSerializer`].
//!
//! Python shares step objects between a builder and the pipelines it builds; here
//! [`PipelineBuilder::build`] copies them (see `PORT-NOTES.md`).

use std::fmt;
use std::sync::Arc;

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{py_repr, str_list_repr, str_repr};
use sem_utils::pycompat::json::{DumpsOptions, dumps};
use sem_utils::{ProgressTracker, SemanticaError, ValidationError, get_logger};
use serde_json::Value;

use crate::enabled_progress_tracker;
use crate::kwargs::{
    KwargValue, Kwargs, PyObject, StepHandler, kwargs_repr, kwargs_to_json, split_config,
};
use crate::pipeline_validator::{PipelineLike, PipelineValidator};

/// The function name CPython prints for `PipelineBuilder.add_step` in call errors.
pub(crate) const ADD_STEP_FUNC: &str =
    "semantica.pipeline.pipeline_builder.PipelineBuilder.add_step";

py_enum! {
    /// Pipeline step status.
    pub enum StepStatus {
        /// `pending`.
        Pending = ("PENDING", "pending"),
        /// `running`.
        Running = ("RUNNING", "running"),
        /// `completed`.
        Completed = ("COMPLETED", "completed"),
        /// `failed`.
        Failed = ("FAILED", "failed"),
        /// `skipped`.
        Skipped = ("SKIPPED", "skipped"),
    }
}

/// Pipeline step definition (a dataclass in the original).
#[derive(Clone)]
pub struct PipelineStep {
    /// Step name/identifier.
    pub name: String,
    /// Step type/category.
    pub step_type: String,
    /// Step configuration: the keyword arguments given to `add_step` (including `handler`
    /// and `dependencies` when given), passed to the handler on every call.
    pub config: Kwargs,
    /// Names of the steps this step depends on.
    pub dependencies: Vec<String>,
    /// The step handler (`None`: the step passes its input through).
    pub handler: Option<StepHandler>,
    /// Execution status.
    pub status: StepStatus,
    /// The handler's last result (`Value::Null` for `None`).
    pub result: Value,
    /// The last error the step raised.
    pub error: Option<SemanticaError>,
    /// Incremental delta mode: the step receives the delta between two graph versions.
    pub delta_mode: bool,
    /// Base version of the delta.
    pub base_version_id: Option<String>,
    /// Target version of the delta.
    pub target_version_id: Option<String>,
}

impl PipelineStep {
    /// `PipelineStep(name, step_type)` with the dataclass defaults.
    pub fn new(name: impl Into<String>, step_type: impl Into<String>) -> Self {
        PipelineStep {
            name: name.into(),
            step_type: step_type.into(),
            config: Kwargs::new(),
            dependencies: Vec::new(),
            handler: None,
            status: StepStatus::Pending,
            result: Value::Null,
            error: None,
            delta_mode: false,
            base_version_id: None,
            target_version_id: None,
        }
    }

    /// `getattr(step, name)`: the dataclass fields; any other name raises Python's
    /// `AttributeError: 'PipelineStep' object has no attribute '<name>'`.
    pub fn getattr(&self, name: &str) -> Result<KwargValue, SemanticaError> {
        let optional = |value: &Option<String>| match value {
            Some(s) => KwargValue::Str(s.clone()),
            None => KwargValue::None,
        };
        Ok(match name {
            "name" => KwargValue::Str(self.name.clone()),
            "step_type" => KwargValue::Str(self.step_type.clone()),
            "config" => KwargValue::Dict(self.config.clone()),
            "dependencies" => KwargValue::from(self.dependencies.clone()),
            "handler" => self
                .handler
                .clone()
                .map_or(KwargValue::None, KwargValue::Handler),
            "status" => KwargValue::Object(PyObject::with_type_name("StepStatus", self.status)),
            "result" => KwargValue::from_json(self.result.clone()),
            "error" => match &self.error {
                Some(e) => KwargValue::Object(PyObject::with_type_name(e.type_name(), e.clone())),
                None => KwargValue::None,
            },
            "delta_mode" => KwargValue::Bool(self.delta_mode),
            "base_version_id" => optional(&self.base_version_id),
            "target_version_id" => optional(&self.target_version_id),
            _ => {
                return Err(SemanticaError::Attribute(format!(
                    "'PipelineStep' object has no attribute '{name}'"
                )));
            }
        })
    }
}

fn opt_repr(value: &Option<String>) -> String {
    value
        .as_deref()
        .map_or_else(|| "None".to_string(), str_repr)
}

impl fmt::Debug for PipelineStep {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let handler = self
            .handler
            .as_ref()
            .map_or_else(|| "None".to_string(), |h| format!("{h:?}"));
        let error = self
            .error
            .as_ref()
            .map_or_else(|| "None".to_string(), |e| format!("{e:?}"));
        write!(
            f,
            "PipelineStep(name={}, step_type={}, config={}, dependencies={}, handler={}, \
             status={:?}, result={}, error={}, delta_mode={}, base_version_id={}, \
             target_version_id={})",
            str_repr(&self.name),
            str_repr(&self.step_type),
            kwargs_repr(&self.config),
            str_list_repr(&self.dependencies),
            handler,
            self.status,
            py_repr(&self.result),
            error,
            if self.delta_mode { "True" } else { "False" },
            opt_repr(&self.base_version_id),
            opt_repr(&self.target_version_id),
        )
    }
}

impl PartialEq for PipelineStep {
    /// The dataclass `__eq__` (field by field; handlers by identity).
    fn eq(&self, other: &PipelineStep) -> bool {
        let handlers_eq = match (&self.handler, &other.handler) {
            (None, None) => true,
            (Some(a), Some(b)) => a.ptr_eq(b),
            _ => false,
        };
        self.name == other.name
            && self.step_type == other.step_type
            && self.config == other.config
            && self.dependencies == other.dependencies
            && handlers_eq
            && self.status == other.status
            && KwargValue::from_json(self.result.clone()) == other.result
            && self.error == other.error
            && self.delta_mode == other.delta_mode
            && self.base_version_id == other.base_version_id
            && self.target_version_id == other.target_version_id
    }
}

/// Pipeline definition (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct Pipeline {
    /// Pipeline name (also its id in the execution engine).
    pub name: String,
    /// The steps, in insertion order.
    pub steps: Vec<PipelineStep>,
    /// Pipeline configuration (`parallelism`, ...).
    pub config: Kwargs,
    /// Metadata (`step_count`, `parallelism`, version information, ...).
    pub metadata: Kwargs,
}

impl Pipeline {
    /// `Pipeline(name)` with the dataclass defaults.
    pub fn new(name: impl Into<String>) -> Self {
        Pipeline {
            name: name.into(),
            steps: Vec::new(),
            config: Kwargs::new(),
            metadata: Kwargs::new(),
        }
    }
}

impl fmt::Debug for Pipeline {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let steps: Vec<String> = self.steps.iter().map(|s| format!("{s:?}")).collect();
        write!(
            f,
            "Pipeline(name={}, steps=[{}], config={}, metadata={})",
            str_repr(&self.name),
            steps.join(", "),
            kwargs_repr(&self.config),
            kwargs_repr(&self.metadata),
        )
    }
}

/// A serialized pipeline: the JSON text (`format="json"`) or the dict itself (any other
/// format).
#[derive(Clone, Debug, PartialEq)]
pub enum SerializedPipeline {
    /// `json.dumps(pipeline_data, indent=2)`.
    Json(String),
    /// The pipeline data dict.
    Dict(Kwargs),
}

impl SerializedPipeline {
    /// The JSON text, for `format="json"`.
    pub fn as_json(&self) -> Option<&str> {
        match self {
            SerializedPipeline::Json(text) => Some(text),
            SerializedPipeline::Dict(_) => None,
        }
    }

    /// The dict, for any other format.
    pub fn as_dict(&self) -> Option<&Kwargs> {
        match self {
            SerializedPipeline::Dict(map) => Some(map),
            SerializedPipeline::Json(_) => None,
        }
    }
}

impl From<String> for SerializedPipeline {
    fn from(value: String) -> Self {
        SerializedPipeline::Json(value)
    }
}

impl From<&str> for SerializedPipeline {
    fn from(value: &str) -> Self {
        SerializedPipeline::Json(value.to_string())
    }
}

impl From<Kwargs> for SerializedPipeline {
    fn from(value: Kwargs) -> Self {
        SerializedPipeline::Dict(value)
    }
}

/// `str(x)` for a value used as a name.
fn name_text(value: &KwargValue) -> String {
    value.py_str()
}

/// `config.get("dependencies", [])` read as the list of names the step iterates.
fn dependencies_of(value: Option<&KwargValue>) -> Vec<String> {
    match value {
        Some(KwargValue::List(items)) => items.iter().map(name_text).collect(),
        // Iterating a string yields its characters.
        Some(KwargValue::Str(s)) => s.chars().map(String::from).collect(),
        Some(KwargValue::Dict(map)) => map.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

/// `config.get("handler")` as the step's handler: a truthy non-callable value becomes a
/// handler that raises the `TypeError` calling it raises in Python.
fn handler_of(value: Option<&KwargValue>) -> Option<StepHandler> {
    match value {
        Some(KwargValue::Handler(h)) => Some(h.clone()),
        Some(other) if other.truthy() => Some(StepHandler::not_callable(&other.type_name())),
        _ => None,
    }
}

/// A popped `base_version_id`/`target_version_id`.
fn optional_text(value: Option<KwargValue>) -> Option<String> {
    match value {
        None | Some(KwargValue::None) => None,
        Some(KwargValue::Str(s)) => Some(s),
        Some(other) => Some(other.py_str()),
    }
}

/// `len(value)`'s `TypeError` for values without a length.
fn no_len(value: &KwargValue) -> SemanticaError {
    SemanticaError::Type(format!(
        "object of type '{}' has no len()",
        value.type_name()
    ))
}

/// `item.get(...)` on something that is not a dict.
fn no_get(value: &KwargValue) -> SemanticaError {
    SemanticaError::Attribute(format!(
        "'{}' object has no attribute 'get'",
        value.type_name()
    ))
}

/// `func(..., **value)` for a non-mapping `value`.
pub(crate) fn not_a_mapping(func: &str, value: &KwargValue) -> SemanticaError {
    SemanticaError::Type(format!(
        "{func}() argument after ** must be a mapping, not {}",
        value.type_name()
    ))
}

/// Pipeline construction and configuration handler.
///
/// • Constructs processing pipelines using DSL
/// • Configures pipeline steps and connections
/// • Validates pipeline structure and dependencies
/// • Handles pipeline serialization
#[derive(Debug)]
pub struct PipelineBuilder {
    /// Configuration (`config` merged with the keyword arguments).
    pub config: Kwargs,
    /// The shared progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    /// The validator used by [`PipelineBuilder::build`].
    pub validator: PipelineValidator,
    /// The steps added so far.
    pub steps: Vec<PipelineStep>,
    /// Handlers registered per step type.
    pub step_registry: IndexMap<String, StepHandler>,
    /// Pipeline-level configuration (`parallelism`).
    pub pipeline_config: Kwargs,
    logger: Logger,
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        PipelineBuilder::new(None, Kwargs::new())
    }
}

impl PipelineBuilder {
    /// `PipelineBuilder(config=None, **kwargs)`.
    pub fn new(config: Option<Kwargs>, kwargs: Kwargs) -> Self {
        let logger = get_logger("pipeline_builder");
        let config = crate::kwargs::init_config(config, kwargs);
        let progress_tracker = enabled_progress_tracker();
        let (validator_config, validator_kwargs) = split_config(&config);
        let validator = PipelineValidator::new(validator_config, validator_kwargs);
        PipelineBuilder {
            config,
            progress_tracker,
            validator,
            steps: Vec::new(),
            step_registry: IndexMap::new(),
            pipeline_config: Kwargs::new(),
            logger,
        }
    }

    /// `add_step(step_name, step_type, **config)`: add a step and return it.
    ///
    /// `delta_mode`, `base_version_id` and `target_version_id` are taken out of `config`;
    /// `dependencies` and `handler` stay in it and also become the step's fields.
    pub fn add_step(
        &mut self,
        step_name: &str,
        step_type: &str,
        mut config: Kwargs,
    ) -> &mut PipelineStep {
        let delta_mode = config
            .shift_remove("delta_mode")
            .is_some_and(|v| v.truthy());
        let base_version_id = optional_text(config.shift_remove("base_version_id"));
        let target_version_id = optional_text(config.shift_remove("target_version_id"));
        let dependencies = dependencies_of(config.get("dependencies"));
        let handler = handler_of(config.get("handler"));

        let step = PipelineStep {
            dependencies,
            handler,
            delta_mode,
            base_version_id,
            target_version_id,
            config,
            ..PipelineStep::new(step_name, step_type)
        };
        self.steps.push(step);
        self.logger.debug(format!(
            "Added step: {step_name} ({step_type}) | Delta Mode: {}",
            if delta_mode { "True" } else { "False" }
        ));
        self.steps.last_mut().expect("a step was just added")
    }

    /// `connect_steps(from_step, to_step, **options)`: make `to_step` depend on `from_step`.
    ///
    /// Raises `ValidationError: Target step not found: <to_step>`.
    pub fn connect_steps(
        &mut self,
        from_step: &str,
        to_step: &str,
        _options: &Kwargs,
    ) -> Result<&mut Self, SemanticaError> {
        let Some(target) = self.steps.iter_mut().find(|s| s.name == to_step) else {
            return Err(ValidationError::new(format!("Target step not found: {to_step}")).into());
        };
        if !target.dependencies.iter().any(|d| d == from_step) {
            // In Python the step's dependency list *is* `config["dependencies"]` when that
            // was given; keep the two in step.
            if let Some(KwargValue::List(items)) = target.config.get_mut("dependencies")
                && items.len() == target.dependencies.len()
                && items
                    .iter()
                    .zip(&target.dependencies)
                    .all(|(item, dep)| item.as_str() == Some(dep.as_str()))
            {
                items.push(KwargValue::Str(from_step.to_string()));
            }
            target.dependencies.push(from_step.to_string());
        }
        Ok(self)
    }

    /// `set_parallelism(level)`.
    pub fn set_parallelism(&mut self, level: impl Into<KwargValue>) -> &mut Self {
        self.pipeline_config
            .insert("parallelism".to_string(), level.into());
        self
    }

    /// `build(name="default_pipeline")`: validate the builder and create the pipeline.
    ///
    /// Raises `ValidationError: Pipeline validation failed: [...]`.
    pub fn build(&self, name: Option<&str>) -> Result<Pipeline, SemanticaError> {
        let name = name.unwrap_or("default_pipeline");
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("PipelineBuilder"),
            &format!("Building pipeline: {name}"),
            None,
        );
        let result: Result<Pipeline, SemanticaError> = (|| {
            tracker.update_tracking(&tracking_id, None, "Validating pipeline structure...");
            let validation = self
                .validator
                .validate_pipeline(PipelineLike::Builder(self), None);
            if !validation.valid {
                return Err(ValidationError::new(format!(
                    "Pipeline validation failed: {}",
                    str_list_repr(&validation.errors)
                ))
                .into());
            }

            tracker.update_tracking(&tracking_id, None, "Creating pipeline object...");
            let mut metadata = Kwargs::new();
            metadata.insert("step_count".to_string(), KwargValue::from(self.steps.len()));
            metadata.insert(
                "parallelism".to_string(),
                self.pipeline_config
                    .get("parallelism")
                    .cloned()
                    .unwrap_or(KwargValue::Int(1)),
            );
            let pipeline = Pipeline {
                name: name.to_string(),
                steps: self.steps.clone(),
                config: self.pipeline_config.clone(),
                metadata,
            };

            let message = format!("Built pipeline: {name} with {} steps", self.steps.len());
            self.logger.info(&message);
            tracker.stop_tracking(&tracking_id, Some("completed"), &message, None);
            Ok(pipeline)
        })();
        if let Err(e) = &result {
            tracker.stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `build_pipeline(pipeline_config, **options)`: add the configured steps
    /// (`{"name", "type", "config"}` dicts under `steps`), apply `parallelism`, and build.
    pub fn build_pipeline(
        &mut self,
        pipeline_config: &Kwargs,
        _options: &Kwargs,
    ) -> Result<Pipeline, SemanticaError> {
        let tracker = Arc::clone(&self.progress_tracker);
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("PipelineBuilder"),
            "Building pipeline from configuration",
            None,
        );
        let result: Result<Pipeline, SemanticaError> = (|| {
            tracker.update_tracking(&tracking_id, None, "Parsing pipeline configuration...");
            let pipeline_name = pipeline_config
                .get("name")
                .map_or_else(|| "default_pipeline".to_string(), name_text);
            let empty = KwargValue::List(Vec::new());
            let steps_config = pipeline_config.get("steps").unwrap_or(&empty);

            let items: Vec<KwargValue> = match steps_config {
                KwargValue::List(items) => items.clone(),
                KwargValue::Str(s) => s.chars().map(|c| KwargValue::Str(c.into())).collect(),
                KwargValue::Dict(map) => map.keys().map(|k| KwargValue::Str(k.clone())).collect(),
                other => return Err(no_len(other)),
            };
            tracker.update_tracking(
                &tracking_id,
                None,
                &format!("Adding {} steps from configuration...", items.len()),
            );
            for step_config in &items {
                let KwargValue::Dict(step_config) = step_config else {
                    return Err(no_get(step_config));
                };
                let step_name = step_config.get("name").cloned().unwrap_or_default();
                let step_type = step_config.get("type").cloned().unwrap_or_default();
                if step_name.truthy() && step_type.truthy() {
                    let config = match step_config.get("config") {
                        None => Kwargs::new(),
                        Some(KwargValue::Dict(map)) => map.clone(),
                        Some(other) => return Err(not_a_mapping(ADD_STEP_FUNC, other)),
                    };
                    self.add_step(&name_text(&step_name), &name_text(&step_type), config);
                }
            }

            if let Some(parallelism) = pipeline_config.get("parallelism") {
                tracker.update_tracking(&tracking_id, None, "Setting parallelism...");
                self.set_parallelism(parallelism.clone());
            }

            let result = self.build(Some(&pipeline_name))?;
            tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!("Built pipeline from configuration: {pipeline_name}"),
                None,
            );
            Ok(result)
        })();
        if let Err(e) = &result {
            tracker.stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `register_step_handler(step_type, handler)`.
    pub fn register_step_handler(&mut self, step_type: &str, handler: StepHandler) {
        self.step_registry.insert(step_type.to_string(), handler);
        self.logger
            .debug(format!("Registered handler for step type: {step_type}"));
    }

    /// `get_step(step_name)`: the first step with this name.
    pub fn get_step(&self, step_name: &str) -> Option<&PipelineStep> {
        self.steps.iter().find(|s| s.name == step_name)
    }

    /// The first step with this name, mutably.
    pub fn get_step_mut(&mut self, step_name: &str) -> Option<&mut PipelineStep> {
        self.steps.iter_mut().find(|s| s.name == step_name)
    }

    /// `serialize(format="json")`: the builder's steps and pipeline configuration (pipeline
    /// name `"pipeline"`) as JSON text, or as a dict for any other format.
    ///
    /// JSON output raises `TypeError: Object of type function is not JSON serializable` when
    /// a step configuration holds a handler, as `json.dumps` does.
    pub fn serialize(&self, format: Option<&str>) -> Result<SerializedPipeline, SemanticaError> {
        let steps: Vec<KwargValue> = self
            .steps
            .iter()
            .map(|step| {
                let mut data = Kwargs::new();
                data.insert("name".into(), KwargValue::Str(step.name.clone()));
                data.insert("type".into(), KwargValue::Str(step.step_type.clone()));
                data.insert("config".into(), KwargValue::Dict(step.config.clone()));
                data.insert(
                    "dependencies".into(),
                    KwargValue::from(step.dependencies.clone()),
                );
                KwargValue::Dict(data)
            })
            .collect();
        let mut pipeline_data = Kwargs::new();
        pipeline_data.insert("name".into(), KwargValue::Str("pipeline".into()));
        pipeline_data.insert("steps".into(), KwargValue::List(steps));
        pipeline_data.insert(
            "config".into(),
            KwargValue::Dict(self.pipeline_config.clone()),
        );

        if format.unwrap_or("json") == "json" {
            let value = Value::Object(kwargs_to_json(&pipeline_data)?);
            Ok(SerializedPipeline::Json(json_dumps_indent2(&value)))
        } else {
            Ok(SerializedPipeline::Dict(pipeline_data))
        }
    }

    /// `validate_pipeline()`.
    ///
    /// The original reads the validator's result with `result.get('valid', False)`, which a
    /// `ValidationResult` does not have, so this always fails with
    /// `AttributeError: 'ValidationResult' object has no attribute 'get'`.
    pub fn validate_pipeline(&self) -> Result<Kwargs, SemanticaError> {
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("PipelineBuilder"),
            "Validating pipeline",
            None,
        );
        let _result = self
            .validator
            .validate_pipeline(PipelineLike::Builder(self), None);
        let error = SemanticaError::Attribute(
            "'ValidationResult' object has no attribute 'get'".to_string(),
        );
        tracker.stop_tracking(&tracking_id, Some("failed"), &error.to_string(), None);
        Err(error)
    }
}

/// `json.dumps(value, indent=2)`.
pub(crate) fn json_dumps_indent2(value: &Value) -> String {
    dumps(
        value,
        &DumpsOptions {
            indent: Some(2),
            ..DumpsOptions::default()
        },
    )
}

/// Pipeline serialization handler.
///
/// • Serializes pipelines to various formats
/// • Handles pipeline deserialization
/// • Manages pipeline versioning
#[derive(Debug, Clone)]
pub struct PipelineSerializer {
    /// Configuration (`**config`), passed on to the builder used for deserialization.
    pub config: Kwargs,
    #[allow(dead_code)]
    logger: Logger,
}

impl Default for PipelineSerializer {
    fn default() -> Self {
        PipelineSerializer::new(Kwargs::new())
    }
}

impl PipelineSerializer {
    /// `PipelineSerializer(**config)`.
    pub fn new(config: Kwargs) -> Self {
        PipelineSerializer {
            config,
            logger: get_logger("pipeline_serializer"),
        }
    }

    /// `serialize_pipeline(pipeline, format="json", **options)`: name, steps (with their
    /// delta settings), config and metadata as JSON text (`json.dumps(..., indent=2,
    /// default=str)`: handlers print as `<function ...>`), or as a dict for any other format.
    pub fn serialize_pipeline(
        &self,
        pipeline: &Pipeline,
        format: Option<&str>,
        _options: &Kwargs,
    ) -> SerializedPipeline {
        let steps: Vec<KwargValue> = pipeline
            .steps
            .iter()
            .map(|step| {
                let mut data = Kwargs::new();
                data.insert("name".into(), KwargValue::Str(step.name.clone()));
                data.insert("type".into(), KwargValue::Str(step.step_type.clone()));
                data.insert("config".into(), KwargValue::Dict(step.config.clone()));
                data.insert(
                    "dependencies".into(),
                    KwargValue::from(step.dependencies.clone()),
                );
                data.insert("delta_mode".into(), KwargValue::Bool(step.delta_mode));
                data.insert(
                    "base_version_id".into(),
                    KwargValue::from(step.base_version_id.clone()),
                );
                data.insert(
                    "target_version_id".into(),
                    KwargValue::from(step.target_version_id.clone()),
                );
                KwargValue::Dict(data)
            })
            .collect();
        let mut pipeline_data = Kwargs::new();
        pipeline_data.insert("name".into(), KwargValue::Str(pipeline.name.clone()));
        pipeline_data.insert("steps".into(), KwargValue::List(steps));
        pipeline_data.insert("config".into(), KwargValue::Dict(pipeline.config.clone()));
        pipeline_data.insert(
            "metadata".into(),
            KwargValue::Dict(pipeline.metadata.clone()),
        );

        if format.unwrap_or("json") == "json" {
            let value = KwargValue::Dict(pipeline_data).to_json_default_str();
            SerializedPipeline::Json(json_dumps_indent2(&value))
        } else {
            SerializedPipeline::Dict(pipeline_data)
        }
    }

    /// `deserialize_pipeline(serialized_pipeline, **options)`: parse JSON text (a dict is
    /// used as is) and rebuild the pipeline with a fresh builder's `build_pipeline`.
    ///
    /// Only what `build_pipeline` reads comes back: each step's `name`, `type` and `config`,
    /// a top-level `parallelism` and the `name`.
    pub fn deserialize_pipeline(
        &self,
        serialized_pipeline: &SerializedPipeline,
        options: &Kwargs,
    ) -> Result<Pipeline, SemanticaError> {
        let pipeline_data = match serialized_pipeline {
            SerializedPipeline::Json(text) => {
                let value: Value = serde_json::from_str(text)
                    .map_err(|e| SemanticaError::JsonDecode(e.to_string()))?;
                match KwargValue::from_json(value) {
                    KwargValue::Dict(map) => map,
                    other => return Err(no_get(&other)),
                }
            }
            SerializedPipeline::Dict(map) => map.clone(),
        };

        let (config, kwargs) = split_config(&self.config);
        let mut builder = PipelineBuilder::new(config, kwargs);
        builder.build_pipeline(&pipeline_data, options)
    }

    /// `version_pipeline(pipeline, version_info)`: record `version` (default `"1.0"`) and the
    /// whole `version_info` in the pipeline metadata.
    pub fn version_pipeline<'a>(
        &self,
        pipeline: &'a mut Pipeline,
        version_info: &Kwargs,
    ) -> &'a mut Pipeline {
        pipeline.metadata.insert(
            "version".to_string(),
            version_info
                .get("version")
                .cloned()
                .unwrap_or_else(|| KwargValue::Str("1.0".to_string())),
        );
        pipeline.metadata.insert(
            "version_info".to_string(),
            KwargValue::Dict(version_info.clone()),
        );
        pipeline
    }
}
