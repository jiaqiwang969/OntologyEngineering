[简体中文](README.md) · **English**

# Ontology Engineering: Make Engineering Knowledge Readable, Searchable, and Verifiable

Start with the [ongoing manufacturing collaboration guide (Chinese)](docs/MANUFACTURING-COLLABORATION.md).
The assistant follows discussions and field feedback about trials, process routes, equipment,
suppliers and production problems. It builds private project ontologies and revises solutions as
evidence develops. Project ontologies and business records stay in the skill’s private `var/projects/` directory;
employees communicate in ordinary language rather than preparing ontology or JSON records.

Say “use ontology-engineering” with the current goal, objects, evidence and constraints. The assistant continues the engineering decision, selects the needed internal and external capabilities, and organizes work by its dependencies. Jev assists with contextual routing; the assistant reviews those candidates before coordinating CAD, manufacturing, evidence methods and Semantica. See [usage](docs/USAGE.md) and [context routing](references/context-routing.md).

**Version 0.7.0** brings textbooks and MISUMI knowledge, source-bound context and original-page viewing into the existing engineering-ontology method. It also unifies software installation and source delivery. Functions, objects, claims, evidence and iterative improvement remain the organizing principles. See the [release notes](docs/releases/0.7.0.md) and [distribution guide](docs/PORTABLE-DISTRIBUTION.md).

A task may begin with an unfinished idea or research question. Preserve the original intent, compare candidates by required functions and mechanisms, and use evidence gaps to plan validation, resources, budgets and schedules. Functional modeling explains what must be achieved and for whom; first-principles reasoning derives physical constraints; process modeling describes manufacturing activities and state changes. Together these connect function, structure, tolerances, processes, inspection and cost as the design evolves.

<p align="center">
  <a href="https://drive.google.com/file/d/1yQAW6EGeBqpxFDHKXXk61odSzjU-7zXD/view">
    <img src="https://raw.githubusercontent.com/jiaqiwang969/OntologyEngineering/manufacturing-v0.7.0/docs/assets/engineering-ontology-cover.jpg" width="320" alt="Cover of Engineering Ontology, Volume 1">
  </a>
</p>

<p align="center">
  Two books provide the theory; manufacturing methods connect customer conversations, process plans, and costs; Semantica executes the semantics.
</p>

<p align="center">
  <a href="https://drive.google.com/file/d/1yQAW6EGeBqpxFDHKXXk61odSzjU-7zXD/view">Read Volume 1</a> ·
  <a href="https://drive.google.com/file/d/1V5VXL20CBZO1giI7hJLcOMvAUXkw5i0e/view">Read Volume 2</a> ·
  <a href="#manufacturing">Manufacturing and costs</a> ·
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

The bundled [manufacturing process and cost module](skills/manufacturing-process-cost/SKILL.md) applies
this method to discrete mechanical manufacturing. Start with incomplete inquiries, drawings, process
notes, equipment inventories, and quotations; uncover the conditions behind them, develop an evidenced
proposal, and use customer and shop-floor feedback to revise it.

The bundled [CAD Agent module](skills/cad-agent/SKILL.md) follows a [general CAD evidence workflow](skills/cad-agent/references/cad-engineering-workflow.md)
for parts, drawings/BOMs, assemblies, mechanisms, and native readbacks. Its [bidirectional handoff](skills/cad-agent/references/cad-process-integration.md)
connects relevant CAD objects and revisions to manufacturing decisions. The portable core uses direct NXOpen/Journal with MISUMI China supplier resources; recipients provide their own CAD software, license and hosts. Fusion is retired and CAD does not default to MCP. The process module then reviews required functions, failure paths, inspection, cost, and lead time; Semantica remains the formal semantic authority.

## Two volumes, two complementary questions

| Volume | The question it answers | What you gain |
|---|---|---|
| [Volume 1, *Engineering Ontology* (`工程本体论`)](https://drive.google.com/file/d/1yQAW6EGeBqpxFDHKXXk61odSzjU-7zXD/view) | How do we turn ambiguous engineering language into a conceptual system that can be tested? | A general method for objects and identity, relations, competency questions, open- and closed-world reasoning, constraints, inference, provenance, and ontology-guided agents |
| [Volume 2, *Trustworthy Product Engineering* (`产品可信工程`)](https://drive.google.com/file/d/1V5VXL20CBZO1giI7hJLcOMvAUXkw5i0e/view) | How can an engineering team explain why a product deserves trust? | An ISO 26262 ontology-engineering walkthrough and ten reusable lenses: claims, identity, governance, contextual hazards, requirements, measurement, change, dependency, field evidence, and assurance |

Volume 1 provides the reusable grammar; Volume 2 shows that grammar at work in difficult product
decisions. The people, incidents, EPS-RC17, ENV-01, and numerical values in Volume 2 are synthetic
teaching material. Exact ISO clauses, tables, and wording must be checked against a lawfully held,
controlled source. This project is not an official interpretation, certification, or conclusion about a
real product.

## Shared methods and domain knowledge: five engineering ontology patterns

Volume 1 provides objects, relations, competency questions, constraints and reasoning boundaries. Volume 2 shows how claims, context, evidence and change guide an engineering decision. CAD, manufacturing and cost modules specialize those common structures; a project supplies actual parts, drawings, operations, quotations and tests. New source knowledge enters through the same method.

| Shared pattern | What it connects in engineering practice |
|---|---|
| Identity and version | Part definitions, assembly instances, complete supplier configurations, model revisions and cost subjects, with checks that they refer to the same object and state |
| Claims and evidence | Source statements, adopted project support, measurements and model suggestions, keeping each conclusion traceable |
| Conditions and scope | Loads, supports, environments, batch sizes, equipment states and source assumptions; missing observations remain unknown |
| Requirements, realization and verification | Required functions, design or process implementations and applicable verification; creating geometry alone does not establish function |
| Change and dependency | Reassessment of designs, inspection, cost and lead time when requirements, dimensions, materials, processes or quotations change |

The patterns support both engineering tasks and new domain ontology construction: pose a competency question, find a reusable structure, specialize the domain differences, and check conditions and counterexamples. The [pattern source map](references/judgment-pattern-source-map.json) records their sources and current check coverage.

**Jev, the agent and Semantica work within this shared method.** Jev helps select capabilities, topics, chapters, passages and follow-up checks; it can also suggest pattern matches and concept differences. The agent reads original pages, proposes representations and coordinates specialized engineering tools. Semantica executes applicable formal semantic checks and retains traceable relationships and evidence. CAD and manufacturing return to the same knowledge entry point when functions, configurations or conditions change. Independent retrieval steps can run in parallel according to their dependencies.

The four mechanical textbooks add machine-element design, mechanism motion, precision and measurement, and design for manufacture and assembly. MISUMI contributes product structures, specifications, configurations and installation conditions. General principles, supplier models, CAD representations and physical project objects retain distinct identities and are related under stated conditions. Chapter navigation locates material; original text, formulas, tables and page images support close reading. Applicable conditions, calculations and project evidence determine whether a result can be used. See [engineering interpretation of textbook and supplier knowledge](references/supplier-knowledge-interpretation.md).

## Who this is for

- Shop-floor employees interpreting work orders, following current instructions, checking results
  and handing over unfinished work;
- Manufacturing business owners deciding whether they can make a new customer's product, how to plan
  production, and what costs to include;
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
| Develop an engineering idea or unfinished research into candidates and a validation plan | Start at the [root skill](SKILL.md) and [decision, evidence and activity methods](skills/engineering-evidence-methods/references/method-internalization.md); retain revisions, conditions and open questions |
| Follow trials, processes, suppliers and field feedback | Use the [manufacturing collaboration guide](docs/MANUFACTURING-COLLABORATION.md) to build project knowledge and revise proposals |
| Learn ontology engineering from first principles | Volume 1, Chapters 1–3: why ontology matters, core concepts, and how to begin with competency questions |
| Work on RDF/OWL, constraints, or reasoning | Volume 1, Chapters 4–5 and 7, then corroborate the examples with their Semantica chapter packages |
| Understand semantics for LLMs and agents | Volume 1, Chapter 8, followed by the engagement rules in [`SKILL.md`](SKILL.md) |
| Build a trustworthy-product or functional-safety evidence chain | The preface and Chapters 1–10 of Volume 2, then the paired ontology answers in Chapters 11–20 |
| Apply the method to a live engineering project | Start with the [`Semantic Engagement Contract`](references/semantic-engagement-contract.md) |
| Design or verify parts, drawings, assemblies, mechanisms, and manufacturing functions | Use the [CAD Agent module](skills/cad-agent/SKILL.md) and [general CAD evidence workflow](skills/cad-agent/references/cad-engineering-workflow.md) for identity, native provenance, relations, claims, and change impact; add the [manufacturing handoff](skills/cad-agent/references/cad-process-integration.md) when needed |
| Infer shape or mechanisms from images, video, patents, or incomplete CAD | Use [evidence-driven reconstruction](skills/cad-agent/references/evidence-driven-reconstruction.md) to separate visible evidence, mechanism hypotheses, conditional physical or mathematical derivations, and a candidate CAD design |
| Turn a customer inquiry into a process and cost proposal, then revise it from feedback | Start with the [manufacturing module](skills/manufacturing-process-cost/SKILL.md): anonymized cases, work norms, record templates, and XeLaTeX report components |

<a id="manufacturing"></a>

## Manufacturing and costs: connect conversations, proposals, and feedback

The current skill version is **0.7.0**, for discrete mechanical manufacturing. Each project supplies its own
industry context, materials, process parameters, and acceptance criteria. The reusable part is how to
find missing knowledge, connect evidence, compare options, and verify decisions. Detailed guides and
example outputs are currently in Chinese.

```mermaid
flowchart LR
    A["Customer inquiry and relevant source documents"] --> B["Context differences and tacit conditions"]
    B --> C["Process, inspection, and resource options"]
    C --> D["Costs on a common basis"]
    D --> E["Customer and shop-floor feedback"]
    E --> B
    E --> F["Review, anonymize, validate, and adopt"]
    F --> A
```

| The decision at hand | Methods and reusable resources |
|---|---|
| Which documents or answers can change the decision? | [Source documents and conversation filtering](skills/manufacturing-process-cost/references/high-value-documents.md): retain provenance, conditions, conflicts, and counterevidence; ask questions tied to the current decision |
| Can an existing process transfer, and what drives precision or equipment requirements? | [Requirements, processes, and costs](skills/manufacturing-process-cost/references/requirements-process-cost.md) and [STEP to process planning](skills/manufacturing-process-cost/references/step-to-process.md): connect critical features, intermediate states, workholding, inspection, and equipment functions |
| Should we make, outsource, reuse equipment, or develop a machine? | [Estimate bases and batch charges](skills/manufacturing-process-cost/references/estimate-basis.md) and [twelve decision patterns](skills/manufacturing-process-cost/references/decision-patterns.md): compare equivalent quantities and scopes, distinguish quotes from estimates and measurements, and choose decision-relevant validation |
| How can the proposal expose its supporting evidence? | [XeLaTeX report components](skills/manufacturing-process-cost/references/report-design.md), [five editable process-diagram templates](skills/manufacturing-process-cost/references/process-flow-templates.md), and a [diagram preview](skills/manufacturing-process-cost/assets/report-template/flowcharts/preview.pdf) |
| What needs rechecking after a correction or revision? | [Evolving case](skills/manufacturing-process-cost/references/case-evolution.md), [work norms](skills/manufacturing-process-cost/references/work-norms.md), and [organizational follow-through](skills/manufacturing-process-cost/references/organization-loop.md): record reasons, trace dependencies, and connect responsibilities to the next round |

The module includes [eight record templates](skills/manufacturing-process-cost/SKILL.md#按需要取用),
[six optional review cards](skills/manufacturing-process-cost/assets/templates/decision-review-cards.md),
and [four rounds of report inputs plus five later decision exercises](skills/manufacturing-process-cost/assets/report-template/example/README.md).
The cases show information arriving over time, conclusions changing, and questions reopening. Report
views follow the current decision; historical A/B/C labels are not fixed manufacturing stages.

STEP-to-process support currently consists of analysis and review methods. The report component takes
prepared records and generates native TeX, with optional PDF compilation. Process diagrams are edited
against project records; real capability and savings require field evidence. Generic material preserves
causal relationships and counterexamples. Customer originals, identities, actual quotations, models,
and conversations stay in the project's private workspace.

## Install the code

Download the [core ZIP](https://github.com/jiaqiwang969/OntologyEngineering/releases/download/manufacturing-v0.7.0/ontology-engineering-core-v0.7.0.zip) from the [0.7.0 release](https://github.com/jiaqiwang969/OntologyEngineering/releases/tag/manufacturing-v0.7.0), or install the matching Git tag:

```bash
mkdir -p "$HOME/.codex/skills"
git clone --branch manufacturing-v0.7.0 --depth 1 \
  https://github.com/jiaqiwang969/OntologyEngineering.git \
  "$HOME/.codex/skills/ontology-engineering"
cd "$HOME/.codex/skills/ontology-engineering"
```

For a ZIP installation, place the complete `ontology-engineering/` directory at that location. Preserve its internal directories; copying only `SKILL.md` is insufficient. When updating an existing installation, preserve your `sources/` and `var/` first and follow the version's update instructions rather than extracting over private data indiscriminately.

Use Python 3.11+ with SQLite FTS5. Queries use the standard library. Registering textbooks also requires Poppler's `pdfinfo` and `pdftotext`; rendering original pages requires `pdftoppm`. Querying an installed MISUMI index does not require PyMuPDF; rebuilding that index from PDF sources does.

## Download and register sources

The [source delivery manifest](runtime/source-delivery.json) holds versions, Drive links, file sizes, hashes and book navigation. Inspect the files and their destinations without making a network request:

```bash
python3 scripts/source_library.py list
```

Sources are delivered separately through the [download / request access link](https://drive.google.com/drive/folders/1QoNTu0gy9aPwQ-Sk6y5IWSBcVUCDVdXK). Downloading currently requires the appropriate Drive access; use the same link to request access if needed, then download after approval. The delivery manifest provides file identity checks; this software release does not make the source documents public or change their copyright and redistribution terms.

After a browser download, extract the outer ZIP created by Drive while keeping the inner `tar.xz` files compressed. Replace the example path with your downloaded directory:

```bash
python3 scripts/source_library.py import --from "/path/to/unzipped-drive-folder"
python3 scripts/source_library.py verify
python3 scripts/source_library.py register
python3 runtime/misumi/setup.py install \
  --from sources/misumi --data-root sources/.indexes/misumi
python3 runtime/misumi/setup.py verify --data-root sources/.indexes/misumi
python3 scripts/jev_knowledge.py --status --sources all --json
```

Import checks each file's size and SHA, retains the downloaded originals, and refuses to overwrite differing bytes. `register` defaults to all six books in the manifest; use `--select` for a partial installation. Rebuilding an existing book index requires `--replace-index`: it replaces the selected collection, so select every book you intend to retain. MISUMI `install` requires a new destination; run `verify` for an existing installation.

With gws installed and authenticated, you can explicitly fetch a selected group:

```bash
python3 scripts/source_library.py fetch --select books
python3 scripts/source_library.py fetch --select misumi
```

gws uses an authenticated session with the required access and does not change permissions. Browser download followed by import is also supported. See the [knowledge guide](references/supplier-knowledge.md) for registering other PDFs you own or may use.

## Try a local query

Once the corresponding sources are registered:

```bash
python3 scripts/jev_knowledge.py "轴承配合与装配条件" --sources all --local --render 1
python3 scripts/jev_knowledge.py --view-page local_pdf:dfma:page:300
```

The first example searches for bearing fits and assembly conditions; Chinese queries are useful for the Chinese supplier catalogs. `--local` makes no Jev request and returns retrieval candidates without approving applicability. `--view-page` renders a registered page directly, independent of query ranking. Page numbers are physical PDF pages and must not be assumed to match printed labels. Scanned pages may have no usable native text; inspect the original image or extract it separately instead of treating empty text as absent knowledge.

Online queries use the same entry point without `--local`:

```bash
python3 scripts/jev_knowledge.py "比较定位销与夹紧件各自承担的功能" --sources all --json
```

Online mode sends the current query, supplied engineering context, navigation and bounded source passages to Jev. Choose content that may be sent under your project’s requirements; the workflow does not send entire PDFs by default. Ordinary distributions contain no credentials. Follow the [usage guide](docs/USAGE.md) to store your own credential in `~/.codex/api-jev.md` with mode `0600`. Keep it out of Git and project records. Without credentials, `--local` remains available.

## One local directory

```text
~/.codex/skills/ontology-engineering/
├── SKILL.md, ontology_engineering/, scripts/, skills/, runtime/
├── sources/                    Originals and indexes; excluded from Git/core ZIP
│   ├── books/                  Two method volumes and four mechanical textbooks
│   ├── misumi/                 Manifest and losslessly compressed catalog shards
│   └── .indexes/               Book/catalog indexes and restored original pages
└── var/                        Private local work; excluded from Git/core ZIP
    ├── projects/               Projects, models, evidence and adoption records
    ├── state/, cache/          Query history, judgments and page images
    └── maintenance/, builds/   Maintenance and build artifacts
```

MISUMI installation restores the index and metadata. Queries restore only the pages they need, rather than expanding every PDF. To restore both complete original catalogs explicitly:

```bash
python3 runtime/misumi/setup.py restore-books --data-root sources/.indexes/misumi
```

Page caches grow with use. Keep `sources/` and `var/` when moving the installation and follow the [installation and relocation guide](docs/PORTABLE-DISTRIBUTION.md). Runtime data does not depend on a maintainer's external workspace. System applications and account authentication are supplied by the environment.

## Enable semantics and CAD when needed

Formal semantic discovery and execution use the pinned Semantica runtime shipped with the core. Initial dependency installation may access a package index:

```bash
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash runtime/setup_runtime.sh --doctor
runtime/.venv/bin/python scripts/check_semantica_backend_policy.py --mode strict
runtime/.venv/bin/python scripts/semantic_engagement.py discover
```

Discovering packages does not perform a project review. Formal review also needs an applicable project binding and current evidence. Jev does not replace Semantica, and source statements do not automatically become project facts or formal ontology assertions.

CAD runs directly through **Siemens NX / NXOpen / Journal**, with MISUMI China preferred for purchased components. You provide the NX installation, license, NXOpen, execution host and access configuration; they are not distributed with the code. Prepare these only for tasks that need CAD:

```bash
bash skills/cad-agent/setup.sh
bash skills/cad-agent/doctor.sh --json
```

Configure execution using the [NX guide](skills/cad-agent/references/nx-execution.md). Fusion and the retired CAD MCP paths are not current backends. Conceptual queries and original-page reading do not require NX.

The optional [Jev web tool](references/jev-browser-tool.md) supports bounded tasks on actual websites and uses a separate Python 3.12+ environment. Prepare it when webpage interaction is needed:

```bash
bash runtime/jev-ultrafast/setup.sh
python3 scripts/jev_browser.py doctor
```

## A five-minute tour

After installation, run the examples from `~/.codex/skills/ontology-engineering/`.

Downloaded PDFs can be read without a semantic runtime. The full Git checkout also includes the two volumes’ authoring sources for topic search; core ZIP users can query registered PDFs using the commands above:

```bash
python3 scripts/search_ontology_sources.py --scope book \
  "对象身份 identity evidence authority"
```

The results point to a volume, chapter, and repository-local source anchor, so you can continue into the
relevant TeX, Markdown, chapter guide, or PDF.

With Python 3.11+, generate your first report's TeX and frozen input records from a synthetic example,
then verify the input-to-output correspondence:

```bash
python3 scripts/manufacturing_report.py generate \
  --input skills/manufacturing-process-cost/assets/report-template/example/03-resource-cost.json \
  --output var/projects/manufacturing-report-001
python3 scripts/manufacturing_report.py verify var/projects/manufacturing-report-001
```

Use a new output directory under the same skill’s `var/`. Add `--compile` to generate a PDF; this requires XeLaTeX,
the usual Chinese font packages, and Poppler. See the [report guide](skills/manufacturing-process-cost/references/report-design.md)
for dependencies and page-by-page review. These commands check record mappings, file hashes, and cost
arithmetic. Engineering semantics for the report's own snapshot require separate Semantica verification.

## Portable distribution

The [0.5.7 update notes](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.7.0/docs/releases/manufacturing-method-0.5.7.md) describe source-bound reconstruction and conditional first-principles derivations. The [0.5.6 notes](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.7.0/docs/releases/manufacturing-method-0.5.6.md) cover the remote CAD bridges and assembly/mechanics kernel.

| Component | Current version and scope |
|---|---|
| Engineering ontology skill | `0.7.0`; unified knowledge, source and project-context bindings, original-page references; CAD evidence, reconstruction, remote bridges, assembly/mechanics, manufacturing processes and costs |
| Record templates / review cards | `0.2.1` for two templates, `0.2.0` for the others / cards `1.4.1` |
| Reports / process diagrams | `1.0.0`; XeLaTeX and five editable TikZ templates |
| Frozen manufacturing semantic package | `0.1.1`; 141 declared assets, 68 synthetic scenarios, and 23 CQs; retains its local technical candidate state |

Keep the complete `ontology-engineering/` root. Code, methods, cases and tools install together; downloaded material belongs in `sources/`, and private work belongs in `var/`. Both directories are excluded from Git and the core ZIP. Copying a single submodule omits shared dependencies. The Git checkout also retains the two books’ authoring sources; the core ZIP provides the code and methods for everyday use. Follow [portable distribution and receiver checks](docs/PORTABLE-DISTRIBUTION.md) to
check, package, and replay it without the original customer project or author workspace. Initial Python
dependency installation may still need a package index.

Storage has separate components: the core ZIP is approximately **6.3 MB**, and the six books plus complete MISUMI source files total approximately **3.16 GB**. Runtime dependencies are additional: a measured Semantica installation on macOS arm64 occupies about **1.9 GB**, before caches and other optional environments. Installed size varies by platform; the ZIP size is not the complete installation footprint.

The [0.5.7 public asset ledger](https://github.com/jiaqiwang969/OntologyEngineering/releases/download/manufacturing-v0.5.7/ontology-engineering-core-v0.5.7-assets.json), the preceding manufacturing release's [frozen asset manifest](https://github.com/jiaqiwang969/OntologyEngineering/blob/b5b148333a39b69aaa8b5b521de8242805cc3838/docs/releases/manufacturing-method-0.5.3.json),
the historical [README update manifest](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.7.0/docs/releases/manufacturing-readme-0.5.3-r1.json), and the
[semantic package NOTICE](runtime/vendor/MANUFACTURING-NOTICE.md) retain their separate scopes.
The distribution guide explains the limits of file checks and synthetic replay. Manufacturing-method
publication is recorded separately from the [whole-book publication status](docs/PUBLIC-RELEASE-STATUS.md).

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
[`Volume 1 handbook`](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.7.0/references/ontology-engineering-book/handbook/README.md) and
[`Volume 2 handbook`](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.7.0/references/product-trustworthiness-book/handbook/README.md). For changes spanning
book text, Semantica, and PDFs, follow the
[`two-book authoring and convergence workflow`](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.7.0/references/book-authoring-workflow.md).

</details>

<a id="technical-governance"></a>
<details>
<summary><strong>Technical and governance notes</strong>: source lock, 29 chapter packages, and the candidate-only boundary</summary>

The statements below describe the bytes pinned today; they are not promises about another branch or an
authorization to publish:

| Area | Current, verifiable state |
|---|---|
| Semantica runtime | [`0.6.5+oe.7`](runtime/semantica-source-lock.json), pinned to an exact source commit and wheel SHA-256. Doctor also verifies every package file against the wheel `RECORD` and checks the real import root |
| Executable semantics | Ontologies, CQs, SHACL, queries, supported rules, cases, contracts, PROV, receipts, and lifecycle state have one executable home: Semantica. OE carries no second backend, fallback, or parallel registry |
| Chapter packages | 29 total: 9 for Volume 1 and 20 for Volume 2. Volume 1 Chapter 6 is `absent`; the other 28 are `partial`; all 29 have `release_status=blocked` |
| Normative-derived package | A separate `semantica.chapter_packages.vol2.normative` domain package is also `partial/blocked`. It is neither a copy of ISO text nor a compliance opinion |
| Frozen manufacturing cases | `semantica.manufacturing.process-cost-loop@0.1.1` travels with the root and runs through Semantica; its 68 synthetic scenarios have a separate scope from chapter packages, new project reports, and industry promotion |
| Two-book artifact v1 | It can only be a technical `candidate`. Rights and publication records accept only `pending` or `blocked`; unsigned JSON, green tests, or package receipts cannot authorize public release |

Canonical contracts and status pages:

- [`Semantic Engagement Contract`](references/semantic-engagement-contract.md): task binding, evidence,
  authority, the three-part result, and failure semantics;
- [`Semantica source lock`](runtime/semantica-source-lock.json): the current commit, version, wheel, and
  verification baseline;
- [`Two-book artifact v1 evidence contract`](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.7.0/references/release-evidence/README.md): the candidate-only
  technical closure;
- [`Whole-book publication status`](docs/PUBLIC-RELEASE-STATUS.md): currently `BLOCKED`; the authorized
  manufacturing-method upload has its own manifest above;
- [`Privacy, sources, and public release`](docs/PRIVACY-AND-RIGHTS.md): the default-deny and allowlist
  boundary;
- [`Adding a book`](https://github.com/jiaqiwang969/OntologyEngineering/blob/manufacturing-v0.7.0/docs/ADDING-A-BOOK.md): the controlled path from a lawfully accessed standard to a
  readable book and Semantica package.

### Minimum maintainer gates

```bash
runtime/.venv/bin/python scripts/check_semantica_backend_policy.py \
  --root . --policy runtime/semantica-backend-policy.json --mode strict --json
runtime/.venv/bin/python -m pytest -q tests --ignore=tests/test_jev_browser_tool.py
bash runtime/jev-ultrafast/setup.sh
runtime/jev-ultrafast/.venv/bin/python -m unittest discover -s tests -p test_jev_browser_tool.py
```

### Repository map

```text
SKILL.md                         default semantic engagement and routing
ontology_engineering/            source-locked Semantica adapter
runtime/                         wheel/source lock, setup, and doctor
runtime/vendor/                  pinned runtime and frozen manufacturing case transport
references/ontology-...-book/    Volume 1 authoring sources, TeX and figures (Git)
references/product-...-book/     Volume 2 authoring sources, TeX and figures (Git)
references/                      source maps, contracts, and release evidence
skills/domain-ontology-loop/     governed industry-ontology outer loop
skills/standard-to-book/         controlled standard-to-book workflow
skills/cad-agent/                direct NX, MISUMI acquisition, and manufacturing evidence handoff
skills/manufacturing-process-cost/ methods, anonymized cases, records, reports, and diagrams
docs/PORTABLE-DISTRIBUTION.md     whole-root distribution and receiver checks
docs/releases/                   versioned update notes and public asset manifests
sources/                         downloaded sources and indexes, excluded from Git/core ZIP
var/                             private projects, evidence and caches, excluded from Git/core ZIP
scripts/                         search, reports, case replay, distribution, and gates
tests/                           contract and regression tests
```

</details>

## Documentation and rights

- [Usage](docs/USAGE.md) · [Installation and source layout](docs/PORTABLE-DISTRIBUTION.md)
- [Engineering knowledge](references/supplier-knowledge.md) · [Manufacturing collaboration](docs/MANUFACTURING-COLLABORATION.md)
- [CAD module](skills/cad-agent/SKILL.md) · [Semantic engagement contract](references/semantic-engagement-contract.md)

Original repository code is provided under the [MIT License](LICENSE); bundled third-party components retain their own notices and licenses. The four third-party textbook PDFs, original supplier documents, private projects, credentials and runtime caches are excluded from GitHub and the core ZIP. Downloading Drive files currently requires the appropriate access; their original copyright and redistribution terms remain unchanged.

Version 0.7.0 addresses software distribution and installation. It does not claim complete ontology modeling of six books, sufficient coverage for every knowledge query, or validation of a real engineering design. Subsequent cases will test whether source knowledge changes decisions and implementations, informing further improvements.

> The books teach us how to look. Project evidence tells us what happened. Semantica makes the meaning
> executable and durable. Authorized people decide what may be accepted, promoted, and published.
