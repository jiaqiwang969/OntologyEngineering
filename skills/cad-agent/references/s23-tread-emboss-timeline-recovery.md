# S23 — Tread Emboss branches and timeline rollback recovery

State: Candidate working memory grounded in five distinct S23 attempts. Keep
their identities and course-credit boundaries separate.

## Branch inventory

- `attempt-0001`: complete raised-tread tutorial reproduction. Native positive
  Emboss, mirror, circular pattern, shoulder cleanup, parameter sweep, export,
  and exact v2 reopen. It is P1 code-native and `p2_ui_credit=false`.
- `attempt-0002`: complete recessed-tread user override in a different lineage.
  It preserves the raised checkpoint, proves signed Deboss depth and residual
  wall, sweeps parameters, and reopens exact v1 then v2. It is not tutorial/P2
  equivalence.
- `attempt-0003`: source-faithful experiment with a retained rejected debug
  result and fork evidence only. It has no final artifact, practice log, or
  reopen acceptance and remains historical/incomplete.
- `attempt-0004`: corrected source-reading branch that proved the full R3 circle
  and video parameter order, but edited cleanup geometry into the
  Revolve-consumed S22 sketch. It fails at `AspectRatio 75→60` because the
  upstream profile-topology change invalidates the S22 Fillet edge references.
  Its clean v2 is recovery evidence, not the next-course checkpoint.
- `attempt-0005`: accepted source-faithful P1 checkpoint. It keeps the video
  tangent arc, full R3 circle, Revolve Cut, mirror and final parameter state,
  but owns cleanup geometry in a downstream YZ sketch projected from the S22
  sidewall. All ten cumulative states pass; exact cloud v3 is closed, reopened,
  read back cleanly, and exported. This is the hard input to S24.

Never let the override supersede the tutorial checkpoint and never let the
incomplete branch become evidence that S23 is complete.

## Tread dependency route

Bind tread construction to the tire definition:

```text
tangent-plane offset <- TireOuterDia
unwrapped pitch       <- TireOuterDia * PI / TreadPatternNu
signed Emboss depth   <- raised/deboss design intent
pattern quantity      <- TreadPatternNu
pattern angle/axis    <- 360 deg about the declared tire axis
shoulder reach        <- TireWidth and the chosen raised/recessed policy
```

`TreadPatternNu` and any pattern/instance-count parameter must be created
unitless; a unit-bearing count breaks both the circumference/count division and
the pattern quantity input.

Then verify fully constrained source profiles, native Emboss/Deboss health,
mirror/pattern health, one watertight solid, instance cardinality, envelope,
and extreme sweep with exact baseline restoration. Before Emboss, also check
that the sketch's profile count equals the expected emboss profile count:
interior solid lines split a cell into multiple profiles; merge them by
converting the interior solid lines to construction geometry.

For S23 specifically, the video does not restore the early baseline after its
final cumulative test. Preserve both meanings explicitly: restore intermediate
pattern probes when the source does, then leave the accepted checkpoint at
`N60 / AR60 / Width350 mm / Rim19 in`.

## Upstream profile ownership and topology naming

Do not add downstream teaching/cut profiles to a sketch already consumed by an
upstream Revolve merely because the source UI edits that sketch successfully.
In API reproduction, changing the sketch's profile inventory can remap the
Revolve result and invalidate later edge-referenced Fillets even when the new
curves do not visibly overlap the original profile.

Diagnose this class in dependency order:

1. reproduce the failing parameter state with downstream features suppressed;
2. test the exact predecessor alone at the same parameter state;
3. compare the consumed sketch's profile inventory before and after the lesson;
4. if the predecessor passes alone, preserve its sketch as an ownership
   boundary and project only the required reference geometry into a downstream
   sketch;
5. retain the source operation semantics and disclose the ownership adaptation;
6. rerun the exact source parameter order, save, close, reopen and read back.

The `attempt-0005` acceptance also corrects two validation assumptions:

- `TreadGap * 1.5` extends the tread across tire width; it is not an extra
  circumferential pitch extension.
- A treaded body's axial bounding box need not equal `TireWidth / 2`. Require it
  to stay within `[TireWidth / 2, TreadOuterReach]`; the final source frame
  visibly retains narrow tread tips beyond the smooth shoulder.
- Embossed dimensions on the curved surface are projection-distorted, shrinking
  toward the pattern center; judge gaps by envelope/topology tolerance, never by
  accepting exact nominal values.

Long cumulative sweeps must journal one state per bounded execution. Write the
current label, expressions, compute result, feature health and BRep state before
raising. A client timeout must not erase already completed states or the first
native failure.

## Timeline rollback recovery

Editing an Emboss property at timeline end can fail with “Didn't roll editing
feature back.” The stable recovery observed in `attempt-0002` is:

1. suppress downstream features that consume the Emboss;
2. roll the timeline immediately before the Emboss edit position;
3. change the signed-depth dependency and Tangent Chain state;
4. move the timeline back to the end;
5. only then reacquire downstream sketches/features and regenerate them;
6. verify failed exploratory transactions rolled back, discard the dirty
   branch, and reopen the exact clean source before the accepted build.

Objects occurring after the rolled-back feature do not exist in the active
timeline context. Do not query or reuse stale object handles until the timeline
is restored.

For a recessed tread, a negative nominal depth is not enough to prove a safe
groove. Measure each groove floor, orient the surface normal, ray-cast to the
first non-self material face, classify points on both sides, require positive
residual wall, and bound shoulder/crown reach. Derive safe depth from current
wall thickness and manufacturing intent; do not copy S23's value blindly.

## Evidence and credit boundary

Primary evidence:

- `curriculum-learning/S23/attempt-0001/practice-log.md`
- `curriculum-learning/S23/attempt-0001/checks/s23-reopen-verification.json`
- `curriculum-learning/S23/attempt-0002/practice-log.md`
- `curriculum-learning/S23/attempt-0002/checks/s23-recessed-reopen-verification.json`
- `curriculum-learning/S23/attempt-0003/checks/s23-a0003-fork.json`
- `curriculum-learning/S23/attempt-0003/checks/debug/s23-a0003-build-pentagon-and-full-circle-rejected.json`
- `curriculum-learning/S23/attempt-0005/practice-log.md`
- `curriculum-learning/S23/attempt-0005/checks/s23-a0005-source-cumulative-path.json`
- `curriculum-learning/S23/attempt-0005/checks/s23-a0005-source-final-v3-reopen.json`
- `curriculum-learning/S23/attempt-0005/checks/s23-a0005-final-acceptance.json`

`attempt-0002` remains the workflow-record candidate for timeline
rollback/reacquisition and residual-wall validation. `attempt-0005` adds a
separate candidate for upstream profile ownership, topology-name isolation and
resumable parameter sweeps. These are curation proposals, not fabricated
admission decisions or capability activation.
