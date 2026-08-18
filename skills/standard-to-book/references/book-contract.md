# Book package contract

## Required outcomes

A candidate book package contains five synchronized outcomes:

1. a plain-language book and teaching figures;
2. a source and proposition map;
3. competency questions and executable checks where appropriate;
4. a reusable Skill workflow with explicit authority boundaries;
5. release evidence for version, rights, privacy, technical review and reader review.

## Minimum files

The initializer creates:

```text
book.yaml
book-charter.md
sources/source-register.csv
cqs/cq-register.csv
chapters/chapter-register.csv
propositions/proposition-register.csv
ontology/package-manifest.yaml
figures/figure-register.csv
release/public-assets.csv
release/package-lock.csv
privacy/public-export.yaml
skill/SKILL.md
```

The machine test report is created only after the ontology runner and declared checks have actually
run. Register it as a `test-report` public asset together with the reader book, teaching figures,
ontology artifacts and the released `SKILL.md`.

Keep raw standards, controlled extracts, enterprise data and sessions outside this package. Connect
them with logical IDs and hashes only.

## Book Charter fields

- target readers and assumed knowledge;
- manufacturing problem and expected learning outcome;
- permitted decisions and mandatory escalation points;
- standard family, edition and applicability boundary;
- exclusions and claims the book will not make;
- domain reviewer, reader reviewer and release owner;
- public/private split and rights basis;
- initial 10–30 competency questions.

Do not start bulk writing or figure generation before these fields are reviewable.

## Competency-question contract

Each CQ declares the natural-language question, reader decision, evidence needed, expected answer
form, acceptance oracle and status. A chapter may answer several CQs; a CQ may span several chapters.
Chapter count and narrative structure follow the reader path, not a fixed template.

## Proposition contract

Each released proposition has a stable ID, owning chapter, CQ IDs, source IDs, a non-quoting public
summary, claim class, authority limit, evidence oracle and review status. Claim class is one of
`standard-grounded`, `author-explanation`, `best-practice` or `teaching-assumption`. Every released
chapter and CQ is covered by at least one reviewed proposition. References are semicolon-separated,
unique and must resolve inside the same package.

## Executable knowledge package

For a claim advertised as machine-answerable, provide the equivalent of:

- a package manifest and conceptualization;
- a self-contained TBox and controlled ABox/adapter;
- source anchors and proposition IDs;
- exact queries and expected bindings;
- SHACL or equivalent constraints;
- positive fixtures and single-fault negative fixtures;
- an isolated runner and compact report.

Package isolation is preferred. Cross-book mappings live outside the package and must not silently
grant facts, conformance or authority.

## Teaching contract

Use a synthetic manufacturing scene to answer the reader's question. Distinguish standard-grounded
requirements, author explanation, best practice and teaching assumptions. State what changes the
answer and what still requires expert judgment.

## Figure contract

Register visual question, semantic baseline, referenced inputs, rights status, generation method,
output hash, caption, alt text, chapter use, reviewer and release status. A visually attractive image
that fails the semantic baseline is rejected.

## Release contract

A release candidate binds the book, source map, ontology packages, tests, figures, Skill and manifests
to one frozen version. Automated green gates prove only the encoded contract; human reviewers retain
technical, rights, privacy and publication authority.

`release/public-assets.csv` is the default-deny allowlist for reader, figure, ontology, query,
constraint, fixture, script, Skill and test-report artifacts. `release/package-lock.csv` freezes every
file in the package except itself, including all registers and policy files. Finish the content,
reviews, assets and machine report first; then write the lock and validate:

```bash
python3 scripts/validate_book.py <package> --stage release --write-lock
python3 scripts/validate_book.py <package> --stage release
```

Any later byte change invalidates the lock. Re-freezing is a new release action and does not replace
technical, rights, privacy or publication review. The test-report gate validates its declared command,
tool, timestamp, runner/manifest hashes and exact CQ/proposition coverage; it does not independently
prove that the domain model or book is substantively correct.
