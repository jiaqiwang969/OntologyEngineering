# Workflow experience memory

> Current execution uses NXDirect and MISUMI_CN from data/execution-policy.json. The historical cad-agent-memory/semantic commands and Fusion/MCP supplier bindings below are method/provenance examples, not available default commands or active routing. Use parent Semantica for formal memory and retain applicability/evidence rules; no historical condition can reactivate an executor.

Use this route for every new lesson or engineering task after the requirement
ledger is known. It is the cross-course process-memory entry point; lesson
references remain useful source guidance but are not the only retrieval index.

For current operational practice, use the parent's
[consolidation method](../../../references/practice-consolidation.md). Supplier
acquisition starts from the [current guidance index](supplier-acquisition-experience.md):
read applicable text into the next decision, bind its digest in the private task,
and record actual adoption or rejection. This is instruction maintenance and
advisory lookup, not a replacement for formal Semantica retrieval/activation or
an invocation of the historical commands below.

## Start-of-work problem and memory review

When a blocker, unexpected state, missing resource, adapter failure, or
validation gap appears, first form this non-authorizing chain:

```text
ProblemSituation
  -> ProblemClassification
  -> ordered ProblemDiagnosticRoute
  -> MemoryReview
       -> PriorPatternAssessment or explicit NoPriorPatternMatch
       -> ResolutionHypothesis
       -> ValidationProbeSpecification
```

Ground the situation in a content-addressed observation. Use controlled current
conditions to distinguish, for example, `MissingArtifactInstanceProblem` from
`MissingCapabilityProblem`. The former should still route through capability
availability and interaction-surface review: it asks whether an old capability
can obtain this missing instance; it does not declare that capability absent.

The observation JSON must use schema `cad-agent.problem-observation.v1` and
carry the same `scope_id` as the review, an informative statement, a timezone-
qualified `observed_at`, and the controlled `observed_conditions` it directly
supports. Those conditions must come from current/context/failure inputs; a
goal condition is never evidence of the current problem.

When the observation controls an interaction-surface exclusion or executable
route, also bind `observation_context` to the exact surface, current tool
release, expected document, and session when available. A same-scope digest is
not sufficient if the observation came from another release or document.

Run `cad-agent-memory-review` when this reflective classification is needed.
`EVIDENCE_REQUIRED`, `EVIDENCE_INVALID`, `EVIDENTIAL_SUPPORT_INCOMPLETE`,
`CLASSIFICATION_INCOMPLETE`,
`CLASSIFICATION_AMBIGUOUS`, `CLASSIFICATION_REVIEW_REQUIRED`, and
`DIAGNOSTIC_ROUTE_INCOMPLETE` are stop-and-investigate results. Stale,
ambiguous, review-required, release-required, and invalid memory retrievals are
also non-ready. A ready result
still has review, execution, pattern-admission, and ontology-mutation authority
set to false.

Describe the task using:

- intended operation semantics;
- current state conditions and observed failures;
- desired postconditions;
- participant roles and invariants as controlled applicability conditions;
- domain context as generic `key=value` facets (supplier and format are only
  two possible facets);
- application, adapter, interaction surface, and release;
- claim to validate and risk boundary.

Retrieve by those typed dimensions before external search, proxy geometry,
one-off automation, or a new recovery design. Object names, lesson IDs, file
paths, catalog identifiers, and literal UI labels are provenance or parameter
bindings, not the primary retrieval key.

When a hard current condition has not yet been observed, first run candidate
discovery by operation semantics, desired post-state, and generic context
facets. Use the returned PRE, exclusion, surface, adapter, and release fields to
choose the least expensive observation. Never insert the expected PRE into the
current state merely to make strict retrieval succeed. Enter strict
`MemoryReview` only after same-scope, content-addressed evidence supports the
current conditions.

For a registered high-frequency intent profile, use
`cad-agent-experience-route` as the natural-language preflight. Its action
proposal remains non-authorizing, and the command is now a compatibility
canary rather than the target runtime authority. For a P3 generic route, compile
the reviewed package/RecognitionAdapter with `cad-agent-semantic-build`, assess
it with `cad-agent-semantic-runtime-route`, and bind the resulting request to one
`cad-agent.task.v2` as `semantic_runtime_route_request` path + SHA-256. The
Agent must then skip the legacy router, report `candidate_consumable=false`,
and require active-pointer/action-link/policy/executor rechecks. An external
fallback is eligible only when
current evidence supports the selected pattern's exact exclusion condition;
an unknown surface, missing local artifact, or one failed adapter is not such
evidence. The profile-owned fallback context policy must also be complete;
for the Fusion embedded supplier route this means surface + release + either
document or session, all matched by the hashed observation. A concrete action
proposal is emitted only after its full argument
instance passes the public MCP input schema; field presence in a route binding
alone is insufficient.

Interpret the four result baskets separately:

- `EXACT_REUSE`: all closed hard conditions and boundaries match; still check
  canonical plan, parameter schema, condition/facet validators, any applicable
  implementation digest, current release, target identity, and session scope;
- `ANALOGICAL_INSPIRATION`: use the shared structure only after reading every
  pattern statement, mismatch, adaptation, risk, and suggested probe;
- `FAILURE_WARNING`: honor its trigger, no-retry boundary, diagnostic probe,
  and separately validated recovery route;
- `VALIDATION_GUIDANCE`: reuse the check design, never a historical PASS.

`AMBIGUOUS`, `INPUT_INVALID`, `REVALIDATION_REQUIRED`,
`STALE_KNOWLEDGE_DEFINITION`, and `STALE_IMPLEMENTATION` are reasons to narrow
or test a route. A retrieval result
always has `execution_authorized=false`; ordinary P1/P2 work still derives its
scope from the user's task, while P3 keeps its separate decision chain.

## During practice

Record meaningful engineering stages, not every API call. Preserve:

- PRE/POST situation evidence;
- ordered stage or GroundedExperience references;
- decision alternatives and rationale;
- failed branch, cause hypothesis, recovery, and no-retry constraint;
- validation strategy, coverage, outcome, unknowns, and environment identity.

When a live observation contradicts a retrieved pattern, preserve the episode
and narrow or invalidate the pattern candidate; do not silently adapt history.

Validation results are revision-scoped workflow evidence. Whenever a stage
changes an upstream subject revision, record which downstream consumers depend
on it, bind a consumer-owned validation profile, and rerun those checks on the
new revision. For a cross-package continuation, preserve the exact package
identities, input/output mapping, both revision identities, and a
content-addressed completion receipt. A prose note that a later stage “should
recheck” is incomplete process memory and cannot satisfy a guard or inherit a
prior PASS.

## End-of-work compilation

Validate the process record with:

```bash
uv run cad-agent-workflow-experience \
  curriculum-learning/SXX/attempt-NNNN/workflow-experience.json \
  --artifact-root .
```

The compiler can propose execution recipes, modeling strategies, recovery
patterns, validation strategies, decision heuristics, and failure warnings.
Failed or partial workflows may become warnings or inspiration but cannot
become an exact successful recipe. Candidate compilation never activates a
shared graph and never authorizes execution.

Every candidate intended for retrieval must carry a concise
`patternStatement`. It states what later work may execute, adapt, avoid, or
validate; a label, lesson number, status, or opaque URI is not sufficient
inspiration.

Only a genuine cross-task delta proceeds through P3 semantic activation with
source evidence, applicability and exclusion boundaries, SHACL/CQ validation,
positive/negative/ambiguity/prior-release regressions, and independent review.
Use `semantic_memory_control` committed intake/materialization/feedback
snapshots and pointer-CAS plans to make that lifecycle explicit. The pointer
plan binds the independent activation decision and expected old state first;
only a later external atomic transition may produce a receipt, followed by
independent reopen verification. A planned target cannot serve as prior state:
only a committed wrapper whose full-state digest, generation/predecessor,
external receipt, and readback relation reproduce may feed the next append or
materialization plan. Distinct actor/verifier strings are not an active identity
trust proof. These assessment functions do not scan, write, materialize, or
move an active pointer.

Application feedback is not a free-form summary. The v2 feedback manifest may
name only a controlled `WorkflowExperienceRecord`, one `application_id`, and
the exact content-addressed operational-pattern source package. The queue
consumer must locate the `PatternApplication` itself, derive its controlled
feedback outcome, and require its frozen `pattern_uri + pattern_version +
package_id + package_version + package_source_sha256` identity to match the
package bytes. The builder also checks the application assessment/evidence
bytes. A v2 committed-snapshot assessment must receive the project
`source_root`, reopen the record, package, and application evidence, and
reproduce the application digest, exact package identity, and outcome/action
mapping; a self-reported `VERIFIED` summary or recomputed wrapper digest is not
evidence. The source manifest itself is embedded and hash-bound. Generation 2+
assessment must also receive the exact predecessor committed snapshot and
prove that prior entries are byte-identical and that `target - prior` equals
the current manifest additions; the queue identity itself cannot change across
generations. PatternApplication identity uniqueness is rechecked both while
building and while reopening committed state. Replaying an application under
a new feedback ID is rejected; corrections require a new application identity.
It may then plan a maintenance or revalidation item, with every
promotion, maturity, retrieval, materialization, index, activation, execution,
Fusion-write, routing, and pointer-change field false.
Legacy v1 queue snapshots remain readable by their frozen compatibility
Schema, but v1 arbitrary-feedback inputs cannot create new v2 work or satisfy
the verified application-binding claim.

Workflow v1 canonicalization omits `applied_pattern` and
`active_memory_use` only when their values are absent. This preserves the
historical digest of pre-extension records while new records retain their exact
package and application-time selection identities in the digest.

Keep two provenance edges distinct: `applicationAppliedPatternPackage` says
which exact pattern revision was tested; `applicationUsedActiveMemorySelection`
says which host-pinned selection was actually consumed at application time.
Never infer the second from the first, from retrospective matching, or from a
read-only selection report. The bounded host-descriptor candidate consumer now
revalidates the selection, routes through its exact build, rejects caller build
overrides and ambiguity, and can emit a preflight-bound application plus a
content-addressed consumption receipt. Selection, consumption, and feedback are
bound to distinct signed clock events from one scoped provider registration;
event/nonce identity and sequence advance strictly while coarse timestamps may
be equal. The consumption uncertainty window is a
`RuntimeConsumptionObservation`, not a claimed operating-system process run.
The receipt is occurrence evidence and cannot validate itself. Feedback v2
deliberately does not accept, independently verify, or reopen that receipt.
Until the feedback control plane does so and an external commit plus fresh
reopen proves queue persistence with an unchanged active-memory closure,
CQ-EA11 remains `UNPROVEN`.

After materializing or changing Pattern/Recipe memory, run:

```bash
uv run cad-agent-experience-linkage-audit --project-root .
```

Treat `GUIDANCE_MEMORY_ONLY` as a valid non-executable memory state.
`ACTION_ROUTE_PROFILE_MISSING`, `ACTION_ROUTE_BINDING_BLOCKED`, and
`NON_RECIPE_ACTION_ROUTE_INVALID` expose compilation/linkage defects;
`ADVISORY_ACTION_PROPOSAL_LINKED` means the legacy semantic/public-tool proposal
closes only as a candidate canary. `ACTIVE_ACTION_ROUTE_LINKED` is reserved for
an independently active, production-routing-eligible semantic binding; neither
status itself authorizes execution.
