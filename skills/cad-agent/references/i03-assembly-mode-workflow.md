# I03 Assembly Mode working memory

> NX migration: the relationship, identity, interface, frame and DOF reasoning below remains active. Fusion commands and UI observations are historical implementation evidence only; use nx-execution.md and native-assembly-invariants.md to realize and verify the method in NX.

Use this Candidate reference for Assembly Design, Joint recovery, Motion Link,
or Edit In Place. It supplies engineering reasoning and release-local Fusion
observations; it does not authorize or block a practice action.

## Start from the relationship model

Imported STEP/IGES or externally referenced components may have shared source
coordinates but no joints, rigid groups, or constraints. Record the relationship
debt, choose a stationary reference, then build rigid identity, kinematic
relations, and geometric constraints as separate layers.

For each connection keep three views distinct:

1. **Structure**: occurrences, ownership, and the boundaries that connect them.
2. **Kinematics**: permitted translations/rotations, axis, neutral/current
   coordinate, limits, and collision behavior.
3. **Fusion realization**: command family, selected entities, key points,
   selection order, frame alignment, offsets, flips, setter order, and release.

Also keep native carrier identity distinct from functional identity. A Joint
can refer to a parent subassembly occurrence while the role-bearing physical
body is in a child occurrence. Accept that mapping only when the parent path has
one unique bound functional descendant, retain both raw native paths, and
recheck the containment at evaluation time. Do not infer the child from a
component name or endpoint order.

Represent a Joint as an n-ary relation:

```text
Joint
  -> moving occurrence -> moving mating interface -> boundary/key point -> local frame
  -> fixed occurrence  -> fixed mating interface  -> boundary/key point -> local frame
  -> construction method and alignment
  -> permitted/constrained DOF
  -> current coordinate, neutral coordinate, limits
  -> contact/interference policy and evidence
```

Do not collapse:

- `JointAlignmentAngle`, which aligns input frames and establishes zero;
- `MotionCoordinate`, which is the current configuration; and
- `MotionLimit`, which bounds that coordinate.

An articulated assembly can be relation-complete while retaining DOF. A
successful Joint command, shared center, or visually plausible pose does not
prove the intended relation.

## Select an interface, not a transient point

For each side derive:

```text
occurrence -> body -> boundary face -> optional boundary edge
           -> key point -> local frame (origin + complete basis)
```

Record the semantic role, analytic surface/curve kind, key-point construction,
selection order, and resulting frame. Numeric face/edge indices and transient
IDs are scoped to the exact document version/session.

A planar face and a cylindrical wall can yield the same center but different
frames. Substitution is valid only when origin, every basis direction, boundary
role, and downstream motion are equivalent.

Fusion's material selection step often contains two different decisions:

1. choose the mating boundary face;
2. choose or derive the snap/key point used to construct Joint geometry.

`createByPlanarFace(face, edge=None, CenterKeyPoint)` uses the face-area center;
supplying a circular edge uses the edge center. Coincident numeric origins do
not make these constructions semantically interchangeable.

## Engineering readiness checklist

Before writing the target relation, establish enough of the following to avoid
guessing the interface:

- unambiguous occurrence identities and assembly ownership;
- occurrence-owned mating boundaries with compatible analytic kinds/dimensions;
- intended fit/contact region and axis relation;
- derivable local frames and expected half-space/orientation;
- intended Joint type and DOF;
- known or isolated-probe evidence for alignment, offset, flip, limits, and
  release-local setter behavior;
- measurable expected post-state and invariants.

An unknown that only concerns release-local API behavior may be resolved with a
minimal isolated probe. Do not cycle combinations in the target assembly.
Unknown interface identity, frame handedness, or destructive target remains a
real blocker.

## Check after each meaningful relation

Read back and verify:

- exact occurrence pair, command/relation kind, parameters, boundary/key-point
  construction, and resulting frames;
- origin coincidence within tolerance and intended axis/orientation;
- expected pose from at least two informative views;
- permitted motion, current/neutral coordinate, asymmetric limits, and bounded
  sweep behavior;
- unexpected overlap versus declared interface occupancy;
- unchanged accepted relations and unrelated transforms.

If a relation fails, preserve only the evidence needed to diagnose it and
return to the earliest wrong boundary, key point, frame, or state. `Move` is a
diagnostic probe, not proof of a completed Joint. For a contaminated imported
component, a clean practice copy/re-import is often clearer than stacked
corrective transforms.

At an assembly checkpoint also verify connection-graph completeness, loose
components, required participants, save, close/reopen, transform stability, and
joint-state persistence.

For a whole-assembly functional decision, use one product-neutral closure:

```text
role count/separation
  -> signed direction and relative placement
  -> endpoint-grounded interface pair
  -> contact, engagement, axis, size and depth
  -> complete body-pair material exclusion with explicit overlap exceptions
  -> participant/interface-grounded Joint state and limits
```

Keep its data path flat. A single version-bound binding plan identifies native
occurrences, landmarks, interface boundaries and Joints; a read-only Fusion
observer emits raw state once; the trusted profile runtime recomputes metrics
and returns PASS/FAIL/UNKNOWN. Do not put thresholds, measurements or decisions
in the binding plan, and do not accept a snapshot that reports its own derived
verdict. Missing evidence is UNKNOWN; observed contradiction is FAIL.

Choose functional-landmark identity before evaluating pose. A rigid owner
transform must move the observed coordinates of the same native/entity-token
or owner-local construction point; it must not cause a different world-space
extreme to inherit the landmark name. World-axis-aligned bounding-box
centers/extrema are envelope evidence only and cannot be
`OccurrenceDirectionRequirement` or `RelativePlacementRequirement` ordered
endpoints. Landmark selectors contain identity, never an expected sign,
threshold, or verdict. Reuse the same frozen `binding_plan_digest` for the
original pose and every 180°/side-relabel check; changing the digest changes
the selector experiment and invalidates the comparison.

The common substitutions that caused S17 failures are invalid in every product
domain: zero interference is not engagement, shared axes are not size
compatibility, a visually connected pose is not interface ownership, and a
Joint does not prove its moving/fixed or boundary roles. An allowed overlap
exempts only its explicitly grounded pair. Axis-aligned native bounds are
conservative bounds evidence, not exact topological containment. Partial and
full engagement describe axial coverage rather than acceptance. A blind,
recessed or stop-limited connection may correctly remain partial, but it passes
only with a profile-owned insertion-depth interval and the other independent
interface criteria; arbitrarily shallow positive overlap is not enough.
Touching at a bound interface requires the native minimum distance between the
exact version-bound BRep face sets selected on its two endpoints and the
profile-owned `InterfaceBoundaryMinimumDistanceMetric` interval. An
occurrence-wide body minimum distance never proves that selected interface;
body-pair observations are retained for interference closure. The current
bounded-cylinder adapter requires exactly one interval for axis alignment,
axis separation, size difference, selected-boundary distance, engagement depth
and positive interference volume; missing, duplicate or unsupported metrics
fail closed before pose or fit acceptance.
Every functional role referenced by the profile must also have exactly one
same-profile role-cardinality requirement. Assigning an extra native
occurrence a new role or reusing a known role does not close the scope.
An allowed positive overlap must be localized inside its declared native
interface envelope in the current release; disabling localization is
fail-closed and never creates a blanket pair exemption.
Every executable profile has exactly one default
`PairwiseMaterialExclusionRequirement` over the complete solid-body-pair
closure; missing or duplicate defaults leave material coverage UNKNOWN.
An axisymmetric fit and a Joint axis leave rotation phase unobserved. When
phase matters, use a stable off-axis landmark direction or independently
grounded local frame; neither `q=0` nor a release-local flip flag is physical
phase evidence by itself.

## Recover parts from a fused assembly

Treat inverse decomposition as observation-bounded recovery of reusable part
definitions, physical occurrences, owner boundaries, interfaces and possible
separation behavior. Silhouette similarity and Boolean-union closure are useful
evidence, but neither one recovers material ownership, occurrence identity or
assembly semantics by itself.

Freeze one `InverseObservationModel` before proposing a hypothesis. It names
the fused source body and source document version, declared physical-part
count, source-ownership multiplicity status, observation channels and explicit
assumptions. An assumption limits only the observation model that cites it; it
is not a global definition of Part and is not evidence for the conclusion it
helps scope. For example:

- `BlindGeometryOnlyObservationAssumption` fixes the source baseline: fused
  geometry and source native measurements are inference inputs, while source
  appearance and native assembly metadata are excluded. It does not forbid
  separately declared candidate-version measurements or another explicitly
  scoped physical observation channel.
- `SingleConnectedSolidPartAssumption` requires one solid, one connected lump
  for each candidate reconstruction only in a model that declares that
  product-family rule. Without it, a valid part may have another explicitly
  represented realization.
- `ManufacturingRealizationRequiredAssumption` requires an accepted
  `CandidatePartRealizationAssessment` for every proposed `PartDefinition` in
  at least one declared `ManufacturingProcessFamily`. Assessments are unique
  per `(PartDefinition, ManufacturingProcessFamily)`; different process
  families may retain accepted, rejected or unknown comparisons. Geometric
  integrity, separability or a plausible building-block shape does not
  establish a process, material, tolerance, tool or mould.

An observation-channel declaration states its kind, use and exact entity or
version to which it applies. Every validation activity used by the inverse
hypothesis binds the same observation model and enumerates the channels it
actually consumed. An `ExcludedFromInference` or `DiagnosticOnly` channel
cannot silently support an inference-bearing result, and an input channel
scoped to a different body, reconstruction or document version cannot support
the measurement. Under a blind model, neutralize or exclude source appearance
so colour cannot act as a surrogate part label. After candidate
reconstructions exist, colour and transparency may be reapplied as diagnostic
views; they still are not partition evidence.

Keep three identity layers separate:

1. `PartDefinition` describes a reusable physical design and is the subject of
   manufacturing-realization assessment.
2. `ComponentOccurrence` is a hypothesis-local physical instance and carries
   position, ownership and connection participation.
3. `CandidatePartReconstruction` binds exactly one candidate occurrence to an
   explicit body in an authoritative candidate document version.

Repeated occurrences may intentionally share a `PartDefinition` and may reuse
the same reconstructed `Body`. Consequently, occurrence completeness is
closed over reconstruction records, not over a set of distinct body IRIs.
Never merge repeated instances merely because their geometry is congruent, and
never manufacture duplicate definitions merely to make an occurrence count
match. Write each reconstruction, motion domain and motion assessment's
child-to-hypothesis membership once; the inverse navigation properties are
semantic inverses and are not duplicate ABox obligations. Native integrity
checks name the exact body operand; solid/lump results do not replace
manufacturing-realization evidence.

Do not perform candidate reconstruction in the fused source version. The
`InverseObservationModel`, source material regions and native partition
measurements remain bound to the fused body and source document version.
Candidate bodies, reconstruction records, membership cells and motion
configurations belong to authoritative candidate evidence. An
`InverseReconstructionCheck` compares both set-difference directions and
enumerates the exact `CandidatePartReconstruction` records, so shared bodies do
not collapse repeated occurrences.

Scope every claim before closing it. `FusedMaterialSetScope` proves only
fused-union equivalence and does not require an invented owner-boundary graph,
individual owner labels or motion domain. `OwnerLabeledPartScope` additionally
closes occurrence reconstructions, owner boundaries, region assignments and
partwise material evidence. For that richer scope, represent a candidate as
coupled structures rather than as a cut list:

```text
reusable part definitions
  -> occurrence-scoped reconstruction records
  -> complementary interface pairs
  -> static occurrence/owner-boundary graph
  -> bounded material-ownership evidence
  -> optional mobility domains and separation plans
```

The owner-boundary graph is a general graph: cycles, several simultaneously
moving occurrences and several stationary occurrences are legitimate. Do not
import a product-specific hierarchy, anatomical orientation or preferred
decomposition order into the graph. Solve boundaries and interfaces together,
because each owner-boundary edge must be grounded by a complementary interface
pair owned by its two endpoint occurrences. A tangent, opening or extrema
plane may locate an interface event; it does not automatically define an
entire part boundary.

A `CandidateOwnerBoundary` is never an unlimited world plane. Ground it in the
two `InterfaceEnvelope` resources of its exact interface pair. Its bounded
support may extend to a natural root, shoulder, stop or closed cut frontier
only when native continuity evidence supports that extension. The associated
`PartitionOwnershipCheck` reports an explicit
`observedPartitionDeltaRegionCount`, including zero, and the evidence branch
depends on that measured count:

- For a positive count, inventory exactly every connected region introduced,
  removed or transferred by the partition; bind the affected regions to the
  bounded support, owner assignments and continuity witnesses.
- For a zero count, do not invent a delta region. Bind the boundary to a
  configuration- and version-bound `InterfaceEngagementObservation` for the
  same interface pair in `TouchingContactState`. Zero material delta without
  this exact contact evidence is not proof of a connection.

For every required source region, use one hypothesis-local
`RegionOwnerAssignment`. Several admissible owners are values of that one
resource, not several independently determined assignments. Outside the mating
core, preserve continuity with the existing exterior boundary, parent feature
or manufacturable stratum unless independent evidence supports reassignment.

Close owner-labelled material with a real matrix. If the hypothesis contains
`m` region assignments and `n` reconstruction records, its native
reconstruction check owns exactly `m × n`
`CandidateRegionMembershipObservation` cells—no omission, duplicate, foreign
cell or cross-hypothesis sharing. Each cell records region-minus-body and
region-intersection-body volume from the same native check:

- `RegionFullyContained`: difference is zero and intersection is positive;
- `RegionAbsent`: intersection is zero and difference is positive;
- `RegionPartiallyContained`: both are positive;
- `RegionMembershipUnknown`: a required native quantity is unavailable.

`OwnerLabeledPartScope` rejects partial and unknown cells. Candidate-owner
labels describe the admissible set; fully-contained cells describe the
owner(s) realized by the concrete reconstruction. Every admissible owner also
needs a same-region continuity witness.

Keep epistemic cases distinct. `EvidenceDeterminedOwner` has exactly one
candidate and one matching fully-contained cell. `AmbiguousOwner` has at least
two admissible candidates, while each concrete hypothesis still realizes its
declared owner assignment; another assignment requires an explicit,
set-equivalent alternative hypothesis in an `EquivalentSolutionFamily`.
`MultiplicityLostOwner` is different: fusion erased source owner multiplicity,
so a concrete reconstruction may legitimately contain the region in more than
one occurrence and its identifiability is
`OwnerMultiplicityUnrecoverable`. If bounded evidence establishes neither an
exhaustive unique domain nor an explicit alternative family, use
`IdentifiabilityUndetermined`. Failure to find an alternative never proves
uniqueness.

Connection existence and rigid mobility are orthogonal. Every owner boundary
has an evidence-bounded `ConnectionMobilityStatus` from its exact
`InterfaceMotionBasisCheck`:

- `AdmissibleRigidMotionObserved` requires a positive observed rigid-separation
  DOF count and a `CandidateMotionDomain` that reproduces the check's complete
  DOF set, axes and scoped assessments.
- `NoAdmissibleRigidMotionObserved` is a legitimate measured zero. It requires
  no artificial axis, DOF or motion domain, does not erase the connection and
  does not exclude process-mediated separation.
- `ConnectionMobilityUndetermined` preserves missing evidence as unknown;
  absence of a reported DOF is not a measured zero. An incomplete retained
  check may name its pair/model/version, but it cannot publish a derived axis,
  DOF or count while the status remains undetermined.

When a motion domain exists, bind its exact interface pair, purpose, moving
occurrence set and stationary occurrence set. The sets may each contain
multiple occurrences, but the two endpoints of the domain's own interface pair
must lie on opposite sides of that cut. Reproduce every native-derived DOF
rather than deleting an untested basis member, and assess ordered directions
separately because a stop may block one direction but not its opposite. Before declaring
`MotionBlocked`, localize the collision region and trace it to resolved owner
assignments. Interface occupancy, unresolved ownership or a partition-created
region yields `MotionUnknown`, not a physical rejection. Close
`interferenceFound` in both directions: clear requires false, blocked requires
true, and every positive collision region needs source-material provenance.

Keep the static owner-boundary graph distinct from the dynamic state of a
separation experiment. A `CandidateSeparationState` is an exact assembly
configuration plus the closed set of owner boundaries open in that state. Its
document version equals every hypothesis reconstruction's candidate version,
and its assembly occurrence inventory equals the hypothesis candidate-
occurrence inventory in both directions. State membership changes; the
underlying graph, occurrence identity and boundary identity do not.
At adjacent steps the output state of one step is the input state of the next.
A `BoundaryTransitionSeparationStep` declares the exact boundary whose
open/closed membership changes:

```text
disassembly: output-open = input-open ∪ transitioned-boundary
reassembly:  output-open = input-open − transitioned-boundary
```

No other boundary may change in that transition, and an already open boundary
cannot be repeatedly “opened” merely because it still lies between moving and
stationary scopes. This state-qualified rule works for general connection
graphs and multi-step extraction without a topology-specific frontier
heuristic.

Step kinds are orthogonal:

- `RigidMotionSeparationStep` applies a measured rigid transform over explicit
  moving and stationary occurrence sets. It may leave the open-boundary set
  unchanged, for example when a prior release step has already opened the
  connection.
- `BoundaryTransitionSeparationStep` changes one declared boundary state. If
  the same rigid motion performs that change, type the step as both kinds.
- `ProcessMediatedSeparationStep` is a boundary transition supported by named
  release, unfastening, heating, dissolving, deformation, cutting, joining or
  other physical-process evidence. It is not silently converted into a rigid
  transform and may be followed by a separate rigid withdrawal.

Thus a valid connection may have zero admissible rigid-separation DOFs, and a
valid rigid step need not itself open or close a connection. Precedence is
derived from explicit state transitions, accessibility and collision evidence,
not from the visual shape or a presumed assembly hierarchy.

Declare a `SeparationPlanReversibilityStatus` independently of plan existence.
`OneWaySeparation` and `SeparationReversibilityUndetermined` do not require a
fabricated inverse plan. A process-mediated or destructive disassembly is not
presumed reversible. Only `BidirectionallyReversibleSeparation` requires:

- an explicitly linked inverse reassembly plan;
- paired steps in reverse sequence order with input/output states exchanged
  and the same transitioned boundary and occurrence scopes;
- native step checks proving each rigid transform maps its declared input state
  to its output state and each linked rigid pair composes to identity;
- exact `transformCompositionOperandStep` inventories, in sequence-derived
  order, rather than an unbound Boolean identity claim; and
- a plan-closure check whose `transformCompositionScopeOccurrence` inventory
  covers every occurrence claimed by the cycle and whose composed result is
  identity.

Any inverse physical process also needs its own evidence; reversing a label or
negating a transform does not prove it. Reassembly must therefore be derived
only when bidirectional closure is claimed and verified, never imposed as an
unconditional sequel to disassembly and never replaced by resetting parts to a
remembered pose.

Keep interface dimensions/engagement, material ownership, manufacturing
realization, rigid mobility and ordered separation behavior as independent
candidate spaces. Passing one does not validate the others. The declared
inputs should deterministically produce the same candidate ordering, but
determinism does not imply identifiability. One admissible result, an explicit
equivalent family and an underdetermined domain remain different outcomes;
“optimal” additionally requires an explicit objective and tie-breaker.

If two owner labellings share the same fused union, retain an
`EquivalentSolutionFamily`; if fusion erased overlap multiplicity, report
`OwnerMultiplicityUnrecoverable`. In the current release,
`UniqueUnderDeclaredObservationModel` remains fail-closed: a bounded inline
ABox or self-authored digest cannot prove an exhaustive candidate domain. That
status is reserved for a trusted inverse runtime that independently rebuilds
and compares a canonical candidate domain.

## Contact and motion evidence

Keep five layers separate:

1. intended mating interface and fit assumptions;
2. Joint frame;
3. complete configuration state;
4. contact/clearance/interference observed for an occurrence pair at that state;
5. motion-envelope coverage and neutral restoration.

Axis coincidence does not prove fit; final-state clearance does not prove a
collision-free path; a UI limit does not prove a physical stop; discrete
samples do not prove continuous coverage. Report sampling interval, clearance/
overlap measurement, first-contact bracket when found, and whether a continuous
bound was actually established.

Position-frame Flip and Motion-direction Flip are independent. The first changes
placement/frame semantics; the second changes the signed interpretation of
motion and limits.

Joint types do not expose identical API fields. A missing native angle, offset,
Flip, motion coordinate, rest value or health property remains unavailable; it
must not be replaced by 0, false, the current coordinate, or an assumed healthy
state. A profile omission means “not constrained here,” while a profile-declared
metric with no native value yields UNKNOWN.

## Release-local Fusion observations

For the currently observed Fusion build:

- `JointGeometry.createByNonPlanarFace` requires `MiddleKeyPoint` for a
  cylindrical/conical face; `CenterKeyPoint` is rejected.
- `GeometricRelationships.add` requires a `ValueInput` in the Python wrapper.
- `AssemblyConstraint.errorOrWarningMessage` may raise
  `InternalValidationError: pDcJointAssembleFeature`; do not read it blindly.
- Treat `WarningFeatureHealthState` as review-required, run a bounded functional
  probe, and restore the original joint state.

Re-probe these behaviors when the release or selection construction changes.

## UI and ownership

Confirm Assembly Design from Fusion state, not from a screenshot alone. If the
required P2 UI path is unstable, preserve that evidence; an idempotent API
equivalent may continue the P1 engineering branch with
`p2_ui_credit=false`.

For Edit In Place, decide whether the external source is safe to modify before
entering. Put learner-owned context-derived geometry in a learner-owned
component. Do not claim an F3D with unresolved external references is a
self-contained F3Z package until upload/reopen proves portability.

Promote a repeated, reproduced rule only through the separate P3 evolution
process; ordinary use of this reference needs no evolution candidate.
