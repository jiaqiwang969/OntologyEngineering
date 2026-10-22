# sem-provenance port notes

Port of `semantica/provenance` (Semantica 0.6.5+oe.7). Module files mirror the original:
`schemas.rs`, `storage.rs`, `manager.rs`, `integrity.rs`, `bridge_axiom.rs`; `__init__.py` ->
`lib.rs`, which re-exports `__all__` under the original names (plus `VERSION`, `AUTHOR`,
`DESCRIPTION`, `ALL`). Three supporting modules hold Python/rdflib behaviour the port needs:
`clock.rs` (`datetime.utcnow()` with a per-thread test seam), `isoformat.rs` (CPython 3.11's C
`datetime.fromisoformat`) and `rdf.rs` (rdflib 7.6 serialisation over oxigraph/oxrdf terms).
`provenance_usage.md` is the Rust edition of the original usage guide; it is the crate-level
documentation and its examples run as doctests.

## For crates built on sem-provenance

- **Manager.** `ProvenanceManager::new()?` (in-memory, or SQLite at the global default path),
  `with_storage_path(path)?`, `with_config(json)?`, `with_storage(backend)`,
  `with_shared_storage(Arc<dyn ProvenanceStorage>)`, `with_options(ProvenanceManagerOptions)`.
  Clones share the backend. `storage()` gives `&dyn ProvenanceStorage` (`.is::<SQLiteStorage>()`,
  `.downcast_ref::<T>()` replace `isinstance`).
- **Recording.** Python keyword arguments are option structs with builder setters:
  - `track_entity(id, source, Option<Map> metadata, TrackEntityOptions)` and
    `track_relationship(...)` — e.g.
    `TrackEntityOptions::new().entity_type("document").agent_id(..).agent_type("software_agent")
    .is_automated(true).activity_started_at_time(..).activity_ended_at_time(..)`; an
    `AgentRecord`/`ActivityRecord` goes in `.agent(..)`/`.activity(..)`.
  - `track_chunk(id, source_document, TrackChunkOptions)` (`source_path`, `span(start, end)`,
    `parent_chunk_id`, `metadata` = the `**metadata` map, which may carry `agent_id`, `agent_type`,
    `is_automated`, `role`, `activity_id`, `activity_started_at_time`, `activity_ended_at_time`
    exactly as Python callers pass them).
  - `track_property_source(entity_id, property, value, &SourceReference, TrackKwargs)`.
  - `track_entities_batch(&[Map], source, TrackEntityOptions, Map)` /
    `track_chunks_batch(&[Map], source_document, Option<&str>, TrackKwargs)`: entity/chunk dicts
    are JSON maps; in the free `Map`, keys from `ProvenanceManager::TRACK_ENTITY_KWARGS` become typed
    options (`TrackEntityOptions::from_kwargs` does the same split for callers forwarding Python
    `**kwargs`).
  - Tracking never fails: like the original it logs and returns `None` (or the previous version of
    a re-tracked entity). The private `_conn` argument is `track_entity_with_conn` /
    `track_chunk_with_conn` (errors propagate), `_save_entry` is `save_entry`,
    `_get_or_create_transaction` is `get_or_create_transaction`.
- **Queries** (`get_lineage`, `get_descendants`, `revision_history`, `lineage`, `audit_log`,
  `check`, `verify_chain`, `get_statistics`, `get_provenance`, ...) return `Result`, with dict
  results as `serde_json::Map` in the original key order. `audit_log` returns `AuditLog`
  (`Text` / `Records`). `export_prov(format, base_uri)` returns the RDF text; `prov_graph(base_uri)`
  returns the same graph as oxigraph triples (`ProvGraph::to_graph()`).
- **Storage.** `ProvenanceStorage` is an object-safe trait (the five abstract methods plus the
  original's defaults). `store_with_conn`/`retrieve_with_conn` are `_store_with_conn`/
  `_retrieve_with_conn`. Transactions are closures: `transaction(&mut |conn| ...)` commits on `Ok`
  and rolls back on `Err` (`ProvenanceStorageExt::with_transaction/with_savepoint` return values).
  The connection is `Option<&StorageConnection>` (`None` = the base default, `InMemory`,
  `Sqlite(&rusqlite::Connection)`, `Custom`).
- **Seams for tests that patch methods in Python** (`patch.object`): wrap a backend in a type that
  overrides the method and delegates the rest; `InMemoryStorage::transaction_via(this, body)` /
  `savepoint_via` commit staged entries through `this.store` (Python's `self.store` dispatch);
  `SQLiteStorage::set_configure_connection_hook` replaces `_configure_connection`;
  `sem_provenance::clock::fixed_clock(start, step)` makes `utcnow()` deterministic per thread.
- **Default storage path.** `default_storage_path(Some(path))` / `ProvenanceManager::
  default_storage_path(..)` return a guard that restores the previous default on drop and, like the
  original's `RLock`, holds the class lock for its lifetime (other threads' `ProvenanceManager::new()`
  wait). `ProvenanceManager::set_default_storage_path`, `current_default_storage_path()`.
- **Errors.** `sem_utils::SemanticaError`: `Value` (`invalidate` of an unknown entity, ISO parsing),
  `Type`, `Runtime`, ...; SQLite failures are `Other { module: "sqlite3", type_name:
  "OperationalError" | "IntegrityError" | ... }` with SQLite's message, chosen by result code as
  CPython does; rdflib errors are `Other { module: "builtins", type_name: "Exception" }` /
  `PluginException`.

## Deviations from the original

### Types
- `ProvenanceEntry` and the other dataclasses have the annotated field types. Python stores any
  object; `from_dict` here raises `TypeError` for a value of the wrong type (unexpected/missing
  keyword errors keep CPython's exact messages). A whole-number confidence is a float, so an entry
  created with `confidence=1` (int) checksums with `"1.0"` where Python would hash `"1"`.
- Explicit `None` for a keyword argument with a non-`None` default (`agent_id`, `agent_type`,
  `is_automated`, `entity_type`, `activity_id`, `confidence`) means "use the default"; Python stores
  `None`. In kwargs maps, non-string scalars for string options are converted with `str()`; a
  `None` `used_entities`/`informed_by` is ignored (Python's `list(None)` raises inside
  `track_entity`, which then degrades to `None`).
- A SQLite `NULL` in a column whose field is not optional (`agent_id`, `entity_type`, `confidence`,
  `version`) reads as `""`/`1.0`/`"1.0"` (Python returns `None`); only hand-written rows can contain
  one. A `used_entities`/`informed_by_activities` cell holding JSON that is not a list of strings, or
  a `metadata` cell that is not a JSON object, raises `TypeError`; invalid JSON raises
  `JSONDecodeError` with serde's wording.
- Batch dicts: a non-string entity id fails that item (as Python's `track_entity` `TypeError`); a
  non-string chunk id is converted with `str()`; a non-integer `start_index`/`end_index` fails the
  item (Python stores the raw value).
- `compute_checksum` on a dict renders a non-string `used_entities` item with `repr()` and a
  non-iterable truthy `used_entities` with `str()` instead of raising `TypeError` (strings and
  dicts are joined character-/key-wise as in Python).
- `BridgeAxiom.apply` and chain layers keep JSON values (ints stay ints); a result that is `inf`
  or `nan` becomes `null` (`serde_json` cannot hold it).
- `datetime.fromisoformat`: a UTC offset with a fractional second keeps whole seconds
  (`PyDateTime` holds a `chrono::FixedOffset`): `+01:30:15.5` prints as `+01:30:15`. Every other
  accepted/rejected form and error message matches CPython 3.11 (68 oracle cases).

### Object identity
- Storages return copies. Python's `InMemoryStorage.retrieve()` returns the stored object, so a
  caller mutating it mutates storage; here it cannot. The original's deep-copy guarantees (rollback
  returns a safe copy) hold.
- `SourceReference.from_dict` / `PropertySource.from_dict` do not rewrite the caller's dict.

### Ordering where Python iterates a set
- `get_lineage()["source_documents"]` is in first-seen order (Python: `list(set(...))`).
- `SQLiteStorage.trace_descendants` lists the children of one BFS level in row order (Python
  iterates a `set`).
- RDF export: rdflib's N-Triples, JSON-LD, RDF/XML and HexTuples statement/node order follows
  Python set iteration and changes between runs; the port writes insertion order. In Turtle,
  sibling blank nodes of one predicate (several `qualifiedDerivation`/`qualifiedUsage` blocks)
  follow creation order instead of rdflib's random blank-node ids. Everything else in Turtle —
  prefixes, subject order, prefixed-name rules (`split_uri`/namespace trie), escaping, indentation,
  `xsd:dateTime` lexical normalisation — matches rdflib byte for byte. Blank-node ids are random
  `N<uuid4 hex>` as in rdflib.

### RDF export
- Serialisers ported: `turtle`/`ttl`/`text/turtle`/`n3`/`text/n3` (rdflib's N3 output is the
  Turtle output for these graphs), `nt`/`ntriples`/`nt11`/`application/n-triples`,
  `json-ld`/`jsonld`/`application/ld+json`, `xml`/`application/rdf+xml`, `hext`. `nquads`, `trix`
  and `patch` raise the errors rdflib raises for a plain graph; unknown names raise
  `PluginException`. Not ported: `pretty-xml`, `longturtle`, `trig`/`application/trig` (they raise
  `NotImplementedError`).
- Unicode categories for prefixed-name splitting come from the `regex` crate's tables (newer than
  Python 3.11's Unicode 14), which only matters for characters assigned after Unicode 14. Literals
  sharing a subject and predicate sort by lexical form (rdflib compares values). A `base_uri` equal
  to one of rdflib's default-bound namespaces is not handled as rdflib's rebinding does.

### Logging
- Messages are the original's; `exc_info` tracebacks are not modelled (sem-utils).

### Other
- `max_depth` is `Option<usize>` (a negative Python depth behaves like `0`: no results).
- `ProvenanceManager::new()` returns `Result` because opening the default SQLite path can fail, as
  `ProvenanceManager()` can raise in Python.
- SQLite is the bundled library of `rusqlite` 0.32 (SQLite 3.46; the oracle runs 3.54); no
  observable difference was found. rusqlite's `": <path>"` suffix on open failures is removed so
  the message is SQLite's (`unable to open database file`).

## Tests
- Ported originals (one `#[test]` per function, classes as `<class_snake>__<test>`):
  `provenance_test_bridge_axiom.rs`, `provenance_test_schemas.rs`, `provenance_test_storage.rs`,
  `provenance_test_sqlite_storage_performance_807.rs` (all 69 passed in the original baseline).
  - SQLite tests use `tempfile` paths (the original: `NamedTemporaryFile(delete=False)` + unlink).
  - Where the original patches a method (`patch.object(storage, "transaction")`,
    `"_configure_connection"`), the port wraps the backend or uses the configure hook.
  - `test_*_does_not_count_individually_failed_items` and
    `test_track_entity_standalone_call_still_degrades_gracefully` store a Python `set` in
    metadata so that `json.dumps` fails for one item; every `serde_json::Value` serialises, so a
    wrapper around the real SQLite storage raises the same `TypeError` for the stand-in `"bad"`
    key.
  - `test_custom_storage_trace_lineage_one_argument_backward_compatibility`: a backend written for
    the one-argument `trace_lineage` declares `supports_max_depth() == false` (Python inspects the
    signature).
  - `test_retrieve_and_trace_lineage_do_not_block_on_writer_lock` waits 10 s instead of 2 s for
    the reader (shared, heavily loaded machine).
- Characterisation tests compare with `tests/fixtures/oracle.json`, produced by running the
  original in the oracle with `tests/fixtures/generate_oracle.py` (command in its docstring). The
  oracle replaces `datetime.utcnow()` with `2026-01-01T00:00:00 + n * 1.25 s`; the Rust tests
  install the same clock and also check how many times it was read. Manager scenarios compare
  entries, checksums, lineage, history, audit logs and chain verification exactly (InMemory and
  SQLite); exports compare Turtle byte for byte (or as canonicalised graphs where rdflib orders
  sibling blank nodes randomly) and N-Triples/JSON-LD/RDF-XML as masked statement/node sets.
- `provenance_failures.rs` replays the failure scenarios of `tests/provenance/test_manager.py`
  (owned by sem-core) against this API: swallowed storage errors, rollback on history/primary write
  failure, safe copies, per-item savepoints and the logged messages.
