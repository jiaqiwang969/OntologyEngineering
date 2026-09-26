---
name: cad-agent
description: Design, reconstruct and validate mechanical parts, assemblies and mechanisms with Siemens NX, native CAD evidence and MISUMI China purchased parts. Use for STEP/PRT/DWG/DXF, drawings, BOM, interfaces, kinematics, assembly planning and CAD-to-manufacturing handoff.
---

# CAD Agent

Use engineering requirements, mechanism behavior and evidence to lead CAD work.
**Default: Siemens NX via direct NXOpen/Journal; purchased parts: MISUMI China.**
CAD operation does not use MCP. Keep the mechanical method independent of the CAD
application. Changing software does not validate a mechanism or remove unknowns.
Fusion has been removed from the tool scope; it is not a fallback or opt-in executor.

This real directory, `ontology-engineering/skills/cad-agent`, is canonical. Edit
operational references and scripts here; never restore it wholesale from a legacy
checkout. Parent [ontology-engineering](../../SKILL.md) owns semantic engagement;
source-locked Semantica alone executes formal CQ, SHACL, rules and project review.
Local ledgers, validators and geometry checks are evidence, not a second ontology
engine or manufacturing approval. See [contribution rules](CONTRIBUTING.md).

## Start with the engineering task

1. Identify exact project/configuration, source revision, current artifact and
   intended claim. Read the project's current state and instructions. Separate
   observed facts, conditional assumptions and unknowns; do not invent critical
   dimensions, materials, loads, tolerances or acceptance limits.
2. Use the [general engineering workflow](references/cad-engineering-workflow.md)
   to establish definitions, occurrences, functional features, datum frames,
   parameters/units, interfaces, constraints and measurable acceptance criteria.
3. For mechanisms, read [kinematics-first design](mechanism/OVERVIEW.md): inputs
   → topology/DOF/frames → actions and trajectories → envelopes/contact → interface
   convergence → detailed CAD → whole-machine checks → prototype validation.
   Exploratory later-stage drafts may run in parallel with unresolved inputs;
   stage closure requires its dependencies. CAD and animation consume the same
   parameter, frame, parent/child and trajectory source.
4. Select only the relevant routes below. Inspect current capability and evidence
   before executing the next bounded stage. One writer per task artifact; no
   duplicate insertion or blind retry after uncertain completion.
5. Read back native geometry, constraints and persistence; recheck every affected
   downstream interface/claim after a revision. Save a named checkpoint and a
   concise record of PRE/POST evidence, decisions, failures, recovery and unknowns.
   Continue to the requested deliverable, without imposing release paperwork on
   ordinary reversible design work.

## Routes and required methods

| Task | Read and use |
|---|---|
| Create, edit, inspect, import or export native CAD | [NX execution](references/nx-execution.md); `scripts/nx_direct.py` |
| Purchased parts, exact supplier CAD and BOM linkage | Start with the applicable [acquisition experience](references/supplier-acquisition-experience.md), then [supplier acquisition](references/supplier-cad-acquisition.md) and [MISUMI China guide](references/misumi-cn-guide.md) |
| Photos, videos, patents, sparse dimensions or hidden mechanism inference | [Evidence-driven reconstruction](references/evidence-driven-reconstruction.md); keep visible facts separate from alternative hypotheses |
| Shape-generating rules, field/lattice geometry or inverse controls | [Computational geometry](references/computational-geometry.md), [inverse parameter discovery](references/inverse-shape-parameter-discovery.md) |
| Complete source-to-native assembly authoring | [A0–A8 authoring](references/source-to-native-assembly-delivery.md), v3 template and `assembly/scripts/validate_authoring_manifest.py` |
| Assembly, interference, mating, insertion, tooling and contact | [Assembly/physics kernel](references/assembly-and-physics-kernel.md), [native assembly invariants](references/native-assembly-invariants.md), [S0–S11](assembly/SKILL.md) |
| Contact-driven transfer, flexible parts, threaded contact or physical release | [Algorithm/physics route](assembly/references/algorithm-physics-routing.md), its contract and validator; use the appropriate solver and physical evidence |
| CAD-to-process/inspection/cost decision | [CAD/process handoff](references/cad-process-integration.md), `scripts/cad_evidence.py`, `scripts/cad_process_handoff.py`, [evidence methods](../engineering-evidence-methods/SKILL.md) |
| Native variants and drawing review/export | [NX variant/drawing lessons](references/nx-variant-drawing-review.md); use direct journals, not its retired bridge instructions |
| Manufacturing/release depth requested | [P3 workflow](references/p3-release-workflow.md), [engineering gates](references/p3-engineering-gates.md), design brief and verification templates |
| Reuse, anomaly diagnosis and learning | [Workflow memory](references/workflow-experience-memory.md), [process-memory authority](references/process-memory-authority.md); retrieve by engineering intent and applicability, never inherit historical PASS |

For every interface bind both participants, ownership, boundary features, local
frames, fits/contact, neutral/current coordinates and limits independently. Cover
all modules and every relevant intermediate state: service motion, assembly,
changeover, homing, maintenance and recovery. Include fasteners, hoses, connectors,
tools, sensors/views, guards and flexible tails. Contact exceptions apply only to
specified boundary regions and states, never an entire body pair. Keep the support,
load or vacuum-holding chain closed through every transfer; prove the next support
before releasing the previous one. These obligations apply in NX unchanged.

Native bounds, visual alignment, no overlap, a constraint call and an animation
have distinct claim limits. Check exact selected interface faces and complete
solid-pair scope where needed. Kinematics does not establish stress, suction hold,
wrinkling, fatigue or registration accuracy. An error budget must name its sources,
reference state, units, sensitivities, bias/repeatability and correlation assumptions;
use worst-case sums or justified probabilistic propagation, and include model and
measurement uncertainty. Unknown inputs prevent a final compliance claim, not
independent exploratory calculations. Verify against the project's actual limit;
no project-specific tolerance is a universal default.

## Native execution and evidence

```bash
bash setup.sh
bash doctor.sh --json                         # local, no CAD/network call
python3 scripts/semantic_query.py capabilities
python3 scripts/nx_direct.py probe --profile /private/nx-profile.json
```

The local runner needs Python standard library. NXOpen runs in the installed,
licensed NX interpreter; never install an unrelated `NXOpen` package from pip.
Configure transport/paths outside this shared skill; in the studio use `fleet` for
live host identity and health. An executable-present probe is not a license or API
test. Direct journals run in isolated batch processes and new task directories;
no desktop focus change, service restart or attachment to somebody else's session.
The runner is not a sandbox: review custom journals and their write targets.

Before reporting a native result: exact model/configuration identity → native
objects/parameters/units → update/constraint/geometry checks appropriate to the
claim → save to task-owned files → close and reopen in a fresh process → second
readback plus file hashes. Journal success alone is not persistence or engineering
acceptance. NX tags and imported face numbers are session/revision-scoped; bind
stable attributes, prototype identity, component paths and geometric fingerprints.

A0–A8 v3 uses `native_*` identities and `PERSISTED_NATIVE_ASSEMBLY`. The explicit v2
compatibility reader retains the old strict checks for historical ledgers; it does
not invoke Fusion. NX readbacks additionally bind `.prt` hashes and distinct native
sessions. A manifest PASS establishes ledger integrity, not the truth of a native
or physical observation. Missing required evidence remains HOLD/UNKNOWN.

## Learning and migration boundary

Read [method preservation and NX mapping](references/nx-method-migration.md) when
transferring a historical lesson or assessing migration coverage. All S01–S27,
assembly, mechanism, contact-physics and evidence methods remain available. Fusion
commands, Joint enums, cloud identities and UI sequences are not NX commands.
Probe version-specific equivalents in isolated artifacts and retain unsupported
fields as unknown. A Fusion tutorial can supply design reasoning; it cannot earn
NX UI evidence, and NX reproduction cannot claim Fusion UI credit.

[The previous entrypoint](references/legacy-20260925/ARCHIVED_ENTRYPOINT.md) and old
software instructions are historical material, not discoverable skills or execution
routes. Fusion adapters and its runtime were archived outside the active skill tree
and removed; no environment variable re-enables them. McMaster material remains
historical supplier evidence. `data/execution-policy.json` is an operational routing
policy only; historical registry and experience snapshots grant no execution right.
Formal learning/promotion still uses parent Semantica. Ordinary practice records a
real reusable delta or stops with its checkpoint; it does not invent a release.
At corrections, repeated failures or useful task checkpoints, follow the parent's
[practice consolidation](../../references/practice-consolidation.md). Bind evidence,
review Jev's learning candidates, and verify that later tasks consume the relevant
guidance text; a saved note alone is not an observed change in behavior.
