# Parameter-aware queries in the existing dataset session

`DatasetSession::query_with_options(query, namespaces, bindings, use_cache)`
returns the existing generated `SemanticSessionQueryResult`. It passes the
original text and typed RDF1.1 parameters to the existing local graph evaluator.
The existing `query` method delegates with empty maps and shares its cache.
`Catalog::execute_chapter_query` uses this same session after its existing
package and asset verification. Its returned result shape remains unchanged.

The cache distinguishes query bytes, complete namespace maps and exact native
RDF binding terms. Ordered maps make insertion order irrelevant. A distinct
option-key domain and escaped JSON encoding prevent concatenation collisions.
Successful dataset mutation clears all query entries; failed loads/queries
preserve the previous revision and cache. `use_cache=false` neither reads nor
populates entries. Results are owned copies. Session cloning and subsequent
mutation do not change the original session.

Existing session limits remain: query 1 MiB; 128 entries per parameter map;
parameter and result payloads 16 MiB; cache 256 entries and 16 MiB. Query inputs are checked
before allocating a key. JSON escaping may make a valid input too large to
cache; a bounded writer stops key construction and the query still executes
without caching. Shared derived-cache accounting and mutation invalidation use
the existing owner. A cache hit cannot bypass the chapter's source checks.

The chapter owner previously returned the graph evaluator's results without
the session's JSON result-size limit. Its internal session entry preserves that
existing domain and all original graph/query/row/parameter guards. A result too
large for the shared cache is returned without caching. The ordinary session
entry still rejects results above 16 MiB. An allocation-free bounded byte
counter stops as soon as the JSON cache allowance is exceeded. A frozen two-row
query repeating a 9 MiB literal exposed the initial integration regression;
the old success, first failure and corrected behavior are separately retained.

The fixture `tests/fixtures/session-query-options-original.json` preserves 34
ordered calls to the actual historical `SemanticRuntime` public API before this
change: 32 completed calls and two original errors. SELECT/ASK/CONSTRUCT terms,
cache hits and revisions are compared across parameter changes, disabled cache,
malformed queries, failed loads, duplicate successful loads and clear. The
original wrapper's missing cells remain omitted. Language DTOs explicitly map
to RDF1.1 `rdf:langString`; raw original DTOs and error messages remain retained.
Rust policy, capacity, ownership and source revalidation checks are separate.

This is a bounded receiving of the parameter-aware session surface. The
native result format is distinct from the original Python DTO. Plain literals
versus explicit `xsd:string`, full nullable/empty-row semantics, original helper
type checks, original DESCRIBE refusal, constructor profiles and unrestricted
original cache behavior remain separate obligations. Public query contracts
1–4, private feature defaults and all historical failures retain their meanings.
No query profile 5, full Semantica completion, engineering adoption or method
benefit is established by this development replay.


The private source evaluator now has a separately admitted result layer for the
original SELECT/ASK/CONSTRUCT forms. It consumes the actual prepared plan and
source term facts. SELECT distinguishes an absent binding dictionary from a
present None cell; ASK checks the first actual solution without SELECT row
filtering. CONSTRUCT retains source template constants, initial values,
plain/typed literal identity, language case and per-solution blank-node scope.
Original invalid triple roles and DESCRIBE remain refusals. Its CONSTRUCT WHERE
shorthand uses the actual source grammar group; the first failed comparison is
retained. The ordinary parser and old SELECT-only source entries keep their
existing contracts. The new private materialization is bounded to10,000 actual
rows,100,000 distinct triples and16MiB JSON with the same cancellation token.

The separately frozen31 result and8 initial-value observations exercise this
result layer. A subsequent, private source session bridge now receives all74
original helper/cache actions through the existing DatasetSession. Source
projection, core binding, revision, publication and cache accounting remain
with that owner. Helpers check type after query execution and cache insertion;
constructor cache disablement suppresses both reads and writes. Exact source
initial bindings and parser namespaces form the source-domain cache key. Native
and source results occupy separate domains in the same bounded cache.

Admission checks the actual source projection before returning a cached result.
Named datasets, GRAPH, SERVICE (including SILENT) and parsed FROM/FROM NAMED are
explicit refusals for this local default-graph bridge. The first control run
accepted FROM; the actual parsed dataset field is now checked without changing
the original74 oracle or older SELECT-only entries. The first ASK helper text
mismatch and the FROM failure remain in the receiving history.

The internal bridge retains the previously received source hash seed0 and
NumericLimits::default() (4096 character/digit/output/power limits), plus1MiB
query text,128 namespace or binding entries,100000 source triples and16MiB
input/result budgets. Successful original DTOs, revision and cache effects
are compared exactly. Original Python and native Rust parser diagnostics are
recorded separately; backend exception text parity is not claimed. Cancellation,
invalid source metadata and budget failures preserve the existing session.

This remains internal and default-off. It does not expose profile5 or a public
lossless wrapper. Complete original RDF loading formats, source-preserving
updates, named Dataset semantics, capability/profile selection, external reads,
public SDK/service behavior and platforms retain their separate obligations.
Full Semantica and release completion remain open.
