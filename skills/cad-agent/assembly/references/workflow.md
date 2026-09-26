# S0–S11 assembly workflow

The stages are gates, not a checklist to mark green. A downstream stage inherits every unresolved upstream hold that affects its claim.

## S0 — Freeze sources and scope

Hash the STEP and every controlled record. Freeze schema, units, frames, transform convention, name encoding, target claims, release intent and mutation permissions. Create the project manifest and evidence ledger. No source anchor means no S1.

## S1 — Bind identity and world pose

Parse the product/occurrence tree. Preserve definition identity separately from occurrence identity. Decode names without destroying raw bytes. Compute each leaf world transform, then cross-check transforms through an independent parser or invariant. Produce `identity-evidence.json` with unique, missing and ambiguous bindings plus exact evidence paths. Multiple candidates or unverified transform direction cause `HOLD`. Determine the exporter's representation-relationship and item direction (which representation is the parent, which item carries the pose) per edge and record the per-direction counts; the SolidWorks convention is not a default, and an AP242 file written by ST-Developer/Fusion is child-first (2380/2380 edges in the 2026-09-02 OLSK case). Origin distribution is not a plausibility check for in-place-modelled components; only an independent XCAF cross-check is.

## S2 — Build interface evidence

Extract analytic cylinders, planes and cones from each unique definition once, transform them per occurrence and form interface candidates using axis angle, line offset and axial overlap. Detect key collisions before joining data. Produce `joint-port-evidence.json`, `global-interface-graph.json` and `operation-classification.json`. Do not treat proximity alone as mating intent.

## S3 — Build the formal identity chain

Connect controlled operation roles to BOM identities, CAD definitions, occurrences and geometric ports. Use normalized aliases, specification tuples and context only under explicit deterministic rules. Keep every unresolved row. Produce `formal-identity-binding.json`; never silently force a one-to-many match.

### S3.5 — Functional-attribute handbook and sequence audit (2026-09-05)

Before certifying or filming a sequence, split the machine into parallel analysis tracks (structure, axes, head,
bed/tooling, electrics, pneumatics, enclosure, purchased families) and let each track write `function.json`,
`order_constraints.json`, `notes.md`, `sources.md` per `tools/function_tracks/SCHEMA.md`: what each unit does,
how it works, how it is installed (fasteners, direction, tools, adjustment after mounting) and which
MUST_BEFORE / MUST_AFTER / SAME_BENCH / ACCESS / ADJUST_AFTER / WIRE_BEFORE_CLOSE / SAFETY constraints follow,
each with source (our data / official / inference) and confidence. Slice inputs with
`tools/build_function_track_inputs.py`; audit the chapter order and the frozen play order with
`tools/audit_sequence_constraints.py` (intra-operation constraints match part names against S3 products and
manual mesh names); build the PDF with `tools/function_tracks/build_function_manual.py`. Feed carrier parts
back as seeds for `tools/build_play_order_functional.py` and bench groups into `tools/walk_unit_joins.py` +
`tools/materialize_unit_joins.py`. Gate: no high-confidence violation before delivery (K-PATH-20, K-DEL-06).

The same handbook feeds three deliverables that do not need any animation: the assembly process regulation
(`tools/function_tracks/build_sop_regulation.py`: per-step carrier order, parts, fasteners with tool sizes and
reference torques, adjustments, prerequisites, remarks, open questions), the complete BOM with procurement and
machining lists (`tools/function_tracks/build_complete_bom.py`: manual vs BOM vs STEP, four procurement classes,
gap tables) and the upstream issue list (deduplicated open questions as bilingual GitHub issue drafts with evidence).

## S4 — Recognize mechanism semantics

Before generic path search, test whether the target is a known mechanism or closed chain. Solve the mechanism with its invariants and self-checks. Searching a free-body path for a constrained mechanism is a category error.

## S5 — Form solver units and motion partitions

Collapse leaves that move together: purchased modules, bearing stacks and rigid subassemblies. Partition rigid core, flexible elements and terminals. Record leaf-to-unit membership and one authoritative world pose per unit. Wrong granularity invalidates subsequent direction and collision results.

## S6 — Determine direction with evidence levels

Use, in order: geometry-forced direction, validated class-specific rule, controlled annotation, then explicitly unresolved. Test both signs when the axis is known. Restrict class rules to their validated domain; a fastener head rule is not a housing rule. Output `direction_rule`, axis, sign, evidence level and competing candidates.

When a controlled order is absent, select a paper-derived planning route only
after identity, interfaces, connection-tree closure, mechanism recognition and
solver-unit partitioning. Read `algorithm-physics-routing.md` and persist an
algorithm/physics contract. Agrawala/Li action graphs, ATA, ASAP and a local
SBDP/DBG-inspired adapter answer different questions; none may be presented as
an interchangeable “assembly algorithm”.

## S7 — Certify path and sequence

Construct the true static set for the operation, including sequence state and always-present unresolved geometry. Separate baseline contact from newly introduced contact at region level. Verify the complete playable interval, not just the endpoint. Prefer continuous collision or conservative advancement; dense sampling stays labelled sampled. Report exact/proxy geometry, coverage interval, step/solver tolerances, blockers and counterfactual direction. Produce a trajectory contract and `state-chain.json`.

For every paper-derived path, bind assembled `q0`, mover/fixed sets, ordered
absolute states, state digest, terminal goal, visited states and failure reason.
Assembly reverses the ordered state list; it never negates absolute transforms.
Planner `Success` admits a candidate to this gate but cannot pass the gate.

## S8 — Verify tool access

Use the controlled tool identity and geometry. Check engagement, approach, working stroke and swept volume for each relevant grip mode. A bare axis ray or bit access is not wrench/screwdriver access. Report unsupported press tooling and unavailable tool CAD as holds.

## S9 — Run an optional physical-direction probe

Use only when geometry leaves multiple candidates. Apply the same proxy, material/contact parameters, initial state, load and stopping rule to every candidate. The probe may rank directions; it cannot release the process. Stop after the smallest discriminating action.

Pre-register the ten-facet fair-comparison contract from
`algorithm-physics-routing.md`. No discriminating feature, a tie, contradictory
repeats or a zero-contact prefix is `AMBIGUOUS`, not a winner. S9 has the claim
ceiling `DIRECTION_RANKING_ONLY`.

## Conditional P route — Validate full contact transfer when required

This is a branch between S7 readiness and S11, not a new numbered stage and not
an expanded meaning of S9. Trigger it for flexible/contact-driven members,
cable pinch, press fits, elastic snaps, guide passage, force/torque/capacity or
other claims that depend on contact physics. Keep CAD/render, collision proxy
and solver meshes distinct; forbid prescribed feed of the claimed deformable;
run staged canaries, matched counterfactuals and convergence; recover force and
torque using the pinned solver convention. A motion-driven angle servo may
validate contact transfer but cannot establish motor capacity. Threaded
engagement uses a separately qualified threaded-contact model.

## S10 — Express only certified motion

Build the instruction, animation or scene from S7/S8 contracts. Keep final assembled transforms frozen, make visibility/material semantics explicit and run a saved-file readback before rendering. Mark flexible, missing-geometry and uncovered operations as annotations; never invent their motion.

Then run the presentation gates before delivery (2026-09-03): derive scale parameters from the machine (K-PATH-19); per-group camera shots with explicit constant/linear F-curve interpolation (K-SCN-10/11); material, occlusion and layout calibration (K-SCN-12/13/14); the independent visual-scope penetration scan with PATH_OVERLAP = 0 and a classified seated-overlap ledger (K-GOV-15, K-DEL-05); and a segmented multi-agent frame review with zero severity-3 defects recorded in the acceptance packet (K-DEL-04). Penetration found here is re-planned along another certified axis, never merely trimmed into a pop-in (K-PATH-18).

## S11 — Fixture, acceptance and minimal falsification

Calibrate fixture-to-product coordinates, repeat after reclamping, execute machine-readable acceptance, verify load transfer, release clamps in sequence and prove fixture withdrawal. Record the minimal high-risk physical test and stop conditions. This is the physical-release boundary.

## Required run artifacts

A full process-lane engagement should leave, at minimum:

- `project-input.json` and `evidence-ledger.json`;
- `identity-evidence.json` and transform cross-check result;
- `joint-port-evidence.json` and `global-interface-graph.json`;
- `formal-identity-binding.json` and `occurrence-resolution.json`;
- direction decisions and unresolved candidates;
- algorithm/physics routing contracts, trajectory contracts, state chain and collision coverage summary;
- S9 comparison records or an evidence-backed `NOT_APPLICABLE` rationale;
- P-route solver/counterfactual/convergence records when a contact-dependent claim is requested;
- tool reachability summary;
- scene/source readback or instruction-package audit;
- S11 records when physical release is claimed;
- final claim ledger with `PASS`, `PASS_WITH_HOLDS`, `HOLD`, `UNKNOWN` or `BLOCKED` per claim.

Store artifacts under the target project, not inside this skill. Preserve superseded runs and record why a new one supersedes them.
