# S14 tangency, trim, blind-hole, and component-mirror method

Use this method when a CAD lesson combines tangent and non-tangent profile boundaries, trim-induced topology changes, component-owned solids, blind holes, small fillets, root-level component mirrors, and localized appearances.

## 1. Treat tangency as an explicit relation

Visual contact is not proof of tangency. Record the two participating curves and create an explicit tangent constraint.

For S14, the retained top arc is tangent only to the right vertical boundary. The left web intersects the circle at x=-0.122 in and is explicitly non-tangent.

## 2. Preserve trim dependencies

Trim changes entity type and identity. S14 starts with one full circle and five lines, trims the lower circular segment, and ends with:

- zero circles
- one retained arc
- five lines
- one closed profile
- one tangent constraint
- a fully constrained sketch

Capture both pre-trim and post-trim witnesses. Never assume constraints or dimensions survived merely because the profile still looks correct.

Treat a Trim error as a symptom of an under-constrained or under-dimensioned sketch, and treat a clean trim as cheap corroborating evidence of sketch constraint completeness.

## 3. Separate authority from topology

The drawing dimensions remain authoritative even when trim replaces a circle with an arc. Store dimensions as parameters and check the resulting geometry independently.

Key S14 derivations are:

    TopDiameter = 2 * TopRadius = 0.276 in
    LeftWebX = TopRadius - WebWidth = -0.122 in
    OverallFootWidth = WebWidth + FootProjection = 0.350 in
    CenterWall = LegThickness - HoleDepth = 0.072 in

## 4. Record direction in the root frame

The drawing depicts the right leg, but the source creates Left Leg by reversing the extrusion.

S14 direction authority is:

    Left Leg  = root negative X, thickness 0.272 in
    Right Leg = root positive X, mirrored across root YZ

Do not rely only on sketch-plane normal labels. Read back body bounds in the root frame.

## 5. Keep component ownership distinct

The side sketch, extrusion, hole, fillet, and body belong to the Left Leg component. Component mirror is a root-level assembly operation.

A body mirror or feature mirror does not satisfy an independent-component requirement. Validate:

- two root occurrences
- component names Left Leg and Right Leg
- two distinct component entity tokens
- one solid body per component
- equal body volumes

## 6. Prove a blind hole geometrically

A blind-hole declaration needs more than a feature name. Verify:

- diameter
- depth
- start side
- inward direction
- remaining wall

S14 has radius-0.0325 in cylindrical faces spanning 0.200 in inward from x=-0.272 and x=+0.272, stopping at x=-0.072 and x=+0.072.

A parameter-derived outer-side datum is acceptable when it removes face-identity and normal-direction ambiguity. Record it as a controlled automation variation.

## 7. Select fillets by semantic role

A broad 18-edge collection failed because some selected references became free during computation. The accepted S14 fillet selects exactly five non-tangent through-thickness profile-corner edges and excludes the smooth tangent-transition edge.

Require health state 0 before using the source component in a mirror. A visually unchanged body with a failed fillet feature is not acceptable.

## 8. Map localized appearances traceably

The source authority is Paint - Enamel Glossy (Blue). On this localized Fusion installation, stable library ID Prism-090 displays under a localized name.

Retain:

- source English authority
- stable library ID
- actual body-level appearance
- ownership on both component bodies

Do not treat a localized display name mismatch as a material mismatch when the stable source ID and appearance properties agree.

## 9. Probe identity-property mutability

This Fusion build rejects root-component renaming and exposes occurrence names as derived values without setters.

Use:

- a root design-id attribute
- writable component names
- Fusion-derived occurrence names
- component entity tokens

Do not make root or occurrence display-name mutability an unstated assumption.

## 10. Engineering acceptance

Accept only when trim topology and tangent authority are revalidated; the
non-tangent boundary remains explicit; parameters and derived values agree;
both blind holes have dimensional witnesses; the fillet and mirror are healthy;
components are independent; appearance mapping is traceable; deterministic and
visual evidence agree; and the result is saved to a named practice checkpoint
without overwriting a trusted source.

This is a candidate method. Promote it into core CAD Agent behavior only after reuse or replay regression evidence.
