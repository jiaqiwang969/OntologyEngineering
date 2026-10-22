//! Same-owner storage relations for explicit session /3. No Dataset is created,
//! no input graph is replayed, and this does not authenticate witness provenance.
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticRdfLibDatasetSnapshot, SemanticSourceDatasetDiff, SemanticSourceSerialization,
    SemanticSourceSerializationWitness3, SemanticSourceSerializeRequest,
    SemanticSourceStorageTerm3 as Term, SemanticSourceWriterRow3 as Row,
};
use serde_json::Value;
use spareval::SourceTermInput;
use std::io::Write;
use std::time::Instant;
/// Count actual serialized bytes without a second unbounded result allocation.
struct Counter {
    bytes: usize,
    limit: usize,
    deadline: Option<Instant>,
}
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.deadline.is_some_and(|at| Instant::now() >= at) {
            return Err(std::io::Error::other("source_session_receiving_deadline"));
        }
        if bytes.len() > self.limit.saturating_sub(self.bytes) {
            return Err(std::io::Error::other("source_session_result_byte_budget"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn size(value: &impl serde::Serialize, limit: usize) -> Result<usize> {
    let mut counter = Counter {
        bytes: 0,
        limit,
        deadline: None,
    };
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.bytes)
}

pub(super) fn size_until(
    value: &impl serde::Serialize,
    limit: usize,
    at: Instant,
) -> Result<usize> {
    deadline(at)?;
    let mut counter = Counter {
        bytes: 0,
        limit,
        deadline: Some(at),
    };
    serde_json::to_writer(&mut counter, value)?;
    deadline(at)?;
    Ok(counter.bytes)
}

#[derive(Debug)]
pub(super) struct WitnessCaptureLimit;
impl std::fmt::Display for WitnessCaptureLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("source_session_result_byte_budget")
    }
}
impl std::error::Error for WitnessCaptureLimit {}

pub(super) fn capture_exhausted(error: &anyhow::Error) -> bool {
    error.chain().any(|e| e.is::<WitnessCaptureLimit>() ||
        matches!(e.downcast_ref::<spareval::QueryEvaluationError>(),
            Some(spareval::QueryEvaluationError::Unexpected(inner)) if inner.is::<WitnessCaptureLimit>()))
}

pub(super) fn owned_term(term: SourceTermInput<'_>) -> Term {
    let (term_type, value, datatype, language) = match term {
        SourceTermInput::Iri(v) => ("iri", v, None, None),
        SourceTermInput::Blank(v) => ("blank_node", v, None, None),
        SourceTermInput::Variable(v) => ("variable", v, None, None),
        SourceTermInput::Literal {
            value,
            datatype,
            language,
        } => ("literal", value, datatype, language),
    };
    Term {
        term_type: term_type.into(),
        value: value.into(),
        datatype: datatype.map(str::to_owned),
        language: language.map(str::to_owned),
    }
}
pub(super) fn borrowed(term: &Term) -> Result<SourceTermInput<'_>> {
    ensure!(
        term.validate().is_ok(),
        "source_session_writer_term_invalid"
    );
    if term.term_type != "literal" {
        ensure!(
            term.datatype.is_none() && term.language.is_none(),
            "source_session_writer_term_metadata"
        );
    }
    // No RDF1.1 reparse, absolute-IRI requirement, language folding, or lexical
    // normalization: original generalized source terms retain their full domain.
    Ok(match term.term_type.as_str() {
        "iri" => SourceTermInput::Iri(&term.value),
        "blank_node" => SourceTermInput::Blank(&term.value),
        "variable" => SourceTermInput::Variable(&term.value),
        "literal" => SourceTermInput::Literal {
            value: &term.value,
            datatype: term.datatype.as_deref(),
            language: term.language.as_deref(),
        },
        _ => anyhow::bail!("source_session_writer_term_kind"),
    })
}
pub(super) fn owned_row(t: [SourceTermInput<'_>; 3], graph: Option<SourceTermInput<'_>>) -> Row {
    Row {
        subject: owned_term(t[0]),
        predicate: owned_term(t[1]),
        object: owned_term(t[2]),
        graph: graph.map(owned_term),
    }
}
pub(super) fn writer_row(
    t: [SourceTermInput<'_>; 3],
    graph: Option<SourceTermInput<'_>>,
    dataset: bool,
) -> String {
    use super::source_storage::{object_nt, term_n3};
    if dataset {
        format!(
            "{} {} {} {} .\n",
            term_n3(t[0]),
            term_n3(t[1]),
            object_nt(t[2]),
            graph.map(term_n3).unwrap_or_default()
        )
    } else {
        format!(
            "{} {} {} .\n",
            term_n3(t[0]),
            term_n3(t[1]),
            object_nt(t[2])
        )
    }
}
pub(super) fn writer_warnings(dataset: bool, has_base: bool) -> Vec<String> {
    if has_base {
        vec![
            if dataset {
                "NQuadsSerializer does not support base."
            } else {
                "NTSerializer does not support base."
            }
            .into(),
        ]
    } else {
        Vec::new()
    }
}
fn deadline(at: Instant) -> Result<()> {
    ensure!(Instant::now() < at, "source_session_receiving_deadline");
    Ok(())
}
fn key(v: &Value) -> Result<String> {
    super::source_observation::observer_key(v, false).map_err(anyhow::Error::msg)
}
/// Compare full original writer text/warnings and complete selected quad
/// content. Sorting is applied only to coverage keys, never the writer rows.
pub(super) fn check_serialization(
    request: &SemanticSourceSerializeRequest,
    witness: &SemanticSourceSerializationWitness3,
    result: &SemanticSourceSerialization,
    source_state: Option<&Value>,
    at: Instant,
) -> Result<()> {
    let dataset = super::source_storage::writer_format(request)? == "nquads";
    let mut text = String::new();
    let mut observed_rows = Vec::new();
    for row in &witness.rows {
        deadline(at)?;
        let terms = [
            borrowed(&row.subject)?,
            borrowed(&row.predicate)?,
            borrowed(&row.object)?,
        ];
        let graph = row.graph.as_ref().map(borrowed).transpose()?;
        let line = writer_row(terms, graph, dataset);
        ensure!(
            text.len()
                .checked_add(line.len())
                .is_some_and(|n| n <= crate::MAX_BYTES),
            "source_serialize_byte_budget"
        );
        text.push_str(&line);
        let mut observed = Vec::with_capacity(4);
        for term in terms {
            observed.push(super::source_observation::present_source_term(term)?);
        }
        observed.push(match graph {
            None => Value::Null,
            Some(term) => super::source_observation::present_source_term(term)?,
        });
        observed_rows.push(key(&Value::Array(observed))?);
    }
    if dataset {
        ensure!(
            text.len() < crate::MAX_BYTES,
            "source_serialize_byte_budget"
        );
        text.push('\n');
    }
    ensure!(
        result.text == text
            && result.warnings == writer_warnings(dataset, request.base_uri.is_some()),
        "source_session_writer_content_mismatch"
    );
    let Some(source_state) = source_state else {
        return Ok(());
    };
    size(source_state, crate::MAX_BYTES)?;
    let selected = match request.graph_name.as_deref() {
        None | Some("urn:x-rdflib:default") => Value::Null,
        Some(name) => super::source_observation::present_source_term(SourceTermInput::Iri(name))?,
    };
    let rows = source_state["quads"]
        .as_array()
        .context("source_session_writer_source_state_unavailable")?;
    let mut expected = Vec::new();
    for row in rows {
        deadline(at)?;
        let fields = row
            .as_array()
            .filter(|v| v.len() == 4)
            .context("source_session_writer_state_quad")?;
        if dataset || fields[3] == selected {
            expected.push(key(row)?);
        }
        if !dataset && request.graph_name.is_none() {
            ensure!(fields[3].is_null(), "source_session_writer_lossy_receipt");
        }
    }
    observed_rows.sort();
    expected.sort();
    ensure!(
        observed_rows == expected,
        "source_session_writer_source_coverage"
    );
    Ok(())
}
/// Reuse the exact original diff algorithm for all fields; this is a relation
/// check over actual retained snapshots, not a new graph evaluator.
pub(super) fn check_diff(
    before: &SemanticRdfLibDatasetSnapshot,
    after: &SemanticRdfLibDatasetSnapshot,
    result: &SemanticSourceDatasetDiff,
    requested_time: &str,
    at: Instant,
) -> Result<()> {
    deadline(at)?;
    if !requested_time.is_empty() {
        ensure!(
            result.created_at == requested_time,
            "source_session_diff_time_identity"
        );
    } else {
        chrono::DateTime::parse_from_rfc3339(&result.created_at)
            .context("source_session_diff_time_observation")?;
    }
    let expected = super::source_storage::diff_snapshot_values(
        &super::source_storage::source_snapshot_argument(before),
        &super::source_storage::source_snapshot_argument(after),
        &result.created_at,
        crate::MAX_BYTES,
    )?;
    deadline(at)?;
    ensure!(&expected == result, "source_session_diff_content_mismatch");
    Ok(())
}

/// Explicit caller time is an exact original value, not a newly restricted
/// date field. Empty input retains the original owner's actual RFC3339 reading.
pub(super) fn check_snapshot_time(
    snapshot: &SemanticRdfLibDatasetSnapshot,
    requested_time: &str,
) -> Result<()> {
    if !requested_time.is_empty() {
        ensure!(
            snapshot.created_at == requested_time,
            "source_session_diff_current_time_identity"
        );
    } else {
        chrono::DateTime::parse_from_rfc3339(&snapshot.created_at)
            .context("source_session_diff_current_time_observation")?;
    }
    Ok(())
}

/// The full typed state is observed in the same existing visit as source_state.
/// Projection equality precedes canonicalization, so malformed/error DTO text
/// cannot be interpreted as a new source fact. Empty contexts remain covered.
pub(super) fn check_current_snapshot(
    typed: &semantica_contracts::SemanticSourceStateWitness3,
    state: &Value,
    snapshot: &SemanticRdfLibDatasetSnapshot,
    at: Instant,
) -> Result<()> {
    let quads = check_state_observation(typed, state, at)?;
    deadline(at)?;
    let expected =
        crate::rdflib_snapshot::snapshot(&quads, snapshot.revision, &snapshot.created_at)?;
    deadline(at)?;
    ensure!(
        &expected == snapshot,
        "source_session_diff_current_snapshot_content"
    );
    Ok(())
}

pub(super) fn check_state_observation(
    typed: &semantica_contracts::SemanticSourceStateWitness3,
    state: &Value,
    at: Instant,
) -> Result<std::collections::BTreeSet<crate::rdflib_snapshot::Quad>> {
    size(state, crate::MAX_BYTES)?;
    ensure!(
        typed.quads.len() <= super::MAX_QUADS,
        "rdflib_snapshot_quad_budget"
    );
    let mut observations = Vec::new();
    let mut quads = std::collections::BTreeSet::new();
    for row in &typed.quads {
        deadline(at)?;
        let terms = [
            borrowed(&row.subject)?,
            borrowed(&row.predicate)?,
            borrowed(&row.object)?,
        ];
        let graph = row.graph.as_ref().map(borrowed).transpose()?;
        let mut observed = Vec::with_capacity(4);
        for term in terms {
            observed.push(super::source_observation::present_source_term(term)?);
        }
        observed.push(match graph {
            None => Value::Null,
            Some(term) => super::source_observation::present_source_term(term)?,
        });
        observations.push(Value::Array(observed));
        quads.insert((
            super::source_update::node(terms[0]),
            super::source_update::node(terms[1]),
            super::source_update::node(terms[2]),
            graph.map(super::source_update::node),
        ));
    }
    let mut contexts = Vec::new();
    for term in &typed.contexts {
        deadline(at)?;
        contexts.push(super::source_observation::present_source_term(borrowed(
            term,
        )?)?);
    }
    super::source_observation::sort_observation(&mut observations, false)
        .map_err(anyhow::Error::msg)?;
    super::source_observation::sort_observation(&mut contexts, true).map_err(anyhow::Error::msg)?;
    ensure!(
        state["quads"] == Value::Array(observations) && state["contexts"] == Value::Array(contexts),
        "source_session_diff_current_source_coverage"
    );
    ensure!(
        state["quad_count"].as_u64() == Some(typed.quads.len() as u64)
            && quads.len() == typed.quads.len(),
        "source_session_diff_current_quad_identity"
    );
    deadline(at)?;
    Ok(quads)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn source_session3_projection_retains_original_term_domain() {
        let terms = [
            SourceTermInput::Variable("slot"),
            SourceTermInput::Iri("relative"),
            SourceTermInput::Blank("Nzz"),
            SourceTermInput::Literal {
                value: "",
                datatype: None,
                language: None,
            },
            SourceTermInput::Literal {
                value: "same",
                datatype: Some("http://www.w3.org/2001/XMLSchema#string"),
                language: None,
            },
            SourceTermInput::Literal {
                value: "language",
                datatype: Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"),
                language: Some("EN-us"),
            },
        ];
        for term in terms {
            let owned = owned_term(term);
            let rebound = borrowed(&owned).unwrap();
            assert_eq!(
                super::super::source_observation::present_source_term(term).unwrap(),
                super::super::source_observation::present_source_term(rebound).unwrap()
            );
        }
        assert_eq!(
            super::super::source_observation::present_source_term(terms[0]).unwrap(),
            json!({"backend_term":{"module":"rdflib.term","type":"Variable","text":"slot","n3":"?slot"},"dto_error":{"module":"semantica.ontology.runtime","type":"SemanticRuntimeError","message":"Unsupported RDF term returned by backend: Variable"}})
        );
        assert_eq!(
            super::super::source_observation::present_source_term(terms[1]).unwrap(),
            json!({"value":"relative","term_type":"iri","datatype":null,"language":null})
        );
        assert_eq!(
            writer_row([terms[0], terms[1], terms[3]], None, false),
            "?slot <relative> \"\" .\n"
        );
        assert_eq!(
            writer_row([terms[2], terms[1], terms[5]], None, false),
            "_:Nzz <relative> \"language\"@EN-us .\n"
        );
        assert!(capture_exhausted(&anyhow::Error::new(
            spareval::QueryEvaluationError::Unexpected(Box::new(WitnessCaptureLimit))
        )));
    }

    #[test]
    fn source_session3_snapshot_time_keeps_original_observed_format() {
        // Batch Request1/2/3 still require nonempty created_at. This tests the
        // existing underlying owner seam; it does not expand batch admission.
        let session = crate::dataset::DatasetSession::new();
        let observed = session.snapshot_rdflib("").unwrap();
        check_snapshot_time(&observed, "").unwrap();
        let mut bad = observed.clone();
        bad.created_at = "not-a-time".into();
        assert_eq!(
            check_snapshot_time(&bad, "").unwrap_err().to_string(),
            "source_session_diff_current_time_observation"
        );
        let explicit = session
            .snapshot_rdflib("original-explicit-time-label")
            .unwrap();
        check_snapshot_time(&explicit, "original-explicit-time-label").unwrap();
        assert_eq!(
            check_snapshot_time(&explicit, "different-label")
                .unwrap_err()
                .to_string(),
            "source_session_diff_current_time_identity"
        );
    }
}
