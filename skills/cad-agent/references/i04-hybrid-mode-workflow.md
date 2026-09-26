# I04 Hybrid workflow working memory

Use this reference after the I04 curriculum cycle and before production promotion.

## Routing rules

- Choose Hybrid when internal geometry creation and assembly structure must coexist during rapid iteration.
- Choose Assembly without Modeling Tools when internal geometry should no longer be edited at the assembly level.
- Choose Assembly with Modeling Tools only when a named cross-component edit remains required.
- Save a distinct Hybrid checkpoint before conversion.

## Fusion execution rules

- Check `DataFile.isComplete` before `Occurrences.addByInsert` and before reopening a new Save As branch.
- Do not rename the root component.
- Prefer a parameterized feature body over a Base Feature body when a later Combine must retain the tool reference.
- After body-to-component conversion, record the body and destination component names.
- For a cross-component cut, set participant bodies explicitly and verify matching topology in each body.
- Use the design timeline to recover assembly modeling features that are absent from a root feature collection.

## Ontology gates

- `designIntent` and `isModelingInAssemblyEnabled` are separate facts.
- A projection counts as linked evidence only after reference and link readback.
- A surrogate never proves geometry fidelity.
- A local F3D with external references is not self-contained without an offline import result.

Primary evidence: `curriculum-learning/I04/attempt-0001`.
