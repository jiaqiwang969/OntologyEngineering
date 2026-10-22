# Tool integration fixture

This owned sample provides `joinLabels` and `fixtureResult` for source retrieval,
static complexity inspection and inspection of an explicitly packaged JavaScript
artifact. Expected result: `["beta", "alpha", "missing"]`.

Copy the directory into a private test workspace before indexing. GitContribute
requires a clean committed checkout; initialize only that copied fixture and use
the synthetic repository identity `example/ontology-engineering-tool-fixture`.
This identity is a local test label, not a claim that a GitHub repository exists.

For REA, copy only `package.json` and `src/lookup.js` into a separate `package/`
directory and inspect that explicit release fixture. Static inspection does not
execute its functions or establish their runtime behavior.
