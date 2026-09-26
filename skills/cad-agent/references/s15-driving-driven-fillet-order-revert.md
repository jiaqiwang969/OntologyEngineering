# S15: Driving/Driven Dimensions, Ordered Fillets, and Revert Position

## Scope

Use this curriculum-only reference for a Fusion arm-like component whose source drawing closes a constrained profile, then adds ordered fillets, a centered blind hole, edge breaks, appearance, component mirroring, and a temporary occurrence move followed by Revert Position.

## Dimension authority

- Drawing dimensions and explicit geometric constraints are driving authority.
- A closing length already implied by tangency, perpendicularity, angle, and other drivers must remain driven/reference.
- Preserve an over-constraint warning as evidence; never force the displayed nominal with a redundant driver.
- Distinguish aligned segment length from horizontal or vertical projection.
- Transient drag values are UI previews, not drawing authority.
- Section-view label X-X means the part is cut along the dashed line, with arrows giving the viewing direction; dimensions placed in section or detail views are binding authority (cross-link from s05 step 3).

## Ordered recipe

1. Freeze the drawing parameters and build the main sketch with bilateral circle tangency, a 145-degree included angle, and a perpendicular wrist line.
2. Verify the closing segment is driven and approximately 0.226676 in; the nominal display is 0.227 in.
3. Extrude the arm body 0.100 in and shoulder post diameter 0.100 in by 0.125 in.
4. Apply the 0.080 in bicep fillet before the 0.070 in forearm fillet.
5. Create the centered diameter 0.075 in hand hole. If face boundaries partition the circle, select the profile union that reconstructs the full circular area.
6. Cut inward 0.125 in and verify a cylindrical face of radius 0.0375 in.
7. Apply radius 0.0025 in to exactly 12 source edges: seven large-side perimeter edges, four wrist transverse/tangent-chain edges, and one planar hole-entry edge.
8. Apply appearance ID Prism-093.
9. Mirror the component across root YZ and verify independent component definitions.
10. For position mutation, require a pending snapshot, call Snapshots.revertPendingSnapshot, and verify exact matrix restoration.

## Recovery rules

- Confirm API members against the running Fusion release.
- Read back after a failed Fusion transaction; do not assume partial geometry survived.
- Select geometry by role, topology, dimensions, area, normals, and adjacency. Collection index and tempId are diagnostic only.
- Occurrence.name is read-only; rename component definitions.
- Protect a trusted source with Save As or a designated practice copy, and save
  meaningful checkpoints there. Never close or discard unsaved valuable work
  without first confirming the intended recovery path.
