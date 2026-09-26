# S21 — Lid/fastener interfaces and topology-first recovery

State: Candidate working memory grounded in `S21/attempt-0001`. Two
WorkflowExperienceRecord files exist, but their stored gate reports are stale
projections. Current recompilation may produce proposals; neither state admits
or activates them.

## Use this route

Use this reference for enclosure lids, clearance/countersink interfaces,
authentic screws, finishing failures, nonmanifold diagnostics, or a parametric
model that passes visually but fails Fillet/Chamfer regeneration.

Separate the problem into four aspects:

1. lid-to-box dimensional and positional dependencies;
2. screw/insert catalog identity and mating-interface compatibility — derive
   the axial length budget (lid thickness plus available insert depth) before
   selection, and check the recorded penetration (`0.3125 in` in S21) against
   the insert depth;
3. native BRep connectivity before finishing;
4. extreme sweep, baseline restore, save, export, and exact reopen.

## Supplier/session recovery

S21 reused the established Fusion-embedded McMaster semantic-CDP route for
exact screw `92210A538`, format `3-D STEP`. An unauthenticated fetch of the
captured URL returned HTTP 403; do not retry it until the session/authentication
context changes. A concurrent Runtime/Page/Network enable also returned CDP
`-32000`; this did not imply total CDP failure. The bounded recovery enabled
Runtime, then Page, then Network sequentially, rediscovered semantic controls,
used the official Download action, froze the STEP digest, and validated the
imported BRep and provenance.

Rediscover selectors, target, part number, exact format, and download behavior
for every new release/session. Never copy S21's URL or historical PASS as a
current acquisition result.

## Topology-first finishing recovery

After an ASM nonmanifold edge/vertex error, stop changing edge selections.
Read native edge-face incidence and intended body connectivity first. In S21,
the floor and walls shared only an edge, leaving four nonmanifold edges. A
named `S21ManifoldOverlap = 0.001 in` created bounded volumetric overlap,
closed the incidence defect, preserved the external bounds, and allowed the
native Fillet/Chamfer chain to rebuild.

The literal `0.001 in` is release- and model-local. For another model derive
any repair from kernel tolerance, part scale, material intent, allowed internal
geometry change, and downstream face stability. Then verify:

- nonmanifold incidence before/after;
- intended cavity and external bounds;
- feature health and semantic edge sets;
- extreme parameter regeneration and exact baseline restore;
- exact-version reopen and native archive identity.

Do not infer full screw-seat contact from coaxial centers and flush top faces.
S21 deliberately records the video's `100°` lid countersink versus the exact
supplier screw's `82°` head as an unresolved interface mismatch. The video
narration at ~10:25 claims the `82°` countersink matches the screw, but the
committed Hole dialog reads `100°`; when narration and dialog conflict, the
committed dialog value governs, and the conflict must be recorded explicitly.

## Projection and authority boundary

The two stored gate reports say `RECORDED_WITH_BLOCKERS` because they captured
an older active-bundle digest mismatch. Recompute from the immutable record
inputs when reviewing current compatibility; keep that live result separate
from the stored historical report. A `COMPILED_PROPOSALS` result still means:

```text
candidate_admission_authorized = false
shared_memory_activation_authorized = false
execution_authorized = false
```

Primary evidence:

- `curriculum-learning/S21/attempt-0001/practice-log.md`
- `curriculum-learning/S21/attempt-0001/checks/s21-fillet-chamfer-and-regression.json`
- `curriculum-learning/S21/attempt-0001/checks/s21-reopen-verification.json`
- `curriculum-learning/S21/attempt-0001/workflow-memory/supplier-acquisition-recovery-workflow-experience.json`
- `curriculum-learning/S21/attempt-0001/workflow-memory/topology-finishing-recovery-workflow-experience.json`
- both adjacent `*-gate-report.json` historical projections

This is P1 code-native reproduction with visible final display and
`p2_ui_credit=false`.
