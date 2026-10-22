//! Pre-built pipeline templates (`semantica/pipeline/pipeline_templates.py`).

use std::sync::Arc;

use indexmap::IndexMap;
use sem_utils::logging::Logger;
use sem_utils::pycompat::fmt::str_repr;
use sem_utils::{ProgressTracker, SemanticaError, ValidationError, get_logger};
use serde_json::json;

use crate::enabled_progress_tracker;
use crate::kwargs::{
    KwargValue, Kwargs, init_config, kwargs_from_json, kwargs_repr, merge_call_kwargs, split_config,
};
use crate::pipeline_builder::{ADD_STEP_FUNC, PipelineBuilder, not_a_mapping};

/// Pipeline template definition (a dataclass in the original).
#[derive(Clone, PartialEq)]
pub struct PipelineTemplate {
    /// Template name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Step dicts: `name`, `type`, optional `config` and `dependencies`.
    pub steps: Vec<Kwargs>,
    /// Pipeline configuration (`parallelism`).
    pub config: Kwargs,
    /// Metadata (`category`).
    pub metadata: Kwargs,
}

impl std::fmt::Debug for PipelineTemplate {
    /// The dataclass `repr`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let steps: Vec<String> = self.steps.iter().map(kwargs_repr).collect();
        write!(
            f,
            "PipelineTemplate(name={}, description={}, steps=[{}], config={}, metadata={})",
            str_repr(&self.name),
            str_repr(&self.description),
            steps.join(", "),
            kwargs_repr(&self.config),
            kwargs_repr(&self.metadata)
        )
    }
}

impl PipelineTemplate {
    /// `PipelineTemplate(name, description)` with the dataclass defaults.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        PipelineTemplate {
            name: name.into(),
            description: description.into(),
            steps: Vec::new(),
            config: Kwargs::new(),
            metadata: Kwargs::new(),
        }
    }
}

/// `dict.update(other)` for a non-dict `other`.
fn update_error(value: &KwargValue) -> SemanticaError {
    SemanticaError::Type(format!("'{}' object is not iterable", value.type_name()))
}

/// `value.update(...)` where `value` is not a dict.
fn no_update(value: &KwargValue) -> SemanticaError {
    SemanticaError::Attribute(format!(
        "'{}' object has no attribute 'update'",
        value.type_name()
    ))
}

/// Pipeline template management system.
///
/// • Pre-built pipeline templates
/// • Template customization and configuration
/// • Template registration and management
#[derive(Debug)]
pub struct PipelineTemplateManager {
    /// Configuration (`config` merged with the keyword arguments), passed to the builders.
    pub config: Kwargs,
    /// The shared progress tracker.
    pub progress_tracker: Arc<ProgressTracker>,
    /// Templates by name: `document_processing`, `rag_pipeline`, `kg_construction`,
    /// `ontology_generation`, then registered ones.
    pub templates: IndexMap<String, PipelineTemplate>,
    logger: Logger,
}

impl Default for PipelineTemplateManager {
    fn default() -> Self {
        PipelineTemplateManager::new(None, Kwargs::new())
    }
}

fn template(
    name: &str,
    description: &str,
    steps: serde_json::Value,
    config: serde_json::Value,
    metadata: serde_json::Value,
) -> PipelineTemplate {
    let steps = match KwargValue::from_json(steps) {
        KwargValue::List(items) => items
            .into_iter()
            .filter_map(|item| match item {
                KwargValue::Dict(map) => Some(map),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    PipelineTemplate {
        name: name.to_string(),
        description: description.to_string(),
        steps,
        config: kwargs_from_json(config),
        metadata: kwargs_from_json(metadata),
    }
}

impl PipelineTemplateManager {
    /// `PipelineTemplateManager(config=None, **kwargs)`: with the default templates loaded.
    pub fn new(config: Option<Kwargs>, kwargs: Kwargs) -> Self {
        let mut manager = PipelineTemplateManager {
            config: init_config(config, kwargs),
            progress_tracker: enabled_progress_tracker(),
            templates: IndexMap::new(),
            logger: get_logger("pipeline_template_manager"),
        };
        manager.load_default_templates();
        manager
    }

    /// `_load_default_templates()`.
    fn load_default_templates(&mut self) {
        self.templates.insert(
            "document_processing".into(),
            template(
                "document_processing",
                "Complete document processing pipeline from ingestion to knowledge graph",
                json!([
                    {"name": "ingest", "type": "ingest", "config": {"source": "documents/"}},
                    {
                        "name": "parse",
                        "type": "parse",
                        "config": {"formats": ["pdf", "docx"]},
                        "dependencies": ["ingest"],
                    },
                    {
                        "name": "normalize",
                        "type": "normalize",
                        "config": {},
                        "dependencies": ["parse"],
                    },
                    {
                        "name": "extract",
                        "type": "extract",
                        "config": {"entities": true, "relations": true},
                        "dependencies": ["normalize"],
                    },
                    {
                        "name": "embed",
                        "type": "embed",
                        "config": {"model": "text-embedding-3-large"},
                        "dependencies": ["extract"],
                    },
                    {
                        "name": "build_kg",
                        "type": "build_kg",
                        "config": {},
                        "dependencies": ["extract", "embed"],
                    },
                ]),
                json!({"parallelism": 2}),
                json!({"category": "document_processing"}),
            ),
        );

        self.templates.insert(
            "rag_pipeline".into(),
            template(
                "rag_pipeline",
                "RAG pipeline for question answering",
                json!([
                    {"name": "ingest", "type": "ingest", "config": {"source": "documents/"}},
                    {
                        "name": "chunk",
                        "type": "chunk",
                        "config": {"chunk_size": 512},
                        "dependencies": ["ingest"],
                    },
                    {
                        "name": "embed",
                        "type": "embed",
                        "config": {},
                        "dependencies": ["chunk"],
                    },
                    {
                        "name": "store_vectors",
                        "type": "store_vectors",
                        "config": {"store": "weaviate"},
                        "dependencies": ["embed"],
                    },
                ]),
                json!({"parallelism": 4}),
                json!({"category": "rag"}),
            ),
        );

        self.templates.insert(
            "kg_construction".into(),
            template(
                "kg_construction",
                "Knowledge graph construction from multiple sources",
                json!([
                    {"name": "ingest_sources", "type": "ingest", "config": {"sources": []}},
                    {
                        "name": "extract_entities",
                        "type": "extract",
                        "config": {"entities": true},
                        "dependencies": ["ingest_sources"],
                    },
                    {
                        "name": "extract_relations",
                        "type": "extract",
                        "config": {"relations": true},
                        "dependencies": ["ingest_sources"],
                    },
                    {
                        "name": "deduplicate",
                        "type": "deduplicate",
                        "config": {},
                        "dependencies": ["extract_entities"],
                    },
                    {
                        "name": "resolve_conflicts",
                        "type": "resolve_conflicts",
                        "config": {},
                        "dependencies": ["extract_entities", "extract_relations"],
                    },
                    {
                        "name": "build_graph",
                        "type": "build_kg",
                        "config": {},
                        "dependencies": ["deduplicate", "resolve_conflicts"],
                    },
                ]),
                json!({"parallelism": 3}),
                json!({"category": "knowledge_graph"}),
            ),
        );

        self.templates.insert(
            "ontology_generation".into(),
            template(
                "ontology_generation",
                "Ontology generation from extracted data",
                json!([
                    {
                        "name": "extract_concepts",
                        "type": "extract",
                        "config": {"entities": true},
                    },
                    {
                        "name": "infer_classes",
                        "type": "infer_classes",
                        "config": {},
                        "dependencies": ["extract_concepts"],
                    },
                    {
                        "name": "infer_properties",
                        "type": "infer_properties",
                        "config": {},
                        "dependencies": ["infer_classes"],
                    },
                    {
                        "name": "generate_owl",
                        "type": "generate_owl",
                        "config": {},
                        "dependencies": ["infer_classes", "infer_properties"],
                    },
                    {
                        "name": "validate_ontology",
                        "type": "validate_ontology",
                        "config": {},
                        "dependencies": ["generate_owl"],
                    },
                ]),
                json!({"parallelism": 1}),
                json!({"category": "ontology"}),
            ),
        );
    }

    /// `get_template(template_name)`.
    pub fn get_template(&self, template_name: &str) -> Option<&PipelineTemplate> {
        self.templates.get(template_name)
    }

    /// `create_pipeline_from_template(template_name, **overrides)`: a builder with the
    /// template's steps and parallelism.
    ///
    /// `overrides[<step name>]` is merged into that step's configuration — in place, as in
    /// the original, so the template keeps the override — and
    /// `overrides["pipeline_config"]` into the pipeline configuration. Raises
    /// `ValidationError: Template not found: <name>`.
    pub fn create_pipeline_from_template(
        &mut self,
        template_name: &str,
        overrides: &Kwargs,
    ) -> Result<PipelineBuilder, SemanticaError> {
        let tracker = Arc::clone(&self.progress_tracker);
        let tracking_id = tracker.start_tracking(
            None,
            Some("pipeline"),
            Some("PipelineTemplateManager"),
            &format!("Creating pipeline from template: {template_name}"),
            None,
        );
        let result: Result<PipelineBuilder, SemanticaError> = (|| {
            tracker.update_tracking(&tracking_id, None, "Getting template...");
            let builder_config = self.config.clone();
            let Some(template) = self.templates.get_mut(template_name) else {
                return Err(SemanticaError::from(ValidationError::new(format!(
                    "Template not found: {template_name}"
                ))));
            };

            let (config, kwargs) = split_config(&builder_config);
            let mut builder = PipelineBuilder::new(config, kwargs);

            tracker.update_tracking(
                &tracking_id,
                None,
                &format!("Adding {} steps from template...", template.steps.len()),
            );
            for step_config in template.steps.iter_mut() {
                let step_name = step_config
                    .get("name")
                    .ok_or_else(|| SemanticaError::Key("name".into()))?
                    .py_str();
                let step_type = step_config
                    .get("type")
                    .ok_or_else(|| SemanticaError::Key("type".into()))?
                    .py_str();
                let dependencies = step_config
                    .get("dependencies")
                    .cloned()
                    .unwrap_or_else(|| KwargValue::List(Vec::new()));

                let mut fresh_config = KwargValue::Dict(Kwargs::new());
                let config = step_config.get_mut("config").unwrap_or(&mut fresh_config);
                if let Some(step_override) = overrides.get(&step_name) {
                    let Some(map) = config.as_dict_mut() else {
                        return Err(no_update(config));
                    };
                    let KwargValue::Dict(update) = step_override else {
                        return Err(update_error(step_override));
                    };
                    for (key, value) in update {
                        map.insert(key.clone(), value.clone());
                    }
                }
                let KwargValue::Dict(config) = &*config else {
                    return Err(not_a_mapping(ADD_STEP_FUNC, config));
                };

                let mut explicit = Kwargs::new();
                explicit.insert("dependencies".into(), dependencies);
                let step_kwargs = merge_call_kwargs(ADD_STEP_FUNC, &[&explicit, config])?;
                builder.add_step(&step_name, &step_type, step_kwargs);
            }

            tracker.update_tracking(&tracking_id, None, "Setting pipeline configuration...");
            let mut pipeline_config = template.config.clone();
            match overrides.get("pipeline_config") {
                None => {}
                Some(KwargValue::Dict(update)) => {
                    for (key, value) in update {
                        pipeline_config.insert(key.clone(), value.clone());
                    }
                }
                Some(other) => return Err(update_error(other)),
            }
            for (key, value) in &pipeline_config {
                if key == "parallelism" {
                    builder.set_parallelism(value.clone());
                }
            }

            tracker.stop_tracking(
                &tracking_id,
                Some("completed"),
                &format!(
                    "Created pipeline from template: {template_name} with {} steps",
                    template.steps.len()
                ),
                None,
            );
            Ok(builder)
        })();
        if let Err(e) = &result {
            tracker.stop_tracking(&tracking_id, Some("failed"), &e.to_string(), None);
        }
        result
    }

    /// `register_template(template)`: add or replace a template under its name.
    pub fn register_template(&mut self, template: PipelineTemplate) {
        let name = template.name.clone();
        self.templates.insert(name.clone(), template);
        self.logger.info(format!("Registered template: {name}"));
    }

    /// `list_templates(category=None)`: template names, optionally those whose
    /// `metadata["category"]` equals `category`.
    pub fn list_templates(&self, category: Option<&str>) -> Vec<String> {
        match category.filter(|c| !c.is_empty()) {
            Some(category) => self
                .templates
                .iter()
                .filter(|(_, t)| {
                    t.metadata
                        .get("category")
                        .is_some_and(|c| c.as_str() == Some(category))
                })
                .map(|(name, _)| name.clone())
                .collect(),
            None => self.templates.keys().cloned().collect(),
        }
    }

    /// `get_template_info(template_name)`: `name`, `description`, `step_count`, `config` and
    /// `metadata`.
    pub fn get_template_info(&self, template_name: &str) -> Option<Kwargs> {
        let template = self.get_template(template_name)?;
        let mut info = Kwargs::new();
        info.insert("name".into(), KwargValue::Str(template.name.clone()));
        info.insert(
            "description".into(),
            KwargValue::Str(template.description.clone()),
        );
        info.insert("step_count".into(), KwargValue::from(template.steps.len()));
        info.insert("config".into(), KwargValue::Dict(template.config.clone()));
        info.insert(
            "metadata".into(),
            KwargValue::Dict(template.metadata.clone()),
        );
        Some(info)
    }
}
