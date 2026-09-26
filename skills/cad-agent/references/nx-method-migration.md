# Mechanical method preservation and NX mapping

This migration changes the default CAD execution and supplier paths, not the
engineering obligations. The full pre-change module was archived before editing;
replaced entrypoints are also available under `legacy-20260925/`. Native NX
operation coverage grows through measured, version-bound cases; it is not inferred
from the existence of NXOpen or old Fusion PASS results.

| Retained method/source | NX realization and verification |
|---|---|
| Requirements and functional decomposition; `cad-engineering-workflow.md`, P3 brief/gates | Same inputs, source/status/units and acceptance limits; NX attributes/expressions represent them without creating new confirmed facts |
| `mechanism/OVERVIEW.md`, single kinematic solver and trajectory contract | NX parts/assemblies and animation consume the same frames/parameters/trajectories; DOF/closure/path evidence stays separate from native positioning |
| Source/image/video/patent reconstruction and inverse parameter discovery | Native features are hypotheses bound to observations; retain alternative hidden structures and uncertainty |
| Sketch constraints, parameter dependency, reference/dimension authority; S01–S27 | NX sketches, expressions, datums, features and update diagnostics; each software API equivalent is locally tested, never mechanically rename enums |
| Part definitions vs occurrences, A0–A8 source closure, BOM/fasteners/harness completeness | NX prototype parts and component paths; native v3 occurrence reconciliation with unchanged strict v2 accounting core |
| Joint interfaces/frames, alignment/current/limits, symmetry and rotation phase | NX positioning/kinematic constraints or explicit motion contracts; separately verify entities, frames, health, residual DOF and behavior |
| `native-assembly-invariants.md` and `assembly-and-physics-kernel.md` | Complete scope, exact interface boundary evidence, native body pair closure; native bbox is only an envelope statistic |
| S0–S11, insertion/tooling/maintenance paths and flexible envelopes | Retain world transforms, sequence, all relevant intermediate states and load/support handoffs; do not infer path feasibility from endpoint clearance |
| Algorithm/physics routing, libuipc/Abaqus or other suitable solvers | Same geometry/configuration, material/contact/load assumptions, causal/control comparisons, numerical error and physical validation requirements |
| Precision and error estimation | Reference-state dimensional chain, bias/repeatability, sensitivity/correlation and thermal/load/control/measurement contributions; project tolerance and test evidence remain required |
| Computational performance core plus mechanical integration shell | Keep generation authority/units/frames/tolerances; import B-Rep into NX and recheck interfaces after regeneration |
| NX width/length/drawing lessons | Retain native API evidence, owner-local transforms, annotation/parts-list review and export completeness; discard their MCP orchestration |
| Exact purchased-part identity and procurement/install quantities | MISUMI full configuration → actual downloaded CAD → NX save/reopen → BOM and dated quantity-specific supply record |
| `cad_evidence.py`, `cad_process_handoff.py`, evidence methods | Same source hashes, JSON pointers, model/configuration identities, claims and downstream impact; only Semantica performs formal semantic review |
| `workflow-experience-memory.md`, `process-memory-authority.md` | Keep PRE/POST, applicability, failures, recovery, validation and revision closure; historical executable profiles do not activate NX or the supplier path |
| Manufacturing handoff and release | Retain drawings/GD&T, materials/process, inspection, native checks, simulation and prototype evidence; no automatic release from a plausible CAD model |

## Software-specific items that do not transfer automatically

Fusion Joint/As-Built Joint, cloud project/folder/lineage IDs, timeline commands,
MCP tool names and UI lesson credits remain historical implementation details.
In NX bind PRT identity/hashes, prototype/component ownership, semantic attributes,
NX constraints and fresh-process readback. A positioned pair has no proved mate or
motion freedom until that relation is established and checked. Exported STEP does
not preserve every native feature, assembly constraint or annotation.

The current v3 ledger validates structure, provenance references and file hashes;
it is not a port of the old profiled semantic engine. Required but unsupported NX
measurements remain UNKNOWN and go to a version-local probe or formal parent
Semantica package. Do not substitute manifest fields for the missing measurement.

## Acceptance scope

Use the task's migration acceptance record for exact tested versions, inputs,
receipts and limits. Required checks for this reorganization are default-route
selection, mechanical-method preservation, legacy accounting/contact regressions,
v3 negative cases, direct NX create/edit/save/fresh-readback and supplier static
file intake. No broad NX API, CAM, simulation, arbitrary drafting or unattended
MISUMI parameterized 3D claim follows from these bounded tests.
