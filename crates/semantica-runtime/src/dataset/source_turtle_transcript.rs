//! Successful Turtle relations, not an execution attestation or a new Dataset.
use super::{
    DatasetSession,
    source_namespace::NamespaceState,
    source_storage_witness::{borrowed, owned_row, owned_term, size_until},
    source_turtle::{PreparedTurtle, prepare_names},
    source_turtle_read::{TurtleRead, TurtleTerm},
    source_turtle_writer::Writer,
};
use anyhow::{Context, Result, ensure};
use semantica_contracts::{
    SemanticSourceSerializeRequest as Request, SemanticSourceStorageTerm3 as Term,
    SemanticSourceTurtleRead4 as Read, SemanticSourceTurtleWitness4 as Witness,
    SemanticSourceWriterRow3 as Row,
};
use spareval::{SourceTermInput, SourceTurtleTerm, SourceTurtleView};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    time::Instant,
};

const CAPTURE_BYTES: usize = 32 * 1024 * 1024;
const WORK: usize = 2_000_000;
fn deadline(at: Instant) -> Result<()> {
    ensure!(Instant::now() < at, "source_turtle_deadline");
    Ok(())
}
struct Budget {
    left: Cell<usize>,
    at: Instant,
}
impl Budget {
    fn take(&self, value: &impl serde::Serialize) -> Result<()> {
        let n = size_until(value, self.left.get(), self.at)?;
        self.left.set(
            self.left
                .get()
                .checked_sub(n + 1)
                .context("source_turtle_capture_budget")?,
        );
        Ok(())
    }
}
fn term<T: TurtleTerm>(value: &T) -> Result<Term> {
    Ok(owned_term(value.term()?))
}
fn row<T: TurtleTerm>(values: &[T; 3]) -> Result<Row> {
    Ok(owned_row(
        [values[0].term()?, values[1].term()?, values[2].term()?],
        None,
    ))
}
fn empty_read(operation: &str) -> Read {
    Read {
        operation: operation.into(),
        arguments: vec![],
        subject: None,
        predicate: None,
        object: None,
        rows: vec![],
        answer: None,
    }
}
struct Recording<'a> {
    graph: SourceTurtleView,
    log: &'a RefCell<Vec<Read>>,
    budget: &'a Budget,
}
impl Recording<'_> {
    fn push(&self, event: Read) -> Result<()> {
        self.budget.take(&event)?;
        self.log.borrow_mut().push(event);
        Ok(())
    }
    fn scalar(&self, op: &str, arguments: &[&SourceTurtleTerm], answer: bool) -> Result<bool> {
        let mut event = empty_read(op);
        event.arguments = arguments.iter().map(|v| term(*v)).collect::<Result<_>>()?;
        event.answer = Some(answer);
        self.push(event)?;
        Ok(answer)
    }
}
impl TurtleRead for Recording<'_> {
    type Term = SourceTurtleTerm;
    fn charge(&self, n: usize) -> Result<()> {
        deadline(self.budget.at)?;
        Ok(self.graph.charge(n)?)
    }
    fn iri(&self, value: &str) -> Result<Self::Term> {
        deadline(self.budget.at)?;
        Ok(self.graph.iri(value)?)
    }
    fn scan(&self, pattern: [Option<&Self::Term>; 3]) -> Result<Vec<[Self::Term; 3]>> {
        deadline(self.budget.at)?;
        let values = self.graph.scan(pattern)?;
        let mut event = empty_read("scan");
        event.subject = pattern[0].map(term).transpose()?;
        event.predicate = pattern[1].map(term).transpose()?;
        event.object = pattern[2].map(term).transpose()?;
        // Check each retained row while copying, not only after a large scan.
        let mut remaining = self.budget.left.get();
        for v in &values {
            deadline(self.budget.at)?;
            let r = row(v)?;
            let n = size_until(&r, remaining, self.budget.at)?;
            remaining = remaining
                .checked_sub(n + 1)
                .context("source_turtle_capture_budget")?;
            event.rows.push(r);
        }
        self.push(event)?;
        Ok(values)
    }
    fn truth(&self, value: &Self::Term) -> Result<bool> {
        self.scalar("truth", &[value], self.graph.truth(value)?)
    }
    fn cached(&self, value: &Self::Term) -> Result<bool> {
        self.scalar("cached", &[value], value.has_cached_value()?)
    }
    fn less(&self, a: &Self::Term, b: &Self::Term) -> Result<bool> {
        self.scalar("less", &[a, b], self.graph.less(a, b)?)
    }
}
fn node_term(value: &crate::rdflib_snapshot::Node) -> Term {
    use crate::rdflib_snapshot::Node;
    owned_term(match value {
        Node::Iri(v) => SourceTermInput::Iri(v),
        Node::Blank(v) => SourceTermInput::Blank(v),
        Node::Variable(v) => SourceTermInput::Variable(v),
        Node::Literal(v) => SourceTermInput::Literal {
            value: &v.text,
            datatype: v.datatype.as_deref(),
            language: v.language.as_deref(),
        },
    })
}
impl DatasetSession {
    /// No complete witness is returned after original failure, capture failure
    /// or resource refusal. Actual namespace/graph effects are never rolled back.
    pub(super) fn capture_turtle(
        &mut self,
        request: &Request,
        limit: usize,
        at: Instant,
    ) -> Result<(String, Witness)> {
        deadline(at)?;
        self.source_storage_admission()?;
        ensure!(
            matches!(self.resource_profile, super::ResourceProfile::Default),
            "source_turtle_witness_profile"
        );
        let limit = limit.min(CAPTURE_BYTES);
        let budget = Budget {
            left: Cell::new(
                limit
                    .checked_sub(1024)
                    .context("source_turtle_capture_budget")?,
            ),
            at,
        };
        let before = self
            .source_namespaces
            .capture_until(budget.left.get(), at)?;
        budget.take(&before)?;
        let (bytes_before, entries_before) = self.source_namespaces.retained_charge();
        let registered = self
            .rdflib_projection
            .source_state
            .as_ref()
            .context("source_state_missing")?
            .serialization_default_registered()?;
        let prepared = self.prepare_source_turtle_until(request, Some(at))?;
        let mut preparation_rows = Vec::new();
        for [s, p, o] in &prepared.rows {
            deadline(at)?;
            let row = Row {
                subject: node_term(s),
                predicate: node_term(p),
                object: node_term(o),
                graph: None,
            };
            budget.take(&row)?;
            preparation_rows.push(row);
        }
        let prepared_names = self
            .source_namespaces
            .capture_until(budget.left.get(), at)?;
        budget.take(&prepared_names)?;
        let bytes = self.dataset_byte_limit();
        let entries = self.resource_profile.quads();
        let graph = self
            .rdflib_projection
            .source_state
            .as_mut()
            .unwrap()
            .turtle_view_until(
                request.graph_name.as_deref(),
                entries,
                bytes,
                WORK,
                Some(at),
            )?;
        let log = RefCell::new(Vec::new());
        let text = Writer::new(
            prepared,
            Recording {
                graph,
                log: &log,
                budget: &budget,
            },
            &mut self.source_namespaces,
            request.graph_name.is_some(),
            bytes,
            entries,
        )
        .render()?;
        budget.take(&text)?;
        let after = self
            .source_namespaces
            .capture_until(budget.left.get(), at)?;
        budget.take(&after)?;
        let (bytes_after, entries_after) = self.source_namespaces.retained_charge();
        let witness = Witness {
            schema: "semantica.source-turtle-witness/4".into(),
            request_sha256: crate::sha256(&serde_json::to_vec(request)?),
            text_sha256: crate::sha256(text.as_bytes()),
            default_registered_before: registered,
            preparation_rows,
            namespace_before: before,
            namespace_prepared: prepared_names,
            namespace_after: after,
            namespace_bytes_before: bytes_before as u64,
            namespace_entries_before: entries_before as u64,
            namespace_bytes_after: bytes_after as u64,
            namespace_entries_after: entries_after as u64,
            reads: log.into_inner(),
        };
        size_until(&(&text, &witness), limit, at)?;
        Ok((text, witness))
    }
}

fn row_key(r: &Row) -> Result<String> {
    for t in [&r.subject, &r.predicate, &r.object] {
        borrowed(t)?;
    }
    ensure!(r.graph.is_none(), "source_turtle_selected_row_graph");
    Ok(serde_json::to_string(&(
        &r.subject,
        &r.predicate,
        &r.object,
    ))?)
}
fn keys<'a>(rows: impl Iterator<Item = &'a Row>, at: Instant) -> Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    for row in rows {
        deadline(at)?;
        ensure!(result.insert(row_key(row)?), "source_turtle_duplicate_row");
    }
    Ok(result)
}
struct Replay<'a> {
    reads: &'a [Read],
    rows: &'a [Row],
    cursor: Cell<usize>,
    work: Cell<usize>,
    at: Instant,
}
impl Replay<'_> {
    fn next(&self, kind: &str) -> Result<&Read> {
        deadline(self.at)?;
        let v = self
            .reads
            .get(self.cursor.get())
            .context("source_turtle_missing_read")?;
        ensure!(
            v.validate().is_ok() && v.operation == kind,
            "source_turtle_read_operation"
        );
        self.cursor.set(self.cursor.get() + 1);
        Ok(v)
    }
    fn scalar(&self, kind: &str, args: &[&Term]) -> Result<bool> {
        let v = self.next(kind)?;
        ensure!(
            v.subject.is_none() && v.predicate.is_none() && v.object.is_none() && v.rows.is_empty(),
            "source_turtle_scalar_fields"
        );
        ensure!(
            v.arguments.len() == args.len() && v.arguments.iter().zip(args).all(|(a, b)| a == *b),
            "source_turtle_scalar_arguments"
        );
        v.answer.context("source_turtle_scalar_answer")
    }
    fn finish(&self) -> Result<()> {
        deadline(self.at)?;
        ensure!(
            self.cursor.get() == self.reads.len(),
            "source_turtle_extra_reads"
        );
        Ok(())
    }
}
impl TurtleRead for &Replay<'_> {
    type Term = Term;
    fn charge(&self, n: usize) -> Result<()> {
        deadline(self.at)?;
        self.work.set(
            self.work
                .get()
                .checked_sub(n)
                .context("source_turtle_work_budget")?,
        );
        Ok(())
    }
    fn iri(&self, value: &str) -> Result<Term> {
        deadline(self.at)?;
        Ok(owned_term(SourceTermInput::Iri(value)))
    }
    fn scan(&self, pattern: [Option<&Term>; 3]) -> Result<Vec<[Term; 3]>> {
        self.charge(1)?;
        let v = self.next("scan")?;
        ensure!(
            v.arguments.is_empty() && v.answer.is_none(),
            "source_turtle_scan_fields"
        );
        ensure!(
            [v.subject.as_ref(), v.predicate.as_ref(), v.object.as_ref()] == pattern,
            "source_turtle_scan_pattern"
        );
        let matches = |r: &&Row| -> bool {
            [&r.subject, &r.predicate, &r.object]
                .into_iter()
                .zip(pattern)
                .all(|(a, b)| {
                    b.is_none_or(|b| {
                        // Original term equality is distinct from SPARQL value order.
                        match (borrowed(a), borrowed(b)) {
                            (Ok(a), Ok(b)) => {
                                super::source_update::node(a) == super::source_update::node(b)
                            }
                            _ => false,
                        }
                    })
                })
        };
        let mut matching = Vec::new();
        for r in self.rows {
            // Receiving scans the supplied coverage, so charge that real work
            // too; an unmatched pattern must still poll the existing deadline.
            self.charge(1)?;
            if matches(&r) {
                matching.push(r);
            }
        }
        let expected = keys(matching.into_iter(), self.at)?;
        ensure!(
            keys(v.rows.iter(), self.at)? == expected,
            "source_turtle_scan_coverage"
        );
        if pattern.iter().all(Option::is_none) {
            ensure!(v.rows == self.rows, "source_turtle_full_scan_order");
        }
        self.charge(v.rows.len())?;
        Ok(v.rows
            .iter()
            .map(|r| [r.subject.clone(), r.predicate.clone(), r.object.clone()])
            .collect())
    }
    fn truth(&self, v: &Term) -> Result<bool> {
        let answer = self.scalar("truth", &[v])?;
        if v.term_type != "literal" {
            ensure!(
                answer == !v.value.is_empty(),
                "source_turtle_truth_relation"
            );
        }
        Ok(answer)
    }
    fn cached(&self, v: &Term) -> Result<bool> {
        ensure!(v.term_type == "literal", "source_turtle_cache_term");
        self.scalar("cached", &[v])
    }
    fn less(&self, a: &Term, b: &Term) -> Result<bool> {
        self.scalar("less", &[a, b])
    }
}

/// Checks complete successful result/state/read relations. The supplied source
/// and observations still need external request/worker/source authentication.
pub(super) fn validate_turtle_witness(
    request: &Request,
    text: &str,
    w: &Witness,
    source: &[Row],
    limit: usize,
    at: Instant,
) -> Result<()> {
    deadline(at)?;
    let limit = limit.min(CAPTURE_BYTES);
    size_until(&(text, w), limit, at)?;
    size_until(&source, 64 * 1024 * 1024, at)?;
    ensure!(
        request.validate().is_ok() && w.validate().is_ok(),
        "source_turtle_witness_contract"
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
    ensure!(
        w.request_sha256 == crate::sha256(&serde_json::to_vec(request)?),
        "source_turtle_request_binding"
    );
    ensure!(
        w.text_sha256 == crate::sha256(text.as_bytes()),
        "source_turtle_text_binding"
    );
    let mut selected = Vec::new();
    for r in source {
        deadline(at)?;
        for t in [&r.subject, &r.predicate, &r.object]
            .into_iter()
            .chain(r.graph.iter())
        {
            borrowed(t)?;
        }
        ensure!(
            request.graph_name.is_some() || r.graph.is_none(),
            "source_turtle_lossy_witness"
        );
        let selected_graph = match (request.graph_name.as_deref(), &r.graph) {
            (None | Some("urn:x-rdflib:default"), None) => true,
            (Some(name), Some(g)) => g.term_type == "iri" && g.value == name,
            _ => false,
        };
        if selected_graph {
            let mut r = r.clone();
            r.graph = None;
            selected.push(r);
        }
    }
    ensure!(
        keys(selected.iter(), at)? == keys(w.preparation_rows.iter(), at)?,
        "source_turtle_preparation_coverage"
    );
    let nb = usize::try_from(w.namespace_bytes_before)?;
    let ne = usize::try_from(w.namespace_entries_before)?;
    let na = usize::try_from(w.namespace_bytes_after)?;
    let ea = usize::try_from(w.namespace_entries_after)?;
    ensure!(
        nb <= crate::MAX_BYTES
            && na <= crate::MAX_BYTES
            && ne <= super::MAX_QUADS
            && ea <= super::MAX_QUADS,
        "source_turtle_namespace_charge"
    );
    let mut names = NamespaceState::checking_projection(&w.namespace_before, nb, ne, limit, at)?;
    if request.graph_name.is_some() || !w.default_registered_before {
        names.initialize(true, crate::MAX_BYTES, super::MAX_QUADS)?;
    }
    let mut rows = Vec::new();
    for r in &w.preparation_rows {
        deadline(at)?;
        rows.push([
            super::source_update::node(borrowed(&r.subject)?),
            super::source_update::node(borrowed(&r.predicate)?),
            super::source_update::node(borrowed(&r.object)?),
        ]);
    }
    let prepared_names = prepare_names(
        &rows,
        &mut names,
        request,
        crate::MAX_BYTES,
        super::MAX_QUADS,
        Some(at),
    )?;
    ensure!(
        names.capture_until(limit, at)? == w.namespace_prepared,
        "source_turtle_prepared_namespace"
    );
    let replay = Replay {
        reads: &w.reads,
        rows: &w.preparation_rows,
        cursor: Cell::new(0),
        work: Cell::new(WORK),
        at,
    };
    let prepared = PreparedTurtle {
        rows,
        names: prepared_names,
        base: request.base_uri.clone(),
    };
    let actual = Writer::new(
        prepared,
        &replay,
        &mut names,
        request.graph_name.is_some(),
        crate::MAX_BYTES,
        super::MAX_QUADS,
    )
    .render()?;
    replay.finish()?;
    ensure!(actual == text, "source_turtle_text_relation");
    ensure!(
        names.capture_until(limit, at)? == w.namespace_after && names.retained_charge() == (na, ea),
        "source_turtle_after_namespace"
    );
    deadline(at)
}

#[cfg(test)]
#[path = "source_turtle_transcript_receiving.rs"]
mod receiving;
