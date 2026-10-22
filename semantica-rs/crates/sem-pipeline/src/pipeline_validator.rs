//! Pipeline validation (`semantica/pipeline/pipeline_validator.py`): structure, step,
//! dependency (missing, circular, unreachable) and performance checks.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use indexmap::{IndexMap, IndexSet};
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::{str_list_repr, str_repr};
use sem_utils::{ProgressTracker, SemanticaError, get_logger};
use serde_json::{Map, Value};

use crate::enabled_progress_tracker;
use crate::kwargs::{KwargValue, Kwargs, bool_repr, json_map_repr};
use crate::pipeline_builder::{Pipeline, PipelineBuilder, PipelineStep};

/// Pipeline validation result (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct ValidationResult {
    /// Whether no errors were found.
    pub valid: bool,
    /// Error messages.
    pub errors: Vec<String>,
    /// Warning messages.
    pub warnings: Vec<String>,
    /// Extra information (`step_count`).
    pub metadata: Map<String, Value>,
}

impl std::fmt::Debug for ValidationResult {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ValidationResult(valid={}, errors={}, warnings={}, metadata={})",
            bool_repr(self.valid),
            str_list_repr(&self.errors),
            str_list_repr(&self.warnings),
            json_map_repr(&self.metadata)
        )
    }
}

impl ValidationResult {
    /// `ValidationResult(valid)` with the dataclass defaults.
    pub fn new(valid: bool) -> Self {
        ValidationResult {
            valid,
            errors: Vec::new(),
            warnings: Vec::new(),
            metadata: Map::new(),
        }
    }
}

/// What the validator accepts: a [`Pipeline`], a [`PipelineBuilder`], or any other object
/// (such as a file path), which has no steps.
#[derive(Debug, Clone, Copy)]
pub enum PipelineLike<'a> {
    /// A built pipeline.
    Pipeline(&'a Pipeline),
    /// A builder (validated for structure and dependencies only).
    Builder(&'a PipelineBuilder),
    /// Any other object, identified by its Python type name (`"str"` for a path).
    Other(&'static str),
}

impl<'a> PipelineLike<'a> {
    /// The object's steps (`pipeline.steps if hasattr(pipeline, "steps") else []`).
    pub fn steps(&self) -> Option<&'a [PipelineStep]> {
        match self {
            PipelineLike::Pipeline(p) => Some(&p.steps),
            PipelineLike::Builder(b) => Some(&b.steps),
            PipelineLike::Other(_) => None,
        }
    }

    fn type_name(&self) -> &'static str {
        match self {
            PipelineLike::Pipeline(_) => "Pipeline",
            PipelineLike::Builder(_) => "PipelineBuilder",
            PipelineLike::Other(name) => name,
        }
    }
}

impl<'a> From<&'a Pipeline> for PipelineLike<'a> {
    fn from(value: &'a Pipeline) -> Self {
        PipelineLike::Pipeline(value)
    }
}

impl<'a> From<&'a mut Pipeline> for PipelineLike<'a> {
    fn from(value: &'a mut Pipeline) -> Self {
        PipelineLike::Pipeline(value)
    }
}

impl<'a> From<&'a PipelineBuilder> for PipelineLike<'a> {
    fn from(value: &'a PipelineBuilder) -> Self {
        PipelineLike::Builder(value)
    }
}

impl<'a> From<&'a mut PipelineBuilder> for PipelineLike<'a> {
    fn from(value: &'a mut PipelineBuilder) -> Self {
        PipelineLike::Builder(value)
    }
}

impl From<&str> for PipelineLike<'_> {
    /// A string (e.g. a pipeline file path): an object without steps.
    fn from(_: &str) -> Self {
        PipelineLike::Other("str")
    }
}

/// A parameter of a registered construct template (`ParameterDescriptor` in
/// `semantica.triplet_store.construct_templates`), as far as validation needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateParameterSpec {
    /// Parameter name.
    pub name: String,
    /// Whether the parameter must be supplied.
    pub required: bool,
}

/// What `validate_pipeline` needs from a `ConstructTemplateRegistry`: `registry.get(name)`,
/// giving the template's parameters, or `None` when no template has that name.
pub trait ConstructTemplateRegistry {
    /// The parameters of the template registered as `template_name`.
    fn get(&self, template_name: &str) -> Option<Vec<TemplateParameterSpec>>;
}

/// The keyword constraints `validate_step` understands.
#[derive(Clone, Copy, Default)]
pub struct ValidateStepOptions<'a> {
    /// `allow_no_handler`: suppress the "has no handler" warning.
    pub allow_no_handler: bool,
    /// `_construct_template_registry`: the registry `validate_pipeline` forwards for
    /// `construct_template` steps.
    pub construct_template_registry: Option<&'a dyn ConstructTemplateRegistry>,
}

/// The result of `check_dependencies` (a dict in the original).
#[derive(Debug, Clone, PartialEq)]
pub struct DependencyCheck {
    /// Error messages (circular and missing dependencies).
    pub errors: Vec<String>,
    /// Warning messages (unreachable steps).
    pub warnings: Vec<String>,
    /// Each cycle found, as the path that closes it (`["A", "B", "A"]`).
    pub circular_dependencies: Vec<Vec<String>>,
    /// Steps reachable from the steps without dependencies (a set in the original; here in
    /// discovery order).
    pub reachable_steps: IndexSet<String>,
}

/// Pipeline validation engine.
///
/// • Pipeline validation and testing
/// • Workflow correctness checking
/// • Performance validation
/// • Error detection and reporting
#[derive(Debug, Clone)]
pub struct PipelineValidator {
    /// Configuration (`config` merged with the keyword arguments).
    pub config: Kwargs,
    /// The shared progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    #[allow(dead_code)]
    logger: Logger,
}

impl Default for PipelineValidator {
    fn default() -> Self {
        PipelineValidator::new(None, Kwargs::new())
    }
}

/// `repr()` of a set of names (`{'a', 'b'}`).
fn set_repr(items: &[&str]) -> String {
    let parts: Vec<String> = items.iter().map(|s| str_repr(s)).collect();
    format!("{{{}}}", parts.join(", "))
}

impl PipelineValidator {
    /// `PipelineValidator(config=None, **kwargs)`.
    pub fn new(config: Option<Kwargs>, kwargs: Kwargs) -> Self {
        PipelineValidator {
            config: crate::kwargs::init_config(config, kwargs),
            progress_tracker: enabled_progress_tracker(),
            logger: get_logger("pipeline_validator"),
        }
    }

    /// `validate(pipeline, **options)`: alias for [`PipelineValidator::validate_pipeline`].
    pub fn validate<'a>(
        &self,
        pipeline: impl Into<PipelineLike<'a>>,
        construct_template_registry: Option<&dyn ConstructTemplateRegistry>,
    ) -> ValidationResult {
        self.validate_pipeline(pipeline, construct_template_registry)
    }

    /// `validate_pipeline(pipeline, construct_template_registry=None, **options)`.
    ///
    /// A builder is checked for structure (no steps, duplicate names, missing name/type) and
    /// dependencies; a pipeline step by step ([`PipelineValidator::validate_step`], whose
    /// warnings are kept even for valid steps) and for dependencies. Other objects pass with
    /// `step_count` 0.
    pub fn validate_pipeline<'a>(
        &self,
        pipeline: impl Into<PipelineLike<'a>>,
        construct_template_registry: Option<&dyn ConstructTemplateRegistry>,
    ) -> ValidationResult {
        let pipeline = pipeline.into();
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("PipelineValidator"),
            "Validating pipeline",
            None,
        );
        let mut errors: Vec<String> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        match pipeline {
            PipelineLike::Builder(builder) => {
                tracker.update_tracking(&tracking_id, None, "Validating pipeline structure...");
                let (structure_errors, structure_warnings) = self.validate_structure(builder);
                errors.extend(structure_errors);
                warnings.extend(structure_warnings);

                tracker.update_tracking(&tracking_id, None, "Checking dependencies...");
                let dependency_result = self.check_dependencies(pipeline);
                errors.extend(dependency_result.errors);
                warnings.extend(dependency_result.warnings);
            }
            PipelineLike::Pipeline(p) => {
                tracker.update_tracking(
                    &tracking_id,
                    None,
                    &format!("Validating {} pipeline steps...", p.steps.len()),
                );
                let options = ValidateStepOptions {
                    allow_no_handler: false,
                    construct_template_registry,
                };
                for step in &p.steps {
                    let step_result = self.validate_step(step, &options);
                    if !step_result.valid {
                        errors.extend(step_result.errors);
                    }
                    warnings.extend(step_result.warnings);
                }

                tracker.update_tracking(&tracking_id, None, "Checking dependencies...");
                let dependency_result = self.check_dependencies(pipeline);
                errors.extend(dependency_result.errors);
                warnings.extend(dependency_result.warnings);
            }
            PipelineLike::Other(_) => {}
        }

        let mut metadata = Map::new();
        metadata.insert(
            "step_count".to_string(),
            Value::from(pipeline.steps().map_or(0, <[PipelineStep]>::len)),
        );
        let result = ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
            metadata,
        };
        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Validation complete: {} ({} errors, {} warnings)",
                if result.valid { "Valid" } else { "Invalid" },
                result.errors.len(),
                result.warnings.len()
            ),
            None,
        );
        result
    }

    /// `_validate_structure(builder)`: errors and warnings.
    fn validate_structure(&self, pipeline: &PipelineBuilder) -> (Vec<String>, Vec<String>) {
        let mut errors = Vec::new();
        let warnings = Vec::new();

        if pipeline.steps.is_empty() {
            errors.push("Pipeline has no steps".to_string());
        }

        // Counter(step_names): counts in first-seen order.
        let mut counts: IndexMap<&str, usize> = IndexMap::new();
        for step in &pipeline.steps {
            *counts.entry(step.name.as_str()).or_default() += 1;
        }
        let duplicates: Vec<&str> = counts
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(name, _)| *name)
            .collect();
        if !duplicates.is_empty() {
            errors.push(format!(
                "Duplicate step names found: {}",
                str_list_repr(&duplicates)
            ));
        }

        for step in &pipeline.steps {
            if step.name.is_empty() {
                errors.push("Step missing name".to_string());
            }
            if step.step_type.is_empty() {
                errors.push(format!("Step '{}' missing type", step.name));
            }
        }
        (errors, warnings)
    }

    /// `validate_step(step, **constraints)`.
    ///
    /// Errors: missing name/type; for `construct_template` steps with a registry, an
    /// unregistered `template_name` or missing required `params`. Warnings: no handler
    /// (unless `allow_no_handler`), no configuration, and `construct_template` steps without a
    /// registry.
    pub fn validate_step(
        &self,
        step: &PipelineStep,
        constraints: &ValidateStepOptions<'_>,
    ) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        if step.name.is_empty() {
            errors.push("Step missing name".to_string());
        }
        if step.step_type.is_empty() {
            errors.push("Step missing type".to_string());
        }

        if step.handler.is_none() && !constraints.allow_no_handler {
            warnings.push(format!("Step '{}' has no handler", step.name));
        }

        if step.config.is_empty() {
            warnings.push(format!("Step '{}' has no configuration", step.name));
        }

        if step.step_type == "construct_template" {
            match constraints.construct_template_registry {
                None => warnings.push(format!(
                    "Step '{}' (construct_template): no construct_template_registry provided \
                     — template existence and required parameters could not be checked.",
                    step.name
                )),
                Some(registry) => {
                    let template_name = step
                        .config
                        .get("template_name")
                        .cloned()
                        .unwrap_or_default();
                    let template = match &template_name {
                        KwargValue::Str(name) if !name.is_empty() => registry.get(name),
                        _ => None,
                    };
                    match template {
                        None => errors.push(format!(
                            "Step '{}' (construct_template): template_name {} is not registered \
                             in the provided construct_template_registry.",
                            step.name,
                            template_name.py_repr()
                        )),
                        Some(parameters) => {
                            let provided = step
                                .config
                                .get("params")
                                .filter(|p| p.truthy())
                                .cloned()
                                .unwrap_or_else(|| KwargValue::Dict(Kwargs::new()));
                            let missing: Vec<&str> = parameters
                                .iter()
                                .filter(|d| d.required && !contains(&provided, &d.name))
                                .map(|d| d.name.as_str())
                                .collect();
                            if !missing.is_empty() {
                                errors.push(format!(
                                    "Step '{}' (construct_template): missing required \
                                     parameter(s) for template {}: {}.",
                                    step.name,
                                    template_name.py_repr(),
                                    str_list_repr(&missing)
                                ));
                            }
                        }
                    }
                }
            }
        }

        ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
            metadata: Map::new(),
        }
    }

    /// `check_dependencies(pipeline)`: circular dependencies and dependencies on unknown steps
    /// are errors; steps not reachable from a step without dependencies are a warning.
    pub fn check_dependencies<'a>(&self, pipeline: impl Into<PipelineLike<'a>>) -> DependencyCheck {
        let pipeline = pipeline.into();
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("PipelineValidator"),
            "Checking pipeline dependencies",
            None,
        );
        let steps = pipeline.steps().unwrap_or(&[]);
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        // set(step.name for step in steps), iterated in step order here.
        let step_names: IndexSet<&str> = steps.iter().map(|s| s.name.as_str()).collect();

        tracker.update_tracking(&tracking_id, None, "Detecting circular dependencies...");
        let circular = detect_circular_dependencies(steps);
        if !circular.is_empty() {
            let cycles: Vec<String> = circular.iter().map(|c| str_list_repr(c)).collect();
            errors.push(format!(
                "Circular dependency detected: [{}]",
                cycles.join(", ")
            ));
        }

        tracker.update_tracking(&tracking_id, None, "Checking for missing dependencies...");
        for step in steps {
            for dep in &step.dependencies {
                if !step_names.contains(dep.as_str()) {
                    errors.push(format!(
                        "Missing dependency '{dep}' for step '{}'",
                        step.name
                    ));
                }
            }
        }

        tracker.update_tracking(&tracking_id, None, "Finding reachable steps...");
        let reachable = find_reachable_steps(steps);
        let unreachable: Vec<&str> = step_names
            .iter()
            .copied()
            .filter(|name| !reachable.contains(*name))
            .collect();
        if !unreachable.is_empty() {
            warnings.push(format!(
                "Unreachable steps found: {}",
                set_repr(&unreachable)
            ));
        }

        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!(
                "Dependency check complete: {} errors, {} warnings",
                errors.len(),
                warnings.len()
            ),
            None,
        );
        DependencyCheck {
            errors,
            warnings,
            circular_dependencies: circular,
            reachable_steps: reachable,
        }
    }

    /// `validate_performance(pipeline, **options)`: `step_count`, `sequential_steps` (steps
    /// with dependencies) and `warnings` (over 100 steps; every step sequential).
    ///
    /// Raises `AttributeError` for an object without steps.
    pub fn validate_performance<'a>(
        &self,
        pipeline: impl Into<PipelineLike<'a>>,
        _options: &Kwargs,
    ) -> Result<Map<String, Value>, SemanticaError> {
        let pipeline = pipeline.into();
        let tracker = &self.progress_tracker;
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("PipelineValidator"),
            "Validating pipeline performance",
            None,
        );
        let Some(steps) = pipeline.steps() else {
            let error = SemanticaError::Attribute(format!(
                "'{}' object has no attribute 'steps'",
                pipeline.type_name()
            ));
            tracker.stop_tracking(&tracking_id, Some("failed"), &error.to_string(), None);
            return Err(error);
        };
        let mut warnings = Vec::new();

        tracker.update_tracking(&tracking_id, None, "Analyzing pipeline structure...");
        let step_count = steps.len();
        if step_count > 100 {
            warnings.push(Value::from(
                "Pipeline has many steps, may impact performance",
            ));
        }

        let sequential_steps = steps.iter().filter(|s| !s.dependencies.is_empty()).count();
        if sequential_steps == step_count {
            warnings.push(Value::from(
                "All steps are sequential, consider parallelization",
            ));
        }

        let warning_count = warnings.len();
        let mut result = Map::new();
        result.insert("step_count".to_string(), Value::from(step_count));
        result.insert(
            "sequential_steps".to_string(),
            Value::from(sequential_steps),
        );
        result.insert("warnings".to_string(), Value::Array(warnings));

        tracker.stop_tracking(
            &tracking_id,
            Some("completed"),
            &format!("Performance validation complete: {warning_count} warnings"),
            None,
        );
        Ok(result)
    }
}

/// `name in provided_params` for a dict (keys), list (items) or string (substring).
fn contains(container: &KwargValue, name: &str) -> bool {
    match container {
        KwargValue::Dict(map) => map.contains_key(name),
        KwargValue::List(items) => items.iter().any(|item| item.as_str() == Some(name)),
        KwargValue::Str(s) => s.contains(name),
        _ => false,
    }
}

/// `_detect_circular_dependencies(steps)`: depth-first search from each unvisited step in
/// order; a dependency on a step still on the stack closes a cycle.
fn detect_circular_dependencies(steps: &[PipelineStep]) -> Vec<Vec<String>> {
    // {step.name: step}: a later step with the same name wins.
    let step_map: HashMap<&str, &PipelineStep> =
        steps.iter().map(|s| (s.name.as_str(), s)).collect();
    let mut cycles = Vec::new();
    let mut visited: HashSet<&str> = HashSet::new();
    let mut rec_stack: HashSet<&str> = HashSet::new();
    let mut path: Vec<&str> = Vec::new();

    fn dfs<'s>(
        node: &'s str,
        step_map: &HashMap<&'s str, &'s PipelineStep>,
        visited: &mut HashSet<&'s str>,
        rec_stack: &mut HashSet<&'s str>,
        path: &mut Vec<&'s str>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node);
        rec_stack.insert(node);
        path.push(node);

        if let Some(step) = step_map.get(node) {
            for dep in &step.dependencies {
                let Some((&dep_name, _)) = step_map.get_key_value(dep.as_str()) else {
                    continue;
                };
                if rec_stack.contains(dep_name) {
                    let start = path.iter().position(|n| *n == dep_name).unwrap_or(0);
                    let mut cycle: Vec<String> =
                        path[start..].iter().map(|s| s.to_string()).collect();
                    cycle.push(dep_name.to_string());
                    cycles.push(cycle);
                } else if !visited.contains(dep_name) {
                    dfs(dep_name, step_map, visited, rec_stack, path, cycles);
                }
            }
        }

        rec_stack.remove(node);
        path.pop();
    }

    for step in steps {
        if !visited.contains(step.name.as_str()) {
            dfs(
                step.name.as_str(),
                &step_map,
                &mut visited,
                &mut rec_stack,
                &mut path,
                &mut cycles,
            );
        }
    }
    cycles
}

/// `_find_reachable_steps(steps)`: breadth-first search from the steps without dependencies,
/// following "is a dependency of" edges.
fn find_reachable_steps(steps: &[PipelineStep]) -> IndexSet<String> {
    let known: HashSet<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    let mut reachable: IndexSet<String> = IndexSet::new();
    let mut queue: VecDeque<&str> = steps
        .iter()
        .filter(|s| s.dependencies.is_empty())
        .map(|s| s.name.as_str())
        .collect();
    while let Some(step_name) = queue.pop_front() {
        if reachable.contains(step_name) {
            continue;
        }
        reachable.insert(step_name.to_string());
        if known.contains(step_name) {
            for dependent in steps {
                if dependent.dependencies.iter().any(|d| d == step_name) {
                    queue.push_back(dependent.name.as_str());
                }
            }
        }
    }
    reachable
}
