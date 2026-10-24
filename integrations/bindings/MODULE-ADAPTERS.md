# Module evidence adapters

This is the shared boundary for the professional modules maintained beside
OntologyEngineering v2. It does not create a second Project or replace the
native runtime of a module. A module adapter preserves the native bytes first,
then emits a strict projection and a receipt that can be checked by the normal
`oev2 project tool-result verify/register` path.

## Required handoff

Every adapter or manual handoff follows this order:

1. Freeze the current Project, object revision, condition, pending decision and
   root acceptance item with `project context`.
2. Copy the complete native output and any required input manifest into the
   selected Project private store. Do not leave an ArtifactReference pointing
   only at a tool-owned cache. Preserve the producer path or evidence ID as a
   field in the copied record and record the copied bytes' SHA-256.
3. Record the actual tool identity, invocation mode, version, executable or
   launcher digest, arguments, environment, budgets, timestamps and cleanup.
4. Emit a module projection when the native result has structure that matters
   to the engineering decision. Keep the projection's source pointers and
   native record IDs; do not flatten a graph, netlist, certificate or profile
   into prose findings.
5. Build `oev2.tool-result/1` with observed, inferred, imported and unknown
   findings kept separate. The result references the raw record and projection
   artifacts through `ToolRecord`, and keeps coverage and partial failures.
6. Run `project tool-result verify`, then `register` into a new Project
   revision. Only a separately reviewed `project decide` or `project adopt`
   can change an engineering decision.

The generic ToolResult envelope is a receipt and handoff gate. It is not a
module semantic adapter by itself: its `findings` and `records` fields cannot
replace the native structure that a decision depends on.

## Module-specific source fields

| Module family | Preserve in the raw record | Projection that is planned or implemented | Boundary |
|---|---|---|---|
| `electronics-design` | Schematic/PCB/native project, BOM, netlist, 3D model, wiring card, tool and parent-project identity | **Implemented outer bridge:** `oev2 project electronics project` → strict request/source envelope, copied source/auxiliary bytes, projection and receipt; operation payload stays in canonical source until its operation-specific contract is added | DRC, simulation or file completeness does not establish electrical function, manufacturing or adoption |
| `anatomy` + CAD | NX occurrence report, native SHA, ordered scene and renderer source | **Implemented:** `oev2 cad presentation project` → `projection.json` + `source.json` | Analytic proxy is not BRep, BOM, manufacturing evidence or engineering adoption |
| REA | Target artifact SHA, provider/session, Evidence ID, parent/page identity, static/runtime scope and coverage | **Implemented:** `oev2 project source-projection project` → typed artifact/edge/finding graph and receipt | Decompilation and static inference are not original source or runtime fact |
| LeanToken | Repository root, commit/dirty manifest, index generation, query and returned bytes | **Implemented:** `oev2 project source-projection project` → bounded source/artifact graph and receipt | Finding a test does not mean the test ran |
| GitContribute | Repository/ref, corpus coverage/freshness, exact thread IDs, live URLs, conflicts and timestamps | **Implemented:** `oev2 project source-projection project` → source/related-work graph and receipt | No-hit or incomplete coverage remains unknown |
| Jacobian | Operation schema, canonical inputs, units, assumptions, exact/approximate type, error and certificate | **Implemented:** `oev2 project jacobian project` → typed values, certificate, projection and receipt | Mathematical validity does not establish engineering applicability |
| SmokingGun + Flameox | Scan profile or capture argv/cwd/provider, baseline/candidate, workload, budgets, environment, replay and cleanup | **Implemented:** `oev2 project performance project` → typed measurements, coverage/cleanup and receipt | Static candidates, measurements, analysis and adoption remain separate |
| `engineering-memory` | Native memory session/handoff/feedback records | **Implemented natively:** `oev2 memory` and `project memory-candidate` | Memory is a candidate and feedback channel, not ToolResult evidence or adoption |
| `fleet` | Host, route, toolchain and health receipt when relevant | **No module projection:** environment condition only | Fleet state is infrastructure context, not an engineering conclusion |

## Adapter implementation rule

The native module remains the producer. Rust v2 owns path and SHA checks,
Project binding, strict decoding, coverage/unknown status, and the receipt; it
does not reimplement an external analyzer or silently rerun a tool. A new
projection is added only when the module's structure is needed downstream. It
must be specified in `specs/`, generated with the locked ggen, and tested with
success, stale-source, Project-drift, partial-result and output-path rejection
cases.
