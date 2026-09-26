# Supplier CAD acquisition

## Scope

Use this reference when a task mentions supplier CAD, an exact purchased part,
a missing STEP artifact, Fusion catalog insertion, McMaster-Carr, a local
supplier STEP, or a choice between an embedded catalog and an external source.
It routes an acquisition task; it does not prove current tool capability,
authorize execution, or replace native Fusion readback.

## Normalize the user intent first

Distinguish three goals before choosing a tool:

1. **Acquire or insert exact supplier CAD in Fusion.** Normalize requests such
   as "find/add/insert this supplier part in Fusion" to the intended supplier
   CAD state transition, even when the user does not say "download".
2. **Import an existing local artifact.** Treat a present, hash-bound STEP as an
   import-and-validate task, not a reason to reacquire the same geometry.
3. **Browse public product information.** Treat an explicit request for price,
   lead time, stock, specifications, or a public product page as information
   lookup rather than CAD acquisition.

For CAD acquisition, form the typed query from:

- operation meaning, current artifact state, and desired post-state;
- exact catalog identifier when supplied, as a recipe parameter rather than the
  primary retrieval key;
- application, supplier, delivery format, interaction surface, adapter, and
  current release;
- local-artifact availability, provenance and digest state;
- relevant failures, exclusions, validation claim, and risk boundary.

Do not turn a missing STEP instance into a claim that acquisition capability is
missing. Do not turn one adapter failure into a claim that every interaction
surface has failed.

Keep target-state absence and local-artifact absence separate. “The Fusion
assembly lacks this purchased part” does not prove that no exact hash-bound STEP
already exists locally. Inspect local identity, provenance and digest before
proposing reacquisition; only an explicit local-file statement or observation
may close the local artifact state as missing.

## Run typed memory preflight

Before external search, proxy substitution, one-off browser automation, or a
new acquisition design, follow `workflow-experience-memory.md` and review prior
patterns by operation semantics, current and desired state, context facets,
surface, adapter, release, applicability, exclusions, and evidence maturity.
Do not retrieve primarily by lesson number, old catalog identifier, filename,
or literal UI text.

Interpret exact reuse, analogy, failure warning, validation guidance,
revalidation required, stale implementation, and no-match separately. A memory
result remains advisory and must report `execution_authorized=false`; the
user's bounded Fusion task or a separate governed decision supplies execution
scope.

For ordinary Chinese or English Fusion/McMaster STEP requests, run the
registered preflight before selecting a browser or supplier API:

```bash
uv run cad-agent-experience-route \
  '在 Fusion 中获取 McMaster 94459A390 的 3-D STEP' \
  --project-root . \
  --artifact-state missing \
  --current-release CURRENT_FUSION_RELEASE \
  --scope-id task:exact-supplier-cad
```

If artifact or surface state has not been observed, pass `unknown`; do not
assert `missing`, `available`, or `unavailable` merely to close the route. An
unknown surface must recall the embedded candidate and request a Fusion surface
probe while keeping external fallback false.

## Select the route in order

```text
normalized intent
  -> typed memory preflight
  -> local hash-bound STEP already present?
       yes -> inspect identity/source/digest -> propose import + native readback
       no  -> exact supplier CAD requested in Fusion?
                yes -> inspect live Fusion context
                    -> probe the current embedded catalog/surface/adapter
                    -> propose the matching internal recipe when compatible
                    -> use an external fallback only after explicit exclusion evidence
                no  -> handle the explicitly requested public-information lookup
```

### Existing local hash-bound STEP

Verify the exact part binding, supplier provenance, STEP suffix and content,
file digest, destination document, and target component before proposing an
import. Keep import, structural/BRep readback, provenance readback, save, and
reopen as separate postconditions. Do not describe an unverified local file as
authentic supplier geometry.

### Fusion embedded route

For the Chinese UI entry, see the user-provided
[McMaster-Carr entry screenshot and menu guide](../../mcmaster-entry-guide.md).
It identifies the menu item only; live instance, document and catalog state
still need their own observations.

When the requested result is exact supplier CAD in Fusion, inspect the current
Fusion document and review the matching embedded-catalog experience before any
external website search. For a matching McMaster-Carr task, retrieve the
parameterized embedded-catalog/CDP recipe by typed transition; never depend on a
particular historical lesson number or catalog identifier.

Treat historical reproduction as prior-release evidence. Probe the current
release for the intended Fusion catalog command, the expected embedded supplier
surface, a compatible adapter, and exact part/format selection. A failed AX or
other single adapter probe is adapter-local evidence and does not exclude the
embedded surface or another recorded adapter.

Passing the probe supports proposing the internal route within the user's
authorized scope. It does not inherit historical download, import, BRep,
provenance, save, or reopen results.

Before proposing an executable recipe action, require the validated tool
contract to contain exactly one semantic-operation binding for the recalled
ABox `ToolOperation`. Its public tool, action, confirmation token, interaction
surface, and non-production-routing flag must exactly match the route. A
contract-valid action with a different identity is not an implementation of
that recalled operation.

### External fallback

Use an external acquisition fallback only after recording explicit exclusion
evidence for the internal route, such as no applicable Fusion session or
catalog, an absent embedded supplier surface, a current-release compatibility
probe failure, or the exact CAD format being unavailable. Record which
condition failed, which alternatives remain, and the next validation probe.
Do not silently treat convenience, an untried adapter, or a missing local file
as exclusion evidence.

An explicit user choice of an external browser is a surface-selection override,
not evidence that the internal route failed and not an `external_fallback`.
Conversely, phrases such as "不要从官方网站查找" or "do not use the official
website" reject that surface and must never be matched as a positive external
request merely because they contain the website alias.

Bind exclusion observations to the exact review scope and content SHA-256.
The route profile must declare a minimum context policy: interaction surface
and current release are mandatory, plus at least one of the expected Fusion
document or current session identity. Missing policy keys fail closed. Do not
reuse an unavailable observation from a
different release or document.

An explicit request for current public price, lead time, stock, specifications,
or a product webpage is not a failed CAD route: it is a different information-
lookup intent and may use the official public source directly. Do not insert or
download CAD merely because the supplier also offers an embedded Fusion route.

## Preserve stage boundaries

Keep these claims independent:

```text
exact-part identity
  -> product/format evidence
  -> STEP acquisition + digest
  -> Fusion target preflight
  -> import
  -> native structure/BRep + provenance readback
  -> save/reopen persistence
```

Success at one stage never proves a later stage. Capture the selected route,
memory disposition, current-release probe, parameter bindings, exclusions,
fallback reason if any, and actual post-state in the workflow record.
