---
name: standard-to-book
description: Convert a lawfully accessed ISO, IEC, GB, industry standard, or controlled technical corpus into a privacy-safe engineering book whose sole executable semantics are a built-in Semantica package. Use when Codex is asked to start a new OntologyEngineering volume, turn a standard into a source-grounded book, or audit that book-to-Semantica release chain. Do not use to redistribute standards, bypass rights, create a parallel ontology runtime, or claim certification.
---

# Standard to Book

Produce an external specification and learning corpus, not a second semantic
implementation.  The two existing OntologyEngineering volumes and every new
book are the durable “stones”: prose, source anchors, proposition maps and
teaching figures.  Semantica owns all executable ontology, CQ, SHACL/shape,
SPARQL/query, case, rule, fixture, lifecycle and release behavior.

## Start safely

1. Read `references/privacy-release.md` before accepting sources or creating public artifacts.
2. Read `references/book-contract.md` before defining the book package.
3. Run `scripts/init_book.py` only when the target package does not exist.
4. Never overwrite, publish or push without the user's explicit authorization.

```bash
python3 scripts/init_book.py \
  --slug welding-quality \
  --title "焊接质量工程导读" \
  --standard "目标标准族" \
  --output ./workbooks
```

The initializer creates book/source registers plus
`semantica/package-proposal.yaml` and `semantica/package-binding.yaml`.  It
must not create a local CQ register, ontology, shape, query, case, rule,
fixture or runner.

Validate progressively:

```bash
python3 scripts/validate_book.py ./workbooks/welding-quality --stage structure
python3 scripts/validate_book.py ./workbooks/welding-quality --stage charter
python3 scripts/validate_book.py ./workbooks/welding-quality --stage release --write-lock
python3 scripts/validate_book.py ./workbooks/welding-quality --stage release
```

Use `--write-lock` only after the book, reviews, public allowlist and the three
Semantica evidence documents are final.  Any byte change invalidates the
lock; freezing again is a new release action.

## Build the book and Semantica package

1. **Freeze the Book Charter.** State readers, manufacturing problem, allowed decisions,
   escalation boundaries, source edition, exclusions, reviewers and public/private boundary.
2. **Register lawful sources.** Use logical IDs and hashes. Keep raw standards, restricted
   extracts, enterprise data, credentials and sessions outside the public package.
3. **Draft reader questions.** Put 10–30 proposed questions in the charter. They guide the
   book, but they are not executable CQs until accepted into Semantica.
4. **Register propositions and chapters.** Bind public summaries to sources and later to the
   stable CQ IDs issued by the Semantica package. Do not copy restricted text.
5. **Submit the package proposal to Semantica.** Implement every CQ, ontology, shape, query,
   synthetic case, rule, fixture and oracle in the built-in package identified by
   `proposed_package_id`. Do not leave a book-local fallback or compatibility backend.
6. **Bind through the sole gateway.** All Semantica discovery, execution and verification from
   OntologyEngineering must go through `ontology_engineering.semantica_runtime`; never import
   `semantica`, RDFLib, pySHACL or another semantic backend in this workflow.
7. **Capture native evidence.** Copy the source lock, content-addressed runtime receipt and
   `complete` release verdict emitted by the bound Semantica package into the exact paths in
   `package-binding.yaml`. Structural validation never manufactures a green verdict.
8. **Teach through the book.** Use original explanations and reviewed teaching figures. A
   narrative example may appear in prose, but its machine case and oracle live in Semantica.
9. **Release in layers.** Check rights, source hashes, book comprehension, technical review,
   privacy, Semantica evidence integrity, rendering and the final package lock separately.

## Preserve the constitution

Reuse source-grounding, provenance, competency-question methodology, versioned package
contracts and human authorization. Rebuild domain content for each book. Do not copy the ISO
26262 10+10 chapter structure unless the reader problem genuinely requires it.

Keep authorities separate:

- Books and controlled standards are external specifications.
- Semantica is the only owner and executor of semantic knowledge packages.
- Native records produce candidate facts; qualified people review facts, accept risk and
  authorize publication.
- A Skill routes work and an LLM translates or organizes; neither creates compliance authority.

## Stop conditions

Stop and report the gap when source rights are unclear, a decisive proposition lacks an anchor,
the target reader or decision is undefined, a real enterprise could be identified, the proposed
Semantica package is absent/unbound, the native receipt or verdict is missing/blocked, an Agent
would need broader authority, or no qualified reviewer is available. A charter or synthetic
book draft may continue only when that remains useful and is clearly unreleased.

## Completion response

Report the book package, the bound Semantica package ID/version, covered CQ IDs, source lock,
receipt and verdict status, excluded private inputs, unresolved rights/review items, validation
results and the next human decision. Never state that a book, ontology, receipt or green gate
establishes certification or product compliance.
