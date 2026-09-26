# S07 parametric modeling intent

## Scope

Use this reference when a CAD task contains a master size, dependent feature dimensions, midpoint-derived placement, symmetric geometry, repeated regeneration, or a claim that a model scales proportionally.

## Core dependency graph

'DiceSize' is the root design freedom.

'DivotDepth = DiceSize / 20', 'PipOffset = DiceSize / 4', and 'EdgeRadius = DiceSize / 25' are pure-ratio dependencies.

'DivotDia = DiceSize / 4 - 0.1 in' is an affine dependency with a fixed physical offset. It is not a strict similarity rule.

## Required reasoning

1. Establish the stable datum at the origin.
2. Use symmetric full-length features when symmetry is design intent.
3. Separate independent dimensions from derived relationships.
4. Encode derived placement with geometric constraints.
5. Build the parameter dependency graph before mutation.
6. Classify every expression as pure ratio, affine, piecewise, or independent.
7. Derive legal parameter bounds for affine and piecewise expressions.
8. Regenerate at small, baseline, large, and restored values.
9. Validate semantic invariants separately from BRep health.
10. Preserve failure evidence and avoid destructive cleanup without authorization.

## Fusion API rule

Do not assume UI sketch inference exists in API-created geometry.

A regeneration-safe center rectangle requires explicit horizontal constraints, explicit vertical constraints, width and height dimensions, and an origin anchor. One robust anchor is a construction diagonal whose midpoint is constrained to the sketch origin.

For midpoint-derived pips, create construction lines from the anchored origin to constrained corners or edge midpoints. Constrain pip centers to the construction-line midpoint. Do not replace this relation with repeated X/Y coordinates unless coordinate freedom is the actual design intent.

Parameter input rule: after editing in Change Parameters, read back the parsed expression and the full parameter table before accepting. A past incident parsed '10in' as '10 min()' and a focus error overwrote DivotDia. Write units canonically (e.g. '10 in').

## Direction rule

For a cut starting on a signed support plane, compute the vector from the plane origin toward the model origin. Compare it with the support-plane normal by dot product. Use positive extent when the dot product is positive and negative extent otherwise.

## Scale semantics

| Expression | Class | Invariant |
|---|---|---|
| 'DiceSize / 4' | Pure ratio | Placement ratio '0.25' |
| 'DiceSize / 20' | Pure ratio | Depth ratio '0.05' |
| 'DiceSize / 25' | Pure ratio | Fillet ratio '0.04' |
| 'DiceSize / 4 - 0.1 in' | Affine | Fixed gap term, variable diameter ratio |

Positive 'DivotDia' requires 'DiceSize > 0.4 in'. A manufacturing task must impose stronger bounds when tool radius, wall thickness, draft, or minimum web constraints require them.

If strict visual similarity is intended, use 'k * DiceSize'. If a fixed physical gap is intended, name that gap as a parameter and keep the affine expression explicit.

## Semantic die checks

A standard six-sided die model in this lesson has face counts 1 through 6, total pip count 21, and opposite pairs 1-6, 2-5, and 3-4. Every pair sums to 7.

These checks are independent of body validity, face count, screenshot appearance, and timeline health.

## Failure lessons

| Failure | Root cause | Durable correction |
|---|---|---|
| Root component rename rejected | Invalid Fusion mutation | Name bodies and features instead |
| Pip sketch overconstrained | Repeated coordinate dimensions | Use midpoint topology |
| Large-size regeneration failed | API center frame lacked explicit anchors | Add H/V constraints and midpoint-to-origin anchor |
| Downstream cut and fillet failures | Earliest sketch dependency drifted | Repair the earliest unhealthy dependency first |

## Skill evolution status

This is reproduced curriculum evidence and is immediately usable as candidate CAD Agent memory.

Promote a reusable Fusion helper for explicit center-frame anchoring when the rule recurs or gains an isolated regression fixture.

Promote a fusion-video-learning correction rule when multiple tutorials repeat mathematically imprecise claims that differ from visible UI expressions.

Do not treat S07 curriculum evidence as production acceptance or semantic release authority.
