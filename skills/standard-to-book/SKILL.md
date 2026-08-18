---
name: standard-to-book
description: Convert a lawfully accessed ISO, IEC, GB, industry standard, or controlled technical corpus into a privacy-safe, plain-language engineering book plus competency questions, source registers, ontology/SHACL packages, synthetic manufacturing cases, figure contracts, and release gates. Use when Codex is asked to turn a standard into a book, start a new OntologyEngineering volume, build AI×manufacturing knowledge for SME manufacturers, lower the barrier to a specialist domain, or audit a standard-to-book pipeline. Do not use to redistribute standards, bypass rights, or claim certification.
---

# Standard to Book

Produce a learning and verification system, not a prose summary. Keep the standard or controlled
corpus in a private evidence root; place only original explanations, synthetic examples and cleared
release assets in the public book package.

## Start safely

1. Read `references/privacy-release.md` before accepting source files or creating public artifacts.
2. Read `references/book-contract.md` before defining the book or its directory structure.
3. Run `scripts/init_book.py` to create a new package when no package exists.
4. Never overwrite an existing package or push/publish without explicit user authorization.

```bash
python3 scripts/init_book.py \
  --slug welding-quality \
  --title "焊接质量工程导读" \
  --standard "目标标准族" \
  --output ./workbooks
```

Confirm the generated structure, then use stricter stages as the work matures:

```bash
python3 scripts/validate_book.py ./workbooks/welding-quality --stage structure
python3 scripts/validate_book.py ./workbooks/welding-quality --stage charter
python3 scripts/validate_book.py ./workbooks/welding-quality --stage release --write-lock
python3 scripts/validate_book.py ./workbooks/welding-quality --stage release
```

Use `--write-lock` only after the reader book, proposition map, teaching figures, ontology artifacts,
released book Skill, machine test report, reviews and public-asset allowlist are complete. Any later
change invalidates the lock; regenerate it only as an explicit new freeze.

## Build the book

1. **Freeze the Book Charter.** State the target SME manufacturing readers, shop-floor problem,
   allowed decisions, forbidden conclusions, standard edition, scope, exclusions, reviewers and
   public/private boundary.
2. **Register sources and rights.** Use logical IDs and hashes. Do not store personal absolute paths,
   credentials, sessions, raw standards or restricted extracts in the public package.
3. **Write competency questions first.** Capture what a non-specialist engineer must understand,
   decide, verify or escalate. Make each CQ testable.
4. **Register propositions.** Bind each plain-language claim to its chapter, CQs, sources, claim
   class, authority limit, evidence oracle and reviewer. Do not copy restricted standard text.
5. **Conceptualize the domain.** Define terms, classes, relations, identity criteria, lifecycle,
   versions, evidence and authority. Keep TBox, controlled ABox and real-world facts distinct.
6. **Create executable answers where useful.** Add exact queries, SHACL or equivalent constraints,
   positive fixtures, single-fault negative fixtures and a runner. Do not build a universal ontology.
7. **Teach through synthetic manufacturing scenes.** Let a plausible misunderstanding happen,
   expose why it fails, then present the proposition, source-grounded explanation and boundary.
8. **Produce figures from contracts.** Define a visual question and semantic baseline before using
   ImageGen or another provider. Treat generated images as candidate teaching visuals, never evidence.
9. **Assemble and release in layers.** Check sources/rights, semantics, fixtures, reader comprehension,
   expert review, privacy, metadata, PDF rendering and frozen checksums separately.

## Preserve the constitution

Reuse competency-question methods, provenance, package shapes, gates and human authorization.
Rebuild domain semantics, facts, sources, rights, cases, conclusions, visuals and review decisions for
every new book. Do not copy the ISO 26262 10+10 chapter structure unless the new teaching problem
actually requires the same mirror.

Keep these authorities separate:

- Ontology and contracts define shared meaning.
- Controlled activities and native records produce candidate facts.
- Qualified people review, accept risk and authorize release.
- LLMs translate and organize; Skills route the workflow; Agents execute only within granted scope.

## Stop conditions

Stop and report the gap when source rights are unclear, a decisive claim lacks an anchor, the target
reader or decision is undefined, a real enterprise could be identified, an Agent would need broader
authority, or no qualified reviewer is available. Continue with a charter or synthetic demo only when
that remains useful and honest.

## Completion response

Report the created or updated package, the CQs and gates covered, private inputs that were deliberately
excluded, unresolved rights/review items, validation results and the next human decision. Never state
that a book, ontology or green test establishes certification or product compliance.
