# S10: component interface assembly

## Scope

Use this reference for multi-component CAD work involving in-context sketches, projected or reconstructed interfaces, subtractive features near other components, fasteners, grounding, joints, supplier parts, or assembly-position provenance.

## Core rule

Assembly geometry is valid only when four layers agree:

1. Feature ownership: every sketch, feature, and body belongs to the intended component.
2. Interface contract: shared axes, planes, diameter stacks, seats, and engagement values agree across components.
3. Assembly constraints: grounding and joints encode persistent position.
4. Provenance: projections, fallbacks, supplier geometry, and proxies are disclosed.

A plausible image or healthy BRep is insufficient.

## Required planning sequence

1. Declare the assembly components and the owner of every planned feature.
2. Build an interface ledger before modeling either side of a cross-component interface.
3. Record each dependency as `source component -> target component`.
4. Apply the S09 interaction-topology rule to projected or reconstructed profiles.
5. For every Cut or Intersect in a multi-component design, set the participant body explicitly.
6. Separate semantic intent from BRep implementation; name and disclose any fallback.
7. Ground one reference occurrence and use persistent joints or constraints for the rest.
8. Independently reacquire ownership, accepted and rejected dimensions, axes, occurrence transforms, body counts, and constraints.

## Fastener-interface ledger

Treat a fastener axis as one contract spanning all participants:

- Threaded component: pilot diameter, thread standard and class, thread depth.
- Clearance component: clearance diameter, counterbore or countersink dimensions, material stack.
- Fastener: nominal thread, length, head envelope, seating elevation, engagement.
- Assembly: common axis coordinates and persistent constraint ownership.

Do not validate these as unrelated local dimensions.

ThreadRepresentationChoice: record modeled/unmodeled for each threaded hole and justify it by the downstream consumer (3D printing needs helical solids, so modeled; otherwise unmodeled keeps a clean cylindrical face).

## In-context dependency policy

Exact projection is preferred when it is stable and source references are explicit. A deterministic interface-ledger reconstruction is acceptable when the execution tool cannot safely project, but it must disclose that it is a reconstruction and prove axis equivalence by independent readback.

Component activation is an ownership boundary. Do not assume that creating geometry while another occurrence is visible assigns the geometry correctly.

## Subtractive-feature guard

Fusion `ExtrudeFeatureInput.participantBodies` defaults to all intersected bodies. Therefore:

- Never leave participant selection implicit for Cut or Intersect in a multi-component design.
- Require the intended body or bodies in the approved plan.
- After execution, verify `parentComponent` and search for both required and forbidden cylindrical radii in every nearby component.
- Treat a feature reported under the wrong component as a hard failure even if the assembly looks correct.

## Native-feature fallback

S10 observed repeated `HoleFeatures.add InternalValidationError logicalSelection` for a four-point LID counterbore. The accepted local fallback used:

- One LID-only through Cut at diameter 0.201 inch.
- One LID-only blind Cut at diameter 0.375 inch and depth 0.190 inch.
- Attributes preserving the original `#10 normal clearance counterbore` intent.
- Explicit `LID_BODY` participation.

This is a semantic-preserving fallback, not permission to silently replace every Hole feature with raw cuts. Thread definitions, manufacturing annotations, and feature semantics must remain explicit.

## S10 failure evidence

The first through-all fallback omitted `participantBodies`. Fusion cut both LID and BOX, enlarged the BOX 0.1505-inch pilot holes to 0.201 inch, and indexed the named clearance feature under BOX. The assembly remained visually plausible.

Independent readback rejected it because:

- BOX radius 0.07525 inch disappeared.
- Rejected BOX radius 0.1005 inch appeared.
- The LID clearance feature was owned by BOX.

After deleting the cross-component feature and recreating it with `participantBodies = [LID_BODY]`, all ownership and radius gates passed.

## Supplier geometry policy

A catalog identity does not prove geometric provenance. Record supplier, catalog identifier, nominal dimensions, acquisition method, and one of:

- authentic supplier geometry, with source evidence;
- deterministic envelope proxy, explicitly marked as not supplier geometry.

S10 uses a McMaster-Carr `92610A247` deterministic envelope proxy. Its major-diameter shank overlaps the semantic pilot geometry because helical threads are not modeled; this expected proxy overlap is disclosed rather than misclassified as an accidental collision.

The `92610A247` deterministic envelope proxy above describes the historical attempt-0001 state. In attempt-0002 (2026-08-01/02) the authentic 92610A247 STEP was acquired (head radius 0.15625 in), so the proxy wording is historical status, not the current state.

## Evolution status

This rule is active as an S10 CAD Agent reference and remains an L1 ontology candidate.

Promote Fusion MCP changes only after an isolated fixture:

- reproduces the multi-point HoleFeature `logicalSelection` failure;
- reproduces implicit cross-component Cut participation;
- proves an explicit-participant semantic fallback and post-write ownership gate.

S11 must load this reference before Fusion planning. Semantic release remains a separate gate.
