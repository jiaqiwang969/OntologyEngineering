# S08 shared-sketch feature-staging intent

## Scope

Use this reference when a CAD task reuses one coplanar sketch for multiple features, contains overlapping primitives that create several profiles, stages different extrusion depths, places a Hole from a sketch point, or delays fillets until the end.

## Drawing decomposition

Separate four concerns before mutation:

1. Section-defining primitives and dimensions.
2. Profile partitions created by intersections.
3. Additive or subtractive feature stages and their operation modes.
4. Edge treatment such as fillets and chamfers.

In this lesson, the primitive sketch contains two concentric circles, one 2 by 2 rectangle, and one located point. It does not contain the final rounded silhouette.

A complete circular feature is diameter-controlled in this drawing, while an interrupted external arc is radius-controlled. This is a drawing-context rule, not a universal naming shortcut.

## Shared-sketch rule

A sketch is not one consumable profile. Intersecting primitives create bounded profile partitions.

For S08, five regions exist:

| Profile | Area in2 | Meaning | Additive consumers |
|---|---:|---|---|
| 0 | 0.371223351 | Inner-bore quarter | None |
| 1 | 2.772815370 | Rectangle outside main outer circle | Lug Join |
| 2 | 0.855961280 | Annulus quarter inside rectangle | Main ring and Lug Join |
| 3 | 1.113670052 | Remaining inner bore | None |
| 4 | 2.567883839 | Remaining annulus | Main ring |

The main ring consumes profiles 2 and 4. The lug consumes profiles 1 and 2. Profiles 0 and 3 remain unconsumed, preserving the main through-bore.

Record profile identity, geometry, and consumers. Do not rely on profile list index as the only identity across regenerations; prefer bounded-loop topology, representative points, area, and semantic containment.

## When to share or split

Share one sketch when plane, stable datum, ownership, change cadence, and downstream lifecycle align.

Split sketches when support geometry differs, one feature changes independently, the profile graph becomes hard to explain, ownership differs, or failure isolation matters more than compactness.

A shared sketch is a dependency optimization with coupling cost, not a default measure of model quality.

## Feature staging

Use operation modes as design semantics:

| Stage | Profiles | Depth | Operation |
|---|---|---:|---|
| Main ring | 2 and 4 | 1.000 in | New Body |
| Lug | 1 and 2 | 0.500 in | Join |
| Small hole | Located sketch point | Through all | Hole / Cut |
| Edge treatment | Five silhouette edges | R0.750 and R0.125 | Fillet |

When the first feature hides the sketch, intentionally reshow it before the next consumer. Visibility is UI state; profile-consumption provenance is design state.

## Hole direction rule

A Hole positioned by a sketch point naturally points opposite the parent sketch normal.

Before mutation:

1. Read the support-plane normal.
2. Determine which signed side contains the target body.
3. Compare that side with the Hole natural direction.
4. Reverse direction when they differ.
5. Specify participant bodies when multiple or ambiguous targets are possible.

In S08, the XZ plane normal is +Y, the body occupies +Y, and the Hole natural direction is -Y. Setting 'isDefaultDirection = false' is required.

Do not infer success from a preview arrow alone. Reacquire cylindrical faces and verify through-hole, counterbore, and target-body topology.

## Deferred fillet rule

Keep physical fillets downstream when they are edge treatment rather than primary section geometry.

S08 uses one Fillet feature with two semantic sets:

| Set | Edge meaning | Count | Radius |
|---|---|---:|---:|
| Lug round | Upper-left extrusion-parallel silhouette edge | 1 | 0.750 in |
| Typical | Remaining sharp silhouette transitions | 4 | 0.125 in |

Select edges by semantic location and geometry. Edge indices are diagnostic evidence, not durable design identity.

Model a radius in the sketch instead when it defines the primary section, controls mating geometry, or must participate in upstream constraints.

## API and transaction rules

Fusion API sketch creation does not guarantee UI inference. Add horizontal, vertical, coincident, and dimensional constraints explicitly.

A failed MCP script can roll back the entire script transaction. Before recovery, reacquire document, body, sketch, and feature counts. Do not assume earlier entities survived.

Checkpoint at meaningful feature boundaries when direction or topology is uncertain. This is not permission for multiple concurrent writers.

When the drawing expresses "equal", prefer an equal geometric constraint over duplicated driving dimensions, and verify the constraint actually exists after creation.

## Failure lessons

| Failure | Root cause | Durable correction |
|---|---|---|
| Root component rename rejected | Read-only API target | Name body and timeline features |
| Hole logical selection failed | Natural direction opposed body side | Measure normal side and reverse direction |
| Diagnostic body index failed | Prior transaction rolled back | Reacquire state before diagnosis |
| Monolithic profile assumption | A sketch contains five regions | Record feature-to-profile consumption |

## Skill evolution status

This is reproduced curriculum evidence and is immediately usable as candidate CAD Agent memory.

Promote a Fusion profile-partition helper only after the need recurs or gains an isolated regression fixture. A durable helper must classify profiles by topology and containment, not only list index.

Promote a Hole-direction preflight helper after a second reproduced lesson confirms the same normal-side rule across another support plane.

Promote a fusion-video-learning extraction rule when repeated tutorials require the same drawing-fact, feature-stage, and transient-UI reconciliation pattern.

Do not treat S08 curriculum evidence as production acceptance or semantic release authority.
