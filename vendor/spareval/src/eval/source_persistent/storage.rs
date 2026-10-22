//! Source writer traversal, sharing the existing persistent Memory context owner.
//! RDFLib 7.6.0 Dataset.contexts / Memory.triples (BSD-3-Clause).
use super::*;
use super::lifecycle::view;

impl SourceDatasetState {
    /// Dataset.graphs() calls Dataset.graph(default) only when the default
    /// context is absent. The wrapper uses this to retain manager side effects.
    pub fn serialization_default_registered(&self) -> Result<bool, QueryEvaluationError> {
        let graph=self.graph(None)?;
        Ok(graph.context_names().map_err(error)?.contains(&graph.default_context().map_err(error)?))
    }
    /// Dataset.graphs() registers the default context even on a read.
    /// Nonempty named contexts alone cause whole-graph syntax loss refusal.
    pub fn serialization_has_named_quads(&mut self, max_quads: usize, max_bytes: usize)
        -> Result<bool, QueryEvaluationError>
    {
        let mut graph = self.graph(None)?;
        let default = graph.default_context().map_err(error)?;
        let mut names = graph.context_names().map_err(error)?;
        if !names.contains(&default) {
            graph.register_context(&default).map_err(error)?;
            names.push(default.clone());
        }
        let mut named = false;
        for name in names {
            if name != default && graph.len_context(Some(&name)).map_err(error)? > 0 {
                named = true;
            }
        }
        self.commit(graph, max_quads, max_bytes)?;
        Ok(named)
    }

    /// Context-major, then the existing full-pattern set-copy iteration.
    /// `None` scan is the UNION and must never stand for the default graph.
    /// Selected empty context registration survives successful/failed writing;
    /// it is not a data mutation and does not invalidate wrapper caches.
    pub fn visit_serialization_rows(
        &mut self,
        dataset_syntax: bool,
        graph_name: Option<&str>,
        max_quads: usize,
        max_bytes: usize,
        mut emit: impl FnMut(SourceQuadInput<'_>) -> Result<(), QueryEvaluationError>,
    ) -> Result<(), QueryEvaluationError> {
        let mut graph = self.graph(None)?;
        let names = if dataset_syntax {
            if graph_name.is_some() { return Err(error("source_writer_selected_dataset_syntax")); }
            let mut names = graph.context_names().map_err(error)?;
            let default = graph.default_context().map_err(error)?;
            if !names.contains(&default) {
                graph.register_context(&default).map_err(error)?;
                names.push(default);
            }
            names
        } else {
            let name = if let Some(name) = graph_name {
                let raw = graph_value(SourceTermInput::Iri(name), graph.session())?;
                graph.admit_recorded_source(raw).map_err(error)?
            } else { graph.default_context().map_err(error)? };
            graph.register_context(&name).map_err(error)?;
            vec![name]
        };
        let result = (|| {
            for name in names {
                let raw_name = graph.externalize_source(name.clone()).map_err(error)?;
                let scope = match view(&raw_name)? {
                    SourceTermInput::Iri(DEFAULT) => None,
                    name => Some(name),
                };
                for row in graph.scan_context(Some(&name), [None, None, None]).map_err(error)? {
                    let [a,b,c] = row.each_ref().map(|v| graph.externalize_source(v.clone()));
                    let values = [a.map_err(error)?, b.map_err(error)?, c.map_err(error)?];
                    emit(([view(&values[0])?, view(&values[1])?, view(&values[2])?], scope))?;
                }
            }
            Ok(())
        })();
        self.commit(graph, max_quads, max_bytes)?;
        result
    }
}
