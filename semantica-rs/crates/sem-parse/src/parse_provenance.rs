//! Provenance-enabled parsing wrapper (`semantica/parse/parse_provenance.py`).
//!
//! [`ParserWithProvenance`] wraps a parser and records each parse as a `parsed_data` entity
//! in a [`ProvenanceManager`] (agent, automation flag, activity start/end times, the file
//! path and the `format` keyword).
//!
//! The original's constructor imports `Parser` from `semantica.parse.parser`, a module that
//! does not exist in the package, so `ParserWithProvenance(...)` always raises
//! `ModuleNotFoundError: No module named 'semantica.parse.parser'`;
//! [`ParserWithProvenance::new`] keeps that. [`ParserWithProvenance::with_parser`] supplies the
//! wrapped parser explicitly (the role the missing `Parser(**config)` would play), so the
//! tracking behaviour of `parse` is available.

use std::fmt;
use std::ops::Deref;

use serde_json::{Map, Value};

use sem_provenance::{ProvenanceManager, TrackEntityOptions};
use sem_utils::SemanticaError;

use crate::common::{Options, PResult, Source};
use crate::methods::ParseOutput;

/// The parser a [`ParserWithProvenance`] wraps (`Parser(**config)` in the original):
/// anything with `parse(file_path, **kwargs)`.
pub trait ProvenanceParser: Send + Sync {
    /// `parser.parse(file_path, **kwargs)`.
    fn parse(&self, file_path: &str, kwargs: &Options) -> PResult<ParseOutput>;
}

impl ProvenanceParser for crate::DocumentParser {
    fn parse(&self, file_path: &str, kwargs: &Options) -> PResult<ParseOutput> {
        crate::DocumentParser::parse(self, Source::Text(file_path.to_string()), kwargs)
    }
}

impl ProvenanceParser for crate::StructuredDataParser {
    fn parse(&self, file_path: &str, kwargs: &Options) -> PResult<ParseOutput> {
        let (data_format, rest) = crate::common::bind_kwarg(kwargs, "data_format");
        let data_format = data_format.and_then(|v| v.as_str().map(str::to_string));
        self.parse_data(file_path, data_format.as_deref(), &rest)
            .map(ParseOutput::Dict)
    }
}

/// `ParserWithProvenance(provenance=False, agent_id=None, is_automated=True, **config)`: base
/// parser with provenance tracking.
pub struct ParserWithProvenance {
    /// `self.provenance`.
    pub provenance: bool,
    parser: Box<dyn ProvenanceParser>,
    prov_manager: Option<ProvenanceManager>,
    agent_id: String,
    is_automated: bool,
}

impl fmt::Debug for ParserWithProvenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParserWithProvenance")
            .field("provenance", &self.provenance)
            .field("agent_id", &self.agent_id)
            .field("is_automated", &self.is_automated)
            .field("has_prov_manager", &self.prov_manager.is_some())
            .finish()
    }
}

impl ParserWithProvenance {
    /// `ParserWithProvenance(provenance, agent_id, is_automated, **config)`: as in the
    /// original, fails with `ModuleNotFoundError` because `semantica.parse.parser` (the
    /// wrapped `Parser` class) does not exist.
    pub fn new(
        _provenance: Option<bool>,
        _agent_id: Option<&str>,
        _is_automated: Option<bool>,
        _config: Options,
    ) -> PResult<Self> {
        Err(SemanticaError::Other {
            module: "builtins".to_string(),
            type_name: "ModuleNotFoundError".to_string(),
            message: "No module named 'semantica.parse.parser'".to_string(),
        })
    }

    /// The constructor with the wrapped parser supplied: `provenance` (default `False`)
    /// creates a [`ProvenanceManager::new`] (whose errors propagate, as Python's would),
    /// `agent_id` defaults to the class name and `is_automated` to `True`.
    pub fn with_parser(
        parser: Box<dyn ProvenanceParser>,
        provenance: Option<bool>,
        agent_id: Option<&str>,
        is_automated: Option<bool>,
    ) -> PResult<Self> {
        let provenance = provenance.unwrap_or(false);
        let prov_manager = if provenance {
            Some(ProvenanceManager::new()?)
        } else {
            None
        };
        Ok(Self::assemble(parser, provenance, prov_manager, agent_id, is_automated))
    }

    /// [`Self::with_parser`] recording into an existing manager (the original creates its
    /// own; this lets callers choose the storage).
    pub fn with_manager(
        parser: Box<dyn ProvenanceParser>,
        manager: ProvenanceManager,
        agent_id: Option<&str>,
        is_automated: Option<bool>,
    ) -> Self {
        Self::assemble(parser, true, Some(manager), agent_id, is_automated)
    }

    fn assemble(
        parser: Box<dyn ProvenanceParser>,
        provenance: bool,
        prov_manager: Option<ProvenanceManager>,
        agent_id: Option<&str>,
        is_automated: Option<bool>,
    ) -> Self {
        ParserWithProvenance {
            provenance,
            parser,
            prov_manager,
            // `agent_id or self.__class__.__name__`
            agent_id: agent_id
                .filter(|a| !a.is_empty())
                .unwrap_or("ParserWithProvenance")
                .to_string(),
            is_automated: is_automated.unwrap_or(true),
        }
    }

    /// `self._agent_id`.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// `self._is_automated`.
    pub fn is_automated(&self) -> bool {
        self.is_automated
    }

    /// `self._prov_manager`.
    pub fn prov_manager(&self) -> Option<&ProvenanceManager> {
        self.prov_manager.as_ref()
    }

    /// `parse(file_path, **kwargs)`: parse with the wrapped parser and, when provenance is
    /// on, track a `parsed_data` entity `parse_<8 hex>` sourced from `file_path`
    /// (`software_agent`, activity times from `datetime.utcnow().isoformat()`, metadata
    /// `{"file_path": ..., "format": kwargs.get("format", "unknown")}`).
    pub fn parse(&self, file_path: &str, kwargs: &Options) -> PResult<ParseOutput> {
        let activity_started_at_time = sem_provenance::clock::utcnow_isoformat();
        let data = self.parser.parse(file_path, kwargs)?;
        let activity_ended_at_time = sem_provenance::clock::utcnow_isoformat();
        if self.provenance
            && let Some(manager) = &self.prov_manager
        {
            let simple = uuid::Uuid::new_v4().simple().to_string();
            let entity_id = format!("parse_{}", &simple[..8]);
            let mut metadata = Map::new();
            metadata.insert("file_path".to_string(), Value::String(file_path.to_string()));
            metadata.insert(
                "format".to_string(),
                kwargs
                    .get("format")
                    .cloned()
                    .unwrap_or_else(|| Value::String("unknown".to_string())),
            );
            manager.track_entity(
                &entity_id,
                file_path,
                Some(metadata),
                TrackEntityOptions::new()
                    .entity_type("parsed_data")
                    .agent_id(self.agent_id.clone())
                    .agent_type("software_agent")
                    .is_automated(self.is_automated)
                    .activity_started_at_time(activity_started_at_time)
                    .activity_ended_at_time(activity_ended_at_time),
            );
        }
        Ok(data)
    }
}

impl Deref for ParserWithProvenance {
    type Target = dyn ProvenanceParser;

    /// `__getattr__`: other attributes come from the wrapped parser.
    fn deref(&self) -> &Self::Target {
        self.parser.as_ref()
    }
}
