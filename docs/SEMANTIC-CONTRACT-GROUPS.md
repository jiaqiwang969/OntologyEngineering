# Shared design and generic semantic contracts

All modules remain organized by engineering purpose, necessary function, object/condition/source, constraints, evidence, acceptance and change impact. The generic semantic slice shares the same editable `specs/engineering.toml`, Rust normalizer, locked official ggen and source-bound build as engineering modules. No manually generated Rust or separate candidate rendering path is admitted.

`Type.contract_group` explicitly selects `engineering` (the backward compatible default) or `semantic`. No prefix of a type name determines ownership. Module groups follow the same declaration. Semantic types may contain primitives, `serde_json::Value` and semantic types. They cannot refer back to engineering Project/identity/result types. Engineering types may refer to semantic types; ggen imports the authoritative definition from `semantica_contracts` rather than copying it. Such a consumer must declare the actual Cargo dependency. Dependency cycles and semantic modules depending on engineering modules are rejected before generation.

The official template products are `contracts.rs` / `contract-tests.rs` for the engineering group and `semantic-contracts.rs` / `semantic-contract-tests.rs` for the generic group. Interface input/output/error contract groups must agree. Explicit interface source path and implementation map to the module owner and actual AST production trait implementation; comments and test scaffolding cannot satisfy this mapping. The generated source map binds these implementations, and generation refuses a missing port owner.

`SemanticQueryRequest` uses `semantica.query-request/1`, exact original UTF-8 RDF and SHA256, explicit `turtle|rdfxml|ntriples`, and a nonempty local read-only query. `raw_utf8` means required field presence; an empty RDF graph is valid and must not be rejected by a generic nonempty-string rule. The generated contract validates shape/status/digest syntax. The actual `RustSemanticRuntime::execute_query` verifies digest equality, format/parser/query behavior and budgets. Query completion/results and engineering conclusions/adoption remain different objects and responsibilities. Failures use `SemanticRuntimeFailure`, not engineering `ErrorCode`.

Kernel graph/SHACL/rules entry points delegate to the same runtime implementation, preserving their existing behavior tests. This moves the algorithm owner; it does not grant all SHACL, OWL, reasoning, connectors, memory, extraction, server or UI coverage. The complete locked Semantica oe.7 source/API/tests and Explorer remain in the replacement goal.

RRSI candidates must update the responsible specification/template/handwritten algorithm and its independent counterexamples, then run the same ggen/build/source binding. Baseline goals, mandatory conditions, exact sources, evaluator and rejection history remain frozen for comparison. A generation receipt is not an engineering acceptance. Selection, later use, observed feedback and precise restoration remain explicit. Arbitrary full-library code evolution or unseen engineering generalization is not established by this slice.

Declared `allowed_values` form a closed domain for every present `String` or
`Option<String>`, independently of whether the field format is `status`.
Both ggen contract templates generate the exact membership check. An absent
optional field stays absent. A non-string representation with a nonempty domain
is rejected before generation instead of losing the constraint. Existing status
error identities, SHA syntax and the required-String `raw_utf8` rule remain.
The semantic template also validates present optional SHA/status values.
Independent generated-contract receivers exercise both groups against frozen
valid/invalid inputs with dependency coordinates from the parent lock.
