# Native assembly invariants

These engineering obligations carry forward to NX. References below to a historical
profiled runtime describe its evidence boundary, not a claim that that runtime or
Fusion Joint API is available in NX. Collect equivalent native observations; missing
NX metrics stay UNKNOWN. Formal semantic evaluation belongs to parent Semantica.


For assembly work, load
`references/i03-assembly-mode-workflow.md` plus the current lesson reference.
When placements are derived from dimensionless instruction panels or a
finished-product photo, also load `references/exploded-instruction-reading.md`
and keep its evidence ladder and perturbation order.
Represent a Joint as an n-ary engineering relation:

```text
Joint
  -> moving occurrence -> owned mating interface -> boundary/key point -> local frame
  -> fixed occurrence  -> owned mating interface -> boundary/key point -> local frame
  -> construction method and alignment
  -> permitted DOF, current coordinate, limits
  -> interference/contact policy and evidence
```

Keep `JointAlignmentAngle`, current `MotionCoordinate`, and `MotionLimit`
independent. Relation completeness does not imply zero remaining DOF. A shared
center, visually plausible pose, successful Joint call, or mirrored source name
does not prove the intended interface/frame/role. Verify occurrence identity,
frame origin and axes, motion behavior, limits, interference, save, and reopen
in proportion to the lesson.

For inverse assembly work, distinguish fused-set closure from owner-labelled
part recovery. Each candidate cut must be carried by an interface pair, a
bounded interface-plus-root support region, a complete connected material-delta
inventory, explicit region-owner assignments and non-interface continuity
witnesses. Ground the support in both interface envelopes; give each connected
delta exactly one assignment resource, keep alternatives inside that resource,
and require every candidate occurrence to participate in an assignment and a
boundary. Never extend an interface tangent/opening plane across unrelated
material. Keep interface dimensions, material ownership and DOF candidates
independent. A native-derived motion domain must retain every symmetry-allowed
DOF; an unmeasured member remains `MotionUnknown`. Before positive interference
rejects a DOF, close the sweep observations, localize the actual collision
material and resolve its owner provenance; a partition artifact or unresolved
region sends the solver back to the owner hypothesis. Assembly/disassembly
motion and in-service motion are separate. Set-equivalent owner alternatives
remain a solution family, and source overlap multiplicity erased by fusion
remains unrecoverable. The bounded RDF sidecar never certifies inverse
uniqueness by itself.

Keep the observation and reconstruction sides physically and semantically
separate. Source body, observed regions and partition checks remain bound to the
fused source document/version. Every candidate occurrence has exactly one
hypothesis-local `CandidatePartReconstruction` with an explicit body in a
different candidate document/version; the reconstruction check and motion/
collision configuration stay on that candidate version. Never reuse the source
body or source document as the candidate merely to make a Boolean check close.

For each hypothesis, require the complete Cartesian matrix
`RegionOwnerAssignment × CandidatePartReconstruction`, with exactly one
check-owned `CandidateRegionMembershipObservation` per cell. Native Boolean
values distinguish `RegionFullyContained`, `RegionAbsent`,
`RegionPartiallyContained`, and `RegionMembershipUnknown`; partial or unknown
cells fail closed for `OwnerLabeledPartScope`. `EvidenceDeterminedOwner` has one
admissible owner and one matching fully-contained cell. `AmbiguousOwner` has
multiple admissible owners but exactly one concrete fully-contained owner per
hypothesis; a different owner belongs in an explicit set-equivalent alternative
hypothesis. `MultiplicityLostOwner` may have one or more fully-contained owners
but must report `OwnerMultiplicityUnrecoverable`. When evidence proves neither
exhaustive uniqueness nor a concrete alternative family, use
`IdentifiabilityUndetermined`.

A native Joint may name a parent subassembly occurrence while the physical body,
landmark and interface belong to a descendant occurrence. Resolve that carrier
only when its native path contains exactly one bound functional descendant,
preserve both raw carrier paths in the snapshot, and let the runtime recheck the
hierarchy. Zero or multiple descendants is UNKNOWN. Capability absence is also
evidence absence: if a Joint kind does not expose alignment, offset, flip,
motion, or health, never synthesize 0, false, current-as-neutral, or healthy.
The profile may constrain only fields it declares and the adapter can actually
read.

For a functional assembly check, use the domain-neutral invariant pattern
rather than product-name branches:

```text
exact configuration + closed occurrence/body-pair scope
  -> contextual roles + profile-controlled role separation/counts
  -> stable landmarks + same-configuration/frame position observations
  -> signed direction + relative placement
  -> endpoint-grounded interface pairs
  -> contact + engagement + axis/size/depth evidence
  -> material exclusion + explicit bounded-overlap exceptions
  -> participant/interface-grounded joints + current state/limits
  -> complete coverage -> PASS / FAIL / UNKNOWN
```

Every configured occurrence must be included or explicitly excluded with a
reason and accounted by a profile invariant. Use a role-cardinality requirement
for ordinary parts whose only relevant invariant is presence/count; do not
invent a direction measurement for them. Close the interface inventory in the
same scope and put interface roles on `MatingInterface` so a roleless interface
or missing envelope remains detectable. Count the profile-selected occurrences
and interfaces before checking their evidence. Anchor each functional frame role to the configured
assembly or a scoped owner and a profile-owned expected basis; resolve canonical
direction definitions through that basis instead of trusting a direction
label. A native check, both landmark-position observations, the measured
vector, and the reference vector must refer to the same exact
configuration/frame. Treat the profile's required fit as the specification and
the envelope/observation as evidence that must agree with it. Use
product-specific profiles only to supply roles, frame bases, directions,
counts, fit/contact/kinematic targets and thresholds; keep the reusable TBox,
SHACL and CQ free of anatomy or part-name branches. Do not let zero
interference stand in for engagement, coaxiality stand in for size, a Joint
stand in for its intended interfaces, or an allowed overlap exempt unrelated
body pairs. Treat `PartiallyEngaged` and `FullyEngaged` as observed geometric
coverage states, not verdicts. Blind, recessed, and stop-limited connections
may legitimately be partial, but only a profile-owned insertion-depth interval
together with the other independent interface criteria can accept them; any
positive overlap by itself is too weak.

Landmark identity must be chosen before pose evaluation and remain the same
under a rigid transform of its owner. Use a version-bound native point/entity,
Joint/construction origin, or independently grounded owner-local point.
World-axis-aligned bounding-box centers/extrema are pose-dependent envelope
statistics: they may support bounds checks but never the ordered endpoints of
an `OccurrenceDirectionRequirement` or `RelativePlacementRequirement`. A
selector must contain identity only, never the expected sign, threshold, or
verdict. Freeze and reuse the same `binding_plan_digest` across a pose repair
and its 180°/side-relabel metamorphic check; a changed digest is a different
selector experiment, not evidence that the original binding survived motion.
Otherwise a reversal can relabel the opposite world extreme and circularly
preserve PASS.

Require exactly one same-profile role-cardinality requirement for every
functional role referenced anywhere in that profile. An extra native
occurrence is not covered merely because the binding plan gives it a new role
or reuses a known role; absent exact cardinality coverage makes the closed
scope `UNKNOWN`, while an observed count excess is `FAIL`.

An allowed material overlap must be localized inside its declared native
interface envelope. The current release does not support disabling
localization; a false containment flag is fail-closed, not a blanket
occurrence-pair exemption.

Every executable assembly profile must also own exactly one default
`PairwiseMaterialExclusionRequirement` over the complete unordered solid-body
pair closure. Missing or duplicate defaults make the result `UNKNOWN`; an
allowed-overlap rule is only a localized exception to that one default.

Do not infer rotation phase from an axisymmetric mating pair, a Joint axis,
`q=0`, or a release-local flip flag. If phase matters to function, require a
stable off-axis landmark direction or an independently grounded local frame.
Without that evidence, report that phase as `UNKNOWN` and narrow any assembly
PASS to the invariants actually covered.

Use one flat live-evaluation path:

```text
hash-bound active profile
  + one binding plan containing only native identities/selectors
  -> one read-only canonical native CAD snapshot
  -> trusted recomputation of every metric
  -> PASS / FAIL / UNKNOWN
```

The binding plan must not contain thresholds, measurements, or outcomes. The
snapshot must not contain derived engineering metrics or a self-reported
decision. Missing scope, pair, endpoint, joint, or native evidence is UNKNOWN;
an observed or structural violation is FAIL. Native axis-aligned bounds are
only bounds evidence, not proof of exact topological containment.
The bounded P1 semantic projection lacks complete native solid-body inventory
and pair closure, so it may report criterion results but never upgrades them to
an overall `FunctionalAssemblyPass`; real assembly PASS comes only from the
live profiled runtime. The offline `evaluate_profiled_snapshot` entry point is
diagnostic only and cannot return a top-level PASS.

Contact at a selected interface requires the native minimum distance between
the exact, version-bound BRep face sets named by that interface-pair binding,
plus its profile-owned
`InterfaceBoundaryMinimumDistanceMetric` interval. An occurrence-wide body
minimum distance never substitutes for this interface-specific observation;
body-pair observations remain the source for interference closure. The current
bounded-cylinder engagement adapter requires exactly one interval for each of
axis alignment, axis separation, interface-size difference, selected-boundary
distance, engagement depth and positive interference volume; a missing,
duplicate or unsupported metric leaves profile selection fail-closed.

Treat a profile digest as derived integrity data: manifest hashes bind static
profiles, while the live profiled runtime, P1 kernel and semantic service must
recompute the canonical executable projection. An inline graph cannot
authenticate its own digest literal.

