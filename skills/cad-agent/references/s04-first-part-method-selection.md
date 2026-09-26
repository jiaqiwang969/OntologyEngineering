# S04 first-part method selection

## Source-grounded contract

S04 creates one rectangular block: 2 inches along X, 3 inches along Y, and 1 inch along Z. Both source methods start from the XY plane and finish with 6 cubic inches of volume and 22 square inches of surface area.

## Method comparison

| Concern | Direct Box primitive | Centered sketch plus Extrude |
| --- | --- | --- |
| Source operations | Create > Box | Create Sketch > Center Rectangle > Extrude |
| Origin role | Lower corner | Base center |
| Source timeline | One Box feature | Sketch then Extrude |
| Parameter ownership | Primitive length, width, height | Profile dimensions plus feature extent |
| Downstream references | Primitive faces and edges | Explicit profile geometry plus feature faces and edges |
| Best fit | Simple primitive with suitable corner origin | Symmetry, profile edits, future references, richer intent |

Equal final geometry does not make the methods equivalent. Compare origin placement, parameter ownership, timeline structure, reference quality, tool support, and replay fidelity.

## Routing policy

Prefer centered sketch plus feature when any of these are true:

- Symmetry or centering at the origin matters.
- Profile dimensions or constraints will change.
- Downstream operations need stable sketch references.
- The profile may become more complex than a primitive.
- Separating 2D intent from 3D extent improves traceability.

Use a direct primitive only when its simplicity, origin semantics, edit model, and current execution support all match the task.

Component ownership precondition: activate a named component before drawing the first geometry so the sketch and features are owned by it, not by the root. In 2704.1.23 the New Component path requires a Convert Design Type -> Hybrid/Assembly context first (verified in attempt-0002).

## Geometry-first gate

Before a writer operation, state:

1. Source and internal units.
2. Exact X, Y, and Z dimensions.
3. Reference plane and positive extent direction.
4. Origin role for each method.
5. Expected local bounding box, volume, area, faces, and edges.
6. Which differences are intentional and must not be normalized away.

When occurrence transforms exist only to display alternatives side by side, validate geometry in each component-local frame.

## Fusion 2704.1.23 API boundary

The public API exposes existing BoxFeatures and BoxFeature delete or dissolve behavior, but no BoxFeatureInput, createInput, or add operation. A direct B-Rep body inside one BaseFeature may contact the geometry and one-feature idea, but it is not a native BoxFeature and does not reproduce primitive edit semantics.

Never rename a fallback into a stronger claim. Record:

- Source method learned.
- Actual Fusion feature created.
- Supported fidelity subset.
- Missing feature identity or edit behavior.
- Version-pinned API evidence.

## Bounded recovery

The Python binding rejected a SketchPoint where addCenterPointRectangle required Point3D. The correct recovery was to resume the known empty sketch with Point3D at the origin, not restart blindly or duplicate completed geometry.

After a typed failure:

1. Record the exact operation, supplied type, required type, and partial state.
2. Resume only if completed operations and the next safe boundary are known.
3. Refuse duplicate geometry.
4. Re-read the resulting CAD state before acceptance.

## Evaluation boundary

S04 may advance the curriculum but does not prove production readiness. Semantic release and production activation remain separately gated.
