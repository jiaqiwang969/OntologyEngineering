//! Provenance-enabled pipeline execution (`semantica/pipeline/pipeline_provenance.py`):
//! runs a pipeline with the [`ExecutionEngine`] and records each run as a `pipeline_run`
//! entity in a [`ProvenanceManager`].

use std::borrow::BorrowMut;
use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;

use sem_provenance::{ProvenanceManager, TrackEntityOptions};
use sem_utils::SemanticaError;
use sem_utils::pycompat::time::PyDateTime;
use serde_json::{Map, Value};

use crate::execution_engine::{ExecutionEngine, ExecutionResult};
use crate::failure_handler::is_instance;
use crate::kwargs::{Kwargs, split_config};
use crate::pipeline_builder::Pipeline;
use crate::time_now;

/// `__all__` of `semantica.pipeline.pipeline_provenance`.
pub const ALL: [&str; 1] = ["PipelineWithProvenance"];

thread_local! {
    static IMPORT_BLOCKED: Cell<usize> = const { Cell::new(0) };
}

/// While alive, importing `semantica.provenance` fails on this thread with Python's
/// `ModuleNotFoundError` — the port of `patch.dict(sys.modules, {"semantica.provenance":
/// None})`. Returned by [`block_provenance_import`].
#[must_use = "the import is unblocked when the guard is dropped"]
pub struct ProvenanceImportBlock {
    _not_send: PhantomData<*const ()>,
}

impl Drop for ProvenanceImportBlock {
    fn drop(&mut self) {
        IMPORT_BLOCKED.with(|blocked| blocked.set(blocked.get().saturating_sub(1)));
    }
}

/// Make `from semantica.provenance import ProvenanceManager` fail on this thread until the
/// returned guard is dropped.
pub fn block_provenance_import() -> ProvenanceImportBlock {
    IMPORT_BLOCKED.with(|blocked| blocked.set(blocked.get() + 1));
    ProvenanceImportBlock {
        _not_send: PhantomData,
    }
}

/// `from semantica.provenance import ProvenanceManager; ProvenanceManager()`.
fn import_provenance_manager() -> Result<ProvenanceManager, SemanticaError> {
    if IMPORT_BLOCKED.with(Cell::get) > 0 {
        return Err(SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "ModuleNotFoundError".to_string(),
            message: "import of semantica.provenance halted; None in sys.modules".to_string(),
        });
    }
    ProvenanceManager::new()
}

/// The keyword arguments of `PipelineWithProvenance(pipeline, provenance=False,
/// agent_id=None, is_automated=True, **engine_config)`.
#[derive(Debug, Clone)]
pub struct PipelineWithProvenanceOptions {
    /// Whether to record provenance.
    pub provenance: bool,
    /// The agent running the pipeline (default: the class name).
    pub agent_id: Option<String>,
    /// Whether the execution is automated rather than human-triggered.
    pub is_automated: bool,
    /// Keyword arguments for the [`ExecutionEngine`].
    pub engine_config: Kwargs,
}

impl Default for PipelineWithProvenanceOptions {
    fn default() -> Self {
        PipelineWithProvenanceOptions {
            provenance: false,
            agent_id: None,
            is_automated: true,
            engine_config: Kwargs::new(),
        }
    }
}

/// Pipeline executor with complete provenance tracking.
///
/// Wraps a built pipeline (owned, or borrowed as `&mut Pipeline`); attribute access falls
/// through to it (`Deref<Target = Pipeline>`, the original's `__getattr__`).
pub struct PipelineWithProvenance<P: BorrowMut<Pipeline> = Pipeline> {
    pipeline: P,
    engine: ExecutionEngine,
    /// Whether runs are recorded (turned off when the provenance package cannot be
    /// imported).
    pub provenance: bool,
    prov_manager: Option<ProvenanceManager>,
    agent_id: String,
    is_automated: bool,
}

impl<P: BorrowMut<Pipeline>> fmt::Debug for PipelineWithProvenance<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipelineWithProvenance")
            .field("pipeline", self.pipeline.borrow())
            .field("provenance", &self.provenance)
            .field("has_prov_manager", &self.prov_manager.is_some())
            .field("agent_id", &self.agent_id)
            .field("is_automated", &self.is_automated)
            .finish()
    }
}

impl<P: BorrowMut<Pipeline>> PipelineWithProvenance<P> {
    /// `PipelineWithProvenance(pipeline, provenance=provenance)`.
    pub fn new(pipeline: P, provenance: bool) -> Result<Self, SemanticaError> {
        Self::with_options(
            pipeline,
            PipelineWithProvenanceOptions {
                provenance,
                ..PipelineWithProvenanceOptions::default()
            },
        )
    }

    /// `PipelineWithProvenance(pipeline, provenance, agent_id, is_automated,
    /// **engine_config)`.
    ///
    /// With `provenance`, a [`ProvenanceManager`] is created; an `ImportError` while doing so
    /// turns provenance off, any other error is returned.
    pub fn with_options(
        pipeline: P,
        options: PipelineWithProvenanceOptions,
    ) -> Result<Self, SemanticaError> {
        let (config, kwargs) = split_config(&options.engine_config);
        let engine = ExecutionEngine::new(config, kwargs);
        let mut provenance = options.provenance;
        let mut prov_manager = None;
        if provenance {
            match import_provenance_manager() {
                Ok(manager) => prov_manager = Some(manager),
                Err(e) if is_instance(&e, "ImportError") => provenance = false,
                Err(e) => return Err(e),
            }
        }
        Ok(PipelineWithProvenance {
            pipeline,
            engine,
            provenance,
            prov_manager,
            agent_id: options
                .agent_id
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| "PipelineWithProvenance".to_string()),
            is_automated: options.is_automated,
        })
    }

    /// `run(data=None, source=None, **kwargs)`: execute the pipeline (`kwargs` are the
    /// engine's execution options) and, with provenance on, track the run as entity
    /// `pipeline_<8 hex>` of type `pipeline_run` from `source` (default
    /// `"pipeline_execution"`), with the step count, duration and status as metadata.
    ///
    pub fn run(&mut self, data: Value, source: Option<&str>, kwargs: &Kwargs) -> ExecutionResult {
        let hex = uuid::Uuid::new_v4().simple().to_string();
        let pipeline_id = format!("pipeline_{}", &hex[..8]);
        let start_time = time_now();
        let activity_started_at_time = PyDateTime::now(true).isoformat('T');

        let result = self
            .engine
            .execute_pipeline(self.pipeline.borrow_mut(), data, kwargs);
        let elapsed = time_now() - start_time;
        let activity_ended_at_time = PyDateTime::now(true).isoformat('T');

        if self.provenance
            && let Some(manager) = &self.prov_manager
        {
            let mut metadata = Map::new();
            metadata.insert(
                "steps".into(),
                Value::from(self.pipeline.borrow().steps.len()),
            );
            metadata.insert("duration_seconds".into(), Value::from(elapsed));
            metadata.insert(
                "status".into(),
                Value::from(if result.success {
                    "completed"
                } else {
                    "failed"
                }),
            );
            manager.track_entity(
                &pipeline_id,
                source
                    .filter(|s| !s.is_empty())
                    .unwrap_or("pipeline_execution"),
                Some(metadata),
                TrackEntityOptions::new()
                    .entity_type("pipeline_run")
                    .agent_id(self.agent_id.clone())
                    .agent_type("software_agent")
                    .is_automated(self.is_automated)
                    .activity_id(pipeline_id.clone())
                    .activity_started_at_time(activity_started_at_time)
                    .activity_ended_at_time(activity_ended_at_time),
            );
        }
        result
    }

    /// The wrapped pipeline (`_pipeline`).
    pub fn pipeline(&self) -> &Pipeline {
        self.pipeline.borrow()
    }

    /// The execution engine (`_engine`).
    pub fn engine(&self) -> &ExecutionEngine {
        &self.engine
    }

    /// The provenance manager (`_prov_manager`), when provenance is on.
    pub fn prov_manager(&self) -> Option<&ProvenanceManager> {
        self.prov_manager.as_ref()
    }

    /// The agent recorded for runs (`_agent_id`).
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Whether runs are recorded as automated (`_is_automated`).
    pub fn is_automated(&self) -> bool {
        self.is_automated
    }

    /// Give back the wrapped pipeline.
    pub fn into_pipeline(self) -> P {
        self.pipeline
    }
}

impl<P: BorrowMut<Pipeline>> Deref for PipelineWithProvenance<P> {
    type Target = Pipeline;

    /// `__getattr__`: attributes the wrapper lacks come from the pipeline.
    fn deref(&self) -> &Pipeline {
        self.pipeline.borrow()
    }
}
