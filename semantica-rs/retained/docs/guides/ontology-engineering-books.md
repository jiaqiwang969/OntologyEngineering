[English (canonical)](ontology-engineering-books.md) | [简体中文](ontology-engineering-books.zh-CN.md)

# Book-grounded engineering ontology packages

This is the canonical guide for the OntologyEngineering integration included in
this fork at Semantica version `0.6.5+oe.6`. The fork integrates executable
semantics derived from the two-volume Chinese book set 《工程本体论》 and
《产品可信工程》 directly into Semantica.

**Start with the books:** [Chinese reader guide and PDFs](https://github.com/jiaqiwang969/OntologyEngineering)
· [English overview](https://github.com/jiaqiwang969/OntologyEngineering/blob/main/README.en.md)

## Why this integration matters

The books give engineers a readable way to learn ontology-engineering methods
and follow an ISO-oriented derivation. Semantica lets teams apply the reusable
parts as deterministic, inspectable packages in real engineering workflows.
Keeping those roles separate makes the method approachable to people while
keeping execution, evidence, and governance auditable by machines and reviewers.

## Responsibility boundary

- The books are the external, human-readable method and ISO-oriented derivation
  layer. They are neither a project fact source nor an executable runtime.
- Semantica is the sole executable semantics. It owns the executable ontologies,
  competency questions, SPARQL, SHACL, cases, supported rules, version snapshots,
  provenance, receipts, and release verification.
- Project tools and controlled records supply project facts and native evidence.
  Passing a semantic check does not accept those facts or expand tool authority.
- Authorized people decide conflicts, removals, risk, compliance, promotion,
  rights, and publication. No execution result or gate grants that authority.

There is no secondary ontology runtime and no fallback to book-local assets.
Unsupported SWRL built-ins, description-logic profiles, or incomplete chapter
contracts return `blocked`; they are never reported as successful.

## Package inventory

Semantica `0.6.5+oe.6` contains 29 chapter packages (9 for Vol.1 and 20 for
Vol.2) plus one normative domain package. This inventory is not a claim that
the packages are release-complete.

```python
from semantica.chapter_packages import (
    list_chapter_packages,
    list_domain_packages,
    validate_chapter_registry,
    verify_book_source_bindings,
)

assert len(list_chapter_packages()) == 29
assert len(list_domain_packages()) == 1
assert validate_chapter_registry() == ()

# Run from a checkout that contains both maintained book projects.
assert verify_book_source_bindings("/path/to/ontology-engineering").passed
```

The chapter IDs range from `semantica.chapter_packages.vol1.ch01` through
`vol1.ch09`, and from `vol2.ch01` through `vol2.ch20`. The additional domain
package is `semantica.chapter_packages.vol2.normative`.

## Execute an exact scenario

```python
import os

from semantica.chapter_packages import SemanticPackageRunner

runner = SemanticPackageRunner()
result = runner.run(
    "semantica.chapter_packages.vol1.ch03",
    runtime_commit=os.environ["SEMANTICA_RUNTIME_COMMIT"],
    runtime_artifact_sha256=os.environ["SEMANTICA_WHEEL_SHA256"],
)

print(result.status)                    # scenario execution status
print(result.release_verdict.complete)  # independent release decision
print(result.to_json())                 # backend-neutral evidence DTO
```

Scenario success and release readiness are deliberately different. Current
package manifests retain `release_status: blocked` while their declared domain
gaps remain open, even when an exact scenario oracle passes.

## CLI and MCP

```bash
semantica package list --json
semantica package show semantica.chapter_packages.vol2.ch12 --json
semantica package verify-books \
  --book-root /path/to/ontology-engineering \
  --json
: "${SEMANTICA_RUNTIME_COMMIT:?set the reviewed 40-character source commit}"
: "${SEMANTICA_WHEEL_SHA256:?set the reviewed 64-character wheel SHA-256}"
semantica package run semantica.chapter_packages.vol2.ch12 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_WHEEL_SHA256" \
  --json
```

The canonical MCP server exposes the same allowlisted control plane through
`list_chapter_packages`, `get_chapter_package`, `verify_book_sources`,
`run_chapter_package`, and `verify_chapter_package`. The top-level `mcp`
package is a compatibility view of that adapter, not a second implementation.

`verify-books` checks all 29 authoritative chapter sources, their maintainer
guides, maintained TeX sources or generated TeX snapshots, the matching
chapter contracts, and every package asset declared as derived from that chapter. A
missing file or any hash drift blocks the command. The book repository's
`scripts/rebind_semantica_books.py` is the explicit, reviewed binding-refresh
step after an intentional book edit; runtime execution
never rewrites or falls back to book-local semantics.

## Migration provenance and book builds

`read_migration_map(volume)` and `resolve_migration_successor(old_path)` retain
the exact source-to-package ledger. `package_asset_text(package_id, asset_id)`
loads a text asset only after registry and SHA-256 verification. Ambiguous old
paths preserve every candidate; unknown paths remain unresolved.

The packaged assets do not include controlled ISO originals. The normative
engraver accepts an explicit, lawfully controlled source root and an explicit
book root, emits only coordinates/modalities/released glosses and hashes, and
preflights the complete output before atomically replacing the generated
artifact set. That build operation is not external publication or a rights
decision.

## Industry refinery: fast engagement loop and slow truth loop

The books are the external method specification: Vol.1 defines the ontology
engineering method and Vol.2 demonstrates its ISO-oriented deduction. Semantica
is the sole executable semantics. A project must not execute a second copy of
the ontology, CQ, SHACL, query, rule, or case assets from the book repository.

The fast loop runs on every engineering task. It binds a task envelope, project
baseline, runtime source identity, five-part engagement result, and source
hashes. A `no_delta` result remains engagement evidence. Only a complete
engagement may create a candidate. The slow loop advances that immutable
candidate through `candidate -> proposed -> committed -> regression_passed ->
release_complete -> promoted` under explicit authority.

Every `PackageDelta` declares all eight arrays, even when an array is empty:
`ontology`, `competency_questions`, `shapes`, `queries`, `rules`, `cases`,
`contract`, and `provenance`. Replace/remove mutations require matching
per-asset decisions at commit. The contract asset is strict JSON: it assigns
runner roles to the exact CAS assets and binds the fixed regression suite to
real scenarios, CQ IDs, and positive, negative, ambiguity, and prior-release
case assets.

After commit, callers do not submit green booleans or synthetic receipts.
`execute_candidate` invokes `SemanticPackageRunner` for the contract-bound
scenarios and stores each full run result plus its native receipt. Gate evidence
is derived from that suite. Regression always contains the six `cq.*`/`case.*`
checks; release always contains the six package/capability/receipt/provenance/
rights/I/O checks. For a non-bootstrap version, prior coverage is derived from
the immutable base descriptor, manifest, projection, CQ registry, and case
objects. A target package cannot satisfy it merely by labelling a new CQ or case
as `prior`.

Every governed candidate/lifecycle write carries a `TransitionContextDTO`: one
exact action, the exact delta, a current single-action task envelope, and the
retained project binding. Candidate registration derives its context from the
candidate envelope; all later writes require it explicitly. The pre-candidate
`record_engagement` operation is the sole exception. Semantica stores each
context in CAS, binds it into the event chain, and replays it on restart.
`actor_id` remains an auditable actor assertion; authority comes only from the
binding and explicit commit or promotion authorization.
Release derivation is impossible until regression has been recorded. Retry is
also fail closed: a caller cannot substitute a new proposal or release context
for the immutable context that actually produced the event.

Execution, regression, release, and promotion each carry a deterministic
provenance closure. The closure binds source evidence, the candidate envelope,
project binding, engagement receipt, delta, manifest/projection, ordered
transition contexts, runtime identity, named provenance assets, scenario input
and output hashes, full runner results, native receipts, and native PROV bundles.
A named rights-evidence asset proves only that the bytes were bound; it is not a
legal ruling.

```python
from semantica.chapter_packages import SemanticPackageRunner
from semantica.ontology import (
    IndustryOntologyRegistry,
    TransitionContextDTO,
    commit_candidate,
    derive_gate_evidence,
    execute_candidate,
    promote_candidate,
    propose_candidate,
    verify_candidate,
)

# envelope is the retained candidate envelope. binding, engagement, delta,
# commit_authorization, promotion_authorization, and runtime_source are also
# validated refinery DTOs. make_task_envelope(action) is application-owned and
# must return a current envelope whose requested_actions is exactly (action,).
registry = IndustryOntologyRegistry.create(
    "/var/lib/semantica/industry", registry_id="factory-ontology"
)

def context(action):
    return TransitionContextDTO.create(
        action=action,
        delta_sha256=delta.delta_sha256,
        envelope=make_task_envelope(action),
        binding=binding,
    )

propose_candidate(
    registry,
    delta=delta,
    envelope=envelope,
    binding=binding,
    engagement=engagement,
    context=context("proposed"),
)
commit_candidate(
    registry,
    delta_sha256=delta.delta_sha256,
    authorization=commit_authorization,
    context=context("committed"),
)
suite = execute_candidate(
    registry,
    delta_sha256=delta.delta_sha256,
    context=context("execute_candidate"),
    runtime_source=runtime_source,
)
regression = derive_gate_evidence(
    registry,
    delta_sha256=delta.delta_sha256,
    context=context("derive_regression_gate"),
    gate="regression",
    execution_suite_sha256=suite.suite_sha256,
)
verification = verify_candidate(
    registry,
    delta_sha256=delta.delta_sha256,
    execution_suite_sha256=suite.suite_sha256,
    regression_evidence=regression,
    regression_context=context("regression_passed"),
    release_derivation_context=context("derive_release_gate"),
    release_context=context("release_complete"),
)
assert verification.state.state == "release_complete"
promote_candidate(
    registry,
    delta_sha256=delta.delta_sha256,
    authorization=promotion_authorization,
    context=context("promoted"),
)

# A later task discovers and executes the promoted package from the registry;
# no package path or book-local fallback is accepted.
reopened = IndustryOntologyRegistry("/var/lib/semantica/industry")
runner = SemanticPackageRunner()
result = runner.run_registry(
    reopened,
    delta.package_id,
    "the-contract-scenario-id",
    runtime_commit=runtime_source.runtime_commit,
    runtime_artifact_sha256=runtime_source.runtime_artifact_sha256,
    runtime_version=runtime_source.runtime_version,
)
assert runner.verify(result).complete
```

If a process stops after recording regression but before recording release,
reopen the registry and call `verify_candidate` with
`regression_evidence=None` and `regression_context=None`. Semantica reloads and
revalidates the immutable regression evidence; the two release contexts are
still required. After release is complete, a retry must supply the exact release
contexts already bound to the event.

`build_refinery_acceptance_delta(...)` is a public Semantica-owned fixture for
adapter and installation acceptance. It constructs the complete executable
eight-family surface, including the strict projection and source-evidence
binding. It is not a substitute for a domain refinery's evidence-grounded
package delta.

Promotion means admission to the local industry registry, not publication.
Distribution, standards licensing, and any external publication remain explicit
decisions of the relevant rights holder or release authority.

See `semantica/chapter_packages/NOTICE.md` for the source and rights boundary of
the bundled derivative package assets.
