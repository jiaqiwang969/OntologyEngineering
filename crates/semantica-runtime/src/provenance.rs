//! Source-byte integrity and a bounded PROV-O projection over the sole DatasetSession.
//! This profile neither establishes historic truth nor changes engineering adoption.
use crate::dataset::DatasetSession;
use anyhow::{Context, Result, ensure};
use oxigraph::{
    io::RdfFormat,
    model::{Dataset, GraphName, Literal, NamedNode, Quad, Term},
};
use semantica_contracts::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const PROFILE: &str = "semantica.prov-source-span/1";
const SPAN_PROFILE: &str = "semantica.source-utf8-span-sha256/1";
const PROV: &str = "http://www.w3.org/ns/prov#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const NS: &str = "urn:semantica:provenance:";
const MAX_QUADS: u64 = 100_000;

/// A trusted local capability, deliberately without public construction or
/// Deserialize. Its public receipt is an observation, never trusted input.
pub struct VerifiedSourceSpan {
    proof: SemanticSourceSpanProof,
    quote: String,
}
impl VerifiedSourceSpan {
    pub fn receipt(&self) -> &SemanticSourceSpanProof {
        &self.proof
    }
}
fn valid<T>(result: std::result::Result<(), T>) -> Result<()> {
    ensure!(result.is_ok(), "provenance_contract_invalid");
    Ok(())
}
fn iri(value: &str) -> Result<NamedNode> {
    Ok(NamedNode::new(value).context("provenance_iri_invalid")?)
}
fn canonical_value(value: Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_value(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_value).collect()),
        other => other,
    }
}
fn canonical_json(value: impl Serialize) -> Result<String> {
    struct ByteBudget(usize);
    impl std::io::Write for ByteBudget {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("provenance_request_byte_budget"))?;
            if self.0 > crate::MAX_BYTES {
                return Err(std::io::Error::other("provenance_request_byte_budget"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(ByteBudget(0), &value).context("provenance_request_byte_budget")?;
    Ok(serde_json::to_string(&canonical_value(
        serde_json::to_value(value)?,
    ))?)
}
fn metadata_budget(value: &Value, total: &mut usize) -> Result<()> {
    let mut stack = vec![(value, 0usize)];
    while let Some((value, depth)) = stack.pop() {
        *total += 1;
        ensure!(
            *total <= 100_000 && depth <= 96,
            "provenance_metadata_structure_budget"
        );
        match value {
            Value::Array(values) => stack.extend(values.iter().map(|value| (value, depth + 1))),
            Value::Object(values) => stack.extend(values.values().map(|value| (value, depth + 1))),
            _ => {}
        }
    }
    Ok(())
}
fn timestamp(value: &str) -> Result<oxsdatatypes::DateTime> {
    let parsed: oxsdatatypes::DateTime = value.parse().context("provenance_timestamp_invalid")?;
    ensure!(
        parsed.timezone_offset().is_some(),
        "provenance_timestamp_timezone_missing"
    );
    Ok(parsed)
}
fn bounded_quads(max: u64) -> Result<()> {
    ensure!(
        (1..=MAX_QUADS).contains(&max),
        "provenance_quad_budget_invalid"
    );
    Ok(())
}
/// Verify the full source bytes before its quoted interval. Character positions
/// are not accepted as byte offsets and decoded JSON text is its own source.
pub fn verify_source_span(
    raw: &[u8],
    binding: &SemanticSourceBinding,
    span: &SemanticSourceSpan,
) -> Result<VerifiedSourceSpan> {
    valid(binding.validate())?;
    valid(span.validate())?;
    iri(&binding.source_id)?;
    iri(&span.span_id)?;
    iri(&span.source_id)?;
    ensure!(
        raw.len() <= crate::MAX_BYTES,
        "provenance_source_byte_budget"
    );
    ensure!(
        raw.len() as u64 == binding.byte_count,
        "source_byte_count_mismatch"
    );
    ensure!(
        crate::sha256(raw) == binding.source_sha256,
        "source_sha_mismatch"
    );
    ensure!(
        span.source_id == binding.source_id && span.source_sha256 == binding.source_sha256,
        "span_source_binding_mismatch"
    );
    let text = std::str::from_utf8(raw).context("provenance_source_utf8_invalid")?;
    ensure!(
        span.start_byte <= span.end_byte && span.end_byte <= binding.byte_count,
        "span_range"
    );
    let start = usize::try_from(span.start_byte)?;
    let end = usize::try_from(span.end_byte)?;
    ensure!(
        text.is_char_boundary(start) && text.is_char_boundary(end),
        "utf8_boundary"
    );
    let quote = &text[start..end];
    ensure!(
        crate::sha256(quote) == span.span_sha256,
        "span_sha_mismatch"
    );
    if let Some(expected) = &span.quote {
        ensure!(expected == quote, "quote_mismatch");
    }
    Ok(VerifiedSourceSpan {
        proof: SemanticSourceSpanProof {
            profile: SPAN_PROFILE.into(),
            binding: binding.clone(),
            span: span.clone(),
            engineering_status: "unknown".into(),
        },
        quote: quote.into(),
    })
}

/// Per-call, graph-scoped index; it owns no persistent dataset, revision or cache.
struct Index {
    quads: BTreeMap<String, Vec<Quad>>,
}
impl Index {
    fn new(dataset: &Dataset, graph: &str, max_quads: u64) -> Result<Self> {
        bounded_quads(max_quads)?;
        let selected = GraphName::NamedNode(iri(graph)?);
        let mut quads = BTreeMap::<String, Vec<Quad>>::new();
        let mut count = 0u64;
        for quad in dataset
            .iter()
            .filter(|quad| quad.graph_name == selected.as_ref())
        {
            count += 1;
            ensure!(count <= max_quads, "provenance_graph_scan_budget");
            if let oxigraph::model::NamedOrBlankNodeRef::NamedNode(subject) = quad.subject {
                quads
                    .entry(subject.as_str().into())
                    .or_default()
                    .push(quad.into_owned());
            }
        }
        for rows in quads.values_mut() {
            rows.sort_by_key(Quad::to_string);
        }
        Ok(Self { quads })
    }
    fn has(&self, subject: &str) -> bool {
        self.quads.contains_key(subject)
    }
    fn objects(&self, subject: &str, predicate: &str) -> Vec<String> {
        self.quads
            .get(subject)
            .into_iter()
            .flatten()
            .filter_map(|quad| {
                if quad.predicate.as_str() == predicate {
                    if let Term::NamedNode(node) = &quad.object {
                        return Some(node.as_str().into());
                    }
                }
                None
            })
            .collect()
    }
    fn typed(&self, subject: &str, kind: &str) -> bool {
        self.objects(subject, RDF_TYPE)
            .iter()
            .any(|value| value == &format!("{PROV}{kind}"))
    }
    fn record_subject(&self, record: &str) -> Result<Option<String>> {
        let matches = self
            .quads
            .keys()
            .filter(|subject| {
                self.objects(subject, &format!("{NS}recordId"))
                    .iter()
                    .any(|value| value == record)
            })
            .cloned()
            .collect::<Vec<_>>();
        ensure!(matches.len() <= 1, "provenance_ambiguous_record");
        if let Some(subject) = matches.first() {
            return Ok(Some(subject.clone()));
        }
        let recorded = self.typed(record, "Entity")
            && (!self
                .objects(record, &format!("{PROV}wasGeneratedBy"))
                .is_empty()
                || !self.objects(record, &format!("{NS}sourceSpan")).is_empty());
        Ok(recorded.then(|| record.into()))
    }
}
struct ProjectionBuilder {
    graph: GraphName,
    dataset: Dataset,
    max_quads: u64,
}
impl ProjectionBuilder {
    fn new(graph: &str, max_quads: u64) -> Result<Self> {
        bounded_quads(max_quads)?;
        Ok(Self {
            graph: GraphName::NamedNode(iri(graph)?),
            dataset: Dataset::new(),
            max_quads,
        })
    }
    fn term(&mut self, subject: &str, predicate: &str, object: Term) -> Result<()> {
        self.dataset.insert(&Quad::new(
            iri(subject)?,
            iri(predicate)?,
            object,
            self.graph.clone(),
        ));
        ensure!(
            self.dataset.len() as u64 <= self.max_quads,
            "provenance_projection_quad_budget"
        );
        Ok(())
    }
    fn node(&mut self, subject: &str, predicate: &str, object: &str) -> Result<()> {
        self.term(subject, predicate, iri(object)?.into())
    }
    fn text(&mut self, subject: &str, predicate: &str, value: &str) -> Result<()> {
        self.term(
            subject,
            predicate,
            Literal::new_simple_literal(value).into(),
        )
    }
    fn number(&mut self, subject: &str, predicate: &str, value: u64) -> Result<()> {
        self.term(
            subject,
            predicate,
            Literal::new_typed_literal(value.to_string(), iri(&format!("{XSD}integer"))?).into(),
        )
    }
    fn time(&mut self, subject: &str, predicate: &str, value: &str) -> Result<()> {
        timestamp(value)?;
        self.term(
            subject,
            predicate,
            Literal::new_typed_literal(value, iri(&format!("{XSD}dateTime"))?).into(),
        )
    }
    fn kind(&mut self, subject: &str, kind: &str) -> Result<()> {
        self.node(subject, RDF_TYPE, &format!("{PROV}{kind}"))
    }
    fn finish(
        self,
        graph: &str,
        input: impl Serialize,
        identities: Vec<SemanticProvenanceRecordIdentity>,
        sources: BTreeSet<String>,
    ) -> Result<SemanticProvenanceProjection> {
        let input = canonical_json(input)?;
        ensure!(
            input.len() <= crate::MAX_BYTES,
            "provenance_request_byte_budget"
        );
        let snapshot = crate::dataset::snapshot(&self.dataset, 0)?;
        Ok(SemanticProvenanceProjection {
            schema: "semantica.provenance-projection/1".into(),
            profile: PROFILE.into(),
            graph_id: graph.into(),
            nquads: snapshot.canonical_nquads,
            record_identities: identities,
            input_sha256: crate::sha256(input),
            source_sha256: sources.into_iter().collect(),
            coverage: "explicit_slice".into(),
            engineering_status: "unknown".into(),
        })
    }
}
fn unique_role(reserved: &mut BTreeSet<String>, id: &str) -> Result<()> {
    iri(id)?;
    ensure!(
        reserved.insert(id.into()),
        "provenance_identity_role_collision:{id}"
    );
    Ok(())
}
/// Build from actual source bytes. No overload accepts proof DTOs or caller
/// declarations of integrity. All records are staged before one session load.
pub fn prepare_records(
    current: &Dataset,
    request: &SemanticProvenanceBatchRequest,
) -> Result<SemanticProvenanceProjection> {
    valid(request.validate())?;
    ensure!(
        (1..=4096).contains(&request.max_records),
        "provenance_record_budget_invalid"
    );
    ensure!(
        !request.records.is_empty() && request.records.len() as u64 <= request.max_records,
        "provenance_record_budget"
    );
    ensure!(
        request.sources.len() <= 64
            && request.agents.len() <= 4096
            && request.activities.len() <= 4096,
        "provenance_batch_budget"
    );
    let mut metadata_nodes = 0;
    for record in &request.records {
        if let Some(metadata) = &record.metadata {
            metadata_budget(metadata, &mut metadata_nodes)?;
        }
        if let Some(confidence) = record.confidence {
            ensure!(
                confidence.is_finite() && (0.0..=1.0).contains(&confidence),
                "provenance_confidence_invalid"
            );
        }
    }
    let input_bytes = canonical_json(request)?.len();
    ensure!(
        input_bytes <= crate::MAX_BYTES,
        "provenance_request_byte_budget"
    );
    let index = Index::new(current, &request.graph_id, request.max_quads)?;
    let mut builder = ProjectionBuilder::new(&request.graph_id, request.max_quads)?;
    let mut reserved = BTreeSet::new();
    let mut proofs = BTreeMap::<String, VerifiedSourceSpan>::new();
    let mut source_hashes = BTreeSet::new();
    let mut entity_ids = BTreeSet::new();
    for source in &request.sources {
        valid(source.validate())?;
        valid(source.binding.validate())?;
        let binding = &source.binding;
        unique_role(&mut reserved, &binding.source_id)?;
        entity_ids.insert(binding.source_id.clone());
        ensure!(
            source.data.len() as u64 == binding.byte_count,
            "source_byte_count_mismatch"
        );
        ensure!(
            crate::sha256(&source.data) == binding.source_sha256,
            "source_sha_mismatch"
        );
        ensure!(source.spans.len() <= 4096, "provenance_span_budget");
        source_hashes.insert(binding.source_sha256.clone());
        builder.kind(&binding.source_id, "Entity")?;
        builder.text(
            &binding.source_id,
            &format!("{NS}sourceSha256"),
            &binding.source_sha256,
        )?;
        builder.number(
            &binding.source_id,
            &format!("{NS}byteCount"),
            binding.byte_count,
        )?;
        for span in &source.spans {
            unique_role(&mut reserved, &span.span_id)?;
            let verified = verify_source_span(source.data.as_bytes(), binding, span)?;
            builder.node(&span.span_id, RDF_TYPE, &format!("{NS}SourceSpan"))?;
            builder.node(
                &span.span_id,
                &format!("{NS}sourceEntity"),
                &binding.source_id,
            )?;
            builder.text(
                &span.span_id,
                &format!("{NS}sourceSha256"),
                &binding.source_sha256,
            )?;
            builder.text(&span.span_id, &format!("{NS}spanSha256"), &span.span_sha256)?;
            builder.number(&span.span_id, &format!("{NS}startByte"), span.start_byte)?;
            builder.number(&span.span_id, &format!("{NS}endByte"), span.end_byte)?;
            builder.text(&span.span_id, &format!("{PROV}value"), &verified.quote)?;
            proofs.insert(span.span_id.clone(), verified);
        }
    }
    let mut agents = BTreeSet::new();
    for agent in &request.agents {
        valid(agent.validate())?;
        unique_role(&mut reserved, &agent.agent_id)?;
        agents.insert(agent.agent_id.clone());
        builder.kind(&agent.agent_id, "Agent")?;
        builder.kind(
            &agent.agent_id,
            match agent.kind.as_str() {
                "person" => "Person",
                "software_agent" => "SoftwareAgent",
                "organization" => "Organization",
                _ => unreachable!(),
            },
        )?;
    }
    let mut activities = BTreeSet::new();
    for activity in &request.activities {
        valid(activity.validate())?;
        unique_role(&mut reserved, &activity.activity_id)?;
        activities.insert(activity.activity_id.clone());
        builder.kind(&activity.activity_id, "Activity")?;
        ensure!(
            activity.used.len() <= 4096 && activity.associations.len() <= 4096,
            "provenance_activity_budget"
        );
        for source in &activity.used {
            iri(source)?;
            builder.node(&activity.activity_id, &format!("{PROV}used"), source)?;
        }
        if let Some(start) = &activity.started_at {
            builder.time(
                &activity.activity_id,
                &format!("{PROV}startedAtTime"),
                start,
            )?;
        }
        if let Some(end) = &activity.ended_at {
            builder.time(&activity.activity_id, &format!("{PROV}endedAtTime"), end)?;
        }
        if let (Some(start), Some(end)) = (&activity.started_at, &activity.ended_at) {
            ensure!(
                timestamp(start)? <= timestamp(end)?,
                "provenance_activity_time_order"
            );
        }
        for association in &activity.associations {
            valid(association.validate())?;
            unique_role(&mut reserved, &association.association_id)?;
            ensure!(
                agents.contains(&association.agent_id)
                    || index.typed(&association.agent_id, "Agent"),
                "provenance_agent_missing"
            );
            builder.node(
                &activity.activity_id,
                &format!("{PROV}wasAssociatedWith"),
                &association.agent_id,
            )?;
            builder.node(
                &activity.activity_id,
                &format!("{PROV}qualifiedAssociation"),
                &association.association_id,
            )?;
            builder.kind(&association.association_id, "Association")?;
            builder.node(
                &association.association_id,
                &format!("{PROV}agent"),
                &association.agent_id,
            )?;
            if let Some(role) = &association.role_id {
                builder.node(&association.association_id, &format!("{PROV}hadRole"), role)?;
            }
        }
    }
    let mut records = BTreeSet::new();
    let mut record_subjects = BTreeSet::new();
    for record in &request.records {
        valid(record.validate())?;
        ensure!(records.insert(record.record_id.clone()), "duplicate_record");
        iri(&record.record_id)?;
        ensure!(
            !index.has(&record.record_id) && index.record_subject(&record.record_id)?.is_none(),
            "existing_record"
        );
        ensure!(!index.has(&record.subject_id), "existing_record");
        ensure!(
            record_subjects.insert(record.subject_id.clone()),
            "duplicate_record_subject"
        );
        // A newly verified source Entity may also carry its explicit parser
        // activity. It remains one RDF entity, never a duplicate state store.
        if entity_ids.contains(&record.subject_id) {
            ensure!(
                record.entity_kind == "entity",
                "provenance_source_record_kind_mismatch"
            );
        } else {
            unique_role(&mut reserved, &record.subject_id)?;
        }
        if record.record_id != record.subject_id {
            unique_role(&mut reserved, &record.record_id)?;
        }
        entity_ids.insert(record.subject_id.clone());
        if let Some(confidence) = record.confidence {
            ensure!(
                confidence.is_finite() && (0.0..=1.0).contains(&confidence),
                "provenance_confidence_invalid"
            );
        }
    }
    let mut identities = Vec::new();
    for record in &request.records {
        builder.kind(&record.subject_id, "Entity")?;
        if record.record_id != record.subject_id {
            builder.node(
                &record.subject_id,
                &format!("{NS}recordId"),
                &record.record_id,
            )?;
        }
        if record.entity_kind != "entity" {
            builder.text(
                &record.subject_id,
                &format!("{NS}entityKind"),
                &record.entity_kind,
            )?;
        }
        if let Some(activity) = &record.generated_by {
            ensure!(
                activities.contains(activity) || index.typed(activity, "Activity"),
                "provenance_activity_missing"
            );
            builder.node(
                &record.subject_id,
                &format!("{PROV}wasGeneratedBy"),
                activity,
            )?;
        }
        ensure!(
            record.attributed_to.len() <= 4096
                && record.source_spans.len() <= 4096
                && record.derived_from.len() <= 4096
                && record.revision_of.len() <= 4096,
            "provenance_record_relation_budget"
        );
        for agent in &record.attributed_to {
            ensure!(
                agents.contains(agent) || index.typed(agent, "Agent"),
                "provenance_agent_missing"
            );
            builder.node(&record.subject_id, &format!("{PROV}wasAttributedTo"), agent)?;
        }
        let mut bound = BTreeMap::new();
        for span in &record.source_spans {
            let verified = proofs.get(span).context("provenance_unverified_span")?;
            builder.node(&record.subject_id, &format!("{NS}sourceSpan"), span)?;
            bound.insert(span, verified.receipt());
        }
        for (values, predicate) in [
            (&record.derived_from, "wasDerivedFrom"),
            (&record.revision_of, "wasRevisionOf"),
        ] {
            for target in values {
                iri(target)?;
                builder.node(&record.subject_id, &format!("{PROV}{predicate}"), target)?;
            }
        }
        if let Some(metadata) = &record.metadata {
            builder.text(
                &record.subject_id,
                &format!("{NS}metadata"),
                &canonical_json(metadata)?,
            )?;
        }
        if let Some(confidence) = record.confidence {
            builder.term(
                &record.subject_id,
                &format!("{NS}confidence"),
                Literal::new_typed_literal(confidence.to_string(), iri(&format!("{XSD}double"))?)
                    .into(),
            )?;
        }
        // Explicit new profile binds the immutable record and verified sources;
        // it does not pretend to preserve legacy checksum coverage.
        let digest = canonical_json(
            json!({"profile":"semantica.provenance-content-sha256/1","record":record,"verified_source_spans":bound}),
        )?;
        identities.push(SemanticProvenanceRecordIdentity {
            record_id: record.record_id.clone(),
            subject_id: record.subject_id.clone(),
            content_sha256: crate::sha256(digest),
        });
    }
    // Existing sources/agents/activities can be reused only with the exact same
    // projected subject quads. They never become mutable event records.
    let proposed = Index::new(&builder.dataset, &request.graph_id, request.max_quads)?;
    for (subject, quads) in &proposed.quads {
        if let Some(existing) = index.quads.get(subject) {
            ensure!(
                existing == quads,
                "provenance_existing_subject_conflict:{subject}"
            );
        }
    }
    identities.sort_by(|a, b| a.record_id.cmp(&b.record_id));
    builder.finish(&request.graph_id, request, identities, source_hashes)
}
pub fn load_records(
    session: &mut DatasetSession,
    request: &SemanticProvenanceBatchRequest,
) -> Result<SemanticMutationResult> {
    let projection = prepare_records(session.dataset(), request)?;
    // Same exclusive borrow covers checking and the unique publication path.
    session.load_batch(&[(projection.nquads.as_bytes(), RdfFormat::NQuads, None, None)])
}

pub fn record_view(
    session: &DatasetSession,
    request: &SemanticProvenanceLookup,
) -> Result<SemanticProvenanceView> {
    valid(request.validate())?;
    iri(&request.subject_id)?;
    let index = Index::new(session.dataset(), &request.graph_id, request.max_quads)?;
    let subject = if index.has(&request.subject_id) {
        Some(request.subject_id.clone())
    } else {
        index.record_subject(&request.subject_id)?
    };
    let mut related = BTreeSet::new();
    let mut invalidations = BTreeSet::new();
    let mut selected = Dataset::new();
    if let Some(subject) = &subject {
        for quad in index.quads.get(subject).into_iter().flatten() {
            selected.insert(quad);
        }
        for source in index
            .objects(subject, &format!("{PROV}wasDerivedFrom"))
            .into_iter()
            .chain(index.objects(subject, &format!("{PROV}wasRevisionOf")))
        {
            related.insert(source);
        }
        for span in index.objects(subject, &format!("{NS}sourceSpan")) {
            for quad in index.quads.get(&span).into_iter().flatten() {
                selected.insert(quad);
            }
            for source in index.objects(&span, &format!("{NS}sourceEntity")) {
                related.insert(source.clone());
                for quad in index.quads.get(&source).into_iter().flatten() {
                    selected.insert(quad);
                }
            }
        }
        for activity in index.objects(subject, &format!("{PROV}wasInvalidatedBy")) {
            invalidations.insert(activity.clone());
            for quad in index.quads.get(&activity).into_iter().flatten() {
                selected.insert(quad);
            }
        }
    }
    Ok(SemanticProvenanceView {
        schema: "semantica.provenance-view/1".into(),
        profile: PROFILE.into(),
        execution: if subject.is_some() {
            "completed"
        } else {
            "not_found"
        }
        .into(),
        subject_id: request.subject_id.clone(),
        nquads: crate::dataset::snapshot(&selected, session.revision())?.canonical_nquads,
        related_entities: related.into_iter().collect(),
        invalidation_activities: invalidations.into_iter().collect(),
        coverage: "explicit_slice".into(),
    })
}
/// Direct source evidence plus declared derivation/revision references; this
/// view does not flatten descendant metadata or infer engineering applicability.
pub fn source_view(
    session: &DatasetSession,
    request: &SemanticProvenanceLookup,
) -> Result<SemanticProvenanceView> {
    record_view(session, request)
}

fn relation_kind(predicate: &str) -> Option<&'static str> {
    if predicate == format!("{PROV}wasDerivedFrom") {
        Some("derived_from")
    } else if predicate == format!("{PROV}wasRevisionOf") {
        Some("revision_of")
    } else {
        None
    }
}
pub fn lineage_view(
    session: &DatasetSession,
    request: &SemanticLineageRequest,
) -> Result<SemanticLineageResult> {
    valid(request.validate())?;
    iri(&request.root)?;
    ensure!(
        (1..=96).contains(&request.max_levels)
            && (1..=10_000).contains(&request.max_nodes)
            && (1..=MAX_QUADS).contains(&request.max_edges),
        "provenance_lineage_budget_invalid"
    );
    ensure!(
        !request.edge_kinds.is_empty() && request.edge_kinds.len() <= 2,
        "provenance_lineage_kind_invalid"
    );
    let kinds = request.edge_kinds.iter().collect::<BTreeSet<_>>();
    ensure!(
        kinds.len() == request.edge_kinds.len()
            && kinds
                .iter()
                .all(|kind| matches!(kind.as_str(), "derived_from" | "revision_of")),
        "provenance_lineage_kind_invalid"
    );
    let index = Index::new(session.dataset(), &request.graph_id, MAX_QUADS)?;
    let mut adjacency = BTreeMap::<String, Vec<(String, SemanticLineageEdge)>>::new();
    for (subject, quads) in &index.quads {
        for quad in quads {
            if let Some(kind) = relation_kind(quad.predicate.as_str()) {
                if !kinds.contains(&kind.to_owned()) {
                    continue;
                }
                let Term::NamedNode(target) = &quad.object else {
                    anyhow::bail!("provenance_lineage_non_iri_target")
                };
                let edge = SemanticLineageEdge {
                    from: subject.clone(),
                    to: target.as_str().into(),
                    kind: kind.into(),
                };
                let (key, next) = if request.direction == "ancestors" {
                    (subject.clone(), target.as_str().to_owned())
                } else {
                    (target.as_str().to_owned(), subject.clone())
                };
                adjacency.entry(key).or_default().push((next, edge));
            }
        }
    }
    let mut visited = BTreeSet::new();
    let mut missing = BTreeSet::new();
    let mut frontier = BTreeSet::new();
    let mut edges = BTreeMap::<(String, String, String), SemanticLineageEdge>::new();
    let mut queue = VecDeque::new();
    if index.has(&request.root) {
        visited.insert(request.root.clone());
        queue.push_back((request.root.clone(), 1u64));
    } else {
        missing.insert(request.root.clone());
    }
    while let Some((subject, level)) = queue.pop_front() {
        for (next, edge) in adjacency.get(&subject).into_iter().flatten() {
            let key = (edge.from.clone(), edge.to.clone(), edge.kind.clone());
            if !index.has(next) {
                missing.insert(next.clone());
            }
            if level >= request.max_levels && !visited.contains(next) {
                frontier.insert(next.clone());
                continue;
            }
            if !edges.contains_key(&key) && edges.len() as u64 >= request.max_edges {
                frontier.insert(next.clone());
                continue;
            }
            edges.insert(key, edge.clone());
            if missing.contains(next) || visited.contains(next) {
                continue;
            }
            if visited.len() as u64 >= request.max_nodes {
                frontier.insert(next.clone());
                continue;
            }
            visited.insert(next.clone());
            queue.push_back((next.clone(), level + 1));
        }
    }
    // Detect cycles in the enumerated subgraph by a Kahn pass. An edge to an
    // already visited node is not sufficient: convergent DAGs are common.
    let mut degrees = visited
        .iter()
        .map(|node| (node.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing = BTreeMap::<String, Vec<String>>::new();
    for edge in edges
        .values()
        .filter(|edge| visited.contains(&edge.from) && visited.contains(&edge.to))
    {
        *degrees.get_mut(&edge.to).unwrap() += 1;
        outgoing
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }
    let mut zero = degrees
        .iter()
        .filter_map(|(node, degree)| (*degree == 0).then_some(node.clone()))
        .collect::<VecDeque<_>>();
    let mut acyclic = 0usize;
    while let Some(node) = zero.pop_front() {
        acyclic += 1;
        for target in outgoing.get(&node).into_iter().flatten() {
            let degree = degrees.get_mut(target).unwrap();
            *degree -= 1;
            if *degree == 0 {
                zero.push_back(target.clone());
            }
        }
    }
    let partial = !missing.is_empty() || !frontier.is_empty();
    Ok(SemanticLineageResult {
        schema: "semantica.lineage-result/1".into(),
        profile: PROFILE.into(),
        execution: if visited.is_empty() {
            "not_found"
        } else if partial {
            "partial"
        } else {
            "completed"
        }
        .into(),
        nodes: visited.iter().cloned().collect(),
        edges: edges.into_values().collect(),
        missing_references: missing.into_iter().collect(),
        frontier: frontier.into_iter().collect(),
        cycle: acyclic < visited.len(),
        coverage: if partial { "partial" } else { "complete" }.into(),
        causal_validation: "not_validated".into(),
    })
}
pub fn descendants_view(
    session: &DatasetSession,
    request: &SemanticLineageRequest,
) -> Result<SemanticLineageResult> {
    ensure!(
        request.direction == "descendants",
        "provenance_descendants_direction_mismatch"
    );
    lineage_view(session, request)
}
pub fn prepare_invalidation(
    current: &Dataset,
    request: &SemanticProvenanceInvalidationRequest,
) -> Result<SemanticProvenanceProjection> {
    valid(request.validate())?;
    iri(&request.subject_id)?;
    iri(&request.record_id)?;
    iri(&request.agent_id)?;
    iri(&request.activity_id)?;
    let index = Index::new(current, &request.graph_id, request.max_quads)?;
    ensure!(
        index.record_subject(&request.record_id)?.as_deref() == Some(request.subject_id.as_str()),
        "provenance_invalidation_record_mismatch"
    );
    ensure!(
        index.typed(&request.agent_id, "Agent"),
        "provenance_agent_missing"
    );
    ensure!(
        !index.has(&request.activity_id) && index.record_subject(&request.activity_id)?.is_none(),
        "existing_activity"
    );
    ensure!(
        request.activity_id != request.subject_id
            && request.activity_id != request.record_id
            && request.activity_id != request.agent_id,
        "provenance_identity_role_collision"
    );
    let mut builder = ProjectionBuilder::new(&request.graph_id, request.max_quads)?;
    builder.kind(&request.activity_id, "Activity")?;
    builder.node(
        &request.subject_id,
        &format!("{PROV}wasInvalidatedBy"),
        &request.activity_id,
    )?;
    builder.node(
        &request.activity_id,
        &format!("{PROV}used"),
        &request.subject_id,
    )?;
    builder.node(
        &request.activity_id,
        &format!("{PROV}wasAssociatedWith"),
        &request.agent_id,
    )?;
    builder.text(
        &request.activity_id,
        &format!("{NS}invalidationReason"),
        &request.reason,
    )?;
    if let Some(time) = &request.timestamp {
        builder.time(
            &request.subject_id,
            &format!("{PROV}invalidatedAtTime"),
            time,
        )?;
        builder.time(&request.activity_id, &format!("{PROV}endedAtTime"), time)?;
    }
    builder.finish(&request.graph_id, request, Vec::new(), BTreeSet::new())
}
pub fn load_invalidation(
    session: &mut DatasetSession,
    request: &SemanticProvenanceInvalidationRequest,
) -> Result<SemanticMutationResult> {
    let projection = prepare_invalidation(session.dataset(), request)?;
    session.load_batch(&[(projection.nquads.as_bytes(), RdfFormat::NQuads, None, None)])
}
pub struct RustProvenanceRuntime;
impl RustProvenanceRuntime {
    pub fn execute_projection(
        &self,
        input: &SemanticProvenanceBatchRequest,
    ) -> Result<SemanticProvenanceProjection, SemanticRuntimeFailure> {
        prepare_records(&Dataset::new(), input).map_err(|error| SemanticRuntimeFailure {
            code: "provenance_failed".into(),
            phase: "prepare".into(),
            message: error.to_string(),
            retryable: false,
        })
    }
}
impl SemanticProvenancePort for RustProvenanceRuntime {
    fn project(
        &self,
        input: &SemanticProvenanceBatchRequest,
    ) -> Result<SemanticProvenanceProjection, SemanticRuntimeFailure> {
        self.execute_projection(input)
    }
}
