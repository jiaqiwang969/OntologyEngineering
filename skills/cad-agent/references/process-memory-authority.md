# Process memory and authority

Preserved method from the former entrypoint. Historical command names, Fusion-write
fields and runtime-principal contracts below are provenance, not active executors.
Use parent Semantica for any formal lifecycle; no NX action follows automatically
from a memory match. The episode, applicability, revision and evidence rules remain.


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

