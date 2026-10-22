# sem-normalize port notes

Port of `semantica/normalize` (Semantica 0.6.5+oe.7). Module files mirror the original
(`config.rs`, `data_cleaner.rs`, `date_normalizer.rs`, `encoding_handler.rs`,
`entity_normalizer.rs`, `language_detector.rs`, `methods.rs`, `number_normalizer.rs`,
`registry.rs`, `text_cleaner.rs`, `text_normalizer.rs`; `__init__.py` -> `lib.rs`, which
re-exports `__all__` under the original names). `normalize_usage.md` became the documentation
module `normalize_usage.rs` (the guide's examples in Rust, run as doctests).

Python behaviour the module relies on lives in private modules: `pystr` (str methods, `float()`
/ `int()` parsing, Python's `\s`/`\w`), `codecs` (`bytes.decode` / `str.encode`), `dateutil`
(port of `dateutil.parser.parse` + `relativedelta`), `zoneinfo` (port of CPython's `zoneinfo`
and the C library's local zone), `chardet` (chardet 7.7.0's deterministic stages),
`htmltext` (BeautifulSoup `get_text`) and `kwargs`. `datetime` (public) models Python
`datetime` values with their `tzinfo`.

`normalize_provenance.rs` wraps a normalizer with `sem_provenance::ProvenanceManager`
tracking (see below).

## For crates built on sem-normalize

- Constructors take the merged configuration (`config` + `**kwargs`) as a
  `serde_json::Map`; every class implements `Default`. Config keys keep their names
  (`alias_map`, `titles`, `similarity_threshold`, `key_fields`, `missing_values`,
  `default_language`, `min_confidence`, `default_encoding`, `fallback_encodings`).
- Keyword defaults are `Option` parameters; keyword groups are option structs
  (`NormalizeTextOptions`, `CleanOptions`, both with `from_kwargs`), and `**options` that the
  original passes through stay `Map<String, Value>`. Text parameters that may be `None` take
  `impl Into<Option<&str>>` (pass `"text"` or `None`).
- Python unions are enums: `TextOrBytes` (`str | bytes`), `DateInput`, `NumberInput` /
  `NumberValue` (`int | float`), `methods::LanguageResult`, `methods::EncodingResult`.
  Dict results stay `Map<String, Value>`; dataclasses (`DuplicateGroup`, `ValidationResult`)
  are serde structs.
- Dates: `date_normalizer::DateTime` (`wall`, `fold`, `tzinfo: Option<TzInfo>`) with
  `isoformat`, `strftime`, `utcoffset`, `tzname`, `astimezone`. `TzInfo` covers
  `datetime.timezone` (`Utc`, `Fixed`), `dateutil.tz.tzoffset`, `zoneinfo.ZoneInfo` and
  `dateutil.tz.tzlocal`.
- Seams the original tests patch: `date_normalizer::patch_now` (thread-local `datetime.now()`
  guard), `date_normalizer::reset_tzpath` (`zoneinfo.reset_tzpath`), the engine traits of
  `TextNormalizer` (`TextCleaning`, `UnicodeNormalizing`, `WhitespaceNormalizing`,
  `SpecialCharacterProcessing`), the public `progress_tracker` fields, and
  `LanguageDetector::with_backend` (`LanguageBackend` trait; `None` = langdetect not installed).
- Globals: `normalize_config` (a `static`, lazily built) and `method_registry` (a handle to the
  process-wide registry). A registered method is `Arc<dyn Fn(&[Value], &Map<String, Value>) ->
  Result<Value, SemanticaError>>`.
- Errors are `sem_utils::SemanticaError` with the original classes and messages;
  `UnicodeDecodeError`, `UnicodeEncodeError` and `LookupError` are `SemanticaError::Other`
  (so `is_value_error()` is false for the Unicode errors).

## Deviations from the original

### Characters, strings and regular expressions
- Unicode properties, normalization forms and case mappings come from Rust's tables (newer than
  CPython 3.11's Unicode 14.0); results differ only for characters assigned after 14.0. The two
  places where Rust's tables do not express Python's rules are embedded from CPython 3.11: full
  titlecase mappings that differ from uppercase (`ß` -> `Ss`, ligatures, digraphs, Georgian) and
  `isdigit()`-but-not-`isdecimal()` characters. `str.title()`/`capitalize()` port CPython's
  algorithms (cased/case-ignorable rules, final sigma).
- Python's Unicode `\s` is matched as `[\s\x1c-\x1f]` and `\w` as `[\p{L}\p{N}_]` (Rust's `\w`
  also matches combining marks). The patterns are otherwise run by the `regex` crate; none of
  them depends on backtracking.

### HTML (`TextCleaner.remove_html(preserve_structure=True)`)
- `BeautifulSoup(text, "html.parser").get_text("\n", strip=True)` is reproduced on the html5ever
  tokenizer: text between markup, `<script>`/`<style>`/`<template>` contents excluded,
  `<textarea>`/`<title>` read as raw text, comments/doctypes/processing instructions dropped
  (all verified against the oracle). Differences: entities bs4 does not know keep their `;`
  (`&bogus;` -> `&bogus;`, bs4 `&bogus`) and HTML5 prefix matching applies (`&notit;` ->
  `¬it;`, bs4 `&notit`); `<![CDATA[...]]>` content is dropped (bs4 keeps it as a separate text
  node); `\r\n` inside text becomes `\n`.

### Entities
- `NameVariantHandler`'s default titles are a Python `set`, iterated in a per-process hash
  order; the port uses the literal's order. Only names with several leading titles are
  affected.
- `alias_map` keeps string values only. Python would also return non-string canonical values
  (and `normalize_entity` would then raise `TypeError` for truthy ones).
- Ill-typed numeric config values (`similarity_threshold`, `min_confidence`) fall back to the
  defaults; Python keeps them and fails at the first comparison.

### Data cleaning
- Without key fields, `calculate_similarity` uses the common fields in `record1`'s order (Python:
  set order, which only changes the floating-point summation order).
  `identify_missing_values` lists fields in first-seen order (Python: set order).
- A non-numeric `threshold` in `duplicate_criteria` raises Python's `TypeError` even when no
  comparison happens (fewer than two records).
- `impute_values(method="mean")` divides integer sums in `f64`: exact for sums below 2^53
  (Python divides arbitrary-precision integers exactly). NaN values sort as equal in the median.
- Equality (`==` on records, `in missing_values`) follows Python (`1 == 1.0 == True`).

### Numbers
- `int` results are `i128`; larger integers come back as floats (Python: exact big ints). A
  `bool` input is returned as the integer 0/1 (Python returns the bool).
- Non-finite floats inside dict results (`normalize_quantity`'s `value`, a currency `amount`)
  are JSON `null` (JSON has no NaN/infinity).
- `float()`/`int()` parsing, `round()` (half-even on the exact binary value) and `format(x,
  "e")` follow CPython (verified on the oracle cases).

### Dates and times
- `dateutil.parser.parse` is a line-by-line port for the default `parserinfo` (the only
  configuration Semantica uses); `tzinfos`, `dayfirst`, `yearfirst` and `fuzzy_with_tokens`
  are not exposed. Fractional numeric tokens use exact integer arithmetic (30 fractional
  digits) where `Decimal` rounds to 28 significant digits; integers beyond `i128` fail parsing
  at once (in Python they fail later, with the same outcome for every caller).
- Time zones are read from TZif files on `TZPATH` (`/usr/share/zoneinfo`, `/usr/lib/zoneinfo`,
  `/usr/share/lib/zoneinfo`, `/etc/zoneinfo`; `PYTHONTZPATH` honoured), with CPython's key
  validation, transition search, `fold` and POSIX-TZ rules. There is no fallback to the
  `tzdata` Python package except built-in `UTC`/`Etc/UTC`. `dst()` is not modelled.
- Naive datetimes converted with `astimezone` (only when an invalid source zone leaves one
  naive) and parsed zone names that match `time.tzname` (dateutil's `tzlocal()`) use the local
  zone of the C library (`TZ`, else `/etc/localtime`), so — as in Python — results depend on
  the machine's zone.
- `patch_now` fixes the module's clock and dateutil's default date and two-digit-year pivot on
  the current thread (the original tests patch only the module's `datetime`; the oracle run
  patches both).

### Encodings
- `bytes.decode`/`str.encode` are exact for UTF-8 (incl. `-sig`), ASCII, Latin-1, cp1252 and
  UTF-16/32 (all byte orders), with Python's names, aliases, error handlers and messages. Other
  legacy codecs use `encoding_rs` (WHATWG tables): bytes Python's code pages leave undefined
  decode to C1 controls instead of raising, KOI8-U is WHATWG's KOI8-RU, and Shift_JIS, EUC-KR,
  GBK and Big5 are WHATWG's supersets. UTF-7 and Python-only codecs raise `LookupError`;
  `surrogateescape`/`surrogatepass` decode like `replace` (Rust strings cannot hold
  surrogates); `namereplace` is unknown.
- `chardet.detect` (chardet 7.7.0, a model-based rewrite): BOMs, UTF-16/32 null-byte patterns,
  binary magic numbers, ASCII, UTF-8 (with its confidence curve) and binary content are ported
  exactly. Not ported: escape-sequence encodings (ISO-2022, HZ, UTF-7) and charset declarations
  in markup / PEP 263 cookies. chardet's statistical stage (trained models) is replaced by
  `chardetng`'s guess under chardet's names, reported with confidence 0.5 (0.1 when chardetng
  cannot separate it from other candidates). For the legacy-encoded inputs the tests use, the
  original's exact answers are recorded by SHA-256 (`chardet::RECORDED`, 3 inputs); other
  legacy inputs can get a different (decodable) encoding name and confidence.
- Files are read like Python's text mode (`errors="replace"`, universal newlines); an
  undetectable encoding reads as UTF-8, the oracle's locale encoding.

### Language detection
- langdetect is not installed in the reference environment, so the original always answers
  with the default language (and its two langdetect tests were skipped). The port detects with
  `whatlang` (`LANGDETECT_AVAILABLE = true`): codes follow langdetect's ISO 639-1 names
  (Mandarin is `zh-cn`; `zh-tw` is never reported; a few languages outside langdetect's 55 can
  be). whatlang gives the original tests' pinned answers (`en`/`fr`/`de`, English with
  confidence > 0.5) without any mapping. `detect_langs` returns one candidate with whatlang's
  confidence (langdetect: several sampled probabilities). `LanguageDetector::with_backend(...,
  None)` reproduces the oracle's no-langdetect behaviour exactly.

### Provenance wrapper (`NormalizerWithProvenance`)
- The original imports `semantica.normalize.normalizer.Normalizer`, which was never shipped, so
  `NormalizerWithProvenance(...)` always raises `ModuleNotFoundError: No module named
  'semantica.normalize.normalizer'`; `NormalizerWithProvenance::new` returns that error
  (`SemanticaError::Other`, module `builtins`). `with_normalizer(normalizer, provenance,
  agent_id, is_automated)` takes the normalizer (`Normalize` trait) instead of `**config` and,
  with `provenance`, creates `ProvenanceManager::new()` as the original does; `with_manager`
  takes a manager. Given the normalizer, `normalize(data, source, kwargs)` behaves like the
  original (verified against oracle runs with a stand-in module): it records one
  `normalized_data` entity `normalize_<8 hex of a uuid4>` with the agent, `software_agent`,
  `is_automated`, the activity's start/end `utcnow()` times and `{"method": kwargs["method"] or
  "default"}`, from `source` (default `"normalization"`).
- `__getattr__` delegation is the `normalizer()` accessor. `ProvenanceManager()` cannot fail with
  `ImportError` in Rust (the original's fallback to `provenance=False`); its other errors (e.g.
  an unusable default SQLite path) propagate from `with_normalizer`, as from Python's
  constructor.
- Port addition: `TextNormalizer` implements `Normalize` (its `normalize(source, **options)`),
  so the wrapper is usable with an existing normalizer.

### Registry, methods and configuration
- Custom methods receive JSON positional arguments (datetimes as `str(datetime)`, bytes as
  lists of ints). Their result is converted to the function's return type; a result of another
  type counts as a failed custom method (warning, then the default implementation), where
  Python would return it unchanged.
- As in Python, keyword arguments are merged into (and persist in) an existing method
  configuration of `normalize_config`; `handle_encoding`'s `TypeError`s for `source_encoding`
  with `"convert"` and keyword arguments with `"remove_bom"` are kept.
- Config files are parsed with `serde_yaml` / `serde_json` / `toml` (warning texts differ for
  malformed files). Environment values that look like integers beyond `i64` stay strings;
  Python's crash on non-ASCII digit values (`NORMALIZE_X=²`) is not reproduced.
- Progress items are tracked with `module = "normalize"` (sem-utils' call-site detection);
  Python detects module/submodule from its caller's caller. Only progress output differs.

## Tests
- `normalize_test_*.rs`: the 81 original test functions, one `#[test]` each
  (`<class_snake>__<test>`). Mock-based tests use the seams above: recording fakes of the
  engines and the cleaner, a fresh progress tracker with a recording display, `patch_now`.
  `test_init` cannot count constructor calls (it checks the built components);
  `test_normalize_failure_fallback` triggers the fallback with an invalid form instead of a
  patched `unicodedata.normalize`. `test_detect_language` and `test_detect_with_confidence` were
  skipped in the original baseline (`original_result: skipped`) and run in the port.
- Characterisation tests (`normalize_*.rs`, ~2,200 cases) compare with
  `tests/fixtures/oracle.json`, produced by `tests/fixtures/generate_oracle.py` in the oracle
  (command in its docstring; clock fixed at 2024-01-02 00:30, `PYTHONHASHSEED=0`). Time zones
  come from `tests/fixtures/zoneinfo` (TZif files copied from the reference system, read by
  both sides). The date fixtures assume the local zone's abbreviations do not include `EST`.
- `normalize_registry.rs` and `normalize_config.rs` mutate process-wide state and are separate
  test binaries.
- `normalize_normalize_provenance.rs` compares the wrapper with oracle runs of the original
  that install a stand-in `semantica.normalize.normalizer` module (and a fixed `utcnow`); the
  Rust side injects the same normalizer and uses `sem_provenance::clock::fixed_clock`.
