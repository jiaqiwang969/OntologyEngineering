# Fusion release compatibility and instance routing

The current runtime is `0.1.0+oe.9`, admitting the exact audited releases through
`2705.1.25`. Historical observations below retain their original scope.
Multi-instance selection and background UI rules are documented in
[fusion-multi-instance.md](fusion-multi-instance.md).

The `0.1.0+oe.2` Fusion-only runtime admits the exact release `2705.1.15`.
The preceding catalog admitted only `2704.1.36` and `2704.1.53`; an installed
MCP transport could therefore list tools while native calls were rejected.

## Scope of the change

- A PID-bound, expiring, owner-only audit manifest permitted two fixed guarded
  observations: the native tool catalog and a `projects` read. The audit helper
  exposes no document mutation or arbitrary script surface.
- Every previously published schema field retained its type and constraints.
  Native additions were not added to the published CAD catalog.
- The versioned wheel changes only `fusion_mcp_proxy/tool_catalog.py` among
  runtime Python files, plus package metadata and a source-evidence JSON record.
  Guard, owner lease, session, acknowledgement and retirement implementations
  retain the previous bytes. Unknown releases remain rejected.
- Normal registered MCP execution subsequently passed a `projects` read.
  Canonical guarded calls also passed an open-document read and an enforced
  read-only Python script. All three returned `delivered_ok` and confirmed ACK.
  No geometry mutation was tested; this does not certify every CAD operation.

The wheel contains `fusion_mcp_proxy/release_audit_2705_1_15.json` with native
schema and raw audit-report hashes. Private process, project and document
details remain in the operator's local evidence archive.

## Maintenance path

Use `scripts/fusion_release_audit.py` only for an explicitly authorized release
compatibility investigation. Review its manifest requirements before use.
Never turn an unsupported-release rejection into an unrestricted allowlist.
The audit uses the same guarded cell, owner actor and canonical safety state;
timeout or ambiguity still retires the PID for further MCP traffic.

`scripts/build_fusion_2705_compat.py` deterministically builds this specific
delta from the hash-locked `oe.1` wheel and the two successful guarded reports.
It checks the exact stage arguments, ACK and schema compatibility. It is not a
generic future-release admission tool. Keep the former wheel outside active
`dist/` for rollback; retain one locked active wheel.

After a reviewed runtime upgrade, update the wheel lock, setup filename and
`BUILD_INFO` runtime identity together. Preserve historical migration-source
fields. Re-review changed operational source hashes, run the negative guard
and transport tests, verify installed wheel bytes, run the strict Semantica
backend policy check and the external doctor, then refresh the canonical
content digest. Never mark an untested geometry operation as validated.

## Verified read-only call shapes

Read-operation fields belong at the top level of the native read schema:

```json
{"queryType":"document","operation":"open"}
```

Here `open` lists already-open documents. The error text referring to an
“object” is an internal dispatcher message; nesting `operation` under an
`object` property does not supply the required native field.

Python scripts must define `run` with exactly one argument. Put inspection
inside it, print the final JSON there, and use `fusion_ops.py script ...
--read-only`. A top-level script may print a result and then fail because the
required `run` entrypoint is absent; printed state alone is not success.
Keep the required preservation-role declaration at the top level:

```python
FUSION_MCP_UNTITLED_PRESERVATION_ROLE = "inspect"

def run(_context: str):
    # Only read native state; no executeTextCommand or document mutation.
    ...
```

Inspect `classification`, native result and ACK for every call. Missing fields
and script exceptions are failed observations; preserve their records, correct
the cause, and follow the independent readback discipline before proceeding.

## Reliability migration after oe.2

The August migration wheel did not contain the September runtime patches even
though the copied runbooks described them. In particular, exact window-origin
comparison could reject macOS Space animation, the offline AppLog diagnostic
module was missing, and the long-request CLI exceeded the installed runtime's
accepted arguments. A later successful native read did not test these cases.

`oe.3` restores only the reviewed operational changes: bounded window census
settling, process-bound blank-window evidence caching, local fallback for
Peekaboo transport faults, separate activation/request budgets, explicit long
requests, sleep detection, and the offline AppLog diagnostic module. It keeps
the previous exclusive marker creation and fail-closed retirement behavior,
and the previous modified/untitled document checks. Archived marker bypasses,
automatic marker lifting, rotation, and disabled preservation checks were
excluded. The explicit offline recovery command remains evidence-gated by its
AppLog conditions; health, identity, and window failures are ineligible.

`scripts/build_fusion_reliability.py` rebuilds the versioned wheel from the exact
`oe.2` wheel plus hash-locked patches in `runtime-patches/reliability-20260924/`.
It checks every output module hash. The review passed 286 offline regressions;
8 historical-base comparison cases were skipped because their original lookup
path was archived. Two builds matched byte for byte and an altered base was
rejected. Installed package bytes match all 28 locked members. Three external
window checks passed, including observed origin jitter. Those external checks
do not establish native MCP health or authorize reuse of a failed health PID.

`oe.4` additionally moves health-attempt reservation after the external
process/window preflight. Refusing before any cell is launched is an external
precondition failure; it does not consume a native health attempt. From the
reservation/challenge boundary onward, failures and previous native attempts
remain terminal. The controller regression suite passes 57 checks, including
a transient preflight followed by exactly one native call, and rejection of
transport, identity, and attestation failures. A real new-process challenge,
document read, delivery ACK, and identity receipt subsequently passed.

Rebuild this bounded delta with `build_fusion_reliability.py --patch-set
health-preflight-20260924` and the hash-locked `oe.3` base. Do not erase or reuse
an old native-attempt record based on a window screenshot. One historical
pre-challenge refusal was separately reclassified using its exact report,
attempt record, code ordering, process identity, and absence of any challenge
or native ambiguity record; its original bytes remain archived.


## Exact instance selection and 2705.1.25 (2026-09-24)

`oe.5` adds an explicit endpoint/PID/high-resolution start-token binding to the
existing one-shot caller and cell. Only exact non-privileged IPv4 loopback MCP
URLs are accepted. Listener ownership and process lifetime are bracketed;
selected-window and actual screenshot IDs require independent CGWindow owner
proof. Same-PID owner leases, shared safety state, retirement and modified or
untitled preservation remain enforced. The candidate regression run recorded
358 passes and 8 archived historical-base skips. The companion external
preflight/F1 wrapper change passed 97 checks including independent wrong-PID,
wrong-window and listener-transfer counterexamples. F1 retains its global busy
check; create documents sequentially before ordinary per-instance calls.

`oe.6` changes the exact release catalog to also admit `2705.1.25`. Its native
catalog and fixed projects read passed through the PID-bound audit helper with
confirmed ACKs. The audit now derives the loopback endpoint from the manifest's
observed listener; it still exposes only catalog/projects. A separate review
passed 53 helper tests. `build_fusion_2705_1_25_compat.py` binds both successful
reports to the same helper/manifest command and preserves all published schema
bytes. Forty compatibility checks passed. Two independent deterministic builds
matched; all 33 retained runtime members other than catalog/metadata/RECORD
were byte-identical to oe.5. The new proof is
`fusion_mcp_proxy/release_audit_2705_1_25.json`.

An isolated design on the second selected process subsequently passed F1:
exact-name collision search, native creation, saveAs and fresh-session readback.
All four calls were delivered_ok with ACK; the result was version 1, saved and
clean. This establishes document creation/persistence on that instance, not
concurrent geometry or production fixture correctness.

Peekaboo 4.5.0 was evaluated separately from the guard's existing signed 3.2.1
read-only path. Its exact-window background capture and official signed
build-scoped producer worked. Background pressing of the tested Fusion
Preferences button failed (no pressable AX element), leaving the panel present.
The action reported indeterminate delivery; external observation was taken and
no repeat or foreground fallback was attempted. Foreground samples remained on
the user's terminal throughout that test. This is a real UI limitation, not
permission to use foreground input or declare parallel modeling accepted.


The second-instance geometry test then created twelve independent stepped
cylinders (24 extrusions/24 sketches), saved version 2, and passed a separate
read-only native check of exact lineage, version, model marker, name sets,
solid counts, dimensions and analytic volume. Both calls returned delivered_ok
with confirmed ACK. This one-instance result does not establish overlapping
two-agent modeling or a speedup; the other instance's Preferences panel remained
open and was not used for native calls.

## Exact Preferences panel blocking (oe.7)

The runtime and external preflight now reject the exact `Preferences` and
`首选项` panel titles after their existing whitespace/case normalization.
Document canvas titles such as `Preferences - Autodesk Fusion` remain distinct.
Only those two entries were added to the title sets; process binding, owner
leases, document preservation, request budgets and retirement methods are
unchanged. The reviewed 49-case offline suite includes initial, settling and
second-preopen panels for both read and execute requests, with zero upstream
initializations or native operations in the rejected cases.

Two deterministic builds matched and the installed members were verified.
Fresh external preflight then blocked instance A for `首选项` while instance B
was ready; neither check opened an MCP connection or changed UI focus. This
prevents sending requests behind the panel; it does not close the panel or
establish two-instance concurrent modeling.

## Closed Preferences and origin settling (oe.8 / oe.9)

Later read-only AX observations exposed the exact enabled Preferences button.
One snapshot-bound Peekaboo 4.5.0 `actionOnly` background action dismissed it;
fresh AX and OS ordering observations confirmed the result. Fusion retained the
closed window in CG listings, so a title alone was insufficient to distinguish
that retained record from a live panel. No foreground fallback was used.

oe.8 adds a shared, fresh proof for only raw `Preferences` / `首选项` titles:
three CG identity samples, complete unfiltered ownership, process-start tokens
before/after, two ordered-out observations and a unique primary canvas observed
ordered-in twice. Off-screen status is not an exemption. Missing or uncertain
private macOS ordering APIs fail closed. The first installed guard exposed
transient common coordinate shifts between Peekaboo and CG observations.

oe.9 classifies only a fully identified common translation of that exact pair
as a reason to fetch a completely new census, within the existing three attempts
and shared deadline. Success still requires every origin to match exactly.
Non-origin identity, title, size and ordering failures reject immediately. No
new persistent state, focus operation or native retry was introduced. The final
candidate passed 21 regression methods and 12 independent boundary cases; two
builds matched, and all 36 installed package members matched the locked wheel.

## Two actual agents, two native rounds accepted (2026-09-24 R2)

Selected Fusion 2705.1.15 and 2705.1.25 processes operated on distinct saved
document lineages. Agent A built stepped rectangular solids and Agent B built
stepped cylindrical pins. Each built 96 bodies, saved/read back v2, then added
48 and saved/read back clean v3: 144 bodies, 288 sketches and 288 extrusions per
document. Four builds and four independent readbacks were delivered_ok with
confirmed ACK and successful exact native geometry assertions, without retry.

The native modeling intervals overlapped for 55.707687 and 69.064195 seconds.
Both workers recorded actual feature progress inside each overlap. Exact name
sets and per-body geometry matched each worker; the first 96 bodies were
unchanged after the second round. Original open-document versions and native
geometry summaries were unchanged. Foreground samples contained only the user's
terminal. Evidence package: `fusion-parallel-acceptance-20260924-r2`, result
`PASS_TWO_ROUNDS`.

This qualifies the tested independent-document parametric modeling workflow,
not same-document concurrency, arbitrary UI/plugins, complex assemblies,
long-duration operation, CAM, NC or production fixture correctness. No serial
baseline was measured, so no speedup factor is claimed. This execution-layer
test makes no engineering semantic release (`no_delta`).
