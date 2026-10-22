# Source Turtle text candidate

The internal `DatasetSession::serialize_source_turtle_internal` now writes a
complete document over the existing source graph and namespace state. It is a
Rust library candidate. Public source-session 1–3 still admit only their original
NT/NQ formats and witnesses; production selection comes from the maintenance
status, not the presence of this source file.

`source_turtle.rs` reuses the existing admission, graph selection, loss refusal
and namespace preparation. `source_turtle_writer.rs` implements subject and
predicate ordering, recursive blank nodes, RDF lists and document layout.
`source_turtle_literal.rs` presents retained source spelling, datatype, language,
numeric shorthand and multiline quotes. The source Memory view in spareval
supplies actual full-pattern and subject-pattern traversals, cached literal
facts and the existing CPython sort schedule. A Turtle-specific comparison seam
uses the already received relational value owner; SPARQL ORDER keeps its old
admission and failures. Derived Rust Node ordering never supplies Turtle order.

Named source loading, selected empty constructs, clear and graph selection retain
the source namespace-manager initialization points. Default Graph and Dataset
managers share bindings but keep distinct caches. Original input prefix bindings
are still not imported, and clear retains generated bindings. Text generation
does not change RDF content, revision or query-cache entries.

Exact compatibility includes surprising original behavior. The Python
`objectList` expression increments indentation even for one object. Empty base
can produce a QName whose declaration was generated after the document header.
A malformed RDF list can omit an unrelated property. These original outputs
are retained as source behavior, not accepted as valid RDF or engineering facts.
An arbitrary cyclic graph need not cause recursive list traversal; only a cycle
actually reached by that traversal triggers the explicit local cycle refusal.

The writer has finite work, nesting and output limits. Output follows the active
dataset byte budget; work is limited to 2,000,000 operations and nesting to 128.
These are project choices, not original RDFLib limits. It returns no successful
partial text. Reached namespace effects survive a refusal; RDF content and
revision do not change. Unknown converter caches remain explicit refusals.

Independent checks use 54 newly frozen original cases (115 actions), plus the
previous three wrapper sequences (13 actions). They compare full output,
mutation DTOs, errors, ordered namespace bindings and revision, covering 65
successful text results and two original loss refusals. The original two-node
cyclic graph is a finite positive case; a separate prefixed cyclic list tests the
local termination guard without running the known nonterminating original.
The earlier failed guard-test premise and all original outputs are preserved.

Further reception is required for the generated public result/state witness,
mixed query/update/NT/NQ namespace histories, remaining source datatype and
generalized-term combinations, actual host/installed consumers and declared
platforms. Mac and Windows library results are recorded separately in private
maintenance receipts. Neither library reception nor generated contracts grant
public-format, full Semantica, engineering adoption or RRSI benefit claims.

The original 63 engineering goals, 19 semantic modules, 19 protocols, 22 owners,
13,494 scope entries and original failures remain. Project/work/Harness and the
current CAD/manufacturing/metrology obligations retain their own evidence.
