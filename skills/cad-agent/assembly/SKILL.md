---
name: assembly-ontology
description: "Analyze, reconstruct, author, validate, animate, and learn from mechanical assemblies through a bundled A0–A8 source-to-native authoring workflow and S0–S11 engineering ontology. Use for arbitrary .step/.stp/.p21 or source-native CAD intake; occurrence identity and world transforms; full BOM/fastener/harness completeness; native B-Rep versus mesh/proxy quality; NX assembly authoring and persistence; mating-interface graphs; paper-derived action-graph, ATA, ASAP, or SBDP-inspired planning; assembly or disassembly sequence and insertion direction; exact/proxy interference sweeps; libuipc contact physics, threaded-contact routing, tool access, physical falsification, animation evidence, fail-closed PASS/HOLD/UNKNOWN decisions; and transferring lessons from Orbita, B13/B14, 灵犀 X1, Poppy, or Fourier N1 to a new assembly."
---

# Assembly ontology

Turn supplied assembly evidence into bounded engineering claims and, when
requested, a complete native NX assembly. The bundled ontology is reusable
method memory; it is never evidence that a new model has passed.

## Coordinate skills and authority

- Use `ontology-engineering` at the beginning for semantic discovery and at the end for the fixed engineering/Semantica/learning report. A local skill snapshot is not a Semantica package, publication or approval.
- Use the current `cad-agent` skill for direct NXOpen/Journal execution and native checks. In this bundled module, read the parent at `../SKILL.md`; keep one writer per task artifact.
- Do not launch or mutate NX merely because it is available. Prefer read-only STEP/XCAF or computational-geometry work until a native-CAD action is necessary and authorized.
- Use the bundled query tool as advisory method memory before relying on memory. Historical T-fusion and MCP tool bindings in its snapshot are inactive; current executor selection comes from ../data/execution-policy.json. `VALIDATED` means validated on the recorded cases, not automatically on the target assembly.

## Start every engagement here

Resolve every `scripts/`, `references/` and `assets/` path below relative to this `SKILL.md`, regardless of the target project's current working directory.

1. Run `python3 scripts/validate_bundle.py` from this skill directory. Stop if the bundle fails.
2. Run `python3 scripts/query_ontology.py overview`, then query criteria/failures relevant to the request.
3. When sequence planning, direction ambiguity, contact transfer, force/torque,
   threaded contact, or physical release is in scope, read
   [algorithm-physics-routing.md](references/algorithm-physics-routing.md). Start
   from `assets/algorithm-physics-contract.template.json` and validate the
   project copy with `scripts/validate_algorithm_physics_contract.py`.
   For source-native brick/wooden-brick assemblies that also need bilateral
   symmetry, CAD-like three views, curved pieces with filled internal support,
   prefix stability, or iterative geometry optimization, read
   [brick-assembly-optimization.md](references/brick-assembly-optimization.md).
   Its counts and assumed dimensions are case evidence only, never defaults.
4. Read [input-contract.md](references/input-contract.md), choose the evidence lane and freeze a project manifest from `assets/project-input.template.json`.
5. Run `python3 scripts/preflight_step.py /path/to/model.step --json`, adding controlled `--bom` and `--sop` records when present.
6. Record source hashes and claim scope before geometry parsing. Store run artifacts in the target project, never in this skill.

The preflight is lexical only. Its `READY` result permits the next gate; it does not prove assembly structure, geometry, transforms, paths or process truth.

## Choose the honest lane

- **STEP only:** geometry intake and computed geometric candidates. Hold actual order, tooling, torque, adhesive, acceptance and physical release.
- **STEP + controlled BOM:** add occurrence identity only where the binding is unique and independently checked.
- **STEP + BOM + controlled SOP:** execute the process lane and bind operations, order, roles, tools, parameters and acceptance.
- **Physical release requested:** require S11 fixture, acceptance and minimal physical falsification evidence.
- **Single part or ambiguous multi-body file:** classify and route appropriately; do not manufacture an assembly tree.

Read [claim-boundaries.md](references/claim-boundaries.md) before issuing a verdict.

## Choose analysis or native-authoring work

- For an existing assembly that only needs identity, interface, sequence,
  interference, tooling or animation analysis, continue with S0-S11 below.
- For a new/repaired NX assembly, missing hardware, source-native product
  tree recovery, B-Rep-versus-mesh quality, or a "complete robot" claim, first
  read `../references/source-to-native-assembly-delivery.md` and execute A0-A8.
- Copy `assets/assembly-authoring-manifest.template.json` into the project and
  validate it with `scripts/validate_authoring_manifest.py`. An A-lane PASS
  proves only its declared authoring claims; S0-S11 remains independently due.

## Non-negotiable engineering rules

1. Separate product definitions, occurrences, process identities, geometric ports and solver units. Preserve the links between them.
2. Preserve raw STEP strings. Declare decoding and normalization; detect key collisions before joining tables.
3. Compute world poses once per occurrence and verify transform direction independently. A plausible render is not a transform check.
4. Extract geometry once per unique definition, then instantiate it by verified occurrence transforms.
5. Recognize a mechanism before running generic path search.
6. Move purchased modules and rigid subassemblies at the correct solver-unit granularity. Partition flexible elements explicitly.
7. Distinguish intended baseline contact from new contact or penetration at region level; do not whitelist whole parts.
8. Certify the full path interval against the sequence-aware static set. Endpoint, broad-phase and dense-sample checks cannot be upgraded to stronger claims.
9. Label exact B-Rep, mesh, convex, voxel and open-surface evidence separately. Report solver and tessellation tolerances.
10. Test both axis signs unless a stronger source fixes direction. Apply heuristic direction rules only inside their validated part class.
11. Validate the actual tool, engagement and swept grip modes. A ray is not a tool envelope.
12. Keep source mutations reversible and within authorization. Preserve superseded evidence and restore temporary scene changes.
13. When source evidence cannot fix a pose, enumerate the candidates in a fixed perturbation order (same-orientation unit translations, including cantilevered or single-side hangs, then orientation, then identity) and report `UNKNOWN` or `HOLD` with the discriminating observation. Never present the most plausible candidate as the result, and never exclude a candidate because it looks weak.

## Tool chain (canonical, since 2026-09-03)

The executable pipeline lives in `tools/` of this module (README lists the S0–S11 execution order,
the K-IF-04/K-IF-09 measurement tool, the independent visual-scope oracle, farm launchers and the
acceptance validator). Run tools against a project via `--case-dir`; never copy the tool chain into a
project. Derive scale-dependent parameters (walk cap, frames per mover, minimum framing) from the
target machine (K-PATH-19). Heavy stages (reverse walks, audits, renders) run on fleet nodes through
`tools/farm/farm_launcher.py` with verified concurrency (K-GOV-16).

## Execute S0–S11

Use [workflow.md](references/workflow.md) as the operating procedure:

- S0 freezes sources, frames, records, claims and authority.
- S1 binds occurrence identity and independently checks world poses.
- S2 builds analytic mating-interface evidence.
- S3 connects process roles to occurrences and ports, and (S3.5, since 2026-09-05) writes the component
  functional-attribute handbook: per unit/subassembly function, principle, install method and sequence
  constraints with source and confidence (`tools/function_tracks/`), then audits the sequence against
  them (`tools/audit_sequence_constraints.py`, K-PATH-20/K-DEL-06). Geometry-feasible order that violates a
  functional or installation dependency is a defect of the same weight as a penetration.
- S4 resolves known mechanism semantics.
- S5 forms solver units and rigid/flexible partitions.
- S6 determines direction with an explicit evidence level.
- S7 certifies trajectory, collision coverage and sequence state; the within-operation order comes from the
  functional play order (seed/carrier first, contact-supported next, fasteners after what they clamp,
  K-PATH-21) and bench groups join the machine as certified rigid units (K-SCN-15).
- S8 verifies controlled tool access.
- S9 optionally ranks ambiguous directions under a fair physical-probe contract.
- S10 creates instructions or animation only from certified motion, performs saved-file readback, and before delivery passes the presentation gates: independent visual-scope penetration scan (K-GOV-15, PATH=0), readability and interpolation checks (K-SCN-10/11), material and occlusion calibration (K-SCN-12/13/14), a multi-agent frame-by-frame review with zero severity-3 defects (K-DEL-04), and a seated-overlap ledger (K-DEL-05). Machine gates passing is not delivery.
- S11 calibrates the fixture and executes machine-readable and minimal physical falsification checks.

S9 is a short-prefix direction-ranking branch, not the general contact-physics
stage. Route flexible/contact-driven, calibrated force/torque, press/snap, or
threaded-contact claims through the separate branches in
[algorithm-physics-routing.md](references/algorithm-physics-routing.md), then
return to the applicable S7/S8/S11 gates. Do not add a fictitious S12 or let a
simulation result bypass S11 physical falsification.

Do not skip a failed upstream gate by marking a downstream visualization successful.

## Query the bundled knowledge

Run commands from this skill directory:

```bash
python3 scripts/query_ontology.py overview
python3 scripts/query_ontology.py criterion K-PATH-14
python3 scripts/query_ontology.py criterion --domain ID
python3 scripts/query_ontology.py failure F-SCN-12
python3 scripts/query_ontology.py stage S7
python3 scripts/query_ontology.py tool T-occt-xcaf
python3 scripts/query_ontology.py paper-algorithms
python3 scripts/query_ontology.py physics-lanes
python3 scripts/query_ontology.py route --operation-class FLEXIBLE_OR_CONTACT_DRIVEN
python3 scripts/query_ontology.py thresholds
python3 scripts/query_ontology.py category-errors
python3 scripts/query_ontology.py open-items --severity BLOCKER
python3 scripts/query_ontology.py search 穿模
python3 scripts/validate_authoring_manifest.py /path/to/assembly-authoring-manifest.json --check-files
python3 scripts/validate_algorithm_physics_contract.py /path/to/algorithm-physics-contract.json
```

Use `--json` before the subcommand for machine-readable output. Read [casebook.md](references/casebook.md) when deciding whether a lesson transfers; never copy a case verdict or threshold without checking its applicability.

## Required final report

Return three distinct blocks even if one is short:

1. **Engineering result:** claim-by-claim status, evidence files, exact/proxy class, coverage, holds, failures and the next discriminating action.
2. **Semantica result:** package/runtime verification actually performed, or an explicit statement that none was performed. Never imply publication, certification or approval.
3. **Learning result:** `no_delta` when the run only reused known method, or a named candidate with evidence when a genuinely reusable new lesson exists. Do not silently edit or promote a formal ontology.

Prefer `HOLD` or `UNKNOWN` to a fluent guess. “No interference found in the checked interval” is a valid bounded result; “this arbitrary assembly is proven buildable” usually is not.

## Maintain this skill

The JSON under `references/ontology/` is an immutable snapshot. The A0-A8
authoring workflow is an operational companion, not a silent ontology
promotion. Do not patch the snapshot casually during project work. Put target
facts in the target project. If a repeated, independently evidenced lesson is
reusable, process it through `ontology-engineering`; only after review create a
new snapshot, update the manifest hashes, run `scripts/validate_bundle.py`, run
the skill-creator validator and regression-test at least one prior case plus
one unrelated STEP or source-native assembly.
