# Component and distribution notice

This notice covers the owner-approved independent core ZIP for
`manufacturing-v0.5.5`. The separate full repository archive also contains the
two books; their rights status remains recorded in
[PUBLIC-RELEASE-STATUS](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.5.5/docs/PUBLIC-RELEASE-STATUS.md). Neither archive releases
customer work or any physical product.

| Component | Included terms and source |
| --- | --- |
| Ontology-engineering core code and authored manufacturing method | Root [MIT license](../LICENSE); the frozen synthetic manufacturing package has its own [notice](../runtime/vendor/MANUFACTURING-NOTICE.md). |
| Semantica 0.6.5+oe.6 wheel | The upstream code is MIT licensed; the wheel includes the upstream license. Exact source and artifact hash are in the [source lock](../runtime/semantica-source-lock.json). Its book-derived chapter packages carry a separate `semantica/chapter_packages/NOTICE.md` inside the wheel: this owner's fork is authorized for publication, but downstream reuse of affected book-derived assets requires independent rights review. |
| Fusion execution adapter wheel | The repository owner authorizes public distribution of this owner-controlled CAD adapter in `manufacturing-v0.5.5` under the root MIT license for its authored code. Its exact wheel is pinned in the [CAD source lock](../runtime/cad-operational-source-lock.json). Autodesk Fusion and any other external software remain separately licensed and are not included. |
| Customer data, books and historical CAD cases | Excluded from the independent core ZIP. The full repository archive contains its existing books under their separate rights status; customer data and private CAD cases are not included. |

The root MIT license does not relicense third-party components or book material.
The core release asset ledger records the public decision for each bundled
file; the package manifest proves transport integrity. Neither is an engineering
acceptance or a promotion of candidate semantic packages.
