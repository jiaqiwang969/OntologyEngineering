//! Content-bound RDF datasets, staged mutations, and W3C RDFC-1.0 snapshots.
use anyhow::{Context, Result, bail, ensure};
use oxigraph::{
    io::{RdfFormat, RdfParser, RdfSerializer},
    model::{BlankNode, Dataset, GraphName, NamedNode, NamedOrBlankNode, Quad, Term},
    sparql::SparqlEvaluator,
    store::Store,
};
use oxrdf::dataset::{CanonicalizationAlgorithm, CanonicalizationHashAlgorithm};
use semantica_contracts::{SemanticDatasetDiff, SemanticDatasetSnapshot, SemanticMutationResult};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

mod query;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_failure;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_ingest;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_load;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_load_receiving;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_observation;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_persistent_receiving;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_query;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_query_receiving;
#[cfg(feature = "source-session-local-v1")]
pub mod source_session;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_storage;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_update;
// Internal preparation only. Public source-session1–3 still admit NT/NQ;
// complete Turtle writing and result witnessing require separate reception.
#[cfg(feature = "rdflib760-nullable-internal")]
#[allow(dead_code)]
mod source_namespace;
#[cfg(feature = "rdflib760-nullable-internal")]
#[allow(dead_code)] // typed foundation; public Turtle protocol reception pending
mod source_namespace_witness;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_storage_witness;
#[cfg(feature = "rdflib760-nullable-internal")]
#[allow(dead_code)]
mod source_turtle;
#[cfg(feature = "rdflib760-nullable-internal")]
#[allow(dead_code)]
mod source_turtle_literal;
#[cfg(feature = "rdflib760-nullable-internal")]
#[allow(dead_code)] // explicit public4 receiving remains gated
mod source_turtle_read;
#[cfg(feature = "rdflib760-nullable-internal")]
#[allow(dead_code)]
mod source_turtle_transcript;
#[cfg(feature = "rdflib760-nullable-internal")]
#[allow(dead_code)] // internal text candidate; public witness/platform reception pending
mod source_turtle_writer;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_update_execution_controls;
#[cfg(feature = "rdflib760-nullable-internal")]
mod source_update_loader;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_update_loader_receiving;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_update_wrapper_receiving;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_update_wrapper_red;
#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_variable_wrapper_receiving;

const CANONICALIZATION: &str = "semantica.rdfc-1.0-sha256/1";
const MAX_QUADS: usize = 100_000;
const MAX_AMBIGUOUS_NODES: usize = 8;

#[derive(Clone, Copy, Default)]
enum ResourceProfile {
    #[default]
    Default,
    Context50k,
}
impl ResourceProfile {
    fn bytes(self) -> usize {
        match self {
            Self::Default => crate::MAX_BYTES,
            Self::Context50k => 128 * 1024 * 1024,
        }
    }
    fn quads(self) -> usize {
        match self {
            Self::Default => MAX_QUADS,
            Self::Context50k => 500_000,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Default => "semantica.dataset-default/1",
            Self::Context50k => "semantica.context-50k/1",
        }
    }
}

// The parser binds native identity and source construction separately. Numeric
// JSON values are already Python-value constructions, not lexical RDF strings.
struct ParsedLoad {
    quad: Quad,
    source: Option<crate::rdflib_snapshot::Quad>,
    normalize_source_literal: bool,
}
impl ParsedLoad {
    fn native(quad: Quad) -> Self {
        Self {
            quad,
            source: None,
            normalize_source_literal: true,
        }
    }
}
fn format_supported(format: RdfFormat) -> Result<()> {
    ensure!(
        matches!(
            format,
            RdfFormat::Turtle
                | RdfFormat::NTriples
                | RdfFormat::RdfXml
                | RdfFormat::TriG
                | RdfFormat::NQuads
        ),
        "dataset_rdf_format_unsupported"
    );
    Ok(())
}
fn dataset_format(format: RdfFormat) -> bool {
    matches!(format, RdfFormat::TriG | RdfFormat::NQuads)
}
fn serialized_nquads(dataset: &Dataset) -> String {
    let mut rows = dataset
        .iter()
        .map(|quad| format!("{quad} .\n"))
        .collect::<Vec<_>>();
    rows.sort();
    rows.concat()
}
// Count the exact N-Quads formatter output without allocating or sorting a
// complete serialization merely to reject it. Ordering cannot change byte size.
struct NQuadsByteBudget {
    remaining: usize,
}
impl std::fmt::Write for NQuadsByteBudget {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.remaining = self
            .remaining
            .checked_sub(text.len())
            .ok_or(std::fmt::Error)?;
        Ok(())
    }
}
fn serialized_nquads_fit(dataset: &Dataset, limit: usize) -> bool {
    use std::fmt::Write;
    let mut counter = NQuadsByteBudget { remaining: limit };
    for quad in dataset.iter() {
        if writeln!(&mut counter, "{quad} .").is_err() {
            return false;
        }
    }
    true
}
fn budget(dataset: &Dataset, profile: ResourceProfile) -> Result<()> {
    ensure!(dataset.len() <= profile.quads(), "dataset_quad_budget");
    ensure!(
        serialized_nquads_fit(dataset, profile.bytes()),
        "dataset_byte_budget"
    );
    Ok(())
}
fn blank_ids(quad: &Quad) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    if let NamedOrBlankNode::BlankNode(node) = &quad.subject {
        ids.insert(node.as_str().into());
    }
    if let Term::BlankNode(node) = &quad.object {
        ids.insert(node.as_str().into());
    }
    if let GraphName::BlankNode(node) = &quad.graph_name {
        ids.insert(node.as_str().into());
    }
    ids
}
fn first_degree_budget(dataset: &Dataset) -> Result<()> {
    let quads = dataset
        .iter()
        .map(|quad| quad.into_owned())
        .collect::<Vec<_>>();
    let mut related = BTreeMap::<String, Vec<usize>>::new();
    for (index, quad) in quads.iter().enumerate() {
        for id in blank_ids(quad) {
            related.entry(id).or_default().push(index);
        }
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for (reference, indices) in related {
        let replacement = |node: &BlankNode| {
            BlankNode::new_unchecked(if node.as_str() == reference { "a" } else { "z" })
        };
        let mut rows = Vec::with_capacity(indices.len());
        for index in indices {
            let mut quad = quads[index].clone();
            if let NamedOrBlankNode::BlankNode(node) = quad.subject {
                quad.subject = replacement(&node).into();
            }
            if let Term::BlankNode(node) = quad.object {
                quad.object = replacement(&node).into();
            }
            if let GraphName::BlankNode(node) = quad.graph_name {
                quad.graph_name = replacement(&node).into();
            }
            rows.push(format!("{quad} .\n"));
        }
        rows.sort();
        *counts.entry(crate::sha256(rows.concat())).or_default() += 1;
    }
    let ambiguous: usize = counts.values().filter(|count| **count > 1).sum();
    ensure!(
        ambiguous <= MAX_AMBIGUOUS_NODES,
        "rdfc_ambiguous_blank_node_budget:{ambiguous}"
    );
    Ok(())
}

/// The explicitly versioned W3C algorithm is used over the complete dataset,
/// including blank graph names. The revision is observation metadata only.
pub fn snapshot(dataset: &Dataset, revision: u64) -> Result<SemanticDatasetSnapshot> {
    snapshot_with_resources(dataset, revision, ResourceProfile::Default)
}
fn snapshot_with_resources(
    dataset: &Dataset,
    revision: u64,
    profile: ResourceProfile,
) -> Result<SemanticDatasetSnapshot> {
    budget(dataset, profile)?;
    let has_blank = dataset.iter().any(|quad| {
        matches!(
            quad.subject,
            oxigraph::model::NamedOrBlankNodeRef::BlankNode(_)
        ) || matches!(quad.object, oxigraph::model::TermRef::BlankNode(_))
            || matches!(quad.graph_name, oxigraph::model::GraphNameRef::BlankNode(_))
    });
    // RDFC only relabels blank nodes; a blank-free dataset needs the same
    // canonical N-Quads sorting, without cloning/relabeling the entire graph.
    let canonical_nquads = if has_blank {
        first_degree_budget(dataset)?;
        let mut canonical = dataset.clone();
        canonical.canonicalize(CanonicalizationAlgorithm::Rdfc10 {
            hash_algorithm: CanonicalizationHashAlgorithm::Sha256,
        });
        serialized_nquads(&canonical)
    } else {
        serialized_nquads(dataset)
    };
    ensure!(
        canonical_nquads.len() <= profile.bytes(),
        "dataset_canonical_byte_budget"
    );
    Ok(SemanticDatasetSnapshot {
        schema: "semantica.dataset-snapshot/1".into(),
        canonicalization: CANONICALIZATION.into(),
        dataset_sha256: crate::sha256(&canonical_nquads),
        canonical_nquads,
        revision,
        quad_count: dataset.len() as u64,
    })
}

fn verify_snapshot(snapshot: &SemanticDatasetSnapshot, profile: ResourceProfile) -> Result<()> {
    ensure!(
        snapshot.schema == "semantica.dataset-snapshot/1"
            && snapshot.canonicalization == CANONICALIZATION,
        "dataset_snapshot_profile_mismatch"
    );
    ensure!(
        snapshot.canonical_nquads.len() <= profile.bytes(),
        "dataset_snapshot_byte_budget"
    );
    ensure!(
        crate::sha256(&snapshot.canonical_nquads) == snapshot.dataset_sha256,
        "dataset_snapshot_digest_mismatch"
    );
    let mut graph = Dataset::new();
    for quad in
        RdfParser::from_format(RdfFormat::NQuads).for_reader(snapshot.canonical_nquads.as_bytes())
    {
        graph.insert(&quad?);
        ensure!(
            graph.len() <= profile.quads(),
            "dataset_snapshot_quad_budget"
        );
    }
    ensure!(
        graph.len() as u64 == snapshot.quad_count,
        "dataset_snapshot_count_mismatch"
    );
    ensure!(
        snapshot_with_resources(&graph, snapshot.revision, profile)?.canonical_nquads
            == snapshot.canonical_nquads,
        "dataset_snapshot_not_canonical"
    );
    Ok(())
}
pub fn diff_snapshots(
    before: &SemanticDatasetSnapshot,
    after: &SemanticDatasetSnapshot,
) -> Result<SemanticDatasetDiff> {
    diff_with_resources(before, after, ResourceProfile::Default)
}
fn diff_with_resources(
    before: &SemanticDatasetSnapshot,
    after: &SemanticDatasetSnapshot,
    profile: ResourceProfile,
) -> Result<SemanticDatasetDiff> {
    verify_snapshot(before, profile)?;
    verify_snapshot(after, profile)?;
    let old = before
        .canonical_nquads
        .lines()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let new = after
        .canonical_nquads
        .lines()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    Ok(SemanticDatasetDiff {
        before_sha256: before.dataset_sha256.clone(),
        after_sha256: after.dataset_sha256.clone(),
        canonicalization: CANONICALIZATION.into(),
        added: new.difference(&old).cloned().collect(),
        removed: old.difference(&new).cloned().collect(),
    })
}

#[derive(Clone)]
struct PackageRecord {
    digest: String,
    retained: Option<semantica_contracts::SemanticPackageLoadResult>,
}
#[derive(Clone)]
struct ExplorerSearchCache {
    native: bool,
    revision: u64,
    dataset_sha256: String,
    index: crate::explorer_search_index::GraphSearchIndex,
    payload_bytes: usize,
}
#[derive(Default, Clone)]
pub struct DatasetSession {
    resource_profile: ResourceProfile,
    #[cfg(feature = "rdflib760-nullable-internal")]
    source_cache_enabled: Option<bool>,
    #[cfg(feature = "rdflib760-nullable-internal")]
    source_contract: source_query::SourceSessionContract,
    #[cfg(feature = "rdflib760-nullable-internal")]
    source_namespaces: source_namespace::NamespaceState,
    dataset: Dataset,
    revision: u64,
    query_cache: BTreeMap<query::QueryCacheKey, Value>,
    cache_bytes: usize,
    explorer_search_cache: Option<ExplorerSearchCache>,
    snapshot_cache: Option<SemanticDatasetSnapshot>,
    rdflib_projection: crate::rdflib_snapshot::Projection,
    rdflib_snapshot_cache: crate::rdflib_snapshot::SnapshotCache,
    packages: BTreeMap<(String, String), PackageRecord>,
}
impl DatasetSession {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_resources(
        selection: &semantica_contracts::SemanticDatasetResourceSelection,
    ) -> Result<Self> {
        ensure!(
            selection.validate().is_ok(),
            "dataset_resource_selection_invalid"
        );
        let resource_profile = match selection.profile.as_str() {
            "semantica.dataset-default/1" => ResourceProfile::Default,
            "semantica.context-50k/1" => ResourceProfile::Context50k,
            _ => bail!("dataset_resource_selection_invalid"),
        };
        Ok(Self {
            resource_profile,
            ..Self::default()
        })
    }
    pub fn resource_profile(&self) -> &'static str {
        self.resource_profile.name()
    }
    pub fn dataset_byte_limit(&self) -> usize {
        self.resource_profile.bytes()
    }
    pub fn worker_input_limit(&self) -> usize {
        match self.resource_profile {
            ResourceProfile::Default => crate::MAX_BYTES,
            ResourceProfile::Context50k => 256 * 1024 * 1024,
        }
    }
    pub(crate) fn context_node_limit(&self) -> usize {
        match self.resource_profile {
            ResourceProfile::Default => 10_000,
            ResourceProfile::Context50k => 50_000,
        }
    }
    pub(crate) fn typed_context_commit(&self) -> bool {
        matches!(self.resource_profile, ResourceProfile::Context50k)
    }
    /// Read one explicitly selected named graph through the existing SHACL
    /// evaluator. Projection preserves RDF terms and never publishes to this
    /// session, changes its revision, or touches its derived caches.
    pub fn validate_named_graph(&self, graph_name: &str, shapes: &[u8]) -> Result<(u64, Value)> {
        ensure!(
            graph_name.len() <= crate::MAX_BYTES,
            "shacl_graph_name_byte_budget"
        );
        ensure!(shapes.len() <= crate::MAX_BYTES, "rdf_byte_budget");
        // Context50k does not implicitly enlarge this SHACL profile. Enforce
        // the existing complete default-Dataset budget before selecting a view.
        self.rdflib_projection.require_native_complete()?;
        budget(&self.dataset, ResourceProfile::Default)?;
        let selected = GraphName::NamedNode(NamedNode::new(graph_name)?);
        let mut view = Dataset::new();
        for quad in self.dataset.iter() {
            if quad.graph_name == selected.as_ref() {
                let mut quad = quad.into_owned();
                quad.graph_name = GraphName::DefaultGraph;
                view.insert(&quad);
            }
        }
        let selected_quad_count = view.len() as u64;
        // A legal but absent name selects an empty graph. Explicit targetNode
        // obligations still execute; no-target conformance is not evidence.
        let report = crate::shacl::validate_dataset(&view, shapes)?;
        Ok((selected_quad_count, report))
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn quad_count(&self) -> u64 {
        #[cfg(feature = "rdflib760-nullable-internal")]
        if matches!(
            self.source_contract,
            source_query::SourceSessionContract::DatasetDataV2
                | source_query::SourceSessionContract::DatasetUpdateV3
        ) {
            if let Some(quads) = &self.rdflib_projection.quads {
                return quads.len() as u64;
            }
        }
        self.dataset.len() as u64
    }
    pub fn dataset(&self) -> &Dataset {
        &self.dataset
    }
    /// Derived cache only: all graph records and revisions remain in this session.
    fn staged_explorer_index(
        &mut self,
        deadline: std::time::Instant,
    ) -> Result<ExplorerSearchCache> {
        self.rdflib_projection.require_native_complete()?;
        ensure!(
            std::time::Instant::now() < deadline,
            "explorer_search_deadline_exceeded"
        );
        let digest = self.snapshot()?.dataset_sha256;
        let staged = match &self.explorer_search_cache {
            Some(cache)
                if !cache.native
                    && cache.revision == self.revision
                    && cache.dataset_sha256 == digest =>
            {
                cache.clone()
            }
            _ => {
                let records = crate::context::records(self)?;
                let mut nodes = Vec::with_capacity(records.nodes.len());
                for node in &records.nodes {
                    ensure!(
                        std::time::Instant::now() < deadline,
                        "explorer_search_deadline_exceeded"
                    );
                    nodes.push(crate::explorer_read::node(node)?);
                }
                let index = if let Some(index) = crate::explorer_mutation::stored_index(self)? {
                    index
                } else {
                    let mut index = crate::explorer_search_index::GraphSearchIndex::default();
                    index.rebuild(&nodes, deadline)?;
                    index
                };
                ExplorerSearchCache {
                    native: false,
                    revision: self.revision,
                    dataset_sha256: digest,
                    index,
                    payload_bytes: 0,
                }
            }
        };
        let available = self
            .dataset_byte_limit()
            .checked_sub(self.cache_bytes)
            .context("explorer_search_cache_budget")?;
        staged.index.payload_bytes(available)?;
        Ok(staged)
    }
    pub(crate) fn explorer_mutation_index(
        &mut self,
        deadline: std::time::Instant,
    ) -> Result<crate::explorer_search_index::GraphSearchIndex> {
        Ok(self.staged_explorer_index(deadline)?.index)
    }
    /// Eager Explorer initialization on the sole session. No query is fabricated,
    /// and no source, revision, service budget, or domain fact is introduced.
    pub fn prepare_explorer_index(&mut self, deadline: std::time::Instant) -> Result<()> {
        let mut staged = self.staged_explorer_index(deadline)?;
        let available = self
            .dataset_byte_limit()
            .checked_sub(self.cache_bytes)
            .context("explorer_search_cache_budget")?;
        staged.payload_bytes = staged.index.payload_bytes(available)?;
        ensure!(
            std::time::Instant::now() < deadline,
            "explorer_search_deadline_exceeded"
        );
        self.explorer_search_cache = Some(staged);
        Ok(())
    }
    pub(crate) fn explorer_search_matches(
        &mut self,
        query: &str,
        limit: usize,
        filters: &Value,
        deadline: std::time::Instant,
    ) -> Result<crate::explorer_search_index::SearchMatches> {
        let mut staged = self.staged_explorer_index(deadline)?;
        let available = self
            .dataset_byte_limit()
            .checked_sub(self.cache_bytes)
            .context("explorer_search_cache_budget")?;
        let result = staged.index.search(query, limit, filters, deadline)?;
        staged.payload_bytes = staged.index.payload_bytes(available)?;
        ensure!(
            std::time::Instant::now() < deadline,
            "explorer_search_deadline_exceeded"
        );
        self.explorer_search_cache = Some(staged);
        Ok(result)
    }
    pub fn load_package(
        &mut self,
        input: &semantica_contracts::SemanticPackageInput,
        provider: &dyn crate::books::SourceProvider,
    ) -> Result<semantica_contracts::SemanticPackageLoadResult> {
        crate::packages::load_package(self, provider, input)
    }
    fn publish(&mut self, operation: &str, staged: Dataset) -> Result<SemanticMutationResult> {
        self.rdflib_projection.require_native_complete()?;
        self.publish_native_state(operation, staged)
    }
    fn validate_source_state(
        &self,
        staged: &Dataset,
        projection: &crate::rdflib_snapshot::Projection,
    ) -> Result<()> {
        ensure!(
            projection.native_projection()? == *staged,
            "source_projection_state_mismatch"
        );
        #[cfg(feature = "rdflib760-nullable-internal")]
        if self.source_cache_enabled.is_some() {
            let quads = projection
                .quads
                .as_ref()
                .context("rdflib_lossless_source_projection_unavailable")?;
            let state = projection
                .source_state
                .as_ref()
                .context("source_persistent_state_unavailable")?;
            ensure!(
                state.matches_projection(quads.iter().map(source_query::quad_input))?,
                "source_persistent_projection_mismatch"
            );
            ensure!(
                quads.len() <= self.resource_profile.quads(),
                "source_dataset_quad_budget"
            );
            let mut bytes = 0usize;
            for (s, p, o, g) in quads {
                for term in [Some(s), Some(p), Some(o), g.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    bytes = bytes
                        .checked_add(term.n3().len() + 1)
                        .context("source_dataset_byte_budget")?;
                }
                bytes = bytes.checked_add(2).context("source_dataset_byte_budget")?;
                ensure!(
                    bytes <= self.dataset_byte_limit(),
                    "source_dataset_byte_budget"
                );
            }
        }
        budget(staged, self.resource_profile)?;
        Ok(())
    }
    fn publish_source_state(
        &mut self,
        operation: &str,
        staged: Dataset,
        projection: crate::rdflib_snapshot::Projection,
    ) -> Result<SemanticMutationResult> {
        self.validate_source_state(&staged, &projection)?;
        let result = self.publish_native_state(operation, staged)?;
        self.rdflib_projection = projection;
        Ok(result)
    }
    fn publish_native_state(
        &mut self,
        operation: &str,
        staged: Dataset,
    ) -> Result<SemanticMutationResult> {
        budget(&staged, self.resource_profile)?;
        let revision = self
            .revision
            .checked_add(1)
            .context("dataset_revision_overflow")?;
        let added = staged
            .iter()
            .filter(|quad| !self.dataset.contains(*quad))
            .count() as u64;
        let removed = self
            .dataset
            .iter()
            .filter(|quad| !staged.contains(*quad))
            .count() as u64;
        let result = SemanticMutationResult {
            operation: operation.into(),
            added,
            removed,
            quad_count: staged.len() as u64,
            revision,
        };
        self.dataset = staged;
        self.revision = revision;
        self.query_cache.clear();
        self.cache_bytes = 0;
        self.snapshot_cache = None;
        self.explorer_search_cache = None;
        self.rdflib_snapshot_cache.set(None);
        Ok(result)
    }
    pub fn load(
        &mut self,
        bytes: &[u8],
        format: RdfFormat,
        base_uri: Option<&str>,
        graph_name: Option<&str>,
    ) -> Result<SemanticMutationResult> {
        self.load_with_mapping(bytes, format, base_uri, graph_name)
            .map(|(result, _)| result)
    }
    fn load_with_mapping(
        &mut self,
        bytes: &[u8],
        format: RdfFormat,
        base_uri: Option<&str>,
        graph_name: Option<&str>,
    ) -> Result<(SemanticMutationResult, BTreeMap<String, BlankNode>)> {
        #[cfg(feature = "rdflib760-nullable-internal")]
        let source_jsonld =
            self.source_cache_enabled.is_some() && matches!(format, RdfFormat::JsonLd { .. });
        #[cfg(not(feature = "rdflib760-nullable-internal"))]
        let source_jsonld = false;
        if !source_jsonld {
            format_supported(format)?;
        }
        ensure!(
            bytes.len() <= self.dataset_byte_limit(),
            "dataset_load_byte_budget"
        );
        ensure!(
            graph_name.is_none() || !dataset_format(format),
            "dataset_graph_override_for_dataset_syntax"
        );
        let selected = graph_name
            .map(NamedNode::new)
            .transpose()?
            .map(GraphName::NamedNode);
        let mut parser = RdfParser::from_format(format);
        if let Some(base) = base_uri {
            parser = parser.with_base_iri(base)?;
        }
        let mut staged = self.dataset.clone();
        let mut occupied = self
            .dataset
            .iter()
            .flat_map(|quad| blank_ids(&quad.into_owned()))
            .collect::<BTreeSet<_>>();
        let mut mappings = BTreeMap::<String, BlankNode>::new();
        // Oversize RDF sources are already outside the unchanged legacy parser
        // byte domain. Do not build a second full term set that it cannot use.
        let track_legacy_terms = bytes.len() <= crate::MAX_BYTES;
        let mut parsed_document = std::collections::HashSet::<Quad>::new();
        #[cfg(feature = "rdflib760-nullable-internal")]
        let capture_source = self.source_cache_enabled.is_some()
            && matches!(
                format,
                RdfFormat::Turtle | RdfFormat::TriG | RdfFormat::RdfXml | RdfFormat::JsonLd { .. }
            );
        #[cfg(not(feature = "rdflib760-nullable-internal"))]
        let capture_source = false;
        #[cfg(feature = "rdflib760-nullable-internal")]
        let mut source_events = self.source_cache_enabled.is_some().then(Vec::new);
        let mut source_bindings =
            capture_source.then(BTreeMap::<crate::rdflib_snapshot::Quad, Vec<Quad>>::new);
        let mut source_byte_budget = capture_source.then(|| NQuadsByteBudget {
            remaining: self.dataset_byte_limit(),
        });
        if let Some(counter) = &mut source_byte_budget {
            use std::fmt::Write;
            for quad in self.dataset.iter() {
                writeln!(counter, "{quad} .")
                    .map_err(|_| anyhow::anyhow!("dataset_byte_budget"))?;
            }
        }
        #[cfg(feature = "rdflib760-nullable-internal")]
        let source_nquads_error = self.source_cache_enabled.is_some()
            && self.source_contract == source_query::SourceSessionContract::DatasetUpdateV3
            && format == RdfFormat::NQuads;
        let parsed: Box<dyn Iterator<Item = Result<ParsedLoad>> + '_> = {
            #[cfg(feature = "rdflib760-nullable-internal")]
            if capture_source {
                Box::new(source_load::parse(bytes, format, base_uri)?)
            } else {
                Box::new(parser.for_reader(bytes).map(move |q| {
                    let q = q.map_err(|e| {
                        if source_nquads_error {
                            source_failure::SourceFailure::nquads_parse(e, bytes)
                        } else {
                            e.into()
                        }
                    })?;
                    Ok(ParsedLoad::native(q))
                }))
            }
            #[cfg(not(feature = "rdflib760-nullable-internal"))]
            {
                Box::new(parser.for_reader(bytes).map(|q| Ok(ParsedLoad::native(q?))))
            }
        };
        let mut next = 0usize;
        let revision = self
            .revision
            .checked_add(1)
            .context("dataset_revision_overflow")?;
        let mut blank =
            |node: BlankNode, mappings: &mut BTreeMap<String, BlankNode>| -> Result<BlankNode> {
                if let Some(mapped) = mappings.get(node.as_str()) {
                    return Ok(mapped.clone());
                }
                loop {
                    let id = format!("document_{revision}_{next}");
                    next += 1;
                    if occupied.insert(id.clone()) {
                        let mapped = BlankNode::new(id)?;
                        mappings.insert(node.as_str().into(), mapped.clone());
                        return Ok(mapped);
                    }
                }
            };
        for item in parsed {
            let ParsedLoad {
                mut quad,
                source: mut raw,
                normalize_source_literal,
            } = item?;
            if let NamedOrBlankNode::BlankNode(node) = quad.subject {
                quad.subject = blank(node, &mut mappings)?.into();
            }
            if let Term::BlankNode(node) = quad.object {
                quad.object = blank(node, &mut mappings)?.into();
            }
            if let GraphName::BlankNode(node) = quad.graph_name {
                quad.graph_name = blank(node, &mut mappings)?.into();
            }
            if let Some(graph) = &selected {
                quad.graph_name = graph.clone();
                if let Some(source) = &mut raw {
                    source.3 = graph_name.map(|v| crate::rdflib_snapshot::Node::Iri(v.into()));
                }
            }
            if track_legacy_terms {
                parsed_document.insert(quad.clone());
            }
            if staged.insert(&quad) {
                if let Some(counter) = &mut source_byte_budget {
                    use std::fmt::Write;
                    writeln!(counter, "{quad} .")
                        .map_err(|_| anyhow::anyhow!("dataset_byte_budget"))?;
                }
            }
            if let Some(bindings) = &mut source_bindings {
                let raw = raw.context("source_rdf_parser_observation_missing")?;
                let mut mapped = crate::rdflib_snapshot::remap_quad(&raw, &mappings)?;
                let normalized = if normalize_source_literal {
                    crate::rdflib_snapshot::normalized(&BTreeSet::from([mapped]))?
                        .pop_first()
                        .context("source_rdf_normalized_term_missing")?
                } else {
                    // Native JSON numbers/bools use Python value construction;
                    // explicit dtype does not trigger a second lexical cast.
                    mapped.3 = mapped.3.filter(|g| {
                        *g != crate::rdflib_snapshot::Node::Iri("urn:x-rdflib:default".into())
                    });
                    mapped
                };
                #[cfg(feature = "rdflib760-nullable-internal")]
                if let Some(events) = &mut source_events {
                    events.push(normalized.clone());
                }
                let terms = bindings.entry(normalized).or_default();
                if !terms.contains(&quad) {
                    terms.push(quad.clone());
                }
            }
            ensure!(
                staged.len() <= self.resource_profile.quads(),
                "dataset_load_quad_budget"
            );
        }
        drop(blank);
        let mut projection = self.rdflib_projection.clone();
        let source_sha = crate::sha256(bytes);
        if !projection.source_sha256.contains(&source_sha) {
            if projection.source_sha256.len() < 1024 {
                projection.source_sha256.push(source_sha);
            } else {
                projection.invalidate();
            }
        }
        if let Some(bindings) = source_bindings {
            let actual_document = bindings
                .values()
                .flatten()
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            ensure!(
                actual_document == parsed_document,
                "source_rdf_actual_parser_binding"
            );
            let retained = projection
                .quads
                .as_mut()
                .context("rdflib_lossless_source_projection_unavailable")?;
            for (source, terms) in bindings {
                retained.insert(source.clone());
                let actual = projection.core.entry(source).or_default();
                for term in terms {
                    if !actual.contains(&term) {
                        actual.push(term);
                    }
                }
            }
        } else if matches!(format, RdfFormat::NTriples | RdfFormat::NQuads) {
            let projected = (|| -> Result<_> {
                let raw = crate::rdflib_snapshot::parse_ordered(bytes, graph_name, revision)?;
                let core = raw
                    .iter()
                    .map(|q| crate::rdflib_snapshot::core_quad(q, Some((revision, &mappings))))
                    .collect::<Result<std::collections::HashSet<_>>>()?;
                ensure!(
                    core == parsed_document,
                    "rdflib_raw_projection_parser_mismatch"
                );
                let mut bindings = BTreeMap::<crate::rdflib_snapshot::Quad, Vec<Quad>>::new();
                for q in raw {
                    let actual =
                        crate::rdflib_snapshot::core_quad(&q, Some((revision, &mappings)))?;
                    let mapped = crate::rdflib_snapshot::source_quad(&q, revision, &mappings)?;
                    let normalized = crate::rdflib_snapshot::normalized(&BTreeSet::from([mapped]))?
                        .pop_first()
                        .context("rdflib_source_term_missing")?;
                    #[cfg(feature = "rdflib760-nullable-internal")]
                    if let Some(events) = &mut source_events {
                        events.push(normalized.clone());
                    }
                    let terms = bindings.entry(normalized).or_default();
                    if !terms.contains(&actual) {
                        terms.push(actual);
                    }
                }
                Ok(bindings)
            })();
            if let (Some(retained), Ok(bindings)) = (&mut projection.quads, projected) {
                for (q, terms) in bindings {
                    retained.insert(q.clone());
                    let actual = projection.core.entry(q).or_default();
                    for term in terms {
                        if !actual.contains(&term) {
                            actual.push(term);
                        }
                    }
                }
            } else {
                projection.invalidate();
            }
        } else {
            projection.invalidate();
        }
        #[cfg(feature = "rdflib760-nullable-internal")]
        let source_publisher = self.source_cache_enabled.is_some();
        #[cfg(not(feature = "rdflib760-nullable-internal"))]
        let source_publisher = false;
        #[cfg(feature = "rdflib760-nullable-internal")]
        if let Some(events) = source_events {
            if !events.is_empty() && (graph_name.is_some() || events.iter().any(|q| q.3.is_some()))
            {
                self.source_namespaces.initialize(
                    true,
                    self.dataset_byte_limit(),
                    self.resource_profile.quads(),
                )?;
            }
            projection
                .source_state
                .as_mut()
                .context("source_persistent_state_unavailable")?
                .load_parsed(
                    events.iter().map(source_query::quad_input),
                    true,
                    self.resource_profile.quads(),
                    self.dataset_byte_limit(),
                )?;
        }
        let result = if source_publisher {
            self.publish_source_state("load", staged, projection)?
        } else {
            let result = self.publish("load", staged)?;
            self.rdflib_projection = projection;
            result
        };
        Ok((result, mappings))
    }
    /// Replace one named graph using typed RDF terms and the existing atomic
    /// publisher. This explicit operation does not route internal data through
    /// the separately bounded external SPARQL text interface.
    pub fn replace_named_graph(
        &mut self,
        graph_name: &str,
        replacement: &Dataset,
    ) -> Result<SemanticMutationResult> {
        budget(replacement, self.resource_profile)?;
        self.replace_named_graph_owned(graph_name, replacement.clone())
    }
    /// Consume a transaction-local graph instead of inserting every quad into
    /// another full dataset. The same publisher still owns the only live state.
    pub(crate) fn replace_named_graph_owned(
        &mut self,
        graph_name: &str,
        mut replacement: Dataset,
    ) -> Result<SemanticMutationResult> {
        let selected = GraphName::NamedNode(NamedNode::new(graph_name)?);
        budget(&replacement, self.resource_profile)?;
        ensure!(
            replacement
                .iter()
                .all(|quad| quad.graph_name == selected.as_ref()),
            "dataset_replacement_graph_mismatch"
        );
        for quad in self.dataset.iter() {
            if quad.graph_name != selected.as_ref() {
                replacement.insert(quad);
                ensure!(
                    replacement.len() <= self.resource_profile.quads(),
                    "dataset_quad_budget"
                );
            }
        }
        let result = self.publish("replace_named_graph", replacement)?;
        self.rdflib_projection.invalidate();
        Ok(result)
    }
    pub fn clear(&mut self, graph_name: Option<&str>) -> Result<SemanticMutationResult> {
        let mut staged = Dataset::new();
        if let Some(name) = graph_name {
            let selected = GraphName::NamedNode(NamedNode::new(name)?);
            for quad in self.dataset.iter() {
                if quad.graph_name != selected.as_ref() {
                    staged.insert(quad);
                }
            }
        }
        let mut projection = self.rdflib_projection.clone();
        if graph_name.is_none() {
            projection.quads = Some(BTreeSet::new());
            projection.core.clear();
            projection.source_sha256.clear();
        } else if let Some(quads) = &mut projection.quads {
            let graph = GraphName::NamedNode(NamedNode::new(graph_name.unwrap())?);
            projection.core.retain(|_, terms| {
                terms.retain(|q| q.graph_name != graph);
                !terms.is_empty()
            });
            quads.retain(|q| projection.core.contains_key(q));
        }
        let result = self.publish("clear", staged)?;
        self.rdflib_projection = projection;
        Ok(result)
    }
    /// Every document is parsed through load into a private clone; only the
    /// complete batch becomes one public mutation. Failed staging never affects
    /// the original dataset, revision, query cache, or snapshot cache.
    pub fn load_batch(
        &mut self,
        sources: &[(&[u8], RdfFormat, Option<&str>, Option<&str>)],
    ) -> Result<SemanticMutationResult> {
        ensure!(
            !sources.is_empty() && sources.len() <= 64,
            "dataset_batch_source_budget"
        );
        let total = sources.iter().try_fold(0usize, |sum, (bytes, _, _, _)| {
            sum.checked_add(bytes.len())
                .context("dataset_batch_byte_budget")
        })?;
        ensure!(
            total <= self.dataset_byte_limit(),
            "dataset_batch_byte_budget"
        );
        let mut staged = self.clone();
        for (bytes, format, base_uri, graph_name) in sources {
            staged.load(bytes, *format, *base_uri, *graph_name)?;
        }
        let projection = staged.rdflib_projection;
        let result = self.publish("load", staged.dataset)?;
        self.rdflib_projection = projection;
        Ok(result)
    }
    /// The caller verifies the immutable manifest and every asset before this
    /// method. This is ephemeral loading identity, never adoption authority.
    pub fn load_verified_package(
        &mut self,
        id: &str,
        version: &str,
        digest: &str,
        sources: &[(&[u8], RdfFormat, Option<&str>, Option<&str>)],
    ) -> Result<Value> {
        ensure!(
            !id.trim().is_empty() && !version.trim().is_empty(),
            "dataset_package_identity_missing"
        );
        ensure!(
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
            "dataset_package_digest_invalid"
        );
        let key = (id.to_owned(), version.to_owned());
        if let Some(existing) = self.packages.get(&key) {
            ensure!(
                existing.digest == digest,
                "dataset_package_identity_digest_conflict"
            );
            return Ok(
                json!({"package_id":id,"version":version,"digest":digest,"rdf_added":0,"revision":self.revision}),
            );
        }
        ensure!(self.packages.len() < 1000, "dataset_package_count_budget");
        let mut detached = Self::new();
        if !sources.is_empty() {
            detached.load_batch(sources)?;
        }
        let prior_projection = self.rdflib_projection.clone();
        let mut loaded = self.clone();
        let mut mappings = BTreeMap::new();
        let added = if detached.quad_count() != 0 {
            let bytes = detached.serialize(RdfFormat::NQuads, None)?;
            let (result, map) =
                loaded.load_with_mapping(bytes.as_bytes(), RdfFormat::NQuads, None, None)?;
            mappings = map;
            result.added
        } else {
            0
        };
        let mut projection = prior_projection;
        if let (Some(retained), Some(source)) =
            (&mut projection.quads, &detached.rdflib_projection.quads)
        {
            for key in source {
                let remapped = crate::rdflib_snapshot::remap_quad(key, &mappings)?;
                retained.insert(remapped.clone());
                let target = projection.core.entry(remapped).or_default();
                for core in detached
                    .rdflib_projection
                    .core
                    .get(key)
                    .context("rdflib_package_core_binding_missing")?
                {
                    let actual = crate::rdflib_snapshot::remap_core(core, &mappings)?;
                    ensure!(
                        loaded.dataset.contains(&actual),
                        "rdflib_package_core_binding_mismatch"
                    );
                    if !target.contains(&actual) {
                        target.push(actual);
                    }
                }
            }
            for digest in &detached.rdflib_projection.source_sha256 {
                if !projection.source_sha256.contains(digest) {
                    if projection.source_sha256.len() < 1024 {
                        projection.source_sha256.push(digest.clone());
                    } else {
                        projection.invalidate();
                        break;
                    }
                }
            }
        } else {
            projection.invalidate();
        }
        loaded.rdflib_projection = projection;
        loaded.rdflib_snapshot_cache.set(None);
        loaded.packages.insert(
            key,
            PackageRecord {
                digest: digest.to_owned(),
                retained: None,
            },
        );
        let result = json!({"package_id":id,"version":version,"digest":digest,"rdf_added":added,"revision":loaded.revision});
        *self = loaded;
        Ok(result)
    }
    /// Registration, retained exact bytes and loaded identity share this session's
    /// one package map. Failed detached parsing cannot publish any of them.
    pub(crate) fn register_prepared_package(
        &mut self,
        package: crate::packages::PreparedPackage,
    ) -> Result<semantica_contracts::SemanticPackageLoadResult> {
        let mut sources = Vec::new();
        for asset in &package.assets {
            if asset.loaded_into_dataset {
                sources.push((
                    asset.content.as_slice(),
                    crate::packages::rdf_format(asset.format.as_deref())?,
                    package.identity.namespace.as_deref(),
                    asset.graph_name.as_deref(),
                ));
            }
        }
        let mutation = self.load_verified_package(
            &package.identity.package_id,
            &package.identity.version,
            &package.identity.digest,
            &sources,
        )?;
        let key = (
            package.identity.package_id.clone(),
            package.identity.version.clone(),
        );
        let record = self
            .packages
            .get_mut(&key)
            .context("dataset_package_registration_missing")?;
        let retained =
            record
                .retained
                .get_or_insert_with(|| semantica_contracts::SemanticPackageLoadResult {
                    identity: package.identity,
                    assets: package.assets,
                    rdf_added: 0,
                    revision: 0,
                });
        let mut result = retained.clone();
        result.rdf_added = mutation["rdf_added"]
            .as_u64()
            .context("dataset_package_added_invalid")?;
        result.revision = self.revision;
        Ok(result)
    }
    pub fn package_identity(
        &self,
        id: &str,
        version: &str,
    ) -> Result<semantica_contracts::SemanticPackageIdentity> {
        self.packages
            .get(&(id.to_owned(), version.to_owned()))
            .and_then(|r| r.retained.as_ref())
            .map(|r| r.identity.clone())
            .context("dataset_package_not_registered")
    }
    /// Borrow the actual registration in this DatasetSession. Release checks
    /// share this record rather than accepting a parallel package registry.
    pub fn registered_package(
        &self,
        id: &str,
        version: &str,
    ) -> Option<&semantica_contracts::SemanticPackageLoadResult> {
        self.packages
            .get(&(id.to_owned(), version.to_owned()))
            .and_then(|record| record.retained.as_ref())
    }
    pub fn package_asset(
        &self,
        id: &str,
        version: &str,
        asset_id: &str,
    ) -> Result<semantica_contracts::SemanticPackageAsset> {
        self.packages
            .get(&(id.to_owned(), version.to_owned()))
            .and_then(|r| r.retained.as_ref())
            .and_then(|r| r.assets.iter().find(|a| a.asset_id == asset_id))
            .cloned()
            .context("dataset_package_asset_not_registered")
    }
    pub fn packages(&self) -> Vec<semantica_contracts::SemanticPackageIdentity> {
        self.packages
            .values()
            .filter_map(|r| r.retained.as_ref().map(|r| r.identity.clone()))
            .collect()
    }
    pub fn serialize(&self, format: RdfFormat, graph_name: Option<&str>) -> Result<String> {
        self.serialize_with_base(format, None, graph_name)
    }
    pub fn serialize_with_base(
        &self,
        format: RdfFormat,
        base_uri: Option<&str>,
        graph_name: Option<&str>,
    ) -> Result<String> {
        // Serialization uses the locked native writer. JSON-LD reading remains
        // separately guarded: this does not grant external context resolution.
        self.rdflib_projection.require_native_complete()?;
        if !matches!(format, RdfFormat::JsonLd { .. }) {
            format_supported(format)?;
        }
        ensure!(
            graph_name.is_none() || !dataset_format(format),
            "dataset_selected_graph_requires_graph_syntax"
        );
        let selected = graph_name
            .map(NamedNode::new)
            .transpose()?
            .map(GraphName::NamedNode);
        if selected.is_none() && !dataset_format(format) {
            ensure!(
                self.dataset
                    .iter()
                    .all(|quad| quad.graph_name.is_default_graph()),
                "dataset_lossy_graph_serialization"
            );
        }
        let mut serializer = RdfSerializer::from_format(format);
        if let Some(base) = base_uri {
            serializer = serializer.with_base_iri(base)?;
        }
        let mut writer = serializer.for_writer(Vec::new());
        for quad in self.dataset.iter() {
            if let Some(name) = &selected {
                if quad.graph_name != name.as_ref() {
                    continue;
                }
                let mut quad = quad.into_owned();
                quad.graph_name = GraphName::DefaultGraph;
                writer.serialize_quad(&quad)?;
            } else {
                writer.serialize_quad(quad)?;
            }
        }
        let bytes = writer.finish()?;
        ensure!(
            bytes.len() <= self.dataset_byte_limit(),
            "dataset_serialize_byte_budget"
        );
        Ok(String::from_utf8(bytes)?)
    }
    pub fn snapshot(&mut self) -> Result<SemanticDatasetSnapshot> {
        self.rdflib_projection.require_native_complete()?;
        if let Some(snapshot) = &self.snapshot_cache {
            return Ok(snapshot.clone());
        }
        let current = snapshot_with_resources(&self.dataset, self.revision, self.resource_profile)?;
        self.snapshot_cache = Some(current.clone());
        Ok(current)
    }
    /// Original RDFLib-v2 identity on the verified, lossless NT/NQ term
    /// projection owned by this same session. It is never inferred from RDF1.1
    /// terms that already collapsed plain and explicit xsd:string literals.
    pub fn snapshot_rdflib(
        &self,
        created_at: &str,
    ) -> Result<semantica_contracts::SemanticRdfLibDatasetSnapshot> {
        let when = if created_at.is_empty() {
            chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
        } else {
            created_at.into()
        };
        if let Some(cached) = self.rdflib_snapshot_cache.get() {
            let mut result = cached.clone();
            result.created_at = when;
            return Ok(result);
        }
        let quads = self
            .rdflib_projection
            .quads
            .as_ref()
            .context("rdflib_lossless_source_projection_unavailable")?;
        let result = crate::rdflib_snapshot::snapshot(quads, self.revision, &when);
        #[cfg(feature = "rdflib760-nullable-internal")]
        let result = if self.source_cache_enabled.is_some() {
            result.map_err(source_failure::SourceFailure::snapshot)
        } else {
            result
        };
        let result = result?;
        self.rdflib_snapshot_cache.set(Some(result.clone()));
        Ok(result)
    }
    /// Observe both identity domains from this same session. Only retained
    /// language spellings are inspected: absence of an alias does not prove
    /// stability, recover discarded spellings, or authenticate loaded sources.
    pub fn identity_diagnostic(
        &self,
        created_at: &str,
    ) -> Result<semantica_contracts::SemanticDatasetIdentityDiagnostic> {
        use crate::rdflib_snapshot::Node;
        use semantica_contracts::{
            SemanticDatasetIdentityDiagnostic, SemanticRdfLibLanguageCaseAlias,
        };
        ensure!(
            created_at.len() <= 1024,
            "identity_diagnostic_time_byte_budget"
        );
        let quads = self
            .rdflib_projection
            .quads
            .as_ref()
            .context("rdflib_lossless_source_projection_unavailable")?;
        ensure!(
            self.rdflib_projection.dataset()? == self.dataset,
            "identity_diagnostic_original_core_mismatch"
        );
        let mut spellings = BTreeMap::<(String, Option<String>, String), BTreeSet<String>>::new();
        for quad in quads {
            for node in [&quad.0, &quad.1, &quad.2]
                .into_iter()
                .chain(quad.3.as_ref())
            {
                if let Node::Literal(literal) = node {
                    if let Some(language) = &literal.language {
                        spellings
                            .entry((
                                literal.text.clone(),
                                literal.datatype.clone(),
                                language.to_ascii_lowercase(),
                            ))
                            .or_default()
                            .insert(language.clone());
                    }
                }
            }
        }
        let aliases = spellings
            .into_iter()
            .filter(|(_, variants)| variants.len() > 1)
            .map(|((text, datatype, language_lowercase), variants)| {
                SemanticRdfLibLanguageCaseAlias {
                    text_sha256: crate::sha256(text.as_bytes()),
                    datatype,
                    language_lowercase,
                    retained_spellings: variants.into_iter().collect(),
                }
            })
            .collect::<Vec<_>>();
        let result = SemanticDatasetIdentityDiagnostic {
            schema: "semantica.dataset-identity-diagnostic/1".into(),
            diagnostic_scope: "retained_projection_language_case/1".into(),
            legacy_stability_assessment: if aliases.is_empty() {
                "not_established"
            } else {
                "known_language_case_alias"
            }
            .into(),
            loaded_source_byte_sha256: self.rdflib_projection.source_sha256.clone(),
            language_case_aliases: aliases,
            legacy_snapshot: self.snapshot_rdflib(created_at)?,
            rdf11_snapshot: snapshot_with_resources(
                &self.dataset,
                self.revision,
                self.resource_profile,
            )?,
            revision: self.revision,
        };
        ensure!(
            serde_json::to_vec(&result)?.len() <= crate::MAX_BYTES,
            "identity_diagnostic_output_byte_budget"
        );
        Ok(result)
    }
    /// Explicit original source normalization and term-count domain. Generic
    /// load/query/RDFC contracts remain unchanged and report their own domain.
    pub fn load_rdflib(
        &mut self,
        bytes: &[u8],
        format: RdfFormat,
        graph_name: Option<&str>,
    ) -> Result<SemanticMutationResult> {
        self.load_rdflib_with_base(bytes, format, None, graph_name)
    }
    fn load_rdflib_with_base(
        &mut self,
        bytes: &[u8],
        format: RdfFormat,
        base_uri: Option<&str>,
        graph_name: Option<&str>,
    ) -> Result<SemanticMutationResult> {
        let before = self
            .rdflib_projection
            .quads
            .as_ref()
            .context("rdflib_lossless_source_projection_unavailable")?
            .clone();
        let mut staged = self.clone();
        staged.load(bytes, format, base_uri, graph_name)?;
        let after = staged
            .rdflib_projection
            .quads
            .as_ref()
            .context("rdflib_lossless_source_projection_unavailable")?;
        let result = SemanticMutationResult {
            operation: "load".into(),
            added: after.difference(&before).count() as u64,
            removed: 0,
            quad_count: after.len() as u64,
            revision: staged.revision,
        };
        *self = staged;
        Ok(result)
    }
    pub fn clear_rdflib(&mut self, graph_name: Option<&str>) -> Result<SemanticMutationResult> {
        let mut projection = self.rdflib_projection.clone();
        let quads = projection
            .quads
            .as_mut()
            .context("rdflib_lossless_source_projection_unavailable")?;
        let before = quads.len();
        match graph_name {
            None => {
                quads.clear();
                projection.core.clear();
                projection.source_sha256.clear();
            }
            Some("urn:x-rdflib:default") => quads.retain(|q| q.3.is_some()),
            Some(name) => {
                NamedNode::new(name)?;
                quads.retain(|q| q.3 != Some(crate::rdflib_snapshot::Node::Iri(name.into())));
            }
        }
        let after = quads.len();
        projection.core.retain(|q, _| quads.contains(q));
        #[cfg(feature = "rdflib760-nullable-internal")]
        if self.source_cache_enabled.is_some() {
            if graph_name.is_some()
                || !projection
                    .source_state
                    .as_ref()
                    .context("source_persistent_state_unavailable")?
                    .serialization_default_registered()?
            {
                self.source_namespaces.initialize(
                    true,
                    self.dataset_byte_limit(),
                    self.resource_profile.quads(),
                )?;
            }
            projection
                .source_state
                .as_mut()
                .context("source_persistent_state_unavailable")?
                .clear(
                    graph_name,
                    self.resource_profile.quads(),
                    self.dataset_byte_limit(),
                )?;
        }
        let staged = projection.native_projection()?;
        let mutation = self.publish_source_state("clear", staged, projection)?;
        Ok(SemanticMutationResult {
            operation: "clear".into(),
            added: 0,
            removed: (before - after) as u64,
            quad_count: after as u64,
            revision: mutation.revision,
        })
    }
    /// Original exact literal DELETE DATA semantics on verified lossless terms.
    /// Other SPARQL update forms stay unsupported in this explicit profile.
    pub fn update_rdflib(&mut self, text: &str) -> Result<SemanticMutationResult> {
        #[cfg(feature = "rdflib760-nullable-internal")]
        ensure!(
            self.source_contract != source_query::SourceSessionContract::DatasetUpdateV3,
            "source_UPDATE_v3_requires_explicit_options_and_loader"
        );
        #[cfg(feature = "rdflib760-nullable-internal")]
        if self.source_contract == source_query::SourceSessionContract::DatasetDataV2 {
            return self.update_source_internal(text, &BTreeMap::new());
        }
        use spargebra::GraphUpdateOperation;
        ensure!(text.len() <= 1024 * 1024, "rdflib_update_byte_budget");
        let parsed = spargebra::SparqlParser::new().parse_update(text)?;
        ensure!(
            parsed.operations.len() == 1
                && matches!(
                    parsed.operations[0],
                    GraphUpdateOperation::DeleteData { .. }
                ),
            "rdflib_update_form_unsupported"
        );
        let first = text.find('{').context("rdflib_delete_body_missing")?;
        let last = text.rfind('}').context("rdflib_delete_body_missing")?;
        let raw = crate::rdflib_snapshot::parse(text[first + 1..last].as_bytes(), None, 0)?;
        let GraphUpdateOperation::DeleteData { data } = &parsed.operations[0] else {
            unreachable!()
        };
        let intended = data
            .iter()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        let projected = raw
            .iter()
            .map(|q| Ok(crate::rdflib_snapshot::core_quad(q, None)?.to_string()))
            .collect::<Result<BTreeSet<_>>>()?;
        ensure!(
            projected == intended,
            "rdflib_delete_projection_ast_mismatch"
        );
        ensure!(
            raw.iter()
                .all(|q| !matches!(q.0, crate::rdflib_snapshot::Node::Blank(_))
                    && !matches!(q.2, crate::rdflib_snapshot::Node::Blank(_))),
            "rdflib_delete_blank_node_invalid"
        );
        let remove = crate::rdflib_snapshot::normalized(&raw)?;
        let mut projection = self.rdflib_projection.clone();
        let quads = projection
            .quads
            .as_mut()
            .context("rdflib_lossless_source_projection_unavailable")?;
        let before = quads.len();
        quads.retain(|q| !remove.contains(q));
        projection.core.retain(|q, _| quads.contains(q));
        #[cfg(feature = "rdflib760-nullable-internal")]
        if self.source_cache_enabled.is_some() {
            projection
                .source_state
                .as_mut()
                .context("source_persistent_state_unavailable")?
                .remove_quads(
                    remove.iter().map(source_query::quad_input),
                    self.resource_profile.quads(),
                    self.dataset_byte_limit(),
                )?;
        }
        let staged = projection.native_projection()?;
        let result = self.publish_source_state("update", staged, projection)?;
        Ok(SemanticMutationResult {
            operation: "update".into(),
            added: 0,
            removed: (before - self.rdflib_projection.quads.as_ref().unwrap().len()) as u64,
            quad_count: self.rdflib_projection.quads.as_ref().unwrap().len() as u64,
            revision: result.revision,
        })
    }
    pub fn diff(
        &mut self,
        before: &SemanticDatasetSnapshot,
        after: Option<&SemanticDatasetSnapshot>,
    ) -> Result<SemanticDatasetDiff> {
        if let Some(after) = after {
            diff_with_resources(before, after, self.resource_profile)
        } else {
            diff_with_resources(before, &self.snapshot()?, self.resource_profile)
        }
    }
    pub fn update(&mut self, update: &str) -> Result<SemanticMutationResult> {
        self.rdflib_projection.require_native_complete()?;
        ensure!(
            !update.trim().is_empty() && update.len() <= 1024 * 1024,
            "dataset_update_byte_budget"
        );
        let staged = std::thread::scope(|scope| -> Result<Dataset> {
            std::thread::Builder::new()
                .name("semantica-dataset-update".into())
                .stack_size(crate::graph::QUERY_STACK_BYTES)
                .spawn_scoped(scope, || self.stage_update(update))?
                .join()
                .map_err(|_| anyhow::anyhow!("dataset_update_panicked"))?
        })?;
        let result = self.publish("update", staged)?;
        self.rdflib_projection.invalidate();
        Ok(result)
    }
    fn stage_update(&self, text: &str) -> Result<Dataset> {
        use spargebra::GraphUpdateOperation;
        let parsed = spargebra::SparqlParser::new().parse_update(text)?;
        let repr = format!("{parsed:?}");
        ensure!(
            !repr.contains("Service {") && !repr.contains("Service("),
            "dataset_remote_service_unsupported"
        );
        for operation in &parsed.operations {
            match operation {
                GraphUpdateOperation::Load { .. } => bail!("dataset_remote_load_unsupported"),
                GraphUpdateOperation::Create { .. } => {
                    bail!("dataset_empty_graph_identity_unsupported")
                }
                GraphUpdateOperation::DeleteInsert { using, .. } => {
                    ensure!(using.is_none(), "dataset_update_remote_dataset_unsupported")
                }
                _ => {}
            }
        }
        let working = Store::new()?;
        for quad in self.dataset.iter() {
            working.insert(quad)?;
        }
        let mut roundtrip = Dataset::new();
        for quad in working.iter() {
            roundtrip.insert(&quad?);
        }
        ensure!(
            roundtrip == self.dataset,
            "dataset_update_store_changes_rdf_terms"
        );
        SparqlEvaluator::new()
            .parse_update(text)?
            .on_store(&working)
            .execute()?;
        let mut staged = Dataset::new();
        for quad in working.iter() {
            staged.insert(&quad?);
            ensure!(
                staged.len() <= self.resource_profile.quads(),
                "dataset_update_quad_budget"
            );
        }
        for graph in working.named_graphs() {
            ensure!(
                staged
                    .quads_for_graph_name(graph?.as_ref())
                    .next()
                    .is_some(),
                "dataset_empty_graph_identity_unsupported"
            );
        }
        budget(&staged, self.resource_profile)?;
        // Ground insert literals must also survive the backend round trip.
        // Other update forms remain subject to the explicit Store boundary.
        for operation in &parsed.operations {
            if let GraphUpdateOperation::InsertData { data } = operation {
                for quad in data {
                    let quad = Quad {
                        subject: quad.subject.clone(),
                        predicate: quad.predicate.clone(),
                        object: quad.object.clone(),
                        graph_name: match &quad.graph_name {
                            spargebra::term::GraphName::NamedNode(name) => name.clone().into(),
                            spargebra::term::GraphName::DefaultGraph => GraphName::DefaultGraph,
                        },
                    };
                    let check = Store::new()?;
                    check.insert(&quad)?;
                    ensure!(
                        check
                            .iter()
                            .next()
                            .context("dataset_update_insert_missing")??
                            == quad,
                        "dataset_update_store_changes_inserted_rdf_terms"
                    );
                }
            }
        }
        Ok(staged)
    }
}

impl semantica_contracts::SemanticRdfLibSnapshotPort for DatasetSession {
    fn snapshot_legacy(
        &self,
        input: &semantica_contracts::SemanticRdfLibSnapshotRequest,
    ) -> Result<
        semantica_contracts::SemanticRdfLibDatasetSnapshot,
        semantica_contracts::SemanticRuntimeFailure,
    > {
        input
            .validate()
            .map_err(|e| semantica_contracts::SemanticRuntimeFailure {
                code: "snapshot_input_invalid".into(),
                phase: "snapshot".into(),
                message: e.join(";"),
                retryable: false,
            })?;
        self.snapshot_rdflib(&input.created_at).map_err(|e| {
            semantica_contracts::SemanticRuntimeFailure {
                code: "rdflib_snapshot_failed".into(),
                phase: "snapshot".into(),
                message: e.to_string(),
                retryable: false,
            }
        })
    }
}

impl semantica_contracts::SemanticDatasetIdentityDiagnosticPort for DatasetSession {
    fn diagnose_identity(
        &self,
        input: &semantica_contracts::SemanticRdfLibSnapshotRequest,
    ) -> Result<
        semantica_contracts::SemanticDatasetIdentityDiagnostic,
        semantica_contracts::SemanticRuntimeFailure,
    > {
        input
            .validate()
            .map_err(|e| semantica_contracts::SemanticRuntimeFailure {
                code: "identity_diagnostic_input_invalid".into(),
                phase: "identity_diagnostic".into(),
                message: e.join(";"),
                retryable: false,
            })?;
        self.identity_diagnostic(&input.created_at).map_err(|e| {
            semantica_contracts::SemanticRuntimeFailure {
                code: "identity_diagnostic_failed".into(),
                phase: "identity_diagnostic".into(),
                message: e.to_string(),
                retryable: false,
            }
        })
    }
}

#[cfg(test)]
mod byte_budget_tests {
    use super::*;

    #[test]
    fn bounded_counter_matches_exact_nquads_bytes_for_rdf_terms() {
        let input = concat!(
            "<urn:s> <urn:p> \"机械\\n\\\"\\\\\" .\n",
            "_:a <urn:p> \"français\"@fr <urn:g> .\n",
            "<urn:s> <urn:q> \"01\"^^<http://www.w3.org/2001/XMLSchema#integer> _:g .\n",
            "<urn:空> <urn:p> _:a .\n",
        );
        let mut graph = Dataset::new();
        assert!(serialized_nquads_fit(&graph, 0));
        for quad in RdfParser::from_format(RdfFormat::NQuads).for_reader(input.as_bytes()) {
            graph.insert(&quad.unwrap());
            let actual = serialized_nquads(&graph).len();
            assert!(serialized_nquads_fit(&graph, actual));
            assert!(serialized_nquads_fit(&graph, actual + 1));
            assert!(!serialized_nquads_fit(&graph, actual - 1));
        }
    }

    #[test]
    fn counter_counts_utf8_bytes_and_rejects_without_underflow() {
        use std::fmt::Write;
        let mut counter = NQuadsByteBudget { remaining: 6 };
        counter.write_str("机械").unwrap();
        assert_eq!(counter.remaining, 0);
        assert!(counter.write_str("x").is_err());
        assert_eq!(counter.remaining, 0);
        counter.write_str("").unwrap();
    }
}

mod native_explorer;

#[cfg(all(test, feature = "rdflib760-nullable-internal"))]
mod source_update_constructor_receiving;
