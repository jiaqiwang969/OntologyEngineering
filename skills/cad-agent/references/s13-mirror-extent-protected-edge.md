# S13 mirror, extent, and protected-edge method

Use this method when a CAD lesson combines polygons, sketch symmetry, feature mirrors, through-all and symmetric extents, derived one-sided lengths, protected sharp edges, and rendering fillets.

## 1. Define an inscribed polygon explicitly

An inscribed polygon is not just eight approximate lines. Record:

- side count
- construction-circle diameter
- center location
- edge orientation
- equal-edge and vertex-on-circle constraints

For S13, the post definition is eight sides on a 0.205 in construction circle, centered 0.150 in from the centerline, with a horizontal edge.

## 2. Separate sketch symmetry from feature mirror

Sketch symmetry operates on definitions before solid creation. S13 creates one constrained right octagon and eight SymmetryConstraint relations to the left octagon about a fixed centerline.

Feature mirror operates on a completed feature. S13 mirrors the named right axle extrusion across the YZ plane with IdenticalPatternCompute.

Do not collapse these into one generic copy relation.

## 3. Treat extent modes as engineering semantics

The lug uses the same 0.276 in circle for two different operations:

    clearance = Cut + All + Symmetric
    lug disc  = Join + Distance 0.032 in + Symmetric + Whole Length

The first operation clears the full participant body. The second creates a local thin solid. Preserve their order and measurement modes.

## 4. Preserve derived extent expressions

The drawing locates each axle tip 0.216 in from the center plane. The lug is 0.032 in thick:

    AxleHalfLug  = 0.032 / 2 = 0.016 in
    AxleExtension = 0.216 - 0.016 = 0.200 in

Retain both source dimensions and the expression. A hard-coded 0.200 in value loses authority.

## 5. Prefer a derived datum over unstable topology

A side-face sketch reproduced the source interaction but exposed profile and normal-direction ambiguity in automation. The accepted reproduction creates a datum plane at +AxleHalfLug and checks that it is at +0.016 in before extruding to +0.216 in.

This is a disclosed automation variation that preserves the same start authority while removing dependence on face identity.

## 6. Represent sharp edges as an exclusion set

The drawing says two axle-root edges must remain sharp. Model this as a protected set, not an informal note.

The fillet collector must:

- identify both root circles at X plus or minus 0.016 in
- exclude them from typical fillets
- assert that exactly two protected edges were found
- apply R0.0025 only to eligible edges

## 7. Resolve constraint status by evidence

Fusion may report a sketch aggregate status of false while every curve is fully constrained. In S13, the only points reporting false are endpoints of a fixed and fully constrained construction centerline.

Do not add redundant constraints to satisfy a display flag. Trace every unconstrained point to its owner, accept only documented fixed-construction exceptions, and reject any other free entity.

## 8. Enforce mutation postconditions

After Cut, Join, Mirror, and Fillet:

- component body count remains one
- feature health message is empty
- derived plane and axle tip positions match their parameters
- two protected root edges remain
- the result is saved to a named practice checkpoint without overwriting a
  trusted source

## 9. Engineering acceptance

Accept only when drawing facts and derived values remain distinct; sketch and
feature symmetry are traceable; extent modes and operation order are explicit;
derived datum and extent witnesses pass; protected edges remain sharp;
entity-level constraint evidence is complete; deterministic and visual evidence
agree; and the named practice checkpoint saves and reopens correctly when
persistence matters.

This is a candidate method. Promote it into core CAD Agent behavior only after reuse or replay regression evidence.
