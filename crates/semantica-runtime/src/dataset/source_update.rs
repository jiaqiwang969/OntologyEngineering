//! Source UPDATE lifecycle in the existing DatasetSession.
use super::source_failure::SourceFailure;
use super::*;
use crate::rdflib_snapshot::{Literal, Node, Projection};
use spareval::SourceTermInput;

pub(super) fn node(value: SourceTermInput<'_>) -> Node {
    match value {
        SourceTermInput::Variable(v) => Node::Variable(v.into()),
        SourceTermInput::Iri(v) => Node::Iri(v.into()),
        SourceTermInput::Blank(v) => Node::Blank(v.into()),
        SourceTermInput::Literal {
            value,
            datatype,
            language,
        } => Node::Literal(Literal {
            text: value.into(),
            datatype: datatype.map(str::to_owned),
            language: language.map(str::to_owned),
        }),
    }
}
impl DatasetSession {
    fn check_source_projection(&self, projection: &Projection) -> Result<()> {
        ensure!(
            projection.native_projection()? == self.dataset,
            "source_update_projection_core_mismatch"
        );
        let quads = projection
            .quads
            .as_ref()
            .context("rdflib_lossless_source_projection_unavailable")?;
        let state = projection
            .source_state
            .as_ref()
            .context("source_persistent_state_unavailable")?;
        ensure!(
            state.matches_projection(quads.iter().map(super::source_query::quad_input))?,
            "source_persistent_projection_mismatch"
        );
        Ok(())
    }
    fn invalidate_source_caches(&mut self) {
        self.query_cache.clear();
        self.cache_bytes = 0;
        self.snapshot_cache = None;
        self.explorer_search_cache = None;
        self.rdflib_snapshot_cache.set(None);
    }
    pub(crate) fn update_source_internal(
        &mut self,
        text: &str,
        namespaces: &BTreeMap<String, String>,
    ) -> Result<SemanticMutationResult> {
        ensure!(
            self.source_cache_enabled.is_some()
                && self.source_contract
                    == super::source_query::SourceSessionContract::DatasetDataV2,
            "source_DATA_contract_admission_required"
        );
        if text.trim().is_empty() {
            return Err(SourceFailure::input("SPARQL update must be a non-empty string").into());
        }
        ensure!(
            text.len() <= 1024 * 1024 && namespaces.len() <= 128,
            "source_update_parameter_budget"
        );
        ensure!(
            super::query::bounded_json_size(namespaces)?.is_some(),
            "source_update_parameter_byte_budget"
        );
        self.check_source_projection(&self.rdflib_projection)?;
        let before = self.rdflib_projection.quads.as_ref().unwrap().len() as u64;
        let mut projection = self.rdflib_projection.clone();
        let result = projection
            .source_state
            .as_mut()
            .context("source_persistent_state_unavailable")?
            .update_data(
                text,
                namespaces,
                self.resource_profile.quads(),
                self.dataset_byte_limit(),
            );
        self.publish_source_update_reached(
            before,
            projection,
            result.map_err(|error| {
                SourceFailure::input(format!("SPARQL update failed: {error}")).into()
            }),
        )
    }

    /// Actual runtime input boundary, including null/non-string refusal.
    /// The caller supplies resource authority explicitly; text grants none.
    pub(crate) fn update_source_full_internal(
        &mut self,
        query: &Value,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Value>,
        loader: &mut dyn spareval::SourceUpdateLoader,
    ) -> Result<SemanticMutationResult> {
        self.update_source_full_with_constructor_internal(query, namespaces, bindings, loader, None)
    }

    /// Per-call explicit construction input, private V3 only. It supplies no
    /// query answer, resource authority, alternative store or public profile.
    pub(crate) fn update_source_full_with_constructor_internal(
        &mut self,
        query: &Value,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Value>,
        loader: &mut dyn spareval::SourceUpdateLoader,
        constructor: Option<spargebra::SourceBlankNodeConstructor>,
    ) -> Result<SemanticMutationResult> {
        self.update_source_full_observed_internal(
            query,
            namespaces,
            bindings,
            loader,
            constructor,
            &mut false,
        )
    }
    /// Carry the existing typed backend boundary for the explicit session port.
    /// This does not infer a phase from error text or change publication rules.
    pub(super) fn update_source_full_observed_internal(
        &mut self,
        query: &Value,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, Value>,
        loader: &mut dyn spareval::SourceUpdateLoader,
        constructor: Option<spargebra::SourceBlankNodeConstructor>,
        backend_reached: &mut bool,
    ) -> Result<SemanticMutationResult> {
        *backend_reached = false;
        ensure!(
            self.source_cache_enabled.is_some()
                && self.source_contract
                    == super::source_query::SourceSessionContract::DatasetUpdateV3,
            "source_UPDATE_v3_contract_admission_required"
        );
        let text = query
            .as_str()
            .filter(|text| !semantica_json::python_strip(text).is_empty())
            .ok_or_else(|| SourceFailure::input("SPARQL update must be a non-empty string"))?;
        ensure!(
            text.len() <= 1024 * 1024 && namespaces.len() <= 128 && bindings.len() <= 128,
            "source_update_parameter_budget"
        );
        ensure!(
            super::query::bounded_json_size(&(namespaces, bindings))?.is_some(),
            "source_update_parameter_byte_budget"
        );
        self.check_source_projection(&self.rdflib_projection)?;
        let bound = bindings
            .iter()
            .map(|(name, value)| Ok((name.clone(), super::source_query::binding(value)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let before = self.rdflib_projection.quads.as_ref().unwrap().len() as u64;
        let bytes = self.dataset_byte_limit();
        let entries = self.resource_profile.quads();
        let implicit_namespaces = namespaces.is_empty();
        let effective_namespaces;
        let namespaces = if implicit_namespaces {
            effective_namespaces = self
                .source_namespaces
                .temporary_namespaces(bytes, entries)?;
            &effective_namespaces
        } else {
            namespaces
        };
        let mut projection = self.rdflib_projection.clone();
        let result = projection
            .source_state
            .as_mut()
            .context("source_persistent_state_unavailable")?
            .update_full_with_constructor(
                text,
                namespaces,
                &bound,
                loader,
                self.resource_profile.quads(),
                self.dataset_byte_limit(),
                constructor,
            );
        if result
            .as_ref()
            .err()
            .is_some_and(|error| error.before_backend)
        {
            return Err(SourceFailure::update(result.unwrap_err()));
        }
        *backend_reached = true;
        // Typed constructors can fail before Graph.update is entered. Only a
        // reached attempt publishes the temporary manager's shared bindings,
        // including syntax failures; its private trie/cache is discarded.
        if implicit_namespaces {
            self.source_namespaces
                .initialize_temporary(bytes, entries)?;
        }
        self.publish_source_update_reached(
            before,
            projection,
            result.map_err(SourceFailure::update),
        )
    }

    fn publish_source_update_reached(
        &mut self,
        before: u64,
        mut projection: Projection,
        result: Result<()>,
    ) -> Result<SemanticMutationResult> {
        // Source state is authoritative. A native projection may cover only
        // representable values; readback cannot sort/rebuild the live store.
        let mut quads = BTreeSet::new();
        projection
            .source_state
            .as_ref()
            .unwrap()
            .visit_quads(|(t, g)| {
                quads.insert((node(t[0]), node(t[1]), node(t[2]), g.map(node)));
                Ok(())
            })?;
        let mut core = BTreeMap::new();
        for q in &quads {
            let represented = match self.rdflib_projection.core.get(q) {
                Some(v) => v.clone(),
                None => crate::rdflib_snapshot::core_quad(q, None)
                    .ok()
                    .into_iter()
                    .collect(),
            };
            core.insert(q.clone(), represented);
        }
        projection.quads = Some(quads);
        projection.core = core;
        let staged = projection.native_projection()?;
        let after = projection.quads.as_ref().unwrap().len() as u64;
        match result {
            Ok(()) => {
                let r = self.publish_source_state("update", staged, projection)?;
                Ok(SemanticMutationResult {
                    operation: "update".into(),
                    added: after.saturating_sub(before),
                    removed: before.saturating_sub(after),
                    quad_count: after,
                    revision: r.revision,
                })
            }
            Err(error) => {
                // Original backend failure preserves the reached graph state,
                // clears both caches, and does not advance the revision.
                self.validate_source_state(&staged, &projection)?;
                self.dataset = staged;
                self.rdflib_projection = projection;
                self.invalidate_source_caches();
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod typed_compatibility_receiving {
    use super::*;
    use serde_json::json;

    const QUERY: &str = "SELECT ?s ?p ?o WHERE { ?s ?p ?o }";
    const WHEN: &str = "2026-10-12T04:00:00Z";
    struct NoResources;
    impl spareval::SourceUpdateLoader for NoResources {
        fn load(
            &mut self,
            _: &str,
            _: &mut dyn FnMut(
                [SourceTermInput<'_>; 3],
            )
                -> std::result::Result<(), spareval::QueryEvaluationError>,
        ) -> std::result::Result<bool, spareval::QueryEvaluationError> {
            panic!("these typed-boundary controls never authorize a resource request")
        }
    }

    fn primed() -> DatasetSession {
        let mut session = DatasetSession::new_source_update_internal(true);
        session
            .load_rdflib_with_base(
                b"<urn:s> <urn:p> \"old\" .\n",
                RdfFormat::NQuads,
                None,
                None,
            )
            .unwrap();
        session
            .helper_source_internal("select", QUERY, &BTreeMap::new(), &BTreeMap::new(), true)
            .unwrap();
        session.snapshot_rdflib(WHEN).unwrap();
        assert_eq!(session.revision(), 1);
        assert_eq!(session.quad_count(), 1);
        assert_eq!(session.query_cache.len(), 1);
        assert!(session.rdflib_snapshot_cache.get().is_some());
        session
    }
    fn assert_original(error: &anyhow::Error, module: &str, class: &str, message: &str) {
        let original = error
            .downcast_ref::<SourceFailure>()
            .expect("typed original exception required");
        assert_eq!(
            (original.module, original.class, original.to_string()),
            (module, class, message.to_owned())
        );
    }
    fn cached_snapshot(session: &DatasetSession) -> Value {
        serde_json::to_value(session.rdflib_snapshot_cache.get()).unwrap()
    }

    #[test]
    fn source_update_typed_invalid_language_preserves_raw_str_and_before_backend_cache() {
        // RDFLib term.py uses "'" + str(lang) + "'", not repr(lang).
        let cases = [
            ("en_US", "'en_US' is not a valid language tag!"),
            ("en'US", "'en'US' is not a valid language tag!"),
            ("en\\US", "'en\\US' is not a valid language tag!"),
            ("en\nUS", "'en\nUS' is not a valid language tag!"),
        ];
        for (language, expected) in cases {
            let mut session = primed();
            let snapshot = cached_snapshot(&session);
            let bindings = BTreeMap::from([(
                "x".into(),
                json!({"term_type":"literal","value":"v","datatype":null,"language":language}),
            )]);
            let error = session
                .update_source_full_internal(
                    &json!("INSERT DATA { <urn:new> <urn:p> \"new\" }"),
                    &BTreeMap::new(),
                    &bindings,
                    &mut NoResources,
                )
                .unwrap_err();
            assert_original(&error, "builtins", "ValueError", expected);
            assert_eq!(session.revision(), 1);
            assert_eq!(session.quad_count(), 1);
            assert_eq!(session.query_cache.len(), 1);
            assert_eq!(cached_snapshot(&session), snapshot);
            assert!(
                session
                    .helper_source_internal(
                        "select",
                        QUERY,
                        &BTreeMap::new(),
                        &BTreeMap::new(),
                        true
                    )
                    .unwrap()
                    .cache_hit
            );
            println!(
                "OE_SOURCE_UPDATE_TYPED_LANGUAGE {}",
                json!({"language":language,"expected":expected,"actual":error.to_string(),"revision":session.revision(),"query_cache_entries":session.query_cache.len(),"snapshot_cached":session.rdflib_snapshot_cache.get().is_some()})
            );
        }
    }

    #[test]
    fn source_update_typed_parser_error_clears_cache_without_running_prior_statement() {
        let cases = [
            (
                "INSERT DATA { <urn:first> <urn:p> \"x\" }; INVALID_more",
                "SPARQL update failed: Expected end of text, found 'INVALID'  (at char 41), (line:1, col:42)",
            ),
            (
                "INSERT DATA { <urn:first> <urn:p> \"x\" }; ABCDEFGHIJKLMNOPQRSTUVW",
                "SPARQL update failed: Expected end of text, found 'ABCDEFGHIJKLMNOP'  (at char 41), (line:1, col:42)",
            ),
            (
                "INSERT DATA { <urn:first> <urn:p> \"x\" };\nINVALID",
                "SPARQL update failed: Expected end of text, found 'INVALID'  (at char 41), (line:2, col:1)",
            ),
        ];
        for (input, expected) in cases {
            let mut session = primed();
            let before = session.snapshot_rdflib(WHEN).unwrap();
            let error = session
                .update_source_full_internal(
                    &json!(input),
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &mut NoResources,
                )
                .unwrap_err();
            assert_original(
                &error,
                "semantica.ontology.runtime",
                "SemanticInputError",
                expected,
            );
            assert_eq!(session.revision(), 1);
            assert_eq!(session.quad_count(), 1);
            assert_eq!(session.query_cache.len(), 0);
            assert!(session.rdflib_snapshot_cache.get().is_none());
            assert_eq!(
                serde_json::to_value(session.snapshot_rdflib(WHEN).unwrap()).unwrap(),
                serde_json::to_value(before).unwrap()
            );
            println!(
                "OE_SOURCE_UPDATE_TYPED_PARSER {}",
                json!({"input":input,"expected":expected,"actual":error.to_string(),"revision":session.revision()})
            );
        }
    }

    #[test]
    fn source_update_typed_python_control_whitespace_fails_before_backend() {
        for input in ["\u{001c}", "\u{001d}", "\u{001e}", "\u{001f}"] {
            let mut session = primed();
            let snapshot = cached_snapshot(&session);
            let error = session
                .update_source_full_internal(
                    &json!(input),
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &mut NoResources,
                )
                .unwrap_err();
            assert_original(
                &error,
                "semantica.ontology.runtime",
                "SemanticInputError",
                "SPARQL update must be a non-empty string",
            );
            assert_eq!(session.revision(), 1);
            assert_eq!(session.quad_count(), 1);
            assert_eq!(session.query_cache.len(), 1);
            assert_eq!(cached_snapshot(&session), snapshot);
        }
    }

    #[test]
    fn source_update_typed_wrapper_does_not_relabel_guard_text() {
        for before_backend in [true, false] {
            let error = SourceFailure::update(spareval::SourceUpdateError {
                before_backend,
                error: spareval::QueryEvaluationError::Unexpected("Create not implemented!".into()),
            });
            assert!(error.downcast_ref::<SourceFailure>().is_none());
            let guard = error
                .downcast_ref::<spareval::SourceUpdateError>()
                .expect("actual Rust source error retained");
            assert_eq!(guard.before_backend, before_backend);
            assert_eq!(guard.to_string(), "Create not implemented!");
        }
    }
}
