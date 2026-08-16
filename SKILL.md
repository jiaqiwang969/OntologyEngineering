---
name: ontology-engineering
description: Use when Codex needs source-grounded answers from the bundled two-volume book set — Vol.1《工程本体论》(ontology, 本体论, OWL/RDF/RDFS/SPARQL/SHACL/SWRL, description logic, methodology, OntoClean, competency questions, knowledge graphs, ontology-guided LLM/agents, manufacturing ontologies) and Vol.2《产品可信工程》(功能安全, ISO 26262, HARA, ASIL, S/E/C, SPFM/LFM/PMHF, ASIL decomposition, DFA, safety case, product trustworthiness, claims/evidence/identity/governance/change/dependency/field/assurance ontologies, 把工程规范本体化的完整示范) — rather than answering from abstract memory. CauchyX PDE Agent and CAD Agent may be referenced as optional local concrete engineering cases when available.
---

# Ontology Engineering（两卷一体）

Use this skill to answer ontology and engineering-ontology questions from the
bundled books first, then add general reasoning only when clearly marked as such.

本 skill 打包两卷一体的书系：

- **第一卷《工程本体论》**（理论卷，references/ontology-engineering-book/）：
  本体论基础、方法论、语言、推理、知识图谱、本体×LLM。
- **第二卷《产品可信工程》**（实战卷，references/product-trustworthiness-book/）：
  第一卷的实战续篇。前十章按 ISO 26262 生命周期讲透 AI 之前的传统功能安全
  最佳实践；后十章把同一套工程逐章本体化（主张/身份/治理/情境危害/需求追溯/
  测量证据/版本变化/依赖独立/制造现场/发布保证十个独立本体），是
  "如何把一部工程规范本体化"的完整示范。卷内地图见
  `references/product-trustworthiness-source-map.md`（含人物与事故索引、使用纪律）。

**Routing**：概念/方法/语言/推理问题 → 第一卷；功能安全、ISO 26262、HARA、
ASIL、度量、安全案例、产品可信、"规范如何本体化" → 第二卷；两卷各章一一
呼应处（如第一卷 ch03 方法论 ↔ 第二卷后十章的最小概念化）可对照引用。
第二卷案例（EPS-RC17/ENV-01/全部人物事故）均为合成教学材料，不得作为真实
产品结论引用；精确 ISO 条款坐标在工程正本仓库 `/Users/jqwang/143-工程规范`
的来源账中，不要凭记忆报条款号。

## Source Grounding Rule

Before giving a substantive answer, locate relevant local evidence.

1. Read `references/source-map.md`（第一卷）or `references/product-trustworthiness-source-map.md`（第二卷）when you need the chapter map or example map.
2. Search the local sources with:

   ```bash
   python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py "<query>"
   ```

3. Read the most relevant files/line ranges returned by the script.
4. Answer with concrete source anchors such as chapter names, file paths, and
   example files. Do not quote long passages.
5. If book evidence is thin or absent, say that the bundled book did
   not cover the point and separate any general ontology knowledge from the
   source-grounded answer.

This skill is intended to prevent empty ontology talk. Prefer "the book's Ch04
OWL examples model X this way" over generic definitions when a local source
exists.

## Source Roots

Bundled source roots:

- `~/.codex/skills/ontology-engineering/references/ontology-engineering-book`（第一卷）
- `~/.codex/skills/ontology-engineering/references/product-trustworthiness-book`（第二卷，
  含 321 页成书 PDF 与全书图谱计划）

Optional external example root:

- If a user separately has `cauchyx-ai`, set `ONTOLOGY_ENGINEERING_ROOT` to the
  workspace that contains it and run `--scope pde`.
- This distributable package intentionally does not bundle `cauchyx-ai`.

Optional concrete CAD Agent case:

- Set `CAD_AGENT_ROOT` to a local `cad-agent` checkout and run `--scope cad`.
- The expected local case is
  `/Users/jqwang/120-agent-cad/01-fusion-tutorial/cad-agent` when available.
- This distributable package intentionally does not bundle CAD artifacts or
  video evidence.

## Answer Workflow

For concept questions:

1. Search the concept and likely synonyms in Chinese and English.
2. Prefer the book chapters first.
3. Use CauchyX PDE Agent only as an optional external applied example when it
   is locally available.
4. Explain the idea in plain Chinese when the user asks in Chinese.

For engineering design questions:

1. Search methodology, language/tooling, reasoning, validation, and application
   chapters.
2. Turn the answer into an implementable artifact: classes/properties, competency
   questions, validation constraints, SPARQL checks, or agent routing rules.
3. Mention which chapter or example supports each design choice.

For ontology-guided agent or LLM questions:

1. Search Ch08 first.
2. If available, search an external CauchyX PDE Agent for concrete
   implementation patterns:
   `ontology/pde_core.ttl`, `sparql/*.rq`, `shapes/*.shacl`,
   `src/ontology_router.py`, and `test_ontology.py`.
3. Explain the control loop as: natural-language input -> ontology normalization
   -> consistency/constraint checks -> solver/tool routing -> provenance report.

For CAD-agent ontology questions or video-driven Agent evolution:

1. Search Ch03, Ch07, and Ch08 for competency questions, SHACL quality gates,
   and ontology-guided agent patterns.
2. Search the local CAD case with `--scope cad`, especially
   `ontology/agent.ttl`, `shapes/cad-agent.shacl.ttl`, `sparql/cq11-*.rq` through
   `cq14-*.rq`, `src/cad_agent/ontology_router.py`,
   `src/cad_agent/evolution.py`, and `docs/video-driven-self-evolution.md`.
3. Keep the Agent orchestrator, read-only semantic MCP, and privileged Fusion
   executor as separate control surfaces. Ontology conformance never grants CAD
   mutation authority.
4. Treat video as candidate evidence. Agent evolution requires actual Fusion
   reproduction, deterministic checks, positive/negative/ambiguity/prior-release
   regressions, independent review, and explicit controlled-application
   authorization.

For actual PDE solving, simulation, routing validation, or PhysicsNeMo/CUDA
execution, use the separate `$cauchyx-pde` skill when installed after using
this skill for the ontology framing.

## SkillOpt-Style Optimization Gate

Treat this skill document as trainable state: make small bounded edits, then
validate them before keeping them.

Use the local gate after any non-trivial edit to this skill, source map, or
search script:

```bash
python3 ~/.codex/skills/ontology-engineering/scripts/eval_ontology_skill.py
python3 ~/.codex/skills/ontology-engineering/scripts/eval_ontology_skill.py --split test
python3 ~/.codex/skills/ontology-engineering/scripts/eval_ontology_skill.py --split cad
```

Use `valid` cases while iterating and `test` cases before delivery. Accept an
edit only if both gates pass. If a case fails, inspect the missed query and fix
the retrieval workflow, source map, or search keywords instead of adding broad
ontology prose to `SKILL.md`.

## Useful Commands

Search broad local sources:

```bash
python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py "能力问题 competency question"
```

Search only the book:

```bash
python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py --scope book "OntoClean 刚性 统一性"
```

Search an optional external CauchyX/PDE Agent:

```bash
ONTOLOGY_ENGINEERING_ROOT=/path/to/workspace python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py --scope pde "SHACL solver routing provenance"
```

Search the concrete CAD Agent case:

```bash
CAD_AGENT_ROOT=/path/to/cad-agent python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py --scope cad "Fusion evolution SHACL regression authorization"
```

Return machine-readable results:

```bash
python3 ~/.codex/skills/ontology-engineering/scripts/search_ontology_sources.py --json "GraphRAG Text2SPARQL"
```

Run the local validation gate:

```bash
python3 ~/.codex/skills/ontology-engineering/scripts/eval_ontology_skill.py
python3 ~/.codex/skills/ontology-engineering/scripts/eval_ontology_skill.py --split test
python3 ~/.codex/skills/ontology-engineering/scripts/eval_ontology_skill.py --split cad
```

## Response Shape

Keep responses direct:

- Start with the operational answer.
- Then identify the local evidence used.
- Then provide the concept, design, or example.
- For implementation tasks, include the next concrete artifact or command.

When source grounding matters, include a short "依据" line with local file paths.
