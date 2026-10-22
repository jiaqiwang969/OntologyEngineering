# Full engineering condition and read-only common fields

`oe-kernel::engineering::basis` returns an `EngineeringBasisView` of the same
Project condition. `protocol()` retains the original `/1` or `/2` identity,
`original_condition()` preserves the complete original UTF-8 string including
whitespace and escapes, `condition_sha256()` binds those bytes, and
`original_value()` exposes the full parsed envelope including goal support.

Existing algorithms borrow the generated `EngineeringBasis` common fields
through `Deref`. That inner DTO keeps its honest `/1` field profile. It is not
the complete `/2` wire envelope. The outer view implements Clone, preserving
the whole view, and implements neither Serialize, Deserialize nor DerefMut.
Direct serialization, serialization of a cloned view and mutation of the common
fields are compile-time errors. Explicitly copying the common DTO produces a
`/1` value; it must never be represented as the original complete `/2` input.

Dispatch uses `protocol()`. Parent/local source hashes continue to use the
original condition and original `/2` body; they are not recomputed from the
common DTO. Full support checking still uses the original source envelope.
Missing or broken support remains unknown or violated and cannot grant adoption,
while independent model, numeric, CAD and counterexample diagnostics continue.
Common-field validation never silently substitutes full `/2` qualification.

The first real reproduction used an independently frozen valid synthetic
parent/local support case. The original API accepted it, then serialized a DTO
labeled `/2` without the required goal_support; actual EngineeringBasisV2 decoding
failed. The original source, test binary and streams remain in the private
reproduction directory. This repair changes the Rust memory API, not the `/1`
or `/2` wire protocols, parent obligations or engineering adoption criteria.

Natural-language historical conditions remain `None`; unsupported protocols,
invalid objects, duplicate facts/assertions, missing declared sources and original
resource limits keep their diagnostics. A consumer of the common `/1` DTO does
not gain full `/2` support from this API fix; it must validate the complete envelope.
Actual ggen/source binding, regression, real cases, reception and installation
require their own receipts. Whole architecture remains under construction.
