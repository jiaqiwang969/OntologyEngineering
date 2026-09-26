# S12 dimension authority and projection method

Use this method when a CAD lesson combines driving dimensions, bracketed reference values, missing drawing data, projected geometry, and features normal to angled faces.

## 1. Classify before modeling

Every numeric claim must be one of:

- DrivingDimension: authoritative value allowed to control geometry.
- ReferenceDimension: check-only value, often bracketed.
- DerivedDimension: calculated from the driving chain.
- RecoveredSpecification: value obtained from an explicitly named non-drawing authority.
- Inference: agent conclusion bounded by evidence.
- DiscretionaryChoice: value not specified by the drawing, fixed by the designer through visual experiment, and renegotiable without violating drawing authority.

Never allow a reference or recovered value to silently become a drawing fact.

## 2. Resolve dimension conflicts explicitly

For S12:

    BaseWidthDerived = TopWidth + 2 * BodyHeight / tan(SideAngle)
                     = 0.400 + 2 * 0.500 / tan(78 deg)
                     = 0.612556562 in

The drawing bracketed value is 0.611 in, so the discrepancy is 0.001556562 in. Preserve the three driving dimensions, retain the bracketed value as a check, and record the mismatch.

## 3. Recover missing data without laundering authority

The drawing omits arm-hole diameter and depth. The tutorial measures the trusted source model and supplies diameter 0.100 in and depth 0.200 in.

Represent this as a RecoveredSpecification that fills a DrawingSpecificationGap and links to a SourceAuthority and evidence locator.

## 4. Preserve projection dependency

A coincident analytical point is not equivalent to a linked projection. The preferred chain is:

    SK_ARM_CENTER_REFERENCE endpoint
      -> linked project2
      -> SK_RIGHT_ARM_HOLE_PROJECTED point
      -> HOLE_RIGHT_ARM_RECOVERED

If linked projection cannot be created, disclose the variation.

## 5. Define angled holes in the support-face frame

The hole axis is normal to the sloped face, not global X. For a 78-degree side:

    absolute axis = (sin 78 deg, 0, cos 78 deg)
                  = (0.9781476007, 0, 0.2079116908)

Normalize the sign toward the body interior before comparing mirrored axes.

## 6. Mirror features, not unexplained geometry

Mirror the named right-hole feature across the YZ plane. Record the source feature, mirror plane, participant body, and symmetric postconditions. Select-through is a UI acquisition technique only.

## 7. Make the direction frame explicit

Do not infer a world direction from the plane label alone. Bind the local
sketch axis, its sign, the world axis, and the intended semantic direction
before creating one-sided features.

The accepted S12 attempt-0002 construction uses this mapping:

    sketch local -Y -> world +Z
    wide bottom      -> world z = 0
    narrow body top  -> world z = +BodyHeight
    post top         -> world z = +TotalHeight

The earlier local `+Y -> world -Z` build had the same dimensions, volume class,
constraints, and feature names, but put the wide bottom-hole face above the
narrow post end. Peekaboo evidence rejected it before save. Check the sign of
every one-sided Join and Cut in the bound frame. After Join, assert both one
body and the intended new BRep topology.

## 8. Acceptance gate

Accept a curriculum checkpoint only when driving, reference, and recovered
claims remain distinguishable; every sketch is fully constrained; projection,
axis, and mirror witnesses pass; one body remains; feature health is clean;
the declared BRep target state exists; deterministic and visual orientation
evidence agree; the native artifact is hashed; and the authorized save barrier
has stable cloud lineage and version.

`isSaved=true` with an empty cloud version ID is `cloud_pending`, not a reason
to repeat non-idempotent `saveAs`. Poll read-only state until the active
DataFile, target folder enumeration, and document search agree.

## 9. Feature health is not target-state proof

S12 produced a healthy top-post Join while its construction plane was outside
the torso. The feature reported no error but added no post cylinder and did not
change the body bounds. A direction-sensitive feature therefore needs all of:

- feature health and empty error text;
- expected participant-body count;
- expected BRep topology, bounds, or volume delta;
- support-plane origin and normal in the declared frame;
- visual orientation evidence when the domain has semantic up/down.

## 10. Use semantic selectors and nullable BRep contracts

Select fillet edges by geometric role, not transient edge index. Fusion may
return a degenerate BRep edge whose `geometry` is null. Skip that edge, retain
the declared cardinality gate, and fail if the semantic set is still incomplete.
For S12 the successful pre-fillet sets are exactly four top edges and thirteen
typical edges.

## 11. Practice evidence and status

The practice-proven candidate package is rooted at:

    curriculum-learning/S12/attempt-0002/internalization/

Its source archive SHA-256 is
`9bf15bc2dfaee1c96d93aaf1f247b56ddf768cd0e090ec373bdd10cc494c68da` and
its cloud lineage is
`urn:adsk.wipprod:dm.lineage:WnjU6O_WRh65oMpiSfByeA`, version 1.

This remains an advisory candidate method. It may guide the next curriculum
lesson but cannot authorize Fusion writes, semantic acceptance, Skill
activation, or production release until its candidate graph, SHACL, CQs,
replay, regressions, and independent review pass.
