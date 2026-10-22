---
title: "Migrating from kg.ProvenanceTracker"
description: "How to move from the deprecated semantica.kg.ProvenanceTracker to the unified semantica.provenance.ProvenanceManager."
---

## Why migrate

`semantica.kg.ProvenanceTracker` is deprecated and will be removed in a future major version. It was a standalone, in-memory implementation that never delegated to the unified provenance backend — `semantica.provenance.ProvenanceManager` is that backend, and is now the supported way to track entity and relationship provenance across every Semantica module (see the [Provenance & Audit Trails guide](/guides/provenance)).

Every method on `kg.ProvenanceTracker` now emits a `DeprecationWarning` on use, but existing code keeps working unchanged until the class is removed — there is no forced migration deadline yet.

## Method mapping

| `kg.ProvenanceTracker` | `ProvenanceManager` equivalent | Notes |
| --- | --- | --- |
| `ProvenanceTracker()` | `ProvenanceManager()` | `ProvenanceManager` also accepts `storage_path=` for SQLite persistence instead of in-memory only. |
| `track_entity(entity_id, source, metadata)` | `track_entity(entity_id, source, metadata)` | Same call shape. `ProvenanceManager` additionally auto-links each update to its prior version via `parent_entity_id`. |
| `get_all_sources(entity_id)` | `get_all_sources(entity_id)` | Field name differs: the `kg` tracker returns each record's time under `"recorded_at"`; `ProvenanceManager` returns `"timestamp"`. |
| `clear(entity_id=None)` | `clear()` | `ProvenanceManager.clear()` clears all provenance data; there is no per-entity clear yet. |
| `query_recorded_between(start, end)` | `query_recorded_between(start, end)` | Same call shape; filters by `timestamp` (ISO 8601 string comparison) across all tracked entries, not just one entity. |
| `revision_history(fact_id)` | `revision_history(fact_id)` | Same call shape and return shape (`version`, `valid_from`, `valid_until`, `recorded_at`, `author`, optional `revision_type`/`supersedes`) — walks the entity's `previous_version_id` chain rather than a flat per-entity dict. |
| `export_audit_log(fact_ids, format)` | *No direct equivalent yet* | Build the export from `get_lineage()` output, or serialize `get_statistics()` for a summary view. |

Methods with no direct equivalent are not planned to be reimplemented on `kg.ProvenanceTracker` — they will need a small adapter in caller code, or a feature request against `ProvenanceManager` if you rely on them heavily.

## Example

```python
# Before
from semantica.kg import ProvenanceTracker

tracker = ProvenanceTracker()
tracker.track_entity("entity_1", source="doc_1", metadata={"confidence": 0.9})
sources = tracker.get_all_sources("entity_1")  # [{"source": ..., "recorded_at": ..., "confidence": 0.9}]

# After
from semantica.provenance import ProvenanceManager

prov = ProvenanceManager()
prov.track_entity("entity_1", source="doc_1", metadata={"confidence": 0.9})
sources = prov.get_all_sources("entity_1")  # [{"source": ..., "timestamp": ..., "metadata": {...}, ...}]
```

## Suppressing the warning during migration

If you need to keep using `kg.ProvenanceTracker` temporarily and want to silence the warning while you plan the switch:

```python
import warnings

with warnings.catch_warnings():
    warnings.simplefilter("ignore", DeprecationWarning)
    tracker = ProvenanceTracker()
```

This is a stopgap, not a fix — plan to move to `ProvenanceManager` before `kg.ProvenanceTracker` is removed.
