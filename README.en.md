[简体中文](README.md) · **English**

# Ontology Engineering: Make Engineering Knowledge Readable, Searchable, and Verifiable

<p align="center">
  <a href="references/ontology-engineering-book/handbook/工程本体论-全书.pdf">
    <img src="docs/assets/engineering-ontology-cover.png" width="320" alt="Cover of Engineering Ontology, Volume 1">
  </a>
</p>

<p align="center">
  Two books, one disciplined way to reason about engineering meaning—from project evidence to reusable, auditable industry ontologies.
</p>

<p align="center">
  <a href="references/ontology-engineering-book/handbook/工程本体论-全书.pdf">Read Volume 1</a> ·
  <a href="references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf">Read Volume 2</a> ·
  <a href="https://github.com/jiaqiwang969/semantica">Explore Semantica</a> ·
  <a href="#where-to-start">Choose a reading path</a> ·
  <a href="#a-five-minute-tour">Take the five-minute tour</a> ·
  <a href="#technical-governance">Technical notes</a>
</p>

Engineering teams rarely suffer from a shortage of documents. The harder problem is shared meaning:
which object and version are under discussion, what a piece of evidence actually supports, how far a
successful check can be trusted, and who is accountable for the final decision.

Ontology Engineering brings those questions into one coherent practice. The two books teach how to
observe and model an engineering domain. Native project records remain the source of facts. Semantica
makes the semantics executable, reproducible, and durable. Authorized people retain responsibility for
accepting facts and risks, resolving conflicts, promoting shared knowledge, and publishing it.

## Two volumes, two complementary questions

| Volume | The question it answers | What you gain |
|---|---|---|
| [Volume 1, *Engineering Ontology* (`工程本体论`)](references/ontology-engineering-book/handbook/工程本体论-全书.pdf) | How do we turn ambiguous engineering language into a conceptual system that can be tested? | A general method for objects and identity, relations, competency questions, open- and closed-world reasoning, constraints, inference, provenance, and ontology-guided agents |
| [Volume 2, *Trustworthy Product Engineering* (`产品可信工程`)](references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf) | How can an engineering team explain why a product deserves trust? | An ISO 26262 ontology-engineering walkthrough and ten reusable lenses: claims, identity, governance, contextual hazards, requirements, measurement, change, dependency, field evidence, and assurance |

Volume 1 provides the reusable grammar; Volume 2 shows that grammar at work in difficult product
decisions. The people, incidents, EPS-RC17, ENV-01, and numerical values in Volume 2 are synthetic
teaching material. Exact ISO clauses, tables, and wording must be checked against a lawfully held,
controlled source. This project is not an official interpretation, certification, or conclusion about a
real product.

## Who this is for

- Engineers and technical leads who need sharper boundaries around objects, terminology, versions,
  evidence, and responsibility;
- teams building enterprise knowledge graphs, industry ontologies, digital threads, or engineering
  knowledge systems;
- developers who want LLMs and agents to operate within explicit semantic, evidence, and authority
  boundaries;
- reviewers who must decide whether a green check genuinely supports a risk, compliance, or release
  claim;
- readers who want methods, worked examples, and reproducible semantics in one learning path.

## Where to start

| Your goal | Suggested route |
|---|---|
| Learn ontology engineering from first principles | Volume 1, Chapters 1–3: why ontology matters, core concepts, and how to begin with competency questions |
| Work on RDF/OWL, constraints, or reasoning | Volume 1, Chapters 4–5 and 7, then corroborate the examples with their Semantica chapter packages |
| Understand semantics for LLMs and agents | Volume 1, Chapter 8, followed by the engagement rules in [`SKILL.md`](SKILL.md) |
| Build a trustworthy-product or functional-safety evidence chain | The preface and Chapters 1–10 of Volume 2, then the paired ontology answers in Chapters 11–20 |
| Apply the method to a live engineering project | Start with the [`Semantic Engagement Contract`](references/semantic-engagement-contract.md) |

## A five-minute tour

You can read both PDFs without installing anything. To search the fixed book sources by topic, run this
from the repository root:

```bash
python3 scripts/search_ontology_sources.py --scope book \
  "对象身份 identity evidence authority"
```

The results point to a volume, chapter, and repository-local source anchor, so you can continue into the
relevant TeX, Markdown, chapter guide, or PDF.

<details>
<summary>Try source-locked Semantica discovery</summary>

Preflight the environment, install and audit the pinned runtime, then discover packages without changing
any registry state:

```bash
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash runtime/setup_runtime.sh --doctor
runtime/.venv/bin/python scripts/semantic_engagement.py discover
```

Missing files, version drift, or hash mismatches fail closed. The workflow never silently falls back to a
different RDF/OWL backend.

</details>

## The relationship in one picture

```text
The books: how to observe, question, and model ─────┐
Project evidence: what actually happened ──────────┼─→ one semantic engagement
Authorized people: what may be accepted or shared ─┘             │
                                                                 ▼
                                              Semantica: sole executable semantics
                                                                 │
                                                                 ▼
                                      engineering result + semantic result + learning verdict
```

These roles are intentionally non-interchangeable. Books are not project evidence. Semantica does not
accept risk on a person's behalf. A project record does not automatically become an industry rule. Human
approval does not replace a reproducible semantic check.

## Go deeper

Implementation and governance material is collapsed by default. Start with the books; open these notes
when you are ready to connect a project or maintain the repository.

<details>
<summary><strong>The fast and slow loops behind each engagement</strong></summary>

### The fast inner loop

The fast inner loop runs for each engineering task:

```text
task and project binding
  → choose a method lens from the books
  → discover existing semantics and capabilities
  → align objects, identity, evidence, and authority
  → run applicable checks and authorized engineering work
  → return the engineering result, Semantica result, and learning verdict
```

Default engagement does not mean default ontology mutation. When the work teaches nothing reusable, the
correct verdict is `no_delta`. Only stable, sourced, reusable knowledge enters the slower governance loop:

```text
candidate → proposed → committed → regression_passed
          → release_complete → promoted → published
```

The states cannot be skipped. A `candidate`, a `committed` version, and even technical
`release_complete` do not mean public release. `published` always remains an external, authorized
decision. See the [`domain-ontology-loop`](skills/domain-ontology-loop/SKILL.md) for the full outer loop.

</details>

<details>
<summary><strong>Book sources, TeX, and PDFs</strong></summary>

The two PDFs are formal build artifacts, but neither is the sole source of the book:

- Volume 1 is maintained through its volume and chapter guides, authored XeLaTeX, figures, and authoring
  tools. Fragments generated from Semantica are controlled publication snapshots, not a second semantic
  implementation to edit by hand.
- Volume 2 takes its content from the preface, twenty `chapter.md` files, four Markdown appendices, and
  TeX assembly sources. Its fragments are produced deterministically.
- Authoring locks record the exact sources and assets consumed by a build. A PDF never replaces its
  Markdown, TeX, figures, or locks.

The build entry points are the
[`Volume 1 handbook`](references/ontology-engineering-book/handbook/README.md) and
[`Volume 2 handbook`](references/product-trustworthiness-book/handbook/README.md). For changes spanning
book text, Semantica, and PDFs, follow the
[`two-book authoring and convergence workflow`](references/book-authoring-workflow.md).

</details>

<a id="technical-governance"></a>
<details>
<summary><strong>Technical and governance notes</strong>: source lock, 29 chapter packages, and the candidate-only boundary</summary>

The statements below describe the bytes pinned today; they are not promises about another branch or an
authorization to publish:

| Area | Current, verifiable state |
|---|---|
| Semantica runtime | [`0.6.5+oe.3`](runtime/semantica-source-lock.json), pinned to an exact source commit and wheel SHA-256. Doctor also verifies every package file against the wheel `RECORD` and checks the real import root |
| Executable semantics | Ontologies, CQs, SHACL, queries, supported rules, cases, contracts, PROV, receipts, and lifecycle state have one executable home: Semantica. OE carries no second backend, fallback, or parallel registry |
| Chapter packages | 29 total: 9 for Volume 1 and 20 for Volume 2. Volume 1 Chapter 6 is `absent`; the other 28 are `partial`; all 29 have `release_status=blocked` |
| Normative-derived package | A separate `semantica.chapter_packages.vol2.normative` domain package is also `partial/blocked`. It is neither a copy of ISO text nor a compliance opinion |
| Two-book artifact v1 | It can only be a technical `candidate`. Rights and publication records accept only `pending` or `blocked`; unsigned JSON, green tests, or package receipts cannot authorize public release |

Canonical contracts and status pages:

- [`Semantic Engagement Contract`](references/semantic-engagement-contract.md): task binding, evidence,
  authority, the three-part result, and failure semantics;
- [`Semantica source lock`](runtime/semantica-source-lock.json): the current commit, version, wheel, and
  verification baseline;
- [`Two-book artifact v1 evidence contract`](references/release-evidence/README.md): the candidate-only
  technical closure;
- [`Public release status`](docs/PUBLIC-RELEASE-STATUS.md): the repository-wide status is currently
  `BLOCKED`;
- [`Privacy, sources, and public release`](docs/PRIVACY-AND-RIGHTS.md): the default-deny and allowlist
  boundary;
- [`Adding a book`](docs/ADDING-A-BOOK.md): the controlled path from a lawfully accessed standard to a
  readable book and Semantica package.

### Minimum maintainer gates

```bash
runtime/.venv/bin/python scripts/check_semantica_backend_policy.py \
  --root . --policy runtime/semantica-backend-policy.json --mode strict --json
runtime/.venv/bin/python -m pytest -q tests
```

### Repository map

```text
SKILL.md                         default semantic engagement and routing
ontology_engineering/            source-locked Semantica adapter
runtime/                         wheel/source lock, setup, and doctor
references/ontology-...-book/    Volume 1 sources, TeX, figures, and PDF
references/product-...-book/     Volume 2 sources, TeX, figures, and PDF
references/                      source maps, contracts, and release evidence
skills/domain-ontology-loop/     governed industry-ontology outer loop
skills/standard-to-book/         controlled standard-to-book workflow
scripts/                         search, semantic engagement, and gates
tests/                           contract and regression tests
```

</details>

> The books teach us how to look. Project evidence tells us what happened. Semantica makes the meaning
> executable and durable. Authorized people decide what may be accepted, promoted, and published.
