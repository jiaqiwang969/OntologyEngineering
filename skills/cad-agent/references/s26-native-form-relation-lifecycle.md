# Native Form relation lifecycle and layered validation

State: Candidate working memory first discovered in S26 attempt-0005 and
currently compiled from the stronger direct evidence set in attempt-0008.
Retrieve it by operation semantics and representation boundary, not by lesson
number, body name, command ID, or serializer row label alone.  The discovery
episode and the evidence basis of the current machine package are distinct
provenance layers; do not cite the former as if it were the latter.

Machine-checkable candidate recipe:
`data/knowledge/candidates/derivation-relation-lifecycle.v1.json`. It remains
non-authorizing and is not part of the active semantic bundle.

## Use this route

Use this guidance for Form/T-Spline work involving Crease, Mirror Duplicate,
Internal Symmetry, more than one symmetry generator, Finish Form conversion,
or a verifier disagreement between the control cage and the evaluated BRep.
Also route here for face-deletion hollowing followed by (negative) Thicken,
and for a deferred-boolean tool body versus retained positive geometry intent;
both are practiced in the attempt-0008 records below.

Model the observation layers explicitly:

```text
Form revision
  -> control-cage representation
  -> serialized direct relation generators
  -> derived relation/orbit closure
  -> evaluated BRep representation
```

A statement about one layer is not automatically a statement about another.
In particular, a T-Spline control-point extremum need not equal the extremum of
the smooth BRep produced by Finish Form.

## Derivation timing is profile-owned

Do not encode "finish the master, then mirror" as a universal feature order.
Bind exactly one contextual timing policy for the current source editable
scope:

- `POST_ACCEPTANCE_DERIVATION`: derive only from an explicitly accepted,
  saved, and exactly reopened source revision; no intervening source mutation
  is permitted.
- `AUTHORING_CONSTRAINT_FROM_SEED`: establish the native relation on a
  qualified seed topology before final shaping, then author under that
  constraint.
- `PRESERVE_EXISTING_DERIVATION`: retain and validate an already valid
  relation while editing its allowed generator or constrained degrees of
  freedom; do not recreate it as a shortcut.

Master, generator, source, and derived are roles in a relation occurrence, not
rigid subclasses of Feature. If no derivation participates in the current
alignment scope, require an explicit profile declaration of that fact rather
than treating a missing observation as "not applicable".

When this lifecycle is composed with reference alignment, keep two profile
coordinates separate: whether a derivation lifecycle belongs to the current
requirement scope, and which timing policy applies. Only four tuples are
admitted: no lifecycle plus `NONE`, or lifecycle-in-scope plus exactly one of
the three policies above. Current relation presence is a native stage
observation, not the meaning of the scope coordinate.

Do not pass a full lifecycle report into an alignment preview before that
preview exists. The alignment preview may generate a preview-only closure
report bound to the exact source, alignment candidate hash, unsaved-preview
observation context, native state digest, representation contract and release.
For `POST_ACCEPTANCE_DERIVATION`, relation absence must still hold through the
alignment exact reopen; only its qualified reopened revision can enter this
separate lifecycle recipe. The derived checkpoint then needs its own save,
exact reopen, relation closure, and a fresh alignment retest. Treat the
alignment profile as a required dependent-validation profile for that second
reopen, and return a content-addressed completion receipt bound to both reopened
revision identities and the exact lifecycle package hash. Documentation that
merely says “retest later” is not a closed composition path.

The seed topology is also profile-owned. Keep it unchanged unless current-
revision evidence demonstrates insufficiency and a bounded refinement plan is
hash-bound before mutation. "Low topology" is a useful modeling heuristic,
not a globally minimal vertex-count axiom.

## Direct relations and entailed closure

For more than one native symmetry generator:

1. classify each serialized relation block by its plane and exact forward map;
2. verify each block's forward and inverse maps independently;
3. treat those blocks as generators of an undirected relation graph;
4. compute the connected orbit for every controlled entity kind;
5. derive relations induced by generator composition and verify their geometry;
6. do not require one raw generator block to materialize relations entailed by
   another generator.

For the S26 evidence, the first raw block stores the front YZ relation and the
second stores the longitudinal front-to-rear relation. The rear YZ relation is
entailed by their composition. Requiring it inside the first raw block was a
verifier error, not a failed Mirror Duplicate.

Keep the release-local raw record as a compatibility canary when it is known,
but also parse its engineering semantics. A `105plane` row in the observed
release carries homogeneous `O`, `U`, and `V` vectors; validate finite values,
weights, an orthonormal basis, `U x V`, and the intended datum equation.

## Native command lifecycle

Use separate event cycles for the transitions that change Fusion's native
interaction state:

```text
settled preflight
  -> startEdit
  -> external settle observation
  -> native command start
  -> semantic surface selection
  -> semantic plane selection
  -> commit and commandTerminated
  -> control-cage and relation validation
  -> Finish Form
  -> external settle observation
  -> independent BRep validation
```

Bind every request to the exact Fusion release, document object, scratch
marker, recipe digest, driver session, and short expiry. Refresh native object
locators after entering Form; entity-token strings are locators, not stable
identity values, so resolve them and compare the returned Fusion entity.

For an unmaterialized Form edit session, `root.features.formFeatures.count == 0`
and `root.bRepBodies.count == 0` do not prove that the live T-Spline edit object
is empty. Before a primitive canary, cast `Design.activeEditObject` to the exact
expected `FormFeature`, verify its expected name, and read the live
`tSplineBodies.count`. Report all three observations. Treat an asynchronously
dispatched `startEdit()` as a transition claim until a later event cycle proves
the active edit object and workspace.

A native start/cancel canary admits only the behavior it observed. Discovery of
real command-input IDs and a clean `commandTerminated` edge does not admit that
driver for a geometry commit when a later source audit finds lifecycle races.
Freeze the canary as positive evidence, classify the driver separately as not
admitted, patch and regress it, obtain an independent source review, hash-bind
the audited source, and only then consume another native-start attempt.

Persistent drivers must preserve the authority of lifecycle events: an
exception returned after a synchronous `commandTerminated` callback must not
overwrite the already-terminal state. Shutdown must first close request ingress,
then reject pending start or unsettled native state, and only then remove event
handlers. A failed `fireCustomEvent` delivery must remain retryable; do not mark
its request fingerprint consumed before successful enqueue.

Do not place the independent four-shell BRep readback inside the callback that
calls `finishEdit()`. That callback must end at the native transition; the
settled solid is a later observation.

## Validation and recovery rules

- `commandTerminated=Completed` proves lifecycle completion, not geometry.
- Control-cage topology, direct generators, orbit closure, datum reflections,
  Crease propagation, and evaluated BRep validity are distinct gates.
- Preview and exact reopen must each close the direct generator, inverse map,
  complete orbit, editable representation, evaluated representation,
  edit-to-evaluated correspondence, and protected invariants. Exact reopen
  additionally verifies revision lineage.
- A consumer invariant never inherits a historical PASS across a new saved
  revision. If relation work depends on an already accepted aligned source, the
  lifecycle exact reopen must run the bound dependent-validation profile on the
  newly reopened revision and include that report in its completion receipt.
- Declare which representation owns every numeric invariant.
- Reject malformed or non-finite control points before numerical reductions;
  NaN can otherwise escape a maximum-error check.
- If the native command completed and the strong geometric postconditions pass
  but a verifier assumed that a raw relation block contains an entailed pair,
  retain the live Form, correct and independently audit the verifier, and then
  finish the Form. Do not replay the native command.
- If command outcome or geometry is still uncertain, keep the Form live and
  investigate; do not infer success from the UI or a screenshot.

## Evidence and admission boundary

Discovery lineage that first exposed the reusable distinction:

- `curriculum-learning/S26/attempt-0005/checks/s26-a0005-foot-native-longitudinal-mirror-candidate.json`
- `curriculum-learning/S26/attempt-0005/checks/s26-a0005-ear-native-box-start-cancel-canary.json`
- `curriculum-learning/S26/attempt-0005/assets/native-stages/S26_FEET_LONGITUDINAL_MIRROR_DUPLICATE.tsm`
- `curriculum-learning/S26/attempt-0005/practice-log.md`

Direct evidence bound by the current machine-checkable lifecycle package:

- `curriculum-learning/S26/attempt-0008/practice/s26-tail-video-sequence-audit-v1.md`
- `curriculum-learning/S26/attempt-0008/checks/09-exact-reopen-single-foot-v2.json`
- `curriculum-learning/S26/attempt-0008/checks/13-four-feet-relation-scratch-canary.json`
- `curriculum-learning/S26/attempt-0008/checks/15-four-feet-visual-acceptance.json`
- `curriculum-learning/S26/attempt-0008/checks/17-four-feet-exact-reopen-v3.json`
- `curriculum-learning/S26/attempt-0008/checks/47-unilateral-ear-open-thickened-independent-exact-reopen-v5.json`
- `curriculum-learning/S26/attempt-0008/checks/84-bilateral-ear-mirror-independent-exact-reopen-v6.json`
- `curriculum-learning/S26/attempt-0008/checks/117-snout-geometry-keyed-production-preview.json`
- `curriculum-learning/S26/attempt-0008/checks/131-exact-v7-post-reopen-window-census.json`

This is immediately retrievable Candidate guidance. Do not yet promote
`SerializedRelationGeneratorObservation`, `DerivedRelationClosureObservation`,
or `EvaluatedRepresentationObservation` into the core ontology from this one
episode. Reassess after the same distinction recurs in S27 or another
relationship-bearing feature, with competency questions and positive,
negative, ambiguous, and prior-release regressions.
