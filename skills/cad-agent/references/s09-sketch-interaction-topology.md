# S09: sketch interaction topology

## Scope

Use this reference when choosing sketch boundaries for primitives whose intersections create profiles consumed by later Extrude, Cut, Hole, Rib, Revolve, or similar features.

## Core rule

Do not optimize sketch count directly. Preserve the **interaction topology** that generates required downstream profile partitions.

A shared sketch is preferred when primitive intersections define the exact regions needed later. Separate sketches are acceptable only when projection, explicit cutting, or another named operation reconstructs those interactions and semantic witnesses prove equivalence.

## Required planning sequence

1. List every profile that downstream features will consume.
2. Map each profile to the primitives and intersections that generate it.
3. Keep that primitive set interaction-closed, or name the operation that reconstructs every lost partition.
4. Treat selected Join profiles as net-new material deltas.
5. Define material and negative-space witnesses at each affected depth interval.
6. Independently read back containment, volume, topology, and feature ownership.
7. Use sketch and feature counts only as secondary maintainability signals.

## S09 controlled evidence

The same dimensions, Hole definition, fillets, and four modeling operations were used in both methods.

| Observation | Shared interaction sketch | Three isolated sketches |
|---|---:|---:|
| Sketches | 1 | 3 |
| Modeling elements | 5 | 7 |
| Faces | 17 | 20 |
| Edges | 37 | 45 |
| Volume | 73.8074353858 cm3 | 76.8490657887 cm3 |
| Main-bore cylindrical area | 27.8689113504 cm2 | 24.3852974316 cm2 |
| Front-quarter bore witness | outside, correct | inside, incorrect |

The control's 3.041630402848 cm3 excess volume agrees with the analytical volume of the accidentally filled inner-bore quarter. Both models were BRep-healthy and exposed the same nominal cylindrical radii, so health and dimensions alone were insufficient.

The recommended lug Join consumed only the rectangle-outside-circle profile, area 2.7728153697 in2. It did not redundantly select annular material already created by the one-inch ring extrusion.

## Fillet policy

Fillet placement is contextual:

- Use downstream fillet features when they improve dimension clarity, edge-set ownership, and editability.
- Keep fillets in a sketch when they are essential to the generating profile or downstream references require that geometry.
- Never turn either choice into a universal rule.

## Failure classification

When a semantic witness fails, classify the cause before revising anything:

- Perception: the source geometry or UI action was misread.
- Interaction mapping: a required primitive intersection was omitted.
- Profile selection: the wrong region or redundant region was selected.
- Execution: Fusion created something different from the approved plan.
- Readback: the witness, coordinate frame, or expected containment was wrong.

Preserve the failed attempt and create a new attempt directory.

## Evolution status

This rule is a CAD Agent candidate, not yet a global ontology or tool-core change. S08 and S09 use the same part and therefore do not constitute independent recurrence.

Promote it when either condition is met:

- A geometrically independent lesson reproduces the same failure class.
- A dedicated regression fixture proves the rule for both additive and subtractive workflows.

S10 must load this reference before Fusion planning. Semantic release remains a separate gate.
