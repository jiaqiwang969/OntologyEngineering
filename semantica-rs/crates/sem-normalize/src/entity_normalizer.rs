//! Entity Normalization Module (`semantica/normalize/entity_normalizer.py`).
//!
//! Entity name standardization, alias resolution, (placeholder) disambiguation, name variant
//! handling and entity linking.

use std::sync::Arc;

use indexmap::IndexMap;
use once_cell::sync::Lazy;
use regex::Regex;
use sem_utils::logging::Logger;
use sem_utils::progress_tracker::ProgressTracker;
use sem_utils::{get_logger, get_progress_tracker};
use serde_json::{Map, Value, json};

use crate::pystr::{self, PY_SPACE};

static WHITESPACE_RUN: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"[{PY_SPACE}]+")).expect("valid regex"));

/// The default `titles` of [`NameVariantHandler`]. Python keeps them in a `set`, whose
/// iteration order varies between processes; the port uses the literal's order.
pub const DEFAULT_TITLES: [&str; 7] = ["Mr.", "Mrs.", "Ms.", "Dr.", "Prof.", "Sir", "Madam"];

/// Entity normalization coordinator (`EntityNormalizer(config=None, **kwargs)`).
pub struct EntityNormalizer {
    /// `config` (with the keyword arguments merged in).
    pub config: Map<String, Value>,
    /// `alias_resolver`.
    pub alias_resolver: AliasResolver,
    /// `disambiguator`.
    pub disambiguator: EntityDisambiguator,
    /// `variant_handler`.
    pub variant_handler: NameVariantHandler,
    /// `progress_tracker` (`get_progress_tracker()`, force-enabled as in the original).
    pub progress_tracker: Arc<ProgressTracker>,
    logger: Logger,
}

impl std::fmt::Debug for EntityNormalizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityNormalizer")
            .field("config", &self.config)
            .field("alias_resolver", &self.alias_resolver)
            .field("variant_handler", &self.variant_handler)
            .finish()
    }
}

impl Default for EntityNormalizer {
    fn default() -> Self {
        EntityNormalizer::new(Map::new())
    }
}

impl EntityNormalizer {
    /// `EntityNormalizer(config)`: the engines are built from the same config (`alias_map`,
    /// `titles`).
    pub fn new(config: Map<String, Value>) -> EntityNormalizer {
        let logger = get_logger("entity_normalizer");
        let alias_resolver = AliasResolver::new(config.clone());
        let disambiguator = EntityDisambiguator::new(config.clone());
        let variant_handler = NameVariantHandler::new(config.clone());
        let progress_tracker = get_progress_tracker();
        if !progress_tracker.enabled() {
            progress_tracker.set_enabled(true);
        }
        logger.debug("Entity normalizer initialized");
        EntityNormalizer {
            config,
            alias_resolver,
            disambiguator,
            variant_handler,
            progress_tracker,
            logger,
        }
    }

    /// `normalize_entity(entity_name, entity_type=None, resolve_aliases=True, **options)`:
    /// strip, collapse whitespace, title-case `Person` names, resolve aliases, then remove
    /// titles and capitalize each word.
    pub fn normalize_entity<'a>(
        &self,
        entity_name: impl Into<Option<&'a str>>,
        entity_type: Option<&str>,
        resolve_aliases: bool,
    ) -> String {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            None,
            None,
            "Semantica: Normalizing entity",
            None,
        );
        let Some(entity_name) = entity_name.into().filter(|n| !n.is_empty()) else {
            self.progress_tracker
                .stop_tracking(&tracking_id, Some("completed"), "", None);
            return String::new();
        };
        let normalized = pystr::strip(entity_name);
        let normalized = WHITESPACE_RUN.replace_all(normalized, " ").into_owned();
        let mut normalized = if entity_type == Some("Person") {
            pystr::title(&normalized)
        } else {
            normalized
        };
        if resolve_aliases
            && let Some(resolved) = self.alias_resolver.resolve_aliases(&normalized)
            && !resolved.is_empty()
        {
            normalized = resolved;
        }
        let normalized = self
            .variant_handler
            .normalize_name_format(&normalized, Some("standard"));
        self.progress_tracker
            .stop_tracking(&tracking_id, Some("completed"), "", None);
        normalized
    }

    /// `resolve_aliases(entity_name, **context)`: the canonical form from the alias map.
    pub fn resolve_aliases(
        &self,
        entity_name: &str,
        _context: &Map<String, Value>,
    ) -> Option<String> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            None,
            None,
            "Semantica: Resolving aliases",
            None,
        );
        let result = self.alias_resolver.resolve_aliases(entity_name);
        self.progress_tracker
            .stop_tracking(&tracking_id, Some("completed"), "", None);
        result
    }

    /// `disambiguate_entity(entity_name, **context)`.
    pub fn disambiguate_entity(
        &self,
        entity_name: &str,
        context: &Map<String, Value>,
    ) -> Map<String, Value> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            None,
            None,
            "Semantica: Disambiguating entity",
            None,
        );
        let result = self.disambiguator.disambiguate(entity_name, context);
        self.progress_tracker
            .stop_tracking(&tracking_id, Some("completed"), "", None);
        result
    }

    /// `link_entities(entities, **options)`: original name -> normalized name (later
    /// duplicates overwrite earlier ones, as in a dict).
    pub fn link_entities<S: AsRef<str>>(
        &self,
        entities: &[S],
        entity_type: Option<&str>,
        resolve_aliases: bool,
    ) -> IndexMap<String, String> {
        let tracking_id = self.progress_tracker.start_tracking(
            None,
            None,
            None,
            "Semantica: Linking entities",
            None,
        );
        let mut linked = IndexMap::new();
        for entity in entities {
            let entity = entity.as_ref();
            let canonical = self.normalize_entity(entity, entity_type, resolve_aliases);
            linked.insert(entity.to_string(), canonical);
        }
        self.progress_tracker
            .stop_tracking(&tracking_id, Some("completed"), "", None);
        linked
    }

    /// The logger (`semantica.entity_normalizer`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

/// Entity alias resolution engine (`AliasResolver(**config)`).
#[derive(Debug, Clone)]
pub struct AliasResolver {
    /// `config`.
    pub config: Map<String, Value>,
    /// `alias_map`: alias -> canonical form (string values of the config's `alias_map`).
    pub alias_map: IndexMap<String, String>,
    logger: Logger,
}

impl Default for AliasResolver {
    fn default() -> Self {
        AliasResolver::new(Map::new())
    }
}

impl AliasResolver {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `AliasResolver(**config)`; `config["alias_map"]` maps aliases to canonical names.
    pub fn new(config: Map<String, Value>) -> AliasResolver {
        let logger = get_logger("alias_resolver");
        let mut alias_map = IndexMap::new();
        if let Some(Value::Object(map)) = config.get("alias_map") {
            for (alias, canonical) in map {
                if let Value::String(canonical) = canonical {
                    alias_map.insert(alias.clone(), canonical.clone());
                }
            }
        }
        logger.debug(format!(
            "Alias resolver initialized ({} aliases)",
            alias_map.len()
        ));
        AliasResolver {
            config,
            alias_map,
            logger,
        }
    }

    /// `resolve_aliases(entity_name, **context)`: exact lookup of the lowercased name, then a
    /// case-insensitive scan of the aliases.
    pub fn resolve_aliases(&self, entity_name: &str) -> Option<String> {
        let entity_lower = pystr::lower(entity_name);
        if let Some(canonical) = self.alias_map.get(&entity_lower) {
            return Some(canonical.clone());
        }
        self.alias_map
            .iter()
            .find(|(alias, _)| pystr::lower(alias) == entity_lower)
            .map(|(_, canonical)| canonical.clone())
    }

    /// `map_variants(entity_name, entity_type)`: returns the name unchanged.
    pub fn map_variants(&self, entity_name: &str, _entity_type: &str) -> String {
        entity_name.to_string()
    }

    /// `handle_cultural_variations(entity_name, culture=None)`: returns the name unchanged.
    pub fn handle_cultural_variations(&self, entity_name: &str, _culture: Option<&str>) -> String {
        entity_name.to_string()
    }
}

/// Entity disambiguation engine (`EntityDisambiguator(**config)`).
#[derive(Debug, Clone)]
pub struct EntityDisambiguator {
    /// `config` (stored, unused).
    pub config: Map<String, Value>,
    logger: Logger,
}

impl Default for EntityDisambiguator {
    fn default() -> Self {
        EntityDisambiguator::new(Map::new())
    }
}

impl EntityDisambiguator {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `EntityDisambiguator(**config)`.
    pub fn new(config: Map<String, Value>) -> EntityDisambiguator {
        let logger = get_logger("entity_disambiguator");
        logger.debug("Entity disambiguator initialized");
        EntityDisambiguator { config, logger }
    }

    /// `disambiguate(entity_name, **context)`: `{"entity_name", "entity_type"
    /// (context's), "confidence": 0.8, "candidates": [entity_name]}`.
    pub fn disambiguate(
        &self,
        entity_name: &str,
        context: &Map<String, Value>,
    ) -> Map<String, Value> {
        let entity_type = context.get("entity_type").cloned().unwrap_or(Value::Null);
        let mut result = Map::new();
        result.insert("entity_name".to_string(), json!(entity_name));
        result.insert("entity_type".to_string(), entity_type);
        result.insert("confidence".to_string(), json!(0.8));
        result.insert("candidates".to_string(), json!([entity_name]));
        result
    }

    /// `classify_entity_type(entity_name, **context)`: `Person` (capitalized, contains a
    /// space), `Organization` (capitalized) or `Entity`.
    pub fn classify_entity_type(&self, entity_name: &str) -> String {
        let Some(first) = entity_name.chars().next() else {
            return "Entity".to_string();
        };
        if pystr::is_upper(first) && entity_name.contains(' ') {
            "Person".to_string()
        } else if pystr::is_upper(first) {
            "Organization".to_string()
        } else {
            "Entity".to_string()
        }
    }

    /// `calculate_confidence(candidates, **context)`: 0.8 for every candidate.
    pub fn calculate_confidence<S: AsRef<str>>(&self, candidates: &[S]) -> IndexMap<String, f64> {
        candidates
            .iter()
            .map(|c| (c.as_ref().to_string(), 0.8))
            .collect()
    }
}

/// Name variant processing engine (`NameVariantHandler(**config)`).
#[derive(Debug, Clone)]
pub struct NameVariantHandler {
    /// `config`.
    pub config: Map<String, Value>,
    /// `titles` (default [`DEFAULT_TITLES`]).
    pub titles: Vec<String>,
    title_patterns: Vec<(String, Regex)>,
    logger: Logger,
}

impl Default for NameVariantHandler {
    fn default() -> Self {
        NameVariantHandler::new(Map::new())
    }
}

impl NameVariantHandler {
    /// The logger (`self.logger`).
    pub fn logger(&self) -> &Logger {
        &self.logger
    }

    /// `NameVariantHandler(**config)`; `config["titles"]` replaces the default titles.
    pub fn new(config: Map<String, Value>) -> NameVariantHandler {
        let logger = get_logger("name_variant_handler");
        let titles = crate::kwargs::get_str_list(&config, "titles", &DEFAULT_TITLES);
        let title_patterns = Self::patterns(&titles);
        logger.debug(format!(
            "Name variant handler initialized ({} titles)",
            titles.len()
        ));
        NameVariantHandler {
            config,
            titles,
            title_patterns,
            logger,
        }
    }

    fn title_pattern(title: &str) -> Regex {
        Regex::new(&format!("(?i)^{}[{PY_SPACE}]*", regex::escape(title)))
            .expect("escaped title is a valid regex")
    }

    fn patterns(titles: &[String]) -> Vec<(String, Regex)> {
        titles
            .iter()
            .map(|t| (t.clone(), Self::title_pattern(t)))
            .collect()
    }

    /// Replace the titles (`handler.titles = ...`).
    pub fn set_titles<S: AsRef<str>>(&mut self, titles: &[S]) {
        self.titles = titles.iter().map(|t| t.as_ref().to_string()).collect();
        self.title_patterns = Self::patterns(&self.titles);
    }

    /// `process_variants(entity_name, **options)`: the name and its standard form when it
    /// differs.
    pub fn process_variants(&self, entity_name: &str) -> Vec<String> {
        let mut variants = vec![entity_name.to_string()];
        let normalized = self.normalize_name_format(entity_name, Some("standard"));
        if normalized != entity_name {
            variants.push(normalized);
        }
        variants
    }

    /// `normalize_name_format(entity_name, format_type="standard")`: remove leading titles
    /// (case-insensitively, each title once, in `titles` order), strip, then `"standard"`
    /// capitalizes each word, `"title"` title-cases, `"lower"` lowercases.
    pub fn normalize_name_format(&self, entity_name: &str, format_type: Option<&str>) -> String {
        let mut name = entity_name.to_string();
        for (i, title) in self.titles.iter().enumerate() {
            // `titles` is public: reuse the compiled pattern only while it still matches.
            name = match self.title_patterns.get(i) {
                Some((cached, pattern)) if cached == title => {
                    pattern.replace(&name, "").into_owned()
                }
                _ => Self::title_pattern(title).replace(&name, "").into_owned(),
            };
        }
        let name = pystr::strip(&name);
        match format_type.unwrap_or("standard") {
            "standard" => pystr::split_whitespace(name)
                .into_iter()
                .map(pystr::capitalize)
                .collect::<Vec<_>>()
                .join(" "),
            "title" => pystr::title(name),
            "lower" => pystr::lower(name),
            _ => name.to_string(),
        }
    }

    /// `handle_titles_and_honorifics(entity_name)`: the first title the name starts with
    /// (case-sensitive) is removed everywhere in the name: `{"name", "title"}`.
    pub fn handle_titles_and_honorifics(&self, entity_name: &str) -> Map<String, Value> {
        let mut title = Value::Null;
        let mut name = entity_name.to_string();
        for t in &self.titles {
            if entity_name.starts_with(t.as_str()) {
                title = json!(t);
                name = pystr::strip(&entity_name.replace(t.as_str(), "")).to_string();
                break;
            }
        }
        let mut result = Map::new();
        result.insert("name".to_string(), json!(name));
        result.insert("title".to_string(), title);
        result
    }
}
