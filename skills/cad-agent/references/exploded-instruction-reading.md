# Exploded-instruction reading (dimensionless panels)

Use this reference when placement evidence comes from instruction panels that
carry no dimensions: brick or kit manuals, assembly leaflets, exploded-view
tutorials, and screenshots of step-by-step builds. Such panels are hand-drawn
oblique projections with inconsistent scale. They still encode identity,
orientation, and position, but only through relations inside one panel and
across neighbouring panels. This complements `s05-drawing-to-hole-intent.md`
(dimensioned drawings), `s12-dimension-authority-projection.md` (numeric
authority), the A5 placement contract in
`source-to-fusion-assembly-delivery.md`, and the direction evidence levels
K-DIR-01/K-DIR-02 in `../assembly/references/ontology/criteria.v1.json`.

Derived from and checked against the 2026-09-02/03 brick-tutorial
transliteration (elephant 10 steps / 26 parts, giraffe 8 / 20, lion 12 / 27;
fixture path at the end). Every rule below was paid for by at least one wrong
build that looked plausible and passed the discrete gates.

## Evidence authority for panel sources

Rank the sources before reading. Record every conflict; never adapt silently.

1. The pre-existing state drawn in panel N+1 is the ground truth for step N.
2. The through-arrow in panel N: a vertical arrow lands on the stud or port
   directly below the part feature it passes through.
3. Cell counts measured inside panel N with the same-panel ruler (Step 2).
4. Apparent position inside panel N. Perspective lies: nearer parts are drawn
   lower and larger, and the exploded copy is displaced toward the camera.
5. The finished-product photo decides which side a part is on and excludes
   candidates through empty regions (negative evidence). It cannot fix a pose
   by itself.
6. Icon order in the parts column and the part's appearance. Icon order is not
   build order; appearance is not identity.

## Step 1 - Identify the part from its icon

- Crop and enlarge each icon separately. Classify thin plate versus thick
  brick by side height, count studs, measure width (one cell or two), and for
  curved parts locate the curve: on the top face (curved-top family) or on
  the underside (curved-bottom family). A curved-bottom 1x2 read as a
  curved-top 2x2 reverses the visible curve.
- Bind each icon to its step, not to a part family remembered from earlier
  steps; the same silhouette can be a plate in one step and a brick in the
  next.
- Names, catalogue order, and prior steps do not carry identity (compare
  K-ID-05 and F-ID-06: a configuration suffix is not the size).

## Step 2 - Orientation by same-panel pitch calibration

- Find one already-placed part along each axis in the same panel and measure
  its pitch in pixels: a stud row on a known part gives the transverse cell,
  a known stud column gives the longitudinal cell. In oblique panels the
  transverse cell is often about half the longitudinal cell in image length,
  so four studs across look like two studs along.
- Measure the new part's top-face edges in those cells. A 2x4 whose edges read
  two cells along and four cells across lies across. Do not decide from
  column-counting intuition or from the orientation of the previous part.
- Keep both orientations as live candidates until a measurement excludes one.
  This is the panel analogue of assembly rule 10 ("test both axis signs"). A
  direction rule proven on one part class ("leg units lie across the body")
  is restricted to that class; full-width leg blocks are a different class.

## Step 3 - Position from the through-arrow

- Follow the arrow's vertical line: record which stud of the new part it
  passes through and which stud or hole of the host it lands on.
- One-stud cantilevers and single-side hangs are legal placements. Partial
  engagement is an observed state, not a verdict (see
  `../assembly/references/claim-boundaries.md`); never exclude a candidate
  because it looks weak.
- If the arrow cannot be resolved to one port pair, hold: report `UNKNOWN`
  with the enumerated candidates and the single observation that would decide
  between them (which stud, which side). Do not present the most plausible
  candidate as the result.

## Step 4 - Verify with the next panel and the photo

- Re-read panel N+1. The parts placed in step N must appear there with the
  same stud field, exposed columns, and side faces. A 4x3 stud field in panel
  N+1 falsifies any step-N reading that yields 2x5.
- Use the photo for side and absence only: an empty region under a column
  proves that no part hangs there on that side.
- Run the discrete gates after every step: volumetric overlap, stud/hole
  connection at interface level (proximity is not connection, K-IF-02),
  unconnected parts off the ground, and inventory against the icons. These
  are the lattice analogues of interference, interface-graph, and
  connection-tree checks; all must pass before the next panel is read.

## Perturbation order under reviewer feedback

When a reviewer says "orientation right, still slightly off", keep the
orientation and enumerate one-cell translations first, including translations
that hang the part one cell beyond the host or onto one side only. Only then
revisit orientation, and only last revisit part identity. Present the
candidates with the discriminating evidence for each instead of one guess.

## Camera discipline for visual comparison

- Identify the panel's viewpoint from asymmetric features (rounded corner,
  face block, muzzle) before rendering. Render the reproduction from that
  viewpoint and keep it fixed across consecutive panels drawn from one side.
- A match under a different camera is not evidence, and a mismatch under a
  different camera is not a defect (compare S17: a face seen from an
  arbitrary camera is not sufficient evidence).
- Measured cell counts decide, not likeness. A candidate set can be fully
  self-consistent under one wrong premise: four candidates for the lion's
  step 9 were all wrong while all assumed the same orientation.

## Promotion status of these rules

- Recurred independently, eligible as rules: same-panel pitch calibration
  (lion S9, S12); "leg units lie across the body" for 1x2-leg units (giraffe
  S5, S7); plate-versus-brick icon classification (elephant S4, giraffe S2);
  camera-side identification before comparison (lion S7, S9).
- Single occurrence, keep as candidates: curved-top versus curved-bottom icon
  classification (lion S12); one-cell sideways hang on one stud (lion S12);
  photo negative evidence (lion S12); paired curved parts as symmetric ears
  (giraffe S3).
- Regression fixtures: the three step chains under
  `~/44-lego-积木/lego-gpt/outputs/bricknet_custom24/object_semantic_safe23_scale1000_candidate_claude_v1_20260825/analysis/instruction_transliteration/claude_takeover_v2_20260902/`
  (`*/*_chain.py`, `*/verification.json`, `final_all.html`). A rule change
  that re-places the lion's step-9 2x4 along the body, or reads the step-12
  part as a curved-top brick, must fail against them.

## Boundaries

This reference derives placements for reproduction and review. It does not
prove fit, tolerance, insertion feasibility, tooling, or physical release;
those remain S0-S11 claims with their own evidence.
