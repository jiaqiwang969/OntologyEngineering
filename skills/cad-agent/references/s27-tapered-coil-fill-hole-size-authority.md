# S27 — Tapered coil tail, Fill Hole ends, and transparent size authority

State: Candidate working memory. S27 P1 curriculum-cycle completion is
supported by attempt-0003 exact cloud v3 and its hash-bound lesson checkpoint.
Attempt-0002's completion interpretation remains superseded; its measured
geometry facts remain valid only for their stated scope. Attempt-0003 rejected
uniform 0.35 without saving and accepted the axis-conditioned
X/Y/Z = 0.50/0.56/0.45 candidate. After exact reopen, registered Right-view
top/rear residuals are 0/0 px (±1 px); root 0 is native/affine-derived, not a
directly visible Canvas pixel. No P2 UI, shared-memory activation, semantic
release, production, or formal lesson acceptance is implied.

## Use this route

Use this reference for coil/horn/tail-like appendages grown by edge-loop
extension, and for any Form feature whose final size is decided by a
transparent reference canvas rather than by the instructional video.

Construction semantics (attempt-0002, reopen-verified):

- Edge-loop double-click selects the loop; Alt-Move extrudes new segments from
  it. The tail grows segment by segment, each with its own taper and turn.
- Smoothness is chain-distributed: a sharp bend cannot be fixed in the last
  ring alone — each extension revisits upstream rings (a 90° bend needs a 45°
  intermediate ring).
- Self-intersection is forbidden anywhere along the coil; root embedding into
  the body must be a bounded cross-body overlap (the S27 episode held it near
  0.9 cm³ against the body with all other solids at zero).
- Per-end Fill Hole semantics differ: creased cap at the root (holds the
  junction), crease-free collapse at the tip (smooth point).
- Rehearse the whole sequence in a throwaway design first; the historical
  tail recipe must be normalized against the new body's bounding box before
  reuse, and reopened T-Spline serialization text may change while topology
  persists — identity is topological, not textual.

## Size authority (the lesson's internalization essence)

The video teaches construction; the transparent reference canvas decides
dimensions. For S27 the calibrated S25 canvas chain is the size authority:
height and rear protrusion are checked separately on the transparent side
view, on the screen actually hosting Fusion. Shape correctness and size
correctness are independent verdicts. The 0.50x rejection is recorded in
`curriculum-learning/S27/attempt-0002/corrections/20260810-completion-claim-superseded/correction.json`.

## Runtime safety boundary

Attempt-0003 recorded MCP initialize/transport incidents (checks 02a/02b/02c
including the same-PID re-request prohibition); consult the incident registry
before the next native session.

## Evidence and credit boundary

Primary evidence:

- `curriculum-learning/S27/attempt-0002/corrections/20260810-completion-claim-superseded/correction.json`
- `curriculum-learning/S27/attempt-0003/checks/05-absolute-035-transparent-right-rejection-and-axis-derivation.json`
- `curriculum-learning/S27/attempt-0003/checks/10-axisfit-transparent-right-acceptance.json`
- `curriculum-learning/S27/attempt-0003/checks/14-exact-v3-reopen-verification.json`
- `curriculum-learning/S27/attempt-0003/checks/15-exact-v3-reopen-transparent-right-remeasurement.json`
- `curriculum-learning/S27/attempt-0003/lesson-checkpoint.json`
- `curriculum-learning/S27/attempt-0003/practice/s27-axisfit-size-correction-and-closure-log-v1.md`

Completion result: attempt-0003 exact v3 satisfies the P1 curriculum-cycle
condition through native readback, one final save, exact close/reopen, and
registered reopened-Right remeasurement. Right/YZ constrains world Y and Z;
it does not certify world X depth. The historical 0.35 plan and pre-acceptance
authority record remain append-only episode evidence and are not current state.
