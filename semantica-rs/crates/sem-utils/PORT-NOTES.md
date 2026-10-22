# sem-utils port notes

Port of `semantica/utils` (Semantica 0.6.5+oe.7). Module files mirror the original
(`constants.rs`, `exceptions.rs`, `helpers.rs`, `logging.rs`, `progress_tracker.rs`, `skos.rs`,
`types.rs`, `validators.rs`; `__init__.py` -> `lib.rs`, which re-exports `__all__` under the
original names). `pycompat/` holds the Python standard-library behaviour the port needs to keep
the original's observable output; it is new, shared infrastructure for the other crates.

## For crates built on sem-utils

- **Errors.** `SemanticaError` is the common error type: one variant per original class
  (`Semantica`, `Validation`, `TemporalValidation`, `Processing`, `Configuration`, `Quality`)
  plus the Python built-ins Semantica raises (`Value`, `Type`, `Key`, `Index`, `Attribute`,
  `Runtime`, `NotImplemented`, `Import`, `Overflow`, `Os(OsError)`, `JsonDecode`, and
  `Other { module, type_name, message }` for any other class). Build class errors with
  `ValidationError::new(msg)`, `.with_context(map)`, `.with_details(msg, ctx, details)`,
  `.detail(k, v)`, then `.into()` / `?`. `Display` = Python `str(e)`, `Debug` = `repr(e)`;
  `type_name()`, `error_code()`, `to_dict()`, `is_semantica_error()`, `is_validation_error()`,
  `is_value_error()`. `From<std::io::Error>` gives the matching `OSError` subclass. Payloads are
  boxed, so `Result<T, SemanticaError>` stays small (no `clippy::result_large_err`).
- **`sem_utils::Result` is the original generic `Result` container**, not `std::result::Result`;
  do not glob-import the crate root.
- **Logging.** `get_logger("module")` returns the `semantica.module` logger (`debug`, `info`,
  `warning`, `error`, `exception`, `critical`, `log`, `log_with_extra`). As in Python the root
  level is WARNING, so `info`/`debug` are silent until configured, and an unhandled WARNING+
  record goes to stderr. Tests can assert log output with `Handler::capture()` (`messages()`,
  `records()`) after `logger.set_level(...)`, like pytest's `caplog`.
- **Progress.** `get_progress_tracker()` returns the shared `Arc<ProgressTracker>`;
  `start_tracking(file, module, submodule, message, pipeline_id) -> String`,
  `update_tracking(id, status, message)`, `update_progress(id, processed, total, message)`,
  `stop_tracking(id, status, message, metadata)`, `track(...)`, `enabled()`/`set_enabled()`.
  Like the original, the default tracker prints progress lines to stdout; `set_displays(vec![])`
  or `set_enabled(false)` silences it.
- **Python output formats.** `pycompat::json::dumps` (= `json.dumps`, incl. `sort_keys`,
  `ensure_ascii`, `indent`), `pycompat::yaml::dump` (= PyYAML `yaml.dump`),
  `pycompat::fmt::{py_str, py_repr, float_repr, format_fixed, percent_format}`,
  `pycompat::time::{PyDateTime, strptime, strftime}`, `pycompat::warnings::{warn,
  catch_warnings, WarningCategory}` (with `TemporalAmbiguityWarning`).
- `helpers::require_mapping` is the exporters' `_require_mapping`; `helpers::register_module`
  makes a module name importable through `safe_import`.

## Deviations from the original

Logging and progress output that no test asserts was still ported line for line; the
differences below are the ones Rust cannot (or deliberately does not) reproduce.

### Exceptions
- No Python frames exist, so every traceback text (`handle_exception`, `format_exception`,
  `log_error`) is the frameless form `module.Class: message\n` — what Python prints for an
  exception that was never raised. Exception chaining (`raise ... from e`) is not modelled.
- `handle_exception` returns the merged context but does not mutate the caller's dict.
- An `error_code` keyword detail on a subclass (a `TypeError` in Python) is ignored.
- `format_exception` options are a typed struct (`FormatExceptionOptions`); `max_length` keeps
  Python's slicing (0 = no limit, negative = cut from the end).

### Logging (`logging.rs`)
- Python's `logging` is replaced by a port of the parts Semantica uses: dotted logger
  hierarchy with propagation, levels, `%`-style `Formatter` (with Python's validation and
  conversions), stdout/stderr/writer `StreamHandler`, `FileHandler`, `RotatingFileHandler`
  (same rollover rule) and an in-memory capture handler. Not ported: filters,
  `logging.config`, timed rotation, `exc_info`/`stack_info` (so `logger.exception` logs an
  ERROR without a traceback), `funcName` (always `(unknown function)`), Python thread ids and
  `logging.disable`.
- `get_logger(name, level=None, **options)` is split into `get_logger(name)` and
  `get_logger_with(name, level, GetLoggerOptions)`.
- `log_data_quality` takes a typed `DataQualityOptions` (`threshold` is an `f64`, so an integer
  threshold prints as `1.0` in the alert message).
- `log_execution_time` is a call-through wrapper `log_execution_time(name, || -> Result)`.

### Helpers
- `format_data`/`hash_data`/`write_json_file` produce Python's exact `json.dumps` text and
  `format_data(.., "yaml")` PyYAML's exact `yaml.dump` text for JSON-like data (verified on 96
  PyYAML and 49 `json.dumps` oracle cases). PyYAML anchors (`&id001`) for shared Python
  objects cannot occur, and tuples/sets/dates/bytes have no `serde_json::Value` form.
- `read_json_file` parses with `serde_json`: same accept/reject decisions for standard JSON,
  but `JSONDecodeError` messages use serde's wording, `NaN`/`Infinity` literals are rejected,
  and integers beyond `u64` become floats. A non-UTF-8 file always reports
  `invalid start byte` as the `UnicodeDecodeError` reason.
- `format_timestamp`/`parse_timestamp` use `pycompat::time`: `strftime` follows CPython on the
  macOS C library in the C locale (verified against the oracle, incl. `%c`, `%e`, `%-d`,
  unknown conversions); `strptime` is a port of `_strptime` for the C locale. `%Z` only
  matches `UTC`/`GMT` (Python also accepts the local zone names), and sub-second UTC offsets
  are dropped. The timestamp argument is typed (datetime or epoch number), so the
  `Invalid timestamp type` error cannot occur.
- `safe_import`: Rust has no dynamic imports. Importable modules are the Python 3.11 standard
  library modules importable on the reference platform, the `semantica.utils` modules, and any
  registered with `register_module`. A relative name without `package` (a `TypeError` in
  Python) reports `(default, false)`. `default` is a `Module` handle.
- `retry_on_error(...)` returns a `RetryOnError` policy; `call` retries any error, `call_if`
  takes a predicate in place of the `exceptions` tuple. A negative/NaN delay skips the sleep
  (Python's `time.sleep` would raise).
- `get_nested_value`/`set_nested_value` return `Result` (empty separator is Python's
  `ValueError`; walking through non-dicts gives Python's exact `TypeError`s).
  `ensure_directory` returns the path as given (Python returns the normalized `Path`).
- `normalize_graph_payload` takes a `serde_json::Value`: records must be JSON objects (typed
  records are serialized first, matching Python's dataclass/attribute conversion); Python's
  module/class/bytes/tuple distinctions do not exist. Errors and messages are identical
  (verified on 47 oracle payloads).

### Validators
- Constraint dicts are typed: `Constraints`, `ConfigOptions`, `PyType` (single types only; a
  tuple of types has no counterpart). Validators that Python can make raise for inputs of the
  documented types (`validate_data`, `validate_schema`, `validate_*_constraints`) return
  `Result<(bool, Option<String>), SemanticaError>` with the same exception kind.
- `pattern` uses Rust `regex` syntax (Python-only constructs such as look-around fail to
  compile and report `re.error`).
- `validate_config` lists invalid keys in configuration order (Python's set order varies
  between runs).

### Types
- `Entity`/`Relationship` fields are typed; `dict_to_entity`/`dict_to_relationship` raise
  `TypeError` for values the record cannot hold (Python stores anything), and a whole-number
  confidence round-trips as a float. `Entity.type` holds the type name (enum members convert
  with `.into()`).
- Dataclass reprs are derived `Debug`; `Result`'s explicit `__repr__` is reproduced (requires
  `T: Display`). `__bool__` is `Result::as_bool`.
- The `Processor`/`Validator`/`Exporter`/`Importer` protocols are traits with required methods
  (their `...` bodies have no behaviour of their own).

### SKOS
- A list/dict value under a predicate key is treated as "not a hierarchy edge" (Python raises
  `TypeError: unhashable type`).
- Concepts are visited in insertion order; Python visits a set, so which concept a cycle error
  names can differ between Python runs. The search is iterative (no `RecursionError` on very
  deep chains).

### Progress tracking
- There is no IPython kernel: `IPYTHON_AVAILABLE` is false, `JupyterProgressDisplay` only keeps
  its items, and `build_html` / `summary_html` return the HTML the original would display.
  Stdout is assumed UTF-8 (the emoji encoding probe and `_safe_write` fallback are moot).
- `ModuleDetector`: Rust has no frame introspection. `detect_from_frame` works on a `FrameInfo`
  (the module `__name__`, `self`/`cls` class and qualname the original reads);
  `detect_from_call_stack` uses `#[track_caller]`: depth 0 is the function itself, any other
  depth resolves to the nearest call site outside `#[track_caller]` functions, whose source path
  gives the module (`crates/sem-ingest/src/x.rs` -> `ingest`); the submodule is never detected.
- `track_progress(...)` returns a `TrackProgress` decorator whose `run(name, || -> Result)` tracks
  one call; Rust closures have no inspectable arguments, so the file comes from `file`.
  `track(...)` takes the body as a closure (completed on `Ok`, failed with the error text on
  `Err`).
- Displays receive snapshots of items (Python passes live objects that later change). Inside
  the tracker items are shared, so the active and pipeline views stay aliased exactly as in
  Python (including the duplicated active item in `get_pipeline_items`).
- The class/module singletons (`ProgressTracker._instance`, `_global_tracker`) are replaced
  through `ProgressTracker::set_instance` and `set_global_progress_tracker`.

## Tests
- Ported originals: `utils_test_utils.rs`, `utils_test_skos.rs`,
  `utils_test_progress_tracker_regressions.rs` (one `#[test]` per original function).
  - `test_ensure_directory` patches `Path.mkdir` in Python; the port checks the directory is
    created in a temporary directory.
  - The deadlock tests wait 10 s instead of 1 s before declaring a deadlock (the machine is
    shared and heavily loaded).
  - `test_disable_progress_env_prevents_reenable` sets `SEMANTICA_DISABLE_PROGRESS` in a child
    process (mutating the environment is `unsafe` in Rust 2024): the test binary re-runs itself
    filtered to a `subprocess_*` helper, which returns at once outside such a child.
  - `test_build_knowledge_base_subprocess_does_not_deadlock` **failed in the original baseline**
    (`subprocess.TimeoutExpired` after 15 s). Re-running it in the oracle shows no deadlock: the
    child completes in ~16.4 s, ~13 s of it in Semantica's lifecycle start-up, so the 15 s budget
    expired on the loaded machine. `build_knowledge_base` belongs to sem-core, so the port runs,
    in a child process with the same 15 s timeout, the progress-tracker call sequence it makes
    (lifecycle tracking, pipeline registration, pipeline-scoped console updates, summary) and
    checks it prints `1`. The ledger records `original_result: failed`.
- `tests/utils/test_normalize_graph_payload.py` moved to a later owner (it drives exporters);
  `utils_normalize_graph_payload.rs` characterises `normalize_graph_payload` on its
  JSON-expressible scenarios.
- Characterisation tests (`utils_*.rs`) compare against `tests/fixtures/oracle.json`, produced by
  running the original in the oracle with `tests/fixtures/generate_oracle.py` (command in its
  docstring). Time-dependent fields (elapsed time, rate, ETA) are masked on both sides.
