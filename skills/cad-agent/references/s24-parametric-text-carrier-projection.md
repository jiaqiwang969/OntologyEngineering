# S24 — Parametric text carrier, projection plane, and appearance persistence

State: Candidate working memory distilled from the persisted S24 attempt-0001
checks and practice records. The two S24 workflow-experience candidates remain
Candidate (no independent review); this reference cites evidence and does not
promote them.

## Use this route

Use this reference for text that must follow a parametric surface: sidewall
lettering, curved engraving, any Text feature whose position and count derive
from model parameters.

Core construction semantics:

- The text carrier is a Center Point Arc with endpoints, never a closed
  circle: text-on-path needs an open path with a defined start so the string
  can center and re-flow when parameters change.
- Vertical centering is a constraint, not a coordinate: offset the carrier
  from the mid-plane by a `textHeight/2` path so unknown future diameters keep
  the string centered.
- Mid-geometry by construction (bridge line + midpoint constraint) survives
  unknown diameters; hard-coded midpoints do not.
- The text sketch lives on a projection-linked expression plane
  (`-TireWidth/2 - 2 in` in the S24 episode) so the carrier follows the tire
  family instead of a frozen offset.
- Prefer Extrude From-Object / New-Body over Emboss when appearance must
  persist: Emboss merges into the target body and loses independent
  appearance; a joined-later new body keeps it.
- Batch parameter application uses `{Param.value:.0f}`-style formatting; the
  final body count contract is `finalBodies = patternQty * seedBodies`.

## Runtime safety boundary

The `thomasa88_ParametricText_Map` interactive command crashed Fusion inside
an MCP transaction and is part of the `InteractiveCommandInsideMcpTransaction`
signature ban. Text parameter mapping through that add-in is P2 UI territory;
the P1 route uses native Text feature parameters only.

## Evidence and credit boundary

Primary evidence:

- `curriculum-learning/S24/attempt-0001/checks/s24-a0001-final-v3-save-export.json`
- `curriculum-learning/S24/attempt-0001/checks/s24-a0001-final-v3-reopen.json`
- `curriculum-learning/S24/attempt-0001/practice-log.md`

The frozen artifact is cloud v3 with exact reopen verification. P1 credit is
established; the bounded ParametricText P2 path is recorded separately. This
reference is a curation candidate distilled after the fact; it does not
retroactively verify the unreviewed workflow candidates.
