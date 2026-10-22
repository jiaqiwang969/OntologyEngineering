//! Source storage lifecycle on the existing V3 session. The native RDF writer,
//! RDFC diff and their completeness guards retain their original contracts.
//! NT/NQ quoting ports RDFLib7.6.0 serializers/nt.py (BSD-3-Clause).
use super::*;
use super::{source_failure::SourceFailure, source_query::SourceSessionContract};
use semantica_contracts::{
    SemanticSourceDatasetDiff, SemanticSourceSerialization, SemanticSourceSerializeRequest,
    SemanticSourceSnapshotArgument,
};
use spareval::{QueryEvaluationError, SourceTermInput};

pub(crate) enum SourceSnapshotInput<'a> {
    Snapshot(&'a SemanticSourceSnapshotArgument),
    NotSnapshot,
}

fn when(created_at: &str) -> String {
    if created_at.is_empty() {
        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
            .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
    } else {
        created_at.to_owned()
    }
}

pub(crate) fn source_snapshot_argument(
    value: &semantica_contracts::SemanticRdfLibDatasetSnapshot,
) -> SemanticSourceSnapshotArgument {
    SemanticSourceSnapshotArgument {
        schema_version: value.schema_version.clone(),
        algorithm: value.algorithm.clone(),
        dataset_sha256: value.dataset_sha256.clone(),
        quad_count: value.quad_count,
        canonical_nquads: value.canonical_nquads.clone(),
        revision: value.revision,
        created_at: value.created_at.clone(),
    }
}

/// Python str.splitlines(), including CRLF and all eleven line boundaries.
/// Source snapshot integrity is not an RDF reparse or RDFC canonicalization.
pub(super) fn splitlines(text: &str) -> Vec<&str> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            rows.push(&text[start..i]);
            start = i + c.len_utf8();
            if c == '\r' && chars.peek().is_some_and(|(_, c)| *c == '\n') {
                let (j, c) = chars.next().unwrap();
                start = j + c.len_utf8();
            }
        }
    }
    if start < text.len() {
        rows.push(&text[start..]);
    }
    rows
}
fn integrity(value: &SemanticSourceSnapshotArgument) -> bool {
    let rows = splitlines(&value.canonical_nquads);
    rows.len() as u64 == value.quad_count
        && rows.windows(2).all(|w| w[0] <= w[1])
        && crate::sha256(value.canonical_nquads.as_bytes()) == value.dataset_sha256
}
pub(super) fn term_n3(value: SourceTermInput<'_>) -> String {
    super::source_update::node(value).n3()
}
pub(super) fn object_nt(value: SourceTermInput<'_>) -> String {
    if let SourceTermInput::Literal {
        value,
        datatype,
        language,
    } = value
    {
        let mut text = format!(
            "\"{}\"",
            value
                .replace('\\', "\\\\")
                .replace('\n', "\\n")
                .replace('"', "\\\"")
                .replace('\r', "\\r")
        );
        if let Some(lang) = language.filter(|v| !v.is_empty()) {
            text.push('@');
            text.push_str(lang);
        } else if let Some(dt) = datatype.filter(|v| !v.is_empty()) {
            text.push_str("^^<");
            text.push_str(dt);
            text.push('>');
        }
        text
    } else {
        term_n3(value)
    }
}

/// Original serialization request validation shared by execution and relation receiving.
pub(super) fn writer_format(request: &SemanticSourceSerializeRequest) -> Result<&'static str> {
    request
        .validate()
        .map_err(|v| anyhow::anyhow!("source_serialize_request_invalid:{v:?}"))?;
    let format = match semantica_json::python_strip(&request.format)
        .to_lowercase()
        .as_str()
    {
        "nt" | "ntriples" | "n-triples" => "nt",
        "nq" | "nquads" | "n-quads" => "nquads",
        _ => bail!("source_serialization_format_not_received"),
    };
    validate_writer_graph(request, format)?;
    Ok(format)
}

pub(super) fn validate_writer_graph(
    request: &SemanticSourceSerializeRequest,
    format: &str,
) -> Result<()> {
    let dataset = matches!(format, "nquads" | "trig" | "trix");
    if let Some(name) = &request.graph_name {
        if semantica_json::python_strip(name).is_empty() {
            return Err(SourceFailure::input("graph_name must be a non-empty absolute IRI").into());
        }
        let scheme = name.split_once(':').map(|v| v.0).unwrap_or("");
        let mut letters = scheme.bytes();
        let valid_scheme = letters.next().is_some_and(|c| c.is_ascii_alphabetic())
            && letters.all(|c| c.is_ascii_alphanumeric() || b"+.-".contains(&c));
        if !valid_scheme
            || name
                .chars()
                .any(|c| semantica_json::python_character_class(c).1)
        {
            return Err(SourceFailure::input(format!(
                "graph_name must be an absolute IRI: {}",
                semantica_json::python_repr(&json!(name))?
            ))
            .into());
        }
        if dataset {
            return Err(SourceFailure::input(format!(
                "a selected graph must use a graph syntax, not {format}"
            ))
            .into());
        }
    }
    Ok(())
}

impl DatasetSession {
    pub(super) fn source_storage_admission(&self) -> Result<()> {
        ensure!(
            self.source_cache_enabled.is_some()
                && self.source_contract == SourceSessionContract::DatasetUpdateV3,
            "source_storage_requires_explicit_UPDATE_v3_session"
        );
        ensure!(
            self.rdflib_projection.native_projection()? == self.dataset,
            "source_storage_projection_core_mismatch"
        );
        let state = self
            .rdflib_projection
            .source_state
            .as_ref()
            .context("source_persistent_state_unavailable")?;
        let quads = self
            .rdflib_projection
            .quads
            .as_ref()
            .context("rdflib_lossless_source_projection_unavailable")?;
        ensure!(
            state.matches_projection(quads.iter().map(super::source_query::quad_input))?,
            "source_persistent_projection_mismatch"
        );
        Ok(())
    }
    /// Explicitly received NT/NQ source writers. Other formats remain pending;
    /// neither a native projection nor sorting the source set supplies a writer.
    pub(crate) fn serialize_source_nt_nq_internal(
        &mut self,
        request: &SemanticSourceSerializeRequest,
    ) -> Result<SemanticSourceSerialization> {
        self.serialize_source_observed_internal(request, None)
            .map(|v| v.0)
    }
    pub(super) fn serialize_source_observed_internal(
        &mut self,
        request: &SemanticSourceSerializeRequest,
        capture_limit: Option<usize>,
    ) -> Result<(
        SemanticSourceSerialization,
        Option<semantica_contracts::SemanticSourceSerializationWitness3>,
    )> {
        self.source_storage_admission()?;
        let format = writer_format(request)?;
        let dataset = format == "nquads";
        let limit = self.dataset_byte_limit();
        let max_quads = self.resource_profile.quads();
        let state = self.rdflib_projection.source_state.as_mut().unwrap();
        // Dataset.contexts supplies an absent default graph through graph(),
        // and explicit selected graphs share the Dataset namespace manager.
        if request.graph_name.is_some() || !state.serialization_default_registered()? {
            self.source_namespaces.initialize(true, limit, max_quads)?;
        }
        if request.graph_name.is_none()
            && !dataset
            && state.serialization_has_named_quads(max_quads, limit)?
        {
            return Err(SourceFailure::lossy(format!("Dataset contains named graphs; serializing the whole dataset as '{format}' would lose graph names. Use 'trig' or 'nquads', or select graph_name explicitly.")).into());
        }
        let mut text = String::new();
        let mut rows = Vec::new();
        let mut captured = 2usize; // JSON array delimiters; full step checked again.
        state.visit_serialization_rows(
            dataset,
            request.graph_name.as_deref(),
            max_quads,
            limit,
            |(t, g)| {
                let row = super::source_storage_witness::writer_row(t, g, dataset);
                if let Some(budget) = capture_limit {
                    let witness = super::source_storage_witness::owned_row(t, g);
                    let bytes = super::source_storage_witness::size(
                        &witness,
                        budget.saturating_sub(captured),
                    )
                    .map_err(|_| {
                        QueryEvaluationError::Unexpected(Box::new(
                            super::source_storage_witness::WitnessCaptureLimit,
                        ))
                    })?;
                    captured = captured
                        .checked_add(bytes + 1)
                        .filter(|v| *v <= budget)
                        .ok_or_else(|| {
                            QueryEvaluationError::Unexpected(Box::new(
                                super::source_storage_witness::WitnessCaptureLimit,
                            ))
                        })?;
                    rows.push(witness);
                }
                if text.len().checked_add(row.len()).is_none_or(|n| n > limit) {
                    return Err(QueryEvaluationError::Unexpected(
                        "source_serialize_byte_budget".into(),
                    ));
                }
                text.push_str(&row);
                Ok(())
            },
        )?;
        if dataset {
            ensure!(text.len() < limit, "source_serialize_byte_budget");
            text.push('\n');
        }
        let warnings =
            super::source_storage_witness::writer_warnings(dataset, request.base_uri.is_some());
        Ok((
            SemanticSourceSerialization { text, warnings },
            capture_limit
                .map(|_| semantica_contracts::SemanticSourceSerializationWitness3 { rows }),
        ))
    }

    /// Original source DTO integrity and algorithm precedence, without altering
    /// the existing v2-only snapshot output or RDFC diff input domain.
    pub(crate) fn diff_source_internal(
        &self,
        before: SourceSnapshotInput<'_>,
        after: Option<SourceSnapshotInput<'_>>,
        created_at: &str,
    ) -> Result<SemanticSourceDatasetDiff> {
        self.diff_source_observed_internal(before, after, created_at)
            .map(|v| v.0)
    }
    pub(super) fn diff_source_observed_internal(
        &self,
        before: SourceSnapshotInput<'_>,
        after: Option<SourceSnapshotInput<'_>>,
        created_at: &str,
    ) -> Result<(
        SemanticSourceDatasetDiff,
        Option<semantica_contracts::SemanticRdfLibDatasetSnapshot>,
    )> {
        self.source_storage_admission()?;
        let SourceSnapshotInput::Snapshot(before) = before else {
            return Err(SourceFailure::input("before must be a DatasetSnapshotDTO").into());
        };
        before
            .validate()
            .map_err(|v| anyhow::anyhow!("source_snapshot_argument_invalid:{v:?}"))?;
        let current = if after.is_none() {
            Some(self.snapshot_rdflib(created_at)?)
        } else {
            None
        };
        let current_argument = current.as_ref().map(source_snapshot_argument);
        let after = match after {
            Some(SourceSnapshotInput::Snapshot(v)) => v,
            Some(SourceSnapshotInput::NotSnapshot) => {
                return Err(SourceFailure::input("after must be a DatasetSnapshotDTO").into());
            }
            None => current_argument.as_ref().unwrap(),
        };
        let result = diff_snapshot_values(before, after, created_at, self.dataset_byte_limit())?;
        Ok((result, current))
    }
}
/// The original snapshot integrity, splitlines and set-difference algorithm.
/// Sharing this function verifies complete result relations without RDF replay.
pub(super) fn diff_snapshot_values(
    before: &SemanticSourceSnapshotArgument,
    after: &SemanticSourceSnapshotArgument,
    created_at: &str,
    limit: usize,
) -> Result<SemanticSourceDatasetDiff> {
    before
        .validate()
        .map_err(|v| anyhow::anyhow!("source_snapshot_argument_invalid:{v:?}"))?;
    after
        .validate()
        .map_err(|v| anyhow::anyhow!("source_snapshot_argument_invalid:{v:?}"))?;
    ensure!(
        before.canonical_nquads.len() <= limit && after.canonical_nquads.len() <= limit,
        "source_diff_byte_budget"
    );
    if !integrity(before) || !integrity(after) {
        return Err(
            SourceFailure::input("cannot diff a dataset snapshot with invalid integrity").into(),
        );
    }
    if before.algorithm != after.algorithm {
        return Err(SourceFailure::input(
            "cannot diff dataset snapshots from different algorithms",
        )
        .into());
    }
    let before_lines: BTreeSet<_> = splitlines(&before.canonical_nquads).into_iter().collect();
    let after_lines: BTreeSet<_> = splitlines(&after.canonical_nquads).into_iter().collect();
    let added_quads: Vec<_> = after_lines
        .difference(&before_lines)
        .map(|v| (*v).to_owned())
        .collect();
    let removed_quads: Vec<_> = before_lines
        .difference(&after_lines)
        .map(|v| (*v).to_owned())
        .collect();
    Ok(SemanticSourceDatasetDiff {
        schema_version: "1.0".into(),
        before_sha256: before.dataset_sha256.clone(),
        after_sha256: after.dataset_sha256.clone(),
        unchanged_count: before_lines.intersection(&after_lines).count() as u64,
        changed: !added_quads.is_empty() || !removed_quads.is_empty(),
        added_quads,
        removed_quads,
        created_at: when(created_at),
    })
}
