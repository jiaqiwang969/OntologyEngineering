# S22 — Parametric tire-section closure and regeneration

State: Candidate working memory grounded only in the persisted S22 checks and
workflow-memory preflight. No S22 practice log or WorkflowExperienceRecord is
synthesized by this reference.

## Use this route

Use this reference for a revolved tire/torus-like section, offset wall
thickness, tangent bead closure, width mirroring, derived dimensional
parameters, or a sketch that must remain closed and fully constrained across a
parameter sweep.

Model the section as a dependency graph rather than a traced silhouette:

```text
TireWidth + AspectRatio -> TireHeight
RimDia + TireHeight - 2*TreadHeight -> TireOuterDia
outer crown/sidewall/bead roles
  -> inner offset chain by -TireWallThickness
  -> tangent bead closure + symmetry closure
  -> one closed, fully constrained profile
  -> half-width revolve -> mirror -> join -> shoulder fillet
```

The exact formulas and baseline values are episode inputs. Reuse the roles and
checks, not S22's historical numeric PASS.

## Closure and validation gates

Before revolving, require one profile, full sketch constraint, correct offset
parent/child roles, valid tangent constraints, and a measured wall-thickness
relation. After the native feature chain, require one solid, healthy features,
bounds that agree with half-width and outer-radius expressions, and the expected
section-analysis plane.

Sweep both reasonable and discriminating parameter states. A source-observed
combination that exceeds the valid solver/domain should remain an explicit
failed case; restore the baseline and prove the restored expressions, sketch
closure, feature health, bounds, and body cardinality. Do not turn a failed
sample into a hidden clamp.

For persistence, bind cloud lineage/version and F3D digest, close the active
document, reopen the exact version, and read the saved state before any
`computeAll()` side effect. If verification computation marks the transient
session modified, discard it rather than overwriting the accepted checkpoint.

## Evidence and credit boundary

Primary evidence:

- `curriculum-learning/S22/attempt-0001/workflow-memory/workflow-memory-preflight.json`
- `curriculum-learning/S22/attempt-0001/checks/s22-build.json`
- `curriculum-learning/S22/attempt-0001/checks/s22-parameter-sweep.json`
- `curriculum-learning/S22/attempt-0001/checks/s22-save-export.json`
- `curriculum-learning/S22/attempt-0001/checks/s22-reopen-verification.json`

The frozen artifact is cloud v1 and
`artifacts/export/S22_Parametric_Tire_Part1_A0001_v1.f3d` on Fusion
`2704.1.36`. The reopen report explicitly limits the claim to the tested
baseline/parameter contract and disclaims P2 UI execution. This reference is a
curation candidate, not retroactive evidence that a missing practice log or
workflow record existed.

