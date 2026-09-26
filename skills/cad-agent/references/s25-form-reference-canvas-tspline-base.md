# S25 — Reference canvas calibration and T-Spline base form

State: Candidate working memory distilled from the persisted S25 attempt-0005
records. Attempt-0005 exact v5 is the accepted predecessor authority for the
S26–S28 Form chain; this reference compiles its kernels without synthesizing a
missing workflow record.

## Use this route

Use this reference when starting any canvas-driven Form/T-Spline model: sculpt
body from reference images, organic part with front/side canvases, or any
lesson whose dimensional truth lives in a calibrated reference image.

Kernels proven in attempt-0005:

- **Calibrate exactly one canvas** to a real dimension (6 in in the episode).
  That calibrated revision becomes the downstream scale authority for the
  whole chain; align every other canvas by landmark only, never by a second
  independent calibration that could contradict the first.
- **Primitive-per-feature decomposition**: each anatomical feature starts from
  its own primitive (box, quadball, cylinder) rather than extruding everything
  from one cage; the primitive choice is a topology decision, not styling.
- **Fewer span faces**: start coarse (quadball at 2 spans) — control-point
  economy is what keeps later per-view alignment solvable. Refinement needs
  current-revision insufficiency evidence, not habit.
- **Symmetry semantics depend on the creation plane**: mirror/internal
  symmetry behaves differently depending on which plane the primitive was
  created on; test one symmetry at a time before stacking.
- **Manipulator grammar**: snap arrow = axis translate, free square = plane
  translate, uniform scale handle ≠ per-axis scale; misreading the manipulator
  silently distorts topology.
- **Per-view silhouette acceptance, exact reopen first**: every accepted stage
  reads back from an exact reopened revision; per-view registration and
  measurement follow the multi-view contract in
  `reference-alignment-iteration`.

## Evidence and credit boundary

Primary evidence:

- `curriculum-learning/S25/attempt-0005/practice-log.md`
- `curriculum-learning/S25/attempt-0005/checks/07-export-exact-v5-f3d.json`
- `curriculum-learning/S25/attempt-0005/checks/08-clean-reopen-after-display-canary.json`

The frozen artifact is exact v5 (24 faces / 48 edges / 26 control points,
canvas-first). Earlier attempts remain diagnostic branches only. This
reference is a curation candidate; the calibrated-canvas scale authority it
names is the same authority the S27 tail size acceptance is gated on.
