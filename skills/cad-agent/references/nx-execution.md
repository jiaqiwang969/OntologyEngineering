# Direct Siemens NX execution

Default route: reviewed NXOpen Python journal → installed `NXBIN/run_journal.exe`
→ independent native process → task-owned PRT/readback/export. No MCP sidecar,
loopback RPC, persistent journal bridge, GUI focus or Fusion process is involved.
The old bridge and historical API findings remain in
[the archived guide](legacy-20260925/references/nx-execution.md); do not execute
its `nx_call.py`/`nx_bridge.sh` commands for this route.

## Prepare and run

Copy `assets/nx-direct/profile.example.json` outside the skill and fill in the real
host and exact installed NX path. `ssh_command` is an argv array, not shell code.
In this studio use `["fleet", "ssh", "dell-nb"]`, after `fleet health dell-nb`.
The runner resolves fleet to its verified SSH argv with stdin closed, then transfers
bytes; discovery subprocesses must not consume the upload stream. The upload reads an
explicit byte/character count instead of depending on Windows SSH EOF. A completed
`stage.json` is required before dispatch. No fixed IP,
account credentials or project paths belong in the shared skill.
`remote_root` must be one stable task root for all cooperating writers on a host.
Do not change roots to evade a pending job. A recipient can use ordinary SSH.

```bash
python3 scripts/nx_direct.py probe --profile /private/nx-profile.json
python3 scripts/nx_direct.py prepare \
  --journal /project/journal.py --params /project/params.json \
  --input /project/copied-source.prt --job-id task-stage-001 --out /project/job-001
python3 scripts/nx_direct.py stage --profile /private/nx-profile.json --job /project/job-001
python3 scripts/nx_direct.py run --profile /private/nx-profile.json --job-id task-stage-001
python3 scripts/nx_direct.py status --profile /private/nx-profile.json --job-id task-stage-001
python3 scripts/nx_direct.py fetch --profile /private/nx-profile.json \
  --job-id task-stage-001 --out /project/receipt-001
```

`prepare` and `fetch` require new local directories; `stage` requires a new remote
job ID. Inputs are copied, named and hashed before execution. Proceed to `run` only after
`stage` has returned STAGED for that exact ID; a timeout is not staging success. Include the complete
assembly dependency closure; matching basenames from different directories need
explicit disambiguation before staging. Default transfer limit is 100 MiB; large
assemblies need a separately reviewed file transfer, not removal of identity checks.

The journal defines `run(session, job_dir, params)` and returns JSON-serializable
native readback. Read only `job_dir/inputs`; write new deliverables under
`job_dir/out`. Use NXOpen internally and destroy builders/dispose load/save status.
This convention is not OS containment: custom journals are trusted reviewed code.
All output geometry must still be inspected. `assets/nx-direct/native_acceptance.py`
is a bounded test, not a universal production model generator.

The launcher uses process-scoped PowerShell execution policy; it does not modify
machine/user policy, install a service, close a user's NX or alter their desktop.
A file lock serializes cooperating batch writers. Persistent STARTING/RUNNING
markers also prevent dispatch after the controller disappears. It does not lock
unrelated NX sessions or protect against noncooperating scripts. Check host load
and license capacity before scheduling; never stop another user's process.

One supplier-part upload stalled while its PowerShell reader still awaited stdin;
an empty job directory and `status.processes=[]` did not prove all upload processes
had exited (`status` filters native worker command lines). After a staging timeout,
inspect task-owned upload process identity too. A bounded recovery reused the
already verified remote PRT in a new job: transmit metadata, copy the source inside
the same host, verify every manifest byte/hash, then write STAGED. Keep the failed
job evidence and the stable writer root; do not dispatch an incomplete directory.

## Completion, timeout and persistence

The normal local wait is 120 seconds (configurable 1–300); process start and licensed
translation can take tens of seconds. Poll via the agent execution session so work
does not suppress user communication.

`COMPLETED` requires native process exit 0 and a matching successful worker receipt.
`FAILED` may leave partial task outputs; inspect them. A missing receipt, connection
loss or local wait timeout is `UNKNOWN_AFTER_DISPATCH` (exit 3), never a retry signal.
Read `status` for the SAME job and compare PID, creation time, command line, result,
stdout/stderr and output files. Do not run a second writer or delete a marker to make
progress. No automatic kill, restart or retry is provided. If a dead controller
leaves a marker, establish exact process completion and residual native state,
record a resolution, then explicitly reconcile that one task-owned marker; unknown
completion stays held. Completed/failed IDs are single-use.

To prove persistence, fetch the complete saved part closure, stage it under a NEW
job ID and inspect in a fresh NX process without saving. Match hashes, assembly
structure, occurrence transforms, parameters, solid/sheet counts, units and relevant
interfaces. Reconcile intended parameter edits with actual geometry. A repeated
part prototype is one definition and multiple occurrences. An imported STEP B-Rep
does not recover the source feature tree, constraints, material or supplier identity.

NX batch session 0 is suitable for tested operations only. Some display/selection,
drafting, translators, simulation or CAM calls need additional licensing or an
interactive session. Record per-operation version/capability evidence. If a requested
feature is untested, use an isolated recorded journal/API probe; do not silently
fall back to MCP or claim the entire NX API has been accepted.

## Version-local lessons retained

- Use expressions, sketches and datum references when they express design intent;
  keep stable semantic attributes plus prototype/component paths. Raw tags are not
  persistent IDs across sessions.
- On the measured NX 2412 binding, native bbox is
  `UFSession.ModlGeneral.AskBoundingBox`. Edge-vertex extrema underbound curved faces;
  bounds are not exact contact, clearance, containment or minimum-distance evidence.
  The native STEP round trip showed ±0.0025 mm padding in this UF envelope for an
  imported planar block; this is not measured part deformation. The bounded planar
  test also reads edge vertices after verifying all faces are planar. General curved
  parts need appropriate native exact geometry checks, not vertex extrema.
- On the same NX 2412 binding, `UFSession.ModlGeneral.AskBoundingBoxExact(body.Tag, 0)`
  returns minimum corner, directions and distances in the work coordinate system.
  The MISUMI UL3-15 import returned 15 × 3 × 3 mm while its ordinary UF envelope
  was looser on curved geometry. Record the coordinate frame and method; envelope
  padding is not supplier dimensional error. Exact bounds still do not prove fits,
  a spring's detailed winding, stiffness or production tolerance.
- Position nested components through their owning subassembly and local frame.
  A successful placement does not establish a mate or permitted DOF. Read native
  positioning constraints separately and check limits/health/behavior.
- The historical `nx_mate_component` wrapper used an invalid API; do not inherit it.
- NX 2412 STEP export needs an explicit layer mask (`1-256` for the bounded test),
  entire-part selection and `ProcessHoldFlag=True`; a valid STEP header may still
  contain zero solids. Inspect translated geometry and round-trip it. STEP AP214
  import uses the version-specific `CreateStep214Importer` factory, not an assumed
  `CreateStepImporter`. Keep API and file-content failures visible.
- Save copies/new parts only. Do not write customer originals, convert them in
  place, or mutate an unrelated open session. Keep load/update errors and unresolved
  references visible rather than treating empty results as zero defects.
- Density-derived mass needs an identified physical material; default NX density
  does not establish actual material. STEP units/tolerances require native readback.

See [method mapping](nx-method-migration.md) and
[NX variant/drawing lessons](nx-variant-drawing-review.md) for method-specific work.
