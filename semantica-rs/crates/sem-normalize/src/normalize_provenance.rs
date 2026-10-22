//! Provenance-enabled wrapper for normalization (`semantica/normalize/normalize_provenance.py`).
//!
//! `NormalizerWithProvenance` runs a normalizer and records each run with
//! [`sem_provenance::ProvenanceManager`]. The original builds its normalizer from
//! `semantica.normalize.normalizer.Normalizer`, a module that was never shipped, so its
//! constructor always fails with `ModuleNotFoundError`; [`NormalizerWithProvenance::new`]
//! reproduces that. [`NormalizerWithProvenance::with_normalizer`] (and
//! [`NormalizerWithProvenance::with_manager`]) take the normalizer instead, behind the
//! [`Normalize`] trait.

use sem_provenance::{ProvenanceManager, TrackEntityOptions};
use sem_utils::SemanticaError;
use serde_json::{Map, Value, json};

use crate::text_normalizer::{NormalizeTextOptions, TextNormalizer};

/// The wrapped normalizer: `normalize(data, **kwargs)`.
pub trait Normalize: Send + Sync {
    /// Normalize `data` with the call's keyword arguments.
    fn normalize(&self, data: &Value, kwargs: &Map<String, Value>)
    -> Result<Value, SemanticaError>;
}

/// `TextNormalizer.normalize(source, **options)` as a wrapped normalizer (a port addition: the
/// original's `Normalizer` class does not exist).
impl Normalize for TextNormalizer {
    fn normalize(
        &self,
        data: &Value,
        kwargs: &Map<String, Value>,
    ) -> Result<Value, SemanticaError> {
        TextNormalizer::normalize(self, data, &NormalizeTextOptions::from_kwargs(kwargs))
    }
}

/// Normalizer with provenance tracking (`NormalizerWithProvenance(provenance=False,
/// agent_id=None, is_automated=True, **config)`).
pub struct NormalizerWithProvenance {
    /// `provenance`: whether runs are tracked.
    pub provenance: bool,
    normalizer: Box<dyn Normalize>,
    prov_manager: Option<ProvenanceManager>,
    agent_id: String,
    is_automated: bool,
}

impl std::fmt::Debug for NormalizerWithProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NormalizerWithProvenance")
            .field("provenance", &self.provenance)
            .field("agent_id", &self.agent_id)
            .field("is_automated", &self.is_automated)
            .field("prov_manager", &self.prov_manager.is_some())
            .finish()
    }
}

/// The agent recorded when none is given (`self.__class__.__name__`).
const CLASS_NAME: &str = "NormalizerWithProvenance";

impl NormalizerWithProvenance {
    /// `NormalizerWithProvenance(provenance, agent_id, is_automated, **config)`: like the
    /// original, this fails with `ModuleNotFoundError` — it imports
    /// `semantica.normalize.normalizer`, which does not exist. Use
    /// [`Self::with_normalizer`] to supply the normalizer.
    pub fn new(
        provenance: bool,
        agent_id: Option<&str>,
        is_automated: Option<bool>,
        config: Map<String, Value>,
    ) -> Result<NormalizerWithProvenance, SemanticaError> {
        let _ = (provenance, agent_id, is_automated, config);
        Err(SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "ModuleNotFoundError".to_string(),
            message: "No module named 'semantica.normalize.normalizer'".to_string(),
        })
    }

    /// The constructor with the normalizer supplied: with `provenance`, a
    /// `ProvenanceManager()` (in-memory, or SQLite at the global default path) records the runs.
    pub fn with_normalizer(
        normalizer: Box<dyn Normalize>,
        provenance: bool,
        agent_id: Option<&str>,
        is_automated: Option<bool>,
    ) -> Result<NormalizerWithProvenance, SemanticaError> {
        let prov_manager = if provenance {
            Some(ProvenanceManager::new()?)
        } else {
            None
        };
        Ok(Self::build(
            normalizer,
            provenance,
            prov_manager,
            agent_id,
            is_automated,
        ))
    }

    /// Tracking enabled, with the given provenance manager.
    pub fn with_manager(
        normalizer: Box<dyn Normalize>,
        manager: ProvenanceManager,
        agent_id: Option<&str>,
        is_automated: Option<bool>,
    ) -> NormalizerWithProvenance {
        Self::build(normalizer, true, Some(manager), agent_id, is_automated)
    }

    fn build(
        normalizer: Box<dyn Normalize>,
        provenance: bool,
        prov_manager: Option<ProvenanceManager>,
        agent_id: Option<&str>,
        is_automated: Option<bool>,
    ) -> NormalizerWithProvenance {
        NormalizerWithProvenance {
            provenance,
            normalizer,
            prov_manager,
            agent_id: agent_id
                .filter(|a| !a.is_empty())
                .unwrap_or(CLASS_NAME)
                .to_string(),
            is_automated: is_automated.unwrap_or(true),
        }
    }

    /// `normalize(data, source=None, **kwargs)`: run the normalizer and, when tracking, record a
    /// `normalized_data` entity (`normalize_<8 hex>`) from `source` (default
    /// `"normalization"`) with the agent, the activity's start/end times and
    /// `{"method": kwargs["method"] or "default"}`. Normalizer errors propagate (nothing is
    /// tracked); tracking itself never fails.
    pub fn normalize(
        &self,
        data: &Value,
        source: Option<&str>,
        kwargs: &Map<String, Value>,
    ) -> Result<Value, SemanticaError> {
        let activity_started_at_time = sem_provenance::clock::utcnow_isoformat();
        let result = self.normalizer.normalize(data, kwargs)?;
        let activity_ended_at_time = sem_provenance::clock::utcnow_isoformat();

        if self.provenance
            && let Some(manager) = &self.prov_manager
        {
            let simple = uuid::Uuid::new_v4().simple().to_string();
            let entity_id = format!("normalize_{}", &simple[..8]);
            let mut metadata = Map::new();
            metadata.insert(
                "method".to_string(),
                kwargs
                    .get("method")
                    .cloned()
                    .unwrap_or_else(|| json!("default")),
            );
            manager.track_entity(
                &entity_id,
                source.filter(|s| !s.is_empty()).unwrap_or("normalization"),
                Some(metadata),
                TrackEntityOptions::new()
                    .entity_type("normalized_data")
                    .agent_id(self.agent_id.as_str())
                    .agent_type("software_agent")
                    .is_automated(self.is_automated)
                    .activity_started_at_time(activity_started_at_time)
                    .activity_ended_at_time(activity_ended_at_time),
            );
        }
        Ok(result)
    }

    /// The wrapped normalizer (the original forwards unknown attributes to it with
    /// `__getattr__`).
    pub fn normalizer(&self) -> &dyn Normalize {
        &*self.normalizer
    }

    /// `_prov_manager`.
    pub fn prov_manager(&self) -> Option<&ProvenanceManager> {
        self.prov_manager.as_ref()
    }

    /// `_agent_id`.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// `_is_automated`.
    pub fn is_automated(&self) -> bool {
        self.is_automated
    }
}
