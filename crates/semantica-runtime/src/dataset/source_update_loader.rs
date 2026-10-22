//! Explicit caller-owned bytes for source LOAD/USING. No file or network I/O.
use super::{source_load, source_query, RdfFormat};
use anyhow::{ensure, Result};
use spareval::{QueryEvaluationError, SourceTermInput, SourceUpdateLoader};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(crate) enum SourceUpdateResource<'a> {
    /// Expected identity must come from the caller's separately verified source.
    Turtle { bytes: &'a [u8], sha256: &'a str },
    /// An explicit observation of absence, never the fallback for an unknown IRI.
    ObservedAbsent,
}

pub(crate) struct SourceUpdateBytesLoader<'a> {
    resources: BTreeMap<String, SourceUpdateResource<'a>>,
    max_triples: usize,
    emitted: usize,
}
impl<'a> SourceUpdateBytesLoader<'a> {
    pub(crate) fn new(
        resources: BTreeMap<String, SourceUpdateResource<'a>>,
        max_bytes: usize,
        max_triples: usize,
    ) -> Result<Self> {
        ensure!(resources.len() <= 128, "source_update_resource_count_budget");
        let mut total = 0usize;
        for resource in resources.values() {
            if let SourceUpdateResource::Turtle { bytes, sha256 } = resource {
                total = total.checked_add(bytes.len())
                    .ok_or_else(|| anyhow::anyhow!("source_update_resource_byte_budget"))?;
                ensure!(total <= max_bytes, "source_update_resource_byte_budget");
                ensure!(crate::sha256(bytes) == *sha256, "source_update_resource_identity_mismatch");
            }
        }
        Ok(Self { resources, max_triples, emitted: 0 })
    }
}
fn refusal(error: impl std::fmt::Display) -> QueryEvaluationError {
    // The private SourceCompValueError is intentionally not used: SILENT must
    // not turn authority, parser-admission, emission or budget refusal into success.
    QueryEvaluationError::Unexpected(error.to_string().into())
}
impl SourceUpdateLoader for SourceUpdateBytesLoader<'_> {
    fn load(
        &mut self,
        iri: &str,
        emit: &mut dyn FnMut([SourceTermInput<'_>; 3]) -> Result<(), QueryEvaluationError>,
    ) -> Result<bool, QueryEvaluationError> {
        let Some(resource) = self.resources.get(iri).copied() else {
            return Err(refusal("source_update_resource_not_authorized"));
        };
        let SourceUpdateResource::Turtle { bytes, .. } = resource else {
            return Ok(false);
        };
        let parsed = source_load::parse(bytes, RdfFormat::Turtle, Some(iri)).map_err(refusal)?;
        for event in parsed {
            let event = event.map_err(refusal)?;
            let source = event.source.ok_or_else(|| refusal("source_update_resource_source_fact_missing"))?;
            if source.3.is_some() {
                return Err(refusal("source_update_resource_named_parser_output"));
            }
            if self.emitted >= self.max_triples {
                return Err(refusal("source_update_resource_triple_budget"));
            }
            self.emitted += 1;
            emit(source_query::quad_input(&source).0)?;
        }
        Ok(true)
    }
}
