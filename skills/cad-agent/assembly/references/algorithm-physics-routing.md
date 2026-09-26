# Assembly algorithm and physics routing

## Contents

1. Authority and purpose
2. Integrated route
3. Paper-derived methods
4. Planning admission invariants
5. Physics route selection
6. S9 directional probe
7. P-level contact physics
8. Threaded-contact route
9. S11 physical falsification
10. Claims and transfer limits
11. Evidence lineage

## Authority and purpose

Use this reference to choose and connect assembly algorithms. It is reusable
method memory, not evidence that a target assembly, path, force, fixture, or
physical process passed. Keep occurrence identity, connection-tree closure and
world transforms upstream of every planner or solver.

Persist one `cad-agent.assembly-algorithm-physics-contract/v1` per operation or
explicitly declared rigid subassembly. From the `assembly/` skill directory,
start from `assets/algorithm-physics-contract.template.json` and run:

```bash
python3 scripts/validate_algorithm_physics_contract.py /path/to/contract.json
```

The validator checks routing and overclaim rules. It does not execute CAD,
collision, libuipc, Factory, a fixture trial, or Semantica.

## Integrated route

```text
occurrence identity + BOM + interfaces + connection tree
  -> mechanism recognition + rigid/flexible partition
  -> controlled process order, or paper-derived sequence/direction candidate
  -> ATA / closed-form / controlled path candidate
  -> S7 exact or explicitly proxy continuous-geometry gate
  -> S9 fair short-prefix direction probe, only when directions remain ambiguous
  -> P-level contact physics, only when the requested claim needs it
  -> S8 tool access and S11 fixture/minimal physical falsification
  -> S10 instruction or animation from certified states only
```

The route contains conditional branches, not a rule that every screw or every
rigid insertion must run every solver. A downstream branch inherits all
upstream holds that affect its claim.

## Paper-derived methods

| Method | Portable contribution | Required boundary |
|---|---|---|
| Aameri, Cheong and Beck assembly ontology | Proper parts, boundary entities, incidence, mating features, connections and relative configuration | Does not supply Fusion Joint frames, current motion state, limits or executable paths |
| Agrawala et al. 2003 | Action graph, exploded guidance and stepwise presentation; planning and presentation must agree | Presentation cannot invent missing parts, relations, order or motion |
| Li et al. 2008 | Directed-acyclic partial order, candidate direction and iterative removal of an unblocked short-escape part | Exploded-layout order is a candidate, not shop-floor process truth |
| Assemble Them All, SIGGRAPH Asia 2022 | Physics-based disassembly candidate generation and assembly by exact reverse state order | Planner `Success` is not an independent continuous-clearance or real-physics certificate |
| ASAP, ICRA 2024 | Sequence-level gravity, support and finite-hold stability | Use only when support/holding stability is modeled; geometric path feasibility alone is insufficient |
| SBDP, AAAI 2025 | State memory, direction-blocking-graph prior, translation before rotation, and fixed-set update after removal | No official code is pinned locally; call local work `SBDP_DBG_INSPIRED_NOT_FULL_SBDP_IMPLEMENTATION`; never transfer paper benchmark rates |
| IPC/libuipc | Barrier-contact simulation for rigid/deformable contact, friction and self-contact | It is not a global assembly planner and does not replace exact geometry or a physical fixture trial |
| Factory, RSS 2022 / Isaac | Dedicated nut-bolt and threaded-contact simulation direction | Current assembly bundle has no project-qualified threaded solver; keep the route held until separately validated |

Project extensions include STEP/XCAF occurrence identity, hash-bound world
poses, exact B-Rep or declared proxy gates, allowed-interface policy, solver
telemetry, guarded Fusion persistence and saved-file readback. Do not attribute
these extensions to a paper.

## Planning admission invariants

Apply all twelve invariants to a paper-derived trajectory:

1. Bind an explicit assembled `q0` to the authoritative final product pose.
2. Pin geometry identity, mover/fixed identities, units, frames and consumed
   source hashes.
3. Move one rigid occurrence or one explicitly declared rigid subassembly per
   transition; partition deformables separately.
4. Persist ordered absolute states, state count and a canonical state digest.
5. Check `q0`, every admitted state and every path edge against the same declared
   mover/fixed model. Label sampled evidence as sampled.
6. Persist the disassembled terminal goal. A convex-hull or other proxy goal
   remains a proxy.
7. Form assembly by reversing state order. Never negate absolute translations,
   Euler angles or transforms.
8. Add a separately verified connector path when the real incoming pose differs
   from the reversed disassembly terminal.
9. For an SBDP reproduction claim, prove translation exhaustion before rotation;
   otherwise label the path as an external or inspired candidate.
10. At every real fastening, release or removal boundary, update the moving and
    fixed partitions instead of rigidly moving future stages together.
11. Reopen the saved source scene and compare every admitted state before
    requesting a render.
12. Persist claim caps. Paper statistics, direction probes, proxies and deferred
    deformables never become target-project success by wording.

Treat an invalid initial state, missing ordered states, mover/fixed mismatch,
unbound connector, flexible parent moved as one rigid body, or canonical-hash
drift as a rejection or hold. A legacy movie is not a state ledger.

## Physics route selection

Choose routes from the operation's claim and mechanism, not tool availability:

| Condition | Required route |
|---|---|
| One geometry-certified rigid path; no force, flexibility, stability or physical-release claim | S7 geometry may be sufficient; record `NOT_REQUIRED` physics rationale |
| Two or more unresolved local approach directions | S9 directional probe after candidates share one frozen comparison basis |
| Flexible/contact-driven member, cable pinch, press fit, elastic snap or guide passage affects the claim | P-level contact physics after S7 readiness |
| Motor/contact force, torque or capacity is claimed | P-level calibrated capacity branch with convergence; a motion servo is insufficient |
| Thread engagement, nut-bolt insertion or screw driving is claimed | Dedicated threaded-contact route; generic libuipc direction probing is insufficient |
| Real buildability or physical release is claimed | S11 fixture and minimal physical falsification, irrespective of simulation results |

Unknown routing facts cause `HOLD`; they do not default to `NOT_REQUIRED`.

## S9 directional probe

Use S9 only to rank local directions. Freeze and pre-register:

- at least two candidates;
- the same assembled initial state and static environment;
- the same proxy geometry, units, material/contact parameters, loads and solver
  schedule;
- the first direction-discriminating feature and common stopping rule;
- matched controls, repeat/noise policy, and the independent variable;
- tie, conflict and no-feature dispositions.

Return only `PROMISING_DIRECTION`, `GUIDED_CONTACT_DIRECTION`,
`RESISTED_DIRECTION` or `AMBIGUOUS_DIRECTION`, plus a stop action. A single
candidate or a zero-contact prefix cannot by itself become promising. Preserve:

```text
DirectionalProbeQualified != CompletePathQualified
PromisingDirection != ApproachPathAccepted
SolverResistanceSignature != ExactGeometricInterference
```

No numerical advance means `INCONCLUSIVE_NO_NUMERICAL_EXECUTION`. A frozen
world-fixed environment is not a physical fixture. The historical B14 C0/D1/D2
runs are useful evidence that features can be distinguished, but their later
fair-comparison audit remains blocking; do not copy the old D2 label as a target
verdict.

## P-level contact physics

Enter this branch only after occurrence identities, world poses, body roles,
units, interfaces, geometry roles and the S7 readiness interval are closed.
Use a project-specific solver contract. For a contact-driven flexible strip,
also load the separately installed `design-simulate-contact-driven-strip`
skill; do not transfer its geometry or numeric parameters to another mechanism.

Enforce these general invariants:

- Keep authoritative CAD/render geometry, error-bounded collision proxies and
  feature-preserving solver meshes distinct.
- Transform geometry exactly once from hash-bound world poses.
- Declare fixed, revolute/rigid and deformable roles; enable only intended
  contact pairs.
- Never prescribe the claimed feed motion directly to a deformable member.
  Motion must emerge through the modeled contact and constraints.
- Declare SI units, gravity, materials, friction, contact distance, timestep,
  loads and drive law with `given/derived/assumed/recommended/unknown`
  provenance.
- Run geometry/placement, static initialization, 2-step smoke, 10-step smoke,
  loaded contact canary, guide/passage canary, no-drive or stationary control,
  and only then a full continuous solve when required.
- Verify full pair action-reaction before using sparse display samples.
- Recover force from the installed solver's documented convention. For the
  currently validated libuipc IPC telemetry pattern:

  ```text
  force = -contact_gradient / dt^2
  torque = axis dot sum((point - axis_origin) cross force)
  ```

- Report contact/motor work separately from gravity or external-load work.
- Use matched counterfactuals; change only the declared independent variable.
- Sweep mesh/proxy error, timestep/contact distance, friction, material, load
  and drive rate before calibrated force or motor-capacity claims.

Choose one drive meaning:

- `MOTION_DRIVEN_CONTACT_VALIDATION`: prescribe an absolute angle trajectory
  with finite tracking strength; report tracking and recovered contact torque.
  Do not call servo strength a motor torque limit.
- `TORQUE_LIMITED_MOTOR_VALIDATION`: model inertia, torque-speed/current curve,
  losses, controller, saturation, stall, slip and backdrive; let speed and
  travel emerge.

Use the monotonic core ladder `GEOMETRY_VERIFIED -> DRIVER_VALIDATED ->
ASSEMBLY_VALIDATED -> SOLVER_QUALIFIED -> CONTACT_TRANSFER_VALIDATED ->
PASSAGE_VALIDATED`. Keep endpoint, motor capacity and media as independent
branches. Visual overlap never proves mechanical reclosure; require signed
feature relations, a feature-preserving mesh, calibrated capture tolerance and
retention/pull-out evidence.

## Threaded-contact route

Route screw/nut engagement, thread following, seating torque and cross-thread
claims to a dedicated threaded model such as a separately qualified Factory /
Isaac environment or a calibrated alternative. Until geometry, friction,
controller, torque, thread state and target hardware have project evidence,
return `HOLD_THREADED_CONTACT_MODEL_NOT_QUALIFIED`. Do not use a coaxial path,
rigid-body animation or generic libuipc short probe as a substitute.

## S11 physical falsification

Simulation never supplies physical release. For a physical claim, calibrate
fixture-to-product coordinates, repeat after reclamping, make completion
machine-observable, verify load transfer, release clamps in order, and prove
fixture/tool withdrawal without collision. Execute the shortest high-risk real
action that can falsify the candidate, with stop conditions and exact evidence.

## Claims and transfer limits

Persist separate claim states for ordered planning, geometry path, direction
ranking, contact transfer, passage, motor capacity, endpoint reclosure,
threaded contact, media and physical release. A stronger result in one branch
does not upgrade another branch.

Portable: route selection, identity/state contracts, counterfactual discipline,
claim separation, solver staging and fail-closed states.

Not portable: case verdicts, thresholds, friction, density, stiffness, load,
timestep, contact distance, servo strength, fixture pose, mesh resolution,
capture tolerance, tool envelope or runtime duration.

## Evidence lineage

The method synthesis used these source-locked records. Paths are provenance
labels; the installed skill must remain usable when historical project trees
are absent.

| Record | SHA-256 | Use |
|---|---|---|
| Orbita academic review, 2026-08-20 | `e389f0725f1d88f16e975f387cd34ba050804bdb9ebc5fed5cd63c293e5786c1` | ATA, ASAP, SBDP, Factory, Agrawala and Li routing |
| Paper algorithm benchmark current bytes | `467e620c2266c0c44844291bae34277d713b2ba1a283e8c042d7cf8aa27e09e9` | PAB rules plus a retained canonical-hash-drift regression; do not copy its current admission booleans |
| Hybrid-fidelity method | `2452e55524f29ac1d9010894af95455360b5633759cf31bb6a9c22d9a556845b` | S9 direction-only semantics |
| Directional comparison audit | `3db6fdc4e8ba1f8a464eb6597fa906074186dc778f62753ceba6a16ade09dbba` | Fair-comparison blocker |
| B14 direction decision | `a23a9dc8eef1d4afe27b534ee9f8ace64dc9d9da1ce9d87fb7572e5865214650` | Historical C0/D1/D2 evidence only |
| Y-Zipper v079 governing record | `21633b2ab3298fc0c5c7567208eb4eefea30dbb8fe5bf808b8e6555052cda007` | Full contact-transfer workflow |
| Y-Zipper formal solve report | `b7f314552a08eef51d9f6f334255e35e532e344f07211805c48cdc0ffa584519` | Contact, passage and action-reaction evidence |
| Y-Zipper reclosure report | `8333d4a7df3b57849037e45e5a9678becb4543aa703ed846b5e121e14d9d4d98` | Geometric reclosure is not mechanical relock |
| Contact-driven-strip skill snapshot | `9b96a9d355d73557819556b3c8a46f191f73ac0249dd8f8014b096ab17cc5a2f` | Operational authority split, CP staging and counterfactual contract |
| Aameri-derived CAD Agent reference | `848eced6a98ece4cdf13137c26e2004e61c82e54d24241e6ab28cf88fb9f32c5` | Static assembly semantics and CAD extensions |

The historical paper benchmark declares canonical digest
`142549acdc43b770804aa2d0a984f959000a8d3f6196bb654073fd6e1b5bdffa`,
but its current bytes recompute to a different canonical digest because four
admission booleans drifted. Keep this as a negative regression: canonical-hash
failure blocks reuse even when the surrounding prose looks correct.
