# S05 drawing-to-hole intent

## Read drawings before choosing features

Use this order:

1. Read title block, units, scale, declared projection standard, and general notes.
2. Map isometric and orthographic views by face and orientation.
3. Classify visible, hidden, center, section, and other line evidence.
4. Extract dimensions, symbols, datums, tolerances, quantities, and feature notes.
5. Separate explicit facts, supported inferences, assumptions, conflicts, and missing requirements.
6. Establish a local coordinate frame and deterministic geometry contract.
7. Select a feature route only after intent and uncertainty are explicit.

## S05 drawing contract

The source part is a 2 by 3 by 1 inch rectangular block. Its top-opening counterbore has a 0.500 inch through bore, a 1.000 inch counterbore diameter, and a 0.500 inch counterbore depth.

Dashed front and right-view lines show hidden bore edges. They reveal topology but do not supply missing dimensions.

The hole is modeled at local X=0,Y=0 only because the source author snaps both circle and Hole operations to the center and the views are symmetric. The drawing has no independent X/Y location dimensions or positional tolerance. Treat center placement as a curriculum assumption and block production release.

Do not claim a projection standard merely from view arrangement when no projection symbol is declared.

## Hole callout semantics

Interpret the compact callout as one coherent feature specification:

- Through bore diameter: 0.500 inch.
- Through extent: THRU or All.
- Counterbore diameter: 1.000 inch.
- Counterbore depth: 0.500 inch from the top face.
- Drill tip: 118 degrees is a default parameter, but terminal tip geometry is irrelevant for a through-all bore.

## Feature routing

The generic route uses five features: base sketch, base Extrude, circle sketch, through cut, and counterbore cut. The semantic route uses three: base sketch, base Extrude, and one counterbore Hole feature.

Prefer the Hole feature here because it preserves hole type, diameters, depth, extent, drill point, edit behavior, and drawing-callout intent in one domain feature.

Do not generalize this into "shortest timeline wins." A shorter route is better only when it removes redundant fragmentation while preserving or improving semantic intent, editability, references, replay, and downstream handoff.

Use generic sketch geometry when a semantic feature cannot represent an explicitly required custom shape. Record why the generic route is necessary.

## Face-hosted profile rule

A sketch on a planar face can expose profiles for the host-face material in addition to newly drawn curves. Never assume profile count equals drawn-loop count. Select by geometry, loop structure, area, containment, and intended operation.

For the S05 concentric sketch:

- Smallest disk profile is the 0.500 inch through bore.
- Second-smallest annulus is the 1.000 inch counterbore ring.
- Largest rectangle-minus-circle profile is surrounding material and must not be cut.

## Native-context rule

Build face-hosted features while their component occurrence is at identity transform. Apply assembly, comparison, or display-only transforms after native features complete. A UI direction sign from a dated source does not override current API spatial context.

## Production stop conditions

Stop rather than infer when required units, feature location, datum scheme, tolerances, projection standard, material-dependent geometry, or conflicting views remain unresolved.

## Evaluation boundary

S05 may advance the curriculum but does not prove production readiness. Semantic release and production activation remain separately gated.
