# Book package contract

## Constitutional boundary

The book is an external specification.  The two existing OntologyEngineering
volumes and every new volume retain prose, source anchors, proposition maps,
reader structure and teaching figures.  They do not retain executable
semantics.

Semantica is the sole authoritative home for:

- ontologies and controlled semantic adapters;
- competency-question definitions and acceptance oracles;
- shapes/SHACL and queries/SPARQL;
- machine cases, positive and single-fault fixtures;
- engineering rules and reasoning contracts;
- dataset lifecycle, provenance, receipts and release verification.

A book package may reference these objects by stable Semantica IDs. It must
not copy or reimplement them.

## Required outcomes

A candidate package contains five synchronized outcomes:

1. a plain-language book and reviewed teaching figures;
2. a source, chapter and proposition map;
3. a proposal and exact binding to one built-in Semantica package;
4. a reusable book Skill with explicit authority and routing boundaries;
5. release evidence for version, rights, privacy, technical/reader review and native Semantica execution.

## Minimum files

The initializer creates:

```text
book.yaml
book-charter.md
sources/source-register.csv
chapters/chapter-register.csv
propositions/proposition-register.csv
figures/figure-register.csv
semantica/package-proposal.yaml
semantica/package-binding.yaml
release/public-assets.csv
release/package-lock.csv
privacy/public-export.yaml
skill/SKILL.md
```

It deliberately does not create `cqs/`, `ontology/`, `shapes/`, `queries/`,
`cases/`, `rules/`, `fixtures/` or a runner. Raw standards, controlled
extracts, enterprise data and sessions remain outside the package and are
connected only by logical IDs and hashes.

## Book Charter fields

- target readers and assumed knowledge;
- manufacturing problem and expected learning outcome;
- permitted decisions and mandatory escalation points;
- source family, edition and applicability boundary;
- exclusions and claims the book will not make;
- domain reviewer, reader reviewer, rights/privacy reviewer and release owner;
- public/private split and rights basis;
- 10–30 proposed reader questions.

The questions in the charter are proposals. Before release, their authoritative
definitions and oracles must exist in Semantica and their stable IDs must be
listed in `package-binding.yaml`.

## Source and proposition contract

Each source has a logical ID, edition, owner, rights basis/status, private
logical locator, SHA-256 digest, distribution decision and technical review.
No private path or restricted text enters the public package.

Each released proposition has a stable ID, owning chapter, bound Semantica CQ
IDs, source IDs, non-quoting summary, claim class, authority limit, evidence
oracle description and review status. Claim class is one of
`standard-grounded`, `author-explanation`, `best-practice` or
`teaching-assumption`. Every chapter and every bound CQ is covered by at least
one reviewed proposition.

## Semantica proposal and binding

`semantica/package-proposal.yaml` reserves a deterministic package ID derived
from the book slug and requests the complete payload: ontology, CQs, shapes,
queries, cases and engineering rules. A release requires
`proposal_status: accepted`.

`semantica/package-binding.yaml` declares:

- the exact package ID and version;
- `binding_status: bound`;
- `execution_authority: semantica-only`;
- `runtime_gateway: ontology_engineering.semantica_runtime`;
- the exact source-lock, runtime-receipt and release-verdict paths;
- the exact set of Semantica CQ IDs used by chapters and propositions.

The binding is a reference contract, not a shadow package manifest. All live
calls must pass through the declared OntologyEngineering gateway.

## Semantica release evidence

A release includes exactly these three public evidence assets:

1. `release/semantica-source-lock.json`, role `source-lock`;
2. `release/semantica-runtime-receipt.json`, role `runtime-receipt`;
3. `release/semantica-release-verdict.json`, role `release-verdict`.

The source lock schema is
`ontology-engineering.book-semantica-source-lock/v1` and contains:

```json
{
  "$schema": "ontology-engineering.book-semantica-source-lock/v1",
  "book_slug": "welding-quality",
  "package_id": "semantica.books.welding_quality",
  "package_version": "1.0.0",
  "package_digest": "<sha256>",
  "runtime_version": "<installed Semantica version>",
  "runtime_commit": "<40-64 lowercase hex source revision>",
  "runtime_artifact_sha256": "<wheel/artifact sha256>",
  "source_register_sha256": "<sha256>",
  "chapter_register_sha256": "<sha256>",
  "proposition_register_sha256": "<sha256>",
  "source_hashes": {"SRC:001": "<sha256>"},
  "created_at": "<timezone-explicit ISO-8601>"
}
```

The runtime receipt and release verdict are the unmodified pure-data documents
returned by Semantica. The receipt must bind the same runtime source/artifact,
package ID/version/digest, package assets, dataset, CQ/SHACL/oracle reports and
provenance bundle. Its CQ report must cover exactly `bound_cq_ids`. The verdict
must bind that receipt, have status `complete`, contain only passed checks and
have no reasons. `blocked` is never translated into success.

`validate_book.py` validates serialization, content hashes and cross-document
bindings without importing a semantic backend. It does not replace the native
Semantica verifier that produced the verdict.

## Teaching and figure contract

Use original explanation and a synthetic manufacturing scene to answer the
reader's question. A narrative scene belongs in book prose; the corresponding
machine case and oracle belong in Semantica. Distinguish standard-grounded
requirements, author explanation, best practice and teaching assumptions.

Register each figure's visual question, semantic baseline, sources, rights,
generation method, output hash, caption, alt text, chapter, reviewer and
release status. Generated visuals are teaching candidates, never evidence.

## Release contract

`release/public-assets.csv` is a default-deny allowlist for reader books,
figures, metadata, styles, the book Skill and the three Semantica evidence
documents. Semantic payload suffixes and parallel semantic roots are rejected.
`release/package-lock.csv` freezes every package file except itself.

```bash
python3 scripts/validate_book.py <package> --stage release --write-lock
python3 scripts/validate_book.py <package> --stage release
```

Finish content, reviews, evidence and the allowlist before freezing. Any later
byte change invalidates the lock. Automated green gates prove only the encoded
contract; qualified people retain technical, rights, privacy and publication
authority.
