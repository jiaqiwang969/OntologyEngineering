# Provenance Tracking Module - Usage Guide

**W3C PROV-O compliant provenance tracking for high-stakes domains requiring complete traceability**

This is the Rust edition of `semantica/provenance/provenance_usage.md`. Every Python call in the
original guide has a direct counterpart here; the examples below are compiled and run as tests.

## Table of Contents

1. Overview
2. Key Features
3. Installation
4. Quick Start
5. Core Concepts
6. API Reference
7. Usage Examples
8. High-Stakes Domain Examples
9. Bridge Axiom Translation Chains
10. Storage Backends
11. Integration with Existing Modules
12. Best Practices
13. Performance
14. Compliance Standards
15. Testing
16. Troubleshooting

---

## Overview

The provenance module provides W3C PROV-O compliant tracking for knowledge graphs, enabling
complete end-to-end lineage from source documents to query responses. It consolidates the
provenance tracking of:

- **kg.ProvenanceTracker** - entity/relationship tracking
- **split.ProvenanceTracker** - chunk tracking
- **conflicts.SourceTracker** - source tracking

Designed for high-stakes domains: blue finance and natural capital, healthcare, legal,
pharmaceutical and financial services.

## Key Features

- **W3C PROV-O compliant** - `prov:Entity`, `prov:Activity`, `prov:Agent`, `prov:wasDerivedFrom`,
  qualified associations/generations/usages/derivations/invalidations
- **Source tracking** - document identifiers, page numbers, sections and quotes
- **Multiple storage backends** - [`InMemoryStorage`] (fast) and [`SQLiteStorage`] (persistent),
  or any type implementing [`ProvenanceStorage`]
- **Bridge axiom support** - translation chain tracking (L1 → L2 → L3)
- **Integrity verification** - SHA-256 checksums plus a hash chain that exposes deleted rows
- **Complete lineage tracing** - upstream ([`ProvenanceManager::get_lineage`]) and downstream
  ([`ProvenanceManager::get_descendants`])
- **Tombstones, not deletes** - [`ProvenanceManager::invalidate`]

## Installation

Add the crate to a workspace member's `Cargo.toml`:

```toml
[dependencies]
sem-provenance = { path = "../sem-provenance" }
```

`from semantica.provenance import ProvenanceManager` becomes `use sem_provenance::ProvenanceManager;`.

## Quick Start

### Basic Usage

```rust
use sem_provenance::{ProvenanceManager, TrackEntityOptions};

// Initialize manager (in-memory)
let prov_mgr = ProvenanceManager::new()?;

// Track entity provenance
prov_mgr.track_entity(
    "entity_1",
    "DOI:10.1371/journal.pone.0023601",
    None,
    TrackEntityOptions::new()
        .source_location("Figure 2")
        .source_quote("Total fish biomass increased by 463%")
        .confidence(0.92),
);

// Trace complete lineage
let lineage = prov_mgr.get_lineage("entity_1")?;
assert_eq!(lineage["source_documents"][0], "DOI:10.1371/journal.pone.0023601");
# Ok::<(), sem_utils::SemanticaError>(())
```

Python keyword arguments map to option structs: [`TrackEntityOptions`] for `track_entity`,
`track_relationship` and `track_entities_batch`; [`TrackChunkOptions`] for `track_chunk`;
[`TrackKwargs`] (agent/activity records plus the free-form `**metadata` map) for
`track_property_source` and `track_chunks_batch`.

### Persistent Storage

```rust
use sem_provenance::{ProvenanceManager, TrackEntityOptions};

let dir = tempfile::tempdir().unwrap();
let path = dir.path().join("provenance.db");

// Use SQLite for persistent storage
let prov_mgr = ProvenanceManager::with_storage_path(path.to_str().unwrap())?;

// All operations work the same
prov_mgr.track_entity("entity_1", "doc_1", None, TrackEntityOptions::default());

// A new manager on the same file sees the data
let again = ProvenanceManager::with_storage_path(path.to_str().unwrap())?;
assert!(again.get_provenance("entity_1")?.is_some());
# Ok::<(), sem_utils::SemanticaError>(())
```

---

## Core Concepts

### W3C PROV-O Entities

- **prov:Entity** - things (entities, chunks, properties)
- **prov:Activity** - actions (extraction, chunking, reasoning)
- **prov:Agent** - who or what performed the activity (Semantica, a user, a model)
- **prov:wasDerivedFrom** - parent-child relationships
- **prov:used** - dependencies between entities

### ProvenanceEntry Structure

[`ProvenanceEntry`] is a plain struct with public fields in the original's order, e.g.
`entity_id` (`prov:Entity`), `entity_type`, `activity_id` (`prov:Activity`), `agent_id`/`agent_type`
(`prov:Agent`), `source_document`, `source_location`, `source_quote`, `timestamp`
(`prov:generatedAtTime`), `confidence`, `checksum`, `parent_entity_id` (`prov:wasDerivedFrom`),
`used_entities` (`prov:used`), `previous_version_id`, `derived_from_id`, the invalidation fields and
`metadata`. `to_dict()`/`from_dict()` give the original dictionary form.

```rust
use sem_provenance::ProvenanceEntry;

let entry = ProvenanceEntry {
    source_document: "DOI:10.1371/journal.pone.0023601".into(),
    source_location: Some("Figure 2".into()),
    confidence: 0.92,
    ..ProvenanceEntry::new("entity_123", "named_entity", "ner_extraction")
};
let data = entry.to_dict();
assert_eq!(data["agent_id"], "semantica");
assert_eq!(ProvenanceEntry::from_dict(&data)?, entry);
# Ok::<(), sem_utils::SemanticaError>(())
```

---

## API Reference

### ProvenanceManager

| Python | Rust |
|---|---|
| `ProvenanceManager()` | `ProvenanceManager::new()?` |
| `ProvenanceManager(storage_path=p)` | `ProvenanceManager::with_storage_path(p)?` |
| `ProvenanceManager(storage=s)` | `ProvenanceManager::with_storage(s)` / `with_shared_storage(arc)` |
| `ProvenanceManager(config=c)` | `ProvenanceManager::with_config(json)?` |
| `with default_storage_path(p):` | `let _guard = default_storage_path(Some(p));` |
| `track_entity(id, source, metadata, **kw)` | `track_entity(id, source, Option<Map>, TrackEntityOptions)` |
| `track_relationship(id, source, metadata, **kw)` | `track_relationship(id, source, Option<Map>, TrackEntityOptions)` |
| `track_chunk(id, doc, source_path, start, end, parent, **md)` | `track_chunk(id, doc, TrackChunkOptions)` |
| `track_property_source(id, prop, value, source, **md)` | `track_property_source(id, prop, value, &SourceReference, TrackKwargs)` |
| `track_entities_batch(entities, source, **md)` | `track_entities_batch(&[Map], source, TrackEntityOptions, Map)` |
| `track_chunks_batch(chunks, doc, source_path, **md)` | `track_chunks_batch(&[Map], doc, Option<&str>, TrackKwargs)` |
| `get_lineage` / `trace_lineage` / `get_all_sources` | same names; dict results are `serde_json::Map` |
| `trace_descendants` / `get_descendants` | same names |
| `revision_history` / `query_recorded_between` | same names |
| `invalidate(id, agent_id, reason, metadata)` | `invalidate(id, agent_id, Option<&str>, Option<Map>)?` |
| `check(strict)` / `verify_chain()` | same names |
| `lineage(id, depth)` / `audit_log(since, format)` / `export_prov(format, base_uri)` | same names |
| `get_statistics()` / `clear()` / `get_provenance(id)` | same names |

Tracking methods never fail: like the original they log a storage failure and return `None` (or
the previous version of a re-tracked entity). Query methods return `Result` because the original
raises storage errors.

---

## Usage Examples

### Basic Entity Tracking

```rust
use sem_provenance::{ProvenanceManager, TrackEntityOptions};

let prov_mgr = ProvenanceManager::new()?;
prov_mgr.track_entity(
    "entity_1",
    "research_paper.pdf",
    None,
    TrackEntityOptions::new().confidence(0.9),
);
let prov = prov_mgr.get_provenance("entity_1")?.expect("tracked");
assert_eq!(prov["confidence"], 0.9);
# Ok::<(), sem_utils::SemanticaError>(())
```

### Chunk Tracking

```rust
use sem_provenance::{ProvenanceManager, TrackChunkOptions};

let prov_mgr = ProvenanceManager::new()?;
prov_mgr.track_chunk(
    "chunk_1",
    "research_paper.pdf",
    TrackChunkOptions::new()
        .source_path("/data/papers/research_paper.pdf")
        .span(0, 500),
);
// Track child chunk
prov_mgr.track_chunk(
    "chunk_2",
    "research_paper.pdf",
    TrackChunkOptions::new().span(500, 1000).parent_chunk_id("chunk_1"),
);
let lineage = prov_mgr.trace_lineage("chunk_2", None)?;
assert_eq!(lineage.len(), 2);
# Ok::<(), sem_utils::SemanticaError>(())
```

### Property Source Tracking

```rust
use sem_provenance::{ProvenanceManager, SourceReference, TrackKwargs};

let prov_mgr = ProvenanceManager::new()?;
let source = SourceReference {
    page: Some(4),
    section: Some("Table S4".into()),
    confidence: 0.92,
    ..SourceReference::new("DOI:10.1038/s41586-021-03371-z")
};
let entry = prov_mgr
    .track_property_source(
        "cabo_pulmo_mpa",
        "biomass_tourism_elasticity",
        0.346,
        &source,
        TrackKwargs::default(),
    )
    .expect("tracked");
assert_eq!(entry.entity_id, "cabo_pulmo_mpa_biomass_tourism_elasticity");
assert_eq!(entry.source_location.as_deref(), Some("page_4"));
# Ok::<(), sem_utils::SemanticaError>(())
```

### Batch Operations

```rust
use sem_provenance::{ProvenanceManager, TrackEntityOptions};
use serde_json::{Map, Value, json};

let prov_mgr = ProvenanceManager::new()?;
let entities: Vec<Map<String, Value>> = ["entity_1", "entity_2", "entity_3"]
    .iter()
    .map(|id| json!({"id": id}).as_object().unwrap().clone())
    .collect();
// Typed keyword arguments go in the options; other keys become metadata of every entity.
let mut metadata = Map::new();
metadata.insert("extraction_method".into(), json!("NER"));
let count = prov_mgr.track_entities_batch(
    &entities,
    "doc_1",
    TrackEntityOptions::new().agent_id("ner_service"),
    metadata,
);
assert_eq!(count, 3);
# Ok::<(), sem_utils::SemanticaError>(())
```

### Versioning, Invalidation and the Hash Chain

```rust
use sem_provenance::{ProvenanceManager, TrackEntityOptions};

let prov_mgr = ProvenanceManager::new()?;
prov_mgr.track_entity("X", "doc_1", None, TrackEntityOptions::default());
// Re-tracking archives the previous version under "X:v:<last_updated>".
let v2 = prov_mgr
    .track_entity("X", "doc_2", None, TrackEntityOptions::new().revision_type("correction"))
    .expect("tracked");
assert!(v2.previous_version_id.unwrap().starts_with("X:v:"));
assert_eq!(prov_mgr.revision_history("X")?.len(), 2);

// Retract a fact without deleting it.
let tombstone = prov_mgr.invalidate("X", "reviewer_jane", Some("source retracted"), None)?;
assert!(tombstone.invalidated);

// Every entry chains its checksum to its predecessor.
assert_eq!(prov_mgr.verify_chain()?["valid"], true);
# Ok::<(), sem_utils::SemanticaError>(())
```

### W3C PROV-O Export

```rust
use sem_provenance::{ProvenanceManager, TrackEntityOptions};

let prov_mgr = ProvenanceManager::new()?;
prov_mgr.track_entity("e1", "doc1", None, TrackEntityOptions::new().agent_id("alice").agent_type("person"));
let turtle = prov_mgr.export_prov("turtle", None)?;
assert!(turtle.contains("ex:e1 a prov:Entity"));
assert!(turtle.contains("ex:alice a prov:Agent,\n        prov:Person"));
// The same graph as oxigraph triples:
assert!(!prov_mgr.prov_graph(None)?.is_empty());
# Ok::<(), sem_utils::SemanticaError>(())
```

---

## High-Stakes Domain Examples

The original walks through blue finance (MARIS), healthcare and legal audit trails. They all
combine the calls above: track the measurement or observation with its source details, apply a
bridge axiom, then report the lineage.

```rust
use sem_provenance::{ProvenanceManager, TrackEntityOptions};
use sem_provenance::bridge_axiom::BridgeAxiom;
use serde_json::{Map, json};

let prov_mgr = ProvenanceManager::new()?;
// L1: Ecological measurement
prov_mgr.track_entity(
    "cabo_pulmo_biomass",
    "DOI:10.1371/journal.pone.0023601",
    None,
    TrackEntityOptions::new()
        .entity_type("ecological_measurement")
        .source_location("Figure 2")
        .source_quote("Total fish biomass increased by 463%")
        .confidence(0.95),
);
// L2: Bridge axiom (ecological -> financial translation)
let axiom = BridgeAxiom {
    input_domain: "ecological".into(),
    output_domain: "financial".into(),
    confidence: 0.92,
    ..BridgeAxiom::new(
        "BA-FINANCE-001",
        "biomass_tourism_elasticity",
        "1% biomass increase -> 0.346% tourism revenue increase",
        0.346,
        "10.1038/s41586-021-03371-z",
        "Table S4",
    )
};
let result = axiom.apply("cabo_pulmo_biomass", 463, Some(&prov_mgr), &Map::new())?;
assert_eq!(result["output_value"], json!(160.19799999999998));
// L3: Audit report
let report = prov_mgr.get_lineage(result["output_entity"].as_str().unwrap())?;
assert_eq!(report["entity_count"], 1);
# Ok::<(), sem_utils::SemanticaError>(())
```

---

## Bridge Axiom Translation Chains

Supported domain translations: ecological → financial, clinical → diagnostic, evidence → legal
conclusion, research data → drug efficacy, raw data → financial metrics, sensor data → security
threat, biological data → biomedical insights, asset data → portfolio risk.

### Multi-Layer Translation Chain

```rust
use sem_provenance::ProvenanceManager;
use sem_provenance::bridge_axiom::{BridgeAxiom, create_translation_chain, trace_translation_chain};
use serde_json::json;

let prov_mgr = ProvenanceManager::new()?;
let input_data = json!({"entity_id": "cabo_pulmo_biomass", "value": 463, "source": "DOI:10.1371/..."});
let axioms = vec![
    BridgeAxiom::new("BA-001", "biomass_tourism", "Test", 0.346, "10.1038/x", "Table S4"),
    BridgeAxiom { confidence: 0.9, ..BridgeAxiom::new("BA-002", "tourism_revenue", "Test", 2.0, "10.1016/y", "p. 7") },
];
let chain = create_translation_chain(input_data.as_object().unwrap(), &axioms, Some(&prov_mgr))?;
assert_eq!(chain.layers.len(), 4); // L1, two axiom layers, output
assert_eq!(chain.confidence, 0.9);
let trace = trace_translation_chain(&chain, &prov_mgr);
assert_eq!(trace["layers"].as_array().unwrap().len(), 4);
# Ok::<(), sem_utils::SemanticaError>(())
```

---

## Storage Backends

### InMemoryStorage (Default)

Fast; data is lost when the process ends. Use for development, testing and short-lived processes.

### SQLiteStorage

Persistent (WAL journal, `busy_timeout=5000`, `synchronous=NORMAL`); one connection per call, so
the file can be removed as soon as a call returns. Use for production and audit trails. A
database created by an older schema is migrated in place when opened.

### Custom Storage Backend

Implement [`ProvenanceStorage`] (the five abstract methods; the others have the original's
defaults) and pass it to [`ProvenanceManager::with_storage`]:

```rust
use std::sync::Mutex;
use sem_provenance::{ProvenanceEntry, ProvenanceManager, ProvenanceStorage, TrackEntityOptions};
use sem_utils::SemanticaError;

#[derive(Default)]
struct VecStorage(Mutex<Vec<ProvenanceEntry>>);

impl ProvenanceStorage for VecStorage {
    fn store(&self, entry: &ProvenanceEntry) -> Result<(), SemanticaError> {
        let mut entries = self.0.lock().unwrap();
        entries.retain(|e| e.entity_id != entry.entity_id);
        entries.push(entry.clone());
        Ok(())
    }
    fn retrieve(&self, entity_id: &str) -> Result<Option<ProvenanceEntry>, SemanticaError> {
        Ok(self.0.lock().unwrap().iter().find(|e| e.entity_id == entity_id).cloned())
    }
    fn retrieve_all(&self, entity_type: Option<&str>) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        let entries = self.0.lock().unwrap();
        Ok(entries
            .iter()
            .filter(|e| entity_type.is_none_or(|t| t.is_empty() || e.entity_type == t))
            .cloned()
            .collect())
    }
    fn trace_lineage(&self, entity_id: &str, _max_depth: Option<usize>) -> Result<Vec<ProvenanceEntry>, SemanticaError> {
        Ok(self.retrieve(entity_id)?.into_iter().collect())
    }
    fn clear(&self) -> Result<usize, SemanticaError> {
        let mut entries = self.0.lock().unwrap();
        let count = entries.len();
        entries.clear();
        Ok(count)
    }
}

let prov_mgr = ProvenanceManager::with_storage(VecStorage::default());
prov_mgr.track_entity("entity_1", "doc_1", None, TrackEntityOptions::default());
assert_eq!(prov_mgr.get_statistics()?["total_entries"], 1);
# Ok::<(), sem_utils::SemanticaError>(())
```

---

## Integration with Existing Modules

The `*_provenance` wrappers of the other modules (`kg`, `split`, `semantic_extract`, `llms`,
`pipeline`, `context`, `ingest`, `embeddings`, `graph_store`, `vector_store`, `triplet_store`,
`reasoning`, `conflicts`, `deduplication`, `export`, `parse`, `normalize`, `ontology`,
`visualization`) live in their own crates and record through this crate's
[`ProvenanceManager`] exactly as above: `track_entity(id, source, metadata, options)` with
`agent_id`, `agent_type`, `is_automated` and the activity start/end times set in
[`TrackEntityOptions`].

---

## Best Practices

1. **Always include source details** - `source_location`, `source_quote` and `confidence`.
2. **Use persistent storage for production** - [`ProvenanceManager::with_storage_path`].
3. **Track confidence scores.**
4. **Use metadata for domain-specific information.**
5. **Verify integrity with checksums**:

```rust
use sem_provenance::{ProvenanceEntry, compute_checksum, verify_checksum};

let mut entry = ProvenanceEntry::new("entity_1", "entity", "extraction");
entry.checksum = Some(compute_checksum(&entry));
assert!(verify_checksum(&entry, None));
entry.source_document = "tampered".into();
assert!(!verify_checksum(&entry, None));
```

6. **Use batch operations for many entities** - one transaction per 1,000 entities, one
   savepoint per entity.

## Performance

In-memory lookups are O(1); SQLite lineage tracing fetches each BFS level with batched
`IN (...)` queries; batch tracking shares one transaction per 1,000 items.

## Compliance Standards Support

The module provides technical capabilities (W3C PROV-O, SHA-256 integrity, hash chains,
tombstones) that support FDA 21 CFR Part 11, SOX, HIPAA, TNFD and similar requirements.
Organizations must implement additional policies, procedures, validation and controls; Semantica
does not provide regulatory certification or legal compliance guarantees.

## Testing

```text
cargo test -p sem-provenance
```

Tests can make timestamps deterministic with [`clock::fixed_clock`].

## Troubleshooting

- **`track_entity` returned `None`** - the storage failed; the error is logged to the
  `semantica.provenance_manager` logger.
- **`database is locked`** - another process held the SQLite writer lock for more than the
  5-second busy timeout.
- **`export_prov` failed with "does not look like a valid URI"** - an entity, agent or activity
  id contains a character Turtle/N-Triples cannot carry (space, quote, `<>{}|\^` or backtick);
  JSON-LD and RDF/XML exports still work.
