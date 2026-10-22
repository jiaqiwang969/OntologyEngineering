# Porting Semantica to Rust

`semantica-rs/` is the Rust port of Semantica 0.6.5+oe.7 (commit `8b2ed123c380fc8abc78507e6e6648e6f5c0052c`).
The port is complete only when every one of the 13,494 frozen scope items in `runtime/semantica-scope.json`
has a verified disposition in `ledger/`. `oev2 design semantica-scope` checks this; nobody can declare it.

## Layout

| Path | Contents |
|---|---|
| `crates/<owner>/` | One crate per owner (`sem-utils`, `sem-kg`, …, `semantica`). `src/` mirrors the original module, `tests/` holds ported tests. |
| `scope/<owner>.json` | The frozen scope items the owner must disposition (generated; never edit). |
| `ledger/<owner>.json` | The owner's disposition for each of those items (you write this). |
| `retained/` | Original files kept byte-identical (frontend sources, assets, licences). |
| `evidence/` | Recorded test runs. Written only by the release process; never edit. |

Original sources (read-only) are in `var/semantica-original/tree/jiaqiwang969-semantica-8b2ed12/`. A working
copy of the original, installed with all extras, is in `var/semantica-original/oracle/` (see "Oracle").

## Owners and order

Owners are ported in waves (see `scope/README.md`); a crate may depend only on crates of earlier waves, as
already declared in its `Cargo.toml`. Do not edit another owner's crate, ledger or scope file. If you need
something a lower crate lacks, implement it privately in your crate and mention it in your `PORT-NOTES.md`.

## Mapping rules

- Module `semantica/<m>/<file>.py` becomes `crates/<owner>/src/<file>.rs` (same stem; `__init__.py` becomes
  `lib.rs` or `<pkg>/mod.rs`). Keep the original module tree recognisable.
- A class becomes a `pub struct` (or `pub enum` for `Enum` classes, `pub trait` for abstract base classes and
  protocols). Public methods keep their snake_case names in an `impl` block; `__init__` becomes `new` (add an
  options struct with `Default` when the original has many optional arguments). Properties become getters.
- Functions keep their names as `pub fn`. Original keyword defaults become `Option<T>` parameters or an options
  struct; `**kwargs` becomes a typed options struct or a `serde_json::Map` when the original passes it through.
- Python dunder methods map to traits: `__str__` → `Display`, `__repr__` → `Debug`, `__eq__` → `PartialEq`,
  `__hash__` → `Hash`, `__iter__` → `IntoIterator`/`iter()`, `__len__` → `len()`, `__contains__` → `contains()`,
  `__getitem__` → `get()`/`Index`, `__enter__/__exit__` → a guard type with `Drop`, `to_dict`/`from_dict` → serde.
- Python dicts with free-form content stay `serde_json::Value`/`Map` (the workspace enables `preserve_order`, so
  insertion order matches Python). Dataclasses and typed records become structs with serde derives.
- Exceptions become error enums (`thiserror`), one variant per original exception class, with the same message
  text. Functions that can raise return `Result<T, Error>`. Keep the original failure conditions.
- Logging, progress bars and rich console output (`logging`, `structlog`, `tqdm`, `rich`) are not behaviour unless a
  test asserts their output; use the shims in `sem-utils` when present.
- Third-party libraries: `numpy`/`scipy`/`sklearn` math → plain Rust (`Vec<f64>`, explicit algorithms);
  `networkx` → `petgraph` or adjacency maps; `rdflib`/`pyshacl` → `oxigraph`/`oxrdf`/`oxttl` plus Rust code;
  `yaml`/`toml` → `serde_yaml`/`toml`; `requests`/SDK clients → `reqwest` (blocking) speaking the documented
  HTTP API; `sqlite3`/`sqlalchemy` → `rusqlite`; HTML/XML → `html5ever`/`quick-xml`; Office files → `zip` +
  `quick-xml` or `calamine`; PDF → `lopdf`; FastAPI/uvicorn → `hyper`.
- External services (databases, vector stores, graph stores, cloud storage, LLM and embedding APIs): define a
  client trait plus an HTTP implementation for the real service, so the port can really talk to it. Tests never
  touch the network; they use fakes, exactly where the original tests use mocks.
- Local ML models (transformers, sentence-transformers, spaCy, torch): use `candle`/`tokenizers` when a model can
  be loaded from local files, otherwise a configurable HTTP inference service. Tests use deterministic fakes, as
  the original tests do.
- Allowed dependencies are the `[workspace.dependencies]` in `Cargo.toml`. Add one to your crate as
  `name.workspace = true`. Do not add new crates or versions; ask the coordinator in your report instead.
- No `unsafe` unless unavoidable (then explain it in `PORT-NOTES.md`). Run `cargo fmt` on your crate.

## Tests

- Port every original test function assigned to you (`section: original_test_symbols`) as one Rust `#[test]`
  in `crates/<owner>/tests/<short>_<original file stem>.rs`, where `<short>` is the crate name without `sem-`
  (`kg_test_graph_builder.rs`). The prefix keeps test target names unique across the workspace. Name the Rust
  test after the original (`test_build_graph`; methods of test classes as `<class_snake>__<test>`).
- Keep the original inputs and assertions: values, ordering, error kinds. Parametrised originals may become one
  Rust test covering every parameter set.
- Every migrated public symbol needs at least one passing test that exercises it: a ported original test, or a
  new characterisation test whose expected values you obtained by running the original (see "Oracle").
- Tests must be deterministic, offline, fast, and must not read the home directory or real services.
- `evidence/original-baseline.json` records how each original test function behaved in the original environment
  (`passed` 4935, `failed` 206, `skipped` 86, `not_run` 41). If the original failed, was skipped or never ran, port
  what the original code actually does and record that `"original_result"` in the ledger (see below).

## Ledger

`ledger/<owner>.json`:

```json
{"schema":"semantica.migration-ledger/1","scope_sha256":"b1470d9980755811c815be24cdc041740de5f55e28232c327f0845d160455226",
 "module":"<owner>","entries":[ ... ]}
```

Every item of `scope/<owner>.json` appears exactly once. Entry fields: `id` (exact scope id), `disposition`, and:

| Disposition | For | Required fields |
|---|---|---|
| `migrated` | public symbols, exports, routes, entry points, placeholders | `rust`: references; `tests`: passing test ids that exercise it |
| `ported_test` | original test functions | `tests`: the passing Rust test id(s); optional `original_result` (`passed`, `failed`, `skipped`, `error`, `not_run`; default `passed`) which must match the recorded baseline |
| `migrated_file` | original source/test files | `rust`: the Rust file(s) carrying that file's content |
| `retained` | frontend sources/exports, assets, docs kept as-is | `retained`: `[{"path":"semantica-rs/retained/…","sha256":"…"}]`, byte-identical to the original |
| `superseded` | Python packaging/CI/tooling with no runtime role | `by`: port files that replace it; `note`: why |

References are paths from the skill root: a file (`semantica-rs/crates/sem-kg/src/graph_builder.rs`) or a
declared item in it (`…/graph_builder.rs::GraphBuilder`, `…/graph_builder.rs::GraphBuilder::build`). The owner
of a method must be declared or implemented (`impl GraphBuilder`) in that file.

Test ids are `<test target>::<path to the test fn>`: integration tests `kg_test_graph_builder::test_build_graph`,
unit tests `sem_kg::graph_builder::tests::test_x` (target = crate name with `_`).

Only `ported_test`, `migrated` and (for frontends/assets) `retained` are expected for code. `superseded` is for
items whose original role does not exist in Rust (for example `pyproject.toml`, `requirements-ci.txt`); explain it.

## Oracle

The original runs in `var/semantica-original/oracle/` (Python 3.11 with `semantica[all]`):

```sh
cd var/semantica-original/oracle/src
env -i HOME=../home PATH=../venv/bin:/usr/bin:/bin ../venv/bin/python -c 'from semantica.utils import x; print(x(...))'
```

Use it to obtain expected values for characterisation tests and to check edge cases. Never pass credentials or
reach real services from it.

## Working files

Several owners are ported at the same time. Keep helper scripts and scratch files in a directory of your own (for example
`<session scratchpad>/<owner>/`) with names that include your owner; never run or overwrite another owner's scripts or files.

## Checking your work

```sh
python3 var/semantica-port-tools/check_owner.py <owner>
```

builds and tests your crate (`cargo test --offline -p <crate>` with a private target directory), then checks your
ledger against the scope file and the test log with the same rules as the release gate. Report its summary.
Each crate also keeps `PORT-NOTES.md`: behaviour you could not reproduce exactly, and why.

## Shared foundations (from ported crates)

**sem-utils** (read its `PORT-NOTES.md`):
- Errors: use `sem_utils::SemanticaError` (one variant per original exception class plus Python built-ins such as
  `Value`, `Type`, `Key`, `Os`, `JsonDecode`, `Other{..}`). Build with e.g. `ValidationError::new(msg).detail(k, v).into()`
  and propagate with `?`. `Display` matches Python `str()`, `Debug` matches `repr()`; `From<io::Error>` exists.
- Do not glob-import `sem_utils`: `sem_utils::Result` is the original generic result container and shadows `std`'s.
- Logging: `get_logger("x")` returns the `semantica.x` logger; root level is WARNING as in Python. Tests can capture log
  output with `Handler::capture()` after setting a level.
- Progress: `get_progress_tracker()` returns `Arc<ProgressTracker>`; it prints like the original, `set_enabled(false)`
  silences it.
- Python-identical output helpers: `pycompat::{json::dumps, yaml::dump, fmt::{py_str, py_repr}, time::{PyDateTime,
  strptime}, warnings}` and `helpers::require_mapping`. Use them wherever the original's output format is asserted.

**sem-change** (read its `PORT-NOTES.md`):
- Storage: `VersionStorage` is a `Send + Sync` trait (all `&self`, `Result<_, SemanticaError>`); snapshots are
  `serde_json::Map<String, Value>`, tags `IndexMap<String, String>`. `SQLiteVersionStorage` uses the original schema and
  reads databases written by Python. `managers::init_storage` reproduces the original backend choice.
- Checksums: `compute_checksum`/`verify_checksum` reproduce the original exactly (sem-kg temporal queries use them).
- `ChangeLogEntry::new(ts, author, desc, change_id, related)` validates like the original; `change_log::generate_change_report(&Map)`.
- Managers: `BaseVersionManager` trait (`list_versions`, `get_version`, `verify_checksum`); `TemporalVersionManager::new(Option<&str>,
  Option<Map>)` with public `storage: Arc<dyn VersionStorage>`; `compare_versions(&Value, &Value, None)` (string = stored label,
  object = snapshot).
- sem-context's `ContextGraph` should implement `MutationTrackedGraph` (callback `MutationCallback`) for `attach_to_graph` /
  `restore_snapshot`; a triplet store should implement `TripletStoreQuery` for `prune_versions`.
- sem-ontology: `VersionManager::new(Option<Map>, Option<NamespaceManager>)`, `OntologyVersion`; re-export
  `sem_change::namespace_manager::NamespaceManager` (IRI methods return `Result`).

**sem-provenance** (read its `PORT-NOTES.md` and crate docs):
- `ProvenanceManager::new()?` (in-memory, or SQLite at the global default path), `with_storage_path(p)?`, `with_config(json)?`,
  `with_storage(backend)`, `with_shared_storage(arc)`; clones share one storage; `Send + Sync`.
- Recording: `track_entity(id, source, Option<Map>, TrackEntityOptions)` (builder: `.entity_type(..).agent_id(..)...`),
  `track_relationship`, `track_chunk(id, doc, TrackChunkOptions)`, `track_property_source(.., &SourceReference, TrackKwargs)`,
  `track_entities_batch`, `track_chunks_batch`. Like the original they never raise (log, return `None`/old version);
  `*_with_conn` variants propagate errors for batch/transaction use.
- Queries (`get_lineage`, `get_descendants`, `revision_history`, `invalidate`, `check`, `verify_chain`, `audit_log`,
  `export_prov`, `prov_graph`) return `Result` with `serde_json::Map` results in original key order.
- `ProvenanceStorage` is object-safe; transactions are closures (`Ok` commits, `Err` rolls back); fixed time via
  `clock::fixed_clock`; patch-style failure injection via wrapper storages, `InMemoryStorage::transaction_via`,
  `SQLiteStorage::set_configure_connection_hook`. `default_storage_path(Some(p))` returns a restoring guard.
- Reuse `sem_provenance::SourceReference` (conflicts) rather than redefining it.
