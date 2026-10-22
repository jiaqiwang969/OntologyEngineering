# sem-change port notes

Port of `semantica/change_management` (Semantica 0.6.5+oe.7) plus
`semantica/ontology/namespace_manager.py`, which lives here to break the original's
`ontology` <-> `change_management` import cycle (sem-ontology re-exports it).

| Original | Rust |
|---|---|
| `change_management/__init__.py` | `src/lib.rs` (re-exports `__all__`; `VERSION`/`AUTHOR`/`DESCRIPTION`) |
| `change_management/change_log.py` | `src/change_log.rs` |
| `change_management/version_storage.py` | `src/version_storage.rs` |
| `change_management/managers.py` | `src/managers.rs` |
| `change_management/ontology_version_manager.py` | `src/ontology_version_manager.rs` |
| `ontology/namespace_manager.py` | `src/namespace_manager.rs` |
| `change_management/change_management_usage.md` | `docs/change_management_usage.md` (byte-identical) |
| — | `src/pycompat.rs` (private): CPython behaviour the port needs, see below |

Dependencies: `sem-utils` only. The pre-declared `sem-provenance`, `oxigraph`, `thiserror`,
`once_cell` and `uuid` were unused and are not depended on; `md-5` (hash-based IRIs) and
`unicode-normalization` (`urlsplit`'s NFKC netloc check) come from `[workspace.dependencies]`.

## For crates built on sem-change (sem-kg, sem-context, sem-ontology)

- **Storage** — `trait VersionStorage: Send + Sync` with `save`, `get`, `list_all`, `exists`,
  `delete`, `save_tag`, `get_tag`, `list_tags`, `save_mutation`, `get_entity_history`,
  `assign_version_to_unlabeled_mutations`; all take `&self` and return
  `Result<_, SemanticaError>`. Snapshots and mutations are `serde_json::Map<String, Value>`
  (labels/tags/entity ids `&str`; tags `IndexMap<String, String>`).
  `InMemoryVersionStorage::new()` and `SQLiteVersionStorage::new(path)` (creates parent dirs and
  the original schema; `storage_path` is public). Both are thread-safe. Implement the trait for a
  custom backend.
- **Checksums** — `compute_checksum(&Map) -> String` and `verify_checksum(&Map) -> bool` are
  byte-identical to the original (SHA-256 over Python's
  `json.dumps(sort_keys=True, separators=(",", ":"))`), so checksums interoperate with Python.
  `version_storage::create_graph_snapshot_record(version_id, graph_uri, author, description,
  metadata)`.
- **Change metadata** — `ChangeLogEntry::new(timestamp, author, description, change_id,
  related_changes)` validates like `__post_init__` (ISO 8601 via a port of CPython 3.11's C
  `datetime.fromisoformat`, e-mail regex, 1..=500 characters); `ChangeLogEntry::create_now(..)`.
  `change_log::{MutationRecord, Severity, ChangeCategory, ImpactReport, ChangeLogAnalyzer,
  generate_change_report}`.
- **Managers** — `trait BaseVersionManager` (`storage()`, `create_snapshot`,
  `compare_versions`, provided `list_versions`, `get_version`, `verify_checksum`; import the
  trait to call the provided methods). `TemporalVersionManager::new(storage_path: Option<&str>,
  config: Option<Map>)` with public `storage: Arc<dyn VersionStorage>`, `logger`, `config`;
  `create_snapshot(&Value, label, author, description, &SnapshotOptions)`,
  `compare_versions(&Value, &Value, comparison_metrics)` (a `Value::String` is a stored label,
  an object a snapshot dict), `diff`, `prune_versions(Option<i64>, Option<&dyn
  TripletStoreQuery>)`, `attach_to_graph(&mut dyn MutationTrackedGraph)`, `record_mutation`,
  `tag_version`, `list_tags`, `get_node_history`, `restore_snapshot(&mut dyn
  MutationTrackedGraph, label, Option<bool>)`. Managers are `Send + Sync`.
  `managers::init_storage(storage_path, &logger)` is `BaseVersionManager.__init__`'s backend
  choice. `OntologyVersionManager::new(..)`, `create_snapshot(&Map, ..)`,
  `compare_versions(&str, &str)`, `versions()`.
- **Graph / store adapters** — `MutationTrackedGraph` is what the managers need from a
  `ContextGraph` (`set_mutation_callback`, `graph_id`, `suspend_mutation_callback`,
  `set_suspend_mutation_callback`, `from_dict`); the callback type is `MutationCallback =
  Arc<dyn Fn(operation, entity_id, &payload, version_label) -> Result<(), SemanticaError> + Send
  + Sync>`. `TripletStoreQuery::execute_query(&str)` is what `prune_versions` needs from a
  triplet store.
- **Ontology versions** — `VersionManager::new(config: Option<Map>, namespace_manager:
  Option<NamespaceManager>)`, `create_version(&mut self, version, &mut Map,
  &CreateVersionOptions)`, `generate_element_iri`, `compare_versions`, `diff_ontologies(&Map,
  &Map)`, `get_version`, `get_latest_version`, `list_versions`, `migrate_ontology`,
  `resolve_versioned_imports`; `OntologyVersion` record.
- **Namespaces** — `namespace_manager::NamespaceManager::new(config: Option<Map>)` (keys
  `base_uri`, `version`, `use_speaking_iris`; the whole map is kept as `config`, as the
  original passes configs through), `get_base_uri`, `generate_{class,property,individual}_iri`,
  `build_concept_scheme_uri`, `register_namespace`, `get_namespace`, `get_all_namespaces`,
  `get_skos_uri`, `get_alignment_predicates`, `validate_iri`. IRIs are joined with a port of
  Python's `urljoin` (verified on 855 base/url pairs), so they match the original exactly; the
  IRI methods return `Result` because `urljoin` raises `ValueError` for malformed bracketed
  hosts.

## Python behaviour reproduced (`src/pycompat.rs`)

- `json.dumps` text (ensure_ascii escapes incl. surrogate pairs, `float.__repr__`, separators,
  `sort_keys`) for checksums and the SQLite `snapshot_data` column; verified against the oracle
  on ~1,000 floats (random bit patterns) and escape-heavy strings.
- `datetime.fromisoformat` as CPython 3.11's C code parses it (separator detection, week dates,
  `HH[:MM[:SS[.ffffff]]]` quirks such as `10:30:00:5`, multi-byte separators, offset limits and
  the exact `ValueError` texts); verified on 4,026 targeted and fuzzed strings.
- `urllib.parse.urlparse`/`urljoin`/`quote` of CPython 3.11.14 (C0/whitespace stripping,
  scheme rules, `;params`, IPv6/IPvFuture bracket validation, NFKC netloc check).
- Container semantics over JSON values: `dict.get`, `list(x)` (dict keys, string characters),
  `set(x)` with Python equality (`1 == 1.0 == True`) and unhashable-type `TypeError`s,
  `AttributeError` for dict methods on non-dicts, `<` ordering with `TypeError` for mixed types.

## Deviations from the original

- **Set order.** Python builds several results from sets of strings, whose iteration order is
  hash-randomized per process: `compare_versions`' `*_added/removed/modified` lists and each
  `changes` dict's key order (temporal manager), `classes_added` etc. (ontology managers),
  `diff_ontologies`' `changes` key order and `resolve_versioned_imports`. The port uses
  first-seen order (first operand, then second). Tests compare these as multisets.
- **Ids/keys that are equal but of different types** (`1`, `1.0`, `True`): matching follows
  Python equality; the id reported for a modified node is the first version's (Python takes it
  from whichever key set is smaller).
- **Labels are strings.** A snapshot whose `label` is a truthy non-string scalar is stored and
  found under `str(label)` by both backends (SQLite's TEXT affinity already does this in
  Python; Python's in-memory dict keeps `5` and `"5"` apart). Lists/dicts as labels raise the
  original errors (`TypeError: unhashable type` in memory, a binding `ProcessingError` in
  SQLite).
- **In-memory storage** keeps each snapshot as its `json.dumps` text plus its `list_all` row and
  decodes on `get`; the original keeps `json.loads(json.dumps(snapshot))` and returns another
  round trip. Results are identical (a deep copy through JSON); errors computing the row are
  still raised by `list_all`. `get_entity_history` returns copies: in Python the in-memory
  history records alias the stored mutation dicts, so a list obtained earlier reflects later
  `assign_version_to_unlabeled_mutations` calls.
- **JSON decoding** uses `serde_json` (`JSONDecodeError` kind kept): its messages differ from
  Python's, `NaN`/`Infinity` literals are rejected, integers beyond `u64` decode as floats, and a
  BLOB column decodes as lossy UTF-8 text. Non-dict `snapshot_data` (only possible if the
  database is edited by hand) is a `TypeError`.
- **SQLite exceptions.** Errors Python lets escape (connect, schema creation) are
  `SemanticaError::Other { module: "sqlite3", type_name: "OperationalError" | "DatabaseError" |
  ... }` chosen by SQLite's primary result code exactly as CPython does, with SQLite's message
  (rusqlite's appended `": <path>"` is removed). Errors inside the original's `try` blocks are
  the same `ProcessingError("<context>: <sqlite message>")`. Binding errors use Python
  3.11.14's text (`Error binding parameter 2: type 'dict' is not supported`); integers above
  `i64::MAX` are the raw `OverflowError`. A failure creating the parent directories is the
  matching `OSError` subclass naming the database's parent directory; when an intermediate level
  fails, Python's `Path.mkdir(parents=True)` may name that level instead.
- **Typed parameters.** Dict parameters are `Map`s; `create_snapshot` (temporal) and the trait
  methods take `&Value` so the original's non-dict checks remain. Dataclass fields are typed
  (`ChangeLogEntry`, `MutationRecord`, `OntologyVersion`, `ImpactReport`);
  `ChangeLogEntry::new(.., related_changes: None)` gives `[]` (an explicit Python `None` would
  stay `None`). Unused `**options`/`comparison_metrics` keywords are accepted or dropped as
  documented on each method. `OntologyVersionManager.compare_versions` takes `&str` labels;
  through the trait, a non-string scalar is looked up by its `str()` and a list/dict is
  `TypeError: unhashable type`.
- **Aliasing.** Python returns live objects in a few places; the port returns copies:
  `OntologyVersionManager::versions()`, `VersionManager::create_version` (the record is also
  stored), `VersionManager::migrate_ontology` (mutates the given map and returns a copy).
- **Attached graphs.** `TemporalVersionManager._attached_graphs` (a private list keeping
  attached graphs alive) is not kept: the manager cannot own borrowed graphs and the list is
  never read. `getattr(graph, "graph_id", "unknown")` is `MutationTrackedGraph::graph_id()`
  (default `None` -> `unknown`). `prune_versions` logs any `Err` from the triplet store as the
  original logs any `Exception`; panics are not caught.
- **Sort errors.** `prune_versions` raises Python's `TypeError` when timestamps are
  incomparable; the operand types in the message follow timsort's first comparison for short
  lists and may be swapped for longer mixed lists.
- **NamespaceManager.** `base_uri` is a `String` (a non-string config value is converted with
  `str()`, `None` becomes `""`: Python's `urljoin(None, x)` behaves alike, but with a version set
  Python raises `AttributeError` on `None.rstrip`); `use_speaking_iris` keeps only the value's
  truthiness (every use tests it); `version` stays a JSON value. `get_skos_uri` returns
  `KeyError('skos')` if the prefix was removed from `namespaces`.
- **Enums.** `Severity`/`ChangeCategory` have `value()`, `name()` and `Display` (= `str()`,
  `Severity.CRITICAL`); `Debug` is derived, not Python's `<Severity.CRITICAL: 'critical'>`.
- Logging and progress tracking are ported (same logger names and messages; ontology managers
  enable the global progress tracker like the original).

## Tests

- **Ported originals** (68, all passing in the original baseline): `change_test_change_log.rs`,
  `change_test_version_storage.rs`, `change_test_integration_realworld.rs`,
  `change_test_performance.rs`; one `#[test]` per original function, named
  `<test_class_snake>__<test>`.
  - The wall-clock thresholds are the original's. Tests with timing assertions or many threads
    run one at a time within their binary (pytest runs serially; libtest would run them in
    parallel threads of one process). Measured in a debug build on the shared machine (load
    20-110): checksum of the 10k-entity graph ~0.2 s of 0.5 s (SHA-256 in an unoptimised
    `sha2` dominates), in-memory `get` of the 1k-entity snapshot ~18 ms of 50 ms (JSON decoding),
    snapshot of the 2k-entity graph ~0.1-0.18 s of 0.5 s, SQLite/in-memory `get` ratio ~1.0 of 5.
    Keeping the in-memory backend's deep copy through JSON text (as the original does) is what
    keeps the backend ratio test meaningful; a plain `clone()` made it ~5.0x, at the limit.
  - `test_memory_usage_scaling` reads the resident set size from `/proc/self/statm` or
    `ps -o rss=` (psutil's `memory_info().rss`).
  - `ThreadPoolExecutor` workers are `std::thread::scope` threads; `tempfile.NamedTemporaryFile`
    / `mkdtemp` are `tempfile` paths removed on drop (the originals' `finally`/`teardown`).
- **Characterisation tests** (`change_change_log.rs`, `change_version_storage.rs`,
  `change_managers.rs`, `change_ontology_version_manager.rs`, `change_namespace_manager.rs`,
  `change_init.rs`) compare against `tests/fixtures/oracle.json`, produced by running the
  original with `tests/fixtures/generate_oracle.py` (command in its docstring). They cover every
  public symbol, all error paths the original reaches, the SQLite schema and rows byte for byte,
  reading a database written by the original, and the managers driven with the same fake graph
  and fake triplet store the generator uses. Time-dependent fields are masked on both sides.
- **Unit tests** in `src/pycompat.rs` diff the private CPython ports against the oracle corpora
  (`fromisoformat`, `urljoin`, `urlparse`, `quote`, `json.dumps`).
