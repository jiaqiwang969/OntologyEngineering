# S16: Drawing Authority, Delete Face, and Component Mirror

## Scope

Use this curriculum-only reference for a Fusion hand-like component built from a concentric C profile, a joined post, healed internal-face deletion, mixed face/edge filleting, localized appearance binding, and a component mirror across a body face.

## Authority and dimensional closure

- Freeze the dimensioned drawing before modeling.
- If a transient Fusion dialog conflicts arithmetically with the drawing, record the conflict and resolve it explicitly; never silently copy the UI.
- For S16, the drawing requires 0.125 in total thickness and fixes side two at 0.040 in. Therefore side one is 0.085 in even though the source dialog visibly shows 0.080 in.
- Read back physical thickness after feature editing.
- Extend the authority order with a DiscretionaryChoice class: values the drawing never specifies, fixed by the designer through visual experiment, and renegotiable. The S16 edge-break radius 0.0025 in is DiscretionaryChoice, not a drawing-authoritative value.

## Ordered recipe

1. Create component 'Left Hand' and a fully constrained XY sketch with outer radius 0.068 in, inner radius 0.055 in, opening width 0.075 in, and construction-axis symmetry.
2. Create the initial symmetric 0.125 in extrusion, then edit it to two sides 0.085 in and 0.040 in. This two-sided split deliberately keeps the shared origin on the post axis, so the XZ post sketch is origin-centered and needs only one diameter dimension; the extent choice carries datum intent.
3. On XZ, sketch a centered diameter 0.075 in post and extrude 0.175 in as New Body.
4. Combine with Join and verify two bodies become one.
5. Delete and heal the intruding internal cylindrical post face. A planar face is a preserved negative example, not the target.
6. Fillet the source-selected four faces and two edges at radius 0.0025 in with tangent-chain semantics.
7. Bind source appearance 'Paint - Enamel Glossy (Yellow)' to installed stable ID 'Prism-095'; localized display text is evidence, not identity.
8. Mirror the component across the planar free end of the post, not a root plane.
9. Verify independent component definitions, one solid per occurrence, post-end touching, near-equal volume, healthy features, and no residual internal post cylinder.

## Recovery rules

- Confirm API members against Fusion release 2704.1.23. Two-sided extrusion editing requires explicit zero-degree taper inputs, and view enums use PascalCase.
- After any failed script, read back whether the transaction rolled back or retained a checkpoint.
- A fixed construction line can still leave endpoint completeness false. Prefer origin coincidence plus aligned length; symmetry may already imply orientation.
- Do not duplicate a symmetry-implied direction with an explicit vertical constraint.
- Read provenance attributes from native bodies and transformed geometry from occurrence proxies.
- Protect a trusted source with Save As or a designated practice copy, and save
  meaningful checkpoints there. Never close or discard unsaved valuable work
  without first confirming the intended recovery path.
