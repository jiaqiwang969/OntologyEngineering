//! Source Turtle prerequisites on the one DatasetSession, not a serializer.
//! RDFLib760 preprocessTriple/get_pname/base rules (BSD-3-Clause).
use super::{
    DatasetSession,
    source_failure::SourceFailure,
    source_namespace::{NamespaceState, TurtleNames, can_relativize},
};
use crate::rdflib_snapshot::Node;
use anyhow::{Result, ensure};
use semantica_contracts::SemanticSourceSerializeRequest;

pub(super) struct PreparedTurtle {
    pub rows: Vec<[Node; 3]>,
    pub names: TurtleNames,
    pub base: Option<String>,
}
impl DatasetSession {
    /// Complete internal text candidate. Public session1–3 deliberately keep
    /// their original NT/NQ result/witness; Turtle needs independent reception.
    pub(super) fn serialize_source_turtle_internal(
        &mut self,
        request: &SemanticSourceSerializeRequest,
    ) -> Result<String> {
        let prepared = self.prepare_source_turtle_internal(request)?;
        let bytes = self.dataset_byte_limit();
        let entries = self.resource_profile.quads();
        let graph = self
            .rdflib_projection
            .source_state
            .as_mut()
            .unwrap()
            .turtle_view(request.graph_name.as_deref(), entries, bytes, 2_000_000)?;
        super::source_turtle_writer::Writer::new(
            prepared,
            graph,
            &mut self.source_namespaces,
            request.graph_name.is_some(),
            bytes,
            entries,
        )
        .render()
    }
    /// Internal seam only. No public batch routing calls this method, and the
    /// existing NT/NQ witness is never reused to claim Turtle coverage.
    pub(super) fn prepare_source_turtle_internal(
        &mut self,
        request: &SemanticSourceSerializeRequest,
    ) -> Result<PreparedTurtle> {
        self.prepare_source_turtle_until(request, None)
    }
    pub(super) fn prepare_source_turtle_until(
        &mut self,
        request: &SemanticSourceSerializeRequest,
        at: Option<std::time::Instant>,
    ) -> Result<PreparedTurtle> {
        check_time(at)?;
        self.source_storage_admission()?;
        ensure!(
            request.validate().is_ok(),
            "source_serialize_request_invalid"
        );
        ensure!(
            matches!(
                semantica_json::python_strip(&request.format)
                    .to_lowercase()
                    .as_str(),
                "ttl" | "turtle"
            ),
            "source_turtle_preparation_format"
        );
        super::source_storage::validate_writer_graph(request, "turtle")?;
        let bytes = self.dataset_byte_limit();
        let max_quads = self.resource_profile.quads();
        let state = self.rdflib_projection.source_state.as_mut().unwrap();
        if request.graph_name.is_some() || !state.serialization_default_registered()? {
            self.source_namespaces.initialize(true, bytes, max_quads)?;
        }
        if request.graph_name.is_none() && state.serialization_has_named_quads(max_quads, bytes)? {
            return Err(SourceFailure::lossy("Dataset contains named graphs; serializing the whole dataset as 'turtle' would lose graph names. Use 'trig' or 'nquads', or select graph_name explicitly.").into());
        }
        let mut rows = Vec::new();
        state.visit_serialization_rows(
            false,
            request.graph_name.as_deref(),
            max_quads,
            bytes,
            |(t, _)| {
                check_time(at).map_err(|e| spareval::QueryEvaluationError::Unexpected(e.into()))?;
                rows.push(t.map(super::source_update::node));
                Ok(())
            },
        )?;
        let names = prepare_names(
            &rows,
            &mut self.source_namespaces,
            request,
            bytes,
            max_quads,
            at,
        )?;
        Ok(PreparedTurtle {
            rows,
            names,
            base: request.base_uri.clone(),
        })
    }
}

fn check_time(at: Option<std::time::Instant>) -> Result<()> {
    ensure!(
        at.is_none_or(|at| std::time::Instant::now() < at),
        "source_turtle_deadline"
    );
    Ok(())
}
pub(super) fn prepare_names(
    rows: &[[Node; 3]],
    state: &mut NamespaceState,
    request: &SemanticSourceSerializeRequest,
    bytes: usize,
    entries: usize,
    at: Option<std::time::Instant>,
) -> Result<TurtleNames> {
    let named = request.graph_name.is_some();
    let base = request.base_uri.as_deref();
    let mut names = TurtleNames::default();
    // Original empty graphs do not access a manager during Turtle
    // preprocessing. Preserve that distinction rather than eager init.
    for triple in rows {
        check_time(at)?;
        for (position, term) in triple.iter().enumerate() {
            check_time(at)?;
            if let Node::Iri(uri) = term {
                if position == 1
                    && (uri == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                        || can_relativize(uri, base))
                {
                    continue;
                }
                names.pname(state, named, uri, position == 1, bytes, entries)?;
            }
            if let Node::Literal(value) = term
                && let Some(uri) = value.datatype.as_deref().filter(|v| !v.is_empty())
            {
                names.pname(state, named, uri, false, bytes, entries)?;
            }
        }
    }
    check_time(at)?;
    Ok(names)
}

#[cfg(test)]
#[path = "source_turtle_receiving.rs"]
mod receiving;
