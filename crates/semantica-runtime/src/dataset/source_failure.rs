//! Original error identity at actual runtime boundaries, not text classification.
#[derive(Debug)]
pub(super) struct SourceFailure {
    pub module: &'static str,
    pub class: &'static str,
    message: String,
}
impl std::fmt::Display for SourceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for SourceFailure {}
impl SourceFailure {
    pub(super) fn unsupported_query(message: String) -> Self {
        Self {
            module: "semantica.ontology.runtime",
            class: "UnsupportedQueryTypeError",
            message,
        }
    }
    pub(super) fn query_initial(error: spareval::QueryEvaluationError) -> anyhow::Error {
        // Shared QUERY/UPDATE constructors retain the same typed source
        // exceptions. Guards and unreceived diagnostics keep their own identity.
        let error = spareval::SourceUpdateError {
            before_backend: true,
            error,
        };
        if let Some((module, class, message)) = error.original_exception() {
            Self {
                module,
                class,
                message,
            }
            .into()
        } else {
            error.error.into()
        }
    }
    pub(super) fn turtle_uri(uri: &str) -> Self {
        Self {
            module: "builtins",
            class: "Exception",
            message: format!(
                "\"{uri}\" does not look like a valid URI, I cannot serialize this as N3/Turtle. Perhaps you wanted to urlencode it?"
            ),
        }
    }
    pub(super) fn input(message: impl Into<String>) -> Self {
        Self {
            module: "semantica.ontology.runtime",
            class: "SemanticInputError",
            message: message.into(),
        }
    }
    pub(super) fn lossy(message: impl Into<String>) -> Self {
        Self {
            module: "semantica.ontology.runtime",
            class: "LossySerializationError",
            message: message.into(),
        }
    }
    /// Source NQ subject refusal at a real native syntax error location.
    /// No diagnostic-string classification, preemptive input replacement or
    /// parser relaxation. Other syntax/IO failures remain their actual errors.
    pub(super) fn nquads_parse(error: oxigraph::io::RdfParseError, bytes: &[u8]) -> anyhow::Error {
        if let oxigraph::io::RdfParseError::Syntax(syntax) = &error
            && let Some(location) = syntax.location()
            && let Ok(text) = std::str::from_utf8(bytes)
            && let Ok(offset) = usize::try_from(location.start.offset)
            && let (Some(prefix), Some(tail)) = (text.get(..offset), text.get(offset..))
        {
            let start = prefix.rfind(['\r', '\n']).map_or(0, |i| i + 1);
            let end = tail.find(['\r', '\n']).map_or(text.len(), |i| offset + i);
            if let Some(line) = text.get(start..end) {
                let subject = line.trim_start_matches([' ', '\t']);
                let subject_start = start + line.len() - subject.len();
                // RDFLib subject() can only begin with uriref or nodeid.
                // A syntax error at any other first token is unambiguous.
                if offset == subject_start
                    && !subject.is_empty()
                    && !subject.starts_with('#')
                    && !subject.starts_with('<')
                    && !subject.starts_with("_:")
                    && let Ok(quoted) = semantica_json::python_repr(&serde_json::json!(line))
                {
                    return Self::input(format!("Failed to parse RDF as nquads: Invalid line (Subject must be uriref or nodeID):\n{quoted}")).into();
                }
            }
        }
        error.into()
    }
    pub(super) fn update(error: spareval::SourceUpdateError) -> anyhow::Error {
        if let Some((module, class, message)) = error.original_exception() {
            if error.before_backend {
                return Self {
                    module,
                    class,
                    message,
                }
                .into();
            }
            return Self::input(format!("SPARQL update failed: {message}")).into();
        }
        // Guards and unreceived diagnostics remain actual Rust errors.
        error.into()
    }
    pub(super) fn query(error: spareval::QueryEvaluationError) -> anyhow::Error {
        match error {
            spareval::QueryEvaluationError::UnsupportedSourceRuntimeTerm(kind) => Self {
                module: "semantica.ontology.runtime",
                class: "SemanticRuntimeError",
                message: format!("Unsupported RDF term returned by backend: {kind}"),
            }
            .into(),
            other => match other.source_parser_exception_ascii() {
                Ok(message) => Self::input(format!("SPARQL query failed: {message}")).into(),
                Err(other) => other.into(),
            },
        }
    }
    pub(super) fn snapshot(error: anyhow::Error) -> anyhow::Error {
        if error.is::<crate::rdflib_snapshot::PredicateNotIri>() {
            Self {
                module: "builtins",
                class: "ValueError",
                message: error.to_string(),
            }
            .into()
        } else {
            error
        }
    }
}
