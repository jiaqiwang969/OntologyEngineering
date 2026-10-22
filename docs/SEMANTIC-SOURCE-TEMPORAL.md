# Shared temporal values and private source queries

The existing Rust literal normalizer and private source graph evaluator use `semantica-temporal` as a shared leaf. The parser was extracted from the existing normalizer; query evaluation does not depend back on `semantica-runtime` and invokes no Python runtime. The leaf returns a parsed immutable value or explicit failure. Query/session and dataset/mutation responsibilities remain with their existing owners.

The original normalizer retains its 10000-byte admission, failed-cast lexical preservation and date/time/duration behavior, including the alternative ISO duration constructor. The query session retains separate input/output budgets. Calendar and ISO week forms, time fractions and UTC offsets follow the pinned CPython 3.13.11 `datetime.fromisoformat` domain used by RDFLib 7.6.0. This is not a timezone database, arbitrary Python `tzinfo`/fold protocol or complete datetime API.

The explicit extension is admitted by the existing sealed source graph factory; earlier/default factories keep their admission. It extends the bounded `semantic_source_default_cast` increment whose earlier dateTime-unreceived note remains historical evidence. Public query profiles and formal release acceptance are unchanged by the presence of this private code.

- Typed query literals preserve lexical spelling and `ill_typed=None`; direct-string initial values honor normalization and actual failed/successful casts.
- DateTime function casts produce canonical spelling and `ill_typed=None`. Bad casts return the original source error; unsupported provenance and resource refusals remain fatal.
- Existing truth, EBV, rich comparison, sorting and grouping consume actual temporal cache facts. Aware values compare instants; naive values compare local fields. Mixed ordered comparison reaches the original Literal lexical fallback. Source identity grouping remains distinct from value equality.
- The same source term, cache facts, source hash/repr and object identity pass through the existing graph owner. Imported RDF does not acquire source constructor facts merely from a matching lexical string.

Independent expected observations are frozen in `vendor/spareval/src/eval/fixtures/source_datetime_query_cases.json`: 55 syntax/function cases and 28 constructor/consumer cases, with complete original rows and function events. The three earlier dateTime cases retain their previous fixture bytes. Separate admission/provenance/budget controls and original runtime literal/snapshot regressions check integration. Actual pass/fail scope belongs to current validation receipts, not to this source description.

These are development replays. General engineering exploration continues through Project/work/Harness (`docs/ENGINEERING-EXPLORATION.md`); this increment introduces no new engineering decision or adoption owner. All original release, platform, engineering delivery and later-task method-benefit obligations remain.

The additional frozen group-base fixture has eight ordinary grouped-query probes. It receives a pre-existing translation gap exposed by the temporal test: current Group.p must be used after simplification, preserving its exact generated assignment chain. Original temporal and older fixtures remain unchanged.
