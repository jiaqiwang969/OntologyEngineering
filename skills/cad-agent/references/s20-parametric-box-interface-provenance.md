# S20 — Parametric box interfaces and purchased-part provenance

State: Candidate working memory grounded in `S20/attempt-0001`. The released
S10 supplier-acquisition pattern remains the reusable acquisition memory; this
reference does not invent a new pattern or authorize supplier access.

## Use this route

Use this reference for a parameter-driven enclosure with walls, floor, bosses,
countersinks, authentic threaded inserts, or rigid interface placement. Route
the supplier portion through `supplier-cad-acquisition.md` and workflow-memory
preflight before selecting an external search or proxy.

Treat the task as two connected state transitions:

```text
enclosure intent -> closed parameter dependencies -> native solid/checkpoint
exact purchased-part requirement -> official supplier STEP -> imported BRep
native solid + purchased BRep -> typed mating interfaces -> stable joints
```

## Parameter and interface closure

Bind dimensions by engineering role, not only by feature order. In S20 the wall
depth is `-Height + LidHeight`, the floor starts from the wall lower interface,
and the four countersink centers depend on `Width`, `Depth`, and
`WallThickness`. A visually plausible slab at the top was wrong even though
the timeline was healthy; center, floor, wall, bounds, and volume probes exposed
the datum error.

When a parametric sketch becomes unstable at an extreme size, a sharp driven
sketch followed by a native solid Fillet may be the more stable representation.
That is a modeling-strategy choice, not geometric equivalence by assertion.
Re-run the declared extreme sweep and restore the baseline. S08's deferred-
fillet rule also extends to component scale: order cosmetic fillets after all
functional interfaces and downstream in-context consumers.

For purchased components, keep three identities separate:

- exact catalog definition and official source bytes;
- imported source component carrying supplier provenance;
- each physical occurrence/copy used by the assembly.

One successful download event does not prove an authentic imported BRep.
Freeze the exact part/format/URL/digest, read back native bodies and provenance,
place via typed face-center interfaces, save, and reopen the exact version.

S20 acquired McMaster `94459A390` as exact `3-D STEP` through Fusion's embedded
McMaster WebView. Direct HTTP 403 was a session-boundary observation, not proof
that the established embedded-CDP capability was absent. Exact semantic part
and format selection preceded the official Download trigger.

## Assembly and adaptation boundary

The lesson's visible Joint/Flip path and a public-API `isFlipped` value are not
assumed to be one-to-one. Verify installed orientation from mating geometry and
the resulting pose. When shared nested instances moved a free parent container
as one unit, S20 used independent native component copies while preserving the
hidden supplier source occurrence. Record that structure adaptation instead of
claiming exact UI or browser equivalence.

## Evidence and credit boundary

Primary evidence:

- `curriculum-learning/S20/attempt-0001/practice-log.md`
- `curriculum-learning/S20/attempt-0001/checks/s20-bottom-fix-and-regression.json`
- `curriculum-learning/S20/attempt-0001/checks/s20-parametric-joint-sweep-v2.json`
- `curriculum-learning/S20/attempt-0001/checks/s20-reopen-verification-v2.json`
- `curriculum-learning/S20/attempt-0001/artifacts/supplier/mcmaster-94459A390-cdp-report.json`
- `curriculum-learning/S20/attempt-0001/workflow-memory/supplier-acquisition-workflow-experience.json`
- `curriculum-learning/S20/attempt-0001/workflow-memory/parametric-box-assembly-workflow-experience.json`

The accepted checkpoint is cloud v6 on Fusion `2704.1.36`. Earlier pre-floor-
correction reports and the earlier F3D are superseded diagnostics. The episode
is code-native engineering evidence with a visible final display; it does not
claim every source-video UI step.

