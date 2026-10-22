---
title: "Project Decision Reviews"
description: "Content-bound semantic checks for evidence, challenges, and downstream engineering decisions."
---

# Project decision reviews

`DecisionReviewRunner` evaluates a project ABox against one exact Semantica
package query and SHACL shape. It does not contain a hard-coded manufacturing,
electronics, or safety ontology. The package supplies the TBox, competency
questions, `?finding` SELECT query, and shape. The project supplies a scoped
RDF snapshot and names the decision focus and its package-defined RDF type.

This addresses a recurring engineering failure: a route or design is preferred,
then new evidence challenges one of its necessary conditions, but a downstream
plan or report keeps the earlier level of certainty. A package can represent
the physical function chain separately from the evidence and decision chain:

```text
requirement -> necessary function -> failure path -> proposed control
evidence -> scoped support/challenge -> claim -> dependent output
```

An expert recommendation is a sourced ABox claim. It is not automatically a
TBox rule, a waived customer requirement, or a verified capability. Evidence
must carry object, revision, state, assumptions, and source identity; a model
warning can invalidate a model support link without asserting physical failure.
The package's shapes and queries decide which missing coverage or open,
applicable challenge blocks a stated decision level. A locally complete list
must be explicit before absence can be used as negative evidence under a local
closed-world assumption.

## Execution boundary

`evaluate_manifest` is for package authoring and controlled tests. The caller
must provide the expected runtime package digest. `evaluate_registry` resolves
only a promoted industry package and requires its exact package SHA-256 from
the project's binding. Both methods check the raw RDF SHA-256, source identity,
capture time, scope, and a focus IRI typed as the declared focus type before returning
`clear`. Exactly one instance of that type may occur in the input projection;
multiple decisions must be reviewed in separate evidence snapshots. The package
may load ontology assets, but not bundled instance or case
data into the project dataset. The result binds the query and shape asset
hashes, input and evaluated dataset snapshots, normalized findings, violations,
and a report SHA-256.

The status `clear` means **no finding in this bounded semantic projection**.
It does not prove that the function list is actually complete, a simulation
matches the manufactured object, a weld is sound, or a product can be released.
Those facts require the relevant engineering tool, test, and decision authority.
Missing or incorrect project facts cannot be repaired by a successful query.

For package authors, a decision query must SELECT `?finding`. It should emit
specific codes for missing completeness, uncovered necessary functions,
applicable challenges, stale support links, and outputs stronger than their
claims. Shapes should enforce the fields required for the package's declared
decision level. An empty query result is safe only after the package's positive,
single-fault negative, ambiguity, and prior-version cases demonstrate the
intended boundary.
