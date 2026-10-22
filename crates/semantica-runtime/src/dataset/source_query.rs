//! Same DatasetSession lifecycle, with an explicitly admitted source result domain.
use super::{
    DatasetSession,
    query::{QueryCacheKey, bounded_json_size},
};
use crate::rdflib_snapshot::Node;
use anyhow::{Context, Result, bail, ensure};
use semantica_contracts::SemanticSessionQueryResult;
use serde_json::Value;
use spareval::SourceTermInput;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Explicit internal comparison contracts; the previous admission is preserved.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SourceSessionContract {
    #[default]
    DefaultGraphV1,
    DatasetDataV2,
    DatasetUpdateV3,
}

fn node(value: &Node) -> SourceTermInput<'_> {
    match value {
        Node::Variable(v) => SourceTermInput::Variable(v),
        Node::Iri(v) => SourceTermInput::Iri(v),
        Node::Blank(v) => SourceTermInput::Blank(v),
        Node::Literal(v) => SourceTermInput::Literal {
            value: &v.text,
            datatype: v.datatype.as_deref(),
            language: v.language.as_deref(),
        },
    }
}
pub(super) fn quad_input(value: &crate::rdflib_snapshot::Quad) -> spareval::SourceQuadInput<'_> {
    (
        [node(&value.0), node(&value.1), node(&value.2)],
        value.3.as_ref().map(node),
    )
}
pub(super) fn binding(value: &Value) -> Result<SourceTermInput<'_>> {
    if let Some(value) = value.as_str() {
        return Ok(SourceTermInput::Literal {
            value,
            datatype: None,
            language: None,
        });
    }
    let object = value
        .as_object()
        .context("source_binding_requires_RDFTermDTO_or_string")?;
    ensure!(
        object
            .keys()
            .all(|v| ["term_type", "value", "datatype", "language"].contains(&v.as_str())),
        "source_binding_unknown_field"
    );
    let text = object
        .get("value")
        .and_then(Value::as_str)
        .context("source_binding_value_required")?;
    let optional = |key| -> Result<Option<&str>> {
        match object.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(v)) => Ok(Some(v)),
            _ => bail!("source_binding_metadata_invalid"),
        }
    };
    let datatype = optional("datatype")?;
    let language = optional("language")?;
    let kind = object
        .get("term_type")
        .and_then(Value::as_str)
        .context("source_binding_type_required")?;
    ensure!(
        kind == "literal" || (datatype.is_none() && language.is_none()),
        "only literal terms may have datatype or language"
    );
    ensure!(
        datatype.is_none() || language.is_none(),
        "an RDF literal cannot have both datatype and language"
    );
    Ok(match kind {
        "iri" => SourceTermInput::Iri(text),
        "blank_node" => SourceTermInput::Blank(text),
        "literal" => SourceTermInput::Literal {
            value: text,
            datatype,
            language,
        },
        _ => bail!("source_binding_term_type_invalid"),
    })
}
impl DatasetSession {
    /// Private admission only. Public profile1-4 and the ordinary constructor retain their meaning.
    pub(crate) fn new_source_internal(cache_queries: bool) -> Self {
        Self {
            source_cache_enabled: Some(cache_queries),
            rdflib_projection: crate::rdflib_snapshot::Projection {
                source_state: Some(spareval::SourceDatasetState::default()),
                ..Default::default()
            },
            ..Self::default()
        }
    }
    /// Extended DATA/named-state contract over the same existing session.
    /// The original source constructor retains DefaultGraphV1 admission.
    pub(crate) fn new_source_data_internal(cache_queries: bool) -> Self {
        Self {
            source_contract: SourceSessionContract::DatasetDataV2,
            ..Self::new_source_internal(cache_queries)
        }
    }
    /// Full UPDATE is an explicit private contract over the same state owner.
    pub(crate) fn new_source_update_internal(cache_queries: bool) -> Self {
        Self {
            source_contract: SourceSessionContract::DatasetUpdateV3,
            ..Self::new_source_internal(cache_queries)
        }
    }
    pub(crate) fn query_source_internal(
        &mut self,
        query: &str,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Value>,
        use_cache: bool,
        cancelled: Option<Arc<AtomicBool>>,
    ) -> Result<SemanticSessionQueryResult> {
        let cache_enabled = self
            .source_cache_enabled
            .context("source_session_admission_required")?;
        if semantica_json::python_strip(query).is_empty() {
            return Err(super::source_failure::SourceFailure::input(
                "SPARQL query must be a non-empty string",
            )
            .into());
        }
        ensure!(
            query.len() <= 1024 * 1024 && namespaces.len() <= 128 && bindings.len() <= 128,
            "source_query_parameter_budget"
        );
        ensure!(
            bounded_json_size(&(namespaces, bindings))?.is_some(),
            "source_query_parameter_byte_budget"
        );
        if cancelled
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            return Err(spareval::QueryEvaluationError::Cancelled.into());
        }
        let quads = self
            .rdflib_projection
            .quads
            .as_ref()
            .context("rdflib_lossless_source_projection_unavailable")?;
        if self.source_contract == SourceSessionContract::DefaultGraphV1 {
            ensure!(
                quads.iter().all(|q| q.3.is_none()),
                "source_query_named_dataset_not_received"
            );
        }
        ensure!(
            self.rdflib_projection.native_projection()? == self.dataset,
            "source_query_projection_core_mismatch"
        );
        let namespace_bytes = self.dataset_byte_limit();
        let namespace_entries = self.resource_profile.quads();
        let state = self
            .rdflib_projection
            .source_state
            .as_ref()
            .context("source_persistent_state_unavailable")?;
        ensure!(
            state.matches_projection(quads.iter().map(quad_input))?,
            "source_persistent_projection_mismatch"
        );
        let bound = bindings
            .iter()
            .map(|(k, v)| Ok((k.clone(), binding(v)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let identities: Vec<_> = bound.iter().map(|(k, v)| (k, v.cache_parts())).collect();
        let key = if cache_enabled && use_cache {
            QueryCacheKey::source(&(query, namespaces, identities))?
        } else {
            None
        };
        if let Some(key) = &key
            && let Some(result) = self.query_cache.get(key)
        {
            return Ok(SemanticSessionQueryResult {
                result: result.clone(),
                revision: self.revision,
                cache_hit: true,
            });
        }
        // Prepare implicit names without warming the retained owner. The
        // original binding constructors run before Dataset.query; only a
        // reached backend attempt publishes its namespace effects.
        let implicit_namespaces = namespaces.is_empty();
        let effective_namespaces;
        let namespaces = if implicit_namespaces {
            effective_namespaces = self
                .source_namespaces
                .temporary_namespaces(namespace_bytes, namespace_entries)?;
            &effective_namespaces
        } else {
            namespaces
        };
        let mut backend_reached = false;
        let result =
            state.query_observed(query, namespaces, &bound, cancelled, &mut backend_reached);
        if implicit_namespaces && backend_reached {
            self.source_namespaces
                .initialize(true, namespace_bytes, namespace_entries)?;
        }
        let result = result.map_err(|error| {
            if backend_reached {
                super::source_failure::SourceFailure::query(error)
            } else {
                super::source_failure::SourceFailure::query_initial(error)
            }
        })?;
        let bytes = bounded_json_size(&result)?.context("dataset_query_result_byte_budget")?;
        if let Some(key) = key {
            self.cache_query_result(key, &result, bytes)?;
        }
        Ok(SemanticSessionQueryResult {
            result,
            revision: self.revision,
            cache_hit: false,
        })
    }
    pub(crate) fn helper_source_internal(
        &mut self,
        helper: &str,
        query: &str,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Value>,
        use_cache: bool,
    ) -> Result<SemanticSessionQueryResult> {
        let expected = match helper {
            "select" => "SELECT",
            "ask" => "ASK",
            "construct" => "CONSTRUCT",
            _ => bail!("source_query_helper_invalid"),
        };
        let result = self.query_source_internal(query, namespaces, bindings, use_cache, None)?;
        if result.result["query_type"] != expected {
            return Err(
                super::source_failure::SourceFailure::unsupported_query(format!(
                    "{}() requires {} {} query, received {}",
                    helper,
                    if expected == "ASK" { "an" } else { "a" },
                    expected,
                    result.result["query_type"].as_str().unwrap_or("unknown"),
                ))
                .into(),
            );
        }
        Ok(result)
    }
}
