---
name: cad-agent
description: Design, repair and validate parts, assemblies and mechanisms in Fusion, Siemens NX or AutoCAD, and pass native CAD evidence to manufacturing process decisions. Use for reference-image reconstruction, STEP/PRT/DWG/DXF, drawings, BOM/native geometry, mating/joints, kinematics, assembly planning, and CAD-to-process handoff.
---

# CAD Agent

Use the ontology as an engineering grammar for CAD work: identify the right
objects, relations, frames, constraints, state transitions, and checks, then
produce and verify the native CAD artifact.

## Engineering-ontology CAD module

The real directory `ontology-engineering/skills/cad-agent` is the canonical
CAD module. The former top-level `~/.codex/skills/cad-agent` directory is a
legacy copy that may be archived after caller migration. This module owns CAD tools, native geometry
readback, assembly and mechanism engineering. The parent
[`ontology-engineering`](../../SKILL.md) controls semantic engagement;
source-locked Semantica alone executes formal ontology, CQ, SHACL, rules and
project decision review. CAD tool success, a local ontology snapshot or a
semantic finding does not prove an as-built joint, seal or product release.

The former local CAD ontology/CQ/SHACL sources are retained in a private
migration archive, outside this module. They are not a second production
semantic backend. The static tool inventory remains available without a query
engine or external repository checkout:

```bash
SKILL_DIR=<this skill directory>
python3 "$SKILL_DIR/scripts/semantic_query.py" capabilities  # bounded CAD tool catalog
```

Runtime resolution uses this skill's own `.venv` and its wrappers discard a
stale `CAD_AGENT_ROOT`, so normal execution cannot silently fall back to the
old checkout. An explicitly selected repository-bound workflow runs through
that repository's own entry points instead. Fusion execution uses the bundled
venv via `scripts/fusion_call.py`.

Edit operational knowledge, references, templates, validators, and safe
wrappers directly in this canonical directory. Do not rebuild it from
`~/120-agent-cad`, do not treat that repository's `.agents/skills/cad-agent`
as the source of truth, and never use a source-to-canonical `rsync --delete`.
If a formal runtime or ontology-code change is developed in a repository,
review and merge its exact delta into this directory without deleting
canonical-only contributions. See `CONTRIBUTING.md`.

Recipients run `setup.sh` once, then `doctor.sh` (add `--smoke` for one live
guarded read) to verify the bundled runtime, Peekaboo, and Fusion endpoint.
`BUILD_INFO` records both the historical migration source and the installed
Fusion-only wheel; skill content has its own canonical update stamp. The
distributed wheel contains no CAD ontology engine, semantic assets, or
`cad-agent-mcp` executable.

Lessons discovered while using the skill (a wrong rule, a new pitfall, a
missing capability) must not be lost: either apply a reviewed change directly
to this canonical directory or record a short note in `lessons-inbox/` for
later review. Formal ontology lifecycle work may still use a bound repository,
but that repository never becomes the authority for this skill directory.

For parts, drawings/BOMs, assemblies and mechanisms, use the
[general CAD engineering workflow](../cad-engineering-workflow.md) and
`contracts/cad-evidence.v1.schema.json` before a process-specific handoff.
`scripts/cad_evidence.py` checks source hashes, native readback pointers, object
references and dependency impact; its project ABox is evidence, not a formal
ontology verdict. Keep definition, occurrence, mating, motion and physical
performance claims separate. Welding is one case, not the module boundary.

When a CAD or simulation result supports a process or product claim, use the
shared [engineering evidence methods](../../../engineering-evidence-methods/SKILL.md).
Bind the observable and reference state, required geometric features, model
revision and applicable causal/validation checks to the claim. Source integrity,
evidence applicability and as-built performance are separate conclusions. Keep
`analysis-record.v1` readable; its free-text assumptions do not automatically
become verified structured conditions.

批量图纸修订、回读摘要或多来源对照按[共用批量判断](../../../../docs/judgment-intake.md)交接对象 ID、
实例/配置版本、原文位置和条件。原生模型与图片先由对应工具提取，再由 Jev 提出候选。
同名对象不自动合并；候选“不相关”不取消必查方法。接口或配置变化后，把支持依赖
传给原生影响查询，列出需重审的 CAD 主张、工艺推荐及报价。

For any assembly or mechanics claim, read the
[general assembly and physics kernel](../assembly-and-physics-kernel.md).
It separates occurrence identity, complete interface graphs, every relevant
intermediate state, outside support, insertion paths, load demand, local joint
capacity and physical evidence. The public [nominal assembly screen](../../assembly/README.md)
and [four-bar screen](../../mechanism/README.md) are bounded examples, not a
replacement for the existing S0-S11, contact-physics or native-CAD evidence.
The [portable remote bridge](../../remote/README.md) is available alongside this
studio's NX/AutoCAD wrappers; recipients configure their own hosts and CAD
software rather than inheriting this studio's machine settings.

When photos, video, patents or incomplete CAD are used to infer a shape,
mechanism or assembly, read [evidence-driven reconstruction](../evidence-driven-reconstruction.md).
Separate visible features from hidden mechanism hypotheses, use physics and
kinematics to derive conditional constraints, and retain indistinguishable
alternatives. Bind derivations and CAD candidates to the actual source revision;
the evidence adapter does not certify that a proposed mechanism is the
original or that a physical product works.

When CAD geometry affects manufacturing choice, or a proposed process needs
CAD fit, access or contact checks, read
[CAD/process integration](../cad-process-integration.md). Produce a
hash-bound project handoff with `scripts/cad_process_handoff.py`; pass its
bidirectional questions to the manufacturing module and project owner. The
adapter exports a project ABox and dependency impact only. A process route
still needs its own functional coverage, test and decision evidence.

## Fusion execution (one-shot, no resident MCP)

When the user explicitly requests registered MCP tools, use the
[optional stdio registration](../mcp-registration.md). Its transport
keeps one guarded cell per Fusion call and routes semantics to the parent
Semantica runtime; it does not restore the legacy CAD semantic service.

All privileged Fusion work goes through the guarded one-shot entrypoint —
never connect to 127.0.0.1:27182 directly:

```bash
SKILL_DIR=<this skill directory>
python3 "$SKILL_DIR/scripts/fusion_call.py" fusion_mcp_read '{"queryType": "projects"}'
```

Anything more than a single call goes through `scripts/fusion_ops.py`, not a
hand-written driver: it owns the one result taxonomy (nine `classification`
values; exit 0 means *delivered*, never *succeeded* — only `delivered_ok` does),
ASCII-safe script rendering, exact document search, and the rule that
`guard_transient` is the only retryable class. The unattended
`scripts/fusion_step_split.py` / `fusion_document_f1.py` /
`fusion_step_import.py` / `fusion_post_import.py` /
`fusion_supplier_occurrences.py` family builds on it; run
`scripts/fusion_preflight.py` first (read-only, sends no MCP request; exit 3 =
blocker) and `doctor.sh --json` for a machine-readable version of the same
checks.

After a timeout or retirement, take evidence before restarting Fusion:
`scripts/fusion_recovery.py evidence --latest` reads Fusion's own AppLog and
the guard markers without sending any MCP request, and
`recover --marker <retirement.json> --apply` lifts the retirement when the log
proves that request completed (exit 4 = insufficient evidence, nothing written;
only then swap to a new PID).

For multiple Fusion instances or parallel agents, follow
[exact instance/document binding and background UI control](../fusion-multi-instance.md).
The 2026-09-24 two-round native test passed for two independent processes and
documents (96 then 144 solids each, saved and separately read back). Use that
bounded evidence for independent-part work; retain the per-PID serialization
and do not infer same-document, arbitrary UI, complex-assembly or CAM acceptance.

Before any Fusion call, read `references/fusion-execution.md` in this skill:
it carries the mandatory safety rules (session-per-call semantics, no blind
retry after timeout/ambiguity, PID quarantine), the call-budget table and host
sleep discipline (`--long-request`, never work with the lid closed), and the
save discipline that prevents the classic agent deadlock on unsaved/untitled
documents and modal save dialogs.

For field-driven, lattice, implicit, or continuously varying geometry read
`references/computational-geometry.md` (performance core vs integration
shell routing) before choosing between Fusion features and code-generated
geometry.

When images or sparse observations leave the shape-generating rules or core
controls unclear, read
[inverse shape parameter discovery](../inverse-shape-parameter-discovery.md).
Use an inspectable intermediate model when it resolves that uncertainty;
extract and test control variables and their relationships, then calibrate the
regenerated shape against the original evidence. Generated meshes are
hypotheses, not measurements. Skip the bridge when the generating model is
already clear. The reference includes a separate case with evidence and limits.

## NX and AutoCAD execution (dell-nb, one-shot, no resident MCP)

Siemens NX 2412 and AutoCAD 2024 live on the Windows notebook dell-nb, not on
this Mac. Both are driven exactly like Fusion: one short-lived process per
call, no MCP registration, nothing resident on the caller side.

```bash
SKILL_DIR=<this skill directory>
"$SKILL_DIR/scripts/nx_bridge.sh" status            # start|stop|restart|log — headless NX bridge on the Dell
"$SKILL_DIR/scripts/nx_call.py" --status
"$SKILL_DIR/scripts/nx_call.py" nx_open_part '{"path": "model.prt"}'  # relative to the Dell workspace
"$SKILL_DIR/scripts/nx_call.py" nx_list_components
"$SKILL_DIR/scripts/nx_call.py" nx_close_part '{"save": false}'
"$SKILL_DIR/scripts/autocad_service.sh" status      # start|stop|restart|start-acad|logs — relay in the user's desktop session
"$SKILL_DIR/scripts/autocad_call.py" --status
"$SKILL_DIR/scripts/autocad_call.py" read_texts '{"space": "model"}' > texts.json
```

Before the first call read `references/nx-execution.md` or
`references/autocad-execution.md`. They carry the Windows session rule (SSH
lands in session 0; only an Interactive scheduled task reaches the user's
desktop, and `ugs_router.exe -ug -use_file_dir "<prt>"` is what opens a part in
the user's NX), the workspace-relative path rule for NX, the verified or
unverified status of every tool, and the save discipline: never save customer
NX parts (`nx_close_part {"save": false}`), open user DWGs read-only and
`save_drawing_as` to a new file. A call costs 6–10 s of SSH and process
start-up; that is not a hang. The Dell-side sources needed to rebuild both
services are bundled under `assets/nx-mcp/` and `assets/autocad-mcp/`.

For NX length/width variants, repeated hole rows, existing-drawing refresh,
annotation overlap, or exporting all final drawings for review, read
`references/nx-variant-drawing-review.md`. It carries the measured NX 2412
workflow, profile-specific limits, and offline row/packet helpers. Retrieve by
this engineering intent; a historical part name is an example, not the route.
Native refresh, drawing legibility, export completeness, and engineering
acceptance have separate outcomes.

## Assembly ontology module

Assembly engineering lives in `assembly/` inside this skill. For an existing
STEP analysis, read `assembly/SKILL.md` and use the bundled S0-S11 ontology.
The executable film/animation tool chain is canonical in `assembly/tools/`
(since 2026-09-03; see its README) and runs against a project `--case-dir`.
For source-native tree recovery, missing screws/nuts/harnesses, rejection of
coarse triangle delivery, or creation of a complete persisted Fusion assembly,
also read `references/source-to-fusion-assembly-delivery.md`, use its A0-A8
authoring lane, and validate the project manifest with
`assembly/scripts/validate_authoring_manifest.py`. The authoring gate and the
S0-S11 process gate are independent; neither a plausible model nor an animation
upgrades unresolved evidence to PASS.

For source-native wooden/brick assemblies whose shape review includes bilateral
symmetry, principal-axis three views, outward curved faces with filled internal
support, prefix stability, or optimization feedback, also read
`assembly/references/brick-assembly-optimization.md`. It is a bounded extension
of the assembly gates; it does not turn assumed pitch/density or static balance
into a measured connector-capacity claim.

When planning provenance, ambiguous insertion direction, flexible/contact-
driven transfer, force/torque, threaded contact, or physical release matters,
route through `assembly/references/algorithm-physics-routing.md`, create the
machine contract from
`assembly/assets/algorithm-physics-contract.template.json`, and validate it
with `assembly/scripts/validate_algorithm_physics_contract.py`. S9 remains a
bounded short direction probe; full contact physics is a conditional branch,
and only S11 physical evidence can close a physical-release claim.

## Mechanism module (kinematics-first design)

Mechanism-first work (mechanism topology reading, kinematics modeling,
trajectory solving/optimization, trajectory contracts that lead physical
design, motion mapping and rendering) lives in `mechanism/` — read
`mechanism/OVERVIEW.md` first. It carries the kinematics layering exemplar (`mechanism/wiper-kinematics/`:
config -> single solver -> regression tests -> trajectory schema), already
transferred successfully to the ceiling-trolley project. The old wiper
ontology and its local validators remain in the private migration archive;
new formal CAD semantics must enter a governed Semantica package.

## Formal semantics and production-depth CAD work

The parent [`ontology-engineering`](../../SKILL.md) owns Semantica engagement,
package lifecycle, binding, query/shape execution, review and reusable
knowledge promotion. Use the CAD/process handoff above for project evidence;
never use the bundled CAD wheel's historical CQ or SHACL methods to make a
project verdict. The legacy standalone CAD repository and old curriculum
records are provenance, not an active semantic authority.

For P3 manufacturing/release depth use the CAD execution toolkit:
`references/p3-release-workflow.md` (design brief + change protocol),
`references/p3-engineering-gates.md` (validation and manufacturing release),
the `assets/` templates (design brief, execution plan, verification report),
and `scripts/init_cad_job.py` / `scripts/validate_design_brief.py`.

Before the first tool call that observes or operates live Fusion, Peekaboo, or
Fusion MCP, always read and apply `references/fusion-execution.md`, then select
the available authority mode. In repository mode, also read `AGENTS.md`
section `Live Fusion / Peekaboo / MCP 操控基线（canonical）`; the repository
copy is the versioned authority for that external workspace even when a
user-global `AGENTS.md` exists. In canonical-skill mode, read the bundled
`references/runbooks/fusion-mcp-caller-guide.md` instead; never require an
unavailable repository path. If the task encounters or resumes from a timeout,
queue hang, crash, forced termination, hidden/delayed modal, or
interactive-command incident, repository mode must additionally read
`docs/fusion-runtime-safety-incidents.md`, while canonical-skill mode must read
`references/runbooks/fusion-runtime-safety-incidents.md` and
`references/runbooks/fusion-mcp-ambiguity-triage.md`, before any recovery
action. Do not delegate reading or interpreting any mandatory route to another
agent.

Treat every untitled or never-saved Fusion document as a preservation
obligation, even when native `isModified` is false.
Before workspace cleanup, unrelated document work, close, quit, controlled
restart, or PID retirement, inspect it with one exact MCP read, derive a
task/model-semantic name, resolve one exact project/folder separately, and use
Fusion MCP/API `Document.saveAs` with a no-overwrite gate. Never substitute a
GUI save, shortcut, or UI automation. After window settle, use a new MCP request
to verify exact name, lineage, version, saved/modified state, and relevant native
state. HOLD if identity, name, or destination is not unique. Discard only when
the user explicitly names the exact document to discard. After an ambiguous MCP
boundary, do not save on that PID; follow the incident rules instead.
For a protected `fusion_mcp_execute` script, declare exactly one top-level
`FUSION_MCP_UNTITLED_PRESERVATION_ROLE` with value `inspect`,
`destination-preflight`, `save-as`, or `readback`; ordinary `fusion_mcp_read`
does not need the constant. The role opens only that runtime phase and never
replaces exact document/project/folder/no-overwrite/native-state guards inside
the script. All production Fusion cells on one execution node must share one
stable, owner-only canonical safety state root. Never rotate
`--state-directory` per project, attempt, or request, because doing so hides a
prior PID's in-flight or retirement marker from the next cell.

When Fusion runs on another Mac, run the Fusion executor, Fusion MCP client,
and Peekaboo on that execution node; keep Fusion MCP loopback-only and never
forward its port to make a second writer. In an SSH session without local TCC,
inspect the execution node's Peekaboo GUI Bridge and permission source before
capture. If the healthy selected source is the GUI Bridge, do not add
`--no-remote`; use the live-discovered absolute CLI path when the noninteractive
shell lacks it. Host-specific addresses, sockets, PIDs, and window IDs come
from higher-scope instructions and fresh discovery, never from this skill as
permanent facts.

For supplier CAD acquisition, exact purchased parts, missing STEP artifacts,
Fusion catalog insertion, or McMaster-Carr work, read
`references/supplier-cad-acquisition.md` before choosing an embedded, API,
local-import, or external-browser path.

## Choose one profile

- **P1 Fusion practice** is the default for modeling, assembly, repair,
  exploration, and engineering reproduction.
- **P2 course UI acceptance** adds exact material-step and visible UI evidence
  only when the lesson requires that path.
- **P3 governed release** covers formal `cad-agent.task.v2` external handoffs,
  runtime ontology/ABox/tool-policy changes, semantic releases, and production
  or manufacturing delivery.

Never impose P3 records or independent-review gates on ordinary P1/P2 work.
An explicit user request to build or continue a lesson authorizes reversible,
in-scope Fusion operations, Save As, and checkpoints in the designated practice
document. Ask separately before deletion, trusted-source overwrite, public
sharing, production export, final toolpaths/post-processing, manufacturing
submission, credentials, or payment.

## P1/P2 execution loop

1. Inspect the current Fusion document, lesson checkpoint, and required source
   assets. Do not infer live state from an attempt directory.
2. Make a compact ledger of requirements, assumptions, critical unknowns,
   invariants, and measurable acceptance conditions.
3. Use the ontology to type the involved definitions, occurrences, features,
   interfaces, frames, parameters, units, constraints, and intended state
   transition.
4. Run the problem-classification and typed workflow-memory preflight from
   `references/workflow-experience-memory.md`. For any blocker or anomaly,
   classify its controlled observations, follow the ordered diagnostic aspects,
   and review prior patterns before external search or a new recovery design.
   Retrieve by intent, current and desired state, per-use roles/invariants,
   generic context facets, environment, failures, and validation claim—not only
   by lesson number, filename, object name, or part number.
5. Resolve only unknowns that could select the wrong object, make the operation
   destructive, or invalidate the engineering result. Probe release-local API
   behavior in an isolated document when that is the fastest safe resolution.
6. Execute the next meaningful Fusion stage with one writer. Related navigation
   and modeling actions may be grouped; do not create a gate file for every API
   call.
7. Read back native Fusion state and run targeted geometry, constraint,
   interference, motion, rebuild, or persistence checks.
8. Save a stage checkpoint and one concise practice log. For each meaningful
   stage preserve its PRE/POST evidence, decision rationale, failures/recovery,
   checks, environment, and unknowns in one process-level workflow record.
9. Continue until the requested artifact and checks are complete. Validate the
   workflow record and compile advisory pattern candidates; activate shared
   memory only when a genuine reusable delta enters P3.

For P2, credit an exact UI action only when the live control path and resulting
state are observed. API reproduction may establish the P1 engineering result
but never masquerades as course UI credit.

## Process memory

Treat process memory as a continuous engineering loop:

```text
problem situation -> classification -> diagnostic route -> memory review
                  -> prior-pattern disposition -> hypothesis + probe

episode -> evidence-bound workflow record -> reusable pattern candidate
        -> exact reuse / analogy / warning / validation guidance
        -> new episode and applicability feedback
```

`GroundedExperience` remains the high-fidelity atomic-event record. A
`WorkflowExperienceRecord` orders those events and preserves decisions,
failures, recovery and validation across the task. Reusable patterns include
execution recipes, modeling strategies, recovery patterns, validation
strategies, decision heuristics, and failure warnings. Every retrievable pattern
must expose a concise statement of what later work may execute, adapt, avoid,
or validate—not just a URI or lifecycle state. `ToolCapability` is only one
dependency of an executable pattern; it is not the parent of all experience.

Before external search, proxy substitution, ad-hoc UI automation, or a new
recovery branch, repeat the lowest-cost problem and memory review. Do not
confuse a missing asset instance with a missing acquisition capability, or one
adapter's failure with every surface's failure. Expose classification evidence,
unresolved diagnostic aspects, prior-pattern accept/reject reasons, and the
next discriminating probe. Exact reuse requires closed hard
conditions; analogous results must expose mismatches, adaptations, risks and a
diagnostic probe. Reuse a validation method, never its historical result. Every
retrieval is advisory and cannot authorize a Fusion action.

Treat every validation claim as scoped to its exact observed subject revision.
When an upstream stage creates a new unsaved or persisted revision, enumerate
the downstream consumer invariants that may have changed and rerun their bound
validation profiles on that revision. A composite workflow is closed only when
its machine state carries the continuation, the exact upstream/downstream
package identities, and a content-addressed completion receipt; a documentation
promise to "recheck later" cannot preserve or recreate a PASS.

For a cross-lesson retrospective, keep the historical episode, retained
workflow memory, and generated curriculum projection as three independent
axes. A hash-bound retrospective index may locate evidence and route curation,
but it must never synthesize a missing `WorkflowExperienceRecord`, merge
parallel attempts, or activate shared memory. A missing record means an
explicit `CURATE_*` disposition or no reusable delta—not that the episode did
not happen and not that a new record may be invented after the fact.
`PATTERN_PRESENT` additionally requires a hash-verified RDF graph plus a real
episode-specific `GROUNDED_IN`, `APPLIED_IN`, or `RETRIEVED_FOR` relation; graph
presence alone is insufficient. Projection state is the bound episode's
membership in one named projection source, not global projection freshness.
Its subject, source, top-level state, and stored state must agree.

For Git source-basis preflight, the checker and Schema jointly own an
irreducible trust closure. A caller-selected policy is diagnostic-only and can
never authorize review, release, activation, routing, execution, or Fusion
writes.

For a P3 experience-to-Agent linkage change, keep four stages distinct:

```text
episode intake -> candidate materialization -> semantic build/route
               -> independent activation pointer -> executor recheck
```

The historical `cad-agent-semantic-build`, `cad-agent-semantic-preflight`, and
`cad-agent-semantic-runtime-route` commands belong to the archived legacy
repository and are unavailable in this module. Route new candidate semantics,
shadow evidence, binding and review through the parent Semantica control plane.
A `cad-agent.task.v2` from the old repository is provenance only here; its
local route request cannot activate knowledge or authorize Fusion execution.

A memory-control target snapshot is still planned state, not inherited memory.
Only a committed wrapper whose external receipt, full-state digest,
generation/predecessor lineage, and independent reopen evidence reproduce may
be accepted as prior state or materialization input. The current candidate
assessor proves structural consistency and distinct supplied actor/verifier
IDs only; it does not prove real identity or organizational independence
without an active trust registry. Pointer causality is decision + expected old
state -> pending CAS plan -> external transition receipt -> independent reopen.
Never require or synthesize the receipt before the transition.
The live pointer state contains only stable pointer/scope identity, monotonic
generation, the exact predecessor logical-state digest, and the current
release/build/selection closure. Keep that logical digest distinct from the
SHA-256 of the canonical live-file bytes. At the available clock resolution,
enforce the non-decreasing causal order `acceptance <= decision <= transition
activity <= receipt <= fresh-process reopen`; activity and receipt must both
fall inside the actuator process interval. Rollback restores an earlier closure
in a new generation; it never copies historic pointer bytes or decrements
lineage.

Runtime identity and active-memory selection have a separate bootstrap rule.
Never accept an authority registry, reviewer, actuator, verifier, clock, active
pointer, or evaluation time from `CadAgentTask`, a route request, or the
candidate being reviewed. `RuntimePrincipal`/`SoftwareService` are stable
identity-bearing entities; reviewer, actuator, verifier, trusted-clock and
active-memory-selector are contextual, scope/version/time-bound role
registrations. A repository registry is still only a candidate until the host
pins the exact fixed authority-pointer bytes and Ed25519 root key outside that
repository. The host may carry that public pin in one absolute,
owner-protected descriptor outside both repository and authority-data roots;
the descriptor may also pair content-addressed consumption and feedback clock
events from the selected data root. It may select the data root but never the
contract root: validation Schemas come only from the running package/source,
and the descriptor contains no private key, task, route, or output path. Only a
loader-verified descriptor object may claim `HostDescriptor` provenance;
direct/synthetic pins must be labelled as test configuration and cannot claim
descriptor verification. Public reports expose stable codes and fixed safe
messages, never schema instance values, absolute roots, descriptor paths or key
material. Without the external pin, report `ACTIVE_AUTHORITY_NOT_CONFIGURED`;
never self-bootstrap from colocated files. Treat the loader result as a sealed
composition-load snapshot (the CLI loads it at run start), not as proof that
the descriptor path is continuously monitored or re-read.

The isolated pointer canary is a POSIX, cooperative-lock mechanism proof. Its
host/actuator must pre-create and pin the owner-only lock inode; a reopen worker
opens that lock read-only and must never create a file or directory entry.
Factory-sealed pins are re-pinned and identity-compared in each worker instead
of being reconstructed from caller fields. Bound worker stdin/stdout/stderr and
file reads before buffering them. A failed round trip must expose a bounded
same-process residual-state observation and explicit recovery status; it may
not hide a candidate generation or convert recovery into PASS. These rules do
not establish Windows, network-filesystem, non-cooperating-writer or production
authority semantics.

An active-memory lookup must start from the authority pointer's explicit
fixed-path selection root, never a task path, directory scan, newest filename,
or legacy fallback. Freeze the full task hash before lookup; distinguish zero,
multiple, invalid, stale, reopen-drift and unsupported-rollback states. A
passing read-only selection verifies signed clock, release acceptance,
activation decision, actuator receipt, independent reopen, exact release,
exactly one reproducible semantic build and actual active-bundle binding, then
reopens the root. The selector itself always reports
`active_memory_consumed=false` and grants no Fusion mutation. The standard
Agent now has a bounded candidate consumer: only a loader-verified host
selection is freshly revalidated, its exact build (never a caller-selected
alternative) produces one unique package, and a separate
`ActiveMemoryPatternUse`/`PatternApplicationConsumptionReceipt` records the
use. Selection, consumption, and feedback use three distinct signed clock
events from the same scoped role registration. Event/nonce identity and
sequence advance strictly; timestamps are non-decreasing at the available
resolution. Represent the bounded consumption window as a
`RuntimeConsumptionObservation`, never as an invented operating-system process
run. Only the later consumer report may be true, while all activation,
execution, pointer-change and Fusion-write fields remain false. Retrospective
joining cannot create this occurrence receipt, and the receipt cannot validate
itself. Feedback v2 does not yet accept, independently reopen, verify, or
persist it, so this candidate path does not close CQ-EA11. Request-supplied
`evaluation_time` remains a caller claim and cannot close freshness without a
verified clock observation.

The route caller supplies raw intent and a correlation `problem_id`, never the
`ProblemType`/goal/context/transfer answer key. Those values must come from a
compiled candidate-only recognition profile; zero or multiple classifications
remain distinct blockers. The shadow route must also expose per-pattern
dispositions and the next discriminating probe, while stating
`active_memory_consumed=false`; it cannot inherit the separate HostDescriptor
consumer's later use claim or impersonate the activated ABox `MemoryReview`
path. Any parameter extracted from raw intent must agree semantically and
lexically with the later preflight binding. `GuidanceOnly` and
`WarningOnly` pass to a reasoning/display consumer with their statement and
next probe; they must not be forced through ToolOperation linkage merely to be
retrievable.

## Assembly semantics

For assembly work, load
`references/i03-assembly-mode-workflow.md` plus the current lesson reference.
When placements are derived from dimensionless instruction panels or a
finished-product photo, also load `references/exploded-instruction-reading.md`
and keep its evidence ladder and perturbation order.
Represent a Joint as an n-ary engineering relation:

```text
Joint
  -> moving occurrence -> owned mating interface -> boundary/key point -> local frame
  -> fixed occurrence  -> owned mating interface -> boundary/key point -> local frame
  -> construction method and alignment
  -> permitted DOF, current coordinate, limits
  -> interference/contact policy and evidence
```

Keep `JointAlignmentAngle`, current `MotionCoordinate`, and `MotionLimit`
independent. Relation completeness does not imply zero remaining DOF. A shared
center, visually plausible pose, successful Joint call, or mirrored source name
does not prove the intended interface/frame/role. Verify occurrence identity,
frame origin and axes, motion behavior, limits, interference, save, and reopen
in proportion to the lesson.

For inverse assembly work, distinguish fused-set closure from owner-labelled
part recovery. Each candidate cut must be carried by an interface pair, a
bounded interface-plus-root support region, a complete connected material-delta
inventory, explicit region-owner assignments and non-interface continuity
witnesses. Ground the support in both interface envelopes; give each connected
delta exactly one assignment resource, keep alternatives inside that resource,
and require every candidate occurrence to participate in an assignment and a
boundary. Never extend an interface tangent/opening plane across unrelated
material. Keep interface dimensions, material ownership and DOF candidates
independent. A native-derived motion domain must retain every symmetry-allowed
DOF; an unmeasured member remains `MotionUnknown`. Before positive interference
rejects a DOF, close the sweep observations, localize the actual collision
material and resolve its owner provenance; a partition artifact or unresolved
region sends the solver back to the owner hypothesis. Assembly/disassembly
motion and in-service motion are separate. Set-equivalent owner alternatives
remain a solution family, and source overlap multiplicity erased by fusion
remains unrecoverable. The bounded RDF sidecar never certifies inverse
uniqueness by itself.

Keep the observation and reconstruction sides physically and semantically
separate. Source body, observed regions and partition checks remain bound to the
fused source document/version. Every candidate occurrence has exactly one
hypothesis-local `CandidatePartReconstruction` with an explicit body in a
different candidate document/version; the reconstruction check and motion/
collision configuration stay on that candidate version. Never reuse the source
body or source document as the candidate merely to make a Boolean check close.

For each hypothesis, require the complete Cartesian matrix
`RegionOwnerAssignment × CandidatePartReconstruction`, with exactly one
check-owned `CandidateRegionMembershipObservation` per cell. Native Boolean
values distinguish `RegionFullyContained`, `RegionAbsent`,
`RegionPartiallyContained`, and `RegionMembershipUnknown`; partial or unknown
cells fail closed for `OwnerLabeledPartScope`. `EvidenceDeterminedOwner` has one
admissible owner and one matching fully-contained cell. `AmbiguousOwner` has
multiple admissible owners but exactly one concrete fully-contained owner per
hypothesis; a different owner belongs in an explicit set-equivalent alternative
hypothesis. `MultiplicityLostOwner` may have one or more fully-contained owners
but must report `OwnerMultiplicityUnrecoverable`. When evidence proves neither
exhaustive uniqueness nor a concrete alternative family, use
`IdentifiabilityUndetermined`.

A native Joint may name a parent subassembly occurrence while the physical body,
landmark and interface belong to a descendant occurrence. Resolve that carrier
only when its native path contains exactly one bound functional descendant,
preserve both raw carrier paths in the snapshot, and let the runtime recheck the
hierarchy. Zero or multiple descendants is UNKNOWN. Capability absence is also
evidence absence: if a Joint kind does not expose alignment, offset, flip,
motion, or health, never synthesize 0, false, current-as-neutral, or healthy.
The profile may constrain only fields it declares and the adapter can actually
read.

For a functional assembly check, use the domain-neutral invariant pattern
rather than product-name branches:

```text
exact configuration + closed occurrence/body-pair scope
  -> contextual roles + profile-controlled role separation/counts
  -> stable landmarks + same-configuration/frame position observations
  -> signed direction + relative placement
  -> endpoint-grounded interface pairs
  -> contact + engagement + axis/size/depth evidence
  -> material exclusion + explicit bounded-overlap exceptions
  -> participant/interface-grounded joints + current state/limits
  -> complete coverage -> PASS / FAIL / UNKNOWN
```

Every configured occurrence must be included or explicitly excluded with a
reason and accounted by a profile invariant. Use a role-cardinality requirement
for ordinary parts whose only relevant invariant is presence/count; do not
invent a direction measurement for them. Close the interface inventory in the
same scope and put interface roles on `MatingInterface` so a roleless interface
or missing envelope remains detectable. Count the profile-selected occurrences
and interfaces before checking their evidence. Anchor each functional frame role to the configured
assembly or a scoped owner and a profile-owned expected basis; resolve canonical
direction definitions through that basis instead of trusting a direction
label. A native check, both landmark-position observations, the measured
vector, and the reference vector must refer to the same exact
configuration/frame. Treat the profile's required fit as the specification and
the envelope/observation as evidence that must agree with it. Use
product-specific profiles only to supply roles, frame bases, directions,
counts, fit/contact/kinematic targets and thresholds; keep the reusable TBox,
SHACL and CQ free of anatomy or part-name branches. Do not let zero
interference stand in for engagement, coaxiality stand in for size, a Joint
stand in for its intended interfaces, or an allowed overlap exempt unrelated
body pairs. Treat `PartiallyEngaged` and `FullyEngaged` as observed geometric
coverage states, not verdicts. Blind, recessed, and stop-limited connections
may legitimately be partial, but only a profile-owned insertion-depth interval
together with the other independent interface criteria can accept them; any
positive overlap by itself is too weak.

Landmark identity must be chosen before pose evaluation and remain the same
under a rigid transform of its owner. Use a version-bound native point/entity,
Joint/construction origin, or independently grounded owner-local point.
World-axis-aligned bounding-box centers/extrema are pose-dependent envelope
statistics: they may support bounds checks but never the ordered endpoints of
an `OccurrenceDirectionRequirement` or `RelativePlacementRequirement`. A
selector must contain identity only, never the expected sign, threshold, or
verdict. Freeze and reuse the same `binding_plan_digest` across a pose repair
and its 180°/side-relabel metamorphic check; a changed digest is a different
selector experiment, not evidence that the original binding survived motion.
Otherwise a reversal can relabel the opposite world extreme and circularly
preserve PASS.

Require exactly one same-profile role-cardinality requirement for every
functional role referenced anywhere in that profile. An extra native
occurrence is not covered merely because the binding plan gives it a new role
or reuses a known role; absent exact cardinality coverage makes the closed
scope `UNKNOWN`, while an observed count excess is `FAIL`.

An allowed material overlap must be localized inside its declared native
interface envelope. The current release does not support disabling
localization; a false containment flag is fail-closed, not a blanket
occurrence-pair exemption.

Every executable assembly profile must also own exactly one default
`PairwiseMaterialExclusionRequirement` over the complete unordered solid-body
pair closure. Missing or duplicate defaults make the result `UNKNOWN`; an
allowed-overlap rule is only a localized exception to that one default.

Do not infer rotation phase from an axisymmetric mating pair, a Joint axis,
`q=0`, or a release-local flip flag. If phase matters to function, require a
stable off-axis landmark direction or an independently grounded local frame.
Without that evidence, report that phase as `UNKNOWN` and narrow any assembly
PASS to the invariants actually covered.

Use one flat live-evaluation path:

```text
hash-bound active profile
  + one binding plan containing only native identities/selectors
  -> one read-only canonical Fusion snapshot
  -> trusted recomputation of every metric
  -> PASS / FAIL / UNKNOWN
```

The binding plan must not contain thresholds, measurements, or outcomes. The
snapshot must not contain derived engineering metrics or a self-reported
decision. Missing scope, pair, endpoint, joint, or native evidence is UNKNOWN;
an observed or structural violation is FAIL. Native axis-aligned bounds are
only bounds evidence, not proof of exact topological containment.
The bounded P1 semantic projection lacks complete native solid-body inventory
and pair closure, so it may report criterion results but never upgrades them to
an overall `FunctionalAssemblyPass`; real assembly PASS comes only from the
live profiled runtime. The offline `evaluate_profiled_snapshot` entry point is
diagnostic only and cannot return a top-level PASS.

Contact at a selected interface requires the native minimum distance between
the exact, version-bound BRep face sets named by that interface-pair binding,
plus its profile-owned
`InterfaceBoundaryMinimumDistanceMetric` interval. An occurrence-wide body
minimum distance never substitutes for this interface-specific observation;
body-pair observations remain the source for interference closure. The current
bounded-cylinder engagement adapter requires exactly one interval for each of
axis alignment, axis separation, interface-size difference, selected-boundary
distance, engagement depth and positive interference volume; a missing,
duplicate or unsupported metric leaves profile selection fail-closed.

Treat a profile digest as derived integrity data: manifest hashes bind static
profiles, while the live profiled runtime, P1 kernel and semantic service must
recompute the canonical executable projection. An inline graph cannot
authenticate its own digest literal.

## Semantica control plane

Use the parent [semantic engagement contract](../../../../references/semantic-engagement-contract.md)
for vocabulary, CQ, SHACL, rules, provenance, package lifecycle and project
review. This module supplies native CAD readback and a source-bound ABox;
`scripts/semantic_query.py capabilities` reads a static tool inventory only.
The migration wheel's former semantic sidecar and P3 command suite are absent
from the installed Fusion-only runtime. Historical CAD task contracts may
still inform a migration candidate but cannot issue a current review receipt,
approve a weld route, or replace native geometry checks.

## Video learning and evolution

Video evidence remains an observation until reproduced in Fusion or the real
lesson application. Freeze the material steps needed for the lesson, reproduce
them, save the correct artifact type, and run native checks.

After successful practice:

- If the existing ontology, recipe, recovery guidance, and tool contract already
  cover the result, save the lesson checkpoint and stop. Do not create an empty
  evolution candidate.
- If a real reusable delta exists, enter P3 and create
  `cad-agent.evolution-candidate.v2`. Bind the reproduced native artifact,
  deterministic report, materialized delta, and exactly four regression
  families: positive, negative, ambiguity, and prior release. Independent
  acceptance and later controlled-application authorization remain separate.

Historical v1 candidates and old authorization booleans are read-only. The
evolution gate never patches the Agent itself.

## Commands

```bash
bash setup.sh
bash doctor.sh --json
.venv/bin/python scripts/verify_fusion_runtime_wheel.py --installed
python3 scripts/semantic_query.py capabilities FusionMCP
.venv/bin/python scripts/cad_process_handoff.py --help
```

Use guarded `scripts/fusion_call.py` and the native CAD tools for engineering;
run formal semantic commands from the parent ontology-engineering skill.

## Boundaries

- Never invent critical dimensions, materials, loads, tolerances, units, or
  acceptance criteria.
- Never save, overwrite, or save-as a customer NX part or a user DWG through the
  dell-nb executors; inspection is read-only and deliverables go to new files
  under the Dell user's `work` directory.
- Never mutate an unrelated or identity-ambiguous Fusion document. A newly
  created practice document is allowed; name and Save As it as soon as one
  unique task/model-semantic name can be derived, and never carry it unnamed
  across unrelated work, cleanup, or any document/PID lifecycle boundary.
- Never treat screenshots, tool success, ontology conformance, or directory
  status as proof of a correct persisted CAD result.
- Keep the CAD tool registry as a bounded tool catalog; formal semantic checks use Semantica. An
  unlisted file or one successful tool call cannot promote knowledge/capability.
- Preserve cited historical evidence append-only. Corrections and new attempts
  replace interpretations, not historical bytes.
