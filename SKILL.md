---
name: ontology-engineering
description: Use when Codex needs source-grounded answers from the bundled two-volume book set — Vol.1《工程本体论》(ontology, 本体论, OWL/RDF/RDFS/SPARQL/SHACL/SWRL, description logic, methodology, OntoClean, competency questions, knowledge graphs, ontology-guided LLM/agents, manufacturing ontologies) and Vol.2《产品可信工程》(功能安全, ISO 26262, HARA, ASIL, S/E/C, SPFM/LFM/PMHF, ASIL decomposition, DFA, safety case, product trustworthiness, claims/evidence/identity/governance/change/dependency/field/assurance ontologies, 把工程规范本体化的完整示范) — rather than answering from abstract memory. CauchyX PDE Agent and CAD Agent may be referenced as optional local concrete engineering cases when available.
---

# Ontology Engineering（两卷一体）

Use this skill to answer from the bundled books first and to corroborate executable
claims through Semantica's built-in packages.

本 skill 的基本边界是：**石头只有两卷书，水是 Semantica**。

- 第一卷《工程本体论》位于 `references/ontology-engineering-book/`，提供理论、
  方法、语言、推理、知识图谱与 ontology-guided LLM/Agent 的书源。
- 第二卷《产品可信工程》位于 `references/product-trustworthiness-book/`，以前十章
  功能安全工程和后十章规范本体化示范给出完整案例。
- 两卷共 29 章对应 Semantica 的 29 个 built-in chapter packages；规范转述层对应
  `semantica.chapter_packages.vol2.normative` domain package。
- 本体、CQ、SHACL、SPARQL、案例、规则、合同、版本、PROV 和执行 receipt 的唯一
  可执行正本都在 Semantica。ontology-engineering 只保留书、来源地图、检索和教学薄入口。

不存在第二套本体实现或平行资产目录；Semantica 运行失败时直接阻断。

## Routing

- 概念、方法、语言、推理、知识图谱、ontology × LLM/Agent：先查第一卷。
- 功能安全、ISO 26262、HARA、ASIL、硬件度量、安全案例、产品可信：先查第二卷。
- “怎样把一部工程规范本体化”：对照第二卷后十章与 Semantica 的 Vol.2 packages。
- 行业本体持续内化：再使用 `skills/domain-ontology-loop/SKILL.md`。
- 把另一部标准做成书：再使用 `skills/standard-to-book/SKILL.md`。

第二卷的 EPS-RC17、ENV-01、人物、事故和数值都是合成教学材料，不能作为真实
产品结论。精确 ISO 条款坐标只可回到用户合法持有、由
`ONTOLOGY_ENGINEERING_AUTHORING_ROOT` 指向的受控来源账核对；不要凭记忆报条款号，
也不要把书中转述冒充标准原文。

## Source Grounding Rule

Before giving a substantive answer, locate the relevant book evidence.

1. Read `references/source-map.md` for Vol.1 or
   `references/product-trustworthiness-source-map.md` for Vol.2.
2. Search the two book roots:

   ```bash
   python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py \
     --scope book "<query>"
   ```

3. Read the most relevant chapter `README.md`, `chapter.md`, handbook source, glossary,
   proposition index, or book PDF page when layout matters.
4. Answer with concrete book anchors. Do not quote long passages.
5. If the book evidence is thin or absent, say so and label any added general knowledge.
6. If a claim is executable, run the corresponding Semantica package and report its
   package ID, scenario, oracle result and receipt/release status separately from the book anchor.

Book prose is source evidence. A Semantica execution is corroborating evidence; it does not
turn a synthetic case into a real product fact or grant compliance/release authority.

## Source Roots

The only bundled primary source roots are:

- `~/.codex/skills/ontology-engineering/references/ontology-engineering-book`
- `~/.codex/skills/ontology-engineering/references/product-trustworthiness-book`

Do not expect an OE-local ontology, fixture, query, shape, CQ or normative-package directory.
Those are resolved from Semantica's allowlisted package registry. The exact locally built
Semantica source and wheel are pinned by `runtime/semantica-source-lock.json`; do not replace
that lock with a moving branch name or an unverified installed version.

Optional applied cases are not book evidence:

- With an authorized local CauchyX checkout, set `ONTOLOGY_ENGINEERING_ROOT` and use
  `--scope pde`.
- With an authorized local CAD Agent checkout, set `CAD_AGENT_ROOT` and use `--scope cad`.

## Answer Workflow

For concept questions:

1. Search Chinese and English synonyms.
2. Prefer the relevant book chapter and glossary.
3. Explain plainly, then identify assumptions and open/closed-world boundaries.
4. Use an applied case only when clearly labelled as external.

For engineering design questions:

1. Search methodology, modeling, reasoning, validation and application chapters.
2. Turn the answer into implementable semantics: classes/properties, CQs, constraints,
   named queries, cases, rules or routing contracts.
3. Put executable semantics in a Semantica package, never beside the book as a second truth source.
4. Distinguish semantic authority, fact authority and decision authority.

For ontology-guided Agent or LLM questions:

1. Search Vol.1 Ch08, plus Ch03 for scope/CQs and Ch07 for validation.
2. Explain the control loop as natural-language input → normalization → semantic checks →
   tool routing → provenance/receipt.
3. Keep the Agent orchestrator, read-only semantic interface and privileged executor as
   separate control surfaces. Ontology conformance never grants mutation authority.

For CAD/video-driven evolution, treat video as candidate evidence. Require actual reproduction,
deterministic checks, positive/negative/ambiguity/prior-release regressions, review and explicit
controlled-application authorization before accepting a lesson delta.

## Runnable Corroboration（薄入口 → Semantica）

`demos/` contains teaching launchers, not semantic implementations. A launcher selects a stable
package/scenario and delegates to Semantica's `SemanticPackageRunner`; its RDF/OWL, CQ, SPARQL,
SHACL, facts, rules, cases, exact oracle, lifecycle record and receipt remain inside Semantica.

```bash
bash runtime/setup_runtime.sh
runtime/.venv/bin/python demos/<demo>.py
```

Discover packages directly from the installed, source-locked Semantica build:

```bash
runtime/.venv/bin/semantica package list --json
runtime/.venv/bin/semantica package show \
  semantica.chapter_packages.vol2.ch14 --json
```

Python, CLI and MCP are thin adapters over the same package runner. MCP exposes the package
operations `list_chapter_packages`, `get_chapter_package`, `run_chapter_package` and
`verify_chapter_package`; it must not introduce another execution path.

Read `demos/README.md` for the launcher-to-package map. A scenario can pass its declared oracle
while release verification remains blocked by an unsupported capability or missing evidence;
report both statuses. Never turn `blocked`, `partial`, `placeholder`, `absent` or an unsupported
reasoning profile into green.

Semantica's supported runtime surface includes lossless RDF Dataset handling, SPARQL query/update,
SHACL validation, bounded positive monotonic forward rules, snapshots/diffs, PROV and release
receipts. Full DL/tableau reasoning, general SWRL built-ins, non-monotonic/default, temporal and
probabilistic reasoning are not implied; requests outside declared capabilities fail closed.

## Domain Evolution and New Books

Use `skills/domain-ontology-loop/` when practice should evolve a domain ontology. Its OE script is
a thin entry to Semantica's governed lifecycle: baseline snapshot → delta → reasoned conflict
verdict → version/PROV commit → old-CQ regression.

Use `skills/standard-to-book/` for a new standard/book. The new readable book and source map belong
here; its executable package must be registered and released in Semantica. A package-local copy in
OE is not an acceptable substitute.

## Zero-Exception Gate

The final architecture is enforced, not advisory:

- OE executable code may import Semantica only through its designated thin bootstrap.
- OE must not import or invoke RDFLib, pySHACL, PyOxigraph, owlready2, Jena, dynamic backends or
  subprocess bypasses.
- OE must not retain executable `.ttl`, `.owl`, `.rq`, `.sparql`, SHACL, rule or fixture copies.
- Runtime creation has one authorized Semantica profile and no fallback.
- Unknown packages/scenarios, missing assets, hash mismatch and unsupported capabilities block.
- Registry/package/source/wheel/input/output hashes and PROV receipts bind every executable claim.

Run the repository gate after changes; any finding or allowlist exception means the migration is
not complete.

## Skill Evaluation

After changing this skill, either source map, or retrieval behavior:

```bash
python3 ~/.codex/skills/ontology-engineering/scripts/eval_ontology_skill.py
python3 ~/.codex/skills/ontology-engineering/scripts/eval_ontology_skill.py --split test
```

Use the valid split while iterating and the held-out test split before delivery. Fix missed source
anchors or retrieval terms; do not hide misses by adding broad generic prose. Optional CAD/PDE
splits validate external cases only and are not portable book gates.

## Useful Commands

```bash
# Bilingual book search
python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py \
  --scope book "能力问题 competency question"

# Machine-readable search results
python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py \
  --scope book --json "GraphRAG Text2SPARQL"

# Optional applied cases
ONTOLOGY_ENGINEERING_ROOT=/path/to/workspace \
  python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py \
  --scope pde "solver routing provenance"
CAD_AGENT_ROOT=/path/to/cad-agent \
  python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py \
  --scope cad "Fusion evolution SHACL regression authorization"
```

## Response Shape

- Start with the operational answer.
- Give a short `依据` line with book path/chapter anchors.
- Separate book explanation, Semantica execution result and any general inference.
- For implementation tasks, name the Semantica package/API and the next concrete artifact.
- State unsupported capability, synthetic-data boundary and decision authority explicitly.
