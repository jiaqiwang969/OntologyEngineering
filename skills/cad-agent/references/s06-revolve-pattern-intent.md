# S06 revolve and repeated-feature intent

## Scope

Use this reference when a CAD task involves an annular or axisymmetric base, repeated holes, a bolt or pitch circle, a THRU callout, Revolve, or Circular Pattern.

This is candidate operational memory from Lesson 6. It is not a production drawing standard and is not formally evaluated yet.

## Fact and inference discipline

- Preserve OD, ID, height, hole diameters, counterbore depth, quantity, and radial location as separate facts.
- Treat 3X as quantity only.
- Infer equal angular spacing only when drawing geometry, an explicit requirement, or tutorial evidence supports it.
- For production, request an explicit equal-spacing or bolt-circle requirement if it is absent.
- Keep THRU as an extent intent, not a numeric distance.

## Strategy routing

- Choose Revolve when the generating axial section and axis are primary intent.
- Choose Extrude when the base is a planar profile with prismatic thickness.
- Choose a seed Hole plus feature Circular Pattern when repeated-feature quantity and spacing should remain editable.
- Choose explicit constrained points when locations need distinct dimensions or tolerances.
- Do not prefer a shared sketch solely because it shortens the timeline.
- Separate base and placement sketches when change isolation is more important than compactness.

## Fusion execution guards

- Build native component features at identity occurrence transform; apply layout transforms afterward.
- The root component name cannot be changed. Name child components, bodies, sketches, and features.
- HoleFeatureInput.participantBodies uses a Python list of BRepBody objects.
- A Hole placement API defines a natural direction opposite the placement plane normal.
- In the S06 tested path, PositiveExtentDirection followed that natural direction and produced AllExtentDefinition.
- Do not map Positive and Negative directly to global Z. Read the plane normal, natural direction, target intersection, and extent readback.
- If a diagnostic feature succeeds, Undo it before creating the final feature.
- Do not declare a multi-point capability gap until direction, point validity, and target body have been isolated.
- In an axial revolve section, a dimension from the profile to the axis is a radius; enter drawing diameters as diameter/2 or use a diametric dimension.

## Deterministic replay checks

For the S06 reference part:

- OD 4 in.
- ID 2 in.
- Height 1 in.
- PCD 3 in.
- Three positions at 90, 210, and 330 degrees.
- Through diameter 0.375 in.
- Counterbore diameter 0.75 in.
- Counterbore depth 0.5 in.
- Volume 140.870221302157 cm3.
- Area 273.147000450995 cm2.
- 13 faces and 16 edges.
- Bounding-box size 10.16 by 10.16 by 2.54 cm.
- Unique cylindrical radii 0.47625, 0.9525, 2.54, and 5.08 cm.
- Every final sketch is fully constrained.
- Every final Hole reads back as AllExtentDefinition.
- Feature patterns read back as quantity 3 over 360 degrees.

## Semantic boundary

The current CADGeometryMCP catalog is read-only and does not register FusionMCP write operations. It can review a semantic plan but cannot authorize or execute Fusion. Keep external Fusion execution tied to explicit human approval and preserve that boundary in provenance.

## Promotion gate

Reuse these rules in later lessons and replay them before promotion. Formal evaluation remains disabled until all 49 videos are learned.
