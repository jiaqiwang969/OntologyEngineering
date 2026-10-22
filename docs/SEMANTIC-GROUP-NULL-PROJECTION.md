# Explicit top-level group-key null projection

`kernel query-options --input FILE` accepts the new explicit
`semantica.query-options-request/4` with profile
`rdflib760-group-null-projection/4`. Its flat request fields are the existing
complete data/source SHA, RDF format, query, namespaces, exact NTriples bindings,
`SemanticQueryShadowBudgetV3` and `SemanticQueryShadowGuardV3`. A generated
`SemanticQueryOptionsPortV4` delegates to the same `RustSemanticRuntime`, parser,
Dataset and evaluator. The result binds the complete canonical request SHA and
original data SHA. Existing neutral1, options2 and initial-shadow3 keep their
original behavior and identities. Unknown schemas and profiles never fall back.

The frozen RDFLib `result.bindings` observation for original15 case
`unbound_variable_group_alias` contains one real aggregate row with
`group: null` and integer `count: 3`. RDFLib7.6.0's
`translateAggregates` samples projected variables; its `Sample.get_value`
retains Python `None` when no value was sampled. The existing Rust RDF tuple
projection omits unbound keys. The new profile repairs this specific result-domain
difference using a proof from the genuine parsed AST, without query-string
patterns, fabricated RDF terms or extra result rows.

The frozen 15 observations describe RDFLib 7.6.0 `result.bindings`, not the
original Semantica `SemanticRuntime.query` / `select` DTO output. That wrapper
iterates the query result and omits None-valued cells when constructing
`QueryRowDTO` (original `runtime.py:1454–1460`). Profile4 exposes an explicit
underlying-result capability; these comparisons do not establish complete
wrapper, cache/revision or SELECT-helper compatibility. Unbound cells and
present null values therefore require separate expectations at each layer.

| Parsed scope | Explicit profile4 behavior |
| --- | --- |
| No Group anywhere in the query | Original behavior, including absent unbound SELECT columns |
| Exactly one top-level Group under one SELECT Project and unary output modifiers | Preserve bound RDF terms; materialize JSON null only for an absent, selected grouping key in each actual emitted row |
| Multiple keys, variable/expression aliases, exact language/IRI values | Same rule, retaining the actual current projection and terms |
| Ordinary joins/UNION/OPTIONAL/MINUS inside the input of the one top-level Group | Existing evaluator behavior; the output still passed through that Group |
| Post-group ORDER/DISTINCT/REDUCED/SLICE | Retain actual output rows and modifiers; LIMIT0 never gains a row |
| Post-group calculation or HAVING independent of grouping keys | Existing behavior, including aggregate aliases and count-based HAVING |
| Computation/HAVING depending on a potentially nullable grouping key | Explicit unsupported; this profile does not claim Python None expression semantics |
| Nested Project/group scope, multiple Groups, Group under an output JOIN/UNION/OPTIONAL/GRAPH or non-SELECT result | Explicit unsupported, even if a particular dataset happens to bind every key |
| Empty input with an explicit nonempty grouping key | Current zero rows retained; frozen raw `result.bindings` empty `[{}]` residual remains open |
| Duplicate/redeclared alias targets, SERVICE/FROM or existing aggregate target collision | Existing guard rejects |

Unsupported grouped scopes return the existing runtime failure envelope with
`code=query_execution_failed` and
`message=query_group_null_projection_scope_unsupported`. This is a declared
restriction of the new profile; an unsupported scope never silently inherits
old missing-key output and claims null-domain compatibility.

The original15 fixture remains byte-for-byte fixed at SHA256
`ee8670c12b2c257ee05310c54102150055cbf5ec08191166fb737c8ecf823596`.
For profile4 its 15 cases separate into 10 complete comparisons against those
frozen RDFLib `result.bindings` observations (including the null case), three
original-success cases still rejected by the existing Rust parser, one
original-success nested case explicitly refused by the new profile, and one
preserved empty-group difference. Those five are compatibility gaps; the three
parser refusals are not original Python rejection observations.
This is not 15/15 underlying-result parity or complete original Semantica API
parity. Profile3 also matches 10 of the frozen 15 observations, but the two
profiles share only nine matches: profile4 adds the null case and explicitly
refuses the nested case retained by profile3. The profile3 tests continue to
preserve their own older results, including the missing null key.

The shared implementation borrows complete request fields. Source16MiB,
query1MiB, parameter count128 per map, parameter bytes16MiB, quads100000,
rows10000, triples100000, AST depth96/nodes100000 and owned stack8MiB remain
unchanged; callers may tighten only the original limits. Both the AST scope
proof and the existing application-binding guard operate within these limits.
No new evaluation deadline, named-dataset query or federation behavior is added.

Complete Python nullable tuple-domain behavior across nested projections,
joins, OPTIONAL/UNION, filters, DISTINCT, expressions and aggregate-empty
semantics remains the next dependency. The current scope does not shrink the
whole Semantica migration goal, promote an engineering conclusion or establish
any software receipt without actual generation and execution.

The native dataset session now has a [parameter-aware query and cache path](SEMANTIC-SESSION-QUERY-OPTIONS.md), received against separately frozen original wrapper observations. It does not alter this profile4 underlying-result contract or close the remaining full-wrapper obligations.
